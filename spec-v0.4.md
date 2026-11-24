# AI Engineering Operator Interface Spec（AEOI）v0.4-draft

> 状态：Draft v0.4 · 2026-11-23 · 作者：Qingfei Yuan（草拟协助：Grok Bot）
>
> v0.4 为第二个实现和不同的计算方法在规范里留出位置，增加两组条款：同一算子可以登记多个计算方法，请求用 `exec.method_policy` 约束所用的方法（3.14，规则 MS-1 – MS-11）；换一个实现是否等价于参考实现，由跨实现一致性的判定规则回答（9.4，规则 CF-1 – CF-12）。**这两组目前只有规范文本：参考实现与 schema 里的新字段（`method_policy`、`alternatives`、`method_selection`）还没有加**（第 13 章 v0.4-draft 列出需要的改动）。线协议标识升为 `aeoi/0.4`，核心 schema 的 `$id` 升为 `/0.4`（与 v0.3 的做法相同，12.1 第 15 项）：标识已经改到 `schemas/`、`examples/`、`write_manifests.py` 与 Rust 的 `aeoi_core::SPEC` 常量里，**但 Rust 尚未重新编译或运行测试**。信封字段只增不减，`input_hash`、`fingerprint` 都不含 `spec`，所以除标识外结果字节不变；v0.3 的消费者若校验 `spec` 或 schema `$id` 需要同步更新（0.x 期间 MINOR 可破坏，G-4）。不认识 `method_policy` 的旧实现会按 3.3 R-4 返回 `E.VALIDATION.SCHEMA`，不会静默忽略硬约束。以下 v0.3.x 各段是各自版本当时的陈述，按原样保留。
>
> v0.3.6 根据 EngDesign 真模型运行（`reports/design-live.md`）补规则：回路里的系统按角色命名（对象、控制器、反馈通路），一次调用得到开环与闭环（新算子 `control.analysis.loopsens`），带宽字段名写明 Hz 或 rad/s（3.13 K-1 – K-6）；互斥选项的冲突统一报错并给出只删除已出现字段的补丁（F-8）；抽样或压缩数据上的检查不算“已验证”（E-12、E-13，新 claim `spot_checked`）；Agent 循环在回合用完之前必须提交（8.4 AL-1 – AL-3）。线协议标识仍为 `aeoi/0.3`：信封只增加可选字段，位置参数调用的 `series`/`feedback` 结果字节不变。
>
> v0.3.5 补上系统组合、定义默认值与近似误差这三处 v0.3.4 写不清楚的规则，并据此增加 6 个算子（串联、并联、反馈、PID、阶跃指标、Padé）。线协议标识仍为 `aeoi/0.3`：信封只在 `receipt.evidence` 上增加可选的 `approximation`（旧消费者忽略即可），已发布的 12 个算子的 `result` 与 `output_hash` 不变。
> SPDX-License-Identifier: CC-BY-4.0 · © 2026 Qingfei Yuan。规范文本按 CC-BY-4.0 许可（`LICENSE-CC-BY-4.0`）；规范中的代码片段、JSON Schema 与参考实现按 MIT OR Apache-2.0 许可（`LICENSE-MIT`、`LICENSE-APACHE`）。
> 定位：叠加在 MCP 之上的"工程计算语义层"。MCP 负责传输与工具调用；AEOI 规定工程算子如何描述自己、如何表达带物理意义的输入输出、如何报告结果可信度、如何报错、如何复现、如何证明与参考实现一致。
> 冻结条件：用 5–10 个真实控制算子跑通后再升级为 v1.0。v0.x 期间任何条款都可能被修改。
> v0.3.1 相对 v0.3 的改动（面向 LLM 函数调用的可消费性，线协议标识仍为 `aeoi/0.3`，向后兼容）及 v0.3、v0.2 的改动见第 13 章 Changelog。v0.3.1 新增 LLM 端到端测试台 `aeoi-web`（8.4、11 章）。v0.3 的 Rust 参考实现（第 11 章）共 12 个控制算子 + MCP stdio server，全部测试通过：v0.2 的 `control.lti.bode`、`control.lti.step`、`control.design.lqr`，以及 v0.3 新增的 `control.lti.convert`、`control.lti.poles_zeros`、`control.analysis.ctrb_obsv`、`control.analysis.margin`、`control.lti.c2d`、`control.lti.impulse`、`control.math.lyap`、`control.design.place`、`control.design.dlqr`。

## 0. 约定

- 关键词 MUST / MUST NOT / SHOULD / SHOULD NOT / MAY 按 RFC 2119 / RFC 8174 解释。
- JSON Schema 一律使用 Draft 2020-12。完整 schema 文件见 `schemas/`，`$id` 形如 `aeoi://schemas/common/0.4`。
- 本文中 `spec` 版本标识写作 `aeoi/0.4`（v0.3.x 为 `aeoi/0.3`）。
- **净室原则（normative）**：算子实现 MUST 仅依据公开文献、教科书、标准与开源许可兼容的代码；MUST NOT 反编译、反汇编、阅读或复制 MATLAB（或任何闭源软件）的实现代码、`.p` 文件、二进制或未公开文档。MATLAB 只作为黑盒 ground truth，通过其公开 API 生成对照数据。每个 manifest MUST 带 `provenance.clean_room.declared = true`。
- **与 MATLAB 的关系（v0.3.6 澄清）**：算子的命名与分类沿用 MATLAB Control System Toolbox 的**公开函数名**（`series`、`feedback`、`margin`、`stepinfo`、`pade`、`loopsens` 等），以便工程师和模型按熟悉的名字找到算子；定义与方程写在本规范里，不依赖任何 MATLAB 文档。到 v0.3.6 为止，**所有参考数据都来自开源实现**（python-control、scipy、mpmath），没有一条由 MATLAB 生成；`matlab/` 目录下的脚本只是留给有 MATLAB 许可的第三方复核用的生成器。
- **示例数值说明**：第 10 章示例的响应由 Rust 参考实现（`aeoi-control 0.3.0`）实际输出（`regen_examples.py`）；其数值另与 python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 在 box 上的计算结果比对（`cargo test`）。**两者都不是 MATLAB 输出。** `timing` 为 box 上的实测值，仅供参考。

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
Agent ──MCP(JSON-RPC, stdio)──▶ aeoi-mcp ──▶ Registry ──▶ dyn Operator (Rust, aeoi-control)
                             │                              └─ pure Rust (nalgebra 0.33)；FFI 后端为可选（v0.2 未用）
Python (PyO3, 计划) ─────────┤
C ABI (cdylib, 计划) ────────┘     所有入口共享同一 manifest、schema、信封与错误码（aeoi-core）
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
- M-6 `backend` MUST 写明库名、例程名、版本、`binding`（`native` | `ffi`）与 SPDX 许可证。纯 Rust 实现写 `binding: "native"`，`routine` 写实现函数名（如 `lqr::solve_care`）。许可证 MUST 与平台分发方式兼容。
- M-7 `determinism` MUST 取以下之一：

| 值 | 含义 | 允许的场景 |
|---|---|---|
| `bitwise` | 同 build、同输入 ⇒ 任何机器上输出逐位相同 | 不依赖运行时 CPU 分派的实现 |
| `platform_bitwise` | 同 build、同"平台"逐位相同；跨平台只保证在容差类内 | 依赖 SIMD/FMA 或运行时 CPU 特性分派的内核 |
| `tolerance` | 同平台多次运行也只保证在容差类内 | 必须说明原因（如并行归约），L3 需评审 |
| `seeded` | 给定 `seed` 后满足 `bitwise` | 随机算法 |

  "平台" = 目标三元组（`reproducibility.platform`）+ 运行时检测到的 CPU 特性集合（`reproducibility.cpu_features`）。WebAssembly 构建（`wasm32-unknown-unknown`，目标三元组里没有 OS）的 `platform` 记为 `wasm32-unknown`；它与原生构建是不同的"平台"：超越函数走 Rust `libm` 而不是 glibc，矩阵乘法没有 AVX/FMA 内核，所以 `platform_bitwise` 不延伸到 wasm ↔ 原生（实测见 8.5）。v0.3 参考实现的 12 个算子都声明 `platform_bitwise`：nalgebra 的稠密乘法走 `matrixmultiply`，后者在运行时按 AVX/FMA 等特性选择内核。
- M-8 `tolerance_class` MUST 取 7.2 表中的一个值。
- M-9 **稀疏支持声明（v0.2 新增）**：manifest MUST 含 `sparse_support`，以输入字段的 JSON Pointer 为键，值为 `{"policy": "native" | "densify" | "reject", "max_dense_elements": <int>}`。未列出的矩阵字段视为 `reject`。`densify` 时算子 MUST 返回 `W.PERFORMANCE.SPARSE_DENSIFIED`；稠密化后元素数超过 `max_dense_elements` MUST 返回 `E.RESOURCE.INPUT_TOO_LARGE`。
- M-10 **编码与上限（v0.2 新增）**：manifest MUST 含 `encodings {input: [...], output: [...]}`（取值 `json`、`b64`、`ref`），以及 `limits`（算子特有的规模上限，如 `max_grid_points`、`max_states`）。上限检查 MUST 在主计算前完成。
- M-11 **方法登记（v0.4）**：`receipt`（4.3 R-1）的现有字段描述 `default` 方法；同一算子的其他计算方法登记在可选的 `receipt.alternatives[]`（3.14 MS-1）。没有 `alternatives` 的算子只有一个方法，manifest 与行为和 v0.3 完全相同。

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
| LTI 系统 | `{"kind":"tf"|"ss"|"zpk", ..., "ts", "time_unit"}` | `ts=0` 连续，`ts>0` 离散；`ts` 与 `time_unit` 必填 |

`schemas/common.schema.json` 中 `Matrix` = `oneOf [DenseMatrix, DenseMatrixB64, MatrixRef, SparseMatrix]`，四个分支以必填字段（`data` / `data_b64` / `uri` / `format`）互斥区分。

规则：
- T-1 矩阵 MUST 使用上表规范形式之一。绑定层 MAY 接受嵌套数组 `[[1,2],[3,4]]` 作为便捷输入，但 MUST 在校验前规范化；哈希只针对规范内容计算（3.7）。选择展平+shape 而非嵌套数组，是为了无歧义地表示空矩阵（如 `shape:[0,3]`）与高维数组，且与 Rust/NumPy 内存布局一致。
- T-2 `dtype` 取 `f64`、`c128`、`i64`、`bool`。v0.2 不支持 f32 输入。复数元素 MUST 为 `[re, im]`。
- T-3 元素个数 MUST 等于 `shape` 各维乘积（内联：`data.length`；二进制：解码字节数 = 元素数 × 元素字节数）；违反返回 `E.VALIDATION.SHAPE`。`dtype` 与算子要求不符返回 `E.VALIDATION.DTYPE`。
- T-4 FFI 到 Fortran 列主序库时，转置 MUST 在 backend 包装层完成，且 MUST 有往返单元测试。（另见第 0 章内存序规则。）
- T-5 LTI 表示：`tf` 支持 SISO（`num`/`den` 为一维多项式）与 MIMO（二维多项式数组）；`zpk` 中复数零极点 MUST 共轭成对（相对容差 1e-12，否则 `E.VALIDATION.NOT_CONJUGATE`）；`ss` 的 `A,B,C,D` 维数 MUST 一致。内部转换（如 tf→ss）MUST 在 trust 中注明所用 realization（v0.2：controllable canonical form，与 scipy `tf2ss` 约定一致；MIMO tf 为逐通道块对角实现）。
- T-6 **多项式前导零（v0.2 修改）**：前导零 MUST 被去除并返回 `W.VALIDATION.LEADING_ZEROS_STRIPPED`（v0.1 规定为报错，实践中前导零常来自数值计算，报错对 Agent 不友好）。分母全零返回 `E.VALIDATION.ZERO_DENOMINATOR`。

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

### 3.5 稀疏矩阵（v0.2 新增）

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

### 3.6 大矩阵传输（v0.2 新增）

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

### 3.7 哈希与编码无关性（v0.2 新增）

- H-1 计算 `input_hash` 前，每个数组 MUST 被替换为 `{"$array": {"dtype","shape","sparse": bool, "sha256"}}`，其中稠密数组的 `sha256` 是 row-major 小端原始缓冲区的 SHA-256（与 L-2 的 `sha256` 字段同值）。因此同一矩阵用 `json`、`b64`、`ref` 三种方式传入，`input_hash` 完全相同（参考实现有测试）。
- H-2 稀疏数组的摘要基于其规范 CSR 形式（排序、合并重复项后的 `indptr/indices/data`），`sparse: true`。**稀疏与稠密表示的同一矩阵 `input_hash` 不同**（有意如此：v0.2 不为哈希而稠密化大型稀疏矩阵）。
- H-3 `structure` 与 `duplicates` 等纯声明字段不进入数组摘要。

### 3.8 安全注意事项（v0.2 新增）

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

### 3.10 多输出结果、系统输出与离散时间约定（v0.3 新增）

实现第二批算子时发现 v0.2 没有规定以下内容，各算子各写各的会让 Agent 无法泛化。以下条款为规范性要求。

**多输出结果**
- R-1 结果是一个对象，每个输出是一个具名字段（如 `poles`、`zeros`、`dcgain`；`gain_margin`、`phase_margin`、`crossings`），MUST NOT 用位置数组表示多个输出。带单位的标量/数组用 `{unit, value}`；无量纲矩阵直接用 `Matrix`；复数集合用 `c128` 向量。
- R-2 复数集合（极点、零点、闭环极点）的顺序没有规范意义：参考实现按实部升序、虚部降序输出，但消费者与 parity 测试 MUST 按集合匹配比较（V-5）。
- R-3 "不存在"的标量用 JSON 字符串 `"Inf"`/`"NaN"` 并在 schema 中允许（如无相位交越时 `gain_margin = "Inf"`、对应频率 `"NaN"`），MUST NOT 用 `null` 表达"无穷大"。`null` 只表示"未计算/不适用"（如不稳定系统的 Gramian，伴随 `W.DOMAIN.GRAMIAN_UNDEFINED`）。
- R-4 选择一个"代表值"的输出（如多个交越中的 GM/PM）MUST 同时返回全部候选（`crossings.phase[]`、`crossings.gain[]`、`crossings.stability[]`），并在 10.x 写明选择规则。

**返回系统的算子**
- R-5 返回 LTI 系统的算子（`convert`、`c2d`）MUST 在 `result.system` 中返回完整的 `LtiSystem`（含 `ts`、`time_unit`），可直接作为下一个算子的输入。MIMO tf 用嵌套数组 `num[i][j]`、`den[i][j]`（`[ny][nu]`）。
- R-6 状态空间实现不唯一。返回 ss 的算子 MUST 用 `realization` 字段（枚举）说明所用实现：`identity`、`controllable_canonical`（scipy `tf2ss` 约定：A 第一行为 −a₁…−aₙ，B = e₁）、`block_diagonal_controllable_canonical`（MIMO tf 逐元素）、`ss2tf_characteristic_polynomial`、`expanded_product`、`polynomial_roots`、`invariant_zeros_eigenvalues_markov_gain`，以及 v0.3.5 的 `polynomial_product`、`polynomial_sum`、`polynomial_feedback`、`series_block`、`parallel_block`、`feedback_block`、`pid_closed_form`、`pade_coefficients`。v0.3 不做最小实现（minreal）；MIMO 转换得到非最小实现时 MUST 给 `W.DOMAIN.NONMINIMAL_REALIZATION`。
- R-7 ss→tf 约定（与 scipy `ss2tf` 一致）：den = det(sI − A)（由特征值展开），numᵢⱼ = det(sI − A + bⱼcᵢ) + (dᵢⱼ − 1)·den；所有元素共享同一分母；首项系数精确置为 dᵢⱼ 后去除前导零；`normalize = true`（默认）时分母为首一。
- R-8 zpk 在 v0.3 只表示 SISO；MIMO → zpk MUST 返回 `E.DOMAIN.MIMO_NOT_SUPPORTED`（remediation 建议 `to: "tf"` 或逐通道调用）。ss→zpk 的增益 k 取首个非零 Markov 参数 C·A^{r−1}·B（r = 相对阶），不经多项式。
- R-9 零点 = 不变零点（Rosenbrock 系统矩阵降秩的有限 s）。系统矩阵不是满法秩（例如传递函数恒为零）时零点无定义，MUST 返回 `E.DOMAIN.DEGENERATE_SYSTEM`，MUST NOT 返回空集合冒充"没有零点"。法秩判定在两个确定性的一般位置点上计算 G(s) 的奇异值（与尺度无关的阈值，X-4a）。

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

### 3.11 面向 LLM 函数调用的可消费性（v0.3.1 新增）

用云端 LLM（Gemini 函数调用）端到端调用 12 个算子时发现：规范里对"人/程序"足够的信息，模型不一定看得到——函数调用接口只接受 JSON Schema 的子集，未知关键字会被丢弃；`oneOf` 校验失败时 jsonschema 只给出"不符合任何分支"，模型无从修正。以下条款为规范性要求（F = function-calling；v0.3.1-draft 早期文本曾记作 M-1 – M-6，与第 2 章 manifest 规则 M-1 – M-6 重号，已改名）：

- F-1 input schema 的每个顶层属性 MUST 有 `description`，写明物理含义、维数（如 `[n,m]`）、单位与默认值；枚举 SHOULD 逐值说明含义（如 `c2d.method`、`lyap.type` 写出方程）。参考实现由 `write_manifests.py` 统一补齐。
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
  - `"user"`：测量值，没有安全默认。规则即 F-5（引用核对、缺失只给 `ask_user`）。v0.3.5 仍只有 `control.lti.c2d` 的 `ts`。
  - `"convention"`：指标或互联的**定义参数**，规范给出默认值，默认值是算子语义的一部分。省略时用默认，MUST 在结果里回显实际使用的值；覆盖时回显覆盖值。绑定层 MUST NOT 要求 `_user_quote`，系统提示词 MUST NOT 把用默认值说成“假设了用户没给的物理量”。v0.3.5：`feedback.sign` 默认 −1，`stepinfo.rise_time_limits` 默认 `[0.1, 0.9]`，`stepinfo.settling_threshold` 默认 `0.02`，PID 的 `form` 默认 `parallel`，`Ki`/`Kd`/`Td`/`Tf` 默认 0（该项不出现）。
  - `"supplied"`：没有安全默认、但**不是**必须从用户原话逐字摘出的数。调用方必须给出；缺失是 schema 错误，remediation 只有 `ask_user`。绑定层 MUST NOT 加 `_user_quote`：这个数可以是设计选择（PID 的 `Kp`、Padé 的 `order`），也可以是从题目里别的数算出来的（Padé 的 `delay`，例如由 t2−τ 得到，原话里没有这个数）。系统提示词 MUST 写明：不得填“常用值”；用户和题目都没给出、也推不出来时先问。设计评测没有用户可问时，评测提示词 MAY 宣布一个回退（例如“order 没写就用 10 并在说明里声明”）；这个回退写在提示词里，MUST NOT 写进算子的默认值。

- F-8 **互斥选项的冲突由算子报告（v0.3.6）。** EngDesign 运行里，XG_12 先在 `form=standard` 下夹带 `Ki`/`Kd`，改成 parallel 后又写了 `Ti: 0`，得到的是 schema 的原话“0 is less than or equal to the minimum of 0”，前 4 步才得到一个控制器；XG_02 也因多余的 `Ti=0` 浪费了一回合。规则：
  - (a) “某组字段只属于某个模式”（PID 的 parallel 与 standard、`series`/`feedback` 的具名角色与位置参数）MUST 由算子的 validate 检查，MUST NOT 只靠 `oneOf`/`not` 这类 schema 关键字：函数调用子集会丢掉它们（8.4 表），校验失败时模型也看不出是哪两个字段冲突。冲突 MUST 报 `E.VALIDATION.OPTION_CONFLICT`，`details` 给出 `conflicting_fields`、生效的模式（例如 `form`）以及该模式是否是调用方写的（`form_given`）。
  - (b) remediation MUST 是可以直接套用的 JSON Patch：`remove` 只针对请求里**确实出现**的字段（不得对不存在的路径 remove，否则套用补丁本身就报错）；需要改模式时用 `add`/`replace` 写模式字段。只有当另一个模式下所有字段都合法时才给“改模式”补丁（例如 `Ti ≤ 0` 时不给“改成 standard”）。两种做法都要猜调用方意图时，`confidence` 为 `medium`，并 MUST 另给 `ask_user`。
  - (c) 有物理含义的禁止值 MUST 用一句说明原因的话报错，而不是 schema 的比较句：`Ti = 0` 的含义是“没有积分项”，参考实现报 `OPTION_CONFLICT`，消息写明“没有积分项就省略 Ti，不要写 0”，补丁 `omit_integral` 去掉 `Ti`；负的 `Ti`/`Td`/`Tf` 报 `E.VALIDATION.RANGE`。因此 `pid` 0.4.0 的 input schema 去掉了 `Ti` 的 `exclusiveMinimum`，改由算子检查（G-2 MINOR：接受的输入集合不变，只是错误更具体）。
  - (d) 工具描述（F-1）MUST 在每个只属于某一模式的字段上写“只用于 form = …”，并写明“不需要的项直接省略，不要用 0 占位”。

### 3.12 系统的组合、跨调用与近似（v0.3.5）

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
- A-4 不在 v0.3.5 提供 `pidtune`。一个能写进规范的整定算子必须事先声明：对象类（例如已经化成的一阶加时滞）、被最小化或被满足的指标、解的唯一性、以及失败时的错误码。缺任何一条，算法就只是“某个实现的默认行为”，不能当算子。

### 3.13 回路的角色、一次性分析与句柄（v0.3.6）

EngDesign 真模型运行（`reports/design-live.md`，7 题）里，模型调用了正确的算子却得到错误的系统：XG_13 用 `feedback(sys1 = 对象, sys2 = 控制器)` 把控制器放进了反馈通路（得到 G/(1+CG)），又在闭环后多串了 1/s；Ziheng_03 把闭环 T 当成开环 L 送进 `margin`，并把因式乘积 (0.5s+1)(s+1)(2s+1) 心算展开错了；XG_11 手拼越来越高阶的分母，丢掉了对象里的 169；XG_02 要求约 85 Hz 的带宽，却没有办法读 plant×controller 的带宽，交了一个带宽为 0 的增益。工具本身都是对的，接口让这些错误很容易犯。下面是规范性要求（K = loop）。

- K-1 **按角色命名。** 组合算子 MUST 接受具名角色：`series(controller, plant)` 得到 L₀ = plant·controller；`feedback(plant, controller, sensor)` 得到 T = G·C/(1 + H·G·C)，`controller` 永远在前向通路，`sensor` 是反馈通路。`controller` 省略 = 单位增益（`composition.controller_implicit = true`），`sensor` 省略 = 单位反馈（与 I-5 的“省略 sys2”相同）。位置参数 `sys1`/`sys2` 为兼容保留，语义按 I-5 不变；**两种写法在同一个请求里混用 MUST 报 `E.VALIDATION.OPTION_CONFLICT`**（F-8），remediation 给出“只保留具名角色”和“只保留位置参数”两个补丁。角色名只在 SISO/MIMO 的方向上有意义，不改变数学：`feedback(plant=G, controller=C, sensor=H)` 与 `feedback(sys1=series(C,G), sys2=H)` 逐位相同。
- K-2 **拓扑是固定的。** 单回路负反馈：r → e = r − H·y → C → G → y。正反馈仍用 `sign = +1`。具名调用 MUST 在 `composition.topology` 写出这条拓扑文字，以便模型和读者核对。
- K-3 **开环与闭环必须标出来。** 具名调用的结果 MUST 有 `composition.loop_role`：`series` 为 `open_loop`，`feedback` 为 `closed_loop`；位置参数调用不写这个键（保持 v0.3.5 的字节）。工具描述（F-1）MUST 说明哪个结果是 L、哪个是 T，以及“`margin` 只接受 L；`stepinfo`/`step` 用 T”。
- K-4 **一次性回路分析 `control.analysis.loopsens`。** 输入 `plant`（必填）、`controller`、`sensor`（可省略，意义同 K-1）、可选 `time`（给出时做 T 的阶跃指标）、`rise_time_limits`、`settling_threshold`、`bandwidth_drop_db`（F-7 `convention`，默认 −3）。输出 `open_loop` = H·G·C、`closed_loop` = T、`sensitivity` = 1/(1+L)、闭环极点与 `closed_loop_stable`、L 的 `margins`、`step_metrics`。它 MUST 等于把原语串起来（K-1 的 `series`/`feedback` → `margin(L)` → `stepinfo(T)`）：参考实现内部直接调用这些算子（DynOperator），golden 用 python-control 的 `series`/`feedback`/`stability_margins`/`bandwidth`/`step_info` 生成（`gen_golden_loopsens.py`，12 例）。子算子报错时该字段为 null，并给 `W.DOMAIN.ANALYSIS_SKIPPED`（`details.code` 为子算子的错误码），其余字段照常返回；子算子的警告加上路径前缀（如 `/margins/…`）原样转发。只支持 SISO；采样周期按 I-4。
- K-5 **频率字段写单位。** 返回频率的字段名 MUST 带单位后缀，并且两种单位都给：`gain_crossover_rad_s` / `gain_crossover_hz`（|L(jω)| = 1，开环穿越频率），`bandwidth_rad_s` / `bandwidth_hz`（闭环 T 的带宽）。`_hz` MUST 恰好等于 `_rad_s / 2π`。带宽的定义 MUST 回显在 `bandwidth_definition`：`{drop_db, reference: "dc_gain", of: "closed_loop", method}`，即第一个使 |T(jω)| < |T(0)|·10^{drop_db/20} 的频率；连续系统用 |T(jω)|² 在 ω² 上的偶多项式求根再二分精化，离散系统在 (0, π/ts) 的网格上找第一个跨越再二分。T 不稳定、T 不 proper、DC 增益为 0 或无穷时为 null 并给 `W.DOMAIN.ANALYSIS_SKIPPED`；T 稳定但在整个频率范围内都没有跌到阈值以下（例如双正则且高频增益不低于阈值）时为 `"Inf"`，与 python-control 的 `bandwidth` 一致。带宽算在 plant·controller 的闭环上，不是只算对象。
- K-6 **系统靠原样传递，不靠手抄。** 返回的 `result.system`（以及 `open_loop` / `closed_loop` / `sensitivity`）就是 I-1 的句柄。绑定层回传给模型时 MUST NOT 抽样、截断或改写任何 `LtiSystem`（F-6 的 `_downsampled` 只适用于时间/频率数组）；系统提示词与工具描述 MUST 写明“把上一个工具返回的系统 JSON 原样作为下一个工具的参数，不要重新抄写或化简系数”，`input` 描述里的 `LtiSystem` 参数 MUST 带这句话。题目给出的因式乘积 SHOULD 用 `series` 相乘，而不是由模型展开。

### 3.14 方法的登记与选择（v0.4）

到 v0.3.6 为止，一个算子版本只有一个方法：manifest 的 `receipt.method_id` 写死，收据的 `method.id` 回显它。于是“换一个实现”或“换一种算法”只能靠发布新的算子版本，用户也没有办法要求“只用经过审计的库”。v0.4 允许同一个算子登记多个方法，并让请求在这些方法之间选择。

**方法 id 和算子 id 不是一回事。** 算子 id（M-2）说的是“算什么”，例如 `control.math.lyap`；方法 id（4.3 R-1）说的是“怎么算”，例如 `aeoi:lqr:sign-function+kform-newton-kleinman-dot2`。同一个算子可以有几种算法，各自有自己的精确性类、误差界和前提。以下条款是规范性要求；v0.4-draft 起，参考实现尚未实现。

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
- E-9 **dry run（v0.2 新增）**：`exec.dry_run = true` 时，算子完成全部校验（schema、语义、规模上限）后停止，返回 `dry_run: true`、`result: null`、`estimate`（如网格点数、flops 估计）；不得执行主计算。信封字段 `dry_run` 为必填。
- E-10 **输出编码**：见 L-5。
- E-11 **没有证据，不声称成功（v0.3.3 新增）**：成功信封的 `receipt.evidence.claim` 只能是 `verified`（manifest `receipt.postconditions` 列出的 `trust.checks` 全部为 true）、`spot_checked`（全部为 true，但至少一个只是抽样检查，E-12，v0.3.6 新增）、`unchecked`（算子没有声明后置条件）或 `unverified`。任何声明的后置条件为 false 或缺失时，框架 MUST 追加 `W.EVIDENCE.POSTCONDITION_FAILED`（于是 `status` 为 `ok_with_warnings`），claim 为 `unverified`。`ok` 本身只表示"算子完成了计算"；只有 `verified` 才能被下游（含 LLM）表述为"已验证"。只描述问题性质的检查（如 `asymptotically_stable`、对称化之前的不对称度）MUST NOT 列为后置条件，否则合法结果会被误判。
- E-7a **框架级码（v0.3.3 新增）**：由框架而不是算子产生的码（目前只有 `W.EVIDENCE.POSTCONDITION_FAILED`）在 5.1 统一登记，不要求在每个 manifest 的 `warnings[]` 中重复声明；E-7 的"MUST 声明"只约束算子自己产生的码。
- E-12 **抽样的检查只是抽查（v0.3.6）。** manifest 的 `receipt.coverage` 为每个后置条件声明它覆盖了多少结果：`full`（在整个结果上求值；缺省）、`test_points`（一个结果必须处处满足的恒等式，在算子事先固定、与结果无关的若干点上核对；结果本身由有限次精确形式的运算得到，例如组合算子的 I-9）、`sampled`（只在结果的子集、网格或压缩副本上求值，例如 `margin` 在频率网格上复查有没有漏掉穿越点）。所有后置条件为 true 但其中有 `sampled` 时，claim MUST 是 `spot_checked`，不是 `verified`。收据 `evidence.coverage` 抄录完整的映射（未声明者写 `full`）；没有声明 coverage 的算子 MUST NOT 出现这个字段（已发布收据字节不变）。绑定层抽样后的视图（F-6 的 `_downsampled`）、模型自己写的摘要都不是证据：MUST NOT 在它们上面计算任何后置条件。v0.3.6 声明 `sampled` 的有：`margin.grid_audit_clean`、`loopsens.sub_operator_checks`（因为裕度来自 `margin`）；声明 `test_points` 的有 `series`/`parallel`/`feedback`/`convert` 的 `response_consistent`、`pid.formula_matches`、`loopsens.closed_loop_identity`。
- E-13 **失败的检查要指出哪些字段不能用（v0.3.6）。** EngDesign 的 XG_02 是 7 阶系统：Krylov 可控性矩阵的数值秩是 4，PBH 检验认为满秩，`pbh_agrees_ctrb` 为 false，claim 正确地为 `unverified`；但信封没有说哪些字段因此不能用，模型看到的又是被抽样的矩阵，`rank`、`full_rank`、`minimal` 看起来和正常结果一样。manifest 的 `receipt.guards` 把后置条件映射到它保护的结果字段（JSON Pointer）；某个检查失败时，收据 `evidence.unreliable` MUST 列出这些字段（排序去重），`W.EVIDENCE.POSTCONDITION_FAILED` 的消息与 `details.unreliable` 写出同一列表，绑定层 MUST 把它作为 `unreliable_fields` 告诉模型，系统提示词 MUST 要求“不得引用、不得作为下一步的依据”。v0.3.6：`ctrb_obsv` 的 `pbh_agrees_ctrb` → `/ctrb/rank`、`/ctrb/full_rank`、`/minimal`（obsv 同理）；组合算子的 `response_consistent` → `/system`；`loopsens` 见其 manifest。

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

### 4.3 结果收据（Result Receipt，v0.3.3 新增）

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
- R-9 **方法选择（v0.4）**：请求带 `exec.method_policy` 时收据多一个 `method_selection`（3.14 MS-7）；重放以 `require: [used]` 进行，环境没有该方法时 verdict 为 `method_unavailable`（MS-9）。`exec` 不进入 `input_hash`（P-2），所以策略不影响 `fingerprint.request`。

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

实测（v0.3.3）：615 个 golden 成功用例全部给出收据，同机重放全部 `bitwise_match`，30 个错误用例全部 `error_reproduced`（`tests/receipt.rs`，`reports/receipts.json`）；claim 分布：10 个算子全部 `verified`，`poles_zeros` 与 `lyap` 为 `unchecked`（没有独立的后验检查，见 11.4）；没有 `unverified`（v0.3.6 复测见 11.3：`margin` 与 `loopsens` 因 E-12 为 `spot_checked`）。36 个已发布示例（原生生成）在 wasm 中重放：19 个 `bitwise_match`、5 个 `tolerance_match`（最大相对差 4.2e-15）、12 个 `error_reproduced`（`rust/aeoi-wasm/parity/verify_examples.mjs`，CI 冒烟测试的一部分）。

## 5. 结构化错误

### 5.1 错误码分类

格式：`E.<CATEGORY>.<NAME>`。类别固定为 5 个；`NAME` 由注册表统一登记，算子 MUST 在 manifest `errors[]` 中列出可能返回的全部错误码（参考实现在 parity 测试中逐次断言这一点）。

| 类别 | 何时使用 | `retryable` 默认 | 已登记的码（v0.3 新增的加粗） |
|---|---|---|---|
| `VALIDATION` | 输入不符合 schema 或语义约束 | true（改输入） | `SCHEMA`、`SHAPE`、`DTYPE`、`RANGE`、`NON_FINITE`、`NOT_SYMMETRIC`、`NOT_CONJUGATE`、`ZERO_DENOMINATOR`、`GRID`、`SPARSE_STRUCTURE`、`DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN`、**`OPTION_CONFLICT`**、**`RANK_DEFICIENT`**、**`UNGROUNDED_INPUT`**（v0.3.1，绑定层）、**`METHOD_UNAVAILABLE`**（v0.4） |
| `DOMAIN` | 输入合法，但数学问题本身无解/不适用 | 视情况 | `IMPROPER_SYSTEM`、`NOT_STABILIZABLE`、`R_NOT_POSITIVE_DEFINITE`、`WEIGHT_NOT_PSD`、`HAMILTONIAN_IMAG_AXIS_EIG`、`ABOVE_NYQUIST`、**`MIMO_NOT_SUPPORTED`**、**`DEGENERATE_SYSTEM`**、**`NOT_CONTINUOUS`**、**`NOT_CONTROLLABLE`**、**`POLE_MULTIPLICITY`**、**`LYAP_SINGULAR`**、**`SYMPLECTIC_UNIT_CIRCLE_EIG`** |
| `NUMERICAL` | 问题理论上可解，但数值上失败或不可信 | true（正则化/放宽/换算法） | `SINGULAR`、`NO_CONVERGENCE`、`POLE_ON_GRID` |
| `RESOURCE` | 超时、超规模、被取消 | true | `TIMEOUT`、`INPUT_TOO_LARGE`、`COMPUTE_LIMIT`、`OUTPUT_TOO_LARGE` |
| `INTERNAL` | 实现缺陷或后端异常 | false（可原样重试一次） | `PANIC`（`BACKEND`、`INVARIANT_VIOLATED` 为保留码，v0.2 算子未使用） |

已登记的 warning（v0.3.3 新增框架级 `W.EVIDENCE.POSTCONDITION_FAILED`，见 E-7a、E-11）：`W.NUMERICAL.{NEAR_POLE, ZERO_ON_GRID, ILL_CONDITIONED, LARGE_RESIDUAL, NEAR_UNCONTROLLABLE, **RANK_AMBIGUOUS, CROSSING_RECOVERED, PLACEMENT_INACCURATE, CONVERSION_DISCREPANCY**}`、`W.DOMAIN.{UNSTABLE_SYSTEM, NOT_SETTLED, INFINITE_DC_GAIN, ZERO_DC_GAIN, **UNDEFINED_DC_GAIN, NONMINIMAL_REALIZATION, REALIZATION_DEPENDENT, GRAMIAN_UNDEFINED, CLOSED_LOOP_UNSTABLE, DIRECT_FEEDTHROUGH_IGNORED**}`、`W.ACCURACY.{GRID_TOO_COARSE, AUTO_GRID, **ALIASING**}`、`W.VALIDATION.{LEADING_ZEROS_STRIPPED, SYMMETRIZED}`、`W.PERFORMANCE.SPARSE_DENSIFIED`（加粗为 v0.3 新增）。

