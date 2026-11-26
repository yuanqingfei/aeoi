"""Golden cases for signal.spectral.fft (spec v0.4.7, 10.17).

Engines: numpy.fft (pocketfft; fft/ifft with norm=..., rfft for sides = one, fftfreq/rfftfreq) for every case,
and mpmath (50-digit direct DFT, the exact answer) for small lengths. NOT MATLAB output (MATLAB fft/ifft use the
same definition as normalization = backward).
"""
import base64, hashlib, json, os, platform, shutil
import numpy as np
import mpmath as mp

SEED = 20261124
rng = np.random.default_rng(SEED)
OP, VER = "signal.spectral.fft", "0.4.0"
ENV_NP = {"engine": "numpy.fft (pocketfft)", "numpy": np.__version__, "python": platform.python_version(), "seed": SEED, "note": "NOT MATLAB output"}
ENV_MP = {"engine": "mpmath direct DFT, 50 digits", "mpmath": mp.__version__, "python": platform.python_version(), "seed": SEED, "note": "exact reference (rounded to f64); NOT MATLAB output"}
DIRS = {e: f"golden/{e}/{OP}/{VER}" for e in ("python-control", "mpmath")}
for d in DIRS.values():
    shutil.rmtree(d, ignore_errors=True); os.makedirs(d)


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


count = {"python-control": 0, "mpmath": 0}
def save(engine, family, request, expected=None, expected_error=None):
    case = {"spec": "aeoi/0.4", "operator": OP, "operator_version": VER, "case_family": family, "request": request,
            "reference_env": ENV_MP if engine == "mpmath" else ENV_NP}
    if expected is not None: case["expected"] = expected
    if expected_error is not None: case["expected_error"] = expected_error
    h = hashlib.sha256(canon(case).encode()).hexdigest()[:12]
    json.dump(case, open(f"{DIRS[engine]}/{family}_{h}.json", "w"), indent=1, ensure_ascii=False)
    count[engine] += 1


def enc(x):
    x = np.asarray(x)
    if np.iscomplexobj(x):
        return {"dtype": "c128", "shape": list(x.shape), "data": [[float(z.real), float(z.imag)] for z in x.ravel()]}
    return {"dtype": "f64", "shape": list(x.shape), "data": [float(v) for v in x.ravel()]}


def b64(x):
    a = np.ascontiguousarray(np.asarray(x, "<f8")); raw = a.tobytes()
    return {"dtype": "f64", "shape": list(a.shape), "order": "C", "byte_order": "little",
            "data_b64": base64.b64encode(raw).decode(), "sha256": "sha256:" + hashlib.sha256(raw).hexdigest()}


def ref_numpy(x, n=None, direction="forward", norm="backward", sides="two", fs=None):
    x = np.atleast_2d(np.asarray(x)) if np.ndim(x) == 2 else np.asarray(x)
    nn = n or x.shape[-1]
    if sides == "one":
        X = np.fft.rfft(x, n=nn, axis=-1, norm=norm)
        f = np.fft.rfftfreq(nn, d=1.0 / (fs or 1.0))
    else:
        X = (np.fft.ifft if direction == "inverse" else np.fft.fft)(x, n=nn, axis=-1, norm=norm)
        f = np.fft.fftfreq(nn, d=1.0 / (fs or 1.0))
    return X, f, nn


def expected_of(X, f, nn, x, direction, norm, sides, fs):
    X = np.asarray(X)
    return {"X": [[float(z.real), float(z.imag)] for z in X.ravel()], "shape": list(X.shape), "frequency": [float(v) for v in f],
            "frequency_unit": "Hz" if fs else "cycles/sample", "n": nn, "input_length": int(np.shape(x)[-1]),
            "direction": direction, "normalization": norm, "sides": sides}


def case(family, x, n=None, direction=None, norm=None, sides=None, fs=None, x_enc=None):
    req = {"x": x_enc if x_enc is not None else enc(x)}
    if n is not None: req["n"] = n
    if direction is not None: req["direction"] = direction
    if norm is not None: req["normalization"] = norm
    if sides is not None: req["sides"] = sides
    if fs is not None: req["sample_rate_hz"] = fs
    X, f, nn = ref_numpy(x, n, direction or "forward", norm or "backward", sides or "two", fs)
    save("python-control", family, req, expected_of(X, f, nn, x, direction or "forward", norm or "backward", sides or "two", fs))


def mp_dft(x, inverse, norm):
    mp.mp.dps = 50
    n = len(x); sgn = 1 if inverse else -1
    s = {"backward": (mp.mpf(1) / n if inverse else mp.mpf(1)), "ortho": 1 / mp.sqrt(n), "forward": (mp.mpf(1) if inverse else mp.mpf(1) / n)}[norm]
    xs = [mp.mpc(complex(v)) for v in x]
    return [s * mp.fsum(xs[j] * mp.expjpi(sgn * mp.mpf(2 * ((j * k) % n)) / n) for j in range(n)) for k in range(n)]


def case_mp(family, x, direction="forward", norm="backward"):
    req = {"x": enc(x)}
    if direction != "forward": req["direction"] = direction
    if norm != "backward": req["normalization"] = norm
    X = mp_dft(list(np.asarray(x).ravel()), direction == "inverse", norm)
    Xc = np.array([complex(z) for z in X])
    save("mpmath", family, req, expected_of(Xc, np.fft.fftfreq(len(x)), len(x), x, direction, norm, "two", None))


