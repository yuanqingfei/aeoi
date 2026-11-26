//! Algebraic Riccati kernel (spec 11.5 RI-1). CARE: symplectic balancing of the Hamiltonian, matrix sign
//! function (Newton with determinantal scaling) for the stable invariant subspace, Newton–Kleinman defect
//! correction with compensated K-form residuals, and an iteratively refined gain. DARE: structure-preserving
//! doubling (SDA) followed by Newton–Hewer refinement. Shared by lqr, dlqr and later care / dare / kalman.
//! The operators keep validation, domain tests (PBH, imaginary-axis / unit-circle) and post-conditions.
use super::sum::Dot2;
use crate::error::OpError;
use crate::linalg::{norm1, symmetrize};
use crate::lti::eigenvalues;
use crate::sysalg::{lyap_bs, LyapKind};
use nalgebra::DMatrix;

/// H = [[Ā, −G], [−Q̄, −Āᵀ]]
pub fn hamiltonian(a: &DMatrix<f64>, g: &DMatrix<f64>, q: &DMatrix<f64>) -> DMatrix<f64> {
    let nx = a.nrows();
    let mut h = DMatrix::zeros(2 * nx, 2 * nx);
    h.view_mut((0, 0), (nx, nx)).copy_from(a);
    h.view_mut((0, nx), (nx, nx)).copy_from(&(-g));
    h.view_mut((nx, 0), (nx, nx)).copy_from(&(-q));
    h.view_mut((nx, nx), (nx, nx)).copy_from(&(-a.transpose()));
    h
}

pub enum SignErr {
    /// the iterate became singular
    Singular,
    /// no convergence within `max_it` (iterations, last relative change)
    NoConvergence(usize, f64),
    /// the least-squares problem for S failed
    Subspace,
    Abort(OpError),
}

