//! linalg.matrix.inv and linalg.matrix.det (spec v0.4.22, 10.35, 10.36) against the truth for the stored data
//! (mpmath, 80 digits; exact rationals for n ≤ 8) with numpy.linalg (LAPACK) recorded for comparison.
//! Golden: golden/mpmath/linalg.matrix.{inv,det}/0.4.0 (gen_golden_matrix.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

const U: f64 = f64::EPSILON / 2.0;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn cases(op: &str) -> Vec<Value> {
    let dir = spec_root().join(format!("golden/mpmath/{op}/0.4.0"));
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files.iter().map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()).collect()
}
fn codes(env: &Value) -> Vec<String> {
    env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap().to_string()).filter(|c| c != "W.EVIDENCE.POSTCONDITION_FAILED").collect()
}
fn expected_warnings(case: &Value) -> Vec<String> {
    match case.get("expect_warning") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().map(|s| s.as_str().unwrap().to_string()).collect(),
        _ => vec![],
    }
}
fn failed_pc(case: &Value) -> Vec<String> {
    case.get("expect_postcondition_failed").and_then(|v| v.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default()
}
fn norm1_rel(got: &[f64], want: &[f64], n: usize) -> f64 {
    let col = |v: &[f64], j: usize| (0..n).map(|i| v[i * n + j].abs()).sum::<f64>();
    let d: Vec<f64> = got.iter().zip(want).map(|(a, b)| a - b).collect();
    (0..n).map(|j| col(&d, j)).fold(0.0, f64::max) / (0..n).map(|j| col(want, j)).fold(0.0, f64::max)
}

#[test]
fn parity_inv() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "linalg.matrix.inv").unwrap();
    let (mut n_ok, mut n_err, mut worst, mut better) = (0, 0, 0.0f64, 0);
    let mut rows = Vec::new();
    for case in cases("linalg.matrix.inv") {
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            assert!(!env["error"]["remediation"].as_array().map(|a| a.is_empty()).unwrap_or(true) || code == "E.VALIDATION.SCHEMA", "{fam}: remediation");
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let e = &case["expected"];
        let n = e["n"].as_u64().unwrap() as usize;
        assert_eq!(env["result"]["X"]["shape"], json!([n, n]), "{fam}");
        let got = fvec(&env["result"]["X"]["data"]);
        let fe = norm1_rel(&got, &fvec(&e["X_true"]), n);
        let kappa = e["kappa1_true"].as_f64().unwrap();
        let tol = 1e-12 * (kappa / 100.0).max(1.0);
        let pcs = failed_pc(&case);
        for c in ["backward_stable", "forward_error_below_one", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c] == true, !pcs.iter().any(|f| f == c), "{fam}: {c} = {}", env["trust"]["checks"][c]);
        }
        if pcs.is_empty() {
            assert!(fe <= tol, "{fam}: ‖ΔX‖₁/‖X‖₁ = {fe:e} > {tol:e} (κ₁ = {kappa:e})");
            worst = worst.max(fe / tol);
        }
        assert_eq!(codes(&env), expected_warnings(&case), "{fam}");
        let np_fe = e["numpy_forward_error"].as_f64().unwrap();
        if fe < np_fe {
            better += 1;
        }
        n_ok += 1;
        rows.push(json!({"family": fam, "n": n, "forward_error": fe, "numpy_forward_error": np_fe, "kappa1_true": kappa, "kappa1_reported": env["result"]["kappa1"], "backward_error": env["trust"]["residual"]["backward_error"], "refinement_steps": env["result"]["refinement_steps"]}));
    }
    assert!(n_ok >= 14 && n_err >= 8, "{n_ok} ok, {n_err} errors");
    println!("inv parity: {n_ok} success + {n_err} error cases; worst forward error / tolerance {worst:.2e}; more accurate than numpy in {better}");
    std::fs::write(spec_root().join("reports/parity-inv.json"), serde_json::to_string_pretty(&json!({
        "reference": "truth for the stored data: mpmath 80 digits (exact rationals for n ≤ 8); numpy.linalg.inv (LAPACK) recorded. Not MATLAB.",
        "cases": n_ok + n_err, "error_cases": n_err, "worst_forward_error_over_tolerance": worst, "more_accurate_than_numpy": better,
        "tolerance": "‖X − X_true‖₁/‖X_true‖₁ ≤ 1e-12 · max(1, κ₁/100) (T1, V-6 κ scaling); flagged cases: the failed post-condition is asserted, accuracy is not",
        "cases_detail": rows})).unwrap() + "\n").unwrap();
}

