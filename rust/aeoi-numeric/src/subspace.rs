//! linalg.subspace.rank, linalg.subspace.basis, linalg.matrix.pinv, linalg.matrix.norm, linalg.matrix.cond
//! (spec 10.37 – 10.41; v0.4.23).
//!
//! All SVD-based results come from kernels::svd::svd_thin (the kernel of linalg.decomp.svd) and every rank
//! decision from kernels::svd::rank_decision (RI-1). Author's decisions (2026-11-26): MATLAB's default tolerance
//! max(m, n)·eps(σ_max), overridable by `tol`; a singular value in (tol/10, 100·tol] makes the rank ambiguous
//! (W.NUMERICAL.RANK_AMBIGUOUS) and the post-condition `rank_well_separated` then guards every rank-dependent
//! field; norm and cond are two operators; null and orth come from one operator.
use crate::linsolve::real_rows;
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::kernels::dense::{inverse_refined, lu_factor, norm1, norm_inf_mat};
use aeoi_core::kernels::sum::neumaier;
use aeoi_core::kernels::svd::*;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use aeoi_libm::libm;
use serde_json::{json, Value};

operator_struct!(Rank, "linalg.subspace.rank");
operator_struct!(Basis, "linalg.subspace.basis");
operator_struct!(Pinv, "linalg.matrix.pinv");
operator_struct!(Norm, "linalg.matrix.norm");
operator_struct!(Cond, "linalg.matrix.cond");

const U: f64 = f64::EPSILON / 2.0;

pub struct MatIn {
    m: usize,
    n: usize,
    a: Vec<f64>,
    tol: Option<f64>,
    one_d: bool,
    p: P,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum P {
    Num(f64),
    Inf,
    NegInf,
    Fro,
}

impl P {
    fn json(&self) -> Value {
        match self {
            P::Num(v) => num(*v),
            P::Inf => json!("inf"),
            P::NegInf => json!("-inf"),
            P::Fro => json!("fro"),
        }
    }
}

fn parse_p(v: Option<&Value>) -> P {
    match v {
        None => P::Num(2.0),
        Some(Value::String(s)) if s == "inf" => P::Inf,
        Some(Value::String(s)) if s == "-inf" => P::NegInf,
        Some(Value::String(_)) => P::Fro,
        Some(x) => P::Num(x.as_f64().unwrap_or(2.0)),
    }
}

/// Real [m, n] matrix (or vector when `allow_1d`) at `key`, with the size limit of the manifest.
fn real_matrix(args: &Value, key: &str, host: &HostCtx, op: &str, lim: usize, allow_1d: bool) -> Result<(usize, usize, Vec<f64>, bool), OpError> {
    let path = format!("/{key}");
    if args[key].get("dtype").and_then(|d| d.as_str()) == Some("c128") {
        return Err(OpError::new("E.VALIDATION.DTYPE", format!("{key} 是复数（c128）；{op} 0.4.0 只支持实数。"), "复数版本以后作为另一个方法提供。").path(&format!("{path}/dtype")));
    }
    let (m, n, a, one_d) = real_rows(&args[key], &path, host)?;
    if one_d && !allow_1d && m > 1 {
        return Err(OpError::new("E.VALIDATION.SHAPE", format!("{key} 是一维的（长度 {m}）；要求 [m, n]。"), "行向量写 shape [1, n]，列向量写 [m, 1]。").path(&format!("{path}/shape")));
    }
    if m == 0 || n == 0 {
        return Err(OpError::new("E.VALIDATION.SHAPE", format!("{key} 是空的。"), format!("给出非空的 {key}。")).path(&format!("{path}/shape")));
    }
    if m.max(n) > lim && !(allow_1d && (one_d || m == 1 || n == 1)) {
        return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{key} 是 {m}×{n}，超过单次上限 {lim}（稠密 SVD / 求逆）。"), "大型矩阵需要迭代法或稀疏方法（另设算子）。").path(&format!("{path}/shape")));
    }
    Ok((m, n, a, one_d))
}

fn tol_arg(args: &Value) -> Option<f64> {
    args.get("tol").and_then(|v| v.as_f64())
}

fn mat_json(rows: usize, cols: &[Vec<f64>]) -> Value {
    let c = cols.len();
    let mut data = vec![0.0; rows * c];
    for (j, col) in cols.iter().enumerate() {
        for i in 0..rows {
            data[i * c + j] = col[i];
        }
    }
    json!({"dtype": "f64", "shape": [rows, c], "data": data.iter().map(|v| num(*v)).collect::<Vec<_>>()})
}

/// SVD of the input with the same checks as linalg.decomp.svd (SV-5, SV-6).
struct Pack {
    s: Vec<f64>,
    u: Vec<Vec<f64>>,
    v: Vec<Vec<f64>>,
    recon: f64,
    orth: f64,
    threshold: f64,
    afro: f64,
    e_abs: f64,
}

fn svd_pack(a: &[f64], m: usize, n: usize, ctx: &ExecCtx) -> Result<Pack, OpError> {
    let p = m.min(n);
    let afro = nrm2(a);
    let Svd { s, u, v } = svd_thin(a, m, n, &|| ctx.check())?.ok_or_else(|| {
        OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("Jacobi SVD 在 {MAX_SWEEPS} 轮扫描内未收敛。"), "检查 A 的数值尺度（极大/极小元素）；可先缩放后重试。").path("/A")
    })?;
    let mut rsq = 0.0f64;
    for r in 0..m {
        for c in 0..n {
            let a_hat: f64 = (0..p).map(|k| u[k][r] * s[k] * v[k][c]).sum();
            let d = a[r * n + c] - a_hat;
            rsq += d * d;
        }
    }
    let recon = rsq.sqrt() / afro.max(f64::MIN_POSITIVE);
    let orth = orth_err(&u).max(orth_err(&v));
    let smax = s.first().copied().unwrap_or(0.0);
    let e_abs = recon * afro + 2.0 * orth * smax;
    Ok(Pack { s, u, v, recon, orth, threshold: 100.0 * m.max(n) as f64 * U, afro, e_abs })
}

