# AEOI 跨实现一致性套件（v0.3）

规范 9.4（CF-1 – CF-12）的实现。标题里的 v0.3 是套件格式的版本，规范本身已到 v0.4.17-draft。

**状态（2026-11-25，规范 v0.4.19-draft）**

- 24 / 37 个算子（65%）有数值规则：
  - 控制：`lyap`、`poles_zeros`、`bode`、`step`、`lqr`、`c2d`、`lsim`、`pidtune`
  - 信号：`fft`、`butter`、`filter`
  - 线性代数：`linalg.system.solve`、`linalg.decomp.eig`、`linalg.decomp.svd`
  - 插值与拟合：`interp1`、`polyfit`
  - 微积分与 ODE：`ode.ivp.solve`、`integral`、`trapz`
  - 非线性方程与优化：`fzero`、`fsolve`、`linprog`、`fmincon`、`fminsearch`
- 参考实现（Rust）：1111 通过 / 0 失败 / 291 跳过。
- 变异测试：第二实现 8 种缺陷、参考实现 45 种改写，全部被抓住。
- Python 第二实现做 5 个算子：`lyap`，以及 v0.4.19 只读规范写成的 `fft`、`linalg.system.solve`、`interp1`、`integral`。233 通过 / 0 失败 / 0 跳过。写它的过程找出 32 处规范问题，见 `impls/python_scipy/SPEC_FINDINGS.md`。

## 内容

| 路径 | 作用 |
|---|---|
| `aeoi_conformance.py` | 运行器。只通过 MCP stdio 驱动被测实现（CF-3），用例直接读 `golden/`（CF-2） |
| `aeoi_hash.py` | 规范 3.7 / 4.3 R-3 / P-2 的独立重写（RFC 8785 JCS、数组摘要、请求指纹）。运行器用它重算指纹，不采信被测实现自报的值 |
| `rules_numeric.py` | 控制以外的 16 个算子：`fft`（规范 v0.4.7，含运行器从请求的 x 与返回的 X 独立重算 Parseval）；`ode.ivp.solve`（规范 v0.4.8：对照精确解而不是参考求解器的步长序列；运行器用精确解检查 `tolerance_consistent = true` 的声明是否属实）；`linalg.system.solve`（规范 v0.4.10：运行器用有理数精确重算后向误差，并检查 `backward_stable = true` 的声明是否属实）；`interp.grid.interp1`（规范 v0.4.10：节点处必须精确、范围外点数由运行器重数）；`fit.poly.polyfit`、`linalg.decomp.eig`、`linalg.decomp.svd`（规范 v0.4.11：特征向量、奇异向量不唯一，只比较不变量——特征值最近匹配、运行器从请求重算残差、正交性与规一化规则）；`butter`、`filter`（v0.4.12：零极点按集合对照精确构造，运行器从返回的 sos / zpk 重算截止点增益与 b/a 偏差，并从请求重算差分方程残差）；`fzero`、`fsolve`（v0.4.13：运行器用自己的表达式求值器重算残差与变号）；`linprog`、`fmincon`、`fminsearch`（v0.4.14：对照独立核对过的最优点，运行器从请求重算可行性与乘子的 KKT 条件）；`integral`、`trapz`（v0.4.15：对照 50 位真值并核查误差估计是否覆盖真实误差，`trapz` 对照精确有理数求和） |
| `rules_control.py` | `poles_zeros`、`bode`、`step`、`lqr`、`c2d` 的数值规则（v0.2），`lsim`（规范 v0.4.5）、`pidtune`（规范 v0.4.6，含运行器用返回的控制器重算 L(jωc)）；`lyap` 的规则在运行器里 |
| `impls/python_scipy/` | 第二实现：Python + numpy/scipy，线级（MCP stdio）。`server.py` 提供 5 个工具；共享层 `aeoi_common.py`（信封、收据、指纹、method_policy、数组编码）、表达式语言 `aeoi_expr.py`、每个算子一个 `op_*.py`；`SPEC_FINDINGS.md` 是只读规范时发现的问题及处理结果 |
| `tests/test_runner_catches_mutants.py` | 证明规则既不太松也不太严：对第二实现注入 8 种缺陷，再经 `proxy_mutant.py` 对参考实现做 45 种改写（v0.4.8 加了 `ode_y_wrong`、`ode_states_reordered`、`ode_overclaims`；v0.4.10 加了 `linsolve_x_wrong`、`linsolve_overclaims`、`interp1_one_ulp`、`interp1_count_wrong`；v0.4.11 加了 `polyfit_coef_wrong`、`polyfit_mu_wrong`、`eig_vectors_swapped`、`eig_pair_order`、`eig_value_wrong`、`svd_u_sign_only`、`svd_both_signs`；v0.4.12 加了 `butter_pole_wrong`、`butter_warning_dropped`、`filter_y_wrong`、`filter_state_dropped`；v0.4.13 加了 `fzero_x_wrong`、`fzero_warning_dropped`、`fsolve_x_wrong`、`fsolve_residual_zeroed`；v0.4.14 加了 `linprog_lambda_sign`、`fmincon_x_wrong`、`fmincon_lambda_dropped`、`fminsearch_x_shifted`；v0.4.15 加了 `integral_value_wrong`、`integral_error_understated`、`trapz_value_ulps`），错误答案（含相位整体加 360°）必须失败，等价答案（换极点顺序、相似变换、tf 分子分母同乘常数）必须通过 |
| `tests/proxy_mutant.py` | 包住参考实现的 MCP 代理，按 `$AEOI_MUTANT` 改写一个算子的结果并重算 `output_hash` |

