# Patched copy of matrixmultiply 0.3.11

Source: crates.io `matrixmultiply` 0.3.11 (MIT OR Apache-2.0, © bluss and contributors; license files kept).
Used through `[patch.crates-io]` in `rust/Cargo.toml`.

Changes: `#![allow(dead_code, …)]` in `src/lib.rs` (the SIMD kernels become unused), and `detect()` in `src/dgemm_kernel.rs` and `src/sgemm_kernel.rs` always selects `KernelFallback`.
The upstream code picks an AVX-512 / AVX2+FMA / AVX / NEON kernel at run time; those kernels fuse
multiply-adds and block differently, so nalgebra's dense products (used for dynamic matrices larger than
5×5) differed in the last bits between CPUs. AEOI spec 2.1 (v0.4.9) requires the same bits on every
platform, so the portable kernel (plain multiply and add, fixed order) is used everywhere.
