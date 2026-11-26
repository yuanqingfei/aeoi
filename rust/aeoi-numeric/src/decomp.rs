//! linalg.decomp.lu, linalg.decomp.qr, linalg.decomp.chol (spec 10.42 – 10.44; v0.4.24).
//!
//! All three expose kernels that other operators already use (RI-1): the LU of linalg.system.solve / inv / det,
//! the Householder QR of the least-squares path and polyfit (extended to m < n in v0.4.24), and the column
//! Cholesky of lqr / dlqr. Author's decisions (2026-11-26): lu returns a 0-based `perm` (A[perm, :] = L·U) and is
//! square-only in 0.4.0; qr is economy-size by default with mode = "full", and R's diagonal is made non-negative;
//! chol returns upper R by default (triangle = "lower" optional), reads the upper triangle and first checks
//! symmetry.
use crate::linsolve::real_rows;
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::dense::{cholesky_detail, lu_factor, qr_factor};
use aeoi_core::kernels::sum::Dot2;
use aeoi_core::kernels::svd::{nrm2, orth_err};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Lu, "linalg.decomp.lu");
operator_struct!(Qr, "linalg.decomp.qr");
operator_struct!(Chol, "linalg.decomp.chol");

const U: f64 = f64::EPSILON / 2.0;

pub struct DecIn {
    m: usize,
    n: usize,
    a: Vec<f64>,
    full: bool,
    lower: bool,
}

fn matrix(args: &Value, host: &HostCtx, op: &str, lim: usize, square: bool) -> Result<(usize, usize, Vec<f64>), OpError> {
    if args["A"].get("dtype").and_then(|d| d.as_str()) == Some("c128") {
        return Err(OpError::new("E.VALIDATION.DTYPE", format!("A 是复数（c128）；{op} 0.4.0 只支持实数矩阵。"), "复数版本以后作为另一个方法提供。").path("/A/dtype"));
    }
    let (m, n, a, one_d) = real_rows(&args["A"], "/A", host)?;
    if (one_d && m > 1) || m == 0 || n == 0 || (square && m != n) {
        let shape = if one_d { json!([m]) } else { json!([m, n]) };
        let need = if square { "非空方阵 [n, n]" } else { "非空矩阵 [m, n]" };
        return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 的 shape 是 {shape}；{op} 需要{need}。"), "检查 A 的录入（row-major）；行向量写 [1, n]。").path("/A/shape").detail("shape", shape));
    }
    if m.max(n) > lim {
        return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("A 是 {m}×{n}，超过单次上限 {lim}（稠密 O(n³)）。"), "大型稀疏矩阵需要稀疏分解（另设算子）。").path("/A/shape"));
    }
    Ok((m, n, a))
}

fn dense(r: usize, c: usize, v: &[f64]) -> Value {
    json!({"dtype": "f64", "shape": [r, c], "data": v.iter().map(|x| num(*x)).collect::<Vec<_>>()})
}

/// ‖X·Y − B‖_F / ‖B‖_F with every entry of X·Y − B accumulated in compensated arithmetic (Dot2).
/// X is p×q, Y is q×s, B is p×s (all row-major).
fn rel_residual(x: &[f64], y: &[f64], b: &[f64], p: usize, q: usize, s: usize) -> f64 {
    let mut e = Vec::with_capacity(p * s);
    for i in 0..p {
        for j in 0..s {
            let mut d = Dot2::default();
            for k in 0..q {
                d.add_prod(x[i * q + k], y[k * s + j]);
            }
            d.add(-b[i * s + j]);
            e.push(d.hi_lo().0);
        }
    }
    nrm2(&e) / nrm2(b).max(f64::MIN_POSITIVE)
}

fn std_trust(resid: f64, thr: f64, def: &str, mut checks: Value, extra: Value) -> Value {
    checks["reconstructs"] = json!(resid <= thr);
    let mut residual = json!({"reconstruction": num(resid), "threshold": num(thr), "definition": def});
    for (k, v) in extra.as_object().unwrap() {
        residual[k] = v.clone();
    }
    json!({"convergence": {"applicable": false}, "residual": residual,
           "error_estimate": {"basis": "因子本身的前向误差 ≲ κ(A)·u（扰动理论）；重构残差是后向误差"},
           "checks": checks, "accuracy_grade": "B", "credentials": Value::Null})
}

