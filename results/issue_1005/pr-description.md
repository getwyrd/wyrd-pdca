## Summary
**User impact:** someone validating a Wyrd server has no way to tell whether it answered each S3 request the way this release should. A correct refusal, a wrong error, and a feature that has quietly started half-working all look alike, and a server error can't be told apart from a network failure. Nothing changes for users yet: this is the rules the validation tool will apply, and no command uses them so far.

This PR adds those rules to `wyrd-validate`: what each S3 operation should do on a given server version, and how a set of observed answers becomes PASS, FAIL or INCONCLUSIVE.

Tracked in #1005. The checks for operations that must succeed follow in #1006.

## What to look at
The main thing to judge is the order in which an answer is judged. A wrong answer always counts as a failure, even if the response was later cut off. A result only counts as "unproved" when nothing was found wrong, and a run can only pass if every row it must prove was proved.

To try it, run `cargo test -p wyrd-validate --test capability_matrix`. It needs no server, network or Docker, because every test passes a fixed answer to the rules.

Two decisions deserve a reviewer's opinion, because #1006 builds on them:
- A delete's follow-up checks are recorded as a list of key-and-answer pairs, so each check names its key and a retry has somewhere to go.
- A bodyless refusal (`HeadBucket`) passes when the client read no error code at all, as well as when it reported "no body". Any code that was read still fails it.

## Root cause
`crates/validate` has an S3 client but nothing that states what a release owes each operation, selects that by version, or judges an answer. Proposal 0017 §3 and §7 specify this, and its text had gone stale: it still said the gateway advertises no version, and it budgeted a 5xx as an availability failure.

## Fix
- **New module `crates/validate/src/matrix.rs`.** It does no I/O and reads no clock, and the client's `ErrorCode` is the only client type it uses.
- **Rows and profile.** Each operation is `required`, `unsupported` (with an exact status and code) or `supported, not checked`. The `pre-0.1` profile declares today's 22 rows, each citing the gateway line behind it.
- **Version selection.** It parses the reported version and selects the profile. `unknown` and `+git.unknown[.dirty]` select no profile, and neither do a malformed version or one no profile covers. The result says which of these happened.
- **Judging an exercise.** `classify` handles another row's exercise and a missing response head first. It then runs every check and reports the first failure in message order: request, status, headers, body, follow-ups. Only then can a result be unsettled.
- **`required` rows.** For now these are judged only on the 5xx rule and are otherwise unsettled, with a private `required` function for #1006 to fill in.
- **Run verdict.** `verdict` gives FAIL on any fatal result, naming each row and class. It gives INCONCLUSIVE if a row that must be proved wasn't, or if nothing was proved. Otherwise it gives PASS and lists the `supported, not checked` rows as not covered.
- **Proposal 0017.** Adds the third row state, reads the version from the wire, and makes a 5xx fatal as `protocol`. It also records that `pre-0.1` is edited in place until `0.1.0`, including what that costs for a release candidate.

## Verification
Lines in `matrix.rs` and `capability_matrix.rs` are from this PR's new files. Other paths are on `main` at `376b106`, which those files are built on.

- **Claim:** today's profile matches what the gateway really answers.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1865-1871` (`CopyObject` 501), `:1842-1849` and `:1797-1804` (subresource denylist 501), `:1809-1815` and `:1819-1826` (bucket and root paths 400), `:1548-1553` (`HEAD` has no body), against `PRE_0_1_ROWS` at `crates/validate/src/matrix.rs:284-337`.
  - **Test:** `pre_0_1_declares_exactly_todays_rows` (`crates/validate/tests/capability_matrix.rs:278`).
- **Claim:** an unsupported operation that succeeds in any part is fatal, whatever its body did afterwards.
  - **Checked:** `refusal`, `matrix.rs:941-996`.
  - **Test:** `any_2xx_to_an_unsupported_row_is_unexpected_support` (`capability_matrix.rs:694`).
- **Claim:** a refusal passes only on the exact status and code.
  - **Checked:** `DeclaredCode::matches`, `matrix.rs:73-80`.
  - **Test:** `unsupported_row_passes_only_on_the_declared_status_and_code` (`:600`), `head_bucket_passes_on_a_400_with_no_body_only` (`:653`).
- **Claim:** a cut or other unproved fact never hides a failure that already arrived, and the first failure in message order names the class.
  - **Checked:** `Evidence::settle`, `matrix.rs:863-885`; checks add findings and never return early.
  - **Test:** `the_first_failing_check_in_message_order_names_the_class` (`:428`), `nothing_that_leaves_an_exercise_unproved_stops_a_later_check` (`:475`).
- **Claim:** a 5xx is `protocol` on every row unless that row declares that status.
  - **Checked:** `shared_facts`, `matrix.rs:926-935`.
  - **Test:** `a_5xx_is_protocol_on_every_row_unless_the_row_declares_that_status` (`:530`).
- **Claim:** an unknown version is never read as `0.0.0`, and the version changes the verdict.
  - **Checked:** `crates/server/src/version/derivation.rs:33-68` for the version shapes, against `parse_version`, `matrix.rs:426-481`, and `select`, `:515-529`.
  - **Test:** `unknown_is_never_read_as_release_0_0_0` (`:847`), `each_version_selects_its_own_profile_and_the_rows_flip_with_it` (`:885`).
- **Claim:** a run never claims more than its answers prove.
  - **Checked:** `verdict`, `matrix.rs:1067-1140`.
  - **Test:** `an_unproved_required_row_keeps_the_run_inconclusive` (`:1091`), `a_row_is_proved_only_when_every_exercise_of_it_passed` (`:1163`), `a_run_that_proves_no_row_is_inconclusive` (`:1179`).

**Red and green.** This is new code, so without the module the test file doesn't compile and there is no ordinary failing run from before the change. Instead, each key rule was removed in turn and the named test failed on its assertion: `unexpected-support` (`:710`), wrong code (`:614`), `+git.unknown` (`:854`), and an unsettled `required` row folding into PASS (`:1115`). With the rules in place, all 30 tests pass.

**Wider checks.** `cargo xtask ci` passes, and the `tikv` feature compiles. Tests run 89.4% of the changed lines that can be instrumented (390 of 436). `cargo mutants --in-diff` (which breaks the changed code in small ways to see if tests notice) reports 89 caught, 27 that didn't compile, and none missed.

**Known limits.**
- One hand-made mutation did get past the tests: skipping the error-code check when the body is `Body::Bytes`. The current code correctly calls that input `protocol`, but no test pins it. The client can't produce that shape today, because its service error carries the code but no raw body (`crates/validate/src/s3/error.rs:16-25`).
- `cargo doc -p wyrd-validate --no-deps` fails on an existing link at `crates/validate/src/sizes.rs:340`. This PR doesn't touch that file, and `cargo xtask ci` doesn't run `cargo doc`.

Fixes #1005
