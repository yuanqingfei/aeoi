# AEOI 参考实现说明

> 说明性文档。规范见 [`spec-v0.4.md`](../spec-v0.4.md) 与 [`profiles/`](../profiles/)。这里描述 Rust 参考实现（`rust/`）如何满足规范、各绑定的实现，以及只约束参考实现本身的规则（11.5 RI-1 – RI-3）。节号沿用规范原来的编号，便于交叉引用。

## 2.1 跨平台逐位相同（M-7a）在参考实现里的做法

v0.4 把 libm 加进"平台"的依据：同一份源码、同一组 CPU 特性（x86_64-linux，avx/avx2/fma/avx512f），glibc 2.31（Ubuntu 20.04，WSL）与生成示例的环境（推断为 glibc 2.39）相比，`control.lti.pade` 的 `approximation.certificate.value` 差 1 ulp，`control.analysis.ctrb_obsv` 非对称示例的两个奇异值差 1–2 ulp，其余示例逐位相同。glibc 会动态链接，换系统而不重新编译也会改变它，所以 `libm` 在运行时读取。`build` 也要如实：参考实现的 `build` 曾在没有设置环境变量时写死 `rustc-1.85`，v0.4 改为由构建脚本记录实际编译器版本。v0.4 之前发布的示例没有 `libm`，与它们只能做容差比较，重新生成示例后恢复逐位比较。

v0.4.9 参考实现去掉了跨平台差异的两个来源。第一，**数学库**：原生构建在二进制内部定义 `sin`、`exp`、`pow`、`hypot`、`sincos` 等 C 符号，转给编入二进制的纯 Rust `libm`（crate `aeoi-libm`，`libm` 0.2.16，musl libm 的移植）。可执行文件里的定义优先于系统库（ELF、Mach-O、PE 都是这样），所以 nalgebra（SVD、矩阵指数、复数模长）和 rustfft（旋转因子）内部的调用也一起换掉了。`reproducibility.libm` 记为 `rust-libm 0.2.16`，不再随操作系统变化。实测 glibc 2.39 与 `libm` 0.2.16 在随机自变量上的不同率：`sin` 3%、`exp` 10%、`pow` 9%、`atan2` 26%、`log` 5%。第二，**矩阵乘法内核**：`matrixmultiply` 换成打过补丁的副本，固定用通用内核，即普通乘加、固定次序、不融合（`rust/vendor/matrixmultiply/AEOI-PATCH.md`）。另外，发布流水线在 5 个平台上的比较又找出两处：(a) 苹果平台上 LLVM 会把 `pow(10, x)` 改写成系统的 `__exp10(x)`，所以 `__exp10`/`exp10` 也要覆盖，并按 `pow(10, x)` 计算；(b) **编译期常量折叠**：参数为常量的 `f64::cos` 等，LLVM 在编译时就用*编译机*的 C 库算掉了。rustfft 固定尺寸蝶形的旋转因子就是这样，结果随“在哪个操作系统上编译”而变（macOS 11 个、Windows 16 个 FFT 用例），与 CPU 无关。rustfft 也换成补丁副本，旋转因子直接调用 `libm::cos/sin`：`libm` 是普通 Rust 代码，折叠它就是精确的 IEEE 运算。（v0.4.16 起 FFT 改用 AEOI 自己的内核 `kernels::fft`，旋转因子同样调用 `libm::cos/sin`，rustfft 补丁副本已删除。）此后参考实现的结果不再依赖 CPU 特性和操作系统，`cpu_features` 只作记录。43 个示例中有 9 个的 `output_hash` 变了，数值差都在各自的容差类内（最大是 `bode` 非对称状态空间示例，相对差 8.9e-12，T2），按 G-3 处理，算子版本不变。发布流水线已证实：在 Linux x86_64、Linux aarch64、macOS arm64、macOS x86_64、Windows x86_64 上各自构建的二进制，对全部 859 个 golden 请求给出完全相同的 `output_hash`；浏览器（wasm）版与 Linux 原生版 925 / 925 逐位相同。manifest 仍声明 `platform_bitwise`：这个承诺依然成立，只是比实际做到的弱。改成 `bitwise` 是对调用方的新承诺，按 G-2 要给每个算子升 MINOR 版本，所以留到各算子下一次升版本时一起做（开放问题 25）。

## 8. 绑定

### 8.2 Rust 核心 trait（参考实现 `rust/aeoi-core/src/operator.rs`）

