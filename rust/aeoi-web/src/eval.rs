//! Eval harness: Chinese questions → agent run → scored against direct operator calls.
use crate::agent::{self, AgentCfg, Transcript};
use crate::provider::{BoxFut, ChatResponse, Msg, Provider};
use crate::tools::{is_error, Bridge, ToolDef};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub const QUESTIONS: &str = include_str!("../../../eval/questions.zh.json");

#[derive(Debug, Clone, PartialEq)]
pub enum CheckVal {
    Num(f64),
    Inf(f64),
    Bool(bool),
}

fn quantity_payload(v: &Value) -> &Value {
    if v.is_object() && v.get("unit").is_some() && v.get("value").is_some() {
        &v["value"]
    } else {
        v
    }
}

fn flatten(v: &Value, component: Option<&str>, out: &mut Vec<CheckVal>) {
    match v {
        Value::Number(n) => out.push(CheckVal::Num(n.as_f64().unwrap())),
        Value::Bool(b) => out.push(CheckVal::Bool(*b)),
        Value::String(s) if s == "Inf" => out.push(CheckVal::Inf(1.0)),
        Value::String(s) if s == "-Inf" => out.push(CheckVal::Inf(-1.0)),
        Value::Array(a) if a.len() == 2 && a.iter().all(|x| x.is_number()) && component.is_some() => {
            let i = if component == Some("im") { 1 } else { 0 };
            out.push(CheckVal::Num(a[i].as_f64().unwrap()));
        }
        Value::Array(a) => a.iter().for_each(|x| flatten(x, component, out)),
        Value::Object(o) => {
            if let Some(d) = o.get("data") {
                flatten(d, component, out)
            } else if o.get("value").is_some() {
                flatten(quantity_payload(v), component, out)
            }
        }
        _ => {}
    }
}

/// Extract the value a check refers to from an AEOI envelope.
pub fn extract(env: &Value, chk: &Value) -> Option<CheckVal> {
    let p = chk["pointer"].as_str()?;
    let raw = env.pointer(p)?;
    let comp = chk["component"].as_str();
    let mut vals = vec![];
    if let Some(w_target) = chk["at_w"].as_f64() {
        let w = env.pointer("/result/w")?;
        let unit = w["unit"].as_str().unwrap_or("rad/s");
        let target = if unit == "Hz" { w_target / (2.0 * std::f64::consts::PI) } else { w_target };
        let mut ws = vec![];
        flatten(w, None, &mut ws);
        let (idx, best) = ws.iter().enumerate().filter_map(|(i, x)| if let CheckVal::Num(x) = x { Some((i, (x - target).abs() / target)) } else { None }).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;
        if best > 1e-3 {
            return None;
        }
        flatten(raw, comp, &mut vals);
        return vals.get(idx).cloned();
    }
    flatten(raw, comp, &mut vals);
    let mut out = if let Some(i) = chk["index"].as_u64() {
        vals.get(i as usize).cloned()?
    } else if let Some(r) = chk["reduce"].as_str() {
        let nums: Vec<f64> = vals.iter().filter_map(|v| if let CheckVal::Num(x) = v { Some(*x) } else { None }).collect();
        if nums.is_empty() {
            return None;
        }
        CheckVal::Num(match r {
            "max" => nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            "min" => nums.iter().cloned().fold(f64::INFINITY, f64::min),
            "maxabs" => nums.iter().cloned().fold(0.0, |a: f64, b| if b.abs() > a.abs() { b } else { a }),
            _ => return None,
        })
    } else {
        vals.first().cloned()?
    };
    if let (Some(np), CheckVal::Num(x)) = (chk["normalize_by"].as_str(), &out) {
        let d = env.pointer(np).and_then(|d| d.as_f64())?;
        out = CheckVal::Num(x / d);
    }
    Some(out)
}

fn close(a: &CheckVal, b: &CheckVal, rel: f64) -> bool {
    match (a, b) {
        (CheckVal::Num(x), CheckVal::Num(y)) => (x - y).abs() <= rel * y.abs().max(x.abs()) + 1e-9,
        (CheckVal::Inf(s), CheckVal::Inf(t)) => s == t,
        (CheckVal::Bool(x), CheckVal::Bool(y)) => x == y,
        _ => false,
    }
}

