"""linalg.decomp.chol (profile 10.44 CH-1 .. CH-5), written from the profile.

Symmetry check (CH-2) on the whole matrix, then column Cholesky reading only the upper triangle
(aeoi_kernels.cholesky_upper, correctly rounded dot products), d_j <= 0 -> NOT_POSITIVE_DEFINITE (CH-3).
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
from aeoi_common import Fail

OP_ID = "linalg.decomp.chol"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:chol:column-exactdot"
MAX_DIM = 1000


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.VALIDATION.NOT_SYMMETRIC", "E.DOMAIN.NOT_POSITIVE_DEFINITE",
              "E.RESOURCE.INPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Cholesky factorization (Python second implementation)",
        description="对称正定矩阵的 Cholesky 分解（= MATLAB chol）：缺省返回上三角 R，RᵀR = A；triangle = lower 时返回 L，L·Lᵀ = A。"
                    "只用 A 的上三角，但先检查对称：max|a_ij − a_ji| > 10·n·u·max|a_ij| 时报 E.VALIDATION.NOT_SYMMETRIC。"
                    "不正定时报 E.DOMAIN.NOT_POSITIVE_DEFINITE 并指出失败的列。",
        category="analysis", tags=["linalg", "cholesky", "chol", "decomposition", "positive-definite"],
        applicability={"preconditions": ["A 为实数 [n, n]，n ≥ 1", "全部元素有限", "A 对称（舍入以内）", "A 正定"],
                       "not_for": ["判断正定性的可靠手段（接近奇异的正定矩阵可能因舍入失败；用特征分解）",
                                   "半正定或不定矩阵（用 LDLᵀ 或特征分解）", "复数矩阵（0.4.0 不支持）"], "siso_only": False},
        provenance={"algorithm": "先检查对称：δ = max|a_ij − a_ji|（i < j，行主序第一个最大者），δ > 10·n·u·max|a_ij| 报错，"
                                 "补丁为 (A + Aᵀ)/2。按列 Cholesky，只读上三角：d_j = a_jj − Σ_{k<j} r_kj²，r_jj = √d_j，"
                                 "r_ji = (a_ji − Σ_{k<j} r_kj r_ki)/r_jj（i > j），内积全部正确舍入（无误差乘积 + fsum）；d_j ≤ 0 报错。"
                                 "lower 返回 Rᵀ。重构残差 ‖S − RᵀR‖_F/‖S‖_F 用正确舍入内积，S 由上三角构成。",
                    "references": [
                        {"type": "textbook", "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., Johns Hopkins 2013, §4.2"},
                        {"type": "textbook", "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, ch. 10",
                         "doi": "10.1137/1.9780898718027"}]},
        backend={"library": "aeoi-pyscipy aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "aeoi_kernels.cholesky_upper",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_dimension": MAX_DIM},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=[],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "symmetric", "statement": "A 对称（舍入容差 10·n·u·max|a_ij| 以内）", "enforced_by": "error:E.VALIDATION.NOT_SYMMETRIC"},
                     {"id": "positive_definite", "statement": "A 正定（数值意义：每列 d_j > 0）", "enforced_by": "error:E.DOMAIN.NOT_POSITIVE_DEFINITE"},
                     {"id": "upper_used", "statement": "只用上三角；阈值以内的不对称被忽略（asymmetry 回显其大小）", "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["reconstructs", "all_finite"],
                 "coverage": {"reconstructs": "full", "all_finite": "full"},
                 "guards": {"reconstructs": ["/R"], "all_finite": ["/R"]}},
    )


def _fmt_data(M):
    return [C.enc_float(x) for x in np.asarray(M).ravel(order="C")]


class Chol(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        A = C.read_array(args["A"], "/A", allowed=("f64", "i64"), max_elements=MAX_DIM * MAX_DIM)
        if len(A.shape) != 2 or A.shape[0] != A.shape[1] or A.shape[0] == 0:
            raise Fail("E.VALIDATION.SHAPE", f"A 的 shape 是 {A.shape}；linalg.decomp.chol 需要非空方阵 [n, n]", "/A/shape",
                       {"shape": A.shape}, [C.ask_user("Cholesky 只对对称正定方阵有定义；核对 A 的 shape（row-major）")])
        n = A.shape[0]
        if n > MAX_DIM:
            raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"n = {n} 超过上限 {MAX_DIM}", "/A/shape", {"limit": MAX_DIM},
                       [C.ask_user("缩小问题规模")])
        a = A.values.astype(float)
        amax = float(np.max(np.abs(a)))
        D = np.abs(a - a.T)
        iu = np.triu_indices(n, 1)
        if iu[0].size:
            vals = D[iu]
            idx = int(np.argmax(vals))           # first maximum in row-major order of the upper triangle
            dmax, at = float(vals[idx]), [int(iu[0][idx]), int(iu[1][idx])]
        else:
            dmax, at = 0.0, [0, 0]
        thr = 10 * n * C.U * amax
        if dmax > thr:
            sym = (a + a.T) / 2
            # inline f64 data: replace /A/data (as the example); b64 or i64 input (halves are not integers): replace /A
            path = "/A/data" if (isinstance(args["A"], dict) and "data" in args["A"] and A.dtype == "f64") else "/A"
            value = _fmt_data(sym) if path == "/A/data" else C.dense_f64(sym)
            raise Fail("E.VALIDATION.NOT_SYMMETRIC",
                       f"A 不对称：|a[{at[0]}][{at[1]}] − a[{at[1]}][{at[0]}]| = {dmax:.3e}，超过舍入容差 10·n·u·max|a| = {thr:.3e}（CH-2）",
                       "/A", {"max_asymmetry": dmax, "at": at, "threshold": thr},
                       [C.patch_remedy("symmetrize", "用 (A + Aᵀ)/2 代替 A（改变了数据，需要用户确认）",
                                       [{"op": "replace", "path": path, "value": value}], confidence="medium", confirm=True),
                        C.ask_user("A 本应对称吗？可能有一个元素录错（row-major）")],
                       agent_hint="不要自行对称化；先问用户是录入错误还是数据误差", retryable=True)
        asym = dmax / amax if amax > 0 else 0.0
        return dict(A=a, n=n, asymmetry=asym, triangle=args.get("triangle", "upper"))

    def estimate(self, P):
        n = P["n"]
        return {"n": n, "flops_estimate": int(n ** 3 // 3)}

    def execute(self, P):
        a, n = P["A"], P["n"]
        S = np.triu(a) + np.triu(a, 1).T
        R, fail = K.cholesky_upper(S)
        if fail is not None:
            j, d = fail
            raise Fail("E.DOMAIN.NOT_POSITIVE_DEFINITE",
                       f"A 不是正定矩阵：第 {j} 列（0 起始）的主元 d = {d:.3e} ≤ 0（前 {j + 1} 阶顺序主子式不正定，CH-3）。"
                       "半正定（奇异）矩阵也在这里失败。", "/A",
                       {"failed_column": j, "pivot": d},
                       [C.ask_user("A 应当正定吗？是数据问题，还是模型本身允许半正定/不定？", "not_pd")],
                       agent_hint="不要加对角扰动凑成正定；判断正定性用特征分解（linalg.decomp.eig）", retryable=False)
        nS = K.norm_fro(S)
        res = K.residual_fro_exact(S, R.T, R)
        recon = res / nS if nS > 0 else res
        thr = 100 * n * C.U
        out = R if P["triangle"] == "upper" else R.T
        finite = bool(np.all(np.isfinite(out)))
        result = {"R": C.dense_f64(out), "triangle": P["triangle"], "asymmetry": P["asymmetry"]}
        trust = {
            "convergence": {"applicable": False},
            "residual": {"reconstruction": recon, "threshold": thr,
                         "definition": "‖S − RᵀR‖_F/‖S‖_F，S 由 A 的上三角构成，正确舍入内积，阈值 100·n·u（CH-4）"},
            "error_estimate": {"basis": "Cholesky 无条件后向稳定；因子的前向误差 ≲ κ₂(A)·u（扰动理论），这里不给数值"},
            "checks": {"reconstructs": bool(recon <= thr), "all_finite": finite},
            "accuracy_grade": "C", "credentials": None,
        }
        return result, trust, []
