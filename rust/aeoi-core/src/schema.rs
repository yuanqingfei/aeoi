//! Embedded spec schemas, `$ref` inlining (spec 8.1: MCP inputSchema MUST be fully dereferenced) and validation.
use serde_json::{Map, Value};
use std::sync::OnceLock;

pub const COMMON: &str = include_str!("../../../schemas/common.schema.json");
pub const ENVELOPE: &str = include_str!("../../../schemas/envelope.schema.json");
pub const ERROR: &str = include_str!("../../../schemas/error.schema.json");
pub const MANIFEST: &str = include_str!("../../../schemas/manifest.schema.json");

fn docs() -> &'static Vec<Value> {
    static D: OnceLock<Vec<Value>> = OnceLock::new();
    D.get_or_init(|| [COMMON, ENVELOPE, ERROR, MANIFEST].iter().map(|s| serde_json::from_str(s).expect("embedded schema")).collect())
}

pub fn doc(name: &str) -> &'static Value {
    docs().iter().find(|d| d["$id"].as_str().map(|i| i.contains(&format!("/{name}/"))).unwrap_or(false)).expect("schema doc")
}

fn pointer<'a>(v: &'a Value, frag: &str) -> Option<&'a Value> {
    if frag.is_empty() {
        Some(v)
    } else {
        v.pointer(frag)
    }
}

fn resolve(r: &str, cur: &Value) -> (Value, Value) {
    if let Some(frag) = r.strip_prefix('#') {
        return (pointer(cur, frag).unwrap_or_else(|| panic!("bad ref {r}")).clone(), cur.clone());
    }
    let (base, frag) = r.split_once('#').unwrap_or((r, ""));
    for d in docs() {
        let id = d["$id"].as_str().unwrap();
        if base == id {
            return (pointer(d, frag).unwrap_or_else(|| panic!("bad ref {r}")).clone(), d.clone());
        }
        if let Some(name) = base.strip_prefix(&format!("{id}/")) {
            return (d["$defs"][name].clone(), d.clone());
        }
    }
    panic!("unresolvable $ref {r}")
}

fn inline_in(v: &Value, cur: &Value, depth: usize) -> Value {
    assert!(depth < 64, "schema $ref recursion too deep");
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get("$ref").and_then(|r| r.as_str()) {
                let (target, tdoc) = resolve(r, cur);
                let mut t = inline_in(&target, &tdoc, depth + 1);
                if let Value::Object(tm) = &mut t {
                    for (k, x) in o {
                        if k != "$ref" {
                            tm.insert(k.clone(), inline_in(x, cur, depth + 1));
                        }
                    }
                }
                return t;
            }
            let mut m = Map::new();
            for (k, x) in o {
                if k == "$defs" || k == "$id" || k == "$schema" {
                    continue;
                }
                m.insert(k.clone(), inline_in(x, cur, depth + 1));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| inline_in(x, cur, depth + 1)).collect()),
        _ => v.clone(),
    }
}

/// Fully inline all `$ref`s (internal and aeoi://schemas/...), dropping `$defs`/`$id`.
pub fn inline(schema: &Value) -> Value {
    let mut out = inline_in(schema, schema, 0);
    if let Value::Object(m) = &mut out {
        m.insert("$schema".into(), Value::from("https://json-schema.org/draft/2020-12/schema"));
    }
    out
}

pub struct Validator(jsonschema::JSONSchema);

impl Validator {
    pub fn new(inlined: &Value) -> Validator {
        Validator(jsonschema::JSONSchema::options().with_draft(jsonschema::Draft::Draft202012).compile(inlined).expect("schema compiles"))
    }
    /// (JSON Pointer, message) for each violation, at most `limit`.
    pub fn violations(&self, v: &Value, limit: usize) -> Vec<(String, String)> {
        match self.0.validate(v) {
            Ok(()) => vec![],
            Err(errs) => errs.take(limit).map(|e| (e.instance_path.to_string(), e.to_string())).collect(),
        }
    }
}

/// Result of [`diagnose`]: leaf-level violations (oneOf failures resolved to the intended branch) and
/// lossless RFC 6902 fixes the caller may apply without asking the user.
pub struct Diagnosis {
    pub violations: Vec<(String, String)>,
    pub patches: Vec<Value>,
}

