//! ode.ivp.solve (spec v0.4.8, 10.18) against scipy.integrate.solve_ivp(RK45) with the same settings
//! (the spec copies RK45's formulas) and against the truth (analytic, or DOP853 at rtol 1e-13).
//! Golden: golden/python-control/ode.ivp.solve/0.4.0 (gen_golden_ode.py; right-hand sides evaluated by an
//! independent Python parser of 3.15). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn warned(env: &Value, code: &str) -> bool {
    env["warnings"].as_array().is_some_and(|w| w.iter().any(|x| x["code"] == code))
}

#[test]
fn parity_ode() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "ode.ivp.solve").unwrap();
    let dir = spec_root().join("golden/python-control/ode.ivp.solve/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err) = (0, 0);
    let (mut worst_vs_scipy, mut worst_acc_ratio) = (0.0_f64, 0.0_f64);
    let mut est_ratios: Vec<f64> = Vec::new();
    let mut rows = Vec::new();
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap().to_string();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert_ne!(env["status"], "error", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let req = &case["request"];
        let rtol = req.get("rtol").and_then(|v| v.as_f64()).unwrap_or(1e-3);
        let atol = req.get("atol").and_then(|v| v.as_f64()).unwrap_or(1e-6);
        assert_eq!(env["result"]["y"]["shape"], x["shape"], "{fam}");
        let got = fvec(&env["result"]["y"]["data"]);
        let (want, truth) = (fvec(&x["y"]), fvec(&x["truth"]));
        // Closeness to scipy RK45 is recorded, not asserted: where the global error exceeds the local tolerance
        // (kinks, long horizons, chaos) two correct runs diverge as soon as one step decision differs.
        let vs_scipy = got.iter().zip(&want).map(|(a, b)| (a - b).abs() / (atol + rtol * b.abs())).fold(0.0, f64::max);
        // T4 (v0.4.8): against the truth, |y − truth| ≤ max(10·(atol + rtol·|truth|), 2·E_ref + 1e-13·max|y|),
        // E_ref = the reference RK45's own max global error on this case
        let sc_err = x["scipy_global_error_max_abs"].as_f64().unwrap();
        let scale = truth.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
        let my_err = got.iter().zip(&truth).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        let acc_ratio = got
            .iter()
            .zip(&truth)
            .map(|(a, b)| (a - b).abs() / (10.0 * (atol + rtol * b.abs())).max(2.0 * sc_err + 1e-13 * scale))
            .fold(0.0, f64::max);
        // OD-5 verdict equals the one the generator predicted with scipy RK45 (same estimator, same step sequence)
        assert_eq!(env["trust"]["checks"]["tolerance_consistent"], x["tolerance_consistent"], "{fam}: tolerance_consistent (scipy max_scaled {})", x["scipy_max_scaled_vs_fine"]);
        assert!(acc_ratio <= 1.0, "{fam}: global error {my_err:.3e} (scipy RK45: {sc_err:.3e}) exceeds max(10·tol, 2·E_ref) by {acc_ratio:.2}");
        // reported global-error estimate vs the actual error
        let est = env["result"]["global_error"]["max_abs"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).fold(0.0, f64::max));
        if let Some(e) = est {
            if my_err > 1e-14 * scale {
                est_ratios.push(e / my_err);
            }
        }
        if case.get("expect_warning").is_some() {
            assert!(warned(&env, case["expect_warning"].as_str().unwrap()), "{fam}: expected {}", case["expect_warning"]);
        }
        assert_eq!(env["trust"]["checks"]["all_finite"], true, "{fam}");
        rows.push(json!({"family": fam, "vs_scipy_scaled": vs_scipy, "global_error": my_err, "scipy_global_error": sc_err,
                         "estimate_over_actual": est.map(|e| e / my_err.max(1e-300)), "tolerance_consistent": env["trust"]["checks"]["tolerance_consistent"],
                         "max_scaled_vs_fine": env["result"]["global_error"]["max_scaled"], "n_steps": env["result"]["stats"]["n_steps"]}));
        worst_vs_scipy = worst_vs_scipy.max(vs_scipy);
        worst_acc_ratio = worst_acc_ratio.max(acc_ratio);
    }
    assert!(n >= 30 && n_err >= 15, "{n} cases, {n_err} errors");
    est_ratios.sort_by(f64::total_cmp);
    let summary = json!({"cases": n, "error_cases": n_err, "worst_vs_scipy_scaled_recorded": worst_vs_scipy, "worst_error_over_T4_bound": worst_acc_ratio,
                         "estimate_over_actual": {"min": est_ratios.first(), "median": est_ratios.get(est_ratios.len() / 2), "max": est_ratios.last()}});
    println!("ode parity: {summary}");
    for r in &rows {
        println!("  {r}");
    }
    std::fs::write(spec_root().join("reports/parity-ode.json"), serde_json::to_string_pretty(&json!({
        "reference": "scipy.integrate.solve_ivp RK45 (same settings) and the truth (analytic or DOP853 rtol 1e-13); rhs by an independent Python parser of 3.15. Not MATLAB.",
        "summary": summary, "cases": rows,
        "tolerances": {"vs_truth": "|y − truth| ≤ max(10·(atol + rtol·|truth|), 2·E_ref + 1e-13·max|y|) per point (T4, v0.4.8)", "vs_scipy": "recorded only"}
    })).unwrap() + "\n").unwrap();
}
