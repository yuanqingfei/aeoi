//! control.lti.stepinfo — step-response metrics on an explicit grid (spec 10.17).
//!
//! The samples are the same ZOH recursion as `control.lti.step`. The metrics are the
//! python-control 0.10 grid definition already implemented by `step::step_info` (no interpolation):
//! a time is a grid abscissa, so its error against a continuous crossing is at most dt and is
//! reported as quantization, not as an integration tolerance. Unstable and non-settling systems
//! keep the window peak and set the steady-state metrics to null (spec: null = not applicable,
//! `"Inf"` = unbounded).
use crate::step::{self, Step};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use serde_json::{json, Value};

operator_struct!(StepInfo, "control.lti.stepinfo");

pub struct In {
    sim: step::StepInput,
    rise: (f64, f64),
    settle: f64,
}

fn q(unit: &str, v: Option<f64>) -> Value {
    json!({"unit": unit, "value": v.map(num).unwrap_or(Value::Null)})
}

fn q_ss(v: Option<f64>, infinite: bool) -> Value {
    if infinite {
        json!({"unit": "1", "value": "Inf", "source": "dcgain"})
    } else {
        json!({"unit": "1", "value": v.map(num).unwrap_or(Value::Null), "source": "dcgain"})
    }
}

fn peak_of(y: &[f64], t: &[f64]) -> (f64, f64) {
    let mut pk = 0usize;
    for k in 1..y.len() {
        if y[k].abs() > y[pk].abs() {
            pk = k;
        }
    }
    (y[pk].abs(), t[pk])
}

/// Last-exit settling index, or None if the final sample is still outside the band.
fn settling_index(y: &[f64], inf: f64, thr: f64) -> Option<usize> {
    let outside = |v: f64| (v / inf - 1.0).abs() >= thr;
    match y.iter().rposition(|v| outside(*v)) {
        Some(k) if k + 1 < y.len() => Some(k + 1),
        Some(_) => None,
        None => Some(0),
    }
}