v0.3.1 新增码：`UNGROUNDED_INPUT` 由 LLM 绑定层（不是算子）返回，表示 `x-aeoi-provenance: "user"` 的输入没有用户原话作为来源（F-5(c)），`retryable: true` 但只能在问过用户之后重试。v0.3 新增码的含义：`OPTION_CONFLICT` 选项组合矛盾（O-2）；`RANK_DEFICIENT` 输入矩阵列不满秩（`place` 的 B）；`MIMO_NOT_SUPPORTED` 该算子/表示只支持 SISO（R-8、`margin`）；`DEGENERATE_SYSTEM` 系统矩阵非满法秩，零点无定义（R-9）；`NOT_CONTINUOUS` 需要连续系统却给了离散系统（`c2d`）；`NOT_CONTROLLABLE` 有不可控模态，无法任意配置极点（`place`；与 `NOT_STABILIZABLE` 区分：后者只关心不稳定模态）；`POLE_MULTIPLICITY` 期望极点重数超过输入数 m；`LYAP_SINGULAR` Lyapunov 方程奇异（连续 λᵢ + λ̄ⱼ ≈ 0，离散 λᵢλ̄ⱼ ≈ 1）；`SYMPLECTIC_UNIT_CIRCLE_EIG` DARE 的辛矩阵束在单位圆上有特征值（单位圆上的模态不被 Q 观测）。这些都是 `DOMAIN`/`VALIDATION`：调数值参数无效（X-1）。

v0.4 新增码：`METHOD_UNAVAILABLE` 由算子返回（3.14 MS-6）：`exec.method_policy` 的硬约束没有任何已登记的方法能满足。`retryable: true`，修复是改策略或问用户。类别固定为 5 个，所以它归 `VALIDATION`（改输入可以解决），不另设 `METHOD` 类别。

规则：
- X-1 `DOMAIN` 与 `NUMERICAL` MUST 区分清楚：前者告诉 Agent"别再调参了，问题本身不成立"，后者告诉 Agent"换个数值处理可能成功"。这是本规范对 Agent 最重要的区分。
- X-2 FFI 后端的每个返回码 MUST 映射到具体错误码；MUST NOT 把后端错误笼统报为 `INTERNAL.BACKEND`，除非确实无法归类。
- X-3 Rust `panic` MUST 被 `catch_unwind` 捕获并转为 `E.INTERNAL.PANIC`，FFI 边界 MUST NOT 让 panic 或 Fortran 异常跨越。
- X-4 数值"勉强成功"SHOULD 返回成功 + `W.NUMERICAL.ILL_CONDITIONED`，而不是错误；阈值 MUST 写在 manifest。
- X-4a **数值判据 MUST 与问题尺度无关（v0.2 新增）**：判断"特征值在虚轴上"、"矩阵奇异"等的阈值 MUST 以机器精度 u 与相关范数表达（如 `|Re λ| ≤ 10³·u·‖H‖₁`），MUST NOT 用与尺度无关的固定小数。（v0.2 实现初版用 `1e-8·‖H‖₁`，在 cond(R)=10⁹ 的合法问题上误报 `HAMILTONIAN_IMAG_AXIS_EIG`，被 parity 测试发现。）**（v0.3.2 补充）** 以机器精度表达仍不够：范数本身 MUST 不被某个可任意缩放的块主导。`lqr` 0.2.0 的 `10³·u·‖H‖₁` 中 ‖H‖₁ 由 ‖BR⁻¹Bᵀ‖ 主导，R = diag(1, 1e-12) 的良态解耦问题被误报（tpt-control 交叉验证发现）；0.3.0 改为在辛对角平衡（状态尺度变换）后的 H̃ 上判断，且只把"接近虚轴"当作怀疑，用直接判据（PBH 不可观检验）确认或求解后检验闭环。
- X-4b **规模类错误要区分（v0.2 新增）**：`COMPUTE_LIMIT`（计算量超限，remediation 改粗网格）与 `OUTPUT_TOO_LARGE`（计算可做但返回体过大，remediation 抽样或只返回指标）MUST 分开，因为两者的有效修复不同。

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
- X-9 **（v0.2 新增）** `call_operator` MUST 引用同一注册表中已存在的算子（v0.1 示例引用了不存在的 `control.lti.ctrb_decomp`，已删除）。`modify_input` 的 patch 应用后 SHOULD 能通过校验；参考实现对 `step` 的 `COMPUTE_LIMIT` 示例测试了"应用 patch 后调用成功"。

## 6. 可复现性

- P-1 **版本**：`build` 字符串 MUST 包含 crate 版本、git commit（无仓库时写 `nogit`）、主要数值依赖版本、编译器版本。任何一项变化都视为不同 build。
- P-2 **哈希**：`input_hash` = SHA-256（RFC 8785 JCS 规范化后的请求 arguments，去掉 `exec`，数组按 3.7 替换为内容摘要）；`output_hash` 同理作用于 `result`。浮点数序列化 MUST 使用 JCS 的 ECMAScript 数字格式（最短往返表示）。
- P-3 **线程与 BLAS**：`bitwise` 算子 MUST 单线程运行或使用确定性归约顺序；BLAS/LAPACK MUST 锁定版本与内核，MUST NOT 使用运行时动态分派到未声明的内核——做不到时 MUST 声明为 `platform_bitwise`，并在 `reproducibility.cpu_features` 中记录分派依据（v0.2 参考实现即属此类，`blas: "none"`，单线程）。
- P-4 **浮点策略**：MUST NOT 启用 fast-math 类优化（重结合、flush-to-zero、近似倒数）。Rust 默认不做 FMA 收缩；C/Fortran 后端编译时 MUST 加 `-ffp-contract=off`（或等价选项）。
- P-5 **随机性**：随机算法 MUST 为 `seeded`，PRNG 算法固定（建议 ChaCha20），`seed` 回显在 `reproducibility.seed`。
- P-6 **容差**：算子内部的收敛容差 MUST 写在 manifest（含默认值）；允许调用方覆盖的容差（如 `lqr.symmetry_tolerance`）MUST 出现在 input schema 中并回显。
- P-7 **重放**：同 `build` + 同平台 + 同 `input_hash` 的两次调用，`output_hash` MUST 相同。CI MUST 有"同机重放"测试（参考实现：`deterministic_repeat_calls_bitwise_equal`，以及示例响应的 `output_hash` 回归比对；另实测 debug 与 release 构建的三个示例 `output_hash` 相同）。
- P-8 **无隐藏状态**：算子 MUST 是纯函数，不得读取环境变量、时钟（`timing` 除外）或全局可变状态影响结果；文件读取仅限 3.6 的 `ref` 输入，且受 data root 约束。

## 7. 验证凭证（Verification Credentials）

### 7.1 生成与存储 golden 数据

- V-1 参考数据来源优先级：① MATLAB（ground truth，正版授权）② 解析解/高精度计算（如 mpmath）③ python-control/scipy、GNU Octave 等开源实现（开发阶段对照，不能单独授予 A 级）。
- V-2 MATLAB 侧只运行公开 API 的生成脚本（`matlab/gen_golden_<op>.m`），固定 `rng(seed)`，记录 `version`、工具箱版本、`version('-blas')`、`version('-lapack')`、`computer('arch')`、`maxNumCompThreads`。脚本 MUST 遵守第 0 章内存序规则。
- V-3 **（v0.2 修改）** golden 目录包含参考引擎名：`golden/<engine>/<op>/<op_version>/<sha256>.json`（如 `golden/python-control/…`、`golden/MATLAB/…`），以免不同参考的数据混在一起被误当作 MATLAB 结果。文件名 = 内容 SHA-256。每个文件 MUST 带 `reference_env`（引擎与版本）。golden 文件 MUST 纳入版本库或制品仓库，MUST NOT 在 CI 中重新生成。
- V-3a **（v0.3.2 新增）golden 的版本归属**：`<op_version>` 是生成该 golden 时的算子版本。算子升级而 input schema 与输出语义不变时，新版本 MAY 继续使用旧版本目录下的 golden（参考值与算子版本无关，MUST NOT 为改目录名而重新生成或改写文件），parity.yaml MUST 列出实际使用的 `golden_store`；为新版本新增的用例写入新版本目录。v0.3.6 补充：只**增加**可选输入或输出字段的 MINOR 升级（G-2），只要旧请求的结果在旧字段上逐位不变（参考实现：位置参数调用的 `series`/`feedback` 0.4.0），同样 MAY 沿用旧目录；新增输入形式（如具名角色）的用例 SHOULD 写入新版本目录或由单元测试覆盖。高精度参考（V-1 ②）用引擎名 `mpmath`：`golden/mpmath/<op>/<ver>/`，`reference_env` 写明 mpmath 版本、精度位数与求解方法。
- V-4 parity 测试 MUST 使用显式输入（显式频点、显式时间网格），MUST NOT 依赖参考实现的自动网格/自动选择逻辑。
- V-5 非唯一输出（特征向量、状态空间实现、Schur 基）MUST 比较不变量（特征值集合、传递函数、投影子），不得逐元素比较。
- V-6 比较公式：|x − x_ref| ≤ atol + rtol·|x_ref|；矩阵 SHOULD 用范数形式。**（v0.2 细化）** 病态问题的容差 MAY 按以下两种方式放宽，且 MUST 写入 parity spec、MUST 在测试报告中单独统计被放宽的用例数：
  - 条件数缩放：`rtol × max(1, κ/10²)`（`lqr` 用 κ = cond(S)）；
  - 误差估计缩放：`max(class_tol, 10 × trust.error_estimate)`——只有当算子自报的误差估计本身已经较大时才放宽（如 `bode` 的近极点频点）。这同时检验了误差估计不是"夸大的好消息"：实际偏差必须落在算子自报估计的 10 倍以内。
  - **（v0.3 新增）根与系数的条件数缩放**：多项式根/特征值逐个按最近匹配比较，容差 = class_tol × max(1, κᵣ)，κᵣ = Σ|aₖ||r|^{n−k} / (max(1,|r|)·|p′(r)|)（参考多项式的 Wilkinson 相对条件数）；由特征值展开得到的多项式系数，容差 = class_tol × max(1, Π(1+|λᵢ|)/‖p‖₁)（系数对特征值扰动的一阶界）。DC 增益用 κ = Π(1+|p|)/Π|p|；Tustin 离散化用 κ = cond(I − αA)；zoh/foh 用 κ = e^{‖A‖₁·T}。
- V-5a **（v0.3 新增）非唯一的设计结果**：多输入极点配置的 K 不唯一，MUST NOT 与参考的 K 比较；parity 测试 MUST 用**参考请求中的 A、B**（而不是算子自报的 `achieved_poles`）重算 eig(A − B·K) 并与期望极点比较——否则把 A 按 column-major 传错的请求也会"成功配置极点"，内存序错误无法检出。闭环特征向量矩阵的条件数与参考（scipy YT）之比作为信息报告。
- V-7 预期报错用例 MUST 比对错误码；预期 warning 用例 MUST 断言 warning 出现。
- V-9 **（v0.3 新增）参考值的后处理** MUST 写入 parity spec 并说明原因。例：python-control（无 slycot）用 scipy QZ 计算零点时，会把无穷远广义特征值返回成 ~10¹⁶–10¹⁸ 的有限数；参考生成器丢弃 |z| > 10¹⁰·(1+‖[A B; C D]‖_F) 的"零点"。
- V-10 **（v0.3 新增）参考本身是局部优化结果时**（python-control 离散 `stability_margins` 用 scipy L-BFGS-B 求 min|1+G|），比较规则为：在类容差内一致，或被测值更小（更好的极小值）且参考值与在参考频率处重新求值的 |1+G| 一致（≤ 1e-9）、差距 ≤ 1e-6；频率按优化器精度比较（rel 1e-3）。这类规则 MUST 在报告中单独标注。

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

v0.3.5 参考实现的 18 个算子当前均为 **B 级**（python-control / mpmath parity + 解析解单元测试 + 不变量），目标 A 级。v0.3.5 的 6 个新算子没有 MATLAB 对照，等级同样是 B。

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
| `outputSchema` | 信封 schema，`result` 替换为 `output_schema` | |
| `annotations` | `readOnlyHint: true`、`idempotentHint: true`、`openWorldHint: false` | 纯计算算子 |
| `_meta` | `aeoi/id`、`aeoi/version`、`aeoi/grade`、`aeoi/spec`、`aeoi/manifest` | 保留原始 id |

- C-1 算子层错误（`E.*`，含 schema 校验失败）MUST 作为正常 tool result 返回，`isError: true` 且 `structuredContent` 为错误信封；MUST NOT 用 JSON-RPC 协议错误，否则 Agent 看不到 remediation。JSON-RPC 错误只用于协议层问题（未知工具 `-32602`、未知方法 `-32601`、解析失败 `-32700`、资源不存在 `-32002`）。
- C-2 server SHOULD 暴露资源：`aeoi://operators`（目录）、`aeoi://operators/{id}/{version}/manifest`、`…/credentials`、`aeoi://schemas/{name}/{ver}`。（参考实现已提供目录、manifest 与 schema；credentials 资源尚未提供。）
- C-3 大型输出 SHOULD 用 `exec.output_encoding = "b64"`（L-5）；超过 `limits` 时返回 `E.RESOURCE.OUTPUT_TOO_LARGE` 并给出抽样/只返回指标的 remediation。基于 resource link 的输出留待 v0.3。
- C-4 工具数量多时，server SHOULD 支持按命名空间过滤（参考实现：`aeoi-mcp --namespace control.lti`）。
- C-6 **（v0.3.3 新增）** server SHOULD 提供只读工具 `aeoi_verify`（4.3 R-5/R-6），参数 `{operator, arguments, envelope}`，返回 verdict 对象（`structuredContent`；不通过时 `isError: true`）。它不是算子，没有 manifest；`tools/list` 中以 `_meta["aeoi/kind"] = "verifier"` 标出。
- C-5 `initialize` 的 `protocolVersion` 协商：参考实现支持 `2025-06-18`、`2025-03-26`、`2024-11-05`，客户端请求其他版本时回落到 `2025-06-18`。

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

### 8.3 绑定层

| 绑定 | crate | 状态 |
|---|---|---|
| MCP server | `aeoi-mcp` | **已实现**（stdio，换行分隔 JSON-RPC 2.0；手写最小实现，不依赖 `rmcp`，以保持依赖少、兼容 Rust 1.85）。Streamable HTTP 未实现 |
| LLM 函数调用 | `aeoi-web` | **已实现（v0.3.1）**：Gemini（必选）与 OpenAI 兼容 provider；进程内调用；网页测试台 + 评测 CLI（8.4） |
| 浏览器（WebAssembly） | `aeoi-wasm` + `pages/` | **已实现（v0.3.1）**：算子与工具桥编译为 wasm（wasm-bindgen），Agent 循环在页面里，模型由浏览器用访问者自己的 key 直接调用；发布在 GitHub Pages（8.5） |
| CLI | `aeoi-call`（在 `aeoi-control` 中） | **已实现**：`aeoi-call <op-id|tool_name> <request.json|-> [--data-root DIR]`；v0.3.3：`aeoi-call verify <op-id> <request.json> <envelope.json>` |
| Python | `aeoi-py`（PyO3 + maturin） | 计划：`aeoi.call(id, args) -> dict`；NumPy 便捷层 MUST 遵守内存序规则；MUST 释放 GIL |
| C ABI | `aeoi-capi`（cdylib） | 计划：`aeoi_call(const char* id, const char* json_args, char** json_envelope)` + `aeoi_free` |

- B-1 所有绑定 MUST 返回同一信封结构；绑定层 MUST NOT 自行改写数值或错误码。
- B-2 Python 绑定 MUST 释放 GIL 执行长计算。

### 8.4 LLM 函数调用绑定（`aeoi-web`，v0.3.1 新增）

`aeoi-web` 是端到端测试台：云端 LLM 通过函数调用使用 19 个算子，网页显示对话与逐步轨迹，CLI 跑评测。

**调用方式**：进程内调用 `aeoi_control::registry()` 的 `DynOperator::call`。这与 `aeoi-mcp` 是同一条管线（schema 校验 → validate → execute → 信封），信封逐字节相同；不经过 stdio 子进程与 JSON-RPC 帧，Windows 上部署更简单，延迟就是算子本身的耗时（亚毫秒到毫秒）。MCP 路径已由 `aeoi-mcp` 的端到端测试覆盖。

**函数声明**：`name` = `manifest.mcp.tool_name`；`description` = `[id] description` + 前提条件 + 不适用范围（+ "仅 SISO"）；`parameters` 由 input schema 两步生成：

1. 收窄与精简（所有 provider，F-6）：`Matrix` 只保留 `DenseMatrix` 分支并固定 `dtype: "f64"`、`shape` 为二维；去掉 `exec`、`time_unit`、`state_names`/`input_names`/`output_names`、`structure`；`x-aeoi-shape-constraints` 并入 description（F-2）。OpenAI 兼容 provider 直接使用这一步的 JSON Schema。
2. Gemini OpenAPI 子集转换（`schema_conv::to_gemini`）：

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

12 个工具的有损映射次数可用 `aeoi-web tools --losses` 打印（当前：`additionalProperties:false` 46 处、`oneOf→anyOf` 27 处、`default` 19 处、`prefixItems` 17 处、`exclusiveMinimum` 15 处）。所有声明合计约 38 KB JSON；Gemini 2.5 Flash 上每次请求约 1.7 万输入 token，其中大部分是工具声明。

**Agent 循环**：系统提示词（`rust/aeoi-web/src/system_prompt.zh.md`）用中文说明 row-major、tf 降幂、`ts` 含义、`rad/s` 与 `Hz`、`"Inf"`/`null`、remediation 的使用规则与工具选择提示。循环为"模型 → 工具调用（可并行多个）→ 信封 → 模型"，最多 `max_steps` 个模型回合（默认 8）。错误信封（含 `agent_hint` 与 `remediation`）原样回传；Gemini 的 `thoughtSignature` 按原样回传；超过 120 个数的数组按 F-6 抽样。"自我纠错"= 某工具报错后同一工具随后调用成功。v0.3.6 起 `LtiSystem` 对象从不抽样（K-6）。

**回合预算（v0.3.6，AL = agent loop）**：真模型运行里 7 题有 4 题在已经拿到 `stepinfo`/`margin` 结果之后仍不调用 `submit_design`，10 个回合用完，没有分数。
- AL-1 有回合上限、且只能通过某个工具提交答案的循环（设计评测的 `submit_design`）MUST 在系统提示词里写明上限（“最多 N 个模型回合，最后一个回合只能提交”）。
- AL-2 剩余 2 个回合（含本回合）时，绑定层 MUST 注入一条以“（系统提示）”开头的提示，要求在本回合或下一回合提交；最后一个回合 MUST 只提供提交工具，并再注入一条“这是最后一个回合，只能提交，在说明里写明哪些指标没有用工具确认”。这些注入不算用户原话（F-5(c) 的来源核对排除它们）。`max_steps < 3` 时不发第一条提示。
- AL-3 绑定层 MUST NOT 替模型提交（例如把最后一次调用的参数当答案）：提交的内容必须是模型自己写的。报告 MUST 记录注入次数（`budget_notices`）。没有提交仍记为未提交、0 分。

**来源核对（F-5(c)）**：对 `x-aeoi-provenance: "user"` 的参数（v0.3.1 只有 `c2d.ts`），函数声明多一个必填参数 `_user_quote`，工具描述末尾写明"必须由用户给出、不得假设的参数：ts（用户没给时先询问，不要调用本工具）"。绑定层在调用算子前核对引用：把用户本人的消息（不含"（系统提示）"开头的注入提示）与引用都去掉空白、`$`、`{}` 等 LaTeX 符号后做子串匹配，并要求引用中有与参数相同的数值（`ms`/`μs` 按单位换算）。不通过则返回 `E.VALIDATION.UNGROUNDED_INPUT`（`details.field/value/quote`，remediation 只有 `ask_user`），不调用算子。直接在网页上调用算子（`/api/call`）与 `rescore` 重算旧报告时不做核对，只移除 `_user_quote`。

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
11. 生成凭证、确定等级与合规等级、发布

### 9.4 跨实现一致性（v0.4）

9.1 的合规等级评价一个算子实现自身的质量（schema、信封、可复现、凭证）。本节回答另一个问题：**换一个实现，行为是否等价于参考实现。** 两者独立：一个实现可以是 L2 却通不过本节的套件，反过来也一样。本节规定判定规则和套件的组成；套件的 v0.1（`conformance/`：运行器、独立的哈希重算、只含 `lyap` 的 Python 第二实现）已经有了，只有 `lyap` 带数值规则，参考实现还没有用它跑过（`conformance/README.md`）。

- CF-1 **对象与形态。** 一致性有两类对象：算子实现（CF-1 – CF-11），以及把算子暴露给模型的绑定层（CF-12）。算子实现有两种接入形态，与 9.1 的 L1 – L3 无关：
  - **线级（wire）**：独立的可执行体，通过 MCP stdio 暴露算子（8.1、C-1），运行器只与它交互。任何语言都能做到。
  - **内核级（kernel）**：只提供数值内核，挂到宿主（如 `aeoi-core`）里，校验、收据与指纹仍由宿主负责。内核接口尚未定义（12.1 第 17 项）。它不是 8.3 的 `aeoi-capi`：后者是“调用 AEOI 的 C ABI”，把整个算子包成 JSON 进、JSON 出，不是把内核插进宿主的接口。

  一致性按（算子 id、版本、方法 id）逐项声明（CF-11）；没有声明的算子，实现不得宣称遵守 AEOI。
- CF-2 **套件沿用 golden 与 `parity.yaml`，不另立用例格式。** 一个用例就是 `golden/<engine>/<op>/<ver>/<sha256>.json`（`request`、`expected`、`reference_env`、`case_family`）；用例族的容差类、`compare` 规则、预期错误与预期警告写在该算子的 `parity.yaml`（V-4 – V-10）。套件 = 这两者 + CF-10 的方法用例。含矩阵输入的算子 MUST 带 `memory_order_nonsymmetric` 族（第 0 章）：运行器断言按列主序展平的请求，其结果偏离参考超过容差。这是对第二实现最便宜也最有效的检验。
- CF-3 **通信与运行。** 运行器通过 MCP stdio 驱动被测实现，只做这一种（`initialize`、`tools/list`、`tools/call`、`resources/read`）；manifest 由资源 `aeoi://operators/{id}/{version}/manifest`（M-1）读取。非 MCP 的传统程序要通过线级套件，需要自带一层薄的 MCP stdio 适配。运行器命令（v0.1 已实现，只覆盖 `lyap`，见 `conformance/README.md`）：`python3 conformance/aeoi_conformance.py run --impl "<启动命令>" --operators all|<id,…> --report report.json`。对声明 `bitwise`、`platform_bitwise` 或 `seeded` 的算子（M-7），运行器对每个请求运行两次，`output_hash` MUST 相同（P-7）。
- CF-4 **必须相同的字段。** 信封的 `status` 是否为 `error`；`error.code`（V-7）；用例 `expect_warning` 列出的 warning 码 MUST 出现（V-7）；算子 id 与版本；`fingerprint.request`。最后一项成立的原因：它是 `H({operator, version, input_hash})`，而 `input_hash` 与编码无关（H-1），所以任何实现对同一请求 MUST 算出相同的值。
- CF-5 **不要求相同的字段。**
  - `ok` 与 `ok_with_warnings` 的区别。基于阈值的 warning 会在不同实现、不同平台上翻转（8.5），所以只核对 CF-4 列出的 warning，其余不作要求；E-2 保证 `status` 只由 warnings 是否为空决定，不需要另核。
  - `fingerprint.result` 与 `output_hash`。不同内核的末位舍入不同，不要求相同（4.3 R-3）；要求的是 P-7（同一实现、同一平台、同一请求，重复运行相同）。
  - 方法 id：不要求等于参考实现的方法 id（MS-1、MS-2）。
  - `timing`、`build`、`platform`、`cpu_features`。
- CF-6 **数值比较。**
  - 沿用 `parity.yaml` 的 `compare`：对每个顶层输出字段按相对范数（`rel_fro`，标量退化为相对误差，向量为 2-范数）；字符串、布尔、键与数组长度必须完全相同。**多项式系数同样按范数比较，不逐元素。**
  - 无序量（极点、零点、特征值）按 V-5 与 3.10 R-2 做集合匹配；状态空间实现、特征向量、Schur 基比较不变量（V-5）；多输入 `place` 的 K 用 V-5a；`stepinfo` 的时间量按 I-10。
  - 容差类与 κ 缩放由套件固定在 `parity.yaml`（7.2、V-6）；**κ 由运行器用参考请求计算，不采信被测实现自报的任何量。** V-6 中“`max(class_tol, 10 × trust.error_estimate)`”这种放宽只对参考实现开放；第二个实现自报的 `error_estimate` MUST NOT 用来放宽验收，否则它把估计报大就能通过任何用例。
  - 4.3 R-5 的 `verify` 用 ‖·‖∞ 做重放比较，`parity.yaml` 用 `rel_fro`；两种范数并存，一致性套件用后者（12.1 第 14 项）。
- CF-7 **验收容差与自报误差界。** 验收容差由套件决定（CF-6），是“合规下限”。被测实现的收据里 `exactness.bound_kind` 为 `rigorous` 或 `a_priori` 且 `bound` 是数值时，`bound` MUST 不大于该用例的验收容差，否则声明本身不合规；`a_posteriori_heuristic` 与 `none` 只作信息记录，通过与否只看实际偏差。个别用例上确有理由的更宽偏差 MUST 登记在声明的 `known_deviations`（用例、实际偏差、原因），记为“带偏差通过”，不计入“完全通过”。
- CF-8 **收据与 manifest 的机械核查。** 运行器不采信自报：
  - manifest 通过 `schemas/manifest.schema.json`，`provenance.clean_room.declared` 为 true（第 0 章）；响应通过 envelope schema 与该算子的 output schema（`parity.yaml` 的 `pass_rule`）。
  - 响应里出现的每个 `E.*`、`W.*` 码 MUST 在该 manifest 中声明（E-7、5.1；框架级码见 E-7a）。
  - `receipt.evidence.claim` 由运行器按 E-11、E-12，用 manifest 的 `receipt.postconditions` 与 `coverage`、响应的 `trust.checks` 重新推出，MUST 与响应一致：后置条件为 false 时不能声称 `verified`。
  - 收据的 `method.id` MUST 属于 manifest 允许的方法集合（MS-1）。
  - （推荐）对 `lyap`、`lqr`、`place`、`poles_zeros` 等，运行器用与被测实现不同的代码路径独立复核后置条件（残差、闭环极点位置），做法参照 `validate_examples.py` 对 E-12、E-13 的独立核对。
- CF-9 **错误、边界与 remediation。** 套件除正常路径外 MUST 包含：manifest 声明的每个错误码至少一个触发用例（`parity.yaml` 的 `expect`）；保护性拒绝（如 `series` 的相对阶上限，I-7）；带 `modify_input` remediation 的错误，运行器把补丁套到请求上重新调用，结果 MUST 通过校验（X-9）。
- CF-10 **方法用例。** 对 manifest 里每个已登记的方法（MS-1），运行器 MUST 以 `exec.method_policy = {"require": ["<该方法 id>"]}` 跑该算子的全部用例，每个方法单独计数（MS-3）；声明了方法却没被检验的实现不能通过。方法选择另有用例：缺省选 `default`；`require` 已登记的方法成功且 `method_selection.used` 相符；`require` 未登记的 id、无解的 `constraints` 返回 `E.VALIDATION.METHOD_UNAVAILABLE` 且 `details` 列出可用方法；含 `method_policy` 的请求发给不支持它的实现得到 `E.VALIDATION.SCHEMA`（MS-8）。只有一个方法的算子只需通过缺省、`require` 未登记 id、`require` 该唯一方法这三项。这些用例用 golden 同样的文件格式，`request.exec` 带 `method_policy`；**不同方法的结果都 MUST 落在同一个验收容差内**，这正是对“换方法不改变结果”的检验。
- CF-11 **声明与报告。** 运行器产生报告，实现发布机器可读的声明，至少包含：实现名称、版本、形态（线级或内核级）；套件版本与 `suite_hash`；逐（算子、版本、方法 id）的结果（`pass`、`pass_with_deviation`、`fail`、`skip`）；`known_deviations`；通过用例按 `reference_env.engine` 的计数，以及该引擎是否与被测方法的 `backend.library`（M-6）同源；所用内核与数值库及其版本。同源的参考值只能证明套件可移植，不能证明实现可替换，所以声明 MUST 把“同源通过”和“独立通过”（mpmath、解析解，V-1 ②）分开列出。

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

## 10. 完整示例

`bode` 与 `step` 使用同一被控对象，`lqr` 使用经典双积分器；v0.3 新增算子（10.4–10.12）的示例尽量复用同一被控对象，并都给出可手算核对的值。10.4–10.12 中用 `jsonc` 占位符 内联的 JSON 为了可读性把超过 16 个元素的数值数组缩写为 `"…(+N)…"`，完整内容见对应文件。**所有响应都是 Rust 参考实现的实际输出**（`regen_examples.py` 调用 `aeoi-call` 生成），并经 `cargo test` 与 python-control 结果比对；不是 MATLAB 输出。每个示例的完整文件在 `examples/<id>/`。

被控对象：G(s) = 4 / (s² + 1.2s + 4)（ωn = 2 rad/s，ζ = 0.3），极点 −0.6 ± 1.9079j。

### 10.1 `control.lti.bode`

语义要点：
- 频点：`explicit` 时 MUST 严格递增（相位 unwrap 依赖顺序；否则 `E.VALIDATION.GRID`，remediation 给出排序后的 patch）；`logspace` 时 `min < max`。输入可为 `Hz`，输出 `w` 一律 `rad/s`。离散系统频点 MUST ≤ π/ts（`E.DOMAIN.ABOVE_NYQUIST`）。
- 相位约定：`branch_center_deg = c`（默认 0）。`unwrap = true` 时首个频点相位取 (c−180, c+180] 内的主值，之后每点取与前一点之差的主值累加（与 `numpy.unwrap` 一致；实测 100 个成功 golden 用例的 unwrapped 相位与 python-control 逐点一致，不只是 mod 360 一致）；`unwrap = false` 时每点都落在该区间。MIMO 按通道独立 unwrap。
- 频点恰为极点：`E.NUMERICAL.POLE_ON_GRID`；恰为零点：幅值 `-Inf` dB + `W.NUMERICAL.ZERO_ON_GRID`；误差估计 > 1e-6 时 `W.NUMERICAL.NEAR_POLE`。
- 求值：tf 用 Horner（带 Higham 运行误差界），zpk 用乘积形式，ss 用逐频点复 LU 解 (jωI−A)x = B，n ≤ 64 时以 SVD 条件数给出启发式误差估计。

**Manifest** — `examples/control.lti.bode/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.bode",
  "version": "0.2.0",
  "title": "Bode frequency response",
  "description": "计算连续或离散 LTI 系统（tf/ss/zpk，SISO 或 MIMO）在给定频率点上的幅值（dB）与相位（deg）。",
  "category": "analysis",
  "tags": [
    "frequency-domain",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "system 为 proper（分子阶次 ≤ 分母阶次；zpk 零点数 ≤ 极点数）",
      "频率点为正、严格递增",
      "离散系统要求 ω ≤ π/ts（Nyquist 频率）"
    ],
    "not_for": [
      "非线性系统",
      "时变系统",
      "带时滞的系统（v0.2 不支持）"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "tf：分子/分母多项式在 s=jω（离散 z=e^{jωts}）处 Horner 复数求值后相除；zpk：乘积形式 k·Π(s−z)/Π(s−p)；ss：每个频点对 (sI−A)X=B 做部分选主元复数 LU 求解，G=CX+D。相位取 atan2，按频率顺序展开（unwrap），首点落在 (c−180°, c+180°]",
    "references": [
      {
        "type": "textbook",
        "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §5.1（Horner 误差界与条件数）",
        "doi": "10.1137/1.9780898718027"
      },
      {
        "type": "textbook",
        "cite": "G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6"
      },
      {
        "type": "paper",
        "cite": "A. J. Laub, Efficient multivariable frequency response computations, IEEE TAC 26(2), 1981（Hessenberg 加速，v0.2 未采用）",
        "doi": "10.1109/TAC.1981.1102605"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "bode::evaluate (Horner / zpk product / complex LU)",
    "version": "0.2.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T3",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": [
      "json",
      "b64",
      "ref"
    ],
    "output": [
      "json",
      "b64"
    ]
  },
  "limits": {
    "max_frequency_points": 100000,
    "max_states_for_svd_condition": 64
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.bode/0.2.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC",
    "E.VALIDATION.GRID",
    "E.VALIDATION.RANGE",
    "E.DOMAIN.ABOVE_NYQUIST",
    "E.NUMERICAL.POLE_ON_GRID"
  ],
  "warnings": [
    "W.NUMERICAL.NEAR_POLE",
    "W.NUMERICAL.ZERO_ON_GRID",
    "W.VALIDATION.LEADING_ZEROS_STRIPPED",
    "W.PERFORMANCE.SPARSE_DENSIFIED"
  ],
  "mcp": {
    "tool_name": "control_lti_bode",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:bode:horner-or-complex-lu",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "grid",
        "statement": "频率网格与单位为调用方所需（rad/s 或 Hz）",
        "enforced_by": "caller"
      },
      {
        "id": "no_pole_on_grid",
        "statement": "频点不落在极点上",
        "enforced_by": "warning:W.NUMERICAL.NEAR_POLE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_priori"
    },
    "postconditions": [
      "all_finite",
      "frequencies_strictly_increasing"
    ]
  }
}
```

**Input schema** — `examples/control.lti.bode/input.schema.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.lti.bode/0.2.0/input",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "system",
    "frequency"
  ],
  "properties": {
    "system": {
      "$ref": "aeoi://schemas/common/0.4/LtiSystem",
      "description": "被分析的 LTI 系统：tf（num/den 按降幂）、ss（A/B/C/D 稠密矩阵，row-major）或 zpk；ts=0 连续，ts>0 为离散采样周期（s）。上一个工具返回了 result.system 时，把那个 JSON 对象原样放在这里，不要手抄或改写系数（spec K-6）"
    },
    "frequency": {
      "oneOf": [
        {
          "type": "object",
          "additionalProperties": false,
          "required": [
            "mode",
            "values",
            "unit"
          ],
          "properties": {
            "mode": {
              "const": "explicit"
            },
            "values": {
              "type": "array",
              "items": {
                "type": "number",
                "exclusiveMinimum": 0
              },
              "minItems": 1,
              "maxItems": 100000,
              "description": "必须严格递增"
            },
            "unit": {
              "enum": [
                "rad/s",
                "Hz"
              ]
            }
          }
        },
        {
          "type": "object",
          "additionalProperties": false,
          "required": [
            "mode",
            "min",
            "max",
            "n",
            "unit"
          ],
          "properties": {
            "mode": {
              "const": "logspace"
            },
            "min": {
              "type": "number",
              "exclusiveMinimum": 0
            },
            "max": {
              "type": "number",
              "exclusiveMinimum": 0
            },
            "n": {
              "type": "integer",
              "minimum": 2,
              "maximum": 100000
            },
            "unit": {
              "enum": [
                "rad/s",
                "Hz"
              ]
            }
          }
        }
      ],
      "description": "频率网格：explicit（values 严格递增）或 logspace（min..max 共 n 点，对数均匀）；unit 为 rad/s 或 Hz，必须与用户给出的单位一致（ω = 2π·f）"
    },
    "phase": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "unwrap": {
          "type": "boolean",
          "default": true
        },
        "branch_center_deg": {
          "type": "number",
          "default": 0,
          "description": "首个频点相位落在 (c-180, c+180] 内；unwrap=false 时每个点都落在该区间"
        }
      },
      "description": "相位展开与分支选择（可省略）"
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.4/ExecOptions"
    }
  }
}
```

