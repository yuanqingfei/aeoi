//! control.design.pid — parallel and standard PID as a transfer function (spec 10.16).
//!
//! Parallel:  C(s) = Kp + Ki/s + Kd·s/(Tf·s+1)
//! Standard:  C(s) = Kp·(1 + 1/(Ti·s) + Td·s/(Tf·s+1))
//! A zero gain drops that term exactly (the factor we introduced), which is part of the
//! definition, not a numerical minreal. Tf = 0 leaves a pure derivative (improper tf).
use aeoi_core::error::{ask_user, modify_input, OpError, Warning};
use aeoi_core::jcs::hash_value;
use aeoi_core::lti::{tf2ss_siso, Lti};
use aeoi_core::poly::{degree, strip};
use aeoi_core::sysalg::lti_json;
use aeoi_core::{ExecCtx, HostCtx, Operator, OperatorMeta, Output};
use serde_json::{json, Value};

operator_struct!(Pid, "control.design.pid");

pub struct In {
    num: Vec<f64>,
    den: Vec<f64>,
    proper: bool,
    as_ss: bool,
    params: Value,
    echoed: Value,
}

const PARALLEL_ONLY: [&str; 2] = ["Ki", "Kd"];
const STANDARD_ONLY: [&str; 2] = ["Ti", "Td"];

/// spec F-8: one error code for every mutually-exclusive field problem. `fields` are the offending
/// fields actually present; every patch only touches those (applies cleanly to the original arguments).
fn conflict(form: &str, form_given: bool, fields: &[&str], other_form_ok: bool) -> OpError {
    let (own, other) = if form == "parallel" { ("Kp、Ki、Kd、Tf", "standard") } else { ("Kp、Ti、Td、Tf", "parallel") };
    let mut e = OpError::new(
        "E.VALIDATION.OPTION_CONFLICT",
        format!("form = {form}{} 只接受 {own}；{} 属于 {other} 形式。两种形式的字段不能混用。", if form_given { "" } else { "（省略 form 时的默认值）" }, fields.join("、")),
        "不需要的项直接省略，不要填 0 占位。按 remediation 的补丁去掉多余字段，或改 form。",
    )
    .path(&format!("/{}", fields[0]))
    .detail("form", json!(form))
    .detail("form_given", json!(form_given))
    .detail("conflicting_fields", json!(fields));
    if other_form_ok {
        // every non-Kp/Tf field present belongs to the other form: switching form is lossless
        let op = if form_given { "replace" } else { "add" };
        e = e.remedy(modify_input("switch_form", &format!("改为 form = {other}（给出的字段都属于该形式）"), "high", false, json!([{"op": op, "path": "/form", "value": other}])));
    }
    let rm: Vec<Value> = fields.iter().map(|k| json!({"op": "remove", "path": format!("/{k}")})).collect();
    e.remedy(modify_input("drop", &format!("保留 form = {form}，去掉 {}", fields.join("、")), "medium", false, Value::Array(rm)))
        .remedy(ask_user("form", "向用户确认要用 parallel 还是 standard"))
}

/// Ti = 0 is read as "no integral" by people but means an infinite integral gain in the formula:
/// a specific message and patch instead of a bare range/schema message (spec F-8).
fn ti_zero(ti: f64) -> OpError {
    OpError::new(
        "E.VALIDATION.OPTION_CONFLICT",
        format!("Ti = {ti} 不是“没有积分”：standard 形式里积分项是 Kp/(Ti·s)，Ti ≤ 0 没有定义。没有积分时省略 Ti。"),
        "没有积分：去掉 Ti。要积分：给出 Ti > 0（秒），或改用 parallel 形式的 Ki。",
    )
    .path("/Ti")
    .detail("form", json!("standard"))
    .detail("conflicting_fields", json!(["Ti"]))
    .remedy(modify_input("omit_integral", "去掉 Ti（没有积分项）", "medium", false, json!([{"op": "remove", "path": "/Ti"}])))
    .remedy(ask_user("Ti", "向用户确认积分时间 Ti（秒）或是否不要积分"))
}

