# AI Engineering Operator Interface Spec（AEOI）v0.4.23-draft

> 状态：Draft v0.4.23 · 2026-11-26 · 作者：Qingfei Yuan（草拟协助：Grok Bot、Claude）
>
> SPDX-License-Identifier: CC-BY-4.0 · © 2026 Qingfei Yuan。规范文本按 CC-BY-4.0 许可（`LICENSE-CC-BY-4.0`）；规范中的代码片段、JSON Schema 与参考实现按 MIT OR Apache-2.0 许可（`LICENSE-MIT`、`LICENSE-APACHE`）。
>
> 定位：叠加在 MCP 之上的“工程计算语义层”。MCP 负责传输与工具调用；AEOI 规定工程算子如何描述自己、如何表达带物理意义的输入输出、如何报告结果可信度、如何报错、如何复现、如何证明与参考实现一致。
>
> 冻结条件：v0.x 期间任何条款都可能被修改；102 个目标函数完成后发布 v0.5，经第二实现与真实使用反馈修订后冻结 v1.0。

**文档结构**（v0.4.18 起拆分；只读本文与 `profiles/` 就应能写出一个实现）

| 文档 | 内容 | 性质 |
|---|---|---|
| 本文 | 约定、manifest、类型、信封与收据、错误、可复现性、凭证、MCP 与绑定、治理与一致性、开放问题 | 规范性 |
| [`profiles/`](profiles/) | 第 10 章：每个算子一份契约（条款、示例、parity 规格） | 规范性 |
| [`docs/reference-impl.md`](docs/reference-impl.md) | Rust 参考实现：结构、运行、内核复用（RI-1 – RI-3）、各绑定的实现、评测结果 | 说明性 |
| [`CHANGELOG.md`](CHANGELOG.md) | 每个版本的起因、改动与结果；已解决的开放问题 | 说明性 |

## 0. 约定

- 关键词 MUST / MUST NOT / SHOULD / SHOULD NOT / MAY 按 RFC 2119 / RFC 8174 解释。
- JSON Schema 一律使用 Draft 2020-12。完整 schema 文件见 `schemas/`，`$id` 形如 `aeoi://schemas/common/0.4`。
- 本文中 `spec` 版本标识写作 `aeoi/0.4`（v0.3.x 为 `aeoi/0.3`）。
- **净室原则（normative）**：算子实现 MUST 仅依据公开文献、教科书、标准与开源许可兼容的代码；MUST NOT 反编译、反汇编、阅读或复制 MATLAB（或任何闭源软件）的实现代码、`.p` 文件、二进制或未公开文档。MATLAB 只作为黑盒 ground truth，通过其公开 API 生成对照数据。每个 manifest MUST 带 `provenance.clean_room.declared = true`。
- **与 MATLAB 的关系**：算子的命名与分类沿用 MATLAB Control System Toolbox 的**公开函数名**（`series`、`feedback`、`margin`、`stepinfo`、`pade`、`loopsens` 等），以便工程师和模型按熟悉的名字找到算子；定义与方程写在本规范里，不依赖任何 MATLAB 文档。到 v0.3.6 为止，**所有参考数据都来自开源实现**（python-control、scipy、mpmath），没有一条由 MATLAB 生成；`matlab/` 目录下的脚本只是留给有 MATLAB 许可的第三方复核用的生成器。
- **示例数值说明**：第 10 章（`profiles/`）示例的响应由 Rust 参考实现实际输出（`scripts/regen_examples.py`）；其数值另与 python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 在 box 上的计算结果比对（`cargo test`）。**两者都不是 MATLAB 输出。** `timing` 为 box 上的实测值，仅供参考。

> ### ⚠ 内存序（MUST，最常见的静默错误）
>
> **AEOI 的稠密数组一律是 row-major（C order）展平**：`shape [m,n]` 的矩阵，`data[i·n + j] = A(i,j)`。
> **MATLAB 与 Fortran 是 column-major。** 在 MATLAB 中写 `A(:)` 或 `reshape(A,1,[])` 得到的是列主序；对非对称矩阵这等于静默传入 Aᵀ —— schema 校验、shape 校验都无法发现，结果错误但"看起来正常"。
>
> - 从 MATLAB 生成 AEOI 数据 MUST 先转置再展平：`data = reshape(A.', 1, [])`；N 维数组用 `reshape(permute(X, ndims(X):-1:1), 1, [])`。
> - 从 NumPy 生成 MUST 使用 `A.ravel(order="C")`（或 `A.tolist()` 后按行展平）；Fortran 序数组（`A.flags.f_contiguous`）MUST 先 `np.ascontiguousarray`。
> - 二进制编码（`data_b64`、`MatrixRef`）同样是 row-major，`order` 字段 MUST 为 `"C"`（v0.2 不接受 `"F"`）。
> - 每个**输入里含有矩阵或状态空间**的算子，其 parity 测试集 MUST 至少包含一个**非对称矩阵**用例（case family `memory_order_nonsymmetric`），并 MUST 断言：把同一矩阵按 column-major 展平的请求，其结果与参考值的偏差远超容差（v0.3 参考实现 12 个算子实测为容差的 4.3×10⁸ – 1.0×10¹³ 倍；`margin` 的错误请求直接得到不同的交越点集合；`place` 的 MIMO 用例只能通过在测试中用参考请求重算 eig(A − BK) 来检出，见 V-5a）。对称矩阵（如 Q = I）无法暴露这类错误。输入里根本没有矩阵的算子（v0.3.5 的 `control.design.pid`、`control.lti.pade`）MUST 在 parity.yaml 写明 `memory_order_nonsymmetric: not_applicable` 以及原因，而不是造一个无关的矩阵参数。
> - 生成脚本 SHOULD 对每个矩阵做往返自检（见 `matlab/gen_golden_bode.m` 中的 `aeoi_dense_check`）。

## 1. 架构与术语

```
Agent ──MCP (JSON-RPC, stdio)──▶ MCP server ──▶ 注册表 ──▶ 算子（validate → execute → 信封）
LLM 函数调用绑定 ─────────────┤
浏览器（WebAssembly）─────────┤   所有入口共享同一 manifest、schema、信封、收据与错误码
CLI / 其他语言绑定 ──────────┘   参考实现：Rust（docs/reference-impl.md）
```

| 术语 | 含义 |
|---|---|
| Operator（算子） | 一个纯函数式的工程计算单元，如 `control.lti.bode` |
| Manifest | 算子的自描述文档（第 2 章） |
| Envelope（信封） | 每次调用的统一返回结构（第 4 章） |
| Trust metadata | 结果可信度信息：收敛、残差、条件数、误差估计、精度等级 |
| Golden file | 参考实现（MATLAB / python-control / 解析解）生成的对照数据 |
| Credential（验证凭证） | 某算子版本的 parity 测试记录与精度等级（第 7 章） |
| Data root | 宿主为 `file://` 引用配置的唯一可读目录（3.6、3.8） |

## 2. Operator Manifest

### 2.1 规则

- M-1 每个算子 MUST 提供一份符合 `schemas/manifest.schema.json` 的 manifest，并随二进制一起发布；MCP server MUST 能以资源 `aeoi://operators/{id}/{version}/manifest` 返回它（schema 已内联）。
- M-2 `id` MUST 为小写点分命名空间 `domain.subdomain.name`（2–4 级点号），如 `control.lti.bode`、`control.design.lqr`。一个 `id` 一旦发布，语义 MUST NOT 改变；语义改变需新 `id` 或主版本升级。
- M-3 `version` MUST 遵循 SemVer 2.0（见 9.2 的破坏性变更定义）。
- M-4 `applicability.preconditions` MUST 列出所有数学前提（如"R 对称正定"）。凡是可以机器检查的前提，`validate()` MUST 检查并在违反时返回对应 `E.VALIDATION.*` 或 `E.DOMAIN.*`。
- M-5 `provenance.references` MUST 至少一条可查证的公开文献（论文 SHOULD 带 DOI）。`provenance.algorithm` MUST 用一句话说明实际采用的算法（而非"调用某库"）。
- M-6 `backend` MUST 写明库名、例程名、版本、`binding`（`native` | `ffi`）与 SPDX 许可证。`binding`：算子的数值代码与实现用同一种语言写成并在同一进程内运行时写 `native`（纯 Rust、纯 Python 或 numpy 的逐元素运算都算），数值核心调用另一种语言编译的库（LAPACK、FFTW 等）时写 `ffi`。纯 Rust 实现写 `binding: "native"`，`routine` 写实现函数名（如 `lqr::solve_care`）。许可证 MUST 与平台分发方式兼容。
- M-7 `determinism` MUST 取以下之一：

| 值 | 含义 | 允许的场景 |
|---|---|---|
| `bitwise` | 同 build、同输入 ⇒ 任何机器上输出逐位相同 | 不依赖运行时 CPU 分派的实现 |
| `platform_bitwise` | 同 build、同"平台"逐位相同；跨平台只保证在容差类内 | 依赖 SIMD/FMA 或运行时 CPU 特性分派的内核 |
| `tolerance` | 同平台多次运行也只保证在容差类内 | 必须说明原因（如并行归约），L3 需评审 |
| `seeded` | 给定 `seed` 后满足 `bitwise` | 随机算法 |

  "平台" = 目标三元组（`reproducibility.platform`）+ 运行时检测到的 CPU 特性集合（`reproducibility.cpu_features`）+ 超越函数所用的 C 数学库及版本（`reproducibility.libm`，v0.4 起，例如 `glibc 2.39`；见下段）。WebAssembly 构建（`wasm32-unknown-unknown`，目标三元组里没有 OS）的 `platform` 记为 `wasm32-unknown`；它与原生构建是不同的"平台"：超越函数走 Rust `libm` 而不是 glibc，矩阵乘法没有 AVX/FMA 内核，所以 `platform_bitwise` 不延伸到 wasm ↔ 原生（实测见 8.5）。v0.3 参考实现的 12 个算子都声明 `platform_bitwise`：nalgebra 的稠密乘法走 `matrixmultiply`，后者在运行时按 AVX/FMA 等特性选择内核。

- M-7a **声明 `bitwise` 的前提**：超越函数 MUST 来自编入 build 的数学库（库名与版本记入 `reproducibility.libm` 和 `build`），MUST NOT 来自运行时的系统库，也 MUST NOT 在编译期用编译机的 C 库求值（常量折叠）；任何运算路径 MUST NOT 按运行时检测到的 CPU 特性分派。证据是在每个支持的平台上、用该平台上构建的二进制跑全部 golden 用例，`output_hash` 完全相同。理由：系统数学库各家舍入不同，而且会随系统升级改变（v0.4 实测 glibc 2.31 与 2.39 相差 1–2 ulp）。
- M-8 `tolerance_class` MUST 取 7.2 表中的一个值。
- M-9 **稀疏支持声明**：manifest MUST 含 `sparse_support`，以输入字段的 JSON Pointer 为键，值为 `{"policy": "native" | "densify" | "reject", "max_dense_elements": <int>}`。未列出的矩阵字段视为 `reject`。`densify` 时算子 MUST 返回 `W.PERFORMANCE.SPARSE_DENSIFIED`；稠密化后元素数超过 `max_dense_elements` MUST 返回 `E.RESOURCE.INPUT_TOO_LARGE`。
- M-10 **编码与上限**：manifest MUST 含 `encodings {input: [...], output: [...]}`（取值 `json`、`b64`、`ref`），以及 `limits`（算子特有的规模上限，如 `max_grid_points`、`max_states`）。上限检查 MUST 在主计算前完成。
- M-11 **方法登记**：`receipt`（4.3 R-1）的现有字段描述 `default` 方法；同一算子的其他计算方法登记在可选的 `receipt.alternatives[]`（3.14 MS-1）。没有 `alternatives` 的算子只有一个方法，manifest 与行为和 v0.3 完全相同。

### 2.2 片段（`control.design.lqr` 0.3.0 实际 manifest 节选）

```json
{
  "spec": "aeoi/0.4",
  "id": "control.design.lqr",
  "version": "0.3.0",
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "provenance": {
    "algorithm": "消去交叉项后构造 Hamiltonian 矩阵；虚轴检验在辛平衡后的 H̃ 上做并用 PBH 确认；带行列式缩放的 Newton 迭代计算矩阵符号函数，由 sign(H)+I 的零空间最小二乘解出 S；随后在原坐标做 K 形式 Newton–Kleinman 缺陷校正（补偿残差）",
    "references": [{ "type": "paper", "cite": "R. Byers, Solving the algebraic Riccati equation with the matrix sign function, LAA 85, 1987", "doi": "10.1016/0024-3795(87)90222-9" }, "…"],
    "clean_room": { "declared": true }
  },
  "backend": { "library": "aeoi-control (pure Rust) + nalgebra 0.33", "routine": "lqr::solve_care", "version": "0.2.0", "binding": "native", "license": "MIT OR Apache-2.0" },
  "sparse_support": { "/A": { "policy": "densify", "max_dense_elements": 1000000 }, "…": "…" },
  "encodings": { "input": ["json", "b64", "ref"], "output": ["json", "b64"] },
  "limits": { "max_states": 200, "max_states_for_refinement": 200, "max_sign_iterations": 100, "max_newton_iterations": 20 }
}
```

## 3. 带物理意义的类型化输入输出

### 3.1 数据类型

| 类型 | 编码 | 说明 |
|---|---|---|
| 实数标量 | JSON number，或 `{"dtype":"f64","value":x}` | IEEE 754 binary64 |
| 复数 | `[re, im]` | 不使用字符串 `"1+2i"` |
| 稠密数组（内联 JSON） | `{"dtype","shape","data"}` | `data` 为 **row-major (C order)** 展平一维数组 |
| 稠密数组（二进制内联） | `{"dtype","shape","order":"C","byte_order":"little","data_b64","sha256"}` | 3.6 |
| 稠密数组（外部引用） | `{"dtype","shape","order","byte_order","encoding":"raw","uri","sha256","byte_length"}` | 3.6 |
| 稀疏矩阵 | `{"format":"csr"|"csc","dtype","shape","indptr","indices","data"}` | 3.5 |
| 物理量 | `{"unit": "<UCUM>", "value": <标量或数组>}` | 见 3.2 |
| 多项式 | 实系数数组，**降幂** | `[1, 1.2, 4]` = s² + 1.2s + 4 |
| LTI 系统 | `{"kind":"tf"|"ss"|"zpk", ..., "ts", "time_unit"}` | `ts=0` 连续，`ts>0` 离散；`ts` 必填，`time_unit` 可省略（缺省 `"s"`，v0.4 只有这一个取值）；返回的系统总是写出两者（R-5） |

`schemas/common.schema.json` 中 `Matrix` = `oneOf [DenseMatrix, DenseMatrixB64, MatrixRef, SparseMatrix]`，四个分支以必填字段（`data` / `data_b64` / `uri` / `format`）互斥区分。

规则：
- T-1 矩阵 MUST 使用上表规范形式之一。绑定层 MAY 接受嵌套数组 `[[1,2],[3,4]]` 作为便捷输入，但 MUST 在校验前规范化；哈希只针对规范内容计算（3.7）。选择展平+shape 而非嵌套数组，是为了无歧义地表示空矩阵（如 `shape:[0,3]`）与高维数组，且与 Rust/NumPy 内存布局一致。
- T-2 `dtype` 取 `f64`、`c128`、`i64`、`bool`。v0.2 不支持 f32 输入。复数元素 MUST 为 `[re, im]`。
- T-3 元素个数 MUST 等于 `shape` 各维乘积（内联：`data.length`；二进制：解码字节数 = 元素数 × 元素字节数）；违反返回 `E.VALIDATION.SHAPE`。`dtype` 与算子要求不符返回 `E.VALIDATION.DTYPE`。
- T-4 FFI 到 Fortran 列主序库时，转置 MUST 在 backend 包装层完成，且 MUST 有往返单元测试。（另见第 0 章内存序规则。）
- T-5 LTI 表示：`tf` 支持 SISO（`num`/`den` 为一维多项式）与 MIMO（二维多项式数组）；`zpk` 中复数零极点 MUST 共轭成对，否则 `E.VALIDATION.NOT_CONJUGATE`（v0.4.21 起写明判定，PY-35：对每个值 c，容差 τ = 1e-12·max(1, |c|)；|Im c| ≤ τ 的值视为实数，不需要伙伴；其余每个值 MUST 有一个尚未配对的伙伴 w，|w − c̄| ≤ τ，一一配对）。SISO 与 MIMO 按维数区分：`num`/`den` 是 1×1 嵌套数组时按 SISO 处理，返回时用一维形式（PY-36）。tf/zpk 的系数与零极点在 schema 里是 `number`，非有限值在 schema 层就失败，得到 `E.VALIDATION.SCHEMA`；ss 的 `Matrix` 允许 `"NaN"` 等字符串，由 R-2 报 `E.VALIDATION.NON_FINITE`（PY-34：两条路径的码不同是有意的，修复方式相同）；`ss` 的 `A,B,C,D` 维数 MUST 一致。内部转换（如 tf→ss）MUST 在 trust 中注明所用 realization（v0.2：controllable canonical form，与 scipy `tf2ss` 约定一致；MIMO tf 为逐通道块对角实现）。
- T-6 **多项式前导零**：前导零 MUST 被去除并返回 `W.VALIDATION.LEADING_ZEROS_STRIPPED`（v0.1 规定为报错，实践中前导零常来自数值计算，报错对 Agent 不友好）。分母全零返回 `E.VALIDATION.ZERO_DENOMINATOR`。

### 3.2 单位

- U-1 单位字符串 MUST 使用 **UCUM 大小写敏感码（c/s）**：`s`、`ms`、`rad`、`rad/s`、`Hz`、`deg`、`m`、`N`、`N.m`、`kg.m2`、`%`、`1`（无量纲）。
- U-2 `dB` 作为本规范登记的扩展单位，定义为幅值比的 `20·log10(|x|)`；功率比 MUST 另用 `dB[pow]`。
- U-3 每个带物理意义的输入 MUST 在 schema 中声明允许的单位（`enum`）；算子 MAY 接受多个单位（如 `rad/s` 与 `Hz`）并在内部换算，输出 MUST 以规范单位返回并标注 `unit`（例：`bode` 输入 `Hz`，输出 `w` 为 `rad/s`）。
- U-4 不得静默假设单位：缺少 `unit` 的物理量 MUST 被拒绝（schema 层 `required`），而不是默认 SI。
- U-5 时间基准：LTI 系统 `time_unit` v0.2 固定为 `s`。

### 3.3 取值范围与 NaN/Inf

- R-1 范围约束（正数、区间、整数）MUST 写入 JSON Schema；跨字段约束（`min < max`、`w < π/ts`、频点严格递增）由 `validate()` 检查（`E.VALIDATION.RANGE` / `E.VALIDATION.GRID` / `E.DOMAIN.ABOVE_NYQUIST`）。
- R-2 JSON 无法表示 NaN/Inf。规范编码为字符串 `"NaN"`、`"Inf"`、`"-Inf"`，且仅在 schema 显式允许的位置出现。**输入默认拒绝非有限值**（`E.VALIDATION.NON_FINITE`），二进制编码中的 NaN/Inf 同样拒绝。
- R-3 输出中出现非有限值 MUST 使用 R-2 编码，并 SHOULD 附一条 warning 说明含义（例：频点恰为传递函数零点时幅值为 `-Inf` dB，附 `W.NUMERICAL.ZERO_ON_GRID`）。
- R-4 所有输入对象 MUST 设 `additionalProperties: false`：Agent 拼错参数名必须立刻得到错误，而不是被静默忽略。

