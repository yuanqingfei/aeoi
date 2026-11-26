//! Shared LTI system algebra for v0.3 operators: ss→tf, invariant zeros, Bartels–Stewart
//! Lyapunov solvers, frequency-response evaluation, DC gain, LtiSystem JSON encoding.
use crate::lti::{eigenvalues, tf2ss_siso, Lti, SsData, C64};
use crate::matrix::{complex_vector, dense_real};
use crate::num;
use crate::poly::{poly_real, polyadd_scaled, polyval, roots, strip};
use nalgebra::{DMatrix, DVector};
use serde_json::{json, Value};

pub const U: f64 = f64::EPSILON / 2.0;

fn cplx(a: &DMatrix<f64>) -> DMatrix<C64> {
    a.map(|x| C64::new(x, 0.0))
}

/// Characteristic polynomial det(sI − A) from the eigenvalues (monic, length n+1).
pub fn charpoly(a: &DMatrix<f64>) -> Vec<f64> {
    poly_real(&eigenvalues(a))
}

/// scipy.signal.ss2tf convention: den = det(sI−A); num_ij = det(sI−A+b_j c_i) + (d_ij − 1)·den.
/// Returns (num[ny][nu], den) with every num of length n+1 (non-minimal; no cancellation).
pub fn ss2tf(ss: &SsData) -> (Vec<Vec<Vec<f64>>>, Vec<f64>) {
    let (ny, nu) = (ss.c.nrows(), ss.b.ncols());
    let den = charpoly(&ss.a);
    let mut num = vec![vec![Vec::new(); nu]; ny];
    for i in 0..ny {
        for j in 0..nu {
            let bc = ss.b.column(j) * ss.c.row(i);
            let p = charpoly(&(&ss.a - bc));
            let mut q = polyadd_scaled(&p, &den, ss.d[(i, j)] - 1.0);
            q[0] = ss.d[(i, j)]; // leading coefficient is exactly D (cancels rounding in 1·s^n − 1·s^n)
            num[i][j] = strip(&q);
        }
    }
    (num, den)
}

/// n×n orthogonal matrix whose first r columns span the columns of `m` (m: n×r, full column rank).
pub fn complete_basis(m: &DMatrix<f64>) -> DMatrix<f64> {
    let n = m.nrows();
    let mut aug = DMatrix::zeros(n, m.ncols() + n);
    aug.view_mut((0, 0), (n, m.ncols())).copy_from(m);
    aug.view_mut((0, m.ncols()), (n, n)).copy_from(&DMatrix::identity(n, n));
    let k = aug.ncols().min(n);
    crate::kernels::nalg::q_full(&aug.columns(0, k).into_owned())
}

/// Orthonormal basis of the leading `r` left singular directions plus completion: returns (U_full, rank).
fn row_compress(m: &DMatrix<f64>, tol: f64) -> (DMatrix<f64>, usize) {
    let p = m.nrows();
    if m.ncols() == 0 || p == 0 {
        return (DMatrix::identity(p, p), 0);
    }
    let Some((sv, u, _)) = crate::kernels::nalg::svd_thin(m) else { return (DMatrix::identity(p, p), 0) };
    let idx: Vec<usize> = (0..sv.len()).collect(); // already descending
    let r = idx.iter().filter(|&&k| sv[k] > tol).count();
    if r == 0 {
        return (DMatrix::identity(p, p), 0);
    }
    let mut ur = DMatrix::zeros(p, r);
    for (c, &k) in idx.iter().take(r).enumerate() {
        ur.set_column(c, &u.column(k));
    }
    (complete_basis(&ur), r)
}

