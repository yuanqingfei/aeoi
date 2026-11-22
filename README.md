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

一个本地网页：云端 LLM（默认 Gemini）通过函数调用使用 12 个 AEOI 算子。左侧是中文对话，右侧是调用轨迹：模型选了哪个工具、请求 JSON、AEOI 响应（result / trust / warnings / error + remediation）、耗时与 token 用量；bode、step、impulse 的结果会自动画图（Plotly basic，已内置在仓库里，不走 CDN；加载失败时退回内置的简易 SVG 图）。没有 key 时可以选 `mock:demo` 模型，看一段脚本化的演示（含一次报错 → 按 remediation 自我纠错）。

纯 Rust（axum + tokio + reqwest/rustls-ring），静态页面编译进二进制，不需要 npm，也不依赖 Linux 专有库，Windows 应可直接构建：已在 Linux 上交叉编译 `x86_64-pc-windows-gnu` 通过（得到 `aeoi-web.exe`），但还没在真 Windows 机器上运行过，MSVC 工具链未验证。

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
   cargo run --release -p aeoi-web -- rescore ../reports/eval-gemini_gemini-3.1-flash-lite.json  # 用当前评分器重算旧报告（不调用模型）
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

### OpenRouter 免费模型

设置 `OPENROUTER_API_KEY` 后可用 `openrouter:<模型 id>`，例如 `openrouter:nvidia/nemotron-3-ultra-550b-a55b:free`。免费模型（id 以 `:free` 结尾）没买过额度的账号每天总共只有约 50 次请求（所有免费模型合计），而且很多免费模型经常因上游共享额度返回 429（`temporarily rate-limited upstream`）。哪些免费模型支持函数调用：`https://openrouter.ai/api/v1/models` 里 `supported_parameters` 含 `tools` 的那些。对比结果见 `reports/eval-comparison.md`。

### Gemini 免费额度

免费层按模型分别限额。`gemini-2.5-flash` 每天只有 20 次请求（每道题至少 2 次），网页里聊几句就会用完；用完后 API 返回 429（`GenerateRequestsPerDayPerProjectPerModel-FreeTier`），`aeoi-web` 会立即报错而不是等十几个小时。日常试用建议在下拉框里选 `gemini-3.1-flash-lite`（额度大得多，29 题评测一次跑完）。

### 浏览器版（GitHub Pages）

不想装 Rust 时可以直接打开 **https://yuanqingfei.github.io/aeoi/** ：12 个算子与工具桥编译成 WebAssembly 在浏览器里运行，Agent 循环也在页面里，没有任何服务器。

- 不填 key：选 `mock:demo`，离线演示（4 次工具调用，含一次自我纠错，带 Plotly 图）。
- 用真实模型：点右上角"API key"填自己的 Gemini 或 OpenRouter key。key **只保存在这个浏览器的 localStorage**，由页面直接发给 Google / OpenRouter（两家都允许跨域调用），不经过 AEOI 的任何服务器，也不在仓库里；"忘记 key"按钮会清除它。公用电脑上请不要保存。
- 与本地版是同一份 Rust 代码（`rust/aeoi-bridge`：函数声明、F-5 核对、provider 线格式、系统提示词），轨迹面板与画图相同。浏览器版没有评测 CLI。
- 数值：与 x86_64 原生版在 651 个 golden/示例请求中 66% 逐位相同，其余差异 ≤ 5e-15（病态 `lqr` 4.7e-6；多输入 `place` 的 K 不唯一，极点相同），见 `reports/parity-wasm-vs-native.json` 与规范 §8.5。
- 体积：wasm 2.97 MB（gzip 1.07 MB）+ Plotly basic 1.07 MB（gzip 354 KB）。

本地构建（需要 `rustup target add wasm32-unknown-unknown`、wasm-bindgen CLI 0.2.130，可选 binaryen 的 wasm-opt）：

```bash
pages/build.sh                       # 生成 _site/
python3 -m http.server -d _site 8000 # 打开 http://localhost:8000/
# wasm 与原生逐例比较
cd rust && cargo run --release -p aeoi-wasm --example golden_dump /tmp/cases.json > /tmp/native.jsonl
wasm-bindgen --target nodejs --out-dir /tmp/pkg-node target/wasm32-unknown-unknown/wasm-release/aeoi_wasm.wasm
node aeoi-wasm/parity/parity.mjs /tmp/pkg-node /tmp/cases.json /tmp/native.jsonl /tmp/parity.json
```

发布工作流放在 `pages/pages.workflow.yml`：复制到 `.github/workflows/pages.yml` 后（推送工作流文件需要带 `workflow` 权限的 token，或在 GitHub 网页上新建该文件），每次推送到 main 自动构建并发布。GitHub Pages 需要在 Settings → Pages → Source 选 "GitHub Actions"；私有仓库需要付费计划，否则要先把仓库设为公开。

### 设计要点

- 算子在进程内调用（`aeoi_control::registry()`），与 `aeoi-mcp` 走同一条 `DynOperator::call` 管线，信封逐字节相同；不需要子进程和 JSON-RPC，Windows 上更简单。
- 函数声明由 manifest + input schema 自动生成。Gemini 只接受 OpenAPI 子集，转换器（`rust/aeoi-web/src/schema_conv.rs`）的有损映射见规范 §8.4，`aeoi-web tools --losses` 打印汇总；算子仍按完整 schema 校验，所以有损只意味着模型看到的约束少了，不会放过非法输入。
- 工具报错（`status: "error"`）时，整个错误信封（含 `agent_hint` 与 `remediation` patch）原样回传给模型，模型可自行修正；轨迹面板统计"自我纠错"次数。
- 不许编造采样周期（规范 F-5）：`c2d` 的 `ts` 在 schema 中标为 `x-aeoi-provenance: "user"`，函数声明因此多一个必填参数 `_user_quote`，模型要逐字引用用户给出 ts 的那句话；引用不在用户消息里或数值对不上时，绑定层不调用算子，返回 `E.VALIDATION.UNGROUNDED_INPUT`（只有 `ask_user` 一种补救）。
- 模型返回空回合（如 `MALFORMED_FUNCTION_CALL`）时，循环最多追加 2 次提示让它重写调用。
- 回传给模型的结果里，超过 120 个数的数组会被均匀抽样并标注 `_downsampled`（网页显示完整结果）。

### 后续计划

- 用 Tauri 打包成桌面应用（复用同一个 axum 服务与静态页面），免去命令行启动。
- 流式输出模型文本；会话持久化。
