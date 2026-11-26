"""Numeric comparison rules (spec 9.4 CF-6) for operators outside control: signal.spectral.fft (v0.4.7),
ode.ivp.solve (v0.4.8), linalg.system.solve and interp.grid.interp1 (v0.4.10), fit.poly.polyfit,
linalg.decomp.eig and linalg.decomp.svd (v0.4.11), signal.design.butter and signal.filter.filter (v0.4.12), nonlinear.root.fzero and nonlinear.system.fsolve (v0.4.13), optim.linear.linprog, optim.local.fmincon and
optim.local.fminsearch (v0.4.14), calculus.quadrature.integral and calculus.quadrature.trapz (v0.4.15), fit.poly.polyval and fit.poly.roots (v0.4.20).

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


# ---------------------------------------------------------------- signal.design.butter (parity.yaml: T1)
def _cl(v):
    return np.array([complex(num(a), num(b)) for a, b in v], dtype=complex)


def _set_err(got, want):
    used = set(); worst = 0.0
    for g in got:
        j = min((j for j in range(len(want)) if j not in used), key=lambda j: abs(g - want[j])); used.add(j)
        worst = max(worst, abs(g - want[j]) / max(1.0, abs(want[j])))
    return worst


def rule_butter(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    z, p = _cl(r["z"]), _cl(r["p"]); k = float(num(r["k"]))
    if len(z) != len(e["z_true"]) or len(p) != len(e["p_true"]):
        return [fail("z, p counts", f"got {len(z)}, {len(p)}")]
    zt = np.array([complex(a, b) for a, b in e["z_true"]]); pt = np.array([complex(a, b) for a, b in e["p_true"]])
    err = max(_set_err(z, zt), _set_err(p, pt), abs(k - e["k_true"]) / abs(e["k_true"]))
    analog = e["analog"]; edges = e["edges"]
    if analog:
        H = lambda x: k * np.prod(x - z) / np.prod(x - p)
        pts = [1j * w for w in edges]
    else:
        S = arr(r["sos"]).reshape(-1, 6)
        H = lambda x: np.prod([np.polyval(s[2::-1], 1 / x) / np.polyval(s[:2:-1], 1 / x) for s in S])
        pts = [np.exp(1j * np.pi * w) for w in edges]
    gain = max(abs(abs(H(x)) ** 2 - 0.5) for x in pts)
    # b/a deviation recomputed by the runner from the returned b, a and z, p, k
    b = np.array([num(v) for v in r["b"]]); a = np.array([num(v) for v in r["a"]])
    grid = (1j * np.prod(edges) ** (1 / len(edges)) * 10 ** np.linspace(-2, 2, 64)) if analog else np.exp(1j * np.pi * (np.arange(64) + 0.5) / 64)
    Hz = np.array([k * np.prod(x - z) / np.prod(x - p) for x in grid])
    Hb = np.array([np.polyval(b, x) / np.polyval(a, x) if analog else np.polyval(b[::-1], 1 / x) / np.polyval(a[::-1], 1 / x) for x in grid])
    dev = float(np.max(np.abs(Hb - Hz)) / np.max(np.abs(Hz)))
    warned = any(w.get("code") == "W.NUMERICAL.ILL_CONDITIONED" for w in env.get("warnings") or [])
    return [check("z, p, k vs the exact construction (sets, relative)  [T1]", err, 1e-12),
            ("type, order, domain echo  [O-1]", r.get("type") == e["type"] and r.get("order") == req["order"] and r.get("domain") == ("analog" if analog else "digital"), None, None, f"got {r.get('type')}, {r.get('order')}, {r.get('domain')}"),
            check("|H(cutoff)|² = 1/2 at every edge, recomputed from the returned sos (digital) / zpk (analog)  [10.24 BW-5]", gain, 1e-9),
            ("W.NUMERICAL.ILL_CONDITIONED exactly when the b/a response deviates > 1e-8 (recomputed by the runner)  [10.24 BW-4]", warned == (dev > 1e-8), None, None, f"deviation {dev:.2e}, warned {warned}")]


# ---------------------------------------------------------------- signal.filter.filter (parity.yaml: T1)
def rule_filter(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    y = arr(r["y"])
    if list(y.shape) != e["y_shape"]:
        return [fail("y: shape of x", f"got {y.shape}, want {e['y_shape']}")]
    yt = np.array(e["y_true"]); zf = arr(r["zf"]).ravel(); zt = np.array(e["zf_true"])
    rad = e["max_pole_radius"]; amp = 1 / (1 - rad) if rad < 1 else len(yt)
    scale = max(float(np.max(np.abs(yt))), np.finfo(float).tiny)
    err = max(float(np.max(np.abs(y.ravel() - yt))), float(np.max(np.abs(zf - zt))) if zt.size else 0.0) / scale
    out = [check("y, zf vs the exact difference equation: ≤ 1e-13·max(1, 1/(1 − r))·max|y*|  [T1]", err, 1e-13 * max(1.0, amp))]
    if "b" in req and "zi" not in req:
        x = np.atleast_2d(arr(req["x"])); Y = np.atleast_2d(y)
        b = np.array(req["b"], float); a = np.array(req["a"], float); m = max(len(b), len(a))
        b = b / a[0]; a = a / a[0]; worst = 0.0
        for xc, yc in zip(x, Y):
            for n_ in range(len(xc)):
                terms = [a[i] * yc[n_ - i] for i in range(len(a)) if i <= n_] + [-b[i] * xc[n_ - i] for i in range(len(b)) if i <= n_]
                sc = math.fsum(abs(t) for t in terms)
                if sc:
                    worst = max(worst, abs(math.fsum(terms)) / sc)
        out.append(check("difference-equation residual of the returned y, recomputed by the runner  [10.25 FL-4]", worst, 10 * m * U))
    return out


# ---------------------------------------------------------------- nonlinear.root.fzero / nonlinear.system.fsolve (T1)
_PYNS = {"sin": math.sin, "cos": math.cos, "tan": math.tan, "asin": math.asin, "acos": math.acos, "atan": math.atan, "atan2": math.atan2,
         "sinh": math.sinh, "cosh": math.cosh, "tanh": math.tanh, "exp": math.exp, "log": math.log, "log10": math.log10, "sqrt": math.sqrt,
         "abs": abs, "min": min, "max": max, "hypot": math.hypot, "sign": lambda v: (v > 0) - (v < 0), "pi": math.pi, "__builtins__": {}}


def _expr(src, names, params):
    """The runner's own evaluator for 3.15 expressions (independent of the implementation): Python floats."""
    code = compile(src.replace("^", "**"), "<aeoi-expr>", "eval")
    return lambda *xs: eval(code, dict(_PYNS), {**params, **dict(zip(names, xs))})


