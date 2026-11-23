# AEOI 控制算子 × tpt-control 交叉验证

> 参考实现：**tpt-control**（<https://github.com/tpt-solutions/tpt-control>，commit `10e432068e094bc37977bfcc5c96bac3f6ef3641`，2026-10-16；© TPT Solutions，许可 **MIT OR Apache-2.0**）。本仓库只在开发工具 `tools/xcheck-tpt/`（独立 cargo 包，不是 `rust/` workspace 成员，也不是任何 aeoi crate 的依赖）里按 git rev 引用 `tpt-ctrl-core`、`tpt-ctrl-classic`、`tpt-ctrl-optimal`，没有复制其源码。
> 第三方参考值来自 python-control / scipy（不是 MATLAB）。机器可读结果：`reports/parity-tpt-control.json`。

## v0.3.2 修复后（`lqr` 0.3.0、`lyap` 0.4.0，2026-11-23）

本报告初版（下文"中文摘要"及之后各节）是在 commit `3640023`（`lqr` 0.2.0、`lyap` 0.3.0）上测的。"改进想法"中的 CARE 部分与离散 Lyapunov 迭代精化已在 v0.3.2 实施（spec 10.3、10.10），用同一 harness、同一随机种子重跑（用例 956 个：语料增加了 2 个新示例请求）。`reports/parity-tpt-control.json` 是重跑后的结果，修复前的数字保留在 `before_fix` 字段。

前向误差 max|X−X*|/max|X*|（lqr 同时给 K），真值为 50 位精度 Newton 迭代（`analyze.py`）；括号内为 AEOI 报错的用例数（`lqr_marginal_undetectable`、`lqr_unstabilizable` 与语料中的 9 个是应当报错的输入）：

| 算子 | 族 | 用例 | AEOI 修复前 X / K（报错数） | AEOI 修复后 X / K（报错数） | tpt X / K | scipy X / K |
|---|---|---|---|---|---|---|
| lqr | corpus | 75 | 2.5e-06 / 1.5e-06（9） | 0 / 0（9） | 3.7e-02 / 3.7e-02 | 7.0e-10 / 1.1e-08 |
| lqr | random/lqr_bigA_1e3 | 10 | 2.0e-12 / 9.1e-13（0） | 0 / 0（0） | 1.6e-13 / 9.2e-14 | 4.0e-11 / 3.0e-11 |
| lqr | random/lqr_condR_1e12 | 10 | 3.0e-05 / 2.5e-05（7） | 3.3e-17 / 3.4e-17（0） | 7.4e-09 / 1.8e-04 | 6.1e-11 / 3.3e-05 |
| lqr | random/lqr_condR_1e8 | 10 | 1.8e-08 / 2.1e-08（0） | 0 / 0（0） | 1.7e-12 / 2.5e-09 | 2.0e-12 / 6.0e-09 |
| lqr | random/lqr_crossN | 6 | 8.2e-14 / 4.3e-14（0） | 0 / 0（0） | 3.2e-14 / 1.8e-14 | 1.0e-13 / 1.0e-13 |
| lqr | random/lqr_marginal_undetectable | 4 | — / —（4） | — / —（4） | — / — | 3.6e-16 / 3.6e-16 |
| lqr | random/lqr_smallQ_1e-8 | 10 | 4.8e-14 / 4.5e-14（0） | 0 / 0（0） | 1.9e-14 / 1.8e-14 | 1.4e-08 / 1.8e-08 |
| lqr | random/lqr_unstabilizable | 5 | — / —（5） | — / —（5） | — / — | — / — |
| lqr | random/lqr_weakB_1e-4 | 10 | 2.9e-10 / 1.4e-10（0） | 0 / 0（0） | 5.2e-13 / 2.5e-13 | 1.1e-03 / 1.0e-03 |
| lqr | random/lqr_well | 10 | 5.9e-12 / 2.2e-12（0） | 0 / 0（0） | 3.7e-13 / 1.3e-13 | 3.6e-13 / 3.4e-13 |
| lqr | repro | 1 | — / —（1） | 0 / 0（0） | 5.1e-23 / 1.2e-16 | 1.1e-17 / 4.4e-11 |
| lyap | corpus | 35 | 2.8e-14（3） | 2.8e-14（3） | 2.9e-12 | 2.8e-14 |
| lyap | random/lyap_c_near_axis_1e-3 | 12 | 1.7e-12（0） | 1.0e-13（0） | 2.0e-12 | 5.5e-13 |
| lyap | random/lyap_c_near_axis_1e-6 | 12 | 2.1e-09（0） | 0（0） | 1.7e-09 | 1.1e-09 |
| lyap | random/lyap_c_nonnormal_rot_1e1 | 8 | 4.9e-03（0） | 4.9e-03（0） | 2.1e-02 | 2.1e-02 |
| lyap | random/lyap_c_nonnormal_rot_1e2 | 8 | 1.0e+00（0） | 1.0e+00（0） | 1.0e+00 | 5.7e-01 |
| lyap | random/lyap_c_nonnormal_rot_1e3 | 8 | 9.0e+01（0） | 9.0e+01（0） | 7.6e+03 | 1.0e+00 |
| lyap | random/lyap_c_nonnormal_triangular | 12 | 5.0e-16（0） | 5.0e-16（0） | 1.0e+00 | 5.2e-16 |
| lyap | random/lyap_c_unstable | 6 | 2.5e-14（0） | 2.5e-14（0） | 8.6e-15 | 1.2e-14 |
| lyap | random/lyap_c_well | 12 | 6.3e-15（0） | 6.3e-15（0） | 6.4e-15 | 3.2e-15 |
| lyap | random/lyap_d_rho_0.999 | 12 | 4.3e-12（0） | 1.4e-13（0） | 6.0e-14 | 1.5e-13 |
| lyap | random/lyap_d_rho_1-1e-6 | 12 | 6.0e-09（0） | 4.3e-11（0） | 5.5e-11 | 2.6e-10 |
| lyap | random/lyap_d_unstable | 6 | 4.1e-14（0） | 4.1e-14（0） | — | 4.3e-15 |
| lyap | random/lyap_d_well | 12 | 2.4e-14（0） | 2.4e-14（0） | 2.0e-16 | 4.9e-15 |

