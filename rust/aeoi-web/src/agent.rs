//! Agent loop: model → tool call(s) → AEOI envelope (success or structured error with remediation)
//! → model … until the model answers in text or `max_steps` model turns are used.
use crate::provider::{Msg, Provider, Usage};
use crate::tools::{compact_for_model, is_error, Bridge};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

pub const SYSTEM_PROMPT: &str = include_str!("system_prompt.zh.md");

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolStep {
    pub step: usize,
    pub tool: String,
    pub operator_id: Option<String>,
    pub call_id: String,
    pub request: Value,
    /// Full AEOI envelope (UI); the model received `compact_for_model` of it.
    pub response: Value,
    pub is_error: bool,
    pub error_code: Option<String>,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelTurn {
    pub step: usize,
    pub text: Option<String>,
    pub tool_calls: Vec<String>,
    pub latency_ms: f64,
    pub usage: Usage,
    pub model_version: Option<String>,
    pub finish_reason: Option<String>,
    pub http_retries: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Transcript {
    pub provider: String,
    pub model: String,
    pub question: String,
    pub final_text: Option<String>,
    pub turns: Vec<ModelTurn>,
    pub tools: Vec<ToolStep>,
    pub stopped: String,
    pub self_corrections: usize,
    /// Empty model turns answered with a corrective nudge (reason, e.g. MALFORMED_FUNCTION_CALL).
    pub nudges: Vec<String>,
    pub total_tokens: u64,
    pub error: Option<String>,
}

pub struct AgentCfg {
    pub max_steps: usize,
    pub max_array_for_model: usize,
}

impl Default for AgentCfg {
    fn default() -> Self {
        AgentCfg { max_steps: 8, max_array_for_model: 120 }
    }
}

fn emit(tx: &Option<UnboundedSender<Value>>, ev: Value) {
    if let Some(t) = tx {
        let _ = t.send(ev);
    }
}

/// A self-correction = a tool call that returned an error, later followed by a successful call of
/// the same tool in the same run.
pub fn count_self_corrections(steps: &[ToolStep]) -> usize {
    let mut n = 0;
    for (i, s) in steps.iter().enumerate() {
        if s.is_error && steps[i + 1..].iter().any(|t| t.tool == s.tool && !t.is_error) {
            // count only the last error before a success, per tool
            if !steps[i + 1..].iter().take_while(|t| !(t.tool == s.tool && !t.is_error)).any(|t| t.tool == s.tool && t.is_error) {
                n += 1;
            }
        }
    }
    n
}

/// Marker of harness-generated user turns (nudges); they never count as the user's own words.
pub const NUDGE_PREFIX: &str = "（系统提示）";

/// Everything the user actually said in this conversation (for the M-5 grounding check).
pub fn user_text(history: &[Msg]) -> String {
    history.iter().filter_map(|m| if let Msg::User(t) = m { (!t.starts_with(NUDGE_PREFIX)).then_some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

/// Runs one user message on top of `history` (which is extended in place).
pub async fn run(provider: &dyn Provider, bridge: &Bridge, history: &mut Vec<Msg>, question: &str, cfg: &AgentCfg, tx: Option<UnboundedSender<Value>>) -> Transcript {
    let mut tr = Transcript { provider: provider.name().into(), model: provider.model().into(), question: question.into(), ..Default::default() };
    history.push(Msg::User(question.to_string()));
    let mut step = 0;
    let mut nudges = 0;
    loop {
        if step >= cfg.max_steps {
            tr.stopped = "max_steps".into();
            emit(&tx, json!({"type": "stopped", "reason": "max_steps", "max_steps": cfg.max_steps}));
            break;
        }
        step += 1;
        emit(&tx, json!({"type": "thinking", "step": step}));
        let t0 = Instant::now();
        let resp = match provider.chat(SYSTEM_PROMPT, history, &bridge.tools).await {
            Ok(r) => r,
            Err(e) => {
                tr.error = Some(e.clone());
                tr.stopped = "provider_error".into();
                emit(&tx, json!({"type": "error", "step": step, "message": e}));
                break;
            }
        };
        let turn = ModelTurn {
            step,
            text: resp.text.clone(),
            tool_calls: resp.calls.iter().map(|c| c.name.clone()).collect(),
            latency_ms: t0.elapsed().as_secs_f64() * 1e3,
            usage: resp.usage.clone(),
            model_version: resp.model_version.clone(),
            finish_reason: resp.finish_reason.clone(),
            http_retries: resp.retries,
        };
        tr.total_tokens += resp.usage.total_tokens.unwrap_or(0);
        emit(&tx, json!({"type": "model", "turn": turn}));
        tr.turns.push(turn);
        if resp.calls.is_empty() && resp.text.is_none() && nudges < 2 {
            // Empty turn — typically Gemini `MALFORMED_FUNCTION_CALL` (the model tried to emit a call
            // that does not fit the declared parameters). Tell it so and let it try again.
            nudges += 1;
            let why = resp.finish_reason.clone().unwrap_or_else(|| "EMPTY".into());
            let nudge = format!("{NUDGE_PREFIX}上一轮没有产生有效输出（{why}）。如果要调用工具，请严格按该工具的 parameters 构造参数：矩阵写成 {{\"dtype\":\"f64\",\"shape\":[行,列],\"data\":[...]}}，必填字段齐全；如果缺少必要信息，请直接用中文向用户提问。");
            emit(&tx, json!({"type": "nudge", "step": step, "reason": why}));
            tr.nudges.push(why);
            history.push(Msg::User(nudge));
            continue;
        }
        history.push(Msg::Assistant { text: resp.text.clone(), calls: resp.calls.clone(), raw: resp.raw.clone() });
        if resp.calls.is_empty() {
            tr.final_text = resp.text.clone();
            tr.stopped = "answered".into();
            emit(&tx, json!({"type": "final", "text": resp.text}));
            break;
        }
        for c in &resp.calls {
            let (env, ms) = {
                let b = bridge;
                // operators are CPU-bound but fast (ms); run inline to keep the borrow simple
                b.call_grounded(&c.name, &c.args, Some(&user_text(history)))
            };
            let err = is_error(&env);
            let ts = ToolStep {
                step,
                tool: c.name.clone(),
                operator_id: bridge.tool(&c.name).map(|t| t.operator_id.clone()),
                call_id: c.id.clone(),
                request: c.args.clone(),
                response: env.clone(),
                is_error: err,
                error_code: env["error"]["code"].as_str().map(|s| s.to_string()),
                latency_ms: ms,
            };
            emit(&tx, json!({"type": "tool", "step": ts}));
            tr.tools.push(ts);
            history.push(Msg::Tool { call_id: c.id.clone(), name: c.name.clone(), response: compact_for_model(&env, cfg.max_array_for_model) });
        }
    }
    tr.self_corrections = count_self_corrections(&tr.tools);
    emit(&tx, json!({"type": "done", "self_corrections": tr.self_corrections, "total_tokens": tr.total_tokens, "stopped": tr.stopped}));
    tr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockProvider;

    fn lqr_args(b_data: Value, b_shape: Value) -> Value {
        json!({"A":{"dtype":"f64","shape":[2,2],"data":[0,1,0,0]},"B":{"dtype":"f64","shape":b_shape,"data":b_data},"Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},"R":{"dtype":"f64","shape":[1,1],"data":[1]}})
    }

    #[tokio::test]
    async fn error_is_fed_back_then_self_corrected() {
        let mock = MockProvider::new(vec![
            MockProvider::calls(vec![("control_design_lqr", lqr_args(json!([0, 1, 5]), json!([2, 1])))]),
            MockProvider::calls(vec![("control_design_lqr", lqr_args(json!([0, 1]), json!([2, 1])))]),
            MockProvider::text("K = [1, 1.732]"),
        ]);
        let bridge = Bridge::new();
        let mut h = vec![];
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let tr = run(&mock, &bridge, &mut h, "求 LQR 增益", &AgentCfg::default(), Some(tx)).await;
        assert_eq!(tr.stopped, "answered");
        assert_eq!(tr.tools.len(), 2);
        assert!(tr.tools[0].is_error);
        assert!(!tr.tools[1].is_error);
        assert_eq!(tr.self_corrections, 1);
        assert_eq!(tr.final_text.as_deref(), Some("K = [1, 1.732]"));
        // the error envelope (with remediation) reached the model on turn 2
        let seen = mock.last_tool_responses.lock().unwrap().clone();
        assert!(!seen.is_empty());
        let mut events = vec![];
        while let Ok(e) = rx.try_recv() {
            events.push(e["type"].as_str().unwrap().to_string());
        }
        assert_eq!(events.iter().filter(|e| *e == "tool").count(), 2);
        assert_eq!(events.last().unwrap(), "done");
        assert_eq!(h.len(), 6);
    }

    #[tokio::test]
    async fn error_envelope_reaches_model_with_remediation() {
        let mock = MockProvider::new(vec![MockProvider::calls(vec![("control_lti_c2d", json!({"system":{"kind":"tf","num":[1],"den":[1,1],"ts":0},"method":"zoh"}))]), MockProvider::text("缺少采样周期")]);
        let bridge = Bridge::new();
        let mut h = vec![];
        let tr = run(&mock, &bridge, &mut h, "离散化", &AgentCfg::default(), None).await;
        assert!(tr.tools[0].is_error);
        let seen = mock.last_tool_responses.lock().unwrap().clone();
        assert_eq!(seen[0]["status"], "error");
        assert!(seen[0]["error"]["remediation"].as_array().map(|a| !a.is_empty()).unwrap_or(false));
        assert_eq!(tr.self_corrections, 0);
    }

    #[tokio::test]
    async fn max_steps_stops_loop() {
        let turns = (0..5).map(|_| MockProvider::calls(vec![("control_lti_poles_zeros", json!({"system":{"kind":"tf","num":[1],"den":[1,1],"ts":0}}))])).collect();
        let mock = MockProvider::new(turns);
        let bridge = Bridge::new();
        let mut h = vec![];
        let tr = run(&mock, &bridge, &mut h, "x", &AgentCfg { max_steps: 3, ..Default::default() }, None).await;
        assert_eq!(tr.stopped, "max_steps");
        assert_eq!(tr.turns.len(), 3);
    }

    #[tokio::test]
    async fn empty_turn_gets_nudged() {
        let mut empty = MockProvider::text("x");
        empty.text = None;
        empty.finish_reason = Some("MALFORMED_FUNCTION_CALL".into());
        let mock = MockProvider::new(vec![empty, MockProvider::text("请提供 C 矩阵")]);
        let bridge = Bridge::new();
        let mut h = vec![];
        let tr = run(&mock, &bridge, &mut h, "q", &AgentCfg::default(), None).await;
        assert_eq!(tr.stopped, "answered");
        assert_eq!(tr.nudges, vec!["MALFORMED_FUNCTION_CALL".to_string()]);
        assert_eq!(tr.final_text.as_deref(), Some("请提供 C 矩阵"));
        assert!(matches!(&h[1], Msg::User(t) if t.contains("MALFORMED_FUNCTION_CALL")));
    }

    #[test]
    fn self_correction_counting() {
        let mk = |tool: &str, e: bool| ToolStep { step: 0, tool: tool.into(), operator_id: None, call_id: String::new(), request: Value::Null, response: Value::Null, is_error: e, error_code: None, latency_ms: 0.0 };
        assert_eq!(count_self_corrections(&[mk("a", true), mk("a", true), mk("a", false)]), 1);
        assert_eq!(count_self_corrections(&[mk("a", true), mk("b", false)]), 0);
        assert_eq!(count_self_corrections(&[mk("a", true), mk("a", false), mk("b", true), mk("b", false)]), 2);
    }
}
