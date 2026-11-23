//! Analytic (hand-derived) checks for the batch-2 operators — independent of any reference software.
mod common;
use aeoi_core::HostCtx;
use common::*;
use serde_json::{json, Value};

fn call(op: &str, req: Value) -> Value {
    let reg = aeoi_control::registry();
    aeoi_control::find(&reg, op).unwrap().call(&req, &HostCtx::default())
}
fn m(rows: usize, cols: usize, d: &[f64]) -> Value {
    json!({"dtype": "f64", "shape": [rows, cols], "data": d})
}
fn ok(env: &Value) -> &Value {
    assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{}", env["error"]);
    &env["result"]
}
fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

#[test]
fn convert_round_trip_and_mimo_common_denominator() {
    let tf = json!({"kind": "tf", "num": [1.0, 3.0], "den": [1.0, 1.2, 4.0], "ts": 0.0, "time_unit": "s"});
    let ss = ok(&call("control.lti.convert", json!({"system": tf, "to": "ss"})))["system"].clone();
    // controllable canonical form (scipy tf2ss convention)
    assert_eq!(fvec(&ss["A"]["data"]), vec![-1.2, -4.0, 1.0, 0.0]);
    assert_eq!(fvec(&ss["C"]["data"]), vec![1.0, 3.0]);
    let back = ok(&call("control.lti.convert", json!({"system": ss, "to": "tf"})))["system"].clone();
    for (a, b) in fvec(&back["num"]).iter().zip([1.0, 3.0]) {
        assert!(close(*a, b, 1e-14));
    }
    let zpk = ok(&call("control.lti.convert", json!({"system": tf, "to": "zpk"})))["system"].clone();
    assert!(close(f(&zpk["z"][0][0]), -3.0, 1e-14) && f(&zpk["k"]) == 1.0);
    // MIMO ss → tf: every element shares det(sI − A)
    let s2 = json!({"kind": "ss", "A": m(2, 2, &[-1.0, 0.0, 0.0, -2.0]), "B": m(2, 2, &[1.0, 0.0, 0.0, 1.0]), "C": m(2, 2, &[1.0, 1.0, 0.0, 1.0]), "D": m(2, 2, &[0.0; 4]), "ts": 0.0, "time_unit": "s"});
    let t2 = ok(&call("control.lti.convert", json!({"system": s2, "to": "tf"})))["system"].clone();
    for i in 0..2 {
        for j in 0..2 {
            let d = fvec(&t2["den"][i][j]);
            assert!(close(d[1], 3.0, 1e-14) && close(d[2], 2.0, 1e-14));
        }
    }
}

#[test]
fn nonsquare_invariant_zero() {
    // G = [(s+3)/((s+1)(s+2)); 2(s+3)/((s+1)(s+2))] (2×1): common transmission zero at s = −3
    let sys = json!({"kind": "ss", "A": m(2, 2, &[-1.0, 0.0, 0.0, -2.0]), "B": m(2, 1, &[1.0, 1.0]), "C": m(2, 2, &[2.0, -1.0, 4.0, -2.0]), "D": m(2, 1, &[0.0, 0.0]), "ts": 0.0, "time_unit": "s"});
    let r = ok(&call("control.lti.poles_zeros", json!({"system": sys}))).clone();
    let z = &r["zeros"]["data"];
    assert_eq!(z.as_array().unwrap().len(), 1, "{z}");
    assert!(close(f(&z[0][0]), -3.0, 1e-12) && f(&z[0][1]).abs() < 1e-12);
    // dcgain = [3/2, 3]
    let dc = fvec(&r["dcgain"]["value"]["data"]);
    assert!(close(dc[0], 1.5, 1e-14) && close(dc[1], 3.0, 1e-14));
}

#[test]
fn ctrb_rank_deficient_by_hand() {
    // A = diag(−1, −2), B = [1, 0]ᵀ: mode −2 uncontrollable ⇒ rank 1
    let sys = json!({"kind": "ss", "A": m(2, 2, &[-1.0, 0.0, 0.0, -2.0]), "B": m(2, 1, &[1.0, 0.0]), "C": m(1, 2, &[1.0, 1.0]), "D": m(1, 1, &[0.0]), "ts": 0.0, "time_unit": "s"});
    let r = ok(&call("control.analysis.ctrb_obsv", json!({"system": sys, "gramians": true}))).clone();
    assert_eq!(r["ctrb"]["rank"], 1);
    assert_eq!(r["obsv"]["rank"], 2);
    assert_eq!(r["minimal"], false);
    // Wc = diag(1/2, 0)
    let wc = fvec(&r["gramians"]["controllability"]["data"]);
    assert!(close(wc[0], 0.5, 1e-15) && wc[1].abs() < 1e-15 && wc[3].abs() < 1e-15);
}

