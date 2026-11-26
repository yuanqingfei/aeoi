"""control.lti.bode (profile 10.1), written from the profile and the core spec.

Frequency response of tf / ss / zpk systems (shared parsing in aeoi_lti): tf by Horner on numerator
and denominator, zpk by the product form, ss by a complex LU solve of (sI - A) X = B at each point
(own vectorised Gaussian elimination, aeoi_kernels.lu_solve_batch). Phase: atan2 in degrees, first
point in (c - 180, c + 180], then each point is the raw phase plus the multiple of 360 that puts it
within 180 deg of the previous point (numpy.unwrap rule, implemented here).
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
import aeoi_lti as L
from aeoi_common import Fail

OP_ID = "control.lti.bode"
VERSION = "0.2.0"
METHOD_ID = "pyscipy:bode:horner-zpk-product-complex-lu"
NEAR_POLE_REL = 1e-6
DB_PER_REL = 20.0 / math.log(10.0)
DEG_PER_REL = 180.0 / math.pi


def _manifest():
    errors = L.LTI_ERRORS + ["E.VALIDATION.GRID", "E.VALIDATION.RANGE", "E.DOMAIN.ABOVE_NYQUIST", "E.NUMERICAL.POLE_ON_GRID"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Bode frequency response (Python second implementation)",
        description="LTI 系统（tf/ss/zpk，SISO 或 MIMO，连续或离散）在给定频点上的幅值（dB）与相位（deg）。"
                    "频点 explicit 时必须严格递增；输入单位可为 rad/s 或 Hz，输出 w 一律 rad/s。",
        category="analysis", tags=["frequency-domain", "lti", "bode"],
        applicability={"preconditions": ["system 为 proper（分子阶次 ≤ 分母阶次；zpk 零点数 ≤ 极点数）",
                                         "频点为正；explicit 时严格递增；logspace 时 min < max",
                                         "离散系统要求 ω ≤ π/ts"],
                       "not_for": ["非线性或时变系统", "带时滞的系统"], "siso_only": False},
        provenance={"algorithm": "tf：分子、分母在 s = jω（离散 z = e^{jωts}）处 Horner 复数求值后相除，Higham 运行误差界作先验估计；"
                                 "zpk：k·Π(s−z)/Π(s−p)；ss：每个频点对 (sI−A)X = B 做部分选主元复数 Gauss 消去，G = CX + D，"
                                 "以 n·u·cond₁(sI−A) 作启发式估计（显式逆的 1-范数，不是 SVD）。相位 atan2，首点落在 (c−180°, c+180°]，"
                                 "其后每点取与前一点相差不超过 180° 的那个 360° 分支（numpy.unwrap 规则）。",
                    "references": [
                        {"type": "textbook", "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §5.1（Horner 误差界）",
                         "doi": "10.1137/1.9780898718027"},
                        {"type": "textbook", "cite": "G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch. 6"}]
                    + L.LTI_REFERENCES},
        backend={"library": "aeoi-pyscipy aeoi_lti + aeoi_kernels (Python + numpy element-wise; no LAPACK)",
                 "routine": "op_bode.Bode.execute -> aeoi_lti.freqresp (Horner / zpk product / aeoi_kernels.lu_solve_batch)",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T3",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_frequency_points": 100000, "max_states": L.MAX_STATES, "max_states_for_condition_estimate": 64},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors,
        warnings=["W.NUMERICAL.NEAR_POLE", "W.NUMERICAL.ZERO_ON_GRID", "W.VALIDATION.LEADING_ZEROS_STRIPPED"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": L.LTI_ASSUMPTIONS + [
                     {"id": "grid", "statement": "频率网格与单位为调用方所需（rad/s 或 Hz）", "enforced_by": "caller"},
                     {"id": "increasing", "statement": "explicit 频点严格递增（相位展开依赖顺序）", "enforced_by": "error:E.VALIDATION.GRID"},
                     {"id": "nyquist", "statement": "离散系统频点不超过 π/ts", "enforced_by": "error:E.DOMAIN.ABOVE_NYQUIST"},
                     {"id": "not_on_pole", "statement": "频点不恰在极点上", "enforced_by": "error:E.NUMERICAL.POLE_ON_GRID"},
                     {"id": "near_pole", "statement": "频点靠近极点时结果精度下降", "enforced_by": "warning:W.NUMERICAL.NEAR_POLE"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["all_finite", "frequencies_strictly_increasing"],
                 "guards": {"all_finite": ["/magnitude", "/phase"], "frequencies_strictly_increasing": ["/phase"]}},
    )


def _branch(x, c):
    """x + 360 k in (c - 180, c + 180]."""
    k = math.floor((c + 180.0 - x) / 360.0)
    return x + 360.0 * k


def _phase(raw, unwrap, c):
    out = np.array(raw, dtype=float)
    prev = None
    for k in range(len(out)):
        x = out[k]
        if not math.isfinite(x):
            continue
        if prev is None or not unwrap:
            out[k] = _branch(x, c)
        else:
            m = math.floor((prev + 180.0 - x) / 360.0)
            out[k] = x + 360.0 * m
        prev = out[k]
    return out


class Bode(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        s = L.parse_system(args["system"])
        L.require_proper(s)
        fr = args["frequency"]
        unit = fr["unit"]
        scale = 2.0 * math.pi if unit == "Hz" else 1.0
        if fr["mode"] == "explicit":
            vals = [float(v) for v in fr["values"]]
            for i in range(1, len(vals)):
                if not vals[i] > vals[i - 1]:
                    fixed = sorted(set(vals))
                    rem = []
                    if len(fixed) >= 1:
                        rem.append(C.patch_remedy("sort_frequencies", "把频点排序并去掉重复值（数值不变）",
                                                  [{"op": "replace", "path": "/frequency/values", "value": fixed}],
                                                  confidence="high", confirm=False))
                    rem.append(C.ask_user("确认频点的顺序（相位展开依赖顺序）"))
                    raise Fail("E.VALIDATION.GRID", f"频点必须严格递增：values[{i}] = {vals[i]!r} ≤ values[{i - 1}] = {vals[i - 1]!r}",
                               f"/frequency/values/{i}", {"index": i}, rem)
            w = np.array([v * scale for v in vals]) if scale != 1.0 else np.array(vals)
        else:
            lo, hi, n = float(fr["min"]), float(fr["max"]), int(fr["n"])
            if not lo < hi:
                raise Fail("E.VALIDATION.RANGE", f"logspace 要求 min < max，这里 min = {lo!r}，max = {hi!r}", "/frequency/max",
                           {"min": lo, "max": hi},
                           [C.patch_remedy("swap_min_max", "交换 min 与 max", [{"op": "replace", "path": "/frequency/min", "value": hi},
                                                                              {"op": "replace", "path": "/frequency/max", "value": lo}],
                                           confidence="medium", confirm=True)] if lo > hi else [C.ask_user("给出不同的 min 与 max")])
            a, b = math.log10(lo), math.log10(hi)
            w = np.array([10.0 ** (a + (b - a) * k / (n - 1)) for k in range(n)]) * scale
        if s.discrete:
            nyq = math.pi / s.ts
            above = np.flatnonzero(w > nyq)
            if above.size:
                i0 = int(above[0])
                rem = []
                if fr["mode"] == "explicit" and i0 > 0:
                    rem.append(C.patch_remedy("drop_above_nyquist", "去掉高于 Nyquist 频率 π/ts 的频点",
                                              [{"op": "replace", "path": "/frequency/values", "value": [float(v) for v in fr["values"][:i0]]}],
                                              confidence="medium", confirm=True))
                elif fr["mode"] == "logspace" and float(fr["min"]) * scale < nyq:
                    rem.append(C.patch_remedy("clip_max", "把 max 降到 Nyquist 频率 π/ts",
                                              [{"op": "replace", "path": "/frequency/max", "value": nyq / scale}], confidence="medium", confirm=True))
                rem.append(C.ask_user("确认采样周期 ts 与所需频段；离散系统只在 [0, π/ts] 上有定义"))
                raise Fail("E.DOMAIN.ABOVE_NYQUIST",
                           f"{above.size} 个频点高于 Nyquist 频率 π/ts = {nyq!r} rad/s（第一个 ω = {float(w[i0])!r} rad/s，D-1）",
                           "/frequency", {"count": int(above.size), "nyquist_rad_s": nyq, "first_index": i0}, rem,
                           retryable=True)
        ph = args.get("phase") or {}
        return dict(sys=s, w=w, hz=unit == "Hz", unwrap=bool(ph.get("unwrap", True)), c=float(ph.get("branch_center_deg", 0.0)),
                    explicit=fr["mode"] == "explicit", raw_values=fr.get("values"))

    def estimate(self, P):
        s = P["sys"]
        n = s.A.shape[0] if s.kind == "ss" else None
        return {"frequency_points": len(P["w"]), "outputs": s.ny, "inputs": s.nu, "states": n}

    def execute(self, P):
        s, w = P["sys"], P["w"]
        pts = L.eval_points(s, w)
        if s.discrete:
            ds = 2.0 * C.U * (1.0 + np.abs(w * s.ts))
        else:
            ds = (C.U * np.abs(w)) if P["hz"] else np.zeros(len(w))
        G, rel, hit, info = L.freqresp(s, pts, ds)
        if np.any(hit):
            i0 = int(np.flatnonzero(hit)[0])
            rem = []
            if P["explicit"] and len(w) > 1:
                keep = [float(v) for k, v in enumerate(P["raw_values"]) if not hit[k]]
                if keep:
                    rem.append(C.patch_remedy("drop_pole_frequencies", "去掉恰好落在极点上的频点",
                                              [{"op": "replace", "path": "/frequency/values", "value": keep}], confidence="medium", confirm=False))
            rem.append(C.ask_user("该频点恰为系统极点，|G| 无界；确认是否需要该频点"))
            raise Fail("E.NUMERICAL.POLE_ON_GRID", f"频点 ω = {float(w[i0])!r} rad/s 恰为系统极点（分母为 0）", f"/frequency",
                       {"index": i0, "w_rad_s": float(w[i0]), "count": int(np.sum(hit))}, rem)
        mag = np.abs(G)
        with np.errstate(divide="ignore"):
            db = 20.0 * np.log10(mag)
        raw = np.degrees(np.arctan2(G.imag, G.real))
        raw = np.where(mag == 0, np.nan, raw)
        phase = np.empty_like(raw)
        for i in range(s.ny):
            for j in range(s.nu):
                phase[i, j] = _phase(raw[i, j], P["unwrap"], P["c"])
        warnings = list(s.warnings)
        zero = mag == 0
        if np.any(zero):
            idx = np.argwhere(zero)
            i, j, k = (int(x) for x in idx[0])
            warnings.append({"code": "W.NUMERICAL.ZERO_ON_GRID",
                             "message": f"{len(idx)} 个频点恰为传递函数零点：幅值为 -Inf dB，相位为 NaN（R-3）",
                             "path": f"/magnitude/value/data/{(i * s.nu + j) * len(w) + k}",
                             "details": {"count": int(len(idx)), "first_w_rad_s": float(w[k]), "channel": [i, j]}})
        finite_rel = rel[np.isfinite(rel)]
        rel_max = float(np.max(finite_rel)) if finite_rel.size else None
        near = np.flatnonzero(rel > NEAR_POLE_REL)
        if near.size:
            warnings.append({"code": "W.NUMERICAL.NEAR_POLE",
                             "message": f"{near.size} 个频点靠近极点，误差估计超过 1e-6（相对），最坏为 {float(np.max(rel[near])):.3g}",
                             "path": "/frequency", "details": {"indices": [int(x) for x in near[:20]],
                                                               "w_rad_s": [float(w[x]) for x in near[:20]],
                                                               "rel_error_estimate_max": float(np.max(rel[near]))}})
        intended = zero
        finite = bool(np.all(np.isfinite(db[~intended])) and np.all(np.isfinite(phase[~intended])))
        inc = bool(np.all(np.diff(w) > 0)) if len(w) > 1 else True
        nw = len(w)
        result = {"w": {"unit": "rad/s", "value": C.dense_f64(w)},
                  "magnitude": {"unit": "dB", "value": C.dense_f64(db.reshape(s.ny, s.nu, nw))},
                  "phase": {"unit": "deg", "value": C.dense_f64(phase.reshape(s.ny, s.nu, nw))}}
        if rel_max is not None and not math.isnan(rel_max):
            kw = int(np.nanargmax(np.where(np.isfinite(rel), rel, -1.0)))
            ee = {"rel_max": rel_max, "magnitude_db_abs": DB_PER_REL * rel_max, "phase_deg_abs": DEG_PER_REL * rel_max,
                  "worst_frequency_rad_s": float(w[kw]),
                  "basis": ("tf：Horner 一阶先验界 γ_{4d+2}·Σ|a_k||s|^k/|p(s)|，分子分母相加，并计入 s（或 z）本身的舍入；"
                            "zpk：γ_{4(#z+#p)+4} 加 s 的舍入；ss：n·u·cond₁(sI−A)（启发式，非严格上界）。"
                            "dB 误差 ≈ 8.686 × 相对误差，相位误差 ≈ 57.30° × 相对误差")}
        else:
            ee = None
        poles = L.poles(s)
        trust = {"convergence": {"applicable": False}, "residual": None,
                 "condition": {"representation": info.get("representation"), "max_cond1_sI_minus_A": info.get("max_cond1_sI_minus_A"),
                               "max_rel_error_estimate": rel_max},
                 "error_estimate": ee,
                 "checks": {"all_finite": finite, "frequencies_strictly_increasing": inc},
                 "accuracy_grade": "C", "credentials": None,
                 "derived": {"poles": C.dense_c128(poles.real, poles.imag),
                             "phase_convention": "unwrapped" if P["unwrap"] else "wrapped", "branch_center_deg": P["c"]}}
        return result, trust, warnings
