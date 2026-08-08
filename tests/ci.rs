use assert_cmd::assert::OutputAssertExt;
use elaborate::std::process::CommandContext;
use std::process::{Command, Stdio};

#[test]
fn clippy() {
    Command::new("cargo")
        .args([
            "+nightly",
            "clippy",
            "--all-targets",
            "--",
            "--deny=warnings",
        ])
        .assert()
        .success();
}

#[test]
fn dylint() {
    Command::new("cargo")
        .args(["dylint", "--all", "--", "--all-targets"])
        .env("DYLINT_RUSTFLAGS", "--deny=warnings")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

#[test]
fn elaborate_disallowed_methods() {
    let status = elaborate::disallowed_methods()
        .arg("--all-targets")
        .status_wc()
        .unwrap();
    assert!(status.success());
}

#[test]
fn supply_chain() {
    supply_chain::check("supply_chain.json");
}

#[test]
fn udeps() {
    Command::new("cargo")
        .args(["+nightly", "udeps", "--all-features", "--all-targets"])
        .assert()
        .success();
}
