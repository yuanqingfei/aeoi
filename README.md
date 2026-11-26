# AEOI — AI Engineering Operator Interface

面向 AI Agent 的工程计算算子接口规范（v0.4.23-draft）及其 Rust 参考实现，以一个 MCP stdio 服务器提供。每个结果都带容差类、后置条件检查、不可依赖字段的标注、带修复补丁的结构化错误，以及可重放的收据。改动历史见 `CHANGELOG.md`；给 AI agent 的入口见 `AGENTS.md`。

## 目标与进度

目标是覆盖工程计算里最常用的 102 个 MATLAB 函数（规范 12.3）。清单、每个函数依赖的共享内核和依赖层都在 `roadmap/targets.py` 里，由它生成 `roadmap/layers.md` 和内核依赖图 `roadmap/deps.mmd`。实现顺序是按依赖层从底往上。102 个全部完成后发布 v0.5，在此之前不发布新版本。

| 类别 | 目标 | 已覆盖 |
|---|---|---|
| 控制 | 35 | 24 |
| 线性代数 | 15 | 9 |
| 信号 | 15 | 3 |
| 多项式、插值、拟合 | 9 | 4 |
| 微积分与 ODE | 10 | 4 |
| 非线性方程与优化 | 10 | 5 |
| 统计 | 8 | 0 |
| 合计 | 102 | 49（48%） |

这 49 个函数由 46 个 AEOI 算子提供：

- 控制（21 个）：`control.lti.{convert, step, impulse, lsim, bode, poles_zeros, c2d, pade, series, parallel, feedback, stepinfo}`、`control.analysis.{margin, ctrb_obsv, loopsens}`、`control.design.{lqr, dlqr, place, pid, pidtune}`、`control.math.lyap`
- 信号：`signal.spectral.fft`、`signal.design.butter`、`signal.filter.filter`
- 线性代数：`linalg.system.solve`、`linalg.matrix.{inv, det, pinv, norm, cond}`、`linalg.subspace.{rank, basis}`、`linalg.decomp.eig`、`linalg.decomp.svd`
- 插值、拟合与多项式：`interp.grid.interp1`、`fit.poly.polyfit`、`fit.poly.polyval`、`fit.poly.roots`
- 微积分与 ODE：`ode.ivp.solve`、`calculus.quadrature.integral`、`calculus.quadrature.trapz`
- 非线性方程与优化：`nonlinear.root.fzero`、`nonlinear.system.fsolve`、`optim.linear.linprog`、`optim.local.fmincon`、`optim.local.fminsearch`

所有算子的数值计算都调用 `aeoi-core::kernels` 里的 25 个共享内核（LU/QR/Cholesky、SVD、实 Schur 与特征分解、FFT、补偿求和、Riccati、频率响应、自适应积分、Runge–Kutta 等）。一个算子用到另一个算子的计算时，调用的是同一段代码，不另写一份，也不改调外部库（规范 11.5）。外部数值代码只剩 nalgebra（只作矩阵容器和做矩阵乘法）和编入二进制的 Rust `libm`。

## 仓库内容