/// One Emami-Naeini/Van Dooren-type reduction pass: returns a system with the same finite invariant
/// zeros whose D has full row rank.
fn reduce(mut a: DMatrix<f64>, mut b: DMatrix<f64>, mut c: DMatrix<f64>, mut d: DMatrix<f64>, tol: f64) -> (DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, DMatrix<f64>) {
    loop {
        let (n, m, p) = (a.nrows(), b.ncols(), c.nrows());
        if p == 0 {
            return (a, b, c, d);
        }
        let (uf, sigma) = row_compress(&d, tol);
        if sigma == p {
            return (a, b, c, d);
        }
        let ut = uf.transpose();
        let cc = &ut * &c;
        let dd = &ut * &d;
        let c1 = cc.rows(0, sigma).into_owned();
        let d1 = dd.rows(0, sigma).into_owned();
        let c2 = cc.rows(sigma, p - sigma).into_owned();
        if n == 0 {
            return (a, b, c1, d1);
        }
        // column compression of C2: C2·V = [0, C22], C22 full column rank rho
        let (vf, rho) = row_compress(&c2.transpose(), tol);
        if rho == 0 {
            c = c1;
            d = d1;
            continue;
        }
        let k = n - rho;
        let mut v = DMatrix::zeros(n, n);
        v.view_mut((0, 0), (n, k)).copy_from(&vf.columns(rho, k));
        v.view_mut((0, k), (n, rho)).copy_from(&vf.columns(0, rho));
        let at = v.transpose() * &a * &v;
        let bt = v.transpose() * &b;
        let c1t = &c1 * &v;
        let mut cn = DMatrix::zeros(sigma + rho, k);
        cn.view_mut((0, 0), (sigma, k)).copy_from(&c1t.columns(0, k));
        cn.view_mut((sigma, 0), (rho, k)).copy_from(&at.view((k, 0), (rho, k)));
        let mut dn = DMatrix::zeros(sigma + rho, m);
        dn.view_mut((0, 0), (sigma, m)).copy_from(&d1);
        dn.view_mut((sigma, 0), (rho, m)).copy_from(&bt.rows(k, rho));
        a = at.view((0, 0), (k, k)).into_owned();
        b = bt.rows(0, k).into_owned();
        c = cn;
        d = dn;
    }
}

#[derive(Debug, Clone)]
pub struct ZerosInfo {
    pub reduced_order: usize,
    pub rank_tolerance: f64,
    pub cond_d_reduced: f64,
}

/// Finite invariant zeros of (A,B,C,D): values λ where the Rosenbrock matrix [[A−λI, B],[C, D]]
/// loses normal rank. Two rank-revealing (SVD) reduction passes (primal + dual) make D square
/// invertible, then zeros = eig(A_r − B_r D_r⁻¹ C_r). Err(()) if the system matrix is not of full
/// normal rank (degenerate: every λ is a zero).
pub fn invariant_zeros(ss: &SsData) -> Result<(Vec<C64>, ZerosInfo), ZerosInfo> {
    let (n, m, p) = (ss.a.nrows(), ss.b.ncols(), ss.c.nrows());
    let scale = (ss.a.norm_squared() + ss.b.norm_squared() + ss.c.norm_squared() + ss.d.norm_squared()).sqrt().max(f64::MIN_POSITIVE);
    let tol = (n + m.max(p)) as f64 * f64::EPSILON * scale * 10.0;
    if normal_rank_deficient(ss) {
        // G(s) has normal rank < min(p, m): every s is a zero of the Rosenbrock pencil (degenerate)
        return Err(ZerosInfo { reduced_order: n, rank_tolerance: tol, cond_d_reduced: f64::INFINITY });
    }
    let (a1, b1, c1, d1) = reduce(ss.a.clone(), ss.b.clone(), ss.c.clone(), ss.d.clone(), tol);
    let (a2, b2, c2, d2) = reduce(a1.transpose(), c1.transpose(), b1.transpose(), d1.transpose(), tol);
    let (a3, b3, c3, d3) = (a2.transpose(), c2.transpose(), b2.transpose(), d2.transpose());
    let nr = a3.nrows();
    let mut info = ZerosInfo { reduced_order: nr, rank_tolerance: tol, cond_d_reduced: 1.0 };
    if d3.nrows() != d3.ncols() {
        return Err(info);
    }
    if nr == 0 {
        return Ok((vec![], info));
    }
    if d3.nrows() == 0 {
        return Ok((eigenvalues(&a3), info));
    }
    info.cond_d_reduced = crate::linalg::cond2(&d3);
    match crate::kernels::nalg::solve(&d3, &c3) {
        Some(x) if x.iter().all(|v| v.is_finite()) => Ok((eigenvalues(&(&a3 - &b3 * x)), info)),
        _ => Err(info),
    }
}

