#!/usr/bin/env python3
"""Target operator list (spec 12.3) and its dependency graph.

Single source for: which ~100 MATLAB functions AEOI targets, which AEOI operator
covers each, which shared kernels (aeoi-core::kernels and friends) each one
needs, and in which order they get built (bottom-up by dependency layer).

Run:  python3 roadmap/targets.py      -> writes roadmap/layers.md and roadmap/deps.mmd
Fails (exit 1) on unknown dependencies, cycles, or a "done" item that depends
on a kernel that is not done.

Rule (spec 11.5 RI-1): an operator never re-implements another operator's
numerics; it depends on the kernel. Where today's code keeps a reusable piece
inside one operator, the kernel is listed with status "extract" and the
operator that holds it in `src`.
"""
import os, sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------
# Kernels.  status: done | extract (exists inside an operator, must move to a
# kernel before a second user appears) | new
# (id, status, deps, where / what)
KERNELS = [
    ("libm",     "done",    [],                       "aeoi-libm：C 数学符号转给编入的 Rust libm"),
    ("sum",      "done",    [],                       "kernels::sum：TwoSum、TwoProduct、Dot2、Neumaier 补偿求和"),
    ("dense",    "done",    ["sum"],                  "kernels::dense：LU、QR、Cholesky、范数、1-范数逆估计、双倍精度残差"),
    ("cdense",   "done",    ["dense"],                "kernels::cdense：复数 LU、复数奇异值、复数 QR"),
    ("svd",      "done",    ["dense"],                "kernels::svd：单边 Jacobi SVD"),
    ("schur",    "done",    ["dense"],                "kernels::schur：Hessenberg + Francis 双位移 QR"),
    ("eig",      "done",    ["schur", "cdense"],      "kernels::eig：对称 Jacobi、复 Schur、一般特征向量"),
    ("fft",      "done",    ["libm"],                 "kernels::fft：基 2 / 混合基 / Bluestein"),
    ("expr",     "done",    ["libm"],                 "aeoi-numeric::expr：表达式求值与自动微分"),
    ("poly",     "done",    ["eig", "sum"],           "aeoi-core::poly：Horner 与补偿 Horner、乘法、求导、友矩阵求根、包含半径"),
    ("filter",   "done",    [],                       "kernels::filter：直接 II 转置型、SOS 级联"),
    ("lp",       "done",    ["dense"],                "kernels::lp：单纯形"),
    ("qp",       "done",    ["lp", "dense"],          "kernels::qp：有效集 QP"),
    ("expm",     "done",    ["dense"],                "linalg::expm：Padé 缩放平方"),
    ("logm",     "new",     ["schur", "expm"],        "矩阵对数（逆缩放平方），d2c 需要"),
    ("lti",      "done",    ["poly", "eig", "dense"], "aeoi-core lti/sysalg：tf/ss/zpk 表示与互转、不变零点"),
    ("lyap",     "done",    ["eig", "cdense", "sum"], "sysalg：Bartels–Stewart Lyapunov / Stein，双倍精度残差精化"),
    ("riccati",  "done",    ["schur", "svd", "lyap"], "kernels::riccati：CARE 符号函数 + Newton–Kleinman，DARE 的 SDA + Newton–Hewer"),
    ("freqresp", "done",    ["lti", "cdense"],        "kernels::freqresp：jω / e^{jωT} 求值、Horner 误差界、zpk、预解式、相位约定"),
    ("timeresp", "done",    ["lti", "expm"],          "kernels::timeresp：零阶/一阶保持离散化（增广 expm）与递推"),
    ("bracket",  "done",    [],                       "kernels::bracket：Brent zeroin（fminbnd 时加 Brent 极小）"),
    ("newton",   "done",    ["dense"],                "kernels::newton：Powell dogleg 信赖域、后验 Newton 修正与 κ₁(J)"),
    ("quad",     "done",    ["sum"],                  "kernels::quad：GK(7,15)、全局自适应二分、MATLAB integral 的变量替换"),
    ("rk",       "done",    [],                       "kernels::rk：Dormand–Prince 5(4)（同 scipy RK45）、稠密输出、刚性检测"),
    ("spline",   "done",    ["dense"],                "kernels::spline：pchip / not-a-knot 斜率、dgtsv、PPoly 系数"),
    ("iir",      "done",    ["poly"],                 "kernels::iir：Butterworth 原型、频率变换、双线性变换、zpk→tf/sos、设计流程"),
    ("lsq",      "new",     ["dense", "svd"], "非线性最小二乘：Levenberg–Marquardt / 信赖域反射"),
    ("quasi",    "new",     ["dense"],        "拟 Newton（BFGS）与线搜索"),
    ("stiff",    "new",     ["newton", "dense"],      "刚性 ODE：NDF / BDF，带 Jacobian 复用"),
    ("bvp",      "new",     ["newton", "dense"],      "边值问题：三阶段 Lobatto 配置"),
    ("window",   "new",     ["libm"],                 "窗函数、分段平均（Welch）"),
    ("stats",    "new",     ["sum"],                  "补偿矩、排序与选择、分位数插值约定"),
    ("delaunay", "new",     [],                       "二维 Delaunay 三角剖分（稳健谓词）"),
]

