"""linalg.decomp.qr (profile 10.43 QR-1 .. QR-5), written from the profile.

aeoi_kernels.householder_qr (own Householder QR, dgeqr2 / dlarfg convention, any shape, no pivoting),
then the QR-3 sign fix (rows of R / columns of Q with R_ii < 0 negated). The same kernel gives the
orthogonal completion used by the Jacobi SVD (rank, pinv).
"""
import numpy as np

import aeoi_common as C
import aeoi_kernels as K
import op_rank as RK
from aeoi_common import Fail

OP_ID = "linalg.decomp.qr"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:qr:householder-dlarfg+signfix"
MAX_DIM = 2000
MAX_OUTPUT_ELEMENTS = 10 ** 6      # by analogy with L-6 (inline JSON arrays <= 10^6 elements); QR-2 gives no value


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.RESOURCE.INPUT_TOO_LARGE", "E.RESOURCE.OUTPUT_TOO_LARGE",
              "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="QR factorization (Python second implementation)",
        description="实矩阵的 QR 分解（= MATLAB qr）：A = Q·R，Householder 变换（dgeqr2/dlarfg 约定），任意形状，不选列主元。"
                    "缺省 mode = econ：Q 为 [m, k]、R 为 [k, n]，k = min(m, n)；mode = full：Q 为 [m, m]、R 为 [m, n]。"
                    "R 的对角元归一化为非负（前 k 列满秩时分解唯一）。秩亏时 R 不揭示秩。",
        category="analysis", tags=["linalg", "qr", "householder", "decomposition"],
        applicability={"preconditions": ["A 为实数 [m, n]，m, n ≥ 1", "全部元素有限",
                                         f"输出元素总数（Q 与 R）≤ {MAX_OUTPUT_ELEMENTS}"],
                       "not_for": ["求数值秩（用 linalg.subspace.rank）", "带列主元的秩揭示 QR", "最小二乘（用 linalg.system.solve）",
                                   "复数矩阵（0.4.0 不支持）"], "siso_only": False},
        provenance={"algorithm": "Householder QR：第 j 步对 R[j:, j] 做 dlarfg（β = −sign(α)·‖x‖，τ = (β − α)/β，v = x/(α − β)，"
                                 "x 的尾部为 0 时 τ = 0、H = I），k = min(m, n) 个反射；Q = H_0…H_{k−1}·[I; 0]（full 时为 I_m）。"
                                 "然后对每个 i < k、R_ii < 0，R 的第 i 行与 Q 的第 i 列同时变号（精确）；R_ii = 0 不变号。"
                                 "full 的 Q 补全列与 R 的 0 行不变号。重构残差用正确舍入内积。",
                    "references": [
                        {"type": "textbook", "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., Johns Hopkins 2013, §5.1–5.2"},
                        {"type": "textbook", "cite": "E. Anderson et al., LAPACK Users' Guide, 3rd ed., SIAM 1999（dgeqr2、dlarfg、dorg2r）",
                         "doi": "10.1137/1.9780898719604"},
                        {"type": "paper", "cite": "A. S. Householder, Unitary triangularization of a nonsymmetric matrix, J. ACM 5(4), 1958",
                         "doi": "10.1145/320941.320947"}]},
        backend={"library": "aeoi-pyscipy aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "aeoi_kernels.householder_qr + aeoi_kernels.qr_sign_fix",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_dimension": MAX_DIM, "max_output_elements": MAX_OUTPUT_ELEMENTS},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=[],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "unique", "statement": "A 的前 k 列满秩时 Q、R 唯一；秩亏时 Q 的部分列与 R 不唯一，full 的补全列也不唯一",
                      "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["reconstructs", "orthonormal", "all_finite"],
                 "coverage": {"reconstructs": "full", "orthonormal": "full", "all_finite": "full"},
                 "guards": {"reconstructs": ["/Q", "/R"], "orthonormal": ["/Q"], "all_finite": ["/Q", "/R"]}},
    )


class QR(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        A = RK.read_matrix(args, MAX_DIM)
        mode = args.get("mode", "econ")
        m, n = A.shape
        k = min(m, n)
        out = (m * m + m * n) if mode == "full" else (m * k + k * n)
        if out > MAX_OUTPUT_ELEMENTS:
            rem = []
            econ_out = m * k + k * n
            if mode == "full" and econ_out <= MAX_OUTPUT_ELEMENTS:
                rem.append(C.patch_remedy("drop_mode", "删除 mode，改用缺省的经济型（Q 为 [m, min(m, n)]）",
                                          [{"op": "remove", "path": "/mode"}], confidence="high", confirm=True))
            rem.append(C.ask_user("真的需要完整的 Q 吗？经济型通常就够了"))
            raise Fail("E.RESOURCE.OUTPUT_TOO_LARGE",
                       f"mode = {mode} 的输出有 {out} 个元素，超过上限 {MAX_OUTPUT_ELEMENTS}（QR-2）", "/mode",
                       {"output_elements": out, "limit": MAX_OUTPUT_ELEMENTS, "mode": mode}, rem,
                       agent_hint="高瘦矩阵用经济型（删除 mode）", retryable=True)
        return dict(A=A, mode=mode)

    def estimate(self, P):
        m, n = P["A"].shape
        k = min(m, n)
        return {"m": m, "n": n, "mode": P["mode"], "flops_estimate": int(2 * m * n * k + 4 * m * m * k)}

    def execute(self, P):
        A, mode = P["A"], P["mode"]
        m, n = A.shape
        Q, R = K.householder_qr(A, full=(mode == "full"))
        Q, R = K.qr_sign_fix(Q, R)
        nA = K.norm_fro(A)
        res = K.residual_fro_exact(A, Q, R)
        recon = res / nA if nA > 0 else res
        orth = float(np.max(np.abs(K.matmul_exact(Q.T, Q) - np.eye(Q.shape[1]))))
        thr = 100 * max(m, n) * C.U
        finite = bool(np.all(np.isfinite(Q)) and np.all(np.isfinite(R)))
        result = {"Q": C.dense_f64(Q), "R": C.dense_f64(R), "mode": mode, "sign_convention": "r_diag_nonnegative"}
        trust = {
            "convergence": {"applicable": False},
            "residual": {"reconstruction": recon, "orthonormality": orth, "threshold": thr,
                         "definition": "‖A − Q·R‖_F/‖A‖_F（正确舍入内积；A = 0 时为绝对残差）与 ‖QᵀQ − I‖_max，阈值 100·max(m, n)·u（QR-4）"},
            "error_estimate": {"basis": "重构残差是后向误差；因子本身的前向误差 ≲ κ₂(A)·u（扰动理论），这里不给数值"},
            "checks": {"reconstructs": bool(recon <= thr), "orthonormal": bool(orth <= thr), "all_finite": finite},
            "accuracy_grade": "C", "credentials": None,
        }
        return result, trust, []
