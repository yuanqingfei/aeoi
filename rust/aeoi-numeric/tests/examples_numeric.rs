//! Published examples of the aeoi-numeric operators (examples/<id>/): each request reproduces its published
//! response bit-for-bit on the same platform (platform + cpu_features + libm, spec P-1), agrees with the
//! reference values in reference.python-control.json, and the error example gives its published code with a
//! remediation. Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn read(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))).unwrap()
}
fn call(op: &str, req: &Value) -> Value {
    let reg = aeoi_numeric::registry();
    aeoi_numeric::find(&reg, op).unwrap().call(req, &HostCtx::default())
}
fn same_platform(a: &Value, b: &Value) -> bool {
    ["platform", "cpu_features", "libm"].iter().all(|k| a["reproducibility"][k] == b["reproducibility"][k] && !a["reproducibility"][k].is_null())
}
fn assert_hash_on_same_platform(env: &Value, published: &Value, what: &str) {
    if same_platform(env, published) {
        assert_eq!(env["reproducibility"]["output_hash"], published["reproducibility"]["output_hash"], "{what}");
    } else {
        println!("{what}: bitwise comparison skipped (different platform / cpu_features / libm)");
    }
}
fn cmax(a: &Value, b: &Value) -> f64 {
    let (a, b) = (a.as_array().unwrap(), b.as_array().unwrap());
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x[0].as_f64().unwrap() - y[0].as_f64().unwrap()).hypot(x[1].as_f64().unwrap() - y[1].as_f64().unwrap())).fold(0.0, f64::max)
}

/// v0.4.7: request.json (50 Hz + 120 Hz, one-sided, 1 kHz) and request.complex.json (complex exponential,
/// ortho) against numpy.fft; request.error.json (complex input with sides = one) is an F-8 option conflict.
#[test]
fn fft_example() {
    let op = "signal.spectral.fft";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.complex.json", "response.complex.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert_eq!(env["status"], "ok", "{rq}: {}", env["error"]);
        assert_eq!(env["operator"]["build"].as_str().unwrap().split(' ').next(), Some("aeoi-numeric"), "build names the implementing crate");
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        assert!(cmax(&env["result"]["X"]["data"], &rf[rq]["X"]) <= 1e-12, "{rq}: X vs numpy");
        assert_eq!(env["result"]["frequency"]["value"]["data"].as_array().unwrap().len(), rf[rq]["frequency"].as_array().unwrap().len());
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
    assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
    assert!(!e["error"]["remediation"].as_array().unwrap().is_empty(), "remediation");
}

/// v0.4.8: request.json (damped oscillator, rtol 1e-8) agrees with scipy RK45 within T4; request.vdp.json
/// (Van der Pol μ = 1 at the default rtol 1e-3, like ode45's defaults) is flagged by the a-posteriori estimate:
/// tolerance_consistent fails and /y is declared unreliable (W.EVIDENCE.POSTCONDITION_FAILED); request.error.json
/// is an E.VALIDATION.EXPRESSION with the 0-based position of the syntax error (spec 3.15).
#[test]
fn ode_example() {
    let op = "ode.ivp.solve";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs, rtol, atol) in [("request.json", "response.success.json", 1e-8, 1e-6), ("request.vdp.json", "response.vdp.json", 1e-3, 1e-6)] {
        let req = read(&format!("{dir}/{rq}"));
        let env = call(op, &req);
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_eq!(env["operator"]["build"].as_str().unwrap().split(' ').next(), Some("aeoi-numeric"));
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let got: Vec<f64> = env["result"]["y"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let want: Vec<f64> = rf[rq]["y"].as_array().unwrap().iter().flat_map(|r| r.as_array().map(|a| a.clone()).unwrap_or_else(|| vec![r.clone()])).map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(got.len(), want.len(), "{rq}: y size");
        let rtol = req.get("rtol").and_then(|v| v.as_f64()).unwrap_or(rtol);
        let atol = req.get("atol").and_then(|v| v.as_f64()).unwrap_or(atol);
        let d = got.iter().zip(&want).map(|(a, b)| (a - b).abs() / (atol + rtol * b.abs())).fold(0.0, f64::max);
        assert!(d <= 10.0, "{rq}: vs scipy RK45 {d:.3e} × tol");
    }
    let vdp = call(op, &read(&format!("{dir}/request.vdp.json")));
    assert_eq!(vdp["trust"]["checks"]["tolerance_consistent"], false, "default tolerances on Van der Pol are flagged");
    assert!(vdp["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.EVIDENCE.POSTCONDITION_FAILED"));
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.EXPRESSION");
    assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
    assert_eq!(e["error"]["details"]["position"], 12);
    assert!(!e["error"]["remediation"].as_array().unwrap().is_empty(), "remediation");
}

/// v0.4.10: request.json (3×3, x = [1, −2, 3] exactly), request.lsq.json (straight-line least squares),
/// request.ill.json (Hilbert 13: W.NUMERICAL.ILL_CONDITIONED, rcond < ε), request.error.json (2×3:
/// E.DOMAIN.UNDERDETERMINED with an ask_user remedy).
#[test]
fn linsolve_example() {
    let op = "linalg.system.solve";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.lsq.json", "response.lsq.json"), ("request.ill.json", "response.ill.json")] {
        let req = read(&format!("{dir}/{rq}"));
        let env = call(op, &req);
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_eq!(env["operator"]["build"].as_str().unwrap().split(' ').next(), Some("aeoi-numeric"));
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        if rq == "request.ill.json" {
            assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED"));
            continue;
        }
        let got: Vec<f64> = env["result"]["x"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let want: Vec<f64> = rf[rq]["x_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-14 * w.abs().max(1.0), "{rq}: {g} vs {w}");
        }
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.DOMAIN.UNDERDETERMINED");
    assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
    assert_eq!(e["error"]["remediation"][0]["kind"], "ask_user");
}

/// v0.4.10: the same step-like data through pchip (request.json: stays within each interval, monotone) and
/// spline (request.spline.json: overshoots below 0 and above 4); request.error.json has a query outside the
/// data range → E.DOMAIN.EXTRAPOLATION_REQUIRED with three patches, each needing the user's confirmation.
#[test]
fn interp1_example() {
    let op = "interp.grid.interp1";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.spline.json", "response.spline.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert_eq!(env["status"], "ok", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let got: Vec<f64> = env["result"]["yq"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let want: Vec<f64> = rf[rq]["yq"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-12 * 4.0, "{rq}: {g} vs {w}");
        }
        let (lo, hi) = got.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        if rq == "request.json" {
            assert!(lo >= 0.0 && hi <= 4.0, "pchip stays in the data range");
        } else {
            assert!(lo < 0.0 && hi > 4.0, "spline overshoots on step data");
        }
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.DOMAIN.EXTRAPOLATION_REQUIRED");
    let rem = e["error"]["remediation"].as_array().unwrap();
    assert_eq!(rem.len(), 3);
    assert!(rem.iter().all(|r| r["requires_user_confirmation"] == true));
}
