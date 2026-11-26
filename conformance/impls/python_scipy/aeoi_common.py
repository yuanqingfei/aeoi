"""Shared AEOI plumbing for the Python second implementation.

Everything here is written from the spec text (spec-v0.4.md): array decoding (3.1, 3.6 L-1/L-2,
T-3, R-2), schema validation with leaf-level violations (3.11 F-3, F-4), method policy (3.14
MS-5 .. MS-8), envelope (4.1 E-1 .. E-13), receipt and fingerprints (4.3 R-1 .. R-4, R-9),
structured errors (5.1, 5.2 X-5 .. X-8), output encoding (L-5) and dry run (E-9).

JCS and the json-array digest are reused from conformance/aeoi_hash.py (an independent rewrite of
3.7 / RFC 8785); the b64 digest path and the $array canonical form for b64 inputs are added here.
"""
import base64
import binascii
import copy
import difflib
import hashlib
import json
import math
import os
import platform
import struct
import subprocess
import sys
import time

import numpy as np
import jsonschema

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(REPO, "conformance"))
import aeoi_hash as H  # noqa: E402

SPEC = "aeoi/0.4"
IMPL_NAME = "aeoi-pyscipy"
IMPL_VERSION = "0.2.0"
U = 2.0 ** -53          # unit roundoff
EPS = 2.0 ** -52        # machine epsilon


def _git_sha():
    try:
        out = subprocess.run(["git", "-C", REPO, "rev-parse", "--short=12", "HEAD"], capture_output=True, text=True, timeout=5)
        if out.returncode == 0 and out.stdout.strip():
            return out.stdout.strip()
    except Exception:
        pass
    return "nogit"


import scipy  # noqa: E402

# P-1: implementation version, commit, main numeric dependencies, interpreter version.
BUILD = (f"{IMPL_NAME} {IMPL_VERSION}+{_git_sha()} numpy-{np.__version__} scipy-{scipy.__version__} "
         f"python-{platform.python_version()}")
PLATFORM = f"{platform.machine()}-{sys.platform}"
_libc = platform.libc_ver()
LIBM = f"{_libc[0]} {_libc[1]}" if _libc[0] else "unknown"


def _cpu_features():
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("flags"):
                    flags = set(line.split(":", 1)[1].split())
                    return [x for x in ("sse4_2", "avx", "avx2", "fma", "avx512f") if x in flags]
    except OSError:
        pass
    return []


CPU_FEATURES = _cpu_features()

# --------------------------------------------------------------------------------------------
# errors


class Fail(Exception):
    """An AEOI structured error (5.1 / 5.2). `remediation` defaults to a single ask_user (X-7)."""

    def __init__(self, code, message, path=None, details=None, remediation=None, agent_hint=None, retryable=None):
        super().__init__(message)
        self.code = code
        self.message = message
        self.path = path
        self.details = dict(details or {})
        self.remediation = remediation
        self.agent_hint = agent_hint
        self.category = code.split(".")[1].lower()
        if retryable is None:
            # 5.1 defaults: validation true, numerical true, resource true, internal false; domain: case by case
            retryable = self.category in ("validation", "numerical", "resource")
        self.retryable = retryable


def ask_user(description, rid="ask"):
    return {"id": rid, "kind": "ask_user", "description": description, "confidence": "high", "requires_user_confirmation": True}


def patch_remedy(rid, description, patch, confidence="medium", confirm=True):
    return {"id": rid, "kind": "modify_input", "description": description, "confidence": confidence,
            "requires_user_confirmation": confirm, "patch": patch}


# --------------------------------------------------------------------------------------------
# schemas (3.9 J-1 .. J-3): load the core schemas and inline every $ref


def _load_core_schemas():
    docs = {}
    sdir = os.path.join(REPO, "schemas")
    for fn in sorted(os.listdir(sdir)):
        if fn.endswith(".schema.json"):
            d = json.load(open(os.path.join(sdir, fn), encoding="utf-8"))
            docs[d["$id"]] = d
    return docs


CORE = _load_core_schemas()


def _resolve_pointer(doc, frag):
    node = doc
    if frag in ("", "/"):
        return node
    for part in frag.lstrip("/").split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        node = node[int(part)] if isinstance(node, list) else node[part]
    return node