fn channel(y: &[f64], t: &[f64], dc: ChannelDc, rise: (f64, f64), thr: f64) -> (Value, bool, bool) {
    let (pk, pt) = peak_of(y, t);
    match dc {
        ChannelDc::Finite(inf) if inf != 0.0 => {
            let raw = step::step_info(y, t, inf, rise, thr);
            let settled = !raw["settling_time"]["value"].is_null();
            let mut v = raw;
            v["settled"] = json!(settled);
            v["window_limited"] = json!(false);
            (v, settled, false)
        }
        ChannelDc::Finite(0.0) => {
            let v = json!({
                "rise_time": q("s", None), "settling_time": q("s", None),
                "settling_min": q("1", None), "settling_max": q("1", None),
                "overshoot": q("%", None), "undershoot": q("%", None),
                "peak": q("1", Some(pk)), "peak_time": q("s", Some(pt)),
                "steady_state": q_ss(Some(0.0), false),
                "settled": false, "window_limited": false
            });
            (v, false, false)
        }
        ChannelDc::Infinite => {
            let v = json!({
                "rise_time": q("s", None), "settling_time": q("s", None),
                "settling_min": q("1", None), "settling_max": q("1", None),
                "overshoot": q("%", None), "undershoot": q("%", None),
                "peak": q("1", Some(pk)), "peak_time": q("s", Some(pt)),
                "steady_state": q_ss(None, true),
                "settled": false, "window_limited": true
            });
            (v, false, true)
        }
        ChannelDc::Undefined => {
            let v = json!({
                "rise_time": q("s", None), "settling_time": q("s", None),
                "settling_min": q("1", None), "settling_max": q("1", None),
                "overshoot": q("%", None), "undershoot": q("%", None),
                "peak": q("1", Some(pk)), "peak_time": q("s", Some(pt)),
                "steady_state": q_ss(None, false),
                "settled": false, "window_limited": true
            });
            (v, false, true)
        }
        ChannelDc::Finite(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::HostCtx;
    #[test]
    fn first_order_grid_and_unstable() {
        let reg = crate::registry();
        let call = |req: Value| crate::find(&reg, "control.lti.stepinfo").unwrap().call(&req, &HostCtx::default());
        let g = json!({"kind":"tf","num":[1.0],"den":[1.0,1.0],"ts":0.0,"time_unit":"s"});
        let e = call(json!({"system": g, "time": {"mode":"uniform","t_final":5.0,"dt":0.5,"unit":"s"}}));
        assert!(e["status"].as_str().unwrap().starts_with("ok"), "{}", e["error"]);
        // 10–90% of 1-e^{-t} on this grid: t=0.5 and t=2.5
        assert!((e["result"]["info"]["rise_time"]["value"].as_f64().unwrap() - 2.0).abs() < 1e-12);
        assert!((e["result"]["info"]["settling_time"]["value"].as_f64().unwrap() - 4.0).abs() < 1e-12);
        assert_eq!(e["result"]["info"]["overshoot"]["value"], json!(0.0));
        assert_eq!(e["result"]["definition"]["settling_threshold"], json!(0.02));
        assert_eq!(e["receipt"]["evidence"]["claim"], "verified");
        let u = call(json!({"system": {"kind":"tf","num":[1.0],"den":[1.0,-1.0],"ts":0.0}, "time": {"mode":"uniform","t_final":1.0,"dt":0.25,"unit":"s"}}));
        assert!(u["status"].as_str().unwrap().starts_with("ok"), "{}", u["error"]);
        assert!(u["result"]["info"]["rise_time"]["value"].is_null());
        assert!(u["result"]["info"]["peak"]["value"].as_f64().unwrap() > 1.0);
        assert_eq!(u["result"]["info"]["window_limited"], true);
        assert!(u["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.DOMAIN.UNSTABLE_SYSTEM"));
        // 1/s is not asymptotically stable: steady-state metrics are null, the window peak is kept.
        let integ = call(json!({"system": {"kind":"tf","num":[1.0],"den":[1.0,0.0],"ts":0.0}, "time": {"mode":"uniform","t_final":1.0,"dt":0.25,"unit":"s"}}));
        assert!(integ["result"]["info"]["steady_state"]["value"].is_null());
        assert!((integ["result"]["info"]["peak"]["value"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(integ["result"]["info"]["window_limited"], true);
    }
}

#[derive(Clone, Copy)]
enum ChannelDc {
    Finite(f64),
    Infinite,
    Undefined,
}

impl Operator for StepInfo {
    type Input = In;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let rise = args.get("rise_time_limits").and_then(|x| x.as_array()).map(|a| (a[0].as_f64().unwrap(), a[1].as_f64().unwrap())).unwrap_or((0.1, 0.9));
        if !(rise.0.is_finite() && rise.1.is_finite()) || rise.0 >= rise.1 || rise.0 < 0.0 || rise.1 > 1.0 {
            return Err(OpError::new("E.VALIDATION.RANGE", "rise_time_limits 需满足 0 ≤ lo < hi ≤ 1。", "缺省是定义用的 [0.1, 0.9]（10%–90%），不是测量值。")
                .path("/rise_time_limits")
                .remedy(modify_input("default_rise", "使用规范默认 [0.1, 0.9]", "high", false, json!([{"op": "add", "path": "/rise_time_limits", "value": [0.1, 0.9]}]))));
        }
        let settle = args.get("settling_threshold").and_then(|x| x.as_f64()).unwrap_or(0.02);
        if !(settle.is_finite() && settle > 0.0 && settle <= 0.5) {
            return Err(OpError::new("E.VALIDATION.RANGE", "settling_threshold 需满足 0 < 阈值 ≤ 0.5。", "缺省 0.02 是 2% 调节带的定义，不是测量值。")
                .path("/settling_threshold")
                .remedy(modify_input("default_settle", "使用规范默认 0.02", "high", false, json!([{"op": "add", "path": "/settling_threshold", "value": 0.02}]))));
        }
        let req = json!({
            "system": args["system"],
            "time": args["time"],
            "step_info": {"enabled": true, "rise_time_limits": [rise.0, rise.1], "settling_threshold": settle},
            "output": {"return_samples": "all"}
        });
        let (sim, w) = Step::new().validate(&req, host)?;
        Ok((In { sim, rise, settle }, w))
    }
    fn estimate(&self, _i: &In) -> Option<Value> {
        Some(json!({"note": "same ZOH recursion as control.lti.step"}))
    }
    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let out = Step::new().execute(i.sim, ctx)?;
        let mut warnings: Vec<Warning> = out.warnings.into_iter().filter(|w| matches!(w.code.as_str(), "W.ACCURACY.AUTO_GRID" | "W.ACCURACY.GRID_TOO_COARSE" | "W.VALIDATION.LEADING_ZEROS_STRIPPED" | "W.PERFORMANCE.SPARSE_DENSIFIED")).collect();
        let yv = &out.result["y"]["value"]["data"];
        let tv = &out.result["t"]["value"]["data"];
        let y: Vec<f64> = yv.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        let t: Vec<f64> = tv.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        let shape: Vec<usize> = out.result["y"]["value"]["shape"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
        let (ny, nu, n) = (shape[0], shape[1], shape[2]);
        let stable = out.trust["checks"]["asymptotically_stable"].as_bool().unwrap_or(false);
        let dc = &out.trust["derived"]["dcgain"];
        let dc_at = |ii: usize, jj: usize| -> ChannelDc {
            if !stable {
                return ChannelDc::Undefined;
            }
            if dc.is_null() {
                return ChannelDc::Infinite;
            }
            let data = dc["data"].as_array().unwrap();
            match &data[ii * nu + jj] {
                Value::Number(v) => ChannelDc::Finite(v.as_f64().unwrap()),
                Value::String(s) if s == "Inf" || s == "-Inf" => ChannelDc::Infinite,
                _ => ChannelDc::Undefined,
            }
        };
        if !stable {
            warnings.push(Warning::new("W.DOMAIN.UNSTABLE_SYSTEM", "系统不是渐近稳定的。依赖稳态值的指标（上升、调节、超调、欠调）为 null；peak / peak_time 只在给定时间窗内，window_limited = true。").path("/system"));
        }
        let mut zero_dc = 0;
        let mut not_settled = 0;
        let mut infinite = 0;
        let mut settling_ok = true;
        let mut rows = Vec::new();
        for ii in 0..ny {
            let mut row = Vec::new();
            for jj in 0..nu {
                let yy = &y[(ii * nu + jj) * n..(ii * nu + jj + 1) * n];
                let kind = dc_at(ii, jj);
                let finite = match kind {
                    ChannelDc::Finite(v) => Some(v),
                    ChannelDc::Infinite => {
                        infinite += 1;
                        None
                    }
                    ChannelDc::Undefined => None,
                };
                if finite == Some(0.0) {
                    zero_dc += 1;
                }
                let (v, settled, _) = channel(yy, &t, kind, i.rise, i.settle);
                if let Some(inf) = finite.filter(|x| *x != 0.0) {
                    match settling_index(yy, inf, i.settle) {
                        Some(k) => {
                            let inside = (yy[k] / inf - 1.0).abs() < i.settle;
                            let prev_out = k == 0 || (yy[k - 1] / inf - 1.0).abs() >= i.settle;
                            let reported = v["settling_time"]["value"].as_f64();
                            settling_ok &= inside && prev_out && reported == Some(t[k]);
                        }
                        None => {
                            not_settled += 1;
                            settling_ok &= v["settling_time"]["value"].is_null();
                        }
                    }
                    let _ = settled;
                }
                row.push(v);
            }
            rows.push(Value::Array(row));
        }
        if infinite > 0 {
            warnings.push(Warning::new("W.DOMAIN.INFINITE_DC_GAIN", "DC 增益无穷（例如含积分器）。steady_state 为 \"Inf\"，上升/调节时间无定义。").path("/system"));
        }
        if zero_dc > 0 {
            warnings.push(Warning::new("W.DOMAIN.ZERO_DC_GAIN", format!("{zero_dc} 个通道的 DC 增益为 0，上升/调节/超调无定义。")).path("/system"));
        }
        if not_settled > 0 {
            warnings.push(Warning::new("W.DOMAIN.NOT_SETTLED", format!("{not_settled} 个通道在 t_final 内没有留在调节带里，settling_time 为 null。这不是 \"Inf\"：加大 t_final 后仍可能调节。")).path("/time/t_final"));
        }
        let info = if ny == 1 && nu == 1 { rows[0][0].clone() } else { Value::Array(rows) };
        let dt = if n >= 2 { t[1] - t[0] } else { 0.0 };
        let definition = json!({
            "rise_time_limits": [i.rise.0, i.rise.1],
            "settling_threshold": i.settle,
            "time_quantization": "grid_point",
            "overshoot_unit": "%",
            "steady_state_source": "dcgain",
            "note": "上升/调节时间取第一个越过阈值的网格点，不插值。与连续时间穿越点之差 ≤ dt，记在 error_estimate.time_quantization_s，不占 T2 的 rtol。"
        });
        let finite = y.iter().all(|v| v.is_finite());
        let heur = out.trust["error_estimate"]["y_abs"].clone();
        let result = json!({
            "definition": definition,
            "stable": stable,
            "n_grid_points": n,
            "info": info
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": out.trust["residual"].clone(),
            "condition": {"dt": dt, "max_real_or_modulus": out.trust["condition"].get("max_real_pole").or_else(|| out.trust["condition"].get("max_pole_modulus")).cloned().unwrap_or(Value::Null)},
            "error_estimate": {
                "y_abs": heur,
                "time_quantization_s": num(dt),
                "basis": "采样值沿用 step 的 ZOH 递推。时间类指标是网格横坐标：相对连续穿越的误差 ≤ dt，这是定义的量化，不是积分误差。"
            },
            "checks": {"samples_finite": finite, "settling_matches_definition": settling_ok},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"dcgain": dc.clone(), "dt": dt}
        });
        Ok(Output { result, trust, warnings })
    }
}
