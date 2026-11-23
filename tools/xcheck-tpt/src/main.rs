//! Cross-check aeoi-control against tpt-control (independent pure-Rust control library).
//!
//! usage: xcheck-tpt <repo-root> <out.jsonl>
//! For every case of the wasm-vs-native corpus (golden/python-control/*/*/*.json + examples/*/request*.json)
//! plus seeded random well-/ill-conditioned cases, runs the AEOI operator and the tpt-control
//! equivalent (with convention mapping, see `map_*`), and writes one JSON record per case. Direct
//! metrics (normwise relative differences) are computed here; Lyapunov/Riccati matrices are emitted so
//! `analyze.py` can compute residuals and forward errors in high precision (mpmath).
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tpt_ctrl_classic::{freq, tf::TransferFunction};
use tpt_ctrl_core::{analysis, discretize, DMat, C64};
use tpt_ctrl_optimal::{care, dare};

// ------------------------------------------------------------------ conversions
fn num(x: &Value) -> f64 {
    match x {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "inf" | "Infinity" | "+inf" => f64::INFINITY,
            "-inf" | "-Infinity" => f64::NEG_INFINITY,
            _ => f64::NAN,
        },
        _ => f64::NAN,
    }
}
fn dm(v: &Value) -> Option<DMat<f64>> {
    if v.get("dtype")?.as_str()? != "f64" {
        return None;
    }
    let sh: Vec<usize> = v["shape"].as_array()?.iter().map(|x| x.as_u64().unwrap() as usize).collect();
    let data: Vec<f64> = v["data"].as_array()?.iter().map(num).collect();
    let (r, c) = match sh.len() {
        2 => (sh[0], sh[1]),
        1 => (sh[0], 1),
        0 => (1, 1),
        _ => return None,
    };
    Some(DMat::from_fn(r, c, |i, j| data[i * c + j]))
}
fn rows(m: &DMat<f64>) -> Vec<Vec<f64>> {
    (0..m.nrows()).map(|i| (0..m.ncols()).map(|j| m[(i, j)]).collect()).collect()
}
fn mj(m: &DMat<f64>) -> Value {
    json!(rows(m).iter().map(|r| r.iter().map(|x| fj(*x)).collect::<Vec<_>>()).collect::<Vec<_>>())
}
/// JSON-safe float (non-finite as string)
fn fj(x: f64) -> Value {
    if x.is_finite() {
        json!(x)
    } else {
        json!(format!("{x}"))
    }
}
fn flat(v: &Value) -> Vec<f64> {
    // nested or flat numeric arrays → flat
    match v {
        Value::Array(a) => a.iter().flat_map(flat).collect(),
        x => vec![num(x)],
    }
}
fn cvec(v: &Value) -> Vec<C64> {
    let data = v["data"].as_array().cloned().unwrap_or_default();
    match v["dtype"].as_str() {
        Some("c128") => data.iter().map(|p| C64::new(num(&p[0]), num(&p[1]))).collect(),
        _ => data.iter().map(|x| C64::new(num(x), 0.0)).collect(),
    }
}
fn cj(v: &[C64]) -> Value {
    json!(v.iter().map(|c| json!([fj(c.re), fj(c.im)])).collect::<Vec<_>>())
}
fn eye(n: usize) -> DMat<f64> {
    DMat::identity(n)
}
fn t(m: &DMat<f64>) -> DMat<f64> {
    m.transpose()
}
fn mul(a: &DMat<f64>, b: &DMat<f64>) -> DMat<f64> {
    a * b
}

// ------------------------------------------------------------------ metrics
/// normwise relative difference max|a-b| / max(max|b|, tiny); both NaN-free required
fn nrel(a: &[f64], b: &[f64]) -> Value {
    if a.len() != b.len() {
        return json!({"shape_mismatch": [a.len(), b.len()]});
    }
    let (mut d, mut s) = (0.0f64, 0.0f64);
    let mut nonfinite = 0;
    for (x, y) in a.iter().zip(b) {
        if !x.is_finite() || !y.is_finite() {
            if !(x == y) {
                nonfinite += 1;
            }
            continue;
        }
        d = d.max((x - y).abs());
        s = s.max(y.abs());
    }
    json!({"max_abs": d, "rel": if s > 0.0 { d / s } else { d }, "nonfinite_mismatch": nonfinite})
}
fn mflat(m: &DMat<f64>) -> Vec<f64> {
    rows(m).concat()
}
/// match two complex multisets greedily by nearest neighbour; returns (max abs diff, max rel diff)
fn match_c(a: &[C64], b: &[C64]) -> Value {
    if a.len() != b.len() {
        return json!({"count_mismatch": [a.len(), b.len()]});
    }
    let mut used = vec![false; b.len()];
    let (mut da, mut dr) = (0.0f64, 0.0f64);
    // match larger-magnitude first for stability
    let mut idx: Vec<usize> = (0..a.len()).collect();
    idx.sort_by(|&i, &j| a[j].abs().partial_cmp(&a[i].abs()).unwrap_or(std::cmp::Ordering::Equal));
    for i in idx {
        let mut best = (f64::INFINITY, usize::MAX);
        for (j, y) in b.iter().enumerate() {
            if !used[j] {
                let d = (a[i] - *y).abs();
                if d < best.0 || best.1 == usize::MAX {
                    best = (d, j);
                }
            }
        }
        used[best.1] = true;
        da = da.max(best.0);
        dr = dr.max(best.0 / a[i].abs().max(1e-300).max(b[best.1].abs()).max(1.0f64.min(a[i].abs().max(b[best.1].abs())).max(1e-300)));
    }
    json!({"max_abs": da, "rel": dr})
}