def inline_refs(schema, base_id=None, depth=0):
    """Return a copy of `schema` with every standard $ref replaced by the referenced subschema."""
    if depth > 64:
        raise RuntimeError("schema $ref nesting too deep")
    if isinstance(schema, list):
        return [inline_refs(x, base_id, depth) for x in schema]
    if not isinstance(schema, dict):
        return schema
    if "$ref" in schema:
        ref = schema["$ref"]
        uri, _, frag = ref.partition("#")
        doc_id = uri or base_id
        if doc_id not in CORE:
            raise RuntimeError(f"cannot resolve $ref {ref}")
        target = _resolve_pointer(CORE[doc_id], frag)
        out = inline_refs(copy.deepcopy(target), doc_id, depth + 1)
        if isinstance(out, dict):
            out = {k: v for k, v in out.items() if k not in ("$schema", "$id", "$defs")}
            for k, v in schema.items():
                if k != "$ref":
                    out[k] = inline_refs(v, base_id, depth)
        return out
    # every $ref is inlined, so $defs are no longer needed in the output
    return {k: inline_refs(v, base_id, depth) for k, v in schema.items() if k != "$defs"}


def load_io_schemas(op_id):
    base = os.path.join(REPO, "examples", op_id)
    out = []
    for fn in ("input.schema.json", "output.schema.json"):
        s = json.load(open(os.path.join(base, fn), encoding="utf-8"))
        out.append(inline_refs(s, s.get("$id")))
    return out


def envelope_output_schema(output_schema):
    """8.1: the MCP outputSchema is the envelope schema with `result` replaced by the output schema (inlined)."""
    env = inline_refs(CORE["aeoi://schemas/envelope/0.4"], "aeoi://schemas/envelope/0.4")
    env = {k: v for k, v in env.items() if k not in ("$id", "$schema")}
    env["properties"]["result"] = {"anyOf": [{"type": "null"}, output_schema]}
    return env


def _branch_score(branch_schema, inst):
    """How well an instance fits a oneOf branch by its discriminators (F-3)."""
    if not isinstance(branch_schema, dict) or not isinstance(inst, dict):
        return 0
    score = 0
    for k in branch_schema.get("required", []):
        score += 1 if k in inst else -1
    for k, sub in (branch_schema.get("properties") or {}).items():
        if isinstance(sub, dict) and "const" in sub and isinstance(sub["const"], str) and k in inst:
            score += 3 if inst[k] == sub["const"] else -3
    return score


def _leaf_errors(err):
    if err.validator in ("oneOf", "anyOf") and err.context:
        branches = {}
        for e in err.context:
            branches.setdefault(e.relative_schema_path[0], []).append(e)
        schemas = err.validator_value
        best = sorted(branches.items(), key=lambda kv: (-_branch_score(schemas[kv[0]], err.instance), len(kv[1]), kv[0]))[0][1]
        out = []
        for e in best:
            out.extend(_leaf_errors(e))
        return out
    return [err]


def _pointer(parts):
    return "".join("/" + str(p).replace("~", "~0").replace("/", "~1") for p in parts)


def schema_check(args, schema):
    """3.11 F-3: leaf-level violations; F-4: lossless nested-array patch. Raises Fail(E.VALIDATION.SCHEMA)."""
    v = jsonschema.Draft202012Validator(schema)
    errs = sorted(v.iter_errors(args), key=lambda e: (list(map(str, e.absolute_path)), e.message))
    if not errs:
        return
    violations, remedies = [], []
    props = list((schema.get("properties") or {}).keys())
    for e in errs:
        for leaf in _leaf_errors(e):
            path = _pointer(leaf.absolute_path)
            msg = leaf.message
            if leaf.validator == "required":
                missing = [k for k in leaf.validator_value if not (isinstance(leaf.instance, dict) and k in leaf.instance)]
                if missing:
                    path = path + "/" + missing[0]
            elif leaf.validator == "additionalProperties" and isinstance(leaf.instance, dict) and leaf.absolute_path == type(leaf.absolute_path)():
                extra = [k for k in leaf.instance if k not in props]
                for k in extra:
                    close = difflib.get_close_matches(k, props, n=1, cutoff=0.75)
                    if close and close[0] not in leaf.instance:
                        remedies.append(patch_remedy(f"rename_{k}", f"参数名 {k} 不存在，可能是 {close[0]}",
                                                     [{"op": "remove", "path": "/" + k}, {"op": "add", "path": "/" + close[0], "value": leaf.instance[k]}],
                                                     confidence="medium", confirm=True))
                if extra:
                    path = "/" + extra[0]
            violations.append({"path": path, "message": msg})
    # F-4: rectangular nested arrays -> row-major DenseMatrix (lossless)
    if isinstance(args, dict):
        for k, val in args.items():
            dm = _nested_to_dense(val)
            if dm is not None:
                remedies.insert(0, patch_remedy("nested_array_to_dense", f"把嵌套数组 {k} 改写为 row-major DenseMatrix（数值不变）",
                                                [{"op": "replace", "path": "/" + k, "value": dm}], confidence="high", confirm=False))
    seen, uniq = set(), []
    for x in violations:
        key = (x["path"], x["message"])
        if key not in seen:
            seen.add(key)
            uniq.append(x)
    remedies.append(ask_user("请对照工具的 input schema 修正参数；不确定的物理量先问用户"))
    raise Fail("E.VALIDATION.SCHEMA", f"输入不符合 schema：{uniq[0]['path'] or '/'}: {uniq[0]['message']}",
               uniq[0]["path"], {"violations": uniq}, remedies,
               agent_hint="按 details.violations 逐条修正；矩阵用 {dtype, shape, data}（row-major 展平）")


