#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![allow(dead_code)]

use anyhow::{Result, ensure};
use elaborate::std::{path::PathContext, process::CommandContext};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::{TempDir, tempdir};

pub fn copy_into_tempdir(from: impl AsRef<Path>) -> Result<(TempDir, PathBuf)> {
    let from = from.as_ref();
    let filename = from.file_name_wc()?;
    let tempdir = tempdir()?;
    copy_into(from, &tempdir)?;
    let path_buf = tempdir.path().join(filename);
    Ok((tempdir, path_buf))
}

pub fn copy_into(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let mut command = Command::new("cp");
    command.arg("-r");
    command.args([from.as_ref(), to.as_ref()]);
    let status = command.status_wc()?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
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