- **lqr**：所有能求解的用例 S、K 都等于真值的 f64 舍入（≤ 3.4e-17）；cond(R) = 1e12 族的 7 个误报 `HAMILTONIAN_IMAG_AXIS_EIG` 与复现用例全部消失；应当报错的 9 个（不可镇定、不可检测的虚轴模态）仍以原错误码报错。
- **lyap**：离散 ρ = 1 − 1e-6 由 6.0e-9 降到 4.3e-11（tpt 5.5e-11），连续 1e-6 近虚轴由 2.1e-9 降到 0；未触发精化的用例逐位不变。`lyap_c_nonnormal_rot_*` 的大误差三家都有（问题本身的条件数达 1e12 以上），未改变。
- **dlqr** 算法未改，结果与修复前相同。
- 与 tpt 的直接差（"直接比较"表）中 lqr 的 K 最大差变为 1.8e-4：来自 `random/lqr_condR_1e12/4`，修复前 AEOI 在该例报错、不参与比较；修复后 AEOI 误差 3.4e-17，差值是 tpt 自身的 K 误差。

## 中文摘要（修复前，commit `3640023`）

- **覆盖**：12 个 AEOI 算子中 11 个在 tpt-control 里有对应或可组合的实现（`place` 没有，只用 tpt 的特征值求解器复核 AEOI 的 K）。用例 954 个：wasm/原生 parity 用的 651 个（617 个 python-control golden + 34 个示例请求），加 300 个带种子的随机良态/病态用例和 3 个最小复现用例。直接比较了 729 个用例，另有 32 个两边都报错。
- **良态问题两边基本一致**：c2d（zoh/tustin）≤ 1e-15，bode 幅值 ≤ 3e-14 dB、相位 ≤ 2e-13°，step/impulse ≤ 1.4e-11，ctrb/obsv 矩阵逐位相同，Gramian ≤ 7e-15，tf→ss 的 Markov 参数 ≤ 5e-15，极点 ≤ 1.3e-12，良态 lyap/lqr/dlqr 的中位差约 1e-14。margin：tpt 在网格上插值，相对 AEOI 的解析值增益裕度差 ≤ 2.8e-6、相位裕度差 ≤ 3.4e-5°，属于网格误差。place：tpt 求的 eig(A − BK) 与目标极点差 ≤ 1.6e-10。
- **谁更准（用 50 位精度真值判定）**：
  - **CARE（lqr）：cond(R) 大时 tpt 明显更准。** cond(R) = 1e8 时前向误差 AEOI 最大 1.8e-8，tpt 1.7e-12，scipy 2.0e-12。cond(R) = 1e12 时 AEOI 3.0e-5，tpt 7.4e-9，scipy 6.1e-11；而且 AEOI 有 7/10 个用例错误地报 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`，实际上存在镇定解（AEOI 的 bug，见下）。golden 中 wasm 与原生相差 4.7e-6 的那类病态 lqr 用例，AEOI 本身离真值约 2.5e-6，tpt 约 1e-11：那次的平台差异反映的是 AEOI 本身精度不足，不是平台问题。
  - **DARE（dlqr）：tpt 的结构保持倍增（SDA）在病态问题上并不更准，反而更差。** cond(R) = 1e12 时 tpt 4.3e-4，AEOI 2.2e-14，scipy 2.1e-15。‖A‖ 大（ρ = 30）时 tpt 2.3e-6，AEOI 2.3e-9。SDA 显式求 R⁻¹，构造 G = BR⁻¹Bᵀ，并反复求逆 (I + GH)；R 病态时这一步就丢精度。
  - **lyap**：连续情形三者相当，误差约为"Kronecker 算子条件数 × ε"。离散且 ρ(A) → 1 时 tpt 的平方级数更准：ρ = 1 − 1e-6、条件数 7e7 时，tpt 5.5e-11，scipy 2.6e-10，AEOI 6.0e-9。AEOI 的误差在 cond·ε 之内，属于后向稳定算法的正常表现，不是 bug。但 ρ ≥ 1 时 tpt 直接报 `NoConvergence`，AEOI 与 scipy 照常求解（带警告）。
- **发现的问题**：
  - **AEOI（bug）**：lqr 的 Hamiltonian 虚轴检查阈值 1e3·ε·‖H‖₁ 中，‖H‖₁ 被 ‖BR⁻¹Bᵀ‖ 主导。cond(R) 大时阈值被放大到 O(0.1)，于是良态问题也会被拒绝。最小复现：A = diag(−0.1, −1)，B = diag(0.1, 1)，Q = I，R = diag(1, 1e-12)。这是两个解耦的标量 CARE，解析解 X = diag(4.1421356…, 9.99999e-7)，tpt 与 scipy 都正确；AEOI 报 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`（min|Re λ|/‖H‖₁ = 1.41e-13）。
  - **AEOI（精度）**：Newton–Kleinman 修正只在"f64 残差下降"时才接受一步，最多 3 步，并且用 X·G·X 形式。cond(R) 大时 f64 残差已到舍入底，修正几乎不生效。AEOI 对这些用例都给了 `W.NUMERICAL.ILL_CONDITIONED`，但实际误差（1e-5 量级）远大于可达到的精度（1e-10 量级）。
  - **tpt（bug）**：`dare` 不检查闭环是否镇定。A 为旋转矩阵（|λ| = 1）、Q = 0 时，倍增迭代因舍入使 A^(2^k) → 0 而"收敛"，返回 X = 0、K = 0，闭环极点仍在单位圆上。正确结果应是报错（AEOI 返回 `E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG`）。
  - **tpt（文档）**：`Margins` 的 `gain_crossover_omega`、`phase_crossover_omega` 两个字段的文档注释写反了（代码本身是对的）。`solve_lyapunov` 的文档写"A 必须 Hurwitz，否则返回 SingularMatrix"，实测对不稳定 A 也给出正确解。`lqr` 不检查 Q 是否对称，非对称 Q 时静默返回解（AEOI 与 scipy 都拒绝）。
