//! LTI system representations (spec T-5): tf (SISO/MIMO), ss, zpk. Polynomials are in descending powers.
use crate::error::{ask_user, modify_input, OpError, Warning};
use crate::host::HostCtx;
use crate::matrix::{parse_array, to_real_matrix};
use nalgebra::{Complex, DMatrix};
use serde_json::{json, Value};

pub type C64 = Complex<f64>;

#[derive(Debug, Clone)]
pub enum Lti {
    /// num[i][j], den[i][j]: polynomial of output i / input j, leading zeros stripped.
    Tf { num: Vec<Vec<Vec<f64>>>, den: Vec<Vec<Vec<f64>>>, ts: f64, siso: bool },
    Ss { a: DMatrix<f64>, b: DMatrix<f64>, c: DMatrix<f64>, d: DMatrix<f64>, ts: f64 },
    Zpk { z: Vec<C64>, p: Vec<C64>, k: f64, ts: f64 },
}

#[derive(Debug, Clone)]
pub struct SsData {
    pub a: DMatrix<f64>,
    pub b: DMatrix<f64>,
    pub c: DMatrix<f64>,
    pub d: DMatrix<f64>,
    pub ts: f64,
}

impl Lti {
    pub fn ts(&self) -> f64 {
        match self {
            Lti::Tf { ts, .. } | Lti::Ss { ts, .. } | Lti::Zpk { ts, .. } => *ts,
        }
    }
    pub fn dims(&self) -> (usize, usize) {
        match self {
            Lti::Tf { num, .. } => (num.len(), num[0].len()),
            Lti::Ss { c, b, .. } => (c.nrows(), b.ncols()),
            Lti::Zpk { .. } => (1, 1),
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Lti::Tf { .. } => "tf",
            Lti::Ss { .. } => "ss",
            Lti::Zpk { .. } => "zpk",
        }
    }
    /// State-space realization. tf: controllable canonical form per element (block diagonal for MIMO, non-minimal).
    pub fn to_ss(&self) -> SsData {
        match self {
            Lti::Ss { a, b, c, d, ts } => SsData { a: a.clone(), b: b.clone(), c: c.clone(), d: d.clone(), ts: *ts },
            Lti::Zpk { z, p, k, ts } => {
                let num: Vec<f64> = poly_from_roots(z).iter().map(|c| c.re * k).collect();
                let den: Vec<f64> = poly_from_roots(p).iter().map(|c| c.re).collect();
                let (a, b, c, d) = tf2ss_siso(&num, &den);
                SsData { a, b, c, d, ts: *ts }
            }
            Lti::Tf { num, den, ts, .. } => {
                let (ny, nu) = (num.len(), num[0].len());
                let mut blocks = Vec::new();
                let mut n = 0;
                for i in 0..ny {
                    for j in 0..nu {
                        let r = tf2ss_siso(&num[i][j], &den[i][j]);
                        n += r.0.nrows();
                        blocks.push((i, j, r));
                    }
                }
                let mut a = DMatrix::zeros(n, n);
                let mut b = DMatrix::zeros(n, nu);
                let mut c = DMatrix::zeros(ny, n);
                let mut d = DMatrix::zeros(ny, nu);
                let mut off = 0;
                for (i, j, (ai, bi, ci, di)) in blocks {
                    let k = ai.nrows();
                    a.view_mut((off, off), (k, k)).copy_from(&ai);
                    b.view_mut((off, j), (k, 1)).copy_from(&bi);
                    c.view_mut((i, off), (1, k)).copy_from(&ci);
                    d[(i, j)] = di[(0, 0)];
                    off += k;
                }
                SsData { a, b, c, d, ts: *ts }
            }
        }
    }
    pub fn poles(&self) -> Vec<C64> {
        match self {
            Lti::Zpk { p, .. } => p.clone(),
            _ => {
                let s = self.to_ss();
                eigenvalues(&s.a)
            }
        }
    }
}

/// Deterministic Householder reflector H = I − 2vvᵀ/(vᵀv) used to perturb the QR iteration path.
fn reflector(n: usize, t: usize) -> DMatrix<f64> {
    let v = nalgebra::DVector::from_fn(n, |k, _| 1.0 + (((k * 7 + t * 13) % 17) as f64) / 17.0);
    DMatrix::identity(n, n) - (&v * v.transpose()) * (2.0 / v.norm_squared())
}

