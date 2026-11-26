"""control.math.lyap (profile 10.10), the v0.1 second implementation moved onto the shared plumbing.

Numerics are scipy.linalg's Bartels-Stewart (LAPACK), the same lineage as the python-control golden
values (CF-11 `same_backend`). Unlike the four newer operators, this manifest is still the reference
manifest with method, backend, determinism, accuracy grade and exactness replaced (it predates the
spec-only rule); see SPEC_FINDINGS.md.
"""
import copy
import json
import os

import numpy as np
import scipy
from scipy import linalg

import aeoi_common as C
from aeoi_common import Fail

OP_ID = "control.math.lyap"
METHOD_ID = "pyscipy:lyap:bartels-stewart"


def _manifest():
    m = copy.deepcopy(json.load(open(os.path.join(C.REPO, "examples", OP_ID, "manifest.json"), encoding="utf-8")))
    m["input_schema"], m["output_schema"] = C.load_io_schemas(OP_ID)
    m["receipt"]["method_id"] = METHOD_ID
    m["backend"] = {"library": "scipy.linalg (LAPACK) via Python",
                    "routine": "solve_continuous_lyapunov / solve_discrete_lyapunov (Bartels-Stewart)",
                    "version": scipy.__version__, "binding": "ffi", "license": "BSD-3-Clause"}
    m["determinism"] = "platform_bitwise"
    m["accuracy_grade"] = {"current": "C", "note": "second implementation, no credential yet"}
    m["receipt"]["exactness"] = {"class": "approximate", "bound_kind": "a_posteriori_heuristic"}
    m["provenance"]["clean_room"] = C.CLEAN_ROOM
    m["x-pyscipy-blas"] = "scipy LAPACK/OpenBLAS (runtime CPU dispatch; single process)"
    return m


def _mat(args, name):
    a = C.read_array(args[name], f"/{name}", allowed=("f64", "i64"))
    if len(a.shape) != 2:
        raise Fail("E.VALIDATION.SHAPE", f"{name} 必须是二维矩阵", f"/{name}/shape")
    return a.values.astype(float)


class Lyap(C.Operator):
    OP_ID = OP_ID
    MANIFEST = _manifest()
    VERSION = MANIFEST["version"]

    def prepare(self, args):
        A, Q = _mat(args, "A"), _mat(args, "Q")
        n = A.shape[0]
        if A.shape != (n, n) or Q.shape != (n, n):
            raise Fail("E.VALIDATION.SHAPE", "A 必须是方阵，Q 与 A 同形", "/Q/shape")
        return dict(A=A, Q=Q, typ=args.get("type", "continuous"))

    def estimate(self, P):
        n = P["A"].shape[0]
        return {"n": n, "flops_estimate": 25 * n ** 3}

    def execute(self, P):
        A, Q, typ = P["A"], P["Q"], P["typ"]
        U = C.U
        warnings = []
        # 10.10: Q is solved as given (not symmetrized); a symmetric Q gets a symmetrized X.
        q_sym = np.linalg.norm(Q - Q.T, "fro") <= 1e-12 * max(np.linalg.norm(Q, "fro"), np.finfo(float).tiny)
        lam = np.linalg.eigvals(A)
        if typ == "continuous":
            sep = float(np.min(np.abs(lam[:, None] + np.conj(lam)[None, :])))
            stable = bool(np.all(lam.real < 0))
        else:
            sep = float(np.min(np.abs(lam[:, None] * np.conj(lam)[None, :] - 1.0)))
            stable = bool(np.all(np.abs(lam) < 1))
        if sep <= 1e3 * U * max(1.0, np.linalg.norm(A, 1)) ** (1 if typ == "continuous" else 2):
            raise Fail("E.DOMAIN.LYAP_SINGULAR", f"{typ} Lyapunov 方程奇异（最小分离 {sep:.2e}）", "/A", {"separation_min": sep},
                       retryable=False)
        try:
            X = linalg.solve_continuous_lyapunov(A, -Q) if typ == "continuous" else linalg.solve_discrete_lyapunov(A, Q)
        except (np.linalg.LinAlgError, ValueError) as e:
            raise Fail("E.NUMERICAL.NO_CONVERGENCE", f"scipy 求解失败：{e}", "/A")
        X = np.asarray(X, dtype=float)
        x_asym_before = float(np.linalg.norm(X - X.T) / max(np.linalg.norm(X), np.finfo(float).tiny))
        if q_sym:
            X = (X + X.T) / 2
        if not np.all(np.isfinite(X)):
            raise Fail("E.NUMERICAL.NO_CONVERGENCE", "解含非有限值", "/A")
        res = (A @ X + X @ A.T + Q) if typ == "continuous" else (A @ X @ A.T - X + Q)
        denom = (2 * np.linalg.norm(A) * np.linalg.norm(X) + np.linalg.norm(Q)) if typ == "continuous" else \
            (2 * np.linalg.norm(A) ** 2 * np.linalg.norm(X) + np.linalg.norm(X) + np.linalg.norm(Q))
        res_abs = float(np.linalg.norm(res))
        res_rel = res_abs / denom if denom else 0.0
        if not stable:
            warnings.append({"code": "W.DOMAIN.UNSTABLE_SYSTEM", "message": "A 不稳定：X 存在，但不是 Gramian（不保证半正定）", "path": "/A"})
        xs = bool(np.linalg.norm(X - X.T) <= 1e-10 * max(1.0, np.linalg.norm(X)))
        try:
            psd = bool(np.all(np.linalg.eigvalsh((X + X.T) / 2) >= -1e-10 * max(1.0, np.linalg.norm(X))))
        except np.linalg.LinAlgError:
            psd = False
        err_abs = res_abs / sep if sep > 0 else float("inf")
        trust = {
            "convergence": {"applicable": False},
            "residual": {"abs_fro": res_abs, "rel": res_rel,
                         "X_asymmetry_before_symmetrization_rel": x_asym_before if q_sym else None,
                         "definition": "rel = ||residual||_F / (2||A||_F ||X||_F + ||Q||_F) (continuous)"},
            "condition": {"separation_min": sep,
                          "definition": "min_{i,j}|lambda_i + conj(lambda_j)| (continuous) / |lambda_i conj(lambda_j) - 1| (discrete)"},
            "error_estimate": {"X_abs_fro": err_abs, "X_rel": err_abs / max(np.linalg.norm(X), np.finfo(float).tiny),
                               "basis": "heuristic: residual / sep_min; not a rigorous bound"},
            "checks": {"A_stable": stable, "X_symmetric": xs, "X_psd": psd},
            "accuracy_grade": "C", "credentials": None,
        }
        return {"X": C.dense_f64(X)}, trust, warnings
