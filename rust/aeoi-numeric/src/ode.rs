//! ode.ivp.solve — explicit Runge–Kutta Dormand–Prince 5(4) for dy/dt = f(t, y) (spec 10.18, v0.4.8).
//!
//! The right-hand side is a list of AEOI expressions (spec 3.15) over named states and parameters. The
//! integrator follows the published formulas of scipy.integrate.RK45 term for term (tableau, RMS error norm,
//! step-size factors 0.9 / 0.2 / 10, Hairer's initial step, Shampine's 4th-order dense output, output points
//! in (t_old, t] evaluated by that step's interpolant), so an independent implementation can follow the same
//! step sequence up to rounding. Global error is estimated by re-solving with rtol/10, atol/10.
use crate::expr::{check_name, eval, expr_err, parse, Node};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Map, Value};

operator_struct!(OdeIvp, "ode.ivp.solve");

// Dormand–Prince 5(4): the coefficients of scipy.integrate.RK45 (Dormand & Prince 1980; P from Shampine 1986).
const C: [f64; 6] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0];
const A: [[f64; 5]; 6] = [
    [0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0 / 5.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0],
    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0],
    [19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0, 0.0],
    [9017.0 / 3168.0, -355.0 / 33.0, 46732.0 / 5247.0, 49.0 / 176.0, -5103.0 / 18656.0],
];
const B: [f64; 6] = [35.0 / 384.0, 0.0, 500.0 / 1113.0, 125.0 / 192.0, -2187.0 / 6784.0, 11.0 / 84.0];
const E: [f64; 7] = [-71.0 / 57600.0, 0.0, 71.0 / 16695.0, -71.0 / 1920.0, 17253.0 / 339200.0, -22.0 / 525.0, 1.0 / 40.0];
const P: [[f64; 4]; 7] = [
    [1.0, -8048581381.0 / 2820520608.0, 8663915743.0 / 2820520608.0, -12715105075.0 / 11282082432.0],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 131558114200.0 / 32700410799.0, -68118460800.0 / 10900136933.0, 87487479700.0 / 32700410799.0],
    [0.0, -1754552775.0 / 470086768.0, 14199869525.0 / 1410260304.0, -10690763975.0 / 1880347072.0],
    [0.0, 127303824393.0 / 49829197408.0, -318862633887.0 / 49829197408.0, 701980252875.0 / 199316789632.0],
    [0.0, -282668133.0 / 205662961.0, 2019193451.0 / 616988883.0, -1453857185.0 / 822651844.0],
    [0.0, 40617522.0 / 29380423.0, -110615467.0 / 29380423.0, 69997945.0 / 29380423.0],
];
const SAFETY: f64 = 0.9;
const MIN_FACTOR: f64 = 0.2;
const MAX_FACTOR: f64 = 10.0;
const ERR_EXP: f64 = -1.0 / 5.0;

/// Python's max(a, b) / min of a list: NaN comparisons are false, so the earlier operand wins.
fn pymax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}
fn pymin(v: &[f64]) -> f64 {
    v.iter().skip(1).fold(v[0], |m, &x| if x < m { x } else { m })
}
fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>()).sqrt() / (v.len() as f64).sqrt()
}

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

#[derive(Debug)]
pub enum Fail {
    NonFiniteAtStart(f64),
    StepTooSmall { t: f64, h: f64, last_non_finite: bool },
    StepLimit { t: f64, steps: usize },
    Cancelled(OpError),
}

