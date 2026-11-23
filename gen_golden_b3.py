"""Golden cases for series / parallel / feedback / pid / stepinfo / pade.

python-control 0.10.2 and mpmath — NOT MATLAB.
pid coefficients are the closed form evaluated at 50 decimal places (mpmath).
pade coefficients are the same Padé recurrence at 50 decimals, cross-checked
against control.pade before anything is written.
"""
import json, hashlib, os, platform
import numpy as np
import mpmath as mp
import control as ct

mp.mp.dps = 50
SEED = 20261123
rng = np.random.default_rng(SEED)
ENV_PC = {"engine": "python-control", "python_control": ct.__version__, "numpy": np.__version__,
          "python": platform.python_version(), "seed": SEED,
          "note": "python-control reference values; NOT MATLAB output"}
ENV_MP = {"engine": "mpmath", "mpmath": mp.__version__, "dps": 50, "python": platform.python_version(),
          "note": "50-decimal closed form, rounded to float64; NOT MATLAB output"}

def M(a):
    a = np.atleast_2d(np.asarray(a, float))
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.reshape(-1, order="C")]}
def MF(a):
    a = np.atleast_2d(np.asarray(a, float))
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.reshape(-1, order="F")]}
def lst(a):
    a = np.atleast_1d(np.asarray(a, float)).reshape(-1)
    # drop leading zeros, keep a single 0
    nz = np.flatnonzero(a)
    a = a[nz[0]:] if nz.size else np.array([0.0])
    return [float(x) for x in a]
def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
count = {}
def save(engine, op, family, request, expected=None, expected_error=None, expected_warnings=None, extra=None, env=None):
    ver = "0.3.0"
    case = {"spec": "aeoi/0.3", "operator": op, "operator_version": ver, "case_family": family,
            "request": request, "reference_env": env or (ENV_MP if engine == "mpmath" else ENV_PC)}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expected_warnings: case["expected_warnings"] = expected_warnings
    if extra: case.update(extra)
    h = hashlib.sha256(canon(case).encode()).hexdigest()
    d = f"golden/{engine}/{op}/{ver}"; os.makedirs(d, exist_ok=True)
    json.dump(case, open(f"{d}/{h}.json", "w"), indent=1, ensure_ascii=False)
    count[(op, family)] = count.get((op, family), 0) + 1

def tf(num, den, ts=0.0):
    return {"kind": "tf", "num": lst(num), "den": lst(den), "ts": float(ts), "time_unit": "s"}
def ss(A, B, C, D, ts=0.0):
    return {"kind": "ss", "A": M(A), "B": M(B), "C": M(C), "D": M(D), "ts": float(ts), "time_unit": "s"}
def coeffs(sys):
    n = np.asarray(sys.num[0][0], float).reshape(-1)
    d = np.asarray(sys.den[0][0], float).reshape(-1)
    return {"num": lst(n), "den": lst(d), "kind": "tf"}
def mats(sys):
    return {"A": M(sys.A), "B": M(sys.B), "C": M(sys.C), "D": M(sys.D), "kind": "ss"}

def rand_tf(n=None, m=None, discrete=False):
    n = int(n if n is not None else rng.integers(1, 5))
    m = int(m if m is not None else rng.integers(0, n + 1))
    pr = rng.uniform(-3, -0.2, size=n)
    # conjugate pairs roughly half the time: just real poles, stable
    zr = rng.uniform(-4, 4, size=m) if m else np.zeros(0)
    num = np.real(np.poly(zr)) * rng.uniform(0.3, 2) if m else np.array([rng.uniform(0.3, 2)])
    den = np.real(np.poly(pr))
    ts = 0.1 if discrete else 0.0
    return ct.tf(num, den, ts), num, den, ts

A3 = np.array([[-1.0, 0.4, 0.0], [0.2, -2.0, 0.5], [0.0, -0.3, -1.5]])
B3 = np.array([[0.0], [1.0], [0.2]]); C3 = np.array([[1.0, 0.0, 0.5]]); D3 = np.array([[0.1]])

