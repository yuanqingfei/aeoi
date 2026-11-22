//! Small dense linear-algebra helpers (pure Rust on nalgebra). Single-threaded.
use nalgebra::{Complex, DMatrix};

pub fn norm1(a: &DMatrix<f64>) -> f64 {
    (0..a.ncols()).map(|j| a.column(j).iter().map(|x| x.abs()).sum::<f64>()).fold(0.0, f64::max)
}

/// 2-norm condition number via SVD (inf if singular).
pub fn cond2(a: &DMatrix<f64>) -> f64 {
    if a.is_empty() {
        return 1.0;
    }
    let s = a.clone().svd(false, false).singular_values;
    let (mx, mn) = (s.max(), s.min());
    if mn == 0.0 {
        f64::INFINITY
    } else {
        mx / mn
    }
}

pub fn cond2_c(a: &DMatrix<Complex<f64>>) -> f64 {
    let s = a.clone().svd(false, false).singular_values;
    let (mx, mn) = (s.max(), s.min());
    if mn == 0.0 {
        f64::INFINITY
    } else {
        mx / mn
    }
}

/// Matrix exponential: scaling & squaring with Padé approximants of degree 3/5/7/9/13 (Higham 2005, Alg. 2.3).
/// Returns (exp(A), number of squarings s).
pub fn expm(a: &DMatrix<f64>) -> (DMatrix<f64>, u32) {
    let n = a.nrows();
    if n == 0 {
        return (DMatrix::zeros(0, 0), 0);
    }
    let id = DMatrix::<f64>::identity(n, n);
    let nrm = norm1(a);
    const THETA: [(usize, f64); 4] = [(3, 1.495585217958292e-2), (5, 2.539398330063230e-1), (7, 9.504178996162932e-1), (9, 2.097847961257068e0)];
    let a2 = a * a;
    for (m, th) in THETA {
        if nrm <= th {
            let b: &[f64] = match m {
                3 => &[120.0, 60.0, 12.0, 1.0],
                5 => &[30240.0, 15120.0, 3360.0, 420.0, 30.0, 1.0],
                7 => &[17297280.0, 8648640.0, 1995840.0, 277200.0, 25200.0, 1512.0, 56.0, 1.0],
                _ => &[17643225600.0, 8821612800.0, 2075673600.0, 302702400.0, 30270240.0, 2162160.0, 110880.0, 3960.0, 90.0, 1.0],
            };
            let mut u = &id * b[1];
            let mut v = &id * b[0];
            let mut pw = id.clone();
            for k in 1..=(m / 2) {
                pw = &pw * &a2;
                u += &pw * b[2 * k + 1];
                v += &pw * b[2 * k];
            }
            let u = a * u;
            return (solve_pade(&u, &v), 0);
        }
    }
    const THETA13: f64 = 5.371920351148152e0;
    let s = if nrm > THETA13 { (nrm / THETA13).log2().ceil().max(0.0) as u32 } else { 0 };
    let sc = 2f64.powi(-(s as i32));
    let a1 = a * sc;
    let b: [f64; 14] = [
        64764752532480000.0, 32382376266240000.0, 7771770303897600.0, 1187353796428800.0, 129060195264000.0, 10559470521600.0, 670442572800.0, 33522128640.0,
        1323241920.0, 40840800.0, 960960.0, 16380.0, 182.0, 1.0,
    ];
    let a2 = &a1 * &a1;
    let a4 = &a2 * &a2;
    let a6 = &a4 * &a2;
    let u_in = &a6 * (&a6 * b[13] + &a4 * b[11] + &a2 * b[9]) + &a6 * b[7] + &a4 * b[5] + &a2 * b[3] + &id * b[1];
    let u = &a1 * u_in;
    let v = &a6 * (&a6 * b[12] + &a4 * b[10] + &a2 * b[8]) + &a6 * b[6] + &a4 * b[4] + &a2 * b[2] + &id * b[0];
    let mut r = solve_pade(&u, &v);
    for _ in 0..s {
        r = &r * &r;
    }
    (r, s)
}

