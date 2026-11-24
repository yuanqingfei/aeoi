//! aeoi-call <operator-id|tool_name> <request.json|-> [--data-root DIR]
//!   Runs one operator through the full AEOI pipeline and prints the envelope as JSON.
//! aeoi-call verify <operator-id|tool_name> <request.json> <envelope-or-receipt.json> [--data-root DIR]
//!   Re-runs the request and compares with the claimed envelope's receipt (spec 4.3); exit code 0 when the
//!   verdict is bitwise_match / tolerance_match / error_reproduced, 1 otherwise.
use aeoi_core::HostCtx;
use std::io::Read;

fn read(p: &str) -> serde_json::Value {
    let text = if p == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    } else {
        std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {p}: {e}"))
    };
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{p}: invalid JSON: {e}"))
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let mut host = HostCtx::default();
    if let Some(i) = args.iter().position(|a| a == "--data-root") {
        host.data_root = args.get(i + 1).map(Into::into);
        args.drain(i..(i + 2).min(args.len()));
    }
    let verify = args.get(1).map(String::as_str) == Some("verify");
    let base = if verify { 2 } else { 1 };
    if args.len() < base + 2 + verify as usize {
        eprintln!("usage: aeoi-call <operator-id> <request.json|-> [--data-root DIR]\n       aeoi-call verify <operator-id> <request.json> <envelope.json> [--data-root DIR]");
        std::process::exit(2);
    }
    let reg = aeoi_ops::registry();
    let Some(op) = aeoi_ops::find(&reg, &args[base]) else {
        eprintln!("unknown operator {}", args[base]);
        std::process::exit(2);
    };
    let req = read(&args[base + 1]);
    if verify {
        let claimed = read(&args[base + 2]);
        let v = aeoi_core::receipt::verify(op, &req, &claimed, &host);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        let ok = matches!(v["verdict"].as_str(), Some("bitwise_match" | "tolerance_match" | "error_reproduced"));
        std::process::exit(if ok { 0 } else { 1 });
    }
    println!("{}", serde_json::to_string_pretty(&op.call(&req, &host)).unwrap());
}
