// In-browser backend for the GitHub Pages build of the AEOI test bench.
//
// The 12 operators and the tool bridge (function declarations, F-5 grounding check, error
// envelopes, provider wire formats) are the Rust code of aeoi-bridge/aeoi-control compiled to
// WebAssembly (rust/aeoi-wasm). This file only does what the axum server + reqwest do natively:
// the agent loop (mirrors rust/aeoi-web/src/agent.rs) and HTTP to the model provider.
//
// Keys: entered by the visitor, kept in localStorage only, sent only to the provider's own API
// (generativelanguage.googleapis.com / openrouter.ai), never logged.
import init, * as W from "./pkg/aeoi_wasm.js";

const KEY = { gemini: "aeoi.key.gemini", openrouter: "aeoi.key.openrouter" };
const getKey = (p) => { try { return localStorage.getItem(KEY[p]) || ""; } catch { return ""; } };
const setKey = (p, v) => { try { v ? localStorage.setItem(KEY[p], v) : localStorage.removeItem(KEY[p]); } catch { /* storage disabled */ } };
const GEMINI = "https://generativelanguage.googleapis.com/v1beta";
const OPENROUTER = "https://openrouter.ai/api/v1";
const MAX_ARRAY_FOR_MODEL = 120;
const BASE_MODELS = [
  "mock:demo",
  "gemini:gemini-3.1-flash-lite",
  "gemini:gemini-2.5-flash",
  "gemini:gemini-3.5-flash",
  "openrouter:nvidia/nemotron-3-ultra-550b-a55b:free",
  "openrouter:nvidia/nemotron-3.5-super-vl:free",
  "openrouter:particle-ai/particle-1:free",
];

