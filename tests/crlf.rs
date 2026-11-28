use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::fs::{read_to_string_wc, write_wc};
use tempfile::tempdir;

#[test]
fn crlf() {
    let tempdir = tempdir().unwrap();
    let path_buf = tempdir.path().join("lib.rs");
    #[rustfmt::skip]
    let contents = concat!(
        "//!  First\r\n",
        "//!\r\n",
        "//! Second\r\n",
    );
    write_wc(&path_buf, contents).unwrap();

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.args(["lib.rs", "--end-of-line=crlf"]);
    command.current_dir(&tempdir);
    command.assert().success();

    let actual = read_to_string_wc(&path_buf).unwrap();
    #[rustfmt::skip]
    let expected = concat!(
        "//! First\r\n",
        "//!\r\n",
        "//! Second\r\n",
    );
    assert_eq!(expected, actual);

    command.arg("--check");
    command.assert().success();
}
