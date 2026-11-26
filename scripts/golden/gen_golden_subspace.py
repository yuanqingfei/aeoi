"""Golden cases for linalg.subspace.rank, linalg.subspace.basis, linalg.matrix.pinv, linalg.matrix.norm and
linalg.matrix.cond (spec v0.4.23, 10.37 – 10.41).

Truth for the stored f64 data: mpmath at 50 digits (SVD, inverse, p-norms); the 1-, inf- and Frobenius-squared
norms are also computed exactly with fractions and asserted equal. numpy.linalg (LAPACK gesdd / getrf) values are
recorded for comparison only. Warnings and failed post-conditions are predicted from the truth alone, and only
cases clearly on one side of each threshold are written. NOT MATLAB output.
Golden store: golden/mpmath/<op>/0.4.0 (V-3).
"""
import hashlib, json, os, platform, shutil, math
from fractions import Fraction as F
import numpy as np
import mpmath as mp

mp.mp.dps = 50
SEED = 20261128
rng = np.random.default_rng(SEED)
U = 2.0 ** -53
VER = "0.4.0"
ENV = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 50, "numpy": np.__version__, "python": platform.python_version(), "seed": SEED,
       "method": "SVD (mpmath svd_r), inverse and p-norms of the stored f64 data at 50 digits; 1-, inf- and squared Frobenius norms also exact with fractions",
       "note": "high-precision truth; numpy.linalg recorded for comparison; NOT MATLAB output"}


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


def eps_of(x):
    x = abs(float(x))
    return math.ldexp(1.0, -1074) if x == 0 else float(np.spacing(x))


def true_svd(A):
    """Singular values (descending), U (m×p), V (n×p) of the stored data at 50 digits."""
    m, n = A.shape
    Um, S, Vt = mp.svd_r(mpm(A), full_matrices=False, compute_uv=True)
    p = min(m, n)
    s = [S[k] for k in range(p)]
    order = sorted(range(p), key=lambda k: -s[k])
    s = [s[k] for k in order]
    Uc = [[Um[i, k] for i in range(m)] for k in order]
    Vc = [[Vt[k, j] for j in range(n)] for k in order]
    # verify: ‖A − U S Vᵀ‖ tiny at 50 digits
    Am = mpm(A)
    res = max(abs(Am[i, j] - sum(Uc[k][i] * s[k] * Vc[k][j] for k in range(p))) for i in range(m) for j in range(n))
    assert res < mp.mpf(10) ** -40 * max(mp.mpf(1), s[0]), res
    return s, Uc, Vc


def rank_truth(A, tol=None):
    """(rank, tol, ambiguous?) from the true singular values; asserts the case is clearly on one side."""
    m, n = A.shape
    s, Uc, Vc = true_svd(A)
    t = tol if tol is not None else max(m, n) * eps_of(float(s[0]))
    r = sum(1 for v in s if v > t)
    amb = any(t / 10 < v <= 100 * t for v in s)          # the band of RK-3
    # every value at least a factor 2 from each edge (tol/10, tol, 100·tol), so rounding cannot flip a prediction
    clear = all(v == 0 or min(abs(math.log2(float(v) / e)) for e in (t / 10, t, 100 * t)) >= 1 for v in s)
    assert clear, f"singular values too close to a band edge: {[float(v) for v in s]} (tol {t:.3e})"
    return r, t, amb, s, Uc, Vc


def floats(v):
    return [float(x) for x in v]


# ============================================================================================== matrices
def lowrank(m, n, r, scale=1.0):
    return (rng.integers(-4, 5, size=(m, r)) @ rng.integers(-4, 5, size=(r, n))).astype(float) * scale


H = lambda n: np.array([[1.0 / (i + j + 1) for j in range(n)] for i in range(n)])
MATS = [("random_square", rng.normal(size=(6, 6))), ("random_tall", rng.normal(size=(9, 4))), ("random_wide", rng.normal(size=(3, 7))),
        ("lowrank_6x5_r2", lowrank(6, 5, 2)), ("lowrank_8x8_r4", lowrank(8, 8, 4)), ("lowrank_4x9_r1", lowrank(4, 9, 1)),
        ("magic3_like_r2", np.array([[1., 2., 3.], [4., 5., 6.], [7., 8., 9.]])), ("zero", np.zeros((3, 4))),
        ("graded_kept", np.diag([1.0, 1e-4, 1e-10])), ("hilbert_8", H(8)), ("single_column", rng.normal(size=(5, 1)))]

