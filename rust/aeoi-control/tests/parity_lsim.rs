//! control.lti.lsim (spec v0.4.5, 10.15) against scipy.signal.lsim / dlsim (foh cases cross-checked with
//! python-control forced_response by the generator). Golden: golden/python-control/control.lti.lsim/0.4.0
//! (scripts/golden/gen_golden_lsim.py). Not MATLAB.
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};

fn data(v: &Value) -> Vec<f64> {
    fvec(&v["data"])
}
fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0_f64, |m, x| m.max(x.abs()))
}
fn rel_to_max(got: &[f64], exp: &[f64]) -> f64 {
    if got.len() != exp.len() {
        return f64::INFINITY;
    }
    got.iter().zip(exp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / max_abs(exp).max(1.0)
}
fn warned(env: &Value, code: &str) -> bool {
    env["warnings"].as_array().is_some_and(|w| w.iter().any(|x| x["code"] == code))
}

#[test]
fn parity_lsim() {
    let reg = aeoi_control::registry();
    let op = aeoi_control::find(&reg, "control.lti.lsim").unwrap();
    let dir = spec_root().join("golden/python-control/control.lti.lsim/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err) = (0, 0);
    let (mut ey, mut ex, mut et, mut eh) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert_ne!(env["status"], "error", "{fam}: {}", env["error"]);
        let r = &env["result"];
        let x = &case["expected"];
        assert_eq!(r["n_grid_points"], x["n_grid_points"], "{fam}");
        assert_eq!(r["interpolation"], x["interpolation"], "{fam}");
        assert_eq!(r["y"]["value"]["shape"], x["shape"], "{fam}");
        let e1 = rel_to_max(&data(&r["y"]["value"]), &fvec(&x["y"]));
        assert!(e1 <= 1e-10, "{fam}: y {e1:e}");
        let e2 = data(&r["t"]["value"]).iter().zip(fvec(&x["t"])).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(e2 <= 1e-12, "{fam}: t {e2:e}");
        if let Some(xs) = x.get("x") {
            assert_eq!(r["x"]["shape"], x["x_shape"], "{fam}");
            let e3 = rel_to_max(&data(&r["x"]), &fvec(xs));
            assert!(e3 <= 1e-10, "{fam}: x {e3:e}");
            ex = ex.max(e3);
        }
        assert_eq!(env["trust"]["checks"]["half_step_consistent"], true, "{fam}: {}", env["trust"]["residual"]);
        if let Some(h) = env["trust"]["residual"]["half_step_state_rel"].as_f64() {
            eh = eh.max(h);
        }
        if fam == "siso_stiff_foh" {
            assert!(warned(&env, "W.ACCURACY.GRID_TOO_COARSE"), "{fam}");
        }
        if fam == "siso_unstable_foh" {
            assert!(warned(&env, "W.DOMAIN.UNSTABLE_SYSTEM"), "{fam}");
        }
        ey = ey.max(e1);
        et = et.max(e2);
    }
    assert!(n >= 30 && n_err >= 10, "{n} cases, {n_err} errors");
    let worst = json!({"y_rel_to_max": ey, "x_rel_to_max": ex, "t_abs": et, "half_step_state_rel": eh});
    println!("lsim parity: {n} cases ({n_err} error cases), worst {worst}");
    std::fs::write(spec_root().join("reports/parity-lsim.json"), serde_json::to_string_pretty(&json!({
        "reference": "scipy.signal.lsim (interp=True: foh, False: zoh) / dlsim; foh cross-checked with python-control 0.10.2 forced_response. Not MATLAB.",
        "cases": n, "error_cases": n_err, "worst": worst,
        "tolerances": {"y_rel_to_max": 1e-10, "x_rel_to_max": 1e-10, "t_abs": 1e-12, "half_step_state_rel": 1e-9}
    })).unwrap() + "\n").unwrap();
}