/// Normal-rank test: G(s) = C(sI−A)⁻¹B + D evaluated at two deterministic generic points
/// s_k = (1+‖A‖_F)·(0.6180 + 1.2361j·k); rank-deficient (σ_min ≤ 1e-11·magnitude bound) at both ⇒ degenerate.
pub fn normal_rank_deficient(ss: &SsData) -> bool {
    let (n, m, p) = (ss.a.nrows(), ss.b.ncols(), ss.c.nrows());
    let r = m.min(p);
    if r == 0 {
        return false;
    }
    let sig = 1.0 + ss.a.norm();
    let mut deficient = true;
    for k in 1..=2 {
        let s = C64::new(0.6180339887 * sig, 1.2360679775 * sig * k as f64);
        let mut g = cplx(&ss.d);
        let mut bound = ss.d.norm();
        if n > 0 {
            let si_a = DMatrix::<C64>::identity(n, n) * s - cplx(&ss.a);
            match crate::kernels::nalg::csolve(&si_a, &cplx(&ss.b)) {
                Some(x) => {
                    g += cplx(&ss.c) * &x;
                    bound += ss.c.norm() * x.norm();
                }
                None => {
                    deficient = false;
                    continue;
                }
            }
        }
        let sv = crate::kernels::nalg::csingular_values(&g);
        let smin = sv.iter().cloned().fold(f64::INFINITY, f64::min);
        if sv.len() < r || !(smin <= 1e-11 * bound.max(f64::MIN_POSITIVE)) {
            deficient = false;
        }
    }
    deficient
}

/// SISO zpk of a state-space system: p = eig(A), z = invariant zeros, k = first non-zero Markov
/// parameter C·A^{r−1}·B (r = #p − #z; k = D when r = 0).
pub fn ss2zpk_siso(ss: &SsData) -> Result<(Vec<C64>, Vec<C64>, f64, ZerosInfo), ZerosInfo> {
    let p = eigenvalues(&ss.a);
    if ss.b.ncols() == 1 && ss.c.nrows() == 1 && normal_rank_deficient(ss) {
        // SISO G(s) ≡ 0: zpk with k = 0, no zeros (python-control/scipy convention)
        let info = ZerosInfo { reduced_order: 0, rank_tolerance: 0.0, cond_d_reduced: f64::INFINITY };
        return Ok((vec![], p, 0.0, info));
    }
    let (z, info) = invariant_zeros(ss)?;
    let r = p.len() as isize - z.len() as isize;
    let k = if r <= 0 {
        ss.d[(0, 0)]
    } else {
        let mut v = ss.b.clone();
        for _ in 1..r {
            v = &ss.a * v;
        }
        (&ss.c * v)[(0, 0)]
    };
    if k == 0.0 {
        // transfer function identically zero
        return Ok((vec![], p, 0.0, info));
    }
    Ok((z, p, k, info))
}

pub fn tf2zpk(num: &[f64], den: &[f64]) -> (Vec<C64>, Vec<C64>, f64) {
    let n = strip(num);
    let d = strip(den);
    if n.len() == 1 && n[0] == 0.0 {
        return (vec![], roots(&d), 0.0);
    }
    (roots(&n), roots(&d), n[0] / d[0])
}

pub fn zpk2tf(z: &[C64], p: &[C64], k: f64) -> (Vec<f64>, Vec<f64>) {
    let num: Vec<f64> = poly_real(z).iter().map(|x| x * k).collect();
    (strip(&num), poly_real(p))
}

