//! linalg.subspace.rank / basis, linalg.matrix.pinv / norm / cond (spec v0.4.23, 10.37 – 10.41) against the truth
//! for the stored data (mpmath 50 digits; exact rationals for the 1-, inf- and Frobenius norms).
//! Golden: golden/mpmath/<op>/0.4.0 (scripts/golden/gen_golden_subspace.py). Not MATLAB.
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
fn want_codes(case: &Value) -> Vec<String> {
    case.get("expect_warning").and_then(|v| v.as_str()).map(|s| vec![s.to_string()]).unwrap_or_default()
}
fn failed_pc(case: &Value) -> Vec<String> {
    case.get("expect_postcondition_failed").and_then(|v| v.as_array()).map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

/// Runs every golden case of `op`; `check` gets (case, envelope) for success cases and returns (error/tolerance, row).
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
            err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{op} {fam}: {}", env["error"]);
        assert_eq!(codes(&env), want_codes(&case), "{op} {fam}");
        let pcs = failed_pc(&case);
        for (k, v) in env["trust"]["checks"].as_object().unwrap() {
            assert_eq!(v == true, !pcs.iter().any(|f| f == k), "{op} {fam}: check {k} = {v} ({})", env["trust"]["residual"]);
        }
        let (ratio, mut row) = check(&case, &env);
        assert!(ratio <= 1.0, "{op} {fam}: error / tolerance = {ratio:e} ({row})");
        worst = worst.max(ratio);
        row["family"] = json!(fam);
        rows.push(row);
        ok += 1;
    }
    assert!(ok >= 10 && err >= 3, "{op}: {ok} ok, {err} errors");
    println!("{op}: {ok} success + {err} error cases; worst error / tolerance {worst:.2e}");
    std::fs::write(spec_root().join(format!("reports/{report}")), serde_json::to_string_pretty(&json!({
        "operator": op, "reference": "truth for the stored data: mpmath 50 digits (exact rationals for the 1-, inf-, Frobenius norms). Not MATLAB.",
        "cases": ok + err, "error_cases": err, "worst_error_over_tolerance": worst, "tolerance": tolerance, "cases_detail": rows})).unwrap() + "\n").unwrap();
}

fn rank_common(case: &Value, env: &Value) -> (f64, Value) {
    let e = &case["expected"];
    let r = &env["result"];
    assert_eq!(r["rank"], e["rank_true"]);
    if let Some(ts) = e.get("tol_source") { assert_eq!(&r["tol_source"], ts); }
    let (m, n) = (e["m"].as_f64().unwrap(), e["n"].as_f64().unwrap());
    let st = fvec(&e["s_true"]);
    let sg = fvec(&r["s"]["data"]);
    let tol = 32.0 * m.max(n) * U * st[0].max(f64::MIN_POSITIVE);
    let d = sg.iter().zip(&st).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    (d / tol, json!({"max_abs_sigma_error": d, "sigma_max": st[0], "rank": r["rank"]}))
}

#[test]
fn parity_rank() {
    run("linalg.subspace.rank", "parity-rank.json", "rank exact; |Δσ_k| ≤ 32·max(m, n)·u·σ_max (absolute, Weyl)", &|case, env| {
        let e = &case["expected"];
        let tg = env["result"]["tol"].as_f64().unwrap();
        let tt = e["tol"].as_f64().unwrap();
        // the default tolerance is computed from the computed σ_max: same binade ⇒ equal, else within a factor 2
        assert!(tg / tt <= 2.0 && tt / tg <= 2.0, "tol {tg:e} vs {tt:e}");
        rank_common(case, env)
    });
}

/// ‖P_got − P_true‖_F for the projector onto span(columns of a basis given row-major [rows, k]).
fn projector(data: &[f64], rows: usize, k: usize) -> Vec<f64> {
    let mut p = vec![0.0; rows * rows];
    for i in 0..rows {
        for j in 0..rows {
            p[i * rows + j] = (0..k).map(|c| data[i * k + c] * data[j * k + c]).sum();
        }
    }
    p
}
fn fro_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt()
}

