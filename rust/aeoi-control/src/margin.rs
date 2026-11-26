//! control.analysis.margin — gain / phase / stability margins with exact (polynomial) crossings plus a
//! dense-grid sign-change audit (spec 10.7).
use crate::mimo_err;
use aeoi_core::error::{OpError, Warning};
use aeoi_core::lti::{parse_lti, Lti, C64};
use aeoi_core::poly::{polyadd_scaled, polyder, polymul, polyval_r, roots, strip};
use aeoi_core::sysalg::{ss2tf, zpk2tf};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};
use std::f64::consts::PI;

operator_struct!(Margin, "control.analysis.margin");

pub struct MarginInput {
    pub num: Vec<f64>,
    pub den: Vec<f64>,
    pub ts: f64,
    pub kind: &'static str,
}

/// Real and imaginary parts (as real polynomials in ω, descending) of p(jω).
fn split_iw(p: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = p.len() - 1;
    let (mut re, mut im) = (vec![0.0; n + 1], vec![0.0; n + 1]);
    for (k, c) in p.iter().enumerate() {
        let e = n - k;
        match e % 4 {
            0 => re[k] = *c,
            1 => im[k] = *c,
            2 => re[k] = -*c,
            _ => im[k] = -*c,
        }
    }
    (strip(&re), strip(&im))
}

fn sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    strip(&polyadd_scaled(a, b, -1.0))
}
fn add(a: &[f64], b: &[f64]) -> Vec<f64> {
    strip(&polyadd_scaled(a, b, 1.0))
}

/// Non-negative real roots of p(ω). p(jω)-derived polynomials are even or odd in ω; then the degree
/// is halved with x = ω² (avoids ± root pairs, better conditioned). Falls back to the full polynomial.
fn real_roots(p: &[f64], allow_zero: bool) -> Vec<f64> {
    let n = p.len() - 1;
    let odd_zero = (0..=n).filter(|k| (n - k) % 2 == 1).all(|k| p[k] == 0.0);
    let even_zero = (0..=n).filter(|k| (n - k) % 2 == 0).all(|k| p[k] == 0.0);
    let mut v: Vec<f64> = Vec::new();
    if (odd_zero || even_zero) && n >= 1 {
        // p(ω) = ω^r · q(ω²), r ∈ {0, 1}
        let r = if odd_zero { 0 } else { 1 };
        let q: Vec<f64> = (0..=n).filter(|k| (n - k) % 2 == r).map(|k| p[k]).collect();
        if r == 1 {
            v.push(0.0);
        }
        for x in roots(&q) {
            if x.im.abs() <= 1e-6 * x.re.abs().max(1e-12) || x.im == 0.0 {
                if x.re > 0.0 {
                    v.push(x.re.sqrt());
                } else if x.re == 0.0 {
                    v.push(0.0);
                }
            }
        }
        // polish on the full polynomial
        let dp = polyder(p);
        v = v
            .into_iter()
            .map(|w| {
                let mut w = w;
                for _ in 0..3 {
                    let (f, d) = (polyval_r(p, w), polyval_r(&dp, w));
                    if d == 0.0 || f == 0.0 {
                        break;
                    }
                    let w1 = w - f / d;
                    if polyval_r(p, w1).abs() < f.abs() && w1 >= 0.0 {
                        w = w1;
                    } else {
                        break;
                    }
                }
                w
            })
            .collect();
    } else {
        v = roots(p).into_iter().filter(|r| r.im.abs() <= 1e-6 * r.re.abs().max(1.0)).map(|r| r.re).collect();
    }
    let mut v: Vec<f64> = v.into_iter().filter(|x| if allow_zero { *x >= 0.0 } else { *x > 0.0 }).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs().max(1e-300));
    v
}

/// Exact crossing candidates in the "ν" variable of G(jν) = n(jν)/d(jν).
fn candidates(n: &[f64], d: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (nr, ni) = split_iw(n);
    let (dr, di) = split_iw(d);
    let phase = sub(&polymul(&ni, &dr), &polymul(&nr, &di));
    let mag = sub(&add(&polymul(&nr, &nr), &polymul(&ni, &ni)), &add(&polymul(&dr, &dr), &polymul(&di, &di)));
    let (sr, si) = (add(&nr, &dr), add(&ni, &di));
    let f = add(&polymul(&sr, &sr), &polymul(&si, &si));
    let g = add(&polymul(&dr, &dr), &polymul(&di, &di));
    let t = sub(&polymul(&polyder(&f), &g), &polymul(&polyder(&g), &f));
    let wp = if phase.len() == 1 && phase[0] == 0.0 { vec![] } else { real_roots(&phase, true) };
    let wc = if mag.len() == 1 && mag[0] == 0.0 { vec![] } else { real_roots(&mag, false) };
    let dt = polyder(&t);
    let ws = if t.len() == 1 { vec![] } else { real_roots(&t, false).into_iter().filter(|w| polyval_r(&dt, *w) > 0.0).collect() };
    (wp, wc, ws)
}

