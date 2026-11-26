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

## 第二轮（control.lti.bode、step、convert，fit.poly.roots）

2026-11-26，规范 v0.4.20-draft。范围：`control.lti.bode`（10.1）、`control.lti.step`（10.2）、`control.lti.convert`（10.4）、`fit.poly.roots`（10.34）。
三个控制算子共用新模块 `aeoi_lti.py`（`LtiSystem` 的解析、校验、表示转换、频率响应、极点、DC 增益、improper 报错）；数值内核在 `aeoi_kernels.py`（自写 Parlett–Reinsch 平衡、Householder Hessenberg、Francis 双位移 QR、实/复 LU、Padé 缩放平方 expm、友矩阵求根）。结果路径上没有 python-control、scipy.signal、numpy.roots、LAPACK 特征值。
只读了 `spec-v0.4.md`、这四份 profile 与其链接的 `examples/<op>/`（含 manifest.json、parity.yaml）、`schemas/`、`golden/**/<op>/**`、`conformance/README.md`、`conformance/aeoi_hash.py`、本目录。没有读 `rust/`、`write_manifests.py`、`gen_golden_*.py`、`regen_examples.py`、`conformance/rules_*.py`、`conformance/tests/`（只运行了 `test_runner_catches_mutants.py` 看退出码）、`docs/reference-impl.md`、`roadmap/`，也没有读运行器源码。

### 结果

| 算子 | golden 通过 / 失败 / 跳过 | method-policy | 默认值用例 | 退出码 |
|---|---|---|---|---|
| `control.lti.bode` | 103 / 0 / 0 | 1 | 0 | 0 |
| `control.lti.step` | 89 / 0 / 0 | 1 | 0 | 0 |
| `control.lti.convert` | 2 / 0 / 74（套件没有 convert 的数值规则，见 PY-60） | 1 | 3 | 0 |
| `fit.poly.roots` | 20 / 0 / 0 | 1 | 0 | 0 |
| 九个算子一起 | 454 / 0 / 74 | | | 0 |

bode、step、convert、roots 第一次运行就全部通过（convert 的 74 个跳过除外）。能一次通过，有两处靠的不是 profile 正文：step 的指标细节靠我对 python-control `step_info` 源码的了解（PY-50，只读 profile 会失败 40 / 89），convert 的 MIMO ss→tf 警告靠 parity.yaml 与 golden（PY-57，只读 profile 正文会失败 8 个）。两处都做了反向实验确认：把实现改回 profile 正文的读法再跑套件。convert 的数值另用 parity.yaml 的 `compare` 公式自查：74 个成功用例全部在容差内（最坏用掉容差的 7.3e-4），2 个错误用例的码相同。

标 **[LTI]** 的条目针对 3.1 T-5/T-6、3.10 R-5 – R-9、D-1/D-2 的共享 LTI 表示，影响所有控制算子（bode、step、convert，以及 poles_zeros、margin、c2d、impulse、lsim、stepinfo、组合算子等）。

### 共享的 LTI 表示

- **PY-33** **[LTI]** 全部控制算子 · 3.1 表格与 T-5、`common.schema.json#/$defs/LtiSystem` · 措辞 · 读
  3.1 写“`ts` 与 `time_unit` 必填”，schema 只要求 `ts`，`time_unit` 是带默认值 `"s"` 的 `const`。我按 schema 处理（省略视为 `s`），返回的系统总是写出 `time_unit`。
- **PY-34** **[LTI]** 全部控制算子 · 3.3 R-2、`$defs/Polynomial`、`$defs/Complex` · 措辞 · 读
  R-2 规定输入里的非有限值报 `E.VALIDATION.NON_FINITE`，而 tf 系数、zpk 零极点在 schema 里是 `number`，字符串 `"NaN"` 先在 schema 层失败，得到 `E.VALIDATION.SCHEMA`。同一个 NaN，在 ss 的 `Matrix` 里是 `NON_FINITE`，在 tf 里是 `SCHEMA`。我照 schema 走（tf/zpk 报 SCHEMA）。
- **PY-35** **[LTI]** 全部控制算子 · 3.1 T-5（共轭配对，相对容差 1e-12） · 措辞 · 读
  “相对容差 1e-12”没有说相对于什么（|z|？max(1,|z|)？），也没有说配对是否一一对应（两个 1+2j 配一个 1−2j 算不算合格），虚部极小（如 1e-300）的值是否也要配对。我的做法：虚部非零的每个值，按 |w − z̄| ≤ 1e-12·|z| 找一个尚未用过的伙伴，一一配对；失败报 `NOT_CONJUGATE` 并给“补上共轭”的补丁。
- **PY-36** **[LTI]** 全部控制算子 · 3.10 R-5、T-5 · 措辞 · 读
  `num`/`den` 写成 [1][1] 嵌套数组的 tf 算 SISO 还是 MIMO，没有规定；返回 tf 时用平铺还是嵌套也只说了“MIMO 用嵌套”。我按维数判断：1×1 一律当 SISO，返回平铺形式。
- **PY-37** **[LTI]** 全部控制算子 · 3.1 T-5、3.10 R-6 · 措辞 · 读
  MIMO tf→ss“逐通道块对角”没有规定块的顺序（按 (i, j) 行主序还是列主序），也没有规定每块用哪一列 B、哪一行 C。只比较不变量时不影响结果，但返回的 `A` 不同实现之间不同。我按 (i, j) 行主序排块。
- **PY-38** **[LTI]** convert、step、bode（zpk 输入）· 3.10 R-6 · 歧义 · 读
  R-6 的 `realization` 枚举里没有 zpk→ss 的条目。step 与 convert 遇到 zpk 都要先得到状态空间实现。我先乘积展开成 tf，再用可控标准型，`realization` 写 `controllable_canonical`。也可以按每个实极点、每对共轭极点做级联实现（数值上更好），但枚举里没有对应的值。
- **PY-39** **[LTI]** step（以及 stepinfo、loopsens、margin）· 3.10 D-2、3.12 I-12、X-4a · 歧义 · 读
  “渐近稳定：连续 Re λ < 0，离散 |λ| < 1”是对精确特征值的定义。算出的特征值带舍入：积分器 1/s 的可控标准型特征值可能是 ±1e-17。X-4a 要求阈值与尺度无关并用 u 表示，但这里没有给出阈值。我的做法：|Re λ| ≤ 100·u·max(‖A‖₁, 1)（离散：|λ| ≥ 1 − 同一量）算作不是渐近稳定。
- **PY-40** **[LTI]** step · 10.2、3.12 I-12、D-2 · 歧义 · 读
  “零 DC 增益通道”没有数值判据。tf/zpk 输入时 DC 增益可以精确为 0；ss 输入时 D − CA⁻¹B 是算出来的，真 0 会变成 1e-17 之类。我的做法：tf/zpk 要求精确为 0；ss 用 |g| ≤ 1e3·n·u·(|C||A⁻¹B| + |D|)。
- **PY-41** **[LTI]** convert（ss→zpk），以及 poles_zeros · 3.10 R-8 · 歧义 · 读
  “k 取首个**非零** Markov 参数 C·A^{r−1}·B”。在浮点下，理论上为 0 的 CB 可能算成 1e-17，这时 r 与 k 都会错（k 差十几个数量级，零点个数也错）。规范没有给判断“非零”的阈值（X-4a 要求有）。我用 |h_i| > 1e3·n·u·‖C‖·‖A^{i−1}B‖。另外 R-8 只规定了 k，零点的算法（R-9 的“法秩判定在两个确定性的一般位置点上”）只说了退化判定，没有说怎么求不变零点；我对 SISO 用零动态 N^T(A − B h_r⁻¹ C A^r)N 的特征值，D ≠ 0 时用 eig(A − BC/d)。
