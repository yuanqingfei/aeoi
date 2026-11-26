"""calculus.quadrature.integral (profile 10.31 IN-1 .. IN-6), written from the profile text.

The integrand is a 3.15 expression parsed and evaluated by aeoi_expr (no eval). Quadrature is this
module's own globally adaptive Gauss-Kronrod (7, 15): QUADPACK's qk15 error formula, worst-first
bisection, 10 initial pieces, at most 650 subintervals, after the MATLAB-integral variable changes
(Shampine 2008). Subinterval results are summed with math.fsum.
"""
import math

import aeoi_common as C
import aeoi_expr as X
from aeoi_common import Fail

OP_ID = "calculus.quadrature.integral"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:integral:gk15-global-adaptive"
MAX_INTERVALS = 650
INITIAL = 10
MAX_EVALS = 200_000
DEFAULT_REL, DEFAULT_ABS = 1e-10, 1e-12

XGK = (0.991455371120812639206854697526329, 0.949107912342758524526189684047851, 0.864864423359769072789712788640926,
       0.741531185599394439863864773280788, 0.586087235467691130294144845693013, 0.405845151377397166906606412076961,
       0.207784955007898467600689403773245, 0.0)
WGK = (0.022935322010529224963732008058970, 0.063092092629978553290700663189204, 0.104790010322250183839876322541518,
       0.140653259715525918745189590510238, 0.169004726639267902826583426598550, 0.190350578064785409913256402421014,
       0.204432940075298892414161999234649, 0.209482141084727828012999174891714)
WG = (0.129484966168869693270611432679082, 0.279705391489276667901467771423780, 0.381830050505118944950369775488975,
      0.417959183673469387755102040816327)
EPMACH = 2.0 ** -52
UFLOW = 2.2250738585072014e-308


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.EXPRESSION", "E.VALIDATION.RANGE", "E.VALIDATION.METHOD_UNAVAILABLE",
              "E.NUMERICAL.NO_CONVERGENCE", "E.NUMERICAL.NON_FINITE_EVALUATION", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Definite integral of an expression (Python second implementation)",
        description="一元函数的定积分 ∫_a^b f(x) dx（= MATLAB integral），f 用 3.15 表达式写，区间端点可为 \"Inf\"/\"-Inf\"。"
                    "缺省 rel_tol = 1e-10、abs_tol = 1e-12（比 MATLAB 严），停止条件 Σ误差 ≤ max(abs_tol, rel_tol·|I|)。",
        category="analysis", tags=["calculus", "quadrature", "integral"],
        applicability={"preconditions": ["f 是 3.15 表达式，只含积分变量、params 中的参数与 pi", "积分存在（有限）",
                                         "limits 不能两端是同一个无穷"],
                       "not_for": ["发散积分", "主值积分", "多重积分"], "siso_only": False},
        provenance={"algorithm": "全局自适应 Gauss–Kronrod (7, 15)：每次二分误差估计最大的子区间（相等取先出现的），QUADPACK qk15 误差公式；"
                                 "有限区间 x = (a+b)/2 + (b−a)/4·t(3 − t²)，(−∞, ∞) 用 x = t/(1 − t²)，[a, ∞) 用 x = a + t/(1 − t)，"
                                 "(−∞, b] 用 x = b − t/(1 − t)；初始 10 段，至多 650 个子区间；子区间积分值用 math.fsum 求和；"
                                 "收敛后把全部子区间再二分一次，误差估计取 max(Σ 误差估计, 积分值的变化)。",
                    "references": [
                        {"type": "textbook", "cite": "R. Piessens, E. de Doncker-Kapenga, C. W. Überhuber, D. K. Kahaner, QUADPACK: A Subroutine Package for Automatic Integration, Springer 1983", "doi": "10.1007/978-3-642-61786-7"},
                        {"type": "paper", "cite": "L. F. Shampine, Vectorized adaptive quadrature in MATLAB, J. Comput. Appl. Math. 211 (2008) 131–140", "doi": "10.1016/j.cam.2006.11.021"},
                        {"type": "paper", "cite": "A. S. Kronrod, Nodes and Weights of Quadrature Formulas, Consultants Bureau 1965"}]},
        backend={"library": "aeoi-pyscipy op_integral (pure Python, math module)", "routine": "op_integral.Integral.execute",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={},
        encodings={"input": ["json"], "output": ["json"]},
        limits={"max_intervals": MAX_INTERVALS, "initial_intervals": INITIAL, "max_evaluations": MAX_EVALS,
                "max_expression_chars": X.MAX_CHARS, "max_expression_nodes": X.MAX_NODES, "max_expression_depth": X.MAX_DEPTH},
        overridable_tolerances={"rel_tol": DEFAULT_REL, "abs_tol": DEFAULT_ABS},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=[],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "expression", "statement": "表达式只用 3.15 的语法、函数表与声明的名字", "enforced_by": "error:E.VALIDATION.EXPRESSION"},
                     {"id": "finite_values", "statement": "被积函数在求值点上有限", "enforced_by": "error:E.NUMERICAL.NON_FINITE_EVALUATION"},
                     {"id": "converges", "statement": "Σ 误差估计 ≤ max(abs_tol, rel_tol·|I|)", "enforced_by": "error:E.NUMERICAL.NO_CONVERGENCE"},
                     {"id": "refinement", "statement": "再二分一次积分值的变化不超过容差", "enforced_by": "check:refinement_consistent"},
                     {"id": "exists", "statement": "积分存在：有限区间上可积、无穷区间上尾部趋于 0；求积点之间没有未被采到的尖峰", "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["tolerance_met", "refinement_consistent", "all_finite"],
                 "guards": {"tolerance_met": ["/value", "/error_estimate"], "refinement_consistent": ["/value", "/error_estimate"],
                            "all_finite": ["/value"]}},
    )


