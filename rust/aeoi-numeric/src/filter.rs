//! signal.filter.filter — apply an IIR/FIR filter (spec 10.25, v0.4.12): transposed direct form II on b/a
//! (MATLAB filter, scipy lfilter) or a cascade of second-order sections (scipy sosfilt), every channel (row
//! of x) independently, optional initial state. Built on aeoi_core::kernels::filter (the kernels butter
//! designs with); stability via crate::poly::roots (the same root finder as control.lti.poles_zeros).
use crate::interp1::vec1;
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::kernels::sum::Dot2;
use aeoi_core::kernels::filter::{lfilter, sosfilt};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Filter, "signal.filter.filter");

const U: f64 = f64::EPSILON / 2.0;

enum Form {
    Ba(Vec<f64>, Vec<f64>),
    Sos(Vec<[f64; 6]>),
}

pub struct In {
    x: Vec<Vec<f64>>,
    x_1d: bool,
    form: Form,
    zi: Option<Vec<Vec<f64>>>, // per channel, flattened state
}

fn poly(v: &Value, path: &str) -> Result<Vec<f64>, OpError> {
    v.as_array()
        .and_then(|a| a.iter().map(|x| x.as_f64()).collect::<Option<Vec<f64>>>())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", format!("{path} 要是非空的实数数组（z⁻¹ 的升幂系数，同 MATLAB）。"), "例如 b = [b0, b1, …]，a = [1, a1, …]。").path(path))
}

/// Residual of the difference equation a·y = b·x (both normalised by a0), compensated, relative to the local
/// scale Σ|a_i y| + Σ|b_i x|; samples n < skip are not checked.
fn de_residual(b: &[f64], a: &[f64], x: &[f64], y: &[f64], skip: usize) -> f64 {
    let a0 = a[0];
    let mut worst = 0.0f64;
    for n in skip..x.len() {
        let mut d = Dot2::default();
        let mut scale = 0.0;
        for (i, ai) in a.iter().enumerate() {
            if i <= n {
                d.add_prod(ai / a0, y[n - i]);
                scale += (ai / a0 * y[n - i]).abs();
            }
        }
        for (i, bi) in b.iter().enumerate() {
            if i <= n {
                d.add_prod(-bi / a0, x[n - i]);
                scale += (bi / a0 * x[n - i]).abs();
            }
        }
        let (h, l) = d.hi_lo();
        let r = (h + l).abs();
        if scale > 0.0 {
            worst = worst.max(r / scale);
        } else if r > 0.0 {
            worst = f64::INFINITY;
        }
    }
    worst
}

