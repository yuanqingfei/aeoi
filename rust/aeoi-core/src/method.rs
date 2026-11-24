//! Method registration and selection (spec 3.14, MS-1 .. MS-11).
//!
//! The manifest's `receipt` object describes the `default` method; further methods of the same operator
//! are listed in `receipt.alternatives[]`. A request may constrain the method with `exec.method_policy`
//! (`require`, `constraints`): every item is a hard constraint, an empty candidate set is
//! `E.VALIDATION.METHOD_UNAVAILABLE`, and nothing is ever silently replaced (MS-6, MS-8).
use crate::error::{ask_user, modify_input, OpError};
use serde_json::{json, Value};

pub const METHOD_UNAVAILABLE: &str = "E.VALIDATION.METHOD_UNAVAILABLE";

/// One registered method of an operator (the default one or an entry of `receipt.alternatives`).
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub id: String,
    /// Static receipt part: assumptions, exactness, postconditions, coverage, guards.
    pub decl: Value,
    /// `backend.routine` of this method.
    pub routine: Value,
    pub library: String,
    /// `exact` | `correctly_rounded` | `approximate`
    pub exactness: String,
    pub determinism: String,
    pub tolerance_class: String,
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// `exact` > `correctly_rounded` > `approximate` (MS-5).
fn exactness_rank(class: &str) -> u8 {
    match class {
        "exact" => 2,
        "correctly_rounded" => 1,
        _ => 0,
    }
}

/// All methods of the manifest, `default` first, then `alternatives[]` in manifest order (MS-1).
pub fn registered(manifest: &Value) -> Vec<Method> {
    let rc = &manifest["receipt"];
    let det = s(&manifest["determinism"]);
    let tol = s(&manifest["tolerance_class"]);
    let mut v = vec![Method {
        id: s(&rc["method_id"]),
        decl: rc.clone(),
        routine: manifest["backend"]["routine"].clone(),
        library: s(&manifest["backend"]["library"]),
        exactness: s(&rc["exactness"]["class"]),
        determinism: det.clone(),
        tolerance_class: tol.clone(),
    }];
    for a in rc["alternatives"].as_array().into_iter().flatten() {
        v.push(Method {
            id: s(&a["method_id"]),
            decl: a.clone(),
            routine: a["backend"]["routine"].clone(),
            library: s(&a["backend"]["library"]),
            exactness: s(&a["exactness"]["class"]),
            determinism: a.get("determinism").and_then(|x| x.as_str()).map(String::from).unwrap_or_else(|| det.clone()),
            tolerance_class: a.get("tolerance_class").and_then(|x| x.as_str()).map(String::from).unwrap_or_else(|| tol.clone()),
        });
    }
    v
}

/// Why `m` is not a candidate under `policy`; empty when it is one.
fn violations(m: &Method, policy: &Value) -> Vec<String> {
    let mut why = Vec::new();
    if let Some(req) = policy.get("require").and_then(|r| r.as_array()) {
        if !req.iter().any(|x| x.as_str() == Some(m.id.as_str())) {
            why.push("不在 require 中".to_string());
        }
    }
    if let Some(min) = policy["constraints"].get("min_exactness").and_then(|x| x.as_str()) {
        if exactness_rank(&m.exactness) < exactness_rank(min) {
            why.push(format!("精确性类 {} 低于 min_exactness = {min}", m.exactness));
        }
    }
    if let Some(libs) = policy["constraints"].get("backend_library_in").and_then(|x| x.as_array()) {
        if !libs.iter().any(|x| x.as_str() == Some(m.library.as_str())) {
            why.push(format!("backend.library = {:?} 不在 backend_library_in 中", m.library));
        }
    }
    why
}

/// MS-6: candidates = registered ∩ require ∩ constraints. The result is the `default` when it is a
/// candidate, otherwise the first candidate in manifest order; empty → `E.VALIDATION.METHOD_UNAVAILABLE`.
pub fn resolve(manifest: &Value, policy: Option<&Value>) -> Result<Method, OpError> {
    let all = registered(manifest);
    let policy = match policy {
        None => return Ok(all[0].clone()),
        Some(p) => p,
    };
    // `all` is in preference order (default first), so the first candidate is the MS-6 choice.
    if let Some(m) = all.iter().find(|m| violations(m, policy).is_empty()) {
        return Ok(m.clone());
    }
    let allowed: Vec<Value> = all
        .iter()
        .map(|m| json!({"method_id": m.id, "exactness": m.exactness, "backend_library": m.library, "rejected_because": violations(m, policy)}))
        .collect();
    let unknown: Vec<Value> = policy
        .get("require")
        .and_then(|r| r.as_array())
        .into_iter()
        .flatten()
        .filter(|id| !all.iter().any(|m| Some(m.id.as_str()) == id.as_str()))
        .cloned()
        .collect();
    let ids = all.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ");
    Err(OpError::new(
        METHOD_UNAVAILABLE,
        format!("exec.method_policy 的硬约束没有任何已登记的方法能满足；该算子允许的方法：{ids}。"),
        "method_policy 里的每一项都是硬约束，不会被静默放宽。去掉或放宽 method_policy，或向用户确认他要求的方法。",
    )
    .retryable(true)
    .detail("policy", policy.clone())
    .detail("allowed_methods", Value::Array(allowed))
    .detail("unregistered_required", Value::Array(unknown))
    .remedy(modify_input(
        "relax_method_policy",
        "去掉 exec.method_policy（改用算子的默认方法）；策略可能是用户有意设的，须先确认",
        "medium",
        true,
        json!([{"op": "remove", "path": "/exec/method_policy"}]),
    ))
    .remedy(ask_user("ask_method", "向用户确认允许使用哪些计算方法")))
}