/// (1+w)^a (1−w)^b expansion helper for the bilinear map z = (1+w)/(1−w).
fn bilinear(p: &[f64], big_n: usize) -> Vec<f64> {
    let np = p.len() - 1;
    let mut out = vec![0.0];
    for (k, c) in p.iter().enumerate() {
        let mut term = vec![*c];
        for _ in 0..(np - k) {
            term = polymul(&term, &[1.0, 1.0]);
        }
        for _ in 0..(big_n - np + k) {
            term = polymul(&term, &[-1.0, 1.0]);
        }
        out = polyadd_scaled(&out, &term, 1.0);
    }
    strip(&out)
}

impl Margin {
    fn g(&self, inp: &MarginInput, w: f64) -> C64 {
        aeoi_core::kernels::freqresp::tf_siso(&inp.num, &inp.den, w, inp.ts)
    }
    /// Secant refinement of f on ω (accept only if |f| decreases).
    fn refine(&self, w0: f64, f: &dyn Fn(f64) -> f64) -> f64 {
        let mut w = w0;
        let mut fw = f(w);
        let mut h = 1e-7 * w.abs().max(1e-12);
        for _ in 0..4 {
            if fw == 0.0 || !fw.is_finite() {
                break;
            }
            let d = (f(w + h) - fw) / h;
            if d == 0.0 || !d.is_finite() {
                break;
            }
            let w1 = w - fw / d;
            let f1 = f(w1);
            if f1.abs() < fw.abs() && w1 >= 0.0 {
                h = (w1 - w).abs().max(1e-14 * w1.abs()).min(h);
                w = w1;
                fw = f1;
            } else {
                break;
            }
        }
        w
    }
    fn bisect(&self, mut a: f64, mut b: f64, f: &dyn Fn(f64) -> f64) -> f64 {
        let mut fa = f(a);
        for _ in 0..200 {
            let m = 0.5 * (a + b);
            if m <= a || m >= b {
                break;
            }
            let fm = f(m);
            if (fm > 0.0) == (fa > 0.0) {
                a = m;
                fa = fm;
            } else {
                b = m;
            }
        }
        0.5 * (a + b)
    }
}

