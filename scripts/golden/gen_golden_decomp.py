"""Golden cases for linalg.decomp.lu, linalg.decomp.qr and linalg.decomp.chol (spec v0.4.24, 10.42 – 10.44).

Truth for the stored f64 data, at 50 digits with mpmath:
  lu   — partial pivoting written out here (first maximal |a_ik|), so the permutation is the operator's rule; a case
         is rejected when two pivot candidates differ by less than a relative 1e-8 without being exactly equal
         (an exact tie is broken the same way in both: the first row);
  qr   — mpmath.qr, then the sign convention R_ii ≥ 0; each true factor is verified (‖A − QR‖ < 1e-40);
  chol — mpmath.cholesky of the matrix built from the upper triangle (the operator reads only that triangle).
numpy.linalg / scipy.linalg values are not used for comparison. Warnings are predicted from the truth only.
NOT MATLAB output. Golden store: golden/mpmath/<op>/0.4.0 (V-3).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 50
SEED = 20261129
rng = np.random.default_rng(SEED)
U = 2.0 ** -53
VER = "0.4.0"
ENV = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 50, "numpy": np.__version__, "python": platform.python_version(), "seed": SEED,
       "method": "partial-pivoting LU (first maximal pivot), QR (mpmath.qr, R_ii ≥ 0), Cholesky (mpmath.cholesky of the upper triangle) of the stored f64 data at 50 digits",
       "note": "high-precision truth; NOT MATLAB output"}


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

    def save(self, family, request, expected=None, expected_error=None, expect_warning=None):
        case = {"spec": "aeoi/0.4", "operator": self.op, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
        if expected is not None: case["expected"] = expected
        if expected_error is not None: case["expected_error"] = expected_error
        if expect_warning: case["expect_warning"] = expect_warning
        h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
        json.dump(case, open(f"{self.dir}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
        self.count += 1


def mpm(A):
    return mp.matrix([[mp.mpf(float(v)) for v in row] for row in A])


def flat(M, r, c):
    return [float(M[i, j]) for i in range(r) for j in range(c)]


def kappa1(A):
    n = A.shape[0]
    try:
        X = mp.inverse(mpm(A))
    except ZeroDivisionError:
        return None
    na = max(sum(abs(float(A[i, j])) for i in range(n)) for j in range(n))
    nx = max(mp.fsum(abs(X[i, j]) for i in range(n)) for j in range(n))
    return float(na * nx)


def kappa2(A):
    s = mp.svd_r(mpm(A), compute_uv=False)
    s = sorted([s[k] for k in range(len(s))], reverse=True)
    return float(s[0] / s[-1]) if s[-1] != 0 else None


# ============================================================================================== lu
LU = Saver("linalg.decomp.lu")


def lu_truth(A):
    n = A.shape[0]
    M = mpm(A); perm = list(range(n)); zero = None
    for k in range(n):
        cand = [abs(M[i, k]) for i in range(k, n)]
        best = max(cand)
        p = k + cand.index(best)
        others = [c for c in cand if c != best]
        if others and best != 0:
            assert max(others) < best * (1 - mp.mpf("1e-8")), "near-tie in the pivot choice"
        if p != k:
            for j in range(n):
                M[k, j], M[p, j] = M[p, j], M[k, j]
            perm[k], perm[p] = perm[p], perm[k]
        if M[k, k] == 0:
            zero = k if zero is None else zero
            continue
        for i in range(k + 1, n):
            M[i, k] = M[i, k] / M[k, k]
            for j in range(k + 1, n):
                M[i, j] -= M[i, k] * M[k, j]
    L = mp.eye(n); Uu = mp.zeros(n, n)
    for i in range(n):
        for j in range(n):
            if j < i: L[i, j] = M[i, j]
            else: Uu[i, j] = M[i, j]
    return perm, L, Uu, zero


def lu_case(fam, A):
    A = np.asarray(A, float); n = A.shape[0]
    perm, L, Uu, zero = lu_truth(A)
    exp = {"n": n, "perm_true": perm, "L_true": flat(L, n, n), "U_true": flat(Uu, n, n), "kappa1_true": kappa1(A), "zero_pivot_true": zero}
    LU.save(fam, {"A": enc(A)}, exp, expect_warning="W.NUMERICAL.ZERO_PIVOT" if zero is not None else None)


def wilkinson(n):
    W = np.eye(n) - np.tril(np.ones((n, n)), -1); W[:, -1] = 1.0
    return W


for n in (1, 2, 5, 20):
    lu_case("random", rng.normal(size=(n, n)))
lu_case("integer", [[2.0, 1.0, 1.0], [4.0, -6.0, 0.0], [-2.0, 7.0, 2.0]])
lu_case("needs_pivoting", [[1e-20, 1.0], [1.0, 1.0]])
lu_case("wilkinson_growth_40", wilkinson(40))
# Hilbert matrices have near-ties among the pivot candidates (rejected); an ill-conditioned matrix with clear pivots:
Qa, _ = np.linalg.qr(rng.normal(size=(6, 6))); Qb, _ = np.linalg.qr(rng.normal(size=(6, 6)))
lu_case("ill_conditioned_1e10", Qa @ np.diag(10.0 ** -np.arange(0, 12, 2)) @ Qb)
lu_case("singular_rank1", [[1.0, 2.0], [2.0, 4.0]])
lu_case("singular_zero_column", [[1.0, 0.0, 2.0], [3.0, 0.0, 1.0], [0.0, 0.0, 5.0]])
LU.save("not_square", {"A": enc(rng.normal(size=(2, 3)))}, expected_error="E.VALIDATION.SHAPE")
LU.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
LU.save("nonfinite", {"A": {"dtype": "f64", "shape": [1, 1], "data": ["NaN"]}}, expected_error="E.VALIDATION.NON_FINITE")
LU.save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
print("lu golden:", LU.count)

# ============================================================================================== qr
QR = Saver("linalg.decomp.qr")


def qr_case(fam, A, mode=None, unique=True):
    A = np.asarray(A, float); m, n = A.shape; k = min(m, n)
    if m >= n:
        Qm, Rm = mp.qr(mpm(A), mode="full")
    else:   # mpmath.qr needs m ≥ n: Q from the leading m×m block, then R = Qᵀ·A (upper trapezoidal)
        Qm, _ = mp.qr(mpm(A[:, :m]), mode="full")
        Rm = Qm.T * mpm(A)
        for i in range(m):
            for j in range(i):
                assert abs(Rm[i, j]) < mp.mpf(10) ** -40; Rm[i, j] = 0
    for i in range(k):
        if Rm[i, i] < 0:
            for j in range(n): Rm[i, j] = -Rm[i, j]
            for r in range(m): Qm[r, i] = -Qm[r, i]
    Am = mpm(A)
    res = max(abs(Am[i, j] - mp.fsum(Qm[i, t] * Rm[t, j] for t in range(m))) for i in range(m) for j in range(n))
    assert res < mp.mpf(10) ** -40 * max(1, max(abs(float(v)) for v in A.ravel())), res
    qc = m if mode == "full" else k
    req = {"A": enc(A)}
    if mode: req["mode"] = mode
    exp = {"m": m, "n": n, "mode": mode or "econ", "unique": unique, "kappa2_true": kappa2(A[:, :k]) if unique else None,
           "Q_true": [float(Qm[i, j]) for i in range(m) for j in range(qc)] if unique else None,
           "R_true": [float(Rm[i, j]) for i in range(qc if mode == "full" else k) for j in range(n)] if unique else None}
    QR.save(fam, req, exp)


qr_case("tall", rng.normal(size=(6, 3)))
qr_case("square", rng.normal(size=(5, 5)))
qr_case("wide", rng.normal(size=(3, 6)))
qr_case("tall_full", rng.normal(size=(5, 3)), mode="full")
qr_case("wide_full", rng.normal(size=(2, 4)), mode="full")
qr_case("scalar_negative", [[-3.0]])
qr_case("hilbert_8", np.array([[1.0 / (i + j + 1) for j in range(8)] for i in range(8)]))
qr_case("graded_columns", rng.normal(size=(6, 4)) @ np.diag([1.0, 1e-3, 1e-6, 1e-9]))
Bq = rng.normal(size=(5, 2))
qr_case("rank_deficient", np.column_stack([Bq, Bq @ np.array([1.0, -2.0])]), unique=False)
QR.save("one_d", {"A": enc([1.0, 2.0, 3.0])}, expected_error="E.VALIDATION.SHAPE")
QR.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
QR.save("bad_mode", {"A": enc(np.eye(2)), "mode": "economy"}, expected_error="E.VALIDATION.SCHEMA")
QR.save("full_too_large", {"A": enc(np.ones((1500, 1))), "mode": "full"}, expected_error="E.RESOURCE.OUTPUT_TOO_LARGE")
print("qr golden:", QR.count)

# ============================================================================================== chol
CH = Saver("linalg.decomp.chol")


def chol_case(fam, A, triangle=None):
    A = np.asarray(A, float); n = A.shape[0]
    S = np.triu(A) + np.triu(A, 1).T          # the matrix the operator factors
    L = mp.cholesky(mpm(S))
    Lf = flat(L, n, n)
    out = Lf if triangle == "lower" else [float(L[j, i]) for i in range(n) for j in range(n)]
    req = {"A": enc(A)}
    if triangle: req["triangle"] = triangle
    CH.save(fam, req, {"n": n, "R_true": out, "triangle": triangle or "upper", "kappa1_true": kappa1(S)})


for n in (1, 3, 10, 30):
    B = rng.normal(size=(n, n)); chol_case("random_spd", B.T @ B + n * np.eye(n))
chol_case("lower_triangle", np.array([[4.0, 2.0, 2.0], [2.0, 5.0, 3.0], [2.0, 3.0, 6.0]]), triangle="lower")
chol_case("hilbert_8", np.array([[1.0 / (i + j + 1) for j in range(8)] for i in range(8)]))
Qo, _ = np.linalg.qr(rng.normal(size=(6, 6)))
Ar = Qo @ np.diag([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]) @ Qo.T   # rounding asymmetry, inside the tolerance
assert 0 < np.max(np.abs(Ar - Ar.T)) <= 10 * 6 * U * np.max(np.abs(Ar))
chol_case("rounding_asymmetry_accepted", Ar)
CH.save("indefinite", {"A": enc([[1.0, 2.0], [2.0, 1.0]])}, expected_error="E.DOMAIN.NOT_POSITIVE_DEFINITE")
CH.save("semidefinite", {"A": enc([[1.0, 1.0], [1.0, 1.0]])}, expected_error="E.DOMAIN.NOT_POSITIVE_DEFINITE")
CH.save("not_symmetric", {"A": enc([[2.0, 1.0], [0.0, 2.0]])}, expected_error="E.VALIDATION.NOT_SYMMETRIC")
CH.save("not_square", {"A": enc(rng.normal(size=(2, 3)))}, expected_error="E.VALIDATION.SHAPE")
CH.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
print("chol golden:", CH.count)

# ============================================================================================== examples
ex = {"linalg.decomp.lu": {"request.json": {"A": enc([[2.0, 1.0, 1.0], [4.0, -6.0, 0.0], [-2.0, 7.0, 2.0]])},
                           "request.singular.json": {"A": enc([[1.0, 2.0], [2.0, 4.0]])},
                           "request.error.json": {"A": enc([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])}},
      "linalg.decomp.qr": {"request.json": {"A": enc([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]])},
                           "request.full.json": {"A": enc([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]), "mode": "full"},
                           "request.error.json": {"A": enc([1.0, 2.0, 3.0])}},
      "linalg.decomp.chol": {"request.json": {"A": enc([[4.0, 2.0, 2.0], [2.0, 5.0, 3.0], [2.0, 3.0, 6.0]])},
                             "request.error.json": {"A": enc([[1.0, 2.0], [2.0, 1.0]])},
                             "request.asymmetric.json": {"A": enc([[2.0, 1.0], [0.0, 2.0]])}}}
for op, reqs in ex.items():
    os.makedirs(f"examples/{op}", exist_ok=True)
    for name, req in reqs.items():
        open(f"examples/{op}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
print("examples written")