/// Real Schur form A = Q T Qᵀ with a bounded number of QR iterations. nalgebra's implicit double-shift
/// QR has no exceptional shifts and can cycle forever (e.g. companion matrices of even polynomials);
/// on non-convergence retry on deterministic similarity transforms / shifts (spec M-7: still deterministic).
/// The first attempt is bit-identical to nalgebra's `complex_eigenvalues`.
pub fn schur_robust(a: &DMatrix<f64>) -> Option<(DMatrix<f64>, DMatrix<f64>)> {
    use nalgebra::linalg::Schur;
    let n = a.nrows();
    let maxit = 100 * n.max(10);
    if let Some(s) = Schur::try_new(a.clone(), f64::EPSILON, maxit) {
        return Some(s.unpack());
    }
    let scale = a.amax().max(f64::MIN_POSITIVE);
    for t in 1..=6 {
        let h = reflector(n, t);
        let sigma = if t % 2 == 0 { 0.0 } else { scale * (0.1 + 0.07 * t as f64) };
        let b = &h * a * &h + DMatrix::<f64>::identity(n, n) * sigma;
        if let Some(s) = Schur::try_new(b, f64::EPSILON, maxit) {
            let (q, tt) = s.unpack();
            return Some((&h * q, tt - DMatrix::<f64>::identity(n, n) * sigma));
        }
    }
    None
}

/// Eigenvalues of a real matrix (complex pairs exactly conjugate). NaN entries if the QR iteration
/// fails to converge even after the fallbacks of `schur_robust`.
pub fn eigenvalues(a: &DMatrix<f64>) -> Vec<C64> {
    let n = a.nrows();
    if n == 0 {
        return vec![];
    }
    match schur_robust(a) {
        Some((_, t)) => quasi_triangular_eigs(&t),
        None => vec![C64::new(f64::NAN, f64::NAN); n],
    }
}

/// Eigenvalues of a real quasi-upper-triangular matrix (same formulas as nalgebra's do_complex_eigenvalues).
fn quasi_triangular_eigs(t: &DMatrix<f64>) -> Vec<C64> {
    let n = t.nrows();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if i == n - 1 || t[(i + 1, i)] == 0.0 {
            out.push(C64::new(t[(i, i)], 0.0));
            i += 1;
        } else {
            let (hmm, hmn, hnm, hnn) = (t[(i, i)], t[(i, i + 1)], t[(i + 1, i)], t[(i + 1, i + 1)]);
            let val = (hmm - hnn) * 0.5;
            let discr = hnm * hmn + val * val;
            let half_tra = (hnn + hmm) * 0.5;
            if discr >= 0.0 {
                let sq = discr.sqrt();
                out.push(C64::new(half_tra + sq, 0.0));
                out.push(C64::new(half_tra - sq, 0.0));
            } else {
                let sq = (-discr).sqrt();
                out.push(C64::new(half_tra, sq));
                out.push(C64::new(half_tra, -sq));
            }
            i += 2;
        }
    }
    out
}

/// scipy.signal.tf2ss-style controllable canonical form.
pub fn tf2ss_siso(num: &[f64], den: &[f64]) -> (DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, DMatrix<f64>) {
    let a0 = den[0];
    let den: Vec<f64> = den.iter().map(|x| x / a0).collect();
    let n = den.len() - 1;
    let mut b = vec![0.0; n + 1];
    let off = n + 1 - num.len();
    for (k, x) in num.iter().enumerate() {
        b[off + k] = x / a0;
    }
    let dd = DMatrix::from_element(1, 1, b[0]);
    if n == 0 {
        return (DMatrix::zeros(0, 0), DMatrix::zeros(0, 1), DMatrix::zeros(1, 0), dd);
    }
    let mut a = DMatrix::zeros(n, n);
    for j in 0..n {
        a[(0, j)] = -den[j + 1];
    }
    for i in 1..n {
        a[(i, i - 1)] = 1.0;
    }
    let mut bm = DMatrix::zeros(n, 1);
    bm[(0, 0)] = 1.0;
    let mut c = DMatrix::zeros(1, n);
    for j in 0..n {
        c[(0, j)] = b[j + 1] - den[j + 1] * b[0];
    }
    (a, bm, c, dd)
}

