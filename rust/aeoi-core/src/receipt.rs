//! Result receipts (spec 4.3, v0.3.3): what was computed, how, under which assumptions, with which
//! evidence, plus deterministic fingerprints and a replay-and-compare verifier.
//!
//! Fingerprints (all `sha256:` over RFC 8785 JCS):
//! - `request` = H({operator, version, input_hash}) — identical on every platform for the same request
//!   (input canonicalisation is exact: shortest round-trip numbers, digests for arrays, spec P-2).
//! - `result`  = H({request, output_hash}) — equal across runs exactly when the result is bitwise equal.
//!   With `determinism: platform_bitwise` that is guaranteed for the same build + platform (P-7) only;
//!   across platforms (e.g. native vs wasm) `verify` falls back to a tolerance-class comparison.
use crate::error::Warning;
use crate::host::HostCtx;
use crate::jcs::hash_value;
use crate::operator::{DynOperator, OperatorMeta};
use serde_json::{json, Map, Value};

pub const RECEIPT_FORMAT: &str = "aeoi-receipt/1";
pub const W_POSTCONDITION: &str = "W.EVIDENCE.POSTCONDITION_FAILED";

/// E-11 + E-12: the evidence claim from (no post-conditions declared, all true, any sampled coverage).
pub fn claim_of(none_declared: bool, all_true: bool, any_sampled: bool) -> &'static str {
    if none_declared {
        "unchecked"
    } else if !all_true {
        "unverified"
    } else if any_sampled {
        "spot_checked"
    } else {
        "verified"
    }
}

pub fn request_fingerprint(id: &str, version: &str, input_hash: &str) -> String {
    hash_value(&json!({"operator": id, "version": version, "input_hash": input_hash}))
}

pub fn result_fingerprint(request_fp: &str, output_hash: &str) -> String {
    hash_value(&json!({"request": request_fp, "output_hash": output_hash}))
}

/// Default relative tolerance of a tolerance class (spec 7.2); None = no numeric criterion (T6).
pub fn class_rtol(class: &str) -> Option<f64> {
    match class {
        "T0" => Some(0.0),
        "T1" => Some(1e-12),
        "T2" => Some(1e-10),
        "T3" => Some(1e-9),
        "T4" | "T5" => Some(1e-6),
        _ => None,
    }
}

/// Exactness block: class from the manifest; bound = numeric fields of trust.error_estimate.
fn exactness(decl: &Value, trust: &Value) -> Value {
    let est = &trust["error_estimate"];
    let mut bound = Map::new();
    if let Some(o) = est.as_object() {
        for (k, v) in o {
            if v.is_number() && !k.starts_with("info_") && !k.starts_with("worst_") {
                bound.insert(k.clone(), v.clone());
            }
        }
    }
    let kind = decl["bound_kind"].as_str().unwrap_or("none");
    let kind = if bound.is_empty() && kind != "none" { "none_reported" } else { kind };
    json!({
        "class": decl["class"].as_str().unwrap_or("approximate"),
        "bound_kind": kind,
        "bound": if bound.is_empty() { Value::Null } else { Value::Object(bound) },
        "basis": est.get("basis").cloned().unwrap_or(Value::Null),
    })
}

/// Build the receipt of a successful call. Applies the "no evidence, no success claim" rule (E-11):
/// a declared post-condition that is false (or missing from trust.checks) adds W.EVIDENCE.POSTCONDITION_FAILED.
pub fn build(m: &OperatorMeta, input_hash: &str, output_hash: &str, trust: &Value, warnings: &mut Vec<Warning>,
             build: &str, platform: &str) -> Value {
    let method = crate::method::resolve(&m.manifest, None).expect("default method");
    build_for(m, &method, None, input_hash, output_hash, trust, warnings, build, platform)
}

