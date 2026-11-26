//! calculus.quadrature.integral (spec 10.31) and calculus.quadrature.trapz (spec 10.32), v0.4.15.
//!
//! integral: global adaptive Gauss–Kronrod (7, 15) with QUADPACK's error estimate, on MATLAB integral's
//! transformations (finite [a, b]: x = (a+b)/2 + (b−a)/4·t(3 − t²), which weakens endpoint singularities;
//! infinite limits: x = t/(1 − t²) or a ± t/(1 − t)). Author's decisions (2026-11-25): default rel_tol 1e-10
//! (MATLAB: 1e-6), tolerances echoed with an error estimate; not reaching the tolerance is an error carrying
//! the current estimate (E.NUMERICAL.NO_CONVERGENCE). Post-condition: every subinterval split once more must
//! agree with the returned value within the tolerance.
//! trapz: trapezoidal rule on data (MATLAB trapz / cumtrapz) with compensated summation; the spacing must be
//! given explicitly (x or dx).
use crate::expr::{check_name, eval, expr_err, parse, parse_params, Node};
use crate::interp1::vec1;
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::sum::Dot2;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Integral, "calculus.quadrature.integral");
operator_struct!(Trapz, "calculus.quadrature.trapz");

// QUADPACK qk15 nodes (positive half) and weights
const XGK: [f64; 8] = [0.991455371120812639206854697526329, 0.949107912342758524526189684047851, 0.864864423359769072789712788640926, 0.741531185599394439863864773280788, 0.586087235467691130294144845693013, 0.405845151377397166906606412076961, 0.207784955007898467600689403773245, 0.0];
const WGK: [f64; 8] = [0.022935322010529224963732008058970, 0.063092092629978553290700663189204, 0.104790010322250183839876322541518, 0.140653259715525918745189590510238, 0.169004726639267902826583426598550, 0.190350578064785409913256402421014, 0.204432940075298892414161999234649, 0.209482141084727828012999174891714];
const WG: [f64; 4] = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780, 0.381830050505118944950369775488975, 0.417959183673469387755102040816327];

#[derive(Clone, Copy)]
enum Map {
    Finite(f64, f64),
    Both,
    Upper(f64), // [a, ∞)
    Lower(f64), // (−∞, b]
}

pub struct InQ {
    expr: Node,
    vars: Vec<f64>,
    a: f64,
    b: f64,
    map: Map,
    sign: f64,
    rel_tol: f64,
    abs_tol: f64,
    max_intervals: usize,
}

struct Seg {
    lo: f64,
    hi: f64,
    val: f64,
    err: f64,
}

fn gk15(f: &mut dyn FnMut(f64) -> f64, lo: f64, hi: f64) -> (f64, f64, bool) {
    let c = 0.5 * (lo + hi);
    let h = 0.5 * (hi - lo);
    let fc = f(c);
    let mut resk = fc * WGK[7];
    let mut resg = fc * WG[3];
    let mut resabs = resk.abs();
    let mut fv1 = [0.0; 7];
    let mut fv2 = [0.0; 7];
    for j in 0..7 {
        let x = h * XGK[j];
        let (f1, f2) = (f(c - x), f(c + x));
        fv1[j] = f1;
        fv2[j] = f2;
        resk += WGK[j] * (f1 + f2);
        resabs += WGK[j] * (f1.abs() + f2.abs());
        if j % 2 == 1 {
            resg += WG[j / 2] * (f1 + f2);
        }
    }
    let reskh = resk * 0.5;
    let mut resasc = WGK[7] * (fc - reskh).abs();
    for j in 0..7 {
        resasc += WGK[j] * ((fv1[j] - reskh).abs() + (fv2[j] - reskh).abs());
    }
    let (result, resabs, resasc) = (resk * h, resabs * h.abs(), resasc * h.abs());
    let mut err = ((resk - resg) * h).abs();
    if resasc != 0.0 && err != 0.0 {
        err = resasc * aeoi_libm::libm::pow(200.0 * err / resasc, 1.5).min(1.0);
    }
    if resabs > f64::MIN_POSITIVE / (50.0 * f64::EPSILON) {
        err = err.max(50.0 * f64::EPSILON * resabs);
    }
    let finite = result.is_finite() && fv1.iter().chain(fv2.iter()).all(|v| v.is_finite()) && fc.is_finite();
    (result, err, finite)
}

