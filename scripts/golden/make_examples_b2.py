#!/usr/bin/env python3
"""Batch-2 (spec v0.3) example requests + python-control/scipy reference values + parity.yaml.
Writes examples/<op>/{request.json, request.error.json, request.nonsymmetric.json, reference.python-control.json, parity.yaml}.
Responses are generated separately by scripts/regen_examples.py from the Rust implementation.
Reference values are python-control/scipy outputs computed on the Grok Bot box — NOT MATLAB output."""
import json, glob, pathlib, collections
import numpy as np, scipy, scipy.linalg as sla, scipy.signal as sig, control as ct

root = pathlib.Path(__file__).resolve().parents[2]
ENV = {"engine": "python-control", "python_control": ct.__version__, "scipy": scipy.__version__, "numpy": np.__version__,
       "note": "python-control/scipy reference values (NOT MATLAB output)"}

def M(a):
    a = np.atleast_2d(np.asarray(a, float)); return {"dtype": "f64", "shape": list(a.shape), "data": a.ravel(order="C").tolist()}
def tf(num, den, ts=0.0): return {"kind": "tf", "num": list(map(float, num)), "den": list(map(float, den)), "ts": ts, "time_unit": "s"}
def ss(A, B, C, D, ts=0.0): return {"kind": "ss", "A": M(A), "B": M(B), "C": M(C), "D": M(D), "ts": ts, "time_unit": "s"}
def cl(v): return [[float(np.real(x)), float(np.imag(x))] for x in v]
def fin(x): x = float(x); return x if np.isfinite(x) else ("Inf" if x > 0 else "-Inf") if np.isinf(x) else "NaN"
def ex(rid, **kw): kw["exec"] = {"request_id": rid}; return kw
def nonsym(op):
    for p in sorted(glob.glob(str(root / f"golden/python-control/{op}/0.3.0/*.json"))):
        d = json.load(open(p))
        if d["case_family"] == "memory_order_nonsymmetric":
            r = dict(d["request"]); r["exec"] = {"request_id": f"req-{op.split('.')[-1]}-003-nonsym"}; return r, d["expected"]
def W(op, name, obj):
    (root / "examples" / op / name).write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n")

plant_num, plant_den = [4.0], [1.0, 1.2, 4.0]          # same plant as bode/step (ωn = 2, ζ = 0.3)
A2 = np.array([[0.0, 1.0], [-4.0, -1.2]]); B2 = np.array([[0.0], [4.0]]); C2 = np.array([[1.0, 0.0]])
refs = {}

# ---- convert: tf → ss (controllable canonical, scipy tf2ss convention)
op = "control.lti.convert"
W(op, "request.json", ex("req-convert-001", system=tf(plant_num, plant_den), to="ss"))
W(op, "request.error.json", ex("req-convert-002", system=ss(-np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2))), to="zpk"))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
A, B, C, D = sig.tf2ss(plant_num, plant_den)
refs[op] = {"request.json": {"A": M(A), "B": M(B), "C": M(C), "D": M(D)}, "request.nonsymmetric.json": e,
            "hand_check": "controllable canonical: A = [[-1.2, -4], [1, 0]], B = [1, 0]ᵀ, C = [0, 4], D = 0"}

# ---- poles_zeros: G(s) = (s + 3) / (s² + 1.2 s + 4)
op = "control.lti.poles_zeros"
W(op, "request.json", ex("req-pz-001", system=tf([1.0, 3.0], plant_den)))
W(op, "request.error.json", ex("req-pz-002", system=ss(-np.eye(2), np.ones((2, 1)), np.zeros((1, 2)), np.zeros((1, 1)))))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
g = ct.tf([1.0, 3.0], plant_den); wn, z, p = ct.damp(g, doprint=False)
refs[op] = {"request.json": {"poles": cl(p), "zeros": cl(np.roots([1.0, 3.0])), "dcgain": [float(g.dcgain())], "natural_frequency": wn.tolist(), "damping_ratio": z.tolist()},
            "request.nonsymmetric.json": e, "hand_check": "zero −3; poles −0.6 ± 1.907878j; dcgain 3/4; ωn 2; ζ 0.3"}

