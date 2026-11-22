//! Provider wire formats (Gemini `generateContent`, OpenAI-compatible `chat/completions`) and the
//! provider-neutral conversation model. Pure functions: transport (reqwest natively, `fetch` in the
//! browser) is the caller's business.
use crate::tools::ToolDef;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// Provider-neutral conversation message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Msg {
    User { text: String },
    /// `raw` = provider-native assistant content, echoed back verbatim on the next turn
    /// (Gemini requires its `thoughtSignature` parts to be returned unchanged).
    Assistant { text: Option<String>, calls: Vec<ToolCall>, raw: Value },
    Tool { call_id: String, name: String, response: Value },
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub thought_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub calls: Vec<ToolCall>,
    pub raw: Value,
    pub usage: Usage,
    pub model_version: Option<String>,
    pub finish_reason: Option<String>,
    /// HTTP retries (429/5xx) spent on this turn.
    #[serde(default)]
    pub retries: u32,
}

impl Msg {
    pub fn user(t: impl Into<String>) -> Msg {
        Msg::User { text: t.into() }
    }
}

/// Marker of harness-generated user turns (nudges); they never count as the user's own words.
pub const NUDGE_PREFIX: &str = "（系统提示）";

/// Corrective user turn after an empty model turn (e.g. Gemini `MALFORMED_FUNCTION_CALL`).
pub fn nudge_text(why: &str) -> String {
    format!("{NUDGE_PREFIX}上一轮没有产生有效输出（{why}）。如果要调用工具，请严格按该工具的 parameters 构造参数：矩阵写成 {{\"dtype\":\"f64\",\"shape\":[行,列],\"data\":[...]}}，必填字段齐全；如果缺少必要信息，请直接用中文向用户提问。")
}

