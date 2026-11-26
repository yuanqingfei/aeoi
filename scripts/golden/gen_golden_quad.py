"""Golden cases for calculus.quadrature.integral and calculus.quadrature.trapz (spec v0.4.15, 10.31–10.32).

Engines: scipy.integrate.quad (QUADPACK) and numpy.trapezoid as everyday references; mpmath.quad at 50 digits
(integral) and exact rational arithmetic on the stored data (trapz) as the truth. NOT MATLAB output.
"""
import hashlib, json, os, platform, shutil
from fractions import Fraction
import numpy as np
import scipy
from scipy import integrate
import mpmath as mp

mp.mp.dps = 50
ENV = {"engine": "scipy.integrate.quad / numpy.trapezoid / mpmath.quad", "scipy": scipy.__version__, "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "note": "NOT MATLAB output"}
OPS = ("calculus.quadrature.integral", "calculus.quadrature.trapz")
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


MP = {"sin": mp.sin, "cos": mp.cos, "exp": mp.exp, "log": mp.log, "sqrt": mp.sqrt, "pi": mp.pi, "abs": mp.fabs}
NPN = {"sin": np.sin, "cos": np.cos, "exp": np.exp, "log": np.log, "sqrt": np.sqrt, "pi": np.pi, "abs": np.abs}


def lim(v, m):
    return (m.inf if v == "Inf" else -m.inf) if isinstance(v, str) else v


def intg(family, req, truth_exact=None):
    var = req.get("variable", "x"); P = req.get("params", {})
    code = compile(req["expression"].replace("^", "**"), "<e>", "eval")
    fm = lambda x: eval(code, dict(MP), {**{k: mp.mpf(v) for k, v in P.items()}, var: x})
    fn = lambda x: eval(code, dict(NPN), {**P, var: x})
    a, b = req["limits"]
    pts = [lim(a, mp), lim(b, mp)]
    truth = mp.quad(fm, pts, maxdegree=10)
    if truth_exact is not None:  # cross-check (AGENTS.md rule 6)
        assert abs(truth - truth_exact) < mp.mpf(10) ** -20, (family, truth, truth_exact)
        truth = mp.mpf(truth_exact)
    ref = integrate.quad(fn, lim(a, np), lim(b, np), epsabs=1e-14, epsrel=1e-13, limit=500)[0]
    save("calculus.quadrature.integral", family, req, {"value_true": float(truth), "value_scipy": float(ref), "rel_tol": req.get("rel_tol", 1e-10), "abs_tol": req.get("abs_tol", 1e-12)})


intg("cubic", {"expression": "x^3", "limits": [0.0, 2.0]}, 4)
intg("gaussian_whole_line", {"expression": "exp(-x^2)", "limits": ["-Inf", "Inf"]}, mp.sqrt(mp.pi))
intg("endpoint_singularity", {"expression": "1/sqrt(x)", "limits": [0.0, 1.0]}, 2)
intg("log_singularity", {"expression": "log(x)", "limits": [0.0, 1.0]}, -1)
intg("sinc", {"expression": "sin(x)/x", "limits": [0.0, 10.0]})
intg("exp_half_line", {"expression": "exp(-x)", "limits": [0.0, "Inf"]}, 1)
intg("lorentzian", {"expression": "1/(1 + x^2)", "limits": ["-Inf", "Inf"]}, mp.pi)
intg("oscillatory", {"expression": "cos(50*x)", "limits": [0.0, 1.0]}, mp.sin(50) / 50)
intg("reversed_limits", {"expression": "sin(x)", "limits": [3.141592653589793, 0.0]})
intg("params_rate", {"expression": "exp(-a*t)", "variable": "t", "params": {"a": 4.0}, "limits": [0.0, "Inf"]}, mp.mpf(1) / 4)
intg("tiny_value_abs_tol", {"expression": "exp(-x)", "limits": [20.0, 30.0]})
intg("tiny_value_tight_abs_tol", {"expression": "exp(-x)", "limits": [20.0, 30.0], "abs_tol": 0.0}, mp.exp(-20) - mp.exp(-30))
intg("lower_half_line", {"expression": "exp(x)", "limits": ["-Inf", 0.0]}, 1)
save("calculus.quadrature.integral", "divergent_interior", {"expression": "1/x^2", "limits": [-1.0, 1.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("calculus.quadrature.integral", "divergent_half_line", {"expression": "1/x", "limits": [1.0, "Inf"]}, expected_error="E.NUMERICAL.NO_CONVERGENCE")
save("calculus.quadrature.integral", "nan_integrand", {"expression": "sqrt(x)", "limits": [-1.0, 0.0]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
save("calculus.quadrature.integral", "same_infinity", {"expression": "x", "limits": ["Inf", "Inf"]}, expected_error="E.VALIDATION.RANGE")
save("calculus.quadrature.integral", "bad_expression", {"expression": "x ^", "limits": [0.0, 1.0]}, expected_error="E.VALIDATION.EXPRESSION")
save("calculus.quadrature.integral", "missing_limits", {"expression": "x"}, expected_error="E.VALIDATION.SCHEMA")


def enc(x):
    x = np.asarray(x, float); return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def trz(family, req):
    Y = np.atleast_2d(np.array(req["y"]["data"]).reshape(req["y"]["shape"]))
    n = Y.shape[1]
    xs = [Fraction(v) for v in req["x"]["data"]] if "x" in req else [Fraction(req["dx"]) * k for k in range(n)]
    vals, cums = [], []
    for row in Y:
        r = [Fraction(float(v)) for v in row]; acc = Fraction(0); cum = [0.0]
        for k in range(n - 1):
            acc += (xs[k + 1] - xs[k]) * (r[k] + r[k + 1]) / 2; cum.append(float(acc))
        vals.append(float(acc)); cums.append(cum)
    ref = [float(np.trapezoid(row, x=np.array(req["x"]["data"]) if "x" in req else None, dx=req.get("dx", 1.0))) for row in Y]
    save("calculus.quadrature.trapz", family, req, {"value_true": vals, "cumulative_true": cums, "value_numpy": ref})


rng = np.random.default_rng(3)
trz("uniform_dx", {"y": enc([1.0, 4.0, 9.0, 16.0]), "dx": 0.5})
trz("nonuniform_x", {"y": enc([0.0, 1.0, 0.5, 2.0]), "x": enc([0.0, 0.1, 0.7, 1.0])})
trz("multichannel", {"y": enc(rng.normal(size=(3, 20))), "dx": 0.1})
trz("cumulative", {"y": enc(np.sin(np.linspace(0, np.pi, 11))), "x": enc(np.linspace(0, np.pi, 11)), "cumulative": True})
trz("single_point", {"y": enc([3.0]), "dx": 1.0})
trz("decreasing_x", {"y": enc([1.0, 2.0, 3.0]), "x": enc([2.0, 1.0, 0.0])})
big = 1e8 + rng.normal(size=100000)
trz("cancellation_large_n", {"y": enc(np.r_[big[:50000], -big[:50000]]), "dx": 1.0})
save("calculus.quadrature.trapz", "both_x_and_dx", {"y": enc([1.0, 2.0]), "x": enc([0.0, 1.0]), "dx": 1.0}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("calculus.quadrature.trapz", "no_spacing", {"y": enc([1.0, 2.0])}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("calculus.quadrature.trapz", "x_length", {"y": enc([1.0, 2.0, 3.0]), "x": enc([0.0, 1.0])}, expected_error="E.VALIDATION.SHAPE")
save("calculus.quadrature.trapz", "y_3d", {"y": enc(np.zeros((2, 2, 2))), "dx": 1.0}, expected_error="E.VALIDATION.SHAPE")
save("calculus.quadrature.trapz", "nan_y", {"y": {"dtype": "f64", "shape": [2], "data": [1.0, "NaN"]}, "dx": 1.0}, expected_error="E.VALIDATION.NON_FINITE")
print("quad golden:", count)
ex = {"calculus.quadrature.integral": {"request.json": {"expression": "exp(-x^2)", "limits": ["-Inf", "Inf"]},
                                        "request.singular.json": {"expression": "1/sqrt(x)", "limits": [0.0, 1.0]},
                                        "request.error.json": {"expression": "1/x^2", "limits": [-1.0, 1.0]}},
      "calculus.quadrature.trapz": {"request.json": {"y": enc(np.sin(np.linspace(0, np.pi, 11))), "x": enc(np.linspace(0, np.pi, 11)), "cumulative": True},
                                    "request.error.json": {"y": enc([1.0, 2.0, 3.0])}}}
for op, d in ex.items():
    os.makedirs(f"examples/{op}", exist_ok=True)
    for name, req in d.items():
        open(f"examples/{op}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
print("quad examples written")
