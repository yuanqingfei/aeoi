"""Batch-2 (spec v0.3) python-control/scipy reference ("golden") cases for the Rust parity tests.

THESE ARE python-control / scipy REFERENCES, NOT MATLAB OUTPUTS.
Layout (spec V-3): golden/<engine>/<op>/<op_version>/<sha256>.json; matrices row-major (numpy C order).
Separate seed from batch 1 so that batch-1 golden files are unchanged.
"""
import json, hashlib, os, platform
import numpy as np, scipy, scipy.linalg as sla, scipy.signal as sig, control as ct

SEED = 20261122
rng = np.random.default_rng(SEED)
ENV = {"engine": "python-control", "python_control": ct.__version__, "scipy": scipy.__version__, "numpy": np.__version__,
       "python": platform.python_version(), "slycot": False, "seed": SEED,
       "note": "python-control/scipy reference values computed on the Grok Bot box; NOT MATLAB output"}
ROOT = "golden/python-control"; VER = "0.3.0"

def M(a):
    a = np.atleast_2d(np.asarray(a, dtype=float))
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.flatten(order="C")]}
def MF(a):  # the classic MATLAB A(:) mistake: column-major flattening
    a = np.atleast_2d(np.asarray(a, dtype=float))
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.flatten(order="F")]}
def lst(a): return [float(x) for x in np.asarray(a, dtype=float).ravel()]
def cl(v): return [[float(np.real(x)), float(np.imag(x))] for x in np.asarray(v, dtype=complex).ravel()]
def canon(o): return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
def fin(x):
    x = float(x)
    return x if np.isfinite(x) else ("NaN" if np.isnan(x) else ("Inf" if x > 0 else "-Inf"))

count = {}
def save(op, family, request, expected=None, expected_error=None, expected_warnings=None, extra=None):
    case = {"spec": "aeoi/0.3", "operator": op, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expected_warnings: case["expected_warnings"] = expected_warnings
    if extra: case.update(extra)
    h = hashlib.sha256(canon(case).encode()).hexdigest()
    d = f"{ROOT}/{op}/{VER}"; os.makedirs(d, exist_ok=True)
    json.dump(case, open(f"{d}/{h}.json", "w"), indent=1, ensure_ascii=False)
    count[(op, family)] = count.get((op, family), 0) + 1

def tf_req(num, den, ts=0.0): return {"kind": "tf", "num": lst(num), "den": lst(den), "ts": ts, "time_unit": "s"}
def ss_req(A, B, C, D, ts=0.0): return {"kind": "ss", "A": M(A), "B": M(B), "C": M(C), "D": M(D), "ts": ts, "time_unit": "s"}
def zpk_req(z, p, k, ts=0.0): return {"kind": "zpk", "z": cl(z), "p": cl(p), "k": float(k), "ts": ts, "time_unit": "s"}
def stable_A(n, lo=0.1, hi=1.0):
    X = rng.standard_normal((n, n))
    return X - (np.max(np.linalg.eigvals(X).real) + rng.uniform(lo, hi)) * np.eye(n)
def schur_stable(n, rho=0.9):
    X = rng.standard_normal((n, n)); return X * (rho * rng.uniform(0.3, 1.0) / max(abs(np.linalg.eigvals(X))))
def rand_roots(k, lo=-5, hi=-0.2, cplx=True):
    r = []
    while len(r) < k:
        if cplx and k - len(r) >= 2 and rng.uniform() < 0.4:
            re, im = rng.uniform(lo, hi), rng.uniform(0.3, 4); r += [re + 1j * im, re - 1j * im]
        else:
            r.append(rng.uniform(lo, hi))
    return np.array(r, dtype=complex)
def realpoly(r): return np.real(np.poly(r)) if len(r) else np.array([1.0])
A3 = np.array([[0., 1., 0.], [0., 0., 1.], [-6., -11., -6.]]) + np.array([[0, 0, 0], [0.5, 0, 0], [0, 0.2, 0]])  # non-symmetric, stable
B3 = np.array([[0.], [1.], [2.]]); C3 = np.array([[1., 0.5, 0.]]); D3 = np.array([[0.]])
def ss_nonsym(ts=0.0, A=A3): return ss_req(A, B3, C3, D3, ts)
def mistake(req, path=("system", "A"), A=A3):
    r = json.loads(json.dumps(req)); t = r
    for p in path[:-1]: t = t[p]
    t[path[-1]] = MF(A); return r

# ------------------------------------------------------------------ control.lti.convert
OP = "control.lti.convert"
def tf_coeffs(num, den):  # normalize monic den, strip leading zeros of num (exact)
    num = np.trim_zeros(np.asarray(num, float), "f"); a0 = den[0]
    return {"num": lst(num / a0 if num.size else [0.0]), "den": lst(np.asarray(den, float) / a0)}
for _ in range(15):
    n = int(rng.integers(1, 7)); m = int(rng.integers(0, n + 1))
    den = realpoly(rand_roots(n)) * rng.uniform(0.5, 3); num = realpoly(rand_roots(m, -6, 6)) * rng.uniform(-3, 3)
    A, B, C, D = sig.tf2ss(num, den)
    save(OP, "tf_to_ss_siso", {"system": tf_req(num, den), "to": "ss"}, {"A": M(A), "B": M(B), "C": M(C), "D": M(D)})
for _ in range(15):
    n = int(rng.integers(1, 7)); A = stable_A(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n)); D = rng.standard_normal((1, 1)) * (rng.uniform() < 0.5)
    num, den = sig.ss2tf(A, B, C, D)
    save(OP, "ss_to_tf_siso", {"system": ss_req(A, B, C, D), "to": "tf"}, tf_coeffs(num[0], den))