**请求**

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4
    ],
    "den": [
      1,
      1.2,
      4
    ],
    "ts": 0,
    "time_unit": "s"
  },
  "frequency": {
    "mode": "explicit",
    "values": [
      0.1,
      0.5,
      1.0,
      2.0,
      5.0,
      10.0
    ],
    "unit": "rad/s"
  },
  "phase": {
    "unwrap": true
  },
  "exec": {
    "request_id": "req-bode-001"
  }
}
```

**成功响应**（ω = 2 rad/s 处相位正好 −90°、幅值 +4.437 dB = 20·log10(1/(2ζ))，可作手算核对）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.bode",
    "version": "0.2.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-bode-001",
  "result": {
    "w": {
      "unit": "rad/s",
      "value": {
        "dtype": "f64",
        "shape": [
          6
        ],
        "data": [
          0.1,
          0.5,
          1.0,
          2.0,
          5.0,
          10.0
        ]
      }
    },
    "magnitude": {
      "unit": "dB",
      "value": {
        "dtype": "f64",
        "shape": [
          1,
          1,
          6
        ],
        "data": [
          0.017815421224700548,
          0.4507943502574479,
          1.8541948398968155,
          4.436974992327127,
          -14.743983963841893,
          -27.671558660821805
        ]
      }
    },
    "phase": {
      "unit": "deg",
      "value": {
        "dtype": "f64",
        "shape": [
          1,
          1,
          6
        ],
        "data": [
          -1.7226620716228027,
          -9.090276920822323,
          -21.80140948635181,
          -90.0,
          -164.05460409907712,
          -172.8749836510982
        ]
      }
    }
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": null,
    "condition": {
      "representation": "tf",
      "max_cond_sI_minus_A": null,
      "max_rel_error_estimate": 3.2196467714129563e-15
    },
    "error_estimate": {
      "kind": "a_priori_bound",
      "rel_max": 3.2196467714129563e-15,
      "magnitude_db_abs": 2.7965496530045342e-14,
      "phase_deg_abs": 1.844721715248841e-13,
      "worst_frequency_rad_s": 2.0,
      "basis": "Horner 先验误差界（一阶）：|Δp/p| ≤ γ_{2n}·Σ|a_k||s|^k/|p(s)|（Higham 2002 §5.1），分子分母相加；dB 误差 ≈ 8.686×相对误差，相位误差 ≈ 57.30°×相对误差"
    },
    "checks": {
      "all_finite": true,
      "frequencies_strictly_increasing": true
    },
    "accuracy_grade": "B",
    "credentials": null,
    "derived": {
      "poles": {
        "dtype": "c128",
        "shape": [
          2
        ],
        "data": [
          [
            -0.6,
            1.9078784028338913
          ],
          [
            -0.6,
            -1.9078784028338913
          ]
        ]
      },
      "phase_convention": "unwrapped",
      "branch_center_deg": 0.0
    }
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:bode:horner-or-complex-lu",
      "routine": "bode::evaluate (Horner / zpk product / complex LU)",
      "provenance": "aeoi://operators/control.lti.bode/0.2.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "grid",
        "statement": "频率网格与单位为调用方所需（rad/s 或 Hz）",
        "enforced_by": "caller"
      },
      {
        "id": "no_pole_on_grid",
        "statement": "频点不落在极点上",
        "enforced_by": "warning:W.NUMERICAL.NEAR_POLE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_priori",
      "bound": {
        "rel_max": 3.2196467714129563e-15,
        "magnitude_db_abs": 2.7965496530045342e-14,
        "phase_deg_abs": 1.844721715248841e-13
      },
      "basis": "Horner 先验误差界（一阶）：|Δp/p| ≤ γ_{2n}·Σ|a_k||s|^k/|p(s)|（Higham 2002 §5.1），分子分母相加；dB 误差 ≈ 8.686×相对误差，相位误差 ≈ 57.30°×相对误差"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "all_finite": true,
        "frequencies_strictly_increasing": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T3",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:23113923f7eb93fa849b481c1ca5e75f602632ea2d1152659ab9cac96edf148e",
      "result": "sha256:06e4f640fdc1f30acc0af36ccf7c96cd1f1dedd974efe7fc586aa3e4476d1c72",
      "output_hash": "sha256:05bbe7b5711a1fbcf66af8712e3672c8c7c5440965ac9c47450bdaed88856048",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.bode@0.2.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T3",
    "input_hash": "sha256:bfdce579308b4a1bd5ca4e277eb79481fbcd93ea4817cc11fd590eefe4761d28",
    "output_hash": "sha256:05bbe7b5711a1fbcf66af8712e3672c8c7c5440965ac9c47450bdaed88856048",
    "platform": "x86_64-linux",
    "cpu_features": [
      "avx",
      "avx2",
      "fma",
      "avx512f"
    ],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.165242
  }
}
```

**错误请求与响应**（非 proper 系统；remediation 给出加滤波极点的 patch，需用户确认）

```json
{
  "system": {
    "kind": "tf",
    "num": [
      1,
      0,
      0
    ],
    "den": [
      1,
      1
    ],
    "ts": 0,
    "time_unit": "s"
  },
  "frequency": {
    "mode": "logspace",
    "min": 0.01,
    "max": 100,
    "n": 200,
    "unit": "rad/s"
  },
  "exec": {
    "request_id": "req-bode-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.bode",
    "version": "0.2.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-bode-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.IMPROPER_SYSTEM",
    "category": "domain",
    "retryable": true,
    "message": "系统不是 proper：分子阶次 2 > 分母阶次 1，频率响应在 ω→∞ 时发散，不存在状态空间实现。",
    "agent_hint": "该算子只接受 proper 系统。若是理想微分环节，请补充远高于关心频带的极点使其 proper；若输入有误，检查 num/den 是否颠倒。",
    "details": {
      "path": "/system",
      "num_degree": 2,
      "den_degree": 1
    },
    "remediation": [
      {
        "id": "add_filter_pole",
        "kind": "modify_input",
        "description": "在分母乘上 (τs+1)^1，τ = 1e-3 s（远高于关心频带时影响可忽略）",
        "confidence": "medium",
        "requires_user_confirmation": true,
        "patch": [
          {
            "op": "replace",
            "path": "/system/den",
            "value": [
              0.001,
              1.001,
              1.0
            ]
          }
        ]
      },
      {
        "id": "swap_num_den",
        "kind": "modify_input",
        "description": "若 num/den 写反，交换二者",
        "confidence": "low",
        "requires_user_confirmation": true,
        "patch": [
          {
            "op": "replace",
            "path": "/system/num",
            "value": [
              1.0,
              1.0
            ]
          },
          {
            "op": "replace",
            "path": "/system/den",
            "value": [
              1.0,
              0.0,
              0.0
            ]
          }
        ]
      },
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认系统模型是否为理想微分环节或输入有误",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.10051399999999999
  }
}
```

**非对称状态空间示例（内存序回归用例）** — `request.ss_nonsymmetric.json`，A 为伴随形 [[0,1,0],[0,0,1],[−6,−11,−6]]（row-major 展平为 `[0,1,0, 0,0,1, -6,-11,-6]`），响应见 `response.ss_nonsymmetric.json`。

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [
        3,
        3
      ],
      "data": [
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        1.0,
        -6.0,
        -11.0,
        -6.0
      ]
    },
    "B": {
      "dtype": "f64",
      "shape": [
        3,
        2
      ],
      "data": [
        0.0,
        1.0,
        0.0,
        0.0,
        1.0,
        2.0
      ]
    },
    "C": {
      "dtype": "f64",
      "shape": [
        2,
        3
      ],
      "data": [
        1.0,
        0.0,
        0.0,
        0.0,
        1.0,
        1.0
      ]
    },
    "D": {
      "dtype": "f64",
      "shape": [
        2,
        2
      ],
      "data": [
        0.0,
        0.0,
        0.0,
        0.0
      ]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "frequency": {
    "mode": "explicit",
    "values": [
      0.01,
      0.01373823795883263,
      0.018873918221350976,
      0.02592943797404667,
      0.035622478902624426,
      0.04893900918477494,
      0.06723357536499334,
      0.09236708571873861,
      0.12689610031679222,
      0.17433288221999882,
      0.2395026619987486,
      0.32903445623126676,
      0.4520353656360243,
      0.6210169418915616,
      0.8531678524172805,
      1.1721022975334805,
      1.6102620275609392,
      2.212216291070448,
      3.039195382313198,
      4.175318936560401,
      5.736152510448681,
      7.880462815669913,
      10.82636733874054,
      14.873521072935118,
      20.433597178569418,
      28.072162039411758,
      38.56620421163472,
      52.98316906283707,
      72.78953843983146,
      100.0
    ],
    "unit": "rad/s"
  },
  "exec": {
    "request_id": "req-bode-003-nonsym"
  }
}
```

**Parity 测试规格** — `examples/control.lti.bode/parity.yaml`

```yaml
# Parity test spec — control.lti.bode 0.2.0 (AEOI v0.2-draft)
operator: control.lti.bode
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol.py, golden_store: "golden/python-control/control.lti.bode/0.2.0/*.json",
     note: "NOT MATLAB output; 当前 cargo test 实际比对的对象"}
  - {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth, status: planned,
     generator: matlab/gen_golden_bode.m, golden_store: "golden/MATLAB/control.lti.bode/0.2.0/*.json",
     note: "尚未运行（box 无 MATLAB）"}
runner: "cargo test --release -p aeoi-control --test parity"
tolerance_class: T3
compare:
  w:         {mode: rel, rtol: 1.0e-14}
  magnitude: {quantity: abs_db, atol: 1.0e-9, scaling: "max(atol, 10·trust.error_estimate.magnitude_db_abs)"}
  phase:     {mode: modulo_360_then_abs, atol_deg: 1.0e-8, scaling: "max(atol, 10·trust.error_estimate.phase_deg_abs)"}
  layout:    "[ny, nu, nw] row-major"
case_families:   # n = 当前 golden 用例数
  - {name: siso_2nd_order_sweep, n: 40}
  - {name: siso_tf_higher_order, n: 15}
  - {name: mimo_ss_random_stable, n: 20}
  - {name: zpk, n: 10}
  - {name: discrete_tf, n: 10}
  - {name: near_imag_axis_poles, n: 3, expect_warning: W.NUMERICAL.NEAR_POLE, note: "容差按算子报告的误差估计放宽"}
  - {name: improper_reject, n: 3, expect: E.DOMAIN.IMPROPER_SYSTEM}
  - {name: hz_unit, n: 1, note: "输入 Hz，输出 w 为 rad/s"}
  - {name: memory_order_nonsymmetric, n: 1,
     note: "非对称 A；同时断言 column-major 展平（A(:) 错误）的请求结果与参考值偏差 > 1e3×容差"}
pass_rule: "所有 case 在容差内；预期报错的 case 报出相同 code；预期 warning 必须出现；所有 code 必须在 manifest 中声明"
```

### 10.2 `control.lti.step`

语义要点：
- 对 LTI 系统，用 ZOH 精确离散化：增广矩阵 [[A, B],[0, 0]]·dt 的矩阵指数（Van Loan 1978；expm 用 Higham 2005 的缩放平方 + Padé 13）。阶跃输入在 ZOH 下没有离散化误差，因此归入 T2。
- 网格：`uniform` 时点数 N = floor(t_final/dt + 1e-9) + 1，t_k = k·dt。离散系统 MUST 满足 dt = ts（否则 `E.VALIDATION.GRID`）。N × nu > 10⁷ 返回 `E.RESOURCE.COMPUTE_LIMIT`（remediation：把 dt 放大到满足上限）；返回值个数 N × ny × nu > 10⁶ 返回 `E.RESOURCE.OUTPUT_TOO_LARGE`（remediation：抽样返回或只返回指标）。`return_samples: "decimated"` 时取下标 round(k(N−1)/(M−1))。
- **阶跃指标在网格上定义，不插值（v0.2 明确）**：上升时间 = 首个 y ≥ hi·y∞ 的网格时刻 − 首个 y ≥ lo·y∞ 的网格时刻；调节时间 = 最后一个 |y − y∞| ≥ thr·|y∞| 的网格点之后一个网格点的时刻；峰值取网格最大 |y|。定义与 python-control 0.10 `step_info` 一致，相对连续时间真值的误差 ≤ dt。y∞ 取 DC 增益。不稳定系统与零 DC 增益通道的 info 为 `null`（分别附 `W.DOMAIN.UNSTABLE_SYSTEM`、`W.DOMAIN.ZERO_DC_GAIN`）。MIMO 时 info 为 `[ny][nu]` 数组。
- trust：终点一致性（逐步递推的 x(t_end) 与一次性 expm 直接计算的 x(t_end) 的相对偏差）作为后验残差。

**Manifest**

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.step",
  "version": "0.2.0",
  "title": "Step response",
  "description": "计算 LTI 系统在均匀时间网格上的单位阶跃响应（每个输入通道分别施加单位阶跃），并给出阶跃特性指标。",
  "category": "analysis",
  "tags": [
    "time-domain",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "system 为 proper",
      "uniform 网格：t_k = k·dt，k = 0..N−1，N = floor(t_final/dt + 1e-9) + 1",
      "离散系统要求 dt = ts（相对误差 1e-9）",
      "step_info 需系统渐近稳定，否则为 null 并给出 warning"
    ],
    "not_for": [
      "非线性系统",
      "任意输入（将来用 control.lti.lsim）",
      "带时滞的系统（v0.2 不支持）"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "连续系统：对增广矩阵 [[A,B],[0,0]]·dt 求矩阵指数（Padé 13 阶 + scaling & squaring）得 Φ、Γ，再递推 x_{k+1}=Φx_k+Γu、y_k=Cx_k+Du；阶跃输入在 ZOH 下无离散化误差。离散系统直接递推。tf/zpk 先转为可控标准型实现。指标按网格点定义，不插值（与 python-control step_info 相同）",
    "references": [
      {
        "type": "paper",
        "cite": "C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978",
        "doi": "10.1109/TAC.1978.1101743"
      },
      {
        "type": "paper",
        "cite": "N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005",
        "doi": "10.1137/04061101X"
      },
      {
        "type": "textbook",
        "cite": "K. Ogata, Modern Control Engineering, 5th ed., ch.5（阶跃指标定义）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "step::simulate (expm Padé-13 + recursion)",
    "version": "0.2.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": [
      "json",
      "b64",
      "ref"
    ],
    "output": [
      "json",
      "b64"
    ]
  },
  "limits": {
    "max_grid_points": 10000000,
    "max_output_values": 1000000
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.step/0.2.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC",
    "E.VALIDATION.GRID",
    "E.VALIDATION.RANGE",
    "E.RESOURCE.COMPUTE_LIMIT",
    "E.RESOURCE.OUTPUT_TOO_LARGE"
  ],
  "warnings": [
    "W.DOMAIN.UNSTABLE_SYSTEM",
    "W.DOMAIN.NOT_SETTLED",
    "W.DOMAIN.INFINITE_DC_GAIN",
    "W.DOMAIN.ZERO_DC_GAIN",
    "W.ACCURACY.GRID_TOO_COARSE",
    "W.ACCURACY.AUTO_GRID",
    "W.VALIDATION.LEADING_ZEROS_STRIPPED",
    "W.PERFORMANCE.SPARSE_DENSIFIED"
  ],
  "mcp": {
    "tool_name": "control_lti_step",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:step:zoh-expm-pade13-recursion",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "zoh_exact",
        "statement": "阶跃输入在 ZOH 下无离散化误差（只有 expm 与递推舍入）",
        "enforced_by": "validation"
      },
      {
        "id": "grid",
        "statement": "时间网格足以分辨动态",
        "enforced_by": "warning:W.ACCURACY.GRID_TOO_COARSE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": [
      "all_finite"
    ]
  }
}
```

**Input schema**

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.lti.step/0.2.0/input",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "system",
    "time"
  ],
  "properties": {
    "system": {
      "$ref": "aeoi://schemas/common/0.4/LtiSystem",
      "description": "被分析的 LTI 系统：tf（num/den 按降幂）、ss（A/B/C/D 稠密矩阵，row-major）或 zpk；ts=0 连续，ts>0 为离散采样周期（s）。上一个工具返回了 result.system 时，把那个 JSON 对象原样放在这里，不要手抄或改写系数（spec K-6）"
    },
    "time": {
      "oneOf": [
        {
          "type": "object",
          "additionalProperties": false,
          "required": [
            "mode",
            "t_final",
            "dt",
            "unit"
          ],
          "properties": {
            "mode": {
              "const": "uniform"
            },
            "t_final": {
              "type": "number",
              "exclusiveMinimum": 0
            },
            "dt": {
              "type": "number",
              "exclusiveMinimum": 0
            },
            "unit": {
              "const": "s"
            }
          }
        },
        {
          "type": "object",
          "additionalProperties": false,
          "required": [
            "mode",
            "unit"
          ],
          "properties": {
            "mode": {
              "const": "auto"
            },
            "t_final": {
              "type": [
                "number",
                "null"
              ],
              "exclusiveMinimum": 0
            },
            "unit": {
              "const": "s"
            }
          },
          "description": "启发式网格（见 manifest），不参与 parity 测试"
        }
      ],
      "description": "时间网格：uniform（t_final、dt，单位 s）或 auto（由算子按系统时间常数选择）；离散系统要求 dt = ts"
    },
    "step_info": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "enabled": {
          "type": "boolean",
          "default": true
        },
        "rise_time_limits": {
          "type": "array",
          "items": {
            "type": "number",
            "minimum": 0,
            "maximum": 1
          },
          "minItems": 2,
          "maxItems": 2,
          "default": [
            0.1,
            0.9
          ]
        },
        "settling_threshold": {
          "type": "number",
          "exclusiveMinimum": 0,
          "maximum": 0.5,
          "default": 0.02
        }
      },
      "description": "阶跃指标（上升时间、调节时间、超调等）；enabled=true 时计算"
    },
    "output": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "return_samples": {
          "enum": [
            "all",
            "decimated",
            "none"
          ],
          "default": "all"
        },
        "max_samples": {
          "type": "integer",
          "minimum": 2,
          "default": 10001
        }
      },
      "description": "返回的采样点：all / decimated（最多 max_samples 点）/ none"
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.4/ExecOptions"
    }
  }
}
```

**请求**

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4
    ],
    "den": [
      1,
      1.2,
      4
    ],
    "ts": 0,
    "time_unit": "s"
  },
  "time": {
    "mode": "uniform",
    "t_final": 10,
    "dt": 0.01,
    "unit": "s"
  },
  "step_info": {
    "enabled": true,
    "rise_time_limits": [
      0.1,
      0.9
    ],
    "settling_threshold": 0.02
  },
  "output": {
    "return_samples": "decimated",
    "max_samples": 11
  },
  "exec": {
    "request_id": "req-step-001"
  }
}
```

**成功响应**（1001 点网格计算后抽样为 11 点返回；指标按 dt = 0.01 s 网格定义：上升时间 0.66 s、调节时间 5.62 s、超调 37.2318%、峰值时刻 1.65 s。v0.1 示例中的 0.5815 s / 5.698 s 来自 python-control 的自动网格，与请求的网格不符，是 v0.1 的错误）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.step",
    "version": "0.2.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-step-001",
  "result": {
    "t": {
      "unit": "s",
      "value": {
        "dtype": "f64",
        "shape": [
          11
        ],
        "data": [
          0.0,
          1.0,
          2.0,
          3.0,
          4.0,
          5.0,
          6.0,
          7.0,
          8.0,
          9.0,
          10.0
        ]
      }
    },
    "y": {
      "unit": "1",
      "value": {
        "dtype": "f64",
        "shape": [
          1,
          1,
          11
        ],
        "data": [
          0.0,
          1.0186307301607034,
          1.2944308432181657,
          0.8875035987716836,
          0.9521578255428503,
          1.05125103644682,
          0.9958045219089461,
          0.9860865361641766,
          1.0063145500010093,
          1.0018983329268827,
          0.9974089570889851
        ]
      }
    },
    "n_grid_points": 1001,
    "info": {
      "rise_time": {
        "unit": "s",
        "value": 0.66
      },
      "settling_time": {
        "unit": "s",
        "value": 5.62
      },
      "settling_min": {
        "unit": "1",
        "value": 0.8613762654882545
      },
      "settling_max": {
        "unit": "1",
        "value": 1.3723177192599603
      },
      "overshoot": {
        "unit": "%",
        "value": 37.23177192599603
      },
      "undershoot": {
        "unit": "%",
        "value": 0.0
      },
      "peak": {
        "unit": "1",
        "value": 1.3723177192599603
      },
      "peak_time": {
        "unit": "s",
        "value": 1.6500000000000001
      },
      "steady_state": {
        "unit": "1",
        "value": 1.0,
        "source": "dcgain"
      }
    }
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "final_state_rel_discrepancy": 3.9471295721541e-16,
      "definition": "‖x_{N−1}(逐步递推) − x(t_end)(一次性 expm 直接计算)‖₂ / ‖x(t_end)‖₂，取所有输入通道最大值"
    },
    "condition": {
      "norm1_A_dt": 0.04,
      "expm_squarings": 0,
      "realization": "controllable canonical form (scipy tf2ss convention)",
      "max_real_pole": -0.6
    },
    "error_estimate": {
      "y_abs": 1.525102307752513e-13,
      "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。非严格上界",
      "info_time_metrics_abs_s": 0.01,
      "info_basis": "指标按网格点定义（首个越过阈值的网格点，不插值），相对连续时间真值误差 ≤ dt"
    },
    "checks": {
      "asymptotically_stable": true,
      "all_finite": true,
      "auto_grid": false
    },
    "accuracy_grade": "B",
    "credentials": null,
    "derived": {
      "dcgain": {
        "dtype": "f64",
        "shape": [
          1,
          1
        ],
        "data": [
          1.0
        ]
      },
      "poles": {
        "dtype": "c128",
        "shape": [
          2
        ],
        "data": [
          [
            -0.6,
            1.9078784028338913
          ],
          [
            -0.6,
            -1.9078784028338913
          ]
        ]
      },
      "dt": 0.01,
      "t_end": 10.0
    }
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:step:zoh-expm-pade13-recursion",
      "routine": "step::simulate (expm Padé-13 + recursion)",
      "provenance": "aeoi://operators/control.lti.step/0.2.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "zoh_exact",
        "statement": "阶跃输入在 ZOH 下无离散化误差（只有 expm 与递推舍入）",
        "enforced_by": "validation"
      },
      {
        "id": "grid",
        "statement": "时间网格足以分辨动态",
        "enforced_by": "warning:W.ACCURACY.GRID_TOO_COARSE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "y_abs": 1.525102307752513e-13
      },
      "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。非严格上界"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "all_finite": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:1fca11705fac9d822caa1ed505883af59785f8f2c5759fab7218aee60230d304",
      "result": "sha256:27ac3c8e6efa285daae90b614b0ebee458ef9937385e52eaff432aaf6ea24043",
      "output_hash": "sha256:f8bed5cefb591bf6b9870f25990c51d2a3760d8dc61a1c81bd656cdde39f1406",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.step@0.2.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:67b10a6f76e735d805f4c079ab1e7254602e568993403bfe5848070e6a2c1f3e",
    "output_hash": "sha256:f8bed5cefb591bf6b9870f25990c51d2a3760d8dc61a1c81bd656cdde39f1406",
    "platform": "x86_64-linux",
    "cpu_features": [
      "avx",
      "avx2",
      "fma",
      "avx512f"
    ],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.24802800000000003
  }
}
```

**错误请求与响应**（dt = 1e-7 s，计算量超限）

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4
    ],
    "den": [
      1,
      1.2,
      4
    ],
    "ts": 0,
    "time_unit": "s"
  },
  "time": {
    "mode": "uniform",
    "t_final": 10,
    "dt": 1e-07,
    "unit": "s"
  },
  "exec": {
    "request_id": "req-step-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.step",
    "version": "0.2.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-step-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.RESOURCE.COMPUTE_LIMIT",
    "category": "resource",
    "retryable": true,
    "message": "时间网格 100000001 点 × 1 个输入通道 = 100000001 步，超过单次计算上限 10000000。",
    "agent_hint": "增大 dt 或缩短 t_final。抽样返回（output.return_samples）不能解决此问题，因为递推仍需走完每个网格点。",
    "details": {
      "path": "/time/dt",
      "grid_points": 100000001,
      "limit": 10000000,
      "fastest_pole_rad_s": 2.0
    },
    "remediation": [
      {
        "id": "coarser_grid",
        "kind": "modify_input",
        "description": "dt 改为 0.05 s（≈ 0.1/最快极点模，且满足上限）",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/time/dt",
            "value": 0.05
          }
        ]
      },
      {
        "id": "shorten",
        "kind": "ask_user",
        "description": "与用户确认是否可以缩短 t_final",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.099568
  }
}
```

**Parity 测试规格**

```yaml
# Parity test spec — control.lti.step 0.2.0 (AEOI v0.2-draft)
operator: control.lti.step
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol.py, golden_store: "golden/python-control/control.lti.step/0.2.0/*.json",
     note: "NOT MATLAB output; python 侧用 step_response(sys, T=显式网格) + step_info(T 同网格)"}
  - {engine: MATLAB, role: ground_truth, status: planned, generator: "matlab/gen_golden_step.m (待写)",
     golden_store: "golden/MATLAB/control.lti.step/0.2.0/*.json",
     note: "MATLAB stepinfo 会在网格间插值，与 AEOI v0.2 的网格定义不同；比较 info 前需用相同定义"}
runner: "cargo test --release -p aeoi-control --test parity"
tolerance_class: T2
compare:
  t: {atol: 1.0e-12}
  y: {rtol: 1.0e-10, scale: "max(1, max|y_ref|)"}
  info.rise_time / settling_time / peak_time: {atol: "dt"}
  info.peak / settling_min / settling_max / steady_state: {rtol: 1.0e-9}
  info.overshoot / undershoot: {atol_pct: 1.0e-6}
  info: {definition: "网格上定义，无插值（v0.2 §5.2）；零 DC 增益通道 info = null（W.DOMAIN.ZERO_DC_GAIN）"}
case_families:
  - {name: siso_underdamped, n: 25}
  - {name: siso_overdamped_and_critically_damped, n: 15}
  - {name: siso_with_zeros, n: 10}
  - {name: siso_with_zeros_nonminimum_phase, n: 5}
  - {name: mimo_ss_random_stable, n: 15}
  - {name: stiff_spread_poles, n: 3, note: "极点跨 4–6 个数量级，输出 decimated 到 1001 点"}
  - {name: unstable_system, n: 5, expect_warning: W.DOMAIN.UNSTABLE_SYSTEM, note: "y 仍比较；info 期望为 null"}
  - {name: discrete_systems, n: 8, note: "dt 必须等于 ts"}
  - {name: memory_order_nonsymmetric, n: 1}
  - {name: compute_limit, n: 1, expect: E.RESOURCE.COMPUTE_LIMIT}
  - {name: output_too_large, n: 1, expect: E.RESOURCE.OUTPUT_TOO_LARGE}
  - {name: improper_reject, n: 1, expect: E.DOMAIN.IMPROPER_SYSTEM}
  - {name: discrete_dt_mismatch, n: 1, expect: E.VALIDATION.GRID}
excluded: ["auto 时间网格（参考实现的自动网格不公开且会变化，不作为 parity 对象）"]
```

### 10.3 `control.design.lqr`

