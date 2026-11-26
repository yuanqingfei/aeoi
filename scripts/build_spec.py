#!/usr/bin/env python3
"""spec-src.md → spec-v0.4.md, and a link check over the split documents (profiles/, docs/, CHANGELOG.md).
{{file:path}}  inlines a file verbatim.
{{jsonc:path}} inlines a JSON file re-indented, with numeric arrays longer than 16 elements shortened
               for readability (marked "…(+N)…"; the full content is in the file)."""
import json, re, pathlib
root = pathlib.Path(__file__).resolve().parents[1]

def compact(v, ind=0):
    pad = "  " * ind
    if isinstance(v, dict):
        if not v: return "{}"
        items = [f'{pad}  {json.dumps(k, ensure_ascii=False)}: {compact(x, ind + 1)}' for k, x in v.items()]
        return "{\n" + ",\n".join(items) + "\n" + pad + "}"
    if isinstance(v, list):
        if all(not isinstance(x, (dict, list)) for x in v) or all(isinstance(x, list) and len(x) == 2 and all(not isinstance(y, (dict, list)) for y in x) for x in v):
            el = [json.dumps(x, ensure_ascii=False, separators=(", ", ": ")) for x in v]
            if len(el) > 16:
                el = el[:6] + [f'"…(+{len(el) - 8})…"'] + el[-2:]
            line = "[" + ", ".join(el) + "]"
            if len(line) < 160 or len(el) <= 9: return line
        return "[\n" + ",\n".join(pad + "  " + compact(x, ind + 1) for x in v) + "\n" + pad + "]"
    return json.dumps(v, ensure_ascii=False)

src = (root / "spec-src.md").read_text()
def sub(m):
    kind, path = m.group(1), m.group(2)
    text = (root / path).read_text()
    return text.rstrip("\n") if kind == "file" else compact(json.loads(text))
out = re.sub(r"\{\{(file|jsonc):([^}]+)\}\}", sub, src)
(root / "spec-v0.4.md").write_text(out)
print("spec-v0.4.md", len(out.splitlines()), "lines")

# every relative Markdown link in the split documents must point to an existing file
bad = []
docs = [root / "spec-v0.4.md", root / "CHANGELOG.md", root / "docs" / "reference-impl.md"] + sorted((root / "profiles").glob("*.md"))
for d in docs:
    for target in re.findall(r"\]\(([^)#\s]+)\)", d.read_text()):
        if "://" in target:
            continue
        if not (d.parent / target).exists():
            bad.append(f"{d.relative_to(root)}: {target}")
print(len(docs), "documents,", len(bad), "broken links")
for b in bad:
    print("  broken:", b)
raise SystemExit(1 if bad else 0)
