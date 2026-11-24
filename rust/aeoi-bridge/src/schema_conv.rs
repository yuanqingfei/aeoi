//! JSON Schema (draft 2020-12, AEOI input schemas, already `$ref`-inlined) → LLM tool parameter schemas.
//!
//! Two stages:
//! 1. `llm_surface` (all providers): restrict the AEOI input to what an LLM can sensibly produce —
//!    `Matrix` keeps only the inline-JSON `DenseMatrix` branch (b64 / file ref / sparse dropped),
//!    and the host-owned `exec` options are removed (the bridge sets them).
//! 2. `to_gemini` (Gemini only): map to the OpenAPI-3.0 subset accepted by the Generative Language API
//!    `FunctionDeclaration.parameters`. Every lossy step is recorded as a [`Loss`] so it can be
//!    documented (`aeoi-web tools --losses`) — the operator still validates the full schema, so a loss
//!    only means the *model* is told less, never that invalid input is accepted.
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Loss {
    pub path: String,
    pub what: String,
}

fn is_alt_matrix_branch(v: &Value) -> bool {
    let req: Vec<&str> = v.get("required").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    req.contains(&"data_b64") || req.contains(&"uri") || (req.contains(&"format") && req.contains(&"indices")) || (req.contains(&"format") && v["properties"].get("row_indices").is_some()) || (req.contains(&"format") && v["properties"]["format"].get("enum").is_some() && v["properties"].get("data").is_none())
}

const DENSE_SHORT: &str = "稠密矩阵 {dtype:\"f64\", shape:[行,列], data:[...]}；data 按 row-major（逐行）展平，长度 = 行×列";
/// Optional, defaulted or cosmetic properties hidden from the model (the operator applies defaults).
const HIDDEN: &[&str] = &["time_unit", "state_names", "input_names", "output_names", "structure"];

fn is_dense_matrix(o: &Map<String, Value>) -> bool {
    let p = o.get("properties").and_then(|p| p.as_object());
    p.map(|p| p.contains_key("dtype") && p.contains_key("shape") && p.contains_key("data") && !p.contains_key("data_b64")).unwrap_or(false)
}

/// Stage 1b: token diet. DenseMatrix → f64-only compact form with a short description; hidden
/// optional fields dropped; spec-internal remarks (warning codes) trimmed from descriptions.
fn slim(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            if is_dense_matrix(o) {
                let d = o.get("description").and_then(|d| d.as_str()).unwrap_or("");
                let field = if d.starts_with("内联 JSON 稠密数组") { String::new() } else { d.split("内联 JSON 稠密数组").next().unwrap_or("").trim_end_matches('。').to_string() };
                let desc = if field.is_empty() { DENSE_SHORT.to_string() } else { format!("{field}。{DENSE_SHORT}") };
                return json!({"type": "object", "description": desc, "properties": {"dtype": {"enum": ["f64"]}, "shape": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 2, "maxItems": 2}, "data": {"type": "array", "items": {"type": "number"}}}, "required": ["dtype", "shape", "data"]});
            }
            let mut m = Map::new();
            for (k, x) in o {
                if k == "properties" {
                    let props: Map<String, Value> = x.as_object().unwrap().iter().filter(|(pk, _)| !HIDDEN.contains(&pk.as_str())).map(|(pk, pv)| (pk.clone(), slim(pv))).collect();
                    m.insert(k.clone(), Value::Object(props));
                } else if k == "description" {
                    let d = x.as_str().unwrap_or("");
                    let d = d.split("。前导零").next().unwrap_or(d);
                    m.insert(k.clone(), json!(d));
                } else if k == "x-aeoi-shape-constraints" {
                    // mirrored into the description (unknown keywords are dropped by function-calling subsets)
                    let sc: Vec<&str> = x.as_array().map(|a| a.iter().filter_map(|s| s.as_str()).collect()).unwrap_or_default();
                    let prev = m.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                    m.insert("description".into(), json!(format!("{}维数约束：{}", if prev.is_empty() { String::new() } else { format!("{prev}；") }, sc.join("，"))));
                } else if k == "$schema" || k == "$id" || k == "x-aeoi-provenance" {
                    continue;
                } else {
                    m.insert(k.clone(), slim(x));
                }
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(slim).collect()),
        _ => v.clone(),
    }
}

/// Name of the binding-level grounding argument (spec F-5): for every top-level input marked
/// `x-aeoi-provenance: "user"` the model must copy the user's own words that state the value.
pub const USER_QUOTE: &str = "_user_quote";

