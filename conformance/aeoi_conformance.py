#!/usr/bin/env python3
"""AEOI cross-implementation conformance runner, v0.1 (spec 9.4, CF-1 .. CF-11).

    python3 conformance/aeoi_conformance.py run --impl "<command that starts an MCP stdio server>" \
        [--golden golden] [--schemas schemas] [--operators control.math.lyap] [--engines python-control,mpmath] \
        [--report report.json] [--known-deviations deviations.json]

Cases are the existing golden files (CF-2). The runner drives the implementation only through MCP stdio (CF-3).
Numeric comparison rules exist for the operators in RULES; for every other operator a success case is reported
as `skip` (its error / warning expectations are still checked). Nothing the implementation reports about its own
accuracy is used to loosen a tolerance (CF-6).
"""
import argparse
import glob
import hashlib
import json
import os
import queue
import re
import shlex
import subprocess
import sys
import threading
import time

import numpy as np
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import aeoi_hash as H  # noqa: E402
import rules_control as RC  # noqa: E402

RUNNER_VERSION = "0.3.0"
CLASS_RTOL = {"T0": 0.0, "T1": 1e-12, "T2": 1e-10, "T3": 1e-9, "T4": 1e-6, "T5": 1e-6}   # spec 7.2
ENGINE_LIBS = {"python-control": ("python-control", "scipy", "slycot"), "mpmath": ("mpmath",), "MATLAB": ("matlab",)}
BITWISE = ("bitwise", "platform_bitwise", "seeded")                                      # M-7
TIMEOUT_S = 60
DEFAULT_VARIANTS = 3                                                                     # per (operator, field, direction)


