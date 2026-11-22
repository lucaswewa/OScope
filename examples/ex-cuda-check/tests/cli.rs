//! Required-device execution must differ from an ordinary fallback smoke run.

use std::process::Command;

#[test]
fn an_unavailable_device_is_a_successful_cpu_fallback() {
    let run = Command::new(env!("CARGO_BIN_EXE_ex-cuda-check"))
        .args(["--device", "99", "--smoke"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{run:?}");
    assert!(stdout.contains("no GPU in use"), "{stdout}");
    assert!(stdout.contains("smoke: ok"), "{stdout}");
}

#[test]
fn a_required_unavailable_device_is_a_failure() {
    let run = Command::new(env!("CARGO_BIN_EXE_ex-cuda-check"))
        .args(["--device", "99", "--require-cuda", "--smoke"])
        .output()
        .unwrap();
    assert!(!run.status.success(), "{run:?}");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("CUDA required but unavailable"),
        "{run:?}"
    );
    assert!(!String::from_utf8_lossy(&run.stdout).contains("smoke: ok"));
}
