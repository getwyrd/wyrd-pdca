# Build notes — issue 894 / multipart-admission-create (attempt 5)

Target: `getwyrd/wyrd` `main`. The worktree base is `d2bd810` (`pdca-integrate: issue_671`), the
same base attempts 3 and 4 used; `git diff --stat 71e9b9f d2bd810` is empty, so the brief's
citations hold. Lines below cite the patched tree unless marked "base".

Starting point: attempt 4's `iteration-v4/patch.diff`, applied cleanly to `d2bd810`. The
iteration-4 carry-forward says: "Code is correct; the only gaps are two unpinned rules. Add exactly
two tests, change no production code." This attempt does exactly that.

**What changed against attempt 4, and nothing else.** `git diff-tree` between base + v4 patch
(tree `2ee98ee9`) and base + this patch (tree `2f2447c2`) lists one path:
`crates/core/tests/multipart_create.rs` (+132 / −11). `crates/core/src/multipart/create.rs`,
`crates/core/src/multipart.rs`, the DST scenario, `support/mod.rs`, the state-machine test and the
architecture doc are byte-identical to attempt 4's.

## The two tests

### (1) `c6_iv_this_creates_request_under_another_nonce_is_still_a_collision` (`crates/core/tests/multipart_create.rs:2147-2210`)

Pins the nonce conjunct of `SessionRecord::same_session`
(`crates/core/src/multipart.rs:2439-2457`, the conjunct at `:2456`), as reached through
`Identity::judge` (`crates/core/src/multipart/create.rs:509-518`) and the loop's `Found::Moved`
branch (`:376-378`).

The case: create A is handed identity 1 (upload id U, nonce N1) for `photos/first`. Its first
commit comes back `CommitUnknownResult { may_still_commit: false }`, **unapplied**; inside that
commit a **real** second create (the same `create_session`, on the inner store) is minted with the
same U, [`RIVAL`]'s nonce N2 and the **same request** (bucket, object `photos/first`, no content
type, the same clock stamp and clock source), and installs its session. A's re-read finds a record
under U that differs from R in the segment-group nonce alone.

Expected, and asserted: A answers `Created` under identity 2 (a collision, never
`InDoubt(SessionMoved)`); `script.minted == [identity(1), identity(2)]`; `script.waits` empty (a
collision spends no contention unit); exactly two commits, the first under U and the second under
identity 2; the rival's record byte-identical afterwards; `mpuctl.count() == 2`; and the whole
population invariant. The fixture also asserts its own premise, field by field: the rival's record
and R agree on `parent`, `object`, `content_type`, `created_at_millis` and `clock_source`, and
differ in the nonce (`:2192-2204`). So if `request()` or `RivalFirst` ever drifts and the rival
differs in some other field, the leg fails on its premise rather than passing for the wrong
reason.

Why this needs its own leg: C6 (iii) (`:2111`) gives the rival this create's nonce under another
object, so it pins the `object` conjunct; C6 (i) gives it another nonce **and** another object, so
any one conjunct keeps it green. Only a same-request rival under another nonce distinguishes the
nonce conjunct. Without it, a rival handed the same upload id for the same key would put this
create into `InDoubt` over a session it never wrote, and the caller would be told to re-read an
upload id that belongs to another client.

Wrapper change the carry-forward asked for: `OnCommit::RivalFirst` takes the rival's object
(`object: &'static str`, `:722-730`; the arm at `:1075-1087` runs `request(&object(name), None)`).
Its four existing uses pass `object: "rival"` (`:1958-1962`, `:2023-2027`, `:2066-2070`,
`:2114-2118`), so their behaviour is unchanged; the new leg passes `object: "first"`.

### (2) `b_a_missing_bucket_is_judged_before_a_full_ledger` (`:1225-1269`)

Pins the read order bucket → ledger: the bucket block (`create.rs:407-415`) before the ledger
block (`:417-444`).

The case: a plain `RedbMetadataStore::in_memory()` with **no** `bucket:` marker and `mpuctl`
stored at `count == max_sessions` (`profile_of(1)`, `ledger_bytes(1, …)`, the codec's own bytes,
round-trip asserted by `ledger_bytes` and again by `ledger()` before the create runs). The create
runs under that profile.

Expected, and asserted: `Refused(NoSuchBucket)`, not `SlowDown { SessionCap { count: 1,
max_sessions: 1 } }`; `script.waits` empty; the store byte-identical afterwards (the brief's
definition of "writes nothing"). It uses the bare store, like its sibling
`b_a_missing_bucket_answers_no_such_bucket_and_writes_nothing` (`:1210`), because the refusal comes
from the read, before any batch is built.

