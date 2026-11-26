//! nonlinear.root.fzero (spec 10.26) and nonlinear.system.fsolve (spec 10.27), v0.4.13.
//!
//! fzero: Brent's method (zeroin, the algorithm of MATLAB fzero) on a sign-changing bracket. Author's
//! decision (2026-11-25): a bracket is preferred; x0 alone is accepted, the bracket is then searched
//! outward like MATLAB (dx = x0/50, ×√2 per step) and the searched bracket is echoed with a warning that
//! the root found need not be the one meant.
//! fsolve: Powell's dogleg trust region (MATLAB fsolve's default) with the exact Jacobian from forward-mode
//! automatic differentiation of the expressions (author's decision; spec 3.15 EX-8), Newton steps by the
//! shared LU kernel (spec 11.5 RI-1). Not converging is an error carrying the best point (author's
//! decision): E.NUMERICAL.NO_CONVERGENCE, or E.NUMERICAL.LOCAL_MINIMUM when ‖F‖ stalls above zero.
use crate::expr::{check_name, eval, eval_grad, expr_err, parse, parse_params, Node};
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::bracket::{zeroin, Stop, ZeroinErr};
use aeoi_core::kernels::dense::norm_inf;
use aeoi_core::kernels::newton::{aposteriori, dogleg};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Fzero, "nonlinear.root.fzero");
operator_struct!(Fsolve, "nonlinear.system.fsolve");

const U: f64 = f64::EPSILON / 2.0;
/// Jacobian condition above which the root is reported as possibly multiple (≈ u^(-1/2)).
pub const COND_WARN: f64 = 1e8;

fn non_finite(what: &str, x: f64, path: &str) -> OpError {
    OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("{what}在 {x} 处不是有限值（NaN 或 Inf）。"), "检查表达式在这一点是否有除零、log 或 sqrt 的负参数，或换一个区间/初值。").path(path).detail("at", json!(num(x)))
}

// ------------------------------------------------------------------------------------------------- fzero
pub struct InZ {
    expr: Node,
    vars: Vec<f64>, // [x, params…]
    bracket: Option<(f64, f64)>,
    x0: Option<f64>,
    xtol: f64,
    max_evals: usize,
}

struct Counter<'a> {
    i: &'a InZ,
    vars: Vec<f64>,
    evals: usize,
}
impl Counter<'_> {
    fn f(&mut self, x: f64) -> f64 {
        self.evals += 1;
        self.vars[0] = x;
        eval(&self.i.expr, &self.vars)
    }
}