### 3.4 内存序（MUST）

见第 0 章醒目规则。补充：
- O-1 对 N 维数组，row-major 即"最后一维变化最快"：`shape [d0,d1,d2]` 时 `data[(i·d1 + j)·d2 + k] = X(i,j,k)`。例：`bode` 输出 `magnitude` 的 `shape` 为 `[ny, nu, nw]`，频率维变化最快。
- O-2 MATLAB 侧对 `[ny,nu,nw]` 数组的正确展平是 `reshape(permute(X,[3 2 1]),1,[])`。
- O-3 实现 SHOULD 在错误提示中提醒内存序（v0.2 的 `E.DOMAIN.NOT_STABILIZABLE` 的 `agent_hint` 会提示"含行列顺序：row-major"），因为不可镇定、不稳定等"意外"结果常由转置错误引起。

### 3.5 稀疏矩阵

```json
{ "format": "csr", "dtype": "f64", "shape": [3, 3],
  "indptr": [0, 2, 3, 4], "indices": [0, 2, 1, 2], "data": [4.0, -1.0, 2.0, 5.0],
  "duplicates": "error", "structure": "general" }
```

- S-1 `format` ∈ `csr` | `csc`。CSR：`indptr` 长度 = m+1，第 i 行的非零元为 `indices[indptr[i]..indptr[i+1]]`（列号）；CSC 对称地按列。
- S-2 校验（失败均为 `E.VALIDATION.SPARSE_STRUCTURE`，`details` 指出违规位置）：`indptr[0] = 0`、单调不减、`indptr[last] = nnz = len(indices) = len(data)`；索引 MUST 在 `[0, n)`（CSR）或 `[0, m)`（CSC）内；每行（列）内索引 MUST **严格递增**（已排序）。
- S-3 重复项：`duplicates` 默认 `"error"`（同一位置出现两次即报错）；`"sum"` 时重复项相加（与 SciPy/MATLAB `sparse()` 一致），但索引仍须已排序。
- S-4 显式零 MAY 出现，计入 nnz，不影响数值。
- S-5 算子按 manifest `sparse_support`（M-9）处理：`native` 直接使用；`densify` 稠密化并返回 warning；`reject` 返回 `E.VALIDATION.DTYPE` 并在 remediation 中给出稠密化补丁。
- S-6 `structure`（`general`/`symmetric`/…）只是声明，算子 MUST NOT 据此跳过校验（例如 `lqr` 仍检查 Q 的对称性）。

### 3.6 大矩阵传输

| 编码 | 字段 | 适用规模（SHOULD） |
|---|---|---|
| `json` 内联 | `data` | ≤ 10⁴ 元素 |
| `b64` 二进制内联 | `data_b64` + `sha256` | 10⁴ – 10⁶ 元素（≤ 约 8 MB 原始数据） |
| `ref` 外部引用 | `uri` + `sha256` + `byte_length` | > 10⁶ 元素或 > 16 MB |

- L-1 二进制布局：f64/i64 为 8 字节**小端**，c128 为 re、im 交错的两个 f64，bool 为 1 字节 0/1；元素按 row-major 排列。`order` MUST 为 `"C"`，`byte_order` MUST 为 `"little"`（v0.2 只允许这一种，避免 Agent 侧组合爆炸）。
- L-2 `data_b64` 为 RFC 4648 标准字母表、带 padding 的 base64。`sha256` = `"sha256:" + hex(SHA-256(解码后的原始字节))`。服务端 MUST 校验摘要，不符返回 `E.VALIDATION.DIGEST_MISMATCH`。
- L-3 `MatrixRef.uri`：v0.2 服务端只解析 `file://` URI，且路径 MUST 位于宿主配置的 data root 之下；其他 scheme（含 `http(s)`）返回 `E.VALIDATION.REF_UNSUPPORTED`，越界路径返回 `E.VALIDATION.REF_FORBIDDEN`。`byte_length` MUST 等于文件实际长度与 dtype×shape 推导的长度。
- L-4 MCP 的 resources 是 server→client 方向的；客户端无法用 MCP resource 把大输入"上传"给 server，所以 v0.2 的大输入通道是 `b64` 与共享文件系统上的 `file://`。HTTP(S)/对象存储引用留待 v0.3（需要 SSRF 防护与凭据模型）。
- L-5 输出编码：`exec.output_encoding` ∈ `json`（默认）| `b64`。`b64` 时结果中的每个稠密数组替换为 `DenseMatrixB64`（含 `sha256`），标量与小型结构不变。`output_hash` 与编码无关（3.7）。
- L-6 服务端上限（参考实现默认值，宿主可调）：单个数组解码后 ≤ 64 MiB；内联 JSON 数组 ≤ 10⁶ 元素。超限返回 `E.RESOURCE.INPUT_TOO_LARGE`，remediation 建议换用 `ref`。

### 3.7 哈希与编码无关性

- H-1 计算 `input_hash` 前，每个数组 MUST 被替换为 `{"$array": {"dtype","shape","sparse": bool, "sha256"}}`，其中稠密数组的 `sha256` 是 row-major 小端原始缓冲区的 SHA-256（与 L-2 的 `sha256` 字段同值）。因此同一矩阵用 `json`、`b64`、`ref` 三种方式传入，`input_hash` 完全相同（参考实现有测试）。
- H-2 稀疏数组的摘要基于其规范 CSR 形式（排序、合并重复项后的 `indptr/indices/data`），`sparse: true`。**稀疏与稠密表示的同一矩阵 `input_hash` 不同**（有意如此：v0.2 不为哈希而稠密化大型稀疏矩阵）。
- H-3 `structure` 与 `duplicates` 等纯声明字段不进入数组摘要。

### 3.8 安全注意事项

- SEC-1 `file://` 解析 MUST 先规范化路径（解析 `..` 与符号链接，`canonicalize`），再判断是否位于 data root 之内；MUST NOT 只做字符串前缀比较。
- SEC-2 MUST 在读取/解码前检查声明大小（`byte_length`、`shape` 乘积 × 元素字节数）是否超过上限，防止内存耗尽；读取后再次核对实际长度。
- SEC-3 MUST 在使用数据前校验 `sha256`；摘要不符时 MUST NOT 返回任何基于该数据的计算结果。
- SEC-4 未配置 data root 时 MUST 拒绝所有 `ref` 输入。宿主 SHOULD 为每个会话使用独立的 data root。
- SEC-5 MUST NOT 跟随 `http(s)`、`ftp`、`smb` 等网络 URI（SSRF）；错误信息 MUST NOT 回显 data root 之外的文件内容或存在性细节。
- SEC-6 `shape` 乘积 MUST 用带溢出检查的整数运算。

### 3.9 通用类型 schema（节选，完整见 `schemas/common.schema.json`）

```json
"DenseMatrix": {
  "type": "object", "additionalProperties": false, "required": ["dtype", "shape", "data"],
  "description": "row-major (C order) 展平；MATLAB 是 column-major，生成时必须先转置再展平",
  "properties": {
    "dtype": { "enum": ["f64", "c128", "i64", "bool"] },
    "shape": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "minItems": 1, "maxItems": 8 },
    "data":  { "type": "array", "items": { "oneOf": [
      { "type": "number" }, { "$ref": "#/$defs/Complex" }, { "type": "boolean" }, { "$ref": "#/$defs/NonFinite" } ] } },
    "structure": { "$ref": "#/$defs/Structure" }
  }
},
"DenseMatrixB64": { "required": ["dtype", "shape", "order", "byte_order", "data_b64", "sha256"], "…": "…" },
"MatrixRef":      { "required": ["dtype", "shape", "order", "byte_order", "encoding", "uri", "sha256", "byte_length"], "…": "…" },
"SparseMatrix":   { "required": ["format", "dtype", "shape", "indptr", "indices", "data"], "…": "…" },
"Matrix": { "oneOf": [ {"$ref": "#/$defs/DenseMatrix"}, {"$ref": "#/$defs/DenseMatrixB64"}, {"$ref": "#/$defs/MatrixRef"}, {"$ref": "#/$defs/SparseMatrix"} ] }
```

**引用写法（规范性）**

- J-1 schema 里的 `$ref` MUST 是标准 JSON Schema 2020-12 引用：只按 `$id` 与 JSON Pointer 就能解析，不需要任何 AEOI 专用规则。同一文件内写 `#/$defs/<Name>`；引用核心 schema（`schemas/*.schema.json`）的定义写 `<$id>#/$defs/<Name>`，例如 `{"$ref": "aeoi://schemas/common/0.4#/$defs/Matrix"}`。v0.4 及以前用过的简写 `aeoi://schemas/common/0.4/Matrix`（把定义名当作 `$id` 下的路径）不是合法引用：实现 MUST NOT 在发布的 schema 里使用它，消费者与一致性套件按无法解析处理（9.4）。
- J-2 引用核心 schema 时，`$id` 中的版本 MUST 与 manifest 的 `spec` 一致（`aeoi/0.4` 对应 `aeoi://schemas/<name>/0.4`）。
- J-3 J-1 管的是独立发布的 schema 文件（如 `examples/<id>/input.schema.json`）。M-1 要求通过 MCP 资源返回的 manifest 是内联后的形式，MCP 工具的 `inputSchema`、`outputSchema` 同样 SHOULD 内联，不含指向其他文档的 `$ref`：多数 MCP 客户端不会去取外部 schema。

### 3.10 多输出结果、系统输出与离散时间约定

实现第二批算子时发现 v0.2 没有规定以下内容，各算子各写各的会让 Agent 无法泛化。以下条款为规范性要求。

**多输出结果**
- R-1 结果是一个对象，每个输出是一个具名字段（如 `poles`、`zeros`、`dcgain`；`gain_margin`、`phase_margin`、`crossings`），MUST NOT 用位置数组表示多个输出。带单位的标量/数组用 `{unit, value}`；无量纲矩阵直接用 `Matrix`；复数集合用 `c128` 向量。
- R-2 复数集合（极点、零点、闭环极点）的顺序没有规范意义：参考实现按实部升序、虚部降序输出，但消费者与 parity 测试 MUST 按集合匹配比较（V-5）。
- R-3 "不存在"的标量用 JSON 字符串 `"Inf"`/`"NaN"` 并在 schema 中允许（如无相位交越时 `gain_margin = "Inf"`、对应频率 `"NaN"`），MUST NOT 用 `null` 表达"无穷大"。`null` 只表示"未计算/不适用"（如不稳定系统的 Gramian，伴随 `W.DOMAIN.GRAMIAN_UNDEFINED`）。
- R-4 选择一个"代表值"的输出（如多个交越中的 GM/PM）MUST 同时返回全部候选（`crossings.phase[]`、`crossings.gain[]`、`crossings.stability[]`），并在 10.x 写明选择规则。

**返回系统的算子**
- R-5 返回 LTI 系统的算子（`convert`、`c2d`）MUST 在 `result.system` 中返回完整的 `LtiSystem`（含 `ts`、`time_unit`），可直接作为下一个算子的输入。MIMO tf 用嵌套数组 `num[i][j]`、`den[i][j]`（`[ny][nu]`）。
- R-6 状态空间实现不唯一。返回 ss 的算子 MUST 用 `realization` 字段（枚举）说明所用实现：`identity`、`controllable_canonical`（scipy `tf2ss` 约定：A 第一行为 −a₁…−aₙ，B = e₁；zpk→ss 先乘积展开成 tf 再用它，PY-38）、`block_diagonal_controllable_canonical`（MIMO tf 逐元素；块按 (i, j) 行主序，块 (i, j) 只占 B 的第 j 列、C 的第 i 行，10.4 CV-2）、`ss2tf_characteristic_polynomial`、`expanded_product`、`polynomial_roots`、`invariant_zeros_eigenvalues_markov_gain`，以及 v0.3.5 的 `polynomial_product`、`polynomial_sum`、`polynomial_feedback`、`series_block`、`parallel_block`、`feedback_block`、`pid_closed_form`、`pade_coefficients`。v0.3 不做最小实现（minreal）；MIMO 转换得到非最小实现时 MUST 给 `W.DOMAIN.NONMINIMAL_REALIZATION`：MIMO tf→ss 与 MIMO ss→tf（共享分母）都属于这种情形（10.4 CV-3）。
- R-7 ss→tf 约定（与 scipy `ss2tf` 一致）：den = det(sI − A)（由特征值展开），numᵢⱼ = det(sI − A + bⱼcᵢ) + (dᵢⱼ − 1)·den；所有元素共享同一分母；首项系数精确置为 dᵢⱼ 后去除前导零；`normalize = true`（默认）时分母为首一。
- R-8 zpk 在 v0.3 只表示 SISO；MIMO → zpk MUST 返回 `E.DOMAIN.MIMO_NOT_SUPPORTED`（remediation 建议 `to: "tf"` 或逐通道调用）。ss→zpk 的增益 k = C·A^{r−1}·B，r = #极点 − #零点（零点个数来自 R-9 的约化，不对 Markov 参数本身做“是否为零”的判断；r = 0 时 k = D），不经多项式。v0.4.21 写明（PY-41）：此前写“首个非零 Markov 参数”，在浮点下需要一个阈值，规范没有给。
- R-9 零点 = 不变零点（Rosenbrock 系统矩阵降秩的有限 s）。返回零点集合的算子（`poles_zeros`）在系统矩阵不是满法秩（例如传递函数恒为零）时，零点无定义，MUST 返回 `E.DOMAIN.DEGENERATE_SYSTEM`，MUST NOT 返回空集合冒充"没有零点"。`convert` 返回的是表示而不是零点集合，SISO 的 G ≡ 0 写成 k = 0 的 zpk（10.4 CV-5，PY-42）。法秩判定（v0.4.21 写明）：在 s_k = (1 + ‖A‖_F)·(0.6180339887 + 1.2360679775·k·j)，k = 1, 2 两点上计算 G(s_k) = C(s_kI − A)⁻¹B + D 的奇异值，两点上都有 σ_min ≤ 1e-11·(‖D‖_F + ‖C‖_F·‖(s_kI − A)⁻¹B‖_F) 才判为不满秩。零点的求法：对 (A, B, C, D) 及其对偶各做一次秩揭示约化（Emami-Naeini – Van Dooren；秩判定容差 10·(n + max(m, p))·u·‖[A B; C D]‖_F），使约化后的 D_r 方且可逆，零点 = eig(A_r − B_r·D_r⁻¹·C_r)；约化得不到方的可逆 D_r 时同样返回 `DEGENERATE_SYSTEM`。

**离散时间**
- D-1 `ts > 0` 为离散系统；频率一律 rad/s，离散系统的有效频段为 [0, π/ts]。
- D-2 DC 增益：连续在 s = 0，离散在 z = 1；未对消的极点给 `±Inf` + `W.DOMAIN.INFINITE_DC_GAIN`，0/0 给 `NaN` + `W.DOMAIN.UNDEFINED_DC_GAIN`。稳定性：连续 Re λ < 0，离散 |λ| < 1。
- D-3 离散脉冲响应的幅值约定 MUST 显式：`discrete_scaling = "unit_area"`（默认，u₀ = 1/ts，与 python-control 一致，使离散脉冲响应逼近连续冲激响应）或 `"unit_pulse"`（u₀ = 1，即 Kronecker δ）。
- D-4 连续冲激响应 y(t) = C·e^{At}·B；D·δ(t) 项无法采样，MUST 去掉并在 `result.direct_feedthrough` 中返回 D，同时给 `W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED`。
- D-5 离散化公式（与 scipy `cont2discrete` 一致，状态坐标相同）：zoh 为增广矩阵指数；foh 为 expm([[A·T, B·T, 0], [0, 0, I], [0, 0, 0]]) 的块组合（Ad = Φ, Bd = Γ₁ − Γ₂ + Φ·Γ₂, Cd = C, Dd = D + C·Γ₂）；tustin 取 α = T_w/2：Ad = (I−αA)⁻¹(I+αA)，Bd = (I−αA)⁻¹B·T_w，Cd = C(I−αA)⁻¹，Dd = D + ½·C·Bd。预畸变时 T_w = 2·tan(ω_p·T/2)/ω_p，否则 T_w = T。
- D-6 离散 `margin`：用双线性映射 z = (1+w)/(1−w)（ν = tan(ω·ts/2)）把单位圆上的交越问题化为实多项式求根；相位交越只在 θ = ω·ts ∈ [0, π) 上搜索（与 python-control 0.10.2 一致，恰在 Nyquist 频率的交越不报告，见 11.4）；稳定裕度取 |1 + G(e^{jθ})| 在 [0, π] 上的最小值，端点 θ = 0、π（对实系数系统总是驻点）是局部极小时也作为候选。

**选项枚举**
- O-1 改变数学定义的选项 MUST 是 input schema 中带默认值的枚举，并在结果中回显（如 `c2d.method ∈ {zoh, foh, tustin}`、`lyap.type ∈ {continuous, discrete}`、`convert.to ∈ {tf, ss, zpk}`、`impulse.discrete_scaling ∈ {unit_area, unit_pulse}`、`c2d.output_kind ∈ {same, tf, ss, zpk}`）。
- O-2 互相矛盾的选项组合 MUST 返回 `E.VALIDATION.OPTION_CONFLICT`，remediation 给出去掉冲突项的 patch（例：`prewarp_frequency` 只对 `tustin` 有意义）。MUST NOT 静默忽略选项。
- O-3 方程约定 MUST 写清符号：`lyap` 连续为 A X + X Aᵀ + Q = 0，离散为 A X Aᵀ − X + Q = 0（= python-control `lyap(A, Q)` / `dlyap(A, Q)`，= scipy `solve_continuous_lyapunov(A, −Q)` / `solve_discrete_lyapunov(A, Q)`）；`dlqr` 的 u = −Kx，代价 Σ xᵀQx + uᵀRu + 2xᵀNu。

### 3.11 面向 LLM 函数调用的可消费性

用云端 LLM（Gemini 函数调用）端到端调用 12 个算子时发现：规范里对"人/程序"足够的信息，模型不一定看得到——函数调用接口只接受 JSON Schema 的子集，未知关键字会被丢弃；`oneOf` 校验失败时 jsonschema 只给出"不符合任何分支"，模型无从修正。以下条款为规范性要求（F = function-calling；v0.3.1-draft 早期文本曾记作 M-1 – M-6，与第 2 章 manifest 规则 M-1 – M-6 重号，已改名）：