// ------------------------------------------------------------------ AEOI side
fn summary(env: &Value) -> Value {
    json!({"status": env["status"], "error_code": env["error"]["code"],
           "warnings": env["warnings"].as_array().map(|w| w.iter().map(|x| x["code"].clone()).collect::<Vec<_>>()).unwrap_or_default()})
}
fn is_ok(env: &Value) -> bool {
    matches!(env["status"].as_str(), Some("ok") | Some("ok_with_warnings"))
}
fn terr(e: impl std::fmt::Debug) -> Value {
    json!({"error": format!("{e:?}")})
}

struct Sys {
    a: DMat<f64>,
    b: DMat<f64>,
    c: DMat<f64>,
    d: DMat<f64>,
    ts: f64,
    from_tf: bool,
}
fn tf_of(s: &Value) -> Option<(Vec<f64>, Vec<f64>)> {
    if s["kind"] != "tf" {
        return None;
    }
    // SISO only: flat arrays
    let n = s["num"].as_array()?;
    if n.iter().any(|x| x.is_array()) {
        return None;
    }
    Some((flat(&s["num"]), flat(&s["den"])))
}
fn trim(p: &[f64]) -> Vec<f64> {
    let i = p.iter().position(|x| *x != 0.0).unwrap_or(p.len().saturating_sub(1));
    p[i..].to_vec()
}
fn sys_of(s: &Value) -> Option<Sys> {
    let ts = s["ts"].as_f64().unwrap_or(0.0);
    if s["kind"] == "ss" {
        return Some(Sys { a: dm(&s["A"])?, b: dm(&s["B"])?, c: dm(&s["C"])?, d: dm(&s["D"])?, ts, from_tf: false });
    }
    let (n, d) = tf_of(s)?;
    let tf = TransferFunction::new(&trim(&n), &trim(&d)).ok()?;
    let (a, b, c, dd) = tf.to_state_space();
    Some(Sys { a, b, c, d: dd, ts, from_tf: true })
}

// ------------------------------------------------------------------ tpt mappings
/// AEOI lyap: continuous A X + X Aᵀ + Q = 0, discrete A X Aᵀ − X + Q = 0.
/// tpt: continuous Aᵀ X + X A = −Q, discrete Aᵀ X A − X = −Q  → pass Aᵀ.
fn map_lyap(req: &Value) -> Option<Value> {
    let a = dm(&req["A"])?;
    let q = dm(&req["Q"])?;
    let disc = req["type"] == "discrete";
    let t0 = Instant::now();
    let r = if disc { tpt_ctrl_core::lyapunov::solve_discrete_lyapunov(&t(&a), &q) } else { tpt_ctrl_core::lyapunov::solve_lyapunov(&t(&a), &q) };
    let us = t0.elapsed().as_secs_f64() * 1e6;
    Some(match r {
        Ok(x) => json!({"X": mj(&x), "us": us}),
        Err(e) => terr(e),
    })
}
/// AEOI lqr/dlqr: optional cross term N. tpt has no N → standard reduction
/// Ā = A − B R⁻¹ Nᵀ, Q̄ = Q − N R⁻¹ Nᵀ; K = R⁻¹(BᵀX + Nᵀ) (continuous), K = (R + BᵀXB)⁻¹(BᵀXA + Nᵀ) (discrete).
fn map_lqr(req: &Value, disc: bool) -> Option<Value> {
    let a = dm(&req["A"])?;
    let b = dm(&req["B"])?;
    let q = dm(&req["Q"])?;
    let r = dm(&req["R"])?;
    let n_cross = req.get("N").and_then(dm);
    let rinv = match r.inverse() {
        Ok(x) => x,
        Err(e) => return Some(terr(e)),
    };
    let (ab, qb) = match &n_cross {
        Some(nn) => (&a - &mul(&mul(&b, &rinv), &t(nn)), &q - &mul(&mul(nn, &rinv), &t(nn))),
        None => (a.clone(), q.clone()),
    };
    let t0 = Instant::now();
    let x = if disc { dare(&ab, &b, &qb, &r).map(|s| s.x) } else { care(&ab, &b, &qb, &r) };
    let us = t0.elapsed().as_secs_f64() * 1e6;
    let x = match x {
        Ok(x) => x,
        Err(e) => return Some(terr(e)),
    };
    let nt = n_cross.as_ref().map(t).unwrap_or_else(|| DMat::zeros(b.ncols(), a.nrows()));
    let k = if disc {
        let lhs = &r + &mul(&mul(&t(&b), &x), &b);
        match lhs.solve(&(&mul(&mul(&t(&b), &x), &a) + &nt)) {
            Ok(k) => k,
            Err(e) => return Some(terr(e)),
        }
    } else {
        mul(&rinv, &(&mul(&t(&b), &x) + &nt))
    };
    let cl = (&a - &mul(&b, &k)).eigenvalues().unwrap_or_default();
    Some(json!({"X": mj(&x), "K": mj(&k), "closed_loop_poles": cj(&cl), "us": us, "cross_term_reduced": n_cross.is_some()}))
}

