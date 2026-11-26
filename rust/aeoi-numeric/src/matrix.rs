//! linalg.matrix.inv and linalg.matrix.det (spec 10.35, 10.36; v0.4.22).
//!
//! Both are built on the LU kernel of aeoi-core::kernels::dense (RI-1), the same factorisation as
//! linalg.system.solve: partial pivoting with the LAPACK dgetf2 pivot order.
//!
//! inv: column j of X solves A·x_j = e_j, with the refinement of LS-2 (kernels::dense::lu_solve_refined).
//!      κ₁ = ‖A‖₁·‖X‖₁ from the returned X. An ill-conditioned A still gets a result (author's decision,
//!      2026-11-26): W.NUMERICAL.ILL_CONDITIONED when rcond < ε, and the post-condition
//!      `forward_error_below_one` (κ₁·η < 1) guards /X.
//! det: sign(P)·Π u_ii, accumulated as mantissa × 2^exponent so the product never overflows mid-way;
//!      log|det| and the sign are always meaningful (author's decision). Relative error estimate
//!      ε = n·u·est(‖A⁻¹‖₁)·‖|L||U|‖₁ + n·u; ε ≥ 1 means det is indistinguishable from 0
//!      (author's decision): W.NUMERICAL.DET_NOT_SIGNIFICANT and the post-condition `significant` fails.
use crate::linsolve::real_rows;
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::kernels::dense::*;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use aeoi_libm::libm;
use serde_json::{json, Value};

operator_struct!(Inv, "linalg.matrix.inv");
operator_struct!(Det, "linalg.matrix.det");

const U: f64 = f64::EPSILON / 2.0;
const MAX_REFINE: usize = 3;

pub struct SqIn {
    n: usize,
    a: Vec<f64>, // row-major n×n
    refine: bool,
}

/// A real square [n, n] matrix at `/A` (complex → DTYPE, 1-D or non-square → SHAPE).
fn square_real(args: &Value, host: &HostCtx, op: &str, max_dim: usize) -> Result<(usize, Vec<f64>), OpError> {
    if args["A"].get("dtype").and_then(|d| d.as_str()) == Some("c128") {
        return Err(OpError::new("E.VALIDATION.DTYPE", format!("A 是复数（c128）；{op} 0.4.0 只支持实数矩阵。"), "复数矩阵以后作为另一个方法提供；目前可以把 A = B + iC 写成 2n 维实矩阵 [B −C; C B] 再处理。").path("/A/dtype"));
    }
    let (m, n, a, one_d) = real_rows(&args["A"], "/A", host)?;
    if one_d || m != n || n == 0 {
        let shape = if one_d { json!([m]) } else { json!([m, n]) };
        return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 的 shape 是 {shape}；{op} 需要非空方阵 [n, n]。"), "检查 A 的录入（row-major，shape [n, n]）。非方阵的“逆”是伪逆（linalg.matrix.pinv）或最小二乘解（linalg.system.solve）。")
            .path("/A/shape")
            .detail("shape", shape));
    }
    if n > max_dim {
        return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("A 是 {n}×{n}，超过单次上限 {max_dim}（稠密 O(n³)）。"), "大型矩阵一般不需要显式的逆或行列式：解方程组用 linalg.system.solve。").path("/A/shape"));
    }
    Ok((n, a))
}

fn singular_err(p: usize) -> OpError {
    OpError::new("E.NUMERICAL.SINGULAR", format!("A 奇异：LU 分解第 {p} 步的主元恰好为 0（部分选主元后该列在对角线以下全为 0），逆矩阵不存在。"), "检查 A 的录入（重复行/列、漏填）；需要“广义逆”时用 linalg.matrix.pinv；只是要解方程组时用 linalg.system.solve。")
        .path("/A")
        .detail("zero_pivot_step", json!(p))
        .remedy(ask_user("singular_model", "A 奇异：是录入错误，还是模型本身缺少约束？"))
}

fn dense(n: usize, v: &[f64]) -> Value {
    json!({"dtype": "f64", "shape": [n, n], "data": v.iter().map(|x| num(*x)).collect::<Vec<_>>()})
}