```rust
pub trait Operator: Send + Sync + 'static {
    type Input;
    fn meta(&self) -> &OperatorMeta;
    /// schema 之外的语义校验；返回解析后的 Input（解析与校验交织，如解码 b64、稠密化稀疏矩阵）。不做主计算
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(Self::Input, Vec<Warning>), OpError>;
    /// 主计算；确定性按 manifest.determinism；长循环中必须调用 ctx.check()（超时/取消）
    fn execute(&self, input: Self::Input, ctx: &ExecCtx) -> Result<Output, OpError>;
    /// dry_run 时的代价估计
    fn estimate(&self, _input: &Self::Input) -> Option<Value> { None }
}
```

调用管线（`DynOperator` blanket impl，算子只实现上面 4 个方法）：

1. JSON Schema 校验（`jsonschema` crate，Draft 2020-12）→ 失败返回 `E.VALIDATION.SCHEMA`，`details.violations` 带最多 20 条 JSON Pointer；`oneOf` 失败按 F-3 递归诊断到叶子，可无损修正的嵌套数组矩阵附 F-4 补丁（v0.3.1）
2. `validate()`（在 `catch_unwind` 内）→ 解析 + 语义校验 + 规模上限，得到 `Input` 与 warnings
3. 计算 `input_hash`（3.7）
4. `dry_run` 时返回 `estimate()` 后停止
5. `execute()`（在 `catch_unwind` 内，受 `deadline` 约束）
6. 计算 `output_hash`；按需转为 b64 输出
7. 框架填充 `reproducibility`、`timing`，组装信封

### 8.3 各绑定的实现状态

| 绑定 | crate | 状态 |
|---|---|---|
| MCP server | `aeoi-mcp` | **已实现**（stdio，换行分隔 JSON-RPC 2.0；手写最小实现，不依赖 `rmcp`，以保持依赖少、兼容 Rust 1.85）。Streamable HTTP 未实现 |
| LLM 函数调用 | `aeoi-web` | **已实现（v0.3.1）**：Gemini（必选）与 OpenAI 兼容 provider；进程内调用；网页测试台 + 评测 CLI（8.4） |
| 浏览器（WebAssembly） | `aeoi-wasm` + `pages/` | **已实现（v0.3.1）**：算子与工具桥编译为 wasm（wasm-bindgen），Agent 循环在页面里，模型由浏览器用访问者自己的 key 直接调用；发布在 GitHub Pages（8.5） |
| CLI | `aeoi-call`（在 `aeoi-control` 中） | **已实现**：`aeoi-call <op-id|tool_name> <request.json|-> [--data-root DIR]`；v0.3.3：`aeoi-call verify <op-id> <request.json> <envelope.json>` |
| Python | `aeoi-py`（PyO3 + maturin） | 计划：`aeoi.call(id, args) -> dict`；NumPy 便捷层 MUST 遵守内存序规则；MUST 释放 GIL |
| C ABI | `aeoi-capi`（cdylib） | 计划：`aeoi_call(const char* id, const char* json_args, char** json_envelope)` + `aeoi_free` |

### 8.4 LLM 函数调用绑定：`aeoi-web`

`aeoi-web` 是端到端测试台：云端 LLM 通过函数调用使用 19 个算子，网页显示对话与逐步轨迹，CLI 跑评测。

**调用方式**：进程内调用 `aeoi_control::registry()` 的 `DynOperator::call`。这与 `aeoi-mcp` 是同一条管线（schema 校验 → validate → execute → 信封），信封逐字节相同；不经过 stdio 子进程与 JSON-RPC 帧，Windows 上部署更简单，延迟就是算子本身的耗时（亚毫秒到毫秒）。MCP 路径已由 `aeoi-mcp` 的端到端测试覆盖。

12 个工具的有损映射次数可用 `aeoi-web tools --losses` 打印（当前：`additionalProperties:false` 46 处、`oneOf→anyOf` 27 处、`default` 19 处、`prefixItems` 17 处、`exclusiveMinimum` 15 处）。所有声明合计约 38 KB JSON；Gemini 2.5 Flash 上每次请求约 1.7 万输入 token，其中大部分是工具声明。

**Agent 循环**：系统提示词（`rust/aeoi-web/src/system_prompt.zh.md`）用中文说明 row-major、tf 降幂、`ts` 含义、`rad/s` 与 `Hz`、`"Inf"`/`null`、remediation 的使用规则与工具选择提示。循环为"模型 → 工具调用（可并行多个）→ 信封 → 模型"，最多 `max_steps` 个模型回合（默认 8）。错误信封（含 `agent_hint` 与 `remediation`）原样回传；Gemini 的 `thoughtSignature` 按原样回传；超过 120 个数的数组按 F-6 抽样。"自我纠错"= 某工具报错后同一工具随后调用成功。v0.3.6 起 `LtiSystem` 对象从不抽样（K-6）。

