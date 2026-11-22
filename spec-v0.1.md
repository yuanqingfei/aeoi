# AI Engineering Operator Interface Spec（AEOI）v0.1 草案

> 状态：Draft v0.1 · 2026-11-21 · 作者：Qingfei Yuan（草拟协助：Grok Bot）
> 定位：叠加在 MCP 之上的"工程计算语义层"。MCP 负责传输与工具调用；AEOI 规定工程算子如何描述自己、如何表达带物理意义的输入输出、如何报告结果可信度、如何报错、如何复现、如何证明与参考实现一致。
> 冻结条件：用 5–10 个真实控制算子跑通后再升级为 v1.0。v0.x 期间任何条款都可能被修改。

## 0. 约定

- 关键词 MUST / MUST NOT / SHOULD / SHOULD NOT / MAY 按 RFC 2119 / RFC 8174 解释。
- JSON Schema 一律使用 Draft 2020-12。完整 schema 文件见 `schemas/`。
- 本文中 `spec` 版本标识写作 `aeoi/0.1`。
- **净室原则（normative）**：算子实现 MUST 仅依据公开文献、教科书、标准与开源许可兼容的代码；MUST NOT 反编译、反汇编、阅读或复制 MATLAB（或任何闭源软件）的实现代码、`.p` 文件、二进制或未公开文档。MATLAB 只作为黑盒 ground truth，通过其公开 API 生成对照数据。每个 manifest MUST 带 `provenance.clean_room.declared = true`。
- **示例数值说明**：第 9 章示例中的数值结果由 python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 在 Grok Bot 的 box 上实际计算（脚本 `compute_examples.py`，原始输出 `computed_values.json`），**不是 MATLAB 输出**。`timing`、`build`、`backend` 版本、`blas` 等字段为示意。

## 1. 架构与术语

```
Agent ──MCP(JSON-RPC)──▶ aeoi-mcp server ──▶ Registry ──▶ dyn Operator (Rust)
                     │                                   ├─ native Rust (faer/nalgebra)
Python (PyO3) ───────┤                                   └─ FFI backend (SLICOT/LAPACK/SUNDIALS/Ipopt/OSQP)
C ABI (cdylib) ──────┘          所有入口共享同一 manifest、schema、信封与错误码
```

| 术语 | 含义 |
|---|---|
| Operator（算子） | 一个纯函数式的工程计算单元，如 `control.lti.bode` |
| Manifest | 算子的自描述文档（第 2 章） |
| Envelope（信封） | 每次调用的统一返回结构（第 4 章） |
| Trust metadata | 结果可信度信息：收敛、残差、条件数、误差估计、精度等级 |
| Golden file | 参考实现（MATLAB/Octave/解析解）生成的对照数据 |
| Credential（验证凭证） | 某算子版本的 parity 测试记录与精度等级（第 7 章） |

## 2. Operator Manifest

### 2.1 规则

- M-1 每个算子 MUST 提供一份符合 `schemas/manifest.schema.json` 的 manifest，并随二进制一起发布；MCP server MUST 能以资源 `aeoi://operators/{id}/{version}/manifest` 返回它。
- M-2 `id` MUST 为小写点分命名空间 `domain.subdomain.name`（2–4 级点号），如 `control.lti.bode`、`control.design.lqr`、`signal.filter.butter`、`optim.qp.solve`。一个 `id` 一旦发布，语义 MUST NOT 改变；语义改变需新 `id` 或主版本升级。
- M-3 `version` MUST 遵循 SemVer 2.0（见 9.2 的破坏性变更定义）。
- M-4 `applicability.preconditions` MUST 列出所有数学前提（如"R 对称正定"）。凡是可以机器检查的前提，`validate()` MUST 检查并在违反时返回对应 `E.VALIDATION.*` 或 `E.DOMAIN.*`。
- M-5 `provenance.references` MUST 至少一条可查证的公开文献（论文 SHOULD 带 DOI）。`provenance.algorithm` MUST 用一句话说明实际采用的算法（而非"调用某库"）。
- M-6 `backend` MUST 写明库名、例程名（如 SLICOT `SB02OD`）、版本、`binding`（`native` | `ffi`）与 SPDX 许可证。许可证 MUST 与平台分发方式兼容（GPL 后端需单独评估）。
- M-7 `determinism` MUST 取以下之一：

| 值 | 含义 | 允许的场景 |
|---|---|---|
| `bitwise` | 同 build、同平台、同输入 ⇒ 输出逐位相同 | 默认要求 |
| `platform_bitwise` | 同平台逐位相同；跨 CPU 架构只保证在容差类内 | 依赖 SIMD/FMA 的内核 |
| `tolerance` | 同平台多次运行也只保证在容差类内 | 必须说明原因（如并行归约），L3 需评审 |
| `seeded` | 给定 `seed` 后满足 `bitwise` | 随机算法（蒙特卡洛、随机初始化） |

- M-8 `tolerance_class` MUST 取 7.2 表中的一个值。

### 2.2 片段

```json
{
  "spec": "aeoi/0.1",
  "id": "control.design.lqr",
  "version": "0.1.0",
  "determinism": "bitwise",
  "tolerance_class": "T2",
  "provenance": {
    "algorithm": "Hamiltonian 矩阵的有序实 Schur 分解，取稳定不变子空间",
    "references": [{ "type": "paper", "cite": "A. J. Laub, A Schur method for solving algebraic Riccati equations, IEEE TAC 24(6), 1979", "doi": "10.1109/TAC.1979.1102178" }],
    "clean_room": { "declared": true }
  },
  "backend": { "library": "SLICOT", "routine": "SB02OD", "version": "5.9", "binding": "ffi", "license": "BSD-3-Clause" }
}
```

## 3. 带物理意义的类型化输入输出

### 3.1 数据类型

| 类型 | 编码 | 说明 |
|---|---|---|
| 实数标量 | JSON number，或 `{"dtype":"f64","value":x}` | IEEE 754 binary64 |
| 复数 | `[re, im]` | 不使用字符串 `"1+2i"` |
| 向量 / 矩阵 / N 维数组 | `{"dtype","shape","data"}` | `data` 为 **row-major (C order)** 展平一维数组 |
| 物理量 | `{"unit": "<UCUM>", "value": <标量或数组>}` | 见 3.2 |
| 多项式 | 实系数数组，**降幂** | `[1, 1.2, 4]` = s² + 1.2s + 4 |
| LTI 系统 | `{"kind":"tf"|"ss"|"zpk", ..., "ts"}` | `ts=0` 连续，`ts>0` 离散 |

规则：
- T-1 矩阵 MUST 使用 `{"dtype","shape","data"}` 规范形式。绑定层 MAY 接受嵌套数组 `[[1,2],[3,4]]` 作为便捷输入，但 MUST 在校验前规范化为规范形式；哈希只针对规范形式计算。选择展平+shape 而非嵌套数组，是为了无歧义地表示空矩阵（如 `shape:[0,3]`）与高维数组，且与 Rust/NumPy 内存布局一致。
- T-2 `dtype` 取 `f64`、`c128`、`i64`、`bool`。v0.1 不支持 f32 输入。复数元素 MUST 为 `[re, im]`。
- T-3 `data.length` MUST 等于 `shape` 各维乘积；违反返回 `E.VALIDATION.SHAPE`（JSON Schema 无法表达，由 `validate()` 检查）。
- T-4 FFI 到 Fortran 列主序库时，转置 MUST 在 backend 包装层完成，且 MUST 有往返单元测试。
- T-5 LTI 表示：v0.1 中 `tf` 支持 SISO（`num`/`den` 为一维多项式）与 MIMO（二维多项式数组）；`zpk` 中复数零极点 MUST 共轭成对；`ss` 的 `A,B,C,D` 维数 MUST 一致。算子 SHOULD 在 manifest 中说明接受哪些 `kind`，内部转换（如 tf→ss）MUST 在 trust 中注明所用 realization。

