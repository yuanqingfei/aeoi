---
name: "aeoi-operator-workflow"
description: "Use when working on the AEOI repo (yuanqingfei/aeoi): adding a new operator, evolving the spec, writing golden/parity/conformance rules, or committing work."
---

# AEOI 工作流程

AEOI（AI Engineering Operator Interface Spec）是叠加在 MCP 之上的工程计算语义层：规定工程算子如何描述自己、如何表达带物理意义的输入输出、如何报告可信度、如何报错、如何复现、如何证明与参考实现一致。仓库 github.com/yuanqingfei/aeoi，分支 `main`。

## 先读当前状态
- 有 claude.ai Project「面向AI的工程计算」时，动手前读 Project 文档 `claude/aeoi-handoff.md`（最新提交、未提交的改动、待办、作者的路线），每个里程碑后更新它（project_read → 改 → project_write 整篇写回）。
- 没有 Project 时：`git log`、规范第 13 章 Changelog 的最新一节、12.2「下一步」。
- 分工：handoff 是易变状态；本文件是不变的做法；历史在规范 Changelog。不要另建 changelog 文件。

## 作者的规则（不可违反）
- **提交、推送不必等作者确认**（作者 2026-11-25 授权）；做完一步就提交并推送，不留未推送的提交。**打 tag / 正式发布仍要作者当次同意。**
- 提交：`git -c user.name=Claude -c user.email=noreply@anthropic.com commit`；推送：`git -c push.negotiate=true push origin main`；提交信息末尾带会话给出的 attribution 行。
- 路线：不断加新算子，一方面扩大用户群，一方面让算子逼出规范问题并写进规范。一次 1–2 个。领域按 MATLAB 使用频率排（控制已告一段落；已做 fft、ode45；候选 interp1、polyfit、linsolve、fzero/fsolve、butter+filter、eig/svd、integral）。
- **算子之间复用必须调用同一内核**（规范 11.5 RI-1，作者 2026-11-25）：要用另一个算子的计算时，调用 `aeoi-core::kernels` 里它所基于的代码；不得重写，不得改调外部库的同类功能。新算子先把可复用的部分写成内核，再在外面包一层算子。
- 目标是约 100 个常用工程计算函数，清单与顺序见规范 12.3。
- 一致性数值规则覆盖的算子 ≥ 全部算子的 30%（以后提高）；每加一个算子都带规则。
- 有真正的设计选择（指标、约定、语法）时用 AskUserQuestion 让作者选，给出推荐项。

## 仓库结构
- `spec-src.md` → `python3 build_spec.py` → `spec-v0.4.md`（展开 `{{file:…}}`、`{{jsonc:…}}`；构建后 grep `{{` 必须为 0）。线协议标识 `aeoi/0.4`；每个算子发布一个补丁版本 v0.4.N-draft。
- `write_manifests.py`：manifest 与 I/O schema 的唯一来源，生成 `examples/<op>/{manifest,input.schema,output.schema}.json`。
- `gen_golden_<op>.py`：生成 `golden/<engine>/<op>/<ver>/*.json` 和示例请求、`reference.python-control.json`。参考值来自 python-control / numpy / scipy / mpmath，**不是 MATLAB**，到处都要这样标注。
- `regen_examples.py`：用 Rust `aeoi-call` 重写 `examples/*/response*.json`。
- `validate_examples.py`：schema 校验全部示例与 golden 请求。
- Rust（`rust/`）：`aeoi-core`；`aeoi-libm`（数学符号覆盖）；`aeoi-control`（控制算子）；`aeoi-numeric`（控制以外：fft、expr、ode）；`aeoi-ops`（跨领域 `registry()`/`find()`、`aeoi-call` bin、遍历全部 golden 的 `tests/receipt.rs`）；前端 `aeoi-mcp`、bridge、web、wasm 都用 `aeoi_ops::registry()`。
- `conformance/`：`aeoi_conformance.py`（运行器，只经 MCP stdio 驱动实现）、`rules_control.py`、`rules_numeric.py`、`tests/proxy_mutant.py`、`tests/test_runner_catches_mutants.py`、`README.md`。

## 加一个新算子的清单
1. **先定约定。** 归一化、轴顺序、缺省值、单位、命名、可达条件、多解时取哪个解，全部写死并在结果里回显（O-1）。不确定就问作者。
2. **Rust 实现**（放在相应领域的 crate，用 `operator_struct!`/`operator_meta!` 宏，并在 `registry()` 注册）：
   - 所有验证错误都给 `path`（JSON Pointer）和 remediation（`modify_input` 带 RFC 6902 patch；改变物理含义的修复要 `requires_user_confirmation: true`）。
   - 互斥选项一律报 `E.VALIDATION.OPTION_CONFLICT`（F-8），补丁只删除请求里已出现的字段。
   - 区分 DOMAIN（问题本身不成立，别调参）与 NUMERICAL（换数值处理可能成功）。
   - 数值阈值以 u 和范数表达，与尺度无关（X-4a）。
   - 确定性 `platform_bitwise`：运算路径不得依赖运行时 CPU 特性（FFT、Schur 等都用 `aeoi-core::kernels` 自己的实现）。
   - 后置条件要能如实失败：失败时发 `W.EVIDENCE.POSTCONDITION_FAILED`，并在 GUARDS 里列出不可依赖的字段。
