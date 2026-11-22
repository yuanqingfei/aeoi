use serde_json::{json, Map, Value};

/// Structured operator error (spec ch.5).
#[derive(Debug, Clone)]
pub struct OpError {
    pub code: String,
    pub retryable: bool,
    pub message: String,
    pub agent_hint: String,
    pub details: Map<String, Value>,
    pub remediation: Vec<Value>,
}

impl OpError {
    pub fn new(code: &str, message: impl Into<String>, agent_hint: impl Into<String>) -> Self {
        let retryable = !code.starts_with("E.INTERNAL.");
        OpError {
            code: code.to_string(),
            retryable,
            message: message.into(),
            agent_hint: agent_hint.into(),
            details: Map::new(),
            remediation: Vec::new(),
        }
    }
    pub fn category(&self) -> String {
        self.code.split('.').nth(1).unwrap_or("INTERNAL").to_ascii_lowercase()
    }
    pub fn path(mut self, p: &str) -> Self {
        self.details.insert("path".into(), Value::from(p));
        self
    }
    pub fn detail(mut self, k: &str, v: Value) -> Self {
        self.details.insert(k.into(), v);
        self
    }
    pub fn retryable(mut self, b: bool) -> Self {
        self.retryable = b;
        self
    }
    pub fn remedy(mut self, r: Value) -> Self {
        self.remediation.push(r);
        self
    }
    pub fn to_json(&self) -> Value {
        let mut rem = self.remediation.clone();
        if rem.is_empty() {
            rem.push(ask_user("ask", "向用户确认输入与建模假设"));
        }
        json!({
            "code": self.code,
            "category": self.category(),
            "retryable": self.retryable,
            "message": self.message,
            "agent_hint": self.agent_hint,
            "details": Value::Object(self.details.clone()),
            "remediation": rem,
        })
    }
}

/// `modify_input` remediation with an RFC 6902 patch.
pub fn modify_input(id: &str, desc: &str, confidence: &str, confirm: bool, patch: Value) -> Value {
    json!({"id": id, "kind": "modify_input", "description": desc, "confidence": confidence,
           "requires_user_confirmation": confirm, "patch": patch})
}
pub fn ask_user(id: &str, desc: &str) -> Value {
    json!({"id": id, "kind": "ask_user", "description": desc, "confidence": "high", "requires_user_confirmation": true})
}
pub fn retry_same(id: &str, desc: &str) -> Value {
    json!({"id": id, "kind": "retry_same", "description": desc, "confidence": "low", "requires_user_confirmation": false})
}

/// Warning (spec E-7).
#[derive(Debug, Clone)]
pub struct Warning {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub details: Option<Value>,
}

impl Warning {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Warning { code: code.into(), message: message.into(), path: None, details: None }
    }
    pub fn path(mut self, p: &str) -> Self {
        self.path = Some(p.into());
        self
    }
    pub fn details(mut self, d: Value) -> Self {
        self.details = Some(d);
        self
    }
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("code".into(), Value::from(self.code.clone()));
        m.insert("message".into(), Value::from(self.message.clone()));
        if let Some(p) = &self.path {
            m.insert("path".into(), Value::from(p.clone()));
        }
        if let Some(d) = &self.details {
            m.insert("details".into(), d.clone());
        }
        Value::Object(m)
    }
}