/// Convert any Lti to the requested kind. MIMO → zpk is not representable (zpk is SISO in v0.3).
pub fn convert(sys: &Lti, to: &str) -> Result<(Lti, &'static str), &'static str> {
    let ts = sys.ts();
    let (ny, nu) = sys.dims();
    Ok(match (sys, to) {
        (Lti::Tf { .. }, "tf") | (Lti::Ss { .. }, "ss") | (Lti::Zpk { .. }, "zpk") => (sys.clone(), "identity"),
        (Lti::Tf { siso, .. }, "ss") => {
            let s = sys.to_ss();
            (Lti::Ss { a: s.a, b: s.b, c: s.c, d: s.d, ts }, if *siso || ny * nu == 1 { "controllable_canonical" } else { "block_diagonal_controllable_canonical" })
        }
        (Lti::Zpk { .. }, "ss") => {
            let s = sys.to_ss();
            (Lti::Ss { a: s.a, b: s.b, c: s.c, d: s.d, ts }, "controllable_canonical")
        }
        (Lti::Ss { .. }, "tf") => {
            let s = sys.to_ss();
            let (num, den) = ss2tf(&s);
            let den_all = vec![vec![den; nu]; ny];
            (Lti::Tf { num, den: den_all, ts, siso: ny == 1 && nu == 1 }, "ss2tf_characteristic_polynomial")
        }
        (Lti::Zpk { z, p, k, .. }, "tf") => {
            let (n, d) = zpk2tf(z, p, *k);
            (Lti::Tf { num: vec![vec![n]], den: vec![vec![d]], ts, siso: true }, "expanded_product")
        }
        (Lti::Tf { num, den, .. }, "zpk") => {
            if ny * nu != 1 {
                return Err("mimo");
            }
            let (z, p, k) = tf2zpk(&num[0][0], &den[0][0]);
            (Lti::Zpk { z, p, k, ts }, "polynomial_roots")
        }
        (Lti::Ss { .. }, "zpk") => {
            if ny * nu != 1 {
                return Err("mimo");
            }
            let s = sys.to_ss();
            let (z, p, k, _) = ss2zpk_siso(&s).map_err(|_| "degenerate")?;
            (Lti::Zpk { z, p, k, ts }, "invariant_zeros_eigenvalues_markov_gain")
        }
        _ => return Err("kind"),
    })
}

/// Normalize tf so that every denominator is monic.
pub fn tf_normalize(sys: &mut Lti) {
    if let Lti::Tf { num, den, .. } = sys {
        for i in 0..num.len() {
            for j in 0..num[0].len() {
                let a0 = den[i][j][0];
                if a0 != 0.0 && a0 != 1.0 {
                    num[i][j].iter_mut().for_each(|x| *x /= a0);
                    den[i][j].iter_mut().for_each(|x| *x /= a0);
                }
            }
        }
    }
}

/// Frequency response G(s) (s complex; for discrete systems pass z).
pub fn eval_tf(sys: &Lti, s: C64) -> Option<DMatrix<C64>> {
    let (ny, nu) = sys.dims();
    match sys {
        Lti::Tf { num, den, .. } => {
            let mut g = DMatrix::zeros(ny, nu);
            for i in 0..ny {
                for j in 0..nu {
                    let d = polyval(&den[i][j], s);
                    if d.norm() == 0.0 {
                        return None;
                    }
                    g[(i, j)] = polyval(&num[i][j], s) / d;
                }
            }
            Some(g)
        }
        Lti::Zpk { z, p, k, .. } => {
            let mut v = C64::new(*k, 0.0);
            for zz in z {
                v *= s - zz;
            }
            for pp in p {
                let d = s - pp;
                if d.norm() == 0.0 {
                    return None;
                }
                v /= d;
            }
            Some(DMatrix::from_element(1, 1, v))
        }
        Lti::Ss { a, b, c, d, .. } => {
            let n = a.nrows();
            if n == 0 {
                return Some(cplx(d));
            }
            let m = DMatrix::<C64>::identity(n, n) * s - cplx(a);
            let x = crate::kernels::nalg::csolve(&m, &cplx(b))?;
            Some(cplx(c) * x + cplx(d))
        }
    }
}

/// Max relative discrepancy between two systems' responses on a few test points on the
/// imaginary axis / unit circle, skipping points too close to a pole. Used as a conversion residual.
pub fn response_discrepancy(a: &Lti, b: &Lti) -> Option<f64> {
    let ts = a.ts();
    let poles = a.poles();
    let scale = poles.iter().map(|p| if ts > 0.0 { 1.0 } else { p.norm() }).fold(1.0, f64::max);
    let mut worst: Option<f64> = None;
    for k in 0..9 {
        let x = (k as f64 / 8.0) * 4.0 - 2.0; // 1e-2 .. 1e2 (× scale)
        let w = 10f64.powf(x) * scale;
        let s = if ts > 0.0 { C64::from_polar(1.0, (w / scale).min(3.0)) } else { C64::new(0.0, w) };
        if poles.iter().any(|p| (s - p).norm() < 1e-6 * s.norm().max(1.0)) {
            continue;
        }
        if let (Some(ga), Some(gb)) = (eval_tf(a, s), eval_tf(b, s)) {
            let den = ga.iter().map(|v| v.norm()).fold(0.0, f64::max).max(1e-300);
            let e = (ga - gb).iter().map(|v| v.norm()).fold(0.0, f64::max) / den;
            worst = Some(worst.unwrap_or(0.0).max(e));
        }
    }
    worst
}

/// LtiSystem JSON (spec T-5); polynomials and matrices row-major.
pub fn lti_json(sys: &Lti) -> Value {
    let ts = sys.ts();
    let poly = |p: &Vec<f64>| Value::Array(p.iter().map(|x| num(*x)).collect());
    match sys {
        Lti::Tf { num: n, den, siso, .. } => {
            if *siso || (n.len() == 1 && n[0].len() == 1) {
                json!({"kind": "tf", "num": poly(&n[0][0]), "den": poly(&den[0][0]), "ts": ts, "time_unit": "s"})
            } else {
                let nest = |m: &Vec<Vec<Vec<f64>>>| Value::Array(m.iter().map(|r| Value::Array(r.iter().map(poly).collect())).collect());
                json!({"kind": "tf", "num": nest(n), "den": nest(den), "ts": ts, "time_unit": "s"})
            }
        }
        Lti::Ss { a, b, c, d, .. } => json!({"kind": "ss", "A": dense_real(a), "B": dense_real(b), "C": dense_real(c), "D": dense_real(d), "ts": ts, "time_unit": "s"}),
        Lti::Zpk { z, p, k, .. } => {
            let cl = |v: &Vec<C64>| Value::Array(v.iter().map(|c| json!([num(c.re), num(c.im)])).collect());
            json!({"kind": "zpk", "z": cl(z), "p": cl(p), "k": num(*k), "ts": ts, "time_unit": "s"})
        }
    }
}

pub fn complex_list(v: &[C64]) -> Value {
    complex_vector(v)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LyapKind {
    Continuous,
    Discrete,
}

#[derive(Debug, Clone)]
pub struct LyapInfo {
    /// continuous: min |λ_i + conj λ_j|; discrete: min |λ_i conj λ_j − 1|
    pub separation: f64,
    pub schur_residual: f64,
}

/// Complex Schur factorization A = U T Uᴴ prepared for Bartels–Stewart solves (reusable for iterative refinement).
pub struct LyapSchur {
    pub kind: LyapKind,
    u: DMatrix<C64>,
    t: DMatrix<C64>,
    pub info: LyapInfo,
}

impl LyapSchur {
    /// Factor A; Err(separation) if the equation is (numerically) singular, Err(NaN) if Schur fails.
    pub fn new(a: &DMatrix<f64>, kind: LyapKind) -> Result<LyapSchur, f64> {
        let n = a.nrows();
        if n == 0 {
            return Ok(LyapSchur { kind, u: DMatrix::zeros(0, 0), t: DMatrix::zeros(0, 0), info: LyapInfo { separation: f64::INFINITY, schur_residual: 0.0 } });
        }
        let ac = cplx(a);
        // complex Schur from the robust real Schur: A = Q R Qᵀ, then triangularize R's 2×2 blocks
        // the complex Schur form of linalg.decomp.eig's kernel (real Schur + rsf2csf), spec 11.5 RI-1
        let cs = crate::kernels::eig::complex_schur(a).ok_or(f64::NAN)?;
        let (u, mut t) = (cs.u, cs.t);
        for i in 0..n {
            for j in 0..i {
                t[(i, j)] = C64::new(0.0, 0.0);
            }
        }
        let schur_residual = (&u * &t * u.adjoint() - &ac).norm() / a.norm().max(f64::MIN_POSITIVE);
        let anorm = a.norm().max(f64::MIN_POSITIVE);
        let mut sep = f64::INFINITY;
        for i in 0..n {
            for j in 0..n {
                let (li, lj) = (t[(i, i)], t[(j, j)]);
                let s = match kind {
                    LyapKind::Continuous => (li + lj.conj()).norm(),
                    LyapKind::Discrete => (li * lj.conj() - 1.0).norm(),
                };
                sep = sep.min(s);
            }
        }
        let thr = match kind {
            LyapKind::Continuous => 1e3 * U * anorm,
            LyapKind::Discrete => 1e3 * U * anorm.max(1.0).powi(2),
        };
        if sep <= thr {
            return Err(sep);
        }
        Ok(LyapSchur { kind, u, t, info: LyapInfo { separation: sep, schur_residual } })
    }

    /// Solve A X + X Aᵀ + Q = 0 (continuous) or A X Aᵀ − X + Q = 0 (discrete) with the stored factorization.
    pub fn solve(&self, q: &DMatrix<f64>) -> DMatrix<f64> {
        let (u, t, kind) = (&self.u, &self.t, self.kind);
        let n = t.nrows();
        if n == 0 {
            return DMatrix::zeros(0, 0);
        }
        let qt = u.adjoint() * cplx(q) * u;
        let mut y = DMatrix::<C64>::zeros(n, n);
        for j in (0..n).rev() {
            let mut acc = DVector::<C64>::zeros(n);
            for k in j + 1..n {
                acc += y.column(k) * t[(j, k)].conj();
            }
            let tjj = t[(j, j)].conj();
            let (m, rhs) = match kind {
                LyapKind::Continuous => (t + DMatrix::<C64>::identity(n, n) * tjj, -qt.column(j) - acc),
                LyapKind::Discrete => (t * tjj - DMatrix::<C64>::identity(n, n), -qt.column(j) - t * acc),
            };
            // back substitution (upper triangular)
            let mut x = DVector::<C64>::zeros(n);
            for i in (0..n).rev() {
                let mut s = rhs[i];
                for l in i + 1..n {
                    s -= m[(i, l)] * x[l];
                }
                x[i] = s / m[(i, i)];
            }
            y.set_column(j, &x);
        }
        (u * y * u.adjoint()).map(|c| c.re)
    }
}

/// Bartels–Stewart with a complex Schur form A = U T Uᴴ.
/// Continuous: A X + X Aᵀ + Q = 0.  Discrete: A X Aᵀ − X + Q = 0.
/// Err(separation) if the equation is (numerically) singular.
pub fn lyap_bs(a: &DMatrix<f64>, q: &DMatrix<f64>, kind: LyapKind) -> Result<(DMatrix<f64>, LyapInfo), f64> {
    let f = LyapSchur::new(a, kind)?;
    let x = f.solve(q);
    Ok((x, f.info))
}

// ---- compensated (double-f64) arithmetic for accurate residuals.
// Error-free transformations (Knuth TwoSum, Dekker TwoProduct with Veltkamp splitting; no FMA so the
// result is bitwise identical on every platform, wasm32 included) and Ogita–Rump–Oishi Dot2.
/// Knuth TwoSum: a + b = s + e exactly.
#[inline]
pub fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

#[inline]
fn split(a: f64) -> (f64, f64) {
    let c = 134217729.0 * a; // 2^27 + 1
    let h = c - (c - a);
    (h, a - h)
}

#[inline]
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, al * bl - (((p - ah * bh) - al * bh) - ah * bl))
}

