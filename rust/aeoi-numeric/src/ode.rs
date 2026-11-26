//! ode.ivp.solve — explicit Runge–Kutta Dormand–Prince 5(4) for dy/dt = f(t, y) (spec 10.18, v0.4.8).
//!
//! The right-hand side is a list of AEOI expressions (spec 3.15) over named states and parameters. The
//! integrator follows the published formulas of scipy.integrate.RK45 term for term (tableau, RMS error norm,
//! step-size factors 0.9 / 0.2 / 10, Hairer's initial step, Shampine's 4th-order dense output, output points
//! in (t_old, t] evaluated by that step's interpolant), so an independent implementation can follow the same
//! step sequence up to rounding. Global error is estimated by re-solving with rtol/10, atol/10.
use crate::expr::{check_ode_name as check_name, eval, expr_err, parse, Node};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::kernels::rk::{dopri5, Fail};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Map, Value};

operator_struct!(OdeIvp, "ode.ivp.solve");

pub struct Model {
    rhs: Vec<Node>,
    n: usize,
    params: Vec<f64>,
}

impl Model {
    /// f(t, y) into `out`; false if any component is not finite.
    fn f(&self, t: f64, y: &[f64], out: &mut [f64], vars: &mut Vec<f64>) -> bool {
        vars.clear();
        vars.push(t);
        vars.extend_from_slice(y);
        vars.extend_from_slice(&self.params);
        let mut ok = true;
        for (i, e) in self.rhs.iter().enumerate() {
            out[i] = eval(e, vars);
            ok &= out[i].is_finite();
        }
        ok
    }
}

pub struct In {
    model: Model,
    states: Vec<String>,
    y0: Vec<f64>,
    t0: f64,
    tf: f64,
    t_eval: Vec<f64>,
    rtol: f64,
    atol: f64,
    max_step: f64,
    first_step: Option<f64>,
}