impl Operator for Fzero {
    type Input = InZ;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InZ, Vec<Warning>), OpError> {
        let var = args.get("variable").and_then(|v| v.as_str()).unwrap_or("x").to_string();
        check_name(&var).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个变量名。").path("/variable").detail("kind", json!("bad_name")))?;
        let (pn, pv) = parse_params(&args["params"], std::slice::from_ref(&var))?;
        let mut names = vec![var];
        names.extend(pn);
        let src = args["expression"].as_str().unwrap();
        let expr = parse(src, &names).map_err(|e| expr_err("/expression", src, e))?;
        let has_b = args.get("bracket").is_some();
        let has_x0 = args.get("x0").is_some();
        if has_b == has_x0 {
            let mut e = OpError::new("E.VALIDATION.OPTION_CONFLICT", if has_b { "同时给出了 bracket 和 x0；只给一种。" } else { "没有给出搜索范围：要变号区间 bracket = [a, b]，或初值 x0。" }, "优先给变号区间：根唯一落在区间内；只给 x0 时算子向外搜索区间，找到的不一定是想要的根。").path("/");
            if has_b {
                e = e.remedy(modify_input("drop_x0", "删除 x0，只用 bracket", "high", false, json!([{"op": "remove", "path": "/x0"}])));
            } else {
                e = e.remedy(ask_user("bracket", "根大概在哪个区间？给出 f 在两端异号的 [a, b]。"));
            }
            return Err(e);
        }
        let bracket = if has_b {
            let b: Vec<f64> = args["bracket"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            if !(b[0] < b[1]) {
                return Err(OpError::new("E.VALIDATION.RANGE", format!("bracket = [{}, {}]：要求 a < b。", b[0], b[1]), "按升序给出区间两端。").path("/bracket").remedy(modify_input("swap", "交换两端", "high", false, json!([{"op": "replace", "path": "/bracket", "value": [b[1], b[0]]}]))));
            }
            Some((b[0], b[1]))
        } else {
            None
        };
        let lim = self.meta.manifest["limits"]["max_evaluations"].as_u64().unwrap() as usize;
        let mut vars = vec![0.0];
        vars.extend(pv);
        Ok((InZ { expr, vars, bracket, x0: args.get("x0").and_then(|v| v.as_f64()), xtol: args.get("xtol").and_then(|v| v.as_f64()).unwrap_or(0.0), max_evals: lim }, Vec::new()))
    }

    fn execute(&self, i: InZ, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut c = Counter { i: &i, vars: i.vars.clone(), evals: 0 };
        let mut warnings = Vec::new();
        let (mut a, mut b, mut fa, mut fb, source);
        if let Some((lo, hi)) = i.bracket {
            a = lo;
            b = hi;
            fa = c.f(a);
            if !fa.is_finite() {
                return Err(non_finite("f ", a, "/bracket/0"));
            }
            fb = c.f(b);
            if !fb.is_finite() {
                return Err(non_finite("f ", b, "/bracket/1"));
            }
            source = "given";
            if fa != 0.0 && fb != 0.0 && (fa > 0.0) == (fb > 0.0) {
                return Err(OpError::new("E.DOMAIN.NO_SIGN_CHANGE", format!("f 在区间两端同号：f({a}) = {fa:e}，f({b}) = {fb:e}。区间里可能没有根，也可能有偶数个根。"), "换一个 f 在两端异号的区间；不确定根在哪时先画出 f 或问用户。")
                    .path("/bracket")
                    .detail("f_a", json!(num(fa)))
                    .detail("f_b", json!(num(fb)))
                    .remedy(ask_user("bracket", "根大概在哪个区间？")));
            }
        } else {
            let x0 = i.x0.unwrap();
            let f0 = c.f(x0);
            if !f0.is_finite() {
                return Err(non_finite("f ", x0, "/x0"));
            }
            a = x0;
            b = x0;
            fa = f0;
            fb = f0;
            let mut dx = if x0 != 0.0 { x0.abs() / 50.0 } else { 1.0 / 50.0 };
            let s2 = 2f64.sqrt();
            // MATLAB fzero's outward search: alternately x0 − dx, x0 + dx, dx ×√2, until a sign change
            while fa != 0.0 && fb != 0.0 && (fa > 0.0) == (fb > 0.0) {
                dx *= s2;
                a = x0 - dx;
                fa = c.f(a);
                if !fa.is_finite() || !a.is_finite() || c.evals > i.max_evals {
                    return Err(OpError::new("E.DOMAIN.NO_SIGN_CHANGE", format!("从 x0 = {x0} 向外搜索到 [{a:e}, {b:e}] 仍未找到 f 的变号区间（{} 次求值）。", c.evals), "给出变号区间 bracket = [a, b]；f 可能没有实根，或根是偶重根（不变号）。").path("/x0").detail("searched", json!([num(a), num(b)])).remedy(ask_user("bracket", "根大概在哪个区间？")));
                }
                if fa == 0.0 || (fa > 0.0) != (fb > 0.0) {
                    break;
                }
                b = x0 + dx;
                fb = c.f(b);
                if !fb.is_finite() || !b.is_finite() {
                    return Err(OpError::new("E.DOMAIN.NO_SIGN_CHANGE", format!("从 x0 = {x0} 向外搜索时 f 变成非有限值（在 {b:e}），未找到变号区间。"), "给出变号区间 bracket = [a, b]。").path("/x0").remedy(ask_user("bracket", "根大概在哪个区间？")));
                }
            }
            if a > b {
                std::mem::swap(&mut a, &mut b);
                std::mem::swap(&mut fa, &mut fb);
            }
            source = "searched_from_x0";
            warnings.push(Warning::new("W.DOMAIN.BRACKET_SEARCHED", format!("只给了初值 x0 = {x0}，算子向外搜索到变号区间 [{a:e}, {b:e}]，求的是这个区间里的根。它不一定是离 x0 最近的根，也不一定是你想要的那个。"), ).path("/x0").details(json!({"bracket": [num(a), num(b)]})));
        }
        let (a0, b0, fa0, fb0) = (a, b, fa, fb);
        // Brent (zeroin), shared kernel
        let max_evals = i.max_evals;
        let z = zeroin(
            &mut |x| {
                let v = c.f(x);
                if !v.is_finite() {
                    Err(Stop::NonFinite(x))
                } else if c.evals > max_evals {
                    Err(Stop::Budget)
                } else {
                    Ok(v)
                }
            },
            a, b, fa, fb, i.xtol, &|| ctx.check(),
        );
        let (x, cc, iters) = match z {
            Ok(z) => (z.x, z.other, z.iters),
            Err(ZeroinErr { stop: Stop::NonFinite(at), .. }) => return Err(non_finite("f ", at, "/expression")),
            Err(ZeroinErr { stop: Stop::Budget, b, c: cc }) => {
                return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("{} 次求值后仍未收敛（当前区间 [{}, {}]）。", c.evals, b.min(cc), b.max(cc)), "放宽 xtol，或检查 f 是否在区间内剧烈振荡。").path("/expression").detail("best", json!(num(b))))
            }
            Err(ZeroinErr { stop: Stop::Abort(e), .. }) => return Err(e),
        };
        let (lo, hi) = (x.min(cc), x.max(cc));
        let (flo, fhi) = (c.f(lo), c.f(hi));
        let sign_change = flo == 0.0 || fhi == 0.0 || (flo > 0.0) != (fhi > 0.0);
        let fx = c.f(x);
        let not_pole = fx.abs() <= fa0.abs().max(fb0.abs());
        let result = json!({
            "x": num(x), "fx": num(fx),
            "bracket": {"initial": [num(a0), num(b0)], "source": source, "final": [num(lo), num(hi)]},
            "iterations": iters, "evaluations": c.evals,
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": "区间半宽 ≤ 2ε|x| + xtol/2（Brent/zeroin）"},
            "residual": {"fx": num(fx), "f_at_initial_bracket": [num(fa0), num(fb0)]},
            "condition": {"applicable": false},
            "error_estimate": {"x_abs": num((hi - lo) / 2.0), "basis": "根被夹在最终区间 [lo, hi] 内（f 连续时严格成立）"},
            "checks": {"sign_change": sign_change, "not_a_pole": not_pole, "all_finite": x.is_finite() && fx.is_finite()},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"not_a_pole_definition": "|f(x)| ≤ max(|f(a₀)|, |f(b₀)|)：变号点若是极点（如 1/x 在 0 处），|f| 在那里很大"}
        });
        Ok(Output { result, trust, warnings })
    }
}

