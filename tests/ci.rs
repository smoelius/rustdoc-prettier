// necessist: skip-file

use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use elaborate::std::{
    fs::{read_dir_wc, read_to_string_wc},
    process::CommandContext,
};
use std::{
    process::{Command, Stdio},
    str::FromStr,
};

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
fn each_fixture_is_in_its_own_workspace() {
    for result in read_dir_wc("fixtures").unwrap() {
        let entry = result.unwrap();
        let path = entry.path();
        let cargo_toml_path = path.join("Cargo.toml");
        let contents = read_to_string_wc(cargo_toml_path).unwrap();
        let table = toml::Table::from_str(&contents).unwrap();
        assert!(
            table.get("workspace").is_some(),
            "failed for: {}",
            path.display()
        );
    }
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

#[test]
fn version_includes_node_and_prettier_versions() {
    const RE: &str = concat!(
        "^rustdoc-prettier ",
        env!("CARGO_PKG_VERSION"),
        r" \(node v[.0-9]+, prettier [.0-9]+\)\n",
    );
    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("--version");
    command
        .assert()
        .success()
        .stdout(predicates::str::is_match(RE).unwrap());
}
