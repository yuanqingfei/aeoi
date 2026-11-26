"""Golden cases for linalg.matrix.inv and linalg.matrix.det (spec v0.4.22, 10.35, 10.36).

Engines: numpy.linalg.inv / numpy.linalg.det / numpy.linalg.slogdet (LAPACK dgetrf + dgetri) as the everyday
reference, and the TRUTH for the stored f64 data: mpmath at 80 digits (inverse, determinant), cross-checked in
exact rational arithmetic (fractions) for every case with n ≤ 8, and exactly singular matrices are verified to
have det = 0 as rationals. Warnings and failed post-conditions are predicted here from the truth only (κ₁ of the
true inverse, the true determinant), never from the operator. NOT MATLAB output.
Golden store: golden/mpmath/<op>/0.4.0 (V-3: the comparison is against the mpmath truth).
"""
import hashlib, json, os, platform, shutil
from fractions import Fraction as F
import numpy as np
import mpmath as mp

mp.mp.dps = 80
SEED = 20261127
rng = np.random.default_rng(SEED)
U = 2.0 ** -53
VER = "0.4.0"
ENV = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 80, "numpy": np.__version__, "python": platform.python_version(), "seed": SEED,
       "method": "inverse and determinant of the stored f64 matrix at 80 digits (mpmath LU); n ≤ 8 cross-checked with exact rationals",
       "note": "high-precision truth; numpy.linalg values recorded for comparison; NOT MATLAB output"}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


