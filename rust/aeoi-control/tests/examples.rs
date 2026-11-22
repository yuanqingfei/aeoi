//! Example requests (examples/<op>/request*.json) must reproduce the published responses and the
//! python-control values in computed_values.json within the operator's tolerance class (NOT MATLAB values).
//! Also: envelopes validate against envelope+output schema; transport encodings (json/b64/ref/sparse) give
//! identical results and identical input_hash; dry_run; output_encoding=b64.
mod common;
use aeoi_core::{schema, HostCtx};
use base64::Engine;
use common::*;
use serde_json::{json, Value};
use sha2::Digest;

fn read(p: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(spec_root().join(p)).unwrap()).unwrap()
}
fn call(op: &str, req: &Value, host: &HostCtx) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, op).unwrap();
    let env = o.call(req, host);
    let v = schema::Validator::new(&schema::envelope_with_result(&o.meta().output_schema));
    let viol = v.violations(&env, 10);
    assert!(viol.is_empty(), "{op}: envelope schema violations {viol:?}");
    env
}
fn maxabs(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

#[test]
fn bode_example() {
    let h = HostCtx::default();
    let env = call("control.lti.bode", &read("examples/control.lti.bode/request.json"), &h);
    let cv = read("computed_values.json");
    let mag = fvec(&env["result"]["magnitude"]["value"]["data"]);
    let ph = fvec(&env["result"]["phase"]["value"]["data"]);
    let em = maxabs(&mag, &fvec(&cv["bode"]["mag_db"]));
    let ep = ph.iter().zip(fvec(&cv["bode"]["phase_deg"])).map(|(a, b)| wrap180(a - b).abs()).fold(0.0, f64::max);
    println!("bode example vs python-control: mag {em:.3e} dB (tol 1e-9), phase {ep:.3e} deg (tol 1e-8)");
    assert!(em <= 1e-9 && ep <= 1e-8);
    let pub_ = read("examples/control.lti.bode/response.success.json");
    assert!(maxabs(&mag, &fvec(&pub_["result"]["magnitude"]["value"]["data"])) <= 1e-9);
    assert_eq!(env["reproducibility"]["output_hash"], pub_["reproducibility"]["output_hash"], "bitwise determinism on this platform");
    let e = call("control.lti.bode", &read("examples/control.lti.bode/request.error.json"), &h);
    assert_eq!(e["error"]["code"], read("examples/control.lti.bode/response.error.json")["error"]["code"]);
    let ns = call("control.lti.bode", &read("examples/control.lti.bode/request.ss_nonsymmetric.json"), &h);
    assert_eq!(ns["status"], "ok");
}

#[test]
fn step_example() {
    let h = HostCtx::default();
    let env = call("control.lti.step", &read("examples/control.lti.step/request.json"), &h);
    let cv = read("computed_values.json");
    let y = fvec(&env["result"]["y"]["value"]["data"]);
    let ey = maxabs(&y, &fvec(&cv["step"]["y"]));
    println!("step example vs python-control: y {ey:.3e} (tol 1e-10·max|y|)");
    assert!(ey <= 1e-10 * 1.3);
    // NB: computed_values.json step info came from python-control's *auto* grid (v0.1 example bug) and is not compared.
    let i = &env["result"]["info"];
    assert!((f(&i["overshoot"]["value"]) - 37.2318).abs() < 1e-3);
    let pub_ = read("examples/control.lti.step/response.success.json");
    assert!(maxabs(&y, &fvec(&pub_["result"]["y"]["value"]["data"])) <= 1e-12);
    let e = call("control.lti.step", &read("examples/control.lti.step/request.error.json"), &h);
    assert_eq!(e["error"]["code"], "E.RESOURCE.COMPUTE_LIMIT");
    assert_eq!(e["error"]["code"], read("examples/control.lti.step/response.error.json")["error"]["code"]);
    // remediation patch must make the request succeed
    let mut fixed = read("examples/control.lti.step/request.error.json");
    for p in e["error"]["remediation"][0]["patch"].as_array().unwrap() {
        *fixed.pointer_mut(p["path"].as_str().unwrap()).unwrap() = p["value"].clone();
    }
    assert_eq!(call("control.lti.step", &fixed, &h)["status"], "ok", "remediation patch did not fix the request");
}

#[test]
fn lqr_example() {
    let h = HostCtx::default();
    let env = call("control.design.lqr", &read("examples/control.design.lqr/request.json"), &h);
    let k = fvec(&env["result"]["K"]["data"]);
    let analytic = [1.0, 3f64.sqrt()];
    let e = maxabs(&k, &analytic) / fro(&analytic);
    println!("lqr example vs analytic K=[1,√3]: {e:.3e} (tol 1e-10)");
    assert!(e <= 1e-10);
    let cv = read("computed_values.json");
    let ek: Vec<f64> = cv["lqr"]["K"][0].as_array().unwrap().iter().map(f).collect();
    assert!(maxabs(&k, &ek) / fro(&ek) <= 1e-10);
    let err = call("control.design.lqr", &read("examples/control.design.lqr/request.error.json"), &h);
    assert_eq!(err["error"]["code"], "E.DOMAIN.NOT_STABILIZABLE");
    // every call_operator remediation must reference a registered operator
    let reg = aeoi_control::registry();
    for r in err["error"]["remediation"].as_array().unwrap() {
        if r["kind"] == "call_operator" {
            assert!(aeoi_control::find(&reg, r["operator"].as_str().unwrap()).is_some());
        }
    }
}

fn le_bytes(d: &[f64]) -> Vec<u8> {
    d.iter().flat_map(|x| x.to_le_bytes()).collect()
}

#[test]
fn transport_encodings_are_equivalent() {
    let dir = std::env::temp_dir().join(format!("aeoi-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let host = HostCtx { data_root: Some(dir.clone()), ..HostCtx::default() };
    let base = read("examples/control.design.lqr/request.json");
    let a = [0.0, 1.0, 0.0, 0.0];
    let r0 = call("control.design.lqr", &base, &host);

    let mut req = base.clone();
    let bytes = le_bytes(&a);
    req["A"] = json!({"dtype":"f64","shape":[2,2],"order":"C","byte_order":"little",
        "data_b64": base64::engine::general_purpose::STANDARD.encode(&bytes), "sha256": format!("sha256:{}", hex(&sha2::Sha256::digest(&bytes)))});
    let bb = le_bytes(&[0.0, 1.0]);
    std::fs::write(dir.join("B.bin"), &bb).unwrap();
    req["B"] = json!({"dtype":"f64","shape":[2,1],"order":"C","byte_order":"little","encoding":"raw","uri": format!("file://{}", dir.join("B.bin").display()),
        "sha256": format!("sha256:{}", hex(&sha2::Sha256::digest(&bb))), "byte_length": bb.len()});
    let rd = call("control.design.lqr", &req, &host);
    assert_eq!(rd["status"], "ok", "{}", rd["error"]);
    assert_eq!(r0["result"]["K"], rd["result"]["K"]);
    assert_eq!(r0["reproducibility"]["input_hash"], rd["reproducibility"]["input_hash"], "input_hash must be independent of dense encoding (json/b64/ref)");
    req["Q"] = json!({"format":"csr","dtype":"f64","shape":[2,2],"indptr":[0,1,2],"indices":[0,1],"data":[1.0,1.0]});
    let r1 = call("control.design.lqr", &req, &host);
    assert_eq!(r1["status"], "ok_with_warnings", "{}", r1["error"]);
    assert!(r1["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.PERFORMANCE.SPARSE_DENSIFIED"));
    assert_eq!(r0["result"]["K"], r1["result"]["K"]);
    // spec v0.2 §3.7: sparse inputs hash their canonical CSR form, so dense vs sparse hashes differ by design
    assert_ne!(r0["reproducibility"]["input_hash"], r1["reproducibility"]["input_hash"]);

    // digest mismatch / path escape / non-file scheme
    let mut bad = req.clone();
    bad["A"]["sha256"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert_eq!(call("control.design.lqr", &bad, &host)["error"]["code"], "E.VALIDATION.DIGEST_MISMATCH");
    let mut esc = req.clone();
    esc["B"]["uri"] = json!("file:///etc/passwd");
    assert_eq!(call("control.design.lqr", &esc, &host)["error"]["code"], "E.VALIDATION.REF_FORBIDDEN");
    let mut http = req.clone();
    http["B"]["uri"] = json!("https://example.com/B.bin");
    assert_eq!(call("control.design.lqr", &http, &host)["error"]["code"], "E.VALIDATION.REF_UNSUPPORTED");
    let mut sp = req.clone();
    sp["Q"]["indices"] = json!([1, 0]);
    sp["Q"]["indptr"] = json!([0, 2, 2]);
    sp["Q"]["data"] = json!([1.0, 1.0]);
    assert_eq!(call("control.design.lqr", &sp, &host)["error"]["code"], "E.VALIDATION.SPARSE_STRUCTURE");

    // dry_run: no computation, estimate present
    let mut dr = base.clone();
    dr["exec"]["dry_run"] = json!(true);
    let d = call("control.design.lqr", &dr, &host);
    assert_eq!(d["dry_run"], true);
    assert!(d["result"].is_null() && d["estimate"].is_object());

    // output_encoding b64 decodes to the same numbers
    let mut ob = base.clone();
    ob["exec"]["output_encoding"] = json!("b64");
    let o = call("control.design.lqr", &ob, &host);
    let raw = base64::engine::general_purpose::STANDARD.decode(o["result"]["K"]["data_b64"].as_str().unwrap()).unwrap();
    let k: Vec<f64> = raw.chunks(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(json!(k), r0["result"]["K"]["data"]);
    assert_eq!(o["reproducibility"]["output_hash"], r0["reproducibility"]["output_hash"], "output_hash must be encoding-independent");
    std::fs::remove_dir_all(&dir).ok();
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
