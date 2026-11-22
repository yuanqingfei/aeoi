//! Batch-2 (spec v0.3) parity tests against python-control/scipy golden cases
//! (golden/python-control/<op>/0.3.0). References are python-control/scipy outputs (NOT MATLAB).
//! Tolerance rules: spec §7.2 classes with the κ-scaling rules of §7.4 (documented per op in parity.yaml).
mod common;
use aeoi_core::lti::C64;
use aeoi_core::poly::{polyder, polyval, roots};
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default, Debug)]
struct Stat {
    cases: usize,
    passed: usize,
    max_ratio: f64,
    max_err: BTreeMap<String, f64>,
    tol: BTreeMap<String, f64>,
    scaled_cases: usize,
    failures: Vec<String>,
    notes: Vec<String>,
}

impl Stat {
    /// Record metric `key`: error e vs tolerance tol (base = unscaled class tolerance). Returns pass.
    fn err(&mut self, key: &str, e: f64, tol: f64, base: f64) -> bool {
        let r = if tol > 0.0 { e / tol } else if e == 0.0 { 0.0 } else { f64::INFINITY };
        let m = self.max_err.entry(key.into()).or_insert(0.0);
        if e > *m || e.is_nan() {
            *m = e;
        }
        self.tol.entry(key.into()).or_insert(base);
        let rk = self.max_err.entry(format!("{key} [max err/tol]")).or_insert(0.0);
        if r > *rk || r.is_nan() {
            *rk = r;
        }
        self.tol.entry(format!("{key} [max err/tol]")).or_insert(1.0);
        if r > self.max_ratio || r.is_nan() {
            self.max_ratio = r;
        }
        e <= tol
    }
}