3. **write_manifests.py**：`man(..., backend=)`、`ins`、`outs`、B3 条目、DESC（L-1 字段描述）、`RECEIPT[...]`、COVERAGE、GUARDS。约束：
   - `exactness.bound_kind` ∈ {rigorous, a_priori, a_posteriori_heuristic, none}；
   - COVERAGE 的值 ∈ {full, test_points, sampled}（有 sampled 时 claim 最多 spot_checked）；
   - `limits` 的值只能是数字（不能是数组）；
   - 框架码 `W.EVIDENCE.POSTCONDITION_FAILED` 不在 manifest 里声明（E-7a）；其他可能返回的错误、警告码都要声明。
4. **golden 生成脚本**：独立参考（最好再加高精度或解析的“真值”）；成功和错误用例都要有（错误用例写 `expected_error`）。可选字段：`expect_warning`、`expect_postcondition_failed`（后置条件不成立本身就是正确答案时，由生成脚本独立预测）。**手写的解析解要用另一种高精度方法交叉核对**（曾把 `- -2^2*3` 算错，测试没发现）。
5. **Rust 测试**：`tests/parity_<op>.rs`，写出 `reports/parity-<op>.json`；在 `tests/examples_<crate>.rs` 里加示例测试（同平台逐位比较 hash、与参考值比较、错误示例的码与 remediation）。
6. **示例**：`examples/<op>/request.json`、`request.error.json`，再加一两个有教育意义的（例如缺省容差被标为不可依赖）；跑 `regen_examples.py`；写 `parity.yaml`（references、compare、`conformance_compare`、case_families）。
7. **一致性规则**（`rules_*.py` 的 `RULES` 字典）：容差来自套件，绝不来自实现自报的值（CF-6）；尽量加一项由运行器从请求独立重算的检查（Parseval、L(jωc)、对真值核查 claim 是否属实）。再加 2–3 个代理变异（改错数值、标签错位、虚报 claim），并在 `test_runner_catches_mutants.py` 的 `PROXY_EXPECT` 里写明预期原因。等价答案要能通过。
8. **规范**：新增 10.x 节（规范性条款编号如 FT-1…、OD-1…；每条说明原因；示例和 parity.yaml 用 `{{…}}` 引入）；跨算子的机制另立第 3 章小节（如 3.15 表达式语言）；新码登记到 5.1 的表格；更新文件头的状态行、版本摘要段、12.2 下一步，以及 Changelog（起因 / 规范改动表 / 已做的改动表 / 开发中发现的错误 / 结果数字）。
9. 更新 `conformance/README.md` 的覆盖率和结果数字。

## 验证（一律看退出码，不看最后一行）
```bash
cd rust && cargo test --workspace --release        # 全部通过
cargo build --release -p aeoi-mcp -p aeoi-ops
cd .. && python3 write_manifests.py && git status   # 不应产生意外改动
python3 validate_examples.py; echo $?              # 必须 0（曾因只看汇总行漏掉 FAIL）
python3 -I conformance/aeoi_conformance.py run --impl rust/target/release/aeoi-mcp --report /tmp/c.json; echo $?
python3 -I conformance/tests/test_runner_catches_mutants.py; echo $?   # 含 Python 第二实现与全部变异
python3 build_spec.py && grep -c '{{' spec-v0.4.md # 0
git checkout reports/parity-python-control*.json   # 测试会改写它们；receipts.json 与 parity-<op>.json 是有意更新
```

## 积累的经验
- **推送后要看 CI。** `gh api repos/yuanqingfei/aeoi/actions/runs --jq '.workflow_runs[:5][] | {name, conclusion, head_sha}'`。Pages 冒烟测试把工具数写死为 19，从 v0.4.5 起连续 5 次推送都失败，没人发现。
- **跨平台逐位相同（M-7a）。** 不要直接引入会调用系统数学库或在运行时按 CPU 分派内核的依赖。`aeoi-libm` 把 C 数学符号转给编入的 Rust `libm`；`rust/vendor/matrixmultiply` 固定用通用内核。改了数值路径要重新生成示例（`regen_examples.py`）。
- **比较对象要选对。** 与参考求解器逐点比较只适用于结果唯一确定的算子。自适应算法（ODE）在折点、混沌、长区间上，两个正确实现会因一次接受/拒绝决策不同而分岔：对照真值，与参考之差只记录。
- **容差界不能把错掉的参考盖住。** 含“参考自身误差 × 2”的界会掩盖真值写错，所以真值要独立核对。
- **局部容差 ≠ 全局精度。** 让结果报告实际达到的精度（后验估计 + 后置条件），比报告成功更有用；golden 要包含如实失败的用例。
- **约定类错误看起来都“像对的”**（FFT 的归一化、fftshift；bode 相位分支），因此要显式回显，并让变异测试专门覆盖。
- 新的测试假设被新算子打破时（例如“golden 成功用例的后置条件都成立”），在 golden 里加显式标记，不要放松断言。
- 收据测试在 `aeoi-ops`，因为它要看到所有领域；不要用某一个领域的注册表遍历全部 golden。
- 新 crate 的算子通过 `operator.build` 报告自己所在的 crate（`with_crate(env!("CARGO_PKG_NAME"))`）。

## 环境
- 云容器：仓库在 `/home/claude/aeoi`（需要先用 add_repo 以 push 权限加入会话）。作者 WSL 路径 `/mnt/c/d/projects/aeoi`。
- golden 依赖：`pip install control==0.10.2 mpmath==1.4.1`，以及 numpy、scipy。
- 与作者用中文交流；规范正文用中文，代码注释用英文。