fn rank_fields(rd: &RankDecision, s: &[f64]) -> Value {
    json!({"rank": rd.rank, "tol": num(rd.tol), "tol_source": if rd.supplied { "supplied" } else { "default" },
           "s": {"dtype": "f64", "shape": [s.len()], "data": s.iter().map(|v| num(*v)).collect::<Vec<_>>()},
           "smallest_kept": rd.smallest_kept.map(num).unwrap_or(Value::Null), "largest_dropped": rd.largest_dropped.map(num).unwrap_or(Value::Null)})
}

fn ambiguous_warning(rd: &RankDecision, fields: &str) -> Warning {
    Warning::new("W.NUMERICAL.RANK_AMBIGUOUS", format!("有奇异值落在阈值 tol = {:.3e} 附近 (tol/10, 100·tol]：数值秩 {} 取决于舍入与阈值的选择，{fields}不可依赖。数据本身有噪声时，应按噪声水平给出 tol。", rd.tol, rd.rank))
        .path("/A")
        .details(json!({"tol": num(rd.tol), "rank": rd.rank, "smallest_kept": rd.smallest_kept.map(num).unwrap_or(Value::Null), "largest_dropped": rd.largest_dropped.map(num).unwrap_or(Value::Null)}))
}

fn svd_trust(pk: &Pack, extra_checks: Value, extra_residual: Value) -> Value {
    let mut checks = json!({"svd_reconstructs": pk.recon <= pk.threshold && pk.orth <= pk.threshold});
    for (k, v) in extra_checks.as_object().unwrap() {
        checks[k] = v.clone();
    }
    let mut residual = json!({"svd_reconstruction": num(pk.recon), "svd_orthonormality": num(pk.orth), "threshold": num(pk.threshold),
                              "definition": "‖A − U·diag(s)·Vᵀ‖_F/‖A‖_F 与 max(‖UᵀU − I‖, ‖VᵀV − I‖)，同 linalg.decomp.svd SV-5"});
    for (k, v) in extra_residual.as_object().unwrap() {
        residual[k] = v.clone();
    }
    json!({
        "convergence": {"applicable": false},
        "residual": residual,
        "error_estimate": {"singular_value_abs": num(pk.e_abs), "basis": "|Δσ_k| ≲ ‖A‖_F·重构残差 + 2σ_max·正交性误差（Weyl；小奇异值只有绝对精度）"},
        "checks": checks,
        "accuracy_grade": "B",
        "credentials": Value::Null
    })
}