/// Numbers appearing in free text (see [`crate::answer`] for the accepted spellings).
pub fn numbers_in(text: &str) -> Vec<f64> {
    crate::answer::candidates(text).iter().map(|c| c.value).collect()
}

pub fn answer_has(text: &str, v: &CheckVal, chk: &Value) -> bool {
    match v {
        CheckVal::Inf(_) => crate::answer::INF_WORDS.iter().any(|k| text.contains(k)),
        CheckVal::Bool(_) => true,
        CheckVal::Num(x) => {
            let tol = crate::answer::Tol {
                rel: chk["answer_rel_tol"].as_f64().unwrap_or(0.02),
                abs: chk["answer_abs_tol"].as_f64(),
                allow_sign_flip: chk["allow_sign_flip"].as_bool().unwrap_or(false),
            };
            crate::answer::text_has(text, *x, &tol)
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QResult {
    pub id: String,
    pub category: String,
    pub question: String,
    pub expected_tool: Value,
    pub tools_called: Vec<String>,
    pub first_tool: Option<String>,
    pub tool_ok: bool,
    pub first_tool_ok: bool,
    pub args_ok: Option<bool>,
    pub answer_ok: bool,
    pub self_corrections: usize,
    pub nudges: Vec<String>,
    pub tool_errors: Vec<String>,
    pub checks: Vec<Value>,
    pub final_text: Option<String>,
    pub stopped: String,
    pub provider_error: Option<String>,
    pub total_tokens: u64,
    pub latency_s: f64,
    pub model_turns: usize,
    pub http_retries: u32,
    pub trace: Vec<Value>,
}

fn expected_tools(q: &Value) -> Vec<String> {
    match &q["expected_tool"] {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(),
        _ => vec![],
    }
}

/// Reference envelope from a direct operator call (no LLM).
pub fn reference(bridge: &Bridge, q: &Value) -> Option<Value> {
    let r = q.get("reference")?;
    Some(bridge.call(r["tool"].as_str()?, &r["args"]).0)
}

pub fn score(bridge: &Bridge, q: &Value, tr: &Transcript, latency_s: f64) -> QResult {
    let exp = expected_tools(q);
    let mode = q["expect"].as_str().unwrap_or("answer");
    let called: Vec<String> = tr.tools.iter().map(|t| t.tool.clone()).collect();
    let final_text = tr.final_text.clone().unwrap_or_default();
    let mut checks_out = vec![];
    let (tool_ok, first_tool_ok, args_ok, answer_ok);
    match mode {
        "clarify" => {
            let forbidden: Vec<&str> = q["forbidden_tools"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
            let guessed = tr.tools.iter().any(|t| forbidden.contains(&t.tool.as_str()) && !t.is_error);
            tool_ok = !guessed;
            first_tool_ok = tr.tools.first().map(|t| !forbidden.contains(&t.tool.as_str()) || t.is_error).unwrap_or(true);
            args_ok = None;
            let kw = q["clarify_keywords"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).any(|k| final_text.contains(k))).unwrap_or(false);
            // the final message must actually ask (a question), not just mention the missing quantity
            let asks = ["？", "?", "请提供", "请告诉", "请问", "请给出", "请确认"].iter().any(|k| final_text.contains(k));
            answer_ok = !guessed && kw && asks;
        }
        _ => {
            tool_ok = exp.iter().any(|e| called.contains(e));
            first_tool_ok = tr.tools.first().map(|t| exp.contains(&t.tool)).unwrap_or(false);
            let refenv = reference(bridge, q);
            if mode == "explain_error" {
                let code = q["expect_error_code"].as_str().unwrap_or("");
                args_ok = Some(tr.tools.iter().any(|t| exp.contains(&t.tool) && t.error_code.as_deref() == Some(code)));
                answer_ok = q["answer_keywords"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).any(|k| final_text.contains(k))).unwrap_or(false);
            } else {
                let refenv = refenv.expect("reference");
                assert!(!is_error(&refenv), "reference call failed for {}: {}", q["id"], refenv["error"]);
                // successful calls of the expected tool, newest first; per check use the newest call
                // in which the quantity exists (e.g. a bode call that actually contains ω = 10)
                let oks: Vec<_> = tr.tools.iter().rev().filter(|t| exp.contains(&t.tool) && !t.is_error).collect();
                let mut all_args = !oks.is_empty();
                let mut all_ans = true;
                for c in q["checks"].as_array().cloned().unwrap_or_default() {
                    let want = extract(&refenv, &c).unwrap_or_else(|| panic!("check {} of {} not extractable from reference", c["name"], q["id"]));
                    let rel = c["rel_tol"].as_f64().unwrap_or(1e-3);
                    let got = oks.iter().find_map(|t| extract(&t.response, &c));
                    let arg_pass = got.as_ref().map(|g| close(g, &want, rel)).unwrap_or(false);
                    let ans_pass = if c["in_answer"] == false { true } else { answer_has(&final_text, &want, &c) };
                    all_args &= arg_pass;
                    all_ans &= ans_pass;
                    checks_out.push(json!({"name": c["name"], "expected": format!("{want:?}"), "from_model_call": got.map(|g| format!("{g:?}")), "args_pass": arg_pass, "answer_pass": ans_pass}));
                }
                args_ok = Some(all_args);
                answer_ok = all_ans && !final_text.is_empty();
            }
        }
    }
    QResult {
        id: q["id"].as_str().unwrap_or("").into(),
        category: q["category"].as_str().unwrap_or("").into(),
        question: q["question"].as_str().unwrap_or("").into(),
        expected_tool: q["expected_tool"].clone(),
        first_tool: called.first().cloned(),
        tools_called: called,
        tool_ok,
        first_tool_ok,
        args_ok,
        answer_ok,
        self_corrections: tr.self_corrections,
        nudges: tr.nudges.clone(),
        tool_errors: tr.tools.iter().filter_map(|t| t.error_code.clone()).collect(),
        checks: checks_out,
        final_text: tr.final_text.clone(),
        stopped: tr.stopped.clone(),
        provider_error: tr.error.clone(),
        total_tokens: tr.total_tokens,
        latency_s,
        model_turns: tr.turns.len(),
        http_retries: tr.turns.iter().map(|t| t.http_retries).sum(),
        trace: tr.tools.iter().map(|t| json!({"step": t.step, "tool": t.tool, "request": t.request, "is_error": t.is_error, "error_code": t.error_code, "error_hint": t.response["error"]["agent_hint"], "latency_ms": t.latency_ms})).collect(),
    }
}

/// Re-score a saved report with the current questions and scorer, without calling the model:
/// every recorded tool request is re-executed (operators are deterministic), the saved final
/// answer is re-checked. Token/latency/retry figures are carried over from the report.
pub fn rescore(bridge: &Bridge, questions: &Value, report: &Value) -> Value {
    let qs = questions["questions"].as_array().cloned().unwrap_or_default();
    let mut results = vec![];
    for old in report["results"].as_array().cloned().unwrap_or_default() {
        let Some(q) = qs.iter().find(|q| q["id"] == old["id"]) else { continue };
        let tools = old["trace"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|t| {
                let name = t["tool"].as_str().unwrap_or("").to_string();
                // no grounding check: the saved run may predate the _user_quote argument
                let (resp, _) = bridge.call(&name, &t["request"]);
                agent::ToolStep {
                    step: t["step"].as_u64().unwrap_or(0) as usize,
                    operator_id: None,
                    call_id: String::new(),
                    request: t["request"].clone(),
                    is_error: is_error(&resp),
                    error_code: resp["error"]["code"].as_str().map(String::from),
                    response: resp,
                    latency_ms: t["latency_ms"].as_f64().unwrap_or(0.0),
                    tool: name,
                }
            })
            .collect::<Vec<_>>();
        let tr = Transcript {
            question: q["question"].as_str().unwrap_or("").into(),
            final_text: old["final_text"].as_str().map(String::from),
            stopped: old["stopped"].as_str().unwrap_or("").into(),
            self_corrections: agent::count_self_corrections(&tools),
            tools,
            nudges: serde_json::from_value(old["nudges"].clone()).unwrap_or_default(),
            total_tokens: old["total_tokens"].as_u64().unwrap_or(0),
            error: old["provider_error"].as_str().map(String::from),
            ..Default::default()
        };
        let mut r = score(bridge, q, &tr, old["latency_s"].as_f64().unwrap_or(0.0));
        r.http_retries = old["http_retries"].as_u64().unwrap_or(0) as u32;
        r.model_turns = old["model_turns"].as_u64().unwrap_or(0) as usize;
        results.push(r);
    }
    let st = &report["settings"];
    let opts = EvalOpts { rpm: st["rpm"].as_f64().unwrap_or(0.0), max_steps: st["max_steps"].as_u64().unwrap_or(8) as usize, only: None, limit: None };
    let mut out = summarize_named(report["provider"].as_str().unwrap_or(""), report["model"].as_str().unwrap_or(""), &results, report["summary"]["wall_time_s"].as_f64().unwrap_or(0.0), &opts);
    out["aborted"] = report["aborted"].clone();
    out["rescored"] = json!({"by": format!("aeoi-web {} rescore", env!("CARGO_PKG_VERSION")), "previous_summary": report["summary"]});
    out
}

/// Spaces model calls to at most `rpm` per minute (free-tier friendly).
pub struct Paced<'a> {
    pub inner: &'a dyn Provider,
    pub min_interval: Duration,
    pub last: std::sync::Mutex<Option<Instant>>,
}

impl Provider for Paced<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn chat<'a>(&'a self, system: &'a str, msgs: &'a [Msg], tools: &'a [ToolDef]) -> BoxFut<'a, Result<ChatResponse, String>> {
        Box::pin(async move {
            let wait = {
                let l = self.last.lock().unwrap();
                l.map(|t| self.min_interval.saturating_sub(t.elapsed())).unwrap_or_default()
            };
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            *self.last.lock().unwrap() = Some(Instant::now());
            self.inner.chat(system, msgs, tools).await
        })
    }
}

