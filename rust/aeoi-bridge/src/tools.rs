//! Tool bridge: AEOI operators ⇄ LLM function declarations, executed in-process.
//!
//! In-process (aeoi-control registry) rather than aeoi-mcp over stdio: it is the very same
//! `DynOperator::call` pipeline the MCP server wraps (schema validation → validate → execute →
//! envelope), so the envelope is byte-identical, but there is no child process to spawn/supervise
//! (simpler on Windows), no JSON-RPC framing, and latency is the operator's own.
use crate::schema_conv::{llm_surface, to_gemini, user_provenance_fields, Loss, USER_QUOTE};
use aeoi_core::{DynOperator, HostCtx};
use serde_json::{json, Map, Value};
use web_time::Instant;

static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone)]
pub struct ToolDef {
    pub name: String,
    pub operator_id: String,
    pub description: String,
    /// Provider-independent JSON Schema (OpenAI-compatible providers use this as-is).
    pub json_schema: Value,
    /// Gemini OpenAPI-subset schema.
    pub gemini_schema: Value,
    pub losses: Vec<Loss>,
    /// Inputs that must be grounded in the user's words (`x-aeoi-provenance: "user"`, spec F-5).
    pub user_fields: Vec<String>,
}

pub struct Bridge {
    reg: Vec<Box<dyn DynOperator>>,
    pub tools: Vec<ToolDef>,
    host: HostCtx,
    /// Host/deployment method policy (spec MS-10); merged into every call's `exec.method_policy`.
    host_method_policy: Option<Value>,
}

