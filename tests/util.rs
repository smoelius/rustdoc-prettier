use anyhow::{Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::{TempDir, tempdir};

pub fn copy_into_tempdir(from: impl AsRef<Path>) -> Result<(TempDir, PathBuf)> {
    let from = from.as_ref();
    let Some(filename) = from.file_name() else {
        bail!("`{}` has no filename", from.display());
    };
    let tempdir = tempdir()?;
    copy_into(from, &tempdir)?;
    let path_buf = tempdir.path().join(filename);
    Ok((tempdir, path_buf))
}

pub fn copy_into(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let mut command = Command::new("cp");
    command.arg("-r");
    command.args([from.as_ref(), to.as_ref()]);
    let status = command.status()?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}
