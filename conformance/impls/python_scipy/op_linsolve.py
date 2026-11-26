"""linalg.system.solve (profile 10.19 LS-1 .. LS-7), written from the profile text.

Square systems: LU with partial pivoting (first row of maximal |a_ik|, as dgetf2), rank-1 updates as
separate elementwise multiplies and subtracts (no FMA, no BLAS); triangular solves and residuals use
correctly rounded dot products (error-free products + math.fsum), so the result does not depend on
BLAS kernels or thread counts. Fixed-precision iterative refinement as LS-2.
Overdetermined systems: Householder QR in the dgeqr2/dlarfg convention, no refinement (LS-3).
Condition: Hager-Higham 1-norm estimator (LS-5).
"""
import math

import numpy as np

import aeoi_common as C
from aeoi_common import Fail

OP_ID = "linalg.system.solve"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:linsolve:lu-pp+exactdot-refine.householder-qr"
MAX_DIM = 2000
MAX_RHS = 1000


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN",
              "E.VALIDATION.METHOD_UNAVAILABLE", "E.DOMAIN.UNDERDETERMINED", "E.NUMERICAL.SINGULAR",
              "E.RESOURCE.INPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Linear system A x = b (Python second implementation)",
        description="解实数稠密线性方程组 A·x = b（= MATLAB A\\b）：方阵用部分选主元 LU（缺省再做至多 3 步迭代精化），"
                    "超定（m > n）用 Householder QR 求最小二乘解；欠定报 E.DOMAIN.UNDERDETERMINED。返回后向误差、1-范数条件数估计与前向误差估计。",
        category="analysis", tags=["linalg", "solve"],
        applicability={"preconditions": ["A 为实数 [m, n]，m ≥ n；b 为 [m] 或 [m, k]", "A、b 全部元素有限",
                                         "方阵 A 非奇异；超定时 A 列满秩"],
                       "not_for": ["欠定方程组（m < n）", "复数矩阵", "稀疏矩阵（先稠密化）"], "siso_only": False},
        provenance={"algorithm": "方阵：部分选主元 LU（每列取 |a_ik| 最大的第一行），秩 1 更新用逐元素乘减；三角回代与残差用无误差乘积 + math.fsum 的正确舍入内积；"
                                 "refinement = auto 时至多 3 步固定精度迭代精化（‖δ‖∞ ≤ 2u‖x‖∞ 或修正量没有减半即停）。"
                                 "超定：Householder QR（dlarfg 约定 β = −sign(α)·‖x‖），x = R⁻¹(Qᵀb) 前 n 行，不精化；"
                                 "|r_jj| ≤ max(m,n)·u·max|r_ii| 判为秩亏。条件数：Hager–Higham 1-范数估计（至多 5 次迭代 + 交错向量）。",
                    "references": [
                        {"type": "textbook", "cite": "G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., Johns Hopkins 2013, §3.2, §5.2, §5.3"},
                        {"type": "textbook", "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, ch. 7, 12, 15, 20", "doi": "10.1137/1.9780898718027"},
                        {"type": "paper", "cite": "J. L. Rigal, J. Gaches, On the compatibility of a given solution with the data of a linear system, J. ACM 14 (1967) 543–548", "doi": "10.1145/321406.321416"},
                        {"type": "paper", "cite": "T. Ogita, S. M. Rump, S. Oishi, Accurate sum and dot product, SIAM J. Sci. Comput. 26 (2005) 1955–1988", "doi": "10.1137/030601818"}]},
        backend={"library": "aeoi-pyscipy op_linsolve (Python + numpy elementwise arithmetic)", "routine": "op_linsolve.LinSolve.execute",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={"/A": {"policy": "reject"}, "/b": {"policy": "reject"}},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_rows": MAX_DIM, "max_cols": MAX_DIM, "max_rhs": MAX_RHS, "max_refinement_steps": 3,
                "condition_estimator_iterations": 5},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=["W.NUMERICAL.ILL_CONDITIONED"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "A、b 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "not_underdetermined", "statement": "m ≥ n", "enforced_by": "error:E.DOMAIN.UNDERDETERMINED"},
                     {"id": "nonsingular", "statement": "方阵非奇异、超定时列满秩（数值意义）", "enforced_by": "error:E.NUMERICAL.SINGULAR"},
                     {"id": "conditioning", "statement": "rcond ≥ ε 时解才可能有有效数字", "enforced_by": "warning:W.NUMERICAL.ILL_CONDITIONED"},
                     {"id": "model", "statement": "A、b 准确描述问题（row-major 展平）", "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["backward_stable", "all_finite"],
                 "guards": {"backward_stable": ["/x"], "all_finite": ["/x"]}},
    )