Why this needs its own leg: the sibling has no ledger at all, so with the blocks swapped the ledger
block bootstraps and falls through to the bucket check, and it stays green. Only a **full** ledger
beside a missing bucket tells the two orders apart.

### Module header

The test module's (B) and (C) bullets name the new legs (`:18-21`, `:26-29`). No production
rustdoc or architecture text changed: neither leg adds or alters an API, a port, a flag or a
persisted field, so the rubric's docs-currency rule asks for nothing here.

## Demonstrated reds (the convergence contract), re-run on this tree

Same method as attempts 3 and 4: `mutate.py` in the scratch dir applies one edit to a pristine
copy of the production file, runs `timeout 900 cargo test -p wyrd-core --test multipart_create`,
records the failing legs and restores the file; `create.rs` edits carry a temporary
`#![allow(unused, unreachable_code, unreachable_patterns)]` (the workspace denies warnings). The
tree hash after the sweep (`git write-tree`) was `8a0d6b16`, the same as the patch's at that
point, so the restore is confirmed. (The shipped tree is `2f2447c2`: after the sweep I reflowed
one ragged doc-comment bullet in the test module header, `:18-23`, a comment-only edit, and
re-ran the gate on the result; see Gate.) The baseline before the sweep: `multipart_create`
**43 passed** (41 + the 2 new).

**The two new rows.** Each failed **only** the leg written for it (42 passed, 1 failed):

| Mutant | Edit | Failed | The wrong answer it produced |
|---|---|---|---|
| `V5-nonce` | delete the nonce conjunct of `same_session` (`multipart.rs:2456`; the destructured `segment_nonce` becomes `_` so it compiles) | `c6_iv_this_creates_request_under_another_nonce_is_still_a_collision` | `expected Created, got Err(InDoubt(InDoubt { upload_id: UploadId("…0001"), segment_nonce: SegmentNonce("5a5a…5a5b"), may_still_commit: false, …, cause: SessionMoved }))` — in doubt over the rival's session, under the shared id |
| `V5-bucket-after-ledger` | move the bucket block (`create.rs:407-415`) to after the ledger block (`:417-444`), before the contention block | `b_a_missing_bucket_is_judged_before_a_full_ledger` | `left: Refused(SlowDown { pressure: SessionCap { count: 1, max_sessions: 1 } })  right: Refused(NoSuchBucket)` |

Under `V5-nonce`, C6 (iii) and all three F10 legs stay green; under `V5-bucket-after-ledger`, the
existing missing-bucket leg and the marker-moved leg stay green. That is the gap the sign-off
named, now closed by one leg each.

**The inherited table, re-run.** Production code is byte-identical to attempt 4's, and every named
leg is unchanged, so the results match attempt 4's row for row. Recorded again here from this
sweep (`results.json` in the scratch dir; "also failed" lists other legs the same edit broke):

