"""Golden cases for optim.linear.linprog, optim.local.fmincon, optim.local.fminsearch (spec v0.4.14, 10.28–10.30).

Engines: scipy.optimize.linprog (HiGHS), scipy.optimize.minimize (SLSQP at ftol 1e-14 / Nelder-Mead with
MATLAB's tolerances) as everyday references; analytic optima where the problem has one (truth). Expressions
use the 3.15 syntax, evaluated here through numpy. NOT MATLAB output.
"""
import hashlib, json, os, platform, shutil
import numpy as np
import scipy
from scipy import optimize

ENV = {"engine": "scipy.optimize (linprog HiGHS / minimize SLSQP, Nelder-Mead)", "scipy": scipy.__version__, "numpy": np.__version__, "python": platform.python_version(), "note": "NOT MATLAB output"}
OPS = ("optim.linear.linprog", "optim.local.fmincon", "optim.local.fminsearch")
for op in OPS:
    shutil.rmtree(f"golden/python-control/{op}/0.4.0", ignore_errors=True); os.makedirs(f"golden/python-control/{op}/0.4.0")
count = {}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def save(op, family, request, expected=None, expected_error=None):
    case = {"spec": "aeoi/0.4", "operator": op, "operator_version": "0.4.0", "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"golden/python-control/{op}/0.4.0/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
    count[op] = count.get(op, 0) + 1


NS = {"sin": np.sin, "cos": np.cos, "cosh": np.cosh, "tan": np.tan, "exp": np.exp, "log": np.log, "sqrt": np.sqrt, "abs": np.abs, "pi": np.pi}


def fn(src, names, params):
    code = compile(src.replace("^", "**"), "<expr>", "eval")
    return lambda v: eval(code, dict(NS), {**params, **dict(zip(names, v))})


def bnd(v):
    return [None if (isinstance(x, str)) else x for x in v]


# ------------------------------------------------------------------ linprog
def lpc(family, req, x_exact=None, unique=True):
    n = len(req["c"])
    lb = req.get("lb", ["-Inf"] * n); ub = req.get("ub", ["Inf"] * n)
    r = optimize.linprog(req["c"], A_ub=req.get("A"), b_ub=req.get("b"), A_eq=req.get("Aeq"), b_eq=req.get("beq"), bounds=list(zip(bnd(lb), bnd(ub))), method="highs")
    assert r.status == 0, (family, r.message)
    if x_exact is not None and unique:  # cross-check the hand-derived vertex against HiGHS
        assert np.max(np.abs(r.x - np.array(x_exact))) < 1e-9, (family, r.x, x_exact)
    save("optim.linear.linprog", family, req, {"fval": float(r.fun), "x_highs": [float(v) for v in r.x], "x_exact": x_exact, "unique": unique})


lpc("production", {"c": [-1.0, -1.0], "A": [[1.0, 2.0], [3.0, 1.0]], "b": [4.0, 6.0], "lb": [0.0, 0.0]}, x_exact=[1.6, 1.2])
lpc("equality_and_bounds", {"c": [2.0, 3.0, 1.0], "Aeq": [[1.0, 1.0, 1.0]], "beq": [10.0], "A": [[-1.0, 0.0, 0.0]], "b": [-2.0], "lb": [0.0, 0.0, 0.0], "ub": [8.0, 8.0, 5.0]}, x_exact=[5.0, 0.0, 5.0])
lpc("free_variables", {"c": [1.0, 1.0], "A": [[-1.0, 1.0], [-1.0, -1.0]], "b": [1.0, 3.0]}, x_exact=[-1.0, -2.0], unique=False)
lpc("upper_bound_only", {"c": [-1.0, -2.0], "ub": [3.0, 4.0], "A": [[1.0, 1.0]], "b": [5.0]}, x_exact=[1.0, 4.0])
lpc("transportation", {"c": [4, 6, 9, 5, 3, 8, 7, 4, 6], "Aeq": [[1, 1, 1, 0, 0, 0, 0, 0, 0], [0, 0, 0, 1, 1, 1, 0, 0, 0], [0, 0, 0, 0, 0, 0, 1, 1, 1], [1, 0, 0, 1, 0, 0, 1, 0, 0], [0, 1, 0, 0, 1, 0, 0, 1, 0], [0, 0, 1, 0, 0, 1, 0, 0, 1]], "beq": [20, 30, 25, 25, 25, 25], "lb": [0.0] * 9}, unique=False)
lpc("redundant_equalities", {"c": [1.0, 2.0], "Aeq": [[1.0, 1.0], [2.0, 2.0]], "beq": [1.0, 2.0], "lb": [0.0, 0.0]}, x_exact=[1.0, 0.0])
km = 4
lpc("klee_minty_4", {"c": [-(2.0 ** (km - 1 - j)) for j in range(km)], "A": [[(2.0 ** (i - j + 1)) if j < i else (1.0 if j == i else 0.0) for j in range(km)] for i in range(km)], "b": [5.0 ** (i + 1) for i in range(km)], "lb": [0.0] * km}, x_exact=[0.0, 0.0, 0.0, 625.0])
lpc("degenerate_vertex", {"c": [-1.0, -1.0], "A": [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]], "b": [1.0, 1.0, 2.0], "lb": [0.0, 0.0]}, x_exact=[1.0, 1.0])
rng = np.random.default_rng(7)
A = rng.uniform(0, 1, (8, 12)); lpc("random_dense", {"c": [float(v) for v in -rng.uniform(0, 1, 12)], "A": A.round(6).tolist(), "b": [1.0] * 8, "lb": [0.0] * 12})
save("optim.linear.linprog", "infeasible", {"c": [1.0], "A": [[1.0]], "b": [-1.0], "lb": [0.0]}, expected_error="E.DOMAIN.INFEASIBLE")
save("optim.linear.linprog", "unbounded", {"c": [-1.0, 0.0], "A": [[0.0, 1.0]], "b": [1.0], "lb": [0.0, 0.0]}, expected_error="E.DOMAIN.UNBOUNDED")
save("optim.linear.linprog", "unbounded_no_default_bounds", {"c": [1.0], "A": [[1.0]], "b": [4.0]}, expected_error="E.DOMAIN.UNBOUNDED")
save("optim.linear.linprog", "bounds_inconsistent", {"c": [1.0], "lb": [2.0], "ub": [1.0]}, expected_error="E.DOMAIN.INFEASIBLE")
save("optim.linear.linprog", "row_length", {"c": [1.0, 1.0], "A": [[1.0]], "b": [1.0]}, expected_error="E.VALIDATION.SHAPE")
save("optim.linear.linprog", "b_mismatch", {"c": [1.0], "A": [[1.0]], "b": [1.0, 2.0]}, expected_error="E.VALIDATION.SHAPE")
save("optim.linear.linprog", "missing_c", {"A": [[1.0]], "b": [1.0]}, expected_error="E.VALIDATION.SCHEMA")


