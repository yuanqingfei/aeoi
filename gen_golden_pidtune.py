"""Golden cases for control.design.pidtune (spec v0.4.6, 10.16).

Independent Python implementation of the spec's frequency-point design (PI closed form; PID with Ti = α·Td,
Tf = Td/N, smallest root of arg H(x) = ∠C* on the specified log scan + bisection), each result verified with
python-control 0.10.2: |L(jωc)| = 1 and ∠L(jωc) = −180° + PM, stability_margins and closed-loop poles.
NOT MATLAB (MATLAB pidtune picks ωc itself and uses a different objective).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import control as ct

SEED = 20261124
rng = np.random.default_rng(SEED)
ENV = {"engine": "numpy (spec formulas) + python-control stability_margins / poles", "python_control": ct.__version__,
       "numpy": np.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
OP, VER = "control.design.pidtune", "0.4.0"
D = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(D, ignore_errors=True); os.makedirs(D)
U = np.finfo(float).eps / 2
K = 4000


def tf(num, den, ts=0.0):
    return {"kind": "tf", "num": [float(x) for x in num], "den": [float(x) for x in den], "ts": ts}


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


n = 0
def save(family, request, expected=None, expected_error=None):
    global n
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{D}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False); n += 1


def design(num, den, typ, wc, pm=60.0, alpha=4.0, N=10.0):
    """Returns ('ok', gains) or ('not_achievable', why)."""
    g = np.polyval(num, 1j * wc) / np.polyval(den, 1j * wc)
    if not np.isfinite(g) or g == 0:
        return "not_achievable", "plant zero/pole at jwc"
    c = np.exp(1j * np.deg2rad(pm - 180.0)) / g
    theta = np.rad2deg(np.angle(c))
    if typ == "PI":
        kp, ki = c.real, -wc * c.imag
        if 0 > ki >= -4 * U * wc * abs(c):                   # PT-3: ∠C* = 0° within rounding is pure P
            ki = 0.0
        if not (kp > 0 and ki >= 0):
            return "not_achievable", f"PI phase {theta}"
        return "ok", {"Kp": kp, "Ki": ki, "Kd": 0.0, "Tf": 0.0}
    H = lambda x: 1 - 1j / (alpha * x) + 1j * x / (1 + 1j * x / N)
    phi = lambda x: np.rad2deg(np.angle(H(x)))
    xs = 10.0 ** (-6 + 12 * np.arange(K + 1) / K)
    gs = np.array([phi(x) - theta for x in xs])
    ks = [k for k in range(K) if gs[k] == 0 or np.sign(gs[k]) != np.sign(gs[k + 1])]
    if not ks:
        return "not_achievable", f"PID phase {theta} out of [{gs.min() + theta}, {gs.max() + theta}]"
    k = ks[0]
    lo, hi, glo = xs[k], xs[k + 1], gs[k]
    while hi - lo > 4 * U * hi:
        mid = np.sqrt(lo * hi)
        gm = phi(mid) - theta
        if gm == 0:
            lo = hi = mid; break
        if np.sign(gm) == np.sign(glo): lo = mid
        else: hi = mid
    x = np.sqrt(lo * hi) if glo != 0 else lo
    kp = abs(c) / abs(H(x)); td = x / wc; ti = alpha * td; tfc = td / N
    return "ok", {"Kp": kp, "Ki": kp / ti, "Kd": kp * td, "Tf": tfc}


def verify(num, den, gains, wc, pm):
    G = ct.tf(num, den)
    kp, ki, kd, tfc = gains["Kp"], gains["Ki"], gains["Kd"], gains["Tf"]
    if kd == 0:                                                     # pure P must not carry an s/s factor (pole at 0)
        C = ct.tf([kp, ki], [1, 0]) if ki != 0 else ct.tf([kp], [1])
    else:
        C = ct.tf([kp * tfc + kd, kp + ki * tfc, ki], [tfc, 1, 0])
    L = G * C
    Lw = complex(L(1j * wc))
    assert abs(abs(Lw) - 1) < 1e-9, abs(Lw)
    dph = (np.rad2deg(np.angle(Lw)) - (pm - 180) + 180) % 360 - 180
    assert abs(dph) < 1e-7, dph
    _, pm_a, _, _, wgc, _ = ct.stability_margins(L)
    stable = bool(np.all(np.real(ct.poles(ct.feedback(L, 1))) < 0))
    return {"phase_margin_deg": float(pm_a), "gain_crossover_rad_s": float(wgc), "closed_loop_stable": stable}


def case(family, num, den, typ, wc, pm=None, alpha=None, N=None, expect_error=None):
    req = {"plant": tf(num, den), "type": typ, "wc_rad_s": wc}
    if pm is not None: req["phase_margin_deg"] = pm
    if alpha is not None: req["ti_td_ratio"] = alpha
    if N is not None: req["derivative_filter_n"] = N
    st, out = design(np.array(num, float), np.array(den, float), typ, wc, pm or 60.0, alpha or 4.0, N or 10.0)
    if st != "ok":
        assert expect_error == "E.DOMAIN.TARGET_NOT_ACHIEVABLE", (family, out)
        save(family, req, expected_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE"); return None
    ach = verify(num, den, out, wc, pm or 60.0)
    if not ach["closed_loop_stable"]:
        assert expect_error == "E.DOMAIN.TARGET_NOT_ACHIEVABLE", (family, "unstable loop")
        save(family, req, expected_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE"); return None
    assert expect_error is None, (family, "expected an error")
    extra = abs(ach["gain_crossover_rad_s"] - wc) > 1e-6 * wc or ach["phase_margin_deg"] < (pm or 60.0) - 1e-6
    save(family, req, {"gains": {k: float(v) for k, v in out.items()}, "achieved": ach, "extra_crossover": bool(extra)})
    return ach


# ---- PI, closed form
case("pi_first_order", [1.0], [1.0, 1.0], "PI", 1.0)
case("pi_first_order", [2.0], [5.0, 1.0], "PI", 0.8, pm=70)
case("pi_second_order", [2.0], [1.0, 3.0, 2.0], "PI", 0.5)
case("pi_second_order", [2.0], [1.0, 3.0, 2.0], "PI", 1.0, pm=45)
case("pi_third_order_lag", [1.0], [1.0, 3.0, 3.0, 1.0], "PI", 0.3)
case("pi_integrating", [1.0], [1.0, 1.0, 0.0], "PI", 0.3)
case("pi_nonminimum_phase", [-1.0, 1.0], [1.0, 2.0, 1.0], "PI", 0.2)
case("pi_unstable_plant", [1.0], [1.0, -1.0], "PI", 2.0)
case("pi_explicit_default", [1.0], [1.0, 1.0], "PI", 1.0, pm=60)
# ---- PID (needs lead)
case("pid_third_order_lag", [1.0], [1.0, 3.0, 3.0, 1.0], "PID", 1.0, pm=45)
case("pid_third_order_lag", [1.0], [1.0, 3.0, 3.0, 1.0], "PID", 0.8)
case("pid_double_integrator", [1.0], [1.0, 0.0, 0.0], "PID", 1.0, pm=45)
case("pid_custom_ratio", [1.0], [1.0, 3.0, 3.0, 1.0], "PID", 1.0, pm=45, alpha=2.5, N=20)
case("pid_explicit_defaults", [1.0], [1.0, 3.0, 3.0, 1.0], "PID", 1.0, pm=45, alpha=4, N=10)
# ---- random stable plants, PI and PID at their natural crossover region
done = 0
while done < 8:
    p = rng.uniform(-5, -0.2, size=int(rng.integers(1, 4)))
    num = [float(rng.uniform(0.5, 3.0))]; den = list(np.real(np.poly(p)))
    typ = "PI" if done % 2 == 0 else "PID"
    wc = float(min(abs(p)) * rng.uniform(0.3, 1.5))
    pm = float(rng.choice([45.0, 55.0, 60.0, 70.0]))
    st, out = design(np.array(num), np.array(den), typ, wc, pm)
    if st != "ok": continue
    if not verify(num, den, out, wc, pm)["closed_loop_stable"]: continue
    case(f"random_{typ.lower()}", num, den, typ, wc, pm=pm); done += 1
# ---- extra crossover: a lightly damped resonance above ωc (searched, not guessed)
found = False
for wc in np.linspace(0.2, 1.5, 27):
    num, den = [25.0], list(np.polymul([1.0, 1.0], [1.0, 0.4, 25.0]))
    st, out = design(np.array(num), np.array(den), "PI", float(wc))
    if st != "ok": continue
    ach = verify(num, den, out, float(wc), 60.0)
    if ach["closed_loop_stable"] and (abs(ach["gain_crossover_rad_s"] - wc) > 1e-6 * wc or ach["phase_margin_deg"] < 60 - 1e-6):
        case("extra_crossover_resonance", num, den, "PI", float(wc)); found = True; break
assert found, "no extra-crossover case found"
# ---- errors
case("pi_needs_lead", [1.0], [1.0, 3.0, 3.0, 1.0], "PI", 1.0, expect_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE")     # ∠C* = +15°
case("pi_boundary_pure_p", [1.0], [1.0, 3.0, 3.0, 1.0], "PI", 1.0, pm=45)                                            # ∠C* = 0°: Ki = 0
case("pid_out_of_range", [1.0], [1.0, 0.0, 0.0, 0.0], "PID", 1.0, pm=80, expect_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE")
case("plant_zero_at_wc", [1.0, 0.0, 4.0], [1.0, 3.0, 3.0, 1.0], "PID", 2.0, expect_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE")
found = False                                                       # point conditions met but closed loop unstable
for wc in np.linspace(0.3, 6.0, 58):
    num, den = [25.0], list(np.polymul([1.0, 0.5], [1.0, 0.2, 25.0]))
    st, out = design(np.array(num), np.array(den), "PI", float(wc), 30.0)
    if st == "ok" and not verify(num, den, out, float(wc), 30.0)["closed_loop_stable"]:
        case("unstable_closed_loop", num, den, "PI", float(wc), pm=30, expect_error="E.DOMAIN.TARGET_NOT_ACHIEVABLE"); found = True; break
assert found, "no unstable-closed-loop case found"
save("pi_with_pid_option", {"plant": tf([1.0], [1.0, 1.0]), "type": "PI", "wc_rad_s": 1.0, "ti_td_ratio": 4}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("discrete_plant", {"plant": tf([1.0], [1.0, -0.5], ts=0.1), "type": "PI", "wc_rad_s": 1.0}, expected_error="E.DOMAIN.NOT_CONTINUOUS")
save("mimo_plant", {"plant": {"kind": "tf", "num": [[[1.0], [1.0]]], "den": [[[1.0, 1.0], [1.0, 2.0]]], "ts": 0.0}, "type": "PI", "wc_rad_s": 1.0}, expected_error="E.DOMAIN.MIMO_NOT_SUPPORTED")
save("missing_wc", {"plant": tf([1.0], [1.0, 1.0]), "type": "PI"}, expected_error="E.VALIDATION.SCHEMA")
save("missing_type", {"plant": tf([1.0], [1.0, 1.0]), "wc_rad_s": 1.0}, expected_error="E.VALIDATION.SCHEMA")
print("pidtune golden:", n, "cases →", D)

# ---- published examples
E = f"examples/{OP}"
ex = {"request.json": {"plant": tf([1.0], [1.0, 3.0, 3.0, 1.0]), "type": "PID", "wc_rad_s": 1.0},
      "request.pi.json": {"plant": tf([2.0], [1.0, 3.0, 2.0]), "type": "PI", "wc_rad_s": 1.0, "phase_margin_deg": 45},
      "request.error.json": {"plant": tf([1.0], [1.0, 3.0, 3.0, 1.0]), "type": "PI", "wc_rad_s": 1.0}}
ref = {"note": "numpy spec formulas verified with python-control 0.10.2 stability_margins; NOT MATLAB"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
    if name != "request.error.json":
        num, den = req["plant"]["num"], req["plant"]["den"]
        pm_ = req.get("phase_margin_deg", 60.0)
        st, out = design(np.array(num), np.array(den), req["type"], req["wc_rad_s"], pm_)
        ref[name] = {"gains": out, "achieved": verify(num, den, out, req["wc_rad_s"], pm_)}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("pidtune examples →", E)
