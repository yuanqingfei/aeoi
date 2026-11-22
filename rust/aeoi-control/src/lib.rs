//! aeoi-control — control-system operators conforming to AEOI v0.2.
pub mod bode;
pub mod lqr;
pub mod step;

use aeoi_core::DynOperator;

/// All operators in this crate.
pub fn registry() -> Vec<Box<dyn DynOperator>> {
    vec![Box::new(bode::Bode::new()), Box::new(step::Step::new()), Box::new(lqr::Lqr::new())]
}

pub fn find<'a>(reg: &'a [Box<dyn DynOperator>], id: &str) -> Option<&'a dyn DynOperator> {
    reg.iter().find(|o| o.meta().id == id || o.meta().manifest["mcp"]["tool_name"] == id).map(|b| b.as_ref())
}

pub(crate) const U: f64 = f64::EPSILON / 2.0; // unit roundoff

pub(crate) fn gamma(k: usize) -> f64 {
    let ku = k as f64 * U;
    ku / (1.0 - ku)
}

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
