use assert_cmd::cargo::CommandCargoExt;
use elaborate::std::{env::var_wc, process::CommandContext};
use std::{
    io::{Write, stderr},
    process::Command,
    sync::Mutex,
};

mod util;

static MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn dogfood() {
    let _lock = MUTEX.lock().unwrap();

    preserves_cleanliness("dogfood", || {
        let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
        command.arg("src/**/*.rs");
        let status = command.status_wc().unwrap();
        assert!(status.success());
    });
}

#[test]
fn dogfood_with_check() {
    let _lock = MUTEX.lock().unwrap();

    let mut command = Command::cargo_bin("rustdoc-prettier").unwrap();
    command.args(["src/**/*.rs", "--check"]);
    let status = command.status_wc().unwrap();
    assert!(status.success());
}

fn preserves_cleanliness(test_name: &str, f: impl FnOnce()) {
    // smoelius: Do not skip tests when running on GitHub.
    if var_wc("CI").is_err() && util::dirty(".").is_some() {
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