# ---- ctrb_obsv: the plant in ss form, with Gramians
op = "control.analysis.ctrb_obsv"
W(op, "request.json", ex("req-co-001", system=ss(A2, B2, C2, [[0.0]]), gramians=True))
W(op, "request.error.json", ex("req-co-002", system=ss(A2, np.array([[0.0], [4.0], [1.0]]), C2, [[0.0]])))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
refs[op] = {"request.json": {"ctrb": M(ct.ctrb(A2, B2)), "obsv": M(ct.obsv(A2, C2)), "Wc": M(sla.solve_continuous_lyapunov(A2, -B2 @ B2.T)),
                              "Wo": M(sla.solve_continuous_lyapunov(A2.T, -C2.T @ C2))}, "request.nonsymmetric.json": e,
            "hand_check": "ctrb = [[0, 4], [4, −4.8]] (rank 2); obsv = I (rank 2)"}

# ---- margin: L(s) = 4 / (s (s+1)(s+2)); GM = 1.5 at ω = √2 (hand check)
op = "control.analysis.margin"
L = ct.tf([4.0], [1.0, 3.0, 2.0, 0.0])
W(op, "request.json", ex("req-margin-001", system=tf([4.0], [1.0, 3.0, 2.0, 0.0])))
W(op, "request.error.json", ex("req-margin-002", system=ss(-np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2)))))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
gm, pm, sm, wpc, wgc, wms = ct.stability_margins(L)
refs[op] = {"request.json": {"gm": fin(gm), "pm": fin(pm), "sm": fin(sm), "wpc": fin(wpc), "wgc": fin(wgc), "wms": fin(wms)},
            "request.nonsymmetric.json": e, "hand_check": "phase −180° at ω = √2 where |L| = 4/(√2·√3·√6) = 2/3 ⇒ GM = 1.5 (3.52 dB)"}

# ---- c2d: plant (ss), ts = 0.1, zoh
op = "control.lti.c2d"
W(op, "request.json", ex("req-c2d-001", system=ss(A2, B2, C2, [[0.0]]), ts=0.1, method="zoh"))
W(op, "request.error.json", ex("req-c2d-002", system=tf(plant_num, plant_den), ts=0.1, method="zoh", prewarp_frequency=2.0))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
Ad, Bd, Cd, Dd, _ = sig.cont2discrete((A2, B2, C2, [[0.0]]), 0.1, method="zoh")
refs[op] = {"request.json": {"A": M(Ad), "B": M(Bd), "C": M(Cd), "D": M(Dd)}, "request.nonsymmetric.json": e,
            "hand_check": "eig(A_d) = exp(0.1·(−0.6 ± 1.907878j))"}

# ---- impulse: plant tf, 0..10 s, dt = 0.05
op = "control.lti.impulse"
W(op, "request.json", ex("req-impulse-001", system=tf(plant_num, plant_den), time={"mode": "uniform", "t_final": 10.0, "dt": 0.05, "unit": "s"}))
W(op, "request.error.json", ex("req-impulse-002", system=tf([0.5], [1.0, -0.5], 0.1), time={"mode": "uniform", "t_final": 1.0, "dt": 0.05, "unit": "s"}))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
t = np.round(np.arange(0, 10.0 + 0.025, 0.05), 12); t = t[t <= 10.0 + 1e-9]
tt, yy = ct.impulse_response(ct.tf(plant_num, plant_den), T=t)
refs[op] = {"request.json": {"t": tt.tolist(), "y": np.asarray(yy).ravel().tolist()}, "request.nonsymmetric.json": e,
            "hand_check": "y(t) = (4/ωd)·e^{−0.6t}·sin(ωd t), ωd = 1.907878"}

# ---- lyap: continuous, non-symmetric A
op = "control.math.lyap"
A = np.array([[-1.0, 2.0], [0.0, -3.0]]); Q = np.eye(2)
W(op, "request.json", ex("req-lyap-001", A=M(A), Q=M(Q), type="continuous"))
W(op, "request.error.json", ex("req-lyap-002", A=M([[0.0, 1.0], [-1.0, 0.0]]), Q=M(Q), type="continuous"))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
refs[op] = {"request.json": {"X": M(sla.solve_continuous_lyapunov(A, -Q))}, "request.nonsymmetric.json": e,
            "hand_check": "A X + X Aᵀ + Q = 0 (AEOI convention = scipy solve_continuous_lyapunov(A, −Q) = python-control lyap(A, Q))"}

