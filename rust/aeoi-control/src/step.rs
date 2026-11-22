//! control.lti.step — step response by exact ZOH discretization (spec 10.2);
//! control.lti.impulse — impulse response (spec 10.9) shares the grid/validation/recursion code.
use crate::U;
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::linalg::{expm, norm1, sort_poles};
use aeoi_core::lti::{eigenvalues, parse_lti, SsData};
use aeoi_core::matrix::{complex_vector, dense_real, dense_real_nd};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use nalgebra::{DMatrix, DVector};
use serde_json::{json, Value};

pub struct Step {
    meta: OperatorMeta,
    impulse: bool,
}

impl Step {
    pub fn new() -> Self {
        Step { meta: operator_meta!("control.lti.step"), impulse: false }
    }
    /// control.lti.impulse
    pub fn impulse() -> Self {
        Step { meta: operator_meta!("control.lti.impulse"), impulse: true }
    }
}

impl Default for Step {
    fn default() -> Self {
        Self::new()
    }
}

pub struct StepInput {
    pub ss: SsData,
    pub kind: &'static str,
    pub dt: f64,
    pub n: usize,
    pub auto: bool,
    pub info: bool,
    pub rise: (f64, f64),
    pub settle: f64,
    pub out_indices: Option<Vec<usize>>, // None => all
    pub return_none: bool,
    pub impulse: bool,
    /// discrete impulse amplitude u_0 (1/ts for unit_area, 1 for unit_pulse)
    pub pulse: f64,
}

/// Grid point count for the uniform grid: N = floor(t_final/dt + 1e-9) + 1.
pub fn grid_len(t_final: f64, dt: f64) -> usize {
    (t_final / dt + 1e-9).floor() as usize + 1
}

/// Evenly spaced decimation indices: round(k (N-1)/(M-1)).
pub fn decimate(n: usize, m: usize) -> Vec<usize> {
    if m >= n {
        return (0..n).collect();
    }
    (0..m).map(|k| ((k as f64) * (n - 1) as f64 / (m - 1) as f64).round() as usize).collect()
}

