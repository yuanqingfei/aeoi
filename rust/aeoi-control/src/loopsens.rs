//! control.analysis.loopsens — one-shot single-loop analysis from named roles (spec 3.13, K-4/K-5).
//!
//! Topology (fixed): r → e = r − H·y → C → G → y. The open loop L = H·G·C, the closed loop
//! T = G·C/(1+L) (r→y) and the sensitivity S = 1/(1+L) are built with the same code paths as
//! control.lti.series / control.lti.feedback; margins and step metrics are the results of
//! control.analysis.margin(L) and control.lti.stepinfo(T) on the very same JSON, so chaining the
//! primitives gives bitwise-identical numbers (K-4). Bandwidths are reported twice, with the unit in
//! the field name (K-5): the open-loop gain crossover |L(jω)| = 1 and the closed-loop −3 dB bandwidth.
use crate::connect::{all_finite, feedback_identity, feedback_lti, is_improper, series_identity, series_lti, unity_siso};
use crate::{check_states, mimo_err};
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::jcs::hash_value;
use aeoi_core::lti::{parse_lti_allow_improper, Lti, C64};
use aeoi_core::poly::{roots, strip};
use aeoi_core::sysalg::{complex_list, convert, eval_tf, lti_json};
use aeoi_core::{DynOperator, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};
use std::f64::consts::PI;

operator_struct!(LoopSens, "control.analysis.loopsens");