fn load(op: &str) -> Vec<(String, Value)> {
    let dir = spec_root().join(format!("golden/python-control/{op}/0.3.0"));
    let mut v: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("missing {}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn call(op: &str, req: &Value) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, op).unwrap();
    let env = o.call(req, &HostCtx::default());
    let m = &o.meta().manifest;
    let declared = |k: &str, c: &Value| m[k].as_array().unwrap().iter().any(|x| x == c);
    if env["status"] == "error" {
        assert!(declared("errors", &env["error"]["code"]), "{op}: undeclared error {}", env["error"]["code"]);
    }
    for w in env["warnings"].as_array().unwrap() {
        assert!(declared("warnings", &w["code"]), "{op}: undeclared warning {}", w["code"]);
    }
    let v = aeoi_core::schema::Validator::new(&aeoi_core::schema::envelope_with_result(&o.meta().output_schema));
    let viol = v.violations(&env, 5);
    assert!(viol.is_empty(), "{op}: envelope schema violations {viol:?}");
    env
}

fn pre(st: &mut Stat, name: &str, case: &Value, env: &Value) -> Option<bool> {
    if let Some(code) = case.get("expected_error") {
        let ok = env["status"] == "error" && env["error"]["code"] == *code;
        if !ok {
            st.failures.push(format!("{name}: expected error {code}, got {} {}", env["status"], env["error"]["code"]));
        }
        return Some(ok);
    }
    if env["status"] == "error" {
        st.failures.push(format!("{name}: unexpected error {}", env["error"]));
        return Some(false);
    }
    if let Some(ws) = case.get("expected_warnings") {
        for w in ws.as_array().unwrap() {
            if !env["warnings"].as_array().unwrap().iter().any(|x| x["code"] == *w) {
                st.failures.push(format!("{name}: missing warning {w}"));
                return Some(false);
            }
        }
    }
    None
}

// ---------------------------------------------------------------- helpers
fn mat(v: &Value) -> Vec<f64> {
    fvec(&v["data"])
}
fn rel_fro(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    let d = fro_diff(a, b);
    if d == 0.0 {
        0.0
    } else {
        d / fro(b).max(f64::MIN_POSITIVE)
    }
}
fn cl(v: &Value) -> Vec<C64> {
    let arr = if v.get("data").is_some() { &v["data"] } else { v };
    arr.as_array().unwrap().iter().map(|c| C64::new(f(&c[0]), f(&c[1]))).collect()
}
/// Wilkinson relative condition number of a simple root r of p: Σ|a_k||r|^(n−k) / (max(1,|r|)·|p'(r)|).
fn kappa_root(p: &[f64], r: C64) -> f64 {
    let n = p.len() - 1;
    let s: f64 = p.iter().enumerate().map(|(k, a)| a.abs() * r.norm().powi((n - k) as i32)).sum();
    let d = polyval(&polyder(p), r).norm();
    if d == 0.0 {
        f64::INFINITY
    } else {
        s / (r.norm().max(1.0) * d)
    }
}
fn poly_of(set: &[C64]) -> Vec<f64> {
    aeoi_core::poly::poly_real(set)
}
/// Greedy nearest matching; returns per pair (|Δ|/max(1,|ref|), ref).
fn matched(a: &[C64], b: &[C64]) -> Option<Vec<(f64, C64)>> {
    if a.len() != b.len() {
        return None;
    }
    let mut used = vec![false; b.len()];
    let mut out = Vec::new();
    for x in a {
        let (mut bi, mut bd) = (usize::MAX, f64::INFINITY);
        for (j, y) in b.iter().enumerate() {
            if !used[j] && (x - y).norm() < bd {
                bd = (x - y).norm();
                bi = j;
            }
        }
        used[bi] = true;
        out.push((bd / b[bi].norm().max(1.0), b[bi]));
    }
    Some(out)
}
/// Root-set comparison with tol_i = base·max(1, κ_i), κ from the reference polynomial p.
fn roots_cmp(st: &mut Stat, key: &str, got: &[C64], want: &[C64], p: &[f64], base: f64) -> bool {
    let Some(pairs) = matched(got, want) else {
        st.failures.push(format!("{key}: count {} vs {}", got.len(), want.len()));
        return false;
    };
    let mut ok = true;
    let mut scaled = false;
    for (e, r) in pairs {
        let k = kappa_root(p, r).max(1.0);
        if k > 1.0 {
            scaled = true;
        }
        ok &= st.err(key, e, base * k, base);
    }
    if scaled {
        st.scaled_cases += 1;
    }
    ok
}
fn align(a: &[f64], n: usize) -> Vec<f64> {
    let mut v = vec![0.0; n.saturating_sub(a.len())];
    v.extend_from_slice(a);
    v
}
/// Coefficient comparison: ‖Δ‖₁/max(‖want‖₁,‖den‖₁) ≤ base·max(1, Π(1+|λ_i|)/‖den‖₁).
fn coeff_cmp(st: &mut Stat, key: &str, got: &[f64], want: &[f64], den_ref: &[f64], base: f64) -> bool {
    let n = got.len().max(want.len());
    let (g, w) = (align(got, n), align(want, n));
    let lam = roots(den_ref);
    let prod: f64 = lam.iter().map(|l| 1.0 + l.norm()).product();
    let l1: f64 = den_ref.iter().map(|x| x.abs()).sum();
    let scale = (prod / l1).max(1.0);
    let denom = w.iter().map(|x| x.abs()).sum::<f64>().max(l1);
    let e = g.iter().zip(&w).map(|(a, b)| (a - b).abs()).sum::<f64>() / denom;
    if scale > 1.0 + 1e-12 {
        st.scaled_cases += 1;
    }
    st.err(key, e, base * scale, base)
}

// ---------------------------------------------------------------- per-op comparators
fn convert_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"]["system"];
    let mut ok = true;
    if e.get("A").is_some() {
        for k in ["A", "B", "C", "D"] {
            ok &= st.err("tf→ss matrices rel_fro (T1)", rel_fro(&mat(&r[k]), &mat(&e[k])), 1e-12, 1e-12);
        }
    } else if e.get("z").is_some() {
        let (wz, wp) = (cl(&e["z"]), cl(&e["p"]));
        let (num, den) = match case["request"]["system"]["kind"].as_str().unwrap() {
            "tf" => (fvec(&case["request"]["system"]["num"]), fvec(&case["request"]["system"]["den"])),
            _ => (poly_of(&wz), poly_of(&wp)),
        };
        ok &= roots_cmp(st, "zpk zeros (T2·κ_root)", &cl(&r["z"]), &wz, &num, 1e-10);
        ok &= roots_cmp(st, "zpk poles (T2·κ_root)", &cl(&r["p"]), &wp, &den, 1e-10);
        let (kg, kw) = (f(&r["k"]), f(&e["k"]));
        ok &= st.err("zpk gain k rel (T2)", (kg - kw).abs() / kw.abs().max(f64::MIN_POSITIVE), 1e-10, 1e-10);
    } else if e["num"][0].is_array() {
        let (gn, gd, wn, wd) = (&r["num"], &r["den"], &e["num"], &e["den"]);
        for i in 0..wn.as_array().unwrap().len() {
            for j in 0..wn[i].as_array().unwrap().len() {
                let d = fvec(&wd[i][j]);
                ok &= coeff_cmp(st, "tf coefficients (T2·Π(1+|λ|)/‖den‖₁)", &fvec(&gn[i][j]), &fvec(&wn[i][j]), &d, 1e-10);
                ok &= coeff_cmp(st, "tf coefficients (T2·Π(1+|λ|)/‖den‖₁)", &fvec(&gd[i][j]), &d, &d, 1e-10);
            }
        }
    } else {
        let d = fvec(&e["den"]);
        ok &= coeff_cmp(st, "tf coefficients (T2·Π(1+|λ|)/‖den‖₁)", &fvec(&r["num"]), &fvec(&e["num"]), &d, 1e-10);
        ok &= coeff_cmp(st, "tf coefficients (T2·Π(1+|λ|)/‖den‖₁)", &fvec(&r["den"]), &d, &d, 1e-10);
    }
    ok
}

