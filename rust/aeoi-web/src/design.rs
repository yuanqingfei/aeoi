//! Design-task eval: the model gets an engineering design brief, iterates with the AEOI analysis
//! operators (step / margin / bode / poles …), then calls `submit_design`; the submission is scored
//! by our own re-implementation of the task's evaluator on AEOI operators.
//!
//! Tasks: EngDesign control / DSP tasks vendored with attribution (MIT, `eval/design/engdesign.json`)
//! and ControlEval tasks fetched at run time (no upstream license → not vendored; see
//! `eval/design/fetch_controleval.py`). `eval/design/pyscore.py` re-scores every submission with
//! python-control / scipy as a cross-check.
use crate::agent::{count_self_corrections, ModelTurn, ToolStep};
use crate::eval::Paced;
use crate::provider::{Msg, Provider};
use crate::tools::{compact_for_model, is_error, Bridge, ToolDef};
use aeoi_core::poly::{polyadd_scaled, polymul, polyval, strip};
use aeoi_core::lti::C64;
use serde_json::{json, Value};
use std::f64::consts::PI;
use std::time::{Duration, Instant};

pub const ENGDESIGN: &str = include_str!("../../../eval/design/engdesign.json");
pub const SUBMIT: &str = "submit_design";

pub const DESIGN_ADDENDUM: &str = "\n\n## 设计评测模式\n\
这是一次自动化的工程设计评测：没有用户可以追问，题目里的数值就是全部已知条件，不要提问。\n\
流程：先给出设计，用工具把对象和控制器接起来再检查，不满足就修改，满足后调用 submit_design 提交。\
只能提交一次。\n\
传递函数运算用工具，不要手算多项式：\
control_design_pid 由 Kp、Ki、Kd（parallel）或 Kp、Ti、Td（standard）得到 C(s)，Tf 省略就是不加滤波；\
control_lti_series 是 sys2·sys1（把对象接到控制器后面或反过来，看谁的输出进谁）；\
control_lti_parallel 是相加；control_lti_feedback 默认 sign=-1 的负反馈，省略 sys2 就是单位反馈。\
把上一个工具返回的 result.system 原样作为下一个的 sys1 或 sys2。\
指标用 control_lti_stepinfo（10%–90% 上升、2% 调节带是定义默认值，不用改），裕度仍用 control_analysis_margin，\
频响用 control_lti_bode，极点用 control_lti_poles_zeros。\
时滞用 control_lti_pade：delay 填题目给出或由题目算出的秒数，order 题目没写时用 10 并在说明里声明（这是本评测提示的约定，不是算子默认值）。\n\
数值请直接写成数字（不要写表达式）。";

#[derive(Debug, Clone, serde::Serialize)]
pub struct DesignResult {
    pub id: String,
    pub benchmark: String,
    pub scorer: String,
    pub submitted: bool,
    pub submission: Option<Value>,
    pub pass: bool,
    pub score: Option<f64>,
    pub details: Value,
    pub expected_blockers: Vec<String>,
    pub tool_calls: Vec<String>,
    pub tool_errors: Vec<String>,
    pub self_corrections: usize,
    pub model_turns: usize,
    pub stopped: String,
    pub final_text: Option<String>,
    pub provider_error: Option<String>,
    pub total_tokens: u64,
    pub latency_s: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controleval_task: Option<Value>,
    pub transcript_tools: Vec<ToolStep>,
}

// ------------------------------------------------------------------------------------------ tasks

/// Vendored EngDesign tasks, as `{id, prompt, fields, scorer, …}`.
pub fn engdesign_tasks() -> Vec<Value> {
    let d: Value = serde_json::from_str(ENGDESIGN).expect("engdesign.json");
    d["tasks"].as_array().unwrap().iter().map(|t| {
        let mut t = t.clone();
        t["benchmark"] = json!("EngDesign");
        t
    }).collect()
}

/// ControlEval tasks from the runtime download (`fetch_controleval.py`): `per_category` tasks of each
/// listed category (all categories when `categories` is empty), starting at index `offset`.
pub fn controleval_tasks(path: &str, per_category: usize, offset: usize, categories: &[String]) -> Result<Vec<Value>, String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}（先运行 python3 eval/design/fetch_controleval.py）"))?;
    let d: Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    let src = d["source"].clone();
    let mut out = vec![];
    for t in d["tasks"].as_array().ok_or("tasks")? {
        let cat = t["category"].as_str().unwrap_or("");
        let idx = t["index"].as_u64().unwrap_or(0) as usize;
        if (!categories.is_empty() && !categories.iter().any(|c| c == cat)) || idx < offset || idx >= offset + per_category {
            continue;
        }
        out.push(json!({
            "id": format!("controleval/{cat}/{idx}"),
            "benchmark": "ControlEval",
            "source": src,
            "prompt": controleval_prompt(t),
            "fields": [
                {"name": "num", "type": "array", "description": "controller numerator coefficients, descending powers of s"},
                {"name": "den", "type": "array", "description": "controller denominator coefficients, descending powers of s"}
            ],
            "scorer": "controleval",
            "expected_blockers": if t["tau"].as_f64().unwrap_or(0.0) > 0.0 { json!(["time delay / pade", "feedback/series (tf arithmetic)"]) } else { json!(["feedback/series (tf arithmetic)"]) },
            "controleval": t,
        }));
    }
    Ok(out)
}

fn fmt_poly(p: &[f64]) -> String {
    format!("[{}]", p.iter().map(|x| format!("{x}")).collect::<Vec<_>>().join(", "))
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default()
}

/// Our own wording of a ControlEval task (the upstream prompt text is not reused).
pub fn controleval_prompt(t: &Value) -> String {
    let tau = t["tau"].as_f64().filter(|x| *x > 0.0);
    format!(
        "Design a controller C(s) for the plant G(s) = N(s)/D(s){} with\n  N = {}\n  D = {}\n(coefficients in descending powers of s). The loop is unity negative feedback, L = G·C.\n\
Requirements (all must hold):\n\
1. every closed-loop pole has real part < -0.01;\n\
2. phase margin of L ≥ {:.4} deg;\n\
3. 2 % settling time of the closed-loop step response between {:.6} s and {:.6} s;\n\
4. steady-state error to a unit step ≤ {} (computed as 1/(1+|L(0)|), 0 when L has an integrator).{}\n\
Submit C(s) as num and den.",
        tau.map(|x| format!(" · e^(-{x} s)")).unwrap_or_default(),
        fmt_poly(&nums(&t["num"])),
        fmt_poly(&nums(&t["den"])),
        t["phase_margin_min"].as_f64().unwrap_or(0.0),
        t["settling_time_min"].as_f64().unwrap_or(0.0),
        t["settling_time_max"].as_f64().unwrap_or(0.0),
        t["steadystate_error_max"].as_f64().unwrap_or(0.0),
        tau.map(|x| format!("\nThe delay is evaluated with a 5th-order Padé approximation of e^(-{x} s).")).unwrap_or_default(),
    )
}

