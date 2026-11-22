use crate::error::OpError;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Host-side configuration (resolved by the binding layer, never by operators; spec P-8).
#[derive(Debug, Clone)]
pub struct HostCtx {
    /// Root directory for file:// MatrixRef resolution. None => refs are rejected.
    pub data_root: Option<PathBuf>,
    /// Max decoded bytes for a single b64/ref array (default 64 MiB).
    pub max_decoded_bytes: usize,
    /// Max elements for a single inline JSON array (default 1e6).
    pub max_inline_elements: usize,
}

impl Default for HostCtx {
    fn default() -> Self {
        HostCtx { data_root: None, max_decoded_bytes: 64 << 20, max_inline_elements: 1_000_000 }
    }
}

/// Per-call execution context.
#[derive(Debug, Clone)]
pub struct ExecCtx {
    pub deadline: Instant,
    pub seed: Option<u64>,
    pub trust_full: bool,
}

impl ExecCtx {
    pub fn new(timeout_ms: u64) -> Self {
        ExecCtx { deadline: Instant::now() + Duration::from_millis(timeout_ms), seed: None, trust_full: false }
    }
    /// Cooperative deadline check for long loops.
    pub fn check(&self) -> Result<(), OpError> {
        if Instant::now() > self.deadline {
            Err(OpError::new(
                "E.RESOURCE.TIMEOUT",
                "计算超过 exec.timeout_ms 限制被中止。",
                "缩小问题规模（更少频点/更短时间网格/更低阶次），或增大 exec.timeout_ms 后重试。",
            ))
        } else {
            Ok(())
        }
    }
}
