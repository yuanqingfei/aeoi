//! Explicit Runge–Kutta kernel: Dormand–Prince 5(4) following scipy.integrate.RK45 term for term (tableau,
//! RMS error norm, step-size factors 0.9 / 0.2 / 10, Hairer's initial step, Shampine's 4th-order dense
//! output, output points in (t_old, t] evaluated by that step's interpolant), with Hairer's DOPRI5
//! stiffness detection. Shared by ode.ivp.solve and later explicit methods (spec 11.5 RI-1).
use crate::error::OpError;

// Dormand–Prince 5(4): the coefficients of scipy.integrate.RK45 (Dormand & Prince 1980; P from Shampine 1986).
const C: [f64; 6] = [0.0, 1.0 / 5.0, 3.0 / 10.0, 4.0 / 5.0, 8.0 / 9.0, 1.0];
const A: [[f64; 5]; 6] = [
    [0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0 / 5.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0],
    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0],
    [19372.0 / 6561.0, -25360.0 / 2187.0, 64448.0 / 6561.0, -212.0 / 729.0, 0.0],
    [9017.0 / 3168.0, -355.0 / 33.0, 46732.0 / 5247.0, 49.0 / 176.0, -5103.0 / 18656.0],
];
const B: [f64; 6] = [35.0 / 384.0, 0.0, 500.0 / 1113.0, 125.0 / 192.0, -2187.0 / 6784.0, 11.0 / 84.0];
const E: [f64; 7] = [-71.0 / 57600.0, 0.0, 71.0 / 16695.0, -71.0 / 1920.0, 17253.0 / 339200.0, -22.0 / 525.0, 1.0 / 40.0];
const P: [[f64; 4]; 7] = [
    [1.0, -8048581381.0 / 2820520608.0, 8663915743.0 / 2820520608.0, -12715105075.0 / 11282082432.0],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 131558114200.0 / 32700410799.0, -68118460800.0 / 10900136933.0, 87487479700.0 / 32700410799.0],
    [0.0, -1754552775.0 / 470086768.0, 14199869525.0 / 1410260304.0, -10690763975.0 / 1880347072.0],
    [0.0, 127303824393.0 / 49829197408.0, -318862633887.0 / 49829197408.0, 701980252875.0 / 199316789632.0],
    [0.0, -282668133.0 / 205662961.0, 2019193451.0 / 616988883.0, -1453857185.0 / 822651844.0],
    [0.0, 40617522.0 / 29380423.0, -110615467.0 / 29380423.0, 69997945.0 / 29380423.0],
];
const SAFETY: f64 = 0.9;
const MIN_FACTOR: f64 = 0.2;
const MAX_FACTOR: f64 = 10.0;
const ERR_EXP: f64 = -1.0 / 5.0;

/// Python's max(a, b) / min of a list: NaN comparisons are false, so the earlier operand wins.
fn pymax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}
fn pymin(v: &[f64]) -> f64 {
    v.iter().skip(1).fold(v[0], |m, &x| if x < m { x } else { m })
}
fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>()).sqrt() / (v.len() as f64).sqrt()
}

#[derive(Debug)]
pub enum Fail {
    NonFiniteAtStart(f64),
    StepTooSmall { t: f64, h: f64, last_non_finite: bool },
    StepLimit { t: f64, steps: usize },
    Cancelled(OpError),
}

pub struct Solution {
    pub y: Vec<Vec<f64>>, // [n_out][n]
    pub n_steps: usize,
    pub n_rejected: usize,
    pub n_rhs: usize,
    pub stiff_at: Option<f64>,
}

