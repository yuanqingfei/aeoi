// AEOI × LLM test bench — plain JS, no build step.
"use strict";
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const fmtMs = (ms) => (ms >= 1000 ? (ms / 1000).toFixed(2) + " s" : ms.toFixed(ms < 10 ? 2 : 0) + " ms");
let sessionId = newSession();
let busy = false;
let plotSeq = 0;

// ---------------------------------------------------------------- backends
// The same page runs against the local axum server (aeoi-web: /api/*, SSE) or, on GitHub Pages,
// against an in-browser backend (pages/browser.js: operators in WebAssembly, LLM called directly
// with the visitor's own key). The browser backend publishes itself as window.AEOI_BACKEND_READY.
const serverBackend = {
  kind: "server",
  config: () => fetch("/api/config").then((r) => r.json()),
  tools: () => fetch("/api/tools").then((r) => r.json()),
  reset: (sid) => fetch("/api/reset", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ session_id: sid }) }),
  async chat(req, onEvent) {
    const resp = await fetch("/api/chat", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(req) });
    if (!resp.ok) { const e = await resp.json().catch(() => ({})); throw new Error(e.error || resp.statusText); }
    const reader = resp.body.getReader();
    const dec = new TextDecoder();
    let buf = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += dec.decode(value, { stream: true });
      let i;
      while ((i = buf.indexOf("\n\n")) >= 0) {
        const chunk = buf.slice(0, i); buf = buf.slice(i + 2);
        const data = chunk.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).replace(/^ /, "")).join("\n");
        if (data) onEvent(JSON.parse(data));
      }
    }
  },
};
let B = serverBackend;

function newSession() { return "s-" + Math.random().toString(36).slice(2) + Date.now().toString(36); }

const EXAMPLES = [
  "开环传递函数 G(s) = 4/(s(s+1)(s+2))，求增益裕度和相位裕度。",
  "G(s)=4/(s^2+1.2s+4) 的单位阶跃响应超调量和调节时间是多少？画出响应曲线。",
  "画出 G(s)=10/(s(s+1)) 在 0.01–100 rad/s 的 Bode 图，并说明相位裕度。",
  "双积分器 A=[[0,1],[0,0]], B=[[0],[1]]，Q=I，R=1，求 LQR 增益 K。",
  "G(s)=1/(s+1)，采样周期 0.1 s，ZOH 离散化。",
  "MATLAB 中 A(:) = [0,-2,1,-3]（2×2，按列），B=[0;1]，求极点配置到 −5、−6 的 K。",
  "G(s)=1/(s^2+2s+5) 的冲激响应。",
];

