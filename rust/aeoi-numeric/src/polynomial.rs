//! fit.poly.polyval and fit.poly.roots (spec 10.33, 10.34; v0.4.20).
//!
//! Both are thin shells over the shared polynomial kernel `aeoi_core::poly` (spec 11.5 RI-1):
//! - polyval: compensated Horner (Graillat–Langlois–Louvet 2005) with a rigorous error bound per point;
//! - roots: eigenvalues of the balanced companion matrix with Newton polishing (the root finder that
//!   control.lti.poles_zeros and signal.filter.filter already use), sorted like poles_zeros, with
//!   Braess–Hadeler / Carstensen inclusion discs per root and cluster detection.
//! Author's decisions (2026-11-26): roots sorted like poles_zeros (real part ascending, then imaginary part
//! descending); roots always c128; per-root error radius plus W.ACCURACY.CLUSTERED_ROOTS for clusters;
//! polyval uses compensated Horner with a rigorous bound.
use crate::interp1::vec1;
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::lti::C64;
use aeoi_core::poly::{comp_horner, disc_components, gamma, inclusion_radii, polyval_r, roots as poly_roots};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(PolyVal, "fit.poly.polyval");
operator_struct!(Roots, "fit.poly.roots");

const U: f64 = f64::EPSILON / 2.0;
/// polyval warns where fewer than ~8 significant digits are guaranteed: y ≠ 0 and bound > REL_WARN·|y|.
pub const REL_WARN: f64 = 1e-8;

fn coeffs(args: &Value, host: &HostCtx, max_degree: usize) -> Result<Vec<f64>, OpError> {
    let (p, ps) = vec1(&args["p"], "/p", host)?;
    if ps.len() != 1 || p.is_empty() {
        return Err(OpError::new("E.VALIDATION.SHAPE", format!("p 的 shape 是 {ps:?}；要求一维 [n + 1]（系数从最高次到常数项），至少一个元素。"), "把系数写成一维数组，最高次在前，例如 x² − 3x + 2 写成 [1, −3, 2]。").path("/p/shape"));
    }
    if p.len() - 1 > max_degree {
        return Err(OpError::new("E.VALIDATION.RANGE", format!("p 的次数 {} 超过上限 {max_degree}。", p.len() - 1), "这么高次的单项式基在 f64 下通常没有意义；检查系数是否写错，或改用正交多项式基。").path("/p"));
    }
    Ok(p)
}

fn fvec(v: &[f64]) -> Value {
    json!({"dtype": "f64", "shape": [v.len()], "data": v.iter().map(|x| num(*x)).collect::<Vec<_>>()})
}

// ------------------------------------------------------------------------------------------------ polyval

pub struct InV {
    p: Vec<f64>,
    x: Vec<f64>,
    mu: Option<(f64, f64)>,
}