fn jn(x: f64) -> Value {
    aeoi_core::num(x)
}

impl Operator for Pid {
    type Input = In;
    fn meta(&self) -> &OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let form_given = args.get("form").is_some_and(|v| !v.is_null());
        let form = args.get("form").and_then(|x| x.as_str()).unwrap_or("parallel");
        let kp = args["Kp"].as_f64().unwrap();
        let has = |k: &str| args.get(k).is_some_and(|v| !v.is_null());
        let ki = args.get("Ki").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let kd = args.get("Kd").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let tf = args.get("Tf").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let td = args.get("Td").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let ti = args.get("Ti").and_then(|x| x.as_f64());
        let (wrong, own): (&[&str], &[&str]) = if form == "parallel" { (&STANDARD_ONLY, &PARALLEL_ONLY) } else { (&PARALLEL_ONLY, &STANDARD_ONLY) };
        let bad: Vec<&str> = wrong.iter().copied().filter(|k| has(k)).collect();
        if !bad.is_empty() {
            // switching form is offered only when it is lossless and leads to a valid request
            let other_form_ok = !own.iter().any(|k| has(k)) && !ti.is_some_and(|t| t <= 0.0);
            return Err(conflict(form, form_given, &bad, other_form_ok));
        }
        if form == "standard" && ti.is_some_and(|t| t <= 0.0) {
            return Err(ti_zero(ti.unwrap()));
        }
        let deriv = if form == "parallel" { kd != 0.0 } else { td != 0.0 };
        if tf > 0.0 && !deriv {
            return Err(OpError::new("E.VALIDATION.OPTION_CONFLICT", "Tf > 0 是微分滤波时间常数，但没有微分项（Kd = 0 或 Td = 0）。不能静默忽略。", "去掉 Tf，或给出微分项。")
                .path("/Tf")
                .detail("form", json!(form))
                .detail("conflicting_fields", json!(["Tf"]))
                .remedy(modify_input("drop", "去掉 Tf", "high", false, json!([{"op": "remove", "path": "/Tf"}])))
                .remedy(ask_user("Td", "向用户确认是否需要微分项")));
        }
        if tf < 0.0 || td < 0.0 {
            return Err(OpError::new("E.VALIDATION.RANGE", "Tf、Td 必须 ≥ 0。", "修正时间常数。").path(if tf < 0.0 { "/Tf" } else { "/Td" }));
        }
        let (num, den) = if form == "parallel" { parallel(kp, ki, kd, tf) } else { standard(kp, ti, td, tf) };
        let proper = degree(&num) <= degree(&den);
        let as_ss = args.get("output_kind").and_then(|x| x.as_str()) == Some("ss");
        if as_ss && !proper {
            return Err(OpError::new(
                "E.DOMAIN.IMPROPER_SYSTEM",
                "Tf = 0 且微分项非零时 C(s) 含理想微分，没有普通状态空间实现。",
                "output_kind 保持 tf，或给出 Tf > 0。不要擅自填一个 Tf。",
            )
            .path("/Tf")
            .remedy(modify_input("as_tf", "output_kind 改为 tf", "high", false, json!([{"op": "replace", "path": "/output_kind", "value": "tf"}])))
            .remedy(ask_user("Tf", "向用户询问微分滤波时间常数 Tf（秒）；没有给出时不要假设")));
        }
        let echoed = match form {
            "parallel" => json!({"form": "parallel", "Kp": jn(kp), "Ki": jn(ki), "Kd": jn(kd), "Tf": jn(tf)}),
            _ => json!({"form": "standard", "Kp": jn(kp), "Ti": ti.map(jn).unwrap_or(Value::Null), "Td": jn(td), "Tf": jn(tf)}),
        };
        Ok((In { num, den, proper, as_ss, params: echoed.clone(), echoed }, Vec::new()))
    }
    fn estimate(&self, i: &In) -> Option<Value> {
        Some(json!({"degree": degree(&i.den).max(0)}))
    }
    fn execute(&self, i: In, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        if !i.proper {
            warnings.push(Warning::new("W.DOMAIN.IMPROPER_RESULT", "C(s) 含理想微分（Tf = 0）。结果是 tf，step/bode/margin 会拒绝它；与严格真对象串联或闭环后通常变为 proper，或设置 Tf > 0。").path("/Tf"));
        }
        let ts = 0.0;
        let sys = if i.as_ss {
            let (a, b, c, d) = tf2ss_siso(&i.num, &i.den);
            Lti::Ss { a, b, c, d, ts }
        } else {
            Lti::Tf { num: vec![vec![i.num.clone()]], den: vec![vec![i.den.clone()]], ts, siso: true }
        };
        // Identity: C(s0) equals the closed form. s0 = 1 is not a pole of these denominators (pole at 0 or -1/Tf).
        let s0 = aeoi_core::lti::C64::new(1.0, 0.0);
        let ev = aeoi_core::sysalg::eval_tf(&sys, s0);
        let formula = aeoi_core::poly::polyval(&i.num, s0) / aeoi_core::poly::polyval(&i.den, s0);
        let rel = match ev {
            Some(g) => (g[(0, 0)] - formula).norm() / formula.norm().max(1.0),
            None => 1.0,
        };
        let result = json!({
            "system": lti_json(&sys),
            "realization": if i.as_ss { "controllable_canonical" } else { "pid_closed_form" },
            "minimal": Value::Null,
            "proper": i.proper,
            "parameters": i.echoed,
            "composition": {
                "operation": "pid",
                "operands": [],
                "parameters_sha256": hash_value(&i.params),
                "lineage": Value::Null,
                "lineage_checked": false
            }
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"formula_rel": aeoi_core::num(rel), "definition": "|C(1) − 闭式| / max(1, |闭式|)"},
            "condition": {},
            "error_estimate": {"formula_rel": aeoi_core::num(rel), "basis": "增益到有理系数是有限次乘加；在 s=1 处复核闭式"},
            "checks": {"formula_matches": rel <= 1e-12, "all_finite": i.num.iter().chain(&i.den).all(|x| x.is_finite())},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {}
        });
        Ok(Output { result, trust, warnings })
    }
}