fn description_for(op: &dyn DynOperator) -> String {
    let m = &op.meta().manifest;
    let mut d = format!("[{}] {}", m["id"].as_str().unwrap_or(""), m["description"].as_str().unwrap_or(""));
    if let Some(pre) = m["applicability"]["preconditions"].as_array() {
        let p: Vec<&str> = pre.iter().filter_map(|x| x.as_str()).collect();
        if !p.is_empty() {
            d.push_str(" 前提：");
            d.push_str(&p.join("；"));
            d.push('。');
        }
    }
    if let Some(nf) = m["applicability"]["not_for"].as_array() {
        let p: Vec<&str> = nf.iter().filter_map(|x| x.as_str()).collect();
        if !p.is_empty() {
            d.push_str(" 不适用：");
            d.push_str(&p.join("；"));
            d.push('。');
        }
    }
    if m["applicability"]["siso_only"] == true {
        d.push_str(" 仅 SISO。");
    }
    // MS-11: tell the model which methods exist when there is a choice (single-method operators: unchanged text).
    let methods = aeoi_core::method::registered(m);
    if methods.len() > 1 {
        d.push_str(&format!(" 可用计算方法（缺省第一个）：{}。", methods.iter().map(|x| x.id.as_str()).collect::<Vec<_>>().join("、")));
    }
    d
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    pub fn new() -> Bridge {
        let reg = aeoi_ops::registry();
        let tools = reg
            .iter()
            .map(|op| {
                let js = llm_surface(&op.meta().input_schema);
                let mut losses = vec![];
                let g = to_gemini(&js, &mut losses);
                let user_fields = user_provenance_fields(&op.meta().input_schema);
                let mut description = description_for(op.as_ref());
                if !user_fields.is_empty() {
                    description.push_str(&format!(" 必须由用户给出、不得假设的参数：{}（用户没给时先询问，不要调用本工具）。", user_fields.join("、")));
                }
                ToolDef {
                    name: op.meta().manifest["mcp"]["tool_name"].as_str().unwrap().to_string(),
                    operator_id: op.meta().id.clone(),
                    description,
                    json_schema: js,
                    gemini_schema: g,
                    losses,
                    user_fields,
                }
            })
            .collect();
        Bridge { reg, tools, host: HostCtx::default(), host_method_policy: None }
    }

    /// Set the host policy (MS-10): e.g. `{"constraints": {"backend_library_in": ["audited-lib"]}}`. The model cannot
    /// loosen it: a request-supplied `method_policy` is intersected with it, never the other way round.
    pub fn with_method_policy(mut self, policy: Value) -> Bridge {
        self.host_method_policy = Some(policy);
        self
    }

    pub fn tool(&self, name: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.name == name || t.operator_id == name)
    }

    /// Run one tool call; always returns an AEOI envelope (bridge-level failures are wrapped in the
    /// same error shape so the model sees a uniform `isError` + remediation contract).
    /// Replay-verify an envelope's receipt (spec 4.3) for a tool call; `_user_quote` is stripped as in `call`.
    pub fn verify(&self, name_or_id: &str, args: &Value, envelope: &Value) -> Value {
        let mut args = args.clone();
        if let Some(o) = args.as_object_mut() {
            o.remove(USER_QUOTE);
        }
        let id = self.tool(name_or_id).map(|t| t.operator_id.clone()).unwrap_or_else(|| name_or_id.to_string());
        match self.reg.iter().find(|o| o.meta().id == id) {
            Some(op) => aeoi_core::receipt::verify(op.as_ref(), &args, envelope, &self.host),
            None => json!({"verdict": "unknown_operator", "operator": {"id": id}}),
        }
    }

    pub fn call(&self, name: &str, args: &Value) -> (Value, f64) {
        self.call_grounded(name, args, None)
    }

    /// Like [`Bridge::call`]; with `user_text` (everything the user said in this conversation) the
    /// F-5 grounding check runs first: each `x-aeoi-provenance: "user"` input must be quoted in
    /// `_user_quote`, the quote must occur in the user's text, and it must state the same number.
    /// Without `user_text` (direct UI calls) the quote is only stripped.
    pub fn call_grounded(&self, name: &str, args: &Value, user_text: Option<&str>) -> (Value, f64) {
        let t0 = Instant::now();
        let mut args = args.clone();
        let quotes = args.as_object_mut().and_then(|o| o.remove(USER_QUOTE));
        if let (Some(t), Some(text)) = (self.tool(name), user_text) {
            if let Some(err) = check_grounding(t, &args, quotes.as_ref(), text) {
                return (err, t0.elapsed().as_secs_f64() * 1e3);
            }
        }
        let args = &args;
        let env = match aeoi_ops::find(&self.reg, name) {
            None => bridge_error(
                "E.VALIDATION.UNKNOWN_TOOL",
                &format!("没有名为 {name} 的工具。"),
                &format!("从可用工具中选择：{}", self.tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")),
            ),
            Some(op) => {
                if !args.is_object() {
                    bridge_error("E.VALIDATION.SCHEMA", "工具参数必须是 JSON 对象。", "按函数声明的 parameters 传入对象。")
                } else {
                    let mut a = args.clone();
                    let exec = a.as_object_mut().unwrap().entry("exec").or_insert_with(|| json!({}));
                    if let Some(e) = exec.as_object_mut() {
                        if let Some(p) = aeoi_core::method::merge_policies(self.host_method_policy.as_ref(), e.get("method_policy")) {
                            e.insert("method_policy".into(), p);
                        }
                        e.entry("request_id").or_insert_with(|| json!(format!("web-{}", CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1)));
                    }
                    op.call(&a, &self.host)
                }
            }
        };
        (env, t0.elapsed().as_secs_f64() * 1e3)
    }
}

fn squash(s: &str) -> String {
    crate::answer::normalize(s).chars().filter(|c| !c.is_whitespace() && !"$*`\\{}".contains(*c)).collect::<String>().to_lowercase()
}

