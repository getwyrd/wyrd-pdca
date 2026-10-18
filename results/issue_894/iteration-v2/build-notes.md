# Build notes — issue 894 / multipart-admission-create (attempt 2)

Target: `getwyrd/wyrd` `main`. The worktree base is `d2bd810` (`pdca-integrate: issue_671`),
whose tree is **identical** to `71e9b9f` (`git diff 71e9b9f d2bd810` is empty), so every
base-line citation in the brief holds. Production lines below cite the patched tree; base lines
are marked "base".

Starting point: attempt 1's `iteration-v1/patch.diff`, applied cleanly (`git apply --check`), then
changed as Design § *Attempt 1: keep and change* lists. Structure, names, the `Barrier`,
`Scripted` and `Ceiling` wrappers and the DST `Creator` are kept.

## What changed against attempt 1, and where

### Production — `crates/core/src/multipart/create.rs` (new file)

1. **Rows 2 and 3 compare R.** In the loop, after an unknown outcome, only `mpu:<id>` **equal
   to R** answers `Created` (`create.rs:338-344`); a record that is not R, or a marker with no
   session, is row 3: while held it gets one settling read and then ends
   `InDoubt { may_still_commit: true, cause: Unsettled::IdentityTaken }`, never a re-mint
   (`create.rs:345-353`); not held, it is a collision (`create.rs:354-365`). The settling read in
   `finish` compares R too (`read_own`, `create.rs:545-548`; `finish`, `create.rs:550-576`).
   `Unsettled::NonceTaken` is renamed `IdentityTaken`, because it now also covers an `mpu:<id>`
   that is not R (`create.rs:138-152`).
2. **F9, the source chain.** `InDoubt` now carries `outcome: CommitUnknownResult` (the first
   `may_still_commit: true` outcome while held, else the latest out-of-flight one), and
   `CreateError::source()` returns it for `InDoubt` (`create.rs:192-204`). `classify` stops at
   the first seam type, so every `InDoubt` reads `Indeterminate`, whatever the stop cause. The
   stop cause stays reachable as `InDoubt::cause` and in `Display`. `CreateError` rustdoc gains
   an "Error class" section (`create.rs:85-94`).
3. **The doubt is two `Option<CommitUnknownResult>`s**, held and unsettled, exactly the brief's
   terms (`Doubt`, `create.rs:485-515`). `observe` keeps the first held outcome and never lets a
   later one replace it.
4. **F8 (d)** — see "Open item 1" below: under the table's settling rule the existing control
   flow already ends `InDoubt { false }` when the settling read fails, so no code change was
   needed beyond the new `Doubt`; mutation 14 pins it.
5. `finish` takes a private `Ending` (`Refused | Ledger | Store`, `create.rs:517-543`), so the
   "identity taken" ending has no unreachable "not in doubt" arm.
6. Rustdoc: the "Two budgets" section now states the narrowed contention guarantee and the bound
   (46 at the shipped profile, linear in a larger one, at most `max_sessions ×` 50 ms = 2.3 s);
   "Adopting a session" is rewritten as "Which session is this create's" (rows 1 and 2 together,
   and the stated same-id-same-nonce-same-request bound); the unknown-outcome section states
   held / unsettled and the settling rule (`create.rs:206-304`).

### Production — `crates/core/src/multipart.rs`

- `Budget::admission_cas_attempts` doc (base `:1688` insertion, now `:1697-1713`): narrowed; no
  "never ordinary concurrency on a ledger with room".
- `Backpressure::AdmissionContention` doc (base `:4292`, now `:4441-4444`): narrowed.
- `Refusal::SessionTooLarge` doc (now `:4482-4493`): names both payload-carrying states.
- `MAX_ADMISSION_CAS_ATTEMPTS` doc (base `:5011-5023`, now `:5180-5204`): "only that derivation
  at the shipped numbers", and the contention refusal is "bounded, not ruled out", with the TiKV
  case.
- `ADMISSION_BACKOFF_CAP_MILLIS` doc (base `:5029-5031`, now `:5210-5213`): kept from attempt 1,
  which already ties the bound to the profile in effect.
- `KnobSet::max_upload_id_attempts` / `max_admission_cas_attempts` (base `:5098-5101`, now
  `:5280-5287`): one sentence each — Create does not read them yet; #508 reconciles.
- Module header (base `:5-7`, `:93-108`): kept from attempt 1, including the re-pin of
  `05-building-block-view.md:202-204` to `:206` (now `:110`).