function splitSpec(spec) {
  const i = spec.indexOf(":");
  const p = i > 0 ? spec.slice(0, i) : "";
  if (["gemini", "openrouter", "mock"].includes(p)) return [p, spec.slice(i + 1)];
  return ["gemini", spec];
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// POST JSON with rate-limit-friendly retries (429 / 408 / 5xx); a daily quota fails fast.
async function postJson(url, headers, body, maxRetries = 4) {
  for (let attempt = 0; ; attempt++) {
    let resp, text;
    try {
      resp = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json", ...headers }, body });
      text = await resp.text();
    } catch (e) {
      if (attempt >= maxRetries) throw new Error("网络错误：" + e.message);
      await sleep(2000 * 2 ** attempt);
      continue;
    }
    if (resp.ok) return { text, retries: attempt };
    let j = {};
    try { j = JSON.parse(text); } catch { /* not JSON */ }
    const msg = (j.error && (j.error.message || JSON.stringify(j.error))) || text.slice(0, 300);
    const retryable = resp.status === 429 || resp.status === 408 || resp.status >= 500;
    const low = text.toLowerCase();
    if (resp.status === 429 && (low.includes("perday") || low.includes("per-day") || low.includes("per day"))) throw new Error(`每日免费额度已用完（HTTP 429）：${msg}`);
    if (!retryable || attempt >= maxRetries) throw new Error(`HTTP ${resp.status}：${msg}`);
    let delay = 2000 * 2 ** attempt;
    for (const d of (j.error && j.error.details) || []) if (d.retryDelay) delay = Math.max(delay, parseFloat(d.retryDelay) * 1000);
    await sleep(Math.min(delay, 30000));
  }
}

async function chatOnce(sess, provider, model, history) {
  if (provider === "mock") {
    if (!sess.script.length) throw new Error("mock 脚本已用完（点“新会话”重新开始演示）");
    return { ...sess.script.shift(), retries: 0 };
  }
  const body = W.build_request(provider, model, JSON.stringify(history));
  let r;
  if (provider === "gemini") {
    const key = getKey("gemini");
    if (!key) throw new Error("没有 Gemini API key：点右上角“API key”填写，或改用 mock:demo。");
    r = await postJson(`${GEMINI}/models/${encodeURIComponent(model)}:generateContent`, { "x-goog-api-key": key }, body);
  } else {
    const key = getKey("openrouter");
    if (!key) throw new Error("没有 OpenRouter API key：点右上角“API key”填写，或改用 mock:demo。");
    r = await postJson(`${OPENROUTER}/chat/completions`, { Authorization: `Bearer ${key}`, "HTTP-Referer": location.origin + location.pathname, "X-Title": "AEOI test bench (GitHub Pages)" }, body);
  }
  const parsed = JSON.parse(W.parse_response(provider, r.text));
  parsed.retries = r.retries;
  return parsed;
}

function countSelfCorrections(steps) {
  let n = 0;
  steps.forEach((s, i) => {
    if (!s.is_error) return;
    const rest = steps.slice(i + 1);
    const okIdx = rest.findIndex((t) => t.tool === s.tool && !t.is_error);
    if (okIdx < 0) return;
    if (!rest.slice(0, okIdx).some((t) => t.tool === s.tool && t.is_error)) n++;
  });
  return n;
}

const sessions = new Map();

async function runAgent(req, emit) {
  const [provider, model] = splitSpec(req.model);
  let sess = sessions.get(req.session_id);
  if (!sess || sess.model !== req.model) {
    sess = { model: req.model, history: [], script: provider === "mock" ? JSON.parse(W.demo_script()) : [] };
    sessions.set(req.session_id, sess);
  }
  const maxSteps = Math.min(Math.max(req.max_steps || 8, 1), 20);
  const history = sess.history;
  emit({ type: "start", model: req.model, history_len: history.length });
  history.push({ role: "user", text: req.message });
  const tools = [];
  let step = 0, nudges = 0, totalTokens = 0, stopped = "";
  for (;;) {
    if (step >= maxSteps) { stopped = "max_steps"; emit({ type: "stopped", reason: "max_steps", max_steps: maxSteps }); break; }
    step++;
    emit({ type: "thinking", step });
    const t0 = performance.now();
    let resp;
    try { resp = await chatOnce(sess, provider, model, history); } catch (e) { stopped = "provider_error"; emit({ type: "error", step, message: String(e.message || e) }); break; }
    const turn = { step, text: resp.text ?? null, tool_calls: (resp.calls || []).map((c) => c.name), latency_ms: performance.now() - t0, usage: resp.usage || {}, model_version: resp.model_version ?? null, finish_reason: resp.finish_reason ?? null, http_retries: resp.retries || 0 };
    totalTokens += turn.usage.total_tokens || 0;
    emit({ type: "model", turn });
    const calls = resp.calls || [];
    if (!calls.length && !resp.text && nudges < 2) {
      nudges++;
      const why = resp.finish_reason || "EMPTY";
      emit({ type: "nudge", step, reason: why });
      history.push({ role: "user", text: W.nudge_text(why) });
      continue;
    }
    history.push({ role: "assistant", text: resp.text ?? null, calls, raw: resp.raw ?? null });
    if (!calls.length) { stopped = "answered"; emit({ type: "final", text: resp.text }); break; }
    for (const c of calls) {
      const r = JSON.parse(W.call_tool(c.name, JSON.stringify(c.args ?? {}), JSON.stringify(history), MAX_ARRAY_FOR_MODEL));
      const ts = { step, tool: c.name, operator_id: r.operator_id, call_id: c.id, request: c.args, response: r.envelope, is_error: r.is_error, error_code: r.error_code, latency_ms: r.latency_ms };
      tools.push(ts);
      emit({ type: "tool", step: ts });
      history.push({ role: "tool", call_id: c.id, name: c.name, response: r.for_model });
    }
  }
  emit({ type: "done", self_corrections: countSelfCorrections(tools), total_tokens: totalTokens, stopped });
}

async function geminiModels() {
  const key = getKey("gemini");
  if (!key) return [];
  try {
    const r = await fetch(`${GEMINI}/models?pageSize=200`, { headers: { "x-goog-api-key": key } });
    if (!r.ok) return [];
    const j = await r.json();
    return (j.models || []).filter((m) => (m.supportedGenerationMethods || []).includes("generateContent")).map((m) => m.name.replace(/^models\//, "")).filter((n) => n.startsWith("gemini") && !/tts|image|embedding/.test(n)).map((n) => "gemini:" + n);
  } catch { return []; }
}

function keysDialog() {
  const dlg = document.getElementById("keys");
  const status = document.getElementById("keys-status");
  const show = () => {
    for (const p of ["gemini", "openrouter"]) document.getElementById("key-" + p).value = "";
    status.textContent = "当前已保存：" + (["gemini", "openrouter"].filter(getKey).join("、") || "无");
    dlg.showModal();
  };
  document.getElementById("keys-btn").addEventListener("click", show);
  dlg.addEventListener("close", () => {
    if (dlg.returnValue === "save") {
      for (const p of ["gemini", "openrouter"]) { const v = document.getElementById("key-" + p).value.trim(); if (v) setKey(p, v); }
    } else if (dlg.returnValue === "forget") {
      for (const p of ["gemini", "openrouter"]) setKey(p, "");
    }
    for (const p of ["gemini", "openrouter"]) document.getElementById("key-" + p).value = "";
    if (dlg.returnValue === "save" || dlg.returnValue === "forget") location.reload();
  });
}

async function boot() {
  const bootEl = document.getElementById("boot");
  try {
    const t0 = performance.now();
    await init({ module_or_path: new URL("./pkg/aeoi_wasm_bg.wasm", import.meta.url) });
    const info = JSON.parse(W.info());
    const toolList = JSON.parse(W.tools());
    bootEl.remove();
    keysDialog();
    const bi = document.getElementById("build-info");
    if (bi) bi.textContent = `WebAssembly 加载 ${(performance.now() - t0).toFixed(0)} ms · ${info.spec} · aeoi ${info.version} · 平台 ${info.platform} · ${info.tools} 个工具`;
    const backend = {
      kind: "browser",
      async config() {
        const extra = await geminiModels();
        const models = [...new Set([...BASE_MODELS, ...extra])];
        const hasG = !!getKey("gemini"), hasO = !!getKey("openrouter");
        return {
          models, default_model: hasG ? "gemini:gemini-3.1-flash-lite" : hasO ? BASE_MODELS[4] : "mock:demo", max_steps: 8,
          providers: [{ provider: "mock", configured: true }, { provider: "gemini", configured: hasG }, { provider: "openrouter", configured: hasO }],
          spec: info.spec, tools: toolList, system_prompt: W.system_prompt(), note: "浏览器内运行（WebAssembly）",
        };
      },
      tools: async () => toolList,
      reset: async (sid) => { sessions.delete(sid); },
      chat: runAgent,
    };
    window.__aeoiBackend.res(backend);
  } catch (e) {
    bootEl.className = "boot err";
    bootEl.textContent = "WebAssembly 加载失败：" + (e.message || e);
    window.__aeoiBackend.rej(e);
  }
}
boot();
