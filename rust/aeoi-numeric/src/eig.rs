//! linalg.decomp.eig — eigenvalues and eigenvectors of a real square matrix (spec 10.22, v0.4.11).
//!
//! structure = auto (default): A bit-identical to Aᵀ → cyclic two-sided Jacobi (rotate (p, q) while
//! |a_pq| > ε·sqrt(|a_pp·a_qq|), at most 60 sweeps), real eigenvalues ascending, orthonormal real
//! eigenvectors. (nalgebra's SymmetricEigen left a backward error of 3e-13 on a graded matrix, 1000·u.) Otherwise (or structure = general): real Schur form
//! (aeoi_core::lti::schur_robust) → complex Schur form (MATLAB rsf2csf: one Givens rotation per 2×2 block,
//! the eigenvalue with positive imaginary part placed first) → eigenvectors by back-substitution on the upper
//! triangular factor, left eigenvectors by forward substitution (small divisors raised to
//! max(ε·|λ|, smlnum) as in LAPACK ztrevc). Eigenvalues of a 2×2 block are computed from the real block, so
//! conjugate pairs are exactly conjugate; the second vector of a pair is the conjugate of the first.
//! Ordering (author's decision 2026-11-25): symmetric ascending; general by (Re, |Im|) ascending, conjugate
//! pairs adjacent with Im > 0 first. Normalisation: ‖v‖₂ = 1, the entry of largest modulus (first on ties)
//! real and positive.
use crate::linsolve::real_rows;
use aeoi_core::error::{OpError, Warning};
use aeoi_core::kernels::eig::*;
use aeoi_core::lti::C64;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

operator_struct!(Eig, "linalg.decomp.eig");

const U: f64 = f64::EPSILON / 2.0;
/// Eigenvalue condition number above which the matrix is reported as possibly defective (≈ u^(-1/2)).
pub const COND_WARN: f64 = 1e8;

pub struct In {
    n: usize,
    a: Vec<f64>,
    force_general: bool,
    vectors: bool,
}

fn cvec(v: &[C64]) -> Vec<Value> {
    v.iter().map(|c| json!([num(c.re), num(c.im)])).collect()
}

