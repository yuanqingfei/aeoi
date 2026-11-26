//! One-sided Jacobi SVD kernels (spec 10.23). linalg.decomp.svd is the operator built on these; other operators
//! needing singular values or vectors call them.
use crate::error::OpError;
use crate::kernels::dense::qr_factor;

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
pub fn jacobi(mut g: Vec<Vec<f64>>, m: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<Option<(Vec<Vec<f64>>, Vec<Vec<f64>>)>, OpError> {
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
        check()?;
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



/// Thin SVD of a row-major m×n matrix: singular values descending (stable), u_k (m) and v_k (n) as column lists,
/// p = min(m, n) triples; sign rule: the largest-modulus entry of u_k (first on ties) is positive.
/// linalg.decomp.svd returns exactly this; every other operator needing an SVD calls it (spec 11.5 RI-1).
pub struct Svd {
    pub s: Vec<f64>,
    pub u: Vec<Vec<f64>>,
    pub v: Vec<Vec<f64>>,
}

pub fn svd_thin(a: &[f64], m: usize, n: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<Option<Svd>, OpError> {
    let p = m.min(n);
    let tall = m >= n;
    let (rows, cols) = if tall { (m, n) } else { (n, m) };
    let g: Vec<Vec<f64>> = (0..cols).map(|k| (0..rows).map(|r| if tall { a[r * n + k] } else { a[k * n + r] }).collect()).collect();
    let Some((g, w)) = jacobi(g, rows, check)? else { return Ok(None) };
    let sig: Vec<f64> = g.iter().map(|c| nrm2(c)).collect();
    let mut order: Vec<usize> = (0..p).collect();
    order.sort_by(|&x, &y| sig[y].partial_cmp(&sig[x]).unwrap().then(x.cmp(&y)));
    let s: Vec<f64> = order.iter().map(|&k| sig[k]).collect();
    // Columns whose σ is at the numerical-zero level (≤ max(m, n)·ε·σ_max, v0.4.23) are not normalised: their
    // inner products may underflow (σ ≈ 1e-172 passed the rotation test with exact zeros), so g_k/σ_k need not be
    // orthogonal to the others. Their singular vectors come from the orthonormal completion, like σ = 0.
    let zero_level = rows.max(cols) as f64 * f64::EPSILON * s.first().copied().unwrap_or(0.0);
    let nz = s.iter().filter(|&&x| x > 0.0 && x > zero_level).count();
    let mut left: Vec<Vec<f64>> = order[..nz].iter().map(|&k| g[k].iter().map(|x| x / sig[k]).collect()).collect();
    if nz < p {
        left = complete(&left, rows)[..p].to_vec();
    }
    let right: Vec<Vec<f64>> = order.iter().map(|&k| w[k].clone()).collect();
    let (uu, vv) = if tall { (left, right) } else { (right, left) };
    let mut u = Vec::with_capacity(p);
    let mut v = Vec::with_capacity(p);
    for k in 0..p {
        let mut uk = uu[k].clone();
        let mut vk = vv[k].clone();
        if sign_fix(&mut uk) < 0.0 {
            for z in vk.iter_mut() {
                *z = -*z;
            }
        }
        u.push(uk);
        v.push(vk);
    }
    Ok(Some(Svd { s, u, v }))
}

/// Numerical rank decision shared by linalg.decomp.svd (SV-4), linalg.subspace.rank, linalg.subspace.basis and
/// linalg.matrix.pinv (spec 10.37 RK-2, RK-3). `tol` = the caller's tolerance, or MATLAB's default
/// max(m, n)·eps(s_max). rank = #{s_k > tol}. The rank is ambiguous when a singular value lies in the band
/// (tol/10, 100·tol]: above it a value is clearly signal, below it it is at the rounding-noise level of a
/// backward-stable SVD of an exactly rank-deficient matrix (≈ u·s_max ≪ tol/10 for the default tolerance).
pub struct RankDecision {
    pub tol: f64,
    pub supplied: bool,
    pub rank: usize,
    pub ambiguous: bool,
    pub smallest_kept: Option<f64>,
    pub largest_dropped: Option<f64>,
}

pub fn rank_decision(s: &[f64], m: usize, n: usize, tol: Option<f64>) -> RankDecision {
    let smax = s.first().copied().unwrap_or(0.0);
    let (tol, supplied) = match tol {
        Some(t) => (t, true),
        None => (m.max(n) as f64 * eps_of(smax), false),
    };
    let rank = s.iter().filter(|&&v| v > tol).count();
    let ambiguous = s.iter().any(|&v| v > tol / 10.0 && v <= 100.0 * tol);
    RankDecision { tol, supplied, rank, ambiguous, smallest_kept: if rank > 0 { Some(s[rank - 1]) } else { None }, largest_dropped: s.get(rank).copied() }
}
