//! signal.spectral.fft — discrete Fourier transform with every convention explicit (spec 10.17, v0.4.7).
//!
//! X_k = s·Σ_j x_j e^{−2πi·jk/n} (forward) / x_j = s'·Σ_k X_k e^{+2πi·jk/n} (inverse); normalization
//! backward (s = 1, s' = 1/n; MATLAB and numpy), ortho (1/√n both) or forward (s = 1/n, s' = 1).
//! The transform itself is aeoi-core kernels::fft (own radix-2 / mixed-radix / Bluestein; v0.4.16, was rustfft).
//! Post-conditions: Parseval (all outputs) and a direct O(n) compensated DFT at 8 fixed bins (sampled).
use aeoi_core::kernels::sum::{neumaier, Neumaier};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::matrix::{parse_array, to_real_matrix, Array, Elems};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::Complex;
use serde_json::{json, Value};

operator_struct!(Fft, "signal.spectral.fft");

type C = Complex<f64>;
const U: f64 = f64::EPSILON / 2.0;
const SPOT_BINS: usize = 8;

pub struct In {
    rows: Vec<Vec<C>>, // m rows, each padded/truncated to n
    one_d: bool,
    complex_input: bool,
    input_length: usize,
    n: usize,
    inverse: bool,
    norm: &'static str,
    one_sided: bool,
    fs: Option<f64>,
}

fn present(args: &Value, k: &str) -> bool {
    args.get(k).is_some_and(|v| !v.is_null())
}

/// Scale applied to the transform output for this direction and normalization.
fn scale(n: usize, inverse: bool, norm: &str) -> f64 {
    let nf = n as f64;
    match (norm, inverse) {
        ("ortho", _) => 1.0 / nf.sqrt(),
        ("forward", false) | ("backward", true) => 1.0 / nf,
        _ => 1.0,
    }
}

/// numpy fftfreq (two-sided) / rfftfreq (one-sided), times fs.
pub fn frequencies(n: usize, n_out: usize, one_sided: bool, fs: f64) -> Vec<f64> {
    let nf = n as f64;
    (0..n_out)
        .map(|k| {
            let kk = if one_sided || k < n.div_ceil(2) { k as f64 } else { k as f64 - nf };
            kk * fs / nf
        })
        .collect()
}

/// The 8 spot-check bins: {0, 1, n−1, ⌊n/2⌋, ⌊n/3⌋, ⌊n/4⌋, ⌊2n/3⌋, ⌊3n/4⌋} ∩ [0, n_out), deduplicated.
pub fn spot_bins(n: usize, n_out: usize) -> Vec<usize> {
    let mut b: Vec<usize> = [0, 1, n.saturating_sub(1), n / 2, n / 3, n / 4, 2 * n / 3, 3 * n / 4].into_iter().filter(|&k| k < n_out).collect();
    b.sort_unstable();
    b.dedup();
    b.truncate(SPOT_BINS);
    b
}

/// Direct DFT at one bin with exact integer reduction of j·k mod n and compensated (Neumaier) sums.
fn direct_bin(x: &[C], k: usize, inverse: bool) -> C {
    let n = x.len();
    let sign = if inverse { 1.0 } else { -1.0 };
    let (mut re, mut im) = (Neumaier::default(), Neumaier::default());
    for (j, xj) in x.iter().enumerate() {
        let r = ((j as u128 * k as u128) % n as u128) as f64;
        let ang = sign * 2.0 * std::f64::consts::PI * r / n as f64;
        let w = C::new(ang.cos(), ang.sin());
        let p = xj * w;
        re.add(p.re);
        im.add(p.im);
    }
    C::new(re.value(), im.value())
}

fn complex_dense(shape: &[usize], v: &[C]) -> Value {
    json!({"dtype": "c128", "shape": shape, "data": v.iter().map(|c| json!([num(c.re), num(c.im)])).collect::<Vec<_>>()})
}

