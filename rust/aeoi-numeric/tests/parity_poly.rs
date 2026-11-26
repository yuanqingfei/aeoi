//! fit.poly.polyval and fit.poly.roots (spec v0.4.20, 10.33, 10.34) against numpy and independent truth:
//! polyval against the exact rational value at the stored points; roots against mpmath.polyroots at 60
//! digits. Golden: golden/python-control/fit.poly.{polyval,roots}/0.4.0 (scripts/golden/gen_golden_poly.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

const U: f64 = f64::EPSILON / 2.0;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::INFINITY)).collect()
}
fn cvec(v: &Value) -> Vec<(f64, f64)> {
    v.as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
}
fn gam(k: usize) -> f64 {
    let ku = k as f64 * U;
    ku / (1.0 - ku)
}
fn cases(op: &str) -> Vec<(String, Value)> {
    let dir = spec_root().join(format!("golden/python-control/{op}/0.4.0"));
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files.iter().map(|p| {
        let c: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        (c["case_family"].as_str().unwrap().to_string(), c)
    }).collect()
}
fn warnings(env: &Value) -> Vec<String> {
    env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap().to_string()).collect()
}
fn check_error(fam: &str, case: &Value, env: &Value) -> bool {
    if let Some(code) = case.get("expected_error") {
        assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
        return true;
    }
    assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
    let w = warnings(env);
    match case.get("expect_warning").and_then(|v| v.as_str()) {
        Some(x) => assert!(w.iter().any(|c| c == x), "{fam}: expected {x}, got {w:?}"),
        None => assert!(w.is_empty(), "{fam}: unexpected {w:?}"),
    }
    false
}

