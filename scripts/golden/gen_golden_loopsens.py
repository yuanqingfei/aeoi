"""Golden cases for control.analysis.loopsens (spec v0.3.6, K-4/K-5).

python-control 0.10.2 — NOT MATLAB. Each case is the chained primitives:
L = series(controller, plant) [then · sensor], T = feedback(plant·controller, sensor),
ct.stability_margins(L), ct.bandwidth(T, dbdrop) and ct.step_info(T, T=grid).
"""
import json, hashlib, os, platform, shutil
import numpy as np
import control as ct

SEED = 20261124
rng = np.random.default_rng(SEED)
ENV = {"engine": "python-control", "python_control": ct.__version__, "numpy": np.__version__,
       "python": platform.python_version(), "seed": SEED, "note": "python-control reference values; NOT MATLAB output"}
OP, VER = "control.analysis.loopsens", "0.4.0"
D = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(D, ignore_errors=True); os.makedirs(D)

def lst(a):
    a = np.atleast_1d(np.asarray(a, float)).reshape(-1)
    nz = np.flatnonzero(a)
    a = a[nz[0]:] if nz.size else np.array([0.0])
    return [float(x) for x in a]
def tf(num, den):
    return {"kind": "tf", "num": lst(num), "den": lst(den), "ts": 0.0, "time_unit": "s"}
def coeffs(s):
    return {"kind": "tf", "num": lst(s.num[0][0]), "den": lst(s.den[0][0])}
def fin(x):
    x = float(x); return x if np.isfinite(x) else None
def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
n = 0
def save(family, request, expected=None, expected_error=None):
    global n
    case = {"spec": "aeoi/0.3", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    h = hashlib.sha256(canon(case).encode()).hexdigest()
    json.dump(case, open(f"{D}/{h}.json", "w"), indent=1, ensure_ascii=False); n += 1

def case(family, G, C=None, H=None, t=None):
    Gs = ct.tf(*G); Cs = ct.tf(*C) if C else ct.tf([1], [1]); Hs = ct.tf(*H) if H else ct.tf([1], [1])
    fwd = ct.series(Cs, Gs)
    L = ct.series(fwd, Hs)
    T = ct.feedback(fwd, Hs)
    poles = ct.poles(T)
    stable = bool(np.all(np.real(poles) < 0))
    gm, pm, sm, wpc, wgc, wms = ct.stability_margins(L)
    exp = {"open_loop": coeffs(L), "closed_loop": coeffs(T), "closed_loop_stable": stable,
           "phase_margin": fin(pm), "gain_crossover_rad_s": fin(wgc)}
    if stable:
        exp["bandwidth_rad_s"] = fin(ct.bandwidth(T))
    req = {"plant": tf(*G)}
    if C: req["controller"] = tf(*C)
    if H: req["sensor"] = tf(*H)
    if t is not None and stable:
        grid = np.arange(0.0, t[0] + 1e-12, t[1])
        try:
            S = ct.step_info(T, T=grid)
        except IndexError:  # 90 % not reached in the window: python-control raises; not compared
            S = None
        req["time"] = {"mode": "uniform", "t_final": t[0], "dt": t[1], "unit": "s"}
        if S is None:
            save(family, req, exp); return
        exp["step"] = {"rise_time": fin(S["RiseTime"]), "settling_time": fin(S["SettlingTime"]), "overshoot": fin(S["Overshoot"]),
                       "peak": fin(S["Peak"]), "steady_state": fin(S["SteadyStateValue"])}
        req["time"] = {"mode": "uniform", "t_final": t[0], "dt": t[1], "unit": "s"}
    save(family, req, exp)

# the published example: second-order plant + PI
case("example_pi", ([2], [1, 3, 2]), ([5, 10], [1, 0]), t=(10.0, 0.01))
# EngDesign XG_13-like loop: integrating plant + lead
case("integrating_plant_lead", ([1], [1, 2, 0]), ([30, 60], [1, 15]), t=(6.0, 0.005))
# non-unity sensor (first-order filter in the feedback path)
case("sensor_filter", ([4], [1, 2, 1]), ([2, 1], [1, 0]), ([1], [0.05, 1]), t=(30.0, 0.01))
# controller omitted (K-1 implicit unity) and sensor omitted
case("plant_only", ([10], [1, 1]), t=(2.0, 0.002))
# unstable closed loop: margins/bandwidth/step skipped or flagged
case("unstable_loop", ([1], [1, 3, 3, 1]), ([20], [1]))
# random stable plants with a PID-like controller
for i in range(6):
    p = rng.uniform(-4, -0.3, size=int(rng.integers(1, 4)))
    k = float(rng.uniform(0.5, 3.0))
    kp, ki, kd = (float(x) for x in rng.uniform(0.2, 2.0, size=3))
    tfc = 0.05
    # parallel PID with derivative filter: Kp + Ki/s + Kd s/(Tf s + 1)
    num = np.polyadd(np.polyadd(np.polymul([kp], [tfc, 1, 0]), np.polymul([ki], [tfc, 1])), [kd, 0, 0])
    den = [tfc, 1, 0]
    case("random_pid", ([k], np.real(np.poly(p))), (num, den), t=(20.0, 0.01))
# missing plant is a schema error (plant is the only required role)
save("missing_plant", {"controller": tf([1], [1, 0])}, expected_error="E.VALIDATION.SCHEMA")
print("loopsens golden:", n, "cases →", D)