# ------------------------------------------------------------------ fmincon
def fmc(family, req, x_exact=None):
    names = req["unknowns"]; P = req.get("params", {})
    f = fn(req["objective"], names, P)
    cons = [{"type": "ineq", "fun": (lambda g: lambda v: -g(v))(fn(s, names, P))} for s in req.get("nonlinear_ineq", [])]
    cons += [{"type": "eq", "fun": fn(s, names, P)} for s in req.get("nonlinear_eq", [])]
    if "A" in req:
        A = np.array(req["A"]); b = np.array(req["b"]); cons.append({"type": "ineq", "fun": lambda v: b - A @ v})
    if "Aeq" in req:
        Ae = np.array(req["Aeq"]); be = np.array(req["beq"]); cons.append({"type": "eq", "fun": lambda v: Ae @ v - be})
    bounds = list(zip(bnd(req.get("lb", ["-Inf"] * len(names))), bnd(req.get("ub", ["Inf"] * len(names)))))
    r = optimize.minimize(f, req["x0"], method="SLSQP", constraints=cons, bounds=bounds, options={"ftol": 1e-14, "maxiter": 1000})
    if x_exact is not None:  # cross-check the hand-derived optimum against SLSQP (AGENTS.md rule 6)
        assert np.max(np.abs(r.x - np.array(x_exact))) < 1e-6, (family, r.x, x_exact)
    save("optim.local.fmincon", family, req, {"x_slsqp": [float(v) for v in r.x], "fval_slsqp": float(r.fun), "x_exact": x_exact})


