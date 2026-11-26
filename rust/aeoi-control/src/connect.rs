//! control.lti.series / parallel / feedback — LTI interconnection (spec 3.12, 10.13–10.15).
//!
//! SISO tf and zpk use polynomial arithmetic (no cancellation). Anything with a state-space
//! operand, a MIMO system, or `output_kind: "ss"` uses the block formulas below, which are the
//! same identities python-control's `series` / `parallel` / `feedback` realize. Clean-room: the
//! formulas are the interconnection equations in spec I-5, not a port of another library.
use crate::{check_states, U};
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::jcs::hash_value;
use aeoi_core::lti::{parse_lti_allow_improper, Lti, SsData, C64};
use aeoi_core::poly::{degree, polyadd_scaled, polymul, strip};
use aeoi_core::sysalg::{eval_tf, lti_json, ss2tf};
use aeoi_core::{ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

const MAX_DEGREE: isize = 64;

pub struct Series { meta: OperatorMeta }
pub struct Parallel { meta: OperatorMeta }
pub struct Feedback { meta: OperatorMeta }

impl Series { pub fn new() -> Self { Series { meta: operator_meta!("control.lti.series") } } }
impl Parallel { pub fn new() -> Self { Parallel { meta: operator_meta!("control.lti.parallel") } } }
impl Feedback { pub fn new() -> Self { Feedback { meta: operator_meta!("control.lti.feedback") } } }
impl Default for Series { fn default() -> Self { Self::new() } }
impl Default for Parallel { fn default() -> Self { Self::new() } }
impl Default for Feedback { fn default() -> Self { Self::new() } }

#[derive(Clone, Copy)]
enum OpKind { Series, Parallel, Feedback }

pub struct In {
    kind: OpKind,
    /// sys1 (positional) / controller (named series) / plant (named feedback)
    a: Lti,
    /// sys2 (positional) / plant (named series) / sensor or implicit unity (named feedback)
    b: Lti,
    /// Named feedback with an explicit controller: forward path = a·pre (controller output → plant).
    pre: Option<Lti>,
    sign: f64,
    output_kind: String,
    lineage: Value,
    hashes: Vec<(&'static str, String)>,
    improper_operand: bool,
    implicit_unity: bool,
    controller_implicit: Option<bool>,
    /// spec K-3: open_loop | closed_loop | unspecified (positional call, role is the caller's)
    loop_role: &'static str,
    topology: Option<&'static str>,
}

const NAMED: [&str; 3] = ["plant", "controller", "sensor"];
const POSITIONAL: [&str; 2] = ["sys1", "sys2"];

fn present(args: &Value, k: &str) -> bool {
    args.get(k).is_some_and(|v| !v.is_null())
}

/// spec F-8 / K-1: named roles and positional operands are mutually exclusive. Each patch removes
/// only fields that are present, so it applies cleanly to the original arguments.
fn role_conflict(named: &[&str], pos: &[&str], kind: OpKind) -> OpError {
    let rm = |ks: &[&str]| Value::Array(ks.iter().map(|k| json!({"op": "remove", "path": format!("/{k}")})).collect());
    let what = if matches!(kind, OpKind::Series) { "controller、plant" } else { "plant、controller、sensor" };
    OpError::new(
        "E.VALIDATION.OPTION_CONFLICT",
        format!("具名角色（{}）与位置参数（{}）不能混用：用一种写法描述同一个回路。", named.join("、"), pos.join("、")),
        format!("推荐只保留具名角色（{what}），它们决定了谁在前向通路、谁在反馈通路；或者只保留 sys1/sys2。"),
    )
    .path(&format!("/{}", pos[0]))
    .detail("conflicting_fields", json!(named.iter().chain(pos.iter()).collect::<Vec<_>>()))
    .detail("groups", json!({"named": named, "positional": pos}))
    .remedy(modify_input("keep_named", "去掉 sys1/sys2，只用具名角色", "medium", false, rm(pos)))
    .remedy(modify_input("keep_positional", "去掉具名角色，只用 sys1/sys2", "medium", false, rm(named)))
    .remedy(ask_user("roles", "向用户确认哪个系统是对象、哪个是控制器"))
}

fn missing(field: &str, msg: &str) -> OpError {
    OpError::new("E.VALIDATION.SCHEMA", msg.to_string(), "按工具描述补上该字段（系统来自题目或上一个工具的 result.system），不要编造系统。")
        .path(&format!("/{field}"))
        .detail("violations", json!([{"path": "", "keyword": "required", "message": format!("'{field}' is a required property")}]))
        .remedy(ask_user(field, &format!("向用户索取 {field}")))
}

fn shape_err(path: &str, msg: &str) -> OpError {
    OpError::new("E.VALIDATION.SHAPE", msg.to_string(), "串联要求 sys1 的输出数 = sys2 的输入数；并联要求两者输入数、输出数都相同；反馈要求 sys2 的输入数 = sys1 的输出数且 sys2 的输出数 = sys1 的输入数。").path(path)
}

fn parse_sys(v: &Value, path: &str, host: &HostCtx, warnings: &mut Vec<Warning>, max_dense: usize) -> Result<Lti, OpError> {
    parse_lti_allow_improper(v, path, host, warnings, max_dense)
}

fn improper_lti(s: &Lti) -> bool {
    match s {
        Lti::Tf { num, den, .. } => num.iter().zip(den).any(|(nr, dr)| nr.iter().zip(dr).any(|(n, d)| degree(n) > degree(d))),
        Lti::Zpk { z, p, .. } => z.len() > p.len(),
        Lti::Ss { .. } => false,
    }
}

fn siso_poly(s: &Lti) -> Option<(Vec<f64>, Vec<f64>)> {
    match s {
        Lti::Tf { num, den, .. } if num.len() == 1 && num[0].len() == 1 => Some((num[0][0].clone(), den[0][0].clone())),
        Lti::Zpk { z, p, k, .. } => {
            let mut num: Vec<f64> = aeoi_core::lti::poly_from_roots(z).iter().map(|c| c.re * k).collect();
            if num.is_empty() {
                num = vec![*k];
            }
            let den: Vec<f64> = aeoi_core::lti::poly_from_roots(p).iter().map(|c| c.re).collect();
            Some((strip(&num), strip(&if den.is_empty() { vec![1.0] } else { den })))
        }
        _ => None,
    }
}

fn tf_siso(num: Vec<f64>, den: Vec<f64>, ts: f64) -> Lti {
    Lti::Tf { num: vec![vec![strip(&num)]], den: vec![vec![strip(&den)]], ts, siso: true }
}

fn guard(p: &[f64], what: &str) -> Result<(), OpError> {
    if degree(p) > MAX_DEGREE {
        return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{what} 的多项式阶次 {} 超过上限 {MAX_DEGREE}。", degree(p)), "降低阶次，或改用 output_kind=ss 的块实现。").path("/sys1"));
    }
    Ok(())
}

fn identity(m: usize, ts: f64) -> Lti {
    Lti::Ss { a: DMatrix::zeros(0, 0), b: DMatrix::zeros(0, m), c: DMatrix::zeros(m, 0), d: DMatrix::identity(m, m), ts }
}

fn load(kind: OpKind, meta: &OperatorMeta, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
    let named: Vec<&str> = NAMED.iter().copied().filter(|k| present(args, k)).collect();
    let pos: Vec<&str> = POSITIONAL.iter().copied().filter(|k| present(args, k)).collect();
    if !named.is_empty() && !pos.is_empty() {
        return Err(role_conflict(&named, &pos, kind));
    }
    if !named.is_empty() {
        return load_named(kind, meta, args, host);
    }
    if !present(args, "sys1") {
        return Err(match kind {
            OpKind::Feedback => missing("plant", "缺少系统：用具名角色给出 plant（可选 controller、sensor），或用位置参数给出 sys1。"),
            OpKind::Series => missing("controller", "缺少系统：用具名角色给出 controller 和 plant，或用位置参数给出 sys1 和 sys2。"),
            OpKind::Parallel => missing("sys1", "缺少 sys1。"),
        });
    }
    if !matches!(kind, OpKind::Feedback) && !present(args, "sys2") {
        return Err(missing("sys2", "缺少 sys2。"));
    }
    let mut warnings = Vec::new();
    let max_d = meta.max_dense("/sys1/A");
    let a = parse_sys(&args["sys1"], "/sys1", host, &mut warnings, max_d)?;
    let sign = args.get("sign").and_then(|x| x.as_f64()).unwrap_or(-1.0);
    let (b, implicit_unity) = if matches!(kind, OpKind::Feedback) && !present(args, "sys2") {
        (unity_for(&a, "/sys2", "省略 sys2 表示单位反馈，要求 sys1 是方的")?, true)
    } else {
        (parse_sys(&args["sys2"], "/sys2", host, &mut warnings, max_d)?, false)
    };
    same_ts(&[("sys1", &a), ("sys2", &b)])?;
    let (ny1, nu1) = a.dims();
    let (ny2, nu2) = b.dims();
    let dim_ok = match kind {
        OpKind::Series => ny1 == nu2,
        OpKind::Parallel => ny1 == ny2 && nu1 == nu2,
        OpKind::Feedback => nu2 == ny1 && ny2 == nu1,
    };
    if !dim_ok {
        return Err(shape_err("/sys2", &format!("维度不匹配：sys1 是 {ny1}×{nu1}，sys2 是 {ny2}×{nu2}。")));
    }
    check_states(meta, nstates(&a).max(nstates(&b)), "/sys1")?;
    let output_kind = args.get("output_kind").and_then(|x| x.as_str()).unwrap_or("same").to_string();
    let lineage = args.get("lineage").cloned().unwrap_or(Value::Null);
    let h2 = if implicit_unity { hash_value(&lti_json(&b)) } else { hash_value(&args["sys2"]) };
    let hashes = vec![("sys1", hash_value(&args["sys1"])), ("sys2", h2)];
    let improper_operand = improper_lti(&a) || improper_lti(&b);
    Ok((In { kind, a, b, pre: None, sign, output_kind, lineage, hashes, improper_operand, implicit_unity, controller_implicit: None, loop_role: "unspecified", topology: None }, warnings))
}

fn nstates(s: &Lti) -> usize {
    s.to_ss_checked().map(|x| x.a.nrows()).unwrap_or(0)
}

fn unity_for(a: &Lti, path: &str, why: &str) -> Result<Lti, OpError> {
    let (ny, nu) = a.dims();
    if ny != nu {
        return Err(shape_err(path, &format!("{why}，得到 {ny}×{nu}。")));
    }
    // SISO unity is the tf 1, so the polynomial path stays available (spec I-5).
    Ok(if ny == 1 { tf_siso(vec![1.0], vec![1.0], a.ts()) } else { identity(ny, a.ts()) })
}

fn same_ts(sys: &[(&str, &Lti)]) -> Result<(), OpError> {
    let (n0, s0) = sys[0];
    for (n, s) in &sys[1..] {
        if s.ts() != s0.ts() {
            return Err(OpError::new(
                "E.DOMAIN.SAMPLE_TIME_MISMATCH",
                format!("两个系统的采样周期不同：{n0}.ts = {}，{n}.ts = {}。连续（0）与离散、或两个不同的采样周期不能互联。", s0.ts(), s.ts()),
                "离散化到同一个 ts（control.lti.c2d，采样周期必须由用户给出），或分别处理。",
            )
            .path(&format!("/{n}/ts"))
            .detail("ts", json!([s0.ts(), s.ts()]))
            .remedy(ask_user("ts", "向用户确认两个系统是否应该在同一采样周期下互联")));
        }
    }
    Ok(())
}

/// spec K-1/K-2: named roles. series(controller, plant) = plant·controller = open loop L (unity sensor);
/// feedback(plant, controller?, sensor?) = closed loop T with the controller in the forward path.
fn load_named(kind: OpKind, meta: &OperatorMeta, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
    let mut warnings = Vec::new();
    let max_d = meta.max_dense("/sys1/A");
    let output_kind = args.get("output_kind").and_then(|x| x.as_str()).unwrap_or("same").to_string();
    let lineage = args.get("lineage").cloned().unwrap_or(Value::Null);
    match kind {
        OpKind::Series => {
            for k in ["controller", "plant"] {
                if !present(args, k) {
                    return Err(missing(k, &format!("具名串联需要同时给出 controller 和 plant，缺少 {k}。结果是开环 L = plant·controller。")));
                }
            }
            let c = parse_sys(&args["controller"], "/controller", host, &mut warnings, max_d)?;
            let g = parse_sys(&args["plant"], "/plant", host, &mut warnings, max_d)?;
            same_ts(&[("controller", &c), ("plant", &g)])?;
            let ((nyc, nuc), (nyg, nug)) = (c.dims(), g.dims());
            if nyc != nug {
                return Err(shape_err("/plant", &format!("controller 的输出数（{nyc}）必须等于 plant 的输入数（{nug}）；controller 是 {nyc}×{nuc}，plant 是 {nyg}×{nug}。")));
            }
            check_states(meta, nstates(&c).max(nstates(&g)), "/plant")?;
            let hashes = vec![("controller", hash_value(&args["controller"])), ("plant", hash_value(&args["plant"]))];
            let improper_operand = improper_lti(&c) || improper_lti(&g);
            Ok((In { kind, a: c, b: g, pre: None, sign: 1.0, output_kind, lineage, hashes, improper_operand, implicit_unity: false, controller_implicit: Some(false),
                     loop_role: "open_loop", topology: Some("u → controller → plant → y；L = plant·controller（单位反馈时的开环）") }, warnings))
        }
        OpKind::Feedback => {
            if !present(args, "plant") {
                return Err(missing("plant", "具名反馈必须给出 plant（controller、sensor 可省略）。"));
            }
            let sign = args.get("sign").and_then(|x| x.as_f64()).unwrap_or(-1.0);
            let g = parse_sys(&args["plant"], "/plant", host, &mut warnings, max_d)?;
            let c = if present(args, "controller") { Some(parse_sys(&args["controller"], "/controller", host, &mut warnings, max_d)?) } else { None };
            let (nyg, nug) = g.dims();
            if let Some(c) = &c {
                same_ts(&[("plant", &g), ("controller", c)])?;
                let (nyc, nuc) = c.dims();
                if nyc != nug {
                    return Err(shape_err("/controller", &format!("controller 的输出数（{nyc}）必须等于 plant 的输入数（{nug}）；controller 是 {nyc}×{nuc}，plant 是 {nyg}×{nug}。")));
                }
            }
            let nu_fwd = c.as_ref().map(|c| c.dims().1).unwrap_or(nug);
            let (h, implicit_unity) = if present(args, "sensor") {
                let h = parse_sys(&args["sensor"], "/sensor", host, &mut warnings, max_d)?;
                same_ts(&[("plant", &g), ("sensor", &h)])?;
                let (nyh, nuh) = h.dims();
                if nuh != nyg || nyh != nu_fwd {
                    return Err(shape_err("/sensor", &format!("sensor 的输入数必须等于 plant 的输出数（{nyg}），输出数必须等于前向通路的输入数（{nu_fwd}）；sensor 是 {nyh}×{nuh}。")));
                }
                (h, false)
            } else {
                if nyg != nu_fwd {
                    return Err(shape_err("/sensor", &format!("省略 sensor 表示单位反馈，要求前向通路是方的，得到 {nyg}×{nu_fwd}。")));
                }
                (if nyg == 1 { tf_siso(vec![1.0], vec![1.0], g.ts()) } else { identity(nyg, g.ts()) }, true)
            };
            let n_all = nstates(&g) + c.as_ref().map(nstates).unwrap_or(0) + nstates(&h);
            check_states(meta, n_all, "/plant")?;
            let unity_c = tf_siso(vec![1.0], vec![1.0], g.ts());
            let hc = if c.is_some() { hash_value(&args["controller"]) } else { hash_value(&lti_json(&unity_c)) };
            let hh = if implicit_unity { hash_value(&lti_json(&h)) } else { hash_value(&args["sensor"]) };
            let hashes = vec![("plant", hash_value(&args["plant"])), ("controller", hc), ("sensor", hh)];
            let improper_operand = improper_lti(&g) || improper_lti(&h) || c.as_ref().is_some_and(improper_lti);
            let controller_implicit = Some(c.is_none());
            Ok((In { kind, a: g, b: h, pre: c, sign, output_kind, lineage, hashes, improper_operand, implicit_unity, controller_implicit,
                     loop_role: "closed_loop", topology: Some("r → e = r + sign·sensor·y → controller → plant → y；T = plant·controller/(1 − sign·sensor·plant·controller)，sign 默认 −1") }, warnings))
        }
        OpKind::Parallel => Err(missing("sys1", "并联只有位置参数 sys1、sys2。")),
    }
}

/// State dimension when the system has a proper realization; None if improper (no ss).
trait SsChecked {
    fn to_ss_checked(&self) -> Option<SsData>;
}
impl SsChecked for Lti {
    fn to_ss_checked(&self) -> Option<SsData> {
        if improper_lti(self) {
            None
        } else {
            Some(self.to_ss())
        }
    }
}

fn place(dst: &mut DMatrix<f64>, row: usize, col: usize, src: &DMatrix<f64>) {
    if src.nrows() == 0 || src.ncols() == 0 {
        return;
    }
    dst.view_mut((row, col), (src.nrows(), src.ncols())).copy_from(src);
}

fn series_ss(s1: &SsData, s2: &SsData) -> SsData {
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let (nu, ny) = (s1.b.ncols(), s2.c.nrows());
    let mut a = DMatrix::zeros(n1 + n2, n1 + n2);
    let mut b = DMatrix::zeros(n1 + n2, nu);
    let mut c = DMatrix::zeros(ny, n1 + n2);
    place(&mut a, 0, 0, &s1.a);
    place(&mut b, 0, 0, &s1.b);
    place(&mut a, n1, 0, &(&s2.b * &s1.c));
    place(&mut a, n1, n1, &s2.a);
    place(&mut b, n1, 0, &(&s2.b * &s1.d));
    place(&mut c, 0, 0, &(&s2.d * &s1.c));
    place(&mut c, 0, n1, &s2.c);
    SsData { a, b, c, d: &s2.d * &s1.d, ts: s1.ts }
}

fn parallel_ss(s1: &SsData, s2: &SsData) -> SsData {
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let (nu, ny) = (s1.b.ncols(), s1.c.nrows());
    let mut a = DMatrix::zeros(n1 + n2, n1 + n2);
    let mut b = DMatrix::zeros(n1 + n2, nu);
    let mut c = DMatrix::zeros(ny, n1 + n2);
    place(&mut a, 0, 0, &s1.a);
    place(&mut b, 0, 0, &s1.b);
    place(&mut c, 0, 0, &s1.c);
    place(&mut a, n1, n1, &s2.a);
    place(&mut b, n1, 0, &s2.b);
    place(&mut c, 0, n1, &s2.c);
    SsData { a, b, c, d: &s1.d + &s2.d, ts: s1.ts }
}

fn algebraic_loop(rcond: f64, n: usize) -> OpError {
    let thr = n as f64 * U * 1.0e4;
    OpError::new(
        "E.DOMAIN.ALGEBRAIC_LOOP",
        format!("反馈代数环不可逆：E = I − sign·D₂·D₁ 的 rcond = {rcond:.3e}，阈值 {thr:.3e}（与 ‖E‖ 无关）。两个静态增益在正反馈下乘积为 1 时闭环没有定义。"),
        "确认 sign（默认 −1 为负反馈）；或让至少一个直通项为 0。",
    )
    .path("/sys2")
    .detail("rcond", json!(rcond))
    .detail("threshold", json!(thr))
    .remedy(modify_input("negative_feedback", "若本意是负反馈，把 sign 改为 -1", "low", true, json!([{"op": "add", "path": "/sign", "value": -1}])))
    .remedy(ask_user("loop", "向用户确认反馈符号和直通项"))
}

fn feedback_ss(s1: &SsData, s2: &SsData, sign: f64) -> Result<(SsData, f64), OpError> {
    let nu = s1.b.ncols();
    let e = DMatrix::<f64>::identity(nu, nu) - (&s2.d * &s1.d) * sign;
    let (rcond, einv) = if nu == 0 {
        (1.0, DMatrix::zeros(0, 0))
    } else {
        let sv = aeoi_core::kernels::nalg::singular_values(&e);
        let smax = sv.iter().cloned().fold(0.0, f64::max);
        let smin = sv.iter().cloned().fold(f64::INFINITY, f64::min);
        let rcond = if smax == 0.0 { 0.0 } else { smin / smax };
        let thr = nu as f64 * U * 1.0e4;
        if !rcond.is_finite() || rcond <= thr {
            return Err(algebraic_loop(rcond, nu));
        }
        let inv = aeoi_core::kernels::nalg::inverse(&e).ok_or_else(|| algebraic_loop(rcond, nu))?;
        (rcond, inv)
    };
    let (n1, n2) = (s1.a.nrows(), s2.a.nrows());
    let ny = s1.c.nrows();
    // u1 = E^{-1} (u + sign D2 C1 x1 + sign C2 x2)
    let s_d2_c1 = (&s2.d * &s1.c) * sign;
    let s_c2 = &s2.c * sign;
    let b1_e = &s1.b * &einv;
    let b2_d1_e = &(&s2.b * &s1.d) * &einv;
    let d1_e = &s1.d * &einv;
    let a11 = &s1.a + &b1_e * &s_d2_c1;
    let a12 = &b1_e * &s_c2;
    let a21 = &s2.b * &s1.c + &b2_d1_e * &s_d2_c1;
    let a22 = &s2.a + &b2_d1_e * &s_c2;
    let c1 = &s1.c + &d1_e * &s_d2_c1;
    let c2 = &d1_e * &s_c2;
    let mut a = DMatrix::zeros(n1 + n2, n1 + n2);
    let mut b = DMatrix::zeros(n1 + n2, nu);
    let mut c = DMatrix::zeros(ny, n1 + n2);
    place(&mut a, 0, 0, &a11);
    place(&mut a, 0, n1, &a12);
    place(&mut a, n1, 0, &a21);
    place(&mut a, n1, n1, &a22);
    place(&mut b, 0, 0, &b1_e);
    place(&mut b, n1, 0, &b2_d1_e);
    place(&mut c, 0, 0, &c1);
    place(&mut c, 0, n1, &c2);
    Ok((SsData { a, b, c, d: d1_e.clone(), ts: s1.ts }, rcond))
}

fn poly_bin(kind: OpKind, a: &(Vec<f64>, Vec<f64>), b: &(Vec<f64>, Vec<f64>), sign: f64) -> Result<(Vec<f64>, Vec<f64>), OpError> {
    let (n1, d1) = a;
    let (n2, d2) = b;
    let (num, den) = match kind {
        OpKind::Series => (strip(&polymul(n2, n1)), strip(&polymul(d2, d1))),
        OpKind::Parallel => (strip(&polyadd_scaled(&polymul(n1, d2), &polymul(n2, d1), 1.0)), strip(&polymul(d1, d2))),
        OpKind::Feedback => {
            let num = strip(&polymul(n1, d2));
            let den = strip(&polyadd_scaled(&polymul(d1, d2), &polymul(n1, n2), -sign));
            (num, den)
        }
    };
    guard(&num, "分子")?;
    guard(&den, "分母")?;
    if den.len() == 1 && den[0] == 0.0 {
        return Err(algebraic_loop(0.0, 1));
    }
    Ok((num, den))
}

fn ss_to_lti(ss: SsData, as_tf: bool, mimo_warn: &mut Vec<Warning>) -> Result<Lti, OpError> {
    if !as_tf {
        return Ok(Lti::Ss { a: ss.a, b: ss.b, c: ss.c, d: ss.d, ts: ss.ts });
    }
    let (ny, nu) = (ss.c.nrows(), ss.b.ncols());
    if ss.a.nrows() == 0 {
        if ny == 1 && nu == 1 {
            return Ok(tf_siso(vec![ss.d[(0, 0)]], vec![1.0], ss.ts));
        }
        // static MIMO gain, one polynomial per entry, shared denominator 1
        let num = (0..ny).map(|i| (0..nu).map(|j| vec![ss.d[(i, j)]]).collect()).collect();
        let den = vec![vec![vec![1.0]; nu]; ny];
        return Ok(Lti::Tf { num, den, ts: ss.ts, siso: false });
    }
    let (num, den) = ss2tf(&ss);
    if ny > 1 || nu > 1 {
        mimo_warn.push(Warning::new("W.DOMAIN.NONMINIMAL_REALIZATION", "MIMO 的 tf 结果用各通道共享的特征多项式作分母，未做零极点对消，一般不是最小实现。").path("/output_kind"));
    }
    let dens = vec![vec![den; nu]; ny];
    Ok(Lti::Tf { num, den: dens, ts: ss.ts, siso: ny == 1 && nu == 1 })
}

fn want_poly(kind_a: &str, kind_b: &str, output: &str, mimo: bool) -> bool {
    if output == "ss" || mimo {
        return false;
    }
    if output == "tf" {
        return true; // SISO: polynomial when both are tf/zpk; ss operand falls through below
    }
    kind_a != "ss" && kind_b != "ss"
}

fn identity_holds(kind: OpKind, out: &Lti, a: &Lti, b: &Lti, sign: f64) -> (bool, usize, f64) {
    let mut worst: f64 = 0.0;
    let mut n = 0;
    for k in 0..6 {
        let s = if out.ts() > 0.0 { C64::from_polar(1.0, 0.25 + 0.35 * k as f64) } else { C64::new(0.2 * k as f64, 0.35 + 0.25 * k as f64) };
        let (Some(gy), Some(g1), Some(g2)) = (eval_tf(out, s), eval_tf(a, s), eval_tf(b, s)) else { continue };
        let (got, expect): (DMatrix<C64>, DMatrix<C64>) = match kind {
            OpKind::Series => (gy, &g2 * &g1),
            OpKind::Parallel => (gy, &g1 + &g2),
            OpKind::Feedback => {
                let lhs = &gy - &g1 * &g2 * &gy * C64::new(sign, 0.0);
                (lhs, g1)
            }
        };
        let gn: f64 = got.norm();
        let en: f64 = expect.norm();
        let dn: f64 = (&got - &expect).norm();
        let scale = gn.max(en).max(1e-12_f64);
        worst = worst.max(dn / scale);
        n += 1;
    }
    (n > 0 && worst <= 1e-5, n, worst)
}

fn realization_name(kind: OpKind, poly: bool) -> &'static str {
    match (kind, poly) {
        (OpKind::Series, true) => "polynomial_product",
        (OpKind::Parallel, true) => "polynomial_sum",
        (OpKind::Feedback, true) => "polynomial_feedback",
        (OpKind::Series, false) => "series_block",
        (OpKind::Parallel, false) => "parallel_block",
        (OpKind::Feedback, false) => "feedback_block",
    }
}

/// One interconnection step: returns (system, algebraic-loop rcond, polynomial path used).
fn combine(kind: OpKind, a: &Lti, b: &Lti, sign: f64, output_kind: &str, warnings: &mut Vec<Warning>) -> Result<(Lti, Option<f64>, bool), OpError> {
    let mimo = {
        let (ny, nu) = a.dims();
        let (ny2, nu2) = b.dims();
        ny > 1 || nu > 1 || ny2 > 1 || nu2 > 1
    };
    let poly = want_poly(a.kind(), b.kind(), output_kind, mimo);
    if poly {
        match (siso_poly(a), siso_poly(b)) {
            (Some(pa), Some(pb)) => {
                let (num, den) = poly_bin(kind, &pa, &pb, sign)?;
                Ok((tf_siso(num, den, a.ts()), None, true))
            }
            _ => {
                // SISO ss mixed with tf and output tf: block then ss2tf
                let s1 = a.to_ss_checked().ok_or_else(|| improper_ss("/sys1"))?;
                let s2 = b.to_ss_checked().ok_or_else(|| improper_ss("/sys2"))?;
                check_states_pair(&s1, &s2)?;
                let (ss, rc) = blocks(kind, &s1, &s2, sign)?;
                Ok((ss_to_lti(ss, true, warnings)?, rc, false))
            }
        }
    } else {
        let s1 = a.to_ss_checked().ok_or_else(|| improper_ss("/sys1"))?;
        let s2 = b.to_ss_checked().ok_or_else(|| improper_ss("/sys2"))?;
        let (ss, rc) = blocks(kind, &s1, &s2, sign)?;
        Ok((ss_to_lti(ss, output_kind == "tf", warnings)?, rc, false))
    }
}

/// Public for control.analysis.loopsens (spec K-4: the one-shot analysis is these same steps).
pub(crate) fn series_lti(first: &Lti, second: &Lti, warnings: &mut Vec<Warning>) -> Result<Lti, OpError> {
    combine(OpKind::Series, first, second, 1.0, "same", warnings).map(|r| r.0)
}
pub(crate) fn feedback_lti(fwd: &Lti, back: &Lti, warnings: &mut Vec<Warning>) -> Result<(Lti, Option<f64>), OpError> {
    combine(OpKind::Feedback, fwd, back, -1.0, "same", warnings).map(|r| (r.0, r.1))
}
pub(crate) fn unity_siso(ts: f64) -> Lti {
    tf_siso(vec![1.0], vec![1.0], ts)
}
pub(crate) fn feedback_identity(t: &Lti, fwd: &Lti, back: &Lti) -> (bool, usize, f64) {
    identity_holds(OpKind::Feedback, t, fwd, back, -1.0)
}
pub(crate) fn series_identity(l: &Lti, first: &Lti, second: &Lti) -> (bool, usize, f64) {
    identity_holds(OpKind::Series, l, first, second, 1.0)
}
pub(crate) fn is_improper(s: &Lti) -> bool {
    improper_lti(s)
}
pub(crate) fn all_finite(s: &Lti) -> bool {
    finite_lti(s)
}

fn execute_connect(inp: In, pre_warn: Vec<Warning>) -> Result<Output, OpError> {
    let mut warnings = pre_warn;
    if inp.improper_operand {
        let p = if inp.loop_role == "unspecified" { "/sys1" } else if inp.pre.is_some() { "/controller" } else { "/plant" };
        warnings.push(Warning::new("W.DOMAIN.IMPROPER_OPERAND", "有操作数不是 proper（分子阶次高于分母）。多项式结果仍会给出；需要状态空间实现时会报 E.DOMAIN.IMPROPER_SYSTEM。分析算子（step、bode、margin）不接受 improper 系统。").path(p));
    }
    // Named feedback with a controller: forward path first (controller output → plant input).
    let (g1, fwd_check) = match &inp.pre {
        Some(c) => {
            let (fwd, _, _) = combine(OpKind::Series, c, &inp.a, 1.0, &inp.output_kind, &mut warnings)?;
            let chk = identity_holds(OpKind::Series, &fwd, c, &inp.a, 1.0);
            (fwd, Some(chk))
        }
        None => (inp.a.clone(), None),
    };
    let (sys, rcond, poly_used) = combine(inp.kind, &g1, &inp.b, inp.sign, &inp.output_kind, &mut warnings)?;
    if improper_lti(&sys) {
        warnings.push(Warning::new("W.DOMAIN.IMPROPER_RESULT", "互联结果不是 proper。step / bode / margin / c2d 会拒绝它；先闭合回路，或给理想微分加滤波极点。").path("/output_kind"));
    }
    if let Some(rc) = rcond {
        if rc < 1e-8 {
            let p = if inp.loop_role == "unspecified" { "/sys2" } else { "/sensor" };
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("代数环矩阵 E 的 rcond = {rc:.3e} < 1e-8，闭环对直通项的舍入敏感。")).path(p).details(json!({"rcond": rc})));
        }
    }
    let (mut ok, mut npts, mut worst) = identity_holds(inp.kind, &sys, &g1, &inp.b, inp.sign);
    if let Some((ok2, n2, w2)) = fwd_check {
        ok = ok && ok2;
        npts += n2;
        worst = worst.max(w2);
    }
    let name = match inp.kind {
        OpKind::Series => "series",
        OpKind::Parallel => "parallel",
        OpKind::Feedback => "feedback",
    };
    let operands: Vec<Value> = inp.hashes.iter().map(|(role, h)| json!({"role": role, "sha256": h})).collect();
    let mut composition = json!({
        "operation": name,
        "operands": operands,
        "sign": if matches!(inp.kind, OpKind::Feedback) { json!(inp.sign) } else { Value::Null },
        "implicit_unity": inp.implicit_unity,
        "lineage": inp.lineage,
        "lineage_checked": false,
    });
    // spec K-3: positional calls keep their v0.3.5 bytes (no loop_role key); named calls state the role.
    if inp.loop_role != "unspecified" {
        composition["loop_role"] = json!(inp.loop_role);
        composition["topology"] = json!(inp.topology);
        if let Some(ci) = inp.controller_implicit {
            composition["controller_implicit"] = json!(ci);
        }
    }
    let result = json!({
        "system": lti_json(&sys),
        "realization": if poly_used { realization_name(inp.kind, true) } else if sys.kind()=="tf" { "ss2tf_characteristic_polynomial" } else { realization_name(inp.kind, false) },
        "minimal": Value::Null,
        "proper": !improper_lti(&sys),
        "composition": composition,
    });
    let trust = json!({
        "convergence": {"applicable": false},
        "residual": {"identity_rel": aeoi_core::num(worst), "definition": "在若干测试点上 ‖互联恒等式残差‖ / max(‖结果‖, ‖期望‖)"},
        "condition": {"identity_points": npts, "algebraic_loop_rcond": rcond.map(aeoi_core::num).unwrap_or(Value::Null)},
        "error_estimate": {"identity_rel": aeoi_core::num(worst), "basis": "多项式算术或块公式在测试点上的相对残差；不是严格上界。系数本身是有限次乘加。"},
        "checks": {"response_consistent": ok, "sample_times_equal": true, "all_finite": finite_lti(&sys)},
        "accuracy_grade": "B",
        "credentials": Value::Null,
        "derived": {"sign": if matches!(inp.kind, OpKind::Feedback) { json!(inp.sign) } else { Value::Null }}
    });
    Ok(Output { result, trust, warnings })
}

