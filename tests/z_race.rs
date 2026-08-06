use anyhow::Result;
use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use elaborate::std::fs::{create_dir_wc, write_wc};
use std::{
    fs::remove_dir_all,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
};
use tempfile::tempdir;

const N_ITERATIONS: usize = 100;

static EXIT: AtomicBool = AtomicBool::new(false);

// Verify that `rustdoc-prettier` succeeds even when the files matched by its glob patterns are
// removed while it runs. One thread repeatedly creates and removes a subdirectory containing a
// source file, while the main thread repeatedly formats `**/*.rs`. A file that vanishes this way
// should be warned about and skipped, not treated as an error.
//
// The motivation for this test is https://github.com/smoelius/rustdoc-prettier/issues/59. Dylint's
// CI ran `rustdoc-prettier './**/*.rs'` while Cargo was creating and removing files under `target`,
// so paths returned by `glob` could vanish before they were read.
#[test]
fn race() {
    let tempdir = tempdir().unwrap();

    // smoelius: `rustdoc-prettier` complains if there are no source files to format.
    create_source_file(tempdir.path()).unwrap();

    // smoelius: Hack to get `tempdir`'s path without holding a reference to `tempdir`.
    let dir = tempdir.path().to_path_buf();

    let handle = thread::spawn(move || {
        loop {
            if EXIT.load(Ordering::SeqCst) {
                break;
            }
            let subdir = create_subdir_with_source_file(&dir).unwrap();
            loop {
                // smoelius: `subdir` could be non-empty because `rustdoc-prettier` wrote into it
                // while it was being removed. Keep trying until the directory is removed
                // successfully.
                #[allow(clippy::disallowed_methods)]
                match remove_dir_all(&subdir) {
                    Ok(()) => break,
                    Err(error) => {
                        eprintln!("Warning: observed {error} while removing directory");
                        assert_eq!(io::ErrorKind::DirectoryNotEmpty, error.kind());
                    }
                }
            }
        }
    });

    for i in 0..N_ITERATIONS {
        dbg!(i);
        let mut command = cargo_bin_cmd!("rustdoc-prettier");
        command.arg("**/*.rs");
        command.current_dir(&tempdir);
        let output = command.output().unwrap();
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        output.assert().success();
    }

    EXIT.store(true, Ordering::SeqCst);

    handle.join().unwrap();
}

fn create_source_file(dir: &Path) -> Result<()> {
    write_wc(dir.join("a.rs"), "///  A comment in need of formatting")?;
    Ok(())
}

fn create_subdir_with_source_file(dir: &Path) -> Result<PathBuf> {
    let subdir = dir.join("subdir");
    create_dir_wc(&subdir)?;
    write_wc(
        subdir.join("b.rs"),
        "///  Another comment in need of formatting",
    )?;
    Ok(subdir)
}