fn raw_errors(schema: &Value, inst: &Value) -> Vec<(String, String, String, bool)> {
    let Ok(c) = jsonschema::JSONSchema::options().with_draft(jsonschema::Draft::Draft202012).compile(schema) else { return vec![] };
    let out = match c.validate(inst) {
        Ok(()) => vec![],
        Err(errs) => errs.map(|e| (e.instance_path.to_string(), e.schema_path.to_string(), e.to_string(), matches!(e.kind, jsonschema::error::ValidationErrorKind::OneOfNotValid))).collect(),
    };
    out
}

/// Nested numeric array → DenseMatrix (row-major), if rectangular.
fn nested_to_dense(v: &Value) -> Option<Value> {
    let rows = v.as_array()?;
    if rows.is_empty() {
        return None;
    }
    let ncol = rows[0].as_array()?.len();
    let mut data = vec![];
    for r in rows {
        let r = r.as_array()?;
        if r.len() != ncol {
            return None;
        }
        for x in r {
            data.push(Value::from(x.as_f64()?));
        }
    }
    Some(serde_json::json!({"dtype": "f64", "shape": [rows.len(), ncol], "data": data}))
}

fn is_dense_matrix_branch(b: &Value) -> bool {
    b["properties"].get("data").is_some() && b["properties"].get("shape").is_some() && b["required"].as_array().map(|r| r.iter().any(|x| x == "data")).unwrap_or(false)
}

fn diagnose_in(schema: &Value, inst: &Value, base: &str, out: &mut Diagnosis, depth: usize) {
    for (ipath, spath, msg, is_oneof) in raw_errors(schema, inst) {
        let full = format!("{base}{ipath}");
        if out.violations.len() >= 20 {
            return;
        }
        if !is_oneof || depth > 8 {
            out.violations.push((full, msg));
            continue;
        }
        let sub_ptr = spath.strip_suffix("/oneOf").unwrap_or(&spath);
        let (Some(sub), Some(sub_inst)) = (schema.pointer(sub_ptr), inst.pointer(&ipath)) else {
            out.violations.push((full, msg));
            continue;
        };
        let branches = sub["oneOf"].as_array().cloned().unwrap_or_default();
        // 1. nested array where a matrix is expected (the most common LLM mistake)
        if branches.iter().any(is_dense_matrix_branch) && sub_inst.is_array() {
            match nested_to_dense(sub_inst) {
                Some(dense) => {
                    out.violations.push((full.clone(), "矩阵必须写成 {\"dtype\":\"f64\",\"shape\":[行,列],\"data\":[...]} 对象（data 为 row-major 逐行展平），收到的是嵌套数组。".into()));
                    out.patches.push(serde_json::json!({"op": "replace", "path": full, "value": dense}));
                }
                None => out.violations.push((full, "矩阵必须写成 {\"dtype\",\"shape\",\"data\"} 对象，data 为 row-major 展平的一维数组；向量也要给出二维 shape（列向量 [n,1]，行向量 [1,n]）。".into())),
            }
            continue;
        }
        // 2. discriminator (string const property such as kind / mode)
        let mut chosen: Option<&Value> = None;
        if let Some(o) = sub_inst.as_object() {
            let mut disc_keys: Vec<String> = vec![];
            for b in &branches {
                if let Some(props) = b["properties"].as_object() {
                    for (k, p) in props {
                        if p.get("const").map(|c| c.is_string()).unwrap_or(false) && !disc_keys.contains(k) && k != "time_unit" && k != "unit" {
                            disc_keys.push(k.clone());
                        }
                    }
                }
            }
            for k in &disc_keys {
                if let Some(val) = o.get(k) {
                    chosen = branches.iter().find(|b| b["properties"][k]["const"] == *val);
                    if chosen.is_none() {
                        let allowed: Vec<Value> = branches.iter().filter_map(|b| b["properties"][k].get("const").cloned()).collect();
                        out.violations.push((format!("{full}/{k}"), format!("{k} = {val} 无效，应为 {} 之一。", Value::Array(allowed))));
                    }
                    break;
                }
            }
            if chosen.is_none() && !disc_keys.is_empty() && !disc_keys.iter().any(|k| o.contains_key(k)) {
                let allowed: Vec<Value> = branches.iter().filter_map(|b| b["properties"][&disc_keys[0]].get("const").cloned()).collect();
                out.violations.push((full.clone(), format!("缺少判别字段 \"{}\"（取值 {}）。", disc_keys[0], Value::Array(allowed))));
                continue;
            }
            if chosen.is_none() && disc_keys.iter().any(|k| o.contains_key(k)) {
                continue;
            }
        }
        // 3. fall back to the branch with the fewest violations
        let b = chosen.cloned().or_else(|| branches.iter().min_by_key(|b| raw_errors(b, sub_inst).len()).cloned());
        match b {
            Some(b) => diagnose_in(&b, sub_inst, &full, out, depth + 1),
            None => out.violations.push((full, msg)),
        }
    }
}