// ============================================================================================== inv
impl Operator for Inv {
    type Input = SqIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(SqIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (n, a) = square_real(args, host, "linalg.matrix.inv", lim)?;
        let refine = args.get("refinement").and_then(|v| v.as_str()).unwrap_or("auto") == "auto";
        Ok((SqIn { n, a, refine }, Vec::new()))
    }
    fn estimate(&self, i: &SqIn) -> Option<Value> {
        Some(json!({"n": i.n, "flops": 2.0 * (i.n as f64).powi(3) * if i.refine { 4.0 } else { 1.0 }}))
    }
    fn execute(&self, i: SqIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        let lu = lu_factor(&i.a, n);
        if let Some(p) = lu.zero_pivot {
            return Err(singular_err(p));
        }
        let anorm1 = norm1(&i.a, n, n);
        let (x_all, eta_worst, steps_max) = inverse_refined(&lu, &i.a, n, if i.refine { MAX_REFINE } else { 0 }, &|| ctx.check())?;
        let xnorm1 = norm1(&x_all, n, n);
        let kappa1 = anorm1 * xnorm1;
        let rcond = 1.0 / kappa1;
        let fwd = kappa1 * eta_worst;
        let threshold = 10.0 * n as f64 * U;
        let finite = x_all.iter().all(|v| v.is_finite());
        let mut warnings = Vec::new();
        if !(rcond >= f64::EPSILON) {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("A 接近奇异：rcond = {rcond:.3e} < ε（κ₁ = {kappa1:.3e}）。逆矩阵的相对误差可能达到 κ₁·η ≈ {fwd:.1e}，结果可能没有有效数字。"))
                .path("/A")
                .details(json!({"rcond": num(rcond), "kappa1": num(kappa1)})));
        }
        let result = json!({"X": dense(n, &x_all), "kappa1": num(kappa1), "rcond": num(rcond), "method": "lu_partial_pivoting", "refinement_steps": steps_max});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"backward_error": num(eta_worst), "threshold": num(threshold), "definition": "max_j ‖e_j − A·x_j‖∞ / (‖A‖∞‖x_j‖∞ + 1)（逐列范数后向误差，Rigal–Gaches）；残差用补偿算术（Dot2）"},
            "condition": {"kappa1": num(kappa1), "rcond": num(rcond), "of": "A", "basis": "‖A‖₁·‖X‖₁，X 为返回的逆（不是估计；X 不准时它也不准，但量级可靠）"},
            "error_estimate": {"X_rel_forward_1": num(fwd), "basis": "一阶界 ‖ΔX‖/‖X‖ ≲ κ₁·η"},
            "checks": {"backward_stable": eta_worst <= threshold, "forward_error_below_one": fwd < 1.0, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"refinement": if i.refine { "auto" } else { "none" }, "norm1_A": num(anorm1), "norm1_X": num(xnorm1)}
        });
        Ok(Output { result, trust, warnings })
    }
}

// ============================================================================================== det
/// Split |x| (finite, non-zero) into m·2^e with m ∈ [0.5, 1), exactly (bit manipulation; subnormals handled).
fn frexp(x: f64) -> (f64, i64) {
    let mut x = x.abs();
    let mut adj = 0i64;
    if x < f64::MIN_POSITIVE {
        x *= 2f64.powi(64); // exact
        adj = -64;
    }
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64 - 1022;
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, e + adj)
}

/// m·2^e as f64 (±Inf on overflow, gradual underflow to 0), exactly when representable.
fn ldexp(m: f64, e: i64) -> f64 {
    let mut v = m;
    let mut e = e;
    while e > 1000 {
        v *= 2f64.powi(1000);
        e -= 1000;
        if v.is_infinite() {
            return v;
        }
    }
    while e < -1000 {
        v *= 2f64.powi(-1000);
        e += 1000;
        if v == 0.0 {
            return v;
        }
    }
    v * 2f64.powi(e as i32)
}