# ============================================================================================== rank
RK = Saver("linalg.subspace.rank")


def rank_case(fam, A, tol=None):
    A = np.asarray(A, float); m, n = A.shape
    r, t, amb, s, _, _ = rank_truth(A, tol)
    req = {"A": enc(A)}
    if tol is not None: req["tol"] = tol
    exp = {"rank_true": r, "tol": t, "tol_source": "supplied" if tol is not None else "default", "s_true": floats(s), "m": m, "n": n,
           "numpy_rank": int(np.linalg.matrix_rank(A, tol=tol))}
    RK.save(fam, req, exp, expect_warning="W.NUMERICAL.RANK_AMBIGUOUS" if amb else None, expect_postcondition_failed=["rank_well_separated"] if amb else None)


for fam, A in MATS:
    rank_case(fam, A)
noisy = lowrank(10, 8, 3) + 1e-9 * rng.normal(size=(10, 8))
rank_case("noisy_lowrank_supplied_tol", noisy, tol=1e-6)
rank_case("noisy_lowrank_default_tol", noisy)
t2 = 2 * eps_of(1.0)
rank_case("ambiguous_near_threshold", np.diag([1.0, 5 * t2]))
RK.save("one_d", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
RK.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
RK.save("nonfinite", {"A": {"dtype": "f64", "shape": [1, 2], "data": [1.0, "NaN"]}}, expected_error="E.VALIDATION.NON_FINITE")
RK.save("negative_tol", {"A": enc(np.eye(2)), "tol": -1.0}, expected_error="E.VALIDATION.SCHEMA")
print("rank golden:", RK.count)

# ============================================================================================== basis
BS = Saver("linalg.subspace.basis")


def basis_case(fam, A, tol=None):
    A = np.asarray(A, float); m, n = A.shape
    r, t, amb, s, Uc, Vc = rank_truth(A, tol)
    # orthonormal bases of the true range / null space: range = span(u_1..u_r); null = complement of span(v_1..v_r)
    Q = mp.matrix([[Vc[k][j] for k in range(r)] for j in range(n)]) if r else None
    P_null = mp.eye(n) - (Q * Q.T if r else mp.zeros(n, n))
    Uo = [[float(Uc[k][i]) for k in range(r)] for i in range(m)]
    gap = float(s[r - 1] - (s[r] if r < len(s) else 0)) if r else None
    req = {"A": enc(A)}
    if tol is not None: req["tol"] = tol
    exp = {"rank_true": r, "tol": t, "m": m, "n": n, "orth_true": [x for row in Uo for x in row],
           "null_projector_true": [float(P_null[i, j]) for i in range(n) for j in range(n)], "gap_true": gap, "sigma_max_true": float(s[0])}
    BS.save(fam, req, exp, expect_warning="W.NUMERICAL.RANK_AMBIGUOUS" if amb else None, expect_postcondition_failed=["rank_well_separated"] if amb else None)


for fam, A in MATS:
    if fam != "zero":
        basis_case(fam, A)
basis_case("zero_rank0", np.zeros((2, 3)))
basis_case("noisy_lowrank_supplied_tol", noisy, tol=1e-6)
basis_case("ambiguous_near_threshold", np.diag([1.0, 5 * t2]))
BS.save("one_d", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
BS.save("complex", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
BS.save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
print("basis golden:", BS.count)

# ============================================================================================== pinv
PV = Saver("linalg.matrix.pinv")


def pinv_case(fam, A, tol=None):
    A = np.asarray(A, float); m, n = A.shape
    r, t, amb, s, Uc, Vc = rank_truth(A, tol)
    X = [[sum(Vc[k][j] * Uc[k][i] / s[k] for k in range(r)) for i in range(m)] for j in range(n)]
    req = {"A": enc(A)}
    if tol is not None: req["tol"] = tol
    exp = {"X_true": [float(v) for row in X for v in row], "rank_true": r, "m": m, "n": n, "cond_r_true": float(s[0] / s[r - 1]) if r else None,
           "numpy_X": [float(v) for v in np.linalg.pinv(A, rcond=None).ravel()] if tol is None else None}
    PV.save(fam, req, exp, expect_warning="W.NUMERICAL.RANK_AMBIGUOUS" if amb else None, expect_postcondition_failed=["rank_well_separated"] if amb else None)


for fam, A in MATS:
    if fam not in ("hilbert_8",):
        pinv_case(fam, A)
pinv_case("noisy_lowrank_supplied_tol", noisy, tol=1e-6)
pinv_case("ambiguous_near_threshold", np.diag([1.0, 5 * t2]))
PV.save("one_d", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
PV.save("nonfinite", {"A": {"dtype": "f64", "shape": [1, 2], "data": [1.0, "Inf"]}}, expected_error="E.VALIDATION.NON_FINITE")
PV.save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
print("pinv golden:", PV.count)

# ============================================================================================== norm
NM = Saver("linalg.matrix.norm")


def exact_sum_abs(v):
    return sum((abs(F(float(x))) for x in v), F(0))


def norm_truth(x, p):
    x = np.asarray(x, float)
    vec = x.ndim == 1 or 1 in x.shape
    if vec:
        v = x.ravel()
        if p == "inf": return float(np.max(np.abs(v))), True
        if p == "-inf": return float(np.min(np.abs(v))), True
        if p == 1:
            e = exact_sum_abs(v); t = mp.fsum(abs(mp.mpf(float(a))) for a in v)
            assert abs(mp.mpf(e.numerator) / e.denominator - t) <= mp.mpf(10) ** -40 * max(t, 1)
            return float(t), True
        q = 2 if p in (2, "fro") else p
        if q == 2:
            e = sum((F(float(a)) ** 2 for a in v), F(0)); t = mp.sqrt(mp.mpf(e.numerator) / e.denominator)
            return float(t), True
        t = mp.fsum(abs(mp.mpf(float(a))) ** q for a in v) ** (mp.mpf(1) / q)
        return float(t), True
    m, n = x.shape
    if p == 1: return float(max(exact_sum_abs(x[:, j]) for j in range(n))), False
    if p == "inf": return float(max(exact_sum_abs(x[i, :]) for i in range(m))), False
    if p == "fro":
        e = sum((F(float(a)) ** 2 for a in x.ravel()), F(0)); return float(mp.sqrt(mp.mpf(e.numerator) / e.denominator)), False
    s, _, _ = true_svd(x)
    return float(s[0]), False


def norm_case(fam, x, p=None):
    x = np.asarray(x, float)
    t, vec = norm_truth(x, 2 if p is None else p)
    req = {"x": enc(x)}
    if p is not None: req["p"] = p
    NM.save(fam, req, {"norm_true": t, "treated_as": "vector" if vec else "matrix", "n_elements": int(x.size), "max_dim": int(max(x.shape))})


v = rng.normal(size=50)
for p in (None, 1, "inf", "-inf", "fro", 3, 1.5, 10):
    norm_case(f"vector_p_{p}", v, p)
norm_case("vector_row_matrix_p1", v.reshape(1, -1), 1)
norm_case("vector_huge_no_overflow", np.array([1e300, 1e300, -1e300]))
norm_case("vector_tiny_no_underflow", np.array([3e-300, 4e-300]))
A = rng.normal(size=(7, 5))
for p in (None, 1, "inf", "fro"):
    norm_case(f"matrix_p_{p}", A, p)
norm_case("matrix_lowrank_2", lowrank(6, 6, 2))
NM.save("matrix_p3", {"x": enc(A), "p": 3}, expected_error="E.VALIDATION.RANGE")
NM.save("p_below_1", {"x": enc(v), "p": 0.5}, expected_error="E.VALIDATION.SCHEMA")
NM.save("complex", {"x": {"dtype": "c128", "shape": [1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
NM.save("overflow", {"x": enc([1.7e308, 1.7e308])}, expected_error="E.NUMERICAL.NON_FINITE_EVALUATION")
print("norm golden:", NM.count)

# ============================================================================================== cond
CD = Saver("linalg.matrix.cond")


def cond_case(fam, A, p=None):
    A = np.asarray(A, float); m, n = A.shape
    q = 2 if p is None else p
    if q == 2:
        s, _, _ = true_svd(A); t = s[0] / s[-1]
    else:
        Ai = mp.inverse(mpm(A)); Am = mpm(A)
        def nrm(M):
            if q == 1: return max(mp.fsum(abs(M[i, j]) for i in range(n)) for j in range(n))
            if q == "inf": return max(mp.fsum(abs(M[i, j]) for j in range(n)) for i in range(n))
            return mp.sqrt(mp.fsum(M[i, j] ** 2 for i in range(n) for j in range(n)))
        t = nrm(Am) * nrm(Ai)
    t = float(t)
    assert t < 1e14 or t > 1e17, f"{fam}: cond {t:.3e} too close to the warning threshold"
    req = {"A": enc(A)}
    if p is not None: req["p"] = p
    CD.save(fam, req, {"cond_true": t, "max_dim": max(m, n), "numpy_cond": float(np.linalg.cond(A, p={"inf": np.inf}.get(q, q)))},
            expect_warning="W.NUMERICAL.ILL_CONDITIONED" if t > 1e17 else None)


B = rng.normal(size=(6, 6)) + 2 * np.eye(6)
for p in (None, 1, "inf", "fro"):
    cond_case(f"square_p_{p}", B, p)
cond_case("rectangular_p2", rng.normal(size=(8, 3)))
cond_case("hilbert_6", H(6))
cond_case("hilbert_6_p1", H(6), 1)
cond_case("hilbert_13_ill", H(13))
cond_case("diag_graded", np.diag([1.0, 1e-3, 1e-6]))
CD.save("singular_zero_pivot_inf", {"A": enc([[1.0, 2.0], [2.0, 4.0]]), "p": "inf"}, {"cond_true": None, "max_dim": 2, "numpy_cond": None}, expect_warning="W.NUMERICAL.ILL_CONDITIONED")
CD.save("rectangular_p1", {"A": enc(rng.normal(size=(3, 2))), "p": 1}, expected_error="E.VALIDATION.SHAPE")
CD.save("bad_p", {"A": enc(np.eye(2)), "p": 3}, expected_error="E.VALIDATION.SCHEMA")
CD.save("one_d", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
print("cond golden:", CD.count)

# ============================================================================================== examples
M3 = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]
ex = {"linalg.subspace.rank": {"request.json": {"A": enc(M3)}, "request.ambiguous.json": {"A": enc(np.diag([1.0, 5 * t2]))},
                               "request.error.json": {"A": enc([1.0, 2.0, 3.0])}},
      "linalg.subspace.basis": {"request.json": {"A": enc(M3)}, "request.error.json": {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}},
      "linalg.matrix.pinv": {"request.json": {"A": enc([[1.0, 2.0], [2.0, 4.0], [3.0, 6.0]])}, "request.ambiguous.json": {"A": enc(np.diag([1.0, 5 * t2]))},
                             "request.error.json": {"A": enc([1.0, 2.0])}},
      "linalg.matrix.norm": {"request.json": {"x": enc([3.0, -4.0])}, "request.matrix.json": {"x": enc([[1.0, -2.0], [3.0, 4.0]]), "p": "inf"},
                             "request.error.json": {"x": enc([[1.0, -2.0], [3.0, 4.0]]), "p": 3}},
      "linalg.matrix.cond": {"request.json": {"A": enc(H(6))}, "request.ill.json": {"A": enc(H(13))},
                             "request.error.json": {"A": enc([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]), "p": 1}}}
for op, reqs in ex.items():
    E = f"examples/{op}"
    for name, req in reqs.items():
        open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
print("examples written")
