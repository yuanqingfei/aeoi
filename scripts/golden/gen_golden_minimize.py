"""Golden cases for optim.quadratic.quadprog and optim.scalar.fminbnd (spec v0.4.26, 10.45, 10.46).

quadprog — truth by enumeration, independent of any active-set path: for every subset of the inequality rows
(bounds included) with at most n − #eq rows, the equality-constrained KKT system is solved at 50 digits
(mpmath); the unique subset whose solution is primal feasible with all its multipliers ≥ 0 is the optimum (H is
positive definite, so it is unique). Cases are rejected when an active constraint has a multiplier below 1e-6 or
an inactive one has a slack below 1e-6 (degenerate: the optimum is still unique, but the multipliers are not).
fminbnd — truth: every local minimiser of f in the interval at 50 digits (mpmath.findroot on f′ from a fine
grid of starts, kept when f″ > 0, plus the end points when f is increasing into the interval), and the global
one. Which valley Brent's method ends in is path-dependent, so for several valleys the golden case lists them all;
the rule then requires W.ACCURACY.BETTER_POINT_FOUND whenever the returned point is not the global minimiser.
NOT MATLAB output. Golden store: golden/mpmath/<op>/0.4.0 (V-3).
"""
import hashlib, itertools, json, math, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 50
SEED = 20261130
rng = np.random.default_rng(SEED)
VER = "0.4.0"
ENV = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 50, "numpy": np.__version__, "python": platform.python_version(), "seed": SEED,
       "method": "quadprog: active-set enumeration with 50-digit KKT solves; fminbnd: every local minimiser by findroot on f′ at 50 digits",
       "note": "high-precision truth; NOT MATLAB output"}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


