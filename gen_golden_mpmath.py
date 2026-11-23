#!/usr/bin/env python3
"""High-precision golden cases (spec V-1 ②: 'analytic / high-precision computation') for the ill-conditioned
families of control.design.lqr 0.3.0 and control.math.lyap 0.4.0, added after the tpt-control cross-check
(reports/parity-tpt-control.md). Reference values are mpmath 50-digit solutions (Kronecker-form Lyapunov solves,
Newton–Kleinman on the Riccati equation started from scipy), rounded to f64 — NOT MATLAB and NOT python-control.
Layout (V-3): golden/mpmath/<op>/<op_version>/<sha256>.json, matrices row-major.
usage: /workspace/.venv-ctrl/bin/python gen_golden_mpmath.py"""
import hashlib, json, os, platform, sys
import numpy as np, scipy, scipy.linalg as sla, mpmath as mp
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "tools", "xcheck-tpt"))
import analyze as an  # 50-digit helpers shared with the cross-check

mp.mp.dps = 50
ROOT = "golden/mpmath"
SEED = 20261123
ENV = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 50, "numpy": np.__version__, "scipy": scipy.__version__, "python": platform.python_version(), "seed": SEED,
       "method": "lqr: Newton–Kleinman in 50 digits from scipy.linalg.solve_continuous_are until ‖ΔX‖/‖X‖ < 1e-40; lyap: Kronecker-form linear solve in 50 digits",
       "note": "high-precision reference values computed on the Grok Bot box; NOT MATLAB output, NOT python-control"}
VERS = {"control.design.lqr": "0.3.0", "control.math.lyap": "0.4.0"}
rng = np.random.default_rng(SEED)
count = {}

def canon(o): return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
def dm(a): a = np.atleast_2d(np.asarray(a, float)); return {"dtype": "f64", "shape": list(a.shape), "data": [float(v) for v in a.ravel()]}
def tonp(m): return an.tonp(m)

def save(op, family, request, expected, extra=None):
    case = {"spec": "aeoi/0.3", "operator": op, "operator_version": VERS[op], "case_family": family, "reference_env": ENV, "request": request, "expected": expected}
    if extra: case.update(extra)
    h = hashlib.sha256(canon(case).encode()).hexdigest()
    d = f"{ROOT}/{op}/{VERS[op]}"; os.makedirs(d, exist_ok=True)
    json.dump(case, open(f"{d}/{h}.json", "w"), indent=1, ensure_ascii=False)
    count[f"{op} / {family}"] = count.get(f"{op} / {family}", 0) + 1

def mp_eigs(m):
    ev = mp.eig(m, left=False, right=False)
    return sorted(([float(mp.re(e)), float(mp.im(e))] for e in ev), key=lambda p: (p[0], -p[1]))

def lqr_case(family, A, B, Q, R, N=None, extra=None):
    n, m = B.shape
    Nn = np.zeros((n, m)) if N is None else N
    X0 = sla.solve_continuous_are(A, B, Q, R, s=Nn)
    mA, mB, mQ, mR, mN = (an.mpm(x) for x in (A, B, Q, R, Nn))
    X, its, conv = an.truth_riccati(mA, mB, mQ, mR, mN, an.mpm(X0), False, iters=40)
    assert conv, family
    K = an.k_of(mA, mB, mR, mN, X, False)
    req = {"A": dm(A), "B": dm(B), "Q": dm(Q), "R": dm(R)}
    if N is not None: req["N"] = dm(N)
    Xf, Kf = tonp(X), tonp(K)
    exp = {"K": dm(Kf), "S": dm(Xf), "closed_loop_poles": mp_eigs(mA - mB * K), "cond_R": float(np.linalg.cond(R)), "cond_S": float(np.linalg.cond(Xf)),
           "scipy_forward_error_rel_max": an.relerr(X0, Xf), "newton_iterations_50_digits": its}
    save("control.design.lqr", family, req, exp, extra)

def lyap_case(family, A, Q, disc, extra=None):
    mA, mQ = an.mpm(A), an.mpm(Q)
    X = an.lyap_d(mA, mQ, symmetrize=False) if disc else an.lyap_c(mA, mQ, symmetrize=False)
    Xf = tonp(X)
    sp = sla.solve_discrete_lyapunov(A, Q) if disc else sla.solve_continuous_lyapunov(A, -Q)
    ev = np.linalg.eigvals(A)
    exp = {"X": dm(Xf), "spectral_radius" if disc else "max_real_eig": float(max(abs(ev)) if disc else max(ev.real)), "scipy_forward_error_rel_max": an.relerr(sp, Xf)}
    req = {"A": dm(A), "Q": dm(Q), "type": "discrete" if disc else "continuous"}
    save("control.math.lyap", family, req, exp, extra)