pub fn model_prompt(task: &Value) -> String {
    let fields = task["fields"].as_array().cloned().unwrap_or_default();
    let list = fields.iter().map(|f| format!("- {}（{}）：{}", f["name"].as_str().unwrap_or(""), f["type"].as_str().unwrap_or(""), f["description"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n");
    format!("{}\n\n---\n完成后调用 {SUBMIT}，answer 对象包含以下字段：\n{list}", task["prompt"].as_str().unwrap_or(""))
}

/// The `submit_design` pseudo-tool for this task's answer fields.
pub fn submit_tool(task: &Value) -> ToolDef {
    let mut props = serde_json::Map::new();
    let mut req = vec![];
    for f in task["fields"].as_array().cloned().unwrap_or_default() {
        let name = f["name"].as_str().unwrap_or("").to_string();
        let desc = f["description"].as_str().unwrap_or("");
        let s = match f["type"].as_str() {
            Some("array") => json!({"type": "array", "items": {"type": "number"}, "description": desc}),
            Some("integer") => json!({"type": "integer", "description": desc}),
            _ => json!({"type": "number", "description": desc}),
        };
        props.insert(name.clone(), s);
        req.push(name);
    }
    let js = json!({"type": "object", "additionalProperties": false, "required": ["answer"], "properties": {
        "answer": {"type": "object", "additionalProperties": false, "required": req, "properties": props},
        "summary": {"type": "string", "description": "一两句话说明设计与验证结果"}
    }});
    let mut losses = vec![];
    let g = crate::schema_conv::to_gemini(&js, &mut losses);
    ToolDef {
        name: SUBMIT.into(),
        operator_id: "aeoi-eval.submit_design".into(),
        description: "提交最终设计（只能调用一次，调用后评测结束）。answer 中按题目要求给出全部字段的数值。".into(),
        json_schema: js,
        gemini_schema: g,
        losses,
        user_fields: vec![],
    }
}

// ------------------------------------------------------------------------------------------ agent loop

#[derive(Clone)]
pub struct DesignRun {
    pub turns: Vec<ModelTurn>,
    pub tools: Vec<ToolStep>,
    pub submission: Option<Value>,
    pub final_text: Option<String>,
    pub stopped: String,
    pub error: Option<String>,
    pub total_tokens: u64,
}

pub async fn run_task(provider: &dyn Provider, bridge: &Bridge, task: &Value, max_steps: usize, call_timeout: Duration, mut on_turn: impl FnMut(&DesignRun)) -> DesignRun {
    let system = format!("{}{DESIGN_ADDENDUM}", crate::agent::SYSTEM_PROMPT);
    let mut tools = bridge.tools.clone();
    tools.push(submit_tool(task));
    let prompt = model_prompt(task);
    let mut hist = vec![Msg::user(&prompt)];
    let mut run = DesignRun { turns: vec![], tools: vec![], submission: None, final_text: None, stopped: String::new(), error: None, total_tokens: 0 };
    let mut nudges = 0;
    for step in 1..=max_steps {
        let t0 = Instant::now();
        let resp = match tokio::time::timeout(call_timeout, provider.chat(&system, &hist, &tools)).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                run.error = Some(e);
                run.stopped = "provider_error".into();
                on_turn(&run);
                return run;
            }
            Err(_) => {
                run.error = Some(format!("call_timeout: 单次模型请求超过 {:.0} s 未返回（含 429/5xx 退避）", call_timeout.as_secs_f64()));
                run.stopped = "call_timeout".into();
                on_turn(&run);
                return run;
            }
        };
        run.total_tokens += resp.usage.total_tokens.unwrap_or(0);
        run.turns.push(ModelTurn {
            step,
            text: resp.text.clone(),
            tool_calls: resp.calls.iter().map(|c| c.name.clone()).collect(),
            latency_ms: t0.elapsed().as_secs_f64() * 1e3,
            usage: resp.usage.clone(),
            model_version: resp.model_version.clone(),
            finish_reason: resp.finish_reason.clone(),
            http_retries: resp.retries,
        });
        if resp.calls.is_empty() {
            if resp.text.is_some() {
                run.final_text = resp.text.clone();
            }
            if nudges >= 2 {
                run.stopped = "answered_without_submit".into();
                on_turn(&run);
                return run;
            }
            nudges += 1;
            if resp.text.is_some() {
                hist.push(Msg::Assistant { text: resp.text.clone(), calls: vec![], raw: resp.raw.clone() });
            }
            let why = resp.finish_reason.clone().unwrap_or_default();
            hist.push(Msg::user(&if resp.text.is_none() { aeoi_bridge::llm::nudge_text(&why) } else { format!("没有用户可以回答问题。请完成设计并调用 {SUBMIT} 提交（只能调用一次）。") }));
            on_turn(&run);
            continue;
        }
        hist.push(Msg::Assistant { text: resp.text.clone(), calls: resp.calls.clone(), raw: resp.raw.clone() });
        for c in &resp.calls {
            if c.name == SUBMIT {
                let ans = c.args.get("answer").cloned().unwrap_or_else(|| c.args.clone());
                run.submission = Some(ans);
                run.final_text = c.args["summary"].as_str().map(|s| s.to_string()).or(resp.text.clone());
                run.stopped = "submitted".into();
                on_turn(&run);
                return run;
            }
            let (env, ms) = bridge.call_grounded(&c.name, &c.args, Some(&prompt));
            let err = is_error(&env);
            run.tools.push(ToolStep {
                step,
                tool: c.name.clone(),
                operator_id: bridge.tool(&c.name).map(|t| t.operator_id.clone()),
                call_id: c.id.clone(),
                request: c.args.clone(),
                response: compact_for_model(&env, 16),
                is_error: err,
                error_code: env["error"]["code"].as_str().map(|s| s.to_string()),
                latency_ms: ms,
            });
            hist.push(Msg::Tool { call_id: c.id.clone(), name: c.name.clone(), response: compact_for_model(&env, 120) });
        }
        on_turn(&run);
    }
    run.stopped = "max_steps".into();
    on_turn(&run);
    run
}

// ------------------------------------------------------------------------------------------ scoring helpers

fn val(v: &Value) -> f64 {
    let v = if v.is_object() && v.get("value").is_some() { &v["value"] } else { v };
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) if s == "Inf" || s == "inf" => f64::INFINITY,
        Value::String(s) if s == "-Inf" || s == "-inf" => f64::NEG_INFINITY,
        _ => f64::NAN,
    }
}

/// JSON number that keeps NaN/Inf (spec R-2 strings). `json!(f64::INFINITY)` becomes null, which
/// made an infinite phase margin look like a missing field while still passing `pm > 60`.
fn jn(x: f64) -> Value {
    aeoi_core::num(x)
}

fn tf(num: &[f64], den: &[f64], ts: f64) -> Value {
    json!({"kind": "tf", "num": num, "den": den, "ts": ts})
}

fn op(b: &Bridge, tool: &str, args: Value) -> Result<Value, String> {
    let (env, _) = b.call(tool, &args);
    if is_error(&env) {
        return Err(format!("{tool}: {} {}", env["error"]["code"].as_str().unwrap_or("?"), env["error"]["message"].as_str().unwrap_or("")));
    }
    Ok(env["result"].clone())
}

/// Closed-loop poles via control.lti.poles_zeros; returns (all Re < thr, max Re).
fn cl_stable(b: &Bridge, num: &[f64], den: &[f64], thr: f64) -> Result<(bool, f64), String> {
    let r = op(b, "control_lti_poles_zeros", json!({"system": tf(num, den, 0.0), "dcgain": false}))?;
    let mx = r["poles"]["data"].as_array().ok_or("poles")?.iter().map(|p| p[0].as_f64().unwrap_or(f64::NAN)).fold(f64::NEG_INFINITY, f64::max);
    Ok((mx < thr, mx))
}

/// (settling time, overshoot %, steady state) of the closed-loop step via control.lti.step.
fn step_info(b: &Bridge, num: &[f64], den: &[f64], t_final: f64, band: f64) -> Result<(f64, f64, f64), String> {
    let r = op(b, "control_lti_step", json!({"system": tf(num, den, 0.0), "time": {"mode": "uniform", "t_final": t_final, "dt": t_final / 100000.0, "unit": "s"},
        "step_info": {"settling_threshold": band}, "output": {"return_samples": "none"}}))?;
    let i = &r["info"];
    Ok((val(&i["settling_time"]), val(&i["overshoot"]), val(&i["steady_state"])))
}

fn margins(b: &Bridge, num: &[f64], den: &[f64]) -> Result<Value, String> {
    op(b, "control_analysis_margin", json!({"system": tf(num, den, 0.0)}))
}