#[test]
fn parity_basis() {
    run("linalg.subspace.basis", "parity-basis.json", "rank exact; ‖P − P_true‖_F ≤ 64·max(m, n)·u·σ_max/gap for the range and null projectors (Wedin); flagged cases: the failed post-condition only", &|case, env| {
        let e = &case["expected"];
        let r = &env["result"];
        let (m, n) = (e["m"].as_u64().unwrap() as usize, e["n"].as_u64().unwrap() as usize);
        let rank = e["rank_true"].as_u64().unwrap() as usize;
        assert_eq!(r["rank"].as_u64().unwrap() as usize, rank);
        assert_eq!(r["orth"]["shape"], json!([m, rank]));
        assert_eq!(r["null"]["shape"], json!([n, n - rank]));
        if !failed_pc(case).is_empty() {
            return (0.0, json!({"flagged": true}));
        }
        let po = projector(&fvec(&r["orth"]["data"]), m, rank);
        let pot = projector(&fvec(&e["orth_true"]), m, rank);
        let pn = projector(&fvec(&r["null"]["data"]), n, n - rank);
        let pnt = fvec(&e["null_projector_true"]);
        let d = fro_diff(&po, &pot).max(fro_diff(&pn, &pnt));
        let tol = match e["gap_true"].as_f64() {
            Some(g) => 64.0 * m.max(n) as f64 * U * e["sigma_max_true"].as_f64().unwrap() / g,
            None => 64.0 * m.max(n) as f64 * U,
        }
        .max(64.0 * m.max(n) as f64 * U);
        (d / tol, json!({"projector_error": d, "tolerance": tol}))
    });
}

#[test]
fn parity_pinv() {
    run("linalg.matrix.pinv", "parity-pinv.json", "rank exact; ‖X − X_true‖_F/‖X_true‖_F ≤ 64·max(m, n)·u·max(1, κ_r) (κ_r = σ_1/σ_r of the truth); flagged cases: the failed post-condition only", &|case, env| {
        let e = &case["expected"];
        let r = &env["result"];
        assert_eq!(r["rank"], e["rank_true"]);
        if !failed_pc(case).is_empty() {
            return (0.0, json!({"flagged": true}));
        }
        let (m, n) = (e["m"].as_f64().unwrap(), e["n"].as_f64().unwrap());
        let xt = fvec(&e["X_true"]);
        let xg = fvec(&r["X"]["data"]);
        let nt = xt.iter().map(|v| v * v).sum::<f64>().sqrt();
        let d = fro_diff(&xg, &xt) / nt.max(f64::MIN_POSITIVE);
        let d = if nt == 0.0 { fro_diff(&xg, &xt) } else { d };
        let k = e["cond_r_true"].as_f64().unwrap_or(1.0).max(1.0);
        let tol = 64.0 * m.max(n) * U * k;
        (d / tol, json!({"rel_error": d, "cond_r_true": k}))
    });
}

#[test]
fn parity_norm() {
    run("linalg.matrix.norm", "parity-norm.json", "|Δ|/|norm_true| ≤ 4·(N + 6)·u (N = number of elements; direct formulas) or 32·max(m, n)·u (matrix 2-norm by SVD)", &|case, env| {
        let e = &case["expected"];
        let r = &env["result"];
        assert_eq!(r["treated_as"], e["treated_as"]);
        let t = e["norm_true"].as_f64().unwrap();
        let g = r["norm"].as_f64().unwrap();
        let svd = e["treated_as"] == "matrix" && case["request"].get("p").map(|p| p == 2).unwrap_or(true);
        let tol = if svd { 32.0 * e["max_dim"].as_f64().unwrap() * U } else { 4.0 * (e["n_elements"].as_f64().unwrap() + 6.0) * U };
        let d = (g - t).abs() / t.abs().max(f64::MIN_POSITIVE);
        (d / tol, json!({"rel_error": d, "relative_error_bound_reported": r["relative_error_bound"]}))
    });
}

#[test]
fn parity_cond() {
    run("linalg.matrix.cond", "parity-cond.json", "|Δ|/cond_true ≤ 64·max(m, n)·u·cond_true (σ_min and A⁻¹ have only absolute accuracy); singular (exact zero pivot): \"Inf\"", &|case, env| {
        let e = &case["expected"];
        let r = &env["result"];
        match e["cond_true"].as_f64() {
            None => {
                assert_eq!(r["cond"], "Inf");
                (0.0, json!({"cond": "Inf"}))
            }
            Some(t) => {
                let g = r["cond"].as_f64().unwrap();
                let tol = 64.0 * e["max_dim"].as_f64().unwrap() * U * t;
                let d = (g - t).abs() / t;
                (d / tol, json!({"rel_error": d, "cond_true": t, "relative_error_estimate": r["relative_error_estimate"], "estimate_covers": r["relative_error_estimate"].as_f64().map(|x| x >= d)}))
            }
        }
    });
}
