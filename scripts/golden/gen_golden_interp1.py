"""Golden cases for interp.grid.interp1 (spec v0.4.10, 10.20).

Engine: numpy.interp (linear), scipy.interpolate interp1d (nearest-up / previous / next / linear extrapolation),
PchipInterpolator (pchip) and CubicSpline(bc_type="not-a-knot") (spline). Families with a known exact answer
(spline on cubic data, every method on linear data) also store the exact values. NOT MATLAB output (MATLAB
interp1 uses the same definitions for these methods; its default out-of-range behaviour differs, see 10.20).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import scipy
from scipy.interpolate import CubicSpline, PchipInterpolator, interp1d

SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "interp.grid.interp1", "0.4.0"
ENV = {"engine": "numpy.interp / scipy.interpolate (interp1d nearest-up, previous, next; PchipInterpolator; CubicSpline not-a-knot)",
       "numpy": np.__version__, "scipy": scipy.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
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
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) if np.isfinite(v) else "NaN" for v in x.ravel()]}


def reference(x, y, xq, method, extrap):
    """Reference values, one row per channel."""
    x = np.asarray(x, float); Y = np.atleast_2d(np.asarray(y, float)); xq = np.asarray(xq, float)
    lo, hi = x[0], x[-1]
    out = []
    for yy in Y:
        q = np.clip(xq, lo, hi) if extrap == "clamp" else xq
        if method == "linear":
            r = interp1d(x, yy, kind="linear", fill_value="extrapolate")(q) if extrap == "extrapolate" else np.interp(q, x, yy)
        elif method in ("nearest", "previous", "next"):
            kind = "nearest-up" if method == "nearest" else method
            r = interp1d(x, yy, kind=kind, bounds_error=False, fill_value=(yy[0], yy[-1]))(q)
        elif method == "pchip":
            r = PchipInterpolator(x, yy, extrapolate=True)(q)
        else:
            r = CubicSpline(x, yy, bc_type="not-a-knot", extrapolate=True)(q)
        r = np.array(r, float)
        if extrap == "nan":
            r[(xq < lo) | (xq > hi)] = np.nan
        out.append(r)
    return np.array(out)


def case(family, x, y, xq, method=None, extrap=None, exact=None, expect_warning=None):
    req = {"x": enc(x), "y": enc(y), "xq": enc(xq)}
    if method: req["method"] = method
    if extrap: req["extrapolation"] = extrap
    R = reference(x, y, xq, method or "linear", extrap or "error")
    shape = [len(xq)] if np.ndim(y) == 1 else [np.shape(y)[0], len(xq)]
    exp = {"yq": [float(v) if np.isfinite(v) else "NaN" for v in R.ravel()], "shape": shape,
           "y_scale": float(np.max(np.abs(y)))}
    if exact is not None:
        exp["yq_exact"] = [float(v) for v in np.asarray(exact, float).ravel()]
    if extrap == "extrapolate" and np.any((np.asarray(xq) < x[0]) | (np.asarray(xq) > x[-1])):
        expect_warning = expect_warning or "W.ACCURACY.EXTRAPOLATED"
    save(family, req, exp, expect_warning=expect_warning)


METHODS = ["linear", "nearest", "previous", "next", "pchip", "spline"]
# ---- every method on random non-uniform data, queries inside (nodes included)
x = np.sort(rng.uniform(0, 10, 15)); x[0], x[-1] = 0.0, 10.0
y = np.sin(x) + 0.1 * x
xq = np.concatenate([rng.uniform(0, 10, 40), x[[0, 3, 7, 14]]])
for mth in METHODS:
    case(f"random_{mth}", x, y, xq, method=mth)
# ---- defaults, multichannel, two/three points
case("default_linear", [1.0, 2.0, 4.0], [10.0, 20.0, 0.0], [1.5, 3.0, 4.0])
Y = np.vstack([np.cos(x), x ** 2 / 10, np.ones_like(x)])
for mth in ("linear", "pchip", "spline"):
    case(f"multichannel_{mth}", x, Y, rng.uniform(0, 10, 20), method=mth)
for mth in ("pchip", "spline"):
    case(f"two_points_{mth}", [0.0, 2.0], [1.0, 5.0], [0.5, 1.0, 1.7], method=mth)
    case(f"three_points_{mth}", [0.0, 1.0, 3.0], [1.0, 0.0, 4.0], [0.2, 0.9, 2.5], method=mth)
case("four_points_spline", [0.0, 1.0, 2.5, 3.0], [0.0, 1.0, -1.0, 2.0], np.linspace(0, 3, 13), method="spline")
# ---- exact answers
xc = np.array([0.0, 0.4, 1.1, 1.5, 2.6, 3.0])
cubic = lambda t: t ** 3 - 2 * t ** 2 + 0.5 * t + 1
qc = np.linspace(0, 3, 31)
case("spline_cubic_exact", xc, cubic(xc), qc, method="spline", exact=cubic(qc))
lin = lambda t: 2.5 * t - 1
for mth in ("linear", "pchip", "spline"):
    case(f"{mth}_line_exact", xc, lin(xc), qc, method=mth, exact=lin(qc))
# ---- nearest ties and previous/next at nodes
case("nearest_ties_right", [0.0, 1.0, 2.0, 4.0], [10.0, 20.0, 30.0, 40.0], [0.5, 1.5, 3.0, 2.99, 1.0], method="nearest")
case("previous_next_nodes_prev", [0.0, 1.0, 2.0], [5.0, 6.0, 7.0], [0.0, 0.5, 1.0, 1.99, 2.0], method="previous")
case("previous_next_nodes_next", [0.0, 1.0, 2.0], [5.0, 6.0, 7.0], [0.0, 0.5, 1.0, 1.01, 2.0], method="next")
# ---- pchip shape preservation (flat segments, sign changes) vs spline overshoot
xs = np.arange(8.0); ys = np.array([0, 0, 0, 1, 1, 1, 4, 4], float)
qs = np.linspace(0, 7, 57)
case("pchip_step_data", xs, ys, qs, method="pchip")
case("spline_step_data_overshoots", xs, ys, qs, method="spline")
case("pchip_sign_changes", [0.0, 1.0, 2.0, 3.0, 4.0], [0.0, 3.0, -1.0, 2.0, 2.0], np.linspace(0, 4, 21), method="pchip")
# ---- out of range: nan / clamp / extrapolate for every method
xo = np.array([0.0, 1.0, 2.0, 3.0]); yo = np.array([1.0, 3.0, 2.0, 5.0]); qo = [-0.5, 0.25, 2.5, 3.5]
for mth in METHODS:
    for ex in ("nan", "clamp", "extrapolate"):
        case(f"outside_{ex}_{mth}", xo, yo, qo, method=mth, extrap=ex)
case("inside_with_extrapolate_no_warning", xo, yo, [0.5, 2.0], method="spline", extrap="extrapolate")
# ---- errors
save("outside_default_error", {"x": enc(xo), "y": enc(yo), "xq": enc([0.5, 4.0])}, expected_error="E.DOMAIN.EXTRAPOLATION_REQUIRED")
save("outside_default_error_spline", {"x": enc(xo), "y": enc(yo), "xq": enc([-1.0]), "method": "spline"}, expected_error="E.DOMAIN.EXTRAPOLATION_REQUIRED")
save("x_not_increasing", {"x": enc([0.0, 2.0, 1.0]), "y": enc([1.0, 2.0, 3.0]), "xq": enc([0.5])}, expected_error="E.VALIDATION.GRID")
save("x_decreasing", {"x": enc([3.0, 2.0, 1.0]), "y": enc([1.0, 2.0, 3.0]), "xq": enc([1.5])}, expected_error="E.VALIDATION.GRID")
save("x_duplicate", {"x": enc([0.0, 1.0, 1.0, 2.0]), "y": enc([1.0, 2.0, 3.0, 4.0]), "xq": enc([0.5])}, expected_error="E.VALIDATION.GRID")
save("y_length_mismatch", {"x": enc([0.0, 1.0, 2.0]), "y": enc([1.0, 2.0]), "xq": enc([0.5])}, expected_error="E.VALIDATION.SHAPE")
save("y_columns_not_rows", {"x": enc([0.0, 1.0, 2.0]), "y": enc(np.ones((3, 2))), "xq": enc([0.5])}, expected_error="E.VALIDATION.SHAPE")
save("one_point", {"x": enc([1.0]), "y": enc([2.0]), "xq": enc([1.0])}, expected_error="E.VALIDATION.SHAPE")
save("xq_2d", {"x": enc([0.0, 1.0]), "y": enc([0.0, 1.0]), "xq": enc([[0.5, 0.6]])}, expected_error="E.VALIDATION.SHAPE")
save("nonfinite_y", {"x": enc([0.0, 1.0]), "y": {"dtype": "f64", "shape": [2], "data": [0.0, "NaN"]}, "xq": enc([0.5])}, expected_error="E.VALIDATION.NON_FINITE")
save("bad_method", {"x": enc([0.0, 1.0]), "y": enc([0.0, 1.0]), "xq": enc([0.5]), "method": "cubic"}, expected_error="E.VALIDATION.SCHEMA")
save("bad_extrapolation", {"x": enc([0.0, 1.0]), "y": enc([0.0, 1.0]), "xq": enc([0.5]), "extrapolation": "zero"}, expected_error="E.VALIDATION.SCHEMA")
print("interp1 golden:", count)

# ---- published examples
E = f"examples/{OP}"
xe = np.array([0.0, 1.0, 2.0, 3.0, 4.0, 5.0]); ye = np.array([0.0, 0.0, 1.0, 1.0, 4.0, 4.0]); qe = np.linspace(0, 5, 11)
ex = {"request.json": {"x": enc(xe), "y": enc(ye), "xq": enc(qe), "method": "pchip"},
      "request.spline.json": {"x": enc(xe), "y": enc(ye), "xq": enc(qe), "method": "spline"},
      "request.error.json": {"x": enc(xe), "y": enc(ye), "xq": enc([2.5, 6.0])}}
ref = {"note": "scipy PchipInterpolator / CubicSpline(not-a-knot); NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
ref["request.json"] = {"yq": [float(v) for v in reference(xe, ye, qe, "pchip", "error").ravel()]}
ref["request.spline.json"] = {"yq": [float(v) for v in reference(xe, ye, qe, "spline", "error").ravel()]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("interp1 examples →", E)
