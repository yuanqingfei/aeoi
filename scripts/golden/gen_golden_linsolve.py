"""Golden cases for linalg.system.solve (spec v0.4.10, 10.19).

Engines: numpy.linalg.solve (LAPACK dgesv) / numpy.linalg.lstsq (LAPACK dgelsd) as the everyday reference,
and mpmath (60-digit LU solve / normal equations of the *stored* f64 matrix: the exact answer for the data as given).
NOT MATLAB output (MATLAB A\\b uses dgesv for square and Householder QR for overdetermined systems).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 60
SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "linalg.system.solve", "0.4.0"
ENV_NP = {"engine": "numpy.linalg (LAPACK dgesv / dgelsd)", "numpy": np.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
DIR = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(DIR, ignore_errors=True); os.makedirs(DIR)
U = np.finfo(float).eps / 2


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


count = 0
def save(family, request, expected=None, expected_error=None, expect_warning=None, expect_postcondition_failed=None):
    global count
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV_NP}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expect_warning: case["expect_warning"] = expect_warning
    if expect_postcondition_failed: case["expect_postcondition_failed"] = expect_postcondition_failed
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{DIR}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False)
    count += 1


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def truth(A, B):
    """Exact (60-digit) solution for the stored f64 data: LU for square, normal equations for least squares."""
    m, n = A.shape
    Am = mp.matrix([[mp.mpf(float(v)) for v in row] for row in A])
    cols = []
    for j in range(B.shape[1]):
        bm = mp.matrix([mp.mpf(float(v)) for v in B[:, j]])
        # least squares: normal equations at 60 digits (κ² ≤ 1e40 still leaves ≥ 20 correct digits)
        x = mp.lu_solve(Am, bm) if m == n else mp.lu_solve(Am.T * Am, Am.T * bm)
        cols.append([float(x[i]) for i in range(n)])
    return np.array(cols).T


def case(family, A, b, refinement=None, expect_warning=None, expect_postcondition_failed=None):
    A = np.asarray(A, float); b = np.asarray(b, float)
    B = b.reshape(-1, 1) if b.ndim == 1 else b
    m, n = A.shape
    x_ref = np.linalg.solve(A, B) if m == n else np.linalg.lstsq(A, B, rcond=None)[0]
    x_true = truth(A, B)
    kappa = float(np.linalg.cond(A, 1)) if m == n else float(np.linalg.cond(A))
    req = {"A": enc(A), "b": enc(b)}
    if refinement: req["refinement"] = refinement
    shape = [n] if b.ndim == 1 else [n, B.shape[1]]
    exp = {"x": [float(v) for v in x_ref.ravel()], "x_true": [float(v) for v in x_true.ravel()], "shape": shape,
           "kappa": kappa, "kappa_norm": "1" if m == n else "2", "method": "lu_partial_pivoting" if m == n else "householder_qr",
           "numpy_forward_error": float(np.max(np.abs(x_ref - x_true)) / max(np.max(np.abs(x_true)), 1e-300))}
    save(family, req, exp, expect_warning=expect_warning, expect_postcondition_failed=expect_postcondition_failed)


# ---- square, well conditioned
for n in (1, 3, 5, 10, 50):
    A = rng.normal(size=(n, n)) + n * np.eye(n) * 0.1
    case("square_random", A, rng.normal(size=n))
case("square_multi_rhs", rng.normal(size=(6, 6)), rng.normal(size=(6, 3)))
case("square_column_rhs", rng.normal(size=(4, 4)), rng.normal(size=(4, 1)))
Ai = np.array([[2, 1, 1], [4, -6, 0], [-2, 7, 2]], float)
case("square_exact_integer", Ai, Ai @ np.array([1.0, -2.0, 3.0]))
case("square_needs_pivoting", np.array([[1e-20, 1.0], [1.0, 1.0]]), np.array([1.0, 2.0]))
case("square_refinement_none", rng.normal(size=(8, 8)), rng.normal(size=8), refinement="none")
# ---- ill conditioned
H = lambda n: np.array([[1.0 / (i + j + 1) for j in range(n)] for i in range(n)])
case("hilbert_6", H(6), np.ones(6))
case("hilbert_10", H(10), np.ones(10))
case("hilbert_13_warns", H(13), np.ones(13), expect_warning="W.NUMERICAL.ILL_CONDITIONED")
# Wilkinson's growth matrix: partial pivoting growth 2^(n-1). Without refinement the backward error is large
# and the operator says so; with refinement (default) the compensated residual repairs it.
def wilkinson(n):
    W = np.eye(n) - np.tril(np.ones((n, n)), -1); W[:, -1] = 1.0
    return W
bw = rng.normal(size=60)
case("wilkinson_growth_refined", wilkinson(60), bw)
case("wilkinson_growth_unrefined", wilkinson(60), bw, refinement="none", expect_postcondition_failed=["backward_stable"])
# ---- overdetermined least squares
case("lsq_random", rng.normal(size=(20, 5)), rng.normal(size=20))
t = np.linspace(0, 1, 50)
case("lsq_polyfit_cubic", np.vander(t, 4), np.cos(3 * t) + 0.01 * rng.normal(size=50))
Ac = rng.normal(size=(12, 4))
case("lsq_consistent", Ac, Ac @ np.array([1.0, 2.0, -1.0, 0.5]))
case("lsq_multi_rhs", rng.normal(size=(15, 3)), rng.normal(size=(15, 2)))
case("lsq_vandermonde_ill", np.vander(np.linspace(0, 1, 30), 9), np.exp(np.linspace(0, 1, 30)))
# ---- errors
save("underdetermined", {"A": enc(rng.normal(size=(2, 3))), "b": enc([1.0, 2.0])}, expected_error="E.DOMAIN.UNDERDETERMINED")
save("singular_square", {"A": enc([[1.0, 2.0], [2.0, 4.0]]), "b": enc([1.0, 2.0])}, expected_error="E.NUMERICAL.SINGULAR")
save("singular_zero_column", {"A": enc([[1.0, 0.0, 2.0], [3.0, 0.0, 1.0], [0.0, 0.0, 5.0]]), "b": enc([1.0, 1.0, 1.0])}, expected_error="E.NUMERICAL.SINGULAR")
save("rank_deficient_lsq", {"A": enc([[1.0, 2.0], [2.0, 4.0], [3.0, 6.0]]), "b": enc([1.0, 2.0, 3.0])}, expected_error="E.NUMERICAL.SINGULAR")
save("row_mismatch", {"A": enc(np.eye(3)), "b": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
save("one_d_A", {"A": enc([1.0, 2.0, 3.0]), "b": enc([1.0, 2.0, 3.0])}, expected_error="E.VALIDATION.SHAPE")
save("complex_A", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}, "b": enc([1.0])}, expected_error="E.VALIDATION.DTYPE")
save("nonfinite_b", {"A": enc(np.eye(2)), "b": {"dtype": "f64", "shape": [2], "data": [1.0, "Inf"]}}, expected_error="E.VALIDATION.NON_FINITE")
save("missing_b", {"A": enc(np.eye(2))}, expected_error="E.VALIDATION.SCHEMA")
save("bad_refinement", {"A": enc(np.eye(2)), "b": enc([1.0, 1.0]), "refinement": "always"}, expected_error="E.VALIDATION.SCHEMA")
print("linsolve golden:", count)

# ---- published examples
E = f"examples/{OP}"
A_ex = np.array([[4.0, -2.0, 1.0], [-2.0, 4.0, -2.0], [1.0, -2.0, 4.0]])
b_ex = np.array([11.0, -16.0, 17.0])
t_ex = np.array([0.0, 1.0, 2.0, 3.0, 4.0])
ex = {"request.json": {"A": enc(A_ex), "b": enc(b_ex)},
      "request.lsq.json": {"A": enc(np.vander(t_ex, 2)), "b": enc([1.1, 2.9, 5.2, 7.1, 8.8])},
      "request.ill.json": {"A": enc(H(13)), "b": enc(np.ones(13))},
      "request.error.json": {"A": enc([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]), "b": enc([1.0, 2.0])}}
ref = {"note": "numpy.linalg (LAPACK); x_true = mpmath 60 digits; NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name == "request.error.json":
        continue
    A = np.array(req["A"]["data"]).reshape(req["A"]["shape"]); b = np.array(req["b"]["data"])
    x = np.linalg.solve(A, b) if A.shape[0] == A.shape[1] else np.linalg.lstsq(A, b, rcond=None)[0]
    ref[name] = {"x": [float(v) for v in x], "x_true": [float(v) for v in truth(A, b.reshape(-1, 1)).ravel()]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("linsolve examples →", E)
