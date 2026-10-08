# Adversarial review — issue #774 (`crates/validate` skeleton + CLI surface)

**Bottom line: I could not refute the fix on any of the four binding criteria.** Below is what I tried, followed by two small unwarranted claims and two scope questions for the maintainer.

## What I attacked and could not break

- **The evidence is real, not a mirror.** `gate-logs/C4-ci.log:2721-2744` shows `tests/cli_surface.rs` running all 19 tests inside `cargo xtask ci`, the one gating row. The binary tests spawn the real `CARGO_BIN_EXE_wyrd-validate` (`crates/validate/tests/cli_surface.rs:78`). The library tests call the production `run`/`resolve_config`, not a copy. `C4-verify` is green-only, but the brief declared that in advance for a crate this patch creates; it is not a hidden red leg.
- **The tests fail when the code is wrong.** I rebuilt the crate in scratch and applied hand-written mutations that `cargo mutants` does not generate. Each one turned the suite red:
  - Refusing only *known* flag names in a value slot (`crates/validate/src/args.rs:136`): caught by `a_flag_in_any_value_slot_is_refused_naming_both_flags`.
  - Silently skipping an unknown flag and its value: caught by `an_unrecognised_flag_is_refused_by_name`.
  - Swallowing a flush error (`crates/validate/src/lib.rs:103-105`): caught by `a_failed_write_or_flush_of_the_echo_exits_non_zero`.
  - Deleting the empty-value check (`args.rs:142`): caught by `each_empty_value_is_refused_by_name`.

  This adds to the frozen `C5-mutants` result (0 missed of 42).
- **Parser edge cases on the real binary are all refused by name, with exit 2:** `--bucket=x`, a bare `--`, `--bucket --typo`, `--bucket --duration 2m`, a repeated flag, and a stray positional.
- **Credential matrix:** all four brief directions pass. Half-pairs are refused in both directions (`access_keys.rs:153`, `:159`), an empty value counts as unset (`:176`), and the test checks the *resolved secret* and the `Debug` redaction (`cli_surface.rs:259-265`). Together these close the v1 carry-forward findings. The citations to `crates/server/src/cli.rs:2166/2168/2173/2550-2553` and `xtask/src/main.rs:1559` match this base.

## Findings

- **Unwarranted claim (low severity; not worth a rebuild on its own): `crates/validate/src/lib.rs:101-102`** says a failed write "is a failed run, not a success with nothing printed". That is false for the production binary when stdout is closed. `wyrd-validate <all ten flags> >&-` exits **0** and prints nothing, because Rust's `std::io::stdout` treats a closed fd 1 (EBADF) as a successful write. I reproduced it: `/dev/full` → exit 1 and a broken pipe → exit 1 behave as claimed, but a closed stdout → exit 0. The test `cli_surface.rs:537` only exercises an injected writer, so it cannot see this. Fix options: narrow the comment, or check `std::io::stdout().as_fd().try_clone_to_owned()` in `main.rs` (safe code). Impact is small, because later slices write the verdict to `--out`, not stdout.
- **Untested documented contract (low severity): `crates/validate/src/lib.rs:63`** says "Argument errors are reported before credential errors". Swapping the two `?` lines in `resolve_config` (`lib.rs:68-69`) still passes 19/19. No test combines bad arguments with no credentials. This is not a brief criterion; I note it only because the doc makes the promise.
- NEEDS-HUMAN [human] — **`AWS_SESSION_TOKEN` is silently ignored: `crates/validate/src/access_keys.rs:139-166`.** Concrete case: a shell holding temporary AWS STS credentials (`AWS_ACCESS_KEY_ID=ASIA…`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`) plus a deliberate `WYRD_S3_*` pair. It resolves to the AWS identity, exits 0, and the echo never mentions the token (reproduced). This is the same mistake the patch refuses for half-pairs ("an operator who set `AWS_ACCESS_KEY_ID` meant to use it", `access_keys.rs:93-95`): an STS pair cannot sign without its token, and nothing in the repo handles session tokens. The brief's "BOTH present → AWS wins" rule is settled. What is still open is whether a set `AWS_SESSION_TOKEN` should be refused or flagged in the echo now, or left to #741's signer. That is a scope call.
- NEEDS-HUMAN [human] — **Scope check against the settled "encoding cases are OUT" decision: `crates/validate/src/access_keys.rs:103`, `:178`, `crates/validate/tests/cli_surface.rs:358-425`.** The patch adds non-UTF-8 handling for *environment variables*: a `NotUnicode` error variant, two tests, and a sentence in the architecture doc. The brief bans "Non-UTF-8 arguments", and env vars are not arguments. The addition is also defensible: `std::env::var(..).ok()` would treat an unreadable AWS id as unset and sign as Wyrd. Meanwhile non-UTF-8 *argv* still panics at `main.rs:9` (`std::env::args()`), which is still a non-zero refusal and is correctly left alone per the brief. I judge the addition reasonable. Only the maintainer can say whether it re-opens what was closed.

## Not raised (per the brief and rubric)

- Control characters or newlines in echoed values, and non-UTF-8 argv: the brief puts these explicitly OUT.
- Value typing (`--duration 7` with no unit, a whitespace-only `--run-id " "`): the brief defers this to the slices that consume the values.