- F-1 input schema 的每个顶层属性 MUST 有 `description`，写明物理含义、维数（如 `[n,m]`）、单位与默认值；枚举 SHOULD 逐值说明含义（如 `c2d.method`、`lyap.type` 写出方程）。参考实现由 `scripts/write_manifests.py` 统一补齐。
- F-2 只存在于扩展关键字中的约束（`x-aeoi-shape-constraints` 等）MUST 同时写进 description，因为函数调用子集会丢弃未知关键字。
- F-3 `E.VALIDATION.SCHEMA` 的 `details.violations` MUST 是叶子级违规：`oneOf` 失败时 MUST 按判别字段（`kind`、`mode` 等字符串常量）选出调用方意图的分支，判别字段缺失或取值无效时直接报告该字段及可选值；无判别字段时取违规最少的分支递归诊断。MUST NOT 只返回"is not valid under any of the schemas listed in the 'oneOf' keyword"。`message` SHOULD 带上第一条违规原文。
- F-4 无损的表示修正 SHOULD 作为 `modify_input` remediation 给出，`confidence: "high"`、`requires_user_confirmation: false`：参考实现把矩形嵌套数组 `[[…],[…]]` 改写为 row-major `DenseMatrix`（数值不变，补丁 id `nested_array_to_dense`）。有歧义的修正（一维数组是行向量还是列向量）MUST NOT 自动给补丁。
- F-5 **不得编造有物理含义的输入。** 有物理含义且无法安全默认的输入（采样周期 `ts`、频率单位 `rad/s`/`Hz` 等）MUST 是必填字段，缺失时只给 `ask_user`。具体要求：
  - (a) 算子 MUST NOT 为它们设默认值；input schema 中 MUST 用 `"x-aeoi-provenance": "user"` 标出（v0.3.1：`control.lti.c2d` 的 `ts`），其 description MUST 写明"必须由用户给出；用户没给时先询问，不得假设"。
  - (b) 绑定层与系统提示词 MUST NOT 代填或"补上常用值"（如 0.1 s）；LLM 看到的工具描述 MUST 列出这些参数并写明"用户没给时先询问，不要调用"，系统提示词 MUST 有同样的规定。
  - (c) 绑定层 SHOULD 要求模型为每个 `x-aeoi-provenance: "user"` 参数给出来源引用（参考实现：额外的必填参数 `_user_quote: {ts: "<用户原话片段>"}`），并核对：引用逐字出现在用户本人的消息中（不含绑定层注入的提示），且引用里含有同一数值（允许 ms/μs 换算）。不满足时 MUST 不调用算子，返回 `E.VALIDATION.UNGROUNDED_INPUT`，remediation 只有 `ask_user`；`_user_quote` 在调用算子前移除，算子 schema 不变。
  - (d) 这一检查只能拦住"没有来源的数值"，拦不住模型从用户原话里错误地摘取数值；评测（8.4）对此单独设陷阱题。
- F-6 绑定层把算子暴露给 LLM 时 MAY 收窄输入面（只保留 `DenseMatrix` 编码，隐藏 `exec`、`time_unit`、`*_names` 等宿主侧或可选字段），MUST 公开有损映射清单（8.4），算子 MUST 仍按完整 input schema 校验。回传给模型的结果 MAY 对长数组抽样并用 `_downsampled` 标注原始形状与保留的下标；展示给用户的信封 MUST 保持原样（B-1）。
- F-7 **三种来源，不要都当成“必须引用用户原话”。** `x-aeoi-provenance` 取下列之一；缺省（不写该关键字）表示普通必填或可选字段，绑定层不附加 `_user_quote`。
  - `"user"`：测量值，没有安全默认。规则即 F-5（引用核对、缺失只给 `ask_user`）。现在的 `user` 字段：`c2d` 的 `ts`、`fft` 与 `butter` 的 `sample_rate_hz`、`butter` 的截止频率、`pidtune` 的 `wc_rad_s`、`trapz` 的 `dx`。问题里给定、但不是测量值的数（积分限、求根区间、初值、参数、优化的约束数据、阶数）是 `supplied`；有缺省值的字段 MUST NOT 是 `user`（F-5(a)），积分变量名之类是 `convention`。
  - `"convention"`：指标或互联的**定义参数**，规范给出默认值，默认值是算子语义的一部分。省略时用默认，MUST 在结果里回显实际使用的值；覆盖时回显覆盖值。绑定层 MUST NOT 要求 `_user_quote`，系统提示词 MUST NOT 把用默认值说成“假设了用户没给的物理量”。v0.3.5：`feedback.sign` 默认 −1，`stepinfo.rise_time_limits` 默认 `[0.1, 0.9]`，`stepinfo.settling_threshold` 默认 `0.02`，PID 的 `form` 默认 `parallel`，`Ki`/`Kd`/`Td`/`Tf` 默认 0（该项不出现）。
  - `"supplied"`：没有安全默认、但**不是**必须从用户原话逐字摘出的数。调用方必须给出；缺失是 schema 错误，remediation 只有 `ask_user`。绑定层 MUST NOT 加 `_user_quote`：这个数可以是设计选择（PID 的 `Kp`、Padé 的 `order`），也可以是从题目里别的数算出来的（Padé 的 `delay`，例如由 t2−τ 得到，原话里没有这个数）。系统提示词 MUST 写明：不得填“常用值”；用户和题目都没给出、也推不出来时先问。设计评测没有用户可问时，评测提示词 MAY 宣布一个回退（例如“order 没写就用 10 并在说明里声明”）；这个回退写在提示词里，MUST NOT 写进算子的默认值。

- F-8 **互斥选项的冲突由算子报告。** EngDesign 运行里，XG_12 先在 `form=standard` 下夹带 `Ki`/`Kd`，改成 parallel 后又写了 `Ti: 0`，得到的是 schema 的原话“0 is less than or equal to the minimum of 0”，前 4 步才得到一个控制器；XG_02 也因多余的 `Ti=0` 浪费了一回合。规则：
  - (a) “某组字段只属于某个模式”（PID 的 parallel 与 standard、`series`/`feedback` 的具名角色与位置参数）MUST 由算子的 validate 检查，MUST NOT 只靠 `oneOf`/`not` 这类 schema 关键字：函数调用子集会丢掉它们（8.4 表），校验失败时模型也看不出是哪两个字段冲突。冲突 MUST 报 `E.VALIDATION.OPTION_CONFLICT`，`details` 给出 `conflicting_fields`、生效的模式（例如 `form`）以及该模式是否是调用方写的（`form_given`）。
  - (b) remediation MUST 是可以直接套用的 JSON Patch：`remove` 只针对请求里**确实出现**的字段（不得对不存在的路径 remove，否则套用补丁本身就报错）；需要改模式时用 `add`/`replace` 写模式字段。只有当另一个模式下所有字段都合法时才给“改模式”补丁（例如 `Ti ≤ 0` 时不给“改成 standard”）。两种做法都要猜调用方意图时，`confidence` 为 `medium`，并 MUST 另给 `ask_user`。
  - (c) 有物理含义的禁止值 MUST 用一句说明原因的话报错，而不是 schema 的比较句：`Ti = 0` 的含义是“没有积分项”，参考实现报 `OPTION_CONFLICT`，消息写明“没有积分项就省略 Ti，不要写 0”，补丁 `omit_integral` 去掉 `Ti`；负的 `Ti`/`Td`/`Tf` 报 `E.VALIDATION.RANGE`。因此 `pid` 0.4.0 的 input schema 去掉了 `Ti` 的 `exclusiveMinimum`，改由算子检查（G-2 MINOR：接受的输入集合不变，只是错误更具体）。
  - (d) 工具描述（F-1）MUST 在每个只属于某一模式的字段上写“只用于 form = …”，并写明“不需要的项直接省略，不要用 0 占位”。

### 3.12 系统的组合、跨调用与近似

第二批算子能分析一个已经写好的 LTI 系统，但不能把它和另一个系统接起来。设计任务因此要手乘多项式。把“接起来”做成算子时，v0.3.4 有五处写不干净，下面是规范性要求（I = interconnection）。

**返回的系统就是下一个调用的输入**
- I-1 返回 LTI 系统的算子 MUST 把完整的 `LtiSystem` 放在 `result.system`（R-5）。它 MUST 能原样作为任何接受 `LtiSystem` 的算子的输入，包括另一个组合算子。MUST NOT 返回句柄或“对象 id”。
- I-2 `result.composition` MUST 给出 `operation`，以及每个操作数的 `sha256`（对该操作数在**本次请求里出现的 JSON 值**做 JCS-SHA-256，与 input_hash 同一套规范化）。不是系统的构造（`pid`、`pade`）改为 `parameters_sha256`，对回显的参数对象做同样的哈希。`minimal` 为 null：v0.3.5 不对消公共因子（与 `convert` 相同）。零增益把 PID 的某一项整个去掉，是定义的一部分，不是 minreal。
- I-3 **收据不嵌套。** 下游收据的 `fingerprint.request` 已经绑住操作数的全部系数，这就是组合的机器可核验来源。可选输入 `lineage`（上游 `receipt.fingerprint.request` 的字符串数组）MUST 原样回显到 `composition.lineage`，`lineage_checked` MUST 为 false。算子 MUST NOT 核验 lineage，也 MUST NOT 因为它缺失而报错。收据的 `assumptions` 仍是 manifest 里的静态列表（校验器逐项比对），不得在运行时把 lineage 塞进 assumptions。
- I-4 两个操作数的 `ts` MUST 完全相等（解析后的 f64 相等，没有容差）。连续与离散、或两个不同的采样周期 → `E.DOMAIN.SAMPLE_TIME_MISMATCH`，remediation 只有 `ask_user`（采样周期仍是 F-5 的 `user` 字段，组合算子不得挑一个）。
- I-5 反馈的方程是规范，不依赖任何闭源文档。`sign` ∈ {+1, −1}，默认 −1：
  - u₁ = u + sign·y₂，y₁ = sys1(u₁)，u₂ = y₁，y₂ = sys2(u₂)，y = y₁。
  - SISO 有理式：num = conv(num₁, den₂)，den = conv(den₁, den₂) − sign·conv(num₁, num₂)。
  - 省略 sys2 表示与 sys1 输出同维的单位增益。sys1 不是方的 → `E.VALIDATION.SHAPE`。
  - 串联是 sys2·sys1（sys1 的输出进 sys2）。并联是 sys1+sys2。
- I-6 实现约定：SISO 且两个操作数都是 tf 或 zpk 时用多项式（zpk 先展开），`realization` 为 `polynomial_product` / `polynomial_sum` / `polynomial_feedback`。任一操作数是 ss、系统是 MIMO，或 `output_kind` 为 `ss` 时用块公式，`realization` 为 `series_block` / `parallel_block` / `feedback_block`（sys1 的状态在前）。`output_kind = tf` 且走了块公式时，再按 R-7 做 ss→tf，`realization` 为 `ss2tf_characteristic_polynomial`，MIMO 时给 `W.DOMAIN.NONMINIMAL_REALIZATION`。
- I-7 **improper 操作数。** 组合算子和 `pid` MAY 接受相对阶 1…`limits.max_relative_degree`（参考实现为 8）的 tf/zpk，因为理想微分是合法的控制器项。分析算子（step、bode、margin、c2d、stepinfo）仍拒绝 improper。结果 improper 时 MUST 设 `proper: false` 并给 `W.DOMAIN.IMPROPER_RESULT`。要求 ss 实现却遇到 improper → `E.DOMAIN.IMPROPER_SYSTEM`，remediation 至少包含“改为 tf”（无损，若多项式路径存在）和 `ask_user`（滤波时间常数由调用方给，不得代填）。
- I-8 代数环 E = I − sign·D₂·D₁。rcond(E) ≤ n·u·10⁴（u 为单位舍入）或分母多项式化成 0 → `E.DOMAIN.ALGEBRAIC_LOOP`。1e-8 > rcond > 该阈值时仍返回结果，并给 `W.NUMERICAL.ILL_CONDITIONED`。判据对 E 的尺度不变。
- I-9 成功结果 MUST 有一个后置条件，在若干不靠近极点的测试点上核对互联恒等式（串联 G = G₂G₁，并联 G = G₁+G₂，反馈 (I − sign·G₁G₂)G = G₁），相对残差写入 `trust.residual`。残差超过 1e-5 则该检查为 false，收据 claim 为 `unverified`（E-11）。

**阶跃指标的容差不是积分容差**
- I-10 `control.lti.stepinfo` 的采样与 `control.lti.step` 相同（连续系统是 ZOH 下的精确递推，容差类 T2 描述的是这条轨迹）。上升时间、调节时间、峰值时间是**网格横坐标**：第一个使 sgn·(y − 阈值·y∞) ≥ 0 的采样，调节时间是最后一次 |y/y∞ − 1| ≥ 阈值 的下一格。不插值。与连续时间穿越点之差 ≤ dt，MUST 写入 `error_estimate.time_quantization_s`，MUST NOT 把它算进 T2 的 rtol。parity 在同一网格、同一阈值下比较，时间量的容差是该定义下的相等（参考实现用绝对 1e-8 吸收舍入，不吸收差一格）。
- I-11 阈值是 F-7 的 `convention`，MUST 在 `result.definition` 回显。稳态值取 DC 增益，不取最后一个采样。超调、欠调的单位是百分数。
- I-12 不是渐近稳定（连续有 Re λ ≥ 0，离散有 |λ| ≥ 1）：依赖 y∞ 的字段（上升、调节、超调、欠调、稳态）为 null，`peak` / `peak_time` 仍按窗口给出，`window_limited = true`，警告 `W.DOMAIN.UNSTABLE_SYSTEM`。渐近稳定但 t_final 内没有留在调节带里：`settling_time` 为 null，警告 `W.DOMAIN.NOT_SETTLED`。这不是 `"Inf"`——窗口太短只说明没观察到，不说明永不调节。DC 增益为 0：上升/调节/超调为 null，警告 `W.DOMAIN.ZERO_DC_GAIN`。DC 增益为无穷且其余极点稳定到算法仍给出无穷（与 step 相同的判定）：`steady_state` 为 `"Inf"`。积分器的极点在虚轴上，按 I-12 的第一条（不是渐近稳定）处理，稳态为 null，峰值取窗口。

**近似不是舍入**
- A-1 结果是某个命名数学对象的近似（不是该对象在 f64 里的舍入）时，MUST 在 `result.approximation` 给出：`target`、`method`、匹配的泰勒阶、下一阶系数、一个写明网格的 `certificate`（计算值），以及 `bound`。没有在规范里证明的严格上界时 `bound` MUST 为 null，`bound_kind` 为 `none`。MUST NOT 把逼近误差填进“系数算得有多准”的那一项。
- A-2 收据 `exactness.bound` 只抄系数（或轨迹）的舍入界。`trust.approximation` 若存在，收据 `evidence.approximation` MUST 抄过去；没有近似的算子 MUST NOT 出现这个字段（这样已发布收据的字节不变）。`evidence.approximation` 在信封 schema 里是可选的。
- A-3 `control.lti.pade` 的对象是 e^{−θs} 的 [N/N] Padé 近似。系数用与 Golub & Van Loan《Matrix Computations》第 3 版算法 11.3.1 等价的递推，但每一步已经对分母首项归一，避免未归一乘积在 N ≳ 10 时冲掉 f64 的有效位。`delay` 与 `order` 都是 F-7 的 `supplied`，没有默认值。容差类 T1 只约束系数；certificate 是虚轴 32 点上 |e^{−jωθ} − R(jω)| 的最大值，ω_k = θ⁻¹·10^{−2+4k/31}，k = 0…31。

**还没写成算子的整定**
- A-4 不在 v0.3.5 提供 `pidtune`。一个能写进规范的整定算子必须事先声明：对象类（例如已经化成的一阶加时滞）、被最小化或被满足的指标、解的唯一性、以及失败时的错误码。缺任何一条，算法就只是“某个实现的默认行为”，不能当算子。v0.4.6 的 `control.design.pidtune` 按 10.16 PT-1 – PT-6 满足这四项。

### 3.13 回路的角色、一次性分析与句柄

EngDesign 真模型运行（`reports/design-live.md`，7 题）里，模型调用了正确的算子却得到错误的系统：XG_13 用 `feedback(sys1 = 对象, sys2 = 控制器)` 把控制器放进了反馈通路（得到 G/(1+CG)），又在闭环后多串了 1/s；Ziheng_03 把闭环 T 当成开环 L 送进 `margin`，并把因式乘积 (0.5s+1)(s+1)(2s+1) 心算展开错了；XG_11 手拼越来越高阶的分母，丢掉了对象里的 169；XG_02 要求约 85 Hz 的带宽，却没有办法读 plant×controller 的带宽，交了一个带宽为 0 的增益。工具本身都是对的，接口让这些错误很容易犯。下面是规范性要求（K = loop）。

- K-1 **按角色命名。** 组合算子 MUST 接受具名角色：`series(controller, plant)` 得到 L₀ = plant·controller；`feedback(plant, controller, sensor)` 得到 T = G·C/(1 + H·G·C)，`controller` 永远在前向通路，`sensor` 是反馈通路。`controller` 省略 = 单位增益（`composition.controller_implicit = true`），`sensor` 省略 = 单位反馈（与 I-5 的“省略 sys2”相同）。位置参数 `sys1`/`sys2` 为兼容保留，语义按 I-5 不变；**两种写法在同一个请求里混用 MUST 报 `E.VALIDATION.OPTION_CONFLICT`**（F-8），remediation 给出“只保留具名角色”和“只保留位置参数”两个补丁。角色名只在 SISO/MIMO 的方向上有意义，不改变数学：`feedback(plant=G, controller=C, sensor=H)` 与 `feedback(sys1=series(C,G), sys2=H)` 逐位相同。
- K-2 **拓扑是固定的。** 单回路负反馈：r → e = r − H·y → C → G → y。正反馈仍用 `sign = +1`。具名调用 MUST 在 `composition.topology` 写出这条拓扑文字，以便模型和读者核对。
- K-3 **开环与闭环必须标出来。** 具名调用的结果 MUST 有 `composition.loop_role`：`series` 为 `open_loop`，`feedback` 为 `closed_loop`；位置参数调用不写这个键（保持 v0.3.5 的字节）。工具描述（F-1）MUST 说明哪个结果是 L、哪个是 T，以及“`margin` 只接受 L；`stepinfo`/`step` 用 T”。
- K-4 **一次性回路分析 `control.analysis.loopsens`。** 输入 `plant`（必填）、`controller`、`sensor`（可省略，意义同 K-1）、可选 `time`（给出时做 T 的阶跃指标）、`rise_time_limits`、`settling_threshold`、`bandwidth_drop_db`（F-7 `convention`，默认 −3）。输出 `open_loop` = H·G·C、`closed_loop` = T、`sensitivity` = 1/(1+L)、闭环极点与 `closed_loop_stable`、L 的 `margins`、`step_metrics`。它 MUST 等于把原语串起来（K-1 的 `series`/`feedback` → `margin(L)` → `stepinfo(T)`）：参考实现内部直接调用这些算子（DynOperator），golden 用 python-control 的 `series`/`feedback`/`stability_margins`/`bandwidth`/`step_info` 生成（`scripts/golden/gen_golden_loopsens.py`，12 例）。子算子报错时该字段为 null，并给 `W.DOMAIN.ANALYSIS_SKIPPED`（`details.code` 为子算子的错误码），其余字段照常返回；子算子的警告加上路径前缀（如 `/margins/…`）原样转发。只支持 SISO；采样周期按 I-4。
- K-5 **频率字段写单位。** 返回频率的字段名 MUST 带单位后缀，并且两种单位都给：`gain_crossover_rad_s` / `gain_crossover_hz`（|L(jω)| = 1，开环穿越频率），`bandwidth_rad_s` / `bandwidth_hz`（闭环 T 的带宽）。`_hz` MUST 恰好等于 `_rad_s / 2π`。带宽的定义 MUST 回显在 `bandwidth_definition`：`{drop_db, reference: "dc_gain", of: "closed_loop", method}`，即第一个使 |T(jω)| < |T(0)|·10^{drop_db/20} 的频率；连续系统用 |T(jω)|² 在 ω² 上的偶多项式求根再二分精化，离散系统在 (0, π/ts) 的网格上找第一个跨越再二分。T 不稳定、T 不 proper、DC 增益为 0 或无穷时为 null 并给 `W.DOMAIN.ANALYSIS_SKIPPED`；T 稳定但在整个频率范围内都没有跌到阈值以下（例如双正则且高频增益不低于阈值）时为 `"Inf"`，与 python-control 的 `bandwidth` 一致。带宽算在 plant·controller 的闭环上，不是只算对象。
- K-6 **系统靠原样传递，不靠手抄。** 返回的 `result.system`（以及 `open_loop` / `closed_loop` / `sensitivity`）就是 I-1 的句柄。绑定层回传给模型时 MUST NOT 抽样、截断或改写任何 `LtiSystem`（F-6 的 `_downsampled` 只适用于时间/频率数组）；系统提示词与工具描述 MUST 写明“把上一个工具返回的系统 JSON 原样作为下一个工具的参数，不要重新抄写或化简系数”，`input` 描述里的 `LtiSystem` 参数 MUST 带这句话。题目给出的因式乘积 SHOULD 用 `series` 相乘，而不是由模型展开。