pub struct Solution {
    pub y: Vec<Vec<f64>>, // [n_out][n]
    pub n_steps: usize,
    pub n_rejected: usize,
    pub n_rhs: usize,
    pub stiff_at: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
pub fn solve(m: &Model, y0: &[f64], t0: f64, tf: f64, t_eval: &[f64], rtol: f64, atol: f64, max_step: f64, first_step: Option<f64>, max_steps: usize, ctx: Option<&ExecCtx>) -> Result<Solution, Fail> {
    let n = m.n;
    let dir = if tf > t0 { 1.0 } else { -1.0 };
    let mut vars = Vec::with_capacity(1 + n + m.params.len());
    let mut nfev = 0usize;
    let mut f = vec![0.0; n];
    nfev += 1;
    if !m.f(t0, y0, &mut f, &mut vars) {
        return Err(Fail::NonFiniteAtStart(t0));
    }
    // initial step (Hairer–Nørsett–Wanner II.4, scipy select_initial_step, order = 4)
    let mut h_abs = match first_step {
        Some(h) => h,
        None => {
            let interval = (tf - t0).abs();
            let scale: Vec<f64> = y0.iter().map(|v| atol + v.abs() * rtol).collect();
            let d0 = rms(&y0.iter().zip(&scale).map(|(a, s)| a / s).collect::<Vec<_>>());
            let d1 = rms(&f.iter().zip(&scale).map(|(a, s)| a / s).collect::<Vec<_>>());
            let mut h0 = if d0 < 1e-5 || d1 < 1e-5 { 1e-6 } else { 0.01 * d0 / d1 };
            h0 = pymin(&[h0, interval]);
            let y1: Vec<f64> = y0.iter().zip(&f).map(|(a, b)| a + h0 * dir * b).collect();
            let mut f1 = vec![0.0; n];
            m.f(t0 + h0 * dir, &y1, &mut f1, &mut vars);
            nfev += 1;
            let d2 = rms(&f1.iter().zip(&f).zip(&scale).map(|((a, b), s)| (a - b) / s).collect::<Vec<_>>()) / h0;
            let h1 = if d1 <= 1e-15 && d2 <= 1e-15 { pymax(1e-6, h0 * 1e-3) } else { (0.01 / pymax(d1, d2)).powf(1.0 / 5.0) };
            pymin(&[100.0 * h0, h1, interval, max_step])
        }
    };
    let mut t = t0;
    let mut y = y0.to_vec();
    let mut k = vec![vec![0.0; n]; 7];
    let mut out: Vec<Vec<f64>> = Vec::with_capacity(t_eval.len());
    let mut ie = 0usize;
    let (mut n_steps, mut n_rej) = (0usize, 0usize);
    let (mut iasti, mut nonsti, mut stiff_at) = (0usize, 0usize, None);
    let mut ystage = vec![0.0; n];
    let mut ynew = vec![0.0; n];
    while dir * (t - tf) < 0.0 {
        let min_step = 10.0 * (next_toward(t, dir) - t).abs();
        h_abs = if h_abs > max_step { max_step } else if h_abs < min_step { min_step } else { h_abs };
        let (mut accepted, mut rejected) = (false, false);
        let (mut h, mut t_new) = (0.0, t);
        let mut last_non_finite = false;
        while !accepted {
            if h_abs < min_step {
                return Err(Fail::StepTooSmall { t, h: h_abs, last_non_finite });
            }
            h = h_abs * dir;
            t_new = t + h;
            if dir * (t_new - tf) > 0.0 {
                t_new = tf;
            }
            h = t_new - t;
            h_abs = h.abs();
            // stages
            k[0].copy_from_slice(&f);
            let mut finite = true;
            for s in 1..6 {
                for i in 0..n {
                    let mut dot = 0.0;
                    for j in 0..s {
                        dot += k[j][i] * A[s][j];
                    }
                    ystage[i] = y[i] + dot * h;
                }
                finite &= m.f(t + C[s] * h, &ystage, &mut k[s], &mut vars);
                nfev += 1;
            }
            for i in 0..n {
                let mut dot = 0.0;
                for j in 0..6 {
                    dot += k[j][i] * B[j];
                }
                ynew[i] = y[i] + h * dot;
            }
            finite &= m.f(t + h, &ynew, &mut k[6], &mut vars);
            nfev += 1;
            let err = if finite {
                let e: Vec<f64> = (0..n)
                    .map(|i| {
                        let mut dot = 0.0;
                        for j in 0..7 {
                            dot += k[j][i] * E[j];
                        }
                        dot * h / (atol + pymax(y[i].abs(), ynew[i].abs()) * rtol)
                    })
                    .collect();
                rms(&e)
            } else {
                f64::NAN
            };
            last_non_finite = !finite;
            if err < 1.0 {
                let mut factor = if err == 0.0 { MAX_FACTOR } else { pymin(&[MAX_FACTOR, SAFETY * err.powf(ERR_EXP)]) };
                if rejected {
                    factor = pymin(&[1.0, factor]);
                }
                h_abs *= factor;
                accepted = true;
            } else {
                h_abs *= pymax(MIN_FACTOR, SAFETY * err.powf(ERR_EXP));
                rejected = true;
                n_rej += 1;
            }
            if n_steps + n_rej > max_steps {
                return Err(Fail::StepLimit { t, steps: n_steps + n_rej });
            }
        }
        n_steps += 1;
        // stiffness detection (Hairer DOPRI5): every 1000 accepted steps, or while suspicious
        if n_steps % 1000 == 0 || iasti > 0 {
            let stnum: f64 = (0..n).map(|i| (k[6][i] - k[5][i]).powi(2)).sum();
            let stden: f64 = (0..n).map(|i| (ynew[i] - ystage[i]).powi(2)).sum();
            if stden > 0.0 {
                let hlamb = h.abs() * (stnum / stden).sqrt();
                if hlamb > 3.25 {
                    nonsti = 0;
                    iasti += 1;
                    if iasti == 15 && stiff_at.is_none() {
                        stiff_at = Some(t_new);
                    }
                } else {
                    nonsti += 1;
                    if nonsti == 6 {
                        iasti = 0;
                    }
                }
            }
        }
        // output points in (t_old, t_new] (and t0 itself on the first step): this step's dense output
        if ie < t_eval.len() && dir * (t_eval[ie] - t_new) <= 0.0 {
            let q: Vec<[f64; 4]> = (0..n)
                .map(|i| {
                    let mut r = [0.0; 4];
                    for (c, rc) in r.iter_mut().enumerate() {
                        let mut dot = 0.0;
                        for j in 0..7 {
                            dot += k[j][i] * P[j][c];
                        }
                        *rc = dot;
                    }
                    r
                })
                .collect();
            while ie < t_eval.len() && dir * (t_eval[ie] - t_new) <= 0.0 {
                // p = cumprod([x, x, x, x]) = [x, x², x³, x⁴], each power from the previous one
                let x = (t_eval[ie] - t) / h;
                let p2 = x * x;
                let p3 = p2 * x;
                let p = [x, p2, p3, p3 * x];
                out.push((0..n).map(|i| h * (q[i][0] * p[0] + q[i][1] * p[1] + q[i][2] * p[2] + q[i][3] * p[3]) + y[i]).collect());
                ie += 1;
            }
        }
        t = t_new;
        std::mem::swap(&mut y, &mut ynew);
        f.copy_from_slice(&k[6]);
        if let Some(c) = ctx {
            if n_steps % 256 == 0 {
                c.check().map_err(Fail::Cancelled)?;
            }
        }
    }
    Ok(Solution { y: out, n_steps, n_rejected: n_rej, n_rhs: nfev, stiff_at })
}

/// nextafter(t, dir·∞)
fn next_toward(t: f64, dir: f64) -> f64 {
    if t.is_nan() {
        return t;
    }
    let bits = t.to_bits();
    let up = dir > 0.0;
    if t == 0.0 {
        return if up { f64::from_bits(1) } else { -f64::from_bits(1) };
    }
    let next = if (t > 0.0) == up { bits + 1 } else { bits - 1 };
    f64::from_bits(next)
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
        let sol = solve(&i.model, &i.y0, i.t0, i.tf, &i.t_eval, i.rtol, i.atol, i.max_step, i.first_step, max_steps, Some(ctx)).map_err(|f| fail(f, ""))?;
        // global error: the same method with rtol/refine, atol/refine at the same output points
        let fine = solve(&i.model, &i.y0, i.t0, i.tf, &i.t_eval, i.rtol / refine, i.atol / refine, i.max_step, None, max_steps, Some(ctx));
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
        assert_eq!(next_toward(1.0, 1.0), 1.0 + f64::EPSILON);
        assert_eq!(next_toward(1.0, -1.0), 1.0 - f64::EPSILON / 2.0);
    }
}