### 3.2 单位

- U-1 单位字符串 MUST 使用 **UCUM 大小写敏感码（c/s）**：`s`、`ms`、`rad`、`rad/s`、`Hz`、`deg`、`m`、`N`、`N.m`、`kg.m2`、`%`、`1`（无量纲）。
- U-2 `dB` 作为本规范登记的扩展单位，定义为幅值比的 `20·log10(|x|)`；功率比 MUST 另用 `dB[pow]`（v0.1 暂未使用）。
- U-3 每个带物理意义的输入 MUST 在 schema 中声明允许的单位（`enum`）；算子 MAY 接受多个单位（如 `rad/s` 与 `Hz`）并在内部换算，换算 MUST 在输出中以规范单位返回并标注 `unit`。
- U-4 不得静默假设单位：缺少 `unit` 的物理量 MUST 返回 `E.VALIDATION.UNIT`，而不是默认 SI。
- U-5 时间基准：LTI 系统 `time_unit` v0.1 固定为 `s`。

### 3.3 取值范围与 NaN/Inf

- R-1 范围约束（正数、区间、整数）MUST 写入 JSON Schema（`minimum`/`exclusiveMinimum`/`maximum`）；跨字段约束（`min < max`、`w < π/ts`）由 `validate()` 检查。
- R-2 JSON 无法表示 NaN/Inf。规范编码为字符串 `"NaN"`、`"Inf"`、`"-Inf"`，且仅在 schema 显式允许的位置出现。**输入默认拒绝非有限值**（`E.VALIDATION.NON_FINITE`）。
- R-3 输出中出现非有限值（如增益裕度为无穷）MUST 使用 R-2 编码，并 SHOULD 附一条 warning 说明含义（例如"相位从未穿越 −180°，增益裕度为 Inf"）。
- R-4 所有输入对象 MUST 设 `additionalProperties: false`：Agent 拼错参数名必须立刻得到错误，而不是被静默忽略。

### 3.4 通用类型 schema（节选，完整见 `schemas/common.schema.json`）

```json
"Matrix": {
  "type": "object", "additionalProperties": false, "required": ["dtype", "shape", "data"],
  "properties": {
    "dtype": { "enum": ["f64", "c128", "i64", "bool"] },
    "shape": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "minItems": 1, "maxItems": 8 },
    "data":  { "type": "array", "items": { "oneOf": [
      { "type": "number" }, { "$ref": "#/$defs/Complex" }, { "type": "boolean" }, { "enum": ["NaN", "Inf", "-Inf"] } ] } },
    "structure": { "enum": ["general", "symmetric", "hermitian", "diagonal", "upper_triangular", "lower_triangular"], "default": "general" }
  }
},
"LtiSystem": { "oneOf": [
  { "properties": { "kind": { "const": "tf" },  "num": "...", "den": "...", "ts": { "type": "number", "minimum": 0 } } },
  { "properties": { "kind": { "const": "ss" },  "A": { "$ref": "#/$defs/Matrix" }, "B": "...", "C": "...", "D": "...", "ts": "..." } },
  { "properties": { "kind": { "const": "zpk" }, "z": "...", "p": "...", "k": { "type": "number" }, "ts": "..." } }
] }
```

## 4. 结果信封与可信度元数据

### 4.1 规则

- E-1 每次调用（无论成败）MUST 返回一个符合 `schemas/envelope.schema.json` 的信封。
- E-2 `status` ∈ `ok` | `ok_with_warnings` | `error`。只要 `warnings` 非空，`status` MUST 为 `ok_with_warnings`。`error` 时 `result` MUST 为 `null`。
- E-3 成功信封 MUST 包含 `trust` 与 `reproducibility`。
- E-4 `trust` 字段：

| 字段 | 必须 | 内容 |
|---|---|---|
| `convergence` | MUST | `applicable`；迭代算法另需 `converged`、`iterations`、`criterion`、`final_value` |
| `residual` | 若算法有自然残差则 MUST | 如 CARE 残差 ‖AᵀS+SA−SBR⁻¹BᵀS+Q‖_F/‖S‖_F、KKT 残差 |
| `condition` | SHOULD | 关键条件数或其估计（如 `cond_R`、`rcond`），以及到奇异/不可控的距离 |
| `error_estimate` | MUST（可为 null） | 误差估计值 + `basis`（先验界 / 后验残差 / 嵌入式估计 / 无） |
| `checks` | SHOULD | 后置条件布尔检查，如 `S_psd`、`closed_loop_stable` |
| `accuracy_grade` | MUST | 当前版本的精度等级 A/B/C/U（第 7 章） |
| `credentials` | L3 MUST | 指向验证凭证的 URI |

- E-5 误差估计不得夸大：若只有启发式估计，`basis` MUST 写明"非严格上界"。没有估计时 MUST 写 `null`，MUST NOT 编造数字。
- E-6 `trust_level: "summary"`（默认）只返回上表字段；`"full"` MAY 附加逐频点/逐时间步的诊断数组。
- E-7 `warnings[]` 元素：`code`（`W.<CATEGORY>.<NAME>`）、`message`、`path`（JSON Pointer）、可选 `details` 与 `remediation`。
- E-8 MCP 返回时，信封放入 `structuredContent`；`content` 中 MUST 同时给出一段简短文本摘要（含 status、关键结果、关键 warning），以兼容不读 `structuredContent` 的客户端。

### 4.2 信封骨架

```json
{
  "spec": "aeoi/0.1",
  "status": "ok | ok_with_warnings | error",
  "operator": { "id": "control.lti.bode", "version": "0.1.0", "build": "<crate版本+git sha+backend版本>" },
  "request_id": "req-001",
  "result": { "...": "由 output_schema 定义" },
  "trust": { "convergence": {}, "residual": {}, "condition": {}, "error_estimate": {}, "checks": {}, "accuracy_grade": "A", "credentials": "aeoi://..." },
  "warnings": [],
  "error": null,
  "reproducibility": { "determinism": "bitwise", "tolerance_class": "T3", "input_hash": "sha256:…", "output_hash": "sha256:…", "platform": "x86_64-linux-gnu", "blas": "…", "threads": 1, "seed": null },
  "timing": { "wall_ms": 0.08, "cpu_ms": 0.08 }
}
```

## 5. 结构化错误

### 5.1 错误码分类

格式：`E.<CATEGORY>.<NAME>`。类别固定为 5 个；`NAME` 由注册表统一登记，算子 MUST 在 manifest `errors[]` 中列出可能返回的全部错误码。

| 类别 | 何时使用 | `retryable` 默认 | 典型码 |
|---|---|---|---|
| `VALIDATION` | 输入不符合 schema 或语义约束 | true（改输入） | `SCHEMA`、`SHAPE`、`UNIT`、`RANGE`、`NON_FINITE`、`NOT_SYMMETRIC`、`GRID` |
| `DOMAIN` | 输入合法，但数学问题本身无解/不适用 | 视情况 | `IMPROPER_SYSTEM`、`NOT_STABILIZABLE`、`NOT_DETECTABLE`、`R_NOT_POSITIVE_DEFINITE`、`ABOVE_NYQUIST`、`INFEASIBLE`、`UNBOUNDED` |
| `NUMERICAL` | 问题理论上可解，但数值上失败或不可信 | true（正则化/放宽/换算法） | `SINGULAR`、`ILL_CONDITIONED`、`NO_CONVERGENCE`、`OVERFLOW`、`POLE_ON_GRID`、`STEP_SIZE_UNDERFLOW` |
| `RESOURCE` | 超时、超内存、输出过大、被取消 | true | `TIMEOUT`、`MEMORY_LIMIT`、`OUTPUT_TOO_LARGE`、`CANCELLED` |
| `INTERNAL` | 实现缺陷或后端异常 | false（可原样重试一次） | `BACKEND`、`PANIC`、`INVARIANT_VIOLATED` |