/// (Kd·s² + Kp·s + Ki) / s , with the s cancelled when Ki = 0; Tf > 0 uses the filtered derivative.
fn parallel(kp: f64, ki: f64, kd: f64, tf: f64) -> (Vec<f64>, Vec<f64>) {
    if ki == 0.0 && kd == 0.0 {
        return (vec![kp], vec![1.0]);
    }
    if tf == 0.0 {
        return if ki == 0.0 { (strip(&[kd, kp]), vec![1.0]) } else { (strip(&[kd, kp, ki]), vec![1.0, 0.0]) };
    }
    if ki == 0.0 {
        (strip(&[kp * tf + kd, kp]), strip(&[tf, 1.0]))
    } else {
        (strip(&[kp * tf + kd, kp + ki * tf, ki]), vec![tf, 1.0, 0.0])
    }
}

fn standard(kp: f64, ti: Option<f64>, td: f64, tf: f64) -> (Vec<f64>, Vec<f64>) {
    match ti {
        None if td == 0.0 => (vec![kp], vec![1.0]),
        None if tf == 0.0 => (strip(&[kp * td, kp]), vec![1.0]),
        None => (strip(&[kp * (tf + td), kp]), strip(&[tf, 1.0])),
        Some(ti) if td == 0.0 => (strip(&[kp * ti, kp]), vec![ti, 0.0]),
        Some(ti) if tf == 0.0 => (strip(&[kp * td * ti, kp * ti, kp]), vec![ti, 0.0]),
        Some(ti) => (strip(&[kp * ti * (tf + td), kp * (ti + tf), kp]), vec![ti * tf, ti, 0.0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_terms() {
        assert_eq!(parallel(3.0, 0.0, 0.0, 0.0), (vec![3.0], vec![1.0]));
        assert_eq!(parallel(1.0, 2.0, 3.0, 0.0), (vec![3.0, 1.0, 2.0], vec![1.0, 0.0]));
        assert_eq!(parallel(1.0, 0.0, 4.0, 0.1), (vec![1.0 * 0.1 + 4.0, 1.0], vec![0.1, 1.0]));
        let (n, d) = standard(2.0, Some(4.0), 0.5, 0.1);
        // Kp * [Ti(Tf+Td), Ti+Tf, 1] / [Ti Tf, Ti, 0]
        assert!((n[0] - 2.0 * 4.0 * 0.6).abs() < 1e-12);
        assert!((d[0] - 0.4).abs() < 1e-12);
    }

    fn run(req: serde_json::Value) -> serde_json::Value {
        let reg = crate::registry();
        crate::find(&reg, "control.design.pid").unwrap().call(&req, &aeoi_core::HostCtx::default())
    }
    /// Apply an RFC 6902 patch of add/replace/remove ops on top-level keys (all F-8 patches are of this kind).
    fn apply(req: &serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
        let mut r = req.clone();
        for op in patch.as_array().unwrap() {
            let k = op["path"].as_str().unwrap().trim_start_matches('/');
            match op["op"].as_str().unwrap() {
                "remove" => assert!(r.as_object_mut().unwrap().remove(k).is_some(), "patch removes absent field {k}"),
                "replace" => { assert!(r.get(k).is_some()); r[k] = op["value"].clone(); }
                "add" => r[k] = op["value"].clone(),
                o => panic!("{o}"),
            }
        }
        r
    }
    fn patch_of<'a>(e: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
        e["error"]["remediation"].as_array().unwrap().iter().find(|r| r["id"] == id).map(|r| &r["patch"])
    }

    #[test]
    fn f8_ti_zero_under_parallel_is_option_conflict_with_clean_patch() {
        // EngDesign XG_02 / XG_12 live run: parallel PID with a stray Ti = 0 was a bare schema error.
        let req = serde_json::json!({"form": "parallel", "Kp": 0.02, "Ti": 0});
        let e = run(req.clone());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT", "{}", e["error"]);
        assert_eq!(e["error"]["details"]["conflicting_fields"], serde_json::json!(["Ti"]));
        assert!(patch_of(&e, "switch_form").is_none(), "Ti = 0 is invalid in standard too: no switch offered");
        let fixed = apply(&req, patch_of(&e, "drop").unwrap());
        assert_eq!(run(fixed)["status"], "ok");
    }
    #[test]
    fn f8_ti_zero_under_standard_has_specific_message() {
        let req = serde_json::json!({"form": "standard", "Kp": 2.0, "Ti": 0, "Td": 0.1, "Tf": 0.01});
        let e = run(req.clone());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert!(e["error"]["message"].as_str().unwrap().contains("不是“没有积分”"));
        assert!(!e["error"]["message"].as_str().unwrap().contains("minimum"));
        assert_eq!(run(apply(&req, patch_of(&e, "omit_integral").unwrap()))["status"], "ok");
    }
    #[test]
    fn f8_mixed_forms_and_inferred_form() {
        // standard with parallel gains: patch removes only the fields that are present
        let req = serde_json::json!({"form": "standard", "Kp": 1.0, "Ti": 2.0, "Kd": 0.5});
        let e = run(req.clone());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        assert_eq!(e["error"]["details"]["conflicting_fields"], serde_json::json!(["Kd"]));
        assert!(patch_of(&e, "switch_form").is_none(), "Ti is present, switching to parallel would conflict");
        assert_eq!(run(apply(&req, patch_of(&e, "drop").unwrap()))["status"], "ok");
        // form omitted (default parallel) but only standard fields given: switching form is lossless
        let req = serde_json::json!({"Kp": 1.0, "Ti": 2.0, "Td": 0.1});
        let e = run(req.clone());
        assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
        let sw = patch_of(&e, "switch_form").unwrap();
        assert_eq!(sw[0]["op"], "add");
        let ok = run(apply(&req, sw));
        assert_eq!(ok["status"], "ok_with_warnings", "ideal derivative → improper warning: {}", ok["error"]);
        assert_eq!(ok["result"]["parameters"]["form"], "standard");
    }
}
