# 只读规范写第二实现：发现的规范问题（Python，2026-11-25，规范 v0.4.18-draft）

范围：`signal.spectral.fft`（10.17）、`linalg.system.solve`（10.19）、`interp.grid.interp1`（10.20）、`calculus.quadrature.integral`（10.31）。
只读了 `spec-v0.4.md`、这四份 profile 及其链接的 `examples/<op>/`、`schemas/`、`golden/**/<op>/**`、`conformance/README.md`、
`conformance/aeoi_hash.py` 与旧的 `server.py`。没有读 `rust/`、`write_manifests.py`、`gen_golden_*.py`、`regen_examples.py`、
`conformance/rules_*.py`、`conformance/tests/`、`docs/reference-impl.md`，也没有读运行器 `aeoi_conformance.py` 的源码（只用了 `--help`）。

## 结果

`python3 -I conformance/aeoi_conformance.py run --impl "python3 conformance/impls/python_scipy/server.py" --operators <op>`，退出码均为 0：

| 算子 | golden 通过 / 失败 / 跳过 | method-policy | 默认值用例 | 引擎（运行器标注） |
|---|---|---|---|---|
| `control.math.lyap` | 43 / 0 / 0 | 1 | 3 | mpmath 12（独立），python-control 31（同源） |
| `signal.spectral.fft` | 44 / 0 / 0 | 1 | 14 | mpmath 6（独立），python-control 38（标为同源，见 PY-30） |
| `linalg.system.solve` | 30 / 0 / 0 | 1 | 3 | python-control 30 |
| `interp.grid.interp1` | 56 / 0 / 0 | 1 | 7 | python-control 56 |
| `calculus.quadrature.integral` | 19 / 0 / 0 | 1 | 9 | python-control 19（真值其实是 mpmath，见 PY-30） |
| 五个一起 | 233 / 0 / 0 | | | |

fft、linsolve、interp1 第一次运行就全部通过；integral 第一次有 3 个失败，原因是我自己的变量名遮蔽 bug，与规范无关。
能通过，有 3 处靠的是读 golden 请求才知道的约定（PY-1、PY-21、PY-26），只读 profile 会失败。

## 发现

严重度：**阻塞** = 只看规范写不出能通过套件的实现；**歧义** = 规范允许多种读法，结果或可信度声明会不同；**措辞** = 引用错、缺定义，不影响结果。
“发现方式”：读 = 读规范时发现；golden = 读 golden 请求时发现；运行 = 一致性运行或自测时发现。

### 通用（核心规范）

- **PY-1** 全部算子 · 10.x 各 profile、4.3 R-3、9.4 CF-4 · **阻塞** · 读
  profile 没有写算子版本号（`0.4.0`）。`fingerprint.request = H({operator, version, input_hash})`，CF-4 要求与参考实现相同，所以版本号是线上契约的一部分，却只出现在参考 manifest 和 golden 目录名里。我取了 golden 的 `operator_version`。建议每个 profile 第一行写“算子版本 x.y.z”。
- **PY-2** 全部算子 · 7.2、10.x · 歧义 · 读
  profile 不写容差类（T1）；只有 `parity.yaml` 有。manifest 的 `tolerance_class` 必填，我照抄 parity.yaml。
- **PY-3** 全部算子 · 7.3、E-4 `accuracy_grade` · 歧义 · 读
  没有说第二实现能否、按什么自评精度等级（凭证流程只描述了参考实现）。我填 `C`（目标 B），因为没有凭证。
- **PY-4** 全部算子 · 2.1 M-6 `binding` · 措辞 · 读
  `native`/`ffi` 只按 Rust 定义（“纯 Rust 写 native”）。Python 调 LAPACK（`dgtsv`、lyap 的 Bartels–Stewart）算哪种不清楚。我把纯 Python/numpy 写 `native`，lyap 写 `ffi`。
- **PY-5** 全部算子 · 3.8 SEC-4、3.6 L-3 · 措辞 · 读
  “未配置 data root 时 MUST 拒绝所有 ref 输入”，没有说用哪个码。我对 `file://` 报 `E.VALIDATION.REF_FORBIDDEN`，其他 scheme 报 `REF_UNSUPPORTED`。
- **PY-6** 全部算子 · 3.11 F-3 · 措辞 · 读
  缺必填字段时，叶子级违规的 `path` 指向父对象还是缺失字段，没有规定。我指向缺失字段（如 `/b`），与 X-8“指向出错的输入字段”一致。
