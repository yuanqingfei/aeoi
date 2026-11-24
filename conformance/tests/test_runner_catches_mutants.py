"""The runner must pass the unmodified second implementation and fail every mutant for the stated reason.

    python3 -I conformance/tests/test_runner_catches_mutants.py      (from the repository root)
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RUNNER = os.path.join(ROOT, "conformance", "aeoi_conformance.py")
MUTANT = os.path.join(ROOT, "conformance", "tests", "mutant_server.py")
SERVER = os.path.join(ROOT, "conformance", "impls", "python_scipy", "server.py")
PROXY = os.path.join(ROOT, "conformance", "tests", "proxy_mutant.py")
REF = os.path.join(ROOT, "rust", "target", "release", "aeoi-mcp")

# proxy mutants over the reference implementation (rules_control.py): needle that must appear among the failed
# checks, or None for an equivalent answer that must pass on every case
PROXY_EXPECT = {
    "pz_pole_wrong": ("control.lti.poles_zeros", "poles: matched set"),
    "pz_reordered": ("control.lti.poles_zeros", None),
    "bode_mag_wrong": ("control.lti.bode", "magnitude"),
    "bode_phase_360": ("control.lti.bode", "phase on the spec's branch"),
    "step_y_wrong": ("control.lti.step", "y: max"),
    "lqr_S_wrong": ("control.design.lqr", "S: rel_fro"),
    "lqr_poles_reordered": ("control.design.lqr", None),
    "c2d_A_wrong": ("control.lti.c2d", "transfer-function values"),
    "c2d_equivalent": ("control.lti.c2d", None),
    "lsim_y_wrong": ("control.lti.lsim", "y: max"),
    "lsim_interp_mislabeled": ("control.lti.lsim", "interpolation equals"),
    "pidtune_kp_wrong": ("control.design.pidtune", "gains Kp"),
    "pidtune_controller_wrong": ("control.design.pidtune", "recomputed by the runner"),
    "fft_bin_wrong": ("signal.spectral.fft", "X: per channel"),
    "fft_shifted_frequencies": ("signal.spectral.fft", "frequency: |Δ|"),
    "fft_scale_wrong": ("signal.spectral.fft", "Parseval, recomputed by the runner"),
}

# mutant -> substring that must appear among the names of the failed checks
EXPECT = {
    "transposed_A": "X",
    "bad_request_fingerprint": "fingerprint.request",
    "ignores_method_policy": "hard constraint not honoured",
    "undeclared_warning": "declared in the manifest",
    "wrong_error_code": "expected error",
    "overclaims": "claim equals",
    "nondeterministic": "repeat run",
    "wrong_default": "gives the same result",
}


def run(command, env=None, operators="control.math.lyap"):
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as t:
        rp = t.name
    p = subprocess.run([sys.executable, "-I", RUNNER, "run", "--impl", command, "--operators", operators, "--report", rp],
                       cwd=ROOT, capture_output=True, text=True, env={**os.environ, **(env or {})})
    rep = json.load(open(rp)); os.unlink(rp)
    return p.returncode, rep


def failed_text(rep):
    return " | ".join(f["check"] + " " + f["detail"] for c in rep["cases"] for f in c.get("failed_checks", [])) + " | " + " | ".join(c.get("detail", "") for c in rep["cases"] if c["result"] == "fail")


def main():
    bad = 0
    rc, rep = run(f"{sys.executable} -I {SERVER}")
    ok = rc == 0 and not any(c["result"] == "fail" for c in rep["cases"])
    print(("ok   " if ok else "FAIL ") + "unmodified implementation passes"); bad += not ok
    for name, needle in EXPECT.items():
        rc, rep = run(f"{sys.executable} -I {MUTANT}", {"AEOI_MUTANT": name})
        nfail = sum(1 for c in rep["cases"] if c["result"] == "fail")
        ok = rc == 1 and nfail > 0 and needle in failed_text(rep)
        print(("ok   " if ok else "FAIL ") + f"mutant {name:26s} -> exit {rc}, {nfail} failing case(s), reason contains {needle!r}")
        bad += not ok
    if not os.path.exists(REF):
        print(f"skip proxy mutants: {REF} not built (cargo build --release -p aeoi-mcp)")
    else:
        for name, (op, needle) in PROXY_EXPECT.items():
            rc, rep = run(f"{sys.executable} -I {PROXY}", {"AEOI_MUTANT": name}, operators=op)
            nfail = sum(1 for c in rep["cases"] if c["result"] == "fail")
            if needle is None:
                ok = rc == 0 and nfail == 0
                what = "equivalent answer passes"
            else:
                ok = rc == 1 and nfail > 0 and needle in failed_text(rep)
                what = f"reason contains {needle!r}"
            print(("ok   " if ok else "FAIL ") + f"proxy {name:23s} -> exit {rc}, {nfail} failing case(s), {what}")
            bad += not ok
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