### 3.14 方法的登记与选择

到 v0.3.6 为止，一个算子版本只有一个方法：manifest 的 `receipt.method_id` 写死，收据的 `method.id` 回显它。于是“换一个实现”或“换一种算法”只能靠发布新的算子版本，用户也没有办法要求“只用经过审计的库”。v0.4 允许同一个算子登记多个方法，并让请求在这些方法之间选择。

**方法 id 和算子 id 不是一回事。** 算子 id（M-2）说的是“算什么”，例如 `control.math.lyap`；方法 id（4.3 R-1）说的是“怎么算”，例如 `aeoi:lqr:sign-function+kform-newton-kleinman-dot2`。同一个算子可以有几种算法，各自有自己的精确性类、误差界和前提。以下条款是规范性要求；参考实现已实现解析、收据、重放与宿主策略合并（`aeoi-core::method`、`aeoi-bridge`），但每个算子仍只登记一个方法。

**登记**
- MS-1 manifest 的 `receipt`（4.3 R-1）描述 `default` 方法，其 `method_id` 就是缺省方法的 id。同一算子的其他方法登记在可选的 `receipt.alternatives[]`，每项与 `receipt` 同形（`method_id`、`assumptions`、`exactness`、`postconditions`，以及可选的 `coverage`、`guards`），另加必填的 `backend`（M-6 的对象）与 `algorithm`（M-5 的一句话），可选的 `determinism`、`tolerance_class`（缺省继承算子级声明）。允许的方法集合 = `{receipt.method_id} ∪ {alternatives[].method_id}`。没有 `alternatives` 的算子只有一个方法，v0.3 的 manifest 不需要修改。
- MS-2 方法 id 的命名空间：参考实现的方法 id 以 `aeoi:` 开头（已有，如 `aeoi:lqr:…`）；其他实现 MUST 用自己的前缀，写成 `<实现名>:<算子短名>:<算法>`，MUST NOT 使用 `aeoi:`。同一算子内方法 id MUST 唯一，字符集沿用 manifest schema 对 `method_id` 的限制 `^[a-z0-9_.:+-]+$`。
- MS-3 **每个方法各自满足该算子的一切要求。** 每个已登记的方法 MUST 单独通过该算子的 parity（7.1、9.4）；`trust.accuracy_grade`、`reproducibility.determinism`、`tolerance_class`、凭证（V-7）都按**实际所用的方法**计。没有被检验过的方法不得登记。收据里的精确性类、误差界与前提 MUST 与所用方法的登记项一致。
- MS-4 版本影响（9.2）：增加方法按 G-2（MINOR）；改变 `default` 会改变缺省请求的 `output_hash`，MUST 按 G-1“改变默认值导致结果变化”处理；删除方法也按 G-1 处理，因为已有请求可能用 `require` 指定了它。

**选择**
- MS-5 请求在 `exec` 中增加可选对象 `method_policy`（`ExecOptions` 增加这个属性）：
  - `require`：方法 id 数组，**硬约束**，只能用其中的方法；
  - `constraints`：**硬约束**，当前定义两个键：`min_exactness`（精确性类有序，`exact` > `correctly_rounded` > `approximate`，所用方法的类 MUST 不低于它）与 `backend_library_in`（库名数组，所用方法的 `backend.library` MUST 属于其中，用来表达“只用这个经过审计的库”）。

  v0.4 没有软偏好（曾设想的 `prefer` 已去掉）：策略里的每一项都是硬约束，所以“请求的策略有没有被满足”没有第三种答案，收据也不需要回显满足程度。需要时再加（12.1 第 16 项）。

  `method_policy` 放在 `exec` 里，不放在算子参数里：它决定“怎么算”，不改变“算什么”。因此按 P-2 它不进入 `input_hash`，`fingerprint.request` 也不变：同一个问题在任何策略下有同一个请求指纹，答案（`fingerprint.result`）因方法而异。4.3 R-3 本来就把这两件事分开，这里不改。
- MS-6 解析规则。候选集 = 允许的方法集合 ∩ `require`（如给出）∩ 满足 `constraints` 的方法。
  - 候选集为空 MUST 返回 `E.VALIDATION.METHOD_UNAVAILABLE`（5.1）：`details` 写明为什么为空（`require` 里未登记的 id、不满足的约束）以及当前允许的方法及各自的声明；remediation 给 `modify_input`（去掉或放宽 `method_policy` 的 RFC 6902 补丁，`requires_user_confirmation: true`，因为策略可能是用户有意设的，X-6）和 `ask_user`。MUST NOT 静默改用不满足硬约束的方法。
  - 候选集非空时：取候选集中的 `default`；`default` 不在候选集里时，取 manifest 中排在最前面的候选方法（`receipt` 之后按 `alternatives[]` 的顺序）。这个选择是确定的，只依赖 manifest 与策略。
- MS-7 收据：请求带了 `exec.method_policy` 时，成功信封的收据 MUST 多一个 `method_selection`：`{ "policy": <回显>, "used": "<方法 id>" }`。请求没带 `method_policy` 时，收据与 v0.3 逐字节相同（与 E-12、A-2 一样，可选字段只在适用时出现）。`method.id` 始终是实际所用方法的 id，`method.routine` 取自该方法的 `backend.routine`。
- MS-8 不认识 `method_policy` 的实现：`ExecOptions` 是封闭对象（3.3 R-4），旧实现会对这个字段返回 `E.VALIDATION.SCHEMA`。实现 MUST NOT 为了兼容而忽略 `method_policy`：硬约束被静默忽略比报错更危险。
- MS-9 重放（4.3 R-5）：收据带 `method_selection` 时，重放 MUST 以 `exec.method_policy = {"require": [<used>]}` 重新运行，不使用原来的策略，因为策略可能来自当时的宿主（MS-10），重放时未必还有同样的宿主策略；`used` 才是结果实际来自的方法。重放环境没有该方法时，算子返回 `E.VALIDATION.METHOD_UNAVAILABLE`，verify 的 `verdict` 为 `method_unavailable`（新增的取值）。比较仍按 R-5（逐位或容差）。两个不同方法之间的比较不是重放，而是 9.4 的一致性判定。

**谁来设置策略**
- MS-10 策略有两个来源：宿主或部署层（用户或组织的规定，例如“只用经过审计的库”），以及模型生成的请求。生效策略 = 宿主策略与请求策略的交集：`require` 取交集，`constraints` 取更严的（`min_exactness` 取较高者，`backend_library_in` 取交集）。合并后候选集为空时按 MS-6 报错。请求 MUST NOT 能放宽宿主策略。宿主策略由绑定层在调用算子前合并进 `exec.method_policy`；这是请求的一部分，不是对结果的改写，所以不违反 B-1。算子只处理它实际收到的 `exec.method_policy`。
- MS-11 LLM 绑定默认隐藏 `exec`（F-6），模型因此不能改策略；宿主策略仍按 MS-10 注入，当前被允许的方法集合 SHOULD 写进工具描述（F-1），让模型知道有哪些方法可用。绑定层若允许模型自己提出 `method_policy`，它只能收紧宿主策略。

### 3.15 表达式语言

到 v0.4.7 为止，算子的输入都是数据：数、数组、系统。ODE 的右端 f(t, y) 是一个函数。JSON 只能传数据。把代码（Python、MATLAB 的匿名函数）交给算子执行，既不安全，也无法复现，因为结果取决于哪个解释器、哪个版本。所以 v0.4.8 用一种小的表达式语言，让函数也作为数据传递。语法固定，函数表固定，算子只解析、只求值，不执行任何代码。以后凡是要由用户给出函数的算子（非线性方程、优化目标、被积函数）都复用本节。参考实现在 `aeoi-numeric::expr`。golden 生成脚本里另有一个独立的 Python 实现（`scripts/golden/gen_golden_ode.py`），它只依据本节写成。

- EX-1 **语法**（EBNF；空白为空格、制表符、CR、LF，可出现在任意两个记号之间）：

  ```text
  expr    = sum
  sum     = product { ("+" | "-") product }          左结合
  product = unary { ("*" | "/") unary }              左结合
  unary   = ("+" | "-") unary | power
  power   = primary [ "^" unary ]                    右结合：2^3^2 = 2^(3^2) = 512；-x^2 = -(x^2)；2^-1 = 0.5
  primary = number | name | name "(" expr { "," expr } ")" | "(" expr ")"
  number  = digits [ "." { digit } ] [ exponent ] | "." digits [ exponent ]
  exponent= ("e" | "E") [ "+" | "-" ] digits
  name    = ( letter | "_" ) { letter | digit | "_" }
  ```

  优先级从低到高：`+ -`（二元）、`* /`、一元 `+ -`、`^`。乘方的优先级高于一元负号，这一点与数学书写和 MATLAB 相同，与 Excel 不同（Excel 里 `-2^2` 等于 4）。所以 `- -2^2*3` 的值是 −((−(2²))·3) = +12。乘方只写作 `^`，`**` 和 MATLAB 的 `.^` 都是语法错误。只允许 ASCII 字符。`×`、`÷`、`·`、`²` 报 `non_ascii`，并指出它的位置。
- EX-2 **名字**。表达式里的名字只能是以下三种：算子声明的变量（ODE 中是 `t`、状态名、参数名，见 10.18 OD-1）、函数表里的函数名、常量 `pi`。名字区分大小写。变量名 MUST 符合 `name` 的语法，MUST NOT 与函数名、`pi` 或算子的保留名（ODE 的保留名是 `t`）相同，同一作用域里的变量名也 MUST NOT 重复。违反时报 `bad_name` 或 `duplicate_name`。用了未声明的名字报 `unknown_name`。语言里没有赋值、条件、循环，也不能自定义函数，所以每个表达式的求值时间是 O(节点数)，一定会结束。
- EX-3 **函数表**（名字是保留的；参数个数不对报 `arity`）：

  | 函数 | 参数 | 定义 |
  |---|---|---|
  | `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `exp` `log` `log10` `sqrt` `abs` | 1 | 同名的 IEEE-754 / C 函数；`log` 为自然对数 |
  | `atan2(y, x)` | 2 | y 在前（与 C、MATLAB、numpy 相同），值域 (−π, π] |
  | `hypot(x, y)` | 2 | √(x² + y²)，中间结果不溢出 |
  | `min(a, b)`、`max(a, b)` | 2 | 任一参数为 NaN 时结果为 NaN（与 C 的 `fmin`/`fmax` 不同：后者会忽略 NaN，模型里的 NaN 不应被悄悄吞掉） |
  | `sign(x)` | 1 | x > 0 为 1，x < 0 为 −1，x = ±0 为 +0，NaN 为 NaN |
  | `a ^ b` | — | IEEE `pow`：`0^0 = 1`，负底数的非整数次幂为 NaN（`(-8)^(1/3)` 不是 −2） |

  定义域外的值（如 `log(-1)`、`sqrt(-1)`）按 IEEE 给 NaN 或 ±Inf，解析时不报错。表达式不知道这些值会用在哪里，是否出错由算子决定（ODE 见 OD-6）。
- EX-4 **求值顺序**。值等于把语法树按 binary64 逐节点计算的结果：从左到右，MUST NOT 重结合（`a + b + c` 就是 `(a + b) + c`），MUST NOT 合并为 FMA，MUST NOT 做会改变结果位的代数化简。常量子表达式可以预先算好，因为位相同。数字字面量按最近舍入转成 binary64；超出范围的字面量（`1e999`）报 `not_finite`。`+ − * /` 和 `sqrt` 是正确舍入的；其余初等函数取自实现所用的数学库，它属于 2.1 的“平台”描述。参考实现自 v0.4.9 起把纯 Rust `libm` 编入二进制（M-7a），各平台用同一套函数。
- EX-5 **规模上限**：每个表达式最多 10 000 个字符、10 000 个语法树节点，嵌套深度不超过 200。深度按语言结构计：每进入一层括号、一个函数调用的参数、一个一元正负号、一个 `^` 的指数，深度加 1；同一层里连写的 `+ − * /`（如 `x + x + … + x`）不增加深度。超出时分别报 `too_long`、`too_many_nodes`、`too_deep`。深度有上限，所以递归下降解析器不会因栈溢出而崩溃（X-3）。
- EX-6 **错误**。静态错误一律报 `E.VALIDATION.EXPRESSION`（新码）。`details` 包含：`path`（指向出错表达式的 JSON Pointer，如 `/rhs/1`）、`expression`（原文）、`position`（从 0 起算的字符偏移）、`kind`（取值为 `syntax`、`unknown_name`、`arity`、`non_ascii`、`too_long`、`too_many_nodes`、`too_deep`、`not_finite`、`bad_name`、`duplicate_name`）。`message` 的最后一行是原文加一个指向错误位置的 `^`。所有表达式都先通过检查，然后才开始求值，不会出现算到一半才发现第三个表达式写错的情况。remediation 是 `modify_input`（改写该表达式）和 `ask_user`。
- EX-7 实现 MUST NOT 借用宿主语言的 `eval` 或任何能执行代码的机制，也 MUST NOT 接受本节以外的语法或函数（不提供“方便的扩展”）。两个实现只要都遵守本节，对同一个表达式就有相同的语义；golden 里的右端由 Python 实现求值，参考实现与它的差别只来自 libm。
- EX-8 **自动微分**（作者决定）：需要导数的算子（`fsolve`，以后的 `fmincon`、`ode15s`）MUST 对表达式做前向自动微分，得到精确到舍入误差的导数，MUST NOT 用有限差分。
  - 值部分与 EX-4 的求值逐位相同。
  - 不可导点的约定：abs′(0) = 0，sign′ = 0；min、max 取被选中那个参数的导数，相等时取第一个；指数为常数的幂 x^c 用 c·x^(c−1)。
  - 参考实现在 `aeoi-numeric::expr::eval_grad`，所有需要导数的算子共用它（11.5 RI-1）。
  - 一致性运行器（测试工具）对套件自带的表达式用 Python 求值，这不受 EX-7 约束：被测的是实现，不是运行器。

## 4. 结果信封与可信度元数据

### 4.1 规则

- E-1 每次调用（无论成败）MUST 返回一个符合 `schemas/envelope.schema.json` 的信封。
- E-2 `status` ∈ `ok` | `ok_with_warnings` | `error`。只要 `warnings` 非空，`status` MUST 为 `ok_with_warnings`。`error` 时 `result` MUST 为 `null`。
- E-3 非 dry-run 的成功信封 MUST 包含非空的 `trust` 与 `reproducibility`；错误信封中二者 MAY 为 `null`（schema 用 `allOf` 条件表达）。
- E-4 `trust` 字段：

| 字段 | 必须 | 内容 |
|---|---|---|
| `convergence` | MUST | `applicable`；迭代算法另需 `converged`、`iterations`、`criterion`、`final_value` |
| `residual` | 若算法有自然残差则 MUST | 如 CARE 残差、阶跃响应终点一致性偏差 |
| `condition` | SHOULD | 关键条件数或其估计，以及到奇异/不可控的距离 |
| `error_estimate` | MUST（可为 null） | 误差估计值 + `basis` |
| `checks` | SHOULD | **只含布尔值**的后置条件检查，如 `S_psd`、`closed_loop_stable`；数值诊断放入 `residual`/`condition` |
| `accuracy_grade` | MUST | 当前版本的精度等级 A/B/C/U（第 7 章） |
| `credentials` | L3 MUST，否则 MAY 为 `null` | 指向验证凭证的 URI |
| `derived` | MAY | 计算中得到、对 Agent 有用的副产品（极点、DC 增益、实际 `dt`） |

- E-5 误差估计不得夸大：若只有启发式估计，`basis` MUST 写明"非严格上界"。没有估计时 MUST 写 `null`，MUST NOT 编造数字。
- E-6 `trust_level: "summary"`（默认）只返回上表字段；`"full"` MAY 附加逐点诊断数组。
- E-7 `warnings[]` 元素：`code`（`W.<CATEGORY>.<NAME>`）、`message`、`path`（JSON Pointer）、可选 `details` 与 `remediation`。算子 MUST 在 manifest `warnings[]` 中声明所有可能返回的 warning code。
- E-8 MCP 返回时，信封放入 `structuredContent`；`content` 中 MUST 给出一段简短文本摘要（status、精度等级、warning 列表），并 SHOULD 再给出信封的 JSON 文本，以兼容不读 `structuredContent` 的客户端。
- E-9 **dry run**：`exec.dry_run = true` 时，算子完成全部校验（schema、语义、规模上限）后停止，返回 `dry_run: true`、`result: null`、`estimate`（如网格点数、flops 估计）；不得执行主计算。信封字段 `dry_run` 为必填。
- E-10 **输出编码**：见 L-5。
- E-11 **没有证据，不声称成功**：成功信封的 `receipt.evidence.claim` 只能是 `verified`（manifest `receipt.postconditions` 列出的 `trust.checks` 全部为 true）、`spot_checked`（全部为 true，但至少一个只是抽样检查，E-12，v0.3.6 新增）、`unchecked`（算子没有声明后置条件）或 `unverified`。任何声明的后置条件为 false 或缺失时，框架 MUST 追加 `W.EVIDENCE.POSTCONDITION_FAILED`（于是 `status` 为 `ok_with_warnings`），claim 为 `unverified`。`ok` 本身只表示"算子完成了计算"；只有 `verified` 才能被下游（含 LLM）表述为"已验证"。只描述问题性质的检查（如 `asymptotically_stable`、对称化之前的不对称度）MUST NOT 列为后置条件，否则合法结果会被误判。
- E-7a **框架级码**：由框架而不是算子产生的码（目前只有 `W.EVIDENCE.POSTCONDITION_FAILED`）在 5.1 统一登记，不要求在每个 manifest 的 `warnings[]` 中重复声明；E-7 的"MUST 声明"只约束算子自己产生的码。
- E-11a **算子契约点名的后置条件是最小集合。** profile（第 10 章）里点名的后置条件（如 FT-6 的 `parseval`、`dft_spot_check`），每个实现 MUST 在 manifest 的 `receipt.postconditions` 里声明，并以同样的 `coverage` 报告；实现 MAY 另加。否则一个实现少列一项检查，就能更容易地声称 `verified`。
- E-12 **抽样的检查只是抽查。** manifest 的 `receipt.coverage` 为每个后置条件声明它覆盖了多少结果：`full`（在整个结果上求值；缺省）、`test_points`（一个结果必须处处满足的恒等式，在算子事先固定、与结果无关的若干点上核对；结果本身由有限次精确形式的运算得到，例如组合算子的 I-9）、`sampled`（只在结果的子集、网格或压缩副本上求值，例如 `margin` 在频率网格上复查有没有漏掉穿越点）。所有后置条件为 true 但其中有 `sampled` 时，claim MUST 是 `spot_checked`，不是 `verified`。收据 `evidence.coverage` 抄录完整的映射（未声明者写 `full`）；没有声明 coverage 的算子 MUST NOT 出现这个字段（已发布收据字节不变）。绑定层抽样后的视图（F-6 的 `_downsampled`）、模型自己写的摘要都不是证据：MUST NOT 在它们上面计算任何后置条件。v0.3.6 声明 `sampled` 的有：`margin.grid_audit_clean`、`loopsens.sub_operator_checks`（因为裕度来自 `margin`）；声明 `test_points` 的有 `series`/`parallel`/`feedback`/`convert` 的 `response_consistent`、`pid.formula_matches`、`loopsens.closed_loop_identity`。
- E-13 **失败的检查要指出哪些字段不能用。** EngDesign 的 XG_02 是 7 阶系统：Krylov 可控性矩阵的数值秩是 4，PBH 检验认为满秩，`pbh_agrees_ctrb` 为 false，claim 正确地为 `unverified`；但信封没有说哪些字段因此不能用，模型看到的又是被抽样的矩阵，`rank`、`full_rank`、`minimal` 看起来和正常结果一样。manifest 的 `receipt.guards` 把后置条件映射到它保护的结果字段（JSON Pointer）；某个检查失败时，收据 `evidence.unreliable` MUST 列出这些字段（排序去重），`W.EVIDENCE.POSTCONDITION_FAILED` 的消息与 `details.unreliable` 写出同一列表，绑定层 MUST 把它作为 `unreliable_fields` 告诉模型，系统提示词 MUST 要求“不得引用、不得作为下一步的依据”。v0.3.6：`ctrb_obsv` 的 `pbh_agrees_ctrb` → `/ctrb/rank`、`/ctrb/full_rank`、`/minimal`（obsv 同理）；组合算子的 `response_consistent` → `/system`；`loopsens` 见其 manifest。

### 4.2 信封骨架（字段取自参考实现的实际输出）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok | ok_with_warnings | error",
  "dry_run": false,
  "operator": { "id": "control.lti.step", "version": "0.2.0", "build": "aeoi-control 0.2.0+<git sha|nogit> nalgebra-0.33 rustc-1.85" },
  "request_id": "req-step-001",
  "result": { "...": "由 output_schema 定义" },
  "trust": { "convergence": {}, "residual": {}, "condition": {}, "error_estimate": {}, "checks": {}, "accuracy_grade": "B", "credentials": null, "derived": {} },
  "warnings": [],
  "error": null,
  "estimate": null,
  "reproducibility": { "determinism": "platform_bitwise", "tolerance_class": "T2", "input_hash": "sha256:…", "output_hash": "sha256:…",
                       "platform": "x86_64-linux", "cpu_features": ["avx", "avx2", "fma", "avx512f"],
                       "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)", "threads": 1, "seed": null },
  "timing": { "wall_ms": 0.4 }
}
```