/// Top-level input properties that must come from the user (`x-aeoi-provenance: "user"`).
pub fn user_provenance_fields(schema: &Value) -> Vec<String> {
    schema["properties"].as_object().map(|p| p.iter().filter(|(_, v)| v["x-aeoi-provenance"] == "user").map(|(k, _)| k.clone()).collect()).unwrap_or_default()
}

/// Stage 1 (provider-independent).
pub fn llm_surface(schema: &Value) -> Value {
    let mut out = slim(&llm_surface_raw(schema));
    let fields = user_provenance_fields(schema);
    if !fields.is_empty() {
        let mut qp = Map::new();
        for f in &fields {
            qp.insert(f.clone(), json!({"type": "string", "description": format!("用户消息里给出 {f} 的原文片段，逐字复制（含数值与单位，如“采样周期 T = 0.1 s”）。用户没有给出时不要调用本工具，先向用户询问")}));
            if let Some(d) = out["properties"][f.as_str()]["description"].as_str() {
                let d = format!("{d}【必须来自用户原话，并在 {USER_QUOTE}.{f} 中引用】");
                out["properties"][f.as_str()]["description"] = json!(d);
            }
        }
        out["properties"][USER_QUOTE] = json!({"type": "object", "description": "来源证明（AEOI F-5）：以下参数必须是用户明确给出的，不能由模型假设。绑定层会核对引用确实出现在用户消息中且数值一致，否则返回 E.VALIDATION.UNGROUNDED_INPUT", "properties": qp, "required": fields});
        if let Some(r) = out["required"].as_array_mut() {
            r.push(json!(USER_QUOTE));
        }
    }
    out
}

fn llm_surface_raw(schema: &Value) -> Value {
    fn walk(v: &Value, top: bool) -> Value {
        match v {
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, x) in o {
                    if top && k == "properties" {
                        let mut props = Map::new();
                        for (pk, pv) in x.as_object().unwrap() {
                            if pk == "exec" {
                                continue;
                            }
                            props.insert(pk.clone(), walk(pv, false));
                        }
                        m.insert(k.clone(), Value::Object(props));
                        continue;
                    }
                    if k == "oneOf" || k == "anyOf" {
                        let branches: Vec<Value> = x.as_array().unwrap().iter().filter(|b| !is_alt_matrix_branch(b)).map(|b| walk(b, false)).collect();
                        if branches.len() == 1 {
                            // unwrap a single remaining branch into the parent
                            if let Value::Object(b) = &branches[0] {
                                for (bk, bv) in b {
                                    if bk == "description" {
                                        let parent = m.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                                        let merged = if parent.is_empty() || parent.starts_with("任一数组编码") { bv.as_str().unwrap_or("").to_string() } else { format!("{parent}。{}", bv.as_str().unwrap_or("")) };
                                        m.insert("description".into(), Value::String(merged));
                                        continue;
                                    }
                                    m.insert(bk.clone(), bv.clone());
                                }
                            }
                            continue;
                        }
                        m.insert(k.clone(), Value::Array(branches));
                        continue;
                    }
                    m.insert(k.clone(), walk(x, false));
                }
                if let Some(Value::Array(req)) = m.get_mut("required") {
                    if top {
                        req.retain(|r| r != "exec");
                    }
                }
                Value::Object(m)
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| walk(x, false)).collect()),
            _ => v.clone(),
        }
    }
    walk(schema, true)
}

