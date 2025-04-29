#![allow(clippy::io_other_error, clippy::unnecessary_debug_formatting)]

// smoelius: `resolve_project_file` is based on the function of the same name from:
// https://github.com/rust-lang/rustfmt/blob/b23b69900eab1260be510b2bd8922f4b6de6cf1e/src/config/mod.rs#L313-L354
//
// However, the version here does not check the user's home or configuration directories.
//
// `get_toml_path` was copied verbatim from the just mentioned commit:
// https://github.com/rust-lang/rustfmt/blob/b23b69900eab1260be510b2bd8922f4b6de6cf1e/src/config/mod.rs#L451-L475

use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Try to find a project file in the given directory and its parents.
/// Returns the path of the nearest project file if one exists,
/// or `None` if no project file was found.
pub fn resolve_project_file(dir: &Path) -> Result<Option<PathBuf>, Error> {
    let mut current = if dir.is_relative() {
        env::current_dir()?.join(dir)
    } else {
        dir.to_path_buf()
    };

    current = fs::canonicalize(current)?;

    loop {
        match get_toml_path(&current) {
            Ok(Some(path)) => return Ok(Some(path)),
            Err(e) => return Err(e),
            _ => (),
        }

        // If the current directory has no parent, we're done searching.
        if !current.pop() {
            break;
        }
    }

    Ok(None)
}

// Check for the presence of known config file names (`rustfmt.toml`, `.rustfmt.toml`) in `dir`
//
// Return the path if a config file exists, empty if no file exists, and Error for IO errors
fn get_toml_path(dir: &Path) -> Result<Option<PathBuf>, Error> {
    const CONFIG_FILE_NAMES: [&str; 2] = [".rustfmt.toml", "rustfmt.toml"];
    for config_file_name in &CONFIG_FILE_NAMES {
        let config_file = dir.join(config_file_name);
        match fs::metadata(&config_file) {
            // Only return if it's a file to handle the unlikely situation of a directory named
            // `rustfmt.toml`.
            Ok(ref md) if md.is_file() => return Ok(Some(config_file.canonicalize()?)),
            // Return the error if it's something other than `NotFound`; otherwise we didn't
            // find the project file yet, and continue searching.
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    let ctx = format!("Failed to get metadata for config file {:?}", &config_file);
                    let err = anyhow::Error::new(e).context(ctx);
                    return Err(Error::new(ErrorKind::Other, err));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}
