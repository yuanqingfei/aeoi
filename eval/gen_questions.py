#!/usr/bin/env python3
"""Generates eval/questions.zh.json (the AEOI LLM eval set). Expected values are NOT stored here:
the harness computes them by calling the operator directly with `reference.args`."""
import json, math

def M(rows):
    r = len(rows); c = len(rows[0])
    return {"dtype": "f64", "shape": [r, c], "data": [float(x) for row in rows for x in row]}
def tf(num, den, ts=0.0):
    return {"kind": "tf", "num": num, "den": den, "ts": ts}
def ss(A, B, C, D, ts=0.0):
    return {"kind": "ss", "A": M(A), "B": M(B), "C": M(C), "D": M(D), "ts": ts}
def chk(name, pointer, **kw):
    d = {"name": name, "pointer": pointer}; d.update(kw); return d

Q = []
def q(id, cat, text, tool, args=None, checks=(), **kw):
    d = {"id": id, "category": cat, "question": text, "expected_tool": tool}
    if args is not None:
        d["reference"] = {"tool": tool if isinstance(tool, str) else tool[0], "args": args}
    d["checks"] = list(checks); d.update(kw); Q.append(d)

# ---------------- margin
q("margin-01", "相位裕度", "开环传递函数 G(s) = 4 / (s(s+1)(s+2))。求增益裕度、相位裕度以及对应的相位穿越频率和增益穿越频率。",
  "control_analysis_margin", {"system": tf([4.0], [1, 3, 2, 0])},
  [chk("增益裕度", "/result/gain_margin"), chk("相位裕度 deg", "/result/phase_margin"),
   chk("相位穿越频率", "/result/phase_crossover_frequency"), chk("增益穿越频率", "/result/gain_crossover_frequency")])
q("margin-02", "相位裕度", "单位负反馈系统的开环传递函数 L(s) = 10 / (s(0.1s+1)(0.5s+1))，相位裕度是多少度？增益裕度是多少 dB？",
  "control_analysis_margin", {"system": tf([10.0], [0.05, 0.6, 1, 0])},
  [chk("相位裕度 deg", "/result/phase_margin"), chk("增益裕度 dB", "/result/gain_margin_db")])
q("margin-03", "相位裕度", "G(s) = 2/(s+1)^2 的增益裕度和相位裕度分别是多少？",
  "control_analysis_margin", {"system": tf([2.0], [1, 2, 1])},
  [chk("增益裕度（无穷）", "/result/gain_margin"), chk("相位裕度 deg", "/result/phase_margin")])
# ---------------- step / impulse
q("step-01", "阶跃响应", "G(s) = 4/(s^2 + 1.2 s + 4)，用 dt = 0.01 s 仿真 10 s 的单位阶跃响应，给出超调量、峰值时间和 2% 调节时间。",
  "control_lti_step", {"system": tf([4.0], [1, 1.2, 4]), "time": {"mode": "uniform", "t_final": 10, "dt": 0.01, "unit": "s"}, "step_info": {"enabled": True}},
  [chk("超调 %", "/result/info/overshoot", rel_tol=1e-3), chk("峰值时间 s", "/result/info/peak_time", rel_tol=2e-2),
   chk("调节时间 s", "/result/info/settling_time", rel_tol=2e-2)])
q("step-02", "阶跃响应", "一阶系统 G(s) = 1/(s+1) 的单位阶跃响应，10%–90% 上升时间和稳态值是多少？",
  "control_lti_step", {"system": tf([1.0], [1, 1]), "time": {"mode": "uniform", "t_final": 10, "dt": 0.001, "unit": "s"}, "step_info": {"enabled": True}},
  [chk("上升时间 s", "/result/info/rise_time", rel_tol=2e-2), chk("稳态值", "/result/info/steady_state", rel_tol=1e-3)])
