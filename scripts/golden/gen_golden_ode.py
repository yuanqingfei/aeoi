"""Golden cases for ode.ivp.solve (spec v0.4.8, 10.18) and the expression language (3.15).

The right-hand sides are parsed and evaluated by an independent Python implementation of the 3.15 grammar
(recursive descent; math-module functions with IEEE results for domain errors). References:
  * scipy.integrate.solve_ivp(method="RK45") with the same rtol, atol, t_eval, first_step, max_step: the spec
    copies RK45's formulas, so this is the step-for-step reference ("expected");
  * the truth: the analytic solution where there is one, else DOP853 at rtol 1e-13, atol 1e-14 ("truth"),
    to measure the global error actually achieved.
NOT MATLAB output (MATLAB ode45 is the same method with a max-norm error test and its own step logic).
"""
import hashlib, json, math, os, platform, re, shutil
import numpy as np
import scipy
from scipy.integrate import solve_ivp

OP, VER = "ode.ivp.solve", "0.4.0"
ENV = {"engine": "scipy.integrate.solve_ivp RK45 (truth: analytic or DOP853 rtol 1e-13)", "scipy": scipy.__version__, "numpy": np.__version__,
       "python": platform.python_version(), "expression_evaluator": "independent Python recursive-descent parser of spec 3.15", "note": "NOT MATLAB output"}
D = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(D, ignore_errors=True); os.makedirs(D)

# ---------------------------------------------------------------- 3.15 expression language, independently
FUNCS = {"sin": 1, "cos": 1, "tan": 1, "asin": 1, "acos": 1, "atan": 1, "atan2": 2, "sinh": 1, "cosh": 1, "tanh": 1, "exp": 1,
         "log": 1, "log10": 1, "sqrt": 1, "abs": 1, "min": 2, "max": 2, "hypot": 2, "sign": 1}
INF, NAN = float("inf"), float("nan")


def _safe(fn):
    def g(*a):
        try:
            return fn(*a)
        except ValueError:
            return NAN
        except OverflowError:
            return INF
    return g


def _log(x):
    return -INF if x == 0 else (NAN if x < 0 or x != x else math.log(x))


def _log10(x):
    return -INF if x == 0 else (NAN if x < 0 or x != x else math.log10(x))


def _pow(a, b):
    try:
        return math.pow(a, b)
    except ValueError:
        return NAN
    except OverflowError:
        return INF if a > 0 or float(b).is_integer() and int(b) % 2 == 0 else -INF


def _minmax(f):
    return lambda a, b: NAN if a != a or b != b else f(a, b)


IMPL = {"sin": _safe(math.sin), "cos": _safe(math.cos), "tan": _safe(math.tan), "asin": _safe(math.asin), "acos": _safe(math.acos),
        "atan": math.atan, "atan2": math.atan2, "sinh": _safe(math.sinh), "cosh": _safe(math.cosh), "tanh": math.tanh, "exp": _safe(math.exp),
        "log": _log, "log10": _log10, "sqrt": lambda x: NAN if x < 0 or x != x else math.sqrt(x), "abs": abs,
        "min": _minmax(min), "max": _minmax(max), "hypot": math.hypot, "sign": lambda x: NAN if x != x else (1.0 if x > 0 else -1.0 if x < 0 else 0.0)}
TOK = re.compile(r"\s*(?:(\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?)|([A-Za-z_][A-Za-z0-9_]*)|(.))")


def parse(src, names):
    toks = [(m.group(1), m.group(2), m.group(3)) for m in TOK.finditer(src) if m.group(0).strip()] + [(None, None, "END")]
    i = [0]
    def peek(): return toks[i[0]]
    def take(): t = toks[i[0]]; i[0] += 1; return t
    def sum_():
        l = prod()
        while peek()[2] in ("+", "-"):
            op = take()[2]; r = prod(); l = ("+" if op == "+" else "-", l, r)
        return l
    def prod():
        l = unary()
        while peek()[2] in ("*", "/"):
            op = take()[2]; r = unary(); l = (op, l, r)
        return l
    def unary():
        if peek()[2] == "-": take(); return ("neg", unary())
        if peek()[2] == "+": take(); return unary()
        return power()
    def power():
        b = primary()
        if peek()[2] == "^": take(); return ("^", b, unary())
        return b
    def primary():
        num, name, op = take()
        if num is not None: return ("num", float(num))
        if op == "(":
            e = sum_(); assert take()[2] == ")"; return e
        if name is not None:
            if peek()[2] == "(":
                take(); args = [sum_()]
                while peek()[2] == ",": take(); args.append(sum_())
                assert take()[2] == ")" and len(args) == FUNCS[name]
                return ("call", name, args)
            if name in names: return ("var", names.index(name))
            if name == "pi": return ("num", math.pi)
        raise SyntaxError(src)
    e = sum_(); assert peek()[2] == "END", src
    return e


