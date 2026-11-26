//! Frequency-response kernel: evaluation points (s = jω, or z = e^{jωTs}), transfer-function evaluation by
//! Horner with a first-order a-priori error bound (Higham 2002 §5.1), zpk products with their bound, the
//! state-space resolvent C(sI − A)⁻¹B + D, and the phase conventions (wrap, unwrap). Shared by bode, margin
//! and later nyquist / nichols / dcgain / freqz (spec 11.5 RI-1).
use crate::lti::C64;
use nalgebra::DMatrix;

/// unit roundoff
pub const U: f64 = f64::EPSILON / 2.0;

/// γ_k = k·u / (1 − k·u)
pub fn gamma(k: usize) -> f64 {
    let ku = k as f64 * U;
    ku / (1.0 - ku)
}

/// The evaluation point for frequency ω (rad/s): jω (continuous, ts = 0) or e^{jωTs} (discrete).
pub fn s_of(w: f64, ts: f64) -> C64 {
    if ts > 0.0 { C64::from_polar(1.0, w * ts) } else { C64::new(0.0, w) }
}

/// Horner evaluation with the running condition sum Σ|a_k||s|^k (Higham 2002 §5.1).
pub fn horner(p: &[f64], s: C64) -> (C64, f64) {
    let mut v = C64::new(0.0, 0.0);
    let mut mag = 0.0;
    let sa = s.norm();
    for &a in p {
        v = v * s + a;
        mag = mag * sa + a.abs();
    }
    (v, mag)
}

/// num(s)/den(s) with its first-order relative error bound (numerator + denominator, without the final
/// rounding); None when s is a pole or numerically indistinguishable from one (bound ≥ 1/2).
pub fn tf_point(num: &[f64], den: &[f64], s: C64) -> Option<(C64, f64)> {
    let (nv, nm) = horner(num, s);
    let (dv, dm) = horner(den, s);
    let bd = gamma(2 * den.len()) * dm / dv.norm();
    if dv.norm() == 0.0 || bd >= 0.5 {
        return None;
    }
    let bn = if nv.norm() == 0.0 { 0.0 } else { gamma(2 * num.len()) * nm / nv.norm() };
    Some((nv / dv, bn + bd))
}

/// k·Π(s − z)/Π(s − p) with the bound Σ u(|s|+|r|)/|s − r| + (n_z + n_p + 1)u; None at a pole.
pub fn zpk_point(z: &[C64], p: &[C64], gain: f64, s: C64) -> Option<(C64, f64)> {
    let mut v = C64::new(gain, 0.0);
    let mut b = 0.0;
    for zz in z {
        let d = s - zz;
        if d.norm() != 0.0 {
            b += U * (s.norm() + zz.norm()) / d.norm();
        }
        v *= d;
    }
    for pp in p {
        let d = s - pp;
        if d.norm() == 0.0 || U * (s.norm() + pp.norm()) / d.norm() >= 0.5 {
            return None;
        }
        b += U * (s.norm() + pp.norm()) / d.norm();
        v /= d;
    }
    Some((v, b + (z.len() + p.len() + 1) as f64 * U))
}

/// Resolvent X = (sI − A)⁻¹B for complexified A, B; also returns the matrix sI − A (for condition
/// estimates). None if singular.
pub fn resolvent(ac: &DMatrix<C64>, bc: &DMatrix<C64>, s: C64) -> Option<(DMatrix<C64>, DMatrix<C64>)> {
    let n = ac.nrows();
    let m = DMatrix::<C64>::identity(n, n) * s - ac;
    let x = super::nalg::csolve(&m, bc)?;
    Some((x, m))
}

/// Map an angle (deg) into (c − 180, c + 180].
pub fn wrap_to(p: f64, c: f64) -> f64 {
    p - 360.0 * ((p - c - 180.0) / 360.0).ceil()
}

/// Magnitude (dB) and phase (deg) of one channel's response along the frequency grid: wrapped into
/// (center − 180, center + 180], or unwrapped from the first point's wrapped value. A zero response gives
/// −∞ dB and NaN phase (and does not move the unwrap reference). Returns the number of zero points.
pub fn mag_phase(h: &[C64], unwrap: bool, center: f64, mag: &mut [f64], phase: &mut [f64]) -> usize {
    let mut prev_arg: Option<f64> = None;
    let mut prev_phase = 0.0;
    let mut zero_hits = 0usize;
    for (k, v) in h.iter().enumerate() {
        if v.norm() == 0.0 {
            mag[k] = f64::NEG_INFINITY;
            phase[k] = f64::NAN;
            zero_hits += 1;
            continue;
        }
        mag[k] = 20.0 * v.norm().log10();
        let arg = v.im.atan2(v.re).to_degrees();
        let ph = match (unwrap, prev_arg) {
            (true, Some(pa)) => prev_phase + wrap_to(arg - pa, 0.0),
            _ => wrap_to(arg, center),
        };
        phase[k] = ph;
        prev_arg = Some(arg);
        prev_phase = ph;
    }
    zero_hits
}

/// SISO num/den at frequency ω by plain polynomial evaluation (no bound).
pub fn tf_siso(num: &[f64], den: &[f64], w: f64, ts: f64) -> C64 {
    let s = s_of(w, ts);
    crate::poly::polyval(num, s) / crate::poly::polyval(den, s)
}
