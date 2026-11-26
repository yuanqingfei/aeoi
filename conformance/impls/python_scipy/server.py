#!/usr/bin/env python3
"""AEOI second implementation (v0.3): Python + numpy/scipy, wire level (MCP over stdio, spec 9.4 CF-1).

Operators
  control.math.lyap              (10.10)  scipy Bartels-Stewart; manifest derived from the reference manifest
  signal.spectral.fft            (10.17)  own mixed-radix DIT FFT + Bluestein, not numpy.fft
  linalg.system.solve            (10.19)  own LU / Householder QR with exact-dot residuals and refinement
  interp.grid.interp1            (10.20)  own linear/nearest/previous/next/pchip; spline slopes via LAPACK dgtsv
  calculus.quadrature.integral   (10.31)  own expression language (3.15) and adaptive Gauss-Kronrod (7, 15)
  control.lti.bode               (10.1)   own Horner / zpk product / batched complex LU (aeoi_lti.freqresp)
  control.lti.step               (10.2)   own Pade-13 scaling-squaring expm (Van Loan ZOH) + recursion
  control.lti.convert            (10.4)   own canonical realizations, R-7 ss2tf, zero dynamics for ss -> zpk
  fit.poly.roots                 (10.34)  own balanced companion + Francis double-shift QR + Newton, inclusion radii
  linalg.subspace.rank           (10.37)  own one-sided Hestenes Jacobi SVD (aeoi_kernels.jacobi_svd) + rank decision
  linalg.matrix.pinv             (10.39)  same SVD kernel and rank decision as rank
  linalg.decomp.qr               (10.43)  own Householder QR (dlarfg convention) + R_ii >= 0 sign fix
  linalg.decomp.chol             (10.44)  own column Cholesky on the upper triangle, correctly rounded dots
  linalg.matrix.det              (10.36)  LU of op_linsolve + frexp mantissa/exponent product + Hager-Higham estimate

  aeoi_lti.py      shared LtiSystem parsing / validation / conversion / frequency response (all control operators)
  aeoi_kernels.py  shared numerics: balance, Hessenberg, Francis QR (hqr), LU, batched complex LU, expm, polynomials

What is independent and what is not
  * fft, linsolve, interp1, integral (round 1) and bode, step, convert, roots (round 2) were written only from
    spec-v0.4.md, profiles/, examples/<op>/, schemas/ and golden files; the Rust reference (rust/), the golden
    generators and the conformance rule files were not read. Their manifests are written here (own method ids,
    backend, algorithm text, accuracy claims, codes actually emitted); I/O schemas are the profiles' linked
    schemas, inlined.
  * No python-control, no scipy.signal, no numpy.roots / numpy.linalg.eig / LAPACK on the round-2 result paths:
    eigenvalues, LU and expm are aeoi_kernels' own code; step uses numpy matmul (OpenBLAS) for small products.
  * Same lineage with the golden values, admitted:
      - the step-metric details (settling_min / settling_max include y_inf, overshoot / undershoot / peak
        formulas) follow python-control 0.10 step_info as I know it from its public source, because the profile
        defines these only by reference to python-control (SPEC_FINDINGS PY-50);
      - the realizations follow the scipy tf2ss / ss2tf conventions the spec names (3.10 R-6, R-7);
      - interp1 (spline uses LAPACK dgtsv, same published formulas as scipy) and lyap (scipy LAPACK).
  * Round 3 (rank, pinv, qr, chol, det) under the same rules; no numpy.linalg / scipy.linalg / LAPACK on the
    result paths (SVD, QR, Cholesky, LU are own code).
  * Envelope, receipt, fingerprints, errors and method_policy live in aeoi_common.py; JCS and the
    json array digest are reused from conformance/aeoi_hash.py (itself an independent rewrite).
"""
import json
import os
import sys

# P-3: single-threaded BLAS for the small matrix products (must precede the numpy import)
for _v in ("OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "MKL_NUM_THREADS"):
    os.environ.setdefault(_v, "1")

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import aeoi_common as C  # noqa: E402
import op_fft  # noqa: E402
import op_integral  # noqa: E402
import op_interp1  # noqa: E402
import op_linsolve  # noqa: E402
import op_lyap  # noqa: E402
import op_bode  # noqa: E402
import op_step  # noqa: E402
import op_convert  # noqa: E402
import op_roots  # noqa: E402
import op_rank  # noqa: E402
import op_pinv  # noqa: E402
import op_qr  # noqa: E402
import op_chol  # noqa: E402
import op_det  # noqa: E402

OPERATORS = [op_lyap.Lyap(), op_fft.FFT(), op_linsolve.LinSolve(), op_interp1.Interp1(), op_integral.Integral(),
             op_bode.Bode(), op_step.Step(), op_convert.Convert(), op_roots.Roots(),
             op_rank.Rank(), op_pinv.Pinv(), op_qr.QR(), op_chol.Chol(), op_det.Det()]
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