# ---- analytic and basic
t16 = np.arange(16)
case("single_tone", np.cos(2 * np.pi * 3 * t16 / 16), fs=16.0)
case("impulse", np.eye(1, 32, 0).ravel())                       # δ → all ones
case("constant", np.ones(10))
case("length_one", np.array([2.5]))
# ---- lengths: powers of two, odd composite, prime (Bluestein/Rader paths)
for nn in (8, 64, 1024):
    case("real_pow2", rng.normal(size=nn))
for nn in (15, 99, 360):
    case("real_composite", rng.normal(size=nn))
for nn in (97, 1009):
    case("real_prime", rng.normal(size=nn))
for nn in (128, 255):
    case("complex_random", rng.normal(size=nn) + 1j * rng.normal(size=nn))
# ---- conventions
xc = rng.normal(size=64) + 1j * rng.normal(size=64)
for norm in ("backward", "ortho", "forward"):
    case("normalization_forward", xc, norm=norm)
    case("normalization_inverse", xc, direction="inverse", norm=norm)
case("explicit_defaults", rng.normal(size=32), direction="forward", norm="backward", sides="two")
case("one_sided_even", rng.normal(size=64), sides="one", fs=1000.0)
case("one_sided_odd", rng.normal(size=63), sides="one", fs=1000.0)
case("one_sided_ortho", rng.normal(size=50), sides="one", norm="ortho")
case("sample_rate", np.sin(2 * np.pi * 50 * np.arange(200) / 1000.0), fs=1000.0)
case("zero_pad", rng.normal(size=100), n=128)
case("truncate", rng.normal(size=100), n=64)
case("multichannel", rng.normal(size=(3, 256)), fs=200.0)
case("multichannel_complex", rng.normal(size=(2, 30)) + 1j * rng.normal(size=(2, 30)), direction="inverse")
xl = rng.normal(size=8192)
case("long_b64", xl, sides="one", fs=48000.0, x_enc=b64(xl))
# ---- exact reference (mpmath)
for nn in (8, 16, 17, 31):
    case_mp("exact_real", rng.normal(size=nn))
case_mp("exact_complex", rng.normal(size=12) + 1j * rng.normal(size=12))
case_mp("exact_inverse_ortho", rng.normal(size=9) + 1j * rng.normal(size=9), direction="inverse", norm="ortho")
# ---- errors
save("python-control", "complex_one_sided", {"x": enc(xc[:8]), "sides": "one"}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("python-control", "inverse_one_sided", {"x": enc(np.ones(8)), "sides": "one", "direction": "inverse"}, expected_error="E.VALIDATION.OPTION_CONFLICT")
save("python-control", "three_d", {"x": {"dtype": "f64", "shape": [2, 2, 2], "data": [1.0] * 8}}, expected_error="E.VALIDATION.SHAPE")
save("python-control", "bool_dtype", {"x": {"dtype": "bool", "shape": [3], "data": [True, False, True]}}, expected_error="E.VALIDATION.DTYPE")
save("python-control", "nonfinite", {"x": {"dtype": "f64", "shape": [3], "data": [1.0, "NaN", 2.0]}}, expected_error="E.VALIDATION.NON_FINITE")
save("python-control", "n_zero", {"x": enc(np.ones(4)), "n": 0}, expected_error="E.VALIDATION.SCHEMA")
save("python-control", "missing_x", {"n": 4}, expected_error="E.VALIDATION.SCHEMA")
save("python-control", "bad_normalization", {"x": enc(np.ones(4)), "normalization": "unitary"}, expected_error="E.VALIDATION.SCHEMA")
print("fft golden:", count)

# ---- published examples
E = f"examples/{OP}"
fs_ex = 1000.0
t_ex = np.arange(100) / fs_ex
x_ex = np.sin(2 * np.pi * 50 * t_ex) + 0.5 * np.sin(2 * np.pi * 120 * t_ex)
ex = {"request.json": {"x": enc(x_ex), "sample_rate_hz": fs_ex, "sides": "one"},
      "request.complex.json": {"x": enc(np.exp(2j * np.pi * 2 * np.arange(8) / 8)), "normalization": "ortho"},
      "request.error.json": {"x": enc(np.exp(2j * np.pi * 2 * np.arange(8) / 8)), "sides": "one"}}
ref = {"note": "numpy.fft (pocketfft); NOT MATLAB output"}
for name, req in ex.items():
    open(f"{E}/{name}", "w").write(json.dumps(req, indent=2, ensure_ascii=False) + "\n")
X1, f1, _ = ref_numpy(x_ex, sides="one", fs=fs_ex)
ref["request.json"] = {"X": [[float(z.real), float(z.imag)] for z in X1], "frequency": [float(v) for v in f1]}
X2, f2, _ = ref_numpy(np.exp(2j * np.pi * 2 * np.arange(8) / 8), norm="ortho")
ref["request.complex.json"] = {"X": [[float(z.real), float(z.imag)] for z in X2], "frequency": [float(v) for v in f2]}
open(f"{E}/reference.python-control.json", "w").write(json.dumps(ref, indent=1) + "\n")
print("fft examples →", E)