/// Magnitude in dB at `f_hz` (discrete when ts > 0) via control.lti.bode.
fn mag_db(b: &Bridge, num: &[f64], den: &[f64], ts: f64, f_hz: &[f64]) -> Result<Vec<f64>, String> {
    let r = op(b, "control_lti_bode", json!({"system": tf(num, den, ts), "frequency": {"mode": "explicit", "values": f_hz, "unit": "Hz"}}))?;
    Ok(r["magnitude"]["value"]["data"].as_array().ok_or("magnitude")?.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
}

fn series(a: (&[f64], &[f64]), c: (&[f64], &[f64])) -> (Vec<f64>, Vec<f64>) {
    (strip(&polymul(a.0, c.0)), strip(&polymul(a.1, c.1)))
}

fn feedback(l: &(Vec<f64>, Vec<f64>)) -> (Vec<f64>, Vec<f64>) {
    (l.0.clone(), strip(&polyadd_scaled(&l.1, &l.0, 1.0)))
}

/// Padé approximation of e^{-θs} (numerator and denominator degree n), descending powers.
pub fn pade(theta: f64, n: usize) -> (Vec<f64>, Vec<f64>) {
    let fact = |k: usize| (1..=k).fold(1.0f64, |a, i| a * i as f64);
    let mut num = vec![0.0; n + 1];
    let mut den = vec![0.0; n + 1];
    for k in 0..=n {
        let c = fact(2 * n - k) * fact(n) / (fact(2 * n) * fact(k) * fact(n - k)) * theta.powi(k as i32);
        den[n - k] = c;
        num[n - k] = if k % 2 == 1 { -c } else { c };
    }
    let lead = den[0];
    (num.iter().map(|x| x / lead).collect(), den.iter().map(|x| x / lead).collect())
}

fn f(a: &Value, k: &str) -> Result<f64, String> {
    a.get(k).and_then(|x| x.as_f64()).ok_or_else(|| format!("缺少数值字段 {k}"))
}

fn arr(a: &Value, k: &str) -> Result<Vec<f64>, String> {
    let v = a.get(k).and_then(|x| x.as_array()).ok_or_else(|| format!("缺少数组字段 {k}"))?;
    let out: Vec<f64> = v.iter().filter_map(|x| x.as_f64()).collect();
    if out.len() != v.len() || out.is_empty() {
        return Err(format!("{k} 必须是非空数字数组"));
    }
    Ok(out)
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect()
}

fn max_over(list: &Value, key: &str) -> f64 {
    list.as_array().map(|a| a.iter().map(|c| val(&c[key])).filter(|x| x.is_finite()).fold(f64::NEG_INFINITY, f64::max)).filter(|x| x.is_finite()).unwrap_or(f64::INFINITY)
}

type Scored = Result<(bool, Option<f64>, Value), String>;

// ------------------------------------------------------------------------------------------ scorers

fn pid_unity(b: &Bridge, tid: &str, a: &Value) -> Scored {
    let (kp, ki, kd) = (f(a, "Kp")?, f(a, "Ki")?, f(a, "Kd")?);
    let g: (Vec<f64>, Vec<f64>) = if tid == "XG_13" { (vec![1.0], vec![1.0, 10.0, 20.0]) } else { (vec![1.0], polymul(&polymul(&[0.5, 1.0], &[1.0, 1.0]), &[2.0, 1.0])) };
    let c = (strip(&[kd, kp, ki]), vec![1.0, 0.0]);
    let l = series((&g.0, &g.1), (&c.0, &c.1));
    let t = feedback(&l);
    let (stable, max_re) = cl_stable(b, &t.0, &t.1, 0.0)?;
    let mut d = json!({"stable": stable, "max_pole_re": jn(max_re)});
    if !stable {
        return Ok((false, Some(0.0), d));
    }
    if tid == "XG_13" {
        let (ts, os, _) = step_info(b, &t.0, &t.1, 10.0, 0.02)?;
        d["settling_time"] = jn(ts);
        d["overshoot_pct"] = jn(os);
        d["upstream_literal_pass"] = json!(ts < 2.0 && os < 0.05);
        let score = 40.0 + if ts < 0.2 { 30.0 } else { 0.0 } + if os < 5.0 { 30.0 } else { 0.0 };
        return Ok((ts < 0.2 && os < 5.0, Some(score), d));
    }
    let (ts, os, yf) = step_info(b, &t.0, &t.1, 30.0, 0.05)?;
    let m = margins(b, &l.0, &l.1)?;
    let (gm, pm) = (val(&m["gain_margin_db"]), val(&m["phase_margin"]));
    let e = (1.0 - yf).abs();
    d.update_with(json!({"settling_time_5pct": jn(ts), "overshoot_pct": jn(os), "ess": jn(e), "gm_db": jn(gm), "pm_deg": jn(pm),
        "upstream_literal_pass": ts < 5.0 && os < 20.0 && e < 1e-3 && gm > 20.0 && pm > 45.0}));
    let score = [(ts < 5.0, 30.0), (os < 20.0, 20.0), (e < 1e-3, 30.0), (gm > 10.0, 10.0), (pm > 45.0, 10.0)].iter().filter(|x| x.0).map(|x| x.1).sum::<f64>();
    Ok((score == 100.0, Some(score), d))
}

trait Update {
    fn update_with(&mut self, other: Value);
}
impl Update for Value {
    fn update_with(&mut self, other: Value) {
        if let (Some(a), Value::Object(o)) = (self.as_object_mut(), other) {
            a.extend(o);
        }
    }
}

fn fopdt(b: &Bridge, a: &Value) -> Scored {
    let (theta, tau) = (f(a, "theta")?, f(a, "tau")?);
    let (num, den) = (arr(a, "num")?, arr(a, "den")?);
    let (pn, pd) = pade(theta, 10);
    let g = series((&pn, &pd), (&[1.0], &[tau, 1.0]));
    let l = series((&g.0, &g.1), (&num, &den));
    let t = feedback(&l);
    let th_ok = (theta - 16.5).abs() <= 2.0;
    let ta_ok = (tau - 19.5).abs() <= 2.0;
    let (stable, max_re) = cl_stable(b, &t.0, &t.1, 0.0)?;
    let mut d = json!({"theta_ok": th_ok, "tau_ok": ta_ok, "stable": stable, "max_pole_re": jn(max_re)});
    let base = if th_ok { 10.0 } else { 0.0 } + if ta_ok { 10.0 } else { 0.0 };
    if !stable {
        return Ok((false, Some(base), d));
    }
    let (ts, os, _) = step_info(b, &t.0, &t.1, 2000.0, 0.02)?;
    let m = margins(b, &l.0, &l.1)?;
    let (gm, pm) = (val(&m["gain_margin_db"]), val(&m["phase_margin"]));
    d.update_with(json!({"settling_time": jn(ts), "overshoot_pct": jn(os), "gm_db": jn(gm), "pm_deg": jn(pm)}));
    let checks = [(os <= 10.0, 20.0), (ts <= 150.0, 20.0), (gm > 7.0, 20.0), (pm > 60.0, 20.0)];
    let score = base + checks.iter().filter(|x| x.0).map(|x| x.1).sum::<f64>();
    Ok((score == 100.0, Some(score), d))
}

const XG_PLANT_NUM: [f64; 1] = [84.5];
const XG_PLANT_DEN: [f64; 4] = [1.0, 0.26, 169.0, 0.0];

/// Kg·Ki as upstream writes it: num = kg·[βb, ωL], den = [√(βb²+1), 0].
fn kgki(wl: f64) -> (Vec<f64>, Vec<f64>) {
    let g = polyval(&XG_PLANT_NUM, C64::new(0.0, wl)) / polyval(&XG_PLANT_DEN, C64::new(0.0, wl));
    let kg = 1.0 / g.norm();
    let bb = 10f64.sqrt();
    (vec![kg * bb, kg * wl], vec![(bb * bb + 1.0).sqrt(), 0.0])
}

fn coef_check(a: &Value, wl: f64) -> Result<Value, String> {
    let (n, d) = (arr(a, "C_num")?, arr(a, "C_den")?);
    let (en, ed) = kgki(wl);
    let dist = |x: &[f64], y: &[f64]| if x.len() != y.len() { f64::INFINITY } else { x.iter().zip(y).map(|(p, q)| (p - q).powi(2)).sum::<f64>().sqrt() };
    let lit_n = dist(&n, &en) < 0.1;
    let lit_d = dist(&d, &ed) < 0.1;
    // same transfer function up to a common scale (normalise by the leading denominator coefficient)
    let equiv = d.first().map(|l| *l != 0.0).unwrap_or(false) && {
        let nn: Vec<f64> = n.iter().map(|x| x / d[0]).collect();
        let nd: Vec<f64> = d.iter().map(|x| x / d[0]).collect();
        let en2: Vec<f64> = en.iter().map(|x| x / ed[0]).collect();
        let ed2: Vec<f64> = ed.iter().map(|x| x / ed[0]).collect();
        dist(&nn, &en2) < 0.1 / ed[0] && dist(&nd, &ed2) < 0.1 / ed[0]
    };
    Ok(json!({"num_ok_upstream_literal": lit_n, "den_ok_upstream_literal": lit_d, "tf_equivalent": equiv, "expected_num": en, "expected_den": ed}))
}

fn loopshape(b: &Bridge, tid: &str, a: &Value) -> Scored {
    let wl = if tid == "XG_11" { 1.0 } else { 3.0 };
    let mut d = coef_check(a, wl)?;
    let (k, gmin, pmin) = if tid == "XG_11" {
        let (wr, br) = (f(a, "omega_r")?, f(a, "beta_r")?);
        let p = br * wr;
        let fr = (vec![p * p], vec![1.0, 2f64.sqrt() * p, p * p]);
        let s = 1.0 / (polyval(&fr.0, C64::new(0.0, wr)) / polyval(&fr.1, C64::new(0.0, wr))).norm();
        ((vec![p * p * s], fr.1), 3.0, 50.0)
    } else {
        let (wn, al, fnn) = (f(a, "omega_n")?, f(a, "alpha_n")?, f(a, "f_n")?);
        if al <= 0.0 {
            return Err("alpha_n 必须 > 0".into());
        }
        ((vec![1.0, fnn * wn * al.sqrt(), wn * wn], vec![1.0, fnn * wn / al.sqrt(), wn * wn]), 2.0, 60.0)
    };
    let c = kgki(wl);
    let g = series((&XG_PLANT_NUM, &XG_PLANT_DEN), (&c.0, &c.1));
    let l = series((&g.0, &g.1), (&k.0, &k.1));
    let t = feedback(&l);
    let (stable, max_re) = cl_stable(b, &t.0, &t.1, 0.0)?;
    d.update_with(json!({"stable": stable, "max_pole_re": jn(max_re)}));
    let coef_ok = d["tf_equivalent"] == true;
    let coef_pts = if coef_ok { 40.0 } else { 0.0 };
    if !stable {
        return Ok((false, Some(coef_pts), d));
    }
    let m = margins(b, &l.0, &l.1)?;
    let gmax = max_over(&m["crossings"]["phase"], "gain_margin_db");
    let pmax = max_over(&m["crossings"]["gain"], "phase_margin_deg");
    let gmax_abs = 10f64.powf(gmax / 20.0);
    d.update_with(json!({"gm_db_min": jn(val(&m["gain_margin_db"])), "pm_deg_min": jn(val(&m["phase_margin"])), "gm_db_max": jn(gmax), "pm_deg_max": jn(pmax),
        "upstream_literal_pass": d["num_ok_upstream_literal"] == true && d["den_ok_upstream_literal"] == true && gmax_abs >= gmin && pmax >= pmin}));
    let score = coef_pts + 20.0 + if pmax >= pmin { 20.0 } else { 0.0 } + if gmax >= gmin { 20.0 } else { 0.0 };
    Ok((score == 100.0, Some(score), d))
}

const XG02_A: [f64; 49] = [
    -3360.27884342382, 24650.4407238876, -24650.4407238876, 24650.4407238876, -24650.4407238876, 24650.4407238876, -6162.61018097190,
    -3270.26403683917, 10278.7248098086, -7476.39347226054, 7476.39347226054, -7476.39347226054, 7476.39347226054, -1869.09836806513,
    -2657.10970968037, 10628.4388387215, -13430.7701762696, 16233.1015138177, -16233.1015138177, 16233.1015138177, -4058.27537845441,
    -3954.03177459846, 15816.1270983939, -15816.1270983939, 13013.7957608458, -10211.4644232977, 10211.4644232977, -2552.86610582443,
    -2325.45756017668, 9301.83024070671, -9301.83024070671, 9301.83024070671, -12104.1615782548, 14906.4929158029, -3726.62322895072,
    -1665.81075466004, 6663.24301864017, -6663.24301864017, 6663.24301864017, -6663.24301864017, 3860.91168109210, -264.645085886005,
    -1596.51916299641, 6386.07665198562, -6386.07665198562, 6386.07665198562, -6386.07665198562, 6386.07665198562, -4398.85050054448,
];
const XG02_B: [f64; 7] = [33.1501, 29.2147, 49.7430, 55.4071, 43.2351, 22.8726, 24.9153];
const XG02_C: [f64; 7] = [41.1585, -164.6342, 164.6342, -164.6342, 164.6342, -164.6342, 41.1585];

fn first(v: &Value) -> Vec<f64> {
    // tf coefficients may come flat or MIMO-nested [[[...]]]
    let mut x = v;
    while x.as_array().map(|a| a.first().map(|y| y.is_array()).unwrap_or(false)).unwrap_or(false) {
        x = &x[0];
    }
    nums(x)
}

fn loopshape_bandwidth(b: &Bridge, a: &Value) -> Scored {
    let (num, den) = (arr(a, "num")?, arr(a, "den")?);
    let m = |d: &[f64], r: usize, c: usize| json!({"dtype": "f64", "shape": [r, c], "data": d});
    let g = op(b, "control_lti_convert", json!({"system": {"kind": "ss", "A": m(&XG02_A, 7, 7), "B": m(&XG02_B, 7, 1), "C": m(&XG02_C, 1, 7), "D": m(&[0.0], 1, 1), "ts": 0.0}, "to": "tf"}))?;
    let (gn, gd) = (first(&g["system"]["num"]), first(&g["system"]["den"]));
    let l = series((&gn, &gd), (&num, &den));
    let t = feedback(&l);
    let (stable, max_re) = cl_stable(b, &t.0, &t.1, 0.0)?;
    let mut d = json!({"stable": stable, "max_pole_re": jn(max_re)});
    if !stable {
        return Ok((false, Some(0.0), d));
    }
    let w: Vec<f64> = (0..1000).map(|i| 10f64.powf(4.0 * i as f64 / 999.0)).collect();
    let fz: Vec<f64> = w.iter().map(|x| x / (2.0 * PI)).collect();
    let db = mag_db(b, &l.0, &l.1, 0.0, &fz)?;
    let mut bw = 0.0;
    for i in 0..db.len() - 1 {
        if db[i].signum() != db[i + 1].signum() {
            bw = (w[i] + (w[i + 1] - w[i]) * (-db[i]) / (db[i + 1] - db[i])) / (2.0 * PI);
            break;
        }
    }
    let mg = margins(b, &l.0, &l.1)?;
    let (gm, pm) = (val(&mg["gain_margin_db"]), val(&mg["phase_margin"]));
    // Rubric (vendored): bandwidth 40, GM > 1.5 dB 30, PM > 60° 30. Upstream's MATLAB scorer is
    // all-or-nothing on the pass flag but still returns this partial score; an earlier version of
    // ours returned 100 or 0, so a stable design that only missed the bandwidth got 0.
    let bw_ok = (bw - 85.0).abs() <= 10.0;
    let gm_ok = gm > 1.5;
    let pm_ok = pm > 60.0;
    let score = (if bw_ok { 40.0 } else { 0.0 }) + (if gm_ok { 30.0 } else { 0.0 }) + (if pm_ok { 30.0 } else { 0.0 });
    d.update_with(json!({"bandwidth_hz": jn(bw), "gm_db": jn(gm), "pm_deg": jn(pm), "bandwidth_ok": bw_ok, "gm_ok": gm_ok, "pm_ok": pm_ok,
        "upstream_literal_pass": bw_ok && 10f64.powf(gm / 20.0) > 1.5 && pm > 60.0}));
    Ok((bw_ok && gm_ok && pm_ok, Some(score), d))
}

fn maglev(a: &Value) -> Scored {
    let r = [("a11", 0.0), ("a12", 1.0), ("a21", 2.0), ("a22", 0.0), ("b11", 0.0), ("b21", -2.0), ("k1", -2.0), ("k2", -1.0), ("l1", 11.0), ("l2", 32.0)];
    let get = |k: &str| a.get(k).and_then(|x| x.as_f64()).unwrap_or(f64::NAN);
    let grp = |ks: &[&str]| ks.iter().all(|k| (get(k) - r.iter().find(|x| x.0 == *k).unwrap().1).abs() <= 1e-6);
    let mut d = json!({"A": grp(&["a11", "a12", "a21", "a22"]), "B": grp(&["b11", "b21"]), "K": grp(&["k1", "k2"]), "L": grp(&["l1", "l2"])});
    let mut score = [("A", 8.0), ("B", 8.0), ("K", 10.0), ("L", 10.0)].iter().filter(|x| d[x.0] == true).map(|x| x.1).sum::<f64>();
    for (i, s) in [1, 0, 1, 0].iter().enumerate() {
        let k = format!("s{}", i + 1);
        let ok = get(&k) == *s as f64;
        d[&k] = json!(ok);
        if ok {
            score += 16.0;
        }
    }
    Ok((score == 100.0, Some(score), d))
}

fn weighted(d: &Value, w: &[(&str, f64)]) -> (bool, f64) {
    let s = w.iter().filter(|x| d[x.0] == true).map(|x| x.1).sum::<f64>() * 100.0;
    (w.iter().all(|x| d[x.0] == true), (s * 1e9).round() / 1e9)
}

/// Upstream evaluates |H| at 0 Hz too; bode needs f > 0, so DC is evaluated directly.
fn dc_db(num: &[f64], den: &[f64]) -> f64 {
    20.0 * (num.iter().sum::<f64>() / den.iter().sum::<f64>()).abs().log10()
}

fn iir_notch(b: &Bridge, a: &Value) -> Scored {
    let (num, den, order) = (arr(a, "coeffs_numerator")?, arr(a, "coeffs_denominator")?, f(a, "order")?);
    let ts = 1.0 / 2000.0;
    let lo_f = linspace(0.0, 115.0, 116);
    let lo = [vec![dc_db(&num, &den)], mag_db(b, &num, &den, ts, &lo_f[1..])?].concat();
    let hi = mag_db(b, &num, &den, ts, &linspace(125.0, 250.0, 126))?;
    let tg = mag_db(b, &num, &den, ts, &[120.0])?[0];
    let d = json!({"lower_ok": lo.iter().all(|x| *x >= -3.0), "upper_ok": hi.iter().all(|x| *x >= -3.0), "target_ok": tg <= -40.0,
        "order_ok": order <= 50.0, "coeff_ok": num.len() as f64 == order + 1.0 && den.len() as f64 == order + 1.0,
        "min_db_lower": jn(lo.iter().cloned().fold(f64::INFINITY, f64::min)), "min_db_upper": jn(hi.iter().cloned().fold(f64::INFINITY, f64::min)), "db_at_120hz": jn(tg)});
    let (ok, s) = weighted(&d, &[("lower_ok", 0.2), ("upper_ok", 0.2), ("target_ok", 0.4), ("order_ok", 0.1), ("coeff_ok", 0.1)]);
    Ok((ok, Some(s), d))
}

fn fir_antialias(b: &Bridge, a: &Value) -> Scored {
    let (c, order, stp, dec) = (arr(a, "coeffs")?, f(a, "order")?, f(a, "stpbnd")?, f(a, "decim")?);
    let ts = 1.0 / 48000.0;
    let mut den = vec![0.0; c.len()];
    den[0] = 1.0; // H(z) = Σ c_k z^-k = (Σ c_k z^{N-k}) / z^N
    let pf = linspace(0.0, 3500.0, 3500);
    let pb = [vec![dc_db(&c, &[1.0])], mag_db(b, &c, &den, ts, &pf[1..])?].concat();
    let sb = mag_db(b, &c, &den, ts, &linspace(4000.0, 10000.0, 6000))?;
    let tb = mag_db(b, &c, &den, ts, &linspace(3500.0, 4000.0, 20))?;
    let d = json!({"pass_ok": pb.iter().all(|x| *x >= -3.0), "stop_ok": sb[1..].iter().all(|x| *x <= -40.0), "trans_ok": tb[1..].iter().all(|x| *x <= 0.0),
        "order_ok": order <= 350.0, "ord_coeff_ok": c.len() as f64 == order + 1.0, "stpbnd_ok": (3500.0..=4000.0).contains(&stp), "decim_ok": dec == 6.0,
        "min_db_pass": jn(pb.iter().cloned().fold(f64::INFINITY, f64::min)), "max_db_stop": jn(sb[1..].iter().cloned().fold(f64::NEG_INFINITY, f64::max))});
    let (ok, s) = weighted(&d, &[("pass_ok", 0.2), ("stop_ok", 0.2), ("trans_ok", 0.1), ("order_ok", 0.1), ("ord_coeff_ok", 0.1), ("stpbnd_ok", 0.2), ("decim_ok", 0.1)]);
    Ok((ok, Some(s), d))
}

fn controleval(b: &Bridge, t: &Value, a: &Value) -> Scored {
    let (num, den) = (arr(a, "num")?, arr(a, "den")?);
    let mut g = (nums(&t["num"]), nums(&t["den"]));
    if let Some(tau) = t["tau"].as_f64().filter(|x| *x > 0.0) {
        let (pn, pd) = pade(tau, 5);
        g = series((&g.0, &g.1), (&pn, &pd));
    }
    let l = series((&g.0, &g.1), (&num, &den));
    let tcl = feedback(&l);
    let (stable, max_re) = cl_stable(b, &tcl.0, &tcl.1, -0.01)?;
    let mut d = json!({"stable": stable, "max_pole_re": jn(max_re)});
    if !stable {
        return Ok((false, None, d));
    }
    let m = margins(b, &l.0, &l.1)?;
    let pm = val(&m["phase_margin"]);
    let (st_min, st_max) = (t["settling_time_min"].as_f64().unwrap_or(0.0), t["settling_time_max"].as_f64().unwrap_or(f64::INFINITY));
    let (ts, os, _) = step_info(b, &tcl.0, &tcl.1, (20.0 * st_max).clamp(10.0, 500.0), 0.02)?;
    let ln = strip(&l.0);
    let ld = strip(&l.1);
    let (n0, d0) = (*ln.last().unwrap_or(&0.0), *ld.last().unwrap_or(&0.0));
    let sse = if d0 == 0.0 && n0 != 0.0 { 0.0 } else if d0 == 0.0 { 1.0 } else { 1.0 / (1.0 + (n0 / d0).abs()) };
    d.update_with(json!({"pm_deg": jn(pm), "settling_time": jn(ts), "overshoot_pct": jn(os), "sse": jn(sse),
        "pm_ok": pm >= t["phase_margin_min"].as_f64().unwrap_or(0.0), "settling_ok": st_min <= ts && ts <= st_max, "sse_ok": sse <= t["steadystate_error_max"].as_f64().unwrap_or(0.0)}));
    let ok = d["pm_ok"] == true && d["settling_ok"] == true && d["sse_ok"] == true;
    Ok((ok, None, d))
}

/// Scores a submission: (pass, score 0–100 where the upstream rubric defines one, details).
pub fn score(b: &Bridge, task: &Value, answer: &Value) -> (bool, Option<f64>, Value) {
    let tid = task["source"]["task"].as_str().unwrap_or("");
    let r = match task["scorer"].as_str().unwrap_or("") {
        "pid_unity" => pid_unity(b, tid, answer),
        "fopdt_controller" => fopdt(b, answer),
        "loopshape_rolloff" | "loopshape_notch" => loopshape(b, tid, answer),
        "loopshape_bandwidth" => loopshape_bandwidth(b, answer),
        "maglev_observer" => maglev(answer),
        "iir_notch" => iir_notch(b, answer),
        "fir_antialias" => fir_antialias(b, answer),
        "alpha_range" => f(answer, "alpha").map(|x| ((0.651..=1.087).contains(&x), Some(if (0.651..=1.087).contains(&x) { 100.0 } else { 0.0 }), json!({"alpha": x}))),
        "controleval" => controleval(b, &task["controleval"], answer),
        s => Err(format!("unknown scorer {s}")),
    };
    r.unwrap_or_else(|e| (false, Some(0.0), json!({"error": e})))
}

// ------------------------------------------------------------------------------------------ run + report

pub struct DesignOpts {
    pub rpm: f64,
    pub max_steps: usize,
    /// Bounds one model request, including provider retries. A stalled socket cannot sit for the
    /// old 180 s × (retries + 1).
    pub call_timeout: Duration,
    /// Bounds one task. The report is checkpointed at task start and after every model turn, so a
    /// kill still leaves the task id and whatever turns finished.
    pub task_timeout: Duration,
}

impl Default for DesignOpts {
    fn default() -> Self {
        DesignOpts { rpm: 8.0, max_steps: 10, call_timeout: Duration::from_secs(150), task_timeout: Duration::from_secs(600) }
    }
}

fn blockers_of(task: &Value) -> Vec<String> {
    task["expected_blockers"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn result_from(task: &Value, run: &DesignRun, latency_s: f64, scored: Option<(bool, Option<f64>, Value)>) -> DesignResult {
    let (pass, score, details) = scored.unwrap_or_else(|| (false, None, json!({"status": if run.stopped.is_empty() { "running" } else { run.stopped.as_str() }})));
    DesignResult {
        id: task["id"].as_str().unwrap_or("").into(),
        benchmark: task["benchmark"].as_str().unwrap_or("").into(),
        scorer: task["scorer"].as_str().unwrap_or("").into(),
        submitted: run.submission.is_some(),
        submission: run.submission.clone(),
        pass,
        score,
        details,
        expected_blockers: blockers_of(task),
        tool_calls: run.tools.iter().map(|t| t.tool.clone()).collect(),
        tool_errors: run.tools.iter().filter_map(|t| t.error_code.clone()).collect(),
        self_corrections: count_self_corrections(&run.tools),
        model_turns: run.turns.len(),
        stopped: if run.stopped.is_empty() { "running".into() } else { run.stopped.clone() },
        final_text: run.final_text.clone(),
        provider_error: run.error.clone(),
        total_tokens: run.total_tokens,
        latency_s,
        controleval_task: if task["benchmark"] == "ControlEval" { Some(task["controleval"].clone()) } else { None },
        transcript_tools: run.tools.clone(),
    }
}

pub async fn run_design(provider: &dyn Provider, bridge: &Bridge, tasks: &[Value], opts: &DesignOpts, mut checkpoint: impl FnMut(&Value)) -> Value {
    let paced = Paced { inner: provider, min_interval: Duration::from_secs_f64(60.0 / opts.rpm.max(0.1)), last: std::sync::Mutex::new(None) };
    let t_all = Instant::now();
    let mut results: Vec<DesignResult> = vec![];
    for (i, task) in tasks.iter().enumerate() {
        let id = task["id"].as_str().unwrap_or("");
        eprintln!("[{}/{}] {} 开始（最多 {} 步，单次请求 {:.0} s，本题 {:.0} s）", i + 1, tasks.len(), id, opts.max_steps, opts.call_timeout.as_secs_f64(), opts.task_timeout.as_secs_f64());
        let t0 = Instant::now();
        let running = DesignRun { turns: vec![], tools: vec![], submission: None, final_text: None, stopped: "running".into(), error: None, total_tokens: 0 };
        results.push(result_from(task, &running, 0.0, None));
        let slot = results.len() - 1;
        checkpoint(&summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts));
        let outcome = tokio::time::timeout(opts.task_timeout, run_task(&paced, bridge, task, opts.max_steps, opts.call_timeout, |run| {
            eprintln!("  … {id} 回合 {} 工具 {} 次 stop={}", run.turns.len(), run.tools.len(), if run.stopped.is_empty() { "…" } else { run.stopped.as_str() });
            results[slot] = result_from(task, run, t0.elapsed().as_secs_f64(), None);
            checkpoint(&summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts));
        }))
        .await;
        let timed_out = outcome.is_err();
        let kept_turns = results[slot].model_turns;
        let run = match outcome {
            Ok(r) => r,
            Err(_) => DesignRun {
                turns: vec![],
                tools: results[slot].transcript_tools.clone(),
                submission: results[slot].submission.clone(),
                final_text: results[slot].final_text.clone(),
                stopped: "task_timeout".into(),
                error: Some(format!("task_timeout: 本题超过 {:.0} s", opts.task_timeout.as_secs_f64())),
                total_tokens: results[slot].total_tokens,
            },
        };
        let quota = run.error.as_deref().map(|e| e.starts_with(crate::provider::QUOTA_EXHAUSTED)).unwrap_or(false);
        let (pass, sc, details) = if let Some(a) = &run.submission {
            score(bridge, task, a)
        } else {
            let why = run.error.clone().unwrap_or_else(|| "no submission".into());
            (false, None, json!({"error": why}))
        };
        let mut r = result_from(task, &run, t0.elapsed().as_secs_f64(), Some((pass, sc, details)));
        if timed_out {
            // the in-flight turn was dropped with the future; the checkpoint has the finished turns
            r.model_turns = kept_turns;
            r.stopped = "task_timeout".into();
        }
        eprintln!("[{}/{}] {:<40} pass={} score={} submitted={} tools={} errs={:?} turns={} stop={} {:.1}s{}", i + 1, tasks.len(), r.id, r.pass,
            r.score.map(|s| format!("{s:.0}")).unwrap_or("-".into()), r.submitted, r.tool_calls.len(), r.tool_errors, r.model_turns, r.stopped, r.latency_s,
            r.provider_error.as_ref().map(|e| format!(" PROVIDER_ERROR: {e}")).unwrap_or_default());
        results[slot] = r;
        checkpoint(&summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts));
        if quota {
            let skipped: Vec<String> = tasks[i + 1..].iter().map(|t| t["id"].as_str().unwrap_or("").to_string()).collect();
            eprintln!("配额用尽，停止；未运行 {} 题：{}", skipped.len(), skipped.join(", "));
            let mut rep = summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts);
            rep["aborted"] = json!({"reason": run.error, "not_run": skipped});
            return rep;
        }
    }
    summarize(provider, &results, t_all.elapsed().as_secs_f64(), opts)
}

