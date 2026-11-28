use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::fs::read_to_string_wc;

mod util;

#[test]
fn unterminated_comment() {
    let (_tempdir, path) = util::copy_into_tempdir("fixtures/unterminated_comment").unwrap();

    let contents = read_to_string_wc(path.join("src/lib.rs")).unwrap();
    assert!(!contents.ends_with('\n'));

    let mut command = cargo_bin_cmd!("rustdoc-prettier");
    command.arg("src/lib.rs");
    command.current_dir(&path);
    command.assert().success();

    util::assert_expected(path.join("src/lib.rs"));
}
