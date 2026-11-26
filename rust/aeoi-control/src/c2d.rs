//! control.lti.c2d — continuous → discrete (zoh / foh / tustin with optional prewarp) (spec 10.8).
use crate::{check_states, mimo_err};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::kernels::timeresp::hold;
use aeoi_core::linalg::{cond2, norm1};
use aeoi_core::lti::{eigenvalues, parse_lti, tf2ss_siso, Lti, SsData, C64};
use aeoi_core::sysalg::{convert, lti_json, ss2tf, tf_normalize};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};
use std::f64::consts::PI;

operator_struct!(C2d, "control.lti.c2d");

pub struct C2dInput {
    pub sys: Lti,
    pub t: f64,
    pub method: String,
    pub prewarp: Option<f64>,
    pub out_kind: String,
}

pub struct Disc {
    pub ss: SsData,
    pub squarings: u32,
    pub cond_ima: Option<f64>,
}

/// Discretize one state-space system (formulas in the manifest; scipy cont2discrete state coordinates).
pub fn discretize(s: &SsData, t: f64, method: &str, tw: f64) -> Result<Disc, OpError> {
    let n = s.a.nrows();
    match method {
        "zoh" => {
            let (phi, f1, _, sq) = hold(&s.a, &s.b, t, false);
            Ok(Disc { ss: SsData { a: phi, b: f1, c: s.c.clone(), d: s.d.clone(), ts: t }, squarings: sq, cond_ima: None })
        }
        "foh" => {
            let (m11, m12, m13, sq) = hold(&s.a, &s.b, t, true);
            let m13 = m13.unwrap();
            let bd = &m12 - &m13 + &m11 * &m13;
            let dd = &s.d + &s.c * &m13;
            Ok(Disc { ss: SsData { a: m11, b: bd, c: s.c.clone(), d: dd, ts: t }, squarings: sq, cond_ima: None })
        }
        _ => {
            let alpha = tw / 2.0;
            let ima = DMatrix::<f64>::identity(n, n) - &s.a * alpha;
            let kappa = if n == 0 { 1.0 } else { cond2(&ima) };
            let sing = || {
                OpError::new("E.NUMERICAL.SINGULAR", format!("Tustin 变换奇异：I − (T/2)·A 不可逆（A 有特征值 ≈ 2/T = {:.6e} rad/s）。", 2.0 / tw), "换用 zoh/foh，或改变采样周期 ts 使 2/ts 远离系统极点。").path("/ts")
            };
            if !kappa.is_finite() || kappa > 1e14 {
                return Err(sing());
            }
            let ad = aeoi_core::kernels::nalg::solve(&ima, &(DMatrix::<f64>::identity(n, n) + &s.a * alpha)).ok_or_else(sing)?;
            let bd = aeoi_core::kernels::nalg::solve(&ima, &(&s.b * tw)).ok_or_else(sing)?;
            let cd = aeoi_core::kernels::nalg::solve(&ima.transpose(), &s.c.transpose()).ok_or_else(sing)?.transpose();
            let dd = &s.d + &s.c * &bd * 0.5; // scipy gbt(α=½): D + ½·C·B_d = D + (T_w/2)·C(I−αA)⁻¹B
            Ok(Disc { ss: SsData { a: ad, b: bd, c: cd, d: dd, ts: t }, squarings: 0, cond_ima: Some(kappa) })
        }
    }
}

fn matched_err(a: &[C64], b: &[C64]) -> f64 {
    let mut used = vec![false; b.len()];
    let mut worst: f64 = 0.0;
    for x in a {
        let mut best = (usize::MAX, f64::INFINITY);
        for (j, y) in b.iter().enumerate() {
            if !used[j] && (x - y).norm() < best.1 {
                best = (j, (x - y).norm());
            }
        }
        if best.0 == usize::MAX {
            return f64::INFINITY;
        }
        used[best.0] = true;
        worst = worst.max(best.1 / b[best.0].norm().max(1.0));
    }
    worst
}