/// `build` for the method actually used (spec 3.14): its declaration, routine, determinism and tolerance class.
/// `selection` (MS-7) is `{policy, used}` and is emitted only when the request carried `exec.method_policy`.
#[allow(clippy::too_many_arguments)]
pub fn build_for(m: &OperatorMeta, method: &crate::method::Method, selection: Option<Value>, input_hash: &str, output_hash: &str,
                 trust: &Value, warnings: &mut Vec<Warning>, build: &str, platform: &str) -> Value {
    let decl = &method.decl;
    let checks = &trust["checks"];
    let mut post = Map::new();
    let mut failed = Vec::new();
    for k in decl["postconditions"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
        let v = checks.get(k).and_then(|x| x.as_bool());
        post.insert(k.into(), v.map(Value::from).unwrap_or(Value::Null));
        if v != Some(true) {
            failed.push(k.to_string());
        }
    }
    // E-12 (v0.3.6): a post-condition evaluated only on a sampled/compressed copy of the result is
    // not evidence for the whole result: all-true with any "sampled" coverage → spot_checked.
    let coverage = decl.get("coverage").and_then(|c| c.as_object()).filter(|c| !c.is_empty());
    let sampled = coverage.is_some_and(|c| post.keys().any(|k| c.get(k).and_then(|v| v.as_str()) == Some("sampled")));
    let claim = claim_of(post.is_empty(), failed.is_empty(), sampled);
    // E-13 (v0.3.6): result fields guarded by a failed post-condition are listed as unreliable.
    let mut unreliable: Vec<String> = Vec::new();
    if let Some(g) = decl.get("guards").and_then(|g| g.as_object()) {
        for k in &failed {
            for p in g.get(k).and_then(|x| x.as_array()).into_iter().flatten().filter_map(|x| x.as_str()) {
                if !unreliable.iter().any(|u| u == p) {
                    unreliable.push(p.to_string());
                }
            }
        }
    }
    if !failed.is_empty() {
        let mut d = json!({"failed": failed, "postconditions": Value::Object(post.clone())});
        let mut msg = format!("后置条件未满足或未给出：{}；结果不能视为已验证", failed.join(", "));
        if !unreliable.is_empty() {
            msg.push_str(&format!("；不可依赖的结果字段：{}", unreliable.join(", ")));
            d["unreliable"] = json!(unreliable);
        }
        warnings.push(Warning::new(W_POSTCONDITION, msg).details(d));
    }
    let conv = &trust["convergence"];
    let req = request_fingerprint(&m.id, &m.version, input_hash);
    let res = result_fingerprint(&req, output_hash);
    let mut evidence = json!({
        "claim": claim,
        "postconditions": Value::Object(post),
        "failed": failed,
        "convergence": if conv["applicable"] == Value::Bool(true) { json!({"converged": conv["converged"].clone(), "iterations": conv["iterations"].clone()}) } else { Value::Null },
        "accuracy_grade": trust["accuracy_grade"].clone(),
        "tolerance_class": method.tolerance_class,
        "credentials": trust["credentials"].clone(),
    });
    // spec 4.3 / A-1: only approximation operators set trust.approximation. Omitting the key
    // keeps every existing operator's receipt byte-for-byte unchanged.
    if let Some(a) = trust.get("approximation").filter(|v| !v.is_null()) {
        evidence["approximation"] = a.clone();
    }
    // E-12/E-13: emitted only when the manifest declares non-full coverage / when a guarded check fails,
    // so receipts of operators without such declarations stay byte-for-byte unchanged.
    if let Some(c) = coverage {
        let mut cov = Map::new();
        for k in evidence["postconditions"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
            cov.insert(k.clone(), c.get(&k).cloned().unwrap_or_else(|| json!("full")));
        }
        evidence["coverage"] = Value::Object(cov);
    }
    if !unreliable.is_empty() {
        evidence["unreliable"] = json!(unreliable);
    }
    let mut receipt = json!({
        "format": RECEIPT_FORMAT,
        "method": {
            "id": decl["method_id"].clone(),
            "routine": method.routine.clone(),
            "provenance": format!("aeoi://operators/{}/{}/manifest#/provenance", m.id, m.version),
        },
        "assumptions": decl["assumptions"].clone(),
        "exactness": exactness(&decl["exactness"], trust),
        "evidence": evidence,
        "fingerprint": {
            "request": req,
            "result": res,
            "output_hash": output_hash,
            "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
            "stable_across": format!("request: all platforms and builds of {}@{}; result: same build + platform guaranteed ({}, P-7), other platforms only when the result is bitwise equal", m.id, m.version, method.determinism),
        },
        "replay": {
            "build": build,
            "platform": platform,
            "verify": {"mcp_tool": "aeoi_verify", "cli": "aeoi-call verify <operator> <request.json> <envelope.json>", "wasm": "verify(operator, request, envelope)"},
        },
    });
    if let Some(sel) = selection {
        receipt["method_selection"] = sel;
    }
    receipt
}