pub fn summarize(provider: &dyn Provider, results: &[DesignResult], wall_s: f64, opts: &DesignOpts) -> Value {
    let n = results.len();
    let cnt = |f: &dyn Fn(&DesignResult) -> bool| results.iter().filter(|r| f(r)).count();
    let mut by_bench: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut tools: std::collections::BTreeMap<String, usize> = Default::default();
    for r in results {
        let e = by_bench.entry(r.benchmark.clone()).or_default();
        e.0 += 1;
        e.1 += r.pass as usize;
        for t in &r.tool_calls {
            *tools.entry(t.clone()).or_default() += 1;
        }
    }
    let scores: Vec<f64> = results.iter().filter_map(|r| r.score).collect();
    let mean_score = if scores.is_empty() { Value::Null } else { json!(scores.iter().sum::<f64>() / scores.len() as f64) };
    json!({
        "mode": "design",
        "provider": provider.name(),
        "model": provider.model(),
        "generated_by": format!("aeoi-web {} design", env!("CARGO_PKG_VERSION")),
        "spec": aeoi_core::SPEC,
        "scoring": "our re-implementation of each task's evaluator on AEOI operators (step/margin/bode/poles_zeros/convert); cross-checked with python-control/scipy by eval/design/pyscore.py (not MATLAB)",
        "sources": {
            "EngDesign": {"repo": "https://github.com/AGI4Engineering/EngDesign", "license": "MIT", "vendored": "eval/design/engdesign.json"},
            "ControlEval": {"repo": "https://github.com/EnvCommons/ControlEval", "data": "https://github.com/ControlAgent/ControlAgent", "license": "none declared — fetched at run time, not vendored"}
        },
        "settings": {"rpm": opts.rpm, "max_steps": opts.max_steps, "call_timeout_s": opts.call_timeout.as_secs_f64(), "task_timeout_s": opts.task_timeout.as_secs_f64(), "temperature": 0.0},
        "aborted": null,
        "summary": {
            "tasks": n,
            "passed": cnt(&|r| r.pass),
            "pass_rate": if n > 0 { cnt(&|r| r.pass) as f64 / n as f64 } else { 0.0 },
            "submitted": cnt(&|r| r.submitted),
            "mean_score_where_defined": mean_score,
            "by_benchmark": by_bench.iter().map(|(k, (t, p))| (k.clone(), json!(format!("{p}/{t}")))).collect::<serde_json::Map<_, _>>(),
            "tool_calls": tools,
            "tool_errors": results.iter().map(|r| r.tool_errors.len()).sum::<usize>(),
            "model_turns": results.iter().map(|r| r.model_turns).sum::<usize>(),
            "provider_errors": cnt(&|r| r.provider_error.is_some()),
            "total_tokens": results.iter().map(|r| r.total_tokens).sum::<u64>(),
            "wall_time_s": wall_s,
        },
        "results": results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{MockProvider, Provider, QUOTA_EXHAUSTED};

    fn opts(max_steps: usize) -> DesignOpts {
        DesignOpts { rpm: 6000.0, max_steps, call_timeout: Duration::from_secs(5), task_timeout: Duration::from_secs(30) }
    }

    fn task(id: &str) -> Value {
        engdesign_tasks().into_iter().find(|t| t["id"] == format!("engdesign/{id}")).unwrap()
    }

    fn reference(id: &str) -> Value {
        match id {
            "XG_13" => json!({"Kp": 800, "Ki": 2000, "Kd": 60}), // the upstream reference (350, 300, 50) misses the prompt's Ts < 0.2 s
            "Ziheng_03" => json!({"Kp": 10, "Ki": 1.55, "Kd": 9.5}),
            "XG_05" => json!({"theta": 16.5, "tau": 19.5, "num": [23.24, 0.9], "den": [25.82, 0]}),
            "XG_11" => json!({"C_num": [6.2871, 1.9882], "C_den": [3.3166, 0], "omega_r": 1.0, "beta_r": 4.7}),
            "XG_12" => json!({"C_num": [17.96, 17.04], "C_den": [3.3166, 0], "omega_n": 13, "alpha_n": 0.04, "f_n": 0.1}),
            "XG_02" => json!({"num": [1.4493e+03, 5.0291e+05, 8.1368e+09, 2.5012e+12], "den": [1, 1.8372e+03, 6.8929e+06, 2.4391e+09, -7.0209e+11]}),
            "LX_02" => json!({"a11": 0, "a12": 1, "a21": 2, "a22": 0, "b11": 0, "b21": -2, "k1": -2, "k2": -1, "l1": 11, "l2": 32, "s1": 1, "s2": 0, "s3": 1, "s4": 0}),
            "AV_02" => json!({"order": 8, "coeffs_numerator": [0.971987554400379, -7.23074141988472, 24.0593587574556, -46.7018447646062, 57.8028554708087, -46.7018447646062, 24.0593587574556, -7.23074141988472, 0.971987554400379],
                "coeffs_denominator": [1, -7.38629070277517, 24.4025053599328, -47.0320991049601, 57.7993568733735, -46.3686717935595, 23.7189260553057, -7.07811076768760, 0.944759805909239]}),
            "AV_03" => {
                let sol = task("AV_03")["reference_solution"].as_str().unwrap().to_string();
                let body = sol.split("coeffs =").nth(1).unwrap().split(']').next().unwrap().replace('[', "");
                let c: Vec<f64> = body.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                json!({"order": c.len() - 1, "coeffs": c, "stpbnd": 3900, "decim": 6})
            }
            "ZC_02" => json!({"alpha": 1.087}),
            _ => unreachable!(),
        }
    }

    #[test]
    fn references_pass_and_perturbations_fail() {
        let b = Bridge::new();
        for t in engdesign_tasks() {
            let id = t["source"]["task"].as_str().unwrap();
            let (ok, sc, d) = score(&b, &t, &reference(id));
            assert!(ok, "{id} reference should pass: {d}");
            assert_eq!(sc, Some(100.0), "{id}: {d}");
        }
        let bad = [
            ("XG_13", json!({"Kp": 350, "Ki": 300, "Kd": 50})),
            ("Ziheng_03", json!({"Kp": 10, "Ki": 1.55, "Kd": 0})),
            ("XG_05", json!({"theta": 16.5, "tau": 19.5, "num": [60, 3], "den": [25.82, 0]})),
            ("XG_11", json!({"C_num": [6.2871, 1.9882], "C_den": [3.3166, 0], "omega_r": 1.0, "beta_r": 1.0})),
            ("XG_12", json!({"C_num": [17.96, 17.04], "C_den": [3.3166, 0], "omega_n": 5, "alpha_n": 0.04, "f_n": 0.1})),
            ("LX_02", json!({"a11": 0, "a12": 1, "a21": 2, "a22": 0, "b11": 0, "b21": -2, "k1": 2, "k2": 1, "l1": 11, "l2": 32, "s1": 1, "s2": 0, "s3": 1, "s4": 0})),
            ("AV_02", json!({"order": 2, "coeffs_numerator": [1, -1.8, 1], "coeffs_denominator": [1, -1.7, 0.9]})),
            ("ZC_02", json!({"alpha": 2.0})),
        ];
        for (id, a) in bad {
            let (ok, _, d) = score(&b, &task(id), &a);
            assert!(!ok, "{id} perturbed answer should fail: {d}");
        }
        // equivalent normalisation of C(s) is accepted; upstream's literal check would reject it
        let (ok, _, d) = score(&b, &task("XG_11"), &json!({"C_num": [1.8956, 0.5994], "C_den": [1, 0], "omega_r": 1.0, "beta_r": 4.7}));
        assert!(ok && d["num_ok_upstream_literal"] == false, "{d}");
        // malformed submission → scored as failure, not a panic
        let (ok, _, d) = score(&b, &task("XG_05"), &json!({"theta": 16.5}));
        assert!(!ok && d["error"].is_string());
    }

    #[test]
    fn xg02_partial_score_and_infinite_margin_is_not_null() {
        // Pilot submission: stable, GM about 15 dB, no 0 dB crossing (bandwidth 0), phase margin
        // infinite because there is no gain crossover. The rubric awards 30 + 30, not 0; Inf must
        // survive JSON as the string "Inf" (a null used to pass `pm > 60` and then disappear).
        let b = Bridge::new();
        let (ok, sc, d) = score(&b, &task("XG_02"), &json!({"num": [100.0], "den": [1.0, 100.0]}));
        assert!(!ok, "{d}");
        assert_eq!(d["pm_deg"], json!("Inf"), "{d}");
        assert_eq!(d["bandwidth_ok"], false);
        assert_eq!(d["gm_ok"], true, "{d}");
        assert_eq!(d["pm_ok"], true, "{d}");
        assert_eq!(sc, Some(60.0), "{d}");
    }

    #[test]
    fn pade_matches_known_first_order() {
        let (n, d) = pade(2.0, 1);
        assert!((n[0] + 1.0).abs() < 1e-12 && (n[1] - 1.0).abs() < 1e-12 && (d[0] - 1.0).abs() < 1e-12 && (d[1] - 1.0).abs() < 1e-12, "{n:?} {d:?}");
        let (n, d) = pade(0.5, 5);
        let w = 3.0;
        let h = polyval(&n, C64::new(0.0, w)) / polyval(&d, C64::new(0.0, w));
        assert!((h.norm() - 1.0).abs() < 1e-12 && (h.arg() + 1.5).abs() < 1e-6, "{h}");
    }

    #[test]
    fn controleval_scorer_on_synthetic_task() {
        // synthetic task (not upstream data): G = 2/(s+1), PI controller
        let b = Bridge::new();
        let t = json!({"scorer": "controleval", "controleval": {"num": [2.0], "den": [1.0, 1.0], "phase_margin_min": 60.0, "settling_time_min": 0.1, "settling_time_max": 3.0, "steadystate_error_max": 1e-4}});
        let (ok, _, d) = score(&b, &t, &json!({"num": [2.0, 2.0], "den": [1.0, 0.0]}));
        assert!(ok, "{d}");
        let (ok, _, d) = score(&b, &t, &json!({"num": [0.1], "den": [1.0]}));
        assert!(!ok && d["sse_ok"] == false, "{d}");
        let mut td = t.clone();
        td["controleval"]["tau"] = json!(0.1);
        let p = controleval_prompt(&td["controleval"]);
        assert!(p.contains("e^(-0.1 s)") && p.contains("Padé"));
    }

    #[tokio::test]
    async fn submit_ends_the_run_and_is_scored() {
        let b = Bridge::new();
        let t = task("ZC_02");
        let p = MockProvider::new(vec![
            MockProvider::calls(vec![("control_lti_poles_zeros", json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 2.0], "ts": 0.0}}))]),
            MockProvider::text("我需要更多信息"),
            MockProvider::calls(vec![(SUBMIT, json!({"answer": {"alpha": 0.9}, "summary": "circle criterion"}))]),
        ]);
        let mut writes = 0;
        let mut saw_running = false;
        let rep = run_design(&p, &b, &[t], &opts(6), |partial| {
            writes += 1;
            if partial["results"][0]["stopped"] == "running" {
                saw_running = true;
            }
        })
        .await;
        let r = &rep["results"][0];
        assert_eq!(r["stopped"], "submitted");
        assert_eq!(r["pass"], true);
        assert_eq!(r["model_turns"], 3);
        assert_eq!(rep["summary"]["passed"], 1);
        assert!(saw_running && writes >= 3, "checkpoint writes={writes} saw_running={saw_running}");
        let st = submit_tool(&task("AV_03"));
        assert_eq!(st.json_schema["properties"]["answer"]["required"], json!(["order", "coeffs", "stpbnd", "decim"]));
    }

    #[tokio::test]
    async fn max_steps_stops_without_another_call() {
        let b = Bridge::new();
        let turns = (0..6).map(|_| MockProvider::calls(vec![("control_lti_poles_zeros", json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 1.0], "ts": 0.0}}))])).collect();
        let p = MockProvider::new(turns);
        let rep = run_design(&p, &b, &[task("ZC_02")], &opts(3), |_| {}).await;
        let r = &rep["results"][0];
        assert_eq!(r["stopped"], "max_steps");
        assert_eq!(r["submitted"], false);
        assert_eq!(r["model_turns"], 3);
        assert_eq!(r["pass"], false);
        assert_eq!(p.seen.lock().unwrap().len(), 3);
    }

    struct SlowProvider {
        delay: Duration,
    }
    impl Provider for SlowProvider {
        fn name(&self) -> &str {
            "slow"
        }
        fn model(&self) -> &str {
            "slow"
        }
        fn chat<'a>(&'a self, _: &'a str, _: &'a [crate::provider::Msg], _: &'a [crate::tools::ToolDef]) -> crate::provider::BoxFut<'a, Result<crate::provider::ChatResponse, String>> {
            let delay = self.delay;
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Ok(MockProvider::text("late"))
            })
        }
    }

    #[tokio::test]
    async fn call_timeout_ends_the_task() {
        let b = Bridge::new();
        let p = SlowProvider { delay: Duration::from_secs(30) };
        let mut o = opts(4);
        o.call_timeout = Duration::from_millis(80);
        o.task_timeout = Duration::from_secs(5);
        let t0 = Instant::now();
        let rep = run_design(&p, &b, &[task("ZC_02")], &o, |_| {}).await;
        assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
        assert_eq!(rep["results"][0]["stopped"], "call_timeout");
        assert!(rep["results"][0]["provider_error"].as_str().unwrap().contains("call_timeout"));
        assert_eq!(rep["aborted"], Value::Null);
    }

    #[tokio::test]
    async fn task_timeout_ends_the_task() {
        let b = Bridge::new();
        let p = SlowProvider { delay: Duration::from_millis(400) };
        let mut o = opts(8);
        o.call_timeout = Duration::from_secs(2);
        o.task_timeout = Duration::from_millis(150);
        let t0 = Instant::now();
        let rep = run_design(&p, &b, &[task("ZC_02")], &o, |_| {}).await;
        assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
        assert_eq!(rep["results"][0]["stopped"], "task_timeout");
    }

    struct QuotaThen;
    impl Provider for QuotaThen {
        fn name(&self) -> &str {
            "q"
        }
        fn model(&self) -> &str {
            "q"
        }
        fn chat<'a>(&'a self, _: &'a str, _: &'a [crate::provider::Msg], _: &'a [crate::tools::ToolDef]) -> crate::provider::BoxFut<'a, Result<crate::provider::ChatResponse, String>> {
            Box::pin(async { Err(format!("{QUOTA_EXHAUSTED}: free tier daily")) })
        }
    }

    #[tokio::test]
    async fn quota_aborts_and_lists_the_rest() {
        let b = Bridge::new();
        let rep = run_design(&QuotaThen, &b, &[task("ZC_02"), task("XG_13")], &opts(3), |_| {}).await;
        assert_eq!(rep["results"].as_array().unwrap().len(), 1);
        assert!(rep["results"][0]["provider_error"].as_str().unwrap().starts_with(QUOTA_EXHAUSTED));
        assert_eq!(rep["aborted"]["not_run"], json!(["engdesign/XG_13"]));
    }

    #[tokio::test]
    async fn mock_design_submits_offline() {
        let b = Bridge::new();
        let p = crate::provider::make("mock:design").unwrap();
        let rep = run_design(p.as_ref(), &b, &[task("ZC_02"), task("XG_13")], &opts(4), |_| {}).await;
        assert_eq!(rep["model"], "design");
        assert_eq!(rep["results"][0]["stopped"], "submitted");
        assert_eq!(rep["results"][0]["pass"], true, "{}", rep["results"][0]["details"]);
        assert_eq!(rep["results"][1]["submitted"], true);
        assert_eq!(rep["results"][1]["submission"]["Kp"], 1.0);
        assert!(rep["aborted"].is_null());
    }

    /// The EngDesign tasks that were blocked on hand-multiplied polynomials can be written as
    /// operator calls: pid / pade → series → feedback → stepinfo. No network, no API key.
    #[tokio::test]
    async fn reference_solutions_use_the_new_operators() {
        let b = Bridge::new();
        let plant = json!({"kind":"tf","num":[1.0],"den":[1.0,10.0,20.0],"ts":0.0,"time_unit":"s"});
        let (pid, _) = b.call("control_design_pid", &json!({"Kp": 800.0, "Ki": 2000.0, "Kd": 60.0}));
        assert!(!is_error(&pid), "{}", pid["error"]);
        let (open, _) = b.call("control_lti_series", &json!({"sys1": pid["result"]["system"], "sys2": plant}));
        assert!(!is_error(&open), "{}", open["error"]);
        let (closed, _) = b.call("control_lti_feedback", &json!({"sys1": open["result"]["system"]}));
        assert!(!is_error(&closed), "{}", closed["error"]);
        assert_eq!(closed["result"]["composition"]["sign"], -1.0);
        assert_eq!(closed["result"]["composition"]["implicit_unity"], true);
        let (info, _) = b.call("control_lti_stepinfo", &json!({"system": closed["result"]["system"], "time": {"mode":"uniform","t_final":2.0,"dt":0.0005,"unit":"s"}}));
        assert!(!is_error(&info), "{}", info["error"]);
        let ts = info["result"]["info"]["settling_time"]["value"].as_f64().unwrap();
        let os = info["result"]["info"]["overshoot"]["value"].as_f64().unwrap();
        assert!(info["result"]["stable"] == true && ts < 0.2 && os < 5.0, "XG_13 chain Ts={ts} OS={os}");

        // Ziheng_03: PID on 1/((0.5s+1)(s+1)(2s+1)), then the same loop, must come back stable.
        let (pidz, _) = b.call("control_design_pid", &json!({"Kp": 10.0, "Ki": 1.55, "Kd": 9.5}));
        let g = json!({"kind":"tf","num":[1.0],"den":[1.0, 3.5, 3.5, 1.0],"ts":0.0});
        // (0.5s+1)(s+1)(2s+1) = (0.5s+1)(2s^2+3s+1) = s^3 + 3.5 s^2 + 3.5 s + 1
        let (oz, _) = b.call("control_lti_series", &json!({"sys1": pidz["result"]["system"], "sys2": g}));
        let (cz, _) = b.call("control_lti_feedback", &json!({"sys1": oz["result"]["system"]}));
        assert!(!is_error(&cz), "{}", cz["error"]);
        let (iz, _) = b.call("control_lti_stepinfo", &json!({"system": cz["result"]["system"], "time": {"mode":"uniform","t_final":20.0,"dt":0.01,"unit":"s"}, "settling_threshold": 0.05}));
        assert!(iz["result"]["stable"] == true, "{}", iz);

        // XG_05: Padé of the identified delay, then series with 1/(τs+1). Order is supplied, not defaulted.
        let (pd, _) = b.call("control_lti_pade", &json!({"delay": 16.5, "order": 10}));
        assert!(!is_error(&pd), "{}", pd["error"]);
        assert!(pd["result"]["approximation"]["certificate"]["value"].as_f64().unwrap() >= 0.0);
        assert!(pd["result"]["approximation"]["bound"].is_null());
        let lag = json!({"kind":"tf","num":[1.0],"den":[19.5, 1.0],"ts":0.0});
        let (plant5, _) = b.call("control_lti_series", &json!({"sys1": pd["result"]["system"], "sys2": lag}));
        assert!(!is_error(&plant5), "{}", plant5["error"]);

        // Offline design loop: the same XG_13 chain, then the reference submission, must pass.
        let script = json!([
            {"calls":[{"name":"control_design_pid","args":{"Kp":800.0,"Ki":2000.0,"Kd":60.0}}]},
            {"calls":[{"name":"control_lti_series","args":{"sys1": pid["result"]["system"], "sys2": plant}}]},
            {"calls":[{"name":"control_lti_feedback","args":{"sys1": open["result"]["system"]}}]},
            {"calls":[{"name":"control_lti_stepinfo","args":{"system": closed["result"]["system"], "time":{"mode":"uniform","t_final":2.0,"dt":0.001,"unit":"s"}}}]},
            {"calls":[{"name":"submit_design","args":{"answer": reference("XG_13"), "summary":"链式检查后提交参考增益"}}]}
        ]);
        let path = std::env::temp_dir().join(format!("aeoi-design-chain-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_string(&script).unwrap()).unwrap();
        let p = crate::provider::ScriptMock::from_file(path.to_str().unwrap()).unwrap();
        let rep = run_design(&p, &b, &[task("XG_13")], &opts(8), |_| {}).await;
        let r = &rep["results"][0];
        assert_eq!(r["stopped"], "submitted", "{r}");
        assert!(r["tool_errors"].as_array().unwrap().is_empty(), "{}", r["tool_errors"]);
        assert!(r["tool_calls"].as_array().unwrap().iter().any(|t| t == "control_design_pid"));
        assert!(r["tool_calls"].as_array().unwrap().iter().any(|t| t == "control_lti_stepinfo"));
        assert_eq!(r["pass"], true, "{}", r["details"]);
        let _ = std::fs::remove_file(&path);
    }
}