for _ in range(8):
    n = int(rng.integers(2, 6)); p = int(rng.integers(1, 3)); m = int(rng.integers(2, 4))
    A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n)); D = rng.standard_normal((p, m))
    nums, dens = [[None] * m for _ in range(p)], [[None] * m for _ in range(p)]
    for j in range(m):
        nj, dj = sig.ss2tf(A, B, C, D, input=j)
        for i in range(p):
            c = tf_coeffs(nj[i], dj); nums[i][j] = c["num"]; dens[i][j] = c["den"]
    save(OP, "ss_to_tf_mimo", {"system": ss_req(A, B, C, D), "to": "tf"}, {"num": nums, "den": dens}, expected_warnings=["W.DOMAIN.NONMINIMAL_REALIZATION"])
for _ in range(15):
    n = int(rng.integers(1, 7)); m = int(rng.integers(0, n + 1)); zr = rand_roots(m, -6, 6); pr = rand_roots(n); k = rng.uniform(0.2, 5) * rng.choice([-1, 1])
    num, den = realpoly(zr) * k, realpoly(pr) * rng.uniform(0.5, 2)
    z, p, kk = sig.tf2zpk(num, den)
    save(OP, "tf_to_zpk", {"system": tf_req(num, den), "to": "zpk"}, {"z": cl(z), "p": cl(p), "k": float(kk)})
for _ in range(10):
    n = int(rng.integers(1, 7)); m = int(rng.integers(0, n + 1)); z = rand_roots(m, -6, 6); p = rand_roots(n); k = rng.uniform(0.2, 5)
    num, den = sig.zpk2tf(z, p, k)
    save(OP, "zpk_to_tf", {"system": zpk_req(z, p, k), "to": "tf"}, tf_coeffs(np.atleast_1d(num), den))
for _ in range(10):
    n = int(rng.integers(2, 6)); A = stable_A(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n)); D = rng.standard_normal((1, 1)) * (rng.uniform() < 0.5)
    sys = ct.ss(A, B, C, D); z = np.asarray(sys.zeros()); z = z[np.abs(z) <= 1e10 * (1 + np.linalg.norm(np.block([[A, B], [C, D]])))]; p = np.linalg.eigvals(A)
    num, den = sig.ss2tf(A, B, C, D); num = num[0]; num[np.abs(num) < 1e-12 * np.max(np.abs(num))] = 0; num = np.trim_zeros(num, "f")
    save(OP, "ss_to_zpk_siso", {"system": ss_req(A, B, C, D), "to": "zpk"}, {"z": cl(z), "p": cl(p), "k": float(num[0] / den[0])})
