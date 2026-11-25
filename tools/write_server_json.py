#!/usr/bin/env python3
"""Print server.json for the official MCP registry (spec 12.2 step 6, v0.4.9).

    python3 tools/write_server_json.py <tag> <sha256-of-aeoi.mcpb>

The registry stores metadata only; the package is the MCPB file of the GitHub Release for <tag>. Clients
check fileSha256 before installing. Published by .github/workflows/release.yml with GitHub OIDC.
"""
import json
import re
import sys

tag, sha = sys.argv[1], sys.argv[2]
m = re.fullmatch(r"v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)", tag)
if not m or not re.fullmatch(r"[0-9a-f]{64}", sha):
    sys.exit(f"usage: write_server_json.py v<semver> <sha256>   (got {tag!r}, {sha!r})")
version = m.group(1)
DESC_MAX = 100  # registry schema 2025-12-11
print(json.dumps({
    "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
    "name": "io.github.yuanqingfei/aeoi",
    "title": "AEOI engineering operators",
    "description": "Engineering computations with evidence: control, FFT, ODE; checked accuracy, replayable receipts.",
    "version": version,
    "repository": {"url": "https://github.com/yuanqingfei/aeoi", "source": "github"},
    "packages": [{
        "registryType": "mcpb",
        "identifier": f"https://github.com/yuanqingfei/aeoi/releases/download/{tag}/aeoi.mcpb",
        "fileSha256": sha,
        "transport": {"type": "stdio"},
    }],
}, indent=2))
assert len("Engineering computations with evidence: control, FFT, ODE; checked accuracy, replayable receipts.") <= DESC_MAX
