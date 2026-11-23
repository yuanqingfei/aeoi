#!/usr/bin/env python3
"""Summarise analyze.py output into reports/parity-tpt-control.json and print the markdown tables used in
reports/parity-tpt-control.md. usage: make_report.py analysis.json proto.json xcheck.jsonl out.json > tables.md"""
import json, sys, math, statistics, collections, subprocess, os

an, proto, raw, dst = sys.argv[1:5]
A = json.load(open(an))
P = json.load(open(proto))
cases = A["cases"]
here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.abspath(os.path.join(here, "../.."))

def git(*a, cwd=repo):
    return subprocess.run(["git", *a], cwd=cwd, capture_output=True, text=True).stdout.strip()

TPT_REV = "10e432068e094bc37977bfcc5c96bac3f6ef3641"
OVERLAP = [
    # operator, tpt equivalent, coverage in this run, convention notes
    ("control.math.lyap", "tpt_ctrl_core::lyapunov::{solve_lyapunov, solve_discrete_lyapunov}", "full",
     "AEOI: A X + X Aᵀ + Q = 0 / A X Aᵀ − X + Q = 0；tpt: Aᵀ X + X A = −Q / Aᵀ X A − X = −Q → 传 Aᵀ。tpt 离散用平方/倍增级数（ρ(A) ≥ 1 时 NoConvergence），连续用实 Schur Bartels–Stewart"),
    ("control.design.lqr", "tpt_ctrl_optimal::{care, lqr}", "full（N 交叉项经标准约化）",
     "同一 CARE 约定 AᵀX + XA − XBR⁻¹BᵀX + Q = 0，K = R⁻¹BᵀX，u = −Kx。tpt 无 N：Ā = A − BR⁻¹Nᵀ、Q̄ = Q − NR⁻¹Nᵀ，K = R⁻¹(BᵀX + Nᵀ)。tpt 用 DARE(ZOH) 求初始镇定增益 + Newton–Kleinman；AEOI 用矩阵符号函数 + 最多 3 步 Newton–Kleinman"),
    ("control.design.dlqr", "tpt_ctrl_optimal::{dare, dlqr}", "full（N 同上）",
     "同一 DARE 约定，K = (R + BᵀXB)⁻¹BᵀXA。tpt 用结构保持倍增（SDA）"),
    ("control.lti.c2d", "tpt_ctrl_core::discretize::{zoh, tustin}", "ss 输入、zoh/tustin、无 prewarp",
     "两边 Tustin 都是 scipy 约定（Cd = C(I − T/2·A)⁻¹，Dd = D + ½·C·Bd）。tpt 无 foh、无 prewarp、不处理 tf 输入"),
    ("control.lti.bode", "tpt_ctrl_classic::freq::bode_data", "连续 SISO tf",
     "两边都是 dB / 度 / rad/s；相位都从第一个频点起展开。tpt 无 ss、zpk、离散、MIMO"),
    ("control.analysis.margin", "tpt_ctrl_classic::freq::margins", "连续 SISO tf",
     "tpt 增益裕度为线性倍数、相位裕度为度；在对数网格（本次 20000 点）上线性插值，只取第一个交越；AEOI 解析求所有交越取最小。tpt `Margins` 的两个频率字段的文档注释写反了（见 bugs）"),
    ("control.lti.poles_zeros", "TransferFunction::{poles, zeros}；analysis::poles", "tf（极点+零点）、ss（仅极点）",
     "tpt 对 tf 首项归一化，零点为分子根；ss 无不变零点"),
    ("control.lti.step", "discretize::zoh + simulate::simulate_discrete（组合，非 tpt 现成算子）", "ss、SISO tf（经 tpt tf.to_state_space）",
     "均匀网格上 ZOH 对阶跃输入是精确的；离散系统直接递推"),
    ("control.lti.impulse", "同上（组合）", "同上",
     "按 python-control 约定：连续 x0 = B、忽略 D；离散 u[0] = 1/dt"),
    ("control.analysis.ctrb_obsv", "analysis::{controllability_matrix, observability_matrix, rank}；Gramian 用 tpt lyapunov", "ss（含离散）",
     "秩：tpt 用 QR 对角元相对阈值（本次 1e-10），AEOI 用 SVD + PBH 交叉检查"),
    ("control.lti.convert", "TransferFunction::to_state_space", "仅 SISO tf → ss",
     "实现不同（tpt 相变量形式，AEOI/scipy 控制器形式），比较实现无关的 Markov 参数 D, CB, CAB, …；ss → tf、zpk 无对应"),
    ("control.design.place", "无（tpt 没有极点配置）", "只用 tpt 的特征值求解器复核 AEOI 的 K",
     "比较 eig(A − BK)（tpt 求）与目标极点、与 AEOI 自报的 achieved_poles"),
]

def q(xs, f):
    xs = [x for x in xs if x is not None and not (isinstance(x, float) and math.isnan(x))]
    if not xs:
        return None
    return f(xs)

