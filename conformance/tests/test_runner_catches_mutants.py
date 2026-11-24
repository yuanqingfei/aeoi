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

# mutant -> substring that must appear among the names of the failed checks
EXPECT = {
    "transposed_A": "X",
    "bad_request_fingerprint": "fingerprint.request",
    "ignores_method_policy": "hard constraint not honoured",
    "undeclared_warning": "declared in the manifest",
    "wrong_error_code": "expected error",
    "overclaims": "claim equals",
    "nondeterministic": "repeat run",
}


def run(command, env=None):
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as t:
        rp = t.name
    p = subprocess.run([sys.executable, "-I", RUNNER, "run", "--impl", command, "--operators", "control.math.lyap", "--report", rp],
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
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