fn pz_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let mut ok = true;
    let (gp, wp) = (cl(&r["poles"]), cl(&e["poles"]));
    let (gz, wz) = (cl(&r["zeros"]), cl(&e["zeros"]));
    ok &= roots_cmp(st, "poles (T2·κ_root)", &gp, &wp, &poly_of(&wp), 1e-10);
    ok &= roots_cmp(st, "zeros (T2·κ_root)", &gz, &wz, &poly_of(&wz), 1e-10);
    let gd = mat(&r["dcgain"]["value"]);
    let wd = fvec(&e["dcgain"]);
    if gd.len() != wd.len() {
        st.failures.push("dcgain size".into());
        return false;
    }
    for (a, b) in gd.iter().zip(&wd) {
        if b.is_infinite() || a.is_infinite() {
            ok &= st.err("dcgain ±Inf agreement (T0)", if a == b { 0.0 } else { 1.0 }, 0.0, 0.0);
        } else {
            // dc = D − C A⁻¹ B (or N(0)/D(0)): κ = Π(1+|p|)/Π|p| (sensitivity of the constant term)
            let k: f64 = (wp.iter().map(|p| 1.0 + p.norm()).product::<f64>() / wp.iter().map(|p| p.norm()).product::<f64>().max(1e-300)).max(1.0);
            if k > 1.0 {
                st.scaled_cases += 1;
            }
            ok &= st.err("dcgain abs/max(1,|ref|) (T2·κ)", (a - b).abs() / b.abs().max(1.0), 1e-10 * k, 1e-10);
        }
    }
    if let (Some(wn), Some(z)) = (e.get("natural_frequency"), e.get("damping_ratio")) {
        let (wn, z) = (fvec(wn), fvec(z));
        let gwn = mat(&r["natural_frequency"]["value"]);
        let gz_ = mat(&r["damping_ratio"]);
        let pp = poly_of(&wp);
        for (k, p) in wp.iter().enumerate() {
            let j = (0..gp.len()).min_by(|&a, &b| (gp[a] - p).norm().partial_cmp(&(gp[b] - p).norm()).unwrap()).unwrap();
            let kap = kappa_root(&pp, *p).max(1.0);
            ok &= st.err("natural_frequency rel (T2·κ_root)", (gwn[j] - wn[k]).abs() / wn[k].abs().max(1.0), 1e-10 * kap, 1e-10);
            if z[k].is_finite() {
                ok &= st.err("damping_ratio abs (T2·κ_root)", (gz_[j] - z[k]).abs(), 1e-10 * kap, 1e-10);
            }
        }
    }
    ok
}