impl Operator for PolyVal {
    type Input = InV;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(InV, Vec<Warning>), OpError> {
        let lim = &self.meta.manifest["limits"];
        let p = coeffs(args, host, lim["max_degree"].as_u64().unwrap() as usize)?;
        if args["x"]["dtype"] == "c128" {
            return Err(OpError::new("E.VALIDATION.DTYPE", "x 是复数（c128）；fit.poly.polyval 0.4.0 只在实数点求值（误差界是对实数补偿 Horner 证明的）。", "实数点照常调用；要在 jω 上求传递函数的频率响应，用 control.lti.bode。").path("/x/dtype"));
        }
        let (x, xs) = vec1(&args["x"], "/x", host)?;
        if xs.len() != 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 的 shape 是 {xs:?}；要求一维 [q]。"), "把求值点排成一维数组（单个点写成 [1]），多维网格先展平。").path("/x/shape"));
        }
        let mu = match args.get("mu") {
            None | Some(Value::Null) => None,
            Some(m) => {
                let (a, b) = (m[0].as_f64().unwrap(), m[1].as_f64().unwrap());
                if !(a.is_finite() && b.is_finite()) || b == 0.0 {
                    return Err(OpError::new("E.VALIDATION.RANGE", "mu = [均值, 标准差] 要求两个有限数，且标准差不为 0。", "mu 应原样取自 fit.poly.polyfit（scaling = center_scale）结果里的 mu。").path("/mu"));
                }
                Some((a, b))
            }
        };
        Ok((InV { p, x, mu }, Vec::new()))
    }

    fn execute(&self, i: InV, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.p.len() - 1;
        let q = i.x.len();
        let mut y = Vec::with_capacity(q);
        let mut bound = Vec::with_capacity(q);
        let mut cond = Vec::with_capacity(q);
        let mut consistent = true;
        let mut uncertain: Vec<usize> = Vec::new();
        for (k, &xv) in i.x.iter().enumerate() {
            if k % 65536 == 65535 {
                ctx.check()?;
            }
            let t = match i.mu {
                Some((m, s)) => (xv - m) / s,
                None => xv,
            };
            let (r, b, pt) = comp_horner(&i.p, t);
            if !(r.is_finite() && b.is_finite() && t.is_finite()) {
                return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("在 x[{k}] = {xv:e} 处求值溢出（|x| 的 {n} 次幂超出 f64 范围）。"), "检查 x 的单位；高次多项式在大 |x| 处可先用 fit.poly.polyfit 的 scaling = center_scale 拟合，再连同 mu 一起求值。").path(&format!("/x/data/{k}")).detail("index", json!(k)));
            }
            // post-condition: plain Horner must agree within its own a-priori bound γ_{2n}·p̃ (plus ours)
            let plain = polyval_r(&i.p, t);
            if !((plain - r).abs() <= gamma(2 * n) * pt / (1.0 - gamma(2 * n)) * (1.0 + 4.0 * U) + b) {
                consistent = false;
            }
            // a computed exact zero is reported with its bound (|p(t)| ≤ bound), not warned about
            if r != 0.0 && b > REL_WARN * r.abs() {
                uncertain.push(k);
            }
            y.push(r);
            bound.push(b);
            cond.push(if r != 0.0 { pt / r.abs() } else if pt == 0.0 { 1.0 } else { f64::INFINITY });
        }
        let mut warnings = Vec::new();
        if !uncertain.is_empty() {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("{} 个点上保证不了 8 位有效数字（y ≠ 0 且误差界 > 1e-8·|y|）：x 在病态的重根或密集根附近，即使用补偿算法，结果也只剩很少几位（看 error_bound 与 condition）。第一个是 x[{}]。其余点不受影响。", uncertain.len(), uncertain[0])).path("/x").details(json!({"count": uncertain.len(), "first_index": uncertain[0], "indices": uncertain.iter().take(20).collect::<Vec<_>>()})));
        }
        let max_rel = y.iter().zip(&bound).map(|(v, b)| if *v != 0.0 { b / v.abs() } else if *b == 0.0 { 0.0 } else { f64::INFINITY }).fold(0.0, f64::max);
        let max_abs = bound.iter().cloned().fold(0.0, f64::max);
        let result = json!({
            "y": fvec(&y),
            "error_bound": fvec(&bound),
            "condition": fvec(&cond),
            "degree": n,
            "mu": match i.mu { Some((a, b)) => json!([num(a), num(b)]), None => Value::Null },
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "error_estimate": {"abs_max": num(max_abs), "rel_max": num(max_rel), "basis": "逐点严格界 |y − p(t)| ≤ (u|y| + γ_{2n}²·p̃(|t|))/(1 − u)·(1 + 4u)（Graillat–Langlois–Louvet 2005 定理），p̃(|t|) = Σ|a_i||t|^i；t = x，或给了 mu 时 t = (x − mu[0])/mu[1]（t 本身在 f64 里算出，界针对这个 t）。不考虑下溢。"},
            "condition": {"definition": "cond(p, t) = p̃(|t|)/|p(t)|：系数的相对扰动 ε 至多把 p(t) 改变 ε·cond 倍（逐点值见 result.condition）"},
            "checks": {"horner_consistent": consistent},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"algorithm": "compensated Horner (TwoProduct + TwoSum, no FMA)", "points_below_8_digits": uncertain.len()}
        });
        Ok(Output { result, trust, warnings })
    }
}

// ------------------------------------------------------------------------------------------------ roots

pub struct InR {
    p: Vec<f64>,
    stripped: usize,
}

fn cvec(v: &[C64]) -> Vec<Value> {
    v.iter().map(|c| json!([num(c.re), num(c.im)])).collect()
}

impl Operator for Roots {
    type Input = InR;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(InR, Vec<Warning>), OpError> {
        let lim = &self.meta.manifest["limits"];
        let p = coeffs(args, host, lim["max_degree"].as_u64().unwrap() as usize)?;
        let Some(k) = p.iter().position(|v| *v != 0.0) else {
            return Err(OpError::new("E.DOMAIN.ZERO_POLYNOMIAL", "p 的系数全为 0：零多项式处处为 0，根不确定。", "检查系数是否传错（例如全被舍入成了 0）。").path("/p").remedy(ask_user("coefficients", "多项式的系数是什么？")));
        };
        let mut w = Vec::new();
        if k > 0 {
            w.push(Warning::new("W.VALIDATION.LEADING_ZEROS_STRIPPED", format!("去掉了 {k} 个为 0 的最高次系数；多项式的次数是 {}。", p.len() - 1 - k)).path("/p").details(json!({"removed": k})));
        }
        Ok((InR { p: p[k..].to_vec(), stripped: k }, w))
    }