// ---------------------------------------------------------------- minimal markdown
function md(text) {
  const lines = esc(text).split(/\n/);
  let html = "", inList = false, inCode = false;
  for (let l of lines) {
    if (l.trim().startsWith("```")) { html += inCode ? "</pre>" : "<pre>"; inCode = !inCode; continue; }
    if (inCode) { html += l + "\n"; continue; }
    l = l.replace(/\*\*(.+?)\*\*/g, "<b>$1</b>").replace(/`([^`]+)`/g, "<code>$1</code>");
    const li = l.match(/^\s*(?:[-*]|\d+\.)\s+(.*)$/);
    if (li) { if (!inList) { html += "<ul>"; inList = true; } html += "<li>" + li[1] + "</li>"; continue; }
    if (inList) { html += "</ul>"; inList = false; }
    const h = l.match(/^(#{1,4})\s+(.*)$/);
    if (h) { html += `<div><b>${h[2]}</b></div>`; continue; }
    html += l.trim() === "" ? "<div style='height:6px'></div>" : `<div>${l}</div>`;
  }
  if (inList) html += "</ul>";
  if (inCode) html += "</pre>";
  return html;
}

// ---------------------------------------------------------------- chat
function addMsg(kind, html, meta) {
  const d = document.createElement("div");
  d.className = "msg " + kind;
  d.innerHTML = html + (meta ? `<div class="meta">${meta}</div>` : "");
  $("#chat").appendChild(d);
  $("#chat").scrollTop = 1e9;
  return d;
}

function currentModel() { return $("#model-custom").value.trim() || $("#model").value; }

async function send(text) {
  if (busy || !text.trim()) return;
  busy = true; $("#send").disabled = true;
  addMsg("user", md(text));
  const pending = addMsg("bot", '<span class="spinner"></span> 正在思考…');
  const traceHeader = document.createElement("div");
  traceHeader.className = "muted";
  traceHeader.style.margin = "14px 0 6px";
  traceHeader.textContent = "— 问题：" + text.slice(0, 80);
  const trace = $("#trace");
  if (trace.querySelector("p.muted")) trace.innerHTML = "";
  trace.appendChild(traceHeader);
  const t0 = performance.now();
  let tokens = 0, toolsN = 0, gotFinal = false;
  try {
    await B.chat({ session_id: sessionId, model: currentModel(), message: text, max_steps: +$("#max-steps").value || 8 }, (ev) => {
      if (ev.type === "thinking") pending.innerHTML = `<span class="spinner"></span> 第 ${ev.step} 步：等待模型…`;
      else if (ev.type === "model") { tokens += ev.turn.usage.total_tokens || 0; renderModelTurn(ev.turn); if (ev.turn.tool_calls.length) pending.innerHTML = `<span class="spinner"></span> 调用 ${ev.turn.tool_calls.map(esc).join(", ")}…`; }
      else if (ev.type === "tool") { toolsN++; renderTool(ev.step); }
      else if (ev.type === "nudge") { $("#trace").insertAdjacentHTML("beforeend", `<div class="card"><h4><span class="badge warn">空回合</span> 模型第 ${ev.step} 步没有输出（${esc(ev.reason)}），已提示其按 schema 重试或向用户提问</h4></div>`); }
      else if (ev.type === "final") { gotFinal = true; pending.innerHTML = md(ev.text || "（模型没有给出文本回答）"); }
      else if (ev.type === "error") { pending.className = "msg err"; pending.innerHTML = "模型调用失败：" + esc(ev.message); gotFinal = true; }
      else if (ev.type === "stopped") { pending.className = "msg err"; pending.innerHTML = `达到最多步数（${ev.max_steps}）仍未得到最终回答。可调大“最多步数”后重试。`; gotFinal = true; }
      else if (ev.type === "done") {
        const meta = `${currentModel()} · ${toolsN} 次工具调用 · 自我纠错 ${ev.self_corrections} 次 · ${tokens || "?"} tokens · ${((performance.now() - t0) / 1000).toFixed(1)} s`;
        pending.insertAdjacentHTML("beforeend", `<div class="meta">${esc(meta)}</div>`);
      }
      $("#trace-col .tab.active").scrollTop = 1e9;
    });
    if (!gotFinal) pending.innerHTML = "（连接已结束，没有收到回答）";
  } catch (e) {
    pending.className = "msg err"; pending.textContent = "请求失败：" + e.message;
  } finally { busy = false; $("#send").disabled = false; $("#chat").scrollTop = 1e9; }
}

// ---------------------------------------------------------------- trace rendering
function usageStr(u) {
  if (!u) return "";
  const p = [];
  if (u.prompt_tokens != null) p.push(`输入 ${u.prompt_tokens}`);
  if (u.completion_tokens != null) p.push(`输出 ${u.completion_tokens}`);
  if (u.thought_tokens) p.push(`思考 ${u.thought_tokens}`);
  if (u.total_tokens != null) p.push(`合计 ${u.total_tokens}`);
  return p.length ? p.join(" / ") + " tokens" : "无 token 统计";
}

function renderModelTurn(t) {
  const c = document.createElement("div");
  c.className = "card";
  const calls = t.tool_calls.length ? `→ 调用 ${t.tool_calls.map((x) => `<code>${esc(x)}</code>`).join(", ")}` : "→ 最终回答";
  c.innerHTML = `<h4><span class="badge model">模型 · 第 ${t.step} 步</span> ${calls}</h4>
    <div class="kv">耗时 <b>${fmtMs(t.latency_ms)}</b> · ${usageStr(t.usage)}${t.model_version ? " · " + esc(t.model_version) : ""}${t.finish_reason ? " · " + esc(t.finish_reason) : ""}${t.http_retries ? ` · <span class="badge warn">HTTP 重试 ${t.http_retries}</span>` : ""}</div>
    ${t.text && t.tool_calls.length ? `<details><summary>伴随文本</summary><div>${md(t.text)}</div></details>` : ""}`;
  $("#trace").appendChild(c);
}

function quantityStr(q) {
  if (q === null || q === undefined) return "null";
  if (typeof q !== "object") return esc(JSON.stringify(q));
  if ("unit" in q && "value" in q) {
    const v = q.value;
    if (v && typeof v === "object" && "data" in v) return arrStr(v) + " " + esc(q.unit);
    return esc(typeof v === "number" ? +v.toPrecision(8) : JSON.stringify(v)) + " " + esc(q.unit);
  }
  if ("data" in q && "shape" in q) return arrStr(q);
  return null;
}
function arrStr(m) {
  const d = m.data || [];
  const fmt = (x) => (Array.isArray(x) ? `${+x[0].toPrecision(6)}${x[1] >= 0 ? "+" : "−"}${Math.abs(+x[1].toPrecision(6))}j` : typeof x === "number" ? +x.toPrecision(6) : JSON.stringify(x));
  const head = d.slice(0, 12).map(fmt).join(", ");
  return `<span class="muted">[${(m.shape || []).join("×")}]</span> ${esc(head)}${d.length > 12 ? ` … (${d.length} 个)` : ""}`;
}
function resultTable(r) {
  if (!r || typeof r !== "object") return "";
  const rows = [];
  const walk = (o, path, depth) => {
    for (const [k, v] of Object.entries(o)) {
      const p = path ? path + "." + k : k;
      const qs = quantityStr(v);
      if (qs !== null && (typeof v !== "object" || v === null || "unit" in v || "data" in v)) rows.push(`<tr><td><code>${esc(p)}</code></td><td>${qs}</td></tr>`);
      else if (v && typeof v === "object" && !Array.isArray(v) && depth < 3) walk(v, p, depth + 1);
      else rows.push(`<tr><td><code>${esc(p)}</code></td><td>${esc(JSON.stringify(v).slice(0, 160))}</td></tr>`);
    }
  };
  walk(r, "", 0);
  return `<table class="res">${rows.join("")}</table>`;
}

function renderTool(s) {
  const env = s.response || {};
  const ok = !s.is_error;
  const c = document.createElement("div");
  c.className = "card";
  const warns = env.warnings || [];
  let body = `<h4><span class="badge ${ok ? "ok" : "bad"}">${ok ? "成功" : "错误"}</span> <code>${esc(s.tool)}</code>
    <span class="muted">${esc(s.operator_id || "")}</span> <span class="kv">耗时 <b>${fmtMs(s.latency_ms)}</b></span>
    ${warns.length ? `<span class="badge warn">${warns.length} 条警告</span>` : ""}</h4>`;
  body += `<details><summary>请求 JSON（模型生成的参数）</summary><pre>${esc(JSON.stringify(s.request, null, 2))}</pre></details>`;
  if (!ok && env.error) {
    const e = env.error;
    body += `<div class="err-box"><b>${esc(e.code)}</b>：${esc(e.message)}${e.agent_hint ? `<br><span class="muted">agent_hint：</span>${esc(e.agent_hint)}` : ""}${e.details && e.details.path ? `<br><span class="muted">path：</span><code>${esc(e.details.path)}</code>` : ""}</div>`;
    if (e.remediation && e.remediation.length) body += `<details open><summary>remediation（已回传给模型）</summary><pre>${esc(JSON.stringify(e.remediation, null, 2))}</pre></details>`;
  }
  for (const w of warns) body += `<div class="warn-box"><b>${esc(w.code)}</b>：${esc(w.message)}</div>`;
  if (ok && env.result) {
    const pid = "plot-" + (++plotSeq);
    const plot = plotKind(env.result);
    if (plot) body += `<div id="${pid}" class="plot"></div>`;
    body += `<details open><summary>result</summary>${resultTable(env.result)}</details>`;
    c.dataset.plot = pid;
    setTimeout(() => plot && drawPlot(pid, plot, env.result, s.tool), 0);
  }
  if (env.trust) body += `<details><summary>trust（可信度元数据）</summary><pre>${esc(JSON.stringify(env.trust, null, 2))}</pre></details>`;
  body += `<details><summary>完整 AEOI 响应信封</summary><pre>${esc(JSON.stringify(env, null, 2))}</pre></details>`;
  c.innerHTML = body;
  $("#trace").appendChild(c);
}

// ---------------------------------------------------------------- plots
function plotKind(r) {
  if (r.w && r.magnitude && r.phase) return "bode";
  if (r.t && r.y && r.y.value && r.y.value.data && r.y.value.data.length > 1) return "time";
  return null;
}
function channels(q) {
  // value: {shape:[ny,nu,n], data:[...]} row-major
  const m = q.value, sh = m.shape, d = m.data, n = sh[sh.length - 1];
  const out = [];
  const nch = d.length / n;
  for (let c = 0; c < nch; c++) {
    const label = sh.length === 3 ? `y${Math.floor(c / sh[1]) + 1}←u${(c % sh[1]) + 1}` : `ch${c + 1}`;
    out.push({ label, y: d.slice(c * n, (c + 1) * n) });
  }
  return out;
}
// Plotly (vendored, loaded with defer); wait for it a little, then fall back to a built-in SVG plot.
function whenPlotly(timeoutMs) {
  return new Promise((res) => {
    const t0 = performance.now();
    (function poll() { if (window.Plotly) return res(true); if (performance.now() - t0 > timeoutMs) return res(false); setTimeout(poll, 100); })();
  });
}
function svgPlot(el, panels) {
  // panels: [{title, xlog, xlabel, ylabel, series:[{x, y}]}]
  const W = el.clientWidth || 600, H = 200, m = { l: 56, r: 10, t: 18, b: 30 };
  const colors = ["#2f5bd8", "#d9534f", "#2a9d58", "#e0a800", "#7a3fd8"];
  let html = "";
  for (const p of panels) {
    const xs = p.series.flatMap((s) => s.x).map((x) => (p.xlog ? Math.log10(x) : x)), ys = p.series.flatMap((s) => s.y).filter(Number.isFinite);
    let [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    if (y1 - y0 < 1e-12) { y0 -= 1; y1 += 1; }
    const X = (x) => m.l + ((p.xlog ? Math.log10(x) : x) - x0) / (x1 - x0 || 1) * (W - m.l - m.r);
    const Y = (y) => H - m.b - (y - y0) / (y1 - y0) * (H - m.t - m.b);
    const paths = p.series.map((s, i) => `<path fill="none" stroke="${colors[i % 5]}" stroke-width="1.5" d="${s.x.map((x, k) => (k ? "L" : "M") + X(x).toFixed(1) + "," + Y(s.y[k]).toFixed(1)).join("")}"/>`).join("");
    const fmt = (v) => +v.toPrecision(3);
    const xt = p.xlog ? [x0, x1].map((v) => "1e" + fmt(v)) : [fmt(x0), fmt(x1)];
    html += `<svg width="${W}" height="${H}" style="display:block"><text x="${m.l}" y="12" font-size="11" font-weight="600">${esc(p.title)}</text>
      <rect x="${m.l}" y="${m.t}" width="${W - m.l - m.r}" height="${H - m.t - m.b}" fill="none" stroke="#ccd3e0"/>${paths}
      <text x="${m.l - 4}" y="${m.t + 8}" font-size="10" text-anchor="end">${fmt(y1)}</text><text x="${m.l - 4}" y="${H - m.b}" font-size="10" text-anchor="end">${fmt(y0)}</text>
      <text x="${m.l}" y="${H - 14}" font-size="10">${xt[0]}</text><text x="${W - m.r}" y="${H - 14}" font-size="10" text-anchor="end">${xt[1]}</text>
      <text x="${(W + m.l) / 2}" y="${H - 3}" font-size="10" text-anchor="middle">${esc(p.xlabel)}</text>
      <text x="12" y="${(H - m.b + m.t) / 2}" font-size="10" transform="rotate(-90 12 ${(H - m.b + m.t) / 2})" text-anchor="middle">${esc(p.ylabel)}</text></svg>`;
  }
  el.style.height = "auto";
  el.innerHTML = html + '<div class="muted">（Plotly 未能加载，使用内置简易图）</div>';
}
async function drawPlot(id, kind, r, tool) {
  const el = document.getElementById(id);
  if (!el) return;
  if (!(await whenPlotly(6000))) {
    if (kind === "bode") {
      const w = r.w.value.data;
      svgPlot(el, [
        { title: "Bode 幅值", xlog: true, xlabel: `ω (${r.w.unit})`, ylabel: "dB", series: channels(r.magnitude).map((c) => ({ x: w, y: c.y })) },
        { title: "Bode 相位", xlog: true, xlabel: `ω (${r.w.unit})`, ylabel: "deg", series: channels(r.phase).map((c) => ({ x: w, y: c.y })) },
      ]);
    } else {
      const t = r.t.value.data;
      svgPlot(el, [{ title: tool.includes("impulse") ? "冲激响应" : "阶跃响应", xlog: false, xlabel: `t (${r.t.unit})`, ylabel: "y", series: channels(r.y).map((c) => ({ x: t, y: c.y })) }]);
    }
    return;
  }
  const layout = { margin: { l: 55, r: 15, t: 28, b: 40 }, font: { size: 11 }, showlegend: false };
  if (kind === "bode") {
    const w = r.w.value.data;
    const mag = channels(r.magnitude), ph = channels(r.phase);
    const traces = [];
    mag.forEach((c) => traces.push({ x: w, y: c.y, name: "|G| " + c.label, xaxis: "x", yaxis: "y", mode: "lines" }));
    ph.forEach((c) => traces.push({ x: w, y: c.y, name: "∠G " + c.label, xaxis: "x2", yaxis: "y2", mode: "lines" }));
    if (w.length < 8) traces.forEach((t) => (t.mode = "lines+markers"));
    Object.assign(layout, {
      title: { text: "Bode 图", font: { size: 12 } }, grid: { rows: 2, columns: 1, pattern: "independent" },
      xaxis: { type: "log", title: "" }, yaxis: { title: "幅值 (dB)" },
      xaxis2: { type: "log", title: `ω (${r.w.unit})` }, yaxis2: { title: "相位 (deg)" },
    });
    el.style.height = "420px";
    Plotly.newPlot(el, traces, layout, { responsive: true, displaylogo: false });
  } else {
    const t = r.t.value.data;
    const traces = channels(r.y).map((c) => ({ x: t, y: c.y, name: c.label, mode: t.length < 30 ? "lines+markers" : "lines" }));
    const title = tool.includes("impulse") ? "冲激响应" : tool.includes("step") ? "阶跃响应" : "时间响应";
    Object.assign(layout, { title: { text: title, font: { size: 12 } }, xaxis: { title: `t (${r.t.unit})` }, yaxis: { title: "y" } });
    if (r.info && r.info.steady_state && typeof r.info.steady_state.value === "number") {
      layout.shapes = [{ type: "line", x0: t[0], x1: t[t.length - 1], y0: r.info.steady_state.value, y1: r.info.steady_state.value, line: { dash: "dot", color: "#999", width: 1 } }];
    }
    Plotly.newPlot(el, traces, layout, { responsive: true, displaylogo: false });
  }
}

// ---------------------------------------------------------------- init
async function init() {
  if (window.AEOI_BACKEND_READY) B = await window.AEOI_BACKEND_READY;
  const cfg = await B.config();
  const sel = $("#model");
  for (const m of cfg.models) { const o = document.createElement("option"); o.value = o.textContent = m; sel.appendChild(o); }
  sel.value = cfg.default_model;
  $("#max-steps").value = cfg.max_steps;
  $("#providers").textContent = (cfg.note ? cfg.note + " · " : "") + "已配置：" + (cfg.providers.filter((p) => p.configured).map((p) => p.provider).join(", ") || "无") + ` · ${cfg.spec} · ${cfg.tools.length} 个工具`;
  $("#prompt").textContent = cfg.system_prompt;
  const tools = await B.tools();
  $("#tab-tools").innerHTML = tools.map((t) => `<div class="card"><h4><code>${esc(t.name)}</code><span class="muted">${esc(t.operator_id)}</span>${t.lossy_mappings.length ? `<span class="badge warn">${t.lossy_mappings.length} 处有损映射</span>` : ""}</h4>
      <div class="kv">${esc(t.description)}</div>
      <details><summary>Gemini parameters（OpenAPI 子集）</summary><pre>${esc(JSON.stringify(t.gemini_schema, null, 2))}</pre></details>
      <details><summary>JSON Schema（OpenAI 兼容 provider）</summary><pre>${esc(JSON.stringify(t.json_schema, null, 2))}</pre></details>
      <details><summary>有损映射明细</summary><pre>${esc(t.lossy_mappings.map((l) => l.path + "  " + l.what).join("\n"))}</pre></details></div>`).join("");
  // deep link: /?model=mock:demo&ask=... preselects a model and sends a question (demos, smoke tests)
  const qp = new URLSearchParams(location.search);
  if (qp.get("model")) $("#model-custom").value = qp.get("model");
  if (qp.get("ask")) setTimeout(() => send(qp.get("ask")), 0);
  for (const ex of EXAMPLES) { const b = document.createElement("button"); b.textContent = ex.length > 28 ? ex.slice(0, 28) + "…" : ex; b.title = ex; b.onclick = () => { $("#input").value = ex; $("#input").focus(); }; $("#examples").appendChild(b); }
}

$("#ask").addEventListener("submit", (e) => { e.preventDefault(); const t = $("#input").value; $("#input").value = ""; send(t); });
$("#input").addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); $("#ask").requestSubmit(); } });
$("#reset").addEventListener("click", async () => {
  await B.reset(sessionId);
  sessionId = newSession(); $("#chat").innerHTML = ""; $("#trace").innerHTML = '<p class="muted">新会话。</p>';
});
document.querySelectorAll(".tabs button").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll(".tabs button, .tab").forEach((x) => x.classList.remove("active"));
  b.classList.add("active"); $("#tab-" + b.dataset.tab).classList.add("active");
}));
init();
