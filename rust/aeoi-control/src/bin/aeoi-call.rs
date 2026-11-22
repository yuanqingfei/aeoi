//! aeoi-call <operator-id|tool_name> <request.json|-> [--data-root DIR]
//! Runs one operator through the full AEOI pipeline and prints the envelope as JSON.
use aeoi_core::HostCtx;
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: aeoi-call <operator-id> <request.json|-> [--data-root DIR]");
        std::process::exit(2);
    }
    let mut host = HostCtx::default();
    if let Some(i) = args.iter().position(|a| a == "--data-root") {
        host.data_root = args.get(i + 1).map(Into::into);
    }
    let text = if args[2] == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    } else {
        std::fs::read_to_string(&args[2]).expect("read request")
    };
    let req: serde_json::Value = serde_json::from_str(&text).expect("request json");
    let reg = aeoi_control::registry();
    let Some(op) = aeoi_control::find(&reg, &args[1]) else {
        eprintln!("unknown operator {}", args[1]);
        std::process::exit(2);
    };
    println!("{}", serde_json::to_string_pretty(&op.call(&req, &host)).unwrap());
}
