//! LLM provider abstraction. Gemini (Generative Language API, function calling) is the primary
//! provider; any OpenAI-compatible `/v1/chat/completions` endpoint (OpenRouter, Groq, Ollama,
//! OpenAI, custom) plugs in through the same trait. `MockProvider` replays a script for tests.
use crate::tools::ToolDef;
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// Provider-neutral conversation message.
#[derive(Debug, Clone)]
pub enum Msg {
    User(String),
    /// `raw` = provider-native assistant content, echoed back verbatim on the next turn
    /// (Gemini requires its `thoughtSignature` parts to be returned unchanged).
    Assistant { text: Option<String>, calls: Vec<ToolCall>, raw: Value },
    Tool { call_id: String, name: String, response: Value },
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub thought_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub calls: Vec<ToolCall>,
    pub raw: Value,
    pub usage: Usage,
    pub model_version: Option<String>,
    pub finish_reason: Option<String>,
    /// HTTP retries (429/5xx) spent on this turn.
    pub retries: u32,
}

/// Prefix of provider errors that mean "no point retrying today" (e.g. Gemini free-tier requests/day).
pub const QUOTA_EXHAUSTED: &str = "QUOTA_EXHAUSTED";

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn chat<'a>(&'a self, system: &'a str, msgs: &'a [Msg], tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>>;
}

// ---------------------------------------------------------------- HTTP with rate-limit-friendly retry

pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy { max_retries: 6, base_delay: Duration::from_secs(2), max_delay: Duration::from_secs(65) }
    }
}

/// Parse a Gemini `RetryInfo.retryDelay` ("37s" / "1.5s") or a `Retry-After` header.
fn server_delay(body: &Value, retry_after: Option<&str>) -> Option<Duration> {
    if let Some(details) = body["error"]["details"].as_array() {
        for d in details {
            if let Some(s) = d["retryDelay"].as_str() {
                if let Ok(x) = s.trim_end_matches('s').parse::<f64>() {
                    return Some(Duration::from_secs_f64(x.max(0.5)));
                }
            }
        }
    }
    retry_after.and_then(|s| s.trim().parse::<f64>().ok()).map(Duration::from_secs_f64)
}

async fn post_json(client: &reqwest::Client, url: &str, headers: &[(&str, String)], body: &Value, policy: &RetryPolicy) -> Result<(Value, u32), String> {
    let mut attempt = 0u32;
    loop {
        let mut rb = client.post(url).json(body);
        for (k, v) in headers {
            rb = rb.header(*k, v);
        }
        let res = rb.send().await;
        let (status, retry_after, text) = match res {
            Ok(r) => {
                let st = r.status();
                let ra = r.headers().get("retry-after").and_then(|h| h.to_str().ok()).map(|s| s.to_string());
                (Some(st), ra, r.text().await.unwrap_or_default())
            }
            Err(e) => (None, None, format!("transport error: {e}")),
        };
        let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"_raw": text.chars().take(500).collect::<String>()}));
        match status {
            Some(st) if st.is_success() => return Ok((parsed, attempt)),
            st => {
                let retryable = st.map(|s| s.as_u16() == 429 || s.as_u16() == 408 || s.is_server_error()).unwrap_or(true);
                if !retryable || attempt >= policy.max_retries {
                    let msg = parsed["error"]["message"].as_str().map(|s| s.to_string()).unwrap_or_else(|| parsed.to_string().chars().take(400).collect());
                    return Err(format!("HTTP {}: {}", st.map(|s| s.as_u16().to_string()).unwrap_or("-".into()), msg));
                }
                // a daily quota (or any server delay far beyond our cap) will not clear by retrying
                let sd = server_delay(&parsed, retry_after.as_deref());
                // Gemini: quotaId …PerDay…; OpenRouter: "Rate limit exceeded: free-models-per-day"
                let body = parsed.to_string().to_ascii_lowercase();
                let per_day = body.contains("perday") || body.contains("per-day") || body.contains("per day");
                if st.map(|s| s.as_u16()) == Some(429) && (per_day || sd.map(|d| d > policy.max_delay * 3).unwrap_or(false)) {
                    let msg: String = parsed["error"]["message"].as_str().unwrap_or("").lines().filter(|l| l.contains("Quota exceeded") || l.contains("retry in")).collect::<Vec<_>>().join(" ");
                    let msg = if msg.is_empty() { parsed["error"]["message"].as_str().unwrap_or("").chars().take(200).collect() } else { msg };
                    return Err(format!("{QUOTA_EXHAUSTED}: {msg}"));
                }
                let backoff = policy.base_delay.mul_f64(2f64.powi(attempt as i32)).min(policy.max_delay);
                let wait = sd.map(|d| d.max(backoff).min(policy.max_delay)).unwrap_or(backoff);
                let why: String = parsed["error"]["message"].as_str().unwrap_or("").chars().take(160).collect();
                eprintln!("[provider] HTTP {:?} {why} — retry {}/{} in {:.1}s", st.map(|s| s.as_u16()), attempt + 1, policy.max_retries, wait.as_secs_f64());
                tokio::time::sleep(wait).await;
                attempt += 1;
            }
        }
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(180)).build().expect("reqwest client")
}

