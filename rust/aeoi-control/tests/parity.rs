//! Parity tests against python-control/scipy golden cases (golden/python-control/...).
//! These are python-control references, NOT MATLAB outputs. Tolerances follow spec ch.7.2 (v0.2).
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default, Debug)]
struct Stat {
    cases: usize,
    passed: usize,
    max_ratio: f64, // max observed error / tolerance
    max_err: BTreeMap<String, f64>,
    tol: BTreeMap<String, f64>,
    scaled_cases: usize,
    failures: Vec<String>,
}

impl Stat {
    fn err(&mut self, key: &str, e: f64, tol: f64) -> bool {
        let r = e / tol;
        let m = self.max_err.entry(key.into()).or_insert(0.0);
        if e > *m || e.is_nan() {
            *m = e;
        }
        // tolerance column = the tightest tolerance applied to this metric
        let t = self.tol.entry(key.into()).or_insert(tol);
        if tol < *t {
            *t = tol;
        }
        let rk = self.max_err.entry(format!("{key} [max err/tol]")).or_insert(0.0);
        if r > *rk {
            *rk = r;
        }
        self.tol.entry(format!("{key} [max err/tol]")).or_insert(1.0);
        if r > self.max_ratio || r.is_nan() {
            self.max_ratio = r;
        }
        e <= tol
    }
}