def _limit(v):
    if isinstance(v, str):
        return math.inf if v == "Inf" else -math.inf
    return float(v)


def _expr_fail(src, path, e):
    return Fail("E.VALIDATION.EXPRESSION", X.caret_message(src, e.position, f"表达式错误（{e.kind}）：{e.message}"), path,
                {"path": path, "expression": src, "position": e.position, "kind": e.kind},
                _expr_remedies(src, path, e),
                agent_hint="按 3.15 改写表达式：乘方写 ^，乘号写 *，只用函数表里的函数")


def _expr_remedies(src, path, e):
    rem = []
    fixed = None
    if e.kind == "non_ascii":
        table = {"×": "*", "·": "*", "÷": "/", "²": "^2", "³": "^3", "−": "-", "π": "pi"}
        if all(ord(c) < 128 or c in table for c in src):
            fixed = "".join(table.get(c, c) for c in src)
    elif "**" in src:
        fixed = src.replace("**", "^")
    if fixed is not None and fixed != src:
        rem.append(C.patch_remedy("rewrite_expression", f"改写为 {fixed}", [{"op": "replace", "path": path, "value": fixed}],
                                  confidence="medium", confirm=True))
    rem.append(C.ask_user("请用户确认被积函数的写法"))
    return rem


class Integral(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        src = args["expression"]
        var = args.get("variable", "x")
        params = args.get("params", {})
        names = [var] + list(params.keys())
        bad = X.check_names(names)
        if bad:
            kind, nm = bad
            path = "/variable" if nm == var and kind == "bad_name" else f"/params/{nm}"
            raise Fail("E.VALIDATION.EXPRESSION", f"名字 {nm!r} 不能用作变量（{kind}：与函数名、pi 或其他变量重名）（EX-2）", path,
                       {"path": path, "expression": src, "position": None, "kind": kind, "name": nm},
                       [C.ask_user("换一个变量名或参数名")])
        try:
            prog = X.compile_expr(src, names)
        except X.ExprError as e:
            raise _expr_fail(src, "/expression", e)
        lim = args["limits"]
        a, b = _limit(lim[0]), _limit(lim[1])
        if math.isinf(a) and a == b:
            raise Fail("E.VALIDATION.RANGE", f"积分区间 [{lim[0]}, {lim[1]}] 两端是同一个无穷，区间为空且没有意义", "/limits",
                       {"limits": lim}, [C.ask_user("确认积分区间")])
        return dict(prog=prog, vals0=[0.0] + [float(params[k]) for k in params], a=a, b=b, limits=lim,
                    rel_tol=float(args.get("rel_tol", DEFAULT_REL)), abs_tol=float(args.get("abs_tol", DEFAULT_ABS)),
                    args=args)

    def estimate(self, P):
        return {"initial_intervals": INITIAL, "evaluations_min": INITIAL * 15, "max_intervals": MAX_INTERVALS}

    def execute(self, P):
        a, b = P["a"], P["b"]
        sign = 1.0
        if a > b:
            a, b, sign = b, a, -1.0
        prog, vals = P["prog"], list(P["vals0"])
        ev = {"n": 0}

        if a == b:
            return self._result(P, 0.0, 0.0, 0.0, 0, 0, 0, "none", 0.0)
        if math.isfinite(a) and math.isfinite(b):
            xc, xq = a / 2 + b / 2, b / 4 - a / 4
            tl, tr, transform = -1.0, 1.0, "x = (a+b)/2 + (b−a)/4·t(3 − t²)"

            def xmap(t):
                return xc + xq * (t * (3.0 - t * t)), 3.0 * xq * ((1.0 - t) * (1.0 + t))
        elif math.isinf(a) and math.isinf(b):
            tl, tr, transform = -1.0, 1.0, "x = t/(1 − t²)"

            def xmap(t):
                d = (1.0 - t) * (1.0 + t)
                return (t / d, (1.0 + t * t) / (d * d)) if d != 0.0 else (math.copysign(math.inf, t), math.inf)
        elif math.isfinite(a):
            tl, tr, transform = 0.0, 1.0, "x = a + t/(1 − t)"

            def xmap(t):
                d = 1.0 - t
                return (a + t / d, 1.0 / (d * d)) if d != 0.0 else (math.inf, math.inf)
        else:
            tl, tr, transform = 0.0, 1.0, "x = b − t/(1 − t)"

            def xmap(t):
                d = 1.0 - t
                return (b - t / d, 1.0 / (d * d)) if d != 0.0 else (-math.inf, math.inf)

        def g(t):
            x, w = xmap(t)
            if not (math.isfinite(x) and math.isfinite(w)):
                return 0.0       # IN-3: an end point that rounds to infinity contributes 0
            vals[0] = x
            fx = X.evaluate(prog, vals)
            ev["n"] += 1
            if not math.isfinite(fx):
                raise Fail("E.NUMERICAL.NON_FINITE_EVALUATION", f"被积函数在 x = {x!r} 处不是有限值（{fx}）（IN-5）", "/expression",
                           {"at": x, "value": str(fx)},
                           [C.ask_user("积分区间内可能有奇点或定义域问题：在奇点处分段，或检查表达式")],
                           agent_hint="在奇点处把区间分开，或确认表达式与区间")
            y = fx * w
            if not math.isfinite(y):
                raise Fail("E.NUMERICAL.NON_FINITE_EVALUATION", f"被积函数在 x = {x!r} 处乘以变量替换的导数后溢出", "/expression",
                           {"at": x}, [C.ask_user("在奇点处分段，或检查表达式")])
            return y

        def qk15(lo, hi):
            c = lo / 2 + hi / 2
            h = hi / 2 - lo / 2
            fc = g(c)
            resg = fc * WG[3]
            resk = fc * WGK[7]
            resabs = abs(resk)
            fv1, fv2 = [0.0] * 7, [0.0] * 7
            for j in range(3):
                jt = 2 * j + 1
                dx = h * XGK[jt]
                f1, f2 = g(c - dx), g(c + dx)
                fv1[jt], fv2[jt] = f1, f2
                resg += WG[j] * (f1 + f2)
                resk += WGK[jt] * (f1 + f2)
                resabs += WGK[jt] * (abs(f1) + abs(f2))
            for j in range(4):
                jt = 2 * j
                dx = h * XGK[jt]
                f1, f2 = g(c - dx), g(c + dx)
                fv1[jt], fv2[jt] = f1, f2
                resk += WGK[jt] * (f1 + f2)
                resabs += WGK[jt] * (abs(f1) + abs(f2))
            reskh = resk * 0.5
            resasc = WGK[7] * abs(fc - reskh)
            for j in range(7):
                resasc += WGK[j] * (abs(fv1[j] - reskh) + abs(fv2[j] - reskh))
            ah = abs(h)
            result = resk * h
            resabs *= ah
            resasc *= ah
            err = abs((resk - resg) * h)
            if resasc != 0.0 and err != 0.0:
                err = resasc * min(1.0, (200.0 * err / resasc) ** 1.5)
            if resabs > UFLOW / (50.0 * EPMACH):
                err = max(EPMACH * 50.0 * resabs, err)
            return result, err

        rel_tol, abs_tol = P["rel_tol"], P["abs_tol"]
        iv = []
        for i in range(INITIAL):
            lo = tl + (tr - tl) * i / INITIAL
            hi = tl + (tr - tl) * (i + 1) / INITIAL
            r, e = qk15(lo, hi)
            iv.append([lo, hi, r, e])
        bisections = 0
        while True:
            I = math.fsum(v[2] for v in iv)
            E = math.fsum(v[3] for v in iv)
            tol = max(abs_tol, rel_tol * abs(I))
            if E <= tol:
                break
            k = max(range(len(iv)), key=lambda i: (iv[i][3], -i))
            lo, hi = iv[k][0], iv[k][1]
            mid = lo / 2 + hi / 2
            reason = None
            if len(iv) >= MAX_INTERVALS:
                reason = f"子区间数达到上限 {MAX_INTERVALS}"
            elif not (lo < mid < hi):
                reason = "误差最大的子区间已经小到无法再二分"
            elif ev["n"] > MAX_EVALS:
                reason = f"求值次数超过 {MAX_EVALS}"
            if reason:
                xl, xr = xmap(lo)[0], xmap(hi)[0]
                p = P["args"]
                rem = []
                if rel_tol < 1e-6:
                    rem.append(C.patch_remedy("relax_rel_tol", "把 rel_tol 放宽到 1e-6（MATLAB 的缺省值）；精度要求是用户的决定",
                                              [{"op": "replace" if "rel_tol" in p else "add", "path": "/rel_tol", "value": 1e-6}],
                                              confidence="low", confirm=True))
                rem.append(C.ask_user("积分可能发散，或区间内有奇点：问用户是否在奇点处分段、积分是否存在", "split_at_singularity"))
                raise Fail("E.NUMERICAL.NO_CONVERGENCE",
                           f"没有达到容差：{reason}；当前估计 {sign * I!r}，误差估计 {E:.3e} > 容差 {tol:.3e}（IN-5）",
                           "/expression",
                           {"value_estimate": sign * I, "error_estimate": E, "tolerance": tol, "intervals": len(iv),
                            "hardest_interval": [min(xl, xr), max(xl, xr)], "reason": reason},
                           rem, agent_hint="积分可能发散或有奇点；不要只放宽容差，先确认积分是否存在")
            r1, e1 = qk15(lo, mid)
            r2, e2 = qk15(mid, hi)
            iv[k:k + 1] = [[lo, mid, r1, e1], [mid, hi, r2, e2]]
            bisections += 1
        # IN-4: bisect every final subinterval once more
        halves = []
        for lo, hi, _, _ in iv:
            mid = lo / 2 + hi / 2
            if lo < mid < hi:
                halves.append(qk15(lo, mid)[0])
                halves.append(qk15(mid, hi)[0])
            else:
                halves.append(qk15(lo, hi)[0])
        I2 = math.fsum(halves)
        diff = abs(I2 - I)
        return self._result(P, sign * I, E, diff, len(iv), ev["n"], bisections, transform, sign * I2, tol)

    def _result(self, P, value, ksum, diff, nint, nev, iters, transform, refined, tol=None):
        if tol is None:
            tol = max(P["abs_tol"], P["rel_tol"] * abs(value))
        est = max(ksum, diff)
        result = {"value": C.enc_float(value), "error_estimate": C.enc_float(est),
                  "tolerances": {"rel_tol": P["rel_tol"], "abs_tol": P["abs_tol"]},
                  "limits": P["limits"], "intervals": nint, "evaluations": nev}
        trust = {
            "convergence": {"applicable": True, "converged": True, "iterations": iters,
                            "criterion": "Σ 子区间误差估计 ≤ max(abs_tol, rel_tol·|I|)", "final_value": ksum},
            "residual": {"refined_value": C.enc_float(refined), "refined_difference": diff,
                         "definition": "全部子区间再二分一次后的积分值与返回值之差"},
            "condition": None,
            "error_estimate": {"value_abs": est, "kronrod_sum": ksum,
                               "basis": "max(Σ QUADPACK qk15 误差估计, |全部子区间再二分一次后的变化|)；启发式，非严格上界"},
            "checks": {"tolerance_met": bool(ksum <= tol), "refinement_consistent": bool(diff <= tol),
                       "all_finite": bool(math.isfinite(value) and math.isfinite(est))},
            "accuracy_grade": "C", "credentials": None,
            "derived": {"transform": transform, "tolerance": tol},
        }
        return result, trust, []
