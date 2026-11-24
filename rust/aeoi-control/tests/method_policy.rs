//! Spec v0.4 §3.14 (MS-1 .. MS-11) against the reference operators. Every reference operator registers exactly
//! one method, so these tests cover the selection contract: policies that the default method satisfies pin it
//! and are echoed in `receipt.method_selection`; policies it cannot satisfy are `E.VALIDATION.METHOD_UNAVAILABLE`
//! with no silent fallback; the policy never changes the request fingerprint or the result; replay pins `used`.
mod common;
use aeoi_core::{receipt, schema, DynOperator, HostCtx};
use common::*;
use serde_json::{json, Value};

const OP: &str = "control.lti.pade";
const METHOD: &str = "aeoi:pade:golub-van-loan-11.3.1";

fn read(p: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(spec_root().join(p)).unwrap()).unwrap()
}
fn request(policy: Option<Value>) -> Value {
    let mut r = read(&format!("examples/{OP}/request.json"));
    if let Some(p) = policy {
        r["exec"]["method_policy"] = p;
    }
    r
}
fn call(req: &Value) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, OP).unwrap();
    let env = o.call(req, &HostCtx::default());
    let v = schema::Validator::new(&schema::envelope_with_result(&o.meta().output_schema));
    let viol = v.violations(&env, 10);
    assert!(viol.is_empty(), "envelope schema violations {viol:?}");
    env
}
fn verify(req: &Value, claimed: &Value) -> Value {
    let reg = aeoi_control::registry();
    let op: &dyn DynOperator = reg.iter().find(|o| o.meta().id == OP).unwrap().as_ref();
    receipt::verify(op, req, claimed, &HostCtx::default())
}

#[test]
fn manifest_registers_one_method_and_declares_the_new_error_code() {
    let reg = aeoi_control::registry();
    for op in &reg {
        let m = &op.meta().manifest;
        let methods = aeoi_core::method::registered(m);
        assert_eq!(methods.len(), 1, "{}: reference operators have a single method", op.meta().id);
        assert_eq!(methods[0].id, m["receipt"]["method_id"].as_str().unwrap());
        assert!(
            m["errors"].as_array().unwrap().iter().any(|c| c == "E.VALIDATION.METHOD_UNAVAILABLE"),
            "{}: METHOD_UNAVAILABLE must be declared (every emitted code is declared)",
            op.meta().id
        );
    }
}

#[test]
fn without_a_policy_the_receipt_has_no_method_selection() {
    let env = call(&request(None));
    assert_eq!(env["status"], "ok");
    assert!(env["receipt"].get("method_selection").is_none(), "MS-7: absent unless the request carried a policy");
    assert_eq!(env["receipt"]["method"]["id"], METHOD);
}

#[test]
fn require_the_registered_method_is_echoed_and_changes_nothing_else() {
    let base = call(&request(None));
    let policy = json!({"require": [METHOD]});
    let env = call(&request(Some(policy.clone())));
    assert_eq!(env["status"], "ok", "{}", env["error"]);
    assert_eq!(env["receipt"]["method_selection"], json!({"policy": policy, "used": METHOD}));
    assert_eq!(env["receipt"]["method"]["id"], METHOD);
    // MS-5: the policy is not part of the request fingerprint; the same method gives the same answer.
    assert_eq!(env["receipt"]["fingerprint"]["request"], base["receipt"]["fingerprint"]["request"]);
    assert_eq!(env["reproducibility"]["input_hash"], base["reproducibility"]["input_hash"]);
    assert_eq!(env["reproducibility"]["output_hash"], base["reproducibility"]["output_hash"]);
    assert_eq!(env["receipt"]["fingerprint"]["result"], base["receipt"]["fingerprint"]["result"]);
}

#[test]
fn constraints_the_method_satisfies_select_it() {
    let policy = json!({"constraints": {"min_exactness": "approximate", "backend_library_in": ["aeoi-control (pure Rust) + nalgebra 0.33"]}});
    let env = call(&request(Some(policy.clone())));
    assert_eq!(env["status"], "ok", "{}", env["error"]);
    assert_eq!(env["receipt"]["method_selection"]["used"], METHOD);
    assert_eq!(env["receipt"]["method_selection"]["policy"], policy);
}

