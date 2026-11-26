//! control.design.lqr — continuous-time LQR via matrix sign function + K-form Newton–Kleinman defect
//! correction with compensated residuals; scale-independent (balanced) imaginary-axis test (spec 10.3).
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::linalg::{cond2, lyap_operator, norm1, rel_asym, sort_poles, symmetrize};
use aeoi_core::kernels::sum::Dot2;
use aeoi_core::sysalg::{lyap_bs, pbh, LyapKind};
use aeoi_core::lti::{eigenvalues, C64};
use aeoi_core::matrix::{complex_vector, dense_real, parse_array, to_real_matrix};
use aeoi_core::{num, opt_num, ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use nalgebra::{DMatrix, DVector};
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

/// Shared LQ input parsing/validation (lqr and dlqr): shapes, symmetry, R ≻ 0, joint weight ⪰ 0.
pub(crate) fn parse_lq(meta: &OperatorMeta, args: &Value, host: &HostCtx) -> Result<(LqrInput, Vec<Warning>), OpError> {
    let mut warnings = Vec::new();
    let get = |k: &str, w: &mut Vec<Warning>| -> Result<DMatrix<f64>, OpError> {
        let p = format!("/{k}");
        let arr = parse_array(&args[k], &p, host, false)?;
        to_real_matrix(&arr, &p, w, meta.max_dense(&p))
    };
    let a = get("A", &mut warnings)?;
    let b = get("B", &mut warnings)?;
    let mut q = get("Q", &mut warnings)?;
    let mut r = get("R", &mut warnings)?;
    let (nx, m) = (a.nrows(), b.ncols());
    if a.ncols() != nx || nx == 0 {
        return Err(shape_err("A", "[n, n]（n ≥ 1）".into(), &a));
    }
    let max_n = meta.manifest["limits"]["max_states"].as_u64().unwrap() as usize;
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
    if aeoi_core::kernels::nalg::cholesky(&r).is_none() {
        let ev = DVector::from_vec(aeoi_core::kernels::nalg::sym_eigenvalues(&r));
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
    let wmin = aeoi_core::kernels::nalg::sym_eigenvalues(&w).iter().cloned().fold(f64::INFINITY, f64::min);
    if wmin < -1e-10 * w.norm().max(1e-300) {
        return Err(OpError::new("E.DOMAIN.WEIGHT_NOT_PSD", format!("代价权重 [[Q,N],[Nᵀ,R]] 不是半正定（最小特征值 {wmin:.3e}），代价函数无下界。"), "检查 Q（需半正定）与交叉项 N（过大会破坏半正定）。")
            .path(if has_n { "/N" } else { "/Q" })
            .detail("min_eigenvalue", json!(wmin))
            .remedy(ask_user("ask", "向用户确认权重矩阵")));
    }
    Ok((LqrInput { a, b, q, r, n: nmat, has_n }, warnings))
}

impl Operator for Lqr {
    type Input = LqrInput;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }

    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(LqrInput, Vec<Warning>), OpError> {
        parse_lq(&self.meta, args, host)
    }

    fn estimate(&self, i: &LqrInput) -> Option<Value> {
        let n = i.a.nrows();
        Some(json!({"states": n, "inputs": i.b.ncols(), "sign_iteration_flops_per_step": 2 * (2 * n).pow(3), "refinement": n <= self.meta.manifest["limits"]["max_states_for_refinement"].as_u64().unwrap_or(200) as usize}))
    }

    fn execute(&self, inp: LqrInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let LqrInput { a, b, q, r, n: nm, has_n: inp_has_n } = inp;
        let (nx, m) = (a.nrows(), b.ncols());
        let mut warnings = Vec::new();
        let chol = aeoi_core::kernels::nalg::cholesky(&r).unwrap();
        let rinv_bt = aeoi_core::kernels::nalg::chol_solve(&chol, &b.transpose()); // R⁻¹Bᵀ
        let rinv_nt = aeoi_core::kernels::nalg::chol_solve(&chol, &nm.transpose()); // R⁻¹Nᵀ
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
            let smin = aeoi_core::kernels::nalg::csingular_values(&mm).iter().cloned().fold(f64::INFINITY, f64::min) / ab_norm;
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
        // --- imaginary-axis test on the symplectically balanced Hamiltonian (scale-independent, X-4a) ---
        // state scaling x = D x̃ with D = diag(2^k) (exact in binary): Ã = D⁻¹ĀD, G̃ = D⁻¹GD⁻¹, Q̃ = DQ̄D, H̃ = diag(D⁻¹, D)·H·diag(D, D⁻¹)
        let d = symplectic_balance(&abar, &g, &qbar);
        let ab = DMatrix::from_fn(nx, nx, |i, j| abar[(i, j)] * d[j] / d[i]);
        let gb = DMatrix::from_fn(nx, nx, |i, j| g[(i, j)] / (d[i] * d[j]));
        let qb = DMatrix::from_fn(nx, nx, |i, j| qbar[(i, j)] * (d[i] * d[j]));
        let mut h = DMatrix::zeros(2 * nx, 2 * nx);
        h.view_mut((0, 0), (nx, nx)).copy_from(&ab);
        h.view_mut((0, nx), (nx, nx)).copy_from(&(-&gb));
        h.view_mut((nx, 0), (nx, nx)).copy_from(&(-&qb));
        h.view_mut((nx, nx), (nx, nx)).copy_from(&(-ab.transpose()));
        let hn = norm1(&h).max(1e-300);
        let h_eigs = eigenvalues(&h);
        let min_re = h_eigs.iter().map(|l| l.re.abs()).fold(f64::INFINITY, f64::min);
        let imag_err = |why: &str, ratio: f64| {
            OpError::new(
                "E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG",
                format!("Hamiltonian 矩阵在虚轴上（或数值上极接近虚轴）有特征值（平衡后 min|Re λ| / ‖H̃‖₁ = {ratio:.2e}；{why}），等价于 (Q, A) 在虚轴上有不可观模态，不存在稳定化解。"),
                "在 Q 中给这些模态加权（例如 Q 加一个小的正定项 εI），或检查系统是否有纯虚极点未被 Q 观测。",
            )
            .path("/Q")
            .detail("min_abs_real_rel", json!(ratio))
            .detail("basis", json!(why))
            .remedy(modify_input("regularize_q", "Q ← Q + 1e-6·I（小正则项，改变优化目标，需确认）", "medium", true, json!([{"op": "replace", "path": "/Q", "value": dense_real(&(&q + DMatrix::<f64>::identity(nx, nx) * 1e-6))}])))
        };
        // Near-axis eigenvalue of the balanced H is only a suspicion: confirm it directly (PBH observability of
        // (Q̄, Ā) at near-axis eigenvalues of Ā), otherwise solve and decide post hoc on the closed loop.
        let imag_suspect = min_re <= 1e3 * f64::EPSILON * hn;
        if imag_suspect {
            let an = abar.norm().max(1e-300);
            for lam in eigenvalues(&abar) {
                if lam.re.abs() <= 1e-6 * an {
                    let smin = pbh(&abar.transpose(), &qbar, lam.conj());
                    if smin <= 1e-10 {
                        return Err(imag_err(&format!("Ā 的特征值 {:.6}{:+.6}j 在 Q̄ 下不可观（PBH 相对最小奇异值 {smin:.2e}）", lam.re, lam.im), min_re / hn)
                            .detail("unobservable_eigenvalue", json!([lam.re, lam.im])));
                    }
                }
            }
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("平衡后 Hamiltonian 特征值接近虚轴（min|Re λ| / ‖H̃‖₁ = {:.2e}），已求解并事后检验闭环稳定性。", min_re / hn)).path("/Q"));
        }
        let sign = |h: &DMatrix<f64>| -> Result<(DMatrix<f64>, usize, f64), OpError> {
            // --- matrix sign function (Newton with determinantal scaling) on the unbalanced H ---
            // (the balanced H̃ is used only for the imaginary-axis test: in the parity runs the sign iteration on H̃
            // lost stabilizability on a 100-state, 3-input case that the unbalanced iteration + Newton–Kleinman solves)
            let mut z = h.clone();
            let mut scaling = true;
            let mut iters = 0;
            let mut last_diff = f64::INFINITY;
            let max_it = self.meta.manifest["limits"]["max_sign_iterations"].as_u64().unwrap_or(100) as usize;
            let mut converged = false;
            while iters < max_it {
                ctx.check()?;
                iters += 1;
                let zinv = aeoi_core::kernels::nalg::inverse(&z).ok_or_else(|| {
                    OpError::new("E.NUMERICAL.SINGULAR", "符号函数迭代中矩阵奇异。", "检查 Hamiltonian 是否有接近零的特征值（系统接近不可镇定/不可检测）；可在 Q 中加小正则项。").path("/Q")
                })?;
                let c = if scaling {
                    let u = aeoi_core::kernels::nalg::lu_u(&z);
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
            let sb = aeoi_core::kernels::nalg::svd_solve(&lhs, &rhs, 1e-14).ok_or_else(|| OpError::new("E.NUMERICAL.SINGULAR", "由符号函数求 S 的最小二乘问题失败。", "问题可能极度病态。"))?;
            Ok((sb, iters, last_diff))
        };
        let h0 = {
            let mut h0 = DMatrix::zeros(2 * nx, 2 * nx);
            h0.view_mut((0, 0), (nx, nx)).copy_from(&abar);
            h0.view_mut((0, nx), (nx, nx)).copy_from(&(-&g));
            h0.view_mut((nx, 0), (nx, nx)).copy_from(&(-&qbar));
            h0.view_mut((nx, nx), (nx, nx)).copy_from(&(-abar.transpose()));
            h0
        };
        let (mut s, iters, last_diff) = match sign(&h0) {
            Ok(v) => v,
            Err(e) if imag_suspect && e.code.starts_with("E.NUMERICAL") => return Err(imag_err("符号函数迭代失败", min_re / hn).detail("numerical_failure", json!(e.code))),
            Err(e) => return Err(e),
        };
        let asym_raw = rel_asym(&s);
        s = symmetrize(&s);
        let resid = |s: &DMatrix<f64>| symmetrize(&(abar.transpose() * s + s * &abar - s * &g * s + &qbar));
        let sign_res = resid(&s).norm();
        // --- Newton–Kleinman refinement in K-form, as defect correction (original coordinates, N explicit) ---
        // K = R⁻¹(BᵀX + Nᵀ) by Cholesky; E(X) = (A−BK)ᵀX + X(A−BK) + Q − NK − KᵀNᵀ + KᵀRK is the CARE residual
        // (stationary in K, so the cond(R) error of K enters only quadratically), evaluated in compensated
        // arithmetic; ΔX solves (A−BK)ᵀΔX + ΔX(A−BK) + E = 0 (Bartels–Stewart), X ← X + ΔX — algebraically the
        // Newton step, but the Lyapunov solve now only has to get the small correction right.
        // Stop when ‖ΔX‖_F ≤ 2ε‖X‖_F or when ‖ΔX‖ stops decreasing below 1e-8‖X‖_F (rounding floor; that step is not applied).
        let gain = |x: &DMatrix<f64>| aeoi_core::kernels::nalg::chol_solve(&chol, &(b.transpose() * x + nm.transpose()));
        let hurwitz = |x: &DMatrix<f64>| eigenvalues(&(&a - &b * gain(x))).iter().all(|p| p.re < 0.0);
        let max_ref = self.meta.manifest["limits"]["max_states_for_refinement"].as_u64().unwrap_or(200) as usize;
        let max_nk = self.meta.manifest["limits"]["max_newton_iterations"].as_u64().unwrap_or(20) as usize;
        let mut newton_steps = 0;
        let mut nk_change = f64::NAN;
        let mut res_dd = f64::NAN;
        // low-order part of S: one more (unapplied) Newton correction once S is at its f64 rounding floor,
        // so that S + S_lo ≈ S* beyond f64 — used only to form an accurate K
        let mut s_lo = DMatrix::<f64>::zeros(nx, nx);
        let zero = DMatrix::<f64>::zeros(nx, nx);
        if nx <= max_ref && hurwitz(&s) {
            let mut x = s.clone();
            let mut prev = f64::INFINITY;
            for _ in 0..max_nk {
                ctx.check()?;
                // accurate K keeps the quadratic term (K − R⁻¹P)ᵀR(K − R⁻¹P) ~ (cond(R)·u)²·‖KᵀRK‖ out of E
                let k = gain_refined(&chol, &r, &b, &nm, &x, &zero);
                let e = care_residual_kform(&a, &b, &q, &r, &nm, &x, &k);
                res_dd = e.norm();
                let ak = &a - &b * &k;
                let Ok((dx, _)) = lyap_bs(&ak.transpose(), &e, LyapKind::Continuous) else { break };
                let dx = symmetrize(&dx);
                let dn = dx.norm();
                // ‖ΔX‖ may grow in the first (global) Newton steps; only near the rounding floor does a
                // non-decreasing correction mean "converged" (that correction is then not applied)
                if !dn.is_finite() || (dn >= prev && dn <= 1e-8 * x.norm()) {
                    break;
                }
                x = symmetrize(&(&x + &dx));
                newton_steps += 1;
                nk_change = dn / x.norm().max(1e-300);
                prev = dn;
                if nk_change <= 2.0 * f64::EPSILON {
                    break;
                }
            }
            if newton_steps > 0 {
                if hurwitz(&x) {
                    let kx = gain_refined(&chol, &r, &b, &nm, &x, &zero);
                    let e = care_residual_kform(&a, &b, &q, &r, &nm, &x, &kx);
                    res_dd = e.norm();
                    if let Ok((dx, _)) = lyap_bs(&(&a - &b * &kx).transpose(), &e, LyapKind::Continuous) {
                        let dx = symmetrize(&dx);
                        if dx.norm() <= 1e-12 * x.norm() {
                            s_lo = dx;
                        }
                    }
                    s = x;
                } else {
                    newton_steps = 0;
                    nk_change = f64::NAN;
                    res_dd = f64::NAN;
                }
            }
        }
        let res = resid(&s);
        let k = gain_refined(&chol, &r, &b, &nm, &s, &s_lo);
        let acl = &a - &b * &k;
        let mut cl = eigenvalues(&acl);
        sort_poles(&mut cl);
        let cl_stable = cl.iter().all(|p| p.re < 0.0);
        let s_eigs = DVector::from_vec(aeoi_core::kernels::nalg::sym_eigenvalues(&s));
        let s_psd = s_eigs.min() >= -1e-10 * s.norm().max(1e-300);
        if imag_suspect {
            let margin = cl.iter().map(|p| -p.re).fold(f64::INFINITY, f64::min);
            if !cl_stable || margin <= 1e3 * f64::EPSILON * norm1(&acl).max(1e-300) {
                return Err(imag_err(&format!("求解后闭环最大实部 {:.2e}，没有稳定裕度", -margin), min_re / hn));
            }
        }
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
            let sv = aeoi_core::kernels::nalg::singular_values(&l);
            let sep = sv.iter().cloned().fold(f64::INFINITY, f64::min);
            // compensated residual after refinement (the f64 residual sits at its rounding floor), plus the f64 representation error of S
            let r_est = if res_dd.is_finite() { res_dd } else { res_f };
            (Some(sep), if sep > 0.0 { Some(r_est / sep + 0.5 * f64::EPSILON * s_f) } else { None })
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
                            "newton_kleinman_steps": newton_steps, "care_residual_compensated_fro": opt_num(if res_dd.is_finite() { Some(res_dd) } else { None }), "newton_kleinman_final_change_rel": opt_num(if nk_change.is_nan() { None } else { Some(nk_change) }), "sign_function_residual_fro": sign_res,
                            "symplectic_balancing_log2_range": [d.iter().map(|v| v.log2()).fold(f64::INFINITY, f64::min), d.iter().map(|v| v.log2()).fold(f64::NEG_INFINITY, f64::max)]},
            "residual": {"care_abs_fro": res_f, "care_rel": res_f / s_f, "care_rel_scaled": scaled, "S_asymmetry_before_symmetrization_rel": asym_raw,
                         "definition": "care_rel = ‖ĀᵀS+SĀ−SGS+Q̄‖_F/‖S‖_F；care_rel_scaled 分母为 ‖Q̄‖_F+2‖Ā‖_F‖S‖_F+‖G‖_F‖S‖_F²（Ā=A−BR⁻¹Nᵀ, G=BR⁻¹Bᵀ, Q̄=Q−NR⁻¹Nᵀ）"},
            "condition": {"cond_R": num(cond_r), "cond_S": num(cond_s), "pbh_min_sv_rel_unstable_modes": opt_num(pbh_unstable), "pbh_min_sv_rel_all_modes": num(pbh_all),
                          "sep_closed_loop_lyapunov": opt_num(sep), "hamiltonian_min_abs_real_rel": min_re / hn, "hamiltonian_balanced": true},
            "error_estimate": match est {
                Some(e) => json!({"S_abs_fro": e, "S_rel": e / s_f, "basis": "一阶后验估计：Newton 修正量 ‖δS‖_F ≈ ‖Res‖_F / σ_min(I⊗A_clᵀ + A_clᵀ⊗I) + u‖S‖_F（Res 在 Newton–Kleinman 修正后用补偿算术计算，否则为 f64 残差），忽略二次项；非严格上界"}),
                None => Value::Null
            },
            "checks": {"S_symmetric": asym_raw <= 1e-8, "S_psd": s_psd, "closed_loop_stable": cl_stable},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}

/// Symplectic diagonal balancing for H = [[Ā, −G], [−Q̄, −Āᵀ]]: D = diag(d, d⁻¹) with d_i powers of two,
/// chosen by coordinate descent to minimise the entrywise ℓ₁ norm of D⁻¹HD (a convex function of log d).
/// Makes the imaginary-axis test and the sign iteration independent of the state scaling
/// (e.g. R = diag(1, 1e-12) puts 1e12 into G and only 1 into Q).
fn symplectic_balance(a: &DMatrix<f64>, g: &DMatrix<f64>, q: &DMatrix<f64>) -> Vec<f64> {
    let n = a.nrows();
    let mut d = vec![1.0f64; n];
    const MAX_LOG2: i32 = 60;
    for _sweep in 0..30 {
        let mut changed = false;
        for i in 0..n {
            // contributions of index i that scale like f, f², 1/f, 1/f² when d_i ← f·d_i
            let (mut up, mut dn) = (0.0f64, 0.0f64);
            for j in 0..n {
                if j == i {
                    continue;
                }
                up += 2.0 * (a[(j, i)] * d[i] / d[j]).abs() + 2.0 * (q[(i, j)] * d[i] * d[j]).abs();
                dn += 2.0 * (a[(i, j)] * d[j] / d[i]).abs() + 2.0 * (g[(i, j)] / (d[i] * d[j])).abs();
            }
            let up2 = (q[(i, i)] * d[i] * d[i]).abs();
            let dn2 = (g[(i, i)] / (d[i] * d[i])).abs();
            let tot = |f: f64| up * f + up2 * f * f + dn / f + dn2 / (f * f);
            let k0 = d[i].log2().round() as i32;
            let mut k = 0i32;
            while k0 + k < MAX_LOG2 && tot(2f64.powi(k + 1)) < 0.95 * tot(2f64.powi(k)) {
                k += 1;
            }
            if k == 0 {
                while k0 + k > -MAX_LOG2 && tot(2f64.powi(k - 1)) < 0.95 * tot(2f64.powi(k)) {
                    k -= 1;
                }
            }
            if k != 0 {
                d[i] *= 2f64.powi(k);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    d
}

/// CARE residual in K-form, E = AᵀX + XA + Q − KᵀP − PᵀK + KᵀRK with P = BᵀX + Nᵀ, evaluated with compensated
/// dot products (Dot2; P and RK kept as unevaluated double-f64 sums). Equals the CARE residual up to the
/// quadratic term (K − R⁻¹P)ᵀR(K − R⁻¹P).
fn care_residual_kform(a: &DMatrix<f64>, b: &DMatrix<f64>, q: &DMatrix<f64>, r: &DMatrix<f64>, nm: &DMatrix<f64>, x: &DMatrix<f64>, k: &DMatrix<f64>) -> DMatrix<f64> {
    let (n, m) = (a.nrows(), b.ncols());
    // P = BᵀX + Nᵀ and T = RK as hi/lo pairs; D = T − P (small) rounded once
    let mut ph = DMatrix::<f64>::zeros(m, n);
    let mut pl = DMatrix::<f64>::zeros(m, n);
    let mut dd = DMatrix::<f64>::zeros(m, n);
    for j in 0..n {
        for i in 0..m {
            let mut p = Dot2::default();
            for l in 0..n {
                p.add_prod(b[(l, i)], x[(l, j)]);
            }
            p.add(nm[(j, i)]);
            let (h, lo) = p.hi_lo();
            ph[(i, j)] = h;
            pl[(i, j)] = lo;
            let mut t = Dot2::default();
            for l in 0..m {
                t.add_prod(r[(i, l)], k[(l, j)]);
            }
            t.add(-h);
            t.add(-lo);
            let (th, tl) = t.hi_lo();
            dd[(i, j)] = th + tl;
        }
    }
    // E = AᵀX + XA + Q + KᵀD − PᵀK   (KᵀRK − KᵀP − PᵀK = KᵀD − PᵀK)
    let mut e = DMatrix::<f64>::zeros(n, n);
    for j in 0..n {
        for i in 0..n {
            let mut s = Dot2::default();
            for l in 0..n {
                s.add_prod(a[(l, i)], x[(l, j)]);
                s.add_prod(x[(i, l)], a[(l, j)]);
            }
            s.add(q[(i, j)]);
            for l in 0..m {
                s.add_prod(k[(l, i)], dd[(l, j)]);
                s.add_prod(-ph[(l, i)], k[(l, j)]);
                s.add_prod(-pl[(l, i)], k[(l, j)]);
            }
            let (h, lo) = s.hi_lo();
            e[(i, j)] = h + lo;
        }
    }
    symmetrize(&e)
}

/// K = R⁻¹(Bᵀ(S + S_lo) + Nᵀ) with iterative refinement (S_lo: low-order part of S kept from the last Newton step): the Cholesky solve alone has forward error ~cond(R)·u
/// (1e-4 at cond(R) = 1e12); corrections K ← K + R⁻¹(P − RK) with P and the residual in compensated arithmetic
/// reach ~u while cond(R)·u < 1. At most 4 corrections, stopping when they stop shrinking.
fn gain_refined(chol: &DMatrix<f64>, r: &DMatrix<f64>, b: &DMatrix<f64>, nm: &DMatrix<f64>, s: &DMatrix<f64>, s_lo: &DMatrix<f64>) -> DMatrix<f64> {
    let (n, m) = (s.nrows(), b.ncols());
    let mut ph = DMatrix::<f64>::zeros(m, n);
    let mut pl = DMatrix::<f64>::zeros(m, n);
    for j in 0..n {
        for i in 0..m {
            let mut p = Dot2::default();
            for l in 0..n {
                p.add_prod(b[(l, i)], s[(l, j)]);
                p.add_prod(b[(l, i)], s_lo[(l, j)]);
            }
            p.add(nm[(j, i)]);
            let (h, lo) = p.hi_lo();
            ph[(i, j)] = h;
            pl[(i, j)] = lo;
        }
    }
    let mut k = aeoi_core::kernels::nalg::chol_solve(chol, &ph);
    let mut prev = f64::INFINITY;
    for _ in 0..4 {
        let mut res = DMatrix::<f64>::zeros(m, n);
        for j in 0..n {
            for i in 0..m {
                let mut t = Dot2::default();
                t.add(ph[(i, j)]);
                t.add(pl[(i, j)]);
                for l in 0..m {
                    t.add_prod(-r[(i, l)], k[(l, j)]);
                }
                let (h, lo) = t.hi_lo();
                res[(i, j)] = h + lo;
            }
        }
        let dk = aeoi_core::kernels::nalg::chol_solve(chol, &res);
        let dn = dk.norm();
        if !dn.is_finite() || dn >= prev || dn == 0.0 {
            break;
        }
        k += dk;
        prev = dn;
        if dn <= f64::EPSILON * k.norm() {
            break;
        }
    }
    k
}
