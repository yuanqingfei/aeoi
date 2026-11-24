#!/usr/bin/env python3
"""AEOI second implementation (v0.1): control.math.lyap only, Python + SciPy, wire level (MCP over stdio).

Purpose: exercise the conformance suite (spec 9.4) against something that is not the Rust reference.
It is NOT clean-room independent of python-control's lineage: the numerics are scipy's LAPACK
(Bartels-Stewart), the same family that produced the python-control golden values, so passing those
cases only shows the suite is portable (CF-11 reports this as `same_backend`).

The manifest is the reference manifest with the method, backend and build fields replaced. Everything
else (envelope, receipt, fingerprints) is built here from the spec text.
"""
import copy
import json
import os
import platform
import sys
import time

import numpy as np
from scipy import linalg

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(REPO, "conformance"))
import aeoi_hash as H  # noqa: E402

OP_ID = "control.math.lyap"
TOOL = "control_math_lyap"
METHOD_ID = "pyscipy:lyap:bartels-stewart"
BUILD = f"aeoi-pyscipy 0.1.0 scipy-{__import__('scipy').__version__} numpy-{np.__version__} python-{platform.python_version()}"
U = np.finfo(float).eps / 2


def load_manifest():
    m = copy.deepcopy(json.load(open(os.path.join(REPO, "examples", OP_ID, "manifest.json"))))
    base = os.path.join(REPO, "examples", OP_ID)
    for key, fn in (("input_schema", "input.schema.json"), ("output_schema", "output.schema.json")):
        if isinstance(m.get(key), dict) and "$ref" in m[key]:
            m[key] = json.load(open(os.path.join(base, fn)))
    m["receipt"]["method_id"] = METHOD_ID
    m["backend"] = {"library": "scipy.linalg (LAPACK) via Python", "routine": "solve_continuous_lyapunov / solve_discrete_lyapunov (Bartels-Stewart)",
                    "version": __import__("scipy").__version__, "binding": "native", "license": "BSD-3-Clause"}
    m["determinism"] = "platform_bitwise"
    m["accuracy_grade"] = {"current": "C", "note": "second implementation, no credential yet"}
    m["receipt"]["exactness"] = {"class": "approximate", "bound_kind": "a_posteriori_heuristic"}
    return m


MANIFEST = load_manifest()


def dense(a):
    a = np.asarray(a, dtype=float)
    return {"dtype": "f64", "shape": list(a.shape), "data": [float(x) for x in a.ravel(order="C")]}


def parse_matrix(o, name):
    if not (isinstance(o, dict) and o.get("dtype") == "f64" and isinstance(o.get("shape"), list) and isinstance(o.get("data"), list)):
        raise Fail("E.VALIDATION.SCHEMA", f"{name} must be a dense f64 matrix", f"/{name}")
    shape = o["shape"]
    if len(shape) != 2 or int(np.prod(shape)) != len(o["data"]):
        raise Fail("E.VALIDATION.SHAPE", f"{name}: data length does not match shape", f"/{name}")
    data = [float("nan") if x == "NaN" else x for x in o["data"]]
    a = np.array(data, dtype=float).reshape(shape, order="C")
    if not np.all(np.isfinite(a)):
        raise Fail("E.VALIDATION.NON_FINITE", f"{name} contains non-finite values", f"/{name}")
    return a


class Fail(Exception):
    def __init__(self, code, message, path=None, details=None, category=None):
        self.code, self.message, self.path, self.details = code, message, path, details or {}
        self.category = category or code.split(".")[1].lower()


