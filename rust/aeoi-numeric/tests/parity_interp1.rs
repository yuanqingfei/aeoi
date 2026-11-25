//! interp.grid.interp1 (spec v0.4.10, 10.20) against numpy.interp / scipy.interpolate (interp1d nearest-up,
//! previous, next; PchipInterpolator; CubicSpline not-a-knot), plus exact answers where they exist (spline on
//! cubic data, every method on a straight line). Golden: golden/python-control/interp.grid.interp1/0.4.0. Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()
}

#[test]
fn parity_interp1() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "interp.grid.interp1").unwrap();
    let dir = spec_root().join("golden/python-control/interp.grid.interp1/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err, mut n_bitwise) = (0, 0, 0);
    let (mut worst_ref, mut worst_exact) = (0.0f64, 0.0f64);
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
        assert_eq!(r["yq"]["shape"], x["shape"], "{fam}");
        let got = fvec(&r["yq"]["data"]);
        let want = fvec(&x["yq"]);
        let scale = x["y_scale"].as_f64().unwrap().max(1.0);
        let tol = 1e-12 * scale; // T1
        let mut bit = true;
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g.is_nan(), w.is_nan(), "{fam}[{k}]: NaN mismatch");
            if !w.is_nan() {
                let d = (g - w).abs();
                assert!(d <= tol, "{fam}[{k}]: {g} vs {w}");
                worst_ref = worst_ref.max(d / scale);
                bit &= g.to_bits() == w.to_bits();
            }
        }
        if bit {
            n_bitwise += 1;
        }
        if let Some(ex) = x.get("yq_exact") {
            for (g, w) in got.iter().zip(fvec(ex)) {
                let d = (g - w).abs();
                assert!(d <= tol, "{fam}: exact {g} vs {w}");
                worst_exact = worst_exact.max(d / scale);
            }
        }
        for c in ["all_finite", "pieces_interpolate", "spline_c2", "local_bounds"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} {}", env["trust"]["residual"]);
        }
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        match case.get("expect_warning").and_then(|v| v.as_str()) {
            Some(w) => assert!(codes.contains(&w), "{fam}: expected {w}, got {codes:?}"),
            None => assert!(codes.is_empty(), "{fam}: unexpected {codes:?}"),
        }
    }
    assert!(n >= 50 && n_err >= 10, "{n} cases, {n_err} errors");
    println!("interp1 parity: {n} cases ({n_err} error cases); bitwise equal to the reference in {n_bitwise}; worst |Δ|/max(1,max|y|): reference {worst_ref:.2e}, exact {worst_exact:.2e}");
    std::fs::write(spec_root().join("reports/parity-interp1.json"), serde_json::to_string_pretty(&json!({
        "reference": "numpy.interp / scipy.interpolate (interp1d nearest-up, previous, next; PchipInterpolator; CubicSpline not-a-knot); exact values for spline on cubic data and every method on a line. Not MATLAB.",
        "cases": n, "error_cases": n_err, "bitwise_equal_cases": n_bitwise,
        "worst_abs_over_scale": {"reference": worst_ref, "exact": worst_exact},
        "tolerance": "|yq − ref| ≤ 1e-12 · max(1, max|y|) per value (T1); NaN at the same places"
    })).unwrap() + "\n").unwrap();
}