def mx(xs):
    return q(xs, max)

def med(xs):
    return q(xs, statistics.median)

def fmt(x):
    return "—" if x is None else ("%.1e" % x if x != 0 else "0")

# ---------------------------------------------------------------- direct comparison per operator
per_op = {}
for op in ["lyap", "lqr", "dlqr", "c2d", "bode", "margin", "poles_zeros", "step", "impulse", "ctrb_obsv", "convert", "place"]:
    cs = [c for c in cases if c["operator"] == op]
    comp = [c for c in cs if c.get("diff") or c.get("margin")]
    fields = collections.defaultdict(list)
    for c in comp:
        for k, v in (c.get("diff") or {}).items():
            if isinstance(v, dict) and "rel" in v:
                fields[k].append(v["rel"])
            elif isinstance(v, (int, float)) and not isinstance(v, bool) and k.endswith(("abs", "mod360")):
                fields[k].append(v)
        if c.get("margin"):
            fields["gain_margin_rel"].append(c["margin"]["gm_rel"])
            fields["phase_margin_abs_deg"].append(c["margin"]["pm_abs_deg"])
    both_err = sum(1 for c in cs if c["aeoi_status"] == "error" and c["tpt_error"])
    per_op[op] = {"cases": len(cs), "compared": len(comp), "skipped": sum(1 for c in cs if c.get("skip")),
                  "both_error": both_err,
                  "fields": {k: {"max": mx(v), "median": med(v)} for k, v in fields.items()},
                  "us_median": {"aeoi": med([c["us"]["aeoi"] for c in comp]), "tpt": med([c["us"]["tpt"] for c in comp if c["us"]["tpt"]])}}

# ---------------------------------------------------------------- accuracy vs truth (lyap/lqr/dlqr) per family
INVALID = ("E.VALIDATION.", "E.DOMAIN.R_NOT_POSITIVE_DEFINITE")
def invalid_input(c):
    return (c["aeoi_error"] or "").startswith(INVALID)
invalid_cases = [{"case": c["case"], "operator": c["operator"], "aeoi": c["aeoi_error"], "tpt": c["tpt_error"] or "ok (no input check)",
                  "scipy": c.get("scipy_error") or ("ok" if "scipy" in (c.get("residual") or {}) else None)}
                 for c in cases if c["operator"] in ("lyap", "lqr", "dlqr") and invalid_input(c)]
fams = collections.defaultdict(list)
for c in cases:
    if c["operator"] in ("lyap", "lqr", "dlqr") and c.get("residual") and not invalid_input(c):
        fams[(c["operator"], c["family"])].append(c)
acc = []
for (op, fam), cs in sorted(fams.items()):
    row = {"operator": op, "family": fam, "cases": len(cs)}
    for impl in ("aeoi", "tpt", "scipy"):
        row[impl] = {"fwd_X_max": mx([c.get("forward_error_X", {}).get(impl) for c in cs]),
                     "fwd_X_median": med([c.get("forward_error_X", {}).get(impl) for c in cs]),
                     "fwd_K_max": mx([c.get("forward_error_K", {}).get(impl) for c in cs]),
                     "residual_max": mx([c["residual"].get(impl) for c in cs]),
                     "solved": sum(1 for c in cs if impl in c["residual"]),
                     "non_stabilizing": sum(1 for c in cs if c.get("stabilizing", {}).get(impl) is False)}
    wins = collections.Counter()
    for c in cs:
        fe = c.get("forward_error_X")
        if fe and len(fe) > 1:
            wins[min(fe, key=fe.get)] += 1
    row["most_accurate_count"] = dict(wins)
    row["truth_cases"] = sum(1 for c in cs if c.get("forward_error_X"))
    conds = [c.get("lyap_operator_cond2") for c in cs]
    if any(conds):
        row["lyap_operator_cond2_max"] = mx(conds)
    acc.append(row)

# ---------------------------------------------------------------- error-case disagreements
dis = []
for c in cases:
    if c.get("skip") or c["operator"] not in ("lyap", "lqr", "dlqr", "c2d", "margin") or invalid_input(c):
        continue
    ae = c["aeoi_status"] == "error"
    te = c["tpt_error"] is not None
    if ae != te:
        dis.append({"case": c["case"], "operator": c["operator"], "aeoi": c["aeoi_error"] or c["aeoi_status"], "tpt": c["tpt_error"] or "ok",
                    "tpt_stabilizing": c.get("stabilizing", {}).get("tpt"), "scipy": c.get("scipy_error") or ("ok" if "scipy" in c.get("residual", {}) else None),
                    "truth_stabilizing": c.get("truth_stabilizing")})

