//! Result receipts (spec 4.3) over every golden request (python-control + mpmath stores) and the examples:
//! fingerprints re-derived from reproducibility hashes, E-11 claims consistent with manifest post-conditions,
//! replay-verify verdicts, and the negative paths (tampered result, changed input, simulated other platform).
//! Writes reports/receipts.json (claim counts per operator).
mod common;
use aeoi_core::receipt::{compare, request_fingerprint, result_fingerprint, verify};
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn golden_files() -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    for engine in ["python-control", "mpmath"] {
        for op in std::fs::read_dir(spec_root().join("golden").join(engine)).unwrap().flatten() {
            if !op.path().is_dir() {
                continue;
            }
            for ver in std::fs::read_dir(op.path()).unwrap().flatten() {
                for f in std::fs::read_dir(ver.path()).unwrap().flatten() {
                    v.push(f.path());
                }
            }
        }
    }
    v.sort();
    v
}

#[test]
fn receipts_on_all_golden_requests() {
    let reg = aeoi_control::registry();
    let h = HostCtx::default();
    let mut claims: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let (mut n_ok, mut n_err) = (0, 0);
    for p in golden_files() {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let op = aeoi_control::find(&reg, case["operator"].as_str().unwrap()).unwrap();
        let m = op.meta();
        let env = op.call(&case["request"], &h);
        if env["status"] == "error" {
            assert!(env["receipt"].is_null(), "{}: error envelope must carry receipt null", p.display());
            let v = verify(op, &case["request"], &env, &h);
            assert_eq!(v["verdict"], "error_reproduced", "{}", p.display());
            n_err += 1;
            continue;
        }
        let r = &env["receipt"];
        let rp = &env["reproducibility"];
        let req = request_fingerprint(&m.id, &m.version, rp["input_hash"].as_str().unwrap());
        assert_eq!(r["fingerprint"]["request"], json!(req));
        assert_eq!(r["fingerprint"]["result"], json!(result_fingerprint(&req, rp["output_hash"].as_str().unwrap())));
        // E-11: claim follows the manifest post-conditions; golden successes never fail one
        let post: Vec<&str> = m.manifest["receipt"]["postconditions"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        let all_true = post.iter().all(|k| env["trust"]["checks"][*k] == json!(true));
        let want = if post.is_empty() { "unchecked" } else if all_true { "verified" } else { "unverified" };
        assert_eq!(r["evidence"]["claim"], want, "{}", p.display());
        assert_ne!(want, "unverified", "{}: a golden success fails post-conditions {:?}", p.display(), r["evidence"]["failed"]);
        *claims.entry(m.id.clone()).or_default().entry(want.to_string()).or_default() += 1;
        // replay on the same build + platform must be bitwise (P-7)
        let v = verify(op, &case["request"], &env, &h);
        assert_eq!(v["verdict"], "bitwise_match", "{}: {v}", p.display());
        n_ok += 1;
    }
    println!("receipts: {n_ok} successes, {n_err} errors; claims {claims:?}");
    assert!(n_ok > 600 && n_err >= 30);
    let out = json!({"generated_by": "rust/aeoi-control/tests/receipt.rs", "golden_successes": n_ok, "golden_errors": n_err,
                     "claims_by_operator": claims, "replay_verdict": "bitwise_match on every golden success; error_reproduced on every error"});
    std::fs::write(spec_root().join("reports/receipts.json"), serde_json::to_string_pretty(&out).unwrap() + "\n").unwrap();
}

#[test]
fn verify_negative_paths_and_cross_platform() {
    let reg = aeoi_control::registry();
    let h = HostCtx::default();
    let root = spec_root().join("examples/control.design.lqr");
    let req: Value = serde_json::from_str(&std::fs::read_to_string(root.join("request.json")).unwrap()).unwrap();
    let op = aeoi_control::find(&reg, "control.design.lqr").unwrap();
    let env = op.call(&req, &h);
    // tampered result
    let mut bad = env.clone();
    bad["result"]["S"]["data"][0] = json!(f(&env["result"]["S"]["data"][0]) * (1.0 + 1e-9));
    assert_eq!(verify(op, &req, &bad, &h)["verdict"], "result_tampered");
    // changed input
    let mut req2 = req.clone();
    req2["R"]["data"][0] = json!(2.0);
    assert_eq!(verify(op, &req2, &env, &h)["verdict"], "request_mismatch");
    // a different platform with last-bit differences: tolerance comparison (T2, rtol 1e-10)
    let mut other = env.clone();
    let s0 = f(&env["result"]["S"]["data"][0]);
    other["result"]["S"]["data"][0] = json!(s0 * (1.0 + 4.0 * f64::EPSILON));
    other["receipt"]["replay"]["platform"] = json!("wasm32-unknown");
    other["receipt"]["fingerprint"]["output_hash"] = json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    other["receipt"]["fingerprint"]["result"] = json!("sha256:1111111111111111111111111111111111111111111111111111111111111111");
    let v = verify(op, &req, &other, &h);
    assert_eq!(v["verdict"], "tolerance_match", "{v}");
    assert!(v["max_rel_diff"].as_f64().unwrap() > 0.0);
    other["result"]["S"]["data"][0] = json!(s0 * (1.0 + 1e-6));
    assert_eq!(verify(op, &req, &other, &h)["verdict"], "mismatch");
    // bare receipt (no result) with a different result fingerprint → mismatch (nothing to compare)
    assert_eq!(verify(op, &req, &other["receipt"], &h)["verdict"], "mismatch");
    assert_eq!(verify(op, &req, &env["receipt"], &h)["verdict"], "bitwise_match");
    // b64 output encoding: receipt fingerprints are encoding-independent
    let mut req_b64 = req.clone();
    req_b64["exec"] = json!({"output_encoding": "b64"});
    let env_b64 = op.call(&req_b64, &h);
    assert_eq!(env_b64["receipt"]["fingerprint"]["result"], env["receipt"]["fingerprint"]["result"]);
    assert_eq!(verify(op, &req, &env_b64, &h)["verdict"], "bitwise_match");
}

#[test]
fn compare_rules() {
    let a = json!({"x": {"data": [1.0, 2.0]}, "tag": "a", "z": 0.0});
    assert!(compare(&a, &a, 0.0).0);
    assert!(!compare(&a, &json!({"x": {"data": [1.0, 2.0]}, "tag": "b", "z": 0.0}), 1.0).0, "strings must match exactly");
    assert!(!compare(&a, &json!({"x": {"data": [1.0]}, "tag": "a", "z": 0.0}), 1.0).0, "lengths must match");
    assert!(compare(&a, &json!({"x": {"data": [1.0, 2.0 + 1e-12]}, "tag": "a", "z": 1e-16}), 1e-10).0, "≈0 field judged against whole-result scale");
    assert!(!compare(&a, &json!({"x": {"data": [1.0, 2.1]}, "tag": "a", "z": 0.0}), 1e-10).0);
    assert!(!compare(&json!({"v": "NaN"}), &json!({"v": 1.0}), 1.0).0);
}
