"""Generate python-control/scipy reference ("golden") cases for the Rust parity tests.

THESE ARE python-control REFERENCES, NOT MATLAB OUTPUTS.
Layout (spec V-3, v0.2): golden/<engine>/<op>/<op_version>/<sha256>.json
Matrices are flattened row-major (numpy C order), as AEOI requires.
"""
import json, hashlib, os, platform, sys, shutil
import numpy as np, scipy, control as ct

SEED = 20261121
rng = np.random.default_rng(SEED)
ENGINE = "python-control"
ENV = {"engine": "python-control", "python_control": ct.__version__, "scipy": scipy.__version__, "numpy": np.__version__,
       "python": platform.python_version(), "slycot": False, "seed": SEED,
       "note": "python-control reference values computed on the Grok Bot box; NOT MATLAB output"}
ROOT = f"golden/{ENGINE}"

def M(a):
    a = np.atleast_2d(np.asarray(a, dtype=float))
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.flatten(order="C")]}

def lst(a): return [float(x) for x in np.asarray(a, dtype=float).ravel()]

def canon(o): return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)

count = {}
def save(op, family, request, expected=None, expected_error=None, expected_warnings=None, extra=None):
    case = {"spec": "aeoi/0.2", "operator": op, "operator_version": "0.2.0", "case_family": family,
            "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expected_warnings: case["expected_warnings"] = expected_warnings
    if extra: case.update(extra)
    h = hashlib.sha256(canon(case).encode()).hexdigest()
    d = f"{ROOT}/{op}/0.2.0"; os.makedirs(d, exist_ok=True)
    json.dump(case, open(f"{d}/{h}.json", "w"), indent=1, ensure_ascii=False)
    count[(op, family)] = count.get((op, family), 0) + 1

def tf_req(num, den, ts=0.0): return {"kind": "tf", "num": lst(num), "den": lst(den), "ts": ts, "time_unit": "s"}
def ss_req(A, B, C, D, ts=0.0): return {"kind": "ss", "A": M(A), "B": M(B), "C": M(C), "D": M(D), "ts": ts, "time_unit": "s"}

def stable_A(n, margin_lo=0.1, margin_hi=1.0):
    X = rng.standard_normal((n, n))
    return X - (np.max(np.linalg.eigvals(X).real) + rng.uniform(margin_lo, margin_hi)) * np.eye(n)

def bode_expected(sys, w):
    H = sys.frequency_response(w).complex  # python-control returns [ny, nu, nw] for MIMO, [nw] for SISO
    H = np.asarray(H)
    if H.ndim == 1: H = H[np.newaxis, np.newaxis, :]
    mag = 20 * np.log10(np.abs(H))
    ph = np.degrees(np.unwrap(np.angle(H), axis=-1))
    return {"w": lst(w), "shape": list(H.shape), "magnitude_db": lst(mag), "phase_deg": lst(ph)}

# ------------------------------------------------------------------ BODE
OPB = "control.lti.bode"
for _ in range(40):
    wn = 10 ** rng.uniform(-2, 2); z = rng.uniform(0.01, 2)
    num, den = [wn**2], [1, 2*z*wn, wn**2]
    w = np.logspace(np.log10(wn) - 2, np.log10(wn) + 2, 30)
    save(OPB, "siso_2nd_order_sweep", {"system": tf_req(num, den), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
         bode_expected(ct.tf(num, den), w))
for _ in range(15):
    n = int(rng.integers(2, 7)); nz = int(rng.integers(0, n + 1))
    poles = -10 ** rng.uniform(-1, 1, n); zeros = rng.uniform(-5, 5, nz)
    den = np.real(np.poly(poles)); num = np.real(np.poly(zeros)) * rng.uniform(0.5, 5)
    w = np.logspace(-2, 2, 40)
    save(OPB, "siso_tf_higher_order", {"system": tf_req(num, den), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
         bode_expected(ct.tf(num, den), w))
for _ in range(20):
    n = int(rng.integers(2, 9)); m = int(rng.integers(1, 4)); p = int(rng.integers(1, 4))
    A = stable_A(n); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n)); D = rng.standard_normal((p, m))
    w = np.logspace(-2, 2, 25)
    save(OPB, "mimo_ss_random_stable", {"system": ss_req(A, B, C, D), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
         bode_expected(ct.ss(A, B, C, D), w))
for _ in range(10):
    k = int(rng.integers(1, 3)); pr = -10 ** rng.uniform(-1, 1, k)
    pc = [complex(-rng.uniform(0.1, 2), rng.uniform(0.5, 5)) for _ in range(int(rng.integers(0, 2)))]
    poles = list(pr) + sum([[c, c.conjugate()] for c in pc], [])
    zeros = list(rng.uniform(-3, 3, int(rng.integers(0, len(poles) + 1))))
    gain = float(rng.uniform(0.5, 10))
    w = np.logspace(-2, 2, 30)
    sys = ct.zpk(zeros, poles, gain)
    save(OPB, "zpk", {"system": {"kind": "zpk", "z": [[float(np.real(x)), float(np.imag(x))] for x in zeros], "p": [[float(np.real(x)), float(np.imag(x))] for x in poles], "k": gain, "ts": 0, "time_unit": "s"},
                      "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}}, bode_expected(sys, w))
for ts in [1e-3, 0.1, 1.0] * 3 + [0.1]:
    n = int(rng.integers(1, 4)); poles = rng.uniform(-0.9, 0.9, n)
    den = np.real(np.poly(poles)); num = np.real(np.poly(rng.uniform(-0.9, 0.9, int(rng.integers(0, n + 1)))))
    w = np.logspace(np.log10(0.001 / ts), np.log10(0.99 * np.pi / ts), 30)
    save(OPB, "discrete_tf", {"system": tf_req(num, den, ts), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
         bode_expected(ct.tf(num, den, ts), w))
for _ in range(3):
    wn = 10 ** rng.uniform(-1, 1); z = 1e-10
    num, den = [wn**2], [1, 2*z*wn, wn**2]
    w = np.sort(np.concatenate([np.logspace(np.log10(wn) - 1, np.log10(wn) + 1, 9), [wn * (1 + 1e-11)]]))
    save(OPB, "near_imag_axis_poles", {"system": tf_req(num, den), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
         bode_expected(ct.tf(num, den), w), expected_warnings=["W.NUMERICAL.NEAR_POLE"])
for num, den in [([1, 0, 0], [1, 1]), ([1, 2, 3, 4], [1, 1, 1]), ([2, 0], [5])]:
    save(OPB, "improper_reject", {"system": tf_req(num, den), "frequency": {"mode": "logspace", "min": 0.01, "max": 100, "n": 50, "unit": "rad/s"}},
         expected_error="E.DOMAIN.IMPROPER_SYSTEM")
save(OPB, "hz_unit", {"system": tf_req([4], [1, 1.2, 4]), "frequency": {"mode": "explicit", "values": [0.01, 0.1, 0.3183098861837907, 1.0], "unit": "Hz"}},
     bode_expected(ct.tf([4], [1, 1.2, 4]), 2 * np.pi * np.array([0.01, 0.1, 0.3183098861837907, 1.0])))
# memory-order regression: non-symmetric A, non-square B/C
A_NS = np.array([[0., 1., 0.], [0., 0., 1.], [-6., -11., -6.]]); B_NS = np.array([[0., 1.], [0., 0.], [1., 2.]])
C_NS = np.array([[1., 0., 0.], [0., 1., 1.]]); D_NS = np.zeros((2, 2))
w = np.logspace(-2, 2, 30)
save(OPB, "memory_order_nonsymmetric", {"system": ss_req(A_NS, B_NS, C_NS, D_NS), "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}},
     bode_expected(ct.ss(A_NS, B_NS, C_NS, D_NS), w),
     extra={"column_major_mistake_request": {"system": {**ss_req(A_NS, B_NS, C_NS, D_NS), "A": {"dtype": "f64", "shape": [3, 3], "data": lst(A_NS.flatten(order="F"))}},
                                             "frequency": {"mode": "explicit", "values": lst(w), "unit": "rad/s"}}})

# ------------------------------------------------------------------ STEP
OPS = "control.lti.step"
def grid_len(tf, dt): return int(np.floor(tf / dt + 1e-9)) + 1
def step_expected(sys, tf, dt, decim=None, info=True, stable=True):
    N = grid_len(tf, dt); T = np.arange(N) * dt
    r = ct.step_response(sys, T, squeeze=False)
    Y = np.asarray(r.outputs)  # [ny, nu, nt]
    exp = {"n_grid_points": N}
    idx = np.arange(N) if decim is None else np.round(np.arange(decim) * (N - 1) / (decim - 1)).astype(int)
    exp["t"] = lst(T[idx]); exp["shape"] = [Y.shape[0], Y.shape[1], len(idx)]; exp["y"] = lst(Y[:, :, idx])
    if info and stable:
        si = ct.step_info(sys, T=T, SettlingTimeThreshold=0.02, RiseTimeLimits=(0.1, 0.9))
        if isinstance(si, dict): si = [[si]]
        exp["info"] = [[{k: (None if not np.isfinite(v) else float(v)) for k, v in c.items()} for c in row] for row in si]
    return exp
for _ in range(25):
    wn = 10 ** rng.uniform(-1, 1); z = rng.uniform(0.05, 0.9)
    tf_ = float(np.round(min(12 / (z * wn), 200), 3)); dt = tf_ / 1000
    sys = ct.tf([wn**2], [1, 2*z*wn, wn**2])
    save(OPS, "siso_underdamped", {"system": tf_req([wn**2], [1, 2*z*wn, wn**2]), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}},
         step_expected(sys, tf_, dt))
for z in list(rng.uniform(1.2, 4, 10)) + [1.0] * 5:
    wn = 10 ** rng.uniform(-1, 1); tf_ = float(np.round(12 * z / wn, 3)); dt = tf_ / 1000
    sys = ct.tf([wn**2], [1, 2*z*wn, wn**2])
    save(OPS, "siso_overdamped_and_critically_damped", {"system": tf_req([wn**2], [1, 2*z*wn, wn**2]), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}},
         step_expected(sys, tf_, dt))
for k in range(15):
    n = int(rng.integers(2, 6)); poles = -10 ** rng.uniform(-0.5, 0.7, n)
    zeros = [float(rng.uniform(0.5, 3))] if k < 5 else list(-10 ** rng.uniform(-0.5, 0.7, int(rng.integers(0, n))))
    den = np.real(np.poly(poles)); num = np.atleast_1d(np.real(np.poly(zeros))); num = num * den[-1] / num[-1]
    tf_ = float(np.round(10 / min(abs(poles)), 3)); dt = tf_ / 1000
    save(OPS, "siso_with_zeros" + ("_nonminimum_phase" if k < 5 else ""), {"system": tf_req(num, den), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}},
         step_expected(ct.tf(num, den), tf_, dt))
for _ in range(15):
    n = int(rng.integers(2, 7)); m = int(rng.integers(1, 4)); p = int(rng.integers(1, 4))
    A = stable_A(n, 0.3, 1.0); B = rng.standard_normal((n, m)); C = rng.standard_normal((p, n)); D = rng.standard_normal((p, m)) * 0.3
    tf_ = 20.0; dt = 0.02
    save(OPS, "mimo_ss_random_stable", {"system": ss_req(A, B, C, D), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}},
         step_expected(ct.ss(A, B, C, D), tf_, dt))
for _ in range(3):
    poles = -np.logspace(-2, 2, 4); den = np.real(np.poly(poles)); num = [den[-1]]
    tf_, dt = 700.0, 0.001
    save(OPS, "stiff_spread_poles", {"system": tf_req(num, den), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}, "output": {"return_samples": "decimated", "max_samples": 1001}},
         step_expected(ct.tf(num, den), tf_, dt, decim=1001))
for _ in range(5):
    a = rng.uniform(0.05, 0.5)
    num, den = [1.0], [1, rng.uniform(0.1, 1), -a]
    tf_, dt = 10.0, 0.01
    save(OPS, "unstable_system", {"system": tf_req(num, den), "time": {"mode": "uniform", "t_final": tf_, "dt": dt, "unit": "s"}},
         step_expected(ct.tf(num, den), tf_, dt, stable=False), expected_warnings=["W.DOMAIN.UNSTABLE_SYSTEM"], extra={"expect_info_null": True})
for _ in range(8):
    ts = 0.1; poles = rng.uniform(-0.8, 0.9, int(rng.integers(1, 4))); den = np.real(np.poly(poles)); num = [float(np.sum(den))]
    tf_ = 10.0
    save(OPS, "discrete_systems", {"system": tf_req(num, den, ts), "time": {"mode": "uniform", "t_final": tf_, "dt": ts, "unit": "s"}},
         step_expected(ct.tf(num, den, ts), tf_, ts))
A_NS2 = np.array([[0., 1., 0.], [0., 0., 1.], [-6., -11., -6.]])
save(OPS, "memory_order_nonsymmetric", {"system": ss_req(A_NS2, B_NS, C_NS, D_NS), "time": {"mode": "uniform", "t_final": 10.0, "dt": 0.01, "unit": "s"}},
     step_expected(ct.ss(A_NS2, B_NS, C_NS, D_NS), 10.0, 0.01),
     extra={"column_major_mistake_request": {"system": {**ss_req(A_NS2, B_NS, C_NS, D_NS), "A": {"dtype": "f64", "shape": [3, 3], "data": lst(A_NS2.flatten(order="F"))}},
                                             "time": {"mode": "uniform", "t_final": 10.0, "dt": 0.01, "unit": "s"}}})
save(OPS, "compute_limit", {"system": tf_req([4], [1, 1.2, 4]), "time": {"mode": "uniform", "t_final": 10, "dt": 1e-7, "unit": "s"}}, expected_error="E.RESOURCE.COMPUTE_LIMIT")
save(OPS, "output_too_large", {"system": ss_req(stable_A(2), np.ones((2, 3)), np.ones((3, 2)), np.zeros((3, 3))), "time": {"mode": "uniform", "t_final": 100, "dt": 0.0005, "unit": "s"}}, expected_error="E.RESOURCE.OUTPUT_TOO_LARGE")
save(OPS, "improper_reject", {"system": tf_req([1, 0, 0], [1, 1]), "time": {"mode": "uniform", "t_final": 1, "dt": 0.01, "unit": "s"}}, expected_error="E.DOMAIN.IMPROPER_SYSTEM")
save(OPS, "discrete_dt_mismatch", {"system": tf_req([1], [1, -0.5], 0.1), "time": {"mode": "uniform", "t_final": 1, "dt": 0.05, "unit": "s"}}, expected_error="E.VALIDATION.GRID")

# ------------------------------------------------------------------ LQR
OPL = "control.design.lqr"
def lqr_expected(A, B, Q, R, N=None):
    K, S, E = ct.lqr(A, B, Q, R, N) if N is not None else ct.lqr(A, B, Q, R)
    E = sorted(E, key=lambda e: (e.real, -e.imag))
    G = B @ np.linalg.solve(R, B.T)
    res = A.T @ S + S @ A - S @ G @ S + Q if N is None else None
    return {"K": M(K), "S": M(S), "closed_loop_poles": [[float(e.real), float(e.imag)] for e in E],
            "ref_care_residual_fro": None if res is None else float(np.linalg.norm(res)), "cond_S": float(np.linalg.cond(S))}
def req(A, B, Q, R, N=None):
    r = {"A": M(A), "B": M(B), "Q": M(Q), "R": M(R)}
    if N is not None: r["N"] = M(N)
    return r
for q, rr in [(1, 1), (10, 1), (1, 10), (100, 0.01), (0.01, 100)]:
    A = np.array([[0., 1.], [0., 0.]]); B = np.array([[0.], [1.]]); Q = q * np.eye(2); R = np.array([[rr]])
    save(OPL, "double_integrator_family", req(A, B, Q, R), lqr_expected(A, B, Q, R))
for _ in range(40):
    n = int(rng.integers(2, 11)); m = int(rng.integers(1, 4))
    A = rng.standard_normal((n, n)); B = rng.standard_normal((n, m))
    Mq = rng.standard_normal((n, n)); Q = Mq.T @ Mq + 0.1 * np.eye(n)
    Mr = rng.standard_normal((m, m)); R = Mr.T @ Mr + np.eye(m)
    save(OPL, "random_stabilizable", req(A, B, Q, R), lqr_expected(A, B, Q, R))
for _ in range(10):
    n = int(rng.integers(2, 7)); m = int(rng.integers(1, 3))
    A = rng.standard_normal((n, n)); B = rng.standard_normal((n, m))
    W = rng.standard_normal((n + m, n + m)); W = W.T @ W + 0.1 * np.eye(n + m)
    Q, N, R = W[:n, :n], W[:n, n:], W[n:, n:]
    save(OPL, "with_cross_term_N", req(A, B, Q, R, N), lqr_expected(A, B, Q, R, N))
for c in [1e6, 1e7, 1e8, 1e9, 1e9]:
    n = 4; m = 2
    A = rng.standard_normal((n, n)); B = rng.standard_normal((n, m)); Q = np.eye(n); R = np.diag([1.0, 1.0 / c])
    save(OPL, "ill_conditioned_R", req(A, B, Q, R), lqr_expected(A, B, Q, R))
for n in [20, 30, 40]:
    m = 3
    A = rng.standard_normal((n, n)) / np.sqrt(n); B = rng.standard_normal((n, m)); Q = np.eye(n); R = np.eye(m)
    save(OPL, "larger_n", req(A, B, Q, R), lqr_expected(A, B, Q, R))
A3 = np.array([[0., 1., 0.], [0., 0., 1.], [-6., 11., 6.]]); B3 = np.array([[0., 1.], [0., 0.], [1., 2.]])
Q3 = np.diag([1., 2., 3.]); R3 = np.eye(2)
save(OPL, "memory_order_nonsymmetric", req(A3, B3, Q3, R3), lqr_expected(A3, B3, Q3, R3),
     extra={"column_major_mistake_request": {**req(A3, B3, Q3, R3), "A": {"dtype": "f64", "shape": [3, 3], "data": lst(A3.flatten(order="F"))}}})
for A, B in [(np.diag([1., -1.]), np.array([[0.], [1.]])), (np.diag([2., 0.5, -1.]), np.array([[0.], [1.], [1.]])), (np.array([[0., 1.], [0., 0.]]), np.array([[1.], [0.]]))]:
    n = A.shape[0]
    save(OPL, "not_stabilizable", req(A, B, np.eye(n), np.eye(1)), expected_error="E.DOMAIN.NOT_STABILIZABLE")
save(OPL, "R_indefinite", req(np.eye(2), np.eye(2), np.eye(2), np.diag([1., -1.])), expected_error="E.DOMAIN.R_NOT_POSITIVE_DEFINITE")
save(OPL, "R_indefinite", req(np.eye(2), np.ones((2, 1)), np.eye(2), np.zeros((1, 1))), expected_error="E.DOMAIN.R_NOT_POSITIVE_DEFINITE")
save(OPL, "Q_not_symmetric", req(np.eye(2), np.eye(2), np.array([[1., 0.5], [0., 1.]]), np.eye(2)), expected_error="E.VALIDATION.NOT_SYMMETRIC")
save(OPL, "hamiltonian_imag_axis", req(np.array([[0., 1.], [-1., 0.]]), np.array([[0.], [1.]]), np.zeros((2, 2)), np.eye(1)), expected_error="E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG")
save(OPL, "shape_mismatch", req(np.eye(3), np.ones((2, 1)), np.eye(3), np.eye(1)), expected_error="E.VALIDATION.SHAPE")

summary = {f"{op} / {fam}": n for (op, fam), n in sorted(count.items())}
json.dump({"reference_env": ENV, "cases": summary, "total": sum(count.values())}, open(f"{ROOT}/INDEX.json", "w"), indent=1, ensure_ascii=False)
print(json.dumps(summary, indent=1)); print("total", sum(count.values()))
