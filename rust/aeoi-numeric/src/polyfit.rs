//! fit.poly.polyfit — least-squares polynomial fit (spec 10.21, v0.4.11).
//!
//! p (highest power first, MATLAB/numpy order) minimises ‖V·p − y‖₂ with the Vandermonde matrix
//! V[i][j] = t_i^(d−j), built by repeated multiplication like MATLAB polyfit. Solved by the Householder QR of
//! linsolve (same factor serves the condition estimate and the coefficient standard errors), then at most 3
//! steps of corrected semi-normal-equation refinement. Rank test, condition and backward error are measured
//! on the column-scaled V·D⁻¹ (D = column 2-norms), so they do not depend on the units of x.
//! t = x (scaling = none, default) or t = (x − mean)/std (scaling = center_scale, MATLAB's [p,S,mu]).
//! Author's decisions (2026-11-25): no silent centering — an ill-conditioned fit is returned as asked, with
//! W.NUMERICAL.ILL_CONDITIONED and a suggested center_scale patch the user has to confirm; standard errors of
//! the coefficients are part of the result.
use crate::interp1::vec1;
use aeoi_core::kernels::dense::{at_r_dd, inv_norm1_estimate, norm2, qr_factor, residual_dd};
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::sum::Dot2;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(PolyFit, "fit.poly.polyfit");

const U: f64 = f64::EPSILON / 2.0;
/// κ₁(R) above which the fit warns (coefficients may have lost ~10 of 16 digits).
pub const COND_WARN: f64 = 1e10;
const MAX_REFINE: usize = 3;

pub struct In {
    x: Vec<f64>,
    y: Vec<f64>,
    d: usize,
    center: bool,
}

fn distinct(x: &[f64]) -> usize {
    let mut s = x.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s.dedup();
    s.len()
}

/// (mean, sample std) with compensated sums.
fn mean_std(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let mut s = Dot2::default();
    for v in x {
        s.add(*v);
    }
    let (h, l) = s.hi_lo();
    let mean = (h + l) / n;
    let mut q = Dot2::default();
    for v in x {
        let d = v - mean;
        q.add_prod(d, d);
    }
    let (h, l) = q.hi_lo();
    (mean, ((h + l) / (n - 1.0)).sqrt())
}

fn vander(t: &[f64], d: usize) -> Vec<f64> {
    let (m, c) = (t.len(), d + 1);
    let mut v = vec![0.0; m * c];
    for i in 0..m {
        v[i * c + d] = 1.0;
        for j in (0..d).rev() {
            v[i * c + j] = v[i * c + j + 1] * t[i];
        }
    }
    v
}

