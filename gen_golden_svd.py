"""Golden cases for linalg.decomp.svd (spec v0.4.11, 10.23).

Engines: numpy.linalg.svd (LAPACK dgesdd) as the everyday reference, and mpmath (50 digits, svd_r) on the
*stored* f64 matrix: exact singular values and vectors for the data as given. NOT MATLAB output.
Numerical rank uses MATLAB's tolerance max(m, n)·eps(s_max) on the exact singular values.
"""
import hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 50
SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "linalg.decomp.svd", "0.4.0"
ENV = {"engine": "numpy.linalg.svd (LAPACK dgesdd) / mpmath svd_r", "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
DIR = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(DIR, ignore_errors=True); os.makedirs(DIR)


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


count = 0
def save(family, request, expected=None, expected_error=None):
    global count
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{DIR}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
    count += 1


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def truth(A):
    m, n = A.shape
    Am = mp.matrix([[mp.mpf(float(v)) for v in row] for row in A])
    U, S, Vt = mp.svd_r(Am)
    p = min(m, n)
    s = [float(S[k]) for k in range(p)]
    order = sorted(range(p), key=lambda k: -s[k])
    s = [s[k] for k in order]
    u = [[float(U[i, k]) for i in range(m)] for k in order]
    v = [[float(Vt[k, j]) for j in range(n)] for k in order]
    return s, u, v


def case(family, A, mode=None):
    A = np.asarray(A, float)
    m, n = A.shape
    s, u, v = truth(A)
    smax = s[0]
    tol = max(m, n) * np.spacing(smax) if smax > 0 else 0.0
    gaps = [min([abs(s[j] - s[k]) for j in range(len(s)) if j != k], default=None) for k in range(len(s))]
    req = {"A": enc(A)}
    if mode: req["mode"] = mode
    exp = {"s_true": s, "s_numpy": [float(x) for x in np.linalg.svd(A, compute_uv=False)], "u_true": u, "v_true": v,
           "gaps_true": gaps, "rank": int(sum(x > tol for x in s)), "shape": [m, n]}
    save(family, req, exp)


case("random_tall", rng.normal(size=(5, 3)))
case("random_wide", rng.normal(size=(3, 5)))
case("random_square", rng.normal(size=(10, 10)))
case("random_tall_full", rng.normal(size=(6, 2)), mode="full")
case("random_wide_full", rng.normal(size=(2, 5)), mode="full")
case("random_values", rng.normal(size=(8, 4)), mode="values")
case("scalar", [[-3.0]])
case("row_vector", [[1.0, 2.0, 2.0, 4.0]])
case("column_vector", [[3.0], [4.0]])
case("rank_deficient_3x3", np.arange(1.0, 10.0).reshape(3, 3))
H = np.array([[1.0 / (i + j + 1) for j in range(8)] for i in range(8)])
case("hilbert_8", H)
Q1 = np.linalg.qr(rng.normal(size=(4, 4)))[0]; Q2 = np.linalg.qr(rng.normal(size=(3, 3)))[0]
case("repeated_singular_values", Q1[:, :3] @ np.diag([2.0, 2.0, 1.0]) @ Q2.T)
case("zero_matrix", np.zeros((2, 3)))
case("graded", np.diag([1e8, 1.0, 1e-8]) @ rng.normal(size=(3, 3)))
case("large_tall", rng.normal(size=(60, 25)))
# ---- errors
save("vector_A", {"A": enc([1.0, 2.0, 3.0])}, expected_error="E.VALIDATION.SHAPE")
save("complex_A", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
save("inf_A", {"A": {"dtype": "f64", "shape": [1, 2], "data": [1.0, "Inf"]}}, expected_error="E.VALIDATION.NON_FINITE")
save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
save("bad_mode", {"A": enc(np.eye(2)), "mode": "econ"}, expected_error="E.VALIDATION.SCHEMA")
print("svd golden:", count)

E = f"examples/{OP}"
os.makedirs(E, exist_ok=True)
ex = {"request.json": {"A": enc([[3.0, 2.0, 2.0], [2.0, 3.0, -2.0]])},
      "request.full.json": {"A": enc(np.arange(1.0, 10.0).reshape(3, 3)), "mode": "full"},
      "request.error.json": {"A": enc([1.0, 2.0, 3.0])}}
ref = {"note": "mpmath 50 digits; NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name == "request.error.json":
        continue
    A = np.array(req["A"]["data"]).reshape(req["A"]["shape"])
    ref[name] = {"s_true": truth(A)[0], "s_numpy": [float(x) for x in np.linalg.svd(A, compute_uv=False)]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("svd examples →", E)
