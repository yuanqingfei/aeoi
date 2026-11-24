"""Numeric comparison rules (spec 9.4 CF-6) for control.lti.poles_zeros, control.lti.bode, control.lti.step,
control.design.lqr, control.lti.c2d, control.lti.lsim (v0.4.5) and control.design.pidtune (v0.4.6).

Each rule is rule(case, env, manifest, req=None, expected=None) -> [(check, ok, max_error, tolerance, detail)].
The first entry is the operator's primary field; the runner reuses its deviation for the section-0 column-major check.

Ground rules (CF-6):
  * tolerances come from the suite (the operator's parity.yaml), never from the implementation's manifest or envelope;
  * every condition number (kappa) is computed here, from the reference request or the reference values;
    nothing the implementation reports (trust.error_estimate, trust.condition, trust.derived) is used;
  * unordered sets (poles, zeros) are matched (V-5, R-2); state-space realizations are compared through invariants
    (transfer-function values and poles), because a different but equivalent realization is a correct answer (V-5).
"""
import numpy as np

EPS = np.finfo(float).eps
_NF = {"NaN": float("nan"), "Inf": float("inf"), "-Inf": float("-inf")}


# ---------------------------------------------------------------- decoding
def num(x):
    if x is None:
        return float("nan")
    return _NF[x] if isinstance(x, str) else float(x)


def arr(o):
    """Inline dense array ({dtype, shape, data}), {unit, value} quantity, a list of [re, im] pairs or a plain list."""
    if isinstance(o, dict) and "value" in o and "unit" in o:
        o = o["value"]
    if isinstance(o, dict) and "data" in o:
        if "data_b64" in o or "uri" in o:
            raise ValueError("only inline JSON arrays are compared")
        shape = o.get("shape", [len(o["data"])])
        if o.get("dtype") == "c128":
            return np.array([complex(num(z[0]), num(z[1])) for z in o["data"]], dtype=complex).reshape(shape)
        return np.array([num(x) for x in o["data"]], dtype=float).reshape(shape)
    if isinstance(o, list):
        if o and isinstance(o[0], list) and len(o[0]) == 2 and not isinstance(o[0][0], list):
            return np.array([complex(num(z[0]), num(z[1])) for z in o], dtype=complex)
        return np.array([num(x) for x in np.ravel(np.array(o, dtype=object))], dtype=float)
    return np.array([num(o)])


def scalar(o):
    if isinstance(o, dict) and "value" in o:
        o = o["value"]
    return num(o)


def check(name, err, tol, detail=""):
    ok = bool(err <= tol) if np.isfinite(err) else False
    return (name, ok, float(err) if np.isfinite(err) else None, float(tol), "" if ok else (detail or f"error={err:.3e} > tolerance={tol:.3e}"))


def fail(name, detail):
    return (name, False, None, None, detail)


# ---------------------------------------------------------------- shared measures
def rel_fro(a, b):
    a, b = np.asarray(a), np.asarray(b)
    if a.shape != b.shape:
        return float("inf")
    d = np.linalg.norm(a - b)
    return 0.0 if d == 0 else float(d / max(np.linalg.norm(b), np.finfo(float).tiny))


def match(got, ref):
    """Globally greedy nearest matching of two complex sets (V-5, R-2). Returns [(ref_value, got_value)] or None."""
    got, ref = list(np.ravel(got)), list(np.ravel(ref))
    if len(got) != len(ref):
        return None
    pairs, ug, ur = [], set(), set()
    d = sorted((abs(g - r), i, j) for i, g in enumerate(got) for j, r in enumerate(ref))
    for _, i, j in d:
        if i not in ug and j not in ur:
            ug.add(i); ur.add(j); pairs.append((ref[j], got[i]))
    return pairs


