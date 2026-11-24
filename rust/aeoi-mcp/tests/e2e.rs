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
    assert_eq!(names.len(), aeoi_ops::registry().len() + 1, "every operator + aeoi_verify");
    assert!(names.contains(&"aeoi_verify"));
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
        assert_eq!(t["outputSchema"]["type"], "object");
    }
    let ops = [
        "control.lti.bode", "control.lti.step", "control.design.lqr", "control.lti.convert", "control.lti.poles_zeros",
        "control.analysis.ctrb_obsv", "control.analysis.margin", "control.lti.c2d", "control.lti.impulse", "control.math.lyap",
        "control.design.place", "control.design.dlqr", "control.lti.series", "control.lti.parallel", "control.lti.feedback",
        "control.design.pid", "control.lti.stepinfo", "control.lti.pade",
    ];
    for op in ops {
        let tool = op.replace('.', "_");
        assert!(names.contains(&tool.as_str()), "{tool} missing from tools/list");
        for (req, expect_err) in [("request.json", false), ("request.error.json", true)] {
            let want = ex(&format!("{op}/{}", if expect_err { "response.error.json" } else { "response.success.json" }));
            let r = rpc("tools/call", json!({"name": tool, "arguments": ex(&format!("{op}/{req}"))}));
            let res = &r["result"];
            println!("tools/call {tool} ({req}): isError={} :: {}", res["isError"], res["content"][0]["text"]);
            assert_eq!(res["isError"], expect_err);
            let env = &res["structuredContent"];
            let (er, wr) = (&env["reproducibility"], &want["reproducibility"]);
            let same_platform = er["platform"] == wr["platform"] && er["cpu_features"] == wr["cpu_features"] && !wr["libm"].is_null() && er["libm"] == wr["libm"];
            assert_eq!(env["spec"], "aeoi/0.4");
            let text_env: Value = serde_json::from_str(res["content"][1]["text"].as_str().unwrap()).unwrap();
            assert_eq!(&text_env, env);
            if expect_err {
                assert_eq!(env["error"]["code"], want["error"]["code"]);
            } else {
                assert!(env["trust"]["accuracy_grade"].is_string());
                // platform_bitwise (spec 2.1, P-7): equal bytes are promised only on the platform the example came from.
                if same_platform {
                    assert_eq!(env["reproducibility"]["output_hash"], want["reproducibility"]["output_hash"], "{op}: MCP result differs from the published example");
                } else {
                    println!("{op}: bitwise comparison skipped (example from {} {}, this host {} {})", want["reproducibility"]["platform"], want["reproducibility"]["cpu_features"], env["reproducibility"]["platform"], env["reproducibility"]["cpu_features"]);
                }
                assert_eq!(env["receipt"]["fingerprint"]["request"], want["receipt"]["fingerprint"]["request"], "{op}: receipt request fingerprint differs");
            }
            // replay-verify the published example envelope through the MCP verifier tool
            let v = rpc("tools/call", json!({"name": "aeoi_verify", "arguments": {"operator": op, "arguments": ex(&format!("{op}/{req}")), "envelope": want}}));
            let verdict = v["result"]["structuredContent"]["verdict"].as_str().unwrap_or("").to_string();
            assert_eq!(v["result"]["isError"], false, "{op} {req}: verify verdict {verdict}");
            if expect_err {
                assert_eq!(verdict, "error_reproduced");
            } else {
                if same_platform {
                    assert_eq!(verdict, "bitwise_match");
                } else {
                    assert!(verdict == "bitwise_match" || verdict == "tolerance_match", "{op}: verify verdict {verdict}");
                }
            }
        }
    }
    // schema-level error (missing required field) is also an AEOI envelope, not a JSON-RPC error
    let r = rpc("tools/call", json!({"name":"control_lti_bode","arguments":{"frequency":{"mode":"explicit","values":[1.0]}}}));
    assert_eq!(r["result"]["structuredContent"]["error"]["code"], "E.VALIDATION.SCHEMA");
    // tampered result → result_tampered; changed input → request_mismatch
    let mut bad = ex("control.design.lqr/response.success.json");
    bad["result"]["K"]["data"][0] = json!(123.0);
    let v = rpc("tools/call", json!({"name": "aeoi_verify", "arguments": {"operator": "control.design.lqr", "arguments": ex("control.design.lqr/request.json"), "envelope": bad}}));
    assert_eq!(v["result"]["structuredContent"]["verdict"], "result_tampered");
    assert_eq!(v["result"]["isError"], true);
    let mut req2 = ex("control.design.lqr/request.json");
    req2["Q"]["data"][0] = json!(2.0);
    let v = rpc("tools/call", json!({"name": "aeoi_verify", "arguments": {"operator": "control.design.lqr", "arguments": req2, "envelope": ex("control.design.lqr/response.success.json")}}));
    assert_eq!(v["result"]["structuredContent"]["verdict"], "request_mismatch");
    // unknown tool → JSON-RPC invalid params
    let r = rpc("tools/call", json!({"name":"nope","arguments":{}}));
    assert_eq!(r["error"]["code"], -32602);
    let rl = rpc("resources/list", json!({}));
    assert!(rl["result"]["resources"].as_array().unwrap().len() >= 8);
    let rr = rpc("resources/read", json!({"uri":"aeoi://operators/control.design.lqr/0.3.0/manifest"}));
    let m: Value = serde_json::from_str(rr["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(m["id"], "control.design.lqr");
    let cat = rpc("resources/read", json!({"uri":"aeoi://operators"}));
    assert!(cat["result"]["contents"][0]["text"].as_str().unwrap().contains("control_lti_step"));
    assert_eq!(rpc("resources/read", json!({"uri":"aeoi://operators/x/0/manifest"}))["error"]["code"], -32002);
    assert_eq!(rpc("ping", json!({}))["result"], json!({}));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
