use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
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