fn co_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let mut ok = true;
    let dr = (r["ctrb"]["rank"].as_i64().unwrap() - e["ctrb_rank"].as_i64().unwrap()).abs() + (r["obsv"]["rank"].as_i64().unwrap() - e["obsv_rank"].as_i64().unwrap()).abs();
    ok &= st.err("ctrb/obsv rank (T0)", dr as f64, 0.0, 0.0);
    ok &= st.err("ctrb/obsv matrices rel_fro (T1)", rel_fro(&mat(&r["ctrb"]["matrix"]), &mat(&e["ctrb"])), 1e-12, 1e-12);
    ok &= st.err("ctrb/obsv matrices rel_fro (T1)", rel_fro(&mat(&r["obsv"]["matrix"]), &mat(&e["obsv"])), 1e-12, 1e-12);
    for (g, w) in [(&r["ctrb"]["singular_values"], &e["ctrb_sv"]), (&r["obsv"]["singular_values"], &e["obsv_sv"])] {
        let (g, w) = (mat(g), fvec(w));
        let smax = w[0];
        let err = g.iter().zip(&w).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / smax;
        ok &= st.err("singular values abs/σ_max (T1)", err, 1e-12, 1e-12);
    }
    if e.get("Wc").is_some() {
        let res = &env["trust"]["residual"];
        let tol = (10.0 * f(&res["controllability_rel_residual"]).max(f(&res["observability_rel_residual"])) / f(&res["separation_min"]).min(1.0)).max(1e-10);
        if tol > 1e-10 {
            st.scaled_cases += 1;
        }
        ok &= st.err("gramians rel_fro (T2; max(1e-10, 10·res/min(1,sep)))", rel_fro(&mat(&r["gramians"]["controllability"]), &mat(&e["Wc"])), tol, 1e-10);
        ok &= st.err("gramians rel_fro (T2; max(1e-10, 10·res/min(1,sep)))", rel_fro(&mat(&r["gramians"]["observability"]), &mat(&e["Wo"])), tol, 1e-10);
    }
    ok
}

fn relx(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() {
        if a == b || (a.is_nan() && b.is_nan()) {
            0.0
        } else {
            f64::INFINITY
        }
    } else if a == b {
        0.0
    } else {
        (a - b).abs() / b.abs().max(1e-300)
    }
}

