//! Real polynomials in descending powers (spec T-5) and root finding.
use crate::lti::{eigenvalues, poly_from_roots, C64};
use aeoi_libm::libm;
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

/// γ_k = k·u/(1 − k·u) (Higham 2002 §3.1).
pub fn gamma(k: usize) -> f64 {
    let ku = k as f64 * (f64::EPSILON / 2.0);
    ku / (1.0 - ku)
}

/// Compensated Horner (Graillat, Langlois & Louvet 2005, "Compensated Horner scheme"): p(x) as accurate
/// as if computed in twice the working precision and then rounded. Returns (value, bound, p̃) with
/// p̃ = Σ|a_i||x|^i and |value − p(x)| ≤ bound, where bound = (u|value| + γ_{2n}²·p̃)/(1 − u)·(1 + 4u)
/// covers the theorem |CompHorner − p(x)| ≤ u|p(x)| + γ_{2n}²·p̃(x) together with the rounding of the
/// bound itself (p̃ is evaluated upward-safe via the factor 1/(1 − γ_{2n})). Valid without underflow or
/// overflow; a non-finite value or bound means overflow. No FMA: bitwise identical on every platform.
pub fn comp_horner(p: &[f64], x: f64) -> (f64, f64, f64) {
    use crate::kernels::sum::{two_prod, two_sum};
    let n = p.len().saturating_sub(1);
    if p.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut s = p[0];
    let mut c = 0.0f64;
    let mut pt = p[0].abs();
    let ax = x.abs();
    for &a in &p[1..] {
        let (pi, pe) = two_prod(s, x);
        let (si, se) = two_sum(pi, a);
        s = si;
        c = c * x + (pe + se);
        pt = pt * ax + a.abs();
    }
    let r = s + c;
    if n == 0 {
        return (r, 0.0, pt);
    }
    let u = f64::EPSILON / 2.0;
    let g = gamma(2 * n);
    let ptu = pt / (1.0 - g);
    let bound = (u * r.abs() + g * g * ptu) / (1.0 - u) * (1.0 + 4.0 * u);
    (r, bound, pt)
}

/// Inclusion discs for the roots of q (degree m ≥ 1, q[0] ≠ 0) from distinct approximations z
/// (Braess & Hadeler 1973; Carstensen 1991): every root of q lies in ∪ D(z_i, m·|W_i|) with the Weierstrass
/// correction W_i = q(z_i) / (q[0]·Π_{j≠i}(z_i − z_j)), and a connected component of k discs contains exactly
/// k roots (with multiplicity). The radii include the rounding of q(z_i) (complex Horner, ≤ γ_{4m+4}·q̃(|z_i|))
/// and of the product. Approximations that coincide exactly (Newton polishing can drive the two halves of a
/// double root onto the same f64) are first split onto a circle of radius max(|c|, 1)·u^(1/k) around their
/// common value c (the natural spread of a k-fold root); the theorem is applied to the split set and each
/// radius is enlarged by the split distance, so the discs around the original z_i stay valid. Returns
/// (radii, componentwise backward errors |q(z_i)|/q̃(|z_i|) at the original z_i).
pub fn inclusion_radii(q: &[f64], z: &[C64]) -> (Vec<f64>, Vec<f64>) {
    let m = q.len() - 1;
    let g = gamma(4 * m + 4);
    let u = f64::EPSILON / 2.0;
    // split exactly coincident approximations (deterministic: angles 2πj/k in order of appearance)
    let mut zs: Vec<C64> = z.to_vec();
    let mut done = vec![false; z.len()];
    for i in 0..z.len() {
        if done[i] {
            continue;
        }
        let grp: Vec<usize> = (i..z.len()).filter(|&j| z[j] == z[i]).collect();
        if grp.len() > 1 {
            let k = grp.len();
            let eps = z[i].norm().max(1.0) * libm::pow(u, 1.0 / k as f64);
            for (t, &j) in grp.iter().enumerate() {
                let th = 2.0 * std::f64::consts::PI * t as f64 / k as f64;
                zs[j] = z[i] + C64::new(eps * libm::cos(th), eps * libm::sin(th));
            }
        }
        for &j in &grp {
            done[j] = true;
        }
    }
    let mut radii = Vec::with_capacity(z.len());
    let mut eta = Vec::with_capacity(z.len());
    for (i, zi) in z.iter().enumerate() {
        let shift = (zs[i] - zi).norm();
        let zi = &zs[i];
        let mut v = C64::new(0.0, 0.0);
        let mut qt = 0.0f64;
        let az = zi.norm();
        for &a in q {
            v = v * zi + a;
            qt = qt * az + a.abs();
        }
        let num = v.norm() + g * qt;
        let mut prod = C64::new(q[0], 0.0);
        for (j, zj) in zs.iter().enumerate() {
            if j != i {
                prod *= zi - zj;
            }
        }
        let den = prod.norm() * (1.0 - g);
        radii.push(if den > 0.0 { (m as f64 * num / den + shift) * (1.0 + 8.0 * u) } else { f64::INFINITY });
        // backward error at the original approximation
        let (mut v0, mut qt0) = (C64::new(0.0, 0.0), 0.0f64);
        let az0 = z[i].norm();
        for &a in q {
            v0 = v0 * z[i] + a;
            qt0 = qt0 * az0 + a.abs();
        }
        eta.push(if qt0 > 0.0 { v0.norm() / qt0 } else { 0.0 });
    }
    (radii, eta)
}

