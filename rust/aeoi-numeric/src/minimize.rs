//! optim.quadratic.quadprog (spec 10.45) and optim.scalar.fminbnd (spec 10.46), v0.4.26.
//!
//! quadprog wraps the QP kernel that fmincon's SQP subproblems use (RI-1). Author's decisions (2026-11-26):
//! H must be positive definite (strictly convex: unique, global optimum), checked with the Cholesky kernel;
//! a non-symmetric H is replaced by (H + Hᵀ)/2 — the objective is unchanged — with W.VALIDATION.SYMMETRIZED.
//! fminbnd is Brent's minimisation (kernels::bracket::fmin, MATLAB fminbnd's algorithm) with MATLAB's default
//! tolerances; afterwards f is probed at both end points and on 101 equally spaced points, and a lower point
//! more than one grid step away from x is reported with W.ACCURACY.BETTER_POINT_FOUND (author's decision).
use crate::expr::{check_name, eval, expr_err, parse, parse_params, Node};
use crate::optim::{bounds, infeasible, linear, rows, vecf};
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::bracket::{fmin, FminErr, Stop};
use aeoi_core::kernels::dense::cholesky_detail;
use aeoi_core::kernels::qp;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Quadprog, "optim.quadratic.quadprog");
operator_struct!(Fminbnd, "optim.scalar.fminbnd");

const U: f64 = f64::EPSILON / 2.0;

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn vj(v: &[f64]) -> Value {
    json!(v.iter().map(|x| num(*x)).collect::<Vec<_>>())
}

// ============================================================================================ quadprog
pub struct InQp {
    h: Vec<f64>, // row-major n×n, symmetrised
    f: Vec<f64>,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    aeq: Vec<Vec<f64>>,
    beq: Vec<f64>,
    lb: Vec<f64>,
    ub: Vec<f64>,
    symmetrized: bool,
}