/// MS-10: effective policy = host policy ∩ request policy. `require` intersects, `min_exactness` takes the
/// stricter value, `backend_library_in` intersects. The request can never loosen the host policy. Done by the
/// binding layer before the call (the operator only sees the merged `exec.method_policy`); an empty
/// `require` / `backend_library_in` array in the result means "no method qualifies" and is reported by
/// `resolve` as METHOD_UNAVAILABLE.
pub fn merge_policies(host: Option<&Value>, request: Option<&Value>) -> Option<Value> {
    let (h, r) = match (host, request) {
        (None, None) => return None,
        (Some(p), None) | (None, Some(p)) => return Some(p.clone()),
        (Some(h), Some(r)) => (h, r),
    };
    fn inter(a: Option<&Value>, b: Option<&Value>) -> Option<Value> {
        match (a.and_then(|x| x.as_array()), b.and_then(|x| x.as_array())) {
            (None, None) => None,
            (Some(x), None) | (None, Some(x)) => Some(Value::Array(x.clone())),
            (Some(x), Some(y)) => Some(Value::Array(x.iter().filter(|v| y.contains(v)).cloned().collect())),
        }
    }
    let mut out = serde_json::Map::new();
    if let Some(v) = inter(h.get("require"), r.get("require")) {
        out.insert("require".into(), v);
    }
    let mut cons = serde_json::Map::new();
    let (hc, rc) = (&h["constraints"], &r["constraints"]);
    match (hc.get("min_exactness").and_then(|x| x.as_str()), rc.get("min_exactness").and_then(|x| x.as_str())) {
        (Some(a), Some(b)) => {
            cons.insert("min_exactness".into(), json!(if exactness_rank(a) >= exactness_rank(b) { a } else { b }));
        }
        (Some(a), None) | (None, Some(a)) => {
            cons.insert("min_exactness".into(), json!(a));
        }
        (None, None) => {}
    }
    if let Some(v) = inter(hc.get("backend_library_in"), rc.get("backend_library_in")) {
        cons.insert("backend_library_in".into(), v);
    }
    if !cons.is_empty() {
        out.insert("constraints".into(), Value::Object(cons));
    }
    Some(Value::Object(out))
}

