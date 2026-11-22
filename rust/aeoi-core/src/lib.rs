//! aeoi-core — AEOI v0.2 core: arrays (dense/b64/ref/sparse), LTI parsing, schema inlining &
//! validation, envelopes, structured errors, canonical hashing, and small dense linear-algebra helpers.
pub mod error;
pub mod jcs;
pub mod host;
pub mod matrix;
pub mod lti;
pub mod schema;
pub mod linalg;
pub mod operator;

pub const SPEC: &str = "aeoi/0.2";

pub use error::{OpError, Warning};
pub use host::{ExecCtx, HostCtx};
pub use operator::{DynOperator, Operator, OperatorMeta, Output};

/// JSON number helper: non-finite values are encoded as "NaN" / "Inf" / "-Inf" (spec R-2).
pub fn num(x: f64) -> serde_json::Value {
    if x.is_finite() {
        serde_json::Value::from(x)
    } else if x.is_nan() {
        serde_json::Value::from("NaN")
    } else if x > 0.0 {
        serde_json::Value::from("Inf")
    } else {
        serde_json::Value::from("-Inf")
    }
}

/// Optional number: None -> null.
pub fn opt_num(x: Option<f64>) -> serde_json::Value {
    match x {
        Some(v) => num(v),
        None => serde_json::Value::Null,
    }
}
