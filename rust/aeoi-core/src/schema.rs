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
        let m = inline(&serde_json::json!({"$ref": "aeoi://schemas/common/0.2/Matrix"}));
        assert!(!serde_json::to_string(&m).unwrap().contains("$ref"));
        let v = Validator::new(&m);
        assert!(v.violations(&serde_json::json!({"dtype":"f64","shape":[1],"data":[1]}), 5).is_empty());
        assert!(!v.violations(&serde_json::json!({"dtype":"f64","shape":[1],"data":[1],"bogus":1}), 5).is_empty());
        let sp = serde_json::json!({"dtype":"f64","shape":[1,1],"format":"csr","indptr":[0,1],"indices":[0],"data":[2]});
        assert!(v.violations(&sp, 5).is_empty());
    }
}