def ev(e, v):
    k = e[0]
    if k == "num": return e[1]
    if k == "var": return v[e[1]]
    if k == "neg": return -ev(e[1], v)
    if k == "call": return IMPL[e[1]](*[ev(a, v) for a in e[2]])
    a, b = ev(e[1], v), ev(e[2], v)
    if k == "+": return a + b
    if k == "-": return a - b
    if k == "*": return a * b
    if k == "/": return a / b if b != 0 else (NAN if a == 0 or a != a else math.copysign(INF, a) * math.copysign(1.0, b))
    if k == "^": return _pow(a, b)


def rhs_fun(states, rhs, params):
    names = ["t"] + list(states) + list(params)
    asts = [parse(r, names) for r in rhs]
    pv = [float(params[p]) for p in params]
    return lambda t, y: np.array([ev(a, [t] + list(y) + pv) for a in asts])


# ---------------------------------------------------------------- cases
def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


n_saved = 0
def save(family, request, expected=None, expected_error=None, expect_warning=None, expect_postcondition_failed=None):
    global n_saved
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expect_warning: case["expect_warning"] = expect_warning
    if expect_postcondition_failed: case["expect_postcondition_failed"] = expect_postcondition_failed   # honest flag, not a defect
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{D}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False); n_saved += 1


def case(family, states, rhs, initial, t_span, params=None, t_eval=None, n_out=None, rtol=None, atol=None, max_step=None, first_step=None, exact=None, expect_warning=None):
    req = {"states": states, "rhs": rhs, "initial": initial, "t_span": t_span}
    if params: req["params"] = params
    if t_eval is not None: req["t_eval"] = t_eval
    if n_out is not None: req["n_out"] = n_out
    for k, v in (("rtol", rtol), ("atol", atol), ("max_step", max_step), ("first_step", first_step)):
        if v is not None: req[k] = v
    f = rhs_fun(states, rhs, params or {})
    te = np.array(t_eval) if t_eval is not None else np.linspace(t_span[0], t_span[1], n_out or 101)
    y0 = [float(initial[s]) for s in states]
    kw = {}
    if max_step is not None: kw["max_step"] = max_step
    if first_step is not None: kw["first_step"] = first_step
    r = solve_ivp(f, t_span, y0, method="RK45", t_eval=te, rtol=rtol or 1e-3, atol=atol or 1e-6, **kw)
    assert r.success, (family, r.message)
    if exact is not None:
        truth = np.array([[exact(t)[i] for t in te] for i in range(len(states))])
        # cross-check the hand-written formula (and the parser) against an independent high-accuracy solve
        tr = solve_ivp(f, t_span, y0, method="DOP853", t_eval=te, rtol=1e-13, atol=1e-14)
        assert np.max(np.abs(tr.y - truth)) <= 1e-9 * max(1.0, np.max(np.abs(truth))), (family, "analytic truth disagrees with DOP853")
    else:
        tr = solve_ivp(f, t_span, y0, method="DOP853", t_eval=te, rtol=1e-13, atol=1e-14)
        assert tr.success, (family, "truth", tr.message)
        truth = tr.y
    exp = {"t": [float(v) for v in te], "y": [float(v) for v in r.y.ravel()], "shape": list(r.y.shape), "truth": [float(v) for v in truth.ravel()],
           "truth_kind": "analytic" if exact is not None else "DOP853 rtol 1e-13", "scipy_nfev": int(r.nfev),
           "scipy_global_error_max_abs": float(np.max(np.abs(r.y - truth)))}
    # OD-5 on the reference: re-solve at tol/10 and predict the tolerance_consistent verdict independently
    rf = solve_ivp(f, t_span, y0, method="RK45", t_eval=te, rtol=(rtol or 1e-3) / 10, atol=(atol or 1e-6) / 10, **kw)
    ms = float(np.max(np.abs(r.y - rf.y) / ((atol or 1e-6) + (rtol or 1e-3) * np.abs(r.y))))
    exp["scipy_max_scaled_vs_fine"] = ms
    exp["tolerance_consistent"] = ms <= 100
    save(family, req, exp, expect_warning=expect_warning, expect_postcondition_failed=None if ms <= 100 else ["tolerance_consistent"])


