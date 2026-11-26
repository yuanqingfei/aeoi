//! control.design.pidtune (spec v0.4.6, 10.16) against an independent numpy implementation of the spec's
//! frequency-point design, each result verified with python-control 0.10.2 (stability_margins, poles).
//! Golden: golden/python-control/control.design.pidtune/0.4.0 (scripts/golden/gen_golden_pidtune.py). Not MATLAB.
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1.0)
}
fn warned(env: &Value, code: &str) -> bool {
    env["warnings"].as_array().is_some_and(|w| w.iter().any(|x| x["code"] == code))
}

#[test]
fn parity_pidtune() {
    let reg = aeoi_control::registry();
    let op = aeoi_control::find(&reg, "control.design.pidtune").unwrap();
    let dir = spec_root().join("golden/python-control/control.design.pidtune/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err, mut n_extra) = (0, 0, 0);
    let (mut eg, mut ea) = (0.0_f64, 0.0_f64);
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert_ne!(env["status"], "error", "{fam}: {}", env["error"]);
        let r = &env["result"];
        let x = &case["expected"];
        let mut e1 = 0.0_f64;
        for k in ["Kp", "Ki", "Kd", "Tf"] {
            e1 = e1.max(rel(f(&r["gains"][k]), f(&x["gains"][k])));
        }
        assert!(e1 <= 1e-10, "{fam}: gains {e1:e}");
        let a = &x["achieved"];
        assert_eq!(r["achieved"]["closed_loop_stable"], a["closed_loop_stable"], "{fam}");
        let e2 = rel(f(&r["achieved"]["phase_margin_deg"]), f(&a["phase_margin_deg"])).max(rel(f(&r["achieved"]["gain_crossover_rad_s"]), f(&a["gain_crossover_rad_s"])));
        assert!(e2 <= 1e-6, "{fam}: achieved margin / crossover {e2:e}");
        let extra = x["extra_crossover"].as_bool().unwrap();
        assert_eq!(warned(&env, "W.DOMAIN.EXTRA_CROSSOVER"), extra, "{fam}: EXTRA_CROSSOVER warning");
        n_extra += extra as usize;
        for c in ["target_met", "closed_loop_stable", "sub_operator_checks", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: check {c}");
        }
        eg = eg.max(e1);
        ea = ea.max(e2);
    }
    assert!(n >= 30 && n_err >= 9 && n_extra >= 1, "{n} cases, {n_err} errors, {n_extra} extra-crossover");
    let worst = json!({"gains_rel": eg, "achieved_margin_crossover_rel": ea});
    println!("pidtune parity: {n} cases ({n_err} error cases), worst {worst}");
    std::fs::write(spec_root().join("reports/parity-pidtune.json"), serde_json::to_string_pretty(&json!({
        "reference": "numpy implementation of the spec 10.16 formulas, verified with python-control 0.10.2 stability_margins / poles. Not MATLAB.",
        "cases": n, "error_cases": n_err, "extra_crossover_cases": n_extra, "worst": worst,
        "tolerances": {"gains_rel": 1e-10, "achieved_margin_crossover_rel": 1e-6}
    })).unwrap() + "\n").unwrap();
}