impl Operator for Fft {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let arr = parse_array(&args["x"], "/x", host, false)?;
        // rows of complex samples, plus the input layout
        let (rows, one_d, complex_input): (Vec<Vec<C>>, bool, bool) = match &arr {
            Array::Dense(d) => {
                if d.shape.is_empty() || d.shape.len() > 2 {
                    return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 的维数是 {}，fft 只接受 [n] 或 [m, n]（沿最后一维变换）。", d.shape.len()), "高维数据先展成 [m, n]：每行一个要变换的序列。").path("/x"));
                }
                let (m, len) = if d.shape.len() == 1 { (1, d.shape[0]) } else { (d.shape[0], d.shape[1]) };
                let vals: Vec<C> = match &d.data {
                    Elems::F64(v) => v.iter().map(|&r| C::new(r, 0.0)).collect(),
                    Elems::C128(v) => v.clone(),
                    Elems::I64(v) => v.iter().map(|&r| C::new(r as f64, 0.0)).collect(),
                    Elems::Bool(_) => return Err(OpError::new("E.VALIDATION.DTYPE", "x 的 dtype 是 bool，fft 需要数值（f64、c128 或 i64）。", "把 x 转成 f64。").path("/x/dtype")),
                };
                let rows = (0..m).map(|i| vals[i * len..(i + 1) * len].to_vec()).collect();
                (rows, d.shape.len() == 1, matches!(d.data, Elems::C128(_)))
            }
            Array::Sparse(_) => {
                let mm = to_real_matrix(&arr, "/x", &mut w, self.meta.max_dense("/x"))?;
                ((0..mm.nrows()).map(|i| mm.row(i).iter().map(|&r| C::new(r, 0.0)).collect()).collect(), false, false)
            }
        };
        let input_length = rows.first().map(|r| r.len()).unwrap_or(0);
        if rows.is_empty() || input_length == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "x 是空的（没有要变换的采样）。", "至少给出一个采样。").path("/x"));
        }
        let n = args.get("n").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(input_length);
        let inverse = args.get("direction").and_then(|v| v.as_str()) == Some("inverse");
        let norm: &'static str = match args.get("normalization").and_then(|v| v.as_str()).unwrap_or("backward") {
            "ortho" => "ortho",
            "forward" => "forward",
            _ => "backward",
        };
        let one_sided = args.get("sides").and_then(|v| v.as_str()) == Some("one");
        if one_sided && (complex_input || inverse) {
            let why = if inverse { "逆变换" } else { "复数输入（dtype c128）" };
            let mut conflicting = vec!["sides"];
            conflicting.push(if inverse { "direction" } else { "x" });
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("sides = one 只用于实数输入的正变换（负频率是正频率的共轭，才可以省略）；这里是{why}。不能静默改成双边（F-8）。"), "去掉 sides（双边）；实数信号的单边谱请用 dtype f64 的正变换。")
                .path("/sides")
                .detail("conflicting_fields", json!(conflicting))
                .remedy(modify_input("two_sided", "去掉 sides，返回双边谱", "high", false, json!([{"op": "remove", "path": "/sides"}]))));
        }
        let m = rows.len();
        let lim_in = self.meta.manifest["limits"]["max_input_elements"].as_u64().unwrap() as usize;
        if m.saturating_mul(n.max(input_length)) > lim_in {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{m} × {} 个采样超过单次上限 {lim_in}。", n.max(input_length)), "分批变换（按通道分组）。").path("/x"));
        }
        let n_out = if one_sided { n / 2 + 1 } else { n };
        let lim_out = self.meta.manifest["limits"]["max_output_values"].as_u64().unwrap() as usize;
        if m.saturating_mul(n_out) > lim_out {
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("输出 {m} × {n_out} 个频点超过单次上限 {lim_out}。"), "分批变换，或对实数信号用 sides = one。").path("/n"));
        }
        let rows = rows
            .into_iter()
            .map(|mut r| {
                r.resize(n, C::new(0.0, 0.0)); // truncate or zero-pad at the end
                r
            })
            .collect();
        let fs = if present(args, "sample_rate_hz") { args["sample_rate_hz"].as_f64() } else { None };
        Ok((In { rows, one_d, complex_input, input_length, n, inverse, norm, one_sided, fs }, w))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.n;
        let m = i.rows.len();
        let sc = scale(n, i.inverse, i.norm);
        let n_out = if i.one_sided { n / 2 + 1 } else { n };
        let (mut parseval_worst, mut spot_worst) = (0.0f64, 0.0f64);
        let mut out = Vec::with_capacity(m * n_out);
        let lg = (n.max(2) as f64).log2().ceil();
        let tol_parseval = 64.0 * (lg + 1.0) * U;
        let tol_spot = 64.0 * (lg + 1.0) * U;
        let bins = spot_bins(n, n_out);
        for row in &i.rows {
            let mut buf = aeoi_core::kernels::fft::fft(row, i.inverse);
            if sc != 1.0 {
                for z in buf.iter_mut() {
                    *z *= sc;
                }
            }
            // Parseval: Σ|out|² = sc²·n·Σ|in|²  (one-sided: count the mirrored bins twice)
            let e_in = neumaier(row.iter().map(|z| z.norm_sqr()));
            let e_out = neumaier((0..n_out).map(|k| {
                let w = if !i.one_sided || k == 0 || (n % 2 == 0 && k == n / 2) { 1.0 } else { 2.0 };
                w * buf[k].norm_sqr()
            }));
            let want = sc * sc * n as f64 * e_in;
            let rel = if want > 0.0 { (e_out - want).abs() / want } else { e_out.abs() };
            parseval_worst = parseval_worst.max(rel);
            // direct DFT at fixed bins, error relative to the per-bin bound sc·Σ|x_j|
            let l1: f64 = neumaier(row.iter().map(|z| z.norm()));
            for &k in &bins {
                let d = direct_bin(row, k, i.inverse) * sc;
                let e = (d - buf[k]).norm() / (sc * l1).max(f64::MIN_POSITIVE);
                spot_worst = spot_worst.max(e);
            }
            out.extend_from_slice(&buf[..n_out]);
            ctx.check()?;
        }
        let parseval_ok = parseval_worst <= tol_parseval;
        let spot_ok = spot_worst <= tol_spot;
        let finite = out.iter().all(|z| z.re.is_finite() && z.im.is_finite());
        let fs = i.fs.unwrap_or(1.0);
        let unit = if i.fs.is_some() { "Hz" } else { "cycles/sample" };
        let freq = frequencies(n, n_out, i.one_sided, fs);
        let shape: Vec<usize> = if i.one_d { vec![n_out] } else { vec![m, n_out] };
        let result = json!({
            "X": complex_dense(&shape, &out),
            "frequency": {"unit": unit, "value": {"dtype": "f64", "shape": [n_out], "data": freq.iter().map(|&f| num(f)).collect::<Vec<_>>()}},
            "n": n,
            "direction": if i.inverse { "inverse" } else { "forward" },
            "normalization": i.norm,
            "sides": if i.one_sided { "one" } else { "two" },
            "input_length": i.input_length,
            "frequency_resolution": {"unit": unit, "value": fs / n as f64}
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"parseval_rel": num(parseval_worst), "dft_spot_rel": num(spot_worst),
                         "definition": format!("Parseval：|Σ|out|² − sc²·n·Σ|in|²| / (sc²·n·Σ|in|²)，各通道取最大；抽查：{} 个频点上直接 DFT 与 FFT 之差 / (sc·Σ|x_j|)", bins.len())},
            "condition": {"n": n, "log2_n": lg, "complex_input": i.complex_input},
            "error_estimate": {"X_rel_normwise": num(10.0 * (lg + 1.0) * U),
                               "basis": "先验范数界 ‖ΔX‖₂/‖X‖₂ ≲ c·log₂n·u（Higham 2002 定理 24.2，基 2；其它分解与 Bluestein 的常数更大，此处取 c = 10 作为估计）"},
            "checks": {"parseval": parseval_ok, "dft_spot_check": spot_ok, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"scale": sc, "spot_bins": bins, "tolerances": {"parseval_rel": tol_parseval, "dft_spot_rel": tol_spot}, "planner": "aeoi-core kernels::fft (radix-2 / mixed radix / Bluestein)"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        Fft::new().call(&args, &HostCtx::default())
    }
    fn xs(env: &Value) -> Vec<C> {
        env["result"]["X"]["data"].as_array().unwrap().iter().map(|p| C::new(p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
    }

    /// cos(2π·3j/16): bins 3 and 13 equal 8 (backward), the rest 0; one-sided keeps bin 3.
    #[test]
    fn single_tone() {
        let x: Vec<f64> = (0..16).map(|j| (2.0 * std::f64::consts::PI * 3.0 * j as f64 / 16.0).cos()).collect();
        let e = call(json!({"x": {"dtype": "f64", "shape": [16], "data": x}, "sample_rate_hz": 16.0}));
        assert_eq!(e["status"], "ok", "{e}");
        let v = xs(&e);
        for (k, z) in v.iter().enumerate() {
            let want = if k == 3 || k == 13 { 8.0 } else { 0.0 };
            assert!((z - C::new(want, 0.0)).norm() < 1e-13, "bin {k}: {z}");
        }
        let f = e["result"]["frequency"]["value"]["data"].as_array().unwrap();
        assert_eq!(f[3], 3.0);
        assert_eq!(f[13], -3.0);
        assert_eq!(f[8], -8.0); // even n: Nyquist bin labelled −fs/2 (numpy fftfreq)
        assert_eq!(e["trust"]["checks"]["parseval"], true);
        assert_eq!(e["trust"]["checks"]["dft_spot_check"], true);
        let e1 = call(json!({"x": {"dtype": "f64", "shape": [16], "data": x}, "sides": "one"}));
        assert_eq!(e1["result"]["X"]["shape"], json!([9]));
        assert_eq!(xs(&e1), v[..9].to_vec(), "one-sided = first n/2+1 two-sided bins, bitwise");
    }

    /// inverse(forward(x)) = x for every normalization; prime length (Bluestein / Rader path).
    #[test]
    fn roundtrip_all_normalizations_prime() {
        let x: Vec<[f64; 2]> = (0..97).map(|j| [(j as f64 * 0.37).sin(), (j as f64 * 0.11).cos()]).collect();
        for norm in ["backward", "ortho", "forward"] {
            let f = call(json!({"x": {"dtype": "c128", "shape": [97], "data": x}, "normalization": norm}));
            assert_eq!(f["status"], "ok", "{f}");
            let b = call(json!({"x": f["result"]["X"], "normalization": norm, "direction": "inverse"}));
            let back = xs(&b);
            let worst = back.iter().zip(&x).map(|(z, w)| (z - C::new(w[0], w[1])).norm()).fold(0.0, f64::max);
            assert!(worst < 1e-13, "{norm}: {worst}");
        }
    }

    #[test]
    fn pad_truncate_and_conflicts() {
        let x = json!({"dtype": "f64", "shape": [4], "data": [1.0, 2.0, 3.0, 4.0]});
        let e = call(json!({"x": x, "n": 6}));
        assert_eq!(e["result"]["n"], 6);
        assert_eq!(e["result"]["input_length"], 4);
        assert_eq!(xs(&e)[0], C::new(10.0, 0.0));
        let e = call(json!({"x": x, "n": 2}));
        assert_eq!(xs(&e), vec![C::new(3.0, 0.0), C::new(-1.0, 0.0)]);
        let e = call(json!({"x": {"dtype": "c128", "shape": [2], "data": [[1.0, 0.0], [0.0, 1.0]]}, "sides": "one"}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let e = call(json!({"x": x, "sides": "one", "direction": "inverse"}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let e = call(json!({"x": {"dtype": "f64", "shape": [1, 2, 2], "data": [1.0, 2.0, 3.0, 4.0]}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SHAPE");
    }

    /// [m, n]: each row transformed independently, shape kept.
    #[test]
    fn multichannel_rows() {
        let e = call(json!({"x": {"dtype": "f64", "shape": [2, 4], "data": [1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]}}));
        assert_eq!(e["result"]["X"]["shape"], json!([2, 4]));
        let v = xs(&e);
        assert_eq!(&v[..4], &[C::new(1.0, 0.0); 4]);
        assert_eq!(v[4], C::new(4.0, 0.0));
        assert!(v[5..].iter().all(|z| z.norm() < 1e-15));
    }
}