def kappa_root(p, r):
    """Wilkinson relative condition number of a simple root r of the real polynomial p (highest degree first)."""
    n = len(p) - 1
    s = sum(abs(a) * abs(r) ** (n - k) for k, a in enumerate(p))
    d = abs(np.polyval(np.polyder(p), r)) if n > 0 else 0.0
    return float("inf") if d == 0 else s / (max(1.0, abs(r)) * d)


def roots_check(name, got, ref, base):
    """Matched root sets; tolerance per root base * max(1, kappa_root) with kappa from the reference set's polynomial.
    Reported error is max_i |delta_i| / max(1,|r_i|) / max(1, kappa_i), compared with base."""
    pairs = match(got, ref)
    if pairs is None:
        return fail(name, f"count {np.size(got)} != reference {np.size(ref)}")
    if not pairs:
        return check(name, 0.0, base)
    p = np.real(np.poly(np.ravel(ref)))
    worst, where = 0.0, None
    for r, g in pairs:
        e = abs(g - r) / max(1.0, abs(r)) / max(1.0, kappa_root(p, r))
        if e > worst:
            worst, where = e, r
    return check(name, worst, base, f"max |Δ|/max(1,|r|)/κ = {worst:.3e} > {base:.1e}" + (f" (at reference root {where:.6g})" if where is not None else ""))