规则：
- X-1 `DOMAIN` 与 `NUMERICAL` MUST 区分清楚：前者告诉 Agent"别再调参了，问题本身不成立"，后者告诉 Agent"换个数值处理可能成功"。这是本规范对 Agent 最重要的区分。
- X-2 FFI 后端的每个 `INFO`/返回码 MUST 映射到具体错误码；MUST NOT 把后端错误笼统报为 `INTERNAL.BACKEND`，除非确实无法归类。
- X-3 Rust `panic` MUST 被 `catch_unwind` 捕获并转为 `E.INTERNAL.PANIC`，FFI 边界 MUST NOT 让 panic 或 Fortran 异常跨越。
- X-4 数值"勉强成功"（如条件数 1e14）SHOULD 返回成功 + `W.NUMERICAL.ILL_CONDITIONED`，而不是错误；阈值 MUST 写在 manifest。

### 5.2 错误对象

```json
{
  "code": "E.DOMAIN.NOT_STABILIZABLE",
  "category": "domain",
  "retryable": false,
  "message": "面向人：发生了什么、为什么（一两句）",
  "agent_hint": "面向 Agent：下一步做什么，哪些重试是无效的",
  "details": { "path": "/B", "uncontrollable_eigenvalues": { "dtype": "c128", "shape": [1], "data": [[1.0, 0.0]] } },
  "remediation": [
    { "id": "inspect", "kind": "call_operator", "operator": "control.lti.ctrb_decomp", "arguments_from": { "A": "/A", "B": "/B" }, "confidence": "high", "requires_user_confirmation": false, "description": "…" },
    { "id": "add_actuator", "kind": "modify_input", "patch": [{ "op": "replace", "path": "/B", "value": "…" }], "confidence": "low", "requires_user_confirmation": true, "description": "…" }
  ]
}
```

- X-5 `remediation[].kind` ∈ `modify_input`（MUST 带 RFC 6902 `patch`，作用于原请求 arguments）、`relax_tolerance`、`regularize`、`call_operator`（MUST 带 `operator`，参数用 `arguments_from` 以 JSON Pointer 引用原请求）、`retry_same`、`ask_user`。
- X-6 凡改变物理模型含义的修复（改 A/B、改系统阶次、加滤波器）MUST 设 `requires_user_confirmation: true`。只影响数值处理或输出规模的修复（换网格、抽样、放宽容差）MAY 为 false。Agent 侧 SHOULD 尊重该标志。
- X-7 `remediation` 按推荐度排序；MUST 至少一条（实在无建议时给 `ask_user`）。
- X-8 `details.path` SHOULD 用 JSON Pointer 指向出错的输入字段。

## 6. 可复现性

- P-1 **版本**：`build` 字符串 MUST 包含 crate 版本、git commit、后端库版本、BLAS/LAPACK 实现与版本。任何一项变化都视为不同 build。
- P-2 **哈希**：`input_hash` = SHA-256（RFC 8785 JCS 规范化后的请求 arguments，去掉 `exec`）；`output_hash` = SHA-256（JCS 规范化后的 `result`）。浮点数序列化 MUST 使用最短往返表示（Rust `ryu`），保证同值同串。
- P-3 **线程与 BLAS**：`bitwise` 算子 MUST 以单线程 BLAS 运行，或使用确定性归约顺序的并行实现；BLAS/LAPACK MUST 随发行版静态链接并锁定版本与内核（如 OpenBLAS 固定 `CORETYPE`），MUST NOT 使用运行时动态分派到未声明的内核。MKL 只能在开启 CNR（Conditional Numerical Reproducibility）模式时用于 `bitwise` 算子。
- P-4 **浮点策略**：MUST NOT 启用 fast-math 类优化（重结合、flush-to-zero、近似倒数）。FMA 收缩仅在 `platform_bitwise` 算子中允许，并在 manifest 中声明。Rust 默认不做 FMA 收缩，C/Fortran 后端编译时 MUST 加 `-ffp-contract=off`（或等价选项）。
- P-5 **随机性**：随机算法 MUST 为 `seeded`，PRNG 算法固定（建议 `ChaCha20`，`rand_chacha`），`seed` 回显在 `reproducibility.seed`。未给 `seed` 时由宿主生成并回显。
- P-6 **容差**：算子内部的收敛容差 MUST 写在 manifest（含默认值）；允许调用方覆盖的容差 MUST 列入 `overridable_tolerances`，覆盖值回显在信封中。
- P-7 **重放**：同 `build` + 同 `input_hash` + 同 `seed` 的两次调用，`bitwise` 算子的 `output_hash` MUST 相同。CI MUST 有"同机重放"测试；跨平台只要求满足容差类。
- P-8 **无隐藏状态**：算子 MUST 是纯函数，不得读取环境变量、文件、时钟（`timing` 除外）或全局可变状态影响结果。

## 7. 验证凭证（Verification Credentials）

### 7.1 生成与存储 golden 数据

- V-1 参考数据来源优先级：① MATLAB（ground truth，用户本机或 MATLAB Online，正版授权）② 解析解/高精度计算（如 mpmath 多精度）③ GNU Octave 与其他开源实现（仅开发阶段对照，不能单独授予 A 级）。
- V-2 MATLAB 侧只运行公开 API 的生成脚本（`matlab/gen_golden_<op>.m`），由固定 `rng(seed)` 生成用例，记录 `version`、工具箱版本、`version('-blas')`、`version('-lapack')`、`computer('arch')`、`maxNumCompThreads`。
- V-3 golden 文件格式：JSON（大数组 MAY 用 `.mat` v7.3 / HDF5 并在 JSON 中引用），文件名 = 内容 SHA-256（内容寻址）。目录：`golden/<op>/<op_version>/<sha256>.json`。golden 文件 MUST 纳入版本库或制品仓库，MUST NOT 在 CI 中重新生成。
- V-4 parity 测试 MUST 使用显式输入（显式频点、显式时间网格），MUST NOT 依赖参考实现的自动网格/自动选择逻辑（这些逻辑未公开且会随版本变化）。
- V-5 非唯一输出（特征向量、状态空间实现、Schur 基）MUST 比较不变量（特征值集合、传递函数、投影子），不得逐元素比较。
- V-6 比较公式（逐元素）：|x − x_ref| ≤ atol + rtol·|x_ref|；矩阵 SHOULD 用范数形式 ‖X − X_ref‖_F ≤ atol + rtol·‖X_ref‖_F；条件数大的问题 rtol MAY 按条件数缩放，缩放规则写入 parity spec。

### 7.2 容差类

| 类 | 算法类型 | 默认判据 | 说明 |
|---|---|---|---|
| T0 | 精确/整数/组合 | 完全相等 | 秩、阶次、结构判定 |
| T1 | 直接稠密线性代数 | rtol 1e-12（× κ 缩放） | 求解、分解、乘法 |
| T2 | 矩阵函数、特征值、Riccati/Lyapunov、LTI 精确离散化 | rtol 1e-10（× κ 缩放） | `expm`、`lqr`、`lyap`、ZOH `step` |
| T3 | 频率响应求值 | 幅值 1e-9 dB abs，相位 1e-8 deg（mod 360） | `bode`、`nyquist`、`freqresp` |
| T4 | 变步长 ODE / 一般仿真 | rtol 1e-6、atol 1e-9，或"求解器容差 × 10" | 参考与被测 MUST 用相同容差设置；只比较输出网格点 |
| T5 | 迭代优化 | KKT 残差 ≤ tol；目标值 rtol 1e-6；解向量仅在严格凸时比较 | `quadprog` 类、NLP |
| T6 | 随机算法 | 统计检验（如 KS，p > 0.01）或固定 seed 下的 T1–T5 | 蒙特卡洛 |

### 7.3 精度等级

