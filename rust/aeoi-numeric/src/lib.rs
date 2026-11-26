//! aeoi-numeric — AEOI operators outside control (signal processing, ODEs) and the shared expression
//! language (spec 3.15). Operators here report `aeoi-numeric` as their build (operator.build).

/// Load manifest + I/O schemas of examples/<dir> at compile time.
macro_rules! operator_meta {
    ($dir:literal) => {
        aeoi_core::OperatorMeta::from_json(
            include_str!(concat!("../../../examples/", $dir, "/manifest.json")),
            include_str!(concat!("../../../examples/", $dir, "/input.schema.json")),
            include_str!(concat!("../../../examples/", $dir, "/output.schema.json")),
        )
        .with_crate(env!("CARGO_PKG_NAME"))
    };
}
/// Boilerplate: operator struct with `new()` and `Default`.
macro_rules! operator_struct {
    ($name:ident, $dir:literal) => {
        pub struct $name {
            meta: aeoi_core::OperatorMeta,
        }
        impl $name {
            pub fn new() -> Self {
                $name { meta: operator_meta!($dir) }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

pub mod butter;
pub mod eig;
pub mod expr;
pub mod filter;
pub mod fft;
pub mod interp1;
pub mod linsolve;
pub mod matrix;
pub mod ode;
pub mod optim;
pub mod polyfit;
pub mod polynomial;
pub mod quad;
pub mod roots;
pub mod svd;

use aeoi_core::DynOperator;

/// All operators in this crate.
pub fn registry() -> Vec<Box<dyn DynOperator>> {
    vec![Box::new(fft::Fft::new()), Box::new(ode::OdeIvp::new()), Box::new(linsolve::LinSolve::new()), Box::new(interp1::Interp1::new()), Box::new(polyfit::PolyFit::new()), Box::new(eig::Eig::new()), Box::new(svd::Svd::new()), Box::new(butter::Butter::new()), Box::new(filter::Filter::new()), Box::new(roots::Fzero::new()), Box::new(roots::Fsolve::new()), Box::new(optim::Linprog::new()), Box::new(optim::Fmincon::new()), Box::new(optim::Fminsearch::new()), Box::new(quad::Integral::new()), Box::new(quad::Trapz::new()), Box::new(polynomial::PolyVal::new()), Box::new(polynomial::Roots::new()), Box::new(matrix::Inv::new()), Box::new(matrix::Det::new())]
}

/// Look up by operator id or MCP tool name.
pub fn find<'a>(reg: &'a [Box<dyn DynOperator>], id: &str) -> Option<&'a dyn DynOperator> {
    reg.iter().find(|o| o.meta().id == id || o.meta().manifest["mcp"]["tool_name"] == id).map(|b| b.as_ref())
}
