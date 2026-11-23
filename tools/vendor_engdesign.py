#!/usr/bin/env python3
"""Vendor the control / signal-processing design tasks we can score from EngDesign (MIT).

Usage: tools/vendor_engdesign.py /path/to/EngDesign-clone
Writes eval/design/engdesign.json. Prompts, rubrics and reference solutions are copied verbatim
(MIT, Copyright (c) 2025 EngDesign Benchmark Team, see eval/design/LICENSE-EngDesign); the scoring
is our own re-implementation (rust/aeoi-web/src/design.rs) on AEOI operators, documented per task
in `scoring_notes` where it deviates from the upstream evaluate.py / *.m.
"""
import json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = Path(sys.argv[1] if len(sys.argv) > 1 else "/workspace/tmp/EngDesign")

# id → (scorer, blockers we expect from missing operators, scoring notes)
TASKS = {
    "XG_13": ("pid_unity", ["feedback/series (tf arithmetic)"],
              "Pass = the prompt's specs (stable, 2% settling < 0.2 s, overshoot < 5 %). Upstream evaluate.py "
              "uses Ts < 2 s and Overshoot < 0.05 (python-control reports percent, so 0.05 %); that literal "
              "verdict is reported as upstream_literal_pass."),
    "Ziheng_03": ("pid_unity", ["feedback/series (tf arithmetic)", "pid constructor"],
                  "5 % settling band, overshoot < 20 %, |e_ss| < 1e-3, GM > 10 dB, PM > 45° as in the prompt. "
                  "Upstream evaluate_pid.m compares MATLAB margin()'s absolute gain margin with 10 (= 20 dB); "
                  "that literal verdict is upstream_literal_pass."),
    "XG_05": ("fopdt_controller", ["time delay / pade", "feedback/series (tf arithmetic)"],
              "Delay approximated by a 10th-order Padé (as upstream). theta, tau within ±2 of 16.5, 19.5."),
    "XG_11": ("loopshape_rolloff", ["feedback/series (tf arithmetic)"],
              "C(s) coefficients within 0.1 (2-norm) of the expected Kg·Ki; loop with the roll-off stable, "
              "PM ≥ 50°, GM ≥ 3 dB. Upstream takes max() over all crossings; we report both the minimum "
              "(classical) margins and the upstream-style maxima and pass on the upstream-style ones."),
    "XG_12": ("loopshape_notch", ["feedback/series (tf arithmetic)"],
              "As XG_11 with the notch; PM ≥ 60°, GM ≥ 2 dB."),
    "XG_02": ("loopshape_bandwidth", ["ss→tf series with a controller (tf arithmetic)", "bandwidth"],
              "Stable closed loop, first 0 dB crossing of L in [75, 95] Hz, GM > 1.5 dB, PM > 60°. Upstream "
              "compares MATLAB margin()'s absolute gain margin with 1.5; we use dB as the prompt says."),
    "LX_02": ("maglev_observer", ["nonlinear simulation (region of attraction)"],
              "A, B, K, L compared with the reference to 1e-6 (upstream: isequal); s1..s4 against the "
              "reference answers (1, 0, 1, 0)."),
    "AV_02": ("iir_notch", ["filter design (iirnotch/butter)"],
              "Same checks and weights as upstream, magnitude evaluated with control.lti.bode on the discrete "
              "tf (ts = 1/2000 s) at the upstream grid points."),
    "AV_03": ("fir_antialias", ["FIR filter design (firwin/remez)"],
              "Same checks and weights as upstream, magnitude via control.lti.bode on the FIR as a discrete tf."),
    "ZC_02": ("alpha_range", ["SDP / LMI solver (absolute stability)"],
              "Pass if 0.651 ≤ alpha ≤ 1.087, the range upstream verifyalpha.m accepts."),
}

FIELD = re.compile(r'^\s*(\w+)\s*:\s*([^=]+?)\s*=\s*Field\((?:\.\.\.\s*,\s*)?description\s*=\s*"([^"]*)"')


def fields(p: Path):
    out = []
    for line in p.read_text().splitlines():
        m = FIELD.match(line)
        if m and m.group(1) != "reasoning":
            t = m.group(2).strip()
            kind = "array" if t.lower().startswith("list") else "integer" if t == "int" else "number"
            out.append({"name": m.group(1), "type": kind, "python_type": t, "description": m.group(3)})
    return out


def main():
    commit = subprocess.check_output(["git", "-C", str(SRC), "rev-parse", "HEAD"], text=True).strip()
    tasks = []
    for tid, (scorer, blockers, notes) in TASKS.items():
        d = SRC / "tasks" / tid
        rd = lambda n: (d / n).read_text(encoding="utf-8").strip() if (d / n).exists() else None
        tasks.append({
            "id": f"engdesign/{tid}",
            "source": {"benchmark": "EngDesign", "task": tid, "path": f"tasks/{tid}",
                       "url": f"https://github.com/AGI4Engineering/EngDesign/tree/{commit}/tasks/{tid}",
                       "license": "MIT", "copyright": "Copyright (c) 2025 EngDesign Benchmark Team"},
            "domain": rd("domain_topic.txt"),
            "prompt": rd("LLM_prompt.txt"),
            "fields": fields(d / "output_structure.py"),
            "rubric": rd("rubrics.txt"),
            "reference_solution": rd("solution.txt"),
            "scorer": scorer,
            "expected_blockers": blockers,
            "scoring_notes": notes,
        })
    doc = {"$comment": "Vendored from EngDesign (MIT) by tools/vendor_engdesign.py; see eval/design/README.md.",
           "upstream": {"repo": "https://github.com/AGI4Engineering/EngDesign", "commit": commit,
                        "license": "MIT", "license_file": "eval/design/LICENSE-EngDesign"},
           "tasks": tasks}
    (ROOT / "eval/design/engdesign.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(tasks)} tasks @ {commit[:12]}")
    for t in tasks:
        print(t["id"], [f["name"] for f in t["fields"]])


if __name__ == "__main__":
    main()