impl Operator for Margin {
    type Input = MarginInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(MarginInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        let (ny, nu) = sys.dims();
        if ny * nu != 1 {
            return Err(mimo_err("/system", "control.analysis.margin（v0.3）", ny, nu));
        }
        let (n, d) = match &sys {
            Lti::Tf { num, den, .. } => (num[0][0].clone(), den[0][0].clone()),
            Lti::Zpk { z, p, k, .. } => zpk2tf(z, p, *k),
            Lti::Ss { a, .. } => {
                crate::check_states(&self.meta, a.nrows(), "/system/A")?;
                let (n, d) = ss2tf(&sys.to_ss());
                (n[0][0].clone(), d)
            }
        };
        Ok((MarginInput { num: strip(&n), den: strip(&d), ts: sys.ts(), kind: sys.kind() }, w))
    }
    fn execute(&self, inp: MarginInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let ts = inp.ts;
        let disc = ts > 0.0;
        let zero_num = inp.num.len() == 1 && inp.num[0] == 0.0;
        // candidates in ν (continuous: ν = ω; discrete: ν = tan(ω ts / 2))
        let (wp, wc, ws) = if zero_num {
            (vec![], vec![], vec![])
        } else if disc {
            let nn = inp.den.len() - 1;
            let (pn, pd) = (bilinear(&inp.num, nn), bilinear(&inp.den, nn));
            let (a, b, c) = candidates(&pn, &pd);
            let map = |v: Vec<f64>| v.into_iter().map(|nu| 2.0 * nu.atan() / ts).filter(|w| *w < PI / ts).collect::<Vec<_>>();
            (map(a), map(b), map(c))
        } else {
            candidates(&inp.num, &inp.den)
        };
        ctx.check()?;
        let g = |w: f64| self.g(&inp, w);
        let fmag = |w: f64| g(w).norm().ln();
        let fph = |w: f64| {
            let v = g(w);
            v.im / v.norm()
        };
        ctx.check()?;
        let mut wp: Vec<f64> = wp.into_iter().map(|w| if w == 0.0 { 0.0 } else { self.refine(w, &fph) }).collect();
        let mut wc: Vec<f64> = wc.into_iter().map(|w| self.refine(w, &fmag)).collect();
        let mut ws = ws;
        if disc && !zero_num {
            // python-control 0.10.2 (discrete): SM = minimum of |1+G(e^{jθ})| over the whole circle, which may
            // sit at θ = π (Nyquist) or θ = 0 — both are stationary points for real coefficients. Add each
            // endpoint when it is a local minimum (one-sided test), so the global minimum is among the candidates.
            let f1 = |w: f64| (g(w) + 1.0).norm();
            let wn = PI / ts;
            let del = 1e-4 * wn;
            for (we, wi) in [(wn, wn - del), (0.0, del)] {
                let (fe, fi) = (f1(we), f1(wi));
                if fe.is_finite() && fi.is_finite() && fe <= fi && !ws.iter().any(|x| (x - we).abs() <= 1e-9 * wn) {
                    ws.push(we);
                }
            }
            ws.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
        // audit: dense log grid sign changes
        let mut recovered = 0usize;
        if !zero_num {
            let mut mags: Vec<f64> = roots(&inp.num).iter().chain(roots(&inp.den).iter()).map(|r| if disc { (r.ln() / ts).norm() } else { r.norm() }).filter(|x| *x > 0.0 && x.is_finite()).collect();
            mags.push(1.0);
            let lo = mags.iter().cloned().fold(f64::INFINITY, f64::min) * 1e-3;
            let hi = if disc { PI / ts * (1.0 - 1e-9) } else { mags.iter().cloned().fold(0.0, f64::max) * 1e3 };
            let npts = self.meta.manifest["limits"]["scan_points"].as_u64().unwrap_or(4000) as usize;
            let grid: Vec<f64> = (0..npts).map(|k| lo * (hi / lo).powf(k as f64 / (npts - 1) as f64)).collect();
            let vals: Vec<C64> = grid.iter().map(|w| g(*w)).collect();
            for k in 0..npts - 1 {
                // every loop here is bounded; the cooperative deadline (exec.timeout_ms) is a second guard
                if k % 256 == 0 {
                    ctx.check()?;
                }
                let (a, b) = (grid[k], grid[k + 1]);
                let (ga, gb) = (vals[k], vals[k + 1]);
                if !(ga.norm().is_finite() && gb.norm().is_finite()) {
                    continue;
                }
                let covered = |list: &Vec<f64>| list.iter().any(|w| *w >= a * (1.0 - 1e-9) && *w <= b * (1.0 + 1e-9));
                if (ga.norm() - 1.0).signum() != (gb.norm() - 1.0).signum() && !covered(&wc) {
                    let w = self.bisect(a, b, &fmag);
                    if fmag(w).abs() < 1e-6 {
                        wc.push(w);
                        recovered += 1;
                    }
                }
                if ga.im.signum() != gb.im.signum() && (ga.re < 0.0 || gb.re < 0.0) && !covered(&wp) {
                    let w = self.bisect(a, b, &fph);
                    let v = g(w);
                    if fph(w).abs() < 1e-6 && v.re < 0.0 && v.norm() < 1e6 * ga.norm().max(gb.norm()) {
                        wp.push(w);
                        recovered += 1;
                    }
                }
            }
        }
        if recovered > 0 {
            warnings.push(Warning::new("W.NUMERICAL.CROSSING_RECOVERED", format!("多项式求根漏检了 {recovered} 个交越点（通常是近重根/相切），已由网格符号变化 + 二分法补回。")).path("/system"));
        }
        let sortd = |v: &mut Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs().max(1e-300));
        };
        sortd(&mut wp);
        sortd(&mut wc);
        // phase crossings with Re G ≤ 0 (G finite), as python-control
        let wp: Vec<f64> = wp.into_iter().filter(|w| {
            let v = g(*w);
            v.re.is_finite() && v.im.is_finite() && v.re <= 0.0
        }).collect();
        let gms: Vec<f64> = wp.iter().map(|w| 1.0 / g(*w).norm()).collect();
        let pms: Vec<f64> = wc.iter().map(|w| g(*w).arg().to_degrees().rem_euclid(360.0) - 180.0).collect();
        let sms: Vec<f64> = ws.iter().map(|w| (g(*w) + 1.0).norm()).collect();
        let pick = |v: &Vec<f64>, key: &dyn Fn(f64) -> f64| -> Option<usize> {
            let mut best: Option<(usize, f64)> = None;
            for (i, x) in v.iter().enumerate() {
                let k = key(*x);
                if k.is_nan() {
                    continue;
                }
                if best.map(|b| k < b.1).unwrap_or(true) {
                    best = Some((i, k));
                }
            }
            best.map(|b| b.0)
        };
        let gi = if gms.iter().all(|x| x.is_infinite()) { None } else { pick(&gms, &|x| x.ln().abs()) };
        let pi = pick(&pms, &|x| x.abs());
        let si = pick(&sms, &|x| x);
        let gm = gi.map(|i| gms[i]).unwrap_or(f64::INFINITY);
        let pm = pi.map(|i| pms[i]).unwrap_or(f64::INFINITY);
        let sm = si.map(|i| sms[i]).unwrap_or(f64::INFINITY);
        // closed loop (negative unity feedback)
        let clp = roots(&add(&inp.num, &inp.den));
        let cl_stable = if disc { clp.iter().all(|p| p.norm() < 1.0) } else { clp.iter().all(|p| p.re < 0.0) };
        if !cl_stable {
            warnings.push(Warning::new("W.DOMAIN.CLOSED_LOOP_UNSTABLE", "负单位反馈闭环不稳定：此时裕度数值不能解释为鲁棒性保证。").path("/system"));
        }
        let max_res_mag = wc.iter().map(|w| fmag(*w).abs()).fold(0.0, f64::max);
        let max_res_ph = wp.iter().map(|w| fph(*w).abs()).fold(0.0, f64::max);
        let q = |u: &str, v: f64| json!({"unit": u, "value": num(v)});
        let result = json!({
            "gain_margin": q("1", gm), "gain_margin_db": q("dB", 20.0 * gm.log10()), "phase_margin": q("deg", pm), "stability_margin": q("1", sm),
            "phase_crossover_frequency": q("rad/s", gi.map(|i| wp[i]).unwrap_or(f64::NAN)),
            "gain_crossover_frequency": q("rad/s", pi.map(|i| wc[i]).unwrap_or(f64::NAN)),
            "stability_margin_frequency": q("rad/s", si.map(|i| ws[i]).unwrap_or(f64::NAN)),
            "crossings": {
                "phase": wp.iter().zip(&gms).map(|(w, m)| json!({"w_rad_s": num(*w), "gain_margin": num(*m), "gain_margin_db": num(20.0 * m.log10())})).collect::<Vec<_>>(),
                "gain": wc.iter().zip(&pms).map(|(w, m)| json!({"w_rad_s": num(*w), "phase_margin_deg": num(*m)})).collect::<Vec<_>>(),
                "stability": ws.iter().zip(&sms).map(|(w, m)| json!({"w_rad_s": num(*w), "stability_margin": num(*m)})).collect::<Vec<_>>()
            },
            "closed_loop_stable": cl_stable
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"gain_crossing_max_abs_ln_mag": max_res_mag, "phase_crossing_max_abs_sin_arg": max_res_ph,
                         "definition": "增益交越处 |ln|G(jω)||、相位交越处 |Im G|/|G| 的最大值（理想为 0）"},
            "condition": {"tf_from": inp.kind, "num_degree": inp.num.len() - 1, "den_degree": inp.den.len() - 1, "grid_audit_recovered": recovered},
            "error_estimate": {"basis": "交越频率为多项式实根（伴随矩阵特征值 + Newton），再在 G 上做割线修正；残差见 residual。相切/近重根时交越频率误差可达 O(√u)"},
            "checks": {"closed_loop_stable": cl_stable, "grid_audit_clean": recovered == 0},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"closed_loop_poles": aeoi_core::matrix::complex_vector(&clp)}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_and_bilinear() {
        // p(s) = s^2 + 2s + 5 -> p(jw) = (5 - w^2) + j 2w
        let (r, i) = split_iw(&[1.0, 2.0, 5.0]);
        assert_eq!(r, vec![-1.0, 0.0, 5.0]);
        assert_eq!(i, vec![2.0, 0.0]);
        // z - 0.5 with N=1 -> (1+w) - 0.5(1-w) = 1.5 w + 0.5
        assert_eq!(bilinear(&[1.0, -0.5], 1), vec![1.5, 0.5]);
    }
}
