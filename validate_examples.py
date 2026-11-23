#!/usr/bin/env python3
"""Independent (python-jsonschema) validation of all v0.3 examples (12 operators) against the schemas —
cross-checks the Rust validator. Exit code 1 on any failure."""
import json, glob, sys
from jsonschema import Draft202012Validator
from referencing import Registry, Resource
L = lambda p: json.load(open(p))
docs = {n: L(f"schemas/{n}.schema.json") for n in ["common", "envelope", "error", "manifest"]}
reg = Registry()
for s in docs.values():
    reg = reg.with_resource(s["$id"], Resource.from_contents(s))
for d in docs["common"]["$defs"]:  # aeoi://schemas/common/0.3/<Def> shorthand used by operator schemas
    reg = reg.with_resource(f"aeoi://schemas/common/0.3/{d}", Resource.from_contents(
        {"$schema": "https://json-schema.org/draft/2020-12/schema", "$ref": f"aeoi://schemas/common/0.3#/$defs/{d}"}))
bad = 0
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
        check(ins, L(rq), f"{op} {rq.split('/')[-1]}")
    for rs in sorted(glob.glob(d + "response*.json")):
        e = L(rs)
        check(docs["envelope"], e, f"{op} {rs.split('/')[-1]} envelope")
        if e["result"] is not None:
            check(outs, e["result"], f"{op} {rs.split('/')[-1]} result vs output schema")
    # every emitted error/warning code must be declared in the manifest
    m = L(d + "manifest.json")
    for rs in sorted(glob.glob(d + "response*.json")):
        e = L(rs)
        codes = ([e["error"]["code"]] if e.get("error") else []) + [w["code"] for w in e.get("warnings", [])]
        undeclared = [c for c in codes if c not in m["errors"] + m["warnings"]]
        bad += bool(undeclared)
        print(("OK  " if not undeclared else "FAIL"), op, rs.split("/")[-1], "codes declared", undeclared or "")
# golden requests: all batch-2 (0.3.0) requests incl. column-major mistake variants; a sample of batch-1 (0.2.0);
# all high-precision (golden/mpmath) requests
gold = sorted(glob.glob("golden/python-control/*/0.3.0/*.json")) + sorted(glob.glob("golden/python-control/*/0.2.0/*.json"))[::25] + sorted(glob.glob("golden/mpmath/*/*/*.json"))
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
