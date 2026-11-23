# xcheck-tpt

Dev-only cross-check of `aeoi-control` against [tpt-control](https://github.com/tpt-solutions/tpt-control)
(© TPT Solutions, MIT OR Apache-2.0; pinned to git rev `10e4320`). Standalone cargo package with its own
empty `[workspace]`: not a member of `rust/`, not a dependency of any aeoi crate.

- `src/main.rs` runs the corpus + seeded random cases through both libraries, writes JSONL
- `src/bin/diag.rs` prints tpt internals for one case
- `analyze.py` computes 50-digit residuals / forward errors (mpmath) and the scipy third opinion
- `prototype_nk.py` checks the proposed K-form Newton–Kleinman CARE refinement (no AEOI change)
- `make_report.py` writes `reports/parity-tpt-control.json` and the tables of `reports/parity-tpt-control.md`

See `reports/parity-tpt-control.md` § 复现 for the commands.
