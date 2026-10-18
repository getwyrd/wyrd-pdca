# Build notes — issue 894 / multipart-admission-create (attempt 3)

Target: `getwyrd/wyrd` `main`. The worktree base is `d2bd810` (`pdca-integrate: issue_671`), and
`git diff 71e9b9f d2bd810` is empty, so the brief's base citations hold. Lines below cite the
patched tree.

Starting point: attempt 2's `iteration-v2/patch.diff`, applied cleanly to `d2bd810`. The
carry-forward said the code is correct and asked for two things only: tests that pin the
adversary's four unpinned mutants, and a cancel-safety paragraph in `create_session`'s
"Termination" rustdoc. "No other changes." This attempt does exactly that. No production logic
changed; the only production edit is the rustdoc paragraph.

Line shift: the new rustdoc paragraph adds 9 lines above the function body, so every attempt-2
line the adversary cited in `create.rs` is now 9 higher (`:343` → `:352`, `:349` → `:358`,
`:362` → `:371`, `:373` → `:382`, `:382` → `:391`, `:392` → `:401`, `:535` → `:544`, `:564` →
`:573`).

## What changed against attempt 2

### Production: `crates/core/src/multipart/create.rs:306-313` (rustdoc only)

A new paragraph at the end of the "Termination" section: the future is not cancel-safe once a
commit is sent. Dropping it before the first commit writes nothing. Dropping it after can leave
a batch that lands with no `Created` and no `InDoubt` naming it, which is `0016:2015-2018`'s
case (checked on the base: "Erroring out, or retrying under a new upload id, can therefore leave
an `Open` session no client will ever name"). So callers bound a create through the store's own
deadlines, never by dropping the future. It cites `crates/traits/src/lib.rs:1334-1345` (checked:
"every operation terminates", and "a `commit` abandoned at a deadline is undetermined"). Wiring
the caller's deadline is left to #668.

I did not touch the module header's C-1 sentence (`create.rs:15-19`) or the
`05-building-block-view.md:206` paragraph, though the adversary noted both promise "no session
goes unreported" without the cancel caveat. The human said "no other changes". See open item 2.

### Tests: `crates/core/tests/multipart_create.rs` (31 legs → 35)

Wrapper change (test-only). Attempt 2's one-purpose `on_ledger_read: Option<usize>` hook is
generalized into `on_read: Option<OnRead>` (`:834`). `OnRead` (`:784`) names which read triggers
it (`Watch`: the `seggrp:`, `mpuctl` or `bucket:` read, `:746`), whether to release the held
batch first, and what to write after (`Meddle`: nothing, admit N, delete the marker, rewrite
`mpuctl` under another profile, tear `mpuctl`, `:767`; applied by `Scripted::meddle`, `:891`).
It fires once, on the first matching read while a batch is held (`:946-958`). F5's existing leg
now uses `OnRead { Ledger, release: true, Admit(2) }`, which is the old behaviour exactly. The
torn-ledger bytes became the shared `TORN_LEDGER` const (`:794`). State is still in struct
fields; no `static` (ADR-0035).

New legs, one per carry-forward item:

| Item | Mutant (attempt-2 line → now) | New leg |
|---|---|---|
| (1) settling read before `NoSuchBucket` / `ProfileSkew`, and the torn ledger met in doubt | `:373`→`:382`, `:392`→`:401`, `:382`→`:391`, `:535`→`:544` | `f5_every_other_ending_met_in_recovery_is_checked_against_the_earlier_batch` (`:2172`) |
| (2) doubt reset on re-mint, the C5 case on a re-minted identity | `:362`→`:371` | `c5_a_byte_identical_record_is_never_adopted_after_a_re_mint` (`:1809`) |
| (3) "found something else and not held, the ending stands" after a 1021 (X1, X2) | `:564`→`:573`, `:343`→`:352` | `f9_a_1021_settled_absent_leaves_nothing_in_doubt` (`:2593`) |
| (4) settling read in the held, identity-taken branch | `:349`→`:358` | `f5_a_held_batch_landing_between_the_identity_reads_is_answered_created` (`:2254`) |

What each leg does:

- **(1)** Commit 1 is held. Three interferences (marker deleted on the `bucket:` read; `mpuctl`
  rewritten at its count under `profile_of(9)` on the `mpuctl` read; `mpuctl` torn on the
  `mpuctl` read), each run twice:
  - *released*: the hook releases the held batch first, so it lands and is counted. The
    create must answer `Created` under its original identity, with exactly R stored and one
    marker. The mutants return the refusal or the ledger error and strand the session.
  - *not released*: the create must end `InDoubt { may_still_commit: true }` naming its
    identity, with the matching cause (`Refused(NoSuchBucket)`, `Refused(ProfileSkew { local,
    ledger })` with both profiles checked, or `Unsettled::Ledger` whose `Display` says "the
    mpuctl ledger is torn"). Released afterwards, the held batch meets `Conflict` and no session
    exists. This drives `create.rs:544` (`Ending::Ledger` → `Unsettled::Ledger`) and the
    `Unsettled::Ledger` `Display` arm (`:186`), which the diff-coverage gate listed as missed.
  Every case goes through `in_doubt`, so F9's contract holds for these endings too.
- **(2)** The adversary's own case. Identity 5's R is planted by a real create for the same
  request. The create's first identity meets `RivalFirst { may_still_commit: false }` (C6 (i)'s
  schedule), collides with the rival's record, and re-mints into identity 5, whose read finds R.
  The answer must be `IdentityCollisions { attempts: MAX_UPLOAD_ID_ATTEMPTS }`, with one commit
  (under identity 1), the planted and rival records byte-identical, and the ledger up by the
  rival's admission only. The mutant answers `Created(identity 5)`, which is the planted session.
- **(3)** Both of the adversary's cases. X1: a 1021 unapplied, the next read fails with a
  `TransientFault`, and the settling read succeeds and finds nothing, so the answer must be
  `CreateError::Store` and classify `Transient`. X2: a 1021 unapplied, the re-read finds
  nothing, the re-drive's commit fails, and every `mpu:` read after it fails, so the answer must
  be `CreateError::Store`, not `Indeterminate`. Both write nothing.
- **(4)** Commit 1 is held, and the hook releases it on the recovery's `seggrp:` read (after the
  `mpu:` read found nothing). The view shows the identity's marker with no session, so the create
  is held with its identity looking taken. It must read once more and answer `Created`; the
  mutant answers `InDoubt(IdentityTaken)`.

