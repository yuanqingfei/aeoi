//! control.design.dlqr — discrete-time LQR via the structure-preserving doubling algorithm (SDA)
//! plus Newton–Hewer refinement (spec 10.12).
use crate::lqr::{parse_lq, LqrInput};
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::linalg::{cond2, sort_poles, symmetrize};
use aeoi_core::lti::eigenvalues;
use aeoi_core::matrix::{complex_vector, dense_real};
use aeoi_core::kernels::riccati::{dare_residual, newton_hewer, sda, SdaErr};
use aeoi_core::sysalg::pbh;
use aeoi_core::{num, opt_num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

operator_struct!(Dlqr, "control.design.dlqr");

impl Operator for Dlqr {
    type Input = LqrInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(LqrInput, Vec<Warning>), OpError> {
        parse_lq(&self.meta, args, host)
    }
    fn estimate(&self, i: &LqrInput) -> Option<Value> {
        let n = i.a.nrows();
        Some(json!({"states": n, "inputs": i.b.ncols(), "sda_flops_per_step": 8 * n.pow(3)}))
    }
    fn execute(&self, inp: LqrInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let LqrInput { a, b, q, r, n: nm, has_n } = inp;
        let nx = a.nrows();
        let mut warnings = Vec::new();
        let chol = aeoi_core::kernels::nalg::cholesky(&r).unwrap();
        let rinv_bt = aeoi_core::kernels::nalg::chol_solve(&chol, &b.transpose());
        let rinv_nt = aeoi_core::kernels::nalg::chol_solve(&chol, &nm.transpose());
        let abar = &a - &b * &rinv_nt;
        let qbar = symmetrize(&(&q - &nm * &rinv_nt));
        let g = symmetrize(&(&b * &rinv_bt));
        // stabilizability: modes with |λ| ≥ 1 must be controllable
        let mut pbh_unstable: Option<f64> = None;
        for lam in eigenvalues(&a) {
            if lam.norm() >= 1.0 - 1e-10 {
                let s = pbh(&a, &b, lam);
                pbh_unstable = Some(pbh_unstable.unwrap_or(f64::INFINITY).min(s));
                if s <= 1e-10 {
                    let mut patch = vec![json!({"op": "replace", "path": "/B", "value": dense_real(&DMatrix::<f64>::identity(nx, nx))}), json!({"op": "replace", "path": "/R", "value": dense_real(&DMatrix::<f64>::identity(nx, nx))})];
                    if has_n {
                        patch.push(json!({"op": "remove", "path": "/N"}));
                    }
                    return Err(OpError::new("E.DOMAIN.NOT_STABILIZABLE", format!("(A,B) 不可镇定：特征值 λ = {:.6}{:+.6}j（|λ| ≥ 1）对应的模态不受输入影响，DARE 无稳定化解。", lam.re, lam.im), "这是模型结构问题，调 Q/R 无效。需为该模态增加执行器（修改 B），或检查 A（含 row-major 顺序）。")
                        .retryable(false).path("/B").detail("uncontrollable_eigenvalue", json!([lam.re, lam.im])).detail("pbh_min_singular_value_rel", json!(s))
                        .remedy(modify_input("add_actuator", "示例：为每个状态各加一路输入（B = I，R = I），仅用于确认问题出在执行器配置", "low", true, serde_json::Value::Array(patch)))
                        .remedy(ask_user("ask", "向用户确认 A 矩阵与执行器配置")));
                }
                if s < 1e-6 {
                    warnings.push(Warning::new("W.NUMERICAL.NEAR_UNCONTROLLABLE", format!("不稳定模态 λ = {:.4}{:+.4}j 接近不可控（PBH {s:.2e}）。", lam.re, lam.im)).path("/B"));
                }
            }
        }
        // unit-circle unobservable modes of (Q̄, Ā) ⇔ symplectic pencil eigenvalues on the unit circle
        let mut uc_min = f64::INFINITY;
        for lam in eigenvalues(&abar) {
            if (lam.norm() - 1.0).abs() <= 1e-6 {
                let s = pbh(&abar.transpose(), &qbar, lam);
                uc_min = uc_min.min(s);
                if s <= 1e-10 {
                    return Err(OpError::new("E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG", format!("Ā 在单位圆上有特征值 λ = {:.6}{:+.6}j 且该模态不被 Q 观测：辛矩阵束在单位圆上有特征值，不存在稳定化解。", lam.re, lam.im), "在 Q 中给该模态加权（例如 Q 加小正定项 εI），或检查系统是否有未被 Q 观测的单位圆极点（如积分器）。")
                        .path("/Q").detail("eigenvalue", json!([lam.re, lam.im])).detail("pbh_rel", json!(s))
                        .remedy(modify_input("regularize_q", "Q ← Q + 1e-6·I（小正则项，改变优化目标，需确认）", "medium", true, json!([{"op": "replace", "path": "/Q", "value": dense_real(&(&q + DMatrix::<f64>::identity(nx, nx) * 1e-6))}]))));
                }
            }
        }
        // SDA (shared Riccati kernel)
        let max_it = self.meta.manifest["limits"]["max_sda_iterations"].as_u64().unwrap_or(100) as usize;
        let (hk, iters, last) = sda(&abar, &g, &qbar, max_it, &|| ctx.check()).map_err(|e| match e {
            SdaErr::Abort(e) => e,
            SdaErr::Singular => OpError::new("E.NUMERICAL.SINGULAR", "SDA 迭代中 I + G·H 奇异。", "问题极度病态；尝试状态尺度归一化或在 Q 中加正则项。").path("/Q"),
            SdaErr::NoConvergence(iters, last) => OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("SDA 迭代 {iters} 步未收敛（最后相对变化 {last:.2e}）。"), "问题可能极度病态（闭环特征值接近单位圆）。尝试状态尺度归一化、在 Q 中加正则项，或增大 exec.timeout_ms。")
                .detail("iterations", json!(iters)).detail("last_relative_change", json!(last))
                .remedy(ask_user("ask", "与用户确认是否可以对模型做尺度归一化")),
        })?;
        let s = symmetrize(&hk);
        let (res, k) = dare_residual(&a, &b, &q, &r, &nm, &s).ok_or_else(|| OpError::new("E.NUMERICAL.SINGULAR", "R + BᵀSB 奇异。", "检查 R。").path("/R"))?;
        let sda_res = res.norm();
        // Newton–Hewer refinement (shared Riccati kernel)
        let (s, res, k, newton) = newton_hewer(&a, &b, &q, &r, &nm, s, res, k, &|| ctx.check())?;
        let acl = &a - &b * &k;
        let mut cl = eigenvalues(&acl);
        sort_poles(&mut cl);
        let rho = cl.iter().map(|p| p.norm()).fold(0.0, f64::max);
        if !(rho < 1.0) {
            return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("求得的解没有使闭环稳定（谱半径 {rho:.6}，数值失败）。"), "问题可能极度病态；尝试状态尺度归一化，或在 Q 中加正则项。")
                .detail("closed_loop_spectral_radius", json!(rho)).remedy(ask_user("ask", "与用户确认是否可以对模型做尺度归一化")));
        }
        let s_f = s.norm().max(f64::MIN_POSITIVE);
        let res_f = res.norm();
        let scaled = res_f / (q.norm() + s_f * (1.0 + a.norm().powi(2)) + (a.norm() * s_f * b.norm() + nm.norm()).powi(2) / r.norm().max(f64::MIN_POSITIVE)).max(f64::MIN_POSITIVE);
        if scaled > 1e-8 {
            warnings.push(Warning::new("W.NUMERICAL.LARGE_RESIDUAL", format!("DARE 缩放残差 {scaled:.2e} 偏大。")).path("/A"));
        }
        let s_psd = aeoi_core::kernels::nalg::sym_eigenvalues(&s).iter().cloned().fold(f64::INFINITY, f64::min) >= -1e-10 * s_f;
        let cond_r = cond2(&r);
        let cond_s = cond2(&s);
        if cond_r > 1e8 {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("cond(R) = {cond_r:.2e}，R⁻¹ 的计算会放大误差。")).path("/R"));
        }
        // a-posteriori estimate: discrete closed-loop Lyapunov sep (min |λ_i λ̄_j − 1| — heuristic for non-normal A_cl)
        let sep = cl.iter().flat_map(|x| cl.iter().map(move |y| (x * y.conj() - 1.0).norm())).fold(f64::INFINITY, f64::min);
        let est = res_f / sep;
        let result = json!({"K": dense_real(&k), "S": dense_real(&s), "closed_loop_poles": complex_vector(&cl)});
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": "SDA：‖H_{k+1}−H_k‖_F/‖H_{k+1}‖_F ≤ 1e-15 或停滞", "final_value": last, "newton_hewer_steps": newton, "sda_residual_fro": sda_res},
            "residual": {"dare_abs_fro": res_f, "dare_rel": res_f / s_f, "dare_rel_scaled": scaled,
                         "definition": "dare_rel = ‖AᵀSA − S − (AᵀSB+N)(R+BᵀSB)⁻¹(BᵀSA+Nᵀ) + Q‖_F / ‖S‖_F；dare_rel_scaled 分母为 ‖Q‖+‖S‖(1+‖A‖²)+(‖A‖‖S‖‖B‖+‖N‖)²/‖R‖"},
            "condition": {"cond_R": num(cond_r), "cond_S": num(cond_s), "pbh_min_sv_rel_unstable_modes": opt_num(pbh_unstable), "closed_loop_spectral_radius": rho,
                          "unit_circle_unobservable_pbh_min": num(uc_min), "sep_closed_loop_eig": num(sep)},
            "error_estimate": {"S_abs_fro": num(est), "S_rel": num(est / s_f), "basis": "启发式：‖Res‖_F / min|λ_iλ̄_j − 1|（λ ∈ eig(A_cl)），A_cl 非正规时偏乐观；非严格上界"},
            "checks": {"S_symmetric": true, "S_psd": s_psd, "closed_loop_stable": true},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
