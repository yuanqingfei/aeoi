"""The Python second implementation with one deliberate defect, chosen by $AEOI_MUTANT.
Used only by test_runner_catches_mutants.py to show that the runner fails when it should.
The defects are injected into control.math.lyap (the operator the mutant run exercises) and the shared
plumbing in aeoi_common.py."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "impls", "python_scipy"))
import scipy.linalg  # noqa: E402

import server as S  # noqa: E402
import aeoi_common as C  # noqa: E402
import op_lyap  # noqa: E402

m = os.environ.get("AEOI_MUTANT", "")
Lyap = op_lyap.Lyap
orig_call, orig_execute, orig_prepare = C.call_operator, Lyap.execute, Lyap.prepare

if m == "transposed_A":                       # wrong equation convention: solves with A^T
    for name in ("solve_continuous_lyapunov", "solve_discrete_lyapunov"):
        f = getattr(scipy.linalg, name)
        setattr(scipy.linalg, name, (lambda f: lambda A, Q: f(A.T, Q))(f))
elif m == "bad_request_fingerprint":          # request fingerprint differs from the spec's definition
    C.H.request_fingerprint = lambda op, ver, ih: "sha256:" + "0" * 64
elif m == "ignores_method_policy":            # silently drops exec.method_policy instead of rejecting it
    def call(op, args):
        if isinstance(args, dict) and isinstance(args.get("exec"), dict):
            args = {**args, "exec": {k: v for k, v in args["exec"].items() if k != "method_policy"}}
        return orig_call(op, args)
    C.call_operator = call
elif m == "undeclared_warning":               # emits a warning code its manifest does not declare
    # (the implementation refuses to emit undeclared codes itself, so the defect is added to the envelope)
    def call(op, args):
        e = orig_call(op, args)
        if e.get("status") in ("ok", "ok_with_warnings"):
            e["warnings"] = e.get("warnings", []) + [{"code": "W.MADE.UP", "message": "x", "path": ""}]
            e["status"] = "ok_with_warnings"
        return e
    C.call_operator = call
elif m == "wrong_error_code":                 # reports a different error code than the golden case expects
    orig_env = C._error_envelope
    def error_envelope(op, f, rid, t0):
        if f.code == "E.DOMAIN.LYAP_SINGULAR":
            f.code, f.category = "E.NUMERICAL.NO_CONVERGENCE", "numerical"
        return orig_env(op, f, rid, t0)
    C._error_envelope = error_envelope
elif m == "overclaims":                       # receipt claims `verified` although the manifest lists no postcondition
    def call(op, args):
        e = orig_call(op, args)
        if e.get("receipt"):
            e["receipt"]["evidence"]["claim"] = "verified"
        return e
    C.call_operator = call
elif m == "wrong_default":                    # omitted `type` treated as discrete (the schema default is continuous)
    def prepare(self, args):
        if isinstance(args, dict) and "type" not in args:
            args = {**args, "type": "discrete"}
        return orig_prepare(self, args)
    Lyap.prepare = prepare
elif m == "nondeterministic":                 # same request, different bits on every call
    import random
    def execute(self, P):
        r, t, w = orig_execute(self, P)
        r["X"]["data"][0] *= 1 + random.random() * 1e-15
        return r, t, w
    Lyap.execute = execute

if __name__ == "__main__":
    S.main()
