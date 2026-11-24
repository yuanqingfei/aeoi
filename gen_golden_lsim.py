"""Golden cases for control.lti.lsim (spec v0.4.5, 10.15).

Reference: scipy.signal.lsim (interp=True -> foh, interp=False -> zoh) and scipy.signal.dlsim for discrete
systems. Every continuous foh case is cross-checked against python-control forced_response (same formula,
independent code path); the generator stops if they disagree by more than 1e-11 relative. NOT MATLAB.
"""
import base64, hashlib, json, os, platform, shutil
import numpy as np
import scipy
import scipy.signal as sig
import control as ct

SEED = 20261124
rng = np.random.default_rng(SEED)
ENV = {"engine": "scipy.signal.lsim / dlsim (cross-checked: python-control forced_response)", "scipy": scipy.__version__,
       "python_control": ct.__version__, "numpy": np.__version__, "python": platform.python_version(), "seed": SEED,
       "note": "scipy reference values; NOT MATLAB output"}
OP, VER = "control.lti.lsim", "0.4.0"
D = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(D, ignore_errors=True); os.makedirs(D)


def mat(a):
    a = np.asarray(a, float)
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.ravel()]}


def mat_cm(a):  # section-0 mistake: column-major flattening of the same matrix
    a = np.asarray(a, float)
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.ravel(order="F")]}


def b64(a):
    a = np.ascontiguousarray(np.asarray(a, "<f8"))
    raw = a.tobytes()
    return {"dtype": "f64", "shape": list(a.shape), "order": "C", "byte_order": "little",
            "data_b64": base64.b64encode(raw).decode(), "sha256": "sha256:" + hashlib.sha256(raw).hexdigest()}


def tf(num, den, ts=0.0):
    return {"kind": "tf", "num": [float(x) for x in num], "den": [float(x) for x in den], "ts": ts}


def ss(A, B, C, D_, ts=0.0):
    return {"kind": "ss", "A": mat(A), "B": mat(B), "C": mat(C), "D": mat(D_), "ts": ts}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


n = 0
def save(family, request, expected=None, expected_error=None, extra=None):
    global n
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if extra: case.update(extra)
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{D}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False); n += 1


def scipy_sys(kind, sysd):
    if kind == "tf":
        return sig.TransferFunction(sysd["num"], sysd["den"]) if sysd["ts"] == 0 else sig.dlti(sysd["num"], sysd["den"], dt=sysd["ts"])
    A, B, C, D_ = (np.array(sysd[k]["data"]).reshape(sysd[k]["shape"]) for k in "ABCD")
    return (A, B, C, D_)


def case(family, sysd, u, dt, t0=0.0, interp="foh", x0=None, states=False, u_enc=None, give_interp=None, cm=None):
    """u: array [nu, nt]. Computes the reference with scipy; foh also with python-control."""
    u = np.atleast_2d(np.asarray(u, float))
    nu, nt = u.shape
    t = t0 + dt * np.arange(nt)
    kind = sysd["kind"]
    if sysd["ts"] > 0:
        s = scipy_sys(kind, sysd)
        if kind == "ss":
            A, B, C, D_ = s; s = sig.dlti(A, B, C, D_, dt=sysd["ts"])
        res = sig.dlsim(s, u.T, t=t, x0=x0)                           # (t, y) for tf, (t, y, x) for ss
        y, x = res[1], (res[2] if len(res) > 2 else None)
        used = "none"
    else:
        s = scipy_sys(kind, sysd)
        if kind == "ss":
            s = sig.StateSpace(*s)
        tout, y, x = sig.lsim(s, u.T if nu > 1 else u[0], t, X0=x0, interp=(interp == "foh"))
        used = interp
        if interp == "foh":  # independent cross-check
            sysc = ct.tf(sysd["num"], sysd["den"]) if kind == "tf" else ct.ss(*scipy_sys("ss", sysd))
            r = ct.forced_response(sysc, t, u if nu > 1 else u[0], initial_state=0 if x0 is None else x0)
            yc = np.atleast_2d(r.outputs)
            ys = np.atleast_2d(np.asarray(y).T) if np.ndim(y) > 1 else np.atleast_2d(y)
            scale = max(1.0, np.max(np.abs(ys)))
            dev = np.max(np.abs(yc - ys)) / scale
            assert dev <= 1e-11, (family, dev)
    y = np.asarray(y, float)
    y = y.reshape(nt, -1).T if y.ndim > 1 else y.reshape(1, nt)
    req = {"system": sysd, "u": u_enc if u_enc is not None else (mat(u[0]) if nu == 1 and family.startswith("siso") else mat(u)),
           "time": {"dt": dt, "unit": "s"}}
    if t0: req["time"]["t0"] = t0
    if give_interp is not None: req["interpolation"] = give_interp
    elif sysd["ts"] == 0 and interp != "foh": req["interpolation"] = interp
    if x0 is not None: req["x0"] = mat(x0)
    if states: req["return_states"] = True
    exp = {"t": [float(v) for v in t], "y": [float(v) for v in y.ravel()], "shape": list(y.shape), "n_grid_points": nt, "interpolation": used}
    if states:
        x = np.asarray(x, float).reshape(nt, -1).T
        exp["x"] = [float(v) for v in x.ravel()]; exp["x_shape"] = list(x.shape)
    extra = {"column_major_mistake_request": dict(req, system=cm)} if cm is not None else None
    save(family, req, exp, extra=extra)