// ============================================================================================== rank
impl Operator for Rank {
    type Input = MatIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MatIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a, one_d) = real_matrix(args, "A", host, "linalg.subspace.rank", lim, false)?;
        Ok((MatIn { m, n, a, tol: tol_arg(args), one_d, p: P::Num(2.0) }, Vec::new()))
    }
    fn execute(&self, i: MatIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let pk = svd_pack(&i.a, i.m, i.n, ctx)?;
        let rd = rank_decision(&pk.s, i.m, i.n, i.tol);
        let warnings = if rd.ambiguous { vec![ambiguous_warning(&rd, "rank ")] } else { vec![] };
        let trust = svd_trust(&pk, json!({"rank_well_separated": !rd.ambiguous}), json!({}));
        Ok(Output { result: rank_fields(&rd, &pk.s), trust, warnings })
    }
}

// ============================================================================================== basis
impl Operator for Basis {
    type Input = MatIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MatIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a, one_d) = real_matrix(args, "A", host, "linalg.subspace.basis", lim, false)?;
        Ok((MatIn { m, n, a, tol: tol_arg(args), one_d, p: P::Num(2.0) }, Vec::new()))
    }
    fn execute(&self, i: MatIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let pk = svd_pack(&i.a, m, n, ctx)?;
        let rd = rank_decision(&pk.s, m, n, i.tol);
        let r = rd.rank;
        let orth: Vec<Vec<f64>> = pk.u[..r].to_vec();
        let vfull = if pk.v.len() < n { complete(&pk.v, n) } else { pk.v.clone() };
        let null: Vec<Vec<f64>> = vfull[r..].to_vec();
        ctx.check()?;
        // ‖A·N‖_F: the null-space residual
        let mut an = 0.0f64;
        let mut terms = Vec::with_capacity(m * null.len());
        for col in &null {
            for row in 0..m {
                terms.push((0..n).map(|k| i.a[row * n + k] * col[k]).sum::<f64>());
            }
        }
        if !terms.is_empty() {
            an = nrm2(&terms);
        }
        let null_thr = ((n - r) as f64).sqrt() * rd.tol + pk.threshold * pk.afro;
        let oe = orth_err(&orth).max(orth_err(&null));
        let mut result = rank_fields(&rd, &pk.s);
        result["orth"] = mat_json(m, &orth);
        result["null"] = mat_json(n, &null);
        let warnings = if rd.ambiguous { vec![ambiguous_warning(&rd, "rank、orth、null ")] } else { vec![] };
        let trust = svd_trust(&pk, json!({"rank_well_separated": !rd.ambiguous, "orthonormal": oe <= pk.threshold, "null_residual": an <= null_thr}),
                              json!({"null_residual_fro": num(an), "null_residual_threshold": num(null_thr), "basis_orthonormality": num(oe)}));
        Ok(Output { result, trust, warnings })
    }
}

// ============================================================================================== pinv
impl Operator for Pinv {
    type Input = MatIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MatIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a, one_d) = real_matrix(args, "A", host, "linalg.matrix.pinv", lim, false)?;
        Ok((MatIn { m, n, a, tol: tol_arg(args), one_d, p: P::Num(2.0) }, Vec::new()))
    }
    fn execute(&self, i: MatIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let pk = svd_pack(&i.a, m, n, ctx)?;
        let rd = rank_decision(&pk.s, m, n, i.tol);
        let r = rd.rank;
        // X[j][i] = Σ_k v_k[j]·u_k[i]/σ_k, X is n×m
        let mut x = vec![0.0f64; n * m];
        for j in 0..n {
            for ii in 0..m {
                x[j * m + ii] = (0..r).map(|k| pk.v[k][j] * pk.u[k][ii] / pk.s[k]).sum();
            }
        }
        ctx.check()?;
        let matmul = |a: &[f64], b: &[f64], p: usize, q: usize, s: usize| -> Vec<f64> {
            let mut c = vec![0.0; p * s];
            for row in 0..p {
                for k in 0..q {
                    let av = a[row * q + k];
                    if av != 0.0 {
                        for col in 0..s {
                            c[row * s + col] += av * b[k * s + col];
                        }
                    }
                }
            }
            c
        };
        let ax = matmul(&i.a, &x, m, n, m); // m×m
        let axa = matmul(&ax, &i.a, m, m, n); // m×n
        let xa = matmul(&x, &i.a, n, m, n); // n×n
        let xax = matmul(&xa, &x, n, n, m); // n×m
        let diff = |p: &[f64], q: &[f64]| nrm2(&p.iter().zip(q).map(|(a, b)| a - b).collect::<Vec<_>>());
        let xfro = nrm2(&x);
        let p1 = diff(&axa, &i.a) / pk.afro.max(f64::MIN_POSITIVE);
        let p2 = if xfro > 0.0 { diff(&xax, &x) / xfro } else { 0.0 };
        let cond_r = if r > 0 { pk.s[0] / pk.s[r - 1] } else { f64::NAN };
        let pthr = pk.threshold * if r > 0 { cond_r.max(1.0) } else { 1.0 };
        // A·X·A − A = −Σ_{k > r} σ_k u_k v_kᵀ: the truncated part counts (it is large when tol is supplied)
        let trunc = nrm2(&pk.s[r..]) / pk.afro.max(f64::MIN_POSITIVE);
        let pthr1 = trunc + pthr;
        let warnings = if rd.ambiguous { vec![ambiguous_warning(&rd, "rank 与伪逆 X ")] } else { vec![] };
        let result = json!({"X": {"dtype": "f64", "shape": [n, m], "data": x.iter().map(|v| num(*v)).collect::<Vec<_>>()},
                            "rank": r, "tol": num(rd.tol), "tol_source": if rd.supplied { "supplied" } else { "default" },
                            "cond_r": if r > 0 { num(cond_r) } else { Value::Null }});
        let trust = svd_trust(&pk, json!({"rank_well_separated": !rd.ambiguous, "penrose": p1 <= pthr1 && p2 <= pthr}),
                              json!({"penrose_axa": num(p1), "penrose_xax": num(p2), "penrose_threshold": num(pthr), "penrose_axa_threshold": num(pthr1),
                                     "penrose_definition": "‖A·X·A − A‖_F/‖A‖_F 与 ‖X·A·X − X‖_F/‖X‖_F，阈值 100·max(m, n)·u·max(1, σ_1/σ_r)；前者另加被舍去奇异值的 ‖σ_{r+1..}‖₂/‖A‖_F"}));
        Ok(Output { result, trust, warnings })
    }
}

