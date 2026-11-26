"""Golden cases for nonlinear.root.fzero and nonlinear.system.fsolve (spec v0.4.13, 10.26–10.27).

Engines: scipy.optimize.brentq / fsolve (MINPACK hybrd) as everyday references, and mpmath.findroot at 50
digits (started from the reference root) as the truth. Expressions are evaluated by translating the AEOI
expression language (3.15) to Python with an mpmath namespace. NOT MATLAB output.
"""
import hashlib, json, os, platform, shutil, re
import numpy as np
import scipy
from scipy import optimize
import mpmath as mp

mp.mp.dps = 50
ENV = {"engine": "scipy.optimize (brentq / fsolve) / mpmath.findroot", "scipy": scipy.__version__, "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "note": "NOT MATLAB output"}
count = {}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def save(op, family, request, expected=None, expected_error=None, expect_warning=None, expect_postcondition_failed=None):
    d = f"golden/python-control/{op}/0.4.0"
    case = {"spec": "aeoi/0.4", "operator": op, "operator_version": "0.4.0", "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expect_warning: case["expect_warning"] = expect_warning
    if expect_postcondition_failed: case["expect_postcondition_failed"] = expect_postcondition_failed
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{d}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
    count[op] = count.get(op, 0) + 1


for op in ("nonlinear.root.fzero", "nonlinear.system.fsolve"):
    shutil.rmtree(f"golden/python-control/{op}/0.4.0", ignore_errors=True); os.makedirs(f"golden/python-control/{op}/0.4.0")

MPNS = {"sin": mp.sin, "cos": mp.cos, "tan": mp.tan, "asin": mp.asin, "acos": mp.acos, "atan": mp.atan, "atan2": mp.atan2,
        "sinh": mp.sinh, "cosh": mp.cosh, "tanh": mp.tanh, "exp": mp.exp, "log": mp.log, "log10": mp.log10, "sqrt": mp.sqrt,
        "abs": mp.fabs, "min": min, "max": max, "hypot": mp.hypot, "sign": mp.sign, "pi": mp.pi}
NPNS = {"sin": np.sin, "cos": np.cos, "tan": np.tan, "asin": np.arcsin, "acos": np.arccos, "atan": np.arctan, "atan2": np.arctan2,
        "sinh": np.sinh, "cosh": np.cosh, "tanh": np.tanh, "exp": np.exp, "log": np.log, "log10": np.log10, "sqrt": np.sqrt,
        "abs": np.abs, "min": min, "max": max, "hypot": np.hypot, "sign": np.sign, "pi": np.pi}


def fn(src, names, ns, params):
    code = compile(src.replace("^", "**"), "<expr>", "eval")
    conv = (lambda v: mp.mpf(v)) if ns is MPNS else float
    pv = {k: conv(v) for k, v in params.items()}
    return lambda *xs: eval(code, dict(ns), {**pv, **dict(zip(names, xs))})


# ------------------------------------------------------------------ fzero
def fz(family, req, roots_hint=None, expect_warning=None, expect_postcondition_failed=None):
    var = req.get("variable", "x"); params = req.get("params", {})
    f = fn(req["expression"], [var], NPNS, params); g = fn(req["expression"], [var], MPNS, params)
    if "bracket" in req:
        a, b = req["bracket"]
    else:  # MATLAB / AEOI outward search, reproduced
        x0 = req["x0"]; dx = abs(x0) / 50 if x0 else 1 / 50; a = b = x0; fa = fb = f(x0)
        while fa != 0 and fb != 0 and (fa > 0) == (fb > 0):
            dx *= np.sqrt(2); a = x0 - dx; fa = f(a)
            if fa == 0 or (fa > 0) != (fb > 0): break
            b = x0 + dx; fb = f(b)
        a, b = min(a, b), max(a, b)
    ref = optimize.brentq(f, a, b, xtol=1e-300, rtol=4 * np.finfo(float).eps, maxiter=1000)
    roots = roots_hint or [ref]
    truth = [float(mp.findroot(g, mp.mpf(r), tol=mp.mpf(10) ** -45)) if g(mp.mpf(r)) != 0 else float(r) for r in roots]
    save("nonlinear.root.fzero", family, req, {"roots_true": truth, "x_brentq": float(ref), "bracket_used": [float(a), float(b)]},
         expect_warning=expect_warning, expect_postcondition_failed=expect_postcondition_failed)


fz("sqrt2", {"expression": "x^2 - 2", "bracket": [0.0, 2.0]})
fz("dottie", {"expression": "cos(x) - x", "bracket": [0.0, 1.0]})
fz("root_at_zero", {"expression": "sin(x)", "bracket": [-1.0, 2.0]})
fz("root_at_endpoint", {"expression": "x - 1", "bracket": [1.0, 3.0]})
fz("tiny_root", {"expression": "x - 1e-20", "bracket": [-1.0, 1.0]})
fz("large_root", {"expression": "x/1e10 - 1", "bracket": [0.0, 1e11]})
fz("steep_exp", {"expression": "exp(x) - 1e10", "bracket": [0.0, 50.0]})
fz("params_cubic", {"expression": "a*x^3 - b", "params": {"a": 2.0, "b": 54.0}, "bracket": [0.0, 10.0]})
fz("named_variable", {"expression": "w*exp(w) - 1", "variable": "w", "bracket": [0.0, 1.0]})
fz("kepler", {"expression": "E - e*sin(E) - M", "variable": "E", "params": {"e": 0.9, "M": 0.3}, "bracket": [0.0, 3.2]})
fz("several_roots_in_bracket", {"expression": "sin(x)", "bracket": [0.5, 10.0]}, roots_hint=[np.pi, 2 * np.pi, 3 * np.pi])
fz("x0_search", {"expression": "cos(x) - x", "x0": 1.0}, expect_warning="W.DOMAIN.BRACKET_SEARCHED")
fz("x0_search_far", {"expression": "x^3 - 1000", "x0": 1.0}, expect_warning="W.DOMAIN.BRACKET_SEARCHED")
fz("x0_zero_start", {"expression": "x - 0.3", "x0": 0.0}, expect_warning="W.DOMAIN.BRACKET_SEARCHED")
fz("pole_not_root", {"expression": "tan(x)", "bracket": [1.0, 2.0]}, roots_hint=[float(np.pi / 2)], expect_postcondition_failed=["not_a_pole"])
save("nonlinear.root.fzero", "no_sign_change", {"expression": "x^2 + 1", "bracket": [-1.0, 1.0]}, expected_error="E.DOMAIN.NO_SIGN_CHANGE")
save("nonlinear.root.fzero", "search_fails", {"expression": "x^2 + 1", "x0": 1.0}, expected_error="E.DOMAIN.NO_SIGN_CHANGE")
save("nonlinear.root.fzero", "both_bracket_and_x0", {"expression": "x", "bracket": [-1.0, 1.0], "x0": 0.5}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("nonlinear.root.fzero", "neither", {"expression": "x"}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("nonlinear.root.fzero", "reversed_bracket", {"expression": "x", "bracket": [1.0, -1.0]}, expected_error="E.VALIDATION.RANGE")
save("nonlinear.root.fzero", "unknown_name", {"expression": "x - y", "bracket": [-1.0, 1.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("nonlinear.root.fzero", "syntax_error", {"expression": "x ^^ 2", "bracket": [-1.0, 1.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("nonlinear.root.fzero", "non_finite_endpoint", {"expression": "log(x)", "bracket": [0.0, 2.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("nonlinear.root.fzero", "missing_expression", {"bracket": [0.0, 1.0]}, expected_error="E.VALIDATION.SCHEMA")


# ------------------------------------------------------------------ fsolve
def fsv(family, req, expect_warning=None, hint=None):
    names = req["unknowns"]; params = req.get("params", {})
    fs_np = [fn(s, names, NPNS, params) for s in req["equations"]]
    fs_mp = [fn(s, names, MPNS, params) for s in req["equations"]]
    ref = optimize.fsolve(lambda v: [f(*v) for f in fs_np], req["x0"], xtol=1e-14, full_output=True)[0]
    start = hint if hint is not None else ref
    truth = mp.findroot(fs_mp if len(names) > 1 else fs_mp[0], [mp.mpf(v) for v in start] if len(names) > 1 else mp.mpf(start[0]), tol=mp.mpf(10) ** -40, maxsteps=200)
    tv = [float(v) for v in (truth if len(names) > 1 else [truth])]
    scipy_res = max(abs(f(*ref)) for f in fs_np)
    save("nonlinear.system.fsolve", family, req, {"x_true": tv, "x_scipy": [float(v) for v in ref], "scipy_residual_inf": float(scipy_res)}, expect_warning=expect_warning)


fsv("circle_line", {"unknowns": ["x", "y"], "equations": ["x^2 + y^2 - 4", "x - y"], "x0": [1.0, 0.5]})
fsv("scalar_cubic", {"unknowns": ["x"], "equations": ["x^3 - 2*x - 5"], "x0": [2.0]})
fsv("three_polynomials", {"unknowns": ["x", "y", "z"], "equations": ["x + y + z - 6", "x*y + y*z + z*x - 11", "x*y*z - 6"], "x0": [0.5, 1.8, 3.5]})
fsv("exp_system", {"unknowns": ["u", "v"], "equations": ["exp(u) + v - 3", "u - sin(v)"], "x0": [0.5, 1.0]})
fsv("params_equilibrium", {"unknowns": ["x", "y"], "equations": ["a*x - b*x*y", "-c*y + d*x*y"], "params": {"a": 1.0, "b": 0.5, "c": 0.75, "d": 0.25}, "x0": [2.0, 1.5]})
fsv("rosenbrock_gradient", {"unknowns": ["x", "y"], "equations": ["-2*(1 - x) - 400*x*(y - x^2)", "200*(y - x^2)"], "x0": [0.5, 0.2]}, hint=[1.0, 1.0])
# from the classic start ½‖F‖² creeps toward a non-root local minimum: scipy hybr and lm fail too; we hit the iteration limit
save("nonlinear.system.fsolve", "rosenbrock_gradient_stalls", {"unknowns": ["x", "y"], "equations": ["-2*(1 - x) - 400*x*(y - x^2)", "200*(y - x^2)"], "x0": [-1.2, 1.0]}, expected_error="E.NUMERICAL.NO_CONVERGENCE")
fsv("far_start", {"unknowns": ["x", "y"], "equations": ["x^2 + y^2 - 4", "x - y"], "x0": [100.0, -50.0]})
nb = 20
eqs = [f"(3 - 2*x{i})*x{i} - {'x' + str(i - 1) if i > 0 else '0'} - 2*{'x' + str(i + 1) if i < nb - 1 else '0'} + 1" for i in range(nb)]
fsv("broyden_tridiagonal_20", {"unknowns": [f"x{i}" for i in range(nb)], "equations": eqs, "x0": [-1.0] * nb})
fsv("double_root_warns", {"unknowns": ["x"], "equations": ["(x - 1)^2"], "x0": [3.0]}, expect_warning="W.NUMERICAL.ILL_CONDITIONED")
save("nonlinear.system.fsolve", "no_real_root", {"unknowns": ["x"], "equations": ["x^2 + 1"], "x0": [0.5]}, expected_error="E.NUMERICAL.LOCAL_MINIMUM")
save("nonlinear.system.fsolve", "count_mismatch", {"unknowns": ["x", "y"], "equations": ["x + y"], "x0": [0.0, 0.0]}, expected_error="E.VALIDATION.SHAPE")
save("nonlinear.system.fsolve", "x0_length", {"unknowns": ["x", "y"], "equations": ["x", "y"], "x0": [0.0]}, expected_error="E.VALIDATION.SHAPE")
save("nonlinear.system.fsolve", "param_clash", {"unknowns": ["x"], "equations": ["x - a"], "params": {"x": 1.0}, "x0": [0.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("nonlinear.system.fsolve", "unknown_name", {"unknowns": ["x"], "equations": ["x - q"], "x0": [0.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("nonlinear.system.fsolve", "reserved_name", {"unknowns": ["pi"], "equations": ["pi - 1"], "x0": [0.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("nonlinear.system.fsolve", "non_finite_at_x0", {"unknowns": ["x"], "equations": ["log(x) - 1"], "x0": [-1.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("nonlinear.system.fsolve", "missing_x0", {"unknowns": ["x"], "equations": ["x"]}, expected_error="E.VALIDATION.SCHEMA")
print("roots golden:", count)

for op, ex in {"nonlinear.root.fzero": {"request.json": {"expression": "E - e*sin(E) - M", "variable": "E", "params": {"e": 0.9, "M": 0.3}, "bracket": [0.0, 3.2]},
                                          "request.x0.json": {"expression": "cos(x) - x", "x0": 1.0},
                                          "request.pole.json": {"expression": "tan(x)", "bracket": [1.0, 2.0]},
                                          "request.error.json": {"expression": "x^2 + 1", "bracket": [-1.0, 1.0]}},
               "nonlinear.system.fsolve": {"request.json": {"unknowns": ["x", "y"], "equations": ["a*x - b*x*y", "-c*y + d*x*y"], "params": {"a": 1.0, "b": 0.5, "c": 0.75, "d": 0.25}, "x0": [2.0, 1.5]},
                                           "request.double_root.json": {"unknowns": ["x"], "equations": ["(x - 1)^2"], "x0": [3.0]},
                                           "request.error.json": {"unknowns": ["x"], "equations": ["x^2 + 1"], "x0": [0.5]}}}.items():
    E = f"examples/{op}"; os.makedirs(E, exist_ok=True)
    for name, req in ex.items():
        open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
ref = {"note": "mpmath.findroot 50 digits; NOT MATLAB output", "request.json": {"x_true": float(mp.findroot(lambda E: E - mp.mpf("0.9") * mp.sin(E) - mp.mpf("0.3"), 1.0))}}
open("examples/nonlinear.root.fzero/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
open("examples/nonlinear.system.fsolve/reference.python-control.json", "w").write(json.dumps({"note": "exact: x = c/d = 3, y = a/b = 2; NOT MATLAB output", "request.json": {"x_true": [3.0, 2.0]}}, indent=1) + "\n")
print("roots examples written")
