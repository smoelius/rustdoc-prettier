use assert_cmd::assert::OutputAssertExt;
use elaborate::std::{
    env::var_wc,
    fs::{read_to_string_wc, write_wc},
    process::CommandContext,
};
use similar_asserts::SimpleDiff;
use std::{
    path::Path,
    process::{Command, ExitStatus, Stdio},
    str::FromStr,
};
use tempfile::tempdir;

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
fn supply_chain() {
    let mut command = Command::new("cargo");
    command.args(["supply-chain", "update", "--cache-max-age=0s"]);
    let _: ExitStatus = command.status_wc().unwrap();

    let mut command = Command::new("cargo");
    command.args(["supply-chain", "json", "--no-dev"]);
    let assert = command.assert().success();

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let mut value = serde_json::Value::from_str(stdout_actual).unwrap();
    remove_avatars(&mut value);
    let stdout_normalized = serde_json::to_string_pretty(&value).unwrap();

    let path_buf = Path::new(env!("CARGO_MANIFEST_DIR")).join("supply_chain.json");

    if enabled("BLESS") {
        write_wc(path_buf, stdout_normalized).unwrap();
    } else {
        let stdout_expected = read_to_string_wc(&path_buf).unwrap();

        assert!(
            stdout_expected == stdout_normalized,
            "{}",
            SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
        );
    }
}

fn remove_avatars(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
        serde_json::Value::Array(array) => {
            for value in array {
                remove_avatars(value);
            }
        }
        serde_json::Value::Object(object) => {
            object.retain(|key, value| {
                if key == "avatar" {
                    return false;
                }
                remove_avatars(value);
                true
            });
        }
    }
}

fn enabled(key: &str) -> bool {
    var_wc(key).is_ok_and(|value| value != "0")
}

#[test]
fn udeps() {
    Command::new("cargo")
        .args(["+nightly", "udeps", "--all-features", "--all-targets"])
        .assert()
        .success();
}
