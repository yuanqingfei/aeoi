#!/usr/bin/env python3
"""Scripted MCP stdio session against rust/target/release/aeoi-mcp; writes reports/mcp-session.jsonl."""
import json, subprocess, pathlib
root = pathlib.Path(__file__).resolve().parent
p = subprocess.Popen([str(root / "rust/target/release/aeoi-mcp")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
log = []
def send(msg, expect=True):
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush(); log.append({"->": msg})
    if expect:
        r = json.loads(p.stdout.readline()); log.append({"<-": r}); return r
ex = lambda f: json.load(open(root / "examples" / f))
r = send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "demo", "version": "0"}}})
print("initialize:", r["result"]["protocolVersion"], r["result"]["serverInfo"])
send({"jsonrpc": "2.0", "method": "notifications/initialized"}, expect=False)
r = send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
print("tools:", [t["name"] for t in r["result"]["tools"]])
calls = [("control_lti_bode", "control.lti.bode/request.json"), ("control_lti_step", "control.lti.step/request.json"),
         ("control_design_lqr", "control.design.lqr/request.json"), ("control_lti_bode", "control.lti.bode/request.error.json")]
# batch 2 (spec v0.3): one success call per operator plus two error calls
for op in ["control.lti.convert", "control.lti.poles_zeros", "control.analysis.ctrb_obsv", "control.analysis.margin", "control.lti.c2d",
           "control.lti.impulse", "control.math.lyap", "control.design.place", "control.design.dlqr"]:
    calls.append((op.replace(".", "_"), f"{op}/request.json"))
calls += [("control_analysis_margin", "control.analysis.margin/request.error.json"), ("control_design_place", "control.design.place/request.error.json")]
for i, (t, f) in enumerate(calls):
    r = send({"jsonrpc": "2.0", "id": 10 + i, "method": "tools/call", "params": {"name": t, "arguments": ex(f)}})
    env = r["result"]["structuredContent"]
    extra = env["error"]["code"] if env["error"] else {k: v for k, v in env["result"].items() if k in ("K", "gain_margin", "X", "realization")} or env["status"]
    print(f"tools/call {t} {f}: isError={r['result']['isError']} -> {extra}")
p.stdin.close(); p.wait()
(root / "reports").mkdir(exist_ok=True)
(root / "reports/mcp-session.jsonl").write_text("\n".join(json.dumps(x, ensure_ascii=False) for x in log) + "\n")
print("exit", p.returncode, "; transcript: reports/mcp-session.jsonl")
