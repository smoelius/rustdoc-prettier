use assert_cmd::Command;
use std::{
    env::var,
    io::{stderr, Write},
    sync::Mutex,
};

#[test]
fn dogfood() {
    preserves_cleanliness("dogfood", || {
        Command::cargo_bin("rustdoc-prettier")
            .unwrap()
            .assert()
            .success();
    });
}

static MUTEX: Mutex<()> = Mutex::new(());

fn preserves_cleanliness(test_name: &str, f: impl FnOnce()) {
    let _lock = MUTEX.lock().unwrap();

    // smoelius: Do not skip tests when running on GitHub.
    if var("CI").is_err() && dirty().is_some() {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `{test_name}` test as repository is dirty"
        )
        .unwrap();
        return;
    }

    f();

    if let Some(stdout) = dirty() {
        panic!("{}", stdout);
    }
}

fn dirty() -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--exit-code"])
        .output()
        .unwrap();

    if output.status.success() {
        None
    } else {
        Some(String::from_utf8(output.stdout).unwrap())
    }
}