- Kept from attempt 1 unchanged: `pub mod create;` (`:146`), `AdmissionRecord::bootstrap`
  (`:1898`) and `admit_one` (`:1911`), `SessionRecord::open` (`:2355`) and
  `widest_reachable_bytes` (`:2388`), the `SessionTooLarge` variant. The creation routes meet the
  "one route, decoder identity" rule; no decoder was loosened (Open question 1: not triggered).

### Docs — `docs/design/architecture/05-building-block-view.md:206`

Attempt 1's paragraph kept; its "never ordinary concurrency on a ledger with room" sentence
replaced by the narrowed one (the ledger moving, an outcome the store could not report, or a
TiKV lock with nothing moved; on TiKV lock contention alone can refuse on a ledger with room).
The unknown-outcome sentence now says a found session is the create's own only when it is
byte for byte its batch's record, and that the error names the id and nonce and classifies as
indeterminate. One line (the whole 4,929+-character paragraph is the hunk, as the brief warned).

### Tests — `crates/core/tests/multipart_create.rs` (new, 31 legs)

Kept: A, A′ ×2, B full/skew/no-bucket/marker-moved, C1 ×2, C2 (extended), C5, F1, F2, F4, F5,
F6, F7 (extended). New or rebuilt:

| Leg | Test fn |
|---|---|
| B torn ledger (row 9) | `b_a_torn_ledger_ends_in_a_ledger_error_and_writes_nothing` |
| B contention with nothing moved (rows 7, 10) | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` |
| B size, Completing widest | `b_the_size_headroom_is_measured_on_encoded_bytes_completing_widest` |
| B size, Completed widest (new fixture) | `b_the_size_headroom_is_measured_on_encoded_bytes_completed_widest` |
| C2 crossings (loss→collision, collision→loss) | `c2_the_contention_and_collision_budgets_never_share` |
| C3 | `c3_an_id_taken_under_the_commit_is_re_minted_and_left_as_it_is` |
| C4 | `c4_a_nonce_taken_under_the_commit_is_re_minted` |
| C6 (i) | `c6_i_another_session_under_the_upload_id_is_never_adopted` |
| C6 (ii) | `c6_ii_while_held_another_session_under_the_upload_id_ends_in_doubt` |
| F3 (write-back) | `f3_a_late_batch_is_excluded_by_the_identity_keys_not_the_ledger` |
| F7 (two batches, batch 2 pre + put) | `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` |
| F8 (a)–(d) | `f8_a_…`, `f8_b_…`, `f8_c_…`, `f8_d_…` |
| F9 | the `in_doubt` helper (every InDoubt leg: C6 (ii), F6, F8 a–d) + `f9_a_store_fault_with_nothing_in_doubt_keeps_its_class` |

Wrapper additions: `OnCommit::{PlantSessionFirst (now an interloper record: another nonce,
another object), PlantGroupFirst, RivalFirst, ConflictUnmoved, Fail}`, a scripted `GetFault`
(armed after N commits, optionally only `mpu:` reads, skip/fail counts, transient or plain),
`Seen` now records the session put (R) and the `mpuctl` put, a `then` action for every commit past
the plan, and a `RUNAWAY_COMMITS` (1,000) guard. `PassThenReleaseHeld` is gone (F3 no longer
needs it). All wrapper state is in struct fields; no `static` (ADR-0035).

`crates/core/tests/multipart_state_machine.rs:769`: the `Refusal::SessionTooLarge` arm (kept
from attempt 1).

### DST — `crates/dst/tests/multipart_admission.rs` (new) and `support/mod.rs`

- Fourth leg `Leg::Witness`: one creator, `profile_of(3)` (contention budget 3), 1031 armed for
  8 commits, and in-flight batches landing only at `quiesce`. New witness test
  `the_witness_leg_holds_and_a_held_in_doubt_batch_lands_by_quiesce` (`:543-570`): fails unless
  some create ended `InDoubt { may_still_commit: true }` **and** its batch landed. Measured on
  seeds 0..64: 11 creates ended held in doubt; 9 of those had their batch land.
- Invariant (3) now checks that the session an `InDoubt` named is **exactly** that create's
  record under the named nonce (`is_record_of`, `:332-346`), and (4) checks every `Created` the
  same way (requested bucket/object, `Open`, epoch 0, no fence spent, canonical bytes).
- The mixed leg's doc no longer claims "the kept doubt meeting an out-of-flight outcome"; it
  claims only what it counts (a writer met a 1031, then a 1021). The kept-doubt claim moved to
  the witness leg.
- `support/mod.rs`: `defer_in_flight: bool` (`:573-575`), the early return in
  `settle_in_flight` (`:615`), the default `false` (`:677`), and
  `SimFdbMetadataStore::defer_in_flight_landings` (`:719-728`). Off by default and it draws no
  randomness, so every other scenario replays bit for bit. Attempt 1's mixed nemesis is kept.

## Why the witness leg needs `defer_in_flight_landings` (a measured decision)

The brief's fourth leg assumes an in-flight 1031 batch waits for `quiesce`. The sim lands every
in-flight batch on a seed coin at **every** later store call (`settle_in_flight(false)` in
`get`, `scan`, `scan_page` and `commit`, `support/mod.rs:614-634`, called at `:782`, `:875`, `:882`, `:902`). A lone creator makes five store calls between its
last commit and its return, so its batch almost always lands before its settling read, and the
create answers `Created`. Measured with the unmodified sim, 1,024 seeds each:

| budget (`WITNESS_ROOM`) | strikes | ended held in doubt | of those, batch landed |
|---|---|---|---|
| 1 | 4 | 341 | **9** |
| 2 | 6 | 119 | **1** |
| 3 | 8 | 45 | **0** (witness fails) |

So without a switch, only a budget of **one** unit reaches the witness reliably, and that needs
1,024 seeds. The rejected alternative — budget 1, `WITNESS_SEEDS = 1024`, zero support changes —
was implemented and run (9 witnesses). I chose the switch (+18/−2 lines in `support/mod.rs` over attempt 1’s +20/−1) because
it keeps the brief's "a few units" (budget 3), reaches the witness on 9 of the shared 64 seeds,
and selects a schedule FoundationDB allows (a timed-out commit may land "at a later point in
time", still through the resolver) rather than changing what a landing does. The racing legs
keep opportunistic landings.

## Size headroom: the measured crossover

With the real encoder (the test's `widest_states`, every spelling round-tripped through the
production decoder and encoder), parent inode 0, no content type, the shipped widths:

- object name of **151** bytes (JSON 153 with quotes): `Completing` 641, `Completed` **642** —
  `Completed` strictly widest;
- **152** bytes: both 643 — a tie;
- **153** bytes and up: `Completing` widest (it gains two bytes per name byte, `Completed` one).

`Open` (349) and `Aborting` (353) are never the widest. The Completed-widest fixture uses
`photos/k` (8 bytes); the Completing-widest one uses `photos/` + 8,000 × U+0001 (48,007 encoded
bytes). Each fixture asserts its state is **strictly** the widest at every point
(`widest_len`).

## Demonstrated reds (the convergence contract)

Each row: the temporary edit was applied to the patched production code, the named test(s) run
through `cargo test -p wyrd-core --test multipart_create` (or the madsim DST target the way
`xtask::run_dst` runs it: `RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=50`), then the file restored
from a pristine copy (`cmp` confirmed after the last row). Every edit failed its named leg.

| # | Edit | Failed (names) | Failure, where checked |
|---|---|---|---|
| 1 | `contention_budget = MAX_UPLOAD_ID_ATTEMPTS` | `a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted`, `a_prime_the_contention_budget_follows_the_profile_in_effect` | both A′ runs |
| 2 | `contention_budget = MAX_ADMISSION_CAS_ATTEMPTS` | `a_prime_the_contention_budget_follows_the_profile_in_effect` (first A′ run passes) | |
| 3 | `losses` merged into `collisions` | `c2_the_contention_and_collision_budgets_never_share` | `IdentityCollisions { attempts: 2 }` on the loss→collision crossing |
| 4 | drop `require_absent(mpu:<id>)` | `c3_an_id_taken_under_the_commit_is_re_minted_and_left_as_it_is` | |
| 5 | drop `require_absent(seggrp:<nonce>)` | `c4_a_nonce_taken_under_the_commit_is_re_minted` | |
| 6 | drop both identity preconditions | `f3_a_late_batch_is_excluded_by_the_identity_keys_not_the_ledger` | |
| 7 | drop the bucket `require` | `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` | |
| 8 | re-mint after an unknown outcome | `f1_an_applied_unknown_outcome_is_adopted_by_the_re_read`, `f2_an_unapplied_unknown_outcome_is_re_driven_under_the_same_identity` | (first try left `observe` unused and failed to compile under the crate's dead-code deny; redone keeping the call) |
| 9 | replay the original batch once after an unknown outcome | `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` | three commits |
| 10 | row 2 as `view.session.is_some()` (attempt 1's code) | `c6_i_another_session_under_the_upload_id_is_never_adopted`, `c6_ii_while_held_another_session_under_the_upload_id_ends_in_doubt` | |
| 11 | collision check only when `doubt.unknown` (no absence read) | `c5_a_byte_identical_record_is_never_adopted` | |
| 12 | `observe` replaces held/unsettled with the latest outcome | `f6_a_batch_that_may_still_land_keeps_the_create_in_doubt` | |
| 13a | `Doubt::default()` on a read fault | `f8_a_a_held_create_stays_in_doubt_past_a_read_fault` (also `f8_d_…`) | |
| 13b | `Doubt::default()` on a plain commit fault | `f8_b_a_held_create_stays_in_doubt_past_a_commit_fault` | |
| 13c | `Doubt::default()` on a failed settling read | `f8_c_a_held_create_stays_in_doubt_past_a_failed_settling_read` (also `f8_d_…`) | |
| 14 | failed settling read clears `unsettled` (ends `Store`) | `f8_d_an_unsettled_1021_with_a_failed_settling_read_ends_in_doubt` | |
| 15 | on an unknown outcome at the last budgeted unit, return without reading | `f4_an_unknown_outcome_on_the_last_budgeted_attempt_is_settled_before_returning` | |
| 16 | `SessionCap` returned directly, no `finish` | `f5_a_capacity_refusal_met_in_recovery_is_checked_against_the_earlier_batch` | |
| 17 | `InDoubt` `source()` = the stop cause (attempt 1's code) | `f8_a_…` (`Transient` ≠ `Indeterminate`); full run: also `c6_ii_…`, `f6_…`, `f8_b_…`, `f8_c_…`, `f8_d_…` | |
| 18a | `>` → `>=` in the size check | both size legs | `SessionTooLarge { bytes: 100000 }` at the exactly-at-ceiling point |
| 18b | drop `Completing` from `widest_reachable_bytes` | `…_completing_widest` only | one byte over admitted |
| 18c | drop `Completed` from `widest_reachable_bytes` | `…_completed_widest` only | one byte over admitted |
| 19 | (G) `Created` straight after a 1021 | `the_1021_leg_holds_and_meets_a_1021_that_did_not_apply`, `the_mixed_leg_…`, `multipart_admission_invariants_hold_under_the_dst_seed_sweep` | "a Created that is not real" (reverse assertion) |
| 20 | (G) `InDoubt` names a different upload id (the real one reversed) | `the_witness_leg_holds_and_a_held_in_doubt_batch_lands_by_quiesce` | "session … is stranded" |
| 21 | a `Conflict` whose re-read finds the same ledger bytes → collision | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | `IdentityCollisions { attempts: 2 }` |
| 22 | the same, retried without spending a unit | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | "the create never stopped committing (1001 commits)" |

On row 22: the brief expected "the leg's timeout fires". It is the wrapper's `RUNAWAY_COMMITS`
guard that fires, in 0.03 s, before the 120 s leg timeout. A runaway loop through redb records a
`Seen` per commit, so waiting out the timeout would grow memory into the hundreds of MB; the
guard is the deterministic form of the same red. (`ConflictUnmoved` also yields, so the timeout
could fire if the guard were removed.)

**Known equivalent mutation:** dropping `Open` or `Aborting` from `widest_reachable_bytes`
changes no answer (349 / 353 bytes against a widest of at least 641). Not run, per the brief.

## Refute your own test

- **(a) Genuine red?** The test cannot compile on the reverted base (`create_session` does not
  exist there), so C4-verify is UNVERIFIABLE (exit 77), as pre-declared. The binding evidence is
  the 26 edits above: each reverts one rule of the production code and its named leg goes red,
  including the two edits that *are* attempt 1's code (rows 10 and 17).
- **(b) Production path?** Yes. Every leg calls `wyrd_core::multipart::create::create_session`
  over a real `RedbMetadataStore::in_memory()` (core) or `SimFdbMetadataStore` (DST). The
  wrappers only act at the `MetadataStore` seam: they delay, answer `Conflict`/unknown/fault, or
  write records through the inner store. Nothing re-implements the create.
- **(c) Fixture includes the fault?** Yes. C3/C4 write the interloper's record for real; C6 runs
  a real second `create_session` under the shared upload id; F1–F8 return real
  `CommitUnknownResult`s with the batch applied, unapplied or held, and the held batch is really
  released and lands (or meets `Conflict`); F8 fails real reads and commits. The DST legs arm the
  nemesis and assert it struck (`ambiguous_commits >= 1` every seed), and the witnesses assert the
  unapplied 1021, the 1031→1021 writer and the landed held-in-doubt batch were each met. The
  witness leg's `defer_in_flight_landings` picks *when* an in-flight batch lands; it does not
  remove the in-flight batch.

## Open items for sign-off

1. **F8 (d) and the settling rule.** The table says a plain read error ends "after the settling
   rule", which reads `mpu:<id>` once more. So when only the first re-read after a 1021 fails and
   the settling read succeeds, the answer comes from that read: `Created` if it finds R, else the
   ending stands (`CreateError::Store`) — both correct, but not "the same answer applied and
   unapplied". (d)'s "the settling read fails ... because the batch may have landed" therefore
   only holds when the settling read itself fails, so the fixture fails **every** read after the
   1021. Under that fixture attempt 1's control flow also answered `InDoubt { false }`; the
   brief's note that attempt 1 "ends Store" matches the single-failed-read case, where its
   settling read succeeded and found nothing (the table's answer). Mutation 14 is the red that
   pins (d). If the human wants "one failed re-read ends `InDoubt` with no second read", that is
   a table change (row 9 / the settling rule), not a code bug here.
2. **Row 3 while held reads once more before `InDoubt`.** That is the settling rule applied
   literally. A side effect: a create whose own batch
   landed between its `mpu:` read and its `seggrp:` read now answers `Created` instead of
   `InDoubt`. That is the more accurate answer.
3. **Await bounds.** `create_session` awaits only store calls and the caller's wait. Core is
   runtime-free (ADR-0009) and cannot impose timeouts; the rustdoc's Termination section leans on
   the store contract's operational envelope, as attempt 1 did, and the caller (#668) owns
   caller-side deadlines.
4. **The `KnobSet` lines** are one sentence each, wrapped over two comment lines.
5. **Size flag**: the patch is over the 100 KB backstop (accepted at Plan, 2026-10-17). No leg
   was dropped or merged.

## Gate

`cargo xtask ci`, run through the project's runner (`./engine/xtask.sh ci`, which execs it in
`$PDCA_WORKTREE`): **exit 0, "xtask ci: all checks passed"**. Steps it ran: `typos` (clean),
`lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK — so both of Plan's
external dependencies, `typos` and the doc renderer, were present and exercised), the
gitlink / unsafe / blackbox guards, `cargo fmt --check`, workspace clippy, build and test
(`multipart_create` 31/31, `multipart_state_machine` green), cargo-machete, the three
`cargo deny` checks, the ADR-0035 statics gate, the deploy guard, and the DST tier under
`--cfg madsim` with `MADSIM_TEST_NUM=50` (clippy, then tests: `multipart_admission` 7/7, every
other DST file green).

