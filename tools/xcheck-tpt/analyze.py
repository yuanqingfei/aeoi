#!/usr/bin/env python3
"""Analyse xcheck-tpt.jsonl: per-operator agreement, error-case disagreements, and — for lyap, lqr,
dlqr — who is closer to the truth.

Truth = high-precision solution (mpmath, 50 digits): Kronecker solve for Lyapunov, Newton–Kleinman
(continuous) / Hewer (discrete) refinement for Riccati started from the best f64 candidate, for
n <= TRUTH_N; for every case also the relative residual of each candidate evaluated in 50 digits.
Third candidate: scipy (python-control's backend) — the stored golden value for corpus cases,
computed here for random cases.

usage: /workspace/.venv-ctrl/bin/python analyze.py xcheck.jsonl out.json
"""
import json, sys, math, collections, time
import numpy as np
import mpmath as mp
import scipy, scipy.linalg as sla

mp.mp.dps = 50
TRUTH_N = 10

def M(x):  # nested list (maybe "inf"/"NaN" strings) -> np array
    return np.array([[float(v) for v in row] for row in x], dtype=float)

def mpm(a):
    return mp.matrix(a.tolist())

def tonp(m):
    return np.array([[float(m[i, j]) for j in range(m.cols)] for i in range(m.rows)])

def rdiv(a, b):
    return float(a / b) if b != 0 else float(a)

def fro(m):
    return mp.sqrt(sum(m[i, j] ** 2 for i in range(m.rows) for j in range(m.cols)))

def kron(a, b):
    ra, ca, rb, cb = a.rows, a.cols, b.rows, b.cols
    k = mp.zeros(ra * rb, ca * cb)
    for i in range(ra):
        for j in range(ca):
            if a[i, j] != 0:
                for p in range(rb):
                    for q in range(cb):
                        k[i * rb + p, j * cb + q] = a[i, j] * b[p, q]
    return k

def vec(x):
    n, m = x.rows, x.cols
    return mp.matrix([x[i, j] for i in range(n) for j in range(m)])

def unvec(v, n):
    x = mp.zeros(n, n)
    for i in range(n):
        for j in range(n):
            x[i, j] = v[i * n + j]
    return x

def sym(x):
    return (x + x.T) / 2

# vec_r(L X R) = (L ⊗ Rᵀ) vec_r(X)
def lyap_c(L, Q, symmetrize=True):  # L X + X Lᵀ + Q = 0
    n = L.rows
    I = mp.eye(n)
    K = kron(L, I) + kron(I, L)
    X = unvec(mp.lu_solve(K, -vec(Q)), n)
    return sym(X) if symmetrize else X

def lyap_d(L, Q, symmetrize=True):  # L X Lᵀ − X + Q = 0
    n = L.rows
    K = kron(L, L) - mp.eye(n * n)
    X = unvec(mp.lu_solve(K, -vec(Q)), n)
    return sym(X) if symmetrize else X

# ---------------------------------------------------------------- residuals (relative, in 50 digits)
def res_lyap(A, Q, X, disc):
    if disc:
        t1 = A * X * A.T
        R = t1 - X + Q
        s = fro(t1) + fro(X) + fro(Q)
    else:
        t1, t2 = A * X, X * A.T
        R = t1 + t2 + Q
        s = fro(t1) + fro(t2) + fro(Q)
    return rdiv(fro(R), s)

def res_care(A, B, Q, Rm, N, X):
    Ri = mp.inverse(Rm)
    XB = X * B + N
    t1, t2, t3 = A.T * X, X * A, XB * Ri * XB.T
    R = t1 + t2 - t3 + Q
    return rdiv(fro(R), fro(t1) + fro(t2) + fro(t3) + fro(Q))

def res_dare(A, B, Q, Rm, N, X):
    G = Rm + B.T * X * B
    F = A.T * X * B + N
    t1, t3 = A.T * X * A, F * mp.inverse(G) * F.T
    R = t1 - X - t3 + Q
    return rdiv(fro(R), fro(t1) + fro(X) + fro(t3) + fro(Q))

def k_of(A, B, Rm, N, X, disc):
    if disc:
        return mp.inverse(Rm + B.T * X * B) * (B.T * X * A + N.T)
    return mp.inverse(Rm) * (B.T * X + N.T)

