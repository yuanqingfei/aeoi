# AEOI Changelog

> 从规范第 13 章移出（v0.4.18）。每个版本：起因、规范改动、已做的改动、开发中发现的问题、结果。最新的在前。

## v0.4.23-draft（2026-11-26）

起因：第三批，SVD 之上的五个函数：`rank`、`null`/`orth`、`pinv`、`norm`、`cond`。

作者的决定（2026-11-26）：

- 秩阈值缺省用 MATLAB 的 max(m, n)·eps(σ_max)，调用方可以给出 `tol`；有奇异值落在阈值附近时发 `W.NUMERICAL.RANK_AMBIGUOUS`。
- 秩不明确时，依赖秩的结果（rank、orth、null、pinv 的 X）标为不可依赖。
- `norm` 与 `cond` 分成两个算子。
- `null` 与 `orth` 合成一个算子 `linalg.subspace.basis`，一次给出两者。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | `profiles/10.37-rank.md`（新） | RK-1 – RK-5；秩不明确的区间 (tol/10, 100·tol]，后置条件 `rank_well_separated` | 数值秩取决于阈值；pinv 在阈值处不连续 |
| 2 | `profiles/10.38-basis.md`（新） | BS-1 – BS-5；基按投影矩阵比较；`orthonormal`、`null_residual` | 基不唯一 |
| 3 | `profiles/10.39-pinv.md`（新） | PI-1 – PI-5；Penrose 条件作后置条件，含被舍去部分 | — |
| 4 | `profiles/10.40-norm.md`（新） | NM-1 – NM-5；[1, n]、[n, 1] 按向量（同 MATLAB）；矩阵一般 p 报 RANGE | — |
| 5 | `profiles/10.41-cond.md`（新） | CN-1 – CN-6；p ≠ 2 用与 inv 同一的求逆 | σ_min 只有绝对精度，cond 的相对误差约 u·cond |
| 6 | `profiles/10.23-svd.md` SV-2 | σ_k ≤ max(m, n)·ε·σ_max 的奇异向量改由正交补全给出 | 见下面“开发中发现的问题”第 1 条 |
| 7 | 第 10 章索引 | 加 10.37 – 10.41 | — |

区间不对称（作者选的是“阈值附近 100 倍内”）的原因：精确秩亏矩阵算出的“零”奇异值约为缺省 tol 的 1/(2·max(m, n))，下沿取 tol/100 会把每个秩亏矩阵都报成秩不明确；golden 上的噪声全部低于 tol/10。

**已做的改动**

| 位置 | 改动 |
|---|---|
| `kernels::svd` | 新增 `rank_decision`（svd、rank、basis、pinv 共用）；数值零奇异值的处理（上表第 6 条） |
| `kernels::dense` | 新增 `inverse_refined`（inv 与 cond 共用；inv 的结果逐位不变） |
| `aeoi-numeric/src/subspace.rs`（新） | 五个算子 |
| `scripts/golden/gen_golden_subspace.py`（新） | 真值：mpmath 50 位 SVD（核对重构残差 < 1e-40）、求逆、p-范数；1、inf 范数与 Frobenius 平方另用有理数；警告只按真值预测，只收离每个阈值边沿至少 2 倍的用例 |
| `conformance/rules_numeric.py` | `rule_rank`、`rule_basis`、`rule_pinv`、`rule_norm`、`rule_cond`；运行器重算缺省 tol 与秩、基的正交性与 ‖A·N‖、Penrose 条件、rcond |
| 代理变异 | `rank_off_by_one`、`rank_overclaims`、`basis_null_scaled`、`pinv_scaled`、`norm_rel_1e12`、`cond_reciprocal`（共 63 种） |
| `examples/linalg.decomp.svd/response.full.json` | 第三个奇异值 1.5e-16 属于数值零，u₃ 改由补全给出（张成同一子空间） |

**开发中发现的问题**

1. **SVD 内核的缺陷。** 4×9 的秩 1 整数矩阵，Jacobi 留下 σ₂ = 1.8e-172 的“残余列”：它与其他列的内积下溢为 0，旋转判据被“满足”，归一化后与第一个奇异向量平行。`linalg.decomp.svd` 的后置条件 `orthonormal_v` 如实失败（所以从未被当作已验证），但结果是错的。新算子的 parity 测试首先撞上它。
2. pinv 的 Penrose 阈值最初没有算上被舍去的奇异值：调用方给出较大的 `tol` 时 ‖AXA − A‖ 本来就等于被舍去部分。
3. 生成脚本里 eps(0) 先下溢成 0，零矩阵的缺省 tol 与实现不一致；改用 `math.ldexp(1, −1074)`。

**结果**

- 82 个新 golden 用例（65 个成功、17 个错误），全部通过；最坏用掉容差：rank 2.3e-2、basis 2.3e-2、pinv 6e-3、norm 1.8e-2、cond 1e-3。
- 目标进度 49 / 102（48%），46 个算子。

## 仓库整理（2026-11-26，规范内容不变）

作者要求清理仓库。