/// Accumulator for Σ xᵢ·yᵢ in roughly twice the working precision (Dot2). `value()` = hi + lo.
#[derive(Clone, Copy, Default)]
pub struct Dot2 {
    s: f64,
    c: f64,
}

impl Dot2 {
    #[inline]
    pub fn add_prod(&mut self, x: f64, y: f64) {
        let (p, pe) = two_prod(x, y);
        let (s, se) = two_sum(self.s, p);
        self.s = s;
        self.c += pe + se;
    }
    #[inline]
    pub fn add(&mut self, x: f64) {
        let (s, se) = two_sum(self.s, x);
        self.s = s;
        self.c += se;
    }
    #[inline]
    pub fn hi_lo(&self) -> (f64, f64) {
        two_sum(self.s, self.c)
    }
}

/// Lyapunov residual computed in compensated arithmetic (Dot2), rounded to f64 at the end:
/// continuous R = A X + X Aᵀ + Q, discrete R = A X Aᵀ − X + Q. Accurate to ~u·|R| + u²·(scale), so it
/// can drive iterative refinement beyond the cond·u limit of a plain f64 residual.
pub fn lyap_residual_dd(a: &DMatrix<f64>, q: &DMatrix<f64>, x: &DMatrix<f64>, kind: LyapKind) -> DMatrix<f64> {
    let n = a.nrows();
    let mut r = DMatrix::<f64>::zeros(n, n);
    match kind {
        LyapKind::Continuous => {
            for j in 0..n {
                for i in 0..n {
                    let mut d = Dot2::default();
                    for k in 0..n {
                        d.add_prod(a[(i, k)], x[(k, j)]);
                        d.add_prod(x[(i, k)], a[(j, k)]);
                    }
                    d.add(q[(i, j)]);
                    let (h, l) = d.hi_lo();
                    r[(i, j)] = h + l;
                }
            }
        }
        LyapKind::Discrete => {
            // Y = A X kept as an unevaluated sum Yh + Yl, then R = (Yh + Yl) Aᵀ − X + Q
            let mut yh = DMatrix::<f64>::zeros(n, n);
            let mut yl = DMatrix::<f64>::zeros(n, n);
            for j in 0..n {
                for i in 0..n {
                    let mut d = Dot2::default();
                    for k in 0..n {
                        d.add_prod(a[(i, k)], x[(k, j)]);
                    }
                    let (h, l) = d.hi_lo();
                    yh[(i, j)] = h;
                    yl[(i, j)] = l;
                }
            }
            for j in 0..n {
                for i in 0..n {
                    let mut d = Dot2::default();
                    for k in 0..n {
                        d.add_prod(yh[(i, k)], a[(j, k)]);
                        d.add_prod(yl[(i, k)], a[(j, k)]);
                    }
                    d.add(-x[(i, j)]);
                    d.add(q[(i, j)]);
                    let (h, l) = d.hi_lo();
                    r[(i, j)] = h + l;
                }
            }
        }
    }
    r
}

