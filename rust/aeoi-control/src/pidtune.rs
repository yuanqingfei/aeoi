//! control.design.pidtune — PI/PID tuning by crossover frequency and phase margin (spec 10.16, v0.4.6).
//!
//! Spec A-4 asks a tuning operator to declare its object class, objective, uniqueness and failure codes
//! before it can exist. Here: SISO continuous proper plants; |L(jωc)| = 1 and ∠L(jωc) = −180° + PM with ωc
//! supplied by the caller; PI is the unique closed-form solution, PID fixes the third degree of freedom with
//! Ti = α·Td and Tf = Td/N and takes the smallest Td on a specified scan; E.DOMAIN.TARGET_NOT_ACHIEVABLE when
//! the required controller phase is out of reach or the resulting loop is unstable.
//! The controller is built by control.design.pid and the loop analysed by control.analysis.loopsens on the
//! very same JSON, so chaining the primitives gives bitwise-identical numbers (K-4).
use crate::loopsens::sub;
use crate::mimo_err;
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::lti::{parse_lti, Lti, C64};
use aeoi_core::sysalg::eval_tf;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(PidTune, "control.design.pidtune");

const U: f64 = f64::EPSILON / 2.0;
const SCAN_K: usize = 4000; // x_k = 10^{-6 + 12k/K}, k = 0..K
const MAX_BISECT: usize = 200;
const TARGET_MAG_TOL: f64 = 1e-9; // | |L(jωc)| − 1 |
const TARGET_PHASE_TOL_DEG: f64 = 1e-7;

pub struct In {
    plant: Lti,
    plant_json: Value,
    pid: bool,
    wc: f64,
    pm: f64,
    alpha: f64,
    n: f64,
    lineage: Value,
}

fn present(args: &Value, k: &str) -> bool {
    args.get(k).is_some_and(|v| !v.is_null())
}

/// H(x) = 1 − j/(αx) + jx/(1 + jx/N): the standard-form PID with Ti = α·Td, Tf = Td/N at s = jωc, x = ωc·Td,
/// divided by Kp. Re H > 0 for every x > 0, so arg H ∈ (−90°, 90°) and needs no unwrapping.
fn h(x: f64, alpha: f64, n: f64) -> C64 {
    let jx = C64::new(0.0, x);
    C64::new(1.0, 0.0) + C64::new(0.0, -1.0 / (alpha * x)) + jx / (C64::new(1.0, 0.0) + jx / n)
}

fn deg(z: C64) -> f64 {
    z.arg().to_degrees()
}

fn not_achievable(msg: String, hint: &str, details: Value) -> OpError {
    let mut e = OpError::new("E.DOMAIN.TARGET_NOT_ACHIEVABLE", msg, hint).path("/wc_rad_s");
    if let Value::Object(m) = details {
        for (k, v) in m {
            e = e.detail(&k, v);
        }
    }
    e.remedy(ask_user("change_target", "与用户确认：降低 ωc（更慢的响应）、降低相位裕度目标，或换控制器类型"))
}