| 等级 | 条件 |
|---|---|
| A | 对 MATLAB golden 全部通过（每个 case family ≥ manifest 声明数量，含边界/病态/报错用例），且不变量检查全部通过 |
| B | 对解析解或高精度参考全部通过，或对 Octave/开源参考全部通过 + 不变量检查；尚无 MATLAB 对照 |
| C | 只有不变量/自洽性检查（残差、对称性、稳定性） |
| U | 未验证（实验性）；生产注册表 MUST NOT 默认暴露 U 级算子 |

### 7.4 凭证记录

```json
{
  "operator": "control.design.lqr", "version": "0.1.0", "build": "…",
  "grade": "A", "tolerance_class": "T2",
  "reference": { "engine": "MATLAB", "version": "R2025b", "toolbox": "Control System Toolbox 25.2", "blas": "…", "arch": "glnxa64" },
  "golden_set": { "root": "golden/control.design.lqr/0.1.0", "n_cases": 500, "manifest_sha256": "…" },
  "results": { "passed": 500, "failed": 0, "max_rel_err": { "S": 3.1e-13, "K": 2.4e-13 }, "expected_errors_matched": 30 },
  "invariants": { "care_rel_residual_max": 4.0e-15, "all_closed_loop_stable": true },
  "run": { "ci_job": "…", "date": "2026-…", "platforms": ["x86_64-linux-gnu", "aarch64-apple-darwin"] }
}
```

（上面的凭证数字为格式示意，尚无实际 MATLAB 运行。）

- V-7 凭证 MUST 与 `build` 绑定；任何后端或 BLAS 升级 MUST 重新跑 parity 并生成新凭证。
- V-8 凭证 SHOULD 由 CI 签名（如 Sigstore），平台展示"verified"标识前 MUST 校验签名。

## 8. 映射到 MCP 与绑定层

### 8.1 manifest → MCP tool

| MCP 字段 | 来源 | 规则 |
|---|---|---|
| `name` | `manifest.mcp.tool_name` | 由 `id` 把 `.` 换成 `_` 得到（`control_lti_bode`），只用 `[a-zA-Z0-9_-]`，兼容所有客户端 |
| `title` | `manifest.title` | |
| `description` | `description` + 前提条件 + 单位约定 + 精度等级 | 面向模型写：先说做什么，再说何时不该用 |
| `inputSchema` | `manifest.input_schema`（解引用后内联） | MUST 完全展开 `$ref`，因部分客户端不解析外部引用 |
| `outputSchema` | 信封 schema，`result` 替换为 `output_schema` | |
| `annotations` | `readOnlyHint: true`、`idempotentHint: true`、`openWorldHint: false` | 纯计算算子 |
| `_meta` | `{"aeoi/id","aeoi/version","aeoi/grade"}` | 保留原始 id |

- C-1 算子层错误（`E.*`）MUST 作为正常 tool result 返回，`isError: true` 且 `structuredContent` 为错误信封；MUST NOT 用 JSON-RPC 协议错误，否则 Agent 看不到 remediation。JSON-RPC 错误只用于协议层问题（未知工具、请求格式错误）。
- C-2 server SHOULD 暴露资源：`aeoi://operators`（目录）、`aeoi://operators/{id}/{version}/manifest`、`…/credentials`、`aeoi://schemas/{name}/{ver}`。
- C-3 大型输出（> 1 MB 或 > 10⁵ 元素）SHOULD 以 MCP resource link 返回，信封中给出摘要与 `resource_uri`（v0.1 尚未细化，见第 10 章）。
- C-4 工具数量多时，server SHOULD 支持按命名空间过滤列出（如只暴露 `control.*`），避免一次向模型塞几百个工具。

### 8.2 Rust 核心 trait（草图，完整见 `rust/operator.rs`）

```rust
pub trait Operator: Send + Sync + 'static {
    type Input: for<'de> Deserialize<'de>;
    type Result: Serialize;

    fn manifest(&self) -> &Manifest;
    /// schema 之外的语义校验：shape、对称性、properness、单位。纯函数，不做主计算
    fn validate(&self, input: &Self::Input) -> Result<Vec<Warning>, OpError>;
    /// 主计算；确定性按 manifest.determinism
    fn execute(&self, input: Self::Input, ctx: &ExecContext) -> Result<Output<Self::Result>, OpError>;
    /// dry_run 时的代价估计
    fn estimate(&self, _input: &Self::Input) -> Option<CostEstimate> { None }
}
```

调用管线（框架统一实现，算子只实现上面 4 个方法）：

1. JSON Schema 校验（`jsonschema` crate）→ 失败返回 `E.VALIDATION.SCHEMA`，`details` 带全部违规的 JSON Pointer
2. 规范化（嵌套数组→规范矩阵、单位换算）
3. `serde` 反序列化为 `Input`
4. `validate()` → warnings 或错误
5. `dry_run` 时返回 `estimate()` 后停止
6. `catch_unwind(execute)`，受 `deadline` 与 `cancel` 约束
7. 框架填充 `reproducibility`（哈希、build、线程数）、`timing`，组装信封

### 8.3 绑定层

| 绑定 | crate | 约定 |
|---|---|---|
| MCP server | `aeoi-mcp`（基于官方 Rust SDK `rmcp`） | stdio 与 Streamable HTTP 两种传输；注册表自动生成 tool 列表 |
| Python | `aeoi-py`（PyO3 + maturin） | `aeoi.call("control.lti.bode", args) -> dict`；另提供 NumPy 便捷层，`ndarray` ↔ 规范矩阵零拷贝转换；错误抛 `AeoiError`，带完整错误对象 |
| C ABI | `aeoi-capi`（cdylib + cbindgen 头文件） | 只暴露 JSON 字符串接口：`aeoi_call(const char* id, const char* json_args, char** json_envelope)` + `aeoi_free`；ABI 稳定，便于 MATLAB MEX、Simulink S-function、C++、Julia 接入 |

- B-1 所有绑定 MUST 返回同一信封结构；绑定层 MUST NOT 自行改写数值或错误码。
- B-2 Python 绑定 MUST 释放 GIL 执行长计算。

## 9. 治理

### 9.1 合规等级

| 等级 | 要求 | 用途 |
|---|---|---|
| L1 Basic | manifest、严格 input/output schema、信封、结构化错误（含 remediation） | 实验性接入 |
| L2 Trusted | L1 + 完整 trust 元数据 + 可复现性（第 6 章全部 MUST）+ 重放测试 | 生产注册表最低要求 |
| L3 Verified | L2 + 验证凭证等级 A 或 B + CI 签名 + parity 测试持续运行 | 平台"verified"标识 |

### 9.2 版本与弃用

- G-1 破坏性变更（MAJOR）：删除/重命名输入输出字段、改变默认值导致结果变化、改变单位、收紧 schema、改变错误码含义、tolerance class 变宽。
- G-2 MINOR：新增可选输入、新增输出字段、新增错误码/warning、精度提升（tolerance class 收窄）。
- G-3 PATCH：修 bug 且结果仍在原容差类内、性能优化。若修复导致结果超出原容差，按 MINOR 处理并在 changelog 说明。
- G-4 0.x 版本按 SemVer 约定，MINOR 也可破坏。
- G-5 弃用：manifest 设 `deprecated {since, replaced_by, removal_after}`；调用时 MUST 返回 `W.LIFECYCLE.DEPRECATED`；弃用期 ≥ 2 个 MINOR 版本或 90 天，取较长者。
- G-6 注册表 MUST 允许同一 `id` 的多个 MAJOR 版本并存；MCP tool 默认指向最新稳定 MAJOR。

### 9.3 新算子接入清单