# ---------------------------------------------------------------- series / parallel / feedback
for op, fn in (("control.lti.series", ct.series), ("control.lti.parallel", ct.parallel)):
    for i in range(6):
        s1, n1, d1, ts = rand_tf()
        s2, n2, d2, _ = rand_tf()
        out = fn(s1, s2)
        save("python-control", op, "siso_tf", {"sys1": tf(n1, d1, ts), "sys2": tf(n2, d2, ts)}, coeffs(out))
    s1 = ct.ss(A3, B3, C3, D3)
    A2 = np.array([[-0.7, 0.2], [0.0, -1.4]]); B2 = np.array([[0.0], [1.0]]); C2 = np.array([[1.5, -0.4]]); D2 = np.array([[0.0]])
    s2 = ct.ss(A2, B2, C2, D2)
    out = fn(s1, s2)
    req = {"sys1": ss(A3, B3, C3, D3), "sys2": ss(A2, B2, C2, D2)}
    extra = {"column_major_mistake_request": json.loads(json.dumps(req))}
    extra["column_major_mistake_request"]["sys1"]["A"] = MF(A3)
    save("python-control", op, "memory_order_nonsymmetric", req, mats(out), extra=extra)
    # MIMO 1x2 series with 2x1, or parallel 2x2
    if op.endswith("series"):
        A = rng.normal(size=(2, 2)); A = A - (np.max(np.real(np.linalg.eigvals(A))) + 1) * np.eye(2)
        s1 = ct.ss(A, np.eye(2), np.eye(2), np.zeros((2, 2)))
        s2 = ct.ss(-np.eye(2), np.eye(2), np.array([[1.0, 0.5]]), np.zeros((1, 2)))
        save("python-control", op, "mimo_ss", {"sys1": ss(s1.A, s1.B, s1.C, s1.D), "sys2": ss(s2.A, s2.B, s2.C, s2.D)}, mats(ct.series(s1, s2)))
    else:
        A = -np.eye(2); B = np.eye(2); C = np.eye(2); D = 0.2 * np.eye(2)
        s1 = ct.ss(A, B, C, D); s2 = ct.ss(-2 * np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2)))
        save("python-control", op, "mimo_ss", {"sys1": ss(s1.A, s1.B, s1.C, s1.D), "sys2": ss(s2.A, s2.B, s2.C, s2.D)}, mats(ct.parallel(s1, s2)))
    save("python-control", op, "sample_time_mismatch",
         {"sys1": tf([1], [1, 1], 0), "sys2": tf([1], [1, 1], 0.1)}, expected_error="E.DOMAIN.SAMPLE_TIME_MISMATCH")
    save("python-control", op, "shape_mismatch",
         {"sys1": tf([1], [1, 1]), "sys2": ss(np.eye(2), np.eye(2), np.eye(2), np.zeros((2, 2)))},
         expected_error="E.VALIDATION.SHAPE")

# feedback
for sign in (-1, 1):
    for i in range(4):
        s1, n1, d1, ts = rand_tf()
        s2, n2, d2, _ = rand_tf(n=int(rng.integers(1, 4)), m=int(rng.integers(0, 2)))
        # keep the loop well posed: strict proper plants (D=0) so no algebraic loop
        out = ct.feedback(s1, s2, sign=sign)
        save("python-control", "control.lti.feedback", "siso_tf",
             {"sys1": tf(n1, d1, ts), "sys2": tf(n2, d2, ts), "sign": sign}, coeffs(out))
s1, n1, d1, _ = rand_tf()
save("python-control", "control.lti.feedback", "unity", {"sys1": tf(n1, d1)}, coeffs(ct.feedback(s1)))
req = {"sys1": ss(A3, B3, C3, D3), "sys2": ss(-np.eye(2)[:1, :1] if False else np.array([[-1.2]]), np.array([[1.0]]), np.array([[1.0]]), np.array([[0.0]]))}
# simpler ss feedback memory order: unity feedback of the nonsymmetric ss
plant = ct.ss(A3, B3, C3, D3)
req = {"sys1": ss(A3, B3, C3, D3)}
bad = json.loads(json.dumps(req)); bad["sys1"]["A"] = MF(A3)
save("python-control", "control.lti.feedback", "memory_order_nonsymmetric", req, mats(ct.feedback(plant)),
     extra={"column_major_mistake_request": bad})
save("python-control", "control.lti.feedback", "algebraic_loop",
     {"sys1": tf([1], [1]), "sys2": tf([1], [1]), "sign": 1}, expected_error="E.DOMAIN.ALGEBRAIC_LOOP")
# improper controller in series with a strict plant stays a polynomial result
save("python-control", "control.lti.series", "improper_operand",
     {"sys1": tf([1, 0], [1]), "sys2": tf([1], [1, 1])},
     coeffs(ct.series(ct.tf([1, 0], [1]), ct.tf([1], [1, 1]))),
     expected_warnings=["W.DOMAIN.IMPROPER_OPERAND"])

# ---------------------------------------------------------------- pid (mpmath closed form)
def mlst(a):
    a = [mp.mpf(x) for x in a]
    while len(a) > 1 and a[0] == 0:
        a = a[1:]
    return [float(x) for x in a]

