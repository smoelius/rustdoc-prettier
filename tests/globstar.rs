use anyhow::{ensure, Result};
use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use std::{fs::read_to_string, path::Path, process::Command};
use tempfile::tempdir;

#[test]
fn globstar() {
    let tempdir = tempdir().unwrap();
    copy_into("fixtures/globstar", &tempdir).unwrap();
    let path = tempdir.path().join("globstar");

    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.arg("**/*.rs");
    command.current_dir(&path);
    command.assert().success();

    let contents = read_to_string(path.join("src/needs_formatting/mod.rs")).unwrap();
    assert_eq!("//! Needs formatting\n", contents);
}

fn copy_into(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let mut command = Command::new("cp");
    command.arg("-r");
    command.args([from.as_ref(), to.as_ref()]);
    let status = command.status()?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}