// ============================================================================================== norm
fn vector_norm(x: &[f64], p: P) -> (f64, f64) {
    let n = x.len() as f64;
    match p {
        P::Inf => (x.iter().fold(0.0f64, |m, v| m.max(v.abs())), 0.0),
        P::NegInf => (x.iter().fold(f64::INFINITY, |m, v| m.min(v.abs())), 0.0),
        P::Fro => (nrm2(x), (n + 2.0) * U),
        P::Num(q) if q == 2.0 => (nrm2(x), (n + 2.0) * U),
        P::Num(q) if q == 1.0 => (neumaier(x.iter().map(|v| v.abs())), 3.0 * U),
        P::Num(q) => {
            let mx = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            if mx == 0.0 {
                return (0.0, 0.0);
            }
            let s = neumaier(x.iter().map(|v| libm::pow(v.abs() / mx, q)));
            (mx * libm::pow(s, 1.0 / q), (n + 6.0) * U)
        }
    }
}

impl Operator for Norm {
    type Input = MatIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MatIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a, one_d) = real_matrix(args, "x", host, "linalg.matrix.norm", lim, true)?;
        let p = parse_p(args.get("p"));
        let vector = one_d || m == 1 || n == 1;
        if !vector && !matches!(p, P::Inf | P::Fro) && p != P::Num(1.0) && p != P::Num(2.0) {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("矩阵（{m}×{n}）的 p 只能是 1、2、\"inf\"、\"fro\"，收到 {}。一般 p 的矩阵范数没有可靠的算法（NP 难）。", p.json()), "改用 p = 2（谱范数，缺省）或其他支持的范数；要的是向量范数时，把 x 写成一维。")
                .path("/p")
                .remedy(modify_input("use_p2", "改用矩阵 2-范数（最大奇异值）", "medium", true, json!([{"op": "replace", "path": "/p", "value": 2}]))));
        }
        Ok((MatIn { m, n, a, tol: None, one_d, p }, Vec::new()))
    }
    fn execute(&self, i: MatIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let vector = i.one_d || m == 1 || n == 1;
        let (value, bound, svd_ok, svd_info) = if vector {
            let (v, b) = vector_norm(&i.a, i.p);
            (v, b, true, Value::Null)
        } else {
            match i.p {
                P::Num(q) if q == 1.0 => {
                    let v = (0..n).map(|c| neumaier((0..m).map(|r| i.a[r * n + c].abs()))).fold(0.0, f64::max);
                    (v, 3.0 * U, true, Value::Null)
                }
                P::Inf => {
                    let v = (0..m).map(|r| neumaier((0..n).map(|c| i.a[r * n + c].abs()))).fold(0.0, f64::max);
                    (v, 3.0 * U, true, Value::Null)
                }
                P::Fro => (nrm2(&i.a), ((m * n) as f64 + 2.0) * U, true, Value::Null),
                _ => {
                    let pk = svd_pack(&i.a, m, n, ctx)?;
                    let smax = pk.s[0];
                    let ok = pk.recon <= pk.threshold && pk.orth <= pk.threshold;
                    (smax, if smax > 0.0 { pk.e_abs / smax } else { 0.0 }, ok, json!({"svd_reconstruction": num(pk.recon), "svd_orthonormality": num(pk.orth), "threshold": num(pk.threshold)}))
                }
            }
        };
        if !value.is_finite() {
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", "范数超出 f64 的范围。", "先把 x 缩放（例如除以 max|x_i|），再把结果乘回去。").path("/x"));
        }
        let result = json!({"norm": num(value), "p": i.p.json(), "treated_as": if vector { "vector" } else { "matrix" }, "relative_error_bound": num(bound)});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": if svd_info.is_null() { json!({"applicable": false}) } else { svd_info.clone() },
            "error_estimate": {"norm_rel": num(bound), "basis": "补偿求和与缩放平方和的舍入界（约 n·u 或 3u）；矩阵 2-范数为 Weyl 界 / σ_max"},
            "checks": {"decomposition_checked": svd_ok, "all_finite": true},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"decomposition": if svd_info.is_null() { "无（直接公式；decomposition_checked 恒为 true）" } else { "Jacobi SVD" }}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================== cond