fn json_type_of(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

fn append_desc(m: &mut Map<String, Value>, extra: &str) {
    let d = m.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
    let nd = if d.is_empty() { extra.to_string() } else { format!("{d}；{extra}") };
    m.insert("description".into(), Value::String(nd));
}

/// Stage 2: Gemini OpenAPI subset. Supported keywords kept: type, nullable, description, enum (strings),
/// properties, required, items, minItems, maxItems, minimum, maximum, minLength, maxLength, pattern, anyOf.
pub fn to_gemini(schema: &Value, losses: &mut Vec<Loss>) -> Value {
    conv(schema, "", losses)
}

fn conv(v: &Value, path: &str, losses: &mut Vec<Loss>) -> Value {
    let Value::Object(o) = v else { return v.clone() };
    let mut m = Map::new();
    let mut loss = |what: String| losses.push(Loss { path: if path.is_empty() { "/".into() } else { path.into() }, what });
    if let Some(d) = o.get("description") {
        m.insert("description".into(), d.clone());
    }
    // type (possibly a union with null)
    match o.get("type") {
        Some(Value::String(t)) => {
            m.insert("type".into(), json!(t));
        }
        Some(Value::Array(ts)) => {
            let non_null: Vec<&Value> = ts.iter().filter(|t| *t != "null").collect();
            if non_null.len() == 1 {
                m.insert("type".into(), non_null[0].clone());
            } else {
                loss(format!("type union {ts:?} 只保留第一个类型"));
                m.insert("type".into(), non_null[0].clone());
            }
            if non_null.len() < ts.len() {
                m.insert("nullable".into(), json!(true));
            }
        }
        _ => {}
    }
    if let Some(c) = o.get("const") {
        match c {
            Value::String(_) => {
                m.insert("type".into(), json!("string"));
                m.insert("enum".into(), json!([c]));
            }
            other => {
                loss(format!("const {other} 非字符串，Gemini enum 只支持字符串；改写进 description"));
                m.insert("type".into(), json!(json_type_of(other)));
                append_desc(&mut m, &format!("必须等于 {other}"));
            }
        }
    }
    if let Some(Value::Array(e)) = o.get("enum") {
        if e.iter().all(|x| x.is_string()) {
            m.insert("type".into(), json!("string"));
            m.insert("enum".into(), Value::Array(e.clone()));
        } else {
            loss("非字符串 enum 改写进 description".into());
            append_desc(&mut m, &format!("取值之一：{}", Value::Array(e.clone())));
        }
    }
    for k in ["minItems", "maxItems", "minimum", "maximum", "minLength", "maxLength", "pattern"] {
        if let Some(x) = o.get(k) {
            m.insert(k.into(), x.clone());
        }
    }
    if let Some(x) = o.get("exclusiveMinimum") {
        loss(format!("exclusiveMinimum {x} → minimum（边界值本身不再被模型侧排除）"));
        m.insert("minimum".into(), x.clone());
        append_desc(&mut m, &format!("必须 > {x}"));
    }
    if let Some(x) = o.get("exclusiveMaximum") {
        loss(format!("exclusiveMaximum {x} → maximum"));
        m.insert("maximum".into(), x.clone());
        append_desc(&mut m, &format!("必须 < {x}"));
    }
    if let Some(d) = o.get("default") {
        loss("default 移入 description".into());
        append_desc(&mut m, &format!("默认 {d}"));
    }
    if let Some(Value::Object(props)) = o.get("properties") {
        let mut pm = Map::new();
        for (k, x) in props {
            let child = conv(x, &format!("{path}/{k}"), losses);
            if child.as_object().map(|c| c.is_empty()).unwrap_or(false) {
                continue;
            }
            pm.insert(k.clone(), child);
        }
        m.insert("type".into(), json!("object"));
        m.insert("properties".into(), Value::Object(pm));
    }
    if let Some(r) = o.get("required") {
        m.insert("required".into(), r.clone());
    }
    if let Some(ap) = o.get("additionalProperties") {
        if ap != &Value::Bool(false) && o.get("properties").is_none() {
            // free-form map (e.g. exec.tolerance): Gemini needs declared properties
            losses.push(Loss { path: path.into(), what: "additionalProperties 映射（无固定属性）不可表达，整个字段对模型隐藏".into() });
            return json!({});
        }
        if ap == &Value::Bool(false) {
            // Gemini rejects unknown properties anyway is NOT guaranteed; the operator schema still enforces it
            losses.push(Loss { path: path.into(), what: "additionalProperties:false 未表达（由算子 schema 校验兜底）".into() });
        }
    }
    if let Some(Value::Array(pi)) = o.get("prefixItems") {
        let types: Vec<&Value> = pi.iter().map(|p| &p["type"]).collect();
        losses.push(Loss { path: path.into(), what: format!("prefixItems（元组，{} 位）→ 同构 items + minItems/maxItems", pi.len()) });
        let item = if types.iter().all(|t| *t == types[0]) { conv(&pi[0], &format!("{path}/0"), losses) } else { json!({}) };
        m.insert("type".into(), json!("array"));
        m.insert("items".into(), item);
        m.insert("minItems".into(), json!(pi.len()));
        m.insert("maxItems".into(), json!(pi.len()));
    } else if let Some(items) = o.get("items") {
        m.insert("type".into(), json!("array"));
        m.insert("items".into(), conv(items, &format!("{path}/items"), losses));
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(br)) = o.get(key) {
            if key == "oneOf" {
                losses.push(Loss { path: path.into(), what: "oneOf → anyOf（互斥性丢失；以 kind/mode 等判别字段的 enum 区分分支）".into() });
            }
            let conv_br: Vec<Value> = br.iter().enumerate().map(|(i, b)| conv(b, &format!("{path}/{key}/{i}"), losses)).collect();
            // anyOf of plain scalar types that Gemini cannot mix inside arrays: collapse number|Complex|bool|NonFinite → number
            let scalarish = conv_br.iter().all(|b| b.get("properties").is_none());
            if scalarish && conv_br.iter().any(|b| b["type"] == "number") && conv_br.len() > 1 {
                losses.push(Loss { path: path.into(), what: format!("标量联合（{} 个分支）收窄为 number；复数/布尔/非有限值不对模型开放", conv_br.len()) });
                let mut n = conv_br.iter().find(|b| b["type"] == "number").unwrap().as_object().unwrap().clone();
                if let Some(d) = m.get("description").cloned() {
                    n.insert("description".into(), d);
                }
                return Value::Object(n);
            }
            m.insert("anyOf".into(), Value::Array(conv_br));
        }
    }
    for k in o.keys() {
        let known = ["description", "type", "const", "enum", "minItems", "maxItems", "minimum", "maximum", "minLength", "maxLength", "pattern", "exclusiveMinimum", "exclusiveMaximum", "default", "properties", "required", "additionalProperties", "prefixItems", "items", "oneOf", "anyOf", "title"];
        if !known.contains(&k.as_str()) {
            losses.push(Loss { path: path.into(), what: format!("关键字 `{k}` 不被支持，已丢弃") });
        }
    }
    Value::Object(m)
}

