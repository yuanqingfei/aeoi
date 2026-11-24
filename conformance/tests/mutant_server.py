"""The Python second implementation with one deliberate defect, chosen by $AEOI_MUTANT.
Used only by test_runner_catches_mutants.py to show that the runner fails when it should."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "impls", "python_scipy"))
import server as S  # noqa: E402

m = os.environ.get("AEOI_MUTANT", "")
orig_call, orig_solve = S.call, S.solve

if m == "transposed_A":                       # wrong equation convention: solves with A^T
    for name in ("solve_continuous_lyapunov", "solve_discrete_lyapunov"):
        f = getattr(S.linalg, name)
        setattr(S.linalg, name, (lambda f: lambda A, Q: f(A.T, Q))(f))
elif m == "bad_request_fingerprint":          # request fingerprint differs from the spec's definition
    S.H.request_fingerprint = lambda op, ver, ih: "sha256:" + "0" * 64
elif m == "ignores_method_policy":            # silently drops exec.method_policy instead of rejecting it
    def call(args):
        if isinstance(args, dict) and isinstance(args.get("exec"), dict):
            args = {**args, "exec": {k: v for k, v in args["exec"].items() if k != "method_policy"}}
        return orig_call(args)
    S.call = call
elif m == "undeclared_warning":               # emits a warning code its manifest does not declare
    def solve(args):
        r, t, w = orig_solve(args)
        return r, t, w + [{"code": "W.MADE.UP", "message": "x", "path": ""}]
    S.solve = solve
elif m == "wrong_error_code":                 # reports a different error code than the golden case expects
    orig_env = S.error_envelope
    S.error_envelope = lambda f, rid: orig_env(S.Fail("E.NUMERICAL.NO_CONVERGENCE", f.message, f.path, f.details, "numerical"), rid) if f.code == "E.DOMAIN.LYAP_SINGULAR" else orig_env(f, rid)
elif m == "overclaims":                       # receipt claims `verified` although the manifest lists no postcondition
    def call(args):
        e = orig_call(args)
        if e["receipt"]:
            e["receipt"]["evidence"]["claim"] = "verified"
        return e
    S.call = call
elif m == "nondeterministic":                 # same request, different bits on every call
    import random
    def solve(args):
        r, t, w = orig_solve(args)
        r["X"]["data"][0] *= 1 + random.random() * 1e-15
        return r, t, w
    S.solve = solve

if __name__ == "__main__":
    S.main()
