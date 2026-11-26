//! optim.linear.linprog / optim.local.fmincon / optim.local.fminsearch (spec v0.4.14, 10.28–10.30) against
//! scipy (HiGHS, SLSQP, Nelder–Mead) and analytic optima. Golden: golden/python-control/optim.*/0.4.0
//! (scripts/golden/gen_golden_optim.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fl(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn rel(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max) / b.iter().fold(1.0f64, |m, v| m.max(v.abs()))
}

fn run(op_id: &str, check: &dyn Fn(&str, &Value, &Value) -> f64) -> Value {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, op_id).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(spec_root().join(format!("golden/python-control/{op_id}/0.4.0"))).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err, mut worst) = (0, 0, 0.0f64);
    let mut rows = Vec::new();
    for p in files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert_eq!(env["status"], "ok", "{fam}: {} {}", env["error"], env["warnings"]);
        let r = check(fam, &case["expected"], &env);
        worst = worst.max(r);
        rows.push(json!({"family": fam, "error_over_tolerance": r, "iterations": env["result"]["iterations"]}));
    }
    println!("{op_id} parity: {n} cases ({n_err} errors); worst error / tolerance {worst:.2e}");
    json!({"cases": n, "error_cases": n_err, "worst_error_over_tolerance": worst, "cases_detail": rows})
}

#[test]
fn parity_optim() {
    let lp = run("optim.linear.linprog", &|fam, x, env| {
        assert_eq!(env["trust"]["checks"]["kkt"], true, "{fam}: {}", env["trust"]["residual"]);
        let f = env["result"]["fval"].as_f64().unwrap();
        let fr = x["fval"].as_f64().unwrap();
        let ef = (f - fr).abs() / (1e-9 * fr.abs().max(1.0));
        assert!(ef <= 1.0, "{fam}: fval {f} vs {fr}");
        let mut e = ef;
        if x["unique"] == true {
            let want = if x["x_exact"].is_array() { fl(&x["x_exact"]) } else { fl(&x["x_highs"]) };
            let ex = rel(&fl(&env["result"]["x"]), &want) / 1e-9;
            assert!(ex <= 1.0, "{fam}: x {} vs {want:?}", env["result"]["x"]);
            e = e.max(ex);
        }
        e
    });
    let con = run("optim.local.fmincon", &|fam, x, env| {
        assert_eq!(env["trust"]["checks"]["kkt"], true, "{fam}: {}", env["trust"]["residual"]);
        let want = if x["x_exact"].is_array() { fl(&x["x_exact"]) } else { fl(&x["x_slsqp"]) };
        let e = rel(&fl(&env["result"]["x"]), &want) / 1e-6;
        assert!(e <= 1.0, "{fam}: x {} vs {want:?}", env["result"]["x"]);
        e
    });
    let nm = run("optim.local.fminsearch", &|fam, x, env| {
        assert_eq!(env["trust"]["checks"]["probe_local_minimum"], true, "{fam}: {}", env["trust"]);
        let tol = 10.0 * x["tol_x"].as_f64().unwrap();
        let e = rel(&fl(&env["result"]["x"]), &fl(&x["x_exact"])) / tol;
        assert!(e <= 1.0, "{fam}: x {} vs {}", env["result"]["x"], x["x_exact"]);
        e
    });
    std::fs::write(spec_root().join("reports/parity-optim.json"), serde_json::to_string_pretty(&json!({
        "reference": "scipy.optimize (linprog HiGHS, minimize SLSQP ftol 1e-14, Nelder-Mead) and analytic optima; not MATLAB",
        "tolerance": {"linprog": "fval ≤ 1e-9·max(1, |f*|); x ≤ 1e-9 (relative) when the optimum is unique", "fmincon": "x ≤ 1e-6 relative (analytic optimum, else SLSQP)", "fminsearch": "x ≤ 10·tol_x relative to the analytic optimum"},
        "linprog": lp, "fmincon": con, "fminsearch": nm
    })).unwrap() + "\n").unwrap();
}
