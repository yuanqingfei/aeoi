#!/usr/bin/env python3
"""Compare golden_eval output (JSON lines) with the high-precision golden store (golden/mpmath/...).
usage: python tools/hp_compare.py after.jsonl [before.jsonl] [--json out.json]
Error metric: max|x − x_ref| / max|x_ref| (elementwise, as in reports/parity-tpt-control.*)."""
import json, sys
import numpy as np

def load(p):
    return {json.loads(l)["file"]: json.loads(l) for l in open(p)}

def mat(d):
    return np.array(d["data"], float).reshape(d["shape"])

def err(rec, gold):
    if rec["status"] not in ("ok", "ok_with_warnings"):
        return {"status": rec["error_code"]}
    r, e = rec["result"], gold["expected"]
    out = {}
    for k in ("S", "K", "X"):
        if k in e:
            x, ref = mat(r[k]["value"] if "value" in r[k] else r[k]), mat(e[k])
            out[k] = float(np.max(np.abs(x - ref)) / max(np.max(np.abs(ref)), 1e-300))
    est = (rec["trust"] or {}).get("error_estimate") or {}
    out["self_estimate_rel"] = est.get("S_rel", est.get("X_rel"))
    return out

args = [a for a in sys.argv[1:] if not a.startswith("--")]
jout = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
if jout: args = [a for a in args if a != jout]
after = load(args[0]); before = load(args[1]) if len(args) > 1 else {}
rows = []
for f, rec in after.items():
    gold = json.load(open(f))
    row = {"file": f, "operator": rec["operator"], "family": rec["family"], "after": err(rec, gold),
           "scipy": gold["expected"].get("scipy_forward_error_rel_max"), "cond_R": gold["expected"].get("cond_R")}
    if f in before: row["before"] = err(before[f], gold)
    rows.append(row)
fam = {}
for r in rows:
    key = (r["operator"], r["family"])
    k = "S" if r["operator"].endswith("lqr") else "X"
    a = fam.setdefault(key, {"n": 0, "after": 0.0, "before": 0.0, "before_errors": 0, "after_errors": 0, "scipy": 0.0})
    a["n"] += 1
    for side in ("after", "before"):
        if side in r:
            v = r[side].get(k)
            if v is None: a[side + "_errors"] += 1
            else: a[side] = max(a[side], v)
    if r["scipy"] is not None: a["scipy"] = max(a["scipy"], r["scipy"])
print("| operator | family | n | before max err | after max err | scipy max err |")
print("|---|---|---|---|---|---|")
for (op, f), a in fam.items():
    b = f"{a['before']:.1e}" + (f" ({a['before_errors']} errors)" if a["before_errors"] else "")
    aa = f"{a['after']:.1e}" + (f" ({a['after_errors']} errors)" if a["after_errors"] else "")
    print(f"| {op} | {f} | {a['n']} | {b} | {aa} | {a['scipy']:.1e} |")
if jout:
    json.dump({"rows": rows, "families": {f"{k[0]} / {k[1]}": v for k, v in fam.items()}}, open(jout, "w"), indent=1)
