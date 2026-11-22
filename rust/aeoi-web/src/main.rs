//! aeoi-web CLI.
//!
//!   aeoi-web [serve] [--addr 127.0.0.1:8787] [--model gemini:gemini-2.5-flash]
//!   aeoi-web eval --model <provider:model> [--rpm 8] [--max-steps 8] [--only id1,id2] [--limit N] [--out file] [--questions file]
//!   aeoi-web ask "<问题>" [--model ...] [--max-steps 8]       one question, prints the trace as JSON
//!   aeoi-web tools [--gemini | --json | --losses]               dump function declarations / lossy-mapping report
use aeoi_web::{agent, eval, provider, server, tools::Bridge};
use serde_json::{json, Value};
use std::path::PathBuf;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}
fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn reports_dir() -> PathBuf {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("reports");
    if repo.is_dir() {
        repo
    } else {
        PathBuf::from("reports")
    }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

fn usage() -> ! {
    eprintln!("{}", include_str!("main.rs").lines().skip(2).take(5).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
    std::process::exit(2)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "serve".into());
    let model = flag(&args, "--model").or_else(|| std::env::var("AEOI_MODEL").ok()).unwrap_or_else(|| provider::DEFAULT_MODEL.to_string());
    match cmd.as_str() {
        "serve" => {
            let addr = flag(&args, "--addr").unwrap_or_else(|| "127.0.0.1:8787".into());
            if let Err(e) = server::serve(&addr, model).await {
                eprintln!("无法启动服务：{e}");
                std::process::exit(1);
            }
        }
        "tools" => {
            let b = Bridge::new();
            if has(&args, "--losses") {
                let all: Vec<_> = b.tools.iter().flat_map(|t| t.losses.clone()).collect();
                for (what, n) in aeoi_web::schema_conv::summarize(&all) {
                    println!("{n:>4} × {what}");
                }
            } else {
                let v: Vec<Value> = b.tools.iter().map(|t| json!({"name": t.name, "description": t.description, "parameters": if has(&args, "--json") { t.json_schema.clone() } else { t.gemini_schema.clone() }})).collect();
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            }
        }
        "ask" => {
            let q = args.get(1).cloned().unwrap_or_else(|| usage());
            let p = provider::make(&model).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            });
            let b = Bridge::new();
            let mut h = vec![];
            let cfg = agent::AgentCfg { max_steps: flag(&args, "--max-steps").and_then(|s| s.parse().ok()).unwrap_or(8), ..Default::default() };
            let tr = agent::run(p.as_ref(), &b, &mut h, &q, &cfg, None).await;
            let mut v = serde_json::to_value(&tr).unwrap();
            if !has(&args, "--full") {
                for t in v["tools"].as_array_mut().unwrap() {
                    t["response"] = aeoi_web::tools::compact_for_model(&t["response"], 8);
                }
            }
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        "eval" => {
            let p = provider::make(&model).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            });
            let qtext = match flag(&args, "--questions") {
                Some(f) => std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{f}: {e}")),
                None => eval::QUESTIONS.to_string(),
            };
            let qs: Value = serde_json::from_str(&qtext).expect("questions json");
            let opts = eval::EvalOpts {
                rpm: flag(&args, "--rpm").and_then(|s| s.parse().ok()).unwrap_or(8.0),
                max_steps: flag(&args, "--max-steps").and_then(|s| s.parse().ok()).unwrap_or(8),
                only: flag(&args, "--only").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()),
                limit: flag(&args, "--limit").and_then(|s| s.parse().ok()),
            };
            let b = Bridge::new();
            let report = eval::run_eval(p.as_ref(), &b, &qs, &opts).await;
            let out = flag(&args, "--out").map(PathBuf::from).unwrap_or_else(|| reports_dir().join(format!("eval-{}.json", sanitize(&model.replace(':', "_")))));
            if let Some(d) = out.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap() + "\n").expect("write report");
            eprintln!("\n{}", serde_json::to_string_pretty(&report["summary"]).unwrap());
            eprintln!("报告已写入 {}", out.display());
        }
        _ => usage(),
    }
}