/// Spec F-5 grounding check (binding level). Returns an error envelope when a user-provenance
/// input is not backed by the user's own words.
pub fn check_grounding(t: &ToolDef, args: &Value, quotes: Option<&Value>, user_text: &str) -> Option<Value> {
    for f in &t.user_fields {
        let Some(v) = args.get(f.as_str()) else { continue }; // absent → operator reports the missing input
        let q = quotes.and_then(|q| q.get(f.as_str())).and_then(|x| x.as_str()).unwrap_or("").trim();
        let problem = if q.is_empty() {
            Some(format!("缺少 {USER_QUOTE}.{f}：没有说明 {f} = {v} 来自用户的哪句话"))
        } else if !squash(user_text).contains(&squash(q)) {
            Some(format!("{USER_QUOTE}.{f} 的引用“{q}”不在用户消息中"))
        } else if let Some(x) = v.as_f64() {
            // the quote must state this number (s, or ms/μs converted)
            let scale: &[f64] = if q.contains("ms") || q.contains("毫秒") { &[1e-3, 1.0] } else if q.contains("μs") || q.contains("us") || q.contains("微秒") { &[1e-6, 1.0] } else { &[1.0] };
            let ok = crate::answer::candidates(q).iter().any(|c| scale.iter().any(|s| (c.value * s - x).abs() <= 1e-9 * x.abs().max(1e-12) + c.half_ulp * s));
            if ok {
                None
            } else {
                Some(format!("{USER_QUOTE}.{f} 的引用“{q}”里没有数值 {x}"))
            }
        } else {
            None
        };
        if let Some(p) = problem {
            let e = aeoi_core::OpError::new(
                "E.VALIDATION.UNGROUNDED_INPUT",
                format!("{p}。{f} 是有物理含义、不能假设的输入（规范 F-5），必须由用户明确给出。"),
                format!("不要猜测 {f}。请向用户询问 {f} 的取值（含单位），得到回答后再调用；不要换一个“常用值”重试。"),
            )
            .detail("source", json!("aeoi-web bridge"))
            .detail("field", json!(f))
            .detail("value", v.clone())
            .detail("quote", json!(q))
            .path(&format!("/{f}"))
            .remedy(aeoi_core::error::ask_user("ask_user_for_value", &format!("向用户询问 {f}（含单位）")));
            return Some(json!({"spec": aeoi_core::SPEC, "status": "error", "result": null, "trust": null, "error": e.to_json()}));
        }
    }
    None
}

pub fn is_error(env: &Value) -> bool {
    env["status"] == "error" || env.get("error").map(|e| !e.is_null()).unwrap_or(false)
}

fn bridge_error(code: &str, msg: &str, fix: &str) -> Value {
    let e = aeoi_core::OpError::new(code, msg, fix).detail("source", json!("aeoi-web bridge"));
    json!({"spec": aeoi_core::SPEC, "status": "error", "result": null, "trust": null, "error": e.to_json()})
}