impl Operator for Integral {
    type Input = InQ;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InQ, Vec<Warning>), OpError> {
        let var = args.get("variable").and_then(|v| v.as_str()).unwrap_or("x").to_string();
        check_name(&var).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个变量名。").path("/variable").detail("kind", json!("bad_name")))?;
        let (pn, pv) = parse_params(&args["params"], std::slice::from_ref(&var))?;
        let mut names = vec![var];
        names.extend(pn);
        let src = args["expression"].as_str().unwrap();
        let expr = parse(src, &names).map_err(|e| expr_err("/expression", src, e))?;
        let lim = |v: &Value| match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) if s == "Inf" => Some(f64::INFINITY),
            Value::String(s) if s == "-Inf" => Some(f64::NEG_INFINITY),
            _ => None,
        };
        let l = args["limits"].as_array().unwrap();
        let (mut a, mut b) = (lim(&l[0]).unwrap(), lim(&l[1]).unwrap());
        let mut sign = 1.0;
        if a > b {
            std::mem::swap(&mut a, &mut b);
            sign = -1.0;
        }
        let map = match (a.is_finite(), b.is_finite()) {
            (true, true) => Map::Finite(a, b),
            (false, false) => {
                if a == b {
                    return Err(OpError::new("E.VALIDATION.RANGE", "两个积分限是同一个无穷大。", "检查积分限。").path("/limits"));
                }
                Map::Both
            }
            (true, false) => Map::Upper(a),
            (false, true) => Map::Lower(b),
        };
        let rel_tol = args.get("rel_tol").and_then(|v| v.as_f64()).unwrap_or(1e-10);
        let abs_tol = args.get("abs_tol").and_then(|v| v.as_f64()).unwrap_or(1e-12);
        let max_intervals = self.meta.manifest["limits"]["max_intervals"].as_u64().unwrap() as usize;
        let mut vars = vec![0.0];
        vars.extend(pv);
        Ok((InQ { expr, vars, a, b, map, sign, rel_tol, abs_tol, max_intervals }, Vec::new()))
    }

    fn execute(&self, i: InQ, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut vars = i.vars.clone();
        let evals = std::cell::Cell::new(0usize);
        let bad: std::cell::Cell<Option<f64>> = std::cell::Cell::new(None);
        // integrand in t, including the Jacobian of the map
        let mut g = |t: f64| -> f64 {
            let (x, dx) = match i.map {
                Map::Finite(a, b) => ((a + b) / 2.0 + (b - a) / 4.0 * t * (3.0 - t * t), (b - a) / 4.0 * 3.0 * (1.0 - t * t)),
                Map::Both => { let d = 1.0 - t * t; (t / d, (1.0 + t * t) / (d * d)) }
                Map::Upper(a) => { let d = 1.0 - t; (a + t / d, 1.0 / (d * d)) }
                Map::Lower(b) => { let d = 1.0 - t; (b - t / d, 1.0 / (d * d)) }
            };
            evals.set(evals.get() + 1);
            // dx = 0 at the ends of a finite interval; x = ±∞ when a node rounds onto t = ±1 of an infinite map
            // (the integrand's tail must vanish there for the integral to exist)
            if dx == 0.0 || !x.is_finite() || !dx.is_finite() {
                return 0.0;
            }
            vars[0] = x;
            let v = eval(&i.expr, &vars) * dx;
            if !v.is_finite() && bad.get().is_none() {
                bad.set(Some(x));
            }
            v
        };
        let (t0, t1) = match i.map {
            Map::Finite(..) | Map::Both => (-1.0, 1.0),
            _ => (0.0, 1.0),
        };
        // global adaptive: start from 10 equal panels (as MATLAB), always split the worst
        let mut segs: Vec<Seg> = Vec::new();
        for k in 0..10 {
            let (lo, hi) = (t0 + (t1 - t0) * k as f64 / 10.0, t0 + (t1 - t0) * (k + 1) as f64 / 10.0);
            let (v, e, _) = gk15(&mut g, lo, hi);
            segs.push(Seg { lo, hi, val: v, err: e });
        }
        let total = |s: &[Seg]| -> (f64, f64) {
            let mut d = Dot2::default();
            for x in s {
                d.add(x.val);
            }
            let (h, l) = d.hi_lo();
            (h + l, s.iter().map(|x| x.err).sum())
        };
        let tol = |v: f64| i.abs_tol.max(i.rel_tol * v.abs());
        loop {
            if let Some(x) = bad.get() {
                return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("被积函数在 x = {x:e} 处不是有限值（NaN 或 Inf）。"), "积分区间内有奇点或定义域问题：把区间在奇点处分开，或检查表达式。").path("/expression").detail("at", json!(num(x))));
            }
            let (v, e) = total(&segs);
            if e <= tol(v) {
                break;
            }
            if segs.len() >= i.max_intervals {
                let worst = segs.iter().max_by(|a, b| a.err.partial_cmp(&b.err).unwrap()).unwrap();
                let back = |t: f64| match i.map {
                    Map::Finite(a, b) => (a + b) / 2.0 + (b - a) / 4.0 * t * (3.0 - t * t),
                    Map::Both => t / (1.0 - t * t),
                    Map::Upper(a) => a + t / (1.0 - t),
                    Map::Lower(b) => b - t / (1.0 - t),
                };
                return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("{} 个子区间后误差估计 {e:.3e} 仍大于容差 {:.3e}；当前估计值 {:.15e}。", segs.len(), tol(v), i.sign * v), "被积函数可能在某处奇异、剧烈振荡或积分发散：在难点处分段积分，或放宽 rel_tol / abs_tol。")
                    .path("/expression")
                    .detail("estimate", json!(num(i.sign * v)))
                    .detail("error_estimate", json!(num(e)))
                    .detail("hardest_subinterval", json!([num(back(worst.lo).min(back(worst.hi))), num(back(worst.lo).max(back(worst.hi)))]))
                    .remedy(modify_input("loosen", "把 rel_tol 放宽到 1e-6（MATLAB 的缺省），须用户确认", "medium", true, json!([{"op": "add", "path": "/rel_tol", "value": 1e-6}])))
                    .remedy(ask_user("singularity", "被积函数在哪里有奇点或剧烈变化？可以在那里分段。")));
            }
            let (k, _) = segs.iter().enumerate().fold((0, -1.0), |(bk, be), (k, s)| if s.err > be { (k, s.err) } else { (bk, be) });
            let s = segs.swap_remove(k);
            let mid = 0.5 * (s.lo + s.hi);
            let (v1, e1, _) = gk15(&mut g, s.lo, mid);
            let (v2, e2, _) = gk15(&mut g, mid, s.hi);
            segs.push(Seg { lo: s.lo, hi: mid, val: v1, err: e1 });
            segs.push(Seg { lo: mid, hi: s.hi, val: v2, err: e2 });
            if segs.len() % 50 == 0 {
                ctx.check()?;
            }
            segs.sort_by(|a, b| a.lo.partial_cmp(&b.lo).unwrap());
        }
        let (v, e) = total(&segs);
        let n_int = segs.len();
        // post-condition: split every subinterval once more and compare
        let mut d = Dot2::default();
        for s in &segs {
            let mid = 0.5 * (s.lo + s.hi);
            d.add(gk15(&mut g, s.lo, mid).0);
            d.add(gk15(&mut g, mid, s.hi).0);
        }
        let (h, l) = d.hi_lo();
        let v2 = h + l;
        let consistent = (v2 - v).abs() <= tol(v);
        // reported error: the larger of the Kronrod estimate and the change under one more bisection everywhere
        // (the Kronrod estimate alone under-reports near endpoint singularities)
        let e_rep = e.max((v2 - v).abs());
        let value = i.sign * v;
        let transform = match i.map {
            Map::Finite(..) => "x = (a+b)/2 + (b−a)/4·t(3 − t²)",
            Map::Both => "x = t/(1 − t²)",
            Map::Upper(_) => "x = a + t/(1 − t)",
            Map::Lower(_) => "x = b − t/(1 − t)",
        };
        let result = json!({
            "value": num(value), "error_estimate": num(e_rep),
            "tolerances": {"rel_tol": i.rel_tol, "abs_tol": i.abs_tol},
            "limits": [num(if i.sign > 0.0 { i.a } else { i.b }), num(if i.sign > 0.0 { i.b } else { i.a })],
            "intervals": n_int, "evaluations": evals.get(),
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "criterion": "Σ 子区间误差估计 ≤ max(abs_tol, rel_tol·|I|)"},
            "residual": {"refined_value": num(i.sign * v2), "refined_difference": num((v2 - v).abs()), "definition": "每个子区间再二分一次后的积分值与返回值之差"},
            "condition": {"applicable": false},
            "error_estimate": {"value_abs": num(e_rep), "kronrod_sum": num(e), "basis": "max(Σ Gauss–Kronrod (7, 15) 误差估计, |全部子区间再二分一次后的变化|)（启发式；前者在端点奇异处会偏小）"},
            "checks": {"tolerance_met": e <= tol(v), "refinement_consistent": consistent, "all_finite": value.is_finite() && e.is_finite()},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"transform": transform}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================ trapz