/// Result of [`lyap_refined`].
pub struct LyapRefined {
    pub x: DMatrix<f64>,
    pub info: LyapInfo,
    /// number of refinement corrections applied
    pub steps: usize,
    /// ‖δ‖_F/‖X‖_F of the last correction computed (applied or not); 0 if refinement was not triggered
    pub last_correction_rel: f64,
    /// whether refinement was triggered (heuristic forward-error estimate above `trigger_rel`)
    pub triggered: bool,
    /// compensated residual ‖R‖_F of the returned X (NaN when refinement was not triggered)
    pub residual_dd: f64,
}

/// Bartels–Stewart followed by iterative refinement with a compensated residual (reusing the Schur factors):
/// X ← X + δ, where δ solves the same equation with Q replaced by the residual R(X). Triggered only when the
/// heuristic forward-error estimate ‖R‖_F/(sep·‖X‖_F) exceeds `trigger_rel` (near-singular equations,
/// e.g. discrete with ρ(A) → 1); at most `max_steps` corrections, stopping when ‖δ‖ ≤ 2u‖X‖ or when a
/// correction does not shrink by half (then it is not applied).
pub fn lyap_refined(a: &DMatrix<f64>, q: &DMatrix<f64>, kind: LyapKind, trigger_rel: f64, max_steps: usize) -> Result<LyapRefined, f64> {
    let f = LyapSchur::new(a, kind)?;
    let mut x = f.solve(q);
    let sep = f.info.separation;
    let xn = x.norm().max(f64::MIN_POSITIVE);
    // trigger on the plain f64 residual (cheap); its rounding floor u·scale/sep makes this fire exactly when sep is small
    let est = lyap_residual(a, q, &x, kind).0 / sep / xn;
    let mut out = LyapRefined { x: DMatrix::zeros(0, 0), info: f.info.clone(), steps: 0, last_correction_rel: 0.0, triggered: false, residual_dd: f64::NAN };
    if !(est > trigger_rel) || !est.is_finite() {
        out.x = x;
        return Ok(out);
    }
    out.triggered = true;
    let mut r = lyap_residual_dd(a, q, &x, kind);
    let mut prev = f64::INFINITY;
    for _ in 0..max_steps {
        let d = f.solve(&r);
        let dn = d.norm();
        let xn = x.norm().max(f64::MIN_POSITIVE);
        out.last_correction_rel = dn / xn;
        if !dn.is_finite() || dn > 0.5 * prev {
            break;
        }
        let xnew = &x + &d;
        let rnew = lyap_residual_dd(a, q, &xnew, kind);
        if !(rnew.norm() <= r.norm()) {
            break;
        }
        x = xnew;
        r = rnew;
        out.steps += 1;
        prev = dn;
        if dn <= 2.0 * U * xn {
            break;
        }
    }
    out.residual_dd = r.norm();
    out.x = x;
    Ok(out)
}

