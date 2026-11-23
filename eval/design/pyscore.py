#!/usr/bin/env python3
"""Independent python-control / scipy scorer for the design tasks (cross-check of the Rust scorer).

  pyscore.py reference              score every vendored reference solution
  pyscore.py report <design-report> re-score the submissions in a design report, compare verdicts
                                    (also accepts aeoi-web design-score output merged with the answers)

Reference values come from python-control and scipy, not MATLAB.
"""
import json, math, sys
from pathlib import Path
import numpy as np
import control as ctrl
from scipy.signal import freqz

HERE = Path(__file__).resolve().parent


def _cl(L):
    return ctrl.feedback(L, 1)


def _stable(T, thr=0.0):
    return bool(np.all(np.real(ctrl.poles(T)) < thr))


def _settle(T, tfinal, band):
    t = np.linspace(0, tfinal, 200001)
    _, y = ctrl.step_response(T, t)
    yf = float(np.real(ctrl.dcgain(T)))
    out = np.abs(y - yf) > band * abs(yf)
    ts = 0.0 if not out.any() else float(t[min(np.where(out)[0][-1] + 1, len(t) - 1)])
    mp = max(0.0, 100 * (y.max() - yf) / yf) if yf else float("inf")
    return ts, mp, yf


def _margins(L):
    gm, pm, _, _ = ctrl.margin(L)
    gm_db = float("inf") if gm is None or not np.isfinite(gm) else 20 * math.log10(gm)
    return gm_db, (float("inf") if pm is None or not np.isfinite(pm) else float(pm))


def pid_unity(tid, a):
    Kp, Ki, Kd = a["Kp"], a["Ki"], a["Kd"]
    G = ctrl.tf([1], [1, 10, 20]) if tid == "XG_13" else ctrl.tf([1], np.polymul(np.polymul([0.5, 1], [1, 1]), [2, 1]))
    C = ctrl.tf([Kd, Kp, Ki], [1, 0])
    T = _cl(C * G)
    d = {"stable": _stable(T)}
    if not d["stable"]:
        return False, d
    if tid == "XG_13":
        info = ctrl.step_info(T, T=10)
        d.update(settling_time=info["SettlingTime"], overshoot=info["Overshoot"])
        ok = info["SettlingTime"] < 0.2 and info["Overshoot"] < 5
        d["upstream_literal_pass"] = info["SettlingTime"] < 2 and info["Overshoot"] < 0.05
        return ok, d
    ts, mp, yf = _settle(T, 30, 0.05)
    gm, pm = _margins(C * G)
    d.update(settling_time=ts, overshoot=mp, ess=1 - yf, gm_db=gm, pm=pm)
    return ts < 5 and mp < 20 and abs(1 - yf) < 1e-3 and gm > 10 and pm > 45, d


def fopdt_controller(tid, a):
    n, dd = ctrl.pade(a["theta"], 10)
    G = ctrl.tf(n, dd) * ctrl.tf([1], [a["tau"], 1])
    L = G * ctrl.tf(a["num"], a["den"])
    T = _cl(L)
    d = {"theta_ok": abs(a["theta"] - 16.5) <= 2, "tau_ok": abs(a["tau"] - 19.5) <= 2, "stable": _stable(T)}
    if not d["stable"]:
        return False, d
    info = ctrl.step_info(T, T=2000)
    gm, pm = _margins(L)
    d.update(overshoot=info["Overshoot"], settling_time=info["SettlingTime"], gm_db=gm, pm=pm)
    return d["theta_ok"] and d["tau_ok"] and info["Overshoot"] <= 10 and info["SettlingTime"] <= 150 and gm > 7 and pm > 60, d


def _xg_plant():
    return ctrl.tf([1], [1, 0]) * ctrl.tf([0.5 * 169], [1, 2 * 0.01 * 13, 169])


def _kgki(wL):
    G = _xg_plant()
    kg = 1 / abs(G(1j * wL))
    bb = math.sqrt(10)
    return kg * ctrl.tf([bb, wL], [1, 0]) / math.sqrt(bb ** 2 + 1)


