//! Linear programming kernel (spec 10.28): two-phase dense tableau simplex with Bland's rule (no cycling,
//! deterministic), on the standard form built from MATLAB-style data
//!   min cᵀx  s.t.  A x ≤ b,  Aeq x = beq,  lb ≤ x ≤ ub   (bounds may be ±∞; default free, as MATLAB).
//! The final basic solution and the row duals are recomputed with the shared LU kernel (spec 11.5 RI-1);
//! multipliers follow MATLAB's sign convention: c + Aᵀλ_ineq + Aeqᵀλ_eq − λ_lower + λ_upper = 0, all λ ≥ 0
//! except λ_eq. optim.linear.linprog is built on this; the SQP of fmincon reuses it for feasibility.
use crate::kernels::dense::lu_factor;

pub struct LpProblem<'a> {
    pub c: &'a [f64],
    pub a: &'a [Vec<f64>],
    pub b: &'a [f64],
    pub aeq: &'a [Vec<f64>],
    pub beq: &'a [f64],
    pub lb: &'a [f64],
    pub ub: &'a [f64],
}

#[derive(Debug)]
pub enum LpError {
    Infeasible { violation: f64 },
    Unbounded { column: usize },
    IterationLimit,
}

pub struct LpSolution {
    pub x: Vec<f64>,
    pub fval: f64,
    pub ineqlin: Vec<f64>,
    pub eqlin: Vec<f64>,
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
    pub iterations: usize,
}

enum VarMap {
    Shift(f64),       // x = l + y
    Reflect(f64),     // x = u − y
    Split(usize),     // x = y − y[other]
}