#[test]
fn unsatisfiable_policies_are_method_unavailable_never_a_silent_fallback() {
    for policy in [
        json!({"require": ["nope:pade:other"]}),
        json!({"constraints": {"min_exactness": "exact"}}),
        json!({"constraints": {"backend_library_in": ["some-other-library"]}}),
        json!({"require": [METHOD], "constraints": {"min_exactness": "correctly_rounded"}}),
    ] {
        let env = call(&request(Some(policy.clone())));
        assert_eq!(env["status"], "error", "{policy}");
        let e = &env["error"];
        assert_eq!(e["code"], "E.VALIDATION.METHOD_UNAVAILABLE", "{policy}");
        assert_eq!(e["category"], "validation");
        assert_eq!(e["retryable"], true);
        assert_eq!(e["details"]["policy"], policy);
        assert_eq!(e["details"]["allowed_methods"][0]["method_id"], METHOD);
        assert!(!e["details"]["allowed_methods"][0]["rejected_because"].as_array().unwrap().is_empty());
        let r = e["remediation"].as_array().unwrap();
        assert_eq!(r[0]["kind"], "modify_input");
        assert_eq!(r[0]["requires_user_confirmation"], true, "X-6: the policy may be deliberate");
        assert_eq!(r[0]["patch"], json!([{"op": "remove", "path": "/exec/method_policy"}]));
        assert!(r.iter().any(|x| x["kind"] == "ask_user"));
        assert!(env["result"].is_null() && env["receipt"].is_null());
    }
}

#[test]
fn dry_run_also_reports_an_unsatisfiable_policy() {
    let mut req = request(Some(json!({"require": ["nope:pade:other"]})));
    req["exec"]["dry_run"] = json!(true);
    assert_eq!(call(&req)["error"]["code"], "E.VALIDATION.METHOD_UNAVAILABLE");
}

#[test]
fn malformed_policies_are_schema_errors_not_ignored() {
    for policy in [json!({"prefer": [METHOD]}), json!({"require": []}), json!({"require": [3]}), json!({"constraints": {"min_exactness": "fast"}}), json!({"constraints": {"x": 1}})] {
        let env = call(&request(Some(policy.clone())));
        assert_eq!(env["error"]["code"], "E.VALIDATION.SCHEMA", "{policy}");
    }
}

#[test]
fn replay_pins_the_used_method_and_reports_method_unavailable_when_it_is_missing() {
    let req = request(Some(json!({"constraints": {"min_exactness": "approximate"}})));
    let env = call(&req);
    assert_eq!(env["status"], "ok");
    // MS-9: replay uses require:[used], not the original policy; same process → bitwise.
    assert_eq!(verify(&req, &env)["verdict"], "bitwise_match");
    // a receipt from an implementation whose method is not registered here → method_unavailable
    let mut foreign = env.clone();
    foreign["receipt"]["method_selection"]["used"] = json!("other-impl:pade:something");
    let v = verify(&req, &foreign);
    assert_eq!(v["verdict"], "method_unavailable");
    assert_eq!(v["claimed_method"], "other-impl:pade:something");
    assert_eq!(v["rerun_error"]["code"], "E.VALIDATION.METHOD_UNAVAILABLE");
    // a receipt without method_selection replays exactly as in v0.3
    let plain = call(&request(None));
    assert_eq!(verify(&request(None), &plain)["verdict"], "bitwise_match");
}

#[test]
fn host_policy_is_intersected_with_the_request_policy() {
    // MS-10: the binding merges the host policy into exec.method_policy; the request cannot loosen it.
    let host = json!({"constraints": {"min_exactness": "exact"}});
    let merged = aeoi_core::method::merge_policies(Some(&host), Some(&json!({"constraints": {"min_exactness": "approximate"}}))).unwrap();
    let env = call(&request(Some(merged)));
    assert_eq!(env["error"]["code"], "E.VALIDATION.METHOD_UNAVAILABLE", "pade is approximate; the host requires exact");
    let merged = aeoi_core::method::merge_policies(Some(&json!({"require": [METHOD]})), None).unwrap();
    assert_eq!(call(&request(Some(merged)))["status"], "ok");
}