/// Normwise comparison of two results: per top-level field, ‖Δ‖∞ ≤ rtol·max(‖a‖∞, ‖b‖∞) (or ≤ 1e-14·‖whole‖∞
/// for fields that are ≈ 0). Strings, booleans, nulls, keys and array lengths must match exactly.
pub fn compare(a: &Value, b: &Value, rtol: f64) -> (bool, f64, Option<String>) {
    fn walk(a: &Value, b: &Value, path: &str, nums: &mut Vec<(f64, f64)>, bad: &mut Option<String>) {
        if bad.is_some() {
            return;
        }
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => nums.push((x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN))),
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (u, v)) in x.iter().zip(y).enumerate() {
                    walk(u, v, &format!("{path}/{i}"), nums, bad);
                }
            }
            (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
                for (k, u) in x {
                    match y.get(k) {
                        Some(v) => walk(u, v, &format!("{path}/{k}"), nums, bad),
                        None => *bad = Some(format!("{path}/{k}")),
                    }
                }
            }
            _ if a == b => {}
            _ => *bad = Some(path.to_string()),
        }
    }
    let inf = |v: &[(f64, f64)], f: fn(&(f64, f64)) -> f64| v.iter().map(f).filter(|x| x.is_finite()).fold(0.0f64, |m, x| m.max(x.abs()));
    let mut fields: Vec<(String, Vec<(f64, f64)>)> = Vec::new();
    let mut bad = None;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
            for (k, u) in x {
                let mut nums = Vec::new();
                match y.get(k) {
                    Some(v) => walk(u, v, &format!("/{k}"), &mut nums, &mut bad),
                    None => bad = Some(format!("/{k}")),
                }
                fields.push((k.clone(), nums));
            }
        }
        _ => {
            let mut nums = Vec::new();
            walk(a, b, "", &mut nums, &mut bad);
            fields.push((String::new(), nums));
        }
    }
    if let Some(p) = bad {
        return (false, f64::INFINITY, Some(p));
    }
    let all: Vec<(f64, f64)> = fields.iter().flat_map(|f| f.1.iter().copied()).collect();
    let whole = inf(&all, |p| p.0).max(inf(&all, |p| p.1));
    let mut max_rel = 0.0f64;
    let mut first = None;
    for (k, nums) in &fields {
        if nums.iter().any(|(x, y)| x.is_nan() != y.is_nan() || (x.is_infinite() && x != y)) {
            return (false, f64::INFINITY, Some(format!("/{k}")));
        }
        let d = nums.iter().filter(|(x, y)| x.is_finite() && y.is_finite()).fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        let s = inf(nums, |p| p.0).max(inf(nums, |p| p.1));
        let rel = if d == 0.0 { 0.0 } else if s > 0.0 { d / s } else { f64::INFINITY };
        max_rel = max_rel.max(rel);
        if d > rtol * s && d > 1e-14 * whole && first.is_none() {
            first = Some(format!("/{k}"));
        }
    }
    (first.is_none(), max_rel, first)
}