impl Operator for C2d {
    type Input = C2dInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(C2dInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        if let Lti::Ss { a, .. } = &sys {
            check_states(&self.meta, a.nrows(), "/system/A")?;
        }
        if sys.ts() != 0.0 {
            return Err(OpError::new("E.DOMAIN.NOT_CONTINUOUS", format!("system 已是离散系统（ts = {}），c2d 只接受连续系统。", sys.ts()), "若要改变采样周期，先 d2c 再 c2d（v0.3 未提供 d2c）；若 ts 填错，改为 0。").path("/system/ts")
                .remedy(modify_input("mark_continuous", "若该系统实为连续系统，把 system.ts 改为 0", "low", true, json!([{"op": "replace", "path": "/system/ts", "value": 0.0}]))));
        }
        let t = args["ts"].as_f64().unwrap();
        let method = args.get("method").and_then(|x| x.as_str()).unwrap_or("zoh").to_string();
        let prewarp = args.get("prewarp_frequency").and_then(|x| x.as_f64());
        if let Some(wp) = prewarp {
            if method != "tustin" {
                return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", format!("prewarp_frequency 只对 tustin 有意义，method = {method}。"), "删除 prewarp_frequency，或把 method 改为 tustin。").path("/prewarp_frequency")
                    .remedy(modify_input("drop_prewarp", "删除 prewarp_frequency", "high", false, json!([{"op": "remove", "path": "/prewarp_frequency"}])))
                    .remedy(modify_input("use_tustin", "method 改为 tustin", "medium", false, json!([{"op": "replace", "path": "/method", "value": "tustin"}]))));
            }
            if wp * t >= PI {
                return Err(OpError::new("E.DOMAIN.ABOVE_NYQUIST", format!("预畸变频率 {wp} rad/s ≥ Nyquist 频率 π/ts = {:.6} rad/s。", PI / t), "选择低于 π/ts 的预畸变频率（通常为关心的穿越/谐振频率）。").path("/prewarp_frequency"));
            }
        }
        let out_kind = match args.get("output_kind").and_then(|x| x.as_str()).unwrap_or("same") {
            "same" => sys.kind().to_string(),
            k => k.to_string(),
        };
        let (ny, nu) = sys.dims();
        if out_kind == "zpk" && ny * nu != 1 {
            return Err(mimo_err("/output_kind", "zpk 输出（v0.3）", ny, nu).remedy(modify_input("tf_out", "输出改为 tf", "high", false, json!([{"op": "replace", "path": "/output_kind", "value": "tf"}]))));
        }
        Ok((C2dInput { sys, t, method, prewarp, out_kind }, w))
    }
    fn execute(&self, inp: C2dInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let t = inp.t;
        let tw = match inp.prewarp {
            Some(wp) => 2.0 * (wp * t / 2.0).tan() / wp,
            None => t,
        };
        let (ny, nu) = inp.sys.dims();
        let ss = inp.sys.to_ss();
        let d = discretize(&ss, t, &inp.method, tw)?;
        ctx.check()?;
        // pole-mapping invariant
        let pc = eigenvalues(&ss.a);
        let mapped: Vec<C64> = pc.iter().map(|l| if inp.method == "tustin" { (C64::new(1.0, 0.0) + l * (tw / 2.0)) / (C64::new(1.0, 0.0) - l * (tw / 2.0)) } else { (l * t).exp() }).collect();
        let pd = eigenvalues(&d.ss.a);
        let pole_map_err = matched_err(&pd, &mapped);
        if inp.method != "tustin" && pc.iter().any(|l| l.im.abs() * t > PI) {
            warnings.push(Warning::new("W.ACCURACY.ALIASING", format!("有极点的虚部超过 Nyquist 频率 π/ts = {:.4e} rad/s，离散化后发生混叠（数学上仍精确，但离散模型无法区分这些频率）。", PI / t)).path("/ts"));
        }
        let mimo_tf = matches!(inp.sys, Lti::Tf { .. }) && ny * nu > 1;
        let out = match inp.out_kind.as_str() {
            "ss" => {
                if mimo_tf {
                    warnings.push(Warning::new("W.DOMAIN.NONMINIMAL_REALIZATION", "MIMO tf 先按逐元素可控标准型块对角实现（非最小）再离散化。").path("/system"));
                }
                Lti::Ss { a: d.ss.a.clone(), b: d.ss.b.clone(), c: d.ss.c.clone(), d: d.ss.d.clone(), ts: t }
            }
            "tf" if mimo_tf => {
                // element-wise: exact for zoh/foh/tustin (all are linear in the system)
                let Lti::Tf { num, den, .. } = &inp.sys else { unreachable!() };
                let mut nn = vec![vec![Vec::new(); nu]; ny];
                let mut dd = vec![vec![Vec::new(); nu]; ny];
                for i in 0..ny {
                    for j in 0..nu {
                        let (a, b, c, dm) = tf2ss_siso(&num[i][j], &den[i][j]);
                        let e = discretize(&SsData { a, b, c, d: dm, ts: 0.0 }, t, &inp.method, tw)?;
                        let (n1, d1) = ss2tf(&e.ss);
                        nn[i][j] = n1[0][0].clone();
                        dd[i][j] = d1;
                    }
                }
                Lti::Tf { num: nn, den: dd, ts: t, siso: false }
            }
            k => {
                let s = Lti::Ss { a: d.ss.a.clone(), b: d.ss.b.clone(), c: d.ss.c.clone(), d: d.ss.d.clone(), ts: t };
                convert(&s, k)
                    .map_err(|_| OpError::new("E.DOMAIN.DEGENERATE_SYSTEM", "离散化后的系统传递函数恒为零或结构退化，无法写成 zpk。", "改用 output_kind = tf 或 ss。").path("/output_kind"))?
                    .0
            }
        };
        let mut out = out;
        tf_normalize(&mut out);
        let result = json!({"system": lti_json(&out), "method": inp.method, "effective_ts_for_formula": tw});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"pole_mapping_max_rel": num(pole_map_err), "definition": if inp.method == "tustin" { "eig(A_d) 与 (1+λT_w/2)/(1−λT_w/2)（λ ∈ eig(A)）逐个最近匹配的最大相对差（以 max(1,|·|) 归一）" } else { "eig(A_d) 与 exp(λ·ts)（λ ∈ eig(A)）逐个最近匹配的最大相对差（以 max(1,|·|) 归一）" }},
            "condition": {"norm1_A_ts": norm1(&ss.a) * t, "expm_squarings": d.squarings, "cond_I_minus_alpha_A": d.cond_ima.map(num).unwrap_or(Value::Null),
                          "realization": if matches!(inp.sys, Lti::Ss { .. }) { "given ss" } else { "controllable canonical form (scipy tf2ss convention)" }},
            "error_estimate": {"basis": "expm（Padé-13 + scaling & squaring）后向误差 ≤ u（Higham 2005）；tustin 的误差 ≈ u·cond(I−αA)。极点映射残差为后验旁证"},
            "checks": {"pole_mapping_consistent": pole_map_err <= 1e-8},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
