#!/usr/bin/env python3
"""Regenerate THIRD_PARTY_NOTICES.md from `cargo metadata` (run from the repo root).
Lists the crates linked into the aeoi binaries (normal dependencies, all targets) with their declared SPDX
license expressions, plus the bundled non-Rust assets."""
import json, subprocess, collections, os
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
m = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1"], cwd=os.path.join(root, "rust"),
                              capture_output=True, text=True, check=True).stdout)
ws = set(m["workspace_members"]); pk = {p["id"]: p for p in m["packages"]}
res = {n["id"]: n for n in m["resolve"]["nodes"]}
def reach(roots):
    seen, st = set(), list(roots)
    while st:
        x = st.pop()
        if x in seen: continue
        seen.add(x)
        st += [d["pkg"] for d in res[x]["deps"] if any(k["kind"] is None for k in d["dep_kinds"])]
    return seen - ws
wasm = reach([w for w in ws if pk[w]["name"] == "aeoi-wasm"])
allc = reach(ws)
rows = sorted(allc, key=lambda i: (pk[i]["name"], pk[i]["version"]))
out = ["# Third-party notices", "",
"AEOI's own code is `MIT OR Apache-2.0` (LICENSE-MIT, LICENSE-APACHE); the specification and documentation are",
"CC-BY-4.0 (LICENSE-CC-BY-4.0). This file lists third-party material that is bundled in this repository or linked",
"into the binaries we build. Regenerate the crate table with `python3 tools/gen_third_party.py`.", "",
"## Bundled assets", "",
"| Asset | Where | License | Notice |", "|---|---|---|---|",
"| Plotly.js basic 2.35.2 (© 2021 Plotly, Inc) | `rust/aeoi-web/static/vendor/plotly-basic-2.35.2.min.js`, copied to the Pages site `vendor/` | MIT | `rust/aeoi-web/static/vendor/plotly-LICENSE.txt` (shipped next to the file on the site) |",
"", "## Reference data and dev-only tools (not linked into any aeoi binary)", "",
"| Item | Use | License |", "|---|---|---|",
"| python-control 0.10.2, scipy, numpy | generate `golden/python-control/` reference values (`gen_golden_pycontrol*.py`); values only, no code copied | BSD-3-Clause |",
"| mpmath | 50-digit reference values in `golden/mpmath/` (`gen_golden_mpmath.py`) | BSD-3-Clause |",
"| tpt-control (rev 10e4320, © TPT Solutions) | `tools/xcheck-tpt/` cross-check harness, referenced by git rev; no source copied | MIT OR Apache-2.0 |",
"",
"Ideas borrowed, no code copied: result receipts and verify-by-replay (spec 4.3) from bicmath (https://github.com/nikoma/bicmath, MIT); \"no evidence, no success claim\" (spec E-11) from matlab-mcp-extensions (https://github.com/zws20070309/matlab-mcp-extensions).", "",
"", "## Rust crates", "",
f"{len(allc)} crates across the workspace ({len(wasm)} of them in the browser build `aeoi-wasm`). All are permissively licensed; where a crate offers a choice we use it under MIT or Apache-2.0.", ""]
c = collections.Counter(pk[i]["license"] for i in allc)
out += ["| License expression | Crates |", "|---|---|"] + [f"| {k} | {v} |" for k, v in c.most_common()] + [""]
out += ["| Crate | Version | License | In browser build |", "|---|---|---|---|"]
out += [f"| {pk[i]['name']} | {pk[i]['version']} | {pk[i]['license']} | {'yes' if i in wasm else ''} |" for i in rows]
open(os.path.join(root, "THIRD_PARTY_NOTICES.md"), "w").write("\n".join(out) + "\n")
print(len(allc), len(wasm))