def rand_stable_ss(nx, nu, ny):
    while True:
        A = rng.normal(size=(nx, nx))
        A = A - (np.max(np.real(np.linalg.eigvals(A))) + rng.uniform(0.3, 1.5)) * np.eye(nx)
        if np.all(np.real(np.linalg.eigvals(A)) < -0.2):
            return A, rng.normal(size=(nx, nu)), rng.normal(size=(ny, nx)), np.zeros((ny, nu))


# ---- SISO, the common inputs
G2 = tf([2.0], [1.0, 3.0, 2.0])
tt = np.arange(0, 1001) * 0.01
case("siso_ramp_foh", G2, tt, 0.01)                                   # foh is exact for a ramp
case("siso_sine_foh", G2, np.sin(2 * tt), 0.01)
case("siso_sine_zoh", G2, np.sin(2 * tt), 0.01, interp="zoh")
sq = np.where((tt % 2.0) < 1.0, 1.0, -1.0)
case("siso_square_zoh", tf([1.0, 1.0], [1.0, 0.8, 4.0]), sq, 0.01, interp="zoh")
case("siso_square_foh", tf([1.0, 1.0], [1.0, 0.8, 4.0]), sq, 0.01)  # foh smears the jumps: a different (still exact) answer
case("siso_feedthrough_foh", tf([1.0, 2.0], [1.0, 5.0]), np.cos(3 * tt), 0.01)   # biproper: D ≠ 0
case("siso_t0_nonzero", G2, np.sin(2 * tt), 0.01, t0=5.0)
case("siso_stiff_foh", tf([1e4], [1.0, 10001.0, 1e4]), np.sin(tt), 0.01)      # poles −1, −1e4; dt > 0.1/|p| (warning)
case("siso_unstable_foh", tf([1.0], [1.0, -0.5]), np.ones_like(tt[:301]), 0.01)
case("siso_explicit_default", G2, np.sin(2 * tt), 0.01, give_interp="foh")   # interpolation written out = default
# ---- state space, x0, states
A = np.array([[0.0, 1.0], [-4.0, -0.4]]); B = np.array([[0.0], [1.0]]); C = np.array([[1.0, 0.0]]); D0 = np.zeros((1, 1))
case("ss_x0_free", ss(A, B, C, D0), np.zeros((1, 601)), 0.01, x0=[1.0, 0.0], states=True)
case("ss_x0_forced_zoh", ss(A, B, C, D0), np.sin(tt[None, :601]), 0.01, x0=[0.5, -1.0], states=True, interp="zoh")
# ---- MIMO
for i in range(3):
    A_, B_, C_, D_ = rand_stable_ss(int(rng.integers(2, 5)), 2, 2)
    U = rng.normal(size=(2, 501)).cumsum(axis=1) * 0.05
    case("mimo_random_foh", ss(A_, B_, C_, D_), U, 0.02, cm=ss(A_, B_, C_, D_) | {"A": mat_cm(A_)})
A_, B_, C_, D_ = rand_stable_ss(3, 2, 1)
case("mimo_states_zoh", ss(A_, B_, C_, D_), np.vstack([np.sin(tt[:401]), np.cos(tt[:401])]), 0.01, interp="zoh", states=True, x0=[0.1, 0.2, -0.3])
# ---- random SISO with noise input
for i in range(4):
    p = rng.uniform(-6, -0.2, size=int(rng.integers(1, 4)))
    case("siso_random_noise_foh", tf([float(rng.uniform(0.5, 3))], np.real(np.poly(p))), rng.normal(size=801), 0.005)
