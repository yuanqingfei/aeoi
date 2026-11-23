//! Run golden requests through the native operators and print one JSON line per file:
//! {file, operator, family, status, error_code, warnings, result, trust}. Used by tools/hp_compare.py to
//! compare an operator build (current or an older checkout) against the high-precision golden store.
//! usage: cargo run --release -p aeoi-control --example golden_eval -- <golden.json>...
use aeoi_core::HostCtx;
use serde_json::{json, Value};

fn main() {
    let reg = aeoi_control::registry();
    for f in std::env::args().skip(1) {
        let g: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let op = g["operator"].as_str().unwrap();
        let env = aeoi_control::find(&reg, op).unwrap().call(&g["request"], &HostCtx::default());
        let codes: Vec<Value> = env["warnings"].as_array().map(|w| w.iter().map(|x| x["code"].clone()).collect()).unwrap_or_default();
        println!("{}", json!({"file": f, "operator": op, "family": g["case_family"], "status": env["status"], "error_code": env["error"]["code"],
                              "warnings": codes, "result": env["result"], "trust": env["trust"], "version": env["operator"]["version"]}));
    }
}
