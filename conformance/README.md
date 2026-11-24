# AEOI 跨实现一致性套件（v0.1）

规范 9.4（CF-1 – CF-12）的第一个实现。**状态：运行器与一个第二实现已可用，只覆盖 `control.math.lyap`；参考实现（Rust）还没有用它跑过。**

## 内容

| 路径 | 作用 |
|---|---|
| `aeoi_conformance.py` | 运行器。只通过 MCP stdio 驱动被测实现（CF-3），用例直接读 `golden/`（CF-2） |
| `aeoi_hash.py` | 规范 3.7 / 4.3 R-3 / P-2 的独立重写（RFC 8785 JCS、数组摘要、请求指纹）。运行器用它重算指纹，不采信被测实现自报的值 |
| `impls/python_scipy/server.py` | 第二实现：Python + SciPy，只做 `lyap`，线级（MCP stdio） |
| `tests/test_runner_catches_mutants.py` | 证明运行器会失败：对第二实现注入 7 种缺陷，每一种都必须被抓出并且原因对得上 |

## 运行

依赖：Python ≥ 3.10、`numpy`、`scipy`、`jsonschema`（≥ 4.18，含 `referencing`）。在仓库根目录执行：

```
python3 conformance/aeoi_conformance.py run --impl "python3 conformance/impls/python_scipy/server.py" \
    --operators control.math.lyap --report report.json
python3 conformance/tests/test_runner_catches_mutants.py
```

`--impl` 是启动被测实现的命令（它必须在 stdin/stdout 上说 MCP，参考实现是 `rust/target/release/aeoi-mcp`）。退出码 0 = 没有失败用例。报告格式见 CF-11。

## 运行器检查什么

每个用例：isError 与 `status` 一致（C-1）；信封通过 envelope schema；结果通过 manifest 的 output schema；出现的 `E.`/`W.` 码都在 manifest 里声明；预期错误码精确匹配、预期警告出现；`fingerprint.request` 与 `input_hash` 等于运行器重算值（CF-4）；`output_hash` 是所返回结果的哈希；收据的方法 id 在 manifest 登记的集合里；收据 `claim` 等于运行器按 E-11/E-12 重新推出的值；声明了 `bitwise`/`platform_bitwise`/`seeded` 的算子，同一请求运行两次 `output_hash` 相同（P-7）。

数值比较：只有 `RULES` 里的算子有规则（现在只有 `lyap`：`X` 的相对 Frobenius 范数，rtol = 容差类 rtol × max(1, κ/100)，κ 由运行器用参考请求计算，V-6）。**没有规则的算子，成功用例记为 `skip`**，它们的错误码与预期警告仍会检查。第二实现自报的 `error_estimate` 不会用来放宽任何容差（CF-6）。

另有一个方法选择用例（CF-10 里今天就能跑的部分）：请求 `exec.method_policy.require` 一个不可能存在的方法 id，实现 MUST NOT 成功（返回 `E.VALIDATION.SCHEMA` 或 `E.VALIDATION.METHOD_UNAVAILABLE` 都可以）。

## 结果（第二实现，`lyap`）

44 个用例全部通过：mpmath 12 个（独立于 SciPy，含病态的 `continuous_near_axis`、`discrete_rho_near_1`）、python-control 31 个（**与第二实现同源**，只能说明套件可移植，不能说明实现可替换，报告里分开列出）、方法选择 1 个。

## 已知限制

- 只有 `lyap` 有数值规则；其余 18 个算子的成功用例是 `skip`（要补各自的比较规则，特别是 V-5 / V-5a 的不变量比较）。
- 没有在参考实现上跑过（这个环境编不了 Rust）。**下一步第一件事：在能编译的机器上 `cargo build --release -p aeoi-mcp`，用 `--impl rust/target/release/aeoi-mcp` 跑一遍，参考实现必须先自己通过。**
- 哈希只支持 `json` 编码的稠密数组；`b64` / `ref` / 稀疏数组的请求不重算指纹。
- 没有 CF-10 的逐方法运行（manifest 里还没有 `receipt.alternatives`，参考实现也还没实现 `method_policy`）。
- 第二实现的 manifest 复用参考实现的 `examples/control.math.lyap/manifest.json`，只替换了方法、后端、构建与精度等级，所以它不是在“只看规范”的条件下写出来的。

## 用这个套件发现的规范问题

1. ~~已解决（规范 v0.4.1，3.9 J-1）：所有 schema 改为标准引用，运行器不再改写，遇到无法解析的引用判失败。~~ 原问题：输出（和输入）schema 里的引用写成 `aeoi://schemas/common/0.4/Matrix`，按标准 JSON Schema 解析不了（应为 `…/0.3#/$defs/Matrix`），3.9 也没有说明这种约定。运行器自己改写后才能校验。
2. `lyap`：input schema 把 Q 描述为“对称”，算法说明却写“Q 非对称也可解”；`W.VALIDATION.SYMMETRIZED` 在 manifest 里声明了，但没有规定什么时候发。第二实现第一版就是因为把非对称 Q 对称化而在 4 个用例上失败。
3. `lyap` 的 `type` 有默认值 `continuous`，golden 里没有一个用例省略它，所以套件不检验默认值。第二实现第一版因此把它当必填也没被发现（读 input schema 时才发现）。