# ---- place: double integrator, poles −1 ± j ⇒ K = [2, 2]
op = "control.design.place"
A = np.array([[0.0, 1.0], [0.0, 0.0]]); B = np.array([[0.0], [1.0]])
W(op, "request.json", ex("req-place-001", A=M(A), B=M(B), poles=[[-1.0, 1.0], [-1.0, -1.0]]))
W(op, "request.error.json", ex("req-place-002", A=M(np.diag([1.0, -1.0])), B=M([[0.0], [1.0]]), poles=[[-1.0, 0.0], [-2.0, 0.0]]))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
refs[op] = {"request.json": {"K": M(ct.place(A, B, [-1 + 1j, -1 - 1j]))}, "request.nonsymmetric.json": e,
            "hand_check": "det(sI − A + BK) = s² + k2 s + k1 = s² + 2s + 2 ⇒ K = [2, 2]"}

# ---- dlqr: ZOH double integrator, ts = 0.1
op = "control.design.dlqr"
A = np.array([[1.0, 0.1], [0.0, 1.0]]); B = np.array([[0.005], [0.1]])
W(op, "request.json", ex("req-dlqr-001", A=M(A), B=M(B), Q=M(np.eye(2)), R=M([[1.0]])))
W(op, "request.error.json", ex("req-dlqr-002", A=M(np.diag([1.2, 0.5])), B=M([[0.0], [1.0]]), Q=M(np.eye(2)), R=M([[1.0]])))
r, e = nonsym(op); W(op, "request.nonsymmetric.json", r)
K, S, E = ct.dlqr(A, B, np.eye(2), np.eye(1))
refs[op] = {"request.json": {"K": M(K), "S": M(S), "closed_loop_poles": cl(E)}, "request.nonsymmetric.json": e,
            "hand_check": "S solves the DARE (scipy solve_discrete_are)"}

for op, v in refs.items():
    v["reference_env"] = ENV
    W(op, "reference.python-control.json", v)

