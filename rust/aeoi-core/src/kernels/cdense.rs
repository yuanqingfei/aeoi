//! Complex dense kernels: LU with partial pivoting (LAPACK zgetf2 order: pivot by |re| + |im|, first maximal)
//! and singular values by one-sided Jacobi with complex rotations. Used by every operator that solves a
//! complex linear system (bode, frequency-response identities, zeros) or needs complex singular values
//! (normal rank, complex condition numbers) — spec 11.5 RI-1.
use crate::error::OpError;
use crate::lti::C64;

fn cabs1(z: C64) -> f64 {
    z.re.abs() + z.im.abs()
}

pub struct CLu {
    n: usize,
    lu: Vec<C64>,
    piv: Vec<usize>,
    pub zero_pivot: Option<usize>,
}

pub fn clu_factor(a: &[C64], n: usize) -> CLu {
    let mut lu = a.to_vec();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut zero_pivot = None;
    for k in 0..n {
        let mut p = k;
        let mut best = cabs1(lu[k * n + k]);
        for i in k + 1..n {
            let v = cabs1(lu[i * n + k]);
            if v > best {
                best = v;
                p = i;
            }
        }
        if p != k {
            for j in 0..n {
                lu.swap(k * n + j, p * n + j);
            }
            perm.swap(k, p);
        }
        let pivot = lu[k * n + k];
        if pivot.re == 0.0 && pivot.im == 0.0 {
            zero_pivot.get_or_insert(k);
            continue;
        }
        for i in k + 1..n {
            let l = lu[i * n + k] / pivot;
            lu[i * n + k] = l;
            if l.re != 0.0 || l.im != 0.0 {
                for j in k + 1..n {
                    let t = lu[k * n + j];
                    lu[i * n + j] -= l * t;
                }
            }
        }
    }
    CLu { n, lu, piv: perm, zero_pivot }
}

impl CLu {
    pub fn solve(&self, b: &[C64]) -> Vec<C64> {
        let n = self.n;
        let mut y: Vec<C64> = self.piv.iter().map(|&p| b[p]).collect();
        for i in 0..n {
            let mut s = y[i];
            for j in 0..i {
                s -= self.lu[i * n + j] * y[j];
            }
            y[i] = s;
        }
        for i in (0..n).rev() {
            let mut s = y[i];
            for j in i + 1..n {
                s -= self.lu[i * n + j] * y[j];
            }
            y[i] = s / self.lu[i * n + i];
        }
        y
    }
}

/// Singular values (descending) of a row-major complex m×n matrix by one-sided Jacobi on the columns (of the
/// conjugate transpose when m < n): each pair is first phase-aligned so that g_pᴴ·g_q is real, then rotated
/// like the real kernel. None if 60 sweeps do not converge.
pub fn csingular_values(a: &[C64], m: usize, n: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<Option<Vec<f64>>, OpError> {
    let tall = m >= n;
    let (rows, cols) = if tall { (m, n) } else { (n, m) };
    let mut g: Vec<Vec<C64>> = (0..cols).map(|k| (0..rows).map(|r| if tall { a[r * n + k] } else { a[k * n + r].conj() }).collect()).collect();
    let tol = (rows as f64).sqrt() * f64::EPSILON;
    let mut converged = false;
    for _ in 0..60 {
        let mut rotated = false;
        for p in 0..cols {
            for q in p + 1..cols {
                let alpha: f64 = g[p].iter().map(|z| z.norm_sqr()).sum();
                let beta: f64 = g[q].iter().map(|z| z.norm_sqr()).sum();
                let gamma = g[p].iter().zip(&g[q]).fold(C64::new(0.0, 0.0), |s, (x, y)| s + x.conj() * y);
                let gabs = gamma.norm();
                if alpha == 0.0 || beta == 0.0 || gabs <= tol * alpha.sqrt() * beta.sqrt() {
                    continue;
                }
                rotated = true;
                let ph = gamma / gabs; // e^{iφ}
                let zeta = (beta - alpha) / (2.0 * gabs);
                let t = (if zeta >= 0.0 { 1.0 } else { -1.0 }) / (zeta.abs() + 1.0f64.hypot(zeta));
                let c = 1.0 / 1.0f64.hypot(t);
                let s = c * t;
                for i in 0..rows {
                    let (x, y) = (g[p][i], g[q][i] * ph.conj()); // w = g_q·e^{−iφ}: g_pᴴw real
                    g[p][i] = x * c - y * s;
                    g[q][i] = (x * s + y * c) * ph;
                }
            }
        }
        check()?;
        if !rotated {
            converged = true;
            break;
        }
    }
    if !converged {
        return Ok(None);
    }
    let mut s: Vec<f64> = g.iter().map(|c| c.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()).collect();
    s.sort_by(|x, y| y.partial_cmp(x).unwrap());
    Ok(Some(s))
}

impl CLu {
    /// Entry (i, j) of the combined factor (U on and above the diagonal).
    pub fn entry(&self, i: usize, j: usize) -> C64 {
        self.lu[i * self.n + j]
    }
}

/// Full m×m unitary Q of the complex Householder QR of the row-major m×k matrix A (LAPACK zgeqr2 / zlarfg
/// conventions, reflectors on the first min(m, k) columns).
pub fn cqr_q_full(a: &[C64], m: usize, k: usize) -> Vec<C64> {
    let z0 = C64::new(0.0, 0.0);
    let mut a = a.to_vec();
    let kk = m.min(k);
    let mut taus = Vec::with_capacity(kk);
    for j in 0..kk {
        let alpha = a[j * k + j];
        let xnorm = (j + 1..m).map(|i| a[i * k + j].norm_sqr()).sum::<f64>().sqrt();
        if xnorm == 0.0 && alpha.im == 0.0 {
            taus.push(z0);
            continue;
        }
        let beta = -(alpha.re.hypot(alpha.im).hypot(xnorm)).copysign(alpha.re);
        let tau = C64::new((beta - alpha.re) / beta, -alpha.im / beta);
        let s = C64::new(1.0, 0.0) / (alpha - beta);
        for i in j + 1..m {
            a[i * k + j] *= s;
        }
        a[j * k + j] = C64::new(beta, 0.0);
        // apply Hᴴ = I − conj(τ)·v·vᴴ to the remaining columns
        for c in j + 1..k {
            let mut w = a[j * k + c];
            for i in j + 1..m {
                w += a[i * k + j].conj() * a[i * k + c];
            }
            w *= tau.conj();
            a[j * k + c] -= w;
            for i in j + 1..m {
                let vij = a[i * k + j];
                a[i * k + c] -= w * vij;
            }
        }
        taus.push(tau);
    }
    // Q = H_0·H_1·…; column e_c: apply H_{kk−1} first
    let mut q = vec![z0; m * m];
    for c in 0..m {
        let mut y = vec![z0; m];
        y[c] = C64::new(1.0, 0.0);
        for j in (0..kk).rev() {
            let tau = taus[j];
            if tau.re == 0.0 && tau.im == 0.0 {
                continue;
            }
            let mut w = y[j];
            for i in j + 1..m {
                w += a[i * k + j].conj() * y[i];
            }
            w *= tau;
            y[j] -= w;
            for i in j + 1..m {
                let vij = a[i * k + j];
                y[i] -= w * vij;
            }
        }
        for i in 0..m {
            q[i * m + c] = y[i];
        }
    }
    q
}
