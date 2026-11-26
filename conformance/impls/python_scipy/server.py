#!/usr/bin/env python3
"""AEOI second implementation (v0.2): Python + numpy/scipy, wire level (MCP over stdio, spec 9.4 CF-1).

Operators
  control.math.lyap              (10.10)  scipy Bartels-Stewart; manifest derived from the reference manifest
  signal.spectral.fft            (10.17)  own mixed-radix DIT FFT + Bluestein, not numpy.fft
  linalg.system.solve            (10.19)  own LU / Householder QR with exact-dot residuals and refinement
  interp.grid.interp1            (10.20)  own linear/nearest/previous/next/pchip; spline slopes via LAPACK dgtsv
  calculus.quadrature.integral   (10.31)  own expression language (3.15) and adaptive Gauss-Kronrod (7, 15)

What is independent and what is not
  * fft, linsolve, interp1, integral were written only from spec-v0.4.md, profiles/, examples/<op>/,
    schemas/ and golden requests; the Rust reference (rust/), the golden generators and the conformance
    rule files were not read. Their manifests are written here (own method ids, backend, algorithm text,
    accuracy claims, codes actually emitted); I/O schemas are the profiles' linked schemas, inlined.
  * Same lineage with the golden values: interp1 (spline uses LAPACK dgtsv; pchip/spline follow the
    same published formulas as scipy's PchipInterpolator / CubicSpline that generated the golden values),
    and lyap (scipy LAPACK, same family as python-control). The FFT does not use pocketfft (numpy's
    FFT, the golden engine); linsolve does not call LAPACK getrf/geqrf/gelsd (numpy's engine).
    integral is checked against mpmath truth values, not against scipy.quad.
  * Envelope, receipt, fingerprints, errors and method_policy live in aeoi_common.py; JCS and the
    json array digest are reused from conformance/aeoi_hash.py (itself an independent rewrite).
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import aeoi_common as C  # noqa: E402
import op_fft  # noqa: E402
import op_integral  # noqa: E402
import op_interp1  # noqa: E402
import op_linsolve  # noqa: E402
import op_lyap  # noqa: E402

OPERATORS = [op_lyap.Lyap(), op_fft.FFT(), op_linsolve.LinSolve(), op_interp1.Interp1(), op_integral.Integral()]
BY_TOOL = {op.tool_name: op for op in OPERATORS}
PROTOCOLS = ("2025-06-18", "2025-03-26", "2024-11-05")


def _manifest_uri(op):
    return f"aeoi://operators/{op.OP_ID}/{op.VERSION}/manifest"


def resources():
    out = [{"uri": "aeoi://operators", "name": "operator catalog", "mimeType": "application/json"}]
    for op in OPERATORS:
        out.append({"uri": _manifest_uri(op), "name": f"{op.OP_ID} manifest", "mimeType": "application/json"})
    for sid in C.CORE:
        out.append({"uri": sid, "name": sid, "mimeType": "application/schema+json"})
    return out


def read_resource(uri):
    if uri == "aeoi://operators":
        return {"spec": C.SPEC, "implementation": {"name": C.IMPL_NAME, "version": C.IMPL_VERSION, "form": "wire"},
                "operators": [{"id": op.OP_ID, "version": op.VERSION, "tool": op.tool_name, "manifest": _manifest_uri(op)}
                              for op in OPERATORS]}
    for op in OPERATORS:
        if uri == _manifest_uri(op):
            return op.MANIFEST
    if uri in C.CORE:
        return C.CORE[uri]
    return None


def handle(msg):
    method, mid, p = msg.get("method"), msg.get("id"), msg.get("params") or {}
    if mid is None:
        return None     # notification

    def ok(r):
        return {"jsonrpc": "2.0", "id": mid, "result": r}

    def err(c, m):
        return {"jsonrpc": "2.0", "id": mid, "error": {"code": c, "message": m}}

    if method == "initialize":
        pv = p.get("protocolVersion")
        return ok({"protocolVersion": pv if pv in PROTOCOLS else PROTOCOLS[0],
                   "capabilities": {"tools": {}, "resources": {}},
                   "serverInfo": {"name": C.IMPL_NAME, "version": C.IMPL_VERSION}})
    if method == "ping":
        return ok({})
    if method == "tools/list":
        return ok({"tools": [C.mcp_tool_entry(op) for op in OPERATORS]})
    if method == "tools/call":
        op = BY_TOOL.get(p.get("name"))
        if op is None:
            return err(-32602, f"Unknown tool: {p.get('name')}")
        args = p.get("arguments")
        env = C.call_operator(op, {} if args is None else args)
        return ok({"content": C.summary_content(env), "structuredContent": env, "isError": env["status"] == "error"})
    if method == "resources/list":
        return ok({"resources": resources()})
    if method == "resources/read":
        uri = p.get("uri", "")
        doc = read_resource(uri)
        if doc is None:
            return err(-32002, f"Resource not found: {uri}")
        return ok({"contents": [{"uri": uri, "mimeType": "application/json", "text": json.dumps(doc, ensure_ascii=False)}]})
    return err(-32601, f"Method not found: {method}")


def main():
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError as e:
            resp = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": f"parse error: {e}"}}
        else:
            try:
                resp = handle(msg)
            except Exception as e:  # last resort: never crash the stdio loop
                resp = {"jsonrpc": "2.0", "id": msg.get("id") if isinstance(msg, dict) else None,
                        "error": {"code": -32603, "message": f"internal error: {e}"}}
        if resp is not None:
            sys.stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