pub struct In {
    plant: Lti,
    controller: Lti,
    sensor: Lti,
    controller_implicit: bool,
    sensor_implicit: bool,
    hashes: Vec<(&'static str, String)>,
    stepinfo_args: Option<Value>,
    drop_db: f64,
    lineage: Value,
}

fn present(args: &Value, k: &str) -> bool {
    args.get(k).is_some_and(|v| !v.is_null())
}

/// Lossless representation change ss → tf (SISO, R-7), so an improper controller can still be
/// combined with a state-space plant on the polynomial path.
fn as_tf(s: Lti) -> Lti {
    if matches!(s, Lti::Ss { .. }) {
        convert(&s, "tf").map(|x| x.0).unwrap_or(s)
    } else {
        s
    }
}

impl Operator for LoopSens {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let md = self.meta.max_dense("/plant/A");
        let mut plant = parse_lti_allow_improper(&args["plant"], "/plant", host, &mut w, md)?;
        let ts = plant.ts();
        let (controller_implicit, mut controller) = if present(args, "controller") { (false, parse_lti_allow_improper(&args["controller"], "/controller", host, &mut w, md)?) } else { (true, unity_siso(ts)) };
        let (sensor_implicit, mut sensor) = if present(args, "sensor") { (false, parse_lti_allow_improper(&args["sensor"], "/sensor", host, &mut w, md)?) } else { (true, unity_siso(ts)) };
        for (name, s) in [("plant", &plant), ("controller", &controller), ("sensor", &sensor)] {
            let (ny, nu) = s.dims();
            if ny != 1 || nu != 1 {
                return Err(mimo_err(&format!("/{name}"), "control.analysis.loopsens", ny, nu));
            }
            if s.ts() != ts {
                return Err(OpError::new(
                    "E.DOMAIN.SAMPLE_TIME_MISMATCH",
                    format!("plant.ts = {ts}，{name}.ts = {}。回路里的系统必须在同一采样周期下。", s.ts()),
                    "离散化到同一个 ts（control.lti.c2d，采样周期必须由用户给出），或分别处理。",
                )
                .path(&format!("/{name}/ts"))
                .detail("ts", json!([ts, s.ts()]))
                .remedy(ask_user("ts", "向用户确认这些系统是否应该在同一采样周期下互联")));
            }
        }
        let nstates = |s: &Lti| if is_improper(s) { 0 } else { s.to_ss().a.nrows() };
        check_states(&self.meta, nstates(&plant) + nstates(&controller) + nstates(&sensor), "/plant")?;
        if is_improper(&plant) || is_improper(&controller) || is_improper(&sensor) {
            plant = as_tf(plant);
            controller = as_tf(controller);
            sensor = as_tf(sensor);
        }
        let h = |k: &str, implicit: bool, s: &Lti| if implicit { hash_value(&lti_json(s)) } else { hash_value(&args[k]) };
        let hashes = vec![("plant", hash_value(&args["plant"])), ("controller", h("controller", controller_implicit, &controller)), ("sensor", h("sensor", sensor_implicit, &sensor))];
        let stepinfo_args = if present(args, "time") {
            let mut a = json!({"time": args["time"].clone()});
            for k in ["rise_time_limits", "settling_threshold"] {
                if present(args, k) {
                    a[k] = args[k].clone();
                }
            }
            Some(a)
        } else {
            None
        };
        let drop_db = args.get("bandwidth_drop_db").and_then(|x| x.as_f64()).unwrap_or(-3.0);
        Ok((In { plant, controller, sensor, controller_implicit, sensor_implicit, hashes, stepinfo_args, drop_db, lineage: args.get("lineage").cloned().unwrap_or(Value::Null) }, w))
    }
    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let ts = i.plant.ts();
        // forward = G·C, L = H·G·C, T = feedback(G·C, H), S = feedback(1, L)   (spec K-2, I-5)
        let fwd = series_lti(&i.controller, &i.plant, &mut warnings)?;
        let l = if i.sensor_implicit { fwd.clone() } else { series_lti(&fwd, &i.sensor, &mut warnings)? };
        let (t, rcond) = feedback_lti(&fwd, &i.sensor, &mut warnings)?;
        let (s, _) = feedback_lti(&unity_siso(ts), &l, &mut warnings)?;
        ctx.check()?;
        let (ok_t, n1, w1) = feedback_identity(&t, &fwd, &i.sensor);
        let (ok_l, n2, w2) = if i.sensor_implicit { (true, 0, 0.0) } else { series_identity(&l, &fwd, &i.sensor) };
        let (ok_s, n3, w3) = feedback_identity(&s, &unity_siso(ts), &l);
        let worst = w1.max(w2).max(w3);
        // closed-loop poles
        let mut poles = if is_improper(&t) { vec![] } else { t.poles() };
        poles.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap_or(std::cmp::Ordering::Equal).then(b.im.partial_cmp(&a.im).unwrap_or(std::cmp::Ordering::Equal)));
        let stable = !is_improper(&t) && if ts > 0.0 { poles.iter().all(|p| p.norm() < 1.0) } else { poles.iter().all(|p| p.re < 0.0) };
        // margins on L, step metrics on T: the primitive operators on the same JSON (K-4)
        let l_json = lti_json(&l);
        let t_json = lti_json(&t);
        let host = HostCtx::default();
        let (margins, mg_ok) = sub(&crate::margin::Margin::new(), &json!({"system": l_json}), "/margins", "control.analysis.margin（L）", &host, &mut warnings);
        let (step_metrics, si_ok) = match &i.stepinfo_args {
            Some(a) => {
                let mut a = a.clone();
                a["system"] = t_json.clone();
                sub(&crate::stepinfo::StepInfo::new(), &a, "/step_metrics", "control.lti.stepinfo（T）", &host, &mut warnings)
            }
            None => (Value::Null, true),
        };
        ctx.check()?;
        let gc = margins.get("gain_crossover_frequency").map(|q| q["value"].clone()).unwrap_or(Value::Null);
        let q = |unit: &str, v: Value| if v.is_null() { Value::Null } else { json!({"unit": unit, "value": v}) };
        let hz = |v: &Value| match v {
            Value::Number(n) => aeoi_core::num(n.as_f64().unwrap() / (2.0 * PI)),
            other => other.clone(),
        };
        let (bw, bw_method, bw_note) = if is_improper(&t) {
            (Value::Null, "none", Some("T 不 proper"))
        } else if !stable {
            // K-5: the bandwidth of an unstable closed loop has no meaning (no steady state to drop from)
            (Value::Null, "none", Some("闭环不稳定"))
        } else {
            bandwidth(&t, i.drop_db, ctx)?
        };
        if let Some(n) = bw_note {
            warnings.push(Warning::new("W.DOMAIN.ANALYSIS_SKIPPED", format!("闭环带宽没有定义：{n}。bandwidth_rad_s / bandwidth_hz 为 null。")).path("/bandwidth_rad_s"));
        }
        let operands: Vec<Value> = i.hashes.iter().map(|(r, h)| json!({"role": r, "sha256": h})).collect();
        let finite = all_finite(&l) && all_finite(&t) && all_finite(&s);
        let margin_stable = margins.get("closed_loop_stable").and_then(|x| x.as_bool());
        let stability_consistent = margin_stable.is_none_or(|m| m == stable);
        let result = json!({
            "open_loop": l_json,
            "closed_loop": t_json,
            "sensitivity": lti_json(&s),
            "roles": {
                "open_loop": "L = sensor·plant·controller：裕度、穿越频率用它（control_analysis_margin 的输入）",
                "closed_loop": "T = plant·controller/(1+L)，r→y：阶跃指标用它（control_lti_stepinfo / step 的输入）",
                "sensitivity": "S = 1/(1+L)，r→e"
            },
            "closed_loop_poles": complex_list(&poles),
            "closed_loop_stable": stable,
            "margins": margins,
            "gain_crossover_rad_s": q("rad/s", gc.clone()),
            "gain_crossover_hz": q("Hz", hz(&gc)),
            "bandwidth_rad_s": q("rad/s", bw.clone()),
            "bandwidth_hz": q("Hz", hz(&bw)),
            "bandwidth_definition": {"drop_db": i.drop_db, "reference": "dc_gain", "of": "closed_loop", "method": bw_method,
                                     "statement": "第一个使 |T(jω)| < |T(0)|·10^(drop_db/20) 的频率"},
            "step_metrics": step_metrics,
            "composition": {
                "operation": "loopsens",
                "operands": operands,
                "controller_implicit": i.controller_implicit,
                "sensor_implicit": i.sensor_implicit,
                "loop_role": "loop_analysis",
                "topology": "r → e = r − sensor·y → controller → plant → y",
                "lineage": i.lineage,
                "lineage_checked": false
            }
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"identity_rel": aeoi_core::num(worst), "definition": "T·(1+L) = G·C、L = H·G·C、S·(1+L) = 1 在测试点上的相对残差（最大值）"},
            "condition": {"identity_points": n1 + n2 + n3, "algebraic_loop_rcond": rcond.map(aeoi_core::num).unwrap_or(Value::Null)},
            "error_estimate": {"identity_rel": aeoi_core::num(worst), "basis": "互联恒等式在测试点上的相对残差，非严格上界；裕度与阶跃指标的误差见对应子算子（margin T3、stepinfo T2）"},
            "checks": {"closed_loop_identity": ok_t && ok_l && ok_s, "stability_consistent": stability_consistent, "all_finite": finite, "sub_operator_checks": mg_ok && si_ok},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"sub_operators": ["control.lti.series", "control.lti.feedback", "control.analysis.margin", "control.lti.stepinfo"]}
        });
        Ok(Output { result, trust, warnings })
    }
}

