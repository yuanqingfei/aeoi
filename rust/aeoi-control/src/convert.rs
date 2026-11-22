//! control.lti.convert — tf ↔ ss ↔ zpk (spec 10.4).
use crate::{check_states, mimo_err};
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::lti::{parse_lti, Lti};
use aeoi_core::sysalg::{convert, lti_json, response_discrepancy, tf_normalize};
use aeoi_core::{opt_num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Convert, "control.lti.convert");

pub struct ConvertInput {
    pub sys: Lti,
    pub to: String,
    pub normalize: bool,
}

impl Operator for Convert {
    type Input = ConvertInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(ConvertInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        if let Lti::Ss { a, .. } = &sys {
            check_states(&self.meta, a.nrows(), "/system/A")?;
        }
        let to = args["to"].as_str().unwrap().to_string();
        let (ny, nu) = sys.dims();
        if to == "zpk" && ny * nu != 1 {
            return Err(mimo_err("/to", "zpk 表示（v0.3）", ny, nu)
                .remedy(modify_input("to_tf", "改为转换到 tf（MIMO 支持）", "high", false, json!([{"op": "replace", "path": "/to", "value": "tf"}])))
                .remedy(modify_input("to_ss", "改为转换到 ss", "high", false, json!([{"op": "replace", "path": "/to", "value": "ss"}]))));
        }
        Ok((ConvertInput { sys, to, normalize: args.get("normalize").and_then(|x| x.as_bool()).unwrap_or(true) }, w))
    }
    fn estimate(&self, i: &ConvertInput) -> Option<Value> {
        Some(json!({"from": i.sys.kind(), "to": i.to}))
    }
    fn execute(&self, inp: ConvertInput, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let mut warnings = Vec::new();
        let (mut out, realization) = convert(&inp.sys, &inp.to).map_err(|e| match e {
            "degenerate" => OpError::new("E.DOMAIN.DEGENERATE_SYSTEM", "系统矩阵不是行/列满秩（传递函数恒为零或结构退化），零点无定义，无法写成 zpk。", "检查 B、C、D 是否全零或存在恒零通道。").path("/system"),
            _ => OpError::new("E.INTERNAL.PANIC", format!("内部转换错误：{e}"), "请报告该输入。"),
        })?;
        if inp.normalize {
            tf_normalize(&mut out);
        }
        let (ny, nu) = inp.sys.dims();
        if realization == "block_diagonal_controllable_canonical" || (realization == "ss2tf_characteristic_polynomial" && ny * nu > 1) {
            warnings.push(Warning::new("W.DOMAIN.NONMINIMAL_REALIZATION", format!("MIMO 转换得到的是非最小实现/表示（{realization}），阶次可能高于最小阶次；v0.3 不做零极点对消。")).path("/system"));
        }
        let disc = response_discrepancy(&inp.sys, &out);
        if let Some(d) = disc {
            if d > 1e-6 {
                warnings.push(Warning::new("W.NUMERICAL.CONVERSION_DISCREPANCY", format!("转换前后频率响应最大相对差 {d:.2e} 偏大（多项式系数/根对扰动敏感，例如重根或高阶）。")).path("/system"));
            }
        }
        let n_states = match &out {
            Lti::Ss { a, .. } => Some(a.nrows()),
            _ => None,
        };
        let result = json!({"system": lti_json(&out), "realization": realization, "minimal": Value::Null});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": {"response_max_rel_discrepancy": opt_num(disc), "definition": "在 9 个测试点（连续：jω，ω = 10^[-2..2]×max(1,max|p|)；离散：单位圆上 e^{jθ}）比较转换前后 G 的最大逐元素相对差（以 max|G| 归一），跳过离极点过近的点"},
            "condition": {"from": inp.sys.kind(), "to": inp.to, "output_states": n_states},
            "error_estimate": {"response_rel": opt_num(disc.map(|d| d.max(f64::EPSILON))), "basis": "后验：等于上述响应残差（非严格上界；对根与系数本身的误差不作保证）"},
            "checks": {"response_consistent": disc.map(|d| d <= 1e-6).unwrap_or(true)},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