impl Operator for Eig {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let (m, n, a, one_d) = real_rows(&args["A"], "/A", host)?;
        if one_d || m != n || n == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("A 的形状是 [{m}{}]；特征值要求非空方阵 [n, n]。", if one_d { String::new() } else { format!(", {n}") }), "检查 A 的形状；非方阵请用 linalg.decomp.svd。").path("/A/shape"));
        }
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        if n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("A 是 {n}×{n}，超过单次上限 {lim}（稠密 O(n³)）。"), "大型稀疏矩阵的部分特征值需要另设算子（eigs）。").path("/A/shape"));
        }
        let force_general = args.get("structure").and_then(|v| v.as_str()) == Some("general");
        let vectors = args.get("vectors").and_then(|v| v.as_bool()).unwrap_or(true);
        Ok((In { n, a, force_general, vectors }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        let am = DMatrix::from_row_slice(n, n, &i.a);
        let afro = am.norm().max(f64::MIN_POSITIVE);
        let symmetric = !i.force_general && (0..n).all(|r| (0..r).all(|c| i.a[r * n + c] == i.a[c * n + r]));
        let threshold = 100.0 * n as f64 * U;
        let mut warnings = Vec::new();
        let no_conv = || OpError::new("E.NUMERICAL.NO_CONVERGENCE", "迭代（对称：Jacobi 扫描；一般：Schur 的 QR 迭代，含确定性回退）未在上限内收敛。", "检查 A 的数值尺度（极大/极小元素）；可先缩放后重试。").path("/A");
        let (result, trust) = if symmetric {
            let (ev, evec) = sym_jacobi(&i.a, n, &|| ctx.check())?.ok_or_else(no_conv)?;
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&x, &y| ev[x].partial_cmp(&ev[y]).unwrap().then(x.cmp(&y)));
            let vals: Vec<f64> = order.iter().map(|&k| ev[k]).collect();
            let mut vecs: Vec<Vec<f64>> = order.iter().map(|&k| evec[k].clone()).collect();
            for v in vecs.iter_mut() {
                normalise_real(v);
            }
            ctx.check()?;
            let mut res = Vec::with_capacity(n);
            for (k, v) in vecs.iter().enumerate() {
                let r: f64 = (0..n).map(|row| {
                    let av: f64 = (0..n).map(|c| i.a[row * n + c] * v[c]).sum();
                    let d = av - vals[k] * v[row];
                    d * d
                }).sum::<f64>().sqrt();
                res.push(r);
            }
            let mut orth = 0.0f64;
            for p in 0..n {
                for q in 0..=p {
                    let d: f64 = (0..n).map(|r| vecs[p][r] * vecs[q][r]).sum::<f64>() - if p == q { 1.0 } else { 0.0 };
                    orth = orth.max(d.abs());
                }
            }
            let gaps: Vec<f64> = (0..n).map(|k| (0..n).filter(|&j| j != k).map(|j| (vals[j] - vals[k]).abs()).fold(f64::INFINITY, f64::min)).collect();
            let worst = res.iter().fold(0.0f64, |m, r| m.max(*r)) / afro;
            let finite = vals.iter().all(|v| v.is_finite()) && vecs.iter().flatten().all(|v| v.is_finite());
            let mut data = vec![0.0; n * n];
            for (k, v) in vecs.iter().enumerate() {
                for r in 0..n {
                    data[r * n + k] = v[r];
                }
            }
            let result = json!({
                "values": {"dtype": "f64", "shape": [n], "data": vals.iter().map(|v| num(*v)).collect::<Vec<_>>()},
                "vectors": if i.vectors { json!({"dtype": "f64", "shape": [n, n], "data": data.iter().map(|v| num(*v)).collect::<Vec<_>>()}) } else { Value::Null },
                "structure": "symmetric",
                "ordering": "ascending",
            });
            let trust = json!({
                "convergence": {"applicable": false},
                "residual": {"backward_error": num(worst), "threshold": num(threshold), "definition": "max_k ‖A·v_k − λ_k·v_k‖₂ / ‖A‖_F", "orthonormality": num(orth)},
                "condition": {"eigenvalue_condition": vec![1.0; n], "note": "对称矩阵的特征值条件数都是 1"},
                "error_estimate": {"value_abs": res.iter().map(|v| num(*v)).collect::<Vec<_>>(), "vector_angle": res.iter().zip(&gaps).map(|(r, g)| num(if g.is_finite() { (r / g).min(std::f64::consts::FRAC_PI_2) } else { 0.0 })).collect::<Vec<_>>(),
                    "basis": "|λ̂_k − λ| ≤ ‖r_k‖₂（对称矩阵的残差界）；特征向量夹角 sinθ ≲ ‖r_k‖₂ / gap_k（Davis–Kahan，gap_k 为到其余特征值的最小距离；重特征值时 gap = 0，向量只确定到子空间）"},
                "checks": {"residual_small": worst <= threshold, "orthonormal": orth <= threshold, "all_finite": finite},
                "accuracy_grade": "B",
                "credentials": Value::Null,
                "derived": {"orthonormal_applies": true, "symmetry": "A 与 Aᵀ 逐位相等"}
            });
            (result, trust)
        } else {
            let g = general(&am).ok_or_else(no_conv)?;
            ctx.check()?;
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|&x, &y| {
                let (a, b) = (g.values[x], g.values[y]);
                a.re.partial_cmp(&b.re).unwrap().then(a.im.abs().partial_cmp(&b.im.abs()).unwrap()).then((a.im < 0.0).cmp(&(b.im < 0.0))).then(x.cmp(&y))
            });
            let vals: Vec<C64> = order.iter().map(|&k| g.values[k]).collect();
            let vecs: Vec<&Vec<C64>> = order.iter().map(|&k| &g.vectors[k]).collect();
            let cond: Vec<f64> = order.iter().map(|&k| g.cond[k]).collect();
            let ac: DMatrix<C64> = am.map(|x| C64::new(x, 0.0));
            let res: Vec<f64> = vecs.iter().zip(&vals).map(|(v, l)| {
                (0..n).map(|r| {
                    let av = (0..n).fold(C64::new(0.0, 0.0), |s, c| s + ac[(r, c)] * v[c]);
                    (av - l * v[r]).norm_sqr()
                }).sum::<f64>().sqrt()
            }).collect();
            let worst = res.iter().fold(0.0f64, |m, r| m.max(*r)) / afro;
            let finite = vals.iter().all(|v| v.re.is_finite() && v.im.is_finite()) && vecs.iter().all(|v| v.iter().all(|z| z.re.is_finite() && z.im.is_finite())) && cond.iter().all(|c| c.is_finite());
            let (kmax, cmax) = cond.iter().enumerate().fold((0usize, 0.0f64), |(bk, bc), (k, c)| if *c > bc { (k, *c) } else { (bk, bc) });
            if !(cmax <= COND_WARN) {
                warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("特征值 {kmax}（{:.6e}{:+.6e}i）的条件数 ≈ {cmax:.3e} > {COND_WARN:.0e}：A 可能是亏损矩阵（重特征值只有一个特征向量）。这时特征值可能只有约一半有效数字，相应特征向量接近平行，V 接近奇异。", vals[kmax].re, vals[kmax].im)).path("/A").details(json!({"index": kmax, "eigenvalue_condition": num(cmax), "threshold": COND_WARN})));
            }
            let mut data = vec![C64::new(0.0, 0.0); n * n];
            for (k, v) in vecs.iter().enumerate() {
                for r in 0..n {
                    data[r * n + k] = v[r];
                }
            }
            let result = json!({
                "values": {"dtype": "c128", "shape": [n], "data": cvec(&vals)},
                "vectors": if i.vectors { json!({"dtype": "c128", "shape": [n, n], "data": cvec(&data)}) } else { Value::Null },
                "structure": "general",
                "ordering": "real_ascending_then_abs_imag_conjugate_pairs_positive_first",
            });
            let trust = json!({
                "convergence": {"applicable": false},
                "residual": {"backward_error": num(worst), "threshold": num(threshold), "definition": "max_k ‖A·v_k − λ_k·v_k‖₂ / ‖A‖_F"},
                "condition": {"eigenvalue_condition": cond.iter().map(|c| num(*c)).collect::<Vec<_>>(), "definition": "1/|yᴴx|（x、y 为单位右、左特征向量）"},
                "error_estimate": {"value_abs": res.iter().zip(&cond).map(|(r, c)| if *c <= COND_WARN { num(r * c) } else { Value::Null }).collect::<Vec<_>>(), "vector_angle": Value::Null,
                    "basis": "一阶估计 |Δλ_k| ≲ cond_k·‖r_k‖₂；cond_k > 1e8（可能亏损）时一阶估计失效，记为 null：误差可达 ‖A‖·u^(1/m)（m 为 Jordan 块阶数）"},
                "checks": {"residual_small": worst <= threshold, "orthonormal": true, "all_finite": finite},
                "accuracy_grade": "B",
                "credentials": Value::Null,
                "derived": {"orthonormal_applies": false, "note": "一般矩阵的特征向量不正交，orthonormal 不适用，按空真报告（规范 IP-6、开放问题 26）"}
            });
            (result, trust)
        };
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn call(args: Value) -> Value {
        Eig::new().call(&args, &HostCtx::default())
    }
    fn mat(n: usize, d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [n, n], "data": d})
    }
    #[test]
    fn symmetric_2x2() {
        let e = call(json!({"A": mat(2, &[2., 1., 1., 2.])}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["structure"], "symmetric");
        let v: Vec<f64> = e["result"]["values"]["data"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        assert!((v[0] - 1.0).abs() < 1e-15 && (v[1] - 3.0).abs() < 1e-15, "{v:?}");
    }
    #[test]
    fn rotation_conjugate_pair() {
        let e = call(json!({"A": mat(2, &[0., -1., 1., 0.])}));
        assert_eq!(e["status"], "ok", "{e}");
        let d = &e["result"]["values"]["data"];
        assert_eq!(d[0][1].as_f64().unwrap(), 1.0, "{d}");
        assert_eq!(d[1][1].as_f64().unwrap(), -1.0);
        assert_eq!(d[0][0], d[1][0]);
        // second vector is the conjugate of the first
        let v = &e["result"]["vectors"]["data"];
        for r in 0..2 {
            assert_eq!(v[r * 2][0], v[r * 2 + 1][0]);
            assert_eq!(v[r * 2][1].as_f64().unwrap(), -v[r * 2 + 1][1].as_f64().unwrap());
        }
    }
    #[test]
    fn jordan_block_warns() {
        let e = call(json!({"A": mat(3, &[2., 1., 0., 0., 2., 1., 0., 0., 2.])}));
        let codes: Vec<&str> = e["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        assert!(codes.contains(&"W.NUMERICAL.ILL_CONDITIONED"), "{e}");
        assert_eq!(e["trust"]["checks"]["residual_small"], true, "{}", e["trust"]);
    }
    #[test]
    fn general_random_residual() {
        let n = 12;
        let d: Vec<f64> = (0..n * n).map(|k| (((k * 7919) % 101) as f64 - 50.0) / 17.0).collect();
        let e = call(json!({"A": mat(n, &d)}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["trust"]["checks"]["residual_small"], true);
    }
    #[test]
    fn not_square() {
        let e = call(json!({"A": {"dtype": "f64", "shape": [2, 3], "data": [1., 2., 3., 4., 5., 6.]}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
    }
}