def rule_fzero(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    x = float(num(r["x"])); roots = e["roots_true"]
    err = min(abs(x - t) / (4 * np.finfo(float).eps * abs(t) + 1e-300) for t in roots)
    f = _expr(req["expression"], [req.get("variable", "x")], req.get("params", {}))
    lo, hi = (float(num(v)) for v in r["bracket"]["final"])
    flo, fhi = f(lo), f(hi)
    flagged = bool(case.get("expect_postcondition_failed"))
    searched = any(w.get("code") == "W.DOMAIN.BRACKET_SEARCHED" for w in env.get("warnings") or [])
    out = [("W.DOMAIN.BRACKET_SEARCHED exactly when only x0 was given  [10.26 FZ-1]", searched == ("x0" in req), None, None, f"warned {searched}"),
           ("final bracket encloses x and f changes sign across it, recomputed by the runner  [10.26 FZ-4]", lo <= x <= hi and (flo == 0 or fhi == 0 or (flo > 0) != (fhi > 0)), None, None, f"[{lo}, {hi}], f = {flo:.3e}, {fhi:.3e}")]
    if not flagged:
        out.append(check("x vs the nearest exact root: |Δ| ≤ 4ε|x*|  [T1]", err, 1.0))
    return out


def rule_fsolve(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    x = [float(num(v)) for v in r["x"]]
    alts = e.get("x_true_alternatives") or [e["x_true"]]
    err = min(max(abs(a - b) for a, b in zip(x, t)) / max(max(abs(v) for v in t), 1e-300) for t in alts)
    warned = any(w.get("code") == "W.NUMERICAL.ILL_CONDITIONED" for w in env.get("warnings") or [])
    F = [_expr(s_, req["unknowns"], req.get("params", {}))(*x) for s_ in req["equations"]]
    fr = [float(num(v)) for v in r["residual"]]
    fn = max(abs(v) for v in F)
    return [check("x vs the exact root: ‖Δ‖∞/‖x*‖∞ ≤ 1e-14 (1e-7 when flagged as a possibly multiple root)  [T1]", err, 1e-7 if warned else 1e-14),
            ("solution names echo unknowns  [O-1]", list(r.get("solution", {}).keys()) == req["unknowns"], None, None, ""),
            check("‖F(x)‖∞ recomputed by the runner ≤ 1e-10·(1 + ‖x‖∞) and equal to the returned residual  [10.27 FS-5]", max(fn / (1 + max(abs(v) for v in x)), max(abs(a - b) for a, b in zip(F, fr)) * 1e6), 1e-10)]


# ---------------------------------------------------------------- optim.* (v0.4.14)
def _bnd(v, n, default):
    return [default if v is None else (float("inf") if x == "Inf" else float("-inf") if x == "-Inf" else float(x)) for x in (v or [None] * n)]


def rule_linprog(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    c = np.array(req["c"], float); n = len(c); x = np.array([num(v) for v in r["x"]])
    A = np.array(req.get("A", []), float).reshape(-1, n); b = np.array(req.get("b", []), float)
    Ae = np.array(req.get("Aeq", []), float).reshape(-1, n); be = np.array(req.get("beq", []), float)
    lb = np.array(_bnd(req.get("lb"), n, float("-inf"))); ub = np.array(_bnd(req.get("ub"), n, float("inf")))
    L = r["lambda"]; li = np.array([num(v) for v in L["ineqlin"]]); le = np.array([num(v) for v in L["eqlin"]])
    llo = np.array([num(v) for v in L["lower"]]); lup = np.array([num(v) for v in L["upper"]])
    sc = max(1.0, float(np.max(np.abs(np.r_[b, be, c]))) if (b.size + be.size + c.size) else 1.0)
    prim = max([0.0] + list(A @ x - b) + list(np.abs(Ae @ x - be)) + list(lb - x) + list(x - ub)) / sc
    stat = float(np.max(np.abs(c + A.T @ li + Ae.T @ le - llo + lup))) / max(1.0, float(np.max(np.abs(c))))
    neg = float(max([0.0] + list(-li) + list(-llo) + list(-lup)))
    fe = abs(float(num(r["fval"])) - e["fval"]) / max(1.0, abs(e["fval"]))
    out = [check("fval vs HiGHS: |Δ| ≤ 1e-9·max(1, |f*|)  [T1]", fe, 1e-9),
           check("KKT recomputed by the runner from the request and the returned x, λ (feasibility, stationarity, λ ≥ 0)  [10.28 LP-4]", max(prim, stat, neg), 1e-8)]
    if e.get("unique"):
        want = np.array(e["x_exact"] if e.get("x_exact") else e["x_highs"])
        out.append(check("x vs the unique optimal vertex  [T1]", float(np.max(np.abs(x - want)) / max(1.0, np.max(np.abs(want)))), 1e-9))
    return out


def rule_fmincon(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    names = req["unknowns"]; P = req.get("params", {}); x = np.array([num(v) for v in r["x"]], float); n = len(x)
    want = np.array(e["x_exact"] if e.get("x_exact") else e["x_slsqp"])
    ex = float(np.max(np.abs(x - want)) / max(1.0, np.max(np.abs(want))))
    gi = [_expr(s_, names, P) for s_ in req.get("nonlinear_ineq", [])]; ge = [_expr(s_, names, P) for s_ in req.get("nonlinear_eq", [])]
    lb = _bnd(req.get("lb"), n, float("-inf")); ub = _bnd(req.get("ub"), n, float("inf"))
    viol = max([0.0] + [g(*x) for g in gi] + [abs(h(*x)) for h in ge] + [lb[j] - x[j] for j in range(n)] + [x[j] - ub[j] for j in range(n)]
               + [float(np.dot(row, x) - bi) for row, bi in zip(req.get("A", []), req.get("b", []))] + [abs(float(np.dot(row, x) - bi)) for row, bi in zip(req.get("Aeq", []), req.get("beq", []))])
    # stationarity of the Lagrangian with central differences
    f = _expr(req["objective"], names, P); L = r["lambda"]
    lam = [float(num(v)) for v in L["ineqnonlin"]]; nu = [float(num(v)) for v in L["eqnonlin"]]
    li = [float(num(v)) for v in L["ineqlin"]]; le = [float(num(v)) for v in L["eqlin"]]
    lo = [float(num(v)) for v in L["lower"]]; up = [float(num(v)) for v in L["upper"]]
    def grad(fun):
        g = []
        for j in range(n):
            h = 1e-6 * max(1.0, abs(x[j])); xp = x.copy(); xm = x.copy(); xp[j] += h; xm[j] -= h
            g.append((fun(*xp) - fun(*xm)) / (2 * h))
        return np.array(g)
    gl = grad(f)
    for l, g in zip(lam, gi): gl = gl + l * grad(g)
    for l, h in zip(nu, ge): gl = gl + l * grad(h)
    for l, row in zip(li, req.get("A", [])): gl = gl + l * np.array(row)
    for l, row in zip(le, req.get("Aeq", [])): gl = gl + l * np.array(row)
    gl = gl - np.array(lo) + np.array(up)
    stat = float(np.max(np.abs(gl))) / max(1.0, float(np.max(np.abs(grad(f)))))
    return [check("x vs the analytic optimum (else SLSQP): ≤ 1e-6 relative  [T2]", ex, 1e-6),
            check("constraint violation recomputed by the runner (own evaluator)  [10.29 FC-5]", viol, 1e-7),
            check("Lagrangian stationarity with the returned multipliers, central differences  [10.29 FC-5]", stat, 1e-5)]


def rule_fminsearch(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    names = req["unknowns"]; f = _expr(req["objective"], names, req.get("params", {}))
    x = [float(num(v)) for v in r["x"]]; want = e["x_exact"]; tx = req.get("tol_x", 1e-4); tf = req.get("tol_fun", 1e-4)
    ex = max(abs(a - b) for a, b in zip(x, want)) / max(1.0, max(abs(v) for v in want))
    fx = f(*x); worst = 0.0
    for j in range(len(x)):
        h = tx * max(1.0, abs(x[j]))
        for sg in (-1, 1):
            y = list(x); y[j] += sg * h; worst = max(worst, fx - f(*y))
    return [check("x vs the analytic optimum: ≤ 10·tol_x relative  [T3]", ex, 10 * tx),
            check("probe recomputed by the runner: f(x) ≤ f(x ± h·e_j) + tol_fun  [10.30 NM-4]", worst, tf)]


# ---------------------------------------------------------------- calculus.quadrature.* (v0.4.15, T1)
def rule_integral(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    v = float(num(r["value"])); t = e["value_true"]; est = float(num(r["error_estimate"]))
    tol = max(e["abs_tol"], e["rel_tol"] * abs(t), 4 * np.finfo(float).eps * abs(t))
    tl = r.get("tolerances", {})
    return [check("value vs the 50-digit truth: |Δ| ≤ max(abs_tol, rel_tol·|I*|)  [T1]", abs(v - t) / tol, 1.0),
            ("tolerances echo the request or the defaults (1e-10, 1e-12)  [10.31 IN-2]", tl.get("rel_tol") == req.get("rel_tol", 1e-10) and tl.get("abs_tol") == req.get("abs_tol", 1e-12), None, None, str(tl)),
            check("the reported error estimate covers the actual error (within ×10)  [10.31 IN-4]", abs(v - t) / max(10 * est, 4 * np.finfo(float).eps * abs(t), 1e-300), 1.0)]


def rule_trapz(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; r = env["result"]
    y = arr(req["y"]); Y = np.atleast_2d(y); n = Y.shape[1]
    xs = [Fraction(float(v)) for v in arr(req["x"]).ravel()] if "x" in req else [Fraction(float(req["dx"])) * k for k in range(n)]
    want = []
    for row in Y:
        acc = Fraction(0)
        for k in range(n - 1):
            acc += (xs[k + 1] - xs[k]) * (Fraction(float(row[k])) + Fraction(float(row[k + 1]))) / 2
        want.append(float(acc))
    got = np.atleast_1d(arr(r["value"]) if isinstance(r["value"], dict) else np.array([float(num(r["value"]))]))
    err = max(abs(g - w) / max(abs(w) * np.finfo(float).eps, np.finfo(float).tiny) for g, w in zip(got, want))
    return [check("value vs the exact rational trapezoidal sum, recomputed by the runner: ≤ 2 roundings  [10.32 TZ-3]", err, 2.0)]


# ---------------------------------------------------------------- fit.poly.polyval (parity.yaml: T1)
def _gam(k):
    return k * U / (1 - k * U)


def rule_polyval(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; r = env["result"]
    p = [float(v) for v in arr(req["p"]).ravel()]; x = [float(v) for v in arr(req["x"]).ravel()]
    n = len(p) - 1
    y = arr(r["y"]).ravel(); b = arr(r["error_bound"]).ravel()
    if y.shape != (len(x),) or b.shape != (len(x),):
        return [fail("y, error_bound: shape [q]", f"got {y.shape}, {b.shape}")]
    mu = req.get("mu")
    worst_acc = worst_claim = 0.0
    for k, xv in enumerate(x):
        t = (xv - float(mu[0])) / float(mu[1]) if mu else xv
        tf = Fraction(t); acc = Fraction(0); pt = Fraction(0)
        for a in p:                                   # exact value and p̃(|t|), recomputed by the runner
            acc = acc * tf + Fraction(a); pt = pt * abs(tf) + abs(Fraction(a))
        exact = float(acc); err = abs(Fraction(float(y[k])) - acc)
        tol = 2 * U * abs(exact) + 2 * _gam(2 * n) ** 2 * float(pt) + np.finfo(float).tiny
        worst_acc = max(worst_acc, float(err) / tol)
        worst_claim = max(worst_claim, float(err) / (float(b[k]) + U * abs(exact) + np.finfo(float).tiny))
    return [check("y vs the exact value p(t), recomputed in rational arithmetic: |Δ| ≤ 2u|p(t)| + 2γ_{2n}²·p̃(|t|)  [10.33 PV-2]", worst_acc, 1.0),
            check("claim: |y − p(t)| ≤ error_bound at every point  [10.33 PV-3, E-11]", worst_claim, 1.0),
            ("degree and mu echo the request  [O-1]", r.get("degree") == n and (r.get("mu") == ([float(mu[0]), float(mu[1])] if mu else None)), None, None, f"got {r.get('degree')}, {r.get('mu')}")]


# ---------------------------------------------------------------- fit.poly.roots (parity.yaml: T2)
def rule_roots(case, env, manifest, req=None, expected=None):
    e = expected or case["expected"]; r = env["result"]
    got = arr(r["roots"]).astype(complex).ravel() if r["roots"]["shape"][0] else np.zeros(0, complex)
    rad = arr(r["error_radius"]).ravel().astype(float)
    truth = [complex(z[0], z[1]) for z in e["roots_true"]]
    if got.shape != (len(truth),):
        return [fail("roots: degree n roots", f"got {got.shape}, reference {len(truth)}")]
    order_ok = all(a.real < b.real or (a.real == b.real and a.imag >= b.imag) for a, b in zip(got, got[1:]))
    out = [("roots sorted: real part ascending, ties by imaginary part descending  [10.34 RT-3]", order_ok, None, None, f"got {got.tolist()}"),
           ("degree and zero_roots equal the reference  [O-1]", r.get("degree") == e["degree"] and r.get("zero_roots") == e["zero_roots"], None, None, f"got {r.get('degree')}, {r.get('zero_roots')}")]
    miss = sum(1 for t in truth if not any(abs(z - t) <= q for z, q in zip(got, rad)))
    out.append(check("claim: every true root lies within error_radius of a reported root  [10.34 RT-4]", float(miss), 0.0))
    worst = 0.0
    for t, c in zip(truth, e["cond_abs"]):
        if c is None or not len(got):
            continue
        err = float(np.min(np.abs(got - t)))
        worst = max(worst, err / (32 * U * c + 2 * U * abs(t) + np.finfo(float).tiny))
    out.append(check("simple roots vs mpmath truth: |Δ| ≤ 32u·q̃(|r|)/|q'(r)| + 2u|r|  [T2]", worst, 1.0))
    return out


# ---------------------------------------------------------------- linalg.matrix.inv (parity.yaml: T1, profile 10.35)
def rule_inv(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    n = int(e["n"])
    X = arr(r["X"])
    if X.shape != (n, n):
        return [fail("X: shape [n, n]", f"got {X.shape}, want {(n, n)}")]
    A = arr(req["A"]).reshape(n, n)
    Xt = np.array(e["X_true"], dtype=float).reshape(n, n)
    kappa = float(e["kappa1_true"])
    claims = (env.get("trust") or {}).get("checks") or {}
    fe = float(np.max(np.sum(np.abs(X - Xt), axis=0)) / np.max(np.sum(np.abs(Xt), axis=0)))
    out = []
    if case.get("expect_postcondition_failed") and claims.get("forward_error_below_one") is False:
        out.append(("flagged case: forward_error_below_one = false and /X listed unreliable (accuracy not required)  [E-13]", True, None, None, ""))
    else:
        out.append(check("X vs exact inverse: ‖ΔX‖₁/‖X_true‖₁ ≤ 1e-12·max(1, κ₁/100)  [T1, V-6]", fe, 1e-12 * max(1.0, kappa / 100.0)))
    out.append(("method echoes lu_partial_pivoting  [IV-2]", r.get("method") == "lu_partial_pivoting", None, None, f"got {r.get('method')}"))
    if claims.get("forward_error_below_one") is True:
        out.append(("claim: forward_error_below_one = true only where the true κ₁·u < 10  [IV-4]", kappa * U < 10, None, None, f"true κ₁ = {kappa:.3e}"))
    k_ret = float(np.max(np.sum(np.abs(A), axis=0)) * np.max(np.sum(np.abs(X), axis=0)))
    kr = num(r.get("kappa1"))
    out.append(check("kappa1 = ‖A‖₁·‖X‖₁ of the returned X, recomputed by the runner (relative)  [IV-4]", abs(kr - k_ret) / k_ret if k_ret else abs(kr), 1e-12))
    if claims.get("backward_stable") is True and n <= 60:
        eta = _exact_backward_error(A, np.eye(n), X, True)
        out.append(check("backward_stable = true ⇒ max_j ‖e_j − A x_j‖∞/(‖A‖∞‖x_j‖∞ + 1) ≤ 10·n·u, recomputed exactly by the runner  [IV-3]", eta, 10 * n * U))
    return out


# ---------------------------------------------------------------- linalg.matrix.det (parity.yaml: T2, profile 10.36)
def rule_det(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = expected or case["expected"]; r = env["result"]
    n = int(e["n"])
    zero = r.get("zero_within_error")
    if e["sign_true"] == 0:
        return [("singular data (det_true = 0 exactly): zero_within_error = true  [DT-4, claim check]", zero is True, None, None, f"zero_within_error = {zero}, det = {r.get('det')}")]
    kappa = float(e["kappa1_true"]); tol = 16 * n * U * max(1.0, kappa)
    out = [("significant data (n·u·κ₁ < 1e-3): zero_within_error = false  [DT-4]", zero is False, None, None, f"zero_within_error = {zero}")]
    la = num(r.get("log_abs_det"))
    out.insert(0, check("ln|det| vs exact: |Δ| ≤ 16·n·u·max(1, κ₁)  [DT-2]", abs(la - float(e["log_abs_det_true"])), tol))
    out.append(("sign equals the sign of the exact det  [DT-2]", r.get("sign") == e["sign_true"], None, None, f"got {r.get('sign')}, want {e['sign_true']}"))
    d = num(r.get("det")); dt = e.get("det_true")
    if dt is not None and abs(dt) >= np.finfo(float).tiny:
        out.append(check("det vs exact: |Δdet|/|det| ≤ 16·n·u·max(1, κ₁)  [DT-2]", abs(d - dt) / abs(dt), tol))
    else:
        want = np.sign(e["sign_true"]) * np.inf if float(e["log_abs_det_true"]) > 0 else None
        ok = (d == want) if want is not None else (abs(d) < np.finfo(float).tiny)
        out.append(("det out of range: ±Inf (overflow) or below the normal range (underflow)  [DT-3]", bool(ok), None, None, f"got {r.get('det')}"))
    return out


# ---------------------------------------------------------------- v0.4.23: rank, basis, pinv, norm, cond (profiles 10.37 – 10.41)
def _flagged(case, env, key="rank_well_separated"):
    return bool(case.get("expect_postcondition_failed")) and ((env.get("trust") or {}).get("checks") or {}).get(key) is False


def _rank_checks(case, env):
    e = case["expected"]; r = env["result"]; claims = (env.get("trust") or {}).get("checks") or {}
    m, n = int(e["m"]), int(e["n"])
    s = arr(r["s"]).ravel(); st = np.array(e["s_true"], dtype=float)
    out = [check("singular values: |Δσ_k| ≤ 32·max(m, n)·u·σ_max  [RK-1]", float(np.max(np.abs(s - st))) if s.shape == st.shape else float("inf"), 32 * max(m, n) * U * max(st[0], np.finfo(float).tiny))]
    if case.get("expect_postcondition_failed"):
        out.append(("claim: rank_well_separated = false where a true singular value lies in (tol/10, 100·tol]  [RK-3]", claims.get("rank_well_separated") is False, None, None, f"rank_well_separated = {claims.get('rank_well_separated')}"))
    else:
        out.append(("rank equals the true rank  [RK-2]", r.get("rank") == e["rank_true"], None, None, f"got {r.get('rank')}, want {e['rank_true']}"))
    if r.get("tol_source") == "default":
        t = max(m, n) * float(np.spacing(s[0])) if s[0] != 0 else max(m, n) * 5e-324
        out.append(("default tol = max(m, n)·eps(s_1) of the returned s, and rank = #{s_k > tol}, recomputed by the runner  [RK-2]",
                    num(r.get("tol")) == t and r.get("rank") == int(np.sum(s > t)), None, None, f"tol {r.get('tol')} vs {t}, rank {r.get('rank')} vs {int(np.sum(s > t))}"))
    return out


def rule_rank(case, env, manifest, req=None, expected=None):
    return _rank_checks(case, env)


def _proj(Q):
    return Q @ Q.T if Q.size else np.zeros((Q.shape[0], Q.shape[0]))


def rule_basis(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]
    m, n, k = int(e["m"]), int(e["n"]), int(e["rank_true"])
    O = arr(r["orth"]).reshape(m, -1); N = arr(r["null"]).reshape(n, -1)
    out = _rank_checks(dict(case, expected=dict(e, s_true=e.get("s_true", list(arr(r["s"]).ravel())))), env)[1:]
    if not _flagged(case, env):
        Ot = np.array(e["orth_true"], dtype=float).reshape(m, k); Pn = np.array(e["null_projector_true"], dtype=float).reshape(n, n)
        if O.shape[1] != k or N.shape[1] != n - k:
            return out + [fail("orth [m, r], null [n, n − r]", f"got {O.shape}, {N.shape}")]
        d = max(float(np.linalg.norm(_proj(O) - _proj(Ot))), float(np.linalg.norm(_proj(N) - Pn)))
        tol = 64 * max(m, n) * U * (e["sigma_max_true"] / e["gap_true"] if e.get("gap_true") else 1.0)
        out.insert(0, check("range and null projectors vs the truth: ‖ΔP‖_F ≤ 64·max(m, n)·u·σ_max/gap  [BS-2]", d, max(tol, 64 * max(m, n) * U)))
        A = arr(req["A"]).reshape(m, n)
        oe = max(float(np.max(np.abs(O.T @ O - np.eye(O.shape[1])))) if O.size else 0.0, float(np.max(np.abs(N.T @ N - np.eye(N.shape[1])))) if N.size else 0.0)
        out.append(check("orthonormality of the returned bases, recomputed by the runner  [BS-3]", oe, 100 * max(m, n) * U))
        an = float(np.linalg.norm(A @ N)) if N.size else 0.0
        out.append(check("‖A·N‖_F, recomputed by the runner  [BS-3]", an, np.sqrt(n - k) * num(r.get("tol")) + 100 * max(m, n) * U * float(np.linalg.norm(A))))
    return out


def rule_pinv(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]
    m, n = int(e["m"]), int(e["n"])
    if _flagged(case, env):
        return [("flagged case: rank_well_separated = false and /X listed unreliable  [PI-3]", True, None, None, "")]
    X = arr(r["X"])
    if X.shape != (n, m):
        return [fail("X: shape [n, m]", f"got {X.shape}")]
    Xt = np.array(e["X_true"], dtype=float).reshape(n, m)
    nt = float(np.linalg.norm(Xt)); k = max(1.0, e["cond_r_true"] or 1.0)
    d = float(np.linalg.norm(X - Xt)) / nt if nt else float(np.linalg.norm(X))
    tol = 64 * max(m, n) * U * k
    A = arr(req["A"]).reshape(m, n)
    s = np.linalg.svd(A, compute_uv=False)
    dropped = float(np.linalg.norm(s[int(e["rank_true"]):])) / max(float(np.linalg.norm(A)), np.finfo(float).tiny)
    p1 = float(np.linalg.norm(A @ X @ A - A)) / max(float(np.linalg.norm(A)), np.finfo(float).tiny)
    p2 = float(np.linalg.norm(X @ A @ X - X)) / nt if nt else 0.0
    return [check("X vs the true pseudoinverse: ‖ΔX‖_F/‖X‖_F ≤ 64·max(m, n)·u·max(1, κ_r)  [PI-2]", d, tol),
            ("rank equals the true rank  [PI-2]", r.get("rank") == e["rank_true"], None, None, f"got {r.get('rank')}"),
            check("Penrose A·X·A = A, recomputed by the runner (plus the dropped part)  [PI-4]", p1, dropped + 100 * max(m, n) * U * k + 1e-300),
            check("Penrose X·A·X = X, recomputed by the runner  [PI-4]", p2, 100 * max(m, n) * U * k)]


def rule_norm(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]
    t = float(e["norm_true"]); g = num(r.get("norm"))
    svd = e["treated_as"] == "matrix" and req.get("p", 2) == 2
    tol = 32 * e["max_dim"] * U if svd else 4 * (e["n_elements"] + 6) * U
    return [check("norm vs the truth: |Δ|/norm ≤ 4·(N + 6)·u (direct) or 32·max(m, n)·u (matrix 2-norm)  [NM-2]", abs(g - t) / max(abs(t), np.finfo(float).tiny), tol),
            ("treated_as (vector for [n], [1, n], [n, 1])  [NM-1]", r.get("treated_as") == e["treated_as"], None, None, f"got {r.get('treated_as')}")]


def rule_cond(case, env, manifest, req=None, expected=None):
    e = case["expected"]; r = env["result"]
    t = e.get("cond_true")
    if t is None:
        return [("exact zero pivot: cond = \"Inf\", rcond = 0  [CN-3]", r.get("cond") == "Inf" and num(r.get("rcond")) == 0.0, None, None, f"got {r.get('cond')}, {r.get('rcond')}")]
    g = num(r.get("cond"))
    return [check("cond vs the truth: |Δ|/cond ≤ 64·max(m, n)·u·cond  [CN-2]", abs(g - t) / t, 64 * e["max_dim"] * U * t),
            check("rcond = 1/cond of the returned value  [CN-4]", abs(num(r.get("rcond")) * g - 1.0), 4 * U)]


# ---------------------------------------------------------------- v0.4.24: lu, qr, chol (profiles 10.42 – 10.44)
def _exact_rel_resid(X, Y, B):
    """‖X·Y − B‖_F / ‖B‖_F with X·Y − B in exact rational arithmetic."""
    Xf = [[Fraction(float(v)) for v in row] for row in X]; Yf = [[Fraction(float(v)) for v in row] for row in Y]
    num_ = Fraction(0); den = Fraction(0)
    for i in range(len(Xf)):
        for j in range(len(Yf[0])):
            d = sum(Xf[i][k] * Yf[k][j] for k in range(len(Yf))) - Fraction(float(B[i][j]))
            num_ += d * d; den += Fraction(float(B[i][j])) ** 2
    return math.sqrt(float(num_ / den)) if den else math.sqrt(float(num_))


def _rel_fro(a, b):
    nb = float(np.linalg.norm(b)); d = float(np.linalg.norm(np.asarray(a) - np.asarray(b)))
    return d / nb if nb else d


def rule_lu(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]; n = int(e["n"])
    L = arr(r["L"]).reshape(n, n); Uu = arr(r["U"]).reshape(n, n); A = arr(req["A"]).reshape(n, n)
    t = 1e-12 * max(1.0, (e["kappa1_true"] or 1.0) / 100.0)
    d = max(_rel_fro(L, np.array(e["L_true"]).reshape(n, n)), _rel_fro(Uu, np.array(e["U_true"]).reshape(n, n)))
    out = [check("L, U vs the truth: ‖Δ‖_F/‖·‖_F ≤ 1e-12·max(1, κ₁/100)  [LU-2]", d, t),
           ("perm equals the partial-pivoting permutation  [LU-2]", r.get("perm") == e["perm_true"], None, None, f"got {r.get('perm')}, want {e['perm_true']}"),
           ("L unit lower triangular, U upper triangular  [LU-3]", bool(np.all(np.triu(L, 1) == 0) and np.all(np.diag(L) == 1) and np.all(np.tril(Uu, -1) == 0)), None, None, "structure")]
    if n <= 40:
        out.append(check("‖A[perm, :] − L·U‖_F/‖A‖_F, recomputed exactly by the runner  [LU-4]", _exact_rel_resid(L, Uu, A[r["perm"], :]), 100 * n * U))
    return out


def rule_qr(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]
    m, n = int(e["m"]), int(e["n"]); k = min(m, n); full = e["mode"] == "full"
    Q = arr(r["Q"]); R = arr(r["R"]); A = arr(req["A"]).reshape(m, n)
    qc = m if full else k
    if Q.shape != (m, qc) or R.shape != (qc, n):
        return [fail("Q, R shapes for the mode  [QR-2]", f"got {Q.shape}, {R.shape}")]
    out = [("R upper triangular with R_ii ≥ 0  [QR-3]", bool(np.all(np.tril(R, -1) == 0) and np.all(np.diag(R)[:k] >= 0)), None, None, "structure / sign")]
    if e["unique"]:
        t = 1e-12 * max(1.0, (e["kappa2_true"] or 1.0) / 100.0)
        d = max(_rel_fro(Q, np.array(e["Q_true"]).reshape(m, qc)), _rel_fro(R, np.array(e["R_true"]).reshape(qc, n)))
        out.insert(0, check("Q, R vs the truth (unique for full column rank): ‖Δ‖_F/‖·‖_F ≤ 1e-12·max(1, κ₂/100)  [QR-3]", d, t))
    out.append(check("‖A − Q·R‖_F/‖A‖_F, recomputed by the runner  [QR-4]", float(np.linalg.norm(A - Q @ R)) / max(float(np.linalg.norm(A)), np.finfo(float).tiny), 100 * max(m, n) * U))
    out.append(check("‖QᵀQ − I‖_max, recomputed by the runner  [QR-4]", float(np.max(np.abs(Q.T @ Q - np.eye(qc)))), 100 * max(m, n) * U))
    return out


def rule_chol(case, env, manifest, req=None, expected=None):
    req = req or case["request"]; e = case["expected"]; r = env["result"]; n = int(e["n"])
    R = arr(r["R"]).reshape(n, n); A = arr(req["A"]).reshape(n, n); S = np.triu(A) + np.triu(A, 1).T
    lower = e["triangle"] == "lower"
    t = 1e-12 * max(1.0, (e["kappa1_true"] or 1.0) / 100.0)
    tri_ok = bool(np.all(np.triu(R, 1) == 0)) if lower else bool(np.all(np.tril(R, -1) == 0))
    out = [check("R vs the truth: ‖ΔR‖_F/‖R‖_F ≤ 1e-12·max(1, κ₁/100)  [CH-3]", _rel_fro(R, np.array(e["R_true"]).reshape(n, n)), t),
           ("triangle as requested (upper: RᵀR = A; lower: L·Lᵀ = A)  [CH-1]", tri_ok and r.get("triangle") == e["triangle"], None, None, f"triangle {r.get('triangle')}")]
    if n <= 40:
        X, Y = (R, R.T) if lower else (R.T, R)
        out.append(check("‖S − RᵀR‖_F/‖S‖_F, recomputed exactly by the runner (S from the upper triangle)  [CH-4]", _exact_rel_resid(X, Y, S), 100 * n * U))
    return out


RULES = {"linalg.decomp.lu": rule_lu, "linalg.decomp.qr": rule_qr, "linalg.decomp.chol": rule_chol, "linalg.subspace.rank": rule_rank, "linalg.subspace.basis": rule_basis, "linalg.matrix.pinv": rule_pinv, "linalg.matrix.norm": rule_norm, "linalg.matrix.cond": rule_cond, "linalg.matrix.inv": rule_inv, "linalg.matrix.det": rule_det, "signal.spectral.fft": rule_fft, "ode.ivp.solve": rule_ode, "linalg.system.solve": rule_linsolve, "interp.grid.interp1": rule_interp1,
         "fit.poly.polyfit": rule_polyfit, "linalg.decomp.eig": rule_eig, "linalg.decomp.svd": rule_svd,
         "signal.design.butter": rule_butter, "signal.filter.filter": rule_filter,
         "nonlinear.root.fzero": rule_fzero, "nonlinear.system.fsolve": rule_fsolve,
         "optim.linear.linprog": rule_linprog, "optim.local.fmincon": rule_fmincon, "optim.local.fminsearch": rule_fminsearch,
         "calculus.quadrature.integral": rule_integral, "calculus.quadrature.trapz": rule_trapz,
         "fit.poly.polyval": rule_polyval, "fit.poly.roots": rule_roots}
