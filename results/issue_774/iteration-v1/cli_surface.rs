//! The `wyrd-validate` argument surface (proposal 0017; the skeleton slice).
//!
//! Binds the invariant *an operator-facing binary never silently discards an argument it was
//! given*: every one of the ten flags is echoed with its value or refused by name, credentials
//! resolve AWS → `WYRD_S3_*` → refuse without ever echoing the secret, and a token the parser
//! does not understand — an unknown flag, or a flag-shaped token in a value slot — is refused.
//!
//! Binary-level tests run the real `wyrd-validate` with an explicit, cleared child environment
//! (`env_clear`), so the test process's own env is never read or mutated. The credential
//! matrix runs over the library's injected lookup for the same reason.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::process::{Command, Output};

use wyrd_validate::{run, CredentialSource, EXIT_OK, FLAGS};

/// A distinct value for every flag, so a swapped or dropped value cannot pass.
const GIVEN: [(&str, &str); 10] = [
    ("endpoint", "http://127.0.0.1:9000"),
    ("region", "eu-central-1"),
    ("bucket", "validate-bucket"),
    ("scenario", "smoke"),
    ("duration", "2m"),
    ("workers", "7"),
    ("seed", "424242"),
    ("out", "/var/lib/wyrd-validate/out"),
    ("run-id", "run-0017"),
    ("driver-placement", "external"),
];

const WYRD_ID: &str = "WYRDKEYID0001";
const WYRD_SECRET: &str = "wyrd-secret-must-never-print";
const AWS_ID: &str = "AKIAAWSKEYID0001";
const AWS_SECRET: &str = "aws-secret-must-never-print";

fn argv(pairs: &[(&str, &str)]) -> Vec<String> {
    pairs
        .iter()
        .flat_map(|(flag, value)| [format!("--{flag}"), (*value).to_string()])
        .collect()
}

fn wyrd_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("WYRD_S3_ACCESS_KEY", WYRD_ID),
        ("WYRD_S3_SECRET_KEY", WYRD_SECRET),
    ]
}

/// Run the real binary with exactly `env` as its environment.
fn bin(args: &[String], env: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wyrd-validate"))
        .args(args)
        .env_clear()
        .envs(env.iter().copied())
        .output()
        .expect("spawn wyrd-validate")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 output")
}

/// Run the library entry point over an injected environment. Returns (status, stdout, stderr).
fn lib_run(args: &[String], env: &[(&str, &str)]) -> (u8, String, String) {
    let env: HashMap<String, String> = env
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let lookup = move |name: &str| env.get(name).cloned();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(args, &lookup, &mut out, &mut err);
    (status, text(&out), text(&err))
}

// ---- Criterion 2: the ten-flag surface, bound flag by flag ----------------------------------

#[test]
fn the_test_table_covers_exactly_the_declared_flags() {
    let given: Vec<&str> = GIVEN.iter().map(|(flag, _)| *flag).collect();
    assert_eq!(given, FLAGS.to_vec());
}

#[test]
fn all_ten_flags_are_echoed_each_with_its_own_value() {
    let out = bin(&argv(&GIVEN), &wyrd_env());
    let stdout = text(&out.stdout);
    assert!(
        out.status.success(),
        "exit {:?}, stderr: {}",
        out.status,
        text(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    for (flag, value) in GIVEN {
        let expected = format!("  --{flag} = {value}");
        assert!(
            lines.contains(&expected.as_str()),
            "flag --{flag} not echoed with its value `{value}`; stdout:\n{stdout}"
        );
    }
}

#[test]
fn flag_order_on_the_command_line_does_not_matter() {
    let mut reversed = GIVEN;
    reversed.reverse();
    let out = bin(&argv(&reversed), &wyrd_env());
    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    let stdout = text(&out.stdout);
    for (flag, value) in GIVEN {
        assert!(
            stdout.contains(&format!("  --{flag} = {value}\n")),
            "{stdout}"
        );
    }
}

#[test]
fn each_missing_flag_is_refused_by_name() {
    for (i, (missing, _)) in GIVEN.iter().enumerate() {
        let mut rest = GIVEN.to_vec();
        rest.remove(i);
        let out = bin(&argv(&rest), &wyrd_env());
        let stderr = text(&out.stderr);
        assert!(!out.status.success(), "missing --{missing} was accepted");
        assert!(
            stderr.contains(&format!("`--{missing}`")),
            "missing --{missing} not named; stderr: {stderr}"
        );
        assert!(
            out.stdout.is_empty(),
            "a refused run echoed a configuration"
        );
    }
}

#[test]
fn no_arguments_names_every_flag() {
    let out = bin(&[], &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    for flag in FLAGS {
        assert!(stderr.contains(&format!("`--{flag}`")), "{stderr}");
    }
}

// ---- Criterion 3: credential resolution, all four directions --------------------------------

/// The echoed block is the only place a credential could leak; assert on everything printed.
fn assert_no_secret(stdout: &str, stderr: &str) {
    for secret in [AWS_SECRET, WYRD_SECRET] {
        assert!(!stdout.contains(secret), "secret on stdout:\n{stdout}");
        assert!(!stderr.contains(secret), "secret on stderr:\n{stderr}");
    }
}

fn source_line(source: CredentialSource) -> String {
    format!("  credential-source = {source}")
}

#[test]
fn aws_pair_alone_resolves_to_aws() {
    let env = [
        ("AWS_ACCESS_KEY_ID", AWS_ID),
        ("AWS_SECRET_ACCESS_KEY", AWS_SECRET),
    ];
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN), &env);
    assert_eq!(status, EXIT_OK, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&format!("  access-key-id = {AWS_ID}").as_str()));
    assert!(lines.contains(&source_line(CredentialSource::Aws).as_str()));
    assert!(stdout.contains("AWS_ACCESS_KEY_ID"), "{stdout}");
    assert_no_secret(&stdout, &stderr);
}