**评测**：`eval/questions.zh.json` 29 道中文题，覆盖 12 个算子，其中 7 道是陷阱题（MATLAB 列优先数据 ×2、缺采样周期、Hz 与 rad/s、离散系统误当连续、不可镇定系统）。期望值不手抄：评测时用题目里的 `reference.args` 直接调用算子得到（算子已与 python-control/scipy 对照，见第 11 章；**不是 MATLAB**）。四个指标：工具选择（期望工具出现在调用中；澄清题要求不带猜测地调用）、参数正确性（对每个检查量，取含该量的最新一次成功调用，其输出与参考调用一致；例如先算 ω = 10 再画整条 Bode 曲线时取前一次）、最终回答（回答文本中能找到期望数值：识别 LaTeX `\frac{a}{b}`、`3.2 \times 10^{-3}`、`0{,}9048`、`±`、unicode 负号与上标指数；容差取相对 2% 与"印出精度的半个末位"（至少 2 位有效数字）中的较大者，期望值为 0 时绝对 1e-3；`z − 0.9048` 这类因式写法对系数检查允许符号相反；澄清题要求最终消息是一个问题且没有带猜测值的成功调用；报错题要求解释原因）、自我纠错次数。单元测试另外验证每道陷阱题的"典型错误"确实会得到不同的数值。

**设计评测（v0.3.4，v0.3.5 更新工具）**：`aeoi-web design` 把同一条 Agent 循环用在设计题上。模型拿到题目和 `submit_design` 伪工具，用算子检查设计，提交一次后由 `design.rs` 按该题的评分细则打分。v0.3.5 起模型可以调用串联、并联、反馈、PID、stepinfo 和 Padé，把上一个 `result.system` 传给下一个；v0.3.6 起评测提示词把 `loopsens` 作为检查回路的首选（K-4），并写明穿越频率与闭环带宽的区别、Hz 与 rad/s（K-5），以及回合预算（AL-1）；评测提示词告诉模型不要再手乘多项式，并声明“题目没写 Padé 阶次时用 N = 10 并在说明里写明”（这是提示词里的回退，不是 `pade.order` 的默认值，见 F-7）。EngDesign 的 10 道控制/DSP 题以 MIT 许可原文放在 `eval/design/engdesign.json`；ControlEval 没有许可证，题目在运行时下载到 gitignore 的 `eval/design/upstream/`，不进入仓库。没有 key 时用 `mock:design` 或 `mock:script` 离线跑通循环。单次请求和单题都有时限；每日配额立即停止；报告在每题开始和每个模型回合后落盘。哪些题因此不再被手算多项式挡住，见 `reports/design-pilot.md`。

**结果（2026-11-22，temperature 0，报告在 `reports/`，对比表见 `reports/eval-comparison.md`）**：

| 模型 | 题数 | 工具选择 | 参数正确 | 最终回答 | 说明 |
|---|---|---|---|---|---|
| gemini-3.1-flash-lite（v0.3.1 第一轮，旧评分器） | 29 | 93.1% | 92.9%（n=28） | 89.7% | 4 道失败 |
| 同一次运行，用新评分器 `rescore` | 29 | 93.1% | 96.4% | 93.1% | `c2d-01`、`bode-01` 是评分器误判；`ctrb-01`（`MALFORMED_FUNCTION_CALL`，无回答）与 `c2d-03-missing-ts`（自行假设 Ts = 0.1 s）是真实失败 |
| gemini-3.1-flash-lite（第二轮修改后） | 29 | 100% | 100% | 100% | 唯一的工具错误是题目预期的 `E.DOMAIN.NOT_STABILIZABLE`；`ctrb-01`、`c2d-03` 另各重跑 5 次全部通过 |
| gemini-2.5-flash（第一轮） | 10 | 10/10 | 10/10 | 10/10 | 第 11 题起免费额度（20 次/天）用尽，未写报告 |
| OpenRouter `nvidia/nemotron-3-ultra-550b-a55b:free` | 8 | 8/8 | 7/7 | 8/8 | 8 题子集（5 道陷阱题 + 3 道普通题）；免费账号每天 50 次请求 |
| OpenRouter `nvidia/nemotron-3.5-super-vl:free` | 8 | 8/8 | 7/7 | 8/8 | 同上 |
| OpenRouter `particle-ai/particle-1:free` | 8 | 8/8 | 7/7 | 7/8 | 唯一失败是上游 HTTP 429（重试 6 次后放弃），不是答错 |

