use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::fs::read_to_string_wc;

mod util;
use util::StderrNormalized;

#[test]
fn globstar() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/globstar").unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("**/*.rs");
    command.current_dir(&path);
    command.assert().success();

    let contents = read_to_string_wc(path.join("src/needs_formatting/mod.rs")).unwrap();
    assert_eq!("//! Needs formatting\n", contents);

    let contents = read_to_string_wc(path.join("src/.hidden.rs")).unwrap();
    assert_eq!("//!  Needs formatting\n", contents);
}

#[test]
fn globstar_with_check() {
    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["**/*.rs", "--check"]);
    command.current_dir("fixtures/globstar");
    let assert = command.assert().failure();
    assert_eq!(
        "\
Error: failed to format src/needs_formatting/mod.rs:1..2

Caused by:
    `prettier` exited with code 1
",
        assert.stderr_normalized()
    );

    // smoelius: Additional check for sanity.
    assert!(util::dirty("fixtures/globstar").is_none());
}

#[test]
fn overlapping_patterns() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/globstar").unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["**/*.rs", "**/*.rs"]);
    command.current_dir(&path);
    command.assert().success();

    let contents = read_to_string_wc(path.join("src/needs_formatting/mod.rs")).unwrap();
    assert_eq!("//! Needs formatting\n", contents);
}