fn load(op: &str) -> Vec<(String, Value)> {
    let dir = spec_root().join(format!("golden/python-control/{op}/0.2.0"));
    let mut v: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("missing {}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn call(op: &str, req: &Value) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, op).unwrap();
    let env = o.call(req, &HostCtx::default());
    // conformance: every emitted error/warning code must be declared in the manifest
    let m = &o.meta().manifest;
    let declared = |k: &str, c: &Value| m[k].as_array().unwrap().iter().any(|x| x == c || x["code"] == *c);
    if env["status"] == "error" {
        assert!(declared("errors", &env["error"]["code"]), "{op}: undeclared error {}", env["error"]["code"]);
    }
    for w in env["warnings"].as_array().unwrap() {
        assert!(declared("warnings", &w["code"]), "{op}: undeclared warning {}", w["code"]);
    }
    env
}

fn check_error_and_warnings(st: &mut Stat, name: &str, case: &Value, env: &Value) -> Option<bool> {
    if let Some(code) = case.get("expected_error") {
        let ok = env["status"] == "error" && env["error"]["code"] == *code;
        if !ok {
            st.failures.push(format!("{name}: expected error {code}, got {} {}", env["status"], env["error"]["code"]));
        }
        return Some(ok);
    }
    if env["status"] == "error" {
        st.failures.push(format!("{name}: unexpected error {}", env["error"]));
        return Some(false);
    }
    if let Some(ws) = case.get("expected_warnings") {
        for w in ws.as_array().unwrap() {
            if !env["warnings"].as_array().unwrap().iter().any(|x| x["code"] == *w) {
                st.failures.push(format!("{name}: missing warning {w}"));
                return Some(false);
            }
        }
    }
    None
}

// ---------------------------------------------------------------- bode
fn bode_compare(st: &mut Stat, name: &str, env: &Value, exp: &Value) -> bool {
    let mag = fvec(&env["result"]["magnitude"]["value"]["data"]);
    let ph = fvec(&env["result"]["phase"]["value"]["data"]);
    let w = fvec(&env["result"]["w"]["value"]["data"]);
    let em = fvec(&exp["magnitude_db"]);
    let ep = fvec(&exp["phase_deg"]);
    let ew = fvec(&exp["w"]);
    let est = &env["trust"]["error_estimate"];
    let (mut tm, mut tp) = (1e-9, 1e-8);
    if est.is_object() {
        let (sm, sp) = (10.0 * f(&est["magnitude_db_abs"]), 10.0 * f(&est["phase_deg_abs"]));
        if sm > tm || sp > tp {
            st.scaled_cases += 1;
        }
        tm = tm.max(sm);
        tp = tp.max(sp);
    }
    let mut ok = mag.len() == em.len();
    let we = w.iter().zip(&ew).map(|(a, b)| ((a - b) / b).abs()).fold(0.0, f64::max);
    ok &= st.err("w_rel", we, 1e-14);
    let me = mag.iter().zip(&em).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    let pe = ph.iter().zip(&ep).map(|(a, b)| wrap180(a - b).abs()).fold(0.0, f64::max);
    // scaled tolerance (if any) is recorded separately so the class-level max stays honest
    if tm > 1e-9 {
        ok &= st.err("magnitude_db_abs (κ-scaled cases)", me, tm);
        ok &= st.err("phase_deg_abs_mod360 (κ-scaled cases)", pe, tp);
    } else {
        ok &= st.err("magnitude_db_abs", me, tm);
        ok &= st.err("phase_deg_abs_mod360", pe, tp);
    }
    if !ok {
        st.failures.push(format!("{name}: mag err {me:.3e} (tol {tm:.1e}), phase err {pe:.3e} (tol {tp:.1e})"));
    }
    ok
}

// ---------------------------------------------------------------- step
fn step_compare(st: &mut Stat, name: &str, env: &Value, exp: &Value, case: &Value) -> bool {
    let r = &env["result"];
    let y = fvec(&r["y"]["value"]["data"]);
    let ey = fvec(&exp["y"]);
    let t = fvec(&r["t"]["value"]["data"]);
    let et = fvec(&exp["t"]);
    let mut ok = y.len() == ey.len() && r["n_grid_points"] == exp["n_grid_points"];
    let scale = ey.iter().map(|v| v.abs()).fold(0.0, f64::max).max(1.0);
    let ye = y.iter().zip(&ey).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / scale;
    ok &= st.err("y_rel_to_max|y|", ye, 1e-10);
    let te = t.iter().zip(&et).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    ok &= st.err("t_abs", te, 1e-12);
    let dt = f(&env["trust"]["derived"]["dt"]);
    if case.get("expect_info_null").is_some() {
        ok &= r["info"].is_null();
    } else if let Some(einfo) = exp.get("info") {
        let rows = einfo.as_array().unwrap();
        let siso = rows.len() == 1 && rows[0].as_array().unwrap().len() == 1;
        for (i, row) in rows.iter().enumerate() {
            for (j, e) in row.as_array().unwrap().iter().enumerate() {
                let got = if siso { r["info"].clone() } else { r["info"][i][j].clone() };
                let ss = e["SteadyStateValue"].as_f64().unwrap_or(f64::NAN);
                if !(ss.abs() > 1e-12) {
                    // python-control's step_info is undefined for zero DC gain; AEOI returns null (W.DOMAIN.ZERO_DC_GAIN)
                    ok &= got.is_null();
                    continue;
                }
                let g = |k: &str| f(&got[k]["value"]);
                let pairs = [("rise_time", "RiseTime"), ("settling_time", "SettlingTime"), ("peak_time", "PeakTime")];
                for (a, b) in pairs {
                    let (x, y) = (g(a), e[b].as_f64().unwrap_or(f64::NAN));
                    let d = if x.is_nan() && y.is_nan() { 0.0 } else { (x - y).abs() };
                    ok &= st.err("info_time_abs_(tol=dt)_ratio", d / dt, 1.0 + 1e-9);
                }
                for (a, b) in [("peak", "Peak"), ("settling_min", "SettlingMin"), ("settling_max", "SettlingMax"), ("steady_state", "SteadyStateValue")] {
                    let (x, y) = (g(a), e[b].as_f64().unwrap());
                    ok &= st.err("info_value_rel", (x - y).abs() / y.abs().max(1e-300), 1e-9);
                }
                for (a, b) in [("overshoot", "Overshoot"), ("undershoot", "Undershoot")] {
                    let (x, y) = (g(a), e[b].as_f64().unwrap());
                    ok &= st.err("info_percent_abs", (x - y).abs(), 1e-6);
                }
            }
        }
    }
    if !ok {
        st.failures.push(format!("{name}: y err {ye:.3e}"));
    }
    ok
}

// ---------------------------------------------------------------- lqr
fn lqr_compare(st: &mut Stat, name: &str, env: &Value, exp: &Value) -> bool {
    let r = &env["result"];
    let s = fvec(&r["S"]["data"]);
    let k = fvec(&r["K"]["data"]);
    let es = fvec(&exp["S"]["data"]);
    let ek = fvec(&exp["K"]["data"]);
    let cond_s = f(&exp["cond_S"]);
    let mut tol = 1e-10 * (cond_s / 1e2).max(1.0);
    let est = &env["trust"]["error_estimate"];
    if est.is_object() && 10.0 * f(&est["S_rel"]) > tol {
        tol = 10.0 * f(&est["S_rel"]);
        st.scaled_cases += 1;
    }
    let se = fro_diff(&s, &es) / fro(&es);
    let ke = fro_diff(&k, &ek) / fro(&ek).max(1e-300);
    let pe = matched_set_err(&cvec(&r["closed_loop_poles"]["data"]), &cvec(&exp["closed_loop_poles"]));
    if se > 1e-9 {
        println!("    note {name}: S_rel {se:.3e} K_rel {ke:.3e} cond_S {cond_s:.2e} est {} cond_R {} tol {tol:.2e}", est["S_rel"], env["trust"]["derived"]["cond_R"]);
    }
    let mut ok = st.err("S_rel_fro", se, tol);
    ok &= st.err("K_rel_fro", ke, tol * 10.0);
    ok &= st.err("poles_rel", pe, 1e-9 * (cond_s / 1e2).max(1.0) * 10.0);
    let res = f(&env["trust"]["residual"]["care_rel_scaled"]);
    ok &= st.err("care_rel_scaled (invariant, tol 1e-12)", res, 1e-12);
    ok &= env["trust"]["checks"]["closed_loop_stable"] == true && env["trust"]["checks"]["S_psd"] == true;
    if !ok {
        st.failures.push(format!("{name}: S {se:.3e} K {ke:.3e} poles {pe:.3e} tol {tol:.1e} res {res:.2e}"));
    }
    ok
}

fn max_mismatch_ratio(op: &str, env: &Value, exp: &Value) -> f64 {
    if env["status"] == "error" {
        return f64::INFINITY;
    }
    match op {
        "control.lti.bode" => {
            let a = fvec(&env["result"]["magnitude"]["value"]["data"]);
            let b = fvec(&exp["magnitude_db"]);
            a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max) / 1e-9
        }
        "control.lti.step" => {
            let a = fvec(&env["result"]["y"]["value"]["data"]);
            let b = fvec(&exp["y"]);
            a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max) / 1e-10
        }
        _ => {
            let a = fvec(&env["result"]["S"]["data"]);
            let b = fvec(&exp["S"]["data"]);
            fro_diff(&a, &b) / fro(&b) / 1e-10
        }
    }
}