| # | Edit | Named leg failed | Also failed |
|---|---|---|---|
| 1 | contention budget = `MAX_UPLOAD_ID_ATTEMPTS` | `a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted` | A storm, A′ second run, B contention, C2, F4, F8 (c) |
| 2 | contention budget = `MAX_ADMISSION_CAS_ATTEMPTS` | `a_prime_the_contention_budget_follows_the_profile_in_effect` | F6, F8 (c) |
| 3 | one counter for losses and collisions | `c2_the_contention_and_collision_budgets_never_share` | — |
| 4 | drop `require_absent(mpu:<id>)` | `c3_an_id_taken_under_the_commit_is_re_minted_and_left_as_it_is` | C2 |
| 5 | drop `require_absent(seggrp:<nonce>)` | `c4_a_nonce_taken_under_the_commit_is_re_minted` | — |
| 6 | drop both identity preconditions | `f3_a_late_batch_is_excluded_by_the_identity_keys_not_the_ledger` | C2, C3, C4 |
| 7 | drop the bucket `require` | `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` | F5 every-other-ending |
| 8 | re-mint after an unknown outcome | `f1_…`, `f2_…` | 19 more (C5 re-mint, C6 (ii), **C6 (iv)**, all three F10, F3–F9) |
| 9 | replay the original batch after an unknown outcome | `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` | 16 more (C5 re-mint, **C6 (iv)**, all three F10, F1, F4–F9) |
| 10 | any record found under the id is `Exact` (attempt 1's code) | `c6_i_…`, `c6_ii_…` | C6 (iii), **C6 (iv)**, C5 re-mint, all three F10 |
| 11 | collision check only after an unknown outcome (no absence read) | `c5_a_byte_identical_record_is_never_adopted` | C1 ×2, C2, C3, C4, C5 re-mint |
| 12 | `observe` keeps only the latest outcome | `f6_a_batch_that_may_still_land_keeps_the_create_in_doubt` | F8 (e) |
| 13a | reset the doubt on a read fault | `f8_a_a_held_create_stays_in_doubt_past_a_read_fault` | F8 (d), F8 (e), F10 settling read |
| 13b | reset the doubt on a plain commit fault | `f8_b_a_held_create_stays_in_doubt_past_a_commit_fault` | — |
| 13c | reset the doubt on a failed settling read | `f8_c_a_held_create_stays_in_doubt_past_a_failed_settling_read` | F8 (d), F8 (e) |
| 14 | a failed settling read clears `unsettled` (ends `Store`) | `f8_d_an_unsettled_1021_with_a_failed_settling_read_ends_in_doubt` | — |
| 15 | on an unknown outcome at the last budgeted unit, refuse without reading | `f4_an_unknown_outcome_on_the_last_budgeted_attempt_is_settled_before_returning` | F6, F8 (c) |
| 16 | `SessionCap` returned without the settling read | `f5_a_capacity_refusal_met_in_recovery_is_checked_against_the_earlier_batch` | — |
| 17 | `InDoubt`'s `source()` = the stop cause (attempt 1's code) | `f8_a_…` (`Transient`, not `Indeterminate`) | C6 (ii), F5 every-other-ending, F6, F8 (b)–(e), all three F10 |
| 18a | `>` → `>=` in the size check | both size legs | — |
| 18b | drop `Completing` from `widest_reachable_bytes` | `…_completing_widest` only | — |
| 18c | drop `Completed` from `widest_reachable_bytes` | `…_completed_widest` only | — |
| 19 | (G) `Created` straight after a 1021 | `multipart_admission_invariants_hold_under_the_dst_seed_sweep` (reverse assertion: "answered Created for …, but no such session exists — a Created that is not real") | the 1021 leg, the mixed leg |
| 20 | (G) `InDoubt` names the upload id reversed | `the_witness_leg_holds_and_a_held_in_doubt_batch_lands_by_quiesce` ("session … is stranded: no create answered Created for it or named it in an in-doubt error") | the seed sweep |

Rows 19 and 20 ran the way `xtask::run_dst` runs the DST tier (`RUSTFLAGS=--cfg madsim`,
`MADSIM_TEST_NUM=50`, `cargo test -p wyrd-dst --test multipart_admission`, `mutate_dst.py`), with a
pristine control first (every leg green). The tree hash after them is `2f2447c2`, the shipped
tree, so the restore is confirmed.
| 21 | a `Conflict` whose re-read finds the same ledger bytes → collision | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | C2, C3, C4 |
| 22 | the same, retried without spending a unit | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | — |

Attempts 2–4's mutants, re-run on this tree: `X-373`, `X-392`, `X-382`, `X-535` each fail
`f5_every_other_ending_…`; `X-362` fails `c5_…_after_a_re_mint`; `X-564` and `X-343` fail
`f9_a_1021_settled_absent_…`; `X-349` fails `f5_a_held_batch_landing_between_the_identity_reads_…`
(and the F10 held leg); `T4-all` fails all three F10 legs; `T4-loop`, `T4-finish`, `T4-held` each
fail their one F10 leg; `T4-nonce` and `T4-object` fail C6 (iii); `CF-standing` fails F8 (e);
`CF-ledger-order` fails the ledger-order leg; `CF-bucket-order` fails the marker-moved leg (and
the ledger-order leg). `X-505` (`else if self.held.is_none()` → `else`) still survives, as attempts
3 and 4 recorded: it changes only which held outcome `InDoubt.outcome` reports.

Note on `T4-nonce` vs `V5-nonce`: `T4-nonce` (attempt 4) judges `Moved` by the nonce **alone**,
which C6 (iii) catches through the `object` mismatch. `V5-nonce` is the complementary edit, every
field **but** the nonce, which only the new C6 (iv) catches.

**Known equivalent mutation:** dropping `Open` or `Aborting` from `widest_reachable_bytes` changes
no answer. Not run, per the brief.

## Refute your own test

- **(a) Genuine red?** Yes, through mutation, because `create_session` does not exist on the base
  (C4-verify is UNVERIFIABLE, exit 77, as the brief pre-declares for this net-new slice). For the
  two legs this attempt adds, the binding checks are `V5-nonce` and `V5-bucket-after-ledger` above:
  each turns its leg red with the exact wrong answer the sign-off described, and each leg is the
  **only** one that goes red, which is what shows these two legs are what pin the two rules.
