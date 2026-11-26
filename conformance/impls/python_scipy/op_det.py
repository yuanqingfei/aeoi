"""linalg.matrix.det (profile 10.36 DT-1 .. DT-6), written from the profile.

LU with partial pivoting: the same code as linalg.system.solve in this implementation (op_linsolve._lu,
first row of maximal |a_ik|, as dgetf2). det = sign(P)·prod u_ii accumulated as mantissa in [0.5, 1)
and binary exponent (math.frexp: exact split, one rounding per factor). Error estimate DT-4 with the
Hager-Higham estimator of op_linsolve (the LS-5 code) and ||(1^T|L|)|U|||_1 in O(n^2).
"""
import math

import numpy as np

import aeoi_common as C
import op_linsolve as LS
from aeoi_common import Fail

OP_ID = "linalg.matrix.det"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:det:lu-pp+frexp-product"
MAX_DIM = 1000
LN2 = math.log(2.0)


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.RESOURCE.INPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Determinant with error estimate (Python second implementation)",
        description="实方阵的行列式（= MATLAB det）：部分选主元 LU（与 linalg.system.solve 同一段代码），乘积按尾数与二进制指数分开累乘，"
                    "不会中途溢出；同时返回 ln|det|、符号与相对误差估计。估计 ≥ 1（或主元恰为 0）时 det 与 0 不可区分，"
                    "发 W.NUMERICAL.DET_NOT_SIGNIFICANT，det、sign、log_abs_det 不可依赖。|det| 超出 f64 正规数范围时发 W.NUMERICAL.DET_OUT_OF_RANGE。",
        category="analysis", tags=["linalg", "determinant", "det", "slogdet", "lu"],
        applicability={"preconditions": ["A 为实数方阵 [n, n]，n ≥ 1", "全部元素有限", f"n ≤ {MAX_DIM}"],
                       "not_for": ["判断矩阵是否奇异或接近奇异（det 的大小与条件数无关；用 linalg.system.solve 或 linalg.matrix.inv 的 rcond）",
                                   "复数矩阵（0.4.0 不支持）"], "siso_only": False},
        provenance={"algorithm": "部分选主元 LU（每列取 |a_ik| 最大的第一行，秩 1 更新逐元素），主元恰为 0 即停（det = 0、sign = 0、"
                                 "log_abs_det = −Inf、误差估计 Inf）。乘积：(m, e) 从 (0.5, 1) 起，每步 frexp(|u_kk|) 精确拆分，"
                                 "尾数相乘一次舍入后再 frexp；sign 为置换奇偶与各主元符号之积；log_abs_det = ln m + e·ln 2（Python math.log，系统 libm）。"
                                 "误差估计 ε = n·u·est(‖A⁻¹‖₁)·‖|L||U|‖₁ + n·u，est 为 Hager–Higham 估计（固定起始向量，至多 5 次迭代 + 交错向量，"
                                 "与 linalg.system.solve 同一段代码），‖|L||U|‖₁ = max_j ((1ᵀ|L|)|U|)_j。ε ≥ 1 时 zero_within_error = true。",
                    "references": [
                        {"type": "textbook", "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, ch. 9, 14, 15",
                         "doi": "10.1137/1.9780898718027"},
                        {"type": "paper", "cite": "W. W. Hager, Condition estimates, SIAM J. Sci. Stat. Comput. 5(2), 1984", "doi": "10.1137/0905023"},
                        {"type": "textbook", "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., Johns Hopkins 2013, §3.2–3.4"}]},
        backend={"library": "aeoi-pyscipy op_linsolve LU (Python + numpy element-wise; no LAPACK)",
                 "routine": "op_linsolve._lu + op_det.Det.execute",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T2",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_dimension": MAX_DIM, "condition_estimator_iterations": 5},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=["W.NUMERICAL.DET_NOT_SIGNIFICANT", "W.NUMERICAL.DET_OUT_OF_RANGE"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "square", "statement": "A 为非空方阵", "enforced_by": "error:E.VALIDATION.SHAPE"},
                     {"id": "significant", "statement": "相对误差估计 < 1（det 与 0 可区分）", "enforced_by": "warning:W.NUMERICAL.DET_NOT_SIGNIFICANT"},
                     {"id": "estimate_heuristic", "statement": "误差估计是一阶启发式界，‖A⁻¹‖₁ 用的是估计（下界）", "enforced_by": "caller"},
                     {"id": "in_range", "statement": "|det| 在 f64 正规数范围内时 det 本身有意义；否则看 log_abs_det 与 sign",
                      "enforced_by": "warning:W.NUMERICAL.DET_OUT_OF_RANGE"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["significant"],
                 "coverage": {"significant": "full"},
                 "guards": {"significant": ["/det", "/sign", "/log_abs_det"]}},
    )


def _perm_parity(perm):
    """+1 / -1 for an even / odd permutation (cycle decomposition)."""
    n = len(perm)
    seen = [False] * n
    sgn = 1
    for i in range(n):
        if seen[i]:
            continue
        j, length = i, 0
        while not seen[j]:
            seen[j] = True
            j = int(perm[j])
            length += 1
        if length % 2 == 0:
            sgn = -sgn
    return sgn


