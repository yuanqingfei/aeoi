//! Shared numerical kernels (spec 11, RI-1). An operator that needs another operator's computation calls the
//! kernel that operator is built on — never a second implementation and never an outside library's equivalent.
pub mod cdense;
pub mod dense;
pub mod eig;
pub mod fft;
pub mod filter;
pub mod lp;
pub mod qp;
pub mod schur;
pub mod nalg;
pub mod svd;
pub mod sum;
pub mod bracket;
pub mod newton;
pub mod quad;
pub mod rk;
pub mod spline;
pub mod riccati;
pub mod freqresp;
pub mod timeresp;
pub mod iir;
