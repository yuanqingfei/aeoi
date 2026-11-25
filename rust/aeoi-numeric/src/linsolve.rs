//! linalg.system.solve — A·X = B for dense real A (spec 10.19, v0.4.10).
//!
//! Square A: LU with partial pivoting (row interchanges, first maximal |pivot|, LAPACK dgetf2 order),
//! then at most 3 steps of fixed-precision iterative refinement with the residual computed in compensated
//! arithmetic (Dot2). Overdetermined A (m > n): Householder QR (LAPACK dgeqr2 convention), least-squares
//! solution R x = Qᵀ b. Underdetermined A (m < n): E.DOMAIN.UNDERDETERMINED (author's decision, 2026-11-25).
//! Own implementations (no nalgebra LU/QR) so that the transposed solves needed by the 1-norm condition
//! estimator (Hager/Higham, LAPACK dlacn2 without the parallel-vector heuristic) use the same factors.
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::matrix::{parse_array, Array, Elems};
use aeoi_core::kernels::dense::*;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(LinSolve, "linalg.system.solve");

const U: f64 = f64::EPSILON / 2.0;
const MAX_REFINE: usize = 3;

pub struct In {
    m: usize,
    n: usize,
    k: usize,
    a: Vec<f64>, // row-major m×n
    b: Vec<f64>, // row-major m×k
    b_vector: bool,
    refine: bool,
}

/// Row-major dense real matrix from a parsed array: (rows, cols, data, was_1d).
pub(crate) fn real_rows(v: &Value, path: &str, host: &HostCtx) -> Result<(usize, usize, Vec<f64>, bool), OpError> {
    let arr = parse_array(v, path, host, false)?;
    match arr {
        Array::Dense(d) => {
            let (r, c, one_d) = match d.shape.as_slice() {
                [r, c] => (*r, *c, false),
                [r] => (*r, 1, true),
                _ => return Err(OpError::new("E.VALIDATION.SHAPE", format!("{path} 的维数是 {}，这里只接受向量或二维矩阵。", d.shape.len()), "传入 [m, n] 矩阵或 [m] 向量。").path(path)),
            };
            let data = match &d.data {
                Elems::F64(x) => x.clone(),
                Elems::I64(x) => x.iter().map(|&t| t as f64).collect(),
                Elems::C128(_) => return Err(OpError::new("E.VALIDATION.DTYPE", format!("{path} 是复数（c128）；linalg.system.solve 0.4.0 只解实数方程组。"), "复数方程组可写成 2n 维实数方程组 [Re −Im; Im Re]，或等以后的复数版本。").path(&format!("{path}/dtype"))),
                Elems::Bool(_) => return Err(OpError::new("E.VALIDATION.DTYPE", format!("{path} 的 dtype 是 bool，需要数值。"), "把它转成 f64。").path(&format!("{path}/dtype"))),
            };
            Ok((r, c, data, one_d))
        }
        Array::Sparse(s) => {
            let dm = s.to_dense_f64();
            let mut data = Vec::with_capacity(dm.len());
            for i in 0..dm.nrows() {
                for j in 0..dm.ncols() {
                    data.push(dm[(i, j)]);
                }
            }
            Ok((dm.nrows(), dm.ncols(), data, false))
        }
    }
}

