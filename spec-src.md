# AI Engineering Operator Interface Spec（AEOI）v0.2-draft

> 状态：Draft v0.2 · 2026-11-21 · 作者：Qingfei Yuan（草拟协助：Grok Bot）
> 定位：叠加在 MCP 之上的"工程计算语义层"。MCP 负责传输与工具调用；AEOI 规定工程算子如何描述自己、如何表达带物理意义的输入输出、如何报告结果可信度、如何报错、如何复现、如何证明与参考实现一致。
> 冻结条件：用 5–10 个真实控制算子跑通后再升级为 v1.0。v0.x 期间任何条款都可能被修改。
> v0.2 相对 v0.1 的全部改动及原因见第 13 章 Changelog。v0.2 已有可运行的 Rust 参考实现（第 11 章）：`control.lti.bode`、`control.lti.step`、`control.design.lqr` 三个算子 + MCP stdio server，全部测试通过。

## 0. 约定

- 关键词 MUST / MUST NOT / SHOULD / SHOULD NOT / MAY 按 RFC 2119 / RFC 8174 解释。
- JSON Schema 一律使用 Draft 2020-12。完整 schema 文件见 `schemas/`，`$id` 形如 `aeoi://schemas/common/0.2`。
- 本文中 `spec` 版本标识写作 `aeoi/0.2`。
- **净室原则（normative）**：算子实现 MUST 仅依据公开文献、教科书、标准与开源许可兼容的代码；MUST NOT 反编译、反汇编、阅读或复制 MATLAB（或任何闭源软件）的实现代码、`.p` 文件、二进制或未公开文档。MATLAB 只作为黑盒 ground truth，通过其公开 API 生成对照数据。每个 manifest MUST 带 `provenance.clean_room.declared = true`。
- **示例数值说明**：第 10 章示例的响应由 Rust 参考实现（`aeoi-control 0.2.0`）实际输出（`regen_examples.py`）；其数值另与 python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5 在 box 上的计算结果比对（`cargo test`）。**两者都不是 MATLAB 输出。** `timing` 为 box 上的实测值，仅供参考。

> ### ⚠ 内存序（MUST，最常见的静默错误）
>
> **AEOI 的稠密数组一律是 row-major（C order）展平**：`shape [m,n]` 的矩阵，`data[i·n + j] = A(i,j)`。
> **MATLAB 与 Fortran 是 column-major。** 在 MATLAB 中写 `A(:)` 或 `reshape(A,1,[])` 得到的是列主序；对非对称矩阵这等于静默传入 Aᵀ —— schema 校验、shape 校验都无法发现，结果错误但"看起来正常"。
>
> - 从 MATLAB 生成 AEOI 数据 MUST 先转置再展平：`data = reshape(A.', 1, [])`；N 维数组用 `reshape(permute(X, ndims(X):-1:1), 1, [])`。
> - 从 NumPy 生成 MUST 使用 `A.ravel(order="C")`（或 `A.tolist()` 后按行展平）；Fortran 序数组（`A.flags.f_contiguous`）MUST 先 `np.ascontiguousarray`。
> - 二进制编码（`data_b64`、`MatrixRef`）同样是 row-major，`order` 字段 MUST 为 `"C"`（v0.2 不接受 `"F"`）。
> - 每个算子的 parity 测试集 MUST 至少包含一个**非对称矩阵**用例（case family `memory_order_nonsymmetric`），并 MUST 断言：把同一矩阵按 column-major 展平的请求，其结果与参考值的偏差远超容差（v0.2 参考实现实测为容差的 9.3×10⁹ – 5.6×10¹⁰ 倍）。对称矩阵（如 Q = I）无法暴露这类错误。
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

  "平台" = 目标三元组（`reproducibility.platform`）+ 运行时检测到的 CPU 特性集合（`reproducibility.cpu_features`）。v0.2 的三个算子都声明 `platform_bitwise`：nalgebra 的稠密乘法走 `matrixmultiply`，后者在运行时按 AVX/FMA 等特性选择内核。