#[test]
fn margin_hand_checks_including_discrete_nyquist_endpoint() {
    let r = ok(&call("control.analysis.margin", json!({"system": {"kind": "tf", "num": [4.0], "den": [1.0, 3.0, 2.0, 0.0], "ts": 0.0, "time_unit": "s"}}))).clone();
    assert!(close(f(&r["gain_margin"]["value"]), 1.5, 1e-13));
    assert!(close(f(&r["phase_crossover_frequency"]["value"]), 2f64.sqrt(), 1e-13));
    // G(z) = 0.5 / z: |1 + 0.5 e^{−jθ}| is minimal at θ = π (Nyquist) with value 0.5
    let d = ok(&call("control.analysis.margin", json!({"system": {"kind": "tf", "num": [0.5], "den": [1.0, 0.0], "ts": 0.1, "time_unit": "s"}}))).clone();
    assert!(close(f(&d["stability_margin"]["value"]), 0.5, 1e-14), "{}", d["stability_margin"]);
    assert!(close(f(&d["stability_margin_frequency"]["value"]), std::f64::consts::PI / 0.1, 1e-14));
    // phase −180° occurs exactly at the Nyquist frequency (|G| = 0.5). Convention (python-control 0.10.2, spec
    // §10.7): phase crossings are searched on θ ∈ [0, π), so a crossing exactly at θ = π is not reported ⇒ GM = Inf.
    assert!(f(&d["gain_margin"]["value"]).is_infinite(), "{}", d["gain_margin"]);
}

#[test]
fn c2d_tustin_scalar_closed_form() {
    // 1/(s+1), T = 0.2: A_d = (1−T/2)/(1+T/2), B_d = T/(1+T/2), C_d = 1/(1+T/2), D_d = (T/2)/(1+T/2)
    let t = 0.2;
    let sys = json!({"kind": "ss", "A": m(1, 1, &[-1.0]), "B": m(1, 1, &[1.0]), "C": m(1, 1, &[1.0]), "D": m(1, 1, &[0.0]), "ts": 0.0, "time_unit": "s"});
    let r = ok(&call("control.lti.c2d", json!({"system": sys, "ts": t, "method": "tustin"})))["system"].clone();
    let g = |k: &str| fvec(&r[k]["data"])[0];
    let den = 1.0 + t / 2.0;
    assert!(close(g("A"), (1.0 - t / 2.0) / den, 1e-15));
    assert!(close(g("B"), t / den, 1e-15));
    assert!(close(g("C"), 1.0 / den, 1e-15));
    assert!(close(g("D"), (t / 2.0) / den, 1e-15), "D_d = {}", g("D"));
    // zoh: A_d = e^{−T}, B_d = 1 − e^{−T}
    let z = ok(&call("control.lti.c2d", json!({"system": sys, "ts": t, "method": "zoh"})))["system"].clone();
    assert!(close(fvec(&z["A"]["data"])[0], (-t).exp(), 1e-15) && close(fvec(&z["B"]["data"])[0], 1.0 - (-t).exp(), 1e-15));
}

#[test]
fn impulse_first_order() {
    let r = ok(&call("control.lti.impulse", json!({"system": {"kind": "tf", "num": [1.0], "den": [1.0, 1.0], "ts": 0.0, "time_unit": "s"},
                                                    "time": {"mode": "uniform", "t_final": 5.0, "dt": 0.01, "unit": "s"}}))).clone();
    let t = fvec(&r["t"]["value"]["data"]);
    let y = fvec(&r["y"]["value"]["data"]);
    let e = t.iter().zip(&y).map(|(t, y)| (y - (-t).exp()).abs()).fold(0.0, f64::max);
    assert!(e < 1e-13, "max |y − e^(−t)| = {e}");
}

#[test]
fn lyap_scalar_closed_forms() {
    let c = ok(&call("control.math.lyap", json!({"A": m(1, 1, &[-2.0]), "Q": m(1, 1, &[3.0]), "type": "continuous"}))).clone();
    assert!(close(fvec(&c["X"]["data"])[0], 0.75, 1e-15)); // −q/(2a)
    let d = ok(&call("control.math.lyap", json!({"A": m(1, 1, &[0.5]), "Q": m(1, 1, &[3.0]), "type": "discrete"}))).clone();
    assert!(close(fvec(&d["X"]["data"])[0], 4.0, 1e-15)); // q/(1 − a²)
}

#[test]
fn place_square_b_is_perfectly_conditioned() {
    let r = call("control.design.place", json!({"A": m(2, 2, &[0.0, 1.0, 2.0, -1.0]), "B": m(2, 2, &[1.0, 0.0, 0.0, 1.0]), "poles": [[-1.0, 2.0], [-1.0, -2.0]]}));
    let res = ok(&r).clone();
    assert!(close(f(&r["trust"]["condition"]["cond_X_eigvec"]), 1.0, 1e-12));
    let k = fvec(&res["K"]["data"]);
    // closed loop A − K must have trace −2 and determinant 5
    let acl = [0.0 - k[0], 1.0 - k[1], 2.0 - k[2], -1.0 - k[3]];
    assert!(close(acl[0] + acl[3], -2.0, 1e-13) && close(acl[0] * acl[3] - acl[1] * acl[2], 5.0, 1e-13));
}