# ---- parity.yaml (families/counts read from the golden store)
TOL = {
 "control.lti.convert": ("T1/T2", {"tf→ss matrices": "rel_fro ≤ 1e-12 (T1)",
     "tf coefficients": "‖Δ‖₁/max(‖ref‖₁,‖den‖₁) ≤ 1e-10·max(1, Π(1+|λ_i|)/‖den‖₁)  (λ = roots(den); coefficient-from-eigenvalue bound)",
     "zpk zeros/poles": "nearest-match |Δ|/max(1,|r|) ≤ 1e-10·max(1, κ_r) (Wilkinson root condition of the reference polynomial)",
     "zpk gain k": "rel ≤ 1e-10"}),
 "control.lti.poles_zeros": ("T2", {"poles/zeros": "nearest-match |Δ|/max(1,|r|) ≤ 1e-10·max(1, κ_r)",
     "dcgain": "|Δ|/max(1,|ref|) ≤ 1e-10·max(1, Π(1+|p|)/Π|p|); ±Inf must agree exactly",
     "natural_frequency / damping_ratio": "≤ 1e-10·max(1, κ_r) of the matched pole"}),
 "control.analysis.ctrb_obsv": ("T0/T1/T2", {"rank": "exact (T0)", "Krylov matrices": "rel_fro ≤ 1e-12 (T1)",
     "singular values": "max|Δσ|/σ_max ≤ 1e-12", "gramians": "rel_fro ≤ max(1e-10, 10·rel_residual/min(1, sep_min))"}),
 "control.analysis.margin": ("T3", {"gain margin": "rel ≤ 1e-9", "phase margin": "abs ≤ 1e-8 deg", "crossover frequencies": "rel ≤ 1e-9",
     "stability margin": "rel ≤ 1e-9; discrete: or ours lower than python-control's optimizer value (reference verified consistent ≤ 1e-9, gap ≤ 1e-6)",
     "SM frequency": "continuous rel ≤ 1e-6 (flat minimum); discrete rel ≤ 1e-3 (python-control uses scipy L-BFGS-B)",
     "crossing sets": "every python-control crossing must be found (extra crossings allowed and reported)"}),
 "control.lti.c2d": ("T2", {"discrete ss matrices": "rel_fro ≤ 1e-10·κ, κ = cond(I − αA) (tustin) or e^{‖A‖₁·ts} (zoh/foh)",
     "discrete tf coefficients": "as convert, times κ"}),
 "control.lti.impulse": ("T2", {"y": "max|Δy|/max|y_ref| ≤ 1e-10"}),
 "control.math.lyap": ("T2", {"X": "rel_fro ≤ max(1e-10, 10·trust.error_estimate.X_rel)"}),
 "control.design.place": ("T2", {"achieved poles": "eig(A − B·K) recomputed by the test from the reference request; |Δ|/max(1,|p|) ≤ 1e-10·cond(X)·(1+‖K‖_F)",
     "K (SISO only; unique)": "rel_fro ≤ 1e-10·cond(X)·(1+‖K‖_F)",
     "K (MIMO)": "not compared (not unique); cond(X) vs scipy YT reported as information"}),
 "control.design.dlqr": ("T2", {"S, K": "rel_fro ≤ max(1e-10·max(1, cond(S)/1e2), 10·trust.error_estimate.S_rel)  (same rule as lqr)",
     "closed-loop poles": "nearest match, κ_r-scaled with the same base tolerance"}),
}
for op, (cls, cmp) in TOL.items():
    fams = collections.Counter(); errs = {}; warns = {}
    for p in glob.glob(str(root / f"golden/python-control/{op}/0.3.0/*.json")):
        d = json.load(open(p)); fams[d["case_family"]] += 1
        if "expected_error" in d: errs[d["case_family"]] = d["expected_error"]
        if "expected_warnings" in d: warns[d["case_family"]] = d["expected_warnings"]
    ver = json.load(open(root / "examples" / op / "manifest.json"))["version"]
    hp = sorted(glob.glob(str(root / f"golden/mpmath/{op}/{ver}/*.json")))
    lines = [f"# Parity test spec — {op} {ver} (AEOI v0.3-draft)" + (f"; python-control golden store generated for 0.3.0 and reused unchanged (V-3a)" if ver != "0.3.0" else ""), f"operator: {op}", "references:",
             '  - {engine: python-control, version: "0.10.2 (scipy 1.18.1, numpy 2.5.5)", role: dev_reference, status: active,',
             f'     generator: scripts/golden/gen_golden_pycontrol_b2.py, golden_store: "golden/python-control/{op}/0.3.0/*.json",',
             '     note: "python-control/scipy reference values, NOT MATLAB output"}',
             *([f'  - {{engine: mpmath, version: "50 digits", role: high_precision_reference, status: active, generator: scripts/golden/gen_golden_mpmath.py,',
                f'     golden_store: "golden/mpmath/{op}/{ver}/*.json", runner: "cargo test --release -p aeoi-control --test parity_hp",',
                '     compare: "rel_fro ≤ 1e-12, no relaxation", note: "ill-conditioned families from the tpt-control cross-check; NOT MATLAB, NOT python-control"}'] if hp else []),
             '  - {engine: MATLAB, role: ground_truth, status: planned, note: "no generator script yet for batch 2"}',
             'runner: "cargo test --release -p aeoi-control --test parity_b2"', f"tolerance_class: {cls}", "compare:"]
    lines += [f'  "{k}": "{v}"' for k, v in cmp.items()]
    lines.append("case_families:   # n = golden cases")
    for f, n in sorted(fams.items()):
        extra = f", expect: {errs[f]}" if f in errs else (f", expect_warning: {warns[f]}" if f in warns else "")
        note = ', note: "non-symmetric matrices; the column-major (A(:)) request must deviate > 1e3 × tolerance"' if f == "memory_order_nonsymmetric" else ""
        lines.append(f"  - {{name: {f}, n: {n}{extra}{note}}}")
    if hp:
        hf = collections.Counter(json.load(open(p))["case_family"] for p in hp)
        lines.append("case_families_high_precision:   # golden/mpmath, n = cases")
        lines += [f"  - {{name: {f}, n: {n}}}" for f, n in sorted(hf.items())]
    if op == "control.lti.poles_zeros":
        lines.append('reference_postprocess: "python-control (no slycot) returns infinite generalized eigenvalues of the zero pencil as huge finite numbers; zeros with |z| > 1e10·(1+‖[A B; C D]‖_F) are dropped from the reference"')
    lines.append('pass_rule: "all cases within tolerance; expected-error cases return the same code; expected warnings present; every emitted code declared in the manifest; outputs validate against envelope+output schema"')
    (root / "examples" / op / "parity.yaml").write_text("\n".join(lines) + "\n")
print("ok")