/// Lyapunov residual ‖·‖_F and the scale used for the relative residual.
pub fn lyap_residual(a: &DMatrix<f64>, q: &DMatrix<f64>, x: &DMatrix<f64>, kind: LyapKind) -> (f64, f64) {
    match kind {
        LyapKind::Continuous => ((a * x + x * a.transpose() + q).norm(), 2.0 * a.norm() * x.norm() + q.norm()),
        LyapKind::Discrete => ((a * x * a.transpose() - x + q).norm(), a.norm().powi(2) * x.norm() + x.norm() + q.norm()),
    }
}

/// PBH test value σ_min([A − λI, B]) / ‖[A, B]‖_F (controllability of mode λ). Use Aᵀ, Cᵀ for observability.
pub fn pbh(a: &DMatrix<f64>, b: &DMatrix<f64>, lam: C64) -> f64 {
    let (n, m) = (a.nrows(), b.ncols());
    let scale = (a.norm_squared() + b.norm_squared()).sqrt().max(f64::MIN_POSITIVE);
    let mut mm = DMatrix::<C64>::zeros(n, n + m);
    mm.view_mut((0, 0), (n, n)).copy_from(&(cplx(a) - DMatrix::<C64>::identity(n, n) * lam));
    mm.view_mut((0, n), (n, m)).copy_from(&cplx(b));
    crate::kernels::nalg::csingular_values(&mm).iter().cloned().fold(f64::INFINITY, f64::min) / scale
}

