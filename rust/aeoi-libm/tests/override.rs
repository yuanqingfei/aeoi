//! On every platform: the `f64` methods (which call the C symbols) return exactly what the `libm` crate
//! returns. If the system math library were still in use, some of the ~10⁵ arguments would differ in the
//! last bit (glibc, Apple libm and the UCRT are not musl). Runs in CI on Linux, macOS and Windows.
use std::hint::black_box;

fn args() -> Vec<f64> {
    // deterministic xorshift over several magnitudes, plus special values
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut v = vec![0.0, -0.0, 1.0, -1.0, 0.5, 2.0, 1e-300, 1e300, f64::MIN_POSITIVE, 710.0, -745.0, 1e22, std::f64::consts::PI];
    for i in 0..60_000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let u = (s >> 11) as f64 / (1u64 << 53) as f64;
        let e = [-20.0, -3.0, 0.0, 1.0, 3.0, 10.0][i % 6];
        v.push((2.0 * u - 1.0) * 10f64.powi(e as i32 + 0) * if i % 2 == 0 { 1.0 } else { 3.7 });
    }
    v
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

#[test]
fn f64_methods_route_to_rust_libm() {
    assert!(aeoi_libm::linked());
    let xs = args();
    let mut bad: Vec<String> = Vec::new();
    macro_rules! u {
        ($m:ident, $l:path) => {
            for &x in &xs {
                let (a, b) = (black_box(x).$m(), $l(x));
                if !same(a, b) && bad.len() < 20 {
                    bad.push(format!("{}({x:e}) = {a:e} vs libm {b:e}", stringify!($m)));
                }
            }
        };
    }
    u!(sin, libm::sin);
    u!(cos, libm::cos);
    u!(tan, libm::tan);
    u!(asin, libm::asin);
    u!(acos, libm::acos);
    u!(atan, libm::atan);
    u!(sinh, libm::sinh);
    u!(cosh, libm::cosh);
    u!(tanh, libm::tanh);
    u!(exp, libm::exp);
    u!(exp2, libm::exp2);
    u!(exp_m1, libm::expm1);
    u!(ln, libm::log);
    u!(log10, libm::log10);
    u!(log2, libm::log2);
    u!(ln_1p, libm::log1p);
    u!(cbrt, libm::cbrt);
    for (i, &x) in xs.iter().enumerate() {
        let y = xs[(i * 7919 + 13) % xs.len()];
        for (name, a, b) in [
            ("atan2", black_box(x).atan2(y), libm::atan2(x, y)),
            ("powf", black_box(x).abs().powf(y / 7.0), libm::pow(x.abs(), y / 7.0)),
            ("hypot", black_box(x).hypot(y), libm::hypot(x, y)),
        ] {
            if !same(a, b) && bad.len() < 20 {
                bad.push(format!("{name}({x:e}, {y:e}) = {a:e} vs libm {b:e}"));
            }
        }
        let (s, c) = black_box(x).sin_cos();
        if !(same(s, libm::sin(x)) && same(c, libm::cos(x))) && bad.len() < 20 {
            bad.push(format!("sin_cos({x:e})"));
        }
    }
    assert!(bad.is_empty(), "system libm still in use:\n{}", bad.join("\n"));
}