def _coef_ok(a, wL, kn="C_num", kd="C_den"):
    C = _kgki(wL)
    num, den = np.squeeze(C.num[0][0]), np.squeeze(C.den[0][0])
    try:
        return float(np.linalg.norm(num - np.array(a[kn], float))) < 0.1, float(np.linalg.norm(den - np.array(a[kd], float))) < 0.1
    except ValueError:
        return False, False


def _allmargin_max(L):
    gm, pm, sm, wpc, wgc, wms = ctrl.stability_margins(L, returnall=True)
    gm = [20 * math.log10(g) for g in np.atleast_1d(gm) if g > 0 and np.isfinite(g)]
    pm = [p for p in np.atleast_1d(pm) if np.isfinite(p)]
    return (max(gm) if gm else float("inf")), (max(pm) if pm else float("inf"))


def loopshape(tid, a):
    wL = 1.0 if tid == "XG_11" else 3.0
    nok, dok = _coef_ok(a, wL)
    if tid == "XG_11":
        p = a["beta_r"] * a["omega_r"]
        Fr = ctrl.tf([p * p], [1, math.sqrt(2) * p, p * p])
        K = Fr / abs(Fr(1j * a["omega_r"]))
        gmin, pmin = 3, 50
    else:
        w, al, f = a["omega_n"], a["alpha_n"], a["f_n"]
        K = ctrl.tf([1, f * w * math.sqrt(al), w * w], [1, f * w / math.sqrt(al), w * w])
        gmin, pmin = 2, 60
    L = _xg_plant() * _kgki(wL) * K
    d = {"num_ok": nok, "den_ok": dok, "stable": _stable(_cl(L))}
    if not d["stable"]:
        return False, d
    gm, pm = _allmargin_max(L)
    d.update(gm_db_max=gm, pm_max=pm)
    return nok and dok and gm >= gmin and pm >= pmin, d


XG02_A = np.array([
    [-3360.27884342382, 24650.4407238876, -24650.4407238876, 24650.4407238876, -24650.4407238876, 24650.4407238876, -6162.61018097190],
    [-3270.26403683917, 10278.7248098086, -7476.39347226054, 7476.39347226054, -7476.39347226054, 7476.39347226054, -1869.09836806513],
    [-2657.10970968037, 10628.4388387215, -13430.7701762696, 16233.1015138177, -16233.1015138177, 16233.1015138177, -4058.27537845441],
    [-3954.03177459846, 15816.1270983939, -15816.1270983939, 13013.7957608458, -10211.4644232977, 10211.4644232977, -2552.86610582443],
    [-2325.45756017668, 9301.83024070671, -9301.83024070671, 9301.83024070671, -12104.1615782548, 14906.4929158029, -3726.62322895072],
    [-1665.81075466004, 6663.24301864017, -6663.24301864017, 6663.24301864017, -6663.24301864017, 3860.91168109210, -264.645085886005],
    [-1596.51916299641, 6386.07665198562, -6386.07665198562, 6386.07665198562, -6386.07665198562, 6386.07665198562, -4398.85050054448]])
XG02_B = np.array([[33.1501], [29.2147], [49.7430], [55.4071], [43.2351], [22.8726], [24.9153]])
XG02_C = np.array([[41.1585, -164.6342, 164.6342, -164.6342, 164.6342, -164.6342, 41.1585]])


def loopshape_bandwidth(tid, a):
    G = ctrl.ss2tf(ctrl.ss(XG02_A, XG02_B, XG02_C, [[0]]))
    L = G * ctrl.tf(a["num"], a["den"])
    d = {"stable": _stable(_cl(L))}
    if not d["stable"]:
        return False, d
    w = np.logspace(0, 4, 1000)
    mag = np.abs(L(1j * w))
    db = 20 * np.log10(mag)
    idx = np.where(np.diff(np.sign(db)) != 0)[0]
    bw = 0.0
    if len(idx):
        i = idx[0]
        bw = (w[i] + (w[i + 1] - w[i]) * (-db[i]) / (db[i + 1] - db[i])) / (2 * math.pi)
    gm, pm = _margins(L)
    d.update(bandwidth_hz=bw, gm_db=gm, pm=pm)
    return abs(bw - 85) <= 10 and gm > 1.5 and pm > 60, d