class Saver:
    def __init__(self, op):
        self.op, self.count = op, 0
        self.dir = f"golden/mpmath/{op}/{VER}"
        shutil.rmtree(self.dir, ignore_errors=True); os.makedirs(self.dir)

    def save(self, family, request, expected=None, expected_error=None, expect_warning=None, expect_postcondition_failed=None):
        case = {"spec": "aeoi/0.4", "operator": self.op, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
        if expected is not None: case["expected"] = expected
        if expected_error is not None: case["expected_error"] = expected_error
        if expect_warning: case["expect_warning"] = expect_warning
        if expect_postcondition_failed: case["expect_postcondition_failed"] = expect_postcondition_failed
        h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
        json.dump(case, open(f"{self.dir}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
        self.count += 1


def mpm(A):
    return mp.matrix([[mp.mpf(float(v)) for v in row] for row in A])


def frac_det(A):
    """Exact determinant of the stored f64 matrix (Bareiss-free Gaussian elimination over the rationals)."""
    M = [[F(float(v)) for v in row] for row in A]
    n = len(M); d = F(1)
    for k in range(n):
        p = next((i for i in range(k, n) if M[i][k] != 0), None)
        if p is None:
            return F(0)
        if p != k:
            M[k], M[p] = M[p], M[k]; d = -d
        d *= M[k][k]
        for i in range(k + 1, n):
            f = M[i][k] / M[k][k]
            if f:
                for j in range(k, n):
                    M[i][j] -= f * M[k][j]
    return d


def frac_inv(A):
    n = len(A)
    M = [[F(float(v)) for v in row] + [F(int(i == j)) for j in range(n)] for i, row in enumerate(A)]
    for k in range(n):
        p = next(i for i in range(k, n) if M[i][k] != 0)
        M[k], M[p] = M[p], M[k]
        pv = M[k][k]
        M[k] = [v / pv for v in M[k]]
        for i in range(n):
            if i != k and M[i][k]:
                f = M[i][k]
                M[i] = [a - f * b for a, b in zip(M[i], M[k])]
    return [[float(v) for v in row[n:]] for row in M]


def inv_truth(A):
    n = A.shape[0]
    Xm = mp.inverse(mpm(A))
    X = np.array([[float(Xm[i, j]) for j in range(n)] for i in range(n)])
    if n <= 8:
        Xf = np.array(frac_inv(A))
        assert np.array_equal(X, Xf), "mpmath inverse disagrees with the exact rational inverse"
    return X


def kappa1(A, X):
    return float(np.max(np.sum(np.abs(A), axis=0)) * np.max(np.sum(np.abs(X), axis=0)))


# ============================================================================================== inv
IV = Saver("linalg.matrix.inv")


def inv_case(family, A, refinement=None, ill=False):
    A = np.asarray(A, float); n = A.shape[0]
    X = inv_truth(A)
    k = kappa1(A, X)
    req = {"A": enc(A)}
    if refinement: req["refinement"] = refinement
    Xnp = np.linalg.inv(A)
    exp = {"X_true": [float(v) for v in X.ravel()], "n": n, "kappa1_true": k, "method": "lu_partial_pivoting",
           "numpy_forward_error": float(np.sum(np.abs(Xnp - X), axis=0).max() / np.sum(np.abs(X), axis=0).max())}
    # predicted from the truth: rcond < ε ⇒ warning; κ·u ≥ 10 ⇒ no correct digit can be guaranteed (κ₁·η ≥ 1)
    w = "W.NUMERICAL.ILL_CONDITIONED" if 1.0 / k < np.finfo(float).eps else None
    pc = ["forward_error_below_one"] if k * U >= 10 else None
    assert (w is not None) == ill and (pc is not None) == ill, f"{family}: κ₁ = {k:.3e} is not clearly on the {'ill' if ill else 'well'}-conditioned side"
    IV.save(family, req, exp, expect_warning=w, expect_postcondition_failed=pc)


H = lambda n: np.array([[1.0 / (i + j + 1) for j in range(n)] for i in range(n)])
for n in (1, 2, 3, 5, 10, 30, 50):
    inv_case("random", rng.normal(size=(n, n)) + 0.1 * n * np.eye(n))
inv_case("integer_unimodular", [[2.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 2.0]])
inv_case("needs_pivoting", [[1e-20, 1.0], [1.0, 1.0]])
Q, _ = np.linalg.qr(rng.normal(size=(8, 8)))
inv_case("orthogonal", Q)
inv_case("graded_scaling", np.diag(10.0 ** np.arange(-6, 7, 3)) @ (rng.normal(size=(5, 5)) + 2 * np.eye(5)))
inv_case("refinement_none", rng.normal(size=(12, 12)) + np.eye(12), refinement="none")
inv_case("hilbert_6", H(6))
inv_case("hilbert_10", H(10))
inv_case("hilbert_13_ill", H(13), ill=True)
IV.save("singular_rank1", {"A": enc([[1.0, 2.0], [2.0, 4.0]])}, expected_error="E.NUMERICAL.SINGULAR")
IV.save("singular_zero_column", {"A": enc([[1.0, 0.0, 2.0], [3.0, 0.0, 1.0], [0.0, 0.0, 5.0]])}, expected_error="E.NUMERICAL.SINGULAR")
IV.save("not_square", {"A": enc(rng.normal(size=(2, 3)))}, expected_error="E.VALIDATION.SHAPE")
IV.save("one_d", {"A": enc([1.0, 2.0, 3.0])}, expected_error="E.VALIDATION.SHAPE")
IV.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
IV.save("nonfinite", {"A": {"dtype": "f64", "shape": [2, 2], "data": [1.0, "NaN", 0.0, 1.0]}}, expected_error="E.VALIDATION.NON_FINITE")
IV.save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
IV.save("bad_refinement", {"A": enc(np.eye(2)), "refinement": "always"}, expected_error="E.VALIDATION.SCHEMA")
print("inv golden:", IV.count)

# ============================================================================================== det
DT = Saver("linalg.matrix.det")


def det_case(family, A, exact_zero=False):
    A = np.asarray(A, float); n = A.shape[0]
    d = mp.det(mpm(A))
    if n <= 8 or exact_zero:
        df = frac_det(A)
        assert (df == 0) == exact_zero, f"{family}: exact det {df}"
        if df != 0:
            assert abs(mp.mpf(df.numerator) / df.denominator - d) <= mp.mpf(10) ** -60 * abs(d), f"{family}: mpmath det disagrees with the rational det"
        else:
            d = mp.mpf(0)
    sign_np, logdet_np = np.linalg.slogdet(A)
    exp = {"n": n, "method": "lu_partial_pivoting", "numpy_det": float(np.linalg.det(A)) if np.isfinite(np.linalg.det(A)) else None,
           "numpy_log_abs_det": float(logdet_np) if np.isfinite(logdet_np) else None}
    w, pc = [], None
    if d == 0:
        exp.update({"det_true": 0.0, "sign_true": 0, "log_abs_det_true": None, "kappa1_true": None})
        # a singular matrix: the operator must not claim a significant det (claim check in the rule)
        w = ["W.NUMERICAL.DET_NOT_SIGNIFICANT"]; pc = ["significant"]
    else:
        X = inv_truth(A)
        la = mp.log(abs(d))
        exp.update({"det_true": float(d) if abs(la) < 700 else None, "sign_true": 1 if d > 0 else -1, "log_abs_det_true": float(la), "kappa1_true": kappa1(A, X)})
        assert n * U * exp["kappa1_true"] < 1e-3, f"{family}: not clearly significant (κ₁ = {exp['kappa1_true']:.2e})"
        # DT-3: outside the f64 normal range (ln 2^1024 ≈ 709.78, ln 2^-1022 ≈ −708.40); stay clear of both edges
        assert abs(float(la) - 709.78) > 1 and abs(float(la) + 708.40) > 1, f"{family}: ln|det| too close to the range edge"
        if float(la) >= 709.78 or float(la) < -708.40:
            w = ["W.NUMERICAL.DET_OUT_OF_RANGE"]
    DT.save(family, {"A": enc(A)}, exp, expect_warning=w[0] if len(w) == 1 else (w or None), expect_postcondition_failed=pc)


for n in (1, 2, 3, 5, 10, 50, 200):
    det_case("random", rng.normal(size=(n, n)) + 0.1 * n * np.eye(n))
det_case("integer_exact", [[2.0, 1.0, 1.0], [4.0, -6.0, 0.0], [-2.0, 7.0, 2.0]])
det_case("permutation_odd", np.eye(4)[[1, 0, 2, 3]])
det_case("triangular", np.triu(rng.normal(size=(6, 6))) + 3 * np.eye(6))
det_case("needs_pivoting", [[1e-20, 1.0], [1.0, 1.0]])
det_case("tiny_but_well_conditioned", 0.1 * np.eye(100))
det_case("hilbert_8", H(8))
det_case("overflow", 100.0 * rng.normal(size=(150, 150)) + 150 * np.eye(150))
det_case("underflow", 1e-4 * (rng.normal(size=(120, 120)) + 0.1 * 120 * np.eye(120)))
det_case("singular_magic4", [[16.0, 2.0, 3.0, 13.0], [5.0, 11.0, 10.0, 8.0], [9.0, 7.0, 6.0, 12.0], [4.0, 14.0, 15.0, 1.0]], exact_zero=True)
det_case("singular_rank1", [[1.0, 2.0], [2.0, 4.0]], exact_zero=True)
Bi = rng.integers(-5, 6, size=(6, 3)).astype(float)
det_case("singular_rank3_integer", Bi @ rng.integers(-5, 6, size=(3, 6)).astype(float), exact_zero=True)
DT.save("not_square", {"A": enc(rng.normal(size=(3, 2)))}, expected_error="E.VALIDATION.SHAPE")
DT.save("one_d", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
DT.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
DT.save("nonfinite", {"A": {"dtype": "f64", "shape": [2, 2], "data": [1.0, "Inf", 0.0, 1.0]}}, expected_error="E.VALIDATION.NON_FINITE")
DT.save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
print("det golden:", DT.count)

# ============================================================================================== examples
ex = {"linalg.matrix.inv": {"request.json": {"A": enc([[4.0, 7.0], [2.0, 6.0]])},
                            "request.ill.json": {"A": enc(H(13))},
                            "request.error.json": {"A": enc([[1.0, 2.0], [2.0, 4.0]])}},
      "linalg.matrix.det": {"request.json": {"A": enc([[2.0, 1.0, 1.0], [4.0, -6.0, 0.0], [-2.0, 7.0, 2.0]])},
                            "request.singular.json": {"A": enc([[16.0, 2.0, 3.0, 13.0], [5.0, 11.0, 10.0, 8.0], [9.0, 7.0, 6.0, 12.0], [4.0, 14.0, 15.0, 1.0]])},
                            "request.error.json": {"A": enc([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])}}}
for op, reqs in ex.items():
    E = f"examples/{op}"
    ref = {"note": "truth for the stored data: mpmath 80 digits (exact rationals for n ≤ 8); numpy.linalg (LAPACK) for comparison; NOT MATLAB output"}
    for name, req in reqs.items():
        open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
        if name == "request.error.json":
            continue
        A = np.array(req["A"]["data"]).reshape(req["A"]["shape"])
        if op.endswith("inv"):
            ref[name] = {"X_true": [float(v) for v in inv_truth(A).ravel()], "numpy": [float(v) for v in np.linalg.inv(A).ravel()]}
        else:
            ref[name] = {"det_true": float(frac_det(A)), "numpy_det": float(np.linalg.det(A))}
    open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1, allow_nan=False) + "\n")
print("examples written")
