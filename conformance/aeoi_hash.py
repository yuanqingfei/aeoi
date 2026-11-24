"""Independent re-implementation of AEOI request hashing (spec 3.7 H-1, 4.3 R-3, 6 P-2).

Written from the spec text and the RFC 8785 rules, not by porting the Rust code, so that a mismatch
means the spec is ambiguous or one side is wrong. Scope of v0.1: dense arrays (json encoding) only;
b64 / ref / sparse arrays are left as-is, so `input_hash` is not reproduced for requests that use them.
"""
import hashlib
import json
import struct
from decimal import Decimal


def es_number(x):
    """ECMAScript Number::toString for a double (RFC 8785 section 3.2.2.3)."""
    x = float(x)
    if x != x or x in (float("inf"), float("-inf")):
        raise ValueError("non-finite number cannot be canonicalised")
    if x == 0:
        return "0"
    sign = "-" if x < 0 else ""
    d = Decimal(repr(abs(x)))
    digits = "".join(map(str, d.as_tuple().digits)).rstrip("0") or "0"
    n = len("".join(map(str, d.as_tuple().digits))) + d.as_tuple().exponent  # position of the decimal point
    k = len(digits)
    if k <= n <= 21:
        body = digits + "0" * (n - k)
    elif 0 < n <= 21:
        body = digits[:n] + "." + digits[n:]
    elif -6 < n <= 0:
        body = "0." + "0" * (-n) + digits
    else:
        e = n - 1
        es = ("+" if e >= 0 else "-") + str(abs(e))
        body = digits[0] + ("." + digits[1:] if k > 1 else "") + "e" + es
    return sign + body


def jcs(v):
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, (int, float)):
        return es_number(v)
    if isinstance(v, str):
        return json.dumps(v, ensure_ascii=False)
    if isinstance(v, list):
        return "[" + ",".join(jcs(x) for x in v) + "]"
    if isinstance(v, dict):
        keys = sorted(v.keys(), key=lambda s: s.encode("utf-16-be"))
        return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + jcs(v[k]) for k in keys) + "}"
    raise TypeError(type(v))


def sha256_hex(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


_NONFINITE = {"NaN": float("nan"), "Inf": float("inf"), "-Inf": float("-inf")}


def _elem_bytes(dtype, x):
    if dtype == "f64":
        return struct.pack("<d", _NONFINITE.get(x, x) if isinstance(x, str) else float(x))
    if dtype == "c128":
        re, im = x
        return struct.pack("<dd", float(re), float(im))
    if dtype == "i64":
        return struct.pack("<q", int(x))
    if dtype == "bool":
        return b"\x01" if x else b"\x00"
    raise ValueError(dtype)


def array_digest(obj):
    return sha256_hex(b"".join(_elem_bytes(obj["dtype"], x) for x in obj["data"]))


def _is_dense_json_array(o):
    return isinstance(o, dict) and "data" in o and "shape" in o and "dtype" in o and isinstance(o["data"], list)


def canonicalize_for_hash(v):
    if _is_dense_json_array(v):
        return {"$array": {"dtype": v["dtype"], "shape": v["shape"], "sparse": False, "sha256": array_digest(v)}}
    if isinstance(v, dict):
        return {k: canonicalize_for_hash(x) for k, x in v.items()}
    if isinstance(v, list):
        return [canonicalize_for_hash(x) for x in v]
    return v


def has_unsupported_arrays(v):
    """True if the value holds b64 / ref / sparse arrays that this module does not hash."""
    if isinstance(v, dict):
        if "data_b64" in v or "uri" in v or ("indptr" in v and "indices" in v):
            return True
        return any(has_unsupported_arrays(x) for x in v.values())
    if isinstance(v, list):
        return any(has_unsupported_arrays(x) for x in v)
    return False


def input_hash(arguments):
    args = {k: x for k, x in arguments.items() if k != "exec"}
    return sha256_hex(jcs(canonicalize_for_hash(args)).encode("utf-8"))


def output_hash(result):
    return sha256_hex(jcs(canonicalize_for_hash(result)).encode("utf-8"))


def request_fingerprint(operator, version, in_hash):
    return sha256_hex(jcs({"operator": operator, "version": version, "input_hash": in_hash}).encode("utf-8"))


def result_fingerprint(req_fp, out_hash):
    return sha256_hex(jcs({"request": req_fp, "output_hash": out_hash}).encode("utf-8"))
