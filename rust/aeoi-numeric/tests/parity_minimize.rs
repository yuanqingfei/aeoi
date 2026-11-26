//! optim.quadratic.quadprog and optim.scalar.fminbnd (spec v0.4.26, 10.45, 10.46) against the truth
//! (mpmath 50 digits: active-set enumeration for quadprog, every local minimiser for fminbnd).
//! Golden: golden/mpmath/<op>/0.4.0 (scripts/golden/gen_golden_minimize.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

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
fn rel_max(a: &[f64], b: &[f64]) -> f64 {
    let d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
    d / b.iter().fold(1.0f64, |m, v| m.max(v.abs()))
}

#[test]
fn parity_quadprog() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "optim.quadratic.quadprog").unwrap();
    let (mut ok, mut err, mut worst) = (0, 0, 0.0f64);
    let mut rows = Vec::new();
    for case in cases("optim.quadratic.quadprog") {
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let want: Vec<String> = case.get("expect_warning").and_then(|v| v.as_str()).map(|s| vec![s.to_string()]).unwrap_or_default();
        assert_eq!(codes(&env), want, "{fam}");
        assert_eq!(env["trust"]["checks"]["kkt"], true, "{fam}: {}", env["trust"]["residual"]);
        let (e, r) = (&case["expected"], &env["result"]);
        let k = e["kappa2_H"].as_f64().unwrap();
        let tol = 1e-12 * (k / 100.0).max(1.0);
        let dx = rel_max(&fvec(&r["x"]), &fvec(&e["x_true"]));
        let mut dl = 0.0f64;
        for g in ["ineqlin", "eqlin", "lower", "upper"] {
            dl = dl.max(rel_max(&fvec(&r["lambda"][g]), &fvec(&e["lambda_true"][g])));
        }
        let d = dx.max(dl);
        assert!(d <= tol, "{fam}: x/λ error {d:e} > {tol:e}");
        worst = worst.max(d / tol);
        rows.push(json!({"family": fam, "x_error": dx, "lambda_error": dl, "kappa2_H": k, "iterations": r["iterations"], "kkt": env["trust"]["residual"]}));
        ok += 1;
    }
    assert!(ok >= 6 && err >= 5, "{ok} ok, {err} errors");
    println!("quadprog parity: {ok} success + {err} error cases; worst error / tolerance {worst:.2e}");
    std::fs::write(spec_root().join("reports/parity-quadprog.json"), serde_json::to_string_pretty(&json!({
        "operator": "optim.quadratic.quadprog", "reference": "truth by active-set enumeration with 50-digit KKT solves (mpmath). Not MATLAB.",
        "cases": ok + err, "error_cases": err, "worst_error_over_tolerance": worst,
        "tolerance": "max|Δx|/max(1, max|x|) and the same for every multiplier group ≤ 1e-12·max(1, κ₂(H)/100)", "cases_detail": rows})).unwrap() + "\n").unwrap();
}

#[test]
fn parity_fminbnd() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "optim.scalar.fminbnd").unwrap();
    let (mut ok, mut err, mut worst, mut local) = (0, 0, 0.0f64, 0);
    let mut rows = Vec::new();
    for case in cases("optim.scalar.fminbnd") {
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let (e, r) = (&case["expected"], &env["result"]);
        let x = r["x"].as_f64().unwrap();
        let tol_x = e["tol_x"].as_f64().unwrap();
        let near = |m: f64| 3.0 * (f64::EPSILON.sqrt() * m.abs().max(1.0) + tol_x);
        // the returned point is one of the true local minimisers, to Brent's accuracy
        let (m, dist) = e["minima"].as_array().unwrap().iter().map(|p| p["x"].as_f64().unwrap()).map(|m| (m, (x - m).abs())).fold((f64::NAN, f64::INFINITY), |b, c| if c.1 < b.1 { c } else { b });
        assert!(dist <= near(m), "{fam}: x = {x} is {dist:e} from the nearest local minimiser {m}");
        worst = worst.max(dist / near(m));
        let g = e["global_x"].as_f64().unwrap();
        let is_global = (x - g).abs() <= near(g);
        let warned = codes(&env).iter().any(|c| c == "W.ACCURACY.BETTER_POINT_FOUND");
        assert_eq!(warned, !is_global, "{fam}: BETTER_POINT_FOUND = {warned}, returned the global minimiser = {is_global}");
        if !is_global {
            local += 1;
        }
        assert_eq!(env["trust"]["checks"]["probe_local_minimum"], true, "{fam}");
        let fv = r["fval"].as_f64().unwrap();
        let fm = e["minima"].as_array().unwrap().iter().find(|p| p["x"].as_f64().unwrap() == m).unwrap()["f"].as_f64().unwrap();
        assert!(fv >= fm - 1e-12 * fm.abs().max(1.0), "{fam}: fval {fv} below the true minimum {fm}");
        rows.push(json!({"family": fam, "x": x, "nearest_minimiser": m, "distance": dist, "global": is_global, "evaluations": r["evaluations"]}));
        ok += 1;
    }
    assert!(ok >= 8 && err >= 5 && local >= 1, "{ok} ok, {err} errors, {local} local");
    println!("fminbnd parity: {ok} success + {err} error cases ({local} ended in a non-global valley, flagged); worst distance / tolerance {worst:.2e}");
    std::fs::write(spec_root().join("reports/parity-fminbnd.json"), serde_json::to_string_pretty(&json!({
        "operator": "optim.scalar.fminbnd", "reference": "every local minimiser at 50 digits (mpmath). Not MATLAB.",
        "cases": ok + err, "error_cases": err, "worst_distance_over_tolerance": worst, "ended_in_local_valley": local,
        "tolerance": "|x − x*| ≤ 3·(√ε·max(1, |x*|) + tol_x) for the nearest true local minimiser x*; BETTER_POINT_FOUND iff x* is not the global minimiser",
        "cases_detail": rows})).unwrap() + "\n").unwrap();
}