// ============================================================================================== lu
impl Operator for Lu {
    type Input = DecIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(DecIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a) = matrix(args, host, "linalg.decomp.lu", lim, true)?;
        Ok((DecIn { m, n, a, full: false, lower: false }, Vec::new()))
    }
    fn execute(&self, i: DecIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        let f = lu_factor(&i.a, n);
        ctx.check()?;
        let mut l = vec![0.0; n * n];
        let mut u = vec![0.0; n * n];
        for r in 0..n {
            for c in 0..n {
                let v = f.entry(r, c);
                if c < r {
                    l[r * n + c] = v;
                } else {
                    u[r * n + c] = v;
                }
            }
            l[r * n + r] = 1.0;
        }
        let perm = f.perm().to_vec();
        let pa: Vec<f64> = perm.iter().flat_map(|&p| i.a[p * n..(p + 1) * n].to_vec()).collect();
        let resid = rel_residual(&l, &u, &pa, n, n, n);
        let amax = i.a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let umax = u.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let growth = if amax > 0.0 { umax / amax } else { 1.0 };
        let mut warnings = Vec::new();
        if let Some(p) = f.zero_pivot {
            warnings.push(Warning::new("W.NUMERICAL.ZERO_PIVOT", format!("第 {p} 步的主元恰好为 0：A 奇异，U 的对角元 u_{p}{p} = 0。分解本身仍然成立（A[perm, :] = L·U），但不能用 U 回代求解。"))
                .path("/A")
                .details(json!({"zero_pivot_step": p})));
        }
        let thr = 100.0 * n as f64 * U;
        let finite = l.iter().chain(u.iter()).all(|v| v.is_finite());
        let result = json!({"L": dense(n, n, &l), "U": dense(n, n, &u), "perm": perm, "perm_sign": f.perm_sign(), "growth_factor": num(growth),
                            "zero_pivot": f.zero_pivot.map(|p| json!(p)).unwrap_or(Value::Null)});
        let trust = std_trust(resid, thr, "‖A[perm, :] − L·U‖_F / ‖A‖_F（补偿算术），阈值 100·n·u", json!({"all_finite": finite}),
                              json!({"growth_factor": num(growth), "note": "增长因子大（如 Wilkinson 矩阵的 2^(n−1)）时部分选主元不是后向稳定的，reconstructs 会如实失败"}));
        Ok(Output { result, trust, warnings })
    }
}

