//! control.lti.bode — frequency response (spec 10.1).
use crate::{gamma, U};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::linalg::{cond2_c, sort_poles};
use aeoi_core::lti::{parse_lti, Lti, C64};
use aeoi_core::matrix::{complex_vector, dense_real_nd};
use aeoi_core::{num, opt_num, ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};
use std::f64::consts::PI;

pub struct Bode {
    meta: OperatorMeta,
}

impl Bode {
    pub fn new() -> Self {
        Bode {
            meta: OperatorMeta::from_json(
                include_str!("../../../examples/control.lti.bode/manifest.json"),
                include_str!("../../../examples/control.lti.bode/input.schema.json"),
                include_str!("../../../examples/control.lti.bode/output.schema.json"),
            ),
        }
    }
}

impl Default for Bode {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BodeInput {
    pub sys: Lti,
    pub w: Vec<f64>,
    pub unwrap: bool,
    pub center: f64,
}

/// Map an angle (deg) into (c-180, c+180].
pub fn wrap_to(p: f64, c: f64) -> f64 {
    p - 360.0 * ((p - c - 180.0) / 360.0).ceil()
}

/// Horner evaluation with the running condition sum Σ|a_k||s|^k (Higham 2002 §5.1).
fn horner(p: &[f64], s: C64) -> (C64, f64) {
    let mut v = C64::new(0.0, 0.0);
    let mut mag = 0.0;
    let sa = s.norm();
    for &a in p {
        v = v * s + a;
        mag = mag * sa + a.abs();
    }
    (v, mag)
}

impl Operator for Bode {
    type Input = BodeInput;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }

    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(BodeInput, Vec<Warning>), OpError> {
        let mut warnings = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut warnings, self.meta.max_dense("/system/A"))?;
        let f = &args["frequency"];
        let scale = if f["unit"] == "Hz" { 2.0 * PI } else { 1.0 };
        let mut w: Vec<f64> = match f["mode"].as_str() {
            Some("explicit") => {
                let v: Vec<f64> = f["values"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
                if let Some(i) = (1..v.len()).find(|&i| v[i] <= v[i - 1]) {
                    let mut sorted = v.clone();
                    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    sorted.dedup();
                    return Err(OpError::new("E.VALIDATION.GRID", format!("频率点必须严格递增（第 {i} 个点不大于前一个）；相位展开依赖频率顺序。"), "对频率点升序排序并去重后重试。")
                        .path(&format!("/frequency/values/{i}"))
                        .remedy(modify_input("sort", "升序排序并去重", "high", false, json!([{"op": "replace", "path": "/frequency/values", "value": sorted}]))));
                }
                v
            }
            _ => {
                let (lo, hi, n) = (f["min"].as_f64().unwrap(), f["max"].as_f64().unwrap(), f["n"].as_u64().unwrap() as usize);
                if lo >= hi {
                    return Err(OpError::new("E.VALIDATION.RANGE", "logspace 要求 min < max。", "交换 min 与 max。")
                        .path("/frequency")
                        .remedy(modify_input("swap", "交换 min 与 max", "high", false, json!([{"op": "replace", "path": "/frequency/min", "value": hi}, {"op": "replace", "path": "/frequency/max", "value": lo}]))));
                }
                let (a, b) = (lo.log10(), hi.log10());
                (0..n).map(|k| 10f64.powf(a + (b - a) * k as f64 / (n - 1) as f64)).collect()
            }
        };
        for x in w.iter_mut() {
            *x *= scale;
        }
        let ts = sys.ts();
        if ts > 0.0 {
            let nyq = PI / ts;
            if let Some(i) = w.iter().position(|&x| x > nyq * (1.0 + 1e-12)) {
                let mut e = OpError::new("E.DOMAIN.ABOVE_NYQUIST", format!("离散系统（ts = {ts} s）的频率响应只定义到 Nyquist 频率 π/ts = {nyq:.6} rad/s，第 {i} 个频率点超出。"), "去掉高于 π/ts 的频率点，或把上限设为 π/ts。")
                    .path("/frequency")
                    .detail("nyquist_rad_s", json!(nyq));
                e = if f["mode"] == "explicit" {
                    let kept: Vec<Value> = f["values"].as_array().unwrap().iter().filter(|x| x.as_f64().unwrap() * scale <= nyq).cloned().collect();
                    e.remedy(modify_input("drop", "删除高于 Nyquist 的频率点", "high", false, json!([{"op": "replace", "path": "/frequency/values", "value": kept}])))
                } else {
                    e.remedy(modify_input("cap", "把上限设为 Nyquist 频率", "high", false, json!([{"op": "replace", "path": "/frequency/max", "value": nyq / scale}])))
                };
                return Err(e);
            }
        }
        let ph = &args["phase"];
        Ok((
            BodeInput { sys, w, unwrap: ph.get("unwrap").and_then(|x| x.as_bool()).unwrap_or(true), center: ph.get("branch_center_deg").and_then(|x| x.as_f64()).unwrap_or(0.0) },
            warnings,
        ))
    }

