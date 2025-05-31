use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use predicates::prelude::*;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn nonexistent_file() {
    let tempdir = tempdir().unwrap();

    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.arg("nonexistent_file.rs");
    command.current_dir(&tempdir);
    command.assert().failure().stderr(predicate::eq(
        "Error: found no files matching pattern: nonexistent_file.rs\n",
    ));
}
