"""interp.grid.interp1 (profile 10.20 IP-1 .. IP-7), written from the profile text.

linear / nearest / previous / next / pchip are this module's own code (plain float64 arithmetic in
the order the profile prescribes); the spline's tridiagonal slope system is solved with LAPACK dgtsv
(scipy.linalg.lapack), as IP-2 names. Cubic pieces are evaluated as ((c0*d + c1)*d + c2)*d + c3.
"""
import math

import numpy as np
from scipy.linalg import lapack

import aeoi_common as C
from aeoi_common import Fail

OP_ID = "interp.grid.interp1"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:interp1:piecewise+dgtsv-spline"
MAX_POINTS = 10_000_000
EXTRAP_PATCHES = [("nan", "范围外给 NaN（标成缺失）", "medium"), ("clamp", "范围外取最近端点的值", "medium"),
                  ("extrapolate", "把端点那一段（直线或三次多项式）延长；nearest/previous/next 取端点值", "low")]


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.GRID", "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED",
              "E.VALIDATION.REF_FORBIDDEN", "E.VALIDATION.METHOD_UNAVAILABLE", "E.DOMAIN.EXTRAPOLATION_REQUIRED",
              "E.RESOURCE.INPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="1-D interpolation (Python second implementation)",
        description="一维插值（= MATLAB interp1 的 linear、nearest、previous、next、pchip、spline）。"
                    "查询点超出数据范围时缺省报错（E.DOMAIN.EXTRAPOLATION_REQUIRED），由调用方用 extrapolation 选 nan、clamp 或 extrapolate。",
        category="analysis", tags=["interp", "interp1"],
        applicability={"preconditions": ["x 为 [n]，严格递增，n ≥ 2", "y 为 [n] 或 [k, n]（每行一个通道）", "xq 为 [q]",
                                         "全部数值有限"],
                       "not_for": ["多维插值", "非单调 x（先排序）", "散乱数据拟合"], "siso_only": False},
        provenance={"algorithm": "区间定位 x_i ≤ xq < x_{i+1}（xq = x_{n−1} 取最后一段），节点命中直接返回 y；linear 为 slope·(xq − x_i) + y_i；"
                                 "nearest 以 x_i/2 + x_{i+1}/2 为界、中点归右；pchip 为 Fritsch–Butland 加权调和平均斜率 + 保形三点端点公式；"
                                 "spline 为 not-a-knot 三次样条，斜率方程三对角，用 LAPACK dgtsv 求解（n = 3 抛物线，n = 2 直线）；"
                                 "三次段按 ((c0·d + c1)·d + c2)·d + c3 求值；范围外按 extrapolation 处理。",
                    "references": [
                        {"type": "paper", "cite": "F. N. Fritsch, R. E. Carlson, Monotone piecewise cubic interpolation, SIAM J. Numer. Anal. 17 (1980) 238–246", "doi": "10.1137/0717021"},
                        {"type": "paper", "cite": "F. N. Fritsch, J. Butland, A method for constructing local monotone piecewise cubic interpolants, SIAM J. Sci. Stat. Comput. 5 (1984) 300–304", "doi": "10.1137/0905021"},
                        {"type": "textbook", "cite": "C. de Boor, A Practical Guide to Splines, rev. ed., Springer 2001, ch. IV (not-a-knot)"},
                        {"type": "textbook", "cite": "C. Moler, Numerical Computing with MATLAB, SIAM 2004, ch. 3 (pchip, spline)", "doi": "10.1137/1.9780898717952"}]},
        backend={"library": "aeoi-pyscipy op_interp1 (Python + numpy) + scipy LAPACK dgtsv", "routine": "op_interp1.Interp1.execute",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_points": MAX_POINTS, "max_queries": MAX_POINTS},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=["W.ACCURACY.EXTRAPOLATED"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "grid", "statement": "x 严格递增、n ≥ 2", "enforced_by": "error:E.VALIDATION.GRID"},
                     {"id": "in_range", "statement": "查询点在数据范围内，或调用方用 extrapolation 指定范围外的处理",
                      "enforced_by": "error:E.DOMAIN.EXTRAPOLATION_REQUIRED"},
                     {"id": "extrapolated", "statement": "外推得到的值有警告", "enforced_by": "warning:W.ACCURACY.EXTRAPOLATED"},
                     {"id": "finite", "statement": "全部输入数值有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "model", "statement": "所选方法适合数据（插值误差相对未知函数无法估计）", "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "none"},
                 "postconditions": ["pieces_interpolate", "spline_c2", "local_bounds", "all_finite"],
                 "guards": {"pieces_interpolate": ["/yq"], "spline_c2": ["/yq"], "local_bounds": ["/yq"], "all_finite": ["/yq"]}},
    )