# ---------------------------------------------------------------- control.lti.poles_zeros (parity.yaml: T2, kappa_root)
def rule_poles_zeros(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    base = 1e-10
    wp, wz = arr(e["poles"]), arr(e["zeros"])
    gp, gz = arr(r["poles"]), arr(r["zeros"])
    out = [roots_check("poles: matched set, |Δ|/max(1,|r|) ≤ 1e-10·max(1,κ_root)  [V-5]", gp, wp, base),
           roots_check("zeros: matched set, |Δ|/max(1,|r|) ≤ 1e-10·max(1,κ_root)  [V-5]", gz, wz, base)]
    gd, wd = arr(r["dcgain"]).ravel(), arr(e["dcgain"]).ravel()
    name = "dcgain: |Δ|/max(1,|ref|) ≤ 1e-10·max(1, Π(1+|p|)/Π|p|); ±Inf exact"
    if gd.shape != wd.shape:
        out.append(fail(name, f"shape {gd.shape} != {wd.shape}"))
    else:
        k = max(1.0, float(np.prod(1 + np.abs(wp)) / max(np.prod(np.abs(wp)), 1e-300))) if wp.size else 1.0
        worst, bad_inf = 0.0, []
        for a, b in zip(gd, wd):
            if np.isinf(a) or np.isinf(b):
                if a != b:
                    bad_inf.append((a, b))
            else:
                worst = max(worst, abs(a - b) / max(1.0, abs(b)) / k)
        out.append(fail(name, f"infinite DC gain disagrees: {bad_inf}") if bad_inf else check(name, worst, base))
    if "natural_frequency" in e and "damping_ratio" in e:
        wn, zr = arr(e["natural_frequency"]).ravel(), arr(e["damping_ratio"]).ravel()
        gwn, gzr = arr(r["natural_frequency"]).ravel(), arr(r["damping_ratio"]).ravel()
        pp = np.real(np.poly(wp)) if wp.size else np.array([1.0])
        if gwn.size != gp.size or gzr.size != gp.size or wn.size != wp.size:
            out.append(fail("natural_frequency / damping_ratio", "length does not match the pole count"))
        else:
            ew = ez = 0.0
            for k_, p in enumerate(wp):
                j = int(np.argmin(np.abs(gp - p)))                         # attribute of the matched returned pole
                kap = max(1.0, kappa_root(pp, p))
                ew = max(ew, abs(gwn[j] - wn[k_]) / max(1.0, abs(wn[k_])) / kap)
                if np.isfinite(zr[k_]):
                    ez = max(ez, abs(gzr[j] - zr[k_]) / kap)
            out.append(check("natural_frequency of the matched pole ≤ 1e-10·max(1,κ_root)", ew, base))
            out.append(check("damping_ratio of the matched pole ≤ 1e-10·max(1,κ_root)", ez, base))
    # `stable` is a boolean (CF-6: must be identical) - derived here from the reference poles, skipped when a
    # reference pole is too close to the stability boundary for the comparison to be meaningful
    if wp.size and "stable" in r:
        disc = num(req["system"].get("ts", 0)) > 0
        margin = (1 - np.abs(wp)) if disc else -np.real(wp)
        if np.min(np.abs(margin)) > 1e-8:
            want = bool(np.all(margin > 0))
            out.append(("stable equals the value implied by the reference poles  [CF-6]", r["stable"] is want, None, None,
                        f"got {r['stable']}, reference poles imply {want}"))
    return out


# ---------------------------------------------------------------- control.lti.bode (parity.yaml: T3)
def _bode_kappa(req, w):
    """Runner-side condition of evaluating G at s = jw (or z = e^{jw ts}), per frequency, from the reference request:
    1 + max over poles p of |s| / |s - p| (relative sensitivity of 1/(s-p) to a relative perturbation of s and p).
    Large only near a pole on the evaluation contour, the near_imag_axis_poles family."""
    sysd = req["system"]
    ts = num(sysd.get("ts", 0))
    if sysd["kind"] == "tf":
        dens = [np.array(sysd["den"], dtype=float)] if not isinstance(sysd["den"][0], list) else \
               [np.array(d, dtype=float) for row in sysd["den"] for d in row]
        poles = np.concatenate([np.roots(d) for d in dens]) if dens else np.array([])
    elif sysd["kind"] == "zpk":
        poles = arr(sysd["p"]).ravel().astype(complex)
    else:
        poles = np.linalg.eigvals(arr(sysd["A"])) if arr(sysd["A"]).size else np.array([])
    s = np.exp(1j * w * ts) if ts > 0 else 1j * w
    if poles.size == 0:
        return np.ones_like(w)
    d = np.min(np.abs(s[:, None] - poles[None, :]), axis=1)
    return 1.0 + np.abs(s) / np.maximum(d, 1e-300)


def rule_bode(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    gm, gph, gw = arr(r["magnitude"]), arr(r["phase"]), arr(r["w"]).ravel()
    wm, wph, ww = np.array(e["magnitude_db"], dtype=float), np.array(e["phase_deg"], dtype=float), np.array(e["w"], dtype=float)
    shape = tuple(e["shape"])
    if gm.shape != shape or gph.shape != shape or gw.shape != ww.shape:
        return [fail("magnitude: shape [ny, nu, nw]", f"got magnitude {gm.shape}, phase {gph.shape}, w {gw.shape}; reference {shape}, w {ww.shape}")]
    k = _bode_kappa(req, ww)                                               # per frequency, broadcast over channels
    em = np.abs(gm.reshape(-1, shape[-1]) - wm.reshape(-1, shape[-1])) / k
    # Phase is compared on the branch the spec fixes (10.x: first point in (c-180, c+180], then numpy-style unwrap;
    # unwrap=false: every point in that interval), not modulo 360 (spec 12.1 item 22). Only where the reference sits
    # within 1e-6 deg of the branch boundary, so that rounding alone may put a correct answer on the other side, is
    # the comparison modulo 360: the whole channel when unwrapping (the first point decides it), else that point.
    c = num(req.get("branch_center_deg", 0.0)); unwrap = req.get("unwrap", True) is not False
    dp = gph.reshape(-1, shape[-1]) - wph.reshape(-1, shape[-1])
    wr = wph.reshape(-1, shape[-1])
    edge = np.minimum(np.abs(wr - (c - 180.0)), np.abs(wr - (c + 180.0))) < 1e-6
    if unwrap:
        edge = np.repeat(edge[:, :1], shape[-1], axis=1)
    dp = np.where(edge, (dp + 180.0) % 360.0 - 180.0, dp)
    ep = np.abs(dp) / k
    ew = np.max(np.abs(gw - ww) / np.abs(ww)) if ww.size else 0.0
    return [check("magnitude: |Δ dB| ≤ 1e-9·κ(ω)", float(np.max(em)) if em.size else 0.0, 1e-9),
            check("phase on the spec's branch (not mod 360): |Δ| ≤ 1e-8·κ(ω) deg", float(np.max(ep)) if ep.size else 0.0, 1e-8),
            check("w: relative ≤ 1e-14 (rad/s)", ew, 1e-14)]


# ---------------------------------------------------------------- control.lti.step (parity.yaml: T2)
INFO_TIME = [("rise_time", "RiseTime"), ("settling_time", "SettlingTime"), ("peak_time", "PeakTime")]
INFO_VAL = [("peak", "Peak"), ("settling_min", "SettlingMin"), ("settling_max", "SettlingMax"), ("steady_state", "SteadyStateValue")]
INFO_PCT = [("overshoot", "Overshoot"), ("undershoot", "Undershoot")]


def rule_step(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    gy, wy = arr(r["y"]), np.array(e["y"], dtype=float)
    gt, wt = arr(r["t"]).ravel(), np.array(e["t"], dtype=float)
    shape = tuple(e["shape"])
    if gy.shape != shape or gt.shape != wt.shape:
        return [fail("y: shape [ny, nu, nt]", f"got y {gy.shape}, t {gt.shape}; reference {shape}, t {wt.shape}")]
    scale = max(1.0, float(np.max(np.abs(wy)))) if wy.size else 1.0
    out = [check("y: max|Δ| / max(1, max|y_ref|) ≤ 1e-10", float(np.max(np.abs(gy.ravel() - wy))) / scale if wy.size else 0.0, 1e-10),
           check("t: |Δ| ≤ 1e-12 s", float(np.max(np.abs(gt - wt))) if wt.size else 0.0, 1e-12),
           ("n_grid_points equals the reference  [CF-6]", r.get("n_grid_points") == e["n_grid_points"], None, None,
            f"got {r.get('n_grid_points')}, reference {e['n_grid_points']}")]
    if case.get("expect_info_null"):
        out.append(("info is null (unstable system, R-3)", r.get("info") is None, None, None, f"got {str(r.get('info'))[:80]}"))
        return out
    if "info" not in e:
        return out
    dt = float(wt[1] - wt[0]) if wt.size > 1 else 0.0                      # grid spacing of the reference, not the impl's dt
    rows = e["info"]
    siso = len(rows) == 1 and len(rows[0]) == 1
    et = ev = ep = 0.0
    msgs = []
    for i, row in enumerate(rows):
        for j, w in enumerate(row):
            try:
                g = r["info"] if siso else r["info"][i][j]
            except (TypeError, IndexError, KeyError):
                msgs.append(f"info[{i}][{j}] missing"); continue
            ss = w.get("SteadyStateValue")
            if ss is None or not abs(num(ss)) > 1e-12:                    # zero DC gain: info undefined, AEOI returns null
                if g is not None:
                    msgs.append(f"info[{i}][{j}] should be null (zero DC gain)")
                continue
            if g is None:
                msgs.append(f"info[{i}][{j}] is null"); continue
            for a, b in INFO_TIME:
                x, y = scalar(g[a]), num(w[b])
                if not (np.isnan(x) and np.isnan(y)):
                    et = max(et, abs(x - y) / dt if dt else (0.0 if x == y else np.inf))
            for a, b in INFO_VAL:
                x, y = scalar(g[a]), num(w[b])
                ev = max(ev, abs(x - y) / max(abs(y), 1e-300))
            for a, b in INFO_PCT:
                ep = max(ep, abs(scalar(g[a]) - num(w[b])))
    out.append(("info present / null as in the reference", not msgs, None, None, "; ".join(msgs[:3])))
    out.append(check("info times (rise, settling, peak): |Δ| ≤ dt (one grid step, I-10)", et, 1.0 + 1e-9))
    out.append(check("info values (peak, settling min/max, steady state): relative ≤ 1e-9", ev, 1e-9))
    out.append(check("info overshoot / undershoot: |Δ| ≤ 1e-6 %", ep, 1e-6))
    return out


# ---------------------------------------------------------------- control.design.lqr (parity.yaml: T2; mpmath: 1e-12)
def _lqr_matrices(req):
    A, B, Q, R = (arr(req[k]) for k in ("A", "B", "Q", "R"))
    N = arr(req["N"]) if req.get("N") is not None else np.zeros(B.shape)
    return A, B, Q, R, N


def rule_lqr(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    S, K, P = arr(r["S"]), arr(r["K"]), arr(r["closed_loop_poles"]).ravel()
    eS, eK, eP = arr(e["S"]), arr(e["K"]), arr(e["closed_loop_poles"]).ravel()
    A, B, Q, R, N = _lqr_matrices(req)
    # Same kappa-scaled tolerance for every reference engine (spec 12.1 item 23): the 50-digit mpmath values are a
    # more accurate reference, not a stricter requirement. "1e-12, not loosened" in parity.yaml is the reference
    # implementation's own target (cargo test parity_hp); a backward-stable solver without refinement does not meet
    # it on these families (SciPy: up to 3.7e-9, golden field scipy_forward_error_rel_max). Justified larger
    # deviations go to the declaration's known_deviations (CF-7).
    c = max(1.0, num(e["cond_S"]) / 1e2)                                   # cond_S of the reference solution
    tS, tK, tP = 1e-10 * c, 1e-9 * c, 1e-8 * c
    pairs = match(P, eP)
    pe = max((abs(g - w) / max(1.0, abs(w)) for w, g in pairs), default=0.0) if pairs is not None else np.inf
    out = [check("S: rel_fro ≤ 1e-10·max(1, cond_S/100)", rel_fro(S, eS), tS),
           check("K: rel_fro ≤ 1e-9·max(1, cond_S/100)", rel_fro(K, eK), tK),
           check("closed_loop_poles: matched set, |Δ|/max(1,|p|) ≤ 1e-8·max(1, cond_S/100)  [V-5]", pe, tP)]
    # independent invariants on the implementation's own S and K (CF-8, recommended): no reference value needed.
    # Residuals are normalised by the size of the terms (backward-error form), so that cond(R) up to 1e12 does not
    # turn the runner's own rounding into a failure; R is never inverted.
    if S.shape == A.shape and K.shape == (B.shape[1], A.shape[0]):
        n2 = lambda M: np.linalg.norm(M, 2) if M.size else 0.0
        res = A.T @ S + S @ A - K.T @ R @ K + Q                            # = CARE residual when R K = Bᵀ S + Nᵀ
        den = 2 * n2(A) * n2(S) + n2(K) ** 2 * n2(R) + n2(Q)
        out.append(check("CARE residual AᵀS + SA − KᵀRK + Q, recomputed by the runner: ‖·‖₂ / (2‖A‖‖S‖ + ‖K‖²‖R‖ + ‖Q‖) ≤ 1e-12",
                         n2(res) / max(den, 1e-300), 1e-12))
        BSN = B.T @ S + N.T
        out.append(check("K consistent with S: ‖RK − (BᵀS + Nᵀ)‖ / (‖R‖‖K‖ + ‖BᵀS + Nᵀ‖) ≤ 1e-12",
                         n2(R @ K - BSN) / max(n2(R) * n2(K) + n2(BSN), 1e-300), 1e-12))
        out.append(check("S symmetric: ‖S − Sᵀ‖/‖S‖ ≤ 1e-12", np.linalg.norm(S - S.T) / max(np.linalg.norm(S), 1e-300), 1e-12))
        ev = np.linalg.eigvals(A - B @ K)
        out.append(("A − BK is stable, recomputed by the runner", bool(np.all(ev.real < 0)), None, None, f"max Re = {ev.real.max():.3e}"))
    return out


# ---------------------------------------------------------------- control.lti.c2d (parity.yaml: T2·kappa)
def _c2d_kappa(req):
    """kappa from the reference request (parity.yaml): cond(I − αA) for tustin, e^{‖A‖₁·ts} for zoh/foh.
    A is the request's state matrix, or the controllable companion form of a tf denominator."""
    s = req["system"]
    if s["kind"] == "ss":
        A = arr(s["A"])
    elif s["kind"] == "tf":
        den = np.array(s["den"], dtype=float); den = den / den[0]
        n = len(den) - 1
        A = np.zeros((n, n))
        if n:
            A[0, :] = -den[1:]; A[1:, :-1] += np.eye(n - 1)
    else:
        p = arr(s["p"]).ravel()
        A = np.diag(p) if p.size else np.zeros((0, 0))
    ts = num(req["ts"]) if "ts" in req else num(req.get("sample_time"))
    method = req.get("method", "zoh")
    if A.size == 0:
        return 1.0
    if method in ("tustin", "bilinear"):
        alpha = ts / 2
        pw = req.get("prewarp_frequency")
        if pw is not None:
            wp_ = num(pw["value"] if isinstance(pw, dict) else pw)
            alpha = np.tan(wp_ * ts / 2) / wp_
        return max(1.0, float(np.linalg.cond(np.eye(A.shape[0]) - alpha * A)))
    return max(1.0, float(np.exp(np.linalg.norm(A, 1) * ts)))


def _tf_eval(num_, den_, z):
    return np.polyval(num_, z) / np.polyval(den_, z)


def _ss_eval(A, B, C, D, z):
    n = A.shape[0]
    return np.array([C @ np.linalg.solve(zz * np.eye(n) - A, B) + D if n else D for zz in z])


def _eval_points(poles, ts):
    """Runner-chosen points on the unit circle (z = e^{jθ}), at least 1e-3 away from every reference pole."""
    th = np.array([0.0, 0.05, 0.3, 0.9, 1.7, 2.6, np.pi])
    pts = np.exp(1j * th)
    if poles.size:
        pts = np.array([z for z in pts if np.min(np.abs(z - poles)) > 1e-3])
    return pts


def rule_c2d(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    g = r["system"]
    k = _c2d_kappa(req)
    base = 1e-10 * k
    out = []
    if "A" in e:                                                            # state-space result: compare invariants
        if g.get("kind") != "ss":
            return [fail("system.kind is ss, as for the reference", f"got {g.get('kind')}")]
        eA, eB, eC, eD = (arr(e[x]) for x in "ABCD")
        gA, gB, gC, gD = (arr(g[x]) for x in "ABCD")
        if gB.shape[1:] != eB.shape[1:] or gC.shape[:1] != eC.shape[:1] or gD.shape != eD.shape:
            return [fail("transfer-function values at runner-chosen z (realization-independent, V-5)", "input/output dimensions differ from the reference")]
        wpol = np.linalg.eigvals(eA) if eA.size else np.array([])
        z = _eval_points(wpol, None)
        He, Hg = _ss_eval(eA, eB, eC, eD, z), _ss_eval(gA, gB, gC, gD, z)
        # relative to the size of H, and scaled by how close the points come to the reference poles
        kz = max(1.0, float(np.max([np.linalg.cond(zz * np.eye(eA.shape[0]) - eA) for zz in z]))) if eA.size else 1.0
        out.append(check("transfer-function values at runner-chosen z on the unit circle: rel_fro ≤ 1e-10·κ·max cond(zI − A_ref)  [V-5]", rel_fro(Hg, He), base * kz))
        out.append(roots_check("discrete poles eig(A): matched set ≤ 1e-10·κ·max(1, κ_root)  [V-5]", np.linalg.eigvals(gA) if gA.size else np.array([]), wpol, base))
        out.append(check("D (feedthrough is realization-independent): rel_fro ≤ 1e-10·κ", rel_fro(gD, eD) if np.linalg.norm(eD) else float(np.linalg.norm(gD)), base))
    else:                                                                   # transfer function: coefficients, scale-normalised
        if g.get("kind") != "tf":
            return [fail("system.kind is tf, as for the reference", f"got {g.get('kind')}")]
        en, ed = np.array(e["num"], dtype=float), np.array(e["den"], dtype=float)
        gn, gd = np.array(g["num"], dtype=float), np.array(g["den"], dtype=float)
        if gd.size == 0 or gd[0] == 0:
            return [fail("tf coefficients", "denominator leading coefficient is zero")]
        en, ed, gn, gd = en / ed[0], ed / ed[0], gn / gd[0], gd / gd[0]  # num/den are defined only up to a common factor
        m = max(len(gn), len(en)); gn, en = np.pad(gn, (m - len(gn), 0)), np.pad(en, (m - len(en), 0))
        m = max(len(gd), len(ed)); gd, ed = np.pad(gd, (m - len(gd), 0)), np.pad(ed, (m - len(ed), 0))
        lam = np.roots(ed)
        l1 = np.sum(np.abs(ed))
        sc = max(1.0, float(np.prod(1 + np.abs(lam)) / l1))
        tol = base * sc
        out.append(check("tf numerator (den monic): ‖Δ‖₁/max(‖ref‖₁,‖den‖₁) ≤ 1e-10·κ·max(1, Π(1+|λ|)/‖den‖₁)",
                         np.sum(np.abs(gn - en)) / max(np.sum(np.abs(en)), l1), tol))
        out.append(check("tf denominator (monic): ‖Δ‖₁/‖den‖₁ ≤ 1e-10·κ·max(1, Π(1+|λ|)/‖den‖₁)", np.sum(np.abs(gd - ed)) / l1, tol))
    ts_ok = num(g.get("ts")) == num(req["ts"]) if "ts" in req else True
    out.append(("system.ts equals the requested sample time  [CF-6]", ts_ok, None, None, f"got {g.get('ts')}, requested {req.get('ts')}"))
    return out


# ---------------------------------------------------------------- control.lti.lsim (parity.yaml: T2)
def rule_lsim(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    if r.get("y") is None or r.get("t") is None:
        return [fail("y: shape [ny, nt]", "y or t is null (the reference returns all samples)")]
    gy, wy = arr(r["y"]), np.array(e["y"], dtype=float)
    gt, wt = arr(r["t"]).ravel(), np.array(e["t"], dtype=float)
    shape = tuple(e["shape"])
    if gy.shape != shape or gt.shape != wt.shape:
        return [fail("y: shape [ny, nt]", f"got y {gy.shape}, t {gt.shape}; reference {shape}, t {wt.shape}")]
    out = [check("y: max|Δ| / max(1, max|y_ref|) ≤ 1e-10", float(np.max(np.abs(gy.ravel() - wy))) / max(1.0, float(np.max(np.abs(wy)))), 1e-10),
           check("t: |Δ| ≤ 1e-12 s", float(np.max(np.abs(gt - wt))), 1e-12),
           ("n_grid_points equals the reference  [CF-6]", r.get("n_grid_points") == e["n_grid_points"], None, None, f"got {r.get('n_grid_points')}, reference {e['n_grid_points']}"),
           ("interpolation equals the reference (foh / zoh / none)  [CF-6]", r.get("interpolation") == e["interpolation"], None, None, f"got {r.get('interpolation')}, reference {e['interpolation']}")]
    if "x" in e:
        if r.get("x") is None:
            out.append(fail("x: states [n, nt] (return_states)", "x is null"))
        else:
            gx, wx = arr(r["x"]), np.array(e["x"], dtype=float)
            if gx.shape != tuple(e["x_shape"]):
                out.append(fail("x: states [n, nt] (return_states)", f"shape {gx.shape} != {tuple(e['x_shape'])}"))
            else:
                out.append(check("x: max|Δ| / max(1, max|x_ref|) ≤ 1e-10 (coordinates of the given ss)", float(np.max(np.abs(gx.ravel() - wx))) / max(1.0, float(np.max(np.abs(wx)))), 1e-10))
    return out


# ---------------------------------------------------------------- control.design.pidtune (parity.yaml: T2)
def _eval_lti(sysd, s):
    """SISO transfer value of an AEOI LtiSystem JSON at complex s (tf, zpk or ss)."""
    k = sysd["kind"]
    if k == "tf":
        num = np.array(sysd["num"], dtype=float).ravel() if not isinstance(sysd["num"][0], list) else np.array(sysd["num"][0][0], dtype=float)
        den = np.array(sysd["den"], dtype=float).ravel() if not isinstance(sysd["den"][0], list) else np.array(sysd["den"][0][0], dtype=float)
        return np.polyval(num, s) / np.polyval(den, s)
    if k == "zpk":
        z, p = arr(sysd["z"]).ravel().astype(complex), arr(sysd["p"]).ravel().astype(complex)
        return num(sysd["k"]) * np.prod(s - z) / np.prod(s - p)
    A, B, C, D = (arr(sysd[x]) for x in "ABCD")
    n = A.shape[0]
    return complex((C @ np.linalg.solve(s * np.eye(n) - A, B) + D)[0, 0]) if n else complex(D[0, 0])


def rule_pidtune(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    eg = max(abs(num(r["gains"][k]) - e["gains"][k]) / max(1.0, abs(e["gains"][k])) for k in ("Kp", "Ki", "Kd", "Tf"))
    a = e["achieved"]
    ea = max(abs(num(r["achieved"][k]) - a[k]) / max(1.0, abs(a[k])) for k in ("phase_margin_deg", "gain_crossover_rad_s"))
    out = [check("gains Kp, Ki, Kd, Tf: |Δ| / max(1, |ref|) ≤ 1e-10", eg, 1e-10),
           check("achieved phase_margin_deg, gain_crossover_rad_s: |Δ| / max(1, |ref|) ≤ 1e-6", ea, 1e-6),
           ("achieved.closed_loop_stable equals the reference  [CF-6]", r["achieved"].get("closed_loop_stable") is a["closed_loop_stable"], None, None, f"got {r['achieved'].get('closed_loop_stable')}"),
           ("type echoes the request", r.get("type") == req["type"], None, None, f"got {r.get('type')}")]
    # independent of the reference: L(jωc) from the request's plant and the RETURNED controller (CF-8)
    wc, pm = num(req["wc_rad_s"]), num(req.get("phase_margin_deg", 60.0))
    L = _eval_lti(req["plant"], 1j * wc) * _eval_lti(r["controller"], 1j * wc)
    dph = abs((np.degrees(np.angle(L)) - (pm - 180.0) + 180.0) % 360.0 - 180.0)
    out.append(check("|L(jωc)| = 1, recomputed by the runner from the returned controller: ||L| − 1| ≤ 1e-9", abs(abs(L) - 1.0), 1e-9))
    out.append(check("∠L(jωc) = PM − 180°, recomputed by the runner: |Δ| ≤ 1e-7 deg", dph, 1e-7))
    if req["type"] == "PID":
        st = r["gains_standard"]
        alpha = num(req.get("ti_td_ratio", 4.0))
        out.append(check("gains_standard.Ti = ti_td_ratio · Td (PT-4)", abs(num(st["Ti"]) - alpha * num(st["Td"])) / max(1.0, abs(num(st["Ti"]))), 1e-12))
    return out


RULES = {
    "control.lti.poles_zeros": rule_poles_zeros,
    "control.lti.bode": rule_bode,
    "control.lti.step": rule_step,
    "control.design.lqr": rule_lqr,
    "control.lti.c2d": rule_c2d,
    "control.lti.lsim": rule_lsim,
    "control.design.pidtune": rule_pidtune,
}
