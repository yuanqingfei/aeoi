//! LLM provider abstraction. Gemini (Generative Language API, function calling) is the primary
//! provider; any OpenAI-compatible `/v1/chat/completions` endpoint (OpenRouter, Groq, Ollama,
//! OpenAI, custom) plugs in through the same trait. `MockProvider` replays a script for tests.
use crate::tools::ToolDef;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

pub use aeoi_bridge::llm::{ChatResponse, Msg, ToolCall, Usage};

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
        aeoi_bridge::llm::gemini_request_body(system, msgs, tools)
    }

    pub fn parse_response(v: &Value) -> Result<ChatResponse, String> {
        aeoi_bridge::llm::gemini_parse_response(v)
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
        aeoi_bridge::llm::openai_request_body(model, system, msgs, tools)
    }

    pub fn parse_response(v: &Value) -> Result<ChatResponse, String> {
        aeoi_bridge::llm::openai_parse_response(v)
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
        aeoi_bridge::llm::mock_calls(calls)
    }
    pub fn text(t: &str) -> ChatResponse {
        aeoi_bridge::llm::mock_text(t)
    }
}

impl MockProvider {
    /// Offline demo script (`mock:demo`): a bad first call (nested-array matrix → schema error with
    /// a lossless patch), the corrected call, a step + bode call, then a final answer. Lets the UI be
    /// exercised without any API key.
    pub fn demo() -> MockProvider {
        MockProvider::new(aeoi_bridge::llm::demo_script())
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
            Msg::user("q"),
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
        let body = OpenAiCompat::request_body("m", "s", &[Msg::user("q"), Msg::Assistant { text: None, calls: r.calls.clone(), raw: r.raw }, Msg::Tool { call_id: "t1".into(), name: "control_lti_bode".into(), response: json!({"r":1}) }], &[]);
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
