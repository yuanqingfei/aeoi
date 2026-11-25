#!/usr/bin/env python3
"""Print the MCPB manifest.json for the Claude Desktop bundle (spec 12.2 step 6, v0.4.9).

    python3 tools/write_mcpb_manifest.py <tag-or-version> <path-to-aeoi-mcp>

The tool list comes from the binary itself (`tools/list`), so the bundle always describes what it ships.
The bundle holds a universal macOS binary `server/aeoi-mcp` and `server/aeoi-mcp.exe` for Windows.
"""
import json
import re
import subprocess
import sys

tag, binary = sys.argv[1], sys.argv[2]
m = re.fullmatch(r"v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)", tag)
version = m.group(1) if m else "0.0.0-dev"

msgs = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize",
     "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "write_mcpb_manifest", "version": "0"}}},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
]
out = subprocess.run([binary], input="".join(json.dumps(x) + "\n" for x in msgs), capture_output=True, text=True, check=True).stdout
tools = next(json.loads(l)["result"]["tools"] for l in out.splitlines() if l.strip() and json.loads(l).get("id") == 2)


def short(desc):
    d = desc.split(" [AEOI")[0].strip()
    first = re.split(r"(?<=[。.])\s*", d, maxsplit=1)[0]
    return first[:300]


manifest = {
    "manifest_version": "0.3",
    "name": "aeoi",
    "display_name": "AEOI engineering operators",
    "version": version,
    "description": "Engineering computations with evidence: control (bode, step, lqr, margin, pidtune, …), FFT and ODE initial value problems. Every result carries units, accuracy, post-condition checks and a replayable receipt.",
    "long_description": "AEOI (AI Engineering Operator Interface Spec) is a semantic layer on top of MCP for engineering computation. "
                        "Each tool returns an envelope with the result, its tolerance class, which post-conditions were checked, "
                        "which fields are unreliable, structured errors with remediation patches, and a receipt that `aeoi_verify` can replay. "
                        "The binary is the reference implementation (pure Rust; the same bits on every platform). Reference values come from "
                        "python-control, numpy, scipy and mpmath, not from MATLAB.",
    "author": {"name": "Qingfei Yuan", "url": "https://github.com/yuanqingfei"},
    "repository": {"type": "git", "url": "https://github.com/yuanqingfei/aeoi"},
    "homepage": "https://github.com/yuanqingfei/aeoi",
    "documentation": "https://github.com/yuanqingfei/aeoi/blob/main/spec-v0.4.md",
    "support": "https://github.com/yuanqingfei/aeoi/issues",
    "license": "MIT OR Apache-2.0",
    "keywords": ["engineering", "control", "bode", "lqr", "pid", "fft", "ode", "numerical", "reproducible"],
    "server": {
        "type": "binary",
        "entry_point": "server/aeoi-mcp",
        "mcp_config": {
            "command": "${__dirname}/server/aeoi-mcp",
            "args": [],
            "platform_overrides": {"win32": {"command": "${__dirname}/server/aeoi-mcp.exe"}},
        },
    },
    "tools": [{"name": t["name"], "description": short(t.get("description", ""))} for t in tools],
    "tools_generated": False,
    "compatibility": {"platforms": ["darwin", "win32"]},
}
print(json.dumps(manifest, indent=2, ensure_ascii=False))