# analytic
case("decay_param", ["y"], ["-k*y"], {"y": 2.0}, [0, 5], params={"k": 0.7}, rtol=1e-8, atol=1e-10, exact=lambda t: [2.0 * math.exp(-0.7 * t)])
case("decay_default_tol", ["y"], ["-k*y"], {"y": 2.0}, [0, 5], params={"k": 0.7}, exact=lambda t: [2.0 * math.exp(-0.7 * t)])
case("harmonic", ["x", "v"], ["v", "-x"], {"x": 1.0, "v": 0.0}, [0, 20], rtol=1e-9, atol=1e-12, exact=lambda t: [math.cos(t), -math.sin(t)])
w0, z = 2.0, 0.1; wd = w0 * math.sqrt(1 - z * z)
case("damped_oscillator", ["x", "v"], ["v", "-k*x - c*v"], {"x": 1.0, "v": 0.0}, [0, 15], params={"k": w0 * w0, "c": 2 * z * w0}, rtol=1e-8, atol=1e-10, n_out=61,
     exact=lambda t: [math.exp(-z * w0 * t) * (math.cos(wd * t) + z * w0 / wd * math.sin(wd * t)),
                      -math.exp(-z * w0 * t) * (w0 * w0 / wd) * math.sin(wd * t)])
case("forced_first_order", ["y"], ["-y + sin(t)"], {"y": 0.0}, [0, 10], rtol=1e-7, atol=1e-10,
     exact=lambda t: [0.5 * (math.sin(t) - math.cos(t) + math.exp(-t))])
case("backward", ["y"], ["-k*y"], {"y": 1.0}, [2.0, 0.0], params={"k": 0.5}, t_eval=[2.0, 1.5, 1.0, 0.25, 0.0], rtol=1e-9, atol=1e-12,
     exact=lambda t: [math.exp(-0.5 * (t - 2.0))])
case("constant_rhs_precedence", ["y"], ["2^3^2 - -2^2*3"], {"y": 0.0}, [0, 1], n_out=5, exact=lambda t: [(512 + 12) * t])   # 2^(3^2) = 512; - -2^2*3 = -((-(2^2))·3) = +12
case("explicit_t_eval", ["y"], ["cos(t)"], {"y": 0.0}, [0, 6], t_eval=[0.0, 0.1, 0.5, 2.0, 3.14159, 6.0], rtol=1e-10, atol=1e-12, exact=lambda t: [math.sin(t)])
case("explicit_defaults", ["y"], ["-y"], {"y": 1.0}, [0, 3], n_out=101, rtol=0.001, atol=1e-06, exact=lambda t: [math.exp(-t)])
case("first_and_max_step", ["y"], ["-y"], {"y": 1.0}, [0, 3], rtol=1e-6, atol=1e-9, first_step=0.01, max_step=0.05, exact=lambda t: [math.exp(-t)])
case("functions", ["y"], ["atan2(1, t + 1) + hypot(3, 4)/5 + min(t, 1) - max(0, t - 2) + sign(t - 1)*abs(t - 1) + pi*0"], {"y": 0.0}, [0, 3], rtol=1e-9, atol=1e-12, n_out=7)
# nonlinear, truth by DOP853
case("van_der_pol_mu1", ["x", "v"], ["v", "mu*(1 - x^2)*v - x"], {"x": 2.0, "v": 0.0}, [0, 20], params={"mu": 1.0}, rtol=1e-8, atol=1e-10)
case("lotka_volterra", ["prey", "pred"], ["a*prey - b*prey*pred", "-c*pred + d*prey*pred"], {"prey": 10.0, "pred": 5.0}, [0, 15],
     params={"a": 1.1, "b": 0.4, "c": 0.4, "d": 0.1}, rtol=1e-8, atol=1e-10)
case("pendulum", ["theta", "omega"], ["omega", "-(g/L)*sin(theta)"], {"theta": 2.5, "omega": 0.0}, [0, 10], params={"g": 9.81, "L": 1.0}, rtol=1e-9, atol=1e-12)
case("lorenz_short", ["x", "y", "z"], ["s*(y - x)", "x*(r - z) - y", "x*y - b*z"], {"x": 1.0, "y": 1.0, "z": 1.0}, [0, 2],
     params={"s": 10.0, "r": 28.0, "b": 8.0 / 3.0}, rtol=1e-10, atol=1e-12, n_out=41)