q("step-03", "阶跃响应", "自然频率 ωn = 2 rad/s、阻尼比 ζ = 0.5 的标准二阶系统，单位阶跃响应的超调量是多少？",
  "control_lti_step", {"system": tf([4.0], [1, 2, 4]), "time": {"mode": "uniform", "t_final": 10, "dt": 0.001, "unit": "s"}, "step_info": {"enabled": True}},
  [chk("超调 %", "/result/info/overshoot", rel_tol=2e-2)])
q("impulse-01", "冲激响应", "G(s) = 1/(s^2 + 2s + 5) 的单位冲激响应峰值大约是多少？出现在什么时刻附近？请画出冲激响应。",
  "control_lti_impulse", {"system": tf([1.0], [1, 2, 5]), "time": {"mode": "uniform", "t_final": 6, "dt": 0.001, "unit": "s"}},
  [chk("冲激响应峰值", "/result/y", reduce="max", rel_tol=1e-2)])
# ---------------- poles / zeros
q("pz-01", "零极点", "G(s) = (s+3)/(s^2 + 1.2 s + 4) 的零点、极点、直流增益和极点阻尼比是多少？",
  "control_lti_poles_zeros", {"system": tf([1.0, 3.0], [1, 1.2, 4])},
  [chk("直流增益", "/result/dcgain", index=0), chk("阻尼比", "/result/damping_ratio", index=0),
   chk("零点", "/result/zeros", component="re", index=0), chk("极点实部", "/result/poles", component="re", index=0)])
q("pz-02", "零极点", "状态空间模型 A = [[0, 1], [-2, -3]]，B = [[0], [1]]，C = [[1, 0]]，D = [[0]]。求极点，并判断系统是否稳定。",
  "control_lti_poles_zeros", {"system": ss([[0, 1], [-2, -3]], [[0], [1]], [[1, 0]], [[0]])},
  [chk("极点（最小实部）", "/result/poles", component="re", reduce="min"), chk("极点（最大实部）", "/result/poles", component="re", reduce="max")])
q("pz-03-discrete", "陷阱：离散系统", "离散系统 G(z) = 0.1/(z − 0.9)，采样周期 0.1 s。求它的极点和直流增益。",
  "control_lti_poles_zeros", {"system": tf([0.1], [1, -0.9], ts=0.1)},
  [chk("直流增益（z=1）", "/result/dcgain", index=0), chk("极点", "/result/poles", component="re", index=0)])
# ---------------- ctrb / obsv
q("ctrb-01", "可控性", "A = [[1, 1], [0, 1]]，B = [[0], [1]]。(A, B) 是否完全可控？可控性矩阵的秩是多少？",
  "control_analysis_ctrb_obsv", {"system": ss([[1, 1], [0, 1]], [[0], [1]], [[1, 0]], [[0]])},
  [chk("可控性矩阵秩", "/result/ctrb/rank", rel_tol=0)])
q("ctrb-02", "可控性", "A = diag(1, 2)，B = [[1], [0]]，C = [[1, 1]]。判断可控性与可观性，并给出可控性矩阵的秩。",
  "control_analysis_ctrb_obsv", {"system": ss([[1, 0], [0, 2]], [[1], [0]], [[1, 1]], [[0]])},
  [chk("可控性矩阵秩", "/result/ctrb/rank", rel_tol=0), chk("可观性矩阵秩", "/result/obsv/rank", rel_tol=0)])
q("ctrb-03-colmajor", "陷阱：列优先", "我从 MATLAB 里用 A(:) 导出了 2×2 矩阵 A，按列展开的数据是 [0, 0, 1, 0]；B = [0; 1]。请判断 (A, B) 是否可控并给出可控性矩阵的秩。",
  "control_analysis_ctrb_obsv", {"system": ss([[0, 1], [0, 0]], [[0], [1]], [[1, 0]], [[0]])},
  [chk("可控性矩阵秩", "/result/ctrb/rank", rel_tol=0)])
