"""Shared LTI system representation for the control operators (spec 3.1 T-5, T-6; 3.10 R-5 .. R-9, D-1, D-2).

Parsing and validation of `LtiSystem` (tf / ss / zpk, continuous ts = 0 or discrete ts > 0), the
conversions between representations, frequency-response evaluation, poles, DC gain and the
improper-system error with its remediation. Used by control.lti.bode, control.lti.step and
control.lti.convert. Numerics come from aeoi_kernels (own Francis QR, LU, expm, Newton-polished
companion roots); no python-control, no scipy.signal.
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
from aeoi_common import Fail

CONJ_RTOL = 1e-12          # T-5
MAX_STATES = 200           # own limit (M-10)


class LTI:
    """A parsed LTI system. kind in {tf, ss, zpk}; dims (ny, nu); ts; warnings from parsing."""

    def __init__(self, kind, ts, ny, nu, nested=False):
        self.kind, self.ts, self.ny, self.nu, self.nested = kind, ts, ny, nu, nested
        self.num = self.den = None          # tf: [ny][nu] lists of coefficient lists
        self.A = self.B = self.C = self.D = None
        self.z = self.p = None
        self.k = None
        self.warnings = []

    @property
    def siso(self):
        return self.ny == 1 and self.nu == 1

    @property
    def discrete(self):
        return self.ts > 0


# ------------------------------------------------------------------------------------------
# parsing


def _poly(c, path, warnings):
    if not isinstance(c, list) or not c or not all(isinstance(x, (int, float)) and not isinstance(x, bool) for x in c):
        raise Fail("E.VALIDATION.SCHEMA", f"{path} 必须是非空实数数组（降幂多项式）", path)
    for i, x in enumerate(c):
        if not math.isfinite(float(x)):
            raise Fail("E.VALIDATION.NON_FINITE", f"{path}/{i} 不是有限值（R-2）", f"{path}/{i}")
    out, removed = K.strip_leading_zeros(c)
    if removed and not (out == [0.0]):
        warnings.append({"code": "W.VALIDATION.LEADING_ZEROS_STRIPPED",
                         "message": f"{path} 去掉了 {removed} 个最高次的 0 系数（T-6）", "path": path,
                         "details": {"removed": removed}})
    return out


def _is_nested(v):
    return isinstance(v, list) and len(v) > 0 and all(isinstance(r, list) for r in v) and \
        all(isinstance(e, list) for r in v for e in r)


def _parse_tf(obj, path, sysobj):
    num, den = obj["num"], obj["den"]
    nn, nd = _is_nested(num), _is_nested(den)
    if nn != nd:
        raise Fail("E.VALIDATION.SHAPE", "num 与 den 必须同为一维多项式（SISO）或同为 [ny][nu] 嵌套数组（MIMO）",
                   path + ("/num" if nn else "/den"), remediation=[C.ask_user("确认系统是 SISO 还是 MIMO，num/den 的写法保持一致")])
    w = []
    if not nn:
        sysobj.num = [[_poly(num, path + "/num", w)]]
        sysobj.den = [[_poly(den, path + "/den", w)]]
    else:
        ny = len(num)
        nu = len(num[0])
        if ny == 0 or nu == 0 or any(len(r) != nu for r in num) or len(den) != ny or any(len(r) != nu for r in den):
            raise Fail("E.VALIDATION.SHAPE", "MIMO tf 的 num 与 den 必须是同形的 [ny][nu] 数组（每个元素一个多项式）",
                       path + "/den", {"num_dims": [len(num), [len(r) for r in num]], "den_dims": [len(den), [len(r) for r in den]]},
                       [C.ask_user("核对每个输出/输入通道的分子与分母")])
        sysobj.num = [[_poly(num[i][j], f"{path}/num/{i}/{j}", w) for j in range(nu)] for i in range(ny)]
        sysobj.den = [[_poly(den[i][j], f"{path}/den/{i}/{j}", w) for j in range(nu)] for i in range(ny)]
    for i, row in enumerate(sysobj.den):
        for j, d in enumerate(row):
            if d == [0.0]:
                p = path + ("/den" if not nn else f"/den/{i}/{j}")
                raise Fail("E.VALIDATION.ZERO_DENOMINATOR", f"{p} 全为 0：传递函数没有定义（T-6）", p,
                           remediation=[C.ask_user("分母多项式是什么？全为 0 通常是录入错误")])
    sysobj.ny, sysobj.nu = len(sysobj.num), len(sysobj.num[0])
    sysobj.nested = nn
    sysobj.warnings.extend(w)


def _parse_ss(obj, path, sysobj):
    mats = {}
    for name in ("A", "B", "C", "D"):
        a = C.read_array(obj[name], f"{path}/{name}", allowed=("f64", "i64"))
        if len(a.shape) != 2:
            raise Fail("E.VALIDATION.SHAPE", f"{path}/{name} 必须是二维矩阵，这里 shape = {a.shape}", f"{path}/{name}/shape",
                       {"shape": a.shape}, [C.ask_user("矩阵按 {dtype, shape:[行, 列], data(row-major)} 给出")])
        mats[name] = a.values.astype(float)
    A, B, Cm, D = mats["A"], mats["B"], mats["C"], mats["D"]
    n = A.shape[0]
    if A.shape[1] != n:
        raise Fail("E.VALIDATION.SHAPE", f"A 必须是方阵，这里是 {list(A.shape)}", f"{path}/A/shape", {"A": list(A.shape)},
                   [C.ask_user("核对 A 的维数")])
    m, p = B.shape[1], Cm.shape[0]
    bad = None
    if B.shape[0] != n:
        bad = ("B", f"B 的行数 {B.shape[0]} 应等于状态数 n = {n}")
    elif Cm.shape[1] != n:
        bad = ("C", f"C 的列数 {Cm.shape[1]} 应等于状态数 n = {n}")
    elif D.shape != (p, m):
        bad = ("D", f"D 的形状 {list(D.shape)} 应为 [ny, nu] = [{p}, {m}]")
    if bad:
        raise Fail("E.VALIDATION.SHAPE", f"状态空间矩阵维数不一致：{bad[1]}（T-5）", f"{path}/{bad[0]}/shape",
                   {"A": list(A.shape), "B": list(B.shape), "C": list(Cm.shape), "D": list(D.shape)},
                   [C.ask_user("核对 A[n,n]、B[n,nu]、C[ny,n]、D[ny,nu]（row-major）")])
    if p == 0 or m == 0:
        raise Fail("E.VALIDATION.SHAPE", "系统至少要有一个输入和一个输出", f"{path}/D/shape", {"D": list(D.shape)})
    if n > MAX_STATES:
        raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"状态数 {n} 超过上限 {MAX_STATES}", f"{path}/A/shape",
                   {"states": n, "limit": MAX_STATES}, [C.ask_user("先做模型降阶，或拆分系统")])
    sysobj.A, sysobj.B, sysobj.C, sysobj.D = A, B, Cm, D
    sysobj.ny, sysobj.nu = p, m


def _complex_list(v, path):
    out = []
    for i, x in enumerate(v):
        if not (isinstance(x, list) and len(x) == 2):
            raise Fail("E.VALIDATION.SCHEMA", f"{path}/{i} 必须是 [re, im]", f"{path}/{i}")
        for t in x:
            if not isinstance(t, (int, float)) or isinstance(t, bool) or not math.isfinite(float(t)):
                raise Fail("E.VALIDATION.NON_FINITE", f"{path}/{i} 不是有限复数", f"{path}/{i}")
        out.append(complex(float(x[0]), float(x[1])))
    return np.array(out, dtype=complex)


def _check_conjugate(v, path):
    """T-5: complex values must come in conjugate pairs (relative tolerance 1e-12)."""
    used = [False] * len(v)
    for i, z in enumerate(v):
        if z.imag == 0.0 or used[i]:
            continue
        used[i] = True
        target = z.conjugate()
        tol = CONJ_RTOL * abs(z)
        best = None
        for j in range(len(v)):
            if not used[j] and abs(v[j] - target) <= tol:
                best = j
                break
        if best is None:
            raise Fail("E.VALIDATION.NOT_CONJUGATE",
                       f"{path}/{i} = {float(z.real)!r}{float(z.imag):+.17g}j 没有共轭配对（复数零极点必须共轭成对，相对容差 1e-12，T-5）",
                       f"{path}/{i}", {"value": [float(z.real), float(z.imag)], "index": i},
                       [C.patch_remedy("add_conjugate", "补上它的共轭",
                                       [{"op": "add", "path": f"{path}/-", "value": [float(z.real), -float(z.imag)]}],
                                       confidence="medium", confirm=True),
                        C.ask_user("实系数系统的复零极点总是共轭成对；确认是否漏写了共轭")])
        used[best] = True


def _parse_zpk(obj, path, sysobj):
    z = _complex_list(obj["z"], path + "/z")
    p = _complex_list(obj["p"], path + "/p")
    k = obj["k"]
    if not math.isfinite(float(k)):
        raise Fail("E.VALIDATION.NON_FINITE", "k 不是有限值", path + "/k")
    _check_conjugate(z, path + "/z")
    _check_conjugate(p, path + "/p")
    sysobj.z, sysobj.p, sysobj.k = z, p, float(k)
    sysobj.ny = sysobj.nu = 1


def parse_system(obj, path="/system"):
    """Parse a schema-valid LtiSystem object into an LTI (raises Fail on semantic errors)."""
    kind = obj["kind"]
    ts = float(obj["ts"])
    if not math.isfinite(ts):
        raise Fail("E.VALIDATION.NON_FINITE", "ts 不是有限值", path + "/ts")
    s = LTI(kind, ts, 1, 1)
    if kind == "tf":
        _parse_tf(obj, path, s)
    elif kind == "ss":
        _parse_ss(obj, path, s)
    else:
        _parse_zpk(obj, path, s)
    return s


# ------------------------------------------------------------------------------------------
# properness and the improper-system error (I-7; 10.1 example remediation)


def degree(c):
    return len(c) - 1 if c != [0.0] else -1


def improper_info(s):
    """Return None when proper, else (excess relative degree, location description)."""
    if s.kind == "tf":
        worst = None
        for i in range(s.ny):
            for j in range(s.nu):
                dn, dd = degree(s.num[i][j]), degree(s.den[i][j])
                if dn > dd and (worst is None or dn - dd > worst[0]):
                    worst = (dn - dd, i, j, dn, dd)
        return worst
    if s.kind == "zpk" and len(s.z) > len(s.p):
        return (len(s.z) - len(s.p), 0, 0, len(s.z), len(s.p))
    return None


def require_proper(s, path="/system", tau=1e-3):
    info = improper_info(s)
    if info is None:
        return
    r, i, j, dn, dd = info
    rem = []
    if s.kind == "tf" and not s.discrete:
        # multiply every improper element's denominator by (tau s + 1)^r_ij
        patch = []
        for a in range(s.ny):
            for b in range(s.nu):
                ra = degree(s.num[a][b]) - degree(s.den[a][b])
                if ra > 0:
                    d = np.array(s.den[a][b])
                    for _ in range(ra):
                        d = np.convolve(d, [tau, 1.0])
                    p = path + "/den" + ("" if not s.nested else f"/{a}/{b}")
                    patch.append({"op": "replace", "path": p, "value": [float(x) for x in d]})
        rem.append(C.patch_remedy("add_filter_pole", f"在分母乘上 (τs+1)^r，τ = {tau:g} s（远高于关心频带时影响可忽略），使系统 proper",
                                  patch, confidence="medium", confirm=True))
        if s.siso:
            rem.append(C.patch_remedy("swap_num_den", "若 num/den 写反，交换二者",
                                      [{"op": "replace", "path": path + "/num", "value": s.den[0][0]},
                                       {"op": "replace", "path": path + "/den", "value": s.num[0][0]}],
                                      confidence="low", confirm=True))
    elif s.kind == "zpk" and not s.discrete:
        newp = [[float(x.real), float(x.imag)] for x in s.p] + [[-1.0 / tau, 0.0]] * r
        rem.append(C.patch_remedy("add_filter_pole", f"补 {r} 个 s = −1/τ（τ = {tau:g} s）的极点，k 乘以 (1/τ)^{r} 保持低频增益",
                                  [{"op": "replace", "path": path + "/p", "value": newp},
                                   {"op": "replace", "path": path + "/k", "value": s.k * (1.0 / tau) ** r}],
                                  confidence="medium", confirm=True))
    rem.append(C.ask_user("向用户确认系统模型是否为理想微分环节或输入有误"))
    where = "" if s.siso else f"（通道 [{i}][{j}]）"
    raise Fail("E.DOMAIN.IMPROPER_SYSTEM",
               f"系统不是 proper{where}：分子阶次 {dn} > 分母阶次 {dd}，频率响应在高频发散，不存在状态空间实现。",
               path, {"num_degree": dn, "den_degree": dd, "channel": [i, j]}, rem,
               agent_hint="该算子只接受 proper 系统。理想微分环节请补一个远高于关心频带的极点；也检查 num/den 是否颠倒。",
               retryable=True)


# ------------------------------------------------------------------------------------------
# conversions


def tf2ss_siso(num, den):
    """Controllable canonical form (scipy tf2ss convention, R-6): A first row = -den[1:]/den[0], B = e1."""
    d0 = den[0]
    a = [x / d0 for x in den]
    n = len(den) - 1
    b = [0.0] * (n + 1 - len(num)) + [x / d0 for x in num] if num != [0.0] else [0.0] * (n + 1)
    D = b[0]
    A = np.zeros((n, n))
    if n:
        A[0, :] = [-x for x in a[1:]]
        for i in range(1, n):
            A[i, i - 1] = 1.0
    B = np.zeros((n, 1))
    if n:
        B[0, 0] = 1.0
    Cm = np.array([[b[k + 1] - b[0] * a[k + 1] for k in range(n)]]) if n else np.zeros((1, 0))
    return A, B, Cm, np.array([[D]])


def to_ss(s):
    """Return (A, B, C, D, realization) for any representation (proper systems only)."""
    if s.kind == "ss":
        return s.A, s.B, s.C, s.D, "identity"
    if s.kind == "zpk":
        num, den = zpk2tf(s.z, s.p, s.k)
        A, B, Cm, D = tf2ss_siso(num, den)
        return A, B, Cm, D, "controllable_canonical"
    if s.siso:
        A, B, Cm, D = tf2ss_siso(s.num[0][0], s.den[0][0])
        return A, B, Cm, D, "controllable_canonical"
    # MIMO tf: one controllable canonical block per element, element (i, j) in row-major order
    blocks = [[tf2ss_siso(s.num[i][j], s.den[i][j]) for j in range(s.nu)] for i in range(s.ny)]
    n = sum(blk[0].shape[0] for row in blocks for blk in row)
    A = np.zeros((n, n))
    B = np.zeros((n, s.nu))
    Cm = np.zeros((s.ny, n))
    D = np.zeros((s.ny, s.nu))
    o = 0
    for i in range(s.ny):
        for j in range(s.nu):
            a, b, c, d = blocks[i][j]
            k = a.shape[0]
            A[o:o + k, o:o + k] = a
            B[o:o + k, j] = b[:, 0]
            Cm[i, o:o + k] = c[0]
            D[i, j] = d[0, 0]
            o += k
    return A, B, Cm, D, "block_diagonal_controllable_canonical"


def ss2tf(A, B, Cm, D):
    """R-7: den = det(sI - A) from the eigenvalues; num_ij = det(sI - A + b_j c_i) + (d_ij - 1) den;
    leading coefficient set exactly to d_ij, then exact leading zeros removed. Shared denominator."""
    n = A.shape[0]
    den = K.poly_from_roots(K.eigvals(A)) if n else np.array([1.0])
    ny, nu = D.shape
    num = [[None] * nu for _ in range(ny)]
    for i in range(ny):
        for j in range(nu):
            if n:
                cz = K.poly_from_roots(K.eigvals(A - np.outer(B[:, j], Cm[i, :])))
                c = cz + (D[i, j] - 1.0) * den
                c[0] = D[i, j]
            else:
                c = np.array([D[i, j]])
            c, _ = K.strip_leading_zeros(list(c))
            num[i][j] = c
    return num, [[list(den) for _ in range(nu)] for _ in range(ny)]


def zpk2tf(z, p, k):
    """Expanded product: num = k * prod(s - z), den = prod(s - p) (monic)."""
    num = k * K.poly_from_roots(z)
    den = K.poly_from_roots(p)
    num, _ = K.strip_leading_zeros(list(num))
    return num, [float(x) for x in den]


def tf2zpk(num, den):
    """Polynomial roots (balanced companion + Newton); k = num[0] / den[0]."""
    if num == [0.0]:
        return np.zeros(0, dtype=complex), _roots(den), 0.0
    return _roots(num), _roots(den), num[0] / den[0]


def _roots(c):
    """Roots of a coefficient list, trailing zero coefficients giving exact zero roots."""
    k = 0
    while k < len(c) - 1 and c[len(c) - 1 - k] == 0.0:
        k += 1
    q = c[:len(c) - k]
    r = list(K.poly_roots(q)) if len(q) > 1 else []
    r += [0j] * k
    return K.sort_complex(np.array(r, dtype=complex))[0]


def markov_scale_tol(n):
    return 1e3 * max(n, 1) * C.U


def ss2zpk(A, B, Cm, D):
    """SISO: p = eig(A); D != 0: z = eig(A - B C / d), k = d; D = 0: relative degree r from the first
    Markov parameter C A^{r-1} B that is not negligible (|h| > 1e3 n u ||C|| ||A^{r-1}B||),
    z = eigenvalues of the zero dynamics on ker [C; CA; ...; CA^{r-1}], k = h_r (R-8).
    Transfer function identically zero: k = 0 and no zeros (10.4)."""
    n = A.shape[0]
    p = K.sort_complex(K.eigvals(A))[0] if n else np.zeros(0, dtype=complex)
    d = float(D[0, 0])
    b = B[:, 0]
    c = Cm[0, :]
    if d != 0.0:
        z = K.eigvals(A - np.outer(b, c) / d) if n else np.zeros(0, dtype=complex)
        return K.sort_complex(z)[0], p, d, {"relative_degree": 0}
    v = b.copy()
    rows = []
    h = 0.0
    r = None
    cn = float(np.linalg.norm(c))
    for i in range(1, n + 1):
        h = float(c @ v)
        rows.append(c @ K.matpow(A, i - 1))
        if abs(h) > markov_scale_tol(n) * cn * float(np.linalg.norm(v)) and h != 0.0:
            r = i
            break
        v = A @ v
    if r is None:
        return np.zeros(0, dtype=complex), p, 0.0, {"relative_degree": None, "zero_transfer_function": True}
    if r == n:
        return np.zeros(0, dtype=complex), p, h, {"relative_degree": r}
    O = np.array(rows)                       # r x n, full row rank
    Q = _householder_q(O.T)                  # n x n orthogonal, first r columns span range(O^T)
    N = Q[:, r:]
    car = c @ K.matpow(A, r)
    Abar = A - np.outer(b, car) / h
    Az = N.T @ Abar @ N
    z = K.eigvals(Az)
    return K.sort_complex(z)[0], p, h, {"relative_degree": r}


def _householder_q(M):
    """Full orthogonal Q of the Householder QR of M (n x r)."""
    M = np.array(M, dtype=float, copy=True)
    n, r = M.shape
    Q = np.eye(n)
    for k in range(min(r, n - 1)):
        x = M[k:, k].copy()
        nx = float(np.linalg.norm(x))
        if nx == 0.0:
            continue
        alpha = -math.copysign(nx, x[0]) if x[0] != 0 else -nx
        v = x
        v[0] -= alpha
        nv = float(np.linalg.norm(v))
        if nv == 0.0:
            continue
        v /= nv
        M[k:, :] -= 2.0 * np.outer(v, v @ M[k:, :])
        Q[:, k:] -= 2.0 * np.outer(Q[:, k:] @ v, v)
    return Q


def poles(s):
    if s.kind == "zpk":
        return K.sort_complex(s.p)[0]
    if s.kind == "ss":
        return K.sort_complex(K.eigvals(s.A))[0] if s.A.shape[0] else np.zeros(0, dtype=complex)
    out = []
    seen = []
    for i in range(s.ny):
        for j in range(s.nu):
            d = s.den[i][j]
            if any(d == x for x in seen):
                continue
            seen.append(d)
            out.extend(list(_roots(d)) if len(d) > 1 else [])
    return K.sort_complex(np.array(out, dtype=complex))[0]


# ------------------------------------------------------------------------------------------
# frequency response


def eval_points(s, w):
    """s = j w (continuous) or z = e^{j w ts} (discrete)."""
    w = np.asarray(w, dtype=float)
    if s.discrete:
        th = w * s.ts
        return np.cos(th) + 1j * np.sin(th)
    return 1j * w


def freqresp(s, pts, perturb=None):
    """G at the complex points `pts`: returns (G [ny, nu, K], rel_err_estimate [K], pole_hit [K], info).

    tf: Horner on numerator and denominator with Higham's running bound gamma_{4d+2} sum|a_k||s|^k / |p(s)|
        (plus the effect of the rounding of s itself, |p'(s)| |ds| / |p(s)|);
    zpk: product form k prod(s - z) / prod(s - p);
    ss: (sI - A) X = B by complex LU with partial pivoting per point, G = C X + D; heuristic estimate
        n u cond_1(sI - A)."""
    pts = np.asarray(pts, dtype=complex)
    Kp = len(pts)
    G = np.zeros((s.ny, s.nu, Kp), dtype=complex)
    rel = np.zeros(Kp)
    hit = np.zeros(Kp, dtype=bool)
    ds = np.zeros(Kp) if perturb is None else np.asarray(perturb, dtype=float)
    info = {}
    if s.kind == "tf":
        a = np.abs(pts)
        for i in range(s.ny):
            for j in range(s.nu):
                nm, dn = s.num[i][j], s.den[i][j]
                nv = K.polyval_c(nm, pts)
                dv = K.polyval_c(dn, pts)
                z = dv == 0
                hit |= z
                with np.errstate(divide="ignore", invalid="ignore"):
                    G[i, j] = np.where(z, np.nan, nv / np.where(z, 1.0, dv))
                    en = K.gamma(4 * len(nm) + 2) * K.polyval_abs(nm, a) + np.abs(K.polyval_c(K.polyder(nm), pts)) * ds
                    ed = K.gamma(4 * len(dn) + 2) * K.polyval_abs(dn, a) + np.abs(K.polyval_c(K.polyder(dn), pts)) * ds
                    rn = np.where(nv == 0, 0.0, en / np.abs(nv))
                    rd = np.where(z, np.inf, ed / np.abs(np.where(z, 1.0, dv)))
                rel = np.maximum(rel, rn + rd)
        info["representation"] = "tf"
    elif s.kind == "zpk":
        num = np.full(Kp, s.k, dtype=complex)
        den = np.ones(Kp, dtype=complex)
        for z in s.z:
            num = num * (pts - z)
        for p in s.p:
            f = pts - p
            hit |= f == 0
            den = den * f
        with np.errstate(divide="ignore", invalid="ignore"):
            G[0, 0] = np.where(hit, np.nan, num / np.where(hit, 1.0, den))
        nz = len(s.z) + len(s.p)
        rel[:] = K.gamma(4 * nz + 4)
        # rounding of s itself: sum over factors |ds| / |s - r|
        with np.errstate(divide="ignore", invalid="ignore"):
            for r in list(s.z) + list(s.p):
                rel += np.where(np.abs(pts - r) > 0, ds / np.abs(pts - r), np.inf)
        rel[hit] = np.inf
        info["representation"] = "zpk"
    else:
        n = s.A.shape[0]
        if n == 0:
            G[:] = s.D[:, :, None]
        else:
            M = pts[:, None, None] * np.eye(n)[None, :, :] - s.A[None, :, :]
            X, sing = K.lu_solve_batch(M, np.broadcast_to(s.B.astype(complex), (Kp, n, s.nu)))
            hit |= sing
            G = np.einsum("in,knj->ijk", s.C.astype(complex), X) + s.D[:, :, None]
            if n <= 64:
                n1 = np.max(np.sum(np.abs(M), axis=1), axis=1)
                kappa = n1 * K.inv_norm1_batch(M)
                rel = n * C.U * kappa + ds * kappa / np.maximum(n1, np.finfo(float).tiny)
                info["max_cond1_sI_minus_A"] = float(np.max(kappa[np.isfinite(kappa)])) if np.any(np.isfinite(kappa)) else None
            else:
                rel[:] = math.nan
            rel[hit] = np.inf
        info["representation"] = "ss"
    return G, rel, hit, info


# ------------------------------------------------------------------------------------------
# DC gain and stability (D-2)


def dcgain(s):
    """Return (G0 [ny, nu] float with +-Inf / NaN per D-2, ok_mask)."""
    if s.kind == "tf":
        g = np.zeros((s.ny, s.nu))
        for i in range(s.ny):
            for j in range(s.nu):
                nm, dn = s.num[i][j], s.den[i][j]
                if s.discrete:
                    nv, dv = math.fsum(nm), math.fsum(dn)
                else:
                    nv, dv = nm[-1], dn[-1]
                if dv == 0.0:
                    g[i, j] = math.nan if nv == 0.0 else math.copysign(math.inf, nv)
                else:
                    g[i, j] = nv / dv
        return g
    if s.kind == "zpk":
        x = 1.0 if s.discrete else 0.0
        nv = s.k * np.prod([x - z for z in s.z]) if len(s.z) else complex(s.k)
        dv = np.prod([x - p for p in s.p]) if len(s.p) else 1.0 + 0j
        if dv == 0:
            return np.array([[math.nan if nv == 0 else math.copysign(math.inf, nv.real)]])
        return np.array([[float((nv / dv).real)]])
    n = s.A.shape[0]
    if n == 0:
        return s.D.copy()
    M = (np.eye(n) - s.A) if s.discrete else -s.A
    try:
        X = K.lu_solve(M, s.B)
    except K.Singular:
        return np.full((s.ny, s.nu), math.nan)
    return s.C @ X + s.D


def asymptotically_stable(pole_list, discrete, scale):
    """Continuous: Re(l) < 0; discrete: |l| < 1. Values within 100 u * scale of the boundary count as on it."""
    tol = 100 * C.U * max(scale, 1.0)
    for l in pole_list:
        if discrete:
            if abs(l) >= 1.0 - tol:
                return False
        elif l.real >= -tol:
            return False
    return True


# ------------------------------------------------------------------------------------------
# output


def system_json(kind, ts, nested=False, num=None, den=None, A=None, B=None, Cm=None, D=None, z=None, p=None, k=None):
    """R-5: a complete LtiSystem object usable as the next operator's input."""
    if kind == "tf":
        if nested:
            nj = [[[float(x) for x in e] for e in row] for row in num]
            dj = [[[float(x) for x in e] for e in row] for row in den]
        else:
            nj = [float(x) for x in num[0][0]]
            dj = [float(x) for x in den[0][0]]
        return {"kind": "tf", "num": nj, "den": dj, "ts": ts, "time_unit": "s"}
    if kind == "ss":
        return {"kind": "ss", "A": C.dense_f64(A), "B": C.dense_f64(B), "C": C.dense_f64(Cm), "D": C.dense_f64(D),
                "ts": ts, "time_unit": "s"}
    return {"kind": "zpk", "z": [[float(x.real), float(x.imag)] for x in z], "p": [[float(x.real), float(x.imag)] for x in p],
            "k": float(k), "ts": ts, "time_unit": "s"}


LTI_ERRORS = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.ZERO_DENOMINATOR", "E.VALIDATION.NOT_CONJUGATE", "E.VALIDATION.DIGEST_MISMATCH",
              "E.VALIDATION.REF_UNSUPPORTED", "E.VALIDATION.REF_FORBIDDEN", "E.VALIDATION.METHOD_UNAVAILABLE",
              "E.RESOURCE.INPUT_TOO_LARGE", "E.DOMAIN.IMPROPER_SYSTEM", "E.INTERNAL.PANIC"]

