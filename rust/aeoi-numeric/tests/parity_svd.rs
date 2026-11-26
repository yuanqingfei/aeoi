//! linalg.decomp.svd (spec v0.4.11, 10.23) against the exact singular values and vectors of the stored matrix
//! (mpmath 50 digits); numpy (LAPACK dgesdd) values recorded. Golden: golden/python-control/linalg.decomp.svd/0.4.0
//! (scripts/golden/gen_golden_svd.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

#[test]
fn parity_svd() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "linalg.decomp.svd").unwrap();
    let dir = spec_root().join("golden/python-control/linalg.decomp.svd/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n_cases, mut n_err, mut worst_s, mut worst_vec) = (0, 0, 0.0f64, 0.0f64);
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
        assert_eq!(env["status"], "ok", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let r = &env["result"];
        let s = fvec(&r["s"]["data"]);
        let st = fvec(&x["s_true"]);
        let smax = st[0];
        for k in 0..s.len() {
            if k > 0 {
                assert!(s[k - 1] >= s[k], "{fam}: not descending");
            }
            let tol = 1e-12 * smax;
            let e = (s[k] - st[k]).abs();
            assert!(e <= tol, "{fam}: s_{k} = {} vs {}, |Δ| = {e:e} > {tol:e}", s[k], st[k]);
            if tol > 0.0 {
                worst_s = worst_s.max(e / tol);
            }
        }
        assert_eq!(r["rank"], x["rank"], "{fam}");
        for c in ["reconstructs", "orthonormal_u", "orthonormal_v", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} ({})", env["trust"]["residual"]);
        }
        let (m, n) = (x["shape"][0].as_u64().unwrap() as usize, x["shape"][1].as_u64().unwrap() as usize);
        let pp = m.min(n);
        let mode = case["request"].get("mode").and_then(|v| v.as_str()).unwrap_or("thin");
        assert_eq!(r["mode"], mode, "{fam}");
        if mode == "values" {
            assert!(r["U"].is_null() && r["V"].is_null(), "{fam}");
        } else {
            let (uc, vc) = if mode == "full" { (m, n) } else { (pp, pp) };
            assert_eq!(r["U"]["shape"], json!([m, uc]), "{fam}");
            assert_eq!(r["V"]["shape"], json!([n, vc]), "{fam}");
            let ud = fvec(&r["U"]["data"]);
            let vd = fvec(&r["V"]["data"]);
            let ut: Vec<Vec<f64>> = x["u_true"].as_array().unwrap().iter().map(fvec).collect();
            let vt: Vec<Vec<f64>> = x["v_true"].as_array().unwrap().iter().map(fvec).collect();
            for k in 0..pp {
                let u: Vec<f64> = (0..m).map(|i| ud[i * uc + k]).collect();
                let v: Vec<f64> = (0..n).map(|i| vd[i * vc + k]).collect();
                // sign rule: largest-modulus entry of u_k (first on ties) positive
                let big = u.iter().map(|z| z.abs()).fold(0.0, f64::max);
                let idx = u.iter().position(|z| z.abs() == big).unwrap();
                assert!(u[idx] >= 0.0, "{fam}: u_{k} sign");
                let gap = x["gaps_true"][k].as_f64().unwrap_or(f64::INFINITY);
                if smax > 0.0 && gap > 1e-8 * smax && st[k] > 1e-8 * smax {
                    let sg = if u.iter().zip(&ut[k]).map(|(a, b)| a * b).sum::<f64>() >= 0.0 { 1.0 } else { -1.0 };
                    let du = u.iter().zip(&ut[k]).map(|(a, b)| (a - sg * b).powi(2)).sum::<f64>().sqrt();
                    let dv = v.iter().zip(&vt[k]).map(|(a, b)| (a - sg * b).powi(2)).sum::<f64>().sqrt();
                    let tol = 1e-10 * (smax / gap).max(1.0) * (smax / st[k]).max(1.0);
                    assert!(du <= tol && dv <= tol, "{fam}: singular vectors {k}: {du:e}, {dv:e} > {tol:e}");
                    worst_vec = worst_vec.max(du.max(dv) / tol);
                }
            }
        }
        assert!(env["warnings"].as_array().unwrap().is_empty(), "{fam}: {}", env["warnings"]);
        rows.push(json!({"family": fam, "shape": [m, n], "mode": mode, "rank": r["rank"], "cond2": r["cond2"], "reconstruction": env["trust"]["residual"]["backward_error"]}));
    }
    assert!(n_cases >= 18 && n_err >= 5);
    println!("svd parity: {n_cases} cases ({n_err} errors); worst |Δs| / tolerance {worst_s:.2e}; worst vector error / tolerance {worst_vec:.2e}");
    std::fs::write(spec_root().join("reports/parity-svd.json"), serde_json::to_string_pretty(&json!({
        "reference": "exact singular values / vectors of the stored matrix (mpmath 50 digits); numpy (LAPACK dgesdd) values recorded. Not MATLAB.",
        "cases": n_cases, "error_cases": n_err, "worst_value_error_over_tolerance": worst_s, "worst_vector_error_over_tolerance": worst_vec,
        "tolerance": "|Δs_k| ≤ 1e-12·s_max (T1, absolute); ‖û_k ∓ u_k‖, ‖v̂_k ∓ v_k‖ ≤ 1e-10·max(1, s_max/gap_k)·max(1, s_max/s_k) for separated, non-negligible s_k",
        "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