impl Operator for Step {
    type Input = StepInput;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }

    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(StepInput, Vec<Warning>), OpError> {
        let mut warnings = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut warnings, self.meta.max_dense("/system/A"))?;
        let ss = sys.to_ss();
        let (ny, nu) = (ss.c.nrows(), ss.b.ncols());
        let tm = &args["time"];
        let auto = tm["mode"] == "auto";
        let (t_final, dt) = if auto {
            let p = eigenvalues(&ss.a);
            let given = tm.get("t_final").and_then(|x| x.as_f64());
            if ss.ts > 0.0 {
                let rho = p.iter().map(|z| z.norm()).fold(0.0, f64::max);
                let tf = given.unwrap_or(if rho < 1.0 && rho > 0.0 { (7.0 * ss.ts / -rho.ln()).max(10.0 * ss.ts) } else { 100.0 * ss.ts });
                (tf, ss.ts)
            } else {
                let stable = p.iter().all(|z| z.re < 0.0);
                let slow = p.iter().map(|z| z.re.abs()).fold(f64::INFINITY, f64::min);
                let fast = p.iter().map(|z| z.norm()).fold(0.0, f64::max);
                let tf = given.unwrap_or(if stable && !p.is_empty() { (7.0 / slow).clamp(1e-6, 1e6) } else { 10.0 });
                let mut dt = tf / 1000.0;
                if fast > 0.0 {
                    dt = dt.min(0.1 / fast);
                }
                dt = dt.max(tf / 10000.0);
                (tf, dt)
            }
        } else {
            (tm["t_final"].as_f64().unwrap(), tm["dt"].as_f64().unwrap())
        };
        if auto {
            warnings.push(Warning::new("W.ACCURACY.AUTO_GRID", format!("使用启发式时间网格：t_final = {t_final:.6} s，dt = {dt:.6} s。auto 网格不参与 parity 测试，需要可复现对比时请显式给出 uniform 网格。")).path("/time"));
        }
        if ss.ts > 0.0 && ((dt - ss.ts) / ss.ts).abs() > 1e-9 {
            return Err(OpError::new("E.VALIDATION.GRID", format!("离散系统要求 dt 等于采样周期 ts = {} s，得到 dt = {dt} s。", ss.ts), "把 dt 设为 ts。")
                .path("/time/dt")
                .remedy(modify_input("dt_eq_ts", "dt 设为 ts", "high", false, json!([{"op": "replace", "path": "/time/dt", "value": ss.ts}]))));
        }
        if dt > t_final {
            return Err(OpError::new("E.VALIDATION.GRID", "dt 大于 t_final，网格少于 2 个点。", "减小 dt 或增大 t_final。").path("/time/dt"));
        }
        let n = grid_len(t_final, dt);
        let max_grid = self.meta.manifest["limits"]["max_grid_points"].as_u64().unwrap() as usize;
        if n.saturating_mul(nu) > max_grid {
            let p = eigenvalues(&ss.a);
            let fast = p.iter().map(|z| z.norm()).fold(0.0, f64::max);
            let mut e = OpError::new(
                "E.RESOURCE.COMPUTE_LIMIT",
                format!("时间网格 {n} 点 × {nu} 个输入通道 = {} 步，超过单次计算上限 {max_grid}。", n * nu),
                "增大 dt 或缩短 t_final。抽样返回（output.return_samples）不能解决此问题，因为递推仍需走完每个网格点。",
            )
            .path("/time/dt")
            .detail("grid_points", json!(n))
            .detail("limit", json!(max_grid));
            if fast > 0.0 {
                let dt_new = (0.1 / fast).max(t_final / ((max_grid / nu.max(1)) as f64 - 1.0));
                let dt_new = (dt_new * 1e6).round() / 1e6;
                e = e.detail("fastest_pole_rad_s", json!(fast)).remedy(modify_input("coarser_grid", &format!("dt 改为 {dt_new} s（≈ 0.1/最快极点模，且满足上限）"), "high", false, json!([{"op": "replace", "path": "/time/dt", "value": dt_new}])));
            }
            return Err(e.remedy(ask_user("shorten", "与用户确认是否可以缩短 t_final")));
        }
        let out = &args["output"];
        let mode = out.get("return_samples").and_then(|x| x.as_str()).unwrap_or("all");
        let max_samples = out.get("max_samples").and_then(|x| x.as_u64()).unwrap_or(10001) as usize;
        let out_indices = if mode == "decimated" { Some(decimate(n, max_samples)) } else { None };
        let returned = if mode == "none" { 0 } else { out_indices.as_ref().map(|v| v.len()).unwrap_or(n) };
        let max_out = self.meta.manifest["limits"]["max_output_values"].as_u64().unwrap() as usize;
        if returned.saturating_mul(ny * nu) > max_out {
            let m = (max_out / (ny * nu)).max(2);
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("将返回 {returned} 个采样 × {} 个通道，超过单次返回上限 {max_out}。", ny * nu), "改为抽样返回，或只返回 step_info。")
                .path("/output")
                .remedy(modify_input("decimate", &format!("抽样返回 {m} 个点"), "high", false, json!([{"op": "add", "path": "/output", "value": {"return_samples": "decimated", "max_samples": m}}])))
                .remedy(modify_input("info_only", "只返回 step_info", "medium", false, json!([{"op": "add", "path": "/output", "value": {"return_samples": "none"}}]))));
        }
        let si = &args["step_info"];
        let rl = si.get("rise_time_limits").and_then(|x| x.as_array()).map(|a| (a[0].as_f64().unwrap(), a[1].as_f64().unwrap())).unwrap_or((0.1, 0.9));
        if rl.0 >= rl.1 {
            return Err(OpError::new("E.VALIDATION.RANGE", "rise_time_limits 需满足 lo < hi。", "例如 [0.1, 0.9]。").path("/step_info/rise_time_limits")
                .remedy(modify_input("default", "使用默认 [0.1, 0.9]", "high", false, json!([{"op": "replace", "path": "/step_info/rise_time_limits", "value": [0.1, 0.9]}]))));
        }
        let pulse = if ss.ts > 0.0 && args.get("discrete_scaling").and_then(|x| x.as_str()) != Some("unit_pulse") { 1.0 / ss.ts } else { 1.0 };
        Ok((
            StepInput {
                ss,
                kind: sys.kind(),
                dt,
                n,
                auto,
                info: !self.impulse && si.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
                rise: rl,
                settle: si.get("settling_threshold").and_then(|x| x.as_f64()).unwrap_or(0.02),
                out_indices,
                return_none: mode == "none",
                impulse: self.impulse,
                pulse,
            },
            warnings,
        ))
    }

    fn estimate(&self, i: &StepInput) -> Option<Value> {
        let nx = i.ss.a.nrows();
        Some(json!({"grid_points": i.n, "recursion_flops": 2 * i.n * i.ss.b.ncols() * (nx * nx + nx * i.ss.c.nrows())}))
    }

    fn execute(&self, inp: StepInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let ss = &inp.ss;
        let (nx, nu, ny) = (ss.a.nrows(), ss.b.ncols(), ss.c.nrows());
        let n = inp.n;
        let dt = inp.dt;
        let continuous = ss.ts == 0.0;
        let mut warnings = Vec::new();
        // discretize
        let aug_expm = |h: f64| -> (DMatrix<f64>, DMatrix<f64>, u32) {
            let mut m = DMatrix::zeros(nx + nu, nx + nu);
            m.view_mut((0, 0), (nx, nx)).copy_from(&(&ss.a * h));
            m.view_mut((0, nx), (nx, nu)).copy_from(&(&ss.b * h));
            let (e, s) = expm(&m);
            (e.view((0, 0), (nx, nx)).into_owned(), e.view((0, nx), (nx, nu)).into_owned(), s)
        };
        let (phi, gam, squarings) = if continuous { aug_expm(dt) } else { (ss.a.clone(), ss.b.clone(), 0) };
        // recursion
        let mut y = vec![0.0f64; ny * nu * n];
        let mut final_states = Vec::with_capacity(nu);
        for j in 0..nu {
            let g = gam.column(j).into_owned();
            let dcol = ss.d.column(j).into_owned();
            if inp.impulse {
                // continuous: x_0 = B e_j, y = C x (D·δ not representable); discrete: u_0 = pulse
                let (mut x, y0) = if continuous { (ss.b.column(j).into_owned(), None) } else { (DVector::<f64>::zeros(nx), Some(&dcol * inp.pulse)) };
                for k in 0..n {
                    let yk = match (&y0, k) {
                        (Some(v), 0) => v.clone(),
                        _ => &ss.c * &x,
                    };
                    for i in 0..ny {
                        y[(i * nu + j) * n + k] = yk[i];
                    }
                    if k + 1 < n {
                        x = if !continuous && k == 0 { ss.b.column(j) * inp.pulse } else { &phi * &x };
                    }
                    if k % 20000 == 0 {
                        ctx.check()?;
                    }
                }
                final_states.push(x);
                continue;
            }
            let mut x = DVector::<f64>::zeros(nx);
            for k in 0..n {
                let yk = &ss.c * &x + &dcol;
                for i in 0..ny {
                    y[(i * nu + j) * n + k] = yk[i];
                }
                if k + 1 < n {
                    x = &phi * &x + &g;
                }
                if k % 20000 == 0 {
                    ctx.check()?;
                }
            }
            final_states.push(x);
        }
        // a-posteriori check: recursion vs direct expm at t_end (continuous only)
        let t_end = (n - 1) as f64 * dt;
        let mut discrepancy: Option<f64> = None;
        if continuous && nx > 0 {
            let (phi_t, gam_t, _) = aug_expm(t_end);
            let mut worst: f64 = 0.0;
            for j in 0..nu {
                let direct = if inp.impulse { &phi_t * ss.b.column(j) } else { gam_t.column(j).into_owned() };
                let denom = direct.norm().max(f64::MIN_POSITIVE);
                worst = worst.max((&final_states[j] - &direct).norm() / denom);
            }
            discrepancy = Some(worst);
        }
        // stability & dc gain
        let mut poles = eigenvalues(&ss.a);
        sort_poles(&mut poles);
        let stable = if continuous { poles.iter().all(|p| p.re < 0.0) } else { poles.iter().all(|p| p.norm() < 1.0) };
        let max_re = poles.iter().map(|p| if continuous { p.re } else { p.norm() }).fold(f64::NEG_INFINITY, f64::max);
        let dcgain: Option<DMatrix<f64>> = if nx == 0 {
            Some(ss.d.clone())
        } else {
            let m = if continuous { -ss.a.clone() } else { DMatrix::identity(nx, nx) - &ss.a };
            m.lu().solve(&ss.b).map(|x| &ss.c * x + &ss.d).filter(|g| g.iter().all(|v| v.is_finite()))
        };
        if !stable {
            let what = if inp.impulse { "脉冲响应不衰减或发散" } else { "阶跃响应发散或不收敛，step_info 置为 null" };
            warnings.push(Warning::new("W.DOMAIN.UNSTABLE_SYSTEM", format!("系统不是渐近稳定的（{} = {:.6}），{what}。", if continuous { "最大极点实部" } else { "最大极点模" }, max_re)).path("/system"));
        }
        let fast = poles.iter().map(|p| p.norm()).fold(0.0, f64::max);
        if continuous && fast > 0.0 && dt > 0.1 / fast {
            warnings.push(Warning::new("W.ACCURACY.GRID_TOO_COARSE", if self.impulse { format!("dt = {dt} s 大于 0.1/最快极点模 = {:.3e} s；响应采样值本身仍精确，但网格可能看不到快速模态的峰值（从采样点读出的峰值/峰时误差可达 dt 量级）。", 0.1 / fast) } else { format!("dt = {dt} s 大于 0.1/最快极点模 = {:.3e} s；响应采样值本身仍精确，但 step_info 的时间类指标误差可达 dt。", 0.1 / fast) }).path("/time/dt"));
        }
        let tvec: Vec<f64> = (0..n).map(|k| k as f64 * dt).collect();
        // step info (python-control / MATLAB-style grid definitions, no interpolation)
        let mut info_val = Value::Null;
        if inp.info {
            let mut rows = Vec::new();
            let mut not_settled = 0;
            let mut zero_dc = 0;
            for i in 0..ny {
                let mut row = Vec::new();
                for j in 0..nu {
                    let inf = dcgain.as_ref().map(|g| g[(i, j)]);
                    let yy = &y[(i * nu + j) * n..(i * nu + j + 1) * n];
                    let v = match inf {
                        Some(inf) if stable && inf != 0.0 => {
                            let r = step_info(yy, &tvec, inf, inp.rise, inp.settle);
                            if r["settling_time"]["value"].is_null() {
                                not_settled += 1;
                            }
                            r
                        }
                        Some(inf) if stable && inf == 0.0 => {
                            zero_dc += 1;
                            Value::Null
                        }
                        _ => Value::Null,
                    };
                    row.push(v);
                }
                rows.push(Value::Array(row));
            }
            if stable && dcgain.is_none() {
                warnings.push(Warning::new("W.DOMAIN.INFINITE_DC_GAIN", "DC 增益无穷或无法计算，step_info 置为 null。").path("/system"));
            }
            if zero_dc > 0 {
                warnings.push(Warning::new("W.DOMAIN.ZERO_DC_GAIN", format!("{zero_dc} 个通道的 DC 增益为 0，上升/调节时间无定义，对应 step_info 为 null。")).path("/system"));
            }
            if not_settled > 0 {
                warnings.push(Warning::new("W.DOMAIN.NOT_SETTLED", format!("{not_settled} 个通道在 t_final 内未进入调节带，settling_time 为 null；可增大 t_final。")).path("/time/t_final"));
            }
            info_val = if ny == 1 && nu == 1 { rows[0][0].clone() } else { Value::Array(rows) };
        }
        // outputs
        let (t_out, y_out) = if inp.return_none {
            (Value::Null, Value::Null)
        } else {
            let idx: Vec<usize> = inp.out_indices.clone().unwrap_or_else(|| (0..n).collect());
            let m = idx.len();
            let ts_: Vec<f64> = idx.iter().map(|&k| tvec[k]).collect();
            let mut yo = Vec::with_capacity(ny * nu * m);
            for ch in 0..ny * nu {
                for &k in &idx {
                    yo.push(y[ch * n + k]);
                }
            }
            (json!({"unit": "s", "value": dense_real_nd(&[m], &ts_)}), json!({"unit": "1", "value": dense_real_nd(&[ny, nu, m], &yo)}))
        };
        let ymax = y.iter().map(|v| v.abs()).fold(0.0, f64::max);
        let heur = (n as f64 * U).max(discrepancy.unwrap_or(0.0)) * ymax.max(f64::MIN_POSITIVE);
        let result = if inp.impulse {
            let feed = continuous && ss.d.iter().any(|v| *v != 0.0);
            if feed {
                warnings.push(Warning::new("W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED", "系统有直通项 D ≠ 0：连续脉冲响应在 t = 0 处含 D·δ(t)，无法用采样值表示，未计入 y；其权重见 result.direct_feedthrough。").path("/system"));
            }
            json!({"t": t_out, "y": y_out, "n_grid_points": n, "direct_feedthrough": if feed { dense_real(&ss.d) } else { Value::Null }})
        } else {
            json!({"t": t_out, "y": y_out, "n_grid_points": n, "info": info_val})
        };
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": if continuous && nx > 0 { json!({"final_state_rel_discrepancy": num(discrepancy.unwrap()), "definition": "‖x_{N−1}(逐步递推) − x(t_end)(一次性 expm 直接计算)‖₂ / ‖x(t_end)‖₂，取所有输入通道最大值"}) } else { Value::Null },
            "condition": {"norm1_A_dt": norm1(&ss.a) * dt, "expm_squarings": squarings, "realization": if inp.kind == "ss" {"given ss"} else {"controllable canonical form (scipy tf2ss convention)"}, if continuous {"max_real_pole"} else {"max_pole_modulus"}: num(max_re)},
            "error_estimate": {"y_abs": num(heur), "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。非严格上界",
                               "info_time_metrics_abs_s": dt, "info_basis": "指标按网格点定义（首个越过阈值的网格点，不插值），相对连续时间真值误差 ≤ dt"},
            "checks": {"asymptotically_stable": stable, "all_finite": y.iter().all(|v| v.is_finite()), "auto_grid": inp.auto},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"dcgain": dcgain.as_ref().map(dense_real).unwrap_or(Value::Null), "poles": complex_vector(&poles), "dt": dt, "t_end": t_end}
        });
        Ok(Output { result, trust, warnings })
    }
}

