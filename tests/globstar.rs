use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use predicates::prelude::*;
use std::{fs::read_to_string, process::Command};

mod util;

#[test]
fn globstar() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/globstar").unwrap();

    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.arg("**/*.rs");
    command.current_dir(&path);
    command.assert().success();

    let contents = read_to_string(path.join("src/needs_formatting/mod.rs")).unwrap();
    assert_eq!("//! Needs formatting\n", contents);
}

#[test]
fn globstar_with_check() {
    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.args(["**/*.rs", "--check"]);
    command.current_dir("fixtures/globstar");
    command.assert().failure().stderr(predicate::eq(
        "\
Error: failed to format src/needs_formatting/mod.rs:1..2

Caused by:
    prettier exited with code 1
",
    ));

    // smoelius: Additional check for sanity.
    assert!(util::dirty("fixtures/globstar").is_none());
}