class Det(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        A = C.read_array(args["A"], "/A", allowed=("f64", "i64"), max_elements=MAX_DIM * MAX_DIM)
        if len(A.shape) != 2 or A.shape[0] != A.shape[1] or A.shape[0] == 0:
            raise Fail("E.VALIDATION.SHAPE", f"A 的 shape 是 {A.shape}；linalg.matrix.det 需要非空方阵 [n, n]（DT-1）", "/A/shape",
                       {"shape": A.shape}, [C.ask_user("核对 A 的 shape（row-major，[n, n]）")],
                       agent_hint="非方阵没有行列式；要的可能是伪逆（linalg.matrix.pinv）或最小二乘解（linalg.system.solve）")
        n = A.shape[0]
        if n > MAX_DIM:
            raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"n = {n} 超过上限 {MAX_DIM}", "/A/shape", {"limit": MAX_DIM},
                       [C.ask_user("缩小问题规模")])
        return dict(A=A.values.astype(float), n=n)

    def estimate(self, P):
        n = P["n"]
        return {"n": n, "flops_estimate": int(2 * n ** 3 // 3 + 20 * n * n)}

    def execute(self, P):
        A, n = P["A"], P["n"]
        nu = n * C.U
        zero_pivot = None
        try:
            LU, perm = LS._lu(A)
        except Fail as f:
            if f.code != "E.NUMERICAL.SINGULAR":
                raise
            zero_pivot = f.details.get("step")
        warnings = []
        cond = {"estimator": "Hager–Higham 1-范数估计（下界）"}
        if zero_pivot is not None:
            det, sign, logdet, eps_est = 0.0, 0, -math.inf, math.inf
            cond["zero_pivot_step"] = zero_pivot
        else:
            d = np.diag(LU)
            sign = _perm_parity(perm)
            mant, ex = 0.5, 1
            for u in d:
                if u < 0:
                    sign = -sign
                mk, ek = math.frexp(abs(float(u)))
                mant, e2 = math.frexp(mant * mk)
                ex += ek + e2
            logdet = math.log(mant) + ex * LN2
            if ex > 1024:
                mag = math.inf
            else:
                mag = math.ldexp(mant, ex)       # may underflow to a subnormal or 0
            det = sign * mag
            out_of_range = not (-1021 <= ex <= 1024)   # |det| = mant·2^ex, mant ∈ [0.5, 1): normal iff 2^-1022 <= |det|
            if out_of_range:
                warnings.append({"code": "W.NUMERICAL.DET_OUT_OF_RANGE",
                                 "message": f"|det| = e^{logdet:.6g} 超出 f64 正规数范围：det 为 {C.enc_float(det)}；"
                                            "log_abs_det 与 sign 仍然有效（DT-3）",
                                 "path": "/A", "details": {"log_abs_det": C.enc_float(logdet), "sign": sign, "det": C.enc_float(det)}})
            inv_est = LS._inv_norm1_est(lambda v: LS._lu_solve(LU, perm, v), lambda v: LS._lu_solve_t(LU, perm, v), n)
            L = np.tril(LU, -1) + np.eye(n)
            Uu = np.triu(LU)
            colL = np.sum(np.abs(L), axis=0)
            lu1 = float(np.max(colL @ np.abs(Uu)))
            eps_est = nu * inv_est * lu1 + nu
            cond.update({"norm1_inv_A_estimate": inv_est, "norm1_abs_L_abs_U": lu1})
        zwe = bool(eps_est >= 1.0)
        if zwe:
            warnings.append({"code": "W.NUMERICAL.DET_NOT_SIGNIFICANT",
                             "message": (f"det 的相对误差估计为 {C.enc_float(eps_est)} ≥ 1" if zero_pivot is None else
                                         f"第 {zero_pivot} 步消元的主元恰为 0") +
                                        f"：算出的 det = {det:.4g} 与 0 不可区分；det、sign、log_abs_det 不可依赖（DT-4）",
                             "path": "/A", "details": {"relative_error_estimate": C.enc_float(eps_est), "det": C.enc_float(det),
                                                       "zero_pivot": zero_pivot is not None}})
        result = {"det": C.enc_float(det), "sign": int(sign), "log_abs_det": C.enc_float(logdet),
                  "relative_error_estimate": C.enc_float(eps_est),
                  "zero_within_error": zwe, "method": "lu_partial_pivoting"}
        trust = {
            "convergence": {"applicable": False},
            "residual": {"applicable": False},
            "condition": cond,
            "error_estimate": {"det_rel": C.enc_float(eps_est),
                               "basis": "启发式一阶界 n·u·est(‖A⁻¹‖₁)·‖|L||U|‖₁ + n·u（DT-4）；est 为下界估计，不是严格上界"},
            "checks": {"significant": not zwe},
            "accuracy_grade": "C", "credentials": None,
        }
        return result, trust, warnings