def _nested_to_dense(val):
    if not (isinstance(val, list) and val and all(isinstance(r, list) and r for r in val)):
        return None
    n = len(val[0])
    if any(len(r) != n for r in val):
        return None
    if not all(isinstance(x, (int, float)) and not isinstance(x, bool) for r in val for x in r):
        return None
    return {"dtype": "f64", "shape": [len(val), n], "data": [float(x) for r in val for x in r]}


# --------------------------------------------------------------------------------------------
# arrays (3.1, 3.5, 3.6, 3.7)

_NONFIN = {"NaN": float("nan"), "Inf": float("inf"), "-Inf": float("-inf")}
_ITEMSIZE = {"f64": 8, "i64": 8, "c128": 16, "bool": 1}
LIMIT_ARRAY_BYTES = 64 * 1024 * 1024     # L-6 default
LIMIT_INLINE_ELEMS = 10 ** 6             # L-6 default


class Array:
    """A decoded input array: dtype, shape, numpy values (float64 / complex128 / bool), content digest."""

    def __init__(self, dtype, shape, values, digest):
        self.dtype, self.shape, self.values, self.digest = dtype, shape, values, digest


def _shape_product(shape, path):
    n = 1
    for d in shape:
        n *= int(d)                 # Python ints: no overflow (SEC-6)
        if n > 2 ** 62:
            raise Fail("E.RESOURCE.INPUT_TOO_LARGE", "shape 的元素个数溢出", path + "/shape",
                       remediation=[ask_user("检查 shape 是否录入错误")])
    return n


