//! Eigen-decomposition kernels (spec 10.22): cyclic Jacobi for symmetric matrices; real Schur → complex
//! Schur (rsf2csf) → triangular back-substitution with left vectors and condition numbers for general ones.
//! linalg.decomp.eig is the operator built on these; other operators needing eigen-decompositions call them.
use crate::error::OpError;
use crate::lti::{schur_robust, C64};
use crate::ExecCtx;
use nalgebra::DMatrix;

/// Normalise: ‖v‖₂ = 1, largest-modulus entry (first on ties) real positive.
pub fn normalise(v: &mut [C64]) {
    let nrm = v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
    if nrm == 0.0 || !nrm.is_finite() {
        return;
    }
    let (mut idx, mut best) = (0usize, -1.0f64);
    for (i, z) in v.iter().enumerate() {
        let a = z.norm();
        if a > best {
            best = a;
            idx = i;
        }
    }
    let ph = v[idx].conj() / v[idx].norm();
    for z in v.iter_mut() {
        *z = *z * ph / nrm;
    }
    v[idx] = C64::new(v[idx].norm(), 0.0); // exactly real
}

pub fn normalise_real(v: &mut [f64]) {
    let nrm = v.iter().map(|z| z * z).sum::<f64>().sqrt();
    if nrm == 0.0 {
        return;
    }
    let (mut idx, mut best) = (0usize, -1.0f64);
    for (i, z) in v.iter().enumerate() {
        if z.abs() > best {
            best = z.abs();
            idx = i;
        }
    }
    let s = if v[idx] < 0.0 { -1.0 } else { 1.0 } / nrm;
    for z in v.iter_mut() {
        *z *= s;
    }
}

/// Eigenvalues of the real 2×2 block [[a, b], [c, d]]: (first, second), first with Im ≥ 0.
pub fn block_eigs(a: f64, b: f64, c: f64, d: f64) -> (C64, C64) {
    let p = 0.5 * (a - d);
    let disc = p * p + b * c;
    let tr = 0.5 * (a + d);
    if disc < 0.0 {
        let s = (-disc).sqrt();
        (C64::new(tr, s), C64::new(tr, -s))
    } else {
        let s = disc.sqrt();
        (C64::new(tr + s, 0.0), C64::new(tr - s, 0.0))
    }
}

pub struct General {
    pub values: Vec<C64>,
    pub vectors: Vec<Vec<C64>>, // in A coordinates, normalised
    pub cond: Vec<f64>,
}

