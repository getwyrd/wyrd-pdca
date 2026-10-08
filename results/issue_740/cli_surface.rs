//! `wyrd-validate`'s CLI surface (#740, criteria 3 and 4) — bound flag by flag and
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
//! / `utf8_args` directly over in-memory inputs (no subprocess) — the SAME functions
//! `main` calls, driven the same "injected lookup" way, for a fast, non-flaky proof of
//! decisions that a subprocess can only observe indirectly.
//!
//! Three input surfaces are covered, because a tool that is strict on one and liberal on
//! another still misreports the run it claims to have measured:
//!
//! * **arguments** — an unknown `--flag`, a flag-shaped token in a VALUE slot (per flag),
//!   a repeated flag, an empty value, and an argument that is not valid UTF-8;
//! * **environment** — both credential pairs, their precedence, half a pair, an
//!   exported-but-empty name, and a name set to non-UTF-8 bytes (which must refuse, not
//!   silently fall back to the other pair's identity);
//! * **output** — the echoed record itself: a value carrying a newline or a terminal
//!   control sequence must not be able to forge a configuration line, and no secret may
//!   appear anywhere in the process output.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::process::Command;

use wyrd_validate::{
    render_config_block, resolve_config, resolve_credentials, utf8_args, CredentialSource,
    Credentials, EnvValue, ParsedArgs, REQUIRED_FLAGS,
};

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

/// One flag's value replaced, the other nine left at their base values.
fn flags_with(target: &str, replacement: &'static str) -> Vec<(&'static str, &'static str)> {
    base_flags()
        .into_iter()
        .map(|(name, value)| {
            if name == target {
                (name, replacement)
            } else {
                (name, value)
            }
        })
        .collect()
}

/// Credentials never appear on the required-flags list, so every base invocation still
/// needs AWS creds set for the CLI-surface tests below to reach a clean exit 0.
fn aws_creds() -> Vec<(&'static str, &'static str)> {
    vec![
        ("AWS_ACCESS_KEY_ID", "AKIAEXAMPLEBASE"),
        ("AWS_SECRET_ACCESS_KEY", "base-secret-should-never-print"),
    ]
}

/// How the echoed block renders one flag: `--name "value"`, quoted and escaped
/// (`wyrd_validate::render_config_block`). The quoting is what makes the record
/// unforgeable, so the per-flag assertions below check the quoted form — a bare
/// substring match would also pass against an unescaped, forgeable line.
fn echoed(name: &str, value: &str) -> String {
    format!("--{name} \"{value}\"")
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
    // each flag's exact `--name "value"` pair must appear, not merely the bare value.
    for (name, value) in &flags {
        let needle = echoed(name, value);
        assert!(
            stdout.contains(&needle),
            "missing `{needle}` in resolved-configuration output:\n{stdout}"
        );
    }
}

/// The structural half of the assertion above: `Config::entries` — the single place a
/// flag is paired with the field holding it, and therefore what the echoed block prints
/// — must name exactly `REQUIRED_FLAGS`, in order. Without this, adding an eleventh flag
/// to the parser and forgetting the echo would be caught only by remembering to extend
/// the fixture above.
#[test]
fn the_echoed_block_covers_every_required_flag_in_order() {
    let parsed = ParsedArgs::parse(&args_for(&base_flags())).expect("base flags parse");
    let config = resolve_config(&parsed).expect("base flags are complete");
    let echoed_flags: Vec<&str> = config.entries().into_iter().map(|(name, _)| name).collect();
    assert_eq!(echoed_flags, REQUIRED_FLAGS.to_vec());
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

/// `--flag=value` is not this product's spelling (`wyrd`'s own parser does not accept it
/// either, `crates/server/src/cli.rs:2506`), and the failure mode matters more than the
/// policy: the whole token must be refused BY NAME, not silently read as a flag called
/// `endpoint=…` whose value is the next token — which would leave `--endpoint` unset
/// while consuming the argument after it.
#[test]
fn the_equals_spelling_is_refused_by_name_rather_than_half_parsed() {
    let mut args = vec!["--endpoint=https://s3.example.invalid".to_string()];
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
        "`--flag=value` must be refused, not half-parsed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--endpoint=https://s3.example.invalid"),
        "stderr must name the whole unrecognised token: {stderr}"
    );
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
        let output = Command::new(bin())
            .args(args_for(&flags_with(target, BOGUS)))
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
    let output = Command::new(bin())
        .args(args_for(&flags_with("seed", "-12345")))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        output.status.success(),
        "a negative seed is a legal value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&echoed("seed", "-12345")),
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
    let output = Command::new(bin())
        .args(args_for(&flags_with("bucket", "")))
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