The module doc header's (C) and (F) bullets now mention the re-minted identity and the settled
doubt.

### Not changed

`crates/core/src/multipart.rs`, `crates/core/tests/multipart_state_machine.rs`,
`crates/dst/tests/multipart_admission.rs`, `crates/dst/tests/support/mod.rs` and
`docs/design/architecture/05-building-block-view.md` are exactly attempt 2's. Attempt 2's notes
(the witness leg's `defer_in_flight_landings` measurement, the size crossover at 151/152/153
bytes, and the F8 (d) reading) still apply unchanged and are not repeated here; they are in
`iteration-v2/build-notes.md`.

## Demonstrated reds (the convergence contract), re-run on this tree

Every row of the brief's table, plus the adversary's mutants, was re-run on the final tree. A
scratch script applied each edit to a pristine copy of the production file, ran `cargo test -p
wyrd-core --test multipart_create` under `timeout 900`, recorded the failing legs, and restored
the file. `cmp` against the pristine copies confirmed the restore after the last row. Edits to
`create.rs` were made with a temporary `#![allow(unused, unreachable_code)]` at its top, because
the workspace denies warnings and some edits leave a binding unused. Rows 19 and 20 were run the
way `xtask::run_dst` runs the DST tier (`RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=50`, `cargo
test -p wyrd-dst --test multipart_admission`).

Every edit failed its named leg. The "Also failed" column lists other legs the same edit broke.