/// Stabilising CARE solution from the matrix sign function of the Hamiltonian H (2n×2n): Newton iteration
/// Z ← (cZ + Z⁻¹/c)/2 with determinantal scaling until the relative change drops below 1e-2, stop at
/// ≤ 1e-13 or stagnation below 1e-9; then [W12; W22+I] S = −[W11+I; W21] by SVD least squares.
/// Returns (S unsymmetrised, iterations, last relative change).
pub fn care_sign(h: &DMatrix<f64>, nx: usize, max_it: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<(DMatrix<f64>, usize, f64), SignErr> {
    let mut z = h.clone();
    let mut scaling = true;
    let mut iters = 0;
    let mut last_diff = f64::INFINITY;
    let mut converged = false;
    while iters < max_it {
        check().map_err(SignErr::Abort)?;
        iters += 1;
        let zinv = super::nalg::inverse(&z).ok_or(SignErr::Singular)?;
        let c = if scaling {
            let u = super::nalg::lu_u(&z);
            let logdet: f64 = (0..2 * nx).map(|i| u[(i, i)].abs().ln()).sum();
            (-logdet / (2 * nx) as f64).exp()
        } else {
            1.0
        };
        let znew = (&z * c + &zinv * (1.0 / c)) * 0.5;
        let diff = norm1(&(&znew - &z)) / norm1(&znew).max(1e-300);
        z = znew;
        if diff < 1e-2 {
            scaling = false;
        }
        if diff <= 1e-13 || (diff >= last_diff && last_diff <= 1e-9) {
            converged = true;
            last_diff = diff.min(last_diff);
            break;
        }
        last_diff = diff;
    }
    if !converged && last_diff > 1e-8 {
        return Err(SignErr::NoConvergence(iters, last_diff));
    }
    // stable invariant subspace: (W + I)[I; S] = 0  ->  [W12; W22+I] S = -[W11+I; W21]
    let id = DMatrix::<f64>::identity(nx, nx);
    let mut lhs = DMatrix::zeros(2 * nx, nx);
    lhs.view_mut((0, 0), (nx, nx)).copy_from(&z.view((0, nx), (nx, nx)));
    lhs.view_mut((nx, 0), (nx, nx)).copy_from(&(z.view((nx, nx), (nx, nx)) + &id));
    let mut rhs = DMatrix::zeros(2 * nx, nx);
    rhs.view_mut((0, 0), (nx, nx)).copy_from(&(-(z.view((0, 0), (nx, nx)) + &id)));
    rhs.view_mut((nx, 0), (nx, nx)).copy_from(&(-z.view((nx, 0), (nx, nx)).into_owned()));
    let sb = super::nalg::svd_solve(&lhs, &rhs, 1e-14).ok_or(SignErr::Subspace)?;
    Ok((sb, iters, last_diff))
}

/// Result of `care_newton_kleinman`.
pub struct NewtonKleinman {
    pub s: DMatrix<f64>,
    /// low-order part of S (one more unapplied correction at the rounding floor), for an accurate K
    pub s_lo: DMatrix<f64>,
    pub steps: usize,
    pub change: f64,
    pub res_dd: f64,
}

/// Newton–Kleinman refinement of a CARE solution s0 in K-form, as defect correction (original coordinates,
/// N explicit). K = R⁻¹(BᵀX + Nᵀ) by Cholesky (`chol` = Cholesky factor of R); E(X) is the CARE residual in
/// compensated arithmetic; ΔX solves (A−BK)ᵀΔX + ΔX(A−BK) + E = 0 (Bartels–Stewart), X ← X + ΔX. Stops when
/// ‖ΔX‖_F ≤ 2ε‖X‖_F or when ‖ΔX‖ stops decreasing below 1e-8‖X‖_F (that step is not applied). Skipped when
/// `enabled` is false or A − BK(s0) is not Hurwitz; a result that is not Hurwitz is discarded.
#[allow(clippy::too_many_arguments)]
pub fn care_newton_kleinman(a: &DMatrix<f64>, b: &DMatrix<f64>, q: &DMatrix<f64>, r: &DMatrix<f64>, nm: &DMatrix<f64>, chol: &DMatrix<f64>, s0: DMatrix<f64>, enabled: bool, max_nk: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<NewtonKleinman, OpError> {
    let nx = a.nrows();
    let gain = |x: &DMatrix<f64>| super::nalg::chol_solve(chol, &(b.transpose() * x + nm.transpose()));
    let hurwitz = |x: &DMatrix<f64>| eigenvalues(&(a - b * gain(x))).iter().all(|p| p.re < 0.0);
    let mut s = s0;
    let mut newton_steps = 0;
    let mut nk_change = f64::NAN;
    let mut res_dd = f64::NAN;
    let mut s_lo = DMatrix::<f64>::zeros(nx, nx);
    let zero = DMatrix::<f64>::zeros(nx, nx);
    if enabled && hurwitz(&s) {
        let mut x = s.clone();
        let mut prev = f64::INFINITY;
        for _ in 0..max_nk {
            check()?;
            // accurate K keeps the quadratic term (K − R⁻¹P)ᵀR(K − R⁻¹P) ~ (cond(R)·u)²·‖KᵀRK‖ out of E
            let k = gain_refined(chol, r, b, nm, &x, &zero);
            let e = care_residual_kform(a, b, q, r, nm, &x, &k);
            res_dd = e.norm();
            let ak = a - b * &k;
            let Ok((dx, _)) = lyap_bs(&ak.transpose(), &e, LyapKind::Continuous) else { break };
            let dx = symmetrize(&dx);
            let dn = dx.norm();
            // ‖ΔX‖ may grow in the first (global) Newton steps; only near the rounding floor does a
            // non-decreasing correction mean "converged" (that correction is then not applied)
            if !dn.is_finite() || (dn >= prev && dn <= 1e-8 * x.norm()) {
                break;
            }
            x = symmetrize(&(&x + &dx));
            newton_steps += 1;
            nk_change = dn / x.norm().max(1e-300);
            prev = dn;
            if nk_change <= 2.0 * f64::EPSILON {
                break;
            }
        }
        if newton_steps > 0 {
            if hurwitz(&x) {
                let kx = gain_refined(chol, r, b, nm, &x, &zero);
                let e = care_residual_kform(a, b, q, r, nm, &x, &kx);
                res_dd = e.norm();
                if let Ok((dx, _)) = lyap_bs(&(a - b * &kx).transpose(), &e, LyapKind::Continuous) {
                    let dx = symmetrize(&dx);
                    if dx.norm() <= 1e-12 * x.norm() {
                        s_lo = dx;
                    }
                }
                s = x;
            } else {
                newton_steps = 0;
                nk_change = f64::NAN;
                res_dd = f64::NAN;
            }
        }
    }
    Ok(NewtonKleinman { s, s_lo, steps: newton_steps, change: nk_change, res_dd })
}

/// Symplectic diagonal balancing for H = [[Ā, −G], [−Q̄, −Āᵀ]]: D = diag(d, d⁻¹) with d_i powers of two,
/// chosen by coordinate descent to minimise the entrywise ℓ₁ norm of D⁻¹HD (a convex function of log d).
/// Makes the imaginary-axis test and the sign iteration independent of the state scaling
/// (e.g. R = diag(1, 1e-12) puts 1e12 into G and only 1 into Q).
pub fn symplectic_balance(a: &DMatrix<f64>, g: &DMatrix<f64>, q: &DMatrix<f64>) -> Vec<f64> {
    let n = a.nrows();
    let mut d = vec![1.0f64; n];
    const MAX_LOG2: i32 = 60;
    for _sweep in 0..30 {
        let mut changed = false;
        for i in 0..n {
            // contributions of index i that scale like f, f², 1/f, 1/f² when d_i ← f·d_i
            let (mut up, mut dn) = (0.0f64, 0.0f64);
            for j in 0..n {
                if j == i {
                    continue;
                }
                up += 2.0 * (a[(j, i)] * d[i] / d[j]).abs() + 2.0 * (q[(i, j)] * d[i] * d[j]).abs();
                dn += 2.0 * (a[(i, j)] * d[j] / d[i]).abs() + 2.0 * (g[(i, j)] / (d[i] * d[j])).abs();
            }
            let up2 = (q[(i, i)] * d[i] * d[i]).abs();
            let dn2 = (g[(i, i)] / (d[i] * d[i])).abs();
            let tot = |f: f64| up * f + up2 * f * f + dn / f + dn2 / (f * f);
            let k0 = d[i].log2().round() as i32;
            let mut k = 0i32;
            while k0 + k < MAX_LOG2 && tot(2f64.powi(k + 1)) < 0.95 * tot(2f64.powi(k)) {
                k += 1;
            }
            if k == 0 {
                while k0 + k > -MAX_LOG2 && tot(2f64.powi(k - 1)) < 0.95 * tot(2f64.powi(k)) {
                    k -= 1;
                }
            }
            if k != 0 {
                d[i] *= 2f64.powi(k);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    d
}

/// CARE residual in K-form, E = AᵀX + XA + Q − KᵀP − PᵀK + KᵀRK with P = BᵀX + Nᵀ, evaluated with compensated
/// dot products (Dot2; P and RK kept as unevaluated double-f64 sums). Equals the CARE residual up to the
/// quadratic term (K − R⁻¹P)ᵀR(K − R⁻¹P).
pub fn care_residual_kform(a: &DMatrix<f64>, b: &DMatrix<f64>, q: &DMatrix<f64>, r: &DMatrix<f64>, nm: &DMatrix<f64>, x: &DMatrix<f64>, k: &DMatrix<f64>) -> DMatrix<f64> {
    let (n, m) = (a.nrows(), b.ncols());
    // P = BᵀX + Nᵀ and T = RK as hi/lo pairs; D = T − P (small) rounded once
    let mut ph = DMatrix::<f64>::zeros(m, n);
    let mut pl = DMatrix::<f64>::zeros(m, n);
    let mut dd = DMatrix::<f64>::zeros(m, n);
    for j in 0..n {
        for i in 0..m {
            let mut p = Dot2::default();
            for l in 0..n {
                p.add_prod(b[(l, i)], x[(l, j)]);
            }
            p.add(nm[(j, i)]);
            let (h, lo) = p.hi_lo();
            ph[(i, j)] = h;
            pl[(i, j)] = lo;
            let mut t = Dot2::default();
            for l in 0..m {
                t.add_prod(r[(i, l)], k[(l, j)]);
            }
            t.add(-h);
            t.add(-lo);
            let (th, tl) = t.hi_lo();
            dd[(i, j)] = th + tl;
        }
    }
    // E = AᵀX + XA + Q + KᵀD − PᵀK   (KᵀRK − KᵀP − PᵀK = KᵀD − PᵀK)
    let mut e = DMatrix::<f64>::zeros(n, n);
    for j in 0..n {
        for i in 0..n {
            let mut s = Dot2::default();
            for l in 0..n {
                s.add_prod(a[(l, i)], x[(l, j)]);
                s.add_prod(x[(i, l)], a[(l, j)]);
            }
            s.add(q[(i, j)]);
            for l in 0..m {
                s.add_prod(k[(l, i)], dd[(l, j)]);
                s.add_prod(-ph[(l, i)], k[(l, j)]);
                s.add_prod(-pl[(l, i)], k[(l, j)]);
            }
            let (h, lo) = s.hi_lo();
            e[(i, j)] = h + lo;
        }
    }
    symmetrize(&e)
}

/// K = R⁻¹(Bᵀ(S + S_lo) + Nᵀ) with iterative refinement (S_lo: low-order part of S kept from the last Newton step): the Cholesky solve alone has forward error ~cond(R)·u
/// (1e-4 at cond(R) = 1e12); corrections K ← K + R⁻¹(P − RK) with P and the residual in compensated arithmetic
/// reach ~u while cond(R)·u < 1. At most 4 corrections, stopping when they stop shrinking.
pub fn gain_refined(chol: &DMatrix<f64>, r: &DMatrix<f64>, b: &DMatrix<f64>, nm: &DMatrix<f64>, s: &DMatrix<f64>, s_lo: &DMatrix<f64>) -> DMatrix<f64> {
    let (n, m) = (s.nrows(), b.ncols());
    let mut ph = DMatrix::<f64>::zeros(m, n);
    let mut pl = DMatrix::<f64>::zeros(m, n);
    for j in 0..n {
        for i in 0..m {
            let mut p = Dot2::default();
            for l in 0..n {
                p.add_prod(b[(l, i)], s[(l, j)]);
                p.add_prod(b[(l, i)], s_lo[(l, j)]);
            }
            p.add(nm[(j, i)]);
            let (h, lo) = p.hi_lo();
            ph[(i, j)] = h;
            pl[(i, j)] = lo;
        }
    }
    let mut k = super::nalg::chol_solve(chol, &ph);
    let mut prev = f64::INFINITY;
    for _ in 0..4 {
        let mut res = DMatrix::<f64>::zeros(m, n);
        for j in 0..n {
            for i in 0..m {
                let mut t = Dot2::default();
                t.add(ph[(i, j)]);
                t.add(pl[(i, j)]);
                for l in 0..m {
                    t.add_prod(-r[(i, l)], k[(l, j)]);
                }
                let (h, lo) = t.hi_lo();
                res[(i, j)] = h + lo;
            }
        }
        let dk = super::nalg::chol_solve(chol, &res);
        let dn = dk.norm();
        if !dn.is_finite() || dn >= prev || dn == 0.0 {
            break;
        }
        k += dk;
        prev = dn;
        if dn <= f64::EPSILON * k.norm() {
            break;
        }
    }
    k
}

// ------------------------------------------------------------------------------------------------ DARE

/// DARE residual AᵀXA − X − (AᵀXB+N)(R+BᵀXB)⁻¹(BᵀXA+Nᵀ) + Q and the gain K.
pub fn dare_residual(a: &DMatrix<f64>, b: &DMatrix<f64>, q: &DMatrix<f64>, r: &DMatrix<f64>, n: &DMatrix<f64>, x: &DMatrix<f64>) -> Option<(DMatrix<f64>, DMatrix<f64>)> {
    let rb = r + b.transpose() * x * b;
    let rhs = b.transpose() * x * a + n.transpose();
    let k = super::nalg::cholesky(&rb).map(|c| super::nalg::chol_solve(&c, &rhs)).or_else(|| super::nalg::solve(&rb, &rhs))?;
    let res = a.transpose() * x * a - x - (a.transpose() * x * b + n) * &k + q;
    Some((symmetrize(&res), k))
}

pub enum SdaErr {
    /// I + G·H became singular
    Singular,
    /// (iterations, last relative change)
    NoConvergence(usize, f64),
    Abort(OpError),
}

/// Structure-preserving doubling for the DARE in (Ā, G, Q̄) form: returns (H_∞ unsymmetrised, iterations,
/// last relative change). Stops at ‖H_{k+1}−H_k‖_F/‖H_{k+1}‖_F ≤ 1e-15 or stagnation below 1e-12.
pub fn sda(abar: &DMatrix<f64>, g: &DMatrix<f64>, qbar: &DMatrix<f64>, max_it: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<(DMatrix<f64>, usize, f64), SdaErr> {
    let nx = abar.nrows();
    let id = DMatrix::<f64>::identity(nx, nx);
    let (mut ak, mut gk, mut hk) = (abar.clone(), g.clone(), qbar.clone());
    let (mut iters, mut last, mut converged) = (0usize, f64::INFINITY, false);
    while iters < max_it {
        check().map_err(SdaErr::Abort)?;
        iters += 1;
        let w = &id + &gk * &hk;
        let wa = super::nalg::solve(&w, &ak).ok_or(SdaErr::Singular)?;
        let wg = super::nalg::solve(&w, &gk).unwrap();
        let a_next = &ak * &wa;
        let g_next = symmetrize(&(&gk + &ak * wg * ak.transpose()));
        let h_next = symmetrize(&(&hk + ak.transpose() * &hk * &wa));
        let diff = (&h_next - &hk).norm() / h_next.norm().max(f64::MIN_POSITIVE);
        ak = a_next;
        gk = g_next;
        hk = h_next;
        if !hk.iter().all(|v| v.is_finite()) {
            break;
        }
        if diff <= 1e-15 || (diff >= last && last <= 1e-12) {
            converged = true;
            last = diff.min(last);
            break;
        }
        last = diff;
    }
    if !converged && !(last <= 1e-8) {
        return Err(SdaErr::NoConvergence(iters, last));
    }
    Ok((hk, iters, last))
}

/// Newton–Hewer refinement of a DARE solution (at most 3 steps, each kept only if it lowers the residual):
/// X = A_kᵀ X A_k + [I; −K]ᵀ W [I; −K]. Returns (S, residual, K, steps).
#[allow(clippy::too_many_arguments)]
pub fn newton_hewer(a: &DMatrix<f64>, b: &DMatrix<f64>, q: &DMatrix<f64>, r: &DMatrix<f64>, nm: &DMatrix<f64>, s0: DMatrix<f64>, res0: DMatrix<f64>, k0: DMatrix<f64>, check: &dyn Fn() -> Result<(), OpError>) -> Result<(DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, usize), OpError> {
    let (mut s, mut res, mut k) = (s0, res0, k0);
    let mut newton = 0;
    for _ in 0..3 {
        check()?;
        let acl = a - b * &k;
        let wq = q - nm * &k - k.transpose() * nm.transpose() + k.transpose() * r * &k;
        let Ok((x, _)) = lyap_bs(&acl.transpose(), &symmetrize(&wq), LyapKind::Discrete) else { break };
        let x = symmetrize(&x);
        let Some((rx, kx)) = dare_residual(a, b, q, r, nm, &x) else { break };
        if rx.norm() < res.norm() {
            s = x;
            res = rx;
            k = kx;
            newton += 1;
        } else {
            break;
        }
    }
    Ok((s, res, k, newton))
}