def maglev(tid, a):
    ref = dict(a11=0, a12=1, a21=2, a22=0, b11=0, b21=-2, k1=-2, k2=-1, l1=11, l2=32, s1=1, s2=0, s3=1, s4=0)
    pts = {"A": (["a11", "a12", "a21", "a22"], 8), "B": (["b11", "b21"], 8), "K": (["k1", "k2"], 10), "L": (["l1", "l2"], 10)}
    d, sc = {}, 0
    for k, (names, w) in pts.items():
        d[k] = all(abs(float(a.get(n, math.nan)) - ref[n]) <= 1e-6 for n in names)
        sc += w * d[k]
    for i in range(1, 5):
        d[f"s{i}"] = int(a.get(f"s{i}", -1)) == ref[f"s{i}"]
        sc += 16 * d[f"s{i}"]
    d["score"] = sc
    return sc == 100, d


def _mag_db(b, a_, f, fs):
    w, h = freqz(b, a_, worN=10000, fs=fs)
    return 20 * np.log10(np.abs(np.interp(f, w, np.abs(h))))


def iir_notch(tid, a):
    b, den, order = a["coeffs_numerator"], a["coeffs_denominator"], a["order"]
    fs = 2000
    lo = _mag_db(b, den, np.linspace(0, 115, 116), fs)
    hi = _mag_db(b, den, np.linspace(125, 250, 126), fs)
    tg = _mag_db(b, den, np.array([120.0]), fs)
    d = {"lower_ok": bool(np.all(lo >= -3)), "upper_ok": bool(np.all(hi >= -3)), "target_ok": bool(tg[0] <= -40),
         "order_ok": order <= 50, "coeff_ok": len(b) == order + 1 and len(den) == order + 1}
    wts = dict(lower_ok=0.2, upper_ok=0.2, target_ok=0.4, order_ok=0.1, coeff_ok=0.1)
    d["score"] = 100 * sum(wts[k] for k in wts if d[k])
    return all(d[k] for k in wts), d


def fir_antialias(tid, a):
    c, order = a["coeffs"], a["order"]
    fs = 48000
    pb = _mag_db(c, [1], np.linspace(0, 3500, 3500), fs)
    sb = _mag_db(c, [1], np.linspace(4000, 10000, 6000), fs)
    tb = _mag_db(c, [1], np.linspace(3500, 4000, 20), fs)
    d = {"pass_ok": bool(np.all(pb >= -3)), "stop_ok": bool(np.all(sb[1:] <= -40)), "trans_ok": bool(np.all(tb[1:] <= 0)),
         "order_ok": order <= 350, "ord_coeff_ok": len(c) == order + 1, "stpbnd_ok": 3500 <= a["stpbnd"] <= 4000,
         "decim_ok": a["decim"] == 6}
    wts = dict(pass_ok=0.2, stop_ok=0.2, trans_ok=0.1, order_ok=0.1, ord_coeff_ok=0.1, stpbnd_ok=0.2, decim_ok=0.1)
    d["score"] = 100 * sum(wts[k] for k in wts if d[k])
    return all(d[k] for k in wts), d


def alpha_range(tid, a):
    return 0.651 <= a["alpha"] <= 1.087, {"alpha": a["alpha"]}


def controleval(task, a):
    G = ctrl.tf(task["num"], task["den"])
    if task.get("tau"):
        n, dd = ctrl.pade(task["tau"], 5)
        G = G * ctrl.tf(n, dd)
    L = G * ctrl.tf(a["num"], a["den"])
    T = _cl(L)
    d = {"stable": _stable(T, -0.01)}
    if not d["stable"]:
        return False, d
    _, pm = _margins(L)
    info = ctrl.step_info(T, T=500.0)
    dc = ctrl.dcgain(L)
    sse = 0.0 if np.isinf(abs(dc)) else 1.0 if np.isnan(dc) else float(1 / (1 + abs(dc)))
    d.update(pm=pm, settling_time=info["SettlingTime"], sse=sse)
    return (pm >= task["phase_margin_min"] and task["settling_time_min"] <= info["SettlingTime"] <= task["settling_time_max"]
            and sse <= task["steadystate_error_max"]), d