def _pchip_slopes(x, y):
    h = np.diff(x)
    d = np.diff(y) / h
    n = len(x)
    s = np.zeros(n)
    if n == 2:
        s[:] = d[0]
        return s
    for k in range(1, n - 1):
        if d[k - 1] == 0.0 or d[k] == 0.0 or (d[k - 1] > 0) != (d[k] > 0):
            s[k] = 0.0
        else:
            w1 = 2 * h[k] + h[k - 1]
            w2 = h[k] + 2 * h[k - 1]
            s[k] = (w1 + w2) / (w1 / d[k - 1] + w2 / d[k])

    def edge(h0, h1, d0, d1):
        v = ((2 * h0 + h1) * d0 - h0 * d1) / (h0 + h1)
        if np.sign(v) != np.sign(d0):
            return 0.0
        if np.sign(d0) != np.sign(d1) and abs(v) > abs(3 * d0):
            return 3 * d0
        return v
    s[0] = edge(h[0], h[1], d[0], d[1])
    s[-1] = edge(h[-1], h[-2], d[-1], d[-2])
    return s


def _spline_slopes(x, y):
    n = len(x)
    h = np.diff(x)
    d = np.diff(y) / h
    if n == 2:
        return np.array([d[0], d[0]])
    if n == 3:
        # not-a-knot with three points = the parabola through them
        dl = np.array([h[1], 1.0])
        dd = np.array([1.0, 2 * (h[0] + h[1]), 1.0])
        du = np.array([1.0, h[0]])
        rhs = np.array([2 * d[0], 3 * (h[1] * d[0] + h[0] * d[1]), 2 * d[1]])
    else:
        dd = np.empty(n)
        du = np.empty(n - 1)
        dl = np.empty(n - 1)
        rhs = np.empty(n)
        w = x[2] - x[0]
        dd[0], du[0] = h[1], w
        rhs[0] = ((h[0] + 2 * w) * h[1] * d[0] + h[0] ** 2 * d[1]) / w
        dd[1:-1] = 2 * (h[:-1] + h[1:])
        du[1:] = h[:-1]
        dl[:-1] = h[1:]
        rhs[1:-1] = 3 * (h[1:] * d[:-1] + h[:-1] * d[1:])
        w = x[-1] - x[-3]
        dd[-1], dl[-1] = h[-2], w
        rhs[-1] = (h[-1] ** 2 * d[-2] + (2 * w + h[-1]) * h[-2] * d[-1]) / w
    _, _, _, sol, info = lapack.dgtsv(dl, dd, du, rhs)
    if info != 0:
        raise RuntimeError(f"dgtsv info = {info} on a strictly increasing grid")
    return np.asarray(sol, dtype=float)


def _hermite_coeffs(x, y, s):
    h = np.diff(x)
    d = np.diff(y) / h
    t = (s[:-1] + s[1:] - 2 * d) / h
    c0 = t / h
    c1 = (d - s[:-1]) / h - t
    return c0, c1, s[:-1].copy(), y[:-1].copy()