- **改进建议（本次未改 AEOI 算法）**：CARE 的 Newton–Kleinman 改用"K 形式"。每步用 Cholesky 求 K = R⁻¹(BᵀX + Nᵀ)，解闭环 Lyapunov 方程 (A−BK)ᵀX + X(A−BK) + Q − NK − KᵀNᵀ + KᵀRK = 0，按 ‖ΔX‖/‖X‖ 停止，而不是按 f64 残差。在 Python 里从 AEOI 的 S 出发验证：17 个病态用例的前向误差从最大 3.0e-5 降到 1.6e-10，与 tpt 相当（下表）。同时把虚轴检查改成与 G 的尺度无关的判据，例如先对 H 做平衡，或只在符号函数迭代失败、闭环不镇定时才报错。dlqr 不需要换成 SDA。

## 环境

- AEOI：`aeoi-control`，commit 见 JSON 的 `aeoi_commit`；Rust 1.85.1，release。
- tpt-control：`rust-toolchain.toml` 固定 1.97.1，workspace 的 `rust-version = 1.85`。在 1.97.1 上它自带的 core/classic/optimal 测试 111 个全部通过；harness 用 1.85.1 编译它，可以正常编译。
- 真值：mpmath 50 位。Lyapunov 用 Kronecker 直接解；Riccati 先做 N 约化，再从最好的 f64 候选出发做 Newton–Kleinman（连续）或 Hewer（离散）迭代至 1e-40（n ≤ 10）。每个候选的残差都在 50 位精度下计算。scipy：corpus 用已存的 golden（python-control 0.10.2 / scipy 1.18.1），随机用例当场用 scipy 计算。
- 指标：前向误差 = max|X − X*| / max|X*|；残差 = ‖Res‖_F / Σ‖各项‖_F。"镇定"要求 |λ| ≤ 1 − 1e-9（离散）或 Re λ ≤ −1e-9·max(1, ‖A‖₁)（连续）。

## 算子对应关系与约定差异

