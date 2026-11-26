//! Dense LU / Householder QR / 1-norm condition estimate / compensated residuals. Shared by every operator
//! that needs them (spec 11, RI-1): linalg.system.solve is the operator built directly on these kernels.
use crate::kernels::sum::Dot2;


// ------------------------------------------------------------------ LU with partial pivoting
pub struct Lu {
    n: usize,
    lu: Vec<f64>,     // row-major, unit-lower L below the diagonal, U on and above
    piv: Vec<usize>,  // row i of P·A is row piv[i] of A
    pub zero_pivot: Option<usize>,
}

pub fn lu_factor(a: &[f64], n: usize) -> Lu {
    let mut lu = a.to_vec();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut zero_pivot = None;
    for k in 0..n {
        // first row with maximal |a_ik| (dgetf2 / idamax: ties → smallest index)
        let mut p = k;
        let mut best = lu[k * n + k].abs();
        for i in k + 1..n {
            let v = lu[i * n + k].abs();
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
        if pivot == 0.0 {
            zero_pivot.get_or_insert(k);
            continue;
        }
        for i in k + 1..n {
            let l = lu[i * n + k] / pivot;
            lu[i * n + k] = l;
            if l != 0.0 {
                for j in k + 1..n {
                    lu[i * n + j] -= l * lu[k * n + j];
                }
            }
        }
    }
    Lu { n, lu, piv: perm, zero_pivot }
}

impl Lu {
    /// Sign of the row permutation P (+1 or −1), from the cycle structure of `piv`.
    pub fn perm_sign(&self) -> i64 {
        let mut seen = vec![false; self.n];
        let mut s = 1i64;
        for i in 0..self.n {
            if seen[i] {
                continue;
            }
            let mut len = 0usize;
            let mut j = i;
            while !seen[j] {
                seen[j] = true;
                j = self.piv[j];
                len += 1;
            }
            if len % 2 == 0 {
                s = -s;
            }
        }
        s
    }
    /// Entry (i, j) of the combined factor (U on and above the diagonal, L multipliers below).
    pub fn entry(&self, i: usize, j: usize) -> f64 {
        self.lu[i * self.n + j]
    }
    /// Solve A x = b.
    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.n;
        let mut y: Vec<f64> = self.piv.iter().map(|&p| b[p]).collect();
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
    /// Solve Aᵀ x = b (Uᵀ z = b, Lᵀ w = z, x = Pᵀ w).
    pub fn solve_t(&self, b: &[f64]) -> Vec<f64> {
        let n = self.n;
        let mut z = b.to_vec();
        for i in 0..n {
            let mut s = z[i];
            for j in 0..i {
                s -= self.lu[j * n + i] * z[j];
            }
            z[i] = s / self.lu[i * n + i];
        }
        for i in (0..n).rev() {
            let mut s = z[i];
            for j in i + 1..n {
                s -= self.lu[j * n + i] * z[j];
            }
            z[i] = s;
        }
        let mut x = vec![0.0; n];
        for (i, &p) in self.piv.iter().enumerate() {
            x[p] = z[i];
        }
        x
    }
}

/// Solve A·x = b with an existing LU factor of the n×n matrix `a` (row-major), then at most `max_steps` steps of
/// fixed-precision iterative refinement with the residual in compensated arithmetic (spec 10.19 LS-2): a
/// correction δ is discarded, and refinement stops, if ‖δ‖∞ is not below half of the previous one (the first is
/// compared with ∞); after adding δ, stop if ‖δ‖∞ ≤ 2u‖x‖∞. Returns (x, steps actually added).
/// Shared by linalg.system.solve and linalg.matrix.inv (RI-1).
pub fn lu_solve_refined(lu: &Lu, a: &[f64], n: usize, b: &[f64], max_steps: usize) -> (Vec<f64>, usize) {
    const U: f64 = f64::EPSILON / 2.0;
    let mut x = lu.solve(b);
    let mut steps = 0usize;
    let mut prev = f64::INFINITY;
    for _ in 0..max_steps {
        let r = residual_dd(a, n, n, &x, b);
        let d = lu.solve(&r);
        let dn = norm_inf(&d);
        if !dn.is_finite() || dn > prev / 2.0 {
            break;
        }
        for (xi, di) in x.iter_mut().zip(&d) {
            *xi += di;
        }
        steps += 1;
        prev = dn;
        if dn <= 2.0 * U * norm_inf(&x) {
            break;
        }
    }
    (x, steps)
}

// ------------------------------------------------------------------ Householder QR (m ≥ n)
pub struct Qr {
    m: usize,
    n: usize,
    a: Vec<f64>,   // row-major: R on/above the diagonal, Householder vectors v (v_k = 1 implicit) below
    tau: Vec<f64>,
}

/// dgeqr2/dlarfg: H = I − τ v vᵀ, v = [1; v₂], β = −sign(α)·‖[α; x]‖₂ (dlapy2 order: hypot).
pub fn qr_factor(a: &[f64], m: usize, n: usize) -> Qr {
    let mut a = a.to_vec();
    let mut tau = vec![0.0; n];
    for k in 0..n {
        let alpha = a[k * n + k];
        let mut xnorm = 0.0f64; // dnrm2 of a[k+1..m, k] with scaling
        {
            let (mut scale, mut ssq) = (0.0f64, 1.0f64);
            for i in k + 1..m {
                let v = a[i * n + k];
                if v != 0.0 {
                    let av = v.abs();
                    if scale < av {
                        ssq = 1.0 + ssq * (scale / av) * (scale / av);
                        scale = av;
                    } else {
                        ssq += (av / scale) * (av / scale);
                    }
                }
            }
            if scale > 0.0 {
                xnorm = scale * ssq.sqrt();
            }
        }
        if xnorm == 0.0 {
            tau[k] = 0.0;
        } else {
            let beta = -alpha.hypot(xnorm).copysign(alpha);
            tau[k] = (beta - alpha) / beta;
            let s = 1.0 / (alpha - beta);
            for i in k + 1..m {
                a[i * n + k] *= s;
            }
            a[k * n + k] = beta;
        }
        // apply H_k to a[k.., k+1..]
        if tau[k] != 0.0 {
            for j in k + 1..n {
                let mut w = a[k * n + j];
                for i in k + 1..m {
                    w += a[i * n + k] * a[i * n + j];
                }
                w *= tau[k];
                a[k * n + j] -= w;
                for i in k + 1..m {
                    a[i * n + j] -= w * a[i * n + k];
                }
            }
        }
    }
    Qr { m, n, a, tau }
}

impl Qr {
    /// Qᵀ b (length m).
    pub fn qt(&self, b: &[f64]) -> Vec<f64> {
        let (m, n) = (self.m, self.n);
        let mut y = b.to_vec();
        for k in 0..n {
            if self.tau[k] == 0.0 {
                continue;
            }
            let mut w = y[k];
            for i in k + 1..m {
                w += self.a[i * n + k] * y[i];
            }
            w *= self.tau[k];
            y[k] -= w;
            for i in k + 1..m {
                y[i] -= w * self.a[i * n + k];
            }
        }
        y
    }
    /// Q b (length m): H_0·H_1·…·H_{n−1}·b.
    pub fn q_apply(&self, b: &[f64]) -> Vec<f64> {
        let (m, n) = (self.m, self.n);
        let mut y = b.to_vec();
        for k in (0..n).rev() {
            if self.tau[k] == 0.0 {
                continue;
            }
            let mut w = y[k];
            for i in k + 1..m {
                w += self.a[i * n + k] * y[i];
            }
            w *= self.tau[k];
            y[k] -= w;
            for i in k + 1..m {
                y[i] -= w * self.a[i * n + k];
            }
        }
        y
    }
    pub fn r(&self, i: usize, j: usize) -> f64 {
        self.a[i * self.n + j]
    }
    /// R x = c (upper triangular n×n).
    pub fn solve_r(&self, c: &[f64]) -> Vec<f64> {
        let n = self.n;
        let mut x = c[..n].to_vec();
        for i in (0..n).rev() {
            let mut s = x[i];
            for j in i + 1..n {
                s -= self.r(i, j) * x[j];
            }
            x[i] = s / self.r(i, i);
        }
        x
    }
    /// Rᵀ x = c.
    pub fn solve_rt(&self, c: &[f64]) -> Vec<f64> {
        let n = self.n;
        let mut x = c[..n].to_vec();
        for i in 0..n {
            let mut s = x[i];
            for j in 0..i {
                s -= self.r(j, i) * x[j];
            }
            x[i] = s / self.r(i, i);
        }
        x
    }
}

/// Hager–Higham 1-norm estimate of ‖M⁻¹‖₁ given solvers for M and Mᵀ (at most 5 iterations, plus the
/// alternating vector of Higham 1988 Alg. 4.1). Deterministic: fixed start vector, ties → smallest index.
pub fn inv_norm1_estimate(n: usize, solve: &dyn Fn(&[f64]) -> Vec<f64>, solve_t: &dyn Fn(&[f64]) -> Vec<f64>) -> f64 {
    let mut x = vec![1.0 / n as f64; n];
    let mut est = 0.0f64;
    let mut last_j = usize::MAX;
    for it in 0..5 {
        let y = solve(&x);
        let new_est: f64 = y.iter().map(|v| v.abs()).sum();
        if it > 0 && new_est <= est {
            break;
        }
        est = new_est;
        let xi: Vec<f64> = y.iter().map(|&v| if v >= 0.0 { 1.0 } else { -1.0 }).collect();
        let z = solve_t(&xi);
        let (mut j, mut zmax) = (0usize, z[0].abs());
        for (i, v) in z.iter().enumerate().skip(1) {
            if v.abs() > zmax {
                zmax = v.abs();
                j = i;
            }
        }
        let ztx: f64 = z.iter().zip(&x).map(|(a, b)| a * b).sum();
        if zmax <= ztx || j == last_j {
            break;
        }
        last_j = j;
        x = vec![0.0; n];
        x[j] = 1.0;
    }
    let alt: Vec<f64> = (0..n).map(|i| (if i % 2 == 0 { 1.0 } else { -1.0 }) * (1.0 + i as f64 / (n.max(2) - 1) as f64)).collect();
    let y = solve(&alt);
    let alt_est = 2.0 * y.iter().map(|v| v.abs()).sum::<f64>() / (3.0 * n as f64);
    est.max(alt_est)
}

pub fn norm1(a: &[f64], m: usize, n: usize) -> f64 {
    (0..n).map(|j| (0..m).map(|i| a[i * n + j].abs()).sum::<f64>()).fold(0.0, f64::max)
}
pub fn norm_inf_mat(a: &[f64], m: usize, n: usize) -> f64 {
    (0..m).map(|i| (0..n).map(|j| a[i * n + j].abs()).sum::<f64>()).fold(0.0, f64::max)
}
pub fn norm_inf(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |s, x| s.max(x.abs()))
}
pub fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// r = b − A x in compensated arithmetic (Dot2), rounded once.
pub fn residual_dd(a: &[f64], m: usize, n: usize, x: &[f64], b: &[f64]) -> Vec<f64> {
    (0..m)
        .map(|i| {
            let mut d = Dot2::default();
            d.add(b[i]);
            for j in 0..n {
                d.add_prod(-a[i * n + j], x[j]);
            }
            let (h, l) = d.hi_lo();
            h + l
        })
        .collect()
}

/// Aᵀ r in compensated arithmetic.
pub fn at_r_dd(a: &[f64], m: usize, n: usize, r: &[f64]) -> Vec<f64> {
    (0..n)
        .map(|j| {
            let mut d = Dot2::default();
            for i in 0..m {
                d.add_prod(a[i * n + j], r[i]);
            }
            let (h, l) = d.hi_lo();
            h + l
        })
        .collect()
}


/// Cholesky A = L·Lᵀ of a symmetric row-major n×n matrix (lower triangle used); None unless every pivot is
/// positive. L is row-major lower triangular.
pub fn cholesky(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0; n * n];
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= l[j * n + k] * l[j * n + k];
        }
        if !(d > 0.0) {
            return None;
        }
        let djj = d.sqrt();
        l[j * n + j] = djj;
        for i in j + 1..n {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            l[i * n + j] = s / djj;
        }
    }
    Some(l)
}

/// Solve (L·Lᵀ) x = b with the factor from `cholesky`.
pub fn chol_solve(l: &[f64], n: usize, b: &[f64]) -> Vec<f64> {
    let mut y = b.to_vec();
    for i in 0..n {
        let mut s = y[i];
        for k in 0..i {
            s -= l[i * n + k] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k * n + i] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    y
}
