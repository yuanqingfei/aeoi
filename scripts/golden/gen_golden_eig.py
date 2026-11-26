"""Golden cases for linalg.decomp.eig (spec v0.4.11, 10.22).

Engines: numpy.linalg.eigh / eig (LAPACK dsyevd / dgeev) as the everyday reference, and mpmath (50 digits,
eigsy / eig with left and right vectors) on the *stored* f64 matrix: exact eigenvalues, eigenvectors and
eigenvalue condition numbers for the data as given. NOT MATLAB output.
Sorting of the reference follows the AEOI rule (10.22): symmetric ascending; general by (Re, |Im|),
conjugate pairs adjacent with Im > 0 first.
"""
import hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

mp.mp.dps = 50
SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "linalg.decomp.eig", "0.4.0"
ENV = {"engine": "numpy.linalg (LAPACK dsyevd / dgeev) / mpmath", "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
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
    json.dump(case, open(f"{DIR}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
    count += 1


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def key(z):
    return (z.real, abs(z.imag), z.imag < 0)


def truth(A, symmetric):
    n = A.shape[0]
    Am = mp.matrix([[mp.mpf(float(v)) for v in row] for row in A])
    if symmetric:
        E, Q = mp.eigsy(Am)
        lam = [complex(float(E[k]), 0.0) for k in range(n)]
        vecs = [[complex(float(Q[i, k]), 0.0) for i in range(n)] for k in range(n)]
        cond = [1.0] * n
    else:
        E, EL, ER = mp.eig(Am, left=True, right=True)
        lam = [complex(E[k]) for k in range(n)]
        vecs, cond = [], []
        for k in range(n):
            x = [ER[i, k] for i in range(n)]; y = [EL[k, i] for i in range(n)]  # y·A = λ·y (row)
            yx = mp.fsum([y[i] * x[i] for i in range(n)])
            nx = mp.sqrt(mp.fsum([abs(v) ** 2 for v in x])); ny = mp.sqrt(mp.fsum([abs(v) ** 2 for v in y]))
            cond.append(float(nx * ny / abs(yx)) if yx != 0 else float("inf"))
            vecs.append([complex(v / nx) for v in x])
    order = sorted(range(n), key=lambda k: key(lam[k]))
    return [lam[k] for k in order], [vecs[k] for k in order], [cond[k] for k in order]


def case(family, A, structure=None, vectors=None, expect_warning=None):
    A = np.asarray(A, float)
    n = A.shape[0]
    sym = structure != "general" and np.array_equal(A, A.T)
    lam, vecs, cond = truth(A, sym)
    ref = np.linalg.eigvalsh(A) if sym else np.linalg.eigvals(A)
    ref = sorted([complex(z) for z in ref], key=key)
    req = {"A": enc(A)}
    if structure: req["structure"] = structure
    if vectors is not None: req["vectors"] = vectors
    gaps = [min([abs(lam[j] - lam[k]) for j in range(n) if j != k], default=float("inf")) for k in range(n)]
    assert (max(cond) > 1e8) == (expect_warning is not None), (family, max(cond))
    inf = lambda v: "Infinity" if v == float("inf") else v
    cond = [inf(c) for c in cond]; gaps = [inf(g) for g in gaps]
    exp = {"structure": "symmetric" if sym else "general", "values_true": [[z.real, z.imag] for z in lam],
           "values_numpy": [[z.real, z.imag] for z in ref], "vectors_true": [[[z.real, z.imag] for z in v] for v in vecs],
           "eigenvalue_condition_true": cond, "gaps_true": gaps, "norm_fro": float(np.linalg.norm(A))}
    save(family, req, exp, expect_warning=expect_warning)


def sym(n):
    B = rng.normal(size=(n, n)); return B + B.T


for n in (1, 2, 5, 20):
    case("symmetric_random", sym(n))
Qo = np.linalg.qr(rng.normal(size=(4, 4)))[0]
S = Qo @ np.diag([1.0, 1.0, 2.0, 5.0]) @ Qo.T; S = (S + S.T) / 2
case("symmetric_repeated", S)
case("symmetric_identity", np.eye(3))
Dg = np.diag([1e6, 1.0, 1e-6]); Sg = sym(3)
case("symmetric_graded", Dg @ Sg @ Dg)
case("general_graded_warns", Dg @ rng.normal(size=(3, 3)) @ np.linalg.inv(Dg), expect_warning="W.NUMERICAL.ILL_CONDITIONED")
case("symmetric_tridiag_laplacian", 2 * np.eye(8) - np.eye(8, k=1) - np.eye(8, k=-1))
case("symmetric_forced_general", sym(4), structure="general")
case("symmetric_values_only", sym(6), vectors=False)
for n in (3, 6, 15):
    case("general_random", rng.normal(size=(n, n)))
case("general_rotation", [[0.0, -1.0], [1.0, 0.0]])
case("general_companion", [[-6.0, -11.0, -6.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
case("general_upper_triangular", [[1.0, 2.0, 3.0], [0.0, 4.0, 5.0], [0.0, 0.0, 6.0]])
P = rng.normal(size=(3, 3))
case("general_equal_real_parts", P @ np.array([[1.0, -2.0, 0.0], [2.0, 1.0, 0.0], [0.0, 0.0, 1.0]]) @ np.linalg.inv(P))
case("general_damped_oscillator", [[0.0, 1.0], [-4.0, -0.4]])
case("general_frank6", np.array([[min(i, j) + 1 if j >= i - 1 else 0 for j in range(6)] for i in range(6)][::-1])[::-1])
case("general_jordan3_warns", [[2.0, 1.0, 0.0], [0.0, 2.0, 1.0], [0.0, 0.0, 2.0]], expect_warning="W.NUMERICAL.ILL_CONDITIONED")
case("general_values_only", rng.normal(size=(5, 5)), vectors=False)
# ---- errors
save("not_square", {"A": enc(rng.normal(size=(2, 3)))}, expected_error="E.VALIDATION.SHAPE")
save("vector_A", {"A": enc([1.0, 2.0])}, expected_error="E.VALIDATION.SHAPE")
save("complex_A", {"A": {"dtype": "c128", "shape": [1, 1], "data": [[1.0, 1.0]]}}, expected_error="E.VALIDATION.DTYPE")
save("nan_A", {"A": {"dtype": "f64", "shape": [2, 2], "data": [1.0, "NaN", 0.0, 1.0]}}, expected_error="E.VALIDATION.NON_FINITE")
save("missing_A", {}, expected_error="E.VALIDATION.SCHEMA")
save("bad_structure", {"A": enc(np.eye(2)), "structure": "hermitian"}, expected_error="E.VALIDATION.SCHEMA")
print("eig golden:", count)

E = f"examples/{OP}"
os.makedirs(E, exist_ok=True)
ex = {"request.json": {"A": enc([[2.0, -1.0, 0.0], [-1.0, 2.0, -1.0], [0.0, -1.0, 2.0]])},
      "request.general.json": {"A": enc([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-2.0, -3.0, -2.0]])},
      "request.defective.json": {"A": enc([[2.0, 1.0, 0.0], [0.0, 2.0, 1.0], [0.0, 0.0, 2.0]])},
      "request.error.json": {"A": enc([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])}}
ref = {"note": "mpmath 50 digits, AEOI ordering; NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name == "request.error.json":
        continue
    A = np.array(req["A"]["data"]).reshape(req["A"]["shape"])
    lam, vecs, cond = truth(A, np.array_equal(A, A.T))
    ref[name] = {"values_true": [[z.real, z.imag] for z in lam], "eigenvalue_condition_true": ["Infinity" if c == float("inf") else c for c in cond]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("eig examples →", E)