# ---------------- convert
q("convert-01", "模型转换", "把 G(s) = (s+2)/(s^2 + 3s + 2) 转换成零极点增益（zpk）形式（不要约分）。",
  "control_lti_convert", {"system": tf([1.0, 2.0], [1, 3, 2]), "to": "zpk"},
  [chk("增益 k", "/result/system/k"), chk("最小极点", "/result/system/p", component="re", reduce="min")])
q("convert-02", "模型转换", "把 G(s) = 4/(s^2 + 1.2s + 4) 转成状态空间形式，给出 A、B、C、D。",
  "control_lti_convert", {"system": tf([4.0], [1, 1.2, 4]), "to": "ss"},
  [chk("A 中的 −1.2", "/result/system/A", index=0), chk("A 中的 −4", "/result/system/A", index=1)])
# ---------------- c2d
q("c2d-01", "离散化", "G(s) = 1/(s+1)，采样周期 T = 0.1 s，用零阶保持（ZOH）离散化，给出离散传递函数 G(z)。",
  "control_lti_c2d", {"system": tf([1.0], [1, 1]), "ts": 0.1, "method": "zoh", "output_kind": "tf"},
  [chk("分母 z^0 系数", "/result/system/den", index=1, rel_tol=1e-6), chk("分子系数", "/result/system/num", reduce="maxabs", rel_tol=1e-6)])
q("c2d-02", "离散化", "用 Tustin（双线性）方法、采样周期 0.5 s 把 G(s) = 2/(s+2) 离散化，给出 G(z) 的分子分母系数。",
  "control_lti_c2d", {"system": tf([2.0], [1, 2]), "ts": 0.5, "method": "tustin", "output_kind": "tf"},
  [chk("分母 z^0 系数", "/result/system/den", index=1, rel_tol=1e-6, normalize_by="/result/system/den/0"), chk("分子首项", "/result/system/num", index=0, rel_tol=1e-6, normalize_by="/result/system/den/0")])
q("c2d-03-missing-ts", "陷阱：缺采样周期", "请用零阶保持器把 G(s) = 1/(s^2 + s + 1) 离散化。",
  None, expect="clarify", clarify_keywords=["采样周期", "采样时间", "采样间隔", "采样频率", "ts", "T ="], forbidden_tools=["control_lti_c2d"])
# ---------------- lqr / dlqr
q("lqr-01", "LQR", "双积分器 A = [[0, 1], [0, 0]]，B = [[0], [1]]，Q = I，R = 1，求 LQR 最优反馈增益 K（u = −Kx）。",
  "control_design_lqr", {"A": M([[0, 1], [0, 0]]), "B": M([[0], [1]]), "Q": M([[1, 0], [0, 1]]), "R": M([[1]])},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1)])
q("lqr-02", "LQR", "A = [[0, 1], [-1, -1]]，B = [[0], [1]]，Q = diag(10, 1)，R = 0.1，求连续 LQR 增益 K 和闭环极点。",
  "control_design_lqr", {"A": M([[0, 1], [-1, -1]]), "B": M([[0], [1]]), "Q": M([[10, 0], [0, 1]]), "R": M([[0.1]])},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1)])
q("lqr-03-colmajor", "陷阱：列优先", "MATLAB 里 A 是 2×2 矩阵，A(:)（按列展开）= [0, -2, 1, -3]；B = [0; 1]，Q = eye(2)，R = 1。求 LQR 增益 K。",
  "control_design_lqr", {"A": M([[0, 1], [-2, -3]]), "B": M([[0], [1]]), "Q": M([[1, 0], [0, 1]]), "R": M([[1]])},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1)])
q("lqr-04-unstabilizable", "陷阱：不可镇定", "A = [[1, 0], [0, 2]]，B = [[1], [0]]，Q = I，R = 1，帮我设计 LQR 控制器。",
  "control_design_lqr", {"A": M([[1, 0], [0, 2]]), "B": M([[1], [0]]), "Q": M([[1, 0], [0, 1]]), "R": M([[1]])},
  [], expect="explain_error", expect_error_code="E.DOMAIN.NOT_STABILIZABLE", answer_keywords=["镇定", "不可控", "无法稳定", "不能稳定", "无法使"])