fn margin_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let mut ok = true;
    ok &= st.err("gain_margin rel (T3 1e-9)", relx(f(&r["gain_margin"]["value"]), f(&e["gm"])), 1e-9, 1e-9);
    let (pg, pw) = (f(&r["phase_margin"]["value"]), f(&e["pm"]));
    ok &= st.err("phase_margin abs deg (T3 1e-8)", if pw.is_finite() && pg.is_finite() { (pg - pw).abs() } else { relx(pg, pw) }, 1e-8, 1e-8);
    ok &= st.err("crossover frequencies rel (T3 1e-9)", relx(f(&r["phase_crossover_frequency"]["value"]), f(&e["wpc"])), 1e-9, 1e-9);
    ok &= st.err("crossover frequencies rel (T3 1e-9)", relx(f(&r["gain_crossover_frequency"]["value"]), f(&e["wgc"])), 1e-9, 1e-9);
    let (smg, smw) = (f(&r["stability_margin"]["value"]), f(&e["sm"]));
    let sys = &case["request"]["system"];
    let ts = sys["ts"].as_f64().unwrap_or(0.0);
    if ts > 0.0 && smw.is_finite() {
        // python-control's discrete SM comes from a local optimizer (scipy L-BFGS-B over θ ∈ [0, 2π]); its value
        // is a sampled upper bound of min|1+G|. Pass if within T3, or if ours is lower (a better minimum) while the
        // reference value is consistent with |1+G(e^{jω_ref ts})| evaluated here (≤ 1e-9) and the gap ≤ 1e-6.
        let (nm, dn) = (fvec(&sys["num"]), fvec(&sys["den"]));
        let fz = |w: f64| {
            let z = C64::new(0.0, w * ts).exp();
            (polyval(&nm, z) / polyval(&dn, z) + 1.0).norm()
        };
        let consistent = relx(fz(f(&e["wms"])), smw) <= 1e-9;
        let rel = relx(smg, smw);
        let better = smg <= smw && consistent && rel <= 1e-6;
        if better && rel > 1e-9 {
            st.notes.push(format!("discrete SM below python-control's optimizer value by rel {rel:.2e} (reference consistent; ours is a lower minimum)"));
        }
        ok &= st.err("stability_margin rel (T3 1e-9; discrete: or a lower consistent minimum)", if better { 0.0 } else { rel }, 1e-9, 1e-9);
        // optimizer xtol: discrete SM frequency compared at 1e-3 rel (also mod the θ ↔ 2π−θ mirror)
        let (wg, ww) = (f(&r["stability_margin_frequency"]["value"]), f(&e["wms"]));
        let mirror = 2.0 * std::f64::consts::PI / ts - ww;
        ok &= st.err("stability_margin frequency rel (discrete: 1e-3, optimizer xtol)", relx(wg, ww).min(relx(wg, mirror)), 1e-3, 1e-3);
    } else {
        ok &= st.err("stability_margin rel (T3 1e-9; discrete: or a lower consistent minimum)", relx(smg, smw), 1e-9, 1e-9);
        // a minimum's abscissa is determined only to O(√u)·scale → documented looser tolerance
        ok &= st.err("stability_margin frequency rel (continuous: 1e-6, flat minimum)", relx(f(&r["stability_margin_frequency"]["value"]), f(&e["wms"])), 1e-6, 1e-6);
    }
    for (key, theirs) in [("phase", "all_w180"), ("gain", "all_wc")] {
        let ow: Vec<f64> = r["crossings"][key].as_array().unwrap().iter().map(|x| f(&x["w_rad_s"])).collect();
        let tw = fvec(&e[theirs]);
        for w in &tw {
            if !ow.iter().any(|x| (x - w).abs() <= 1e-9 * w.abs().max(1e-12)) {
                st.failures.push(format!("{key} crossing {w} reported by python-control not found (ours {ow:?})"));
                ok = false;
            }
        }
        if ow.len() > tw.len() {
            st.notes.push(format!("{} extra {key} crossing(s) vs python-control in {}", ow.len() - tw.len(), case["case_family"]));
        }
    }
    ok
}

fn c2d_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"]["system"];
    let mut ok = true;
    let t = &env["trust"]["condition"];
    let kappa = if t["cond_I_minus_alpha_A"].is_number() { f(&t["cond_I_minus_alpha_A"]) } else { f(&t["norm1_A_ts"]).exp() };
    let kappa = kappa.max(1.0);
    let tol = 1e-10 * kappa;
    if kappa > 1.0 {
        st.scaled_cases += 1;
    }
    if e.get("A").is_some() {
        for k in ["A", "B", "C", "D"] {
            ok &= st.err("discrete ss matrices rel_fro (T2·κ: cond(I−αA) | e^{‖A‖₁T})", rel_fro(&mat(&r[k]), &mat(&e[k])), tol, 1e-10);
        }
    } else {
        let d = fvec(&e["den"]);
        ok &= coeff_cmp(st, "discrete tf coefficients (T2·κ·Π(1+|λ|)/‖den‖₁)", &fvec(&r["num"]), &fvec(&e["num"]), &d, tol);
        ok &= coeff_cmp(st, "discrete tf coefficients (T2·κ·Π(1+|λ|)/‖den‖₁)", &fvec(&r["den"]), &d, &d, tol);
    }
    ok
}

fn impulse_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let y = mat(&r["y"]["value"]);
    let w = fvec(&e["y"]);
    let t = mat(&r["t"]["value"]);
    if y.len() != w.len() || t.len() != fvec(&e["t"]).len() {
        st.failures.push(format!("length mismatch y {} vs {}, t {} vs {}", y.len(), w.len(), t.len(), fvec(&e["t"]).len()));
        return false;
    }
    let ymax = w.iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-300);
    let err = y.iter().zip(&w).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max) / ymax;
    st.err("y max abs / max|y| (T2)", err, 1e-10, 1e-10)
}