pub fn solve(p: &LpProblem, max_iter: usize) -> Result<LpSolution, LpError> {
    let n = p.c.len();
    // ---- standard form columns
    let mut maps = Vec::with_capacity(n);
    let mut ncol = n;
    for j in 0..n {
        let (l, u) = (p.lb[j], p.ub[j]);
        maps.push(if l.is_finite() {
            VarMap::Shift(l)
        } else if u.is_finite() {
            VarMap::Reflect(u)
        } else {
            ncol += 1;
            VarMap::Split(ncol - 1)
        });
    }
    // rows: (coeffs over original x, rhs, kind) kind 0 = ≤, 1 = =, 2 = bound row on column j (y_j ≤ u − l)
    let mut rows: Vec<(Vec<f64>, f64, u8)> = Vec::new();
    for (r, bi) in p.a.iter().zip(p.b) {
        rows.push((r.clone(), *bi, 0));
    }
    for (r, bi) in p.aeq.iter().zip(p.beq) {
        rows.push((r.clone(), *bi, 1));
    }
    let n_ub = p.a.len();
    let n_eq = p.aeq.len();
    for j in 0..n {
        if p.lb[j].is_finite() && p.ub[j].is_finite() {
            let mut r = vec![0.0; n];
            r[j] = 1.0;
            rows.push((r, p.ub[j], 2));
        }
    }
    let m = rows.len();
    let n_slack = rows.iter().filter(|r| r.2 != 1).count();
    let nv = ncol + n_slack; // structural + slacks
    // M y = r in standard variables
    let mut mm = vec![vec![0.0; nv]; m];
    let mut rhs = vec![0.0; m];
    let mut cc = vec![0.0; nv];
    let mut c0 = 0.0;
    for j in 0..n {
        match maps[j] {
            VarMap::Shift(l) => { cc[j] = p.c[j]; c0 += p.c[j] * l; }
            VarMap::Reflect(u) => { cc[j] = -p.c[j]; c0 += p.c[j] * u; }
            VarMap::Split(k) => { cc[j] = p.c[j]; cc[k] = -p.c[j]; }
        }
    }
    let mut s = ncol;
    let mut slack_of_row = vec![usize::MAX; m];
    for (i, (coef, bi, kind)) in rows.iter().enumerate() {
        let mut r = *bi;
        for j in 0..n {
            let a = coef[j];
            if a == 0.0 {
                continue;
            }
            match maps[j] {
                VarMap::Shift(l) => { mm[i][j] += a; if *kind != 2 { r -= a * l; } }
                VarMap::Reflect(u) => { mm[i][j] -= a; r -= a * u; }
                VarMap::Split(k) => { mm[i][j] += a; mm[i][k] -= a; }
            }
        }
        if *kind == 2 {
            if let VarMap::Shift(l) = maps[coef.iter().position(|v| *v == 1.0).unwrap()] {
                r -= l;
            }
        }
        if *kind != 1 {
            mm[i][s] = 1.0;
            slack_of_row[i] = s;
            s += 1;
        }
        rhs[i] = r;
    }
    // row signs so that rhs ≥ 0
    let mut sigma = vec![1.0; m];
    for i in 0..m {
        if rhs[i] < 0.0 {
            sigma[i] = -1.0;
            rhs[i] = -rhs[i];
            for v in mm[i].iter_mut() {
                *v = -*v;
            }
        }
    }
    // ---- tableau with artificials (columns nv .. nv+m)
    let w = nv + m;
    let mut t = vec![vec![0.0; w + 1]; m];
    for i in 0..m {
        t[i][..nv].copy_from_slice(&mm[i]);
        t[i][nv + i] = 1.0;
        t[i][w] = rhs[i];
    }
    let mut basis: Vec<usize> = (nv..nv + m).collect();
    let scale = rhs.iter().fold(1.0f64, |a, v| a.max(v.abs()));
    let tol = 1e-9;
    let mut iters = 0usize;
    let run = |t: &mut Vec<Vec<f64>>, basis: &mut Vec<usize>, cost: &[f64], allowed: usize, iters: &mut usize| -> Result<(), LpError> {
        loop {
            // reduced costs d_j = cost_j − c_Bᵀ t[:, j]
            let mut enter = None;
            for j in 0..allowed {
                if basis.contains(&j) {
                    continue;
                }
                let d = cost[j] - (0..t.len()).map(|i| cost[basis[i]] * t[i][j]).sum::<f64>();
                if d < -tol * (1.0 + cost[j].abs()) {
                    enter = Some(j);
                    break; // Bland: smallest index
                }
            }
            let Some(q) = enter else { return Ok(()) };
            let mut leave: Option<(usize, f64)> = None;
            for i in 0..t.len() {
                if t[i][q] > tol {
                    let ratio = t[i][w] / t[i][q];
                    match leave {
                        None => leave = Some((i, ratio)),
                        Some((li, lr)) => {
                            if ratio < lr - 1e-12 * lr.abs().max(1.0) || (ratio <= lr + 1e-12 * lr.abs().max(1.0) && basis[i] < basis[li]) {
                                leave = Some((i, ratio));
                            }
                        }
                    }
                }
            }
            let Some((r, _)) = leave else { return Err(LpError::Unbounded { column: q }) };
            let piv = t[r][q];
            for v in t[r].iter_mut() {
                *v /= piv;
            }
            let pr = t[r].clone();
            for (i, row) in t.iter_mut().enumerate() {
                if i != r && row[q] != 0.0 {
                    let f = row[q];
                    for (v, pv) in row.iter_mut().zip(&pr) {
                        *v -= f * pv;
                    }
                }
            }
            basis[r] = q;
            *iters += 1;
            if *iters > max_iter {
                return Err(LpError::IterationLimit);
            }
        }
    };
    // phase I
    let mut cost1 = vec![0.0; w];
    for v in cost1[nv..].iter_mut() {
        *v = 1.0;
    }
    run(&mut t, &mut basis, &cost1, w, &mut iters).map_err(|e| match e { LpError::Unbounded { .. } => LpError::IterationLimit, e => e })?;
    let infeas: f64 = (0..m).filter(|&i| basis[i] >= nv).map(|i| t[i][w]).sum();
    if infeas > 1e-9 * scale {
        return Err(LpError::Infeasible { violation: infeas });
    }
    // drive artificials out; drop redundant rows
    let mut keep = vec![true; m];
    for i in 0..m {
        if basis[i] >= nv {
            if let Some(q) = (0..nv).find(|&j| t[i][j].abs() > 1e-9 && !basis.contains(&j)) {
                let piv = t[i][q];
                for v in t[i].iter_mut() {
                    *v /= piv;
                }
                let pr = t[i].clone();
                for (k, row) in t.iter_mut().enumerate() {
                    if k != i && row[q] != 0.0 {
                        let f = row[q];
                        for (v, pv) in row.iter_mut().zip(&pr) {
                            *v -= f * pv;
                        }
                    }
                }
                basis[i] = q;
            } else {
                keep[i] = false;
            }
        }
    }
    let mut t2: Vec<Vec<f64>> = Vec::new();
    let mut b2 = Vec::new();
    let mut rows_kept = Vec::new();
    for i in 0..m {
        if keep[i] {
            t2.push(t[i].clone());
            b2.push(basis[i]);
            rows_kept.push(i);
        }
    }
    let mut cost2 = cc.clone();
    cost2.extend(std::iter::repeat(0.0).take(m));
    run(&mut t2, &mut b2, &cost2, nv, &mut iters)?;
    // ---- recompute basic solution and duals with the LU kernel on the kept rows
    let mk = rows_kept.len();
    let mut bmat = vec![0.0; mk * mk];
    for (r, &i) in rows_kept.iter().enumerate() {
        for (k, &j) in b2.iter().enumerate() {
            bmat[r * mk + k] = mm[i][j];
        }
    }
    let lu = lu_factor(&bmat, mk);
    let mut y = vec![0.0; nv];
    let mut pi = vec![0.0; m];
    if mk > 0 && lu.zero_pivot.is_none() {
        let xb = lu.solve(&rows_kept.iter().map(|&i| rhs[i]).collect::<Vec<_>>());
        for (k, &j) in b2.iter().enumerate() {
            y[j] = xb[k].max(0.0);
        }
        let yb = lu.solve_t(&b2.iter().map(|&j| cc[j]).collect::<Vec<_>>());
        for (r, &i) in rows_kept.iter().enumerate() {
            pi[i] = sigma[i] * yb[r];
        }
    } else {
        for (r, &j) in b2.iter().enumerate() {
            y[j] = t2[r][w].max(0.0);
        }
    }
    let x: Vec<f64> = (0..n).map(|j| match maps[j] {
        VarMap::Shift(l) => l + y[j],
        VarMap::Reflect(u) => u - y[j],
        VarMap::Split(k) => y[j] - y[k],
    }).collect();
    let _ = slack_of_row;
    let ineqlin: Vec<f64> = (0..n_ub).map(|i| (-pi[i]).max(0.0)).collect();
    let eqlin: Vec<f64> = (0..n_eq).map(|i| -pi[n_ub + i]).collect();
    let mut lower = vec![0.0; n];
    let mut upper = vec![0.0; n];
    for j in 0..n {
        let r = p.c[j] + (0..n_ub).map(|i| p.a[i][j] * ineqlin[i]).sum::<f64>() + (0..n_eq).map(|i| p.aeq[i][j] * eqlin[i]).sum::<f64>();
        if r > 0.0 && p.lb[j].is_finite() {
            lower[j] = r;
        } else if r < 0.0 && p.ub[j].is_finite() {
            upper[j] = -r;
        }
    }
    let fval = p.c.iter().zip(&x).map(|(a, b)| a * b).sum::<f64>();
    let _ = c0;
    Ok(LpSolution { x, fval, ineqlin, eqlin, lower, upper, iterations: iters })
}