    fn estimate(&self, i: &BodeInput) -> Option<Value> {
        let (ny, nu) = i.sys.dims();
        Some(json!({"frequency_points": i.w.len(), "output_values": 2 * ny * nu * i.w.len()}))
    }

    fn execute(&self, inp: BodeInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (ny, nu) = inp.sys.dims();
        let nw = inp.w.len();
        let ts = inp.sys.ts();
        let s_of = |w: f64| if ts > 0.0 { C64::from_polar(1.0, w * ts) } else { C64::new(0.0, w) };
        let mut h = vec![C64::new(0.0, 0.0); ny * nu * nw];
        let mut rel_bound = vec![0.0f64; nw]; // first-order relative error bound / estimate per frequency
        let mut max_cond_ss: Option<f64> = None;
        let mut warnings = Vec::new();
        let pole_err = |k: usize, w: f64| {
            OpError::new("E.NUMERICAL.POLE_ON_GRID", format!("频率点 ω = {w} rad/s 与系统极点重合（或在数值上无法区分），该点频率响应为无穷大。"), "去掉或微移该频率点后重试；若需要谐振峰值，使用相邻频率。")
                .path(&format!("/frequency/values/{k}"))
                .detail("frequency_rad_s", json!(w))
        };
        let (estimate_kind, basis) = match &inp.sys {
            Lti::Tf { num, den, .. } => {
                for k in 0..nw {
                    let s = s_of(inp.w[k]);
                    for i in 0..ny {
                        for j in 0..nu {
                            let (nv, nm) = horner(&num[i][j], s);
                            let (dv, dm) = horner(&den[i][j], s);
                            let bd = gamma(2 * den[i][j].len()) * dm / dv.norm();
                            if dv.norm() == 0.0 || bd >= 0.5 {
                                return Err(pole_err(k, inp.w[k]));
                            }
                            let bn = if nv.norm() == 0.0 { 0.0 } else { gamma(2 * num[i][j].len()) * nm / nv.norm() };
                            h[(i * nu + j) * nw + k] = nv / dv;
                            rel_bound[k] = rel_bound[k].max(bn + bd + U);
                        }
                    }
                    if k % 4096 == 0 {
                        ctx.check()?;
                    }
                }
                ("a_priori_bound", "Horner 先验误差界（一阶）：|Δp/p| ≤ γ_{2n}·Σ|a_k||s|^k/|p(s)|（Higham 2002 §5.1），分子分母相加；dB 误差 ≈ 8.686×相对误差，相位误差 ≈ 57.30°×相对误差")
            }
            Lti::Zpk { z, p, k: gain, .. } => {
                for k in 0..nw {
                    let s = s_of(inp.w[k]);
                    let mut v = C64::new(*gain, 0.0);
                    let mut b = 0.0;
                    for zz in z {
                        let d = s - zz;
                        if d.norm() != 0.0 {
                            b += U * (s.norm() + zz.norm()) / d.norm();
                        }
                        v *= d;
                    }
                    for pp in p {
                        let d = s - pp;
                        if d.norm() == 0.0 || U * (s.norm() + pp.norm()) / d.norm() >= 0.5 {
                            return Err(pole_err(k, inp.w[k]));
                        }
                        b += U * (s.norm() + pp.norm()) / d.norm();
                        v /= d;
                    }
                    h[k] = v;
                    rel_bound[k] = b + (z.len() + p.len() + 1) as f64 * U;
                }
                ("a_priori_bound", "乘积形式先验误差界（一阶）：Σ u·(|s|+|r|)/|s−r| + (n_z+n_p+1)·u；dB 误差 ≈ 8.686×相对误差，相位误差 ≈ 57.30°×相对误差")
            }
            Lti::Ss { a, b, c, d, .. } => {
                let n = a.nrows();
                let ac = a.map(|x| C64::new(x, 0.0));
                let bc = b.map(|x| C64::new(x, 0.0));
                let cc = c.map(|x| C64::new(x, 0.0));
                let dc = d.map(|x| C64::new(x, 0.0));
                let use_svd = n <= self.meta.manifest["limits"]["max_states_for_svd_condition"].as_u64().unwrap_or(64) as usize;
                for k in 0..nw {
                    let s = s_of(inp.w[k]);
                    let g = if n == 0 {
                        dc.clone()
                    } else {
                        let m = DMatrix::<C64>::identity(n, n) * s - &ac;
                        let x = m.clone().lu().solve(&bc).ok_or_else(|| pole_err(k, inp.w[k]))?;
                        if use_svd {
                            let kappa = cond2_c(&m);
                            if !kappa.is_finite() || kappa * U * n as f64 >= 0.5 {
                                return Err(pole_err(k, inp.w[k]));
                            }
                            max_cond_ss = Some(max_cond_ss.unwrap_or(0.0).max(kappa));
                            rel_bound[k] = 4.0 * n as f64 * U * kappa;
                        }
                        &cc * x + &dc
                    };
                    for i in 0..ny {
                        for j in 0..nu {
                            h[(i * nu + j) * nw + k] = g[(i, j)];
                        }
                    }
                    if k % 256 == 0 {
                        ctx.check()?;
                    }
                }
                if use_svd {
                    ("heuristic", "启发式估计 4·n·u·κ₂(sI−A)（逐频点），不是严格上界；dB 误差 ≈ 8.686×相对误差")
                } else {
                    ("none", "n 超过 max_states_for_svd_condition，未估计")
                }
            }
        };
        // magnitude & phase
        let mut mag = vec![0.0; h.len()];
        let mut phase = vec![0.0; h.len()];
        let mut zero_hits = 0usize;
        for ch in 0..ny * nu {
            let mut prev_arg: Option<f64> = None;
            let mut prev_phase = 0.0;
            for k in 0..nw {
                let idx = ch * nw + k;
                let v = h[idx];
                if v.norm() == 0.0 {
                    mag[idx] = f64::NEG_INFINITY;
                    phase[idx] = f64::NAN;
                    zero_hits += 1;
                    continue;
                }
                mag[idx] = 20.0 * v.norm().log10();
                let arg = v.im.atan2(v.re).to_degrees();
                let ph = match (inp.unwrap, prev_arg) {
                    (true, Some(pa)) => prev_phase + wrap_to(arg - pa, 0.0),
                    _ => wrap_to(arg, inp.center),
                };
                phase[idx] = ph;
                prev_arg = Some(arg);
                prev_phase = ph;
            }
        }
        if zero_hits > 0 {
            warnings.push(Warning::new("W.NUMERICAL.ZERO_ON_GRID", format!("{zero_hits} 个点的频率响应恰为 0（频率点落在传输零点上），幅值记为 \"-Inf\" dB，相位记为 \"NaN\"。")).path("/frequency"));
        }
        let (worst_k, worst) = rel_bound.iter().cloned().enumerate().fold((0, 0.0), |a, (k, b)| if b > a.1 { (k, b) } else { a });
        if estimate_kind != "none" && worst > 1e-6 {
            warnings.push(Warning::new("W.NUMERICAL.NEAR_POLE", format!("ω = {} rad/s 附近求值条件差，相对误差估计 {:.2e}。", inp.w[worst_k], worst)).path("/frequency").details(json!({"frequency_rad_s": inp.w[worst_k], "rel_error_estimate": worst})));
        }
        let mut poles = inp.sys.poles();
        sort_poles(&mut poles);
        let all_finite = mag.iter().all(|x| x.is_finite()) && phase.iter().all(|x| x.is_finite());
        let est = if estimate_kind == "none" {
            Value::Null
        } else {
            json!({"kind": estimate_kind, "rel_max": worst, "magnitude_db_abs": 20.0 / std::f64::consts::LN_10 * worst, "phase_deg_abs": worst.to_degrees(), "worst_frequency_rad_s": inp.w[worst_k], "basis": basis})
        };
        let result = json!({
            "w": {"unit": "rad/s", "value": dense_real_nd(&[nw], &inp.w)},
            "magnitude": {"unit": "dB", "value": dense_real_nd(&[ny, nu, nw], &mag)},
            "phase": {"unit": "deg", "value": dense_real_nd(&[ny, nu, nw], &phase)},
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": Value::Null,
            "condition": {"representation": inp.sys.kind(), "max_cond_sI_minus_A": opt_num(max_cond_ss), "max_rel_error_estimate": num(worst)},
            "error_estimate": est,
            "checks": {"all_finite": all_finite, "frequencies_strictly_increasing": true},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"poles": complex_vector(&poles), "phase_convention": if inp.unwrap {"unwrapped"} else {"wrapped"}, "branch_center_deg": inp.center}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrap() {
        assert_eq!(wrap_to(180.0, 0.0), 180.0);
        assert_eq!(wrap_to(-180.0, 0.0), 180.0);
        assert_eq!(wrap_to(190.0, 0.0), -170.0);
        assert_eq!(wrap_to(-90.0, -180.0), -90.0);
        assert_eq!(wrap_to(10.0, -180.0), -350.0);
    }
}
