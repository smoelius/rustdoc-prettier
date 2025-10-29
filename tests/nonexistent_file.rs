use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn nonexistent_file() {
    let tempdir = tempdir().unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("nonexistent_file.rs");
    command.current_dir(&tempdir);
    command.assert().failure().stderr(predicate::eq(
        "Error: found no files matching pattern: nonexistent_file.rs\n",
    ));
}
