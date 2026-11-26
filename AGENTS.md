# AGENTS.md

给在本仓库工作的 AI agent（Claude Code、Codex 等）的入口说明。完整流程见 [`.claude/skills/aeoi-operator-workflow/SKILL.md`](.claude/skills/aeoi-operator-workflow/SKILL.md)；本文件只列最要紧的几条。

## 这是什么
AEOI（AI Engineering Operator Interface Spec）是叠加在 MCP 之上的工程计算语义层：规范（核心协议 `spec-src.md` → `spec-v0.4.md`，算子契约 `profiles/`，参考实现说明 `docs/reference-impl.md`，历史 `CHANGELOG.md`）、Rust 参考实现（`rust/`）、golden 参考数据（`golden/`）、示例（`examples/`）和跨实现一致性套件（`conformance/`）。参考值来自 python-control / numpy / scipy / mpmath，**不是 MATLAB**。

## 当前状态从哪里看
- `CHANGELOG.md` 的最新一节（做了什么、为什么、结果数字）和规范 12.2「下一步」。
- 有 claude.ai Project 时，另有交接文档 `claude/aeoi-handoff.md`（未提交的改动与待办）。

## 必须遵守
1. **提交、推送不必等作者确认（作者 2026-11-25 授权），但每个里程碑都要提交并推送，不留未推送的提交。打 tag / 正式发布（GitHub Release、MCP 目录）仍要作者当次同意。**
2. 每个新算子同时带：schema（经 `scripts/write_manifests.py`）、Rust 实现、golden（`scripts/golden/gen_golden_<op>.py`）、示例与 `parity.yaml`、一致性规则与代理变异；它逼出的规范问题写进规范和 Changelog。一次 1–2 个算子。
3. 一致性容差来自套件，不来自被测实现自报的值（CF-6）。有数值规则的算子 ≥ 全部算子的 30%。
4. 验证一律看**退出码**，不要只看输出的最后一行。
5. `scripts/write_manifests.py` 是 manifest 与 I/O schema 的唯一来源；不要手改 `examples/*/manifest.json`、`*.schema.json`。
6. 手写的解析解（golden 的真值）要用独立的高精度方法交叉核对。
7. 推送后查看 CI 结果（`gh api repos/yuanqingfei/aeoi/actions/runs`）。Pages 的冒烟测试曾连续失败 5 次没人发现。
8. 不要引入依赖系统数学库或按 CPU 分派内核的计算路径（规范 2.1 M-7a）：超越函数经 `aeoi-libm` 走编入的 Rust `libm`，矩阵乘法用 `rust/vendor/matrixmultiply` 的通用内核。
9. **算子之间复用必须调用同一内核**（规范 11.5 RI-1，作者 2026-11-25）：一个算子用到另一个算子的计算（线性方程组、QR、SVD、特征分解……），调用 `aeoi-core::kernels` 里那个算子所基于的代码，不得另写一份，也不得改调 nalgebra 等外部库的同类功能。新内核放进 `aeoi-core::kernels`，算子只做校验、输出与后置条件。
10. 目标：约 100 个常用工程计算函数（规范 12.3 的清单与依赖层，唯一来源 `roadmap/targets.py`），按依赖层自底向上实现。102 个全部完成后才发布 v0.5，在此之前不打 tag（作者 2026-11-25）。

## 验证
```bash
cd rust && cargo test --workspace --release && cargo build --release -p aeoi-mcp -p aeoi-ops && cd ..
python3 scripts/write_manifests.py && git status          # 不应有意外改动
python3 scripts/validate_examples.py; echo $?             # 0
python3 -I conformance/aeoi_conformance.py run --impl rust/target/release/aeoi-mcp --report /tmp/c.json; echo $?
python3 -I conformance/tests/test_runner_catches_mutants.py; echo $?
python3 scripts/build_spec.py && grep -c '{{' spec-v0.4.md   # 0
git checkout reports/parity-python-control*.json  # 测试会改写它们
```

## 语言
与作者用中文交流；规范正文中文，代码注释英文。
