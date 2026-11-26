//! Bracketing scalar kernels. `zeroin`: Brent's method on a sign-changing bracket (the algorithm of
//! MATLAB fzero / Brent 1973 zeroin). Shared by fzero and every later operator that needs a scalar root
//! inside a bracket (spec 11.5 RI-1).
use crate::error::OpError;

const EPS: f64 = f64::EPSILON;

/// Why an evaluation stopped the iteration (returned by the caller's function).
pub enum Stop {
    /// f(x) was not finite at this x
    NonFinite(f64),
    /// the caller's evaluation budget is exhausted
    Budget,
    /// cancelled / deadline (from the check closure)
    Abort(OpError),
}

/// Failure with the bracket at the time: `b` is the best iterate, `c` the other end.
pub struct ZeroinErr {
    pub stop: Stop,
    pub b: f64,
    pub c: f64,
}

/// Converged: root `x`, the other end `other` of the final bracket, number of iterations.
pub struct Zeroin {
    pub x: f64,
    pub other: f64,
    pub iters: usize,
}

/// Brent's zeroin on [a, b] with f(a) = fa, f(b) = fb of opposite sign (or one of them zero).
/// Stops when the bracket half-width ≤ 2ε|x| + xtol/2 (+ smallest normal) or f(x) = 0.
/// `check` is called every 64 iterations.
pub fn zeroin(
    f: &mut dyn FnMut(f64) -> Result<f64, Stop>,
    mut a: f64,
    mut b: f64,
    mut fa: f64,
    mut fb: f64,
    xtol: f64,
    check: &dyn Fn() -> Result<(), OpError>,
) -> Result<Zeroin, ZeroinErr> {
    let (mut cc, mut fc) = (a, fa);
    let (mut d, mut e) = (b - a, b - a);
    let mut iters = 0usize;
    if fa == 0.0 {
        return Ok(Zeroin { x: a, other: a, iters });
    }
    if fb == 0.0 {
        return Ok(Zeroin { x: b, other: b, iters });
    }
    loop {
        if (fb > 0.0) == (fc > 0.0) {
            cc = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = cc;
            cc = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * EPS * b.abs() + 0.5 * xtol + f64::MIN_POSITIVE;
        let xm = 0.5 * (cc - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return Ok(Zeroin { x: b, other: cc, iters });
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == cc {
                p = 2.0 * xm * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * xm * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < (3.0 * xm * q - (tol1 * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol1 { d } else { tol1.copysign(xm) };
        iters += 1;
        fb = match f(b) {
            Ok(v) => v,
            Err(stop) => return Err(ZeroinErr { stop, b, c: cc }),
        };
        if iters % 64 == 0 {
            if let Err(e) = check() {
                return Err(ZeroinErr { stop: Stop::Abort(e), b, c: cc });
            }
        }
    }
}

/// Converged minimisation: point `x`, value `fx`, final bracket [a, b], iterations, evaluations.
pub struct Fmin {
    pub x: f64,
    pub fx: f64,
    pub a: f64,
    pub b: f64,
    pub iters: usize,
}

/// Failure of `fmin`: why, and the best point so far.
pub struct FminErr {
    pub stop: Stop,
    pub x: f64,
    pub fx: f64,
}

/// Brent's minimisation on [a, b] (Brent 1973 Ch. 5; Forsythe–Malcolm–Moler `fmin`, the algorithm of MATLAB
/// fminbnd; spec 10.46 FB-2): golden-section steps c = (3 − √5)/2, parabolic interpolation when it is
/// acceptable, tol1 = √ε·|x| + tol_x/3, tol2 = 2·tol1; stops when |x − (a+b)/2| ≤ tol2 − (b − a)/2.
/// The end points themselves are never evaluated. `check` is called every 64 iterations.
pub fn fmin(f: &mut dyn FnMut(f64) -> Result<f64, Stop>, mut a: f64, mut b: f64, tol_x: f64, check: &dyn Fn() -> Result<(), OpError>) -> Result<Fmin, FminErr> {
    let c = 0.5 * (3.0 - 5f64.sqrt());
    let seps = EPS.sqrt();
    let mut v = a + c * (b - a);
    let mut w = v;
    let mut xf = v;
    let (mut d, mut e) = (0.0f64, 0.0f64);
    let mut fx = f(xf).map_err(|stop| FminErr { stop, x: xf, fx: f64::NAN })?;
    let (mut fv, mut fw) = (fx, fx);
    let mut xm = 0.5 * (a + b);
    let mut tol1 = seps * xf.abs() + tol_x / 3.0;
    let mut tol2 = 2.0 * tol1;
    let mut iters = 0usize;
    while (xf - xm).abs() > tol2 - 0.5 * (b - a) {
        iters += 1;
        if iters % 64 == 0 {
            check().map_err(|e| FminErr { stop: Stop::Abort(e), x: xf, fx })?;
        }
        let mut golden = true;
        if e.abs() > tol1 {
            // parabolic fit through x, w, v
            let mut r = (xf - w) * (fx - fv);
            let mut q = (xf - v) * (fx - fw);
            let mut p = (xf - v) * q - (xf - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            }
            q = q.abs();
            r = e;
            e = d;
            if p.abs() < (0.5 * q * r).abs() && p > q * (a - xf) && p < q * (b - xf) {
                d = p / q;
                let u = xf + d;
                golden = false;
                if (u - a) < tol2 || (b - u) < tol2 {
                    let si = if xm - xf >= 0.0 { 1.0 } else { -1.0 };
                    d = tol1 * si;
                }
            }
        }
        if golden {
            e = if xf >= xm { a - xf } else { b - xf };
            d = c * e;
        }
        let si = if d >= 0.0 { 1.0 } else { -1.0 };
        let u = xf + si * d.abs().max(tol1);
        let fu = f(u).map_err(|stop| FminErr { stop, x: xf, fx })?;
        if fu <= fx {
            if u >= xf {
                a = xf;
            } else {
                b = xf;
            }
            v = w;
            fv = fw;
            w = xf;
            fw = fx;
            xf = u;
            fx = fu;
        } else {
            if u < xf {
                a = u;
            } else {
                b = u;
            }
            if fu <= fw || w == xf {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == xf || v == w {
                v = u;
                fv = fu;
            }
        }
        xm = 0.5 * (a + b);
        tol1 = seps * xf.abs() + tol_x / 3.0;
        tol2 = 2.0 * tol1;
    }
    Ok(Fmin { x: xf, fx, a, b, iters })
}