pub struct InT {
    y: Vec<Vec<f64>>,
    y_1d: bool,
    x: Option<Vec<f64>>,
    dx: f64,
    cumulative: bool,
}

impl Operator for Trapz {
    type Input = InT;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(InT, Vec<Warning>), OpError> {
        let (yv, ys) = vec1(&args["y"], "/y", host)?;
        let (y, y_1d): (Vec<Vec<f64>>, bool) = match ys.as_slice() {
            [n] => (vec![yv], *n >= 1),
            [k, n] if *k >= 1 => ((0..*k).map(|r| yv[r * n..(r + 1) * n].to_vec()).collect(), false),
            _ => return Err(OpError::new("E.VALIDATION.SHAPE", format!("y 的 shape 是 {ys:?}；要求 [n] 或 [k, n]（每行一个通道）。"), "MATLAB 按列积分，这里按行：若 y 是 [n, k]，先转置。").path("/y/shape")),
        };
        let n = y[0].len();
        if n < 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "y 是空的。", "给出数据。").path("/y"));
        }
        let has_x = args.get("x").is_some();
        let has_dx = args.get("dx").is_some();
        if has_x == has_dx {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", if has_x { "同时给出了 x 和 dx；只给一种。" } else { "没有给出采样位置：要 x（每个点的位置）或 dx（等间距的步长）。MATLAB 的 trapz(y) 默认步长为 1，常被误用。" }, "给出 x，或给出带单位意义的 dx。").path("/").remedy(ask_user("spacing", "数据点之间的间距是多少（单位）？或者给出每个点的位置。")));
        }
        let x = if has_x {
            let (xv, xs) = vec1(&args["x"], "/x", host)?;
            if xs.len() != 1 || xv.len() != n {
                return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 有 {} 个点，y 每行有 {n} 个。", xv.len()), "x 与 y 的最后一维等长。").path("/x/shape"));
            }
            Some(xv)
        } else {
            None
        };
        let dx = args.get("dx").and_then(|v| v.as_f64()).unwrap_or(1.0);
        Ok((InT { y, y_1d, x, dx, cumulative: args.get("cumulative").and_then(|v| v.as_bool()).unwrap_or(false) }, Vec::new()))
    }
    fn execute(&self, i: InT, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.y[0].len();
        let h = |k: usize| match &i.x { Some(x) => x[k + 1] - x[k], None => i.dx };
        let mut totals = Vec::new();
        let mut cums = Vec::new();
        for row in &i.y {
            let mut d = Dot2::default();
            let mut cum = vec![0.0];
            for k in 0..n.saturating_sub(1) {
                // (x_{k+1} − x_k)·(y_k + y_{k+1})/2 as two exact-product terms
                d.add_prod(0.5 * h(k), row[k]);
                d.add_prod(0.5 * h(k), row[k + 1]);
                if i.cumulative {
                    let (a, b) = d.hi_lo();
                    cum.push(a + b);
                }
            }
            let (a, b) = d.hi_lo();
            totals.push(a + b);
            cums.push(cum);
        }
        let finite = totals.iter().all(|v| v.is_finite());
        let value = if i.y_1d { num(totals[0]) } else { json!({"dtype": "f64", "shape": [totals.len()], "data": totals.iter().map(|v| num(*v)).collect::<Vec<_>>()}) };
        let cumulative = if !i.cumulative { Value::Null } else if i.y_1d { json!({"dtype": "f64", "shape": [n], "data": cums[0].iter().map(|v| num(*v)).collect::<Vec<_>>()}) } else { json!({"dtype": "f64", "shape": [cums.len(), n], "data": cums.iter().flatten().map(|v| num(*v)).collect::<Vec<_>>()}) };
        let result = json!({"value": value, "cumulative": cumulative, "spacing": match &i.x { Some(_) => json!("x"), None => json!({"dx": num(i.dx)}) }});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"applicable": false},
            "condition": {"applicable": false},
            "error_estimate": {"applicable": false, "note": "梯形公式对给定数据是精确的求和（补偿算术，至多一次舍入）；它与数据背后真实函数的积分之差取决于采样，算子无法估计"},
            "checks": {"all_finite": finite},
            "accuracy_grade": "A",
            "credentials": Value::Null,
            "derived": {"rule": "Σ (x_{k+1} − x_k)·(y_k + y_{k+1})/2，补偿求和"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn q(args: Value) -> Value {
        Integral::new().call(&args, &HostCtx::default())
    }
    #[test]
    fn integral_cases() {
        let e = q(json!({"expression": "exp(-x^2)", "limits": ["-Inf", "Inf"]}));
        assert_eq!(e["status"], "ok", "{e}");
        assert!((e["result"]["value"].as_f64().unwrap() - std::f64::consts::PI.sqrt()).abs() < 1e-13, "{e}");
        let e = q(json!({"expression": "1/sqrt(x)", "limits": [0.0, 1.0]}));
        assert!((e["result"]["value"].as_f64().unwrap() - 2.0).abs() < 1e-9, "{e}");
        let e = q(json!({"expression": "sin(x)", "limits": [3.141592653589793, 0.0]}));
        assert!((e["result"]["value"].as_f64().unwrap() + 2.0).abs() < 1e-13, "{e}");
        let e = q(json!({"expression": "1/x", "limits": [0.0, 1.0]}));
        assert!(e["error"]["code"] == "E.NUMERICAL.NO_CONVERGENCE" || e["error"]["code"] == "E.NUMERICAL.NON_FINITE_EVALUATION", "{e}");
    }
    #[test]
    fn trapz_cases() {
        let e = Trapz::new().call(&json!({"y": {"dtype": "f64", "shape": [3], "data": [1.0, 2.0, 3.0]}, "dx": 0.5, "cumulative": true}), &HostCtx::default());
        assert_eq!(e["result"]["value"], 2.0, "{e}");
        assert_eq!(e["result"]["cumulative"]["data"], json!([0.0, 0.75, 2.0]));
        let e = Trapz::new().call(&json!({"y": {"dtype": "f64", "shape": [3], "data": [1.0, 2.0, 3.0]}}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
    }
}
