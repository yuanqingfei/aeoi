//! aeoi-bridge — the runtime-free half of the LLM binding (spec §8.4), shared by the native test
//! bench (`aeoi-web`: axum + reqwest) and the browser build (`aeoi-wasm`: wasm-bindgen, fetch in JS).
//!
//! Everything here is synchronous and I/O-free: function-declaration generation and the Gemini
//! OpenAPI-subset converter (`schema_conv`), the in-process tool bridge with the F-5 grounding check
//! (`tools`), the provider wire formats and the offline demo script (`llm`), and answer-number
//! parsing (`answer`).
pub mod answer;
pub mod llm;
pub mod schema_conv;
pub mod tools;

/// Chinese system prompt explaining AEOI conventions to the model.
pub const SYSTEM_PROMPT: &str = include_str!("system_prompt.zh.md");