// ============================================================================================== qr
impl Operator for Qr {
    type Input = DecIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(DecIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a) = matrix(args, host, "linalg.decomp.qr", lim, false)?;
        let full = args.get("mode").and_then(|v| v.as_str()) == Some("full");
        let out_lim = self.meta.manifest["limits"]["max_output_elements"].as_u64().unwrap() as usize;
        let q_cols = if full { m } else { m.min(n) };
        // Q is m×q_cols, R is q_cols×n (econ: k = min(m, n) for both; full: m for both)
        if m * q_cols + q_cols * n > out_lim {
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("mode = full 时 Q 为 {m}×{m}，输出超过上限 {out_lim} 个元素。"), "改用经济型（缺省）：Q 为 [m, min(m, n)]，张成同一个列空间。")
                .path("/mode")
                .remedy(modify_input("use_econ", "改用经济型 QR", "high", false, json!([{"op": "remove", "path": "/mode"}]))));
        }
        Ok((DecIn { m, n, a, full, lower: false }, Vec::new()))
    }
    fn execute(&self, i: DecIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let k = m.min(n);
        let f = qr_factor(&i.a, m, n);
        let qc = if i.full { m } else { k };
        let rr = if i.full { m } else { k };
        // Q columns: Q·e_j
        let mut qcols: Vec<Vec<f64>> = (0..qc).map(|j| {
            let mut e = vec![0.0; m];
            e[j] = 1.0;
            f.q_apply(&e)
        }).collect();
        ctx.check()?;
        let mut r = vec![0.0; rr * n];
        for ii in 0..k {
            for j in ii..n {
                r[ii * n + j] = f.r(ii, j);
            }
        }
        // sign normalisation: R_ii ≥ 0 (exact sign flips of a row of R and the matching column of Q)
        for ii in 0..k {
            if r[ii * n + ii] < 0.0 {
                for j in ii..n {
                    r[ii * n + j] = -r[ii * n + j];
                }
                for v in qcols[ii].iter_mut() {
                    *v = -*v;
                }
            }
        }
        let mut q = vec![0.0; m * qc];
        for (j, col) in qcols.iter().enumerate() {
            for row in 0..m {
                q[row * qc + j] = col[row];
            }
        }
        let resid = rel_residual(&q, &r, &i.a, m, qc, n);
        let oe = orth_err(&qcols);
        let thr = 100.0 * m.max(n) as f64 * U;
        let finite = q.iter().chain(r.iter()).all(|v| v.is_finite());
        let result = json!({"Q": dense(m, qc, &q), "R": dense(rr, n, &r), "mode": if i.full { "full" } else { "econ" }, "sign_convention": "r_diag_nonnegative"});
        let trust = std_trust(resid, thr, "‖A − Q·R‖_F / ‖A‖_F（补偿算术），阈值 100·max(m, n)·u", json!({"orthonormal": oe <= thr, "all_finite": finite}), json!({"orthonormality": num(oe)}));
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================== chol
impl Operator for Chol {
    type Input = DecIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(DecIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a) = matrix(args, host, "linalg.decomp.chol", lim, true)?;
        let amax = a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let (mut worst, mut at) = (0.0f64, (0usize, 0usize));
        for r in 0..n {
            for c in r + 1..n {
                let d = (a[r * n + c] - a[c * n + r]).abs();
                if d > worst {
                    worst = d;
                    at = (r, c);
                }
            }
        }
        let tol = 10.0 * n as f64 * U * amax;
        if worst > tol {
            let sym: Vec<Value> = (0..n * n).map(|t| num(0.5 * (a[t] + a[(t % n) * n + t / n]))).collect();
            return Err(OpError::new("E.VALIDATION.NOT_SYMMETRIC", format!("A 不对称：|a[{}][{}] − a[{}][{}]| = {worst:.3e}，超过舍入容差 10·n·u·max|a| = {tol:.3e}。Cholesky 只对对称矩阵有定义（MATLAB 会悄悄只用上三角）。", at.0, at.1, at.1, at.0), "检查 A 的录入；如果 A 应当对称、误差来自数据，可以用 (A + Aᵀ)/2。")
                .path("/A")
                .detail("max_asymmetry", num(worst))
                .detail("at", json!([at.0, at.1]))
                .remedy(modify_input("symmetrize", "用 (A + Aᵀ)/2 代替 A（改变了数据，需要确认）", "medium", true, json!([{"op": "replace", "path": "/A/data", "value": sym}]))));
        }
        let lower = args.get("triangle").and_then(|v| v.as_str()) == Some("lower");
        Ok((DecIn { m, n, a, full: false, lower }, Vec::new()))
    }
    fn execute(&self, i: DecIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        // read the upper triangle (as MATLAB): the kernel reads the lower triangle, so give it Aᵀ
        let at: Vec<f64> = (0..n * n).map(|t| i.a[(t % n) * n + t / n]).collect();
        let l = cholesky_detail(&at, n).map_err(|(j, d)| {
            OpError::new("E.DOMAIN.NOT_POSITIVE_DEFINITE", format!("A 不是正定矩阵：第 {j} 列的主元 d = {d:.3e} ≤ 0（A 的前 {} 阶顺序主子式不正定）。半正定（奇异）矩阵也会在这里失败。", j + 1), "检查 A 是否应当正定（协方差、刚度、质量矩阵等）；只是想判断正定性或处理半正定矩阵时，用特征分解（linalg.decomp.eig）。")
                .path("/A")
                .detail("failed_column", json!(j))
                .detail("pivot", num(d))
                .remedy(ask_user("not_pd", "A 应当是正定的吗？是数据问题，还是模型本身允许半正定/不定？"))
        })?;
        ctx.check()?;
        let amax = i.a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let asym = (0..n).flat_map(|r| (r + 1..n).map(move |c| (r, c))).map(|(r, c)| (i.a[r * n + c] - i.a[c * n + r]).abs()).fold(0.0, f64::max) / amax.max(f64::MIN_POSITIVE);
        // L·Lᵀ = Aᵀ; R = Lᵀ (upper), RᵀR = L·Lᵀ
        let lt: Vec<f64> = (0..n * n).map(|t| l[(t % n) * n + t / n]).collect();
        let resid = rel_residual(&l, &lt, &at, n, n, n);
        let out = if i.lower { l.clone() } else { lt.clone() };
        let thr = 100.0 * n as f64 * U;
        let finite = out.iter().all(|v| v.is_finite());
        let result = json!({"R": dense(n, n, &out), "triangle": if i.lower { "lower" } else { "upper" }, "asymmetry": num(asym)});
        let trust = std_trust(resid, thr, "‖A − RᵀR‖_F / ‖A‖_F（A 取上三角对称化，补偿算术），阈值 100·n·u", json!({"all_finite": finite}), json!({}));
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn mat(m: usize, n: usize, d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [m, n], "data": d})
    }
    fn data(v: &Value) -> Vec<f64> {
        v["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
    }
    #[test]
    fn lu_small() {
        let e = Lu::new().call(&json!({"A": mat(2, 2, &[1., 2., 3., 4.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["perm"], json!([1, 0]));
        assert_eq!(data(&e["result"]["U"]), vec![3., 4., 0., 2. - 4. / 3.]);
        let e = Lu::new().call(&json!({"A": mat(2, 2, &[1., 2., 2., 4.])}), &HostCtx::default());
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.ZERO_PIVOT"), "{e}");
    }
    #[test]
    fn qr_shapes_and_signs() {
        let a = mat(3, 2, &[1., 2., 3., 4., 5., 6.]);
        let e = Qr::new().call(&json!({"A": a}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["Q"]["shape"], json!([3, 2]));
        let r = data(&e["result"]["R"]);
        assert!(r[0] > 0.0 && r[3] > 0.0, "{r:?}");
        let e = Qr::new().call(&json!({"A": mat(2, 3, &[1., 2., 3., 4., 5., 6.]), "mode": "full"}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["R"]["shape"], json!([2, 3]));
        assert_eq!(e["trust"]["checks"]["reconstructs"], true);
    }
    #[test]
    fn chol_cases() {
        let e = Chol::new().call(&json!({"A": mat(2, 2, &[4., 2., 2., 3.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(data(&e["result"]["R"]), vec![2., 1., 0., 2f64.sqrt()]);
        let e = Chol::new().call(&json!({"A": mat(2, 2, &[1., 2., 2., 1.])}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.NOT_POSITIVE_DEFINITE");
        let e = Chol::new().call(&json!({"A": mat(2, 2, &[1., 2., 0., 1.])}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.VALIDATION.NOT_SYMMETRIC");
    }
}