/// Re-run `args` on `op` and compare with a claimed envelope (or a bare receipt).
pub fn verify(op: &dyn DynOperator, args: &Value, claimed: &Value, host: &HostCtx) -> Value {
    let m = op.meta();
    let receipt = if claimed.get("receipt").is_some() { &claimed["receipt"] } else { claimed };
    let claimed_result = claimed.get("result").filter(|r| !r.is_null());
    // MS-9: a receipt that records a method selection is replayed with exactly the method that produced the
    // claimed result (`require: [used]`), not with the original policy (which may have come from the host).
    let used = receipt["method_selection"]["used"].as_str().filter(|_| claimed.get("status").and_then(|s| s.as_str()) != Some("error"));
    let replay_args;
    let args = match used {
        Some(u) => {
            let mut a = args.clone();
            if !a["exec"].is_object() {
                a["exec"] = json!({});
            }
            a["exec"]["method_policy"] = crate::method::replay_policy(u);
            replay_args = a;
            &replay_args
        }
        None => args,
    };
    let fresh = op.call(args, host);
    let fresh_status = fresh["status"].as_str().unwrap_or("").to_string();
    if used.is_some() && fresh["error"]["code"] == crate::method::METHOD_UNAVAILABLE {
        return json!({"verdict": "method_unavailable", "claimed_method": used, "rerun_error": fresh["error"].clone(),
            "note": "收据所用的方法在重放环境里没有登记：无法重放（MS-9）；两个不同方法之间的比较属于一致性判定，不是重放",
            "operator": {"id": m.id, "version": m.version}});
    }
    // error envelopes carry no receipt: compare the error code
    if claimed.get("status").and_then(|s| s.as_str()) == Some("error") || fresh_status == "error" {
        let same = claimed["status"] == fresh["status"] && claimed["error"]["code"] == fresh["error"]["code"];
        return json!({"verdict": if same {"error_reproduced"} else {"status_mismatch"},
            "claimed_status": claimed["status"].clone(), "rerun_status": fresh_status,
            "claimed_error": claimed["error"]["code"].clone(), "rerun_error": fresh["error"]["code"].clone(),
            "operator": {"id": m.id, "version": m.version}});
    }
    let fr = &fresh["receipt"];
    let claimed_req = receipt["fingerprint"]["request"].as_str().unwrap_or("");
    let fresh_req = fr["fingerprint"]["request"].as_str().unwrap_or("");
    let claimed_res = receipt["fingerprint"]["result"].as_str().unwrap_or("");
    let fresh_res = fr["fingerprint"]["result"].as_str().unwrap_or("");
    let same_build = receipt["replay"]["build"] == fr["replay"]["build"];
    let same_platform = receipt["replay"]["platform"] == fr["replay"]["platform"];
    let class = m.tolerance_class.clone();
    let rtol = class_rtol(&class);
    let mut out = json!({
        "operator": {"id": m.id, "version": m.version},
        "request_fingerprint": {"claimed": claimed_req, "recomputed": fresh_req, "match": claimed_req == fresh_req},
        "result_fingerprint": {"claimed": claimed_res, "recomputed": fresh_res, "match": claimed_res == fresh_res},
        "same_build": same_build, "same_platform": same_platform,
        "claimed_replay": receipt["replay"].clone(), "rerun_replay": fr["replay"].clone(),
        "rerun_status": fresh_status, "rerun_claim": fr["evidence"]["claim"].clone(),
        "tolerance": {"class": class, "rtol": rtol},
    });
    let verdict;
    if claimed_req != fresh_req {
        verdict = "request_mismatch";
        out["note"] = json!("请求或算子版本与收据不一致：收据不属于这组输入（或输入被改动）");
    } else if claimed_res == fresh_res {
        verdict = "bitwise_match";
        // the claimed result itself must hash to the claimed output_hash, otherwise the envelope was edited
        if let Some(r) = claimed_result {
            let oh = hash_value(&crate::matrix::canonicalize_for_hash(&crate::matrix::decode_b64_for_hash(r, host), host));
            if oh != receipt["fingerprint"]["output_hash"].as_str().unwrap_or("") {
                out["verdict"] = json!("result_tampered");
                out["note"] = json!("信封中的 result 与收据的 output_hash 不符：结果在签发后被改动");
                return out;
            }
        }
    } else if let (Some(r), Some(t)) = (claimed_result, rtol) {
        let (ok, rel, path) = compare(&crate::matrix::decode_b64_for_hash(r, host), &crate::matrix::decode_b64_for_hash(&fresh["result"], host), t);
        out["max_rel_diff"] = json!(rel);
        out["first_mismatch_path"] = json!(path);
        verdict = if ok { "tolerance_match" } else { "mismatch" };
        if same_build && same_platform {
            out["note"] = json!("同 build、同平台但结果不逐位相同：违反 P-7（或结果被改动）");
        }
    } else {
        verdict = "mismatch";
        out["note"] = json!("结果指纹不同，且没有可比较的 result（只给了收据）或该容差类没有数值判据");
    }
    out["verdict"] = json!(verdict);
    out
}