- **(b) Production path?** Yes. Both legs call `wyrd_core::multipart::create::create_session` over
  a real `RedbMetadataStore::in_memory()`. In C6 (iv) the rival is itself a real `create_session`
  on the inner store, and the record A's re-read meets is what that create wrote. In the B leg the
  ledger is written with the codec's own bytes and read back through `decode_admission_record`.
  Nothing re-implements the create; the `Scripted` wrapper only withholds one commit's outcome.
- **(c) Fixture includes the fault?** Yes. C6 (iv): the rival really took the shared upload id
  (asserted), its record really differs from R in the nonce alone (asserted field by field), the
  1021 really was unapplied (the wrapper returns the error without committing; the two recorded
  commits are identity 1's and identity 2's), and the rival's record is re-read afterwards. B: the
  ledger really is at `count == max_sessions` and the marker really is absent, both asserted
  before the create runs, so a `SessionCap` answer is reachable and the leg can tell the orders
  apart.

## Gate

`cargo xtask ci` through the project's runner (`PDCA_WORKTREE=… ./engine/xtask.sh ci`), **run
1** on tree `8a0d6b16` (everything but the comment reflow): **exit 0, "xtask ci: all checks
passed"**, no `FAILED` line in the log (`ci.log`). Steps it ran: `typos`; `lint_docs.py` (OK);
`render_site.py --check` (99 pages, link audit OK), so both of Plan's external dependencies
(`typos`, the doc renderer) were present and exercised; the gitlink, unsafe and blackbox guards;
`cargo fmt --all -- --check`; workspace clippy, build and test (`multipart_create` **43/43**,
`multipart_state_machine` 22/22); cargo-machete; the three `cargo deny` checks; conformance; the
ADR-0035 statics gate; the deploy guard; and the DST tier under `--cfg madsim` (clippy, then tests;
`multipart_admission` **7/7**, every other DST file green).

**Run 2**, on the shipped tree `2f2447c2` (after the comment reflow and the DST mutation rows,
whose restore left the tree at that hash): **exit 0, "xtask ci: all checks passed"**, no `FAILED`
line (`ci-final.log`); the same steps, `multipart_create` **43/43**, `multipart_state_machine`
22/22, `multipart_admission` **7/7** under `--cfg madsim`. So the result above is for the shipped
bytes.

Commit-readiness: the target repo configures no git hooks (no `core.hooksPath`, no pre-commit
config); its formatter (`cargo fmt --all -- --check`) passed on the final worktree and runs again
inside the gate, as does `typos`.

`patch.diff` (239,610 bytes) applies to `d2bd810` (`git apply --cached --check` into a scratch
index), and base + patch gives exactly the worktree's tree (`git write-tree` of both is
`2f2447c2`). The bundle
carries the core test at the root (`multipart_create.rs`, as before) and at
`crates/core/tests/multipart_create.rs`, and the unchanged DST scenario at
`crates/dst/tests/multipart_admission.rs`.

## Open items for sign-off

1. **Size flag:** the patch is 240 KB (attempt 4 was 234 KB), over the 100 KB backstop, accepted at
   Plan 2026-10-17 and at every sign-off since. No leg was dropped or merged.
2. **Strictly within "no other changes":** beyond the two legs, the only edits are the `RivalFirst`
   `object` field the carry-forward itself asked for, `object: "rival"` at its four existing uses
   (behaviour unchanged), and two bullets in the test module's header. No production line changed.
3. **`create.rs:557` (`X-505`) is still unpinned**, as attempts 3 and 4 recorded (cosmetic: which
   held outcome `InDoubt.outcome` reports).
4. Attempts 2–4's other open items stand as recorded in their notes (the torn-`mpu:` reading, no
   DST fencer, F8 (d)'s reading, await bounds, cancel safety in the other C-1 texts).

## Reading scope

Beyond `brief.md` (with its four carry-forward blocks): `iteration-v4/patch.diff` (the starting
point the carry-forward names), `iteration-v4/build-notes.md` (to carry the reds table forward),
and my own prior scratch tooling (`mutate.py`, `mutate_dst.py` in the v4 scratch dir). On the
target: `crates/core/src/multipart/create.rs` (whole), `crates/core/src/multipart.rs` around
`same_session` and the `SessionRecord` accessors, the existing test file's helpers, wrapper and the
B / C6 legs, `AGENTS.md` § Review rubric (as given in the prompt). For the runner: `pdca.toml`,
`docs/INTEGRATION.md` §3, `engine/xtask.sh`. I did not read any SUMMARY.md or check-* file.

## Scratch

Mutation scripts, pristine copies, per-row logs, `results.json`, the v4→v5 interdiff and the gate
log are in `$PDCA_SCRATCH/pdca-builder-894-mutants-v5`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_894/pdca-builder-894-mutants-v5`), inside the harness's
per-issue scratch root. Left for the harness to reclaim, as in earlier attempts.