num, den = sig.ss2tf(A3, B3, C3, D3); r = {"system": ss_nonsym(), "to": "tf"}
save(OP, "memory_order_nonsymmetric", r, tf_coeffs(num[0], den), extra={"column_major_mistake_request": mistake(r)})
save(OP, "mimo_to_zpk", {"system": ss_req(-np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2))), "to": "zpk"}, expected_error="E.DOMAIN.MIMO_NOT_SUPPORTED")
save(OP, "improper", {"system": tf_req([1, 0, 0], [1, 1]), "to": "ss"}, expected_error="E.DOMAIN.IMPROPER_SYSTEM")

# ------------------------------------------------------------------ control.lti.poles_zeros
OP = "control.lti.poles_zeros"
def damp(sys):
    wn, zeta, p = ct.damp(sys, doprint=False); return wn, zeta, p
def finite_zeros(sys):
    # python-control (no slycot) computes zeros with scipy QZ on the system pencil; infinite generalized
    # eigenvalues can come back as huge finite numbers (~1e16..1e18). They are not finite zeros: drop
    # |z| > 1e10·(1+‖[A B; C D]‖_F). Documented in parity.yaml (reference post-processing).
    z = np.asarray(sys.zeros()); S = np.block([[sys.A, sys.B], [sys.C, sys.D]])
    return z[np.abs(z) <= 1e10 * (1 + np.linalg.norm(S))]
def pz_exp(sys, zeros, dc):
    wn, zeta, p = damp(sys)
    return {"poles": cl(p), "zeros": cl(zeros), "dcgain": [fin(x) for x in np.atleast_1d(np.asarray(dc, float)).ravel()],
            "natural_frequency": [fin(x) for x in wn], "damping_ratio": [fin(x) for x in zeta]}
for _ in range(15):
    n = int(rng.integers(1, 7)); m = int(rng.integers(0, n + 1))
    den = realpoly(rand_roots(n)); num = realpoly(rand_roots(m, -6, 6)) * rng.uniform(0.5, 3)
    sys = ct.tf(num, den)
    save(OP, "siso_tf", {"system": tf_req(num, den)}, pz_exp(sys, np.roots(num), sys.dcgain()))
for _ in range(15):
    n = int(rng.integers(2, 7)); m = int(rng.integers(1, 4))
    A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((m, n)); D = rng.standard_normal((m, m)) * (rng.uniform() < 0.4)
    sys = ct.ss(A, B, C, D)
    save(OP, "square_ss", {"system": ss_req(A, B, C, D)}, pz_exp(sys, finite_zeros(sys), sys.dcgain()))
for _ in range(8):
    n = int(rng.integers(2, 6)); A = schur_stable(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n)); D = np.zeros((1, 1))
    sys = ct.ss(A, B, C, D, 0.1)
    save(OP, "discrete_ss", {"system": ss_req(A, B, C, D, 0.1)}, pz_exp(sys, finite_zeros(sys), sys.dcgain()))
for _ in range(5):
    z = rand_roots(int(rng.integers(0, 3)), -6, 6); p = rand_roots(int(rng.integers(3, 5))); k = rng.uniform(0.5, 3)
    sys = ct.zpk(z, p, k)
    save(OP, "zpk", {"system": zpk_req(z, p, k)}, pz_exp(sys, z, sys.dcgain()))
for a in [1.0, 3.0]:
    save(OP, "integrator_infinite_dc", {"system": tf_req([2.0], [1.0, a, 0.0])},
         {"poles": cl([-a, 0.0]), "zeros": [], "dcgain": ["Inf"]}, expected_warnings=["W.DOMAIN.INFINITE_DC_GAIN"])
sys = ct.ss(A3, B3, C3, D3); r = {"system": ss_nonsym()}
save(OP, "memory_order_nonsymmetric", r, pz_exp(sys, finite_zeros(sys), sys.dcgain()), extra={"column_major_mistake_request": mistake(r)})
save(OP, "degenerate", {"system": ss_req(-np.eye(2), np.ones((2, 1)), np.zeros((1, 2)), np.zeros((1, 1)))}, expected_error="E.DOMAIN.DEGENERATE_SYSTEM")

