use assert_cmd::cargo::cargo_bin_cmd;

mod util;
use util::StderrNormalized;

#[test]
fn rustfmt_toml_in_parent_directory() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/clippy_issue_14274").unwrap();

    let src_dir = path.join("src");

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("main.rs");
    command.current_dir(&src_dir);
    command.assert().success();

    util::assert_expected(src_dir.join("main.rs"));
}

#[test]
fn rustfmt_toml_in_parent_directory_with_check() {
    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["main.rs", "--check"]);
    command.current_dir("fixtures/clippy_issue_14274/src");
    let assert = command.assert().failure();
    assert_eq!(
        "\
Error: failed to format main.rs:3..6

Caused by:
    `prettier` exited with code 1
",
        assert.stderr_normalized()
    );

    // smoelius: Additional check for sanity.
    assert!(util::dirty("fixtures/clippy_issue_14274").is_none());
}
