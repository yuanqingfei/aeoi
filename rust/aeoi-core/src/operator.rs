//! Operator trait + framework pipeline (spec 8.2). Operators implement `validate` and `execute`;
//! the framework does schema validation, deadline/panic handling, hashing and envelope assembly.
use crate::error::{ask_user, retry_same, OpError, Warning};
use crate::host::{ExecCtx, HostCtx};
use crate::jcs::hash_value;
use crate::matrix::{canonicalize_for_hash, encode_b64_inplace};
use crate::schema::{inline, Validator};
use crate::SPEC;
use serde_json::{json, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};
use web_time::Instant;

/// Successful operator output; the framework adds hashes, timing and the envelope.
pub struct Output {
    pub result: Value,
    pub trust: Value,
    pub warnings: Vec<Warning>,
}

/// Static description of an operator: manifest + inlined schemas + compiled validator.
pub struct OperatorMeta {
    pub manifest: Value,
    pub id: String,
    pub version: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub determinism: String,
    pub tolerance_class: String,
    validator: Validator,
}

impl OperatorMeta {
    pub fn from_json(manifest: &str, input_schema: &str, output_schema: &str) -> OperatorMeta {
        let manifest: Value = serde_json::from_str(manifest).expect("manifest json");
        let input_schema = inline(&serde_json::from_str(input_schema).expect("input schema json"));
        let output_schema = inline(&serde_json::from_str(output_schema).expect("output schema json"));
        let validator = Validator::new(&input_schema);
        OperatorMeta {
            id: manifest["id"].as_str().unwrap().into(),
            version: manifest["version"].as_str().unwrap().into(),
            determinism: manifest["determinism"].as_str().unwrap().into(),
            tolerance_class: manifest["tolerance_class"].as_str().unwrap().into(),
            manifest,
            input_schema,
            output_schema,
            validator,
        }
    }
    /// Manifest with input/output schemas inlined (for resources/read).
    pub fn manifest_resolved(&self) -> Value {
        let mut m = self.manifest.clone();
        m["input_schema"] = self.input_schema.clone();
        m["output_schema"] = self.output_schema.clone();
        m
    }
    pub fn max_dense(&self, pointer: &str) -> usize {
        self.manifest["sparse_support"][pointer]["max_dense_elements"].as_u64().unwrap_or(1_000_000) as usize
    }
}

pub trait Operator: Send + Sync {
    type Input;
    fn meta(&self) -> &OperatorMeta;
    /// Semantic validation beyond JSON Schema; returns the parsed input. No main computation here.
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(Self::Input, Vec<Warning>), OpError>;
    /// Main computation. Deterministic per manifest.determinism; must call `ctx.check()` in long loops.
    fn execute(&self, input: Self::Input, ctx: &ExecCtx) -> Result<Output, OpError>;
    /// Optional cost estimate for exec.dry_run.
    fn estimate(&self, _input: &Self::Input) -> Option<Value> {
        None
    }
}

pub trait DynOperator: Send + Sync {
    fn meta(&self) -> &OperatorMeta;
    /// Never panics: always returns an envelope.
    fn call(&self, args: &Value, host: &HostCtx) -> Value;
}

pub fn build_string() -> String {
    format!(
        "aeoi-control {}+{} nalgebra-0.33 rustc-{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("AEOI_GIT_SHA").unwrap_or("nogit"),
        option_env!("AEOI_RUSTC_VERSION").unwrap_or("unknown")
    )
}

/// `<arch>-<os>`, e.g. `x86_64-linux`; the browser build (wasm32-unknown-unknown, OS = "") reports
/// `wasm32-unknown`. A different platform string means `platform_bitwise` results may differ.
pub fn platform() -> String {
    let os = if std::env::consts::OS.is_empty() { "unknown" } else { std::env::consts::OS };
    format!("{}-{}", std::env::consts::ARCH, os)
}

