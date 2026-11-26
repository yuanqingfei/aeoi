"""Golden cases for fit.poly.polyfit (spec v0.4.11, 10.21).

Engines: numpy.polyfit (LAPACK dgelsd on the column-scaled Vandermonde matrix) as the everyday reference, and
mpmath (60-digit normal equations of the Vandermonde matrix built from the *stored* f64 data: the exact
least-squares answer for the data as given). NOT MATLAB output (MATLAB polyfit uses Householder QR without
column scaling).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 60
SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "fit.poly.polyfit", "0.4.0"
ENV = {"engine": "numpy.polyfit / mpmath", "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
DIR = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(DIR, ignore_errors=True); os.makedirs(DIR)


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


count = 0
def save(family, request, expected=None, expected_error=None, expect_warning=None):
    global count
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expect_warning: case["expect_warning"] = expect_warning
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{DIR}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False)
    count += 1


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def truth(x, y, d, center):
    """Exact fit for the stored data: (p, mu, residual_norm, std_errors) at 60 digits."""
    xs = [mp.mpf(float(v)) for v in x]; ys = [mp.mpf(float(v)) for v in y]
    n = len(xs); mu = None
    if center:
        mean = mp.fsum(xs) / n
        sd = mp.sqrt(mp.fsum([(v - mean) ** 2 for v in xs]) / (n - 1))
        mu = [mean, sd]; ts = [(v - mean) / sd for v in xs]
    else:
        ts = xs
    V = mp.matrix([[t ** (d - j) for j in range(d + 1)] for t in ts])
    G = V.T * V
    p = mp.lu_solve(G, V.T * mp.matrix(ys))
    r = mp.matrix(ys) - V * p
    rn = mp.sqrt(mp.fsum([r[i] ** 2 for i in range(n)]))
    dof = n - d - 1
    se = None
    if dof > 0:
        Gi = G ** -1
        se = [float(mp.sqrt(Gi[j, j]) * rn / mp.sqrt(dof)) for j in range(d + 1)]
    cn = [float(mp.sqrt(mp.fsum([V[i, j] ** 2 for i in range(n)]))) for j in range(d + 1)]
    Vs = np.array([[float(V[i, j]) / cn[j] for j in range(d + 1)] for i in range(n)])
    return [float(v) for v in p], (None if mu is None else [float(mu[0]), float(mu[1])]), float(rn), se, cn, float(np.linalg.cond(Vs))


def case(family, x, y, d, scaling=None, expect_warning=None):
    x = np.asarray(x, float); y = np.asarray(y, float)
    req = {"x": enc(x), "y": enc(y), "degree": d}
    if scaling: req["scaling"] = scaling
    p, mu, rn, se, cn, kap = truth(x, y, d, scaling == "center_scale")
    if scaling == "center_scale":
        xh = (x - x.mean()) / x.std(ddof=1)
        p_np = np.polyfit(xh, y, d)
    else:
        p_np = np.polyfit(x, y, d)
    exp = {"p_true": p, "p_numpy": [float(v) for v in p_np], "mu_true": mu, "residual_norm_true": rn, "std_errors_true": se,
           "col_norms": cn, "kappa_scaled_2": kap, "dof": len(x) - d - 1,
           "numpy_scaled_forward_error": float(np.linalg.norm(np.array(cn) * (p_np - p)) / max(np.linalg.norm(np.array(cn) * np.array(p)), 1e-300))}
    save(family, req, exp, expect_warning=expect_warning)


x5 = np.arange(5.0)
case("line_exact", x5, 2 * x5 + 1, 1)
case("quadratic_exact", [-1.0, 0.0, 1.0, 2.0, 3.0], [6.0, 3.0, 2.0, 3.0, 6.0], 2)
case("constant_mean", [0.0, 1.0, 2.0, 3.0], [1.0, 2.0, 4.0, 8.0], 0)
t = np.linspace(0, 1, 50)
case("noisy_cubic", t, 1 - 2 * t + 0.5 * t ** 3 + 0.01 * rng.normal(size=50), 3)
case("interpolating_dof0", [0.0, 1.0, 3.0, 4.0], [1.0, -1.0, 2.0, 0.5], 3)
case("repeated_points", [0.0, 0.0, 1.0, 1.0, 2.0, 2.0], [0.9, 1.1, 2.0, 2.2, 5.1, 4.9], 2)
case("degree8_unit_interval", np.linspace(0, 1, 40), np.exp(np.linspace(0, 1, 40)), 8)
xr = np.linspace(-1, 1, 41)
case("runge_degree10", xr, 1 / (1 + 25 * xr ** 2), 10)
yrs = np.arange(1990, 2021, dtype=float)
yy = np.sin((yrs - 2005) / 10) + 0.01 * rng.normal(size=yrs.size)
case("years_degree3", yrs, yy, 3)
case("years_degree4_warns", yrs, yy, 4, expect_warning="W.NUMERICAL.ILL_CONDITIONED")
case("years_degree4_center_scale", yrs, yy, 4, scaling="center_scale")
case("years_degree8_center_scale", yrs, yy, 8, scaling="center_scale")
case("negative_large_x", -1e6 + np.arange(20.0), np.arange(20.0) ** 2, 2, scaling="center_scale")
case("random_degree5", rng.uniform(-3, 3, 30), rng.normal(size=30), 5)
# ---- errors
save("too_few_points", {"x": enc([0.0, 1.0, 2.0]), "y": enc([1.0, 2.0, 0.0]), "degree": 3}, expected_error="E.DOMAIN.UNDERDETERMINED")
save("too_few_distinct", {"x": enc([1.0, 1.0, 2.0, 2.0]), "y": enc([1.0, 2.0, 3.0, 4.0]), "degree": 2}, expected_error="E.DOMAIN.UNDERDETERMINED")
save("years_degree8_singular", {"x": enc(yrs), "y": enc(yy), "degree": 8}, expected_error="E.NUMERICAL.SINGULAR")
save("length_mismatch", {"x": enc([0.0, 1.0, 2.0]), "y": enc([1.0, 2.0]), "degree": 1}, expected_error="E.VALIDATION.SHAPE")
save("x_matrix", {"x": enc([[0.0, 1.0], [2.0, 3.0]]), "y": enc([1.0, 2.0, 3.0, 4.0]), "degree": 1}, expected_error="E.VALIDATION.SHAPE")
save("nan_y", {"x": enc([0.0, 1.0, 2.0]), "y": {"dtype": "f64", "shape": [3], "data": [1.0, "NaN", 2.0]}, "degree": 1}, expected_error="E.VALIDATION.NON_FINITE")
save("missing_degree", {"x": enc([0.0, 1.0]), "y": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SCHEMA")
save("negative_degree", {"x": enc([0.0, 1.0]), "y": enc([1.0, 2.0]), "degree": -1}, expected_error="E.VALIDATION.SCHEMA")
save("degree_too_high", {"x": enc(np.arange(40.0)), "y": enc(np.arange(40.0)), "degree": 31}, expected_error="E.VALIDATION.RANGE")
save("center_scale_constant_x", {"x": enc([2.0, 2.0, 2.0]), "y": enc([1.0, 2.0, 3.0]), "degree": 0, "scaling": "center_scale"}, expected_error="E.VALIDATION.OPTION_CONFLICT")
print("polyfit golden:", count)

E = f"examples/{OP}"
os.makedirs(E, exist_ok=True)
xe = np.array([0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0])
ye = np.array([1.02, 0.31, 0.05, 0.22, 0.98, 2.27, 4.01])
ex = {"request.json": {"x": enc(xe), "y": enc(ye), "degree": 2},
      "request.ill.json": {"x": enc(yrs), "y": enc(np.round(yy, 4)), "degree": 4},
      "request.error.json": {"x": enc([1.0, 1.0, 2.0, 2.0, 2.0]), "y": enc([0.9, 1.1, 2.0, 2.1, 1.9]), "degree": 2}}
ref = {"note": "numpy.polyfit; p_true = mpmath 60 digits; NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name == "request.error.json":
        continue
    x = np.array(req["x"]["data"]); y = np.array(req["y"]["data"])
    ref[name] = {"p": [float(v) for v in np.polyfit(x, y, req["degree"])], "p_true": truth(x, y, req["degree"], False)[0]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("polyfit examples →", E)