#[cfg(test)]
mod evidence_tests {
    //! E-12 / E-13 (v0.3.6) on a synthetic manifest: sampled coverage caps the claim at spot_checked,
    //! guarded fields of a failed post-condition are listed as unreliable.
    use super::*;
    fn meta(coverage: Value, guards: Value) -> OperatorMeta {
        let man = json!({"id": "test.op", "version": "0.0.1", "determinism": "platform_bitwise", "tolerance_class": "T2",
            "backend": {"routine": "r"},
            "receipt": {"method_id": "m", "assumptions": [], "exactness": {"class": "approximate", "bound_kind": "none"},
                        "postconditions": ["a", "b"], "coverage": coverage, "guards": guards}});
        OperatorMeta::from_json(&man.to_string(), r#"{"type":"object"}"#, r#"{"type":"object"}"#)
    }
    fn run(m: &OperatorMeta, a: bool, b: bool) -> (Value, Vec<Warning>) {
        let mut w = vec![];
        let trust = json!({"checks": {"a": a, "b": b}, "convergence": {"applicable": false}, "accuracy_grade": "B", "credentials": null, "error_estimate": null});
        (build(m, "sha256:00", "sha256:11", &trust, &mut w, "b", "p"), w)
    }
    #[test]
    fn claims() {
        assert_eq!(claim_of(true, true, false), "unchecked");
        assert_eq!(claim_of(false, false, true), "unverified");
        assert_eq!(claim_of(false, true, true), "spot_checked");
        assert_eq!(claim_of(false, true, false), "verified");
    }
    #[test]
    fn sampled_coverage_is_spot_checked_not_verified() {
        let m = meta(json!({"a": "sampled"}), json!({}));
        let (r, w) = run(&m, true, true);
        assert_eq!(r["evidence"]["claim"], "spot_checked");
        assert_eq!(r["evidence"]["coverage"], json!({"a": "sampled", "b": "full"}));
        assert!(w.is_empty());
        let m = meta(json!({"a": "test_points"}), json!({}));
        assert_eq!(run(&m, true, true).0["evidence"]["claim"], "verified");
    }
    #[test]
    fn failed_guard_lists_unreliable_fields() {
        let m = meta(json!({}), json!({"b": ["/rank", "/minimal"]}));
        let (r, w) = run(&m, true, false);
        assert_eq!(r["evidence"]["claim"], "unverified");
        assert_eq!(r["evidence"]["unreliable"], json!(["/rank", "/minimal"]));
        assert!(r["evidence"].get("coverage").is_none(), "empty coverage map → no coverage key");
        assert_eq!(w[0].to_json()["details"]["unreliable"], json!(["/rank", "/minimal"]));
        let (r, _) = run(&m, true, true);
        assert!(r["evidence"].get("unreliable").is_none());
    }
}
