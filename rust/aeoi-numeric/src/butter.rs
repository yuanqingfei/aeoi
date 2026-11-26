//! signal.design.butter — Butterworth filter design (spec 10.24, v0.4.12).
//!
//! Analog prototype → lp2lp / lp2hp / lp2bp / lp2bs → (digital) bilinear transform with prewarping, exactly
//! the scipy.signal.butter construction (also MATLAB butter's), on the shared kernels in
//! aeoi_core::kernels::filter. Author's decisions (2026-11-25): zpk and second-order sections are the primary
//! outputs, b/a is also returned but warned about when the polynomial form has lost accuracy; the cutoff
//! must be given explicitly as Hz (+ sample rate) or as MATLAB's normalised Wn (1 = Nyquist), never
//! defaulted; analog designs (domain = analog) are supported and returned as an LTI system as well.
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::kernels::filter::{ba_eval_s, ba_eval_z, sos_eval_z};
use aeoi_core::kernels::iir::*;
use aeoi_core::lti::C64;
use aeoi_libm::libm;
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Butter, "signal.design.butter");

/// Relative deviation of the b/a response from the zpk response above which b/a is reported as unreliable.
pub const BA_WARN: f64 = 1e-8;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Low,
    High,
    BandPass,
    BandStop,
}

pub struct In {
    n: usize,
    kind: Kind,
    analog: bool,
    /// digital: normalised edges (1 = Nyquist); analog: rad/s
    edges: Vec<f64>,
    fs: Option<f64>,
    given: &'static str,
}

fn nums(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::Number(x) => Some(vec![x.as_f64()?]),
        Value::Array(a) => a.iter().map(|x| x.as_f64()).collect(),
        _ => None,
    }
}

fn cjson(v: &[C64]) -> Value {
    json!(v.iter().map(|z| json!([num(z.re), num(z.im)])).collect::<Vec<_>>())
}

