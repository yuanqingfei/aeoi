# AEOI — AI Engineering Operator Interface

面向 AI Agent 的工程计算算子接口规范（v0.3-draft）及其 Rust 参考实现。

- `spec-v0.3.md` — 规范全文（源稿 `spec-src.md`，`python3 build_spec.py` 生成；`spec-v0.2.md`、`spec-v0.1.md` 为历史版本）
- `schemas/` — JSON Schema（common / manifest / envelope / error）
- `examples/` — 12 个示例算子：v0.2 的 `control.lti.bode`、`control.lti.step`、`control.design.lqr`；v0.3 新增 `control.lti.convert`、`control.lti.poles_zeros`、`control.analysis.ctrb_obsv`、`control.analysis.margin`、`control.lti.c2d`、`control.lti.impulse`、`control.math.lyap`、`control.design.place`、`control.design.dlqr`
- `golden/python-control/` — 对照参考用例（参考值来自 python-control/scipy，**非 MATLAB**）
- `matlab/` — MATLAB golden 生成脚本（尚未在 MATLAB 中运行）
- `rust/` — cargo workspace：`aeoi-core`、`aeoi-control`、`aeoi-mcp`（stdio MCP server）、`aeoi-web`（LLM 端到端测试台 + 评测）
- `eval/` — LLM 评测题库（`questions.zh.json`，由 `gen_questions.py` 生成）；报告写入 `reports/eval-<model>.json`

## 运行

```
cd rust
cargo test --release
cargo test --release -p aeoi-control --test parity -- --nocapture
cargo test --release -p aeoi-control --test parity_b2 -- --nocapture
./target/release/aeoi-mcp
```

## LLM 端到端测试台（`aeoi-web`）

一个本地网页：云端 LLM（默认 Gemini）通过函数调用使用 12 个 AEOI 算子。左侧是中文对话，右侧是调用轨迹：模型选了哪个工具、请求 JSON、AEOI 响应（result / trust / warnings / error + remediation）、耗时与 token 用量；bode、step、impulse 的结果会自动画图（Plotly，经 CDN 加载；离线时只显示数值表）。

纯 Rust（axum + tokio + reqwest/rustls-ring），静态页面编译进二进制，不需要 npm，也不依赖 Linux 专有库，Windows（MSVC 工具链）可直接构建。

### Windows 上运行

1. 安装 Rust（https://rustup.rs ，选 MSVC 工具链；需要 Visual Studio Build Tools 的 "C++ build tools"）。
2. 设置 API key（只在当前终端有效；**不要写进仓库**）：
   - PowerShell：`$env:GEMINI_API_KEY = "你的 key"`
   - cmd：`set GEMINI_API_KEY=你的 key`
   - 永久保存：`setx GEMINI_API_KEY "你的 key"`（之后新开终端生效）
3. 启动：
   ```
   cd rust
   cargo run --release -p aeoi-web
   ```
   浏览器打开 http://127.0.0.1:8787/ 。服务只监听本机回环地址；`--addr 0.0.0.0:8787` 可改，但这样局域网内谁都能用你的 key，不建议。
4. 命令行评测（自动按免费额度节流，遇到 429/503 会按服务端给出的 retryDelay 退避重试）：
   ```
   cargo run --release -p aeoi-web -- eval --model gemini:gemini-2.5-flash --rpm 8
   cargo run --release -p aeoi-web -- eval --model gemini:gemini-2.5-flash --only lqr,c2d   # 只跑部分题
   cargo run --release -p aeoi-web -- ask "G(s)=4/(s^2+1.2s+4) 的超调量是多少？"           # 单题，打印轨迹 JSON
   cargo run --release -p aeoi-web -- tools --losses                                         # 函数声明的有损映射汇总
   ```
   Linux/macOS 用法相同（`export GEMINI_API_KEY=...`）。

### 模型与 provider

模型写成 `provider:model`，网页里也可以下拉选择或手动输入：

| provider | 例子 | 环境变量 |
|---|---|---|
| Gemini（默认，必选） | `gemini:gemini-2.5-flash`、`gemini:gemini-3.5-flash` | `GEMINI_API_KEY`（可选 `GEMINI_BASE_URL`） |
| OpenRouter | `openrouter:qwen/qwen3-coder:free` | `OPENROUTER_API_KEY` |
| Groq | `groq:llama-3.3-70b-versatile` | `GROQ_API_KEY` |
| Ollama（本地 /v1） | `ollama:qwen2.5:7b` | 无需 key；`OLLAMA_BASE_URL` 默认 `http://localhost:11434/v1` |
| OpenAI / 其他兼容端点 | `openai:gpt-4o-mini`、`compat:<model>` | `OPENAI_API_KEY`；`OPENAI_COMPAT_BASE_URL` + `OPENAI_COMPAT_API_KEY` |

不写 provider 前缀时按 Gemini 处理；也可以用环境变量 `AEOI_MODEL` 设默认模型。网页的模型下拉框列出你的 key 可用的 Gemini 模型（ListModels）。

### 设计要点

- 算子在进程内调用（`aeoi_control::registry()`），与 `aeoi-mcp` 走同一条 `DynOperator::call` 管线，信封逐字节相同；不需要子进程和 JSON-RPC，Windows 上更简单。
- 函数声明由 manifest + input schema 自动生成。Gemini 只接受 OpenAPI 子集，转换器（`rust/aeoi-web/src/schema_conv.rs`）的有损映射见规范 §8.4，`aeoi-web tools --losses` 打印汇总；算子仍按完整 schema 校验，所以有损只意味着模型看到的约束少了，不会放过非法输入。
- 工具报错（`status: "error"`）时，整个错误信封（含 `agent_hint` 与 `remediation` patch）原样回传给模型，模型可自行修正；轨迹面板统计"自我纠错"次数。
- 回传给模型的结果里，超过 120 个数的数组会被均匀抽样并标注 `_downsampled`（网页显示完整结果）。

### 后续计划

- 用 Tauri 打包成桌面应用（复用同一个 axum 服务与静态页面），免去命令行启动。
- 流式输出模型文本；会话持久化；把 uPlot 内置以支持完全离线画图。