// ─── the echoed record itself: unforgeable, one field per line ──────────────────────

/// The block is what an operator (and every later slice's artifact) reads as "what this
/// run was configured with". A raw interpolation lets a value carrying a newline print a
/// line that reads exactly like another resolved flag, so the record would state an
/// endpoint the run never used. The value is not rejected — it is escaped, so it stays
/// fully recoverable — but it can no longer forge a line.
#[test]
fn a_newline_in_a_value_cannot_forge_a_configuration_line() {
    const FORGED: &str = "bucket-real\n--endpoint https://forged.invalid";
    let output = Command::new(bin())
        .args(args_for(&flags_with("bucket", FORGED)))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        output.status.success(),
        "an odd but representable value is escaped, not refused: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // One header line + ten flags + one credentials line, and not one line more: the
    // forged `--endpoint` never became a line of its own.
    assert_eq!(
        stdout.lines().count(),
        REQUIRED_FLAGS.len() + 2,
        "the block must have exactly one line per field:\n{stdout}"
    );
    let endpoint_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("--endpoint "))
        .collect();
    assert_eq!(
        endpoint_lines,
        vec![echoed("endpoint", "https://s3.example.invalid")],
        "exactly one `--endpoint` line, and it is the one that was given:\n{stdout}"
    );
    assert!(
        stdout.contains(r#"--bucket "bucket-real\n--endpoint https://forged.invalid""#),
        "the value survives, escaped, inside its own field:\n{stdout}"
    );
}

/// The same forging, one layer down: a terminal control sequence in a value can erase or
/// rewrite what the operator sees printed, which is a misreported run that never even
/// reaches a file. No raw control byte may leave this tool.
#[test]
fn a_terminal_control_sequence_in_a_value_is_escaped() {
    const ESCAPE_SEQUENCE: &str = "run\u{1b}[2K-id";
    let output = Command::new(bin())
        .args(args_for(&flags_with("run-id", ESCAPE_SEQUENCE)))
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains('\u{1b}'),
        "no raw escape byte may reach the terminal:\n{stdout:?}"
    );
    assert!(
        stdout.contains(r#"--run-id "run\u{1b}[2K-id""#),
        "the sequence is escaped into its own field:\n{stdout}"
    );
}

/// The access-key id is operator-supplied too (it comes from the environment), so it
/// gets the same treatment as a flag value — asserted over the pure renderer, where the
/// forged text can be pinned exactly.
#[test]
fn a_newline_in_the_access_key_id_cannot_forge_a_credentials_line() {
    let parsed = ParsedArgs::parse(&args_for(&base_flags())).expect("base flags parse");
    let config = resolve_config(&parsed).expect("base flags are complete");
    let credentials = Credentials {
        access_key_id: "AKIA-real\ncredentials: access-key-id=AKIA-forged".to_string(),
        secret_access_key: "never-printed".to_string(),
        source: CredentialSource::Aws,
    };
    let block = render_config_block(&config, &credentials);
    assert_eq!(
        block.lines().count(),
        REQUIRED_FLAGS.len() + 2,
        "the forged line never became a line:\n{block}"
    );
    assert_eq!(
        block
            .lines()
            .filter(|line| line.starts_with("credentials:"))
            .count(),
        1,
        "exactly one credentials line:\n{block}"
    );
    assert!(
        !block.contains("never-printed"),
        "the secret is never rendered:\n{block}"
    );
}

// ─── criterion 4: credential resolution, both directions ───────────────────────────

const AWS_ID: &str = "AKIA_REAL_AWS_ID";
const AWS_SECRET: &str = "aws-secret-never-printed-6f3a";
const WYRD_ID: &str = "wyrd-gateway-id-2b7";
const WYRD_SECRET: &str = "wyrd-secret-never-printed-9d1";

