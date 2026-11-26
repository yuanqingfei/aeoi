//! Time-response kernel: exact hold discretisation of ẋ = Ax + Bu by one augmented matrix exponential
//! (Van Loan), zero-order and first-order (triangle) hold, and the discrete recursion with interpolated
//! input. Shared by step, impulse, lsim and c2d (spec 11.5 RI-1).
use crate::error::OpError;
use crate::linalg::expm;
use nalgebra::{DMatrix, DVector};

/// expm of [[A h, B h, 0], [0, 0, I], [0, 0, 0]] (the last block row/column only for foh). Returns
/// (Φ = e^{Ah}, F1 = ∫₀ʰ e^{As} ds B, F2 (foh: (1/h)∫₀ʰ e^{As}(h − s) ds B; zoh: None), squarings).
pub fn hold(a: &DMatrix<f64>, b: &DMatrix<f64>, h: f64, foh: bool) -> (DMatrix<f64>, DMatrix<f64>, Option<DMatrix<f64>>, u32) {
    let (nx, nu) = (a.nrows(), b.ncols());
    let size = nx + if foh { 2 * nu } else { nu };
    let mut m = DMatrix::zeros(size, size);
    m.view_mut((0, 0), (nx, nx)).copy_from(&(a * h));
    m.view_mut((0, nx), (nx, nu)).copy_from(&(b * h));
    if foh {
        m.view_mut((nx, nx + nu), (nu, nu)).copy_from(&DMatrix::identity(nu, nu));
    }
    let (e, sq) = expm(&m);
    let phi = e.view((0, 0), (nx, nx)).into_owned();
    let f1 = e.view((0, nx), (nx, nu)).into_owned();
    let f2 = if foh { Some(e.view((0, nx + nu), (nx, nu)).into_owned()) } else { None };
    (phi, f1, f2, sq)
}

/// (Φ, Γ0, Γ1, squarings) with x_{k+1} = Φ x_k + Γ0 u_k + Γ1 u_{k+1}; Γ1 = 0 for zoh.
pub fn hold_gammas(a: &DMatrix<f64>, b: &DMatrix<f64>, h: f64, foh: bool) -> (DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, u32) {
    let (phi, f1, f2, sq) = hold(a, b, h, foh);
    match f2 {
        Some(f2) => (phi, &f1 - &f2, f2, sq),
        None => {
            let (nx, nu) = (a.nrows(), b.ncols());
            (phi, f1, DMatrix::zeros(nx, nu), sq)
        }
    }
}

/// y_k = C x_k + D u_k, x_{k+1} = Φ x_k + Γ0 u_k (+ Γ1 u_{k+1} when `foh`), u is nu × nt. Returns y
/// (row-major ny × nt) and the first `keep` states. `check` every 20000 steps.
#[allow(clippy::too_many_arguments)]
pub fn propagate(phi: &DMatrix<f64>, g0: &DMatrix<f64>, g1: &DMatrix<f64>, c: &DMatrix<f64>, d: &DMatrix<f64>, u: &DMatrix<f64>, x0: &DVector<f64>, foh: bool, keep: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<(Vec<f64>, Vec<DVector<f64>>), OpError> {
    let ny = c.nrows();
    let nt = u.ncols();
    let mut y = vec![0.0f64; ny * nt];
    let mut xs: Vec<DVector<f64>> = Vec::with_capacity(keep);
    let mut x = x0.clone();
    for k in 0..nt {
        let uk = u.column(k);
        let yk = c * &x + d * uk;
        for r in 0..ny {
            y[r * nt + k] = yk[r];
        }
        if k < keep {
            xs.push(x.clone());
        }
        if k + 1 < nt {
            let mut xn = phi * &x + g0 * uk;
            if foh {
                xn += g1 * u.column(k + 1);
            }
            x = xn;
        }
        if k % 20000 == 0 {
            check()?;
        }
    }
    Ok((y, xs))
}