case("van_der_pol_mu1_default_tol", ["x", "v"], ["v", "mu*(1 - x^2)*v - x"], {"x": 2.0, "v": 0.0}, [0, 20], params={"mu": 1.0})
# stiff: works, slowly, and is flagged
case("stiff_van_der_pol_mu100", ["x", "v"], ["v", "mu*(1 - x^2)*v - x"], {"x": 2.0, "v": 0.0}, [0, 60], params={"mu": 100.0}, n_out=31,
     expect_warning="W.NUMERICAL.STIFF_PROBLEM")
# ---- errors
base = {"states": ["x", "v"], "rhs": ["v", "-k*x"], "params": {"k": 4.0}, "initial": {"x": 1.0, "v": 0.0}, "t_span": [0, 1]}
def err(family, code, **chg):
    r = json.loads(json.dumps(base)); r.update(chg)
    for k in [k for k, v in r.items() if v is None]: del r[k]
    save(family, r, expected_error=code)
err("expr_syntax", "E.VALIDATION.EXPRESSION", rhs=["v", "-k*x +"])
err("expr_unknown_name", "E.VALIDATION.EXPRESSION", rhs=["v", "-k*y"])
err("expr_unknown_function", "E.VALIDATION.EXPRESSION", rhs=["v", "-k*sinc(x)"])
err("expr_arity", "E.VALIDATION.EXPRESSION", rhs=["v", "-atan2(x)"])
err("expr_non_ascii", "E.VALIDATION.EXPRESSION", rhs=["v", "-k×x"])
err("reserved_state_name", "E.VALIDATION.EXPRESSION", states=["t", "v"], initial={"t": 1.0, "v": 0.0})
err("param_shadows_state", "E.VALIDATION.EXPRESSION", params={"x": 1.0, "k": 4.0})
err("rhs_length", "E.VALIDATION.SHAPE", rhs=["v"])
err("initial_missing", "E.VALIDATION.SHAPE", initial={"x": 1.0})
err("t_span_empty", "E.VALIDATION.RANGE", t_span=[1, 1])
err("t_eval_out_of_span", "E.VALIDATION.RANGE", t_eval=[0, 2])
err("t_eval_not_monotone", "E.VALIDATION.GRID", t_eval=[0, 0.5, 0.2])
err("t_eval_and_n_out", "E.VALIDATION.OPTION_CONFLICT", t_eval=[0, 1], n_out=5)
err("rtol_too_small", "E.VALIDATION.RANGE", rtol=1e-16)
err("missing_rhs", "E.VALIDATION.SCHEMA", rhs=None)
save("blow_up", {"states": ["y"], "rhs": ["y^2"], "initial": {"y": 1.0}, "t_span": [0, 2]}, expected_error="E.NUMERICAL.STEP_SIZE_TOO_SMALL")
save("non_finite_start", {"states": ["y"], "rhs": ["log(y)"], "initial": {"y": 0.0}, "t_span": [0, 1]}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
print("ode golden:", n_saved, "cases →", D)

# ---- published examples
E = f"examples/{OP}"
ex = {"request.json": {"states": ["x", "v"], "rhs": ["v", "-k*x - c*v"], "params": {"k": 4.0, "c": 0.4}, "initial": {"x": 1.0, "v": 0.0},
                       "t_span": [0, 15], "n_out": 31, "rtol": 1e-8, "atol": 1e-10},
      "request.vdp.json": {"states": ["x", "v"], "rhs": ["v", "mu*(1 - x^2)*v - x"], "params": {"mu": 1.0}, "initial": {"x": 2.0, "v": 0.0}, "t_span": [0, 20], "n_out": 21},
      "request.error.json": {"states": ["x", "v"], "rhs": ["v", "-k*x - c*v +"], "params": {"k": 4.0, "c": 0.4}, "initial": {"x": 1.0, "v": 0.0}, "t_span": [0, 15]}}
ref = {"note": "scipy solve_ivp RK45 with the same settings; rhs evaluated by the independent Python parser of 3.15. NOT MATLAB"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name == "request.error.json": continue
    f = rhs_fun(req["states"], req["rhs"], req["params"])
    te = np.linspace(req["t_span"][0], req["t_span"][1], req["n_out"])
    r = solve_ivp(f, req["t_span"], [req["initial"][s] for s in req["states"]], method="RK45", t_eval=te, rtol=req.get("rtol", 1e-3), atol=req.get("atol", 1e-6))
    ref[name] = {"y": [float(v) for v in r.y.ravel()], "nfev": int(r.nfev)}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("ode examples →", E)