pub fn general(a: &DMatrix<f64>) -> Option<General> {
    let n = a.nrows();
    let (q, t) = schur_robust(a)?;
    // eigenvalues from the real quasi-triangular T (exact conjugate pairs)
    let mut lam = vec![C64::new(0.0, 0.0); n];
    let mut is_block_first = vec![false; n];
    let mut k = 0;
    while k < n {
        if k + 1 < n && t[(k + 1, k)] != 0.0 {
            let (l1, l2) = block_eigs(t[(k, k)], t[(k, k + 1)], t[(k + 1, k)], t[(k + 1, k + 1)]);
            lam[k] = l1;
            lam[k + 1] = l2;
            is_block_first[k] = true;
            k += 2;
        } else {
            lam[k] = C64::new(t[(k, k)], 0.0);
            k += 1;
        }
    }
    // rsf2csf
    let mut tc: DMatrix<C64> = t.map(|x| C64::new(x, 0.0));
    let mut uc: DMatrix<C64> = q.map(|x| C64::new(x, 0.0));
    for m in (1..n).rev() {
        if t[(m, m - 1)] != 0.0 && is_block_first[m - 1] {
            let mu = lam[m - 1] - tc[(m, m)];
            let sub = tc[(m, m - 1)];
            let r = mu.norm().hypot(sub.norm());
            let c = mu / r;
            let s = sub / r;
            // G = [c̄ s; −s c]; T(k, m−1:n) = G·T(k, m−1:n); T(0:m, k) = T(0:m, k)·Gᴴ; U(:, k) = U(:, k)·Gᴴ
            for j in m - 1..n {
                let (x1, x2) = (tc[(m - 1, j)], tc[(m, j)]);
                tc[(m - 1, j)] = c.conj() * x1 + s * x2;
                tc[(m, j)] = -s * x1 + c * x2;
            }
            for i in 0..=m {
                let (x1, x2) = (tc[(i, m - 1)], tc[(i, m)]);
                tc[(i, m - 1)] = x1 * c + x2 * s.conj();
                tc[(i, m)] = -x1 * s.conj() + x2 * c.conj();
            }
            for i in 0..n {
                let (x1, x2) = (uc[(i, m - 1)], uc[(i, m)]);
                uc[(i, m - 1)] = x1 * c + x2 * s.conj();
                uc[(i, m)] = -x1 * s.conj() + x2 * c.conj();
            }
            tc[(m, m - 1)] = C64::new(0.0, 0.0);
        }
    }
    let ulp = f64::EPSILON;
    let smlnum = f64::MIN_POSITIVE * (n as f64 / ulp);
    const BIG: f64 = 1e150;
    let mut vectors: Vec<Vec<C64>> = vec![Vec::new(); n];
    let mut cond = vec![1.0; n];
    for k in 0..n {
        if lam[k].im < 0.0 && k > 0 && is_block_first[k - 1] {
            vectors[k] = vectors[k - 1].iter().map(|z| z.conj()).collect();
            cond[k] = cond[k - 1];
            continue;
        }
        let tkk = tc[(k, k)];
        let smin = (ulp * (tkk.re.abs() + tkk.im.abs())).max(smlnum);
        // right: T[0..=k, 0..=k] x = λ x, x_k = 1
        let mut x = vec![C64::new(0.0, 0.0); k + 1];
        x[k] = C64::new(1.0, 0.0);
        for j in (0..k).rev() {
            let mut s = C64::new(0.0, 0.0);
            for l in j + 1..=k {
                s -= tc[(j, l)] * x[l];
            }
            let mut d = tc[(j, j)] - tkk;
            if d.norm() < smin {
                d = C64::new(smin, 0.0);
            }
            x[j] = s / d;
            if x[j].norm() > BIG {
                for z in x.iter_mut() {
                    *z /= BIG;
                }
            }
        }
        // left: w T = λ w on columns k.., w_k = 1
        let mut w = vec![C64::new(0.0, 0.0); n];
        w[k] = C64::new(1.0, 0.0);
        for j in k + 1..n {
            let mut s = C64::new(0.0, 0.0);
            for l in k..j {
                s -= w[l] * tc[(l, j)];
            }
            let mut d = tc[(j, j)] - tkk;
            if d.norm() < smin {
                d = C64::new(smin, 0.0);
            }
            w[j] = s / d;
            if w[j].norm() > BIG {
                for z in w.iter_mut() {
                    *z /= BIG;
                }
            }
        }
        let xn = x.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        let wn = w.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        cond[k] = xn * wn / (w[k] * x[k]).norm();
        let mut v: Vec<C64> = (0..n).map(|i| (0..=k).fold(C64::new(0.0, 0.0), |s, l| s + uc[(i, l)] * x[l])).collect();
        normalise(&mut v);
        if lam[k].im == 0.0 {
            for z in v.iter_mut() {
                z.im = 0.0;
            }
            normalise(&mut v);
        }
        vectors[k] = v;
    }
    Some(General { values: lam, vectors, cond })
}

pub const MAX_SWEEPS: usize = 60;

/// Cyclic Jacobi for a symmetric matrix (row-major n×n): (eigenvalues, eigenvectors as columns-lists).
pub fn sym_jacobi(a: &[f64], n: usize, ctx: &ExecCtx) -> Result<Option<(Vec<f64>, Vec<Vec<f64>>)>, OpError> {
    let mut a = a.to_vec();
    let mut v: Vec<Vec<f64>> = (0..n).map(|k| (0..n).map(|i| if i == k { 1.0 } else { 0.0 }).collect()).collect();
    for _ in 0..MAX_SWEEPS {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                let (app, aqq) = (a[p * n + p], a[q * n + q]);
                if apq == 0.0 || apq.abs() <= f64::EPSILON * (app.abs() * aqq.abs()).sqrt() {
                    continue;
                }
                rotated = true;
                let tau = (aqq - app) / (2.0 * apq);
                let t = (if tau >= 0.0 { 1.0 } else { -1.0 }) / (tau.abs() + 1.0f64.hypot(tau));
                let c = 1.0 / 1.0f64.hypot(t);
                let s = c * t;
                for k in 0..n {
                    if k == p || k == q {
                        continue;
                    }
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    let np = c * akp - s * akq;
                    let nq = s * akp + c * akq;
                    a[k * n + p] = np;
                    a[p * n + k] = np;
                    a[k * n + q] = nq;
                    a[q * n + k] = nq;
                }
                a[p * n + p] = app - t * apq;
                a[q * n + q] = aqq + t * apq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for i in 0..n {
                    let (x, y) = (v[p][i], v[q][i]);
                    v[p][i] = c * x - s * y;
                    v[q][i] = s * x + c * y;
                }
            }
        }
        ctx.check()?;
        if !rotated {
            return Ok(Some(((0..n).map(|k| a[k * n + k]).collect(), v)));
        }
    }
    Ok(None)
}

