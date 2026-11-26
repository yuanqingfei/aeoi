//! Adaptive quadrature kernel: Gauss–Kronrod (7, 15) with QUADPACK's error estimate, global adaptive
//! bisection (always split the worst subinterval, starting from 10 equal panels as MATLAB integral), and
//! MATLAB integral's variable transformations. Shared by integral and later integral2 (spec 11.5 RI-1).
use super::sum::Dot2;
use crate::error::OpError;

// QUADPACK qk15 nodes (positive half) and weights
const XGK: [f64; 8] = [0.991455371120812639206854697526329, 0.949107912342758524526189684047851, 0.864864423359769072789712788640926, 0.741531185599394439863864773280788, 0.586087235467691130294144845693013, 0.405845151377397166906606412076961, 0.207784955007898467600689403773245, 0.0];
const WGK: [f64; 8] = [0.022935322010529224963732008058970, 0.063092092629978553290700663189204, 0.104790010322250183839876322541518, 0.140653259715525918745189590510238, 0.169004726639267902826583426598550, 0.190350578064785409913256402421014, 0.204432940075298892414161999234649, 0.209482141084727828012999174891714];
const WG: [f64; 4] = [0.129484966168869693270611432679082, 0.279705391489276667901467771423780, 0.381830050505118944950369775488975, 0.417959183673469387755102040816327];

/// MATLAB integral's transformation of [a, b] onto t ∈ [−1, 1] or [0, 1]: finite x = (a+b)/2 +
/// (b−a)/4·t(3 − t²) (weakens endpoint singularities); (−∞, ∞) x = t/(1 − t²); [a, ∞) x = a + t/(1 − t);
/// (−∞, b] x = b − t/(1 − t).
#[derive(Clone, Copy)]
pub enum Map {
    Finite(f64, f64),
    Both,
    Upper(f64),
    Lower(f64),
}

impl Map {
    /// (x(t), dx/dt)
    pub fn x_dx(&self, t: f64) -> (f64, f64) {
        match *self {
            Map::Finite(a, b) => ((a + b) / 2.0 + (b - a) / 4.0 * t * (3.0 - t * t), (b - a) / 4.0 * 3.0 * (1.0 - t * t)),
            Map::Both => { let d = 1.0 - t * t; (t / d, (1.0 + t * t) / (d * d)) }
            Map::Upper(a) => { let d = 1.0 - t; (a + t / d, 1.0 / (d * d)) }
            Map::Lower(b) => { let d = 1.0 - t; (b - t / d, 1.0 / (d * d)) }
        }
    }
    /// x(t) alone (for reporting subintervals in x)
    pub fn x(&self, t: f64) -> f64 {
        match *self {
            Map::Finite(a, b) => (a + b) / 2.0 + (b - a) / 4.0 * t * (3.0 - t * t),
            Map::Both => t / (1.0 - t * t),
            Map::Upper(a) => a + t / (1.0 - t),
            Map::Lower(b) => b - t / (1.0 - t),
        }
    }
    /// range of t
    pub fn t_range(&self) -> (f64, f64) {
        match self {
            Map::Finite(..) | Map::Both => (-1.0, 1.0),
            _ => (0.0, 1.0),
        }
    }
}

/// One subinterval of the adaptive partition (in the transformed variable t).
pub struct Seg {
    pub lo: f64,
    pub hi: f64,
    pub val: f64,
    pub err: f64,
}

