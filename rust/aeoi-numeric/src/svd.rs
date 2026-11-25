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
use crate::linsolve::{qr_factor, real_rows};
use aeoi_core::error::{OpError, Warning};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Svd, "linalg.decomp.svd");

const U: f64 = f64::EPSILON / 2.0;
const MAX_SWEEPS: usize = 60;

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

/// MATLAB eps(x): distance from |x| to the next larger double.
pub fn eps_of(x: f64) -> f64 {
    let x = x.abs();
    if x == 0.0 || !x.is_finite() {
        return f64::MIN_POSITIVE * f64::EPSILON;
    }
    let next = f64::from_bits(x.to_bits() + 1);
    next - x
}

/// Complete the orthonormal columns `cols` (each length m) to an orthonormal basis of R^m.
fn complete(cols: &[Vec<f64>], m: usize) -> Vec<Vec<f64>> {
    let p = cols.len();
    if p >= m {
        return cols.to_vec();
    }
    let mut a = vec![0.0; m * p];
    for (j, c) in cols.iter().enumerate() {
        for i in 0..m {
            a[i * p + j] = c[i];
        }
    }
    let qr = qr_factor(&a, m, p);
    let mut out = cols.to_vec();
    for j in p..m {
        let mut e = vec![0.0; m];
        e[j] = 1.0;
        let mut q = qr.q_apply(&e);
        sign_fix(&mut q);
        out.push(q);
    }
    out
}

/// Largest-modulus entry (first on ties) positive; returns the sign applied.
fn sign_fix(v: &mut [f64]) -> f64 {
    let (mut idx, mut best) = (0usize, -1.0f64);
    for (i, z) in v.iter().enumerate() {
        if z.abs() > best {
            best = z.abs();
            idx = i;
        }
    }
    if v.is_empty() || v[idx] >= 0.0 {
        return 1.0;
    }
    for z in v.iter_mut() {
        *z = -*z;
    }
    -1.0
}

/// Overflow-safe 2-norm (LAPACK dnrm2 scaling).
fn nrm2(v: &[f64]) -> f64 {
    let (mut scale, mut ssq) = (0.0f64, 1.0f64);
    for &x in v {
        if x != 0.0 {
            let a = x.abs();
            if scale < a {
                ssq = 1.0 + ssq * (scale / a) * (scale / a);
                scale = a;
            } else {
                ssq += (a / scale) * (a / scale);
            }
        }
    }
    scale * ssq.sqrt()
}

/// One-sided Jacobi on the columns g_0..g_{n−1} (each of length m, m ≥ n). Returns (G rotated, V) or None
/// when the sweep limit is reached.
fn jacobi(mut g: Vec<Vec<f64>>, m: usize, ctx: &ExecCtx) -> Result<Option<(Vec<Vec<f64>>, Vec<Vec<f64>>)>, OpError> {
    let n = g.len();
    let mut v: Vec<Vec<f64>> = (0..n).map(|k| (0..n).map(|i| if i == k { 1.0 } else { 0.0 }).collect()).collect();
    let tol = (m as f64).sqrt() * f64::EPSILON;
    for _sweep in 0..MAX_SWEEPS {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let (gp, gq) = (&g[p], &g[q]);
                let alpha: f64 = gp.iter().map(|x| x * x).sum();
                let beta: f64 = gq.iter().map(|x| x * x).sum();
                let gamma: f64 = gp.iter().zip(gq).map(|(a, b)| a * b).sum();
                if alpha == 0.0 || beta == 0.0 || gamma.abs() <= tol * alpha.sqrt() * beta.sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = (if zeta >= 0.0 { 1.0 } else { -1.0 }) / (zeta.abs() + 1.0f64.hypot(zeta));
                let c = 1.0 / 1.0f64.hypot(t);
                let s = c * t;
                for i in 0..m {
                    let (a, b) = (g[p][i], g[q][i]);
                    g[p][i] = c * a - s * b;
                    g[q][i] = s * a + c * b;
                }
                for i in 0..n {
                    let (a, b) = (v[p][i], v[q][i]);
                    v[p][i] = c * a - s * b;
                    v[q][i] = s * a + c * b;
                }
            }
        }
        ctx.check()?;
        if !rotated {
            return Ok(Some((g, v)));
        }
    }
    Ok(None)
}

fn orth_err(cols: &[Vec<f64>]) -> f64 {
    let mut e = 0.0f64;
    for p in 0..cols.len() {
        for q in 0..=p {
            let d: f64 = cols[p].iter().zip(&cols[q]).map(|(a, b)| a * b).sum::<f64>() - if p == q { 1.0 } else { 0.0 };
            e = e.max(d.abs());
        }
    }
    e
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
        // columns of A (m ≥ n) or of Aᵀ (m < n)
        let tall = m >= n;
        let (rows, cols) = if tall { (m, n) } else { (n, m) };
        let g: Vec<Vec<f64>> = (0..cols).map(|k| (0..rows).map(|r| if tall { i.a[r * n + k] } else { i.a[k * n + r] }).collect()).collect();
        let (g, w) = jacobi(g, rows, ctx)?.ok_or_else(|| OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("Jacobi SVD 在 {MAX_SWEEPS} 轮扫描内未收敛。"), "检查 A 的数值尺度（极大/极小元素）；可先缩放后重试。").path("/A"))?;
        let sig: Vec<f64> = g.iter().map(|c| nrm2(c)).collect();
        let mut order: Vec<usize> = (0..p).collect();
        order.sort_by(|&x, &y| sig[y].partial_cmp(&sig[x]).unwrap().then(x.cmp(&y)));
        let s: Vec<f64> = order.iter().map(|&k| sig[k]).collect();
        // left vectors of the transformed problem: g_k/σ_k; zero σ completed afterwards
        let nz = s.iter().filter(|&&x| x > 0.0).count();
        let mut left: Vec<Vec<f64>> = order[..nz].iter().map(|&k| g[k].iter().map(|x| x / sig[k]).collect()).collect();
        if nz < p {
            left = complete(&left, rows)[..p].to_vec();
        }
        let right: Vec<Vec<f64>> = order.iter().map(|&k| w[k].clone()).collect();
        let (uu, vv) = if tall { (left, right) } else { (right, left) };
        let mut ucols: Vec<Vec<f64>> = Vec::with_capacity(p);
        let mut vcols: Vec<Vec<f64>> = Vec::with_capacity(p);
        for k in 0..p {
            let mut u = uu[k].clone();
            let mut v = vv[k].clone();
            if sign_fix(&mut u) < 0.0 {
                for z in v.iter_mut() {
                    *z = -*z;
                }
            }
            ucols.push(u);
            vcols.push(v);
        }
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
