"""linalg.subspace.rank (profile 10.37 RK-1 .. RK-5), written from the profile.

Singular values: aeoi_kernels.jacobi_svd (own one-sided Hestenes Jacobi, 10.23 SV-2), the same kernel as
linalg.matrix.pinv in this implementation. The rank decision (RK-2, RK-3) is `rank_decision` below and is
shared with op_pinv, so both operators give the same rank for the same A.
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
from aeoi_common import Fail

OP_ID = "linalg.subspace.rank"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:rank:hestenes-jacobi-svd"
MAX_DIM = 500

SVD_ALGORITHM = ("单边 Jacobi（Hestenes）SVD，m < n 时作用在 Aᵀ 上；|g_pᵀg_q| > sqrt(行数)·ε·‖g_p‖‖g_q‖ 时旋转，至多 60 轮；"
                 "σ_k = ‖g_k‖₂（缩放后 fsum），降序（相等保持列序）；σ_k ≤ max(m, n)·ε·σ_max 的 u_k 由已得 u 的 Householder QR 正交补全给出")
RANK_ALGORITHM = ("tol 缺省 max(m, n)·eps(σ_max)，eps(x) = x 到下一个更大浮点数的距离（eps(0) = 2⁻¹⁰⁷⁴）；秩 r = #{σ_k > tol}；"
                  "有 σ_k ∈ (tol/10, 100·tol] 时秩不明确（W.NUMERICAL.RANK_AMBIGUOUS，rank_well_separated 不成立）")
SVD_REFS = [
    {"type": "paper", "cite": "M. R. Hestenes, Inversion of matrices by biorthogonalization and related results, J. SIAM 6(1), 1958",
     "doi": "10.1137/0106005"},
    {"type": "paper", "cite": "J. Demmel, K. Veselić, Jacobi's method is more accurate than QR, SIAM J. Matrix Anal. Appl. 13(4), 1992",
     "doi": "10.1137/0613074"},
    {"type": "textbook", "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., Johns Hopkins 2013, §2.4, §5.5, §8.6"},
]


def read_matrix(args, max_dim=MAX_DIM):
    """RK-1: real [m, n], m, n >= 1. Order of checks: schema -> encoding/dtype -> non-finite (read_array) -> shape."""
    A = C.read_array(args["A"], "/A", allowed=("f64", "i64"), max_elements=max_dim * max_dim)
    if len(A.shape) != 2:
        raise Fail("E.VALIDATION.SHAPE", f"A 的 shape 是 {A.shape}；要求二维矩阵 [m, n]", "/A/shape", {"shape": A.shape},
                   [C.ask_user("行向量写 shape [1, n]，列向量写 [m, 1]（row-major）")],
                   agent_hint="行向量写 shape [1, n]，列向量写 [m, 1]")
    m, n = A.shape
    if m == 0 or n == 0:
        raise Fail("E.VALIDATION.SHAPE", f"A 是空矩阵（shape {A.shape}）", "/A/shape", {"shape": A.shape},
                   [C.ask_user("提供非空矩阵")])
    if m > max_dim or n > max_dim:
        raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"A 的维数超过上限 {max_dim}", "/A/shape", {"limit": max_dim},
                   [C.ask_user("缩小问题规模")])
    return A.values.astype(float)


def svd_checked(A):
    """Jacobi SVD plus the SV-5 checks. Returns dict with U, s, V, recon, orth, thr, ok, err_abs."""
    m, n = A.shape
    try:
        U, s, V, sweeps = K.jacobi_svd(A)
    except K.NoConvergence as e:
        raise Fail("E.NUMERICAL.NO_CONVERGENCE", f"单边 Jacobi SVD 在 60 轮内没有收敛：{e}", "/A",
                   {"max_sweeps": 60}, [C.ask_user("检查 A 是否有极端的元素尺度")], retryable=False)
    nA = K.norm_fro(A)
    res = K.norm_fro(A - (U * s) @ V.T)
    recon = res / nA if nA > 0 else res          # zero A: absolute residual (0 when U·0·Vᵀ = 0)
    p = s.size
    orth = max(float(np.max(np.abs(U.T @ U - np.eye(p)))), float(np.max(np.abs(V.T @ V - np.eye(p)))))
    thr = 100 * max(m, n) * C.U
    smax = float(s[0])
    err_abs = nA * recon + 2 * smax * orth
    return dict(U=U, s=s, V=V, recon=recon, orth=orth, thr=thr, ok=bool(recon <= thr and orth <= thr),
                err_abs=err_abs, sweeps=sweeps, normA=nA)


def rank_decision(s, m, n, tol_supplied):
    """RK-2 / RK-3. Returns dict(rank, tol, tol_source, smallest_kept, largest_dropped, ambiguous, in_band)."""
    smax = float(s[0])
    if tol_supplied is None:
        tol = max(m, n) * K.spacing(smax)
        src = "default"
    else:
        tol = float(tol_supplied)
        src = "supplied"
    r = int(np.sum(s > tol))
    kept = float(s[r - 1]) if r > 0 else None
    dropped = float(s[r]) if r < s.size else None
    lo, hi = tol / 10.0, 100.0 * tol
    in_band = [float(x) for x in s if lo < x <= hi]
    return dict(rank=r, tol=tol, tol_source=src, smallest_kept=kept, largest_dropped=dropped,
                ambiguous=bool(in_band), in_band=in_band)


def ambiguous_warning(dec, what):
    return {"code": "W.NUMERICAL.RANK_AMBIGUOUS",
            "message": (f"有奇异值落在阈值 tol = {dec['tol']:.3e} 附近的 (tol/10, 100·tol] 内：数值秩 {dec['rank']} "
                        f"取决于舍入与阈值的选择，{what}不可依赖。数据本身有噪声时，应按噪声水平给出 tol。"),
            "path": "/A",
            "details": {"tol": dec["tol"], "rank": dec["rank"], "smallest_kept": dec["smallest_kept"],
                        "largest_dropped": dec["largest_dropped"], "singular_values_in_band": dec["in_band"]}}


SVD_ERROR_BASIS = "|Δσ_k| ≲ ‖A‖_F·重构残差 + 2σ_max·正交性误差（Weyl；小奇异值只有绝对精度）；非严格上界"
SVD_RESIDUAL_DEF = "‖A − U·diag(s)·Vᵀ‖_F/‖A‖_F（A = 0 时为绝对残差）与 max(‖UᵀU − I‖_max, ‖VᵀV − I‖_max)，阈值 100·max(m, n)·u（SV-5）"


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.NUMERICAL.NO_CONVERGENCE", "E.RESOURCE.INPUT_TOO_LARGE",
              "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Numerical rank (Python second implementation)",
        description="实矩阵的数值秩（= MATLAB rank）：奇异值大于阈值 tol 的个数。tol 缺省为 max(m, n)·eps(σ_max)，"
                    "也可由调用方按数据的噪声水平给出。返回全部奇异值与阈值两侧最近的奇异值；有奇异值落在 (tol/10, 100·tol] 内时"
                    "秩不明确，发 W.NUMERICAL.RANK_AMBIGUOUS，rank 标为不可依赖。",
        category="analysis", tags=["linalg", "rank", "svd", "subspace"],
        applicability={"preconditions": ["A 为实数 [m, n]，m, n ≥ 1", "全部元素有限", "max(m, n) ≤ 500"],
                       "not_for": ["有理数或符号矩阵的精确秩", "复数矩阵（0.4.0 不支持）", "大型稀疏矩阵"], "siso_only": False},
        provenance={"algorithm": SVD_ALGORITHM + "。" + RANK_ALGORITHM, "references": SVD_REFS},
        backend={"library": "aeoi-pyscipy aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "aeoi_kernels.jacobi_svd + op_rank.rank_decision",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_dimension": MAX_DIM, "max_jacobi_sweeps": 60},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=["W.NUMERICAL.RANK_AMBIGUOUS"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "threshold_is_modelling", "statement": "秩取决于 tol；缺省 tol 只区分舍入噪声，数据本身的噪声水平由调用方给出",
                      "enforced_by": "caller"},
                     {"id": "separated", "statement": "阈值附近 (tol/10, 100·tol] 没有奇异值", "enforced_by": "warning:W.NUMERICAL.RANK_AMBIGUOUS"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["svd_reconstructs", "rank_well_separated"],
                 "coverage": {"svd_reconstructs": "full", "rank_well_separated": "full"},
                 "guards": {"svd_reconstructs": ["/rank", "/s", "/smallest_kept", "/largest_dropped"],
                            "rank_well_separated": ["/rank", "/smallest_kept", "/largest_dropped"]}},
    )


class Rank(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        A = read_matrix(args)
        return dict(A=A, tol=args.get("tol"))

    def estimate(self, P):
        m, n = P["A"].shape
        return {"m": m, "n": n, "flops_estimate": int(10 * max(m, n) * min(m, n) ** 2)}

    def execute(self, P):
        A = P["A"]
        m, n = A.shape
        sv = svd_checked(A)
        dec = rank_decision(sv["s"], m, n, P["tol"])
        result = {"rank": dec["rank"], "tol": dec["tol"], "tol_source": dec["tol_source"], "s": C.dense_f64(sv["s"]),
                  "smallest_kept": dec["smallest_kept"], "largest_dropped": dec["largest_dropped"]}
        warnings = [ambiguous_warning(dec, "rank ")] if dec["ambiguous"] else []
        trust = {
            "convergence": {"applicable": True, "converged": True, "iterations": sv["sweeps"]},
            "residual": {"svd_reconstruction": sv["recon"], "svd_orthonormality": sv["orth"], "threshold": sv["thr"],
                         "definition": SVD_RESIDUAL_DEF},
            "error_estimate": {"singular_value_abs": sv["err_abs"], "basis": SVD_ERROR_BASIS},
            "checks": {"svd_reconstructs": sv["ok"], "rank_well_separated": not dec["ambiguous"]},
            "accuracy_grade": "C", "credentials": None,
            "derived": {"ambiguity_band": [dec["tol"] / 10.0, 100.0 * dec["tol"]]},
        }
        return result, trust, warnings