def read_array(obj, path, allowed=("f64", "i64", "c128"), max_elements=10 ** 7, sparse_policy="reject"):
    """Decode a Matrix (3.1). Order of checks: encoding -> declared size (SEC-2) -> digest (SEC-3) -> T-3 -> dtype -> R-2."""
    if not isinstance(obj, dict):
        raise Fail("E.VALIDATION.SCHEMA", f"{path} 必须是数组对象 {{dtype, shape, data}}", path)
    dtype = obj.get("dtype")
    shape = obj.get("shape")
    if "format" in obj:
        # S-5 / M-9: this implementation declares policy reject for every matrix field.
        dense = _sparse_to_dense_value(obj)
        rem = []
        if dense is not None:
            rem.append(patch_remedy("densify", f"把 {path} 改写为稠密数组（数值不变）", [{"op": "replace", "path": path, "value": dense}],
                                    confidence="high", confirm=False))
        rem.append(ask_user("本算子不接受稀疏矩阵；请提供稠密数组"))
        raise Fail("E.VALIDATION.DTYPE", f"{path}：本算子不接受稀疏矩阵（sparse_support = reject）", path, remediation=rem)
    if "uri" in obj:
        uri = str(obj.get("uri"))
        if not uri.startswith("file://"):
            raise Fail("E.VALIDATION.REF_UNSUPPORTED", f"{path}：只支持 file:// 引用", path + "/uri",
                       remediation=[ask_user("改用内联 json 或 b64 编码")])
        # SEC-4: no data root is configured in this server -> every ref is rejected.
        raise Fail("E.VALIDATION.REF_FORBIDDEN", f"{path}：本服务没有配置 data root，拒绝所有 file:// 引用（SEC-4）", path + "/uri",
                   remediation=[ask_user("改用内联 json 或 b64 编码")])
    if dtype not in allowed:
        raise Fail("E.VALIDATION.DTYPE", f"{path}：dtype {dtype} 不被本算子接受（允许 {', '.join(allowed)}）", path + "/dtype",
                   remediation=[ask_user(f"提供 dtype 为 {' / '.join(allowed)} 的数组")])
    count = _shape_product(shape, path)
    if count > max_elements or count * _ITEMSIZE[dtype] > LIMIT_ARRAY_BYTES:
        raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"{path}：{count} 个元素超过上限 {max_elements}", path + "/shape",
                   {"elements": count, "limit": max_elements}, [ask_user("缩小数据规模或分段处理")])
    if "data_b64" in obj:
        try:
            raw = base64.b64decode(obj["data_b64"], validate=True)
        except (binascii.Error, ValueError):
            raise Fail("E.VALIDATION.SCHEMA", f"{path}/data_b64 不是带 padding 的标准 base64（L-2）", path + "/data_b64")
        dig = H.sha256_hex(raw)
        if dig != obj.get("sha256"):
            raise Fail("E.VALIDATION.DIGEST_MISMATCH", f"{path}：sha256 与解码后的数据不符（L-2、SEC-3）", path + "/sha256",
                       {"declared": obj.get("sha256"), "computed": dig}, [ask_user("重新编码数据并计算 sha256")])
        if len(raw) != count * _ITEMSIZE[dtype]:
            raise Fail("E.VALIDATION.SHAPE", f"{path}：解码字节数 {len(raw)} ≠ 元素数 {count} × {_ITEMSIZE[dtype]}（T-3）", path + "/shape",
                       {"bytes": len(raw), "elements": count})
        npdt = {"f64": "<f8", "i64": "<i8", "c128": "<c16", "bool": "u1"}[dtype]
        vals = np.frombuffer(raw, dtype=npdt).copy()
        if dtype == "i64":
            vals = vals.astype(np.float64)
        elif dtype == "bool":
            vals = vals.astype(bool)
        else:
            vals = vals.astype(np.complex128 if dtype == "c128" else np.float64)
            bad = np.flatnonzero(~np.isfinite(vals))
            if bad.size:
                raise Fail("E.VALIDATION.NON_FINITE", f"{path}：第 {int(bad[0])} 个元素不是有限值（R-2）", path,
                           {"index": int(bad[0])}, [ask_user("去掉或替换非有限值")])
        return Array(dtype, list(shape), vals.reshape(shape), dig)
    data = obj.get("data")
    if not isinstance(data, list):
        raise Fail("E.VALIDATION.SCHEMA", f"{path} 缺少 data", path + "/data")
    if len(data) > LIMIT_INLINE_ELEMS and len(data) > max_elements:
        raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"{path}：内联 JSON 数组过大，请改用 b64", path)
    if len(data) != count:
        raise Fail("E.VALIDATION.SHAPE", f"{path}：data 有 {len(data)} 个元素，shape {shape} 要求 {count} 个（T-3）", path + "/shape",
                   {"data_length": len(data), "shape": shape},
                   [ask_user("核对 shape 与 data（row-major 展平）")])
    out = []
    for i, x in enumerate(data):
        ep = f"{path}/data/{i}"
        if dtype == "bool":
            if not isinstance(x, bool):
                raise Fail("E.VALIDATION.DTYPE", f"{ep} 不是布尔值", ep)
            out.append(x)
        elif dtype == "c128":
            if not (isinstance(x, list) and len(x) == 2 and all(isinstance(t, (int, float)) and not isinstance(t, bool) for t in x)):
                if isinstance(x, list) and len(x) == 2 and any(isinstance(t, str) for t in x):
                    raise Fail("E.VALIDATION.NON_FINITE", f"{ep} 不是有限值（R-2）", ep, remediation=[ask_user("去掉或替换非有限值")])
                raise Fail("E.VALIDATION.DTYPE", f"{ep}：c128 元素必须是 [re, im]", ep)
            out.append(complex(float(x[0]), float(x[1])))
        else:
            if isinstance(x, str) and x in _NONFIN:
                raise Fail("E.VALIDATION.NON_FINITE", f"{ep} = {x}：输入默认拒绝非有限值（R-2）", ep,
                           {"index": i}, [ask_user("去掉或替换非有限值（数据缺失时先问用户怎么处理）")])
            if isinstance(x, bool) or not isinstance(x, (int, float)) or isinstance(x, list):
                raise Fail("E.VALIDATION.DTYPE", f"{ep}：{dtype} 元素必须是实数", ep)
            if dtype == "i64" and (isinstance(x, float) and not float(x).is_integer()):
                raise Fail("E.VALIDATION.DTYPE", f"{ep}：i64 元素必须是整数", ep)
            fx = float(x)
            if not math.isfinite(fx):
                raise Fail("E.VALIDATION.NON_FINITE", f"{ep} 不是有限值", ep)
            out.append(fx)
    npdt = {"bool": bool, "c128": np.complex128}.get(dtype, np.float64)
    vals = np.array(out, dtype=npdt).reshape(shape) if count else np.zeros(shape, dtype=npdt)
    return Array(dtype, list(shape), vals, H.array_digest(obj))


