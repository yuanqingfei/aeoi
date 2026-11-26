//! nalgebra-matrix adapters over the AEOI kernels. Control code keeps nalgebra's `DMatrix` as its container and
//! for products, but every decomposition and solve goes through these functions, i.e. through the kernels the
//! linalg.* operators are built on (spec 11.5 RI-1, RI-3; v0.4.16). Nothing here calls nalgebra's LU, QR,
//! Cholesky, SVD or eigen-solvers.
use crate::error::OpError;
use crate::kernels::{cdense, dense, eig, svd};
use crate::lti::C64;
use nalgebra::DMatrix;

fn no_cancel() -> Result<(), OpError> {
    Ok(())
}

pub fn rows(a: &DMatrix<f64>) -> Vec<f64> {
    let (m, n) = a.shape();
    let mut v = Vec::with_capacity(m * n);
    for i in 0..m {
        for j in 0..n {
            v.push(a[(i, j)]);
        }
    }
    v
}
fn crows(a: &DMatrix<C64>) -> Vec<C64> {
    let (m, n) = a.shape();
    let mut v = Vec::with_capacity(m * n);
    for i in 0..m {
        for j in 0..n {
            v.push(a[(i, j)]);
        }
    }
    v
}

/// A·X = B by the LU kernel (partial pivoting). None on an exact zero pivot or a non-finite result.
pub fn solve(a: &DMatrix<f64>, b: &DMatrix<f64>) -> Option<DMatrix<f64>> {
    let n = a.nrows();
    if a.ncols() != n || b.nrows() != n {
        return None;
    }
    let lu = dense::lu_factor(&rows(a), n);
    if lu.zero_pivot.is_some() {
        return None;
    }
    let mut x = DMatrix::zeros(n, b.ncols());
    for j in 0..b.ncols() {
        let col: Vec<f64> = (0..n).map(|i| b[(i, j)]).collect();
        let s = lu.solve(&col);
        for i in 0..n {
            x[(i, j)] = s[i];
        }
    }
    if x.iter().all(|v| v.is_finite()) { Some(x) } else { None }
}

/// A⁻¹ by the LU kernel.
pub fn inverse(a: &DMatrix<f64>) -> Option<DMatrix<f64>> {
    solve(a, &DMatrix::identity(a.nrows(), a.nrows()))
}

/// Complex A·X = B by the complex LU kernel.
pub fn csolve(a: &DMatrix<C64>, b: &DMatrix<C64>) -> Option<DMatrix<C64>> {
    let n = a.nrows();
    if a.ncols() != n || b.nrows() != n {
        return None;
    }
    let lu = cdense::clu_factor(&crows(a), n);
    if lu.zero_pivot.is_some() {
        return None;
    }
    let mut x = DMatrix::from_element(n, b.ncols(), C64::new(0.0, 0.0));
    for j in 0..b.ncols() {
        let col: Vec<C64> = (0..n).map(|i| b[(i, j)]).collect();
        let s = lu.solve(&col);
        for i in 0..n {
            x[(i, j)] = s[i];
        }
    }
    if x.iter().all(|v| v.re.is_finite() && v.im.is_finite()) { Some(x) } else { None }
}

/// Cholesky factor (lower, as a DMatrix) of a symmetric matrix; None unless positive definite.
pub fn cholesky(a: &DMatrix<f64>) -> Option<DMatrix<f64>> {
    let n = a.nrows();
    dense::cholesky(&rows(a), n).map(|l| DMatrix::from_row_slice(n, n, &l))
}

/// (L·Lᵀ)·X = B with a factor from `cholesky`.
pub fn chol_solve(l: &DMatrix<f64>, b: &DMatrix<f64>) -> DMatrix<f64> {
    let n = l.nrows();
    let lr = rows(l);
    let mut x = DMatrix::zeros(n, b.ncols());
    for j in 0..b.ncols() {
        let col: Vec<f64> = (0..n).map(|i| b[(i, j)]).collect();
        let s = dense::chol_solve(&lr, n, &col);
        for i in 0..n {
            x[(i, j)] = s[i];
        }
    }
    x
}

/// Singular values, descending (Jacobi SVD kernel). NaN-filled if the sweeps do not converge.
pub fn singular_values(a: &DMatrix<f64>) -> Vec<f64> {
    let (m, n) = a.shape();
    if m == 0 || n == 0 {
        return vec![];
    }
    match svd::svd_thin(&rows(a), m, n, &no_cancel) {
        Ok(Some(s)) => s.s,
        _ => vec![f64::NAN; m.min(n)],
    }
}