/// Connected components of overlapping discs (|z_i − z_j| ≤ r_i + r_j): component id per disc, ids 0, 1, …
/// in order of first appearance.
pub fn disc_components(z: &[C64], r: &[f64]) -> Vec<usize> {
    let n = z.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            if (z[i] - z[j]).norm() <= r[i] + r[j] {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[b.max(a)] = a.min(b);
                }
            }
        }
    }
    let mut ids = vec![usize::MAX; n];
    let mut next = 0;
    let mut out = vec![0; n];
    for i in 0..n {
        let root = find(&mut parent, i);
        if ids[root] == usize::MAX {
            ids[root] = next;
            next += 1;
        }
        out[i] = ids[root];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comp_horner_beats_horner_near_multiple_root() {
        // (x − 1)^6 expanded at x = 1.001 (x − 1 is exact by Sterbenz, so d^6 is the truth to ~6u)
        let p = [1.0, -6.0, 15.0, -20.0, 15.0, -6.0, 1.0];
        let x = 1.001f64;
        let (r, b, _) = comp_horner(&p, x);
        let truth = (x - 1.0).powi(6);
        assert!((r - truth).abs() <= b + 8.0 * f64::EPSILON * truth, "{r:e} vs {truth:e}, bound {b:e}");
        assert!((r - truth).abs() <= 1e-3 * truth, "{r:e}");
        assert!((polyval_r(&p, x) - truth).abs() > 10.0 * truth);
        assert_eq!(comp_horner(&[3.5], 1e300), (3.5, 0.0, 3.5));
    }
    #[test]
    fn inclusion_discs_contain_roots() {
        let q = [1.0, -6.0, 11.0, -6.0];
        let z = [C64::new(1.0 + 1e-9, 0.0), C64::new(2.0, 1e-10), C64::new(3.0 - 1e-9, 0.0)];
        let (r, _) = inclusion_radii(&q, &z);
        for (zi, w) in z.iter().zip([1.0, 2.0, 3.0]) {
            assert!(r.iter().zip(&z).any(|(ri, zj)| (zj - C64::new(w, 0.0)).norm() <= *ri), "{zi}");
        }
        assert_eq!(disc_components(&z, &r), vec![0, 1, 2]);
        // double root: the two discs overlap
        let q = [1.0, -2.0, 1.0];
        let z = [C64::new(1.0 - 1e-8, 0.0), C64::new(1.0 + 1e-8, 0.0)];
        let (r, _) = inclusion_radii(&q, &z);
        assert_eq!(disc_components(&z, &r), vec![0, 0]);
    }
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