## 运行

依赖：Python ≥ 3.10、`numpy`、`scipy`、`jsonschema`（≥ 4.18，含 `referencing`）。在仓库根目录执行：

```
python3 conformance/aeoi_conformance.py run --impl "python3 conformance/impls/python_scipy/server.py" \
    --operators control.math.lyap --report report.json
python3 conformance/aeoi_conformance.py run --impl rust/target/release/aeoi-mcp --report ref.json   # 参考实现，先 cargo build --release -p aeoi-mcp
python3 conformance/tests/test_runner_catches_mutants.py
```

`--impl` 是启动被测实现的命令（它必须在 stdin/stdout 上说 MCP，参考实现是 `rust/target/release/aeoi-mcp`）。退出码 0 = 没有失败用例。报告格式见 CF-11。

## 运行器检查什么

每个用例：isError 与 `status` 一致（C-1）；信封通过 envelope schema；结果通过 manifest 的 output schema；出现的 `E.`/`W.` 码都在 manifest 里声明；预期错误码精确匹配、预期警告出现；`fingerprint.request` 与 `input_hash` 等于运行器重算值（CF-4）；`output_hash` 是所返回结果的哈希；收据的方法 id 在 manifest 登记的集合里；收据 `claim` 等于运行器按 E-11/E-12 重新推出的值；声明了 `bitwise`/`platform_bitwise`/`seeded` 的算子，同一请求运行两次 `output_hash` 相同（P-7）。

数值比较：只有 `RULES` 里的算子有规则，每条规则的公式写在该算子 `parity.yaml` 的 `conformance_compare` 里（`lyap` 写在 `compare`）。**没有规则的算子，成功用例记为 `skip`**，它们的错误码与预期警告仍会检查。共同约定（CF-6）：

- 容差类取自套件（`parity.yaml`），不取被测实现 manifest 里自报的 `tolerance_class`；
- κ 只由运行器从参考请求或参考值算出；被测实现报告的 `trust.error_estimate`、`trust.condition`、`trust.derived` 一概不用。参考实现自己的 parity 测试（`cargo test`）对 `bode` 近极点用例与病态 `lqr` 用自报误差估计放宽，套件改用运行器算的 κ，实测参考实现照样全部通过；
- 无序量按集合匹配（V-5）；`c2d` 返回的状态空间实现只比较不变量（单位圆上若干点的传递函数值、极点、D），返回 tf 时分子分母先除以 den[0]；
- `lqr` 另由运行器用返回的 S、K 独立复核 CARE 残差、K 与 S 的一致性、S 对称、A − BK 稳定（CF-8 推荐项）；残差按各项大小归一，不对 R 求逆，所以 cond(R) 到 1e12 也不会因运行器自身的舍入误判；
- 第 0 章的列主序检查：错误展平的请求在所有数值检查里的最大“偏差/容差”> 1e3 才算查出。单看一个字段可能看不出：eig(Aᵀ) = eig(A)，极点不动，零点与 DC 增益会动。