// ---------------------------------------------------------------- Gemini

pub struct Gemini {
    key: String,
    model: String,
    base: String,
    client: reqwest::Client,
    pub policy: RetryPolicy,
}

impl Gemini {
    pub fn from_env(model: &str) -> Result<Gemini, String> {
        let key = std::env::var("GEMINI_API_KEY").map_err(|_| "环境变量 GEMINI_API_KEY 未设置".to_string())?;
        if key.trim().is_empty() {
            return Err("GEMINI_API_KEY 为空".into());
        }
        Ok(Gemini { key, model: model.to_string(), base: std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".into()), client: http_client(), policy: RetryPolicy::default() })
    }

    pub fn request_body(system: &str, msgs: &[Msg], tools: &[ToolDef]) -> Value {
        let mut contents: Vec<Value> = vec![];
        for m in msgs {
            match m {
                Msg::User(t) => contents.push(json!({"role": "user", "parts": [{"text": t}]})),
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

    pub fn parse_response(v: &Value) -> Result<ChatResponse, String> {
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

    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/models?pageSize=200", self.base);
        let r = self.client.get(&url).header("x-goog-api-key", &self.key).send().await.map_err(|e| e.to_string())?;
        let v: Value = r.json().await.map_err(|e| e.to_string())?;
        let mut out = vec![];
        for m in v["models"].as_array().cloned().unwrap_or_default() {
            let gen = m["supportedGenerationMethods"].as_array().map(|a| a.iter().any(|x| x == "generateContent")).unwrap_or(false);
            let name = m["name"].as_str().unwrap_or("").trim_start_matches("models/").to_string();
            if gen && name.starts_with("gemini") && !name.contains("tts") && !name.contains("image") && !name.contains("embedding") {
                out.push(name);
            }
        }
        Ok(out)
    }
}

impl Provider for Gemini {
    fn name(&self) -> &str {
        "gemini"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn chat<'a>(&'a self, system: &'a str, msgs: &'a [Msg], tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move {
            let body = Gemini::request_body(system, msgs, tools);
            let url = format!("{}/models/{}:generateContent", self.base, self.model);
            let (v, retries) = post_json(&self.client, &url, &[("x-goog-api-key", self.key.clone())], &body, &self.policy).await?;
            let mut r = Gemini::parse_response(&v)?;
            r.retries = retries;
            Ok(r)
        })
    }
}

// ---------------------------------------------------------------- OpenAI-compatible

pub struct OpenAiCompat {
    label: String,
    base: String,
    key: Option<String>,
    model: String,
    client: reqwest::Client,
    pub policy: RetryPolicy,
}

/// (provider, default base URL, key env var, base-URL override env var)
pub const OPENAI_COMPAT: &[(&str, &str, &str, &str)] = &[
    ("openrouter", "https://openrouter.ai/api/v1", "OPENROUTER_API_KEY", "OPENROUTER_BASE_URL"),
    ("groq", "https://api.groq.com/openai/v1", "GROQ_API_KEY", "GROQ_BASE_URL"),
    ("ollama", "http://localhost:11434/v1", "OLLAMA_API_KEY", "OLLAMA_BASE_URL"),
    ("openai", "https://api.openai.com/v1", "OPENAI_API_KEY", "OPENAI_BASE_URL"),
    ("compat", "http://localhost:8000/v1", "OPENAI_COMPAT_API_KEY", "OPENAI_COMPAT_BASE_URL"),
];

impl OpenAiCompat {
    pub fn from_env(provider: &str, model: &str) -> Result<OpenAiCompat, String> {
        let (label, base, key_env, base_env) = OPENAI_COMPAT.iter().find(|p| p.0 == provider).ok_or_else(|| format!("未知 provider {provider}"))?;
        let key = std::env::var(key_env).ok().filter(|k| !k.trim().is_empty());
        if key.is_none() && *label != "ollama" && *label != "compat" {
            return Err(format!("环境变量 {key_env} 未设置"));
        }
        Ok(OpenAiCompat { label: label.to_string(), base: std::env::var(base_env).unwrap_or_else(|_| base.to_string()), key, model: model.to_string(), client: http_client(), policy: RetryPolicy::default() })
    }

    pub fn request_body(model: &str, system: &str, msgs: &[Msg], tools: &[ToolDef]) -> Value {
        let mut out = vec![json!({"role": "system", "content": system})];
        for m in msgs {
            match m {
                Msg::User(t) => out.push(json!({"role": "user", "content": t})),
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

    pub fn parse_response(v: &Value) -> Result<ChatResponse, String> {
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
}

impl Provider for OpenAiCompat {
    fn name(&self) -> &str {
        &self.label
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn chat<'a>(&'a self, system: &'a str, msgs: &'a [Msg], tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move {
            let body = OpenAiCompat::request_body(&self.model, system, msgs, tools);
            let mut headers = vec![];
            if let Some(k) = &self.key {
                headers.push(("authorization", format!("Bearer {k}")));
            }
            if self.label == "openrouter" {
                headers.push(("x-title", "AEOI web test".to_string()));
            }
            let (v, retries) = post_json(&self.client, &format!("{}/chat/completions", self.base.trim_end_matches('/')), &headers, &body, &self.policy).await?;
            let mut r = OpenAiCompat::parse_response(&v)?;
            r.retries = retries;
            Ok(r)
        })
    }
}

// ---------------------------------------------------------------- Mock

/// Replays scripted turns; each turn is either tool calls or a final text. Records what it was sent.
pub struct MockProvider {
    script: Mutex<Vec<ChatResponse>>,
    pub seen: Mutex<Vec<usize>>,
    pub last_tool_responses: Mutex<Vec<Value>>,
}

impl MockProvider {
    pub fn new(turns: Vec<ChatResponse>) -> MockProvider {
        let mut t = turns;
        t.reverse();
        MockProvider { script: Mutex::new(t), seen: Mutex::new(vec![]), last_tool_responses: Mutex::new(vec![]) }
    }
    pub fn calls(calls: Vec<(&str, Value)>) -> ChatResponse {
        ChatResponse { text: None, calls: calls.into_iter().enumerate().map(|(i, (n, a))| ToolCall { id: format!("m{i}"), name: n.into(), args: a }).collect(), raw: Value::Null, usage: Usage::default(), model_version: Some("mock".into()), finish_reason: None, retries: 0 }
    }
    pub fn text(t: &str) -> ChatResponse {
        ChatResponse { text: Some(t.into()), calls: vec![], raw: Value::Null, usage: Usage { total_tokens: Some(10), ..Default::default() }, model_version: Some("mock".into()), finish_reason: Some("STOP".into()), retries: 0 }
    }
}

impl MockProvider {
    /// Offline demo script (`mock:demo`): a bad first call (nested-array matrix → schema error with
    /// a lossless patch), the corrected call, a step + bode call, then a final answer. Lets the UI be
    /// exercised without any API key.
    pub fn demo() -> MockProvider {
        let sys = json!({"kind": "tf", "num": [4.0], "den": [1.0, 1.2, 4.0], "ts": 0.0});
        MockProvider::new(vec![
            MockProvider::calls(vec![("control_design_lqr", json!({"A": [[0, 1], [0, 0]], "B": [[0], [1]], "Q": [[1, 0], [0, 1]], "R": [[1]]}))]),
            MockProvider::calls(vec![("control_design_lqr", json!({"A": {"dtype": "f64", "shape": [2, 2], "data": [0, 1, 0, 0]}, "B": {"dtype": "f64", "shape": [2, 1], "data": [0, 1]}, "Q": {"dtype": "f64", "shape": [2, 2], "data": [1, 0, 0, 1]}, "R": {"dtype": "f64", "shape": [1, 1], "data": [1]}}))]),
            MockProvider::calls(vec![
                ("control_lti_step", json!({"system": sys, "time": {"mode": "uniform", "t_final": 10, "dt": 0.01, "unit": "s"}, "step_info": {"enabled": true}})),
                ("control_lti_bode", json!({"system": sys, "frequency": {"mode": "logspace", "min": 0.1, "max": 100, "n": 200, "unit": "rad/s"}})),
            ]),
            MockProvider::text("（离线演示，未调用真实模型）\n- LQR 增益 **K = [1, 1.732]**\n- G(s)=4/(s²+1.2s+4)：超调 **37.2%**，峰值时间 1.65 s，调节时间 5.62 s\n- Bode 图见右侧轨迹面板。"),
        ])
    }
}

impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        "mock"
    }
    fn chat<'a>(&'a self, _system: &'a str, msgs: &'a [Msg], _tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(msgs.len());
            let tr: Vec<Value> = msgs.iter().rev().take_while(|m| matches!(m, Msg::Tool { .. })).filter_map(|m| if let Msg::Tool { response, .. } = m { Some(response.clone()) } else { None }).collect();
            *self.last_tool_responses.lock().unwrap() = tr;
            self.script.lock().unwrap().pop().ok_or_else(|| "mock script exhausted".to_string())
        })
    }
}

// ---------------------------------------------------------------- factory

pub const DEFAULT_MODEL: &str = "gemini:gemini-2.5-flash";

/// `provider:model`; a bare model name means Gemini. Examples: `gemini:gemini-2.5-flash`,
/// `openrouter:qwen/qwen3-coder:free`, `groq:llama-3.3-70b-versatile`, `ollama:qwen2.5:7b`.
pub fn split_spec(spec: &str) -> (String, String) {
    match spec.split_once(':') {
        Some((p, m)) if p == "gemini" || p == "mock" || OPENAI_COMPAT.iter().any(|x| x.0 == p) => (p.to_string(), m.to_string()),
        _ => ("gemini".to_string(), spec.to_string()),
    }
}

pub fn make(spec: &str) -> Result<Box<dyn Provider>, String> {
    let (p, m) = split_spec(spec);
    match p.as_str() {
        "gemini" => Ok(Box::new(Gemini::from_env(&m)?)),
        "mock" => Ok(Box::new(MockProvider::demo())),
        _ => Ok(Box::new(OpenAiCompat::from_env(&p, &m)?)),
    }
}

/// Which providers have credentials configured (never returns the keys themselves).
pub fn configured() -> Vec<Value> {
    let mut v = vec![json!({"provider": "gemini", "configured": std::env::var("GEMINI_API_KEY").map(|k| !k.trim().is_empty()).unwrap_or(false), "key_env": "GEMINI_API_KEY"})];
    for (p, _, k, _) in OPENAI_COMPAT {
        let has = std::env::var(k).map(|k| !k.trim().is_empty()).unwrap_or(false) || *p == "ollama";
        v.push(json!({"provider": p, "configured": has, "key_env": k}));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_parsing() {
        assert_eq!(split_spec("gemini-2.5-flash"), ("gemini".into(), "gemini-2.5-flash".into()));
        assert_eq!(split_spec("ollama:qwen2.5:7b"), ("ollama".into(), "qwen2.5:7b".into()));
        assert_eq!(split_spec("openrouter:qwen/qwen3:free"), ("openrouter".into(), "qwen/qwen3:free".into()));
    }

    #[test]
    fn gemini_roundtrip_preserves_signature_and_groups_tool_results() {
        let resp = json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"a","args":{"x":1},"id":"c1"},"thoughtSignature":"SIG"},{"functionCall":{"name":"b","args":{}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8},"modelVersion":"gemini-x"});
        let r = Gemini::parse_response(&resp).unwrap();
        assert_eq!(r.calls.len(), 2);
        assert_eq!(r.calls[0].id, "c1");
        assert_eq!(r.usage.total_tokens, Some(8));
        let msgs = vec![
            Msg::User("q".into()),
            Msg::Assistant { text: None, calls: r.calls.clone(), raw: r.raw.clone() },
            Msg::Tool { call_id: "c1".into(), name: "a".into(), response: json!({"ok":1}) },
            Msg::Tool { call_id: r.calls[1].id.clone(), name: "b".into(), response: json!({"ok":2}) },
        ];
        let body = Gemini::request_body("sys", &msgs, &[]);
        let c = body["contents"].as_array().unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[1]["parts"][0]["thoughtSignature"], "SIG");
        assert_eq!(c[2]["parts"].as_array().unwrap().len(), 2);
        assert_eq!(c[2]["parts"][0]["functionResponse"]["id"], "c1");
        assert!(c[2]["parts"][1]["functionResponse"].get("id").is_none());
    }

    #[test]
    fn openai_roundtrip() {
        let resp = json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"t1","type":"function","function":{"name":"control_lti_bode","arguments":"{\"a\":1}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}});
        let r = OpenAiCompat::parse_response(&resp).unwrap();
        assert_eq!(r.calls[0].args, json!({"a":1}));
        let body = OpenAiCompat::request_body("m", "s", &[Msg::User("q".into()), Msg::Assistant { text: None, calls: r.calls.clone(), raw: r.raw }, Msg::Tool { call_id: "t1".into(), name: "control_lti_bode".into(), response: json!({"r":1}) }], &[]);
        let m = body["messages"].as_array().unwrap();
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"], "{\"a\":1}");
        assert_eq!(m[3]["tool_call_id"], "t1");
    }

    #[test]
    fn retry_delay_parsing() {
        let b = json!({"error":{"details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"37s"}]}});
        assert_eq!(server_delay(&b, None), Some(Duration::from_secs(37)));
        assert_eq!(server_delay(&json!({}), Some("3")), Some(Duration::from_secs(3)));
    }
}