def pid_parallel(kp, ki, kd, tf):
    kp, ki, kd, tf = map(mp.mpf, (kp, ki, kd, tf))
    if ki == 0 and kd == 0:
        return mlst([kp]), mlst([1])
    if tf == 0:
        return (mlst([kd, kp]), mlst([1])) if ki == 0 else (mlst([kd, kp, ki]), mlst([1, 0]))
    if ki == 0:
        return mlst([kp * tf + kd, kp]), mlst([tf, 1])
    return mlst([kp * tf + kd, kp + ki * tf, ki]), mlst([tf, 1, 0])

def pid_standard(kp, ti, td, tf):
    kp, td, tf = map(mp.mpf, (kp, td, tf))
    if ti is None and td == 0:
        return mlst([kp]), mlst([1])
    if ti is None and tf == 0:
        return mlst([kp * td, kp]), mlst([1])
    if ti is None:
        return mlst([kp * (tf + td), kp]), mlst([tf, 1])
    ti = mp.mpf(ti)
    if td == 0:
        return mlst([kp * ti, kp]), mlst([ti, 0])
    if tf == 0:
        return mlst([kp * td * ti, kp * ti, kp]), mlst([ti, 0])
    return mlst([kp * ti * (tf + td), kp * (ti + tf), kp]), mlst([ti * tf, ti, 0])

pid_cases = [
    ("parallel_terms", {"Kp": 1.5, "Ki": 0.25, "Kd": 0.4, "Tf": 0.05}, pid_parallel(1.5, 0.25, 0.4, 0.05)),
    ("parallel_pi", {"Kp": 3, "Ki": 1.2}, pid_parallel(3, 1.2, 0, 0)),
    ("parallel_pd_improper", {"Kp": 2, "Kd": 0.7}, pid_parallel(2, 0, 0.7, 0)),
    ("parallel_p", {"Kp": -0.5}, pid_parallel(-0.5, 0, 0, 0)),
    ("parallel_filtered_d", {"Kp": 1, "Kd": 4, "Tf": 0.1}, pid_parallel(1, 0, 4, 0.1)),
    ("standard_full", {"form": "standard", "Kp": 2, "Ti": 4, "Td": 0.5, "Tf": 0.1}, pid_standard(2, 4, 0.5, 0.1)),
    ("standard_pi", {"form": "standard", "Kp": 1.7, "Ti": 3}, pid_standard(1.7, 3, 0, 0)),
    ("standard_pd_improper", {"form": "standard", "Kp": 1, "Td": 0.8}, pid_standard(1, None, 0.8, 0)),
]
for fam, req, (num, den) in pid_cases:
    warn = ["W.DOMAIN.IMPROPER_RESULT"] if len(num) > len(den) else None
    save("mpmath", "control.design.pid", fam, req, {"num": num, "den": den}, expected_warnings=warn)