impl Operator for Quadprog {
    type Input = InQp;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InQp, Vec<Warning>), OpError> {
        let f = vecf(&args["f"], "/f", None)?;
        let n = f.len();
        if n == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "f 是空的：没有变量。", "给出 n ≥ 1 个变量的一次项系数。").path("/f"));
        }
        let lim = self.meta.manifest["limits"]["max_variables"].as_u64().unwrap() as usize;
        if n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{n} 个变量超过上限 {lim}（稠密有效集法）。"), "大规模问题需要稀疏 / 内点法（以后的方法）。").path("/f"));
        }
        if f.iter().any(|v| !v.is_finite()) {
            return Err(OpError::new("E.VALIDATION.NON_FINITE", "f 含非有限值。", "目标系数必须有限。").path("/f"));
        }
        let hr = rows(&args["H"], "/H", n)?;
        if hr.len() != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("H 有 {} 行，f 有 {n} 个元素；H 应为 {n}×{n}。", hr.len()), "H 每行一个数组，共 n 行。").path("/H").detail("expected_rows", json!(n)));
        }
        let mut h: Vec<f64> = hr.into_iter().flatten().collect();
        let hmax = h.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let asym = (0..n).flat_map(|r| (r + 1..n).map(move |c| (r, c))).map(|(r, c)| (h[r * n + c] - h[c * n + r]).abs()).fold(0.0, f64::max);
        let mut warnings = Vec::new();
        let symmetrized = asym > 0.0;
        if symmetrized {
            let hs: Vec<f64> = (0..n * n).map(|t| 0.5 * (h[t] + h[(t % n) * n + t / n])).collect();
            h = hs;
            if asym > 10.0 * n as f64 * U * hmax {
                warnings.push(Warning::new("W.VALIDATION.SYMMETRIZED", format!("H 不对称（max|h_ij − h_ji| = {asym:.3e}）：按 (H + Hᵀ)/2 求解。½xᵀHx 只取决于对称部分，所以问题不变；但不对称常常说明 H 录错了。")).path("/H").details(json!({"max_asymmetry": num(asym)})));
            }
        }
        if let Err((j, d)) = cholesky_detail(&h, n) {
            return Err(OpError::new("E.DOMAIN.NOT_POSITIVE_DEFINITE", format!("H（对称部分）不是正定的：Cholesky 第 {j} 列的主元 d = {d:.3e} ≤ 0。本算子只解严格凸的二次规划；半正定或不定的 H 可能没有唯一解或没有下界。"), "检查 H 的符号与录入（求最大值时目标要取负）；H = 0 时用 optim.linear.linprog；非凸问题需要别的方法。")
                .path("/H")
                .detail("failed_column", json!(j))
                .detail("pivot", num(d))
                .remedy(ask_user("convexity", "目标应当是严格凸的吗？H 是否录错了符号，或者问题本来就是线性/非凸的？")));
        }
        let (a, b, aeq, beq) = linear(args, n)?;
        let (lb, ub) = bounds(args, n)?;
        Ok((InQp { h, f, a, b, aeq, beq, lb, ub, symmetrized }, warnings))
    }
    fn execute(&self, i: InQp, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.f.len();
        // finite bounds become inequality rows: −x_j ≤ −lb_j, x_j ≤ ub_j
        let mut a = i.a.clone();
        let mut b = i.b.clone();
        let mut lrow = vec![None; n];
        let mut urow = vec![None; n];
        for j in 0..n {
            if i.lb[j].is_finite() {
                let mut r = vec![0.0; n];
                r[j] = -1.0;
                lrow[j] = Some(a.len());
                a.push(r);
                b.push(-i.lb[j]);
            }
            if i.ub[j].is_finite() {
                let mut r = vec![0.0; n];
                r[j] = 1.0;
                urow[j] = Some(a.len());
                a.push(r);
                b.push(i.ub[j]);
            }
        }
        let max_it = self.meta.manifest["limits"]["max_iterations"].as_u64().unwrap() as usize;
        let s = match qp::solve(&i.h, &i.f, &a, &b, &i.aeq, &i.beq, max_it.min(50 * (n + a.len() + i.aeq.len()).max(10))) {
            Ok(s) => s,
            Err(qp::QpError::Infeasible) => return Err(infeasible("约束没有可行解（线性规划第一阶段找不到可行点）。".into())),
            Err(qp::QpError::IterationLimit) => return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", "有效集迭代达到上限。", "问题可能高度退化（许多约束在最优点同时起作用且线性相关）；删去多余的约束后重试。").path("/")),
        };
        ctx.check()?;
        let x = s.p;
        let m0 = i.a.len();
        let ineqlin: Vec<f64> = s.lam_ineq[..m0].to_vec();
        let lower: Vec<f64> = (0..n).map(|j| lrow[j].map(|r| s.lam_ineq[r]).unwrap_or(0.0)).collect();
        let upper: Vec<f64> = (0..n).map(|j| urow[j].map(|r| s.lam_ineq[r]).unwrap_or(0.0)).collect();
        let eqlin = s.lam_eq.clone();
        // KKT residuals relative to the data scale (as linprog LP-4)
        let hx: Vec<f64> = (0..n).map(|r| dot(&i.h[r * n..(r + 1) * n], &x)).collect();
        let grad: Vec<f64> = (0..n).map(|r| hx[r] + i.f[r]).collect();
        let mut st = grad.clone();
        for (r, row) in i.a.iter().enumerate() {
            for j in 0..n {
                st[j] += row[j] * ineqlin[r];
            }
        }
        for (r, row) in i.aeq.iter().enumerate() {
            for j in 0..n {
                st[j] += row[j] * eqlin[r];
            }
        }
        for j in 0..n {
            st[j] += upper[j] - lower[j];
        }
        let gscale = grad.iter().chain(&i.f).fold(1.0f64, |m, v| m.max(v.abs()));
        let stat = st.iter().fold(0.0f64, |m, v| m.max(v.abs())) / gscale;
        let mut primal = 0.0f64;
        let mut comp = 0.0f64;
        for (r, row) in i.a.iter().enumerate() {
            let sl = i.b[r] - dot(row, &x);
            let sc = i.b[r].abs().max(1.0);
            primal = primal.max((-sl).max(0.0) / sc);
            comp = comp.max((ineqlin[r] * sl).abs() / (sc * gscale));
        }
        for (r, row) in i.aeq.iter().enumerate() {
            primal = primal.max((dot(row, &x) - i.beq[r]).abs() / i.beq[r].abs().max(1.0));
        }
        for j in 0..n {
            if i.lb[j].is_finite() {
                let sl = x[j] - i.lb[j];
                primal = primal.max((-sl).max(0.0) / i.lb[j].abs().max(1.0));
                comp = comp.max((lower[j] * sl).abs() / (i.lb[j].abs().max(1.0) * gscale));
            }
            if i.ub[j].is_finite() {
                let sl = i.ub[j] - x[j];
                primal = primal.max((-sl).max(0.0) / i.ub[j].abs().max(1.0));
                comp = comp.max((upper[j] * sl).abs() / (i.ub[j].abs().max(1.0) * gscale));
            }
        }
        let dual = ineqlin.iter().chain(&lower).chain(&upper).fold(0.0f64, |m, v| m.max((-v).max(0.0))) / gscale;
        let tol = 1e-9;
        let fval = 0.5 * dot(&x, &hx) + dot(&i.f, &x);
        let finite = x.iter().all(|v| v.is_finite()) && fval.is_finite();
        let result = json!({
            "x": vj(&x), "fval": num(fval),
            "lambda": {"ineqlin": vj(&ineqlin), "eqlin": vj(&eqlin), "lower": vj(&lower), "upper": vj(&upper)},
            "iterations": s.iterations, "bounds_used": {"lb": vj(&i.lb), "ub": vj(&i.ub)}, "symmetrized": i.symmetrized,
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": s.iterations},
            "residual": {"primal_infeasibility": num(primal), "dual_infeasibility": num(dual), "complementarity": num(comp), "stationarity": num(stat), "threshold": tol,
                "definition": "相对数据尺度的 KKT 残差：Hx + f + Aᵀλ_ineqlin + Aeqᵀλ_eqlin − λ_lower + λ_upper = 0，λ ≥ 0，λ·松弛 = 0；驻定与互补以 max(1, ‖Hx + f‖∞, ‖f‖∞) 归一，原始可行以 max(1, |右端|) 归一"},
            "condition": {"applicable": false},
            "error_estimate": {"applicable": false, "note": "H 正定：最优解唯一且是全局最优；KKT 残差就是验证"},
            "checks": {"kkt": primal <= tol && dual <= tol && comp <= tol && stat <= tol, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"optimum": "global", "method": "原始有效集法；可行起点由 LP 内核给出，KKT 系统用 LU 内核，工作集独立性用 QR 内核"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================= fminbnd
pub struct InFb {
    expr: Node,
    vars: Vec<f64>,
    lo: f64,
    hi: f64,
    tol_x: f64,
    max_evals: usize,
}

impl Operator for Fminbnd {
    type Input = InFb;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InFb, Vec<Warning>), OpError> {
        let var = args.get("variable").and_then(|v| v.as_str()).unwrap_or("x").to_string();
        check_name(&var).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个变量名。").path("/variable").detail("kind", json!("bad_name")))?;
        let (pn, pv) = parse_params(&args["params"], std::slice::from_ref(&var))?;
        let mut names = vec![var];
        names.extend(pn);
        let src = args["expression"].as_str().unwrap();
        let expr = parse(src, &names).map_err(|e| expr_err("/expression", src, e))?;
        let iv: Vec<f64> = args["interval"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        if !(iv[0] < iv[1]) || !iv[0].is_finite() || !iv[1].is_finite() {
            let mut e = OpError::new("E.VALIDATION.RANGE", format!("interval = [{}, {}]：要求有限的 x1 < x2。", iv[0], iv[1]), "按升序给出区间两端。").path("/interval");
            if iv[0] > iv[1] {
                e = e.remedy(modify_input("swap", "交换两端", "high", false, json!([{"op": "replace", "path": "/interval", "value": [iv[1], iv[0]]}])));
            } else {
                e = e.remedy(ask_user("interval", "要在哪个区间上求极小？"));
            }
            return Err(e);
        }
        let mut vars = vec![0.0];
        vars.extend(pv);
        let tol_x = args.get("tol_x").and_then(|v| v.as_f64()).unwrap_or(1e-4);
        let max_evals = args.get("max_evaluations").and_then(|v| v.as_u64()).unwrap_or(500) as usize;
        Ok((InFb { expr, vars, lo: iv[0], hi: iv[1], tol_x, max_evals }, Vec::new()))
    }
    fn execute(&self, i: InFb, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut vars = i.vars.clone();
        let mut evals = 0usize;
        let mut f = |x: f64| -> f64 {
            vars[0] = x;
            eval(&i.expr, &vars)
        };
        let max_evals = i.max_evals;
        let res = fmin(
            &mut |x| {
                evals += 1;
                if evals > max_evals {
                    return Err(Stop::Budget);
                }
                let v = f(x);
                if v.is_finite() {
                    Ok(v)
                } else {
                    Err(Stop::NonFinite(x))
                }
            },
            i.lo, i.hi, i.tol_x, &|| ctx.check(),
        );
        let r = match res {
            Ok(r) => r,
            Err(FminErr { stop: Stop::NonFinite(at), .. }) => {
                return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("f 在 {at} 处不是有限值（NaN 或 Inf）。"), "检查表达式在区间内是否有除零、log 或 sqrt 的负参数；缩小区间。").path("/expression").detail("at", json!(num(at))))
            }
            Err(FminErr { stop: Stop::Budget, x, fx }) => {
                return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("{max_evals} 次求值后仍未收敛（目前最好的点 x = {x}，f = {fx:e}）。"), "放宽 tol_x，或增加 max_evaluations；f 可能在区间内剧烈振荡。").path("/max_evaluations").detail("best", json!({"x": num(x), "f": num(fx)})))
            }
            Err(FminErr { stop: Stop::Abort(e), .. }) => return Err(e),
        };
        let used = evals;
        // local probe (as fminsearch NM-4): f(x) ≤ f(x ± h), h = 3·tol1; a side outside [x1, x2] is skipped
        // (a minimum at an end point is then within h of x, which is the accuracy Brent gives there)
        let tol1 = f64::EPSILON.sqrt() * r.x.abs() + i.tol_x / 3.0;
        let h = 3.0 * tol1;
        let (xl, xr) = (r.x - h, r.x + h);
        let fl = if xl >= i.lo { f(xl) } else { f64::NAN };
        let fr = if xr <= i.hi { f(xr) } else { f64::NAN };
        let local_ok = (!fl.is_finite() || r.fx <= fl) && (!fr.is_finite() || r.fx <= fr);
        // global probe: the two end points and 101 equally spaced points (k = 0 … 100, end points included)
        let step = (i.hi - i.lo) / 100.0;
        let mut best = (f64::NAN, f64::INFINITY);
        let mut better: Option<(f64, f64)> = None;
        for k in 0..=100 {
            let p = if k == 100 { i.hi } else { i.lo + k as f64 * step };
            let fp = f(p);
            if !fp.is_finite() {
                continue;
            }
            if fp < best.1 {
                best = (p, fp);
            }
            if fp < r.fx && (p - r.x).abs() > step && better.map(|b| fp < b.1).unwrap_or(true) {
                better = Some((p, fp));
            }
        }
        let mut warnings = Vec::new();
        if let Some((p, fp)) = better {
            warnings.push(Warning::new("W.ACCURACY.BETTER_POINT_FOUND", format!("Brent 收敛到 x = {:.6e}（f = {:.6e}），但探测点 x = {p:.6e} 上 f = {fp:.6e} 更低：返回的只是局部极小，区间里还有更低的谷。", r.x, r.fx))
                .path("/interval")
                .details(json!({"x": num(p), "f": num(fp), "returned_x": num(r.x), "returned_f": num(r.fx)})));
        }
        let result = json!({
            "x": num(r.x), "fval": num(r.fx), "interval": [num(i.lo), num(i.hi)], "final_bracket": [num(r.a), num(r.b)],
            "iterations": r.iters, "evaluations": used, "tol_x": num(i.tol_x), "max_evaluations": i.max_evals,
            "best_probe": {"x": num(best.0), "f": num(best.1)},
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": r.iters, "criterion": "|x − (a + b)/2| ≤ tol2 − (b − a)/2，tol2 = 2(√ε·|x| + tol_x/3)"},
            "residual": {"local_probe": {"h": num(h), "f_left": num(fl), "f_right": num(fr)}, "probe_points": 101, "probe_step": num(step)},
            "condition": {"applicable": false},
            "error_estimate": {"x_abs": num(0.5 * (r.b - r.a)), "basis": "f 在最终区间上单峰时，极小点在 [a, b] 内（非严格：f 不单峰时不成立）"},
            "checks": {"probe_local_minimum": local_ok, "grid_probe": better.is_none(), "all_finite": r.x.is_finite() && r.fx.is_finite()},
            "accuracy_grade": "C",
            "credentials": Value::Null,
            "derived": {"optimum": "local", "probe_extra_evaluations": 103}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    #[test]
    fn quadprog_basic() {
        // min ½(x² + y²) − x − y  s.t.  x + y ≤ 1  → x = y = ½, λ = ½
        let e = Quadprog::new().call(&json!({"H": [[1, 0], [0, 1]], "f": [-1, -1], "A": [[1, 1]], "b": [1]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let x: Vec<f64> = e["result"]["x"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((x[0] - 0.5).abs() < 1e-14 && (x[1] - 0.5).abs() < 1e-14, "{x:?}");
        assert!((e["result"]["lambda"]["ineqlin"][0].as_f64().unwrap() - 0.5).abs() < 1e-14);
        let e = Quadprog::new().call(&json!({"H": [[1, 2], [2, 1]], "f": [0, 0]}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.NOT_POSITIVE_DEFINITE");
        let e = Quadprog::new().call(&json!({"H": [[2, 1], [0, 2]], "f": [1, 1], "lb": [0, 0]}), &HostCtx::default());
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.VALIDATION.SYMMETRIZED"), "{e}");
    }
    #[test]
    fn fminbnd_basic() {
        let e = Fminbnd::new().call(&json!({"expression": "cos(x)", "interval": [3, 4]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert!((e["result"]["x"].as_f64().unwrap() - std::f64::consts::PI).abs() < 1e-4);
        // two valleys: Brent finds the local one near the start, the probe reports the deeper one
        // x⁴ − 8x² + 3x on [−2.5, 3]: Brent lands in the shallow valley at x ≈ 1.90; the probe finds −2.09
        let e = Fminbnd::new().call(&json!({"expression": "x^4 - 8*x^2 + 3*x", "interval": [-2.5, 3]}), &HostCtx::default());
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.ACCURACY.BETTER_POINT_FOUND"), "{e}");
        assert_eq!(e["trust"]["checks"]["grid_probe"], false);
    }
}