- **PY-42** **[LTI]** convert（ss→zpk）、poles_zeros · 3.10 R-9 与 10.4 · **矛盾** · 读
  R-9：系统矩阵不是满法秩（“例如传递函数恒为零”）时 MUST 返回 `E.DOMAIN.DEGENERATE_SYSTEM`，MUST NOT 返回空集合冒充“没有零点”。10.4：“传递函数恒为零的 ss→zpk 返回 k = 0、无零点（与 python-control 一致），结构退化 → `DEGENERATE_SYSTEM`”。对 SISO，法秩不足恰好等价于 G ≡ 0，所以 10.4 里的“结构退化”在 SISO 上没有剩下的情形。我按 10.4（返回 k = 0、`z: []`），manifest 不声明 `DEGENERATE_SYSTEM`，因为我从不发它。
- **PY-43** **[LTI]** convert · 3.1 T-6、3.10 R-7 · 措辞 · 读
  T-6 说去掉前导零 MUST 发 `W.VALIDATION.LEADING_ZEROS_STRIPPED`；R-7 的 ss→tf 在 D = 0 时也“去除前导零”。示例 `response.nonsymmetric.json` 没有这个警告，所以 T-6 只管输入。规范没写明。我只对输入发。另外 R-7 的 `normalize` 只对 tf→tf 有作用（ss→tf、zpk→tf 的分母本来就是首一），`normalize = false` 时 ss→tf 是否应保留别的首项，没有说。
- **PY-44** **[LTI]** bode、step、convert · 10.1、10.2、10.4 全文；4.1 E-11a · 歧义 · 读
  这三份 profile 没有条款编号（只有“语义要点”），因此无法像 RT-3、FT-6 那样引用；也没有点名任何后置条件。E-11a 的“最小集合”于是为空：bode、step 的后置条件只出现在参考 manifest 里（`all_finite`、`frequencies_strictly_increasing`），convert 的 `response_consistent` 只出现在核心规范 E-12 的举例里。我照这些名字声明了（见下表），但它们不是 profile 要求的。

### 10.1 control.lti.bode

- **PY-45** bode · 10.1“误差估计 > 1e-6 时 `W.NUMERICAL.NEAR_POLE`” · 歧义 · 读
  没有说是哪一个误差估计：相对误差、dB 绝对误差（≈ 8.686 × 相对）还是相位度数（≈ 57.3 × 相对）。三者阈值相同、尺度差 1–2 个数量级。参考示例的 `error_estimate` 三个都有。我用相对误差 `rel_max`。ss 的估计 profile 写“SVD 条件数”，我用 1-范数条件数（由批量 LU 求显式逆），在 basis 里写明。
- **PY-46** bode · 10.1 相位约定 · 歧义 · 读
  频点恰为零点时相位是 `NaN`（output schema），但 unwrap 规则“每点取与前一点之差的主值累加”遇到 NaN 怎么接下去没有规定：下一点相对哪一点展开？首点就是 NaN 时“首个频点”指哪个？我的做法：跳过 NaN，下一个有限点相对上一个有限点展开；第一个有限点落在 (c−180, c+180]。
- **PY-47** bode · 10.1 频点、`input.schema.json` logspace · 歧义 · 读
  logspace 的公式没写（`10^(a + k(b−a)/(n−1))`，还是端点精确取 min、max）；`unit = Hz` 时是先在 Hz 上取对数网格再乘 2π，还是先换算再取网格（结果差几个 ulp，而 `w` 按相对 1e-14 比较）。golden 里 logspace 只出现在报错用例，所以套件没有检验。我用前一种公式，Hz 在最后乘 2π。
- **PY-48** bode · 10.1“频点恰为极点：`E.NUMERICAL.POLE_ON_GRID`” · 歧义 · 读
  “恰为”是浮点意义上分母精确为 0，还是带容差？ss 表示下等价于消去时出现精确 0 主元吗？我按精确：tf 分母值 == 0、zpk 某个 s − p == 0、ss 出现 0 主元。离极点 1e-300 的频点会走 `NEAR_POLE` 而不是报错。另外 explicit 频点无序时“remediation 给出排序后的 patch”：有重复频点时只排序的补丁套用后仍然报 GRID（CF-9 要求套用后通过），所以我的补丁排序并去重。
- **PY-49** bode · 4.3 R-2、9.4 CF-7 · 歧义 · 读
  参考 manifest 的 `bound_kind` 是 `a_priori`；10.1 自己说 ss 路径的估计是“启发式”。`bound_kind` 是每个方法一个值，同一方法的 tf 路径（一阶 Horner 先验界）与 ss 路径（启发式）强度不同，枚举里也没有“先验但非严格”。CF-7 又要求 `a_priori` 的 bound 不大于“该用例的验收容差”，而 bode 的验收容差逐频点随 κ(ω) 变化，收据的 bound 是全网格最大值：两者怎么比没有说。我写 `a_priori`，basis 写明 ss 是启发式（与 PY-15 / 开放问题 30 同类）。

### 10.2 control.lti.step

- **PY-50** step · 10.2 第三条“定义与 python-control 0.10 `step_info` 一致” · **阻塞** · 读 + 运行
  profile 只定义了上升时间、调节时间、峰值（与 3.12 I-10）。`settling_min`、`settling_max`、`overshoot`、`undershoot` 没有定义，只能去看 python-control 的源码。关键细节是 `settling_min = min(min(y[i_hi:]), y∞)`、`settling_max = max(max(y[i_hi:]), y∞)`（包括 y∞），以及 `undershoot` 只在与 y∞ 反号时非零、`overshoot` 用 max(sgn·y)。我凭对 python-control 公开源码的了解实现（开源，净室规则允许，但它正是 golden 引擎）。反向实验：把 `settling_min/max` 改成只取 y[i_hi:] 的最小/最大，step 失败 40 / 89（“info values relative ≤ 1e-9”）。规范应写出全部指标的公式。按 CF-11 的机械判定这些用例算“独立通过”，但定义本身与 golden 同源。
- **PY-51** step · 10.2、`output.schema.json` 的 `info` · 歧义 · 读
  `info` 在 output schema 里只是 `type: [object, array, null]`，字段名、每个字段的形式（`{unit, value}`，`steady_state` 另有 `source: "dcgain"`）、单位（`"1"`、`"%"`、`"s"`）、某个指标不存在时写 `null` 还是 `{unit, value: null}`，都只出现在示例响应里；`y` 的单位 `"1"` 也是。golden 的期望值用的是 python-control 的键名（`RiseTime`、`SettlingMin`，SISO 也是 [[{…}]]），与线上格式不同，靠运行器映射。`step_info.enabled = false` 时 `info` 是什么也没写（我返回 null）。我照示例响应的键名写。
- **PY-52** step · 10.2 第二条“离散系统 MUST 满足 dt = ts” · 措辞 · 读
  profile 写等号，参考 manifest 的前提写“相对误差 1e-9”。我用相对 1e-9，并以 dt 作网格步长。
- **PY-53** step · 10.2 `decimated`：“取下标 round(k(N−1)/(M−1))” · 措辞 · 读
  round 的平局规则（四舍五入还是银行家舍入）没写；Python 的 `round` 是后者。golden 里只有 N − 1 被 M − 1 整除的用例，测不出来。我用 floor(x + 0.5)。