/// Gauss–Kronrod (7, 15) on [lo, hi] with QUADPACK qk15's error estimate: (value, error, all finite).
pub fn gk15(f: &mut dyn FnMut(f64) -> f64, lo: f64, hi: f64) -> (f64, f64, bool) {
    let c = 0.5 * (lo + hi);
    let h = 0.5 * (hi - lo);
    let fc = f(c);
    let mut resk = fc * WGK[7];
    let mut resg = fc * WG[3];
    let mut resabs = resk.abs();
    let mut fv1 = [0.0; 7];
    let mut fv2 = [0.0; 7];
    for j in 0..7 {
        let x = h * XGK[j];
        let (f1, f2) = (f(c - x), f(c + x));
        fv1[j] = f1;
        fv2[j] = f2;
        resk += WGK[j] * (f1 + f2);
        resabs += WGK[j] * (f1.abs() + f2.abs());
        if j % 2 == 1 {
            resg += WG[j / 2] * (f1 + f2);
        }
    }
    let reskh = resk * 0.5;
    let mut resasc = WGK[7] * (fc - reskh).abs();
    for j in 0..7 {
        resasc += WGK[j] * ((fv1[j] - reskh).abs() + (fv2[j] - reskh).abs());
    }
    let (result, resabs, resasc) = (resk * h, resabs * h.abs(), resasc * h.abs());
    let mut err = ((resk - resg) * h).abs();
    if resasc != 0.0 && err != 0.0 {
        err = resasc * aeoi_libm::libm::pow(200.0 * err / resasc, 1.5).min(1.0);
    }
    if resabs > f64::MIN_POSITIVE / (50.0 * f64::EPSILON) {
        err = err.max(50.0 * f64::EPSILON * resabs);
    }
    let finite = result.is_finite() && fv1.iter().chain(fv2.iter()).all(|v| v.is_finite()) && fc.is_finite();
    (result, err, finite)
}

/// (Σ values in compensated arithmetic, Σ error estimates)
pub fn total(s: &[Seg]) -> (f64, f64) {
    let mut d = Dot2::default();
    for x in s {
        d.add(x.val);
    }
    let (h, l) = d.hi_lo();
    (h + l, s.iter().map(|x| x.err).sum())
}

pub enum AdaptErr {
    /// the caller's poll or check failed
    Stopped(OpError),
    /// `max_intervals` reached before Σ err ≤ tol; carries the partition and its (value, error)
    Budget { segs: Vec<Seg>, value: f64, error: f64 },
}

/// Global adaptive GK(7,15) of g over [t0, t1] until Σ err ≤ max(abs_tol, rel_tol·|value|).
/// `poll` is called before every refinement step (e.g. to report a non-finite integrand), `check`
/// every 50 subintervals. Returns the partition sorted by `lo`.
pub fn adaptive(
    g: &mut dyn FnMut(f64) -> f64,
    t0: f64,
    t1: f64,
    abs_tol: f64,
    rel_tol: f64,
    max_intervals: usize,
    poll: &dyn Fn() -> Result<(), OpError>,
    check: &dyn Fn() -> Result<(), OpError>,
) -> Result<Vec<Seg>, AdaptErr> {
    let mut segs: Vec<Seg> = Vec::new();
    for k in 0..10 {
        let (lo, hi) = (t0 + (t1 - t0) * k as f64 / 10.0, t0 + (t1 - t0) * (k + 1) as f64 / 10.0);
        let (v, e, _) = gk15(g, lo, hi);
        segs.push(Seg { lo, hi, val: v, err: e });
    }
    let tol = |v: f64| abs_tol.max(rel_tol * v.abs());
    loop {
        poll().map_err(AdaptErr::Stopped)?;
        let (v, e) = total(&segs);
        if e <= tol(v) {
            return Ok(segs);
        }
        if segs.len() >= max_intervals {
            return Err(AdaptErr::Budget { segs, value: v, error: e });
        }
        let (k, _) = segs.iter().enumerate().fold((0, -1.0), |(bk, be), (k, s)| if s.err > be { (k, s.err) } else { (bk, be) });
        let s = segs.swap_remove(k);
        let mid = 0.5 * (s.lo + s.hi);
        let (v1, e1, _) = gk15(g, s.lo, mid);
        let (v2, e2, _) = gk15(g, mid, s.hi);
        segs.push(Seg { lo: s.lo, hi: mid, val: v1, err: e1 });
        segs.push(Seg { lo: mid, hi: s.hi, val: v2, err: e2 });
        if segs.len() % 50 == 0 {
            check().map_err(AdaptErr::Stopped)?;
        }
        segs.sort_by(|a, b| a.lo.partial_cmp(&b.lo).unwrap());
    }
}

/// The value with every subinterval bisected once more (compensated sum): the post-condition and the
/// second half of the reported error estimate.
pub fn bisected_value(g: &mut dyn FnMut(f64) -> f64, segs: &[Seg]) -> f64 {
    let mut d = Dot2::default();
    for s in segs {
        let mid = 0.5 * (s.lo + s.hi);
        d.add(gk15(g, s.lo, mid).0);
        d.add(gk15(g, mid, s.hi).0);
    }
    let (h, l) = d.hi_lo();
    h + l
}
