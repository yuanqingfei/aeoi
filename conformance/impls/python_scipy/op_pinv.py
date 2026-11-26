"""linalg.matrix.pinv (profile 10.39 PI-1 .. PI-5), written from the profile.

Same SVD kernel and the same rank decision as op_rank (PI-1 = RK-1 .. RK-3).
X = sum_{k <= r} v_k u_k^T / sigma_k (PI-2); Penrose checks as PI-4.
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
import op_rank as RK

OP_ID = "linalg.matrix.pinv"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:pinv:hestenes-jacobi-svd"
MAX_DIM = RK.MAX_DIM


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.NUMERICAL.NO_CONVERGENCE", "E.RESOURCE.INPUT_TOO_LARGE",
              "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Moore-Penrose pseudoinverse (Python second implementation)",
        description="Moore–Penrose 伪逆（= MATLAB pinv）：X = Σ_{k ≤ r} v_k·u_kᵀ/σ_k，[n, m]；r 为数值秩，阈值与 linalg.subspace.rank 相同"
                    "（缺省 max(m, n)·eps(σ_max)，可由调用方给出）。伪逆在秩的阈值处不连续：秩不明确时 X 与 rank 标为不可依赖。"
                    "返回 cond_r = σ_1/σ_r。",
        category="analysis", tags=["linalg", "pinv", "pseudoinverse", "svd"],
        applicability={"preconditions": ["A 为实数 [m, n]，m, n ≥ 1", "全部元素有限", "max(m, n) ≤ 500"],
                       "not_for": ["解最小二乘问题（直接用 linalg.system.solve，不要先求伪逆再相乘）",
                                   "满秩方阵求逆（用 linalg.matrix.inv）", "复数矩阵（0.4.0 不支持）"], "siso_only": False},
        provenance={"algorithm": RK.SVD_ALGORITHM + "。" + RK.RANK_ALGORITHM +
                                 "。X = V_r·diag(1/σ_1 … 1/σ_r)·U_rᵀ（逐元素），r = 0 时 X = 0、cond_r = null。"
                                 "Penrose 检查：‖AXA − A‖_F/‖A‖_F ≤ ‖σ_{r+1..}‖₂/‖A‖_F + 100·max(m, n)·u·max(1, cond_r)，"
                                 "‖XAX − X‖_F/‖X‖_F ≤ 100·max(m, n)·u·max(1, cond_r)；分母为 0 的一项按空真",
                    "references": RK.SVD_REFS + [
                        {"type": "paper", "cite": "R. Penrose, A generalized inverse for matrices, Proc. Cambridge Philos. Soc. 51, 1955",
                         "doi": "10.1017/S0305004100030401"}]},
        backend={"library": "aeoi-pyscipy aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "aeoi_kernels.jacobi_svd + op_rank.rank_decision + op_pinv.Pinv.execute",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T2",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_dimension": MAX_DIM, "max_jacobi_sweeps": 60},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=["W.NUMERICAL.RANK_AMBIGUOUS"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "threshold_is_modelling", "statement": "伪逆取决于 tol；缺省 tol 只区分舍入噪声，数据本身的噪声水平由调用方给出",
                      "enforced_by": "caller"},
                     {"id": "separated", "statement": "阈值附近 (tol/10, 100·tol] 没有奇异值", "enforced_by": "warning:W.NUMERICAL.RANK_AMBIGUOUS"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["svd_reconstructs", "rank_well_separated", "penrose"],
                 "coverage": {"svd_reconstructs": "full", "rank_well_separated": "full", "penrose": "full"},
                 "guards": {"svd_reconstructs": ["/X", "/rank", "/cond_r"], "rank_well_separated": ["/X", "/rank"],
                            "penrose": ["/X"]}},
    )


class Pinv(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        return dict(A=RK.read_matrix(args, MAX_DIM), tol=args.get("tol"))

    def estimate(self, P):
        m, n = P["A"].shape
        return {"m": m, "n": n, "flops_estimate": int(10 * max(m, n) * min(m, n) ** 2 + 2 * m * n * min(m, n))}

    def execute(self, P):
        A = P["A"]
        m, n = A.shape
        sv = RK.svd_checked(A)
        s, U, V = sv["s"], sv["U"], sv["V"]
        dec = RK.rank_decision(s, m, n, P["tol"])
        r = dec["rank"]
        if r > 0:
            X = np.zeros((n, m))
            for k in range(r):                      # element-wise rank-1 sums, no BLAS on the result path
                X += np.outer(V[:, k] / s[k], U[:, k])
            cond_r = float(s[0] / s[r - 1])
        else:
            X = np.zeros((n, m))
            cond_r = None
        # PI-4
        fac = 100 * max(m, n) * C.U * max(1.0, cond_r if cond_r is not None else 1.0)
        nA = sv["normA"]
        nX = K.norm_fro(X)
        dropped = K.norm_fro(s[r:]) if r < s.size else 0.0
        axa = K.norm_fro(A @ X @ A - A)
        xax = K.norm_fro(X @ A @ X - X)
        axa_rel = axa / nA if nA > 0 else axa
        xax_rel = xax / nX if nX > 0 else xax
        axa_thr = (dropped / nA if nA > 0 else 0.0) + fac
        penrose = bool(axa_rel <= axa_thr and xax_rel <= fac)
        result = {"X": C.dense_f64(X), "rank": r, "tol": dec["tol"], "tol_source": dec["tol_source"], "cond_r": cond_r,
                  "smallest_kept": dec["smallest_kept"], "largest_dropped": dec["largest_dropped"]}
        warnings = [RK.ambiguous_warning(dec, "rank 与伪逆 X ")] if dec["ambiguous"] else []
        trust = {
            "convergence": {"applicable": True, "converged": True, "iterations": sv["sweeps"]},
            "residual": {"svd_reconstruction": sv["recon"], "svd_orthonormality": sv["orth"], "threshold": sv["thr"],
                         "definition": RK.SVD_RESIDUAL_DEF,
                         "penrose_axa": axa_rel, "penrose_xax": xax_rel, "penrose_axa_threshold": axa_thr,
                         "penrose_xax_threshold": fac,
                         "penrose_definition": "‖A·X·A − A‖_F/‖A‖_F 与 ‖X·A·X − X‖_F/‖X‖_F（分母为 0 时取绝对值）；"
                                               "阈值 100·max(m, n)·u·max(1, cond_r)，前者另加 ‖σ_{r+1..}‖₂/‖A‖_F（PI-4）"},
            "error_estimate": {"singular_value_abs": sv["err_abs"], "basis": RK.SVD_ERROR_BASIS},
            "checks": {"svd_reconstructs": sv["ok"], "rank_well_separated": not dec["ambiguous"], "penrose": penrose},
            "accuracy_grade": "C", "credentials": None,
        }
        return result, trust, warnings
