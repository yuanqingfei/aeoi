//! control.lti.poles_zeros — poles, zeros, DC gain, damping (spec 10.5).
use crate::check_states;
use aeoi_core::error::{OpError, Warning};
use aeoi_core::linalg::sort_poles;
use aeoi_core::lti::{eigenvalues, parse_lti, Lti, SsData, C64};
use aeoi_core::matrix::{complex_vector, dense_real, dense_real_nd};
use aeoi_core::poly::{polyval_r, roots};
use aeoi_core::sysalg::{invariant_zeros, ss2zpk_siso};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

operator_struct!(PolesZeros, "control.lti.poles_zeros");

pub struct PzInput {
    pub sys: Lti,
    pub dcgain: bool,
}

/// DC gain of a SISO zpk at s0 (0 continuous, 1 discrete) with cancellation of coincident roots at s0.
fn zpk_dc(z: &[C64], p: &[C64], k: f64, s0: f64, tol: f64) -> f64 {
    let at = |r: &C64| (r - s0).norm() <= tol * (1.0 + s0.abs());
    let nz0 = z.iter().filter(|r| at(r)).count() as isize;
    let np0 = p.iter().filter(|r| at(r)).count() as isize;
    let mut v = k;
    for r in z.iter().filter(|r| !at(r)) {
        v *= (C64::new(s0, 0.0) - r).re.signum() * (C64::new(s0, 0.0) - r).norm();
    }
    for r in p.iter().filter(|r| !at(r)) {
        v /= (C64::new(s0, 0.0) - r).re.signum() * (C64::new(s0, 0.0) - r).norm();
    }
    if nz0 > np0 || k == 0.0 {
        0.0
    } else if nz0 == np0 {
        v
    } else {
        v.signum() * f64::INFINITY
    }
}

fn sub(ss: &SsData, i: usize, j: usize) -> SsData {
    SsData { a: ss.a.clone(), b: ss.b.columns(j, 1).into_owned(), c: ss.c.rows(i, 1).into_owned(), d: DMatrix::from_element(1, 1, ss.d[(i, j)]), ts: ss.ts }
}

