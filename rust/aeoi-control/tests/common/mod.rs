#![allow(dead_code)]
use serde_json::Value;
use std::path::PathBuf;

pub fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn f(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) if s == "NaN" => f64::NAN,
        Value::String(s) if s == "Inf" => f64::INFINITY,
        Value::String(s) if s == "-Inf" => f64::NEG_INFINITY,
        Value::Null => f64::NAN,
        _ => panic!("not a number: {v}"),
    }
}

pub fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(f).collect()
}

/// Complex vector from [[re,im],...]
pub fn cvec(v: &Value) -> Vec<(f64, f64)> {
    v.as_array().unwrap().iter().map(|c| (f(&c[0]), f(&c[1]))).collect()
}

pub fn wrap180(d: f64) -> f64 {
    let mut x = d % 360.0;
    if x > 180.0 {
        x -= 360.0
    }
    if x <= -180.0 {
        x += 360.0
    }
    x
}

pub fn fro(a: &[f64]) -> f64 {
    a.iter().map(|x| x * x).sum::<f64>().sqrt()
}

pub fn fro_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt()
}

/// Greedy nearest matching of two complex sets; returns max |a_i - b_match| / max(1,|b|).
pub fn matched_set_err(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mut used = vec![false; b.len()];
    let mut worst: f64 = 0.0;
    for x in a {
        let (mut bi, mut bd) = (usize::MAX, f64::INFINITY);
        for (j, y) in b.iter().enumerate() {
            if used[j] {
                continue;
            }
            let d = ((x.0 - y.0).powi(2) + (x.1 - y.1).powi(2)).sqrt();
            if d < bd {
                bd = d;
                bi = j;
            }
        }
        used[bi] = true;
        let scale = (b[bi].0.powi(2) + b[bi].1.powi(2)).sqrt().max(1.0);
        worst = worst.max(bd / scale);
    }
    worst
}

/// Spec 2.1 / P-7: `platform_bitwise` promises equal bytes only on the same build AND the same "platform"
/// (target triple + runtime-detected CPU features). Published example hashes were produced on one such platform,
/// so on any other machine a differing `output_hash` is not a defect; the numeric checks (python-control /
/// mpmath tolerance) still run. Returns whether the bitwise comparison applies.
pub fn same_platform(env: &Value, published: &Value) -> bool {
    let (a, b) = (&env["reproducibility"], &published["reproducibility"]);
    // v0.4: platform = target triple + CPU features + C math library. Examples published before v0.4 do not
    // record `libm`, so the comparison is skipped for them until they are regenerated.
    a["platform"] == b["platform"] && a["cpu_features"] == b["cpu_features"] && !b["libm"].is_null() && a["libm"] == b["libm"]
}

/// Assert equal `output_hash` when the published example comes from this platform; otherwise note and skip.
pub fn assert_hash_on_same_platform(env: &Value, published: &Value, what: &str) {
    if same_platform(env, published) {
        assert_eq!(env["reproducibility"]["output_hash"], published["reproducibility"]["output_hash"], "{what}");
    } else {
        println!(
            "{what}: bitwise comparison skipped (platform_bitwise; example from {} {} libm {}, this host {} {} libm {})",
            published["reproducibility"]["platform"], published["reproducibility"]["cpu_features"], published["reproducibility"]["libm"],
            env["reproducibility"]["platform"], env["reproducibility"]["cpu_features"], env["reproducibility"]["libm"]
        );
    }
}