const CREDENTIAL_NAMES: [&str; 4] = [
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "WYRD_S3_ACCESS_KEY",
    "WYRD_S3_SECRET_KEY",
];

/// Spawn `wyrd-validate` with the base flags plus an EXPLICIT credential environment —
/// every one of the four names is either set to a fixture value or removed, so no
/// ambient variable from the test runner's own environment can leak into the case
/// under test.
fn run_with_credential_env(env: &[(&str, Option<OsString>)]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(args_for(&base_flags()));
    for name in CREDENTIAL_NAMES {
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

/// `(name, value)` pairs for [`run_with_credential_env`], from ordinary strings.
fn set(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, Option<OsString>)> {
    pairs
        .iter()
        .map(|(name, value)| (*name, Some(OsString::from(*value))))
        .collect()
}

#[test]
fn aws_credentials_present_resolve_to_the_aws_id_and_source() {
    let output = run_with_credential_env(&set(&[
        ("AWS_ACCESS_KEY_ID", AWS_ID),
        ("AWS_SECRET_ACCESS_KEY", AWS_SECRET),
    ]));
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
    let output = run_with_credential_env(&set(&[
        ("WYRD_S3_ACCESS_KEY", WYRD_ID),
        ("WYRD_S3_SECRET_KEY", WYRD_SECRET),
    ]));
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
    let output = run_with_credential_env(&set(&[
        ("AWS_ACCESS_KEY_ID", AWS_ID),
        ("AWS_SECRET_ACCESS_KEY", AWS_SECRET),
        ("WYRD_S3_ACCESS_KEY", WYRD_ID),
        ("WYRD_S3_SECRET_KEY", WYRD_SECRET),
    ]));
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

#[test]
fn half_an_aws_pair_refuses_over_the_real_binary_instead_of_using_the_wyrd_identity() {
    let output = run_with_credential_env(&set(&[
        ("AWS_ACCESS_KEY_ID", AWS_ID),
        ("WYRD_S3_ACCESS_KEY", WYRD_ID),
        ("WYRD_S3_SECRET_KEY", WYRD_SECRET),
    ]));
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

// ─── the pure decisions, over the injected lookup, no subprocess ───────────────────

fn present(value: &str) -> EnvValue {
    EnvValue::Present(value.to_string())
}

fn lookup_over(map: BTreeMap<&'static str, EnvValue>) -> impl FnMut(&str) -> EnvValue {
    move |name| map.get(name).cloned().unwrap_or(EnvValue::Absent)
}

#[test]
fn resolve_credentials_prefers_aws_over_wyrd_over_the_injected_lookup() {
    let mut lookup = lookup_over(BTreeMap::from([
        ("AWS_ACCESS_KEY_ID", present(AWS_ID)),
        ("AWS_SECRET_ACCESS_KEY", present(AWS_SECRET)),
        ("WYRD_S3_ACCESS_KEY", present(WYRD_ID)),
        ("WYRD_S3_SECRET_KEY", present(WYRD_SECRET)),
    ]));
    let creds = resolve_credentials(&mut lookup).expect("both pairs present must resolve");
    assert_eq!(creds.access_key_id, AWS_ID);
    assert_eq!(creds.source, CredentialSource::Aws);
}

#[test]
fn resolve_credentials_falls_back_to_wyrd_over_the_injected_lookup() {
    let mut lookup = lookup_over(BTreeMap::from([
        ("WYRD_S3_ACCESS_KEY", present(WYRD_ID)),
        ("WYRD_S3_SECRET_KEY", present(WYRD_SECRET)),
    ]));
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
/// a credential they never chose.
#[test]
fn resolve_credentials_refuses_half_a_pair_rather_than_falling_back() {
    for (present_name, missing_name) in [
        ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"),
        ("AWS_SECRET_ACCESS_KEY", "AWS_ACCESS_KEY_ID"),
    ] {
        // The complete fallback pair IS available — the point is that it is not silently
        // taken while the first pair is half-stated.
        let mut lookup = lookup_over(BTreeMap::from([
            (present_name, present("half-a-pair-value")),
            ("WYRD_S3_ACCESS_KEY", present(WYRD_ID)),
            ("WYRD_S3_SECRET_KEY", present(WYRD_SECRET)),
        ]));
        let err = resolve_credentials(&mut lookup)
            .map(|c| c.access_key_id)
            .expect_err("half a pair must refuse, not fall through");
        assert!(
            err.contains(missing_name) && !err.contains(WYRD_ID),
            "the error must name the missing half ({missing_name}): {err}"
        );
    }
}

#[test]
fn resolve_credentials_refuses_a_present_but_empty_name() {
    let mut lookup = lookup_over(BTreeMap::from([
        ("AWS_ACCESS_KEY_ID", present("")),
        ("AWS_SECRET_ACCESS_KEY", present(AWS_SECRET)),
    ]));
    let err = resolve_credentials(&mut lookup).expect_err("an empty credential must refuse");
    assert!(err.contains("AWS_ACCESS_KEY_ID"), "{err}");
}

/// A name set to bytes that are not UTF-8 is SET — it is simply unusable. Reporting it
/// as absent (what `std::env::var(name).ok()` does) would sign the run with the other
/// pair's identity and echo that identity as though it had been chosen. Asserted for
/// both names of both pairs: for the AWS pair with the Wyrd fallback fully available (so
/// a fall-through would be silent and successful), and for the Wyrd pair with AWS
/// absent (so the failure mode would be the misleading "no credentials found").
#[test]
fn resolve_credentials_refuses_a_non_utf8_name_rather_than_falling_back() {
    let cases: [(&str, Vec<(&'static str, EnvValue)>); 4] = [
        (
            "AWS_ACCESS_KEY_ID",
            vec![
                ("AWS_SECRET_ACCESS_KEY", present(AWS_SECRET)),
                ("WYRD_S3_ACCESS_KEY", present(WYRD_ID)),
                ("WYRD_S3_SECRET_KEY", present(WYRD_SECRET)),
            ],
        ),
        (
            "AWS_SECRET_ACCESS_KEY",
            vec![
                ("AWS_ACCESS_KEY_ID", present(AWS_ID)),
                ("WYRD_S3_ACCESS_KEY", present(WYRD_ID)),
                ("WYRD_S3_SECRET_KEY", present(WYRD_SECRET)),
            ],
        ),
        (
            "WYRD_S3_ACCESS_KEY",
            vec![("WYRD_S3_SECRET_KEY", present(WYRD_SECRET))],
        ),
        (
            "WYRD_S3_SECRET_KEY",
            vec![("WYRD_S3_ACCESS_KEY", present(WYRD_ID))],
        ),
    ];
    for (unusable, rest) in cases {
        let mut entries: BTreeMap<&'static str, EnvValue> = rest.into_iter().collect();
        entries.insert(unusable, EnvValue::NotUnicode);
        let mut lookup = lookup_over(entries);
        let err = resolve_credentials(&mut lookup)
            .map(|c| c.access_key_id)
            .expect_err("an unreadable credential must refuse, not fall through");
        assert!(
            err.contains(unusable) && err.contains("UTF-8"),
            "the error must name the unreadable variable ({unusable}): {err}"
        );
        assert!(
            !err.contains(WYRD_ID) && !err.contains(AWS_ID),
            "no other identity is offered in place of the unreadable one: {err}"
        );
    }
}

/// The boundary of the rule above, stated so it is not mistaken for an oversight: only
/// the source actually CONSULTED matters. A complete, readable AWS pair resolves before
/// the `WYRD_S3_*` names are read at all, so an unusable value parked in an unconsulted
/// variable is irrelevant to this run — the run is signed with the identity precedence
/// says it is signed with, and the echoed block names it.
#[test]
fn an_unusable_value_in_an_unconsulted_variable_does_not_block_the_winning_pair() {
    let mut lookup = lookup_over(BTreeMap::from([
        ("AWS_ACCESS_KEY_ID", present(AWS_ID)),
        ("AWS_SECRET_ACCESS_KEY", present(AWS_SECRET)),
        ("WYRD_S3_ACCESS_KEY", EnvValue::NotUnicode),
        ("WYRD_S3_SECRET_KEY", EnvValue::NotUnicode),
    ]));
    let creds = resolve_credentials(&mut lookup).expect("the AWS pair still wins");
    assert_eq!(creds.access_key_id, AWS_ID);
    assert_eq!(creds.source, CredentialSource::Aws);
}

/// `Credentials` carries the secret (a later slice signs with it) and must never print
/// it — including through the `Debug` a derive would have given it, which is how a
/// secret reaches a log line without anyone deciding to put it there.
#[test]
fn the_debug_rendering_of_credentials_redacts_the_secret() {
    let credentials = Credentials {
        access_key_id: AWS_ID.to_string(),
        secret_access_key: AWS_SECRET.to_string(),
        source: CredentialSource::Aws,
    };
    let rendered = format!("{credentials:?}");
    assert!(
        !rendered.contains(AWS_SECRET),
        "the secret must not survive a Debug format: {rendered}"
    );
    assert!(
        rendered.contains(AWS_ID),
        "the id (which the echoed block already publishes) stays visible: {rendered}"
    );
}

// ─── input that is not valid UTF-8, at the OS boundary ─────────────────────────────
//
// `#[cfg(unix)]`: building a byte string that is not valid UTF-8 is a platform API
// (`OsStringExt`), and the workspace's deployment targets are Linux (`deploy/`,
// `Dockerfile`, the systemd/compose units). A `#[cfg(windows)]` twin would be a second
// spelling that no gate ever compiles, so these three read the unix form only — the
// DECISION they cover (`EnvValue::NotUnicode` vs `Absent`) is platform-independent and
// is asserted over the injected lookup above, on every platform.

/// `std::env::args()` PANICS on a non-UTF-8 argument and a lossy conversion would report
/// a bucket the operator never asked for, so the argument vector is read as `OsString`
/// and refused by name. Pure half: `utf8_args` is the function `run` calls first.
#[cfg(unix)]
#[test]
fn utf8_args_refuses_an_unreadable_argument_naming_its_position() {
    let args = vec![
        OsString::from("--endpoint"),
        non_utf8_os_string(),
        OsString::from("--region"),
    ];
    let err = utf8_args(args).expect_err("a non-UTF-8 argument must be refused");
    assert!(
        err.contains("argument 2"),
        "the error names which argument it could not read: {err}"
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_argument_exits_nonzero_without_panicking() {
    let mut args: Vec<OsString> = args_for(&base_flags())
        .into_iter()
        .map(OsString::from)
        .collect();
    args.push(non_utf8_os_string());
    let output = Command::new(bin())
        .args(&args)
        .envs(aws_creds())
        .output()
        .expect("failed to spawn wyrd-validate");
    assert!(
        !output.status.success(),
        "an unreadable argument must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("wyrd-validate: ") && stderr.contains("UTF-8"),
        "the refusal is a message, not a panic: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "a panic is not an argument refusal: {stderr}"
    );
}

/// The same unreadable-environment case as the pure test above, driven end to end
/// through the binary's own `std::env::var_os` closure (`crates/validate/src/main.rs`):
/// that closure is the production code that decides `NotUnicode` vs `Absent`, and a
/// `std::env::var(name).ok()` there would make this fall back to the Wyrd identity and
/// exit 0.
#[cfg(unix)]
#[test]
fn a_non_utf8_credential_refuses_over_the_real_binary_instead_of_falling_back() {
    let mut env: Vec<(&'static str, Option<OsString>)> =
        vec![("AWS_ACCESS_KEY_ID", Some(non_utf8_os_string()))];
    env.extend(set(&[
        ("AWS_SECRET_ACCESS_KEY", AWS_SECRET),
        ("WYRD_S3_ACCESS_KEY", WYRD_ID),
        ("WYRD_S3_SECRET_KEY", WYRD_SECRET),
    ]));
    let output = run_with_credential_env(&env);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "an unreadable AWS id must refuse, not fall back: {stdout}"
    );
    assert!(
        !stdout.contains(WYRD_ID),
        "the Wyrd identity must not be silently substituted: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("AWS_ACCESS_KEY_ID") && stderr.contains("UTF-8"),
        "stderr must name the unreadable variable: {stderr}"
    );
}

/// A byte string an OS accepts but UTF-8 does not (a lone `0xFF` continuation byte),
/// built without `unsafe` — `OsString::from_vec` is safe; it is `String::from_utf8` that
/// would refuse these bytes, which is exactly the case under test.
#[cfg(unix)]
fn non_utf8_os_string() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(vec![b'A', 0xff, b'Z'])
}