/// Thin SVD: (s descending, U as m×p, V as n×p).
pub fn svd_thin(a: &DMatrix<f64>) -> Option<(Vec<f64>, DMatrix<f64>, DMatrix<f64>)> {
    let (m, n) = a.shape();
    let s = svd::svd_thin(&rows(a), m, n, &no_cancel).ok()??;
    let p = s.s.len();
    let u = DMatrix::from_fn(m, p, |i, k| s.u[k][i]);
    let v = DMatrix::from_fn(n, p, |i, k| s.v[k][i]);
    Some((s.s, u, v))
}

/// Minimum-norm least-squares solution via the SVD kernel, singular values ≤ `eps` treated as zero
/// (the semantics of nalgebra's `SVD::solve(b, eps)` it replaces).
pub fn svd_solve(a: &DMatrix<f64>, b: &DMatrix<f64>, eps: f64) -> Option<DMatrix<f64>> {
    let (s, u, v) = svd_thin(a)?;
    let utb = u.transpose() * b;
    let mut y = utb.clone();
    for k in 0..s.len() {
        let inv = if s[k] > eps { 1.0 / s[k] } else { 0.0 };
        for j in 0..y.ncols() {
            y[(k, j)] *= inv;
        }
    }
    Some(v * y)
}

/// Complex singular values, descending.
pub fn csingular_values(a: &DMatrix<C64>) -> Vec<f64> {
    let (m, n) = a.shape();
    if m == 0 || n == 0 {
        return vec![];
    }
    match cdense::csingular_values(&crows(a), m, n, &no_cancel) {
        Ok(Some(s)) => s,
        _ => vec![f64::NAN; m.min(n)],
    }
}

/// Eigenvalues of a symmetric matrix, ascending (Jacobi kernel). NaN-filled on non-convergence.
pub fn sym_eigenvalues(a: &DMatrix<f64>) -> Vec<f64> {
    sym_eigen(a).map(|(v, _)| v).unwrap_or_else(|| vec![f64::NAN; a.nrows()])
}

/// Symmetric eigen-decomposition: (values ascending, eigenvectors as the columns of a matrix).
pub fn sym_eigen(a: &DMatrix<f64>) -> Option<(Vec<f64>, DMatrix<f64>)> {
    let n = a.nrows();
    let sym = DMatrix::from_fn(n, n, |i, j| 0.5 * (a[(i, j)] + a[(j, i)]));
    let (vals, vecs) = eig::sym_jacobi(&rows(&sym), n, &no_cancel).ok()??;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| vals[x].partial_cmp(&vals[y]).unwrap().then(x.cmp(&y)));
    let v = DMatrix::from_fn(n, n, |i, k| vecs[order[k]][i]);
    Some((order.iter().map(|&k| vals[k]).collect(), v))
}

/// The full m×m orthogonal Q of the Householder QR of A (m×k), by the QR kernel.
pub fn q_full(a: &DMatrix<f64>) -> DMatrix<f64> {
    let (m, k) = a.shape();
    let qr = dense::qr_factor(&rows(a), m, k);
    let mut q = DMatrix::zeros(m, m);
    for j in 0..m {
        let mut e = vec![0.0; m];
        e[j] = 1.0;
        let col = qr.q_apply(&e);
        for i in 0..m {
            q[(i, j)] = col[i];
        }
    }
    q
}

/// R of the Householder QR (k×k upper triangle for m ≥ k) — the factor `place` uses.
pub fn r_factor(a: &DMatrix<f64>) -> DMatrix<f64> {
    let (m, k) = a.shape();
    let qr = dense::qr_factor(&rows(a), m, k);
    DMatrix::from_fn(k.min(m), k, |i, j| if j >= i { qr.r(i, j) } else { 0.0 })
}

/// U of the LU factorisation with partial pivoting (for the pivot-growth diagnostics of lqr).
pub fn lu_u(a: &DMatrix<f64>) -> DMatrix<f64> {
    let n = a.nrows();
    let lu = dense::lu_factor(&rows(a), n);
    DMatrix::from_fn(n, n, |i, j| if j >= i { lu.entry(i, j) } else { 0.0 })
}

/// Full unitary Q (n×n) of the complex Householder QR of A (n×k), reflectors on the first min(n, k) columns.
pub fn cq_full(a: &DMatrix<C64>) -> DMatrix<C64> {
    let (m, k) = a.shape();
    let q = cdense::cqr_q_full(&crows(a), m, k);
    DMatrix::from_row_slice(m, m, &q)
}

/// log Σ|u_ii| of the complex LU factor (log |det| up to the permutation sign).
pub fn clog_abs_det(a: &DMatrix<C64>) -> f64 {
    let n = a.nrows();
    let lu = cdense::clu_factor(&crows(a), n);
    (0..n).map(|i| lu.entry(i, i).norm().ln()).sum()
}