def _sparse_to_dense_value(obj):
    try:
        m, n = obj["shape"]
        if m * n > 10000 or obj.get("dtype") != "f64":
            return None
        a = [[0.0] * n for _ in range(m)]
        ip, ix, dv = obj["indptr"], obj["indices"], obj["data"]
        outer = m if obj["format"] == "csr" else n
        for o in range(outer):
            for p in range(ip[o], ip[o + 1]):
                i, j = (o, ix[p]) if obj["format"] == "csr" else (ix[p], o)
                a[i][j] += float(dv[p])
        return {"dtype": "f64", "shape": [m, n], "data": [x for r in a for x in r]}
    except Exception:
        return None


def dense_f64(a):
    a = np.asarray(a, dtype=np.float64)
    return {"dtype": "f64", "shape": list(a.shape), "data": [enc_float(x) for x in a.ravel(order="C")]}


def dense_c128(re, im):
    re = np.asarray(re, dtype=np.float64)
    im = np.asarray(im, dtype=np.float64)
    return {"dtype": "c128", "shape": list(re.shape),
            "data": [[enc_float(a), enc_float(b)] for a, b in zip(re.ravel(order="C"), im.ravel(order="C"))]}


def enc_float(x):
    """R-2 / R-3: non-finite output values use the string encoding."""
    x = float(x)
    if x != x:
        return "NaN"
    if x == math.inf:
        return "Inf"
    if x == -math.inf:
        return "-Inf"
    return x


# --------------------------------------------------------------------------------------------
# hashing (3.7 H-1 .. H-3, 6 P-2)


def _canon(v):
    if isinstance(v, dict):
        if "data_b64" in v and "dtype" in v and "shape" in v:
            try:
                dig = H.sha256_hex(base64.b64decode(v["data_b64"], validate=True))
            except Exception:
                dig = v.get("sha256")
            return {"$array": {"dtype": v["dtype"], "shape": v["shape"], "sparse": False, "sha256": dig}}
        if "data" in v and "dtype" in v and "shape" in v and isinstance(v["data"], list):
            return {"$array": {"dtype": v["dtype"], "shape": v["shape"], "sparse": False, "sha256": H.array_digest(v)}}
        return {k: _canon(x) for k, x in v.items()}
    if isinstance(v, list):
        return [_canon(x) for x in v]
    return v


def input_hash(args):
    a = {k: x for k, x in args.items() if k != "exec"}
    return H.sha256_hex(H.jcs(_canon(a)).encode("utf-8"))


def output_hash(result):
    return H.sha256_hex(H.jcs(_canon(result)).encode("utf-8"))


def to_b64_result(v):
    """L-5: every dense array in the result becomes a DenseMatrixB64; scalars and small structures unchanged."""
    if isinstance(v, dict):
        if "data" in v and "dtype" in v and "shape" in v and isinstance(v["data"], list):
            raw = b"".join(H._elem_bytes(v["dtype"], x) for x in v["data"])
            return {"dtype": v["dtype"], "shape": v["shape"], "order": "C", "byte_order": "little",
                    "data_b64": base64.b64encode(raw).decode("ascii"), "sha256": H.sha256_hex(raw)}
        return {k: to_b64_result(x) for k, x in v.items()}
    if isinstance(v, list):
        return [to_b64_result(x) for x in v]
    return v


# --------------------------------------------------------------------------------------------
# compensated / exact arithmetic used by several operators

_SPLIT = 134217729.0  # 2^27 + 1 (Dekker)


def two_prod(a, b):
    """Error-free product a*b = p + e for numpy arrays, without FMA (Dekker / Veltkamp)."""
    a = np.asarray(a, dtype=np.float64)
    b = np.asarray(b, dtype=np.float64)
    p = a * b
    ca = _SPLIT * a
    ah = ca - (ca - a)
    al = a - ah
    cb = _SPLIT * b
    bh = cb - (cb - b)
    bl = b - bh
    e = al * bl - (((p - ah * bh) - al * bh) - ah * bl)
    return p, e