LTI_ASSUMPTIONS = [
    {"id": "lti_model", "statement": "输入系统准确描述被研究对象（单位、符号、row-major 录入）", "enforced_by": "caller"},
    {"id": "proper", "statement": "系统为 proper（deg num ≤ deg den；zpk 零点数 ≤ 极点数）", "enforced_by": "error:E.DOMAIN.IMPROPER_SYSTEM"},
    {"id": "finite", "statement": "所有输入数值有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
    {"id": "conjugate", "statement": "zpk 的复零极点共轭成对（相对容差 1e-12）", "enforced_by": "error:E.VALIDATION.NOT_CONJUGATE"},
]

LTI_REFERENCES = [
    {"type": "textbook", "cite": "T. Kailath, Linear Systems, Prentice-Hall 1980（实现、标准型、传递函数）"},
    {"type": "textbook", "cite": "J. H. Wilkinson, C. Reinsch, Handbook for Automatic Computation II: Linear Algebra, Springer 1971（balance、Hessenberg、hqr）",
     "doi": "10.1007/978-3-642-86940-2"},
    {"type": "paper", "cite": "B. N. Parlett, C. Reinsch, Balancing a matrix for calculation of eigenvalues and eigenvectors, Numer. Math. 13, 1969",
     "doi": "10.1007/BF02165404"},
]