impl Operator for PolyFit {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let (x, xs) = vec1(&args["x"], "/x", host)?;
        if xs.len() != 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 的 shape 是 {xs:?}；要求一维 [n]。"), "把 x 写成 shape [n]。").path("/x/shape"));
        }
        let (y, ys) = vec1(&args["y"], "/y", host)?;
        if ys.len() != 1 || y.len() != x.len() {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("y 的 shape 是 {ys:?}，x 有 {} 个点；y 要与 x 等长的一维 [n]。", x.len()), "检查 x 与 y 是否一一对应；多组数据分别调用。").path("/y/shape").detail("n", json!(x.len())));
        }
        let d = args["degree"].as_u64().unwrap() as usize;
        let lim = self.meta.manifest["limits"]["max_degree"].as_u64().unwrap() as usize;
        if d > lim {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("degree = {d} 超过上限 {lim}。这么高次的单项式基拟合在 f64 下没有意义。"), "降低次数，或改用样条（interp.grid.interp1 的 spline）/ 正交多项式基。").path("/degree"));
        }
        let center = args.get("scaling").and_then(|v| v.as_str()).unwrap_or("none") == "center_scale";
        let nd = distinct(&x);
        if nd < d + 1 {
            let mut e = OpError::new("E.DOMAIN.UNDERDETERMINED", format!("{d} 次多项式有 {} 个系数，但 x 只有 {nd} 个不同的值（共 {} 个点）：拟合多项式不唯一。", d + 1, x.len()), "降低次数（至多 {nd} − 1），或补充不同 x 处的数据。")
                .path("/degree")
                .detail("distinct_x", json!(nd))
                .detail("coefficients", json!(d + 1));
            if nd >= 1 {
                e = e.remedy(modify_input("lower_degree", &format!("把次数降到 {}（模型改变，须用户确认）", nd - 1), "medium", true, json!([{"op": "replace", "path": "/degree", "value": nd - 1}])));
            }
            return Err(e.remedy(ask_user("more_data", "数据点不够：能否补充更多不同 x 处的测量？")));
        }
        if center && !(x.len() >= 2 && nd >= 2) {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", "scaling = center_scale 要用 x 的标准差缩放，但 x 只有一个不同的值，标准差为 0。", "去掉 scaling（只拟合常数时不需要缩放）。")
                .path("/scaling")
                .remedy(modify_input("drop_scaling", "删除 scaling", "high", false, json!([{"op": "remove", "path": "/scaling"}]))));
        }
        Ok((In { x, y, d, center }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, d) = (i.x.len(), i.d);
        let c = d + 1;
        let mu = if i.center { Some(mean_std(&i.x)) } else { None };
        let t: Vec<f64> = match mu {
            Some((mean, sd)) => i.x.iter().map(|v| (v - mean) / sd).collect(),
            None => i.x.clone(),
        };
        let v = vander(&t, d);
        if v.iter().any(|e| !e.is_finite()) {
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("x 的 {d} 次幂溢出（|x| 太大）。"), "用 scaling = center_scale，或先把 x 换成较小的单位。")
                .path("/x")
                .remedy(modify_input("center_scale", "加上 scaling = center_scale（系数改为对 (x − mean)/std 的多项式，须用户确认）", "high", true, json!([{"op": "add", "path": "/scaling", "value": "center_scale"}]))));
        }
        let qr = qr_factor(&v, m, c);
        // Column scaling D = diag(‖V[:, j]‖₂): rank test, condition and backward error are measured on
        // V·D⁻¹ (= R·D⁻¹ after QR), so they do not depend on the units of x, only on its position and spread.
        let cn: Vec<f64> = (0..c).map(|j| (0..m).map(|r| v[r * c + j] * v[r * c + j]).sum::<f64>().sqrt()).collect();
        let rs = |i: usize, j: usize| qr.r(i, j) / cn[j];
        let rmax = (0..c).map(|j| rs(j, j).abs()).fold(0.0, f64::max);
        let tol_rank = m.max(c) as f64 * U * rmax;
        if let Some(j) = (0..c).find(|&j| rs(j, j).abs() <= tol_rank) {
            let rank = (0..c).filter(|&j| rs(j, j).abs() > tol_rank).count();
            let mut e = OpError::new("E.NUMERICAL.SINGULAR", format!("x 有足够多的不同值，但 {d} 次单项式基在 f64 下数值秩不足：列缩放后 R 的第 {j} 个对角元 ≤ max(m, n)·u·max|r_ii|（数值秩 {rank} < {c}）。"), if i.center { "降低次数，或改用正交多项式基。" } else { "x 远离 0 或跨度大时单项式基严重病态：用 scaling = center_scale，或降低次数。" })
                .path("/degree")
                .detail("rank_estimate", json!(rank))
                .detail("coefficients", json!(c));
            if !i.center {
                e = e.remedy(modify_input("center_scale", "加上 scaling = center_scale（系数改为对 (x − mean)/std 的多项式，须用户确认）", "high", true, json!([{"op": "add", "path": "/scaling", "value": "center_scale"}])));
            }
            return Err(e.remedy(ask_user("lower_degree", "要不要降低多项式次数？")));
        }
        ctx.check()?;
        let mut p = qr.solve_r(&qr.qt(&i.y));
        // Corrected semi-normal equations (Björck): δ = R⁻¹R⁻ᵀ·(Vᵀr) with Vᵀr in compensated arithmetic,
        // at most MAX_REFINE steps while the correction keeps halving.
        let mut refine_steps = 0usize;
        let mut prev = f64::INFINITY;
        for _ in 0..MAX_REFINE {
            let r = residual_dd(&v, m, c, &p, &i.y);
            let delta = qr.solve_r(&qr.solve_rt(&at_r_dd(&v, m, c, &r)));
            let dn = norm2(&delta.iter().zip(&cn).map(|(a, n)| a * n).collect::<Vec<_>>());
            let pn = norm2(&p.iter().zip(&cn).map(|(a, n)| a * n).collect::<Vec<_>>());
            if !dn.is_finite() || dn > prev / 2.0 {
                break;
            }
            for (pj, dj) in p.iter_mut().zip(&delta) {
                *pj += dj;
            }
            refine_steps += 1;
            prev = dn;
            if dn <= 2.0 * U * pn {
                break;
            }
        }
        let r = residual_dd(&v, m, c, &p, &i.y);
        let atr = at_r_dd(&v, m, c, &r);
        // scaled quantities: Vs = V·D⁻¹ (‖Vs‖_F = sqrt(c)), ps = D·p, Vsᵀr = D⁻¹·Vᵀr
        let vfro = (c as f64).sqrt();
        let atr_s: Vec<f64> = atr.iter().zip(&cn).map(|(a, n)| a / n).collect();
        let ps: Vec<f64> = p.iter().zip(&cn).map(|(a, n)| a * n).collect();
        let (rn, pn) = (norm2(&r), norm2(&ps));
        let eta = norm2(&atr_s) / (vfro * (rn + vfro * pn)).max(f64::MIN_POSITIVE);
        let threshold = 10.0 * m as f64 * U;
        // (R·D⁻¹)⁻¹ = D·R⁻¹ ; (R·D⁻¹)⁻ᵀ c = R⁻ᵀ·(D·c)
        let inv1 = inv_norm1_estimate(c, &|b| qr.solve_r(b).iter().zip(&cn).map(|(z, n)| z * n).collect(), &|b| qr.solve_rt(&b.iter().zip(&cn).map(|(z, n)| z * n).collect::<Vec<_>>()));
        let rnorm1 = (0..c).map(|j| (0..=j).map(|k| rs(k, j).abs()).sum::<f64>()).fold(0.0, f64::max);
        let kappa1 = rnorm1 * inv1;
        let fwd = eta * (2.0 * kappa1 + kappa1 * kappa1 * rn / (vfro * pn).max(f64::MIN_POSITIVE));
        // standard errors: sqrt(diag((RᵀR)⁻¹)) · ‖r‖/sqrt(dof); diag((RᵀR)⁻¹)_j = ‖row j of R⁻¹‖² (R⁻¹ by columns)
        let dof = m - c;
        let std_errors = if dof > 0 {
            let mut rowsq = vec![0.0f64; c];
            for col in 0..c {
                let mut e = vec![0.0; c];
                e[col] = 1.0;
                let z = qr.solve_r(&e);
                for (j, zj) in z.iter().enumerate() {
                    rowsq[j] += zj * zj;
                }
            }
            let s = rn / (dof as f64).sqrt();
            json!(rowsq.iter().map(|q| num(q.sqrt() * s)).collect::<Vec<_>>())
        } else {
            Value::Null
        };
        let mut warnings = Vec::new();
        if !(kappa1 <= COND_WARN) {
            let mut det = json!({"kappa1_estimate": num(kappa1), "threshold": COND_WARN});
            if !i.center {
                det["suggested_remediation"] = modify_input("center_scale", "加上 scaling = center_scale：系数改为对 (x − mean)/std 的多项式（mu 随结果返回），条件数通常降几个数量级。改变系数的含义，须用户确认", "high", true, json!([{"op": "add", "path": "/scaling", "value": "center_scale"}]));
            }
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("拟合病态：κ₁(R·D⁻¹) ≈ {kappa1:.3e} > {COND_WARN:.0e}。对给定数据算得的系数仍然准确（误差估计 {fwd:.1e}），但数据的相对扰动会被放大约 κ 倍传到系数上，系数之间高度相关。{}", if i.center { "已经中心化缩放；考虑降低次数。" } else { "建议 scaling = center_scale（见 details.suggested_remediation，须用户确认）。拟合曲线本身（残差）通常仍然可用。" })).path("/degree").details(det));
        }
        let finite = p.iter().all(|v| v.is_finite());
        let result = json!({
            "p": {"dtype": "f64", "shape": [c], "data": p.iter().map(|v| num(*v)).collect::<Vec<_>>()},
            "degree": d,
            "scaling": if i.center { "center_scale" } else { "none" },
            "mu": match mu { Some((a, b)) => json!([num(a), num(b)]), None => Value::Null },
            "residual_norm": num(rn),
            "dof": dof,
            "std_errors": std_errors,
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"backward_error": num(eta), "threshold": num(threshold), "definition": "‖Vsᵀ(y − V·p)‖₂ / (‖Vs‖_F·(‖r‖₂ + ‖Vs‖_F‖D·p‖₂))，Vs = V·D⁻¹，D = diag(‖V 的列‖₂)（最小二乘最优性，与 x 的单位无关；残差用补偿算术 Dot2）"},
            "condition": {"kappa1_estimate": num(kappa1), "rcond": num(1.0 / kappa1), "of": "R·D⁻¹（V = QR，V 为所用基的 Vandermonde 矩阵，D 为其列 2-范数；与 x 的单位无关）", "estimator": "Hager–Higham 1-范数估计，下界"},
            "error_estimate": {"Dp_rel_forward_2": num(fwd), "basis": "‖D·(p̂ − p)‖₂/‖D·p‖₂ 的一阶界 η·(2κ + κ²·‖r‖/(‖Vs‖‖D·p‖))（Golub–Van Loan 5.3.8）；D·p 的分量 = 各项在数据上的贡献大小"},
            "checks": {"backward_stable": eta <= threshold, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"refinement_steps": refine_steps, "std_errors_model": "y = V·p + ε，ε 独立同分布、等方差；σ̂ = ‖r‖₂/sqrt(dof)", "basis": if i.center { "t = (x − mu[0])/mu[1]" } else { "t = x" }}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn call(args: Value) -> Value {
        PolyFit::new().call(&args, &HostCtx::default())
    }
    fn v(d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [d.len()], "data": d})
    }
    fn p(env: &Value) -> Vec<f64> {
        env["result"]["p"]["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
    }
    #[test]
    fn exact_line_and_quadratic() {
        let e = call(json!({"x": v(&[0., 1., 2., 3., 4.]), "y": v(&[1., 3., 5., 7., 9.]), "degree": 1}));
        assert_eq!(e["status"], "ok", "{e}");
        let q = p(&e);
        assert!((q[0] - 2.0).abs() < 1e-14 && (q[1] - 1.0).abs() < 1e-14, "{q:?}");
        let e = call(json!({"x": v(&[-1., 0., 1., 2.]), "y": v(&[6., 3., 2., 3.]), "degree": 2}));
        let q = p(&e);
        assert!((q[0] - 1.0).abs() < 1e-14 && (q[1] + 2.0).abs() < 1e-14 && (q[2] - 3.0).abs() < 1e-14, "{q:?}");
        assert_eq!(e["result"]["dof"], 1);
    }
    #[test]
    fn interpolating_fit_has_no_std_errors() {
        let e = call(json!({"x": v(&[0., 1., 2.]), "y": v(&[1., 0., 5.]), "degree": 2}));
        assert_eq!(e["result"]["std_errors"], Value::Null);
    }
    #[test]
    fn years_warn_and_center_scale_fixes() {
        let x: Vec<f64> = (1990..=2020).map(|t| t as f64).collect();
        let y: Vec<f64> = x.iter().map(|t| ((t - 2005.0) / 10.0).sin()).collect();
        let e = call(json!({"x": v(&x), "y": v(&y), "degree": 3}));
        assert_eq!(e["status"], "ok", "{e}"); // κ(R·D⁻¹) ≈ 1.3e8: fine
        let e = call(json!({"x": v(&x), "y": v(&y), "degree": 8}));
        assert_eq!(e["error"]["code"], "E.NUMERICAL.SINGULAR", "{e}");
        assert_eq!(e["error"]["remediation"][0]["patch"][0]["value"], "center_scale");
        let e = call(json!({"x": v(&x), "y": v(&y), "degree": 4}));
        let codes: Vec<&str> = e["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        assert!(codes.contains(&"W.NUMERICAL.ILL_CONDITIONED"), "{e}");
        let e = call(json!({"x": v(&x), "y": v(&y), "degree": 4, "scaling": "center_scale"}));
        assert_eq!(e["status"], "ok", "{e}");
        assert!((e["result"]["mu"][0].as_f64().unwrap() - 2005.0).abs() < 1e-12);
    }
    #[test]
    fn errors() {
        let e = call(json!({"x": v(&[0., 1., 2.]), "y": v(&[1., 2., 3.]), "degree": 3}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.UNDERDETERMINED");
        let e = call(json!({"x": v(&[1., 1., 2., 2.]), "y": v(&[1., 2., 3., 4.]), "degree": 2}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.UNDERDETERMINED");
        let e = call(json!({"x": v(&[1., 2.]), "y": v(&[1., 2., 3.]), "degree": 1}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
    }
}