# ------------------------------------------------------------------ control.analysis.ctrb_obsv
OP = "control.analysis.ctrb_obsv"
def co_exp(A, B, C, ts, gram):
    Kc, Ko = ct.ctrb(A, B), ct.obsv(A, C)
    e = {"ctrb": M(Kc), "ctrb_rank": int(np.linalg.matrix_rank(Kc)), "ctrb_sv": lst(np.linalg.svd(Kc, compute_uv=False)),
         "obsv": M(Ko), "obsv_rank": int(np.linalg.matrix_rank(Ko)), "obsv_sv": lst(np.linalg.svd(Ko, compute_uv=False))}
    if gram:
        if ts == 0:
            e["Wc"] = M(sla.solve_continuous_lyapunov(A, -B @ B.T)); e["Wo"] = M(sla.solve_continuous_lyapunov(A.T, -C.T @ C))
        else:
            e["Wc"] = M(sla.solve_discrete_lyapunov(A, B @ B.T)); e["Wo"] = M(sla.solve_discrete_lyapunov(A.T, C.T @ C))
    return e
for _ in range(15):
    n = int(rng.integers(2, 7)); m = int(rng.integers(1, 3)); p = int(rng.integers(1, 3))
    A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n))
    save(OP, "random_stable_gramians", {"system": ss_req(A, B, C, np.zeros((p, m))), "gramians": True}, co_exp(A, B, C, 0, True))
for _ in range(5):
    n = int(rng.integers(2, 6)); A = schur_stable(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n))
    save(OP, "discrete_gramians", {"system": ss_req(A, B, C, np.zeros((1, 1)), 0.05), "gramians": True}, co_exp(A, B, C, 0.05, True))
for _ in range(4):  # uncontrollable by construction: block diagonal with an unreachable block
    n1 = int(rng.integers(1, 3)); n2 = int(rng.integers(1, 3))
    A = sla.block_diag(stable_A(n1), stable_A(n2)); B = np.vstack([rng.standard_normal((n1, 1)), np.zeros((n2, 1))]); C = rng.standard_normal((1, n1 + n2))
    T = np.linalg.qr(rng.standard_normal((n1 + n2, n1 + n2)))[0]; A, B, C = T @ A @ T.T, T @ B, C @ T.T
    save(OP, "uncontrollable", {"system": ss_req(A, B, C, np.zeros((1, 1)))}, co_exp(A, B, C, 0, False))
A = np.array([[1., 1.], [0., 1.]]); B = np.array([[0.], [1.]]); C = np.array([[1., 0.]])
save(OP, "unstable_gramian_undefined", {"system": ss_req(A, B, C, np.zeros((1, 1))), "gramians": True}, co_exp(A, B, C, 0, False), expected_warnings=["W.DOMAIN.GRAMIAN_UNDEFINED"])
r = {"system": ss_nonsym(), "gramians": True}
save(OP, "memory_order_nonsymmetric", r, co_exp(A3, B3, C3, 0, True), extra={"column_major_mistake_request": mistake(r)})

# ------------------------------------------------------------------ control.analysis.margin
OP = "control.analysis.margin"
def mg_exp(sys, method="best"):
    gm, pm, sm, wpc, wgc, wms = ct.stability_margins(sys, method=method)
    GM, PM, SM, W180, WC, WS = ct.stability_margins(sys, returnall=True, method=method)
    return {"gm": fin(gm), "pm": fin(pm), "sm": fin(sm), "wpc": fin(wpc), "wgc": fin(wgc), "wms": fin(wms),
            "all_w180": lst(W180), "all_wc": lst(WC), "all_ws": lst(WS)}
for _ in range(20):
    n = int(rng.integers(2, 6)); m = int(rng.integers(0, n))
    den = realpoly(rand_roots(n, -5, -0.1)); num = realpoly(rand_roots(m, -8, -0.1)) * rng.uniform(0.5, 20) * den[-1] / realpoly(rand_roots(m, -8, -0.1))[-1] if False else realpoly(rand_roots(m, -8, -0.1))
    num = num * rng.uniform(1, 30) * abs(den[-1] / num[-1])
    save(OP, "random_tf", {"system": tf_req(num, den)}, mg_exp(ct.tf(num, den)))
