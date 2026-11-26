"""fit.poly.roots (profile 10.34 RT-1 .. RT-8), written from the profile.

Roots: eigenvalues of the balanced companion matrix by this package's own Francis double-shift QR
(aeoi_kernels.hqr, not LAPACK / numpy.roots), at most 4 Newton steps per root accepted only when
|q(z)| decreases, conjugate pairs exact. Inclusion radii: Braess-Hadeler / Carstensen theorem with
the rounding of q(z_i) (complex Horner) and of the product prod_{j != i}(z_i - z_j) accounted for;
exactly equal approximations are spread on a small circle first (RT-4).
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
from aeoi_common import Fail

OP_ID = "fit.poly.roots"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:roots:balanced-companion-francis-qr+newton+inclusion"
MAX_DEGREE = 500


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.RANGE", "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED",
              "E.VALIDATION.REF_FORBIDDEN", "E.VALIDATION.METHOD_UNAVAILABLE", "E.DOMAIN.ZERO_POLYNOMIAL",
              "E.NUMERICAL.NO_CONVERGENCE", "E.RESOURCE.INPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Polynomial roots with inclusion radii (Python second implementation)",
        description="实系数多项式的全部根（= MATLAB roots），p 从最高次到常数项。根以 c128 返回，实部升序、实部相同按虚部降序。"
                    "每个根带包含半径（真根一定在这些圆盘的并内）；重根或密集根组成的簇单独报告，簇内各根的位置不能单独依赖。",
        category="analysis", tags=["polynomial", "roots", "companion"],
        applicability={"preconditions": ["p 为 [n + 1] 实数系数，最高次在前", "最高次的 0 系数被去掉（警告）", "次数 ≤ 500"],
                       "not_for": ["复系数多项式", "只求区间内的实根（用 nonlinear.root.fzero）"], "siso_only": False},
        provenance={"algorithm": "去掉最高次的 0 系数；末尾 k 个 0 系数给出 k 个精确零根。其余部分 q（次数 m）：友矩阵（首行 −q[1:]/q[0]）"
                                 "做 Parlett–Reinsch 2 的幂平衡，自写 Francis 隐式双位移 QR 求特征值，每个根至多 4 步 Newton（只接受使 |q| 下降的步），"
                                 "共轭对精确共轭。包含半径：q 的根在 ∪ D(z_i, m·|W_i|) 内，W_i = q(z_i)/(q_0·Π_{j≠i}(z_i − z_j))，"
                                 "|q(z_i)| 加上复数 Horner 的舍入界 γ_{4m+4}·q̃(|z_i|)，乘积按 γ_{4m+2} 取下界；相等的近似值先放到半径 max(|c|,1)·u^{1/k} 的圆上。"
                                 "k 个圆盘连成的分支恰含 k 个根；多于一个圆盘的分支报告为根簇。",
                    "references": [
                        {"type": "paper", "cite": "D. Braess, K. P. Hadeler, Simultaneous inclusion of the zeros of a polynomial, Numer. Math. 21, 1973",
                         "doi": "10.1007/BF01436624"},
                        {"type": "paper", "cite": "C. Carstensen, Inclusion of the roots of a polynomial based on Gerschgorin's theorem, Numer. Math. 59, 1991",
                         "doi": "10.1007/BF01385789"},
                        {"type": "paper", "cite": "A. Edelman, H. Murakami, Polynomial roots from companion matrix eigenvalues, Math. Comp. 64(210), 1995",
                         "doi": "10.1090/S0025-5718-1995-1262279-2"},
                        {"type": "textbook", "cite": "J. H. Wilkinson, C. Reinsch, Handbook for Automatic Computation II: Linear Algebra, Springer 1971（balance、hqr）",
                         "doi": "10.1007/978-3-642-86940-2"}]},
        backend={"library": "aeoi-pyscipy aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "op_roots.Roots.execute (aeoi_kernels.balance + aeoi_kernels.hqr + Newton; inclusion radii)",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T2",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_degree": MAX_DEGREE, "max_qr_iterations_per_eigenvalue": 60, "newton_steps": 4},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors,
        warnings=["W.VALIDATION.LEADING_ZEROS_STRIPPED", "W.ACCURACY.CLUSTERED_ROOTS"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "所有系数有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "nonzero", "statement": "p 不是零多项式", "enforced_by": "error:E.DOMAIN.ZERO_POLYNOMIAL"},
                     {"id": "coefficients_exact", "statement": "根与半径针对给定的 f64 系数；系数本身的误差另经根的条件数放大", "enforced_by": "caller"},
                     {"id": "clusters", "statement": "簇内各根的位置不能单独确定", "enforced_by": "warning:W.ACCURACY.CLUSTERED_ROOTS"}],
                 "exactness": {"class": "approximate", "bound_kind": "rigorous"},
                 "postconditions": ["backward_stable", "radii_finite"],
                 "guards": {"backward_stable": ["/roots", "/error_radius"], "radii_finite": ["/error_radius", "/multiplicity"]}},
    )


def _horner_c(q, z):
    v = 0j
    for a in q:
        v = v * z + a
    return v


def _qtilde(q, t):
    v = 0.0
    for a in q:
        v = v * t + abs(a)
    return v


def _prod_abs_lower(zi, others):
    """|prod (zi - zj)| computed with an explicit exponent (no overflow), and its lower bound."""
    mant = 1.0 + 0j
    exp = 0
    for zj in others:
        mant *= (zi - zj)
        a = abs(mant)
        if a == 0.0:
            return 0.0, 0
        e = math.frexp(a)[1]
        mant = complex(math.ldexp(mant.real, -e), math.ldexp(mant.imag, -e))
        exp += e
    return abs(mant), exp


def _radii(q, z):
    """Theorem radii for the approximations z of the roots of q (degree m = len(z)). Returns (radii, spread)."""
    m = len(z)
    q0 = abs(q[0])
    pts = np.array(z, dtype=complex)
    spread = np.zeros(m)
    # exactly equal approximations: spread on a circle of radius max(|c|, 1) u^(1/k)
    groups = {}
    for i, v in enumerate(pts):
        groups.setdefault((v.real, v.imag), []).append(i)
    for (re, im), idx in groups.items():
        k = len(idx)
        if k < 2:
            continue
        c = complex(re, im)
        rho = max(abs(c), 1.0) * C.U ** (1.0 / k)
        for t, i in enumerate(idx):
            ang = 2.0 * math.pi * t / k
            pts[i] = c + rho * complex(math.cos(ang), math.sin(ang))
            spread[i] = abs(pts[i] - c) * (1 + 4 * C.U)
    gq = K.gamma(4 * m + 4)
    gp = K.gamma(4 * m + 2)
    radii = np.empty(m)
    for i in range(m):
        zi = pts[i]
        qv = abs(_horner_c(q, zi)) + gq * _qtilde(q, abs(zi))
        mant, ex = _prod_abs_lower(zi, [pts[j] for j in range(m) if j != i])
        low = mant * (1.0 - gp)
        if low <= 0.0 or not math.isfinite(qv):
            radii[i] = math.inf
            continue
        # m * qv / (q0 * low * 2^ex), evaluated in scaled form
        w = (qv / q0) / low
        r = m * math.ldexp(w, -ex) * (1 + 8 * C.U)
        radii[i] = r + spread[i]
    return radii


def _components(z, r):
    m = len(z)
    parent = list(range(m))

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a
    for i in range(m):
        for j in range(i + 1, m):
            if abs(z[i] - z[j]) <= (r[i] + r[j]) * (1 + 4 * C.U):
                parent[find(i)] = find(j)
    comp = {}
    for i in range(m):
        comp.setdefault(find(i), []).append(i)
    return list(comp.values())


class Roots(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        a = C.read_array(args["p"], "/p", allowed=("f64", "i64"), max_elements=10 ** 6)
        if len(a.shape) != 1:
            raise Fail("E.VALIDATION.SHAPE", f"p 必须是一维 [n + 1] 系数向量，这里 shape = {a.shape}（RT-1）", "/p/shape",
                       {"shape": a.shape}, [C.ask_user("把系数写成一维数组，最高次在前")])
        if a.shape[0] == 0:
            raise Fail("E.VALIDATION.SHAPE", "p 至少要有一个系数", "/p/shape", {"shape": a.shape}, [C.ask_user("多项式的系数是什么？")])
        p = [float(x) for x in a.values]
        lead = 0
        while lead < len(p) and p[lead] == 0.0:
            lead += 1
        if lead == len(p):
            raise Fail("E.DOMAIN.ZERO_POLYNOMIAL", "p 的系数全为 0：零多项式处处为 0，根不确定（RT-1）。", "/p",
                       remediation=[C.ask_user("多项式的系数是什么？")], agent_hint="检查系数是否传错（例如全被舍入成了 0）。",
                       retryable=False)
        p = p[lead:]
        deg = len(p) - 1
        if deg > MAX_DEGREE:
            raise Fail("E.VALIDATION.RANGE", f"次数 {deg} 超过上限 {MAX_DEGREE}（RT-1）", "/p/shape",
                       {"degree": deg, "limit": MAX_DEGREE}, [C.ask_user("高次多项式的根对系数极其敏感；确认是否真的需要这么高的次数")])
        return dict(p=p, lead=lead, deg=deg)

    def estimate(self, P):
        return {"degree": P["deg"], "flops_estimate": 10 * P["deg"] ** 3}

    def execute(self, P):
        p, deg, lead = P["p"], P["deg"], P["lead"]
        warnings = []
        if lead:
            warnings.append({"code": "W.VALIDATION.LEADING_ZEROS_STRIPPED",
                             "message": f"去掉了 {lead} 个最高次的 0 系数，次数按 {deg} 计（RT-1）", "path": "/p",
                             "details": {"removed": lead}})
        kz = 0
        while kz < deg and p[deg - kz] == 0.0:
            kz += 1
        q = p[:len(p) - kz]
        m = len(q) - 1
        try:
            zq = K.poly_roots(q) if m >= 1 else np.zeros(0, dtype=complex)
        except K.NoConvergence as e:
            raise Fail("E.NUMERICAL.NO_CONVERGENCE", f"友矩阵的 QR 迭代没有收敛：{e}", "/p",
                       remediation=[C.ask_user("系数尺度差异可能过大；确认系数")])
        radii = _radii(q, zq) if m >= 1 else np.zeros(0)
        comps = _components(zq, radii) if m >= 1 else []
        mult = np.ones(m, dtype=int)
        err_r = radii.copy()
        for comp in comps:
            if len(comp) > 1:
                for i in comp:
                    mult[i] = len(comp)
                    err_r[i] = max(abs(zq[i] - zq[j]) + radii[j] for j in comp) * (1 + 4 * C.U)
        # backward error of the approximations (unspread)
        bw = 0.0
        for zi in zq:
            qt = _qtilde(q, abs(zi))
            v = abs(_horner_c(q, zi))
            bw = max(bw, v / qt if qt > 0 else (0.0 if v == 0 else math.inf))
        thr = 10 * m * C.U
        # assemble with the exact zero roots and sort
        allz = list(zq) + [0j] * kz
        allr = list(err_r) + [0.0] * kz
        allm = list(mult) + [kz] * kz
        comp_of = {}
        for ci, comp in enumerate(comps):
            for i in comp:
                comp_of[i] = ci
        order = sorted(range(len(allz)), key=lambda i: (allz[i].real, -allz[i].imag))
        pos = {old: new for new, old in enumerate(order)}
        roots = np.array([allz[i] for i in order], dtype=complex)
        rad = np.array([allr[i] for i in order])
        mul = [int(allm[i]) for i in order]
        clusters = []
        for comp in comps:
            if len(comp) < 2:
                continue
            idx = sorted(pos[i] for i in comp)
            cen = sum(zq[i] for i in comp) / len(comp)
            rr = max(abs(zq[i] - cen) + radii[i] for i in comp)
            clusters.append({"indices": idx, "multiplicity": len(comp), "center": [cen.real, cen.imag], "radius": rr})
        clusters.sort(key=lambda c: c["indices"][0])
        if clusters:
            warnings.append({"code": "W.ACCURACY.CLUSTERED_ROOTS",
                             "message": f"{len(clusters)} 个根簇（重根或密集根）：簇内各根的位置不能单独确定，簇的中心、半径与根数可以依赖；"
                                        "各根的 error_radius 是到同簇最远点的距离。",
                             "path": "/p", "details": {"clusters": clusters,
                                                       "unreliable_roots": [f"/roots/data/{i}" for c in clusters for i in c["indices"]]}})
        real_count = int(sum(1 for z in roots if z.imag == 0.0))
        result = {"roots": C.dense_c128(roots.real, roots.imag), "error_radius": C.dense_f64(rad), "multiplicity": mul,
                  "degree": deg, "real_count": real_count, "all_real": real_count == deg, "zero_roots": kz,
                  "leading_zeros_removed": lead}
        finite = bool(np.all(np.isfinite(rad)))
        amax = float(np.max(rad)) if rad.size else 0.0
        rmax = float(np.max(np.abs(roots))) if roots.size else 0.0
        trust = {"convergence": {"applicable": False},
                 "residual": {"backward_error": bw, "threshold": thr,
                              "definition": "max_i |q(z_i)|/q̃(|z_i|)（逐根分量后向误差；q 为去掉零根后的多项式，q̃(t) = Σ|a_k|t^k）"},
                 "condition": {"degree_q": m, "zero_roots": kz},
                 "error_estimate": {"abs_max": amax, "rel_to_max_root": (amax / rmax) if rmax > 0 else amax,
                                    "basis": "包含定理（Braess–Hadeler 1973，Carstensen 1991）：q 的根全在 ∪ D(z_i, m·|W_i|) 内，k 个圆盘连成的分支恰含 k 个根；"
                                             "半径计入 q(z_i) 的复数 Horner 舍入与乘积的舍入。孤立根的 error_radius 是它的圆盘半径，簇内根是到同簇最远点的距离"},
                 "checks": {"backward_stable": bool(bw <= thr), "radii_finite": finite},
                 "accuracy_grade": "C", "credentials": None,
                 "derived": {"algorithm": "eigenvalues of the balanced companion matrix (own Francis double-shift QR), ≤ 4 Newton steps",
                             "clusters": len(clusters)}}
        return result, trust, warnings
