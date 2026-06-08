//! CLI surface tests (`assert_cmd`), deterministic regardless of whether a
//! device is attached. These run against the default (no-`hardware`) binary.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("openterface-rs").unwrap()
}

#[test]
fn version_prints_and_exits_zero() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("openterface-rs "));
}

#[test]
fn help_lists_all_commands() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("connect"))
        .stdout(predicate::str::contains("scan"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("reset"));
}

#[test]
fn no_subcommand_is_usage_error() {
    // clap exits 2 and prints help on a missing subcommand.
    bin().assert().code(2);
}

#[test]
fn short_version_flag_is_rejected() {
    // Only `--version` exists (C++ parity); `-V` is an unknown argument.
    bin().arg("-V").assert().code(2);
}

#[test]
fn scan_runs_and_exits_zero() {
    bin()
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanning for Openterface"));
}

#[test]
fn status_runs_and_exits_zero() {
    bin()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Openterface KVM Status"));
}

#[test]
fn reset_without_serial_fails_with_usage() {
    bin()
        .arg("reset")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--serial is required"));
}

#[test]
#[cfg(not(feature = "hardware"))]
fn connect_without_hardware_feature_fails_clearly() {
    // The default binary is built without the `hardware` feature.
    bin()
        .arg("connect")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("without hardware support"));
}

#[test]
fn connect_dummy_succeeds_without_hardware() {
    bin()
        .args(["connect", "--dummy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dummy mode"));
}

#[test]
fn unknown_flag_is_usage_error() {
    bin().args(["connect", "--nope"]).assert().code(2);
}