pub fn poly_from_roots(r: &[C64]) -> Vec<C64> {
    let mut p = vec![C64::new(1.0, 0.0)];
    for root in r {
        let mut q = vec![C64::new(0.0, 0.0); p.len() + 1];
        for (k, c) in p.iter().enumerate() {
            q[k] += c;
            q[k + 1] -= c * root;
        }
        p = q;
    }
    p
}

fn parse_poly(v: &Value, path: &str, is_den: bool, warnings: &mut Vec<Warning>) -> Result<Vec<f64>, OpError> {
    let a = v.as_array().ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", "多项式必须是数字数组。", "按降幂给出实系数数组。").path(path))?;
    let mut p: Vec<f64> = Vec::with_capacity(a.len());
    for (i, x) in a.iter().enumerate() {
        let f = x.as_f64().ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", "多项式系数必须是有限实数。", "修正系数。").path(&format!("{path}/{i}")))?;
        if !f.is_finite() {
            return Err(OpError::new("E.VALIDATION.NON_FINITE", "多项式系数不能是 NaN/Inf。", "修正系数。").path(&format!("{path}/{i}")));
        }
        p.push(f);
    }
    let first_nz = p.iter().position(|x| *x != 0.0);
    match first_nz {
        None => {
            if is_den {
                return Err(OpError::new("E.VALIDATION.ZERO_DENOMINATOR", "分母多项式全为零。", "提供非零分母。").path(path));
            }
            Ok(vec![0.0])
        }
        Some(0) => Ok(p),
        Some(k) => {
            warnings.push(Warning::new("W.VALIDATION.LEADING_ZEROS_STRIPPED", format!("多项式前 {k} 个系数为 0，已去除。")).path(path));
            Ok(p[k..].to_vec())
        }
    }
}

fn degree(p: &[f64]) -> isize {
    if p.len() == 1 && p[0] == 0.0 {
        -1
    } else {
        p.len() as isize - 1
    }
}

pub fn parse_complex_list(v: &Value, path: &str) -> Result<Vec<C64>, OpError> {
    v.as_array()
        .unwrap_or(&vec![])
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let a = x.as_array().filter(|a| a.len() == 2);
            match a.and_then(|a| Some(C64::new(a[0].as_f64()?, a[1].as_f64()?))) {
                Some(c) if c.re.is_finite() && c.im.is_finite() => Ok(c),
                _ => Err(OpError::new("E.VALIDATION.SCHEMA", "复数必须为有限的 [re, im]。", "修正该元素。").path(&format!("{path}/{i}"))),
            }
        })
        .collect()
}

pub fn check_conjugate(v: &[C64], path: &str) -> Result<(), OpError> {
    let mut used = vec![false; v.len()];
    for i in 0..v.len() {
        if used[i] {
            continue;
        }
        let c = v[i];
        let tol = 1e-12 * c.norm().max(1.0);
        if c.im.abs() <= tol {
            used[i] = true;
            continue;
        }
        let partner = (0..v.len()).find(|&j| j != i && !used[j] && (v[j] - c.conj()).norm() <= tol);
        match partner {
            Some(j) => {
                used[i] = true;
                used[j] = true;
            }
            None => {
                return Err(OpError::new("E.VALIDATION.NOT_CONJUGATE", format!("复数 {}{:+}j 没有共轭配对，实系数系统要求复零极点共轭成对出现。", c.re, c.im), "补上共轭项，或检查输入。")
                    .path(&format!("{path}/{i}"))
                    .remedy(modify_input("add_conj", "补上共轭项", "medium", true, json!([{"op": "add", "path": format!("{path}/-"), "value": [c.re, -c.im]}]))));
            }
        }
    }
    Ok(())
}