#[test]
fn dlqr_scalar_golden_ratio() {
    // a = b = q = r = 1: s² = s + 1 ⇒ s = φ, K = s/(1+s) = 1/φ
    let r = ok(&call("control.design.dlqr", json!({"A": m(1, 1, &[1.0]), "B": m(1, 1, &[1.0]), "Q": m(1, 1, &[1.0]), "R": m(1, 1, &[1.0])}))).clone();
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    assert!(close(fvec(&r["S"]["data"])[0], phi, 1e-14));
    assert!(close(fvec(&r["K"]["data"])[0], 1.0 / phi, 1e-14));
}

/// Regression for the false E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG found by the tpt-control cross-check
/// (reports/parity-tpt-control.md): decoupled scalar CAREs with cond(R) = 1e12. Exact: sᵢ = qᵢ/(√(aᵢ² + bᵢ²qᵢ/rᵢ) − aᵢ).
#[test]
fn lqr_condr_1e12_decoupled_closed_form() {
    let env = call("control.design.lqr", json!({"A": m(2, 2, &[-0.1, 0.0, 0.0, -1.0]), "B": m(2, 2, &[0.1, 0.0, 0.0, 1.0]), "Q": m(2, 2, &[1.0, 0.0, 0.0, 1.0]), "R": m(2, 2, &[1.0, 0.0, 0.0, 1e-12])}));
    let r = ok(&env);
    let s = fvec(&r["S"]["data"]);
    let s1 = 1.0 / ((0.01f64 + 0.01).sqrt() + 0.1);
    let s2 = 1.0 / ((1.0f64 + 1e12).sqrt() + 1.0);
    assert!(((s[0] - s1) / s1).abs() <= 1e-13, "S11 = {} vs {s1}", s[0]);
    assert!(((s[3] - s2) / s2).abs() <= 1e-12, "S22 = {} vs {s2}", s[3]);
    assert!(s[1].abs() <= 1e-15 && s[2].abs() <= 1e-15, "off-diagonal {:?}", &s[1..3]);
    let k = fvec(&r["K"]["data"]);
    assert!(((k[3] - s2 / 1e-12) / (s2 / 1e-12)).abs() <= 1e-12, "K22 = {}", k[3]);
    assert!((s1 - 4.142135623730952).abs() < 1e-14 && (s2 - 9.99999e-7).abs() < 1e-12);
}

/// Genuinely ill-posed CAREs must still report E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG at any scale.
#[test]
fn lqr_imag_axis_unobservable_modes_still_error() {
    for (a, b, q) in [
        (vec![0.0, 1e3, -1e3, 0.0], vec![0.0, 1.0], vec![0.0; 4]),
        (vec![0.0, 1e-3, -1e-3, 0.0], vec![0.0, 1.0], vec![0.0; 4]),
        (vec![0.0, 0.0, 0.0, -1.0], vec![1.0, 1.0], vec![0.0, 0.0, 0.0, 1.0]),
    ] {
        let env = call("control.design.lqr", json!({"A": m(2, 2, &a), "B": m(2, 1, &b), "Q": m(2, 2, &q), "R": m(1, 1, &[1.0])}));
        assert_eq!(env["error"]["code"], "E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG", "A = {a:?}: {}", env);
    }
}

/// Discrete Lyapunov with ρ(A) → 1: A = [[p, −q], [q, p]], p = 1 − 2⁻²⁰, q = 2⁻¹⁰, so AAᵀ = (1 − 2⁻²⁰ + 2⁻⁴⁰)·I exactly
/// and X = I / (2⁻²⁰ − 2⁻⁴⁰) (sep ≈ 9.5e-7). Refinement with a compensated residual must reach ~u.
#[test]
fn dlyap_rho_near_one_refined() {
    let (p, q) = (1.0 - 2f64.powi(-20), 2f64.powi(-10));
    let env = call("control.math.lyap", json!({"A": m(2, 2, &[p, -q, q, p]), "Q": m(2, 2, &[1.0, 0.0, 0.0, 1.0]), "type": "discrete"}));
    let r = ok(&env);
    let x = fvec(&r["X"]["data"]);
    let exact = 1.0 / (2f64.powi(-20) - 2f64.powi(-40));
    for (i, v) in x.iter().enumerate() {
        let want = if i == 0 || i == 3 { exact } else { 0.0 };
        assert!((v - want).abs() <= 1e-13 * exact, "X[{i}] = {v} vs {want} (rel {:.2e})", (v - want).abs() / exact);
    }
    assert_eq!(env["trust"]["convergence"]["applicable"], true);
}