| AEOI 算子 | tpt-control 对应 | 本次覆盖 | 约定/语义差异 |
|---|---|---|---|
| `control.math.lyap` | tpt_ctrl_core::lyapunov::{solve_lyapunov, solve_discrete_lyapunov} | full | AEOI: A X + X Aᵀ + Q = 0 / A X Aᵀ − X + Q = 0；tpt: Aᵀ X + X A = −Q / Aᵀ X A − X = −Q → 传 Aᵀ。tpt 离散用平方/倍增级数（ρ(A) ≥ 1 时 NoConvergence），连续用实 Schur Bartels–Stewart |
| `control.design.lqr` | tpt_ctrl_optimal::{care, lqr} | full（N 交叉项经标准约化） | 同一 CARE 约定 AᵀX + XA − XBR⁻¹BᵀX + Q = 0，K = R⁻¹BᵀX，u = −Kx。tpt 无 N：Ā = A − BR⁻¹Nᵀ、Q̄ = Q − NR⁻¹Nᵀ，K = R⁻¹(BᵀX + Nᵀ)。tpt 用 DARE(ZOH) 求初始镇定增益 + Newton–Kleinman；AEOI 用矩阵符号函数 + 最多 3 步 Newton–Kleinman |
| `control.design.dlqr` | tpt_ctrl_optimal::{dare, dlqr} | full（N 同上） | 同一 DARE 约定，K = (R + BᵀXB)⁻¹BᵀXA。tpt 用结构保持倍增（SDA） |
| `control.lti.c2d` | tpt_ctrl_core::discretize::{zoh, tustin} | ss 输入、zoh/tustin、无 prewarp | 两边 Tustin 都是 scipy 约定（Cd = C(I − T/2·A)⁻¹，Dd = D + ½·C·Bd）。tpt 无 foh、无 prewarp、不处理 tf 输入 |
| `control.lti.bode` | tpt_ctrl_classic::freq::bode_data | 连续 SISO tf | 两边都是 dB / 度 / rad/s；相位都从第一个频点起展开。tpt 无 ss、zpk、离散、MIMO |
| `control.analysis.margin` | tpt_ctrl_classic::freq::margins | 连续 SISO tf | tpt 增益裕度为线性倍数、相位裕度为度；在对数网格（本次 20000 点）上线性插值，只取第一个交越；AEOI 解析求所有交越取最小。tpt `Margins` 的两个频率字段的文档注释写反了（见 bugs） |
| `control.lti.poles_zeros` | TransferFunction::{poles, zeros}；analysis::poles | tf（极点+零点）、ss（仅极点） | tpt 对 tf 首项归一化，零点为分子根；ss 无不变零点 |
| `control.lti.step` | discretize::zoh + simulate::simulate_discrete（组合，非 tpt 现成算子） | ss、SISO tf（经 tpt tf.to_state_space） | 均匀网格上 ZOH 对阶跃输入是精确的；离散系统直接递推 |
| `control.lti.impulse` | 同上（组合） | 同上 | 按 python-control 约定：连续 x0 = B、忽略 D；离散 u[0] = 1/dt |
| `control.analysis.ctrb_obsv` | analysis::{controllability_matrix, observability_matrix, rank}；Gramian 用 tpt lyapunov | ss（含离散） | 秩：tpt 用 QR 对角元相对阈值（本次 1e-10），AEOI 用 SVD + PBH 交叉检查 |
| `control.lti.convert` | TransferFunction::to_state_space | 仅 SISO tf → ss | 实现不同（tpt 相变量形式，AEOI/scipy 控制器形式），比较实现无关的 Markov 参数 D, CB, CAB, …；ss → tf、zpk 无对应 |
| `control.design.place` | 无（tpt 没有极点配置） | 只用 tpt 的特征值求解器复核 AEOI 的 K | 比较 eig(A − BK)（tpt 求）与目标极点、与 AEOI 自报的 achieved_poles |

共同约定：矩阵均为 row-major（AEOI 线格式按 row-major 解析后传给 tpt 的 `DMat::from_fn`），tf 系数按降幂，频率 rad/s，相位为度，幅值为 dB。

## 结果

### 直接比较（AEOI vs tpt）

| 算子 | 用例 | 比较 | 跳过 | 两边都报错 | 字段：最大 / 中位差 | 中位耗时 AEOI / tpt (µs) |
|---|---|---|---|---|---|---|
| lyap | 154 | 145 | 0 | 3 | X 4.6e+04 / 3.9e-13 | 4.3e+01 / 6.7e+00 |
| lqr | 150 | 124 | 0 | 17 | S 3.0e-05 / 7.2e-15; K 3.0e-05 / 5.1e-15 | 1.9e+02 / 8.0e+01 |
| dlqr | 105 | 91 | 0 | 12 | S 4.3e-04 / 5.0e-14; K 1.2e-04 / 8.9e-14 | 1.0e+02 / 1.6e+01 |
| c2d | 80 | 49 | 31 | 0 | A 9.8e-16 / 1.1e-16; B 5.1e-16 / 1.5e-16; C 5.3e-16 / 0; D 1.8e-16 / 0 | 6.7e+01 / — |
| bode | 106 | 60 | 46 | 0 | magnitude_db_max_abs 2.8e-14 / 7.1e-15; phase_deg_max_abs 1.7e-13 / 2.8e-14; phase_deg_max_abs_mod360 1.7e-13 / 2.8e-14 | 6.2e+01 / — |
| margin | 48 | 31 | 17 | 0 | gain_margin_rel 2.8e-06 / 8.6e-08; phase_margin_abs_deg 3.4e-05 / 9.8e-07 | 2.1e+02 / 1.1e+03 |
| poles_zeros | 50 | 43 | 5 | 0 | poles 1.3e-12 / 2.3e-15; zeros 4.9e-12 / 0 | 6.7e+01 / — |
| step | 91 | 86 | 5 | 0 | y 1.4e-11 / 7.2e-15 | 2.4e+02 / — |
| impulse | 31 | 29 | 2 | 0 | y 5.9e-13 / 1.3e-15 | 8.7e+01 / — |
| ctrb_obsv | 29 | 28 | 1 | 0 | ctrb_matrix 0 / 0; obsv_matrix 0 / 0; gramian_c 5.1e-15 / 2.2e-15; gramian_o 7.0e-15 / 3.0e-15 | 1.1e+02 / — |
| convert | 79 | 16 | 63 | 0 | markov 5.1e-15 / 0 | 4.3e+01 / — |
| place | 31 | 27 | 4 | 0 | vs_requested 1.6e-10 / 4.2e-15; vs_aeoi_achieved 4.4e-10 / 4.6e-15 | 8.6e+01 / — |

