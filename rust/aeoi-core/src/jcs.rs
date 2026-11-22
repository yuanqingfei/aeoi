//! RFC 8785 (JCS) canonical JSON serialization + SHA-256 helpers (spec P-2).
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(71);
    s.push_str("sha256:");
    for b in d {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write(v, &mut out);
    out
}

pub fn hash_value(v: &Value) -> String {
    sha256_hex(canonical(v).as_bytes())
}

fn write(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&es_number(i as f64));
            } else if let Some(u) = n.as_u64() {
                out.push_str(&es_number(u as f64));
            } else {
                out.push_str(&es_number(n.as_f64().unwrap_or(f64::NAN)));
            }
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push(':');
                write(&m[*k], out);
            }
            out.push('}');
        }
    }
}

/// ECMAScript Number::toString for finite doubles (JCS number serialization).
pub fn es_number(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return "null".into();
    }
    let neg = x < 0.0;
    let s = format!("{:e}", x.abs()); // shortest round-trip digits, e.g. "1.2345e-7"
    let (mant, exp) = s.split_once('e').unwrap();
    let e: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = e + 1;
    let mut r = String::new();
    if neg {
        r.push('-');
    }
    if k <= n && n <= 21 {
        r.push_str(&digits);
        for _ in 0..(n - k) {
            r.push('0');
        }
    } else if 0 < n && n <= 21 {
        r.push_str(&digits[..n as usize]);
        r.push('.');
        r.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        r.push_str("0.");
        for _ in 0..(-n) {
            r.push('0');
        }
        r.push_str(&digits);
    } else {
        r.push_str(&digits[..1]);
        if k > 1 {
            r.push('.');
            r.push_str(&digits[1..]);
        }
        r.push('e');
        let ee = n - 1;
        r.push(if ee >= 0 { '+' } else { '-' });
        r.push_str(&ee.abs().to_string());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn es_numbers() {
        assert_eq!(es_number(1.0), "1");
        assert_eq!(es_number(0.1), "0.1");
        assert_eq!(es_number(-1.5e-7), "-1.5e-7");
        assert_eq!(es_number(1e21), "1e+21");
        assert_eq!(es_number(123456789012345680000.0), "123456789012345680000");
        assert_eq!(es_number(0.000001), "0.000001");
        assert_eq!(es_number(1.2345e-7), "1.2345e-7");
        assert_eq!(es_number(4.43697499233), "4.43697499233");
    }
    #[test]
    fn canonical_sorts_keys() {
        let v: Value = serde_json::from_str(r#"{"b":1.0,"a":[1,2.5,"x"],"c":{"z":null,"y":true}}"#).unwrap();
        assert_eq!(canonical(&v), r#"{"a":[1,2.5,"x"],"b":1,"c":{"y":true,"z":null}}"#);
    }
}
