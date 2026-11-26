//! IIR design kernel: analog prototypes (Butterworth), zpk frequency transforms (scipy lp2*_zpk
//! conventions), bilinear transform, zpk → polynomial and second-order sections, zpk frequency response,
//! and the design pipeline (prewarp → transform → bilinear) shared by butter and later cheby1 / cheby2 /
//! decimate (spec 11.5 RI-1). Polynomial expansion uses crate::poly.
use crate::lti::C64;
use crate::poly::poly_real;
use aeoi_libm::libm;

// Every transcendental call below goes to the bundled `libm` by name, and powers / complex square roots are
// written out: the Windows build differed in 8 butter golden cases when these went through f64::tan,
// num_complex's polar sqrt and powi (release compare, v0.4.12), so nothing here relies on how the platform
// resolves math symbols (spec 2.1 M-7a).

/// x^k for integer k ≥ 0 by repeated multiplication (same order everywhere).
pub fn powu(x: f64, k: usize) -> f64 {
    (0..k).fold(1.0, |a, _| a * x)
}

/// Principal complex square root (Kahan's formulation, hypot from libm).
pub fn csqrt(z: C64) -> C64 {
    let (a, b) = (z.re, z.im);
    if a == 0.0 && b == 0.0 {
        return C64::new(0.0, b);
    }
    let r = libm::hypot(a, b);
    if a >= 0.0 {
        let t = ((r + a) / 2.0).sqrt();
        C64::new(t, b / (2.0 * t))
    } else {
        let t = ((r - a) / 2.0).sqrt();
        C64::new(b.abs() / (2.0 * t), t.copysign(b))
    }
}

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
            -C64::new(libm::cos(th), libm::sin(th))
        })
        .collect();
    Zpk { z: vec![], p, k: 1.0 }
}

pub fn lp2lp(f: &Zpk, wo: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    Zpk { z: f.z.iter().map(|z| z * wo).collect(), p: f.p.iter().map(|p| p * wo).collect(), k: f.k * powu(wo, deg as usize) }
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
    let mut out: Vec<C64> = l.iter().map(|x| x + csqrt(x * x - wo * wo)).collect();
    out.extend(l.iter().map(|x| x - csqrt(x * x - wo * wo)));
    out
}

pub fn lp2bp(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let mut z = bp_roots(&f.z, bw, wo);
    z.extend(std::iter::repeat(c(0.0)).take(deg));
    Zpk { z, p: bp_roots(&f.p, bw, wo), k: f.k * powu(bw, deg) }
}

pub fn lp2bs(f: &Zpk, wo: f64, bw: f64) -> Zpk {
    let deg = f.p.len() - f.z.len();
    let inv = |v: &[C64]| -> Vec<C64> { v.iter().map(|x| c(bw / 2.0) / x).collect() };
    let split = |h: Vec<C64>| -> Vec<C64> {
        let mut out: Vec<C64> = h.iter().map(|x| x + csqrt(x * x - wo * wo)).collect();
        out.extend(h.iter().map(|x| x - csqrt(x * x - wo * wo)));
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

/// Band type of a design.
#[derive(Clone, Copy, PartialEq)]
pub enum Band {
    Low,
    High,
    BandPass,
    BandStop,
}

/// Design from an analog lowpass prototype (cutoff 1 rad/s): digital edges are normalised (1 = Nyquist) and
/// prewarped with fs = 2 like scipy (4·tan(π·Wn/2)), then the frequency transform for `band` and, for a
/// digital design, the bilinear transform with fs = 2. Analog edges are in rad/s.
pub fn design(proto: &Zpk, band: Band, edges: &[f64], analog: bool) -> Zpk {
    let warp = |w: f64| if analog { w } else { 4.0 * libm::tan(std::f64::consts::PI * w / 2.0) };
    let w: Vec<f64> = edges.iter().map(|e| warp(*e)).collect();
    let a = match band {
        Band::Low => lp2lp(proto, w[0]),
        Band::High => lp2hp(proto, w[0]),
        Band::BandPass => lp2bp(proto, (w[0] * w[1]).sqrt(), w[1] - w[0]),
        Band::BandStop => lp2bs(proto, (w[0] * w[1]).sqrt(), w[1] - w[0]),
    };
    if analog { a } else { bilinear(&a, 2.0) }
}