def dot_exact(a, b):
    """Correctly rounded dot product: exact products (two_prod) summed exactly by math.fsum."""
    p, e = two_prod(a, b)
    return math.fsum(np.concatenate([p.ravel(), e.ravel()]).tolist())


def residual_exact(A, x, b):
    """r = b - A x, each component correctly rounded."""
    m = A.shape[0]
    r = np.empty(m)
    for i in range(m):
        p, e = two_prod(A[i], x)
        r[i] = math.fsum([float(b[i])] + (-p).tolist() + (-e).tolist())
    return r


# --------------------------------------------------------------------------------------------
# operator base class, method policy, envelope

_EXACT_ORDER = {"approximate": 0, "correctly_rounded": 1, "exact": 2}


class Operator:
    """Subclasses set OP_ID, VERSION, MANIFEST and implement prepare(args) -> P, estimate(P), execute(P)."""
    OP_ID = None
    VERSION = None
    MANIFEST = None

    def prepare(self, args):
        raise NotImplementedError

    def estimate(self, prep):
        return {}

    def execute(self, prep):
        raise NotImplementedError

    # ---- helpers
    @property
    def tool_name(self):
        return self.MANIFEST["mcp"]["tool_name"]

    def methods(self):
        r = self.MANIFEST["receipt"]
        out = [{"method_id": r["method_id"], "exactness": r["exactness"], "backend": self.MANIFEST["backend"]}]
        for alt in r.get("alternatives", []):
            out.append(alt)
        return out


def resolve_method(op, policy):
    """3.14 MS-6. Returns the method id to use or raises METHOD_UNAVAILABLE."""
    methods = op.methods()
    cands = list(methods)
    reasons = []
    req = policy.get("require")
    if req is not None:
        unknown = [m for m in req if m not in [x["method_id"] for x in methods]]
        if unknown:
            reasons.append({"require_unregistered": unknown})
        cands = [m for m in cands if m["method_id"] in req]
    cons = policy.get("constraints") or {}
    if "min_exactness" in cons:
        lo = _EXACT_ORDER[cons["min_exactness"]]
        bad = [m["method_id"] for m in cands if _EXACT_ORDER[m["exactness"]["class"]] < lo]
        if bad:
            reasons.append({"min_exactness": cons["min_exactness"], "methods_below": bad})
        cands = [m for m in cands if _EXACT_ORDER[m["exactness"]["class"]] >= lo]
    if "backend_library_in" in cons:
        bad = [m["method_id"] for m in cands if m["backend"]["library"] not in cons["backend_library_in"]]
        if bad:
            reasons.append({"backend_library_in": cons["backend_library_in"], "methods_outside": bad})
        cands = [m for m in cands if m["backend"]["library"] in cons["backend_library_in"]]
    if not cands:
        available = [{"method_id": m["method_id"], "exactness": m["exactness"]["class"], "backend_library": m["backend"]["library"]}
                     for m in methods]
        raise Fail("E.VALIDATION.METHOD_UNAVAILABLE", "exec.method_policy 的硬约束没有任何已登记的方法能满足（MS-6）",
                   "/exec/method_policy", {"reasons": reasons, "available_methods": available},
                   [patch_remedy("drop_method_policy", "去掉 exec.method_policy，使用缺省方法",
                                 [{"op": "remove", "path": "/exec/method_policy"}], confidence="medium", confirm=True),
                    ask_user("策略可能是用户有意设置的；问用户是否放宽方法约束")],
                   agent_hint="不要静默改用其他方法；改策略前先问用户")
    return cands[0]["method_id"]   # default first, then alternatives in manifest order (MS-6)


def _summary_text(env):
    if env["status"] == "error":
        return f"AEOI {env['operator']['id']} -> error {env['error']['code']}: {env['error']['message']}"
    grade = (env.get("trust") or {}).get("accuracy_grade")
    w = ", ".join(x["code"] for x in env.get("warnings", [])) or "none"
    claim = (env.get("receipt") or {}).get("evidence", {}).get("claim")
    return f"AEOI {env['operator']['id']} -> {env['status']} (grade {grade}, claim {claim}); warnings: {w}"


