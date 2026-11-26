//! Piecewise-cubic interpolation kernel: pchip slopes (Fritsch–Butland, scipy PchipInterpolator = MATLAB
//! pchip), not-a-knot cubic spline slopes (scipy CubicSpline = MATLAB spline) with a LAPACK dgtsv
//! tridiagonal solve, Hermite coefficients in scipy PPoly order, and interval search. Shared by interp1
//! and later spline / interp2 (spec 11.5 RI-1).

/// LAPACK dgtsv (tridiagonal, partial pivoting), single right-hand side. dl, d, du are overwritten.
pub fn gtsv(mut dl: Vec<f64>, mut d: Vec<f64>, mut du: Vec<f64>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = d.len();
    if n == 1 {
        return if d[0] == 0.0 { None } else { Some(vec![b[0] / d[0]]) };
    }
    for i in 0..n - 1 {
        if d[i].abs() >= dl[i].abs() {
            if d[i] == 0.0 {
                return None;
            }
            let fact = dl[i] / d[i];
            d[i + 1] -= fact * du[i];
            b[i + 1] -= fact * b[i];
            dl[i] = 0.0;
        } else {
            let fact = d[i] / dl[i];
            d[i] = dl[i];
            let temp = d[i + 1];
            d[i + 1] = du[i] - fact * temp;
            if i < n - 2 {
                dl[i] = du[i + 1];
                du[i + 1] = -fact * dl[i];
            }
            du[i] = temp;
            let tb = b[i];
            b[i] = b[i + 1];
            b[i + 1] = tb - fact * b[i + 1];
        }
    }
    if d[n - 1] == 0.0 {
        return None;
    }
    // back substitution; after an interchange dl[i] holds the second superdiagonal (dgtsv)
    b[n - 1] /= d[n - 1];
    if n > 1 {
        b[n - 2] = (b[n - 2] - du[n - 2] * b[n - 1]) / d[n - 2];
    }
    for i in (0..n.saturating_sub(2)).rev() {
        b[i] = (b[i] - du[i] * b[i + 1] - dl[i] * b[i + 2]) / d[i];
    }
    Some(b)
}

/// Slopes for pchip (scipy PchipInterpolator._find_derivatives).
pub fn pchip_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let h: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let m: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    if n == 2 {
        return vec![m[0], m[0]];
    }
    let sign = |v: f64| if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 };
    let mut d = vec![0.0; n];
    for k in 1..n - 1 {
        let cond = sign(m[k]) != sign(m[k - 1]) || m[k] == 0.0 || m[k - 1] == 0.0;
        if !cond {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            let whmean = (w1 / m[k - 1] + w2 / m[k]) / (w1 + w2);
            d[k] = 1.0 / whmean;
        }
    }
    let edge = |h0: f64, h1: f64, m0: f64, m1: f64| {
        let dd = ((2.0 * h0 + h1) * m0 - h0 * m1) / (h0 + h1);
        if sign(dd) != sign(m0) {
            0.0
        } else if sign(m0) != sign(m1) && dd.abs() > 3.0 * m0.abs() {
            3.0 * m0
        } else {
            dd
        }
    };
    d[0] = edge(h[0], h[1], m[0], m[1]);
    d[n - 1] = edge(h[n - 2], h[n - 3], m[n - 2], m[n - 3]);
    d
}

/// Slopes of the not-a-knot cubic spline (scipy CubicSpline, bc_type = not-a-knot).
pub fn spline_slopes(x: &[f64], y: &[f64]) -> Option<Vec<f64>> {
    let n = x.len();
    let dx: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let slope: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / dx[i]).collect();
    if n == 2 {
        return Some(vec![slope[0], slope[0]]);
    }
    if n == 3 {
        // the parabola through the three points: 3×3 system solved by LU with partial pivoting
        let a = [1.0, 1.0, 0.0, dx[1], 2.0 * (dx[0] + dx[1]), dx[0], 0.0, 1.0, 1.0];
        let b = [2.0 * slope[0], 3.0 * (dx[0] * slope[1] + dx[1] * slope[0]), 2.0 * slope[1]];
        let lu = super::dense::lu_factor(&a, 3);
        if lu.zero_pivot.is_some() {
            return None;
        }
        return Some(lu.solve(&b));
    }
    let mut diag = vec![0.0; n];
    let mut upper = vec![0.0; n - 1]; // A[i, i+1]
    let mut lower = vec![0.0; n - 1]; // A[i+1, i]
    let mut b = vec![0.0; n];
    for i in 1..n - 1 {
        diag[i] = 2.0 * (dx[i - 1] + dx[i]);
        upper[i] = dx[i - 1];
        lower[i - 1] = dx[i];
        b[i] = 3.0 * (dx[i] * slope[i - 1] + dx[i - 1] * slope[i]);
    }
    // not-a-knot start
    diag[0] = dx[1];
    let d0 = x[2] - x[0];
    upper[0] = d0;
    b[0] = ((dx[0] + 2.0 * d0) * dx[1] * slope[0] + dx[0] * dx[0] * slope[1]) / d0;
    // not-a-knot end
    diag[n - 1] = dx[n - 3];
    let d1 = x[n - 1] - x[n - 3];
    lower[n - 2] = d1;
    b[n - 1] = (dx[n - 2] * dx[n - 2] * slope[n - 3] + (2.0 * d1 + dx[n - 2]) * dx[n - 3] * slope[n - 2]) / d1;
    gtsv(lower, diag, upper, b)
}

/// Cubic Hermite coefficients of piece i (scipy CubicHermiteSpline / PPoly order).
pub fn coeffs(x: &[f64], y: &[f64], s: &[f64], i: usize) -> [f64; 4] {
    let h = x[i + 1] - x[i];
    let m = (y[i + 1] - y[i]) / h;
    let t = (s[i] + s[i + 1] - 2.0 * m) / h;
    [t / h, (m - s[i]) / h - t, s[i], y[i]]
}
pub fn horner(c: &[f64; 4], d: f64) -> f64 {
    ((c[0] * d + c[1]) * d + c[2]) * d + c[3]
}

/// Index i with x[i] ≤ v < x[i+1], clamped to [0, n−2].
pub fn interval(x: &[f64], v: f64) -> usize {
    let n = x.len();
    let i = x.partition_point(|&t| t <= v);
    i.saturating_sub(1).min(n - 2)
}

/// Value of the cubic Hermite interpolant with slopes `s` at v (piece by `interval`, evaluated from x_i).
pub fn eval_cubic(x: &[f64], y: &[f64], s: &[f64], v: f64) -> f64 {
    let i = interval(x, v);
    horner(&coeffs(x, y, s, i), v - x[i])
}