/// Dedupe losses by (normalised path, what) for documentation.
pub fn summarize(losses: &[Loss]) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = Vec::new();
    for l in losses {
        if let Some(e) = v.iter_mut().find(|(w, _)| *w == l.what) {
            e.1 += 1;
        } else {
            v.push((l.what.clone(), 1));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANNED: &[&str] = &["$ref", "oneOf", "const", "prefixItems", "additionalProperties", "exclusiveMinimum", "exclusiveMaximum", "default", "$defs", "title"];

    fn assert_gemini_clean(v: &Value, path: &str) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if path.ends_with("/properties") {
                        assert_gemini_clean(x, &format!("{path}/{k}"));
                        continue;
                    }
                    assert!(!BANNED.contains(&k.as_str()), "banned keyword {k} at {path}");
                    if k == "enum" {
                        assert!(x.as_array().unwrap().iter().all(|e| e.is_string()), "non-string enum at {path}");
                    }
                    assert_gemini_clean(x, &format!("{path}/{k}"));
                }
            }
            Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| assert_gemini_clean(x, &format!("{path}/{i}"))),
            _ => {}
        }
    }

    #[test]
    fn all_operator_schemas_convert_cleanly() {
        for op in aeoi_ops::registry() {
            let s = llm_surface(&op.meta().input_schema);
            let txt = s.to_string();
            assert!(!txt.contains("data_b64") && !txt.contains("\"uri\""), "{}: alt matrix encodings must be dropped", op.meta().id);
            assert!(s["properties"].get("exec").is_none());
            let mut losses = vec![];
            let g = to_gemini(&s, &mut losses);
            assert_gemini_clean(&g, "");
            assert_eq!(g["type"], "object", "{}", op.meta().id);
        }
    }

    #[test]
    fn dense_matrix_shape_survives() {
        let reg = aeoi_ops::registry();
        let lqr = aeoi_ops::find(&reg, "control.design.lqr").unwrap();
        let mut l = vec![];
        let g = to_gemini(&llm_surface(&lqr.meta().input_schema), &mut l);
        let a = &g["properties"]["A"];
        assert_eq!(a["type"], "object");
        assert_eq!(a["properties"]["data"]["type"], "array");
        assert_eq!(a["properties"]["data"]["items"]["type"], "number");
        assert!(a["description"].as_str().unwrap().contains("row-major"));
        assert_eq!(a["properties"]["dtype"]["enum"], json!(["f64"]));
        let top = g["description"].as_str().unwrap_or("");
        assert!(top.contains("B: [n,m]"), "{top}");
    }

    #[test]
    fn const_and_union_types() {
        let mut l = vec![];
        let g = to_gemini(&json!({"type":"object","properties":{"k":{"const":"tf"},"n":{"const":3},"s":{"type":["integer","null"]},"x":{"type":"number","exclusiveMinimum":0}}}), &mut l);
        assert_eq!(g["properties"]["k"]["enum"], json!(["tf"]));
        assert_eq!(g["properties"]["n"]["type"], "integer");
        assert_eq!(g["properties"]["s"]["nullable"], true);
        assert_eq!(g["properties"]["x"]["minimum"], 0);
        assert_eq!(l.len(), 2);
    }
}
