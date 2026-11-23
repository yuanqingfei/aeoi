# 设计任务真模型重跑（18 个算子）

## 摘要

在 v0.3.5（18 个算子，含 `series` / `parallel` / `feedback` / `pid` / `stepinfo` / `pade`）之后，用 Gemini 对同一组 7 道 EngDesign 题重跑设计评测。指定模型 `gemini-3.1-flash-lite` 在带齐 18 个工具声明的请求上持续返回 HTTP 503，无法完成任何一轮；改用与试点相同、且能处理该载荷的 `gemini-flash-lite-latest`（temperature 0，`--max-steps 10`，`--rpm 10`，`--call-timeout 150`，`--task-timeout 600`）。完整结果在 `reports/design-live-gemini_gemini-flash-lite-latest.json`。

**0/7 通过，3 道提交了答案，有分数的 3 道平均 62.7。** `eval/design/pyscore.py`（python-control / scipy）对这 3 份提交的通过/不通过判定与 Rust 评分器一致，均为不通过。参考数据来自开源高精度实现，与 MATLAB 无关联；算子命名参考 MATLAB 的公开函数名。

对照试点（`reports/design-pilot.md`，同一模型，当时 12 个算子，XG_02 评分器修正后 0/7、5 道有分、平均 55.6）：通过数没有增加。新算子被大量调用（`pid` 11、`series` 6、`feedback` 4、`stepinfo` 4、`pade` 1），接线错误和「用尽 10 步却不调用 `submit_design`」成了新的主因。有分数的平均分不可直接与试点比较：分母从 5 降到 3，试点里 70 分的 XG_13 和 0 分的 XG_12 这次都没有提交。

评测循环没有「禁用工具 / 无工具基线」开关，本次未跑该对照。

## 跑了什么

| 项 | 内容 |
|---|---|
| 日期 | 2026-11-23，America/Toronto（EST） |
| 仓库 | `main` @ `4831999`（PR #2 已合并）后重新编译的 `aeoi-web` |
| 规范 | 线协议 `aeoi/0.3`，草案 v0.3.5 |
| 可用算子（18） | `bode`、`step`、`impulse`、`convert`、`c2d`、`poles_zeros`、`ctrb_obsv`、`margin`、`lqr`、`dlqr`、`place`、`lyap`、`series`、`parallel`、`feedback`、`pid`、`stepinfo`、`pade` |
| 题目 | 与试点相同的 EngDesign 前 7 题：XG_13、Ziheng_03、XG_05、XG_11、XG_12、XG_02、LX_02（不是 `--engdesign all`，不含 AV_02、AV_03、ZC_02） |
| 成功报告 | `reports/design-live-gemini_gemini-flash-lite-latest.json` |
| 模型 | `gemini-flash-lite-latest`（见下方模型说明） |
| 设置 | temperature 0，`--rpm 10`，`--max-steps 10`，`--call-timeout 150`，`--task-timeout 600` |
| 用量 | 54 个模型回合，1 783 137 token，墙钟 823.0 s（约 13.7 min，含 429/503 退避） |
| 交叉评分 | `pyscore.py` 对 3 份提交均为不通过，与 Rust 一致 |

本次没有跑 ControlEval，也没有把任何无许可证的上游数据写入仓库。

### 为何不是指定的 `gemini-3.1-flash-lite`

按任务先用 `gemini-3.1-flash-lite` 试了两次。不带工具的短探测可以成功；一旦带上 18 个函数声明（请求体约 100 KB），该模型连续返回 HTTP 503（high demand）。评测侧按默认策略重试 4 次（2+4+8+16 s）后记 `provider_error` 并进入下一题，两轮都是题题如此，0 次工具调用。同一次探测里，相同工具载荷发给 `gemini-flash-lite-latest` 立即 HTTP 200。这与试点一致：当时 `gemini-3.1-flash-lite` 也在 XG_13 上 503，完整 7 题是 `gemini-flash-lite-latest` 跑完的。因此本次用后者完成评测，以便和试点同一模型对比算子是否有帮助。未改用 OpenRouter。

成功跑次里仍有短暂 503（4 次）和每分钟 429（12 次，`retryDelay` 约 4–59 s）。它们都被重试成功，没有触发每日配额中止。未把不完整的 3.1 检查点提交进仓库。

## 与试点对照