- **PY-54** step · 10.2 `COMPUTE_LIMIT` 的 remediation、9.4 CF-9 · 措辞 · 读
  “把 dt 放大到满足上限”没有给出取值规则（示例取 0.05 = 0.1/最快极点模）；也没说补丁后的请求还要满足 `OUTPUT_TOO_LARGE`（CF-9 要求套用补丁后通过校验，`return_samples = all` 时只满足计算上限的 dt 仍可能触发 `OUTPUT_TOO_LARGE`）。离散系统的 dt 不能改，该给什么补丁也没写。我的做法：dt 取 max(满足两个上限所需的值, 0.1/max|p|) 向上取到 {1, 2, 5}×10^k；离散系统改为缩短 t_final 的补丁。
- **PY-55** step · `input.schema.json` 的 `time.mode = auto`（“启发式网格（见 manifest）”） · 歧义 · 读
  参考 manifest 里没有任何启发式的描述，规范其他地方也没有。parity 不测（excluded），但这是一个公开的输入模式，不同实现的结果会完全不同。我自定：连续系统 t_final = 7/最慢稳定极点实部（没有稳定极点时 10 s），dt = t_final/1000；离散系统 dt = ts。总是发 `W.ACCURACY.AUTO_GRID`。
- **PY-56** step · 10.2 第三条、output schema“元素可为 null（该通道不稳定或 DC 增益无穷）” · 歧义 · 读
  MIMO 时“不稳定”是整个系统（eig(A)）还是逐通道？某个不稳定模态可能在某个通道里不可见，该通道的阶跃响应是收敛的。`W.DOMAIN.NOT_SETTLED` 与 `W.ACCURACY.GRID_TOO_COARSE` 在参考 manifest 里声明了，step 的 profile 却没有说何时发（I-12 只为 stepinfo 规定了 NOT_SETTLED）；上升阈值从未达到时上升时间是什么也没写。我的做法：按整个系统判稳定，不稳定时所有通道为 null；没进调节带时 `settling_time` 为 null 并发 `NOT_SETTLED`；上升阈值没达到时 `rise_time` 为 null；dt·max|p| > π 时发 `GRID_TOO_COARSE`。

### 10.4 control.lti.convert

- **PY-57** convert · 10.4 第一条、3.10 R-6、`parity.yaml` 的 `ss_to_tf_mimo` · 歧义（只读 profile 正文会失败） · 读 + 运行
  profile 正文只在“MIMO tf→ss 为逐元素块对角”时提到 `W.DOMAIN.NONMINIMAL_REALIZATION`；R-6 的“MIMO 转换得到非最小实现时”没有说共享分母的 ss→tf 算不算“实现”。parity.yaml 与 golden 期望 MIMO ss→tf 也发这个警告。反向实验：不对 ss→tf 发，8 个 `ss_to_tf_mimo` 用例失败（CF-4 预期警告）。我按 parity.yaml 发。
- **PY-58** convert · 10.4 第一条“非 proper 的 tf → `E.DOMAIN.IMPROPER_SYSTEM`” · 歧义 · 读
  这句不分目标表示。improper tf 转 tf 或 zpk 在数学上没有问题（只有 ss 实现不存在），I-7 也允许组合算子接受 improper。我照字面一律报错（golden 只有 to = ss 的用例）。
- **PY-59** convert · 10.4 trust、4.1 E-12（`response_consistent`，`test_points`）、开放问题 13 · 歧义 · 读
  离散系统的测试点只写了“单位圆上”，没有给角度；“跳过离极点过近的点”没有给距离；`response_consistent` 何时为 false 没写（profile 只说 > 1e-6 时发 `CONVERSION_DISCREPANCY`；组合算子 I-9 用的是 1e-5）。我的做法：θ_k = π(k + ½)/9；距输入系统极点 ≤ 1e-8·max(1,|s|) 的点去掉；`response_consistent` 与警告用同一阈值 1e-6。
- **PY-60** convert · 9.4 CF-6、CF-2，`parity.yaml` · 歧义（套件） · 运行
  convert 的 parity.yaml 有 `compare` 但没有 `conformance_compare`，套件也没有 convert 的数值规则，所以 74 个成功用例全部记为 skip，只检查错误码、预期警告、指纹与收据（记为跳过的用例也检查预期警告，见 PY-57）；数值错误（包括第 0 章的列主序回归）不会被发现。我用 parity.yaml 的 `compare` 公式自查了全部 golden（tf 系数按 Π(1+|λ|)/‖den‖₁ 缩放、zpk 按 Wilkinson κ 最近匹配、ss 矩阵相对 Frobenius 1e-12），最坏用掉容差的 7.3e-4。

### 10.34 fit.poly.roots

- **PY-61** roots · 10.34 RT-2 · 措辞 · 读
  规范性条款 RT-2 写“经 `kernels::schur` 求特征值……与 `control.lti.poles_zeros`、`signal.filter.filter` 用的是同一个求根内核（11.5 RI-1）”。`kernels::schur` 是 Rust 参考实现的内部名字，RI-1 按第 11 章只约束参考实现；profile 末尾还引用 `aeoi-numeric/tests/parity_poly.rs`。对第二实现，这一句里什么是要求（友矩阵特征值？Schur 形？）不清楚。我用平衡后的友矩阵 + 自写 Francis 双位移 QR（只求特征值），其余按 RT-2。
- **PY-62** roots · 10.34 RT-5、RT-7 · 歧义 · 读
  `multiplicity` 定义为“每个根所在分支的根数”，RT-5 又说“末尾 0 系数给出的精确零根不算簇”。精确零根的 `multiplicity` 是 1 还是 k 没写；golden 的 `multiplicity_true` 对 x²·(…) 的零根写 2，parity 规则说“真重根必须在至少同样大小的簇里”。我报 k（零根个数），不发 `CLUSTERED_ROOTS`。q 自己的根的圆盘与 0 重叠时是否合并，也没写（我不合并）。
- **PY-63** roots · 10.34 RT-5 · 措辞 · 读
  警告 `details.clusters` 的 `center`、`radius` 没有定义。我取簇内近似值的平均作中心，半径 = max_j(|z_j − c| + r_j)，与 `response.cluster.json` 的数值一致。
- **PY-64** roots · 10.34 RT-4、4.3 R-2、9.4 CF-7 · 歧义 · 读
  RT-4 规定 `bound_kind = rigorous`，CF-7 要求 rigorous 的 bound 不大于该用例的验收容差。包含半径是严格的，但按设计比验收容差（简单根 32u·q̃/|q′| + 2u|r|）宽：半径含 m·γ_{4m+4} 量级的因子，簇的半径更是 u^{1/k} 量级（示例 1.4e-6）。照 CF-7 字面，每个有根簇的用例都是“声明本身不合规”；套件显然没这样判。规范应说明 CF-7 对包含半径类的界如何适用（例如只比较孤立根，或改为 claim check）。
- **PY-65** roots · 7.1 V-3、V-3a、9.4 CF-11 · 措辞（套件） · 运行
  roots 的 golden 放在 `golden/python-control/fit.poly.roots/`，`reference_env.engine` 却是 “numpy.roots / mpmath.polyroots (60 digits)”，比较对象是 mpmath 真值。V-3 要求目录名就是引擎名，V-3a 要求高精度参考用 `golden/mpmath/`。运行器按目录把 20 个用例记为 “python-control (independent)”（PY-30 同类，新实例）。

### 新增：无法只从规范推出的 manifest 字段（第二轮）

