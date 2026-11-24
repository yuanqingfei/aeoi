//! Native side of the wasm-vs-native parity run: executes every golden request
//! (`golden/python-control/*/*/*.json`, `golden/mpmath/*/*/*.json`) and every `examples/*/request*.json` through the native
//! operators and prints one JSON line per case: {case, operator, status, error_code, warnings,
//! input_hash, output_hash, result}. `parity/parity.mjs` does the same through the wasm build and
//! compares.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn cases(repo: &Path) -> Vec<(String, String, Value)> {
    let mut out = vec![];
    let mut files: Vec<PathBuf> = vec![];
    for engine in ["python-control", "mpmath"] {
        let g = repo.join("golden").join(engine);
        for op in std::fs::read_dir(&g).unwrap().flatten().filter(|e| e.path().is_dir()) {
            for ver in std::fs::read_dir(op.path()).unwrap().flatten() {
                for f in std::fs::read_dir(ver.path()).unwrap().flatten() {
                    files.push(f.path());
                }
            }
        }
    }
    files.sort();
    for f in files {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        out.push((f.strip_prefix(repo).unwrap().display().to_string(), v["operator"].as_str().unwrap().to_string(), v["request"].clone()));
    }
    let mut ex: Vec<PathBuf> = std::fs::read_dir(repo.join("examples")).unwrap().flatten().flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten().map(|e| e.path())).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("request")).collect();
    ex.sort();
    for f in ex {
        let op = f.parent().unwrap().file_name().unwrap().to_string_lossy().to_string();
        out.push((f.strip_prefix(repo).unwrap().display().to_string(), op, serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap()));
    }
    out
}

pub fn summary(case: &str, op: &str, env: &Value) -> Value {
    json!({"case": case, "operator": op, "status": env["status"], "error_code": env["error"]["code"],
           "warnings": env["warnings"].as_array().map(|w| w.iter().map(|x| x["code"].clone()).collect::<Vec<_>>()),
           "input_hash": env["reproducibility"]["input_hash"], "output_hash": env["reproducibility"]["output_hash"], "result": env["result"]})
}

fn main() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reg = aeoi_ops::registry();
    let host = aeoi_core::HostCtx::default();
    // the cases list is shared with the node side so both run exactly the same inputs
    let cs = cases(&repo);
    // request_text keeps serde_json's exact number spelling (1.0 vs 1): JS JSON.parse/stringify would
    // turn 1.0 into 1, which changes input_hash
    let list: Vec<Value> = cs.iter().map(|(c, o, r)| json!({"case": c, "operator": o, "request_text": r.to_string()})).collect();
    std::fs::write(std::env::args().nth(1).unwrap_or("/tmp/aeoi-parity-cases.json".into()), serde_json::to_string(&list).unwrap()).unwrap();
    for (c, o, r) in &cs {
        let op = aeoi_ops::find(&reg, o).unwrap();
        println!("{}", summary(c, o, &op.call(r, &host)));
    }
}