#[test]
fn parity_polyval() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "fit.poly.polyval").unwrap();
    let (mut n, mut n_err, mut worst, mut pts, mut better, mut worse) = (0, 0, 0.0f64, 0usize, 0usize, 0usize);
    let mut rows = Vec::new();
    for (fam, case) in cases("fit.poly.polyval") {
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if check_error(&fam, &case, &env) {
            n_err += 1;
            continue;
        }
        let x = &case["expected"];
        let deg = case["request"]["p"]["data"].as_array().unwrap().len() - 1;
        let (y, b) = (fvec(&env["result"]["y"]["data"]), fvec(&env["result"]["error_bound"]["data"]));
        let (yt, pt, ynp) = (fvec(&x["y_true"]), fvec(&x["ptilde"]), fvec(&x["numpy_abs_error"]));
        let mut fam_worst = 0.0f64;
        for k in 0..y.len() {
            let err = (y[k] - yt[k]).abs();
            // the reported bound must hold (y_true is the exact value rounded once: allow its half ulp)
            assert!(err <= b[k] + U * yt[k].abs(), "{fam}[{k}]: |y − p| = {err:e} > bound {:e}", b[k]);
            // suite tolerance, independent of the reported bound: compensated-Horner accuracy
            let tol = 2.0 * U * yt[k].abs() + 2.0 * gam(2 * deg).powi(2) * pt[k] + f64::MIN_POSITIVE;
            assert!(err <= tol, "{fam}[{k}]: |y − p| = {err:e} > {tol:e}");
            fam_worst = fam_worst.max(err / tol);
            pts += 1;
            if err < ynp[k] {
                better += 1;
            } else if err > ynp[k] {
                worse += 1;
            }
        }
        assert_eq!(env["trust"]["checks"]["horner_consistent"], true, "{fam}");
        worst = worst.max(fam_worst);
        rows.push(json!({"family": fam, "points": y.len(), "worst_error_over_tolerance": fam_worst}));
    }
    assert!(n >= 18 && n_err >= 8, "{n} cases, {n_err} errors");
    assert_eq!(worse, 0, "compensated Horner was less accurate than numpy at {worse} points");
    println!("polyval parity: {n} cases ({n_err} error cases), {pts} points; worst error/tolerance {worst:.2e}; more accurate than numpy at {better}, never less");
    std::fs::write(spec_root().join("reports/parity-polyval.json"), serde_json::to_string_pretty(&json!({
        "reference": "exact rational value at the stored f64 points (Python fractions); numpy.polyval for comparison. Not MATLAB.",
        "cases": n, "error_cases": n_err, "points": pts, "worst_error_over_tolerance": worst, "more_accurate_than_numpy": better, "less_accurate_than_numpy": worse,
        "tolerance": "|y − p(t)| ≤ 2u|p(t)| + 2γ_{2n}²·p̃(|t|) (suite), and |y − p(t)| ≤ reported error_bound (claim)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}

#[test]
fn parity_roots() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "fit.poly.roots").unwrap();
    let (mut n, mut n_err, mut worst, mut simple, mut clusters) = (0, 0, 0.0f64, 0usize, 0usize);
    let mut rows = Vec::new();
    for (fam, case) in cases("fit.poly.roots") {
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if check_error(&fam, &case, &env) {
            n_err += 1;
            continue;
        }
        let x = &case["expected"];
        let r = &env["result"];
        let got = cvec(&r["roots"]["data"]);
        let rad = fvec(&r["error_radius"]["data"]);
        let mult: Vec<u64> = r["multiplicity"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        let truth = cvec(&x["roots_true"]);
        let tmult: Vec<u64> = x["multiplicity_true"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(got.len(), truth.len(), "{fam}: root count");
        assert_eq!(r["degree"], x["degree"], "{fam}");
        assert_eq!(r["zero_roots"], x["zero_roots"], "{fam}");
        // order: real part ascending, ties by imaginary part descending
        for w in got.windows(2) {
            assert!(w[0].0 < w[1].0 || (w[0].0 == w[1].0 && w[0].1 >= w[1].1), "{fam}: order {got:?}");
        }
        // claim: every true root lies within error_radius of some reported root
        for t in &truth {
            assert!(got.iter().zip(&rad).any(|(z, r)| (z.0 - t.0).hypot(z.1 - t.1) <= *r), "{fam}: true root {t:?} outside every disc");
        }
        // truly multiple roots must be reported in a cluster of at least that size
        for (k, t) in truth.iter().enumerate() {
            if tmult[k] > 1 && !(t.0 == 0.0 && t.1 == 0.0 && x["zero_roots"].as_u64().unwrap() >= tmult[k]) {
                let j = (0..got.len()).min_by(|&a, &b| (got[a].0 - t.0).hypot(got[a].1 - t.1).partial_cmp(&(got[b].0 - t.0).hypot(got[b].1 - t.1)).unwrap()).unwrap();
                assert!(mult[j] >= tmult[k], "{fam}: root {t:?} has multiplicity {} but reported {}", tmult[k], mult[j]);
                clusters += 1;
            }
        }
        // accuracy of simple roots: |z − r| ≤ 32u·cond_abs + 2u|r| (suite), with cond_abs = q̃(|r|)/|q'(r)|
        let cond = x["cond_abs"].as_array().unwrap();
        let mut fam_worst = 0.0f64;
        for (k, t) in truth.iter().enumerate() {
            let Some(c) = cond[k].as_f64() else { continue };
            let j = (0..got.len()).min_by(|&a, &b| (got[a].0 - t.0).hypot(got[a].1 - t.1).partial_cmp(&(got[b].0 - t.0).hypot(got[b].1 - t.1)).unwrap()).unwrap();
            let err = (got[j].0 - t.0).hypot(got[j].1 - t.1);
            let tol = 32.0 * U * c + 2.0 * U * t.0.hypot(t.1) + f64::MIN_POSITIVE;
            assert!(err <= tol, "{fam}: root {t:?} error {err:e} > {tol:e} (cond {c:e})");
            fam_worst = fam_worst.max(err / tol);
            simple += 1;
        }
        worst = worst.max(fam_worst);
        assert_eq!(env["trust"]["checks"]["backward_stable"], true, "{fam}: {}", env["trust"]["residual"]);
        assert_eq!(env["trust"]["checks"]["radii_finite"], true, "{fam}");
        rows.push(json!({"family": fam, "degree": got.len(), "worst_simple_root_error_over_tolerance": fam_worst, "max_error_radius": rad.iter().cloned().fold(0.0, f64::max), "backward_error": env["trust"]["residual"]["backward_error"]}));
    }
    assert!(n >= 18 && n_err >= 5, "{n} cases, {n_err} errors");
    println!("roots parity: {n} cases ({n_err} error cases); {simple} simple roots, worst error/tolerance {worst:.2e}; {clusters} multiple roots inside reported clusters");
    std::fs::write(spec_root().join("reports/parity-roots.json"), serde_json::to_string_pretty(&json!({
        "reference": "mpmath.polyroots at 60 digits on the stored f64 coefficients; numpy.roots for comparison. Not MATLAB.",
        "cases": n, "error_cases": n_err, "simple_roots": simple, "worst_simple_root_error_over_tolerance": worst, "multiple_roots_in_clusters": clusters,
        "tolerance": "simple roots: |z − r| ≤ 32u·q̃(|r|)/|q'(r)| + 2u|r| (suite); every true root within the reported error_radius of a reported root (claim)", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