fn finite_lti(s: &Lti) -> bool {
    match s {
        Lti::Tf { num, den, .. } => num.iter().flatten().flatten().all(|x| x.is_finite()) && den.iter().flatten().flatten().all(|x| x.is_finite()),
        Lti::Ss { a, b, c, d, .. } => a.iter().chain(b.iter()).chain(c.iter()).chain(d.iter()).all(|x| x.is_finite()),
        Lti::Zpk { z, p, k, .. } => k.is_finite() && z.iter().chain(p).all(|c| c.re.is_finite() && c.im.is_finite()),
    }
}

fn improper_ss(path: &str) -> OpError {
    OpError::new("E.DOMAIN.IMPROPER_SYSTEM", "该输出需要状态空间实现，但操作数或结果不是 proper，不存在普通状态空间实现。", "output_kind 改为 tf（SISO 多项式结果），或给理想微分加一个滤波极点。")
        .path(path)
        .remedy(modify_input("as_tf", "output_kind 改为 tf", "high", false, json!([{"op": "add", "path": "/output_kind", "value": "tf"}])))
        .remedy(ask_user("proper", "向用户确认是否允许理想微分"))
}

fn check_states_pair(s1: &SsData, s2: &SsData) -> Result<(), OpError> {
    let n = s1.a.nrows() + s2.a.nrows();
    if n > 200 {
        return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("互联后的状态维数 {n} 超过上限 200。"), "先降阶再互联。").path("/sys1"));
    }
    Ok(())
}