| 字段 | bode | step | convert | roots |
|---|---|---|---|---|
| `version` | profile 开头（0.2.0） | 同（0.2.0） | 同（0.3.0） | 同（0.4.0） |
| `tolerance_class` | profile 开头（T3） | T2 | T2（parity.yaml 写 “T1/T2”，profile 写 T2） | T2 |
| `receipt.postconditions` | **照抄参考 manifest**（`all_finite`、`frequencies_strictly_increasing`；profile 未点名，PY-44） | **照抄参考 manifest**（`all_finite`） | 名字取自核心 E-12 的举例（`response_consistent`） | RT-6 点名，照写 |
| `receipt.coverage` | 未声明（缺省 full） | 未声明 | `test_points`（E-12 举例，profile 没写） | 未声明（RT-6 没写） |
| `receipt.guards` | 自定 | 自定 | `/system`（E-13 举例“组合算子的 response_consistent → /system”类推） | RT-6 写明，照写 |
| `receipt.exactness.bound_kind` | **照抄参考 manifest**（`a_priori`；PY-49） | 自定（`a_posteriori_heuristic`） | 自定（`a_posteriori_heuristic`） | RT-4 规定 `rigorous` |
| `errors[]` | 按实际会发的码；`E.RESOURCE.TIMEOUT`、`E.VALIDATION.SPARSE_STRUCTURE` 不发所以不列 | 同；`W.DOMAIN.INFINITE_DC_GAIN` 不发所以不列 | 同；`DEGENERATE_SYSTEM` 不发（PY-42），另加 `E.NUMERICAL.NO_CONVERGENCE`（自写 QR 的迭代上限） | 同；参考列的 `NON_FINITE_EVALUATION` 不发，另加 `NO_CONVERGENCE` |
| `warnings[]` | 按实际会发的码 | 同；`GRID_TOO_COARSE`、`NOT_SETTLED` 的触发条件自定（PY-56） | 同 | 同 |
| `limits` | `max_frequency_points` 取自 schema；`max_states` = 200 自定 | 10⁷ / 10⁶ 取自 profile；其余自定 | 自定 | `max_degree` 500 取自 RT-1 |
| `applicability.preconditions` | 自写；zpk 的 proper 定义（零点数 ≤ 极点数）只在参考 manifest 里 | 自写；`dt = ts` 的容差取自参考 manifest（PY-52） | 自写 | 自写 |
| `accuracy_grade`、`conformance` | C / L1（7.3：没有凭证报 C） | 同 | 同 | 同 |
| `determinism` | `platform_bitwise`（自定：numpy 的 `log10`/`arctan2`/`cos`/`sin` 按 CPU 特性分派 SIMD 内核或调用系统 libm，M-7a 不允许声明 `bitwise`） | `platform_bitwise`（numpy matmul 走 OpenBLAS DYNAMIC_ARCH，按 CPU 分派） | `platform_bitwise` | `platform_bitwise`（RT-8 规定） |
| `sparse_support` | `{}`（全部 reject；参考是 densify） | 同 | 同 | `{}` |
| `backend.binding` | `native`（M-6：数值核心是本语言代码 + numpy 逐元素） | `native`（小矩阵乘法走 OpenBLAS，严格说是 ffi；M-6 对“少量 BLAS 调用”没有说法） | `native` | `native` |
| `provenance.references`、`title`、`description`、`category`、`tags`、`not_for` | 自写 | 自写 | 自写 | 自写 |
| `input_schema`、`output_schema` | profile 链接的 schema，内联 | 同；`info` 的结构 schema 里没有（PY-51） | 同 | 同 |

## 处理结果（第二轮，v0.4.21-draft，2026-11-26）

同第一轮：每条先对照参考实现（Rust）的实际行为核实，再决定改规范、改套件，还是列入开放问题 31。bode、step、convert 三份 profile 改写为编号条款（BD-1 – BD-8、ST-1 – ST-9、CV-1 – CV-8），条款内容取自参考实现的代码，而不是 python-control。

| 发现 | 处理 |
|---|---|
| PY-33 | 3.1 表格：`time_unit` 可省略（缺省 `"s"`），返回的系统总是写出 |
| PY-34 | T-5 写明：tf/zpk 的非有限值在 schema 层失败（`SCHEMA`），ss 的 `Matrix` 走 R-2（`NON_FINITE`）；两条路径的码不同是有意的 |
| PY-35 | T-5 写明共轭配对判定：τ = 1e-12·max(1, \|c\|)，\|Im c\| ≤ τ 视为实数，其余一一配对（你的 \|z\| 在 \|z\| < 1 时比参考严） |
| PY-36 | T-5：1×1 嵌套按 SISO，返回一维形式 |
| PY-37 | R-6、CV-2：块按 (i, j) 行主序，块 (i, j) 只占 B 的第 j 列、C 的第 i 行（与你的做法相同） |
| PY-38 | R-6、CV-2：zpk→ss 先展开成 tf 再用可控标准型，`realization = controllable_canonical`（与你的做法相同） |
| PY-39 | ST-5：用算出的极点严格判定，不设容差；与尺度无关的阈值列入开放问题 31 |
| PY-40 | ST-5：算出的 y∞ 恰为 0 才算零 DC 增益；阈值同上，开放问题 31 |
| PY-41 | R-8 改写：k = C·A^{r−1}·B，r = #极点 − #零点，零点个数来自 R-9 的约化，不判断 Markov 参数是否为零；R-9 写明约化算法与秩容差 |
| PY-42 | R-9 限定为返回零点集合的算子；CV-5：convert 的 SISO G ≡ 0 写成 k = 0 的 zpk（与你的做法相同）；约化得不到方的可逆 D_r 时仍为 `DEGENERATE_SYSTEM` |
| PY-43 | CV-6：T-6 的警告只针对输入；`normalize` 只影响 tf→tf。ss→tf 的 `normalize = false` 列入开放问题 31 |
| PY-44 | 三份 profile 改为编号条款，后置条件点名：bode `all_finite`、`frequencies_strictly_increasing`（BD-7）；step `all_finite`（ST-8）；convert `response_consistent`（CV-7） |
| PY-45 | BD-5：阈值针对相对误差；ss 的估计方法写明 |
| PY-46 | BD-6：NaN 点不更新展开的参考点 |
| PY-47 | BD-1：logspace 在输入单位上生成，最后换算（Hz × 2π） |
| PY-48 | BD-1、BD-3：显式频点的补丁排序并去重；“恰为极点”的判据写明 |
| PY-49 | BD-5：manifest 的 `bound_kind` 取各输入类型中最弱者，参考 manifest 改为 `a_posteriori_heuristic`（你的 manifest 同步修改） |
| PY-50 | ST-6：全部指标在 profile 里给出定义（含 settling_min/max 包括 y∞、overshoot/undershoot 的符号约定），不再需要参考 python-control 源码 |
| PY-51 | ST-7：`info` 字段名与形式写明；golden 用 python-control 键名由运行器映射 |
| PY-52 | ST-1：dt 与 ts 的相对容差 1e-9 |
| PY-53 | ST-4：四舍五入取平局远离零 |
| PY-54 | ST-3：补丁取值公式，以及 `COMPUTE_LIMIT` 与 `OUTPUT_TOO_LARGE` 的先后 |
| PY-55 | ST-2：`auto` 网格公式写明（仍不参与 parity） |
| PY-56 | ST-5：按整个系统判稳定，不稳定时全部通道为 null；ST-6：`NOT_SETTLED` 与未达上升阈值的处理写明 |
| PY-57 | CV-3：MIMO ss→tf 也发 `NONMINIMAL_REALIZATION`；R-6 同步 |
| PY-58 | CV-1：improper 输入在解析阶段就报错，与目标表示无关；理由写明 |
| PY-59 | CV-7：测试点公式（连续与离散）、跳过距离 1e-6·max(1, \|s\|)、`response_consistent` 与警告共用阈值 1e-6，全部跳过时为 true |
| PY-60 | 套件新增 `rule_convert`（CV-8）：由请求独立重算被测结果与输入系统的响应差，容差 max(1e-8, 10·d_ref)；另查 `realization`、`ts`、ss 的状态数。convert 的 parity.yaml 加 `conformance_compare`；两个代理变异（增益差 1e-6、`realization` 标错）。两个实现的 76 个成功用例现在都按数值判定 |
| PY-61 | RT-2：“经 `kernels::schur`”改为只约束参考实现，第二实现可用任何后向稳定的特征值算法 |
| PY-62 | RT-5：k 个精确零根各自 `multiplicity = k`、半径 0，不进入簇，也不与 q 的圆盘合并（与你的做法相同） |
| PY-63 | RT-5：簇中心 = 簇内近似根的平均，半径 = max(\|z_j − c\| + r_j)（与你的做法相同） |
| PY-64 | 9.4 CF-7 加例外：逐元素的包含区域不与验收容差比大小，而由规则核查真值在区域内；RT-4 引用之 |
| PY-65 | 开放问题 31（与 PY-30 同类，等统一迁移 golden 目录时处理） |