# ---- discrete
case("discrete_tf", tf([0.2, 0.1], [1.0, -1.2, 0.5], ts=0.1), np.ones(51), 0.1)
Ad = np.array([[0.9, 0.1], [0.0, 0.7]]); Bd = np.array([[0.0], [1.0]])
case("discrete_ss_x0", ss(Ad, Bd, np.array([[1.0, 1.0]]), np.zeros((1, 1)), ts=0.5), np.sin(np.arange(41) * 0.3)[None, :], 0.5, x0=[1.0, -1.0], states=True)
# ---- long input as b64 (3.6): 20000 samples
ul = np.sin(0.7 * np.arange(20000) * 0.001) + 0.2 * rng.normal(size=20000)
case("siso_long_b64", tf([4.0], [1.0, 2.0, 4.0]), ul, 0.001, u_enc=b64(ul))
# ---- errors
save("x0_with_tf", {"system": G2, "u": mat(np.zeros(3)), "time": {"dt": 0.1, "unit": "s"}, "x0": mat([1.0, 0.0])}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("interp_on_discrete", {"system": tf([1.0], [1.0, -0.5], ts=0.1), "u": mat(np.ones(3)), "time": {"dt": 0.1, "unit": "s"}, "interpolation": "zoh"}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("discrete_dt_mismatch", {"system": tf([1.0], [1.0, -0.5], ts=0.1), "u": mat(np.ones(3)), "time": {"dt": 0.05, "unit": "s"}}, expected_error="E.VALIDATION.GRID")
save("u_rows_mismatch", {"system": ss(A_, B_, C_, D_), "u": mat(np.ones((3, 10))), "time": {"dt": 0.1, "unit": "s"}}, expected_error="E.VALIDATION.SHAPE")
save("u_one_d_for_mimo", {"system": ss(A_, B_, C_, D_), "u": mat(np.ones(10)), "time": {"dt": 0.1, "unit": "s"}}, expected_error="E.VALIDATION.SHAPE")
save("x0_wrong_length", {"system": ss(A, B, C, D0), "u": mat(np.zeros(3)), "time": {"dt": 0.1, "unit": "s"}, "x0": mat([1.0, 2.0, 3.0])}, expected_error="E.VALIDATION.SHAPE")
save("improper_tf", {"system": tf([1.0, 0.0, 0.0], [1.0, 1.0]), "u": mat(np.zeros(3)), "time": {"dt": 0.1, "unit": "s"}}, expected_error="E.DOMAIN.IMPROPER_SYSTEM")
save("dt_not_positive", {"system": G2, "u": mat(np.zeros(3)), "time": {"dt": 0.0, "unit": "s"}}, expected_error="E.VALIDATION.SCHEMA")
save("missing_u", {"system": G2, "time": {"dt": 0.1, "unit": "s"}}, expected_error="E.VALIDATION.SCHEMA")
save("u_nonfinite", {"system": G2, "u": {"dtype": "f64", "shape": [3], "data": [0.0, "NaN", 1.0]}, "time": {"dt": 0.1, "unit": "s"}}, expected_error="E.VALIDATION.NON_FINITE")
print("lsim golden:", n, "cases →", D)

# ---- published examples (examples/control.lti.lsim): requests + scipy reference values; responses come from
# regen_examples.py (the Rust implementation)
E = f"examples/{OP}"
t_ex = np.arange(201) * 0.05
u_ex = np.sin(2 * t_ex)
req_ex = {"system": tf([2.0], [1.0, 3.0, 2.0]), "u": mat(u_ex), "time": {"dt": 0.05, "unit": "s"}}
_, y_ex, _ = sig.lsim(sig.TransferFunction([2.0], [1.0, 3.0, 2.0]), u_ex, t_ex)
A_ex = np.array([[0.0, 1.0], [-4.0, -0.4]])                      # non-symmetric: memory order matters (section 0)
sys_ex = ss(A_ex, np.array([[0.0], [1.0]]), np.array([[1.0, 0.0]]), np.zeros((1, 1)))
t2 = np.arange(301) * 0.02
u2 = 0.5 * np.ones(301)
req_x0 = {"system": sys_ex, "u": mat(u2), "time": {"dt": 0.02, "unit": "s"}, "x0": mat([1.0, 0.0]), "return_states": True, "interpolation": "zoh"}
_, y2, x2 = sig.lsim(sig.StateSpace(A_ex, [[0.0], [1.0]], [[1.0, 0.0]], [[0.0]]), u2, t2, X0=[1.0, 0.0], interp=False)
req_err = {"system": tf([2.0], [1.0, 3.0, 2.0]), "u": mat(u_ex[:5]), "time": {"dt": 0.05, "unit": "s"}, "x0": mat([1.0, 0.0])}
for name, obj in [("request.json", req_ex), ("request.ss_x0.json", req_x0), ("request.error.json", req_err)]:
    open(f"{E}/{name}", "w").write(json.dumps(obj, indent=2, ensure_ascii=False) + "\n")
ref = {"request.json": {"y": [float(v) for v in y_ex]},
       "request.ss_x0.json": {"y": [float(v) for v in y2], "x": [float(v) for v in np.asarray(x2).T.ravel()]},
       "note": "scipy.signal.lsim reference values (interp=True for request.json, interp=False for request.ss_x0.json); NOT MATLAB output"}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("lsim examples →", E)
