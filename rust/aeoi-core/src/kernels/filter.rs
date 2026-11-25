//! IIR filter kernels (spec 10.24–10.25): Butterworth prototype, zpk frequency transforms (scipy lp2*_zpk
//! conventions), bilinear transform, second-order-section pairing, frequency response, and the transposed
//! direct form II recursion (MATLAB filter / scipy lfilter). signal.design.butter and signal.filter.filter
//! are built on these; freqz / filtfilt / other designs reuse them (spec 11.5 RI-1). Polynomial expansion
//! and root finding use crate::poly (the same code as control.lti.convert / poles_zeros).
use crate::lti::C64;
use crate::poly::poly_real;

pub struct Zpk {
    pub z: Vec<C64>,
    pub p: Vec<C64>,
    pub k: f64,
}

fn c(re: f64) -> C64 {
    C64::new(re, 0.0)
}
fn prod(v: &[C64]) -> C64 {
    v.iter().fold(c(1.0), |a, b| a * b)
}

/// Analog Butterworth prototype (cutoff 1 rad/s): p = −exp(iπm/(2n)), m = −n+1, −n+3, …, n−1 (scipy buttap).
pub fn buttap(n: usize) -> Zpk {
    let p = (0..n)
        .map(|j| {
            let m = -(n as f64) + 1.0 + 2.0 * j as f64;
            let th = std::f64::consts::PI * m / (2.0 * n as f64);
            -C64::new(th.cos(), th.sin())
        })
        .collect();
    Zpk { z: vec![], p, k: 1.0 }
}

pub fn lp2lp(f: &Zpk, wo: f64) -> Zpk {
    let deg = (f.p.len() - f.z.len()) as i32;
    Zpk { z: f.z.iter().map(|z| z * wo).collect(), p: f.p.iter().map(|p| p * wo).collect(), k: f.k * wo.powi(deg) }
}

pub fn lp2hp(f: &Zpk, wo: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let mut z: Vec<C64> = f.z.iter().map(|z| c(wo) / z).collect();
    let p = f.p.iter().map(|p| c(wo) / p).collect();
    z.extend(std::iter::repeat(c(0.0)).take(deg));
    let nz: Vec<C64> = f.z.iter().map(|z| -z).collect();
    let np: Vec<C64> = f.p.iter().map(|p| -p).collect();
    Zpk { z, p, k: f.k * (prod(&nz) / prod(&np)).re }
}

fn bp_roots(v: &[C64], bw: f64, wo: f64) -> Vec<C64> {
    let l: Vec<C64> = v.iter().map(|x| x * (bw / 2.0)).collect();
    let mut out: Vec<C64> = l.iter().map(|x| x + (x * x - wo * wo).sqrt()).collect();
    out.extend(l.iter().map(|x| x - (x * x - wo * wo).sqrt()));
    out
}

pub fn lp2bp(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let mut z = bp_roots(&f.z, bw, wo);
    z.extend(std::iter::repeat(c(0.0)).take(deg));
    Zpk { z, p: bp_roots(&f.p, bw, wo), k: f.k * bw.powi(deg as i32) }
}

pub fn lp2bs(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let inv = |v: &[C64]| -> Vec<C64> { v.iter().map(|x| c(bw / 2.0) / x).collect() };
    let split = |h: Vec<C64>| -> Vec<C64> {
        let mut out: Vec<C64> = h.iter().map(|x| x + (x * x - wo * wo).sqrt()).collect();
        out.extend(h.iter().map(|x| x - (x * x - wo * wo).sqrt()));
        out
    };
    let mut z = split(inv(&f.z));
    z.extend(std::iter::repeat(C64::new(0.0, wo)).take(deg));
    z.extend(std::iter::repeat(C64::new(0.0, -wo)).take(deg));
    let nz: Vec<C64> = f.z.iter().map(|z| -z).collect();
    let np: Vec<C64> = f.p.iter().map(|p| -p).collect();
    Zpk { z, p: split(inv(&f.p)), k: f.k * (prod(&nz) / prod(&np)).re }
}