impl Operator for Filter {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let has_ba = args.get("b").is_some() || args.get("a").is_some();
        let has_sos = args.get("sos").is_some();
        if has_ba == has_sos {
            let mut e = OpError::new("E.VALIDATION.OPTION_CONFLICT", if has_ba { "同时给出了 b/a 与 sos；滤波器只能用一种形式。" } else { "没有给出滤波器：要 b 和 a，或 sos。" }, "高阶滤波器优先用 sos（数值稳定）；b/a 与 MATLAB filter 相同。").path("/");
            if has_ba {
                e = e.remedy(modify_input("drop_ba", "删除 b、a，只用 sos（更稳定）", "high", true, json!([{"op": "remove", "path": "/b"}, {"op": "remove", "path": "/a"}])));
            }
            return Err(e);
        }
        let (xv, xs) = vec1(&args["x"], "/x", host)?;
        let (x, x_1d): (Vec<Vec<f64>>, bool) = match xs.as_slice() {
            [n] => (vec![xv], *n > 0),
            [k, n] if *k >= 1 => ((0..*k).map(|r| xv[r * n..(r + 1) * n].to_vec()).collect(), false),
            _ => return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 的 shape 是 {xs:?}；要求 [n] 或 [k, n]（每行一个通道）。"), "MATLAB 按列存通道，这里按行：若 x 是 [n, k]，先转置。").path("/x/shape")),
        };
        let form = if has_sos {
            let (sv, ss) = vec1(&args["sos"], "/sos", host)?;
            if ss.len() != 2 || ss[1] != 6 || ss[0] == 0 {
                return Err(OpError::new("E.VALIDATION.SHAPE", format!("sos 的 shape 是 {ss:?}；要求 [L, 6]，每行 [b0 b1 b2 a0 a1 a2]。"), "按 MATLAB/scipy 的 sos 格式给出。").path("/sos/shape"));
            }
            let rows: Vec<[f64; 6]> = (0..ss[0]).map(|r| { let mut a = [0.0; 6]; a.copy_from_slice(&sv[r * 6..r * 6 + 6]); a }).collect();
            if let Some(r) = rows.iter().position(|s| s[3] == 0.0) {
                return Err(OpError::new("E.VALIDATION.ZERO_DENOMINATOR", format!("sos 第 {r} 节的 a0 为 0。"), "每节的 a0 必须非零（通常为 1）。").path(&format!("/sos/data/{}", r * 6 + 3)));
            }
            Form::Sos(rows)
        } else {
            let b = poly(&args["b"], "/b")?;
            let a = poly(&args["a"], "/a")?;
            if a[0] == 0.0 {
                return Err(OpError::new("E.VALIDATION.ZERO_DENOMINATOR", "a 的第一个系数 a0 为 0（同 MATLAB filter 的要求）。", "a0 必须非零；检查系数顺序：这里是 z⁻¹ 的升幂，a0 在最前。").path("/a/0"));
            }
            Form::Ba(b, a)
        };
        let k = x.len();
        let zi = match args.get("zi") {
            None => None,
            Some(v) => {
                let (zv, zs) = vec1(v, "/zi", host)?;
                let per = match &form { Form::Ba(b, a) => b.len().max(a.len()) - 1, Form::Sos(s) => 2 * s.len() };
                let ok_single = zv.len() == per && (zs.len() == 1 || (zs.len() == 2 && matches!(form, Form::Sos(_))));
                let ok_multi = zv.len() == per * k && k > 1 && zs[0] == k;
                if !(ok_single || ok_multi) {
                    return Err(OpError::new("E.VALIDATION.SHAPE", format!("zi 的 shape 是 {zs:?}；要每个通道 {per} 个状态（b/a：max(len(a), len(b)) − 1；sos：[L, 2]），或 [k, …] 每个通道一份。"), "检查初始状态的长度。").path("/zi/shape"));
                }
                Some((0..k).map(|c| if ok_multi { zv[c * per..(c + 1) * per].to_vec() } else { zv.clone() }).collect())
            }
        };
        Ok((In { x, x_1d, form, zi }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let (mut ys, mut zfs) = (Vec::new(), Vec::new());
        let mut worst = 0.0f64;
        let m_order;
        let skip_if_zi = |m: usize| if i.zi.is_some() { m } else { 0 };
        let unstable_poles: Vec<f64>;
        match &i.form {
            Form::Ba(b, a) => {
                m_order = b.len().max(a.len());
                let r = aeoi_core::poly::roots(&aeoi_core::poly::strip(a));
                unstable_poles = r.iter().map(|z| z.norm()).filter(|v| *v >= 1.0).collect();
                for (c, x) in i.x.iter().enumerate() {
                    let (y, zf) = lfilter(b, a, x, i.zi.as_ref().map(|z| &z[c][..]));
                    worst = worst.max(de_residual(b, a, x, &y, skip_if_zi(m_order - 1)));
                    ys.push(y);
                    zfs.push(zf);
                    ctx.check()?;
                }
            }
            Form::Sos(s) => {
                m_order = 3;
                unstable_poles = s.iter().flat_map(|sec| aeoi_core::poly::roots(&aeoi_core::poly::strip(&sec[3..]))).map(|z| z.norm()).filter(|v| *v >= 1.0).collect();
                for (c, x) in i.x.iter().enumerate() {
                    let zi: Option<Vec<[f64; 2]>> = i.zi.as_ref().map(|z| z[c].chunks(2).map(|p| [p[0], p[1]]).collect());
                    let (y, zf, stages) = sosfilt(s, x, zi.as_deref());
                    let mut input = x.clone();
                    for (sec, out) in s.iter().zip(&stages) {
                        worst = worst.max(de_residual(&sec[..3], &sec[3..], &input, out, skip_if_zi(2)));
                        input = out.clone();
                    }
                    ys.push(y);
                    zfs.push(zf.iter().flatten().cloned().collect());
                    ctx.check()?;
                }
            }
        }
        if !unstable_poles.is_empty() {
            warnings.push(Warning::new("W.DOMAIN.UNSTABLE_SYSTEM", format!("滤波器不稳定：{} 个极点在单位圆上或圆外（最大模 {:.6}）。输出可能无界增长。", unstable_poles.len(), unstable_poles.iter().cloned().fold(0.0, f64::max))).path(if matches!(i.form, Form::Sos(_)) { "/sos" } else { "/a" }).details(json!({"max_pole_radius": num(unstable_poles.iter().cloned().fold(0.0, f64::max))})));
        }
        let threshold = 10.0 * m_order as f64 * U;
        let finite = ys.iter().flatten().all(|v| v.is_finite());
        let n = i.x[0].len();
        let k = i.x.len();
        let y_val = if i.x_1d || k == 1 && i.x_1d { json!({"dtype": "f64", "shape": [n], "data": ys[0].iter().map(|v| num(*v)).collect::<Vec<_>>()}) } else { json!({"dtype": "f64", "shape": [k, n], "data": ys.iter().flatten().map(|v| num(*v)).collect::<Vec<_>>()}) };
        let per = zfs[0].len();
        let zf_shape: Vec<usize> = match (&i.form, i.x_1d) {
            (Form::Sos(s), true) => vec![s.len(), 2],
            (Form::Sos(s), false) => vec![k, s.len(), 2],
            (Form::Ba(..), true) => vec![per],
            (Form::Ba(..), false) => vec![k, per],
        };
        let result = json!({
            "y": y_val,
            "zf": {"dtype": "f64", "shape": zf_shape, "data": zfs.iter().flatten().map(|v| num(*v)).collect::<Vec<_>>()},
            "form": if matches!(i.form, Form::Sos(_)) { "sos" } else { "ba" },
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"difference_equation": num(worst), "threshold": num(threshold), "definition": "max_n |Σ a_i·y[n−i] − Σ b_i·x[n−i]| / (Σ|a_i·y[n−i]| + Σ|b_i·x[n−i]|)（a、b 已除以 a0；sos 逐节检查；补偿算术；给了 zi 时前 m − 1 个样本不查）"},
            "condition": {"unstable_pole_count": unstable_poles.len()},
            "error_estimate": {"applicable": false, "note": "每个样本满足差分方程到舍入误差量级；极点接近单位圆时，舍入误差会沿递推放大（约 1/(1 − 最大极点模) 倍）"},
            "checks": {"difference_equation": worst <= threshold, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"structure": if matches!(i.form, Form::Sos(_)) { "cascade of transposed direct form II sections" } else { "transposed direct form II" }}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    fn call(args: Value) -> Value {
        Filter::new().call(&args, &HostCtx::default())
    }
    #[test]
    fn moving_average_and_iir() {
        let x = json!({"dtype": "f64", "shape": [4], "data": [1.0, 2.0, 3.0, 4.0]});
        let e = call(json!({"x": x, "b": [0.5, 0.5], "a": [1.0]}));
        assert_eq!(e["result"]["y"]["data"], json!([0.5, 1.5, 2.5, 3.5]), "{e}");
        let e = call(json!({"x": {"dtype": "f64", "shape": [3], "data": [1.0, 0.0, 0.0]}, "b": [1.0], "a": [1.0, -0.5]}));
        assert_eq!(e["result"]["y"]["data"], json!([1.0, 0.5, 0.25]), "{e}");
        assert_eq!(e["status"], "ok");
    }
    #[test]
    fn unstable_warns_and_conflicts() {
        let e = call(json!({"x": {"dtype": "f64", "shape": [3], "data": [1.0, 0.0, 0.0]}, "b": [1.0], "a": [1.0, -2.0]}));
        assert!(e["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.DOMAIN.UNSTABLE_SYSTEM"), "{e}");
        let e = call(json!({"x": {"dtype": "f64", "shape": [1], "data": [1.0]}, "b": [1.0], "a": [0.0, 1.0]}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.ZERO_DENOMINATOR");
        let e = call(json!({"x": {"dtype": "f64", "shape": [1], "data": [1.0]}}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
    }
}