/// The C math library that `f64::exp`, `sin`, `cos`, `powf`, `hypot`, … resolve to. Part of the "platform"
/// of `platform_bitwise` (spec 2.1, v0.4): glibc 2.31 and 2.39 differ by 1–2 ulp on the same CPU features.
/// `glibc <version>` on linux-gnu (read at run time, since the library is linked dynamically), `rust-libm`
/// on wasm32, `<os>-system` elsewhere.
pub fn libm() -> String {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let s = {
        unsafe extern "C" {
            fn gnu_get_libc_version() -> *const std::os::raw::c_char;
        }
        // SAFETY: glibc returns a pointer to a static NUL-terminated string.
        let v = unsafe { std::ffi::CStr::from_ptr(gnu_get_libc_version()) };
        format!("glibc {}", v.to_string_lossy())
    };
    #[cfg(target_arch = "wasm32")]
    let s = String::from("rust-libm");
    #[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_arch = "wasm32")))]
    let s = format!("{}-system", std::env::consts::OS);
    s
}

pub fn cpu_features() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut f = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx") {
            f.push("avx");
        }
        if std::is_x86_feature_detected!("avx2") {
            f.push("avx2");
        }
        if std::is_x86_feature_detected!("fma") {
            f.push("fma");
        }
        if std::is_x86_feature_detected!("avx512f") {
            f.push("avx512f");
        }
    }
    f
}

fn op_header(m: &OperatorMeta) -> Value {
    json!({"id": m.id, "version": m.version, "build": build_string()})
}

pub fn error_envelope(m: &OperatorMeta, request_id: &Value, e: &OpError, warnings: &[Warning], t0: Instant) -> Value {
    json!({
        "spec": SPEC, "status": "error", "dry_run": false,
        "operator": op_header(m), "request_id": request_id, "result": Value::Null,
        "trust": Value::Null, "receipt": Value::Null, "reproducibility": Value::Null,
        "error": e.to_json(),
        "warnings": warnings.iter().map(|w| w.to_json()).collect::<Vec<_>>(),
        "timing": {"wall_ms": t0.elapsed().as_secs_f64() * 1e3}
    })
}