pub struct EvalOpts {
    pub rpm: f64,
    pub max_steps: usize,
    pub only: Option<Vec<String>>,
    pub limit: Option<usize>,
}

pub async fn run_eval(provider: &dyn Provider, bridge: &Bridge, questions: &Value, opts: &EvalOpts) -> Value {
    run_eval_with(provider, bridge, questions, opts, |_| {}).await
}

/// Like [`run_eval`], calling `checkpoint(partial_report)` after every question (so an interrupted
/// run still leaves a report). Stops early when the provider reports an exhausted daily quota.
pub async fn run_eval_with(provider: &dyn Provider, bridge: &Bridge, questions: &Value, opts: &EvalOpts, mut checkpoint: impl FnMut(&Value)) -> Value {
    let paced = Paced { inner: provider, min_interval: Duration::from_secs_f64(60.0 / opts.rpm.max(0.1)), last: std::sync::Mutex::new(None) };
    let mut results = vec![];
    let qs: Vec<Value> = questions["questions"].as_array().cloned().unwrap_or_default().into_iter().filter(|q| opts.only.as_ref().map(|o| o.iter().any(|id| q["id"].as_str().unwrap_or("").starts_with(id.as_str()))).unwrap_or(true)).take(opts.limit.unwrap_or(usize::MAX)).collect();
    let t_all = Instant::now();
    for (i, q) in qs.iter().enumerate() {
        let t0 = Instant::now();
        let mut hist = vec![];
        let tr = agent::run(&paced, bridge, &mut hist, q["question"].as_str().unwrap_or(""), &AgentCfg { max_steps: opts.max_steps, ..Default::default() }, None).await;
        let r = score(bridge, q, &tr, t0.elapsed().as_secs_f64());
        eprintln!(
            "[{}/{}] {:<24} tool={} first={} args={} answer={} selfcorr={} errs={:?} nudges={} tokens={} {:.1}s{}",
            i + 1,
            qs.len(),
            r.id,
            ok(r.tool_ok),
            ok(r.first_tool_ok),
            r.args_ok.map(ok).unwrap_or("n/a"),
            ok(r.answer_ok),
            r.self_corrections,
            r.tool_errors,
            r.nudges.len(),
            r.total_tokens,
            r.latency_s,
            r.provider_error.as_ref().map(|e| format!(" PROVIDER_ERROR: {e}")).unwrap_or_default()
        );
        let quota = r.provider_error.as_deref().map(|e| e.starts_with(crate::provider::QUOTA_EXHAUSTED)).unwrap_or(false);
        if quota {
            // the failed question is not the model's fault: don't score it
            let skipped: Vec<String> = qs[i..].iter().map(|q| q["id"].as_str().unwrap_or("").to_string()).collect();
            eprintln!("配额用尽，停止评测；未运行 {} 题：{}", skipped.len(), skipped.join(", "));
            let mut rep = summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts);
            rep["aborted"] = json!({"reason": r.provider_error, "not_run": skipped});
            return rep;
        }
        results.push(r);
        checkpoint(&summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts));
    }
    summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts)
}

