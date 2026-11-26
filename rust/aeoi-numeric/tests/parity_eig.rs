//! linalg.decomp.eig (spec v0.4.11, 10.22) against the exact eigenvalues, eigenvectors and condition numbers
//! of the stored matrix (mpmath, 50 digits); numpy (LAPACK) values recorded. Golden:
//! golden/python-control/linalg.decomp.eig/0.4.0 (scripts/golden/gen_golden_eig.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

type C = (f64, f64);
fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn cvec(v: &Value) -> Vec<C> {
    v.as_array().unwrap().iter().map(|z| match z {
        Value::Array(p) => (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()),
        x => (x.as_f64().unwrap(), 0.0),
    }).collect()
}
fn abs(z: C) -> f64 {
    z.0.hypot(z.1)
}
fn sub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
fn key_le(a: C, b: C) -> bool {
    (a.0, a.1.abs(), (a.1 < 0.0) as u8) <= (b.0, b.1.abs(), (b.1 < 0.0) as u8)
}

#[test]
fn parity_eig() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "linalg.decomp.eig").unwrap();
    let dir = spec_root().join("golden/python-control/linalg.decomp.eig/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n_cases, mut n_err, mut worst_val, mut worst_vec) = (0, 0, 0.0f64, 0.0f64);
    let mut rows = Vec::new();
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n_cases += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let r = &env["result"];
        assert_eq!(r["structure"], x["structure"], "{fam}");
        let vals = cvec(&r["values"]["data"]);
        let n = vals.len();
        for k in 1..n {
            assert!(key_le(vals[k - 1], vals[k]), "{fam}: ordering {vals:?}");
        }
        let truth = cvec(&x["values_true"]);
        let cond: Vec<f64> = x["eigenvalue_condition_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap_or(f64::INFINITY)).collect();
        let gaps: Vec<f64> = x["gaps_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap_or(f64::INFINITY)).collect();
        let anorm = x["norm_fro"].as_f64().unwrap().max(f64::MIN_POSITIVE);
        // greedy nearest matching (the sort of near-equal real parts may differ)
        let mut used = vec![false; n];
        let mut matched = vec![0usize; n];
        for k in 0..n {
            let j = (0..n).filter(|&j| !used[j]).min_by(|&a, &b| abs(sub(vals[k], truth[a])).partial_cmp(&abs(sub(vals[k], truth[b]))).unwrap()).unwrap();
            used[j] = true;
            matched[k] = j;
            let err = abs(sub(vals[k], truth[j]));
            let defective = !(cond[j] <= 1e8);
            let tol = if defective { 1e-4 * anorm } else { 1e-12 * anorm * cond[j].max(1.0) };
            assert!(err <= tol, "{fam}: λ_{k} = {:?} vs {:?}, |Δ| = {err:e} > {tol:e}", vals[k], truth[j]);
            if !defective {
                worst_val = worst_val.max(err / tol);
            }
        }
        // eigenvector normalisation, conjugate pairs, accuracy
        if !r["vectors"].is_null() {
            let data = cvec(&r["vectors"]["data"]);
            let col = |k: usize| -> Vec<C> { (0..n).map(|i| data[i * n + k]).collect() };
            let vt: Vec<Vec<C>> = x["vectors_true"].as_array().unwrap().iter().map(cvec).collect();
            for k in 0..n {
                let v = col(k);
                let nrm = v.iter().map(|z| z.0 * z.0 + z.1 * z.1).sum::<f64>().sqrt();
                assert!((nrm - 1.0).abs() < 1e-14, "{fam}: ‖v_{k}‖ = {nrm}");
                let big = v.iter().map(|z| abs(*z)).fold(0.0, f64::max);
                let idx = v.iter().position(|z| abs(*z) == big).unwrap();
                assert!(v[idx].1 == 0.0 && v[idx].0 > 0.0, "{fam}: v_{k}[{idx}] = {:?} not real positive", v[idx]);
                if k + 1 < n && vals[k].1 > 0.0 && vals[k + 1] == (vals[k].0, -vals[k].1) {
                    let w = col(k + 1);
                    assert!(v.iter().zip(&w).all(|(a, b)| a.0 == b.0 && a.1 == -b.1), "{fam}: pair {k} not conjugate");
                }
                let j = matched[k];
                if gaps[j] > 1e-8 * anorm && cond[j] <= 1e8 {
                    let t = &vt[j];
                    let ip = v.iter().zip(t).fold((0.0, 0.0), |s, (a, b)| (s.0 + a.0 * b.0 + a.1 * b.1, s.1 + a.0 * b.1 - a.1 * b.0));
                    // ‖v − e^{iφ}·t‖₂ with the best phase (no 1 − |⟨v,t⟩|² cancellation)
                    let ph = (ip.0 / abs(ip), ip.1 / abs(ip));
                    let sin = v.iter().zip(t).map(|(a, b)| {
                        let pb = (ph.0 * b.0 + ph.1 * b.1, ph.0 * b.1 - ph.1 * b.0);
                        (a.0 - pb.0).powi(2) + (a.1 - pb.1).powi(2)
                    }).sum::<f64>().sqrt();
                    let tol = 1e-10 * (anorm / gaps[j]).max(1.0) * cond[j].max(1.0);
                    assert!(sin <= tol, "{fam}: vector {k} angle {sin:e} > {tol:e}");
                    worst_vec = worst_vec.max(sin / tol);
                }
            }
        }
        for c in ["residual_small", "orthonormal", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} ({})", env["trust"]["residual"]);
        }
        // condition numbers: within a factor 2 of the truth while the eigenvalue is not (nearly) defective
        let est: Vec<f64> = env["trust"]["condition"]["eigenvalue_condition"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap_or(f64::INFINITY)).collect();
        for k in 0..n {
            let j = matched[k];
            if cond[j] <= 1e8 {
                let q = est[k] / cond[j];
                assert!((0.5..=2.0).contains(&q), "{fam}: cond_{k} {} vs {}", est[k], cond[j]);
            }
        }
        let codes: Vec<&str> = env["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        match case.get("expect_warning").and_then(|v| v.as_str()) {
            Some(w) => assert!(codes.contains(&w), "{fam}: expected {w}, got {codes:?}"),
            None => assert!(codes.is_empty(), "{fam}: unexpected {codes:?}"),
        }
        rows.push(json!({"family": fam, "n": n, "structure": r["structure"], "backward_error": env["trust"]["residual"]["backward_error"], "max_condition_true": cond.iter().cloned().fold(0.0, f64::max)}));
    }
    assert!(n_cases >= 20 && n_err >= 5);
    println!("eig parity: {n_cases} cases ({n_err} errors); worst value error / tolerance {worst_val:.2e}; worst vector angle / tolerance {worst_vec:.2e}");
    std::fs::write(spec_root().join("reports/parity-eig.json"), serde_json::to_string_pretty(&json!({
        "reference": "exact eigenvalues / eigenvectors / condition numbers of the stored matrix (mpmath 50 digits); numpy (LAPACK dsyevd / dgeev) values recorded. Not MATLAB.",
        "cases": n_cases, "error_cases": n_err, "worst_value_error_over_tolerance": worst_val, "worst_vector_angle_over_tolerance": worst_vec,
        "tolerance": "|Δλ_k| ≤ 1e-12·‖A‖_F·max(1, cond_k) (1e-4·‖A‖_F if cond_k > 1e8); sin∠(v̂_k, v_k) ≤ 1e-10·max(1, ‖A‖_F/gap_k)·max(1, cond_k) for simple, well separated eigenvalues",
        "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
