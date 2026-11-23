#!/usr/bin/env python3
"""Check the proposed CARE improvement WITHOUT touching aeoi-control: start from AEOI's S and run
Newton–Kleinman in float64 in "K-form" (K = R⁻¹(BᵀX + Nᵀ) via Cholesky, closed-loop Lyapunov
(A−BK)ᵀX + X(A−BK) + Q − NK − KᵀNᵀ + KᵀRK = 0 by Bartels–Stewart), stopping on ‖ΔX‖/‖X‖ rather than on a
decrease of the f64 residual. Compares the forward error with AEOI's result, tpt and scipy (truth: 50-digit
Newton from analyze.py). usage: /workspace/.venv-ctrl/bin/python prototype_nk.py xcheck-tpt.jsonl out.json"""
import json, sys
import numpy as np, scipy.linalg as sla, mpmath as mp
sys.path.insert(0, __import__("os").path.dirname(__file__))
import analyze as an

def nk_kform(A, B, Q, R, N, X, iters=12):
    c = sla.cho_factor(R)
    steps = 0
    for _ in range(iters):
        K = sla.cho_solve(c, B.T @ X + N.T)
        Ak = A - B @ K
        rhs = Q - N @ K - K.T @ N.T + K.T @ R @ K
        Xn = sla.solve_continuous_lyapunov(Ak.T, -(rhs + rhs.T) / 2)
        Xn = (Xn + Xn.T) / 2
        ch = np.linalg.norm(Xn - X) / max(np.linalg.norm(Xn), 1e-300)
        X = Xn
        steps += 1
        if ch < 1e-14:
            break
    return X, sla.cho_solve(c, B.T @ X + N.T), steps

rows = []
for l in open(sys.argv[1]):
    r = json.loads(l)
    if r["operator"] != "control.design.lqr" or "S" not in r["aeoi"]:
        continue
    if not (r["aeoi"]["warnings"] or "condR" in r["case"]):
        continue
    inp = r["inputs"]
    A, B, Q, R = (an.M(inp[k]) for k in "ABQR")
    N = an.M(inp["N"]) if "N" in inp else np.zeros_like(B)
    if A.shape[0] > an.TRUTH_N:
        continue
    S0 = an.M(r["aeoi"]["S"])
    Xp, Kp, steps = nk_kform(A, B, Q, R, N, S0)
    mA, mB, mQ, mR, mN = (an.mpm(x) for x in (A, B, Q, R, N))
    Xs, its, conv = an.truth_riccati(mA, mB, mQ, mR, mN, an.mpm(Xp), False)
    Xt = an.tonp(Xs)
    Kt = an.tonp(an.k_of(mA, mB, mR, mN, Xs, False))
    row = {"case": r["case"], "n": A.shape[0], "m": B.shape[1], "cond_R": float(np.linalg.cond(R)), "nk_steps": steps, "truth_converged": conv,
           "forward_error_X": {"aeoi": an.relerr(S0, Xt), "aeoi_plus_kform_nk": an.relerr(Xp, Xt)},
           "forward_error_K": {"aeoi": an.relerr(an.M(r["aeoi"]["K"]), Kt), "aeoi_plus_kform_nk": an.relerr(Kp, Kt)}}
    if r.get("tpt") and "X" in r["tpt"]:
        row["forward_error_X"]["tpt"] = an.relerr(an.M(r["tpt"]["X"]), Xt)
    rows.append(row)
    print(row["case"][-28:], "condR=%.0e" % row["cond_R"], {k: "%.1e" % v for k, v in row["forward_error_X"].items()}, "steps", steps)
json.dump(rows, open(sys.argv[2], "w"), indent=1)
