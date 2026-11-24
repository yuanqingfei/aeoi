# AGENTS.md

给在本仓库工作的 AI agent（Claude Code、Codex 等）的入口说明。完整流程见 [`.claude/skills/aeoi-operator-workflow/SKILL.md`](.claude/skills/aeoi-operator-workflow/SKILL.md)；本文件只列最要紧的几条。

## 这是什么
AEOI（AI Engineering Operator Interface Spec）是叠加在 MCP 之上的工程计算语义层：规范（`spec-src.md` → `spec-v0.4.md`）、Rust 参考实现（`rust/`）、golden 参考数据（`golden/`）、示例（`examples/`）和跨实现一致性套件（`conformance/`）。参考值来自 python-control / numpy / scipy / mpmath，**不是 MATLAB**。

## 当前状态从哪里看
- 规范第 13 章 Changelog 的最新一节（做了什么、为什么、结果数字）和 12.2「下一步」。
- 有 claude.ai Project 时，另有交接文档 `claude/aeoi-handoff.md`（未提交的改动与待办）。

## 必须遵守
1. **提交、推送要作者当次明确同意。**
2. 每个新算子同时带：schema（经 `write_manifests.py`）、Rust 实现、golden（`gen_golden_<op>.py`）、示例与 `parity.yaml`、一致性规则与代理变异；它逼出的规范问题写进规范和 Changelog。一次 1–2 个算子。
3. 一致性容差来自套件，不来自被测实现自报的值（CF-6）。有数值规则的算子 ≥ 全部算子的 30%。
4. 验证一律看**退出码**，不要只看输出的最后一行。
5. `write_manifests.py` 是 manifest 与 I/O schema 的唯一来源；不要手改 `examples/*/manifest.json`、`*.schema.json`。
6. 手写的解析解（golden 的真值）要用独立的高精度方法交叉核对。

## 验证
```bash
cd rust && cargo test --workspace --release && cargo build --release -p aeoi-mcp -p aeoi-ops && cd ..
python3 write_manifests.py && git status          # 不应有意外改动
python3 validate_examples.py; echo $?             # 0
python3 -I conformance/aeoi_conformance.py run --impl rust/target/release/aeoi-mcp --report /tmp/c.json; echo $?
python3 -I conformance/tests/test_runner_catches_mutants.py; echo $?
python3 build_spec.py && grep -c '{{' spec-v0.4.md   # 0
git checkout reports/parity-python-control*.json  # 测试会改写它们
```

## 语言
与作者用中文交流；规范正文中文，代码注释英文。
