//! LLM provider abstraction. Gemini (Generative Language API, function calling) is the primary
//! provider; any OpenAI-compatible `/v1/chat/completions` endpoint (OpenRouter, Groq, Ollama,
//! OpenAI, custom) plugs in through the same trait. `MockProvider` replays a script for tests.
use crate::tools::ToolDef;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    /// Per-attempt HTTP timeout. A stalled connection used to sit for 180 s and then be retried
    /// `max_retries` more times (tens of minutes, with no report written).
    pub request_timeout: Duration,
    /// Wall-clock cap on sleeping + retries for a single `chat` call. The next wait is not started
    /// once `elapsed + wait` would pass this.
    pub total_budget: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 4,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(65),
            request_timeout: Duration::from_secs(90),
            total_budget: Duration::from_secs(150),
        }
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

enum RetryDecision {
    /// Sleep, then try again.
    Retry(Duration),
    Stop(String),
}

/// Decide what a failed HTTP attempt means. Daily quotas and delays far past `max_delay` stop
/// immediately (`QUOTA_EXHAUSTED`) so a free-tier 429 cannot turn into an hour of sleeping.
/// Transient 429/408/5xx and transport errors (including the client timeout) back off, but only
/// while `elapsed + wait` stays inside `total_budget`.
fn classify_failure(status: Option<u16>, body: &Value, retry_after: Option<&str>, attempt: u32, elapsed: Duration, policy: &RetryPolicy) -> RetryDecision {
    let msg = body["error"]["message"].as_str().map(|s| s.to_string()).unwrap_or_else(|| body.to_string().chars().take(400).collect());
    let retryable = status.map(|s| s == 429 || s == 408 || (500..600).contains(&s)).unwrap_or(true);
    let code = status.map(|s| s.to_string()).unwrap_or_else(|| "-".into());
    if !retryable || attempt >= policy.max_retries {
        return RetryDecision::Stop(format!("HTTP {code}: {msg}"));
    }
    let sd = server_delay(body, retry_after);
    // Gemini: quotaId …PerDay…; OpenRouter: "Rate limit exceeded: free-models-per-day"
    let blob = body.to_string().to_ascii_lowercase();
    let per_day = blob.contains("perday") || blob.contains("per-day") || blob.contains("per day");
    if status == Some(429) && (per_day || sd.map(|d| d > policy.max_delay.saturating_mul(3)).unwrap_or(false)) {
        let short: String = body["error"]["message"].as_str().unwrap_or("").lines().filter(|l| l.contains("Quota exceeded") || l.contains("retry in")).collect::<Vec<_>>().join(" ");
        let short = if short.is_empty() { msg.chars().take(200).collect() } else { short };
        return RetryDecision::Stop(format!("{QUOTA_EXHAUSTED}: {short}"));
    }
    let backoff = policy.base_delay.mul_f64(2f64.powi(attempt as i32)).min(policy.max_delay);
    let wait = sd.map(|d| d.max(backoff).min(policy.max_delay)).unwrap_or(backoff);
    if elapsed.saturating_add(wait) > policy.total_budget {
        let why: String = msg.chars().take(160).collect();
        return RetryDecision::Stop(format!("HTTP {code}: retry budget {:.0}s exhausted after {attempt} retries ({why})", policy.total_budget.as_secs_f64()));
    }
    RetryDecision::Retry(wait)
}

async fn post_json(client: &reqwest::Client, url: &str, headers: &[(&str, String)], body: &Value, policy: &RetryPolicy) -> Result<(Value, u32), String> {
    let started = Instant::now();
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
        let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"error": {"message": text.chars().take(500).collect::<String>()}}));
        match status {
            Some(st) if st.is_success() => return Ok((parsed, attempt)),
            st => {
                let code = st.map(|s| s.as_u16());
                match classify_failure(code, &parsed, retry_after.as_deref(), attempt, started.elapsed(), policy) {
                    RetryDecision::Stop(e) => return Err(e),
                    RetryDecision::Retry(wait) => {
                        let why: String = parsed["error"]["message"].as_str().unwrap_or("").chars().take(160).collect();
                        eprintln!("[provider] HTTP {:?} {why} — retry {}/{} in {:.1}s", code, attempt + 1, policy.max_retries, wait.as_secs_f64());
                        tokio::time::sleep(wait).await;
                        attempt += 1;
                    }
                }
            }
        }
    }
}

fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder().timeout(timeout).build().expect("reqwest client")
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
        let policy = RetryPolicy::default();
        Ok(Gemini { key, model: model.to_string(), base: std::env::var("GEMINI_BASE_URL").unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta".into()), client: http_client(policy.request_timeout), policy })
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
        let policy = RetryPolicy::default();
        Ok(OpenAiCompat { label: label.to_string(), base: std::env::var(base_env).unwrap_or_else(|_| base.to_string()), key, model: model.to_string(), client: http_client(policy.request_timeout), policy })
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
    label: String,
}

impl MockProvider {
    pub fn new(turns: Vec<ChatResponse>) -> MockProvider {
        let mut t = turns;
        t.reverse();
        MockProvider { script: Mutex::new(t), seen: Mutex::new(vec![]), last_tool_responses: Mutex::new(vec![]), label: "mock".into() }
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
        let mut m = MockProvider::new(aeoi_bridge::llm::demo_script());
        m.label = "demo".into();
        m
    }
}

impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        &self.label
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

/// Offline design loop (`mock:design`): one analysis call, then `submit_design` filled from the
/// task's answer schema (numbers and integers are 1, arrays are `[1, 1]`). Works for every task
/// without a network or a key. `alpha = 1` happens to pass ZC_02; other tasks are scored as usual.
pub struct DesignMock;

fn placeholder_answer(tools: &[ToolDef]) -> Value {
    let Some(t) = tools.iter().find(|t| t.name == "submit_design") else { return json!({}) };
    let mut ans = serde_json::Map::new();
    if let Some(props) = t.json_schema["properties"]["answer"]["properties"].as_object() {
        for (k, sch) in props {
            let v = match sch["type"].as_str() {
                Some("array") => json!([1.0, 1.0]),
                Some("integer") => json!(1),
                _ => json!(1.0),
            };
            ans.insert(k.clone(), v);
        }
    }
    Value::Object(ans)
}

impl Provider for DesignMock {
    fn name(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        "design"
    }
    fn chat<'a>(&'a self, _system: &'a str, msgs: &'a [Msg], tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move {
            let saw_tool = msgs.iter().any(|m| matches!(m, Msg::Tool { .. }));
            if !saw_tool {
                return Ok(MockProvider::calls(vec![("control_lti_poles_zeros", json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 1.0], "ts": 0.0}}))]));
            }
            Ok(MockProvider::calls(vec![("submit_design", json!({"answer": placeholder_answer(tools), "summary": "offline mock:design"}))]))
        })
    }
}

/// File-scripted provider (`mock:script` / `--mock-script`). Each element is
/// `{"calls":[{"name","args"}]}`, `{"text":"..."}`, or `{"error":"..."}`.
/// `{"error":"QUOTA_EXHAUSTED: ..."}` is how tests and dry-runs exercise the abort path.
pub struct ScriptMock {
    script: Mutex<Vec<Result<ChatResponse, String>>>,
    label: String,
}

pub fn parse_script(text: &str) -> Result<Vec<Result<ChatResponse, String>>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let arr = v.as_array().ok_or("脚本必须是 JSON 数组")?;
    let mut out = vec![];
    for t in arr {
        if let Some(err) = t.get("error").and_then(|e| e.as_str()) {
            out.push(Err(err.to_string()));
            continue;
        }
        if let Some(calls) = t.get("calls").and_then(|c| c.as_array()) {
            let cs = calls.iter().enumerate().map(|(i, c)| ToolCall {
                id: format!("m{i}"),
                name: c["name"].as_str().unwrap_or("").into(),
                args: if c.get("args").is_some() { c["args"].clone() } else { json!({}) },
            }).collect();
            out.push(Ok(ChatResponse {
                text: t["text"].as_str().map(|s| s.to_string()),
                calls: cs,
                raw: Value::Null,
                usage: Usage::default(),
                model_version: Some("mock".into()),
                finish_reason: None,
                retries: 0,
            }));
        } else if let Some(text) = t.get("text").and_then(|x| x.as_str()) {
            out.push(Ok(MockProvider::text(text)));
        } else {
            return Err(format!("无法识别的脚本回合: {t}"));
        }
    }
    Ok(out)
}

