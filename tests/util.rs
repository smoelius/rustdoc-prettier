#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![allow(dead_code)]

use anyhow::{Context, Result, ensure};
use assert_cmd::assert::Assert;
use elaborate::std::{
    ffi::OsStrContext, fs::read_to_string_wc, path::PathContext, process::CommandContext,
};
use methodify::methodify;
use similar_asserts::SimpleDiff;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::{TempDir, tempdir};

pub fn copy_into_tempdir(from: impl AsRef<Path>) -> Result<(TempDir, PathBuf)> {
    let from = from.as_ref();
    let filename = from.file_name_wc()?;
    let tempdir = tempdir().with_context(|| "failed to create temporary directory")?;
    copy_into(from, &tempdir)?;
    let path_buf = tempdir.path().join(filename);
    Ok((tempdir, path_buf))
}

fn copy_into(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let mut command = Command::new("cp");
    command.arg("-r");
    command.args([from.as_ref(), to.as_ref()]);
    let status = command.status_wc()?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}

pub fn assert_expected(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let extension = path.extension_wc().and_then(OsStr::to_str_wc).unwrap();
    let path_expected = path.with_extension(format!("expected.{extension}"));
    let contents_expected = read_to_string_wc(path_expected).unwrap();
    let contents_actual = read_to_string_wc(path).unwrap();
    assert!(
        contents_expected == contents_actual,
        "{}",
        SimpleDiff::from_str(&contents_expected, &contents_actual, "expected", "actual")
    );
}

pub fn dirty(path: impl AsRef<Path>) -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--exit-code"])
        .arg(path.as_ref())
        .output_wc()
        .unwrap();

    if output.status.success() {
        None
    } else {
        Some(String::from_utf8(output.stdout).unwrap())
    }
}

#[methodify]
#[must_use]
pub fn stderr_normalized(assert: &Assert) -> String {
    let stderr = str::from_utf8(&assert.get_output().stderr).unwrap();
    stderr.replace('\\', "/")
}