### 4.3 结果收据（Result Receipt）

每个非 dry-run 的成功信封带一个 `receipt`（`schemas/envelope.schema.json#/$defs/Receipt`）；错误与 dry-run 信封中为 `null`。收据回答四个问题：**用什么方法算的、在什么前提下成立、有多精确、凭什么说它对**，并给出可以离线复核的指纹。思路借鉴 bicmath（nikoma，MIT）的"可验证结果"与 matlab-mcp-extensions（zws20070309）的"没有证据不声称成功"；只借鉴想法，没有复制代码。

- R-1 **静态部分来自 manifest**：manifest 新增 `receipt { method_id, assumptions[], exactness {class, bound_kind}, postconditions[] }`（`schemas/manifest.schema.json`）。`assumptions[].enforced_by` 写明每条前提由谁保证：`validation`、`error:<code>`（不满足时算子报该错误）、`warning:<code>`、`check:<key>`，或 `caller`（算子不检查，由调用方负责，例如"模型录入正确"、"ts 来自用户"）。参考实现的校验器检查每个 `error:`/`warning:` 码都在该 manifest 中声明。
- R-2 **精确性类** `exactness.class` ∈ `exact`（结果对给定的 f64 输入无舍入）、`correctly_rounded`（结果是真值的正确舍入，需有证明）、`approximate`。`bound_kind` ∈ `rigorous`（严格上界）、`a_priori`、`a_posteriori_heuristic`、`none`；`bound` 抄录 `trust.error_estimate` 中的数值字段（`info_*`、`worst_*` 等非误差字段除外），没有数值时 `bound_kind` 写 `none_reported`。12 个参考算子都是 `approximate`：即使 `lqr` 0.3.0 在 50 位真值上测得的误差为 0（10.3），也没有证明，MUST NOT 写成 `correctly_rounded`。
- R-3 **指纹**（均为 RFC 8785 JCS 后的 SHA-256）：
  - `fingerprint.request = H({operator, version, input_hash})`：输入规范化是精确的（P-2：最短往返数字、数组用内容摘要），所以它在所有平台、所有 build 上相同，并且与输入/输出编码（json/b64/ref）无关。
  - `fingerprint.result = H({request, output_hash})`：结果逐位相同时才相同。`platform_bitwise` 算子只保证同 build + 同平台相同（P-7）；跨平台（如原生 x86_64 与 wasm32）只有逐位相同的结果相同（v0.3.2 实测 74% 的 golden 用例）。
  - 选择理由：把平台或 build 放进 result 指纹会让跨平台复核永远失败；只用输入指纹又无法区分结果是否被改动。分成两层后，"同一个问题"与"同一个答案"各有一个稳定的标识，跨平台时由 R-5 的容差比较补上。`build` 与 `platform` 写在 `replay` 中，不进入指纹。
- R-4 `evidence`：`claim`（E-11/E-12）、各后置条件的取值与失败列表、迭代算法的收敛信息、`accuracy_grade`、`tolerance_class`、`credentials`；v0.3.6 起可选的 `coverage`（E-12）与 `unreliable`（E-13）。manifest 的 `receipt` 相应增加可选的 `coverage`（后置条件 → `full`|`test_points`|`sampled`）与 `guards`（后置条件 → JSON Pointer 数组）。
- R-5 **重放验证**：`verify(operator, arguments, envelope|receipt)` 重新运行请求并比较。结论 `verdict`：
  - `request_mismatch`：重算的 request 指纹不同（输入或算子版本不是这张收据的）；
  - `bitwise_match`：result 指纹相同，且信封中的 `result` 重新哈希后等于收据的 `output_hash`——不等时为 `result_tampered`（结果在签发后被改过）；
  - `tolerance_match` / `mismatch`：result 指纹不同时，按算子容差类（7.2：T1 1e-12、T2 1e-10、T3 1e-9、T4/T5 1e-6）对每个顶层输出字段做范数相对比较（‖Δ‖∞ ≤ rtol·‖x‖∞，近零字段以整个结果的 1e-14·‖·‖∞ 为界），字符串、布尔、键与数组长度必须完全相同。同 build、同平台却不逐位相同时附注"违反 P-7"。只给收据（没有 result）时无法做容差比较，结论为 `mismatch`；
  - 错误信封比较错误码：`error_reproduced` / `status_mismatch`。
  - 对解不唯一的算子（多输入 `place` 的 K），跨平台的 `mismatch` 不代表错误，应改比不变量（V-5a，达到的闭环极点）；verify 目前不做这种替换。
- R-6 **入口**：参考实现在 `aeoi_core::receipt::verify`，暴露为 MCP 工具 `aeoi_verify`（参数 `operator`、`arguments`、`envelope`；只读、幂等）、命令行 `aeoi-call verify <operator> <request.json> <envelope.json>`（通过时退出码 0）、wasm 导出 `verify` / `verify_tool_call`，以及网页轨迹面板中每次工具调用的"重放验证"按钮（本地版走 `POST /api/verify`，浏览器版在 wasm 里重放）。
- R-7 **对 LLM 的呈现**：绑定层回传给模型的是收据的短形式（`claim`、`failed_postconditions`、`exactness`、`unchecked_assumptions` = `enforced_by: "caller"` 的前提、`method`、`fingerprint`）；系统提示词要求模型：只有 `verified` 才能说"已验证"，`spot_checked` 只能说“抽查通过”，`unchecked` 要说明没有独立后验检查，`unverified` 不能当作成功结论，并在回答前确认 `unchecked_assumptions` 成立。v0.3.6 起短形式另带 `coverage`（有非 `full` 项时）、`unreliable_fields` 与一句不得引用这些字段的说明（E-13）；回传结果里有数组被抽样时，短形式带 `result_view: "downsampled"`，提醒模型这不是完整结果。
- R-8 收据不是签名：它证明"这组输入在这个算子版本上会得到这个结果"，任何人可以重放复核；它不证明是谁算的。需要来源证明时，对收据（或整个信封）再做 V-8 的 CI 签名。
- R-9 **方法选择**：请求带 `exec.method_policy` 时收据多一个 `method_selection`（3.14 MS-7）；重放以 `require: [used]` 进行，环境没有该方法时 verdict 为 `method_unavailable`（MS-9）。`exec` 不进入 `input_hash`（P-2），所以策略不影响 `fingerprint.request`。

收据实例（`control.design.lqr` 示例响应，节选）：

```json
{
  "format": "aeoi-receipt/1",
  "method": { "id": "aeoi:lqr:sign-function+kform-newton-kleinman-dot2", "routine": "lqr::solve_care (…)", "provenance": "aeoi://operators/control.design.lqr/0.3.0/manifest#/provenance" },
  "assumptions": [ { "id": "stabilizable", "statement": "(A,B) 可镇定", "enforced_by": "error:E.DOMAIN.NOT_STABILIZABLE" },
                   { "id": "model", "statement": "A、B、Q、R、N 准确描述设计问题（单位一致、row-major）", "enforced_by": "caller" } ],
  "exactness": { "class": "approximate", "bound_kind": "a_posteriori_heuristic", "bound": { "S_abs_fro": 8.5e-16, "S_rel": 3.0e-16 }, "basis": "一阶后验估计 …；非严格上界" },
  "evidence": { "claim": "verified", "postconditions": { "S_psd": true, "closed_loop_stable": true }, "failed": [], "convergence": { "converged": true, "iterations": 3 }, "accuracy_grade": "B", "tolerance_class": "T2", "credentials": null },
  "fingerprint": { "request": "sha256:f5d755e5…", "result": "sha256:041eafec…", "output_hash": "sha256:7c9bd47e…", "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))" },
  "replay": { "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85", "platform": "x86_64-linux", "verify": { "mcp_tool": "aeoi_verify", "cli": "aeoi-call verify …", "wasm": "verify(operator, request, envelope)" } }
}
```

## 5. 结构化错误

### 5.1 错误码分类

格式：`E.<CATEGORY>.<NAME>`。类别固定为 5 个；`NAME` 由注册表统一登记，算子 MUST 在 manifest `errors[]` 中列出可能返回的全部错误码（参考实现在 parity 测试中逐次断言这一点）。

| 类别 | 何时使用 | `retryable` 默认 | 已登记的码（v0.3 新增的加粗） |
|---|---|---|---|
| `VALIDATION` | 输入不符合 schema 或语义约束 | true（改输入） | `SCHEMA`、`SHAPE`、`DTYPE`、`RANGE`、`NON_FINITE`、`NOT_SYMMETRIC`、`NOT_CONJUGATE`、`ZERO_DENOMINATOR`、`GRID`、`SPARSE_STRUCTURE`、`DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN`、**`OPTION_CONFLICT`**、**`RANK_DEFICIENT`**、**`UNGROUNDED_INPUT`**（v0.3.1，绑定层）、**`METHOD_UNAVAILABLE`**、`EXPRESSION`（v0.4.8，3.15） |
| `DOMAIN` | 输入合法，但数学问题本身无解/不适用 | 视情况 | `IMPROPER_SYSTEM`、`NOT_STABILIZABLE`、`R_NOT_POSITIVE_DEFINITE`、`WEIGHT_NOT_PSD`、`HAMILTONIAN_IMAG_AXIS_EIG`、`ABOVE_NYQUIST`、**`MIMO_NOT_SUPPORTED`**、**`DEGENERATE_SYSTEM`**、**`NOT_CONTINUOUS`**、**`NOT_CONTROLLABLE`**、**`POLE_MULTIPLICITY`**、**`LYAP_SINGULAR`**、**`SYMPLECTIC_UNIT_CIRCLE_EIG`**、`UNDERDETERMINED`、`EXTRAPOLATION_REQUIRED`、`NO_SIGN_CHANGE`、`INFEASIBLE`、`UNBOUNDED`、`ZERO_POLYNOMIAL` |
| `NUMERICAL` | 问题理论上可解，但数值上失败或不可信 | true（正则化/放宽/换算法） | `SINGULAR`、`NO_CONVERGENCE`、`POLE_ON_GRID`、`NON_FINITE_EVALUATION`、`STEP_SIZE_TOO_SMALL`、`LOCAL_MINIMUM` |
| `RESOURCE` | 超时、超规模、被取消 | true | `TIMEOUT`、`INPUT_TOO_LARGE`、`COMPUTE_LIMIT`、`OUTPUT_TOO_LARGE` |
| `INTERNAL` | 实现缺陷或后端异常 | false（可原样重试一次） | `PANIC`（`BACKEND`、`INVARIANT_VIOLATED` 为保留码，v0.2 算子未使用） |

已登记的 warning（v0.3.3 新增框架级 `W.EVIDENCE.POSTCONDITION_FAILED`，见 E-7a、E-11）：`W.NUMERICAL.{NEAR_POLE, ZERO_ON_GRID, ILL_CONDITIONED, LARGE_RESIDUAL, NEAR_UNCONTROLLABLE, **RANK_AMBIGUOUS, CROSSING_RECOVERED, PLACEMENT_INACCURATE, CONVERSION_DISCREPANCY**, STIFF_PROBLEM, DET_NOT_SIGNIFICANT, DET_OUT_OF_RANGE}`、`W.DOMAIN.{BRACKET_SEARCHED, UNSTABLE_SYSTEM, NOT_SETTLED, INFINITE_DC_GAIN, ZERO_DC_GAIN, **UNDEFINED_DC_GAIN, NONMINIMAL_REALIZATION, REALIZATION_DEPENDENT, GRAMIAN_UNDEFINED, CLOSED_LOOP_UNSTABLE, DIRECT_FEEDTHROUGH_IGNORED**}`、`W.ACCURACY.{GRID_TOO_COARSE, AUTO_GRID, **ALIASING**, EXTRAPOLATED, CLUSTERED_ROOTS}`、`W.VALIDATION.{LEADING_ZEROS_STRIPPED, SYMMETRIZED}`、`W.PERFORMANCE.SPARSE_DENSIFIED`（加粗为 v0.3 新增）。

v0.3.1 新增码：`UNGROUNDED_INPUT` 由 LLM 绑定层（不是算子）返回，表示 `x-aeoi-provenance: "user"` 的输入没有用户原话作为来源（F-5(c)），`retryable: true` 但只能在问过用户之后重试。v0.3 新增码的含义：`OPTION_CONFLICT` 选项组合矛盾（O-2）；`RANK_DEFICIENT` 输入矩阵列不满秩（`place` 的 B）；`MIMO_NOT_SUPPORTED` 该算子/表示只支持 SISO（R-8、`margin`）；`DEGENERATE_SYSTEM` 系统矩阵非满法秩，零点无定义（R-9）；`NOT_CONTINUOUS` 需要连续系统却给了离散系统（`c2d`）；`NOT_CONTROLLABLE` 有不可控模态，无法任意配置极点（`place`；与 `NOT_STABILIZABLE` 区分：后者只关心不稳定模态）；`POLE_MULTIPLICITY` 期望极点重数超过输入数 m；`LYAP_SINGULAR` Lyapunov 方程奇异（连续 λᵢ + λ̄ⱼ ≈ 0，离散 λᵢλ̄ⱼ ≈ 1）；`SYMPLECTIC_UNIT_CIRCLE_EIG` DARE 的辛矩阵束在单位圆上有特征值（单位圆上的模态不被 Q 观测）。这些都是 `DOMAIN`/`VALIDATION`：调数值参数无效（X-1）。

v0.4.10 新增码：`UNDERDETERMINED` 方程个数少于未知数，解不唯一（LS-1）；`EXTRAPOLATION_REQUIRED` 查询点超出数据范围，而调用方没有说明怎么处理（IP-3）；警告 `W.ACCURACY.EXTRAPOLATED` 结果中有外推得到的值。

v0.4.13 新增码：`NO_SIGN_CHANGE` 求根区间两端同号，或从 x0 搜不到变号区间（FZ-1）；`LOCAL_MINIMUM` 迭代停在残差的局部极小，不是根（FS-4）；警告 `W.DOMAIN.BRACKET_SEARCHED` 求根区间是从 x0 搜出来的，根不一定是想要的那个。v0.4.14 新增码：`INFEASIBLE` 约束没有可行解（线性规划或边界）；`UNBOUNDED` 目标在可行域上无下界。

v0.4.8 新增码：`E.VALIDATION.EXPRESSION`（表达式的静态错误，3.15 EX-6）、`E.NUMERICAL.NON_FINITE_EVALUATION`（模型在某点算出 NaN/Inf）、`E.NUMERICAL.STEP_SIZE_TOO_SMALL`（自适应步长缩到下限）与 `W.NUMERICAL.STIFF_PROBLEM`，含义见 10.18 OD-6、OD-7。

v0.4 新增码：`METHOD_UNAVAILABLE` 由算子返回（3.14 MS-6）：`exec.method_policy` 的硬约束没有任何已登记的方法能满足。`retryable: true`，修复是改策略或问用户。类别固定为 5 个，所以它归 `VALIDATION`（改输入可以解决），不另设 `METHOD` 类别。

