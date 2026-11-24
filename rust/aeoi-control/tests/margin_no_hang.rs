//! Regression: control.analysis.margin must never hang. A stale `aeoi-call control.analysis.margin m2.json`
//! (discrete 0.1/(z²−1.5z+0.7), ts = 0.1) from an early build ran for ~11 h. Every loop in margin and in
//! the root finder is bounded (balancing sweeps, QR iterations, Newton/secant steps, grid audit), and the
//! grid audit also polls the cooperative deadline. Each case here runs on a watchdog thread.
use aeoi_core::HostCtx;
use serde_json::{json, Value};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn call_with_watchdog(req: Value, limit: Duration) -> Value {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reg = aeoi_control::registry();
        let op = aeoi_control::find(&reg, "control.analysis.margin").unwrap();
        let _ = tx.send(op.call(&req, &HostCtx::default()));
    });
    rx.recv_timeout(limit).unwrap_or_else(|_| panic!("margin did not return within {limit:?}"))
}

fn tf(num: &[f64], den: &[f64], ts: f64) -> Value {
    json!({"system": {"kind": "tf", "num": num, "den": den, "ts": ts}})
}

#[test]
fn margin_returns_on_the_m2_input_and_adversarial_systems() {
    let cases = vec![
        tf(&[0.1], &[1.0, -1.5, 0.7], 0.1), // the m2.json input
        tf(&[0.1], &[1.0, -1.5, 0.7], 1e-6),
        tf(&[1.0], &[1.0, -2.0, 1.0], 0.1),                    // double pole on the unit circle
        tf(&[1.0, -1.0], &[1.0, -1.0 + 1e-12], 0.1),           // near pole/zero cancellation at z = 1
        tf(&[1e150], &[1.0, 1e-150, 1e-300], 0.0),             // extreme scaling
        tf(&[1.0], &[1e-300, 1.0, 1e300], 0.0),
        tf(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0], &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0], 0.0), // lightly damped, imag-axis poles
        tf(&(0..21).map(|k| if k == 0 { 1.0 } else { 0.0 }).collect::<Vec<_>>(), &(0..22).map(|k| 1.0 / (1.0 + k as f64)).collect::<Vec<_>>(), 0.0),
        tf(&[1.0], &[1.0, 0.0, 0.0, 0.0], 0.0), // triple integrator
    ];
    for (i, c) in cases.into_iter().enumerate() {
        let t0 = Instant::now();
        let e = call_with_watchdog(c, Duration::from_secs(20));
        assert!(e["status"].is_string(), "case {i}: no envelope");
        assert!(t0.elapsed() < Duration::from_secs(20), "case {i}");
    }
}

#[test]
fn margin_honours_a_tiny_deadline_instead_of_running_on() {
    // a high-order system with exec.timeout_ms = 0: either it finishes immediately or reports
    // E.RESOURCE.TIMEOUT; it must not keep running.
    let mut req = tf(&[1.0], &(0..41).map(|k| 1.0 / (1.0 + k as f64)).collect::<Vec<_>>(), 0.0);
    req["exec"] = json!({"timeout_ms": 0});
    let e = call_with_watchdog(req, Duration::from_secs(20));
    assert!(e["status"] != "error" || e["error"]["code"] == "E.RESOURCE.TIMEOUT" || e["error"]["category"] == "validation", "{}", e["error"]);
}
