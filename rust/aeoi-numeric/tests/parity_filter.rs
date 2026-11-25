//! signal.filter.filter (spec v0.4.12, 10.25) against the 50-digit difference equation on the stored data
//! (mpmath) and scipy lfilter / sosfilt. Golden: golden/python-control/signal.filter.filter/0.4.0
//! (gen_golden_filter.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fl(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

#[test]
fn parity_filter() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "signal.filter.filter").unwrap();
    let dir = spec_root().join("golden/python-control/signal.filter.filter/0.4.0");
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
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let r = &env["result"];
        assert_eq!(r["y"]["shape"], x["y_shape"], "{fam}");
        let (y, yt) = (fl(&r["y"]["data"]), fl(&x["y_true"]));
        let (zf, zt) = (fl(&r["zf"]["data"]), fl(&x["zf_true"]));
        let rad = x["max_pole_radius"].as_f64().unwrap();
        let amp = if rad < 1.0 { 1.0 / (1.0 - rad) } else { yt.len() as f64 };
        let scale = yt.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(f64::MIN_POSITIVE);
        let e = y.iter().zip(&yt).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / scale;
        let ez = zf.iter().zip(&zt).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / scale.max(zt.iter().fold(0.0f64, |m, v| m.max(v.abs())));
        let tol = 1e-13 * amp.max(1.0);
        assert!(e <= tol && ez <= tol, "{fam}: y error {e:e}, zf error {ez:e} > {tol:e} (r = {rad})");
        worst = worst.max(e.max(ez) / tol);
        let se = x["scipy_error"].as_f64().unwrap();
        if e <= se {
            better += 1;
        }
        for c in ["difference_equation", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} {}", env["trust"]["residual"]);
        }
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        match case.get("expect_warning").and_then(|v| v.as_str()) {
            Some(w) => assert!(codes.contains(&w), "{fam}: expected {w}, got {codes:?}"),
            None => assert!(codes.is_empty(), "{fam}: unexpected {codes:?}"),
        }
        rows.push(json!({"family": fam, "y_error": e, "scipy_error": se, "max_pole_radius": rad, "difference_equation_residual": env["trust"]["residual"]["difference_equation"]}));
    }
    assert!(n >= 20 && n_err >= 8);
    println!("filter parity: {n} cases ({n_err} errors); worst error / tolerance {worst:.2e}; at least as accurate as scipy in {better}");
    std::fs::write(spec_root().join("reports/parity-filter.json"), serde_json::to_string_pretty(&json!({
        "reference": "50-digit difference equation on the stored data (mpmath); scipy lfilter / sosfilt recorded. Not MATLAB.",
        "cases": n, "error_cases": n_err, "worst_error_over_tolerance": worst, "at_least_as_accurate_as_scipy": better,
        "tolerance": "max|y − y*| and max|zf − zf*| ≤ 1e-13 · max(1, 1/(1 − r)) · max|y*| (r = largest pole radius; n for unstable filters)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