    fn execute(&self, i: InR, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.p.len() - 1;
        let nz = i.p.iter().rev().take_while(|v| **v == 0.0).count();
        let q = &i.p[..i.p.len() - nz];
        let m = q.len() - 1;
        ctx.check()?;
        // kernel: exact zeros first, then the m roots of q
        let all = poly_roots(&i.p);
        let zq: Vec<C64> = all[nz..].to_vec();
        let (rq, eta) = if m > 0 { inclusion_radii(q, &zq) } else { (vec![], vec![]) };
        // (root, inclusion radius, component key); exact zeros: radius 0, one component of multiplicity nz
        let comp = disc_components(&zq, &rq);
        let mut items: Vec<(C64, f64, usize)> = (0..nz).map(|_| (C64::new(0.0, 0.0), 0.0, usize::MAX)).collect();
        items.extend(zq.iter().zip(&rq).zip(&comp).map(|((z, r), c)| (*z, *r, *c)));
        if items.iter().any(|(z, _, _)| !(z.re.is_finite() && z.im.is_finite())) {
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", "求根时出现非有限值（系数的量级相差过大，友矩阵溢出）。", "把多项式按 x = s·t 换元缩放后再求根，或检查系数是否写错。").path("/p"));
        }
        // per-root error radius: isolated → its disc; in a component → the farthest point of the component
        let key = |c: usize| c;
        let mut mult = vec![0usize; items.len()];
        let mut radius = vec![0.0f64; items.len()];
        for a in 0..items.len() {
            let members: Vec<usize> = (0..items.len()).filter(|&b| key(items[b].2) == key(items[a].2)).collect();
            mult[a] = members.len();
            radius[a] = if members.len() == 1 { items[a].1 } else { members.iter().map(|&b| (items[a].0 - items[b].0).norm() + items[b].1).fold(0.0, f64::max) };
        }
        // sort like control.lti.poles_zeros: real part ascending, then imaginary part descending
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (items[a].0, items[b].0);
            x.re.partial_cmp(&y.re).unwrap_or(std::cmp::Ordering::Equal).then(y.im.partial_cmp(&x.im).unwrap_or(std::cmp::Ordering::Equal))
        });
        let roots: Vec<C64> = order.iter().map(|&k| items[k].0).collect();
        let radii: Vec<f64> = order.iter().map(|&k| radius[k]).collect();
        let mults: Vec<usize> = order.iter().map(|&k| mult[k]).collect();
        let comp_sorted: Vec<usize> = order.iter().map(|&k| items[k].2).collect();
        // clusters (components with more than one member, exact zeros excluded: they are exact)
        let mut clusters = Vec::new();
        let mut seen: Vec<usize> = Vec::new();
        for (pos, &c) in comp_sorted.iter().enumerate() {
            if c == usize::MAX || mults[pos] < 2 || seen.contains(&c) {
                continue;
            }
            seen.push(c);
            let idx: Vec<usize> = (0..roots.len()).filter(|&j| comp_sorted[j] == c).collect();
            let center = idx.iter().fold(C64::new(0.0, 0.0), |s, &j| s + roots[j]) / idx.len() as f64;
            let rad = idx.iter().map(|&j| (roots[j] - center).norm() + items[order[j]].1).fold(0.0, f64::max);
            clusters.push(json!({"indices": idx, "multiplicity": idx.len(), "center": [num(center.re), num(center.im)], "radius": num(rad)}));
        }
        let mut warnings = Vec::new();
        if !clusters.is_empty() {
            let unreliable: Vec<String> = comp_sorted.iter().enumerate().filter(|(j, c)| **c != usize::MAX && mults[*j] > 1).map(|(j, _)| format!("/roots/data/{j}")).collect();
            warnings.push(Warning::new("W.ACCURACY.CLUSTERED_ROOTS", format!("{} 个根簇（重根或密集根）：簇内各根的位置不能单独确定（m 重根只有约 u^(1/m) 的精度），簇的中心、半径与根数可以依赖。各根的 error_radius 是到同簇最远点的距离。", clusters.len())).path("/p").details(json!({"clusters": clusters, "unreliable_roots": unreliable})));
        }
        let eta_max = eta.iter().cloned().fold(0.0, f64::max);
        let threshold = 10.0 * (m.max(1)) as f64 * U;
        let real_count = roots.iter().filter(|z| z.im == 0.0).count();
        let finite_radii = radii.iter().all(|r| r.is_finite());
        let scale = roots.iter().map(|z| z.norm()).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
        let result = json!({
            "roots": {"dtype": "c128", "shape": [n], "data": cvec(&roots)},
            "error_radius": fvec(&radii),
            "multiplicity": mults,
            "degree": n,
            "real_count": real_count,
            "all_real": real_count == n,
            "zero_roots": nz,
            "leading_zeros_removed": i.stripped,
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"backward_error": num(eta_max), "threshold": num(threshold), "definition": "max_i |q(z_i)|/q̃(|z_i|)：每个根是系数相对扰动不超过该值的多项式的精确根（逐根分量后向误差，q 为去掉零根后的多项式，q̃(t) = Σ|a_k|t^k）"},
            "error_estimate": {"abs_max": num(radii.iter().cloned().fold(0.0, f64::max)), "rel_to_max_root": num(radii.iter().cloned().fold(0.0, f64::max) / scale), "basis": "包含定理（Braess–Hadeler 1973，Carstensen 1991）：q 的根全在 ∪ D(z_i, m·|W_i|) 内，W_i = q(z_i)/(q_0·Π_{j≠i}(z_i − z_j))，k 个圆盘连成的分支恰含 k 个根；半径计入 q(z_i) 与乘积的舍入。孤立根的 error_radius 是它的圆盘半径，簇内根是到同簇最远点的距离"},
            "checks": {"backward_stable": eta_max <= threshold, "radii_finite": finite_radii},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"algorithm": "eigenvalues of the balanced companion matrix (kernels::schur), ≤ 4 Newton polishing steps", "clusters": clusters.len()}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn v(d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [d.len()], "data": d})
    }
    #[test]
    fn polyval_basic_and_mu() {
        let e = PolyVal::new().call(&json!({"p": v(&[1., -6., 11., -6.]), "x": v(&[0., 1., 2.5, 4.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let y: Vec<f64> = e["result"]["y"]["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        assert_eq!(y, vec![-6.0, 0.0, -0.375, 6.0]);
        let e = PolyVal::new().call(&json!({"p": v(&[2., 1.]), "x": v(&[10.]), "mu": [8.0, 2.0]}), &HostCtx::default());
        assert_eq!(e["result"]["y"]["data"][0], 3.0);
        let e = PolyVal::new().call(&json!({"p": v(&[1., 0., 0., 0.]), "x": v(&[1e200])}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.NUMERICAL.NON_FINITE_EVALUATION");
    }
    #[test]
    fn roots_sorted_clusters_and_zeros() {
        let e = Roots::new().call(&json!({"p": v(&[1., -6., 11., -6.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let d = &e["result"]["roots"]["data"];
        for (k, w) in [1.0, 2.0, 3.0].iter().enumerate() {
            assert!((d[k][0].as_f64().unwrap() - w).abs() < 1e-14 && d[k][1] == 0.0, "{d}");
        }
        let e = Roots::new().call(&json!({"p": v(&[1., 2., 5., 0., 0.])}), &HostCtx::default());
        assert_eq!(e["result"]["zero_roots"], 2);
        assert_eq!(e["result"]["roots"]["data"][0][0].as_f64().unwrap(), -1.0);
        assert_eq!(e["result"]["roots"]["data"][0][1].as_f64().unwrap(), 2.0);
        assert!(e["warnings"].as_array().unwrap().is_empty(), "exact zero roots are not a cluster: {e}");
        let e = Roots::new().call(&json!({"p": v(&[1., -3., 3., -1.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok_with_warnings", "{e}");
        assert_eq!(e["warnings"][0]["code"], "W.ACCURACY.CLUSTERED_ROOTS");
        assert_eq!(e["result"]["multiplicity"], json!([3, 3, 3]));
        let e = Roots::new().call(&json!({"p": v(&[0., 0.])}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.ZERO_POLYNOMIAL");
        let e = Roots::new().call(&json!({"p": v(&[0., 2., -4.])}), &HostCtx::default());
        assert_eq!(e["warnings"][0]["code"], "W.VALIDATION.LEADING_ZEROS_STRIPPED");
        assert_eq!(e["result"]["degree"], 1);
        let e = Roots::new().call(&json!({"p": v(&[5.])}), &HostCtx::default());
        assert_eq!(e["result"]["roots"]["shape"], json!([0]));
    }
}