class Interp1(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        xa = C.read_array(args["x"], "/x", allowed=("f64", "i64"), max_elements=MAX_POINTS)
        ya = C.read_array(args["y"], "/y", allowed=("f64", "i64"), max_elements=MAX_POINTS)
        qa = C.read_array(args["xq"], "/xq", allowed=("f64", "i64"), max_elements=MAX_POINTS)
        if len(xa.shape) != 1:
            raise Fail("E.VALIDATION.SHAPE", f"x 必须是一维 [n]，这里 shape = {xa.shape}", "/x/shape", {"shape": xa.shape})
        n = xa.shape[0]
        if n < 2:
            raise Fail("E.VALIDATION.SHAPE", f"x 至少要有 2 个点，这里 n = {n}（IP-1）", "/x/shape", {"n": n},
                       [C.ask_user("插值至少需要两个数据点")])
        x = xa.values.astype(float)
        dx = np.diff(x)
        if np.any(dx <= 0):
            i = int(np.flatnonzero(dx <= 0)[0]) + 1
            dec = bool(np.all(dx < 0))
            msg = f"x 必须严格递增：x[{i}] = {float(x[i])!r} ≤ x[{i - 1}] = {float(x[i - 1])!r}（IP-1）"
            rem = []
            if dec:
                msg += "；x 整体递减，请把 x 和 y 一起倒序"
                rem.append(C.patch_remedy("reverse_xy", "把 x 与 y 一起倒序（数据不变）", _reverse_patch(args, xa, ya), confidence="high",
                                          confirm=False) if _reverse_patch(args, xa, ya) else None)
            rem = [r for r in rem if r] + [C.ask_user("检查 x 是否有重复或顺序错误（插值要求严格递增）")]
            raise Fail("E.VALIDATION.GRID", msg, f"/x/data/{i}", {"index": i, "decreasing": dec}, rem,
                       agent_hint="x 递减时把 x 和 y 一起倒序；有重复点时先合并或去掉")
        if len(ya.shape) == 1:
            if ya.shape[0] != n:
                raise Fail("E.VALIDATION.SHAPE", f"y 的长度 {ya.shape[0]} 与 x 的长度 {n} 不同", "/y/shape",
                           {"x_length": n, "y_shape": ya.shape}, [C.ask_user("核对 x 与 y 的点数")])
            Y = ya.values.astype(float).reshape(1, n)
        elif len(ya.shape) == 2:
            if ya.shape[1] != n:
                hint = "；看起来是按列存的 [n, k]，请转置为 [k, n]（每行一个通道）" if ya.shape[0] == n else ""
                rem = []
                if ya.shape[0] == n and isinstance(args["y"], dict) and "data" in args["y"]:
                    T = ya.values.astype(float).T
                    rem.append(C.patch_remedy("transpose_y", "把 y 转置为 [k, n]（每行一个通道）",
                                              [{"op": "replace", "path": "/y", "value": C.dense_f64(T)}], confidence="medium", confirm=True))
                rem.append(C.ask_user("确认 y 的布局：每行一个通道 [k, n]"))
                raise Fail("E.VALIDATION.SHAPE", f"y 的形状 {ya.shape} 与 x 的长度 {n} 不符，应为 [k, {n}]{hint}（IP-1）", "/y/shape",
                           {"x_length": n, "y_shape": ya.shape}, rem)
            Y = ya.values.astype(float)
        else:
            raise Fail("E.VALIDATION.SHAPE", f"y 必须是 [n] 或 [k, n]，这里 shape = {ya.shape}", "/y/shape", {"y_shape": ya.shape})
        if len(qa.shape) != 1:
            raise Fail("E.VALIDATION.SHAPE", f"xq 必须是一维 [q]，这里 shape = {qa.shape}", "/xq/shape", {"shape": qa.shape},
                       [C.ask_user("把查询点展平成一维 [q]")])
        xq = qa.values.astype(float)
        method = args.get("method", "linear")
        extrap = args.get("extrapolation", "error")
        outside = (xq < x[0]) | (xq > x[-1])
        cnt = int(np.sum(outside))
        if cnt and extrap == "error":
            first = int(np.flatnonzero(outside)[0])
            lo, hi = float(np.min(xq)), float(np.max(xq))
            raise Fail("E.DOMAIN.EXTRAPOLATION_REQUIRED",
                       f"{cnt} 个查询点在数据范围 [{float(x[0])!r}, {float(x[-1])!r}] 之外（第一个是 xq[{first}] = {float(xq[first])!r}；xq 范围 [{lo!r}, {hi!r}]）。"
                       "范围外要不要给值、怎么给，需要调用方决定（IP-3）。",
                       f"/xq/data/{first}",
                       {"count": cnt, "first_index": first, "first_value": float(xq[first]), "x_range": [float(x[0]), float(x[-1])],
                        "xq_range": [lo, hi]},
                       [C.patch_remedy(f"extrapolation_{v}", desc, [{"op": "add", "path": "/extrapolation", "value": v}],
                                       confidence=conf, confirm=True) for v, desc, conf in EXTRAP_PATCHES],
                       agent_hint="问用户范围外要哪种处理，再加上 extrapolation（nan / clamp / extrapolate）", retryable=True)
        return dict(x=x, Y=Y, xq=xq, vec=len(ya.shape) == 1, method=method, extrap=extrap, outside=outside, count=cnt)

    def estimate(self, P):
        return {"n": len(P["x"]), "channels": P["Y"].shape[0], "queries": len(P["xq"]), "out_of_range_count": P["count"]}

    def execute(self, P):
        x, Y, xq, method, extrap = P["x"], P["Y"], P["xq"], P["method"], P["extrap"]
        n = len(x)
        k = Y.shape[0]
        q = len(xq)
        # interval index: x_i <= xq < x_{i+1}; xq = x_{n-1} -> last piece; outside -> end pieces
        idx = np.searchsorted(x, xq, side="right") - 1
        idx = np.clip(idx, 0, n - 2)
        node = np.searchsorted(x, xq, side="left")
        node_hit = (node < n) & (x[np.minimum(node, n - 1)] == xq)
        node_idx = np.minimum(node, n - 1)
        below = xq < x[0]
        above = xq > x[-1]
        out = np.empty((k, q))
        cubic = method in ("pchip", "spline")
        piece_rel = 0.0
        c2_rel = 0.0
        lb_viol = 0
        for ch in range(k):
            y = Y[ch]
            xi = x[idx]
            xi1 = x[idx + 1]
            yi = y[idx]
            yi1 = y[idx + 1]
            if method == "linear":
                slope = (yi1 - yi) / (xi1 - xi)
                v = slope * (xq - xi) + yi
            elif method == "nearest":
                v = np.where(xq >= xi / 2 + xi1 / 2, yi1, yi)
            elif method == "previous":
                v = yi.copy()
            elif method == "next":
                v = np.where(xq > xi, yi1, yi)
            else:
                s = _pchip_slopes(x, y) if method == "pchip" else _spline_slopes(x, y)
                c0, c1, c2, c3 = _hermite_coeffs(x, y, s)
                dd = xq - xi
                v = ((c0[idx] * dd + c1[idx]) * dd + c2[idx]) * dd + c3[idx]
                # IP-5 pieces_interpolate: each piece at its right end vs the data
                h = np.diff(x)
                end = ((c0 * h + c1) * h + c2) * h + c3
                scale = np.abs(y[:-1]) + np.abs(y[1:]) + (np.abs(s[:-1]) + np.abs(s[1:])) * h
                rel = np.abs(end - y[1:]) / np.where(scale > 0, scale, 1.0)
                piece_rel = max(piece_rel, float(np.max(rel)))
                if method == "spline" and n >= 3:
                    left2 = 6 * c0[:-1] * h[:-1] + 2 * c1[:-1]     # second derivative at the right end of piece i
                    right2 = 2 * c1[1:]                             # at the left end of piece i+1
                    gscale = max(float(np.max(np.abs(np.concatenate([left2, right2])))),
                                 float(np.max(np.abs(y))) / float(np.max(h)) ** 2, np.finfo(float).tiny)
                    c2_rel = max(c2_rel, float(np.max(np.abs(left2 - right2))) / gscale)
            # IP-2: exact node hits return the data value, for every method
            v = np.where(node_hit, y[node_idx], v)
            # IP-3 outside the range
            if extrap == "nan":
                v = np.where(below | above, np.nan, v)
            elif extrap == "clamp":
                v = np.where(below, y[0], np.where(above, y[-1], v))
            elif extrap == "extrapolate" and method in ("nearest", "previous", "next"):
                v = np.where(below, y[0], np.where(above, y[-1], v))
            # IP-5 local_bounds (in range points; not for spline)
            if method != "spline":
                inside = ~(below | above)
                lo = np.minimum(yi, yi1)
                hi = np.maximum(yi, yi1)
                slack = 4 * C.U * np.maximum(np.abs(lo), np.abs(hi))
                bad = inside & ((v < lo - slack) | (v > hi + slack))
                lb_viol += int(np.sum(bad))
            out[ch] = v
        intended_nan = (below | above) if extrap == "nan" else np.zeros(q, bool)
        finite = bool(np.all(np.isfinite(out[:, ~intended_nan]))) if q else True
        warnings = []
        if extrap == "extrapolate" and P["count"]:
            first = int(np.flatnonzero(below | above)[0])
            warnings.append({"code": "W.ACCURACY.EXTRAPOLATED",
                             "message": f"{P['count']} 个查询点在数据范围之外，结果是外推值（{method}），没有数据依据",
                             "path": f"/xq/data/{first}", "details": {"count": P["count"]}})
        yq = out[0] if P["vec"] else out
        result = {"yq": C.dense_f64(yq), "method": method, "extrapolation": extrap, "out_of_range_count": P["count"]}
        trust = {
            "convergence": {"applicable": False},
            "residual": {"piece_end_rel": piece_rel, "spline_c2_rel": c2_rel,
                         "definition": "piece_end_rel = |p_i(x_{i+1}) − y_{i+1}| / (|y_i| + |y_{i+1}| + (|s_i| + |s_{i+1}|)·h_i)；"
                                       "spline_c2_rel = 内部节点两侧二阶导之差 / max(最大二阶导, max|y|/max(h)²)"},
            "condition": {"n": n, "min_spacing": float(np.min(np.diff(x)))},
            "error_estimate": None,
            "checks": {"pieces_interpolate": bool(piece_rel <= 64 * C.U), "spline_c2": bool(c2_rel <= 6.4e4 * C.U),
                       "local_bounds": bool(lb_viol == 0), "all_finite": finite},
            "accuracy_grade": "C", "credentials": None,
            "derived": {"local_bounds_applies": method != "spline", "local_bounds_violations": lb_viol,
                        "pieces_interpolate_applies": cubic, "spline_c2_applies": method == "spline"},
        }
        return result, trust, warnings


def _reverse_patch(args, xa, ya):
    if not (isinstance(args.get("x"), dict) and "data" in args["x"] and isinstance(args.get("y"), dict) and "data" in args["y"]):
        return None
    xr = xa.values.astype(float)[::-1]
    yv = ya.values.astype(float)
    yr = yv[::-1] if yv.ndim == 1 else yv[:, ::-1]
    return [{"op": "replace", "path": "/x", "value": C.dense_f64(xr)}, {"op": "replace", "path": "/y", "value": C.dense_f64(yr)}]
