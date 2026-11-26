//! Digital filtering kernel (spec 10.25): b/a and second-order-section frequency response and the
//! transposed direct form II recursion (MATLAB filter / scipy lfilter), SOS cascade. signal.filter.filter is
//! built on these; filtfilt / freqz reuse them (spec 11.5 RI-1). IIR design lives in kernels::iir.
use crate::lti::C64;

pub use super::iir::Zpk;

fn c(re: f64) -> C64 {
    C64::new(re, 0.0)
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