fn improper(path: &str, nd: isize, dd: isize, siso_patch: Option<(&[f64], &[f64])>) -> OpError {
    let mut e = OpError::new(
        "E.DOMAIN.IMPROPER_SYSTEM",
        format!("系统不是 proper：分子阶次 {nd} > 分母阶次 {dd}，频率响应在 ω→∞ 时发散，不存在状态空间实现。"),
        "该算子只接受 proper 系统。若是理想微分环节，请补充远高于关心频带的极点使其 proper；若输入有误，检查 num/den 是否颠倒。",
    )
    .path(path)
    .detail("num_degree", json!(nd))
    .detail("den_degree", json!(dd));
    if let Some((num, den)) = siso_patch {
        // multiply den by (tau s + 1)^(nd-dd), tau = 1e-3 s
        let mut d: Vec<f64> = den.to_vec();
        for _ in 0..(nd - dd) {
            let mut q = vec![0.0; d.len() + 1];
            for (k, c) in d.iter().enumerate() {
                q[k] += 1e-3 * c;
                q[k + 1] += c;
            }
            d = q;
        }
        e = e
            .remedy(modify_input("add_filter_pole", &format!("在分母乘上 (τs+1)^{}，τ = 1e-3 s（远高于关心频带时影响可忽略）", nd - dd), "medium", true, json!([{"op": "replace", "path": format!("{path}/den"), "value": d}])))
            .remedy(modify_input("swap_num_den", "若 num/den 写反，交换二者", "low", true, json!([{"op": "replace", "path": format!("{path}/num"), "value": den}, {"op": "replace", "path": format!("{path}/den"), "value": num}])));
    }
    e.remedy(ask_user("ask", "向用户确认系统模型是否为理想微分环节或输入有误"))
}