def solve(args):
    if set(args) - {"A", "Q", "type", "exec"}:
        raise Fail("E.VALIDATION.SCHEMA", f"unknown properties: {sorted(set(args) - {'A', 'Q', 'type', 'exec'})}", "")
    for k in ("A", "Q"):
        if k not in args:
            raise Fail("E.VALIDATION.SCHEMA", f"missing required property {k}", f"/{k}")
    typ = args.get("type", "continuous")        # input schema: `type` is optional, default "continuous"
    if typ not in ("continuous", "discrete"):
        raise Fail("E.VALIDATION.SCHEMA", "type must be continuous or discrete", "/type")
    A, Q = parse_matrix(args["A"], "A"), parse_matrix(args["Q"], "Q")
    n = A.shape[0]
    if A.shape != (n, n) or Q.shape != (n, n):
        raise Fail("E.VALIDATION.SHAPE", "A must be square and Q the same shape", "/Q")
    warnings = []
    # spec 10.10 / lyap algorithm text: Q symmetric -> the solution is symmetrized; Q not symmetric is still solved as given.
    q_sym = np.linalg.norm(Q - Q.T, "fro") <= 1e-12 * max(np.linalg.norm(Q, "fro"), np.finfo(float).tiny)
    lam = np.linalg.eigvals(A)
    if typ == "continuous":
        sep = float(np.min(np.abs(lam[:, None] + np.conj(lam)[None, :])))
        stable = bool(np.all(lam.real < 0))
    else:
        sep = float(np.min(np.abs(lam[:, None] * np.conj(lam)[None, :] - 1.0)))
        stable = bool(np.all(np.abs(lam) < 1))
    if sep <= 1e3 * U * max(1.0, np.linalg.norm(A, 1)) ** (1 if typ == "continuous" else 2):
        raise Fail("E.DOMAIN.LYAP_SINGULAR", f"{typ} Lyapunov equation is singular (min separation {sep:.2e})", "/A",
                   {"separation_min": sep})
    try:
        X = linalg.solve_continuous_lyapunov(A, -Q) if typ == "continuous" else linalg.solve_discrete_lyapunov(A, Q)
    except (np.linalg.LinAlgError, ValueError) as e:
        raise Fail("E.NUMERICAL.NO_CONVERGENCE", f"scipy failed: {e}", "/A", category="numerical")
    X = np.asarray(X, dtype=float)
    x_asym_before = float(np.linalg.norm(X - X.T) / max(np.linalg.norm(X), np.finfo(float).tiny))
    if q_sym:
        X = (X + X.T) / 2
    if not np.all(np.isfinite(X)):
        raise Fail("E.NUMERICAL.NO_CONVERGENCE", "non-finite solution", "/A", category="numerical")
    res = (A @ X + X @ A.T + Q) if typ == "continuous" else (A @ X @ A.T - X + Q)
    denom = (2 * np.linalg.norm(A) * np.linalg.norm(X) + np.linalg.norm(Q)) if typ == "continuous" else (2 * np.linalg.norm(A) ** 2 * np.linalg.norm(X) + np.linalg.norm(X) + np.linalg.norm(Q))
    res_abs = float(np.linalg.norm(res)); res_rel = res_abs / denom if denom else 0.0
    if not stable:
        warnings.append({"code": "W.DOMAIN.UNSTABLE_SYSTEM", "message": "A is not stable; X exists but is not a Gramian (X not PSD)", "path": "/A"})
    xs = bool(np.linalg.norm(X - X.T) <= 1e-10 * max(1.0, np.linalg.norm(X)))
    try:
        psd = bool(np.all(np.linalg.eigvalsh((X + X.T) / 2) >= -1e-10 * max(1.0, np.linalg.norm(X))))
    except np.linalg.LinAlgError:
        psd = False
    err_abs = res_abs / sep if sep > 0 else float("inf")
    trust = {
        "convergence": {"applicable": False},
        "residual": {"abs_fro": res_abs, "rel": res_rel, "X_asymmetry_before_symmetrization_rel": x_asym_before if q_sym else None, "definition": "rel = ||residual||_F / (2||A||_F ||X||_F + ||Q||_F) (continuous)"},
        "condition": {"separation_min": sep, "definition": "min_{i,j}|lambda_i + conj(lambda_j)| (continuous) / |lambda_i conj(lambda_j) - 1| (discrete)"},
        "error_estimate": {"X_abs_fro": err_abs, "X_rel": err_abs / max(np.linalg.norm(X), np.finfo(float).tiny),
                           "basis": "heuristic: residual / sep_min; not a rigorous bound"},
        "checks": {"A_stable": stable, "X_symmetric": xs, "X_psd": psd},
        "accuracy_grade": "C", "credentials": None,
    }
    return {"X": dense(X)}, trust, warnings


def error_envelope(f, request_id):
    return {"spec": "aeoi/0.4", "status": "error", "dry_run": False, "operator": {"id": OP_ID, "version": MANIFEST["version"], "build": BUILD},
            "request_id": request_id, "result": None, "trust": None, "receipt": None, "reproducibility": None,
            "error": {"code": f.code, "category": f.category, "retryable": f.category in ("validation", "numerical", "resource"),
                      "message": f.message, "agent_hint": "check the inputs (matrices are row-major) and the equation type",
                      "details": ({"path": f.path} if f.path is not None else {}) | f.details,
                      "remediation": [{"id": "ask", "kind": "ask_user", "description": "ask the user to confirm A, Q (row-major) and the equation type",
                                       "confidence": "high", "requires_user_confirmation": True}]},
            "warnings": [], "timing": {"wall_ms": 0.0}}