fn obj_keys(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

impl Operator for OdeIvp {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let states: Vec<String> = args["states"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let rhs_src: Vec<String> = args["rhs"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let lim = |k: &str| self.meta.manifest["limits"][k].as_u64().unwrap() as usize;
        if states.len() > lim("max_states") {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{} 个状态超过上限 {}。", states.len(), lim("max_states")), "减少状态数。").path("/states"));
        }
        for (i, s) in states.iter().enumerate() {
            check_name(s).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个名字。").path(&format!("/states/{i}")).detail("kind", json!("bad_name")))?;
        }
        if rhs_src.len() != states.len() {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("rhs 有 {} 个表达式，states 有 {} 个状态；第 i 个表达式是 d(states[i])/dt，两者必须一一对应。", rhs_src.len(), states.len()), "让 rhs 与 states 等长。").path("/rhs").detail("expected_length", json!(states.len())));
        }
        let params = obj_keys(&args["params"]);
        let mut pnames = Vec::new();
        let mut pvals = Vec::new();
        for (k, v) in &params {
            check_name(k).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个名字。").path(&format!("/params/{k}")).detail("kind", json!("bad_name")))?;
            if states.contains(k) {
                return Err(OpError::new("E.VALIDATION.EXPRESSION", format!("'{k}' 既是状态名又是参数名，表达式里无法区分。"), "给参数换一个名字。").path(&format!("/params/{k}")).detail("kind", json!("duplicate_name")));
            }
            pnames.push(k.clone());
            pvals.push(v.as_f64().unwrap());
        }
        let mut names = vec!["t".to_string()];
        names.extend(states.iter().cloned());
        names.extend(pnames.iter().cloned());
        let mut rhs = Vec::with_capacity(rhs_src.len());
        for (i, src) in rhs_src.iter().enumerate() {
            rhs.push(parse(src, &names).map_err(|e| expr_err(&format!("/rhs/{i}"), src, e))?);
        }
        // initial: exactly the states
        let init = obj_keys(&args["initial"]);
        let missing: Vec<&String> = states.iter().filter(|s| !init.contains_key(*s)).collect();
        let extra: Vec<&String> = init.keys().filter(|k| !states.contains(k)).collect();
        if !missing.is_empty() || !extra.is_empty() {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("initial 的键必须正好是 states：缺少 {missing:?}，多出 {extra:?}。"), "为每个状态给一个初值，键用状态名。").path("/initial").detail("missing", json!(missing)).detail("unknown", json!(extra)));
        }
        let y0: Vec<f64> = states.iter().map(|s| init[s].as_f64().unwrap()).collect();
        let ts = args["t_span"].as_array().unwrap();
        let (t0, tf) = (ts[0].as_f64().unwrap(), ts[1].as_f64().unwrap());
        if t0 == tf {
            return Err(OpError::new("E.VALIDATION.RANGE", "t_span 的两端相同，没有要积分的区间。", "给出 [t0, tf]，tf ≠ t0。").path("/t_span"));
        }
        let dir = if tf > t0 { 1.0 } else { -1.0 };
        let has_te = args.get("t_eval").is_some_and(|v| !v.is_null());
        let has_n = args.get("n_out").is_some_and(|v| !v.is_null());
        if has_te && has_n {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", "t_eval 与 n_out 都给了；输出时刻只能用一种方式指定（F-8）。", "去掉 n_out（用 t_eval），或去掉 t_eval（用等距的 n_out 个点）。")
                .path("/n_out").detail("conflicting_fields", json!(["t_eval", "n_out"]))
                .remedy(modify_input("drop_n_out", "去掉 n_out", "high", false, json!([{"op": "remove", "path": "/n_out"}]))));
        }
        let t_eval: Vec<f64> = if has_te {
            let te: Vec<f64> = args["t_eval"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            let (lo, hi) = (t0.min(tf), t0.max(tf));
            if let Some((i, v)) = te.iter().enumerate().find(|(_, v)| **v < lo || **v > hi) {
                return Err(OpError::new("E.VALIDATION.RANGE", format!("t_eval[{i}] = {v} 不在 t_span [{t0}, {tf}] 内。"), "只请求积分区间内的输出时刻。").path(&format!("/t_eval/{i}")));
            }
            if let Some(i) = (1..te.len()).find(|&i| dir * (te[i] - te[i - 1]) <= 0.0) {
                return Err(OpError::new("E.VALIDATION.GRID", format!("t_eval 必须沿积分方向严格单调（{}），在第 {i} 个元素处不是。", if dir > 0.0 { "递增" } else { "递减" }), "排序并去重。").path(&format!("/t_eval/{i}")));
            }
            te
        } else {
            let n = args.get("n_out").and_then(|v| v.as_u64()).unwrap_or(101) as usize;
            let step = (tf - t0) / (n - 1) as f64; // numpy.linspace
            let mut v: Vec<f64> = (0..n).map(|k| k as f64 * step + t0).collect();
            v[n - 1] = tf;
            v
        };
        let rtol = args.get("rtol").and_then(|v| v.as_f64()).unwrap_or(1e-3);
        let atol = args.get("atol").and_then(|v| v.as_f64()).unwrap_or(1e-6);
        let floor = 100.0 * f64::EPSILON;
        if rtol < floor {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("rtol = {rtol:e} 小于 100·ε = {floor:e}：在 f64 里达不到，结果会被舍入误差主导。不能静默放宽。"), "把 rtol 设为至少 100·ε（约 2.2e-14）。")
                .path("/rtol").remedy(modify_input("rtol_floor", "rtol 设为 100·ε", "high", false, json!([{"op": "replace", "path": "/rtol", "value": floor}]))));
        }
        let first_step = args.get("first_step").and_then(|v| v.as_f64());
        if let Some(h) = first_step {
            if h > (tf - t0).abs() {
                return Err(OpError::new("E.VALIDATION.RANGE", format!("first_step = {h} 大于积分区间长度 {}。", (tf - t0).abs()), "调小 first_step 或不给。").path("/first_step"));
            }
        }
        let max_step = args.get("max_step").and_then(|v| v.as_f64()).unwrap_or(f64::INFINITY);
        if states.len() * t_eval.len() > lim("max_output_values") || t_eval.len() > lim("max_output_points") {
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("{} 个状态 × {} 个输出点超过单次返回上限。", states.len(), t_eval.len()), "减少输出点（t_eval / n_out）。").path(if has_te { "/t_eval" } else { "/n_out" }));
        }
        let n = states.len();
        Ok((In { model: Model { rhs, n, params: pvals }, states, y0, t0, tf, t_eval, rtol, atol, max_step, first_step }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let max_steps = self.meta.manifest["limits"]["max_steps"].as_u64().unwrap() as usize;
        let refine = self.meta.manifest["limits"]["global_error_refinement"].as_f64().unwrap();
        let cfac = self.meta.manifest["limits"]["tolerance_consistency_factor"].as_f64().unwrap();
        let fail = |f: Fail, what: &str| -> OpError {
            match f {
                Fail::NonFiniteAtStart(t) => OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", format!("右端在初始点 t = {t} 处不是有限值（NaN 或 Inf）。"), "检查 rhs 在初值处是否有除零、log 或 sqrt 的负参数。").path("/rhs").detail("t", json!(t)),
                Fail::StepTooSmall { t, h, last_non_finite } => {
                    let why = if last_non_finite { "最后一次试探步里右端出现了 NaN/Inf（如 sqrt 或 log 的参数变负）" } else { "解可能在这里发散（有限时间爆破）或出现奇点，或问题极度刚性" };
                    OpError::new(if last_non_finite { "E.NUMERICAL.NON_FINITE_EVALUATION" } else { "E.NUMERICAL.STEP_SIZE_TOO_SMALL" }, format!("{what}在 t = {t} 处步长缩到 {h:e} 仍不能满足容差：{why}。"), "检查模型在这个时刻附近的行为；缩短 t_span 到这之前，或放宽容差。")
                        .path("/t_span").detail("t", json!(t)).detail("h", json!(h)).detail("non_finite_rhs", json!(last_non_finite))
                }
                Fail::StepLimit { t, steps } => OpError::new("E.RESOURCE.COMPUTE_LIMIT", format!("{what}走到 t = {t} 时已用 {steps} 步，超过上限 {max_steps}。问题可能是刚性的（显式方法要用极小步长）。"), "缩短 t_span、放宽容差；刚性问题需要隐式方法（本算子暂未提供）。").path("/t_span").detail("t", json!(t)).detail("steps", json!(steps)),
                Fail::Cancelled(e) => e,
            }
        };
        let solve = |rtol: f64, atol: f64, first: Option<f64>| { let mut vars = Vec::with_capacity(1 + i.model.n + i.model.params.len()); dopri5(&mut |t, y, out| i.model.f(t, y, out, &mut vars), i.model.n, &i.y0, i.t0, i.tf, &i.t_eval, rtol, atol, i.max_step, first, max_steps, &|| ctx.check()) };
        let sol = solve(i.rtol, i.atol, i.first_step).map_err(|f| fail(f, ""))?;
        // global error: the same method with rtol/refine, atol/refine at the same output points
        let fine = solve(i.rtol / refine, i.atol / refine, None);
        let n = i.states.len();
        let nout = i.t_eval.len();
        let (mut max_abs, mut max_scaled) = (vec![0.0f64; n], 0.0f64);
        let fine_ok = fine.is_ok();
        if let Ok(fs) = &fine {
            for (yk, fk) in sol.y.iter().zip(&fs.y) {
                for s in 0..n {
                    let d = (yk[s] - fk[s]).abs();
                    max_abs[s] = max_abs[s].max(d);
                    max_scaled = max_scaled.max(d / (i.atol + i.rtol * yk[s].abs()));
                }
            }
        }
        let finite = sol.y.iter().all(|r| r.iter().all(|v| v.is_finite()));
        let consistent = fine_ok && max_scaled <= cfac;
        let mut warnings = Vec::new();
        if let Some(ts) = sol.stiff_at {
            warnings.push(Warning::new("W.NUMERICAL.STIFF_PROBLEM", format!("从 t ≈ {ts} 起问题表现为刚性（h·|λ| 连续 15 步 > 3.25，Hairer DOPRI5 检测）：显式方法要用很小的步长，步数会很大。结果仍按容差控制。"), ).path("/rhs").details(json!({"t": ts, "n_steps": sol.n_steps})));
        }
        let mut ymat = Vec::with_capacity(n * nout);
        for s in 0..n {
            for k in 0..nout {
                ymat.push(num(sol.y[k][s]));
            }
        }
        let fine_stats = fine.as_ref().map(|f| json!({"n_steps": f.n_steps, "n_rejected": f.n_rejected, "n_rhs_evals": f.n_rhs})).unwrap_or(Value::Null);
        let result = json!({
            "t": {"dtype": "f64", "shape": [nout], "data": i.t_eval.iter().map(|v| num(*v)).collect::<Vec<_>>()},
            "y": {"dtype": "f64", "shape": [n, nout], "data": ymat},
            "states": i.states,
            "stats": {"n_steps": sol.n_steps, "n_rejected": sol.n_rejected, "n_rhs_evals": sol.n_rhs},
            "tolerance": {"rtol": i.rtol, "atol": i.atol},
            "global_error": if fine_ok { json!({"max_abs": max_abs.iter().map(|v| num(*v)).collect::<Vec<_>>(), "max_scaled": num(max_scaled), "method": format!("同一方法以 rtol/{refine}、atol/{refine} 重解，在输出点上比较")}) } else { Value::Null }
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"global_error_max_scaled": if fine_ok { num(max_scaled) } else { Value::Null },
                         "definition": format!("max_{{i,k}} |y − y_fine| / (atol + rtol·|y|)，y_fine 以 rtol/{refine}、atol/{refine} 重解；≤ {cfac} 记为 tolerance_consistent")},
            "condition": {"stiffness_detected_at": sol.stiff_at.map(num).unwrap_or(Value::Null)},
            "error_estimate": {"y_abs_max": if fine_ok { num(max_abs.iter().cloned().fold(0.0, f64::max)) } else { Value::Null },
                               "basis": "后验估计：与 10 倍更严容差的解之差，近似当前解的全局误差（更严的解本身误差约小 10^(4/5) 倍以上）。非严格上界；rtol/atol 只控制每步的局部误差"},
            "checks": {"all_finite": finite, "tolerance_consistent": consistent},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"refinement_solve": fine_stats, "tolerance_consistency_factor": cfac, "direction": if i.tf > i.t0 { 1 } else { -1 }}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        OdeIvp::new().call(&args, &HostCtx::default())
    }
    fn row(env: &Value, s: usize) -> Vec<f64> {
        let sh = env["result"]["y"]["shape"].as_array().unwrap();
        let nout = sh[1].as_u64().unwrap() as usize;
        env["result"]["y"]["data"].as_array().unwrap()[s * nout..(s + 1) * nout].iter().map(|v| v.as_f64().unwrap()).collect()
    }

    /// Harmonic oscillator x'' = −x: x = cos t, v = −sin t; tight tolerance.
    #[test]
    fn harmonic_oscillator() {
        let e = call(json!({"states": ["x", "v"], "rhs": ["v", "-x"], "initial": {"x": 1.0, "v": 0.0}, "t_span": [0.0, 10.0], "rtol": 1e-10, "atol": 1e-12, "n_out": 11}));
        assert_eq!(e["status"], "ok", "{e}");
        let (x, v) = (row(&e, 0), row(&e, 1));
        for k in 0..11 {
            let t = k as f64;
            assert!((x[k] - t.cos()).abs() < 1e-8 && (v[k] + t.sin()).abs() < 1e-8, "t={t}: {} {}", x[k], v[k]);
        }
        assert_eq!(x[0], 1.0, "t0 is exactly y0");
        assert_eq!(e["trust"]["checks"]["tolerance_consistent"], true, "{}", e["trust"]["residual"]);
    }

    /// Exponential decay with a parameter, backward in time.
    #[test]
    fn params_and_backward() {
        let e = call(json!({"states": ["y"], "rhs": ["-k*y"], "params": {"k": 0.5}, "initial": {"y": 1.0}, "t_span": [2.0, 0.0], "t_eval": [2.0, 1.0, 0.0], "rtol": 1e-9, "atol": 1e-12}));
        assert_eq!(e["status"], "ok", "{e}");
        let y = row(&e, 0);
        assert!((y[2] - 1.0f64.exp()).abs() < 1e-7, "{y:?}");
    }

    #[test]
    fn validation_errors() {
        let base = json!({"states": ["x"], "rhs": ["-x"], "initial": {"x": 1.0}, "t_span": [0.0, 1.0]});
        let mut a = base.clone();
        a["rhs"] = json!(["-x +"]);
        let e = call(a);
        assert_eq!(e["error"]["code"], "E.VALIDATION.EXPRESSION");
        assert_eq!(e["error"]["details"]["position"], 4);
        let mut a = base.clone();
        a["rhs"] = json!(["-y"]);
        assert_eq!(call(a)["error"]["details"]["kind"], "unknown_name");
        let mut a = base.clone();
        a["initial"] = json!({"y": 1.0});
        assert_eq!(call(a)["error"]["code"], "E.VALIDATION.SHAPE");
        let mut a = base.clone();
        a["params"] = json!({"x": 1.0});
        assert_eq!(call(a)["error"]["details"]["kind"], "duplicate_name");
        let mut a = base.clone();
        a["states"] = json!(["t"]);
        assert_eq!(call(a)["error"]["details"]["kind"], "bad_name");
        let mut a = base.clone();
        a["t_eval"] = json!([0.0, 1.0]);
        a["n_out"] = json!(5);
        assert_eq!(call(a)["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let mut a = base.clone();
        a["t_eval"] = json!([0.0, 2.0]);
        assert_eq!(call(a)["error"]["code"], "E.VALIDATION.RANGE");
        let mut a = base.clone();
        a["rtol"] = json!(1e-16);
        assert_eq!(call(a)["error"]["code"], "E.VALIDATION.RANGE");
    }

    /// y' = y², y(0) = 1 blows up at t = 1.
    #[test]
    fn blow_up_is_step_too_small() {
        let e = call(json!({"states": ["y"], "rhs": ["y^2"], "initial": {"y": 1.0}, "t_span": [0.0, 2.0]}));
        assert_eq!(e["error"]["code"], "E.NUMERICAL.STEP_SIZE_TOO_SMALL", "{e}");
        let t = e["error"]["details"]["t"].as_f64().unwrap();
        assert!((t - 1.0).abs() < 1e-2, "{t}");
    }

    #[test]
    fn nextafter() {
        assert_eq!(aeoi_core::kernels::rk::next_toward(1.0, 1.0), 1.0 + f64::EPSILON);
        assert_eq!(aeoi_core::kernels::rk::next_toward(1.0, -1.0), 1.0 - f64::EPSILON / 2.0);
    }
}