默认值用例（v0.3，规范 9.4 CF-2）：运行器从成功的 golden 用例派生“省略默认值”与“写出默认值”的变体（默认值取自套件的 `examples/<op>/input.schema.json`，不取被测实现的 manifest；每个算子、字段、方向至多 3 个），结果必须与原请求逐位相同。写出的字段在该模式下不适用（F-8 的 `OPTION_CONFLICT` 点名该字段）时跳过。

另有一个方法选择用例（CF-10 里今天就能跑的部分）：请求 `exec.method_policy.require` 一个不可能存在的方法 id，实现 MUST NOT 成功（返回 `E.VALIDATION.SCHEMA` 或 `E.VALIDATION.METHOD_UNAVAILABLE` 都可以）。

## 结果

参考实现：`rust/target/release/aeoi-mcp`，规范 v0.4.17-draft，在云容器 x86_64-linux（glibc 2.39）上运行。结果是 1111 通过 / 0 失败 / 291 跳过，共 1402 个用例：

- 1132 个来自 `golden/python-control`（参考值出自 python-control / numpy / scipy）；
- 54 个来自 `golden/mpmath`（高精度真值）；
- 216 个由运行器派生，其中 179 个默认值用例、37 个方法选择用例（每个算子 1 个）。

参考实现在 5 个平台上逐位相同（发布流水线比较 `output_hash`），在 wasm 上也与原生版逐位相同，所以这个结果不随平台变化。

有数值规则的 24 个算子，golden 成功用例数：

| 算子 | 通过 | 算子 | 通过 |
|---|---|---|---|
| `control.lti.bode` | 103 | `interp.grid.interp1` | 56 |
| `control.lti.step` | 89 | `signal.spectral.fft` | 44（mpmath 6） |
| `control.design.lqr` | 88（mpmath 16） | `ode.ivp.solve` | 34 |
| `control.lti.poles_zeros` | 47 | `signal.design.butter` | 33 |
| `control.lti.c2d` | 47 | `linalg.system.solve` | 30 |
| `control.math.lyap` | 43（mpmath 12） | `linalg.decomp.eig` | 28 |
| `control.lti.lsim` | 33 | `fit.poly.polyfit` | 24 |
| `control.design.pidtune` | 33 | `signal.filter.filter` | 24 |
| `nonlinear.root.fzero` | 24 | `linalg.decomp.svd` | 20 |
| `calculus.quadrature.integral` | 19 | `nonlinear.system.fsolve` | 18 |
| `optim.linear.linprog` | 16 | `optim.local.fmincon` | 13 |
| `calculus.quadrature.trapz` | 12 | `optim.local.fminsearch` | 10 |

跳过的 291 个用例，是其余 13 个算子的 golden 成功用例，以及少数只对某种模式适用的默认值用例（F-8，例如 PID 的 `Ti`/`Td`、离散系统上的 `interpolation`）。没有数值规则的算子，错误码、警告、指纹、收据照样检查。

第二实现（Python，`impls/python_scipy/`）：5 个算子 233 通过 / 0 失败 / 0 跳过（lyap 47、fft 59、linsolve 34、interp1 64、integral 29，各含默认值与方法选择用例）。

- `fft`、`linalg.system.solve`、`interp1`、`integral` 是 v0.4.19 由一个只读规范的子 agent 写的：它不能看 Rust 代码、golden 生成脚本和本套件的规则文件，manifest 也是自己写的。FFT 是自写的混合基加 Bluestein，没有用 numpy 的 pocketfft；线性方程组是自写的 LU 与 Householder QR，残差用正确舍入内积；积分用自写的表达式解析器与 Gauss–Kronrod。按 CF-11 的机械判定，这四个算子的通过都算独立通过。
- `lyap` 是 v0.2 时的第一版：数值用 scipy 的 Bartels–Stewart，与 python-control 同族，所以它的 31 个 python-control 用例算同源，只能说明套件可移植；mpmath 的 12 个用例是独立的。它的 manifest 仍是从参考 manifest 改写的。