算法与来源：nalgebra 没有有序实 Schur 分解，因此 v0.2 不采用 v0.1 计划的 Schur 方法（Laub 1979 / SLICOT `SB02OD`），改用：
1. 有交叉项 N 时先消去：Ā = A − BR⁻¹Nᵀ，Q̄ = Q − NR⁻¹Nᵀ；
2. 前置检查：R 的 Cholesky（`E.DOMAIN.R_NOT_POSITIVE_DEFINITE`）、[[Q,N],[Nᵀ,R]] 半正定（`E.DOMAIN.WEIGHT_NOT_PSD`）、PBH 可镇定性检验 σ_min([A−λI, B]) / ‖[A,B]‖_F ≤ 1e-10 即 `E.DOMAIN.NOT_STABILIZABLE`（< 1e-6 时 `W.NUMERICAL.NEAR_UNCONTROLLABLE`）、Hamiltonian 虚轴特征值检验 min|Re λ(H)| ≤ 10³·u·‖H‖₁ 即 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`；
3. 矩阵符号函数 Newton 迭代 Z ← (Z + Z⁻¹)/2，前期用行列式缩放（Byers 1987；Kenney–Laub 1995），收敛判据 ‖Z_{k+1}−Z_k‖₁/‖Z_{k+1}‖₁ ≤ 1e-13 或停滞；
4. 由 [sign(H) + I] 的列空间方程用 SVD 最小二乘解出 S（Roberts 1980），对称化；
5. n ≤ 32 时至多 3 步 Newton–Kleinman 修正（Kleinman 1968；Lyapunov 方程用 Kronecker 形式直接解），残差不降即停；
6. trust 报告 CARE 残差（绝对、相对、按各项范数缩放的相对残差）、cond(R)、cond(S)、PBH 裕度、基于 sep 的一阶后验误差估计（n ≤ 20）。

**0.3.0 的改动**（v0.3.2；起因是 tpt-control 交叉验证 `reports/parity-tpt-control.md`：cond(R) = 1e12 时 0.2.0 前向误差 3e-5，且 7/10 个合法用例误报虚轴特征值）：
- 第 2 步的虚轴检验改为在辛对角平衡后的 H̃ = diag(D⁻¹, D)·H·diag(D, D⁻¹) 上做（D = diag(2^k)，坐标下降最小化 H̃ 的逐元 ℓ₁ 范数；等价于状态尺度变换 x = Dx̃，在二进制下精确）。min|Re λ(H̃)| ≤ 10³·u·‖H̃‖₁ 只是怀疑：对 Ā 在虚轴附近（|Re λ| ≤ 1e-6·‖Ā‖_F）的特征值做 PBH 可观性检验 σ_min([Ā−λI; Q̄]) / ‖[Ā; Q̄]‖_F ≤ 1e-10 → `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`（基本原因写入 `details.basis`）；未确认时照常求解（`W.NUMERICAL.ILL_CONDITIONED`），符号函数失败或求得的闭环没有稳定裕度（max Re λ(A−BK) ≥ −10³·u·‖A−BK‖₁）时才报该错误码。符号函数本身仍在未平衡的 H 上迭代（在 100 维、3 输入的极病态用例上，平衡后的迭代丢失了镇定性）。
- 第 5 步改为 K 形式 Newton–Kleinman 缺陷校正，n ≤ 200（`limits.max_states_for_refinement`），至多 20 步（`limits.max_newton_iterations`）：K = R⁻¹(BᵀX + Nᵀ)（Cholesky，再用补偿残差 P − RK 做至多 4 步迭代精化）；残差 E = (A−BK)ᵀX + X(A−BK) + Q − NK − KᵀNᵀ + KᵀRK 用补偿点积（Ogita–Rump–Oishi Dot2，Dekker 无 FMA 乘法，各平台逐位相同）计算——E 对 K 驻定，K 的误差只以二次项进入；ΔX 解 (A−BK)ᵀΔX + ΔX(A−BK) + E = 0（复 Schur Bartels–Stewart，替代 O(n⁶) 的 Kronecker 形式），X ← X + ΔX。‖ΔX‖_F ≤ 2ε‖X‖_F，或 ‖ΔX‖ 在 1e-8‖X‖_F 以下不再减小（该步不施加）即停；修正后 A−BK 必须 Hurwitz，否则退回符号函数解。
- 输出的 K 由 S 加上"再一步未施加的 Newton 修正量"（S 在 f64 舍入之外的低位部分）求得：cond(R) = 1e12 时仅由 S 的 f64 舍入引起的 K 误差就可达 cond(R)·u。
- trust 新增 `convergence.care_residual_compensated_fro`、`newton_kleinman_final_change_rel`、`symplectic_balancing_log2_range`、`condition.hamiltonian_balanced`；`error_estimate` 用补偿残差 / sep 加 u‖S‖_F。
- 结果（50 位 mpmath 真值，`golden/mpmath/control.design.lqr/0.3.0`，16 例，cond(R) 1e6–1e12）：S、K 全部与真值的 f64 舍入一致（相对误差 0 到 4.5e-16）；0.2.0 在同一批用例上 S 最大误差 2.3e-4，且 4 例报错。复现例 A = diag(−0.1, −1)、B = diag(0.1, 1)、Q = I、R = diag(1, 1e-12) 现在返回 S = diag(4.14213562373095, 9.999990000004999e-7)、K = diag(0.414213562373095, 999999.0000005)，与 50 位真值的 f64 舍入一致（见 `examples/control.design.lqr/request.ill_conditioned_R.json`）。

**Manifest**

```json
{
  "spec": "aeoi/0.4",
  "id": "control.design.lqr",
  "version": "0.3.0",
  "title": "Continuous-time LQR",
  "description": "求解连续时间代数 Riccati 方程 (CARE)，返回最优状态反馈增益 K（u=−Kx）、Riccati 解 S 与闭环极点，使 J=∫(xᵀQx+uᵀRu+2xᵀNu)dt 最小。",
  "category": "design",
  "tags": [
    "optimal-control",
    "riccati",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "R 对称正定",
      "Q 对称；[[Q,N],[Nᵀ,R]] 对称半正定",
      "(A,B) 可镇定",
      "Hamiltonian 矩阵在虚轴上无特征值（等价于 (Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在虚轴上无不可观模态）"
    ],
    "not_for": [
      "离散系统（将来用 control.design.dlqr）",
      "有约束的 MPC"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "消去交叉项后构造 Hamiltonian 矩阵 H=[[Ā,−BR⁻¹Bᵀ],[−Q̄,−Āᵀ]]。虚轴检验在辛对角平衡后的 H̃=D⁻¹HD 上做（D=diag(d,d⁻¹)，d_i 为 2 的幂，坐标下降最小化逐元 ℓ₁ 范数，等价于状态尺度变换）：min|Re λ(H̃)| ≤ 10³·u·‖H̃‖₁ 只作为怀疑，用 PBH 检验 (Q̄,Ā) 在近虚轴特征值处是否不可观来确认，否则照常求解并在事后检验闭环稳定裕度；在 H 上用带行列式缩放的 Newton 迭代计算 sign(H)，由 (sign(H)+I) 的零空间最小二乘解出 S；随后在原坐标做 K 形式 Newton–Kleinman 修正（缺陷校正形式）：K=R⁻¹(BᵀX+Nᵀ)（Cholesky + 补偿残差迭代精化），残差 E=(A−BK)ᵀX+X(A−BK)+Q−NK−KᵀNᵀ+KᵀRK 用补偿点积（Dot2）计算，ΔX 解 (A−BK)ᵀΔX+ΔX(A−BK)+E=0（复 Schur Bartels–Stewart），X←X+ΔX；‖ΔX‖_F ≤ 2ε‖X‖_F 或在 1e-8‖X‖_F 以下不再减小即停，至多 20 步，结果须保持 A−BK Hurwitz；最终 K 由 S 加一步未施加的 Newton 修正量（S 的低位部分）精化求得",
    "references": [
      {
        "type": "paper",
        "cite": "J. D. Roberts, Linear model reduction and solution of the algebraic Riccati equation by use of the sign function, Int. J. Control 32(4), 1980",
        "doi": "10.1080/00207178008922881"
      },
      {
        "type": "paper",
        "cite": "R. Byers, Solving the algebraic Riccati equation with the matrix sign function, Linear Algebra Appl. 85, 1987",
        "doi": "10.1016/0024-3795(87)90222-9"
      },
      {
        "type": "paper",
        "cite": "C. S. Kenney, A. J. Laub, The matrix sign function, IEEE TAC 40(8), 1995",
        "doi": "10.1109/9.402226"
      },
      {
        "type": "paper",
        "cite": "D. L. Kleinman, On an iterative technique for Riccati equation computations, IEEE TAC 13(1), 1968",
        "doi": "10.1109/TAC.1968.1098829"
      },
      {
        "type": "paper",
        "cite": "P. Benner, Symplectic balancing of Hamiltonian matrices, SIAM J. Sci. Comput. 22(5), 2001",
        "doi": "10.1137/S1064827500367993"
      },
      {
        "type": "paper",
        "cite": "R. H. Bartels, G. W. Stewart, Solution of the matrix equation AX + XB = C, Comm. ACM 15(9), 1972",
        "doi": "10.1145/361573.361582"
      },
      {
        "type": "textbook",
        "cite": "B. D. O. Anderson, J. B. Moore, Optimal Control: Linear Quadratic Methods, 1990"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "lqr::solve_care (matrix sign function + K-form Newton–Kleinman defect correction; balanced imaginary-axis test)",
    "version": "0.2.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/Q": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/R": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/N": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": [
      "json",
      "b64",
      "ref"
    ],
    "output": [
      "json",
      "b64"
    ]
  },
  "limits": {
    "max_states": 200,
    "max_states_for_refinement": 200,
    "max_sign_iterations": 100,
    "max_newton_iterations": 20
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.design.lqr/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.NOT_SYMMETRIC",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.DOMAIN.R_NOT_POSITIVE_DEFINITE",
    "E.DOMAIN.WEIGHT_NOT_PSD",
    "E.DOMAIN.NOT_STABILIZABLE",
    "E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG",
    "E.NUMERICAL.NO_CONVERGENCE",
    "E.NUMERICAL.SINGULAR",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC"
  ],
  "warnings": [
    "W.NUMERICAL.LARGE_RESIDUAL",
    "W.NUMERICAL.NEAR_UNCONTROLLABLE",
    "W.NUMERICAL.ILL_CONDITIONED",
    "W.VALIDATION.SYMMETRIZED",
    "W.PERFORMANCE.SPARSE_DENSIFIED"
  ],
  "mcp": {
    "tool_name": "control_design_lqr",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:lqr:sign-function+kform-newton-kleinman-dot2",
    "assumptions": [
      {
        "id": "weights",
        "statement": "Q 对称半正定、R 对称正定、[[Q,N],[Nᵀ,R]] ⪰ 0",
        "enforced_by": "error:E.DOMAIN.WEIGHT_NOT_PSD"
      },
      {
        "id": "R_pd",
        "statement": "R 正定",
        "enforced_by": "error:E.DOMAIN.R_NOT_POSITIVE_DEFINITE"
      },
      {
        "id": "stabilizable",
        "statement": "(A,B) 可镇定",
        "enforced_by": "error:E.DOMAIN.NOT_STABILIZABLE"
      },
      {
        "id": "no_imag_axis_unobservable",
        "statement": "(Q̄,Ā) 在虚轴上没有不可观模态",
        "enforced_by": "error:E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG"
      },
      {
        "id": "model",
        "statement": "A、B、Q、R、N 准确描述设计问题（单位一致、row-major）",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": [
      "S_psd",
      "closed_loop_stable"
    ]
  }
}
```

**Input schema**

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.design.lqr/0.3.0/input",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "A",
    "B",
    "Q",
    "R"
  ],
  "properties": {
    "A": {
      "$ref": "aeoi://schemas/common/0.4/Matrix",
      "description": "状态矩阵 A，[n,n]"
    },
    "B": {
      "$ref": "aeoi://schemas/common/0.4/Matrix",
      "description": "输入矩阵 B，[n,m]（单输入时为列向量 [n,1]）"
    },
    "Q": {
      "$ref": "aeoi://schemas/common/0.4/Matrix",
      "description": "状态权重 Q，[n,n]，对称半正定"
    },
    "R": {
      "$ref": "aeoi://schemas/common/0.4/Matrix",
      "description": "输入权重 R，[m,m]，对称正定（标量写成 [1,1]）"
    },
    "N": {
      "$ref": "aeoi://schemas/common/0.4/Matrix",
      "description": "交叉权重 N，[n,m]，缺省为 0"
    },
    "symmetry_tolerance": {
      "type": "number",
      "exclusiveMinimum": 0,
      "default": 1e-10,
      "description": "相对容差 ‖X−Xᵀ‖_F ≤ tol·‖X‖_F；超出即 E.VALIDATION.NOT_SYMMETRIC，范围内（但非精确对称）对称化并给 W.VALIDATION.SYMMETRIZED"
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.4/ExecOptions"
    }
  },
  "x-aeoi-shape-constraints": [
    "A: [n,n]",
    "B: [n,m]",
    "Q: [n,n]",
    "R: [m,m]",
    "N: [n,m]，缺省为 0"
  ]
}
```

**请求**

```json
{
  "A": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      0,
      1,
      0,
      0
    ]
  },
  "B": {
    "dtype": "f64",
    "shape": [
      2,
      1
    ],
    "data": [
      0,
      1
    ]
  },
  "Q": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1,
      0,
      0,
      1
    ]
  },
  "R": {
    "dtype": "f64",
    "shape": [
      1,
      1
    ],
    "data": [
      1
    ]
  },
  "exec": {
    "request_id": "req-lqr-001"
  }
}
```

**成功响应**（解析解 K = [1, √3]，S = [[√3,1],[1,√3]]，闭环极点 −√3/2 ± 0.5j；实现结果与解析解的相对误差为 0）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.design.lqr",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-lqr-001",
  "result": {
    "K": {
      "dtype": "f64",
      "shape": [
        1,
        2
      ],
      "data": [
        1.0,
        1.7320508075688772
      ]
    },
    "S": {
      "dtype": "f64",
      "shape": [
        2,
        2
      ],
      "data": [
        1.7320508075688772,
        1.0,
        1.0,
        1.7320508075688772
      ]
    },
    "closed_loop_poles": {
      "dtype": "c128",
      "shape": [
        2
      ],
      "data": [
        [
          -0.8660254037844386,
          0.5000000000000001
        ],
        [
          -0.8660254037844386,
          -0.5000000000000001
        ]
      ]
    }
  },
  "trust": {
    "convergence": {
      "applicable": true,
      "converged": true,
      "iterations": 3,
      "criterion": "‖Z_{k+1}−Z_k‖₁/‖Z_{k+1}‖₁ ≤ 1e-13 或停滞",
      "final_value": 0.0,
      "newton_kleinman_steps": 2,
      "care_residual_compensated_fro": 3.476255146080469e-16,
      "newton_kleinman_final_change_rel": 5.01754211090345e-17,
      "sign_function_residual_fro": 2.4525628233156873e-15,
      "symplectic_balancing_log2_range": [
        0.0,
        0.0
      ]
    },
    "residual": {
      "care_abs_fro": 4.440892098500626e-16,
      "care_rel": 1.5700924586837752e-16,
      "care_rel_scaled": 2.9466340102353637e-17,
      "S_asymmetry_before_symmetrization_rel": 0.0,
      "definition": "care_rel = ‖ĀᵀS+SĀ−SGS+Q̄‖_F/‖S‖_F；care_rel_scaled 分母为 ‖Q̄‖_F+2‖Ā‖_F‖S‖_F+‖G‖_F‖S‖_F²（Ā=A−BR⁻¹Nᵀ, G=BR⁻¹Bᵀ, Q̄=Q−NR⁻¹Nᵀ）"
    },
    "condition": {
      "cond_R": 1.0,
      "cond_S": 3.7320508075688807,
      "pbh_min_sv_rel_unstable_modes": 0.7071067811865475,
      "pbh_min_sv_rel_all_modes": 0.7071067811865475,
      "sep_closed_loop_lyapunov": 0.647363319261565,
      "hamiltonian_min_abs_real_rel": 0.4330127018922313,
      "hamiltonian_balanced": true
    },
    "error_estimate": {
      "S_abs_fro": 8.510052258701883e-16,
      "S_rel": 3.0087578301899986e-16,
      "basis": "一阶后验估计：Newton 修正量 ‖δS‖_F ≈ ‖Res‖_F / σ_min(I⊗A_clᵀ + A_clᵀ⊗I) + u‖S‖_F（Res 在 Newton–Kleinman 修正后用补偿算术计算，否则为 f64 残差），忽略二次项；非严格上界"
    },
    "checks": {
      "S_symmetric": true,
      "S_psd": true,
      "closed_loop_stable": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:lqr:sign-function+kform-newton-kleinman-dot2",
      "routine": "lqr::solve_care (matrix sign function + K-form Newton–Kleinman defect correction; balanced imaginary-axis test)",
      "provenance": "aeoi://operators/control.design.lqr/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "weights",
        "statement": "Q 对称半正定、R 对称正定、[[Q,N],[Nᵀ,R]] ⪰ 0",
        "enforced_by": "error:E.DOMAIN.WEIGHT_NOT_PSD"
      },
      {
        "id": "R_pd",
        "statement": "R 正定",
        "enforced_by": "error:E.DOMAIN.R_NOT_POSITIVE_DEFINITE"
      },
      {
        "id": "stabilizable",
        "statement": "(A,B) 可镇定",
        "enforced_by": "error:E.DOMAIN.NOT_STABILIZABLE"
      },
      {
        "id": "no_imag_axis_unobservable",
        "statement": "(Q̄,Ā) 在虚轴上没有不可观模态",
        "enforced_by": "error:E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG"
      },
      {
        "id": "model",
        "statement": "A、B、Q、R、N 准确描述设计问题（单位一致、row-major）",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "S_abs_fro": 8.510052258701883e-16,
        "S_rel": 3.0087578301899986e-16
      },
      "basis": "一阶后验估计：Newton 修正量 ‖δS‖_F ≈ ‖Res‖_F / σ_min(I⊗A_clᵀ + A_clᵀ⊗I) + u‖S‖_F（Res 在 Newton–Kleinman 修正后用补偿算术计算，否则为 f64 残差），忽略二次项；非严格上界"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "S_psd": true,
        "closed_loop_stable": true
      },
      "failed": [],
      "convergence": {
        "converged": true,
        "iterations": 3
      },
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:f5d755e5f4fa8d08602d61de1649eb6d4b4b981f4ccfa488b284b288fb116f7e",
      "result": "sha256:041eafec539bf66b3a4c4e731c00b830e93fc61772afbf57bba60a2b9c5ab66f",
      "output_hash": "sha256:7c9bd47ea584368793a27a5f39e426f17b2e9492a3d575db3b0a5cb75b217c21",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.design.lqr@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:0230fc20928bfd639f221a6da28f00765e5da8cc0ea097d17e460edc336876f3",
    "output_hash": "sha256:7c9bd47ea584368793a27a5f39e426f17b2e9492a3d575db3b0a5cb75b217c21",
    "platform": "x86_64-linux",
    "cpu_features": [
      "avx",
      "avx2",
      "fma",
      "avx512f"
    ],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.422746
  }
}
```

**错误请求与响应**（不可镇定：A = diag(1, −1)，B = [0;1]，不稳定模态 λ=1 不可控）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1,
      0,
      0,
      -1
    ]
  },
  "B": {
    "dtype": "f64",
    "shape": [
      2,
      1
    ],
    "data": [
      0,
      1
    ]
  },
  "Q": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1,
      0,
      0,
      1
    ]
  },
  "R": {
    "dtype": "f64",
    "shape": [
      1,
      1
    ],
    "data": [
      1
    ]
  },
  "exec": {
    "request_id": "req-lqr-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.design.lqr",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-lqr-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.NOT_STABILIZABLE",
    "category": "domain",
    "retryable": false,
    "message": "(A,B) 不可镇定：特征值 λ = 1.000000+0.000000j（Re ≥ 0）对应的模态不受输入影响，不存在使闭环稳定的状态反馈，CARE 无稳定化解。",
    "agent_hint": "这是模型结构问题，调整 Q/R 无效。需要为该模态增加执行器（修改 B），或确认 A 是否录入有误（含行列顺序：row-major）。",
    "details": {
      "path": "/B",
      "uncontrollable_eigenvalue": [
        1.0,
        0.0
      ],
      "pbh_min_singular_value_rel": 0.0,
      "pbh_test": "σ_min([A−λI, B]) / ‖[A, B]‖_F ≤ 1e-10"
    },
    "remediation": [
      {
        "id": "add_actuator",
        "kind": "modify_input",
        "description": "示例：为每个状态各加一路输入（B = I，R = I），仅用于确认问题出在执行器配置",
        "confidence": "low",
        "requires_user_confirmation": true,
        "patch": [
          {
            "op": "replace",
            "path": "/B",
            "value": {
              "dtype": "f64",
              "shape": [
                2,
                2
              ],
              "data": [
                1.0,
                0.0,
                0.0,
                1.0
              ]
            }
          },
          {
            "op": "replace",
            "path": "/R",
            "value": {
              "dtype": "f64",
              "shape": [
                2,
                2
              ],
              "data": [
                1.0,
                0.0,
                0.0,
                1.0
              ]
            }
          }
        ]
      },
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认 A 矩阵与执行器配置",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.241047
  }
}
```

**Parity 测试规格**

```yaml
# Parity test spec — control.design.lqr 0.3.0 (AEOI v0.3-draft); python-control golden store generated for 0.2.0 and reused unchanged (V-3a)
operator: control.design.lqr
references:
  - {engine: python-control, version: "0.10.2 → scipy.linalg.solve_continuous_are 1.18.1 (无 slycot)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol.py, golden_store: "golden/python-control/control.design.lqr/0.2.0/*.json",
     note: "NOT MATLAB output"}
  - {engine: mpmath, version: "50 digits", role: high_precision_reference, status: active, generator: gen_golden_mpmath.py,
     golden_store: "golden/mpmath/control.design.lqr/0.3.0/*.json", runner: "cargo test --release -p aeoi-control --test parity_hp",
     compare: "S, K rel_fro ≤ 1e-12（不放宽）；闭环极点 ≤ max(1e-10, 1e2·ε·‖A−BK‖_F)（特征值后向误差下限）",
     note: "tpt-control 交叉验证发现的病态族（cond(R) 至 1e12、误报 HAMILTONIAN_IMAG_AXIS_EIG 的复现例）；NOT MATLAB, NOT python-control"}
  - {engine: analytic, role: second_reference, note: "双积分器族、标量情形、cond(R)=1e12 解耦标量 CARE 有解析解（单元测试 analytic_b2.rs）"}
  - {engine: MATLAB, role: ground_truth, status: planned, generator: "matlab/gen_golden_lqr.m (待写)",
     golden_store: "golden/MATLAB/control.design.lqr/0.3.0/*.json"}
runner: "cargo test --release -p aeoi-control --test parity"
tolerance_class: T2
compare:
  S: {mode: normwise_rel_fro, rtol: "max(1e-10·max(1, cond_S/1e2), 10·trust.error_estimate.S_rel)"}
  K: {mode: normwise_rel_fro, rtol: "10 × S 的容差"}
  closed_loop_poles: {mode: matched_set, matching: greedy_nearest, rtol: "1e-8·max(1, cond_S/1e2)"}
  invariants: ["care_rel_scaled ≤ 1e-12", "S 对称半正定", "A−BK 稳定"]   # 不依赖参考实现，也必须通过
case_families:
  - {name: double_integrator_family, n: 5}
  - {name: random_stabilizable, n: 40, params: "n∈[2,10], m∈[1,3]"}
  - {name: with_cross_term_N, n: 10}
  - {name: ill_conditioned_R, n: 5, params: "cond(R)∈[1e6,1e9]",
     note: "0.2.0 时与 scipy 相对差 ~2e-6；0.3.0（K 形式 Newton–Kleinman 缺陷校正）≤ 4e-13，且与 50 位真值逐位一致（golden/mpmath ill_conditioned_R_hp）"}
  - {name: larger_n, n: 3, params: "n = 20/30/40"}
  - {name: memory_order_nonsymmetric, n: 1}
  - {name: not_stabilizable, n: 3, expect: E.DOMAIN.NOT_STABILIZABLE}
  - {name: R_indefinite, n: 2, expect: E.DOMAIN.R_NOT_POSITIVE_DEFINITE}
  - {name: Q_not_symmetric, n: 1, expect: E.VALIDATION.NOT_SYMMETRIC}
  - {name: hamiltonian_imag_axis, n: 1, expect: E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG}
  - {name: shape_mismatch, n: 1, expect: E.VALIDATION.SHAPE}
case_families_high_precision:   # golden/mpmath, n = cases
  - {name: condR_1e12_decoupled_repro, n: 1, note: "0.2.0 误报 E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG"}
  - {name: ill_conditioned_R_hp, n: 5, note: "与 python-control 族 ill_conditioned_R 同输入，50 位真值"}
  - {name: ill_conditioned_R_extreme, n: 8, params: "n∈[2,6], m∈[2,3], cond(R)∈{1e10,1e11,1e12}"}
  - {name: ill_conditioned_R_cross_N, n: 2, params: "cond(R)∈{1e9,1e11}，带交叉项 N"}
```

### 10.4 `control.lti.convert`（v0.3 新增）

tf ↔ ss ↔ zpk 互转。

语义要点：
- `to ∈ {tf, ss, zpk}`；`realization` 字段说明所用实现（R-6）。tf→ss 用可控标准型（scipy `tf2ss` 约定）；MIMO tf→ss 为逐元素块对角（非最小，`W.DOMAIN.NONMINIMAL_REALIZATION`）。
- ss→tf 按 R-7（scipy `ss2tf` 约定，所有元素共享 det(sI − A)）；zpk→tf 为乘积展开；tf→zpk 为多项式求根（平衡伴随矩阵特征值 + Newton 精修，共轭对精确共轭）；ss→zpk 为不变零点 + eig(A) + Markov 增益（R-8）。
- 非 proper 的 tf → `E.DOMAIN.IMPROPER_SYSTEM`；MIMO→zpk → `E.DOMAIN.MIMO_NOT_SUPPORTED`；传递函数恒为零的 ss→zpk 返回 k = 0、无零点（与 python-control 一致），结构退化 → `E.DOMAIN.DEGENERATE_SYSTEM`。
- trust：在 9 个测试点（连续 jω；离散单位圆）比较转换前后的频率响应，最大相对差写入 `residual.response_max_rel_discrepancy`；> 1e-6 时 `W.NUMERICAL.CONVERSION_DISCREPANCY`（高阶/重根时多项式表示本身病态）。

**Manifest** — `examples/control.lti.convert/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.convert",
  "version": "0.3.0",
  "title": "LTI representation conversion",
  "description": "在 tf / ss / zpk 三种表示之间转换 LTI 系统，并给出实现约定与转换残差（转换前后频率响应的最大相对差）。",
  "category": "transform",
  "tags": ["lti", "realization"],
  "applicability": {
    "preconditions": ["system 为 proper", "zpk 只表示 SISO 系统（v0.3）：MIMO → zpk 返回 E.DOMAIN.MIMO_NOT_SUPPORTED"],
    "not_for": ["最小实现（minreal，v0.3 不做零极点对消）", "带时滞的系统"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "tf→ss：可控标准型（controller canonical form，A 首行 = −den[1:]/den[0]，B = e₁，与 scipy.signal.tf2ss 及 MATLAB tf2ss 形式相同）；MIMO tf→ss：逐元素可控标准型块对角拼接（非最小实现）；ss→tf：den = det(sI−A)（由特征值展开），num_ij = det(sI−A+b_j c_i) + (d_ij−1)·den（与 scipy.signal.ss2tf 相同，非最小，不对消）；tf→zpk：伴随矩阵（Parlett–Reinsch 平衡）特征值求根 + ≤4 步 Newton 修正，k = num[0]/den[0]；ss→zpk：p = eig(A)，z = 不变零点（见 control.lti.poles_zeros），k = 首个非零 Markov 参数 C·A^{r−1}·B（r = #p−#z）；zpk→tf：乘积展开",
    "references": [
      {
        "type": "textbook",
        "cite": "T. Kailath, Linear Systems, Prentice-Hall 1980（实现、可控/可观、PBH 判据）"
      },
      {
        "type": "paper",
        "cite": "A. Emami-Naeini, P. Van Dooren, Computation of zeros of linear multivariable systems, Automatica 18(4), 1982",
        "doi": "10.1016/0005-1098(82)90070-X"
      },
      {
        "type": "paper",
        "cite": "B. N. Parlett, C. Reinsch, Balancing a matrix for calculation of eigenvalues and eigenvectors, Numer. Math. 13, 1969",
        "doi": "10.1007/BF02165404"
      },
      {
        "type": "software",
        "cite": "scipy.signal tf2ss / ss2tf 文档（仅作约定对照，未参考实现代码）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "sysalg::convert",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 200
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.convert/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC",
    "E.DOMAIN.MIMO_NOT_SUPPORTED",
    "E.DOMAIN.DEGENERATE_SYSTEM"
  ],
  "warnings": ["W.DOMAIN.NONMINIMAL_REALIZATION", "W.NUMERICAL.CONVERSION_DISCREPANCY", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_lti_convert",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:convert:canonical-realization+companion-roots",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "nonminimal",
        "statement": "不做零极点对消（MIMO 结果为非最小实现）",
        "enforced_by": "warning:W.DOMAIN.NONMINIMAL_REALIZATION"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": ["response_consistent"],
    "coverage": {
      "response_consistent": "test_points"
    }
  }
}
```

**请求**（G(s) = 4/(s² + 1.2s + 4) 转为 ss；手算：A = [[−1.2, −4], [1, 0]]，B = [1, 0]ᵀ，C = [0, 4]，D = 0）

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4.0
    ],
    "den": [
      1.0,
      1.2,
      4.0
    ],
    "ts": 0.0,
    "time_unit": "s"
  },
  "to": "ss",
  "exec": {
    "request_id": "req-convert-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.convert",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-convert-001",
  "result": {
    "system": {
      "kind": "ss",
      "A": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [-1.2, -4.0, 1.0, 0.0]
      },
      "B": {
        "dtype": "f64",
        "shape": [2, 1],
        "data": [1.0, 0.0]
      },
      "C": {
        "dtype": "f64",
        "shape": [1, 2],
        "data": [0.0, 4.0]
      },
      "D": {
        "dtype": "f64",
        "shape": [1, 1],
        "data": [0.0]
      },
      "ts": 0.0,
      "time_unit": "s"
    },
    "realization": "controllable_canonical",
    "minimal": null
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "response_max_rel_discrepancy": 2.5727768029146797e-16,
      "definition": "在 9 个测试点（连续：jω，ω = 10^[-2..2]×max(1,max|p|)；离散：单位圆上 e^{jθ}）比较转换前后 G 的最大逐元素相对差（以 max|G| 归一），跳过离极点过近的点"
    },
    "condition": {
      "from": "tf",
      "to": "ss",
      "output_states": 2
    },
    "error_estimate": {
      "response_rel": 2.5727768029146797e-16,
      "basis": "后验：等于上述响应残差（非严格上界；对根与系数本身的误差不作保证）"
    },
    "checks": {
      "response_consistent": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:convert:canonical-realization+companion-roots",
      "routine": "sysalg::convert",
      "provenance": "aeoi://operators/control.lti.convert/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "nonminimal",
        "statement": "不做零极点对消（MIMO 结果为非最小实现）",
        "enforced_by": "warning:W.DOMAIN.NONMINIMAL_REALIZATION"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "response_rel": 2.5727768029146797e-16
      },
      "basis": "后验：等于上述响应残差（非严格上界；对根与系数本身的误差不作保证）"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "response_consistent": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null,
      "coverage": {
        "response_consistent": "test_points"
      }
    },
    "fingerprint": {
      "request": "sha256:1dc402f9a40d13aa494f87f0784a0011ce79aecffe2edba8c66fab1d6b0c4119",
      "result": "sha256:2fd30e29c67c43f7ec73f026b083b46d1a6a970a334ae0c39ee490bb7bdc3d6f",
      "output_hash": "sha256:84dd5712391abb4cee44a4f7fbe27e6924baec2df209a064a071aa08b1c382d7",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.convert@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:47ab71b6489b0ee84c790c9faf7a6dbded5ea9cf55e55c8d7985b8fe22b519bf",
    "output_hash": "sha256:84dd5712391abb4cee44a4f7fbe27e6924baec2df209a064a071aa08b1c382d7",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.16304000000000002
  }
}
```

**错误请求与响应**（2×2 ss 请求转 zpk：v0.3 的 zpk 只表示 SISO）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [-1.0, -0.0, -0.0, -1.0]
    },
    "B": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [1.0, 0.0, 0.0, 1.0]
    },
    "C": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [1.0, 0.0, 0.0, 1.0]
    },
    "D": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [0.0, 0.0, 0.0, 0.0]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "to": "zpk",
  "exec": {
    "request_id": "req-convert-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.convert",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-convert-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.MIMO_NOT_SUPPORTED",
    "category": "domain",
    "retryable": true,
    "message": "zpk 表示（v0.3） 只支持 SISO 系统，输入为 2×2。",
    "agent_hint": "逐通道调用（取出单个输出/输入对应的 SISO 子系统），或改用支持 MIMO 的表示（tf/ss）。",
    "details": {
      "path": "/to",
      "dims": [2, 2]
    },
    "remediation": [
      {
        "id": "to_tf",
        "kind": "modify_input",
        "description": "改为转换到 tf（MIMO 支持）",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/to",
            "value": "tf"
          }
        ]
      },
      {
        "id": "to_ss",
        "kind": "modify_input",
        "description": "改为转换到 ss",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/to",
            "value": "ss"
          }
        ]
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.112819
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（3 阶非对称 A 的 ss→tf），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.lti.convert/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.lti.convert/parity.yaml`