| | 试点（12 个算子，评分器修正后） | 本次（18 个算子） |
|---|---|---|
| 模型 | `gemini-flash-lite-latest` | 同左（3.1-flash-lite 不可用） |
| 通过 | 0/7 | 0/7 |
| 提交 | 5 | 3 |
| 有分数的平均 | 55.6（5 道） | 62.7（3 道，分母不同，见上） |
| 结束原因 | 提交 5，`max_steps` 2 | 提交 3，`max_steps` 4 |
| 模型回合 / token / 墙钟 | 33 / 723 271 / 402 s | 54 / 1 783 137 / 823 s |
| 工具错误 | 3（schema / range） | 4（`pid` 的 form 冲突与 `Ti=0` schema） |
| 新算子调用 | 无 | `pid` 11，`series` 6，`feedback` 4，`stepinfo` 4，`pade` 1，`parallel` 1 |

工具调用合计（本次）：`margin` 14，`pid` 11，`series` 6，`stepinfo` 4，`feedback` 4，`bode` 3，`poles_zeros` 3，`place` 2，`ctrb_obsv` 1，`convert` 1，`pade` 1，`parallel` 1。自我纠正 2 次（XG_12、XG_02 在 `pid` 参数报错后改对）。

## 逐题

分类：模型推理、接线/组合用法、schema/工具、缺算子、评分器。一道题可以占多类。分数只在提交后才有。

| 题 | 提交 | 结束原因 | 本次分 | 试点分 | 主因 |
|---|---|---|---|---|---|
| XG_13 | 否 | max_steps | — | 70 | 接线 + 模型推理。已会 `pid`→`feedback`→`stepinfo`，但第一次把 C 放进反馈通路（得到 G/(1+CG) 而不是 CG/(1+CG)），又多串了 1/s，闭环不稳定。第二次 P 控制接到了正确的 T(s)，调节时间 0.776 s、超调 7.0%、稳态增益 2/3，本身也不满足指标，且没有提交 |
| Ziheng_03 | 否 | max_steps | — | — | 模型推理 + 接线。会用 `pid`/`series`/`feedback`/`stepinfo`。对象分母写成 `[1, 3.5, 2.5, 0.5]`（正确展开约为 `[1, 3.5, 3.5, 1]`）；第一次还把对象串了两遍。第二次闭环已稳定，但 30 s 内未进入调节带，且把闭环 T 拿去 `margin`。10 步用完未提交 |
| XG_05 | 是 | submitted | 60 | 80 | 模型推理。公式仍在题面：τ = 1.5×(36−23)=19.5，θ=16.5。模型交了 θ=5.14、τ=14.07（辨识 0 分）。调用了 `pade(delay=10, order=10)`，结果没有进入后续串联。控制器部分：超调、GM、PM 成立，调节时间在 2000 s 窗内为 NaN，合计 60 |
| XG_11 | 否 | max_steps | — | — | 模型推理。对象分母仍写成 `[1, 0.26, 0, 0]`，共振项 169 丢掉（与试点相同）。只用了一次 `series`，随后手拼越来越高阶的分母去 `margin`，闭环不稳定警告反复出现，没有提交 |
| XG_12 | 否 | max_steps | — | 0 | schema + 模型推理。对象分母这次是对的 `[1, 0.26, 169, 0]`。前 4 步耗在 `pid` 的 standard/parallel 字段冲突上，第 5 步才成功；之后手乘 L(s) 调增益，相位裕度只有零点几度，10 步未提交 |
| XG_02 | 是 | submitted | 60 | 60（修正后） | 模型推理 + schema。`ctrb_obsv` 对七阶矩阵给出压缩抽样和 `W.EVIDENCE.POSTCONDITION_FAILED`；随后 `convert` 把 ss 转成了 tf（试点没有走到这一步）。`pid` 因多余的 `Ti=0` 报了一次 schema 错，改对后交了 C(s)=0.02。闭环稳定，GM/PM 够，带宽 0 Hz，按细则 60 分。模型在最终文字里声称带宽也满足 |
| LX_02 | 是 | submitted | 68 | 68 | 模型推理；非线性不在算子范围内。`place` 得到的 A、B、K、L 仍对（36 分）。吸引域参考是 1,0,1,0，模型交了 0,0,1,1，对了 s2、s3，合计仍是 68 |

### XG_13（接线 + 模型推理）

对象 `1/(s²+10s+20)`，要求 2% 调节时间 < 0.2 s、超调 < 5%、稳态误差为 0。模型第一次构造 Kp=40、Ki=50、Kd=15，这正是试点所缺的那条链，但 `feedback(sys1=G, sys2=C)` 按现有语义是前向 G、反馈 C，闭环分子是 1 而不是 PID 分子；接着又把该闭环与 1/s 串联，`stepinfo` 报系统不稳定。第二次改为纯比例 Kp=40，在反馈后再乘上 40，得到了正确的 T(s)=40/(s²+10s+60)：稳定，调节时间 0.776 s，超调 7.03%，稳态值 2/3（比例控制消不掉位置误差）。指标本身不够，而且 10 步结束时没有 `submit_design`。试点那次交过一组超调偏大的增益（70 分）；这次算子链会用了，却没有交卷。

