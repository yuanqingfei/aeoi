//! linalg.decomp.lu / qr / chol (spec v0.4.24, 10.42 – 10.44) against the truth for the stored data (mpmath,
//! 50 digits; the LU truth follows the same first-maximal-pivot rule). Golden: golden/mpmath/<op>/0.4.0
//! (scripts/golden/gen_golden_decomp.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn rel(a: &[f64], b: &[f64]) -> f64 {
    let d = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt();
    let nb = b.iter().map(|y| y * y).sum::<f64>().sqrt();
    if nb == 0.0 { d } else { d / nb }
}
fn cases(op: &str) -> Vec<Value> {
    let dir = spec_root().join(format!("golden/mpmath/{op}/0.4.0"));
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files.iter().map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()).collect()
}

fn run(op: &str, report: &str, tolerance: &str, check: &dyn Fn(&Value, &Value) -> (f64, Value)) {
    let reg = aeoi_numeric::registry();
    let o = aeoi_numeric::find(&reg, op).unwrap();
    let (mut ok, mut err, mut worst) = (0, 0, 0.0f64);
    let mut rows = Vec::new();
    for case in cases(op) {
        let fam = case["case_family"].as_str().unwrap().to_string();
        let env = o.call(&case["request"], &HostCtx::default());
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{op} {fam}: {}", env["error"]);
            assert!(!env["error"]["remediation"].as_array().map(|a| a.is_empty()).unwrap_or(true) || code == "E.VALIDATION.SCHEMA" || code == "E.VALIDATION.NON_FINITE", "{op} {fam}: remediation");
            err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{op} {fam}: {}", env["error"]);
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        let want: Vec<&str> = case.get("expect_warning").and_then(|v| v.as_str()).into_iter().collect();
        assert_eq!(codes, want, "{op} {fam}");
        for (k, v) in env["trust"]["checks"].as_object().unwrap() {
            assert_eq!(v, &json!(true), "{op} {fam}: {k} ({})", env["trust"]["residual"]);
        }
        let (ratio, mut row) = check(&case, &env);
        assert!(ratio <= 1.0, "{op} {fam}: error / tolerance = {ratio:e} ({row})");
        worst = worst.max(ratio);
        row["family"] = json!(fam);
        row["reconstruction"] = env["trust"]["residual"]["reconstruction"].clone();
        rows.push(row);
        ok += 1;
    }
    assert!(ok >= 6 && err >= 4, "{op}: {ok} ok, {err} errors");
    println!("{op}: {ok} success + {err} error cases; worst error / tolerance {worst:.2e}");
    std::fs::write(spec_root().join(format!("reports/{report}")), serde_json::to_string_pretty(&json!({
        "operator": op, "reference": "truth for the stored data: mpmath 50 digits. Not MATLAB.", "cases": ok + err, "error_cases": err,
        "worst_error_over_tolerance": worst, "tolerance": tolerance, "cases_detail": rows})).unwrap() + "\n").unwrap();
}

fn tol(k: &Value) -> f64 {
    1e-12 * (k.as_f64().unwrap_or(1.0) / 100.0).max(1.0)
}

#[test]
fn parity_lu() {
    run("linalg.decomp.lu", "parity-lu.json", "perm and zero_pivot exact; ‖ΔL‖_F/‖L‖_F and ‖ΔU‖_F/‖U‖_F ≤ 1e-12·max(1, κ₁/100)", &|case, env| {
        let (e, r) = (&case["expected"], &env["result"]);
        assert_eq!(r["perm"], e["perm_true"]);
        assert_eq!(r["zero_pivot"], e["zero_pivot_true"]);
        let d = rel(&fvec(&r["L"]["data"]), &fvec(&e["L_true"])).max(rel(&fvec(&r["U"]["data"]), &fvec(&e["U_true"])));
        let t = tol(&e["kappa1_true"]);
        (d / t, json!({"factor_rel_error": d, "kappa1_true": e["kappa1_true"], "growth_factor": r["growth_factor"]}))
    });
}

#[test]
fn parity_qr() {
    run("linalg.decomp.qr", "parity-qr.json", "R_ii ≥ 0; unique cases (full column rank of the leading k columns): ‖ΔQ‖_F/‖Q‖_F, ‖ΔR‖_F/‖R‖_F ≤ 1e-12·max(1, κ₂/100); rank-deficient: post-conditions only", &|case, env| {
        let (e, r) = (&case["expected"], &env["result"]);
        let rv = fvec(&r["R"]["data"]);
        let (rows, n) = (r["R"]["shape"][0].as_u64().unwrap() as usize, e["n"].as_u64().unwrap() as usize);
        for ii in 0..rows.min(n) {
            assert!(rv[ii * n + ii] >= 0.0, "R_ii < 0");
        }
        assert_eq!(r["mode"], e["mode"]);
        if e["unique"] != true {
            return (0.0, json!({"unique": false}));
        }
        let d = rel(&fvec(&r["Q"]["data"]), &fvec(&e["Q_true"])).max(rel(&rv, &fvec(&e["R_true"])));
        let t = tol(&e["kappa2_true"]);
        (d / t, json!({"factor_rel_error": d, "kappa2_true": e["kappa2_true"]}))
    });
}

#[test]
fn parity_chol() {
    run("linalg.decomp.chol", "parity-chol.json", "‖ΔR‖_F/‖R‖_F ≤ 1e-12·max(1, κ₁/100)", &|case, env| {
        let (e, r) = (&case["expected"], &env["result"]);
        assert_eq!(r["triangle"], e["triangle"]);
        let d = rel(&fvec(&r["R"]["data"]), &fvec(&e["R_true"]));
        let t = tol(&e["kappa1_true"]);
        (d / t, json!({"factor_rel_error": d, "kappa1_true": e["kappa1_true"]}))
    });
}
