//! Tool bridge: AEOI operators ⇄ LLM function declarations, executed in-process.
//!
//! In-process (aeoi-control registry) rather than aeoi-mcp over stdio: it is the very same
//! `DynOperator::call` pipeline the MCP server wraps (schema validation → validate → execute →
//! envelope), so the envelope is byte-identical, but there is no child process to spawn/supervise
//! (simpler on Windows), no JSON-RPC framing, and latency is the operator's own.
use crate::schema_conv::{llm_surface, to_gemini, Loss};
use aeoi_core::{DynOperator, HostCtx};
use serde_json::{json, Map, Value};
use std::time::Instant;

static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub struct ToolDef {
    pub name: String,
    pub operator_id: String,
    pub description: String,
    /// Provider-independent JSON Schema (OpenAI-compatible providers use this as-is).
    pub json_schema: Value,
    /// Gemini OpenAPI-subset schema.
    pub gemini_schema: Value,
    pub losses: Vec<Loss>,
}

pub struct Bridge {
    reg: Vec<Box<dyn DynOperator>>,
    pub tools: Vec<ToolDef>,
    host: HostCtx,
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
    d
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    pub fn new() -> Bridge {
        let reg = aeoi_control::registry();
        let tools = reg
            .iter()
            .map(|op| {
                let js = llm_surface(&op.meta().input_schema);
                let mut losses = vec![];
                let g = to_gemini(&js, &mut losses);
                ToolDef {
                    name: op.meta().manifest["mcp"]["tool_name"].as_str().unwrap().to_string(),
                    operator_id: op.meta().id.clone(),
                    description: description_for(op.as_ref()),
                    json_schema: js,
                    gemini_schema: g,
                    losses,
                }
            })
            .collect();
        Bridge { reg, tools, host: HostCtx::default() }
    }

    pub fn tool(&self, name: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.name == name || t.operator_id == name)
    }

    /// Run one tool call; always returns an AEOI envelope (bridge-level failures are wrapped in the
    /// same error shape so the model sees a uniform `isError` + remediation contract).
    pub fn call(&self, name: &str, args: &Value) -> (Value, f64) {
        let t0 = Instant::now();
        let env = match aeoi_control::find(&self.reg, name) {
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
                        e.entry("request_id").or_insert_with(|| json!(format!("web-{}", CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1)));
                    }
                    op.call(&a, &self.host)
                }
            }
        };
        (env, t0.elapsed().as_secs_f64() * 1e3)
    }
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
    fn walk(v: &Value, max_len: usize) -> Value {
        match v {
            Value::Object(o) => {
                if let (Some(Value::Array(data)), Some(shape)) = (o.get("data"), o.get("shape")) {
                    if data.len() > max_len && data.iter().all(|x| x.is_number()) {
                        let nums: Vec<f64> = data.iter().filter_map(|x| x.as_f64()).collect();
                        let step = (nums.len() - 1) as f64 / (max_len - 1) as f64;
                        let idx: Vec<usize> = (0..max_len).map(|i| (i as f64 * step).round() as usize).collect();
                        let mut m = Map::new();
                        for (k, x) in o {
                            if k != "data" {
                                m.insert(k.clone(), x.clone());
                            }
                        }
                        m.insert("_downsampled".into(), json!({"original_shape": shape, "kept_flat_indices": idx, "min": nums.iter().cloned().fold(f64::INFINITY, f64::min), "max": nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "note": "数组过长，仅给出均匀抽样点；需要精确点值时请用显式频点/时间网格重新调用"}));
                        m.insert("data".into(), Value::Array(idx.iter().map(|&i| json!(nums[i])).collect()));
                        return Value::Object(m);
                    }
                }
                Value::Object(o.iter().filter(|(k, _)| k.as_str() != "provenance").map(|(k, x)| (k.clone(), walk(x, max_len))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| walk(x, max_len)).collect()),
            _ => v.clone(),
        }
    }
    walk(env, max_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_tools_with_names_valid_for_all_providers() {
        let b = Bridge::new();
        assert_eq!(b.tools.len(), 12);
        for t in &b.tools {
            assert!(t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{}", t.name);
            assert!(t.name.len() <= 64);
            assert!(t.description.len() > 20);
        }
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
}