for _ in range(10):
    a, b = rng.uniform(0.5, 5, 2); K = rng.uniform(0.5, 2) * a * b * (a + b)
    num, den = [K], np.polymul([1, 0], np.polymul([1, a], [1, b]))
    save(OP, "type1_loop", {"system": tf_req(num, den)}, mg_exp(ct.tf(num, den)))
for _ in range(8):
    n = int(rng.integers(1, 4)); p = rand_roots(n, -0.9, 0.9, cplx=False).real
    den = realpoly(p); num = [rng.uniform(0.05, 0.5) * abs(np.polyval(den, 1.0))]
    save(OP, "discrete_tf", {"system": tf_req(num, den, 0.1)}, mg_exp(ct.tf(num, den, 0.1), method="poly"))
for _ in range(5):
    n = int(rng.integers(2, 5)); A = stable_A(n, 0.1, 0.5); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n))
    C = C * 5 / abs((C @ np.linalg.solve(-A, B))[0, 0])
    save(OP, "ss_siso", {"system": ss_req(A, B, C, np.zeros((1, 1)))}, mg_exp(ct.ss(A, B, C, 0)))
Cm = C3 * 30; r = {"system": ss_req(A3, B3, Cm, D3)}
save(OP, "memory_order_nonsymmetric", r, mg_exp(ct.ss(A3, B3, Cm, D3)), extra={"column_major_mistake_request": mistake(r)})
save(OP, "mimo", {"system": ss_req(-np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2)))}, expected_error="E.DOMAIN.MIMO_NOT_SUPPORTED")

# ------------------------------------------------------------------ control.lti.c2d
OP = "control.lti.c2d"
for method in ["zoh", "foh", "tustin"]:
    for _ in range(8):
        n = int(rng.integers(1, 6)); m = int(rng.integers(1, 3)); p = int(rng.integers(1, 3)); T = 10 ** rng.uniform(-2, -0.5)
        A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n)); D = rng.standard_normal((p, m))
        Ad, Bd, Cd, Dd, _ = sig.cont2discrete((A, B, C, D), T, method="bilinear" if method == "tustin" else method)
        save(OP, f"ss_{method}", {"system": ss_req(A, B, C, D), "ts": T, "method": method}, {"A": M(Ad), "B": M(Bd), "C": M(Cd), "D": M(Dd)})
    for _ in range(5):
        n = int(rng.integers(1, 5)); m = int(rng.integers(0, n + 1)); T = 10 ** rng.uniform(-2, -0.5)
        den = realpoly(rand_roots(n)); num = realpoly(rand_roots(m, -6, 6)) * rng.uniform(0.5, 3)
        sd = ct.sample_system(ct.tf(num, den), T, method=method)
        save(OP, f"tf_{method}", {"system": tf_req(num, den), "ts": T, "method": method}, tf_coeffs(sd.num[0][0], sd.den[0][0]))
for _ in range(5):
    n = int(rng.integers(1, 5)); A = stable_A(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n)); D = np.zeros((1, 1))
    T = 10 ** rng.uniform(-2, -0.5); wp = rng.uniform(0.1, 0.9) * np.pi / T
    sd = ct.sample_system(ct.ss(A, B, C, D), T, method="tustin", prewarp_frequency=wp)
    save(OP, "tustin_prewarp", {"system": ss_req(A, B, C, D), "ts": T, "method": "tustin", "prewarp_frequency": wp}, {"A": M(sd.A), "B": M(sd.B), "C": M(sd.C), "D": M(sd.D)})