#[test]
fn wyrd_pair_alone_resolves_to_wyrd() {
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN), &wyrd_env());
    assert_eq!(status, EXIT_OK, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&format!("  access-key-id = {WYRD_ID}").as_str()));
    assert!(lines.contains(&source_line(CredentialSource::Wyrd).as_str()));
    assert!(stdout.contains("WYRD_S3_ACCESS_KEY"), "{stdout}");
    assert_no_secret(&stdout, &stderr);
}

#[test]
fn both_pairs_present_aws_wins() {
    let mut env = wyrd_env();
    env.push(("AWS_ACCESS_KEY_ID", AWS_ID));
    env.push(("AWS_SECRET_ACCESS_KEY", AWS_SECRET));
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN), &env);
    assert_eq!(status, EXIT_OK, "{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&format!("  access-key-id = {AWS_ID}").as_str()));
    assert!(lines.contains(&source_line(CredentialSource::Aws).as_str()));
    assert!(!stdout.contains(WYRD_ID), "{stdout}");
    assert_no_secret(&stdout, &stderr);
}

#[test]
fn neither_pair_is_refused_naming_what_to_set() {
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN), &[]);
    assert_ne!(status, EXIT_OK);
    assert!(stdout.is_empty(), "{stdout}");
    for var in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "WYRD_S3_ACCESS_KEY",
        "WYRD_S3_SECRET_KEY",
    ] {
        assert!(stderr.contains(var), "{var} not named; stderr: {stderr}");
    }
}

#[test]
fn half_a_pair_is_refused_rather_than_skipped() {
    // An AWS id with no secret must not quietly fall through to the Wyrd identity.
    let mut env = wyrd_env();
    env.push(("AWS_ACCESS_KEY_ID", AWS_ID));
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN), &env);
    assert_ne!(status, EXIT_OK);
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("AWS_SECRET_ACCESS_KEY"), "{stderr}");
    assert_no_secret(&stdout, &stderr);
}

#[test]
fn the_real_binary_reads_its_environment_aws_first() {
    let mut env = wyrd_env();
    env.push(("AWS_ACCESS_KEY_ID", AWS_ID));
    env.push(("AWS_SECRET_ACCESS_KEY", AWS_SECRET));
    let out = bin(&argv(&GIVEN), &env);
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains(&format!("  access-key-id = {AWS_ID}\n")));
    assert_no_secret(&stdout, &stderr);

    let out = bin(&argv(&GIVEN), &[]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("WYRD_S3_ACCESS_KEY"));
}

// ---- Criterion 4: strict rejection, both halves ---------------------------------------------

#[test]
fn an_unrecognised_flag_is_refused_by_name() {
    let mut args = argv(&GIVEN);
    args.extend(["--end-point".to_string(), "http://elsewhere".to_string()]);
    let out = bin(&args, &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "unknown flag accepted");
    assert!(stderr.contains("`--end-point`"), "{stderr}");
    assert!(out.stdout.is_empty());
}

#[test]
fn a_flag_in_any_value_slot_is_refused_naming_both_flags() {
    for (flag, _) in GIVEN {
        // `--<flag> --typo`, then every other flag with a good value, so only the value slot
        // is wrong; a lenient parser would set `<flag>` to "--typo" and exit 0.
        let rest: Vec<(&str, &str)> = GIVEN.iter().copied().filter(|(f, _)| *f != flag).collect();
        let mut args = vec![format!("--{flag}"), "--typo".to_string()];
        args.extend(argv(&rest));
        let out = bin(&args, &wyrd_env());
        let stderr = text(&out.stderr);
        assert!(!out.status.success(), "--{flag} --typo was accepted");
        assert!(stderr.contains(&format!("`--{flag}`")), "{stderr}");
        assert!(stderr.contains("`--typo`"), "{stderr}");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn a_known_flag_in_a_value_slot_is_refused_too() {
    // `--bucket --duration 2m` — the forgotten bucket must not become "--duration".
    let rest: Vec<(&str, &str)> = GIVEN
        .iter()
        .copied()
        .filter(|(flag, _)| *flag != "bucket")
        .collect();
    let mut args = vec!["--bucket".to_string()];
    args.extend(argv(&rest));
    let out = bin(&args, &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "stdout: {}", text(&out.stdout));
    assert!(stderr.contains("`--bucket`"), "{stderr}");
    assert!(stderr.contains(&format!("`--{}`", rest[0].0)), "{stderr}");
}

#[test]
fn a_trailing_flag_with_no_value_is_refused() {
    let mut args = argv(&GIVEN[..9]);
    args.push("--driver-placement".to_string());
    let out = bin(&args, &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("flag `--driver-placement` needs a value"),
        "{stderr}"
    );
}

#[test]
fn a_repeated_flag_is_refused_rather_than_last_wins() {
    let mut args = argv(&GIVEN);
    args.extend(["--duration".to_string(), "7d".to_string()]);
    let out = bin(&args, &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "stdout: {}", text(&out.stdout));
    assert!(stderr.contains("`--duration`"), "{stderr}");
}

#[test]
fn a_stray_positional_is_refused() {
    let mut args = argv(&GIVEN);
    args.push("smoke".to_string());
    let out = bin(&args, &wyrd_env());
    let stderr = text(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("`smoke`"), "{stderr}");
}
