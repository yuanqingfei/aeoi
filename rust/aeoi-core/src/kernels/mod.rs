//! Shared numerical kernels (spec 11, RI-1). An operator that needs another operator's computation calls the
//! kernel that operator is built on — never a second implementation and never an outside library's equivalent.
pub mod dense;
pub mod eig;
pub mod filter;
pub mod lp;
pub mod qp;
pub mod svd;
