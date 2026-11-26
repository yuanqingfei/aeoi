"""control.lti.step (profile 10.2), written from the profile and the core spec.

Continuous systems: exact ZOH discretisation of the step input through exp([[A, B], [0, 0]] dt)
(Van Loan 1978) with this package's own scaling-and-squaring Pade expm (Higham 2005), then the
recursion x_{k+1} = Phi x_k + Gamma, y_k = C x_k + D for every input channel. Discrete systems recurse
directly (dt = ts). tf / zpk inputs use the controllable canonical realization (aeoi_lti.to_ss).
Step metrics on the grid, no interpolation.
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
import aeoi_lti as L
from aeoi_common import Fail

OP_ID = "control.lti.step"
VERSION = "0.2.0"
METHOD_ID = "pyscipy:step:zoh-vanloan-expm-recursion"
MAX_GRID_STEPS = 10_000_000      # N x nu (profile 10.2)
MAX_OUTPUT_VALUES = 1_000_000    # returned N x ny x nu (profile 10.2)
DT_REL_TOL = 1e-9                # discrete systems: |dt - ts| <= 1e-9 ts


def _manifest():
    errors = L.LTI_ERRORS + ["E.VALIDATION.GRID", "E.VALIDATION.RANGE", "E.RESOURCE.COMPUTE_LIMIT", "E.RESOURCE.OUTPUT_TOO_LARGE"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Step response (Python second implementation)",
        description="LTI 系统（tf/ss/zpk，连续或离散）在均匀时间网格上的单位阶跃响应（每个输入通道分别施加单位阶跃），"
                    "以及在网格上定义（不插值）的阶跃指标。离散系统要求 dt = ts。",
        category="analysis", tags=["time-domain", "lti", "step"],
        applicability={"preconditions": ["system 为 proper", "uniform 网格：t_k = k·dt，k = 0..N−1，N = floor(t_final/dt + 1e-9) + 1",
                                         "离散系统要求 dt = ts（相对差 ≤ 1e-9）", "阶跃指标要求系统渐近稳定且该通道 DC 增益非零"],
                       "not_for": ["非线性系统", "任意输入（用 control.lti.lsim）", "带时滞的系统"], "siso_only": False},
        provenance={"algorithm": "连续系统：对 [[A, B], [0, 0]]·dt 求矩阵指数（自写缩放平方 + Padé 3/5/7/9/13，Higham 2005）得 Φ、Γ，"
                                 "再递推 x_{k+1} = Φx_k + Γ、y_k = Cx_k + D（每个输入通道一列）；阶跃输入在 ZOH 下无离散化误差。"
                                 "离散系统直接递推 x_{k+1} = Ax_k + B。tf/zpk 先化为可控标准型（MIMO tf 逐元素块对角）。"
                                 "终点一致性：递推得到的 x(t_end) 与 [[A, B], [0, 0]]·t_end 的矩阵指数（离散为增广矩阵幂）直接给出的 x(t_end) 比较。"
                                 "指标在网格上定义：上升时间为首个越过 hi·y∞ 与首个越过 lo·y∞ 的网格时刻之差，调节时间为最后一个"
                                 " |y/y∞ − 1| ≥ 阈值 的下一格，峰值为网格上最大的 |y|，y∞ 取 DC 增益。",
                    "references": [
                        {"type": "paper", "cite": "C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978",
                         "doi": "10.1109/TAC.1978.1101743"},
                        {"type": "paper", "cite": "N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005",
                         "doi": "10.1137/04061101X"},
                        {"type": "textbook", "cite": "K. Ogata, Modern Control Engineering, 5th ed., ch. 5（阶跃指标）"}] + L.LTI_REFERENCES},
        backend={"library": "aeoi-pyscipy aeoi_lti + aeoi_kernels (Python + numpy; small matrix products via numpy matmul)",
                 "routine": "op_step.Step.execute (aeoi_kernels.expm Pade-13 scaling-squaring + recursion)",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T2",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_grid_steps": MAX_GRID_STEPS, "max_output_values": MAX_OUTPUT_VALUES, "max_states": L.MAX_STATES},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors,
        warnings=["W.DOMAIN.UNSTABLE_SYSTEM", "W.DOMAIN.NOT_SETTLED", "W.DOMAIN.ZERO_DC_GAIN", "W.ACCURACY.GRID_TOO_COARSE",
                  "W.ACCURACY.AUTO_GRID", "W.VALIDATION.LEADING_ZEROS_STRIPPED"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": L.LTI_ASSUMPTIONS + [
                     {"id": "zoh_exact", "statement": "阶跃输入在 ZOH 下无离散化误差（只有 expm 与递推的舍入）", "enforced_by": "validation"},
                     {"id": "dt_equals_ts", "statement": "离散系统的 dt 等于 ts", "enforced_by": "error:E.VALIDATION.GRID"},
                     {"id": "grid", "statement": "时间网格足以分辨动态（指标相对连续时间真值的误差 ≤ dt）", "enforced_by": "warning:W.ACCURACY.GRID_TOO_COARSE"},
                     {"id": "stable_for_info", "statement": "阶跃指标只对渐近稳定系统给出", "enforced_by": "warning:W.DOMAIN.UNSTABLE_SYSTEM"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["all_finite"],
                 "guards": {"all_finite": ["/y", "/info"]}},
    )


def _nice_ceil(x):
    """Smallest of {1, 2, 5} x 10^k that is >= x."""
    if x <= 0:
        return x
    e = math.floor(math.log10(x))
    for base in (e, e + 1):
        for m in (1, 2, 5):
            v = float(f"{m}e{base}")
            if v >= x * (1 - 1e-12):
                return v
    return float(f"1e{e + 2}")


def _grid_points(t_final, dt):
    return int(math.floor(t_final / dt + 1e-9)) + 1


def _blas_name():
    try:
        b = np.show_config(mode="dicts")["Build Dependencies"]["blas"]
        return f"{b.get('name')} {b.get('version')}"
    except Exception:
        return "unknown"


class Step(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()
    MANIFEST["x-pyscipy-blas"] = (f"numpy matmul for small dense products (BLAS {_blas_name()}); "
                                  "threads pinned to 1 by server.py")

    def prepare(self, args):
        s = L.parse_system(args["system"])
        L.require_proper(s)
        A, B, Cm, D, realization = L.to_ss(s)
        n = A.shape[0]
        poles = K.sort_complex(K.eigvals(A))[0] if n else np.zeros(0, dtype=complex)
        tm = args["time"]
        auto = tm["mode"] == "auto"
        if auto:
            t_final, dt = self._auto_grid(s, poles, tm.get("t_final"))
        else:
            t_final, dt = float(tm["t_final"]), float(tm["dt"])
            if s.discrete and abs(dt - s.ts) > DT_REL_TOL * s.ts:
                raise Fail("E.VALIDATION.GRID", f"离散系统要求 dt = ts：这里 dt = {dt!r}，ts = {s.ts!r}", "/time/dt",
                           {"dt": dt, "ts": s.ts},
                           [C.patch_remedy("dt_equals_ts", "把 dt 改为采样周期 ts", [{"op": "replace", "path": "/time/dt", "value": s.ts}],
                                           confidence="high", confirm=False),
                            C.ask_user("确认采样周期 ts；离散系统只在采样时刻有定义")])
        N = _grid_points(t_final, dt)
        steps = N * s.nu
        fastest = float(np.max(np.abs(poles))) if len(poles) else 0.0
        out = args.get("output") or {}
        mode = out.get("return_samples", "all")
        max_samples = int(out.get("max_samples", 10001))
        if steps > MAX_GRID_STEPS:
            rem = []
            if not s.discrete and not auto:
                need = t_final / (MAX_GRID_STEPS // s.nu - 2)
                if mode == "all":
                    need = max(need, t_final / (MAX_OUTPUT_VALUES // (s.ny * s.nu) - 2))
                want = max(need, 0.1 / fastest) if fastest > 0 else need
                new_dt = _nice_ceil(want)
                rem.append(C.patch_remedy("coarser_grid", f"dt 改为 {new_dt:g} s（≈ 0.1/最快极点模，且满足上限）",
                                          [{"op": "replace", "path": "/time/dt", "value": new_dt}], confidence="high", confirm=False))
            elif s.discrete and not auto:
                new_t = (MAX_GRID_STEPS // s.nu - 1) * dt
                rem.append(C.patch_remedy("shorten", f"t_final 缩短到 {new_t:g} s（离散系统的 dt 必须等于 ts）",
                                          [{"op": "replace", "path": "/time/t_final", "value": new_t}], confidence="medium", confirm=True))
            rem.append(C.ask_user("与用户确认是否可以缩短 t_final 或放大 dt"))
            raise Fail("E.RESOURCE.COMPUTE_LIMIT",
                       f"时间网格 {N} 点 × {s.nu} 个输入通道 = {steps} 步，超过单次计算上限 {MAX_GRID_STEPS}。",
                       "/time/dt", {"grid_points": N, "inputs": s.nu, "limit": MAX_GRID_STEPS, "fastest_pole_rad_s": fastest}, rem,
                       agent_hint="增大 dt 或缩短 t_final；抽样返回不能解决，因为递推仍要走完每个网格点。")
        n_ret = {"all": N, "decimated": min(N, max_samples), "none": 0}[mode]
        if n_ret * s.ny * s.nu > MAX_OUTPUT_VALUES:
            m_ok = max(2, MAX_OUTPUT_VALUES // (s.ny * s.nu))
            rem = [C.patch_remedy("decimate", f"抽样返回最多 {m_ok} 个时间点（计算网格不变）",
                                  [{"op": "add", "path": "/output", "value": {"return_samples": "decimated", "max_samples": m_ok}}],
                                  confidence="high", confirm=False),
                   C.patch_remedy("metrics_only", "只返回阶跃指标，不返回采样",
                                  [{"op": "add", "path": "/output", "value": {"return_samples": "none"}}], confidence="medium", confirm=False),
                   C.ask_user("确认需要多少个采样点")]
            raise Fail("E.RESOURCE.OUTPUT_TOO_LARGE",
                       f"返回值个数 {n_ret} × {s.ny} × {s.nu} = {n_ret * s.ny * s.nu} 超过上限 {MAX_OUTPUT_VALUES}。",
                       "/output", {"returned_points": n_ret, "outputs": s.ny, "inputs": s.nu, "limit": MAX_OUTPUT_VALUES}, rem,
                       agent_hint="用 output.return_samples = decimated（max_samples）或 none；计算本身可以做。")
        si = args.get("step_info") or {}
        lims = [float(x) for x in si.get("rise_time_limits", [0.1, 0.9])]
        if not lims[0] < lims[1]:
            raise Fail("E.VALIDATION.RANGE", f"rise_time_limits 要求下限 < 上限，这里是 {lims}", "/step_info/rise_time_limits",
                       {"rise_time_limits": lims}, [C.ask_user("给出递增的上升时间阈值，如 [0.1, 0.9]")])
        return dict(sys=s, A=A, B=B, C=Cm, D=D, realization=realization, poles=poles, t_final=t_final, dt=dt, N=N, auto=auto,
                    mode=mode, max_samples=max_samples, info=bool(si.get("enabled", True)), lims=lims,
                    thr=float(si.get("settling_threshold", 0.02)))

    @staticmethod
    def _auto_grid(s, poles, t_final):
        # own heuristic (the profile leaves the auto grid to the implementation): 7 time constants of the
        # slowest stable mode, at least 10/fastest; 1000 steps; discrete systems step at ts.
        if s.discrete:
            dt = s.ts
            if t_final is None:
                mags = [abs(p) for p in poles if 0 < abs(p) < 1]
                slow = max(mags) if mags else 0.5
                t_final = dt * max(100, math.ceil(7.0 / max(-math.log(slow), 1e-3)))
            return float(t_final), dt
        if t_final is None:
            re = [abs(p.real) for p in poles if p.real < 0]
            fast = max([abs(p) for p in poles], default=1.0)
            t_final = 7.0 / min(re) if re else 10.0
            t_final = max(t_final, 10.0 / max(fast, 1e-12)) if not re else t_final
        return float(t_final), float(t_final) / 1000.0

    def estimate(self, P):
        n = P["A"].shape[0]
        return {"grid_points": P["N"], "states": n, "inputs": P["sys"].nu, "outputs": P["sys"].ny,
                "flops_estimate": 2 * P["N"] * (n * n + n * P["sys"].ny) * P["sys"].nu}

    def execute(self, P):
        s, A, B, Cm, D = P["sys"], P["A"], P["B"], P["C"], P["D"]
        n, ny, nu, N, dt = A.shape[0], s.ny, s.nu, P["N"], P["dt"]
        expm_info = None
        if s.discrete:
            Phi, Gam = A, B
        elif n:
            M = np.zeros((n + nu, n + nu))
            M[:n, :n] = A * dt
            M[:n, n:] = B * dt
            E, expm_info = K.expm(M)
            Phi, Gam = E[:n, :n].copy(), E[:n, n:].copy()
        else:
            Phi, Gam = np.zeros((0, 0)), np.zeros((0, nu))
        Y = np.empty((ny, nu, N))
        X = np.zeros((n, nu))
        x_last = X
        for k in range(N):
            Y[:, :, k] = (Cm @ X + D) if n else D
            if k < N - 1:
                X = Phi @ X + Gam
        x_last = X
        # final-state consistency
        t_end = (N - 1) * dt
        disc = 0.0
        if n and N > 1:
            if s.discrete:
                Maug = np.zeros((n + nu, n + nu))
                Maug[:n, :n] = A
                Maug[:n, n:] = B
                Maug[n:, n:] = np.eye(nu)
                x_dir = K.matpow(Maug, N - 1)[:n, n:]
            else:
                M = np.zeros((n + nu, n + nu))
                M[:n, :n] = A * t_end
                M[:n, n:] = B * t_end
                x_dir = K.expm(M)[0][:n, n:]
            for j in range(nu):
                nd = float(np.linalg.norm(x_dir[:, j]))
                dd = float(np.linalg.norm(x_last[:, j] - x_dir[:, j]))
                disc = max(disc, dd / nd if nd > 0 else dd)
        warnings = list(s.warnings)
        if P["auto"]:
            warnings.append({"code": "W.ACCURACY.AUTO_GRID", "message": f"时间网格由算子自动选择：t_final = {P['t_final']:g} s，dt = {dt:g} s",
                             "path": "/time", "details": {"t_final": P["t_final"], "dt": dt}})
        fastest = float(np.max(np.abs(P["poles"]))) if len(P["poles"]) else 0.0
        if not s.discrete and fastest * dt > math.pi:
            warnings.append({"code": "W.ACCURACY.GRID_TOO_COARSE",
                             "message": f"dt·max|p| = {fastest * dt:.3g} > π：网格分辨不了最快的模态（指标误差 ≤ dt）",
                             "path": "/time/dt", "details": {"dt": dt, "fastest_pole_rad_s": fastest}})
        # step metrics
        info = None
        stable = L.asymptotically_stable(P["poles"], s.discrete, float(np.max(np.sum(np.abs(A), axis=0))) if n else 0.0)
        tgrid = np.arange(N) * dt
        if P["info"]:
            if not stable:
                warnings.append({"code": "W.DOMAIN.UNSTABLE_SYSTEM",
                                 "message": "系统不是渐近稳定的（" + ("|λ| ≥ 1" if s.discrete else "Re λ ≥ 0") + "），阶跃指标没有定义，info 为 null；y 仍按窗口给出",
                                 "path": "/system", "details": {"max_real_pole": float(max(p.real for p in P["poles"])) if len(P["poles"]) else None}})
                info = None if s.siso else [[None] * nu for _ in range(ny)]
            else:
                g0, zero_mask = self._dcgain(s, A, B, Cm, D)
                cells = [[None] * nu for _ in range(ny)]
                zero_ch, unsettled = [], []
                for i in range(ny):
                    for j in range(nu):
                        if zero_mask[i][j]:
                            zero_ch.append([i, j])
                            continue
                        cells[i][j], settled = _metrics(tgrid, Y[i, j], float(g0[i, j]), P["lims"], P["thr"])
                        if not settled:
                            unsettled.append([i, j])
                if zero_ch:
                    warnings.append({"code": "W.DOMAIN.ZERO_DC_GAIN", "message": f"{len(zero_ch)} 个通道的 DC 增益为 0，阶跃指标没有定义（info 为 null）",
                                     "path": "/info", "details": {"channels": zero_ch}})
                if unsettled:
                    warnings.append({"code": "W.DOMAIN.NOT_SETTLED", "message": f"{len(unsettled)} 个通道在 t_final 内没有进入调节带，settling_time 为 null",
                                     "path": "/info", "details": {"channels": unsettled}})
                info = cells[0][0] if s.siso else cells
        # returned samples
        mode = P["mode"]
        if mode == "none":
            t_out = y_out = None
        else:
            if mode == "decimated" and N > P["max_samples"]:
                M_ = P["max_samples"]
                idx = np.array([int(math.floor(k * (N - 1) / (M_ - 1) + 0.5)) for k in range(M_)])
            else:
                idx = np.arange(N)
            t_out = {"unit": "s", "value": C.dense_f64(tgrid[idx])}
            y_out = {"unit": "1", "value": C.dense_f64(Y[:, :, idx])}
        finite = bool(np.all(np.isfinite(Y)))
        ymax = float(np.max(np.abs(Y))) if Y.size and finite else None
        result = {"t": t_out, "y": y_out, "n_grid_points": N, "info": info}
        trust = {"convergence": {"applicable": False},
                 "residual": {"final_state_rel_discrepancy": disc,
                              "definition": "‖x_{N−1}(逐步递推) − x(t_end)(一次性直接计算：连续为 [[A,B],[0,0]]·t_end 的矩阵指数，离散为增广矩阵幂)‖₂ / ‖x(t_end)‖₂，取所有输入通道最大值"},
                 "condition": {"norm1_A_dt": (float(np.max(np.sum(np.abs(A), axis=0))) * dt) if n else 0.0,
                               "expm_squarings": expm_info["squarings"] if expm_info else None,
                               "expm_pade_order": expm_info["order"] if expm_info else None,
                               "realization": {"identity": "given state space",
                                               "controllable_canonical": "controllable canonical form (scipy tf2ss convention)",
                                               "block_diagonal_controllable_canonical": "block-diagonal controllable canonical form (one block per tf element)"}[P["realization"]],
                               "max_real_pole": float(max(p.real for p in P["poles"])) if len(P["poles"]) else None},
                 "error_estimate": ({"y_abs": max(N * C.U, disc) * ymax,
                                     "basis": "启发式：max(N·u, 终点一致性偏差) × max|y|；非严格上界",
                                     "info_time_metrics_abs_s": dt,
                                     "info_basis": "指标按网格点定义（不插值），相对连续时间真值误差 ≤ dt"} if ymax is not None else None),
                 "checks": {"all_finite": finite},
                 "accuracy_grade": "C", "credentials": None,
                 "derived": {"poles": C.dense_c128(P["poles"].real, P["poles"].imag), "dt": dt, "t_end": t_end, "auto_grid": P["auto"],
                             "asymptotically_stable": stable}}
        return result, trust, warnings

    @staticmethod
    def _dcgain(s, A, B, Cm, D):
        """DC gain per channel and a mask of channels whose DC gain is zero (to rounding)."""
        if s.kind in ("tf", "zpk"):
            g = L.dcgain(s)
            return g, [[g[i, j] == 0.0 for j in range(s.nu)] for i in range(s.ny)]
        n = A.shape[0]
        if n == 0:
            return D.copy(), [[D[i, j] == 0.0 for j in range(s.nu)] for i in range(s.ny)]
        M = (np.eye(n) - A) if s.discrete else -A
        X = K.lu_solve(M, B)
        g = Cm @ X + D
        scale = np.abs(Cm) @ np.abs(X) + np.abs(D)
        return g, [[abs(g[i, j]) <= 1e3 * n * C.U * scale[i, j] for j in range(s.nu)] for i in range(s.ny)]


def _q(unit, v):
    return {"unit": unit, "value": float(v)}


def _metrics(t, y, yinf, lims, thr):
    """Grid metrics for one channel. Returns (info object, settled)."""
    sgn = 1.0 if yinf > 0 else -1.0
    lo_i = np.flatnonzero(sgn * (y - lims[0] * yinf) >= 0)
    hi_i = np.flatnonzero(sgn * (y - lims[1] * yinf) >= 0)
    rise = float(t[hi_i[0]] - t[lo_i[0]]) if (lo_i.size and hi_i.size) else None
    out = np.flatnonzero(np.abs(y / yinf - 1.0) >= thr)
    settled_idx = int(out[-1]) + 1 if out.size else 0
    settled = settled_idx < len(t)
    tail = y[int(hi_i[0]):] if hi_i.size else y
    smin = min(float(np.min(tail)), yinf)
    smax = max(float(np.max(tail)), yinf)
    y_os = float(np.max(sgn * y))
    dy = abs(y_os) - abs(yinf)
    overshoot = abs(100.0 * dy / yinf) if dy > 0 else 0.0
    ius = int(np.argmin(sgn * y))
    y_us = float(y[ius])
    undershoot = (-100.0 * y_us / yinf) if sgn * y_us < 0 else 0.0
    ipk = int(np.argmax(np.abs(y)))
    info = {"rise_time": _q("s", rise) if rise is not None else None,
            "settling_time": _q("s", t[settled_idx]) if settled else None,
            "settling_min": _q("1", smin), "settling_max": _q("1", smax),
            "overshoot": _q("%", overshoot), "undershoot": _q("%", undershoot),
            "peak": _q("1", abs(float(y[ipk]))), "peak_time": _q("s", t[ipk]),
            "steady_state": {"unit": "1", "value": float(yinf), "source": "dcgain"}}
    return info, settled