impl Operator for Det {
    type Input = SqIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(SqIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (n, a) = square_real(args, host, "linalg.matrix.det", lim)?;
        Ok((SqIn { n, a, refine: false }, Vec::new()))
    }
    fn estimate(&self, i: &SqIn) -> Option<Value> {
        Some(json!({"n": i.n, "flops": 2.0 / 3.0 * (i.n as f64).powi(3)}))
    }
    fn execute(&self, i: SqIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        let lu = lu_factor(&i.a, n);
        ctx.check()?;
        let mut warnings = Vec::new();
        let perm_sign = lu.perm_sign();
        let nu = n as f64 * U;
        let (det, sign, logabs, est, inv1, lu1);
        if let Some(p) = lu.zero_pivot {
            det = 0.0;
            sign = 0i64;
            logabs = f64::NEG_INFINITY;
            est = f64::INFINITY;
            inv1 = f64::INFINITY;
            lu1 = f64::NAN;
            warnings.push(Warning::new("W.NUMERICAL.DET_NOT_SIGNIFICANT", format!("LU 分解第 {p} 步的主元恰好为 0：算出的 det = 0。A 的数据可能是精确奇异的，也可能只是舍入使然；det 的值与符号不可依赖。"))
                .path("/A")
                .details(json!({"zero_pivot_step": p, "relative_error_estimate": "Inf"})));
        } else {
            // scaled product: mantissa in [0.5, 1) and a binary exponent (one rounding per factor)
            let (mut m, mut e) = (1.0f64, 0i64);
            let mut s = perm_sign;
            for k in 0..n {
                let ukk = lu.entry(k, k);
                if ukk < 0.0 {
                    s = -s;
                }
                let (mk, ek) = frexp(ukk);
                let (mm, em) = frexp(m * mk);
                m = mm;
                e += ek + em;
            }
            sign = s;
            det = s as f64 * ldexp(m, e);
            logabs = libm::log(m) + e as f64 * std::f64::consts::LN_2;
            // ‖|L|·|U|‖₁ = max_j Σ_k v_k |u_kj|, v_k = 1 + Σ_{i>k} |l_ik| (column sums of |L|)
            let v: Vec<f64> = (0..n).map(|k| 1.0 + (k + 1..n).map(|ii| lu.entry(ii, k).abs()).sum::<f64>()).collect();
            lu1 = (0..n).map(|j| (0..=j).map(|k| v[k] * lu.entry(k, j).abs()).sum::<f64>()).fold(0.0, f64::max);
            inv1 = inv_norm1_estimate(n, &|x| lu.solve(x), &|x| lu.solve_t(x));
            let e1 = nu * inv1 * lu1 + nu;
            est = if e1.is_finite() { e1 } else { f64::INFINITY };
            if est >= 1.0 {
                warnings.push(Warning::new("W.NUMERICAL.DET_NOT_SIGNIFICANT", format!("det 的相对误差估计为 {est:.2e} ≥ 1：算出的 det = {det:.3e} 与 0 不可区分（矩阵在舍入意义下奇异）；det、sign、log_abs_det 不可依赖。"))
                    .path("/A")
                    .details(json!({"relative_error_estimate": num(est), "det": num(det)})));
            }
            if det == 0.0 || det.is_infinite() || det.abs() < f64::MIN_POSITIVE {
                warnings.push(Warning::new("W.NUMERICAL.DET_OUT_OF_RANGE", format!("|det| = e^{logabs:.6} 超出 f64 的正规数范围，det 写成 {}；请用 log_abs_det 与 sign。", if det.is_infinite() { "±Inf" } else { "下溢后的值" }))
                    .path("/A")
                    .details(json!({"log_abs_det": num(logabs), "sign": sign})));
            }
        }
        let zero = est >= 1.0;
        let result = json!({"det": num(det), "sign": sign, "log_abs_det": num(logabs), "relative_error_estimate": num(est), "zero_within_error": zero, "method": "lu_partial_pivoting"});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"applicable": false},
            "condition": {"norm1_inv_A_estimate": num(inv1), "norm1_abs_L_abs_U": num(lu1), "estimator": "Hager–Higham 1-范数估计（下界）"},
            "error_estimate": {"det_rel": num(est), "basis": "启发式一阶界 n·u·est(‖A⁻¹‖₁)·‖|L||U|‖₁ + n·u：LU 的后向误差 |E| ≲ n·u·|L||U| 经 det(A + E) ≈ det(A)(1 + tr(A⁻¹E)) 传到 det，再加乘积的 n 次舍入；不是严格上界"},
            "checks": {"significant": !zero},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn mat(n: usize, d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [n, n], "data": d})
    }
    fn data(v: &Value) -> Vec<f64> {
        v["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
    }

    #[test]
    fn frexp_ldexp_roundtrip() {
        for x in [1.0, 0.75, 3.0, 1e300, 1e-300, 5e-324, -2.5, f64::MAX] {
            let (m, e) = frexp(x);
            assert!((0.5..1.0).contains(&m), "{x}: {m}");
            assert_eq!(ldexp(m, e), x.abs(), "{x}");
        }
        assert_eq!(ldexp(0.5, 2000), f64::INFINITY);
        assert_eq!(ldexp(0.5, -2000), 0.0);
    }

    #[test]
    fn inv_small_exact() {
        // A = [[4, 7], [2, 6]], A⁻¹ = [[0.6, −0.7], [−0.2, 0.4]]
        let env = Inv::new().call(&json!({"A": mat(2, &[4.0, 7.0, 2.0, 6.0])}), &HostCtx::default());
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        let x = data(&env["result"]["X"]);
        for (g, w) in x.iter().zip([0.6, -0.7, -0.2, 0.4]) {
            assert!((g - w).abs() <= 1e-16, "{x:?}");
        }
        assert_eq!(env["trust"]["checks"]["backward_stable"], true);
        assert_eq!(env["trust"]["checks"]["forward_error_below_one"], true);
    }

    #[test]
    fn inv_errors_and_hilbert() {
        let op = Inv::new();
        let e = op.call(&json!({"A": mat(2, &[1.0, 2.0, 2.0, 4.0])}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.NUMERICAL.SINGULAR");
        let e = op.call(&json!({"A": {"dtype": "f64", "shape": [2, 3], "data": vec![1.0; 6]}}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
        let n = 13;
        let h: Vec<f64> = (0..n * n).map(|t| 1.0 / ((t / n + t % n + 1) as f64)).collect();
        let env = op.call(&json!({"A": mat(n, &h)}), &HostCtx::default());
        assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED"));
        assert_eq!(env["trust"]["checks"]["forward_error_below_one"], false);
    }

    #[test]
    fn det_values() {
        let op = Det::new();
        // det [[2, 1, 1], [4, −6, 0], [−2, 7, 2]] = −16
        let env = op.call(&json!({"A": mat(3, &[2.0, 1.0, 1.0, 4.0, -6.0, 0.0, -2.0, 7.0, 2.0])}), &HostCtx::default());
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        let d = env["result"]["det"].as_f64().unwrap();
        assert!((d + 16.0).abs() <= 1e-13, "{d}");
        assert_eq!(env["result"]["sign"], -1);
        assert!((env["result"]["log_abs_det"].as_f64().unwrap() - 16f64.ln()).abs() < 1e-14);
        // singular after rounding: magic(4)
        let m4 = [16.0, 2.0, 3.0, 13.0, 5.0, 11.0, 10.0, 8.0, 9.0, 7.0, 6.0, 12.0, 4.0, 14.0, 15.0, 1.0];
        let env = op.call(&json!({"A": mat(4, &m4)}), &HostCtx::default());
        assert_eq!(env["result"]["zero_within_error"], true, "{}", env["result"]);
        assert_eq!(env["trust"]["checks"]["significant"], false);
        // overflow: 10·I₄₀₀ → det = 1e400, log = 400 ln 10
        let n = 400;
        let a: Vec<f64> = (0..n * n).map(|t| if t / n == t % n { 10.0 } else { 0.0 }).collect();
        let env = op.call(&json!({"A": mat(n, &a)}), &HostCtx::default());
        assert_eq!(env["result"]["det"], "Inf");
        assert!((env["result"]["log_abs_det"].as_f64().unwrap() - 400.0 * 10f64.ln()).abs() < 1e-11);
        assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.DET_OUT_OF_RANGE"));
        assert_eq!(env["result"]["zero_within_error"], false);
        // exact zero pivot
        let env = op.call(&json!({"A": mat(2, &[1.0, 2.0, 2.0, 4.0])}), &HostCtx::default());
        assert_eq!(env["result"]["det"].as_f64(), Some(0.0));
        assert_eq!(env["result"]["sign"], 0);
        assert_eq!(env["result"]["log_abs_det"], "-Inf");
    }
}