解读：第二轮修改后，缺采样周期的陷阱在 5 次重跑里都是模型直接提问、没有调用 `c2d`，所以 `_user_quote` 核对（F-5(c)）在真实运行中没有被触发过，只有单元测试覆盖。8 题子集区分不出模型，需要完整 29 题。

### 8.5 浏览器绑定（`aeoi-wasm`，GitHub Pages，v0.3.1 新增）

纯静态页面 https://yuanqingfei.github.io/aeoi/ （账号的自定义域名生效时为 https://yuanqingfei.me/aeoi/ ；页面只用相对路径，CORS 预检对两个源都已核对）：不需要服务器，访问者用自己的 Gemini 或 OpenRouter key（BYOK），也可以不填 key 用 `mock:demo` 演示。

- **分层**：`aeoi-bridge` 是从 `aeoi-web` 拆出的、不含 tokio/reqwest 的同步部分——函数声明转换（schema_conv）、工具桥与 F-5 `_user_quote` 核对、错误信封与回传模型的抽样、provider 线格式（Gemini `generateContent` / OpenAI 兼容 `chat/completions` 的请求构造与响应解析）、空回合提示、Mock 脚本、回答评分。`aeoi-web`（原生）与 `aeoi-wasm`（浏览器）都用它，所以两边的函数声明、核对规则与系统提示词是同一份代码。
- **wasm API**（JSON 字符串进出）：`info`、`tools`、`system_prompt`、`build_request(provider, model, history)`、`parse_response(provider, body)`、`call_tool(name, args, history?, max_array?)`、`run_operator(id, request)`、`verify(id, request, envelope)` 与 `verify_tool_call(name, args, envelope)`（v0.3.3，4.3）、`nudge_text`、`demo_script`。页面的 JS 只做 HTTP 与 Agent 循环（与 `agent.rs` 同语义：最多步数、空回合最多提示 2 次、工具消息回传抽样后的信封）。Rust 的 Provider trait 要求 `Send` future，浏览器 fetch 的 future 不是 `Send`，因此循环放在 JS 一侧。
- **key**：只存放在该浏览器的 `localStorage`，只发给 provider 自己的 API（Gemini 用 `x-goog-api-key` 头，OpenRouter 用 `Authorization`）；页面有"忘记 key"。两家 API 都允许跨域调用（已核对 CORS 预检：Gemini 回显 `https://yuanqingfei.github.io` 并允许 `x-goog-api-key`；OpenRouter 为 `*`）。key 会被同源脚本读到——页面只加载仓库内的文件（Plotly 也已内置，不走 CDN）。
- **wasm 与原生的一致性**（`rust/aeoi-wasm/parity/parity.mjs`，报告 `reports/parity-wasm-vs-native.json`；v0.3.5：755 个请求，node 22 vs x86_64-linux）：状态、错误码、warning 码、`input_hash` 全部一致（`status_or_code_mismatch = 0`）。566 例结果逐位相同。串联、并联、反馈、pid、stepinfo 全部逐位相同；pade 的系数逐位相同，certificate 用到 `exp`，最大相对差约 5e-17。其余差异仍是末位舍入，多输入 `place` 的 K 不唯一：

| 算子 | 用例 | 逐位相同 | 最大范数相对差 |
|---|---|---|---|
| series、parallel、feedback、pid、stepinfo、convert、lyap、lqr | 14、13、14、12、9、79、47、91 | 全部 | 0 |
| pade | 12 | 3 | 5e-17（certificate 的 `exp`；系数逐位相同） |
| bode、step、impulse、pz、ctrb_obsv、margin、c2d | 106、91、31、50、29、48、50 | 5、81、28、43、20、35、38 | ≤ 5e-15 |
| dlqr | 28 | 20 | 1.5e-14 |
| place | 31 | 14 | 2.4e-2（见下） |

  v0.3.1（`lqr` 0.2.0）时 lqr 只有 35/74 例逐位相同、最大相对差 4.7e-6，且一个病态用例原生返回 `ok_with_warnings`（`W.NUMERICAL.ILL_CONDITIONED`）而 wasm 返回 `ok`（条件估计恰在阈值附近）；`lqr` 0.3.0 的补偿 Newton–Kleinman 把结果收敛到真值的 f64 舍入，消掉了两边舍入路径的差异，lqr 全部逐位相同，CI 的 `AEOI_PARITY_ALLOW_MISMATCH` 由 1 改为 0。`place` 的差异出在多输入 KNV 用例：多输入时满足极点配置的 K 不唯一，两边都达到了指定极点，只是迭代路径因舍入不同而停在不同的 K。由此得到两条规范性说明：**跨平台时，基于阈值的 warning 可能翻转**（消费者不能把 warning 集合当作跨平台确定）；**对解不唯一的算子，跨平台比较 MUST 用不变量（达到的闭环极点）而不是解本身**。
