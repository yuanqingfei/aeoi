//! End-to-end MCP stdio session: initialize, tools/list, tools/call for each operator, one error call,
//! plus resources/read and an unknown-tool call.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn ex(p: &str) -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    serde_json::from_str(&std::fs::read_to_string(root.join(p)).unwrap()).unwrap()
}

#[test]
fn mcp_stdio_session() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aeoi-mcp")).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut next_id = 0;
    let mut rpc = |method: &str, params: Value| -> Value {
        next_id += 1;
        writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":next_id,"method":method,"params":params})).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], next_id);
        v
    };
    let init = rpc("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // notification: must produce no response (next response must still match next id)
    // (written directly; rpc() would wait for a line)
    let tl = rpc("tools/list", json!({}));
    let tools = tl["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    println!("tools/list: {names:?}");
    assert_eq!(names.len(), 3);
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
        assert_eq!(t["outputSchema"]["type"], "object");
    }
    let cases = [
        ("control_lti_bode", "control.lti.bode/request.json", false),
        ("control_lti_step", "control.lti.step/request.json", false),
        ("control_design_lqr", "control.design.lqr/request.json", false),
        ("control_design_lqr", "control.design.lqr/request.error.json", true),
    ];
    for (tool, req, expect_err) in cases {
        let r = rpc("tools/call", json!({"name": tool, "arguments": ex(req)}));
        let res = &r["result"];
        println!("tools/call {tool} ({req}): isError={} :: {}", res["isError"], res["content"][0]["text"]);
        assert_eq!(res["isError"], expect_err);
        let env = &res["structuredContent"];
        assert_eq!(env["spec"], "aeoi/0.2");
        let text_env: Value = serde_json::from_str(res["content"][1]["text"].as_str().unwrap()).unwrap();
        assert_eq!(&text_env, env);
        if expect_err {
            assert_eq!(env["error"]["code"], "E.DOMAIN.NOT_STABILIZABLE");
            assert!(!env["error"]["remediation"].as_array().unwrap().is_empty());
        } else {
            assert!(env["trust"]["accuracy_grade"].is_string());
        }
    }
    // schema-level error (missing required field) is also an AEOI envelope, not a JSON-RPC error
    let r = rpc("tools/call", json!({"name":"control_lti_bode","arguments":{"frequency":{"mode":"explicit","values":[1.0]}}}));
    assert_eq!(r["result"]["structuredContent"]["error"]["code"], "E.VALIDATION.SCHEMA");
    // unknown tool → JSON-RPC invalid params
    let r = rpc("tools/call", json!({"name":"nope","arguments":{}}));
    assert_eq!(r["error"]["code"], -32602);
    let rl = rpc("resources/list", json!({}));
    assert!(rl["result"]["resources"].as_array().unwrap().len() >= 8);
    let rr = rpc("resources/read", json!({"uri":"aeoi://operators/control.design.lqr/0.2.0/manifest"}));
    let m: Value = serde_json::from_str(rr["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(m["id"], "control.design.lqr");
    let cat = rpc("resources/read", json!({"uri":"aeoi://operators"}));
    assert!(cat["result"]["contents"][0]["text"].as_str().unwrap().contains("control_lti_step"));
    assert_eq!(rpc("resources/read", json!({"uri":"aeoi://operators/x/0/manifest"}))["error"]["code"], -32002);
    assert_eq!(rpc("ping", json!({}))["result"], json!({}));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