- **PY-7** 全部算子 · 8.1 `outputSchema` · 措辞 · 读
  “信封 schema，`result` 替换为 `output_schema`”：照字面替换，错误信封（`result: null`）就不符合 outputSchema。我用 `anyOf [null, output_schema]`。
- **PY-8** 全部算子 · 5.1 `retryable` · 措辞 · 读
  `DOMAIN` 的缺省是“视情况”，每个 DOMAIN 码取什么没写。参考示例里 `UNDERDETERMINED` 是 `true`，我认为补方程是问用户，不是改输入重试，写了 `false`；`EXTRAPOLATION_REQUIRED` 写 `true`。套件不比较这个字段。
- **PY-9** 全部算子 · 4.1 E-9 · 措辞 · 读
  dry-run 信封的 `status`、要不要带 `input_hash` 没有规定（schema 只要求 trust/reproducibility 可为 null）。我返回 `status: ok`、`receipt/reproducibility: null`、`estimate`。
- **PY-10** 全部算子 · 4.3 R-2 · 措辞 · 读
  “没有数值时 `bound_kind` 写 `none_reported`”，与 manifest 声明 `bound_kind: none` 的关系不清（manifest schema 不允许 `none_reported`）。我的做法：manifest 为 `none` 时收据保持 `none`，其他情况没有数值才写 `none_reported`。
- **PY-11** 参考 manifest · 5.1 · 措辞 · 读
  参考 fft manifest 的 `errors[]` 含 `E.VALIDATION.ENCODING`、`E.RESOURCE.CANCELLED`，二者都不在 5.1 的登记表里。（只是读到；我的 manifest 只列实际会发的码。）

### 3.15 表达式语言（integral 用）

- **PY-12** integral · 3.15 EX-5 · 歧义 · 读
  “嵌套深度不超过 200”没有定义深度：语法树深度，还是括号/函数调用/一元符号的嵌套？按树深度，`x+x+…+x`（300 项，左结合）就会报 `too_deep`。我按括号、函数调用、一元符号与指数的嵌套计，扁平链不算。
- **PY-13** integral · 3.15 EX-6、5.2 X-5 · 歧义 · 读
  EX-6 要求 remediation 是 `modify_input`（改写该表达式）+ `ask_user`，而 X-5 要求 `modify_input` MUST 带 patch。一般的语法错误给不出正确的替换值。我只在能确定改写时给补丁（`**`→`^`，`×`→`*`，`²`→`^2` 等，`requires_user_confirmation: true`），其余只给 `ask_user`。
- **PY-14** integral · 3.15 EX-2、EX-6 · 措辞 · 读
  `bad_name` / `duplicate_name` 来自 `variable` 或 `params` 的键，不在表达式里：`path` 和 `position` 填什么没有规定。我填 `/variable` 或 `/params/<名字>`，`position: null`。函数名后面没有括号（`sin + 1`）算 `syntax` 还是 `unknown_name` 也没写，我报 `syntax`。

### 10.17 fft

- **PY-15** fft · 10.17 FT-6、4.3 R-2、9.4 CF-7 · 歧义 · 读
  FT-6 规定 `bound_kind = a_priori`，同时说常数 “c 取 10 作估计”（对 Bluestein 和混合基没有证明）。R-2 里 `a_priori` 是先验界，CF-7 还要求这类界 ≤ 验收容差。一个估计被归为先验界，声明的强度高于实际。我照 FT-6 写 `a_priori`（c = 10，用 Bluestein 时 30），basis 写明“非严格上界”。
- **PY-16** fft · 10.17 FT-4 · 歧义 · 读
  逆变换的输出是时域序列，FT-4 仍说“`result.frequency` 给出每个输出频点的频率”。逆变换时这一轴是什么没有说。我返回与正变换相同的 fftfreq 轴（套件接受，但对用户没有意义）。
- **PY-17** fft · 10.17 FT-6 · 措辞 · 读
  Parseval 与抽查公式里的 `sc` 没有定义（推测是本次实际所用方向的缩放因子 s 或 s′）；单边谱时抽查频点 {n−1, ⌊3n/4⌋, …} 不在输出里，怎么处理没写（我只抽查输出中存在的频点）；`frequency_resolution` 只出现在 output schema，profile 正文没有。

### 10.19 linalg.system.solve

- **PY-18** linsolve · 10.19 LS-6、3.10 O-1 · 歧义 · 读
  O-1 要求改变数学定义的选项在结果里回显；`refinement` 改变结果，但 LS-6 的输出清单没有它。我在 `result.refinement` 回显（output schema 没有 `additionalProperties: false`，所以合法）。
