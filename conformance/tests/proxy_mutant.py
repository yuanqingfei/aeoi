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


def m_pidtune_kp_wrong(r):                  # reported Kp off by 1e-6 (controller untouched)
    r["gains"]["Kp"] *= 1 + 1e-6


def m_pidtune_controller_wrong(r):          # controller numerator scaled by 1.001, gains untouched
    r["controller"]["num"] = [x * 1.001 for x in r["controller"]["num"]]


def m_fft_bin_wrong(r):                     # one bin off by 1e-9 relative to the largest bin
    d = r["X"]["data"]
    big = max(abs(complex(*z)) for z in d) or 1.0
    d[len(d) // 3][0] += 1e-9 * big


def m_fft_shifted_frequencies(r):           # frequencies relabelled in fftshift order (the classic mistake)
    f = r["frequency"]["value"]["data"]
    if r.get("sides") == "two" and len(f) > 2:
        f[:] = sorted(f)                    # ascending = fftshift order for the numpy fftfreq labels


def m_fft_scale_wrong(r):                   # X scaled by 1/n (another normalization than the one echoed)
    n = r["n"]
    for z in r["X"]["data"]:
        z[0] /= n; z[1] /= n


def m_ode_y_wrong(r):                       # one output value off by 100 × its own tolerance (atol + rtol·|y|)
    tol = r["tolerance"]; d = r["y"]["data"]; i = len(d) // 2
    d[i] += 100 * (tol["atol"] + tol["rtol"] * abs(d[i]))


def m_ode_states_reordered(r):              # state labels reversed, rows of y untouched (rows no longer match the names)
    if len(r["states"]) > 1:
        r["states"] = list(reversed(r["states"]))


def m_ode_overclaims(r, env):               # always claims tolerance_consistent, whatever the estimate said
    env["trust"]["checks"]["tolerance_consistent"] = True


def m_linsolve_x_wrong(r):                 # one solution component off by 1e-9 (relative)
    d = r["x"]["data"]; d[0] = d[0] * (1 + 1e-9) + 1e-9


def m_linsolve_overclaims(r, env):          # always claims a backward-stable solution
    env["trust"]["checks"]["backward_stable"] = True


def m_inv_transposed(r):                   # returns Xᵀ (a row/column-major mix-up); exact only for symmetric A
    n = r["X"]["shape"][0]; d = r["X"]["data"]
    r["X"]["data"] = [d[j * n + i] for i in range(n) for j in range(n)]


def m_inv_overclaims(r, env):               # always claims the result has correct digits
    env["trust"]["checks"]["forward_error_below_one"] = True


def m_det_sign_flipped(r):                  # sign of det reversed (det negated, |det| unchanged)
    if isinstance(r["det"], (int, float)):
        r["det"] = -r["det"]
    r["sign"] = -r["sign"]


def m_det_never_zero(r):                    # never admits that det is indistinguishable from 0
    r["zero_within_error"] = False


def m_det_log_wrong(r):                     # log|det| off by 1e-9 (absolute)
    if isinstance(r["log_abs_det"], float):
        r["log_abs_det"] += 1e-9


def m_rank_off_by_one(r):                  # reports one more than the numerical rank
    r["rank"] = r["rank"] + 1


def m_rank_overclaims(r, env):              # never admits an ambiguous rank
    env["trust"]["checks"]["rank_well_separated"] = True


def m_basis_null_scaled(r):                 # null-space basis no longer orthonormal (columns scaled by 1 + 1e-6)
    r["null"]["data"] = [v * (1 + 1e-6) for v in r["null"]["data"]]


def m_pinv_scaled(r):                       # pseudoinverse off by a factor 1 + 1e-9
    r["X"]["data"] = [v * (1 + 1e-9) for v in r["X"]["data"]]


def m_norm_rel_1e12(r):                     # norm off by 1e-12 (relative): beyond a compensated sum's accuracy
    r["norm"] = r["norm"] * (1 + 1e-12)


def m_cond_reciprocal(r):                   # returns rcond in the cond field
    if isinstance(r["cond"], float) and r["cond"] != 0:
        r["cond"], r["rcond"] = 1.0 / r["cond"], r["cond"]


def m_lu_perm_swapped(r):                  # first two entries of perm exchanged (factors unchanged)
    if len(r["perm"]) > 1:
        r["perm"][0], r["perm"][1] = r["perm"][1], r["perm"][0]


def m_qr_sign_not_normalised(r):            # Q column 0 and R row 0 negated: still A = QR, but R_00 < 0
    n = r["R"]["shape"][1]; qc = r["Q"]["shape"][1]
    r["R"]["data"][:n] = [-v for v in r["R"]["data"][:n]]
    r["Q"]["data"] = [-v if i % qc == 0 else v for i, v in enumerate(r["Q"]["data"])]


def m_chol_wrong_triangle(r):               # returns the transpose of what was asked for
    n = r["R"]["shape"][0]; d = r["R"]["data"]
    r["R"]["data"] = [d[j * n + i] for i in range(n) for j in range(n)]


def m_quadprog_lambda_sign(r):             # inequality multipliers with scipy's sign (negated)
    r["lambda"]["ineqlin"] = [-v for v in r["lambda"]["ineqlin"]]


def m_quadprog_x_wrong(r):                  # first component off by 1e-8 (relative)
    r["x"][0] = r["x"][0] * (1 + 1e-8) + 1e-8


def m_fminbnd_no_warning(r, env):           # drops the better-point warning and claims the grid probe passed
    env["warnings"] = [w for w in env["warnings"] if w.get("code") != "W.ACCURACY.BETTER_POINT_FOUND"]
    env["trust"]["checks"]["grid_probe"] = True


def m_interp1_one_ulp(r):                   # every value one ulp up: inside the tolerance, but node hits are no longer exact
    import math
    r["yq"]["data"] = [math.nextafter(v, math.inf) if isinstance(v, float) else v for v in r["yq"]["data"]]


def m_interp1_count_wrong(r):               # out_of_range_count off by one
    r["out_of_range_count"] = r["out_of_range_count"] + 1



def m_polyfit_coef_wrong(r):                # leading coefficient off by 1e-6 (relative)
    d = r["p"]["data"]; d[0] = d[0] * (1 + 1e-6) + 1e-12


def m_polyfit_mu_wrong(r):                  # centering mean shifted by 1e-9 of its size (p untouched)
    if r.get("mu"):
        r["mu"][0] = r["mu"][0] * (1 + 1e-9) + 1e-12


def m_polyval_value_wrong(r):               # every value off by 1e-12 (relative)
    r["y"]["data"] = [v * (1 + 1e-12) + 1e-300 if isinstance(v, float) else v for v in r["y"]["data"]]


def m_polyval_bound_zeroed(r):              # claims an error bound of 0 everywhere
    r["error_bound"]["data"] = [0.0 for _ in r["error_bound"]["data"]]


def m_roots_value_wrong(r):                 # first root off by 1e-9 (relative)
    d = r["roots"]["data"]
    if d:
        d[0][0] = d[0][0] * (1 + 1e-9) + 1e-12


def m_roots_radius_understated(r):          # error radii a million times too small
    r["error_radius"]["data"] = [v / 1e6 if isinstance(v, float) else v for v in r["error_radius"]["data"]]


def m_roots_unsorted(r):                    # roots in reverse order (radii and multiplicities follow)
    for k in ("roots", "error_radius"):
        r[k]["data"] = r[k]["data"][::-1]
    r["multiplicity"] = r["multiplicity"][::-1]


def m_convert_gain_wrong(r):               # returned system off by a factor 1 + 1e-6 (one coefficient set scaled)
    g = r["system"]
    if g["kind"] == "zpk":
        g["k"] = g["k"] * (1 + 1e-6)
    elif g["kind"] == "ss":
        g["C"]["data"] = [v * (1 + 1e-6) if isinstance(v, float) else v for v in g["C"]["data"]]
    else:
        g["num"] = [[[c * (1 + 1e-6) for c in e] for e in row] for row in g["num"]] if isinstance(g["num"][0], list) else [c * (1 + 1e-6) for c in g["num"]]


def m_convert_realization_mislabeled(r):    # correct system, wrong realization label
    r["realization"] = "identity" if r.get("realization") != "identity" else "controllable_canonical"


def m_eig_vectors_swapped(r):               # eigenvector columns 0 and 1 exchanged (labels no longer match)
    v = r.get("vectors")
    if v and v["shape"][0] > 1:
        n = v["shape"][0]; d = v["data"]
        for i in range(n):
            d[i * n], d[i * n + 1] = d[i * n + 1], d[i * n]


def m_eig_pair_order(r):                    # conjugate pairs listed with Im < 0 first (values only)
    d = r["values"]["data"]
    for k in range(len(d) - 1):
        if isinstance(d[k], list) and isinstance(d[k][1], float) and d[k][1] > 0 and d[k + 1][1] == -d[k][1]:
            d[k], d[k + 1] = d[k + 1], d[k]


def m_eig_value_wrong(r):                   # one eigenvalue off by 1e-8 (relative to its size)
    d = r["values"]["data"]
    if isinstance(d[0], list):
        d[0][0] = d[0][0] * (1 + 1e-8) + 1e-8
    else:
        d[0] = d[0] * (1 + 1e-8) + 1e-8


def m_svd_u_sign_only(r):                   # u_0 negated without v_0: factors no longer reproduce A
    u = r.get("U")
    if u:
        c = u["shape"][1]
        for i in range(u["shape"][0]):
            u["data"][i * c] = -u["data"][i * c]


def m_svd_both_signs(r):                    # u_0 and v_0 both negated: an equivalent SVD that breaks the sign rule
    for key in ("U", "V"):
        f = r.get(key)
        if f:
            c = f["shape"][1]
            for i in range(f["shape"][0]):
                f["data"][i * c] = -f["data"][i * c]



def m_butter_pole_wrong(r):                 # first pole moved by 1e-9
    r["p"][0][0] = r["p"][0][0] + 1e-9


def m_butter_warning_dropped(r, env):       # the b/a warning removed (b/a still useless)
    env["warnings"] = [w for w in env.get("warnings", []) if w.get("code") != "W.NUMERICAL.ILL_CONDITIONED"]


def m_filter_y_wrong(r):                    # one output sample off by 1e-9 of the signal scale
    d = r["y"]["data"]; i = len(d) // 2
    d[i] = d[i] + 1e-9 * max(abs(v) for v in d if isinstance(v, float))


def m_filter_state_dropped(r):              # final state reported as zero
    r["zf"]["data"] = [0.0 for _ in r["zf"]["data"]]



def m_fzero_x_wrong(r):                     # root moved by 1e-12 (relative)
    r["x"] = r["x"] * (1 + 1e-12) + 1e-300 if isinstance(r["x"], float) else r["x"]


def m_fzero_warning_dropped(r, env):        # the searched-bracket warning removed
    env["warnings"] = [w for w in env.get("warnings", []) if w.get("code") != "W.DOMAIN.BRACKET_SEARCHED"]


def m_fsolve_x_wrong(r):                    # first unknown off by 1e-10 (relative)
    r["x"][0] = r["x"][0] * (1 + 1e-10) + 1e-12


def m_fsolve_residual_zeroed(r):            # residual reported as exactly zero
    r["residual"] = [0.0 for _ in r["residual"]]



def m_linprog_lambda_sign(r):               # inequality multipliers negated (MATLAB vs scipy sign convention mix-up)
    r["lambda"]["ineqlin"] = [-v for v in r["lambda"]["ineqlin"]]


def m_fmincon_x_wrong(r):                   # optimum moved by 1e-4 (relative)
    r["x"] = [v * (1 + 1e-4) + 1e-6 for v in r["x"]]


def m_fmincon_lambda_dropped(r):            # all multipliers reported as zero
    for k in r["lambda"]:
        r["lambda"][k] = [0.0 for _ in r["lambda"][k]]


def m_fminsearch_x_shifted(r):              # optimum shifted by 100·tol_x
    r["x"][0] = r["x"][0] + 1e-2



def m_integral_value_wrong(r):              # value off by 1e-8 (relative)
    r["value"] = r["value"] * (1 + 1e-8) if isinstance(r["value"], float) else r["value"]


def m_integral_error_understated(r):        # error estimate reported a million times too small
    r["value"] = r["value"] * (1 + 1e-12) if isinstance(r["value"], float) else r["value"]
    r["error_estimate"] = r["error_estimate"] / 1e6 if isinstance(r["error_estimate"], float) else r["error_estimate"]


def m_trapz_value_ulps(r):                  # value 8 ulps off (a naive sum would do this on large inputs)
    import math
    v = r["value"]
    if isinstance(v, float):
        for _ in range(8):
            v = math.nextafter(v, math.inf)
        r["value"] = v


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
    "pidtune_kp_wrong": ("control.design.pidtune", m_pidtune_kp_wrong),
    "pidtune_controller_wrong": ("control.design.pidtune", m_pidtune_controller_wrong),
    "fft_bin_wrong": ("signal.spectral.fft", m_fft_bin_wrong),
    "fft_shifted_frequencies": ("signal.spectral.fft", m_fft_shifted_frequencies),
    "fft_scale_wrong": ("signal.spectral.fft", m_fft_scale_wrong),
    "ode_y_wrong": ("ode.ivp.solve", m_ode_y_wrong),
    "ode_states_reordered": ("ode.ivp.solve", m_ode_states_reordered),
    "ode_overclaims": ("ode.ivp.solve", m_ode_overclaims),
    "linsolve_x_wrong": ("linalg.system.solve", m_linsolve_x_wrong),
    "linsolve_overclaims": ("linalg.system.solve", m_linsolve_overclaims),
    "quadprog_lambda_sign": ("optim.quadratic.quadprog", m_quadprog_lambda_sign),
    "quadprog_x_wrong": ("optim.quadratic.quadprog", m_quadprog_x_wrong),
    "fminbnd_no_warning": ("optim.scalar.fminbnd", m_fminbnd_no_warning),
    "lu_perm_swapped": ("linalg.decomp.lu", m_lu_perm_swapped),
    "qr_sign_not_normalised": ("linalg.decomp.qr", m_qr_sign_not_normalised),
    "chol_wrong_triangle": ("linalg.decomp.chol", m_chol_wrong_triangle),
    "rank_off_by_one": ("linalg.subspace.rank", m_rank_off_by_one),
    "rank_overclaims": ("linalg.subspace.rank", m_rank_overclaims),
    "basis_null_scaled": ("linalg.subspace.basis", m_basis_null_scaled),
    "pinv_scaled": ("linalg.matrix.pinv", m_pinv_scaled),
    "norm_rel_1e12": ("linalg.matrix.norm", m_norm_rel_1e12),
    "cond_reciprocal": ("linalg.matrix.cond", m_cond_reciprocal),
    "inv_transposed": ("linalg.matrix.inv", m_inv_transposed),
    "inv_overclaims": ("linalg.matrix.inv", m_inv_overclaims),
    "det_sign_flipped": ("linalg.matrix.det", m_det_sign_flipped),
    "det_never_zero": ("linalg.matrix.det", m_det_never_zero),
    "det_log_wrong": ("linalg.matrix.det", m_det_log_wrong),
    "interp1_one_ulp": ("interp.grid.interp1", m_interp1_one_ulp),
    "interp1_count_wrong": ("interp.grid.interp1", m_interp1_count_wrong),
    "polyfit_coef_wrong": ("fit.poly.polyfit", m_polyfit_coef_wrong),
    "polyfit_mu_wrong": ("fit.poly.polyfit", m_polyfit_mu_wrong),
    "polyval_value_wrong": ("fit.poly.polyval", m_polyval_value_wrong),
    "polyval_bound_zeroed": ("fit.poly.polyval", m_polyval_bound_zeroed),
    "roots_value_wrong": ("fit.poly.roots", m_roots_value_wrong),
    "roots_radius_understated": ("fit.poly.roots", m_roots_radius_understated),
    "roots_unsorted": ("fit.poly.roots", m_roots_unsorted),
    "convert_gain_wrong": ("control.lti.convert", m_convert_gain_wrong),
    "convert_realization_mislabeled": ("control.lti.convert", m_convert_realization_mislabeled),
    "eig_vectors_swapped": ("linalg.decomp.eig", m_eig_vectors_swapped),
    "eig_pair_order": ("linalg.decomp.eig", m_eig_pair_order),
    "eig_value_wrong": ("linalg.decomp.eig", m_eig_value_wrong),
    "svd_u_sign_only": ("linalg.decomp.svd", m_svd_u_sign_only),
    "svd_both_signs": ("linalg.decomp.svd", m_svd_both_signs),
    "butter_pole_wrong": ("signal.design.butter", m_butter_pole_wrong),
    "butter_warning_dropped": ("signal.design.butter", m_butter_warning_dropped),
    "filter_y_wrong": ("signal.filter.filter", m_filter_y_wrong),
    "filter_state_dropped": ("signal.filter.filter", m_filter_state_dropped),
    "fzero_x_wrong": ("nonlinear.root.fzero", m_fzero_x_wrong),
    "fzero_warning_dropped": ("nonlinear.root.fzero", m_fzero_warning_dropped),
    "fsolve_x_wrong": ("nonlinear.system.fsolve", m_fsolve_x_wrong),
    "fsolve_residual_zeroed": ("nonlinear.system.fsolve", m_fsolve_residual_zeroed),
    "linprog_lambda_sign": ("optim.linear.linprog", m_linprog_lambda_sign),
    "fmincon_x_wrong": ("optim.local.fmincon", m_fmincon_x_wrong),
    "fmincon_lambda_dropped": ("optim.local.fmincon", m_fmincon_lambda_dropped),
    "fminsearch_x_shifted": ("optim.local.fminsearch", m_fminsearch_x_shifted),
    "integral_value_wrong": ("calculus.quadrature.integral", m_integral_value_wrong),
    "integral_error_understated": ("calculus.quadrature.integral", m_integral_error_understated),
    "trapz_value_ulps": ("calculus.quadrature.trapz", m_trapz_value_ulps),
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
    f(env["result"], env) if f.__code__.co_argcount == 2 else f(env["result"])
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
    for st in (sys.stdin, sys.stdout):
        st.reconfigure(encoding="utf-8")   # Windows: the console code page would mangle non-ASCII JSON
    op, f = MUTANTS[os.environ["AEOI_MUTANT"]]
    impl = os.environ.get("AEOI_IMPL", os.path.join(ROOT, "rust", "target", "release", "aeoi-mcp"))
    p = subprocess.Popen([impl], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1)

    def pump_in():
        for line in sys.stdin:
            p.stdin.write(line); p.stdin.flush()
        p.stdin.close()
    threading.Thread(target=pump_in, daemon=True).start()
    for line in p.stdout:
        sys.stdout.write(rewrite(line, op, f)); sys.stdout.flush()


if __name__ == "__main__":
    main()
