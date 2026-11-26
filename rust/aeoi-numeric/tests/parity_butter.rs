//! signal.design.butter (spec v0.4.12, 10.24) against an independent 50-digit mpmath construction (truth) and
//! scipy.signal.butter. Golden: golden/python-control/signal.design.butter/0.4.0 (scripts/golden/gen_golden_butter.py). Not MATLAB.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::path::PathBuf;

type C = (f64, f64);
fn spec_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn cl(v: &Value) -> Vec<C> {
    v.as_array().unwrap().iter().map(|z| (z[0].as_f64().unwrap(), z[1].as_f64().unwrap())).collect()
}
fn fl(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
fn set_err(got: &[C], want: &[C]) -> f64 {
    assert_eq!(got.len(), want.len());
    let mut used = vec![false; want.len()];
    let mut worst = 0.0f64;
    for g in got {
        let (j, d) = want.iter().enumerate().filter(|(j, _)| !used[*j]).map(|(j, w)| (j, (g.0 - w.0).hypot(g.1 - w.1) / w.0.hypot(w.1).max(1.0))).fold((0, f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
        used[j] = true;
        worst = worst.max(d);
    }
    worst
}
fn mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn div(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
fn hz(z: &[C], p: &[C], k: f64, x: C) -> C {
    let n = z.iter().fold((k, 0.0), |a, zz| mul(a, (x.0 - zz.0, x.1 - zz.1)));
    let d = p.iter().fold((1.0, 0.0), |a, pp| mul(a, (x.0 - pp.0, x.1 - pp.1)));
    div(n, d)
}
fn sec(s: &[f64], x: C) -> C {
    // b0 + b1 x⁻¹ + b2 x⁻²  over  a0 + a1 x⁻¹ + a2 x⁻²
    let xi = div((1.0, 0.0), x);
    let ev = |c: &[f64]| { let t = (c[1] + c[2] * xi.0, c[2] * xi.1); let t = mul(t, xi); (c[0] + t.0, t.1) };
    div(ev(&s[..3]), ev(&s[3..]))
}

#[test]
fn parity_butter() {
    let reg = aeoi_numeric::registry();
    let op = aeoi_numeric::find(&reg, "signal.design.butter").unwrap();
    let dir = spec_root().join("golden/python-control/signal.design.butter/0.4.0");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut n, mut n_err, mut worst_zp, mut worst_sos, mut worst_ba) = (0, 0, 0.0f64, 0.0f64, 0.0f64);
    let mut rows = Vec::new();
    for p in &files {
        let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let fam = case["case_family"].as_str().unwrap();
        let env = op.call(&case["request"], &HostCtx::default());
        n += 1;
        if let Some(code) = case.get("expected_error") {
            assert_eq!(&env["error"]["code"], code, "{fam}: {}", env["error"]);
            n_err += 1;
            continue;
        }
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{fam}: {}", env["error"]);
        let x = &case["expected"];
        let r = &env["result"];
        let (zt, pt, kt) = (cl(&x["z_true"]), cl(&x["p_true"]), x["k_true"].as_f64().unwrap());
        let ez = set_err(&cl(&r["z"]), &zt).max(set_err(&cl(&r["p"]), &pt)).max((r["k"].as_f64().unwrap() - kt).abs() / kt.abs());
        assert!(ez <= 1e-12, "{fam}: zpk error {ez:e}");
        worst_zp = worst_zp.max(ez);
        let analog = x["analog"].as_bool().unwrap();
        let edges = fl(&x["edges"]);
        let pts: Vec<C> = if analog {
            let w0 = edges.iter().product::<f64>().powf(1.0 / edges.len() as f64);
            (0..64).map(|k| (0.0, w0 * 10f64.powf(-2.0 + 4.0 * k as f64 / 63.0))).collect()
        } else {
            (0..64).map(|k| { let t = std::f64::consts::PI * (k as f64 + 0.5) / 64.0; (t.cos(), t.sin()) }).collect()
        };
        let ht: Vec<C> = pts.iter().map(|q| hz(&zt, &pt, kt, *q)).collect();
        let hmax = ht.iter().map(|h| h.0.hypot(h.1)).fold(0.0, f64::max);
        if !analog {
            let s = fl(&r["sos"]["data"]);
            let d = pts.iter().zip(&ht).map(|(q, h)| { let g = s.chunks(6).fold((1.0, 0.0), |a, c| mul(a, sec(c, *q))); (g.0 - h.0).hypot(g.1 - h.1) }).fold(0.0, f64::max) / hmax;
            assert!(d <= 1e-11, "{fam}: sos response deviation {d:e}");
            worst_sos = worst_sos.max(d);
        } else {
            assert!(r["sos"].is_null() && r["system"]["kind"] == "zpk" && r["system"]["ts"] == 0.0, "{fam}");
        }
        let warned = env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED");
        assert_eq!(warned, case.get("expect_warning").is_some(), "{fam}: ba warning {}", env["trust"]["residual"]);
        if !warned {
            let (b, a, bt, at) = (fl(&r["b"]), fl(&r["a"]), fl(&x["b_true"]), fl(&x["a_true"]));
            let bm = bt.iter().chain(at.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
            let d = b.iter().zip(&bt).chain(a.iter().zip(&at)).map(|(g, w)| (g - w).abs()).fold(0.0, f64::max) / bm;
            assert!(d <= 1e-12, "{fam}: b/a coefficients {d:e}");
            worst_ba = worst_ba.max(d);
        }
        for c in ["stable", "edge_gain", "sos_consistent", "all_finite"] {
            assert_eq!(env["trust"]["checks"][c], true, "{fam}: {c} {}", env["trust"]);
        }
        rows.push(json!({"family": fam, "zpk_error": ez, "ba_deviation": env["trust"]["residual"]["ba_relative_deviation"], "scipy_ba_deviation": x["scipy_ba_response_deviation"]}));
    }
    assert!(n >= 30 && n_err >= 12);
    println!("butter parity: {n} cases ({n_err} errors); worst zpk {worst_zp:.1e}, sos response {worst_sos:.1e}, b/a {worst_ba:.1e}");
    std::fs::write(spec_root().join("reports/parity-butter.json"), serde_json::to_string_pretty(&json!({
        "reference": "independent 50-digit mpmath construction (truth); scipy.signal.butter recorded. Not MATLAB.",
        "cases": n, "error_cases": n_err, "worst_zpk_error": worst_zp, "worst_sos_response_deviation": worst_sos, "worst_ba_coefficient_error": worst_ba,
        "tolerance": "z, p, k ≤ 1e-12 relative (sets); sos response ≤ 1e-11·max|H| on 64 points; b/a coefficients ≤ 1e-12·max|coefficient| unless flagged ill-conditioned", "cases_detail": rows
    })).unwrap() + "\n").unwrap();
}