- **PY-19** linsolve · 10.19 LS-2 · 措辞 · 读
  精化的停止规则没有说清：“修正量没有减半时停止”时这一步的 δ 加不加；`refinement_steps` 算不算 δ = 0 的那一步。示例 “精化 1 步后逐位等于精确解” 暗示不算。我的做法是：没减半的 δ 不加，δ = 0 不计步。LS-2 里写的 Dot2 是要求还是参考实现的选择也不清楚；我用的是更精确的正确舍入内积（无误差乘积 + `math.fsum`）。
- **PY-20** linsolve · 10.19 LS-6 · 措辞 · 读
  b 为 [m, k] 时 `residual_norm` 的编码（JSON 数组还是 `Matrix`）没有规定，output schema 也没有类型。我按 3.10 R-1 用 `DenseMatrix [k]`。LS-1 只说复数 A 报 `DTYPE`，没说复数 b；我同样报 `DTYPE`。

### 10.20 interp.grid.interp1

- **PY-21** interp1 · 10.20 IP-1 · **阻塞** · golden
  IP-1：“x 为 [n]，严格递增，n ≥ 2。不满足报 `E.VALIDATION.GRID`”。golden `one_point`（n = 1）却期望 `E.VALIDATION.SHAPE`。只读 profile 会报 GRID，从而失败。我按 golden 报 SHAPE。
- **PY-22** interp1 · 10.20 IP-5 · 歧义 · 读
  `pieces_interpolate` 的“相对局部尺度”和 `spline_c2` 的“相对全局尺度”都没有定义。同一个阈值（64u、6.4e4·u）配不同的尺度，结论会翻转。我采用了参考示例响应 `trust.residual.definition` 里写的尺度。`local_bounds` 对 `extrapolate` 的范围外点是否适用也没写，我只检查范围内的点。
- **PY-23** interp1 · 10.20 IP-3 · 措辞 · 读
  “三个补丁都要求用户确认（R-2）”引用错了：R-2 是 3.3 的 NaN 编码，应为 5.2 X-6。`details` 的字段名（点数、第一个点、范围）也只写了含义，没写键名。IP-2 只写了 “previous、next” 两个名字，没给定义（我按区间定位规则加上节点命中规则实现）。

### 10.31 calculus.quadrature.integral

- **PY-24** integral · 10.31 IN-3 · **歧义（影响错误码）** · 读 + 运行
  “全局自适应 … 子区间上限 650（同 MATLAB）”有两种读法。(a) QUADPACK 式：每次二分误差最大的一个子区间，650 是子区间总数。(b) MATLAB/Shampine 式：每轮二分全部未收敛的子区间，650 只计未收敛的。对 golden `divergent_interior`（1/x² 在 [−1, 1] 上），按 (a) 一侧会被连续二分约 512 次，直到 1/x² 溢出，报 `NON_FINITE_EVALUATION`（与 golden 相同，在同一个 x = −4.894e−155 处）。按 (b) 读法，结果取决于“收敛的子区间”怎么判，可能得到 `NO_CONVERGENCE`。我选了 (a)，因为“全局”和“误差公式同 QUADPACK”都指向它。规范应写明选哪个子区间二分、650 怎么数、相等时怎么选。
- **PY-25** integral · 10.31 IN-3 · 措辞 · 读
  “(−∞, b] 用 a ± t/(1 − t)”没有写 t 的范围和正负号；“初始 10 段”没说是在 t 上等分还是在 x 上等分。我用 x = b − t/(1 − t)，t ∈ [0, 1)，并在 t 上等分。
- **PY-26** integral · 10.31 IN-1 · **阻塞** · golden
  golden `same_infinity`（limits = ["Inf", "Inf"]）期望 `E.VALIDATION.RANGE`。profile 没有这条规则（IN-1 只说 a > b 取负），只读 profile 的实现会返回 0 或其他结果。我按 golden 实现。有限的 a = b 返回 0，规范同样没写。
- **PY-27** integral · 10.31 IN-5 · 歧义 · 运行
  IN-5 给的修复是“把 rel_tol 放宽到 1e-6 的补丁”。我把它套到发散的 golden 用例 `divergent_half_line`（1/x 在 [1, ∞) 上）上，算子“收敛”到 36.62（`ok_with_warnings`）。只有 IN-4 的 `refinement_consistent` 失败、claim 为 `unverified`，才让它没有冒充成功。CF-9 只要求“套用补丁后通过校验”，所以套件会接受这个结果。建议：没有奇点证据时 NO_CONVERGENCE 不给放宽补丁，或者在 profile 里写明放宽后 MUST 检查 `refinement_consistent`。
