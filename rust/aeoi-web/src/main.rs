//! aeoi-web CLI.
//!
//!   aeoi-web [serve] [--addr 127.0.0.1:8787] [--model gemini:gemini-2.5-flash]
//!   aeoi-web eval --model <provider:model> [--rpm 8] [--max-steps 8] [--only id1,id2] [--limit N] [--out file] [--questions file]
//!   aeoi-web ask "<问题>" [--model ...] [--max-steps 8]       one question, prints the trace as JSON
//!   aeoi-web tools [--gemini | --json | --losses]               dump function declarations / lossy-mapping report
//!   aeoi-web rescore <report.json> [--out file]                 re-score a saved eval report (no model calls)
//!   aeoi-web design --model <p:m> [--engdesign all|none|ID,ID] [--controleval N] [--controleval-offset K] [--categories c1,c2] [--rpm 8] [--max-steps 10] [--call-timeout 150] [--task-timeout 600] [--mock-script file] [--out file]
//!   aeoi-web design-score <submissions.json>                     score [{id, answer}] without a model (EngDesign ids, or ControlEval with controleval_task)
use aeoi_web::{agent, design, eval, provider, server, tools::Bridge};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

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

/// Replace the report in place. A kill during the write leaves the previous checkpoint.
fn write_report(path: &Path, r: &Value) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = path.with_extension("json.partial");
    std::fs::write(&tmp, serde_json::to_string_pretty(r).unwrap() + "\n").expect("write report");
    std::fs::rename(&tmp, path).expect("rename report");
}

fn usage() -> ! {
    eprintln!("{}", include_str!("main.rs").lines().skip(2).take(8).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
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
            let out = flag(&args, "--out").map(PathBuf::from).unwrap_or_else(|| reports_dir().join(format!("eval-{}.json", sanitize(&model.replace(':', "_")))));
            if let Some(d) = out.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let write = |r: &Value| write_report(&out, r);
            let report = eval::run_eval_with(p.as_ref(), &b, &qs, &opts, |partial| write(partial)).await;
            write(&report);
            eprintln!("\n{}", serde_json::to_string_pretty(&report["summary"]).unwrap());
            eprintln!("报告已写入 {}", out.display());
        }
        "rescore" => {
            let f = args.get(1).cloned().unwrap_or_else(|| usage());
            let rep: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{f}: {e}"))).expect("report json");
            let qs: Value = serde_json::from_str(eval::QUESTIONS).unwrap();
            let out = eval::rescore(&Bridge::new(), &qs, &rep);
            let dst = flag(&args, "--out").unwrap_or(f);
            std::fs::write(&dst, serde_json::to_string_pretty(&out).unwrap() + "\n").expect("write report");
            for r in out["results"].as_array().unwrap() {
                let o = rep["results"].as_array().unwrap().iter().find(|x| x["id"] == r["id"]).unwrap();
                for k in ["tool_ok", "args_ok", "answer_ok"] {
                    if o[k] != r[k] {
                        eprintln!("{:<24} {k}: {} → {}", r["id"].as_str().unwrap(), o[k], r[k]);
                    }
                }
            }
            eprintln!("{}", serde_json::to_string_pretty(&out["summary"]).unwrap());
            eprintln!("报告已写入 {dst}");
        }
        "design" => {
            let p = if let Some(path) = flag(&args, "--mock-script") {
                provider::ScriptMock::from_file(&path).map(|m| Box::new(m) as Box<dyn provider::Provider>).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1)
                })
            } else {
                provider::make(&model).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1)
                })
            };
            let mut tasks = vec![];
            let sel = flag(&args, "--engdesign").unwrap_or_else(|| "all".into());
            if sel != "none" {
                let ids: Vec<String> = sel.split(',').map(|x| x.trim().to_string()).collect();
                tasks.extend(design::engdesign_tasks().into_iter().filter(|t| sel == "all" || ids.iter().any(|i| t["id"].as_str().unwrap_or("").ends_with(&format!("/{i}")))));
            }
            if let Some(n) = flag(&args, "--controleval").and_then(|s| s.parse::<usize>().ok()) {
                let path = flag(&args, "--controleval-file").unwrap_or_else(|| reports_dir().join("../eval/design/upstream/controleval.json").display().to_string());
                let cats: Vec<String> = flag(&args, "--categories").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
                let off = flag(&args, "--controleval-offset").and_then(|s| s.parse().ok()).unwrap_or(0);
                match design::controleval_tasks(&path, n, off, &cats) {
                    Ok(t) => tasks.extend(t),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1)
                    }
                }
            }
            let opts = design::DesignOpts {
                rpm: flag(&args, "--rpm").and_then(|s| s.parse().ok()).unwrap_or(8.0),
                max_steps: flag(&args, "--max-steps").and_then(|s| s.parse().ok()).unwrap_or(10),
                call_timeout: Duration::from_secs_f64(flag(&args, "--call-timeout").and_then(|s| s.parse().ok()).unwrap_or(150.0)),
                task_timeout: Duration::from_secs_f64(flag(&args, "--task-timeout").and_then(|s| s.parse().ok()).unwrap_or(600.0)),
            };
            let out = flag(&args, "--out").map(PathBuf::from).unwrap_or_else(|| reports_dir().join(format!("design-{}.json", sanitize(&model.replace(':', "_")))));
            let write = |r: &Value| write_report(&out, r);
            eprintln!("{} 个设计任务", tasks.len());
            let rep = design::run_design(p.as_ref(), &Bridge::new(), &tasks, &opts, |r| write(r)).await;
            write(&rep);
            eprintln!("\n{}", serde_json::to_string_pretty(&rep["summary"]).unwrap());
            eprintln!("报告已写入 {}", out.display());
        }
        "design-score" => {
            let f = args.get(1).cloned().unwrap_or_else(|| usage());
            let subs: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{f}: {e}"))).expect("json");
            let b = Bridge::new();
            let eng = design::engdesign_tasks();
            let mut out = vec![];
            for s in subs.as_array().expect("array of {id, answer}") {
                let id = s["id"].as_str().unwrap_or("");
                let task = match eng.iter().find(|t| t["id"] == id) {
                    Some(t) => t.clone(),
                    None => json!({"scorer": "controleval", "controleval": s["controleval_task"]}),
                };
                let (pass, score, details) = design::score(&b, &task, &s["answer"]);
                out.push(json!({"id": id, "pass": pass, "score": score, "details": details}));
            }
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        _ => usage(),
    }
}
