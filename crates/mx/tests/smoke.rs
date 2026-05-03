//! Black-box tests against the compiled binary.

use assert_cmd::Command;

#[test]
fn version_prints_expected_string() {
    Command::cargo_bin("mx")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("mx "));
}

#[test]
fn help_mentions_keyword() {
    Command::cargo_bin("mx")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Midnight X"))
        .stdout(predicates::str::contains("--print-default-config"));
}

#[test]
fn print_default_config_round_trips() {
    let out = Command::cargo_bin("mx")
        .unwrap()
        .arg("--print-default-config")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("[ui]"));
    assert!(text.contains("\"classic\""));
    // Parse it; must not warn.
    let (_cfg, warnings) = mx_config::parse_str(&text).expect("default must parse");
    assert!(warnings.is_empty(), "default config emits warnings: {warnings:?}");
}

#[test]
fn unknown_flag_exits_non_zero() {
    Command::cargo_bin("mx")
        .unwrap()
        .arg("--no-such-flag")
        .assert()
        .failure();
}