### Ziheng_03（模型推理 + 接线）

对象 `1/((0.5s+1)(s+1)(2s+1))`。模型不再连续 9 次只看裕度，而是走 `pid`→`series`→`feedback`→`stepinfo`。对象系数写错，第一次还重复串联，闭环不稳定。第二次 Kp=0.5、Ki=0.02、Kd=0.5 的闭环已经稳定，超调 0，稳态增益 1，但 t_final=30 s 内未进入调节带；同时又把已经闭环的 T(s) 送给 `margin`（开环裕度应作用在 L=C·G 上）。参考增益是 Kp=10、Ki=1.55、Kd=9.5。没有 `pidtune`，10 步用完未提交。

### XG_05（模型推理）

辨识公式不需要新算子，也不需要图。模型没有用 t1、t2 算出 θ=16.5、τ=19.5，而是交了 5.14 和 14.07。`pade` 被调用了，但 delay 用的是 10，且返回的系统没有进入后面的 `series`；`margin` 打在 PI 与 `1/(s+20)` 的串联上，对象和时滞都不在回路里。评分细则：辨识各 10 分，超调 / 调节时间 / GM / PM 各 20 分。本次辨识 0，超调、GM（46.95 dB）、PM（89.07°）成立，调节时间为 NaN，共 60 分。试点是 θ=10、τ=13、控制器 80 分。Padé 仍然解不开辨识分。

### XG_11（模型推理）

对象应是 `(0.5/s)·169/(s²+0.26s+169)`，分母含 169。模型第一次 Bode 就丢掉了这项，之后把错误分母手拼进更高阶多项式再求裕度，反复收到闭环不稳定警告。`series` 只用了一次，而且串联的不是该对象。试点卡在手算和同样的对象错误上；这次有了串联算子，对象仍然写错，也没有提交。

### XG_12（schema + 模型推理）

同类回路成形，带宽 3 rad/s，需要陷波，答案不是 PID。模型先对了对象，然后三次 `pid` 失败：`form=standard` 时夹带 `Ki`/`Kd`，得到 `E.VALIDATION.OPTION_CONFLICT` 和可执行的 JSON Patch；改 `parallel` 后又带了 `Ti=0`，报 `E.VALIDATION.SCHEMA`（「0 is less than or equal to the minimum of 0」）。第四次去掉 `Ti` 才成功。随后手写 L(s) 调增益，`closed_loop_stable` 曾为 true，但相位裕度约 0.2–0.9°，不是题面要的回路成形，10 步未提交。试点交过一个不稳定的 C(s)，0 分。

### XG_02（模型推理 + schema）

七阶 SISO 状态空间，要带宽约 85 Hz、GM > 1.5 dB、PM > 60°。模型先调 `ctrb_obsv`（对设计没有帮助，矩阵被压缩抽样，并带 `W.EVIDENCE.POSTCONDITION_FAILED`），再用 `poles_zeros` 和 `convert` 把 ss 转成 tf——这一点比试点进了一步。`pid` 同样因 `Ti=0` 报了一次 schema 错。最终交 C(s)=0.02（常数增益）。闭环稳定，GM ≈ 31.5 dB、PM 为 Inf，带宽 0 Hz，按细则 60 分，通过为否。最终文字却写「满足带宽、增益裕度和相位裕度」。没有用 `series` 把控制器接到对象上再读带宽。

### LX_02（模型推理；非线性不在范围内）

`place` 两次，A、B、K、L 与参考一致（36 分）。s1..s4 参考 1,0,1,0，模型交 0,0,1,1（试点是 1,1,0,0），对了另外两问，仍是 68 分。线性时不变算子覆盖不了这四问的吸引域仿真。

## 新算子帮到了哪里，以及仍然失败的地方

帮到的：

* 模型会主动调用 `pid`、`series`、`feedback`、`stepinfo`、`pade`，不再把 10 步全部耗在手乘多项式或只看 `margin` 上。Ziheng_03 和 XG_13 已经能做出「构造控制器 → 接回路 → 看调节时间/超调」的形状。
* XG_02 会用已有的 `control_lti_convert` 做 ss→tf（试点在 `ctrb_obsv` 的 schema 错误之后就放弃了）。
* XG_12 的对象分母这次是对的。
* `pid` 的 `OPTION_CONFLICT` 补丁起了作用（XG_12 第 2、3 步之后改了 form），算一次自我纠正。