#[test]
fn parity_python_control() {
    let mut report = serde_json::Map::new();
    let mut all_ok = true;
    for op in ["control.lti.bode", "control.lti.step", "control.design.lqr"] {
        let mut st = Stat::default();
        let mut families: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut memory_order_caught = None;
        for (file, case) in load(op) {
            st.cases += 1;
            let fam = case["case_family"].as_str().unwrap().to_string();
            let name = format!("{fam}/{}", &file[..12]);
            let env = call(op, &case["request"]);
            let ok = match check_error_and_warnings(&mut st, &name, &case, &env) {
                Some(b) => b,
                None => match op {
                    "control.lti.bode" => bode_compare(&mut st, &name, &env, &case["expected"]),
                    "control.lti.step" => step_compare(&mut st, &name, &env, &case["expected"], &case),
                    _ => lqr_compare(&mut st, &name, &env, &case["expected"]),
                },
            };
            if let Some(bad) = case.get("column_major_mistake_request") {
                // The same data flattened column-major (the classic MATLAB A(:) mistake) must NOT match.
                let env2 = call(op, bad);
                let ratio = max_mismatch_ratio(op, &env2, &case["expected"]);
                memory_order_caught = Some(ratio);
                assert!(ratio > 1e3, "{op}: column-major mistake was not detected (ratio {ratio})");
            }
            let e = families.entry(fam).or_insert((0, 0));
            e.0 += 1;
            if ok {
                e.1 += 1;
                st.passed += 1;
            }
        }
        println!("\n=== {op}: {}/{} passed; max error/tolerance = {:.3e}; κ-scaled cases = {}", st.passed, st.cases, st.max_ratio, st.scaled_cases);
        for (k, v) in &st.max_err {
            println!("    {k:<45} max {v:.3e}   tol {:.1e}", st.tol[k]);
        }
        if let Some(r) = memory_order_caught {
            println!("    memory-order regression: column-major request deviates by {r:.3e} × tolerance (detected)");
        }
        for f in &st.failures {
            println!("    FAIL {f}");
        }
        all_ok &= st.failures.is_empty();
        report.insert(op.into(), json!({
            "cases": st.cases, "passed": st.passed, "max_error_over_tolerance": st.max_ratio, "kappa_scaled_cases": st.scaled_cases,
            "max_errors": st.max_err, "tolerances": st.tol, "families": families.iter().map(|(k, v)| (k.clone(), json!({"cases": v.0, "passed": v.1}))).collect::<serde_json::Map<_, _>>(),
            "memory_order_mismatch_ratio": memory_order_caught, "failures": st.failures,
        }));
    }
    let out = json!({"reference": "python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 (NOT MATLAB)", "build": aeoi_core::operator::build_string(),
                     "cpu_features": aeoi_core::operator::cpu_features(), "results": report});
    std::fs::write(spec_root().join("reports/parity-python-control.json"), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    assert!(all_ok, "parity failures (see output)");
}
