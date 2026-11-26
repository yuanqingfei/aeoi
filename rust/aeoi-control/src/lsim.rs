//! control.lti.lsim — response to an arbitrary sampled input (spec 10.15, v0.4.5).
//!
//! Between samples the input is interpreted as `foh` (linear, default) or `zoh` (held); for that
//! interpretation the sampled response has no discretization error. Continuous systems use the block
//! matrix exponential of M = [[A·dt, B·dt, 0], [0, 0, I], [0, 0, 0]] (Van Loan 1978), the same
//! formulas as scipy.signal.lsim and python-control forced_response:
//!   foh: x_{k+1} = Φ x_k + (F1 − F2) u_k + F2 u_{k+1},   zoh: x_{k+1} = Φ x_k + F1 u_k.
//! Discrete systems recurse x_{k+1} = A x_k + B u_k. y_k = C x_k + D u_k, x_0 = x0 (default 0).
//! Post-condition `half_step_consistent`: the discretization is redone at dt/2 and the first steps are
//! re-run on the same interpolated input; even-indexed states must agree (an independent check of Φ, F1, F2).
use crate::step::decimate;
use crate::U;
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::kernels::timeresp::{hold_gammas, propagate};
use aeoi_core::linalg::{norm1, sort_poles};
use aeoi_core::lti::{eigenvalues, parse_lti, SsData};
use aeoi_core::matrix::{dense_real, dense_real_nd, parse_array, to_real_matrix, Array};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::{DMatrix, DVector};
use serde_json::{json, Value};

operator_struct!(Lsim, "control.lti.lsim");

const HALF_STEP_STEPS: usize = 64;
const HALF_STEP_TOL: f64 = 1e-9;

pub struct In {
    ss: SsData,
    kind: &'static str,
    u: DMatrix<f64>, // [nu, nt]
    dt: f64,
    t0: f64,
    foh: bool,
    x0: Option<DVector<f64>>,
    return_states: bool,
    out_indices: Option<Vec<usize>>,
    return_none: bool,
}

fn present(args: &Value, k: &str) -> bool {
    args.get(k).is_some_and(|v| !v.is_null())
}

fn discretize(ss: &SsData, h: f64, foh: bool) -> (DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, u32) {
    hold_gammas(&ss.a, &ss.b, h, foh)
}

