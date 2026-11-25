"""Numeric comparison rules (spec 9.4 CF-6) for operators outside control: signal.spectral.fft (v0.4.7),
ode.ivp.solve (v0.4.8), linalg.system.solve and interp.grid.interp1 (v0.4.10), fit.poly.polyfit,
linalg.decomp.eig and linalg.decomp.svd (v0.4.11).

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


# ---------------------------------------------------------------- fit.poly.polyfit (parity.yaml: T1)
def rule_polyfit(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    d = int(req["degree"]); c = d + 1
    gp = arr(r["p"]).ravel()
    if gp.shape != (c,):
        return [fail("p: shape [degree + 1]", f"got {gp.shape}")]
    truth = np.array(e["p_true"], dtype=float); cn = np.array(e["col_norms"], dtype=float)
    kappa = float(e["kappa_scaled_2"])
    fe = float(np.linalg.norm(cn * (gp - truth)) / max(np.linalg.norm(cn * truth), np.finfo(float).tiny))
    scaling = req.get("scaling", "none")
    out = [check("p vs exact answer: ‖D(p − p*)‖₂/‖D p*‖₂ ≤ 1e-12·max(1, κ/100)  [T1, V-6]", fe, 1e-12 * max(1.0, kappa / 100.0)),
           ("degree, scaling and dof echo the request  [O-1]", r.get("degree") == d and r.get("scaling") == scaling and r.get("dof") == e["dof"], None, None, f"got {r.get('degree')}, {r.get('scaling')}, {r.get('dof')}")]
    x = arr(req["x"]).ravel(); y = arr(req["y"]).ravel()
    if scaling == "center_scale":
        mu = r.get("mu") or [float("nan"), float("nan")]
        mt = e["mu_true"]
        dmu = max(abs(float(mu[0]) - mt[0]), abs(float(mu[1]) - mt[1])) / max(abs(mt[0]), mt[1])
        out.append(check("mu vs exact mean and sample std  [10.21 PF-3]", dmu, 4e-16))
        t = (x - float(mu[0])) / float(mu[1])
    else:
        out.append(("mu is null without scaling", r.get("mu") is None, None, None, f"got {r.get('mu')}"))
        t = x
    if ((env.get("trust") or {}).get("checks") or {}).get("backward_stable") is True and np.all(np.isfinite(gp)):
        # exact rational residual of the normal equations for the returned p, in the basis t the spec defines
        tf = [Fraction(float(v)) for v in t]; yf = [Fraction(float(v)) for v in y]; pf = [Fraction(float(v)) for v in gp]
        V = [[ti ** (d - j) for j in range(c)] for ti in tf]
        res = [yf[i] - sum(V[i][j] * pf[j] for j in range(c)) for i in range(len(tf))]
        vtr = [sum(V[i][j] * res[i] for i in range(len(tf))) for j in range(c)]
        D = [math.sqrt(float(sum(V[i][j] ** 2 for i in range(len(tf))))) for j in range(c)]
        num_ = math.sqrt(sum((float(vtr[j]) / D[j]) ** 2 for j in range(c)))
        rn = math.sqrt(float(sum(v * v for v in res))); dp = math.sqrt(sum((float(pf[j]) * D[j]) ** 2 for j in range(c)))
        den = math.sqrt(c) * (rn + math.sqrt(c) * dp)
        eta = num_ / den if den else 0.0
        out.append(check("backward_stable = true ⇒ ‖D⁻¹Vᵀr‖₂ ≤ 10·n·u·‖V·D⁻¹‖_F·(‖r‖₂ + ‖V·D⁻¹‖_F‖D p‖₂), recomputed exactly by the runner  [E-5]", eta, 10 * len(x) * U))
    return out


# ---------------------------------------------------------------- linalg.decomp.eig (parity.yaml: T2)
def _eig_key(z):
    return (z.real, abs(z.imag), z.imag < 0)


def rule_eig(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    A = arr(req["A"]); n = A.shape[0]
    lam = arr(r["values"]).astype(complex).ravel()
    if lam.shape != (n,):
        return [fail("values: n eigenvalues", f"got {lam.shape}")]
    truth = np.array([complex(a, b) for a, b in e["values_true"]])
    cond = [float("inf") if v == "Infinity" else float(v) for v in e["eigenvalue_condition_true"]]
    anorm = max(float(e["norm_fro"]), np.finfo(float).tiny)
    ordered = all(_eig_key(lam[k - 1]) <= _eig_key(lam[k]) for k in range(1, n))
    used = set(); worst = 0.0
    for k in range(n):
        j = min((j for j in range(n) if j not in used), key=lambda j: abs(lam[k] - truth[j])); used.add(j)
        tol = 1e-4 * anorm if not cond[j] <= 1e8 else 1e-12 * anorm * max(1.0, cond[j])
        worst = max(worst, abs(lam[k] - truth[j]) / tol)
    out = [("structure echoes the reference  [10.22 EG-1]", r.get("structure") == e["structure"], None, None, f"got {r.get('structure')}"),
           ("ordering: ascending (symmetric) / (Re, |Im|) with conjugate pairs Im > 0 first  [10.22 EG-3]", ordered, None, None, str(lam)),
           check("values vs exact eigenvalues: |Δλ_k| ≤ 1e-12·‖A‖_F·max(1, cond_k)  [T2]", worst, 1.0)]
    if r.get("vectors") is not None:
        Vm = arr(r["vectors"]).astype(complex).reshape(n, n)
        res = 0.0; norm_ok = True
        for k in range(n):
            v = Vm[:, k]
            av = [complex(math.fsum((A[i, c] * v[c]).real for c in range(n)) - (lam[k] * v[i]).real,
                          math.fsum((A[i, c] * v[c]).imag for c in range(n)) - (lam[k] * v[i]).imag) for i in range(n)]
            res = max(res, math.sqrt(sum(abs(z) ** 2 for z in av)) / anorm)
            big = int(np.argmax(np.abs(v)))
            norm_ok &= abs(np.linalg.norm(v) - 1) < 1e-14 and v[big].imag == 0 and v[big].real > 0
        out.append(check("‖A·v_k − λ_k·v_k‖₂/‖A‖_F ≤ 100·n·u for every pair, recomputed by the runner  [10.22 EG-5]", res, 100 * n * U))
        out.append(("vectors: ‖v‖₂ = 1, largest-modulus entry real positive  [10.22 EG-4]", bool(norm_ok), None, None, ""))
        if e["structure"] == "symmetric":
            G = Vm.conj().T @ Vm - np.eye(n)
            out.append(check("symmetric: ‖VᵀV − I‖_max ≤ 100·n·u, recomputed  [10.22 EG-5]", float(np.max(np.abs(G))), 100 * n * U))
    return out


# ---------------------------------------------------------------- linalg.decomp.svd (parity.yaml: T1)
def rule_svd(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    A = arr(req["A"]); m, n = A.shape; p = min(m, n)
    s = arr(r["s"]).ravel(); st = np.array(e["s_true"], dtype=float)
    if s.shape != (p,):
        return [fail("s: min(m, n) singular values", f"got {s.shape}")]
    smax = float(st[0])
    mode = req.get("mode", "thin")
    out = [("s descending", bool(np.all(np.diff(s) <= 0)), None, None, str(s)),
           check("s vs exact singular values: |Δs_k| ≤ 1e-12·s_max  [T1]", float(np.max(np.abs(s - st))), 1e-12 * smax),
           ("mode and rank echo / match the reference (MATLAB tolerance)  [10.23 SV-4]", r.get("mode") == mode and r.get("rank") == e["rank"], None, None, f"got {r.get('mode')}, rank {r.get('rank')}, want {e['rank']}")]
    if mode != "values":
        Uf = arr(r["U"]); Vf = arr(r["V"])
        uc, vc = (m, n) if mode == "full" else (p, p)
        if Uf.shape != (m, uc) or Vf.shape != (n, vc):
            return out + [fail("U, V shapes by mode", f"got {Uf.shape}, {Vf.shape}")]
        R = A - Uf[:, :p] @ np.diag(s) @ Vf[:, :p].T
        rec = float(np.linalg.norm(R) / max(np.linalg.norm(A), np.finfo(float).tiny))
        thr = 100 * max(m, n) * U
        out.append(check("‖A − U·diag(s)·Vᵀ‖_F/‖A‖_F ≤ 100·max(m, n)·u, recomputed by the runner  [10.23 SV-5]", rec, thr))
        out.append(check("‖UᵀU − I‖_max, ‖VᵀV − I‖_max ≤ 100·max(m, n)·u, recomputed  [10.23 SV-5]", float(max(np.max(np.abs(Uf.T @ Uf - np.eye(uc))), np.max(np.abs(Vf.T @ Vf - np.eye(vc))))), thr))
        sign_ok = all(Uf[int(np.argmax(np.abs(Uf[:, k]))), k] >= 0 for k in range(p))
        out.append(("sign rule: largest-modulus entry of u_k positive  [10.23 SV-3]", bool(sign_ok), None, None, ""))
    return out


RULES = {"signal.spectral.fft": rule_fft, "ode.ivp.solve": rule_ode, "linalg.system.solve": rule_linsolve, "interp.grid.interp1": rule_interp1,
         "fit.poly.polyfit": rule_polyfit, "linalg.decomp.eig": rule_eig, "linalg.decomp.svd": rule_svd}
