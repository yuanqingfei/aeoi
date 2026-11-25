//! nonlinear.root.fzero / nonlinear.system.fsolve (spec v0.4.13, 10.26–10.27) against mpmath.findroot at 50
//! digits; scipy brentq / fsolve recorded. Golden: golden/python-control/nonlinear.*/0.4.0
//! (gen_golden_roots.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fl(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn files(op: &str) -> Vec<PathBuf> {
    let mut f: Vec<_> = std::fs::read_dir(spec_root().join(format!("golden/python-control/{op}/0.4.0"))).unwrap().map(|e| e.unwrap().path()).collect();
    f.sort();
    f
}
fn warnings(env: &Value) -> Vec<String> {
    env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap().to_string()).collect()
}

#[test]
fn parity_fzero() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "nonlinear.root.fzero").unwrap();
    let (mut n, mut n_err, mut worst) = (0, 0, 0.0f64);
    let mut rows = Vec::new();
    for p in files("nonlinear.root.fzero") {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = env["result"]["x"].as_f64().unwrap();
        let roots = fl(&case["expected"]["roots_true"]);
        let (best, err) = roots.iter().map(|r| (*r, (x - r).abs())).fold((f64::NAN, f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
        let tol = 4.0 * f64::EPSILON * best.abs() + 1e-300;
        let failed: Vec<String> = case.get("expect_postcondition_failed").and_then(|v| v.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default();
        if failed.is_empty() {
            assert!(err <= tol, "{fam}: x = {x:e}, root {best:e}, |Δ| = {err:e} > {tol:e}");
            worst = worst.max(err / tol);
        }
        // the final bracket encloses the returned x and is tight
        let fb = fl(&env["result"]["bracket"]["final"]);
        assert!(fb[0] <= x && x <= fb[1], "{fam}: bracket {fb:?}");
        assert_eq!(fl(&env["result"]["bracket"]["initial"]), fl(&case["expected"]["bracket_used"]), "{fam}: initial bracket");
        for c in ["sign_change", "not_a_pole", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c] == true, !failed.iter().any(|f| f == c), "{fam}: {c}");
        }
        let w = warnings(&env);
        let expect: Vec<String> = case.get("expect_warning").map(|v| vec![v.as_str().unwrap().to_string()]).unwrap_or_default();
        let w: Vec<String> = w.into_iter().filter(|c| c != "W.EVIDENCE.POSTCONDITION_FAILED").collect();
        assert_eq!(w, expect, "{fam}");
        rows.push(json!({"family": fam, "x": x, "error": err, "evaluations": env["result"]["evaluations"]}));
    }
    assert!(n >= 20 && n_err >= 8);
    println!("fzero parity: {n} cases ({n_err} errors); worst error / (4ε|x*|) {worst:.2}");
    std::fs::write(spec_root().join("reports/parity-fzero.json"), serde_json::to_string_pretty(&json!({
        "reference": "mpmath.findroot 50 digits started from scipy brentq; not MATLAB", "cases": n, "error_cases": n_err,
        "worst_error_over_tolerance": worst, "tolerance": "|x − x*| ≤ 4ε|x*| (nearest exact root in the bracket)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}

#[test]
fn parity_fsolve() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "nonlinear.system.fsolve").unwrap();
    let (mut n, mut n_err, mut worst) = (0, 0, 0.0f64);
    let mut rows = Vec::new();
    for p in files("nonlinear.system.fsolve") {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            assert!(code == "E.VALIDATION.SCHEMA" || !env["error"]["remediation"].as_array().map(|a| a.is_empty()).unwrap_or(true) || code.as_str().unwrap().starts_with("E.VALIDATION"), "{fam}: remediation");
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = fl(&env["result"]["x"]);
        let alts: Vec<Vec<f64>> = match case["expected"].get("x_true_alternatives") {
            Some(a) => a.as_array().unwrap().iter().map(fl).collect(),
            None => vec![fl(&case["expected"]["x_true"])],
        };
        let warned = warnings(&env).iter().any(|c| c == "W.NUMERICAL.ILL_CONDITIONED");
        assert_eq!(warned, case.get("expect_warning").is_some(), "{fam}: {:?}", warnings(&env));
        let err = alts.iter().map(|t| x.iter().zip(t).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / t.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300)).fold(f64::INFINITY, f64::min);
        let tol = if warned { 1e-7 } else { 1e-14 };
        assert!(err <= tol, "{fam}: x = {x:?}, relative error {err:e} > {tol:e}");
        if !warned {
            worst = worst.max(err / tol);
        }
        assert_eq!(env["trust"]["checks"]["newton_step_small"], true, "{fam}: {}", env["trust"]);
        rows.push(json!({"family": fam, "relative_error": err, "iterations": env["result"]["iterations"], "scipy_residual_inf": case["expected"]["scipy_residual_inf"]}));
    }
    assert!(n >= 15 && n_err >= 7);
    println!("fsolve parity: {n} cases ({n_err} errors); worst error / tolerance {worst:.2}");
    std::fs::write(spec_root().join("reports/parity-fsolve.json"), serde_json::to_string_pretty(&json!({
        "reference": "mpmath.findroot 50 digits started from scipy fsolve (or a known root); not MATLAB", "cases": n, "error_cases": n_err,
        "worst_error_over_tolerance": worst, "tolerance": "‖x − x*‖∞/‖x*‖∞ ≤ 1e-14 (1e-7 for flagged multiple roots)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