impl Operator for Lsim {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        let ss = sys.to_ss();
        let (nx, nu, ny) = (ss.a.nrows(), ss.b.ncols(), ss.c.nrows());
        let discrete = ss.ts > 0.0;
        // u: [nu, nt], or [nt] when nu = 1
        let arr = parse_array(&args["u"], "/u", host, false)?;
        let one_d = matches!(&arr, Array::Dense(d) if d.shape.len() == 1);
        let um = to_real_matrix(&arr, "/u", &mut w, self.meta.max_dense("/u"))?;
        let u = if one_d {
            if nu != 1 {
                return Err(OpError::new("E.VALIDATION.SHAPE", format!("系统有 {nu} 个输入，u 必须是 [nu, nt] = [{nu}, nt] 的二维数组；一维 u 只用于单输入系统。"), "按行给出每个输入通道：第 i 行是第 i 个输入。")
                    .path("/u")
                    .detail("expected_rows", json!(nu)));
            }
            um.transpose()
        } else {
            um
        };
        if u.nrows() != nu {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("u 有 {} 行，系统有 {nu} 个输入；u 的形状应为 [nu, nt]（第 i 行 = 第 i 个输入通道）。", u.nrows()), "检查是否把 [nt, nu] 当成了 [nu, nt]：每一行是一个输入通道，每一列是一个时刻。")
                .path("/u")
                .detail("expected_rows", json!(nu))
                .detail("got_shape", json!([u.nrows(), u.ncols()])));
        }
        let nt = u.ncols();
        if nt == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "u 没有采样点（nt = 0）。", "至少给出一个时刻的输入。").path("/u"));
        }
        let tm = &args["time"];
        let dt = tm["dt"].as_f64().unwrap();
        let t0 = tm.get("t0").and_then(|x| x.as_f64()).unwrap_or(0.0);
        if discrete {
            if ((dt - ss.ts) / ss.ts).abs() > 1e-9 {
                return Err(OpError::new("E.VALIDATION.GRID", format!("离散系统的输入是每个采样周期一个值，time.dt 必须等于 ts = {} s，得到 dt = {dt} s。", ss.ts), "把 time.dt 设为 ts；输入采样间隔不同就先重采样。")
                    .path("/time/dt")
                    .remedy(modify_input("dt_eq_ts", "dt 设为 ts", "high", false, json!([{"op": "replace", "path": "/time/dt", "value": ss.ts}]))));
            }
            if present(args, "interpolation") {
                return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", "离散系统的输入就是采样序列，没有“采样之间”，interpolation 不适用。不能静默忽略（F-8）。", "去掉 interpolation。")
                    .path("/interpolation")
                    .detail("conflicting_fields", json!(["interpolation"]))
                    .detail("ts", json!(ss.ts))
                    .remedy(modify_input("drop_interpolation", "去掉 interpolation", "high", false, json!([{"op": "remove", "path": "/interpolation"}]))));
            }
        }
        let foh = !discrete && args.get("interpolation").and_then(|x| x.as_str()).unwrap_or("foh") == "foh";
        // x0 belongs to the given realization: only meaningful for kind = ss
        let x0 = if present(args, "x0") {
            if sys.kind() != "ss" {
                return Err(OpError::new(
                    "E.VALIDATION.OPTION_CONFLICT",
                    format!("x0 是状态向量，它的坐标由状态空间实现决定；system 是 {}，没有状态坐标，x0 没有定义。", sys.kind()),
                    "先用 control_lti_convert（output_kind = ss）把系统转成状态空间，再把 x0 写成那个实现的状态；或去掉 x0（零初始状态）。",
                )
                .path("/x0")
                .detail("conflicting_fields", json!(["x0"]))
                .detail("system_kind", json!(sys.kind()))
                .remedy(modify_input("drop_x0", "去掉 x0（零初始状态）", "medium", true, json!([{"op": "remove", "path": "/x0"}]))));
            }
            let xa = parse_array(&args["x0"], "/x0", host, false)?;
            let xm = to_real_matrix(&xa, "/x0", &mut w, 1_000_000)?;
            if xm.len() != nx || (xm.nrows() != nx && xm.ncols() != nx) {
                return Err(OpError::new("E.VALIDATION.SHAPE", format!("x0 有 {} 个元素，系统有 {nx} 个状态。", xm.len()), "x0 是长度为 n 的向量（n = A 的阶数）。").path("/x0").detail("expected_length", json!(nx)));
            }
            Some(DVector::from_iterator(nx, xm.iter().cloned()))
        } else {
            None
        };
        let return_states = args.get("return_states").and_then(|x| x.as_bool()).unwrap_or(false);
        let max_grid = self.meta.manifest["limits"]["max_grid_points"].as_u64().unwrap() as usize;
        if nt > max_grid {
            return Err(OpError::new("E.RESOURCE.COMPUTE_LIMIT", format!("输入有 {nt} 个采样，超过单次计算上限 {max_grid}。"), "分段计算：把上一段的最终状态作为下一段的 x0（系统为 ss 时）。")
                .path("/u")
                .detail("grid_points", json!(nt))
                .detail("limit", json!(max_grid)));
        }
        let out = &args["output"];
        let mode = out.get("return_samples").and_then(|x| x.as_str()).unwrap_or("all");
        let max_samples = out.get("max_samples").and_then(|x| x.as_u64()).unwrap_or(10001) as usize;
        let out_indices = if mode == "decimated" { Some(decimate(nt, max_samples)) } else { None };
        let returned = if mode == "none" { 0 } else { out_indices.as_ref().map(|v| v.len()).unwrap_or(nt) };
        let per = ny + if return_states { nx } else { 0 };
        let max_out = self.meta.manifest["limits"]["max_output_values"].as_u64().unwrap() as usize;
        if returned.saturating_mul(per.max(1)) > max_out {
            let m = (max_out / per.max(1)).max(2);
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("将返回 {returned} 个采样 × {per} 个通道（输出{}），超过单次返回上限 {max_out}。", if return_states { "与状态" } else { "" }), "抽样返回，或不返回状态。")
                .path("/output")
                .remedy(modify_input("decimate", &format!("抽样返回 {m} 个点"), "high", false, json!([{"op": "add", "path": "/output", "value": {"return_samples": "decimated", "max_samples": m}}]))));
        }
        Ok((In { kind: sys.kind(), ss, u, dt, t0, foh, x0, return_states, out_indices, return_none: mode == "none" }, w))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let ss = &i.ss;
        let (nx, nu, ny) = (ss.a.nrows(), ss.b.ncols(), ss.c.nrows());
        let nt = i.u.ncols();
        let continuous = ss.ts == 0.0;
        let mut warnings = Vec::new();
        let (phi, g0, g1, squarings) = if continuous { discretize(ss, i.dt, i.foh) } else { (ss.a.clone(), ss.b.clone(), DMatrix::zeros(nx, nu), 0) };
        let x0 = i.x0.clone().unwrap_or_else(|| DVector::zeros(nx));
        // recursion; states kept only if returned or needed for the half-step check
        let keep = if i.return_states { nt } else { HALF_STEP_STEPS.min(nt) };
        let (y, xs) = propagate(&phi, &g0, &g1, &ss.c, &ss.d, &i.u, &x0, i.foh, keep, &|| ctx.check())?;
        // post-condition: re-discretize at dt/2, feed the same interpolated input (midpoint = mean for foh,
        // u_k for zoh), and compare even-indexed states over the first steps
        let (half_ok, half_rel) = if continuous && nx > 0 && nt > 1 {
            let (ph, h0, h1, _) = discretize(ss, i.dt / 2.0, i.foh);
            let steps = (HALF_STEP_STEPS.min(nt) - 1).max(1);
            let mut z = x0.clone();
            let (mut worst, mut scale) = (0.0_f64, 0.0_f64);
            for k in 0..steps {
                let (a, b) = (i.u.column(k).into_owned(), i.u.column(k + 1).into_owned());
                let mid = if i.foh { (&a + &b) * 0.5 } else { a.clone() };
                // zoh: Γ1 = 0 and mid = u_k, so the same two half steps apply
                let zm = &ph * &z + &h0 * &a + &h1 * &mid;
                z = &ph * &zm + &h0 * &mid + &h1 * &b;
                worst = worst.max((&z - &xs[k + 1]).norm());
                scale = scale.max(xs[k + 1].norm());
            }
            let rel = if scale > 0.0 { worst / scale } else { worst };
            (rel <= HALF_STEP_TOL, Some(rel))
        } else {
            (true, None)
        };
        // stability, grid
        let mut poles = eigenvalues(&ss.a);
        sort_poles(&mut poles);
        let stable = if continuous { poles.iter().all(|p| p.re < 0.0) } else { poles.iter().all(|p| p.norm() < 1.0) };
        let max_re = poles.iter().map(|p| if continuous { p.re } else { p.norm() }).fold(f64::NEG_INFINITY, f64::max);
        if !stable && nx > 0 {
            warnings.push(Warning::new("W.DOMAIN.UNSTABLE_SYSTEM", format!("系统不是渐近稳定的（{} = {:.6}），响应可能发散；数值本身仍按公式计算。", if continuous { "最大极点实部" } else { "最大极点模" }, max_re)).path("/system"));
        }
        let fast = poles.iter().map(|p| p.norm()).fold(0.0, f64::max);
        if continuous && fast > 0.0 && i.dt > 0.1 / fast {
            warnings.push(Warning::new("W.ACCURACY.GRID_TOO_COARSE", format!("dt = {} s 大于 0.1/最快极点模 = {:.3e} s。对“采样之间按 {} 解释”的输入，采样点上的响应仍然精确；但真实输入在采样之间若不是这个形状，误差由输入采样决定，且网格看不到快速模态的峰值。", i.dt, 0.1 / fast, if i.foh { "foh（线性）" } else { "zoh（保持）" })).path("/time/dt"));
        }
        // outputs
        let idx: Vec<usize> = i.out_indices.clone().unwrap_or_else(|| (0..nt).collect());
        let (t_out, y_out, x_out) = if i.return_none {
            (Value::Null, Value::Null, Value::Null)
        } else {
            let m = idx.len();
            let tv: Vec<f64> = idx.iter().map(|&k| i.t0 + k as f64 * i.dt).collect();
            let mut yo = Vec::with_capacity(ny * m);
            for r in 0..ny {
                for &k in &idx {
                    yo.push(y[r * nt + k]);
                }
            }
            let xo = if i.return_states {
                let mut xm = DMatrix::zeros(nx, m);
                for (c, &k) in idx.iter().enumerate() {
                    xm.set_column(c, &xs[k]);
                }
                dense_real(&xm)
            } else {
                Value::Null
            };
            (json!({"unit": "s", "value": dense_real_nd(&[m], &tv)}), json!({"unit": "1", "value": dense_real_nd(&[ny, m], &yo)}), xo)
        };
        let finite = y.iter().all(|v| v.is_finite()) && (!i.return_states || xs.iter().all(|v| v.iter().all(|z| z.is_finite())));
        let ymax = y.iter().map(|v| v.abs()).fold(0.0, f64::max);
        let heur = (nt as f64 * U).max(half_rel.unwrap_or(0.0)) * ymax.max(f64::MIN_POSITIVE);
        let realization = if i.kind == "ss" { "given ss" } else { "controllable canonical form (scipy tf2ss convention)" };
        let result = json!({
            "t": t_out, "y": y_out, "x": x_out, "n_grid_points": nt,
            "interpolation": if continuous { if i.foh { "foh" } else { "zoh" } } else { "none" },
            "realization": realization
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": half_rel.map(|r| json!({"half_step_state_rel": num(r), "definition": format!("dt/2 重新离散化、同一插值输入，前 {} 步偶数步状态与原递推之差 max‖Δx‖₂ / max‖x‖₂", HALF_STEP_STEPS.min(nt) - 1)})).unwrap_or(Value::Null),
            "condition": {"norm1_A_dt": norm1(&ss.a) * i.dt, "expm_squarings": squarings, "realization": realization, if continuous {"max_real_pole"} else {"max_pole_modulus"}: num(max_re)},
            "error_estimate": {"y_abs": num(heur), "basis": "启发式：max(N·u, 半步一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。只针对 foh/zoh 解释下的输入，非严格上界"},
            "checks": {"all_finite": finite, "half_step_consistent": half_ok, "asymptotically_stable": stable},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"dt": i.dt, "t0": i.t0, "t_end": i.t0 + (nt - 1) as f64 * i.dt, "half_step_tolerance": HALF_STEP_TOL}
        });
        Ok(Output { result, trust, warnings })
    }

    fn estimate(&self, i: &In) -> Option<Value> {
        let nx = i.ss.a.nrows();
        Some(json!({"grid_points": i.u.ncols(), "recursion_flops": 2 * i.u.ncols() * (nx * nx + nx * i.ss.b.ncols() * if i.foh { 2 } else { 1 } + i.ss.c.nrows() * (nx + i.ss.b.ncols()))}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        Lsim::new().call(&args, &HostCtx::default())
    }
    fn m(shape: &[usize], data: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": shape, "data": data})
    }
    fn ydata(env: &Value) -> Vec<f64> {
        env["result"]["y"]["value"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    }

    /// First-order lag 1/(s+1) driven by a ramp u = t: y = t − 1 + e^{−t}. foh is exact for a ramp.
    #[test]
    fn ramp_foh_exact() {
        let n = 501;
        let dt = 0.01;
        let u: Vec<f64> = (0..n).map(|k| k as f64 * dt).collect();
        let env = call(json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 1.0], "ts": 0.0}, "u": m(&[n], &u), "time": {"dt": dt, "unit": "s"}}));
        assert_eq!(env["status"], "ok", "{env}");
        let y = ydata(&env);
        let worst = (0..n).map(|k| { let t = k as f64 * dt; (y[k] - (t - 1.0 + (-t).exp())).abs() }).fold(0.0, f64::max);
        assert!(worst < 1e-13, "{worst}");
        assert_eq!(env["trust"]["checks"]["half_step_consistent"], true);
    }

    /// zoh with a constant input equals the step response.
    #[test]
    fn zoh_constant_is_step() {
        let u = vec![1.0; 201];
        let env = call(json!({"system": {"kind": "tf", "num": [2.0], "den": [1.0, 3.0, 2.0], "ts": 0.0}, "u": m(&[201], &u), "time": {"dt": 0.05, "unit": "s"}, "interpolation": "zoh"}));
        let y = ydata(&env);
        let worst = (0..201).map(|k| { let t = k as f64 * 0.05; (y[k] - (1.0 - 2.0 * (-t).exp() + (-2.0 * t).exp())).abs() }).fold(0.0, f64::max);
        assert!(worst < 1e-13, "{worst}");
    }

    /// x0 with a tf system is an option conflict (no state coordinates).
    #[test]
    fn x0_requires_ss() {
        let env = call(json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 1.0], "ts": 0.0}, "u": m(&[2], &[0.0, 0.0]), "time": {"dt": 0.1, "unit": "s"}, "x0": m(&[1], &[1.0])}));
        assert_eq!(env["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert_eq!(env["error"]["details"]["conflicting_fields"], json!(["x0"]));
    }

    /// Free response of ẋ = −x from x0 = 1 is e^{−t}.
    #[test]
    fn x0_free_response() {
        let env = call(json!({"system": {"kind": "ss", "A": m(&[1, 1], &[-1.0]), "B": m(&[1, 1], &[1.0]), "C": m(&[1, 1], &[1.0]), "D": m(&[1, 1], &[0.0]), "ts": 0.0}, "u": m(&[101], &[0.0; 101]), "time": {"dt": 0.02, "unit": "s"}, "x0": m(&[1], &[1.0]), "return_states": true}));
        assert_eq!(env["status"], "ok", "{env}");
        let y = ydata(&env);
        assert!((y[100] - (-2.0_f64).exp()).abs() < 1e-14);
        assert_eq!(env["result"]["x"]["shape"], json!([1, 101]));
    }

    /// Discrete: interpolation is not applicable; dt must equal ts.
    #[test]
    fn discrete_rules() {
        let sys = json!({"kind": "tf", "num": [0.5], "den": [1.0, -0.5], "ts": 0.1});
        let e = call(json!({"system": sys, "u": m(&[2], &[1.0, 1.0]), "time": {"dt": 0.1, "unit": "s"}, "interpolation": "foh"}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let e = call(json!({"system": sys, "u": m(&[2], &[1.0, 1.0]), "time": {"dt": 0.2, "unit": "s"}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.GRID");
        let e = call(json!({"system": sys, "u": m(&[3], &[1.0, 1.0, 1.0]), "time": {"dt": 0.1, "unit": "s"}}));
        assert_eq!(ydata(&e), vec![0.0, 0.5, 0.75]);
        assert_eq!(e["result"]["interpolation"], "none");
    }

    /// u given as [nt, nu] instead of [nu, nt] is a shape error with the expected row count.
    #[test]
    fn transposed_u_rejected() {
        let sys = json!({"kind": "ss", "A": m(&[2, 2], &[-1.0, 0.0, 0.0, -2.0]), "B": m(&[2, 2], &[1.0, 0.0, 0.0, 1.0]), "C": m(&[1, 2], &[1.0, 1.0]), "D": m(&[1, 2], &[0.0, 0.0]), "ts": 0.0});
        let e = call(json!({"system": sys, "u": m(&[3, 2], &[1.0, 2.0, 1.0, 2.0, 1.0, 2.0]), "time": {"dt": 0.1, "unit": "s"}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
        assert_eq!(e["error"]["details"]["expected_rows"], 2);
    }
}