没有帮到、或新引入的摩擦：

* **组合语义。** `feedback(sys1, sys2)` 的前向/反馈角色、开环 L 与闭环 T 该送给哪个算子，模型经常接错。有了原语之后，错误从「不会乘多项式」变成了「接错回路」。
* **不提交。** 4 道题在已经拿到 `stepinfo`/`margin` 结果之后仍不调用 `submit_design`，步数用尽即无分数。试点只有 2 道未提交。
* **手抄系数。** 尽管 v0.3.5 允许把 `result.system` 原样传入下一次调用，XG_11 等题仍手改分母并改错对象。
* **PID 字段互斥。** `standard` 与 `parallel` 混用、以及在 parallel 下写 `Ti=0`，浪费了 XG_12 和 XG_02 的回合。后一种情况应是「多余字段」，却落成 exclusiveMinimum 的含混报文。
* **辨识和回路成形仍靠模型。** Padé 解不开 XG_05 的 θ、τ；XG_12 需要的也不是 PID。
* **带宽和吸引域。** 与试点判断相同：没有 plant×controller 的带宽读数；LX_02 的非线性仿真不在范围内。
* **报告里的 `expected_blockers` 已过时**（仍写「缺 series/feedback/pid/pade」），这是评测元数据，不是评分错误。

## 评分器

没有新的评分器缺陷需要为此次改代码。3 份提交的通过/不通过与 `pyscore.py` 一致。XG_02 使用的已是试点后的部分分规则（带宽 40 + GM 30 + PM 30），60 分合理。XG_05 的调节时间在细节里写成 `"NaN"`（窗内未给出有限值），该项不得分，不影响另外三项控制器分数。与上游字面标准的差异仍以 `upstream_literal_pass` 和 `eval/design/README.md` 为准；本次没有题目因为这条差异而改变通过结论。

## 这次运行建议的规范改动（只列、不实现）

1. **给组合算子具名角色，或提供 plant+controller 的闭环入口。** 将 `series`/`feedback` 的 `sys1`/`sys2` 改为（或并列）`plant`/`controller`，并写清：`margin` 作用于开环 L，`stepinfo` 作用于闭环 T。或者增加「对象 + 控制器 → 闭环 stepinfo / 开环 margin」的单一入口。本次最常见的失败是把 C 放进反馈通路、把 T 当成 L、以及闭环后再串一个 1/s。
2. **收紧 PID 形式的字段互斥与诊断（F-4）。** 不属于当前 `form` 的字段应报 `E.VALIDATION.OPTION_CONFLICT`（可执行补丁），而不是 `exclusiveMinimum` 下的 “0 is less than or equal to the minimum of 0”。`parallel` 下出现 `Ti`、`standard` 下出现 `Ki`/`Kd` 应同一套规则。
3. **把「优先传递 `result.system`、不要手抄多项式」写进更硬的用法。** v0.3.5 的 F-7 已经允许链式传递，但模型仍手抄并改错对象（XG_11 丢掉 169）。工具描述或规范用法条款需要把这一点说得更强制。
4. **若规范描述 agent 循环，应写清必须提交。** 本次 4/7 在已有可评分的中间结果后不调用 `submit_design`。这首先是评测合同，但如果规范谈工具循环的终止，应要求在步数用尽前给出最终答案。
5. **带宽读数应能直接作用在 plant×controller 上，并标明 Hz 与 rad/s。** 只有 XG_02 需要。模型已经会 `convert`，但仍交了一个带宽为 0 的增益，并在文字里声称满足指标。
6. **`pidtune` 仍建议以后再做，且必须先写 A-4**（声明对象类、指标、唯一性），不能做成没有定义的「自动整定」。XG_13 与 Ziheng_03 把 10 步用在手工试增益上，有合法初值算子才会改变能否在 `max_steps` 内交卷。
7. **大矩阵的压缩表示不能当作已验证的成功证据。** `ctrb_obsv` 对七阶系统返回 `_downsampled` 矩阵，并带 `W.EVIDENCE.POSTCONDITION_FAILED`。规范可明确：设计场景下被抽样的矩阵不是可用证据；工具描述应引导此类题目走 `convert`/`series`，而不是先求可控性矩阵。

未在这次运行中被新证据改变的既有判断：控制器 `c2d`、MIMO 裕度对这 7 道题仍然用不上；LX_02 的非线性吸引域不建议为单题加仿真器。评测元数据 `expected_blockers` 应在实现侧更新，不属于规范正文。