fn solve_pade(u: &DMatrix<f64>, v: &DMatrix<f64>) -> DMatrix<f64> {
    let p = v + u;
    let q = v - u;
    q.lu().solve(&p).expect("Padé denominator singular")
}

/// Kronecker product.
pub fn kron(a: &DMatrix<f64>, b: &DMatrix<f64>) -> DMatrix<f64> {
    let (ar, ac, br, bc) = (a.nrows(), a.ncols(), b.nrows(), b.ncols());
    let mut k = DMatrix::zeros(ar * br, ac * bc);
    for i in 0..ar {
        for j in 0..ac {
            let s = a[(i, j)];
            if s != 0.0 {
                k.view_mut((i * br, j * bc), (br, bc)).copy_from(&(b * s));
            }
        }
    }
    k
}

/// Lyapunov operator L(X) = MᵀX + XM as an n²×n² matrix acting on column-major vec(X).
pub fn lyap_operator(m: &DMatrix<f64>) -> DMatrix<f64> {
    let n = m.nrows();
    let id = DMatrix::<f64>::identity(n, n);
    let mt = m.transpose();
    kron(&id, &mt) + kron(&mt, &id)
}

/// Solve MᵀX + XM = C via the Kronecker form (small n only). Returns None if singular.
pub fn lyap_solve(m: &DMatrix<f64>, c: &DMatrix<f64>) -> Option<DMatrix<f64>> {
    let n = m.nrows();
    let l = lyap_operator(m);
    let rhs = nalgebra::DVector::from_column_slice(c.as_slice()); // nalgebra storage is column-major
    let x = l.lu().solve(&rhs)?;
    Some(DMatrix::from_column_slice(n, n, x.as_slice()))
}

/// Relative asymmetry ‖X − Xᵀ‖_F / max(‖X‖_F, tiny).
pub fn rel_asym(x: &DMatrix<f64>) -> f64 {
    let d = (x - x.transpose()).norm();
    let n = x.norm();
    if n == 0.0 {
        0.0
    } else {
        d / n
    }
}

pub fn symmetrize(x: &DMatrix<f64>) -> DMatrix<f64> {
    (x + x.transpose()) * 0.5
}

/// Sort complex values by (re asc, im desc) — canonical pole ordering in outputs.
pub fn sort_poles(v: &mut [Complex<f64>]) {
    v.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap_or(std::cmp::Ordering::Equal).then(b.im.partial_cmp(&a.im).unwrap_or(std::cmp::Ordering::Equal)));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expm_known() {
        // exp([[0,1],[-1,0]] t) = rotation
        let t = 2.0;
        let a = DMatrix::from_row_slice(2, 2, &[0.0, t, -t, 0.0]);
        let (e, _) = expm(&a);
        let want = DMatrix::from_row_slice(2, 2, &[t.cos(), t.sin(), -t.sin(), t.cos()]);
        assert!((e - want).norm() < 1e-14);
        // large norm path with squaring
        let a = DMatrix::from_row_slice(2, 2, &[-50.0, 0.0, 0.0, -1.0]);
        let (e, s) = expm(&a);
        assert!(s > 0);
        assert!(((e[(0, 0)] - (-50f64).exp()) / (-50f64).exp()).abs() < 1e-12);
        assert!(((e[(1, 1)] - (-1f64).exp()) / (-1f64).exp()).abs() < 1e-14);
        // nilpotent
        let a = DMatrix::from_row_slice(2, 2, &[0.0, 1.0, 0.0, 0.0]);
        assert!((expm(&a).0 - DMatrix::from_row_slice(2, 2, &[1.0, 1.0, 0.0, 1.0])).norm() < 1e-15);
    }
    #[test]
    fn lyap() {
        let m = DMatrix::from_row_slice(2, 2, &[-1.0, 2.0, 0.0, -3.0]);
        let c = DMatrix::from_row_slice(2, 2, &[1.0, 0.5, 0.5, 2.0]);
        let x = lyap_solve(&m, &c).unwrap();
        assert!((m.transpose() * &x + &x * &m - c).norm() < 1e-13);
    }
}