## 第三轮（linalg.subspace.rank、linalg.matrix.pinv、linalg.decomp.qr、linalg.decomp.chol、linalg.matrix.det）

2026-11-26，规范 v0.4.24-draft。范围：`linalg.subspace.rank`（10.37）、`linalg.matrix.pinv`（10.39）、`linalg.decomp.qr`（10.43）、`linalg.decomp.chol`（10.44）、`linalg.matrix.det`（10.36）。
新文件 `op_rank.py`、`op_pinv.py`、`op_qr.py`、`op_chol.py`、`op_det.py`；`aeoi_kernels.py` 新增自写内核：单边 Hestenes Jacobi SVD（`jacobi_svd`，含正交补全）、Householder QR（`householder_qr`，dlarfg 约定，任意形状，econ/full）与 QR-3 的变号、按列 Cholesky（`cholesky_upper`，正确舍入内积）、`spacing`（eps(x)）、补偿残差。rank 与 pinv 共用同一个 SVD 内核与同一段秩判定（`op_rank.rank_decision`）；det 直接调用 `op_linsolve` 的 LU（`_lu`）与 Hager–Higham 估计（`_inv_norm1_est`），即与 linsolve 同一段代码。说明：`op_linsolve` 里超定情形用的 Householder QR 仍是第一轮的那份，没有改成调用 `aeoi_kernels.householder_qr`（为了不动已通过、且被变异测试注入的旧文件），所以按 RI-1 的精神，本实现的 QR 目前有两份。结果路径上没有 numpy.linalg、scipy.linalg 或 LAPACK；Householder 更新与 pinv 的 X 也改成了逐元素运算（不走 BLAS），只有后置条件里的矩阵乘积用 numpy `@`。
只读了 `spec-v0.4.md`、这五份 profile 及其引用的 10.19（LS-2、LS-5）、10.23（SV-1 – SV-7）、10.35（只看了引用处）、`examples/<op>/`（含 manifest.json、parity.yaml、各 request/response）、`schemas/`、`golden/**/<op>/**`、`conformance/README.md`、`conformance/aeoi_hash.py` 与本目录。没有读 `rust/`、`scripts/`、`conformance/rules_*.py`、`conformance/aeoi_conformance.py` 与 `conformance/tests/` 的源码（只运行并看退出码与报告）、`docs/reference-impl.md`、`roadmap/`、`CHANGELOG.md`，也没有看 git 历史。

### 结果

| 算子 | golden 通过 / 失败 / 跳过 | method-policy | 默认值用例 | 退出码 |
|---|---|---|---|---|
| `linalg.subspace.rank` | 18 / 0 / 0（mpmath，独立） | 1 | 0（input schema 没有带 default 的字段） | 0 |
| `linalg.matrix.pinv` | 15 / 0 / 0（mpmath，独立） | 1 | 0 | 0 |
| `linalg.decomp.qr` | 13 / 0 / 0（mpmath，独立） | 1 | 3 | 0 |
| `linalg.decomp.chol` | 12 / 0 / 0（mpmath，独立） | 1 | 3 | 0 |
| `linalg.matrix.det` | 23 / 0 / 0（mpmath，独立） | 1 | 0 | 0 |
| 五个一起 | 92 / 0 / 0 | | | 0 |
| 十四个算子一起（前两轮 9 个 + 本轮 5 个） | 620 / 0 / 0（前 9 个仍是 528 / 0 / 0） | | | 0 |

`python3 -I conformance/tests/test_runner_catches_mutants.py` 退出码 0。

第一次运行 87 通过 / 5 失败：3 个 det 失败是我自己的缺陷（`"Inf"`/`"-Inf"` 在求 output_hash 之前没有编码成 R-3 的字符串，服务端抛 internal error），与规范无关；另 2 个（rank、pinv 的 `lowrank_4x9_r1`，报 `E.NUMERICAL.NO_CONVERGENCE`）是照 SV-2 字面写的 Jacobi 判据不收敛（PY-68）。改正后全部通过。另做了几个反向实验（把实现改回另一种同样符合正文的读法，在临时副本上跑套件），数字写在各条里。

还有一类“通过但不一致”的情形套件看不到：本实现在 `lowrank_6x5_r2`（rank 与 pinv）和 pinv 的示例 `request.json` 上发 `W.NUMERICAL.RANK_AMBIGUOUS`、claim 为 `unverified`，参考实现在同样的输入上不发（PY-70）。

### 通用（核心规范）

- **PY-66** 全部五个算子 · RK-1、DT-1、CH-1/CH-2、QR-1，3.3 R-2 · 歧义 · 读
  每份 profile 都列了 `SHAPE`、`DTYPE`、`NON_FINITE`（chol 还有 `NOT_SYMMETRIC`、`NOT_POSITIVE_DEFINITE`），但核心规范和 profile 都没有规定一个输入同时违反几条时报哪一个：复数的一维数组、带 `"NaN"` 的 2×3 矩阵（det 要方阵）、不对称且不是方阵的 chol 输入等。golden 里每个错误用例只违反一条，所以套件也不决定。我的顺序是：schema → 编码与 dtype → 非有限值（`read_array` 解码时）→ 形状 → 对称 → 正定。两个实现可以对同一个请求报不同的码，而 CF-4 要求错误码精确匹配。建议在 3.x 给出全局的校验顺序。
- **PY-67** rank、pinv、qr、chol · SV-5、PI-4、QR-4、CH-4 · 歧义 · 读 + 运行
  这些后置条件都是“相对”量，分母为 0 时没有定义：A = 0 时 ‖A − UΣVᵀ‖_F/‖A‖_F（rank、pinv 的 `svd_reconstructs`，golden `zero` 用例就是 3×4 零矩阵）、‖A − QR‖_F/‖A‖_F；r = 0 时 X = 0，PI-4 的 ‖XAX − X‖_F/‖X‖_F = 0/0（golden pinv `zero`）；`max(1, cond_r)` 在 `cond_r = null` 时怎么算也没写。开放问题 26 的“空真”针对的是整条后置条件不适用，这里是适用的条件里出现 0/0。我的做法：分母为 0 时改用绝对残差（零矩阵上恰为 0），`cond_r = null` 按 1。反向实验：把 0/0 当作 NaN（于是 `penrose = false`、claim `unverified`），套件仍然 0 失败——claim 是运行器按本实现自己的 checks 重推的，所以这一点套件不检验，两种实现会对同一个零矩阵给出不同的 claim。