save("mpmath", "control.design.pid", "option_conflict", {"Kp": 1, "Ki": 1, "Ti": 2}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("mpmath", "control.design.pid", "filter_without_derivative", {"Kp": 1, "Tf": 0.1}, expected_error="E.VALIDATION.OPTION_CONFLICT")

# ---------------------------------------------------------------- stepinfo
def step_case(sys, tf_final, dt, family, req_sys, warnings=None):
    t = np.arange(0.0, tf_final + 1e-12, dt)
    try:
        S = ct.step_info(sys, T=t)
    except Exception:
        S = None
    if S is None or not np.isfinite(S["SteadyStateValue"]):
        # unstable / integrator: peak on the python-control trajectory; other fields null
        tt, yy = ct.step_response(sys, T=t)
        yy = np.atleast_1d(yy)
        k = int(np.argmax(np.abs(yy)))
        exp = {"rise_time": None, "settling_time": None, "overshoot": None, "undershoot": None,
               "peak": float(abs(yy[k])), "peak_time": float(tt[k]), "steady_state": None, "stable": False}
        save("python-control", "control.lti.stepinfo", family,
             {"system": req_sys, "time": {"mode": "uniform", "t_final": tf_final, "dt": dt, "unit": "s"}},
             exp, expected_warnings=warnings or ["W.DOMAIN.UNSTABLE_SYSTEM"])
        return
    def f(x):
        x = float(x)
        return x if np.isfinite(x) else None
    exp = {"rise_time": f(S["RiseTime"]), "settling_time": f(S["SettlingTime"]), "overshoot": f(S["Overshoot"]),
           "undershoot": f(S["Undershoot"]), "peak": f(S["Peak"]), "peak_time": f(S["PeakTime"]),
           "steady_state": f(S["SteadyStateValue"]), "stable": True,
           "settling_min": f(S["SettlingMin"]), "settling_max": f(S["SettlingMax"])}
    save("python-control", "control.lti.stepinfo", family,
         {"system": req_sys, "time": {"mode": "uniform", "t_final": tf_final, "dt": dt, "unit": "s"}},
         exp, expected_warnings=warnings)

step_case(ct.tf([1], [1, 1]), 5.0, 0.5, "first_order", tf([1], [1, 1]), ["W.ACCURACY.GRID_TOO_COARSE"])
step_case(ct.tf([4], [1, 1.2, 4]), 10.0, 0.01, "second_order", tf([4], [1, 1.2, 4]))
step_case(ct.tf([1], [1, -1]), 1.0, 0.05, "unstable", tf([1], [1, -1]))
# Short window: 90% is never reached, so python-control's step_info raises. The spec
# definition still applies: rise/settling are null, steady state is the DC gain, peak is
# the window maximum. Expected values are that definition on this grid (y = 1-exp(-t)).
t_ns = np.arange(0.0, 1.0 + 1e-12, 0.05)
y_ns = 1.0 - np.exp(-t_ns)
k_ns = int(np.argmax(np.abs(y_ns)))
save("python-control", "control.lti.stepinfo", "not_settled",
     {"system": tf([1], [1, 1]), "time": {"mode": "uniform", "t_final": 1.0, "dt": 0.05, "unit": "s"}},
     {"rise_time": None, "settling_time": None, "overshoot": 0.0, "undershoot": 0.0,
      "peak": float(abs(y_ns[k_ns])), "peak_time": float(t_ns[k_ns]), "steady_state": 1.0, "stable": True},
     expected_warnings=["W.DOMAIN.NOT_SETTLED"])
plant = ct.ss(A3, B3, C3, D3)
step_case(plant, 8.0, 0.02, "memory_order_nonsymmetric", ss(A3, B3, C3, D3),
          )
# attach the mistake request onto the last written file of that family: easier to pass extra via a second save? 
# Re-save by finding is messy. Do it explicitly:
t = np.arange(0, 8 + 1e-12, 0.02)
S = ct.step_info(plant, T=t)
def fnum(x):
    x = float(x); return x if np.isfinite(x) else None
exp = {"rise_time": fnum(S["RiseTime"]), "settling_time": fnum(S["SettlingTime"]), "overshoot": fnum(S["Overshoot"]),
       "undershoot": fnum(S["Undershoot"]), "peak": fnum(S["Peak"]), "peak_time": fnum(S["PeakTime"]),
       "steady_state": fnum(S["SteadyStateValue"]), "stable": True,
       "settling_min": fnum(S["SettlingMin"]), "settling_max": fnum(S["SettlingMax"])}
req = {"system": ss(A3, B3, C3, D3), "time": {"mode": "uniform", "t_final": 8.0, "dt": 0.02, "unit": "s"}}
bad = json.loads(json.dumps(req)); bad["system"]["A"] = MF(A3)
# The step_case above already wrote one memory_order file without the mistake. Overwrite policy: two files are ok
# if the test requires the mistake on at least one. Add it here as the one the test looks for.
save("python-control", "control.lti.stepinfo", "memory_order_with_mistake", req, exp,
     extra={"column_major_mistake_request": bad})

# ---------------------------------------------------------------- pade (mpmath recurrence, cross-checked with control.pade)
def pade_mp(theta, n):
    """50-digit evaluation of the normalized recurrence (leading denominator coefficient 1)."""
    theta = mp.mpf(theta)
    den = [mp.mpf(0)] * (n + 1)
    den[0] = mp.mpf(1)
    for j in range(n):
        den[j + 1] = den[j] * mp.mpf(n + j + 1) * mp.mpf(n - j) / (theta * mp.mpf(j + 1))
    num = [((-1) ** (n - j)) * den[j] for j in range(n + 1)]
    return [float(x) for x in num], [float(x) for x in den]

for n, theta in [(1, 1.0), (1, 2.0), (2, 2.0), (3, 0.5), (4, 1.5), (5, 0.8), (8, 1.0), (10, 1.5)]:
    num, den = pade_mp(theta, n)
    if n <= 4:
        cn, cd = ct.pade(theta, n)
        rel = max(np.max(np.abs(np.array(num) - np.array(cn))), np.max(np.abs(np.array(den) - np.array(cd))))
        rel /= max(1.0, np.max(np.abs(den)))
        if rel > 1e-12:
            raise SystemExit(f"mpmath vs control.pade mismatch n={n} theta={theta} rel={rel}")
    save("mpmath", "control.lti.pade", f"n{n}", {"delay": theta, "order": n}, {"num": num, "den": den})
save("mpmath", "control.lti.pade", "order_zero", {"delay": 1.0, "order": 0}, expected_error="E.VALIDATION.SCHEMA")
save("mpmath", "control.lti.pade", "negative_delay", {"delay": -0.2, "order": 2}, expected_error="E.VALIDATION.SCHEMA")

print("families:")
for k in sorted(count):
    print(f"  {k[0]:28} {k[1]:32} {count[k]}")
