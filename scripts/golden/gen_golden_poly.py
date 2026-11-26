"""Golden cases for fit.poly.polyval and fit.poly.roots (spec v0.4.20, 10.33, 10.34).

polyval — engines: numpy.polyval (plain Horner) as the everyday reference, and the EXACT value of the
polynomial at the stored f64 point computed with Python fractions (rational arithmetic, no rounding): the
truth. With mu the point is t = (x − mu[0])/mu[1] rounded once in f64, as the spec defines.

roots — engines: numpy.roots (LAPACK companion eigenvalues) and mpmath.polyroots at 60 digits on the stored
f64 coefficients (the truth). Every case is cross-checked here, independently of the Rust code: the truth
roots are verified by their residual at 60 digits, and the Braess–Hadeler / Carstensen inclusion theorem the
operator relies on is checked on numpy's approximations (every true root lies in the union of the discs and
each connected component of k discs holds k true roots).
NOT MATLAB output.
"""
import hashlib, json, os, platform, shutil
from fractions import Fraction as F
import numpy as np
import mpmath as mp

mp.mp.dps = 60
SEED = 20261126
rng = np.random.default_rng(SEED)
U = 2.0 ** -53


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


class Saver:
    def __init__(self, op, ver, env):
        self.op, self.ver, self.env, self.count = op, ver, env, 0
        self.dir = f"golden/python-control/{op}/{ver}"
        shutil.rmtree(self.dir, ignore_errors=True); os.makedirs(self.dir)

    def save(self, family, request, expected=None, expected_error=None, expect_warning=None):
        case = {"spec": "aeoi/0.4", "operator": self.op, "operator_version": self.ver, "case_family": family, "request": request, "reference_env": self.env}
        if expected is not None: case["expected"] = expected
        if expected_error is not None: case["expected_error"] = expected_error
        if expect_warning: case["expect_warning"] = expect_warning
        h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
        json.dump(case, open(f"{self.dir}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
        self.count += 1


# ============================================================================================ polyval
def exact_val(p, t):
    """p(t) exactly (rationals), then rounded to the nearest f64."""
    acc = F(0)
    for a in p:
        acc = acc * F(t) + F(a)
    return acc


def gam(k):
    return k * U / (1 - k * U)


def pv_truth(p, x, mu=None):
    ts = [float((v - mu[0]) / mu[1]) for v in x] if mu else [float(v) for v in x]
    vals = [exact_val(p, t) for t in ts]
    ptil = [float(sum(abs(F(a)) * abs(F(t)) ** (len(p) - 1 - i) for i, a in enumerate(p))) for t in ts]
    return ts, vals, ptil


PV = Saver("fit.poly.polyval", "0.4.0", {"engine": "numpy.polyval / exact rational arithmetic (fractions)", "numpy": np.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"})


def pv_case(family, p, x, mu=None, expect_warning=None):
    p = [float(v) for v in p]; x = [float(v) for v in x]
    req = {"p": enc(p), "x": enc(x)}
    if mu is not None:
        req["mu"] = [float(mu[0]), float(mu[1])]
    ts, vals, ptil = pv_truth(p, x, mu)
    y_np = [float(v) for v in np.polyval(p, ts)]
    n = len(p) - 1
    # independent prediction of the warning from the profile's rule (10.33 PV-4), with the true value:
    # fewer than 8 guaranteed digits where p(t) ≠ 0 and γ_{2n}²·p̃ > 1e-8·|p(t)| (cases keep a wide margin)
    exp_warn_pts = [k for k, (v, pt) in enumerate(zip(vals, ptil)) if n > 0 and v != 0 and gam(2 * n) ** 2 * pt > 1e-8 * abs(float(v))]
    exp = {"t": ts, "y_true": [float(v) for v in vals], "y_numpy": y_np, "ptilde": ptil,
           "numpy_abs_error": [abs(float(F(a) - v)) for a, v in zip(y_np, vals)],
           "warn_points": exp_warn_pts}
    if bool(exp_warn_pts) != bool(expect_warning):
        raise SystemExit(f"{family}: warning prediction {exp_warn_pts} disagrees with the case design")
    PV.save(family, req, exp, expect_warning=expect_warning)


def expand(roots):
    return [float(v) for v in np.real(np.poly(roots))]


pv_case("cubic_grid", [1, -6, 11, -6], np.linspace(0, 4, 9))
pv_case("exact_roots_zero", [1, -6, 11, -6], [1.0, 2.0, 3.0])
pv_case("near_multiple_root_6", expand([1] * 6), [1 + k * 1e-3 for k in range(-5, 6)])
pv_case("near_multiple_root_10_warns", expand([1] * 10), [1 + 1e-3, 1 + 2e-3, 1 - 3e-3], expect_warning="W.NUMERICAL.ILL_CONDITIONED")
cheb = np.polynomial.chebyshev.cheb2poly([0] * 20 + [1])[::-1]
pv_case("chebyshev_T20", cheb, np.cos(np.linspace(0.01, 3.1, 50)))
pv_case("wide_range", [1e-10, 3.0, -2e5, 7.0], [-1e8, -1.0, 0.0, 1.0, 1e8])
pv_case("constant", [3.5], [-1.0, 0.0, 1e300])
pv_case("leading_zeros_kept", [0.0, 0.0, 2.0, -1.0], [0.5, 2.0])
yrs = np.arange(1990, 2021, dtype=float)
yy = np.sin((yrs - 2005) / 10)
mu = [float(np.mean(yrs)), float(np.std(yrs, ddof=1))]
pfit = np.polyfit((yrs - mu[0]) / mu[1], yy, 3)
pv_case("with_mu_from_polyfit", pfit, yrs, mu=mu)
pv_case("random_degree15", rng.normal(size=16), rng.uniform(-2, 2, 40))
pv_case("large_degree_200", rng.normal(size=201) / 10, rng.uniform(-1, 1, 20))
# ---- errors
PV.save("overflow", {"p": enc([1.0, 0.0, 0.0, 0.0]), "x": enc([1.0, 1e200])}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
PV.save("complex_x", {"p": enc([1.0, 1.0]), "x": {"dtype": "c128", "shape": [1], "data": [[0.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
PV.save("nan_x", {"p": enc([1.0, 1.0]), "x": {"dtype": "f64", "shape": [2], "data": [1.0, "NaN"]}}, expected_error="E.VALIDATION.NON_FINITE")
PV.save("p_matrix", {"p": enc([[1.0, 2.0], [3.0, 4.0]]), "x": enc([1.0])}, expected_error="E.VALIDATION.SHAPE")
PV.save("x_matrix", {"p": enc([1.0, 2.0]), "x": enc([[1.0, 2.0], [3.0, 4.0]])}, expected_error="E.VALIDATION.SHAPE")
PV.save("mu_zero_std", {"p": enc([1.0, 2.0]), "x": enc([1.0]), "mu": [0.0, 0.0]}, expected_error="E.VALIDATION.RANGE")
PV.save("mu_wrong_length", {"p": enc([1.0, 2.0]), "x": enc([1.0]), "mu": [0.0, 1.0, 2.0]}, expected_error="E.VALIDATION.SCHEMA")
PV.save("missing_x", {"p": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SCHEMA")
PV.save("degree_too_high", {"p": enc(np.ones(1002)), "x": enc([0.5])}, expected_error="E.VALIDATION.RANGE")
print("polyval golden:", PV.count)

# ============================================================================================ roots
RT = Saver("fit.poly.roots", "0.4.0", {"engine": "numpy.roots / mpmath.polyroots (60 digits)", "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"})


def sort_key(z):
    return (z.real, -z.imag)


def truth_roots(p):
    p = list(np.trim_zeros(np.asarray(p, float), "f"))
    nz = 0
    while p and p[-1] == 0.0:
        p.pop(); nz += 1
    q = [mp.mpf(v) for v in p]
    rs = []
    if len(q) > 1:
        rs, err = mp.polyroots(q, maxsteps=2000, extraprec=600, error=True)
        rs = [mp.mpc(r) for r in rs]
        for r in rs:   # the truth must be a root of the stored polynomial to ~40 digits
            res = abs(mp.polyval(q, r)); scale = mp.polyval([abs(c) for c in q], abs(r))
            assert res <= mp.mpf(10) ** -40 * scale, (p, r, res)
    rs = [mp.mpc(0)] * nz + rs
    return p, nz, rs


def inclusion(qc, z):
    """Braess–Hadeler / Carstensen discs for approximations z of the roots of qc (mpmath, exact-ish)."""
    m = len(qc) - 1
    radii = []
    for i, zi in enumerate(z):
        prod = mp.mpc(qc[0])
        for j, zj in enumerate(z):
            if j != i:
                prod *= (zi - zj)
        radii.append(m * abs(mp.polyval(qc, zi) / prod) if prod != 0 else mp.inf)
    return radii


def components(z, r):
    n = len(z); comp = list(range(n))
    def f(i):
        while comp[i] != i:
            i = comp[i]
        return i
    for i in range(n):
        for j in range(i + 1, n):
            if abs(z[i] - z[j]) <= r[i] + r[j]:
                a, b = f(i), f(j); comp[max(a, b)] = min(a, b)
    return [f(i) for i in range(n)]


def rt_case(family, p, expect_warning=None):
    p = [float(v) for v in p]
    q, nz, rs = truth_roots(p)
    rs_sorted = sorted(rs, key=lambda z: (float(z.real), -float(z.imag)))
    qm = [mp.mpf(v) for v in q[: len(q)]]
    # true multiplicity (roots equal to 30 digits) and absolute condition of simple roots: q̃(|r|)/|q'(r)|
    mult, cond = [], []
    dq = [qm[i] * (len(qm) - 1 - i) for i in range(len(qm) - 1)]
    for r in rs_sorted:
        k = sum(1 for s in rs_sorted if abs(s - r) <= mp.mpf(10) ** -30 * max(1, abs(r)))
        mult.append(k)
        if k == 1 and r != 0 and len(qm) > 1:
            cond.append(float(mp.polyval([abs(c) for c in qm], abs(r)) / abs(mp.polyval(dq, r))))
        elif k == 1:
            cond.append(0.0 if nz and r == 0 else float(mp.polyval([abs(c) for c in qm], abs(r)) / abs(mp.polyval(dq, r))) if dq else 0.0)
        else:
            cond.append(None)
    # independent check of the inclusion theorem on numpy's approximations of the roots of q
    if len(q) > 1:
        zn = [mp.mpc(complex(v)) for v in np.roots(q)]
        if len(set((complex(v) for v in zn))) == len(zn):
            rad = inclusion(qm, zn)
            comp = components(zn, rad)
            truth_q = rs[nz:]
            for t in truth_q:
                assert any(abs(t - zi) <= ri for zi, ri in zip(zn, rad)), (family, t)
            for c in set(comp):
                members = [i for i in range(len(zn)) if comp[i] == c]
                inside = [t for t in truth_q if any(abs(t - zn[i]) <= rad[i] for i in members)]
                assert len(inside) == len(members), (family, len(inside), len(members))
    numpy_r = sorted([complex(v) for v in np.roots(p)] if len(q) > 1 or nz else [], key=sort_key)
    exp = {"roots_true": [[float(z.real), float(z.imag)] for z in rs_sorted], "multiplicity_true": mult, "cond_abs": cond,
           "roots_numpy": [[v.real, v.imag] for v in numpy_r], "degree": len(q) - 1 + nz, "zero_roots": nz}
    RT.save(family, {"p": enc(p)}, exp, expect_warning=expect_warning)


rt_case("cubic_distinct", [1, -6, 11, -6])
rt_case("complex_pair", [1, 2, 5])
rt_case("trailing_zeros", [1, 2, 5, 0, 0])
rt_case("leading_zeros", [0, 0, 1, -3, 2], expect_warning="W.VALIDATION.LEADING_ZEROS_STRIPPED")
rt_case("double_root", [1, -2, 1], expect_warning="W.ACCURACY.CLUSTERED_ROOTS")
rt_case("triple_root", [1, -3, 3, -1], expect_warning="W.ACCURACY.CLUSTERED_ROOTS")
rt_case("double_plus_simple", [1, 0, -3, 2], expect_warning="W.ACCURACY.CLUSTERED_ROOTS")
rt_case("wilkinson10", np.poly(np.arange(1, 11)))
rt_case("chebyshev_T12", np.polynomial.chebyshev.cheb2poly([0] * 12 + [1])[::-1])
rt_case("unit_roots_8", [1, 0, 0, 0, 0, 0, 0, 0, -1])
rt_case("scale_disparity", [1.0, -(1e8 + 1e-8), 1.0])
rt_case("close_pair", np.poly([1.0, 1.0 + 1e-6]))
rt_case("random_degree30", rng.normal(size=31))
rt_case("linear", [2.0, -3.0])
rt_case("constant_no_roots", [5.0])
# ---- errors
RT.save("zero_polynomial", {"p": enc([0.0, 0.0, 0.0])}, expected_error="E.DOMAIN.ZERO_POLYNOMIAL")
RT.save("nan_coefficient", {"p": {"dtype": "f64", "shape": [3], "data": [1.0, "NaN", 2.0]}}, expected_error="E.VALIDATION.NON_FINITE")
RT.save("p_matrix", {"p": enc([[1.0, 2.0], [3.0, 4.0]])}, expected_error="E.VALIDATION.SHAPE")
RT.save("degree_too_high", {"p": enc(np.ones(502))}, expected_error="E.VALIDATION.RANGE")
RT.save("missing_p", {}, expected_error="E.VALIDATION.SCHEMA")
print("roots golden:", RT.count)

# ============================================================================================ examples
def write_examples(op, ex, ref):
    E = f"examples/{op}"
    for name, req in ex.items():
        open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1, allow_nan=False) + "\n")


ex = {"request.json": {"p": enc([1.0, -6.0, 11.0, -6.0]), "x": enc([0.0, 0.5, 1.5, 4.0])},
      "request.ill.json": {"p": enc(expand([1] * 6)), "x": enc([0.999, 1.001, 1.002])},
      "request.error.json": {"p": enc([1.0, 0.0, 1.0]), "x": {"dtype": "c128", "shape": [1], "data": [[0.0, 1.0]]}}}
ref = {"note": "y_true = exact rational value at the stored points; y_numpy = numpy.polyval; NOT MATLAB output"}
for name in ("request.json", "request.ill.json"):
    req = ex[name]; ts, vals, _ = pv_truth(req["p"]["data"], req["x"]["data"])
    ref[name] = {"y_true": [float(v) for v in vals], "y_numpy": [float(v) for v in np.polyval(req["p"]["data"], ts)]}
write_examples("fit.poly.polyval", ex, ref)
ex = {"request.json": {"p": enc([1.0, -6.0, 11.0, -6.0])},
      "request.cluster.json": {"p": enc([1.0, 0.0, -3.0, 2.0])},
      "request.error.json": {"p": enc([0.0, 0.0])}}
ref = {"note": "roots_true = mpmath.polyroots at 60 digits on the stored coefficients; NOT MATLAB output"}
for name in ("request.json", "request.cluster.json"):
    _, _, rs = truth_roots(ex[name]["p"]["data"])
    ref[name] = {"roots_true": [[float(z.real), float(z.imag)] for z in sorted(rs, key=lambda z: (float(z.real), -float(z.imag)))]}
write_examples("fit.poly.roots", ex, ref)
print("examples written")
