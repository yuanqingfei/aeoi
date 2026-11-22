#!/usr/bin/env python3
"""Regenerate examples/<op>/response*.json from the Rust implementation (v0.2).
Replaces build_examples.py (v0.1, retired). Values are Rust aeoi-control outputs; parity vs
python-control is checked by `cargo test` (tests/examples.rs, tests/parity.rs). NOT MATLAB output."""
import json, subprocess, pathlib, glob
root = pathlib.Path(__file__).resolve().parent
bin_ = root / "rust/target/release/aeoi-call"
subprocess.check_call(["cargo", "build", "--release", "-q", "--bin", "aeoi-call"], cwd=root / "rust")

# extra example: non-symmetric state-space bode (memory-order regression; same as golden memory_order case)
g = [p for p in glob.glob(str(root / "golden/python-control/control.lti.bode/0.2.0/*.json"))
     if json.load(open(p))["case_family"] == "memory_order_nonsymmetric"][0]
req = json.load(open(g))["request"]; req["exec"] = {"request_id": "req-bode-003-nonsym"}
(root / "examples/control.lti.bode/request.ss_nonsymmetric.json").write_text(json.dumps(req, indent=2, ensure_ascii=False) + "\n")

pairs = [("request.json", "response.success.json"), ("request.error.json", "response.error.json"),
         ("request.ss_nonsymmetric.json", "response.ss_nonsymmetric.json")]
for op in ["control.lti.bode", "control.lti.step", "control.design.lqr"]:
    for rq, rs in pairs:
        p = root / "examples" / op / rq
        if not p.exists():
            continue
        out = subprocess.check_output([str(bin_), op, str(p)])
        env = json.loads(out)
        (root / "examples" / op / rs).write_text(json.dumps(env, indent=2, ensure_ascii=False) + "\n")
        print(op, rq, "->", env["status"], env.get("error") and env["error"]["code"])