1. 选定 `id`，确认不与已有算子重复；写出数学定义与前提条件
2. 列出公开文献（含 DOI），声明净室来源；确认后端许可证
3. 写 manifest、input/output schema（`additionalProperties:false`、单位、范围）
4. 列出全部错误码与 warning，每个错误码至少一条 remediation
5. 实现 `validate()`、`execute()`；FFI 包装含返回码映射与转置测试
6. 实现 trust 字段（残差/条件数/误差估计及其 basis）
7. 写 parity spec：case families、容差类、比较模式、预期报错用例
8. 写 MATLAB 生成脚本，由有授权的机器运行生成 golden 文件并入库
9. CI：schema 校验、单元测试、重放测试、parity 测试、跨平台测试
10. 评审：数值评审（算法与容差合理性）+ 接口评审（Agent 可用性：错误信息与 remediation 是否可执行）
11. 生成凭证、确定等级与合规等级、发布

## 10. 完整示例

`bode` 与 `step` 使用同一被控对象，`lqr` 使用经典双积分器。所有数值由 python-control 0.10.2 / scipy 1.18.1 在 box 上实际计算，不是 MATLAB 输出；`timing`、`build`、后端版本与精度等级为示意。每个示例的完整文件在 `examples/<id>/`。

被控对象：G(s) = 4 / (s² + 1.2s + 4)（ωn = 2 rad/s，ζ = 0.3），极点 −0.6 ± 1.9079j。

### 10.1 `control.lti.bode`

**Manifest** — `examples/control.lti.bode/manifest.json`

```json
{
  "spec": "aeoi/0.1",
  "id": "control.lti.bode",
  "version": "0.1.0",
  "title": "Bode frequency response",
  "description": "计算连续或离散 LTI 系统在给定频率点上的幅值（dB）与相位（deg）。",
  "category": "analysis",
  "tags": [
    "frequency-domain",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "system 为 proper（分子阶次 ≤ 分母阶次）",
      "离散系统要求 w < π/ts（Nyquist 频率）"
    ],
    "not_for": [
      "非线性系统",
      "时变系统"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "Hessenberg 化后逐频点求解 (jωI - H)x = b",
    "references": [
      {
        "type": "paper",
        "cite": "A. J. Laub, Efficient multivariable frequency response computations, IEEE TAC 26(2), 1981",
        "doi": "10.1109/TAC.1981.1102605"
      },
      {
        "type": "textbook",
        "cite": "G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源实现；MATLAB 仅用于生成对照数据"
    }
  },
  "backend": {
    "library": "SLICOT",
    "routine": "TB05AD",
    "version": "5.9",
    "binding": "ffi",
    "license": "BSD-3-Clause"
  },
  "determinism": "bitwise",
  "tolerance_class": "T3",
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "draft"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "U"
  },
  "credentials": "aeoi://operators/control.lti.bode/0.1.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.UNIT",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.DOMAIN.ABOVE_NYQUIST",
    "E.NUMERICAL.POLE_ON_GRID"
  ],
  "warnings": [
    "W.NUMERICAL.NEAR_POLE",
    "W.NUMERICAL.ILL_CONDITIONED"
  ],
  "mcp": {
    "tool_name": "control_lti_bode",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  }
}
```

**Input schema** — `examples/control.lti.bode/input.schema.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.lti.bode/0.1.0/input",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "system",
    "frequency"
  ],
  "properties": {
    "system": {
      "$ref": "aeoi://schemas/common/0.1/LtiSystem"
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
              "maxItems": 100000
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
      ]
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
          "description": "首个频点相位落在 (c-180, c+180] 内"
        }
      }
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.1/ExecOptions"
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

**成功响应**（幅值/相位为 python-control 实算；ω=2 rad/s 处相位正好 −90°、幅值 +4.437 dB = 20·log10(1/(2ζ))，可作手算核对）

```json
{
  "spec": "aeoi/0.1",
  "status": "ok",
  "operator": {
    "id": "control.lti.bode",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
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
          0.0178154212247,
          0.450794350257,
          1.8541948399,
          4.43697499233,
          -14.7439839638,
          -27.6715586608
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
          -1.72266207162,
          -9.09027692082,
          -21.8014094864,
          -90.0,
          -164.054604099,
          -172.874983651
        ]
      }
    }
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "condition": {
      "max_cond_jwI_minus_A": 10.9251346,
      "realization": "python-control tf2ss 默认实现",
      "min_pole_distance_to_grid_rel": 0.303515
    },
    "residual": null,
    "error_estimate": {
      "magnitude_db_abs": 1e-10,
      "phase_deg_abs": 1e-09,
      "basis": "前向误差界 ≈ κ(jωI-A)·u，按最坏频点估计"
    },
    "accuracy_grade": "A",
    "credentials": "aeoi://operators/control.lti.bode/0.1.0/credentials",
    "derived": {
      "poles": {
        "dtype": "c128",
        "shape": [
          2
        ],
        "data": [
          [
            -0.6,
            1.90787840283
          ],
          [
            -0.6,
            -1.90787840283
          ]
        ]
      }
    }
  },
  "warnings": [],
  "reproducibility": {
    "determinism": "bitwise",
    "tolerance_class": "T3",
    "input_hash": "sha256:39fd070e50c26349b59f5c7365c830ab1ac5e6397f0142e67bf9205640e1da0c",
    "output_hash": "sha256:9c0a1650cc791d964fa0fbe43a76062062990900a488a7753f558a61b0b53ee9",
    "platform": "x86_64-linux-gnu",
    "blas": "openblas-0.3.28 (pinned, OPENBLAS_CORETYPE=Haswell, threads=1)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.08,
    "illustrative": true
  },
  "reference_note": {
    "engine": "python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5",
    "note": "示例数值由 python-control 在 box 上实际计算，非 MATLAB 输出；timing 与 build 字段为示意"
  }
}
```

**错误请求与响应**（非 proper 系统）

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
  "spec": "aeoi/0.1",
  "status": "error",
  "operator": {
    "id": "control.lti.bode",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
  },
  "request_id": "req-bode-002",
  "result": null,
  "error": {
    "code": "E.DOMAIN.IMPROPER_SYSTEM",
    "category": "domain",
    "retryable": true,
    "message": "传递函数不是 proper：分子阶次 2 > 分母阶次 1，频率响应在 ω→∞ 时发散，无状态空间实现。",
    "agent_hint": "该算子仅接受 proper 系统。若这是理想微分器，请补充高频极点使其 proper；若输入有误，请检查 num/den 是否颠倒。",
    "details": {
      "num_degree": 2,
      "den_degree": 1,
      "path": "/system"
    },
    "remediation": [
      {
        "id": "add_filter_pole",
        "kind": "modify_input",
        "description": "在分母补一个远高于关心频带的极点 1/(τs+1)，τ=1e-3 s",
        "confidence": "medium",
        "requires_user_confirmation": true,
        "patch": [
          {
            "op": "replace",
            "path": "/system/den",
            "value": [
              0.001,
              1.001,
              1
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
              1,
              1
            ]
          },
          {
            "op": "replace",
            "path": "/system/den",
            "value": [
              1,
              0,
              0
            ]
          }
        ]
      },
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认系统模型是否为理想微分环节"
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.01,
    "illustrative": true
  }
}
```

**Parity 测试规格** — `examples/control.lti.bode/parity.yaml`

```yaml
# Parity test spec — control.lti.bode 0.1.0
operator: control.lti.bode
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
  secondary: {engine: GNU Octave, version: "9.x", package: "control 4.x", role: dev_only}
generator: matlab/gen_golden_bode.m      # MATLAB 只运行公开 API，输出 golden 文件
golden_store: golden/control.lti.bode/0.1.0/*.json   # 内容寻址，文件名 = sha256
tolerance_class: T3
compare:
  magnitude: {quantity: abs_db, atol: 1.0e-9}          # dB 绝对误差
  phase:     {mode: modulo_360_then_abs, atol_deg: 1.0e-8, note: "相位分支约定可能与参考实现不同；先比 mod 360，再单独测 unwrap 连续性"}
  w:         {mode: exact}                              # 频点由请求显式给定，不使用参考实现的自动网格
case_families:
  - {name: siso_2nd_order_sweep, n: 200, params: "wn∈logU[1e-2,1e2], zeta∈U[0.01,2]"}
  - {name: mimo_ss_random_stable, n: 100, params: "n∈[2,30], m,p∈[1,4]"}
  - {name: discrete_ts, n: 50, params: "ts∈{1e-3,0.1,1}, w up to 0.99·π/ts"}
  - {name: near_imag_axis_poles, n: 30, expect: "W.NUMERICAL.NEAR_POLE"}
  - {name: integrators_and_delays_excluded, n: 0, note: "v0.1 不支持 InputDelay"}
  - {name: improper_reject, n: 10, expect: "E.DOMAIN.IMPROPER_SYSTEM"}
pass_rule: "所有 case 在容差内；预期报错的 case 报出相同 code"
```