/// Parse and validate an LtiSystem value at `path`.
pub fn parse_lti(v: &Value, path: &str, host: &HostCtx, warnings: &mut Vec<Warning>, max_dense: usize) -> Result<Lti, OpError> {
    let o = v.as_object().ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", "system 必须是对象。", "按 LtiSystem schema 提供。").path(path))?;
    let ts = o.get("ts").and_then(|t| t.as_f64()).unwrap_or(0.0);
    match o.get("kind").and_then(|k| k.as_str()) {
        Some("tf") => {
            let nv = &o["num"];
            let dv = &o["den"];
            let siso = nv.as_array().map(|a| a.first().map(|x| x.is_number()).unwrap_or(true)).unwrap_or(true);
            let (num, den) = if siso {
                if !dv.as_array().map(|a| a.first().map(|x| x.is_number()).unwrap_or(true)).unwrap_or(false) {
                    return Err(OpError::new("E.VALIDATION.SHAPE", "SISO 的 num 与 den 必须都是一维多项式。", "统一 num/den 的形式。").path(path));
                }
                (vec![vec![parse_poly(nv, &format!("{path}/num"), false, warnings)?]], vec![vec![parse_poly(dv, &format!("{path}/den"), true, warnings)?]])
            } else {
                let rows_n = nv.as_array().unwrap();
                let rows_d = dv.as_array().ok_or_else(|| OpError::new("E.VALIDATION.SHAPE", "MIMO 的 den 必须与 num 同形。", "按 [ny][nu] 给出 den。").path(path))?;
                let ny = rows_n.len();
                let nu = rows_n.first().and_then(|r| r.as_array()).map(|r| r.len()).unwrap_or(0);
                if ny == 0 || nu == 0 || rows_d.len() != ny {
                    return Err(OpError::new("E.VALIDATION.SHAPE", "MIMO tf 的 num/den 必须为非空的 [ny][nu] 多项式数组且同形。", "检查维度。").path(path));
                }
                let mut num = Vec::new();
                let mut den = Vec::new();
                for i in 0..ny {
                    let rn = rows_n[i].as_array().filter(|r| r.len() == nu);
                    let rd = rows_d[i].as_array().filter(|r| r.len() == nu);
                    let (Some(rn), Some(rd)) = (rn, rd) else {
                        return Err(OpError::new("E.VALIDATION.SHAPE", format!("第 {i} 行的列数与第 0 行不一致。"), "检查维度。").path(&format!("{path}/num/{i}")));
                    };
                    let mut nrow = Vec::new();
                    let mut drow = Vec::new();
                    for j in 0..nu {
                        nrow.push(parse_poly(&rn[j], &format!("{path}/num/{i}/{j}"), false, warnings)?);
                        drow.push(parse_poly(&rd[j], &format!("{path}/den/{i}/{j}"), true, warnings)?);
                    }
                    num.push(nrow);
                    den.push(drow);
                }
                (num, den)
            };
            for i in 0..num.len() {
                for j in 0..num[0].len() {
                    let (nd, dd) = (degree(&num[i][j]), degree(&den[i][j]));
                    if nd > dd {
                        let p = if siso { path.to_string() } else { format!("{path}/num/{i}/{j}") };
                        return Err(improper(&p, nd, dd, if siso { Some((&num[0][0], &den[0][0])) } else { None }));
                    }
                }
            }
            Ok(Lti::Tf { num, den, ts, siso })
        }
        Some("ss") => {
            let mut get = |k: &str| -> Result<DMatrix<f64>, OpError> {
                let p = format!("{path}/{k}");
                let arr = parse_array(&o[k], &p, host, false)?;
                to_real_matrix(&arr, &p, warnings, max_dense)
            };
            let (a, b, c, d) = (get("A")?, get("B")?, get("C")?, get("D")?);
            let n = a.nrows();
            let shp = |m: &DMatrix<f64>| json!([m.nrows(), m.ncols()]);
            let bad = |what: &str, k: &str| {
                OpError::new("E.VALIDATION.SHAPE", format!("状态空间维度不一致：{what}。"), "要求 A:[n,n] B:[n,m] C:[p,n] D:[p,m]；注意数据按 row-major 展平。")
                    .path(&format!("{path}/{k}"))
                    .detail("shapes", json!({"A": shp(&a), "B": shp(&b), "C": shp(&c), "D": shp(&d)}))
            };
            if a.ncols() != n {
                return Err(bad("A 不是方阵", "A"));
            }
            if b.nrows() != n {
                return Err(bad("B 的行数 ≠ n", "B"));
            }
            if c.ncols() != n {
                return Err(bad("C 的列数 ≠ n", "C"));
            }
            if d.nrows() != c.nrows() || d.ncols() != b.ncols() {
                return Err(bad("D 必须为 [p,m]", "D"));
            }
            if b.ncols() == 0 || c.nrows() == 0 {
                return Err(bad("至少需要 1 个输入和 1 个输出", "B"));
            }
            Ok(Lti::Ss { a, b, c, d, ts })
        }
        Some("zpk") => {
            let z = parse_complex_list(&o["z"], &format!("{path}/z"))?;
            let p = parse_complex_list(&o["p"], &format!("{path}/p"))?;
            check_conjugate(&z, &format!("{path}/z"))?;
            check_conjugate(&p, &format!("{path}/p"))?;
            let k = o.get("k").and_then(|k| k.as_f64()).unwrap_or(f64::NAN);
            if !k.is_finite() {
                return Err(OpError::new("E.VALIDATION.NON_FINITE", "k 必须是有限实数。", "修正 k。").path(&format!("{path}/k")));
            }
            if z.len() > p.len() {
                return Err(improper(path, z.len() as isize, p.len() as isize, None));
            }
            Ok(Lti::Zpk { z, p, k, ts })
        }
        _ => Err(OpError::new("E.VALIDATION.SCHEMA", "system.kind 必须为 tf/ss/zpk。", "修正 kind。").path(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tf2ss_matches_scipy_form() {
        let (a, b, c, d) = tf2ss_siso(&[4.0], &[1.0, 1.2, 4.0]);
        assert_eq!(a, DMatrix::from_row_slice(2, 2, &[-1.2, -4.0, 1.0, 0.0]));
        assert_eq!(b, DMatrix::from_row_slice(2, 1, &[1.0, 0.0]));
        assert_eq!(c, DMatrix::from_row_slice(1, 2, &[0.0, 4.0]));
        assert_eq!(d[(0, 0)], 0.0);
    }
    #[test]
    fn improper_detected_with_patch() {
        let v = json!({"kind":"tf","num":[1,0,0],"den":[1,1],"ts":0});
        let e = parse_lti(&v, "/system", &HostCtx::default(), &mut vec![], 100).unwrap_err();
        assert_eq!(e.code, "E.DOMAIN.IMPROPER_SYSTEM");
        let patch = &e.remediation[0]["patch"][0]["value"];
        assert_eq!(patch, &json!([0.001, 1.001, 1.0]));
    }
    #[test]
    fn conjugate_check() {
        let v = json!({"kind":"zpk","z":[],"p":[[-1,2]],"k":1,"ts":0});
        assert_eq!(parse_lti(&v, "/system", &HostCtx::default(), &mut vec![], 100).unwrap_err().code, "E.VALIDATION.NOT_CONJUGATE");
    }
    #[test]
    fn leading_zero_warning() {
        let mut w = vec![];
        let v = json!({"kind":"tf","num":[0,1],"den":[1,2],"ts":0});
        parse_lti(&v, "/system", &HostCtx::default(), &mut w, 100).unwrap();
        assert_eq!(w[0].code, "W.VALIDATION.LEADING_ZEROS_STRIPPED");
    }
}