## 已知限制

- 24 个算子有数值规则；其余 13 个算子（`convert`、`ctrb_obsv`、`margin`、`impulse`、`place`、`dlqr`、`series`、`parallel`、`feedback`、`pid`、`stepinfo`、`pade`、`loopsens`）的成功用例是 `skip`。
- 有第二实现检验的规则只有 5 个算子（`lyap`、`fft`、`linalg.system.solve`、`interp1`、`integral`）；其余 19 个算子的规则只在参考实现上跑过。
- 哈希只支持 `json` 编码的稠密数组；`b64` / `ref` / 稀疏数组的请求不重算指纹。
- 没有 CF-10 的逐方法运行：参考实现已实现 `method_policy`（v0.4），但还没有哪个算子在 `receipt.alternatives` 里登记第二个方法。
- 第二实现里 `lyap` 的 manifest 复用参考实现的 manifest，只替换了方法、后端、构建与精度等级，不是在“只看规范”的条件下写出来的；其余 4 个是自己写的。
- E-11a（profile 点名的后置条件是最小集合）还没有机械检查。

## 用这个套件发现的规范问题

1. ~~已解决（规范 v0.4.1，3.9 J-1）：所有 schema 改为标准引用，运行器不再改写，遇到无法解析的引用判失败。~~ 原问题：输出（和输入）schema 里的引用写成 `aeoi://schemas/common/0.4/Matrix`，按标准 JSON Schema 解析不了（应为 `…/0.3#/$defs/Matrix`），3.9 也没有说明这种约定。运行器自己改写后才能校验。
2. ~~已解决（规范 v0.4.4，10.10）：Q 可以不对称，按原样求解，不得对称化 Q；`lyap` 不声明 `W.VALIDATION.SYMMETRIZED`。~~ 原问题：`lyap`：input schema 把 Q 描述为“对称”，算法说明却写“Q 非对称也可解”；`W.VALIDATION.SYMMETRIZED` 在 manifest 里声明了，但没有规定什么时候发。第二实现第一版就是因为把非对称 Q 对称化而在 4 个用例上失败。
3. ~~已解决（规范 v0.4.4，CF-2）：运行器派生默认值用例，变异 `wrong_default` 必须失败。~~ 原问题：`lyap` 的 `type` 有默认值 `continuous`，golden 里没有一个用例省略它，所以套件不检验默认值。第二实现第一版因此把它当必填也没被发现（读 input schema 时才发现）。
4. ~~已解决（规范 v0.4.3，12.1 第 22 项）：相位改为在 10.x 规定的分支上比较，不取模；只在参考值距分支边界 1e-6° 以内时取模。变异测试 `bode_phase_360` 必须失败。~~ 原问题：`bode` 的 `parity.yaml` 按“模 360”比较相位，分支约定不被检验。
5. ~~已解决（规范 v0.4.3，12.1 第 23 项）：1e-12 是参考实现自己的精度目标（它做精化，实测 ≤ 7e-15），不是一致性门槛；一致性对所有引擎用同一个 κ 缩放容差，`lqr` 的 mpmath 分支也改成这样。标准 SciPy 在 `lyap` κ ≈ 1.6e8 时误差 7.5e-9、在 `lqr` cond(R) = 1e11 时 3.7e-9，属于后向稳定算法的正常表现；超出容差的个别用例走 CF-7 的 `known_deviations`。~~ 原问题：`lyap` 的 mpmath 用例 `parity.yaml` 写 1e-12，规则用 T2 × max(1, κ/100)。
6. （v0.2 规则发现）CF-6 说每个顶层字段默认按 `rel_fro` 比较，而 `bode`（dB 与度的绝对差）、`step`（相对 max|y_ref| 的最大差）在 `parity.yaml` 里另有写法；规则按 `parity.yaml`。规范 v0.4.2 已写明以 `parity.yaml` 为准。
7. （v0.3 发现）省略与写出默认值的请求，`fingerprint.request` 不同（H-1 按原样哈希），套件只比较结果。见规范 12.1 第 24 项。