class Saver:
    def __init__(self, op):
        self.op, self.count = op, 0
        self.dir = f"golden/mpmath/{op}/{VER}"
        shutil.rmtree(self.dir, ignore_errors=True); os.makedirs(self.dir)

    def save(self, family, request, expected=None, expected_error=None, expect_warning=None, may_fail=None):
        case = {"spec": "aeoi/0.4", "operator": self.op, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
        if may_fail: case["postcondition_may_fail"] = may_fail
        if expected is not None: case["expected"] = expected
        if expected_error is not None: case["expected_error"] = expected_error
        if expect_warning: case["expect_warning"] = expect_warning
        h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
        json.dump(case, open(f"{self.dir}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
        self.count += 1


# ============================================================================================ quadprog
QP = Saver("optim.quadratic.quadprog")


def qp_truth(H, f, A, b, Aeq, beq, lb, ub):
    n = len(f)
    Hs = (np.asarray(H, float) + np.asarray(H, float).T) / 2
    rows = [list(map(float, r)) for r in A]; rhs = list(map(float, b)); kind = [("ineqlin", i) for i in range(len(A))]
    for j in range(n):
        if lb is not None and lb[j] != "-Inf":
            r = [0.0] * n; r[j] = -1.0; rows.append(r); rhs.append(-float(lb[j])); kind.append(("lower", j))
        if ub is not None and ub[j] != "Inf":
            r = [0.0] * n; r[j] = 1.0; rows.append(r); rhs.append(float(ub[j])); kind.append(("upper", j))
    me = len(Aeq); m = len(rows)
    Hm = mp.matrix([[mp.mpf(v) for v in r] for r in Hs]); fm = mp.matrix([mp.mpf(float(v)) for v in f])
    found = []
    for k in range(0, min(m, n - me) + 1):
        for S in itertools.combinations(range(m), k):
            W = [list(map(float, r)) for r in Aeq] + [rows[i] for i in S]
            wr = list(map(float, beq)) + [rhs[i] for i in S]
            dim = n + len(W)
            K = mp.zeros(dim, dim); R = mp.zeros(dim, 1)
            for i in range(n):
                for j in range(n): K[i, j] = Hm[i, j]
                R[i] = -fm[i]
            for t, wrow in enumerate(W):
                for j in range(n):
                    K[n + t, j] = mp.mpf(wrow[j]); K[j, n + t] = mp.mpf(wrow[j])
                R[n + t] = mp.mpf(wr[t])
            try:
                sol = mp.lu_solve(K, R)
            except ZeroDivisionError:
                continue
            x = [sol[i] for i in range(n)]; mu = [sol[n + t] for t in range(len(W))]
            slack = [mp.mpf(rhs[i]) - mp.fsum(mp.mpf(rows[i][j]) * x[j] for j in range(n)) for i in range(m)]
            if min(slack + [mp.mpf(1)]) < -mp.mpf(10) ** -30 or any(l < -mp.mpf(10) ** -30 for l in mu[me:]):
                continue
            found.append((S, x, mu, slack))
    assert len(found) == 1, f"{len(found)} KKT points"
    S, x, mu, slack = found[0]
    lam = {"ineqlin": [0.0] * len(A), "eqlin": [float(v) for v in mu[:me]], "lower": [0.0] * n, "upper": [0.0] * n}
    for t, i in enumerate(S):
        g, idx = kind[i]; lam[g][idx] = float(mu[me + t])
    act = [float(mu[me + t]) for t in range(len(S))]
    inact = [float(slack[i]) for i in range(m) if i not in S]
    assert all(a > 1e-6 for a in act) and all(s > 1e-6 for s in inact), "degenerate optimum"
    xf = [float(v) for v in x]
    fval = float(mp.mpf(0.5) * mp.fsum(x[i] * Hm[i, j] * x[j] for i in range(n) for j in range(n)) + mp.fsum(fm[i] * x[i] for i in range(n)))
    return {"x_true": xf, "fval_true": fval, "lambda_true": lam, "active": [kind[i][0] + f"/{kind[i][1]}" for i in S],
            "kappa2_H": float(np.linalg.cond(Hs)), "n": n}


def qp_case(fam, H, f, A=(), b=(), Aeq=(), beq=(), lb=None, ub=None, warn=None):
    req = {"H": [list(map(float, r)) for r in H], "f": list(map(float, f))}
    if len(A): req["A"] = [list(map(float, r)) for r in A]; req["b"] = list(map(float, b))
    if len(Aeq): req["Aeq"] = [list(map(float, r)) for r in Aeq]; req["beq"] = list(map(float, beq))
    if lb is not None: req["lb"] = lb
    if ub is not None: req["ub"] = ub
    QP.save(fam, req, qp_truth(H, f, A, b, Aeq, beq, lb, ub), expect_warning=warn)


qp_case("unconstrained", [[2.0, 0.5], [0.5, 1.0]], [-1.0, 2.0])
qp_case("matlab_doc_example", [[1.0, -1.0], [-1.0, 2.0]], [-2.0, -6.0], A=[[1.0, 1.0], [-1.0, 2.0], [2.0, 1.0]], b=[2.0, 2.0, 3.0])
qp_case("bounds_only", np.eye(3), [-3.0, 1.0, -0.5], lb=[0.0, 0.0, 0.0], ub=[2.0, "Inf", 1.0])
qp_case("equality_and_inequality", [[4.0, 1.0, 0.0], [1.0, 3.0, 0.5], [0.0, 0.5, 2.0]], [1.0, -2.0, 0.5],
        A=[[1.0, 1.0, 1.0], [-1.0, 0.0, 2.0]], b=[0.5, 1.0], Aeq=[[1.0, -1.0, 0.0]], beq=[0.25])
B = rng.normal(size=(8, 8)); Hr = B.T @ B + 0.5 * np.eye(8)
qp_case("random_8", Hr, rng.normal(size=8), A=rng.normal(size=(6, 8)), b=rng.uniform(0.1, 1.0, size=6),
        Aeq=rng.normal(size=(1, 8)), beq=[0.3], lb=["-Inf"] * 7 + [-0.2], ub=["Inf"] * 8)
Qm, _ = np.linalg.qr(rng.normal(size=(4, 4)))
qp_case("ill_conditioned_H", Qm @ np.diag([1.0, 1e-2, 1e-4, 1e-6]) @ Qm.T, [1.0, -1.0, 0.5, 0.0], A=[[1.0, 1.0, 1.0, 1.0]], b=[0.5])
qp_case("asymmetric_H_symmetrized", [[2.0, 1.0], [0.0, 2.0]], [1.0, -1.0], lb=[0.0, 0.0], warn="W.VALIDATION.SYMMETRIZED")
QP.save("indefinite_H", {"H": [[1.0, 2.0], [2.0, 1.0]], "f": [0.0, 0.0]}, expected_error="E.DOMAIN.NOT_POSITIVE_DEFINITE")
QP.save("semidefinite_H", {"H": [[1.0, 0.0], [0.0, 0.0]], "f": [1.0, 1.0], "lb": [0.0, 0.0]}, expected_error="E.DOMAIN.NOT_POSITIVE_DEFINITE")
QP.save("infeasible", {"H": [[1.0]], "f": [0.0], "A": [[1.0], [-1.0]], "b": [0.0, -1.0]}, expected_error="E.DOMAIN.INFEASIBLE")
QP.save("bounds_crossed", {"H": [[1.0]], "f": [0.0], "lb": [2.0], "ub": [1.0]}, expected_error="E.DOMAIN.INFEASIBLE")
QP.save("H_wrong_shape", {"H": [[1.0, 0.0]], "f": [1.0, 1.0]}, expected_error="E.VALIDATION.SHAPE")
QP.save("missing_H", {"f": [1.0]}, expected_error="E.VALIDATION.SCHEMA")
print("quadprog golden:", QP.count)

# ============================================================================================= fminbnd
FB = Saver("optim.scalar.fminbnd")


def local_minima(F, lo, hi, n_starts=400):
    """All local minimisers of F on [lo, hi] at 50 digits (interior: F′ = 0, F″ > 0; end points: F increasing inward)."""
    d1 = lambda x: mp.diff(F, x); d2 = lambda x: mp.diff(F, x, 2)
    pts = []
    for s in np.linspace(lo, hi, n_starts):
        try:
            r = mp.findroot(d1, mp.mpf(float(s)))
        except (ValueError, ZeroDivisionError):
            continue
        if lo < r < hi and d2(r) > 0 and all(abs(r - p) > mp.mpf(10) ** -20 for p in pts):
            pts.append(r)
    eps = mp.mpf(10) ** -20   # a stationary end point (f′ = 0, e.g. a maximum there) is not a boundary minimum
    if d1(mp.mpf(lo)) > eps: pts.append(mp.mpf(lo))
    if d1(mp.mpf(hi)) < -eps: pts.append(mp.mpf(hi))
    for p in pts:   # independent check: no lower value next to each minimiser
        for h in (mp.mpf(10) ** -12, mp.mpf(10) ** -6):
            for q in (p - h, p + h):
                if lo <= q <= hi:
                    assert F(q) >= F(p), "not a local minimum"
    return sorted(pts, key=lambda p: F(p))


def fb_case(fam, expr, F, interval, tol_x=None, params=None, max_evaluations=None):
    lo, hi = interval
    mins = local_minima(F, lo, hi)
    deep = len(mins) > 1
    req = {"expression": expr, "interval": [float(lo), float(hi)]}
    if tol_x is not None: req["tol_x"] = tol_x
    if params: req["params"] = params
    if max_evaluations is not None: req["max_evaluations"] = max_evaluations
    exp = {"minima": [{"x": float(p), "f": float(F(p))} for p in mins], "global_x": float(mins[0]), "global_f": float(F(mins[0])), "tol_x": tol_x or 1e-4}
    # several valleys: which one Brent ends in is path-dependent, so the failure of grid_probe is not predicted;
    # `postcondition_may_fail` tells the reference receipt test that either outcome is honest (the rule checks it)
    FB.save(fam, req, exp, may_fail=["grid_probe"] if deep else None)


fb_case("cos_3_4", "cos(x)", mp.cos, (3.0, 4.0))
fb_case("matlab_doc_sin", "sin(x)", mp.sin, (0.0, 2 * math.pi))
fb_case("parabola", "(x - 2)^2 + 1", lambda x: (x - 2) ** 2 + 1, (0.0, 5.0))
fb_case("cubic", "x^3 - 2*x - 5", lambda x: x ** 3 - 2 * x - 5, (0.0, 2.0))
fb_case("boundary_minimum", "x*exp(-x)", lambda x: x * mp.exp(-x), (0.0, 1.0))
fb_case("tight_tolerance", "(x - pi)^2", lambda x: (x - mp.pi) ** 2, (0.0, 5.0), tol_x=1e-10)
fb_case("params", "a*(x - c)^2 + d", lambda x: mp.mpf(3) * (x - mp.mpf(0.75)) ** 2 - 1, (-1.0, 2.0), params={"a": 3.0, "c": 0.75, "d": -1.0})
fb_case("two_valleys_shallow_found", "x^4 - 8*x^2 + 3*x", lambda x: x ** 4 - 8 * x ** 2 + 3 * x, (-2.5, 3.0))
fb_case("two_valleys_global_found", "x^4 - 8*x^2 + 3*x", lambda x: x ** 4 - 8 * x ** 2 + 3 * x, (-3.0, 3.0))
FB.save("interval_reversed", {"expression": "x^2", "interval": [1.0, -1.0]}, expected_error="E.VALIDATION.RANGE")
FB.save("interval_empty", {"expression": "x^2", "interval": [1.0, 1.0]}, expected_error="E.VALIDATION.RANGE")
FB.save("log_negative", {"expression": "log(x)", "interval": [-1.0, 1.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
FB.save("bad_expression", {"expression": "x^^2", "interval": [0.0, 1.0]}, expected_error="E.VALIDATION.EXPRESSION")
FB.save("budget_too_small", {"expression": "cos(x)", "interval": [3.0, 4.0], "max_evaluations": 3}, expected_error="E.NUMERICAL.NO_CONVERGENCE")
FB.save("missing_interval", {"expression": "x^2"}, expected_error="E.VALIDATION.SCHEMA")
print("fminbnd golden:", FB.count)

# ============================================================================================= examples
ex = {"optim.quadratic.quadprog": {"request.json": {"H": [[1.0, -1.0], [-1.0, 2.0]], "f": [-2.0, -6.0], "A": [[1.0, 1.0], [-1.0, 2.0], [2.0, 1.0]], "b": [2.0, 2.0, 3.0]},
                                   "request.error.json": {"H": [[1.0, 2.0], [2.0, 1.0]], "f": [0.0, 0.0]}},
      "optim.scalar.fminbnd": {"request.json": {"expression": "sin(x)", "interval": [0.0, 2 * math.pi]},
                               "request.valleys.json": {"expression": "x^4 - 8*x^2 + 3*x", "interval": [-2.5, 3.0]},
                               "request.error.json": {"expression": "x^2", "interval": [1.0, -1.0]}}}
for op, reqs in ex.items():
    os.makedirs(f"examples/{op}", exist_ok=True)
    for name, req in reqs.items():
        open(f"examples/{op}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
print("examples written")