fn map_c2d(req: &Value) -> Option<(Value, Sys)> {
    let s = &req["system"];
    if s["kind"] != "ss" || req.get("prewarp_frequency").map(|v| !v.is_null()).unwrap_or(false) {
        return None;
    }
    let sy = sys_of(s)?;
    let ts = req["ts"].as_f64()?;
    let method = req["method"].as_str().unwrap_or("zoh");
    let out = match method {
        "zoh" => discretize::zoh(&sy.a, &sy.b, ts).map(|(ad, bd)| (ad, bd, sy.c.clone(), sy.d.clone())),
        "tustin" => discretize::tustin(&sy.a, &sy.b, &sy.c, &sy.d, ts),
        _ => return None,
    };
    Some((
        match out {
            Ok((ad, bd, cd, dd)) => json!({"A": mj(&ad), "B": mj(&bd), "C": mj(&cd), "D": mj(&dd)}),
            Err(e) => terr(e),
        },
        sy,
    ))
}

fn aeoi_ss(v: &Value) -> Option<[DMat<f64>; 4]> {
    Some([dm(&v["A"])?, dm(&v["B"])?, dm(&v["C"])?, dm(&v["D"])?])
}

fn omega_of(res: &Value) -> Option<Vec<f64>> {
    let w = flat(&res["w"]["value"]["data"]);
    let unit = res["w"]["unit"].as_str().unwrap_or("rad/s");
    Some(if unit == "Hz" { w.iter().map(|f| 2.0 * std::f64::consts::PI * f).collect() } else { w })
}

/// discrete-time simulation with tpt (Ad, Bd); y[p][m][N]
fn sim(ad: &DMat<f64>, bd: &DMat<f64>, b_cont: &DMat<f64>, c: &DMat<f64>, d: &DMat<f64>, nsteps: usize, impulse: Option<(f64, bool)>) -> Vec<f64> {
    let (n, m, p) = (ad.nrows(), bd.ncols(), c.nrows());
    let mut y = vec![0.0; p * m * nsteps];
    for j in 0..m {
        let (x0, us, _dscale): (Vec<f64>, Vec<Vec<f64>>, f64) = match impulse {
            None => (vec![0.0; n], vec![(0..m).map(|i| if i == j { 1.0 } else { 0.0 }).collect()], 1.0),
            // continuous impulse (python-control): x0 = B e_j, u = 0, D ignored
            Some((_, false)) => ((0..n).map(|i| b_cont[(i, j)]).collect(), vec![vec![0.0; m]], 0.0),
            // discrete impulse: u[0] = 1/dt (unit area), then 0
            Some((dt, true)) => (vec![0.0; n], vec![(0..m).map(|i| if i == j { 1.0 / dt } else { 0.0 }).collect(), vec![0.0; m]], 0.0),
        };
        let bsim = match impulse {
            Some((_, false)) => DMat::zeros(n, m),
            _ => bd.clone(),
        };
        let tr = tpt_ctrl_core::simulate::simulate_discrete(ad, &bsim, &x0, &us, nsteps - 1);
        for k in 0..nsteps {
            let xk = &tr.states[k];
            let uk: Vec<f64> = if k < us.len() { us[k].clone() } else { us.last().unwrap().clone() };
            for i in 0..p {
                let mut v = 0.0;
                for l in 0..n {
                    v += c[(i, l)] * xk[l];
                }
                for l in 0..m {
                    v += d[(i, l)] * uk[l] * if matches!(impulse, Some((_, false))) { 0.0 } else { 1.0 };
                }
                y[(i * m + j) * nsteps + k] = v;
            }
        }
    }
    y
}

