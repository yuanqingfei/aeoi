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

/// v0.4.11: polyfit — request.json (quadratic, std errors), request.ill.json (years, degree 4: warns with a
/// center_scale suggestion), request.error.json (two distinct x for degree 2 → UNDERDETERMINED).
#[test]
fn polyfit_example() {
    let op = "fit.poly.polyfit";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.ill.json", "response.ill.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let got: Vec<f64> = env["result"]["p"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let want: Vec<f64> = rf[rq]["p_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let tol = if rq == "request.ill.json" { 1e-9 } else { 1e-14 };
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= tol * w.abs().max(1e-300), "{rq}: {g} vs {w}");
        }
        if rq == "request.ill.json" {
            let w = env["warnings"].as_array().unwrap().iter().find(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED").unwrap();
            assert_eq!(w["details"]["suggested_remediation"]["requires_user_confirmation"], true);
            assert_eq!(w["details"]["suggested_remediation"]["patch"][0]["value"], "center_scale");
        }
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.DOMAIN.UNDERDETERMINED");
    assert_eq!(e["error"]["remediation"][0]["requires_user_confirmation"], true);
}

/// v0.4.11: eig — symmetric tridiagonal (request.json), companion matrix with a conjugate pair
/// (request.general.json), Jordan block (request.defective.json: warns, the three vectors coincide).
#[test]
fn eig_example() {
    let op = "linalg.decomp.eig";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.general.json", "response.general.json"), ("request.defective.json", "response.defective.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let got = &env["result"]["values"]["data"];
        let want = &rf[rq]["values_true"];
        for k in 0..want.as_array().unwrap().len() {
            let (gr, gi) = match &got[k] {
                Value::Array(p) => (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()),
                v => (v.as_f64().unwrap(), 0.0),
            };
            let (wr, wi) = (want[k][0].as_f64().unwrap(), want[k][1].as_f64().unwrap());
            assert!((gr - wr).hypot(gi - wi) <= 1e-13, "{rq}: λ_{k}");
        }
        assert_eq!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED"), rq == "request.defective.json", "{rq}");
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
}

/// v0.4.11: svd — thin (request.json) and full with a rank-deficient matrix (request.full.json: rank 2).
#[test]
fn svd_example() {
    let op = "linalg.decomp.svd";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.full.json", "response.full.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert_eq!(env["status"], "ok", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let got: Vec<f64> = env["result"]["s"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let want: Vec<f64> = rf[rq]["s_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() <= 1e-14 * want[0], "{rq}: {g} vs {w}");
        }
    }
    assert_eq!(call(op, &read(&format!("{dir}/request.full.json")))["result"]["rank"], 2);
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
}

/// v0.4.12: butter — digital low-pass from Hz + sample rate, analog low-pass (an LTI system), order 20 at
/// Wn = 0.01 (b/a useless: warns); request.error.json gives cutoff_hz without a sample rate.
#[test]
fn butter_example() {
    let op = "signal.design.butter";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.analog.json", "response.analog.json"), ("request.ill.json", "response.ill.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        if rq == "request.ill.json" {
            assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED"));
            continue;
        }
        for key in ["b", "a"] {
            let got: Vec<f64> = env["result"][key].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            let want: Vec<f64> = rf[rq][format!("{key}_true")].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            let m = want.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            for (g, w) in got.iter().zip(&want) {
                assert!((g - w).abs() <= 1e-13 * m, "{rq} {key}: {g} vs {w}");
            }
        }
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
}

/// v0.4.12: filter — a butter sos on a test signal, a first-order IIR impulse response (0.1·0.9^n), and an
/// unstable recursion (warns); request.error.json has a0 = 0.
#[test]
fn filter_example() {
    let op = "signal.filter.filter";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.ba.json", "response.ba.json"), ("request.unstable.json", "response.unstable.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        assert_eq!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.DOMAIN.UNSTABLE_SYSTEM"), rq == "request.unstable.json");
    }
    let env = call(op, &read(&format!("{dir}/request.ba.json")));
    let got: Vec<f64> = env["result"]["y"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let want: Vec<f64> = rf["request.ba.json"]["y_true"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() <= 1e-16, "{g} vs {w}");
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.VALIDATION.ZERO_DENOMINATOR");
}

/// v0.4.13: fzero — Kepler's equation on a bracket, cos(x) = x from x0 alone (bracket searched: warns), tan(x)
/// on [1, 2] (the sign change is a pole: post-condition not_a_pole fails); no sign change → error.
#[test]
fn fzero_example() {
    let op = "nonlinear.root.fzero";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.x0.json", "response.x0.json"), ("request.pole.json", "response.pole.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        match rq {
            "request.json" => assert!((env["result"]["x"].as_f64().unwrap() - rf[rq]["x_true"].as_f64().unwrap()).abs() <= 4.0 * f64::EPSILON),
            "request.x0.json" => assert_eq!(env["warnings"][0]["code"], "W.DOMAIN.BRACKET_SEARCHED"),
            _ => assert_eq!(env["trust"]["checks"]["not_a_pole"], false),
        }
    }
    assert_eq!(call(op, &read(&format!("{dir}/request.error.json")))["error"]["code"], "E.DOMAIN.NO_SIGN_CHANGE");
}

/// v0.4.13: fsolve — predator–prey equilibrium (exact (3, 2)), a double root (warns), x² + 1 = 0 (no real
/// root: LOCAL_MINIMUM with the best point).
#[test]
fn fsolve_example() {
    let op = "nonlinear.system.fsolve";
    let dir = format!("examples/{op}");
    for (rq, rs) in [("request.json", "response.success.json"), ("request.double_root.json", "response.double_root.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")));
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
    }
    let env = call(op, &read(&format!("{dir}/request.json")));
    assert!((env["result"]["x"][0].as_f64().unwrap() - 3.0).abs() <= 1e-15 && (env["result"]["x"][1].as_f64().unwrap() - 2.0).abs() <= 1e-15);
    let e = call(op, &read(&format!("{dir}/request.error.json")));
    assert_eq!(e["error"]["code"], "E.NUMERICAL.LOCAL_MINIMUM");
    assert!(e["error"]["details"]["best"]["x"].is_array());
}

/// v0.4.14: linprog (vertex (1.6, 1.2) with multipliers; no default bounds → UNBOUNDED), fmincon (HS071,
/// f = 17.014017…; infeasible disk → NO_CONVERGENCE with the best point), fminsearch (non-smooth |x − 1| + 2|y + 2|;
/// iteration limit → NO_CONVERGENCE).
#[test]
fn optim_examples() {
    for (op, code) in [("optim.linear.linprog", "E.DOMAIN.UNBOUNDED"), ("optim.local.fmincon", "E.NUMERICAL.NO_CONVERGENCE"), ("optim.local.fminsearch", "E.NUMERICAL.NO_CONVERGENCE")] {
        let dir = format!("examples/{op}");
        let env = call(op, &read(&format!("{dir}/request.json")));
        assert_eq!(env["status"], "ok", "{op}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/response.success.json")), &format!("{op}: bitwise example"));
        assert_eq!(call(op, &read(&format!("{dir}/request.error.json")))["error"]["code"], code, "{op}");
    }
    let f = call("optim.local.fmincon", &read("examples/optim.local.fmincon/request.json"))["result"]["fval"].as_f64().unwrap();
    assert!((f - 17.0140172891563).abs() < 1e-9, "{f}");
}