def truth_riccati(A, B, Q, Rm, N, X0, disc, iters=12):
    """Newton (continuous) / Hewer (discrete) on the N-reduced problem, from X0."""
    Ri = mp.inverse(Rm)
    Ab = A - B * Ri * N.T
    Qb = Q - N * Ri * N.T
    G = B * Ri * B.T
    X = X0
    for it in range(iters):
        if disc:
            K = mp.inverse(Rm + B.T * X * B) * (B.T * X * Ab)
            Ak = Ab - B * K
            Xn = lyap_d(Ak.T, Qb + K.T * Rm * K)
        else:
            Ak = Ab - G * X
            Xn = lyap_c(Ak.T, Qb + X * G * X)
        ch = fro(Xn - X) / max(fro(Xn), mp.mpf(10) ** -300)
        X = Xn
        if ch < mp.mpf(10) ** -40:
            return X, it + 1, True
    return X, iters, False

def relerr(x, ref):
    d = np.max(np.abs(x - ref))
    s = np.max(np.abs(ref))
    return float(d / s) if s > 0 else float(d)

STAB_TOL = 1e-9  # closed loop counts as stabilizing only with a margin (|λ| ≤ 1 − tol, Re λ ≤ −tol·max(1, ‖A‖))

def stab(A, B, K, disc):
    ev = np.linalg.eigvals(A - B @ K)
    if disc:
        return bool(np.max(np.abs(ev)) <= 1 - STAB_TOL)
    return bool(np.max(ev.real) <= -STAB_TOL * max(1.0, np.linalg.norm(A, 1)))

# ---------------------------------------------------------------- scipy third opinion
def scipy_solve(op, inp, req):
    try:
        if op == "lyap":
            A, Q = M(inp["A"]), M(inp["Q"])
            if inp["type"] == "discrete":
                return {"X": sla.solve_discrete_lyapunov(A, Q)}
            return {"X": sla.solve_continuous_lyapunov(A, -Q)}
        A, B, Q, R = (M(inp[k]) for k in "ABQR")
        N = M(inp["N"]) if "N" in inp else None
        if op == "dlqr":
            X = sla.solve_discrete_are(A, B, Q, R, s=N)
            Nn = N if N is not None else np.zeros_like(B)
            K = np.linalg.solve(R + B.T @ X @ B, B.T @ X @ A + Nn.T)
        else:
            X = sla.solve_continuous_are(A, B, Q, R, s=N)
            Nn = N if N is not None else np.zeros_like(B)
            K = np.linalg.solve(R, B.T @ X + Nn.T)
        return {"X": X, "K": K}
    except Exception as e:
        return {"error": f"{type(e).__name__}: {e}"[:200]}