规则：
- X-1 `DOMAIN` 与 `NUMERICAL` MUST 区分清楚：前者告诉 Agent"别再调参了，问题本身不成立"，后者告诉 Agent"换个数值处理可能成功"。这是本规范对 Agent 最重要的区分。
- X-2 FFI 后端的每个返回码 MUST 映射到具体错误码；MUST NOT 把后端错误笼统报为 `INTERNAL.BACKEND`，除非确实无法归类。
- X-3 Rust `panic` MUST 被 `catch_unwind` 捕获并转为 `E.INTERNAL.PANIC`，FFI 边界 MUST NOT 让 panic 或 Fortran 异常跨越。
- X-4 数值"勉强成功"SHOULD 返回成功 + `W.NUMERICAL.ILL_CONDITIONED`，而不是错误；阈值 MUST 写在 manifest。
- X-4a **数值判据 MUST 与问题尺度无关**：判断"特征值在虚轴上"、"矩阵奇异"等的阈值 MUST 以机器精度 u 与相关范数表达（如 `|Re λ| ≤ 10³·u·‖H‖₁`），MUST NOT 用与尺度无关的固定小数。（v0.2 实现初版用 `1e-8·‖H‖₁`，在 cond(R)=10⁹ 的合法问题上误报 `HAMILTONIAN_IMAG_AXIS_EIG`，被 parity 测试发现。）**（v0.3.2 补充）** 以机器精度表达仍不够：范数本身 MUST 不被某个可任意缩放的块主导。`lqr` 0.2.0 的 `10³·u·‖H‖₁` 中 ‖H‖₁ 由 ‖BR⁻¹Bᵀ‖ 主导，R = diag(1, 1e-12) 的良态解耦问题被误报（tpt-control 交叉验证发现）；0.3.0 改为在辛对角平衡（状态尺度变换）后的 H̃ 上判断，且只把"接近虚轴"当作怀疑，用直接判据（PBH 不可观检验）确认或求解后检验闭环。
- X-4b **规模类错误要区分**：`COMPUTE_LIMIT`（计算量超限，remediation 改粗网格）与 `OUTPUT_TOO_LARGE`（计算可做但返回体过大，remediation 抽样或只返回指标）MUST 分开，因为两者的有效修复不同。

### 5.2 错误对象（`control.design.lqr` 的实际错误输出，节选）

```json
{
  "code": "E.DOMAIN.NOT_STABILIZABLE",
  "category": "domain",
  "retryable": false,
  "message": "(A,B) 不可镇定：特征值 λ = 1.000000+0.000000j（Re ≥ 0）对应的模态不受输入影响……",
  "agent_hint": "这是模型结构问题，调整 Q/R 无效。需要为该模态增加执行器（修改 B），或确认 A 是否录入有误（含行列顺序：row-major）。",
  "details": { "path": "/B", "uncontrollable_eigenvalue": [1.0, 0.0], "pbh_min_singular_value_rel": 0.0, "pbh_test": "σ_min([A−λI, B]) / ‖[A, B]‖_F ≤ 1e-10" },
  "remediation": [
    { "id": "add_actuator", "kind": "modify_input", "patch": [{ "op": "replace", "path": "/B", "value": "…" }, { "op": "replace", "path": "/R", "value": "…" }],
      "confidence": "low", "requires_user_confirmation": true, "description": "示例：为每个状态各加一路输入（B = I，R = I），仅用于确认问题出在执行器配置" },
    { "id": "ask", "kind": "ask_user", "confidence": "high", "requires_user_confirmation": true, "description": "…" }
  ]
}
```

- X-5 `remediation[].kind` ∈ `modify_input`（MUST 带 RFC 6902 `patch`，作用于原请求 arguments）、`relax_tolerance`、`regularize`、`call_operator`（MUST 带 `operator`，参数用 `arguments_from` 以 JSON Pointer 引用原请求）、`retry_same`、`ask_user`。
- X-6 凡改变物理模型含义的修复（改 A/B、改系统阶次、加滤波器、改 Q）MUST 设 `requires_user_confirmation: true`。只影响数值处理或输出规模的修复（换网格、抽样、排序频点）MAY 为 false。
- X-7 `remediation` 按推荐度排序；MUST 至少一条（实在无建议时给 `ask_user`）。
- X-8 `details.path` SHOULD 用 JSON Pointer 指向出错的输入字段。
- X-9 **** `call_operator` MUST 引用同一注册表中已存在的算子（v0.1 示例引用了不存在的 `control.lti.ctrb_decomp`，已删除）。`modify_input` 的 patch 应用后 SHOULD 能通过校验；参考实现对 `step` 的 `COMPUTE_LIMIT` 示例测试了"应用 patch 后调用成功"。

## 6. 可复现性

- P-1 **版本**：`build` 字符串 MUST 包含 crate 版本、git commit（无仓库时写 `nogit`）、主要数值依赖版本、编译器版本。任何一项变化都视为不同 build。
- P-2 **哈希**：`input_hash` = SHA-256（RFC 8785 JCS 规范化后的请求 arguments，去掉 `exec`，数组按 3.7 替换为内容摘要）；`output_hash` 同理作用于 `result`。浮点数序列化 MUST 使用 JCS 的 ECMAScript 数字格式（最短往返表示）。
- P-3 **线程与 BLAS**：`bitwise` 算子 MUST 单线程运行或使用确定性归约顺序；BLAS/LAPACK MUST 锁定版本与内核，MUST NOT 使用运行时动态分派到未声明的内核——做不到时 MUST 声明为 `platform_bitwise`，并在 `reproducibility.cpu_features` 中记录分派依据（v0.2 参考实现即属此类，`blas: "none"`，单线程）。
- P-4 **浮点策略**：MUST NOT 启用 fast-math 类优化（重结合、flush-to-zero、近似倒数）。Rust 默认不做 FMA 收缩；C/Fortran 后端编译时 MUST 加 `-ffp-contract=off`（或等价选项）。
- P-5 **随机性**：随机算法 MUST 为 `seeded`，PRNG 算法固定（建议 ChaCha20），`seed` 回显在 `reproducibility.seed`。
- P-6 **容差**：算子内部的收敛容差 MUST 写在 manifest（含默认值）；允许调用方覆盖的容差（如 `lqr.symmetry_tolerance`）MUST 出现在 input schema 中并回显。
- P-7 **重放**：同 `build` + 同平台（2.1，含 `libm`）+ 同 `input_hash` 的两次调用，`output_hash` MUST 相同。CI MUST 有"同机重放"测试（参考实现：`deterministic_repeat_calls_bitwise_equal`，以及示例响应的 `output_hash` 回归比对；另实测 debug 与 release 构建的三个示例 `output_hash` 相同）。
- P-8 **无隐藏状态**：算子 MUST 是纯函数，不得读取环境变量、时钟（`timing` 除外）或全局可变状态影响结果；文件读取仅限 3.6 的 `ref` 输入，且受 data root 约束。

## 7. 验证凭证（Verification Credentials）

### 7.1 生成与存储 golden 数据

- V-1 参考数据来源优先级：① MATLAB（ground truth，正版授权）② 解析解/高精度计算（如 mpmath）③ python-control/scipy、GNU Octave 等开源实现（开发阶段对照，不能单独授予 A 级）。
- V-2 MATLAB 侧只运行公开 API 的生成脚本（`matlab/gen_golden_<op>.m`），固定 `rng(seed)`，记录 `version`、工具箱版本、`version('-blas')`、`version('-lapack')`、`computer('arch')`、`maxNumCompThreads`。脚本 MUST 遵守第 0 章内存序规则。
- V-3 **** golden 目录包含参考引擎名：`golden/<engine>/<op>/<op_version>/<sha256>.json`（如 `golden/python-control/…`、`golden/MATLAB/…`），以免不同参考的数据混在一起被误当作 MATLAB 结果。文件名 = 内容 SHA-256。每个文件 MUST 带 `reference_env`（引擎与版本）。golden 文件 MUST 纳入版本库或制品仓库，MUST NOT 在 CI 中重新生成。
- V-3a **golden 的版本归属**：`<op_version>` 是生成该 golden 时的算子版本。算子升级而 input schema 与输出语义不变时，新版本 MAY 继续使用旧版本目录下的 golden（参考值与算子版本无关，MUST NOT 为改目录名而重新生成或改写文件），parity.yaml MUST 列出实际使用的 `golden_store`；为新版本新增的用例写入新版本目录。v0.3.6 补充：只**增加**可选输入或输出字段的 MINOR 升级（G-2），只要旧请求的结果在旧字段上逐位不变（参考实现：位置参数调用的 `series`/`feedback` 0.4.0），同样 MAY 沿用旧目录；新增输入形式（如具名角色）的用例 SHOULD 写入新版本目录或由单元测试覆盖。高精度参考（V-1 ②）用引擎名 `mpmath`：`golden/mpmath/<op>/<ver>/`，`reference_env` 写明 mpmath 版本、精度位数与求解方法。
- V-4 parity 测试 MUST 使用显式输入（显式频点、显式时间网格），MUST NOT 依赖参考实现的自动网格/自动选择逻辑。
- V-5 非唯一输出（特征向量、状态空间实现、Schur 基）MUST 比较不变量（特征值集合、传递函数、投影子），不得逐元素比较。
- V-6 比较公式：|x − x_ref| ≤ atol + rtol·|x_ref|；矩阵 SHOULD 用范数形式。**（v0.2 细化）** 病态问题的容差 MAY 按以下两种方式放宽，且 MUST 写入 parity spec、MUST 在测试报告中单独统计被放宽的用例数：
  - 条件数缩放：`rtol × max(1, κ/10²)`（`lqr` 用 κ = cond(S)）；
  - 误差估计缩放：`max(class_tol, 10 × trust.error_estimate)`——只有当算子自报的误差估计本身已经较大时才放宽（如 `bode` 的近极点频点）。这同时检验了误差估计不是"夸大的好消息"：实际偏差必须落在算子自报估计的 10 倍以内。
  - **根与系数的条件数缩放**：多项式根/特征值逐个按最近匹配比较，容差 = class_tol × max(1, κᵣ)，κᵣ = Σ|aₖ||r|^{n−k} / (max(1,|r|)·|p′(r)|)（参考多项式的 Wilkinson 相对条件数）；由特征值展开得到的多项式系数，容差 = class_tol × max(1, Π(1+|λᵢ|)/‖p‖₁)（系数对特征值扰动的一阶界）。DC 增益用 κ = Π(1+|p|)/Π|p|；Tustin 离散化用 κ = cond(I − αA)；zoh/foh 用 κ = e^{‖A‖₁·T}。
- V-5a **非唯一的设计结果**：多输入极点配置的 K 不唯一，MUST NOT 与参考的 K 比较；parity 测试 MUST 用**参考请求中的 A、B**（而不是算子自报的 `achieved_poles`）重算 eig(A − B·K) 并与期望极点比较——否则把 A 按 column-major 传错的请求也会"成功配置极点"，内存序错误无法检出。闭环特征向量矩阵的条件数与参考（scipy YT）之比作为信息报告。
- V-7 预期报错用例 MUST 比对错误码；预期 warning 用例 MUST 断言 warning 出现。
- V-9 **参考值的后处理** MUST 写入 parity spec 并说明原因。例：python-control（无 slycot）用 scipy QZ 计算零点时，会把无穷远广义特征值返回成 ~10¹⁶–10¹⁸ 的有限数；参考生成器丢弃 |z| > 10¹⁰·(1+‖[A B; C D]‖_F) 的"零点"。
- V-10 **参考本身是局部优化结果时**（python-control 离散 `stability_margins` 用 scipy L-BFGS-B 求 min|1+G|），比较规则为：在类容差内一致，或被测值更小（更好的极小值）且参考值与在参考频率处重新求值的 |1+G| 一致（≤ 1e-9）、差距 ≤ 1e-6；频率按优化器精度比较（rel 1e-3）。这类规则 MUST 在报告中单独标注。

### 7.2 容差类

| 类 | 算法类型 | 默认判据 | 说明 |
|---|---|---|---|
| T0 | 精确/整数/组合 | 完全相等 | 秩、阶次、结构判定 |
| T1 | 直接稠密线性代数 | rtol 1e-12（× κ 缩放） | 求解、分解、乘法 |
| T2 | 矩阵函数、特征值、Riccati/Lyapunov、LTI 精确离散化 | rtol 1e-10（× κ 缩放） | `expm`、`lqr`、`dlqr`、`lyap`、`c2d`、`place`、`poles_zeros`、ZOH `step`/`impulse`、`feedback`（含代数环的线性求解）、`stepinfo` 的轨迹 |

派生指标与近似不另设容差类（v0.3.5，I-10、A-1）：`stepinfo` 的时间量是网格定义下的 T0（同一网格上相等；相对连续穿越的 ≤ dt 写在 `time_quantization_s`，不放大 T2 的 rtol）。`pade` 与 `pid`、`series`、`parallel` 的系数是直接算术，容差类 T1；Padé 对 e^{−θs} 的偏差在 `approximation`，不占 T1。
| T3 | 频率响应求值及其导出量 | 幅值 1e-9 dB abs，相位 1e-8 deg（mod 360）；裕度：GM 与交越频率 rel 1e-9，PM 1e-8 deg | `bode`、`margin`、`nyquist`、`freqresp` |
| T4 | 变步长 ODE / 一般仿真 | rtol 1e-6、atol 1e-9，或"求解器容差 × 10" | 参考与被测 MUST 用相同容差设置 |
| T5 | 迭代优化 | KKT 残差 ≤ tol；目标值 rtol 1e-6 | `quadprog` 类、NLP |
| T6 | 随机算法 | 统计检验或固定 seed 下的 T1–T5 | 蒙特卡洛 |

### 7.3 精度等级

| 等级 | 条件 |
|---|---|
| A | 对 MATLAB golden 全部通过（每个 case family ≥ manifest 声明数量，含边界/病态/报错用例），且不变量检查全部通过 |
| B | 对解析解或高精度参考全部通过，或对开源参考（python-control/Octave）全部通过 + 不变量检查；尚无 MATLAB 对照 |
| C | 只有不变量/自洽性检查 |
| U | 未验证（实验性）；生产注册表 MUST NOT 默认暴露 U 级算子 |

等级要有凭证（7.4）支撑。没有凭证的实现（例如刚写成的第二实现）MUST 报 C 或 U，即使它已通过一致性套件：一致性证明的是与参考实现等价，不是等级本身。参考实现的算子当前都是 B 级，目标 A 级。

### 7.4 凭证记录

参考实现的 parity 测试会写出 `reports/parity-python-control.json`（v0.2 算子）与 `reports/parity-python-control-b2.json`（v0.3 新增算子）（每个算子：用例数、通过数、各量的最大误差与容差、各 case family 统计、被放宽的用例数、内存序回归结果、build、cpu_features）。它是凭证的原始材料；正式凭证还需要 MATLAB golden 与 CI 签名。

- V-7 凭证 MUST 与 `build` 绑定；任何后端或数值依赖升级 MUST 重新跑 parity 并生成新凭证。
- V-8 凭证 SHOULD 由 CI 签名（如 Sigstore），平台展示"verified"标识前 MUST 校验签名。

## 8. 映射到 MCP 与绑定层

### 8.1 manifest → MCP tool

| MCP 字段 | 来源 | 规则 |
|---|---|---|
| `name` | `manifest.mcp.tool_name` | 由 `id` 把 `.` 换成 `_`（`control_lti_bode`），只用 `[a-zA-Z0-9_-]` |
| `title` | `manifest.title` | |
| `description` | `description` + 精度等级 + 容差类 + 确定性 | 面向模型写 |
| `inputSchema` | `manifest.input_schema`（`$ref` 全部内联） | MUST 完全展开 `$ref`，因部分客户端不解析外部引用 |
| `outputSchema` | 信封 schema，`result` 替换为 `oneOf [null, output_schema]` | 错误信封的 `result` 是 null，所以 MUST 允许 null |
| `annotations` | `readOnlyHint: true`、`idempotentHint: true`、`openWorldHint: false` | 纯计算算子 |
| `_meta` | `aeoi/id`、`aeoi/version`、`aeoi/grade`、`aeoi/spec`、`aeoi/manifest` | 保留原始 id |

- C-1 算子层错误（`E.*`，含 schema 校验失败）MUST 作为正常 tool result 返回，`isError: true` 且 `structuredContent` 为错误信封；MUST NOT 用 JSON-RPC 协议错误，否则 Agent 看不到 remediation。JSON-RPC 错误只用于协议层问题（未知工具 `-32602`、未知方法 `-32601`、解析失败 `-32700`、资源不存在 `-32002`）。
- C-2 server SHOULD 暴露资源：`aeoi://operators`（目录）、`aeoi://operators/{id}/{version}/manifest`、`…/credentials`、`aeoi://schemas/{name}/{ver}`。（参考实现已提供目录、manifest 与 schema；credentials 资源尚未提供。）
- C-3 大型输出 SHOULD 用 `exec.output_encoding = "b64"`（L-5）；超过 `limits` 时返回 `E.RESOURCE.OUTPUT_TOO_LARGE` 并给出抽样/只返回指标的 remediation。基于 resource link 的输出留待 v0.3。
- C-4 工具数量多时，server SHOULD 支持按命名空间过滤（参考实现：`aeoi-mcp --namespace control.lti`）。
- C-6 **** server SHOULD 提供只读工具 `aeoi_verify`（4.3 R-5/R-6），参数 `{operator, arguments, envelope}`，返回 verdict 对象（`structuredContent`；不通过时 `isError: true`）。它不是算子，没有 manifest；`tools/list` 中以 `_meta["aeoi/kind"] = "verifier"` 标出。
- C-5 `initialize` 的 `protocolVersion` 协商：参考实现支持 `2025-06-18`、`2025-03-26`、`2024-11-05`，客户端请求其他版本时回落到 `2025-06-18`。

### 8.2 Rust 核心 trait

见 `docs/reference-impl.md` 8.2。

### 8.3 绑定层

- B-1 所有绑定 MUST 返回同一信封结构；绑定层 MUST NOT 自行改写数值或错误码。
- B-2 Python 绑定 MUST 释放 GIL 执行长计算。

各绑定的实现状态见 `docs/reference-impl.md` 8.3。

### 8.4 LLM 函数调用绑定

把算子交给 LLM 的函数调用接口时，绑定层按下列规则生成函数声明、核对来源并管理回合。参考实现 `aeoi-web`（测试台、评测与结果）见 `docs/reference-impl.md` 8.4。

**函数声明**：`name` = `manifest.mcp.tool_name`；`description` = `[id] description` + 前提条件 + 不适用范围（+ "仅 SISO"）；`parameters` 由 input schema 两步生成：

1. 收窄与精简（所有 provider，F-6）：`Matrix` 只保留 `DenseMatrix` 分支并固定 `dtype: "f64"`、`shape` 为二维；去掉 `exec`、`time_unit`、`state_names`/`input_names`/`output_names`、`structure`；`x-aeoi-shape-constraints` 并入 description（F-2）。OpenAI 兼容 provider 直接使用这一步的 JSON Schema。
2. 转换到 provider 支持的 schema 子集。以 Gemini 的 OpenAPI 子集为例，映射如下；有损的映射 MUST 只让模型看到的约束变少，算子仍按完整 input schema 校验：

| JSON Schema | Gemini `parameters` | 是否有损 |
|---|---|---|
| `type`、`description`、`properties`、`required`、`items`、`minItems`/`maxItems`、`minimum`/`maximum`、`minLength`/`maxLength`、`pattern` | 原样 | 否 |
| 字符串 `enum` / 字符串 `const` | `type: string` + `enum`（`const` 变为单值 enum） | 否 |
| 非字符串 `const`/`enum` | 写进 description（"必须等于 …"） | 是 |
| `type: [T, "null"]` | `type: T` + `nullable: true` | 否 |
| `oneOf` | `anyOf`（判别字段的单值 enum 仍能区分分支） | 是（互斥性） |
| `prefixItems`（复数 `[re, im]`） | 同构 `items` + `minItems = maxItems = 2` | 是（逐位类型） |
| `exclusiveMinimum`/`exclusiveMaximum` | `minimum`/`maximum` + description "必须 > x" | 是（边界值本身） |
| `default` | 写进 description（"默认 …"） | 是 |
| `additionalProperties: false` | 不表达 | 是（由算子 schema 兜底） |
| 只有 `additionalProperties` 的自由映射 | 整个字段对模型隐藏 | 是 |
| 数/复数/布尔/非有限值的标量联合 | 收窄为 `number` | 是 |
| 其他关键字（`$schema`、`title`、扩展关键字） | 丢弃 | 是 |

