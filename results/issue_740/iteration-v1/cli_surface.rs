//! `wyrd-validate`'s CLI surface (brief #740, criteria 3 and 4) — bound flag by flag and
//! credential case by credential case, over the REAL compiled binary
//! (`env!("CARGO_BIN_EXE_wyrd-validate")`), not a copy of its parsing logic. Every
//! process here is spawned with an explicit, per-command environment
//! (`Command::env`/`env_remove`) rather than mutating the test process's own
//! environment — the same "injected I/O, no shared mutable state" reason
//! `xtask/src/main.rs:1519` passes `run_ci_steps` a lookup closure instead of reading
//! `std::env` directly (a `std::env::set_var` in one test flakes every other test
//! running in parallel in the same process).
//!
//! A handful of pure-function cases additionally call `wyrd_validate::resolve_credentials`
//! directly over an in-memory lookup closure (no subprocess) — the SAME function `main`
//! calls, driven the same "injected lookup" way, for a fast, non-flaky proof of the
//! fallback ordering that does not depend on spawning a process at all.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::process::Command;

use wyrd_validate::{resolve_credentials, CredentialSource};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_wyrd-validate")
}

/// The ten required flags, each with a distinct, greppable value so a per-flag
/// assertion cannot pass by coincidence (no two values share a substring).
fn base_flags() -> Vec<(&'static str, &'static str)> {
    vec![
        ("endpoint", "https://s3.example.invalid"),
        ("region", "us-validate-1"),
        ("bucket", "bucket-alpha-9f2"),
        ("scenario", "smoke-scenario-7c1"),
        ("duration", "30s-marker-b4e"),
        ("workers", "workers-marker-1a3"),
        ("seed", "seed-marker-de9"),
        ("out", "/tmp/out-marker-5b6"),
        ("run-id", "run-id-marker-88c"),
        ("driver-placement", "placement-marker-f01"),
    ]
}

fn args_for(flags: &[(&str, &str)]) -> Vec<String> {
    let mut args = Vec::new();
    for (name, value) in flags {
        args.push(format!("--{name}"));
        args.push((*value).to_string());
    }
    args
}

/// Credentials never appear on the required-flags list, so every base invocation still
/// needs AWS creds set for the CLI-surface tests below to reach a clean exit 0.
fn aws_creds() -> Vec<(&'static str, &'static str)> {
    vec![
        ("AWS_ACCESS_KEY_ID", "AKIAEXAMPLEBASE"),
        ("AWS_SECRET_ACCESS_KEY", "base-secret-should-never-print"),
    ]
}

// ─── criterion 3: the CLI surface, bound flag by flag ──────────────────────────────

#[test]
fn all_ten_flags_exit_zero_and_each_is_echoed_with_its_given_value() {
    let flags = base_flags();
    let output = Command::new(bin())
        .args(args_for(&flags))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Assert PER FLAG, so an implementation that parses three and ignores seven fails:
    // each flag's exact `--name value` pair must appear, not merely the bare value.
    for (name, value) in &flags {
        let needle = format!("--{name} {value}");
        assert!(
            stdout.contains(&needle),
            "missing `{needle}` in resolved-configuration output:\n{stdout}"
        );
    }
}

#[test]
fn a_missing_required_flag_exits_nonzero_naming_it_on_stderr() {
    let flags = base_flags();
    for (omit_name, _) in &flags {
        let subset: Vec<(&str, &str)> = flags
            .iter()
            .copied()
            .filter(|(name, _)| name != omit_name)
            .collect();
        let output = Command::new(bin())
            .args(args_for(&subset))
            .envs(aws_creds())
            .output()
            .expect("failed to spawn wyrd-validate");
        assert!(
            !output.status.success(),
            "omitting --{omit_name} must exit non-zero"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("--{omit_name}")),
            "stderr must name the missing flag `--{omit_name}`: {stderr}"
        );
    }
}

#[test]
fn an_unrecognised_flag_exits_nonzero_naming_it() {
    let mut flags = base_flags();
    let mut args = args_for(&flags);
    args.push("--totally-bogus-flag".to_string());
    args.push("value".to_string());
    // Also cover the case where a mistyped flag REPLACES a known one, which is the
    // concrete footgun the Design section names: a misspelled `--duration` silently
    // ignored would misreport how long the run actually measured.
    flags.retain(|(name, _)| *name != "duration");
    let mut mistyped_args = args_for(&flags);
    mistyped_args.push("--duration-zz".to_string());
    mistyped_args.push("30s".to_string());

    for candidate in [args, mistyped_args] {
        let output = Command::new(bin())
            .args(&candidate)
            .envs(aws_creds())
            .output()
            .expect("failed to spawn wyrd-validate");
        assert!(
            !output.status.success(),
            "an unrecognised flag must exit non-zero: {candidate:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("bogus") || stderr.contains("duration-zz"),
            "stderr must name the unrecognised flag: {stderr}"
        );
    }
}

// ─── criterion 4: credential resolution, both directions ───────────────────────────

const AWS_ID: &str = "AKIA_REAL_AWS_ID";
const AWS_SECRET: &str = "aws-secret-never-printed-6f3a";
const WYRD_ID: &str = "wyrd-gateway-id-2b7";
const WYRD_SECRET: &str = "wyrd-secret-never-printed-9d1";