impl Operator for PidTune {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let plant = parse_lti(&args["plant"], "/plant", host, &mut w, self.meta.max_dense("/plant/A"))?;
        let (ny, nu) = plant.dims();
        if ny != 1 || nu != 1 {
            return Err(mimo_err("/plant", "control.design.pidtune", ny, nu));
        }
        if plant.ts() > 0.0 {
            return Err(OpError::new("E.DOMAIN.NOT_CONTINUOUS", format!("plant 是离散系统（ts = {}）。v0.4.6 的 pidtune 只整定连续对象。", plant.ts()), "用连续时间模型整定，再用 control.lti.c2d 把控制器离散化（ts 由用户给出）。")
                .path("/plant/ts"));
        }
        let pid = args["type"] == "PID";
        if !pid {
            let extra: Vec<&str> = ["ti_td_ratio", "derivative_filter_n"].into_iter().filter(|k| present(args, k)).collect();
            if !extra.is_empty() {
                let patch: Vec<Value> = extra.iter().map(|k| json!({"op": "remove", "path": format!("/{k}")})).collect();
                return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("type = PI 没有微分项，{} 只用于 PID。不能静默忽略（F-8）。", extra.join("、")), "去掉这些字段，或把 type 改成 PID。")
                    .path(&format!("/{}", extra[0]))
                    .detail("conflicting_fields", json!(extra))
                    .detail("type", json!("PI"))
                    .remedy(modify_input("drop", "去掉只用于 PID 的字段", "high", false, json!(patch))));
            }
        }
        let f = |k: &str, d: f64| args.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
        Ok((
            In {
                plant,
                plant_json: args["plant"].clone(),
                pid,
                wc: args["wc_rad_s"].as_f64().unwrap(),
                pm: f("phase_margin_deg", 60.0),
                alpha: f("ti_td_ratio", 4.0),
                n: f("derivative_filter_n", 10.0),
                lineage: args.get("lineage").cloned().unwrap_or(Value::Null),
            },
            w,
        ))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let s = C64::new(0.0, i.wc);
        let g = eval_tf(&i.plant, s).map(|m| m[(0, 0)]).filter(|z| z.re.is_finite() && z.im.is_finite() && z.norm() > 0.0);
        let Some(g) = g else {
            return Err(not_achievable(
                format!("对象在 jωc = j{} 处为零或为极点（G(jωc) = 0 或无穷），任何有限的控制器都不能使 |L(jωc)| = 1 且相位确定。", i.wc),
                "换一个 ωc（离开对象在虚轴上的零点或极点）。",
                json!({"wc_rad_s": i.wc}),
            ));
        };
        let target = C64::from_polar(1.0, (i.pm - 180.0).to_radians());
        let c_req = target / g;
        let theta = deg(c_req);
        let plant_phase = deg(g);
        let (gains, gains_std, td_search) = if !i.pid {
            // PI: C(jω) = Kp − j·Ki/ω (unique)
            let kp = c_req.re;
            let mut ki = -i.wc * c_req.im;
            // ∠C* = 0° is achievable (pure P); a negative Ki within rounding of 0 must not flip that (10.16 PT-3)
            if ki < 0.0 && ki >= -4.0 * U * i.wc * c_req.norm() {
                ki = 0.0;
            }
            if !(kp > 0.0 && ki >= 0.0) {
                let hint = if theta > 0.0 { "所需控制器相位为超前，PI 只能提供 (−90°, 0°]：改用 type = PID，或降低 ωc / 降低相位裕度目标。" } else { "所需控制器相位滞后超过 90°，PI 给不了：提高 ωc 或降低相位裕度目标（对象在 ωc 处的相位滞后不够）。" };
                return Err(not_achievable(
                    format!("PI 达不到目标：在 ωc = {} rad/s 处需要控制器相位 {:.4}°，PI 的可达范围是 (−90°, 0°]（对象相位 {:.4}°，目标相位裕度 {}°）。", i.wc, theta, plant_phase, i.pm),
                    hint,
                    json!({"required_controller_phase_deg": theta, "achievable_range_deg": [-90.0, 0.0], "plant_phase_deg": plant_phase, "type": "PI"}),
                ));
            }
            (json!({"form": "parallel", "Kp": kp, "Ki": ki, "Kd": 0.0, "Tf": 0.0}), json!({"Kp": kp, "Ti": if ki > 0.0 { num(kp / ki) } else { Value::Null }, "Td": 0.0, "Tf": 0.0}), Value::Null)
        } else {
            // PID: smallest x = ωc·Td on the specified log scan with arg H(x) = θ, then bisection in log x
            let phi = |x: f64| deg(h(x, i.alpha, i.n));
            let xs: Vec<f64> = (0..=SCAN_K).map(|k| 10f64.powf(-6.0 + 12.0 * k as f64 / SCAN_K as f64)).collect();
            let gs: Vec<f64> = xs.iter().map(|&x| phi(x) - theta).collect();
            let (lo_ph, hi_ph) = gs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v + theta), b.max(v + theta)));
            let Some(k) = (0..SCAN_K).find(|&k| gs[k] == 0.0 || gs[k].signum() != gs[k + 1].signum()) else {
                return Err(not_achievable(
                    format!("PID（Ti = {}·Td，N = {}）达不到目标：在 ωc = {} rad/s 处需要控制器相位 {:.4}°，可达范围约为 [{:.4}°, {:.4}°]（对象相位 {:.4}°）。", i.alpha, i.n, i.wc, theta, lo_ph, hi_ph, plant_phase),
                    "降低 ωc 或相位裕度目标；需要更多超前时可增大 derivative_filter_n 或减小 ti_td_ratio。",
                    json!({"required_controller_phase_deg": theta, "achievable_range_deg": [lo_ph, hi_ph], "plant_phase_deg": plant_phase, "type": "PID", "ti_td_ratio": i.alpha, "derivative_filter_n": i.n}),
                ));
            };
            let (mut lo, mut hi) = (xs[k], xs[k + 1]);
            let glo = gs[k];
            let mut it = 0;
            if glo != 0.0 {
                while (hi - lo) > 4.0 * U * hi && it < MAX_BISECT {
                    let mid = (lo * hi).sqrt();
                    let gm = phi(mid) - theta;
                    if gm == 0.0 {
                        lo = mid;
                        hi = mid;
                        break;
                    }
                    if gm.signum() == glo.signum() {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                    it += 1;
                }
                if (hi - lo) > 4.0 * U * hi {
                    return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("Td 的二分在 {MAX_BISECT} 次内没有收敛（区间相对宽度 {:.3e}）。", (hi - lo) / hi), "报告此用例；可试另一个 ωc。").path("/wc_rad_s"));
                }
            } else {
                hi = lo;
            }
            let x = if glo == 0.0 { lo } else { (lo * hi).sqrt() };
            ctx.check()?;
            let hx = h(x, i.alpha, i.n);
            let kp = c_req.norm() / hx.norm();
            let td = x / i.wc;
            let ti = i.alpha * td;
            let tf = td / i.n;
            let search = json!({"variable": "x = ωc·Td", "scan": {"points": SCAN_K + 1, "log10_range": [-6, 6]}, "bracket_index": k,
                                "bisection_iterations": it, "bracket_rel_width": num((hi - lo) / hi.max(f64::MIN_POSITIVE)), "x": x,
                                "achievable_phase_range_deg": [lo_ph, hi_ph], "rule": "smallest root on the scan"});
            (json!({"form": "parallel", "Kp": kp, "Ki": kp / ti, "Kd": kp * td, "Tf": tf}), json!({"Kp": kp, "Ti": ti, "Td": td, "Tf": tf}), search)
        };
        // controller via control.design.pid, loop via control.analysis.loopsens (same JSON, K-4)
        let host = HostCtx::default();
        let mut pid_args = json!({"form": "parallel", "Kp": gains["Kp"], "Ki": gains["Ki"]});
        if i.pid {
            pid_args["Kd"] = gains["Kd"].clone();
            pid_args["Tf"] = gains["Tf"].clone();
        }
        let (pid_res, pid_ok) = sub(&crate::pid::Pid::new(), &pid_args, "/controller", "control.design.pid", &host, &mut warnings);
        let controller = pid_res["system"].clone();
        if controller.is_null() {
            return Err(OpError::new("E.INTERNAL.PANIC", "control.design.pid 没有返回控制器。", "报告此用例。").retryable(false));
        }
        let (ls, ls_ok) = sub(&crate::loopsens::LoopSens::new(), &json!({"plant": i.plant_json, "controller": controller}), "/achieved", "control.analysis.loopsens", &host, &mut warnings);
        ctx.check()?;
        let stable = ls["closed_loop_stable"].as_bool().unwrap_or(false);
        let poles = ls["closed_loop_poles"].clone();
        if !stable {
            return Err(not_achievable(
                format!("在 ωc = {} rad/s 处满足了 |L| = 1、相位裕度 {}°，但闭环不稳定（L 还有别的穿越或对象不稳定的极点没被包围）。频率点条件只约束一个频率。", i.wc, i.pm),
                "降低 ωc，或提高相位裕度目标；对不稳定对象先确认目标是否合理。",
                json!({"closed_loop_poles": poles, "gains": gains}),
            ));
        }
        // post-condition: L(jωc) of the built controller meets the target (independent of the gain formulas)
        let c_built = aeoi_core::lti::parse_lti(&controller, "/controller", &host, &mut Vec::new(), 1_000_000)
            .ok()
            .and_then(|c| eval_tf(&c, s))
            .map(|m| m[(0, 0)]);
        let (mag_err, ph_err) = match c_built {
            Some(c) => {
                let l = g * c;
                let mut d = deg(l) - (i.pm - 180.0);
                d = (d + 180.0).rem_euclid(360.0) - 180.0;
                ((l.norm() - 1.0).abs(), d.abs())
            }
            None => (f64::INFINITY, f64::INFINITY),
        };
        let target_met = mag_err <= TARGET_MAG_TOL && ph_err <= TARGET_PHASE_TOL_DEG;
        let m = &ls["margins"];
        let qv = |v: &Value| v.get("value").cloned().unwrap_or(Value::Null);
        let wgc = qv(&m["gain_crossover_frequency"]);
        let pm_ach = qv(&m["phase_margin"]);
        if let (Some(w), Some(p)) = (wgc.as_f64(), pm_ach.as_f64()) {
            if (w - i.wc).abs() > 1e-6 * i.wc || p < i.pm - 1e-6 {
                warnings.push(
                    Warning::new("W.DOMAIN.EXTRA_CROSSOVER", format!("L 还有别的增益穿越点：实际相位裕度（所有穿越点里最小的）为 {p:.6}°，在 ω = {w:.6} rad/s，小于目标 {}°。ωc 处的条件已满足。", i.pm))
                        .path("/achieved/phase_margin_deg")
                        .details(json!({"target_wc_rad_s": i.wc, "binding_crossover_rad_s": w, "phase_margin_deg": p})),
                );
            }
        }
        let finite = [&gains["Kp"], &gains["Ki"], &gains["Kd"], &gains["Tf"]].iter().all(|v| v.as_f64().is_some_and(|x| x.is_finite()));
        let result = json!({
            "type": if i.pid { "PID" } else { "PI" },
            "controller": controller,
            "gains": gains,
            "gains_standard": gains_std,
            "target": {"wc_rad_s": i.wc, "phase_margin_deg": i.pm},
            "achieved": {"closed_loop_stable": stable, "gain_crossover_rad_s": wgc, "phase_margin_deg": pm_ach, "gain_margin": qv(&m["gain_margin"])},
            "closed_loop_poles": poles,
            "design": {"method": "frequency_point", "required_controller_phase_deg": theta, "required_controller_gain": c_req.norm(), "plant_at_wc": {"magnitude": g.norm(), "phase_deg": plant_phase},
                       "ti_td_ratio": if i.pid { json!(i.alpha) } else { Value::Null }, "derivative_filter_n": if i.pid { json!(i.n) } else { Value::Null }, "td_search": td_search,
                       "lineage": i.lineage}
        });
        let trust = json!({
            "convergence": if i.pid { json!({"applicable": true, "converged": true, "iterations": result["design"]["td_search"]["bisection_iterations"], "criterion": "bracket relative width ≤ 4u"}) } else { json!({"applicable": false}) },
            "residual": {"target_magnitude_abs": num(mag_err), "target_phase_deg_abs": num(ph_err), "definition": "用 control.design.pid 构造的控制器重新计算 L(jωc)：||L| − 1| 与 |∠L − (PM − 180°)|"},
            "condition": {"plant_magnitude_at_wc": g.norm()},
            "error_estimate": {"gains_rel": num(64.0 * U), "basis": "PI 为封闭式；PID 的 Td 由二分到 4u 的相对宽度。增益误差主要来自 G(jωc) 的求值。非严格上界；达到的裕度误差见 margin（T3）"},
            "checks": {"target_met": target_met, "closed_loop_stable": stable, "sub_operator_checks": pid_ok && ls_ok, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"sub_operators": ["control.design.pid", "control.analysis.loopsens"], "target_tolerances": {"magnitude": TARGET_MAG_TOL, "phase_deg": TARGET_PHASE_TOL_DEG}}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        PidTune::new().call(&args, &HostCtx::default())
    }
    fn tf(num: &[f64], den: &[f64]) -> Value {
        json!({"kind": "tf", "num": num, "den": den, "ts": 0.0})
    }

    /// PI on 1/(s+1) at ωc = 1, PM = 60°: G(j) = (1 − j)/2, C* = e^{−j120°}/G ⇒ closed form.
    #[test]
    fn pi_first_order_closed_form() {
        let e = call(json!({"plant": tf(&[1.0], &[1.0, 1.0]), "type": "PI", "wc_rad_s": 1.0}));
        assert_eq!(e["status"], "ok", "{e}");
        let g = C64::new(0.5, -0.5);
        let c = C64::from_polar(1.0, (-120f64).to_radians()) / g;
        let r = &e["result"];
        assert!((r["gains"]["Kp"].as_f64().unwrap() - c.re).abs() < 1e-14);
        assert!((r["gains"]["Ki"].as_f64().unwrap() + c.im).abs() < 1e-14);
        assert_eq!(e["trust"]["checks"]["target_met"], true);
        assert!((r["achieved"]["phase_margin_deg"].as_f64().unwrap() - 60.0).abs() < 1e-6);
        assert!((r["achieved"]["gain_crossover_rad_s"].as_f64().unwrap() - 1.0).abs() < 1e-6);
    }

    /// 1/(s+1)^3 at ωc = 1 needs phase lead: PI is not achievable, PID is.
    #[test]
    fn lead_needs_pid() {
        let p = tf(&[1.0], &[1.0, 3.0, 3.0, 1.0]);
        let e = call(json!({"plant": p, "type": "PI", "wc_rad_s": 1.0}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.TARGET_NOT_ACHIEVABLE", "{e}");
        assert!((e["error"]["details"]["required_controller_phase_deg"].as_f64().unwrap() - 15.0).abs() < 1e-9);
        let e = call(json!({"plant": p, "type": "PID", "wc_rad_s": 1.0}));
        assert_eq!(e["status"], "ok", "{e}");
        let st = &e["result"]["gains_standard"];
        assert!((st["Ti"].as_f64().unwrap() - 4.0 * st["Td"].as_f64().unwrap()).abs() < 1e-12 * st["Ti"].as_f64().unwrap());
        assert_eq!(e["trust"]["checks"]["target_met"], true);
        assert_eq!(e["trust"]["convergence"]["converged"], true);
    }

    /// Boundary ∠C* = 0° (1/(s+1)³, ωc = 1, PM = 45°: plant phase −135°): pure P, not an error.
    #[test]
    fn pi_boundary_is_pure_p() {
        let e = call(json!({"plant": tf(&[1.0], &[1.0, 3.0, 3.0, 1.0]), "type": "PI", "wc_rad_s": 1.0, "phase_margin_deg": 45}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["gains"]["Ki"].as_f64().unwrap(), 0.0);
        assert!(e["result"]["gains_standard"]["Ti"].is_null());
        assert!((e["result"]["gains"]["Kp"].as_f64().unwrap() - 2f64.powf(1.5)).abs() < 1e-12);
    }

    #[test]
    fn option_conflict_and_domain() {
        let p = tf(&[1.0], &[1.0, 1.0]);
        let e = call(json!({"plant": p, "type": "PI", "wc_rad_s": 1.0, "ti_td_ratio": 4}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert_eq!(e["error"]["details"]["conflicting_fields"], json!(["ti_td_ratio"]));
        let e = call(json!({"plant": {"kind": "tf", "num": [1.0], "den": [1.0, -0.5], "ts": 0.1}, "type": "PI", "wc_rad_s": 1.0}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.NOT_CONTINUOUS");
        let e = call(json!({"plant": p, "type": "PI"}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SCHEMA");
        // plant zero on the imaginary axis at ωc
        let e = call(json!({"plant": tf(&[1.0, 0.0, 4.0], &[1.0, 3.0, 3.0, 1.0]), "type": "PID", "wc_rad_s": 2.0}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.TARGET_NOT_ACHIEVABLE");
    }
}