### 与高精度真值的距离（前向误差 max|X−X*|/max|X*|，最大值；括号内为 50 位精度下的相对残差最大值）

| 算子 | 族 | 用例 | 有真值 | AEOI | tpt | scipy | 最准（次数） |
|---|---|---|---|---|---|---|---|
| dlqr | corpus | 25 | 25 | 3.5e-14 (1.4e-15) · 求出 24/25 | 8.0e-14 (4.5e-15) · 非镇定 1 | 1.0e-13 (3.9e-15) · 非镇定 1 | aeoi 12, tpt 12, scipy 1 |
| dlqr | random/dlqr_bigA_1e3 | 10 | 10 | 2.3e-09 (1.4e-15) | 2.3e-06 (4.3e-09) | 1.6e-05 (9.9e-09) | aeoi 9, tpt 1 |
| dlqr | random/dlqr_condR_1e12 | 10 | 10 | 2.2e-14 (3.1e-15) | 4.3e-04 (1.4e-04) | 2.1e-15 (4.2e-16) | scipy 10 |
| dlqr | random/dlqr_condR_1e8 | 10 | 10 | 2.2e-14 (3.2e-15) | 7.4e-08 (1.6e-08) | 4.9e-15 (1.1e-15) | scipy 8, aeoi 2 |
| dlqr | random/dlqr_crossN | 6 | 6 | 2.3e-15 (3.0e-16) | 2.3e-15 (2.4e-16) | 3.9e-14 (3.3e-15) | aeoi 5, tpt 1 |
| dlqr | random/dlqr_marginal_undetectable | 4 | 2 | 未求出 | 未求出 | 5.4e-18 (1.1e-16) · 非镇定 4 |  |
| dlqr | random/dlqr_smallQ_1e-8 | 10 | 10 | 5.2e-15 (9.4e-16) | 8.3e-12 (1.1e-12) | 2.0e-14 (5.4e-15) | aeoi 9, scipy 1 |
| dlqr | random/dlqr_weakB_1e-4 | 10 | 10 | 2.7e-14 (1.7e-15) | 3.5e-12 (1.7e-12) | 1.4e-06 (5.9e-08) | aeoi 10 |
| dlqr | random/dlqr_well | 10 | 10 | 4.7e-14 (7.5e-16) | 3.4e-15 (2.9e-15) | 1.0e-11 (4.5e-13) | aeoi 6, tpt 4 |
| dlqr | repro | 2 | 2 | 2.0e-16 (2.8e-17) · 求出 1/2 | 2.0e-16 (2.8e-17) · 非镇定 1 | 6.0e-16 (6.5e-17) · 非镇定 1 | aeoi 1, tpt 1 |
| lqr | corpus | 66 | 62 | 2.5e-06 (7.3e-07) · 求出 65/66 | 2.8e-11 (2.3e-11) · 求出 65/66 | 7.0e-10 (7.4e-09) · 非镇定 1 | aeoi 29, tpt 22, scipy 11 |
| lqr | random/lqr_bigA_1e3 | 10 | 10 | 2.0e-12 (1.7e-14) | 1.6e-13 (1.4e-14) | 4.0e-11 (1.3e-11) | tpt 6, aeoi 4 |
| lqr | random/lqr_condR_1e12 | 10 | 10 | 3.0e-05 (1.8e-05) · 求出 3/10 | 7.4e-09 (7.1e-05) | 6.1e-11 (2.3e-05) | scipy 10 |
| lqr | random/lqr_condR_1e8 | 10 | 10 | 1.8e-08 (1.3e-08) | 1.7e-12 (4.0e-10) | 2.0e-12 (4.1e-09) | scipy 8, tpt 2 |
| lqr | random/lqr_crossN | 6 | 6 | 8.2e-14 (3.0e-15) | 3.2e-14 (1.7e-15) | 1.0e-13 (4.8e-14) | aeoi 3, tpt 2, scipy 1 |
| lqr | random/lqr_marginal_undetectable | 4 | 4 | 未求出 | 未求出 | 3.6e-16 (3.1e-16) · 非镇定 4 |  |
| lqr | random/lqr_smallQ_1e-8 | 10 | 10 | 4.8e-14 (5.6e-16) | 1.9e-14 (1.9e-15) | 1.4e-08 (1.4e-09) | scipy 4, tpt 3, aeoi 3 |
| lqr | random/lqr_weakB_1e-4 | 10 | 10 | 2.9e-10 (1.4e-13) | 5.2e-13 (2.4e-13) | 1.1e-03 (5.2e-04) | tpt 6, aeoi 4 |
| lqr | random/lqr_well | 10 | 10 | 5.9e-12 (2.5e-14) | 3.7e-13 (1.3e-14) | 3.6e-13 (1.7e-13) | scipy 4, aeoi 3, tpt 3 |
| lqr | repro | 1 | 1 | 未求出 | 5.1e-23 (6.9e-17) | 1.1e-17 (2.7e-11) | tpt 1 |
| lyap | corpus | 33 | 31 | 2.8e-14 (1.3e-14) · 求出 31/33 | 1.7e-14 (1.3e-14) · 求出 31/33 | 2.8e-14 (8.4e-15) | tpt 14, aeoi 10, scipy 7 |
| lyap | random/lyap_c_near_axis_1e-3 | 12 | 12 | 1.7e-12 (1.0e-12) | 2.0e-12 (4.9e-13) | 5.5e-13 (5.4e-13) | scipy 7, aeoi 3, tpt 2 |
| lyap | random/lyap_c_near_axis_1e-6 | 12 | 12 | 2.1e-09 (7.8e-10) | 1.7e-09 (8.1e-10) | 1.1e-09 (5.5e-10) | tpt 5, aeoi 4, scipy 3 |
| lyap | random/lyap_c_nonnormal_rot_1e1 | 8 | 8 | 4.9e-03 (3.2e-11) | 2.1e-02 (1.5e-11) | 2.1e-02 (4.2e-11) | tpt 4, aeoi 2, scipy 2 |
| lyap | random/lyap_c_nonnormal_rot_1e2 | 8 | 8 | 1.0e+00 (7.9e-10) | 1.0e+00 (1.0e-09) | 5.7e-01 (1.9e-09) | scipy 4, aeoi 3, tpt 1 |
| lyap | random/lyap_c_nonnormal_rot_1e3 | 8 | 8 | 9.0e+01 (9.2e-08) | 7.6e+03 (6.4e-08) | 1.0e+00 (7.5e-08) | scipy 4, tpt 3, aeoi 1 |
| lyap | random/lyap_c_nonnormal_triangular | 12 | 12 | 5.0e-16 (3.0e-11) | 1.0e+00 (1.2e-07) | 5.2e-16 (1.5e-11) | scipy 6, aeoi 5, tpt 1 |
| lyap | random/lyap_c_unstable | 6 | 6 | 2.5e-14 (2.4e-14) | 8.6e-15 (1.8e-14) | 1.2e-14 (2.7e-14) | scipy 3, aeoi 2, tpt 1 |
| lyap | random/lyap_c_well | 12 | 12 | 6.3e-15 (2.5e-15) | 6.4e-15 (2.6e-15) | 3.2e-15 (1.4e-15) | scipy 7, tpt 3, aeoi 2 |
| lyap | random/lyap_d_rho_0.999 | 12 | 12 | 4.3e-12 (5.3e-15) | 6.0e-14 (1.0e-15) | 1.5e-13 (1.1e-15) | tpt 6, scipy 6 |
| lyap | random/lyap_d_rho_1-1e-6 | 12 | 12 | 6.0e-09 (6.1e-15) | 5.5e-11 (1.3e-15) | 2.6e-10 (1.3e-10) | tpt 6, scipy 5, aeoi 1 |
| lyap | random/lyap_d_unstable | 6 | 6 | 4.1e-14 (4.1e-15) | 未求出 | 4.3e-15 (1.4e-16) | scipy 5, aeoi 1 |
| lyap | random/lyap_d_well | 12 | 12 | 2.4e-14 (6.8e-15) | 2.0e-16 (2.9e-16) | 4.9e-15 (8.4e-16) | tpt 12 |

