AEOI reference implementation: an MCP server for engineering computation (control, FFT, ODE) whose results carry accuracy, post-condition checks and replayable receipts. What changed in this version: the Changelog (chapter 13) of [`spec-v0.4.md`](https://github.com/yuanqingfei/aeoi/blob/main/spec-v0.4.md). Reference values come from python-control, numpy, scipy and mpmath, not from MATLAB.

## Install

**Claude Desktop (macOS, Windows)**: download `aeoi.mcpb` and double-click it (or Settings → Extensions → Advanced settings → Install Extension…). It adds the AEOI tools to Claude; nothing else changes and you can disable it at any time.

**Any MCP client** (Claude Code, Cursor, VS Code, …): download the archive for your platform, unpack it, and register the `aeoi-mcp` binary as a stdio server, for example

```bash
claude mcp add aeoi -- /path/to/aeoi-mcp
```

**Without an LLM**: `aeoi-call <operator-id> request.json` prints the result envelope; `aeoi-call verify …` replays a receipt.

| Archive | Platform |
|---|---|
| `aeoi-linux-x86_64.tar.gz`, `aeoi-linux-aarch64.tar.gz` | Linux (static, any distribution) |
| `aeoi-macos-arm64.tar.gz`, `aeoi-macos-x86_64.tar.gz` | macOS |
| `aeoi-windows-x86_64.zip` | Windows |

The same request gives the same bits on every platform (spec 2.1 M-7a): the release CI compared the `output_hash` of every golden request across them. `conformance-<platform>.json` are the conformance reports of each binary. Check downloads with `SHA256SUMS` or `gh attestation verify <file> --repo yuanqingfei/aeoi`.

The binaries are not code-signed yet; macOS may ask you to allow `aeoi-mcp` the first time it runs from a downloaded archive.
