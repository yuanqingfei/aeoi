// Replay-verify every published example envelope (produced natively, x86_64-linux) inside wasm (spec 4.3):
// usage: node verify_examples.mjs <pkg-node dir> <repo root> [report.json]
// Exit 1 unless every verdict is bitwise_match / tolerance_match / error_reproduced.
import { createRequire } from "module";
import { readdirSync, readFileSync, writeFileSync } from "fs";
const [pkg, root, out] = process.argv.slice(2);
const w = createRequire(import.meta.url)(`${pkg}/aeoi_wasm.js`);
const rows = [], counts = {};
for (const op of readdirSync(`${root}/examples`).sort()) {
  for (const f of readdirSync(`${root}/examples/${op}`).filter((f) => f.startsWith("request")).sort()) {
    const resp = f === "request.json" ? "response.success.json" : f === "request.error.json" ? "response.error.json" : "response." + f.slice(8);
    const v = JSON.parse(w.verify(op, readFileSync(`${root}/examples/${op}/${f}`, "utf8"), readFileSync(`${root}/examples/${op}/${resp}`, "utf8")));
    counts[v.verdict] = (counts[v.verdict] || 0) + 1;
    rows.push({ operator: op, request: f, verdict: v.verdict, max_rel_diff: v.max_rel_diff ?? null });
  }
}
const report = { generated_by: "rust/aeoi-wasm/parity/verify_examples.mjs", platform: JSON.parse(w.info()).platform, node: process.version, counts, rows };
console.log(JSON.stringify({ ...report, rows: rows.length }, null, 1));
if (out) writeFileSync(out, JSON.stringify(report, null, 1) + "\n");
process.exit(rows.every((r) => ["bitwise_match", "tolerance_match", "error_reproduced"].includes(r.verdict)) ? 0 : 1);