### 报错/失败行为不一致

| 用例 | 算子 | AEOI | tpt | tpt 解镇定? | scipy | 真值镇定? |
|---|---|---|---|---|---|---|
| `97a7c31c120e0e91eaeb322a06a86d1f4fb0db8a16e.json` | dlqr | E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG | ok | False | ok | False |
| `random/lyap_d_unstable/0` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lyap_d_unstable/1` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lyap_d_unstable/2` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lyap_d_unstable/3` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lyap_d_unstable/4` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lyap_d_unstable/5` | lyap | ok_with_warnings | NoConvergence | None | ok | None |
| `random/lqr_condR_1e12/2` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/4` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/5` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/6` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/7` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/8` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `random/lqr_condR_1e12/9` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `repro/lqr_diag_condR_1e12` | lqr | E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG | ok | True | ok | True |
| `repro/dlqr_rotation_Q0` | dlqr | E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG | ok | False | ok | False |

### 非法输入（AEOI 拒绝，不计入精度表）

| 用例 | 算子 | AEOI | tpt | scipy |
|---|---|---|---|---|
| `ff0096e886f51b62fa2a2adc745882b7db2291dda74.json` | dlqr | E.DOMAIN.R_NOT_POSITIVE_DEFINITE | SingularMatrix | ok |
| `adb57b5bc249b6a8c496e12c993db7af38055e975f1.json` | lqr | E.VALIDATION.NOT_SYMMETRIC | ok (no input check) | ValueError: Matrix q should be symmetric/hermitian. |
| `06a1890228860c095341b3c8730f6e11fa7b1c35b79.json` | lqr | E.VALIDATION.SHAPE | DimensionMismatch { expected: (3, 3), found: (2, 3) } | ValueError: Matrix a and b should have the same number of rows. |
| `faff56be6898e102c7eef887a16f59538ef8b807c8b.json` | lqr | E.DOMAIN.R_NOT_POSITIVE_DEFINITE | SingularMatrix | LinAlgError: Failed to find a finite solution. |
| `cfff93a0eae51e29c3128966474b49cb13ac3ebf1be.json` | lqr | E.DOMAIN.R_NOT_POSITIVE_DEFINITE | SingularMatrix | ValueError: Matrix r is numerically singular. |