### 10.2 `control.lti.step`

关键设计：对 LTI 系统，阶跃响应用 ZOH 精确离散化（矩阵指数）计算，阶跃输入在 ZOH 下没有离散化误差，因此归入 T2 而不是 ODE 的 T4。阶跃指标（上升时间等）受时间网格分辨率限制，误差界为一个网格步 `dt`。

**Manifest**

```json
{
  "spec": "aeoi/0.1",
  "id": "control.lti.step",
  "version": "0.1.0",
  "title": "Step response",
  "description": "计算 LTI 系统在给定时间网格上的单位阶跃响应，并给出阶跃特性指标（上升时间、调节时间、超调等）。",
  "category": "analysis",
  "tags": [
    "time-domain",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "system 为 proper",
      "时间网格均匀、单调递增且 t0=0",
      "若请求 step_info，系统应渐近稳定（否则指标标记为 null 并给出 warning）"
    ],
    "not_for": [
      "非线性系统",
      "任意输入（用 control.lti.lsim）"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "零阶保持精确离散化：对增广矩阵 [[A,B],[0,0]]·dt 求矩阵指数后递推（阶跃输入在 ZOH 下无离散化误差）",
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
      "sources": "仅公开文献与开源实现；MATLAB 仅用于生成对照数据"
    }
  },
  "backend": {
    "library": "faer (pure Rust) expm + 自研递推",
    "version": "0.19",
    "binding": "native",
    "license": "MIT"
  },
  "determinism": "bitwise",
  "tolerance_class": "T2",
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "draft"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "U"
  },
  "credentials": "aeoi://operators/control.lti.step/0.1.0/credentials",
  "errors": [
    "E.VALIDATION.SCHEMA",
    "E.VALIDATION.GRID",
    "E.DOMAIN.IMPROPER_SYSTEM",
    "E.RESOURCE.OUTPUT_TOO_LARGE"
  ],
  "warnings": [
    "W.DOMAIN.UNSTABLE_SYSTEM",
    "W.ACCURACY.GRID_TOO_COARSE",
    "W.DOMAIN.NOT_SETTLED"
  ],
  "mcp": {
    "tool_name": "control_lti_step",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  }
}
```

**Input schema**

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.lti.step/0.1.0/input",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "system",
    "time"
  ],
  "properties": {
    "system": {
      "$ref": "aeoi://schemas/common/0.1/LtiSystem"
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
            "t_final",
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
              ]
            },
            "unit": {
              "const": "s"
            }
          },
          "description": "auto 网格不参与 parity 测试"
        }
      ]
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
      }
    },
    "output": {
      "type": "object",
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
          "default": 10001
        }
      }
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.1/ExecOptions"
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

**成功响应**（1001 点网格计算后抽样为 11 点返回；指标为 python-control `step_info` 实算，网格 dt = 0.01 s）

```json
{
  "spec": "aeoi/0.1",
  "status": "ok",
  "operator": {
    "id": "control.lti.step",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
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
      },
      "decimated_from": 1001
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
          1.01863073016,
          1.29443084322,
          0.887503598772,
          0.952157825543,
          1.05125103645,
          0.995804521909,
          0.986086536164,
          1.00631455,
          1.00189833293,
          0.997408957089
        ]
      }
    },
    "info": {
      "rise_time": {
        "unit": "s",
        "value": 0.58146088
      },
      "settling_time": {
        "unit": "s",
        "value": 5.6983166
      },
      "overshoot": {
        "unit": "%",
        "value": 37.206794
      },
      "peak": {
        "unit": "1",
        "value": 1.372067944
      },
      "peak_time": {
        "unit": "s",
        "value": 1.6280905
      },
      "steady_state": {
        "unit": "1",
        "value": 1.0,
        "source": "dcgain"
      },
      "grid_resolution": {
        "unit": "s",
        "value": 0.01
      }
    }
  },
  "trust": {
    "convergence": {
      "applicable": false
    },
    "condition": {
      "norm1_A_dt": 0.04,
      "realization": "python-control tf2ss 默认实现",
      "scaling_squarings": 0
    },
    "residual": null,
    "error_estimate": {
      "y_abs": 1e-13,
      "basis": "expm 后向误差 ≤ u（Higham 2005）+ 递推误差累积 O(N·u)",
      "info_time_metrics_abs_s": 0.01,
      "info_basis": "时间类指标受网格分辨率 dt 限制（线性插值），误差 ≤ dt"
    },
    "accuracy_grade": "A",
    "credentials": "aeoi://operators/control.lti.step/0.1.0/credentials",
    "derived": {
      "dcgain": 1.0,
      "asymptotically_stable": true
    }
  },
  "warnings": [],
  "reproducibility": {
    "determinism": "bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:67b10a6f76e735d805f4c079ab1e7254602e568993403bfe5848070e6a2c1f3e",
    "output_hash": "sha256:f77f6064c49db12d4f852bf7e958f2774d2362fb78d34cba19425cef67270429",
    "platform": "x86_64-linux-gnu",
    "blas": "openblas-0.3.28 (pinned, OPENBLAS_CORETYPE=Haswell, threads=1)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.21,
    "illustrative": true
  },
  "reference_note": {
    "engine": "python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5",
    "note": "示例数值由 python-control 在 box 上实际计算，非 MATLAB 输出；timing 与 build 字段为示意"
  }
}
```