- **体积**：wasm 2.97 MB（gzip 1.07 MB；`wasm-release` profile：opt-level s、LTO、codegen-units 1、panic=abort，再经 `wasm-opt -Oz`），Plotly basic 1.07 MB（gzip 354 KB）。这些编译选项不改变 IEEE 运算结果（parity 用的就是这个 profile）。
- **构建与发布**：`.github/workflows/pages.yml`——Rust 1.85.1 + wasm32 目标，固定版本并校验 sha256 的 wasm-bindgen CLI 0.2.130（与 crate 的 `=0.2.130` 一致）与 binaryen version_133，`pages/build.sh` 组装 `_site/`，node 冒烟测试后用 `actions/deploy-pages` 发布；另一个不阻塞发布的 job 跑全量 parity 并上传报告。

## 11.1 结构

```
rust/                         cargo workspace（resolver 3，Rust 1.85，纯 Rust，无 C/Fortran 依赖）
├── aeoi-core/                规范层：错误/warning、JCS 哈希、schema 内联与校验、数组解析（json/b64/ref/sparse）、
│                             LTI 解析与 tf→ss、线性代数（expm、Lyapunov、条件数）、Operator trait 与调用管线；
│                             v0.3 新增 poly.rs（平衡伴随矩阵求根 + Newton 精修）、sysalg.rs（ss2tf、不变零点、
│                             Bartels–Stewart、频响求值、PBH），以及带迭代上限的 schur_robust
├── aeoi-control/             18 个算子 + aeoi-call CLI
│   ├── src/bode.rs step.rs(含 impulse) lqr.rs                       v0.2
│   ├── src/convert.rs poles_zeros.rs ctrb_obsv.rs margin.rs c2d.rs
│   │       lyap.rs place.rs dlqr.rs                                 v0.3
│   ├── tests/parity.rs parity_b2.rs parity_hp.rs examples.rs analytic_b2.rs receipt.rs
│   └── examples/golden_eval.rs   （v0.3.2）对任意 golden 文件跑算子并输出 JSON 行（tools/hp_compare.py 用于新旧版本对比）
├── aeoi-mcp/                 MCP stdio server + tests/e2e.rs
├── aeoi-bridge/              （v0.3.1）无运行时的同步部分，原生与 wasm 共用（8.5）
│   ├── src/schema_conv.rs    input schema → LLM 函数参数（收窄 + Gemini OpenAPI 子集），记录有损映射
│   ├── src/tools.rs          工具桥（进程内调用、F-5 来源核对、回传抽样）
│   ├── src/llm.rs            provider 线格式（Gemini / OpenAI 兼容请求构造与解析）、中性消息、空回合提示、Mock 脚本
│   ├── src/answer.rs         回答数值识别与评分；system_prompt.zh.md
├── aeoi-web/                 （v0.3.1）LLM 端到端测试台：axum 网页 + 评测 CLI（8.4）
│   ├── src/provider.rs       Provider trait（reqwest）；Gemini、OpenAI 兼容（OpenRouter/Groq/Ollama/OpenAI）、Mock
│   ├── src/agent.rs          Agent 循环
│   ├── src/eval.rs           评测（eval/questions.zh.json，期望值来自直接调用算子）
│   ├── src/design.rs         （v0.3.4）设计题循环与评分（eval/design/，不新增算子）
│   └── static/               index.html、app.js、style.css、vendor/plotly-basic（编译期嵌入，不走 CDN）
└── aeoi-wasm/                （v0.3.1）wasm-bindgen 导出（8.5）；examples/golden_dump.rs 与 parity/parity.mjs
pages/                        GitHub Pages 页面：index.html、browser.js（浏览器内 Agent 循环与 BYOK）、build.sh
```

依赖：nalgebra 0.33、serde_json（`preserve_order`、`float_roundtrip`）、jsonschema 0.18（`draft202012`）、sha2、base64。`aeoi-web` 另用 axum 0.8、tokio、reqwest 0.12（rustls + ring，无 OpenSSL/aws-lc，Windows MSVC 可直接构建）。schema 文件在编译期通过 `include_str!` 嵌入，与 `schemas/` 目录同源。

