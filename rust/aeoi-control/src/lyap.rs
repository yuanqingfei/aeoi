//! control.math.lyap — continuous / discrete Lyapunov equations via complex-Schur Bartels–Stewart (spec 10.10).
use crate::check_states;
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::linalg::{rel_asym, symmetrize};
use aeoi_core::lti::eigenvalues;
use aeoi_core::matrix::{dense_real, parse_array, to_real_matrix};
use aeoi_core::sysalg::{lyap_bs, lyap_residual, LyapKind};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

operator_struct!(Lyap, "control.math.lyap");

pub struct LyapInput {
    pub a: DMatrix<f64>,
    pub q: DMatrix<f64>,
    pub kind: LyapKind,
    pub q_symmetric: bool,
}

impl Operator for Lyap {
    type Input = LyapInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(LyapInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let mut get = |k: &str| -> Result<DMatrix<f64>, OpError> {
            let p = format!("/{k}");
            let arr = parse_array(&args[k], &p, host, false)?;
            to_real_matrix(&arr, &p, &mut w, self.meta.max_dense(&p))
        };
        let a = get("A")?;
        let q = get("Q")?;
        let n = a.nrows();
        if a.ncols() != n || n == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 必须是非空方阵，得到 [{}, {}]。", a.nrows(), a.ncols()), "检查维度；data 按 row-major 展平。").path("/A"));
        }
        check_states(&self.meta, n, "/A")?;
        if q.shape() != (n, n) {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("Q 的形状应为 [{n}, {n}]，得到 [{}, {}]。", q.nrows(), q.ncols()), "Q 与 A 同形。").path("/Q"));
        }
        let kind = if args.get("type").and_then(|x| x.as_str()) == Some("discrete") { LyapKind::Discrete } else { LyapKind::Continuous };
        let q_symmetric = rel_asym(&q) <= 1e-14;
        Ok((LyapInput { a, q, kind, q_symmetric }, w))
    }
    fn estimate(&self, i: &LyapInput) -> Option<Value> {
        let n = i.a.nrows();
        Some(json!({"states": n, "flops_estimate": 25 * n.pow(3) * 4}))
    }
    fn execute(&self, inp: LyapInput, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let LyapInput { a, q, kind, q_symmetric } = inp;
        let disc = kind == LyapKind::Discrete;
        let (mut x, info) = lyap_bs(&a, &q, kind).map_err(|sep| {
            if sep.is_nan() {
                // Schur iteration did not converge within its bounded iteration budget (incl. fallbacks)
                return OpError::new("E.NUMERICAL.NO_CONVERGENCE", "Schur 分解（QR 迭代，含确定性回退）未在迭代上限内收敛，无法求解 Lyapunov 方程。", "检查 A 的数值尺度（极大/极小元素、NaN 附近的值）；可先对 A 做平衡或缩放后重试。")
                    .path("/A")
                    .remedy(ask_user("ask", "向用户确认 A 的数值是否合理（尺度、单位）"));
            }
            OpError::new(
                "E.DOMAIN.LYAP_SINGULAR",
                if disc { format!("离散 Lyapunov 方程奇异：A 存在特征值对满足 λ_i·λ̄_j ≈ 1（min|λ_i λ̄_j − 1| = {sep:.2e}），解不存在或不唯一。") } else { format!("连续 Lyapunov 方程奇异：A 存在特征值对满足 λ_i + λ̄_j ≈ 0（min|λ_i + λ̄_j| = {sep:.2e}），例如虚轴上的特征值或关于虚轴对称的特征值对。") },
                "检查 A 是否有位于稳定边界上的特征值（连续：虚轴；离散：单位圆）；若是 Gramian 计算，请先确认系统稳定。",
            )
            .retryable(false)
            .path("/A")
            .detail("separation_min", json!(sep))
            .remedy(ask_user("ask", "向用户确认 A（含 row-major 顺序）与方程类型（continuous/discrete）"))
        })?;
        let asym_raw = rel_asym(&x);
        if q_symmetric {
            x = symmetrize(&x);
        }
        let (res, scale) = lyap_residual(&a, &q, &x, kind);
        let rel = res / scale.max(f64::MIN_POSITIVE);
        let eig = eigenvalues(&a);
        let stable = if disc { eig.iter().all(|l| l.norm() < 1.0) } else { eig.iter().all(|l| l.re < 0.0) };
        if !stable {
            warnings.push(Warning::new("W.DOMAIN.UNSTABLE_SYSTEM", "A 不是稳定矩阵：解存在且唯一，但不再是 Gramian/能量意义下的半正定解。").path("/A"));
        }
        let anorm = a.norm().max(f64::MIN_POSITIVE);
        let sep_rel = info.separation / if disc { anorm.max(1.0).powi(2) } else { anorm };
        // heuristic forward error: residual / separation (exact for normal A)
        let est = res / info.separation;
        let xn = x.norm().max(f64::MIN_POSITIVE);
        if sep_rel < 1e-8 {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("相对分离度 {sep_rel:.2e} 很小（特征值接近使方程奇异的位置），解对扰动敏感。")).path("/A"));
        }
        if rel > 1e-10 {
            warnings.push(Warning::new("W.NUMERICAL.LARGE_RESIDUAL", format!("相对残差 {rel:.2e} 偏大。")).path("/A"));
        }
        let x_psd = if q_symmetric { Some(x.clone().symmetric_eigenvalues().min() >= -1e-10 * xn) } else { None };
        let result = json!({"X": dense_real(&x)});
        let mut checks = json!({"A_stable": stable, "X_symmetric": q_symmetric || asym_raw <= 1e-12});
        if let Some(p) = x_psd {
            checks["X_psd"] = json!(p);
        }
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"abs_fro": res, "rel": rel, "X_asymmetry_before_symmetrization_rel": asym_raw, "schur_backward_rel": info.schur_residual,
                         "definition": if disc { "rel = ‖A X Aᵀ − X + Q‖_F / (‖A‖_F²‖X‖_F + ‖X‖_F + ‖Q‖_F)" } else { "rel = ‖A X + X Aᵀ + Q‖_F / (2‖A‖_F‖X‖_F + ‖Q‖_F)" }},
            "condition": {"separation_min": num(info.separation), "separation_rel": num(sep_rel), "definition": if disc { "min_{i,j}|λ_i λ̄_j − 1|" } else { "min_{i,j}|λ_i + λ̄_j|" }},
            "error_estimate": {"X_abs_fro": num(est), "X_rel": num(est / xn), "basis": "启发式：‖残差‖_F / sep_min；A 为正规矩阵时 sep_min 即真实分离度，非正规时真实 sep 可能更小（估计偏乐观）"},
            "checks": checks,
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
