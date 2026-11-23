#!/usr/bin/env python3
"""Fetch the ControlEval task data from upstream at run time (NOT vendored).

ControlEval (https://github.com/EnvCommons/ControlEval) and the data repo it downloads from
(https://github.com/ControlAgent/ControlAgent, directory ControlEval/) declare no license
(GitHub reports none, no LICENSE file), so we may not redistribute the tasks. This script
downloads them into eval/design/upstream/ (gitignored) for local evaluation only, and records the
upstream commit. Our prompts and scoring are our own (rust/aeoi-web/src/design.rs), written from
the criteria ControlEval documents in its README.
"""
import json, sys, urllib.request
from pathlib import Path

REPO = "ControlAgent/ControlAgent"
CATEGORIES = ["first_order_stable_fast", "first_order_stable_moderate", "first_order_stable_slow",
              "first_order_unstable", "first_order_w_delay", "second_order_stable_fast",
              "second_order_stable_moderate", "second_order_stable_slow", "second_order_unstable", "higher_order"]
OUT = Path(__file__).resolve().parent / "upstream"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "aeoi-eval"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def main():
    OUT.mkdir(exist_ok=True)
    try:
        commit = json.loads(get(f"https://api.github.com/repos/{REPO}/commits/main"))["sha"]
    except Exception as e:  # unauthenticated API rate limit: try the gh CLI, else the branch name
        try:
            import subprocess
            commit = subprocess.check_output(["gh", "api", f"repos/{REPO}/commits/main", "--jq", ".sha"], text=True, timeout=60).strip()
        except Exception:
            print(f"warning: could not resolve commit ({e}); using main", file=sys.stderr)
            commit = "main"
    tasks = []
    for cat in CATEGORIES:
        data = json.loads(get(f"https://raw.githubusercontent.com/{REPO}/{commit}/ControlEval/{cat}_data.json"))
        for i, t in enumerate(data):
            t = dict(t)
            t["category"], t["index"] = cat, i
            tasks.append(t)
        print(f"{cat}: {len(data)}")
    meta = {"repo": f"https://github.com/{REPO}", "path": "ControlEval/", "commit": commit,
            "license": "none declared (not redistributable); fetched for local evaluation only",
            "via": "https://github.com/EnvCommons/ControlEval (no license declared)"}
    (OUT / "controleval.json").write_text(json.dumps({"source": meta, "tasks": tasks}, indent=1) + "\n")
    print(f"{len(tasks)} tasks @ {commit[:12]} → {OUT / 'controleval.json'}")


if __name__ == "__main__":
    main()