impl ScriptMock {
    pub fn from_file(path: &str) -> Result<ScriptMock, String> {
        let txt = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let mut turns = parse_script(&txt)?;
        turns.reverse();
        Ok(ScriptMock { script: Mutex::new(turns), label: format!("script:{path}") })
    }
}

impl Provider for ScriptMock {
    fn name(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        &self.label
    }
    fn chat<'a>(&'a self, _system: &'a str, _msgs: &'a [Msg], _tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move { self.script.lock().unwrap().pop().unwrap_or_else(|| Err("mock script exhausted".into())) })
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
        "mock" => match m.as_str() {
            "demo" => Ok(Box::new(MockProvider::demo())),
            "design" => Ok(Box::new(DesignMock)),
            "script" => {
                let path = std::env::var("AEOI_MOCK_SCRIPT").map_err(|_| "mock:script 需要环境变量 AEOI_MOCK_SCRIPT 或 --mock-script <文件>".to_string())?;
                Ok(Box::new(ScriptMock::from_file(&path)?))
            }
            other => Err(format!("未知的 mock 模型 {other}。可用：mock:demo（网页演示）、mock:design（设计评测离线脚本）、mock:script（AEOI_MOCK_SCRIPT）")),
        },
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

    fn policy_for_tests() -> RetryPolicy {
        RetryPolicy { max_retries: 5, base_delay: Duration::from_millis(20), max_delay: Duration::from_secs(65), request_timeout: Duration::from_millis(200), total_budget: Duration::from_secs(2) }
    }

    #[test]
    fn daily_quota_stops_without_a_retry() {
        let body = json!({"error": {"message": "Quota exceeded for metric: GenerateRequestsPerDayPerProjectPerModel-FreeTier. retry in 12h", "details": [{"retryDelay": "3600s"}]}});
        let d = classify_failure(Some(429), &body, None, 0, Duration::ZERO, &policy_for_tests());
        match d {
            RetryDecision::Stop(s) => assert!(s.starts_with(QUOTA_EXHAUSTED), "{s}"),
            RetryDecision::Retry(w) => panic!("should not sleep {w:?}"),
        }
    }

    #[test]
    fn transient_429_backs_off_inside_the_budget() {
        let body = json!({"error": {"message": "Resource exhausted. Please try again.", "details": [{"retryDelay": "0.5s"}]}});
        let p = policy_for_tests();
        match classify_failure(Some(429), &body, None, 0, Duration::from_millis(10), &p) {
            RetryDecision::Retry(w) => assert!(w >= Duration::from_millis(500) && w <= p.max_delay, "{w:?}"),
            RetryDecision::Stop(s) => panic!("transient 429 should retry, got {s}"),
        }
        // a wait that would blow the budget is a hard stop, not another sleep
        let huge = json!({"error": {"message": "slow down", "details": [{"retryDelay": "30s"}]}});
        let mut tight = p;
        tight.total_budget = Duration::from_millis(100);
        tight.max_delay = Duration::from_secs(65);
        match classify_failure(Some(429), &huge, None, 1, Duration::from_millis(80), &tight) {
            RetryDecision::Stop(s) => assert!(s.contains("budget") && !s.starts_with(QUOTA_EXHAUSTED), "{s}"),
            RetryDecision::Retry(w) => panic!("should not sleep {w:?} past the budget"),
        }
    }

    #[test]
    fn openrouter_daily_quota_and_client_error() {
        let body = json!({"error": {"message": "Rate limit exceeded: free-models-per-day"}});
        match classify_failure(Some(429), &body, Some("86400"), 0, Duration::ZERO, &RetryPolicy::default()) {
            RetryDecision::Stop(s) => assert!(s.starts_with(QUOTA_EXHAUSTED), "{s}"),
            RetryDecision::Retry(_) => panic!("per-day 429 must not retry"),
        }
        let bad = json!({"error": {"message": "invalid argument"}});
        match classify_failure(Some(400), &bad, None, 0, Duration::ZERO, &RetryPolicy::default()) {
            RetryDecision::Stop(s) => assert!(s.contains("400") && !s.starts_with(QUOTA_EXHAUSTED)),
            RetryDecision::Retry(_) => panic!("400 is not retryable"),
        }
    }

    fn http_response(status: u16, json_body: &str) -> String {
        format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json_body}", json_body.len())
    }

    /// Local one-shot HTTP server. `None` accepts the request and never answers (stall).
    async fn serve(responses: Vec<Option<String>>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut left = std::collections::VecDeque::from(responses);
            while let Some(resp) = left.pop_front() {
                let Ok((mut sock, _)) = listener.accept().await else { break };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let _ = tokio::time::timeout(Duration::from_secs(2), sock.read(&mut buf)).await;
                    if let Some(body) = resp {
                        let _ = sock.write_all(body.as_bytes()).await;
                    } else {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                });
            }
        });
        format!("http://{addr}/chat")
    }

    #[tokio::test]
    async fn post_json_quota_returns_immediately() {
        let body = r#"{"error":{"message":"Quota exceeded for metric: GenerateRequestsPerDayPerProjectPerModel-FreeTier","details":[{"retryDelay":"3600s"}]}}"#;
        let url = serve(vec![Some(http_response(429, body))]).await;
        let policy = RetryPolicy { base_delay: Duration::from_secs(30), max_delay: Duration::from_secs(60), max_retries: 6, request_timeout: Duration::from_secs(5), total_budget: Duration::from_secs(120) };
        let t0 = Instant::now();
        let err = post_json(&http_client(policy.request_timeout), &url, &[], &json!({}), &policy).await.unwrap_err();
        assert!(err.starts_with(QUOTA_EXHAUSTED), "{err}");
        assert!(t0.elapsed() < Duration::from_secs(2), "slept {:?}", t0.elapsed());
    }

    #[tokio::test]
    async fn post_json_retries_429_then_succeeds() {
        let retry = http_response(429, r#"{"error":{"message":"temporarily rate-limited upstream","details":[{"retryDelay":"0.5s"}]}}"#);
        let ok = http_response(200, r#"{"candidates":[]}"#);
        let url = serve(vec![Some(retry), Some(ok)]).await;
        let policy = RetryPolicy { base_delay: Duration::from_millis(50), max_delay: Duration::from_secs(2), max_retries: 4, request_timeout: Duration::from_secs(2), total_budget: Duration::from_secs(5) };
        let t0 = Instant::now();
        let (v, retries) = post_json(&http_client(policy.request_timeout), &url, &[], &json!({"x":1}), &policy).await.unwrap();
        assert_eq!(retries, 1, "{v}");
        assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
        assert!(t0.elapsed() >= Duration::from_millis(400), "did not honour retryDelay: {:?}", t0.elapsed());
    }

    #[tokio::test]
    async fn post_json_stall_is_cut_by_timeout_and_budget() {
        let url = serve(vec![None, None, None, None]).await;
        let policy = RetryPolicy { base_delay: Duration::from_secs(30), max_delay: Duration::from_secs(30), max_retries: 6, request_timeout: Duration::from_millis(200), total_budget: Duration::from_millis(500) };
        let t0 = Instant::now();
        let err = post_json(&http_client(policy.request_timeout), &url, &[], &json!({}), &policy).await.unwrap_err();
        assert!(err.contains("budget") || err.contains("transport") || err.contains("timeout"), "{err}");
        assert!(t0.elapsed() < Duration::from_secs(2), "stall was not bounded: {:?} {err}", t0.elapsed());
    }
}