/// Everything the user actually said in this conversation (for the F-5 grounding check).
pub fn user_text(history: &[Msg]) -> String {
    history.iter().filter_map(|m| if let Msg::User { text } = m { (!text.starts_with(NUDGE_PREFIX)).then_some(text.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------- Gemini

pub fn gemini_request_body(system: &str, msgs: &[Msg], tools: &[ToolDef]) -> Value {
    let mut contents: Vec<Value> = vec![];
    for m in msgs {
        match m {
            Msg::User { text: t } => contents.push(json!({"role": "user", "parts": [{"text": t}]})),
            Msg::Assistant { raw, text, calls } => {
                if raw.is_object() && raw.get("parts").is_some() {
                    contents.push(raw.clone());
                } else {
                    // assistant turn produced by another provider: rebuild (no signatures)
                    let mut parts = vec![];
                    if let Some(t) = text {
                        parts.push(json!({"text": t}));
                    }
                    for c in calls {
                        parts.push(json!({"functionCall": {"name": c.name, "args": c.args}}));
                    }
                    contents.push(json!({"role": "model", "parts": parts}));
                }
            }
            Msg::Tool { call_id, name, response } => {
                let mut fr = json!({"name": name, "response": response});
                if !call_id.is_empty() && !call_id.starts_with("auto-") {
                    fr["id"] = json!(call_id);
                }
                let part = json!({"functionResponse": fr});
                // consecutive tool results go into one user turn
                match contents.last_mut() {
                    Some(last) if last["role"] == "user" && last["parts"].as_array().map(|p| p.iter().all(|x| x.get("functionResponse").is_some())).unwrap_or(false) => {
                        last["parts"].as_array_mut().unwrap().push(part);
                    }
                    _ => contents.push(json!({"role": "user", "parts": [part]})),
                }
            }
        }
    }
    let decls: Vec<Value> = tools.iter().map(|t| json!({"name": t.name, "description": t.description, "parameters": t.gemini_schema})).collect();
    json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": contents,
        "tools": [{"functionDeclarations": decls}],
        "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}},
        "generationConfig": {"temperature": 0.0},
    })
}

pub fn gemini_parse_response(v: &Value) -> Result<ChatResponse, String> {
    let cand = &v["candidates"][0];
    if cand.is_null() {
        let fb = &v["promptFeedback"];
        return Err(format!("Gemini 没有返回候选：{}", if fb.is_null() { v.to_string().chars().take(300).collect() } else { fb.to_string() }));
    }
    let content = cand["content"].clone();
    let mut text = String::new();
    let mut calls = vec![];
    if let Some(parts) = content["parts"].as_array() {
        for (i, p) in parts.iter().enumerate() {
            if p["thought"] == true {
                continue;
            }
            if let Some(t) = p["text"].as_str() {
                text.push_str(t);
            }
            if let Some(fc) = p.get("functionCall") {
                calls.push(ToolCall {
                    id: fc["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("auto-{i}")),
                    name: fc["name"].as_str().unwrap_or("").to_string(),
                    args: fc.get("args").cloned().unwrap_or(json!({})),
                });
            }
        }
    }
    let u = &v["usageMetadata"];
    let raw = if content.is_object() && content.get("parts").is_some() { content } else { json!({"role": "model", "parts": [{"text": ""}]}) };
    Ok(ChatResponse {
        text: if text.trim().is_empty() { None } else { Some(text) },
        calls,
        raw,
        usage: Usage { prompt_tokens: u["promptTokenCount"].as_u64(), completion_tokens: u["candidatesTokenCount"].as_u64(), thought_tokens: u["thoughtsTokenCount"].as_u64(), total_tokens: u["totalTokenCount"].as_u64() },
        model_version: v["modelVersion"].as_str().map(|s| s.to_string()),
        finish_reason: cand["finishReason"].as_str().map(|s| match cand["finishMessage"].as_str() {
            Some(m) => format!("{s}: {}", m.chars().take(300).collect::<String>()),
            None => s.to_string(),
        }),
        retries: 0,
    })
}

// ---------------------------------------------------------------- OpenAI-compatible

pub fn openai_request_body(model: &str, system: &str, msgs: &[Msg], tools: &[ToolDef]) -> Value {
    let mut out = vec![json!({"role": "system", "content": system})];
    for m in msgs {
        match m {
            Msg::User { text: t } => out.push(json!({"role": "user", "content": t})),
            Msg::Assistant { text, calls, .. } => {
                let mut a = Map::new();
                a.insert("role".into(), json!("assistant"));
                a.insert("content".into(), text.clone().map(Value::String).unwrap_or(Value::Null));
                if !calls.is_empty() {
                    a.insert("tool_calls".into(), Value::Array(calls.iter().map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.args.to_string()}})).collect()));
                }
                out.push(Value::Object(a));
            }
            Msg::Tool { call_id, response, .. } => out.push(json!({"role": "tool", "tool_call_id": call_id, "content": response.to_string()})),
        }
    }
    let tools: Vec<Value> = tools.iter().map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.json_schema}})).collect();
    json!({"model": model, "messages": out, "tools": tools, "tool_choice": "auto", "temperature": 0})
}