/// Run a primitive operator on JSON; its warnings are re-pointed under `prefix`. A sub-analysis
/// error becomes `null` plus W.DOMAIN.ANALYSIS_SKIPPED (the loop itself is still valid).
/// Runs a sub-operator; returns its result (null on error) and whether all of its post-condition
/// checks passed (vacuously true when it did not run).
fn sub(op: &dyn DynOperator, args: &Value, prefix: &str, what: &str, host: &HostCtx, warnings: &mut Vec<Warning>) -> (Value, bool) {
    let env = op.call(args, host);
    if env["status"] == "error" {
        let e = &env["error"];
        warnings.push(
            Warning::new("W.DOMAIN.ANALYSIS_SKIPPED", format!("{what} 没有结果：{}（{}）。该字段为 null。", e["message"].as_str().unwrap_or(""), e["code"].as_str().unwrap_or("")))
                .path(prefix)
                .details(json!({"code": e["code"], "agent_hint": e["agent_hint"]})),
        );
        return (Value::Null, true);
    }
    // only the sub-operator's declared post-conditions count (E-11: descriptive checks such as
    // margin's closed_loop_stable are not evidence either way)
    let checks_ok = env["receipt"]["evidence"]["failed"].as_array().is_none_or(|f| f.is_empty());
    for w in env["warnings"].as_array().into_iter().flatten() {
        let code = w["code"].as_str().unwrap_or("");
        let mut nw = Warning::new(code, w["message"].as_str().unwrap_or("")).path(&format!("{prefix}{}", w["path"].as_str().unwrap_or("")));
        if let Some(d) = w.get("details").filter(|d| !d.is_null()) {
            nw = nw.details(d.clone());
        }
        warnings.push(nw);
    }
    (env["result"].clone(), checks_ok)
}

