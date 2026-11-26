//! aeoi-control — control-system operators conforming to AEOI v0.3.
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

pub mod bode;
pub mod c2d;
pub mod connect;
pub mod convert;
pub mod ctrb_obsv;
pub mod dlqr;
pub mod loopsens;
pub mod lqr;
pub mod lsim;
pub mod lyap;
pub mod margin;
pub mod pade;
pub mod pid;
pub mod pidtune;
pub mod place;
pub mod poles_zeros;
pub mod step;
pub mod stepinfo;

use aeoi_core::DynOperator;

/// All operators in this crate.
pub fn registry() -> Vec<Box<dyn DynOperator>> {
    vec![
        Box::new(bode::Bode::new()),
        Box::new(step::Step::new()),
        Box::new(lqr::Lqr::new()),
        Box::new(convert::Convert::new()),
        Box::new(poles_zeros::PolesZeros::new()),
        Box::new(ctrb_obsv::CtrbObsv::new()),
        Box::new(margin::Margin::new()),
        Box::new(c2d::C2d::new()),
        Box::new(step::Step::impulse()),
        Box::new(lyap::Lyap::new()),
        Box::new(place::Place::new()),
        Box::new(dlqr::Dlqr::new()),
        Box::new(connect::Series::new()),
        Box::new(connect::Parallel::new()),
        Box::new(connect::Feedback::new()),
        Box::new(pid::Pid::new()),
        Box::new(stepinfo::StepInfo::new()),
        Box::new(pade::Pade::new()),
        Box::new(loopsens::LoopSens::new()),
        Box::new(lsim::Lsim::new()),
        Box::new(pidtune::PidTune::new()),
    ]
}

pub fn find<'a>(reg: &'a [Box<dyn DynOperator>], id: &str) -> Option<&'a dyn DynOperator> {
    reg.iter().find(|o| o.meta().id == id || o.meta().manifest["mcp"]["tool_name"] == id).map(|b| b.as_ref())
}

/// E.RESOURCE.INPUT_TOO_LARGE if n exceeds manifest limits.max_states.
pub(crate) fn check_states(meta: &aeoi_core::OperatorMeta, n: usize, path: &str) -> Result<(), aeoi_core::OpError> {
    let max_n = meta.manifest["limits"]["max_states"].as_u64().unwrap_or(200) as usize;
    if n > max_n {
        return Err(aeoi_core::OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("状态维数 n = {n} 超过该算子上限 {max_n}。"), "先降阶（如平衡截断）再调用，或拆分问题。").path(path).detail("limit", serde_json::json!(max_n)));
    }
    Ok(())
}

pub(crate) fn mimo_err(path: &str, what: &str, ny: usize, nu: usize) -> aeoi_core::OpError {
    aeoi_core::OpError::new("E.DOMAIN.MIMO_NOT_SUPPORTED", format!("{what} 只支持 SISO 系统，输入为 {ny}×{nu}。"), "逐通道调用（取出单个输出/输入对应的 SISO 子系统），或改用支持 MIMO 的表示（tf/ss）。").path(path).detail("dims", serde_json::json!([ny, nu]))
}

pub(crate) const U: f64 = f64::EPSILON / 2.0; // unit roundoff


#[cfg(test)]
mod op_tests {
    //! Operator-level unit tests against analytic results (independent of any reference implementation).
    use super::*;
    use aeoi_core::HostCtx;
    use serde_json::{json, Value};

    fn run(op: &str, req: Value) -> Value {
        let reg = registry();
        find(&reg, op).unwrap().call(&req, &HostCtx::default())
    }
    fn nums(v: &Value) -> Vec<f64> {
        v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
    }

    #[test]
    fn bode_first_order_analytic() {
        // G = 1/(s+1): |G(jw)| = 1/sqrt(1+w^2), arg = -atan(w)
        let e = run("control.lti.bode", json!({"system":{"kind":"tf","num":[1.0],"den":[1.0,1.0],"ts":0.0,"time_unit":"s"},
            "frequency":{"mode":"explicit","values":[0.1,1.0,10.0],"unit":"rad/s"}}));
        let m = nums(&e["result"]["magnitude"]["value"]["data"]);
        let p = nums(&e["result"]["phase"]["value"]["data"]);
        for (i, w) in [0.1f64, 1.0, 10.0].iter().enumerate() {
            assert!((m[i] + 10.0 * (1.0 + w * w).log10()).abs() < 1e-12);
            assert!((p[i] + w.atan().to_degrees()).abs() < 1e-12);
        }
    }