## 11.2 运行

```bash
cd ai-operator-spec/rust
cargo test --release                      # 全部测试（含 264 + 353 个 python-control golden 用例）
cargo test --release -p aeoi-control --test parity -- --nocapture      # v0.2 算子，写 reports/parity-python-control.json
cargo test --release -p aeoi-control --test parity_b2 -- --nocapture   # v0.3 算子，写 reports/parity-python-control-b2.json
cargo test --release -p aeoi-control --test parity_hp                  # lqr / lyap 对 50 位 mpmath 真值，写 reports/parity-mpmath.json
cargo build --release
./target/release/aeoi-call control.lti.bode ../examples/control.lti.bode/request.json
./target/release/aeoi-mcp [--data-root DIR] [--namespace control.lti]   # MCP stdio server
cd .. && python3 mcp_session_demo.py      # 脚本化 MCP 会话，记录到 reports/mcp-session.jsonl
python3 regen_examples.py                 # 用 Rust 输出重新生成 examples/*/response*.json
python3 gen_golden_pycontrol.py           # 重新生成 v0.2 算子的 python-control golden（需 python-control/scipy）
python3 gen_golden_pycontrol_b2.py        # v0.3 算子的 golden（seed 20261122）
python3 gen_golden_mpmath.py              # lqr 0.3.0 / lyap 0.4.0 的高精度 golden（mpmath 50 位，seed 20261123）
python3 make_examples_b2.py               # v0.3 算子的示例请求、python-control 参考值与 parity.yaml
python3 build_spec.py                     # spec-src.md → spec-v0.4.md（内联示例文件）
python3 validate_examples.py              # 独立的 Python jsonschema 校验
cd rust && cargo run --release -p aeoi-web                              # 网页测试台 http://127.0.0.1:8787/（需 GEMINI_API_KEY）
cargo run --release -p aeoi-web -- eval --model gemini:gemini-2.5-flash  # LLM 评测，写 reports/eval-<model>.json
cargo run --release -p aeoi-web -- design --model mock:design --engdesign ZC_02  # 设计评测，离线；见 8.4
cargo run --release -p aeoi-web -- tools --losses                       # 函数声明有损映射汇总
```

MCP 客户端配置示例：`{"command": "/path/to/aeoi-mcp", "args": ["--data-root", "/path/to/shared"]}`。

## 11.3 测试结果

当前结果不再写进规范：parity 报告在 `reports/parity-*.json`（`cargo test --workspace --release` 生成），一致性结果见 `conformance/README.md`，跨平台比较见发布流水线（`.github/workflows/release.yml`）与 Pages 工作流的 wasm parity 任务。v0.3.2 时的测试记录见 CHANGELOG。

## 11.4 已知限制

- **收据（v0.3.3）**：`poles_zeros` 与 `lyap` 没有声明后置条件，claim 为 `unchecked`（`lyap` 有残差，但没有与问题尺度无关的通过/不通过判据；`poles_zeros` 没有独立检查）；`exactness.class` 全部为 `approximate`，没有算子给出严格误差界；`verify` 对解不唯一的输出（多输入 `place` 的 K）不会改比不变量；收据没有签名（R-8）；`assumptions[].statement` 只有中文。

