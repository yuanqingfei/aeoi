//! signal.spectral.fft (spec v0.4.7, 10.17) against numpy.fft (pocketfft) and a 50-digit mpmath direct DFT.
//! Golden: golden/{python-control,mpmath}/signal.spectral.fft/0.4.0 (gen_golden_fft.py). Not MATLAB.
use aeoi_core::HostCtx;
use nalgebra::Complex;
use serde_json::{json, Value};
use std::path::PathBuf;

fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn cvec(v: &Value) -> Vec<Complex<f64>> {
    v.as_array().unwrap().iter().map(|p| Complex::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
}
fn fvec(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

#[test]
fn parity_fft() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "signal.spectral.fft").unwrap();
    let (mut n_cases, mut n_err) = (0, 0);
    let mut worst = json!({});
    for engine in ["python-control", "mpmath"] {
        let dir = spec_root().join(format!("golden/{engine}/signal.spectral.fft/0.4.0"));
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        let (mut ex, mut ef) = (0.0_f64, 0.0_f64);
        for p in &files {
            let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
            let fam = case["case_family"].as_str().unwrap();
            let env = op.call(&case["request"], &HostCtx::default());
            n_cases += 1;
            if let Some(code) = case.get("expected_error") {
                assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
                n_err += 1;
                continue;
            }
            assert_eq!(env["status"], "ok", "{fam}: {}", env["error"]);
            let r = &env["result"];
            let x = &case["expected"];
            assert_eq!(r["X"]["shape"], x["shape"], "{fam}");
            for k in ["n", "input_length", "direction", "normalization", "sides"] {
                assert_eq!(r[k], x[k], "{fam}: {k}");
            }
            assert_eq!(r["frequency"]["unit"], x["frequency_unit"], "{fam}");
            let (got, want) = (cvec(&r["X"]["data"]), cvec(&x["X"]));
            let nout = *x["shape"].as_array().unwrap().last().unwrap().as_u64().as_ref().unwrap() as usize;
            let n = x["n"].as_u64().unwrap() as f64;
            let tol = 1e-12 * ((n.max(2.0)).log2().ceil() + 1.0);
            for (g, w) in got.chunks(nout).zip(want.chunks(nout)) {
                let d: f64 = g.iter().zip(w).map(|(a, b)| (a - b).norm_sqr()).sum::<f64>().sqrt();
                let s: f64 = w.iter().map(|b| b.norm_sqr()).sum::<f64>().sqrt().max(f64::MIN_POSITIVE);
                assert!(d / s <= tol, "{engine} {fam}: X rel {:e} > {tol:e}", d / s);
                ex = ex.max(d / s);
            }
            let (gf, wf) = (fvec(&r["frequency"]["value"]["data"]), fvec(&x["frequency"]));
            assert_eq!(gf.len(), wf.len(), "{fam}");
            let e2 = gf.iter().zip(&wf).map(|(a, b)| (a - b).abs() / b.abs().max(1e-300)).fold(0.0, f64::max);
            assert!(e2 <= 4.0 * f64::EPSILON, "{fam}: frequency {e2:e}");
            ef = ef.max(e2);
            for c in ["parseval", "dft_spot_check", "all_finite"] {
                assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} {}", env["trust"]["residual"]);
            }
        }
        worst[engine] = json!({"X_rel_normwise": ex, "frequency_rel": ef});
    }
    assert!(n_cases >= 40 && n_err >= 8, "{n_cases} cases, {n_err} errors");
    println!("fft parity: {n_cases} cases ({n_err} error cases), worst {worst}");
    std::fs::write(spec_root().join("reports/parity-fft.json"), serde_json::to_string_pretty(&json!({
        "reference": "numpy.fft (pocketfft) for every case; mpmath 50-digit direct DFT (exact) for small n. Not MATLAB.",
        "cases": n_cases, "error_cases": n_err, "worst": worst,
        "tolerances": {"X_rel_normwise": "1e-12 · (⌈log2 n⌉ + 1)", "frequency_rel": "4ε"}
    })).unwrap() + "\n").unwrap();
}
