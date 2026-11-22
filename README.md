# AEOI — AI Engineering Operator Interface

面向 AI Agent 的工程计算算子接口规范（v0.3-draft）及其 Rust 参考实现。

- `spec-v0.3.md` — 规范全文（源稿 `spec-src.md`，`python3 build_spec.py` 生成；`spec-v0.2.md`、`spec-v0.1.md` 为历史版本）
- `schemas/` — JSON Schema（common / manifest / envelope / error）
- `examples/` — 12 个示例算子：v0.2 的 `control.lti.bode`、`control.lti.step`、`control.design.lqr`；v0.3 新增 `control.lti.convert`、`control.lti.poles_zeros`、`control.analysis.ctrb_obsv`、`control.analysis.margin`、`control.lti.c2d`、`control.lti.impulse`、`control.math.lyap`、`control.design.place`、`control.design.dlqr`
- `golden/python-control/` — 对照参考用例（参考值来自 python-control/scipy，**非 MATLAB**）
- `matlab/` — MATLAB golden 生成脚本（尚未在 MATLAB 中运行）
- `rust/` — cargo workspace：`aeoi-core`、`aeoi-control`、`aeoi-mcp`（stdio MCP server）

## 运行

```
cd rust
cargo test --release
cargo test --release -p aeoi-control --test parity -- --nocapture
cargo test --release -p aeoi-control --test parity_b2 -- --nocapture
./target/release/aeoi-mcp
```