    #[test]
    fn bode_unwrap_is_continuous_and_anchored() {
        // 1/(s+1)^4: phase goes 0 → -360; unwrapped output must be continuous and start in (-180,180]
        let e = run("control.lti.bode", json!({"system":{"kind":"zpk","z":[],"p":[[-1.0,0.0],[-1.0,0.0],[-1.0,0.0],[-1.0,0.0]],"k":1.0,"ts":0.0,"time_unit":"s"},
            "frequency":{"mode":"logspace","min":0.01,"max":100.0,"n":50,"unit":"rad/s"}}));
        let p = nums(&e["result"]["phase"]["value"]["data"]);
        assert!(p[0] > -180.0 && p[0] <= 180.0);
        assert!(p.windows(2).all(|w| (w[1] - w[0]).abs() < 180.0));
        assert!(*p.last().unwrap() < -300.0);
    }

    #[test]
    fn bode_pole_on_grid_is_error() {
        let e = run("control.lti.bode", json!({"system":{"kind":"tf","num":[1.0],"den":[1.0,0.0,1.0],"ts":0.0,"time_unit":"s"},
            "frequency":{"mode":"explicit","values":[0.5,1.0,2.0],"unit":"rad/s"}}));
        assert_eq!(e["error"]["code"], "E.NUMERICAL.POLE_ON_GRID");
    }

    #[test]
    fn step_first_order_analytic() {
        // 1/(s+1): y = 1 - e^{-t}; ZOH is exact for step input
        let e = run("control.lti.step", json!({"system":{"kind":"tf","num":[1.0],"den":[1.0,1.0],"ts":0.0,"time_unit":"s"},
            "time":{"mode":"uniform","t_final":5.0,"dt":0.5,"unit":"s"}}));
        let y = nums(&e["result"]["y"]["value"]["data"]);
        for (k, yk) in y.iter().enumerate() {
            let t = 0.5 * k as f64;
            assert!((yk - (1.0 - (-t).exp())).abs() < 1e-14, "k={k}");
        }
    }

    #[test]
    fn step_unstable_has_null_info_and_warning() {
        let e = run("control.lti.step", json!({"system":{"kind":"tf","num":[1.0],"den":[1.0,-1.0],"ts":0.0,"time_unit":"s"},
            "time":{"mode":"uniform","t_final":1.0,"dt":0.1,"unit":"s"},"step_info":{"enabled":true}}));
        assert!(e["result"]["info"].is_null());
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.DOMAIN.UNSTABLE_SYSTEM"));
    }

    #[test]
    fn lqr_scalar_analytic() {
        // a=1,b=1,q=1,r=1: s = 1+sqrt(2), k = s
        let m = |v: f64| json!({"dtype":"f64","shape":[1,1],"data":[v]});
        let e = run("control.design.lqr", json!({"A":m(1.0),"B":m(1.0),"Q":m(1.0),"R":m(1.0)}));
        let k = e["result"]["K"]["data"][0].as_f64().unwrap();
        assert!((k - (1.0 + 2f64.sqrt())).abs() < 1e-14);
    }

    #[test]
    fn lqr_rejects_indefinite_r_and_asymmetric_q() {
        let m = |s: [usize; 2], d: Vec<f64>| json!({"dtype":"f64","shape":s,"data":d});
        let base = json!({"A":m([2,2],vec![0.,1.,0.,0.]),"B":m([2,1],vec![0.,1.]),"Q":m([2,2],vec![1.,0.,0.,1.]),"R":m([1,1],vec![-1.])});
        assert_eq!(run("control.design.lqr", base.clone())["error"]["code"], "E.DOMAIN.R_NOT_POSITIVE_DEFINITE");
        let mut q = base.clone();
        q["R"] = m([1, 1], vec![1.]);
        q["Q"] = m([2, 2], vec![1., 0.5, 0., 1.]);
        assert_eq!(run("control.design.lqr", q)["error"]["code"], "E.VALIDATION.NOT_SYMMETRIC");
    }

    #[test]
    fn deterministic_repeat_calls_bitwise_equal() {
        let req = json!({"system":{"kind":"tf","num":[4.0],"den":[1.0,1.2,4.0],"ts":0.0,"time_unit":"s"},"time":{"mode":"uniform","t_final":10.0,"dt":0.01,"unit":"s"}});
        let a = run("control.lti.step", req.clone());
        let b = run("control.lti.step", req);
        assert_eq!(a["reproducibility"]["output_hash"], b["reproducibility"]["output_hash"]);
    }
}
