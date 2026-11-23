// wasm-vs-native parity: node parity/parity.mjs <pkg-node dir> <cases.json> <native.jsonl> [report.json]
// Runs every case through the wasm build (wasm-bindgen --target nodejs) and compares with the
// native run (cargo run --example golden_dump): status, error code, warning codes, input_hash,
// output_hash (sha256 of the canonical result, so equal hash = bitwise-equal result), and — when
// the hashes differ — the normwise difference per result field (max|Δ| / max|native|).
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
const [pkg, casesFile, nativeFile, reportFile] = process.argv.slice(2);
const require = createRequire(import.meta.url);
const wasm = require(path.resolve(pkg, "aeoi_wasm.js"));
const cases = JSON.parse(readFileSync(casesFile, "utf8"));
const native = new Map(readFileSync(nativeFile, "utf8").trim().split("\n").map((l) => JSON.parse(l)).map((x) => [x.case, x]));
const nums = (v, out = []) => { if (typeof v === "number") out.push(v); else if (Array.isArray(v)) v.forEach((x) => nums(x, out)); else if (v && typeof v === "object") Object.values(v).forEach((x) => nums(x, out)); return out; };
const info = JSON.parse(wasm.info());
let same = 0, hashDiff = 0, mismatch = 0, maxRel = 0;
const diffs = [];
const byOp = {};
const t0 = Date.now();
for (const c of cases) {
  const env = JSON.parse(wasm.run_operator(c.operator, c.request_text));
  const w = { status: env.status, error_code: env.error?.code ?? null, warnings: env.warnings ? env.warnings.map((x) => x.code) : null, input_hash: env.reproducibility?.input_hash ?? null, output_hash: env.reproducibility?.output_hash ?? null };
  const n = native.get(c.case);
  const o = (byOp[c.operator] ??= { cases: 0, bitwise: 0, max_rel_diff: 0 });
  o.cases++;
  const meta = ["status", "error_code", "input_hash"].every((k) => JSON.stringify(w[k]) === JSON.stringify(n[k] ?? null)) && JSON.stringify(w.warnings) === JSON.stringify(n.warnings ?? null);
  if (meta && w.output_hash === (n.output_hash ?? null)) { same++; o.bitwise++; continue; }
  // per top-level result field: max|wasm − native| / max|native| (normwise, so tiny entries do not blow up)
  let rel = 0;
  const per_field = {};
  for (const k of new Set([...Object.keys(env.result ?? {}), ...Object.keys(n.result ?? {})])) {
    const a = nums(env.result?.[k]), b = nums(n.result?.[k]);
    let d = a.length === b.length ? 0 : Infinity;
    const scale = Math.max(1e-300, ...b.map(Math.abs));
    if (d === 0) for (let i = 0; i < a.length; i++) d = Math.max(d, Math.abs(a[i] - b[i]) / scale);
    per_field[k] = d;
    rel = Math.max(rel, d);
  }
  o.max_rel_diff = Math.max(o.max_rel_diff, rel);
  if (meta) hashDiff++; else mismatch++;
  maxRel = Math.max(maxRel, rel);
  diffs.push({ case: c.case, operator: c.operator, meta_equal: meta, max_rel_diff: rel, per_field, wasm: w, native: { status: n.status, error_code: n.error_code, warnings: n.warnings, output_hash: n.output_hash } });
}
const report = { generated_by: "rust/aeoi-wasm/parity/parity.mjs", wasm_platform: info.platform, node: process.version, cases: cases.length, bitwise_equal: same, output_hash_differs_only: hashDiff, status_or_code_mismatch: mismatch, max_rel_diff: maxRel, seconds: (Date.now() - t0) / 1000, by_operator: byOp, diffs: diffs.sort((x, y) => (x.meta_equal - y.meta_equal) || (y.max_rel_diff - x.max_rel_diff)).slice(0, 80) };
console.log(JSON.stringify({ ...report, diffs: diffs.length }, null, 1));
if (reportFile) writeFileSync(reportFile, JSON.stringify(report, null, 1) + "\n");
// CI gate: status/code mismatches above AEOI_PARITY_ALLOW_MISMATCH (default 0) fail. Up to lqr 0.2.0 one
// borderline case (condition estimate on the W.NUMERICAL.ILL_CONDITIONED threshold) was allowed; since
// lqr 0.3.0 (compensated Newton–Kleinman) lqr and lyap are bitwise equal on wasm and CI gates at 0.
const allow = +(process.env.AEOI_PARITY_ALLOW_MISMATCH || 0);
process.exit(mismatch > allow ? 1 : 0);