# ---------------------------------------------------------------- lqr
# the false-HAMILTONIAN_IMAG_AXIS_EIG repro from the tpt-control cross-check
lqr_case("condR_1e12_decoupled_repro", np.diag([-0.1, -1.0]), np.diag([0.1, 1.0]), np.eye(2), np.diag([1.0, 1e-12]),
         extra={"note": "AEOI lqr 0.2.0 returned E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG (min|Re λ|/‖H‖₁ = 1.41e-13); exact S = diag(4.142135623730952, 9.99999e-7)"})
# the python-control ill_conditioned_R golden requests (0.2.0 store), re-referenced in 50 digits
import glob
for p in sorted(glob.glob("golden/python-control/control.design.lqr/0.2.0/*.json")):
    g = json.load(open(p))
    if g["case_family"] != "ill_conditioned_R": continue
    r = g["request"]
    mat = lambda k: np.array(r[k]["data"], float).reshape(r[k]["shape"])
    lqr_case("ill_conditioned_R_hp", mat("A"), mat("B"), mat("Q"), mat("R"), mat("N") if "N" in r else None,
             extra={"source_case": "golden/python-control/control.design.lqr/0.2.0/" + os.path.basename(p)})
# random, cond(R) 1e10 – 1e12
for c in [1e10, 1e10, 1e11, 1e11, 1e12, 1e12, 1e12, 1e12]:
    n, m = int(rng.integers(2, 7)), int(rng.integers(2, 4))
    A = rng.normal(size=(n, n)); B = rng.normal(size=(n, m))
    W = rng.normal(size=(n, n)); Q = W @ W.T / n + 0.1 * np.eye(n)
    U, _ = np.linalg.qr(rng.normal(size=(m, m)))
    R = U @ np.diag(np.logspace(0, -np.log10(c), m)) @ U.T; R = (R + R.T) / 2
    lqr_case("ill_conditioned_R_extreme", A, B, Q, R)
# cross term with ill-conditioned R
for c in [1e9, 1e11]:
    n, m = 4, 2
    A = rng.normal(size=(n, n)); B = rng.normal(size=(n, m))
    R = np.diag([1.0, 1.0 / c]); N = 1e-3 * rng.normal(size=(n, m))
    Q = np.eye(n) + N @ np.linalg.inv(R) @ N.T; Q = (Q + Q.T) / 2  # keeps [[Q,N],[Nᵀ,R]] ⪰ 0; exactly symmetric
    lqr_case("ill_conditioned_R_cross_N", A, B, Q, R, N)

# ---------------------------------------------------------------- lyap
for rho in [1 - 1e-4, 1 - 1e-6, 1 - 1e-6, 1 - 1e-8]:
    for _ in range(2):
        n = int(rng.integers(2, 6))
        M = rng.normal(size=(n, n)); A = M * (rho / max(abs(np.linalg.eigvals(M))))
        W = rng.normal(size=(n, n)); Q = W @ W.T + np.eye(n)
        lyap_case("discrete_rho_near_1", A, Q, True, extra={"rho": rho})
n = 3; M = rng.normal(size=(n, n)); A = M * ((1 - 1e-6) / max(abs(np.linalg.eigvals(M)))); lyap_case("discrete_rho_near_1_nonsymmetric_Q", A, rng.normal(size=(n, n)), True)
for delta in [1e-4, 1e-6, 1e-8]:
    n = int(rng.integers(2, 6))
    M = rng.normal(size=(n, n)); A = M - (max(np.linalg.eigvals(M).real) + delta * np.linalg.norm(M)) * np.eye(n)
    W = rng.normal(size=(n, n)); Q = W @ W.T + np.eye(n)
    lyap_case("continuous_near_axis", A, Q, False, extra={"delta_rel": delta})
json.dump({"reference_env": ENV, "cases": count, "total": sum(count.values())}, open(f"{ROOT}/INDEX.json", "w"), indent=1, ensure_ascii=False)
print(json.dumps(count, indent=1)); print("total", sum(count.values()))
