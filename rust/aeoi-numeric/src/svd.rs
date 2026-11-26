//! linalg.decomp.svd — singular value decomposition A = U·diag(s)·Vᵀ of a real matrix (spec 10.23, v0.4.11).
//!
//! One-sided Jacobi (Hestenes) on the columns of A (of Aᵀ when m < n): cyclic row-by-row sweeps, rotate the
//! pair (p, q) while |g_pᵀg_q| > sqrt(m)·ε·‖g_p‖‖g_q‖, at most 60 sweeps. σ_k = ‖g_k‖₂, u_k = g_k/σ_k; zero
//! singular values get u_k from an orthonormal completion. Reordered by descending singular value (stable).
//! (nalgebra's bidiagonal QR SVD was tried first and lost a row of a row-graded matrix: reconstruction
//! residual 8.7e-10, caught by the `reconstructs` post-condition.) mode = values | thin (default, MATLAB svd(A, "econ")) | full (U, V completed to square
//! orthogonal matrices with the Householder QR of the thin factor; the first p columns stay the thin
//! vectors). Sign convention (author's decision 2026-11-25): the largest-modulus entry of u_k (first on
//! ties) is positive, v_k flips with it. V is returned, not Vᵀ (MATLAB convention). Numerical rank uses
//! MATLAB's tolerance max(m, n)·eps(s_max).
use crate::linsolve::real_rows;
use aeoi_core::kernels::svd::*;
use aeoi_core::error::{OpError, Warning};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Svd, "linalg.decomp.svd");

const U: f64 = f64::EPSILON / 2.0;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Values,
    Thin,
    Full,
}

pub struct In {
    m: usize,
    n: usize,
    a: Vec<f64>,
    mode: Mode,
}

fn mat_json(cols: &[Vec<f64>], rows: usize) -> Value {
    let c = cols.len();
    let mut data = vec![0.0; rows * c];
    for (j, col) in cols.iter().enumerate() {
        for i in 0..rows {
            data[i * c + j] = col[i];
        }
    }
    json!({"dtype": "f64", "shape": [rows, c], "data": data.iter().map(|v| num(*v)).collect::<Vec<_>>()})
}