Ad, Bd, Cd, Dd, _ = sig.cont2discrete((A3, B3, C3, D3), 0.1, method="zoh"); r = {"system": ss_nonsym(), "ts": 0.1, "method": "zoh"}
save(OP, "memory_order_nonsymmetric", r, {"A": M(Ad), "B": M(Bd), "C": M(Cd), "D": M(Dd)}, extra={"column_major_mistake_request": mistake(r)})
save(OP, "already_discrete", {"system": tf_req([1], [1, -0.5], 0.1), "ts": 0.1}, expected_error="E.DOMAIN.NOT_CONTINUOUS")
save(OP, "prewarp_with_zoh", {"system": tf_req([1], [1, 1]), "ts": 0.1, "method": "zoh", "prewarp_frequency": 1.0}, expected_error="E.VALIDATION.OPTION_CONFLICT")

# ------------------------------------------------------------------ control.lti.impulse
OP = "control.lti.impulse"
def grid_len(tf, dt): return int(np.floor(tf / dt + 1e-9)) + 1
def imp_exp(sys, tf, dt):
    N = grid_len(tf, dt); T = np.arange(N) * dt
    resp = ct.impulse_response(sys, T)
    Y = np.asarray(resp.outputs)
    if Y.ndim == 1: Y = Y[np.newaxis, np.newaxis, :]
    elif Y.ndim == 2: Y = Y[:, np.newaxis, :] if sys.ninputs == 1 else Y[np.newaxis, :, :]
    return {"t": lst(T), "shape": list(Y.shape), "y": lst(Y)}
for _ in range(10):
    n = int(rng.integers(1, 6)); m = int(rng.integers(0, n)); den = realpoly(rand_roots(n)); num = realpoly(rand_roots(m, -6, 6)) * rng.uniform(0.5, 3)
    tf_, dt = rng.uniform(3, 10), rng.choice([0.01, 0.02, 0.05])
    save(OP, "siso_tf", {"system": tf_req(num, den), "time": {"mode": "uniform", "t_final": tf_, "dt": float(dt), "unit": "s"}}, imp_exp(ct.tf(num, den), tf_, dt))
for _ in range(8):
    n = int(rng.integers(2, 6)); m = int(rng.integers(1, 3)); p = int(rng.integers(1, 3)); A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n))
    save(OP, "mimo_ss", {"system": ss_req(A, B, C, np.zeros((p, m))), "time": {"mode": "uniform", "t_final": 5.0, "dt": 0.05, "unit": "s"}}, imp_exp(ct.ss(A, B, C, np.zeros((p, m))), 5.0, 0.05))
for _ in range(6):
    n = int(rng.integers(2, 5)); A = schur_stable(n); B = rng.standard_normal((n, 1)); C = rng.standard_normal((1, n)); D = rng.standard_normal((1, 1))
    save(OP, "discrete_unit_area", {"system": ss_req(A, B, C, D, 0.1), "time": {"mode": "uniform", "t_final": 3.0, "dt": 0.1, "unit": "s"}}, imp_exp(ct.ss(A, B, C, D, 0.1), 3.0, 0.1))
for _ in range(2):
    A = stable_A(2); B = rng.standard_normal((2, 1)); C = rng.standard_normal((1, 2)); D = np.array([[0.7]])
    import warnings as _w
    with _w.catch_warnings():
        _w.simplefilter("ignore"); e = imp_exp(ct.ss(A, B, C, D), 4.0, 0.05)
    save(OP, "direct_feedthrough", {"system": ss_req(A, B, C, D), "time": {"mode": "uniform", "t_final": 4.0, "dt": 0.05, "unit": "s"}}, e, expected_warnings=["W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED"])
r = {"system": ss_nonsym(), "time": {"mode": "uniform", "t_final": 5.0, "dt": 0.05, "unit": "s"}}
save(OP, "memory_order_nonsymmetric", r, imp_exp(ct.ss(A3, B3, C3, D3), 5.0, 0.05), extra={"column_major_mistake_request": mistake(r)})
save(OP, "discrete_dt_mismatch", {"system": tf_req([1], [1, -0.5], 0.1), "time": {"mode": "uniform", "t_final": 1.0, "dt": 0.05, "unit": "s"}}, expected_error="E.VALIDATION.GRID")

# ------------------------------------------------------------------ control.math.lyap
OP = "control.math.lyap"
for _ in range(12):
    n = int(rng.integers(1, 9)); A = stable_A(n); L = rng.standard_normal((n, n)); Q = L @ L.T
    save(OP, "continuous_spd", {"A": M(A), "Q": M(Q), "type": "continuous"}, {"X": M(sla.solve_continuous_lyapunov(A, -Q))})
