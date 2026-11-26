//! Discrete Fourier transform kernel (v0.4.16, replaces rustfft): X_k = Σ_j x_j·exp(∓2πi·jk/n), unnormalised.
//! Power-of-two lengths: iterative radix-2 (bit reversal + butterflies). Other lengths: recursive mixed-radix
//! decimation in time on the smallest prime factor p (p-point DFTs written out); a prime length > 31 goes
//! through Bluestein's chirp-z convolution on a power-of-two FFT. Twiddles exp(∓2πi·j/n) are computed directly
//! (j reduced mod n) with the bundled libm, never by recurrence, and chirp exponents k² are reduced mod 2n in
//! integers. signal.spectral.fft is built on this; spectral estimates, convolution, correlation reuse it
//! (spec 11.5 RI-1).
use crate::lti::C64;
use aeoi_libm::libm;
use std::collections::HashMap;

type Cache = HashMap<(usize, bool), Vec<C64>>;

fn twiddles(n: usize, sign: f64, cache: &mut Cache) -> Vec<C64> {
    cache
        .entry((n, sign > 0.0))
        .or_insert_with(|| {
            (0..n)
                .map(|j| {
                    let th = sign * 2.0 * std::f64::consts::PI * (j as f64) / (n as f64);
                    C64::new(libm::cos(th), libm::sin(th))
                })
                .collect()
        })
        .clone()
}

fn radix2(x: &[C64], sign: f64, cache: &mut Cache) -> Vec<C64> {
    let n = x.len();
    let bits = n.trailing_zeros();
    let mut a: Vec<C64> = (0..n).map(|i| x[if bits == 0 { 0 } else { i.reverse_bits() >> (usize::BITS - bits) }]).collect();
    let w = twiddles(n, sign, cache);
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let t = w[k * step] * a[start + k + len / 2];
                let u = a[start + k];
                a[start + k] = u + t;
                a[start + k + len / 2] = u - t;
            }
        }
        len <<= 1;
    }
    a
}

fn naive(x: &[C64], sign: f64, cache: &mut Cache) -> Vec<C64> {
    let n = x.len();
    let w = twiddles(n, sign, cache);
    (0..n).map(|k| (0..n).fold(C64::new(0.0, 0.0), |s, j| s + x[j] * w[(j * k) % n])).collect()
}

fn bluestein(x: &[C64], sign: f64, cache: &mut Cache) -> Vec<C64> {
    let n = x.len();
    let m = (2 * n - 1).next_power_of_two();
    // w^{j²} with w = exp(sign·iπ/n); j² reduced mod 2n
    let chirp: Vec<C64> = (0..n)
        .map(|j| {
            let e = ((j as u128 * j as u128) % (2 * n as u128)) as f64;
            let th = sign * std::f64::consts::PI * e / n as f64;
            C64::new(libm::cos(th), libm::sin(th))
        })
        .collect();
    let mut a = vec![C64::new(0.0, 0.0); m];
    let mut b = vec![C64::new(0.0, 0.0); m];
    for j in 0..n {
        a[j] = x[j] * chirp[j];
        b[j] = chirp[j].conj();
        if j > 0 {
            b[m - j] = chirp[j].conj();
        }
    }
    let fa = radix2(&a, -1.0, cache);
    let fb = radix2(&b, -1.0, cache);
    let prod: Vec<C64> = fa.iter().zip(&fb).map(|(p, q)| p * q).collect();
    let conv = radix2(&prod, 1.0, cache);
    let inv_m = 1.0 / m as f64;
    (0..n).map(|k| chirp[k] * conv[k] * inv_m).collect()
}

fn smallest_factor(n: usize) -> usize {
    let mut p = 2;
    while p * p <= n {
        if n % p == 0 {
            return p;
        }
        p += 1;
    }
    n
}

fn dft(x: &[C64], sign: f64, cache: &mut Cache) -> Vec<C64> {
    let n = x.len();
    if n <= 1 {
        return x.to_vec();
    }
    if n.is_power_of_two() {
        return radix2(x, sign, cache);
    }
    let p = smallest_factor(n);
    if p == n {
        return if n <= 31 { naive(x, sign, cache) } else { bluestein(x, sign, cache) };
    }
    let m = n / p;
    let subs: Vec<Vec<C64>> = (0..p).map(|r| dft(&x.iter().skip(r).step_by(p).cloned().collect::<Vec<_>>(), sign, cache)).collect();
    let wn = twiddles(n, sign, cache);
    let wp = twiddles(p, sign, cache);
    let mut out = vec![C64::new(0.0, 0.0); n];
    let mut t = vec![C64::new(0.0, 0.0); p];
    for k in 0..m {
        for r in 0..p {
            t[r] = if r == 0 { subs[0][k] } else { wn[(r * k) % n] * subs[r][k] };
        }
        for q in 0..p {
            out[k + m * q] = (0..p).fold(C64::new(0.0, 0.0), |s, r| s + t[r] * wp[(r * q) % p]);
        }
    }
    out
}

/// Unnormalised DFT: forward exp(−2πi·jk/n), inverse exp(+2πi·jk/n).
pub fn fft(x: &[C64], inverse: bool) -> Vec<C64> {
    let mut cache = Cache::new();
    dft(x, if inverse { 1.0 } else { -1.0 }, &mut cache)
}
