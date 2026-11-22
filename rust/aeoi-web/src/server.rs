//! axum web server: embedded single-page UI + JSON/SSE API. Binds to 127.0.0.1 by default.
use crate::agent::{self, AgentCfg};
use crate::provider::{self, Msg};
use crate::tools::Bridge;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;

const INDEX: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const STYLE: &str = include_str!("../static/style.css");

pub struct AppState {
    pub bridge: Bridge,
    pub default_model: String,
    pub sessions: Mutex<HashMap<String, (String, Vec<Msg>)>>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(|| async { ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX) }))
        .route("/app.js", get(|| async { ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], APP_JS) }))
        .route("/style.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE) }))
        .route("/api/config", get(config))
        .route("/api/tools", get(tools))
        .route("/api/chat", post(chat))
        .route("/api/reset", post(reset))
        .route("/api/call", post(direct_call))
        .with_state(state)
}

async fn config(State(st): State<Arc<AppState>>) -> Json<Value> {
    let mut models: Vec<String> = vec![];
    if let Ok(g) = provider::Gemini::from_env("x") {
        if let Ok(list) = g.list_models().await {
            models = list.into_iter().map(|m| format!("gemini:{m}")).collect();
        }
    }
    let d = st.default_model.clone();
    if !models.contains(&d) {
        models.insert(0, d.clone());
    }
    models.push("mock:demo".into());
    Json(json!({
        "default_model": d,
        "models": models,
        "providers": provider::configured(),
        "spec": aeoi_core::SPEC,
        "tools": st.bridge.tools.iter().map(|t| json!({"name": t.name, "operator_id": t.operator_id})).collect::<Vec<_>>(),
        "max_steps": AgentCfg::default().max_steps,
        "system_prompt": agent::SYSTEM_PROMPT,
    }))
}

async fn tools(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!(st.bridge.tools.iter().map(|t| json!({"name": t.name, "operator_id": t.operator_id, "description": t.description, "json_schema": t.json_schema, "gemini_schema": t.gemini_schema, "lossy_mappings": t.losses.iter().map(|l| json!({"path": l.path, "what": l.what})).collect::<Vec<_>>()})).collect::<Vec<_>>()))
}

#[derive(serde::Deserialize)]
struct ChatReq {
    session_id: String,
    model: Option<String>,
    message: String,
    max_steps: Option<usize>,
}

async fn chat(State(st): State<Arc<AppState>>, Json(req): Json<ChatReq>) -> Response {
    let model = req.model.clone().unwrap_or_else(|| st.default_model.clone());
    let prov = match provider::make(&model) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let st2 = st.clone();
    tokio::spawn(async move {
        // take the session history; a model switch starts a fresh history (provider-native parts differ)
        let mut hist = {
            let mut s = st2.sessions.lock().unwrap();
            match s.remove(&req.session_id) {
                Some((m, h)) if m == model => h,
                _ => vec![],
            }
        };
        let cfg = AgentCfg { max_steps: req.max_steps.unwrap_or(8).clamp(1, 20), ..Default::default() };
        let _ = tx.send(json!({"type": "start", "model": model, "history_len": hist.len()}));
        agent::run(prov.as_ref(), &st2.bridge, &mut hist, &req.message, &cfg, Some(tx)).await;
        st2.sessions.lock().unwrap().insert(req.session_id.clone(), (model, hist));
    });
    let stream = UnboundedReceiverStream::new(rx).map(|v| Ok::<Event, Infallible>(Event::default().data(v.to_string())));
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[derive(serde::Deserialize)]
struct ResetReq {
    session_id: String,
}

async fn reset(State(st): State<Arc<AppState>>, Json(r): Json<ResetReq>) -> Json<Value> {
    st.sessions.lock().unwrap().remove(&r.session_id);
    Json(json!({"ok": true}))
}

#[derive(serde::Deserialize)]
struct CallReq {
    tool: String,
    args: Value,
}

/// Direct operator call without the LLM (UI "re-run" button / debugging).
async fn direct_call(State(st): State<Arc<AppState>>, Json(r): Json<CallReq>) -> Json<Value> {
    let (env, ms) = st.bridge.call(&r.tool, &r.args);
    Json(json!({"response": env, "latency_ms": ms}))
}

pub async fn serve(addr: &str, default_model: String) -> std::io::Result<()> {
    let state = Arc::new(AppState { bridge: Bridge::new(), default_model, sessions: Mutex::new(HashMap::new()) });
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("AEOI web UI: http://{addr}/   (Ctrl+C 退出)");
    axum::serve(listener, router(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn http_endpoints_smoke() {
        let state = Arc::new(AppState { bridge: Bridge::new(), default_model: "gemini:gemini-2.5-flash".into(), sessions: Mutex::new(HashMap::new()) });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
        let c = reqwest::Client::new();
        let html = c.get(format!("http://{addr}/")).send().await.unwrap().text().await.unwrap();
        assert!(html.contains("AEOI"));
        let js = c.get(format!("http://{addr}/app.js")).send().await.unwrap();
        assert_eq!(js.status(), 200);
        let tools: Value = c.get(format!("http://{addr}/api/tools")).send().await.unwrap().json().await.unwrap();
        assert_eq!(tools.as_array().unwrap().len(), 12);
        let r: Value = c.post(format!("http://{addr}/api/call")).json(&json!({"tool":"control_lti_poles_zeros","args":{"system":{"kind":"tf","num":[1],"den":[1,3,2],"ts":0}}})).send().await.unwrap().json().await.unwrap();
        assert_eq!(r["response"]["status"], "ok", "{r}");
        // offline demo provider streams the full event sequence
        let sse = c.post(format!("http://{addr}/api/chat")).json(&json!({"session_id":"t","model":"mock:demo","message":"演示"})).send().await.unwrap().text().await.unwrap();
        for ev in ["\"type\":\"start\"", "\"type\":\"tool\"", "\"type\":\"final\"", "\"type\":\"done\"", "nested_array_to_dense"] {
            assert!(sse.contains(ev), "missing {ev} in SSE stream");
        }
        assert!(sse.contains("\"self_corrections\":1"));
        // unknown provider → 400 JSON, not a hang
        let bad = c.post(format!("http://{addr}/api/chat")).json(&json!({"session_id":"s","model":"groq:x","message":"hi"})).send().await.unwrap();
        if std::env::var("GROQ_API_KEY").is_err() {
            assert_eq!(bad.status(), 400);
        }
    }
}
