//! One-sided Jacobi SVD kernels (spec 10.23). linalg.decomp.svd is the operator built on these; other operators
//! needing singular values or vectors call them.
use crate::error::OpError;
use crate::kernels::dense::qr_factor;
use crate::ExecCtx;

pub const MAX_SWEEPS: usize = 60;

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
pub fn complete(cols: &[Vec<f64>], m: usize) -> Vec<Vec<f64>> {
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
pub fn sign_fix(v: &mut [f64]) -> f64 {
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
pub fn nrm2(v: &[f64]) -> f64 {
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
pub fn jacobi(mut g: Vec<Vec<f64>>, m: usize, ctx: &ExecCtx) -> Result<Option<(Vec<Vec<f64>>, Vec<Vec<f64>>)>, OpError> {
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

pub fn orth_err(cols: &[Vec<f64>]) -> f64 {
    let mut e = 0.0f64;
    for p in 0..cols.len() {
        for q in 0..=p {
            let d: f64 = cols[p].iter().zip(&cols[q]).map(|(a, b)| a * b).sum::<f64>() - if p == q { 1.0 } else { 0.0 };
            e = e.max(d.abs());
        }
    }
    e
}