def _error_envelope(op, f, request_id, t0):
    details = dict(f.details)
    if f.path is not None and "path" not in details:
        details["path"] = f.path
    rem = f.remediation if f.remediation else [ask_user("请向用户确认输入与建模假设")]
    return {"spec": SPEC, "status": "error", "dry_run": False,
            "operator": {"id": op.OP_ID, "version": op.VERSION, "build": BUILD},
            "request_id": request_id, "result": None, "trust": None, "receipt": None, "reproducibility": None, "estimate": None,
            "error": {"code": f.code, "category": f.category, "retryable": bool(f.retryable), "message": f.message,
                      "agent_hint": f.agent_hint or "按 remediation 修改输入；不确定时问用户（矩阵为 row-major）",
                      "details": details, "remediation": rem},
            "warnings": [], "timing": {"wall_ms": (time.perf_counter() - t0) * 1000}}


def jsonable(v):
    """Plain JSON types only: numpy scalars -> Python, non-finite floats -> R-2 strings."""
    if isinstance(v, dict):
        return {k: jsonable(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [jsonable(x) for x in v]
    if isinstance(v, (bool, np.bool_)):
        return bool(v)
    if isinstance(v, (int, np.integer)):
        return int(v)
    if isinstance(v, (float, np.floating)):
        return enc_float(v)
    return v


def call_operator(op, args):
    return jsonable(_call_operator(op, args))


def _call_operator(op, args):
    t0 = time.perf_counter()
    exec_ = args.get("exec") if isinstance(args, dict) else None
    request_id = exec_.get("request_id") if isinstance(exec_, dict) and isinstance(exec_.get("request_id"), str) else None
    try:
        if not isinstance(args, dict):
            raise Fail("E.VALIDATION.SCHEMA", "arguments 必须是 JSON 对象", "")
        schema_check(args, op.MANIFEST["input_schema"])
        exec_ = args.get("exec") or {}
        policy = exec_.get("method_policy")
        used = resolve_method(op, policy) if policy is not None else op.MANIFEST["receipt"]["method_id"]
        prep = op.prepare(args)
        if exec_.get("dry_run"):
            return {"spec": SPEC, "status": "ok", "dry_run": True,
                    "operator": {"id": op.OP_ID, "version": op.VERSION, "build": BUILD}, "request_id": request_id,
                    "result": None, "trust": None, "receipt": None, "reproducibility": None, "warnings": [], "error": None,
                    "estimate": op.estimate(prep), "timing": {"wall_ms": (time.perf_counter() - t0) * 1000}}
        result, trust, warnings = op.execute(prep)
    except Fail as f:
        return _error_envelope(op, f, request_id, t0)
    except Exception as e:  # X-3 analogue: never let an exception escape as a protocol error
        f = Fail("E.INTERNAL.PANIC", f"内部错误：{type(e).__name__}: {e}", None, retryable=False,
                 remediation=[{"id": "retry", "kind": "retry_same", "description": "原样重试一次；仍失败请报告实现缺陷",
                               "confidence": "low", "requires_user_confirmation": False}])
        return _error_envelope(op, f, request_id, t0)

    man = op.MANIFEST
    rec = man["receipt"]
    checks = trust.get("checks", {})
    posts = rec["postconditions"]
    failed = [p for p in posts if checks.get(p) is not True]
    coverage = rec.get("coverage")
    if not posts:
        claim = "unchecked"
    elif failed:
        claim = "unverified"
    elif coverage and any(coverage.get(p, "full") == "sampled" for p in posts):
        claim = "spot_checked"
    else:
        claim = "verified"
    unreliable = sorted({ptr for p in failed for ptr in (rec.get("guards") or {}).get(p, [])})
    warnings = list(warnings)
    if failed:   # E-11 framework warning (E-7a)
        w = {"code": "W.EVIDENCE.POSTCONDITION_FAILED",
             "message": f"后置条件不成立：{', '.join(failed)}" + (f"；不可依赖的字段：{', '.join(unreliable)}" if unreliable else ""),
             "path": "", "details": {"failed": failed}}
        if unreliable:
            w["details"]["unreliable"] = unreliable
        warnings.append(w)
    declared = set(man.get("warnings", [])) | {"W.EVIDENCE.POSTCONDITION_FAILED"}
    for w in warnings:
        if w["code"] not in declared:
            raise RuntimeError(f"undeclared warning {w['code']}")

    in_hash = input_hash(args)
    out_hash = output_hash(result)
    req_fp = H.request_fingerprint(op.OP_ID, op.VERSION, in_hash)
    res_fp = H.result_fingerprint(req_fp, out_hash)
    ee = trust.get("error_estimate") or {}
    bound = {k: v for k, v in ee.items() if isinstance(v, (int, float)) and not isinstance(v, bool)
             and not k.startswith(("info_", "worst_"))}
    bk = rec["exactness"]["bound_kind"]
    exactness = {"class": rec["exactness"]["class"], "bound_kind": bk if (bound or bk == "none") else "none_reported",
                 "bound": bound or None, "basis": ee.get("basis")}
    method = next(m for m in op.methods() if m["method_id"] == used)
    evidence = {"claim": claim, "postconditions": {p: checks.get(p) for p in posts}, "failed": failed,
                "convergence": ({k: trust["convergence"][k] for k in ("converged", "iterations") if k in trust["convergence"]}
                                if trust["convergence"].get("applicable") else None),
                "accuracy_grade": trust["accuracy_grade"], "tolerance_class": man["tolerance_class"], "credentials": None}
    if coverage:
        evidence["coverage"] = {p: coverage.get(p, "full") for p in posts}
    if unreliable:
        evidence["unreliable"] = unreliable
    receipt = {
        "format": "aeoi-receipt/1",
        "method": {"id": used, "routine": method["backend"].get("routine"),
                   "provenance": f"aeoi://operators/{op.OP_ID}/{op.VERSION}/manifest#/provenance"},
        "assumptions": rec["assumptions"],
        "exactness": exactness,
        "evidence": evidence,
        "fingerprint": {"request": req_fp, "result": res_fp, "output_hash": out_hash,
                        "scheme": "request = sha256(JCS({operator, version, input_hash})); result = sha256(JCS({request, output_hash}))",
                        "stable_across": "request: all implementations; result: same build + platform only"},
        "replay": {"build": BUILD, "platform": PLATFORM, "libm": LIBM, "verify": {}},
    }
    if isinstance(args.get("exec"), dict) and "method_policy" in args["exec"]:
        receipt["method_selection"] = {"policy": args["exec"]["method_policy"], "used": used}
    if (args.get("exec") or {}).get("output_encoding") == "b64":
        result = to_b64_result(result)
    return {"spec": SPEC, "status": "ok_with_warnings" if warnings else "ok", "dry_run": False,
            "operator": {"id": op.OP_ID, "version": op.VERSION, "build": BUILD}, "request_id": request_id,
            "result": result, "trust": trust, "warnings": warnings, "error": None, "estimate": None,
            "reproducibility": {"determinism": man["determinism"], "tolerance_class": man["tolerance_class"],
                                "input_hash": in_hash, "output_hash": out_hash, "platform": PLATFORM,
                                "cpu_features": CPU_FEATURES, "libm": LIBM,
                                "blas": man.get("x-pyscipy-blas", "none (no BLAS reductions on the result path)"),
                                "threads": 1, "seed": None},
            "receipt": receipt, "timing": {"wall_ms": (time.perf_counter() - t0) * 1000}}


def mcp_tool_entry(op):
    m = op.MANIFEST
    grade = m.get("accuracy_grade", {}).get("current", "U")
    return {"name": op.tool_name, "title": m["title"],
            "description": f"[{op.OP_ID}] {m['description']}（精度等级 {grade}，容差类 {m['tolerance_class']}，确定性 {m['determinism']}）",
            "inputSchema": m["input_schema"], "outputSchema": envelope_output_schema(m["output_schema"]),
            "annotations": m["mcp"]["annotations"],
            "_meta": {"aeoi/id": op.OP_ID, "aeoi/version": op.VERSION, "aeoi/grade": grade, "aeoi/spec": SPEC,
                      "aeoi/manifest": f"aeoi://operators/{op.OP_ID}/{op.VERSION}/manifest"}}


def summary_content(env):
    return [{"type": "text", "text": _summary_text(env)}, {"type": "text", "text": json.dumps(env, ensure_ascii=False)}]


ANNOTATIONS = {"readOnlyHint": True, "idempotentHint": True, "openWorldHint": False}
CLEAN_ROOM = {"declared": True, "sources": "spec-v0.4.md、profiles/ 与公开文献；未阅读参考实现（rust/）与 golden 生成脚本"}


def base_manifest(op_id, version, **fields):
    inp, out = load_io_schemas(op_id)
    m = {"spec": SPEC, "id": op_id, "version": version}
    m.update(fields)
    m["input_schema"] = inp
    m["output_schema"] = out
    m["mcp"] = {"tool_name": op_id.replace(".", "_"), "annotations": ANNOTATIONS}
    m["provenance"]["clean_room"] = CLEAN_ROOM
    return m
