//! fit.poly.polyfit (spec v0.4.11, 10.21) against numpy.polyfit and the exact least-squares answer for the
//! stored data (mpmath, 60 digits). Golden: golden/python-control/fit.poly.polyfit/0.4.0 (gen_golden_polyfit.py).
//! Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn n2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[test]
fn parity_polyfit() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "fit.poly.polyfit").unwrap();
    let dir = spec_root().join("golden/python-control/fit.poly.polyfit/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err, mut worst, mut better) = (0, 0, 0.0f64, 0usize);
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
        let got = fvec(&r["p"]["data"]);
        let truth = fvec(&x["p_true"]);
        let cn = fvec(&x["col_norms"]);
        let kappa = x["kappa_scaled_2"].as_f64().unwrap();
        // scaled forward error ‖D(p − p*)‖/‖D p*‖ ≤ 1e-12 · max(1, κ/100) (T1, V-6 κ scaling)
        let d: Vec<f64> = got.iter().zip(&truth).zip(&cn).map(|((a, b), c)| (a - b) * c).collect();
        let dp: Vec<f64> = truth.iter().zip(&cn).map(|(a, c)| a * c).collect();
        let fe = n2(&d) / n2(&dp).max(f64::MIN_POSITIVE);
        let tol = 1e-12 * (kappa / 100.0).max(1.0);
        assert!(fe <= tol, "{fam}: scaled forward error {fe:e} > {tol:e} (κ = {kappa:e})");
        worst = worst.max(fe / tol);
        let np_fe = x["numpy_scaled_forward_error"].as_f64().unwrap();
        if fe < np_fe {
            better += 1;
        }
        assert_eq!(r["dof"], x["dof"], "{fam}");
        // mu
        if !x["mu_true"].is_null() {
            let mu = fvec(&r["mu"]);
            let mt = fvec(&x["mu_true"]);
            for k in 0..2 {
                assert!((mu[k] - mt[k]).abs() <= 4e-16 * mt[k].abs().max(mt[1]), "{fam}: mu {mu:?} vs {mt:?}");
            }
        } else {
            assert!(r["mu"].is_null(), "{fam}");
        }
        // residual norm and standard errors
        let y = fvec(&case["request"]["y"]["data"]);
        let rn = r["residual_norm"].as_f64().unwrap();
        let rt = x["residual_norm_true"].as_f64().unwrap();
        assert!((rn - rt).abs() <= 1e-12 * kappa.max(1.0) * n2(&y), "{fam}: residual {rn:e} vs {rt:e}");
        if x["std_errors_true"].is_null() {
            assert!(r["std_errors"].is_null(), "{fam}");
        } else if rt > 1e-8 * n2(&y) {
            let se = fvec(&r["std_errors"]);
            let st = fvec(&x["std_errors_true"]);
            for (a, b) in se.iter().zip(&st) {
                assert!((a - b).abs() <= 1e-8 * (kappa / 1e4).max(1.0) * b.abs(), "{fam}: std errors {se:?} vs {st:?}");
            }
        }
        assert_eq!(env["trust"]["checks"]["backward_stable"], true, "{fam}: {}", env["trust"]["residual"]);
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        match case.get("expect_warning").and_then(|v| v.as_str()) {
            Some(w) => assert!(codes.contains(&w), "{fam}: expected {w}, got {codes:?}"),
            None => assert!(codes.is_empty(), "{fam}: unexpected {codes:?}"),
        }
        rows.push(json!({"family": fam, "scaled_forward_error": fe, "numpy_scaled_forward_error": np_fe, "kappa_scaled_2": kappa,
                         "kappa1_estimate": env["trust"]["condition"]["kappa1_estimate"], "backward_error": env["trust"]["residual"]["backward_error"]}));
    }
    assert!(n >= 20 && n_err >= 8, "{n} cases, {n_err} errors");
    println!("polyfit parity: {n} cases ({n_err} error cases); worst scaled forward error / tolerance {worst:.2e}; more accurate than numpy in {better}");
    std::fs::write(spec_root().join("reports/parity-polyfit.json"), serde_json::to_string_pretty(&json!({
        "reference": "numpy.polyfit (dgelsd, column-scaled); exact answer for the stored data by mpmath at 60 digits. Not MATLAB.",
        "cases": n, "error_cases": n_err, "worst_scaled_forward_error_over_tolerance": worst, "more_accurate_than_numpy": better,
        "tolerance": "‖D(p − p*)‖₂/‖D p*‖₂ ≤ 1e-12 · max(1, κ(V·D⁻¹)/100), D = column 2-norms of V (T1, V-6 κ scaling)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
