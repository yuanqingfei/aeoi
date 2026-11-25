//! linalg.system.solve (spec v0.4.10, 10.19) against numpy.linalg (LAPACK dgesv / dgelsd) and the exact
//! answer for the stored data (mpmath, 60 digits). Golden: golden/python-control/linalg.system.solve/0.4.0
//! (gen_golden_linsolve.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn rel_inf(a: &[f64], b: &[f64]) -> f64 {
    let d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
    d / b.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(f64::MIN_POSITIVE)
}

#[test]
fn parity_linsolve() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "linalg.system.solve").unwrap();
    let dir = spec_root().join("golden/python-control/linalg.system.solve/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err) = (0, 0);
    let (mut worst_ratio, mut ours_better, mut kappa_ratio_min) = (0.0f64, 0usize, f64::INFINITY);
    let mut rows = Vec::new();
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            assert!(!env["error"]["remediation"].as_array().map(|a| a.is_empty()).unwrap_or(true) || code == "E.VALIDATION.SCHEMA", "{fam}: remediation");
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let r = &env["result"];
        assert_eq!(r["x"]["shape"], x["shape"], "{fam}");
        assert_eq!(r["method"], x["method"], "{fam}");
        let got = fvec(&r["x"]["data"]);
        let truth = fvec(&x["x_true"]);
        let kappa = x["kappa"].as_f64().unwrap();
        // T1 with the condition-number scaling of V-6: 1e-12 · max(1, κ/100), against the exact answer
        let tol = 1e-12 * (kappa / 100.0).max(1.0);
        let fe = rel_inf(&got, &truth);
        let ratio = fe / tol;
        // a case whose post-condition is expected to fail (x listed as unreliable) is checked for the flag below,
        // not for accuracy: Wilkinson's matrix without refinement really is solved badly by partial pivoting
        let flagged = case.get("expect_postcondition_failed").is_some();
        if !flagged {
            assert!(ratio <= 1.0, "{fam}: forward error {fe:e} > {tol:e} (κ = {kappa:e})");
            worst_ratio = worst_ratio.max(ratio);
        }
        let np_fe = x["numpy_forward_error"].as_f64().unwrap();
        if fe < np_fe {
            ours_better += 1;
        }
        // post-conditions and warnings
        let failed: Vec<String> = case.get("expect_postcondition_failed").and_then(|v| v.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default();
        for c in ["backward_stable", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c] == true, !failed.iter().any(|f| f == c), "{fam}: {c} = {} ({})", env["trust"]["checks"][c], env["trust"]["residual"]);
        }
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        match case.get("expect_warning").and_then(|v| v.as_str()) {
            Some(w) => assert!(codes.contains(&w), "{fam}: expected {w}, got {codes:?}"),
            None => assert!(codes.iter().all(|c| *c == "W.EVIDENCE.POSTCONDITION_FAILED"), "{fam}: unexpected {codes:?}"),
        }
        // the 1-norm estimate is a lower bound, close to the truth while κ·u ≪ 1
        let est = env["trust"]["condition"]["kappa1_estimate"].as_f64().unwrap();
        if x["kappa_norm"] == "1" && kappa * f64::EPSILON < 1e-2 {
            let q = est / kappa;
            assert!(q <= 1.0 + 1e-8 && q >= 0.3, "{fam}: κ₁ estimate {est:e} vs {kappa:e}");
            kappa_ratio_min = kappa_ratio_min.min(q);
        }
        rows.push(json!({"family": fam, "forward_error": fe, "numpy_forward_error": np_fe, "kappa": kappa, "kappa1_estimate": est,
                         "backward_error": env["trust"]["residual"]["backward_error"], "refinement_steps": r["refinement_steps"]}));
    }
    assert!(n >= 25 && n_err >= 8, "{n} cases, {n_err} errors");
    println!("linsolve parity: {n} cases ({n_err} error cases); worst forward error / tolerance {worst_ratio:.2e}; more accurate than numpy in {ours_better} cases; min κ₁ estimate / κ₁ {kappa_ratio_min:.3}");
    std::fs::write(spec_root().join("reports/parity-linsolve.json"), serde_json::to_string_pretty(&json!({
        "reference": "numpy.linalg (LAPACK dgesv / dgelsd); exact answer for the stored data by mpmath at 60 digits. Not MATLAB.",
        "cases": n, "error_cases": n_err, "worst_forward_error_over_tolerance": worst_ratio,
        "more_accurate_than_numpy": ours_better, "min_kappa1_estimate_ratio": kappa_ratio_min,
        "tolerance": "‖x − x_true‖∞/‖x_true‖∞ ≤ 1e-12 · max(1, κ/100) (T1, V-6 κ scaling)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