impl<T: Operator> DynOperator for T {
    fn meta(&self) -> &OperatorMeta {
        Operator::meta(self)
    }
    fn call(&self, args: &Value, host: &HostCtx) -> Value {
        let t0 = Instant::now();
        let m = Operator::meta(self);
        let exec = args.get("exec").cloned().unwrap_or(json!({}));
        let request_id = exec.get("request_id").cloned().unwrap_or(Value::Null);
        // 1. JSON Schema
        let viol = m.validator.violations(args, 20);
        if !viol.is_empty() {
            // spec E-9 (v0.4): resolve oneOf failures to leaf violations; offer lossless patches
            let diag = crate::schema::diagnose(&m.input_schema, args);
            let leaf = if diag.violations.is_empty() { viol.clone() } else { diag.violations };
            let mut e = OpError::new("E.VALIDATION.SCHEMA", format!("输入不符合 {} 的 inputSchema（{} 处违规）：{}", m.id, leaf.len(), leaf[0].1), "按 details.violations 中的 JSON Pointer 逐项修正后重试；注意所有对象都禁止多余字段。")
                .detail("violations", Value::Array(leaf.iter().map(|(p, msg)| json!({"path": p, "message": msg})).collect()))
                .detail("path", Value::from(leaf[0].0.clone()));
            if !diag.patches.is_empty() {
                e = e.remedy(crate::error::modify_input("nested_array_to_dense", "把嵌套数组形式的矩阵改写为 row-major DenseMatrix（无损，数值不变）", "high", false, Value::Array(diag.patches)));
            }
            let e = e.remedy(ask_user("ask", "若无法确定字段含义，向用户确认"));
            return error_envelope(m, &request_id, &e, &[], t0);
        }
        // 1b. method selection (spec 3.14 MS-6): hard constraints only, never a silent fallback.
        let method_policy = exec.get("method_policy").filter(|p| !p.is_null());
        let method = match crate::method::resolve(&m.manifest, method_policy) {
            Ok(x) => x,
            Err(e) => return error_envelope(m, &request_id, &e, &[], t0),
        };
        // 2-4. semantic validation (also resolves b64/ref/sparse)
        let validated = catch_unwind(AssertUnwindSafe(|| self.validate(args, host)));
        let (input, mut warnings) = match validated {
            Ok(Ok(x)) => x,
            Ok(Err(e)) => return error_envelope(m, &request_id, &e, &[], t0),
            Err(_) => return error_envelope(m, &request_id, &panic_err(), &[], t0),
        };
        let mut ctx = ExecCtx::new(exec.get("timeout_ms").and_then(|x| x.as_u64()).unwrap_or(30_000));
        ctx.seed = exec.get("seed").and_then(|x| x.as_u64());
        ctx.trust_full = exec.get("trust_level").and_then(|x| x.as_str()) == Some("full");
        ctx.method = method.id.clone();
        let mut hash_args = args.clone();
        if let Some(o) = hash_args.as_object_mut() {
            o.remove("exec");
        }
        let input_hash = hash_value(&canonicalize_for_hash(&hash_args, host));
        // 5. dry run
        if exec.get("dry_run").and_then(|x| x.as_bool()) == Some(true) {
            let est = self.estimate(&input);
            return json!({
                "spec": SPEC, "status": if warnings.is_empty() {"ok"} else {"ok_with_warnings"}, "dry_run": true,
                "operator": op_header(m), "request_id": request_id, "result": Value::Null, "estimate": est,
                "trust": Value::Null, "receipt": Value::Null, "reproducibility": Value::Null, "error": Value::Null,
                "warnings": warnings.iter().map(|w| w.to_json()).collect::<Vec<_>>(),
                "input_hash": input_hash,
                "timing": {"wall_ms": t0.elapsed().as_secs_f64() * 1e3}
            });
        }
        // 6. execute
        let out = match catch_unwind(AssertUnwindSafe(|| self.execute(input, &ctx))) {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return error_envelope(m, &request_id, &e, &warnings, t0),
            Err(_) => return error_envelope(m, &request_id, &panic_err(), &warnings, t0),
        };
        warnings.extend(out.warnings);
        let mut result = out.result;
        let output_hash = hash_value(&canonicalize_for_hash(&result, host));
        // spec 4.3 receipt + E-11 (no evidence, no success claim): may append W.EVIDENCE.POSTCONDITION_FAILED
        // MS-7: method_selection only when the request carried a method_policy (otherwise byte-identical to v0.3).
        let selection = method_policy.map(|p| json!({"policy": p, "used": method.id}));
        let receipt = crate::receipt::build_for(m, &method, selection, &input_hash, &output_hash, &out.trust, &mut warnings, &build_string(), &platform());
        if exec.get("output_encoding").and_then(|x| x.as_str()) == Some("b64") {
            encode_b64_inplace(&mut result, host);
        }
        json!({
            "spec": SPEC,
            "status": if warnings.is_empty() {"ok"} else {"ok_with_warnings"},
            "dry_run": false,
            "operator": op_header(m),
            "request_id": request_id,
            "result": result,
            "trust": out.trust,
            "receipt": receipt,
            "warnings": warnings.iter().map(|w| w.to_json()).collect::<Vec<_>>(),
            "error": Value::Null,
            "reproducibility": {
                "determinism": method.determinism, "tolerance_class": method.tolerance_class,
                "input_hash": input_hash, "output_hash": output_hash,
                "platform": platform(),
                "cpu_features": cpu_features(),
                "libm": libm(),
                "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
                "threads": 1, "seed": ctx.seed
            },
            "timing": {"wall_ms": t0.elapsed().as_secs_f64() * 1e3}
        })
    }
}

fn panic_err() -> OpError {
    OpError::new("E.INTERNAL.PANIC", "算子内部发生未预期的错误（panic 已被捕获）。", "这是实现缺陷，不是输入问题；可原样重试一次，仍失败请报告该输入。")
        .retryable(false)
        .remedy(retry_same("retry", "原样重试一次"))
}