fn run_case(case: &str, op: &str, req: &Value, env: &Value, expected: Option<&Value>) -> Value {
    let mut rec = json!({"case": case, "operator": op, "aeoi": summary(env)});
    let res = &env["result"];
    let ok = is_ok(env);
    let set = |rec: &mut Value, k: &str, v: Value| {
        rec[k] = v;
    };
    match op {
        "control.math.lyap" => {
            set(&mut rec, "inputs", json!({"A": mj(&dm(&req["A"]).unwrap()), "Q": mj(&dm(&req["Q"]).unwrap()), "type": req["type"]}));
            let tp = map_lyap(req).unwrap();
            if ok {
                let xa = dm(&res["X"]).unwrap();
                rec["aeoi"]["X"] = mj(&xa);
                if let Some(xt) = tp.get("X").map(|v| DMat::from_fn(xa.nrows(), xa.ncols(), |i, j| num(&v[i][j]))) {
                    rec["diff"] = json!({"X": nrel(&mflat(&xt), &mflat(&xa))});
                }
            }
            if let Some(e) = expected.and_then(|e| dm(&e["X"])) {
                rec["python_control"] = json!({"X": mj(&e)});
            }
            rec["tpt"] = tp;
        }
        "control.design.lqr" | "control.design.dlqr" => {
            let disc = op.ends_with("dlqr");
            let mut inp = json!({"A": mj(&dm(&req["A"]).unwrap()), "B": mj(&dm(&req["B"]).unwrap()), "Q": mj(&dm(&req["Q"]).unwrap()), "R": mj(&dm(&req["R"]).unwrap())});
            if let Some(nn) = req.get("N").and_then(dm) {
                inp["N"] = mj(&nn);
            }
            rec["inputs"] = inp;
            let tp = map_lqr(req, disc).unwrap();
            if ok {
                let (k, s) = (dm(&res["K"]).unwrap(), dm(&res["S"]).unwrap());
                rec["aeoi"]["K"] = mj(&k);
                rec["aeoi"]["S"] = mj(&s);
                rec["aeoi"]["closed_loop_poles"] = cj(&cvec(&res["closed_loop_poles"]));
                if let (Some(xv), Some(kv)) = (tp.get("X"), tp.get("K")) {
                    let xt: Vec<f64> = flat(xv);
                    let kt: Vec<f64> = flat(kv);
                    rec["diff"] = json!({"S": nrel(&xt, &mflat(&s)), "K": nrel(&kt, &mflat(&k))});
                }
            }
            if let Some(e) = expected {
                if let (Some(k), Some(s)) = (dm(&e["K"]), dm(&e["S"])) {
                    rec["python_control"] = json!({"K": mj(&k), "S": mj(&s)});
                }
            }
            rec["tpt"] = tp;
        }
        "control.lti.c2d" => match map_c2d(req) {
            None => rec["skip"] = json!("tf/zpk input, foh, or tustin with prewarp: no tpt equivalent"),
            Some((tp, sy)) => {
                rec["inputs"] = json!({"A": mj(&sy.a), "B": mj(&sy.b), "C": mj(&sy.c), "D": mj(&sy.d), "ts": req["ts"], "method": req["method"]});
                if ok {
                    if let (Some(s), Some(_)) = (aeoi_ss(&res["system"]), tp.get("A")) {
                        let mut d = json!({});
                        for (i, k) in ["A", "B", "C", "D"].iter().enumerate() {
                            d[*k] = nrel(&flat(&tp[*k]), &mflat(&s[i]));
                        }
                        rec["diff"] = d;
                        rec["aeoi"]["system"] = json!({"A": mj(&s[0]), "B": mj(&s[1]), "C": mj(&s[2]), "D": mj(&s[3])});
                    }
                }
                rec["tpt"] = tp;
            }
        },
        "control.lti.bode" => {
            let s = &req["system"];
            let tfv = tf_of(s).filter(|_| s["ts"].as_f64().unwrap_or(0.0) == 0.0);
            match (tfv, ok) {
                (None, _) => rec["skip"] = json!("ss/zpk or discrete system: tpt bode works on continuous SISO tf only"),
                (_, false) => rec["skip"] = json!("aeoi error"),
                (Some((n, d)), true) => match TransferFunction::new(&trim(&n), &trim(&d)) {
                    Err(e) => rec["tpt"] = terr(e),
                    Ok(tf) => {
                        let w = omega_of(res).unwrap();
                        let bd = freq::bode_data(&tf, &w);
                        let mag = flat(&res["magnitude"]["value"]["data"]);
                        let ph = flat(&res["phase"]["value"]["data"]);
                        let mag_db: Vec<f64> = if res["magnitude"]["unit"] == "dB" { mag.clone() } else { mag.iter().map(|x| 20.0 * x.log10()).collect() };
                        let dmag = mag_db.iter().zip(&bd.magnitude_db).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
                        let dph = ph.iter().zip(&bd.phase_deg).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
                        let dph_mod = ph.iter().zip(&bd.phase_deg).map(|(a, b)| { let r = (a - b).rem_euclid(360.0); r.min(360.0 - r) }).fold(0.0, f64::max);
                        let offset360 = ph.first().zip(bd.phase_deg.first()).map(|(a, b)| ((a - b) / 360.0).round()).unwrap_or(0.0);
                        rec["diff"] = json!({"magnitude_db_max_abs": dmag, "phase_deg_max_abs": dph, "phase_deg_max_abs_mod360": dph_mod, "phase_offset_turns_at_w0": offset360, "points": w.len()});
                    }
                },
            }
        }
        "control.analysis.margin" => {
            let s = &req["system"];
            match tf_of(s).filter(|_| s["ts"].as_f64().unwrap_or(0.0) == 0.0) {
                None => rec["skip"] = json!("ss/zpk or discrete: tpt margins work on continuous SISO tf only"),
                Some((n, d)) => match TransferFunction::new(&trim(&n), &trim(&d)) {
                    Err(e) => rec["tpt"] = terr(e),
                    Ok(tf) => {
                        let mut mags: Vec<f64> = tf.poles().iter().chain(tf.zeros().iter()).map(|c| c.abs()).filter(|x| *x > 1e-9).collect();
                        mags.push(1.0);
                        let lo = mags.iter().cloned().fold(f64::INFINITY, f64::min) * 1e-3;
                        let hi = mags.iter().cloned().fold(0.0, f64::max) * 1e3;
                        let t0 = Instant::now();
                        let m = freq::margins(&tf, lo, hi, 20000).unwrap();
                        let us = t0.elapsed().as_secs_f64() * 1e6;
                        // tpt naming: gain_crossover_omega = |L| = 1 crossing (despite its doc comment), phase_crossover_omega = −180°
                        rec["tpt"] = json!({"gain_margin": fj(m.gain_margin), "phase_margin_deg": fj(m.phase_margin_deg),
                            "w_gain_crossover": m.gain_crossover_omega, "w_phase_crossover": m.phase_crossover_omega, "grid": [lo, hi, 20000], "us": us});
                        if ok {
                            rec["aeoi"]["gain_margin"] = res["gain_margin"]["value"].clone();
                            rec["aeoi"]["phase_margin_deg"] = res["phase_margin"]["value"].clone();
                            rec["aeoi"]["w_gain_crossover"] = res["gain_crossover_frequency"]["value"].clone();
                            rec["aeoi"]["w_phase_crossover"] = res["phase_crossover_frequency"]["value"].clone();
                        }
                    }
                },
            }
        }
        "control.lti.poles_zeros" => {
            let s = &req["system"];
            let (tp_p, tp_z): (Option<Vec<C64>>, Option<Vec<C64>>) = if let Some((n, d)) = tf_of(s) {
                match TransferFunction::new(&trim(&n), &trim(&d)) {
                    Ok(tf) => (Some(tf.poles()), Some(tf.zeros())),
                    Err(e) => {
                        rec["tpt"] = terr(e);
                        (None, None)
                    }
                }
            } else if s["kind"] == "ss" {
                (analysis::poles(&dm(&s["A"]).unwrap()).ok(), None)
            } else {
                rec["skip"] = json!("zpk input");
                (None, None)
            };
            if ok {
                let mut d = json!({});
                if let Some(p) = &tp_p {
                    d["poles"] = match_c(&cvec(&res["poles"]), p);
                }
                if let Some(z) = &tp_z {
                    d["zeros"] = match_c(&cvec(&res["zeros"]), z);
                }
                if tp_p.is_some() {
                    rec["diff"] = d;
                }
            }
            if let Some(p) = tp_p {
                rec["tpt"] = json!({"poles": cj(&p), "zeros": tp_z.map(|z| cj(&z))});
            }
        }
        "control.lti.step" | "control.lti.impulse" => {
            let impulse = op.ends_with("impulse");
            match (sys_of(&req["system"]), ok) {
                (None, _) => rec["skip"] = json!("zpk or MIMO tf input"),
                (_, false) => rec["skip"] = json!("aeoi error"),
                (Some(sy), true) => {
                    let tv = flat(&res["t"]["value"]["data"]);
                    let ya = flat(&res["y"]["value"]["data"]);
                    let nst = tv.len();
                    let dt = if nst > 1 { tv[1] - tv[0] } else { 1.0 };
                    let uniform = tv.windows(2).all(|w| ((w[1] - w[0]) - dt).abs() <= 1e-9 * dt.abs().max(1e-300) * 10.0 + 1e-12);
                    let disc = sy.ts > 0.0;
                    let (ad, bd) = if disc { (sy.a.clone(), sy.b.clone()) } else { discretize::zoh(&sy.a, &sy.b, dt).unwrap() };
                    if !uniform {
                        rec["skip"] = json!("non-uniform time grid");
                    } else {
                        let yt = sim(&ad, &bd, &sy.b, &sy.c, &sy.d, nst, if impulse { Some((sy.ts, disc)) } else { None });
                        rec["diff"] = json!({"y": nrel(&yt, &ya), "points": nst, "realization": if sy.from_tf { "tpt tf.to_state_space" } else { "input ss" }});
                    }
                }
            }
        }
        "control.analysis.ctrb_obsv" => {
            let s = &req["system"];
            match (sys_of(s), ok) {
                (Some(sy), true) => {
                    let cm = analysis::controllability_matrix(&sy.a, &sy.b).unwrap();
                    let om = analysis::observability_matrix(&sy.a, &sy.c).unwrap();
                    let (rc, ro) = (analysis::rank(&cm, 1e-10), analysis::rank(&om, 1e-10));
                    let n = sy.a.nrows();
                    let mut d = json!({
                        "ctrb_matrix": nrel(&mflat(&cm), &mflat(&dm(&res["ctrb"]["matrix"]).unwrap())),
                        "obsv_matrix": nrel(&mflat(&om), &mflat(&dm(&res["obsv"]["matrix"]).unwrap())),
                        "ctrb_rank": [res["ctrb"]["rank"].clone(), rc], "obsv_rank": [res["obsv"]["rank"].clone(), ro],
                        "ctrb_full_rank_agree": res["ctrb"]["full_rank"].as_bool() == Some(rc == n),
                        "obsv_full_rank_agree": res["obsv"]["full_rank"].as_bool() == Some(ro == n),
                    });
                    if let (Some(wc), Some(wo)) = (dm(&res["gramians"]["controllability"]), dm(&res["gramians"]["observability"])) {
                        let bbt = mul(&sy.b, &t(&sy.b));
                        let ctc = mul(&t(&sy.c), &sy.c);
                        let (gc, go) = if sy.ts > 0.0 {
                            (tpt_ctrl_core::lyapunov::solve_discrete_lyapunov(&t(&sy.a), &bbt), tpt_ctrl_core::lyapunov::solve_discrete_lyapunov(&sy.a, &ctc))
                        } else {
                            (tpt_ctrl_core::lyapunov::solve_lyapunov(&t(&sy.a), &bbt), tpt_ctrl_core::lyapunov::solve_lyapunov(&sy.a, &ctc))
                        };
                        d["gramian_c"] = gc.map(|g| nrel(&mflat(&g), &mflat(&wc))).unwrap_or_else(terr);
                        d["gramian_o"] = go.map(|g| nrel(&mflat(&g), &mflat(&wo))).unwrap_or_else(terr);
                    }
                    rec["diff"] = d;
                }
                (None, _) => rec["skip"] = json!("unsupported system"),
                (_, false) => rec["skip"] = json!("aeoi error"),
            }
        }
        "control.lti.convert" => {
            let s = &req["system"];
            match (tf_of(s), req["to"].as_str(), ok) {
                (Some((n, d)), Some("ss"), true) => match TransferFunction::new(&trim(&n), &trim(&d)) {
                    Err(e) => rec["tpt"] = terr(e),
                    Ok(tf) => {
                        let (ta, tb, tc, td) = tf.to_state_space();
                        let s2 = aeoi_ss(&res["system"]).unwrap();
                        // Markov parameters D, CB, CAB, ... (realization-invariant)
                        let markov = |a: &DMat<f64>, b: &DMat<f64>, c: &DMat<f64>, d: &DMat<f64>, k: usize| -> Vec<f64> {
                            let mut out = mflat(d);
                            let mut ab = b.clone();
                            for _ in 0..k {
                                out.extend(mflat(&mul(c, &ab)));
                                ab = mul(a, &ab);
                            }
                            out
                        };
                        let k = 2 * ta.nrows().max(1);
                        rec["diff"] = json!({"markov": nrel(&markov(&ta, &tb, &tc, &td, k), &markov(&s2[0], &s2[1], &s2[2], &s2[3], k)), "order": [ta.nrows(), s2[0].nrows()]});
                    }
                },
                _ => rec["skip"] = json!("only SISO tf → ss has a tpt equivalent (tf.to_state_space)"),
            }
        }
        "control.design.place" => {
            if ok {
                let a = dm(&req["A"]).unwrap();
                let b = dm(&req["B"]).unwrap();
                let k = dm(&res["K"]).unwrap();
                let want = cvec(&json!({"dtype": "c128", "data": req["poles"]}));
                let want = if want.iter().all(|c| c.re.is_nan()) { cvec(&json!({"dtype": "f64", "data": req["poles"]})) } else { want };
                match (&a - &mul(&b, &k)).eigenvalues() {
                    Ok(ev) => {
                        rec["tpt"] = json!({"eig_A_minus_BK": cj(&ev), "note": "tpt has no pole placement; tpt's eigen-solver checks AEOI's K"});
                        rec["diff"] = json!({"vs_requested": match_c(&want, &ev), "vs_aeoi_achieved": match_c(&cvec(&res["achieved_poles"]), &ev)});
                    }
                    Err(e) => rec["tpt"] = terr(e),
                }
            } else {
                rec["skip"] = json!("aeoi error (no K to check)");
            }
        }
        _ => rec["skip"] = json!("no mapping"),
    }
    rec
}

