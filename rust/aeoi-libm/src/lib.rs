//! One math library on every platform (AEOI spec 2.1, v0.4.9).
//!
//! `f64::sin`, `exp`, `powf`, `hypot`, … in Rust compile to calls of the C symbols `sin`, `exp`, `pow`,
//! `hypot`, … . On a native target those symbols come from the system's C math library (glibc, Apple libm,
//! the Windows UCRT), and the libraries round differently in the last bit. The calls are not only ours:
//! nalgebra (SVD, matrix exponential, complex modulus via num-complex) and rustfft (twiddle factors) make
//! them too. This crate defines the symbols in the binary itself and forwards them to the pure-Rust `libm`
//! crate (a port of musl's libm; the same code that `compiler-builtins` uses on wasm32), so every caller in
//! the binary computes the same bits on every platform.
//!
//! A definition in the executable takes precedence over the system library on ELF, Mach-O and PE, both for
//! dynamic (glibc, libSystem, UCRT import library) and static (musl `libc.a`) linking. [`linked`] takes the
//! address of every definition, so the object file is always pulled into the link; `aeoi_core::operator::libm`
//! calls it. `tests/override.rs` checks, on each platform, that `f64` methods return exactly what `libm`
//! returns.
//!
//! Not on wasm32: there `compiler-builtins` already provides these symbols from the same `libm` source.
#![no_builtins]
#![allow(clippy::missing_safety_doc)]

/// Version of the `libm` crate the math symbols come from (reported in `reproducibility.libm`).
pub const LIBM_VERSION: &str = "0.2.16";

#[cfg(not(target_arch = "wasm32"))]
mod symbols {
    macro_rules! unary {
        ($($name:ident),*) => {$(
            #[no_mangle]
            pub extern "C" fn $name(x: f64) -> f64 {
                libm::$name(x)
            }
        )*};
    }
    macro_rules! binary {
        ($($name:ident),*) => {$(
            #[no_mangle]
            pub extern "C" fn $name(x: f64, y: f64) -> f64 {
                libm::$name(x, y)
            }
        )*};
    }
    unary!(sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh, exp, exp2, expm1, log, log10, log2, log1p, cbrt);
    binary!(atan2, pow, hypot);

    /// glibc / musl: `void sincos(double, double *, double *)` (LLVM merges `sin(x)` and `cos(x)` into it).
    #[no_mangle]
    pub unsafe extern "C" fn sincos(x: f64, s: *mut f64, c: *mut f64) {
        let (sv, cv) = libm::sincos(x);
        *s = sv;
        *c = cv;
    }

    #[repr(C)]
    pub struct SinCos {
        pub sin: f64,
        pub cos: f64,
    }

    /// Apple: the merged sin/cos returns both values in registers.
    #[no_mangle]
    pub extern "C" fn __sincos_stret(x: f64) -> SinCos {
        let (sin, cos) = libm::sincos(x);
        SinCos { sin, cos }
    }

    /// MSVC: Rust's `f64::hypot` calls `_hypot`.
    #[no_mangle]
    pub extern "C" fn _hypot(x: f64, y: f64) -> f64 {
        libm::hypot(x, y)
    }

    pub fn addresses() -> [usize; 27] {
        [
            sin as *const () as usize, cos as *const () as usize, tan as *const () as usize, asin as *const () as usize, acos as *const () as usize, atan as *const () as usize, sinh as *const () as usize,
            cosh as *const () as usize, tanh as *const () as usize, asinh as *const () as usize, acosh as *const () as usize, atanh as *const () as usize, exp as *const () as usize, exp2 as *const () as usize,
            expm1 as *const () as usize, log as *const () as usize, log10 as *const () as usize, log2 as *const () as usize, log1p as *const () as usize, cbrt as *const () as usize,
            atan2 as *const () as usize, pow as *const () as usize, hypot as *const () as usize, sincos as *const () as usize, __sincos_stret as *const () as usize, _hypot as *const () as usize,
            0,
        ]
    }
}

/// Keeps the symbol definitions in the link (their addresses are used) and says whether this build routes
/// the C math symbols to `libm` (false on wasm32, where compiler-builtins does it).
pub fn linked() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let a = std::hint::black_box(symbols::addresses());
        a.iter().take(26).all(|&p| p != 0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}
