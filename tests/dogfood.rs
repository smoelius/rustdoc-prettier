use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::env::var_wc;
use std::{
    io::{Write, stderr},
    sync::Mutex,
};

mod util;

static MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn dogfood() {
    let _lock = MUTEX.lock().unwrap();

    preserves_cleanliness("dogfood", || {
        let mut command = cargo_bin_cmd!("rustdoc-prettier");
        command.arg("src/**/*.rs");
        command.assert().success();
    });
}

#[test]
fn dogfood_with_check() {
    let _lock = MUTEX.lock().unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["src/**/*.rs", "--check"]);
    command.assert().success();
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
