use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use similar_asserts::SimpleDiff;
use std::{fs::read_to_string, process::Command};

mod util;

#[test]
fn rustfmt_toml_in_parent_directory() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/clippy_issue_14274").unwrap();

    let src_dir = path.join("src");

    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.arg("main.rs");
    command.current_dir(&src_dir);
    command.assert().success();

    let contents_expected = read_to_string(src_dir.join("main.expected.rs")).unwrap();
    let contents_actual = read_to_string(src_dir.join("main.rs")).unwrap();
    assert!(
        contents_expected == contents_actual,
        "{}",
        SimpleDiff::from_str(&contents_expected, &contents_actual, "expected", "actual")
    );
}