fn lyap_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let est = f(&env["trust"]["error_estimate"]["X_rel"]);
    let tol = (10.0 * est).max(1e-10);
    if tol > 1e-10 {
        st.scaled_cases += 1;
    }
    st.err("X rel_fro (T2; max(1e-10, 10·trust X_rel))", rel_fro(&mat(&env["result"]["X"]), &mat(&case["expected"]["X"])), tol, 1e-10)
}

fn place_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let mut ok = true;
    let want = cl(&e["poles"]);
    // independent check: eig(A − B·K) recomputed here from the *reference* request (not trusting achieved_poles)
    let dm = |v: &Value| {
        let sh: Vec<usize> = v["shape"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
        nalgebra::DMatrix::from_row_slice(sh[0], sh[1], &mat(v))
    };
    let (a, b, k) = (dm(&case["request"]["A"]), dm(&case["request"]["B"]), dm(&r["K"]));
    if k.nrows() != b.ncols() || k.ncols() != a.nrows() {
        st.failures.push("K shape".into());
        return false;
    }
    let got = aeoi_core::lti::eigenvalues(&(&a - &b * &k));
    let c = &env["trust"]["condition"];
    let cx = f(&c["cond_X_eigvec"]);
    let tol = 1e-10 * cx.max(1.0) * (1.0 + f(&c["K_norm_fro"]));
    st.scaled_cases += 1;
    match matched(&got, &want) {
        Some(pairs) => {
            for (er, _) in pairs {
                ok &= st.err("achieved poles rel (T2·cond(X)·(1+‖K‖_F))", er, tol, 1e-10);
            }
        }
        None => ok = false,
    }
    if e.get("K").is_some() {
        ok &= st.err("SISO K rel_fro (T2·cond(X)·(1+‖K‖_F))", rel_fro(&mat(&r["K"]), &mat(&e["K"])), tol, 1e-10);
    }
    let ratio = cx / f(&e["cond_X_scipy"]);
    let m = st.max_err.entry("cond(X) ours / scipy-YT (informational)".into()).or_insert(0.0);
    if ratio > *m {
        *m = ratio;
    }
    st.tol.entry("cond(X) ours / scipy-YT (informational)".into()).or_insert(f64::NAN);
    ok
}

fn dlqr_cmp(st: &mut Stat, case: &Value, env: &Value) -> bool {
    let e = &case["expected"];
    let r = &env["result"];
    let c = &env["trust"]["condition"];
    let est = f(&env["trust"]["error_estimate"]["S_rel"]);
    let tol = (1e-10 * (f(&c["cond_S"]) / 1e2).max(1.0)).max(10.0 * est);
    if tol > 1e-10 {
        st.scaled_cases += 1;
    }
    let mut ok = st.err("S rel_fro (T2; lqr V-6 rule)", rel_fro(&mat(&r["S"]), &mat(&e["S"])), tol, 1e-10);
    ok &= st.err("K rel_fro (T2; lqr V-6 rule)", rel_fro(&mat(&r["K"]), &mat(&e["K"])), tol, 1e-10);
    let want = cl(&e["closed_loop_poles"]);
    ok &= roots_cmp(st, "closed-loop poles (T2·κ_root, lqr rule)", &cl(&r["closed_loop_poles"]), &want, &poly_of(&want), tol);
    ok
}

fn compare(op: &str, st: &mut Stat, case: &Value, env: &Value) -> bool {
    match op {
        "control.lti.convert" => convert_cmp(st, case, env),
        "control.lti.poles_zeros" => pz_cmp(st, case, env),
        "control.analysis.ctrb_obsv" => co_cmp(st, case, env),
        "control.analysis.margin" => margin_cmp(st, case, env),
        "control.lti.c2d" => c2d_cmp(st, case, env),
        "control.lti.impulse" => impulse_cmp(st, case, env),
        "control.math.lyap" => lyap_cmp(st, case, env),
        "control.design.place" => place_cmp(st, case, env),
        "control.design.dlqr" => dlqr_cmp(st, case, env),
        _ => unreachable!(),
    }
}