/// Dormand–Prince 5(4) for dy/dt = f(t, y); `f_rhs(t, y, out)` returns false if any component is not finite.
#[allow(clippy::too_many_arguments)]
pub fn dopri5(f_rhs: &mut dyn FnMut(f64, &[f64], &mut [f64]) -> bool, n: usize, y0: &[f64], t0: f64, tf: f64, t_eval: &[f64], rtol: f64, atol: f64, max_step: f64, first_step: Option<f64>, max_steps: usize, check: &dyn Fn() -> Result<(), OpError>) -> Result<Solution, Fail> {
    let dir = if tf > t0 { 1.0 } else { -1.0 };
    let mut nfev = 0usize;
    let mut f = vec![0.0; n];
    nfev += 1;
    if !f_rhs(t0, y0, &mut f) {
        return Err(Fail::NonFiniteAtStart(t0));
    }
    // initial step (Hairer–Nørsett–Wanner II.4, scipy select_initial_step, order = 4)
    let mut h_abs = match first_step {
        Some(h) => h,
        None => {
            let interval = (tf - t0).abs();
            let scale: Vec<f64> = y0.iter().map(|v| atol + v.abs() * rtol).collect();
            let d0 = rms(&y0.iter().zip(&scale).map(|(a, s)| a / s).collect::<Vec<_>>());
            let d1 = rms(&f.iter().zip(&scale).map(|(a, s)| a / s).collect::<Vec<_>>());
            let mut h0 = if d0 < 1e-5 || d1 < 1e-5 { 1e-6 } else { 0.01 * d0 / d1 };
            h0 = pymin(&[h0, interval]);
            let y1: Vec<f64> = y0.iter().zip(&f).map(|(a, b)| a + h0 * dir * b).collect();
            let mut f1 = vec![0.0; n];
            f_rhs(t0 + h0 * dir, &y1, &mut f1);
            nfev += 1;
            let d2 = rms(&f1.iter().zip(&f).zip(&scale).map(|((a, b), s)| (a - b) / s).collect::<Vec<_>>()) / h0;
            let h1 = if d1 <= 1e-15 && d2 <= 1e-15 { pymax(1e-6, h0 * 1e-3) } else { (0.01 / pymax(d1, d2)).powf(1.0 / 5.0) };
            pymin(&[100.0 * h0, h1, interval, max_step])
        }
    };
    let mut t = t0;
    let mut y = y0.to_vec();
    let mut k = vec![vec![0.0; n]; 7];
    let mut out: Vec<Vec<f64>> = Vec::with_capacity(t_eval.len());
    let mut ie = 0usize;
    let (mut n_steps, mut n_rej) = (0usize, 0usize);
    let (mut iasti, mut nonsti, mut stiff_at) = (0usize, 0usize, None);
    let mut ystage = vec![0.0; n];
    let mut ynew = vec![0.0; n];
    while dir * (t - tf) < 0.0 {
        let min_step = 10.0 * (next_toward(t, dir) - t).abs();
        h_abs = if h_abs > max_step { max_step } else if h_abs < min_step { min_step } else { h_abs };
        let (mut accepted, mut rejected) = (false, false);
        let (mut h, mut t_new) = (0.0, t);
        let mut last_non_finite = false;
        while !accepted {
            if h_abs < min_step {
                return Err(Fail::StepTooSmall { t, h: h_abs, last_non_finite });
            }
            h = h_abs * dir;
            t_new = t + h;
            if dir * (t_new - tf) > 0.0 {
                t_new = tf;
            }
            h = t_new - t;
            h_abs = h.abs();
            // stages
            k[0].copy_from_slice(&f);
            let mut finite = true;
            for s in 1..6 {
                for i in 0..n {
                    let mut dot = 0.0;
                    for j in 0..s {
                        dot += k[j][i] * A[s][j];
                    }
                    ystage[i] = y[i] + dot * h;
                }
                finite &= f_rhs(t + C[s] * h, &ystage, &mut k[s]);
                nfev += 1;
            }
            for i in 0..n {
                let mut dot = 0.0;
                for j in 0..6 {
                    dot += k[j][i] * B[j];
                }
                ynew[i] = y[i] + h * dot;
            }
            finite &= f_rhs(t + h, &ynew, &mut k[6]);
            nfev += 1;
            let err = if finite {
                let e: Vec<f64> = (0..n)
                    .map(|i| {
                        let mut dot = 0.0;
                        for j in 0..7 {
                            dot += k[j][i] * E[j];
                        }
                        dot * h / (atol + pymax(y[i].abs(), ynew[i].abs()) * rtol)
                    })
                    .collect();
                rms(&e)
            } else {
                f64::NAN
            };
            last_non_finite = !finite;
            if err < 1.0 {
                let mut factor = if err == 0.0 { MAX_FACTOR } else { pymin(&[MAX_FACTOR, SAFETY * err.powf(ERR_EXP)]) };
                if rejected {
                    factor = pymin(&[1.0, factor]);
                }
                h_abs *= factor;
                accepted = true;
            } else {
                h_abs *= pymax(MIN_FACTOR, SAFETY * err.powf(ERR_EXP));
                rejected = true;
                n_rej += 1;
            }
            if n_steps + n_rej > max_steps {
                return Err(Fail::StepLimit { t, steps: n_steps + n_rej });
            }
        }
        n_steps += 1;
        // stiffness detection (Hairer DOPRI5): every 1000 accepted steps, or while suspicious
        if n_steps % 1000 == 0 || iasti > 0 {
            let stnum: f64 = (0..n).map(|i| (k[6][i] - k[5][i]).powi(2)).sum();
            let stden: f64 = (0..n).map(|i| (ynew[i] - ystage[i]).powi(2)).sum();
            if stden > 0.0 {
                let hlamb = h.abs() * (stnum / stden).sqrt();
                if hlamb > 3.25 {
                    nonsti = 0;
                    iasti += 1;
                    if iasti == 15 && stiff_at.is_none() {
                        stiff_at = Some(t_new);
                    }
                } else {
                    nonsti += 1;
                    if nonsti == 6 {
                        iasti = 0;
                    }
                }
            }
        }
        // output points in (t_old, t_new] (and t0 itself on the first step): this step's dense output
        if ie < t_eval.len() && dir * (t_eval[ie] - t_new) <= 0.0 {
            let q: Vec<[f64; 4]> = (0..n)
                .map(|i| {
                    let mut r = [0.0; 4];
                    for (c, rc) in r.iter_mut().enumerate() {
                        let mut dot = 0.0;
                        for j in 0..7 {
                            dot += k[j][i] * P[j][c];
                        }
                        *rc = dot;
                    }
                    r
                })
                .collect();
            while ie < t_eval.len() && dir * (t_eval[ie] - t_new) <= 0.0 {
                // p = cumprod([x, x, x, x]) = [x, x², x³, x⁴], each power from the previous one
                let x = (t_eval[ie] - t) / h;
                let p2 = x * x;
                let p3 = p2 * x;
                let p = [x, p2, p3, p3 * x];
                out.push((0..n).map(|i| h * (q[i][0] * p[0] + q[i][1] * p[1] + q[i][2] * p[2] + q[i][3] * p[3]) + y[i]).collect());
                ie += 1;
            }
        }
        t = t_new;
        std::mem::swap(&mut y, &mut ynew);
        f.copy_from_slice(&k[6]);
        if n_steps % 256 == 0 {
            check().map_err(Fail::Cancelled)?;
        }
    }
    Ok(Solution { y: out, n_steps, n_rejected: n_rej, n_rhs: nfev, stiff_at })
}

/// nextafter(t, dir·∞)
pub fn next_toward(t: f64, dir: f64) -> f64 {
    if t.is_nan() {
        return t;
    }
    let bits = t.to_bits();
    let up = dir > 0.0;
    if t == 0.0 {
        return if up { f64::from_bits(1) } else { -f64::from_bits(1) };
    }
    let next = if (t > 0.0) == up { bits + 1 } else { bits - 1 };
    f64::from_bits(next)
}