def call(args):
    t0 = time.perf_counter()
    exec_ = args.get("exec") if isinstance(args, dict) else None
    request_id = exec_.get("request_id") if isinstance(exec_, dict) else None
    try:
        if not isinstance(args, dict):
            raise Fail("E.VALIDATION.SCHEMA", "arguments must be an object", "")
        if isinstance(exec_, dict) and "method_policy" in exec_:
            raise Fail("E.VALIDATION.SCHEMA", "exec.method_policy is not supported by this implementation", "/exec/method_policy")
        result, trust, warnings = solve(args)
    except Fail as f:
        return error_envelope(f, request_id)
    in_hash = H.input_hash(args) if not H.has_unsupported_arrays(args) else None
    out_hash = H.output_hash(result)
    version = MANIFEST["version"]
    req_fp = H.request_fingerprint(OP_ID, version, in_hash) if in_hash else None
    res_fp = H.result_fingerprint(req_fp, out_hash) if req_fp else None
    r = MANIFEST["receipt"]
    claim = "unchecked" if not r["postconditions"] else ("verified" if all(trust["checks"].get(p) for p in r["postconditions"]) else "unverified")
    plat = f"{platform.machine()}-{sys.platform}"
    receipt = {
        "format": "aeoi-receipt/1",
        "method": {"id": METHOD_ID, "routine": MANIFEST["backend"]["routine"], "provenance": f"aeoi://operators/{OP_ID}/{version}/manifest#/provenance"},
        "assumptions": r["assumptions"],
        "exactness": {**r["exactness"], "bound": {"X_abs_fro": trust["error_estimate"]["X_abs_fro"], "X_rel": trust["error_estimate"]["X_rel"]},
                      "basis": trust["error_estimate"]["basis"]},
        "evidence": {"claim": claim, "postconditions": {}, "failed": [], "convergence": None, "accuracy_grade": "C",
                     "tolerance_class": MANIFEST["tolerance_class"], "credentials": None},
        "fingerprint": {"request": req_fp, "result": res_fp, "output_hash": out_hash,
                        "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
                        "stable_across": "request: all implementations; result: same build + platform only"},
        "replay": {"build": BUILD, "platform": plat, "verify": {}},
    }
    return {"spec": "aeoi/0.4", "status": "ok_with_warnings" if warnings else "ok", "dry_run": False,
            "operator": {"id": OP_ID, "version": version, "build": BUILD}, "request_id": request_id, "result": result,
            "trust": trust, "warnings": warnings, "error": None,
            "reproducibility": {"determinism": "platform_bitwise", "tolerance_class": MANIFEST["tolerance_class"], "input_hash": in_hash,
                                "output_hash": out_hash, "platform": plat, "cpu_features": [], "blas": "scipy LAPACK (single process)",
                                "threads": 1, "seed": None},
            "receipt": receipt, "timing": {"wall_ms": (time.perf_counter() - t0) * 1000}}


def tools_list():
    return {"tools": [{"name": TOOL, "title": MANIFEST["title"], "description": MANIFEST["description"],
                       "inputSchema": {"type": "object"}, "annotations": MANIFEST["mcp"]["annotations"],
                       "_meta": {"aeoi/id": OP_ID, "aeoi/version": MANIFEST["version"], "aeoi/spec": "aeoi/0.4",
                                 "aeoi/manifest": f"aeoi://operators/{OP_ID}/{MANIFEST['version']}/manifest"}}]}


def handle(msg):
    method, mid, p = msg.get("method"), msg.get("id"), msg.get("params") or {}
    if mid is None:
        return None
    def ok(r): return {"jsonrpc": "2.0", "id": mid, "result": r}
    def err(c, m): return {"jsonrpc": "2.0", "id": mid, "error": {"code": c, "message": m}}
    if method == "initialize":
        return ok({"protocolVersion": p.get("protocolVersion", "2025-06-18"), "capabilities": {"tools": {}, "resources": {}},
                   "serverInfo": {"name": "aeoi-pyscipy", "version": "0.1.0"}})
    if method == "ping":
        return ok({})
    if method == "tools/list":
        return ok(tools_list())
    if method == "tools/call":
        if p.get("name") != TOOL:
            return err(-32602, f"Unknown tool: {p.get('name')}")
        env = call(p.get("arguments") or {})
        return ok({"content": [{"type": "text", "text": f"AEOI {OP_ID} -> {env['status']}"}, {"type": "text", "text": json.dumps(env)}],
                   "structuredContent": env, "isError": env["status"] == "error"})
    if method == "resources/read":
        uri = p.get("uri", "")
        if uri == f"aeoi://operators/{OP_ID}/{MANIFEST['version']}/manifest":
            return ok({"contents": [{"uri": uri, "mimeType": "application/json", "text": json.dumps(MANIFEST)}]})
        return err(-32002, f"Resource not found: {uri}")
    return err(-32601, f"Method not found: {method}")


def main():
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            resp = handle(json.loads(line))
        except Exception as e:  # last resort: never crash the stdio loop
            resp = {"jsonrpc": "2.0", "id": None, "error": {"code": -32603, "message": f"internal error: {e}"}}
        if resp is not None:
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
