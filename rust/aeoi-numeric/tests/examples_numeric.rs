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
