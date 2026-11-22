//! control.design.place — robust pole placement, KNV method 0 with conjugate-pair handling (spec 10.11).
use crate::check_states;
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::linalg::sort_poles;
use aeoi_core::lti::{check_conjugate, eigenvalues, parse_complex_list, C64};
use aeoi_core::matrix::{complex_vector, dense_real, parse_array, to_real_matrix};
use aeoi_core::sysalg::{complete_basis, pbh};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::{DMatrix, DVector};
use serde_json::{json, Value};

operator_struct!(Place, "control.design.place");

pub struct PlaceInput {
    pub a: DMatrix<f64>,
    pub b: DMatrix<f64>,
    pub poles: Vec<C64>,
    pub max_sweeps: usize,
}

fn cplx(a: &DMatrix<f64>) -> DMatrix<C64> {
    a.map(|x| C64::new(x, 0.0))
}

/// n×n unitary whose first r columns span the columns of m (complex).
fn complete_c(m: &DMatrix<C64>) -> DMatrix<C64> {
    let n = m.nrows();
    let mut aug = DMatrix::<C64>::zeros(n, m.ncols() + n);
    aug.view_mut((0, 0), (n, m.ncols())).copy_from(m);
    aug.view_mut((0, m.ncols()), (n, n)).copy_from(&DMatrix::<C64>::identity(n, n));
    aug.qr().q()
}

fn log_abs_det_normalized(x: &DMatrix<C64>) -> f64 {
    let mut xn = x.clone();
    for mut c in xn.column_iter_mut() {
        let nn = c.norm();
        if nn > 0.0 {
            c /= C64::new(nn, 0.0);
        }
    }
    let lu = xn.lu();
    let u = lu.u();
    (0..u.nrows()).map(|i| u[(i, i)].norm().ln()).sum()
}