fn blocks(kind: OpKind, s1: &SsData, s2: &SsData, sign: f64) -> Result<(SsData, Option<f64>), OpError> {
    check_states_pair(s1, s2)?;
    match kind {
        OpKind::Series => Ok((series_ss(s1, s2), None)),
        OpKind::Parallel => Ok((parallel_ss(s1, s2), None)),
        OpKind::Feedback => {
            let (ss, rc) = feedback_ss(s1, s2, sign)?;
            Ok((ss, Some(rc)))
        }
    }
}

macro_rules! connect_op {
    ($ty:ty, $kind:expr) => {
        impl Operator for $ty {
            type Input = In;
            fn meta(&self) -> &OperatorMeta { &self.meta }
            fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
                load($kind, &self.meta, args, host)
            }
            fn estimate(&self, i: &In) -> Option<Value> {
                let n = i.a.to_ss_checked().map(|s| s.a.nrows()).unwrap_or(0) + i.b.to_ss_checked().map(|s| s.a.nrows()).unwrap_or(0);
                Some(json!({"states_after": n}))
            }
            fn execute(&self, input: In, _ctx: &ExecCtx) -> Result<Output, OpError> {
                execute_connect(input, Vec::new())
            }
        }
    };
}

connect_op!(Series, OpKind::Series);
connect_op!(Parallel, OpKind::Parallel);
connect_op!(Feedback, OpKind::Feedback);

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::HostCtx;
    fn run(op: &str, req: Value) -> Value {
        let reg = crate::registry();
        crate::find(&reg, op).unwrap().call(&req, &HostCtx::default())
    }
    fn tf(num: &[f64], den: &[f64]) -> Value {
        json!({"kind":"tf","num":num,"den":den,"ts":0.0,"time_unit":"s"})
    }
    #[test]
    fn series_parallel_feedback_hand() {
        let g1 = tf(&[1.0], &[1.0, 1.0]);
        let g2 = tf(&[2.0], &[1.0, 3.0, 2.0]);
        let s = run("control.lti.series", json!({"sys1": g1, "sys2": g2}));
        assert_eq!(s["status"], "ok", "{}", s["error"]);
        assert_eq!(s["result"]["system"]["num"], json!([2.0]));
        assert_eq!(s["result"]["system"]["den"], json!([1.0, 4.0, 5.0, 2.0]));
        assert_eq!(s["result"]["realization"], "polynomial_product");
        assert_eq!(s["receipt"]["evidence"]["claim"], "verified");
        let p = run("control.lti.parallel", json!({"sys1": g1, "sys2": g2}));
        assert_eq!(p["result"]["system"]["num"], json!([1.0, 5.0, 4.0]));
        assert_eq!(p["result"]["system"]["den"], json!([1.0, 4.0, 5.0, 2.0]));
        let f = run("control.lti.feedback", json!({"sys1": g1, "sys2": g2}));
        assert_eq!(f["result"]["system"]["num"], json!([1.0, 3.0, 2.0]));
        assert_eq!(f["result"]["system"]["den"], json!([1.0, 4.0, 5.0, 4.0]));
        assert_eq!(f["result"]["composition"]["sign"], json!(-1.0));
        let u = run("control.lti.feedback", json!({"sys1": g1}));
        // 1/(s+1) in unity negative feedback = 1/(s+2)
        assert_eq!(u["result"]["system"]["num"], json!([1.0]));
        assert_eq!(u["result"]["system"]["den"], json!([1.0, 2.0]));
    }
    #[test]
    fn k1_named_roles_fix_the_loop_topology() {
        // EngDesign XG_13 live run: feedback(sys1 = G, sys2 = C) put the PID in the feedback path.
        let g = tf(&[1.0], &[1.0, 10.0, 20.0]);
        let c = tf(&[15.0, 40.0, 50.0], &[1.0, 0.0]); // Kp=40, Ki=50, Kd=15 (ideal derivative)
        let t = run("control.lti.feedback", json!({"plant": g, "controller": c}));
        assert_eq!(t["status"], "ok_with_warnings", "{}", t["error"]);
        assert_eq!(t["result"]["system"]["num"], json!([15.0, 40.0, 50.0]));
        assert_eq!(t["result"]["system"]["den"], json!([1.0, 25.0, 60.0, 50.0]));
        assert_eq!(t["result"]["composition"]["loop_role"], "closed_loop");
        assert_eq!(t["result"]["composition"]["controller_implicit"], false);
        assert_eq!(t["receipt"]["evidence"]["claim"], "verified");
        assert_eq!(t["receipt"]["evidence"]["coverage"]["response_consistent"], "test_points");
        let roles: Vec<&str> = t["result"]["composition"]["operands"].as_array().unwrap().iter().map(|o| o["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["plant", "controller", "sensor"]);
        // the v0.3.5 mistake, still expressible positionally, gives G/(1+CG): numerator is not the PID's
        let wrong = run("control.lti.feedback", json!({"sys1": g, "sys2": c}));
        assert_ne!(wrong["result"]["system"]["num"], t["result"]["system"]["num"]);
        assert!(wrong["result"]["composition"].get("loop_role").is_none(), "positional calls keep v0.3.5 bytes");
        // open loop from named series
        let l = run("control.lti.series", json!({"controller": c, "plant": g}));
        assert_eq!(l["result"]["system"]["num"], json!([15.0, 40.0, 50.0]));
        assert_eq!(l["result"]["system"]["den"], json!([1.0, 10.0, 20.0, 0.0]));
        assert_eq!(l["result"]["composition"]["loop_role"], "open_loop");
    }
    #[test]
    fn k1_sensor_is_the_feedback_path() {
        let g = tf(&[1.0], &[1.0, 1.0]);
        let h = tf(&[2.0], &[1.0, 3.0]);
        let t = run("control.lti.feedback", json!({"plant": g, "sensor": h}));
        // (s+3)/((s+1)(s+3)+2)
        assert_eq!(t["result"]["system"]["num"], json!([1.0, 3.0]));
        assert_eq!(t["result"]["system"]["den"], json!([1.0, 4.0, 5.0]));
        assert_eq!(t["result"]["composition"]["controller_implicit"], true);
        assert_eq!(t["result"]["composition"]["implicit_unity"], false);
    }
    #[test]
    fn k1_mixing_roles_and_positional_is_option_conflict() {
        let g = tf(&[1.0], &[1.0, 1.0]);
        let req = json!({"plant": g, "sys2": g});
        let e = run("control.lti.feedback", req.clone());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let rem = e["error"]["remediation"].as_array().unwrap();
        let keep = rem.iter().find(|r| r["id"] == "keep_named").unwrap();
        let mut fixed = req.clone();
        for p in keep["patch"].as_array().unwrap() {
            fixed.as_object_mut().unwrap().remove(p["path"].as_str().unwrap().trim_start_matches('/')).expect("patch removes a present field");
        }
        assert_eq!(run("control.lti.feedback", fixed)["status"], "ok");
        let e = run("control.lti.series", json!({"controller": g}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.SCHEMA");
        assert_eq!(e["error"]["details"]["path"], "/plant");
        assert_eq!(run("control.lti.series", json!({}))["error"]["code"], "E.VALIDATION.SCHEMA");
    }
    #[test]
    fn positive_static_loop_is_singular() {
        let one = tf(&[1.0], &[1.0]);
        let e = run("control.lti.feedback", json!({"sys1": one, "sys2": one, "sign": 1}));
        assert_eq!(e["error"]["code"], "E.DOMAIN.ALGEBRAIC_LOOP");
        assert!(!e["error"]["remediation"].as_array().unwrap().is_empty());
    }
    #[test]
    fn sample_time_and_shape() {
        let a = tf(&[1.0], &[1.0, 1.0]);
        let mut b = a.clone();
        b["ts"] = json!(0.1);
        assert_eq!(run("control.lti.series", json!({"sys1": a, "sys2": b}))["error"]["code"], "E.DOMAIN.SAMPLE_TIME_MISMATCH");
    }
    #[test]
    fn ss_series_matches_block_formula() {
        // numbers from a hand-built pair; identity check is the postcondition
        let ss = |a, b, c, d| json!({"kind":"ss","A":{"dtype":"f64","shape":[2,2],"data":a},"B":{"dtype":"f64","shape":[2,1],"data":b},"C":{"dtype":"f64","shape":[1,2],"data":c},"D":{"dtype":"f64","shape":[1,1],"data":d},"ts":0.0});
        let s1 = ss([ -1.0, 0.2, 0.0, -2.0 ], [1.0, 0.5], [1.0, 0.0], [0.3]);
        let s2 = ss([ -0.5, 1.0, 0.0, -3.0 ], [0.0, 1.0], [2.0, 1.0], [0.0]);
        let e = run("control.lti.series", json!({"sys1": s1, "sys2": s2}));
        assert_eq!(e["status"], "ok", "{}", e["error"]);
        assert_eq!(e["result"]["realization"], "series_block");
        let a = &e["result"]["system"]["A"]["data"];
        assert_eq!(a[0], json!(-1.0));
        assert_eq!(a[12], json!(1.0)); // B2 C1 = [[0,0],[1,0]], so A[3,0] = 1
    }
}
