//! control.design.lqr — continuous-time LQR via matrix sign function + Newton–Kleinman refinement (spec 10.3).
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::linalg::{cond2, lyap_operator, lyap_solve, norm1, rel_asym, sort_poles, symmetrize};
use aeoi_core::lti::{eigenvalues, C64};
use aeoi_core::matrix::{complex_vector, dense_real, parse_array, to_real_matrix};
use aeoi_core::{num, opt_num, ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

pub struct Lqr {
    meta: OperatorMeta,
}

impl Lqr {
    pub fn new() -> Self {
        Lqr {
            meta: OperatorMeta::from_json(
                include_str!("../../../examples/control.design.lqr/manifest.json"),
                include_str!("../../../examples/control.design.lqr/input.schema.json"),
                include_str!("../../../examples/control.design.lqr/output.schema.json"),
            ),
        }
    }
}

impl Default for Lqr {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LqrInput {
    pub a: DMatrix<f64>,
    pub b: DMatrix<f64>,
    pub q: DMatrix<f64>,
    pub r: DMatrix<f64>,
    pub n: DMatrix<f64>,
    pub has_n: bool,
}

fn shape_err(name: &str, want: String, got: &DMatrix<f64>) -> OpError {
    OpError::new("E.VALIDATION.SHAPE", format!("{name} 的形状应为 {want}，得到 [{}, {}]。", got.nrows(), got.ncols()), "检查维度；注意 data 按 row-major 展平（MATLAB 数据需先转置再展平）。")
        .path(&format!("/{name}"))
}

impl Operator for Lqr {
    type Input = LqrInput;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }

    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(LqrInput, Vec<Warning>), OpError> {
        let mut warnings = Vec::new();
        let get = |k: &str, w: &mut Vec<Warning>| -> Result<DMatrix<f64>, OpError> {
            let p = format!("/{k}");
            let arr = parse_array(&args[k], &p, host, false)?;
            to_real_matrix(&arr, &p, w, self.meta.max_dense(&p))
        };
        let a = get("A", &mut warnings)?;
        let b = get("B", &mut warnings)?;
        let mut q = get("Q", &mut warnings)?;
        let mut r = get("R", &mut warnings)?;
        let (nx, m) = (a.nrows(), b.ncols());
        if a.ncols() != nx || nx == 0 {
            return Err(shape_err("A", "[n, n]（n ≥ 1）".into(), &a));
        }
        let max_n = self.meta.manifest["limits"]["max_states"].as_u64().unwrap() as usize;
        if nx > max_n {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("n = {nx} 超过该算子上限 {max_n}。"), "降阶后再设计，或等待支持大规模 Riccati 的算子。").path("/A"));
        }
        if b.nrows() != nx || m == 0 {
            return Err(shape_err("B", format!("[{nx}, m]（m ≥ 1）"), &b));
        }
        if q.shape() != (nx, nx) {
            return Err(shape_err("Q", format!("[{nx}, {nx}]"), &q));
        }
        if r.shape() != (m, m) {
            return Err(shape_err("R", format!("[{m}, {m}]"), &r));
        }
        let has_n = !args["N"].is_null();
        let nmat = if has_n { get("N", &mut warnings)? } else { DMatrix::zeros(nx, m) };
        if nmat.shape() != (nx, m) {
            return Err(shape_err("N", format!("[{nx}, {m}]"), &nmat));
        }
        let tol = args.get("symmetry_tolerance").and_then(|x| x.as_f64()).unwrap_or(1e-10);
        for (name, mat) in [("Q", &mut q), ("R", &mut r)] {
            let asym = rel_asym(mat);
            if asym > tol {
                let s = symmetrize(mat);
                return Err(OpError::new("E.VALIDATION.NOT_SYMMETRIC", format!("{name} 不对称：‖X−Xᵀ‖_F/‖X‖_F = {asym:.3e} > {tol:.1e}。"), "权重矩阵必须对称。若这是录入错误（常见：行列顺序弄反），请修正；或使用对称化后的矩阵。")
                    .path(&format!("/{name}"))
                    .detail("relative_asymmetry", json!(asym))
                    .remedy(modify_input("symmetrize", &format!("用 ({name}+{name}ᵀ)/2 替换"), "medium", true, json!([{"op": "replace", "path": format!("/{name}"), "value": dense_real(&s)}]))));
            } else if asym > 0.0 {
                *mat = symmetrize(mat);
                warnings.push(Warning::new("W.VALIDATION.SYMMETRIZED", format!("{name} 有 {asym:.2e} 的相对不对称（在容差内），已对称化。")).path(&format!("/{name}")));
            }
        }
        if r.clone().cholesky().is_none() {
            let ev = r.clone().symmetric_eigenvalues();
            return Err(OpError::new("E.DOMAIN.R_NOT_POSITIVE_DEFINITE", format!("R 不是正定矩阵（最小特征值 {:.3e}）。LQR 要求每个控制输入都有正的代价。", ev.min()), "R 必须对称正定，例如 R = ρI（ρ > 0）。调 Q 无法解决该问题。")
                .path("/R")
                .detail("min_eigenvalue", json!(ev.min()))
                .remedy(modify_input("identity_r", "用单位阵作为 R（随后再按需调权重）", "low", true, json!([{"op": "replace", "path": "/R", "value": dense_real(&DMatrix::<f64>::identity(m, m))}])))
                .remedy(ask_user("ask", "向用户确认控制量权重")));
        }
        // joint weight PSD
        let mut w = DMatrix::zeros(nx + m, nx + m);
        w.view_mut((0, 0), (nx, nx)).copy_from(&q);
        w.view_mut((0, nx), (nx, m)).copy_from(&nmat);
        w.view_mut((nx, 0), (m, nx)).copy_from(&nmat.transpose());
        w.view_mut((nx, nx), (m, m)).copy_from(&r);
        let wmin = w.clone().symmetric_eigenvalues().min();
        if wmin < -1e-10 * w.norm().max(1e-300) {
            return Err(OpError::new("E.DOMAIN.WEIGHT_NOT_PSD", format!("代价权重 [[Q,N],[Nᵀ,R]] 不是半正定（最小特征值 {wmin:.3e}），代价函数无下界。"), "检查 Q（需半正定）与交叉项 N（过大会破坏半正定）。")
                .path(if has_n { "/N" } else { "/Q" })
                .detail("min_eigenvalue", json!(wmin))
                .remedy(ask_user("ask", "向用户确认权重矩阵")));
        }
        Ok((LqrInput { a, b, q, r, n: nmat, has_n }, warnings))
    }

    fn estimate(&self, i: &LqrInput) -> Option<Value> {
        let n = i.a.nrows();
        Some(json!({"states": n, "inputs": i.b.ncols(), "sign_iteration_flops_per_step": 2 * (2 * n).pow(3), "refinement": n <= 32}))
    }

    fn execute(&self, inp: LqrInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let LqrInput { a, b, q, r, n: nm, has_n: inp_has_n } = inp;
        let (nx, m) = (a.nrows(), b.ncols());
        let mut warnings = Vec::new();
        let chol = r.clone().cholesky().unwrap();
        let rinv_bt = chol.solve(&b.transpose()); // R⁻¹Bᵀ
        let rinv_nt = chol.solve(&nm.transpose()); // R⁻¹Nᵀ
        let abar = &a - &b * &rinv_nt;
        let qbar = symmetrize(&(&q - &nm * &rinv_nt));
        let g = symmetrize(&(&b * &rinv_bt));
        // --- stabilizability (PBH) ---
        let ab_norm = (a.norm().powi(2) + b.norm().powi(2)).sqrt().max(1e-300);
        let a_eigs = eigenvalues(&a);
        let mut pbh_unstable: Option<f64> = None;
        let mut pbh_all = f64::INFINITY;
        for lam in &a_eigs {
            let mut mm = DMatrix::<C64>::zeros(nx, nx + m);
            mm.view_mut((0, 0), (nx, nx)).copy_from(&(a.map(|x| C64::new(x, 0.0)) - DMatrix::<C64>::identity(nx, nx) * *lam));
            mm.view_mut((0, nx), (nx, m)).copy_from(&b.map(|x| C64::new(x, 0.0)));
            let smin = mm.svd(false, false).singular_values.min() / ab_norm;
            pbh_all = pbh_all.min(smin);
            if lam.re >= -1e-10 * a.norm().max(1.0) {
                pbh_unstable = Some(pbh_unstable.unwrap_or(f64::INFINITY).min(smin));
                if smin <= 1e-10 {
                    return Err(OpError::new(
                        "E.DOMAIN.NOT_STABILIZABLE",
                        format!("(A,B) 不可镇定：特征值 λ = {:.6}{:+.6}j（Re ≥ 0）对应的模态不受输入影响，不存在使闭环稳定的状态反馈，CARE 无稳定化解。", lam.re, lam.im),
                        "这是模型结构问题，调整 Q/R 无效。需要为该模态增加执行器（修改 B），或确认 A 是否录入有误（含行列顺序：row-major）。",
                    )
                    .retryable(false)
                    .path("/B")
                    .detail("uncontrollable_eigenvalue", json!([lam.re, lam.im]))
                    .detail("pbh_min_singular_value_rel", json!(smin))
                    .detail("pbh_test", json!("σ_min([A−λI, B]) / ‖[A, B]‖_F ≤ 1e-10"))
                    .remedy({
                        let mut patch = vec![
                            json!({"op": "replace", "path": "/B", "value": dense_real(&DMatrix::<f64>::identity(nx, nx))}),
                            json!({"op": "replace", "path": "/R", "value": dense_real(&DMatrix::<f64>::identity(nx, nx))}),
                        ];
                        if inp_has_n {
                            patch.push(json!({"op": "remove", "path": "/N"}));
                        }
                        modify_input("add_actuator", "示例：为每个状态各加一路输入（B = I，R = I），仅用于确认问题出在执行器配置", "low", true, Value::Array(patch))
                    })
                    .remedy(ask_user("ask", "向用户确认 A 矩阵与执行器配置")));
                }
                if smin < 1e-6 {
                    warnings.push(Warning::new("W.NUMERICAL.NEAR_UNCONTROLLABLE", format!("不稳定模态 λ = {:.4}{:+.4}j 接近不可控（PBH 相对最小奇异值 {smin:.2e}），所需增益可能极大。", lam.re, lam.im)).path("/B"));
                }
            }
        }
        // --- Hamiltonian ---
        let mut h = DMatrix::zeros(2 * nx, 2 * nx);
        h.view_mut((0, 0), (nx, nx)).copy_from(&abar);
        h.view_mut((0, nx), (nx, nx)).copy_from(&(-&g));
        h.view_mut((nx, 0), (nx, nx)).copy_from(&(-&qbar));
        h.view_mut((nx, nx), (nx, nx)).copy_from(&(-abar.transpose()));
        let hn = norm1(&h).max(1e-300);
        let h_eigs = eigenvalues(&h);
        let min_re = h_eigs.iter().map(|l| l.re.abs()).fold(f64::INFINITY, f64::min);
        if min_re <= 1e3 * f64::EPSILON * hn {
            return Err(OpError::new(
                "E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG",
                format!("Hamiltonian 矩阵在虚轴上（或数值上极接近虚轴）有特征值（min|Re λ| / ‖H‖₁ = {:.2e}），等价于 (Q, A) 在虚轴上有不可观模态，不存在稳定化解。", min_re / hn),
                "在 Q 中给这些模态加权（例如 Q 加一个小的正定项 εI），或检查系统是否有纯虚极点未被 Q 观测。",
            )
            .path("/Q")
            .detail("min_abs_real_rel", json!(min_re / hn))
            .remedy(modify_input("regularize_q", "Q ← Q + 1e-6·I（小正则项，改变优化目标，需确认）", "medium", true, json!([{"op": "replace", "path": "/Q", "value": dense_real(&(&q + DMatrix::<f64>::identity(nx, nx) * 1e-6))}]))));
        }
        // --- matrix sign function (Newton with determinantal scaling) ---
        let mut z = h.clone();
        let mut scaling = true;
        let mut iters = 0;
        let mut last_diff = f64::INFINITY;
        let max_it = self.meta.manifest["limits"]["max_sign_iterations"].as_u64().unwrap_or(100) as usize;
        let mut converged = false;
        while iters < max_it {
            ctx.check()?;
            iters += 1;
            let lu = z.clone().lu();
            let zinv = lu.try_inverse().ok_or_else(|| {
                OpError::new("E.NUMERICAL.SINGULAR", "符号函数迭代中矩阵奇异。", "检查 Hamiltonian 是否有接近零的特征值（系统接近不可镇定/不可检测）；可在 Q 中加小正则项。").path("/Q")
            })?;
            let c = if scaling {
                let u = z.clone().lu().u();
                let logdet: f64 = (0..2 * nx).map(|i| u[(i, i)].abs().ln()).sum();
                (-logdet / (2 * nx) as f64).exp()
            } else {
                1.0
            };
            let znew = (&z * c + &zinv * (1.0 / c)) * 0.5;
            let diff = norm1(&(&znew - &z)) / norm1(&znew).max(1e-300);
            z = znew;
            if diff < 1e-2 {
                scaling = false;
            }
            if diff <= 1e-13 || (diff >= last_diff && last_diff <= 1e-9) {
                converged = true;
                last_diff = diff.min(last_diff);
                break;
            }
            last_diff = diff;
        }
        if !converged && last_diff > 1e-8 {
            return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("矩阵符号函数迭代 {iters} 步未收敛（最后相对变化 {last_diff:.2e}）。"), "问题可能极度病态（Hamiltonian 特征值接近虚轴）。尝试对状态做尺度归一化、在 Q 中加正则项，或增大 exec.timeout_ms。")
                .detail("iterations", json!(iters))
                .detail("last_relative_change", json!(last_diff))
                .remedy(ask_user("ask", "与用户确认是否可以对模型做尺度归一化")));
        }
        // stable invariant subspace: (W + I)[I; S] = 0  ->  [W12; W22+I] S = -[W11+I; W21]
        let id = DMatrix::<f64>::identity(nx, nx);
        let mut lhs = DMatrix::zeros(2 * nx, nx);
        lhs.view_mut((0, 0), (nx, nx)).copy_from(&z.view((0, nx), (nx, nx)));
        lhs.view_mut((nx, 0), (nx, nx)).copy_from(&(z.view((nx, nx), (nx, nx)) + &id));
        let mut rhs = DMatrix::zeros(2 * nx, nx);
        rhs.view_mut((0, 0), (nx, nx)).copy_from(&(-(z.view((0, 0), (nx, nx)) + &id)));
        rhs.view_mut((nx, 0), (nx, nx)).copy_from(&(-z.view((nx, 0), (nx, nx)).into_owned()));
        let svd = lhs.svd(true, true);
        let mut s = svd.solve(&rhs, 1e-14).map_err(|_| OpError::new("E.NUMERICAL.SINGULAR", "由符号函数求 S 的最小二乘问题失败。", "问题可能极度病态。"))?;
        let asym_raw = rel_asym(&s);
        s = symmetrize(&s);
        let resid = |s: &DMatrix<f64>| symmetrize(&(abar.transpose() * s + s * &abar - s * &g * s + &qbar));
        let mut res = resid(&s);
        let sign_res = res.norm();
        // --- Newton–Kleinman refinement ---
        let max_ref = self.meta.manifest["limits"]["max_states_for_refinement"].as_u64().unwrap_or(32) as usize;
        let mut newton_steps = 0;
        if nx <= max_ref {
            for _ in 0..3 {
                ctx.check()?;
                let ak = &abar - &g * &s;
                let c = -(&qbar + &s * &g * &s);
                let Some(x) = lyap_solve(&ak, &c) else { break };
                let x = symmetrize(&x);
                let rx = resid(&x);
                if rx.norm() < res.norm() {
                    s = x;
                    res = rx;
                    newton_steps += 1;
                } else {
                    break;
                }
            }
        }
        let k = chol.solve(&(b.transpose() * &s + nm.transpose()));
        let acl = &a - &b * &k;
        let mut cl = eigenvalues(&acl);
        sort_poles(&mut cl);
        let cl_stable = cl.iter().all(|p| p.re < 0.0);
        let s_eigs = s.clone().symmetric_eigenvalues();
        let s_psd = s_eigs.min() >= -1e-10 * s.norm().max(1e-300);
        if !cl_stable {
            return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", "求得的解没有使闭环稳定（数值失败）。", "问题可能极度病态；尝试对状态做尺度归一化，或在 Q 中加正则项。")
                .detail("closed_loop_max_real", json!(cl.iter().map(|p| p.re).fold(f64::NEG_INFINITY, f64::max)))
                .remedy(ask_user("ask", "与用户确认是否可以对模型做尺度归一化")));
        }
        let res_f = res.norm();
        let s_f = s.norm().max(1e-300);
        let scaled = res_f / (qbar.norm() + 2.0 * abar.norm() * s_f + g.norm() * s_f * s_f).max(1e-300);
        if scaled > 1e-8 {
            warnings.push(Warning::new("W.NUMERICAL.LARGE_RESIDUAL", format!("CARE 缩放残差 {scaled:.2e} 偏大，结果精度可能不足。")).path("/A"));
        }
        // first-order a-posteriori error estimate via sep of the closed-loop Lyapunov operator
        let (sep, est) = if nx <= 20 {
            let l = lyap_operator(&(&abar - &g * &s));
            let sv = l.svd(false, false).singular_values;
            let sep = sv.min();
            (Some(sep), if sep > 0.0 { Some(res_f / sep) } else { None })
        } else {
            (None, None)
        };
        let cond_r = cond2(&r);
        let cond_s = cond2(&s);
        if cond_r > 1e8 {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("cond(R) = {cond_r:.2e}，R⁻¹ 的计算会放大误差。")).path("/R"));
        }
        if let Some(e) = est {
            if e / s_f > 1e-6 {
                warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("S 的相对误差估计 {:.2e} 偏大（CARE 病态）。", e / s_f)).path("/A"));
            }
        }
        let result = json!({"K": dense_real(&k), "S": dense_real(&s), "closed_loop_poles": complex_vector(&cl)});
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": "‖Z_{k+1}−Z_k‖₁/‖Z_{k+1}‖₁ ≤ 1e-13 或停滞", "final_value": last_diff,
                            "newton_kleinman_steps": newton_steps, "sign_function_residual_fro": sign_res},
            "residual": {"care_abs_fro": res_f, "care_rel": res_f / s_f, "care_rel_scaled": scaled, "S_asymmetry_before_symmetrization_rel": asym_raw,
                         "definition": "care_rel = ‖ĀᵀS+SĀ−SGS+Q̄‖_F/‖S‖_F；care_rel_scaled 分母为 ‖Q̄‖_F+2‖Ā‖_F‖S‖_F+‖G‖_F‖S‖_F²（Ā=A−BR⁻¹Nᵀ, G=BR⁻¹Bᵀ, Q̄=Q−NR⁻¹Nᵀ）"},
            "condition": {"cond_R": num(cond_r), "cond_S": num(cond_s), "pbh_min_sv_rel_unstable_modes": opt_num(pbh_unstable), "pbh_min_sv_rel_all_modes": num(pbh_all),
                          "sep_closed_loop_lyapunov": opt_num(sep), "hamiltonian_min_abs_real_rel": min_re / hn},
            "error_estimate": match est {
                Some(e) => json!({"S_abs_fro": e, "S_rel": e / s_f, "basis": "一阶后验估计：Newton 修正量 ‖δS‖_F ≈ ‖Res‖_F / σ_min(I⊗A_clᵀ + A_clᵀ⊗I)，忽略二次项；非严格上界"}),
                None => Value::Null
            },
            "checks": {"S_symmetric": asym_raw <= 1e-8, "S_psd": s_psd, "closed_loop_stable": cl_stable},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