impl Operator for Place {
    type Input = PlaceInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(PlaceInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let mut get = |k: &str| -> Result<DMatrix<f64>, OpError> {
            let p = format!("/{k}");
            let arr = parse_array(&args[k], &p, host, false)?;
            to_real_matrix(&arr, &p, &mut w, self.meta.max_dense(&p))
        };
        let a = get("A")?;
        let b = get("B")?;
        let (n, m) = (a.nrows(), b.ncols());
        let shp = |k: &str, want: String, got: &DMatrix<f64>| OpError::new("E.VALIDATION.SHAPE", format!("{k} 的形状应为 {want}，得到 [{}, {}]。", got.nrows(), got.ncols()), "检查维度；data 按 row-major 展平（MATLAB 数据需先转置再展平）。").path(&format!("/{k}"));
        if a.ncols() != n || n == 0 {
            return Err(shp("A", "[n, n]（n ≥ 1）".into(), &a));
        }
        check_states(&self.meta, n, "/A")?;
        if b.nrows() != n || m == 0 {
            return Err(shp("B", format!("[{n}, m]（m ≥ 1）"), &b));
        }
        let poles = parse_complex_list(&args["poles"], "/poles")?;
        if poles.len() != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("poles 的个数应等于状态维数 n = {n}，得到 {}。", poles.len()), "为每个状态给出一个期望极点（复极点成对给出）。").path("/poles"));
        }
        check_conjugate(&poles, "/poles")?;
        // B full column rank
        let sv = b.clone().svd(false, false).singular_values;
        let smax = sv.max();
        let rank = sv.iter().filter(|s| **s > smax * n.max(m) as f64 * f64::EPSILON).count();
        if rank < m {
            return Err(OpError::new("E.VALIDATION.RANK_DEFICIENT", format!("B 的列线性相关（rank = {rank} < m = {m}），多余的输入不提供额外自由度且使增益不唯一。"), "去掉线性相关的输入列（并相应去掉 K 的行），或合并执行器。").path("/B")
                .detail("rank", json!(rank))
                .remedy(ask_user("ask", "向用户确认执行器配置（哪些输入是冗余的）")));
        }
        // multiplicity ≤ m
        for (i, p) in poles.iter().enumerate() {
            let mult = poles.iter().filter(|q| (*q - p).norm() <= 1e-12 * p.norm().max(1.0)).count();
            if mult > m {
                return Err(OpError::new("E.DOMAIN.POLE_MULTIPLICITY", format!("极点 {}{:+}j 重复 {mult} 次，超过输入数 m = {m}：状态反馈无法给出几何重数大于 m 的特征值。", p.re, p.im), "把重复极点稍微错开（例如相差 1%），或增加输入。").path(&format!("/poles/{i}")).detail("multiplicity", json!(mult)));
            }
        }
        Ok((PlaceInput { a, b, poles, max_sweeps: args.get("max_sweeps").and_then(|x| x.as_u64()).unwrap_or(50) as usize }, w))
    }
    fn execute(&self, inp: PlaceInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let PlaceInput { a, b, poles, max_sweeps } = inp;
        let (n, m) = (a.nrows(), b.ncols());
        let mut warnings = Vec::new();
        // controllability (PBH at eigenvalues of A)
        let mut pbh_min = f64::INFINITY;
        for lam in eigenvalues(&a) {
            let s = pbh(&a, &b, lam);
            pbh_min = pbh_min.min(s);
            if s <= 1e-10 {
                return Err(OpError::new("E.DOMAIN.NOT_CONTROLLABLE", format!("(A,B) 不可控：特征值 λ = {:.6}{:+.6}j 对应的模态不受输入影响，无法任意配置极点。", lam.re, lam.im), "这是模型结构问题：为该模态增加执行器（修改 B），或检查 A（含 row-major 顺序）。若只需镇定，可考虑 LQR。").retryable(false)
                    .path("/B").detail("uncontrollable_eigenvalue", json!([lam.re, lam.im])).detail("pbh_min_singular_value_rel", json!(s))
                    .remedy(ask_user("ask", "向用户确认执行器配置与 A 矩阵")));
            }
            if s < 1e-6 {
                warnings.push(Warning::new("W.NUMERICAL.NEAR_UNCONTROLLABLE", format!("模态 λ = {:.4}{:+.4}j 接近不可控（PBH {s:.2e}），所需增益可能极大。", lam.re, lam.im)).path("/B"));
            }
        }
        // B = [U0 U1] [Z; 0]
        let qb = complete_basis(&b);
        let u0 = qb.columns(0, m).into_owned();
        let u1 = qb.columns(m, n - m).into_owned();
        let z = u0.transpose() * &b;
        // slots: real poles and the upper member of each conjugate pair
        let mut partner = vec![usize::MAX; n];
        let mut used = vec![false; n];
        for i in 0..n {
            if used[i] || poles[i].im == 0.0 {
                continue;
            }
            let tol = 1e-12 * poles[i].norm().max(1.0);
            if let Some(j) = (0..n).find(|&j| j != i && !used[j] && (poles[j] - poles[i].conj()).norm() <= tol) {
                used[i] = true;
                used[j] = true;
                partner[i] = j;
                partner[j] = i;
            }
        }
        // null-space bases S_i (n×m)
        let mut s_basis: Vec<DMatrix<C64>> = Vec::with_capacity(n);
        for i in 0..n {
            if m == n {
                s_basis.push(DMatrix::<C64>::identity(n, n));
                continue;
            }
            let lam = poles[i];
            if lam.im == 0.0 {
                let mi = u1.transpose() * (&a - DMatrix::<f64>::identity(n, n) * lam.re);
                let q = complete_basis(&mi.transpose());
                s_basis.push(cplx(&q.columns(n - m, m).into_owned()));
            } else if partner[i] < i {
                let sc = s_basis[partner[i]].map(|c| c.conj());
                s_basis.push(sc);
            } else {
                let mi = cplx(&u1.transpose()) * (cplx(&a) - DMatrix::<C64>::identity(n, n) * lam);
                let q = complete_c(&mi.adjoint());
                s_basis.push(q.columns(n - m, m).into_owned());
            }
        }
        // initial X: k-th repetition of a pole takes the k-th basis vector
        let mut x = DMatrix::<C64>::zeros(n, n);
        for i in 0..n {
            if partner[i] < i {
                continue;
            }
            let col = if m == n {
                // B invertible: any X works; choose a unitary X (cond = 1): e_i for real poles,
                // (e_i + j·e_partner)/√2 for a conjugate pair (so M = XΛX⁻¹ is real)
                let mut v = DVector::<C64>::zeros(n);
                if partner[i] == usize::MAX {
                    v[i] = C64::new(1.0, 0.0);
                } else {
                    v[i] = C64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0);
                    v[partner[i]] = C64::new(0.0, std::f64::consts::FRAC_1_SQRT_2);
                }
                v
            } else {
                let rep = (0..i).filter(|&j| (poles[j] - poles[i]).norm() <= 1e-12 * poles[i].norm().max(1.0)).count();
                s_basis[i].column(rep % m).into_owned()
            };
            x.set_column(i, &col);
            if partner[i] != usize::MAX {
                x.set_column(partner[i], &col.map(|c| c.conj()));
            }
        }
        let mut ld = log_abs_det_normalized(&x);
        let mut sweeps = 0;
        let mut converged = m == 1 || m == n;
        while !converged && sweeps < max_sweeps {
            ctx.check()?;
            sweeps += 1;
            for i in 0..n {
                if partner[i] < i {
                    continue;
                }
                let mut others = DMatrix::<C64>::zeros(n, n - 1);
                let mut c = 0;
                for j in 0..n {
                    if j != i {
                        others.set_column(c, &x.column(j));
                        c += 1;
                    }
                }
                let q = complete_c(&others);
                let y = q.column(n - 1).into_owned();
                let s = &s_basis[i];
                let newx: DVector<C64> = if poles[i].im == 0.0 {
                    let wv = s.adjoint() * &y;
                    let wr = wv.map(|c| c.re);
                    let wi = wv.map(|c| c.im);
                    let g = &wr * wr.transpose() + &wi * wi.transpose();
                    let eig = g.symmetric_eigen();
                    let k = eig.eigenvalues.imax();
                    let cvec = eig.eigenvectors.column(k).into_owned();
                    let sr = s.map(|c| c.re);
                    cplx(&DMatrix::from_column_slice(n, 1, (sr * cvec).as_slice())).column(0).into_owned()
                } else {
                    s * (s.adjoint() * &y)
                };
                let nn = newx.norm();
                if nn > 1e-300 {
                    let v = newx / C64::new(nn, 0.0);
                    x.set_column(i, &v);
                    if partner[i] != usize::MAX {
                        x.set_column(partner[i], &v.map(|c| c.conj()));
                    }
                }
            }
            let ld_new = log_abs_det_normalized(&x);
            if (ld_new - ld).abs() < 1e-6 {
                converged = true;
            }
            ld = ld_new;
        }
        // M = X Λ X⁻¹ (real), K = Z⁻¹ U0ᵀ (A − M)
        let lam = DMatrix::<C64>::from_diagonal(&DVector::from_vec(poles.clone()));
        let xl = &x * &lam;
        let mt = x.transpose().lu().solve(&xl.transpose()).ok_or_else(|| OpError::new("E.NUMERICAL.SINGULAR", "特征向量矩阵 X 奇异，无法构造闭环矩阵。", "极点过于接近或系统接近不可控；把重复/相近的极点错开后重试。").path("/poles"))?;
        let mc = mt.transpose();
        let mnorm = mc.norm().max(f64::MIN_POSITIVE);
        let imag_rel = mc.iter().map(|c| c.im.abs()).fold(0.0, f64::max) / mnorm;
        let mreal = mc.map(|c| c.re);
        let k = z.lu().solve(&(u0.transpose() * (&a - &mreal))).ok_or_else(|| OpError::new("E.NUMERICAL.SINGULAR", "Z（B 的 QR 因子）奇异。", "检查 B 是否列满秩。").path("/B"))?;
        let mut achieved = eigenvalues(&(&a - &b * &k));
        sort_poles(&mut achieved);
        let mut want = poles.clone();
        sort_poles(&mut want);
        // matched relative error
        let mut usedw = vec![false; n];
        let mut place_err: f64 = 0.0;
        for p in &achieved {
            let (mut bi, mut bd) = (0, f64::INFINITY);
            for (j, q) in want.iter().enumerate() {
                if !usedw[j] && (p - q).norm() < bd {
                    bd = (p - q).norm();
                    bi = j;
                }
            }
            usedw[bi] = true;
            place_err = place_err.max(bd / want[bi].norm().max(1.0));
        }
        let mut xn = x.clone();
        for mut c in xn.column_iter_mut() {
            let nn = c.norm();
            c /= C64::new(nn, 0.0);
        }
        let svx = xn.svd(false, false).singular_values;
        let cond_x = svx.max() / svx.min().max(f64::MIN_POSITIVE);
        if place_err > 1e-6 {
            warnings.push(Warning::new("W.NUMERICAL.PLACEMENT_INACCURATE", format!("达成极点与期望极点的最大相对偏差 {place_err:.2e}（问题病态：cond(X) = {cond_x:.2e}）。")).path("/poles"));
        }
        if cond_x > 1e8 {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("闭环特征向量矩阵条件数 {cond_x:.2e} 很大：闭环极点对扰动极敏感（鲁棒性差）。考虑放慢期望极点或增加输入。")).path("/poles"));
        }
        let result = json!({"K": dense_real(&k), "achieved_poles": complex_vector(&achieved)});
        let trust = json!({
            "convergence": {"applicable": m > 1 && m < n, "converged": converged, "iterations": sweeps, "criterion": "一次扫描后 ln|det X̂|（列归一化）变化 < 1e-6", "final_value": ld},
            "residual": {"pole_placement_max_rel": place_err, "M_imag_rel": imag_rel, "definition": "eig(A−BK) 与期望极点最近匹配的最大相对差（以 max(1,|p|) 归一）；M_imag_rel = max|Im(XΛX⁻¹)|/‖·‖_F"},
            "condition": {"cond_X_eigvec": num(cond_x), "pbh_min_rel_sv": num(pbh_min), "K_norm_fro": k.norm(), "inputs": m},
            "error_estimate": {"poles_rel": num(place_err.max(f64::EPSILON * cond_x)), "basis": "max(后验极点偏差, u·cond(X))；闭环极点的一阶扰动界 ∝ cond(X)·‖ΔA‖（Bauer–Fike）"},
            "checks": {"poles_placed": place_err <= 1e-6, "conjugate_closed": true},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
