//! control.analysis.loopsens (spec v0.3.6, K-4/K-5) against python-control 0.10.2: the one-shot loop
//! analysis must equal the chained primitives series → feedback → stability_margins → bandwidth →
//! step_info. Golden: golden/python-control/control.analysis.loopsens/0.4.0 (gen_golden_loopsens.py). Not MATLAB.
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};

fn norm(num: &[f64], den: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let a = den[0];
    (num.iter().map(|x| x / a).collect(), den.iter().map(|x| x / a).collect())
}
fn rel_coeff(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    let scale = b.iter().map(|x| x.abs()).fold(1.0_f64, f64::max);
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max) / scale
}
fn tf_err(got: &Value, exp: &Value) -> f64 {
    let (gn, gd) = norm(&fvec(&got["num"]), &fvec(&got["den"]));
    let (en, ed) = norm(&fvec(&exp["num"]), &fvec(&exp["den"]));
    rel_coeff(&gn, &en).max(rel_coeff(&gd, &ed))
}
fn q(v: &Value) -> Option<f64> {
    if v.is_null() { None } else { let x = f(&v["value"]); x.is_finite().then_some(x) }
}
fn rel(a: Option<f64>, b: &Value) -> f64 {
    let b = if b.is_null() { None } else { Some(f(b)) };
    match (a, b) {
        (None, None) => 0.0,
        (Some(x), Some(y)) => (x - y).abs() / y.abs().max(1.0),
        _ => f64::INFINITY,
    }
}

#[test]
fn parity_loopsens() {
    let reg = aeoi_control::registry();
    let op = aeoi_control::find(&reg, "control.analysis.loopsens").unwrap();
    let dir = spec_root().join("golden/python-control/control.analysis.loopsens/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut worst) = (0, json!({}));
    let (mut e_tf, mut e_freq, mut e_step) = (0.0_f64, 0.0_f64, 0.0_f64);
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}");
            continue;
        }
        assert_ne!(env["status"], "error", "{fam}: {}", env["error"]);
        let r = &env["result"];
        let x = &case["expected"];
        let et = tf_err(&r["open_loop"], &x["open_loop"]).max(tf_err(&r["closed_loop"], &x["closed_loop"]));
        assert!(et <= 1e-10, "{fam}: L/T coefficients {et:e}");
        assert_eq!(r["closed_loop_stable"], x["closed_loop_stable"], "{fam}");
        let mut ef = rel(q(&r["gain_crossover_rad_s"]), &x["gain_crossover_rad_s"]);
        ef = ef.max(rel(q(&r["margins"]["phase_margin"]), &x["phase_margin"]));
        if let Some(bw) = x.get("bandwidth_rad_s") {
            ef = ef.max(rel(q(&r["bandwidth_rad_s"]), bw));
        }
        assert!(ef <= 1e-6, "{fam}: crossover / margin / bandwidth {ef:e}");
        // K-5: the Hz fields are the rad/s fields / 2π
        if let (Some(w), Some(h)) = (q(&r["bandwidth_rad_s"]), q(&r["bandwidth_hz"])) {
            assert!((w / (2.0 * std::f64::consts::PI) - h).abs() <= 1e-12 * w.max(1.0));
        }
        let mut es = 0.0_f64;
        if let Some(s) = x.get("step") {
            let info = &r["step_metrics"]["info"];
            for k in ["rise_time", "settling_time", "overshoot", "peak", "steady_state"] {
                es = es.max(rel(q(&info[k]), &s[k]));
            }
            assert!(es <= 1e-8, "{fam}: step metrics {es:e}");
        }
        e_tf = e_tf.max(et);
        e_freq = e_freq.max(ef);
        e_step = e_step.max(es);
        worst = json!({"coefficients": e_tf, "crossover_margin_bandwidth": e_freq, "step_metrics": e_step});
    }
    assert!(n >= 12);
    println!("loopsens parity: {n} cases, worst {worst}");
    std::fs::write(spec_root().join("reports/parity-loopsens.json"), serde_json::to_string_pretty(&json!({
        "reference": "python-control 0.10.2 (series, feedback, stability_margins, bandwidth, step_info). Not MATLAB.",
        "cases": n, "worst_relative_error": worst,
        "tolerances": {"coefficients": 1e-10, "crossover_margin_bandwidth": 1e-6, "step_metrics": 1e-8}
    })).unwrap() + "\n").unwrap();
}