impl Operator for Svd {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let (m, n, a, one_d) = real_rows(&args["A"], "/A", host)?;
        if one_d && m > 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 是一维的（长度 {m}）；要求 [m, n]。"), "行向量写 shape [1, n]，列向量写 [m, 1]。").path("/A/shape"));
        }
        if m == 0 || n == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "A 是空矩阵。", "给出非空的 A。").path("/A/shape"));
        }
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        if m > lim || n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("A 是 {m}×{n}，超过单次上限 {lim}。"), "大型矩阵的部分奇异值需要另设算子（svds）。").path("/A/shape"));
        }
        let mode = match args.get("mode").and_then(|v| v.as_str()).unwrap_or("thin") {
            "values" => Mode::Values,
            "full" => Mode::Full,
            _ => Mode::Thin,
        };
        Ok((In { m, n, a, mode }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let p = m.min(n);
        let afro = nrm2(&i.a);
        let aeoi_core::kernels::svd::Svd { s, u: ucols, v: vcols } = svd_thin(&i.a, m, n, &|| ctx.check())?.ok_or_else(|| OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("Jacobi SVD 在 {MAX_SWEEPS} 轮扫描内未收敛。"), "检查 A 的数值尺度（极大/极小元素）；可先缩放后重试。").path("/A"))?;
        // reconstruction residual ‖A − U·diag(s)·Vᵀ‖_F / ‖A‖_F
        let mut rsq = 0.0f64;
        for r in 0..m {
            for c in 0..n {
                let a_hat: f64 = (0..p).map(|k| ucols[k][r] * s[k] * vcols[k][c]).sum();
                let d = i.a[r * n + c] - a_hat;
                rsq += d * d;
            }
        }
        let recon = rsq.sqrt() / afro.max(f64::MIN_POSITIVE);
        let (uout, vout) = if i.mode == Mode::Full { (complete(&ucols, m), complete(&vcols, n)) } else { (ucols.clone(), vcols.clone()) };
        let (ou, ov) = (orth_err(&uout), orth_err(&vout));
        let threshold = 100.0 * m.max(n) as f64 * U;
        let smax = s.first().copied().unwrap_or(0.0);
        let smin = s.last().copied().unwrap_or(0.0);
        let rank_tol = m.max(n) as f64 * eps_of(smax);
        let rank = s.iter().filter(|&&v| v > rank_tol).count();
        let cond2 = if smin > 0.0 { smax / smin } else { f64::INFINITY };
        let finite = s.iter().all(|v| v.is_finite()) && uout.iter().chain(vout.iter()).flatten().all(|v| v.is_finite());
        let e_abs = recon * afro + (ou + ov) * smax;
        let gaps: Vec<f64> = (0..p).map(|k| (0..p).filter(|&j| j != k).map(|j| (s[j] - s[k]).abs()).fold(f64::INFINITY, f64::min)).collect();
        let mode = match i.mode {
            Mode::Values => "values",
            Mode::Thin => "thin",
            Mode::Full => "full",
        };
        let result = json!({
            "s": {"dtype": "f64", "shape": [p], "data": s.iter().map(|v| num(*v)).collect::<Vec<_>>()},
            "U": if i.mode == Mode::Values { Value::Null } else { mat_json(&uout, m) },
            "V": if i.mode == Mode::Values { Value::Null } else { mat_json(&vout, n) },
            "mode": mode,
            "rank": rank,
            "rank_tol": num(rank_tol),
            "cond2": num(cond2),
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"backward_error": num(recon), "threshold": num(threshold), "definition": "‖A − U·diag(s)·Vᵀ‖_F / ‖A‖_F（p 个奇异三元组）", "orthonormality_u": num(ou), "orthonormality_v": num(ov)},
            "condition": {"cond2": num(cond2), "note": "奇异值本身是良态的：|Δs_k| ≤ ‖ΔA‖₂（Weyl）"},
            "error_estimate": {"value_abs": num(e_abs), "vector_angle": gaps.iter().map(|g| num(if g.is_finite() && *g > 0.0 { (e_abs / g).min(std::f64::consts::FRAC_PI_2) } else if g.is_finite() { std::f64::consts::FRAC_PI_2 } else { 0.0 })).collect::<Vec<_>>(),
                "basis": "|Δs_k| ≲ ‖A‖_F·残差 + s_max·(正交性误差)（Weyl；小奇异值只有绝对精度）；奇异向量夹角 ≲ 该界 / gap_k（Wedin；重奇异值时 gap = 0，向量只确定到子空间）"},
            "checks": {"reconstructs": recon <= threshold, "orthonormal_u": ou <= threshold, "orthonormal_v": ov <= threshold, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"checked_factors": if i.mode == Mode::Values { "U、V 在内部计算并检查，不返回" } else { "返回的 U、V" }}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn call(args: Value) -> Value {
        Svd::new().call(&args, &HostCtx::default())
    }
    #[test]
    fn diag_and_full() {
        let a = json!({"dtype": "f64", "shape": [3, 2], "data": [3., 0., 0., -4., 0., 0.]});
        let e = call(json!({"A": a, "mode": "full"}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["s"]["data"], json!([4.0, 3.0]));
        assert_eq!(e["result"]["U"]["shape"], json!([3, 3]));
        assert_eq!(e["result"]["rank"], 2);
    }
    #[test]
    fn rank_deficient() {
        let a = json!({"dtype": "f64", "shape": [3, 3], "data": [1., 2., 3., 4., 5., 6., 7., 8., 9.]});
        let e = call(json!({"A": a}));
        assert_eq!(e["result"]["rank"], 2, "{e}");
        assert_eq!(e["status"], "ok");
    }
    #[test]
    fn wide_values() {
        let a = json!({"dtype": "f64", "shape": [2, 4], "data": [1., 2., 3., 4., 5., 6., 7., 8.5]});
        let e = call(json!({"A": a, "mode": "values"}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["U"], Value::Null);
    }
}
