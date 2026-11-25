//! aeoi-call <operator-id|tool_name> <request.json|-> [--data-root DIR]
//!   Runs one operator through the full AEOI pipeline and prints the envelope as JSON.
//! aeoi-call verify <operator-id|tool_name> <request.json> <envelope-or-receipt.json> [--data-root DIR]
//!   Re-runs the request and compares with the claimed envelope's receipt (spec 4.3); exit code 0 when the
//!   verdict is bitwise_match / tolerance_match / error_reproduced, 1 otherwise.
//! aeoi-call hashes <golden-dir> [--data-root DIR]
//!   Runs every golden request under the directory and prints {"<path>": "<output_hash>" | "error:<code>"}
//!   with sorted keys. Release CI compares this map across platforms (spec 2.1 M-7a, v0.4.9).
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
    if args.get(1).map(String::as_str) == Some("mathprobe") {
        mathprobe();
        return;
    }
    if args.get(1).map(String::as_str) == Some("hashes") && args.len() >= 3 {
        hashes(std::path::Path::new(&args[2]), &host);
        return;
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

fn golden_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            golden_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

fn hashes(dir: &std::path::Path, host: &HostCtx) {
    let reg = aeoi_ops::registry();
    let mut files = Vec::new();
    golden_files(dir, &mut files);
    let mut map = std::collections::BTreeMap::new();
    for p in files {
        let case = read(p.to_str().unwrap());
        let (Some(id), Some(req)) = (case["operator"].as_str(), case.get("request")) else { continue };
        let Some(op) = aeoi_ops::find(&reg, id) else { continue };
        let env = op.call(req, host);
        let v = if env["status"] == "error" {
            format!("error:{}", env["error"]["code"].as_str().unwrap_or("?"))
        } else {
            env["reproducibility"]["output_hash"].as_str().unwrap_or("?").to_string()
        };
        let key = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        map.insert(key, v);
    }
    println!("{}", serde_json::to_string_pretty(&map).unwrap());
}

/// Diagnostic (release CI): digests of the math functions as this binary computes them, called the way
/// operator code calls them (including patterns LLVM rewrites, such as pow(10, x)). Equal on every platform
/// iff spec 2.1 M-7a holds for these functions.
fn mathprobe() {
    use std::hint::black_box;
    let xs: Vec<f64> = (0..20_000).map(|i| (i as f64 - 10_000.0) * 0.000_731 + 1e-3 * ((i * 7919) % 97) as f64).collect();
    let probes: Vec<(&str, Box<dyn Fn(f64) -> f64>)> = vec![
        ("sin", Box::new(|x: f64| x.sin())),
        ("cos", Box::new(|x: f64| x.cos())),
        ("sin_cos", Box::new(|x: f64| { let (s, c) = x.sin_cos(); s + 3.0 * c })),
        ("twiddle", Box::new(|x: f64| { let a = -2f64 * std::f64::consts::PI / 97.0 * x; a.cos() + a.sin() })),
        ("exp", Box::new(|x: f64| x.exp())),
        ("ln", Box::new(|x: f64| x.abs().ln())),
        ("pow10", Box::new(|x: f64| 10f64.powf(x))),
        ("pow2", Box::new(|x: f64| 2f64.powf(x))),
        ("powf", Box::new(|x: f64| x.abs().powf(1.37))),
        ("atan2", Box::new(|x: f64| x.atan2(0.3))),
        ("hypot", Box::new(|x: f64| x.hypot(0.3))),
        ("powi", Box::new(|x: f64| x.powi(7))),
        ("mul_add", Box::new(|x: f64| x.mul_add(1.0 / 3.0, 0.1))),
        ("sqrt", Box::new(|x: f64| x.abs().sqrt())),
    ];
    for (name, f) in &probes {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a over the result bits
        for &x in &xs {
            for b in f(black_box(x)).to_bits().to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        println!("{name:8} {h:016x}");
    }
    println!("libm     {}", aeoi_core::operator::libm());
}
