use assert_cmd::Command;
use elaborate::std::process::CommandContext;
use std::env::remove_var;
use tempfile::tempdir;

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
}

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
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "markdown-link-check"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/markdown_link_check.json"
    );

    let readme_md = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");

    Command::new("npx")
        .args(["markdown-link-check", "--config", config, readme_md])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn udeps() {
    Command::new("cargo")
        .args(["+nightly", "udeps", "--all-features", "--all-targets"])
        .assert()
        .success();
}