/// MS-9: the policy a replay uses — exactly the method that produced the claimed result.
pub fn replay_policy(used: &str) -> Value {
    json!({"require": [used]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        json!({
            "determinism": "platform_bitwise", "tolerance_class": "T2",
            "backend": {"library": "aeoi-control", "routine": "a::r"},
            "receipt": {
                "method_id": "aeoi:x:default", "exactness": {"class": "approximate", "bound_kind": "a_priori"},
                "assumptions": [], "postconditions": [],
                "alternatives": [
                    {"method_id": "ref:x:exact", "exactness": {"class": "exact", "bound_kind": "rigorous"}, "assumptions": [], "postconditions": [],
                     "backend": {"library": "gmp", "routine": "g::r"}, "algorithm": "rational", "determinism": "bitwise", "tolerance_class": "T0"},
                    {"method_id": "ref:x:blas", "exactness": {"class": "approximate", "bound_kind": "none"}, "assumptions": [], "postconditions": [],
                     "backend": {"library": "openblas", "routine": "b::r"}, "algorithm": "blas"}
                ]
            }
        })
    }
    fn used(p: Option<Value>) -> Result<String, String> {
        resolve(&manifest(), p.as_ref()).map(|m| m.id).map_err(|e| e.to_json()["code"].as_str().unwrap().to_string())
    }

    #[test]
    fn registration_order_and_inheritance() {
        let r = registered(&manifest());
        assert_eq!(r.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["aeoi:x:default", "ref:x:exact", "ref:x:blas"]);
        assert_eq!((r[1].determinism.as_str(), r[1].tolerance_class.as_str()), ("bitwise", "T0"));
        assert_eq!((r[2].determinism.as_str(), r[2].tolerance_class.as_str()), ("platform_bitwise", "T2"), "omitted → inherited");
        assert_eq!(r[1].routine, json!("g::r"));
    }

    #[test]
    fn no_policy_is_the_default_method() {
        assert_eq!(used(None).unwrap(), "aeoi:x:default");
        assert_eq!(used(Some(json!({}))).unwrap(), "aeoi:x:default");
    }

    #[test]
    fn require_picks_default_when_allowed_else_first_candidate_in_manifest_order() {
        assert_eq!(used(Some(json!({"require": ["ref:x:blas", "aeoi:x:default"]}))).unwrap(), "aeoi:x:default");
        assert_eq!(used(Some(json!({"require": ["ref:x:blas", "ref:x:exact"]}))).unwrap(), "ref:x:exact");
        assert_eq!(used(Some(json!({"require": ["ref:x:blas"]}))).unwrap(), "ref:x:blas");
    }

    #[test]
    fn constraints_are_hard() {
        assert_eq!(used(Some(json!({"constraints": {"min_exactness": "exact"}}))).unwrap(), "ref:x:exact");
        assert_eq!(used(Some(json!({"constraints": {"backend_library_in": ["openblas"]}}))).unwrap(), "ref:x:blas");
        assert_eq!(used(Some(json!({"constraints": {"min_exactness": "correctly_rounded"}}))).unwrap(), "ref:x:exact");
        assert_eq!(used(Some(json!({"require": ["aeoi:x:default"], "constraints": {"min_exactness": "exact"}}))), Err(METHOD_UNAVAILABLE.into()));
    }

    #[test]
    fn empty_candidate_set_is_method_unavailable_with_reasons_and_no_silent_fallback() {
        let e = resolve(&manifest(), Some(&json!({"require": ["nope:x:y"]}))).unwrap_err().to_json();
        assert_eq!(e["code"], METHOD_UNAVAILABLE);
        assert_eq!(e["details"]["unregistered_required"], json!(["nope:x:y"]));
        assert_eq!(e["details"]["allowed_methods"].as_array().unwrap().len(), 3);
        assert_eq!(e["details"]["allowed_methods"][0]["rejected_because"], json!(["不在 require 中"]));
        assert_eq!(e["remediation"][0]["kind"], "modify_input");
        assert_eq!(e["remediation"][0]["requires_user_confirmation"], true);
        assert_eq!(e["remediation"][0]["patch"], json!([{"op": "remove", "path": "/exec/method_policy"}]));
        assert_eq!(e["remediation"][1]["kind"], "ask_user");
    }

    #[test]
    fn single_method_operator_without_alternatives() {
        let mut m = manifest();
        m["receipt"].as_object_mut().unwrap().remove("alternatives");
        assert_eq!(registered(&m).len(), 1);
        assert_eq!(resolve(&m, Some(&json!({"require": ["aeoi:x:default"]}))).unwrap().id, "aeoi:x:default");
        assert!(resolve(&m, Some(&json!({"require": ["ref:x:exact"]}))).is_err());
    }

    #[test]
    fn merge_is_host_intersect_request_and_never_loosens() {
        let host = json!({"require": ["a", "b", "c"], "constraints": {"min_exactness": "correctly_rounded", "backend_library_in": ["x", "y"]}});
        let req = json!({"require": ["b", "c", "d"], "constraints": {"min_exactness": "approximate", "backend_library_in": ["y", "z"]}});
        assert_eq!(
            merge_policies(Some(&host), Some(&req)).unwrap(),
            json!({"require": ["b", "c"], "constraints": {"min_exactness": "correctly_rounded", "backend_library_in": ["y"]}})
        );
        let stricter = json!({"constraints": {"min_exactness": "exact"}});
        assert_eq!(merge_policies(Some(&host), Some(&stricter)).unwrap()["constraints"]["min_exactness"], "exact");
        assert_eq!(merge_policies(None, Some(&req)).unwrap(), req);
        assert_eq!(merge_policies(Some(&host), None).unwrap(), host);
        assert_eq!(merge_policies(None, None), None);
        // disjoint require → empty array → no candidate
        let m = merge_policies(Some(&json!({"require": ["a"]})), Some(&json!({"require": ["b"]}))).unwrap();
        assert_eq!(m, json!({"require": []}));
        assert!(resolve(&manifest(), Some(&m)).is_err());
    }

    #[test]
    fn replay_policy_pins_the_used_method() {
        assert_eq!(replay_policy("ref:x:exact"), json!({"require": ["ref:x:exact"]}));
    }
}
