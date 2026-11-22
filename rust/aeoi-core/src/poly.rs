//! Real polynomials in descending powers (spec T-5) and root finding.
use crate::lti::{eigenvalues, poly_from_roots, C64};
use nalgebra::DMatrix;

/// Remove leading zeros; the zero polynomial is `[0.0]`.
pub fn strip(p: &[f64]) -> Vec<f64> {
    match p.iter().position(|x| *x != 0.0) {
        None => vec![0.0],
        Some(k) => p[k..].to_vec(),
    }
}

/// Degree; -1 for the zero polynomial.
pub fn degree(p: &[f64]) -> isize {
    let s = strip(p);
    if s.len() == 1 && s[0] == 0.0 {
        -1
    } else {
        s.len() as isize - 1
    }
}

pub fn polyval(p: &[f64], s: C64) -> C64 {
    p.iter().fold(C64::new(0.0, 0.0), |acc, &a| acc * s + a)
}

pub fn polyval_r(p: &[f64], x: f64) -> f64 {
    p.iter().fold(0.0, |acc, &a| acc * x + a)
}

pub fn polymul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut r = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            r[i + j] += x * y;
        }
    }
    r
}

/// a + s·b, right-aligned (same power for the last coefficient).
pub fn polyadd_scaled(a: &[f64], b: &[f64], s: f64) -> Vec<f64> {
    let n = a.len().max(b.len());
    let mut r = vec![0.0; n];
    for (k, x) in a.iter().enumerate() {
        r[n - a.len() + k] += x;
    }
    for (k, x) in b.iter().enumerate() {
        r[n - b.len() + k] += s * x;
    }
    r
}

pub fn polyder(p: &[f64]) -> Vec<f64> {
    let n = p.len();
    if n <= 1 {
        return vec![0.0];
    }
    (0..n - 1).map(|k| p[k] * (n - 1 - k) as f64).collect()
}

/// Real polynomial with the given (conjugate-closed) roots, monic.
pub fn poly_real(roots: &[C64]) -> Vec<f64> {
    poly_from_roots(roots).iter().map(|c| c.re).collect()
}

/// Parlett–Reinsch diagonal balancing with powers of 2 (exact similarity, no rounding error).
pub fn balance(a: &mut DMatrix<f64>) {
    let n = a.nrows();
    if !a.iter().all(|x| x.is_finite()) {
        return;
    }
    let (radix, sqrdx) = (2.0f64, 4.0f64);
    for _sweep in 0..100 {
        let mut done = true;
        for i in 0..n {
            let (mut c, mut r) = (0.0, 0.0);
            for j in 0..n {
                if j != i {
                    c += a[(j, i)].abs();
                    r += a[(i, j)].abs();
                }
            }
            if c != 0.0 && r != 0.0 {
                let s = c + r;
                let mut f = 1.0;
                let mut g = r / radix;
                let mut guard = 0;
                while c < g && guard < 2100 {
                    f *= radix;
                    c *= sqrdx;
                    guard += 1;
                }
                g = r * radix;
                while c > g && guard < 4200 {
                    guard += 1;
                    f /= radix;
                    c /= sqrdx;
                }
                if (c + r) / f < 0.95 * s {
                    done = false;
                    for j in 0..n {
                        a[(i, j)] /= f;
                    }
                    for j in 0..n {
                        a[(j, i)] *= f;
                    }
                }
            }
        }
        if done {
            break;
        }
    }
}

fn polish(q: &[f64], dq: &[f64], z0: C64) -> C64 {
    let mut z = z0;
    let mut v = polyval(q, z).norm();
    for _ in 0..4 {
        let d = polyval(dq, z);
        if d.norm() == 0.0 || v == 0.0 {
            break;
        }
        let z1 = z - polyval(q, z) / d;
        let v1 = polyval(q, z1).norm();
        if v1 < v {
            z = z1;
            v = v1;
        } else {
            break;
        }
    }
    z
}

/// Roots of a real polynomial: exact zero roots for trailing zero coefficients, the rest from the
/// eigenvalues of the balanced companion matrix, polished by ≤ 4 Newton steps (accepted only if
/// |p(z)| decreases), conjugate pairs kept exactly conjugate. Order: as produced (callers sort).
pub fn roots(p: &[f64]) -> Vec<C64> {
    let p = strip(p);
    if p.len() <= 1 || !p.iter().all(|x| x.is_finite()) {
        return vec![];
    }
    let nz = p.iter().rev().take_while(|x| **x == 0.0).count();
    let q = &p[..p.len() - nz];
    let n = q.len() - 1;
    let mut out = vec![C64::new(0.0, 0.0); nz];
    if n == 0 {
        return out;
    }
    let mut c = DMatrix::zeros(n, n);
    for j in 0..n {
        c[(0, j)] = -q[j + 1] / q[0];
    }
    for i in 1..n {
        c[(i, i - 1)] = 1.0;
    }
    balance(&mut c);
    let ev = eigenvalues(&c);
    let dq = polyder(q);
    let mut polished = Vec::with_capacity(n);
    for z in &ev {
        if z.im < 0.0 {
            continue;
        }
        if z.im == 0.0 {
            let r = polish(q, &dq, *z);
            polished.push(C64::new(r.re, 0.0));
        } else {
            let mut r = polish(q, &dq, *z);
            if r.im <= 0.0 {
                r = *z;
            }
            polished.push(r);
            polished.push(r.conj());
        }
    }
    if polished.len() == n {
        out.extend(polished);
    } else {
        out.extend(ev);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_basic() {
        let mut r = roots(&[1.0, -6.0, 11.0, -6.0]);
        r.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap());
        for (x, w) in r.iter().zip([1.0, 2.0, 3.0]) {
            assert!((x.re - w).abs() < 1e-13 && x.im == 0.0);
        }
        let r = roots(&[1.0, 2.0, 5.0, 0.0]);
        assert_eq!(r.len(), 3);
        assert!(r.iter().any(|z| z.norm() == 0.0));
        assert!(r.iter().any(|z| (z - C64::new(-1.0, 2.0)).norm() < 1e-14));
        assert!(r.iter().any(|z| (z - C64::new(-1.0, -2.0)).norm() < 1e-14));
        assert!(roots(&[0.0, 3.0]).is_empty());
    }
    #[test]
    fn poly_ops() {
        assert_eq!(polymul(&[1.0, 1.0], &[1.0, -1.0]), vec![1.0, 0.0, -1.0]);
        assert_eq!(polyadd_scaled(&[1.0, 0.0, 0.0], &[1.0, 2.0], -1.0), vec![1.0, -1.0, -2.0]);
        assert_eq!(polyder(&[3.0, 2.0, 1.0]), vec![6.0, 2.0]);
    }
}