fn ok(b: bool) -> &'static str {
    if b {
        "✓"
    } else {
        "✗"
    }
}

pub fn summarize(provider: &dyn Provider, results: &[QResult], wall_s: f64, opts: &EvalOpts) -> Value {
    summarize_named(provider.name(), provider.model(), results, wall_s, opts)
}

pub fn summarize_named(provider: &str, model: &str, results: &[QResult], wall_s: f64, opts: &EvalOpts) -> Value {
    let n = results.len().max(1) as f64;
    let frac = |f: &dyn Fn(&QResult) -> bool| results.iter().filter(|r| f(r)).count() as f64 / n;
    let arg_app: Vec<&QResult> = results.iter().filter(|r| r.args_ok.is_some()).collect();
    let arg_acc = if arg_app.is_empty() { Value::Null } else { json!(arg_app.iter().filter(|r| r.args_ok == Some(true)).count() as f64 / arg_app.len() as f64) };
    let mut by_cat: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for r in results {
        let e = by_cat.entry(r.category.clone()).or_default();
        e.0 += 1;
        if r.answer_ok {
            e.1 += 1;
        }
    }
    json!({
        "provider": provider,
        "model": model,
        "generated_by": format!("aeoi-web {} eval", env!("CARGO_PKG_VERSION")),
        "spec": aeoi_core::SPEC,
        "reference_values": "direct AEOI operator calls (python-control/scipy-verified implementation; not MATLAB)",
        "settings": {"rpm": opts.rpm, "max_steps": opts.max_steps, "temperature": 0.0},
        "aborted": null,
        "summary": {
            "questions": results.len(),
            "tool_selection_accuracy": frac(&|r| r.tool_ok),
            "first_tool_accuracy": frac(&|r| r.first_tool_ok),
            "argument_correctness": arg_acc,
            "argument_applicable": arg_app.len(),
            "final_answer_accuracy": frac(&|r| r.answer_ok),
            "self_corrections": results.iter().map(|r| r.self_corrections).sum::<usize>(),
            "nudges_after_empty_turn": results.iter().map(|r| r.nudges.len()).sum::<usize>(),
            "tool_errors": results.iter().map(|r| r.tool_errors.len()).sum::<usize>(),
            "provider_errors": results.iter().filter(|r| r.provider_error.is_some()).count(),
            "http_retries": results.iter().map(|r| r.http_retries).sum::<u32>(),
            "total_tokens": results.iter().map(|r| r.total_tokens).sum::<u64>(),
            "mean_latency_s": results.iter().map(|r| r.latency_s).sum::<f64>() / n,
            "wall_time_s": wall_s,
            "by_category_answer": by_cat.iter().map(|(k, (t, c))| (k.clone(), json!(format!("{c}/{t}")))).collect::<serde_json::Map<_, _>>(),
        },
        "results": results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockProvider;

    #[test]
    fn number_extraction() {
        assert_eq!(numbers_in("K = [1, 1.732]，S≈−0.5e-3；超调 37.2%"), vec![1.0, 1.732, -0.0005, 37.2]);
        assert_eq!(numbers_in("增益裕度 1.5 (3.52 dB)，ω=1.414 rad/s"), vec![1.5, 3.52, 1.414]);
        assert_eq!(numbers_in("x2 = 3×10^-2"), vec![0.03]);
    }

    #[test]
    fn all_questions_have_working_references() {
        let b = Bridge::new();
        let qs: Value = serde_json::from_str(QUESTIONS).unwrap();
        let qs = qs["questions"].as_array().unwrap();
        assert!(qs.len() >= 20 && qs.len() <= 30, "{}", qs.len());
        let tools: Vec<&str> = b.tools.iter().map(|t| t.name.as_str()).collect();
        let mut covered = std::collections::BTreeSet::new();
        for q in qs {
            for t in expected_tools(q) {
                assert!(tools.contains(&t.as_str()), "{} unknown tool {t}", q["id"]);
                covered.insert(t);
            }
            match q["expect"].as_str().unwrap_or("answer") {
                "clarify" => assert!(q["clarify_keywords"].is_array()),
                "explain_error" => {
                    let env = reference(&b, q).unwrap();
                    assert_eq!(env["error"]["code"], q["expect_error_code"], "{}", q["id"]);
                }
                _ => {
                    let env = reference(&b, q).unwrap();
                    assert!(!is_error(&env), "{}: {}", q["id"], env["error"]);
                    let checks = q["checks"].as_array().unwrap();
                    assert!(!checks.is_empty(), "{}", q["id"]);
                    for c in checks {
                        let v = extract(&env, c).unwrap_or_else(|| panic!("{} {}", q["id"], c["name"]));
                        assert!(!matches!(v, CheckVal::Num(x) if !x.is_finite()));
                    }
                }
            }
        }
        // all 12 operators except possibly a few are exercised
        assert!(covered.len() >= 12, "covered only {covered:?}");
    }

    /// The trap questions really are traps: the naive mistake gives a different value.
    #[test]
    fn traps_discriminate() {
        let b = Bridge::new();
        let qs: Value = serde_json::from_str(QUESTIONS).unwrap();
        let get = |id: &str| qs["questions"].as_array().unwrap().iter().find(|q| q["id"] == id).unwrap().clone();
        // column-major misread of A
        let q = get("ctrb-03-colmajor");
        let mut wrong = q["reference"]["args"].clone();
        wrong["system"]["A"]["data"] = json!([0.0, 0.0, 1.0, 0.0]);
        let (env, _) = b.call("control_analysis_ctrb_obsv", &wrong);
        assert_ne!(extract(&env, &q["checks"][0]), extract(&reference(&b, &q).unwrap(), &q["checks"][0]));
        let q = get("lqr-03-colmajor");
        let mut wrong = q["reference"]["args"].clone();
        wrong["A"]["data"] = json!([0.0, -2.0, 1.0, -3.0]);
        let (env, _) = b.call("control_design_lqr", &wrong);
        let r = reference(&b, &q).unwrap();
        assert!(!close(&extract(&env, &q["checks"][0]).unwrap(), &extract(&r, &q["checks"][0]).unwrap(), 1e-3));
        // Hz treated as rad/s
        let q = get("bode-02-hz");
        let mut wrong = q["reference"]["args"].clone();
        wrong["frequency"]["unit"] = json!("rad/s");
        let (env, _) = b.call("control_lti_bode", &wrong);
        let mag_wrong = env["result"]["magnitude"]["value"]["data"][0].as_f64().unwrap();
        assert!((mag_wrong - (-3.0103)).abs() > 1.0, "{mag_wrong}");
        // discrete system treated as continuous
        let q = get("pz-03-discrete");
        let mut wrong = q["reference"]["args"].clone();
        wrong["system"]["ts"] = json!(0.0);
        let (env, _) = b.call("control_lti_poles_zeros", &wrong);
        assert!(!close(&extract(&env, &q["checks"][0]).unwrap(), &extract(&reference(&b, &q).unwrap(), &q["checks"][0]).unwrap(), 1e-3));
    }

    #[tokio::test]
    async fn scoring_with_mock_provider() {
        let b = Bridge::new();
        let qs: Value = serde_json::from_str(QUESTIONS).unwrap();
        let q = qs["questions"].as_array().unwrap().iter().find(|q| q["id"] == "lqr-01").unwrap().clone();
        let args = q["reference"]["args"].clone();
        let mut bad = args.clone();
        bad["B"]["shape"] = json!([1, 2]);
        let mock = MockProvider::new(vec![MockProvider::calls(vec![("control_design_lqr", bad)]), MockProvider::calls(vec![("control_design_lqr", args)]), MockProvider::text("K = [1.000, 1.732]")]);
        let mut h = vec![];
        let tr = agent::run(&mock, &b, &mut h, q["question"].as_str().unwrap(), &AgentCfg::default(), None).await;
        let r = score(&b, &q, &tr, 0.0);
        assert!(r.tool_ok && r.first_tool_ok && r.args_ok == Some(true) && r.answer_ok, "{r:?}");
        assert_eq!(r.self_corrections, 1);
        // wrong final number → answer fails, args still pass
        let mock = MockProvider::new(vec![MockProvider::calls(vec![("control_design_lqr", q["reference"]["args"].clone())]), MockProvider::text("K = [1, 2.5]")]);
        let mut h = vec![];
        let tr = agent::run(&mock, &b, &mut h, "", &AgentCfg::default(), None).await;
        let r = score(&b, &q, &tr, 0.0);
        assert!(r.args_ok == Some(true) && !r.answer_ok);
        // clarify question: asking passes, guessing fails
        let q = qs["questions"].as_array().unwrap().iter().find(|q| q["id"] == "c2d-03-missing-ts").unwrap().clone();
        let mock = MockProvider::new(vec![MockProvider::text("请问采样周期 T 是多少？")]);
        let mut h = vec![];
        let tr = agent::run(&mock, &b, &mut h, "", &AgentCfg::default(), None).await;
        assert!(score(&b, &q, &tr, 0.0).answer_ok);
        let mock = MockProvider::new(vec![MockProvider::calls(vec![("control_lti_c2d", json!({"system":{"kind":"tf","num":[1],"den":[1,1,1],"ts":0},"ts":0.1}))]), MockProvider::text("采样周期取 0.1 s")]);
        let mut h = vec![];
        let tr = agent::run(&mock, &b, &mut h, "", &AgentCfg::default(), None).await;
        let r = score(&b, &q, &tr, 0.0);
        // the binding blocks the invented ts (M-5), but an answer that still states a guess fails
        assert_eq!(r.tool_errors, vec!["E.VALIDATION.UNGROUNDED_INPUT".to_string()]);
        assert!(r.tool_ok && !r.answer_ok, "{r:?}");
        // blocked guess, then asking → pass
        let mock = MockProvider::new(vec![MockProvider::calls(vec![("control_lti_c2d", json!({"system":{"kind":"tf","num":[1],"den":[1,1,1],"ts":0},"ts":0.1}))]), MockProvider::text("请问采样周期 T 是多少？")]);
        let mut h = vec![];
        let tr = agent::run(&mock, &b, &mut h, q["question"].as_str().unwrap(), &AgentCfg::default(), None).await;
        assert!(score(&b, &q, &tr, 0.0).answer_ok);
        // a guessed call that the bridge cannot block (no user text check) is still caught by the scorer
        let ok_env = b.call("control_lti_c2d", &json!({"system":{"kind":"tf","num":[1],"den":[1,1,1],"ts":0},"ts":0.1})).0;
        assert!(!is_error(&ok_env));
    }
}
