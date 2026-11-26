//! Real Schur decomposition A = Q·T·Qᵀ (v0.4.16, replaces nalgebra's Schur): Householder reduction to upper
//! Hessenberg form, then the implicit Francis double-shift QR iteration (Golub–Van Loan Alg. 7.5.1/7.5.2,
//! EISPACK hqr2 structure) with the full matrix updated so that T and Q stay consistent, deflation when
//! |h_{k,k−1}| ≤ ε·(|h_{k−1,k−1}| + |h_{k,k}|), and exceptional shifts after 10 and 20 iterations without
//! deflation (as EISPACK / LAPACK dlahqr). T is quasi-upper-triangular: 1×1 blocks and 2×2 blocks.
//! linalg.decomp.eig's kernel and every eigenvalue computation (poles, polynomial roots, Lyapunov, …) use it
//! (spec 11.5 RI-1).

/// Returns (Q, T) row-major n×n, or None if the iteration budget (30 iterations per eigenvalue on average,
/// at least 100) is exhausted.
pub fn real_schur(a: &[f64], n: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    let mut h = a.to_vec();
    let mut q = vec![0.0; n * n];
    for i in 0..n {
        q[i * n + i] = 1.0;
    }
    if n == 0 {
        return Some((q, h));
    }
    // ---- Hessenberg reduction
    for k in 0..n.saturating_sub(2) {
        let xnorm = (k + 2..n).map(|i| h[i * n + k] * h[i * n + k]).sum::<f64>();
        if xnorm == 0.0 {
            continue;
        }
        let alpha = h[(k + 1) * n + k];
        let norm = alpha.hypot(xnorm.sqrt());
        let beta = -norm.copysign(alpha);
        let mut v = vec![0.0; n];
        v[k + 1] = alpha - beta;
        for i in k + 2..n {
            v[i] = h[i * n + k];
        }
        let vtv: f64 = v[k + 1..].iter().map(|x| x * x).sum();
        let tau = 2.0 / vtv;
        // H ← P H P, Q ← Q P (P = I − τ v vᵀ, v supported on k+1..n)
        for j in 0..n {
            let s: f64 = (k + 1..n).map(|i| v[i] * h[i * n + j]).sum::<f64>() * tau;
            for i in k + 1..n {
                h[i * n + j] -= s * v[i];
            }
        }
        for i in 0..n {
            let s: f64 = (k + 1..n).map(|j| h[i * n + j] * v[j]).sum::<f64>() * tau;
            for j in k + 1..n {
                h[i * n + j] -= s * v[j];
            }
            let s: f64 = (k + 1..n).map(|j| q[i * n + j] * v[j]).sum::<f64>() * tau;
            for j in k + 1..n {
                q[i * n + j] -= s * v[j];
            }
        }
        h[(k + 1) * n + k] = beta;
        for i in k + 2..n {
            h[i * n + k] = 0.0;
        }
    }
    let hnorm = h.iter().map(|x| x * x).sum::<f64>().sqrt().max(f64::MIN_POSITIVE);
    let eps = f64::EPSILON;
    let budget = (30 * n).max(100);
    let mut total = 0usize;
    let mut hi = n - 1;
    let mut its = 0usize;
    // apply a reflector I − τ·v·vᵀ (v of length 2 or 3, starting at row/column k) to the full matrices
    let apply = |h: &mut Vec<f64>, q: &mut Vec<f64>, k: usize, v: &[f64], tau: f64, col_from: usize, row_to: usize| {
        let m = v.len();
        for j in col_from..n {
            let s = (0..m).map(|r| v[r] * h[(k + r) * n + j]).sum::<f64>() * tau;
            for r in 0..m {
                h[(k + r) * n + j] -= s * v[r];
            }
        }
        for i in 0..=row_to {
            let s = (0..m).map(|r| h[i * n + k + r] * v[r]).sum::<f64>() * tau;
            for r in 0..m {
                h[i * n + k + r] -= s * v[r];
            }
        }
        for i in 0..n {
            let s = (0..m).map(|r| q[i * n + k + r] * v[r]).sum::<f64>() * tau;
            for r in 0..m {
                q[i * n + k + r] -= s * v[r];
            }
        }
    };
    let reflector = |x: &[f64]| -> Option<(Vec<f64>, f64)> {
        let norm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm == 0.0 || x[1..].iter().all(|v| *v == 0.0) {
            return None;
        }
        let beta = -norm.copysign(x[0]);
        let mut v = x.to_vec();
        v[0] -= beta;
        let vtv: f64 = v.iter().map(|t| t * t).sum();
        Some((v, 2.0 / vtv))
    };
    while hi > 0 {
        // deflation: find the start lo of the active unreduced block ending at hi
        let mut lo = 0;
        for k in (1..=hi).rev() {
            let mut s = h[(k - 1) * n + k - 1].abs() + h[k * n + k].abs();
            if s == 0.0 {
                s = hnorm;
            }
            if h[k * n + k - 1].abs() <= eps * s {
                h[k * n + k - 1] = 0.0;
                lo = k;
                break;
            }
        }
        if lo == hi {
            hi -= 1;
            its = 0;
            continue;
        }
        if lo + 1 == hi {
            if hi < 2 {
                break;
            }
            hi -= 2;
            its = 0;
            continue;
        }
        its += 1;
        total += 1;
        if total > budget {
            return None;
        }
        // shifts: eigenvalues of the trailing 2×2 (s = sum, t = product), exceptional after 10 / 20 iterations
        let (mut s, mut t) = {
            let (a11, a12, a21, a22) = (h[(hi - 1) * n + hi - 1], h[(hi - 1) * n + hi], h[hi * n + hi - 1], h[hi * n + hi]);
            (a11 + a22, a11 * a22 - a12 * a21)
        };
        if its == 10 || its == 20 {
            let ss = h[hi * n + hi - 1].abs() + h[(hi - 1) * n + hi - 2].abs();
            s = 1.5 * ss;
            t = ss * ss;
        }
        let h00 = h[lo * n + lo];
        let h10 = h[(lo + 1) * n + lo];
        let mut x = h00 * h00 + h[lo * n + lo + 1] * h10 - s * h00 + t;
        let mut y = h10 * (h00 + h[(lo + 1) * n + lo + 1] - s);
        let mut z = h10 * h[(lo + 2) * n + lo + 1];
        for k in lo..=hi - 2 {
            if let Some((v, tau)) = reflector(&[x, y, z]) {
                let col_from = if k > lo { k - 1 } else { lo };
                apply(&mut h, &mut q, k, &v, tau, col_from, (k + 3).min(hi));
                if k > lo {
                    h[(k + 1) * n + k - 1] = 0.0;
                    h[(k + 2) * n + k - 1] = 0.0;
                }
            }
            x = h[(k + 1) * n + k];
            y = h[(k + 2) * n + k];
            if k + 3 <= hi {
                z = h[(k + 3) * n + k];
            }
        }
        if let Some((v, tau)) = reflector(&[x, y]) {
            let k = hi - 1;
            apply(&mut h, &mut q, k, &v, tau, k - 1, hi);
            h[hi * n + k - 1] = 0.0;
        }
    }
    // standardise 2×2 blocks with real eigenvalues into upper-triangular form (LAPACK dlanv2, real case)
    let mut i = 0;
    while i + 1 < n {
        let c = h[(i + 1) * n + i];
        if c == 0.0 {
            i += 1;
            continue;
        }
        let (a, bb, d) = (h[i * n + i], h[i * n + i + 1], h[(i + 1) * n + i + 1]);
        let p = 0.5 * (a - d);
        let bcmax = bb.abs().max(c.abs());
        let bcmis = bb.abs().min(c.abs()) * 1f64.copysign(bb) * 1f64.copysign(c);
        let scale = p.abs().max(bcmax);
        let zz = p / scale * p + bcmax / scale * bcmis;
        if zz >= 4.0 * eps {
            let z = p + (scale.sqrt() * zz.sqrt()).copysign(p);
            let a_new = d + z;
            let d_new = d - bcmax / z * bcmis;
            let tau = c.hypot(z);
            let (cs, sn) = (z / tau, c / tau);
            // T ← Gᵀ T G, Q ← Q G with G = [cs −sn; sn cs], rows/columns i, i+1
            for j in 0..n {
                let (r1, r2) = (h[i * n + j], h[(i + 1) * n + j]);
                h[i * n + j] = cs * r1 + sn * r2;
                h[(i + 1) * n + j] = -sn * r1 + cs * r2;
            }
            for r in 0..n {
                let (c1, c2) = (h[r * n + i], h[r * n + i + 1]);
                h[r * n + i] = c1 * cs + c2 * sn;
                h[r * n + i + 1] = -c1 * sn + c2 * cs;
                let (q1, q2) = (q[r * n + i], q[r * n + i + 1]);
                q[r * n + i] = q1 * cs + q2 * sn;
                q[r * n + i + 1] = -q1 * sn + q2 * cs;
            }
            h[i * n + i] = a_new;
            h[(i + 1) * n + i + 1] = d_new;
            h[i * n + i + 1] = bb - c;
            h[(i + 1) * n + i] = 0.0;
        }
        i += 2;
    }
    // clean below the subdiagonal
    for i in 0..n {
        for j in 0..i.saturating_sub(1) {
            h[i * n + j] = 0.0;
        }
    }
    Some((q, h))
}