impl Operator for LinSolve {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let (m, n, a, a_1d) = real_rows(&args["A"], "/A", host)?;
        if a_1d && m > 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 是一维的（长度 {m}）；系数矩阵要写成 [m, n]。"), "按 row-major 给出 shape [m, n]；1×1 方程写 shape [1, 1]。").path("/A/shape"));
        }
        let (bm, k, b, b_1d) = real_rows(&args["b"], "/b", host)?;
        if m == 0 || n == 0 || k == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "A 或 b 是空的。", "给出非空的 A 与 b。").path("/A"));
        }
        if bm != m {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 有 {m} 行，b 有 {bm} 行；A·X = b 要求行数相同。"), "检查 b 是否需要转置（列向量写成 shape [m] 或 [m, 1]），以及 A 是否按 row-major 录入。")
                .path("/b/shape")
                .detail("A_shape", json!([m, n]))
                .detail("b_shape", json!(if b_1d { vec![bm] } else { vec![bm, k] })));
        }
        if m < n {
            return Err(OpError::new("E.DOMAIN.UNDERDETERMINED", format!("A 是 {m}×{n}（方程少于未知数），解不唯一。本算子只解方阵与超定（最小二乘）方程组。"), "补足方程（增加约束），或说明要哪一个解（例如最小范数解）；最小范数解将来作为另一个方法提供。")
                .path("/A/shape")
                .detail("m", json!(m))
                .detail("n", json!(n))
                .remedy(ask_user("which_solution", "方程不够：要补充哪些约束，或要哪一种解（最小范数、稀疏……）")));
        }
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        if m > lim || n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("A 是 {m}×{n}，超过单次上限 {lim}（稠密 O(n³) 求解）。"), "大型稀疏方程组需要迭代法或稀疏分解（另设算子）。").path("/A/shape"));
        }
        let lim_k = self.meta.manifest["limits"]["max_rhs"].as_u64().unwrap() as usize;
        if k > lim_k {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("b 有 {k} 列，超过上限 {lim_k}。"), "分批求解。").path("/b/shape"));
        }
        let refine = args.get("refinement").and_then(|v| v.as_str()).unwrap_or("auto") == "auto";
        Ok((In { m, n, k, a, b, b_vector: b_1d, refine }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n, k) = (i.m, i.n, i.k);
        let col = |v: &[f64], j: usize| -> Vec<f64> { (0..m).map(|r| v[r * k + j]).collect() };
        let mut x_all = vec![0.0f64; n * k];
        let mut warnings = Vec::new();
        let anorm1 = norm1(&i.a, m, n);
        let anorm_inf = norm_inf_mat(&i.a, m, n);
        let afro = i.a.iter().map(|v| v * v).sum::<f64>().sqrt();
        let (mut eta_worst, mut fwd_worst, mut res2): (f64, f64, Vec<f64>) = (0.0, 0.0, Vec::with_capacity(k));
        let mut refine_steps = 0usize;
        let method;
        let kappa1;
        let backward_ok;
        let threshold;
        if m == n {
            method = "lu_partial_pivoting";
            let lu = lu_factor(&i.a, n);
            if let Some(p) = lu.zero_pivot {
                return Err(OpError::new("E.NUMERICAL.SINGULAR", format!("A 奇异：LU 分解第 {p} 步的主元恰好为 0（部分选主元后整列为 0）。方程组无解或有无穷多解。"), "检查 A 的录入（重复行/列、漏填）；若模型本身奇异，需要补充约束或改用最小二乘/最小范数解。")
                    .path("/A")
                    .detail("zero_pivot_step", json!(p))
                    .remedy(ask_user("singular_model", "A 奇异：是录入错误，还是模型本身缺少约束？")));
            }
            let inv1 = inv_norm1_estimate(n, &|v| lu.solve(v), &|v| lu.solve_t(v));
            kappa1 = anorm1 * inv1;
            threshold = 10.0 * n as f64 * U;
            for j in 0..k {
                let b = col(&i.b, j);
                let mut x = lu.solve(&b);
                let mut steps = 0usize;
                if i.refine {
                    let mut prev = f64::INFINITY;
                    for _ in 0..MAX_REFINE {
                        let r = residual_dd(&i.a, m, n, &x, &b);
                        let d = lu.solve(&r);
                        let dn = norm_inf(&d);
                        if !dn.is_finite() || dn > prev / 2.0 {
                            break;
                        }
                        for (xi, di) in x.iter_mut().zip(&d) {
                            *xi += di;
                        }
                        steps += 1;
                        prev = dn;
                        if dn <= 2.0 * U * norm_inf(&x) {
                            break;
                        }
                    }
                }
                refine_steps = refine_steps.max(steps);
                let r = residual_dd(&i.a, m, n, &x, &b);
                let eta = norm_inf(&r) / (anorm_inf * norm_inf(&x) + norm_inf(&b)).max(f64::MIN_POSITIVE);
                eta_worst = eta_worst.max(eta);
                fwd_worst = fwd_worst.max(kappa1 * eta);
                res2.push(norm2(&r));
                for (r_i, v) in x.iter().enumerate() {
                    x_all[r_i * k + j] = *v;
                }
                ctx.check()?;
            }
            backward_ok = eta_worst <= threshold;
            let rcond = 1.0 / kappa1;
            if !(rcond >= f64::EPSILON) {
                warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("A 接近奇异：rcond = {rcond:.3e} < ε（κ₁ ≈ {kappa1:.3e}）。解的相对误差可能达到 κ·η ≈ {fwd_worst:.1e}，结果可能没有有效数字。")).path("/A").details(json!({"rcond": num(rcond), "kappa1_estimate": num(kappa1)})));
            }
        } else {
            method = "householder_qr";
            let qr = qr_factor(&i.a, m, n);
            let rmax = (0..n).map(|j| qr.r(j, j).abs()).fold(0.0, f64::max);
            let tol_rank = m.max(n) as f64 * U * rmax;
            if let Some(j) = (0..n).find(|&j| qr.r(j, j).abs() <= tol_rank) {
                let rank = (0..n).filter(|&j| qr.r(j, j).abs() > tol_rank).count();
                return Err(OpError::new("E.NUMERICAL.SINGULAR", format!("A（{m}×{n}）列秩不足：R 的第 {j} 个对角元 |r_jj| ≤ max(m, n)·u·max|r_ii|（估计秩 {rank} < {n}）。最小二乘解不唯一。"), "去掉线性相关的列（未知数），或改用带秩判定的最小范数解（将来作为另一方法）。")
                    .path("/A")
                    .detail("rank_estimate", json!(rank))
                    .detail("n", json!(n))
                    .remedy(ask_user("rank_deficient", "A 的列线性相关：哪些未知数应当合并或去掉？")));
            }
            let inv1 = inv_norm1_estimate(n, &|v| qr.solve_r(v), &|v| qr.solve_rt(v));
            let rnorm1 = (0..n).map(|j| (0..=j).map(|ii| qr.r(ii, j).abs()).sum::<f64>()).fold(0.0, f64::max);
            kappa1 = rnorm1 * inv1;
            threshold = 10.0 * m as f64 * U;
            for j in 0..k {
                let b = col(&i.b, j);
                let c = qr.qt(&b);
                let x = qr.solve_r(&c);
                let r = residual_dd(&i.a, m, n, &x, &b);
                let atr = at_r_dd(&i.a, m, n, &r);
                let (rn, xn) = (norm2(&r), norm2(&x));
                // optimality of the least-squares solution: ‖Aᵀr‖₂ / (‖A‖_F·(‖r‖₂ + ‖A‖_F‖x‖₂))
                let eta = norm2(&atr) / (afro * (rn + afro * xn)).max(f64::MIN_POSITIVE);
                eta_worst = eta_worst.max(eta);
                // first-order LS sensitivity (Golub–Van Loan 5.3.8): ε(2κ + κ²·‖r‖/(‖A‖‖x‖)), ε = η
                fwd_worst = fwd_worst.max(eta * (2.0 * kappa1 + kappa1 * kappa1 * rn / (afro * xn).max(f64::MIN_POSITIVE)));
                res2.push(rn);
                for (r_i, v) in x.iter().enumerate() {
                    x_all[r_i * k + j] = *v;
                }
                ctx.check()?;
            }
            backward_ok = eta_worst <= threshold;
            if !(1.0 / kappa1 >= f64::EPSILON) {
                warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("A 的列接近线性相关：κ₁(R) ≈ {kappa1:.3e}，rcond < ε。")).path("/A").details(json!({"rcond": num(1.0 / kappa1), "kappa1_estimate": num(kappa1)})));
            }
        }
        let finite = x_all.iter().all(|v| v.is_finite());
        let x_val = if i.b_vector { json!({"dtype": "f64", "shape": [n], "data": x_all.iter().map(|v| num(*v)).collect::<Vec<_>>()}) } else { json!({"dtype": "f64", "shape": [n, k], "data": x_all.iter().map(|v| num(*v)).collect::<Vec<_>>()}) };
        let res_val = if i.b_vector { num(res2[0]) } else { json!({"dtype": "f64", "shape": [k], "data": res2.iter().map(|v| num(*v)).collect::<Vec<_>>()}) };
        let result = json!({
            "x": x_val,
            "residual_norm": res_val,
            "method": method,
            "shape": {"m": m, "n": n, "rhs": k, "kind": if m == n { "square" } else { "overdetermined" }},
            "refinement_steps": refine_steps, // max over right-hand sides
        });
        let eta_def = if m == n { "‖b − A·x‖∞ / (‖A‖∞‖x‖∞ + ‖b‖∞)（范数后向误差，Rigal–Gaches）" } else { "‖Aᵀ(b − A·x)‖₂ / (‖A‖_F·(‖r‖₂ + ‖A‖_F‖x‖₂))（最小二乘最优性）" };
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"backward_error": num(eta_worst), "threshold": num(threshold), "definition": format!("{eta_def}；残差用补偿算术（Dot2）计算，取各列最大")},
            "condition": {"kappa1_estimate": num(kappa1), "rcond": num(1.0 / kappa1), "of": if m == n { "A" } else { "R（A = QR）" }, "estimator": "Hager–Higham 1-范数估计（LAPACK dlacn2 思路），下界，通常与真值相差不到 3 倍"},
            "error_estimate": {"x_rel_forward_inf": num(fwd_worst), "basis": if m == n { "一阶界 ‖Δx‖/‖x‖ ≲ κ₁·η" } else { "一阶界 η·(2κ + κ²·‖r‖/(‖A‖‖x‖))（Golub–Van Loan 5.3.8）" }},
            "checks": {"backward_stable": backward_ok, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"refinement": if i.refine { "auto" } else { "none" }, "norm1_A": num(anorm1)}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        LinSolve::new().call(&args, &HostCtx::default())
    }
    fn mat(r: usize, c: usize, d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [r, c], "data": d})
    }
    fn xs(env: &Value) -> Vec<f64> {
        env["result"]["x"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    }

    #[test]
    fn square_3x3() {
        // x = [1, -2, 3]
        let a = [4.0, -2.0, 1.0, -2.0, 4.0, -2.0, 1.0, -2.0, 4.0];
        let b = [4.0 * 1.0 + -2.0 * -2.0 + 3.0, -2.0 - 8.0 - 6.0, 1.0 + 4.0 + 12.0];
        let env = call(json!({"A": mat(3, 3, &a), "b": {"dtype": "f64", "shape": [3], "data": b}}));
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        let x = xs(&env);
        assert_eq!(x, vec![1.0, -2.0, 3.0]);
        assert_eq!(env["result"]["method"], "lu_partial_pivoting");
        assert_eq!(env["trust"]["checks"]["backward_stable"], true);
    }

    #[test]
    fn least_squares_line_fit() {
        // fit y = c0 + c1 t through (0,1), (1,3), (2,5), (3,7.5)
        let a = [1.0, 0.0, 1.0, 1.0, 1.0, 2.0, 1.0, 3.0];
        let env = call(json!({"A": mat(4, 2, &a), "b": {"dtype": "f64", "shape": [4], "data": [1.0, 3.0, 5.0, 7.5]}}));
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        let x = xs(&env);
        assert!((x[0] - 0.9).abs() < 1e-14 && (x[1] - 2.15).abs() < 1e-14, "{x:?}");
        assert_eq!(env["result"]["method"], "householder_qr");
        assert_eq!(env["trust"]["checks"]["backward_stable"], true);
    }

    #[test]
    fn errors() {
        let e = call(json!({"A": mat(2, 3, &[1.0; 6]), "b": {"dtype": "f64", "shape": [2], "data": [1.0, 2.0]}}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.UNDERDETERMINED");
        let e = call(json!({"A": mat(2, 2, &[1.0, 2.0, 2.0, 4.0]), "b": {"dtype": "f64", "shape": [2], "data": [1.0, 2.0]}}));
        assert_eq!(e["error"]["code"], "E.NUMERICAL.SINGULAR");
        let e = call(json!({"A": mat(2, 2, &[1.0, 0.0, 0.0, 1.0]), "b": {"dtype": "f64", "shape": [3], "data": [1.0, 2.0, 3.0]}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
        let e = call(json!({"A": mat(3, 2, &[1.0, 2.0, 2.0, 4.0, 3.0, 6.0]), "b": {"dtype": "f64", "shape": [3], "data": [1.0, 2.0, 3.0]}}));
        assert_eq!(e["error"]["code"], "E.NUMERICAL.SINGULAR");
    }

    #[test]
    fn hilbert_is_flagged() {
        let n = 13;
        let a: Vec<f64> = (0..n * n).map(|t| 1.0 / ((t / n + t % n + 1) as f64)).collect();
        let env = call(json!({"A": mat(n, n, &a), "b": {"dtype": "f64", "shape": [n], "data": vec![1.0; n]}}));
        assert_eq!(env["status"], "ok_with_warnings", "{}", env["error"]);
        assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ILL_CONDITIONED"));
        let k = env["trust"]["condition"]["kappa1_estimate"].as_f64().unwrap();
        assert!(k > 1e17, "κ₁(H13) ≈ 4e18, estimate {k:e}");
    }
}
