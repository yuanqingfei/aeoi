//! AEOI in the browser: the 12 control operators and the LLM tool bridge, exported through
//! wasm-bindgen with a JSON-string API (the JS side owns `fetch`, the agent loop and the UI).
//!
//! Everything a tool call goes through is the native code path: `aeoi_bridge::tools::Bridge::
//! call_grounded` → F-5 grounding check → `DynOperator::call` (schema validation, validate,
//! execute, envelope). Only the transport differs from `aeoi-web`.
use aeoi_bridge::llm::{self, ChatResponse, Msg};
use aeoi_bridge::tools::{compact_for_model, is_error, Bridge};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

thread_local! {
    static BRIDGE: Bridge = Bridge::new();
}

fn parse(s: &str, what: &str) -> Result<Value, JsError> {
    serde_json::from_str(s).map_err(|e| JsError::new(&format!("{what}: invalid JSON: {e}")))
}

fn history(s: &str) -> Result<Vec<Msg>, JsError> {
    serde_json::from_str(s).map_err(|e| JsError::new(&format!("history: {e}")))
}

/// `{spec, version, platform, tools: n}`.
#[wasm_bindgen]
pub fn info() -> String {
    BRIDGE.with(|b| json!({"spec": aeoi_core::SPEC, "version": env!("CARGO_PKG_VERSION"), "platform": aeoi_core::operator::platform(), "tools": b.tools.len()}).to_string())
}

#[wasm_bindgen]
pub fn system_prompt() -> String {
    aeoi_bridge::SYSTEM_PROMPT.to_string()
}

/// Tool declarations, same shape as `aeoi-web`'s `GET /api/tools`.
#[wasm_bindgen]
pub fn tools() -> String {
    BRIDGE.with(|b| {
        Value::Array(
            b.tools
                .iter()
                .map(|t| json!({"name": t.name, "operator_id": t.operator_id, "description": t.description, "json_schema": t.json_schema, "gemini_schema": t.gemini_schema, "lossy_mappings": t.losses.iter().map(|l| json!({"path": l.path, "what": l.what})).collect::<Vec<_>>()}))
                .collect(),
        )
        .to_string()
    })
}

/// Provider request body for `history` (neutral messages: `{role:"user",text}`,
/// `{role:"assistant",text,calls,raw}`, `{role:"tool",call_id,name,response}`).
/// `provider` = `gemini` or any OpenAI-compatible name.
#[wasm_bindgen]
pub fn build_request(provider: &str, model: &str, history_json: &str) -> Result<String, JsError> {
    let h = history(history_json)?;
    let sys = aeoi_bridge::SYSTEM_PROMPT;
    Ok(BRIDGE.with(|b| if provider == "gemini" { llm::gemini_request_body(sys, &h, &b.tools) } else { llm::openai_request_body(model, sys, &h, &b.tools) }).to_string())
}

/// Parse a provider response into the neutral `ChatResponse` JSON.
#[wasm_bindgen]
pub fn parse_response(provider: &str, body_json: &str) -> Result<String, JsError> {
    let v = parse(body_json, "response")?;
    let r: ChatResponse = if provider == "gemini" { llm::gemini_parse_response(&v) } else { llm::openai_parse_response(&v) }.map_err(|e| JsError::new(&e))?;
    Ok(serde_json::to_string(&r).unwrap())
}

/// Run one tool call through the bridge. With `history_json` the F-5 grounding check runs against
/// the user's own messages. Returns `{envelope, for_model, latency_ms, is_error, error_code, operator_id}`.
#[wasm_bindgen]
pub fn call_tool(name: &str, args_json: &str, history_json: Option<String>, max_array_for_model: Option<usize>) -> Result<String, JsError> {
    let args = parse(args_json, "args")?;
    let ut = match history_json {
        Some(h) => Some(llm::user_text(&history(&h)?)),
        None => None,
    };
    Ok(BRIDGE.with(|b| {
        let (env, ms) = b.call_grounded(name, &args, ut.as_deref());
        json!({
            "for_model": compact_for_model(&env, max_array_for_model.unwrap_or(120)),
            "latency_ms": ms,
            "is_error": is_error(&env),
            "error_code": env["error"]["code"],
            "operator_id": b.tool(name).map(|t| t.operator_id.clone()),
            "envelope": env,
        })
        .to_string()
    }))
}

/// Raw operator call (operator id or tool name), no LLM layer: used by the wasm-vs-native parity run.
#[wasm_bindgen]
pub fn run_operator(id: &str, request_json: &str) -> Result<String, JsError> {
    let req = parse(request_json, "request")?;
    let reg = aeoi_control::registry();
    let op = aeoi_control::find(&reg, id).ok_or_else(|| JsError::new(&format!("unknown operator {id}")))?;
    Ok(op.call(&req, &aeoi_core::HostCtx::default()).to_string())
}

#[wasm_bindgen]
pub fn nudge_text(why: &str) -> String {
    llm::nudge_text(why)
}

/// The `mock:demo` script (array of neutral `ChatResponse`).
#[wasm_bindgen]
pub fn demo_script() -> String {
    serde_json::to_string(&llm::demo_script()).unwrap()
}
