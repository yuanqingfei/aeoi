# Patched copy of rustfft 6.4.1

Source: crates.io `rustfft` 6.4.1 (MIT OR Apache-2.0, © Allen Welkie, Elliott Mahler and contributors; license
files kept; `benches/`, `tools/` and `Cargo.lock` removed). Used through `[patch.crates-io]` in `rust/Cargo.toml`.

Only change: `compute_twiddle` in `src/twiddles.rs` calls `libm::cos` / `libm::sin` (libm 0.2.16) instead of
`f64::cos` / `f64::sin`.

Why: the fixed-size butterflies call `compute_twiddle` with constant arguments, and LLVM folds `f64::cos` of a
constant at compile time using the C library of the machine that runs the compiler. The release CI (spec 2.1
M-7a) found FFT outputs that differed between binaries built on Linux, macOS and Windows, although every
runtime math call already went to the same libm. `libm::cos` is ordinary Rust code, so folding it is exact
IEEE arithmetic and gives the same bits on every build host.