### 10.37 linalg.subspace.rank 与 10.23 SV-2（rank、pinv 共用）

- **PY-68** rank、pinv · SV-2（经 RK-1、PI-1 引用） · **阻塞** · 读 + 运行
  SV-2 的旋转判据“|g_pᵀg_q| > sqrt(m)·ε·‖g_p‖‖g_q‖ 时旋转，至多 60 轮”照字面实现（范数先缩放再求，不会下溢）时，`lowrank_4x9_r1`（4×9，秩 1）不收敛：Aᵀ 的一个列只剩舍入残余（第 5 轮 7.4e-79），它几乎与模 30 的列平行，余弦接近 1，判据每轮都成立；每次旋转只把它缩小约 ε 倍，一路缩进次正规数范围，60 轮后仍在旋转，报 `NO_CONVERGENCE`。SV-2 自己的说明里写参考实现在 σ₂ = 1.8e-172 处停下，是因为“内积会下溢为 0，旋转判据被‘满足’”——也就是说，参考实现能停下来靠的是某种求值顺序下的下溢，而这个求值顺序规范没有写。我加了一条规范里没有的规则：模 ≤ max(m, n)·ε·max_j‖g_j‖（SV-2 的“数值零”）的列不参与旋转。反向实验：去掉这条，rank、pinv 各失败 1 个（`lowrank_4x9_r1`，`E.NUMERICAL.NO_CONVERGENCE`）。另外，m < n 时作用在 Aᵀ 上，判据里的 sqrt(m) 是 A 的行数还是被处理矩阵（Aᵀ）的行数，也没有写（我用后者）。建议 SV-2 把“数值零的列不旋转”写成规则，而不是写成下溢的副作用。
- **PY-69** rank、pinv · RK-2“eps(x) 为 x 处的浮点间距” · 歧义 · 读 + 运行
  x 是 2 的幂时，“x 处的间距”向上是 2^(e−52)、向下是 2^(e−53)，差一倍；MATLAB `eps` 与 `numpy.spacing` 都取向上，规范没写。运行器按 parity.yaml 的 `conformance_compare` 用 `spacing(s_1)` 精确重算 tol。反向实验：取向下的间距，rank 失败 2 个（`ambiguous_near_threshold`、`graded_kept`，两者 σ_max 都恰为 1）。把 eps(σ_max) 读成 ε·σ_max（很常见的写法）则 rank 失败 11 个；而同一个 parity.yaml 的 `compare` 写的是“default：在 max(m, n)·eps(σ_max_true) 的 2 倍以内”，`conformance_compare` 却是逐位重算，两处的严格程度不同。eps(0) 按“间距”是 2⁻¹⁰⁷⁴（golden `zero` 的 tol = 2e-323 与此一致），但规范没有单独写零矩阵：这时 tol/10 下溢为 0，区间变成 (0, 100·tol]，零奇异值不在区间内只是因为下沿是开的。
- **PY-70** rank、pinv · RK-3 区间下沿 tol/10、SV-5/SV-6、parity.yaml · **矛盾** · 读 + 运行
  RK-3 选 tol/10 作下沿的理由是“精确秩亏的矩阵，后向稳定的 SVD 算出的‘零’奇异值在 u·σ_max 量级，约为缺省 tol 的 1/(2·max(m, n))”。但后向稳定只保证 |Δσ| ≤ c(m, n)·u·‖A‖，SV-5 自己允许的重构/正交性误差到 100·max(m, n)·u，SV-6 的误差界也是 ‖A‖_F·残差 + σ_max·正交性误差。本实现的 Jacobi SVD 满足 SV-5（重构 1e-16 量级），但在 `lowrank_6x5_r2` 上给出的“零”奇异值是 3.15e-14 = 0.74·tol（约 4.8·u·σ_max），在 pinv 示例 `request.json`（2×3，秩 1）上是 6.66e-16 = 0.125·tol，都落在 (tol/10, 100·tol] 内，于是发 `RANK_AMBIGUOUS`、`rank_well_separated = false`、`/rank`（pinv 还有 `/X`）列为不可依赖；参考实现在同样的输入上不发（示例 `response.success.json` 的 status 是 `ok`）。同一个 golden 用例、两个都“合规”的实现，给出相反的可靠性结论。parity.yaml 的 `compare` 写“warnings：exactly the predicted set”，而 `conformance_compare` 只把“真值在区间内却声称 `rank_well_separated = true`”判为虚报，所以运行器接受我多发的警告（这两条在同一个规范性文件里不一致）。我保留了按 RK-3 字面的行为（发警告），没有为了贴近参考而调 SVD 的舍入。反向实验：下沿改成 tol/100，套件 0 失败——RK-3 理由里说的“每个秩亏矩阵都会被误报”在本实现上没有出现，区间的宽窄在套件上不可观察。建议：下沿的理由改为与 SV-5 的阈值一致（或者把区间与算出的误差界 SV-6 挂钩），并决定多发 `RANK_AMBIGUOUS` 是否算不一致。
- **PY-71** rank、pinv · RK-3“依赖秩的字段列为不可依赖（E-13）”、PI-3 · 歧义 · 读
  RK-3 没有列出 rank 的哪些字段“依赖秩”。参考 manifest 只列 `/rank`；但 `smallest_kept`、`largest_dropped` 是按 r 切开 s 得到的，r 不可依赖时它们“是哪两个奇异值”也不可依赖。pinv 的 PI-3 明确写 `/X` 与 `/rank`，而 `cond_r` = σ₁/σ_r 同样取决于 r。我对 rank 列 `/rank`、`/smallest_kept`、`/largest_dropped`，对 pinv 照 PI-3 只列 `/X`、`/rank`。运行器不比较 `unreliable` 的内容（它来自各自的 manifest），所以两个实现可以告诉模型不同的“不得引用”字段。

### 10.39 linalg.matrix.pinv

- **PY-72** pinv · PI-1“同 RK-1 – RK-3”、`output.schema.json` · 歧义 · 读
  PI-1 整体引用 RK-2，而 RK-2 要求“结果回显 tol 与 tol_source，并给出 smallest_kept、largest_dropped”；pinv 的 output schema 只要求 `X`、`rank`、`tol`、`tol_source`、`cond_r`，示例响应也没有 smallest_kept / largest_dropped。照 PI-1 字面要输出，照 schema 与示例不用。我输出了（schema 没有 `additionalProperties: false`，合法），但它们进入 output_hash，与参考实现的结果哈希不可能相同（P-7 只在同一构建内比较，所以不算失败）。PI-4 里的 ‖σ_{r+1..}‖₂ 用算出的还是真的奇异值、是否包括 RK-3 区间内被舍去的那些，也没有写（我用算出的、全部被舍去的）。

### 10.43 linalg.decomp.qr

