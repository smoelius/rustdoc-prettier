use assert_cmd::{assert::OutputAssertExt, cargo::cargo_bin_cmd};
use elaborate::std::fs::read_to_string_wc;
use predicates::str::contains;
use regex::Regex;
use std::process::Command;
use tempfile::tempdir;

#[cfg_attr(dylint_lib = "supplementary", allow(abs_home_path))]
const README_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");

#[cfg_attr(windows, ignore = "testing on Unix-like platforms is sufficient")]
#[test]
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/markdown_link_check.json"
    );

    Command::new("npx")
        .args(["markdown-link-check", "--config", config, README_MD])
        // necessist: skip
        .current_dir(&tempdir)
        .assert()
        .success()
        .stdout(contains("links checked."));
}

#[test]
fn readme_contains_usage() {
    let readme = read_to_string_wc(README_MD).unwrap();

    let assert = cargo_bin_cmd!(env!("CARGO_PKG_NAME"))
        .arg("--help")
        .assert();
    let stdout = &assert.get_output().stdout;

    let usage = std::str::from_utf8(stdout)
        .unwrap()
        .split_inclusive('\n')
        .skip(2)
        .collect::<String>();

    assert_ne!(usage, "");
    assert!(readme.contains(&usage));
}

#[test]
fn readme_reference_links_are_sorted() {
    let re = Regex::new(r"^\[[^\]]*\]:").unwrap();
    let readme = read_to_string_wc(README_MD).unwrap();
    let links = readme
        .lines()
        .filter(|line| re.is_match(line))
        .collect::<Vec<_>>();
    let mut links_sorted = links.clone();
    // necessist: skip
    links_sorted.sort_unstable();
    assert_eq!(links_sorted, links);
}

#[test]
fn readme_reference_links_are_used() {
    let re = Regex::new(r"(?m)^(\[[^\]]*\]):").unwrap();
    let readme = read_to_string_wc(README_MD).unwrap();
    for captures in re.captures_iter(&readme) {
        assert_eq!(2, captures.len());
        let m = captures.get(1).unwrap();
        assert!(
            readme[..m.start()].contains(m.as_str()),
            "{} is unused",
            m.as_str()
        );
    }
}
