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
//!
//! The strict-input cases below cover BOTH token positions and both input surfaces,
//! because a strict parser that is strict in only one of them is not strict: an unknown
//! `--flag` where a flag is expected, a `--`-prefixed token where a VALUE is expected (per
//! flag), a repeated flag, an empty value, and half a credential pair.

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

/// The other half of "an unrecognised flag is refused": a flag-shaped token sitting in a
/// VALUE slot. Checking unknown tokens only where a flag is expected leaves the typo one
/// slot to the left — `--endpoint --totally-bogus …` parsing as "the endpoint is the
/// string `--totally-bogus`", exiting 0, and echoing the typo back as configuration. That
/// is the same misreported run the strict policy exists to prevent, reached by a
/// different route, so it is asserted for EVERY flag, not just the first: a check written
/// into one arm of the parser would otherwise pass.
#[test]
fn a_flag_shaped_token_in_a_value_slot_is_refused_for_every_flag() {
    const BOGUS: &str = "--totally-bogus";
    for (target, _) in base_flags() {
        let mutated: Vec<(&str, &str)> = base_flags()
            .into_iter()
            .map(|(name, value)| {
                if name == target {
                    (name, BOGUS)
                } else {
                    (name, value)
                }
            })
            .collect();
        let output = Command::new(bin())
            .args(args_for(&mutated))
            .envs(aws_creds())
            .output()
            .expect("failed to spawn wyrd-validate");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !output.status.success(),
            "`--{target} {BOGUS}` must exit non-zero, not be swallowed as a value; \
             stdout: {stdout}"
        );
        assert!(
            !stdout.contains(BOGUS),
            "`{BOGUS}` must never be echoed back as `--{target}`'s resolved value: {stdout}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(BOGUS) && stderr.contains(&format!("--{target}")),
            "stderr must name both the flag left without a value and the offending token: \
             {stderr}"
        );
    }
}

/// The everyday shape of the same slip: a flag typed with its value forgotten, so the
/// NEXT (perfectly valid) flag lands in the value slot. `--endpoint --region us-east-1`
/// must not resolve to "endpoint = `--region`, region missing" — it names both the flag
/// left without a value and the token that was not one.
#[test]
fn a_flag_left_without_its_value_does_not_consume_the_next_flag() {
    let mut args = vec!["--endpoint".to_string()];
    args.extend(args_for(
        &base_flags()
            .into_iter()
            .filter(|(name, _)| *name != "endpoint")
            .collect::<Vec<_>>(),
    ));
    let output = Command::new(bin())
        .args(&args)
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        !output.status.success(),
        "a flag left without its value must exit non-zero: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--endpoint") && stderr.contains("--region"),
        "stderr must name the flag awaiting a value and the flag that is not one: {stderr}"
    );
}

/// A single `-` may legitimately lead a value (a negative `--seed`), so the refusal above
/// is on the `--` flag prefix specifically — not on "anything dash-shaped". Without this,
/// tightening the check to `starts_with('-')` would look equally correct and would reject
/// a legal invocation.
#[test]
fn a_single_dash_value_is_still_accepted() {
    let flags: Vec<(&str, &str)> = base_flags()
        .into_iter()
        .map(|(name, value)| {
            if name == "seed" {
                (name, "-12345")
            } else {
                (name, value)
            }
        })
        .collect();
    let output = Command::new(bin())
        .args(args_for(&flags))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        output.status.success(),
        "a negative seed is a legal value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("--seed -12345"),
        "the negative seed must be echoed as given"
    );
}

/// `--duration 30s --duration 30m` silently keeping one of the two would report a run
/// that is not the one asked for — the same class as the swallowed typo above.
#[test]
fn a_repeated_flag_is_refused_naming_it() {
    let mut args = args_for(&base_flags());
    args.push("--duration".to_string());
    args.push("30m-second-value".to_string());
    let output = Command::new(bin())
        .args(&args)
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        !output.status.success(),
        "a repeated flag must exit non-zero: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--duration"),
        "stderr must name the repeated flag: {stderr}"
    );
}

/// `--bucket ""` is not a configured bucket; echoing one back as resolved configuration
/// is the same silent misreport as swallowing a typo.
#[test]
fn an_empty_value_is_refused_naming_the_flag() {
    let flags: Vec<(&str, &str)> = base_flags()
        .into_iter()
        .map(|(name, value)| {
            if name == "bucket" {
                (name, "")
            } else {
                (name, value)
            }
        })
        .collect();
    let output = Command::new(bin())
        .args(args_for(&flags))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        !output.status.success(),
        "an empty value must exit non-zero: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--bucket"),
        "stderr must name the flag given an empty value: {stderr}"
    );
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

/// Half a pair is the credential-shaped form of the swallowed typo: an operator who
/// exported `AWS_ACCESS_KEY_ID` and mistyped the secret's name has stated an intent, and
/// signing the run with the OTHER pair's identity instead would attribute the results to
/// a credential they never chose. Asserted over the injected lookup AND (below) over the
/// real binary, both directions of the pair.
#[test]
fn resolve_credentials_refuses_half_a_pair_rather_than_falling_back() {
    for (present, missing, other_pair_id) in [
        ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", WYRD_ID),
        ("AWS_SECRET_ACCESS_KEY", "AWS_ACCESS_KEY_ID", WYRD_ID),
    ] {
        let mut map = BTreeMap::new();
        map.insert(present, "half-a-pair-value");
        // The complete fallback pair IS available — the point is that it is not silently
        // taken while the first pair is half-stated.
        map.insert("WYRD_S3_ACCESS_KEY", WYRD_ID);
        map.insert("WYRD_S3_SECRET_KEY", WYRD_SECRET);
        let mut lookup = lookup_over(map);
        let err = resolve_credentials(&mut lookup)
            .map(|c| c.access_key_id)
            .expect_err("half a pair must refuse, not fall through");
        assert!(
            err.contains(missing) && !err.contains(other_pair_id),
            "the error must name the missing half ({missing}): {err}"
        );
    }
}

#[test]
fn resolve_credentials_refuses_a_present_but_empty_name() {
    let mut map = BTreeMap::new();
    map.insert("AWS_ACCESS_KEY_ID", "");
    map.insert("AWS_SECRET_ACCESS_KEY", AWS_SECRET);
    let mut lookup = lookup_over(map);
    let err = resolve_credentials(&mut lookup).expect_err("an empty credential must refuse");
    assert!(err.contains("AWS_ACCESS_KEY_ID"), "{err}");
}

#[test]
fn half_an_aws_pair_refuses_over_the_real_binary_instead_of_using_the_wyrd_identity() {
    let output = run_with_credential_env(&[
        ("AWS_ACCESS_KEY_ID", Some(AWS_ID)),
        ("WYRD_S3_ACCESS_KEY", Some(WYRD_ID)),
        ("WYRD_S3_SECRET_KEY", Some(WYRD_SECRET)),
    ]);
    assert!(
        !output.status.success(),
        "half an AWS pair must refuse: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(WYRD_ID),
        "the Wyrd identity must not be silently substituted: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("AWS_SECRET_ACCESS_KEY"),
        "stderr must name the missing half: {stderr}"
    );
    assert!(
        !stderr.contains(WYRD_SECRET) && !stdout.contains(WYRD_SECRET),
        "no secret may appear in process output"
    );
}