# ------------------------------------------------------------------ kernels

def _lu(A):
    n = A.shape[0]
    LU = A.copy()
    perm = np.arange(n)
    for k in range(n):
        col = np.abs(LU[k:, k])
        p = k + int(np.argmax(col))           # first maximal entry
        if LU[p, k] == 0.0:
            raise Fail("E.NUMERICAL.SINGULAR", f"A 奇异：第 {k} 步消元的主元恰为 0（LS-2）", "/A",
                       {"step": k}, [C.ask_user("检查 A 是否录入有误（row-major），或方程是否线性相关")],
                       agent_hint="A 奇异：检查矩阵（含行列顺序：row-major）；方程不独立时问用户要哪种解")
        if p != k:
            LU[[k, p]] = LU[[p, k]]
            perm[[k, p]] = perm[[p, k]]
        if k + 1 < n:
            LU[k + 1:, k] = LU[k + 1:, k] / LU[k, k]
            LU[k + 1:, k + 1:] = LU[k + 1:, k + 1:] - np.outer(LU[k + 1:, k], LU[k, k + 1:])
    return LU, perm


def _lu_solve(LU, perm, b):
    n = LU.shape[0]
    y = np.empty(n)
    pb = b[perm]
    for i in range(n):
        y[i] = pb[i] - C.dot_exact(LU[i, :i], y[:i]) if i else pb[i]
    x = np.empty(n)
    for i in range(n - 1, -1, -1):
        s = y[i] - C.dot_exact(LU[i, i + 1:], x[i + 1:]) if i < n - 1 else y[i]
        x[i] = s / LU[i, i]
    return x


def _lu_solve_t(LU, perm, c):
    """Solve A^T z = c with A = P^T L U  ->  U^T L^T P z = c."""
    n = LU.shape[0]
    w = np.empty(n)
    for i in range(n):
        s = c[i] - C.dot_exact(LU[:i, i], w[:i]) if i else c[i]
        w[i] = s / LU[i, i]
    v = np.empty(n)
    for i in range(n - 1, -1, -1):
        v[i] = w[i] - C.dot_exact(LU[i + 1:, i], v[i + 1:]) if i < n - 1 else w[i]
    z = np.empty(n)
    z[perm] = v
    return z


def _tri_solve(R, b):
    n = R.shape[0]
    x = np.empty(n)
    for i in range(n - 1, -1, -1):
        s = b[i] - C.dot_exact(R[i, i + 1:], x[i + 1:]) if i < n - 1 else b[i]
        x[i] = s / R[i, i]
    return x


def _tri_solve_t(R, c):
    n = R.shape[0]
    x = np.empty(n)
    for i in range(n):
        s = c[i] - C.dot_exact(R[:i, i], x[:i]) if i else c[i]
        x[i] = s / R[i, i]
    return x


def _norm2(v):
    v = np.asarray(v, dtype=float).ravel()
    s = float(np.max(np.abs(v))) if v.size else 0.0
    if s == 0.0:
        return 0.0
    w = v / s
    return s * math.sqrt(math.fsum((w * w).tolist()))


def _inv_norm1_est(solve, solve_t, n):
    """Hager-Higham estimate of ||M^-1||_1 (fixed start vector, <= 5 iterations, alternating vector)."""
    x = np.full(n, 1.0 / n)
    est = 0.0
    for it in range(5):
        y = solve(x)
        e = math.fsum(np.abs(y).tolist())
        if it > 0 and e <= est:
            break
        est = e
        xi = np.where(y >= 0, 1.0, -1.0)
        z = solve_t(xi)
        j = int(np.argmax(np.abs(z)))
        if it > 0 and abs(z[j]) <= C.dot_exact(z, x):
            break
        x = np.zeros(n)
        x[j] = 1.0
    if n > 1:
        alt = np.array([(-1.0) ** i * (1.0 + i / (n - 1)) for i in range(n)])
        y = solve(alt)
        est = max(est, 2.0 * math.fsum(np.abs(y).tolist()) / (3.0 * n))
    return est


