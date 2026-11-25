"""Numeric comparison rules (spec 9.4 CF-6) for operators outside control: signal.spectral.fft (v0.4.7),
ode.ivp.solve (v0.4.8), linalg.system.solve and interp.grid.interp1 (v0.4.10).

Same ground rules as rules_control.py: tolerances from the suite (parity.yaml), every scale computed here from
the reference request or values, nothing the implementation reports is used.
"""
import base64
from fractions import Fraction
import math

import numpy as np

from rules_control import check, fail, num, arr

U = np.finfo(float).eps / 2


def decode(o):
    """Request array in any inline encoding: dense JSON (f64 / c128) or DenseMatrixB64 (f64 / c128)."""
    if isinstance(o, dict) and "data_b64" in o:
        raw = base64.b64decode(o["data_b64"])
        dt = np.dtype("<c16") if o.get("dtype") == "c128" else np.dtype("<f8")
        return np.frombuffer(raw, dtype=dt).reshape(o["shape"]).astype(complex)
    return np.asarray(arr(o), dtype=complex)


# ---------------------------------------------------------------- signal.spectral.fft (parity.yaml: T1)
def rule_fft(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    gX = arr(r["X"])
    shape = tuple(e["shape"])
    if gX.shape != shape:
        return [fail("X: shape [n_out] or [m, n_out]", f"got {gX.shape}, reference {shape}")]
    wX = np.array([complex(z[0], z[1]) for z in e["X"]]).reshape(shape)
    n = int(e["n"])
    tol = 1e-12 * (np.ceil(np.log2(max(n, 2))) + 1)
    g2, w2 = np.atleast_2d(gX), np.atleast_2d(wX)
    ex = max(np.linalg.norm(a - b) / max(np.linalg.norm(b), np.finfo(float).tiny) for a, b in zip(g2, w2))
    gf, wf = arr(r["frequency"]).ravel(), np.array(e["frequency"], dtype=float)
    ef = float(np.max(np.where(wf == 0, np.abs(gf - wf) / np.finfo(float).tiny * (gf != wf), np.abs(gf - wf) / np.maximum(np.abs(wf), 1e-300)))) if gf.shape == wf.shape else np.inf
    out = [check("X: per channel ‖ΔX‖₂ / ‖X_ref‖₂ ≤ 1e-12·(⌈log2 n⌉ + 1)", ex, tol),
           check("frequency: |Δ| / |f_ref| ≤ 4ε", ef, 4 * np.finfo(float).eps),
           ("frequency.unit equals the reference", r["frequency"].get("unit") == e["frequency_unit"], None, None, f"got {r['frequency'].get('unit')}")]
    for k in ("n", "input_length", "direction", "normalization", "sides"):
        out.append((f"{k} equals the reference  [CF-6, O-1 echo]", r.get(k) == e[k], None, None, f"got {r.get(k)!r}, reference {e[k]!r}"))
    # Parseval, recomputed by the runner from the request's x and the returned X (no reference value)
    x = np.atleast_2d(decode(req["x"]))
    xp = np.zeros((x.shape[0], n), dtype=complex)
    m_ = min(n, x.shape[1]); xp[:, :m_] = x[:, :m_]
    inv = e["direction"] == "inverse"; norm = e["normalization"]
    sc = {"ortho": 1 / np.sqrt(n)}.get(norm, 1.0 / n if (norm == "forward") != inv else 1.0)
    one = e["sides"] == "one"
    worst = 0.0
    for xi, Xi in zip(xp, g2):
        wgt = np.ones(Xi.shape[0])
        if one:
            wgt[1:] = 2.0
            if n % 2 == 0: wgt[-1] = 1.0
        want = sc * sc * n * np.sum(np.abs(xi) ** 2)
        got = np.sum(wgt * np.abs(Xi) ** 2)
        worst = max(worst, abs(got - want) / want if want > 0 else abs(got))
    out.append(check("Parseval, recomputed by the runner from x and the returned X ≤ 64·(⌈log2 n⌉ + 1)·u", worst, 64 * (np.ceil(np.log2(max(n, 2))) + 1) * U))
    return out


# ---------------------------------------------------------------- ode.ivp.solve (parity.yaml: T4)
def rule_ode(case, env, manifest, req=None, expected=None):
    """Against the truth (analytic, or DOP853 at rtol 1e-13), not against the reference RK45's step sequence: two
    correct RK45 runs part as soon as one step decision differs (kinks, chaos, long horizons). E_ref = the reference
    RK45's own global error on the case; rtol/atol come from the request (defaults 1e-3 / 1e-6, spec 10.18)."""
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    gy, gt = arr(r["y"]), arr(r["t"]).ravel()
    shape = tuple(e["shape"]); wt = np.array(e["t"], dtype=float); truth = np.array(e["truth"], dtype=float).reshape(shape)
    if gy.shape != shape or gt.shape != wt.shape:
        return [fail("y: shape [n_states, n_out]", f"got y {gy.shape}, t {gt.shape}; reference {shape}, t {wt.shape}")]
    rtol = float(req.get("rtol", 1e-3)); atol = float(req.get("atol", 1e-6))
    e_ref = float(e["scipy_global_error_max_abs"]); scale = max(1.0, float(np.max(np.abs(truth))))
    bound = np.maximum(10 * (atol + rtol * np.abs(truth)), 2 * e_ref + 1e-13 * scale)
    tscale = max(1.0, float(np.max(np.abs(wt))))
    out = [check("y vs truth: max |y − truth| / max(10·(atol + rtol·|truth|), 2·E_ref + 1e-13·max|y|) ≤ 1  [T4]", float(np.max(np.abs(gy - truth) / bound)), 1.0),
           check("t: |Δ| ≤ 4ε·max(1, |t|)", float(np.max(np.abs(gt - wt))) / tscale, 4 * np.finfo(float).eps),
           ("states echo the request, in order  [O-1]", r.get("states") == list(req["states"]), None, None, f"got {r.get('states')}"),
           ("tolerance echoes the effective rtol / atol  [O-1]", (r.get("tolerance") or {}).get("rtol") == rtol and (r.get("tolerance") or {}).get("atol") == atol, None, None, f"got {r.get('tolerance')}, effective rtol={rtol}, atol={atol}")]
    # evidence honesty, measured by the runner against the truth: a tolerance_consistent claim must not hide an
    # error far beyond what the claim allows (claim: |y − y_fine| ≤ 100·tol; ×10 margin for the estimate's own error)
    claim = ((env.get("trust") or {}).get("checks") or {}).get("tolerance_consistent")
    if claim is True:
        act = float(np.max(np.abs(gy - truth) / (atol + rtol * np.abs(truth))))
        out.append(check("tolerance_consistent = true ⇒ max |y − truth| / (atol + rtol·|truth|) ≤ 1000, recomputed by the runner  [E-5]", act, 1000.0))
    return out


# ---------------------------------------------------------------- linalg.system.solve (parity.yaml: T1, κ-scaled)
def _exact_backward_error(A, b, x, square):
    """Backward error of the returned x, from the request, in exact rational arithmetic (float only at the end)."""
    m, n = A.shape
    Af = [[Fraction(float(v)) for v in row] for row in A]
    worst = 0.0
    for j in range(b.shape[1]):
        xf = [Fraction(float(v)) for v in x[:, j]]
        bf = [Fraction(float(v)) for v in b[:, j]]
        r = [bf[i] - sum(Af[i][k] * xf[k] for k in range(n)) for i in range(m)]
        if square:
            num_ = max(abs(v) for v in r)
            den = max(sum(abs(v) for v in row) for row in Af) * max(abs(v) for v in xf) + max(abs(v) for v in bf)
            eta = float(num_ / den) if den else 0.0
        else:
            atr = [sum(Af[i][k] * r[i] for i in range(m)) for k in range(n)]
            afro = math.sqrt(float(sum(v * v for row in Af for v in row)))
            rn = math.sqrt(float(sum(v * v for v in r))); xn = math.sqrt(float(sum(v * v for v in xf)))
            den = afro * (rn + afro * xn)
            eta = math.sqrt(float(sum(v * v for v in atr))) / den if den else 0.0
        worst = max(worst, eta)
    return worst


def rule_linsolve(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    gx = arr(r["x"]); shape = tuple(e["shape"])
    if gx.shape != shape:
        return [fail("x: shape [n] or [n, k]", f"got {gx.shape}, reference {shape}")]
    A = arr(req["A"]); b = arr(req["b"]); B = b.reshape(-1, 1) if b.ndim == 1 else b
    m, n = A.shape
    truth = np.array(e["x_true"], dtype=float).reshape(shape)
    kappa = float(e["kappa"])
    tol = 1e-12 * max(1.0, kappa / 100.0)
    fe = float(np.max(np.abs(gx - truth)) / max(np.max(np.abs(truth)), np.finfo(float).tiny))
    claims = (env.get("trust") or {}).get("checks") or {}
    out = [("method echoes the reference method  [10.19]", r.get("method") == e["method"], None, None, f"got {r.get('method')}, want {e['method']}")]
    if case.get("expect_postcondition_failed") and claims.get("backward_stable") is False:
        out.append(("flagged case: backward_stable = false and /x listed unreliable (accuracy not required)  [E-13]", True, None, None, ""))
    else:
        out.append(check("x vs exact answer: ‖Δx‖∞/‖x_true‖∞ ≤ 1e-12·max(1, κ/100)  [T1, V-6]", fe, tol))
    if claims.get("backward_stable") is True:
        X = gx.reshape(n, -1)
        eta = _exact_backward_error(A, B, X, m == n)
        thr = 10 * (n if m == n else m) * U
        out.append(check("backward_stable = true ⇒ backward error ≤ 10·n·u (square) / 10·m·u (least squares), recomputed exactly by the runner  [E-5]", eta, thr))
    return out


# ---------------------------------------------------------------- interp.grid.interp1 (parity.yaml: T1)
def rule_interp1(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    g = arr(r["yq"]); shape = tuple(e["shape"])
    if g.shape != shape:
        return [fail("yq: shape [q] or [k, q]", f"got {g.shape}, reference {shape}")]
    want = np.array([np.nan if v == "NaN" else v for v in e["yq"]], dtype=float).reshape(shape)
    scale = max(1.0, float(e["y_scale"]))
    nan_same = bool(np.array_equal(np.isnan(g), np.isnan(want)))
    d = float(np.nanmax(np.abs(g - want))) / scale if np.any(~np.isnan(want)) else 0.0
    x = arr(req["x"]).ravel(); Y = np.atleast_2d(arr(req["y"])); xq = arr(req["xq"]).ravel()
    method = req.get("method", "linear"); extrap = req.get("extrapolation", "error")
    outside = (xq < x[0]) | (xq > x[-1])
    G = np.atleast_2d(g)
    # recomputed from the request: node hits, interval bounds (not spline), clamp values
    viol = 0
    for yy, gg in zip(Y, G):
        for t, v in enumerate(xq):
            if outside[t]:
                if extrap == "clamp" and gg[t] != (yy[0] if v < x[0] else yy[-1]):
                    viol += 1
                continue
            hit = np.nonzero(x == v)[0]
            if hit.size and gg[t] != yy[hit[0]]:
                viol += 1
            elif not hit.size and method != "spline":
                k = min(max(int(np.searchsorted(x, v, side="right")) - 1, 0), len(x) - 2)
                lo, hi = min(yy[k], yy[k + 1]), max(yy[k], yy[k + 1])
                slack = 4 * U * max(abs(lo), abs(hi))
                if not (lo - slack <= gg[t] <= hi + slack):
                    viol += 1
    return [("yq: NaN at the same places as the reference", nan_same, None, None, ""),
            check("yq: |Δ| ≤ 1e-12·max(1, max|y|)  [T1]", d, 1e-12),
            ("method and extrapolation echo the request  [O-1]", r.get("method") == method and r.get("extrapolation") == extrap, None, None, f"got {r.get('method')}, {r.get('extrapolation')}"),
            ("out_of_range_count recomputed from x and xq", r.get("out_of_range_count") == int(outside.sum()), None, None, f"got {r.get('out_of_range_count')}, want {int(outside.sum())}"),
            check("node hits exact, values within the interval's end values (not spline), clamp outside — recomputed from the request", float(viol), 0.0)]


RULES = {"signal.spectral.fft": rule_fft, "ode.ivp.solve": rule_ode, "linalg.system.solve": rule_linsolve, "interp.grid.interp1": rule_interp1}
