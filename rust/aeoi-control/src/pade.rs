//! control.lti.pade — diagonal Padé approximant of a pure delay (spec 10.18, A-1).
//!
//! e^{-θ s} ≈ P(-θ s) / P(θ s), with P the degree-N numerator of the [N/N] Padé table of the
//! exponential. Coefficients follow Golub & Van Loan, Matrix Computations, 3rd ed., Algorithm
//! 11.3.1 (the recurrence for the Padé numerator of exp), scaled so the leading denominator
//! coefficient is 1. The delay error is reported separately from rounding error.
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::jcs::hash_value;
use aeoi_core::lti::C64;
use aeoi_core::poly::{polyval, strip};
use aeoi_core::sysalg::lti_json;
use aeoi_core::{ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use serde_json::{json, Value};

operator_struct!(Pade, "control.lti.pade");

pub struct In {
    theta: f64,
    order: usize,
    lineage: Value,
}

/// Descending coefficients of the diagonal approximant, leading denominator coefficient = 1.
///
/// Equivalent to Golub & Van Loan's recurrence, but each step is already divided by the
/// leading coefficient so intermediates stay near the final magnitude (the unnormalized
/// product overflows the f64 significand for N ≳ 10).
pub fn pade_tf(theta: f64, n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut den = vec![0.0; n + 1];
    den[0] = 1.0;
    for j in 0..n {
        den[j + 1] = den[j] * ((n + j + 1) as f64) * ((n - j) as f64) / (theta * (j + 1) as f64);
    }
    let num: Vec<f64> = (0..=n).map(|j| { let s = if (n - j) % 2 == 0 { 1.0 } else { -1.0 }; s * den[j] }).collect();
    (strip(&num), strip(&den))
}

/// Taylor coefficient of s^{2N+1} in e^{-θ s} − R(s), and the max relative residual on orders 0..=2N.
fn taylor_gap(num: &[f64], den: &[f64], theta: f64) -> (f64, f64) {
    let n = den.len() - 1;
    let na: Vec<f64> = num.iter().rev().cloned().collect();
    let da: Vec<f64> = den.iter().rev().cloned().collect();
    let m = 2 * n + 2;
    let mut r = vec![0.0; m];
    let d0 = da[0];
    for k in 0..m {
        let mut s = if k < na.len() { na[k] } else { 0.0 };
        for j in 1..=k.min(da.len() - 1) {
            s -= da[j] * r[k - j];
        }
        r[k] = s / d0;
    }
    let mut pk: f64 = 1.0;
    let mut max_rel: f64 = 0.0;
    let mut next: f64 = 0.0;
    for k in 0..m {
        if k > 0 {
            pk *= -theta / k as f64;
        }
        let ek = pk - r[k];
        if k <= 2 * n {
            let scale = pk.abs().max(r[k].abs()).max(1e-30_f64);
            max_rel = max_rel.max(ek.abs() / scale);
        } else {
            next = ek;
        }
    }
    (next, max_rel)
}

fn certificate(num: &[f64], den: &[f64], theta: f64) -> (f64, Vec<f64>) {
    let mut ws = Vec::with_capacity(32);
        let mut max_e: f64 = 0.0;
    for k in 0..32 {
        let w = theta.recip() * 10f64.powf(-2.0 + 4.0 * k as f64 / 31.0);
        ws.push(w);
        let s = C64::new(0.0, w);
        let rv = polyval(num, s) / polyval(den, s);
        let target = C64::new(0.0, -w * theta).exp();
        let err: f64 = (rv - target).norm();
        max_e = max_e.max(err);
    }
    (max_e, ws)
}

impl Operator for Pade {
    type Input = In;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let theta = args["delay"].as_f64().unwrap();
        let order = args["order"].as_u64().unwrap() as usize;
        if !(1..=20).contains(&order) {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("Padé 阶次 N = {order} 不在 1..=20。"), "阶次是建模选择，没有安全默认值；向用户询问，不要假设 N = 1。")
                .path("/order")
                .remedy(ask_user("order", "向用户询问 Padé 阶次 N（正整数，建议不超过 10）")));
        }
        if !(theta.is_finite() && theta > 0.0) {
            return Err(OpError::new("E.VALIDATION.RANGE", "时滞 delay 必须是有限正数（秒）。", "delay 没有默认值。用户没给、题目也推不出来时先询问。").path("/delay").remedy(ask_user("delay", "向用户询问时滞 θ（秒）")));
        }
        Ok((In { theta, order, lineage: args.get("lineage").cloned().unwrap_or(Value::Null) }, Vec::new()))
    }
    fn estimate(&self, i: &In) -> Option<Value> {
        Some(json!({"order": i.order}))
    }
    fn execute(&self, i: In, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let (num, den) = pade_tf(i.theta, i.order);
        let (next, matched_rel) = taylor_gap(&num, &den, i.theta);
        let (cert, grid) = certificate(&num, &den, i.theta);
        let matched = 2 * i.order;
        // rounding of the recurrence: each of N steps is a product, a priori ~ N·u·max|coeff|
        let coeff_scale = num.iter().chain(&den).map(|x| x.abs()).fold(0.0, f64::max).max(1.0);
        let coeff_abs = (i.order as f64) * f64::EPSILON * coeff_scale;
        let sys = lti_json(&aeoi_core::lti::Lti::Tf {
            num: vec![vec![num.clone()]],
            den: vec![vec![den.clone()]],
            ts: 0.0,
            siso: true,
        });
        let approximation = json!({
            "target": "exp(-delay*s)",
            "method": "diagonal_pade",
            "order": i.order,
            "delay": aeoi_core::num(i.theta),
            "matched_order": matched,
            "next_coefficient": aeoi_core::num(next),
            "next_power": matched + 1,
            "taylor_residual_rel": aeoi_core::num(matched_rel),
            "certificate": {
                "norm": "max_abs_on_grid",
                "value": aeoi_core::num(cert),
                "grid_unit": "rad/s",
                "grid": grid.iter().copied().map(aeoi_core::num).collect::<Vec<_>>(),
                "grid_definition": "ω_k = (1/θ)·10^(-2 + 4k/31), k = 0..31"
            },
            "bound": Value::Null,
            "bound_kind": "none",
            "note": "next_coefficient 是 e^{-θs}−R(s) 在 s=0 的 s^{2N+1} 系数（级数除法）。certificate 是虚轴网格上的计算值，不是严格上界。二者都是逼近误差，不是系数的舍入误差。"
        });
        let result = json!({
            "system": sys,
            "realization": "pade_coefficients",
            "minimal": Value::Null,
            "proper": true,
            "approximation": approximation,
            "composition": {
                "operation": "pade",
                "operands": [],
                "parameters_sha256": hash_value(&json!({"delay": i.theta, "order": i.order})),
                "lineage": i.lineage,
                "lineage_checked": false
            }
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"taylor_matched_rel": aeoi_core::num(matched_rel)},
            "condition": {"coefficient_scale": aeoi_core::num(coeff_scale)},
            "error_estimate": {
                "coefficient_abs": aeoi_core::num(coeff_abs),
                "basis": "系数：递推 N 步，先验界 N·ε·max|coeff|。这不是对 e^{-θs} 的逼近误差。",
                "approximation_abs": aeoi_core::num(cert),
                "approximation_basis": "虚轴 32 点网格上 |e^{-jωθ}−R(jω)| 的最大值（certificate），不是舍入界"
            },
            "checks": {"taylor_matched": matched_rel <= 1e-8, "dc_is_one": (num.last().unwrap() - den.last().unwrap()).abs() <= 1e-12 * coeff_scale, "all_finite": num.iter().chain(&den).all(|x| x.is_finite())},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "approximation": approximation,
            "derived": {}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orders_match_closed_form() {
        // N=1: (1 − θ s/2) / (1 + θ s/2), scaled to leading 1 → [-1, 2/θ] / [1, 2/θ] for the monic-leading form.
        let (n, d) = pade_tf(2.0, 1);
        assert!((n[0] + 1.0).abs() < 1e-15 && (n[1] - 1.0).abs() < 1e-15, "{n:?}");
        assert!((d[0] - 1.0).abs() < 1e-15 && (d[1] - 1.0).abs() < 1e-15, "{d:?}");
        let (n, d) = pade_tf(2.0, 2);
        // ct/hand: [1, -3, 3] / [1, 3, 3]
        for (a, b) in n.iter().zip([1.0, -3.0, 3.0]) {
            assert!((a - b).abs() < 1e-12, "{n:?}");
        }
        for (a, b) in d.iter().zip([1.0, 3.0, 3.0]) {
            assert!((a - b).abs() < 1e-12, "{d:?}");
        }
        let (next, rel) = taylor_gap(&n, &d, 2.0);
        assert!(rel < 1e-12, "{rel}");
        assert!(next.is_finite());
        let (n, d) = pade_tf(1.0, 8);
        let (_next, rel) = taylor_gap(&n, &d, 1.0);
        assert!(rel < 1e-8, "N=8 taylor residual {rel}");
    }
}