# ---------------------------------------------------------------------------
# Targets: one row per MATLAB function (aliases after "/").
# (category, matlab, aeoi operator id, status done|todo, deps)
# deps may name kernels or other MATLAB rows (prefix "op:").
T = []
def add(cat, rows):
    for r in rows:
        T.append((cat,) + r)

add("控制", [
    ("tf",        "control.lti.convert",       "done", ["lti"]),
    ("ss",        "control.lti.convert",       "done", ["lti"]),
    ("zpk",       "control.lti.convert",       "done", ["lti"]),
    ("step",      "control.lti.step",          "done", ["timeresp"]),
    ("impulse",   "control.lti.impulse",       "done", ["timeresp"]),
    ("lsim",      "control.lti.lsim",          "done", ["timeresp"]),
    ("bode",      "control.lti.bode",          "done", ["freqresp"]),
    ("nyquist",   "control.lti.nyquist",       "todo", ["freqresp"]),
    ("nichols",   "control.lti.nichols",       "todo", ["freqresp"]),
    ("margin",    "control.analysis.margin",   "done", ["freqresp", "poly"]),
    ("rlocus",    "control.analysis.rlocus",   "todo", ["poly", "lti"]),
    ("pole",      "control.lti.poles_zeros",   "done", ["lti"]),
    ("zero",      "control.lti.poles_zeros",   "done", ["lti"]),
    ("feedback",  "control.lti.feedback",      "done", ["lti"]),
    ("series",    "control.lti.series",        "done", ["lti"]),
    ("parallel",  "control.lti.parallel",      "done", ["lti"]),
    ("dcgain",    "control.lti.dcgain",        "todo", ["lti", "freqresp"]),
    ("c2d",       "control.lti.c2d",           "done", ["expm", "lti"]),
    ("d2c",       "control.lti.d2c",           "todo", ["logm", "lti"]),
    ("pade",      "control.lti.pade",          "done", ["lti", "dense"]),
    ("minreal",   "control.lti.minreal",       "todo", ["lti", "svd"]),
    ("balreal",   "control.lti.balreal",       "todo", ["lyap", "svd"]),
    ("ctrb/obsv", "control.analysis.ctrb_obsv","done", ["svd"]),
    ("lyap",      "control.math.lyap",         "done", ["lyap"]),
    ("dlyap",     "control.math.dlyap",        "todo", ["lyap"]),
    ("care/icare","control.math.care",         "todo", ["riccati"]),
    ("dare/idare","control.math.dare",         "todo", ["riccati"]),
    ("lqr",       "control.design.lqr",        "done", ["riccati"]),
    ("dlqr",      "control.design.dlqr",       "done", ["riccati"]),
    ("kalman/lqe","control.design.kalman",     "todo", ["riccati"]),
    ("place",     "control.design.place",      "done", ["cdense", "svd", "eig"]),
    ("pid",       "control.design.pid",        "done", ["lti"]),
    ("stepinfo",  "control.lti.stepinfo",      "done", ["op:step"]),
    ("pidtune",   "control.design.pidtune",    "done", ["op:margin", "op:pid"]),
    ("loopsens",  "control.analysis.loopsens", "done", ["op:feedback", "op:margin", "op:stepinfo"]),
])
add("线性代数", [
    ("mldivide/linsolve", "linalg.system.solve", "done", ["dense"]),
    ("eig",       "linalg.decomp.eig",         "done", ["eig"]),
    ("svd",       "linalg.decomp.svd",         "done", ["svd"]),
    ("lu",        "linalg.decomp.lu",          "todo", ["dense"]),
    ("qr",        "linalg.decomp.qr",          "todo", ["dense"]),
    ("chol",      "linalg.decomp.chol",        "todo", ["dense"]),
    ("schur",     "linalg.decomp.schur",       "todo", ["schur"]),
    ("inv",       "linalg.matrix.inv",         "todo", ["dense"]),
    ("det",       "linalg.matrix.det",         "todo", ["dense"]),
    ("norm/cond", "linalg.matrix.norm",        "todo", ["svd", "dense"]),
    ("rank",      "linalg.subspace.rank",      "todo", ["svd"]),
    ("null/orth", "linalg.subspace.basis",     "todo", ["svd"]),
    ("pinv",      "linalg.matrix.pinv",        "todo", ["svd"]),
    ("lsqminnorm","linalg.system.lstsq",       "todo", ["dense", "svd"]),
    ("expm",      "linalg.func.expm",          "todo", ["expm"]),
])
add("信号", [
    ("fft/ifft",  "signal.spectral.fft",       "done", ["fft"]),
    ("filter",    "signal.filter.filter",      "done", ["filter"]),
    ("butter",    "signal.design.butter",      "done", ["iir"]),
    ("cheby1/cheby2", "signal.design.cheby",   "todo", ["iir"]),
    ("filtfilt",  "signal.filter.filtfilt",    "todo", ["filter"]),
    ("freqz",     "signal.design.freqz",       "todo", ["poly", "fft"]),
    ("conv",      "signal.filter.conv",        "todo", ["fft", "sum"]),
    ("xcorr",     "signal.spectral.xcorr",     "todo", ["fft"]),
    ("pwelch",    "signal.spectral.pwelch",    "todo", ["window", "fft"]),
    ("spectrogram","signal.spectral.spectrogram","todo", ["window", "fft"]),
    ("hilbert",   "signal.spectral.hilbert",   "todo", ["fft"]),
    ("fir1",      "signal.design.fir1",        "todo", ["window"]),
    ("resample",  "signal.rate.resample",      "todo", ["op:fir1", "filter"]),
    ("decimate",  "signal.rate.decimate",      "todo", ["iir", "op:filtfilt"]),
    ("findpeaks", "signal.measure.findpeaks",  "todo", []),
])
add("多项式、插值、拟合", [
    ("polyfit",   "fit.poly.polyfit",          "done", ["dense", "svd"]),
    ("polyval",   "fit.poly.polyval",          "done", ["poly", "sum"]),
    ("roots",     "fit.poly.roots",            "done", ["poly"]),
    ("deconv",    "fit.poly.deconv",           "todo", ["poly"]),
    ("interp1",   "interp.grid.interp1",       "done", ["spline"]),
    ("spline/pchip","interp.grid.spline",      "todo", ["spline"]),
    ("interp2",   "interp.grid.interp2",       "todo", ["spline"]),
    ("griddata",  "interp.scattered.griddata", "todo", ["delaunay"]),
    ("lsqcurvefit","fit.nonlinear.lsqcurvefit","todo", ["lsq", "expr"]),
])
add("微积分与 ODE", [
    ("ode45",     "ode.ivp.solve",             "done", ["rk", "expr"]),
    ("ode23",     "ode.ivp.solve（method）",   "todo", ["rk", "expr"]),
    ("ode15s",    "ode.ivp.solve（stiff）",    "todo", ["stiff", "expr"]),
    ("integral",  "calculus.quadrature.integral","done", ["quad", "expr"]),
    ("integral2", "calculus.quadrature.integral2","todo", ["quad", "expr"]),
    ("trapz",     "calculus.quadrature.trapz", "done", ["sum"]),
    ("cumtrapz",  "calculus.quadrature.trapz", "done", ["sum"]),
    ("gradient",  "calculus.diff.gradient",    "todo", []),
    ("bvp4c",     "ode.bvp.solve",             "todo", ["bvp", "expr"]),
    ("pdepe",     "pde.parabolic.pdepe",       "todo", ["stiff"]),
])
add("非线性方程与优化", [
    ("fzero",     "nonlinear.root.fzero",      "done", ["bracket", "expr"]),
    ("fsolve",    "nonlinear.system.fsolve",   "done", ["newton", "expr"]),
    ("fminbnd",   "optim.scalar.fminbnd",      "todo", ["bracket", "expr"]),
    ("fminsearch","optim.local.fminsearch",    "done", ["expr"]),
    ("fminunc",   "optim.local.fminunc",       "todo", ["quasi", "expr"]),
    ("fmincon",   "optim.local.fmincon",       "done", ["qp", "expr"]),
    ("linprog",   "optim.linear.linprog",      "done", ["lp"]),
    ("quadprog",  "optim.quadratic.quadprog",  "todo", ["qp"]),
    ("lsqnonlin", "optim.lsq.lsqnonlin",       "todo", ["lsq", "expr"]),
    ("lsqlin",    "optim.lsq.lsqlin",          "todo", ["qp", "dense"]),
])
add("统计", [
    ("mean",      "stats.describe.moments",    "todo", ["stats"]),
    ("std/var",   "stats.describe.moments",    "todo", ["stats"]),
    ("median",    "stats.describe.quantile",   "todo", ["stats"]),
    ("quantile/prctile","stats.describe.quantile","todo", ["stats"]),
    ("cov/corrcoef","stats.describe.cov",      "todo", ["stats", "dense"]),
    ("histcounts","stats.describe.histcounts", "todo", ["stats"]),
    ("regress",   "stats.regression.regress",  "todo", ["dense", "svd"]),
    ("fitlm",     "stats.regression.fitlm",    "todo", ["op:regress"]),
])