/// KKT residuals of an LP solution, relative to the data's scale: (primal infeasibility, dual infeasibility,
/// complementarity, stationarity).
pub fn kkt(p: &LpProblem, s: &LpSolution) -> (f64, f64, f64, f64) {
    let n = p.c.len();
    let sc = |v: f64| v.abs().max(1.0);
    let mut primal = 0.0f64;
    let mut comp = 0.0f64;
    for (i, (row, bi)) in p.a.iter().zip(p.b).enumerate() {
        let ax: f64 = row.iter().zip(&s.x).map(|(a, x)| a * x).sum();
        primal = primal.max((ax - bi).max(0.0) / sc(*bi));
        comp = comp.max((s.ineqlin[i] * (bi - ax)).abs() / sc(*bi));
    }
    for (row, bi) in p.aeq.iter().zip(p.beq) {
        let ax: f64 = row.iter().zip(&s.x).map(|(a, x)| a * x).sum();
        primal = primal.max((ax - bi).abs() / sc(*bi));
    }
    let mut stat = 0.0f64;
    let mut dual = 0.0f64;
    for j in 0..n {
        if p.lb[j].is_finite() {
            primal = primal.max((p.lb[j] - s.x[j]).max(0.0) / sc(p.lb[j]));
            comp = comp.max((s.lower[j] * (s.x[j] - p.lb[j])).abs() / sc(p.lb[j]));
        }
        if p.ub[j].is_finite() {
            primal = primal.max((s.x[j] - p.ub[j]).max(0.0) / sc(p.ub[j]));
            comp = comp.max((s.upper[j] * (p.ub[j] - s.x[j])).abs() / sc(p.ub[j]));
        }
        let r = p.c[j] + (0..p.a.len()).map(|i| p.a[i][j] * s.ineqlin[i]).sum::<f64>() + (0..p.aeq.len()).map(|i| p.aeq[i][j] * s.eqlin[i]).sum::<f64>() - s.lower[j] + s.upper[j];
        stat = stat.max(r.abs() / sc(p.c[j]));
    }
    for v in s.ineqlin.iter().chain(&s.lower).chain(&s.upper) {
        dual = dual.max((-v).max(0.0));
    }
    (primal, dual, comp, stat)
}