/// Bilinear transform s = 2·fs·(z − 1)/(z + 1) (scipy bilinear_zpk).
pub fn bilinear(f: &Zpk, fs: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let fs2 = c(2.0 * fs);
    let mut z: Vec<C64> = f.z.iter().map(|z| (fs2 + z) / (fs2 - z)).collect();
    let p = f.p.iter().map(|p| (fs2 + p) / (fs2 - p)).collect();
    z.extend(std::iter::repeat(c(-1.0)).take(deg));
    let nz: Vec<C64> = f.z.iter().map(|z| fs2 - z).collect();
    let np: Vec<C64> = f.p.iter().map(|p| fs2 - p).collect();
    Zpk { z, p, k: f.k * (prod(&nz) / prod(&np)).re }
}

/// H(x) = k·Π(x − z)/Π(x − p).
pub fn zpk_eval(f: &Zpk, x: C64) -> C64 {
    f.z.iter().fold(c(f.k), |a, z| a * (x - z)) / f.p.iter().fold(c(1.0), |a, p| a * (x - p))
}

/// b, a (descending powers, a monic) from zpk: the same expansion as control.lti.convert (crate::poly).
pub fn zpk2tf(f: &Zpk) -> (Vec<f64>, Vec<f64>) {
    let a = poly_real(&f.p);
    let mut b: Vec<f64> = poly_real(&f.z).iter().map(|x| x * f.k).collect();
    // digital filters: pad b to len(a) on the left is NOT done here (degrees already equal after bilinear)
    if b.is_empty() {
        b = vec![f.k];
    }
    (b, a)
}

/// Group roots into conjugate pairs (positive imaginary part representative) and real singles, preserving a
/// deterministic order. Roots with |Im| ≤ tol·max(1, |root|) count as real.
fn group(v: &[C64]) -> (Vec<C64>, Vec<f64>) {
    let tol = 1e-12;
    let mut pairs = Vec::new();
    let mut reals = Vec::new();
    for r in v {
        if r.im.abs() <= tol * r.norm().max(1.0) {
            reals.push(r.re);
        } else if r.im > 0.0 {
            pairs.push(*r);
        }
    }
    (pairs, reals)
}