# ---------------------------------------------------------------- main
def main(src, dst):
    recs = [json.loads(l) for l in open(src)]
    out = {"cases": [], "by_operator": {}}
    t0 = time.time()
    for r in recs:
        op = r["operator"].split(".")[-1]
        row = {"case": r["case"], "operator": op, "source": r["source"], "family": r["case"].rsplit("/", 1)[0] if r["source"] == "random" else ("repro" if r["source"] == "repro" else "corpus"),
               "aeoi_status": r["aeoi"]["status"], "aeoi_error": r["aeoi"]["error_code"], "aeoi_warnings": r["aeoi"]["warnings"],
               "tpt_error": (r.get("tpt") or {}).get("error"), "skip": r.get("skip"), "diff": r.get("diff"),
               "us": {"aeoi": r["aeoi"].get("us"), "tpt": (r.get("tpt") or {}).get("us")}}
        if op == "margin" and "tpt" in r and "aeoi" in r and r["aeoi"].get("phase_margin_deg") is not None:
            a, t = r["aeoi"], r["tpt"]
            f = lambda v: float(v) if v is not None else math.nan
            row["margin"] = {"aeoi": {k: a.get(k) for k in ("gain_margin", "phase_margin_deg", "w_gain_crossover", "w_phase_crossover")},
                             "tpt": {k: t.get(k) for k in ("gain_margin", "phase_margin_deg", "w_gain_crossover", "w_phase_crossover")}}
            gm_a, gm_t = f(a.get("gain_margin")), f(t.get("gain_margin"))
            row["margin"]["gm_rel"] = abs(gm_a - gm_t) / abs(gm_a) if math.isfinite(gm_a) and math.isfinite(gm_t) else (0.0 if gm_a == gm_t else None)
            row["margin"]["pm_abs_deg"] = abs(f(a.get("phase_margin_deg")) - f(t.get("phase_margin_deg")))
        if op in ("lyap", "lqr", "dlqr") and "inputs" in r:
            inp = r["inputs"]
            disc = (inp.get("type") == "discrete") if op == "lyap" else (op == "dlqr")
            cands = {}
            if "X" in r["aeoi"]:
                cands["aeoi"] = {"X": M(r["aeoi"]["X"])}
                if "K" in r["aeoi"]:
                    cands["aeoi"] = {"X": M(r["aeoi"]["S"]), "K": M(r["aeoi"]["K"])}
            elif "S" in r["aeoi"]:
                cands["aeoi"] = {"X": M(r["aeoi"]["S"]), "K": M(r["aeoi"]["K"])}
            if r.get("tpt") and "X" in r["tpt"]:
                cands["tpt"] = {"X": M(r["tpt"]["X"])}
                if "K" in r["tpt"]:
                    cands["tpt"]["K"] = M(r["tpt"]["K"])
            sp = None
            if r["source"] == "corpus" and r.get("python_control"):
                pc = r["python_control"]
                sp = {"X": M(pc["X"])} if "X" in pc else {"X": M(pc["S"]), "K": M(pc["K"])}
                row["scipy_source"] = "golden (python-control 0.10.2 / scipy 1.18.1)"
            else:
                sp = scipy_solve(op, inp, None)
                row["scipy_source"] = f"scipy {scipy.__version__} (computed now)"
            if "error" in sp:
                row["scipy_error"] = sp["error"]
            else:
                cands["scipy"] = sp
            A = mpm(M(inp["A"]))
            Q = mpm(M(inp["Q"]))
            res, stabl = {}, {}
            finite = {k: v for k, v in cands.items() if np.all(np.isfinite(v["X"]))}
            if op == "lyap":
                for k, v in finite.items():
                    res[k] = res_lyap(A, Q, mpm(v["X"]), disc)
            else:
                B, Rm = mpm(M(inp["B"])), mpm(M(inp["R"]))
                N = mpm(M(inp["N"])) if "N" in inp else mp.zeros(B.rows, B.cols)
                An, Bn = M(inp["A"]), M(inp["B"])
                for k, v in finite.items():
                    X = mpm(v["X"])
                    res[k] = (res_dare if disc else res_care)(A, B, Q, Rm, N, X)
                    if "K" in v:
                        stabl[k] = stab(An, Bn, v["K"], disc)
            row["residual"] = res
            row["stabilizing"] = stabl
            n = len(inp["A"])
            if finite and n <= TRUTH_N:
                try:
                    if op == "lyap":
                        Xs = (lyap_d if disc else lyap_c)(A, Q, symmetrize=False)  # Q may be non-symmetric
                        conv = True
                        An = M(inp["A"]); nn = An.shape[0]; I = np.eye(nn)
                        Kop = np.kron(An, An) - np.eye(nn * nn) if disc else np.kron(An, I) + np.kron(I, An)
                        row["lyap_operator_cond2"] = float(np.linalg.cond(Kop))
                    else:
                        # start from the best stabilizing candidate
                        order = sorted(finite, key=lambda k: (not stabl.get(k, False), res[k]))
                        Xs, its, conv = truth_riccati(A, B, Q, Rm, N, mpm(finite[order[0]]["X"]), disc)
                        row["truth_newton_iterations"] = its
                        row["truth_start"] = order[0]
                    Xt = tonp(Xs)
                    row["truth_converged"] = conv
                    fe = {k: relerr(v["X"], Xt) for k, v in finite.items()}
                    row["forward_error_X"] = fe
                    if op != "lyap":
                        Kt = tonp(k_of(A, B, Rm, N, Xs, disc))
                        row["forward_error_K"] = {k: relerr(v["K"], Kt) for k, v in finite.items() if "K" in v}
                        row["truth_stabilizing"] = stab(M(inp["A"]), M(inp["B"]), Kt, disc)
                except Exception as e:
                    row["truth_error"] = f"{type(e).__name__}: {e}"[:200]
        out["cases"].append(row)
    out["seconds"] = time.time() - t0
    json.dump(out, open(dst, "w"), indent=1, default=float)
    print("analysed", len(out["cases"]), "cases in %.0f s" % out["seconds"])

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
