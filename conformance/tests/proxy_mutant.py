"""MCP stdio proxy that rewrites one operator's results, chosen by $AEOI_MUTANT (see MUTANTS below).
Wraps the reference implementation ($AEOI_IMPL, default rust/target/release/aeoi-mcp).

Two kinds of mutant, both used by test_runner_catches_mutants.py:
  * "wrong" mutants: a small numeric error above the suite tolerance; the runner must fail them;
  * "equivalent" mutants: a different but equally correct answer (another pole order, another state-space
    realization, a tf with num and den scaled by the same factor); the runner must pass them, otherwise the
    rules would reject a legitimate second implementation.
output_hash and the result fingerprint are recomputed, so only the numbers differ from a real answer."""
import json
import os
import subprocess
import sys
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(HERE, ".."))
import aeoi_hash as H  # noqa: E402
import numpy as np  # noqa: E402


def _data(o):
    return o["value"]["data"] if "value" in o else o["data"]


def _ss_similarity(sysd):
    """A' = T⁻¹AT, B' = T⁻¹B, C' = CT with a fixed, well-conditioned T (same transfer function)."""
    A = np.array(sysd["A"]["data"], dtype=float).reshape(sysd["A"]["shape"])
    B = np.array(sysd["B"]["data"], dtype=float).reshape(sysd["B"]["shape"])
    C = np.array(sysd["C"]["data"], dtype=float).reshape(sysd["C"]["shape"])
    n = A.shape[0]
    T = np.eye(n) + 0.3 * np.triu(np.ones((n, n)), 1) + np.diag(np.linspace(0.5, 1.5, n)) - np.eye(n)
    Ti = np.linalg.inv(T)
    for k, M in (("A", Ti @ A @ T), ("B", Ti @ B), ("C", C @ T)):
        sysd[k]["data"] = [float(x) for x in M.ravel()]


def _reverse_c128(o):
    d = o["data"]
    d.reverse()


def _bump(lst, i, rel):
    lst[i] = lst[i] * (1 + rel) if lst[i] != 0 else rel


def m_pz_pole_wrong(r):                     # one pole moved by 1e-6 relative
    _bump(r["poles"]["data"][0], 0, 1e-6)


def m_pz_reordered(r):                      # same sets, other order; per-pole attributes follow their pole
    _reverse_c128(r["poles"]); _reverse_c128(r["zeros"])
    _data(r["natural_frequency"]).reverse(); r["damping_ratio"]["data"].reverse()


def m_bode_mag_wrong(r):                    # 1e-6 dB on one point
    _data(r["magnitude"])[0] += 1e-6


def m_bode_phase_360(r):                    # every phase + 360 deg: equal modulo 360, wrong branch (10.x)
    d = _data(r["phase"])
    d[:] = [x + 360.0 for x in d]


def m_step_y_wrong(r):                      # y scaled by 1 + 1e-8
    d = _data(r["y"])
    d[:] = [x * (1 + 1e-8) for x in d]


def m_lqr_S_wrong(r):                       # symmetric relative error 1e-7 in S
    d = r["S"]["data"]
    d[:] = [x * (1 + 1e-7) for x in d]


def m_lqr_poles_reordered(r):
    _reverse_c128(r["closed_loop_poles"])


def m_c2d_A_wrong(r):                       # 1e-6 relative on A[0,0] (ss) or on num[0] (tf)
    s = r["system"]
    if s["kind"] == "ss":
        _bump(s["A"]["data"], 0, 1e-6)
    else:
        _bump(s["num"], 0, 1e-6)


def m_c2d_equivalent(r):                    # similarity transform (ss) or num, den × 3 (tf)
    s = r["system"]
    if s["kind"] == "ss":
        _ss_similarity(s)
    else:
        s["num"] = [3.0 * x for x in s["num"]]; s["den"] = [3.0 * x for x in s["den"]]


def m_lsim_y_wrong(r):                      # y scaled by 1 + 1e-8
    d = _data(r["y"])
    d[:] = [x * (1 + 1e-8) for x in d]


def m_lsim_zoh_as_foh(r):                   # claims the other interpolation (result otherwise unchanged)
    if r.get("interpolation") in ("foh", "zoh"):
        r["interpolation"] = "zoh" if r["interpolation"] == "foh" else "foh"


# name -> (operator, transform)
MUTANTS = {
    "pz_pole_wrong": ("control.lti.poles_zeros", m_pz_pole_wrong),
    "pz_reordered": ("control.lti.poles_zeros", m_pz_reordered),
    "bode_mag_wrong": ("control.lti.bode", m_bode_mag_wrong),
    "bode_phase_360": ("control.lti.bode", m_bode_phase_360),
    "step_y_wrong": ("control.lti.step", m_step_y_wrong),
    "lqr_S_wrong": ("control.design.lqr", m_lqr_S_wrong),
    "lqr_poles_reordered": ("control.design.lqr", m_lqr_poles_reordered),
    "c2d_A_wrong": ("control.lti.c2d", m_c2d_A_wrong),
    "c2d_equivalent": ("control.lti.c2d", m_c2d_equivalent),
    "lsim_y_wrong": ("control.lti.lsim", m_lsim_y_wrong),
    "lsim_interp_mislabeled": ("control.lti.lsim", m_lsim_zoh_as_foh),
}


def rewrite(line, op, f):
    try:
        msg = json.loads(line)
    except ValueError:
        return line
    res = msg.get("result") if isinstance(msg, dict) else None
    env = res.get("structuredContent") if isinstance(res, dict) else None
    if not (isinstance(env, dict) and env.get("status") in ("ok", "ok_with_warnings") and (env.get("operator") or {}).get("id") == op):
        return line
    f(env["result"])
    oh = H.output_hash(env["result"])
    env["reproducibility"]["output_hash"] = oh
    fp = (env.get("receipt") or {}).get("fingerprint")
    if fp:
        fp["output_hash"] = oh
        fp["result"] = H.result_fingerprint(fp["request"], oh)
    for c in res.get("content", []):
        if c.get("type") == "text":
            try:
                if "status" in json.loads(c["text"]):
                    c["text"] = json.dumps(env, ensure_ascii=False)
            except (ValueError, TypeError):
                pass
    return json.dumps(msg, ensure_ascii=False) + "\n"


def main():
    op, f = MUTANTS[os.environ["AEOI_MUTANT"]]
    impl = os.environ.get("AEOI_IMPL", os.path.join(ROOT, "rust", "target", "release", "aeoi-mcp"))
    p = subprocess.Popen([impl], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)

    def pump_in():
        for line in sys.stdin:
            p.stdin.write(line); p.stdin.flush()
        p.stdin.close()
    threading.Thread(target=pump_in, daemon=True).start()
    for line in p.stdout:
        sys.stdout.write(rewrite(line, op, f)); sys.stdout.flush()


if __name__ == "__main__":
    main()
