"""The runner must pass the unmodified second implementation and fail every mutant for the stated reason.

    python3 -I conformance/tests/test_runner_catches_mutants.py      (from the repository root)
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RUNNER = os.path.join(ROOT, "conformance", "aeoi_conformance.py")
MUTANT = os.path.join(ROOT, "conformance", "tests", "mutant_server.py")
SERVER = os.path.join(ROOT, "conformance", "impls", "python_scipy", "server.py")
PROXY = os.path.join(ROOT, "conformance", "tests", "proxy_mutant.py")
REF = os.path.join(ROOT, "rust", "target", "release", "aeoi-mcp")

# proxy mutants over the reference implementation (rules_control.py): needle that must appear among the failed
# checks, or None for an equivalent answer that must pass on every case
PROXY_EXPECT = {
    "pz_pole_wrong": ("control.lti.poles_zeros", "poles: matched set"),
    "pz_reordered": ("control.lti.poles_zeros", None),
    "bode_mag_wrong": ("control.lti.bode", "magnitude"),
    "bode_phase_360": ("control.lti.bode", "phase on the spec's branch"),
    "step_y_wrong": ("control.lti.step", "y: max"),
    "lqr_S_wrong": ("control.design.lqr", "S: rel_fro"),
    "lqr_poles_reordered": ("control.design.lqr", None),
    "c2d_A_wrong": ("control.lti.c2d", "transfer-function values"),
    "c2d_equivalent": ("control.lti.c2d", None),
    "lsim_y_wrong": ("control.lti.lsim", "y: max"),
    "lsim_interp_mislabeled": ("control.lti.lsim", "interpolation equals"),
    "pidtune_kp_wrong": ("control.design.pidtune", "gains Kp"),
    "pidtune_controller_wrong": ("control.design.pidtune", "recomputed by the runner"),
    "fft_bin_wrong": ("signal.spectral.fft", "X: per channel"),
    "fft_shifted_frequencies": ("signal.spectral.fft", "frequency: |Δ|"),
    "fft_scale_wrong": ("signal.spectral.fft", "Parseval, recomputed by the runner"),
    "ode_y_wrong": ("ode.ivp.solve", "y vs truth"),
    "ode_states_reordered": ("ode.ivp.solve", "states echo the request"),
    "ode_overclaims": ("ode.ivp.solve", "tolerance_consistent = true ⇒"),
    "linsolve_x_wrong": ("linalg.system.solve", "x vs exact answer"),
    "linsolve_overclaims": ("linalg.system.solve", "backward_stable = true ⇒"),
    "interp1_one_ulp": ("interp.grid.interp1", "node hits exact"),
    "interp1_count_wrong": ("interp.grid.interp1", "out_of_range_count recomputed"),
    "polyfit_coef_wrong": ("fit.poly.polyfit", "p vs exact answer"),
    "polyfit_mu_wrong": ("fit.poly.polyfit", "mu vs exact mean"),
    "polyval_value_wrong": ("fit.poly.polyval", "y vs the exact value"),
    "polyval_bound_zeroed": ("fit.poly.polyval", "claim: |y − p(t)| ≤ error_bound"),
    "roots_value_wrong": ("fit.poly.roots", "simple roots vs mpmath truth"),
    "roots_radius_understated": ("fit.poly.roots", "claim: every true root lies within error_radius"),
    "roots_unsorted": ("fit.poly.roots", "roots sorted"),
    "quadprog_lambda_sign": ("optim.quadratic.quadprog", "multipliers"),
    "quadprog_x_wrong": ("optim.quadratic.quadprog", "x vs the true optimum"),
    "fminbnd_no_warning": ("optim.scalar.fminbnd", "BETTER_POINT_FOUND"),
    "lu_perm_swapped": ("linalg.decomp.lu", "perm equals"),
    "qr_sign_not_normalised": ("linalg.decomp.qr", "R_ii ≥ 0"),
    "chol_wrong_triangle": ("linalg.decomp.chol", "triangle as requested"),
    "rank_off_by_one": ("linalg.subspace.rank", "rank equals the true rank"),
    "rank_overclaims": ("linalg.subspace.rank", "claim: rank_well_separated"),
    "basis_null_scaled": ("linalg.subspace.basis", "orthonormality of the returned bases"),
    "pinv_scaled": ("linalg.matrix.pinv", "X vs the true pseudoinverse"),
    "norm_rel_1e12": ("linalg.matrix.norm", "norm vs the truth"),
    "cond_reciprocal": ("linalg.matrix.cond", "cond vs the truth"),
    "inv_transposed": ("linalg.matrix.inv", "X vs exact inverse"),
    "inv_overclaims": ("linalg.matrix.inv", "claim: forward_error_below_one"),
    "det_sign_flipped": ("linalg.matrix.det", "sign equals"),
    "det_never_zero": ("linalg.matrix.det", "singular data"),
    "det_log_wrong": ("linalg.matrix.det", "ln|det| vs exact"),
    "convert_gain_wrong": ("control.lti.convert", "response of the returned system vs the INPUT"),
    "convert_realization_mislabeled": ("control.lti.convert", "realization as fixed by CV-2"),
    "eig_vectors_swapped": ("linalg.decomp.eig", "recomputed by the runner"),
    "eig_pair_order": ("linalg.decomp.eig", "ordering"),
    "eig_value_wrong": ("linalg.decomp.eig", "values vs exact eigenvalues"),
    "svd_u_sign_only": ("linalg.decomp.svd", "U·diag(s)·Vᵀ"),
    "svd_both_signs": ("linalg.decomp.svd", "sign rule"),
    "butter_pole_wrong": ("signal.design.butter", "z, p, k vs the exact"),
    "butter_warning_dropped": ("signal.design.butter", "exactly when the b/a"),
    "filter_y_wrong": ("signal.filter.filter", "y, zf vs the exact"),
    "filter_state_dropped": ("signal.filter.filter", "y, zf vs the exact"),
    "fzero_x_wrong": ("nonlinear.root.fzero", "nearest exact root"),
    "fzero_warning_dropped": ("nonlinear.root.fzero", "BRACKET_SEARCHED exactly"),
    "fsolve_x_wrong": ("nonlinear.system.fsolve", "x vs the exact root"),
    "fsolve_residual_zeroed": ("nonlinear.system.fsolve", "recomputed by the runner"),
    "linprog_lambda_sign": ("optim.linear.linprog", "KKT recomputed"),
    "fmincon_x_wrong": ("optim.local.fmincon", "x vs the analytic"),
    "fmincon_lambda_dropped": ("optim.local.fmincon", "Lagrangian stationarity"),
    "fminsearch_x_shifted": ("optim.local.fminsearch", "x vs the analytic"),
    "integral_value_wrong": ("calculus.quadrature.integral", "value vs the 50-digit truth"),
    "integral_error_understated": ("calculus.quadrature.integral", "error estimate covers"),
    "trapz_value_ulps": ("calculus.quadrature.trapz", "exact rational trapezoidal"),
}

# mutant -> substring that must appear among the names of the failed checks
EXPECT = {
    "transposed_A": "X",
    "bad_request_fingerprint": "fingerprint.request",
    "ignores_method_policy": "hard constraint not honoured",
    "undeclared_warning": "declared in the manifest",
    "wrong_error_code": "expected error",
    "overclaims": "claim equals",
    "nondeterministic": "repeat run",
    "wrong_default": "gives the same result",
}


def run(command, env=None, operators="control.math.lyap"):
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as t:
        rp = t.name
    p = subprocess.run([sys.executable, "-I", RUNNER, "run", "--impl", command, "--operators", operators, "--report", rp],
                       cwd=ROOT, capture_output=True, text=True, encoding="utf-8", env={**os.environ, **(env or {})})
    rep = json.load(open(rp, encoding="utf-8")); os.unlink(rp)
    return p.returncode, rep


def failed_text(rep):
    return " | ".join(f["check"] + " " + f["detail"] for c in rep["cases"] for f in c.get("failed_checks", [])) + " | " + " | ".join(c.get("detail", "") for c in rep["cases"] if c["result"] == "fail")


def main():
    bad = 0
    rc, rep = run(f"{sys.executable} -I {SERVER}")
    ok = rc == 0 and not any(c["result"] == "fail" for c in rep["cases"])
    print(("ok   " if ok else "FAIL ") + "unmodified implementation passes"); bad += not ok
    for name, needle in EXPECT.items():
        rc, rep = run(f"{sys.executable} -I {MUTANT}", {"AEOI_MUTANT": name})
        nfail = sum(1 for c in rep["cases"] if c["result"] == "fail")
        ok = rc == 1 and nfail > 0 and needle in failed_text(rep)
        print(("ok   " if ok else "FAIL ") + f"mutant {name:26s} -> exit {rc}, {nfail} failing case(s), reason contains {needle!r}")
        bad += not ok
    if not os.path.exists(REF):
        print(f"skip proxy mutants: {REF} not built (cargo build --release -p aeoi-mcp)")
    else:
        for name, (op, needle) in PROXY_EXPECT.items():
            rc, rep = run(f"{sys.executable} -I {PROXY}", {"AEOI_MUTANT": name}, operators=op)
            nfail = sum(1 for c in rep["cases"] if c["result"] == "fail")
            if needle is None:
                ok = rc == 0 and nfail == 0
                what = "equivalent answer passes"
            else:
                ok = rc == 1 and nfail > 0 and needle in failed_text(rep)
                what = f"reason contains {needle!r}"
            print(("ok   " if ok else "FAIL ") + f"proxy {name:23s} -> exit {rc}, {nfail} failing case(s), {what}")
            bad += not ok
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