def _householder_qr(A, B):
    """In-place Householder QR of A (m x n) applied also to B (m x k). Returns R (n x n), Q^T B."""
    R = A.copy()
    QB = B.copy()
    m, n = R.shape
    for j in range(n):
        x = R[j:, j]
        alpha = x[0]
        xnorm = _norm2(x[1:]) if m - j > 1 else 0.0
        if xnorm == 0.0:
            continue          # tau = 0, H = I (dlarfg)
        beta = -math.copysign(math.hypot(alpha, xnorm), alpha)
        tau = (beta - alpha) / beta
        v = np.concatenate([[1.0], x[1:] / (alpha - beta)])
        R[j, j] = beta
        R[j + 1:, j] = 0.0
        for M in (R[j:, j + 1:], QB[j:, :]):
            if M.shape[1] == 0:
                continue
            w = np.array([C.dot_exact(v, M[:, c]) for c in range(M.shape[1])])
            M -= tau * np.outer(v, w)
    return R[:n, :n], QB[:n, :]


class LinSolve(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        A = C.read_array(args["A"], "/A", allowed=("f64", "i64"), max_elements=MAX_DIM * MAX_DIM)
        b = C.read_array(args["b"], "/b", allowed=("f64", "i64"), max_elements=MAX_DIM * MAX_RHS)
        if len(A.shape) != 2:
            raise Fail("E.VALIDATION.SHAPE", f"A 必须是二维矩阵 [m, n]，这里 shape = {A.shape}", "/A/shape", {"shape": A.shape},
                       [C.ask_user("把 A 写成 {dtype, shape: [m, n], data}（row-major）")])
        m, n = A.shape
        if m == 0 or n == 0:
            raise Fail("E.VALIDATION.SHAPE", "A 不能是空矩阵", "/A/shape", {"shape": A.shape})
        if len(b.shape) not in (1, 2) or b.shape[0] != m or (len(b.shape) == 2 and b.shape[1] == 0):
            raise Fail("E.VALIDATION.SHAPE", f"b 的形状 {b.shape} 与 A（{m} 行）不符：应为 [{m}] 或 [{m}, k]", "/b/shape",
                       {"A_shape": A.shape, "b_shape": b.shape}, [C.ask_user("核对 b 的长度与 A 的行数（每列一个右端项）")])
        if m < n:
            raise Fail("E.DOMAIN.UNDERDETERMINED", f"A 是 {m}×{n}（方程少于未知数），解不唯一（LS-1）", "/A/shape",
                       {"m": m, "n": n},
                       [C.ask_user("方程不够：要补充哪些约束，或要哪一种解（最小范数解等）", "which_solution")],
                       agent_hint="不要自己挑一个解；问用户补充方程或说明要哪种解", retryable=False)
        if m > MAX_DIM or n > MAX_DIM:
            raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"A 的维数超过上限 {MAX_DIM}", "/A/shape", {"limit": MAX_DIM},
                       [C.ask_user("缩小问题规模")])
        Bv = b.values.astype(float).reshape(m, -1)
        return dict(A=A.values.astype(float), B=Bv, vec=len(b.shape) == 1, m=m, n=n,
                    refinement=args.get("refinement", "auto"))

    def estimate(self, P):
        m, n, k = P["m"], P["n"], P["B"].shape[1]
        fl = (2 * n ** 3 // 3 + 2 * n * n * k) if m == n else (2 * m * n * n + 4 * m * n * k)
        return {"m": m, "n": n, "rhs": k, "flops_estimate": int(fl)}

    def execute(self, P):
        A, B, m, n = P["A"], P["B"], P["m"], P["n"]
        k = B.shape[1]
        X = np.empty((n, k))
        steps_max = 0
        if m == n:
            kind, method = "square", "lu_partial_pivoting"
            LU, perm = _lu(A)
            for c in range(k):
                b = B[:, c]
                x = _lu_solve(LU, perm, b)
                steps = 0
                if P["refinement"] == "auto":
                    prev = None
                    for _ in range(3):
                        r = C.residual_exact(A, x, b)
                        d = _lu_solve(LU, perm, r)
                        nd = float(np.max(np.abs(d)))
                        if not math.isfinite(nd):
                            break
                        if prev is not None and nd > prev / 2:
                            break        # correction did not halve: stop, keep the current x
                        # LS-2 (v0.4.19): a zero correction is applied and counted as a step
                        x = x + d
                        steps += 1
                        prev = nd
                        if nd <= 2 * C.U * float(np.max(np.abs(x))):
                            break
                X[:, c] = x
                steps_max = max(steps_max, steps)
            inv_est = _inv_norm1_est(lambda v: _lu_solve(LU, perm, v), lambda v: _lu_solve_t(LU, perm, v), n)
            normA1 = float(np.max(np.sum(np.abs(A), axis=0)))
            kappa = normA1 * inv_est
            of = "A"
        else:
            kind, method = "overdetermined", "householder_qr"
            R, QB = _householder_qr(A, B)
            d = np.abs(np.diag(R))
            dmax = float(np.max(d))
            thr = max(m, n) * C.U * dmax
            if dmax == 0.0 or np.any(d <= thr):
                rank = int(np.sum(d > thr))
                raise Fail("E.NUMERICAL.SINGULAR", f"A 列秩不足（估计秩 {rank} < n = {n}）：最小二乘解不唯一，不能返回（LS-3）", "/A",
                           {"estimated_rank": rank, "n": n, "threshold": "max(m, n)·u·max|r_ii|"},
                           [C.ask_user("A 的列线性相关：去掉冗余列，或说明要哪一个解（例如最小范数解）")],
                           agent_hint="去掉线性相关的列或问用户；不要随意加正则化")
            for c in range(k):
                X[:, c] = _tri_solve(R, QB[:, c])
            inv_est = _inv_norm1_est(lambda v: _tri_solve(R, v), lambda v: _tri_solve_t(R, v), n)
            kappa = float(np.max(np.sum(np.abs(R), axis=0))) * inv_est
            of = "R（A = QR）"
        # LS-4 backward error with correctly rounded residuals
        etas, rnorms = [], []
        normA_inf = float(np.max(np.sum(np.abs(A), axis=1)))
        normA_F = _norm2(A)
        for c in range(k):
            r = C.residual_exact(A, X[:, c], B[:, c])
            rn = _norm2(r)
            rnorms.append(rn)
            if kind == "square":
                den = normA_inf * float(np.max(np.abs(X[:, c]))) + float(np.max(np.abs(B[:, c])))
                eta = float(np.max(np.abs(r))) / den if den > 0 else 0.0
            else:
                atr = np.array([C.dot_exact(A[:, j], r) for j in range(n)])
                den = normA_F * (rn + normA_F * _norm2(X[:, c]))
                eta = _norm2(atr) / den if den > 0 else 0.0
            etas.append(eta)
        eta = max(etas)
        thr = 10 * (n if kind == "square" else m) * C.U
        rcond = 1.0 / kappa if kappa > 0 else math.inf
        if kind == "square":
            fwd = kappa * eta
            basis = "一阶估计 ‖Δx‖/‖x‖ ≲ κ₁·η（κ₁ 为下界估计）；非严格上界"
        else:
            xn = max(_norm2(X[:, c]) for c in range(k))
            fwd = eta * (2 * kappa + kappa * kappa * (max(rnorms) / (normA_F * xn) if normA_F * xn > 0 else 0.0))
            basis = "一阶估计 η·(2κ + κ²·‖r‖/(‖A‖‖x‖))（Golub–Van Loan 5.3.8，κ 为 R 的 1-范数条件数估计）；非严格上界"
        warnings = []
        if rcond < C.EPS:
            warnings.append({"code": "W.NUMERICAL.ILL_CONDITIONED",
                             "message": f"A 接近奇异：rcond = {rcond:.3e} < ε（κ₁ ≈ {kappa:.3e}）；前向误差估计 {fwd:.1e}，解可能没有有效数字",
                             "path": "/A", "details": {"rcond": rcond, "kappa1_estimate": kappa}})
        finite = bool(np.all(np.isfinite(X)))
        xout = X[:, 0] if P["vec"] else X
        result = {"x": C.dense_f64(xout),
                  "residual_norm": rnorms[0] if P["vec"] else C.dense_f64(np.array(rnorms)),
                  "method": method,
                  "shape": {"m": m, "n": n, "rhs": k, "kind": kind},
                  "refinement_steps": steps_max,
                  "refinement": P["refinement"]}
        trust = {
            "convergence": {"applicable": False},
            "residual": {"backward_error": eta, "threshold": thr,
                         "definition": ("‖b − A·x‖∞ / (‖A‖∞‖x‖∞ + ‖b‖∞)" if kind == "square" else
                                        "‖Aᵀr‖₂ / (‖A‖_F(‖r‖₂ + ‖A‖_F‖x‖₂))") + "，残差为正确舍入内积，取各右端项最大"},
            "condition": {"kappa1_estimate": kappa, "rcond": rcond, "of": of,
                          "estimator": "Hager–Higham 1-范数估计（下界）"},
            "error_estimate": {"x_rel_forward": C.enc_float(fwd), "basis": basis},
            "checks": {"backward_stable": bool(eta <= thr), "all_finite": finite},
            "accuracy_grade": "C", "credentials": None,
            "derived": {"refinement": P["refinement"], "norm1_A": float(np.max(np.sum(np.abs(A), axis=0)))},
        }
        return result, trust, warnings