pub fn openai_parse_response(v: &Value) -> Result<ChatResponse, String> {
    let msg = &v["choices"][0]["message"];
    if msg.is_null() {
        return Err(format!("响应中没有 choices：{}", v.to_string().chars().take(300).collect::<String>()));
    }
    let mut calls = vec![];
    for (i, tc) in msg["tool_calls"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let args_s = tc["function"]["arguments"].as_str().unwrap_or("{}");
        let args = serde_json::from_str(args_s).unwrap_or_else(|_| json!({"_unparseable_arguments": args_s}));
        calls.push(ToolCall { id: tc["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("call_{i}")), name: tc["function"]["name"].as_str().unwrap_or("").into(), args });
    }
    let u = &v["usage"];
    Ok(ChatResponse {
        text: msg["content"].as_str().filter(|s| !s.trim().is_empty()).map(|s| s.to_string()),
        calls,
        raw: msg.clone(),
        usage: Usage { prompt_tokens: u["prompt_tokens"].as_u64(), completion_tokens: u["completion_tokens"].as_u64(), thought_tokens: u["completion_tokens_details"]["reasoning_tokens"].as_u64(), total_tokens: u["total_tokens"].as_u64() },
        model_version: v["model"].as_str().map(|s| s.to_string()),
        finish_reason: v["choices"][0]["finish_reason"].as_str().map(|s| s.to_string()),
        retries: 0,
    })
}

// ---------------------------------------------------------------- mock / offline demo

pub fn mock_calls(calls: Vec<(&str, Value)>) -> ChatResponse {
    ChatResponse { text: None, calls: calls.into_iter().enumerate().map(|(i, (n, a))| ToolCall { id: format!("m{i}"), name: n.into(), args: a }).collect(), raw: Value::Null, usage: Usage::default(), model_version: Some("mock".into()), finish_reason: None, retries: 0 }
}

pub fn mock_text(t: &str) -> ChatResponse {
    ChatResponse { text: Some(t.into()), calls: vec![], raw: Value::Null, usage: Usage { total_tokens: Some(10), ..Default::default() }, model_version: Some("mock".into()), finish_reason: Some("STOP".into()), retries: 0 }
}

/// Offline demo script (`mock:demo`): a bad first call (nested-array matrix → schema error with
/// a lossless patch), the corrected call, a step + bode call, then a final answer.
pub fn demo_script() -> Vec<ChatResponse> {
        let sys = json!({"kind": "tf", "num": [4.0], "den": [1.0, 1.2, 4.0], "ts": 0.0});
        vec![
            mock_calls(vec![("control_design_lqr", json!({"A": [[0, 1], [0, 0]], "B": [[0], [1]], "Q": [[1, 0], [0, 1]], "R": [[1]]}))]),
            mock_calls(vec![("control_design_lqr", json!({"A": {"dtype": "f64", "shape": [2, 2], "data": [0, 1, 0, 0]}, "B": {"dtype": "f64", "shape": [2, 1], "data": [0, 1]}, "Q": {"dtype": "f64", "shape": [2, 2], "data": [1, 0, 0, 1]}, "R": {"dtype": "f64", "shape": [1, 1], "data": [1]}}))]),
            mock_calls(vec![
                ("control_lti_step", json!({"system": sys, "time": {"mode": "uniform", "t_final": 10, "dt": 0.01, "unit": "s"}, "step_info": {"enabled": true}})),
                ("control_lti_bode", json!({"system": sys, "frequency": {"mode": "logspace", "min": 0.1, "max": 100, "n": 200, "unit": "rad/s"}})),
            ]),
            mock_text("（离线演示，未调用真实模型）\n- LQR 增益 **K = [1, 1.732]**\n- G(s)=4/(s²+1.2s+4)：超调 **37.2%**，峰值时间 1.65 s，调节时间 5.62 s\n- Bode 图见右侧轨迹面板。"),
        ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_messages_json_shape() {
        // the browser build exchanges history with JS in this shape
        let h: Vec<Msg> = serde_json::from_value(json!([
            {"role": "user", "text": "q"},
            {"role": "assistant", "text": null, "calls": [{"id": "c1", "name": "t", "args": {"a": 1}}], "raw": {"role": "model", "parts": [{"functionCall": {"name": "t", "args": {"a": 1}}, "thoughtSignature": "S"}]}},
            {"role": "tool", "call_id": "c1", "name": "t", "response": {"ok": true}},
            {"role": "user", "text": format!("{NUDGE_PREFIX}x 0.1")}
        ]))
        .unwrap();
        assert_eq!(user_text(&h), "q");
        let g = gemini_request_body("s", &h, &[]);
        assert_eq!(g["contents"][1]["parts"][0]["thoughtSignature"], "S");
        assert_eq!(g["contents"][2]["parts"][0]["functionResponse"]["id"], "c1");
        let back = serde_json::to_value(&h).unwrap();
        assert_eq!(back[0], json!({"role": "user", "text": "q"}));
        assert_eq!(demo_script().len(), 4);
    }
}