#[test]
fn parity_det() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "linalg.matrix.det").unwrap();
    let (mut n_ok, mut n_err, mut worst, mut covered, mut sig) = (0, 0, 0.0f64, 0, 0);
    let mut rows = Vec::new();
    for case in cases("linalg.matrix.det") {
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let (e, r) = (&case["expected"], &env["result"]);
        let n = e["n"].as_u64().unwrap() as f64;
        assert_eq!(codes(&env), expected_warnings(&case), "{fam}");
        let pcs = failed_pc(&case);
        assert_eq!(env["trust"]["checks"]["significant"] == true, pcs.is_empty(), "{fam}");
        n_ok += 1;
        if e["sign_true"] == 0 {
            // singular data: claiming a significant det would be false
            assert_eq!(r["zero_within_error"], true, "{fam}: {r}");
            rows.push(json!({"family": fam, "det": r["det"], "relative_error_estimate": r["relative_error_estimate"], "det_true": 0.0}));
            continue;
        }
        sig += 1;
        assert_eq!(r["zero_within_error"], false, "{fam}");
        assert_eq!(r["sign"], e["sign_true"], "{fam}");
        let kappa = e["kappa1_true"].as_f64().unwrap();
        let tol = 16.0 * n * U * kappa.max(1.0);
        let lt = e["log_abs_det_true"].as_f64().unwrap();
        let dl = (r["log_abs_det"].as_f64().unwrap() - lt).abs();
        assert!(dl <= tol, "{fam}: |Δ log|det|| = {dl:e} > {tol:e}");
        worst = worst.max(dl / tol);
        if let (Some(dt), Some(dg)) = (e["det_true"].as_f64(), r["det"].as_f64()) {
            if dt.abs() >= f64::MIN_POSITIVE {
                let rel = (dg - dt).abs() / dt.abs();
                assert!(rel <= 1.01 * tol, "{fam}: |Δdet|/|det| = {rel:e} > {tol:e}");
            }
        }
        // the heuristic estimate is recorded against the actual error (≈ |Δ log|det||), not asserted
        let est = r["relative_error_estimate"].as_f64().unwrap();
        if est >= dl {
            covered += 1;
        }
        rows.push(json!({"family": fam, "n": n, "abs_error_log_abs_det": dl, "relative_error_estimate": est, "kappa1_true": kappa, "numpy_log_abs_det": e["numpy_log_abs_det"], "log_abs_det_true": lt}));
    }
    assert!(n_ok >= 15 && n_err >= 5, "{n_ok} ok, {n_err} errors");
    println!("det parity: {n_ok} success + {n_err} error cases; worst |Δ log|det|| / tolerance {worst:.2e}; estimate ≥ actual error in {covered}/{sig}");
    std::fs::write(spec_root().join("reports/parity-det.json"), serde_json::to_string_pretty(&json!({
        "reference": "truth for the stored data: mpmath 80 digits; exact rationals for n ≤ 8 and for every singular case. numpy.linalg.det/slogdet recorded. Not MATLAB.",
        "cases": n_ok + n_err, "error_cases": n_err, "worst_error_over_tolerance": worst, "estimate_covers_actual": format!("{covered}/{sig}"),
        "tolerance": "|log|det| − log|det_true|| ≤ 16·n·u·max(1, κ₁) and the same for |Δdet|/|det| when det is in range; sign exact; singular data ⇒ zero_within_error = true",
        "cases_detail": rows})).unwrap() + "\n").unwrap();
}