| 改动 | 说明 |
|---|---|
| 删除 `spec-v0.1.md`、`spec-v0.2.md`、`spec-v0.3.md` | 已被 v0.4 取代；仍可在 tag [`v0.4.9`](https://github.com/yuanqingfei/aeoi/tree/v0.4.9) 中查看 |
| 删除 `legacy/` | v0.1 的构建脚本与源稿 |
| 删除 `reports/receipts-wasm-verify.json` | 只被 v0.3 规范引用，没有程序再生成它 |
| 根目录的 Python 脚本移到 `scripts/` | `write_manifests.py`、`regen_examples.py`、`validate_examples.py`、`build_spec.py`、`mcp_session_demo.py`；都从仓库根目录运行，例如 `python3 scripts/write_manifests.py` |
| golden 生成脚本移到 `scripts/golden/` | 22 个 `gen_golden_*.py`、`make_examples_b2.py`、`compute_examples.py` |
| `computed_values.json` 移到 `golden/` | v0.1 示例的 python-control 数值，`aeoi-control/tests/examples.rs` 使用 |

规范、AGENTS.md、skill、README、`docs/reference-impl.md`、各 `parity.yaml` 与测试注释里的路径已同步更新；本文件中更早的条目保留当时的文件名。全套验证结果不变（1278 / 0 / 217）。

## v0.4.22-draft（2026-11-26）

起因：按 12.3 的依赖层自底向上加算子，第二批是 `inv` + `det`。两者都建在 `kernels::dense` 的 LU 上，与 `linalg.system.solve` 同一内核（RI-1）。

作者的决定（2026-11-26）：

- `inv` 遇到病态矩阵（rcond < ε）仍返回结果并发警告；结果没有有效数字时把 X 标为不可依赖。
- `det` 同时返回 `det`、`log_abs_det` 与 `sign`，后两者不会溢出。
- `det` 带相对误差估计；估计 ≥ 1 时标为“与零不可区分”。
- 只支持实数矩阵，与 `linalg.system.solve` 0.4.0 一致。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | `profiles/10.35-inv.md`（新） | IV-1 – IV-6；后置条件 `backward_stable`（逐列）与 `forward_error_below_one`（κ₁·η < 1），后者不成立时 `/X` 不可依赖 | MATLAB 的 `inv(hilb(13))` 只打印一行警告，返回的矩阵一位有效数字也没有 |
| 2 | `profiles/10.36-det.md`（新） | DT-1 – DT-6；尾数/指数分开累乘；误差估计与 `zero_within_error`；后置条件 `significant` | MATLAB 的 `det(magic(4))` = −1.4495e-12，常被误读为可逆；150×150 的矩阵 det 就会溢出 |
| 3 | 5.1 | 新码 `W.NUMERICAL.DET_NOT_SIGNIFICANT`、`W.NUMERICAL.DET_OUT_OF_RANGE` | — |
| 4 | 第 10 章索引 | 加 10.35、10.36 | — |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `kernels::dense` | 新增 `lu_solve_refined`（LS-2 的精化循环从 `linsolve` 移入内核，`inv` 与 `linsolve` 共用；`linsolve` 的示例逐位不变）与 `Lu::perm_sign` |
| `aeoi-numeric/src/matrix.rs`（新） | `linalg.matrix.inv`、`linalg.matrix.det` |
| `gen_golden_matrix.py`（新） | 真值：mpmath 80 位；n ≤ 8 与全部奇异用例另用有理数精确计算并断言一致；警告与后置条件只按真值预测，只收明确落在一侧的用例 |
| golden | 放在 `golden/mpmath/linalg.matrix.{inv,det}/`（V-3：比较对象是 mpmath 真值；不再重复 PY-65 的问题） |
| `conformance/rules_numeric.py` | `rule_inv`（对真值、运行器用有理数重算逐列后向误差、重算 κ₁、核查 `forward_error_below_one` 是否虚报）、`rule_det`（ln|det|、det、符号对真值；奇异数据上 `zero_within_error = false` 是虚报） |
| 代理变异 | `inv_transposed`、`inv_overclaims`、`det_sign_flipped`、`det_never_zero`、`det_log_wrong`（共 57 种） |
| `roadmap/targets.py` | inv、det 完成：45 / 102 |

**结果**

- inv：23 个 golden（15 成功、8 错误），最坏用掉容差 2e-4；15 个成功用例中 13 个与真值逐位相同，10 个比 numpy 准（Hilbert 10：本算子 0，numpy 2.3e-6）。
- det：23 个 golden（18 成功、5 错误），ln|det| 最坏用掉容差 8e-3；非奇异用例的误差估计 15 / 15 不小于实际误差；三个奇异用例都标为与零不可区分。

## v0.4.21-draft（2026-11-26）

起因：第二实现第二轮。Python 实现只读规范，新增 `control.lti.bode`、`step`、`convert` 与 `fit.poly.roots`（共享层 `aeoi_lti.py`；数值内核自写，结果路径上没有 python-control、scipy.signal、numpy.roots、LAPACK 特征值）。它报告了 33 处发现（PY-33 – PY-65），其中阻塞 3 处：step 的指标定义只能去读 python-control 源码（PY-50）；convert 的 MIMO ss→tf 警告只出现在 parity.yaml 里（PY-57）；套件没有 convert 的数值规则，76 个成功用例全部记为跳过（PY-60）。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | `profiles/10.01-bode.md` | 不编号的“语义要点”改为 BD-1 – BD-8：频点生成与补丁、tf/zpk/ss 的求值与误差界、极点/零点判据、相对误差阈值、NaN 处的相位展开、后置条件 | PY-44 – PY-49 |
| 2 | `profiles/10.02-step.md` | 改为 ST-1 – ST-9：ZOH、网格与 dt 容差、计算量补丁、抽样舍入、稳定性与 y∞、全部阶跃指标的定义、info 字段、后置条件 | PY-50 – PY-56；此前定义的一半在 python-control 源码里 |
| 3 | `profiles/10.04-convert.md` | 改为 CV-1 – CV-8：improper 输入与目标无关、各路径与 `realization` 表（含 MIMO 块的顺序与位置、zpk→ss）、非最小实现警告的两种情形、G ≡ 0、前导零与归一化、测试点公式、一致性判定 | PY-37、PY-38、PY-42、PY-43、PY-57 – PY-60 |
| 4 | 3.1 表格、T-5 | `time_unit` 可省略；共轭配对判定写明（τ = 1e-12·max(1, \|c\|)，一一配对）；1×1 嵌套按 SISO；tf/zpk 与 ss 非有限值报不同的码是有意的 | PY-33 – PY-36 |
| 5 | 3.10 R-6、R-8、R-9 | R-6：MIMO 块顺序、zpk→ss、MIMO ss→tf 也是非最小；R-8：k = C·A^{r−1}·B，r 取自零点个数，不再判断 Markov 参数是否“非零”；R-9：只约束返回零点集合的算子，写明法秩检验与约化算法、容差 | PY-38、PY-41、PY-42 |
| 6 | 9.4 CF-7 | 逐元素的包含区域（如 roots 的半径）不与验收容差比大小，改由规则核查真值在区域内 | PY-64 |
| 7 | `profiles/10.34-roots.md` | RT-2 的内核名只约束参考实现；精确零根的 multiplicity 与簇的中心、半径写明；RT-4 引用 CF-7 的例外 | PY-61 – PY-64 |
| 8 | 12.1 | 开放问题 31：第二轮尚未处理的细节（零 DC 增益与稳定性的阈值、ss→tf 的 `normalize = false`、roots 的 golden 目录名） | — |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `conformance/rules_control.py` | `rule_convert`：运行器由请求独立重算被测结果相对输入系统的频率响应差（CV-7 的测试点），容差 max(1e-8, 10·d_ref)，d_ref 是 golden 期望系统的同一量；另查 `realization`、`ts`、ss 的状态数 |
| `examples/control.lti.convert/parity.yaml` | 加 `conformance_compare` |
| 代理变异 | `convert_gain_wrong`、`convert_realization_mislabeled`（共 52 种） |
| `write_manifests.py` | bode 的 `bound_kind` 由 `a_priori` 改为 `a_posteriori_heuristic`（ss 路径的估计是启发式，取最弱者）；示例响应随之更新（结果与指纹不变） |
| `conformance/impls/python_scipy/` | 新增 bode、step、convert、roots 与共享层 `aeoi_lti.py`、`aeoi_kernels.py`；`SPEC_FINDINGS.md` 第二轮与处理表 |

**开发中发现的问题**

- 参考实现的 convert 对 SISO 的 G ≡ 0 返回 k = 0，而 R-9 要求 `DEGENERATE_SYSTEM`：两者针对的对象不同（表示与零点集合），矛盾在措辞上，行为没有改。
- convert 的 profile 只在 MIMO tf→ss 处提到非最小实现警告，参考实现对 MIMO ss→tf 也发；规范落后于实现。

**结果**

- 参考实现：1227 通过 / 0 失败 / 217 跳过（共 1444 个用例；convert 的 76 个成功用例由跳过变为按数值判定）。
- 第二实现：9 个算子 528 通过 / 0 失败 / 0 跳过（第二轮报告时 454 / 0 / 74）。
- 有数值规则的算子：27 / 39。

## v0.4.20-draft（2026-11-26）

起因：按 12.3 的依赖层自底向上加算子，第一批是作者选定的 `polyval` + `roots`。两者都建在已有的 `aeoi-core::poly` 内核上（`roots` 用的求根代码与 `poles_zeros`、`filter` 相同），算子本身很薄；工作量在于把精度如实报告出来。

作者的决定（2026-11-26）：

- `roots` 的根按实部升序、实部相同按虚部降序，与 `poles_zeros` 相同。
- 根总是以 c128 返回。
- 每个根带误差半径；有重根或密集根时发簇警告，簇内的根标为不可单独依赖。
- `polyval` 用补偿 Horner，每个点带严格误差界。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | `profiles/10.33-polyval.md`（新） | PV-1 – PV-8 | 重根附近普通 Horner 一位也不剩（(x − 1)⁶ 在 1.001 处，numpy 给出 −2.2e-15，真值 1e-18） |
| 2 | `profiles/10.34-roots.md`（新） | RT-1 – RT-8 | MATLAB 的根顺序不可复现；重根精度只有 u^(1/m)，要告诉调用方哪些根不能单独相信 |
| 3 | 5.1 | 新码 `E.DOMAIN.ZERO_POLYNOMIAL`、`W.ACCURACY.CLUSTERED_ROOTS` | — |
| 4 | 第 10 章索引 | 加 10.33、10.34 | — |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `aeoi-core::poly` | `comp_horner`（补偿 Horner 与严格误差界）、`inclusion_radii`（Braess–Hadeler / Carstensen 包含半径，含重合近似值的处理）、`disc_components`、`gamma`；单元测试 |
| `aeoi-numeric/src/polynomial.rs`（新） | `fit.poly.polyval`、`fit.poly.roots` |
| `gen_golden_poly.py`（新） | 各 20 个 golden；polyval 的真值用有理数精确计算，roots 的真值用 mpmath 60 位；生成时独立核对包含定理 |
| `parity_poly.rs`、`examples_numeric.rs` | parity 与示例测试 |
| `conformance/rules_numeric.py` | `rule_polyval`（运行器用有理数重算真值，并核查误差界是否属实）、`rule_roots`（顺序、真根是否都在半径内、单根精度）；5 个代理改写 |
| `roadmap/targets.py` | polyval、roots 标为已完成：102 个里 43 个（42%） |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | Newton 修正把二重根的两个近似值推到同一个 f64（都是 1.0），包含定理不能用，半径成了 ∞ | 先把重合的 k 个值放到半径 max(|c|, 1)·u^(1/k) 的小圆上，再用定理，半径加上放开的距离（RT-4） |
| 2 | 第一版 polyval 警告规则“误差界 ≥ |y|”在每个精确根上都会触发（y = 0，界 1e-29），没有信息量；第二版加了 u·p̃ 的下限，结果次数 < 4800 时永远不会触发 | 定为 y ≠ 0 且误差界 > 1e-8·|y|；算得恰为 0 的点不警告，界本身就是回答（PV-4）。golden 生成脚本按真值独立预测哪些用例该警告 |
| 3 | 单元测试在 x = 1 + 2⁻¹⁰ 上比较补偿与普通 Horner：二进位的点上普通 Horner 本身就是精确的 | 改用非二进位的点 |

**结果**

- polyval：177 个求值点都在套件容差内（最坏 2.7e-3），都在报告的误差界内；157 个点比 numpy 准，没有一个点不如 numpy。
- roots：75 个单根都在容差内（最坏 1.7e-2）；每个真根都在报告的半径内；7 个真重根都在报告的簇里。
- 一致性：有数值规则的算子 26 / 39；代理改写 50 种，全部被抓住。

## v0.4.19-draft（2026-11-25）

起因：作者同意用只读规范的第二实现检验规范。一个只能读核心协议、对应 profile、schemas 和 golden 请求的子 agent，用 Python 写了 `fft`、`linalg.system.solve`、`interp1`、`integral`。它不能读 Rust 代码、golden 生成脚本、一致性规则和参考实现说明，manifest 也要自己写。凡是规范没说清楚的地方，它都记成一条发现，而不是去反推参考实现。结果：四个算子都通过了一致性套件，同时找出 32 处规范问题（`conformance/impls/python_scipy/SPEC_FINDINGS.md`）。其中 3 处只读 profile 就会写错。

**规范改动**

| # | 位置 | 改动 | 发现 |
|---|---|---|---|
| 1 | 第 10 章、全部 profile | 开头写明算子版本与容差类（版本号进入 `fingerprint.request`） | PY-1、PY-2（阻塞） |
| 2 | 10.20 IP-1 | n < 2 报 `SHAPE`（参考实现与 golden 一直如此，profile 写成了 `GRID`） | PY-21（阻塞） |
| 3 | 10.31 IN-1 | 同一个无穷大报 `RANGE`；有限 a = b 结果为 0 | PY-26（阻塞） |
| 4 | 10.31 IN-2、IN-3、IN-5 | 二分哪个子区间、650 怎么数、变量替换的 t 范围、初始等分在 t 上、容差都为 0 的行为、溢出的报法 | PY-24、25、29 |
| 5 | 10.17 FT-4、FT-6 | 逆变换的频率轴；sc 的定义；抽查只取输出范围内的频点 | PY-16、17 |
| 6 | 10.19 LS-2、LS-6 | 精化的停止与计步写成算法；补偿算术是精度下限；`refinement` 的回显位置；`residual_norm` 编码；复数 b | PY-18、19、20 |
| 7 | 10.20 IP-2、IP-3、IP-5 | previous / next 的定义；引用改为 X-6；两种尺度的公式 | PY-22、23 |
| 8 | 3.15 EX-5 | 嵌套深度按语言结构定义 | PY-12 |
| 9 | 3.11 F-7 | `user` 只留测量值；问题给定的数是 `supplied`；有缺省的字段不能是 `user` | PY-28 |
| 10 | 2.1 M-6、7.3、8.1 | `binding` 的语言无关定义；没有凭证的实现报 C 或 U；`outputSchema` 允许 null | PY-4、3、7 |
| 11 | 4.1 E-11a（新） | profile 点名的后置条件是最小集合 | PY-32 |
| 12 | 9.4 CF-8、CF-11 | 运行器校验 manifest；同源的机械判定 | PY-31、30 |
| 13 | 12.1 | 新开放问题 28（7 处小的未规定细节）、29（放宽补丁掩盖发散）、30（估计常数写成先验界） | PY-5 等、27、15 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `conformance/impls/python_scipy/` | 第二实现从 1 个算子扩到 5 个：共享层 `aeoi_common.py`、表达式语言 `aeoi_expr.py`、每个算子一个模块；新算子的 manifest 是自己写的 |
| `conformance/tests/mutant_server.py` | 按新结构重写，8 种注入缺陷不变 |
| `conformance/aeoi_conformance.py` | CF-8 校验 manifest；CF-11 同源判定改为机械规则 |
| `rust/aeoi-numeric/src/expr.rs` | 嵌套深度按 EX-5 的新定义计（原先约 40 层括号就报 `too_deep`），加单元测试 |
| `write_manifests.py` | 13 处 `x-aeoi-provenance` 改为 `supplied` 或 `convention`；去掉从未发出的 `ENCODING`、`CANCELLED`，改为声明核心实际会发的 `DIGEST_MISMATCH`、`REF_UNSUPPORTED`、`REF_FORBIDDEN` |

**结果**

- 第二实现（Python）：5 个算子 233 通过 / 0 失败 / 0 跳过。fft、linsolve、interp1 第一次运行就全部通过。
- 同源判定改用机械规则以后，第二实现的 fft、linsolve、interp1、integral 都算独立通过，只有 lyap（scipy LAPACK，与 python-control 同族）仍算同源。
- 参考实现：示例输出逐位不变；一致性 1111 / 0 / 291；54 个变异检查全部通过。

## v0.4.18-draft（2026-11-25）

起因：规范源稿 3400 行，生成后 16000 行、819 KB（168 处把示例响应与 parity.yaml 整段内联），AI agent 和第二实现者都很难读完；版本历史、测量记录和参考实现细节混在规范性条款里。作者同意拆分，并同意把 Changelog 移成独立文件。没有条款被删除或改变含义。

**改动**

| # | 内容 | 去向 |
|---|---|---|
| 1 | 第 13 章 Changelog 与文件头的 22 段版本摘要 | `CHANGELOG.md`（摘要与各版本条目重复，只保留条目） |
| 2 | 第 10 章 32 节算子契约 | `profiles/10.NN-<op>.md`，条款编号不变；示例与 parity.yaml 改为链接；核心协议第 10 章是索引表 |
| 3 | 8.2 Rust trait、8.3 的实现状态表、8.4 中 `aeoi-web` 的说明与评测结果、8.5 浏览器版、第 11 章（含 RI-1 – RI-3）、2.1 M-7 里的两段实现记录 | `docs/reference-impl.md`，节号沿用 |
| 4 | 8.4 里约束所有 LLM 绑定的部分（函数声明的生成与有损映射、AL-1 – AL-3、F-5(c) 来源核对） | 留在核心协议 |
| 5 | 已解决的 6 个开放问题（15、19 – 23） | `CHANGELOG.md` 附录，编号保留 |
| 6 | 11.3 v0.3.2 的测试结果（已过时）、4.3 末尾的 v0.3.3 实测段 | 删除；当前结果见 `reports/` 与 `conformance/README.md` |
| 7 | 标题与条款里只记版本号的括注（“v0.2 新增”等，41 处） | 删除，历史见本文件 |
| 8 | 12.2 下一步重写；12.3 只留汇总表，完整清单链接到 `roadmap/layers.md` | — |
| 9 | 9.3 新算子接入清单加两步：写 `profiles/` 文件、记 CHANGELOG | — |
| 10 | `build_spec.py` 检查核心协议、profiles、docs、CHANGELOG 里的全部相对链接 | 断链时退出码 1 |

**结果**

- 核心协议 `spec-v0.4.md`：885 行、131 KB（原 16261 行、819 KB）。
- `profiles/`：32 个文件，合计 1298 行；`docs/reference-impl.md` 207 行；`CHANGELOG.md` 约 750 行。
- 35 个文档 0 个断链。

## v0.4.17-draft（2026-11-25）

起因：作者确认 12.3 的清单与“先抽出 10 个内核”的顺序，并决定 102 个函数全部完成后才发布 v0.5。12.3 里有 10 个内核还藏在单个算子里，下一层的算子（nyquist、kalman、cheby、integral2、ode23……）都要用到它们，所以先抽出来。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 11.5 RI-1 | 列出 25 个内核；内核以闭包接收被积函数、右端与取消检查，不依赖表达式语言；能被复用的部分在第二个使用者出现前就放进内核 | 控制层（没有表达式）与数值层要共用同一个内核 |
| 2 | 12.2、12.3 | 10 个内核标为已有；发布计划改为“102 个完成后发布 v0.5” | 作者决定 |

**已做的改动**

| 内核 | 来自 | 内容 |
|---|---|---|
| `sum` | sysalg、fft | TwoSum、TwoProduct、Dot2、Neumaier |
| `bracket` | fzero | Brent zeroin |
| `newton` | fsolve | Powell dogleg 信赖域、后验 Newton 修正与 κ₁(J) |
| `quad` | integral | GK(7, 15)、全局自适应、变量替换、再二分检验 |
| `rk` | ode.ivp.solve | Dormand–Prince 5(4)、稠密输出、刚性检测 |
| `spline` | interp1 | pchip / not-a-knot 斜率、dgtsv、PPoly 系数 |
| `riccati` | lqr、dlqr | CARE 符号函数 + Newton–Kleinman + 精化增益；DARE 的 SDA + Newton–Hewer |
| `freqresp` | bode、margin | 求值点、Horner 误差界、zpk 乘积、预解式、相位卷绕/展开 |
| `timeresp` | step、lsim、c2d | 零阶/一阶保持离散化（增广 expm）、递推 |
| `iir` | butter（及 filter 内核） | 原型、频率变换、双线性变换、zpk→tf/sos、设计流程 |

**结果**

- 每抽一个内核都重新生成全部示例：`output_hash` 一个都没有变；全部 Rust 测试通过；一致性 1111 / 0 / 291；45 种代理改写都被抓住。

## v0.4.16-draft（2026-11-25）

起因：作者定下规则——算子之间有依赖时 MUST 用 AEOI 自己的实现，不得重写，也不得改调外部库；已有算子一次性检查并改正（11.5 RI-1 – RI-3）。本版没有新算子。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 11.5 RI-3 | 迁移完成；写明仍在用的外部数值代码（nalgebra 只作容器与矩阵乘法，`libm`）和检查办法 | 让以后的审计可以机械地做 |
| 2 | 10.x FT-5、2.1 | 参考实现的 FFT 改为 `kernels::fft`；rustfft 补丁副本删除 | — |
| 3 | 12.2、12.3 | 12.3 改为逐函数清单（102 个 MATLAB 函数、33 个共享内核、依赖层），由 `roadmap/targets.py` 生成；下一步自底向上实现 | 作者建议：按依赖图从底层开始 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `kernels::dense` | 加 Cholesky 与 `chol_solve`、`Lu::entry` |
| `kernels::cdense`（新） | 复数 LU（按 cabs1 选主元）、复数 Jacobi 奇异值、复数 Householder QR 的完整 Q（zgeqr2/zlarfg 约定） |
| `kernels::svd` | 加 `svd_thin`；迭代内核改收一个检查闭包（不再依赖 ExecCtx），这样控制层也能调用 |
| `kernels::eig` | `complex_schur` 从 `general` 拆出，供 `sysalg` 的 Lyapunov 求解器复用 |
| `kernels::schur`（新） | 实 Schur：Householder 化为 Hessenberg 形，Francis 双位移 QR（第 10、20 次迭代用特例位移，预算 max(30n, 100)），之后按 LAPACK dlanv2 把实特征值的 2×2 块标准化为上三角 |
| `kernels::fft`（新） | 基 2、混合基按时间抽取、小素数直接 DFT、大素数 Bluestein；旋转因子用 `libm::cos/sin` 并按 (n, 方向) 缓存 |
| `kernels::nalg`（新） | nalgebra `DMatrix` 与内核之间的适配层（solve、inverse、Cholesky、奇异值、对称特征、QR、复数版本） |
| `aeoi-core` linalg、sysalg、lti | cond2、Padé、Lyapunov、补基、行压缩、PBH、Schur 全部改调内核 |
| `aeoi-control` | bode、c2d、connect、ctrb_obsv、dlqr、lqr、lyap、place、poles_zeros、step 全部改调内核 |
| `aeoi-numeric` | `fft` 改用 `kernels::fft`；`eig`、`svd` 改用新接口；去掉 rustfft 依赖 |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | 自写 Schur 后 `eig` 的 parity `general_companion` 失败：实特征值的 2×2 块没有标准化，后续 rsf2csf 得到的向量与参考的取法不同 | 按 dlanv2 的实数情形旋转成上三角块 |
| 2 | Bluestein 的 Parseval 误差 3%：旋转因子缓存只按 n 作键，正变换与逆变换的旋转因子互相覆盖 | 缓存键改为 (n, 方向) |

**结果**

- 38 个示例的 `output_hash` 变了，数值差都在各自的容差类内（G-3）；parity 最坏值：`fft` 对 mpmath 5.3e-16，`eig` 值误差 / 容差 0.0013，`lqr` 的病态 R 极点相对差 6.4e-8（容差 2.0e-7）。
- 全部 Rust 测试通过；一致性 1111 / 0 / 291；45 种代理改写都被抓住。

## v0.4.15-draft（2026-11-25）

起因：四批中的最后一批，积分。作者定了两件事：
- integral 缺省 rel_tol = 1e-10，并回显容差、给出误差估计；
- 达不到容差时报错，附上当前估计值。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 10.31 IN-1 – IN-6 | 新算子 `calculus.quadrature.integral` 0.4.0：自适应 GK(7, 15)，变量替换同 MATLAB；误差估计取 Kronrod 估计与“再二分一次的变化”中较大的那个 | 端点奇异时，单靠 Kronrod 估计会偏小 10 倍 |
| 2 | 10.32 TZ-1 – TZ-3 | 新算子 `calculus.quadrature.trapz` 0.4.0：补偿求和；x 或 dx 必须显式给出 | MATLAB 缺省步长 1，常被误用 |
| 3 | 3.15 EX-2（实现） | `t` 只在 ODE 里是保留名（规范原文就是这样写的，实现一直把它全局保留）；fzero、fsolve、integral、优化算子都可以用 t 作变量 | 积分、求根里用 t 作变量很常见；v0.4.13 记下的问题，这里修正 |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | 无穷区间上，GK 节点在舍入后可能恰好落在 t = 1，映射到 x = ∞，得到 0·∞ = NaN | 该点的贡献记为 0（IN-3） |
| 2 | 1/x 在 [0, 1] 上会求值到 x = 0，得到 Inf，所以走的是 `NON_FINITE_EVALUATION`，不是不收敛 | 两种都是如实的报告；golden 的不收敛用例改用 1/x 在 [1, ∞) 上：处处有限，但积分发散 |

**结果**

- golden：`integral` 19 个（13 个成功 / 6 个错误），真值来自 mpmath，且与闭式交叉核对；`trapz` 12 个（7 / 5），全部与精确的有理数求和相同。
- 一致性：24 / 37 = 65% 的算子有数值规则，参考实现 1111 / 0 / 291；新增 3 种代理改写，都被抓住，其中包括“把误差估计少报 10⁶ 倍”。

## v0.4.14-draft（2026-11-25）

起因：四批中的第三批，优化。作者定了三件事：
- fmincon 一次支持边界、线性与非线性约束；
- 每个结果都声明只是局部最优，并以 KKT 残差作为后置条件；
- linprog 用单纯形法，给出顶点与乘子。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 10.28 LP-1 – LP-4 | 新算子 `optim.linear.linprog` 0.4.0 与 LP 内核 | 变量缺省无界（同 MATLAB）并回显；scipy 缺省 x ≥ 0，同一请求可能得到相反的结论 |
| 2 | 10.29 FC-1 – FC-5 | 新算子 `optim.local.fmincon` 0.4.0：SQP + QP 内核 + 自动微分梯度；乘子按 MATLAB 的分组 | 只保证局部最优，写进 receipt 的前提 |
| 3 | 10.30 NM-1 – NM-4 | 新算子 `optim.local.fminsearch` 0.4.0：Nelder–Mead；用不依赖导数的探测检查做后置条件 | — |
| 4 | 11.5 | 新内核 `kernels::lp`、`kernels::qp`（QP 用 LP 求可行起点，用 LU、QR 内核） | RI-1；quadprog 以后只需加一层外壳 |
| 5 | 5.1 | 新码 `E.DOMAIN.INFEASIBLE`、`E.DOMAIN.UNBOUNDED` | — |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | `fminsearch` 用例 `quadratic_3d` 的解析最优解手算错了（写成 (16/7, −11/7)，应为 (12/7, −10/7)），parity 测试抓到了 | 生成脚本里给每个手算的最优解加了独立的紧容差求解来交叉核对（AGENTS.md 第 6 条） |

**结果**

- golden：`linprog` 16 个（9 个成功 / 7 个错误），`fmincon` 13 个（7 / 6），`fminsearch` 10 个（5 / 5）。HS071 的 f* = 17.0140172891563。
- 一致性：22 / 35 = 63% 的算子有数值规则，参考实现 1066 / 0 / 291；新增 4 种代理改写，都被抓住，其中包括“乘子符号按 scipy 约定写反”。运行器从请求出发，自己重算 KKT 条件。

## v0.4.13-draft（2026-11-25）

起因：四批中的第二批，`fzero` + `fsolve`。作者为这一批定了三件事：
- fzero 优先用区间；也接受 x0，但回显搜到的区间并警告；
- fsolve 的雅可比用表达式自动微分；
- 不收敛时报错，并附上目前最好的点。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 3.15 EX-8 | 表达式语言的前向自动微分；不可导点的约定 | 精确导数；以后 fmincon、ode15s 共用 |
| 2 | 10.26 FZ-1 – FZ-5 | 新算子 `nonlinear.root.fzero` 0.4.0：Brent 方法；用后置条件 `not_a_pole` 区分根与极点 | 变号点不一定是根 |
| 3 | 10.27 FS-1 – FS-7 | 新算子 `nonlinear.system.fsolve` 0.4.0：dogleg 信赖域 + 自动微分雅可比 + 共享 LU；用线性收敛判断重根 | 一维时 κ(J) 恒为 1，只能靠收敛速度 |
| 4 | 5.1 | 新码 `E.DOMAIN.NO_SIGN_CHANGE`、`E.NUMERICAL.LOCAL_MINIMUM`、`W.DOMAIN.BRACKET_SEARCHED` | — |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | 3.15 把 `t` 保留给 ODE 的时间，所以 fzero 不能用 t 作变量名 | 维持保留（语言要简单），golden 改用 w；以后如果用户反映不便，再考虑按算子放开 |
| 3 | v0.4.12 的发布预演中，Windows 版在 8 个 `butter` golden 用例上的 `output_hash` 与其他平台不同：都是数字设计，都要用 tan 预畸变，而模拟设计都相同 | 滤波内核与表达式求值改为按名字直接调用编入的 `libm`（`aeoi_libm::libm::tan` 等）；复数平方根和整数次幂改为显式写出，不再依赖 `f64::tan`、num_complex 的极坐标开方和 `powi` 在各平台上解析到哪个符号（2.1 M-7a）。Linux、macOS 上的结果不变（原本就走同一个 libm）；Windows 上由发布流水线确认 |
| 2 | 从 (−1.2, 1) 出发解 Rosenbrock 梯度方程会失败，scipy 的 hybr 和 lm 也一样 | 作为错误用例收入 golden（`E.NUMERICAL.NO_CONVERGENCE`，附最好的点）；另从 (0.5, 0.2) 出发的用例能收敛 |

**结果**

- golden：`fzero` 24 个（15 个成功 / 9 个错误），`fsolve` 18 个（9 / 9），真值来自 mpmath。
- 一致性：19 / 32 = 59% 的算子有数值规则，参考实现 1012 / 0 / 291；新增 4 种代理改写，都被抓住。运行器用自己的求值器（Python）重算区间两端的 f 和 F(x)。

## v0.4.12-draft（2026-11-25）

起因：作者定下项目目标是约 100 个常用工程计算函数（12.3），并要求算子之间的复用必须调用同一内核（11.5 RI-1）。之后继续剩下的四批，这是第一批：`butter` + `filter`。作者为 `butter` 选了三件事：
- zpk 与 sos 为主，b/a 也给，但失真时警告；
- 截止频率必须显式二选一（Hz 加采样率，或 MATLAB 的 Wn），没有缺省；
- 支持模拟滤波器，并输出为 LTI 系统。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 11.5 RI-1 – RI-3 | 共享内核 `aeoi-core::kernels`（dense、svd、eig、filter）；已知约 45 处违反列入迁移 | 同一计算两份实现会不一致 |
| 2 | 12.3 | 目标算子清单约 100 个，及完成顺序 | 作者决定 |
| 3 | 10.24 BW-1 – BW-6 | 新算子 `signal.design.butter` 0.4.0 | 频率单位必须显式；b/a 失真时警告，不判整个结果失败 |
| 4 | 10.25 FL-1 – FL-6 | 新算子 `signal.filter.filter` 0.4.0：b/a 或 sos，多通道，zi/zf，差分方程残差作为后置条件 | — |
| 5 | 开放问题 26 | 第三个实例（`butter` 的模拟滤波器没有 sos） | — |

**结果**

- golden：`butter` 33 个（20 个成功 / 13 个错误），真值来自 50 位精度的独立构造；`filter` 24 个（15 / 9），真值来自 50 位精度的差分方程。
- 高阶失真的两个例子里，我们和 scipy 的 b/a 响应偏差都是 1.0，我们的 sos 偏差 ≤ 1e-11。
- 一致性：17 / 30 = 57% 的算子有数值规则，参考实现 962 / 0 / 291；新增 4 种代理改写，都被抓住，其中包括“删掉 b/a 警告”：运行器自己重算偏差来判断该不该有警告。

## v0.4.11-draft（2026-11-25）

起因：按 MATLAB 使用频率继续加算子。作者选了 `polyfit` + `eig`/`svd`，并定了三件事：
- `polyfit` 病态时照原样拟合，警告并附中心化缩放的补丁，补丁须用户确认，不悄悄缩放；
- 直接给出每个系数的标准误差；
- `eig` 先做实对称与一般实矩阵，排序与规一化写死在规范里；`svd` 支持 thin 与 full。

线协议标识、核心 schema 和已有算子的版本都不变。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 10.21 PF-1 – PF-7 | 新算子 `fit.poly.polyfit` 0.4.0：Householder QR 加修正半正规方程精化；输出标准误差、自由度、mu | 与 MATLAB `[p, S, mu]` 对应，但直接给出调用方真正要的标准误差 |
| 2 | 10.21 PF-3 | 秩判定、条件数、后向误差都在列缩放的 V·D⁻¹ 上计算 | 未缩放时，年份数据 3 次拟合就被误判为数值秩不足；缩放后只反映 x 的位置与跨度，与单位无关 |
| 3 | 10.21 PF-4、开放问题 27 | 病态警告的 `details.suggested_remediation` 带一个须用户确认的补丁 | 中心化缩放改变系数的含义，只能建议，不能代做；warning 目前没有 remediation 字段 |
| 4 | 10.22 EG-1 – EG-7 | 新算子 `linalg.decomp.eig` 0.4.0：对称 Jacobi / 一般 Schur + 三角回代；排序（对称升序；一般按 (Re, \|Im\|)，共轭对正虚部在前）与规一化（单位 2-范数，最大模分量为正实数）写死；报告每个特征值的条件数，> 1e8 时警告可能亏损 | 特征向量不唯一，不写死约定就无法比较、无法复现 |
| 5 | 10.22 EG-5、开放问题 26 | `orthonormal` 只适用于对称算法，一般算法按空真报告 | 开放问题 26 的第二个实例 |
| 6 | 10.23 SV-1 – SV-7 | 新算子 `linalg.decomp.svd` 0.4.0：单边 Jacobi；values / thin / full；u_k 的符号约定；数值秩用 MATLAB 的容差 | — |
| 8 | 11.5 RI-1 – RI-3、12.3 | 作者决定：目标是约 100 个常用工程计算函数（12.3）；算子之间复用 MUST 调用同一内核（`aeoi-core::kernels`），不得重写，也不得改调外部库；已知的约 45 处违反列入迁移计划 | 同一计算两份实现会不一致；nalgebra 的 SVD、对称特征分解刚被证明在分级矩阵上不准 |
| 7 | 9.4（`rules_numeric.py`） | 三个算子的一致性规则只比较不变量：特征值（最近匹配）、残差、正交性、规一化规则；运行器从请求重算残差 | 两个正确实现给出的 V 可以完全不同 |

**开发中发现的问题**

| # | 问题 | 处理 |
|---|---|---|
| 1 | nalgebra 的 SVD（双对角 QR）在行分级矩阵（行尺度 1e8 : 1 : 1e-8）上，重构残差为 8.7e-10，中间一行的信息丢了 | 后置条件 `reconstructs` 如实报了不成立；改用单边 Jacobi，残差降到 2.5e-16 |
| 2 | nalgebra 的 SymmetricEigen 在分级对称矩阵上的后向误差是 3e-13（≈ 1000u） | 后置条件 `residual_small` 如实报了不成立；改用循环 Jacobi |
| 3 | `polyfit` 的秩判定起初用未缩放的 R，年份数据 3 次就报 `E.NUMERICAL.SINGULAR` | 改为列缩放（PF-3） |
| 4 | `polyfit` 只做 QR 时，病态用例的误差比 numpy 大 10–20 倍 | 加了修正半正规方程精化，之后全部比 numpy 准 |

**结果**

- golden 用例数（成功 / 错误）：`polyfit` 24 个（14 / 10）；`eig` 28 个（22 / 6）；`svd` 20 个（15 / 5）。真值全部来自 mpmath。
- 最坏用例占容差的比例：`polyfit` 缩放后的前向误差 6.2e-4；`eig` 特征值 1.6e-3、特征向量 4.0e-6；`svd` 奇异值 2.3e-3、奇异向量 5.1e-6。
- 一致性套件：15 / 28 = 54% 的算子有数值规则；参考实现 900 / 0 / 291。新增 7 种代理改写，都被预期的检查抓住，其中包括两种约定类改写：共轭对顺序反了；u、v 同时变号，这个 SVD 本身等价，但违反了符号约定。

## v0.4.10-draft（2026-11-25）

起因：按 MATLAB 使用频率继续加算子，作者选了 `A\b` 与 `interp1`，并定了两件事：插值范围外缺省报错、外推要显式选择；线性方程组支持方阵与超定最小二乘，欠定报错。线协议标识（`aeoi/0.4`）、核心 schema 和已有算子的版本都不变。

**规范改动**

| # | 位置 | 改动 | 理由 |
|---|---|---|---|
| 1 | 10.19 LS-1 – LS-7 | 新算子 `linalg.system.solve` 0.4.0：方阵用 LU（部分选主元）加至多 3 步迭代精化，超定用 Householder QR；报告后向误差、κ₁ 估计和前向误差估计 | MATLAB 只在 rcond < ε 时打印警告；调用方需要从信封里看出解能信几位 |
| 2 | 10.19 LS-1 | 欠定报 `E.DOMAIN.UNDERDETERMINED`（新码），不返回 MATLAB 的“基本解” | 基本解不唯一，也不是最小范数解，悄悄返回会被当成“唯一答案” |
| 3 | 10.20 IP-1 – IP-7 | 新算子 `interp.grid.interp1` 0.4.0：linear、nearest、previous、next、pchip、spline，y 每行一个通道 | 与 `fft`、`lsim` 的通道约定一致 |
| 4 | 10.20 IP-3 | 范围外缺省 `E.DOMAIN.EXTRAPOLATION_REQUIRED`（新码），附 nan / clamp / extrapolate 三个须用户确认的补丁；实际外推时发 `W.ACCURACY.EXTRAPOLATED`（新码） | MATLAB 对不同方法的缺省行为不同（NaN 或外推），换方法会不声不响地改变结果的含义 |
| 5 | 10.20 IP-6、开放问题 26 | 只对部分方法有意义的后置条件按空真报告，`derived` 里注明是否适用 | E-11 把缺失的后置条件算作不成立；正式写法留作开放问题 |
| 6 | 5.1 | 登记上面三个新码 | — |

**实现与验证**

- golden：`linalg.system.solve` 30 个用例（10 个错误），真值来自 mpmath 60 位；`interp.grid.interp1` 56 个用例（12 个错误），参考值来自 numpy/scipy。
- 精化的效果：Hilbert 10 上 numpy 的相对误差为 7.2e-6，本算子为 0；Wilkinson 增长矩阵上 numpy 为 0.90，本算子为 1.3e-16。不做精化的 Wilkinson 用例如实报告 `backward_stable = false`（golden 标记 `expect_postcondition_failed`）。
- `interp1`：44 个成功用例中 29 个与 scipy 逐位相同，最大偏差 3.0e-16（相对）。
- 一致性套件：两个算子都有数值规则（12 / 25 = 48%），参考实现 813 / 0 / 291；新增 4 种代理改写，都被预期的检查抓住。

## v0.4.9-draft（2026-11-24）

起因：作者决定先做分发（12.2 第 6 项），并选定“全平台用同一个纯 Rust 数学库”。审计时发现，跨平台差异除了系统数学库，还有 `matrixmultiply` 按 CPU 选内核这第二个来源（2.1 早就记着，作者问起时我漏说了）。线协议标识、核心 schema、算子版本都不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 2.1 | 记录参考实现消除了跨平台差异的两个来源：编入二进制的 Rust `libm`，以及固定使用通用内核的 `matrixmultiply`；`reproducibility.libm` 记为 `rust-libm 0.2.16` | 让同一请求在各平台上逐位相同；系统数学库还会随系统升级改变 |
| 2 | 2.1 M-7a（新） | 声明 `bitwise` 的前提：超越函数来自编入 build 的数学库，不在编译期用编译机的 C 库求值，任何运算路径都不按运行时 CPU 特性分派；证据是各平台 golden 哈希完全相同 | 把上面的做法变成对所有实现的要求；“编译期求值”是 CI 找出来的第三个来源 |
| 3 | 3.15 EX-4、10.18 OD-8 | “平台的 libm”改为“实现所用的数学库” | 与 M-7a 一致 |
| 4 | 12.2 第 6 项 | 分发：多平台发布、Claude Desktop 一键安装包、MCP 公开目录；不做独立 GUI 和聊天客户端 | 作者决定 |
| 5 | 12.1 第 25 项（新） | `platform_bitwise` 何时升为 `bitwise` | 全平台逐位相同已证实，但改声明要给所有算子升版本 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `rust/aeoi-libm`（新 crate） | 在二进制内定义 `sin`、`cos`、`tan`、反三角、双曲、`exp`/`exp2`/`expm1`、`log` 系列、`cbrt`、`atan2`、`pow`、`hypot`、`sincos`、`__sincos_stret`（Apple）、`_hypot`（MSVC），转给 `libm` 0.2.16。`tests/override.rs` 在每个平台上检查 `f64` 方法与 `libm` 逐位相同（6 万个自变量）。在本机，同一组函数下 glibc 2.39 与 `libm` 的不同率是 3%–26%，所以这个测试确实能区分两者 |
| `rust/vendor/matrixmultiply` | 0.3.11 的补丁副本，经 `[patch.crates-io]` 使用：`detect()` 固定选通用内核（`AEOI-PATCH.md`） |
| `rust/vendor/rustfft` | 6.4.1 的补丁副本：旋转因子用 `libm::cos/sin`，不受编译机 C 库影响（`AEOI-PATCH.md`）。Linux 上只有 1 个 golden 用例（n = 31）的哈希因此改变 |
| `aeoi-libm` | 另覆盖 `__exp10`、`exp10`（苹果平台上 `10f64.powf(x)` 被改写成它们；`pade` 在 macOS 上的 8 个差异由此而来） |
| `conformance/` | Windows 上统一用 UTF-8 读写文件与子进程管道；`suite_hash` 的路径统一用 `/`；`.gitattributes` 规定检出用 LF |
| `aeoi-call mathprobe` | 14 类数学函数调用的结果摘要；发布流水线每个平台都打印它和二进制从系统导入的数学符号（各平台摘要相同；Windows、macOS x86_64 只导入 `fma`，它是正确舍入的） |
| `aeoi-core::operator` | `libm()` 报告 `rust-libm 0.2.16`；`build` 字符串加上 `matrixmultiply-0.3.11-portable libm-0.2.16`；`blas` 写明是通用内核 |
| Linux 二进制 | 不再从 glibc 导入任何数学函数（`nm -D` 已核对） |
| `examples/*/response*.json` | 重新生成：9 / 43 个成功示例的 `output_hash` 变了（数值差在容差类内，最大为 `bode` 非对称 ss 示例的相对差 8.9e-12；fft 示例 3.7e-17·max|X|），其余逐位不变 |
| `aeoi-call hashes <golden-dir>`（新） | 输出每个 golden 请求的 `output_hash`（错误用例输出错误码），供发布流水线做跨平台比较 |
| `.github/workflows/release.yml`（新） | 5 个平台（Linux x86_64/aarch64 musl 静态、macOS arm64/x86_64、Windows x86_64）上分别跑测试、一致性套件和 `hashes`；`compare` 任务要求各平台的结果完全相同；生成 `aeoi.mcpb`（macOS 通用二进制 + Windows exe）；打 tag 时发布 Release（SHA256SUMS、构建来源证明），并用 GitHub OIDC 登记到 MCP 公开目录 |
| `tools/write_mcpb_manifest.py`、`tools/write_server_json.py`、`release/NOTES.md`（新） | MCPB manifest（工具清单从二进制的 `tools/list` 读取，已用 `mcpb validate` 和 `mcpb pack` 试过）；registry 的 server.json；发布说明 |
| `.github/workflows/pages.yml` | 修复冒烟测试：它把工具数写死为 19，所以从 v0.4.5（`b19feab`）起的 5 次推送 Pages 构建都失败了，一直没人发现。现在改为从 `examples/` 的目录数推出 |
| `README.md`、`THIRD_PARTY_NOTICES.md` | 安装说明；登记 `libm` 与 `matrixmultiply` 补丁副本 |

**结果**：`cargo test` 181 / 0；一致性 715 / 0 / 291（5 个平台都是）；变异 28 / 28；`validate_examples.py` 退出码 0。浏览器版与 Linux 原生版 925 / 925 逐位相同。第 3 次试运行时 Linux x86_64 与 aarch64 的 859 个 golden 哈希完全相同，macOS、Windows 只剩 FFT 不同，原因是上面的 (b)。第 4 次试运行（`8c0a13c`）：5 个平台的 859 个 golden 哈希完全相同，`compare` 通过，MCPB 打包通过。

## v0.4.8-draft（2026-11-24）

起因：按 MATLAB 使用频率排的第二个非控制算子 `ode45`。作者选定右端用“安全表达式语言”给出：规范定语法、优先级、函数表与求值顺序，不执行代码，以后的领域复用。线协议标识与核心 schema 不变；已有算子的结果不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 3.15 EX-1 – EX-7（新节） | 表达式语言：EBNF、优先级（`^` 右结合且高于一元负号）、名字规则、函数表、逐节点 binary64 求值、规模上限、`E.VALIDATION.EXPRESSION`（`position`、`kind`） | 函数不能作为 JSON 数据传；执行代码既不安全也不可复现 |
| 2 | 10.18 OD-1 – OD-8 | 新算子 `ode.ivp.solve` 0.4.0（领域 `ode`）：Dormand–Prince 5(4)，步长决策与 scipy RK45 相同；状态与参数按名字给出 | 见上 |
| 3 | OD-4 | rtol < 100ε 报错，不像 scipy 那样悄悄抬高 | 达不到的精度要让调用方知道 |
| 4 | OD-5 | 用 tol/10 重解估计全局误差；`max_scaled` > 100 时 `tolerance_consistent` 不成立，`/y` 不可依赖 | rtol/atol 只控制局部误差；缺省容差下的 Van der Pol 误差 0.23 而求解器报告成功 |
| 5 | OD-6、OD-7、5.1 | 新码 `E.NUMERICAL.NON_FINITE_EVALUATION`、`E.NUMERICAL.STEP_SIZE_TOO_SMALL`、`W.NUMERICAL.STIFF_PROBLEM` | 爆破、步长失败与刚性需要不同的修复 |
| 6 | 9.4 CF-6 的应用 | ODE 的一致性对照精确解，界为 max(10·tol, 2·E_ref)；与参考求解器之差只记录 | 两个正确的 RK45 在折点处会因一次接受/拒绝决策不同而分岔：`functions` 用例两者差容差的 20.9 倍，相对精确解的误差却相同（1.60e-7、1.58e-7） |
| 7 | golden 约定 | 用例可写 `expect_postcondition_failed`：后置条件不成立就是正确答案（由生成脚本用 scipy 独立预测） | 原来的收据测试假定 golden 成功用例的后置条件全部成立；`pendulum`、`van_der_pol_mu1`、缺省容差的 Van der Pol 如实报告不成立 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `rust/aeoi-numeric/src/expr.rs`（新） | 3.15 的解析器与求值器（3 个单元测试） |
| `rust/aeoi-numeric/src/ode.rs`（新） | DOPRI5(4)、稠密输出、刚性检测、全局误差估计（5 个单元测试）；`tests/parity_ode.rs`（新）、`tests/examples_numeric.rs` 加 `ode_example` |
| `write_manifests.py` | `ode.ivp.solve` 的 manifest、schema、L-1 描述、receipt（`a_posteriori_heuristic`，后置条件 `all_finite`、`tolerance_consistent`） |
| `gen_golden_ode.py`（新）、`golden/python-control/ode.ivp.solve/0.4.0` | 34 个用例（17 成功、17 错误）；右端由独立的 Python 表达式实现求值；解析解另用 DOP853 交叉核对 |
| `conformance/rules_numeric.py`、`tests/proxy_mutant.py` | `rule_ode`；变异 `ode_y_wrong`、`ode_states_reordered`、`ode_overclaims` |
| `rust/aeoi-ops/tests/receipt.rs` | 认 `expect_postcondition_failed` |

开发中发现两处错误。第一处：`constant_rhs_precedence` 的精确解写错了（把 `- -2^2*3` 当成 −12，实际是 +12）。parity 测试没有报错，因为两个实现都按 3.15 正确求值，与 scipy 的误差相同，而界里含 2·E_ref。之后生成脚本对每个解析解都用 DOP853 交叉核对。第二处：第一版 parity 断言与 scipy 逐点相差 ≤ 10·tol，`functions` 用例因此失败（见规范改动第 6 条）。

**结果**：`cargo test` 180 / 0；parity 34 / 34（对精确解最坏用掉界的 0.50，估计误差 / 实际误差的中位数 0.92、最大 1.03）；参考实现一致性 715 通过 / 0 失败 / 291 跳过（`ode` 34 + 默认值 11 + 方法选择 1）；变异测试 28 / 28；`validate_examples.py` 退出码 0；有数值规则的算子 10 / 23 = 43%。

## v0.4.7-draft（2026-11-24）

起因：控制领域告一段落，作者决定按 MATLAB 的使用频率转向其他领域，先做 `fft` 与 `ode45`。这一版是 `fft`，以及为多领域做的结构调整。线协议标识与核心 schema 不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 10.17 FT-1 – FT-6 | 新算子 `signal.spectral.fft` 0.4.0（领域 `signal`） | 见上 |
| 2 | FT-4 | 频率轴总是返回，顺序为 numpy `fftfreq`，不做 `fftshift` | 频点下标与频率的对应是最常见的用法错误 |
| 3 | FT-5 | 快速算法的运算路径不得依赖运行时 CPU 特性 | rustfft 缺省规划器按 AVX/SSE 分派，`platform_bitwise` 会因 CPU 不同而失效 |
| 4 | P-1 补充 | `operator.build` 与 `receipt.replay.build` 报告实现所在的 crate（`aeoi-control …`、`aeoi-numeric …`） | 原来写死为 `aeoi-control`，对其他领域的算子是错的；控制算子的输出不变 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `rust/aeoi-numeric`（新 crate） | `fft.rs`（4 个单元测试）、`tests/parity_fft.rs`、`tests/examples_numeric.rs`；依赖 rustfft 6.4（标量规划器） |
| `rust/aeoi-ops`（新 crate） | 跨领域注册表 `registry()` / `find()`；`aeoi-call` 与遍历全部 golden 的收据测试 `tests/receipt.rs` 从 `aeoi-control` 移来（它们需要看到所有领域） |
| `rust/aeoi-{mcp,bridge,web,wasm}` | 改用 `aeoi_ops::registry()`；控制 crate 的测试仍用 `aeoi_control::registry()` |
| `rust/aeoi-core/src/operator.rs` | `OperatorMeta.crate_name`、`build_string_for(crate)`；`operator_meta!` 宏传入 `env!("CARGO_PKG_NAME")` |
| `write_manifests.py` | `man(…, backend=)`；`fft` 的 manifest、schema、L-1 描述、receipt |
| `gen_golden_fft.py`（新）、`golden/{python-control,mpmath}/signal.spectral.fft/0.4.0` | 38 个 numpy 用例、6 个 mpmath 精确用例；同时生成示例请求与参考值 |
| `conformance/rules_numeric.py`（新）、`aeoi_conformance.py` | `fft` 的一致性规则（含运行器用请求的 x 与返回的 X 独立重算 Parseval，b64 输入也解码）；变异 `fft_bin_wrong`、`fft_shifted_frequencies`、`fft_scale_wrong` |
| `examples/control.design.pidtune/manifest.json` | 修正 v0.4.6 的一处 manifest 错误：`limits.td_scan_log10_range` 是数组，manifest schema 要求 `limits` 的值是数字，改为 `td_scan_log10_min`、`td_scan_log10_max`。`validate_examples.py` 在 `8a997b4` 上就报了这个错，当时检查时只看了它输出的最后一行（汇总），漏掉了前面的 FAIL 行；之后以退出码为准 |

写 `fft` 的 manifest 时两次用了 schema 枚举之外的值（`bound_kind = a_priori_normwise`、覆盖 `all`），一致性套件的信封校验（CF-8）和 `validate_examples.py` 都抓到了。

**结果**：`cargo test` 170 / 0；parity 44 / 44（与 numpy 8.4e-16，与 mpmath 精确值 2.9e-16）；参考实现一致性 669 通过 / 0 失败 / 290 跳过（`fft` 44 + 方法选择 1 + 默认值 14）；变异测试 24 / 24；`validate_examples.py` 退出码 0；有数值规则的算子 9 / 22 = 41%。

## v0.4.6-draft（2026-11-24）

起因：按用途广度补的第二个算子。工业回路绝大多数是 PID，“帮我把 PID 调好”是最常见的设计请求；此前 Agent 只能自己猜参数再交给 `pid`、`loopsens` 验证。A-4 要求的四项由作者选定：指标为穿越频率 + 相位裕度，ωc 必须由调用方给出，PID 用 Ti = 4·Td 固定多出的自由度并取最小 Td 根。线协议标识与核心 schema 不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 10.16 PT-1 – PT-6、A-4 | 新算子 `control.design.pidtune` 0.4.0 | 见上 |
| 2 | PT-3 | ∠C* = 0°（纯 P）在舍入内 MUST 按 Ki = 0 处理 | 第一版在这个边界上报“不可达”，而错误消息又说所需相位 0° 在可达范围内 |
| 3 | PT-4 | Td 的求根法（扫描网格、二分终止条件、取最小根）是规范的一部分 | 方程可有多个根；规定取法后两个独立实现的增益相差 ≤ 7.3e-16 |
| 4 | PT-5 | 闭环不稳定报 `E.DOMAIN.TARGET_NOT_ACHIEVABLE`，额外穿越给 `W.DOMAIN.EXTRA_CROSSOVER`（两个新码） | 频率点条件只约束一个频率；golden 里两种情况都是从系统族中搜出来的真实例子 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `write_manifests.py` | `pidtune` 的 manifest、schema、L-1 描述、receipt（假设、后置条件、覆盖、guards） |
| `rust/aeoi-control/src/pidtune.rs`（新）、`lib.rs`、`loopsens.rs`（`sub` 改为 crate 内可见） | 实现与注册；4 个单元测试（PI 封闭解、需要超前时 PI 报错而 PID 成功、纯 P 边界、冲突与域错误） |
| `gen_golden_pidtune.py`（新）、`golden/python-control/control.design.pidtune/0.4.0` | 33 个用例，独立 numpy 实现 + python-control 核验；同时生成示例请求与参考值 |
| `rust/aeoi-control/tests/parity_pidtune.rs`（新）、`examples.rs` 的 `pidtune_example` | parity 与示例测试；示例测试同时核对 controller 与 `control.design.pid(gains)` 逐位相同（K-4） |
| `examples/control.design.pidtune/` | manifest、schema、三个示例请求与响应、`parity.yaml`（含 `conformance_compare`） |
| `conformance/rules_control.py`、`tests/proxy_mutant.py`、`test_runner_catches_mutants.py` | `pidtune` 的一致性规则（含运行器用返回的控制器重算 L(jωc)）；变异 `pidtune_kp_wrong`、`pidtune_controller_wrong` |

一致性套件在第一次运行时就查出一处 manifest 错误：`pidtune` 会发出 `E.DOMAIN.MIMO_NOT_SUPPORTED`，但没有声明（CF-8），已改正。

**结果**：`cargo test` 164 / 0；parity 33 / 33（增益 7.3e-16）；参考实现一致性 610 通过 / 0 失败 / 290 跳过（`pidtune` 33 + 方法选择 1 + 默认值 8 通过、6 跳过）；变异测试 21 / 21；有数值规则的算子 8 / 21 = 38%。

## v0.4.5-draft（2026-11-24）

起因：作者决定按用途广度补算子，第一个是 `lsim`（任意输入的响应）：`step`、`impulse` 只回答两种标准输入，实际工作里更常见的是斜坡、正弦、扰动和实测数据。线协议标识与核心 schema 不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 10.15 LS-1 – LS-6 | 新算子 `control.lti.lsim` 0.4.0 | 见上 |
| 2 | LS-3 | 采样之间的输入解释有固定缺省（foh），实现不得按数据自动选择；离散系统写出即 F-8 冲突 | MATLAB 不给 method 时自动选择，同一个请求会因数据不同而换问题 |
| 3 | LS-4 | `x0` 只用于 ss；tf/zpk 报 `OPTION_CONFLICT` 并指向 `convert` | tf/zpk 没有状态坐标，`x0` 的含义取决于实现选哪个状态空间实现 |
| 4 | LS-5 | 后置条件 `half_step_consistent`（dt/2 重新离散化），覆盖记 `test_points` | 原先 `step` 只有 `all_finite`；这是对离散化矩阵的独立检查 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `write_manifests.py` | `lsim` 的 manifest、输入输出 schema、L-1 字段描述、receipt（假设、后置条件、覆盖、guards） |
| `rust/aeoi-control/src/lsim.rs`（新）、`lib.rs` | 实现与注册；6 个单元测试（斜坡 foh 精确到 1e-13、常值 zoh 等于阶跃、x0 规则、离散规则、转置的 u） |
| `rust/aeoi-{bridge,mcp,web}` | 三处写死的“19 个工具”改为跟注册表数量一致 |
| `gen_golden_lsim.py`（新）、`golden/python-control/control.lti.lsim/0.4.0` | 33 个用例；foh 用例与 python-control 交叉核对 ≤ 1e-11；同时生成示例请求与参考值 |
| `rust/aeoi-control/tests/parity_lsim.rs`（新）、`examples.rs` 的 `lsim_example` | parity 与示例测试 |
| `examples/control.lti.lsim/` | manifest、schema、三个示例请求与响应、`parity.yaml`（含 `conformance_compare`） |
| `conformance/rules_control.py`、`tests/proxy_mutant.py`、`test_runner_catches_mutants.py` | `lsim` 的一致性规则；变异 `lsim_y_wrong`、`lsim_interp_mislabeled` |

**结果**：`cargo test` 158 / 0；parity 33 / 33（y 9.2e-14）；参考实现一致性 568 通过 / 0 失败 / 284 跳过（`lsim` 33 + 方法选择 1 + 默认值 5 通过、2 跳过）；变异测试 19 / 19；有数值规则的算子 7 / 20 = 35%。

## v0.4.4-draft（2026-11-24）

起因：清理开放问题 20、21（一致性套件 v0.1 发现）。线协议标识与核心 schema 不变。

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 10.10、12.1 第 20 项 | `lyap` 的 Q 可以不对称，MUST 按原样求解、MUST NOT 对称化 Q；Q 对称时 MAY 对称化 X，不发 warning；不声明 `W.VALIDATION.SYMMETRIZED` | input schema 说“对称”、算法说明说“非对称也可解”，manifest 声明了一个从不发出的码；第二实现照字面理解后在 4 个用例上失败 |
| 2 | 9.4 CF-2、12.1 第 21 项 | 默认值用例由运行器从 golden 派生，省略与写出默认值的结果必须逐位相同；F-8 不适用的字段跳过 | 19 个算子里有 15 个带默认值的字段，golden 几乎从不写出默认值，`lyap` 一个省略 `type` 的用例都没有 |
| 3 | 12.1 第 24 项 | 新开放问题：省略与写出默认值的请求指纹不同 | 写默认值用例时发现 |

**已做的改动**：`write_manifests.py`（`lyap` 的 Q 描述、前置条件，manifest 去掉 `W.VALIDATION.SYMMETRIZED`；重新生成 `examples/control.math.lyap/{manifest,input.schema}.json`）；`conformance/aeoi_conformance.py`（默认值变体，`--examples` 参数，报告里按 `defaults` 分组；运行器 0.3.0）；`conformance/tests/mutant_server.py` 与 `test_runner_catches_mutants.py`（变异 `wrong_default`）；`conformance/README.md`。`lyap` 算子版本不升：接受的输入、输出和发出的码都不变。

**结果**：参考实现 529 通过 / 0 失败 / 282 跳过（新增 73 个默认值用例通过、4 个跳过）；Python 第二实现 47 / 47；变异测试 17 / 17；`cargo test` 150 / 0。

## v0.4.3-draft（2026-11-24）

起因：清理 v0.4.2 写规则时发现的开放问题 22、23。线协议标识与 schema 不变。

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 12.1 第 22 项、9.4 CF-6 | `bode` 相位按 10.x 的分支比较，不取模；只在参考值距分支边界 1e-6° 以内时取模 | 分支约定是规范性的；模 360 比较会放过相位整体差 360° 的实现 |
| 2 | 12.1 第 23 项、9.4 CF-6、CF-7 | 高精度参考与其他引擎用同一 κ 缩放容差；“1e-12 不放宽”是参考实现自己的目标；超差走 `known_deviations`，CF-7 加例子并写明只有数值偏差能登记 | 实测标准 SciPy 在病态用例上达不到 1e-12，这是后向稳定算法的正常表现 |

**已做的改动**：`conformance/rules_control.py`（`bode` 相位不取模；`lqr` 的 mpmath 分支改为 `cond_S` 缩放）；`conformance/tests/proxy_mutant.py` 与 `test_runner_catches_mutants.py` 新增变异 `bode_phase_360`（相位整体 +360°，必须失败）；`examples/{bode,lqr,lyap}/parity.yaml` 的 `conformance_compare`；`conformance/README.md`。

**结果**：参考实现仍为 456 通过 / 0 失败 / 278 跳过；Python 第二实现 44 / 44；变异测试 16 / 16。

## v0.4.2-draft（2026-11-24）

起因：作者定下目标，一致性数值规则覆盖的算子数不少于全部算子的 30%，以后再提高。v0.4.1 时只有 `lyap`（19 个中的 1 个）。这一版补 5 个，挑选标准是比较方式各不相同：复数集合（`poles_zeros`）、频率响应（`bode`）、时间响应与网格指标（`step`）、唯一解加独立残差复核（`lqr`）、实现不唯一的系统（`c2d`）。线协议标识与 schema 不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 9.4 CF-2 | 列主序检查取全部数值检查里最大的“偏差 / 容差” | `poles_zeros`：eig(Aᵀ) = eig(A)，只看极点查不出错误 |
| 2 | 9.4 CF-6 | `parity.yaml` 的字段写法优先于默认的 `rel_fro`；运行器的公式写进 `conformance_compare`；容差类取自套件而不是被测实现的 manifest；κ 可由参考请求或参考值算，列出不得采信的自报量；状态空间实现比较哪些不变量，tf 系数先归一 | 写规则时这几处都要自己做决定，规范原来没写 |
| 3 | 9.4 CF-8 | 记录 `lqr` 的独立复核与残差归一方式 | 第一版按 R⁻¹ 计算残差，cond(R) = 1e12 时运行器自己误判 |
| 4 | 12.1 第 22、23 项；12.2 第 7 项 | 新开放问题：`bode` 相位分支不被检验；`lyap` 的 mpmath 容差与 `parity.yaml` 不一致 | 写规则时发现 |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `conformance/rules_control.py`（新） | 5 个算子的规则。`bode` 近极点用例的 κ(ω) = 1 + |s| / min |s − p|（p 为参考请求的极点），取代参考实现 parity 测试里用的自报误差估计；`lqr` 的 S 容差按参考解的 cond_S 缩放，mpmath 用例 1e-12 不放宽；`step` 的 dt 取参考网格；`c2d` 的 κ 由请求算（tustin：cond(I − αA)；zoh/foh：e^{‖A‖₁ts}），ss 结果比较单位圆上 7 个点的传递函数值、极点与 D |
| `conformance/aeoi_conformance.py` | 接入新规则；列主序检查改为最大比值；`lyap` 规则的容差类改为取自套件（值仍是 T2，结果不变）；运行器版本 0.2.0 |
| `conformance/tests/proxy_mutant.py`（新）、`test_runner_catches_mutants.py` | 包住参考实现的代理做 8 种改写：5 种错误答案必须失败、3 种等价答案（极点换序、相似变换、tf 分子分母同乘 3）必须通过 |
| `examples/{poles_zeros,bode,step,lqr,c2d}/parity.yaml` | 新增 `conformance_compare` |
| `conformance/README.md` | 状态、结果、限制与新发现的规范问题 |

**结果**：参考实现 456 通过 / 0 失败 / 278 跳过（v0.4.1 时 100 / 0 / 634）；有规则的 6 个算子全部通过，各检查的最大“偏差 / 容差”≤ 0.15。变异测试 15 / 15。

**仍未做**：新规则还没有被独立实现检验过；其余 13 个算子没有规则。

## v0.4.1-draft（2026-11-24）

起因：一致性套件 v0.1 发现输入、输出 schema 用 `aeoi://schemas/common/0.4/Matrix` 这种简写引用核心 schema（开放问题 19）。标准 JSON Schema 解析不了它，参考实现、`validate_examples.py` 和一致性运行器各自写了一段兼容代码才能用。作者决定先定这个问题，再扩充一致性规则。线协议标识保持 `aeoi/0.4`，核心 schema 的 `$id` 不变。

**规范改动**

| # | 条款 | 改动 | 原因 |
|---|---|---|---|
| 1 | 3.9 J-1 – J-3 | `$ref` MUST 是标准 2020-12 引用（`<$id>#/$defs/<Name>`），简写不合法；引用的 `$id` 版本与 manifest `spec` 一致；MCP 返回的 manifest 与工具 schema 内联 | 第三方实现和通用校验器不应需要专用解析器 |
| 2 | 12.1 第 19 项 | 标为已决定 | |

**已做的改动**

| 位置 | 改动 |
|---|---|
| `write_manifests.py` | 引用前缀改为 `aeoi://schemas/common/0.4#/$defs/`；`E.VALIDATION.METHOD_UNAVAILABLE` 改在写出前统一补到每个 manifest（v0.4 时 `bode`、`step`、`lqr` 三个 v0.2 算子是手改 manifest 文件加的，脚本重新生成会丢掉） |
| `examples/*/input.schema.json`、`output.schema.json` | 由 `write_manifests.py` 重新生成，只有引用写法改变（19 个算子，37 个文件） |
| `schemas/common.schema.json`、`envelope.schema.json` | `MethodPolicy` 的两处引用（文件内改为 `#/$defs/MethodPolicy`） |
| `aeoi-core/src/schema.rs` | 去掉 `resolve` 的简写分支；新单元测试 `shorthand_ref_rejected` |
| `validate_examples.py` | 去掉为简写注册的别名资源；新增检查：所有 schema 文件里的非本地 `$ref` 按原样解析 |
| `conformance/aeoi_conformance.py` | 去掉 `fix_refs`（自动改写）；manifest 内的 schema 有无法解析的 `$ref` 时，该算子的用例判失败并注明 J-1；运行器版本 0.1.1 |

指纹、`input_hash`、`output_hash` 都不含 schema 文本，示例响应不变。另：v0.4 一节所说“示例都没有 `libm`、逐位比较暂时跳过”已由 `3dada65` 解决（在 glibc 2.39 上重新生成，只改了元数据）。

## v0.4-draft（2026-11-23）

起因：准备写第二个实现（先 Python，再 C 的数值内核）时发现两处规范是空的：一个算子版本只能有一个方法 id，没有办法登记另一种算法，用户也没有办法要求“只用经过审计的库”；换一个实现是否等价于参考实现，没有判定规则，`parity.yaml` 只说明参考实现怎么测自己。这一版先改规范文本，随后补上 schema 与参考实现（见下“已做的改动”）；另加了一致性套件 v0.1（`conformance/`，只做了 `lyap` 的数值比较规则；在参考实现上的运行结果：734 个用例中 100 通过、0 失败，其余 634 个因没有比较规则而跳过）。线协议标识升为 `aeoi/0.4`（已决定，12.1 第 15 项）：`spec` 与核心 schema 的 `$id` 一起升，改动见下表第 7 行。

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

**已改（只涉及标识）**：`schemas/*`（`$id`、`$ref`、`spec` 的 const）、`examples/**`（manifest 的 `spec`、示例响应、输入输出 schema 里对核心 schema 的引用）、`write_manifests.py`、`validate_examples.py`、`rust/`（`aeoi_core::SPEC`、`schema.rs` 两处单元测试字面量、`aeoi-mcp` 的 e2e 断言与说明文字）、`conformance/`。没有改：`golden/`、`reports/`、`gen_golden_*.py`（它们只记录生成当时的版本）。`validate_examples.py`（459 个 golden 请求与全部示例校验通过）、一致性套件（44 个用例、7 个变异测试）通过；Rust 在 WSL（x86_64-linux）上，标识改动前后 `cargo test` 的结果相同，只有上面两个与平台有关的示例测试失败（见下）。

**已做的改动（参考实现与数据）**

| 位置 | 改动 |
|---|---|
| `schemas/common.schema.json` | `ExecOptions.method_policy`；新定义 `MethodPolicy`（`require`、`constraints.min_exactness`、`constraints.backend_library_in`，封闭对象，没有 `prefer`） |
| `schemas/manifest.schema.json` | `receipt.alternatives[]`（与 `receipt` 同形，另必填 `backend`、`algorithm`，可选 `determinism`、`tolerance_class`） |
| `schemas/envelope.schema.json` | `Receipt.method_selection`（`policy`、`used`） |
| `examples/*/manifest.json`、`write_manifests.py` | 19 个 manifest 的 `errors` 声明 `E.VALIDATION.METHOD_UNAVAILABLE`（CF-8：发出的码必须声明） |
| `aeoi-core` | 新模块 `method`：登记（`registered`）、解析（`resolve`，MS-6）、策略合并（`merge_policies`，MS-10）、重放策略（`replay_policy`，MS-9）；`operator.rs` 在语义校验前解析方法，收据按所用方法生成并在请求带策略时加 `method_selection`；`verify` 以 `require: [used]` 重放，缺方法时 verdict 为 `method_unavailable`；`ExecCtx.method` 把所选方法 id 交给算子 |
| `aeoi-mcp` | `aeoi_verify` 的 verdict 枚举加 `method_unavailable` |
| `aeoi-bridge` | `Bridge::with_method_policy`（宿主策略与请求策略合并后写入 `exec.method_policy`，请求只能收紧）；算子登记多于一个方法时，工具描述列出允许的方法（MS-11）；只登记一个方法的算子描述不变 |
| 测试 | `aeoi-core/src/method.rs` 单元测试（合成多方法 manifest：登记顺序与继承、`require` / `constraints`、无解报错与理由、策略合并）；`aeoi-control/tests/method_policy.rs`（对 `pade`：无策略时收据与 v0.3 相同、策略不改请求指纹与结果、无解策略报 `METHOD_UNAVAILABLE`、畸形策略报 `SCHEMA`、重放与 `method_unavailable`）；`aeoi-control` 与 `aeoi-mcp` 的示例测试只在示例所在平台（`reproducibility.platform` 与 `cpu_features` 相同）比较 `output_hash`，其余平台只跑数值检查（见下） |

**`platform_bitwise` 的示例测试与"平台"的定义**：在作者的 WSL（x86_64-linux，CPU 特性与生成示例的环境相同，glibc 2.31）上，`pade` 示例、`ctrb_obsv` 非对称示例与 MCP e2e 的 `output_hash` 与发布值不同；标识改动前后失败相同，所以与标识改动无关。逐字段比较：`pade` 只有 `approximation.certificate.value` 差 1 ulp（复数 `exp` 与模长，走 libm），`ctrb_obsv` 只有可观性矩阵的两个奇异值差 1–2 ulp（nalgebra 的 SVD 用到 libm）。先前按"CPU 特性不同"修过一次测试，判断错了：两边 CPU 特性相同。现在的改动：2.1 的"平台"加上 `libm`，`reproducibility` 与收据 `replay` 记录 `libm`（linux-gnu 上运行时读取 glibc 版本），`verify` 的同平台判定包含 `libm`；`build` 改为记录实际编译器版本（`aeoi-core/build.rs`），不再在缺环境变量时写死 `rustc-1.85`；示例测试只在 `platform`、`cpu_features`、`libm` 都相同时比较 `output_hash`，示例没有记录 `libm` 时只做数值检查。**目前所有示例都没有 `libm`，所以示例的逐位比较暂时全部跳过，需要在能编译的环境里重新生成示例后恢复**；同机重放（`deterministic_repeat_calls_bitwise_equal`）不受影响。哪个 libm 函数造成了这几位差异没有逐一确认。

**仍未做**：算子端没有任何一个真正登记第二个方法（`alternatives` 的路径只有合成 manifest 的单元测试）；`conformance/` 里只有 `lyap` 有数值比较规则。

## v0.3.6-draft（2026-11-23）

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

## v0.3.5-draft（2026-11-23）

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

## v0.3.4-draft（2026-11-23）

起因：问答评测只检查"会不会调用对的算子"。设计评测让模型迭代出一个控制器再提交，用独立评分器打分。线协议标识保持 `aeoi/0.3`，不新增算子，已发布示例的 `output_hash` 不变。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 8.4 增加设计评测：`aeoi-web design`、`submit_design`、EngDesign（MIT，原文入库）与 ControlEval（无许可证，运行时下载，不入库） | 问答对了不等于能设计 |
| 2 | 循环有单次请求时限、单题时限、429/5xx 重试预算；每日配额立即停止；报告在每题开始和每个模型回合后落盘。离线路径 `mock:design` 与 `mock:script` | 一次 ControlEval 试跑在空报告上停了约一小时：单次 HTTP 超时 180 秒再重试 6 次，报告却要等整道题结束才写 |
| 3 | XG_02 按细则给部分分（带宽 40、增益裕度 30、相位裕度 30）；非有限裕度写成 `"Inf"` / `"NaN"` 而不是 JSON null | 试点里一个稳定但带宽不对的控制器被打成 0 分；无穷相位裕度在报告里变成 null，看起来像缺字段，比较时却已经算通过 |
| 4 | 11 章、12.2 与 `reports/design-pilot.md`：7 道 EngDesign 试点的失败分类，以及按影响排序的缺失能力（串联/反馈、pid、闭环 stepinfo、Padé、带宽、控制器 c2d、MIMO 裕度）。这些算子不在本版实现 | 0/7 通过。多数是模型没交或交错，不是评分器把对的判错；缺的是让模型少做多项式手算的算子 |

## v0.3.3-draft（2026-11-23）

起因：让每个成功结果自带"凭什么可信"的机器可读证据，并能被任何人离线重放复核（想法借鉴 bicmath（MIT）的可验证结果与 matlab-mcp-extensions 的"没有证据不声称成功"，未复制代码）；同时补齐许可证。线协议标识保持 `aeoi/0.3`：`receipt` 是新增的可选信封字段（旧消费者忽略即可），`W.EVIDENCE.POSTCONDITION_FAILED` 是新增 warning；12 个算子的计算与 `result` 不变（所有示例 `output_hash` 不变），manifest 只增加描述性的 `receipt` 块，因此算子版本不变（同 v0.3.1 增加 description 的先例）。

| # | 改动 | 原因 |
|---|---|---|
| 1 | 新增 4.3 结果收据（R-1 – R-8）、信封 `receipt`（`$defs/Receipt`）、manifest `receipt` 块；12 个算子都声明了方法 id、前提（含由谁保证）、精确性类与后置条件 | `trust` 只给数值诊断，没有说明前提由谁保证、结果属于哪个请求，也没法复核 |
| 2 | 两层指纹：request（跨平台、跨编码稳定）与 result（逐位相同才相同）；平台与 build 只写在 `replay` 中 | 原生与 wasm 只有约 3/4 的结果逐位相同；把平台放进指纹会使跨平台复核恒失败 |
| 3 | `verify` 重放比较（bitwise / 容差类 / 篡改 / 请求不符 / 错误码）；MCP `aeoi_verify`（C-6）、`aeoi-call verify`、wasm `verify`、网页"重放验证"按钮；CI 在 wasm 中重放全部原生示例信封 | 复核不依赖信任算子提供方 |
| 4 | E-11 没有证据不声称成功；E-7a 框架级码；系统提示词第 6 条与收据短形式（R-7） | 防止 Agent 把"算完了"说成"算对了"，以及把 `caller` 前提当作已检查 |
| 5 | 许可：代码 MIT OR Apache-2.0，规范与文档 CC-BY-4.0；`THIRD_PARTY_NOTICES.md` | 之前仓库没有许可文件 |

## v0.3.2-draft（2026-11-23）

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

## v0.3.1-draft（2026-11-22）

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

## v0.3-draft（2026-11-21）

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

## v0.2-draft（2026-11-21）

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

## v0.1（2026-11-21）

初稿：manifest、类型、信封、错误、可复现性、验证凭证、MCP 映射、治理与三个示例算子。原文见 `spec-v0.1.md`。

## 附录：已解决的开放问题（原规范 12.1，编号保留）

15. ~~**（v0.4 新）线协议标识**~~ **已决定：升为 `aeoi/0.4`，核心 schema 的 `$id` 升为 `/0.4`。** 作者先决定保持 `aeoi/0.3`（理由是新增字段全部可选，与 v0.3.1、v0.3.5、v0.3.6 的做法一致），随后改为随版本升级。做法与 v0.3 相同（第 13 章 v0.3-draft 第 1 行）：`spec` 与 `$id` 一起升；`golden/` 不改（文件名是内容哈希，V-3；其中的 `spec` 记录生成时的版本，没有测试或脚本比较它）；`reports/` 是历史运行记录，不改。没有兼容层：v0.4 的实现只发 `aeoi/0.4`，是否同时接受 `aeoi/0.3` 的 manifest/信封留给消费者。`input_hash`、`fingerprint` 不含 `spec`，不受影响。
19. ~~**（v0.4 新，一致性套件发现）`$ref` 约定**~~ **已决定（v0.4.1）：改写所有 schema 为标准引用，见 3.9 J-1 – J-3。** 没有选“在 3.9 写明简写约定”：那会要求每个实现和每个通用校验器都先写专用解析器。原问题：manifest 的输入、输出 schema 用 `aeoi://schemas/common/0.4/Matrix` 引用通用类型，标准 JSON Schema 解析不了（应为 `aeoi://schemas/common/0.4#/$defs/Matrix`），3.9 也没有说明这种写法。要么改写所有 schema，要么在 3.9 写明约定。
20. ~~**（v0.4 新，一致性套件发现）`lyap` 的 Q 对称性**~~ **已决定（v0.4.4）：Q 可以不对称，按原样求解，不得对称化 Q；Q 对称时 MAY 对称化 X，不发 warning；`lyap` 不声明 `W.VALIDATION.SYMMETRIZED`（10.10）。** 参考实现的行为本来就是这样，改的是 input schema 的描述（原写“对称”）与 manifest 的声明（声明了从不发出的码）；接受的输入、输出和发出的码都不变，所以算子版本不升。原问题：：input schema 把 Q 描述为“对称”，10.10 的算法说明写“Q 非对称也可解”，二者要统一；`W.VALIDATION.SYMMETRIZED` 已在 manifest 声明，但规范没有规定触发条件（Q 非对称？结果被对称化？）。第二实现第一版因把非对称 Q 对称化，在 golden 的 4 个用例上失败。
21. ~~**（v0.4 新，一致性套件发现）默认值没有用例**~~ **已决定（v0.4.4）：运行器派生默认值用例（9.4 CF-2）。** 参考实现 77 个变体中 73 个通过、4 个按 F-8 跳过（PID 的模式专属字段）；`lyap` 有了 3 个省略 `type` 的用例；变异“缺省 `type` 当作 discrete”在这 3 个用例上全部失败。原问题：：`lyap` 的 `type` 默认 `continuous`，golden 里没有省略它的用例，F-7 的“默认值回显”也无人检验。套件需要“省略可选字段”的用例族。
22. ~~**（v0.4.2 新，一致性规则发现）`bode` 的相位分支不被检验**~~ **已决定（v0.4.3）：检验。** 10.x 的分支约定是规范性的，套件按它比较（不取模）；参考值距分支边界 1e-6° 以内时才取模，因为那里舍入就可能让正确的实现落到另一边（unwrap 时整条通道，否则单点）。参考实现不取模的最大误差为容差的 3e-12 倍，现有 golden 首点距边界最近 0.027°。原问题：：`parity.yaml` 按“模 360”比较相位，`branch_center_deg` 与 unwrap 规则规定的分支（v0.2，12.1 第 4 项）因此不进入一致性判定，相位整体差 360° 的实现能通过。要么在套件里加分支检查（同一 `branch_center_deg` 下逐点差 < 180°），要么写明分支只是参考实现约定。
23. ~~**（v0.4.2 新，一致性规则发现）`lyap` 的 mpmath 容差不一致**~~ **已决定（v0.4.3）：一致性对所有参考引擎用同一个 κ 缩放容差。** 高精度参考（mpmath）只是更准的参考值，不是更严的要求。`parity.yaml` 里的“1e-12 不放宽”是参考实现自己的精度目标，由它的 `cargo test` 检验：参考实现带精化，实测 `lyap` ≤ 7e-15；而没有精化的后向稳定算法达不到，SciPy 在 `lyap`（κ ≈ 1.6e8）上为 7.5e-9，在 `lqr`（cond(R) = 1e11）上为 3.7e-9。把它当门槛会让标准实现无法通过。`lqr` 的 mpmath 分支同样改为 `cond_S` 缩放（v0.4.2 时误设为 1e-12）。试过用 CARE 的一阶条件数（闭环 Lyapunov 算子）缩放，最病态的用例容差 ≥ 1，等于不检查，所以没有采用；剩下的个别超差（上面 SciPy 的 `lqr` 用例超出 6.5 倍）按 CF-7 登记 `known_deviations`。原问题：：`parity.yaml` 对 mpmath 用例写“rel_fro ≤ 1e-12，不放宽”，运行器的 `lyap` 规则对所有引擎都用 T2 × max(1, κ/100)。`lqr` 的新规则对 mpmath 用 1e-12。改 `lyap` 规则会影响 Python 第二实现在病态用例上的结果，需要先重跑。
