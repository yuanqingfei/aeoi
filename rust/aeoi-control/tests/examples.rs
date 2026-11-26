//! Example requests (examples/<op>/request*.json) must reproduce the published responses and the
//! python-control values in golden/computed_values.json within the operator's tolerance class (NOT MATLAB values).
//! Also: envelopes validate against envelope+output schema; transport encodings (json/b64/ref/sparse) give
//! identical results and identical input_hash; dry_run; output_encoding=b64.
mod common;
use aeoi_core::{schema, HostCtx};
use base64::Engine;
use common::*;
use serde_json::{json, Value};
use sha2::Digest;

fn read(p: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(spec_root().join(p)).unwrap()).unwrap()
}
fn call(op: &str, req: &Value, host: &HostCtx) -> Value {
    let reg = aeoi_control::registry();
    let o = aeoi_control::find(&reg, op).unwrap();
    let env = o.call(req, host);
    let v = schema::Validator::new(&schema::envelope_with_result(&o.meta().output_schema));
    let viol = v.violations(&env, 10);
    assert!(viol.is_empty(), "{op}: envelope schema violations {viol:?}");
    env
}
fn maxabs(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

#[test]
fn bode_example() {
    let h = HostCtx::default();
    let env = call("control.lti.bode", &read("examples/control.lti.bode/request.json"), &h);
    let cv = read("golden/computed_values.json");
    let mag = fvec(&env["result"]["magnitude"]["value"]["data"]);
    let ph = fvec(&env["result"]["phase"]["value"]["data"]);
    let em = maxabs(&mag, &fvec(&cv["bode"]["mag_db"]));
    let ep = ph.iter().zip(fvec(&cv["bode"]["phase_deg"])).map(|(a, b)| wrap180(a - b).abs()).fold(0.0, f64::max);
    println!("bode example vs python-control: mag {em:.3e} dB (tol 1e-9), phase {ep:.3e} deg (tol 1e-8)");
    assert!(em <= 1e-9 && ep <= 1e-8);
    let pub_ = read("examples/control.lti.bode/response.success.json");
    assert!(maxabs(&mag, &fvec(&pub_["result"]["magnitude"]["value"]["data"])) <= 1e-9);
    assert_hash_on_same_platform(&env, &pub_, "bitwise determinism on this platform");
    let e = call("control.lti.bode", &read("examples/control.lti.bode/request.error.json"), &h);
    assert_eq!(e["error"]["code"], read("examples/control.lti.bode/response.error.json")["error"]["code"]);
    let ns = call("control.lti.bode", &read("examples/control.lti.bode/request.ss_nonsymmetric.json"), &h);
    assert_eq!(ns["status"], "ok");
}

#[test]
fn step_example() {
    let h = HostCtx::default();
    let env = call("control.lti.step", &read("examples/control.lti.step/request.json"), &h);
    let cv = read("golden/computed_values.json");
    let y = fvec(&env["result"]["y"]["value"]["data"]);
    let ey = maxabs(&y, &fvec(&cv["step"]["y"]));
    println!("step example vs python-control: y {ey:.3e} (tol 1e-10·max|y|)");
    assert!(ey <= 1e-10 * 1.3);
    // NB: computed_values.json step info came from python-control's *auto* grid (v0.1 example bug) and is not compared.
    let i = &env["result"]["info"];
    assert!((f(&i["overshoot"]["value"]) - 37.2318).abs() < 1e-3);
    let pub_ = read("examples/control.lti.step/response.success.json");
    assert!(maxabs(&y, &fvec(&pub_["result"]["y"]["value"]["data"])) <= 1e-12);
    let e = call("control.lti.step", &read("examples/control.lti.step/request.error.json"), &h);
    assert_eq!(e["error"]["code"], "E.RESOURCE.COMPUTE_LIMIT");
    assert_eq!(e["error"]["code"], read("examples/control.lti.step/response.error.json")["error"]["code"]);
    // remediation patch must make the request succeed
    let mut fixed = read("examples/control.lti.step/request.error.json");
    for p in e["error"]["remediation"][0]["patch"].as_array().unwrap() {
        *fixed.pointer_mut(p["path"].as_str().unwrap()).unwrap() = p["value"].clone();
    }
    assert_eq!(call("control.lti.step", &fixed, &h)["status"], "ok", "remediation patch did not fix the request");
}

#[test]
fn lqr_example() {
    let h = HostCtx::default();
    let env = call("control.design.lqr", &read("examples/control.design.lqr/request.json"), &h);
    let k = fvec(&env["result"]["K"]["data"]);
    let analytic = [1.0, 3f64.sqrt()];
    let e = maxabs(&k, &analytic) / fro(&analytic);
    println!("lqr example vs analytic K=[1,√3]: {e:.3e} (tol 1e-10)");
    assert!(e <= 1e-10);
    let cv = read("golden/computed_values.json");
    let ek: Vec<f64> = cv["lqr"]["K"][0].as_array().unwrap().iter().map(f).collect();
    assert!(maxabs(&k, &ek) / fro(&ek) <= 1e-10);
    let err = call("control.design.lqr", &read("examples/control.design.lqr/request.error.json"), &h);
    assert_eq!(err["error"]["code"], "E.DOMAIN.NOT_STABILIZABLE");
    // every call_operator remediation must reference a registered operator
    let reg = aeoi_control::registry();
    for r in err["error"]["remediation"].as_array().unwrap() {
        if r["kind"] == "call_operator" {
            assert!(aeoi_control::find(&reg, r["operator"].as_str().unwrap()).is_some());
        }
    }
}

fn le_bytes(d: &[f64]) -> Vec<u8> {
    d.iter().flat_map(|x| x.to_le_bytes()).collect()
}

#[test]
fn transport_encodings_are_equivalent() {
    let dir = std::env::temp_dir().join(format!("aeoi-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let host = HostCtx { data_root: Some(dir.clone()), ..HostCtx::default() };
    let base = read("examples/control.design.lqr/request.json");
    let a = [0.0, 1.0, 0.0, 0.0];
    let r0 = call("control.design.lqr", &base, &host);

    let mut req = base.clone();
    let bytes = le_bytes(&a);
    req["A"] = json!({"dtype":"f64","shape":[2,2],"order":"C","byte_order":"little",
        "data_b64": base64::engine::general_purpose::STANDARD.encode(&bytes), "sha256": format!("sha256:{}", hex(&sha2::Sha256::digest(&bytes)))});
    let bb = le_bytes(&[0.0, 1.0]);
    std::fs::write(dir.join("B.bin"), &bb).unwrap();
    req["B"] = json!({"dtype":"f64","shape":[2,1],"order":"C","byte_order":"little","encoding":"raw","uri": format!("file://{}", dir.join("B.bin").display()),
        "sha256": format!("sha256:{}", hex(&sha2::Sha256::digest(&bb))), "byte_length": bb.len()});
    let rd = call("control.design.lqr", &req, &host);
    assert_eq!(rd["status"], "ok", "{}", rd["error"]);
    assert_eq!(r0["result"]["K"], rd["result"]["K"]);
    assert_eq!(r0["reproducibility"]["input_hash"], rd["reproducibility"]["input_hash"], "input_hash must be independent of dense encoding (json/b64/ref)");
    req["Q"] = json!({"format":"csr","dtype":"f64","shape":[2,2],"indptr":[0,1,2],"indices":[0,1],"data":[1.0,1.0]});
    let r1 = call("control.design.lqr", &req, &host);
    assert_eq!(r1["status"], "ok_with_warnings", "{}", r1["error"]);
    assert!(r1["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.PERFORMANCE.SPARSE_DENSIFIED"));
    assert_eq!(r0["result"]["K"], r1["result"]["K"]);
    // spec v0.2 §3.7: sparse inputs hash their canonical CSR form, so dense vs sparse hashes differ by design
    assert_ne!(r0["reproducibility"]["input_hash"], r1["reproducibility"]["input_hash"]);

    // digest mismatch / path escape / non-file scheme
    let mut bad = req.clone();
    bad["A"]["sha256"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert_eq!(call("control.design.lqr", &bad, &host)["error"]["code"], "E.VALIDATION.DIGEST_MISMATCH");
    let mut esc = req.clone();
    esc["B"]["uri"] = json!("file:///etc/passwd");
    assert_eq!(call("control.design.lqr", &esc, &host)["error"]["code"], "E.VALIDATION.REF_FORBIDDEN");
    let mut http = req.clone();
    http["B"]["uri"] = json!("https://example.com/B.bin");
    assert_eq!(call("control.design.lqr", &http, &host)["error"]["code"], "E.VALIDATION.REF_UNSUPPORTED");
    let mut sp = req.clone();
    sp["Q"]["indices"] = json!([1, 0]);
    sp["Q"]["indptr"] = json!([0, 2, 2]);
    sp["Q"]["data"] = json!([1.0, 1.0]);
    assert_eq!(call("control.design.lqr", &sp, &host)["error"]["code"], "E.VALIDATION.SPARSE_STRUCTURE");

    // dry_run: no computation, estimate present
    let mut dr = base.clone();
    dr["exec"]["dry_run"] = json!(true);
    let d = call("control.design.lqr", &dr, &host);
    assert_eq!(d["dry_run"], true);
    assert!(d["result"].is_null() && d["estimate"].is_object());

    // output_encoding b64 decodes to the same numbers
    let mut ob = base.clone();
    ob["exec"]["output_encoding"] = json!("b64");
    let o = call("control.design.lqr", &ob, &host);
    let raw = base64::engine::general_purpose::STANDARD.decode(o["result"]["K"]["data_b64"].as_str().unwrap()).unwrap();
    let k: Vec<f64> = raw.chunks(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(json!(k), r0["result"]["K"]["data"]);
    assert_eq!(o["reproducibility"]["output_hash"], r0["reproducibility"]["output_hash"], "output_hash must be encoding-independent");
    std::fs::remove_dir_all(&dir).ok();
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ------------------------------------------------------------------ batch 2 (spec v0.3)
fn relf(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let d = fro_diff(a, b);
    if d == 0.0 {
        0.0
    } else {
        d / fro(b).max(f64::MIN_POSITIVE)
    }
}
fn cset(v: &Value) -> Vec<(f64, f64)> {
    let arr = if v.get("data").is_some() { &v["data"] } else { v };
    arr.as_array().unwrap().iter().map(|c| (f(&c[0]), f(&c[1]))).collect()
}

/// Batch-2 examples: request.json reproduces python-control/scipy reference values (reference.python-control.json,
/// NOT MATLAB) and the published response bit-for-bit (output_hash); request.error.json returns the published
/// error code with remediation; request.nonsymmetric.json reproduces its published response.
#[test]
fn batch2_examples() {
    let h = HostCtx::default();
    let ops = [
        "control.lti.convert", "control.lti.poles_zeros", "control.analysis.ctrb_obsv", "control.analysis.margin", "control.lti.c2d",
        "control.lti.impulse", "control.math.lyap", "control.design.place", "control.design.dlqr",
    ];
    for op in ops {
        let dir = format!("examples/{op}");
        let env = call(op, &read(&format!("{dir}/request.json")), &h);
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{op}: {}", env["error"]);
        let pub_ = read(&format!("{dir}/response.success.json"));
        assert_hash_on_same_platform(&env, &pub_, &format!("{op}: bitwise determinism on this platform"));
        let rf = &read(&format!("{dir}/reference.python-control.json"))["request.json"];
        let r = &env["result"];
        let err: f64 = match op {
            "control.lti.convert" => ["A", "B", "C", "D"].iter().map(|k| relf(&fvec(&r["system"][*k]["data"]), &fvec(&rf[*k]["data"]))).fold(0.0, f64::max),
            "control.lti.poles_zeros" => matched_set_err(&cset(&r["poles"]), &cset(&rf["poles"]))
                .max(matched_set_err(&cset(&r["zeros"]), &cset(&rf["zeros"])))
                .max((fvec(&r["dcgain"]["value"]["data"])[0] - fvec(&rf["dcgain"])[0]).abs()),
            "control.analysis.ctrb_obsv" => relf(&fvec(&r["ctrb"]["matrix"]["data"]), &fvec(&rf["ctrb"]["data"]))
                .max(relf(&fvec(&r["obsv"]["matrix"]["data"]), &fvec(&rf["obsv"]["data"])))
                .max(relf(&fvec(&r["gramians"]["controllability"]["data"]), &fvec(&rf["Wc"]["data"])))
                .max(relf(&fvec(&r["gramians"]["observability"]["data"]), &fvec(&rf["Wo"]["data"]))),
            "control.analysis.margin" => {
                let g = |k: &str| f(&r[k]["value"]);
                let e1 = ((g("gain_margin") - f(&rf["gm"])) / f(&rf["gm"])).abs()
                    .max(((g("phase_crossover_frequency") - f(&rf["wpc"])) / f(&rf["wpc"])).abs())
                    .max(((g("gain_crossover_frequency") - f(&rf["wgc"])) / f(&rf["wgc"])).abs())
                    .max(((g("stability_margin") - f(&rf["sm"])) / f(&rf["sm"])).abs())
                    .max((g("phase_margin") - f(&rf["pm"])).abs() * 0.1);
                assert!(((g("stability_margin_frequency") - f(&rf["wms"])) / f(&rf["wms"])).abs() <= 1e-6);
                assert!((g("gain_margin") - 1.5).abs() <= 1e-12 && (g("phase_crossover_frequency") - 2f64.sqrt()).abs() <= 1e-12, "hand check GM = 1.5 at √2");
                e1
            }
            "control.lti.c2d" => ["A", "B", "C", "D"].iter().map(|k| relf(&fvec(&r["system"][*k]["data"]), &fvec(&rf[*k]["data"]))).fold(0.0, f64::max),
            "control.lti.impulse" => {
                let y = fvec(&r["y"]["value"]["data"]);
                let w = fvec(&rf["y"]);
                maxabs(&y, &w) / w.iter().map(|x| x.abs()).fold(0.0, f64::max)
            }
            "control.math.lyap" => relf(&fvec(&r["X"]["data"]), &fvec(&rf["X"]["data"])),
            "control.design.place" => {
                let k = fvec(&r["K"]["data"]);
                assert!(maxabs(&k, &[2.0, 2.0]) <= 1e-12, "hand check K = [2, 2]: {k:?}");
                relf(&k, &fvec(&rf["K"]["data"]))
            }
            "control.design.dlqr" => relf(&fvec(&r["K"]["data"]), &fvec(&rf["K"]["data"])).max(relf(&fvec(&r["S"]["data"]), &fvec(&rf["S"]["data"]))),
            _ => unreachable!(),
        };
        println!("{op} example vs python-control/scipy: max err {err:.3e} (tol 1e-10)");
        assert!(err <= 1e-10, "{op}: example deviates from python-control/scipy by {err:e}");
        let e = call(op, &read(&format!("{dir}/request.error.json")), &h);
        let pe = read(&format!("{dir}/response.error.json"));
        assert_eq!(e["status"], "error");
        assert_eq!(e["error"]["code"], pe["error"]["code"], "{op}: error code");
        assert!(!e["error"]["remediation"].as_array().map(|v| v.is_empty()).unwrap_or(true) || e["error"]["category"] == "validation", "{op}: domain/numerical errors carry remediation");
        let ns = call(op, &read(&format!("{dir}/request.nonsymmetric.json")), &h);
        let pns = read(&format!("{dir}/response.nonsymmetric.json"));
        assert_hash_on_same_platform(&ns, &pns, &format!("{op}: nonsymmetric example"));
    }
}

/// v0.3.6 (K-4): control.analysis.loopsens example equals the chained python-control primitives.
#[test]
fn loopsens_example() {
    let h = HostCtx::default();
    let op = "control.analysis.loopsens";
    let dir = format!("examples/{op}");
    let env = call(op, &read(&format!("{dir}/request.json")), &h);
    assert_eq!(env["status"], "ok", "{}", env["error"]);
    let pub_ = read(&format!("{dir}/response.success.json"));
    assert_hash_on_same_platform(&env, &pub_, "bitwise example");
    assert_eq!(env["receipt"]["evidence"]["claim"], "spot_checked", "E-12: margins are grid-audited");
    let rf = &read(&format!("{dir}/reference.python-control.json"))["request.json"];
    let r = &env["result"];
    let e = relf(&fvec(&r["closed_loop"]["num"]), &fvec(&rf["closed_loop"]["num"])).max(relf(&fvec(&r["closed_loop"]["den"]), &fvec(&rf["closed_loop"]["den"])));
    assert!(e <= 1e-12, "T vs python-control {e:e}");
    for (k, want) in [("bandwidth_rad_s", &rf["bandwidth_rad_s"]), ("gain_crossover_rad_s", &rf["gain_crossover_rad_s"])] {
        assert!((f(&r[k]["value"]) - f(want)).abs() <= 1e-9 * f(want).abs(), "{k}");
    }
    assert!((f(&r["step_metrics"]["info"]["overshoot"]["value"]) - f(&rf["step"]["overshoot"])).abs() <= 1e-8);
    let err = call(op, &read(&format!("{dir}/request.error.json")), &h);
    assert_eq!(err["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
}

/// Batch 3 (spec v0.3.5): interconnection, pid, stepinfo, pade.
#[test]
fn batch3_examples() {
    let h = HostCtx::default();
    let ops = ["control.lti.series", "control.lti.parallel", "control.lti.feedback", "control.design.pid", "control.lti.stepinfo", "control.lti.pade"];
    for op in ops {
        let dir = format!("examples/{op}");
        let env = call(op, &read(&format!("{dir}/request.json")), &h);
        assert!(env["status"] == "ok" || env["status"] == "ok_with_warnings", "{op}: {}", env["error"]);
        let pub_ = read(&format!("{dir}/response.success.json"));
        assert_hash_on_same_platform(&env, &pub_, &format!("{op}: bitwise example"));
        assert_eq!(env["receipt"]["evidence"]["claim"], pub_["receipt"]["evidence"]["claim"]);
        let rf = &read(&format!("{dir}/reference.python-control.json"))["request.json"];
        let err = if op == "control.lti.stepinfo" {
            let info = &env["result"]["info"];
            ["rise_time", "settling_time", "overshoot", "steady_state"].iter().map(|k| (f(&info[k]["value"]) - f(&rf[k])).abs()).fold(0.0, f64::max)
        } else {
            relf(&fvec(&env["result"]["system"]["num"]), &fvec(&rf["num"])).max(relf(&fvec(&env["result"]["system"]["den"]), &fvec(&rf["den"])))
        };
        assert!(err <= 1e-12, "{op}: example vs closed form {err:e}");
        let e = call(op, &read(&format!("{dir}/request.error.json")), &h);
        assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"], "{op} error");
        assert!(!e["error"]["remediation"].as_array().unwrap().is_empty(), "{op}: remediation");
        let ns_path = format!("{dir}/request.nonsymmetric.json");
        if spec_root().join(&ns_path).exists() {
            let ns = call(op, &read(&ns_path), &h);
            let pns = read(&format!("{dir}/response.nonsymmetric.json"));
            assert_hash_on_same_platform(&ns, &pns, &format!("{op}: nonsymmetric"));
        }
    }
}

/// v0.4.5: control.lti.lsim examples. request.json (sine input, foh) and request.ss_x0.json (non-symmetric A,
/// x0, zoh, states) reproduce scipy.signal.lsim (reference.python-control.json, NOT MATLAB) and their published
/// responses bit-for-bit on the same platform; request.error.json (x0 with a tf system) is an F-8 option conflict.
#[test]
fn lsim_example() {
    let h = HostCtx::default();
    let op = "control.lti.lsim";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.ss_x0.json", "response.ss_x0.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")), &h);
        assert_eq!(env["status"], "ok", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let y = fvec(&env["result"]["y"]["value"]["data"]);
        let w = fvec(&rf[rq]["y"]);
        let err = maxabs(&y, &w) / w.iter().map(|x| x.abs()).fold(1.0, f64::max);
        assert!(err <= 1e-10, "{rq}: y vs scipy {err:e}");
        if let Some(xr) = rf[rq].get("x") {
            let x = fvec(&env["result"]["x"]["data"]);
            assert!(maxabs(&x, &fvec(xr)) <= 1e-10, "{rq}: x vs scipy");
        }
        assert_eq!(env["trust"]["checks"]["half_step_consistent"], true, "{rq}");
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")), &h);
    assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
    assert_eq!(e["error"]["code"], "E.VALIDATION.OPTION_CONFLICT");
    assert!(!e["error"]["remediation"].as_array().unwrap().is_empty(), "remediation");
}

/// v0.4.6: control.design.pidtune examples. request.json (PID, 1/(s+1)³, ωc = 1, PM 60°) and request.pi.json
/// reproduce the reference gains (numpy spec formulas verified with python-control) and their published
/// responses bit-for-bit on the same platform; request.error.json (PI needs +15° lead) is TARGET_NOT_ACHIEVABLE.
/// The controller equals control.design.pid on the returned gains (K-4).
#[test]
fn pidtune_example() {
    let h = HostCtx::default();
    let op = "control.design.pidtune";
    let dir = format!("examples/{op}");
    let rf = read(&format!("{dir}/reference.python-control.json"));
    for (rq, rs) in [("request.json", "response.success.json"), ("request.pi.json", "response.pi.json")] {
        let env = call(op, &read(&format!("{dir}/{rq}")), &h);
        assert_eq!(env["status"], "ok", "{rq}: {}", env["error"]);
        assert_hash_on_same_platform(&env, &read(&format!("{dir}/{rs}")), &format!("{op} {rq}: bitwise example"));
        let r = &env["result"];
        for k in ["Kp", "Ki", "Kd", "Tf"] {
            let (a, b) = (f(&r["gains"][k]), f(&rf[rq]["gains"][k]));
            assert!((a - b).abs() <= 1e-10 * b.abs().max(1.0), "{rq}: {k} {a} vs {b}");
        }
        assert!((f(&r["achieved"]["phase_margin_deg"]) - f(&rf[rq]["achieved"]["phase_margin_deg"])).abs() <= 1e-6);
        let mut pa = json!({"form": "parallel", "Kp": r["gains"]["Kp"], "Ki": r["gains"]["Ki"]});
        if r["type"] == "PID" {
            pa["Kd"] = r["gains"]["Kd"].clone();
            pa["Tf"] = r["gains"]["Tf"].clone();
        }
        let pid = call("control.design.pid", &pa, &h);
        assert_eq!(pid["result"]["system"], r["controller"], "{rq}: controller == control.design.pid(gains) (K-4)");
    }
    let e = call(op, &read(&format!("{dir}/request.error.json")), &h);
    assert_eq!(e["error"]["code"], "E.DOMAIN.TARGET_NOT_ACHIEVABLE");
    assert_eq!(e["error"]["code"], read(&format!("{dir}/response.error.json"))["error"]["code"]);
    assert!((f(&e["error"]["details"]["required_controller_phase_deg"]) - 15.0).abs() < 1e-9);
    assert!(!e["error"]["remediation"].as_array().unwrap().is_empty(), "remediation");
}
