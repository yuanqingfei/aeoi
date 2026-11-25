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
use aeoi_core::lti::{schur_robust, C64};
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

/// Normalise: ‖v‖₂ = 1, largest-modulus entry (first on ties) real positive.
fn normalise(v: &mut [C64]) {
    let nrm = v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
    if nrm == 0.0 || !nrm.is_finite() {
        return;
    }
    let (mut idx, mut best) = (0usize, -1.0f64);
    for (i, z) in v.iter().enumerate() {
        let a = z.norm();
        if a > best {
            best = a;
            idx = i;
        }
    }
    let ph = v[idx].conj() / v[idx].norm();
    for z in v.iter_mut() {
        *z = *z * ph / nrm;
    }
    v[idx] = C64::new(v[idx].norm(), 0.0); // exactly real
}

fn normalise_real(v: &mut [f64]) {
    let nrm = v.iter().map(|z| z * z).sum::<f64>().sqrt();
    if nrm == 0.0 {
        return;
    }
    let (mut idx, mut best) = (0usize, -1.0f64);
    for (i, z) in v.iter().enumerate() {
        if z.abs() > best {
            best = z.abs();
            idx = i;
        }
    }
    let s = if v[idx] < 0.0 { -1.0 } else { 1.0 } / nrm;
    for z in v.iter_mut() {
        *z *= s;
    }
}

/// Eigenvalues of the real 2×2 block [[a, b], [c, d]]: (first, second), first with Im ≥ 0.
fn block_eigs(a: f64, b: f64, c: f64, d: f64) -> (C64, C64) {
    let p = 0.5 * (a - d);
    let disc = p * p + b * c;
    let tr = 0.5 * (a + d);
    if disc < 0.0 {
        let s = (-disc).sqrt();
        (C64::new(tr, s), C64::new(tr, -s))
    } else {
        let s = disc.sqrt();
        (C64::new(tr + s, 0.0), C64::new(tr - s, 0.0))
    }
}

struct General {
    values: Vec<C64>,
    vectors: Vec<Vec<C64>>, // in A coordinates, normalised
    cond: Vec<f64>,
}

