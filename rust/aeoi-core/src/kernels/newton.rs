//! Nonlinear systems F(x) = 0 by Powell's dogleg trust region (MATLAB fsolve's default) with the Jacobian
//! supplied by the caller (fsolve: forward-mode automatic differentiation), Newton steps by the LU kernel.
//! Shared by fsolve and later implicit solvers (stiff ODE, BVP collocation) — spec 11.5 RI-1.
//! Matrices are row-major n×n slices.
use super::dense::{inv_norm1_estimate, lu_factor, norm_inf};
use crate::error::OpError;

const U: f64 = f64::EPSILON / 2.0;

pub fn half_sq(f: &[f64]) -> f64 {
    0.5 * f.iter().map(|v| v * v).sum::<f64>()
}
pub fn n2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}
pub fn matvec(j: &[f64], v: &[f64], n: usize) -> Vec<f64> {
    (0..n).map(|r| (0..n).map(|c| j[r * n + c] * v[c]).sum()).collect()
}
pub fn matvec_t(j: &[f64], v: &[f64], n: usize) -> Vec<f64> {
    (0..n).map(|c| (0..n).map(|r| j[r * n + c] * v[r]).sum()).collect()
}

/// Outcome of `dogleg`. `x`, `f`, `j` are the last accepted point; `evals` counts trial evaluations
/// (not the initial one); `newton_norms` are ‖Newton step‖∞ per iteration (for linear-convergence tests).
pub struct Dogleg {
    pub x: Vec<f64>,
    pub f: Vec<f64>,
    pub j: Vec<f64>,
    pub iters: usize,
    pub evals: usize,
    pub converged: bool,
    pub stalled: bool,
    pub newton_norms: Vec<f64>,
}

/// Dogleg trust region from x0 with F(x0) = f0, J(x0) = j0 (finite). `fj(x)` returns (F, J) at x;
/// a non-finite value there rejects the trial step. Converged when the full Newton step satisfies
/// ‖p‖∞ ≤ 4u‖x‖∞ or F = 0; stalled when no step decreases ½‖F‖² or when ½‖F‖² fell by < 0.1% over the
/// last 10 accepted steps (MINPACK-style). `check` is called once per iteration.
pub fn dogleg(
    fj: &mut dyn FnMut(&[f64]) -> (Vec<f64>, Vec<f64>),
    x0: Vec<f64>,
    f0: Vec<f64>,
    j0: Vec<f64>,
    max_iter: usize,
    check: &dyn Fn() -> Result<(), OpError>,
) -> Result<Dogleg, OpError> {
    let n = x0.len();
    let (mut xs, mut f, mut j) = (x0, f0, j0);
    let mut evals = 0usize;
    let mut phi = half_sq(&f);
    let mut delta = n2(&xs).max(1.0);
    let mut iters = 0usize;
    let mut converged = false;
    let mut stalled = false;
    let mut newton_norms: Vec<f64> = Vec::new();
    let mut phi_hist: Vec<f64> = vec![phi];
    while iters < max_iter {
        if phi == 0.0 {
            converged = true;
            break;
        }
        iters += 1;
        let x = xs.clone();
        let g = matvec_t(&j, &f, n);
        let lu = lu_factor(&j, n);
        let pn: Option<Vec<f64>> = if lu.zero_pivot.is_none() {
            let s: Vec<f64> = lu.solve(&f).iter().map(|v| -v).collect();
            if s.iter().all(|v| v.is_finite()) { Some(s) } else { None }
        } else {
            None
        };
        let gn = n2(&g);
        let jg = matvec(&j, &g, n);
        let pc: Vec<f64> = if gn == 0.0 { vec![0.0; n] } else { let alpha = gn * gn / jg.iter().map(|v| v * v).sum::<f64>(); g.iter().map(|v| -alpha * v).collect() };
        if let Some(p) = &pn {
            newton_norms.push(norm_inf(p));
            // full Newton step tiny ⇒ converged
            if norm_inf(p) <= 4.0 * U * norm_inf(&x) {
                converged = true;
                break;
            }
        }
        if pn.is_none() && gn == 0.0 {
            stalled = true;
            break;
        }
        let mut accepted = false;
        for _ in 0..60 {
            let (step, full) = match &pn {
                Some(p) if n2(p) <= delta => (p.clone(), true),
                _ => {
                    let pcn = n2(&pc);
                    if pn.is_none() || pcn >= delta {
                        (if gn == 0.0 { vec![0.0; n] } else { g.iter().map(|v| -delta * v / gn).collect() }, false)
                    } else {
                        let p = pn.as_ref().unwrap();
                        let dvec: Vec<f64> = p.iter().zip(&pc).map(|(a, b)| a - b).collect();
                        let (aa, bb, cc) = (dvec.iter().map(|v| v * v).sum::<f64>(), 2.0 * pc.iter().zip(&dvec).map(|(a, b)| a * b).sum::<f64>(), pcn * pcn - delta * delta);
                        let tau = (-bb + (bb * bb - 4.0 * aa * cc).sqrt()) / (2.0 * aa);
                        (pc.iter().zip(&dvec).map(|(a, b)| a + tau * b).collect(), false)
                    }
                }
            };
            let pnorm = n2(&step);
            if pnorm <= 4.0 * U * n2(&x) {
                break;
            }
            let lin: Vec<f64> = f.iter().zip(matvec(&j, &step, n)).map(|(a, b)| a + b).collect();
            let pred = phi - half_sq(&lin);
            let trial: Vec<f64> = (0..n).map(|k| x[k] + step[k]).collect();
            let (ft, jt) = fj(&trial);
            evals += 1;
            let ok = ft.iter().chain(jt.iter()).all(|v| v.is_finite());
            let phit = if ok { half_sq(&ft) } else { f64::INFINITY };
            let rho = if pred > 0.0 { (phi - phit) / pred } else { -1.0 };
            if rho < 0.25 {
                delta = 0.25 * pnorm;
            } else if rho > 0.75 && (full || (pnorm - delta).abs() <= 1e-12 * delta) {
                delta = (2.0 * pnorm).max(delta);
            }
            if ok && rho > 1e-4 && phit < phi {
                xs = trial;
                f = ft;
                j = jt;
                phi = phit;
                accepted = true;
                break;
            }
        }
        if !accepted {
            stalled = true;
            break;
        }
        phi_hist.push(phi);
        let h = phi_hist.len();
        if h > 10 && phi > (1.0 - 1e-3) * phi_hist[h - 11] {
            stalled = true;
            break;
        }
        check()?;
    }
    Ok(Dogleg { x: xs, f, j, iters, evals, converged, stalled, newton_norms })
}

/// A posteriori at a computed root: (‖next Newton correction‖∞, κ₁(J) estimate); both ∞ if J is singular.
pub fn aposteriori(f: &[f64], j: &[f64], n: usize) -> (f64, f64) {
    let lu = lu_factor(j, n);
    if lu.zero_pivot.is_none() {
        let d = lu.solve(f);
        let inv1 = inv_norm1_estimate(n, &|v| lu.solve(v), &|v| lu.solve_t(v));
        let jn1 = (0..n).map(|c| (0..n).map(|r| j[r * n + c].abs()).sum::<f64>()).fold(0.0, f64::max);
        (norm_inf(&d), jn1 * inv1)
    } else {
        (f64::INFINITY, f64::INFINITY)
    }
}