**回合预算（AL = agent loop）**：模型常在已经拿到结果后仍不提交，直到回合用完（v0.3.6 的真模型运行里 7 题有 4 题如此）。
- AL-1 有回合上限、且只能通过某个工具提交答案的循环（设计评测的 `submit_design`）MUST 在系统提示词里写明上限（“最多 N 个模型回合，最后一个回合只能提交”）。
- AL-2 剩余 2 个回合（含本回合）时，绑定层 MUST 注入一条以“（系统提示）”开头的提示，要求在本回合或下一回合提交；最后一个回合 MUST 只提供提交工具，并再注入一条“这是最后一个回合，只能提交，在说明里写明哪些指标没有用工具确认”。这些注入不算用户原话（F-5(c) 的来源核对排除它们）。`max_steps < 3` 时不发第一条提示。
- AL-3 绑定层 MUST NOT 替模型提交（例如把最后一次调用的参数当答案）：提交的内容必须是模型自己写的。报告 MUST 记录注入次数（`budget_notices`）。没有提交仍记为未提交、0 分。

**来源核对（F-5(c)）**：对 `x-aeoi-provenance: "user"` 的参数（v0.3.1 只有 `c2d.ts`），函数声明多一个必填参数 `_user_quote`，工具描述末尾写明"必须由用户给出、不得假设的参数：ts（用户没给时先询问，不要调用本工具）"。绑定层在调用算子前核对引用：把用户本人的消息（不含"（系统提示）"开头的注入提示）与引用都去掉空白、`$`、`{}` 等 LaTeX 符号后做子串匹配，并要求引用中有与参数相同的数值（`ms`/`μs` 按单位换算）。不通过则返回 `E.VALIDATION.UNGROUNDED_INPUT`（`details.field/value/quote`，remediation 只有 `ask_user`），不调用算子。直接在网页上调用算子（`/api/call`）与 `rescore` 重算旧报告时不做核对，只移除 `_user_quote`。

### 8.5 浏览器绑定

见 `docs/reference-impl.md` 8.5。

## 9. 治理

### 9.1 合规等级

| 等级 | 要求 | 用途 |
|---|---|---|
| L1 Basic | manifest、严格 input/output schema、信封、结构化错误（含 remediation） | 实验性接入 |
| L2 Trusted | L1 + 完整 trust 元数据 + 可复现性（第 6 章全部 MUST）+ 重放测试 | 生产注册表最低要求 |
| L3 Verified | L2 + 验证凭证等级 A 或 B + CI 签名 + parity 测试持续运行 | 平台"verified"标识 |

参考实现 18 个算子当前为 L2（缺 CI 签名与凭证资源，故未到 L3）。

### 9.2 版本与弃用

- G-1 破坏性变更（MAJOR）：删除/重命名输入输出字段、改变默认值导致结果变化、改变单位、收紧 schema、改变错误码含义、tolerance class 变宽。
- G-2 MINOR：新增可选输入、新增输出字段、新增错误码/warning、精度提升。
- G-3 PATCH：修 bug 且结果仍在原容差类内、性能优化。若修复导致结果超出原容差，按 MINOR 处理并在 changelog 说明。
- G-4 0.x 版本按 SemVer 约定，MINOR 也可破坏（v0.1 → v0.2 即为破坏性升级，见第 13 章）。
- G-5 弃用：manifest 设 `deprecated {since, replaced_by, removal_after}`；调用时 MUST 返回 `W.LIFECYCLE.DEPRECATED`；弃用期 ≥ 2 个 MINOR 版本或 90 天，取较长者。
- G-6 注册表 MUST 允许同一 `id` 的多个 MAJOR 版本并存；MCP tool 默认指向最新稳定 MAJOR。

### 9.3 新算子接入清单

1. 选定 `id`；写出数学定义与前提条件
2. 列出公开文献（含 DOI），声明净室来源；确认后端许可证
3. 写 manifest（含 `sparse_support`、`encodings`、`limits`）、input/output schema（`additionalProperties:false`、单位、范围）
4. 列出全部错误码与 warning，每个错误码至少一条 remediation；数值判据与尺度无关（X-4a）
5. 实现 `validate()`、`execute()`、`estimate()`
6. 实现 trust 字段（残差/条件数/误差估计及其 basis；`checks` 只放布尔值）
7. 写 parity spec：case families（MUST 含 `memory_order_nonsymmetric`）、容差类、比较模式、预期报错/warning 用例
8. 用开源参考生成 `golden/<engine>/…` 开发数据；写 MATLAB 生成脚本，由有授权的机器运行生成 `golden/MATLAB/…`
9. CI：schema 校验（Rust 与独立的 Python 校验器）、单元测试、重放测试、parity 测试、MCP 端到端测试、跨平台测试
10. 评审：数值评审 + 接口评审（错误信息与 remediation 是否可执行）
11. 写算子契约 `profiles/10.NN-<op>.md`（语义、规范性条款、示例与 parity 规格的链接），在第 10 章的索引表里加一行；它逼出的通用规则写进本文相应章节
12. 在 `CHANGELOG.md` 记一节（起因、规范改动、已做的改动、发现的问题、结果）
13. 生成凭证、确定等级与合规等级、发布

### 9.4 跨实现一致性

9.1 的合规等级评价一个算子实现自身的质量（schema、信封、可复现、凭证）。本节回答另一个问题：**换一个实现，行为是否等价于参考实现。** 两者独立：一个实现可以是 L2 却通不过本节的套件，反过来也一样。本节规定判定规则和套件的组成；套件 v0.2（`conformance/`：运行器、独立的哈希重算、只含 `lyap` 的 Python 第二实现）有 6 个算子的数值规则（`lyap`、`poles_zeros`、`bode`、`step`、`lqr`、`c2d`），参考实现全部通过（`conformance/README.md`）。

- CF-1 **对象与形态。** 一致性有两类对象：算子实现（CF-1 – CF-11），以及把算子暴露给模型的绑定层（CF-12）。算子实现有两种接入形态，与 9.1 的 L1 – L3 无关：
  - **线级（wire）**：独立的可执行体，通过 MCP stdio 暴露算子（8.1、C-1），运行器只与它交互。任何语言都能做到。
  - **内核级（kernel）**：只提供数值内核，挂到宿主（如 `aeoi-core`）里，校验、收据与指纹仍由宿主负责。内核接口尚未定义（12.1 第 17 项）。它不是 8.3 的 `aeoi-capi`：后者是“调用 AEOI 的 C ABI”，把整个算子包成 JSON 进、JSON 出，不是把内核插进宿主的接口。

  一致性按（算子 id、版本、方法 id）逐项声明（CF-11）；没有声明的算子，实现不得宣称遵守 AEOI。
- CF-2 **套件沿用 golden 与 `parity.yaml`，不另立用例格式。** 一个用例就是 `golden/<engine>/<op>/<ver>/<sha256>.json`（`request`、`expected`、`reference_env`、`case_family`）；用例族的容差类、`compare` 规则、预期错误与预期警告写在该算子的 `parity.yaml`（V-4 – V-10）。套件 = 这两者 + CF-10 的方法用例。含矩阵输入的算子 MUST 带 `memory_order_nonsymmetric` 族（第 0 章）：运行器断言按列主序展平的请求，其结果偏离参考超过容差：取该算子全部数值检查里最大的“偏差 / 容差”，须 > 1e3（或请求被拒绝）。只看一个字段可能看不出错误：eig(Aᵀ) = eig(A)，极点不动，零点与 DC 增益会动。这是对第二实现最便宜也最有效的检验。
  - **默认值用例。** 运行器从成功的 golden 用例派生，不另存：对套件 input schema（`examples/<op>/input.schema.json`，不是被测实现的 manifest）里每个带 `default` 的顶层字段，显式写了默认值的用例派生“省略它”的变体，省略了的用例派生“写出默认值”的变体（每个算子、字段、方向至多 3 个）。两者问的是同一个问题（F-7），结果 MUST 与原请求逐位相同（`output_hash` 相等，`convention` 字段的回显也包括在内）。写出的字段在该请求的模式下不适用、实现按 F-8 报 `E.VALIDATION.OPTION_CONFLICT` 且 `conflicting_fields` 含该字段时，记为跳过。嵌套字段的默认值（如 `time` 内部）暂不派生。
- CF-3 **通信与运行。** 运行器通过 MCP stdio 驱动被测实现，只做这一种（`initialize`、`tools/list`、`tools/call`、`resources/read`）；manifest 由资源 `aeoi://operators/{id}/{version}/manifest`（M-1）读取。非 MCP 的传统程序要通过线级套件，需要自带一层薄的 MCP stdio 适配。运行器命令（v0.1 已实现，只覆盖 `lyap`，见 `conformance/README.md`）：`python3 conformance/aeoi_conformance.py run --impl "<启动命令>" --operators all|<id,…> --report report.json`。对声明 `bitwise`、`platform_bitwise` 或 `seeded` 的算子（M-7），运行器对每个请求运行两次，`output_hash` MUST 相同（P-7）。
- CF-4 **必须相同的字段。** 信封的 `status` 是否为 `error`；`error.code`（V-7）；用例 `expect_warning` 列出的 warning 码 MUST 出现（V-7）；算子 id 与版本；`fingerprint.request`。最后一项成立的原因：它是 `H({operator, version, input_hash})`，而 `input_hash` 与编码无关（H-1），所以任何实现对同一请求 MUST 算出相同的值。
- CF-5 **不要求相同的字段。**
  - `ok` 与 `ok_with_warnings` 的区别。基于阈值的 warning 会在不同实现、不同平台上翻转（8.5），所以只核对 CF-4 列出的 warning，其余不作要求；E-2 保证 `status` 只由 warnings 是否为空决定，不需要另核。
  - `fingerprint.result` 与 `output_hash`。不同内核的末位舍入不同，不要求相同（4.3 R-3）；要求的是 P-7（同一实现、同一平台、同一请求，重复运行相同）。
  - 方法 id：不要求等于参考实现的方法 id（MS-1、MS-2）。
  - `timing`、`build`、`platform`、`cpu_features`。
- CF-6 **数值比较。**
  - 沿用 `parity.yaml` 的 `compare`：对每个顶层输出字段按相对范数（`rel_fro`，标量退化为相对误差，向量为 2-范数）；字符串、布尔、键与数组长度必须完全相同。**多项式系数同样按范数比较，不逐元素。**
  - 某个字段在 `parity.yaml` 里有自己的写法时（如 `bode` 的 dB 与度按绝对差、相位模 360；`step` 的 y 按相对 max|y_ref| 的最大差），以 `parity.yaml` 为准，上一条的 `rel_fro` 只是没写时的默认。运行器实际执行的公式 MUST 写在该算子 `parity.yaml` 的 `conformance_compare` 里，使套件不必读运行器代码也能复现。
  - 无序量（极点、零点、特征值）按 V-5 与 3.10 R-2 做集合匹配；状态空间实现、特征向量、Schur 基比较不变量（V-5）；多输入 `place` 的 K 用 V-5a；`stepinfo` 的时间量按 I-10。
  - 容差类与 κ 缩放由套件固定在 `parity.yaml`（7.2、V-6）；被测实现 manifest 里的 `tolerance_class` 只是它的自我声明，MUST NOT 用来决定验收容差（否则声明一个更宽的类就能通过）。**κ 由运行器用参考请求或参考值计算，不采信被测实现自报的任何量**（`trust.error_estimate`、`trust.condition`、`trust.derived` 都不用）。 V-6 中“`max(class_tol, 10 × trust.error_estimate)`”这种放宽只对参考实现开放；第二个实现自报的 `error_estimate` MUST NOT 用来放宽验收，否则它把估计报大就能通过任何用例。
  - 高精度参考（如 mpmath）与其他参考引擎用同一个 κ 缩放容差：它们提供更准的参考值，不提高要求。`parity.yaml` 中为参考实现自己写的更严目标（如“1e-12 不放宽”）不是一致性门槛；`conformance_compare` 写明一致性实际用的容差。
  - 状态空间实现的不变量（v0.4.2，以 `c2d` 为例）：运行器在自己选的点（离散系统取单位圆上若干点，避开参考极点）上比较传递函数值，再比较极点集合与 D；A、B、C 本身不比较，因为相似变换得到的实现同样正确。返回传递函数时，分子分母先同除以分母首项系数（二者只确定到一个公共因子）再比较系数。
  - 4.3 R-5 的 `verify` 用 ‖·‖∞ 做重放比较，`parity.yaml` 用 `rel_fro`；两种范数并存，一致性套件用后者（12.1 第 14 项）。
- CF-7 **验收容差与自报误差界。** 验收容差由套件决定（CF-6），是“合规下限”。被测实现的收据里 `exactness.bound_kind` 为 `rigorous` 或 `a_priori` 且 `bound` 是数值时，`bound` MUST 不大于该用例的验收容差，否则声明本身不合规；`a_posteriori_heuristic` 与 `none` 只作信息记录，通过与否只看实际偏差。例外（v0.4.21，PY-64）：误差界是逐元素的包含区域（如 RT-4 的包含半径，`bound` 是半径而不是全局误差）时，不与验收容差比较大小，而是由规则核查声明本身：每个真值 MUST 落在声明的区域里（claim check）；这比“界不大于容差”更强，也适用于根簇这种半径本来就远大于容差的情形。个别用例上确有理由的更宽偏差 MUST 登记在声明的 `known_deviations`（用例、实际偏差、原因），记为“带偏差通过”，不计入“完全通过”。典型情形：病态问题上，没有精化的后向稳定算法的前向误差超出 κ 缩放容差，而运行器独立复核的后向误差（如 `lqr` 的 CARE 残差）合格（12.1 第 23 项）。运行器用 `--known-deviations` 读入；只有数值偏差可以登记，结构性失败（形状、错误码、指纹）不行。
- CF-8 **收据与 manifest 的机械核查。** 运行器不采信自报：
  - manifest 通过 `schemas/manifest.schema.json`（运行器逐个校验；不通过时该算子的全部用例判失败），`provenance.clean_room.declared` 为 true（第 0 章）；响应通过 envelope schema 与该算子的 output schema（`parity.yaml` 的 `pass_rule`）。
  - 响应里出现的每个 `E.*`、`W.*` 码 MUST 在该 manifest 中声明（E-7、5.1；框架级码见 E-7a）。
  - `receipt.evidence.claim` 由运行器按 E-11、E-12，用 manifest 的 `receipt.postconditions` 与 `coverage`、响应的 `trust.checks` 重新推出，MUST 与响应一致：后置条件为 false 时不能声称 `verified`。
  - 收据的 `method.id` MUST 属于 manifest 允许的方法集合（MS-1）。
  - （推荐）对 `lyap`、`lqr`、`place`、`poles_zeros` 等，运行器用与被测实现不同的代码路径独立复核后置条件（残差、闭环极点位置），做法参照 `scripts/validate_examples.py` 对 E-12、E-13 的独立核对。v0.4.2 已对 `lqr` 做了：由返回的 S、K 重算 CARE 残差、K 与 S 的一致性、S 的对称性、A − BK 的稳定性。残差按各项大小归一（后向误差形式），不对 R 求逆：否则 cond(R) 到 1e12 时运行器自身的舍入就会造成误判。
- CF-9 **错误、边界与 remediation。** 套件除正常路径外 MUST 包含：manifest 声明的每个错误码至少一个触发用例（`parity.yaml` 的 `expect`）；保护性拒绝（如 `series` 的相对阶上限，I-7）；带 `modify_input` remediation 的错误，运行器把补丁套到请求上重新调用，结果 MUST 通过校验（X-9）。
- CF-10 **方法用例。** 对 manifest 里每个已登记的方法（MS-1），运行器 MUST 以 `exec.method_policy = {"require": ["<该方法 id>"]}` 跑该算子的全部用例，每个方法单独计数（MS-3）；声明了方法却没被检验的实现不能通过。方法选择另有用例：缺省选 `default`；`require` 已登记的方法成功且 `method_selection.used` 相符；`require` 未登记的 id、无解的 `constraints` 返回 `E.VALIDATION.METHOD_UNAVAILABLE` 且 `details` 列出可用方法；含 `method_policy` 的请求发给不支持它的实现得到 `E.VALIDATION.SCHEMA`（MS-8）。只有一个方法的算子只需通过缺省、`require` 未登记 id、`require` 该唯一方法这三项。这些用例用 golden 同样的文件格式，`request.exec` 带 `method_policy`；**不同方法的结果都 MUST 落在同一个验收容差内**，这正是对“换方法不改变结果”的检验。
- CF-11 **声明与报告。** 运行器产生报告，实现发布机器可读的声明，至少包含：实现名称、版本、形态（线级或内核级）；套件版本与 `suite_hash`；逐（算子、版本、方法 id）的结果（`pass`、`pass_with_deviation`、`fail`、`skip`）；`known_deviations`；通过用例按 `reference_env.engine` 的计数，以及该引擎是否与被测方法的 `backend`（M-6）同源。同源的判定是机械的：用例 `reference_env.engine` 里出现的库例程名（带点的名字，如 `numpy.fft`、`scipy.integrate`；`python-control` 展开为 `python-control`、`scipy.linalg`、`slycot`；以及 `mpmath`、`matlab`），只要有一个作为完整词出现在被测方法的 `backend.library` 或 `backend.routine` 里，就算同源。因此实现 MUST 在 `backend` 里如实写出数值核心所用的库例程；所用内核与数值库及其版本。同源的参考值只能证明套件可移植，不能证明实现可替换，所以声明 MUST 把“同源通过”和“独立通过”（mpmath、解析解，V-1 ②）分开列出。

  ```json
  { "suite_version": "…", "suite_hash": "sha256:…",
    "impl": { "name": "…", "version": "…", "form": "wire" },
    "results": [ { "operator": "control.math.lyap", "version": "0.4.0", "method": "<方法 id>",
                   "pass": 0, "pass_with_deviation": 0, "fail": 0, "skip": 0,
                   "by_engine": { "python-control": { "n": 0, "same_backend": true },
                                  "mpmath": { "n": 0, "same_backend": false } } } ],
    "known_deviations": [] }
  ```
- CF-12 **绑定层一致性。** 另立套件，不属于算子实现。检验：函数声明由 manifest 与 input schema 生成且有损映射可见（8.4）；F-5(c) 的来源核对与 `UNGROUNDED_INPUT`；F-7 的 `convention` 回显；F-8 与 K-6（系统不抽样）；AL-1 – AL-3；MS-10、MS-11 的策略合并。本节不展开。

## 10. 算子契约

每个算子的语义、约定、规范性条款（如 FT-1、OD-1）、示例与 parity 规格各占一个文件，放在 `profiles/`。它们与本文同样是规范性的；条款编号在全规范内唯一，正文里写“10.17 FT-3”指 `profiles/10.17-fft.md` 的 FT-3。新算子接入时新增一个文件（9.3）。每个 profile MUST 在开头写明所含算子的版本号与容差类：版本号是线上契约的一部分（`fingerprint.request` 含版本号，4.3 R-3），只读规范的实现要能写出与参考实现相同的指纹。