- **PY-73** qr · QR-2“输出超过 `limits.max_output_elements` 时报 `E.RESOURCE.OUTPUT_TOO_LARGE`” · **阻塞** · 读 + 运行
  profile 没有给这个上限的值，也没有说计数是只算 Q 还是 Q 与 R 之和；golden `full_too_large`（1500×1，mode = full：Q 有 2 250 000 个元素，加 R 共 2 251 500 个）却期望 `OUTPUT_TOO_LARGE`。也就是说，只有选的上限恰好低于 2 250 000 的实现才能通过；参考 manifest 写的是 2 000 000。我类比 L-6（内联 JSON 数组 ≤ 10⁶ 个元素，原文只针对输入）取 10⁶、计 Q + R，所以通过只是碰巧。反向实验：上限取 10⁷，这个用例开始真算，本实现的精确正交性检查（O(m³) 个正确舍入内积）超过运行器的 60 s，失败 1 个，并拖累其后几个用例也超时。建议 QR-2 写出上限的数值与计数方式，或者 golden 里不放依赖实现上限的用例。补丁“删除 mode”在 econ 本身也超限时无效，这种情形我只给 `ask_user`。
- **PY-74** qr · QR-1/QR-3、SV-1 · 歧义 · 读
  (1) `mode = full` 时 Q 的后 m − k 列：QR-3 只对 i < k 的列变号，没说这些列是什么。m − k ≥ 2 时它们只确定到一个正交变换，规范没有写“取 H₀…H_{k−1} 作用在单位阵上的那几列”（我是这样做的，与示例 `response.full.json` 一致到 1e-15）。parity 对 `tall_full`、`wide_full` 标 `unique: True` 并比较 ‖ΔQ‖_F，这对补全列只在采用同一构造时成立。(2) dlarfg 在尾部为 0 时 τ = 0、H = I，R_jj = α 可为负，QR-3 的变号兜住了；R_ii 恰为 0 不变号，规范按“R_ii < 0”字面也是如此。但 α = −0.0 时 dlarfg 的 SIGN(β, α) 取哪一边（Fortran 对负零的 SIGN 依编译器而定）没有说；只影响零列，结果仍满足 R_ii ≥ 0。(3) QR-4 的 `reconstructs` 写明补偿算术，`orthonormal` 没写；我两者都用正确舍入内积。

### 10.44 linalg.decomp.chol

- **PY-75** chol · CH-1、CH-2 · 歧义 · 读
  (1) CH-2 写“只读 A 的上三角（同 MATLAB）”，CH-1 又有 `triangle = "lower"`。MATLAB 的 `chol(A, 'lower')` 读的是**下**三角；按 CH-2 字面，lower 也读上三角、只是返回 Rᵀ（我这样做），那么“同 MATLAB”只对缺省成立。(2) 阈值 10·n·u·max|a_ij| 里的 max|a_ij| 是整个矩阵还是上三角没写（golden 里两者相同；我用整个矩阵，与 `asymmetry` 的定义“max|a_ij − a_ji|/max|a_ij|”一致）；零矩阵时 `asymmetry` = 0/0（随后报 NOT_POSITIVE_DEFINITE，不影响输出，但检查顺序也没写）。(3) `details` 的“最大差与位置”只有示例给了键名（`max_asymmetry`、`at`），位置是 [i, j] 还是 [j, i]、并列取哪一个没写（我取上三角行主序第一个最大者，[i, j]，i < j）。(4) 补丁“把 A 换成 (A + Aᵀ)/2”的 JSON Pointer 只在示例里是 `/A/data`：A 以 b64 编码给出时没有 `/A/data`，A 为 i64 时 (A + Aᵀ)/2 可能不是整数，套用后不通过校验（CF-9）。我在这两种情形下改为替换整个 `/A`（f64）。
- **PY-76** chol · CH-3 · 措辞 · 读
  “MUST 与 `lqr`、`dlqr` 同一内核”是对参考实现的约束（RI-1）；对第二实现，d_j = a_jj − Σ r_kj² 的求和顺序与精度没有规定，而 d_j ≤ 0 的判定恰好对这一点敏感：接近奇异的正定矩阵，普通求和与正确舍入求和可能一个通过、一个报 `NOT_POSITIVE_DEFINITE`（golden 的 `semidefinite` [[1, 1], [1, 1]] 在任何算术下 d₁ 都恰为 0，测不出来）。我用正确舍入内积。`details` 的键名（`failed_column`、`pivot`）与 `retryable` 只在示例里。

### 10.36 linalg.matrix.det

- **PY-77** det · DT-4 的 est(‖A⁻¹‖₁)、10.19 LS-5 · 歧义 · 读
  DT-4 的误差估计公式 ε = n·u·est(‖A⁻¹‖₁)·‖|L||U|‖₁ + n·u 里，‖|L||U|‖₁ 的算法写清楚了，但 est 只引用 LS-5“固定起始向量，至多 5 次迭代，加上交错向量”：起始向量是什么、迭代的停止条件、交错向量的公式以及它怎样并入估计，都没有写。`zero_within_error` 是 ε ≥ 1 的阈值判定，est 不同，ε 接近 1 的矩阵上旗标就不同。我用第一轮按 Higham（Algorithm 15.4 的 Hager 迭代，x = e/n，‖y‖₁ 不再增大或 |z_j| ≤ zᵀx 即停；交错向量 x_i = (−1)^i(1 + i/(n − 1))，取 max(est, 2‖y‖₁/(3n))）写的那份，示例的 2.25 与 2251799813685247.5 逐位相同——但这是文献，不是规范。建议 LS-5 写出算法。
- **PY-78** det · DT-3、`parity.yaml` · **矛盾** · 读
  DT-3：“|det| 超出 f64 的**正规数**范围时 MUST 发 `W.NUMERICAL.DET_OUT_OF_RANGE`”，即 ln|det| < ln(2⁻¹⁰²²) ≈ −708.40 或 ln|det| ≥ 1024·ln 2 ≈ 709.78。parity.yaml（profile 声明它是规范的一部分）写“DET_OUT_OF_RANGE iff |ln|det_true|| > 708”：上溢一侧 708 < ln|det| < 709.78 时 parity 要求发、DT-3 不要求；还有一处不同是 parity 按真值判、DT-3 没有说按真值还是按算出的值（接近边界时两者可能在边界两侧）。golden 里只有 991 与 −807，测不出来。我按 DT-3 用算出的尾数与指数判定（2⁻¹⁰²² ≤ |det| 且不溢出）。
- **PY-79** det · DT-2、DT-4、DT-5、`output.schema.json` · 措辞 · 读 + 运行
  (1) 某步主元恰为 0 时，DT-4 规定了四个输出，但 `trust.condition`（est、‖|L||U|‖₁）这时算不出来，写什么没说；是否像 dgetf2 那样继续消元也没说（我立即停止）。(2) 舍入后“奇异”的矩阵（`magic(4)`）主元不为 0，`sign` = −1 而真值为 0；`conformance_compare` 写“sign：equal to the truth”，运行器实际上对被标出的用例不比较 sign（本实现通过），这一例外只能从 E-13 推出，parity.yaml 没写。(3) `sign` 的 schema 是 `enum [−1, 0, 1]`，没有说必须是 JSON 整数；反向实验：把 sign 写成 `−1.0`，套件 0 失败（JSON Schema 的数值相等与 JCS 都不区分 1 与 1.0）。规范里“整数”字段（`sign`、`rank`、`failed_column`）是否允许 1.0 没有一般规定。(4) `log_abs_det` 用了 `math.log`（系统 libm），按 M-7a 本实现的确定性只能是 `platform_bitwise`，与 DT-6 一致。

### 新增：无法只从规范推出的 manifest 字段（第三轮）

