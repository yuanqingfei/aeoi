//! Parity against the high-precision golden store golden/mpmath/<op>/<current op version>/ (spec V-1 ②):
//! mpmath 50-digit solutions of the ill-conditioned families found by the tpt-control cross-check
//! (cond(R) up to 1e12, discrete Lyapunov with ρ(A) → 1, continuous near the imaginary axis).
//! Reference values are mpmath, NOT MATLAB and NOT python-control. Fixed tolerances (no V-6 relaxation):
//! lqr S, K and lyap X normwise relative ≤ 1e-12; closed-loop poles (matched set, |Δλ|/max(1,|λ|)) ≤ max(1e-10,
//! 1e2·ε·‖A − BK‖_F) — the f64 eigenvalue backward-error floor, since ‖K‖ reaches 1e6 in these families.
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const TOL_X: f64 = 1e-12;
const TOL_POLES: f64 = 1e-10;

#[test]
fn parity_mpmath_high_precision() {
    let reg = aeoi_control::registry();
    let mut report = serde_json::Map::new();
    let mut failures = vec![];
    for op in ["control.design.lqr", "control.math.lyap"] {
        let o = aeoi_control::find(&reg, op).unwrap();
        let ver = o.meta().manifest["version"].as_str().unwrap().to_string();
        let dir = spec_root().join(format!("golden/mpmath/{op}/{ver}"));
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap_or_else(|_| panic!("missing {}", dir.display())).map(|e| e.unwrap().path()).collect();
        files.sort();
        let mut fam: BTreeMap<String, Value> = BTreeMap::new();
        for p in files {
            let case: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            let name = format!("{}/{}", case["case_family"].as_str().unwrap(), p.file_name().unwrap().to_string_lossy());
            let env = o.call(&case["request"], &HostCtx::default());
            let m = &o.meta().manifest;
            for w in env["warnings"].as_array().unwrap() {
                assert!(m["warnings"].as_array().unwrap().iter().any(|x| *x == w["code"]), "{op}: undeclared warning {}", w["code"]);
            }
            let exp = &case["expected"];
            let mut errs = serde_json::Map::new();
            let ok = if env["status"] == "error" {
                failures.push(format!("{op} {name}: unexpected error {}", env["error"]["code"]));
                false
            } else if op == "control.design.lqr" {
                let r = &env["result"];
                let (s, es) = (fvec(&r["S"]["data"]), fvec(&exp["S"]["data"]));
                let (k, ek) = (fvec(&r["K"]["data"]), fvec(&exp["K"]["data"]));
                let se = fro_diff(&s, &es) / fro(&es);
                let ke = fro_diff(&k, &ek) / fro(&ek).max(1e-300);
                let pe = matched_set_err(&cvec(&r["closed_loop_poles"]["data"]), &cvec(&exp["closed_loop_poles"]));
                // eigenvalues of A − BK in f64 cannot beat the eigenvalue backward error u·‖A − BK‖_F (‖K‖ ~ 1e6 here)
                let (a, b) = (&case["request"]["A"], &case["request"]["B"]);
                let (n, m) = (a["shape"][0].as_u64().unwrap() as usize, b["shape"][1].as_u64().unwrap() as usize);
                let (av, bv) = (fvec(&a["data"]), fvec(&b["data"]));
                let acl: Vec<f64> = (0..n * n).map(|idx| { let (i, j) = (idx / n, idx % n); av[idx] - (0..m).map(|l| bv[i * m + l] * ek[l * n + j]).sum::<f64>() }).collect();
                let tol_p = TOL_POLES.max(1e2 * f64::EPSILON * fro(&acl));
                errs.insert("poles_tol".into(), json!(tol_p));
                errs.insert("S_rel_fro".into(), json!(se));
                errs.insert("K_rel_fro".into(), json!(ke));
                errs.insert("poles_rel".into(), json!(pe));
                se <= TOL_X && ke <= TOL_X && pe <= tol_p && env["trust"]["checks"]["closed_loop_stable"] == true
            } else {
                let (x, ex) = (fvec(&env["result"]["X"]["data"]), fvec(&exp["X"]["data"]));
                let xe = fro_diff(&x, &ex) / fro(&ex);
                errs.insert("X_rel_fro".into(), json!(xe));
                xe <= TOL_X
            };
            if !ok && env["status"] != "error" {
                failures.push(format!("{op} {name}: {}", Value::Object(errs.clone())));
            }
            let e = fam.entry(case["case_family"].as_str().unwrap().to_string()).or_insert(json!({"cases": 0, "passed": 0, "max": {}}));
            e["cases"] = json!(e["cases"].as_u64().unwrap() + 1);
            if ok {
                e["passed"] = json!(e["passed"].as_u64().unwrap() + 1);
            }
            for (k, v) in errs {
                let cur = e["max"][&k].as_f64().unwrap_or(0.0);
                e["max"][&k] = json!(cur.max(v.as_f64().unwrap()));
            }
        }
        report.insert(op.into(), json!({"operator_version": ver, "golden_store": format!("golden/mpmath/{op}/{ver}"), "families": fam}));
    }
    let out = json!({"reference": "mpmath 50-digit (gen_golden_mpmath.py); NOT MATLAB, NOT python-control",
                     "tolerances": {"S_K_X_rel_fro": TOL_X, "closed_loop_poles_rel": "max(1e-10, 1e2·ε·‖A − B K_ref‖_F)", "relaxation": "poles only: eigenvalue backward-error floor of A − BK"},
                     "operators": report, "failures": failures});
    std::fs::write(spec_root().join("reports/parity-mpmath.json"), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    assert!(failures.is_empty(), "{failures:#?}");
}