`bode` 与 `step` 使用同一被控对象，`lqr` 使用经典双积分器；v0.3 新增算子（10.4–10.12）的示例尽量复用同一被控对象，并都给出可手算核对的值。

被控对象：G(s) = 4 / (s² + 1.2s + 4)（ωn = 2 rad/s，ζ = 0.3），极点 −0.6 ± 1.9079j。

| 节 | 算子 | 文件 |
|---|---|---|
| 10.1 | `control.lti.bode` | [`profiles/10.01-bode.md`](profiles/10.01-bode.md) |
| 10.2 | `control.lti.step` | [`profiles/10.02-step.md`](profiles/10.02-step.md) |
| 10.3 | `control.design.lqr` | [`profiles/10.03-lqr.md`](profiles/10.03-lqr.md) |
| 10.4 | `control.lti.convert` | [`profiles/10.04-convert.md`](profiles/10.04-convert.md) |
| 10.5 | `control.lti.poles_zeros` | [`profiles/10.05-poles_zeros.md`](profiles/10.05-poles_zeros.md) |
| 10.6 | `control.analysis.ctrb_obsv` | [`profiles/10.06-ctrb_obsv.md`](profiles/10.06-ctrb_obsv.md) |
| 10.7 | `control.analysis.margin` | [`profiles/10.07-margin.md`](profiles/10.07-margin.md) |
| 10.8 | `control.lti.c2d` | [`profiles/10.08-c2d.md`](profiles/10.08-c2d.md) |
| 10.9 | `control.lti.impulse` | [`profiles/10.09-impulse.md`](profiles/10.09-impulse.md) |
| 10.10 | `control.math.lyap` | [`profiles/10.10-lyap.md`](profiles/10.10-lyap.md) |
| 10.11 | `control.design.place` | [`profiles/10.11-place.md`](profiles/10.11-place.md) |
| 10.12 | `control.design.dlqr` | [`profiles/10.12-dlqr.md`](profiles/10.12-dlqr.md) |
| 10.13 | 组合、PID、阶跃指标与 Padé | [`profiles/10.13-composition-pid-stepinfo-pade.md`](profiles/10.13-composition-pid-stepinfo-pade.md) |
| 10.14 | 具名角色与 `control.analysis.loopsens` | [`profiles/10.14-loopsens.md`](profiles/10.14-loopsens.md) |
| 10.15 | `control.lti.lsim`：任意输入的响应 | [`profiles/10.15-lsim.md`](profiles/10.15-lsim.md) |
| 10.16 | `control.design.pidtune`：按穿越频率与相位裕度整定 PI/PID | [`profiles/10.16-pidtune.md`](profiles/10.16-pidtune.md) |
| 10.17 | `signal.spectral.fft`：离散傅里叶变换 | [`profiles/10.17-fft.md`](profiles/10.17-fft.md) |
| 10.18 | `ode.ivp.solve`：常微分方程初值问题 | [`profiles/10.18-ode.md`](profiles/10.18-ode.md) |
| 10.19 | `linalg.system.solve`：线性方程组 | [`profiles/10.19-linsolve.md`](profiles/10.19-linsolve.md) |
| 10.20 | `interp.grid.interp1`：一维插值 | [`profiles/10.20-interp1.md`](profiles/10.20-interp1.md) |
| 10.21 | `fit.poly.polyfit`：多项式拟合 | [`profiles/10.21-polyfit.md`](profiles/10.21-polyfit.md) |
| 10.22 | `linalg.decomp.eig`：特征值与特征向量 | [`profiles/10.22-eig.md`](profiles/10.22-eig.md) |
| 10.23 | `linalg.decomp.svd`：奇异值分解 | [`profiles/10.23-svd.md`](profiles/10.23-svd.md) |
| 10.24 | `signal.design.butter`：巴特沃斯滤波器设计 | [`profiles/10.24-butter.md`](profiles/10.24-butter.md) |
| 10.25 | `signal.filter.filter`：数字滤波 | [`profiles/10.25-filter.md`](profiles/10.25-filter.md) |
| 10.26 | `nonlinear.root.fzero`：单变量求根 | [`profiles/10.26-fzero.md`](profiles/10.26-fzero.md) |
| 10.27 | `nonlinear.system.fsolve`：非线性方程组 | [`profiles/10.27-fsolve.md`](profiles/10.27-fsolve.md) |
| 10.28 | `optim.linear.linprog`：线性规划 | [`profiles/10.28-linprog.md`](profiles/10.28-linprog.md) |
| 10.29 | `optim.local.fmincon`：带约束的非线性最小化 | [`profiles/10.29-fmincon.md`](profiles/10.29-fmincon.md) |
| 10.30 | `optim.local.fminsearch`：无导数最小化 | [`profiles/10.30-fminsearch.md`](profiles/10.30-fminsearch.md) |
| 10.31 | `calculus.quadrature.integral`：数值积分 | [`profiles/10.31-integral.md`](profiles/10.31-integral.md) |
| 10.32 | `calculus.quadrature.trapz`：数据的梯形积分 | [`profiles/10.32-trapz.md`](profiles/10.32-trapz.md) |
| 10.33 | `fit.poly.polyval`：多项式求值（补偿 Horner，逐点严格误差界） | [`profiles/10.33-polyval.md`](profiles/10.33-polyval.md) |
| 10.34 | `fit.poly.roots`：多项式求根（包含半径、根簇） | [`profiles/10.34-roots.md`](profiles/10.34-roots.md) |
| 10.35 | `linalg.matrix.inv`：逆矩阵（逐列精化、条件数、无有效数字时标为不可依赖） | [`profiles/10.35-inv.md`](profiles/10.35-inv.md) |
| 10.36 | `linalg.matrix.det`：行列式（不溢出的 log/符号、误差估计、与零不可区分） | [`profiles/10.36-det.md`](profiles/10.36-det.md) |
| 10.37 | `linalg.subspace.rank`：数值秩（阈值、秩不明确） | [`profiles/10.37-rank.md`](profiles/10.37-rank.md) |
| 10.38 | `linalg.subspace.basis`：值域与零空间的标准正交基（orth、null） | [`profiles/10.38-basis.md`](profiles/10.38-basis.md) |
| 10.39 | `linalg.matrix.pinv`：Moore–Penrose 伪逆 | [`profiles/10.39-pinv.md`](profiles/10.39-pinv.md) |
| 10.40 | `linalg.matrix.norm`：向量与矩阵范数 | [`profiles/10.40-norm.md`](profiles/10.40-norm.md) |
| 10.41 | `linalg.matrix.cond`：条件数 | [`profiles/10.41-cond.md`](profiles/10.41-cond.md) |

## 11. 参考实现

Rust 参考实现的结构、运行方式、已知限制与内核复用规则（11.1 – 11.5，含 RI-1 – RI-3）见 [`docs/reference-impl.md`](docs/reference-impl.md)。那些规则约束的是参考实现本身，不是其他实现必须满足的协议。

## 12. 开放问题与下一步

### 12.1 开放问题

编号沿用历史编号；已解决的问题移到 CHANGELOG 的附录。

1. ~~大数组传输~~（v0.2 已定义 b64 与 file:// 引用）；剩余：网络引用（HTTP/对象存储）的 SSRF 防护与凭据模型，以及基于 resource link 的大输出。
2. **单位的深度**：v0.2 只做声明与校验，不做量纲推导。是否需要状态/输入/输出逐通道单位？
3. **时滞系统**：`InputDelay`/`OutputDelay`/内部时滞的表示与 Padé 近似策略未定义。
4. ~~相位分支约定~~（v0.2 已用 `branch_center_deg` + numpy 一致的 unwrap 规则定义）；剩余：MATLAB `bode` 的分支选择与此是否一致，需 MATLAB golden 验证。
5. **κ 缩放的容差**：v0.2 给出了 `lqr` 的 cond(S) 缩放与"自报估计 × 10"两种规则；v0.3 增加了根（Wilkinson κ）、系数（Π(1+|λ|)）、DC 增益、离散化的缩放（V-6）。实测误差都远低于缩放后的容差，说明这些界偏保守；其合理性还需 MATLAB golden 与更大规模、更病态的用例标定。
12. **（v0.3 新）参考实现自身的约定是否应成为规范**：例如 python-control 不报告恰在 Nyquist 频率的相位交越、离散 SM 用局部优化。v0.3 为了 parity 采用了相同约定并写明（D-6、V-10），但数学上更完整的定义可能不同；待 MATLAB golden 对照后再决定。
6. **组合调用的可信度传播**：Agent 串联 `lqr → step` 时，上游误差如何传递到下游 trust？未处理。v0.3.6 只做了一件事：`loopsens` 把子算子的检查汇总成 `sub_operator_checks`，并按 E-12 把 claim 降为 `spot_checked`；跨调用（下游收据引用上游 claim）仍未定义。
13. **（v0.3.6 新）`test_points` 的边界**：E-12 把“在固定点上核对一个处处成立的恒等式”算作完整检查，理由是结果由有限次精确形式的运算得到。若以后有算子的结果来自迭代或拟合，同样的核对就只是抽查，届时应改标 `sampled`。点数是否要与系统阶次挂钩（有理恒等式在 > 阶次个点上成立即处处成立）尚未规定。
7. **会话/对象句柄**：是否允许"创建系统对象、返回句柄、后续按句柄调用"？会破坏纯函数性。v0.3.6 的结论（K-6）：句柄就是完整的系统 JSON，绑定层保证不改写它；服务端对象 id 仍不引入。
8. **MATLAB 版本漂移**：建议凭证中锁定参考版本，并做版本间 diff 报告。
9. **许可合规**：MATLAB 许可条款对"用输出做对比测试"本身是否有限制，需要法务确认。
10. **Agent 可用性评估**：建立一组 Agent 回归任务（故意给错输入，测自我修复成功率）。
11. **稀疏 vs 稠密的哈希等价**（新）：是否应让同一矩阵的稀疏/稠密表示 `input_hash` 相同？代价是哈希大型稀疏矩阵时需要逻辑稠密化。
14. **（v0.4 新）两种范数并存**：`parity.yaml` 的 `compare` 用相对 Frobenius 范数，4.3 R-5 的 `verify` 用 ‖·‖∞（近零字段另有下限）。一致性套件沿用前者（9.4 CF-6）。是否统一？统一会改变已发布 `verify` 的判定，需要重跑已有测试。
16. **（v0.4 新）`method_policy` 的细节**：~~`prefer` 是否保留~~ 已决定去掉（3.14 MS-5），以后若有“偏好但不强制”的需求再加，届时要同时定义回退顺序与收据里的回显。仍待定：字段名；`constraints` 还需要哪些键（例如对 `bound_kind` 的要求）；`METHOD_UNAVAILABLE` 归 `VALIDATION` 还是 `DOMAIN`；`receipt.alternatives` 是否要求 `backend` 与 `algorithm` 必填。
17. **（v0.4 新）内核级接入的接口**：数值内核如何挂到宿主（输入输出是否限于数值数组加元数据、错误如何跨 ABI 表达、内存归属、收据由哪一层生成）。需要先对照 `aeoi-core` 现有的 `validate`/`execute` 分层（8.2）再设计；它不是 8.3 的 `aeoi-capi`。
18. **（v0.4 新）宿主策略的表达与注入**：3.14 MS-10 规定了合并语义，但宿主策略从哪里来（配置文件、MCP 客户端、会话参数）、如何审计，属于绑定层设计，尚未写。参考实现的 `Bridge::with_method_policy` 只提供合并这一步，策略的来源（目前只能由嵌入方在代码里设置）、持久化与审计都没有设计。
24. **（v0.4.4 新，写默认值用例时发现）默认值与请求指纹**：H-1 按请求原样计算 `input_hash`，不补默认值，所以“省略 `type`”与“写出 `type: continuous`”的请求指纹不同，而 4.3 R-3 说请求指纹表示“同一个问题”。要么在哈希前补齐 schema 默认值（会改变所有省略了默认值的已发布 `input_hash` 与 golden 文件名之外的指纹，需要一次迁移），要么在 R-3 写明指纹只认字面相同的请求。一致性套件目前只要求结果相同，不比较这两个指纹。
25. **（v0.4.9 新，发布流水线证实全平台逐位相同后）`platform_bitwise` 升为 `bitwise`**：参考实现已在 5 个平台上对全部 golden 逐位相同（2.1、M-7a），但 manifest 仍声明较弱的 `platform_bitwise`。改声明是新的承诺（G-2 MINOR），23 个算子要一起升版本、重新生成 manifest 与示例；还要决定 `aeoi_verify` 在 `bitwise` 下遇到跨平台差异时的判定（现在跨平台只给 `tolerance_match`）。建议随下一批算子升版本时一并处理。
26. **（v0.4.10 新）只对部分方法有意义的后置条件**：`interp1` 的 `spline_c2` 只适用于 spline，`local_bounds` 不适用于 spline。E-11 把缺失的后置条件算作不成立，所以现在只能按空真报告（IP-6）。可选做法：manifest 的 `receipt.postconditions` 允许按 `method` 条件列出，或在 `trust.checks` 里允许 `null` 表示“不适用”。两种都改核心 schema（MINOR），留到第二个有此需要的算子出现时再定。v0.4.11 出现了第二个实例：`eig` 的 `orthonormal` 只对对称算法有意义（EG-5）；v0.4.12 出现了第三个：`butter` 的 `sos_consistent` 不适用于模拟滤波器（BW-5）。暂时都按空真处理，等作者决定。
27. **（v0.4.11 新）警告附修复建议**：5.1 的 warning 没有 `remediation` 字段。`polyfit` 的病态警告把一个与错误 remediation 同形的对象放在 `details.suggested_remediation`（PF-4）。可选做法：给 warning 增加可选的 `remediation` 数组（核心 schema MINOR），或维持放在 `details` 的约定。
28. **（v0.4.19 新，第二实现发现）尚未规定的细节**：见 `conformance/impls/python_scipy/SPEC_FINDINGS.md`。未处理的有：未配置 data root 时拒绝 `ref` 的码（PY-5，参考实现用 `REF_FORBIDDEN` / `REF_UNSUPPORTED`）；缺必填字段时违规 path 指向哪里（PY-6）；各 `DOMAIN` 码的 `retryable`（PY-8）；dry-run 信封的 `status` 等字段（PY-9）；manifest 的 `bound_kind: none` 与收据 `none_reported` 的关系（PY-10）；一般语法错误给不出补丁时 EX-6 与 X-5 的冲突（PY-13）；`variable`/`params` 的名字错误该填哪个 path 与 position（PY-14）。
29. **（v0.4.19 新，PY-27）放宽容差的补丁可能掩盖发散**：IN-5 的“放宽到 1e-6”补丁套到发散积分（1/x 在 [1, ∞)）上，结果“收敛”为 36.62，只靠 `refinement_consistent` 不成立把 claim 降为 `unverified`。CF-9 只要求套用补丁后通过校验，套件会接受。可选：没有奇点证据时不给放宽补丁；或规定套用放宽补丁后 MUST 检查 `refinement_consistent`。
30. **（v0.4.19 新，PY-15）用估计常数写成的“先验界”**：FT-6 把 c = 10 的估计归为 `bound_kind = a_priori`，强于实际（对 Bluestein 与混合基没有证明）。可选：改为 `a_posteriori_heuristic`，或给出有证明的常数。
31. **（v0.4.21 新，第二实现第二轮）尚未规定的细节**：bode、step、convert、roots 的 profile 与 3.1/3.10 已按第二轮发现改写（BD-1 – BD-8、ST-1 – ST-9、CV-1 – CV-8、T-5、R-6、R-8、R-9、CF-7，处理表见 `SPEC_FINDINGS.md`）。仍未处理的有：零 DC 增益与稳定性用算出的值、不设容差（ST-5，PY-39/40：确定但不与尺度无关，X-4a 要求的阈值留待以后）；R-7 的 `normalize = false` 对 ss→tf 无意义（PY-43 后半）；roots 的 golden 目录名不符合 V-3（PY-65，与 PY-30 同类，等统一迁移 golden 时处理）。

### 12.2 下一步

1. 按 12.3 的依赖层自底向上实现目标函数；同一层内按 MATLAB 使用频率排。
2. 用只读规范的第二实现（`conformance/impls/python_scipy/`）检验规范是否足以定义行为。v0.4.19 已做 `fft`、`interp1`、`linalg.system.solve`、`integral`（另有 `lyap`）；下一批 `bode`、`step`。
3. 一致性套件（9.4）：有数值规则的算子不低于全部算子的 30%（v0.4.17：24 / 37）。
4. 发布：102 个目标函数全部完成后发布 v0.5（作者决定 2026-11-25）；在此之前不打 tag。
5. 在有授权的机器上运行 `matlab/` 下的 golden 脚本；PyO3 与 C ABI 绑定（8.3）。
6. 根据以上反馈修订，冻结 v1.0。

### 12.3 目标算子清单与依赖

本项目的目标是覆盖工程计算中最常用的约 100 个 MATLAB 函数（作者决定，v0.4.11）。逐元素运算、绘图、文件读写、字符串和数据结构不在其中，它们不需要 AEOI 的信任信息。清单是作者与 Claude 的判断，不是统计结果。

完整清单在 [`roadmap/layers.md`](roadmap/layers.md)（由唯一来源 `roadmap/targets.py` 生成）：每个 MATLAB 函数对应的 AEOI 算子、状态、依赖的共享内核（11.5）与依赖层，以及内核依赖图 `roadmap/deps.mmd`。实现按依赖层自底向上。

目标共 102 个 MATLAB 函数，已覆盖 49 个（48%），由 45 个 AEOI 算子提供。共享内核 33 个：已有 25，待从算子中抽出 0，待新写 8。层号 = 1 + 依赖中最大的层号（无依赖为 0）；由 `roadmap/targets.py` 生成，改清单只改那个文件。

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

## 13. Changelog

见 [`CHANGELOG.md`](CHANGELOG.md)。

## 附录 A：文件清单

```
spec-src.md / spec-v0.4.md     核心规范（第 0 – 9、12 章）；scripts/build_spec.py 由源稿生成，并检查 profiles/ 里的链接
profiles/10.NN-<op>.md         算子契约（第 10 章，规范性）
docs/reference-impl.md         参考实现说明（8.2 – 8.5、第 11 章，含 RI-1 – RI-3）
CHANGELOG.md                   改动历史（原第 13 章）与已解决的开放问题
roadmap/                       目标函数与内核清单、依赖层（12.3）
schemas/                       common / manifest / envelope / error（/0.4）
examples/<op>/                 manifest、input/output schema（scripts/write_manifests.py 生成）、示例请求与响应、parity.yaml
golden/{python-control,mpmath}/<op>/<ver>/  参考用例（参考值来自 python-control / numpy / scipy / mpmath，不是 MATLAB）
scripts/                       write_manifests.py、regen_examples.py、validate_examples.py、build_spec.py
scripts/golden/                golden 与示例请求的生成脚本（gen_golden_*.py 等）
conformance/                   跨实现一致性套件（9.4）与 Python 第二实现
rust/                          参考实现（docs/reference-impl.md）
reports/                       parity、收据、一致性与评测报告
eval/、tools/、matlab/、pages/ LLM 评测、辅助工具、MATLAB golden 脚本（计划中）、GitHub Pages 站点
```