- **PY-28** integral · 10.31 IN-1、3.11 F-5(a)、F-7 · 歧义 · 读
  input schema 把 `variable` 标为 `x-aeoi-provenance: "user"`，同时给了 `default: "x"`。F-5(a) 规定 `user` 字段 MUST NOT 有默认值。`limits`、`params` 也标为 `user`，按 F-5(c) 绑定层要为它们附 `_user_quote`；“Inf” 或积分变量名很难从用户原话里逐字摘出。按含义，`variable` 应为 `convention`，`limits`/`params` 应为 `supplied`。
- **PY-29** integral · 10.31 IN-2、IN-4 · 措辞 · 读
  schema 允许 `rel_tol = abs_tol = 0`，这时停止条件永远不成立（除非误差恰为 0），profile 没说要报错还是跑到上限；我跑到上限后报 `NO_CONVERGENCE`。profile 只命名了 `refinement_consistent` 一个后置条件；其余后置条件的键（我用 `tolerance_met`、`all_finite`）由实现自定，所以不同实现的 claim 不可比（另见 PY-32）。被积函数值有限、乘上变量替换的导数后溢出时报什么没有写（我报 `NON_FINITE_EVALUATION`）。

### 一致性套件（不是规范本身）

- **PY-30** 一致性报告 · 9.4 CF-11、7.1 V-3 · 歧义 · 运行
  运行器把 `golden/python-control/` 下的用例全部标为 “same backend”。我把 fft 的 `backend.library` 改成不含 numpy 的字符串重跑，结果不变。这个标注与实现无关，只看 golden 目录。实际情况是：fft 的 golden 来自 numpy pocketfft，而我的 FFT 是自写的，不同源；integral 比较的是 50 位 mpmath 真值（`reference_env.engine` 写的是 “scipy.integrate.quad / numpy.trapezoid / mpmath.quad”），却也标为同源。CF-11 要求按 `reference_env.engine` 和 `backend.library` 判定，但没有给出机械规则。
- **PY-31** 套件 · 9.4 CF-8 · 歧义 · 运行
  我的 linsolve 第一版方法 id 含 `|`，不符合 `manifest.schema.json` 的 `^[a-z0-9_.:+-]+$`，运行器却全部判通过（我用 jsonschema 自查才发现，已改）。CF-8 要求 manifest 通过 `manifest.schema.json`，运行器至少没有校验 `receipt.method_id` 的模式。
- **PY-32** 全部算子 · 4.1 E-11、9.4 CF-8 · 歧义 · 读
  claim 由运行器“按被测实现自己的 manifest 后置条件”重推。profile 只点名了部分后置条件（FT-6 两项、LS-4 一项、IP-5 四项、IN-4 一项）。实现可以少列后置条件，从而更容易得到 `verified`。规范没有说 profile 点名的后置条件是不是每个实现 MUST 声明的最小集合。我声明了 profile 点名的全部后置条件，另加 `all_finite`。

## 无法只从规范推出的 manifest 字段

| 字段 | 情况 |
|---|---|
| `version` | profile 没写，取自 golden 目录名（PY-1） |
| `tolerance_class` | profile 没写，取自 parity.yaml（PY-2） |
| `accuracy_grade`、`conformance` | 没有第二实现的评定规则，自填 C / L1（PY-3） |
| `backend.binding` | Python 的取值无定义（PY-4） |
| `receipt.postconditions` | profile 点名的照写；`all_finite`、`tolerance_met` 是我加的（PY-29、PY-32） |
| `receipt.coverage` | 只有 FT-6 写了 `sampled`；其余全部按 `full`（缺省） |
| `receipt.guards` | 只有 LS-4 写了 `/x`；其余由我按“该检查保护哪个输出”自定 |
| `receipt.assumptions` | 条目、id、措辞全部自定；规范只规定格式与 `enforced_by` 的取值 |
| `receipt.exactness.bound_kind` | 只有 FT-6 规定（`a_priori`）；linsolve、integral 自定为 `a_posteriori_heuristic`，interp1 为 `none` |
| `limits` 的键与数值 | 全部自定（规范只要求有这个字段，L-6 给了单数组的默认上限） |
| `sparse_support` | 自定为 `reject`（M-9 允许） |
| `encodings` | 自定（不支持 `ref`，因为没有 data root；integral 没有数组） |
| `applicability.preconditions`、`not_for`、`category`、`tags`、`title`、`description` | 自写 |
| `provenance.references` | 自选公开文献（M-5 只要求至少一条） |
| `input_schema`、`output_schema` | 用 profile 链接的 `examples/<op>/*.schema.json`，内联 `$ref` 后放入（M-1、J-3）；profile 正文不足以写出它们（例如 fft `n ≥ 1`、integral `params` 的键模式只在 schema 里） |
| `errors[]`、`warnings[]` | 按实际会发的码自列 |
| lyap 的整份 manifest | 仍是参考 manifest 替换方法、后端、确定性、等级与精确性；它早于“只读规范”的要求，没有重写 |

