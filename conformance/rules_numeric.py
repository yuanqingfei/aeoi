"""Numeric comparison rules (spec 9.4 CF-6) for operators outside control: signal.spectral.fft (v0.4.7),
ode.ivp.solve (v0.4.8).

Same ground rules as rules_control.py: tolerances from the suite (parity.yaml), every scale computed here from
the reference request or values, nothing the implementation reports is used.
"""
import base64

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


RULES = {"signal.spectral.fft": rule_fft, "ode.ivp.solve": rule_ode}
