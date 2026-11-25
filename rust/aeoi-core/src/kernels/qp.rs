//! Convex quadratic programming kernel (spec 10.29): primal active-set method for
//!   min ½ pᵀHp + gᵀp  s.t.  Aeq p = beq,  A p ≤ b      (H symmetric positive definite)
//! started from a feasible vertex found by the LP kernel (phase I), equality-constrained steps by the LU
//! kernel on the KKT matrix, independence of the working set checked with the QR kernel (spec 11.5 RI-1).
//! The SQP iteration of optim.local.fmincon is built on this; a future quadprog operator wraps it directly.
use crate::kernels::dense::{lu_factor, qr_factor};
use crate::kernels::lp::{self, LpError, LpProblem};

pub struct QpSolution {
    pub p: Vec<f64>,
    /// multipliers: ineq (≥ 0, one per row of A), eq (one per row of Aeq)
    pub lam_ineq: Vec<f64>,
    pub lam_eq: Vec<f64>,
    pub iterations: usize,
}

#[derive(Debug)]
pub enum QpError {
    Infeasible,
    IterationLimit,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Rank test: are the rows independent (|r_kk| > n·ε·max|r_ii|)?
fn independent(rows: &[&Vec<f64>], n: usize) -> bool {
    let k = rows.len();
    if k == 0 {
        return true;
    }
    if k > n {
        return false;
    }
    let mut a = vec![0.0; n * k];
    for (j, r) in rows.iter().enumerate() {
        for i in 0..n {
            a[i * k + j] = r[i];
        }
    }
    let qr = qr_factor(&a, n, k);
    let mx = (0..k).map(|j| qr.r(j, j).abs()).fold(0.0, f64::max);
    (0..k).all(|j| qr.r(j, j).abs() > (n.max(k) as f64) * f64::EPSILON * mx.max(f64::MIN_POSITIVE))
}

pub fn solve(h: &[f64], g: &[f64], a: &[Vec<f64>], b: &[f64], aeq: &[Vec<f64>], beq: &[f64], max_iter: usize) -> Result<QpSolution, QpError> {
    let n = g.len();
    // feasible start: LP with zero cost, free variables
    let zero = vec![0.0; n];
    let free_l = vec![f64::NEG_INFINITY; n];
    let free_u = vec![f64::INFINITY; n];
    let lpp = LpProblem { c: &zero, a, b, aeq, beq, lb: &free_l, ub: &free_u };
    let mut p = match lp::solve(&lpp, 50 * (n + a.len() + aeq.len()).max(10)) {
        Ok(s) => s.x,
        Err(LpError::Infeasible { .. }) => return Err(QpError::Infeasible),
        Err(_) => return Err(QpError::IterationLimit),
    };
    let scale = |i: usize| b[i].abs().max(1.0);
    let mut work: Vec<usize> = Vec::new(); // active inequality indices
    {
        let mut rows: Vec<&Vec<f64>> = aeq.iter().collect();
        for i in 0..a.len() {
            if (dot(&a[i], &p) - b[i]).abs() <= 1e-9 * scale(i) {
                rows.push(&a[i]);
                if independent(&rows, n) && rows.len() <= n {
                    work.push(i);
                } else {
                    rows.pop();
                }
            }
        }
    }
    let me = aeq.len();
    for it in 0..max_iter {
        // KKT: [H Wᵀ; W 0] [s; μ] = [−(Hp + g); 0]
        let k = me + work.len();
        let dim = n + k;
        let mut kkt = vec![0.0; dim * dim];
        for i in 0..n {
            for j in 0..n {
                kkt[i * dim + j] = h[i * n + j];
            }
        }
        let wrow = |r: usize| -> &Vec<f64> { if r < me { &aeq[r] } else { &a[work[r - me]] } };
        for r in 0..k {
            let row = wrow(r);
            for j in 0..n {
                kkt[(n + r) * dim + j] = row[j];
                kkt[j * dim + n + r] = row[j];
            }
        }
        let mut rhs = vec![0.0; dim];
        for i in 0..n {
            rhs[i] = -(dot(&h[i * n..(i + 1) * n], &p) + g[i]);
        }
        let lu = lu_factor(&kkt, dim);
        if lu.zero_pivot.is_some() {
            // dependent working set: drop the most recently added inequality
            if work.pop().is_none() {
                return Err(QpError::IterationLimit);
            }
            continue;
        }
        let sol = lu.solve(&rhs);
        let s = &sol[..n];
        let mu = &sol[n..];
        let sn = s.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let pn = p.iter().fold(1.0f64, |m, v| m.max(v.abs()));
        if sn <= 1e-13 * pn {
            // multipliers: KKT gives H s + Wᵀμ = −(Hp+g); with the sign convention λ = μ for a·p ≤ b rows
            let (mut worst, mut wi) = (0.0, None);
            for (r, _) in work.iter().enumerate() {
                let l = mu[me + r];
                if l < worst {
                    worst = l;
                    wi = Some(r);
                }
            }
            let lam_scale = mu.iter().fold(1.0f64, |m, v| m.max(v.abs()));
            match wi {
                Some(r) if worst < -1e-12 * lam_scale => {
                    work.remove(r);
                }
                _ => {
                    let mut lam_ineq = vec![0.0; a.len()];
                    for (r, &i) in work.iter().enumerate() {
                        lam_ineq[i] = mu[me + r].max(0.0);
                    }
                    return Ok(QpSolution { p, lam_ineq, lam_eq: mu[..me].to_vec(), iterations: it });
                }
            }
            continue;
        }
        // step length with blocking constraints
        let mut alpha = 1.0f64;
        let mut block = None;
        for i in 0..a.len() {
            if work.contains(&i) {
                continue;
            }
            let as_ = dot(&a[i], s);
            if as_ > 1e-14 * pn {
                let t = (b[i] - dot(&a[i], &p)) / as_;
                if t < alpha {
                    alpha = t.max(0.0);
                    block = Some(i);
                }
            }
        }
        for (pj, sj) in p.iter_mut().zip(s) {
            *pj += alpha * sj;
        }
        if let Some(i) = block {
            let mut rows: Vec<&Vec<f64>> = aeq.iter().collect();
            rows.extend(work.iter().map(|&w| &a[w]));
            rows.push(&a[i]);
            if independent(&rows, n) {
                work.push(i);
            } else if alpha == 0.0 {
                // degenerate and dependent: drop a working constraint to make progress (Bland-like: smallest index)
                if !work.is_empty() {
                    let (pos, _) = work.iter().enumerate().min_by_key(|(_, w)| **w).unwrap();
                    work.remove(pos);
                }
                work.push(i);
            }
        }
    }
    Err(QpError::IterationLimit)
}