**错误请求与响应**（网格过密）

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
  "spec": "aeoi/0.1",
  "status": "error",
  "operator": {
    "id": "control.lti.step",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
  },
  "request_id": "req-step-002",
  "result": null,
  "error": {
    "code": "E.RESOURCE.OUTPUT_TOO_LARGE",
    "category": "resource",
    "retryable": true,
    "message": "请求的时间网格包含 100000001 个采样点，超过单次返回上限 1000000。",
    "agent_hint": "缩小 dt 或缩短 t_final，或只返回抽样后的曲线与 step_info。对该系统（最快极点 |p|≈2.0 rad/s），dt=0.01 s 已足够。",
    "details": {
      "n_samples": 100000001,
      "limit": 1000000,
      "fastest_pole_rad_s": 2.0,
      "path": "/time/dt"
    },
    "remediation": [
      {
        "id": "coarser_grid",
        "kind": "modify_input",
        "description": "dt 改为 0.01 s（≈ 最快时间常数的 1/50）",
        "confidence": "high",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "replace",
            "path": "/time/dt",
            "value": 0.01
          }
        ]
      },
      {
        "id": "decimate",
        "kind": "modify_input",
        "description": "保持 dt，但只返回 1001 个抽样点",
        "confidence": "medium",
        "requires_user_confirmation": false,
        "patch": [
          {
            "op": "add",
            "path": "/output",
            "value": {
              "return_samples": "decimated",
              "max_samples": 1001
            }
          }
        ]
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.01,
    "illustrative": true
  }
}
```

**Parity 测试规格**

```yaml
# Parity test spec — control.lti.step 0.1.0
operator: control.lti.step
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
generator: matlab/gen_golden_step.m   # 调用 step(sys, t) 时 t 必须显式给定均匀网格
golden_store: golden/control.lti.step/0.1.0/*.json
tolerance_class: T2
compare:
  y: {rtol: 1.0e-10, atol: 1.0e-12, scale: "max|y_ref|"}
  info.rise_time / settling_time / peak_time: {atol: "dt", note: "插值与阈值判定在网格上进行，允许 1 个网格步的差异"}
  info.overshoot: {atol_pct: 1.0e-6, note: "若峰值落在网格点上则应一致；否则放宽到 grid-limited"}
  info: {definition_check: "rise 10–90%，settling 2%，与参考实现 stepinfo 默认值一致；不同定义时不比较"}
case_families:
  - {name: siso_underdamped, n: 200}
  - {name: siso_overdamped_and_critically_damped, n: 100}
  - {name: mimo_ss_random_stable, n: 100}
  - {name: stiff_spread_poles, n: 50, params: "极点跨 6 个数量级"}
  - {name: unstable_system, n: 20, expect_warning: "W.DOMAIN.UNSTABLE_SYSTEM", note: "y 仍比较；info 期望为 null"}
  - {name: discrete_systems, n: 50}
excluded: ["auto 时间网格（参考实现的自动网格算法未公开且会变化，不作为 parity 对象）"]
```

### 10.3 `control.design.lqr`

示例系统：双积分器 A = [[0,1],[0,0]]，B = [[0],[1]]，Q = I，R = 1。解析解 K = [1, √3]，S = [[√3,1],[1,√3]]，闭环极点 −√3/2 ± 0.5j；python-control 结果与解析解一致到 ~1e-15。

**Manifest**

```json
{
  "spec": "aeoi/0.1",
  "id": "control.design.lqr",
  "version": "0.1.0",
  "title": "Continuous-time LQR",
  "description": "求解连续时间代数 Riccati 方程 (CARE)，返回最优状态反馈增益 K、Riccati 解 S 与闭环极点，使 J=∫(xᵀQx+uᵀRu+2xᵀNu)dt 最小。",
  "category": "design",
  "tags": [
    "optimal-control",
    "riccati",
    "lti"
  ],
  "applicability": {
    "preconditions": [
      "R 对称正定",
      "[[Q,N],[Nᵀ,R]] 对称半正定",
      "(A,B) 可镇定",
      "(Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在虚轴上无不可观模态"
    ],
    "not_for": [
      "离散系统（用 control.design.dlqr）",
      "有约束的 MPC"
    ],
    "siso_only": false
  },
  "provenance": {
    "algorithm": "Hamiltonian 矩阵（或扩展辛矩阵束）的有序实 Schur 分解，取稳定不变子空间",
    "references": [
      {
        "type": "paper",
        "cite": "A. J. Laub, A Schur method for solving algebraic Riccati equations, IEEE TAC 24(6), 1979",
        "doi": "10.1109/TAC.1979.1102178"
      },
      {
        "type": "paper",
        "cite": "W. F. Arnold, A. J. Laub, Generalized eigenproblem algorithms and software for algebraic Riccati equations, Proc. IEEE 72(12), 1984",
        "doi": "10.1109/PROC.1984.13083"
      },
      {
        "type": "textbook",
        "cite": "B. D. O. Anderson, J. B. Moore, Optimal Control: Linear Quadratic Methods, 1990"
      }
    ],
    "clean_room": {
      "declared": true,
      "sources": "仅公开文献与开源实现；MATLAB 仅用于生成对照数据"
    }
  },
  "backend": {
    "library": "SLICOT",
    "routine": "SB02OD",
    "version": "5.9",
    "binding": "ffi",
    "license": "BSD-3-Clause"
  },
  "determinism": "bitwise",
  "tolerance_class": "T2",
  "input_schema": {
    "$ref": "./input.schema.json"
  },
  "output_schema": {
    "$ref": "./output.schema.json"
  },
  "conformance": {
    "target": "L3",
    "current": "draft"
  },
  "accuracy_grade": {
    "target": "A",
    "current": "U"
  },
  "credentials": "aeoi://operators/control.design.lqr/0.1.0/credentials",
  "errors": [
    "E.VALIDATION.SHAPE",
    "E.VALIDATION.NOT_SYMMETRIC",
    "E.DOMAIN.R_NOT_POSITIVE_DEFINITE",
    "E.DOMAIN.NOT_STABILIZABLE",
    "E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG",
    "E.NUMERICAL.ILL_CONDITIONED"
  ],
  "warnings": [
    "W.NUMERICAL.LARGE_RESIDUAL",
    "W.NUMERICAL.NEAR_UNCONTROLLABLE",
    "W.VALIDATION.SYMMETRIZED"
  ],
  "mcp": {
    "tool_name": "control_design_lqr",
    "annotations": {
      "readOnlyHint": true,
      "idempotentHint": true,
      "openWorldHint": false
    }
  }
}
```

**Input schema**

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "aeoi://schemas/control.design.lqr/0.1.0/input",
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
      "$ref": "aeoi://schemas/common/0.1/Matrix"
    },
    "B": {
      "$ref": "aeoi://schemas/common/0.1/Matrix"
    },
    "Q": {
      "$ref": "aeoi://schemas/common/0.1/Matrix"
    },
    "R": {
      "$ref": "aeoi://schemas/common/0.1/Matrix"
    },
    "N": {
      "$ref": "aeoi://schemas/common/0.1/Matrix"
    },
    "symmetry_tolerance": {
      "type": "number",
      "default": 1e-12,
      "description": "相对容差；超出即 E.VALIDATION.NOT_SYMMETRIC，范围内对称化并给 W.VALIDATION.SYMMETRIZED"
    },
    "exec": {
      "$ref": "aeoi://schemas/common/0.1/ExecOptions"
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

**成功响应**

```json
{
  "spec": "aeoi/0.1",
  "status": "ok",
  "operator": {
    "id": "control.design.lqr",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
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
        1.73205080757
      ]
    },
    "S": {
      "dtype": "f64",
      "shape": [
        2,
        2
      ],
      "data": [
        1.73205080757,
        1.0,
        1.0,
        1.73205080757
      ]
    },
    "closed_loop_poles": {
      "dtype": "c128",
      "shape": [
        2
      ],
      "data": [
        [
          -0.866025403784,
          0.5
        ],
        [
          -0.866025403784,
          -0.5
        ]
      ]
    }
  },
  "trust": {
    "convergence": {
      "applicable": false,
      "note": "直接法（Schur），无迭代"
    },
    "residual": {
      "care_abs_fro": 2.22e-15,
      "care_rel": 7.85e-16,
      "definition": "‖AᵀS+SA−SBR⁻¹BᵀS+Q‖_F / ‖S‖_F"
    },
    "condition": {
      "cond_R": 1.0,
      "cond_S": 3.73205,
      "ctrb_min_singular_value": 1.0,
      "rcond_care": null
    },
    "error_estimate": {
      "S_rel": 1e-14,
      "basis": "rel_residual × cond 估计（非严格上界）"
    },
    "checks": {
      "S_symmetric": true,
      "S_psd": true,
      "closed_loop_stable": true
    },
    "accuracy_grade": "A",
    "credentials": "aeoi://operators/control.design.lqr/0.1.0/credentials"
  },
  "warnings": [],
  "reproducibility": {
    "determinism": "bitwise",
    "tolerance_class": "T2",
    "input_hash": "sha256:8553ca51b4b8b8dfd4917eb9b152bb872b624d477cd1f850fb9cadd797d7968c",
    "output_hash": "sha256:5446eb7bb158068eca6481416a1843999141a00b1f7141d69698ef3f9b8052be",
    "platform": "x86_64-linux-gnu",
    "blas": "openblas-0.3.28 (pinned, OPENBLAS_CORETYPE=Haswell, threads=1)",
    "threads": 1,
    "seed": null
  },
  "timing": {
    "wall_ms": 0.05,
    "illustrative": true
  },
  "reference_note": {
    "engine": "python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5",
    "note": "示例数值由 python-control 在 box 上实际计算，非 MATLAB 输出；timing 与 build 字段为示意"
  }
}
```

**错误请求与响应**（不可镇定：A = diag(1, −1)，B = [0;1]，不稳定模态 λ=1 不可控；可控性矩阵奇异值实算为 [1.414, 0]）

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
  "spec": "aeoi/0.1",
  "status": "error",
  "operator": {
    "id": "control.design.lqr",
    "version": "0.1.0",
    "build": "aeoi-control 0.1.0-dev+illustrative"
  },
  "request_id": "req-lqr-002",
  "result": null,
  "error": {
    "code": "E.DOMAIN.NOT_STABILIZABLE",
    "category": "domain",
    "retryable": false,
    "message": "(A,B) 不可镇定：特征值 λ=1（不稳定）对应的模态不受输入 B 影响，不存在使闭环稳定的状态反馈，CARE 无稳定化解。",
    "agent_hint": "这是模型结构问题，调 Q/R 无效。需要为该模态增加执行器（修改 B），或确认 A 是否录入有误。可先调用 control.lti.ctrb_decomp 查看不可控子空间。",
    "details": {
      "uncontrollable_eigenvalues": {
        "dtype": "c128",
        "shape": [
          1
        ],
        "data": [
          [
            1.0,
            0.0
          ]
        ]
      },
      "ctrb_singular_values": {
        "dtype": "f64",
        "shape": [
          2
        ],
        "data": [
          1.41421356237,
          0.0
        ]
      },
      "pbh_test": "rank([A−λI, B]) = 1 < 2 at λ=1",
      "path": "/B"
    },
    "remediation": [
      {
        "id": "inspect",
        "kind": "call_operator",
        "operator": "control.lti.ctrb_decomp",
        "arguments_from": {
          "A": "/A",
          "B": "/B"
        },
        "description": "做 Kalman 可控性分解以定位不可控模态",
        "confidence": "high",
        "requires_user_confirmation": false
      },
      {
        "id": "add_actuator",
        "kind": "modify_input",
        "description": "示例：为第一个状态增加一路输入（B 变为 2×2，R 同步扩为 2×2）",
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
                1,
                0,
                0,
                1
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
                1,
                0,
                0,
                1
              ]
            }
          }
        ]
      },
      {
        "id": "ask",
        "kind": "ask_user",
        "description": "向用户确认 A 矩阵与执行器配置"
      }
    ]
  },
  "warnings": [],
  "timing": {
    "wall_ms": 0.02,
    "illustrative": true
  }
}
```

**Parity 测试规格**

```yaml
# Parity test spec — control.design.lqr 0.1.0
operator: control.design.lqr
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
generator: matlab/gen_golden_lqr.m    # [K,S,P] = lqr(A,B,Q,R,N)
golden_store: golden/control.design.lqr/0.1.0/*.json
tolerance_class: T2
compare:
  S: {mode: normwise_rel, rtol: "1.0e-10 · max(1, cond_S/1e2)", note: "条件数缩放"}
  K: {mode: normwise_rel, rtol: "1.0e-10 · max(1, cond_S/1e2)"}
  closed_loop_poles: {mode: matched_set, matching: hungarian, rtol: 1.0e-9}
  invariants: ["CARE 相对残差 ≤ 1e-12", "S 对称半正定", "A−BK 稳定"]   # 不依赖参考实现，也必须通过
case_families:
  - {name: double_integrator_family, n: 20, note: "有解析解，可做 grade A 的第二参考"}
  - {name: random_stabilizable, n: 300, params: "n∈[2,50], m∈[1,5]"}
  - {name: with_cross_term_N, n: 100}
  - {name: ill_conditioned_R, n: 50, params: "cond(R)∈[1e6,1e12]", expect_warning: "W.NUMERICAL.ILL_CONDITIONED"}
  - {name: not_stabilizable, n: 20, expect: "E.DOMAIN.NOT_STABILIZABLE"}
  - {name: R_indefinite, n: 10, expect: "E.DOMAIN.R_NOT_POSITIVE_DEFINITE"}
```

## 11. 开放问题与下一步

### 11.1 开放问题

1. **大数组传输**：MCP 文本通道传 10⁶ 元素的 JSON 很低效。候选：resource link + 二进制（NPY/Arrow），或 base64 打包的 `data_b64` 字段。需在第一批仿真类算子上实测。
2. **单位的深度**：v0.1 只做声明与校验，不做量纲推导（如自动从 `A` 矩阵推出状态单位）。是否需要状态/输入/输出逐通道单位？
3. **时滞系统**：`InputDelay`/`OutputDelay`/内部时滞的表示与 Padé 近似策略未定义。
4. **相位分支约定**：不同实现的相位 unwrap 起点不同；规范是否强制 `branch_center_deg`，还是只在 parity 中 mod 360 比较？
5. **κ 缩放的容差**：T1/T2 用条件数缩放 rtol 的具体公式需要用真实 golden 数据标定。
6. **组合调用的可信度传播**：Agent 串联 `lqr → step` 时，上游误差如何传递到下游 trust？v0.1 不处理。
7. **会话/对象句柄**：是否允许"创建系统对象、返回句柄、后续按句柄调用"，以减少重复传大矩阵？这会破坏纯函数性，需权衡。
8. **MATLAB 版本漂移**：MATLAB 升级导致 golden 变化时，以哪个版本为准？建议凭证中锁定参考版本，并做版本间 diff 报告。
9. **许可合规**：MATLAB 许可条款对"用输出做对比测试"本身是否有限制，需要法务确认（对比输出 ≠ 逆向工程，但应书面确认）。
10. **Agent 可用性评估**：如何量化"错误信息与 remediation 对 Agent 有效"？建议建立一组 Agent 回归任务（故意给错输入，测自我修复成功率）。

### 11.2 下一步（建议顺序）

1. 用本草案实现 `aeoi-core` + 3 个示例算子（Rust，`bode`/`step` 走 faer 纯 Rust，`lqr` 走 SLICOT FFI），验证 trait 与信封是否顺手
2. 再接 5–7 个差异较大的控制算子：`tf2ss`/`ss2tf`、`margin`、`lsim`、`lyap`、`place`、`c2d`、`kalman`，专门暴露规范的盲点（非唯一输出、离散化方法选项、无穷值输出、大数组）
3. 在有授权的机器上写并运行 MATLAB golden 生成脚本，跑通第一份 A 级凭证
4. 做 Agent 回归评测：同一组工程任务，对比"Agent + AEOI 工具"与"Agent + 裸 scipy"的正确率与自我修复率
5. 根据以上反馈修订，冻结 v1.0；冻结前做一次相关标准/竞品的最新调研（MCP 规范更新、FMI 3.0、MathWorks/Wolfram 的 AI 接口）

## 附录 A：文件清单

```
ai-operator-spec/
├── spec-v0.1.md                     本文
├── schemas/
│   ├── common.schema.json           Matrix / Quantity / LtiSystem / ExecOptions
│   ├── manifest.schema.json
│   ├── envelope.schema.json
│   └── error.schema.json
├── examples/<id>/                   bode / step / lqr 各 8 个文件
│   ├── manifest.json  input.schema.json  output.schema.json
│   ├── request.json   response.success.json
│   ├── request.error.json  response.error.json
│   └── parity.yaml
├── rust/operator.rs                 Rust trait 草图（未编译）
├── matlab/gen_golden_bode.m         MATLAB golden 生成脚本草图（未在 MATLAB 中运行）
├── compute_examples.py              示例数值的实际计算脚本（python-control）
├── computed_values.json             上述脚本的原始输出
├── build_examples.py                由 computed_values.json 生成 examples/
├── validate_examples.py             用 jsonschema 校验全部示例（当前 18/18 通过）
└── spec-src.md                      本文源稿（{{file:…}} 占位符由脚本内联示例文件生成 spec-v0.1.md）
```
