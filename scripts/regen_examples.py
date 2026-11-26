#!/usr/bin/env python3
"""Regenerate examples/<op>/response*.json from the Rust implementation (v0.3: all 12 operators).
Replaces build_examples.py (v0.1, retired). Values are Rust aeoi-control outputs; parity vs
python-control is checked by `cargo test` (tests/examples.rs, tests/parity.rs). NOT MATLAB output."""
import json, subprocess, pathlib, glob
root = pathlib.Path(__file__).resolve().parents[1]
bin_ = root / "rust/target/release/aeoi-call"
subprocess.check_call(["cargo", "build", "--release", "-q", "--bin", "aeoi-call"], cwd=root / "rust")

# extra example: non-symmetric state-space bode (memory-order regression; same as golden memory_order case)
g = [p for p in glob.glob(str(root / "golden/python-control/control.lti.bode/0.2.0/*.json"))
     if json.load(open(p))["case_family"] == "memory_order_nonsymmetric"][0]
req = json.load(open(g))["request"]; req["exec"] = {"request_id": "req-bode-003-nonsym"}
(root / "examples/control.lti.bode/request.ss_nonsymmetric.json").write_text(json.dumps(req, indent=2, ensure_ascii=False) + "\n")

def resp_name(rq):
    if rq == "request.json":
        return "response.success.json"
    if rq == "request.error.json":
        return "response.error.json"
    return "response." + rq[len("request."):]

for opdir in sorted((root / "examples").iterdir()):
    op = opdir.name
    for p in sorted(opdir.glob("request*.json")):
        out = subprocess.check_output([str(bin_), op, str(p)])
        env = json.loads(out)
        dst = opdir / resp_name(p.name)
        if dst.exists():  # keep the published timing when the result did not change (avoids wall_ms churn)
            prev = json.loads(dst.read_text())
            if prev.get("status") == env.get("status") and (prev.get("reproducibility") or {}).get("output_hash") == (env.get("reproducibility") or {}).get("output_hash"):
                env["timing"] = prev.get("timing", env.get("timing"))
        dst.write_text(json.dumps(env, indent=2, ensure_ascii=False) + "\n")
        print(op, p.name, "->", env["status"], env.get("error") and env["error"]["code"])
