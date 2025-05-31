use assert_cmd::cargo::CommandCargoExt;
use std::{
    env::var,
    io::{Write, stderr},
    process::Command,
    sync::Mutex,
};

mod util;

#[test]
fn dogfood() {
    preserves_cleanliness("dogfood", || {
        let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
        command.arg("src/**/*.rs");
        let status = command.status().unwrap();
        assert!(status.success());
    });
}

static MUTEX: Mutex<()> = Mutex::new(());

fn preserves_cleanliness(test_name: &str, f: impl FnOnce()) {
    let _lock = MUTEX.lock().unwrap();

    // smoelius: Do not skip tests when running on GitHub.
    if var("CI").is_err() && util::dirty(".").is_some() {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `{test_name}` test as repository is dirty"
        )
        .unwrap();
        return;
    }

    f();

    if let Some(stdout) = util::dirty(".") {
        panic!("{}", stdout);
    }
}
