## Summary
**User impact:** An operator who wants to check their own Wyrd cluster from the outside, the
way a real S3 client sees it, has no tool to run. Proposal 0017 describes one, but there was
nowhere in the repo to build it, and every later step of that work (the S3 client, packaging,
the capability matrix) assumes this first piece exists.

This PR adds the `wyrd-validate` binary as its own crate. For now it only reads its settings:
it checks the ten command-line flags and the S3 credentials, prints the configuration it
resolved, and exits. It sends no requests yet, and says so. Any input it cannot use is refused
by name, never silently ignored or replaced with a default.

## What to look at
- **The new crate**, `crates/validate`: a library holding the decisions (argument parsing,
  credential lookup, the printed configuration) and a thin binary that only does I/O. It has
  **no dependencies**, neither third-party nor other `wyrd-*` crates.
- **Try it** after `cargo build -p wyrd-validate`:
  ```sh
  WYRD_S3_ACCESS_KEY=id WYRD_S3_SECRET_KEY=secret target/debug/wyrd-validate \
    --endpoint http://localhost:9000 --region us-east-1 --bucket b --scenario smoke \
    --duration 2m --workers 4 --seed 1 --out ./out --run-id r1 --driver-placement local
  ```
  This prints the ten values plus the access-key id and where it came from, and exits 0.
  Then try `--bucket --typo`, a missing flag, `--run-id ""`, or only `AWS_ACCESS_KEY_ID` set.
  Each exits non-zero and names what is wrong.
- **Two things that go further than the issue text**, both agreed with the maintainer:
  1. The parser is **strict**, unlike `wyrd`'s own. `wyrd` takes the next token as a flag's
     value no matter what it is, so `--bucket --typo` sets the bucket to `"--typo"`. The new
     binary refuses that. Nothing existing depends on this binary, so strict costs nothing
     now and would be a breaking change later.
  2. An **empty flag value** and a **credential variable that is not valid UTF-8** are both
     refused by name. Otherwise `--run-id "$RUN_ID"` with `RUN_ID` unset would run with an
     empty run id, and a garbled `AWS_ACCESS_KEY_ID` would quietly fall through to the Wyrd
     key pair.
- **Known limitation, left for #741:** `AWS_SESSION_TOKEN` is not read. A shell with temporary
  AWS credentials resolves to the AWS key pair and exits 0 without mentioning the token.
  Handling it belongs to the request signer, which arrives in #741.

## Root cause
There was no `crates/validate` on `main`, and none had ever existed
(`git log -- crates/validate` is empty). The workspace guard also rejects any package under
`crates/` that is not listed in `[workspace] members` (`xtask/src/repo_guard.rs:421`), so the
crate has to be registered, not just added.

## Fix
- `Cargo.toml`: `crates/validate` added to `[workspace] members`. `Cargo.lock` gets the
  dependency-free `wyrd-validate` entry.
- `crates/validate/Cargo.toml`: a `[lib]` and a `[[bin]] wyrd-validate`, no dependencies, and
  the workspace lint policy. Both crate roots (`src/lib.rs:1`, `src/main.rs:1`) carry
  `#![forbid(unsafe_code)]`.
- `src/args.rs`: the ten-flag parser, modelled on `ParsedArgs`
  (`crates/server/src/cli.rs:2533-2570`) and using the same "needs a value" wording, but
  strict. `parse` (`:122`) refuses an unknown flag, a `--` token in a value slot (`:136`), an
  empty value (`:143`), a repeated flag, a stray positional, and lists every missing flag.
- `src/access_keys.rs`: `resolve` (`:139`) tries the AWS pair, then the `WYRD_S3_*` pair (the
  same variables `wyrd s3` reads, `crates/server/src/cli.rs:2166`, `:2173`), then refuses. It
  reads the environment through an injected lookup, so tests never change the process
  environment. `read` (`:170`) treats empty as unset and refuses non-UTF-8 by name. `Debug` on
  `Credentials` (`:78`) redacts the secret.
- `src/lib.rs`: `ResolvedConfig::render` (`:45`) prints the block (id, never the secret).
  `run` (`:93`) takes argv, environment, stdout and stderr as arguments. A failed write or
  flush of the output exits non-zero (`:105`) and says why on stderr.
- `src/main.rs`: real argv, `std::env::var_os`, real stdout/stderr.
- `docs/design/architecture/05-building-block-view.md`: a table row and a paragraph for the
  new binary and its command-line surface.

## Verification
Target: `getwyrd/wyrd` @ `main` (`36f006d`). Line numbers in `crates/validate/**` refer to the
files this PR adds.

- **Claim:** the crate is a registered workspace member and the full project gate passes.
  **Checked:** `Cargo.toml` members; `xtask/src/repo_guard.rs:421` (unregistered crates fail
  the gate). `cargo xtask ci` exits 0 ("all checks passed"), including the unsafe-code guard,
  clippy, `cargo deny`, the docs lint and the DST (simulation test) build.
- **Claim:** all ten flags are parsed and each is echoed with its own value; a missing flag is
  refused by name.
  **Checked:** `src/args.rs:122-176`, `src/lib.rs:45`.
  **Test:** `all_ten_flags_are_echoed_each_with_its_own_value` (checks every flag, not a
  sample), `each_missing_flag_is_refused_by_name`,
  `no_arguments_names_every_flag_and_prints_the_usage_line`.
- **Claim:** credentials resolve AWS, then Wyrd, then refuse. AWS wins when both are set. The
  right secret is resolved, and the secret never appears in output or `Debug`.
  **Checked:** `src/access_keys.rs:139-185`.
  **Test:** `credentials_resolve_aws_then_wyrd_then_refuse` (all four cases; searches stdout
  and stderr for the secret; asserts the resolved id, secret and source),
  `half_a_pair_is_refused_in_either_direction_rather_than_skipped`,
  `an_empty_variable_counts_as_unset`,
  `a_non_utf8_variable_is_refused_by_name_rather_than_treated_as_unset`,
  `the_real_binary_refuses_a_non_utf8_aws_id_instead_of_signing_as_wyrd`.
- **Claim:** unknown flags and flag-shaped tokens in a value slot are refused, naming both
  flags. `wyrd`'s parser accepts these (`crates/server/src/cli.rs:2550-2553`).
  **Checked:** `src/args.rs:122-150`.
  **Test:** `an_unrecognised_flag_is_refused_by_name`,
  `a_flag_in_any_value_slot_is_refused_naming_both_flags` (all ten slots),
  `a_known_flag_in_a_value_slot_is_refused_too`, `each_empty_value_is_refused_by_name`,
  `a_repeated_flag_is_refused_rather_than_last_wins`, `a_stray_positional_is_refused`.
- **Claim:** if the configuration cannot be written to stdout, the exit code is non-zero.
  **Checked:** `src/lib.rs:103-110`.
  **Test:** `a_failed_write_or_flush_of_the_echo_exits_non_zero` (write fails; flush fails).
- **Tests:** `crates/validate/tests/cli_surface.rs`, 19 tests. They fail before this PR, since
  the crate does not exist yet, and pass after it. Because this is new code, each piece of
  logic was also reverted one at a time to confirm a named test goes red. `cargo mutants
  --in-diff` found 0 missed mutants (26 caught, 16 unviable). `cargo llvm-cov` reports 100%
  line coverage for all four source files; the tests that spawn the real binary pass
  `LLVM_PROFILE_FILE` through so `main.rs` is counted.

Fixes #774
