"""Golden cases for signal.design.butter (spec v0.4.12, 10.24).

Engines: scipy.signal.butter (output zpk / ba / sos) as the everyday reference, and an independent 50-digit
mpmath implementation of the same construction (prototype, lp2*_zpk transforms, prewarped bilinear
transform, polynomial expansion) as the truth. NOT MATLAB output (MATLAB butter uses the same construction).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import scipy
from scipy import signal
import mpmath as mp

mp.mp.dps = 50
OP, VER = "signal.design.butter", "0.4.0"
ENV = {"engine": "scipy.signal.butter / mpmath", "scipy": scipy.__version__, "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "note": "NOT MATLAB output"}
DIR = f"golden/python-control/{OP}/{VER}"
shutil.rmtree(DIR, ignore_errors=True); os.makedirs(DIR)
count = 0


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def save(family, request, expected=None, expected_error=None, expect_warning=None):
    global count
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request, "reference_env": ENV}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    if expect_warning: case["expect_warning"] = expect_warning
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{DIR}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False, allow_nan=False)
    count += 1


def mp_butter(n, btype, edges, analog):
    """Truth: the same construction at 50 digits."""
    p = [-mp.exp(1j * mp.pi * m / (2 * n)) for m in range(-n + 1, n, 2)]
    z = []; k = mp.mpf(1)
    w = [mp.mpf(e) if analog else 4 * mp.tan(mp.pi * mp.mpf(e) / 2) for e in edges]
    if btype == "low":
        z = [x * w[0] for x in z]; p = [x * w[0] for x in p]; k = k * w[0] ** n
    elif btype == "high":
        k = k * mp.re(mp.fprod([-x for x in z]) / mp.fprod([-x for x in p]))
        z = [w[0] / x for x in z] + [mp.mpc(0)] * n; p = [w[0] / x for x in p]
    else:
        wo = mp.sqrt(w[0] * w[1]); bw = w[1] - w[0]
        if btype == "bandpass":
            pl = [x * bw / 2 for x in p]
            p = [x + mp.sqrt(x * x - wo * wo) for x in pl] + [x - mp.sqrt(x * x - wo * wo) for x in pl]
            z = [mp.mpc(0)] * n; k = k * bw ** n
        else:
            k = k * mp.re(1 / mp.fprod([-x for x in p]))
            ph = [bw / 2 / x for x in p]
            p = [x + mp.sqrt(x * x - wo * wo) for x in ph] + [x - mp.sqrt(x * x - wo * wo) for x in ph]
            z = [1j * wo] * n + [-1j * wo] * n
    if not analog:
        fs2 = 4
        k = k * mp.re(mp.fprod([fs2 - x for x in z]) / mp.fprod([fs2 - x for x in p]))
        z = [(fs2 + x) / (fs2 - x) for x in z] + [mp.mpc(-1)] * (len(p) - len(z))
        p = [(fs2 + x) / (fs2 - x) for x in p]
    def poly(r):
        c = [mp.mpc(1)]
        for x in r:
            c = [a - x * b for a, b in zip(c + [0], [0] + c)]
        return [float(mp.re(v)) for v in c]
    b = [k * v for v in poly(z)] if z else [k]
    return [complex(x) for x in z], [complex(x) for x in p], float(k), [float(v) for v in b], poly(p)


def H(z, p, k, x):
    return k * np.prod([x - zz for zz in z]) / np.prod([x - pp for pp in p])


def case(family, req, expect_warning=None):
    n = req["order"]; analog = req.get("domain") == "analog"
    if analog:
        edges = req.get("cutoff_rad_s", None)
        if edges is None: edges = np.array(req["cutoff_hz"], float) * 2 * np.pi
        edges = list(np.atleast_1d(edges)); fs = None
    elif "cutoff_hz" in req:
        edges = list(np.atleast_1d(np.array(req["cutoff_hz"], float) / (req["sample_rate_hz"] / 2)))
    else:
        edges = list(np.atleast_1d(req["cutoff_normalized"]))
    btype = req.get("type", "bandpass" if len(edges) == 2 else "low")
    z, p, k, b, a = mp_butter(n, btype, edges, analog)
    sz, sp, sk = signal.butter(n, edges if len(edges) == 2 else edges[0], btype=btype, analog=analog, output="zpk")
    sb, sa = signal.butter(n, edges if len(edges) == 2 else edges[0], btype=btype, analog=analog, output="ba")
    pts = (1j * np.sqrt(np.prod(edges)) ** (1 / 1) * 10 ** np.linspace(-2, 2, 64) if analog else np.exp(1j * np.pi * (np.arange(64) + 0.5) / 64))
    if analog and len(edges) == 2:
        pts = 1j * np.sqrt(edges[0] * edges[1]) * 10 ** np.linspace(-2, 2, 64)
    Ht = np.array([H(z, p, k, x) for x in pts])
    Hb = np.array([np.polyval(sb, x) / np.polyval(sa, x) for x in pts]) if analog else np.array([np.polyval(sb[::-1], 1 / x) / np.polyval(sa[::-1], 1 / x) for x in pts])
    dev = float(np.max(np.abs(Hb - Ht)) / np.max(np.abs(Ht)))
    assert dev < 1e-10 or dev > 1e-6, (family, dev)
    assert (dev > 1e-6) == (expect_warning is not None), (family, dev)
    exp = {"z_true": [[v.real, v.imag] for v in z], "p_true": [[v.real, v.imag] for v in p], "k_true": k, "b_true": b, "a_true": a,
           "z_scipy": [[float(v.real), float(v.imag)] for v in sz], "p_scipy": [[float(v.real), float(v.imag)] for v in sp], "k_scipy": float(sk),
           "b_scipy": [float(v) for v in sb], "a_scipy": [float(v) for v in sa], "type": btype, "edges": [float(e) for e in edges], "analog": analog,
           "scipy_ba_response_deviation": dev}
    save(family, req, exp, expect_warning=expect_warning)


for n in (1, 2, 3, 4, 6, 8):
    case("low_normalized", {"order": n, "cutoff_normalized": 0.3})
case("low_hz", {"order": 4, "cutoff_hz": 50.0, "sample_rate_hz": 1000.0})
case("low_near_nyquist", {"order": 5, "cutoff_normalized": 0.95})
case("high_normalized", {"order": 5, "type": "high", "cutoff_normalized": 0.2})
case("high_hz", {"order": 3, "type": "high", "cutoff_hz": 1.0, "sample_rate_hz": 100.0})
case("bandpass_default_type", {"order": 4, "cutoff_hz": [8.0, 12.0], "sample_rate_hz": 100.0})
case("bandpass_explicit", {"order": 3, "type": "bandpass", "cutoff_normalized": [0.1, 0.4]})
case("bandstop_mains", {"order": 2, "type": "bandstop", "cutoff_hz": [48.0, 52.0], "sample_rate_hz": 1000.0})
case("bandstop_wide", {"order": 5, "type": "bandstop", "cutoff_normalized": [0.2, 0.6]})
case("analog_low", {"order": 4, "domain": "analog", "cutoff_rad_s": 2.0})
case("analog_high_hz", {"order": 3, "domain": "analog", "type": "high", "cutoff_hz": 10.0})
case("analog_bandpass", {"order": 2, "domain": "analog", "cutoff_rad_s": [1.0, 4.0]})
case("analog_bandstop", {"order": 3, "domain": "analog", "type": "bandstop", "cutoff_rad_s": [5.0, 6.0]})
case("high_order_low_cutoff_warns", {"order": 20, "cutoff_normalized": 0.01}, expect_warning="W.NUMERICAL.ILL_CONDITIONED")
case("bandpass_narrow_warns", {"order": 10, "cutoff_hz": [9.9, 10.1], "sample_rate_hz": 1000.0}, expect_warning="W.NUMERICAL.ILL_CONDITIONED")
# ---- errors
save("no_cutoff", {"order": 2}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("two_cutoff_forms", {"order": 2, "cutoff_hz": 10.0, "sample_rate_hz": 100.0, "cutoff_normalized": 0.2}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("hz_without_fs", {"order": 2, "cutoff_hz": 10.0}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("normalized_with_fs", {"order": 2, "cutoff_normalized": 0.2, "sample_rate_hz": 100.0}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("analog_normalized", {"order": 2, "domain": "analog", "cutoff_normalized": 0.2}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("rad_s_digital", {"order": 2, "cutoff_rad_s": 1.0}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("above_nyquist_hz", {"order": 2, "cutoff_hz": 60.0, "sample_rate_hz": 100.0}, expected_error="E.DOMAIN.ABOVE_NYQUIST")
save("wn_is_one", {"order": 2, "cutoff_normalized": 1.0}, expected_error="E.DOMAIN.ABOVE_NYQUIST")
save("band_one_value", {"order": 2, "type": "bandpass", "cutoff_normalized": 0.3}, expected_error="E.VALIDATION.SHAPE")
save("band_reversed", {"order": 2, "cutoff_normalized": [0.4, 0.1]}, expected_error="E.VALIDATION.RANGE")
save("order_zero", {"order": 0, "cutoff_normalized": 0.3}, expected_error="E.VALIDATION.SCHEMA")
save("order_too_high", {"order": 31, "cutoff_normalized": 0.3}, expected_error="E.VALIDATION.RANGE")
save("negative_cutoff", {"order": 2, "cutoff_normalized": -0.3}, expected_error="E.VALIDATION.SCHEMA")
print("butter golden:", count)

E = f"examples/{OP}"
os.makedirs(E, exist_ok=True)
ex = {"request.json": {"order": 4, "cutoff_hz": 50.0, "sample_rate_hz": 1000.0},
      "request.analog.json": {"order": 3, "domain": "analog", "cutoff_rad_s": 10.0},
      "request.ill.json": {"order": 20, "cutoff_normalized": 0.01},
      "request.error.json": {"order": 4, "cutoff_hz": 50.0}}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
ref = {"note": "scipy.signal.butter; *_true = mpmath 50 digits; NOT MATLAB output"}
for name in ("request.json", "request.analog.json"):
    req = ex[name]; analog = req.get("domain") == "analog"
    edges = [req["cutoff_rad_s"]] if analog else [req["cutoff_hz"] / (req["sample_rate_hz"] / 2)]
    z, p, k, b, a = mp_butter(req["order"], "low", edges, analog)
    ref[name] = {"b_true": b, "a_true": a, "k_true": k}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("butter examples →", E)