/// Spawn `wyrd-validate` with the base flags plus an EXPLICIT credential environment —
/// every one of the four names is either set to a fixture value or removed, so no
/// ambient variable from the test runner's own environment can leak into the case
/// under test.
fn run_with_credential_env(env: &[(&str, Option<&str>)]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(args_for(&base_flags()));
    for name in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "WYRD_S3_ACCESS_KEY",
        "WYRD_S3_SECRET_KEY",
    ] {
        cmd.env_remove(name);
    }
    for (name, value) in env {
        match value {
            Some(v) => {
                cmd.env(name, v);
            }
            None => {
                cmd.env_remove(name);
            }
        }
    }
    cmd.output().expect("failed to spawn wyrd-validate")
}

#[test]
fn aws_credentials_present_resolve_to_the_aws_id_and_source() {
    let output = run_with_credential_env(&[
        ("AWS_ACCESS_KEY_ID", Some(AWS_ID)),
        ("AWS_SECRET_ACCESS_KEY", Some(AWS_SECRET)),
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(AWS_ID), "{stdout}");
    assert!(stdout.contains("AWS_ACCESS_KEY_ID"), "{stdout}");
    assert!(
        !stdout.contains(AWS_SECRET)
            && !String::from_utf8_lossy(&output.stderr).contains(AWS_SECRET),
        "the secret must never appear in process output:\nstdout: {stdout}"
    );
}

#[test]
fn wyrd_fallback_credentials_resolve_when_aws_is_absent() {
    let output = run_with_credential_env(&[
        ("WYRD_S3_ACCESS_KEY", Some(WYRD_ID)),
        ("WYRD_S3_SECRET_KEY", Some(WYRD_SECRET)),
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(WYRD_ID), "{stdout}");
    assert!(stdout.contains("WYRD_S3_ACCESS_KEY"), "{stdout}");
    assert!(
        !stdout.contains(WYRD_SECRET)
            && !String::from_utf8_lossy(&output.stderr).contains(WYRD_SECRET),
        "the secret must never appear in process output:\nstdout: {stdout}"
    );
}

#[test]
fn both_present_aws_wins() {
    let output = run_with_credential_env(&[
        ("AWS_ACCESS_KEY_ID", Some(AWS_ID)),
        ("AWS_SECRET_ACCESS_KEY", Some(AWS_SECRET)),
        ("WYRD_S3_ACCESS_KEY", Some(WYRD_ID)),
        ("WYRD_S3_SECRET_KEY", Some(WYRD_SECRET)),
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(AWS_ID), "AWS must win: {stdout}");
    assert!(!stdout.contains(WYRD_ID), "AWS must win: {stdout}");
    assert!(
        !stdout.contains(AWS_SECRET) && !stdout.contains(WYRD_SECRET),
        "neither secret may ever appear in process output:\n{stdout}"
    );
}

#[test]
fn neither_present_exits_nonzero_naming_what_to_set() {
    let output = run_with_credential_env(&[]);
    assert!(
        !output.status.success(),
        "no credentials present must refuse, not silently proceed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("AWS_ACCESS_KEY_ID") || stderr.contains("WYRD_S3_ACCESS_KEY"),
        "stderr must name what to set: {stderr}"
    );
}

// ─── the pure fallback logic, over the injected lookup, no subprocess ──────────────

fn lookup_over(map: BTreeMap<&'static str, &'static str>) -> impl FnMut(&str) -> Option<String> {
    move |name| map.get(name).map(|v| v.to_string())
}

#[test]
fn resolve_credentials_prefers_aws_over_wyrd_over_the_injected_lookup() {
    let mut map = BTreeMap::new();
    map.insert("AWS_ACCESS_KEY_ID", AWS_ID);
    map.insert("AWS_SECRET_ACCESS_KEY", AWS_SECRET);
    map.insert("WYRD_S3_ACCESS_KEY", WYRD_ID);
    map.insert("WYRD_S3_SECRET_KEY", WYRD_SECRET);
    let mut lookup = lookup_over(map);
    let creds = resolve_credentials(&mut lookup).expect("both pairs present must resolve");
    assert_eq!(creds.access_key_id, AWS_ID);
    assert_eq!(creds.source, CredentialSource::Aws);
}

#[test]
fn resolve_credentials_falls_back_to_wyrd_over_the_injected_lookup() {
    let mut map = BTreeMap::new();
    map.insert("WYRD_S3_ACCESS_KEY", WYRD_ID);
    map.insert("WYRD_S3_SECRET_KEY", WYRD_SECRET);
    let mut lookup = lookup_over(map);
    let creds = resolve_credentials(&mut lookup).expect("wyrd pair alone must resolve");
    assert_eq!(creds.access_key_id, WYRD_ID);
    assert_eq!(creds.source, CredentialSource::Wyrd);
}

#[test]
fn resolve_credentials_refuses_over_the_injected_lookup_when_neither_pair_is_present() {
    let mut lookup = lookup_over(BTreeMap::new());
    let err = resolve_credentials(&mut lookup).expect_err("neither pair present must refuse");
    assert!(err.contains("AWS_ACCESS_KEY_ID") || err.contains("WYRD_S3_ACCESS_KEY"));
}