def main():
    kid = {k[0]: k for k in KERNELS}
    rows = {t[1]: t for t in T}
    assert len(rows) == len(T), "duplicate MATLAB names"
    errs = []
    deps = {}
    for k, st, d, _ in KERNELS:
        deps["k:" + k] = ["k:" + x for x in d]
    for cat, m, op, st, d in T:
        deps["m:" + m] = [("m:" + x[3:]) if x.startswith("op:") else "k:" + x for x in d]
    for n, ds in deps.items():
        for x in ds:
            if x not in deps:
                errs.append(f"{n}: unknown dependency {x}")
    layer = {}
    def L(n, stack=()):
        if n in layer:
            return layer[n]
        if n in stack:
            errs.append("cycle: " + " -> ".join(stack + (n,)))
            return 0
        v = 0 if not deps[n] else 1 + max(L(x, stack + (n,)) for x in deps[n] if x in deps)
        layer[n] = v
        return v
    for n in deps:
        L(n)
    status = {"k:" + k: st for k, st, _, _ in KERNELS}
    status.update({"m:" + m: st for _, m, _, st, _ in T})
    for n, ds in deps.items():
        if status[n] == "done":
            for x in ds:
                if status.get(x) == "new":
                    errs.append(f"{n} is done but depends on new kernel {x}")
    if errs:
        print("\n".join(errs)); sys.exit(1)

    # ---- markdown --------------------------------------------------------
    done = sum(1 for t in T if t[3] == "done")
    out = []
    out.append(f"目标共 {len(T)} 个 MATLAB 函数，已覆盖 {done} 个（{round(100*done/len(T))}%），"
               f"由 {len({t[2] for t in T if t[3]=='done'})} 个 AEOI 算子提供。"
               f"共享内核 {len(KERNELS)} 个：已有 {sum(k[1]=='done' for k in KERNELS)}，"
               f"待从算子中抽出 {sum(k[1]=='extract' for k in KERNELS)}，待新写 {sum(k[1]=='new' for k in KERNELS)}。"
               "层号 = 1 + 依赖中最大的层号（无依赖为 0）；由 `roadmap/targets.py` 生成，改清单只改那个文件。\n")
    out.append("**共享内核**\n")
    out.append("| 层 | 内核 | 状态 | 依赖 | 内容 |")
    out.append("|---|---|---|---|---|")
    ST = {"done": "已有", "extract": "待抽出", "new": "待新写", "todo": "待做"}
    for k, st, d, what in sorted(KERNELS, key=lambda k: (layer["k:" + k[0]], k[0])):
        out.append(f"| {layer['k:'+k]} | `{k}` | {ST[st]} | {', '.join('`'+x+'`' for x in d) or '—'} | {what} |")
    out.append("")
    out.append("**目标函数**（同一层内的先后以后按 MATLAB 使用频率排）\n")
    out.append("| 层 | 类别 | MATLAB | AEOI 算子 | 状态 | 依赖 |")
    out.append("|---|---|---|---|---|---|")
    for cat, m, op, st, d in sorted(T, key=lambda t: (layer["m:" + t[1]], t[3] != "done", t[0])):
        dd = ", ".join(("**" + x[3:] + "**") if x.startswith("op:") else "`" + x + "`" for x in d) or "—"
        out.append(f"| {layer['m:'+m]} | {cat} | {m} | `{op}` | {ST[st]} | {dd} |")
    out.append("")
    cats = defaultdict(lambda: [0, 0])
    for cat, m, op, st, d in T:
        cats[cat][0] += 1; cats[cat][1] += st == "done"
    out.append("| 类别 | 目标 | 已覆盖 |")
    out.append("|---|---|---|")
    for c, (a, b) in cats.items():
        out.append(f"| {c} | {a} | {b} |")
    out.append(f"| 合计 | {len(T)} | {done}（{round(100*done/len(T))}%） |")
    open(os.path.join(HERE, "layers.md"), "w").write("\n".join(out) + "\n")
    # short summary for spec 12.3 (the full tables stay in layers.md)
    summ = [out[0], "| 类别 | 目标 | 已覆盖 |", "|---|---|---|"]
    for c, (a, b) in cats.items():
        summ.append(f"| {c} | {a} | {b} |")
    summ.append(f"| 合计 | {len(T)} | {done}（{round(100*done/len(T))}%） |")
    open(os.path.join(HERE, "summary.md"), "w").write("\n".join(summ) + "\n")

    # ---- mermaid (kernel graph + operator counts per kernel) ------------
    mm = ["graph BT"]
    for k, st, d, _ in KERNELS:
        mm.append(f'  {k}["{k}"]:::{st}')
        for x in d:
            mm.append(f"  {k} --> {x}")
    mm.append("  classDef done fill:#d4edda,stroke:#2e7d32")
    mm.append("  classDef extract fill:#fff3cd,stroke:#b8860b")
    mm.append("  classDef new fill:#f8d7da,stroke:#b71c1c")
    open(os.path.join(HERE, "deps.mmd"), "w").write("\n".join(mm) + "\n")
    print(f"{len(T)} targets, {done} done; {len(KERNELS)} kernels; max layer {max(layer.values())}")


if __name__ == "__main__":
    main()