/// Deviation (× tolerance) of the column-major mistake request from the correct reference: must be ≫ 1.
fn mistake_ratio(op: &str, case: &Value, env2: &Value) -> f64 {
    if env2["status"] == "error" {
        return f64::INFINITY;
    }
    let mut st = Stat::default();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compare(op, &mut st, case, env2)));
    if !st.failures.is_empty() {
        return f64::INFINITY;
    }
    st.max_ratio
}

pub const OPS: [&str; 9] = [
    "control.lti.convert", "control.lti.poles_zeros", "control.analysis.ctrb_obsv", "control.analysis.margin", "control.lti.c2d",
    "control.lti.impulse", "control.math.lyap", "control.design.place", "control.design.dlqr",
];

#[test]
fn parity_python_control_batch2() {
    let mut report = serde_json::Map::new();
    let mut all_ok = true;
    for op in OPS {
        let mut st = Stat::default();
        let mut families: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut mo = None;
        for (file, case) in load(op) {
            st.cases += 1;
            let fam = case["case_family"].as_str().unwrap().to_string();
            let name = format!("{fam}/{}", &file[..file.len().min(12)]);
            let env = call(op, &case["request"]);
            let nf = st.failures.len();
            let ok = match pre(&mut st, &name, &case, &env) {
                Some(b) => b,
                None => compare(op, &mut st, &case, &env),
            };
            if !ok && st.failures.len() == nf {
                st.failures.push(format!("{name}: tolerance exceeded (max err/tol so far {:.3e})", st.max_ratio));
            }
            for k in st.failures.iter_mut().skip(nf) {
                if !k.starts_with(&fam) {
                    *k = format!("{name}: {k}");
                }
            }
            if let Some(bad) = case.get("column_major_mistake_request") {
                let env2 = call(op, bad);
                let ratio = mistake_ratio(op, &case, &env2);
                mo = Some(ratio);
                assert!(ratio > 1e3, "{op}: column-major mistake not detected (ratio {ratio})");
            }
            let e = families.entry(fam).or_insert((0, 0));
            e.0 += 1;
            if ok {
                e.1 += 1;
                st.passed += 1;
            }
        }
        println!("\n=== {op}: {}/{} passed; max error/tolerance = {:.3e}; κ-scaled comparisons = {}", st.passed, st.cases, st.max_ratio, st.scaled_cases);
        for (k, v) in &st.max_err {
            println!("    {k:<70} max {v:.3e}   base tol {:.1e}", st.tol[k]);
        }
        if let Some(r) = mo {
            println!("    memory-order: column-major request deviates by {r:.3e} × tolerance (detected)");
        }
        for nte in st.notes.iter().take(5) {
            println!("    note: {nte}");
        }
        for fl in &st.failures {
            println!("    FAIL {fl}");
        }
        all_ok &= st.failures.is_empty();
        report.insert(op.into(), json!({
            "cases": st.cases, "passed": st.passed, "max_error_over_tolerance": aeoi_core::num(st.max_ratio), "kappa_scaled_comparisons": st.scaled_cases,
            "max_errors": st.max_err.iter().map(|(k, v)| (k.clone(), aeoi_core::num(*v))).collect::<serde_json::Map<_, _>>(),
            "base_tolerances": st.tol.iter().map(|(k, v)| (k.clone(), aeoi_core::num(*v))).collect::<serde_json::Map<_, _>>(),
            "families": families.iter().map(|(k, v)| (k.clone(), json!({"cases": v.0, "passed": v.1}))).collect::<serde_json::Map<_, _>>(),
            "memory_order_mismatch_ratio": mo.map(aeoi_core::num), "notes": st.notes, "failures": st.failures,
        }));
    }
    let out = json!({"reference": "python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 (python-control/scipy reference values; not MATLAB)",
                     "build": aeoi_core::operator::build_string(), "cpu_features": aeoi_core::operator::cpu_features(), "results": report});
    std::fs::write(spec_root().join("reports/parity-python-control-b2.json"), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    assert!(all_ok, "parity failures (see output)");
}