# ---------------------------------------------------------------- MCP stdio client (CF-3)
class McpClient:
    def __init__(self, command):
        self.p = subprocess.Popen(shlex.split(command), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        self.q, self.err, self.n = queue.Queue(), [], 0
        threading.Thread(target=self._pump, args=(self.p.stdout, self.q), daemon=True).start()
        threading.Thread(target=lambda: [self.err.append(l) for l in self.p.stderr], daemon=True).start()

    @staticmethod
    def _pump(stream, q):
        for line in stream:
            q.put(line)
        q.put(None)

    def request(self, method, params=None):
        self.n += 1
        mid = self.n
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": mid, "method": method, "params": params or {}}) + "\n")
        self.p.stdin.flush()
        deadline = time.time() + TIMEOUT_S
        while True:
            try:
                line = self.q.get(timeout=max(0.1, deadline - time.time()))
            except queue.Empty:
                raise TimeoutError(f"no response to {method} within {TIMEOUT_S}s")
            if line is None:
                raise ConnectionError("implementation closed stdout; stderr tail: " + "".join(self.err[-5:]))
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == mid:
                return msg

    def notify(self, method):
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
        self.p.stdin.flush()

    def close(self):
        try:
            self.p.stdin.close()
            self.p.wait(timeout=5)
        except Exception:
            self.p.kill()


def envelope_of(resp):
    """-> (envelope or None, isError, problem). C-1: operator errors are normal tool results, not JSON-RPC errors."""
    if "error" in resp:
        return None, None, f"JSON-RPC error {resp['error'].get('code')}: {resp['error'].get('message')}"
    r = resp["result"]
    env = r.get("structuredContent")
    if env is None:
        for c in r.get("content", []):
            try:
                cand = json.loads(c.get("text", ""))
                if isinstance(cand, dict) and "status" in cand:
                    env = cand
            except (ValueError, TypeError):
                pass
    return env, r.get("isError"), None if env is not None else "no structuredContent and no envelope in content"


# ---------------------------------------------------------------- schemas (CF-8)
def load_registry(schema_dir):
    reg, docs = Registry(), {}
    for f in glob.glob(os.path.join(schema_dir, "*.schema.json")):
        d = json.load(open(f))
        docs[d["$id"]] = d
        reg = reg.with_resource(d["$id"], Resource.from_contents(d))
    return reg, docs


def bad_refs(schema, reg):
    """3.9 J-1: every non-local `$ref` in an operator schema MUST resolve by standard JSON Schema 2020-12 rules
    against the core schemas' `$id`s (e.g. `aeoi://schemas/common/0.4#/$defs/Matrix`). Returns the ones that do not.
    Local refs (`#/...`) are left to the validator; `./input.schema.json` style manifest pointers are not schemas."""
    out, stack = [], [schema]
    while stack:
        v = stack.pop()
        if isinstance(v, dict):
            r = v.get("$ref")
            if isinstance(r, str) and not r.startswith("#") and not r.startswith("./"):
                try:
                    reg.resolver().lookup(r)
                except Exception:
                    out.append(r)
            stack.extend(v.values())
        elif isinstance(v, list):
            stack.extend(v)
    return sorted(set(out))


# ---------------------------------------------------------------- numeric rules (CF-6)
def dense_of(o):
    if not (isinstance(o, dict) and o.get("dtype") == "f64" and "data" in o and "shape" in o):
        raise ValueError("not an inline dense f64 array")
    return np.array([float("nan") if x == "NaN" else (float("inf") if x == "Inf" else (float("-inf") if x == "-Inf" else x)) for x in o["data"]]).reshape(o["shape"])


def rel_fro(a, b):
    return float(np.linalg.norm(a - b) / max(np.linalg.norm(b), np.finfo(float).tiny))


def lyap_kappa(req):
    """Condition number of the Lyapunov operator, computed by the runner from the reference request (CF-6)."""
    A = dense_of(req["A"]); n = A.shape[0]
    if n > 20:
        return 1.0
    I = np.eye(n)
    L = np.kron(I, A) + np.kron(A, I) if req["type"] == "continuous" else np.kron(A, A) - np.eye(n * n)
    with np.errstate(all="ignore"):
        k = np.linalg.cond(L)
    return float(k) if np.isfinite(k) else 1e300


def rule_lyap(case, env, manifest, req=None, expected=None):
    """X compared by relative Frobenius norm; rtol = class rtol x max(1, kappa/100) (V-6), kappa from the reference request."""
    req = req or case["request"]; expected = expected or case["expected"]
    rtol = CLASS_RTOL["T2"] * max(1.0, lyap_kappa(req) / 100.0)       # class from parity.yaml, not the impl's manifest (CF-6)
    X = dense_of(env["result"]["X"]); Xr = dense_of(expected["X"])
    if X.shape != Xr.shape:
        return [("X.shape", False, None, None, f"shape {X.shape} != {Xr.shape}")]
    e = rel_fro(X, Xr)
    return [("X", e <= rtol, e, rtol, f"rel_fro={e:.3e} > tolerance={rtol:.3e}" if e > rtol else "")]


RULES = {"control.math.lyap": rule_lyap, **RC.RULES}


# ---------------------------------------------------------------- one case
def declared_ok(code, manifest):
    if code.startswith("W."):
        return code in manifest.get("warnings", []) or code.startswith("W.EVIDENCE.")
    return code in manifest.get("errors", [])


def recompute_claim(manifest, env):
    pcs = manifest["receipt"].get("postconditions", [])
    if not pcs:
        return "unchecked"
    checks = (env.get("trust") or {}).get("checks", {})
    if not all(checks.get(p) is True for p in pcs):
        return "unverified"
    cov = manifest["receipt"].get("coverage", {})
    return "spot_checked" if any(cov.get(p, "full") == "sampled" for p in pcs) else "verified"      # E-11 / E-12


def judge(case, env, is_error, manifest, validators, call):
    checks = []                                                            # (name, ok, max_error, tolerance, detail)
    add = lambda n, ok, d="", me=None, tol=None: checks.append((n, bool(ok), me, tol, d))
    add("protocol: isError == (status == error)  [C-1]", is_error == (env["status"] == "error"), f"isError={is_error} status={env['status']}")
    errs = sorted(validators["envelope"].iter_errors(env), key=lambda e: list(e.path))
    add("envelope validates against envelope schema  [CF-8]", not errs, "; ".join(f"{'/'.join(map(str, e.path))}: {e.message[:80]}" for e in errs[:3]))
    codes = ([env["error"]["code"]] if env.get("error") else []) + [w["code"] for w in env.get("warnings", [])]
    undeclared = [c for c in codes if not declared_ok(c, manifest)]
    add("every emitted E./W. code is declared in the manifest  [CF-8]", not undeclared, f"undeclared: {undeclared}")
    exp_err = case.get("expected_error")
    if exp_err:
        add(f"expected error {exp_err}  [CF-4]", env["status"] == "error" and env["error"]["code"] == exp_err,
            f"got status={env['status']} code={(env.get('error') or {}).get('code')}")
        return checks
    add("status is not error  [CF-4]", env["status"] != "error", f"error={(env.get('error') or {}).get('code')}: {(env.get('error') or {}).get('message', '')[:100]}")
    if env["status"] == "error":
        return checks
    got_w = {w["code"] for w in env.get("warnings", [])}
    for w in case.get("expected_warnings", []):
        add(f"expected warning {w}  [CF-4]", w in got_w, f"warnings: {sorted(got_w)}")
    if manifest.get("output_schema"):
        oe = sorted(validators["output"].iter_errors(env["result"]), key=lambda e: list(e.path))
        add("result validates against the output schema  [CF-8]", not oe, "; ".join(e.message[:80] for e in oe[:2]))
    rc = env.get("receipt") or {}
    # fingerprints recomputed by the runner (CF-4: request fingerprint must be identical across implementations)
    if not H.has_unsupported_arrays(case["request"]):
        ih = H.input_hash(case["request"]); rfp = H.request_fingerprint(manifest["id"], manifest["version"], ih)
        add("fingerprint.request equals the runner's recomputation  [CF-4]", (rc.get("fingerprint") or {}).get("request") == rfp,
            f"impl={(rc.get('fingerprint') or {}).get('request')} runner={rfp}")
        add("reproducibility.input_hash equals the runner's recomputation  [CF-4]", env["reproducibility"].get("input_hash") == ih)
    oh = H.output_hash(env["result"])
    add("output_hash is the hash of the returned result  [P-2]", env["reproducibility"].get("output_hash") == oh and (rc.get("fingerprint") or {}).get("output_hash") == oh)
    allowed = {manifest["receipt"]["method_id"]} | {a["method_id"] for a in manifest["receipt"].get("alternatives", [])}
    add("receipt.method.id is a registered method  [MS-1, CF-8]", (rc.get("method") or {}).get("id") in allowed, f"{(rc.get('method') or {}).get('id')} not in {sorted(allowed)}")
    add("receipt claim equals the runner's recomputation  [E-11/E-12, CF-8]", (rc.get("evidence") or {}).get("claim") == recompute_claim(manifest, env),
        f"impl={(rc.get('evidence') or {}).get('claim')} runner={recompute_claim(manifest, env)}")
    if manifest.get("determinism") in BITWISE:                              # P-7, CF-3
        e2, _, prob = call(case["request"])
        add("repeat run gives the same output_hash  [P-7]", e2 is not None and e2["reproducibility"]["output_hash"] == env["reproducibility"]["output_hash"], prob or "")
    rule = RULES.get(case["operator"])
    if rule:
        for c in rule(case, env, manifest):
            checks.append(c)
        if "column_major_mistake_request" in case:                          # section 0: wrong memory order must change the answer
            e3, _, prob = call(case["column_major_mistake_request"])
            if e3 is None or e3["status"] == "error":
                add("column-major request deviates or is rejected  [section 0]", True, "rejected or no result: " + (prob or (e3 or {}).get("error", {}).get("code", "")))
            else:
                # largest deviation/tolerance over the rule's numeric checks: a single field can be blind to the
                # mistake (eig(Aᵀ) = eig(A), so poles do not move while zeros and the DC gain do)
                try:
                    cm = rule(case, e3, manifest, req=case["request"])
                except Exception as ex:                                     # malformed answer to the wrong request: deviates
                    cm = [("rule raised", False, None, None, repr(ex)[:80])]
                ratios = [c[2] / c[3] for c in cm if c[2] is not None and c[3]]
                hard = any(not c[1] and c[2] is None for c in cm)          # shape / count / kind mismatch
                worst = max(ratios, default=0.0)
                add("column-major request deviates > 1e3 x tolerance  [section 0]", hard or worst > 1e3, f"max deviation/tolerance = {worst:.3e}")
    return checks


# ---------------------------------------------------------------- run
def main():
    ap = argparse.ArgumentParser(); sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--impl", required=True)
    r.add_argument("--golden", default="golden"); r.add_argument("--schemas", default="schemas")
    r.add_argument("--examples", default="examples", help="suite input schemas (defaults are read from here, not from the implementation)")
    r.add_argument("--operators", default="all"); r.add_argument("--engines", default="all")
    r.add_argument("--report", default=None); r.add_argument("--known-deviations", default=None)
    r.add_argument("--verbose", action="store_true")
    a = ap.parse_args()

    reg, docs = load_registry(a.schemas)
    env_validator = Draft202012Validator(docs["aeoi://schemas/envelope/0.4"], registry=reg)
    deviations = {d["case_id"]: d for d in json.load(open(a.known_deviations))} if a.known_deviations else {}

    files = sorted(f for f in glob.glob(os.path.join(a.golden, "*", "*", "*", "*.json")) if not f.endswith("INDEX.json"))
    want_ops = None if a.operators == "all" else set(a.operators.split(","))
    want_eng = None if a.engines == "all" else set(a.engines.split(","))
    cases = []
    for f in files:
        engine, op = f.split(os.sep)[-4], f.split(os.sep)[-3]
        if (want_ops and op not in want_ops) or (want_eng and engine not in want_eng):
            continue
        d = json.load(open(f)); d["_engine"] = engine; d["_file"] = f
        d["_id"] = f"{engine}/{op}/{d['case_family']}/{os.path.basename(f)[:12]}"
        cases.append(d)
    suite_hash = "sha256:" + hashlib.sha256(b"".join(os.path.relpath(c["_file"], a.golden).encode() + open(c["_file"], "rb").read() for c in cases)).hexdigest()

    cli = McpClient(a.impl)
    init = cli.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "aeoi-conformance", "version": RUNNER_VERSION}})
    if "error" in init:
        sys.exit("initialize failed: " + json.dumps(init["error"]))
    cli.notify("notifications/initialized")
    info = init["result"].get("serverInfo", {})
    tools = {t["name"]: t for t in cli.request("tools/list")["result"]["tools"] if "aeoi/id" in t.get("_meta", {})}
    by_id = {t["_meta"]["aeoi/id"]: t for t in tools.values()}

    manifests, validators, results, notes, ref_problems, ok_envs = {}, {}, [], set(), {}, {}
    for op in sorted({c["operator"] for c in cases}):
        t = by_id.get(op)
        if not t:
            continue
        resp = cli.request("resources/read", {"uri": t["_meta"]["aeoi/manifest"]})
        if "error" in resp:
            notes.add(f"{op}: manifest resource not readable ({resp['error'].get('message')})")
            continue
        m = json.loads(resp["result"]["contents"][0]["text"]); manifests[op] = m
        validators[op] = {"envelope": env_validator}
        bad = bad_refs([m.get("input_schema"), m.get("output_schema")], reg)
        if bad:
            ref_problems[op] = "3.9 J-1: $ref not resolvable by standard JSON Schema 2020-12: " + ", ".join(bad[:5])
        if m.get("output_schema") and "$ref" not in m["output_schema"] and not bad:
            validators[op]["output"] = Draft202012Validator(m["output_schema"], registry=reg)
        else:
            m["output_schema"] = None

    def make_call(op):
        def call(args):
            try:
                resp = cli.request("tools/call", {"name": by_id[op]["name"], "arguments": args})
            except (TimeoutError, ConnectionError) as e:
                return None, None, str(e)
            return envelope_of(resp)
        return call

    for c in cases:
        op = c["operator"]
        if op not in by_id:
            results.append({"case_id": c["_id"], "operator": op, "engine": c["_engine"], "result": "skip", "detail": "operator not implemented by the implementation"}); continue
        if op not in manifests:
            results.append({"case_id": c["_id"], "operator": op, "engine": c["_engine"], "result": "fail", "detail": "manifest resource not readable"}); continue
        if op in ref_problems:
            results.append({"case_id": c["_id"], "operator": op, "engine": c["_engine"], "result": "fail", "detail": ref_problems[op]}); continue
        call = make_call(op)
        env, is_err, prob = call(c["request"])
        if env is not None and env.get("status") in ("ok", "ok_with_warnings") and "expected" in c:
            ok_envs.setdefault(op, []).append((c, env))
        if env is None:
            results.append({"case_id": c["_id"], "operator": op, "engine": c["_engine"], "result": "fail", "detail": prob}); continue
        try:
            checks = judge(c, env, is_err, manifests[op], validators[op], call)
        except Exception as e:                                             # a malformed envelope must be a failure, not a crash
            checks = [("envelope could be judged", False, None, None, f"{type(e).__name__}: {e}")]
        failed = [x for x in checks if not x[1]]
        needs_rule = ("expected" in c) and op not in RULES
        res = "fail" if failed else ("skip" if needs_rule else "pass")
        if failed and c["_id"] in deviations and all(x[2] is not None for x in failed):
            res = "pass_with_deviation"
        errs = [x[2] for x in checks if x[2] is not None and x[0] in ("X",)]
        results.append({"case_id": c["_id"], "operator": op, "engine": c["_engine"], "result": res,
                        "method": (env.get("receipt") or {}).get("method", {}).get("id"),
                        "max_error": max(errs) if errs else None, "tolerance": next((x[3] for x in checks if x[3]), None),
                        "failed_checks": [{"check": x[0], "detail": x[4]} for x in failed],
                        "detail": "no numeric comparison rule for this operator in runner v0.1" if res == "skip" else ""})

    # CF-10 (what can run today): a hard method constraint that cannot be met must never succeed (MS-6, MS-8)
    for op, m in manifests.items():
        base = next((c for c in cases if c["operator"] == op and "expected" in c), None)
        if not base:
            continue
        req = json.loads(json.dumps(base["request"])); req.setdefault("exec", {})["method_policy"] = {"require": ["conformance:unregistered:method"]}
        env, _, prob = make_call(op)(req)
        ok = env is not None and env["status"] == "error" and env["error"]["code"] in ("E.VALIDATION.SCHEMA", "E.VALIDATION.METHOD_UNAVAILABLE")
        results.append({"case_id": f"method-policy/{op}/unregistered-require", "operator": op, "engine": "conformance", "result": "pass" if ok else "fail",
                        "detail": "" if ok else f"hard constraint not honoured: {prob or (env or {}).get('status')}"})
        if m["receipt"].get("alternatives"):
            notes.add(f"{op}: manifest declares alternatives; per-method runs (CF-10) are not implemented in runner v0.1")

    # CF-2 / spec 12.1 item 21: default values. Derived from the golden cases, not stored: for each top-level input
    # field with a default in the SUITE's input schema (examples/<op>/input.schema.json, not the implementation's
    # manifest), a successful case that writes the default gets a variant that omits it, and one that omits the field
    # gets a variant that writes the default. Both ask the same question (F-7), so the result MUST be bit-identical
    # to the original run: same output_hash (a `convention` field's echo included). A variant answered with
    # E.VALIDATION.OPTION_CONFLICT naming the added field is skipped: that field does not apply in the request's mode
    # (F-8, e.g. Ki under PID form = standard), so writing it is not the same question.
    for op in sorted(ok_envs):
        sf = os.path.join(a.examples, op, "input.schema.json")
        if not os.path.exists(sf):
            continue
        props = json.load(open(sf)).get("properties", {})
        defaults = {k: v["default"] for k, v in props.items() if isinstance(v, dict) and "default" in v}
        for field, dv in sorted(defaults.items()):
            picked = {"omit": [], "add": []}
            for c, env in sorted(ok_envs[op], key=lambda x: x[0]["_id"]):
                d = "omit" if c["request"].get(field, object()) == dv else ("add" if field not in c["request"] else None)
                if d and len(picked[d]) < DEFAULT_VARIANTS:
                    picked[d].append((c, env))
            for d, lst in picked.items():
                for c, env in lst:
                    req = json.loads(json.dumps(c["request"]))
                    if d == "omit":
                        del req[field]
                    else:
                        req[field] = dv
                    e2, _, prob = make_call(op)(req)
                    cid = f"defaults/{op}/{field}/{d}/{c['_id'].split('/')[-1]}"
                    if e2 is None:
                        res, det = "fail", prob
                    elif e2["status"] == "error":
                        conflict = e2["error"]["code"] == "E.VALIDATION.OPTION_CONFLICT" and field in json.dumps((e2["error"].get("details") or {}).get("conflicting_fields", []))
                        res, det = ("skip", "field does not apply in this mode (F-8)") if conflict else ("fail", f"default {d} changed the answer to error {e2['error']['code']}")
                    else:
                        same = e2["reproducibility"].get("output_hash") == env["reproducibility"].get("output_hash")
                        res, det = ("pass", "") if same else ("fail", f"default {d} changed the result (output_hash differs from the original request)")
                    results.append({"case_id": cid, "operator": op, "engine": "conformance", "group": "defaults", "result": res,
                                    "failed_checks": [] if res != "fail" else [{"check": f"{'omitting' if d == 'omit' else 'writing'} the default {field}={json.dumps(dv)} gives the same result  [F-7, 12.1 #21]", "detail": det}],
                                    "detail": det})

    cli.close()
    # ---- report (CF-11)
    groups = {}
    for x in results:
        if x["engine"] == "conformance":
            key = (x["operator"], manifests[x["operator"]]["version"], x.get("group", "method-policy"))
        else:
            key = (x["operator"], manifests.get(x["operator"], {}).get("version"), next((y.get("method") for y in results if y["operator"] == x["operator"] and y.get("method")), None))
        g = groups.setdefault(key, {"pass": 0, "pass_with_deviation": 0, "fail": 0, "skip": 0, "by_engine": {}})
        g[x["result"]] += 1
        if x["engine"] != "conformance" and x["result"] in ("pass", "pass_with_deviation"):
            lib = manifests.get(x["operator"], {}).get("backend", {}).get("library", "").lower()
            e = g["by_engine"].setdefault(x["engine"], {"n": 0, "same_backend": any(k in lib for k in ENGINE_LIBS.get(x["engine"], (x["engine"].lower(),)))})
            e["n"] += 1
    report = {"runner_version": RUNNER_VERSION, "suite": "golden", "suite_hash": suite_hash,
              "impl": {"command": a.impl, "serverInfo": info, "form": "wire"},
              "results": [{"operator": k[0], "version": k[1], "method": k[2], **v} for k, v in groups.items()],
              "known_deviations": list(deviations.values()), "notes": sorted(notes), "cases": results}
    if a.report:
        json.dump(report, open(a.report, "w"), ensure_ascii=False, indent=1)
    tot = {k: sum(1 for x in results if x["result"] == k) for k in ("pass", "pass_with_deviation", "fail", "skip")}
    print(f"impl: {info.get('name')} {info.get('version')}   suite {suite_hash[:19]}…   cases {len(results)}")
    for g in report["results"]:
        print(f"  {g['operator']}@{g['version']} [{g['method']}]  pass {g['pass']}  deviation {g['pass_with_deviation']}  fail {g['fail']}  skip {g['skip']}   "
              + "  ".join(f"{e}: {v['n']} ({'same backend' if v['same_backend'] else 'independent'})" for e, v in g["by_engine"].items()))
    print("TOTAL", tot)
    for n in sorted(notes):
        print("NOTE:", n)
    for x in results:
        if x["result"] == "fail" and (a.verbose or sum(1 for y in results if y["result"] == "fail") <= 12):
            print("FAIL", x["case_id"], x.get("detail", ""), *[f"\n      - {f['check']}: {f['detail']}" for f in x.get("failed_checks", [])])
    sys.exit(1 if tot["fail"] else 0)


if __name__ == "__main__":
    main()