### CARE 改进原型（不改 aeoi-control，只在 Python 里验证）

| 用例 | n×m | cond(R) | AEOI 前向误差 | AEOI + K 形式 Newton–Kleinman | tpt |
|---|---|---|---|---|---|
| `ba81aa6a0edbd6d9b6f0b4c077791.json` | 10×1 | 1e+00 | 3.6e-08 | 4.1e-11 | 2.1e-11 |
| `6f3d1905ef0972b80997e1e95d7ec.json` | 4×2 | 1e+09 | 2.5e-06 | 1.5e-11 | 1.1e-11 |
| `0607072b26828c19a302e5bb535ab.json` | 4×2 | 1e+09 | 2.6e-08 | 2.6e-11 | 3.9e-12 |
| `1033b069f9167687a05c5c2fe6c1d.json` | 4×2 | 1e+08 | 3.2e-08 | 1.1e-11 | 2.8e-11 |
| `random/lqr_condR_1e8/0` | 4×2 | 1e+08 | 5.2e-10 | 2.4e-13 | 4.9e-13 |
| `random/lqr_condR_1e8/1` | 5×3 | 1e+08 | 4.8e-10 | 2.6e-13 | 4.6e-13 |
| `random/lqr_condR_1e8/2` | 7×3 | 1e+08 | 1.0e-09 | 1.2e-12 | 8.9e-13 |
| `random/lqr_condR_1e8/3` | 2×2 | 1e+08 | 1.8e-08 | 3.5e-13 | 7.1e-13 |
| `random/lqr_condR_1e8/4` | 4×2 | 1e+08 | 9.5e-09 | 1.3e-12 | 3.3e-13 |
| `random/lqr_condR_1e8/5` | 4×3 | 1e+08 | 5.9e-10 | 1.3e-12 | 6.8e-13 |
| `random/lqr_condR_1e8/6` | 5×2 | 1e+08 | 6.5e-09 | 2.5e-12 | 6.2e-13 |
| `random/lqr_condR_1e8/7` | 3×2 | 1e+08 | 8.6e-10 | 2.2e-12 | 1.7e-12 |
| `random/lqr_condR_1e8/8` | 5×3 | 1e+08 | 6.6e-10 | 2.2e-13 | 4.8e-13 |
| `random/lqr_condR_1e8/9` | 2×2 | 1e+08 | 1.3e-10 | 8.5e-13 | 3.5e-13 |
| `random/lqr_condR_1e12/0` | 5×2 | 1e+12 | 3.0e-05 | 1.6e-10 | 8.6e-11 |
| `random/lqr_condR_1e12/1` | 4×2 | 1e+12 | 1.1e-05 | 5.0e-11 | 1.6e-10 |
| `random/lqr_condR_1e12/3` | 3×2 | 1e+12 | 2.1e-05 | 4.9e-11 | 2.2e-10 |


说明：

- lyap 直接比较的最大差（4.6e4）来自非正规族。这些问题的 Kronecker 条件数达 1e17–1e25，在 f64 下本身不适定，三方误差都是 O(1)。`lyap_c_nonnormal_triangular` 族中 AEOI 与 scipy 准确，只是因为 A 本身是上三角，Schur 分解没有舍入；tpt 按约定拿到的是 Aᵀ（下三角），需要真正做 QR 迭代。这不算 tpt 的缺陷。把 A 做一次随机正交相似后（`rot` 族），三方误差相当。
- 耗时只作参考：AEOI 的耗时包含 schema 校验、信封、哈希和信任度元数据（残差、误差估计、条件数），tpt 只算解。
- step/impulse 的"tpt"结果是用 tpt 的 ZOH 离散化加递推组合出来的，不是 tpt 的现成算子。
- poles_zeros 中有 2 个用例（C = 0 的退化系统）AEOI 报 `E.DOMAIN.DEGENERATE_SYSTEM`，因为零点没有定义；tpt 只求极点，所以没有放进"不一致"表。