After that run I re-pinned two proposal citations in doc comments only (below), then re-ran
`typos`, `cargo clippy -p wyrd-core --all-targets` and `cargo test -p wyrd-core --test
multipart_create` (31/31) on the final tree. `cargo fmt --all -- --check` is clean. The final
`patch.diff` applies to `d2bd810` and yields exactly the worktree tree (`git write-tree` of base +
patch equals the worktree's).

Rustdoc is not a gate here (the base already fails `-D rustdoc::broken_intra_doc_links` in
`erasure.rs` and `metadata.rs`). `cargo doc -p wyrd-core --no-deps` reports one warning in the
multipart files, at `multipart.rs:4948`, which is base code this patch does not touch.

### Citations re-pinned in the patch

- `AdmissionRecord::bootstrap` doc (`multipart.rs:1888`): attempt 1's `0016:2009-2015` →
  `0016:2003-2010` (the self-bootstrapping `mpuctl` text).
- `MAX_ADMISSION_CAS_ATTEMPTS` doc (`multipart.rs:5181`): the base's `0016:1999-2034` →
  `0016:2001-2036` (the admission-control paragraph begins at `:2001`; `:1999` is the end of the
  clock-guard paragraph). The other 0016 citations the patch carries were spot-checked at their
  lines (`:657`, `:1957-1990`, `:2010-2028`, `:2018-2025`) and hold.

### Reading-scope slip

While checking where attempt 1 had put its test copies in the bundle, one `grep` read a few
lines of `iteration-v1/build-notes.md` (lines 61-62, 124, 163-164, 178, 217, 245: test names,
counts and commands). The brief scopes `iteration-v1/` to `patch.diff` only. Nothing from those
lines changed a decision; I used them only to place the test copies the same way (core test at
the bundle root, DST test under `crates/dst/tests/`).

## Scratch

Throwaway files (pristine copies of the two production files, the two mutation runner scripts
and the gate log) live in `$PDCA_SCRATCH/pdca-builder-894-mutants`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_894/pdca-builder-894-mutants`), inside the harness's
per-issue scratch root; left for the harness to reclaim.