fn general(a: &DMatrix<f64>) -> Option<General> {
    let n = a.nrows();
    let (q, t) = schur_robust(a)?;
    // eigenvalues from the real quasi-triangular T (exact conjugate pairs)
    let mut lam = vec![C64::new(0.0, 0.0); n];
    let mut is_block_first = vec![false; n];
    let mut k = 0;
    while k < n {
        if k + 1 < n && t[(k + 1, k)] != 0.0 {
            let (l1, l2) = block_eigs(t[(k, k)], t[(k, k + 1)], t[(k + 1, k)], t[(k + 1, k + 1)]);
            lam[k] = l1;
            lam[k + 1] = l2;
            is_block_first[k] = true;
            k += 2;
        } else {
            lam[k] = C64::new(t[(k, k)], 0.0);
            k += 1;
        }
    }
    // rsf2csf
    let mut tc: DMatrix<C64> = t.map(|x| C64::new(x, 0.0));
    let mut uc: DMatrix<C64> = q.map(|x| C64::new(x, 0.0));
    for m in (1..n).rev() {
        if t[(m, m - 1)] != 0.0 && is_block_first[m - 1] {
            let mu = lam[m - 1] - tc[(m, m)];
            let sub = tc[(m, m - 1)];
            let r = mu.norm().hypot(sub.norm());
            let c = mu / r;
            let s = sub / r;
            // G = [c̄ s; −s c]; T(k, m−1:n) = G·T(k, m−1:n); T(0:m, k) = T(0:m, k)·Gᴴ; U(:, k) = U(:, k)·Gᴴ
            for j in m - 1..n {
                let (x1, x2) = (tc[(m - 1, j)], tc[(m, j)]);
                tc[(m - 1, j)] = c.conj() * x1 + s * x2;
                tc[(m, j)] = -s * x1 + c * x2;
            }
            for i in 0..=m {
                let (x1, x2) = (tc[(i, m - 1)], tc[(i, m)]);
                tc[(i, m - 1)] = x1 * c + x2 * s.conj();
                tc[(i, m)] = -x1 * s.conj() + x2 * c.conj();
            }
            for i in 0..n {
                let (x1, x2) = (uc[(i, m - 1)], uc[(i, m)]);
                uc[(i, m - 1)] = x1 * c + x2 * s.conj();
                uc[(i, m)] = -x1 * s.conj() + x2 * c.conj();
            }
            tc[(m, m - 1)] = C64::new(0.0, 0.0);
        }
    }
    let ulp = f64::EPSILON;
    let smlnum = f64::MIN_POSITIVE * (n as f64 / ulp);
    const BIG: f64 = 1e150;
    let mut vectors: Vec<Vec<C64>> = vec![Vec::new(); n];
    let mut cond = vec![1.0; n];
    for k in 0..n {
        if lam[k].im < 0.0 && k > 0 && is_block_first[k - 1] {
            vectors[k] = vectors[k - 1].iter().map(|z| z.conj()).collect();
            cond[k] = cond[k - 1];
            continue;
        }
        let tkk = tc[(k, k)];
        let smin = (ulp * (tkk.re.abs() + tkk.im.abs())).max(smlnum);
        // right: T[0..=k, 0..=k] x = λ x, x_k = 1
        let mut x = vec![C64::new(0.0, 0.0); k + 1];
        x[k] = C64::new(1.0, 0.0);
        for j in (0..k).rev() {
            let mut s = C64::new(0.0, 0.0);
            for l in j + 1..=k {
                s -= tc[(j, l)] * x[l];
            }
            let mut d = tc[(j, j)] - tkk;
            if d.norm() < smin {
                d = C64::new(smin, 0.0);
            }
            x[j] = s / d;
            if x[j].norm() > BIG {
                for z in x.iter_mut() {
                    *z /= BIG;
                }
            }
        }
        // left: w T = λ w on columns k.., w_k = 1
        let mut w = vec![C64::new(0.0, 0.0); n];
        w[k] = C64::new(1.0, 0.0);
        for j in k + 1..n {
            let mut s = C64::new(0.0, 0.0);
            for l in k..j {
                s -= w[l] * tc[(l, j)];
            }
            let mut d = tc[(j, j)] - tkk;
            if d.norm() < smin {
                d = C64::new(smin, 0.0);
            }
            w[j] = s / d;
            if w[j].norm() > BIG {
                for z in w.iter_mut() {
                    *z /= BIG;
                }
            }
        }
        let xn = x.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        let wn = w.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        cond[k] = xn * wn / (w[k] * x[k]).norm();
        let mut v: Vec<C64> = (0..n).map(|i| (0..=k).fold(C64::new(0.0, 0.0), |s, l| s + uc[(i, l)] * x[l])).collect();
        normalise(&mut v);
        if lam[k].im == 0.0 {
            for z in v.iter_mut() {
                z.im = 0.0;
            }
            normalise(&mut v);
        }
        vectors[k] = v;
    }
    Some(General { values: lam, vectors, cond })
}

const MAX_SWEEPS: usize = 60;

/// Cyclic Jacobi for a symmetric matrix (row-major n×n): (eigenvalues, eigenvectors as columns-lists).
fn sym_jacobi(a: &[f64], n: usize, ctx: &ExecCtx) -> Result<Option<(Vec<f64>, Vec<Vec<f64>>)>, OpError> {
    let mut a = a.to_vec();
    let mut v: Vec<Vec<f64>> = (0..n).map(|k| (0..n).map(|i| if i == k { 1.0 } else { 0.0 }).collect()).collect();
    for _ in 0..MAX_SWEEPS {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                let (app, aqq) = (a[p * n + p], a[q * n + q]);
                if apq == 0.0 || apq.abs() <= f64::EPSILON * (app.abs() * aqq.abs()).sqrt() {
                    continue;
                }
                rotated = true;
                let tau = (aqq - app) / (2.0 * apq);
                let t = (if tau >= 0.0 { 1.0 } else { -1.0 }) / (tau.abs() + 1.0f64.hypot(tau));
                let c = 1.0 / 1.0f64.hypot(t);
                let s = c * t;
                for k in 0..n {
                    if k == p || k == q {
                        continue;
                    }
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    let np = c * akp - s * akq;
                    let nq = s * akp + c * akq;
                    a[k * n + p] = np;
                    a[p * n + k] = np;
                    a[k * n + q] = nq;
                    a[q * n + k] = nq;
                }
                a[p * n + p] = app - t * apq;
                a[q * n + q] = aqq + t * apq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for i in 0..n {
                    let (x, y) = (v[p][i], v[q][i]);
                    v[p][i] = c * x - s * y;
                    v[q][i] = s * x + c * y;
                }
            }
        }
        ctx.check()?;
        if !rotated {
            return Ok(Some(((0..n).map(|k| a[k * n + k]).collect(), v)));
        }
    }
    Ok(None)
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
            let (ev, evec) = sym_jacobi(&i.a, n, ctx)?.ok_or_else(no_conv)?;
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