for _ in range(3):
    n = int(rng.integers(2, 6)); A = stable_A(n); Q = rng.standard_normal((n, n))
    save(OP, "continuous_nonsymmetric_Q", {"A": M(A), "Q": M(Q), "type": "continuous"}, {"X": M(sla.solve_continuous_lyapunov(A, -Q))})
for _ in range(10):
    n = int(rng.integers(1, 9)); A = schur_stable(n, 0.95); L = rng.standard_normal((n, n)); Q = L @ L.T
    save(OP, "discrete_spd", {"A": M(A), "Q": M(Q), "type": "discrete"}, {"X": M(sla.solve_discrete_lyapunov(A, Q))})
for _ in range(3):
    n = 3; A = stable_A(n) + 1.5 * np.eye(n); A = A if min(abs(np.add.outer(np.linalg.eigvals(A), np.linalg.eigvals(A).conj()).ravel())) > 1e-2 else A + 0.3 * np.eye(n)
    Q = np.eye(n)
    save(OP, "unstable_solvable", {"A": M(A), "Q": M(Q), "type": "continuous"}, {"X": M(sla.solve_continuous_lyapunov(A, -Q))}, expected_warnings=["W.DOMAIN.UNSTABLE_SYSTEM"])
save(OP, "singular_continuous", {"A": M(np.diag([1., -1.])), "Q": M(np.eye(2)), "type": "continuous"}, expected_error="E.DOMAIN.LYAP_SINGULAR")
save(OP, "singular_discrete", {"A": M(np.array([[0., 1.], [-1., 0.]])), "Q": M(np.eye(2)), "type": "discrete"}, expected_error="E.DOMAIN.LYAP_SINGULAR")
Q3 = np.diag([1., 2., 3.]); r = {"A": M(A3), "Q": M(Q3), "type": "continuous"}
save(OP, "memory_order_nonsymmetric", r, {"X": M(sla.solve_continuous_lyapunov(A3, -Q3))}, extra={"column_major_mistake_request": mistake(r, ("A",))})

# ------------------------------------------------------------------ control.design.place
OP = "control.design.place"
def nX(X): X = np.asarray(X); return float(np.linalg.cond(X / np.linalg.norm(X, axis=0)))
for _ in range(12):
    n = int(rng.integers(2, 7)); A = rng.standard_normal((n, n)); B = rng.standard_normal((n, 1)); P = rand_roots(n, -6, -0.5)
    res = sig.place_poles(A, B, P)
    save(OP, "siso", {"A": M(A), "B": M(B), "poles": cl(P)}, {"K": M(res.gain_matrix), "poles": cl(P), "cond_X_scipy": nX(res.X)})
for _ in range(12):
    n = int(rng.integers(3, 8)); m = int(rng.integers(2, min(n, 4) + 1)); A = rng.standard_normal((n, n)); B = rng.standard_normal((n, m)); P = rand_roots(n, -6, -0.5)
    res = sig.place_poles(A, B, P)
    save(OP, "mimo_ytt", {"A": M(A), "B": M(B), "poles": cl(P)}, {"K_scipy_not_unique": M(res.gain_matrix), "poles": cl(P), "cond_X_scipy": nX(res.X)})
B3p = np.array([[0., 1.], [1., 0.], [2., 1.]]); P3 = np.array([-1, -2 + 1j, -2 - 1j]); res = sig.place_poles(A3, B3p, P3)
r = {"A": M(A3), "B": M(B3p), "poles": cl(P3)}
save(OP, "memory_order_nonsymmetric", r, {"K_scipy_not_unique": M(res.gain_matrix), "poles": cl(P3), "cond_X_scipy": nX(res.X)}, extra={"column_major_mistake_request": mistake(r, ("A",))})
save(OP, "uncontrollable", {"A": M(np.diag([1., 2.])), "B": M([[1.], [0.]]), "poles": cl([-1, -2])}, expected_error="E.DOMAIN.NOT_CONTROLLABLE")
save(OP, "multiplicity", {"A": M(np.array([[0., 1.], [2., 3.]])), "B": M([[0.], [1.]]), "poles": cl([-1, -1])}, expected_error="E.DOMAIN.POLE_MULTIPLICITY")
save(OP, "not_conjugate", {"A": M(np.array([[0., 1.], [2., 3.]])), "B": M([[0.], [1.]]), "poles": [[-1, 1], [-1, 2]]}, expected_error="E.VALIDATION.NOT_CONJUGATE")