impl Operator for Butter {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let n = args["order"].as_u64().unwrap() as usize;
        let lim = self.meta.manifest["limits"]["max_order"].as_u64().unwrap() as usize;
        if n > lim {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("order = {n} 超过上限 {lim}。"), "降低阶数；很陡的过渡带考虑椭圆或切比雪夫设计。").path("/order"));
        }
        let analog = args.get("domain").and_then(|v| v.as_str()) == Some("analog");
        let has = |k: &str| args.get(k).is_some();
        let present: Vec<&str> = ["cutoff_hz", "cutoff_normalized", "cutoff_rad_s"].into_iter().filter(|k| has(k)).collect();
        let allowed: &[&str] = if analog { &["cutoff_hz", "cutoff_rad_s"] } else { &["cutoff_hz", "cutoff_normalized"] };
        if let Some(bad) = present.iter().find(|k| !allowed.contains(k)) {
            let why = if analog { "模拟滤波器的截止频率用 cutoff_rad_s 或 cutoff_hz（不是归一化频率）" } else { "数字滤波器的截止频率用 cutoff_hz（加 sample_rate_hz）或 cutoff_normalized（= MATLAB 的 Wn，1 = Nyquist）；cutoff_rad_s 只用于模拟滤波器" };
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("{bad} 不适用于 domain = {}：{why}。", if analog { "analog" } else { "digital" }), "删掉这个字段，或改 domain。")
                .path(&format!("/{bad}"))
                .remedy(modify_input("drop_field", &format!("删除 {bad}"), "medium", true, json!([{"op": "remove", "path": format!("/{bad}")}]))));
        }
        if present.len() != 1 {
            let msg = if present.is_empty() { "没有给出截止频率。".to_string() } else { format!("同时给出了 {}；截止频率只能用一种写法。", present.join(" 和 ")) };
            let mut e = OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("{msg}本算子要求显式二选一：{}。", if analog { "cutoff_rad_s 或 cutoff_hz" } else { "cutoff_hz（加 sample_rate_hz）或 cutoff_normalized（MATLAB 的 Wn，0 < Wn < 1，1 = Nyquist）" }), "问清用户的截止频率与单位，不要猜（Hz、rad/s、归一化频率经常被混淆）。").path("/");
            for k in &present {
                e = e.remedy(modify_input(&format!("keep_other_than_{k}"), &format!("删除 {k}"), "low", true, json!([{"op": "remove", "path": format!("/{k}")}])));
            }
            return Err(e.remedy(ask_user("cutoff_and_unit", "截止频率是多少，单位是什么（Hz、rad/s，还是相对 Nyquist 的归一化频率）？")));
        }
        let key = present[0];
        let raw = nums(&args[key]).ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", format!("{key} 要是一个数或两个数的数组。"), "低通/高通给一个数，带通/带阻给 [下限, 上限]。").path(&format!("/{key}")))?;
        let fs = args.get("sample_rate_hz").and_then(|v| v.as_f64());
        if !analog && key == "cutoff_hz" && fs.is_none() {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", "cutoff_hz 需要 sample_rate_hz 才能换算成数字频率。", "给出采样率（Hz），或改用 cutoff_normalized。").path("/sample_rate_hz").remedy(ask_user("sample_rate", "采样率是多少 Hz？")));
        }
        if (analog || key == "cutoff_normalized") && fs.is_some() {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("sample_rate_hz 与 {} 一起给出没有意义。", if analog { "模拟滤波器" } else { "cutoff_normalized" }), "删掉 sample_rate_hz，或改用 cutoff_hz。").path("/sample_rate_hz").remedy(modify_input("drop_fs", "删除 sample_rate_hz", "medium", true, json!([{"op": "remove", "path": "/sample_rate_hz"}]))));
        }
        let kind = match args.get("type").and_then(|v| v.as_str()) {
            Some("high") => Kind::High,
            Some("bandpass") => Kind::BandPass,
            Some("bandstop") => Kind::BandStop,
            Some(_) => Kind::Low,
            None => if raw.len() == 2 { Kind::BandPass } else { Kind::Low },
        };
        let want = if matches!(kind, Kind::BandPass | Kind::BandStop) { 2 } else { 1 };
        if raw.len() != want {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("{key} 有 {} 个值；该类型要 {want} 个（低通/高通一个，带通/带阻 [下限, 上限]）。", raw.len()), "按滤波器类型给出截止频率。").path(&format!("/{key}")));
        }
        if raw.iter().any(|v| !(*v > 0.0) || !v.is_finite()) || (want == 2 && !(raw[0] < raw[1])) {
            return Err(OpError::new("E.VALIDATION.RANGE", format!("{key} = {raw:?}：截止频率必须为正，带通/带阻要求下限 < 上限。"), "检查截止频率。").path(&format!("/{key}")));
        }
        let (edges, given) = if analog {
            (if key == "cutoff_hz" { raw.iter().map(|f| 2.0 * std::f64::consts::PI * f).collect() } else { raw.clone() }, if key == "cutoff_hz" { "cutoff_hz" } else { "cutoff_rad_s" })
        } else {
            let w: Vec<f64> = if key == "cutoff_hz" { raw.iter().map(|f| f / (fs.unwrap() / 2.0)).collect() } else { raw.clone() };
            if let Some(bad) = w.iter().find(|v| **v >= 1.0) {
                let detail = if key == "cutoff_hz" { format!("{} Hz ≥ Nyquist {} Hz", bad * fs.unwrap() / 2.0, fs.unwrap() / 2.0) } else { format!("Wn = {bad} ≥ 1") };
                return Err(OpError::new("E.DOMAIN.ABOVE_NYQUIST", format!("截止频率达到或超过 Nyquist 频率：{detail}。"), "数字滤波器的截止频率必须低于采样率的一半；检查采样率，或 Wn 是否误用了 Hz / rad/sample。").path(&format!("/{key}")).remedy(ask_user("cutoff_units", "截止频率与采样率是否写对了单位？")));
            }
            (w, if key == "cutoff_hz" { "cutoff_hz" } else { "cutoff_normalized" })
        };
        Ok((In { n, kind, analog, edges, fs, given }, Vec::new()))
    }

    fn execute(&self, i: In, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let proto = buttap(i.n);
        // digital: prewarp with fs = 2 (normalised frequency, 1 = Nyquist), like scipy
        let band = match i.kind {
            Kind::Low => Band::Low,
            Kind::High => Band::High,
            Kind::BandPass => Band::BandPass,
            Kind::BandStop => Band::BandStop,
        };
        let f = design(&proto, band, &i.edges, i.analog);
        let (b, aa) = zpk2tf(&f);
        let sos = if i.analog { None } else { Some(zpk2sos(&f)) };
        // response grid: digital on the unit circle, analog on jω around the band
        let pts: Vec<C64> = if i.analog {
            let w0 = if i.edges.len() == 2 { (i.edges[0] * i.edges[1]).sqrt() } else { i.edges[0] };
            (0..64).map(|k| C64::new(0.0, w0 * libm::pow(10.0, -2.0 + 4.0 * k as f64 / 63.0))).collect()
        } else {
            (0..64).map(|k| { let t = std::f64::consts::PI * (k as f64 + 0.5) / 64.0; C64::new(libm::cos(t), libm::sin(t)) }).collect()
        };
        let hz: Vec<C64> = pts.iter().map(|x| zpk_eval(&f, *x)).collect();
        let hmax = hz.iter().map(|h| h.norm()).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
        let ba_dev = pts.iter().zip(&hz).map(|(x, h)| ((if i.analog { ba_eval_s(&b, &aa, *x) } else { ba_eval_z(&b, &aa, *x) }) - h).norm()).fold(0.0, f64::max) / hmax;
        let sos_dev = sos.as_ref().map(|s| pts.iter().zip(&hz).map(|(x, h)| (sos_eval_z(s, *x) - h).norm()).fold(0.0, f64::max) / hmax);
        // |H|² = 1/2 at every edge (Butterworth: −3.0103 dB)
        let edge_pts: Vec<C64> = i.edges.iter().map(|e| if i.analog { C64::new(0.0, *e) } else { let t = std::f64::consts::PI * e; C64::new(libm::cos(t), libm::sin(t)) }).collect();
        let edge_gain: Vec<f64> = edge_pts.iter().map(|x| zpk_eval(&f, *x).norm_sqr()).collect();
        let edge_err = edge_gain.iter().map(|g| (g - 0.5).abs()).fold(0.0, f64::max);
        let stable = f.p.iter().all(|p| if i.analog { p.re < 0.0 } else { p.norm() < 1.0 });
        let max_radius = f.p.iter().map(|p| if i.analog { p.re } else { p.norm() }).fold(f64::NEG_INFINITY, f64::max);
        let mut warnings = Vec::new();
        if !(ba_dev <= BA_WARN) {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("多项式形式 b/a 已失去精度：其频率响应与零极点形式相差 {ba_dev:.1e}（相对最大增益）。{}阶数较高或截止频率很低/很高时常见。请用 sos（filter 的缺省）或 zpk，不要用 b/a。", if i.analog { "" } else { "" })).path("/order").details(json!({"ba_relative_deviation": num(ba_dev), "threshold": BA_WARN, "unreliable_fields": ["/b", "/a"]})));
        }
        let finite = b.iter().chain(aa.iter()).all(|v| v.is_finite()) && f.p.iter().chain(f.z.iter()).all(|z| z.re.is_finite() && z.im.is_finite()) && f.k.is_finite();
        let kind = match i.kind {
            Kind::Low => "low",
            Kind::High => "high",
            Kind::BandPass => "bandpass",
            Kind::BandStop => "bandstop",
        };
        let system = if i.analog {
            json!({"kind": "zpk", "z": cjson(&f.z), "p": cjson(&f.p), "k": num(f.k), "ts": 0.0, "time_unit": "s"})
        } else if let Some(fs) = i.fs {
            json!({"kind": "zpk", "z": cjson(&f.z), "p": cjson(&f.p), "k": num(f.k), "ts": num(1.0 / fs), "time_unit": "s"})
        } else {
            Value::Null
        };
        let result = json!({
            "z": cjson(&f.z), "p": cjson(&f.p), "k": num(f.k),
            "sos": match &sos { Some(s) => json!({"dtype": "f64", "shape": [s.len(), 6], "data": s.iter().flatten().map(|v| num(*v)).collect::<Vec<_>>()}), None => Value::Null },
            "b": b.iter().map(|v| num(*v)).collect::<Vec<_>>(),
            "a": aa.iter().map(|v| num(*v)).collect::<Vec<_>>(),
            "system": system,
            "type": kind, "order": i.n, "domain": if i.analog { "analog" } else { "digital" },
            "cutoff": if i.analog { json!({"rad_s": i.edges.iter().map(|v| num(*v)).collect::<Vec<_>>()}) } else {
                json!({"normalized": i.edges.iter().map(|v| num(*v)).collect::<Vec<_>>(), "hz": match i.fs { Some(fs) => json!(i.edges.iter().map(|v| num(v * fs / 2.0)).collect::<Vec<_>>()), None => Value::Null }})
            },
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"edge_gain_error": num(edge_err), "sos_relative_deviation": match sos_dev { Some(d) => num(d), None => Value::Null }, "ba_relative_deviation": num(ba_dev),
                "definition": "edge_gain_error = max |‖H(截止点)‖² − 1/2|；*_relative_deviation = 64 个频点上与零极点形式响应之差的最大值 / max|H|"},
            "condition": {"max_pole_radius": num(max_radius), "note": if i.analog { "最右极点的实部" } else { "极点最大模（越接近 1，滤波越敏感）" }},
            "error_estimate": {"applicable": false},
            "checks": {"stable": stable, "edge_gain": edge_err <= 1e-10, "sos_consistent": sos_dev.map(|d| d <= 1e-10).unwrap_or(true), "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"sos_consistent_applies": !i.analog, "cutoff_given_as": i.given, "prewarp": if i.analog { "none" } else { "4·tan(π·Wn/2)（fs = 2，同 scipy）" }}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn call(args: Value) -> Value {
        Butter::new().call(&args, &HostCtx::default())
    }
    #[test]
    fn lowpass_order2_matches_scipy() {
        // scipy.signal.butter(2, 0.5): b = [0.29289322, 0.58578644, 0.29289322], a = [1, 0, 0.17157288]
        let e = call(json!({"order": 2, "cutoff_normalized": 0.5}));
        assert_eq!(e["status"], "ok", "{e}");
        let b: Vec<f64> = e["result"]["b"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let a: Vec<f64> = e["result"]["a"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((b[0] - 0.2928932188134524).abs() < 1e-15 && (a[2] - 0.1715728752538099).abs() < 1e-15 && a[1].abs() < 1e-15, "{b:?} {a:?}");
    }
    #[test]
    fn cutoff_rules() {
        assert_eq!(call(json!({"order": 2}))["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert_eq!(call(json!({"order": 2, "cutoff_hz": 10.0}))["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert_eq!(call(json!({"order": 2, "cutoff_hz": 60.0, "sample_rate_hz": 100.0}))["error"]["code"], "E.DOMAIN.ABOVE_NYQUIST");
        let e = call(json!({"order": 4, "cutoff_hz": [10.0, 20.0], "sample_rate_hz": 100.0}));
        assert_eq!(e["result"]["type"], "bandpass", "{e}");
        assert_eq!(e["result"]["sos"]["shape"], json!([4, 6]));
    }
    #[test]
    fn analog_is_an_lti_system() {
        let e = call(json!({"order": 3, "domain": "analog", "cutoff_rad_s": 2.0}));
        assert_eq!(e["status"], "ok", "{e}");
        assert_eq!(e["result"]["system"]["kind"], "zpk");
        assert_eq!(e["result"]["sos"], Value::Null);
    }
    #[test]
    fn high_order_ba_warns() {
        let e = call(json!({"order": 20, "cutoff_normalized": 0.01}));
        let codes: Vec<&str> = e["warnings"].as_array().unwrap().iter().map(|w| w["code"].as_str().unwrap()).collect();
        assert!(codes.contains(&"W.NUMERICAL.ILL_CONDITIONED"), "{}", e["trust"]);
        assert_eq!(e["trust"]["checks"]["sos_consistent"], true, "{}", e["trust"]);
    }
}