out = {
    "generated_by": "tools/xcheck-tpt (xcheck-tpt → analyze.py → make_report.py)",
    "tpt_control": {"repo": "https://github.com/tpt-solutions/tpt-control", "commit": TPT_REV, "commit_date": "2026-10-16",
                    "license": "MIT OR Apache-2.0", "crates": ["tpt-ctrl-core", "tpt-ctrl-classic", "tpt-ctrl-optimal"],
                    "toolchain": "rust-toolchain.toml pins 1.97.1 (workspace rust-version 1.85); its 111 core/classic/optimal tests pass on 1.97.1; the harness builds it with 1.85.1"},
    "aeoi_commit": git("rev-parse", "HEAD"),
    "truth": {"method": "mpmath 50 digits: Kronecker solve (Lyapunov); Newton–Kleinman / Hewer refinement of the N-reduced Riccati equation from the best f64 candidate (n ≤ %d); residuals of every candidate evaluated in 50 digits" % 10,
              "scipy": "corpus: stored python-control 0.10.2 / scipy 1.18.1 golden values; random: scipy %s computed by analyze.py" % __import__("scipy").__version__,
              "forward_error": "max|X − X*| / max|X*|", "residual": "‖Res(X)‖_F / Σ‖terms‖_F"},
    "corpus": {"cases": sum(1 for c in cases if c["source"] == "corpus"), "random": sum(1 for c in cases if c["source"] == "random"), "repro": sum(1 for c in cases if c["source"] == "repro")},
    "overlap": [{"operator": o, "tpt": t, "coverage": cov, "conventions": n} for o, t, cov, n in OVERLAP],
    "direct_comparison": per_op,
    "accuracy_vs_truth": acc,
    "error_disagreements": dis,
    "invalid_inputs_excluded_from_accuracy": invalid_cases,
    "care_kform_newton_prototype": P,
    "cases": cases,
}
json.dump(out, open(dst, "w"), ensure_ascii=False, indent=1, default=float)

# ---------------------------------------------------------------- markdown tables
print("### 直接比较（AEOI vs tpt）\n")
print("| 算子 | 用例 | 比较 | 跳过 | 两边都报错 | 字段：最大 / 中位差 | 中位耗时 AEOI / tpt (µs) |")
print("|---|---|---|---|---|---|---|")
for op, d in per_op.items():
    fl = "; ".join(f"{k} {fmt(v['max'])} / {fmt(v['median'])}" for k, v in d["fields"].items())
    print(f"| {op} | {d['cases']} | {d['compared']} | {d['skipped']} | {d['both_error']} | {fl} | {fmt(d['us_median']['aeoi'])} / {fmt(d['us_median']['tpt'])} |")
print("\n### 与高精度真值的距离（前向误差 max|X−X*|/max|X*|，最大值；括号内为 50 位精度下的相对残差最大值）\n")
print("| 算子 | 族 | 用例 | 有真值 | AEOI | tpt | scipy | 最准（次数） |")
print("|---|---|---|---|---|---|---|---|")
for r in acc:
    cell = lambda i: ("未求出" if r[i]["solved"] == 0 else f"{fmt(r[i]['fwd_X_max'])} ({fmt(r[i]['residual_max'])})" + (f" · 非镇定 {r[i]['non_stabilizing']}" if r[i]["non_stabilizing"] else "") + ("" if r[i]["solved"] == r["cases"] else f" · 求出 {r[i]['solved']}/{r['cases']}"))
    w = ", ".join(f"{k} {v}" for k, v in sorted(r["most_accurate_count"].items(), key=lambda kv: -kv[1]))
    print(f"| {r['operator']} | {r['family']} | {r['cases']} | {r['truth_cases']} | {cell('aeoi')} | {cell('tpt')} | {cell('scipy')} | {w} |")
print("\n### 报错/失败行为不一致\n")
print("| 用例 | 算子 | AEOI | tpt | tpt 解镇定? | scipy | 真值镇定? |")
print("|---|---|---|---|---|---|---|")
for d in dis:
    print(f"| `{d['case'][-48:]}` | {d['operator']} | {d['aeoi']} | {d['tpt']} | {d['tpt_stabilizing']} | {d['scipy']} | {d['truth_stabilizing']} |")
print("\n### 非法输入（AEOI 拒绝，不计入精度表）\n")
print("| 用例 | 算子 | AEOI | tpt | scipy |")
print("|---|---|---|---|---|")
for d in invalid_cases:
    print(f"| `{d['case'][-48:]}` | {d['operator']} | {d['aeoi']} | {d['tpt']} | {d['scipy']} |")
print("\n### CARE 改进原型（不改 aeoi-control，只在 Python 里验证）\n")
print("| 用例 | n×m | cond(R) | AEOI 前向误差 | AEOI + K 形式 Newton–Kleinman | tpt |")
print("|---|---|---|---|---|---|")
for p in P:
    fe = p["forward_error_X"]
    print(f"| `{p['case'][-34:]}` | {p['n']}×{p['m']} | {p['cond_R']:.0e} | {fmt(fe['aeoi'])} | {fmt(fe['aeoi_plus_kform_nk'])} | {fmt(fe.get('tpt'))} |")
