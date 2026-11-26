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
