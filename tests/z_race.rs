use anyhow::Result;
use assert_cmd::cargo::cargo_bin_cmd;
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
                        eprintln!("{error}");
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
        command.assert().success();
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