// ------------------------------------------------------------------------------------------------ fsolve
pub struct InS {
    unknowns: Vec<String>,
    eqs: Vec<Node>,
    vars0: Vec<f64>, // [unknowns…, params…]
    max_iter: usize,
}

fn eval_fj(eqs: &[Node], vars: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let seed: Vec<Option<usize>> = (0..vars.len()).map(|k| if k < n { Some(k) } else { None }).collect();
    let mut f = Vec::with_capacity(n);
    let mut j = vec![0.0; n * n];
    for (r, e) in eqs.iter().enumerate() {
        let (v, g) = eval_grad(e, vars, &seed, n);
        f.push(v);
        j[r * n..(r + 1) * n].copy_from_slice(&g);
    }
    (f, j)
}
impl Operator for Fsolve {
    type Input = InS;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InS, Vec<Warning>), OpError> {
        let unknowns: Vec<String> = args["unknowns"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let lim = self.meta.manifest["limits"]["max_unknowns"].as_u64().unwrap() as usize;
        if unknowns.len() > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{} 个未知量超过上限 {lim}。", unknowns.len()), "减少未知量。").path("/unknowns"));
        }
        for (k, u) in unknowns.iter().enumerate() {
            check_name(u).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个名字。").path(&format!("/unknowns/{k}")).detail("kind", json!("bad_name")))?;
            if unknowns[..k].contains(u) {
                return Err(OpError::new("E.VALIDATION.EXPRESSION", format!("未知量 '{u}' 重复。"), "每个未知量只写一次。").path(&format!("/unknowns/{k}")).detail("kind", json!("duplicate_name")));
            }
        }
        let n = unknowns.len();
        let srcs: Vec<String> = args["equations"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        if srcs.len() != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("有 {} 个方程、{n} 个未知量；0.4.0 只解方程个数等于未知量个数的方程组。", srcs.len()), "补齐或删减方程；超定/欠定问题用最小二乘（lsqnonlin，以后的算子）。").path("/equations").detail("expected_length", json!(n)));
        }
        let x0: Vec<f64> = args["x0"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        if x0.len() != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("x0 有 {} 个值、{n} 个未知量。", x0.len()), "x0 按 unknowns 的顺序给出每个未知量的初值。").path("/x0").detail("expected_length", json!(n)));
        }
        let (pn, pv) = parse_params(&args["params"], &unknowns)?;
        let mut names = unknowns.clone();
        names.extend(pn);
        let mut eqs = Vec::with_capacity(n);
        for (k, s) in srcs.iter().enumerate() {
            eqs.push(parse(s, &names).map_err(|e| expr_err(&format!("/equations/{k}"), s, e))?);
        }
        let mut vars0 = x0;
        vars0.extend(pv);
        let max_iter = self.meta.manifest["limits"]["max_iterations"].as_u64().unwrap() as usize;
        Ok((InS { unknowns, eqs, vars0, max_iter }, Vec::new()))
    }

    fn execute(&self, i: InS, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.unknowns.len();
        let mut vars = i.vars0.clone();
        let (f, j) = eval_fj(&i.eqs, &vars, n);
        if f.iter().chain(j.iter()).any(|v| !v.is_finite()) {
            let k = f.iter().position(|v| !v.is_finite()).unwrap_or(0);
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("方程（或其导数）在 x0 处不是有限值（第 {k} 个方程附近）。"), "检查方程在初值处是否有除零、log 或 sqrt 的负参数；换一个初值。").path("/x0"));
        }
        let params = vars[n..].to_vec();
        let eqs = &i.eqs;
        let dl = dogleg(
            &mut |x: &[f64]| {
                let mut v = x.to_vec();
                v.extend_from_slice(&params);
                eval_fj(eqs, &v, n)
            },
            vars[..n].to_vec(), f, j, i.max_iter, &|| ctx.check(),
        )?;
        let (iters, converged, stalled, newton_norms, evals) = (dl.iters, dl.converged, dl.stalled, dl.newton_norms, 1 + dl.evals);
        vars[..n].copy_from_slice(&dl.x);
        let (f, j) = (dl.f, dl.j);
        let x: Vec<f64> = vars[..n].to_vec();
        let fnorm = norm_inf(&f);
        let best = json!({"x": x.iter().map(|v| num(*v)).collect::<Vec<_>>(), "residual_inf": num(fnorm), "iterations": iters});
        if !converged {
            let (code, why) = if stalled { ("E.NUMERICAL.LOCAL_MINIMUM", format!("迭代停在 ‖F‖∞ = {fnorm:.3e} 的点上，无法再下降：这是残差 ½‖F‖² 的局部极小（或雅可比奇异），不是方程组的根。")) } else { ("E.NUMERICAL.NO_CONVERGENCE", format!("{iters} 次迭代后仍未收敛，当前 ‖F‖∞ = {fnorm:.3e}。")) };
            return Err(OpError::new(code, why, "换一个初值 x0（details.best 是目前最好的点）；或检查方程组是否真的有实根。").path("/x0").detail("best", best).remedy(ask_user("better_x0", "能否给一个更接近解的初值，或说明解大概在什么范围？")));
        }
        // a posteriori: next Newton correction and Jacobian condition at the solution
        let (delta_next, kappa) = aposteriori(&f, &j, n);
        let mut warnings = Vec::new();
        // linear convergence of the last Newton steps (ratio ≥ 1/4 three times) also signals a singular Jacobian
        // at the root — the only signal for n = 1, where κ(J) is always 1
        let k = newton_norms.len();
        let linear = k >= 4 && (k - 3..k).all(|t| newton_norms[t - 1] > 0.0 && newton_norms[t] / newton_norms[t - 1] >= 0.25);
        if !(kappa <= COND_WARN) || linear {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("解处雅可比矩阵接近奇异（κ₁ ≈ {kappa:.3e}{}）：可能是重根或非孤立解。牛顿法在这里只线性收敛，解可能只有约一半有效数字。", if linear { "，最后几步牛顿迭代只线性收敛" } else { "" })).path("/equations").details(json!({"jacobian_kappa1": num(kappa), "threshold": COND_WARN, "linear_convergence": linear})));
        }
        let small = delta_next <= 1e3 * U * norm_inf(&x).max(f64::MIN_POSITIVE) || fnorm == 0.0;
        let finite = x.iter().chain(f.iter()).all(|v| v.is_finite());
        let solution: serde_json::Map<String, Value> = i.unknowns.iter().zip(&x).map(|(k, v)| (k.clone(), num(*v))).collect();
        let result = json!({
            "x": x.iter().map(|v| num(*v)).collect::<Vec<_>>(),
            "solution": solution,
            "residual": f.iter().map(|v| num(*v)).collect::<Vec<_>>(),
            "iterations": iters, "evaluations": evals,
            "jacobian": "automatic_differentiation",
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": "完整牛顿步 ‖p‖∞ ≤ 4u‖x‖∞，或 F = 0"},
            "residual": {"residual_inf": num(fnorm), "next_newton_correction_inf": num(delta_next)},
            "condition": {"jacobian_kappa1": num(kappa), "estimator": "Hager–Higham 1-范数估计（共享内核）"},
            "error_estimate": {"x_abs_inf": num(delta_next), "basis": "解处再做一步牛顿修正的大小（单根时与真实误差同阶；重根时偏小）"},
            "checks": {"newton_step_small": small, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"method": "Powell dogleg 信赖域 + 自动微分雅可比 + LU（共享内核）"}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    #[test]
    fn fzero_bracket_and_search() {
        let e = Fzero::new().call(&json!({"expression": "x^2 - 2", "bracket": [0.0, 2.0]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert!((e["result"]["x"].as_f64().unwrap() - 2f64.sqrt()).abs() <= 4e-16, "{e}");
        let e = Fzero::new().call(&json!({"expression": "cos(x) - x", "x0": 1.0}), &HostCtx::default());
        assert_eq!(e["status"], "ok_with_warnings", "{e}");
        assert_eq!(e["warnings"][0]["code"], "W.DOMAIN.BRACKET_SEARCHED");
        let e = Fzero::new().call(&json!({"expression": "x^2 + 1", "bracket": [-1.0, 1.0]}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.NO_SIGN_CHANGE");
        let e = Fzero::new().call(&json!({"expression": "tan(x)", "bracket": [1.0, 2.0]}), &HostCtx::default());
        assert_eq!(e["trust"]["checks"]["not_a_pole"], false, "{e}");
    }
    #[test]
    fn fsolve_circle_line_and_failures() {
        let e = Fsolve::new().call(&json!({"unknowns": ["x", "y"], "equations": ["x^2 + y^2 - 4", "x - y"], "x0": [1.0, 0.5]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let x = e["result"]["x"][0].as_f64().unwrap();
        assert!((x - 2f64.sqrt()).abs() < 1e-15, "{e}");
        let e = Fsolve::new().call(&json!({"unknowns": ["x"], "equations": ["x^2 + 1"], "x0": [0.5]}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.NUMERICAL.LOCAL_MINIMUM", "{e}");
        // double root: Newton converges only linearly → warned as a possibly multiple root
        let e = Fsolve::new().call(&json!({"unknowns": ["x"], "equations": ["(x - 1)^2"], "x0": [3.0]}), &HostCtx::default());
        assert_eq!(e["warnings"][0]["code"], "W.NUMERICAL.ILL_CONDITIONED", "{e}");
    }
}