## 发现的问题与复现输入

1. **AEOI lqr：Hamiltonian 虚轴检查误报**（`rust/aeoi-control/src/lqr.rs`，阈值 `min|Re λ| ≤ 1e3·ε·‖H‖₁`）
   ```json
   {"A":{"dtype":"f64","shape":[2,2],"data":[-0.1,0,0,-1]},
    "B":{"dtype":"f64","shape":[2,2],"data":[0.1,0,0,1]},
    "Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},
    "R":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1e-12]}}
   ```
   期望 S = diag(4.142135623730952, 9.99999e-7)。AEOI 返回 `E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG`。原因：‖H‖₁ ≈ ‖G‖ = 1e12，阈值约 0.22，而模态 1 的 Hamiltonian 特征值为 ±0.1414。随机用例 `random/lqr_condR_1e12/{2,4,5,6,7,8,9}` 同样误报（输入在 JSON 的 `cases` 里，也可由 harness 重新生成）。
2. **AEOI lqr：病态 R 时精度不足**（Newton–Kleinman 的接受准则与 X·G·X 形式），数字见上表；带 `W.NUMERICAL.ILL_CONDITIONED`。
3. **tpt dare：返回非镇定解**
   ```text
   A = [[cos 0.3, −sin 0.3], [sin 0.3, cos 0.3]], B = [[0], [1]], Q = 0, R = [[1]]
   tpt: X = 0, K = 0, eig(A − BK) = 0.9553 ± 0.2955j（|λ| = 1）   AEOI: E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG
   ```
   golden 中的 `control.design.dlqr/0.3.0/a981bc1f…` 是同一个问题。
4. **tpt 文档**：`freq::Margins` 两个交越频率字段的注释写反；`solve_lyapunov` 关于不稳定 A 的说明与实际行为不符；`lqr`/`care` 不检查 Q 的对称性。

## 改进想法（CARE 与离散 Lyapunov 部分已在 v0.3.2 实施，见文首）

- **lqr**：
  1. Newton–Kleinman 用 K 形式：K = chol(R)⁻¹(BᵀX + Nᵀ)，Lyapunov 方程右端为 Q − NK − KᵀNᵀ + KᵀRK，不显式构造 G = BR⁻¹Bᵀ。
  2. 按 ‖ΔX‖_F/‖X‖_F < 1e-13 停止，或在不再下降时停止，最多约 10 步，不再用 f64 残差作为接受准则。
  3. Lyapunov 子问题改用 Bartels–Stewart（复 Schur），取代 n ≤ 32 的 Kronecker 形式，这样 n > 32 也能修正。
  4. 虚轴检查改用平衡后的 H，或改成事后判定：符号函数不收敛或闭环不镇定才报错，判定虚轴特征值时用相对 |λ| 的阈值。
  
  上面的原型（`tools/xcheck-tpt/prototype_nk.py`）表明，仅第 1、2 条就足以把 cond(R) = 1e12 时的误差从 1e-5 降到 1e-10。
- **dlqr**：保持现方法。可以借鉴 tpt 的一点：初值或回退时用倍增，但 R 病态时不要显式求 R⁻¹。
- **lyap（离散，ρ → 1）**：可以对 Bartels–Stewart 的结果做一步残差修正，或者在 Q ⪰ 0 且 ρ(A) < 1 时用平方级数交叉检查。

## 复现

```bash
cd tools/xcheck-tpt
cp ../../rust/Cargo.lock .            # 与主 workspace 相同的依赖版本（Rust 1.85 兼容）
cargo run --release --bin xcheck-tpt -- ../.. /tmp/xcheck-tpt.jsonl
PY=/path/to/venv/bin/python           # 需要 numpy、scipy、mpmath
$PY analyze.py /tmp/xcheck-tpt.jsonl /tmp/analysis.json
$PY prototype_nk.py /tmp/xcheck-tpt.jsonl /tmp/proto.json
$PY make_report.py /tmp/analysis.json /tmp/proto.json /tmp/xcheck-tpt.jsonl ../../reports/parity-tpt-control.json
```

本对比的局限：tpt 只覆盖连续 SISO tf 的 bode/margin（ss、zpk、离散、MIMO 未比较）；c2d 的 foh 与 prewarp、convert 的 ss→tf、place 本身都没有 tpt 对应；真值只算到 n ≤ 10（golden 里 n = 20–40 的 lqr 只比较了残差）；随机用例每族 6–12 个，族内的结论是"最大值"，并非统计意义上的保证。