| # | Edit | Named leg failed | Also failed |
|---|---|---|---|
| 1 | contention budget = `MAX_UPLOAD_ID_ATTEMPTS` | `a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted` | A storm, A′ second run, B contention, C2, F4, F8 (c) |
| 2 | contention budget = `MAX_ADMISSION_CAS_ATTEMPTS` | `a_prime_the_contention_budget_follows_the_profile_in_effect` | F6, F8 (c) |
| 3 | one counter for losses and collisions | `c2_the_contention_and_collision_budgets_never_share` | — |
| 4 | drop `require_absent(mpu:<id>)` | `c3_an_id_taken_under_the_commit_is_re_minted_and_left_as_it_is` | C2 |
| 5 | drop `require_absent(seggrp:<nonce>)` | `c4_a_nonce_taken_under_the_commit_is_re_minted` | — |
| 6 | drop both identity preconditions | `f3_a_late_batch_is_excluded_by_the_identity_keys_not_the_ledger` | C2, C3, C4 |
| 7 | drop the bucket `require` | `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` | F5 every-other-ending |
| 8 | re-mint after an unknown outcome | `f1_…`, `f2_…` | 14 more (C5 re-mint, C6 (ii), F3–F9) |
| 9 | replay the original batch after an unknown outcome | `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` | 12 more |
| 10 | `Created` for any record found under the id (loop and settling read; attempt 1's code) | `c6_i_…`, `c6_ii_…` | C5 re-mint |
| 11 | collision check only after an unknown outcome (no absence read) | `c5_a_byte_identical_record_is_never_adopted` | C1 ×2, C2, C3, C4, C5 re-mint |
| 12 | `observe` keeps only the latest outcome | `f6_a_batch_that_may_still_land_keeps_the_create_in_doubt` | — |
| 13a | reset the doubt on a read fault | `f8_a_a_held_create_stays_in_doubt_past_a_read_fault` | F8 (d) |
| 13b | reset the doubt on a plain commit fault | `f8_b_a_held_create_stays_in_doubt_past_a_commit_fault` | — |
| 13c | reset the doubt on a failed settling read | `f8_c_a_held_create_stays_in_doubt_past_a_failed_settling_read` | F8 (d) |
| 14 | a failed settling read clears `unsettled` (ends `Store`) | `f8_d_an_unsettled_1021_with_a_failed_settling_read_ends_in_doubt` | — |
| 15 | on an unknown outcome at the last budgeted unit, refuse without reading | `f4_an_unknown_outcome_on_the_last_budgeted_attempt_is_settled_before_returning` | F6, F8 (c) |
| 16 | `SessionCap` returned without the settling read | `f5_a_capacity_refusal_met_in_recovery_is_checked_against_the_earlier_batch` | — |
| 17 | `InDoubt`'s `source()` = the stop cause (attempt 1's code) | `f8_a_…` (`Transient`, not `Indeterminate`) | C6 (ii), F5 every-other-ending, F6, F8 (b)–(d) |
| 18a | `>` → `>=` in the size check | both size legs | — |
| 18b | drop `Completing` from `widest_reachable_bytes` | `…_completing_widest` only | — |
| 18c | drop `Completed` from `widest_reachable_bytes` | `…_completed_widest` only | — |
| 19 | (G) `Created` straight after a 1021 | `multipart_admission_invariants_hold_under_the_dst_seed_sweep` (reverse assertion) | the 1021 leg, the mixed leg |
| 20 | (G) `InDoubt` names the upload id reversed | `the_witness_leg_holds_and_a_held_in_doubt_batch_lands_by_quiesce` (stranded record) | the seed sweep |
| 21 | a `Conflict` whose re-read finds the same ledger bytes → collision | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | C2, C3, C4 |
| 22 | the same, retried without spending a unit | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | — |

The adversary's mutants (attempt-2 line → current line). Each failed **only** the new leg
written for it, which shows that attempt 2's 31 legs did not pin them and that the new legs do:

| Mutant | Edit | Failed |
|---|---|---|
| `:373`→`:382` | `NoSuchBucket` returned without `finish` | `f5_every_other_ending_met_in_recovery_is_checked_against_the_earlier_batch` |
| `:392`→`:401` | `ProfileSkew` returned without `finish` | same |
| `:382`→`:391` | torn ledger returned as `CreateError::Ledger` without `finish` | same |
| `:535`→`:544` | `Ending::Ledger` mapped to `Unsettled::Store` | same |
| `:362`→`:371` | `doubt = Doubt::default()` deleted on re-mint | `c5_a_byte_identical_record_is_never_adopted_after_a_re_mint` |
| `:564`→`:573` | `Ok(false) => doubt.unsettled = None` → `Ok(false) => {}` | `f9_a_1021_settled_absent_leaves_nothing_in_doubt` |
| `:343`→`:352` | the loop's `doubt.unsettled = None;` deleted | same |
| `:349`→`:358` | the held, identity-taken settling read → `if false` | `f5_a_held_batch_landing_between_the_identity_reads_is_answered_created` |
| `:505`→`:514` | `else if self.held.is_none()` → `else` | **survives**, as expected (see open item 3) |

Clean runs on the final tree: `multipart_create` 35/35, and the DST target `multipart_admission`
7/7 under `--cfg madsim` with 50 seeds.

**Known equivalent mutation:** dropping `Open` or `Aborting` from `widest_reachable_bytes`
changes no answer (349 and 353 bytes, against a widest of at least 641). Not run, per the brief.

## Refute your own test

