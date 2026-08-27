use assert_cmd::cargo::cargo_bin;
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
        #[cfg_attr(dylint_lib = "general", allow(unnecessary_conversion_for_trait))]
        let mut command = Command::new(cargo_bin!("rustdoc-prettier"));
        command.arg("src/**/*.rs");
        assert!(command.status_wc().unwrap().success());
    });
}

#[test]
fn dogfood_with_check() {
    let _lock = MUTEX.lock().unwrap();

    #[cfg_attr(dylint_lib = "general", allow(unnecessary_conversion_for_trait))]
    let mut command = Command::new(cargo_bin!("rustdoc-prettier"));
    command.args(["src/**/*.rs", "--check"]);
    assert!(command.status_wc().unwrap().success());
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