// ------------------------------------------------------------------ corpus + random cases
fn corpus(repo: &Path) -> Vec<(String, String, Value, Option<Value>)> {
    let mut out = vec![];
    let mut files: Vec<PathBuf> = vec![];
    for op in std::fs::read_dir(repo.join("golden/python-control")).unwrap().flatten().filter(|e| e.path().is_dir()) {
        for ver in std::fs::read_dir(op.path()).unwrap().flatten() {
            for f in std::fs::read_dir(ver.path()).unwrap().flatten() {
                files.push(f.path());
            }
        }
    }
    files.sort();
    for f in files {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        out.push((f.strip_prefix(repo).unwrap().display().to_string(), v["operator"].as_str().unwrap().to_string(), v["request"].clone(), Some(v["expected"].clone())));
    }
    let mut ex: Vec<PathBuf> = std::fs::read_dir(repo.join("examples")).unwrap().flatten().flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten().map(|e| e.path())).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("request")).collect();
    ex.sort();
    for f in ex {
        let op = f.parent().unwrap().file_name().unwrap().to_string_lossy().to_string();
        out.push((f.strip_prefix(repo).unwrap().display().to_string(), op, serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap(), None));
    }
    out
}

struct Rng(u64);
impl Rng {
    fn u(&mut self) -> f64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn n(&mut self) -> f64 {
        let (a, b) = (self.u().max(1e-300), self.u());
        (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
    }
    fn mat(&mut self, r: usize, c: usize) -> DMat<f64> {
        DMat::from_fn(r, c, |_, _| self.n())
    }
    fn int(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.u() * ((hi - lo + 1) as f64)) as usize % (hi - lo + 1)
    }
}
fn dj(m: &DMat<f64>) -> Value {
    json!({"dtype": "f64", "shape": [m.nrows(), m.ncols()], "data": mflat(m)})
}
fn spectral_abscissa(a: &DMat<f64>) -> f64 {
    a.eigenvalues().unwrap().iter().map(|c| c.re).fold(f64::NEG_INFINITY, f64::max)
}
fn spectral_radius(a: &DMat<f64>) -> f64 {
    a.eigenvalues().unwrap().iter().map(|c| c.abs()).fold(0.0, f64::max)
}
fn spd(r: &mut Rng, n: usize, cond: f64) -> DMat<f64> {
    // Q diag(logspace(0, -log10 cond)) Qᵀ with a random orthogonal Q (from QR)
    let (qm, _) = r.mat(n, n).qr();
    let d = DMat::diagonal(&(0..n).map(|i| if n == 1 { 1.0 } else { cond.powf(-(i as f64) / ((n - 1) as f64)) }).collect::<Vec<_>>());
    let s = mul(&mul(&qm, &d), &t(&qm));
    // exact symmetry
    DMat::from_fn(n, n, |i, j| 0.5 * (s[(i, j)] + s[(j, i)]))
}