/// Explain why `inst` violates `schema` (inlined) at leaf level, resolving `oneOf` failures to the
/// branch the caller evidently meant (discriminator `kind`/`mode`, else fewest violations).
pub fn diagnose(schema: &Value, inst: &Value) -> Diagnosis {
    let mut d = Diagnosis { violations: vec![], patches: vec![] };
    diagnose_in(schema, inst, "", &mut d, 0);
    d
}

/// Envelope schema with `result` replaced by an operator's output schema (MCP outputSchema).
pub fn envelope_with_result(output_schema_inlined: &Value) -> Value {
    let mut env = inline(doc("envelope"));
    env["properties"]["result"] = serde_json::json!({"oneOf": [{"type": "null"}, output_schema_inlined.clone()]});
    if let Value::Object(m) = &mut env {
        m.remove("$schema");
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_has_no_refs() {
        let s = inline(doc("envelope"));
        assert!(!serde_json::to_string(&s).unwrap().contains("$ref"));
        let m = inline(&serde_json::json!({"$ref": "aeoi://schemas/common/0.4/Matrix"}));
        assert!(!serde_json::to_string(&m).unwrap().contains("$ref"));
        let v = Validator::new(&m);
        assert!(v.violations(&serde_json::json!({"dtype":"f64","shape":[1],"data":[1]}), 5).is_empty());
        assert!(!v.violations(&serde_json::json!({"dtype":"f64","shape":[1],"data":[1],"bogus":1}), 5).is_empty());
        let sp = serde_json::json!({"dtype":"f64","shape":[1,1],"format":"csr","indptr":[0,1],"indices":[0],"data":[2]});
        assert!(v.violations(&sp, 5).is_empty());
    }

    #[test]
    fn diagnose_resolves_oneof() {
        let s = inline(&serde_json::json!({"type":"object","properties":{"A":{"$ref":"aeoi://schemas/common/0.4/Matrix"},"system":{"$ref":"aeoi://schemas/common/0.4/LtiSystem"}}}));
        let d = diagnose(&s, &serde_json::json!({"A":[[0,1],[2,3]]}));
        assert_eq!(d.patches.len(), 1);
        assert_eq!(d.patches[0]["value"]["data"], serde_json::json!([0.0,1.0,2.0,3.0]));
        assert_eq!(d.patches[0]["path"], "/A");
        let d = diagnose(&s, &serde_json::json!({"system":{"kind":"tf","num":[1],"den":[1,1]}}));
        assert_eq!(d.violations.len(), 1, "{:?}", d.violations);
        assert!(d.violations[0].1.contains("ts"), "{:?}", d.violations);
        let d = diagnose(&s, &serde_json::json!({"system":{"kind":"transfer","num":[1],"den":[1,1],"ts":0}}));
        assert_eq!(d.violations[0].0, "/system/kind");
        let d = diagnose(&s, &serde_json::json!({"system":{"kind":"ss","A":{"dtype":"f64","shape":[1,1],"data":[1]},"B":[[1]],"C":{"dtype":"f64","shape":[1,1],"data":[1]},"D":{"dtype":"f64","shape":[1,1],"data":[0]},"ts":0}}));
        assert_eq!(d.patches[0]["path"], "/system/B", "{:?}", d.violations);
    }
}