/// SISO tf → ss in controllable canonical form (re-export for operators).
pub fn tf2ss(num: &[f64], den: &[f64]) -> (DMatrix<f64>, DMatrix<f64>, DMatrix<f64>, DMatrix<f64>) {
    tf2ss_siso(num, den)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ss(a: &[f64], n: usize, b: &[f64], m: usize, c: &[f64], p: usize, d: &[f64]) -> SsData {
        SsData { a: DMatrix::from_row_slice(n, n, a), b: DMatrix::from_row_slice(n, m, b), c: DMatrix::from_row_slice(p, n, c), d: DMatrix::from_row_slice(p, m, d), ts: 0.0 }
    }
    #[test]
    fn zeros_siso() {
        // (s+2)/((s+1)(s+3))
        let s = ss(&[-4.0, -3.0, 1.0, 0.0], 2, &[1.0, 0.0], 1, &[1.0, 2.0], 1, &[0.0]);
        let (z, _) = invariant_zeros(&s).unwrap();
        assert_eq!(z.len(), 1);
        assert!((z[0] - C64::new(-2.0, 0.0)).norm() < 1e-13);
        let (z, p, k, _) = ss2zpk_siso(&s).unwrap();
        assert_eq!((z.len(), p.len()), (1, 2));
        assert!((k - 1.0).abs() < 1e-14);
        // relative degree 3, no zeros
        let s = ss(&[0.0, 1.0, 0.0, 0.0, 0.0, 1.0, -1.0, -3.0, -3.0], 3, &[0.0, 0.0, 1.0], 1, &[2.0, 0.0, 0.0], 1, &[0.0]);
        let (z, p, k, _) = ss2zpk_siso(&s).unwrap();
        assert!(z.is_empty() && p.len() == 3 && (k - 2.0).abs() < 1e-14);
    }
    #[test]
    fn zeros_nonsquare() {
        // 2 outputs, 1 input: y1 = (s+2)/(s+1)(s+3), y2 = (s+2)/((s+1)(s+3)) * 3  -> common zero at -2
        let s = ss(&[-4.0, -3.0, 1.0, 0.0], 2, &[1.0, 0.0], 1, &[1.0, 2.0, 3.0, 6.0], 2, &[0.0, 0.0]);
        let (z, _) = invariant_zeros(&s).unwrap();
        assert_eq!(z.len(), 1);
        assert!((z[0] - C64::new(-2.0, 0.0)).norm() < 1e-12);
        // y1 = 1/(s+1)(s+3), y2 = s/(s+1)(s+3): no common zero
        let s = ss(&[-4.0, -3.0, 1.0, 0.0], 2, &[1.0, 0.0], 1, &[0.0, 1.0, 1.0, 0.0], 2, &[0.0, 0.0]);
        assert!(invariant_zeros(&s).unwrap().0.is_empty());
    }
    #[test]
    fn ss2tf_scipy_form() {
        let s = ss(&[-4.0, -3.0, 1.0, 0.0], 2, &[1.0, 0.0], 1, &[1.0, 2.0], 1, &[0.5]);
        let (n, d) = ss2tf(&s);
        let want_d = [1.0, 4.0, 3.0];
        let want_n = [0.5, 3.0, 3.5];
        for k in 0..3 {
            assert!((d[k] - want_d[k]).abs() < 1e-13 && (n[0][0][k] - want_n[k]).abs() < 1e-13);
        }
    }
    #[test]
    fn lyap_bs_both() {
        let a = DMatrix::from_row_slice(3, 3, &[-1.0, 2.0, 0.5, 0.0, -3.0, 1.0, 0.3, -0.2, -2.0]);
        let q = DMatrix::from_row_slice(3, 3, &[2.0, 0.5, 0.1, 0.5, 1.0, 0.0, 0.1, 0.0, 3.0]);
        let (x, _) = lyap_bs(&a, &q, LyapKind::Continuous).unwrap();
        assert!((&a * &x + &x * a.transpose() + &q).norm() < 1e-13 * x.norm());
        let ad = &a * 0.3;
        let (x, _) = lyap_bs(&ad, &q, LyapKind::Discrete).unwrap();
        assert!((&ad * &x * ad.transpose() - &x + &q).norm() < 1e-13 * x.norm());
        // complex eigenvalues
        let a = DMatrix::from_row_slice(2, 2, &[-0.5, 3.0, -3.0, -0.5]);
        let (x, _) = lyap_bs(&a, &DMatrix::identity(2, 2), LyapKind::Continuous).unwrap();
        assert!((&a * &x + &x * a.transpose() + DMatrix::<f64>::identity(2, 2)).norm() < 1e-14);
        // singular: eigenvalues ±1
        assert!(lyap_bs(&DMatrix::from_row_slice(2, 2, &[1.0, 0.0, 0.0, -1.0]), &DMatrix::identity(2, 2), LyapKind::Continuous).is_err());
    }
}