impl Operator for PolesZeros {
    type Input = PzInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(PzInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        if let Lti::Ss { a, .. } = &sys {
            check_states(&self.meta, a.nrows(), "/system/A")?;
        }
        Ok((PzInput { sys, dcgain: args.get("dcgain").and_then(|x| x.as_bool()).unwrap_or(true) }, w))
    }
    fn execute(&self, inp: PzInput, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let ts = inp.sys.ts();
        let (ny, nu) = inp.sys.dims();
        let degenerate = || OpError::new("E.DOMAIN.DEGENERATE_SYSTEM", "Rosenbrock 系统矩阵不是满法秩（例如某输出恒为零或输入全部不起作用），此时每个复数都是“零点”，零点无定义。", "检查 B、C、D 中是否有全零行/列；去掉恒零通道后再求零点。").path("/system");
        let mut zinfo = None;
        let (mut poles, mut zeros, zdef) = match &inp.sys {
            Lti::Tf { num: n, den, .. } if ny * nu == 1 => (roots(&den[0][0]), roots(&n[0][0]), "polynomial_roots"),
            Lti::Zpk { z, p, .. } => (p.clone(), z.clone(), "polynomial_roots"),
            _ => {
                let ss = inp.sys.to_ss();
                if matches!(inp.sys, Lti::Tf { .. }) {
                    warnings.push(Warning::new("W.DOMAIN.NONMINIMAL_REALIZATION", "MIMO tf 的极点取各元素分母根的并集（块对角实现，非最小），零点为该实现的不变零点（可能包含与极点对消的解耦零点）。").path("/system"));
                }
                let (z, info) = invariant_zeros(&ss).map_err(|_| degenerate())?;
                zinfo = Some(info);
                (eigenvalues(&ss.a), z, "invariant_zeros")
            }
        };
        sort_poles(&mut poles);
        sort_poles(&mut zeros);
        if let Some(i) = &zinfo {
            if i.cond_d_reduced > 1e10 {
                warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("零点计算中约化后的 D_r 条件数 {:.2e} 偏大，零点精度可能下降。", i.cond_d_reduced)).path("/system"));
            }
        }
        // DC gain
        let s0 = if ts > 0.0 { 1.0 } else { 0.0 };
        let mut dc_method = "none";
        let dc: Option<DMatrix<f64>> = if !inp.dcgain {
            None
        } else {
            Some(match &inp.sys {
                Lti::Tf { num: n, den, .. } => {
                    dc_method = "polynomial_evaluation_with_exact_cancellation";
                    let mut g = DMatrix::zeros(ny, nu);
                    for i in 0..ny {
                        for j in 0..nu {
                            let (mut a, mut b) = (n[i][j].clone(), den[i][j].clone());
                            if ts == 0.0 {
                                // exact common factors s^k (trailing zeros in both)
                                while a.len() > 1 && b.len() > 1 && *a.last().unwrap() == 0.0 && *b.last().unwrap() == 0.0 {
                                    a.pop();
                                    b.pop();
                                }
                            }
                            let (nv, dv) = (polyval_r(&a, s0), polyval_r(&b, s0));
                            g[(i, j)] = if dv != 0.0 { nv / dv } else if nv != 0.0 { nv.signum() * f64::INFINITY } else { f64::NAN };
                        }
                    }
                    g
                }
                Lti::Zpk { z, p, k, .. } => {
                    dc_method = "zpk_product_with_cancellation";
                    DMatrix::from_element(1, 1, zpk_dc(z, p, *k, s0, 1e-12))
                }
                Lti::Ss { .. } => {
                    let ss = inp.sys.to_ss();
                    let n = ss.a.nrows();
                    let m = if ts > 0.0 { DMatrix::identity(n, n) - &ss.a } else { -ss.a.clone() };
                    let rc = if n == 0 { 1.0 } else { 1.0 / aeoi_core::linalg::cond2(&m) };
                    if n == 0 {
                        dc_method = "D";
                        ss.d.clone()
                    } else if rc > 1e-13 {
                        dc_method = if ts > 0.0 { "D + C(I−A)⁻¹B" } else { "D − C A⁻¹ B" };
                        let x = aeoi_core::kernels::nalg::solve(&m, &ss.b).unwrap();
                        &ss.c * x + &ss.d
                    } else {
                        dc_method = "per_element_zpk_limit (A numerically singular)";
                        let mut g = DMatrix::zeros(ny, nu);
                        for i in 0..ny {
                            for j in 0..nu {
                                g[(i, j)] = match ss2zpk_siso(&sub(&ss, i, j)) {
                                    Ok((z, p, k, _)) => zpk_dc(&z, &p, k, s0, 1e-8),
                                    Err(_) => 0.0,
                                };
                            }
                        }
                        g
                    }
                }
            })
        };
        if let Some(g) = &dc {
            let ninf = g.iter().filter(|v| v.is_infinite()).count();
            let nnan = g.iter().filter(|v| v.is_nan()).count();
            if ninf > 0 {
                warnings.push(Warning::new("W.DOMAIN.INFINITE_DC_GAIN", format!("{ninf} 个通道在 {} 处有未对消的极点，DC 增益为 ±Inf（符号取实轴上 {} 一侧的极限）。", if ts > 0.0 { "z = 1" } else { "s = 0" }, if ts > 0.0 { "z → 1⁺" } else { "s → 0⁺" })).path("/system"));
            }
            if nnan > 0 {
                warnings.push(Warning::new("W.DOMAIN.UNDEFINED_DC_GAIN", format!("{nnan} 个通道出现 0/0（数值上无法对消），DC 增益为 NaN。")).path("/system"));
            }
        }
        // damping
        let (mut wn, mut zeta) = (Vec::new(), Vec::new());
        for p in &poles {
            let s = if ts > 0.0 { p.ln() / ts } else { *p };
            let w = s.norm();
            wn.push(w);
            zeta.push(if w == 0.0 { f64::NAN } else { -s.re / w });
        }
        let stable = if ts > 0.0 { poles.iter().all(|p| p.norm() < 1.0) } else { poles.iter().all(|p| p.re < 0.0) };
        let np = poles.len();
        let result = json!({
            "poles": complex_vector(&poles), "zeros": complex_vector(&zeros), "zero_definition": zdef,
            "dcgain": dc.as_ref().map(|g| json!({"unit": "1", "value": dense_real(g)})).unwrap_or(Value::Null),
            "natural_frequency": {"unit": "rad/s", "value": dense_real_nd(&[np], &wn)},
            "damping_ratio": dense_real_nd(&[np], &zeta),
            "stable": stable
        });
        let mut checks = json!({"stable": stable});
        if let Some(g) = &dc {
            checks["dcgain_finite"] = json!(g.iter().all(|v| v.is_finite()));
        }
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": Value::Null,
            "condition": {
                "zero_reduction": zinfo.as_ref().map(|i| json!({"reduced_order": i.reduced_order, "rank_tolerance": i.rank_tolerance, "cond_D_reduced": num(i.cond_d_reduced)})).unwrap_or(Value::Null),
                "dcgain_method": dc_method,
                "max_pole_modulus": num(poles.iter().map(|p| p.norm()).fold(0.0, f64::max))
            },
            "error_estimate": {"basis": "未给出逐根误差界：特征值/多项式根对扰动的敏感度取决于（未计算的）特征向量条件数；重根的误差可达 O(u^{1/k})"},
            "checks": checks,
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dc_cancel() {
        // s(s+2)/(s(s+1)) at 0 -> 2
        let z = [C64::new(0.0, 0.0), C64::new(-2.0, 0.0)];
        let p = [C64::new(0.0, 0.0), C64::new(-1.0, 0.0)];
        assert!((zpk_dc(&z, &p, 1.0, 0.0, 1e-12) - 2.0).abs() < 1e-15);
        assert_eq!(zpk_dc(&[], &p, 1.0, 0.0, 1e-12), f64::INFINITY);
        assert_eq!(zpk_dc(&[], &[C64::new(0.0, 0.0), C64::new(1.0, 0.0)], 1.0, 0.0, 1e-12), f64::NEG_INFINITY);
        // complex pair (s^2+2s+5) -> dc 1/5
        assert!((zpk_dc(&[], &[C64::new(-1.0, 2.0), C64::new(-1.0, -2.0)], 1.0, 0.0, 1e-12) - 0.2).abs() < 1e-15);
    }
}