SCORERS = {"pid_unity": pid_unity, "fopdt_controller": fopdt_controller, "loopshape_rolloff": loopshape,
           "loopshape_notch": loopshape, "loopshape_bandwidth": loopshape_bandwidth, "maglev_observer": maglev,
           "iir_notch": iir_notch, "fir_antialias": fir_antialias, "alpha_range": alpha_range}

REFERENCE = {
    "XG_13": {"Kp": 350, "Ki": 300, "Kd": 50},
    "Ziheng_03": {"Kp": 10, "Ki": 1.55, "Kd": 9.5},
    "XG_05": {"theta": 16.5, "tau": 19.5, "num": [23.24, 0.9], "den": [25.82, 0]},
    "XG_11": {"C_num": [6.2871, 1.9882], "C_den": [3.3166, 0], "omega_r": 1.0, "beta_r": 4.7},
    "XG_12": {"C_num": [17.96, 17.04], "C_den": [3.3166, 0], "omega_n": 13, "alpha_n": 0.04, "f_n": 0.1},
    "XG_02": {"num": [1.4493e+03, 5.0291e+05, 8.1368e+09, 2.5012e+12], "den": [1, 1.8372e+03, 6.8929e+06, 2.4391e+09, -7.0209e+11]},
    "LX_02": dict(a11=0, a12=1, a21=2, a22=0, b11=0, b21=-2, k1=-2, k2=-1, l1=11, l2=32, s1=1, s2=0, s3=1, s4=0),
    "AV_02": {"order": 8, "coeffs_numerator": [0.971987554400379, -7.23074141988472, 24.0593587574556, -46.7018447646062, 57.8028554708087, -46.7018447646062, 24.0593587574556, -7.23074141988472, 0.971987554400379],
              "coeffs_denominator": [1, -7.38629070277517, 24.4025053599328, -47.0320991049601, 57.7993568733735, -46.3686717935595, 23.7189260553057, -7.07811076768760, 0.944759805909239]},
    "ZC_02": {"alpha": 1.087},
}


def _av03_reference(tasks):
    import re
    sol = tasks["engdesign/AV_03"]["reference_solution"]
    c = [float(x) for x in re.findall(r"-?\d+\.\d+(?:e-?\d+)?", sol.split("coeffs =")[1].split("]")[0])]
    return {"order": len(c) - 1, "coeffs": c, "stpbnd": 3900, "decim": 6}


def score(task, a):
    tid = task["source"]["task"] if "source" in task and task["source"].get("benchmark") == "EngDesign" else None
    try:
        if tid:
            ok, d = SCORERS[task["scorer"]](tid, a)
        else:
            ok, d = controleval(task["controleval"], a)
        return {"pass": bool(ok), "details": json.loads(json.dumps(d, default=float))}
    except Exception as e:  # malformed submission
        return {"pass": False, "details": {"error": str(e)}}


def main():
    tasks = {t["id"]: t for t in json.load(open(HERE / "engdesign.json"))["tasks"]}
    if sys.argv[1] == "reference":
        REFERENCE["AV_03"] = _av03_reference(tasks)
        for tid, a in REFERENCE.items():
            r = score(tasks[f"engdesign/{tid}"], a)
            print(f"{tid:<10} pass={r['pass']}  {r['details']}")
    elif sys.argv[1] == "report":
        rep = json.load(open(sys.argv[2]))
        bad = 0
        rows = rep["results"] if isinstance(rep, dict) else rep
        for r in rows:
            r.setdefault("submission", r.get("answer"))
            if r.get("submission") is None:
                continue
            t = tasks.get(r["id"]) or {"controleval": r["controleval_task"]}
            p = score(t, r["submission"])
            agree = p["pass"] == r["pass"]
            bad += not agree
            print(f"{r['id']:<28} rust={r['pass']!s:<5} python-control={p['pass']!s:<5} {'' if agree else 'DISAGREE ' + json.dumps(p['details'])}")
        sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
