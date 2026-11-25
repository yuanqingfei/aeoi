"""Golden cases for signal.filter.filter (spec v0.4.12, 10.25).

Engines: scipy.signal.lfilter / sosfilt as the everyday reference, and the difference equation evaluated at
50 digits by mpmath on the *stored* coefficients and signal (the exact output for the data as given).
NOT MATLAB output (MATLAB filter is the same transposed direct form II recursion as lfilter).
"""
import hashlib, json, os, platform, shutil
import numpy as np
import scipy
from scipy import signal
import mpmath as mp

mp.mp.dps = 50
SEED = 20261125
rng = np.random.default_rng(SEED)
OP, VER = "signal.filter.filter", "0.4.0"
ENV = {"engine": "scipy.signal.lfilter / sosfilt / mpmath", "scipy": scipy.__version__, "numpy": np.__version__, "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
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


def enc(x):
    x = np.asarray(x, float)
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def mp_ba(b, a, x, zi=None):
    """Exact output of the difference equation (zero initial conditions, or the DF2T state zi converted)."""
    m = max(len(b), len(a))
    B = [mp.mpf(float(v)) for v in b] + [mp.mpf(0)] * (m - len(b))
    A = [mp.mpf(float(v)) for v in a] + [mp.mpf(0)] * (m - len(a))
    B = [v / A[0] for v in B]; A = [v / A[0] for v in A]
    z = [mp.mpf(float(v)) for v in zi] if zi is not None else [mp.mpf(0)] * (m - 1)
    y = []
    for xn in x:
        xn = mp.mpf(float(xn))
        yn = (z[0] if m > 1 else 0) + B[0] * xn
        for i in range(m - 1):
            z[i] = (z[i + 1] if i + 1 < m - 1 else 0) + B[i + 1] * xn - A[i + 1] * yn
        y.append(yn)
    return y, z


def radius(a_list):
    return max([float(np.max(np.abs(np.roots(a)))) if len(np.trim_zeros(np.asarray(a, float), "b")) > 1 else 0.0 for a in a_list])


def case(family, x, b=None, a=None, sos=None, zi=None, expect_warning=None):
    x = np.asarray(x, float)
    X = x.reshape(1, -1) if x.ndim == 1 else x
    req = {"x": enc(x)}
    ys, zfs, ref = [], [], []
    if sos is None:
        req["b"] = [float(v) for v in b]; req["a"] = [float(v) for v in a]
        if zi is not None: req["zi"] = enc(zi)
        Z = None if zi is None else (np.asarray(zi).reshape(X.shape[0], -1) if np.asarray(zi).ndim == 2 else np.tile(zi, (X.shape[0], 1)))
        for c, xc in enumerate(X):
            y, z = mp_ba(b, a, xc, None if Z is None else Z[c]); ys.append(y); zfs.append(z)
            ref.append(signal.lfilter(b, a, xc, zi=None if Z is None else Z[c])[0] if Z is not None else signal.lfilter(b, a, xc))
        r = radius([a])
    else:
        sos = np.asarray(sos, float); req["sos"] = enc(sos)
        if zi is not None: req["zi"] = enc(zi)
        for c, xc in enumerate(X):
            cur = list(xc); zf = []
            for l, s in enumerate(sos):
                zs = None if zi is None else (np.asarray(zi)[c, l] if np.asarray(zi).ndim == 3 else np.asarray(zi)[l])
                cur, z = mp_ba(s[:3], s[3:], cur, zs); zf.extend(z)
            ys.append(cur); zfs.append(zf)
            ref.append(signal.sosfilt(sos, xc) if zi is None else signal.sosfilt(sos, xc, zi=np.asarray(zi)[c] if np.asarray(zi).ndim == 3 else zi)[0])
        r = radius([s[3:] for s in sos])
    yt = np.array([[float(v) for v in y] for y in ys]); zt = np.array([[float(v) for v in z] for z in zfs])
    ref = np.array(ref)
    exp = {"y_true": [float(v) for v in yt.ravel()], "zf_true": [float(v) for v in zt.ravel()], "y_shape": list(x.shape),
           "max_pole_radius": r, "scipy_error": float(np.max(np.abs(ref - yt)) / max(np.max(np.abs(yt)), 1e-300))}
    save(family, req, exp, expect_warning=expect_warning)


t = np.arange(400) / 1000.0
sig = np.sin(2 * np.pi * 10 * t) + 0.5 * np.sin(2 * np.pi * 120 * t) + 0.1 * rng.normal(size=t.size)
case("fir_moving_average", sig[:50], b=np.ones(5) / 5, a=[1.0])
case("iir_first_order", rng.normal(size=60), b=[0.1], a=[1.0, -0.9])
b4, a4 = signal.butter(4, 0.1)
case("butter4_ba", sig, b=b4, a=a4)
case("a0_not_one", rng.normal(size=40), b=[2.0, 1.0], a=[4.0, -1.0, 0.5])
case("b_longer_than_a", rng.normal(size=40), b=[1.0, 0.5, 0.25, 0.125], a=[1.0, -0.3])
case("a_longer_than_b", rng.normal(size=40), b=[1.0], a=[1.0, -0.5, 0.2, -0.1])
case("ba_with_zi", rng.normal(size=30), b=b4, a=a4, zi=rng.normal(size=4))
case("multichannel_ba", rng.normal(size=(3, 50)), b=b4, a=a4)
s8 = signal.butter(8, [0.2, 0.3], btype="bandpass", output="sos")
case("butter8_bandpass_sos", sig, sos=s8)
case("narrowband_sos", sig, sos=signal.butter(4, [0.0995, 0.1005], btype="bandpass", output="sos"))
s3 = signal.butter(3, 0.2, output="sos")
case("sos_with_zi", rng.normal(size=30), sos=s3, zi=rng.normal(size=(len(s3), 2)))
case("multichannel_sos", rng.normal(size=(2, 80)), sos=s3)
case("long_signal_sos", rng.normal(size=10000), sos=signal.butter(6, 0.05, output="sos"))
case("unstable_warns", np.r_[1.0, np.zeros(19)], b=[1.0], a=[1.0, -1.1], expect_warning="W.DOMAIN.UNSTABLE_SYSTEM")
case("marginal_integrator_warns", np.ones(20), b=[1.0], a=[1.0, -1.0], expect_warning="W.DOMAIN.UNSTABLE_SYSTEM")
# ---- errors
save("both_forms", {"x": enc([1.0, 2.0]), "b": [1.0], "a": [1.0], "sos": enc([[1, 0, 0, 1, 0, 0]])}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("no_filter", {"x": enc([1.0, 2.0])}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("a0_zero", {"x": enc([1.0, 2.0]), "b": [1.0], "a": [0.0, 1.0]}, expected_error="E.VALIDATION.ZERO_DENOMINATOR")
save("sos_a0_zero", {"x": enc([1.0, 2.0]), "sos": enc([[1, 0, 0, 0, 1, 0]])}, expected_error="E.VALIDATION.ZERO_DENOMINATOR")
save("sos_bad_shape", {"x": enc([1.0, 2.0]), "sos": enc([[1, 0, 0, 1, 0]])}, expected_error="E.VALIDATION.SHAPE")
save("zi_wrong_length", {"x": enc([1.0, 2.0]), "b": [1.0, 1.0, 1.0], "a": [1.0], "zi": enc([0.0])}, expected_error="E.VALIDATION.SHAPE")
save("x_3d", {"x": enc(np.zeros((2, 2, 2))), "b": [1.0], "a": [1.0]}, expected_error="E.VALIDATION.SHAPE")
save("nan_x", {"x": {"dtype": "f64", "shape": [2], "data": [1.0, "NaN"]}, "b": [1.0], "a": [1.0]}, expected_error="E.VALIDATION.NON_FINITE")
save("empty_b", {"x": enc([1.0]), "b": [], "a": [1.0]}, expected_error="E.VALIDATION.SCHEMA")
print("filter golden:", count)

E = f"examples/{OP}"
os.makedirs(E, exist_ok=True)
sos_ex = signal.butter(4, 50.0, fs=1000.0, output="sos")
ex = {"request.json": {"x": enc(np.round(sig[:200], 6)), "sos": enc(sos_ex)},
      "request.ba.json": {"x": enc([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]), "b": [0.1], "a": [1.0, -0.9]},
      "request.unstable.json": {"x": enc([1.0] + [0.0] * 9), "b": [1.0], "a": [1.0, -1.2]},
      "request.error.json": {"x": enc([1.0, 2.0, 3.0]), "b": [1.0, 0.5], "a": [0.0, 1.0]}}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
ref = {"note": "y_true = mpmath 50-digit difference equation; NOT MATLAB output"}
y, _ = mp_ba([0.1], [1.0, -0.9], ex["request.ba.json"]["x"]["data"]); ref["request.ba.json"] = {"y_true": [float(v) for v in y]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("filter examples →", E)