```yaml
# Parity test spec — control.lti.convert 0.3.0 (AEOI v0.3-draft)
operator: control.lti.convert
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.lti.convert/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T1/T2
compare:
  "tf→ss matrices": "rel_fro ≤ 1e-12 (T1)"
  "tf coefficients": "‖Δ‖₁/max(‖ref‖₁,‖den‖₁) ≤ 1e-10·max(1, Π(1+|λ_i|)/‖den‖₁)  (λ = roots(den); coefficient-from-eigenvalue bound)"
  "zpk zeros/poles": "nearest-match |Δ|/max(1,|r|) ≤ 1e-10·max(1, κ_r) (Wilkinson root condition of the reference polynomial)"
  "zpk gain k": "rel ≤ 1e-10"
case_families:   # n = golden cases
  - {name: improper, n: 1, expect: E.DOMAIN.IMPROPER_SYSTEM}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: mimo_to_zpk, n: 1, expect: E.DOMAIN.MIMO_NOT_SUPPORTED}
  - {name: ss_to_tf_mimo, n: 8, expect_warning: ['W.DOMAIN.NONMINIMAL_REALIZATION']}
  - {name: ss_to_tf_siso, n: 15}
  - {name: ss_to_zpk_siso, n: 10}
  - {name: tf_to_ss_siso, n: 15}
  - {name: tf_to_zpk, n: 15}
  - {name: zpk_to_tf, n: 10}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.5 `control.lti.poles_zeros`（v0.3 新增）

极点、（传输/不变）零点、DC 增益、自然频率与阻尼比。

语义要点：
- 极点 = eig(A)（tf 为分母根）；零点 = 不变零点：先做原始与对偶两遍 Emami-Naeini/Van Dooren 式 SVD 秩揭示约化直到 D 方阵可逆，再求 eig(A − B·D⁻¹·C)。支持非方系统（如 2×1 系统的公共零点）。
- 法秩不足（例如 C = 0）→ `E.DOMAIN.DEGENERATE_SYSTEM`（R-9），不返回空集。
- DC 增益按 D-2：连续 s = 0，离散 z = 1；A 数值奇异时改用逐元素 zpk 极限（带零极点对消判定），方法写在 `trust.condition.dcgain_method`。
- 自然频率/阻尼比：连续 ωn = |λ|、ζ = −Re λ/|λ|；离散先映射 s = ln(z)/ts（与 python-control `damp` 一致）。
- 约化后 D 的条件数写入 trust；偏大时 `W.NUMERICAL.ILL_CONDITIONED`。

**Manifest** — `examples/control.lti.poles_zeros/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.poles_zeros",
  "version": "0.3.0",
  "title": "Poles, zeros and DC gain",
  "description": "返回 LTI 系统的极点、（不变/传输）零点、DC 增益（连续 s=0，离散 z=1）以及每个极点的自然频率与阻尼比。",
  "category": "analysis",
  "tags": ["lti", "poles", "zeros"],
  "applicability": {
    "preconditions": ["system 为 proper"],
    "not_for": ["非线性系统", "带时滞的系统"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "极点：ss 为 eig(A)；SISO tf 为 roots(den)；MIMO tf 为各元素分母根的并集（即块对角实现的特征值，非最小）；zpk 为 p。零点：SISO tf 为 roots(num)；ss（任意 p×m）为 Rosenbrock 系统矩阵 [[A−λI,B],[C,D]] 的有限不变零点：两遍基于 SVD 数值秩的压缩（Emami-Naeini & Van Dooren 1982 思路：先压缩 D 的行、再压缩 C 的残余行，原系统与对偶系统各一遍）把 D 化为方阵可逆，再取 eig(A_r − B_r D_r⁻¹ C_r)；实现最小时不变零点 = 传输零点。DC 增益：A（或 I−A）非奇异时 D − C A⁻¹ B（离散 D + C(I−A)⁻¹B）；奇异时逐元素按 zpk 形式在 0（或 1）处对消重合零极点后求极限，未对消则为 ±Inf。阻尼：连续 ω_n = |p|、ζ = −Re p/|p|；离散先映射 s = ln(z)/ts",
    "references": [
      {
        "type": "paper",
        "cite": "A. Emami-Naeini, P. Van Dooren, Computation of zeros of linear multivariable systems, Automatica 18(4), 1982",
        "doi": "10.1016/0005-1098(82)90070-X"
      },
      {
        "type": "textbook",
        "cite": "T. Kailath, Linear Systems, Prentice-Hall 1980（实现、可控/可观、PBH 判据）"
      },
      {
        "type": "paper",
        "cite": "B. N. Parlett, C. Reinsch, Balancing a matrix for calculation of eigenvalues and eigenvectors, Numer. Math. 13, 1969",
        "doi": "10.1007/BF02165404"
      },
      {
        "type": "textbook",
        "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., 2013（SVD 数值秩、Schur 分解）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "poles_zeros::analyze",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 200
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.poles_zeros/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC",
    "E.DOMAIN.DEGENERATE_SYSTEM"
  ],
  "warnings": ["W.DOMAIN.NONMINIMAL_REALIZATION", "W.DOMAIN.INFINITE_DC_GAIN", "W.DOMAIN.UNDEFINED_DC_GAIN", "W.NUMERICAL.ILL_CONDITIONED", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_lti_poles_zeros",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:poles-zeros:balanced-qr+emami-naeini-van-dooren",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "nondegenerate",
        "statement": "系统满法秩（零点有定义）",
        "enforced_by": "error:E.DOMAIN.DEGENERATE_SYSTEM"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none"
    },
    "postconditions": []
  }
}
```

**请求**（G(s) = (s + 3)/(s² + 1.2s + 4)；手算：零点 −3，极点 −0.6 ± 1.907878j，DC 增益 3/4，ωn = 2，ζ = 0.3）

```json
{
  "system": {
    "kind": "tf",
    "num": [
      1.0,
      3.0
    ],
    "den": [
      1.0,
      1.2,
      4.0
    ],
    "ts": 0.0,
    "time_unit": "s"
  },
  "exec": {
    "request_id": "req-pz-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.poles_zeros",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-pz-001",
  "result": {
    "poles": {
      "dtype": "c128",
      "shape": [2],
      "data": [[-0.6, 1.9078784028338913], [-0.6, -1.9078784028338913]]
    },
    "zeros": {
      "dtype": "c128",
      "shape": [1],
      "data": [[-3.0, 0.0]]
    },
    "zero_definition": "polynomial_roots",
    "dcgain": {
      "unit": "1",
      "value": {
        "dtype": "f64",
        "shape": [1, 1],
        "data": [0.75]
      }
    },
    "natural_frequency": {
      "unit": "rad/s",
      "value": {
        "dtype": "f64",
        "shape": [2],
        "data": [2.0, 2.0]
      }
    },
    "damping_ratio": {
      "dtype": "f64",
      "shape": [2],
      "data": [0.3, 0.3]
    },
    "stable": true
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": null,
    "condition": {
      "zero_reduction": null,
      "dcgain_method": "polynomial_evaluation_with_exact_cancellation",
      "max_pole_modulus": 2.0
    },
    "error_estimate": {
      "basis": "未给出逐根误差界：特征值/多项式根对扰动的敏感度取决于（未计算的）特征向量条件数；重根的误差可达 O(u^{1/k})"
    },
    "checks": {
      "stable": true,
      "dcgain_finite": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:poles-zeros:balanced-qr+emami-naeini-van-dooren",
      "routine": "poles_zeros::analyze",
      "provenance": "aeoi://operators/control.lti.poles_zeros/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "nondegenerate",
        "statement": "系统满法秩（零点有定义）",
        "enforced_by": "error:E.DOMAIN.DEGENERATE_SYSTEM"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none",
      "bound": null,
      "basis": "未给出逐根误差界：特征值/多项式根对扰动的敏感度取决于（未计算的）特征向量条件数；重根的误差可达 O(u^{1/k})"
    },
    "evidence": {
      "claim": "unchecked",
      "postconditions": {},
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:67e415f5e5302d7fd9b74e8a8bae42223582dfc89be113343e2b8ff592f3bc65",
      "result": "sha256:c42395f18827cd28b019a4af16f793c8ce683625aca18f14f734a8b74e531720",
      "output_hash": "sha256:22d247d0c6422a9995702e229b144a26ed08811a4de6bdcf8cfd4f17b27081eb",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.poles_zeros@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:6d770ac001d7fd62932c5acfc061d77f909dbfc6267ae104f641e6c56dc84ee5",
    "output_hash": "sha256:22d247d0c6422a9995702e229b144a26ed08811a4de6bdcf8cfd4f17b27081eb",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.159227
  }
}
```

**错误请求与响应**（C = 0 的 ss：传递函数恒为零，零点无定义）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [-1.0, -0.0, -0.0, -1.0]
    },
    "B": {
      "dtype": "f64",
      "shape": [2, 1],
      "data": [1.0, 1.0]
    },
    "C": {
      "dtype": "f64",
      "shape": [1, 2],
      "data": [0.0, 0.0]
    },
    "D": {
      "dtype": "f64",
      "shape": [1, 1],
      "data": [0.0]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "exec": {
    "request_id": "req-pz-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.poles_zeros",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-pz-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.DEGENERATE_SYSTEM",
    "category": "domain",
    "retryable": true,
    "message": "Rosenbrock 系统矩阵不是满法秩（例如某输出恒为零或输入全部不起作用），此时每个复数都是“零点”，零点无定义。",
    "agent_hint": "检查 B、C、D 中是否有全零行/列；去掉恒零通道后再求零点。",
    "details": {
      "path": "/system"
    },
    "remediation": [
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认输入与建模假设",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.174316
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（3 阶非对称 A），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.lti.poles_zeros/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.lti.poles_zeros/parity.yaml`

```yaml
# Parity test spec — control.lti.poles_zeros 0.3.0 (AEOI v0.3-draft)
operator: control.lti.poles_zeros
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.lti.poles_zeros/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "poles/zeros": "nearest-match |Δ|/max(1,|r|) ≤ 1e-10·max(1, κ_r)"
  "dcgain": "|Δ|/max(1,|ref|) ≤ 1e-10·max(1, Π(1+|p|)/Π|p|); ±Inf must agree exactly"
  "natural_frequency / damping_ratio": "≤ 1e-10·max(1, κ_r) of the matched pole"
case_families:   # n = golden cases
  - {name: degenerate, n: 1, expect: E.DOMAIN.DEGENERATE_SYSTEM}
  - {name: discrete_ss, n: 8}
  - {name: integrator_infinite_dc, n: 2, expect_warning: ['W.DOMAIN.INFINITE_DC_GAIN']}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: siso_tf, n: 15}
  - {name: square_ss, n: 15}
  - {name: zpk, n: 5}
reference_postprocess: "python-control (no slycot) returns infinite generalized eigenvalues of the zero pencil as huge finite numbers; zeros with |z| > 1e10·(1+‖[A B; C D]‖_F) are dropped from the reference"
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.6 `control.analysis.ctrb_obsv`（v0.3 新增）

可控性/可观性矩阵、数值秩、可选 Gramian。

语义要点：
- Krylov 矩阵 [B, AB, …, A^{n−1}B] 与 [C; CA; …]（T1）；数值秩 = 奇异值 > rtol·σ_max 的个数，默认 rtol = max(行, 列)·ε（与 numpy `matrix_rank` 一致），可用 `rank_rtol` 覆盖。
- PBH 交叉检查：min over λ ∈ eig(A) 的 σ_min([A − λI, B])/‖[A, B]‖_F；与 Krylov 秩结论不一致、或秩阈值处无明显奇异值间隙时 `W.NUMERICAL.RANK_AMBIGUOUS`（Krylov 矩阵在 n 较大时本身病态）。
- `gramians: true`：连续 A Wc + Wc Aᵀ + B Bᵀ = 0，离散 A Wc Aᵀ − Wc + B Bᵀ = 0（Wo 对偶），用 `control.math.lyap` 同一 Bartels–Stewart 实现；系统不渐近稳定时 Gramian 为 `null` + `W.DOMAIN.GRAMIAN_UNDEFINED`（R-3）。
- tf/zpk 输入先按可控标准型实现，结果依赖实现：`W.DOMAIN.REALIZATION_DEPENDENT`。

**Manifest** — `examples/control.analysis.ctrb_obsv/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.analysis.ctrb_obsv",
  "version": "0.3.0",
  "title": "Controllability / observability",
  "description": "判断 LTI 系统是否可控、是否可观（“(A, B) 是否可控”“可控性矩阵的秩是多少”“系统是否可观”这类问题用本工具；只给出 A、B 时也用本工具，C、D 按 system 的说明补零即可）。计算可控性矩阵 [B AB … A^{n−1}B] 与可观性矩阵 [C; CA; …; CA^{n−1}]、基于 SVD 的数值秩，以及（可选）由 Lyapunov 方程得到的可控/可观 Gramian。高阶系统的 Krylov 矩阵严重病态：Krylov 秩与 PBH 检验不一致时 receipt.claim 为 unverified，receipt 列出不可依赖的字段（rank、full_rank、minimal），不要把它们当结论。设计控制器不需要先调用本工具。",
  "category": "analysis",
  "tags": ["lti", "controllability", "observability", "gramian"],
  "applicability": {
    "preconditions": ["system 为 proper；tf/zpk 先按可控标准型实现（结果依赖实现，给出 warning）", "Gramian 需系统渐近稳定（连续 Re λ < 0；离散 |λ| < 1）"],
    "not_for": ["大规模系统（n > 100）的秩判定：Krylov 矩阵病态，应改用 PBH/阶梯形", "控制器设计、带宽/裕度检查（用 control_analysis_loopsens，或 control_lti_series / control_lti_feedback 的具名角色）"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "Krylov 块逐列构造；数值秩 = #{σ_i > rtol·σ_max}，rtol 缺省为 max(行,列)·ε（与 numpy.linalg.matrix_rank 相同）；同时对 A 的每个特征值做 PBH 检验 σ_min([A−λI, B])/‖[A,B]‖_F 作为更稳健的旁证，二者不一致时给出 W.NUMERICAL.RANK_AMBIGUOUS。Gramian：连续 A W_c + W_c Aᵀ + BBᵀ = 0、Aᵀ W_o + W_o A + CᵀC = 0；离散 A W_c Aᵀ − W_c + BBᵀ = 0、Aᵀ W_o A − W_o + CᵀC = 0；用复 Schur 形式的 Bartels–Stewart 法求解（同 control.math.lyap）",
    "references": [
      {
        "type": "textbook",
        "cite": "T. Kailath, Linear Systems, Prentice-Hall 1980（实现、可控/可观、PBH 判据）"
      },
      {
        "type": "textbook",
        "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., 2013（SVD 数值秩、Schur 分解）"
      },
      {
        "type": "paper",
        "cite": "R. H. Bartels, G. W. Stewart, Solution of the matrix equation AX + XB = C, Comm. ACM 15(9), 1972",
        "doi": "10.1145/361573.361582"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "ctrb_obsv::analyze",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 100
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.analysis.ctrb_obsv/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC"
  ],
  "warnings": ["W.DOMAIN.REALIZATION_DEPENDENT", "W.NUMERICAL.RANK_AMBIGUOUS", "W.DOMAIN.GRAMIAN_UNDEFINED", "W.NUMERICAL.ILL_CONDITIONED", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_analysis_ctrb_obsv",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:ctrb-obsv:krylov-svd-rank+pbh+bartels-stewart-gramian",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "rank_threshold",
        "statement": "数值秩阈值 max(n,m)·u·σ₁ 有意义（不在阈值附近）",
        "enforced_by": "warning:W.NUMERICAL.RANK_AMBIGUOUS"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none"
    },
    "postconditions": ["pbh_agrees_ctrb", "pbh_agrees_obsv"],
    "guards": {
      "pbh_agrees_ctrb": ["/ctrb/rank", "/ctrb/full_rank", "/minimal"],
      "pbh_agrees_obsv": ["/obsv/rank", "/obsv/full_rank", "/minimal"]
    }
  }
}
```

**请求**（G(s) = 4/(s² + 1.2s + 4) 的 ss 形式 + Gramian；手算：ctrb = [[0, 4], [4, −4.8]]，obsv = I）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [
        2,
        2
      ],
      "data": [
        0.0,
        1.0,
        -4.0,
        -1.2
      ]
    },
    "B": {
      "dtype": "f64",
      "shape": [
        2,
        1
      ],
      "data": [
        0.0,
        4.0
      ]
    },
    "C": {
      "dtype": "f64",
      "shape": [
        1,
        2
      ],
      "data": [
        1.0,
        0.0
      ]
    },
    "D": {
      "dtype": "f64",
      "shape": [
        1,
        1
      ],
      "data": [
        0.0
      ]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "gramians": true,
  "exec": {
    "request_id": "req-co-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.analysis.ctrb_obsv",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-co-001",
  "result": {
    "ctrb": {
      "matrix": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [0.0, 4.0, 4.0, -4.8]
      },
      "rank": 2,
      "singular_values": {
        "dtype": "f64",
        "shape": [2],
        "data": [7.06476151587624, 2.26476151587624]
      },
      "full_rank": true
    },
    "obsv": {
      "matrix": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [1.0, 0.0, 0.0, 1.0]
      },
      "rank": 2,
      "singular_values": {
        "dtype": "f64",
        "shape": [2],
        "data": [1.0, 1.0]
      },
      "full_rank": true
    },
    "gramians": {
      "controllability": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [1.666666666666666, 1.8041124150158794e-16, 1.8041124150158794e-16, 6.666666666666664]
      },
      "observability": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [0.5666666666666668, 0.125, 0.125, 0.10416666666666666]
      }
    },
    "minimal": true
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "controllability_rel_residual": 7.119997686835877e-17,
      "observability_rel_residual": 2.5816843116640615e-17,
      "separation_min": 1.1999999999999995,
      "definition": "‖A W + W Aᵀ + BBᵀ‖_F / (2‖A‖_F‖W‖_F + ‖BBᵀ‖_F)（离散：‖A W Aᵀ − W + BBᵀ‖_F / (‖A‖²‖W‖+‖W‖+‖BBᵀ‖)），可观同理"
    },
    "condition": {
      "ctrb_rank_tolerance": 3.13738435936461e-15,
      "obsv_rank_tolerance": 4.440892098500626e-16,
      "ctrb_sv_gap_at_rank": null,
      "obsv_sv_gap_at_rank": null,
      "pbh_ctrb_min_rel_sv": 0.2442208482397884,
      "pbh_obsv_min_rel_sv": 0.0998330718720949,
      "pbh_threshold": 1e-10,
      "ctrb_condition": 3.119428454762873,
      "realization": "given ss"
    },
    "error_estimate": {
      "basis": "秩为整数判定（T0）；Krylov 矩阵元素为精确的矩阵乘积（T1）；Gramian 见 residual"
    },
    "checks": {
      "pbh_agrees_ctrb": true,
      "pbh_agrees_obsv": true,
      "asymptotically_stable": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:ctrb-obsv:krylov-svd-rank+pbh+bartels-stewart-gramian",
      "routine": "ctrb_obsv::analyze",
      "provenance": "aeoi://operators/control.analysis.ctrb_obsv/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "rank_threshold",
        "statement": "数值秩阈值 max(n,m)·u·σ₁ 有意义（不在阈值附近）",
        "enforced_by": "warning:W.NUMERICAL.RANK_AMBIGUOUS"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none",
      "bound": null,
      "basis": "秩为整数判定（T0）；Krylov 矩阵元素为精确的矩阵乘积（T1）；Gramian 见 residual"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "pbh_agrees_ctrb": true,
        "pbh_agrees_obsv": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:c2070194b581519bcdd13116d7fb6a5fe66365e5c690098bea91d26e1c227b94",
      "result": "sha256:abc418c68b6d7ca75ae893ba9c9e6a9a6f328acf80d162b9b4439b8f359d8b36",
      "output_hash": "sha256:4b88f480e4dff3988e44cce0b2f28f372d9420e371da7be0103ae969bdd37258",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.analysis.ctrb_obsv@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:6da5768803cb1a03a779b6e10dbf4d62d7825c430bfe6c21d6b2ae1067dd1c53",
    "output_hash": "sha256:4b88f480e4dff3988e44cce0b2f28f372d9420e371da7be0103ae969bdd37258",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.269598
  }
}
```

**错误请求与响应**（B 的行数与 A 不符）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [0.0, 1.0, -4.0, -1.2]
    },
    "B": {
      "dtype": "f64",
      "shape": [3, 1],
      "data": [0.0, 4.0, 1.0]
    },
    "C": {
      "dtype": "f64",
      "shape": [1, 2],
      "data": [1.0, 0.0]
    },
    "D": {
      "dtype": "f64",
      "shape": [1, 1],
      "data": [0.0]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "exec": {
    "request_id": "req-co-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.analysis.ctrb_obsv",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-co-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.VALIDATION.SHAPE",
    "category": "validation",
    "retryable": true,
    "message": "状态空间维度不一致：B 的行数 ≠ n。",
    "agent_hint": "要求 A:[n,n] B:[n,m] C:[p,n] D:[p,m]；注意数据按 row-major 展平。",
    "details": {
      "path": "/system/B",
      "shapes": {
        "A": [2, 2],
        "B": [3, 1],
        "C": [1, 2],
        "D": [1, 1]
      }
    },
    "remediation": [
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认输入与建模假设",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.115999
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A，含 Gramian），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.analysis.ctrb_obsv/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.analysis.ctrb_obsv/parity.yaml`

```yaml
# Parity test spec — control.analysis.ctrb_obsv 0.3.0 (AEOI v0.3-draft)
operator: control.analysis.ctrb_obsv
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.analysis.ctrb_obsv/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T0/T1/T2
compare:
  "rank": "exact (T0)"
  "Krylov matrices": "rel_fro ≤ 1e-12 (T1)"
  "singular values": "max|Δσ|/σ_max ≤ 1e-12"
  "gramians": "rel_fro ≤ max(1e-10, 10·rel_residual/min(1, sep_min))"
case_families:   # n = golden cases
  - {name: discrete_gramians, n: 5}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: random_stable_gramians, n: 15}
  - {name: uncontrollable, n: 4}
  - {name: unstable_gramian_undefined, n: 1, expect_warning: ['W.DOMAIN.GRAMIAN_UNDEFINED']}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.7 `control.analysis.margin`（v0.3 新增）

增益裕度、相位裕度、稳定裕度及全部交越点（SISO）。

语义要点：
- 交越点由多项式精确求根得到，不依赖网格：相位交越 Im[N(jω)·D(−jω)] = 0，增益交越 |N(jω)|² − |D(jω)|² = 0，稳定裕度的驻点为 d|1+G|²/dω 的分子根；偶/奇多项式先做 x = ω² 降阶。离散系统按 D-6 经双线性映射处理。
- 求根后在 G 上做割线修正；再用 4000 点对数网格做符号变化审计，漏检的交越点用二分补回并给 `W.NUMERICAL.CROSSING_RECOVERED`（近重根/相切时求根可能漏根）。
- 选择规则与 python-control 0.10.2 一致：GM 取 Re G ≤ 0 的相位交越中 |ln GM| 最小者；PM = mod(∠G, 360) − 180 中 |PM| 最小者；SM 取最小值。全部候选在 `crossings` 中返回（R-4）。
- 负单位反馈闭环稳定性由 N + D 的根判定；不稳定时 `W.DOMAIN.CLOSED_LOOP_UNSTABLE`（裕度数值不再代表鲁棒性）。MIMO → `E.DOMAIN.MIMO_NOT_SUPPORTED`。

**Manifest** — `examples/control.analysis.margin/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.analysis.margin",
  "version": "0.3.0",
  "title": "Gain / phase / stability margins",
  "description": "计算 SISO 开环系统（负单位反馈）的增益裕度、相位裕度、稳定裕度（到 −1 点的最小距离）及对应交越频率；交越点由多项式求根精确求得，并用密集对数网格的符号变化做漏检复查。",
  "category": "analysis",
  "tags": ["frequency-domain", "robustness", "lti"],
  "applicability": {
    "preconditions": ["SISO", "system 为 proper", "离散系统：频率范围 (0, π/ts)"],
    "not_for": ["MIMO（v0.3 不支持，用 disk margin 类算子）", "带时滞的系统"],
    "siso_only": true
  },
  "provenance": {
    "algorithm": "先转为 tf G=n/d（ss 用 ss2tf）。连续：令 s=jω，n(jω)=n_r(ω)+j n_i(ω)、d 同理，均为实系数多项式；相位交越 Im G=0 ⇔ n_i d_r − n_r d_i = 0；增益交越 |G|=1 ⇔ n_r²+n_i² − d_r²−d_i² = 0；稳定裕度极值 ⇔ d/dω(|n+d|²/|d|²) = 0 且二阶导 > 0。各多项式用平衡伴随矩阵求根并 Newton 修正，取 |Im r| ≤ 1e-6·max(1,|r|) 的实根，再在原 G 上用 Newton 迭代精化。离散：双线性代换 z=(1+w)/(1−w) 把单位圆映为虚轴，在 w=jν 上重复上述步骤，ω = (2/ts)·atan ν。复查：在覆盖全部零极点频率的 4000 点对数网格上检测 |G|−1 与 Im G 的符号变化，漏检者用二分法精化后补入并给出 W.NUMERICAL.CROSSING_RECOVERED。选择规则（与 python-control stability_margins 相同）：GM 取 |log GM| 最小者（相位交越且 Re G ≤ 0），PM = mod(∠G, 360°) − 180° 取 |PM| 最小者，SM 取最小值",
    "references": [
      {
        "type": "textbook",
        "cite": "G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6（裕度定义）"
      },
      {
        "type": "paper",
        "cite": "B. N. Parlett, C. Reinsch, Balancing a matrix for calculation of eigenvalues and eigenvectors, Numer. Math. 13, 1969",
        "doi": "10.1007/BF02165404"
      },
      {
        "type": "software",
        "cite": "python-control stability_margins 文档（仅作选择规则对照）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "margin::compute",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T3",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 100,
    "scan_points": 4000
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.analysis.margin/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.ZERO_DENOMINATOR",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC",
    "E.DOMAIN.MIMO_NOT_SUPPORTED"
  ],
  "warnings": ["W.NUMERICAL.CROSSING_RECOVERED", "W.DOMAIN.CLOSED_LOOP_UNSTABLE", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_analysis_margin",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:margin:crossing-polynomial-roots+secant",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "siso",
        "statement": "开环 SISO、单位负反馈",
        "enforced_by": "error:E.DOMAIN.MIMO_NOT_SUPPORTED"
      },
      {
        "id": "no_delay",
        "statement": "没有时滞",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none"
    },
    "postconditions": ["grid_audit_clean"],
    "coverage": {
      "grid_audit_clean": "sampled"
    }
  }
}
```

**请求**（L(s) = 4/(s(s+1)(s+2))；手算：ω = √2 处相位 −180°，|L| = 2/3 ⇒ GM = 1.5（3.52 dB））

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4.0
    ],
    "den": [
      1.0,
      3.0,
      2.0,
      0.0
    ],
    "ts": 0.0,
    "time_unit": "s"
  },
  "exec": {
    "request_id": "req-margin-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.analysis.margin",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-margin-001",
  "result": {
    "gain_margin": {
      "unit": "1",
      "value": 1.4999999999999998
    },
    "gain_margin_db": {
      "unit": "dB",
      "value": 3.521825181113623
    },
    "phase_margin": {
      "unit": "deg",
      "value": 11.424981844921433
    },
    "stability_margin": {
      "unit": "1",
      "value": 0.17020817530521032
    },
    "phase_crossover_frequency": {
      "unit": "rad/s",
      "value": 1.414213562373095
    },
    "gain_crossover_frequency": {
      "unit": "rad/s",
      "value": 1.1432030439610739
    },
    "stability_margin_frequency": {
      "unit": "rad/s",
      "value": 1.2021253965135141
    },
    "crossings": {
      "phase": [
        {
          "w_rad_s": 1.414213562373095,
          "gain_margin": 1.4999999999999998,
          "gain_margin_db": 3.521825181113623
        }
      ],
      "gain": [
        {
          "w_rad_s": 1.1432030439610739,
          "phase_margin_deg": 11.424981844921433
        }
      ],
      "stability": [
        {
          "w_rad_s": 1.2021253965135141,
          "stability_margin": 0.17020817530521032
        }
      ]
    },
    "closed_loop_stable": true
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "gain_crossing_max_abs_ln_mag": 2.2204460492503128e-16,
      "phase_crossing_max_abs_sin_arg": 1.0467283057891834e-16,
      "definition": "增益交越处 |ln|G(jω)||、相位交越处 |Im G|/|G| 的最大值（理想为 0）"
    },
    "condition": {
      "tf_from": "tf",
      "num_degree": 0,
      "den_degree": 3,
      "grid_audit_recovered": 0
    },
    "error_estimate": {
      "basis": "交越频率为多项式实根（伴随矩阵特征值 + Newton），再在 G 上做割线修正；残差见 residual。相切/近重根时交越频率误差可达 O(√u)"
    },
    "checks": {
      "closed_loop_stable": true,
      "grid_audit_clean": true
    },
    "accuracy_grade": "B",
    "credentials": null,
    "derived": {
      "closed_loop_poles": {
        "dtype": "c128",
        "shape": [3],
        "data": [[-2.7963219032594417, 0.0], [-0.10183904837027921, 1.1916707956047328], [-0.10183904837027921, -1.1916707956047328]]
      }
    }
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:margin:crossing-polynomial-roots+secant",
      "routine": "margin::compute",
      "provenance": "aeoi://operators/control.analysis.margin/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "siso",
        "statement": "开环 SISO、单位负反馈",
        "enforced_by": "error:E.DOMAIN.MIMO_NOT_SUPPORTED"
      },
      {
        "id": "no_delay",
        "statement": "没有时滞",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none",
      "bound": null,
      "basis": "交越频率为多项式实根（伴随矩阵特征值 + Newton），再在 G 上做割线修正；残差见 residual。相切/近重根时交越频率误差可达 O(√u)"
    },
    "evidence": {
      "claim": "spot_checked",
      "postconditions": {
        "grid_audit_clean": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T3",
      "credentials": null,
      "coverage": {
        "grid_audit_clean": "sampled"
      }
    },
    "fingerprint": {
      "request": "sha256:99eef92fb181f407d8bebfd819935594a3678bd607c576e7e4ad6ec9fbba3792",
      "result": "sha256:75d6522ed81bcae2614a8040f1b009713bc8dbfacce5a88be09b85cac2fbb48e",
      "output_hash": "sha256:6cb2fc3c7c0667751b2821236fc15af6715c6049ba2739e24b6a41ae58840c1b",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.analysis.margin@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T3",
    "input_hash": "sha256:ee06efb23272488856dcdb3d7a1c46976cf73839392c5a606da3d599a5cb88de",
    "output_hash": "sha256:6cb2fc3c7c0667751b2821236fc15af6715c6049ba2739e24b6a41ae58840c1b",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.340081
  }
}
```

**错误请求与响应**（2×2 系统）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [-1.0, -0.0, -0.0, -1.0]
    },
    "B": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [1.0, 0.0, 0.0, 1.0]
    },
    "C": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [1.0, 0.0, 0.0, 1.0]
    },
    "D": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [0.0, 0.0, 0.0, 0.0]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "exec": {
    "request_id": "req-margin-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.analysis.margin",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-margin-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.MIMO_NOT_SUPPORTED",
    "category": "domain",
    "retryable": true,
    "message": "control.analysis.margin（v0.3） 只支持 SISO 系统，输入为 2×2。",
    "agent_hint": "逐通道调用（取出单个输出/输入对应的 SISO 子系统），或改用支持 MIMO 的表示（tf/ss）。",
    "details": {
      "path": "/system",
      "dims": [2, 2]
    },
    "remediation": [
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认输入与建模假设",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.111566
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A 的 SISO ss），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.analysis.margin/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.analysis.margin/parity.yaml`

```yaml
# Parity test spec — control.analysis.margin 0.3.0 (AEOI v0.3-draft)
operator: control.analysis.margin
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.analysis.margin/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T3
compare:
  "gain margin": "rel ≤ 1e-9"
  "phase margin": "abs ≤ 1e-8 deg"
  "crossover frequencies": "rel ≤ 1e-9"
  "stability margin": "rel ≤ 1e-9; discrete: or ours lower than python-control's optimizer value (reference verified consistent ≤ 1e-9, gap ≤ 1e-6)"
  "SM frequency": "continuous rel ≤ 1e-6 (flat minimum); discrete rel ≤ 1e-3 (python-control uses scipy L-BFGS-B)"
  "crossing sets": "every python-control crossing must be found (extra crossings allowed and reported)"
case_families:   # n = golden cases
  - {name: discrete_tf, n: 8}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: mimo, n: 1, expect: E.DOMAIN.MIMO_NOT_SUPPORTED}
  - {name: random_tf, n: 20}
  - {name: ss_siso, n: 5}
  - {name: type1_loop, n: 10}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.8 `control.lti.c2d`（v0.3 新增）

连续 → 离散：zoh / foh / tustin（可选预畸变）。

语义要点：
- 公式按 D-5，与 scipy `cont2discrete` 相同的状态坐标（因此可逐元素比较）；`output_kind = same` 时 tf 输入返回 tf（MIMO tf 逐元素离散化）。
- `prewarp_frequency` 只与 `tustin` 组合，否则 `E.VALIDATION.OPTION_CONFLICT`（O-2）；预畸变频率 ≥ π/ts → `E.DOMAIN.ABOVE_NYQUIST`；输入已是离散系统 → `E.DOMAIN.NOT_CONTINUOUS`；I − αA 奇异 → `E.NUMERICAL.SINGULAR`。
- 极点虚部超过 π/ts 时 `W.ACCURACY.ALIASING`。trust 记录极点映射不变量：eig(Ad) 与 exp(λ·ts)（tustin 为 (1+λT_w/2)/(1−λT_w/2)）的最大偏差，以及 cond(I − αA)、‖A‖₁·ts、expm 平方次数。
- 实现中发现的缺陷：tustin 的 Dd 初版写成 D + (T/2)·C·Bd（多乘了一个 T），D ≠ 0 的 18 个 golden 用例因此失败；正确为 D + ½·C·Bd（scipy gbt, α = ½）。

**Manifest** — `examples/control.lti.c2d/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.c2d",
  "version": "0.3.0",
  "title": "Continuous-to-discrete conversion",
  "description": "把连续时间 LTI 系统按 zoh / foh / tustin（可选预畸变）离散化为采样周期 ts 的离散系统。",
  "category": "transform",
  "tags": ["lti", "discretization"],
  "applicability": {
    "preconditions": ["ts（采样周期）必须由用户明确给出；用户没给时不要调用本工具，先询问用户，不得假设 0.1 s 等常用值", "system 为连续时间（ts = 0）且 proper", "tustin 要求 1 − p·T/2 ≠ 0（无极点位于 s = 2/T）", "prewarp_frequency 仅用于 tustin，且 < π/ts"],
    "not_for": ["非线性系统", "带时滞的系统（v0.3 不支持）", "d2c（v0.3 未实现，见 §12）"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "ss 层面计算，tf/zpk 先实现为可控标准型（MIMO tf 逐元素离散化后再拼装，结果仍为 tf）。zoh：expm([[A,B],[0,0]]·T) 得 Φ、Γ（Van Loan 1978；Higham 2005 Padé-13）；foh：expm([[A·T, B·T, 0],[0, 0, I],[0, 0, 0]]) 的块 M11、M12、M13，A_d=M11，B_d=M12−M13+M11·M13，C_d=C，D_d=D+C·M13（三角保持形式，与 scipy cont2discrete 'foh' 相同的状态坐标）；tustin：α=T_w/2，A_d=(I−αA)⁻¹(I+αA)，B_d=(I−αA)⁻¹B·T_w，C_d=C(I−αA)⁻¹，D_d=D+α·C·B_d（与 scipy 'bilinear' 相同坐标）；预畸变 T_w = 2·tan(ω_w·T/2)/ω_w，否则 T_w=T。输出种类缺省与输入相同",
    "references": [
      {
        "type": "paper",
        "cite": "C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978",
        "doi": "10.1109/TAC.1978.1101743"
      },
      {
        "type": "paper",
        "cite": "N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005",
        "doi": "10.1137/04061101X"
      },
      {
        "type": "textbook",
        "cite": "G. F. Franklin, J. D. Powell, M. Workman, Digital Control of Dynamic Systems, 3rd ed., 1998（ZOH/FOH/Tustin 与预畸变）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "c2d::discretize",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 200
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.c2d/0.3.0/credentials",
  "errors": ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE", "E.VALIDATION.ZERO_DENOMINATOR", "E.VALIDATION.NOT_CONJUGATE", "…(+12)…", "E.DOMAIN.MIMO_NOT_SUPPORTED", "E.DOMAIN.DEGENERATE_SYSTEM"],
  "warnings": ["W.DOMAIN.NONMINIMAL_REALIZATION", "W.ACCURACY.ALIASING", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_lti_c2d",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:c2d:expm-pade13-or-tustin-lu",
    "assumptions": [
      {
        "id": "ts_from_user",
        "statement": "采样周期 ts 来自用户（不是假设值）",
        "enforced_by": "caller"
      },
      {
        "id": "continuous",
        "statement": "输入为连续系统",
        "enforced_by": "error:E.DOMAIN.NOT_CONTINUOUS"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_priori"
    },
    "postconditions": ["pole_mapping_consistent"]
  }
}
```

**请求**（被控对象 ss 形式，ts = 0.1 s，zoh）

```json
{
  "system": {
    "kind": "ss",
    "A": {
      "dtype": "f64",
      "shape": [
        2,
        2
      ],
      "data": [
        0.0,
        1.0,
        -4.0,
        -1.2
      ]
    },
    "B": {
      "dtype": "f64",
      "shape": [
        2,
        1
      ],
      "data": [
        0.0,
        4.0
      ]
    },
    "C": {
      "dtype": "f64",
      "shape": [
        1,
        2
      ],
      "data": [
        1.0,
        0.0
      ]
    },
    "D": {
      "dtype": "f64",
      "shape": [
        1,
        1
      ],
      "data": [
        0.0
      ]
    },
    "ts": 0.0,
    "time_unit": "s"
  },
  "ts": 0.1,
  "method": "zoh",
  "exec": {
    "request_id": "req-c2d-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.c2d",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-c2d-001",
  "result": {
    "system": {
      "kind": "ss",
      "A": {
        "dtype": "f64",
        "shape": [2, 2],
        "data": [0.9808400407960062, 0.09360615514093044, -0.37442462056372183, 0.8685126546268895]
      },
      "B": {
        "dtype": "f64",
        "shape": [2, 1],
        "data": [0.019159959203993996, 0.3744246205637217]
      },
      "C": {
        "dtype": "f64",
        "shape": [1, 2],
        "data": [1.0, 0.0]
      },
      "D": {
        "dtype": "f64",
        "shape": [1, 1],
        "data": [0.0]
      },
      "ts": 0.1,
      "time_unit": "s"
    },
    "method": "zoh",
    "effective_ts_for_formula": 0.1
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "pole_mapping_max_rel": 1.1443916996305594e-16,
      "definition": "eig(A_d) 与 exp(λ·ts)（λ ∈ eig(A)）逐个最近匹配的最大相对差（以 max(1,|·|) 归一）"
    },
    "condition": {
      "norm1_A_ts": 0.4,
      "expm_squarings": 0,
      "cond_I_minus_alpha_A": null,
      "realization": "given ss"
    },
    "error_estimate": {
      "basis": "expm（Padé-13 + scaling & squaring）后向误差 ≤ u（Higham 2005）；tustin 的误差 ≈ u·cond(I−αA)。极点映射残差为后验旁证"
    },
    "checks": {
      "pole_mapping_consistent": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:c2d:expm-pade13-or-tustin-lu",
      "routine": "c2d::discretize",
      "provenance": "aeoi://operators/control.lti.c2d/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "ts_from_user",
        "statement": "采样周期 ts 来自用户（不是假设值）",
        "enforced_by": "caller"
      },
      {
        "id": "continuous",
        "statement": "输入为连续系统",
        "enforced_by": "error:E.DOMAIN.NOT_CONTINUOUS"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "none_reported",
      "bound": null,
      "basis": "expm（Padé-13 + scaling & squaring）后向误差 ≤ u（Higham 2005）；tustin 的误差 ≈ u·cond(I−αA)。极点映射残差为后验旁证"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "pole_mapping_consistent": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:b9508f2734c8aeec7cecd604c7c6b9c737851643b73787c8c9c4e48f8b706609",
      "result": "sha256:6e54c3c76a781e6dce56daa439dc91a39f77a13d636a219f21d5014c9be1cdd4",
      "output_hash": "sha256:dba22c18d22875927dfe4ecdf33bf0c537a4081e6b61c91bf9e64936e92a9935",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.c2d@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:60558c9b03f398cd802969aafcc2fb47af2f618020659998b0c32b0f15479478",
    "output_hash": "sha256:dba22c18d22875927dfe4ecdf33bf0c537a4081e6b61c91bf9e64936e92a9935",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.19500699999999999
  }
}
```

**错误请求与响应**（zoh + 预畸变频率：选项冲突）

```json
{
  "system": {
    "kind": "tf",
    "num": [4.0],
    "den": [1.0, 1.2, 4.0],
    "ts": 0.0,
    "time_unit": "s"
  },
  "ts": 0.1,
  "method": "zoh",
  "prewarp_frequency": 2.0,
  "exec": {
    "request_id": "req-c2d-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.c2d",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-c2d-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.VALIDATION.OPTION_CONFLICT",
    "category": "validation",
    "retryable": true,
    "message": "prewarp_frequency 只对 tustin 有意义，method = zoh。",
    "agent_hint": "删除 prewarp_frequency，或把 method 改为 tustin。",
    "details": {
      "path": "/prewarp_frequency"
    },
    "remediation": [
      {
        "id": "drop_prewarp",
        "kind": "modify_input",
        "description": "删除 prewarp_frequency",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "remove",
            "path": "/prewarp_frequency"
          }
        ]
      },
      {
        "id": "use_tustin",
        "kind": "modify_input",
        "description": "method 改为 tustin",
        "confidence": "medium",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/method",
            "value": "tustin"
          }
        ]
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.08299100000000001
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A，zoh），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.lti.c2d/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.lti.c2d/parity.yaml`

```yaml
# Parity test spec — control.lti.c2d 0.3.0 (AEOI v0.3-draft)
operator: control.lti.c2d
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.lti.c2d/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "discrete ss matrices": "rel_fro ≤ 1e-10·κ, κ = cond(I − αA) (tustin) or e^{‖A‖₁·ts} (zoh/foh)"
  "discrete tf coefficients": "as convert, times κ"
case_families:   # n = golden cases
  - {name: already_discrete, n: 1, expect: E.DOMAIN.NOT_CONTINUOUS}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: prewarp_with_zoh, n: 1, expect: E.VALIDATION.OPTION_CONFLICT}
  - {name: ss_foh, n: 8}
  - {name: ss_tustin, n: 8}
  - {name: ss_zoh, n: 8}
  - {name: tf_foh, n: 5}
  - {name: tf_tustin, n: 5}
  - {name: tf_zoh, n: 5}
  - {name: tustin_prewarp, n: 5}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.9 `control.lti.impulse`（v0.3 新增）

冲激（脉冲）响应，与 `step` 共用网格、规模限制与 ZOH 递推。

语义要点：
- 连续：y(t) = C·e^{At}·B，在均匀网格上用精确递推；D·δ(t) 按 D-4 去掉并返回 `direct_feedthrough` + `W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED`。
- 离散：`dt` MUST 等于 `ts`（否则 `E.VALIDATION.GRID`）；幅值按 D-3（默认 unit_area，u₀ = 1/ts）。
- trust：终点一致性检查用 Φ(t_end)·B 一次性计算与逐步递推比较；不稳定系统给 `W.DOMAIN.UNSTABLE_SYSTEM`。
- 输出 `{t, y, n_grid_points, direct_feedthrough}`，y 布局 `[ny, nu, nt]` row-major（与 `step` 相同）。

**Manifest** — `examples/control.lti.impulse/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.lti.impulse",
  "version": "0.3.0",
  "title": "Impulse response",
  "description": "计算 LTI 系统在均匀时间网格上的脉冲响应（每个输入通道分别施加）。",
  "category": "analysis",
  "tags": ["time-domain", "lti"],
  "applicability": {
    "preconditions": ["system 为 proper", "uniform 网格同 control.lti.step：t_k = k·dt，N = floor(t_final/dt + 1e-9) + 1", "离散系统要求 dt = ts"],
    "not_for": ["任意输入（将来用 control.lti.lsim）", "带时滞的系统"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "连续：y(t) = C·e^{At}·B（t ≥ 0），由 x_0 = B[:,j]、x_{k+1} = Φx_k 递推，Φ = expm(A·dt)（Padé-13 + scaling & squaring），采样值无离散化误差；D ≠ 0 时 t=0 处的 D·δ(t) 无法用采样值表示，不计入 y，单独在 direct_feedthrough 中返回并给出 W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED。离散：缺省 unit_area（u_0 = 1/ts，与 python-control 及 MATLAB impulse 相同），可选 unit_pulse（u_0 = 1）。tf/zpk 先转为可控标准型",
    "references": [
      {
        "type": "paper",
        "cite": "C. Moler, C. Van Loan, Nineteen dubious ways to compute the exponential of a matrix, twenty-five years later, SIAM Review 45(1), 2003",
        "doi": "10.1137/S00361445024180"
      },
      {
        "type": "paper",
        "cite": "N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005",
        "doi": "10.1137/04061101X"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "step::simulate_impulse (expm Padé-13 + recursion)",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/system/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/C": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/system/D": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_grid_points": 10000000,
    "max_output_values": 1000000
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.lti.impulse/0.3.0/credentials",
  "errors": ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE", "E.VALIDATION.ZERO_DENOMINATOR", "E.VALIDATION.NOT_CONJUGATE", "…(+10)…", "E.RESOURCE.COMPUTE_LIMIT", "E.RESOURCE.OUTPUT_TOO_LARGE"],
  "warnings": ["W.DOMAIN.UNSTABLE_SYSTEM", "W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED", "W.ACCURACY.GRID_TOO_COARSE", "W.ACCURACY.AUTO_GRID", "W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_lti_impulse",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:impulse:zoh-expm-pade13-recursion",
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "convention",
        "statement": "python-control 冲激约定（连续 x0 = B、忽略 D；离散按 discrete_scaling）",
        "enforced_by": "validation"
      },
      {
        "id": "grid",
        "statement": "时间网格足以分辨动态",
        "enforced_by": "warning:W.ACCURACY.GRID_TOO_COARSE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": ["all_finite"]
  }
}
```

**请求**（G(s) = 4/(s² + 1.2s + 4)，0–10 s，dt = 0.05；手算 y(t) = (4/ωd)·e^{−0.6t}·sin(ωd·t)，ωd = 1.907878）

```json
{
  "system": {
    "kind": "tf",
    "num": [
      4.0
    ],
    "den": [
      1.0,
      1.2,
      4.0
    ],
    "ts": 0.0,
    "time_unit": "s"
  },
  "time": {
    "mode": "uniform",
    "t_final": 10.0,
    "dt": 0.05,
    "unit": "s"
  },
  "exec": {
    "request_id": "req-impulse-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.lti.impulse",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-impulse-001",
  "result": {
    "t": {
      "unit": "s",
      "value": {
        "dtype": "f64",
        "shape": [201],
        "data": [0.0, 0.05, 0.1, 0.15000000000000002, 0.2, 0.25, "…(+193)…", 9.950000000000001, 10.0]
      }
    },
    "y": {
      "unit": "1",
      "value": {
        "dtype": "f64",
        "shape": [1, 1, 201],
        "data": [0.0, 0.1937948721398335, 0.3744246205637218, 0.540904206522726, 0.6924431812722138, 0.828442019666579, "…(+193)…", 0.0007145638600934523, 0.0011808649160704012]
      }
    },
    "n_grid_points": 201,
    "direct_feedthrough": null
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "final_state_rel_discrepancy": 8.35226966550217e-15,
      "definition": "‖x_{N−1}(逐步递推) − x(t_end)(一次性 expm 直接计算)‖₂ / ‖x(t_end)‖₂，取所有输入通道最大值"
    },
    "condition": {
      "norm1_A_dt": 0.2,
      "expm_squarings": 0,
      "realization": "controllable canonical form (scipy tf2ss convention)",
      "max_real_pole": -0.6
    },
    "error_estimate": {
      "y_abs": 2.996061599777966e-14,
      "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。非严格上界",
      "info_time_metrics_abs_s": 0.05,
      "info_basis": "指标按网格点定义（首个越过阈值的网格点，不插值），相对连续时间真值误差 ≤ dt"
    },
    "checks": {
      "asymptotically_stable": true,
      "all_finite": true,
      "auto_grid": false
    },
    "accuracy_grade": "B",
    "credentials": null,
    "derived": {
      "dcgain": {
        "dtype": "f64",
        "shape": [1, 1],
        "data": [1.0]
      },
      "poles": {
        "dtype": "c128",
        "shape": [2],
        "data": [[-0.6, 1.9078784028338913], [-0.6, -1.9078784028338913]]
      },
      "dt": 0.05,
      "t_end": 10.0
    }
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:impulse:zoh-expm-pade13-recursion",
      "routine": "step::simulate_impulse (expm Padé-13 + recursion)",
      "provenance": "aeoi://operators/control.lti.impulse/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "lti_model",
        "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）",
        "enforced_by": "caller"
      },
      {
        "id": "proper",
        "statement": "系统为 proper（deg num ≤ deg den）",
        "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"
      },
      {
        "id": "finite",
        "statement": "所有输入数值有限",
        "enforced_by": "error:E.VALIDATION.NON_FINITE"
      },
      {
        "id": "convention",
        "statement": "python-control 冲激约定（连续 x0 = B、忽略 D；离散按 discrete_scaling）",
        "enforced_by": "validation"
      },
      {
        "id": "grid",
        "statement": "时间网格足以分辨动态",
        "enforced_by": "warning:W.ACCURACY.GRID_TOO_COARSE"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "y_abs": 2.996061599777966e-14
      },
      "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；expm 后向误差 ≤ u（Higham 2005）。非严格上界"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "all_finite": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:026b3084504eeb82e6c9705ecbc125acaf7290b0d45685cdde40d0ba82666a24",
      "result": "sha256:0708f7c4ef466cfa35085fffdeb276db84841cadb741b7a87f3523524348e05c",
      "output_hash": "sha256:4e5809cdfc1b534de7c183757528866f09ccf54c47e93b312d73eb3fb7ae7b75",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.lti.impulse@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:312dc186259c886b6bfbc901688831202c4b3147d9c6ad84ec1c961055103119",
    "output_hash": "sha256:4e5809cdfc1b534de7c183757528866f09ccf54c47e93b312d73eb3fb7ae7b75",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.21312899999999999
  }
}
```

**错误请求与响应**（离散系统 ts = 0.1 但 dt = 0.05）

```json
{
  "system": {
    "kind": "tf",
    "num": [0.5],
    "den": [1.0, -0.5],
    "ts": 0.1,
    "time_unit": "s"
  },
  "time": {
    "mode": "uniform",
    "t_final": 1.0,
    "dt": 0.05,
    "unit": "s"
  },
  "exec": {
    "request_id": "req-impulse-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.lti.impulse",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-impulse-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.VALIDATION.GRID",
    "category": "validation",
    "retryable": true,
    "message": "离散系统要求 dt 等于采样周期 ts = 0.1 s，得到 dt = 0.05 s。",
    "agent_hint": "把 dt 设为 ts。",
    "details": {
      "path": "/time/dt"
    },
    "remediation": [
      {
        "id": "dt_eq_ts",
        "kind": "modify_input",
        "description": "dt 设为 ts",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/time/dt",
            "value": 0.1
          }
        ]
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.087478
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A 的 MIMO ss），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.lti.impulse/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.lti.impulse/parity.yaml`

```yaml
# Parity test spec — control.lti.impulse 0.3.0 (AEOI v0.3-draft)
operator: control.lti.impulse
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.lti.impulse/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "y": "max|Δy|/max|y_ref| ≤ 1e-10"
case_families:   # n = golden cases
  - {name: direct_feedthrough, n: 2, expect_warning: ['W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED']}
  - {name: discrete_dt_mismatch, n: 1, expect: E.VALIDATION.GRID}
  - {name: discrete_unit_area, n: 6}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: mimo_ss, n: 8}
  - {name: siso_tf, n: 10}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.10 `control.math.lyap`（v0.3 新增）

连续/离散 Lyapunov 方程（O-3 约定）。

语义要点：
- Bartels–Stewart：A 的实 Schur 分解（带迭代上限与确定性回退的 `schur_robust`）→ 复 Schur → 逐列回代。Q 对称时解对称化（对称化前的不对称度写入 trust）；Q 非对称也可解。
- 奇异判据与尺度无关（X-4a）：连续 min|λᵢ + λ̄ⱼ| ≤ 10³·u·‖A‖，离散 min|λᵢλ̄ⱼ − 1| ≤ 10³·u·max(1, ‖A‖)² → `E.DOMAIN.LYAP_SINGULAR`（不是数值问题，调参无效）。Schur 迭代不收敛 → `E.NUMERICAL.NO_CONVERGENCE`。
- trust.residual：相对残差（连续 ‖AX + XAᵀ + Q‖_F/(2‖A‖_F‖X‖_F + ‖Q‖_F)，离散类似）与 Schur 后向误差；error_estimate = ‖残差‖/sep_min（非严格上界：A 非正规时偏乐观）。
- A 不稳定但方程非奇异时照常求解（Lyapunov 方程本身有唯一解），trust.checks.A_stable = false。
- **（0.4.0，v0.3.2）迭代精化**：启发式前向误差估计 ‖R‖_F/(sep_min·‖X‖_F)（R 为 f64 残差）> 1e-12 时（方程接近奇异，如离散 ρ(A) → 1、连续特征值接近虚轴）触发：用补偿算术（Dot2；离散的 A X Aᵀ 先把 A X 保留为未求值的 hi + lo 和）计算残差 R(X)，复用同一 Schur 分解解 δ，X ← X + δ，至多 4 步（`limits.max_refinement_steps`），‖δ‖_F ≤ 2u‖X‖_F 或修正量未减半、补偿残差不降即停。trust.convergence 在触发时为 `applicable: true` 并给出步数与最后修正量，`residual.abs_fro_compensated` 给出补偿残差，error_estimate 改用补偿残差。未触发时输出与 0.3.0 逐位相同。50 位真值上（`golden/mpmath/control.math.lyap/0.4.0`，12 例：离散 ρ = 1 − 1e-4 … 1 − 1e-8，连续 max Re λ = −1e-4 … −1e-8·‖A‖）最大误差由 0.3.0 的 8.2e-8 降到 6.7e-15（scipy 6.2e-9）；示例 `request.discrete_near_unit.json`（A = [[p, −q], [q, p]]，p = 1 − 2⁻²⁰，q = 2⁻¹⁰，X = I/(2⁻²⁰ − 2⁻⁴⁰) 精确）逐位等于解析解。

**Manifest** — `examples/control.math.lyap/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.math.lyap",
  "version": "0.4.0",
  "title": "Lyapunov equation",
  "description": "求解连续 Lyapunov 方程 A X + X Aᵀ + Q = 0 或离散 Lyapunov 方程 A X Aᵀ − X + Q = 0。",
  "category": "utility",
  "tags": ["matrix-equation", "lyapunov"],
  "applicability": {
    "preconditions": ["A 方阵，Q 同形", "连续：λ_i + λ̄_j ≠ 0（对所有特征值对）；离散：λ_i·λ̄_j ≠ 1"],
    "not_for": ["Sylvester 方程（v0.3 未暴露）", "大规模稀疏（低秩 ADI 类方法）"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "Bartels–Stewart：复 Schur 分解 A = U T Uᴴ（T 上三角），Q̃ = Uᴴ Q U；连续逐列（j 从 n−1 到 0）解 (T + t̄_jj I) y_j = −q̃_j − Σ_{k>j} t̄_jk y_k，离散解 (t̄_jj T − I) y_j = −q̃_j − T Σ_{k>j} t̄_jk y_k，均为上三角回代；X = Re(U Y Uᴴ)。当启发式前向误差估计 ‖R‖_F/(sep_min·‖X‖_F) > 1e-12（方程接近奇异，如离散 ρ(A) → 1）时做迭代精化：用补偿算术（Dot2，Dekker 无 FMA 乘法，跨平台逐位相同）计算残差 R(X)，复用同一 Schur 分解解修正量 δ，X ← X + δ，至多 4 步，‖δ‖_F ≤ 2u‖X‖_F 或修正量未减半即停。可解性分离度 sep_min = min|λ_i+λ̄_j|（离散 min|λ_i λ̄_j − 1|）≤ 10³·u·‖A‖_F（离散 10³·u·max(1,‖A‖_F)²）时报 E.DOMAIN.LYAP_SINGULAR。Q 对称时输出对称化",
    "references": [
      {
        "type": "paper",
        "cite": "R. H. Bartels, G. W. Stewart, Solution of the matrix equation AX + XB = C, Comm. ACM 15(9), 1972",
        "doi": "10.1145/361573.361582"
      },
      {
        "type": "textbook",
        "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., 2013（SVD 数值秩、Schur 分解）"
      },
      {
        "type": "paper",
        "cite": "S. J. Hammarling, Numerical solution of the stable, non-negative definite Lyapunov equation, IMA J. Numer. Anal. 2(3), 1982",
        "doi": "10.1093/imanum/2.3.303"
      },
      {
        "type": "paper",
        "cite": "T. Ogita, S. M. Rump, S. Oishi, Accurate sum and dot product, SIAM J. Sci. Comput. 26(6), 2005",
        "doi": "10.1137/030601818"
      },
      {
        "type": "textbook",
        "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM, 2002（迭代精化，第 12 章）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "sysalg::lyap_refined (complex Schur Bartels–Stewart + compensated-residual iterative refinement)",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/Q": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 300,
    "max_refinement_steps": 4
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.math.lyap/0.4.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.DOMAIN.LYAP_SINGULAR",
    "E.NUMERICAL.NO_CONVERGENCE",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC"
  ],
  "warnings": ["W.DOMAIN.UNSTABLE_SYSTEM", "W.NUMERICAL.ILL_CONDITIONED", "W.NUMERICAL.LARGE_RESIDUAL", "W.VALIDATION.SYMMETRIZED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_math_lyap",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:lyap:complex-schur-bartels-stewart+dot2-refinement",
    "assumptions": [
      {
        "id": "nonsingular",
        "statement": "方程非奇异（连续 λᵢ + λ̄ⱼ ≠ 0，离散 λᵢλ̄ⱼ ≠ 1）",
        "enforced_by": "error:E.DOMAIN.LYAP_SINGULAR"
      },
      {
        "id": "convention",
        "statement": "方程约定 A X + X Aᵀ + Q = 0 / A X Aᵀ − X + Q = 0（不是 Aᵀ X + X A）",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": []
  }
}
```

**请求**（A = [[−1, 2], [0, −3]]（非对称），Q = I，连续）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      -1.0,
      2.0,
      0.0,
      -3.0
    ]
  },
  "Q": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1.0,
      0.0,
      0.0,
      1.0
    ]
  },
  "type": "continuous",
  "exec": {
    "request_id": "req-lyap-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.math.lyap",
    "version": "0.4.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-lyap-001",
  "result": {
    "X": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [0.6666666666666667, 0.08333333333333333, 0.08333333333333333, 0.16666666666666666]
    }
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "residual": {
      "abs_fro": 2.220446049250313e-16,
      "rel": 3.3482277612368947e-17,
      "abs_fro_compensated": null,
      "X_asymmetry_before_symmetrization_rel": 0.0,
      "schur_backward_rel": 0.0,
      "definition": "rel = ‖A X + X Aᵀ + Q‖_F / (2‖A‖_F‖X‖_F + ‖Q‖_F)"
    },
    "condition": {
      "separation_min": 2.0,
      "separation_rel": 0.5345224838248488,
      "definition": "min_{i,j}|λ_i + λ̄_j|"
    },
    "error_estimate": {
      "X_abs_fro": 1.1102230246251565e-16,
      "X_rel": 1.5923643861285e-16,
      "basis": "启发式：‖残差‖_F / sep_min；A 为正规矩阵时 sep_min 即真实分离度，非正规时真实 sep 可能更小（估计偏乐观）"
    },
    "checks": {
      "A_stable": true,
      "X_symmetric": true,
      "X_psd": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:lyap:complex-schur-bartels-stewart+dot2-refinement",
      "routine": "sysalg::lyap_refined (complex Schur Bartels–Stewart + compensated-residual iterative refinement)",
      "provenance": "aeoi://operators/control.math.lyap/0.4.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "nonsingular",
        "statement": "方程非奇异（连续 λᵢ + λ̄ⱼ ≠ 0，离散 λᵢλ̄ⱼ ≠ 1）",
        "enforced_by": "error:E.DOMAIN.LYAP_SINGULAR"
      },
      {
        "id": "convention",
        "statement": "方程约定 A X + X Aᵀ + Q = 0 / A X Aᵀ − X + Q = 0（不是 Aᵀ X + X A）",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "X_abs_fro": 1.1102230246251565e-16,
        "X_rel": 1.5923643861285e-16
      },
      "basis": "启发式：‖残差‖_F / sep_min；A 为正规矩阵时 sep_min 即真实分离度，非正规时真实 sep 可能更小（估计偏乐观）"
    },
    "evidence": {
      "claim": "unchecked",
      "postconditions": {},
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:b140ca8e6e9f777ee672f8c6343e5833c13cf645220489bd98a68fc318333bf6",
      "result": "sha256:e9031e4cd6e0611f037773ec30fa73522363086d56dfc456f67e7eed9c45488b",
      "output_hash": "sha256:9d4ad79f58f7e2b4020863e3410d440ad79dfc587df3347b4da5439babe82214",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.math.lyap@0.4.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:582fbd4cdf82b23004ba743e8cf72441306efd12241241640618d689077d5018",
    "output_hash": "sha256:9d4ad79f58f7e2b4020863e3410d440ad79dfc587df3347b4da5439babe82214",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.165096
  }
}
```

**错误请求与响应**（A 的特征值为 ±j：λ₁ + λ̄₂ = 0）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [2, 2],
    "data": [0.0, 1.0, -1.0, 0.0]
  },
  "Q": {
    "dtype": "f64",
    "shape": [2, 2],
    "data": [1.0, 0.0, 0.0, 1.0]
  },
  "type": "continuous",
  "exec": {
    "request_id": "req-lyap-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.math.lyap",
    "version": "0.4.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-lyap-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.LYAP_SINGULAR",
    "category": "domain",
    "retryable": false,
    "message": "连续 Lyapunov 方程奇异：A 存在特征值对满足 λ_i + λ̄_j ≈ 0（min|λ_i + λ̄_j| = 0.00e0），例如虚轴上的特征值或关于虚轴对称的特征值对。",
    "agent_hint": "检查 A 是否有位于稳定边界上的特征值（连续：虚轴；离散：单位圆）；若是 Gramian 计算，请先确认系统稳定。",
    "details": {
      "path": "/A",
      "separation_min": 0.0
    },
    "remediation": [
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认 A（含 row-major 顺序）与方程类型（continuous/discrete）",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.159306
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.math.lyap/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.math.lyap/parity.yaml`

```yaml
# Parity test spec — control.math.lyap 0.4.0 (AEOI v0.3-draft); python-control golden store generated for 0.3.0 and reused unchanged (V-3a)
operator: control.math.lyap
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.math.lyap/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: mpmath, version: "50 digits", role: high_precision_reference, status: active, generator: gen_golden_mpmath.py,
     golden_store: "golden/mpmath/control.math.lyap/0.4.0/*.json", runner: "cargo test --release -p aeoi-control --test parity_hp",
     compare: "rel_fro ≤ 1e-12, no relaxation", note: "ill-conditioned families from the tpt-control cross-check; NOT MATLAB, NOT python-control"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "X": "rel_fro ≤ max(1e-10, 10·trust.error_estimate.X_rel)"
case_families:   # n = golden cases
  - {name: continuous_nonsymmetric_Q, n: 3}
  - {name: continuous_spd, n: 12}
  - {name: discrete_spd, n: 10}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: singular_continuous, n: 1, expect: E.DOMAIN.LYAP_SINGULAR}
  - {name: singular_discrete, n: 1, expect: E.DOMAIN.LYAP_SINGULAR}
  - {name: unstable_solvable, n: 3, expect_warning: ['W.DOMAIN.UNSTABLE_SYSTEM']}
case_families_high_precision:   # golden/mpmath, n = cases
  - {name: continuous_near_axis, n: 3}
  - {name: discrete_rho_near_1, n: 8}
  - {name: discrete_rho_near_1_nonsymmetric_Q, n: 1}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.11 `control.design.place`（v0.3 新增）

状态反馈极点配置 u = −Kx。

语义要点：
- SISO：K 唯一（与 scipy/python-control 一致到舍入）。MIMO：KNV 方法 0 的复数版本——B = [U₀ U₁][Z; 0]，Sᵢ = null(U₁ᵀ(A − λᵢI))；实极点取实向量，共轭对成对更新，逐列扫描直到 Δln|det X̂| < 1e-6（`max_sweeps` 默认 50）；然后 M = XΛX⁻¹，K = Z⁻¹U₀ᵀ(A − M)。m = n（B 方阵可逆）时直接取酉矩阵 X（cond = 1）。
- 不可控 → `E.DOMAIN.NOT_CONTROLLABLE`（PBH）；期望极点重数 > m → `E.DOMAIN.POLE_MULTIPLICITY`；B 列不满秩 → `E.VALIDATION.RANK_DEFICIENT`；极点不共轭封闭 → `E.VALIDATION.NOT_CONJUGATE`。
- trust：达成极点与期望极点的最大相对偏差、闭环特征向量矩阵 cond(X)（鲁棒性指标）、‖K‖_F；偏差大时 `W.NUMERICAL.PLACEMENT_INACCURATE`，cond(X) 大时 `W.NUMERICAL.ILL_CONDITIONED`。
- parity 按 V-5a 比较；golden 中本实现的 cond(X) 最多为 scipy YT 方法的 1.08 倍。

**Manifest** — `examples/control.design.place/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.design.place",
  "version": "0.3.0",
  "title": "Pole placement",
  "description": "求状态反馈增益 K（u = −Kx）使 A − BK 的特征值为给定极点；多输入时用 KNV 方法 0 选择条件数较小的特征向量组（鲁棒配置）。",
  "category": "design",
  "tags": ["pole-placement", "state-feedback", "lti"],
  "applicability": {
    "preconditions": ["(A,B) 可控（PBH 判据）", "B 列满秩", "极点集合关于实轴共轭封闭（相对容差 1e-12）", "每个极点的重数 ≤ rank(B)"],
    "not_for": ["输出反馈配置", "离散/连续不区分（只看特征值）"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "B = [U0 U1]·[Z;0]（QR 补全为正交阵）；对每个极点 λ_i 求 S_i = null(U1ᵀ(A − λ_i I))（复 QR 补全）；特征向量 x_i ∈ span(S_i)。KNV 方法 0（Kautsky–Nichols–Van Dooren 1985）：逐个把 x_i 替换为 S_i 上与其余列正交补方向 y_i 最接近的单位向量（实极点限制为实向量：取 [Re SᵢᵀY, Im SᵢᵀY] 主方向；复共轭对用复向量，伙伴列取共轭），扫描直至 |det X|（列归一化）相对变化 < 1e-6 或达到最大扫描次数；然后 M = X Λ X⁻¹（取实部），K = Z⁻¹ U0ᵀ (A − M)。单输入时 S_i 一维，K 唯一（等价于 Ackermann 公式但数值更稳）。说明：scipy.signal.place_poles 缺省用 Tits–Yang，本实现用 KNV0 的复数推广，多输入时 K 不唯一，parity 只比较达成极点与（SISO 时）K",
    "references": [
      {
        "type": "paper",
        "cite": "J. Kautsky, N. K. Nichols, P. Van Dooren, Robust pole assignment in linear state feedback, Int. J. Control 41(5), 1985",
        "doi": "10.1080/0020718508961188"
      },
      {
        "type": "paper",
        "cite": "A. L. Tits, Y. Yang, Globally convergent algorithms for robust pole assignment by state feedback, IEEE TAC 41(10), 1996",
        "doi": "10.1109/9.539425"
      },
      {
        "type": "textbook",
        "cite": "T. Kailath, Linear Systems, Prentice-Hall 1980（实现、可控/可观、PBH 判据）"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "place::knv0",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 100,
    "max_sweeps": 50
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.design.place/0.3.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.DTYPE",
    "E.VALIDATION.NON_FINITE",
    "E.VALIDATION.SPARSE_STRUCTURE",
    "E.VALIDATION.DIGEST_MISMATCH",
    "E.VALIDATION.REF_UNSUPPORTED",
    "E.VALIDATION.REF_FORBIDDEN",
    "E.VALIDATION.NOT_CONJUGATE",
    "E.VALIDATION.RANK_DEFICIENT",
    "E.DOMAIN.NOT_CONTROLLABLE",
    "E.DOMAIN.POLE_MULTIPLICITY",
    "E.NUMERICAL.SINGULAR",
    "E.RESOURCE.INPUT_TOO_LARGE",
    "E.RESOURCE.TIMEOUT",
    "E.INTERNAL.PANIC"
  ],
  "warnings": ["W.NUMERICAL.PLACEMENT_INACCURATE", "W.NUMERICAL.ILL_CONDITIONED", "W.NUMERICAL.NEAR_UNCONTROLLABLE", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_design_place",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:place:knv0-complex",
    "assumptions": [
      {
        "id": "controllable",
        "statement": "(A,B) 可控",
        "enforced_by": "error:E.DOMAIN.NOT_CONTROLLABLE"
      },
      {
        "id": "multiplicity",
        "statement": "期望极点重数 ≤ rank(B)",
        "enforced_by": "error:E.DOMAIN.POLE_MULTIPLICITY"
      },
      {
        "id": "nonunique",
        "statement": "多输入时 K 不唯一：结果是满足极点配置的一个 K",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": ["poles_placed", "conjugate_closed"]
  }
}
```

**请求**（双积分器，期望极点 −1 ± j；手算 K = [2, 2]）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      0.0,
      1.0,
      0.0,
      0.0
    ]
  },
  "B": {
    "dtype": "f64",
    "shape": [
      2,
      1
    ],
    "data": [
      0.0,
      1.0
    ]
  },
  "poles": [
    [
      -1.0,
      1.0
    ],
    [
      -1.0,
      -1.0
    ]
  ],
  "exec": {
    "request_id": "req-place-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.design.place",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-place-001",
  "result": {
    "K": {
      "dtype": "f64",
      "shape": [1, 2],
      "data": [1.9999999999999996, 2.0]
    },
    "achieved_poles": {
      "dtype": "c128",
      "shape": [2],
      "data": [[-1.0, 0.9999999999999998], [-1.0, -0.9999999999999998]]
    }
  },
  "trust": {
    "convergence": {
      "applicable": false,
      "converged": true,
      "iterations": 0,
      "criterion": "一次扫描后 ln|det X̂|（列归一化）变化 < 1e-6",
      "final_value": -0.4054651081081644
    },
    "residual": {
      "pole_placement_max_rel": 1.570092458683775e-16,
      "M_imag_rel": 0.0,
      "definition": "eig(A−BK) 与期望极点最近匹配的最大相对差（以 max(1,|p|) 归一）；M_imag_rel = max|Im(XΛX⁻¹)|/‖·‖_F"
    },
    "condition": {
      "cond_X_eigvec": 2.618033988749896,
      "pbh_min_rel_sv": 0.7071067811865475,
      "K_norm_fro": 2.82842712474619,
      "inputs": 1
    },
    "error_estimate": {
      "poles_rel": 5.813203227122745e-16,
      "basis": "max(后验极点偏差, u·cond(X))；闭环极点的一阶扰动界 ∝ cond(X)·‖ΔA‖（Bauer–Fike）"
    },
    "checks": {
      "poles_placed": true,
      "conjugate_closed": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:place:knv0-complex",
      "routine": "place::knv0",
      "provenance": "aeoi://operators/control.design.place/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "controllable",
        "statement": "(A,B) 可控",
        "enforced_by": "error:E.DOMAIN.NOT_CONTROLLABLE"
      },
      {
        "id": "multiplicity",
        "statement": "期望极点重数 ≤ rank(B)",
        "enforced_by": "error:E.DOMAIN.POLE_MULTIPLICITY"
      },
      {
        "id": "nonunique",
        "statement": "多输入时 K 不唯一：结果是满足极点配置的一个 K",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "poles_rel": 5.813203227122745e-16
      },
      "basis": "max(后验极点偏差, u·cond(X))；闭环极点的一阶扰动界 ∝ cond(X)·‖ΔA‖（Bauer–Fike）"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "poles_placed": true,
        "conjugate_closed": true
      },
      "failed": [],
      "convergence": null,
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:03cb75e677cd411717fe432149eadcc60e9f3557180ce5f06503dc5519a53398",
      "result": "sha256:6d3bca4667f58d0ef11c17fa215f096ec0009402eb40a696eb265eb118b283b4",
      "output_hash": "sha256:8353eccf88599846e3653216dd6efce8df922a38183be90044a3afab0a59a59c",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.design.place@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:fc645b827a90ff0e3685068d7b789a332d038da91170e7c0fda6697b6c25516a",
    "output_hash": "sha256:8353eccf88599846e3653216dd6efce8df922a38183be90044a3afab0a59a59c",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.191034
  }
}
```

**错误请求与响应**（A = diag(1, −1)，B = [0, 1]ᵀ：λ = 1 不可控）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [2, 2],
    "data": [1.0, 0.0, 0.0, -1.0]
  },
  "B": {
    "dtype": "f64",
    "shape": [2, 1],
    "data": [0.0, 1.0]
  },
  "poles": [[-1.0, 0.0], [-2.0, 0.0]],
  "exec": {
    "request_id": "req-place-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.design.place",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-place-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.NOT_CONTROLLABLE",
    "category": "domain",
    "retryable": false,
    "message": "(A,B) 不可控：特征值 λ = 1.000000+0.000000j 对应的模态不受输入影响，无法任意配置极点。",
    "agent_hint": "这是模型结构问题：为该模态增加执行器（修改 B），或检查 A（含 row-major 顺序）。若只需镇定，可考虑 LQR。",
    "details": {
      "path": "/B",
      "uncontrollable_eigenvalue": [1.0, 0.0],
      "pbh_min_singular_value_rel": 0.0
    },
    "remediation": [
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认执行器配置与 A 矩阵",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.20441700000000002
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A，2 输入），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.design.place/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.design.place/parity.yaml`

```yaml
# Parity test spec — control.design.place 0.3.0 (AEOI v0.3-draft)
operator: control.design.place
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.design.place/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "achieved poles": "eig(A − B·K) recomputed by the test from the reference request; |Δ|/max(1,|p|) ≤ 1e-10·cond(X)·(1+‖K‖_F)"
  "K (SISO only; unique)": "rel_fro ≤ 1e-10·cond(X)·(1+‖K‖_F)"
  "K (MIMO)": "not compared (not unique); cond(X) vs scipy YT reported as information"
case_families:   # n = golden cases
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: mimo_ytt, n: 12}
  - {name: multiplicity, n: 1, expect: E.DOMAIN.POLE_MULTIPLICITY}
  - {name: not_conjugate, n: 1, expect: E.VALIDATION.NOT_CONJUGATE}
  - {name: siso, n: 12}
  - {name: uncontrollable, n: 1, expect: E.DOMAIN.NOT_CONTROLLABLE}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

### 10.12 `control.design.dlqr`（v0.3 新增）

离散 LQR：DARE 的稳定化解、增益与闭环极点（O-3 约定，支持交叉项 N）。

语义要点：
- 算法：结构保持倍增算法（SDA）求 DARE，再做至多 3 步 Newton–Hewer 修正（每步解一个离散 Lyapunov 方程，复用 `lyap` 的 Bartels–Stewart）。K = (R + BᵀSB)⁻¹(BᵀSA + Nᵀ)。
- 前置检查：对 |λ| ≥ 1 的模态做 PBH 可镇定性检查 → `E.DOMAIN.NOT_STABILIZABLE`；单位圆上不被 Q 观测的模态 → `E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG`；R 非正定 → `E.DOMAIN.R_NOT_POSITIVE_DEFINITE`。输入校验与 `lqr` 共用（对称化规则、`symmetry_tolerance`）。
- trust：DARE 相对残差与缩放残差、cond(S)、cond(R)、闭环谱半径、error_estimate = ‖残差‖/min|λᵢλ̄ⱼ − 1|（非严格上界）。
- parity 容差规则与 `lqr` 相同（V-6）。

**Manifest** — `examples/control.design.dlqr/manifest.json`

```json
{
  "spec": "aeoi/0.4",
  "id": "control.design.dlqr",
  "version": "0.3.0",
  "title": "Discrete-time LQR",
  "description": "求解离散代数 Riccati 方程 (DARE)，返回最优状态反馈增益 K（u_k=−Kx_k）、Riccati 解 S 与闭环极点，使 J=Σ(xᵀQx+uᵀRu+2xᵀNu) 最小。",
  "category": "design",
  "tags": ["optimal-control", "riccati", "lti", "discrete-time"],
  "applicability": {
    "preconditions": ["R 对称正定", "Q 对称；[[Q,N],[Nᵀ,R]] 对称半正定", "(A,B) 可镇定（|λ| ≥ 1 的模态可控）", "辛矩阵束在单位圆上无特征值（等价于 (Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在单位圆上无不可观模态）"],
    "not_for": ["连续系统（用 control.design.lqr）", "有约束的 MPC"],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "消去交叉项（Ā=A−BR⁻¹Nᵀ，Q̄=Q−NR⁻¹Nᵀ，G=BR⁻¹Bᵀ）后用结构保持倍增算法 SDA：A_{k+1}=A_k(I+G_kH_k)⁻¹A_k，G_{k+1}=G_k+A_k(I+G_kH_k)⁻¹G_kA_kᵀ，H_{k+1}=H_k+A_kᵀH_k(I+G_kH_k)⁻¹A_k，H_0=Q̄，H_k→S（二次收敛）；随后至多 3 步 Newton–Hewer 修正（闭环离散 Lyapunov 方程用 Bartels–Stewart 求解），残差不降即停。K=(R+BᵀSB)⁻¹(BᵀSA+Nᵀ)",
    "references": [
      {
        "type": "paper",
        "cite": "E. K.-W. Chu, H.-Y. Fan, W.-W. Lin, A structure-preserving doubling algorithm for continuous-time algebraic Riccati equations, Linear Algebra Appl. 396, 2005",
        "doi": "10.1016/j.laa.2004.10.010"
      },
      {
        "type": "paper",
        "cite": "G. A. Hewer, An iterative technique for the computation of the steady state gains for the discrete optimal regulator, IEEE TAC 16(4), 1971",
        "doi": "10.1109/TAC.1971.1099755"
      },
      {
        "type": "textbook",
        "cite": "P. Lancaster, L. Rodman, Algebraic Riccati Equations, Oxford 1995"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"
    }
  },
  "backend": {
    "library": "aeoi-control (pure Rust) + nalgebra 0.33",
    "routine": "dlqr::solve_dare (SDA + Newton–Hewer)",
    "version": "0.3.0",
    "binding": "native",
    "license": "MIT OR Apache-2.0"
  },
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "sparse_support": {
    "/A": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/B": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/Q": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/R": {
      "policy": "densify",
      "max_dense_elements": 1000000
    },
    "/N": {
      "policy": "densify",
      "max_dense_elements": 1000000
    }
  },
  "encodings": {
    "input": ["json", "b64", "ref"],
    "output": ["json", "b64"]
  },
  "limits": {
    "max_states": 200,
    "max_sda_iterations": 100
  },
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "L2"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "B"
  },
  "credentials": "aeoi://operators/control.design.dlqr/0.3.0/credentials",
  "errors": ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE", "E.VALIDATION.NOT_SYMMETRIC", "E.VALIDATION.SPARSE_STRUCTURE", "…(+10)…", "E.RESOURCE.TIMEOUT", "E.INTERNAL.PANIC"],
  "warnings": ["W.NUMERICAL.LARGE_RESIDUAL", "W.NUMERICAL.NEAR_UNCONTROLLABLE", "W.NUMERICAL.ILL_CONDITIONED", "W.VALIDATION.SYMMETRIZED", "W.PERFORMANCE.SPARSE_DENSIFIED"],
  "mcp": {
    "tool_name": "control_design_dlqr",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  "receipt": {
    "method_id": "aeoi:dlqr:sda+newton-hewer",
    "assumptions": [
      {
        "id": "weights",
        "statement": "Q 对称半正定、R 对称正定",
        "enforced_by": "error:E.DOMAIN.WEIGHT_NOT_PSD"
      },
      {
        "id": "stabilizable",
        "statement": "(A,B) 可镇定",
        "enforced_by": "error:E.DOMAIN.NOT_STABILIZABLE"
      },
      {
        "id": "no_unit_circle_unobservable",
        "statement": "单位圆上没有不被 Q 观测的模态",
        "enforced_by": "error:E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG"
      },
      {
        "id": "model",
        "statement": "离散模型 x[k+1] = A x[k] + B u[k] 与采样周期由调用方保证",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic"
    },
    "postconditions": ["S_psd", "closed_loop_stable"]
  }
}
```

**请求**（ZOH 离散双积分器（ts = 0.1），Q = I，R = 1）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1.0,
      0.1,
      0.0,
      1.0
    ]
  },
  "B": {
    "dtype": "f64",
    "shape": [
      2,
      1
    ],
    "data": [
      0.005,
      0.1
    ]
  },
  "Q": {
    "dtype": "f64",
    "shape": [
      2,
      2
    ],
    "data": [
      1.0,
      0.0,
      0.0,
      1.0
    ]
  },
  "R": {
    "dtype": "f64",
    "shape": [
      1,
      1
    ],
    "data": [
      1.0
    ]
  },
  "exec": {
    "request_id": "req-dlqr-001"
  }
}
```

**成功响应**（Rust 参考实现实际输出；与 `reference.python-control.json` 中的 python-control/scipy 值一致，见 11.3）

```json
{
  "spec": "aeoi/0.4",
  "status": "ok",
  "dry_run": false,
  "operator": {
    "id": "control.design.dlqr",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-dlqr-001",
  "result": {
    "K": {
      "dtype": "f64",
      "shape": [1, 2],
      "data": [0.9170745631140953, 1.6355961850466318]
    },
    "S": {
      "dtype": "f64",
      "shape": [2, 2],
      "data": [17.834931322188975, 10.012492197250403, 10.012492197250403, 17.85658646032884]
    },
    "closed_loop_poles": {
      "dtype": "c128",
      "shape": [2],
      "data": [[0.9159275043398831, 0.045853692377174946], [0.9159275043398831, -0.045853692377174946]]
    }
  },
  "trust": {
    "convergence": {
      "applicable": true,
      "converged": true,
      "iterations": 9,
      "criterion": "SDA：‖H_{k+1}−H_k‖_F/‖H_{k+1}‖_F ≤ 1e-15 或停滞",
      "final_value": 0.0,
      "newton_hewer_steps": 0,
      "sda_residual_fro": 2.0411201962889075e-15
    },
    "residual": {
      "dare_abs_fro": 2.0411201962889075e-15,
      "dare_rel": 7.053279481530721e-17,
      "dare_rel_scaled": 1.9366556644389042e-17,
      "definition": "dare_rel = ‖AᵀSA − S − (AᵀSB+N)(R+BᵀSB)⁻¹(BᵀSA+Nᵀ) + Q‖_F / ‖S‖_F；dare_rel_scaled 分母为 ‖Q‖+‖S‖(1+‖A‖²)+(‖A‖‖S‖‖B‖+‖N‖)²/‖R‖"
    },
    "condition": {
      "cond_R": 1.0,
      "cond_S": 3.5564061396358535,
      "pbh_min_sv_rel_unstable_modes": 0.06862231995825939,
      "closed_loop_spectral_radius": 0.9170745631140944,
      "unit_circle_unobservable_pbh_min": 0.4993761694389223,
      "sep_closed_loop_eig": 0.1589742456890928
    },
    "error_estimate": {
      "S_abs_fro": 1.283931360983302e-14,
      "S_rel": 4.436743480654644e-16,
      "basis": "启发式：‖Res‖_F / min|λ_iλ̄_j − 1|（λ ∈ eig(A_cl)），A_cl 非正规时偏乐观；非严格上界"
    },
    "checks": {
      "S_symmetric": true,
      "S_psd": true,
      "closed_loop_stable": true
    },
    "accuracy_grade": "B",
    "credentials": null
  },
  "receipt": {
    "format": "aeoi-receipt/1",
    "method": {
      "id": "aeoi:dlqr:sda+newton-hewer",
      "routine": "dlqr::solve_dare (SDA + Newton–Hewer)",
      "provenance": "aeoi://operators/control.design.dlqr/0.3.0/manifest#/provenance"
    },
    "assumptions": [
      {
        "id": "weights",
        "statement": "Q 对称半正定、R 对称正定",
        "enforced_by": "error:E.DOMAIN.WEIGHT_NOT_PSD"
      },
      {
        "id": "stabilizable",
        "statement": "(A,B) 可镇定",
        "enforced_by": "error:E.DOMAIN.NOT_STABILIZABLE"
      },
      {
        "id": "no_unit_circle_unobservable",
        "statement": "单位圆上没有不被 Q 观测的模态",
        "enforced_by": "error:E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG"
      },
      {
        "id": "model",
        "statement": "离散模型 x[k+1] = A x[k] + B u[k] 与采样周期由调用方保证",
        "enforced_by": "caller"
      }
    ],
    "exactness": {
      "class": "approximate",
      "bound_kind": "a_posteriori_heuristic",
      "bound": {
        "S_abs_fro": 1.283931360983302e-14,
        "S_rel": 4.436743480654644e-16
      },
      "basis": "启发式：‖Res‖_F / min|λ_iλ̄_j − 1|（λ ∈ eig(A_cl)），A_cl 非正规时偏乐观；非严格上界"
    },
    "evidence": {
      "claim": "verified",
      "postconditions": {
        "S_psd": true,
        "closed_loop_stable": true
      },
      "failed": [],
      "convergence": {
        "converged": true,
        "iterations": 9
      },
      "accuracy_grade": "B",
      "tolerance_class": "T2",
      "credentials": null
    },
    "fingerprint": {
      "request": "sha256:04936b75ccf9e7edde2f9867c0c1cd6c5f80f1a54cc3e180b3b5091e2253862c",
      "result": "sha256:2c77621ec891caf6d35954e83618ae7b950e818616ecb089f2cb93682df9b020",
      "output_hash": "sha256:2ba88e6c6340c1954eba51085c802ae5b720418c9edb3f5af707dd573a2aade3",
      "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
      "stable_across": "request: all platforms and builds of control.design.dlqr@0.3.0; result: same build + platform guaranteed (platform_bitwise, P-7), other platforms only when the result is bitwise equal"
    },
    "replay": {
      "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85",
      "platform": "x86_64-linux",
      "verify": {
        "mcp_tool": "aeoi_verify",
        "cli": "aeoi-call verify <operator> <request.json> <envelope.json>",
        "wasm": "verify(operator, request, envelope)"
      }
    }
  },
  "warnings": [],
  "error": null,
  "reproducibility": {
    "determinism": "platform_bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:63b84365b05506fa48c549a624cd87063f8141c372ff2032a82a610296214af1",
    "output_hash": "sha256:2ba88e6c6340c1954eba51085c802ae5b720418c9edb3f5af707dd573a2aade3",
    "platform": "x86_64-linux",
    "cpu_features": ["avx", "avx2", "fma", "avx512f"],
    "blas": "none (pure Rust nalgebra 0.33 + matrixmultiply, single-threaded)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.20335899999999998
  }
}
```

**错误请求与响应**（A = diag(1.2, 0.5)，B = [0, 1]ᵀ：不稳定模态 1.2 不可控）

```json
{
  "A": {
    "dtype": "f64",
    "shape": [2, 2],
    "data": [1.2, 0.0, 0.0, 0.5]
  },
  "B": {
    "dtype": "f64",
    "shape": [2, 1],
    "data": [0.0, 1.0]
  },
  "Q": {
    "dtype": "f64",
    "shape": [2, 2],
    "data": [1.0, 0.0, 0.0, 1.0]
  },
  "R": {
    "dtype": "f64",
    "shape": [1, 1],
    "data": [1.0]
  },
  "exec": {
    "request_id": "req-dlqr-002"
  }
}
```

```json
{
  "spec": "aeoi/0.4",
  "status": "error",
  "dry_run": false,
  "operator": {
    "id": "control.design.dlqr",
    "version": "0.3.0",
    "build": "aeoi-control 0.3.0+nogit nalgebra-0.33 rustc-1.85"
  },
  "request_id": "req-dlqr-002",
  "result": null,
  "trust": null,
  "receipt": null,
  "reproducibility": null,
  "error": {
    "code": "E.DOMAIN.NOT_STABILIZABLE",
    "category": "domain",
    "retryable": false,
    "message": "(A,B) 不可镇定：特征值 λ = 1.200000+0.000000j（|λ| ≥ 1）对应的模态不受输入影响，DARE 无稳定化解。",
    "agent_hint": "这是模型结构问题，调 Q/R 无效。需为该模态增加执行器（修改 B），或检查 A（含 row-major 顺序）。",
    "details": {
      "path": "/B",
      "uncontrollable_eigenvalue": [1.2, 0.0],
      "pbh_min_singular_value_rel": 0.0
    },
    "remediation": [
      {
        "id": "add_actuator",
        "kind": "modify_input",
        "description": "示例：为每个状态各加一路输入（B = I，R = I），仅用于确认问题出在执行器配置",
        "confidence": "low",
        "requires_user_confirmation": true,
        "patch": [
          {
            "op": "replace",
            "path": "/B",
            "value": {
              "dtype": "f64",
              "shape": [2, 2],
              "data": [1.0, 0.0, 0.0, 1.0]
            }
          },
          {
            "op": "replace",
            "path": "/R",
            "value": {
              "dtype": "f64",
              "shape": [2, 2],
              "data": [1.0, 0.0, 0.0, 1.0]
            }
          }
        ]
      },
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认 A 矩阵与执行器配置",
        "confidence": "high",
        "requires_user_confirmation": true
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.16165
  }
}
```

**非对称矩阵示例（内存序回归用例）** — `request.nonsymmetric.json`（非对称 A），响应见 `response.nonsymmetric.json`。Input/output schema 见 `examples/control.design.dlqr/input.schema.json`、`output.schema.json`。

**Parity 测试规格** — `examples/control.design.dlqr/parity.yaml`

```yaml
# Parity test spec — control.design.dlqr 0.3.0 (AEOI v0.3-draft)
operator: control.design.dlqr
references:
  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,
     generator: gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/control.design.dlqr/0.3.0/*.json",
     note: "python-control/scipy reference values, NOT MATLAB output"}
  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}
runner: "cargo test --release -p aeoi-control --test parity_b2"
tolerance_class: T2
compare:
  "S, K": "rel_fro ≤ max(1e-10·max(1, cond(S)/1e2), 10·trust.error_estimate.S_rel)  (same rule as lqr)"
  "closed-loop poles": "nearest match, κ_r-scaled with the same base tolerance"
case_families:   # n = golden cases
  - {name: R_indefinite, n: 1, expect: E.DOMAIN.R_NOT_POSITIVE_DEFINITE}
  - {name: cross_term, n: 5}
  - {name: double_integrator, n: 1}
  - {name: memory_order_nonsymmetric, n: 1, note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"}
  - {name: not_stabilizable, n: 1, expect: E.DOMAIN.NOT_STABILIZABLE}
  - {name: random, n: 15}
  - {name: unit_circle_unobservable, n: 1, expect: E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG}
pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"
```

## 10.13 组合、PID、阶跃指标与 Padé（v0.3.5）

规范性定义在 3.12。下面每个数都可以手算，完整信封在 `examples/<id>/`，由 `aeoi-call` 实际生成。参考值来自 python-control 0.10.2 或 mpmath 50 位，不是 MATLAB。

**串联** `control.lti.series`，容差 T1，等级 B。G₁ = 1/(s+1)，G₂ = 2/(s²+3s+2)：

- num = [2]，den = [1, 4, 5, 2]（即 2/((s+1)(s²+3s+2))）。`realization = polynomial_product`。

**并联** `control.lti.parallel`，T1，B。同一对系统：num = [1, 5, 4]，den = [1, 4, 5, 2]。

**反馈** `control.lti.feedback`，T2（含代数环求解；纯多项式例子远好于 1e-10），B。`sign` 默认 −1。1/(s+1) 的单位负反馈是 1/(s+2)。两个静态增益 1 在 `sign = +1` 下是 `E.DOMAIN.ALGEBRAIC_LOOP`。

**PID** `control.design.pid`，T1，B。parallel，Kp=1，Ki=2，Kd=3，Tf 省略（即 0）：num = [3, 1, 2]，den = [1, 0]，`proper = false`，警告 `W.DOMAIN.IMPROPER_RESULT`。这是理想微分，不是算错。standard 形式与 Ti、Td 混用 parallel 的 Ki 会得到 `E.VALIDATION.OPTION_CONFLICT`。不提供 pidtune（A-4）。

**阶跃指标** `control.lti.stepinfo`，轨迹 T2，时间量按网格定义（I-10），B。G = 1/(s+1)，t = 0, 0.5, …, 5：上升时间 2.0 s（0.5 → 2.5），2% 调节时间 4.0 s，超调 0，稳态 1。dt = 0.5 大于 0.1/|极点|，另有 `W.ACCURACY.GRID_TOO_COARSE`：采样值仍然是 ZOH 精确的，时间量的量化是 0.5 s。1/(s−1) 不稳定：上升/调节为 null，`window_limited = true`，峰值取窗口。1/(s+1) 只仿真到 1 s：`settling_time` 为 null 且 `W.DOMAIN.NOT_SETTLED`，不是 Inf。

**Padé** `control.lti.pade`，系数 T1，B。θ = 2、N = 2：num = [1, −3, 3]，den = [1, 3, 3]（首项已归一）。`approximation.bound` 为 null。`delay` 与 `order` 都没有默认值。

块状态空间公式（串联、并联、负反馈）与 python-control 0.10.2 的 `series` / `parallel` / `feedback` 在黄金用例上的矩阵差在 1e-15 量级；列优先展开的非对称 A 偏离超过容差的 1e9 倍。SISO 多项式与 mpmath 50 位闭式在 PID 上逐位相同，Padé 系数的相对差 ≤ 4e-16。

## 10.14 具名角色与 `control.analysis.loopsens`（v0.3.6）

规范性定义在 3.13。参考值来自 python-control 0.10.2，不是 MATLAB。

**具名反馈** `control.lti.feedback` 0.4.0：plant = 1/(s²+2s)，controller = (30s+60)/(s+15)，sensor = 1/(0.05s+1)：num = [1.5, 33, 60]，den = [0.05, 1.85, 18.5, 60, 60]，`composition.loop_role = "closed_loop"`。与 `feedback(sys1 = series(sys1 = 对象, sys2 = 控制器), sys2 = sensor)` 逐位相同。同时给 `plant` 和 `sys1` → `E.VALIDATION.OPTION_CONFLICT`。

**PID 冲突** `control.design.pid` 0.4.0：`{Kp: 1, Ki: 1, Ti: 2}`（form 省略 = parallel）→ `OPTION_CONFLICT`，`details.conflicting_fields = ["Ti"]`；补丁 `drop` 只有 `{"op": "remove", "path": "/Ti"}`（v0.3.5 还会删除并不存在的 `/Td`，套用补丁本身就会失败），另有 `ask_user`。因为 `Ki` 不属于 standard，这里不给“改模式”补丁；`{Kp: 2, Ti: 4}`（form 省略）给出的字段全属于 standard，才另给 `switch_form`（`add /form = "standard"`，confidence high）。`{form: standard, Kp: 2, Ti: 0}` → `OPTION_CONFLICT`，消息说明“没有积分项就省略 Ti”，补丁 `omit_integral`。

**一次性回路分析** `control.analysis.loopsens` 0.4.0，T3，等级 B。plant = 2/(s²+3s+2)，controller = (5s+10)/s，time = 0…10 s、dt = 0.01：

- `open_loop` = (10s+20)/(s³+3s²+2s)，`closed_loop` = (10s+20)/(s³+3s²+12s+20)，`closed_loop_stable = true`；
- 相位裕度 17.964°，增益裕度 Inf；`gain_crossover_rad_s` = 3.08423，`gain_crossover_hz` = 0.490871；
- `bandwidth_rad_s` = 4.82451，`bandwidth_hz` = 0.767845（−3 dB，`method = even_polynomial_roots`）；
- `step_metrics` 与对 T 单独调用 `stepinfo` 相同；
- claim `spot_checked`：恒等式是 `test_points`，裕度来自 `margin` 的网格复查（`sampled`，E-12）。

缺少 `plant` → `E.VALIDATION.SCHEMA`。12 个 golden 用例（含非单位反馈、只有对象、闭环不稳定、6 个随机 PID）上与 python-control 的最大相对差：系数 0、穿越频率/裕度/带宽 9.7e-13、阶跃指标 1.1e-12（`tests/parity_loopsens.rs`，`reports/parity-loopsens.json`）。

## 11. Rust 参考实现（v0.3）

### 11.1 结构

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

### 11.2 运行

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

### 11.3 测试结果（2026-11-23，v0.3.2，x86_64-linux，cpu_features avx/avx2/fma/avx512f）

全部通过（`cargo test --release`）：aeoi-core 单元测试 23 个，aeoi-control 单元测试 12 个，解析解测试 12 个（`analytic_b2.rs`：非方系统零点、Tustin 标量闭式解、离散 SM 的 Nyquist 端点、标量 DLQR = 黄金比例等；v0.3.2 新增 3 个，均在旧实现上失败：cond(R) = 1e12 解耦 CARE 的闭式解、尺度 ×1e3 / ×1e-3 的不可观虚轴模态仍报错、ρ = 1 − 2⁻²⁰ 的离散 Lyapunov 精确解），示例测试 5 个（12 个算子的示例请求 vs python-control/scipy 参考值与已发布响应的 `output_hash`），parity 测试 3 个（v0.2：264 例；v0.3：353 例；v0.3.2 高精度：28 例，`parity_hp.rs`，S/K/X 容差 1e-12，实测 lqr S、K ≤ 3.1e-19、lyap X ≤ 6.7e-15），MCP 端到端测试 1 个（12 个算子工具 + `aeoi_verify`，每个算子的成功与错误请求，以及对已发布示例信封的重放验证、篡改结果 → `result_tampered`、改输入 → `request_mismatch`），收据测试 3 个（v0.3.3，`receipt.rs`：全部 645 个 golden 请求的指纹、claim 与同机重放；跨平台容差比较、b64 编码无关性、比较规则），aeoi-bridge 测试 11 个（v0.3.3 +1：收据短形式与 bridge 重放验证）与 aeoi-web 测试 30 个（v0.3.1：schema 转换、工具桥与 F-5 来源核对、回答数值识别、Mock provider 驱动的 Agent 循环与评测打分、陷阱题判别性、HTTP 端点；v0.3.4 另有设计评测：10 道 EngDesign 参考答案、XG_02 部分分、`mock:design`、`max_steps`、单次请求/单题超时、429 日配额立即停止与重试预算）。独立 Python 校验器（`validate_examples.py`，v0.3.3 起独立重算收据指纹并核对 E-11 claim）180 项 + 392 个 golden 请求（含 28 个 mpmath）全部通过。

v0.2 算子的 parity（参考 = python-control 0.10.2 / scipy 1.18.1，**不是 MATLAB**）：

| 算子 | 用例 | 通过 | 量 | 实测最大误差 | 容差 |
|---|---|---|---|---|---|
| bode | 103 | 103 | 幅值（dB，绝对） | 2.8e-13 | 1e-9 |
| | | | 相位（deg，mod 360） | 3.0e-12 | 1e-8 |
| | | | 近极点 3 例（容差按自报估计放宽） | 幅值 7.1e-15 | 5.8e-4 |
| step | 89 | 89 | y（相对 max\|y\|） | 1.2e-11 | 1e-10 |
| | | | 指标时间（单位 dt） | 0（全部同一网格点） | 1 dt |
| | | | 峰值/调节值等（相对） | 1.2e-11 | 1e-9 |
| | | | 超调/欠调（百分点） | 3.2e-10 | 1e-6 |
| lqr | 72 | 72 | S（Frobenius 相对） | 6.9e-10（`lqr` 0.3.0；0.2.0 为 2.4e-6，cond(R)=10⁹ 用例）；不再需要 κ 缩放（0.2.0：4 例）。剩余差值是参考值本身的误差：50 位真值下本实现为 0，scipy 为 7.0e-10 | 1e-10·max(1,cond(S)/100)，或 10×自报估计 |
| | | | K（Frobenius 相对） | 1.1e-8（0.2.0：1.7e-6） | S 容差 × 10 |
| | | | 闭环极点（集合匹配，相对） | 5.1e-8（0.2.0：2.3e-5） | 1e-8·max(1,cond(S)/100) |
| | | | 不变量：缩放 CARE 残差 | 8.9e-17 | 1e-12 |

内存序回归（v0.2 三个算子）：column-major 错误请求与参考值偏差分别为容差的 5.6e10（bode）、1.3e10（step）、9.3e9（lqr）倍，均被检出。

v0.3 新增算子的 parity（`reports/parity-python-control-b2.json`；参考 = python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5，**不是 MATLAB**）。"基准容差"为未缩放的类容差；"κ 缩放"列为按 V-6 放宽过容差的比较次数（实际误差除 `place` 外都低于未缩放的基准容差）：

| 算子 | 用例 | 通过 | 量 | 实测最大误差 | 基准容差 | 最大 误差/容差 | κ 缩放 | 内存序检出倍数 |
|---|---|---|---|---|---|---|---|---|
| convert | 76 | 76 | tf→ss 矩阵（rel_fro） | 0（逐位相同） | 1e-12 | 7.4e-4 | 133 | 2.5e10 |
| | | | tf 系数 | 1.9e-14 | 1e-10 | | | |
| | | | zpk 根 / 增益 | 7.0e-14 / 7.4e-14 | 1e-10 | | | |
| poles_zeros | 47 | 47 | 极点 / 零点 | 2.9e-12 / 2.1e-12 | 1e-10 | 4.5e-5 | 165 | 1.8e10 |
| | | | DC 增益 | 8.3e-16 | 1e-10 | | | |
| | | | ωn / ζ | 2.9e-12 / 3.7e-14 | 1e-10 | | | |
| ctrb_obsv | 26 | 26 | 秩 | 0（全部一致） | 精确 | 1.1e-3 | 0 | 1.0e13 |
| | | | Krylov 矩阵 / 奇异值 | 5.0e-16 / 1.1e-15 | 1e-12 | | | |
| | | | Gramian | 9.1e-15 | 1e-10 | | | |
| margin | 45 | 45 | GM / 交越频率 | 1.9e-14 / 6.3e-15 | 1e-9 | 0.12 | 0 | 检出（交越集合不同） |
| | | | PM（deg） | 2.0e-13 | 1e-8 | | | |
| | | | SM / SM 频率（连续） | 4.4e-15 / 2.3e-15 | 1e-9 / 1e-6 | | | |
| | | | SM 频率（离散，V-10） | 1.2e-4 | 1e-3 | | | |
| c2d | 47 | 47 | 离散 ss 矩阵 / tf 系数 | 5.7e-16 / 2.7e-15 | 1e-10 | 1.4e-5 | 44 | 8.5e8 |
| impulse | 28 | 28 | y（相对 max\|y\|） | 3.0e-14 | 1e-10 | 3.0e-4 | 0 | 4.6e10 |
| lyap | 31 | 31 | X（rel_fro） | 1.6e-14 | 1e-10 | 1.6e-4 | 0 | 1.6e10 |
| place | 28 | 28 | 达成极点（V-5a） | 4.3e-10 | 1e-10（× cond(X)·(1+‖K‖)） | 1.2e-6 | 25 | 4.3e8 |
| | | | SISO K | 3.0e-13 | 同上 | | | |
| | | | cond(X) / scipy YT（信息） | 最大 1.08 | — | | | |
| dlqr | 25 | 25 | S / K | 6.7e-14 / 4.8e-14 | 1e-10 | 5.4e-4 | 15 | 2.8e10 |
| | | | 闭环极点 | 2.2e-14 | 1e-10 | | | |

v0.3 parity 测试在开发中发现并修正的问题：`c2d` tustin 的 Dd 多乘了 T（18 例失败）；`margin` 离散系统漏掉 Nyquist 端点处的稳定裕度极小值（6 例）；`poles_zeros` 对 C = 0 的退化系统返回了空零点集而不是报错；`place` 在 B 方阵时初始特征向量矩阵奇异；python-control 零点参考中的伪"无穷远零点"（V-9）。


**v0.3.6 复测（2026-11-23，同一台机器）**：`cargo test --release --workspace` 23 个测试目标、132 个测试全部通过（新增：`margin` 不挂住 2 个、K-1 角色 3 个、F-8 冲突 3 个、`loopsens` 单元 5 个、E-12/E-13 收据 3 个、绑定层 K-6/E-13/K-3 3 个、AL 预算 1 个、`loopsens` 示例与 parity 各 1 个）。`loopsens` 对 python-control 的 12 个 golden 用例见 10.14。收据（`reports/receipts.json`）：675 个 golden 成功用例、40 个错误用例；按 E-12，`margin` 44 例与 `loopsens` 11 例为 `spot_checked`，其余声明了后置条件的算子为 `verified`，没有 `unverified`。`validate_examples.py` 独立核对 claim、coverage 与 unreliable，全部通过。wasm 重放（`verify_examples.mjs`）由 CI 的 Pages 工作流执行（本机没有 wasm32 目标）。

### 11.4 已知限制

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

## 12. 开放问题与下一步

### 12.1 开放问题

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
15. ~~**（v0.4 新）线协议标识**~~ **已决定：升为 `aeoi/0.4`，核心 schema 的 `$id` 升为 `/0.4`。** 作者先决定保持 `aeoi/0.3`（理由是新增字段全部可选，与 v0.3.1、v0.3.5、v0.3.6 的做法一致），随后改为随版本升级。做法与 v0.3 相同（第 13 章 v0.3-draft 第 1 行）：`spec` 与 `$id` 一起升；`golden/` 不改（文件名是内容哈希，V-3；其中的 `spec` 记录生成时的版本，没有测试或脚本比较它）；`reports/` 是历史运行记录，不改。没有兼容层：v0.4 的实现只发 `aeoi/0.4`，是否同时接受 `aeoi/0.3` 的 manifest/信封留给消费者。`input_hash`、`fingerprint` 不含 `spec`，不受影响。
16. **（v0.4 新）`method_policy` 的细节**：~~`prefer` 是否保留~~ 已决定去掉（3.14 MS-5），以后若有“偏好但不强制”的需求再加，届时要同时定义回退顺序与收据里的回显。仍待定：字段名；`constraints` 还需要哪些键（例如对 `bound_kind` 的要求）；`METHOD_UNAVAILABLE` 归 `VALIDATION` 还是 `DOMAIN`；`receipt.alternatives` 是否要求 `backend` 与 `algorithm` 必填。
17. **（v0.4 新）内核级接入的接口**：数值内核如何挂到宿主（输入输出是否限于数值数组加元数据、错误如何跨 ABI 表达、内存归属、收据由哪一层生成）。需要先对照 `aeoi-core` 现有的 `validate`/`execute` 分层（8.2）再设计；它不是 8.3 的 `aeoi-capi`。
18. **（v0.4 新）宿主策略的表达与注入**：3.14 MS-10 规定了合并语义，但宿主策略从哪里来（配置文件、MCP 客户端、会话参数）、如何审计，属于绑定层设计，尚未写。
19. **（v0.4 新，一致性套件发现）`$ref` 约定**：manifest 的输入、输出 schema 用 `aeoi://schemas/common/0.4/Matrix` 引用通用类型，标准 JSON Schema 解析不了（应为 `aeoi://schemas/common/0.4#/$defs/Matrix`），3.9 也没有说明这种写法。要么改写所有 schema，要么在 3.9 写明约定。
20. **（v0.4 新，一致性套件发现）`lyap` 的 Q 对称性**：input schema 把 Q 描述为“对称”，10.10 的算法说明写“Q 非对称也可解”，二者要统一；`W.VALIDATION.SYMMETRIZED` 已在 manifest 声明，但规范没有规定触发条件（Q 非对称？结果被对称化？）。第二实现第一版因把非对称 Q 对称化，在 golden 的 4 个用例上失败。
21. **（v0.4 新，一致性套件发现）默认值没有用例**：`lyap` 的 `type` 默认 `continuous`，golden 里没有省略它的用例，F-7 的“默认值回显”也无人检验。套件需要“省略可选字段”的用例族。

### 12.2 下一步（建议顺序）

1. 在有授权的机器上运行 `matlab/gen_golden_bode.m`（并补 step/lqr 脚本），先核对 `memory_order_nonsymmetric` 用例与 python-control 一致，再跑通第一份 A 级凭证
2. ~~修复 `lqr` 病态 R 的精度差距~~（v0.3.2 用补偿 Newton–Kleinman 完成）；剩余：平衡 + 有序 Schur（或广义特征值）作为符号函数不收敛时的后备路径，以及 `dlqr` 的同类精化
3. ~~再接 5–7 个差异较大的控制算子~~（v0.3 已完成 9 个：convert、poles_zeros、ctrb_obsv、margin、c2d、impulse、lyap、place、dlqr）；剩余：`lsim`、`kalman`/`dlqe`、d2c、`minreal`、MIMO zpk、时滞裕度，以及为 v0.3 算子写 MATLAB 生成脚本
4. PyO3 绑定 + CI（跨平台 parity、重放、签名凭证）
5. Agent 回归评测：v0.3.1 已有 `aeoi-web eval`（29 题、4 个指标）；v0.3.4 另有 `aeoi-web design`。v0.3.5 实现了试点里排在前面的算子（串联、并联、反馈、PID 构造、stepinfo、Padé），v0.3.6 加了具名角色、`loopsens`（含带宽读数）和回合预算。下一步是按 A-4 写 `pidtune`（必须先声明对象类、指标和唯一性），以及控制器离散化和 MIMO 裕度；扩充问答题、加入"Agent + 裸 scipy"对照组。非线性吸引域仍不做。
6. 用 Tauri 打包 `aeoi-web` 为桌面应用（复用 axum 服务与静态页面）
7. 一致性套件（9.4）：v0.1 已有（运行器、`lyap` 的 Python 第二实现，44 个用例通过，7 种注入缺陷都被抓出）。接下来：在能编译 Rust 的机器上让参考实现自己先通过；给其余算子补比较规则（V-5、V-5a 的不变量）；再做 C 的内核级实现验证实现可替换；按 3.14 在参考实现里实现 `exec.method_policy`、`receipt.alternatives` 与 `method_selection`
8. 根据以上反馈修订，冻结 v1.0

## 13. Changelog

### v0.4-draft（2026-11-23）

起因：准备写第二个实现（先 Python，再 C 的数值内核）时发现两处规范是空的：一个算子版本只能有一个方法 id，没有办法登记另一种算法，用户也没有办法要求“只用经过审计的库”；换一个实现是否等价于参考实现，没有判定规则，`parity.yaml` 只说明参考实现怎么测自己。这一版的规范改动只是文本，参考实现、schema 与示例都没有动；另加了一致性套件 v0.1（`conformance/`，只覆盖 `lyap`，没有在参考实现上跑过）。线协议标识升为 `aeoi/0.4`（已决定，12.1 第 15 项）：`spec` 与核心 schema 的 `$id` 一起升，改动见下表第 7 行。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 2.1 M-11、3.14 MS-1 – MS-4 | 方法登记：manifest `receipt` 描述 `default` 方法，其他方法登记在可选的 `receipt.alternatives[]`；非参考实现的方法 id 不得用 `aeoi:` 前缀；每个方法各自通过 parity；增删方法与改 `default` 的版本影响 | 换算法或换内核不应该只能靠发布新算子版本 |
| 2 | 3.14 MS-5 – MS-9、4.3 R-9 | `exec.method_policy`（`require`、`constraints`，都是硬约束；没有软偏好），放在 `exec` 里所以不进入 `input_hash`；硬约束无解报错而不是静默换方法；收据 `method_selection` 只在请求带策略时出现；重放以 `require: [used]` 进行，verify 新增 verdict `method_unavailable` | 用户要能约束计算方式，同时保持 R-3 “同一个问题 / 同一个答案”两层指纹的分工 |
| 3 | 3.14 MS-10、MS-11 | 生效策略 = 宿主策略 ∩ 请求策略，请求只能收紧；LLM 绑定默认隐藏 `exec`，允许的方法集合写进工具描述 | 约束要对模型有约束力 |
| 4 | 5.1 | 新错误码 `E.VALIDATION.METHOD_UNAVAILABLE`（类别仍是 5 个） | MS-6 |
| 5 | 9.4 CF-1 – CF-12 | 跨实现一致性：线级与内核级两种形态（避开 9.1 的 L1 – L3 命名）；套件沿用 golden 与 `parity.yaml`；精确字段与不要求相同的字段（`ok` 与 `ok_with_warnings`、结果指纹、方法 id 不要求相同）；范数比较；第二实现自报的 `error_estimate` 不得放宽验收；`rel_fro` 与 R-5 的 ‖·‖∞ 并存的说明；收据与 manifest 的机械核查；逐方法检验；声明区分“同源通过”与“独立通过” | 让“换实现”可以被检验，而不是只被声称 |
| 6 | 12、附录 A | 新开放问题 14 – 21（19 – 21 由一致性套件的第一次运行发现）；下一步 7；附录 A 加 `spec-v0.4.md` 与 `conformance/` | |
| 7 | 0、2.2、4.2、12.1 第 15 项 | 线协议标识升为 `aeoi/0.4`，核心 schema `$id` 升为 `/0.4`（与 v0.3 的做法相同） | 作者决定：新增字段后版本号与线标识保持同步 |

**已改（只涉及标识）**：`schemas/*`（`$id`、`$ref`、`spec` 的 const）、`examples/**`（manifest 的 `spec`、示例响应、输入输出 schema 里对核心 schema 的引用）、`write_manifests.py`、`validate_examples.py`、`rust/`（`aeoi_core::SPEC`、`schema.rs` 两处单元测试字面量、`aeoi-mcp` 的 e2e 断言与说明文字）、`conformance/`。没有改：`golden/`、`reports/`、`gen_golden_*.py`（它们只记录生成当时的版本）。除 `validate_examples.py`（459 个 golden 请求与全部示例校验通过）和一致性套件（44 个用例、7 个变异测试）外，**Rust 没有重新编译或测试**。

**需要但尚未做的改动（参考实现与数据）**

| 位置 | 改动 |
|---|---|
| `schemas/common.schema.json` | `ExecOptions` 增加 `method_policy` |
| `schemas/manifest.schema.json` | `receipt` 增加可选的 `alternatives[]`（需确认 `receipt` 对象是否允许新增属性） |
| `schemas/envelope.schema.json` | `Receipt` 增加可选的 `method_selection` |
| `aeoi-core` | 方法解析（MS-6）、`method_selection`、`verify` 的 `require: [used]` 重放与 `method_unavailable` verdict |
| `aeoi-bridge` | 宿主策略合并（MS-10）、工具描述里的允许方法集合（MS-11） |
| `conformance/`、`aeoi-conformance` | 套件与运行器（9.4） |

### v0.3.6-draft（2026-11-23）

起因：EngDesign 7 题的真模型运行（`reports/design-live.md`，gemini-flash-lite-latest）。模型用对了算子，却在接口容易出错的地方出错：把控制器放进反馈通路、把闭环送进 `margin`；PID 的 `Ti: 0` 和混用字段连续报错；手抄、心算系统系数；读不到 plant×controller 的带宽；4 题回合用完没有提交；7 阶系统上一个失败的秩检查没有说明哪些字段不能用。线协议标识保持 `aeoi/0.3`；信封只增加可选字段（`receipt.evidence.coverage`、`unreliable`，claim 多一个取值 `spot_checked`）。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 3.13 K-1–K-3 | 组合算子接受具名角色 `plant` / `controller` / `sensor`，拓扑固定为单回路负反馈，`controller` 永远在前向通路；结果标 `composition.loop_role`（`open_loop` / `closed_loop`）与 `topology`；与位置参数混用报 `OPTION_CONFLICT`；工具描述必须写明哪个是 L、哪个是 T | XG_13：`feedback(sys1=对象, sys2=控制器)` 把控制器放进了反馈通路；Ziheng_03 把 T 送进 `margin` |
| 2 | 3.13 K-4 | 新算子 `control.analysis.loopsens`：一次得到 L、T、S、闭环极点与稳定性、L 的裕度、带宽、T 的阶跃指标；必须等于串起原语的结果，子算子失败时该字段为 null 加 `W.DOMAIN.ANALYSIS_SKIPPED` | 一次调用就不会接错；也省回合 |
| 3 | 3.13 K-5 | 频率字段名带单位并两种都给：`gain_crossover_rad_s/_hz`、`bandwidth_rad_s/_hz`；带宽定义（drop_db、相对 DC 增益、闭环）必须回显；带宽算在 plant·controller 的闭环上 | XG_02：要求约 85 Hz 带宽，没有 plant×controller 的带宽读数，交了带宽 0 Hz 的增益还声称满足 |
| 4 | 3.13 K-6、F-6 | `LtiSystem` 永不抽样；系统提示词、工具描述必须要求原样传递上一步的系统 | XG_11 手拼分母丢了 169、Ziheng_03 心算展开错了对象，后面每一步都在算另一个系统 |
| 5 | 3.11 F-8 | 互斥选项由算子检查，统一 `E.VALIDATION.OPTION_CONFLICT`，`details.conflicting_fields`；补丁只删除请求里出现的字段；`Ti = 0` 给一句说明原因的话和 `omit_integral` 补丁；只属于某模式的字段在描述里标明 | XG_12 前 4 步耗在 PID 字段冲突与 “less than or equal to the minimum of 0”；XG_02 也因 `Ti=0` 报错一次；旧补丁还会删除不存在的 `/Td` |
| 6 | 4.1 E-11/E-12、R-4、R-7 | manifest `receipt.coverage`：`full` / `test_points` / `sampled`；有 `sampled` 的全通过结果 claim 为 `spot_checked`；绑定层的抽样视图不是证据；短收据带 `coverage` 与 `result_view` | 网格复查与抽样视图只能说明“没看到问题”，不能说“已验证” |
| 7 | 4.1 E-13 | manifest `receipt.guards`；检查失败时收据 `evidence.unreliable` 列出不可用的结果字段，警告与短收据同步，系统提示词禁止引用 | XG_02（7 阶）：`pbh_agrees_ctrb` 为 false，但没有说明 rank / minimal 不能用，模型看到的还是抽样后的矩阵 |
| 8 | 8.4 AL-1–AL-3 | 有上限的提交式循环必须告诉模型上限；剩 2 回合时提示，最后一回合只提供提交工具；绑定层不替模型提交，报告记录 `budget_notices` | 7 题里 4 题 10 回合内没有提交 |
| 9 | 7.1 V-3a | 只增加可选字段、旧请求结果逐位不变的 MINOR 升级可以沿用旧 golden 目录 | `series`/`feedback` 0.4.0 的位置参数结果与 0.3.0 相同 |
| 10 | §0 | 写明与 MATLAB 的关系：只沿用公开函数名与分类；所有参考数据来自开源实现，没有 MATLAB 生成的数据 | 避免读者以为有 MATLAB 对照数据 |
| 11 | 10.14、12 | 新示例；开放问题 6、7 更新，新增 13（`test_points` 的边界） | |

`pidtune` 仍不做（A-4）。

**参考实现改动**

| 模块 | 改动 |
|---|---|
| `aeoi-control` | `series`/`feedback`/`pid` 升到 0.4.0（G-2 MINOR）：具名角色与冲突检查；PID 冲突补丁只删出现的字段、`Ti = 0` 专门报错；新算子 `loopsens` 0.4.0（内部调用 series/feedback/margin/stepinfo；连续系统带宽用 ω² 偶多项式求根）。位置参数调用的结果与 `output_hash` 不变 |
| `aeoi-core` | `receipt::claim_of`；`coverage` → `spot_checked`；`guards` → `evidence.unreliable`；envelope / manifest schema 增加可选字段 |
| `write_manifests.py` | 新的角色描述、L/T 说明、K-6 句子、coverage 与 guards；清理来源说明的 MATLAB 措辞 |
| `aeoi-bridge` | `LtiSystem` 从不抽样；短收据加 `coverage`、`unreliable_fields`、`result_view`；系统提示词：具名角色、`loopsens`、带宽单位、原样传递、`spot_checked` |
| `aeoi-web design` | 评测提示词改用 `loopsens`；回合预算（AL-1–AL-3），报告新增 `budget_notices` |
| 测试与数据 | 新单元测试（K-1 角色、F-8 冲突、E-12/E-13、K-6 不抽样、AL 预算）；`gen_golden_loopsens.py` 12 例与 `tests/parity_loopsens.rs`；`loopsens` 示例；`validate_examples.py` 独立核对 E-12/E-13，并跳过本就应该 schema 报错的示例请求；工具数 18 → 19（网页、CI、MCP 测试） |

`control.analysis.margin`：网格复查每 256 点检查一次协作式截止时间；新增 `tests/margin_no_hang.rs`（看门狗线程，含一个旧构建上挂住过的离散输入和 8 个极端系统）。`loopsens`：闭环不稳定时带宽为 null（K-5）。

真模型重跑（`reports/design-live-v036.md`，同一模型与设置、同 7 题）：通过 0/7 → 0/7，提交 3/7 → 7/7，工具错误 4 → 0，7 题平均分（未提交记 0）26.9 → 56.9；16 次 `loopsens` 调用全部角色正确，4 道题在回合预算提示后提交。仍失败的原因是模型推理（心算展开对象、辨识公式、回路成形）。

### v0.3.5-draft（2026-11-23）

起因：设计试点（`reports/design-pilot.md`）里，模型把步数花在手乘多项式上。要把串联、反馈、PID、阶跃指标和 Padé 写成算子，规范有五处写不干净：返回的系统怎么接到下一次调用、收据能不能嵌套、阈值默认值算不算“编造用户输入”、网格上的时间指标该用哪一档容差、近似误差和舍入误差是不是同一件事。线协议标识保持 `aeoi/0.3`。已发布 12 个算子的 `result` 与 `output_hash` 不变；`receipt.evidence.approximation` 只在近似算子上出现。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 新增 3.12（I-1–I-12、A-1–A-4）：`result.system` 可原样再输入；`composition` 带操作数哈希；收据不嵌套，`lineage` 只回显；采样周期必须相同；反馈方程与 `sign` 默认 −1；SISO 用多项式、含 ss 或 MIMO 用块公式且不对消；improper 操作数允许到给定相对阶，分析算子仍拒绝；代数环用尺度无关的 rcond | 否则“把控制器接到对象上”只能手算，或者靠一个规范里不存在的对象句柄 |
| 2 | F-7：`x-aeoi-provenance` 分为 `user`（引用核对）、`convention`（规范默认值，必须回显）、`supplied`（必填、无默认、不核对引用）。`_user_quote` 仍只覆盖 `user` | 阶跃阈值和 `sign = -1` 是定义，不是测量；Padé 的 θ 常常是算出来的，原话里没有这个数，硬要引用会把正确调用挡掉。阶次也没有安全默认，但不能因此去核对一句不存在的原话 |
| 3 | I-10–I-12：stepinfo 的时间量是网格定义，量化误差单独报告；不稳定与“窗口内没调节完”分开，后者不得写成 Inf | 把 ≤ dt 的定义差算进 T2，或者把没跑够的仿真说成永不调节，都会让下游误判 |
| 4 | A-1–A-3：近似算子必须给出 `approximation`（匹配阶、下一阶系数、写明网格的 certificate、没有证明时 bound 为 null）。收据只在有近似时多一个 `evidence.approximation`。Padé 用归一化后的 11.3.1 递推 | 未归一的乘积在 N ≳ 10 时会冲掉 f64 有效位；把逼近误差写进系数舍入界会让收据声称“算准了延迟” |
| 5 | A-4：不实现 pidtune。将来的整定算子必须声明对象类、指标和唯一性 | 没有这三条，整定只是某个实现的隐藏默认 |
| 6 | 新算子 `control.lti.series`、`control.lti.parallel`、`control.lti.feedback`（T1 / T1 / T2，B 级）、`control.design.pid`（T1，B）、`control.lti.stepinfo`（T2，时间量为网格定义，B）、`control.lti.pade`（T1 只约束系数，B）。无矩阵输入的 pid 与 pade 在 parity.yaml 声明 memory-order 不适用 | 见 `reports/design-pilot.md`：这四块挡住的设计题最多 |
| 7 | §0：memory-order 用例只要求输入里真有矩阵的算子 | 给 Padé 硬加一个矩阵参数并不能测出列优先错误 |
| 8 | 8.4 与 12.2：设计循环改用这些算子；`pidtune`、带宽、控制器离散化、MIMO 裕度留到下一步 | 试点的排序 |

### v0.3.4-draft（2026-11-23）

起因：问答评测只检查"会不会调用对的算子"。设计评测让模型迭代出一个控制器再提交，用独立评分器打分。线协议标识保持 `aeoi/0.3`，不新增算子，已发布示例的 `output_hash` 不变。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 8.4 增加设计评测：`aeoi-web design`、`submit_design`、EngDesign（MIT，原文入库）与 ControlEval（无许可证，运行时下载，不入库） | 问答对了不等于能设计 |
| 2 | 循环有单次请求时限、单题时限、429/5xx 重试预算；每日配额立即停止；报告在每题开始和每个模型回合后落盘。离线路径 `mock:design` 与 `mock:script` | 一次 ControlEval 试跑在空报告上停了约一小时：单次 HTTP 超时 180 秒再重试 6 次，报告却要等整道题结束才写 |
| 3 | XG_02 按细则给部分分（带宽 40、增益裕度 30、相位裕度 30）；非有限裕度写成 `"Inf"` / `"NaN"` 而不是 JSON null | 试点里一个稳定但带宽不对的控制器被打成 0 分；无穷相位裕度在报告里变成 null，看起来像缺字段，比较时却已经算通过 |
| 4 | 11 章、12.2 与 `reports/design-pilot.md`：7 道 EngDesign 试点的失败分类，以及按影响排序的缺失能力（串联/反馈、pid、闭环 stepinfo、Padé、带宽、控制器 c2d、MIMO 裕度）。这些算子不在本版实现 | 0/7 通过。多数是模型没交或交错，不是评分器把对的判错；缺的是让模型少做多项式手算的算子 |

### v0.3.3-draft（2026-11-23）

起因：让每个成功结果自带"凭什么可信"的机器可读证据，并能被任何人离线重放复核（想法借鉴 bicmath（MIT）的可验证结果与 matlab-mcp-extensions 的"没有证据不声称成功"，未复制代码）；同时补齐许可证。线协议标识保持 `aeoi/0.3`：`receipt` 是新增的可选信封字段（旧消费者忽略即可），`W.EVIDENCE.POSTCONDITION_FAILED` 是新增 warning；12 个算子的计算与 `result` 不变（所有示例 `output_hash` 不变），manifest 只增加描述性的 `receipt` 块，因此算子版本不变（同 v0.3.1 增加 description 的先例）。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 新增 4.3 结果收据（R-1 – R-8）、信封 `receipt`（`$defs/Receipt`）、manifest `receipt` 块；12 个算子都声明了方法 id、前提（含由谁保证）、精确性类与后置条件 | `trust` 只给数值诊断，没有说明前提由谁保证、结果属于哪个请求，也没法复核 |
| 2 | 两层指纹：request（跨平台、跨编码稳定）与 result（逐位相同才相同）；平台与 build 只写在 `replay` 中 | 原生与 wasm 只有约 3/4 的结果逐位相同；把平台放进指纹会使跨平台复核恒失败 |
| 3 | `verify` 重放比较（bitwise / 容差类 / 篡改 / 请求不符 / 错误码）；MCP `aeoi_verify`（C-6）、`aeoi-call verify`、wasm `verify`、网页"重放验证"按钮；CI 在 wasm 中重放全部原生示例信封 | 复核不依赖信任算子提供方 |
| 4 | E-11 没有证据不声称成功；E-7a 框架级码；系统提示词第 6 条与收据短形式（R-7） | 防止 Agent 把"算完了"说成"算对了"，以及把 `caller` 前提当作已检查 |
| 5 | 许可：代码 MIT OR Apache-2.0，规范与文档 CC-BY-4.0；`THIRD_PARTY_NOTICES.md` | 之前仓库没有许可文件 |

### v0.3.2-draft（2026-11-23）

起因：与 tpt-control 的交叉验证（`reports/parity-tpt-control.md`）发现 `lqr` 0.2.0 在 cond(R) = 1e12 时前向误差 3e-5，并把 7/10 个合法问题误报为 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`；离散 `lyap` 在 ρ(A) → 1 时误差 6e-9。线协议标识保持 `aeoi/0.3`；input schema 不变，output 只增加 trust 字段。按 G-2 精度改进为 MINOR：`control.design.lqr` 0.2.0 → 0.3.0，`control.math.lyap` 0.3.0 → 0.4.0；其他算子（含 `dlqr`）不变。

| # | 改动 | 原因 |
|---|---|---|
| 1 | `lqr` 虚轴检验在辛对角平衡后的 H̃ 上做，只作怀疑；由 PBH 不可观检验确认，或在求解后由闭环稳定裕度判定（10.3、X-4a 补充） | ‖H‖₁ 被 ‖BR⁻¹Bᵀ‖ 主导，阈值随 R 的尺度变化；复现 A = diag(−0.1, −1)、B = diag(0.1, 1)、Q = I、R = diag(1, 1e-12) 被误报 |
| 2 | `lqr` 的修正改为 K 形式 Newton–Kleinman 缺陷校正：Cholesky 求 K 并迭代精化、Dot2 补偿残差、Bartels–Stewart 解 ΔX，n ≤ 200、至多 20 步，按 ‖ΔX‖/‖X‖ 停止；输出 K 由 S 加未施加的修正量求得 | 原实现的 S·G·S 残差在 cond(R) 大时到舍入底；Kronecker 形式 O(n⁶) 只能用于 n ≤ 32 |
| 3 | `lyap` 0.4.0：近奇异时（‖R‖/(sep·‖X‖) > 1e-12）用补偿残差做至多 4 步迭代精化，复用 Schur 分解；trust.convergence 与 `residual.abs_fro_compensated` | 离散 ρ → 1、连续近虚轴时 f64 残差的舍入被 1/sep 放大 |
| 4 | 新增 V-3a（golden 的版本归属）与高精度参考引擎 `golden/mpmath/`：`gen_golden_mpmath.py` 生成 28 例（lqr 0.3.0：16，lyap 0.4.0：12），`parity_hp.rs` 用 1e-12 容差检验；`lqr` 0.3.0 / `lyap` 0.4.0 的 python-control golden 沿用 0.2.0 / 0.3.0 目录，parity.yaml 写明 | 病态用例上 scipy 本身的误差（最大 3.7e-9）大于本实现，不能再作这类用例的参考 |
| 5 | 新示例 `lqr/request.ill_conditioned_R.json`（复现例）与 `lyap/request.discrete_near_unit.json`（解析解 X = I/(2⁻²⁰ − 2⁻⁴⁰)） | 回归 |
| 6 | wasm/原生 parity：lqr 由 35/74 逐位相同（最大差 4.7e-6，1 例 warning 不一致）变为 91/91 逐位相同；CI 的 `AEOI_PARITY_ALLOW_MISMATCH` 由 1 改为 0 | 结果收敛到真值的 f64 舍入后，平台间舍入路径的差异消失 |

实测（50 位真值，`tools/hp_compare.py`，误差 = max|Δ|/max|ref|）：

| 族 | 用例 | 0.2.0 / 0.3.0 | 0.3.0 / 0.4.0 | scipy |
|---|---|---|---|---|
| lqr cond(R) 1e10–1e12 | 8 | 2.3e-4，3 例误报错 | ≤ 2.9e-16 | 3.7e-9 |
| lqr cond(R) 1e6–1e9（同 python-control 病态 R 用例） | 5 | 2.5e-6 | 0 | 3.2e-13 |
| lqr cond(R) 大 + 交叉项 N | 2 | 8.6e-7 | 0 | 1.1e-11 |
| lqr 复现例 | 1 | `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG` | 0 | — |
| lyap 离散 ρ = 1 − 1e-4 … 1 − 1e-8 | 8 | 8.2e-8 | 6.7e-15 | 6.2e-9 |
| lyap 连续近虚轴 | 3 | 1.8e-10 | 4.4e-15 | 7.5e-9 |
| lyap 离散非对称 Q | 1 | 1.0e-11 | 0 | 1.6e-12 |

python-control golden 上 `lqr` 的输出普遍有末位变化（修正后的 S、K 是真值的 f64 舍入），最大变化在病态用例上（向真值靠近）；已发布的 lqr success 示例 S、K 逐位不变；`lyap` 未触发精化的用例逐位不变。

### v0.3.1-draft（2026-11-22）

起因：新增 LLM 端到端测试台 `aeoi-web`（8.4），用 Gemini 函数调用实际驱动 12 个算子后，发现规范里有几处对 LLM 不够清楚。线协议标识保持 `aeoi/0.3`，所有改动向后兼容（只增加描述、改进错误详情）；已发布示例的成功响应与 `output_hash` 不变。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 新增 3.11 F-1：input schema 每个顶层属性 MUST 有 description（含义、维数、单位、默认值、枚举逐值说明）；12 个算子的 input schema 已补齐（如 `lqr.A` "状态矩阵 A，[n,n]"、`c2d.ts` "用户未给出时必须询问"、`lyap.type` 写出方程） | v0.3 的 `A`、`B`、`Q`、`R`、`system`、`method`、`type` 等没有描述；模型只能从字段名猜维数与方程约定 |
| 2 | F-2：`x-aeoi-shape-constraints` 等扩展关键字 MUST 同时写进 description | Gemini 的函数参数只接受 OpenAPI 子集，扩展关键字被丢弃，维数约束对模型不可见 |
| 3 | F-3：`E.VALIDATION.SCHEMA` 的 violations MUST 是叶子级；`oneOf` 失败按判别字段（`kind`/`mode`）或最少违规分支递归诊断；`message` 带首条违规（参考实现 `aeoi_core::schema::diagnose`） | 模型把 tf 的 `ts` 漏掉、把矩阵写成嵌套数组时，原错误只有"is not valid under any of the schemas listed in the 'oneOf' keyword"并附上整段输入，模型无从修正 |
| 4 | F-4：嵌套数组矩阵 → row-major DenseMatrix 的无损补丁（`modify_input`，`requires_user_confirmation: false`）；一维数组不自动补丁 | 嵌套数组是 LLM 最常见的矩阵写法；补丁可直接应用、数值不变；向量方向有歧义，不能代填 |
| 5 | F-5：有物理含义且无法安全默认的输入（`ts`、频率单位）MUST 必填，缺失时只给 `ask_user` | 评测中的"缺采样周期"陷阱：算子或绑定层若代填默认值，会把猜测包装成结果 |
| 6 | F-6 与 8.4：LLM 绑定层可收窄输入面（DenseMatrix only、隐藏 exec 等），MUST 公开有损映射；回传模型的长数组可抽样并标注 `_downsampled`，用户可见信封不变 | 给出 Gemini OpenAPI 子集的完整映射表；控制 token（上千点的时间响应原样回传会占用数千 token） |
| 7 | Agent 循环：模型返回空回合（如 Gemini `finishReason = MALFORMED_FUNCTION_CALL`）时，最多 2 次追加用户提示要求按 schema 重写调用或向用户提问，计入报告 `nudges_after_empty_turn` | 评测中 `gemini-3.1-flash-lite` 在 ctrb 题上一次生成了不合法的函数调用，原循环直接以空答案结束 |
| 8 | `ctrb_obsv.system` 描述写明：只判断可控性时 C、D 不影响结果，可取 C = [[1,0,…]]、D = [[0]] | 用户只给 A、B 时，模型因 ss 要求 C、D 而放弃调用 |
| 9 | 3.11 条款由 M-1 – M-6 改名为 F-1 – F-6 | 与第 2 章 manifest 规则 M-1 – M-6 重号 |
| 10 | F-5 改写为规范性的四条（a–d）：input schema 用 `x-aeoi-provenance: "user"` 标出不能假设的输入（`c2d.ts`），description 写明"先询问"；绑定层与系统提示词 MUST NOT 代填；绑定层 SHOULD 要求 `_user_quote` 引用并核对；新错误码 `E.VALIDATION.UNGROUNDED_INPUT`（绑定层，remediation 只有 `ask_user`） | 评测中 `gemini-3.1-flash-lite` 遇到缺采样周期的题时自行假设 Ts = 0.1 s 并给出"结果"。另外发现 `c2d.ts` 原有的短描述"采样周期，unit s"挡住了 `write_manifests.py` 里带"必须询问"的描述（只在没有描述时才补），这一句此前从未到达模型 |
| 11 | `ctrb_obsv` 的 manifest 描述以用途开头（"判断是否可控、是否可观……只给 A、B 时也用本工具"），system 描述给出补零的 C、D 的 DenseMatrix 写法；系统提示词的工具选择表同步 | `ctrb-01` 只给了 A、B，模型构造 C、D 时生成了不合法的函数调用 |
| 12 | 评测（8.4）：回答评分识别 LaTeX、unicode、`±`、分数与 `√`，容差取相对 2% 与印出精度半个末位中的较大者，去掉一律 1e-3 的绝对容差（会把 0.004 判成 0.0048）；澄清题要求最终消息是一个问题；新增 `aeoi-web rescore` 用当前评分器重算旧报告（重新执行记录的工具请求，不调用模型） | 第一轮的 4 道失败中有 2 道是评分器误判 |
| 13 | OpenRouter 免费模型的真实评测与每日额度识别（`free-models-per-day` → 立即停止并写 `aborted`） | 第一轮只用 Mock 测过 OpenAI 兼容 provider |
| 14 | 8.2 管线第 1 步、8.3 绑定表、第 11 章（结构、运行、测试、限制）、12.2 下一步（评测、Tauri）更新 | 反映 `aeoi-web` 与上述改动 |
| 15 | 新增 8.5 浏览器绑定与 8.3 表中一行：算子与工具桥编译为 WebAssembly，GitHub Pages 发布，BYOK（key 只在浏览器 `localStorage`，直接发给 provider） | 让没有 Rust 工具链的人也能试用测试台 |
| 16 | 参考实现拆出 `aeoi-bridge`（schema 转换、工具桥、provider 线格式、评分；无 tokio/reqwest），`aeoi-web` 与 `aeoi-wasm` 共用 | 保证浏览器版与本地版的函数声明、F-5 核对、系统提示词是同一份代码 |
| 17 | 2.1 确定性说明："平台"含 wasm32；wasm 构建的 `platform` 记为 `wasm32-unknown`（`aeoi_core::operator::platform()`，目标三元组无 OS 时补 `unknown`）；`platform_bitwise` 不延伸到 wasm ↔ 原生 | 实测 651 例中 66% 逐位相同，其余末几位不同（Rust `libm` vs glibc、无 AVX/FMA 内核） |
| 18 | 8.5：基于阈值的 warning 可能跨平台翻转；解不唯一的算子（多输入 `place`）跨平台比较 MUST 用不变量 | parity 中 1 个病态 `lqr` 用例的 `W.NUMERICAL.ILL_CONDITIONED` 只在原生出现；多输入 `place` 的 K 相差 2.4e-2 但闭环极点相同 |
| 19 | `aeoi-core` 用 `web-time` 代替 `std::time::Instant` | `Instant::now()` 在 `wasm32-unknown-unknown` 上 panic |
| 20 | Plotly 改为仓库内置（basic 包，MIT），本地版与浏览器版都不走 CDN | BYOK 页面不应加载第三方脚本；离线可用 |

### v0.3-draft（2026-11-21）

新增 9 个控制算子（10.4–10.12），实现过程中暴露的规范缺口与对应修订：

| # | 改动 | 原因 |
|---|---|---|
| 1 | spec 标识升为 `aeoi/0.3`，schema `$id` 升为 `/0.3`；新算子版本 0.3.0，v0.2 三个算子保持 0.2.0（行为与输出逐位不变，示例 `output_hash` 未变） | 新增错误码、输出约定与比较规则属实质性改动；信封结构未变 |
| 2 | 新增 3.10 多输出结果约定（R-1 – R-4）：具名字段、复数集合按集合比较、`"Inf"`/`"NaN"` 与 `null` 的区分、"代表值"必须同时返回全部候选 | `poles_zeros`、`margin`、`ctrb_obsv` 都有多个输出；`margin` 的 GM/PM 选择规则会让 Agent 误解，必须给出全部交越点 |
| 3 | 返回系统的算子用 `result.system`（完整 LtiSystem，可直接串联）；新增 `realization` 枚举；ss→tf 约定写明（R-5 – R-7） | 状态空间实现不唯一，不写明则 Agent 无法判断两个 ss 是否等价；首项系数的浮点残留曾产生伪前导 0 |
| 4 | zpk 在 v0.3 只表示 SISO；ss→zpk 的增益取 Markov 参数（R-8） | MIMO 的零极点表示需要逐元素或 Smith–McMillan 形式，v0.3 不定义 |
| 5 | 零点定义为不变零点；非满法秩 MUST 报 `E.DOMAIN.DEGENERATE_SYSTEM`，不得返回空集（R-9） | 初版对 C = 0 的系统返回空零点集，被 parity 用例发现——"没有零点"与"零点无定义"对 Agent 是完全不同的结论 |
| 6 | 离散时间约定 D-1 – D-6：频段、DC 增益在 z = 1、脉冲幅值 `discrete_scaling`、D·δ 的处理、c2d 公式、离散 margin 的搜索区间与端点 | v0.2 只有 `step` 一个离散相关约定；脉冲幅值（1/ts 还是 1）与 Nyquist 端点在不同软件间不一致，必须显式 |
| 7 | 选项枚举 O-1 – O-3：改变数学定义的选项必须是带默认值的枚举并回显；矛盾组合报 `E.VALIDATION.OPTION_CONFLICT`；方程符号写明 | `c2d` 的 `prewarp_frequency` 只对 tustin 有意义，静默忽略会给出错误模型；Lyapunov 方程的符号约定在 scipy 与 python-control 之间不同 |
| 8 | 登记新错误码 `OPTION_CONFLICT`、`RANK_DEFICIENT`、`MIMO_NOT_SUPPORTED`、`DEGENERATE_SYSTEM`、`NOT_CONTINUOUS`、`NOT_CONTROLLABLE`、`POLE_MULTIPLICITY`、`LYAP_SINGULAR`、`SYMPLECTIC_UNIT_CIRCLE_EIG` 与 11 个 warning（5.1） | 新算子的失败模式；全部是 DOMAIN/VALIDATION，告诉 Agent 调数值参数无效（X-1） |
| 9 | V-6 增加根、系数、DC 增益、离散化的条件数缩放规则 | 多项式根与由特征值展开的系数天生病态，固定 rtol 会在合法用例上误报，或者被迫放宽到失去意义 |
| 10 | 新增 V-5a：非唯一设计结果（MIMO `place` 的 K）用参考请求重算 eig(A − BK) 比较 | 初版只比较算子自报的达成极点，column-major 错误请求同样"配置成功"，内存序回归断言失败——这正是第 0 章警告的静默错误 |
| 11 | 新增 V-9（参考值后处理须写明）与 V-10（参考是局部优化结果时的比较规则） | python-control 无 slycot 时把无穷远零点返回为 ~1e17 的有限数；其离散稳定裕度来自 L-BFGS-B，自身只有 ~1e-4 的频率精度 |
| 12 | T2/T3 容差类的适用算子表扩充（7.2） | 覆盖新算子 |

实现中发现并修正的问题：

| # | 改动 | 原因（发现过程） |
|---|---|---|
| 13 | `c2d` tustin：Dd = D + ½·C·Bd（初版多乘了 T） | parity：D ≠ 0 的 18 个 tustin 用例偏差 ~0.1–1 |
| 14 | 离散 `margin` 的稳定裕度候选加入 θ = 0、π 端点（D-6） | parity：6 个离散用例 python-control 的最小值在 Nyquist 端点 |
| 15 | 实 Schur 分解改为带迭代上限与确定性回退的 `schur_robust`，`eigenvalues()` 与 `lyap` 共用 | nalgebra 0.33 的 QR 迭代在偶多项式的伴随矩阵上死循环（`margin` 调用挂起）；v0.2 算子示例输出逐位不变 |
| 16 | `place` 在 m = n 时直接取酉矩阵 X | 初版的初始向量选择使 X 奇异，1 个 MIMO 用例报 `E.NUMERICAL.SINGULAR` |
| 17 | `lyap` 的 Schur 不收敛映射为 `E.NUMERICAL.NO_CONVERGENCE`（不再与奇异混为一谈） | 区分 DOMAIN 与 NUMERICAL（X-1） |
| 18 | `impulse` 与 `step` 共用实现（`control.lti.impulse` 为独立 manifest），warning 文案按算子区分 | 网格、规模限制、ZOH 递推完全相同；`step_info` 文案不适用于冲激响应 |
| 19 | 示例目录新增 `reference.python-control.json`（示例请求的 python-control/scipy 参考值）；`regen_examples.py` 覆盖全部算子与全部 `request*.json` | 让每个示例都能独立核对，且 MCP 端到端测试按已发布响应的 `output_hash` 比对 |

### v0.2-draft（2026-11-21）

规范性改动（PART A，按需求补充）：

| # | 改动 | 原因 |
|---|---|---|
| 1 | spec 标识升为 `aeoi/0.2`，schema `$id` 升为 `/0.2`，三个算子升为 0.2.0 | 以下多项为破坏性改动（G-4） |
| 2 | 新增第 0 章醒目的内存序规则与 3.4 节；MATLAB 生成 MUST `reshape(A.',1,[])`；parity 集 MUST 含 `memory_order_nonsymmetric` 用例并断言 column-major 请求被检出；修正 `matlab/gen_golden_bode.m`（加 `aeoi_dense` + 往返自检 + 非对称 ss 用例） | v0.1 只说了 row-major，没有警告 MATLAB 侧的静默转置；对称矩阵用例无法发现此错误 |
| 3 | 新增 `SparseMatrix`（CSR/CSC）与校验规则（3.5）；manifest 新增 `sparse_support`（M-9）；新错误码 `E.VALIDATION.SPARSE_STRUCTURE`、warning `W.PERFORMANCE.SPARSE_DENSIFIED` | 用户要求；工程矩阵常为稀疏 |
| 4 | 新增 `DenseMatrixB64`、`MatrixRef` 与规模阈值（3.6）、安全规则（3.8）；manifest 新增 `encodings`、`limits`（M-10）；新错误码 `DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN`、`E.RESOURCE.INPUT_TOO_LARGE`；`exec.output_encoding` | 用户要求；v0.1 开放问题 1 |

实现中发现并修正的问题（PART B）：

| # | 改动 | 原因（发现过程） |
|---|---|---|
| 5 | v0.2 的 `ref` 只支持 data root 下的 `file://`（L-3、L-4） | 实现时确认 MCP resources 是 server→client 方向，客户端无法借此上传大输入；网络 URI 有 SSRF 风险 |
| 6 | 哈希改为对数组内容摘要计算（3.7、P-2） | 否则同一矩阵用 json/b64/ref 传入会得到不同 `input_hash`，重放与缓存失效 |
| 7 | 三个算子 `determinism` 由 `bitwise` 改为 `platform_bitwise`；"平台"定义含 CPU 特性；`reproducibility` 新增 `cpu_features`；P-3 允许做不到锁定内核时降级声明 | nalgebra 的 matrixmultiply 在运行时按 CPU 特性分派内核，跨机器不能保证逐位相同 |
| 8 | 后端由 SLICOT FFI 改为纯 Rust；`lqr` 算法由有序 Schur 改为矩阵符号函数 + Newton–Kleinman（10.3），manifest 的 provenance/backend 相应更新 | nalgebra 没有有序实 Schur 分解；保持纯 Rust、无 Fortran 依赖 |
| 9 | 阶跃指标明确为网格定义、不插值（10.2）；v0.1 示例的 info 数值作废 | v0.1 示例的 info 来自 python-control 自动网格，与请求的 dt = 0.01 网格不符 |
| 10 | `step` 规模错误拆分为 `COMPUTE_LIMIT` 与 `OUTPUT_TOO_LARGE`（X-4b） | v0.1 示例对 dt = 1e-7 给出"抽样返回"的 remediation，但抽样不减少计算量，是无效修复 |
| 11 | `call_operator` remediation MUST 引用已注册算子（X-9）；删除 v0.1 示例中的 `ctrb_decomp` | v0.1 示例引用了不存在的算子，Agent 照做会失败 |
| 12 | `bode` 显式频点 MUST 严格递增；相位约定明确（`branch_center_deg` + numpy 一致的 unwrap）；`Hz` 输入、`rad/s` 输出 | unwrap 结果依赖频点顺序；v0.1 开放问题 4 |
| 13 | 多项式前导零由报错改为去除 + warning（T-6） | 前导零常由上游数值计算产生，报错对 Agent 不友好 |
| 14 | 离散系统 `step` 要求 dt = ts；网格点数公式 N = floor(t_final/dt + 1e-9) + 1 | 避免 t_final/dt 的浮点舍入导致多/少一个点；离散系统没有"任意 dt"的定义 |
| 15 | golden 目录包含引擎名 `golden/<engine>/<op>/<ver>/`（V-3） | 防止把 python-control 数据误当作 MATLAB ground truth |
| 16 | `validate()` 返回解析后的 Input（8.2） | 解析（b64 解码、稀疏稠密化）与校验交织，分开会重复解码 |
| 17 | 信封新增必填 `dry_run` 与 `estimate`（E-9）；`trust`/`reproducibility` 在错误信封中可为 null（E-3） | v0.1 提到 dry_run 但信封中无字段表达；错误时不存在有意义的 trust |
| 18 | `trust.credentials` 在 L3 以下可为 null；三个算子当前精度等级 B | 尚无 MATLAB golden，不能声称 A 级 |
| 19 | parity 容差的两种放宽规则（V-6）：cond(S) 缩放与"自报估计 × 10"，且须单独统计 | 近极点频点、病态 R 用例在固定容差下不可能通过，需要有据可查的放宽规则 |
| 20 | 新增 warning `W.DOMAIN.ZERO_DC_GAIN`（零 DC 增益通道 info 为 null）及其余 v0.2 码（5.1 表） | python-control 对零 DC 增益通道的 step_info 会除零并给出无意义数值 |
| 21 | `step` 输出 `t`/`y` 可为 null（`return_samples: "none"`），新增 `n_grid_points` | 支持只要指标的调用，配合 `OUTPUT_TOO_LARGE` 的 remediation |
| 22 | 数值判据 MUST 与尺度无关（X-4a）；`lqr` 虚轴判据由 `1e-8·‖H‖₁` 改为 `10³·u·‖H‖₁`；cond(R) 告警阈值由 1e10 降为 1e8 | parity 测试中 3 个 cond(R) = 10⁸–10⁹ 的合法用例被误报为 `HAMILTONIAN_IMAG_AXIS_EIG`；实测 cond(R) ≥ 10⁸ 时精度已明显下降 |
| 23 | `trust.checks` 只允许布尔值（E-4），数值诊断移至 `residual` | 实现初版把对称性偏差数值放进 `checks`，被信封 schema 拒绝（示例测试发现） |
| 24 | 算子 MUST 在 manifest 声明所有 warning（E-7），并由测试断言所有实际返回的 code 已声明 | 实现中发现 `step` 会返回 manifest 未声明的 `E.VALIDATION.RANGE` 与 `W.DOMAIN.ZERO_DC_GAIN` |
| 25 | `LtiSystem` 的 `ts` 与 `time_unit` 必填 | 避免连续/离散的静默假设（与 U-4 一致） |
| 26 | MCP 映射：tool result 的 `content` 同时给摘要与信封 JSON 文本（E-8）；`_meta` 增加 `aeoi/spec`、`aeoi/manifest`；协议版本协商（C-5）；JSON-RPC 错误码约定（C-1） | 实现 `aeoi-mcp` 时确定的细节 |
| 27 | 绑定层：`aeoi-mcp` 为手写最小 stdio 实现，不依赖 `rmcp`（8.3） | 依赖最少、兼容 Rust 1.85；`rmcp` 可在 HTTP 传输时再引入 |

### v0.1（2026-11-21）

初稿：manifest、类型、信封、错误、可复现性、验证凭证、MCP 映射、治理与三个示例算子。原文见 `spec-v0.1.md`。

## 附录 A：文件清单

```
ai-operator-spec/
├── spec-v0.4.md                     本文（由 spec-src.md 经 build_spec.py 内联示例文件生成）
├── spec-v0.3.md                     v0.3.6-draft 的快照（历史版本，不再重新生成）
├── spec-v0.2.md / spec-v0.1.md      历史版本
├── spec-src.md                      本文源稿（file / jsonc 占位符）
├── schemas/                         common / manifest / envelope / error（/0.4）
├── examples/<id>/                   12 个算子：manifest、input/output schema、request(.error/.nonsymmetric)、
│                                    response(.success/.error/.nonsymmetric)、parity.yaml；v0.3 算子另有 reference.python-control.json
├── golden/python-control/<op>/<ver>/<sha256>.json + INDEX.json   264（0.2.0）+ 353（0.3.0）个 python-control 参考用例（NOT MATLAB）
├── golden/mpmath/<op>/<ver>/<sha256>.json + INDEX.json   28 个 50 位 mpmath 高精度参考用例（lqr 0.3.0：16，lyap 0.4.0：12）
├── gen_golden_mpmath.py             高精度 golden 生成器（复用 tools/xcheck-tpt/analyze.py 的 50 位求解）
├── reports/parity-mpmath.json       高精度 parity 报告（cargo test 生成）
├── reports/receipts.json            收据测试报告（golden 成功/错误数、各算子 claim 分布；cargo test 生成）
├── reports/receipts-wasm-verify.json  原生示例信封在 wasm 中重放验证的结果（verify_examples.mjs）
├── rust/aeoi-core/src/receipt.rs    收据构造、指纹、比较与 verify（4.3）
├── LICENSE-MIT、LICENSE-APACHE、LICENSE-CC-BY-4.0、THIRD_PARTY_NOTICES.md（v0.3.3；tools/gen_third_party.py 重新生成 crate 列表）
├── tools/xcheck-tpt/ + reports/parity-tpt-control.{json,md}   与 tpt-control 的交叉验证（开发用，不是 workspace 成员）
├── tools/hp_compare.py              golden_eval 输出与高精度 golden 的逐族对比（新旧版本 before/after）
├── reports/parity-python-control.json / parity-python-control-b2.json   parity 测试报告（cargo test 生成）
├── reports/mcp-session.jsonl        脚本化 MCP 会话记录（12 个工具）
├── rust/                            cargo workspace：aeoi-core、aeoi-control、aeoi-mcp、aeoi-bridge、aeoi-web、aeoi-wasm（第 11 章、8.4、8.5）
├── pages/                           浏览器版测试台与 build.sh（8.5；工作流在 .github/workflows/pages.yml）
├── reports/parity-wasm-vs-native.json     wasm 与原生的逐例比较
├── eval/questions.zh.json + gen_questions.py   LLM 评测题库（29 题）
├── reports/eval-<model>.json         LLM 评测报告（aeoi-web eval 生成）
├── matlab/gen_golden_bode.m         MATLAB golden 生成脚本（已修正内存序；未在 MATLAB 中运行）
├── write_manifests.py               examples/*/manifest 与 schema 的源
├── gen_golden_pycontrol.py / gen_golden_pycontrol_b2.py   python-control golden 生成器（v0.2 / v0.3 算子）
├── make_examples_b2.py              v0.3 算子的示例请求、参考值与 parity.yaml
├── regen_examples.py                用 Rust 输出生成示例响应
├── conformance/                     跨实现一致性套件 v0.1（9.4）：aeoi_conformance.py（运行器）、aeoi_hash.py、impls/python_scipy/（只含 lyap 的第二实现）、tests/（变异测试）
├── build_spec.py                    spec-src.md → spec-v0.4.md
├── eval/gen_questions.py            生成 eval/questions.zh.json（LLM 评测题库）
├── validate_examples.py             独立 Python jsonschema 校验
├── mcp_session_demo.py              脚本化 MCP 会话
├── compute_examples.py / computed_values.json   v0.1 的 python-control 计算（examples 测试仍用其作对照）
└── legacy/                          v0.1 的 build_examples.py 与 spec 源稿
```