q("dlqr-01", "LQR", "离散系统 x[k+1] = A x[k] + B u[k]，A = [[1, 0.1], [0, 1]]，B = [[0.005], [0.1]]，Q = I，R = 1。求离散 LQR 增益 K。",
  "control_design_dlqr", {"A": M([[1, 0.1], [0, 1]]), "B": M([[0.005], [0.1]]), "Q": M([[1, 0], [0, 1]]), "R": M([[1]])},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1)])
# ---------------- place
q("place-01", "极点配置", "双积分器 A = [[0, 1], [0, 0]]，B = [[0], [1]]，用状态反馈 u = −Kx 把闭环极点配置到 −2 ± 2j，求 K。",
  "control_design_place", {"A": M([[0, 1], [0, 0]]), "B": M([[0], [1]]), "poles": [[-2, 2], [-2, -2]]},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1)])
q("place-02", "极点配置", "A = [[0, 1, 0], [0, 0, 1], [-6, -11, -6]]，B = [[0], [0], [1]]，期望闭环极点 −2、−3、−4，求状态反馈增益 K。",
  "control_design_place", {"A": M([[0, 1, 0], [0, 0, 1], [-6, -11, -6]]), "B": M([[0], [0], [1]]), "poles": [[-2, 0], [-3, 0], [-4, 0]]},
  [chk("K1", "/result/K", index=0), chk("K2", "/result/K", index=1), chk("K3", "/result/K", index=2)])
# ---------------- lyap
q("lyap-01", "Lyapunov", "求解连续 Lyapunov 方程 A X + X Aᵀ + Q = 0，其中 A = [[-1, 2], [0, -3]]，Q = I。",
  "control_math_lyap", {"A": M([[-1, 2], [0, -3]]), "Q": M([[1, 0], [0, 1]]), "type": "continuous"},
  [chk("X11", "/result/X", index=0), chk("X12", "/result/X", index=1), chk("X22", "/result/X", index=3)])
q("lyap-02", "Lyapunov", "离散 Lyapunov 方程 A X Aᵀ − X + Q = 0，A = diag(0.5, 0.8)，Q = I，求 X。",
  "control_math_lyap", {"A": M([[0.5, 0], [0, 0.8]]), "Q": M([[1, 0], [0, 1]]), "type": "discrete"},
  [chk("X11", "/result/X", index=0), chk("X22", "/result/X", index=3)])
# ---------------- bode
q("bode-01", "频率响应", "G(s) = 10/(s+10) 在 ω = 10 rad/s 处的幅值（dB）和相位（度）是多少？顺便画出 0.1–1000 rad/s 的 Bode 图。",
  "control_lti_bode", {"system": tf([10.0], [1, 10]), "frequency": {"mode": "explicit", "values": [10.0], "unit": "rad/s"}},
  [chk("幅值 dB", "/result/magnitude", at_w=10.0), chk("相位 deg", "/result/phase", at_w=10.0, allow_sign_flip=True)])
wc = 2 * math.pi * 100
q("bode-02-hz", "陷阱：Hz 与 rad/s", f"一阶低通滤波器 G(s) = {wc:.4f}/(s + {wc:.4f})（截止频率 100 Hz）。求 f = 100 Hz 处的幅值（dB）和相位。",
  "control_lti_bode", {"system": tf([wc], [1, wc]), "frequency": {"mode": "explicit", "values": [100.0], "unit": "Hz"}},
  [chk("幅值 dB", "/result/magnitude", at_w=wc), chk("相位 deg", "/result/phase", at_w=wc, allow_sign_flip=True)])

json.dump({"version": 1, "notes": "期望值由 reference.args 直接调用算子得到（python-control 校验过的 AEOI 实现），不是手抄常数。", "questions": Q},
          open("questions.zh.json", "w"), ensure_ascii=False, indent=1)
print(len(Q), "questions")