- **(a) Genuine red?** Yes, but only through mutations. `create_session` does not exist on the
  base, so the test cannot compile there, and C4-verify is UNVERIFIABLE (exit 77), as the brief
  pre-declares. The evidence that binds is the table above: each of the 26 brief edits and each
  of the adversary's 8 mutants reverts one rule of the production code, and a named leg goes red.
  The 8 adversary mutants passed every attempt-2 leg (the adversary's finding) and each now
  fails a new leg.
- **(b) Production path?** Yes. Every new leg calls `wyrd_core::multipart::create::create_session`
  over a real `RedbMetadataStore::in_memory()`. The `OnRead` hook only acts at the
  `MetadataStore` seam: it releases a real held batch through the inner store's `commit`, and it
  writes records with real batches. C5 on a re-minted identity plants its session with a real
  `create_session` and uses a real rival `create_session`. Nothing re-implements the create.
- **(c) Fixture includes the fault?** Yes. The held batch really lands (asserted: `late ==
  Committed`, or `release_held` returning `Committed`), the marker is really deleted, `mpuctl` is
  really rewritten or torn in the store, the planted record is really there, and the read faults
  really fail reads. Each hook asserts it fired (`on_read.is_none()` after the create).

## Open items for sign-off

1. **Size flag:** the patch is over the 100 KB backstop again (accepted at Plan, 2026-10-17). No
   leg was dropped or merged.
2. **Cancel safety in the other two "no session goes unreported" texts.** The new paragraph is in
   `create_session`'s rustdoc only, as the carry-forward asked. The module header (`create.rs:15-19`)
   and the living-architecture paragraph (`05-building-block-view.md:206`) still state the C-1
   promise without "provided the future is not dropped after a commit is sent". I left them, per
   "no other changes". If the human wants the caveat there too, it is one clause in each.
3. **`create.rs:514` (`else if self.held.is_none()`) is unpinned.** The adversary called it
   cosmetic: the `else` mutant only changes which held `CommitUnknownResult` `InDoubt.outcome`
   reports (the latest instead of the first; both say `may_still_commit: true`). Every answer and
   class stays the same. I confirmed it survives. It does contradict the `InDoubt::outcome`
   rustdoc ("the first that said `may_still_commit`"). Pinning it needs the wrapper to give each
   unknown outcome a distinct `detail` and a leg with two held commits. That is outside the
   carry-forward's four items, so I did not add it.
4. Attempt 2's open items on F8 (d)'s reading, row 3's settling read while held, and await
   bounds still stand as written in `iteration-v2/build-notes.md`. Item 2 there ("a create whose
   own batch landed between its `mpu:` read and its `seggrp:` read now answers `Created`") is now
   pinned by the new F5 identity-reads leg.

## Gate

`cargo xtask ci` through the project's runner (`PDCA_WORKTREE=… ./engine/xtask.sh ci`) on the
final tree: **exit 0, "xtask ci: all checks passed"**. Steps it ran: `typos` (clean),
`lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK), so both of Plan's
external dependencies (`typos`, the doc renderer) were present and exercised; the gitlink,
unsafe and blackbox guards; `cargo fmt --all -- --check`; workspace clippy, build and test
(`multipart_create` 35/35, `multipart_state_machine` 22/22); cargo-machete; the three `cargo
deny` checks; the conformance vectors; the ADR-0035 statics gate; the deploy guard; and the DST
tier under `--cfg madsim` with `MADSIM_TEST_NUM=50` (clippy, then tests; `multipart_admission`
7/7, every other DST file green).

Commit-readiness: the target repo configures no git hooks (no `core.hooksPath`, no pre-commit
config). Its formatter and spell-checker run inside the gate above and are clean.

`patch.diff` (213 KB) applies to `d2bd810`, and base + patch gives exactly the worktree's tree
(`git write-tree` of both is `5296ef6c`). The bundle carries the core test at the root
(`multipart_create.rs`, as attempts 1 and 2 did) and at `crates/core/tests/multipart_create.rs`,
and the DST scenario at `crates/dst/tests/multipart_admission.rs`.

## Reading scope

Beyond `brief.md`, I read what the carry-forward points at in `iteration-v2/`: `patch.diff`
(the starting point), `session-carry-forward`, `check-advisory-adversary.md` (the four mutants
and their probe cases), `check-advisory-code-review.md`, and `build-notes.md` (to carry its
measurements forward rather than re-derive them). I also read the bundle's `review-batch.md`
(no open findings). I did not read `iteration-v1/` beyond what attempt 2's patch already holds,
or any SUMMARY.md.

## Scratch

The mutation scripts (`mutate.py`, `mutate_dst.py`), pristine copies, per-row logs and the gate
log are in `$PDCA_SCRATCH/pdca-builder-894-mutants-v3`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_894/pdca-builder-894-mutants-v3`), inside the
harness's per-issue scratch root. Left for the harness to reclaim.