- `spec-v0.4.md`：核心协议（约定、manifest、类型、信封与收据、错误、可复现性、凭证、绑定、治理与一致性）。源稿是 `spec-src.md`，用 `python3 scripts/build_spec.py` 生成。历史版本 v0.1 – v0.3 见 tag [`v0.4.9`](https://github.com/yuanqingfei/aeoi/tree/v0.4.9)（`spec-v0.1.md` – `spec-v0.3.md`）。
- `profiles/`：算子契约（规范第 10 章），每个算子一个文件，与核心协议同样是规范性的。
- `docs/reference-impl.md`：Rust 参考实现说明（结构、内核复用规则、各绑定的实现、评测结果）。
- `CHANGELOG.md`：每个版本的起因、改动与结果。
- `schemas/`：JSON Schema（common / manifest / envelope / error）。
- `examples/`：每个算子一个目录，含 manifest、输入输出 schema（由 `scripts/write_manifests.py` 生成）、示例请求与响应、`parity.yaml`。
- `golden/`：对照参考用例，共 1272 个。`python-control/` 下的参考值来自 python-control / numpy / scipy，`mpmath/` 下的是高精度真值。**参考值都不是 MATLAB 算的。** 生成脚本在 `scripts/golden/`。
- `scripts/`：`write_manifests.py`（manifest 与 schema 的唯一来源）、`regen_examples.py`、`validate_examples.py`、`build_spec.py`；`scripts/golden/` 是 golden 与示例请求的生成脚本。都从仓库根目录运行。
- `roadmap/`：102 个目标函数与内核的清单和依赖层。
- `rust/`：cargo workspace。
  - `aeoi-core`：信封、校验、收据，以及共享内核 `kernels`。
  - `aeoi-libm`：编入二进制的数学库。
  - `aeoi-control`：控制算子。
  - `aeoi-numeric`：控制以外的算子与表达式语言。
  - `aeoi-ops`：跨领域注册表与命令行工具 `aeoi-call`。
  - `aeoi-mcp`：stdio MCP 服务器。
  - `aeoi-bridge`、`aeoi-web`：LLM 端到端测试台与评测。
  - `aeoi-wasm`：浏览器版。
  - `vendor/matrixmultiply`：固定使用通用内核的补丁副本。
- `conformance/`：跨实现一致性套件（规范 9.4），见 `conformance/README.md`。
- `matlab/`：MATLAB golden 生成脚本，尚未在 MATLAB 里运行过。
- `eval/`：LLM 评测题库，报告写入 `reports/eval-<model>.json`。
- `reports/`：parity、一致性、评测报告。

## 安装

从 [Releases](https://github.com/yuanqingfei/aeoi/releases) 下载，不需要 Rust：

- **Claude Desktop（macOS、Windows）**：下载 `aeoi.mcpb`，双击安装；双击没反应时，在 Claude Desktop 里打开 设置 → 桌面应用（Desktop app）→ Extensions 旁的“管理” → 高级设置 → 安装扩展（Install Extension…），选中 `aeoi.mcpb`（旧版界面是 设置 → 扩展）。注意不是“插件”页。也可以把文件拖进 Claude Desktop 窗口。它只是给 Claude 多加一组工具，随时可以停用或卸载。
- **任何 MCP 客户端**（Claude Code、Cursor、VS Code 等）：下载对应平台的压缩包，把 `aeoi-mcp` 登记为 stdio 服务器，例如 `claude mcp add aeoi -- /path/to/aeoi-mcp`。Cursor、VS Code 在各自的 MCP 配置里写 `{"command": "/path/to/aeoi-mcp"}`。
- **不经过 LLM**：`aeoi-call <算子 id> request.json` 输出结果信封，`aeoi-call verify …` 重放收据。

最新正式发布是 **v0.4.9**，只含当时的 23 个算子（控制 21 个、`fft`、`ode.ivp.solve`）。之后加的算子要从源码构建（见下节），下一次发布是 v0.5。

同一请求在各平台上的结果逐位相同（规范 2.1 M-7a）。发布流水线会在 Linux x86_64、Linux aarch64、macOS arm64、macOS x86_64、Windows x86_64 五个平台上构建，并比较全部 golden 用例的 `output_hash`。

## 运行

```
cd rust
cargo build --release -p aeoi-mcp -p aeoi-ops
cargo test --workspace --release                                  # 含全部 parity 测试，报告写到 ../reports/
./target/release/aeoi-mcp                                         # MCP stdio 服务器
./target/release/aeoi-call signal.spectral.fft ../examples/signal.spectral.fft/request.json
```

完整的验证步骤（manifest、示例校验、一致性套件、变异测试、规范构建）见 `AGENTS.md`。

每个成功结果都带**结果收据**（`receipt`，规范 §4.3）：方法 id、前提（以及由谁保证）、精确性类与误差界、后置条件证据（只有全部通过才是 `verified`）、请求/结果两层指纹。复核：`./target/release/aeoi-call verify control.design.lqr ../examples/control.design.lqr/request.json ../examples/control.design.lqr/response.success.json`，或 MCP 工具 `aeoi_verify`，或网页轨迹面板的"重放验证"按钮。

与独立的纯 Rust 实现 [tpt-control](https://github.com/tpt-solutions/tpt-control)（MIT OR Apache-2.0）的交叉验证见 `reports/parity-tpt-control.md`（开发工具 `tools/xcheck-tpt/`，不是任何 aeoi crate 的依赖）。

## LLM 端到端测试台（`aeoi-web`）

一个本地网页：云端 LLM（默认 Gemini）通过函数调用使用全部 39 个 AEOI 算子。左侧是中文对话，右侧是调用轨迹：模型选了哪个工具、请求 JSON、AEOI 响应（result / trust / warnings / error + remediation）、耗时与 token 用量；bode、step、impulse 的结果会自动画图（Plotly basic，已内置在仓库里，不走 CDN；加载失败时退回内置的简易 SVG 图）。没有 key 时可以选 `mock:demo` 模型，看一段脚本化的演示（含一次报错 → 按 remediation 自我纠错）。

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

### 设计任务评测（`aeoi-web design`）

问答评测检查模型会不会用对算子。设计评测检查另一件事：给一段工程设计题（设计控制器 C(s)、滤波器、增益），模型用分析算子反复检查，再调用 `submit_design` 提交，由独立评分器打分。算子只做分析，不做传递函数运算，串联和闭环要模型自己算多项式。任务来源、与上游评分的差异、以及为什么 ControlEval 的题不能进仓库，见 `eval/design/README.md`。

```
cd rust
# 离线，不需要 key，也不访问网络。mock:design 先调用一次 poles_zeros，再按题目字段交占位答案
# （数字都是 1；ZC_02 的 alpha=1 会通过，其余题照常评分，多半不通过）
cargo run --release -p aeoi-web -- design --model mock:design --engdesign ZC_02,XG_13
# 自己的脚本：JSON 数组，元素为 {"calls":[{"name","args"}]}、{"text":"..."} 或 {"error":"QUOTA_EXHAUSTED: ..."}
cargo run --release -p aeoi-web -- design --mock-script turns.json --engdesign ZC_02

# 真实模型（key 只放环境变量，不要写进仓库）
export GEMINI_API_KEY=...          # 或 OPENROUTER_API_KEY
cargo run --release -p aeoi-web -- design --model gemini:gemini-3.1-flash-lite \
    --engdesign all --rpm 10 --max-steps 10 --call-timeout 150 --task-timeout 600
cargo run --release -p aeoi-web -- design --model openrouter:nvidia/nemotron-3-ultra-550b-a55b:free \
    --engdesign XG_13,ZC_02 --rpm 8 --max-steps 8
```

额度与问答评测相同：Gemini 免费层按模型分开计算，`gemini-2.5-flash` 每天约 20 次请求，设计题每题最多 `--max-steps` 次（默认 10），用 `gemini-3.1-flash-lite`；OpenRouter 免费模型（id 以 `:free` 结尾）没买额度的账号每天合计约 50 次请求，而且经常因上游共享池返回 429。每日配额（Gemini 的 `PerDay`、OpenRouter 的 `free-models-per-day`）会立刻停止并在报告的 `aborted.not_run` 里列出还没开始的题。短暂的 429/503 会按服务端的 `retryDelay` 退避，但单次请求的重试总时间有上限（默认 150 秒），到了就记 `provider_error` 并进入下一题。

报告在每道题开始时、以及每个模型回合之后写入 `reports/design-<model>.json`（先写临时文件再改名，中途杀掉进程会留下上一份完整检查点）。`--call-timeout`（默认 150 秒）限制单次模型请求，`--task-timeout`（默认 600 秒）限制一整道题。这两项是为了避免一次挂起的连接把整次 ControlEval 跑卡死、报告却一直是空的。试点结论见 `reports/design-pilot.md`。

只给提交、不调用模型：`aeoi-web design-score submissions.json`（`[{id, answer}]`；ControlEval 要另带 `controleval_task`）。EngDesign 的参考答案由单元测试覆盖；`eval/design/pyscore.py` 用 python-control / scipy 对同一份提交再打一遍，核对通过/不通过是否一致。

### 浏览器版（GitHub Pages）

不想装 Rust 时可以直接打开 **https://yuanqingfei.github.io/aeoi/** （会跳转到账号的自定义域名 https://yuanqingfei.me/aeoi/ ）：全部 39 个算子与工具桥编译成 WebAssembly 在浏览器里运行，Agent 循环也在页面里，没有任何服务器。

- 不填 key：选 `mock:demo`，离线演示（4 次工具调用，含一次自我纠错，带 Plotly 图）。
- 用真实模型：点右上角"API key"填自己的 Gemini 或 OpenRouter key。key **只保存在这个浏览器的 localStorage**，由页面直接发给 Google / OpenRouter（两家都允许跨域调用），不经过 AEOI 的任何服务器，也不在仓库里；"忘记 key"按钮会清除它。公用电脑上请不要保存。
- 与本地版是同一份 Rust 代码（`rust/aeoi-bridge`：函数声明、F-5 核对、provider 线格式、系统提示词），轨迹面板与画图相同。浏览器版没有评测 CLI。
- 数值：与 Linux x86_64 原生版逐位相同。每次推送 main，Pages 工作流的 parity 任务都会把全部 golden 与示例请求分别在 wasm 和原生版上运行并比较。2026-11-25（提交 `4f1114b`）的结果是 1295 / 1295 个请求逐位相同，状态或错误码不一致的为 0。`reports/parity-wasm-vs-native.json` 是 v0.4.9 之前的旧记录（当时 755 个里 566 个逐位相同），v0.4.9 把数学库编入二进制以后，剩下的差异都消失了（规范 2.1）。
- 体积：wasm 约 4.2 MB（未压缩），另加 Plotly basic 1.07 MB（gzip 后 354 KB）。

本地构建（需要 `rustup target add wasm32-unknown-unknown`、wasm-bindgen CLI 0.2.130，可选 binaryen 的 wasm-opt）：

```bash
pages/build.sh                       # 生成 _site/
python3 -m http.server -d _site 8000 # 打开 http://localhost:8000/
# wasm 与原生逐例比较
cd rust && cargo run --release -p aeoi-wasm --example golden_dump /tmp/cases.json > /tmp/native.jsonl
wasm-bindgen --target nodejs --out-dir /tmp/pkg-node target/wasm32-unknown-unknown/wasm-release/aeoi_wasm.wasm
node aeoi-wasm/parity/parity.mjs /tmp/pkg-node /tmp/cases.json /tmp/native.jsonl /tmp/parity.json
```

在线地址：https://yuanqingfei.me/aeoi/（`yuanqingfei.github.io/aeoi/` 会跳转到这里）。发布工作流在 `.github/workflows/pages.yml`，每次推送到 main 自动构建并发布到 GitHub Pages（Source = GitHub Actions）。

### 设计要点

- 算子在进程内调用（`aeoi_ops::registry()`），与 `aeoi-mcp` 走同一条 `DynOperator::call` 管线，信封逐字节相同；不需要子进程和 JSON-RPC，Windows 上更简单。
- 函数声明由 manifest + input schema 自动生成。Gemini 只接受 OpenAPI 子集，转换器（`rust/aeoi-web/src/schema_conv.rs`）的有损映射见规范 §8.4，`aeoi-web tools --losses` 打印汇总；算子仍按完整 schema 校验，所以有损只意味着模型看到的约束少了，不会放过非法输入。
- 工具报错（`status: "error"`）时，整个错误信封（含 `agent_hint` 与 `remediation` patch）原样回传给模型，模型可自行修正；轨迹面板统计"自我纠错"次数。
- 不许编造采样周期（规范 F-5）：`c2d` 的 `ts` 在 schema 中标为 `x-aeoi-provenance: "user"`，函数声明因此多一个必填参数 `_user_quote`，模型要逐字引用用户给出 ts 的那句话；引用不在用户消息里或数值对不上时，绑定层不调用算子，返回 `E.VALIDATION.UNGROUNDED_INPUT`（只有 `ask_user` 一种补救）。
- 模型返回空回合（如 `MALFORMED_FUNCTION_CALL`）时，循环最多追加 2 次提示让它重写调用。
- 回传给模型的结果里，超过 120 个数的数组会被均匀抽样并标注 `_downsampled`（网页显示完整结果）。

### 后续计划

- 流式输出模型文本；会话持久化。

## 许可（License）

- 代码（`rust/`、`pages/`、`tools/`、Python 脚本、schemas、examples、golden 数据）：**MIT OR Apache-2.0** 双许可，任选其一——见 [`LICENSE-MIT`](LICENSE-MIT)（Copyright (c) 2026 Qingfei Yuan）与 [`LICENSE-APACHE`](LICENSE-APACHE)。所有 crate 的 `Cargo.toml` 都声明 `license = "MIT OR Apache-2.0"`（workspace 继承）。
- 规范与文档（`spec-src.md`、`spec-v*.md`、`profiles/`、`docs/`、`CHANGELOG.md`、`reports/*.md`、本 README 的文字部分）：**CC-BY-4.0**，见 [`LICENSE-CC-BY-4.0`](LICENSE-CC-BY-4.0)。
- 除非另行明确声明，你有意提交给本项目的贡献按上述同样条款许可（Apache-2.0 第 5 条）。
- 第三方组件（内置的 Plotly.js basic，MIT；编译进二进制的 Rust crate；参考数据来源）见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。