- ~~`lqr` 在 cond(R) ≥ 10⁸ 时比 scipy 不准~~（v0.3.2 / `lqr` 0.3.0 已修复：K 形式 Newton–Kleinman + 补偿残差，见 10.3；50 位真值下 cond(R) 到 1e12 的 S、K 误差 ≤ 4.5e-16，scipy 为 3.7e-9）。仍有的限制：Newton–Kleinman 从符号函数解出发，符号函数在极病态的大问题上可能不收敛（例：n = 200、m = 3 的随机问题，cond(X) ~1e15，0.2.0 与 0.3.0 都返回 `E.NUMERICAL.NO_CONVERGENCE`）；没有平衡的有序 Schur / 广义特征值路径作后备。`lqr` 0.3.0 在 n = 200 时约 4.1 s（0.2.0 为 3.7 s）。
- `lqr` 的 sep 误差估计在部分随机用例上很悲观（例：实际 3.6e-8，估计 2e-3）。
- ~~Newton–Kleinman 修正只在 n ≤ 32 时进行~~（0.3.0：Bartels–Stewart，n ≤ 200）。补偿算术（Dot2）的残差是 O(n³) 的标量循环，没有向量化；golden 中最大 n = 40，n = 100 / 200 只做了手工性能与收敛测试。
- 只实现了 stdio 传输；未实现 Streamable HTTP、PyO3、C ABI 绑定；credentials 资源未提供。
- 稀疏输入一律稠密化（`densify`），没有原生稀疏算法。
- golden 规模小于 v0.1 parity.yaml 的计划数量（实际 264 + 353 例），且未跑 MATLAB；`matlab/gen_golden_bode.m` 未在 MATLAB 上运行过，v0.3 算子还没有 MATLAB 生成脚本。
- **v0.3 算子的限制**：
  - 未实现 d2c（离散 → 连续，需要矩阵对数）；`c2d` 只接受连续系统。
  - 不做最小实现（minreal）/零极点对消；MIMO 转换得到非最小实现（有 warning）。zpk 只表示 SISO（R-8）。
  - `place` 用 KNV 方法 0 的复数变体，不是 scipy 的 YT 方法；golden 中 cond(X) 最多为 YT 的 1.08 倍，没有对极难问题做系统比较。
  - `margin` 只支持 SISO；恰在 Nyquist 频率的相位交越不报告（与 python-control 0.10.2 一致，D-6）；没有返回时滞裕度。
  - `dlqr` 的误差估计（‖残差‖/sep）是启发式的；没有像 `lqr` 那样做病态 R 的专门测试。
  - 不变零点在接近退化的系统上精度依赖约化时的秩判定；只有 2 个确定性点的法秩检查。
  - `lyap` 的 error_estimate 在 A 非正规时可能偏乐观；规模上限由 manifest `limits` 控制，未做大规模（n > 100）测试。0.4.0 的迭代精化只修正舍入误差，不改善问题本身的条件数：高度非正规的 A（tpt 交叉验证的 `lyap_c_nonnormal_rot_*`，条件数 1e12 以上）上本实现、tpt、scipy 的前向误差都很大；精化在 ρ = 1 − 1e-6 时把误差降到 4.3e-11，但仍未到 f64 舍入（触发阈值与步数上限是启发式的）。
  - `ctrb_obsv` 的 Krylov 矩阵在 n 较大时本身病态，秩判定以 PBH 交叉检查兜底（`W.NUMERICAL.RANK_AMBIGUOUS`）。
  - nalgebra 0.33 的实 Schur QR 没有特殊移位，个别矩阵会不收敛（实现中遇到：`margin` 的偶多项式伴随矩阵导致死循环）；参考实现用带迭代上限与确定性回退的 `schur_robust` 规避，回退路径只在少数矩阵上被触发过。
- **aeoi-web（v0.3.1）的限制**：
  - 评测只覆盖 29 道题；回答正确性用"文本中出现期望数值（相对 2%）"判定，可能把恰好出现的数字算作正确，也可能漏判用分数或根式表达的正确回答。
  - Gemini 免费额度很小（`gemini-2.5-flash` 每天 20 次请求），每题至少 2 次请求，一个模型一天跑不完全部题；评测遇到每日配额用尽时停止并在报告 `aborted` 中列出未运行的题。
  - 工具声明约 1.7 万 token/请求；未做按问题筛选工具。
  - OpenAI 兼容 provider 用真实 OpenRouter 跑过 3 个免费模型的 8 题子集；Groq、Ollama 只做了 Mock 层面的请求/响应格式测试。OpenRouter 免费账号每天 50 次请求（所有免费模型合计），许多免费模型经常因上游共享额度返回 429。
  - `_user_quote` 核对只能确认数值出现在用户原话里，不能确认模型摘取的是对的那个数（例如用户同时给了采样周期和仿真步长）；它只覆盖 `x-aeoi-provenance: "user"` 标出的参数（目前只有 `c2d.ts`）。
  - 模型文本不流式输出（轨迹按步骤用 SSE 推送）；会话只保存在内存；Plotly basic 2.35.2 已内置（`static/vendor/`，MIT），加载失败时退回内置的简易 SVG 图（无缩放、悬停）。
  - **设计评测（v0.3.4）**不新增算子：模型要自己做传递函数串联/反馈，没有 `pid`/`pidtune`、Padé、闭环 stepinfo 便利函数、MIMO 裕度。ControlEval 题库无许可证，不能随仓库分发。一次真实模型的试点见 `reports/design-pilot.md`。