/// Second-order sections [b0 b1 b2 1 a1 a2] (digital). Pole groups (conjugate pairs; real poles two at a time,
/// a lone real pole last) are ordered by distance from the unit circle, farthest first; each takes the
/// nearest remaining zeros (a conjugate pair, else the nearest reals). The gain goes into the first section.
pub fn zpk2sos(f: &Zpk) -> Vec<[f64; 6]> {
    let (pp, mut pr) = group(&f.p);
    let (mut zp, mut zr) = group(&f.z);
    // pole groups: (poly coefficients [1 a1 a2], representative for distance)
    let mut groups: Vec<(Vec<f64>, f64)> = pp.iter().map(|p| (vec![1.0, -2.0 * p.re, p.norm_sqr()], (1.0 - p.norm()).abs())).collect();
    pr.sort_by(|a, b| (1.0 - a.abs()).abs().partial_cmp(&(1.0 - b.abs()).abs()).unwrap().reverse());
    let mut i = 0;
    while i + 1 < pr.len() {
        groups.push((vec![1.0, -(pr[i] + pr[i + 1]), pr[i] * pr[i + 1]], (1.0 - pr[i].abs()).abs().min((1.0 - pr[i + 1].abs()).abs())));
        i += 2;
    }
    let lone = if i < pr.len() { Some(pr[i]) } else { None };
    groups.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let rep = |g: &[f64]| -> C64 {
        // a root of 1 + a1 x + a2 x²
        let disc = g[1] * g[1] - 4.0 * g[2];
        if disc < 0.0 { C64::new(-g[1] / 2.0, (-disc).sqrt() / 2.0) } else { c((-g[1] + disc.sqrt()) / 2.0) }
    };
    let mut sos: Vec<[f64; 6]> = Vec::new();
    let take_zeros = |p: C64, want: usize, zp: &mut Vec<C64>, zr: &mut Vec<f64>| -> Vec<f64> {
        if want == 2 && !zp.is_empty() {
            let (j, _) = zp.iter().enumerate().fold((0, f64::INFINITY), |(bj, bd), (j, z)| if (z - p).norm() < bd { (j, (z - p).norm()) } else { (bj, bd) });
            let z = zp.remove(j);
            return vec![1.0, -2.0 * z.re, z.norm_sqr()];
        }
        let mut poly = vec![1.0];
        for _ in 0..want {
            if zr.is_empty() {
                break;
            }
            let (j, _) = zr.iter().enumerate().fold((0, f64::INFINITY), |(bj, bd), (j, z)| if (c(*z) - p).norm() < bd { (j, (c(*z) - p).norm()) } else { (bj, bd) });
            let z = zr.remove(j);
            poly = crate::poly::polymul(&poly, &[1.0, -z]);
        }
        poly
    };
    for (g, _) in &groups {
        let zpoly = take_zeros(rep(g), 2, &mut zp, &mut zr);
        let mut b = [0.0; 3];
        for (k, v) in zpoly.iter().enumerate() {
            b[k] = *v;
        }
        sos.push([b[0], b[1], b[2], 1.0, g[1], g[2]]);
    }
    if let Some(p) = lone {
        let zpoly = take_zeros(c(p), 1, &mut zp, &mut zr);
        let mut b = [0.0; 3];
        for (k, v) in zpoly.iter().enumerate() {
            b[k] = *v;
        }
        sos.push([b[0], b[1], b[2], 1.0, -p, 0.0]);
    }
    if sos.is_empty() {
        sos.push([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
    for v in &mut sos[0][..3] {
        *v *= f.k;
    }
    sos
}

/// H(e^{iω}) of b/a (descending powers of z, i.e. ascending powers of z⁻¹).
pub fn ba_eval_z(b: &[f64], a: &[f64], z: C64) -> C64 {
    let zi = c(1.0) / z;
    let h = |p: &[f64]| p.iter().rev().fold(c(0.0), |acc, v| acc * zi + v);
    h(b) / h(a)
}

/// H of an analog b/a at s (descending powers of s).
pub fn ba_eval_s(b: &[f64], a: &[f64], s: C64) -> C64 {
    crate::poly::polyval(b, s) / crate::poly::polyval(a, s)
}

pub fn sos_eval_z(sos: &[[f64; 6]], z: C64) -> C64 {
    sos.iter().fold(c(1.0), |acc, s| acc * ba_eval_z(&s[..3], &s[3..], z))
}

/// Transposed direct form II (MATLAB filter, scipy lfilter). b and a are padded to the same length m and
/// divided by a[0]. `zi` (length m − 1) is the initial state; returns (y, final state).
pub fn lfilter(b: &[f64], a: &[f64], x: &[f64], zi: Option<&[f64]>) -> (Vec<f64>, Vec<f64>) {
    let m = b.len().max(a.len());
    let a0 = a[0];
    let bn: Vec<f64> = (0..m).map(|i| b.get(i).copied().unwrap_or(0.0) / a0).collect();
    let an: Vec<f64> = (0..m).map(|i| a.get(i).copied().unwrap_or(0.0) / a0).collect();
    let mut z: Vec<f64> = match zi {
        Some(v) => v.to_vec(),
        None => vec![0.0; m - 1],
    };
    let mut y = Vec::with_capacity(x.len());
    for &xn in x {
        let yn = if m > 1 { z[0] + bn[0] * xn } else { bn[0] * xn };
        for i in 0..m.saturating_sub(1) {
            let next = if i + 1 < m - 1 { z[i + 1] } else { 0.0 };
            z[i] = next + bn[i + 1] * xn - an[i + 1] * yn;
        }
        y.push(yn);
    }
    (y, z)
}

/// Cascade of second-order sections, each by `lfilter` (scipy sosfilt). zi: per section, 2 values.
/// Returns (y, final states, the output of every section).
pub fn sosfilt(sos: &[[f64; 6]], x: &[f64], zi: Option<&[[f64; 2]]>) -> (Vec<f64>, Vec<[f64; 2]>, Vec<Vec<f64>>) {
    let mut cur = x.to_vec();
    let mut zf = Vec::with_capacity(sos.len());
    let mut stages = Vec::with_capacity(sos.len());
    for (l, s) in sos.iter().enumerate() {
        let z0 = zi.map(|z| z[l]);
        let (y, z) = lfilter(&s[..3], &s[3..], &cur, z0.as_ref().map(|v| &v[..]));
        zf.push([z[0], z[1]]);
        stages.push(y.clone());
        cur = y;
    }
    (cur, zf, stages)
}