fmc("disk_linear_objective", {"unknowns": ["x", "y"], "objective": "x + y", "nonlinear_ineq": ["x^2 + y^2 - 2"], "x0": [0.5, 0.0]}, x_exact=[-1.0, -1.0])
fmc("sphere_on_plane", {"unknowns": ["x", "y", "z"], "objective": "x^2 + y^2 + z^2", "Aeq": [[1.0, 1.0, 1.0]], "beq": [1.0], "x0": [1.0, 0.0, 0.0]}, x_exact=[1 / 3, 1 / 3, 1 / 3])
fmc("projection_with_bounds", {"unknowns": ["x", "y"], "objective": "(x - 3)^2 + (y - 2)^2", "A": [[1.0, 1.0]], "b": [4.0], "lb": [0.0, 0.0], "x0": [0.0, 0.0]}, x_exact=[2.5, 1.5])
fmc("hs071", {"unknowns": ["x1", "x2", "x3", "x4"], "objective": "x1*x4*(x1 + x2 + x3) + x3", "nonlinear_ineq": ["25 - x1*x2*x3*x4"], "nonlinear_eq": ["x1^2 + x2^2 + x3^2 + x4^2 - 40"], "lb": [1.0] * 4, "ub": [5.0] * 4, "x0": [1.0, 5.0, 5.0, 1.0]})
fmc("rosenbrock_bounded", {"unknowns": ["x", "y"], "objective": "100*(y - x^2)^2 + (1 - x)^2", "ub": [0.5, "Inf"], "x0": [-1.2, 1.0]})
fmc("unconstrained_smooth", {"unknowns": ["x", "y"], "objective": "exp(x - 1) - x + (y + 2)^2", "x0": [0.0, 0.0]}, x_exact=[1.0, -2.0])
fmc("params_equality", {"unknowns": ["x", "y"], "objective": "a*x^2 + b*y^2", "nonlinear_eq": ["x*y - c"], "params": {"a": 1.0, "b": 4.0, "c": 2.0}, "x0": [1.0, 1.0]}, x_exact=[2.0, 1.0])
save("optim.local.fmincon", "infeasible_nonlinear", {"unknowns": ["x", "y"], "objective": "x + y", "nonlinear_ineq": ["x^2 + y^2 + 1"], "x0": [0.0, 0.0]}, expected_error="E.NUMERICAL.NO_CONVERGENCE")
save("optim.local.fmincon", "bounds_inconsistent", {"unknowns": ["x"], "objective": "x", "lb": [1.0], "ub": [0.0], "x0": [0.0]}, expected_error="E.DOMAIN.INFEASIBLE")
save("optim.local.fmincon", "bad_expression", {"unknowns": ["x"], "objective": "x +", "x0": [0.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("optim.local.fmincon", "x0_length", {"unknowns": ["x", "y"], "objective": "x + y", "x0": [0.0]}, expected_error="E.VALIDATION.SHAPE")
save("optim.local.fmincon", "non_finite_at_x0", {"unknowns": ["x"], "objective": "log(x)", "x0": [-1.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("optim.local.fmincon", "missing_objective", {"unknowns": ["x"], "x0": [0.0]}, expected_error="E.VALIDATION.SCHEMA")


# ------------------------------------------------------------------ fminsearch
def fms(family, req, x_exact):
    f = fn(req["objective"], req["unknowns"], req.get("params", {}))
    r = optimize.minimize(f, req["x0"], method="Nelder-Mead", options={"xatol": req.get("tol_x", 1e-4), "fatol": req.get("tol_fun", 1e-4), "maxiter": 200 * len(req["x0"])})
    # cross-check the hand-derived optimum (AGENTS.md rule 6): a tight independent solve must land on it
    rt = optimize.minimize(f, x_exact, method="Nelder-Mead", options={"xatol": 1e-12, "fatol": 1e-15, "maxiter": 20000})
    assert np.max(np.abs(rt.x - np.array(x_exact))) < 1e-6, (family, rt.x, x_exact)
    save("optim.local.fminsearch", family, req, {"x_scipy": [float(v) for v in r.x], "x_exact": x_exact, "tol_x": req.get("tol_x", 1e-4)})


fms("rosenbrock", {"unknowns": ["x", "y"], "objective": "100*(y - x^2)^2 + (1 - x)^2", "x0": [-1.2, 1.0]}, [1.0, 1.0])
fms("nonsmooth_abs", {"unknowns": ["x", "y"], "objective": "abs(x - 1) + 2*abs(y + 2)", "x0": [0.0, 0.0]}, [1.0, -2.0])
fms("quadratic_3d", {"unknowns": ["a", "b", "c"], "objective": "(a - 1)^2 + 2*(b + 1)^2 + 3*(c - 0.5)^2 + a*b", "x0": [0.0, 0.0, 0.0]}, [12 / 7, -10 / 7, 0.5])
fms("tight_tolerance", {"unknowns": ["x"], "objective": "(x - 2)^2", "x0": [0.0], "tol_x": 1e-10, "tol_fun": 1e-14}, [2.0])
fms("params_scalar", {"unknowns": ["x"], "objective": "cosh(k*(x - s))", "params": {"k": 2.0, "s": -0.75}, "x0": [1.0]}, [-0.75])
save("optim.local.fminsearch", "iteration_limit", {"unknowns": ["x", "y"], "objective": "100*(y - x^2)^2 + (1 - x)^2", "x0": [-1.2, 1.0], "max_iterations": 10}, expected_error="E.NUMERICAL.NO_CONVERGENCE")
save("optim.local.fminsearch", "bad_name", {"unknowns": ["sin"], "objective": "sin", "x0": [0.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("optim.local.fminsearch", "x0_length", {"unknowns": ["x"], "objective": "x^2", "x0": [0.0, 1.0]}, expected_error="E.VALIDATION.SHAPE")
save("optim.local.fminsearch", "non_finite_start", {"unknowns": ["x"], "objective": "log(x)", "x0": [-1.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("optim.local.fminsearch", "missing_x0", {"unknowns": ["x"], "objective": "x^2"}, expected_error="E.VALIDATION.SCHEMA")
print("optim golden:", count)

ex = {"optim.linear.linprog": {"request.json": {"c": [-1.0, -1.0], "A": [[1.0, 2.0], [3.0, 1.0]], "b": [4.0, 6.0], "lb": [0.0, 0.0]},
                               "request.error.json": {"c": [1.0], "A": [[1.0]], "b": [4.0]}},
      "optim.local.fmincon": {"request.json": {"unknowns": ["x1", "x2", "x3", "x4"], "objective": "x1*x4*(x1 + x2 + x3) + x3", "nonlinear_ineq": ["25 - x1*x2*x3*x4"], "nonlinear_eq": ["x1^2 + x2^2 + x3^2 + x4^2 - 40"], "lb": [1.0] * 4, "ub": [5.0] * 4, "x0": [1.0, 5.0, 5.0, 1.0]},
                              "request.error.json": {"unknowns": ["x", "y"], "objective": "x + y", "nonlinear_ineq": ["x^2 + y^2 + 1"], "x0": [0.0, 0.0]}},
      "optim.local.fminsearch": {"request.json": {"unknowns": ["x", "y"], "objective": "abs(x - 1) + 2*abs(y + 2)", "x0": [0.0, 0.0]},
                                 "request.error.json": {"unknowns": ["x", "y"], "objective": "100*(y - x^2)^2 + (1 - x)^2", "x0": [-1.2, 1.0], "max_iterations": 10}}}
for op, d in ex.items():
    E = f"examples/{op}"; os.makedirs(E, exist_ok=True)
    for name, req in d.items():
        open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
print("optim examples written")