fn random_cases() -> Vec<(String, String, Value, Option<Value>)> {
    let mut out = vec![];
    let mut r = Rng(0x5eed_2026_1123);
    // ---- lyap continuous: margin to the imaginary axis 1, 1e-3, 1e-6; non-normal
    for (fam, delta, nonnormal) in [("lyap_c_well", 1.0, 0.0), ("lyap_c_near_axis_1e-3", 1e-3, 0.0), ("lyap_c_near_axis_1e-6", 1e-6, 0.0), ("lyap_c_nonnormal_triangular", 0.1, 1e3)] {
        for i in 0..12 {
            let n = r.int(2, 10);
            let mut a = r.mat(n, n);
            if nonnormal > 0.0 {
                a = DMat::from_fn(n, n, |p, q| if p < q { nonnormal * a[(p, q)] } else if p == q { -delta * (1.0 + r.u()) } else { 0.0 });
            } else {
                let sa = spectral_abscissa(&a);
                a = &a - &(&eye(n) * (sa + delta));
            }
            let qm = r.mat(n, n);
            let q = &mul(&qm, &t(&qm)) + &eye(n);
            out.push((format!("random/{fam}/{i}"), "control.math.lyap".into(), json!({"A": dj(&a), "Q": dj(&q), "type": "continuous"}), None));
        }
    }
    // ---- lyap continuous, non-normal but NOT triangular: A = U T Uᵀ (U random orthogonal), so neither
    //      solver gets the Schur form for free; off-diagonal scale 10, 100, 1000
    for (fam, scale) in [("lyap_c_nonnormal_rot_1e1", 10.0), ("lyap_c_nonnormal_rot_1e2", 100.0), ("lyap_c_nonnormal_rot_1e3", 1000.0)] {
        for i in 0..8 {
            let n = r.int(3, 8);
            let a0 = r.mat(n, n);
            let tm = DMat::from_fn(n, n, |p, q| if p < q { scale * a0[(p, q)] } else if p == q { -0.1 * (1.0 + p as f64 / n as f64) } else { 0.0 });
            let (u, _) = r.mat(n, n).qr();
            let a = mul(&mul(&u, &tm), &t(&u));
            let qm = r.mat(n, n);
            let q = &mul(&qm, &t(&qm)) + &eye(n);
            out.push((format!("random/{fam}/{i}"), "control.math.lyap".into(), json!({"A": dj(&a), "Q": dj(&q), "type": "continuous"}), None));
        }
    }
    // ---- lyap discrete: spectral radius 0.5, 0.999, 0.999999
    for (fam, rho) in [("lyap_d_well", 0.5), ("lyap_d_rho_0.999", 0.999), ("lyap_d_rho_1-1e-6", 1.0 - 1e-6)] {
        for i in 0..12 {
            let n = r.int(2, 10);
            let a0 = r.mat(n, n);
            let a = &a0 * (rho / spectral_radius(&a0));
            let qm = r.mat(n, n);
            let q = &mul(&qm, &t(&qm)) + &eye(n);
            out.push((format!("random/{fam}/{i}"), "control.math.lyap".into(), json!({"A": dj(&a), "Q": dj(&q), "type": "discrete"}), None));
        }
    }
    // ---- lyap on unstable / marginal A (error behaviour)
    for i in 0..6 {
        let n = r.int(2, 6);
        let a0 = r.mat(n, n);
        let sa = spectral_abscissa(&a0);
        let a = &a0 - &(&eye(n) * (sa - 0.5)); // abscissa +0.5
        let q = eye(n);
        out.push((format!("random/lyap_c_unstable/{i}"), "control.math.lyap".into(), json!({"A": dj(&a), "Q": dj(&q), "type": "continuous"}), None));
        let ad = &a0 * (1.2 / spectral_radius(&a0));
        out.push((format!("random/lyap_d_unstable/{i}"), "control.math.lyap".into(), json!({"A": dj(&ad), "Q": dj(&q), "type": "discrete"}), None));
    }
    // ---- lqr / dlqr
    for disc in [false, true] {
        let op = if disc { "control.design.dlqr" } else { "control.design.lqr" };
        let p = if disc { "dlqr" } else { "lqr" };
        for (fam, cond_r, qscale, bscale, ascale) in [
            ("well", 1.0, 1.0, 1.0, 1.0),
            ("condR_1e8", 1e8, 1.0, 1.0, 1.0),
            ("condR_1e12", 1e12, 1.0, 1.0, 1.0),
            ("smallQ_1e-8", 1.0, 1e-8, 1.0, 1.0),
            ("weakB_1e-4", 1.0, 1.0, 1e-4, 1.0),
            ("bigA_1e3", 1.0, 1.0, 1.0, 1e3),
        ] {
            for i in 0..10 {
                let n = r.int(2, 8);
                let m = r.int(if cond_r > 1.0 { 2 } else { 1 }, 3.min(n));
                let mut a = &r.mat(n, n) * ascale;
                if disc {
                    // spectral radius around 1.05 (slightly unstable open loop), scaled by ascale only for "bigA"
                    let rho = spectral_radius(&a);
                    a = &a * ((if ascale > 1.0 { 30.0 } else { 1.05 }) / rho);
                }
                let b = &r.mat(n, m) * bscale;
                let q = &spd(&mut r, n, 10.0) * qscale;
                let rr = spd(&mut r, m, cond_r);
                out.push((format!("random/{p}_{fam}/{i}"), op.into(), json!({"A": dj(&a), "B": dj(&b), "Q": dj(&q), "R": dj(&rr)}), None));
            }
        }
        // cross term
        for i in 0..6 {
            let n = r.int(2, 6);
            let m = r.int(1, 2);
            let a = r.mat(n, n);
            let a = if disc { &a * (0.95 / spectral_radius(&a)) } else { a };
            let b = r.mat(n, m);
            let rr = spd(&mut r, m, 3.0);
            let nn = &r.mat(n, m) * 0.1;
            let q = &spd(&mut r, n, 3.0) + &eye(n);
            out.push((format!("random/{p}_crossN/{i}"), op.into(), json!({"A": dj(&a), "B": dj(&b), "Q": dj(&q), "R": dj(&rr), "N": dj(&nn)}), None));
        }
        // not stabilizable: unstable mode without input authority
        for i in 0..5 {
            let n = r.int(2, 6);
            let mut a = DMat::from_fn(n, n, |p, q| if p == q { -1.0 - p as f64 } else { 0.0 });
            a[(0, 0)] = if disc { 1.5 } else { 0.7 };
            let mut b = r.mat(n, 1);
            b[(0, 0)] = 0.0;
            out.push((format!("random/{p}_unstabilizable/{i}"), op.into(), json!({"A": dj(&a), "B": dj(&b), "Q": dj(&eye(n)), "R": dj(&eye(1))}), None));
        }
        // undetectable on the unit circle / imaginary axis (Q = 0 on a marginal mode) → no stabilizing solution in python-control
        for i in 0..4 {
            let n = 2;
            let a = if disc { DMat::from_rows(&[&[1.0, 0.0], &[0.0, 0.5]]) } else { DMat::from_rows(&[&[0.0, 0.0], &[0.0, -1.0]]) };
            let b = DMat::from_rows(&[&[1.0], &[1.0 + i as f64]]);
            let q = DMat::from_rows(&[&[0.0, 0.0], &[0.0, 1.0]]);
            out.push((format!("random/{p}_marginal_undetectable/{i}"), op.into(), json!({"A": dj(&a), "B": dj(&b), "Q": dj(&q), "R": dj(&eye(1))}), None));
            let _ = n;
        }
    }
    // ---- minimal repros found by this cross-check (fixed inputs, not random)
    // (1) decoupled, benign CARE with cond(R) = 1e12: x₁ = 4.1421356…, x₂ = 9.99999e-7 in closed form
    let a = DMat::from_rows(&[&[-0.1, 0.0], &[0.0, -1.0]]);
    let b = DMat::from_rows(&[&[0.1, 0.0], &[0.0, 1.0]]);
    let rr = DMat::from_rows(&[&[1.0, 0.0], &[0.0, 1e-12]]);
    out.push(("repro/lqr_diag_condR_1e12".into(), "control.design.lqr".into(), json!({"A": dj(&a), "B": dj(&b), "Q": dj(&eye(2)), "R": dj(&rr)}), None));
    out.push(("repro/dlqr_diag_condR_1e12".into(), "control.design.dlqr".into(), json!({"A": dj(&DMat::from_rows(&[&[0.9, 0.0], &[0.0, 0.5]])), "B": dj(&b), "Q": dj(&eye(2)), "R": dj(&rr)}), None));
    // (2) rotation A (|λ| = 1), Q = 0: no stabilizing DARE solution exists
    let th: f64 = 0.3;
    let rot = DMat::from_rows(&[&[th.cos(), -th.sin()], &[th.sin(), th.cos()]]);
    out.push(("repro/dlqr_rotation_Q0".into(), "control.design.dlqr".into(), json!({"A": dj(&rot), "B": dj(&DMat::from_rows(&[&[0.0], &[1.0]])), "Q": dj(&DMat::zeros(2, 2)), "R": dj(&eye(1))}), None));
    // ---- c2d zoh / tustin with ts·‖A‖ from 1e-3 to 10
    for method in ["zoh", "tustin"] {
        for i in 0..15 {
            let n = r.int(1, 6);
            let m = r.int(1, 3);
            let p = r.int(1, 3);
            let a = r.mat(n, n);
            let na = a.norm_1().max(1e-3);
            let scale = [1e-3, 1e-1, 1.0, 3.0, 10.0][i % 5];
            let ts = scale / na;
            out.push((format!("random/c2d_{method}_tsA_{scale}/{i}"), "control.lti.c2d".into(),
                json!({"system": {"kind": "ss", "A": dj(&a), "B": dj(&r.mat(n, m)), "C": dj(&r.mat(p, n)), "D": dj(&r.mat(p, m)), "ts": 0.0, "time_unit": "s"}, "ts": ts, "method": method}), None));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repo = PathBuf::from(args.get(1).cloned().unwrap_or("../..".into()));
    let out_path = args.get(2).cloned().unwrap_or("/tmp/xcheck-tpt.jsonl".into());
    let reg = aeoi_control::registry();
    let host = aeoi_core::HostCtx::default();
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out_path).unwrap());
    let mut cases = corpus(&repo);
    let nc = cases.len();
    cases.extend(random_cases());
    for (i, (case, op, req, expected)) in cases.iter().enumerate() {
        let opx = aeoi_control::find(&reg, op).unwrap();
        let t0 = Instant::now();
        let env = opx.call(req, &host);
        let us = t0.elapsed().as_secs_f64() * 1e6;
        let mut rec = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_case(case, op, req, &env, expected.as_ref()))).unwrap_or_else(|_| json!({"case": case, "operator": op, "harness_panic": true, "aeoi": summary(&env)}));
        rec["aeoi"]["us"] = json!(us);
        rec["source"] = json!(if i < nc { "corpus" } else if case.starts_with("repro/") { "repro" } else { "random" });
        if i >= nc {
            rec["request"] = req.clone();
        }
        writeln!(f, "{}", rec).unwrap();
    }
    eprintln!("{} cases ({} corpus, {} random) → {}", cases.len(), nc, cases.len() - nc, out_path);
}
