//! Batch-3 parity: series/parallel/feedback/stepinfo against python-control,
//! pid/pade against mpmath closed forms. Not MATLAB.
mod common;
use common::*;
use aeoi_core::HostCtx;
use serde_json::{json, Value};
fn load(engine: &str, op: &str) -> Vec<(String, Value)> {
    let dir = spec_root().join(format!("golden/{engine}/{op}/0.3.0"));
    let mut v: Vec<(String, Value)> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().map(|x| x == "json").unwrap_or(false)).map(|p| {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let val = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        (name, val)
    }).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn call(op: &str, req: &Value) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, op).unwrap();
    let env = o.call(req, &HostCtx::default());
    let m = &o.meta().manifest;
    if env["status"] == "error" {
        assert!(m["errors"].as_array().unwrap().iter().any(|c| c == &env["error"]["code"]), "{op} undeclared {}", env["error"]["code"]);
    }
    for w in env["warnings"].as_array().unwrap() {
        let code = &w["code"];
        let framework = code == "W.EVIDENCE.POSTCONDITION_FAILED";
        assert!(framework || m["warnings"].as_array().unwrap().iter().any(|c| c == code), "{op} undeclared warning {code}");
    }
    env
}

fn rel_coeff(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    let scale = b.iter().map(|x| x.abs()).fold(1.0_f64, f64::max);
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max) / scale
}

fn tf_of(env: &Value) -> (Vec<f64>, Vec<f64>) {
    let s = &env["result"]["system"];
    (fvec(&s["num"]), fvec(&s["den"]))
}

fn ss_rel(env: &Value, exp: &Value) -> f64 {
    ["A", "B", "C", "D"].iter().map(|k| rel_coeff(&fvec(&env["result"]["system"][k]["data"]), &fvec(&exp[k]["data"]))).fold(0.0, f64::max)
}

fn cmp_tf(env: &Value, exp: &Value) -> f64 {
    let (n, d) = tf_of(env);
    rel_coeff(&n, &fvec(&exp["num"])).max(rel_coeff(&d, &fvec(&exp["den"])))
}

struct Stat { cases: usize, passed: usize, max_ratio: f64, failures: Vec<String> }

fn run_op(op: &str, engine: &str, tol: f64, st: &mut Stat) -> Option<f64> {
    let mut mo = None;
    for (file, case) in load(engine, op) {
        st.cases += 1;
        let fam = case["case_family"].as_str().unwrap();
        let name = format!("{fam}/{}", &file[..12.min(file.len())]);
        let env = call(op, &case["request"]);
        let ok = if let Some(code) = case.get("expected_error") {
            env["status"] == "error" && env["error"]["code"] == *code
        } else if env["status"] == "error" {
            st.failures.push(format!("{name}: {}", env["error"]["message"]));
            false
        } else {
            if let Some(ws) = case.get("expected_warnings") {
                for w in ws.as_array().unwrap() {
                    if !env["warnings"].as_array().unwrap().iter().any(|x| &x["code"] == w) {
                        st.failures.push(format!("{name}: missing {w}"));
                    }
                }
            }
            let e = match op {
                "control.lti.stepinfo" => stepinfo_err(&env, &case["expected"]),
                "control.design.pid" | "control.lti.pade" => cmp_tf(&env, &case["expected"]),
                _ if case["expected"]["kind"] == "ss" => ss_rel(&env, &case["expected"]),
                _ => cmp_tf(&env, &case["expected"]),
            };
            let ratio = if tol == 0.0 { if e == 0.0 { 0.0 } else { f64::INFINITY } } else { e / tol };
            if ratio > st.max_ratio { st.max_ratio = ratio; }
            if e > tol {
                st.failures.push(format!("{name}: err {e:.3e} > tol {tol:.1e}"));
                false
            } else {
                st.failures.iter().any(|f| f.starts_with(&name)) == false
            }
        };
        if let Some(bad) = case.get("column_major_mistake_request") {
            let env2 = call(op, bad);
            let dev = if env2["status"] == "error" {
                f64::INFINITY
            } else {
                let e = if op == "control.lti.stepinfo" { stepinfo_err(&env2, &case["expected"]) } else if case["expected"]["kind"] == "ss" { ss_rel(&env2, &case["expected"]) } else { cmp_tf(&env2, &case["expected"]) };
                if tol == 0.0 { f64::INFINITY } else { e / tol }
            };
            mo = Some(dev);
            if dev < 1e3 {
                st.failures.push(format!("{op}: column-major mistake ratio {dev}"));
            }
        }
        if ok && !st.failures.iter().any(|f| f.starts_with(&name)) {
            st.passed += 1;
        }
    }
    mo
}

fn num_or_null(v: &Value) -> Option<f64> {
    if v.is_null() { None } else { Some(f(v)) }
}

fn stepinfo_err(env: &Value, exp: &Value) -> f64 {
    let info = &env["result"]["info"];
    let mut e = 0.0_f64;
    for (k, tol_scale) in [("rise_time", 1.0), ("settling_time", 1.0), ("peak_time", 1.0), ("overshoot", 1.0), ("undershoot", 1.0), ("peak", 1.0), ("steady_state", 1.0), ("settling_min", 1.0), ("settling_max", 1.0)] {
        if exp.get(k).is_none() { continue; }
        let got = num_or_null(&info[k]["value"]);
        let want = if exp[k].is_null() { None } else { Some(f(&exp[k])) };
        e = e.max(match (got, want) {
            (None, None) => 0.0,
            (Some(a), Some(b)) => (a - b).abs() / (tol_scale as f64).max(b.abs()).max(1.0),
            _ => 1.0,
        });
    }
    if exp.get("stable").and_then(|s| s.as_bool()) == Some(env["result"]["stable"].as_bool().unwrap()) {
        e
    } else if exp.get("stable").is_some() {
        1.0
    } else {
        e
    }
}

#[test]
fn parity_batch3() {
    let specs = [
        ("control.lti.series", "python-control", 1e-12),
        ("control.lti.parallel", "python-control", 1e-12),
        ("control.lti.feedback", "python-control", 1e-10),
        ("control.lti.stepinfo", "python-control", 1e-8),
        ("control.design.pid", "mpmath", 1e-12),
        ("control.lti.pade", "mpmath", 1e-12),
    ];
    let mut all_ok = true;
    let mut report = serde_json::Map::new();
    for (op, engine, tol) in specs {
        let mut st = Stat { cases: 0, passed: 0, max_ratio: 0.0, failures: vec![] };
        let mo = run_op(op, engine, tol, &mut st);
        println!("\n=== {op}: {}/{}  max err/tol {:.3e}", st.passed, st.cases, st.max_ratio);
        if let Some(r) = mo { println!("    memory-order deviation {r:.3e} × tol"); }
        for f in &st.failures { println!("    FAIL {f}"); }
        all_ok &= st.failures.is_empty() && st.passed == st.cases;
        report.insert(op.into(), json!({"cases": st.cases, "passed": st.passed, "max_error_over_tolerance": st.max_ratio, "tolerance": tol, "failures": st.failures}));
    }
    let _ = std::fs::create_dir_all(spec_root().join("reports"));
    std::fs::write(spec_root().join("reports/parity-python-control-b3.json"), serde_json::to_string_pretty(&json!({
        "reference": "python-control 0.10.2 for interconnection and stepinfo; mpmath 1.3.0 (50 dps) for pid and pade. Not MATLAB.",
        "results": Value::Object(report)
    })).unwrap()).ok();
    assert!(all_ok, "batch-3 parity failures");
}