- M-8 `tolerance_class` MUST 取 7.2 表中的一个值。
- M-9 **稀疏支持声明（v0.2 新增）**：manifest MUST 含 `sparse_support`，以输入字段的 JSON Pointer 为键，值为 `{"policy": "native" | "densify" | "reject", "max_dense_elements": <int>}`。未列出的矩阵字段视为 `reject`。`densify` 时算子 MUST 返回 `W.PERFORMANCE.SPARSE_DENSIFIED`；稠密化后元素数超过 `max_dense_elements` MUST 返回 `E.RESOURCE.INPUT_TOO_LARGE`。
- M-10 **编码与上限（v0.2 新增）**：manifest MUST 含 `encodings {input: [...], output: [...]}`（取值 `json`、`b64`、`ref`），以及 `limits`（算子特有的规模上限，如 `max_grid_points`、`max_states`）。上限检查 MUST 在主计算前完成。

### 2.2 片段（`control.design.lqr` 0.2.0 实际 manifest 节选）

```json
{
  "spec": "aeoi/0.2",
  "id": "control.design.lqr",
  "version": "0.2.0",
  "determinism": "platform_bitwise",
  "tolerance_class": "T2",
  "provenance": {
    "algorithm": "消去交叉项后构造 Hamiltonian 矩阵；带行列式缩放的 Newton 迭代计算矩阵符号函数，由 sign(H)+I 的零空间得稳定不变子空间并最小二乘解出 S；随后至多 3 步 Newton–Kleinman 修正",
    "references": [{ "type": "paper", "cite": "R. Byers, Solving the algebraic Riccati equation with the matrix sign function, LAA 85, 1987", "doi": "10.1016/0024-3795(87)90222-9" }, "…"],
    "clean_room": { "declared": true }
  },
  "backend": { "library": "aeoi-control (pure Rust) + nalgebra 0.33", "routine": "lqr::solve_care", "version": "0.2.0", "binding": "native", "license": "MIT OR Apache-2.0" },
  "sparse_support": { "/A": { "policy": "densify", "max_dense_elements": 1000000 }, "…": "…" },
  "encodings": { "input": ["json", "b64", "ref"], "output": ["json", "b64"] },
  "limits": { "max_states": 200, "max_states_for_refinement": 32, "max_sign_iterations": 100 }
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

### 4.2 信封骨架（字段取自参考实现的实际输出）

```json
{
  "spec": "aeoi/0.2",
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

## 5. 结构化错误

### 5.1 错误码分类

格式：`E.<CATEGORY>.<NAME>`。类别固定为 5 个；`NAME` 由注册表统一登记，算子 MUST 在 manifest `errors[]` 中列出可能返回的全部错误码（参考实现在 parity 测试中逐次断言这一点）。

| 类别 | 何时使用 | `retryable` 默认 | v0.2 已登记的码 |
|---|---|---|---|
| `VALIDATION` | 输入不符合 schema 或语义约束 | true（改输入） | `SCHEMA`、`SHAPE`、`DTYPE`、`RANGE`、`NON_FINITE`、`NOT_SYMMETRIC`、`NOT_CONJUGATE`、`ZERO_DENOMINATOR`、`GRID`、`SPARSE_STRUCTURE`、`DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN` |
| `DOMAIN` | 输入合法，但数学问题本身无解/不适用 | 视情况 | `IMPROPER_SYSTEM`、`NOT_STABILIZABLE`、`R_NOT_POSITIVE_DEFINITE`、`WEIGHT_NOT_PSD`、`HAMILTONIAN_IMAG_AXIS_EIG`、`ABOVE_NYQUIST` |
| `NUMERICAL` | 问题理论上可解，但数值上失败或不可信 | true（正则化/放宽/换算法） | `SINGULAR`、`NO_CONVERGENCE`、`POLE_ON_GRID` |
| `RESOURCE` | 超时、超规模、被取消 | true | `TIMEOUT`、`INPUT_TOO_LARGE`、`COMPUTE_LIMIT`、`OUTPUT_TOO_LARGE` |
| `INTERNAL` | 实现缺陷或后端异常 | false（可原样重试一次） | `PANIC`（`BACKEND`、`INVARIANT_VIOLATED` 为保留码，v0.2 算子未使用） |

v0.2 已登记的 warning：`W.NUMERICAL.{NEAR_POLE, ZERO_ON_GRID, ILL_CONDITIONED, LARGE_RESIDUAL, NEAR_UNCONTROLLABLE}`、`W.DOMAIN.{UNSTABLE_SYSTEM, NOT_SETTLED, INFINITE_DC_GAIN, ZERO_DC_GAIN}`、`W.ACCURACY.{GRID_TOO_COARSE, AUTO_GRID}`、`W.VALIDATION.{LEADING_ZEROS_STRIPPED, SYMMETRIZED}`、`W.PERFORMANCE.SPARSE_DENSIFIED`。

规则：
- X-1 `DOMAIN` 与 `NUMERICAL` MUST 区分清楚：前者告诉 Agent"别再调参了，问题本身不成立"，后者告诉 Agent"换个数值处理可能成功"。这是本规范对 Agent 最重要的区分。
- X-2 FFI 后端的每个返回码 MUST 映射到具体错误码；MUST NOT 把后端错误笼统报为 `INTERNAL.BACKEND`，除非确实无法归类。
- X-3 Rust `panic` MUST 被 `catch_unwind` 捕获并转为 `E.INTERNAL.PANIC`，FFI 边界 MUST NOT 让 panic 或 Fortran 异常跨越。
- X-4 数值"勉强成功"SHOULD 返回成功 + `W.NUMERICAL.ILL_CONDITIONED`，而不是错误；阈值 MUST 写在 manifest。
- X-4a **数值判据 MUST 与问题尺度无关（v0.2 新增）**：判断"特征值在虚轴上"、"矩阵奇异"等的阈值 MUST 以机器精度 u 与相关范数表达（如 `|Re λ| ≤ 10³·u·‖H‖₁`），MUST NOT 用与尺度无关的固定小数。（v0.2 实现初版用 `1e-8·‖H‖₁`，在 cond(R)=10⁹ 的合法问题上误报 `HAMILTONIAN_IMAG_AXIS_EIG`，被 parity 测试发现。）
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
- V-4 parity 测试 MUST 使用显式输入（显式频点、显式时间网格），MUST NOT 依赖参考实现的自动网格/自动选择逻辑。
- V-5 非唯一输出（特征向量、状态空间实现、Schur 基）MUST 比较不变量（特征值集合、传递函数、投影子），不得逐元素比较。
- V-6 比较公式：|x − x_ref| ≤ atol + rtol·|x_ref|；矩阵 SHOULD 用范数形式。**（v0.2 细化）** 病态问题的容差 MAY 按以下两种方式放宽，且 MUST 写入 parity spec、MUST 在测试报告中单独统计被放宽的用例数：
  - 条件数缩放：`rtol × max(1, κ/10²)`（`lqr` 用 κ = cond(S)）；
  - 误差估计缩放：`max(class_tol, 10 × trust.error_estimate)`——只有当算子自报的误差估计本身已经较大时才放宽（如 `bode` 的近极点频点）。这同时检验了误差估计不是"夸大的好消息"：实际偏差必须落在算子自报估计的 10 倍以内。
- V-7 预期报错用例 MUST 比对错误码；预期 warning 用例 MUST 断言 warning 出现。

### 7.2 容差类

| 类 | 算法类型 | 默认判据 | 说明 |
|---|---|---|---|
| T0 | 精确/整数/组合 | 完全相等 | 秩、阶次、结构判定 |
| T1 | 直接稠密线性代数 | rtol 1e-12（× κ 缩放） | 求解、分解、乘法 |
| T2 | 矩阵函数、特征值、Riccati/Lyapunov、LTI 精确离散化 | rtol 1e-10（× κ 缩放） | `expm`、`lqr`、`lyap`、ZOH `step` |
| T3 | 频率响应求值 | 幅值 1e-9 dB abs，相位 1e-8 deg（mod 360） | `bode`、`nyquist`、`freqresp` |
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

v0.2 参考实现的三个算子当前均为 **B 级**（python-control parity + 解析解单元测试 + 不变量），目标 A 级。

### 7.4 凭证记录

参考实现的 parity 测试会写出 `reports/parity-python-control.json`（每个算子：用例数、通过数、各量的最大误差与容差、各 case family 统计、被放宽的用例数、内存序回归结果、build、cpu_features）。它是凭证的原始材料；正式凭证还需要 MATLAB golden 与 CI 签名。

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

1. JSON Schema 校验（`jsonschema` crate，Draft 2020-12）→ 失败返回 `E.VALIDATION.SCHEMA`，`details.violations` 带最多 20 条 JSON Pointer
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
| CLI | `aeoi-call`（在 `aeoi-control` 中） | **已实现**：`aeoi-call <op-id|tool_name> <request.json|-> [--data-root DIR]` |
| Python | `aeoi-py`（PyO3 + maturin） | 计划：`aeoi.call(id, args) -> dict`；NumPy 便捷层 MUST 遵守内存序规则；MUST 释放 GIL |
| C ABI | `aeoi-capi`（cdylib） | 计划：`aeoi_call(const char* id, const char* json_args, char** json_envelope)` + `aeoi_free` |

- B-1 所有绑定 MUST 返回同一信封结构；绑定层 MUST NOT 自行改写数值或错误码。
- B-2 Python 绑定 MUST 释放 GIL 执行长计算。

## 9. 治理

### 9.1 合规等级

| 等级 | 要求 | 用途 |
|---|---|---|
| L1 Basic | manifest、严格 input/output schema、信封、结构化错误（含 remediation） | 实验性接入 |
| L2 Trusted | L1 + 完整 trust 元数据 + 可复现性（第 6 章全部 MUST）+ 重放测试 | 生产注册表最低要求 |
| L3 Verified | L2 + 验证凭证等级 A 或 B + CI 签名 + parity 测试持续运行 | 平台"verified"标识 |

参考实现三个算子当前为 L2（缺 CI 签名与凭证资源，故未到 L3）。

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

## 10. 完整示例

`bode` 与 `step` 使用同一被控对象，`lqr` 使用经典双积分器。**所有响应都是 Rust 参考实现的实际输出**（`regen_examples.py` 调用 `aeoi-call` 生成），并经 `cargo test` 与 python-control 结果比对；不是 MATLAB 输出。每个示例的完整文件在 `examples/<id>/`。

被控对象：G(s) = 4 / (s² + 1.2s + 4)（ωn = 2 rad/s，ζ = 0.3），极点 −0.6 ± 1.9079j。

### 10.1 `control.lti.bode`

语义要点：
- 频点：`explicit` 时 MUST 严格递增（相位 unwrap 依赖顺序；否则 `E.VALIDATION.GRID`，remediation 给出排序后的 patch）；`logspace` 时 `min < max`。输入可为 `Hz`，输出 `w` 一律 `rad/s`。离散系统频点 MUST ≤ π/ts（`E.DOMAIN.ABOVE_NYQUIST`）。
- 相位约定：`branch_center_deg = c`（默认 0）。`unwrap = true` 时首个频点相位取 (c−180, c+180] 内的主值，之后每点取与前一点之差的主值累加（与 `numpy.unwrap` 一致；实测 100 个成功 golden 用例的 unwrapped 相位与 python-control 逐点一致，不只是 mod 360 一致）；`unwrap = false` 时每点都落在该区间。MIMO 按通道独立 unwrap。
- 频点恰为极点：`E.NUMERICAL.POLE_ON_GRID`；恰为零点：幅值 `-Inf` dB + `W.NUMERICAL.ZERO_ON_GRID`；误差估计 > 1e-6 时 `W.NUMERICAL.NEAR_POLE`。
- 求值：tf 用 Horner（带 Higham 运行误差界），zpk 用乘积形式，ss 用逐频点复 LU 解 (jωI−A)x = B，n ≤ 64 时以 SVD 条件数给出启发式误差估计。

**Manifest** — `examples/control.lti.bode/manifest.json`

```json
{{file:examples/control.lti.bode/manifest.json}}
```

**Input schema** — `examples/control.lti.bode/input.schema.json`

```json
{{file:examples/control.lti.bode/input.schema.json}}
```

**请求**

```json
{{file:examples/control.lti.bode/request.json}}
```

**成功响应**（ω = 2 rad/s 处相位正好 −90°、幅值 +4.437 dB = 20·log10(1/(2ζ))，可作手算核对）

```json
{{file:examples/control.lti.bode/response.success.json}}
```

**错误请求与响应**（非 proper 系统；remediation 给出加滤波极点的 patch，需用户确认）

```json
{{file:examples/control.lti.bode/request.error.json}}
```

```json
{{file:examples/control.lti.bode/response.error.json}}
```

**非对称状态空间示例（内存序回归用例）** — `request.ss_nonsymmetric.json`，A 为伴随形 [[0,1,0],[0,0,1],[−6,−11,−6]]（row-major 展平为 `[0,1,0, 0,0,1, -6,-11,-6]`），响应见 `response.ss_nonsymmetric.json`。

```json
{{file:examples/control.lti.bode/request.ss_nonsymmetric.json}}
```

**Parity 测试规格** — `examples/control.lti.bode/parity.yaml`

```yaml
{{file:examples/control.lti.bode/parity.yaml}}
```

### 10.2 `control.lti.step`

语义要点：
- 对 LTI 系统，用 ZOH 精确离散化：增广矩阵 [[A, B],[0, 0]]·dt 的矩阵指数（Van Loan 1978；expm 用 Higham 2005 的缩放平方 + Padé 13）。阶跃输入在 ZOH 下没有离散化误差，因此归入 T2。
- 网格：`uniform` 时点数 N = floor(t_final/dt + 1e-9) + 1，t_k = k·dt。离散系统 MUST 满足 dt = ts（否则 `E.VALIDATION.GRID`）。N × nu > 10⁷ 返回 `E.RESOURCE.COMPUTE_LIMIT`（remediation：把 dt 放大到满足上限）；返回值个数 N × ny × nu > 10⁶ 返回 `E.RESOURCE.OUTPUT_TOO_LARGE`（remediation：抽样返回或只返回指标）。`return_samples: "decimated"` 时取下标 round(k(N−1)/(M−1))。
- **阶跃指标在网格上定义，不插值（v0.2 明确）**：上升时间 = 首个 y ≥ hi·y∞ 的网格时刻 − 首个 y ≥ lo·y∞ 的网格时刻；调节时间 = 最后一个 |y − y∞| ≥ thr·|y∞| 的网格点之后一个网格点的时刻；峰值取网格最大 |y|。定义与 python-control 0.10 `step_info` 一致，相对连续时间真值的误差 ≤ dt。y∞ 取 DC 增益。不稳定系统与零 DC 增益通道的 info 为 `null`（分别附 `W.DOMAIN.UNSTABLE_SYSTEM`、`W.DOMAIN.ZERO_DC_GAIN`）。MIMO 时 info 为 `[ny][nu]` 数组。
- trust：终点一致性（逐步递推的 x(t_end) 与一次性 expm 直接计算的 x(t_end) 的相对偏差）作为后验残差。

**Manifest**

```json
{{file:examples/control.lti.step/manifest.json}}
```

**Input schema**

```json
{{file:examples/control.lti.step/input.schema.json}}
```

**请求**

```json
{{file:examples/control.lti.step/request.json}}
```

**成功响应**（1001 点网格计算后抽样为 11 点返回；指标按 dt = 0.01 s 网格定义：上升时间 0.66 s、调节时间 5.62 s、超调 37.2318%、峰值时刻 1.65 s。v0.1 示例中的 0.5815 s / 5.698 s 来自 python-control 的自动网格，与请求的网格不符，是 v0.1 的错误）

```json
{{file:examples/control.lti.step/response.success.json}}
```

**错误请求与响应**（dt = 1e-7 s，计算量超限）

```json
{{file:examples/control.lti.step/request.error.json}}
```

```json
{{file:examples/control.lti.step/response.error.json}}
```

**Parity 测试规格**

```yaml
{{file:examples/control.lti.step/parity.yaml}}
```

### 10.3 `control.design.lqr`

算法与来源：nalgebra 没有有序实 Schur 分解，因此 v0.2 不采用 v0.1 计划的 Schur 方法（Laub 1979 / SLICOT `SB02OD`），改用：
1. 有交叉项 N 时先消去：Ā = A − BR⁻¹Nᵀ，Q̄ = Q − NR⁻¹Nᵀ；
2. 前置检查：R 的 Cholesky（`E.DOMAIN.R_NOT_POSITIVE_DEFINITE`）、[[Q,N],[Nᵀ,R]] 半正定（`E.DOMAIN.WEIGHT_NOT_PSD`）、PBH 可镇定性检验 σ_min([A−λI, B]) / ‖[A,B]‖_F ≤ 1e-10 即 `E.DOMAIN.NOT_STABILIZABLE`（< 1e-6 时 `W.NUMERICAL.NEAR_UNCONTROLLABLE`）、Hamiltonian 虚轴特征值检验 min|Re λ(H)| ≤ 10³·u·‖H‖₁ 即 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`；
3. 矩阵符号函数 Newton 迭代 Z ← (Z + Z⁻¹)/2，前期用行列式缩放（Byers 1987；Kenney–Laub 1995），收敛判据 ‖Z_{k+1}−Z_k‖₁/‖Z_{k+1}‖₁ ≤ 1e-13 或停滞；
4. 由 [sign(H) + I] 的列空间方程用 SVD 最小二乘解出 S（Roberts 1980），对称化；
5. n ≤ 32 时至多 3 步 Newton–Kleinman 修正（Kleinman 1968；Lyapunov 方程用 Kronecker 形式直接解），残差不降即停；
6. trust 报告 CARE 残差（绝对、相对、按各项范数缩放的相对残差）、cond(R)、cond(S)、PBH 裕度、基于 sep 的一阶后验误差估计（n ≤ 20）。

**Manifest**

```json
{{file:examples/control.design.lqr/manifest.json}}
```

**Input schema**

```json
{{file:examples/control.design.lqr/input.schema.json}}
```

**请求**

```json
{{file:examples/control.design.lqr/request.json}}
```

**成功响应**（解析解 K = [1, √3]，S = [[√3,1],[1,√3]]，闭环极点 −√3/2 ± 0.5j；实现结果与解析解的相对误差为 0）

```json
{{file:examples/control.design.lqr/response.success.json}}
```

**错误请求与响应**（不可镇定：A = diag(1, −1)，B = [0;1]，不稳定模态 λ=1 不可控）

```json
{{file:examples/control.design.lqr/request.error.json}}
```

```json
{{file:examples/control.design.lqr/response.error.json}}
```

**Parity 测试规格**

```yaml
{{file:examples/control.design.lqr/parity.yaml}}
```

## 11. Rust 参考实现（v0.2）

### 11.1 结构

```
rust/                         cargo workspace（resolver 3，Rust 1.85，纯 Rust，无 C/Fortran 依赖）
├── aeoi-core/                规范层：错误/warning、JCS 哈希、schema 内联与校验、数组解析（json/b64/ref/sparse）、
│                             LTI 解析与 tf→ss、线性代数（expm、Lyapunov、条件数）、Operator trait 与调用管线
├── aeoi-control/             三个算子 + aeoi-call CLI
│   ├── src/bode.rs  step.rs  lqr.rs
│   └── tests/parity.rs  examples.rs
└── aeoi-mcp/                 MCP stdio server + tests/e2e.rs
```

依赖：nalgebra 0.33、serde_json（`preserve_order`、`float_roundtrip`）、jsonschema 0.18（`draft202012`）、sha2、base64。schema 文件在编译期通过 `include_str!` 嵌入，与 `schemas/` 目录同源。

### 11.2 运行

```bash
cd ai-operator-spec/rust
cargo test --release                      # 全部测试（含 264 个 python-control golden 用例）
cargo test --release -p aeoi-control --test parity -- --nocapture   # 打印 parity 误差表，写 reports/parity-python-control.json
cargo build --release
./target/release/aeoi-call control.lti.bode ../examples/control.lti.bode/request.json
./target/release/aeoi-mcp [--data-root DIR] [--namespace control.lti]   # MCP stdio server
cd .. && python3 mcp_session_demo.py      # 脚本化 MCP 会话，记录到 reports/mcp-session.jsonl
python3 regen_examples.py                 # 用 Rust 输出重新生成 examples/*/response*.json
python3 gen_golden_pycontrol.py           # 重新生成 python-control golden（需 python-control/scipy）
python3 validate_examples.py              # 独立的 Python jsonschema 校验
```

MCP 客户端配置示例：`{"command": "/path/to/aeoi-mcp", "args": ["--data-root", "/path/to/shared"]}`。

### 11.3 测试结果（2026-11-21，x86_64-linux，cpu_features avx/avx2/fma/avx512f）

全部通过：aeoi-core 单元测试 16 个，aeoi-control 单元测试 10 个（含解析解：一阶系统 bode/step、标量 LQR），示例测试 4 个，parity 测试 1 个（覆盖 264 个 golden 用例），MCP 端到端测试 1 个；release 与 debug 构建都通过。独立 Python 校验器 32 项全部通过。

parity（参考 = python-control 0.10.2 / scipy 1.18.1，**不是 MATLAB**）：

| 算子 | 用例 | 通过 | 量 | 实测最大误差 | 容差 |
|---|---|---|---|---|---|
| bode | 103 | 103 | 幅值（dB，绝对） | 2.8e-13 | 1e-9 |
| | | | 相位（deg，mod 360） | 3.0e-12 | 1e-8 |
| | | | 近极点 3 例（容差按自报估计放宽） | 幅值 7.1e-15 | 5.8e-4 |
| step | 89 | 89 | y（相对 max\|y\|） | 1.2e-11 | 1e-10 |
| | | | 指标时间（单位 dt） | 0（全部同一网格点） | 1 dt |
| | | | 峰值/调节值等（相对） | 1.2e-11 | 1e-9 |
| | | | 超调/欠调（百分点） | 3.2e-10 | 1e-6 |
| lqr | 72 | 72 | S（Frobenius 相对） | 2.4e-6（cond(R)=10⁹ 用例）；其余 58/63 个成功用例 ≤ 1e-9 | 1e-10·max(1,cond(S)/100)，或 10×自报估计 |
| | | | K（Frobenius 相对） | 1.7e-6（同上） | S 容差 × 10 |
| | | | 闭环极点（集合匹配，相对） | 2.3e-5 | 1e-8·max(1,cond(S)/100) |
| | | | 不变量：缩放 CARE 残差 | 6.4e-17 | 1e-12 |

内存序回归：三个算子的 column-major 错误请求与参考值偏差分别为容差的 5.6e10（bode）、1.3e10（step）、9.3e9（lqr）倍，均被检出。

### 11.4 已知限制

- `lqr` 在 cond(R) ≥ 10⁸ 时比 scipy 不准：高精度（mpmath）残差显示 scipy 解的残差约 2.6e-8，本实现约 1.2e-5，相对差 ~2e-6。原因是 S·G·S 项（G = BR⁻¹Bᵀ 很大）的浮点残差已到舍入底，Newton–Kleinman 修正无法继续下降。算子会返回 `W.NUMERICAL.ILL_CONDITIONED`（cond(R) > 10⁸ 或估计偏大时），自报误差估计（2e-6）与实际偏差同量级。改进方向：基于广义特征值问题 / 平衡的有序 Schur 方法。
- `lqr` 的 sep 误差估计在部分随机用例上很悲观（例：实际 3.6e-8，估计 2e-3）。
- Newton–Kleinman 修正只在 n ≤ 32 时进行（Kronecker 形式 O(n⁶)）；n > 32 只用符号函数解。golden 中最大 n = 40。
- 只实现了 stdio 传输；未实现 Streamable HTTP、PyO3、C ABI 绑定；credentials 资源未提供。
- 稀疏输入一律稠密化（`densify`），没有原生稀疏算法。
- golden 规模小于 v0.1 parity.yaml 的计划数量（实际 264 例），且未跑 MATLAB；`matlab/gen_golden_bode.m` 未在 MATLAB 上运行过。
- 只在一台 x86_64-linux 机器上测试，跨平台（aarch64、无 AVX-512）的 `platform_bitwise` 行为未验证。
- b64/ref 编码只测试了 f64；c128/i64/bool 的二进制解码路径已实现但没有测试。`reject` 稀疏策略已定义但三个算子都未使用。

## 12. 开放问题与下一步

### 12.1 开放问题

1. ~~大数组传输~~（v0.2 已定义 b64 与 file:// 引用）；剩余：网络引用（HTTP/对象存储）的 SSRF 防护与凭据模型，以及基于 resource link 的大输出。
2. **单位的深度**：v0.2 只做声明与校验，不做量纲推导。是否需要状态/输入/输出逐通道单位？
3. **时滞系统**：`InputDelay`/`OutputDelay`/内部时滞的表示与 Padé 近似策略未定义。
4. ~~相位分支约定~~（v0.2 已用 `branch_center_deg` + numpy 一致的 unwrap 规则定义）；剩余：MATLAB `bode` 的分支选择与此是否一致，需 MATLAB golden 验证。
5. **κ 缩放的容差**：v0.2 给出了 `lqr` 的 cond(S) 缩放与"自报估计 × 10"两种规则；其合理性还需 MATLAB golden 与更大规模用例标定。
6. **组合调用的可信度传播**：Agent 串联 `lqr → step` 时，上游误差如何传递到下游 trust？未处理。
7. **会话/对象句柄**：是否允许"创建系统对象、返回句柄、后续按句柄调用"？会破坏纯函数性。
8. **MATLAB 版本漂移**：建议凭证中锁定参考版本，并做版本间 diff 报告。
9. **许可合规**：MATLAB 许可条款对"用输出做对比测试"本身是否有限制，需要法务确认。
10. **Agent 可用性评估**：建立一组 Agent 回归任务（故意给错输入，测自我修复成功率）。
11. **稀疏 vs 稠密的哈希等价**（新）：是否应让同一矩阵的稀疏/稠密表示 `input_hash` 相同？代价是哈希大型稀疏矩阵时需要逻辑稠密化。

### 12.2 下一步（建议顺序）

1. 在有授权的机器上运行 `matlab/gen_golden_bode.m`（并补 step/lqr 脚本），先核对 `memory_order_nonsymmetric` 用例与 python-control 一致，再跑通第一份 A 级凭证
2. 为 `lqr` 实现基于平衡 + 有序 Schur（或广义特征值）的求解路径，修复病态 R 的精度差距
3. 再接 5–7 个差异较大的控制算子：`tf2ss`/`ss2tf`、`margin`、`lsim`、`lyap`、`place`、`c2d`、`kalman`
4. PyO3 绑定 + CI（跨平台 parity、重放、签名凭证）
5. Agent 回归评测：同一组工程任务，对比"Agent + AEOI 工具"与"Agent + 裸 scipy"的正确率与自我修复率
6. 根据以上反馈修订，冻结 v1.0

## 13. Changelog

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
├── spec-v0.2.md                     本文（由 spec-src.md 内联示例文件生成）
├── spec-v0.1.md                     v0.1 原文（历史）
├── spec-src.md                      本文源稿（{{file:…}} 占位符）
├── schemas/                         common / manifest / envelope / error（/0.2）
├── examples/<id>/                   manifest、input/output schema、request(.error)、response(.success/.error)、parity.yaml
│                                    bode 另有 request/response.ss_nonsymmetric.json
├── golden/python-control/<op>/0.2.0/<sha256>.json + INDEX.json   264 个 python-control 参考用例（NOT MATLAB）
├── reports/parity-python-control.json   parity 测试报告（cargo test 生成）
├── reports/mcp-session.jsonl        脚本化 MCP 会话记录
├── rust/                            cargo workspace：aeoi-core、aeoi-control、aeoi-mcp（第 11 章）
├── matlab/gen_golden_bode.m         MATLAB golden 生成脚本（已修正内存序；未在 MATLAB 中运行）
├── write_manifests.py               examples/*/manifest 与 schema 的源
├── gen_golden_pycontrol.py          python-control golden 生成器
├── regen_examples.py                用 Rust 输出生成示例响应
├── validate_examples.py             独立 Python jsonschema 校验
├── mcp_session_demo.py              脚本化 MCP 会话
├── compute_examples.py / computed_values.json   v0.1 的 python-control 计算（examples 测试仍用其作对照）
└── legacy/                          v0.1 的 build_examples.py 与 spec 源稿
```