impl Operator for Cond {
    type Input = MatIn;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MatIn, Vec<Warning>), OpError> {
        let lim = self.meta.manifest["limits"]["max_dimension"].as_u64().unwrap() as usize;
        let (m, n, a, one_d) = real_matrix(args, "A", host, "linalg.matrix.cond", lim, false)?;
        let p = parse_p(args.get("p"));
        if p != P::Num(2.0) && m != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("p = {} 的条件数要用 A⁻¹，只对方阵有定义；A 是 {m}×{n}。", p.json()), "矩形矩阵用 p = 2（σ_max/σ_min）。")
                .path("/A/shape")
                .remedy(modify_input("use_p2", "改用 2-范数条件数（矩形矩阵也有定义）", "high", true, json!([{"op": "replace", "path": "/p", "value": 2}]))));
        }
        Ok((MatIn { m, n, a, tol: None, one_d, p }, Vec::new()))
    }
    fn execute(&self, i: MatIn, ctx: &ExecCtx) -> Result<Output, OpError> {
        let (m, n) = (i.m, i.n);
        let (cond, est, ok, resid) = if i.p == P::Num(2.0) {
            let pk = svd_pack(&i.a, m, n, ctx)?;
            let (smax, smin) = (pk.s[0], *pk.s.last().unwrap());
            let ok = pk.recon <= pk.threshold && pk.orth <= pk.threshold;
            let cond = if smin > 0.0 { smax / smin } else { f64::INFINITY };
            let est = if smin > 0.0 { pk.e_abs / smin + pk.e_abs / smax.max(f64::MIN_POSITIVE) } else { f64::INFINITY };
            (cond, est, ok, json!({"svd_reconstruction": num(pk.recon), "svd_orthonormality": num(pk.orth), "threshold": num(pk.threshold)}))
        } else {
            let lu = lu_factor(&i.a, n);
            if lu.zero_pivot.is_some() {
                (f64::INFINITY, f64::INFINITY, true, json!({"zero_pivot_step": lu.zero_pivot}))
            } else {
                let (x, eta, _) = inverse_refined(&lu, &i.a, n, 3, &|| ctx.check())?;
                let (na, nx) = match i.p {
                    P::Inf => (norm_inf_mat(&i.a, n, n), norm_inf_mat(&x, n, n)),
                    P::Fro => (nrm2(&i.a), nrm2(&x)),
                    _ => (norm1(&i.a, n, n), norm1(&x, n, n)),
                };
                let cond = na * nx;
                let thr = 10.0 * n as f64 * U;
                (cond, (cond * eta).max(4.0 * U), eta <= thr, json!({"inverse_backward_error": num(eta), "threshold": num(thr)}))
            }
        };
        let rcond = if cond.is_finite() { 1.0 / cond } else { 0.0 };
        let mut warnings = Vec::new();
        if rcond < f64::EPSILON {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("cond = {} > 1/ε：A 在舍入意义下奇异；只有量级有意义（相对误差估计 {}）。", if cond.is_finite() { format!("{cond:.3e}") } else { "Inf".into() }, if est.is_finite() { format!("{est:.1e}") } else { "Inf".into() }))
                .path("/A")
                .details(json!({"cond": num(cond), "rcond": num(rcond)})));
        }
        let result = json!({"cond": num(cond), "rcond": num(rcond), "p": i.p.json(), "relative_error_estimate": num(est)});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": resid,
            "error_estimate": {"cond_rel": num(est), "basis": if i.p == P::Num(2.0) { "Weyl 界 e/σ_min + e/σ_max，e = ‖A‖_F·重构残差 + 2σ_max·正交性误差（启发式）" } else { "κ·η，η 为求逆的逐列后向误差（一阶，启发式）" }},
            "checks": {"decomposition_checked": ok},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn mat(m: usize, n: usize, d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [m, n], "data": d})
    }
    const M3: [f64; 9] = [1., 2., 3., 4., 5., 6., 7., 8., 9.];

    #[test]
    fn rank_and_noise_floor() {
        let e = Rank::new().call(&json!({"A": mat(3, 3, &M3)}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["rank"], 2);
        let tol = e["result"]["tol"].as_f64().unwrap();
        let dropped = e["result"]["largest_dropped"].as_f64().unwrap();
        assert!(dropped <= tol / 10.0, "noise {dropped:e} vs tol {tol:e}");
        // a singular value just above the threshold: ambiguous
        let d = [1.0, 0.0, 0.0, 5e-15];
        let e = Rank::new().call(&json!({"A": mat(2, 2, &d)}), &HostCtx::default());
        assert_eq!(e["result"]["rank"], 2);
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.NUMERICAL.RANK_AMBIGUOUS"));
        assert_eq!(e["trust"]["checks"]["rank_well_separated"], false);
    }

    #[test]
    fn basis_and_pinv() {
        let e = Basis::new().call(&json!({"A": mat(3, 3, &M3)}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["null"]["shape"], json!([3, 1]));
        let nv: Vec<f64> = e["result"]["null"]["data"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let w = 1.0 / 6f64.sqrt();
        for (g, t) in nv.iter().zip([w, -2.0 * w, w]) {
            assert!((g.abs() - t.abs()).abs() < 1e-14, "{nv:?}");
        }
        let e = Pinv::new().call(&json!({"A": mat(3, 2, &[1., 0., 0., 1., 0., 0.])}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["X"]["data"], json!([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
    }

    #[test]
    fn norms_and_cond() {
        let call = |a: Value| Norm::new().call(&a, &HostCtx::default());
        let v = json!({"dtype": "f64", "shape": [2], "data": [3.0, -4.0]});
        assert_eq!(call(json!({"x": v}))["result"]["norm"], 5.0);
        assert_eq!(call(json!({"x": v, "p": 1}))["result"]["norm"], 7.0);
        assert_eq!(call(json!({"x": v, "p": "-inf"}))["result"]["norm"], 3.0);
        let r = call(json!({"x": v, "p": 3}))["result"]["norm"].as_f64().unwrap();
        assert!((r - 91f64.cbrt()).abs() < 1e-15);
        let a = mat(2, 2, &[1., -2., 3., 4.]);
        assert_eq!(call(json!({"x": a, "p": 1}))["result"]["norm"], 6.0);
        assert_eq!(call(json!({"x": a, "p": "inf"}))["result"]["norm"], 7.0);
        assert_eq!(call(json!({"x": a, "p": 3}))["error"]["code"], "E.VALIDATION.RANGE");
        let c = Cond::new().call(&json!({"A": mat(2, 2, &[2., 0., 0., 0.5])}), &HostCtx::default());
        assert_eq!(c["result"]["cond"], 4.0, "{c}");
        let c = Cond::new().call(&json!({"A": mat(2, 2, &[2., 0., 0., 0.5]), "p": 1}), &HostCtx::default());
        assert_eq!(c["result"]["cond"], 4.0, "{c}");
        let c = Cond::new().call(&json!({"A": mat(2, 2, &[1., 2., 2., 4.]), "p": "inf"}), &HostCtx::default());
        assert_eq!(c["result"]["cond"], "Inf");
        let c = Cond::new().call(&json!({"A": mat(2, 3, &[1.; 6]), "p": 1}), &HostCtx::default());
        assert_eq!(c["error"]["code"], "E.VALIDATION.SHAPE");
    }
}