# ------------------------------------------------------------------ control.design.dlqr
OP = "control.design.dlqr"
def dl_exp(A, B, Q, R, N=None):
    K, S, E = ct.dlqr(A, B, Q, R) if N is None else ct.dlqr(A, B, Q, R, N)
    return {"K": M(K), "S": M(S), "closed_loop_poles": cl(E)}
def dreq(A, B, Q, R, N=None):
    r = {"A": M(A), "B": M(B), "Q": M(Q), "R": M(R)}
    if N is not None: r["N"] = M(N)
    return r
for _ in range(15):
    n = int(rng.integers(2, 9)); m = int(rng.integers(1, 4)); A = rng.standard_normal((n, n)) / np.sqrt(n) * rng.uniform(0.5, 1.5); B = rng.standard_normal((n, m))
    L = rng.standard_normal((n, n)); Q = L @ L.T + 0.1 * np.eye(n); R = np.diag(rng.uniform(0.5, 2, m))
    save(OP, "random", dreq(A, B, Q, R), dl_exp(A, B, Q, R))
for _ in range(5):
    n = int(rng.integers(2, 6)); m = int(rng.integers(1, 3)); A = rng.standard_normal((n, n)) / np.sqrt(n); B = rng.standard_normal((n, m))
    Q = np.eye(n) * 2; R = np.eye(m); N = 0.2 * rng.standard_normal((n, m))
    save(OP, "cross_term", dreq(A, B, Q, R, N), dl_exp(A, B, Q, R, N))
T = 0.1; A = np.array([[1, T], [0, 1.]]); B = np.array([[T * T / 2], [T]])
save(OP, "double_integrator", dreq(A, B, np.eye(2), np.eye(1)), dl_exp(A, B, np.eye(2), np.eye(1)))
Ad3 = sla.expm(A3 * 0.1); Bd3 = np.array([[0., 1.], [1., 0.], [0.5, 0.5]])
r = dreq(Ad3, Bd3, np.diag([1., 2., 3.]), np.eye(2))
save(OP, "memory_order_nonsymmetric", r, dl_exp(Ad3, Bd3, np.diag([1., 2., 3.]), np.eye(2)), extra={"column_major_mistake_request": mistake(r, ("A",), A=Ad3)})
save(OP, "not_stabilizable", dreq(np.diag([1.5, 0.5]), np.array([[0.], [1.]]), np.eye(2), np.eye(1)), expected_error="E.DOMAIN.NOT_STABILIZABLE")
save(OP, "R_indefinite", dreq(np.eye(2) * 0.5, np.eye(2), np.eye(2), np.diag([1., -1.])), expected_error="E.DOMAIN.R_NOT_POSITIVE_DEFINITE")
th = 0.3; save(OP, "unit_circle_unobservable", dreq(np.array([[np.cos(th), -np.sin(th)], [np.sin(th), np.cos(th)]]), np.array([[0.], [1.]]), np.zeros((2, 2)), np.eye(1)), expected_error="E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG")

# ------------------------------------------------------------------ index (batch 1 + batch 2)
idx_path = f"{ROOT}/INDEX.json"
idx = json.load(open(idx_path)) if os.path.exists(idx_path) else {"cases": {}}
for (op, fam), n in sorted(count.items()):
    idx["cases"][f"{op} / {fam}"] = n
idx["total"] = sum(idx["cases"].values()); idx["reference_env_batch2"] = ENV
json.dump(idx, open(idx_path, "w"), indent=1, ensure_ascii=False)
print(json.dumps({f"{op} / {fam}": n for (op, fam), n in sorted(count.items())}, indent=1)); print("batch2 total", sum(count.values()))
