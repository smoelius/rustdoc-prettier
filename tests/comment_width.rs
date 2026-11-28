use assert_cmd::cargo::cargo_bin_cmd;

mod util;
use util::StderrNormalized;

#[test]
fn comment_width() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/three_modules").unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("src/lib.rs");
    command.current_dir(&path);
    command.assert().success();

    util::assert_expected(path.join("src/lib.rs"));
}

#[test]
fn comment_width_with_check() {
    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["src/lib.rs", "--check"]);
    command.current_dir("fixtures/three_modules");
    let assert = command.assert().failure();
    assert_eq!(
        "\
Error: failed to format src/lib.rs:1..2

Caused by:
    `prettier` exited with code 1
",
        assert.stderr_normalized()
    );

    // smoelius: Additional check for sanity.
    assert!(util::dirty("fixtures/three_modules").is_none());
}