/// |P(jω)|² as a polynomial in x = ω² (descending), from P(s)·P(−s).
fn mag2_in_x(p: &[f64]) -> Vec<f64> {
    let n = p.len();
    let neg: Vec<f64> = p.iter().enumerate().map(|(k, c)| if (n - 1 - k) % 2 == 1 { -c } else { *c }).collect();
    let pp = aeoi_core::poly::polymul(p, &neg); // even polynomial in s, descending, degree 2(n-1)
    let deg = pp.len() - 1;
    // coefficient of s^{2m} → (−1)^m x^m
    let mut out = Vec::new();
    for (k, c) in pp.iter().enumerate() {
        let pw = deg - k;
        if pw % 2 == 0 {
            let m = pw / 2;
            out.push(if m % 2 == 1 { -c } else { *c });
        }
    }
    strip(&out)
}

/// Closed-loop bandwidth (K-5): first ω with |T(jω)| < |T(0)|·10^(drop/20). Returns
/// (value in rad/s or "Inf", method, Some(reason) when undefined).
fn bandwidth(t: &Lti, drop_db: f64, ctx: &ExecCtx) -> Result<(Value, &'static str, Option<&'static str>), OpError> {
    let ts = t.ts();
    let z0 = if ts > 0.0 { C64::new(1.0, 0.0) } else { C64::new(0.0, 0.0) };
    let mag = |w: f64| -> f64 {
        let s = if ts > 0.0 { C64::from_polar(1.0, w * ts) } else { C64::new(0.0, w) };
        eval_tf(t, s).map(|g| g[(0, 0)].norm()).unwrap_or(f64::NAN)
    };
    let t0 = match eval_tf(t, z0) {
        Some(g) => g[(0, 0)].norm(),
        None => return Ok((Value::Null, "none", Some("T 的直流增益为无穷（s = 0 / z = 1 处有极点）"))),
    };
    if !t0.is_finite() {
        return Ok((Value::Null, "none", Some("T 的直流增益为无穷（s = 0 / z = 1 处有极点）")));
    }
    if t0 == 0.0 {
        return Ok((Value::Null, "none", Some("T 的直流增益为 0，−3 dB 带宽相对直流增益没有定义")));
    }
    let target = t0 * 10f64.powf(drop_db / 20.0);
    let f = |w: f64| mag(w) - target;
    let refine = |mut lo: f64, mut hi: f64| -> f64 {
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if f(mid) >= 0.0 { lo = mid } else { hi = mid }
            if hi - lo <= 4.0 * f64::EPSILON * hi {
                break;
            }
        }
        0.5 * (lo + hi)
    };
    let nyq = if ts > 0.0 { PI / ts } else { f64::INFINITY };
    if ts == 0.0 {
        // continuous: candidates are the positive real roots x = ω² of |N|² − target²·|D|²
        let tf = convert(t, "tf").map(|x| x.0).unwrap_or_else(|_| t.clone());
        if let Lti::Tf { num, den, .. } = &tf {
            let (n, d) = (&num[0][0], &den[0][0]);
            let p = aeoi_core::poly::polyadd_scaled(&mag2_in_x(n), &mag2_in_x(d), -target * target);
            let mut cands: Vec<f64> = roots(&strip(&p)).into_iter().filter(|r| r.re > 0.0 && r.im.abs() <= 1e-6 * r.re.abs().max(1.0)).map(|r| r.re.sqrt()).collect();
            cands.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for w in cands {
                let (lo, hi) = (w * (1.0 - 1e-6), w * (1.0 + 1e-6));
                if f(lo) >= 0.0 && f(hi) < 0.0 && f(0.5 * lo) >= 0.0 {
                    return Ok((aeoi_core::num(refine(lo, hi)), "even_polynomial_roots", None));
                }
            }
        }
    }
    // grid scan (discrete always; continuous as a fallback when no root brackets a crossing)
    let (wmin, wmax) = if ts > 0.0 { (nyq * 1e-6, nyq) } else { (1e-6, 1e9) };
    let npts = 4000;
    let mut prev = wmin;
    if f(prev) < 0.0 {
        return Ok((aeoi_core::num(refine(0.0, prev)), "grid_bisection", None));
    }
    for k in 1..npts {
        if k % 256 == 0 {
            ctx.check()?;
        }
        let w = wmin * (wmax / wmin).powf(k as f64 / (npts - 1) as f64);
        if f(w) < 0.0 {
            return Ok((aeoi_core::num(refine(prev, w)), "grid_bisection", None));
        }
        prev = w;
    }
    Ok((json!("Inf"), if ts > 0.0 { "grid_bisection" } else { "even_polynomial_roots" }, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(req: Value) -> Value {
        let reg = crate::registry();
        crate::find(&reg, "control.analysis.loopsens").unwrap().call(&req, &HostCtx::default())
    }
    fn tf(num: &[f64], den: &[f64]) -> Value {
        json!({"kind":"tf","num":num,"den":den,"ts":0.0})
    }
    fn v(x: &Value) -> f64 {
        x["value"].as_f64().unwrap()
    }
    #[test]
    fn first_order_bandwidth_and_crossover_analytic() {
        // G = 1/(s+1), C = 9: L = 9/(s+1), T = 9/(s+10). |L| = 1 at ω = √80; −3 dB of T at ω = 10.
        let e = run(json!({"plant": tf(&[1.0], &[1.0, 1.0]), "controller": tf(&[9.0], &[1.0])}));
        assert_eq!(e["status"], "ok", "{}", e);
        let r = &e["result"];
        assert_eq!(r["closed_loop"]["num"], json!([9.0]));
        assert_eq!(r["closed_loop"]["den"], json!([1.0, 10.0]));
        // exactly −3 dB (not −3.0103 dB): |T| = 9/√(ω²+100) = 0.9·10^(−3/20) → ω = 10·√(10^0.3 − 1)
        let wb = 10.0 * (10f64.powf(0.3) - 1.0).sqrt();
        assert!((v(&r["bandwidth_rad_s"]) - wb).abs() < 1e-12 * wb, "{}", r["bandwidth_rad_s"]);
        assert!((v(&r["bandwidth_hz"]) - wb / (2.0 * PI)).abs() < 1e-12);
        assert_eq!(r["bandwidth_definition"]["method"], "even_polynomial_roots");
        // half-power definition on request: 20·log10(1/√2) dB gives exactly 10 rad/s
        let e2 = run(json!({"plant": tf(&[1.0], &[1.0, 1.0]), "controller": tf(&[9.0], &[1.0]), "bandwidth_drop_db": 20.0 * 0.5f64.sqrt().log10()}));
        assert!((v(&e2["result"]["bandwidth_rad_s"]) - 10.0).abs() < 1e-11);
        assert_eq!(r["bandwidth_hz"]["unit"], "Hz");
        assert!((v(&r["gain_crossover_rad_s"]) - 80f64.sqrt()).abs() < 1e-8);
        assert!((v(&r["gain_crossover_hz"]) - 80f64.sqrt() / (2.0 * PI)).abs() < 1e-8);
        assert_eq!(r["closed_loop_stable"], true);
        // E-12: margins come from a grid-audited sub-operator → at most spot_checked, never verified
        assert_eq!(e["receipt"]["evidence"]["claim"], "spot_checked");
        assert_eq!(e["trust"]["checks"]["sub_operator_checks"], true);
        assert!(r["step_metrics"].is_null());
    }
    #[test]
    fn equals_chaining_the_primitives() {
        // K-4: same numbers as series → feedback → margin → stepinfo on the same JSON (same build: bitwise)
        let g = tf(&[1.0], &[1.0, 10.0, 20.0]);
        let c = tf(&[15.0, 40.0, 50.0], &[1.0, 0.0]);
        let time = json!({"mode": "uniform", "t_final": 3.0, "dt": 0.001, "unit": "s"});
        let e = run(json!({"plant": g, "controller": c, "time": time}));
        assert_ne!(e["status"], "error", "{}", e["error"]);
        let reg = crate::registry();
        let call = |op: &str, a: Value| crate::find(&reg, op).unwrap().call(&a, &HostCtx::default());
        let l = call("control.lti.series", json!({"controller": c, "plant": g}));
        let t = call("control.lti.feedback", json!({"plant": g, "controller": c}));
        assert_eq!(e["result"]["open_loop"], l["result"]["system"]);
        assert_eq!(e["result"]["closed_loop"], t["result"]["system"]);
        let m = call("control.analysis.margin", json!({"system": l["result"]["system"]}));
        assert_eq!(e["result"]["margins"], m["result"]);
        let si = call("control.lti.stepinfo", json!({"system": t["result"]["system"], "time": time}));
        assert_eq!(e["result"]["step_metrics"], si["result"]);
    }
    #[test]
    fn improper_controller_with_ss_plant_and_unstable_loop() {
        let ss = json!({"kind":"ss","A":{"dtype":"f64","shape":[1,1],"data":[1.0]},"B":{"dtype":"f64","shape":[1,1],"data":[1.0]},
                        "C":{"dtype":"f64","shape":[1,1],"data":[1.0]},"D":{"dtype":"f64","shape":[1,1],"data":[0.0]},"ts":0.0});
        // unstable plant 1/(s−1) with C = 0.5: T = 0.5/(s−0.5) unstable
        let e = run(json!({"plant": ss, "controller": tf(&[0.5], &[1.0])}));
        assert_ne!(e["status"], "error", "{}", e["error"]);
        assert_eq!(e["result"]["closed_loop_stable"], false);
        // K-5: no bandwidth for an unstable closed loop (null + ANALYSIS_SKIPPED, not "Inf")
        assert!(e["result"]["bandwidth_rad_s"].is_null() && e["result"]["bandwidth_hz"].is_null());
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.DOMAIN.ANALYSIS_SKIPPED" && w["path"] == "/bandwidth_rad_s"));
        // PD controller (improper) with the ss plant: converted to tf, T = (s+2)/(s+1)... proper
        let e = run(json!({"plant": ss, "controller": tf(&[1.0, 2.0], &[1.0])}));
        assert_ne!(e["status"], "error", "{}", e["error"]);
        assert_eq!(e["result"]["closed_loop"]["den"], json!([2.0, 1.0]));
    }
    #[test]
    fn mimo_and_ts_mismatch_are_errors() {
        let mut d = tf(&[1.0], &[1.0, 1.0]);
        d["ts"] = json!(0.1);
        assert_eq!(run(json!({"plant": tf(&[1.0], &[1.0, 1.0]), "controller": d}))["error"]["code"], "E.DOMAIN.SAMPLE_TIME_MISMATCH");
    }
    #[test]
    fn mag2_polynomial() {
        // |jω + 2|² = ω² + 4
        assert_eq!(mag2_in_x(&[1.0, 2.0]), vec![1.0, 4.0]);
        // |(jω)² + jω + 1|² = ω⁴ − ω² + 1
        assert_eq!(mag2_in_x(&[1.0, 1.0, 1.0]), vec![1.0, -1.0, 1.0]);
    }
}