- **浏览器版（8.5）的限制**：
  - wasm 2.97 MB + Plotly 1.07 MB（gzip 后约 1.4 MB），首次打开需要下载；没有 Service Worker 离线缓存。
  - 结果与原生不逐位相同（26% 用例末几位不同，多输入 `place` 的 K 不同；v0.3.2 起 lqr、lyap 全部逐位相同，不再有 warning 不一致），见 8.5。
  - key 存在 `localStorage`，同源的任何脚本都能读到——页面的源是该账号 Pages 的域名（`yuanqingfei.github.io`；该账号设了自定义域名，实际会 301 到 `https://yuanqingfei.me/aeoi/`），同一账号下其他仓库的 Pages 页面也属于这个源；不要在公用电脑上保存。没有服务端代理，provider 的速率限制按访问者自己的 key 计算；Gemini 免费额度很小。
  - 浏览器版没有评测 CLI；只测试了 Chrome（headless）与 node 24，没有测 Firefox/Safari。
  - Windows：已在 Linux 上交叉编译 `x86_64-pc-windows-gnu`（Rust 1.85.1 + mingw-w64）通过，得到 `aeoi-web.exe`；但没有在 Windows 机器上实际运行，MSVC 工具链未验证。
- 只在一台 x86_64-linux 机器上测试，跨平台（aarch64、无 AVX-512）的 `platform_bitwise` 行为未验证。
- b64/ref 编码只测试了 f64；c128/i64/bool 的二进制解码路径已实现但没有测试。`reject` 稀疏策略已定义但算子都未使用。

## 11.5 算子之间的复用（v0.4.11，作者决定 2026-11-25）

- RI-1 **一个算子用到另一个算子的计算时，参考实现 MUST 调用那个算子所基于的内核，也就是同一段代码。** MUST NOT 另写一份，也 MUST NOT 改调外部库里的同类功能。
  - 内核放在 `aeoi-core::kernels`（v0.4.17 起共 25 个，清单与依赖见 12.3）：`dense`（LU、Householder QR、Cholesky、1-范数条件数估计、补偿残差）、`svd`（单边 Jacobi）、`eig`（对称 Jacobi；Schur → 复 Schur → 三角回代）、`sum`、`schur`、`fft`、`filter`、`iir`、`lp`、`qp`、`bracket`、`newton`、`quad`、`rk`、`spline`、`riccati`、`freqresp`、`timeresp` 等。
  - 内核不依赖表达式语言：被积函数、右端、方程组都以闭包传入；取消与超时以检查闭包传入。这样控制层和数值层的算子都能调用同一个内核。
  - 规则同样适用于同一算子内部能被别的算子复用的部分：在第二个使用者出现之前就把它放进内核（v0.4.17 从现有算子里抽出 10 个）。
  - 对应的算子（`linalg.system.solve`、`linalg.decomp.svd`、`linalg.decomp.eig`）只是内核外面的一层：校验、输出格式、后置条件。
  - 理由：同一个计算出现两份实现，精度与行为就可能不一致，而 parity 和一致性套件只验证了其中一份。v0.4.11 正好有实例：nalgebra 的 SVD 和对称特征分解在分级矩阵上不准，被 `svd`、`eig` 的后置条件抓到；而控制算子当时还在用它们。
- RI-2 **一个算子自身的核心算法可以来自外部库**，例如 v0.4.15 之前 `fft` 用 rustfft、`eig` 的实 Schur 用 nalgebra，但 MUST 写在 manifest 的 `backend` 里，并受 2.1 M-7a（跨平台逐位相同）约束。外部库给出的功能一旦有了 AEOI 自己的算子，其他算子 MUST 改用该算子的内核（RI-1）。
- RI-3 **迁移**（已知违反项）：v0.4.11 审计时，控制算子与 `aeoi-core` 的 `linalg`、`sysalg` 里还有约 45 处直接调用 nalgebra 的 LU、SVD、Cholesky、对称特征分解或 QR（`bode`、`c2d`、`connect`、`ctrb_obsv`、`dlqr`、`lqr`、`lyap`、`place`、`poles_zeros`、`step`）。它们要逐一改用内核。结果的末几位会变：在容差类内按 G-3 处理，超出容差的按 G-2 升版本。**v0.4.16 已完成**：上述调用、`fft` 的 rustfft、`eig` 与 `lti` 的 nalgebra Schur 全部改为 `aeoi-core::kernels`（新增 `cdense`、`schur`、`fft`、`nalg` 适配层，`dense` 加了 Cholesky）。仍在用的外部数值代码只有：nalgebra 作矩阵容器与矩阵乘法（经打补丁的 `matrixmultiply`，固定通用内核）、`libm`（经 `aeoi-libm`）。检查办法：在 `rust/` 下搜 `.svd(`、`.lu()`、`try_inverse`、`.qr()`、`.cholesky()`、`symmetric_eigen`、`Schur::`，结果应为空。