/// What the model gets back: the envelope with long numeric arrays down-sampled (the UI keeps the
/// full envelope). Arrays longer than `max_len` numbers are replaced by an evenly spaced subset plus
/// min/max, flagged with `_downsampled` so the model knows not to quote interpolated values.
pub fn compact_for_model(env: &Value, max_len: usize) -> Value {
    fn is_system(o: &Map<String, Value>) -> bool {
        matches!(o.get("kind").and_then(|k| k.as_str()), Some("tf" | "ss" | "zpk")) && (o.contains_key("num") || o.contains_key("A") || o.contains_key("z"))
    }
    fn walk(v: &Value, max_len: usize, n: &mut usize) -> Value {
        match v {
            Value::Object(o) => {
                // spec K-6 / F-6 (v0.3.6): an LtiSystem is a handle the model passes to the next call;
                // it is never downsampled (a thinned system could not be chained).
                if is_system(o) {
                    return v.clone();
                }
                if let (Some(Value::Array(data)), Some(shape)) = (o.get("data"), o.get("shape")) {
                    if data.len() > max_len && data.iter().all(|x| x.is_number()) {
                        *n += 1;
                        let nums: Vec<f64> = data.iter().filter_map(|x| x.as_f64()).collect();
                        let step = (nums.len() - 1) as f64 / (max_len - 1) as f64;
                        let idx: Vec<usize> = (0..max_len).map(|i| (i as f64 * step).round() as usize).collect();
                        let mut m = Map::new();
                        for (k, x) in o {
                            if k != "data" {
                                m.insert(k.clone(), x.clone());
                            }
                        }
                        m.insert("_downsampled".into(), json!({"original_shape": shape, "kept_flat_indices": idx, "min": nums.iter().cloned().fold(f64::INFINITY, f64::min), "max": nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "note": "数组过长，仅给出均匀抽样点（这是展示视图，不是证据，不能当作已核对的数值）；需要精确点值时请用显式频点/时间网格重新调用"}));
                        m.insert("data".into(), Value::Array(idx.iter().map(|&i| json!(nums[i])).collect()));
                        return Value::Object(m);
                    }
                }
                Value::Object(o.iter().filter(|(k, _)| k.as_str() != "provenance").map(|(k, x)| (k.clone(), walk(x, max_len, n))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| walk(x, max_len, n)).collect()),
            _ => v.clone(),
        }
    }
    let mut n = 0;
    let mut out = walk(env, max_len, &mut n);
    if let Some(r) = env.get("receipt").filter(|r| r.is_object()) {
        let mut short = receipt_for_model(r);
        if n > 0 {
            // spec E-13: the evidence is about the full result the operator computed, not this view
            short["result_view"] = json!({"downsampled_arrays": n, "note": "result 中带 _downsampled 的数组只是抽样视图：claim 针对算子算出的完整结果，抽样的数值不能当作已核对的证据引用"});
        }
        out["receipt"] = short;
    }
    out
}

/// The model sees a short form of the receipt (spec 4.3): the evidence claim, failed post-conditions,
/// exactness class, the assumptions the operator does NOT check (the model must make sure they hold or
/// say so), and the result fingerprint to cite. v0.3.6: `coverage` when a check is not over the whole
/// result (E-12) and `unreliable_fields` when a failed check guards result fields (E-13).
pub fn receipt_for_model(r: &Value) -> Value {
    let caller: Vec<Value> = r["assumptions"].as_array().into_iter().flatten()
        .filter(|a| a["enforced_by"] == "caller").map(|a| a["statement"].clone()).collect();
    let mut out = json!({
        "claim": r["evidence"]["claim"], "failed_postconditions": r["evidence"]["failed"],
        "exactness": {"class": r["exactness"]["class"], "bound_kind": r["exactness"]["bound_kind"]},
        "unchecked_assumptions": caller, "method": r["method"]["id"], "fingerprint": r["fingerprint"]["result"],
    });
    if let Some(c) = r["evidence"].get("coverage") {
        out["coverage"] = c.clone();
    }
    if let Some(u) = r["evidence"].get("unreliable") {
        out["unreliable_fields"] = u.clone();
        out["unreliable_note"] = json!("这些结果字段由失败的后置条件保护，不能作为结论引用或用于下一步");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tool_per_operator_with_names_valid_for_all_providers() {
        let b = Bridge::new();
        assert_eq!(b.tools.len(), aeoi_ops::registry().len());
        for t in &b.tools {
            assert!(t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{}", t.name);
            assert!(t.name.len() <= 64);
            assert!(t.description.len() > 20);
        }
    }

    #[test]
    fn receipt_short_form_and_verify() {
        let b = Bridge::new();
        let args = json!({"A":{"dtype":"f64","shape":[2,2],"data":[0,1,0,0]},"B":{"dtype":"f64","shape":[2,1],"data":[0,1]},"Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},"R":{"dtype":"f64","shape":[1,1],"data":[1]}});
        let (env, _) = b.call("control_design_lqr", &args);
        let short = compact_for_model(&env, 120);
        assert_eq!(short["receipt"]["claim"], "verified");
        assert!(short["receipt"]["unchecked_assumptions"].as_array().unwrap().len() >= 1);
        assert!(short["receipt"].get("assumptions").is_none(), "model gets the short form only");
        let mut with_quote = args.clone();
        with_quote["_user_quote"] = json!({});
        assert_eq!(b.verify("control_design_lqr", &with_quote, &env)["verdict"], "bitwise_match");
    }

    #[test]
    fn call_success_error_unknown() {
        let b = Bridge::new();
        let (ok, ms) = b.call("control_design_lqr", &json!({"A":{"dtype":"f64","shape":[2,2],"data":[0,1,0,0]},"B":{"dtype":"f64","shape":[2,1],"data":[0,1]},"Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},"R":{"dtype":"f64","shape":[1,1],"data":[1]}}));
        assert!(!is_error(&ok), "{ok}");
        assert!(ms >= 0.0);
        let k = ok["result"]["K"]["data"][1].as_f64().unwrap();
        assert!((k - 3f64.sqrt()).abs() < 1e-12);
        let (err, _) = b.call("control_design_lqr", &json!({"A":{"dtype":"f64","shape":[2,2],"data":[0,1,0]}}));
        assert!(is_error(&err));
        assert!(err["error"]["remediation"].is_array(), "{err}");
        let (unk, _) = b.call("nope", &json!({}));
        assert_eq!(unk["error"]["code"], "E.VALIDATION.UNKNOWN_TOOL");
    }

    #[test]
    fn compaction_downsamples_long_arrays() {
        let b = Bridge::new();
        let (env, _) = b.call("control_lti_impulse", &json!({"system":{"kind":"tf","num":[1],"den":[1,1],"ts":0},"time":{"mode":"uniform","t_final":10,"dt":0.01,"unit":"s"}}));
        assert!(!is_error(&env), "{env}");
        let c = compact_for_model(&env, 50);
        assert_eq!(c["result"]["y"]["value"]["data"].as_array().unwrap().len(), 50);
        assert!(c["result"]["y"]["value"]["_downsampled"].is_object());
        assert_eq!(env["result"]["y"]["value"]["data"].as_array().unwrap().len(), 1001);
    }

    #[test]
    fn k6_systems_are_never_downsampled_and_views_are_flagged() {
        let b = Bridge::new();
        // a 12-state series result: A has 144 entries > 120, but it is a handle for the next call
        let g = json!({"kind":"ss","A":{"dtype":"f64","shape":[6,6],"data":(0..36).map(|i| if i % 7 == 0 { -1.0 } else if i % 6 == 0 && i > 0 { 0.5 } else { 0.0 }).collect::<Vec<f64>>()},
            "B":{"dtype":"f64","shape":[6,1],"data":[1,0,0,0,0,1]},"C":{"dtype":"f64","shape":[1,6],"data":[1,0,0,0,0,1]},"D":{"dtype":"f64","shape":[1,1],"data":[0]},"ts":0});
        let (env, _) = b.call("control_lti_series", &json!({"sys1": g, "sys2": g}));
        assert!(!is_error(&env), "{env}");
        assert_eq!(env["result"]["system"]["A"]["data"].as_array().unwrap().len(), 144);
        let c = compact_for_model(&env, 120);
        assert_eq!(c["result"]["system"], env["result"]["system"], "LtiSystem must reach the model intact");
        assert!(c["receipt"].get("result_view").is_none());
        // a long response array is a view: flagged in the short receipt
        let (env, _) = b.call("control_lti_impulse", &json!({"system":{"kind":"tf","num":[1],"den":[1,1],"ts":0},"time":{"mode":"uniform","t_final":10,"dt":0.01,"unit":"s"}}));
        let c = compact_for_model(&env, 50);
        assert!(c["receipt"]["result_view"]["downsampled_arrays"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn e13_ctrb_obsv_on_xg02_lists_unreliable_fields() {
        // EngDesign XG_02 7th-order plant: Krylov rank (4) disagrees with PBH → unverified, and the
        // rank / full_rank / minimal fields are named as unusable in the short receipt.
        let a: Vec<f64> = vec![-3360.27884342382, 24650.4407238876, -24650.4407238876, 24650.4407238876, -24650.4407238876, 24650.4407238876, -6162.6101809719, -3270.26403683917, 10278.7248098086, -7476.39347226054, 7476.39347226054, -7476.39347226054, 7476.39347226054, -1869.09836806513, -2657.10970968037, 10628.4388387215, -13430.7701762696, 16233.1015138177, -16233.1015138177, 16233.1015138177, -4058.27537845441, -3954.03177459846, 15816.1270983939, -15816.1270983939, 13013.7957608458, -10211.4644232977, 10211.4644232977, -2552.86610582443, -2325.45756017668, 9301.83024070671, -9301.83024070671, 9301.83024070671, -12104.1615782548, 14906.4929158029, -3726.62322895072, -1665.81075466004, 6663.24301864017, -6663.24301864017, 6663.24301864017, -6663.24301864017, 3860.9116810921, -264.645085886005, -1596.51916299641, 6386.07665198562, -6386.07665198562, 6386.07665198562, -6386.07665198562, 6386.07665198562, -4398.85050054448];
        let sys = json!({"kind":"ss","A":{"dtype":"f64","shape":[7,7],"data":a},"B":{"dtype":"f64","shape":[7,1],"data":[33.1501, 29.2147, 49.743, 55.4071, 43.2351, 22.8726, 24.9153]},
            "C":{"dtype":"f64","shape":[1,7],"data":[41.1585, -164.6342, 164.6342, -164.6342, 164.6342, -164.6342, 41.1585]},"D":{"dtype":"f64","shape":[1,1],"data":[0]},"ts":0});
        let (env, _) = b_call("control_analysis_ctrb_obsv", json!({"system": sys}));
        assert_eq!(env["receipt"]["evidence"]["claim"], "unverified");
        let short = compact_for_model(&env, 120);
        let u: Vec<&str> = short["receipt"]["unreliable_fields"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert!(u.contains(&"/ctrb/rank") && u.contains(&"/minimal"), "{u:?}");
    }
    fn b_call(t: &str, a: Value) -> (Value, f64) {
        Bridge::new().call(t, &a)
    }

    #[test]
    fn k3_tool_descriptions_name_open_and_closed_loop() {
        let b = Bridge::new();
        let d = |t: &str| b.tool(t).unwrap().description.clone();
        assert!(d("control_lti_feedback").contains("closed_loop") && d("control_lti_feedback").contains("不要送给 margin"));
        assert!(d("control_lti_series").contains("open_loop"));
        assert!(d("control_analysis_loopsens").contains("gain_crossover") && d("control_analysis_loopsens").contains("Hz"));
        let m = &b.tool("control_analysis_margin").unwrap().json_schema["properties"]["system"]["description"];
        assert!(m.as_str().unwrap().contains("不要传闭环 T"));
        // K-6: every LtiSystem parameter tells the model to paste result.system verbatim
        let bode = &b.tool("control_lti_bode").unwrap().json_schema["properties"]["system"]["description"];
        assert!(bode.as_str().unwrap().contains("result.system"));
    }

    #[test]
    fn ungrounded_sample_time_is_rejected() {
        let b = Bridge::new();
        let t = b.tool("control_lti_c2d").unwrap();
        assert_eq!(t.user_fields, vec!["ts".to_string()]);
        assert!(t.json_schema["required"].as_array().unwrap().contains(&json!(USER_QUOTE)));
        assert!(t.description.contains("不得假设"));
        let sys = json!({"kind":"tf","num":[1],"den":[1,1],"ts":0});
        let q = "G(s)=1/(s+1)，用 ZOH 离散化";
        // invented ts, no quote
        let (e, _) = b.call_grounded("control_lti_c2d", &json!({"system": sys, "ts": 0.1}), Some(q));
        assert_eq!(e["error"]["code"], "E.VALIDATION.UNGROUNDED_INPUT", "{e}");
        assert_eq!(e["error"]["remediation"][0]["kind"], "ask_user");
        // quote not in the user's text
        let (e, _) = b.call_grounded("control_lti_c2d", &json!({"system": sys, "ts": 0.1, "_user_quote": {"ts": "T = 0.1 s"}}), Some(q));
        assert_eq!(e["error"]["code"], "E.VALIDATION.UNGROUNDED_INPUT");
        // quote present but a different number
        let q2 = "G(s)=1/(s+1)，采样周期 T = 0.5 s，用 ZOH 离散化";
        let (e, _) = b.call_grounded("control_lti_c2d", &json!({"system": sys, "ts": 0.1, "_user_quote": {"ts": "采样周期 T = 0.5 s"}}), Some(q2));
        assert_eq!(e["error"]["code"], "E.VALIDATION.UNGROUNDED_INPUT");
        // grounded (also with ms and extra spacing/LaTeX)
        let (ok, _) = b.call_grounded("control_lti_c2d", &json!({"system": sys, "ts": 0.5, "_user_quote": {"ts": "采样周期 T = 0.5 s"}}), Some(q2));
        assert!(!is_error(&ok), "{ok}");
        let (ok, _) = b.call_grounded("control_lti_c2d", &json!({"system": sys, "ts": 0.02, "_user_quote": {"ts": "T=20ms"}}), Some("请按 T = 20 ms 离散化 1/(s+1)"));
        assert!(!is_error(&ok), "{ok}");
        // without user text (direct UI call) the quote is just stripped
        let (ok, _) = b.call("control_lti_c2d", &json!({"system": sys, "ts": 0.1, "_user_quote": {"ts": "x"}}));
        assert!(!is_error(&ok), "{ok}");
    }
}
