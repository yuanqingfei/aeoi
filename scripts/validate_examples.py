#!/usr/bin/env python3
"""Independent (python-jsonschema) validation of all v0.3 examples (12 operators) against the schemas —
cross-checks the Rust validator. Exit code 1 on any failure."""
import os, json, glob, sys, hashlib
from jsonschema import Draft202012Validator
from referencing import Registry, Resource
L = lambda p: json.load(open(p))
docs = {n: L(f"schemas/{n}.schema.json") for n in ["common", "envelope", "error", "manifest"]}
reg = Registry()
for s in docs.values():
    reg = reg.with_resource(s["$id"], Resource.from_contents(s))
bad = 0
# spec 3.9 J-1: operator schemas use standard 2020-12 $refs only (no shorthand); every non-local $ref must resolve as written
def _refs(v):
    if isinstance(v, dict):
        if isinstance(v.get("$ref"), str): yield v["$ref"]
        for x in v.values(): yield from _refs(x)
    elif isinstance(v, list):
        for x in v: yield from _refs(x)
for f in sorted(glob.glob("examples/*/*.schema.json")) + [f"schemas/{n}.schema.json" for n in docs]:
    for r in _refs(L(f)):
        if r.startswith("#"): continue
        try: reg.resolver().lookup(r)
        except Exception as ex: print(f"FAIL {f}: $ref {r} does not resolve (3.9 J-1): {type(ex).__name__}"); bad += 1
# framework-level codes (spec E-7a): emitted by the pipeline, not by an operator, so not listed per manifest
FRAMEWORK_CODES = ["W.EVIDENCE.POSTCONDITION_FAILED"]
H = lambda o: "sha256:" + hashlib.sha256(json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
def check_receipt(e, m, label):
    """Independent re-derivation of the receipt fingerprints (spec 4.3 R-3), the E-11/E-12 claim and E-13 unreliable fields."""
    global bad
    r, msgs = e.get("receipt"), []
    if e["status"] == "error" or e.get("dry_run"):
        msgs += [] if r is None else ["error/dry-run envelope must have receipt null"]
    else:
        rp = e["reproducibility"]
        req = H({"operator": m["id"], "version": m["version"], "input_hash": rp["input_hash"]})
        if r["fingerprint"]["request"] != req: msgs.append("request fingerprint")
        if r["fingerprint"]["output_hash"] != rp["output_hash"]: msgs.append("output_hash")
        if r["fingerprint"]["result"] != H({"request": req, "output_hash": rp["output_hash"]}): msgs.append("result fingerprint")
        post = m["receipt"]["postconditions"]; ch = e["trust"].get("checks") or {}
        failed = [k for k in post if ch.get(k) is not True]
        cov = m["receipt"].get("coverage") or {}
        sampled = any(cov.get(k) == "sampled" for k in post)  # E-12 (v0.3.6)
        claim = "unchecked" if not post else ("unverified" if failed else ("spot_checked" if sampled else "verified"))
        if cov and r["evidence"].get("coverage") != {k: cov.get(k, "full") for k in post}: msgs.append("E-12 coverage map")
        guards = m["receipt"].get("guards") or {}
        unrel = sorted({p for k in failed for p in guards.get(k, [])})
        if unrel and sorted(r["evidence"].get("unreliable") or []) != unrel: msgs.append("E-13 unreliable fields")
        if r["evidence"]["claim"] != claim or r["evidence"]["failed"] != failed: msgs.append(f"claim {r['evidence']['claim']} != {claim}")
        if failed and "W.EVIDENCE.POSTCONDITION_FAILED" not in [w["code"] for w in e["warnings"]]: msgs.append("E-11 warning missing")
        if r["assumptions"] != m["receipt"]["assumptions"]: msgs.append("assumptions differ from manifest")
    bad += bool(msgs)
    print(("OK  " if not msgs else "FAIL"), label, "receipt", msgs or "")
def check(schema, inst, label):
    global bad
    errs = list(Draft202012Validator(schema, registry=reg).iter_errors(inst))
    bad += bool(errs)
    print(("OK  " if not errs else "FAIL"), label, *[e.message[:150] + " @" + e.json_path for e in errs[:3]])
for d in sorted(glob.glob("examples/*/")):
    op = d.split("/")[1]
    check(docs["manifest"], L(d + "manifest.json"), op + " manifest")
    ins, outs = L(d + "input.schema.json"), L(d + "output.schema.json")
    for rq in sorted(glob.glob(d + "request*.json")):
        rs_ = rq.replace("request", "response", 1)
        rs_ = d + "response.success.json" if rq.endswith("/request.json") else rs_
        if os.path.exists(rs_) and (L(rs_).get("error") or {}).get("code") == "E.VALIDATION.SCHEMA":
            print("OK  ", op, rq.split("/")[-1], "is the schema-error example (expected invalid)"); continue
        check(ins, L(rq), f"{op} {rq.split('/')[-1]}")
    for rs in sorted(glob.glob(d + "response*.json")):
        e = L(rs)
        check(docs["envelope"], e, f"{op} {rs.split('/')[-1]} envelope")
        if e["result"] is not None:
            check(outs, e["result"], f"{op} {rs.split('/')[-1]} result vs output schema")
        check_receipt(e, L(d + "manifest.json"), f"{op} {rs.split('/')[-1]}")
    # every emitted error/warning code must be declared in the manifest
    m = L(d + "manifest.json")
    for rs in sorted(glob.glob(d + "response*.json")):
        e = L(rs)
        codes = ([e["error"]["code"]] if e.get("error") else []) + [w["code"] for w in e.get("warnings", [])]
        undeclared = [c for c in codes if c not in m["errors"] + m["warnings"] + FRAMEWORK_CODES]
        bad += bool(undeclared)
        print(("OK  " if not undeclared else "FAIL"), op, rs.split("/")[-1], "codes declared", undeclared or "")
# golden requests: all batch-2 (0.3.0) requests incl. column-major mistake variants; a sample of batch-1 (0.2.0);
# all high-precision (golden/mpmath) requests
gold = sorted(glob.glob("golden/python-control/*/0.3.0/*.json")) + sorted(glob.glob("golden/python-control/*/0.4.0/*.json")) + sorted(glob.glob("golden/python-control/*/0.2.0/*.json"))[::25] + sorted(glob.glob("golden/mpmath/*/*/*.json"))
ng = 0
for g in gold:
    op = g.split("/")[2]; case = L(g)
    if case.get("expected_error") == "E.VALIDATION.SCHEMA":
        continue
    ins = L(f"examples/{op}/input.schema.json")
    errs = list(Draft202012Validator(ins, registry=reg).iter_errors(case["request"]))
    if "column_major_mistake_request" in case:
        errs += list(Draft202012Validator(ins, registry=reg).iter_errors(case["column_major_mistake_request"]))
    ng += 1
    if errs:
        bad += 1
        print("FAIL golden", op, g.split("/")[-1][:12], errs[0].message[:150])
print(f"golden requests validated: {ng}")
sys.exit(1 if bad else 0)
