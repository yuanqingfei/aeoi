//! calculus.quadrature.integral / trapz (spec v0.4.15, 10.31–10.32) against mpmath.quad at 50 digits and exact
//! rational sums of the stored data; scipy quad / numpy trapezoid recorded. Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fl(v: &Value) -> Vec<f64> {
    match v {
        Value::Array(a) => a.iter().map(|x| x.as_f64().unwrap()).collect(),
        Value::Object(_) => fl(&v["data"]),
        x => vec![x.as_f64().unwrap()],
    }
}
fn cases(op: &str) -> Vec<Value> {
    let mut f: Vec<_> = std::fs::read_dir(spec_root().join(format!("golden/python-control/{op}/0.4.0"))).unwrap().map(|e| e.unwrap().path()).collect();
    f.sort();
    f.iter().map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()).collect()
}

#[test]
fn parity_quad() {
    let reg = aeoi_numeric::registry();
    let mut report = json!({});
    for op_id in ["calculus.quadrature.integral", "calculus.quadrature.trapz"] {
        let op = aeoi_numeric::find(&reg, op_id).unwrap();
        let (mut n, mut n_err, mut worst) = (0, 0, 0.0f64);
        let mut rows = Vec::new();
        for case in cases(op_id) {
            let fam = case["case_family"].as_str().unwrap();
            let env = op.call(&case["request"], &HostCtx::default());
            n += 1;
            if let Some(code) = case.get("expected_error") {
                assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
                n_err += 1;
                continue;
            }
            assert_eq!(env["status"], "ok", "{fam}: {} {}", env["error"], env["trust"]);
            let x = &case["expected"];
            let r = if op_id.ends_with("integral") {
                let v = env["result"]["value"].as_f64().unwrap();
                let t = x["value_true"].as_f64().unwrap();
                let tol = x["abs_tol"].as_f64().unwrap().max(x["rel_tol"].as_f64().unwrap() * t.abs());
                let e = (v - t).abs() / tol.max(4.0 * f64::EPSILON * t.abs());
                assert!(e <= 1.0, "{fam}: {v:e} vs {t:e} (tolerance {tol:e})");
                let est = env["result"]["error_estimate"].as_f64().unwrap();
                assert!((v - t).abs() <= est.max(4.0 * f64::EPSILON * t.abs()) * 10.0, "{fam}: error estimate {est:e} misses the actual error {:e}", (v - t).abs());
                e
            } else {
                let v = fl(&env["result"]["value"]);
                let t = fl(&x["value_true"]);
                let e = v.iter().zip(&t).map(|(a, b)| (a - b).abs() / (f64::EPSILON * b.abs()).max(f64::MIN_POSITIVE)).fold(0.0, f64::max);
                assert!(e <= 1.0, "{fam}: {v:?} vs {t:?} (more than one rounding)");
                if case["request"].get("cumulative") == Some(&json!(true)) {
                    let c = fl(&env["result"]["cumulative"]);
                    let ct: Vec<f64> = x["cumulative_true"].as_array().unwrap().iter().flat_map(fl).collect();
                    assert!(c.iter().zip(&ct).all(|(a, b)| (a - b).abs() <= f64::EPSILON * b.abs()), "{fam}: cumulative");
                }
                e
            };
            worst = worst.max(r);
            rows.push(json!({"family": fam, "error_over_tolerance": r}));
        }
        println!("{op_id} parity: {n} cases ({n_err} errors); worst error / tolerance {worst:.2e}");
        report[op_id] = json!({"cases": n, "error_cases": n_err, "worst_error_over_tolerance": worst, "cases_detail": rows});
    }
    report["reference"] = json!("integral: mpmath.quad 50 digits (cross-checked against closed forms); trapz: exact rational sums of the stored data. Not MATLAB.");
    report["tolerance"] = json!({"integral": "|I − I*| ≤ max(abs_tol, rel_tol·|I*|), and the reported error estimate ≥ the actual error / 10", "trapz": "≤ one rounding (ε·|I*|), compensated summation"});
    std::fs::write(spec_root().join("reports/parity-quad.json"), serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
}
