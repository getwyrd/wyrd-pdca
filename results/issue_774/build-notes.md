# Build notes — issue 774, iteration 2 (validate crate skeleton + CLI surface)

Target: `getwyrd/wyrd` @ `main`. The per-cycle worktree is based on `36f006d` (not the
brief's `65ca4fd`; `main` moved on). Every peer line number below was re-checked on
`36f006d`: `ParsedArgs` is now `crates/server/src/cli.rs:2533-2570` (brief: `:2495-2532`), the
value-slot read is `:2550-2553`, the credential fallbacks are `:2166` / `:2173` with the
"no anonymous access" refusal at `:2168`, and the injected env lookup is
`xtask/src/main.rs:1559` (brief: `:1519`). The code comments cite the `36f006d` numbers.

## What the patch does

Same shape as iteration 1 (which the carry-forward did not reject as an approach; its
findings were test gaps plus two refusal holes):

- `Cargo.toml:31-33` — `crates/validate` added to `[workspace] members` (with a comment
  naming proposal 0017 §9). `Cargo.lock:5274-5276` gains the dependency-free
  `wyrd-validate` entry.
- `crates/validate/Cargo.toml` — package `wyrd-validate`, `[lib]` + `[[bin]] wyrd-validate`,
  **no dependencies at all** (no third-party crate, no `wyrd-*`), `[lints] workspace = true`.
- `crates/validate/src/lib.rs:1`, `crates/validate/src/main.rs:1` — `#![forbid(unsafe_code)]`
  on both crate roots.
- `crates/validate/src/args.rs` — the ten-flag strict parser (`FLAGS` at `:18-29`, `parse` at
  `:122-176`, `usage` at `:179`).
- `crates/validate/src/access_keys.rs` — AWS → `WYRD_S3_*` → refuse, over an injected lookup
  (`resolve` at `:139`, `read` at `:170`).
- `crates/validate/src/lib.rs` — `ResolvedConfig::render` (the echo block, id only),
  `resolve_config`, and `run` over injected argv / env / stdout / stderr.
- `crates/validate/src/main.rs` — the I/O shell: real argv, `std::env::var_os`, real
  stdout/stderr.
- `docs/design/architecture/05-building-block-view.md:249` (table row) and `:253` (paragraph)
  — docs currency for the new binary and its CLI surface (rubric: a new CLI flag updates the
  living architecture doc in the same PR).
- `crates/validate/tests/cli_surface.rs` — the named test, 19 tests.

## Carry-forward items, and what I did about each

1. **C5 survivor `lib.rs:100` (`||` → `&&`) / no test of the stdout-failure exit.** Rewrote
   the echo as one `write_all(..).and_then(|()| out.flush())` (`lib.rs:103-105`) so there is
   no `||` to flip, and on failure it now also says so on stderr (`lib.rs:106-110`) instead of
   exiting 1 silently. New test `a_failed_write_or_flush_of_the_echo_exits_non_zero`
   (`tests/cli_surface.rs:537`) uses an injected writer for write-fails / flush-ok and
   write-ok / flush-fails, asserting `EXIT_IO` and the stderr cause for each, and checks that
   the success path both writes and flushes. Reverting to the old `||`→`&&` shape or
   dropping the flush turns it red (see refutation table).
2. **Coverage lost because `env_clear` drops `LLVM_PROFILE_FILE`.** `bin()`
   (`tests/cli_surface.rs:77-87`) now passes `LLVM_PROFILE_FILE` through when the test process
   has it; the header comment (`:8-11`) no longer claims the child env is only the named
   variables. Measured: `cargo llvm-cov -p wyrd-validate --test cli_surface --summary-only`
   → **100% lines, regions and functions in all four files, `main.rs` included** (11/11
   lines; iteration 1 showed `main.rs` at 0 hits). No `*.profraw` files were left in
   `crates/validate/` afterwards.
3. **Half-pair tested in one direction only; empty-means-unset untested.**
   `half_a_pair_is_refused_in_either_direction_rather_than_skipped` (`:305`) covers all four
   half-pairs (AWS id only, AWS secret only — both in front of a full Wyrd pair — and the two
   Wyrd halves), each asserting the exact "`X` is set but `Y` is not" wording so a swapped
   `present`/`missing` fails. `an_empty_variable_counts_as_unset` (`:336`) covers an empty
   AWS id + Wyrd pair and an empty AWS pair + Wyrd pair → resolves to Wyrd.
4. **No test checked which secret is resolved; `Debug` redaction never ran.** Every
   credential case goes through `check_credentials` (`:233`), which (a) runs `run` and
   searches stdout **and** stderr for both secrets, (b) calls `resolve_config` and asserts
   `access_key_id()`, `secret_access_key()` and `source()` equal the expected pair, and (c)
   asserts `format!("{config:?}")` contains the id but not the secret. The echoed source line
   is compared against a literal (`AWS_LINE` / `WYRD_LINE`), so `CredentialSource`'s
   `Display` is bound too.
5. **`usage()` never asserted.** `no_arguments_names_every_flag_and_prints_the_usage_line`
   (`:171`) requires the exact usage line (`USAGE`, `:108`) on stderr.
6. **T4 gating finding: `main.rs:10` `std::env::var(name).ok()` treats a non-UTF-8 variable
   as unset** → a non-UTF-8 `AWS_ACCESS_KEY_ID` silently fell through to the Wyrd identity.
   See the judgment-call section below — I fixed it.

The two findings deferred to sign-off (`deferred-findings.json`) are both addressed in the
patch, as small changes. They are **judgment calls the human may overrule**:

### Judgment call A — non-UTF-8 credential variables are refused by name (the T4 finding)

- Change: the lookup now returns the raw `Option<OsString>` (`access_keys.rs:139`,
  `lib.rs:65`, `lib.rs:94`), `main.rs:12` passes `std::env::var_os` — the same call the cited
  peer uses (`xtask/src/main.rs:1559`) — and `read` (`access_keys.rs:170-180`) refuses a
  non-UTF-8 value with `CredentialError::NotUnicode(var)` naming the variable. Size: about 25
  changed lines in `src/`, plus one lib-level test (`:367`, two cases) and one binary-level
  test (`:410`), both `#[cfg(unix)]` because a non-UTF-8 `OsString` is built with
  `OsStringExt::from_vec` (Linux is the only CI platform, INTEGRATION §3).
- Why I took it rather than record-rejecting it: it is a real instance of the brief's own
  invariant (an input is "refused rather than absorbed") and of the rubric's "never silent
  skip" class — reproduced in iteration 1 as a silent identity switch. The maintainer's
  "encoding cases are OUT" ruling names *non-UTF-8 arguments* and *control characters in
  output*, which "found no defect". This one is environment values and did find one. I did
  **not** touch non-UTF-8 argv (`std::env::args()` still panics on it, `main.rs:9`) or
  output escaping.
- To revert: restore `Option<String>` in the three signatures, drop `NotUnicode` and `read`,
  and delete the two `#[cfg(unix)]` tests — about 70 lines out.

### Judgment call B — an empty flag value is refused by name

- Change: `ArgError::EmptyValue(flag)` (`args.rs:85-87`, display `:110`), checked in `parse`
  at `args.rs:142-144`. Test `each_empty_value_is_refused_by_name` (`:185`) loops all ten
  flags.
- Why: `--run-id "$RUN_ID"` with `RUN_ID` unset is a missing value, not a chosen one. The
  brief's scope (b) is "validated for presence", its STRICT decision says the new binary
  "should be the correct one", and proposal 0017 scopes every delete to the run id (§5, §15),
  so an empty run id is the dangerous case. It also makes the flag side agree with the
  credential side, where an empty variable already counts as unset. No flag has a legitimate
  empty value.
- To revert: delete the variant, its display arm, the 3-line check and the test — about 20
  lines out.

## Refutation — is the test binding? (forced)

Net-new crate: before the patch there is no crate, so the whole test file is red by absence
(`C4-verify` records `PASS (green-only)`, as the brief pre-declares). To prove each assertion
binds, I reverted each piece of production logic in turn and re-ran the test
(`timeout 600 cargo test -q -p wyrd-validate --test cli_surface`), restoring the originals
after each:

| Reverted | Result | Test(s) that went red |
|---|---|---|
| `main.rs:12` back to `std::env::var(name).ok()` | RED | `the_real_binary_refuses_a_non_utf8_aws_id_instead_of_signing_as_wyrd` |
| `read`: non-UTF-8 → `Ok(None)` | RED | `a_non_utf8_variable_is_refused…`, `the_real_binary_refuses_a_non_utf8…` |
| `(None, Some(_)) => continue` | RED | `half_a_pair_is_refused_in_either_direction_rather_than_skipped` |
| drop the empty-means-unset arm | RED | `an_empty_variable_counts_as_unset` |
| AWS id paired with `WYRD_S3_SECRET_KEY` | RED | `credentials_resolve_aws_then_wyrd_then_refuse` (+5 others) |
| `secret_access_key()` returns `""` | RED | `credentials_resolve_aws_then_wyrd_then_refuse`, `an_empty_variable_counts_as_unset` |
| `Debug` prints the real secret | RED | same two |
| write/flush check as `&&` (flush only if write failed) | RED | `a_failed_write_or_flush_of_the_echo_exits_non_zero` |
| flush skipped entirely | RED | same |
| empty flag value accepted | RED | `each_empty_value_is_refused_by_name` |
| `usage()` → `String::new()` / `"xyzzy"` | RED | `no_arguments_names_every_flag_and_prints_the_usage_line` |
| value-slot check disabled (`--bucket --typo` accepted) | RED | `a_flag_in_any_value_slot…`, `a_known_flag_in_a_value_slot…` |
| unknown flag absorbed as `endpoint` | RED | `an_unrecognised_flag_is_refused_by_name` |

(My first run of the `usage` revert reported green; that was my script misreading a build
error — the mutation left an unused variable, which `warnings = "deny"` rejects. Re-run with
the variable consumed: red, as above.)

`cargo mutants --in-diff patch.diff --no-shuffle` (the `C5-mutants` command): **42 mutants,
26 caught, 16 unviable, 0 missed** (iteration 1: 5 missed).

- **(a) Genuine red?** Yes. Every production behaviour the criteria name, reverted on its own,
  turns a named test red (table above); with no patch, the crate and its binary do not exist.
- **(b) Production path?** Yes. Binary-level tests spawn the real `wyrd-validate` built by
  cargo (`env!("CARGO_BIN_EXE_wyrd-validate")`, `tests/cli_surface.rs:78`), so they run
  `main.rs` → `wyrd_validate::run`. Library-level tests call the same public `run` /
  `resolve_config` the binary calls; the only injected pieces are the environment map and
  the stdout writer — the I/O seam the brief asks for — never a copy of the logic.
- **(c) Fixture includes the fault?** Yes. Each refusal case contains the bad input itself:
  the half-pair and non-UTF-8 cases sit **in front of a complete Wyrd pair**, so the fault
  they guard against (silently signing as Wyrd) is reachable in the fixture; the empty-AWS
  case includes the empty variable; the value-slot test puts `--typo` in every one of the ten
  value slots; the write/flush test uses writers that actually return errors.

## Verification run

- `cargo fmt -p wyrd-validate` applied; `cargo clippy -p wyrd-validate --all-targets -- -D
  warnings` clean; `typos` over `crates/validate`, the doc and `Cargo.toml` clean.
- `cargo test -p wyrd-validate` (bounded by `timeout`): 19 passed.
- **`cargo xtask ci` via `./engine/xtask.sh ci`** (the project's gate runner, pointed at the
  worktree): **exit 0, "xtask ci: all checks passed"**. Every step ran — `typos`, docs lint
  and render, gitlink-guard, unsafe-guard ("every crate root forbids unsafe code"), `cargo fmt
  --check`, clippy, build, `cargo test --workspace` (`tests/cli_surface.rs`: 19 passed),
  cargo-machete, the three `cargo deny` legs, statics, deploy-guard, and the madsim DST clippy
  + test. The `cargo deny` output carries three `license-not-encountered` /
  `advisory-not-detected` warnings; they are about existing allowlist entries, not this
  dependency-free crate.
- `cargo llvm-cov -p wyrd-validate --test cli_surface --summary-only`: 100% lines / regions /
  functions in `access_keys.rs`, `args.rs`, `lib.rs`, `main.rs`.

## Considered and ruled out

- **`clap` or any other crate** — ruled out by the brief (zero new crates in this slice).
- **Depending on `wyrd-server` to reuse `ParsedArgs`** — the edge child-3's lint forbids, and
  its value-slot behaviour is what criterion 4 departs from.
- **`env_remove` of the four credential variables instead of `env_clear` in `bin()`** — would
  also keep `LLVM_PROFILE_FILE`, but lets anything else in the developer's shell (a stray
  `AWS_PROFILE`, a future variable the binary starts reading) leak into the tests. Kept
  `env_clear` plus one explicit pass-through (4 lines, `:83-85`).
- **Merging the four credential test functions into one table** — saves roughly 30 lines of
  the 571, but loses the per-group test names the carry-forward refers to. Kept them separate.
- **Refusing a `--flag=value` spelling with a special message** — it is already refused, by
  name, as an unrecognised flag (`args.rs:130-134`); a dedicated hint is polish for later.