/// Step metrics with the python-control 0.10 `step_info` definitions (grid-based).
pub fn step_info(y: &[f64], t: &[f64], inf: f64, rise: (f64, f64), thr: f64) -> Value {
    let sgn = inf.signum();
    let q = |unit: &str, v: Option<f64>| json!({"unit": unit, "value": v.map(num).unwrap_or(Value::Null)});
    let lo = y.iter().position(|v| sgn * (v - rise.0 * inf) >= 0.0);
    let hi = y.iter().position(|v| sgn * (v - rise.1 * inf) >= 0.0);
    let rise_time = match (lo, hi) {
        (Some(a), Some(b)) => Some(t[b] - t[a]),
        _ => None,
    };
    let settled = match y.iter().rposition(|v| (v / inf - 1.0).abs() >= thr) {
        Some(k) => k + 1,
        None => 0,
    };
    let settling_time = if settled < t.len() { Some(t[settled]) } else { None };
    let tail = &y[hi.unwrap_or(0)..];
    let smin = tail.iter().cloned().fold(f64::INFINITY, f64::min).min(inf);
    let smax = tail.iter().cloned().fold(f64::NEG_INFINITY, f64::max).max(inf);
    let y_os = y.iter().map(|v| sgn * v).fold(f64::NEG_INFINITY, f64::max);
    let dy = y_os.abs() - inf.abs();
    let overshoot = if dy > 0.0 { (100.0 * dy / inf).abs() } else { 0.0 };
    let mut us_idx = 0;
    for k in 1..y.len() {
        if sgn * y[k] < sgn * y[us_idx] {
            us_idx = k;
        }
    }
    let undershoot = if sgn * y[us_idx] < 0.0 { -100.0 * y[us_idx] / inf } else { 0.0 };
    let mut pk = 0;
    for k in 1..y.len() {
        if y[k].abs() > y[pk].abs() {
            pk = k;
        }
    }
    json!({
        "rise_time": q("s", rise_time), "settling_time": q("s", settling_time),
        "settling_min": q("1", hi.map(|_| smin)), "settling_max": q("1", hi.map(|_| smax)),
        "overshoot": q("%", Some(overshoot)), "undershoot": q("%", Some(undershoot)),
        "peak": q("1", Some(y[pk].abs())), "peak_time": q("s", Some(t[pk])),
        "steady_state": {"unit": "1", "value": num(inf), "source": "dcgain"}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_and_decimate() {
        assert_eq!(grid_len(10.0, 0.01), 1001);
        assert_eq!(grid_len(1.0, 0.3), 4);
        assert_eq!(decimate(1001, 11), (0..11).map(|k| k * 100).collect::<Vec<_>>());
    }
}