| 字段 | rank | pinv | qr | chol | det |
|---|---|---|---|---|---|
| `version`、`tolerance_class` | profile 开头（0.4.0，T1） | 同（0.4.0，T2） | 同（0.4.0，T1） | 同（0.4.0，T1） | 同（0.4.0，T2） |
| `receipt.postconditions` | RK-1 `svd_reconstructs` + RK-3 `rank_well_separated`（SV-5 的 `all_finite` 没有列，RK-1 没点名） | PI-4 点名的三项 | QR-4 点名的三项 | CH-4 点名的两项 | DT-4 `significant` |
| `receipt.coverage` | 全部 `full`：profile 没写，E-11a 要求“同样的 coverage”却无从对照；与参考 manifest 相同 | 同 | 同 | 同 | 同 |
| `receipt.guards` | `rank_well_separated` 自定（PY-71）；`svd_reconstructs` → `/rank`、`/s`、`/smallest_kept`、`/largest_dropped`：**读参考 manifest 后采用** | `rank_well_separated` 取自 PI-3；`svd_reconstructs` → `/X`、`/rank`、`/cond_r`、`penrose` → `/X`：**读参考 manifest 后采用** | `reconstructs` → `/Q`、`/R`，`orthonormal` → `/Q`：**读参考 manifest 后采用**；`all_finite` 自加 | `reconstructs` → `/R`：**读参考 manifest 后采用**；`all_finite` 自加 | DT-4 写明（`/det`、`/sign`、`/log_abs_det`） |
| `receipt.exactness.bound_kind` | 自定 `a_posteriori_heuristic` | 自定 | 自定 | 自定 | DT-4 规定 |
| `receipt.assumptions` | 自写（“阈值是建模选择”一条的意思取自 RK-2） | 同 | 自写 | 自写 | 自写（“估计是启发式”取自 DT-4） |
| `errors[]` | 按实际会发的码；`NO_CONVERGENCE` 来自 SV-2 的 60 轮上限（profile 没写码） | 同 | `OUTPUT_TOO_LARGE` 来自 QR-2 | `NOT_SYMMETRIC`（CH-2）、`NOT_POSITIVE_DEFINITE`（CH-3） | 按实际会发的码 |
| `warnings[]` | `RANK_AMBIGUOUS`（RK-3） | 同 | 无 | 无 | `DET_NOT_SIGNIFICANT`、`DET_OUT_OF_RANGE`（DT-3、DT-4） |
| `limits` | `max_dimension` 500（取 SV-2 给 svd 的值；参考 rank 是 1000）、`max_jacobi_sweeps` 60（SV-2） | 同 | `max_output_elements` 10⁶（类比 L-6，PY-73）、`max_dimension` 2000 自定 | `max_dimension` 1000 自定 | `max_dimension` 1000 自定、估计迭代 5（LS-5） |
| `determinism` | RK-5 | PI-5 | QR-5 | CH-5 | DT-6（全部 `platform_bitwise`） |
| `applicability.not_for` | 自写 | PI 末段要求写“最小二乘不要先求伪逆”，措辞自写 | 自写 | CH-3 要求写“不是判断正定性的可靠手段”，措辞自写 | DT 末段要求写“判断奇异用条件数”，措辞自写 |
| `method_id`、`backend`、`provenance`、`title`、`description`、`tags`、`category` | 自写 | 自写 | 自写 | 自写 | 自写 |
| `accuracy_grade`、`conformance` | C / L1（7.3） | 同 | 同 | 同 | 同 |
| `sparse_support`、`encodings` | `{}`（reject）、json + b64（没有 data root，不收 ref） | 同 | 同 | 同 | 同 |
| `input_schema`、`output_schema` | profile 链接的 schema，内联 | 同（输出另含 `smallest_kept`、`largest_dropped`，PY-72） | 同 | 同 | 同 |

## 处理结果（第三轮，v0.4.25-draft，2026-11-26）

同前两轮：每条先对照参考实现核实。两条阻塞、两条矛盾都成立。

| 发现 | 处理 |
|---|---|
| PY-66 | 核心 3.3 新增 R-2a：校验顺序 SHOULD 为 schema → 编码与 dtype → 非有限值 → 形状 → 语义约束 → DOMAIN（与你的顺序相同，也与参考实现相同）；暂定 SHOULD，见开放问题 32 |
| PY-67 | SV-5、PI-4、QR-4：相对残差的分母为 0 时取绝对残差（参考实现一直如此：分母取 max(·, 最小正规数)） |
| PY-68 | **成立。** 参考实现停下来确实靠未缩放平方和下溢。SV-2 新增“旋转下限”：‖g_p‖ 或 ‖g_q‖ ≤ max(m, n)·ε²·‖A‖_F 时不旋转；参考内核改用这个显式判据（`lowrank_4x9_r1` 的 σ₂ 由 1.8e-172 变为 1.35e-31，其余 golden 与示例逐位不变）。下限先试过 ε·‖A‖_F：它冻结了 Hilbert 13 的 σ_min，cond 警告消失，秩亏矩阵的噪声奇异值落进 RK-3 的区间——正是 PY-70 说的现象；所以用 ε² |
| PY-69 | SV-4 写明 eps：到下一个更大 f64 的距离（2 的幂时向上），eps(0) = 2⁻¹⁰⁷⁴；rank 的 parity.yaml `compare` 改为逐位重算，与 `conformance_compare` 一致 |
| PY-70 | **成立。** RK-3 改为“按算出的奇异值判定”，并写明它依赖实现：多报秩不明确是保守的方向，允许；一致性套件只要求真奇异值在区间内时必须报。rank、basis、pinv 的 parity.yaml 由“exactly the predicted set”改为“必须包含；多报允许”。是否换成与实现无关的判据，列入开放问题 32 |
| PY-71 | RK-3 列全不可依赖的字段：rank 为 `/rank`、`/smallest_kept`、`/largest_dropped`；pinv 为 `/X`、`/rank`、`/cond_r`；basis 不变。参考 manifest 的 guards 同步（你的 rank 与此相同；pinv 请补 `/cond_r`） |
| PY-72 | PI-1 写明：只引用 RK 的范围、阈值与秩判定，输出字段只有 PI-2 的那些 |
| PY-73 | **成立。** QR-2 写明上限 MUST 为 2 000 000，按 Q 与 R 的元素数之和计 |
| PY-74 | QR-3 写明：full 模式的 Q 全部 m 列都是 H₀…H_{k−1} 作用在单位阵上，后 m − k 列不变号；sign(α) 按 IEEE 符号位（−0.0 为负）；R_ii = 0 不变号；QR-4 正交性残差用普通 f64 即可 |
| PY-75 | CH-2 写明：不论 `triangle` 取什么都读上三角；max|a_ij| 取整个矩阵；补丁改为替换整个 `/A`（内联 f64），参考实现同步修改，b64/i64 输入也能套用 |
| PY-76 | CH-3 写明参考实现的求和方式，并允许精度更高的求和（你的正确舍入内积合规）；接近奇异时结论可能不同，golden 不收这类用例 |
| PY-77 | LS-5 把 Hager–Higham 估计器写全（起始向量、停止条件、交错向量及其并入方式），DT-4 引用它 |
| PY-78 | **成立。** parity.yaml 改为与 DT-3 一致（ln|det_true| ≥ 709.78 或 < −708.40）；DT-3 写明按算出的尾数与指数判定；生成脚本断言用例离两条边界都远 |
| PY-79 | DT-5：`sign` MUST 是 JSON 整数（schema 改为 `type: integer`，lu 的 `perm_sign` 同）；主元为 0 时不必继续消元，`trust.condition` 写 `"Inf"` 与 `"NaN"`；被标出的用例上不比较 `sign`（E-13） |
| RI-1（你的附注） | 第二实现内部两份 QR：对第二实现而言是 RI-1 的偏差，记入开放问题 32，暂不处理 |