## 处理结果（v0.4.19-draft，2026-11-25）

每条发现都先对照参考实现的实际行为核实过，再决定改规范、改参考实现还是列为开放问题。

| 发现 | 处理 |
|---|---|
| PY-1 | 每个 profile 开头写明算子版本与容差类；第 10 章规定 MUST 写 |
| PY-2 | 同上（容差类） |
| PY-3 | 7.3：没有凭证的实现 MUST 报 C 或 U |
| PY-4 | 2.1 M-6：`native` / `ffi` 按“数值核心是否调用另一种语言编译的库”定义，不再只针对 Rust |
| PY-5、6、8、9、10、13、14 | 列为开放问题 28 |
| PY-7 | 8.1：`outputSchema` 的 `result` 为 `oneOf [null, output_schema]`（参考实现本来就这样做） |
| PY-11 | 参考 manifest 去掉从未发出、也未登记的 `E.VALIDATION.ENCODING`、`E.RESOURCE.CANCELLED`；接受 b64 / ref 的数值算子改为声明核心实际会发的 `DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN` |
| PY-12 | 3.15 EX-5 定义深度（括号、函数参数、一元正负号、指数各加 1，同层连写不加）。参考实现原先按解析器递归层数计（约 40 层括号就报 `too_deep`），已改成这个定义，并加了单元测试 |
| PY-15 | 列为开放问题 30 |
| PY-16 | FT-4：逆变换时 `frequency` 是输入频谱的频率轴 |
| PY-17 | FT-6：定义 sc；抽查频点只取输出范围内的 |
| PY-18、20 | LS-6：`refinement` 回显在 `trust.derived`（不改变定义）；`residual_norm` 的编码；复数 A 或 b 都报 `DTYPE` |
| PY-19 | LS-2：停止与计步规则写成算法（δ = 0 也算一步，与参考实现一致）；补偿算术是精度下限，正确舍入内积也满足。本实现已按此改了计步 |
| PY-21 | IP-1：n < 2 报 `SHAPE`（参考实现与 golden 一直如此，profile 写错了） |
| PY-22 | IP-5：写出局部尺度与全局尺度的公式；`local_bounds` 只查范围内的点 |
| PY-23 | IP-3 引用改为 X-6，`details` 键名明确不作规定；IP-2 给出 previous / next 的定义 |
| PY-24、25 | IN-3：每步只二分误差最大的一个子区间（相等取 t 最小），650 是子区间总数；四种变量替换的 t 范围；初始在 t 上等分 |
| PY-26 | IN-1：同一个无穷大报 `RANGE`；有限 a = b 结果为 0 |
| PY-27 | 列为开放问题 29 |
| PY-28 | F-7：`variable` 改为 `convention`；积分限、求根区间、初值、参数、优化约束数据、阶数、滤波器类型、多项式次数改为 `supplied`。`user` 只留测量值（`ts`、`sample_rate_hz`、截止频率、`wc_rad_s`、`dx`） |
| PY-29 | IN-2：两个容差都为 0 时的行为；IN-5：乘以 dx/dt 后溢出也报 `NON_FINITE_EVALUATION` |
| PY-30 | 9.4 CF-11 给出同源的机械判定（用例引擎里的库例程名作为完整词出现在 `backend` 里）；运行器按此实现，不再因为 `aeoi-pyscipy` 含 “scipy” 就判同源 |
| PY-31 | 运行器按 CF-8 用 `manifest.schema.json` 校验每个 manifest，不通过时该算子全部用例判失败 |
| PY-32 | 新条款 4.1 E-11a：profile 点名的后置条件是每个实现 MUST 声明的最小集合（运行器还没有机械检查） |
