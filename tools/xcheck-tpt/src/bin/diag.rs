//! Diagnostics for individual disagreements: reads one record's request from xcheck-tpt.jsonl (by case id)
//! and prints tpt internals (Schur backward error, eigenvalues). usage: diag <jsonl> <case>
use serde_json::Value;
use tpt_ctrl_core::DMat;
fn dm(v: &Value) -> DMat<f64> {
    let sh: Vec<usize> = v["shape"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
    let d: Vec<f64> = v["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    DMat::from_fn(sh[0], sh[1], |i, j| d[i * sh[1] + j])
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    for l in std::fs::read_to_string(&a[1]).unwrap().lines() {
        let r: Value = serde_json::from_str(l).unwrap();
        if r["case"] != a[2].as_str() {
            continue;
        }
        let req = &r["request"];
        let am = dm(&req["A"]);
        let s = am.transpose().schur().unwrap();
        let back = &(&(&s.q * &s.t) * &s.q.transpose()) - &am.transpose();
        let orth = &(&s.q.transpose() * &s.q) - &DMat::identity(am.nrows());
        println!("n={} ‖A‖_F={:.3e} schur backward ‖QTQᵀ−Aᵀ‖_F/‖A‖_F={:.3e} ‖QᵀQ−I‖_F={:.3e}", am.nrows(), am.norm_fro(), back.norm_fro() / am.norm_fro(), orth.norm_fro());
        let sub: f64 = (1..am.nrows()).map(|i| s.t[(i, i - 1)].abs()).fold(0.0, f64::max);
        println!("max |T[i,i-1]| = {sub:.3e}");
        let x = tpt_ctrl_core::lyapunov::solve_lyapunov(&am.transpose(), &dm(&req["Q"])).unwrap();
        println!("tpt X[0][0..3] = {:?}", &x.row(0)[..3.min(am.nrows())]);
    }
}
