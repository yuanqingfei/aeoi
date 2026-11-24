# AEOI 跨实现一致性套件（v0.2）

规范 9.4（CF-1 – CF-12）的实现。**状态：6 个算子有数值规则（`lyap`、`poles_zeros`、`bode`、`step`、`lqr`、`c2d`，占 19 个算子的 32%）；参考实现（Rust）全部通过；Python 第二实现只做 `lyap`。**

## 内容

| 路径 | 作用 |
|---|---|
| `aeoi_conformance.py` | 运行器。只通过 MCP stdio 驱动被测实现（CF-3），用例直接读 `golden/`（CF-2） |
| `aeoi_hash.py` | 规范 3.7 / 4.3 R-3 / P-2 的独立重写（RFC 8785 JCS、数组摘要、请求指纹）。运行器用它重算指纹，不采信被测实现自报的值 |
| `rules_control.py` | `poles_zeros`、`bode`、`step`、`lqr`、`c2d` 的数值规则（v0.2）；`lyap` 的规则在运行器里 |
| `impls/python_scipy/server.py` | 第二实现：Python + SciPy，只做 `lyap`，线级（MCP stdio） |
| `tests/test_runner_catches_mutants.py` | 证明规则既不太松也不太严：对第二实现注入 7 种缺陷；再经 `proxy_mutant.py` 对参考实现做 8 种改写，错误答案必须失败，等价答案（换极点顺序、相似变换、tf 分子分母同乘常数）必须通过 |
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

另有一个方法选择用例（CF-10 里今天就能跑的部分）：请求 `exec.method_policy.require` 一个不可能存在的方法 id，实现 MUST NOT 成功（返回 `E.VALIDATION.SCHEMA` 或 `E.VALIDATION.METHOD_UNAVAILABLE` 都可以）。

## 结果

参考实现（`rust/target/release/aeoi-mcp`，云容器 x86_64-linux、glibc 2.39）：456 通过 / 0 失败 / 278 跳过。有规则的 6 个算子全部通过：`lyap` 43、`poles_zeros` 47、`bode` 103、`step` 89、`lqr` 88（mpmath 16 + python-control 72）、`c2d` 47；跳过的是其余 13 个算子的成功用例。各检查的最大“偏差/容差”都 ≤ 0.15（`lqr` 的 mpmath 闭环极点），多数在 1e-3 以下。

第二实现（`lyap`）：44 个用例全部通过：mpmath 12 个（独立于 SciPy，含病态的 `continuous_near_axis`、`discrete_rho_near_1`）、python-control 31 个（**与第二实现同源**，只能说明套件可移植，不能说明实现可替换，报告里分开列出）、方法选择 1 个。

## 已知限制

- 6 个算子有数值规则；其余 13 个算子（`convert`、`ctrb_obsv`、`margin`、`impulse`、`place`、`dlqr`、`series`、`parallel`、`feedback`、`pid`、`stepinfo`、`pade`、`loopsens`）的成功用例是 `skip`。
- 新规则只在参考实现上跑过，目标是有一个独立的第二实现来检验（现有的 Python 第二实现只做 `lyap`）。
- 哈希只支持 `json` 编码的稠密数组；`b64` / `ref` / 稀疏数组的请求不重算指纹。
- 没有 CF-10 的逐方法运行：参考实现已实现 `method_policy`（v0.4），但还没有哪个算子在 `receipt.alternatives` 里登记第二个方法。
- 第二实现的 manifest 复用参考实现的 `examples/control.math.lyap/manifest.json`，只替换了方法、后端、构建与精度等级，所以它不是在“只看规范”的条件下写出来的。

## 用这个套件发现的规范问题

1. ~~已解决（规范 v0.4.1，3.9 J-1）：所有 schema 改为标准引用，运行器不再改写，遇到无法解析的引用判失败。~~ 原问题：输出（和输入）schema 里的引用写成 `aeoi://schemas/common/0.4/Matrix`，按标准 JSON Schema 解析不了（应为 `…/0.3#/$defs/Matrix`），3.9 也没有说明这种约定。运行器自己改写后才能校验。
2. `lyap`：input schema 把 Q 描述为“对称”，算法说明却写“Q 非对称也可解”；`W.VALIDATION.SYMMETRIZED` 在 manifest 里声明了，但没有规定什么时候发。第二实现第一版就是因为把非对称 Q 对称化而在 4 个用例上失败。
3. `lyap` 的 `type` 有默认值 `continuous`，golden 里没有一个用例省略它，所以套件不检验默认值。第二实现第一版因此把它当必填也没被发现（读 input schema 时才发现）。
4. （v0.2 规则发现）`bode` 的 `parity.yaml` 按“模 360”比较相位，所以 `branch_center_deg` 定的分支约定（规范 3.x）不被检验：相位整体差 360° 的实现会通过。见规范 12.1 第 22 项。
5. （v0.2 规则发现）`lyap` 的 mpmath 用例，`parity.yaml` 写的是“rel_fro ≤ 1e-12，不放宽”，运行器的 `lyap` 规则对所有引擎都用 T2 × max(1, κ/100)。二者要统一。见规范 12.1 第 23 项。
6. （v0.2 规则发现）CF-6 说每个顶层字段默认按 `rel_fro` 比较，而 `bode`（dB 与度的绝对差）、`step`（相对 max|y_ref| 的最大差）在 `parity.yaml` 里另有写法；规则按 `parity.yaml`。规范 v0.4.2 已写明以 `parity.yaml` 为准。
