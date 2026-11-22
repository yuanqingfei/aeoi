//! aeoi-mcp: minimal MCP server (stdio, newline-delimited JSON-RPC 2.0) exposing AEOI operators as MCP tools.
//!
//! Usage: aeoi-mcp [--data-root DIR] [--namespace PREFIX]
//!   --data-root DIR    enable file:// MatrixRef inputs under DIR (canonicalized; symlink escapes rejected)
//!   --namespace PREFIX only expose operators whose id starts with PREFIX (e.g. "control.lti")
//!
//! Implemented: initialize, notifications/*, ping, tools/list, tools/call, resources/list, resources/read.
//! tools/call returns the AEOI envelope as structuredContent, plus the same JSON as a text block (for clients
//! that ignore structuredContent). isError = (envelope.status == "error").
use aeoi_core::{schema, HostCtx};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const SUPPORTED: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

struct Server {
    reg: Vec<Box<dyn aeoi_core::operator::DynOperator>>,
    host: HostCtx,
}

fn rpc_err(id: &Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}
fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

impl Server {
    fn tool_name(o: &dyn aeoi_core::operator::DynOperator) -> String {
        o.meta().manifest["mcp"]["tool_name"].as_str().map(String::from).unwrap_or_else(|| o.meta().id.replace('.', "_"))
    }

    fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .reg
            .iter()
            .map(|o| {
                let m = o.meta();
                let ann = m.manifest["mcp"]["annotations"].clone();
                json!({
                    "name": Self::tool_name(o.as_ref()),
                    "title": m.manifest["title"],
                    "description": format!("{}\n[AEOI {} v{} · tolerance {} · grade {} · determinism {}]",
                        m.manifest["description"].as_str().unwrap_or(""), m.id, m.version, m.tolerance_class,
                        m.manifest["accuracy_grade"]["current"].as_str().unwrap_or("?"), m.determinism),
                    "inputSchema": m.input_schema,
                    "outputSchema": schema::envelope_with_result(&m.output_schema),
                    "annotations": if ann.is_null() { json!({"readOnlyHint": true, "idempotentHint": true, "openWorldHint": false}) } else { ann },
                    "_meta": {"aeoi/id": m.id, "aeoi/version": m.version, "aeoi/spec": aeoi_core::SPEC,
                              "aeoi/grade": m.manifest["accuracy_grade"]["current"],
                              "aeoi/manifest": format!("aeoi://operators/{}/{}/manifest", m.id, m.version)},
                })
            })
            .collect();
        json!({"tools": tools})
    }

    fn tools_call(&self, id: &Value, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or("");
        let Some(op) = self.reg.iter().find(|o| Self::tool_name(o.as_ref()) == name || o.meta().id == name) else {
            return rpc_err(id, -32602, &format!("Unknown tool: {name}"));
        };
        let args = if params["arguments"].is_null() { json!({}) } else { params["arguments"].clone() };
        let env = op.call(&args, &self.host);
        let is_error = env["status"] == "error";
        let summary = if is_error {
            format!("AEOI {} → error {}: {}", op.meta().id, env["error"]["code"].as_str().unwrap_or(""), env["error"]["message"].as_str().unwrap_or(""))
        } else {
            let w: Vec<&str> = env["warnings"].as_array().map(|a| a.iter().filter_map(|x| x["code"].as_str()).collect()).unwrap_or_default();
            format!("AEOI {} → {} (grade {}, warnings: {})", op.meta().id, env["status"].as_str().unwrap_or(""),
                env["trust"]["accuracy_grade"].as_str().unwrap_or("-"), if w.is_empty() { "none".into() } else { w.join(", ") })
        };
        ok(id, json!({
            "content": [{"type": "text", "text": summary}, {"type": "text", "text": serde_json::to_string(&env).unwrap()}],
            "structuredContent": env,
            "isError": is_error,
        }))
    }

    fn resources_list(&self) -> Value {
        let mut r: Vec<Value> = vec![json!({"uri": "aeoi://operators", "name": "operators", "title": "AEOI operator catalog", "mimeType": "application/json"})];
        r.extend(self
            .reg
            .iter()
            .map(|o| json!({"uri": format!("aeoi://operators/{}/{}/manifest", o.meta().id, o.meta().version), "name": o.meta().id,
                            "title": format!("AEOI manifest: {}", o.meta().id), "mimeType": "application/json"}))
            .collect::<Vec<Value>>());
        for s in ["common", "envelope", "error", "manifest"] {
            r.push(json!({"uri": format!("aeoi://schemas/{s}/0.2"), "name": format!("{s}.schema.json"), "mimeType": "application/schema+json"}));
        }
        json!({"resources": r})
    }

    fn resources_read(&self, id: &Value, params: &Value) -> Value {
        let uri = params["uri"].as_str().unwrap_or("");
        let body = if uri == "aeoi://operators" {
            Some(json!({"spec": aeoi_core::SPEC, "operators": self.reg.iter().map(|o| json!({"id": o.meta().id, "version": o.meta().version,
                "tool_name": Self::tool_name(o.as_ref()), "manifest": format!("aeoi://operators/{}/{}/manifest", o.meta().id, o.meta().version)})).collect::<Vec<_>>()}))
        } else if let Some(rest) = uri.strip_prefix("aeoi://operators/") {
            let parts: Vec<&str> = rest.split('/').collect();
            match parts.as_slice() {
                [id, ver, "manifest"] => self.reg.iter().find(|o| o.meta().id == *id && o.meta().version == *ver).map(|o| o.meta().manifest_resolved()),
                _ => None,
            }
        } else if let Some(rest) = uri.strip_prefix("aeoi://schemas/") {
            let name = rest.split('/').next().unwrap_or("");
            if ["common", "envelope", "error", "manifest"].contains(&name) { Some(schema::doc(name).clone()) } else { None }
        } else {
            None
        };
        match body {
            Some(b) => ok(id, json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": serde_json::to_string_pretty(&b).unwrap()}]})),
            None => rpc_err(id, -32002, &format!("Resource not found: {uri}")),
        }
    }

    fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");
        let Some(id) = id else { return None }; // notification: no response
        let p = &msg["params"];
        Some(match method {
            "initialize" => {
                let req = p["protocolVersion"].as_str().unwrap_or("");
                let v = if SUPPORTED.contains(&req) { req } else { SUPPORTED[0] };
                ok(&id, json!({
                    "protocolVersion": v,
                    "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false, "subscribe": false}},
                    "serverInfo": {"name": "aeoi-mcp", "title": "AEOI engineering operators (control)", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Tools return AEOI envelopes (spec aeoi/0.2) in structuredContent. Matrices are row-major. \
                                     On status=error, read error.remediation (RFC 6902 patches) before retrying; \
                                     check trust.accuracy_grade and warnings before relying on a result.",
                }))
            }
            "ping" => ok(&id, json!({})),
            "tools/list" => ok(&id, self.tools_list()),
            "tools/call" => self.tools_call(&id, p),
            "resources/list" => ok(&id, self.resources_list()),
            "resources/read" => self.resources_read(&id, p),
            _ => rpc_err(&id, -32601, &format!("Method not found: {method}")),
        })
    }
}

fn main() {
    let mut host = HostCtx::default();
    let mut ns: Option<String> = None;
    let a: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--data-root" => { host.data_root = Some(std::path::PathBuf::from(&a[i + 1])); i += 1; }
            "--namespace" => { ns = Some(a[i + 1].clone()); i += 1; }
            "-h" | "--help" => { eprintln!("usage: aeoi-mcp [--data-root DIR] [--namespace PREFIX]"); return; }
            x => { eprintln!("unknown argument {x}"); std::process::exit(2); }
        }
        i += 1;
    }
    let mut reg = aeoi_control::registry();
    if let Some(n) = &ns {
        reg.retain(|o| o.meta().id.starts_with(n.as_str()));
    }
    let server = Server { reg, host };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let r: Vec<Value> = batch.iter().filter_map(|m| server.handle(m)).collect();
                if r.is_empty() { None } else { Some(Value::Array(r)) }
            }
            Ok(m) => server.handle(&m),
            Err(e) => Some(rpc_err(&Value::Null, -32700, &format!("Parse error: {e}"))),
        };
        if let Some(r) = resp {
            writeln!(out, "{}", serde_json::to_string(&r).unwrap()).unwrap();
            out.flush().unwrap();
        }
    }
}
