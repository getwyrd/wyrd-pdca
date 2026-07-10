# Build notes — issue 468 / metadata-fdb-dst-story (iteration 4)

## What this iteration changes, and why it is narrow

Iteration 3's sign-off **rejected on exactly two coverage gaps, both advisory**, and
explicitly said: "The design proposal, the simulated-FDB model, and the linkage guard all
stand… Do not churn any of that." So legs 1–3 (`ambiguous_cas_settles_over`,
`ambiguous_pending_put_over`, `timed_out_commit_over`), the shared-contract leg
(`conformance.rs`) and the whole `no_fdb_linkage.rs` graph guard are carried **byte-identical**
from iteration 3. The delta is confined to:

- `crates/dst/tests/support/mod.rs` — one new `FdbFidelity` variant + one line in
  `settle_in_flight` (the violating deferral twin), additive, no existing path changed.
- `crates/dst/tests/commit_ambiguity.rs` — a new **leg 4** (multi-writer `1031` under
  contention) with its property body, sweep, counter assertions, and two demonstrated reds.

The two carry-forward items (iteration-3 sign-off rationale, verbatim):

> 1. Criterion 2(ii) "exactly one writer won" is demonstrated ONLY for 1021… Add a
>    multi-writer 1031 leg with a demonstrated red: four writers race the version CAS with the
>    1031 nemesis armed, writer A's timed-out batch left in flight, landing deferred AFTER
>    writer B has already won and bumped the version; assert the model rejects A's
>    stale-precondition batch at deferral so "exactly one winner" survives a 1031 deferral
>    under contention. Do not instead narrow the criterion.
> 2. …the 1031/torn observation counters (`deferred_landings`, `deferred_rejections`,
>    `resolver_conflicts`, `torn_applies`) are incremented but asserted by no test… Assert the
>    relevant counters in the new leg so a sweep that never armed the deferral branch is
>    visibly vacuous.

## Root cause, in two sentences

The `settle_in_flight` **resolver-rejection branch** (`support/mod.rs:539-557`,
`deferred_rejections += 1`) — the code that keeps "exactly one writer wins" alive across a
`1031` deferred landing — was never reached by a test: the only multi-writer race
(`ambiguous_cas_settles_over`) armed `1021`, which leaves nothing in flight, and the only
`1031` leg (`timed_out_commit_over`) drove a single writer, so the deferral and the
contention never combined. Consequently the deferral counters were incremented but read by no
test, so the brief's anti-vacuity argument ("`Observations` records how often it fired") did
not actually cover them.

## The fix

### Leg 4a — the contended race + counter anti-vacuity (items 1 and 2)

`contended_cas_under_1031_over` (`commit_ambiguity.rs:746`) is leg 1's four-writer race with
the **`1031`** nemesis armed (budget 4). Struck batches are seed-selected into landed-now /
in-flight / dropped; the ones left in flight resolve *later*, at the forced deferral
(`meta.quiesce()`, `:825`), after another writer may already have won. The invariant asserted
over the terminal state: the settled winner count equals the version bump, at most one bump,
the chunk map is whole, and the batch applied whole (`:857`-region assertions, reusing leg 1's
`assert_chunk_map_is_whole` / `assert_batch_applied_whole`).

`contention_under_1031_keeps_exactly_one_winner_through_the_deferral`
(`commit_ambiguity.rs:877`) sweeps 0..64 and asserts the counters aggregate non-zero:
`deferred_landings >= 1`, `deferred_rejections >= 1`, `resolver_conflicts >= 1`. Empirically
across 0..64 all three fire (the sweep passes). This is item 2: a sweep that never armed the
deferral-rejection branch now fails loudly here instead of passing hollow.

The demonstrated red for the *counting* half (item 1's "exactly one winner"),
`counting_every_timed_out_commit_a_winner_fails_the_contended_sweep`
(`commit_ambiguity.rs:916`, `#[should_panic(expected = "must equal the inode's version
bump")]`), is the multi-writer analogue of leg 1's `AssumeNotCommitted`: an observer that
counts every `1031` `Err` a winner over-counts, because the deferral rejected all but one, and
the version-bump assertion trips.

### Leg 4b — the resolver re-check, demonstrated load-bearing on its own terms (item 1)

The multi-writer-same-prior race in 4a *cannot* by itself show the resolver re-check is
load-bearing: every writer supersedes the same prior inode and writes version 2 with a **whole**
inode record, so even a stale deferred landing yields a whole inode at version 2 — the
version-bump/whole-map assertions pass regardless. (This is the same single-inode-key property
iteration 1 flagged: the model cannot render a *torn* inode.) To make the deferral rejection
observably load-bearing I isolate it:

`deferred_1031_settles_against_current_truth` (`commit_ambiguity.rs:930`): writer A's CAS is
struck by `1031` and, on the seeds it acts on, left in flight; writer **B** then commits
*determinately* and wins cleanly (distinct chunk-id range, `:947`), bumping the inode to B's
whole map; A's now-stale batch lands only at the forced deferral. The faithful model re-runs A
through the resolver, its precondition on the prior inode no longer holds, and **A is
rejected** — so `settled.chunk_map == B` (`:977`). The seed only "counts" (returns `true`) when
it reached that case; `a_stale_deferred_1031_batch_is_rejected_against_current_truth`
(`:1007`) asserts the sweep reaches it at least once, so the guarantee is executed, not
assumed.

The demonstrated red, `a_deferred_1031_batch_that_skips_the_resolver_clobbers_the_winner`
(`:1027`, `#[should_panic(expected = "clobbered the winner")]`), swaps in the new violating
fidelity `FdbFidelity::DeferredResolverSkipped` (`support/mod.rs:433`), which omits the
precondition re-check on the **forced** deferral only (`support/mod.rs:546,552`). A's stale
batch then clobbers B's win, `settled.chunk_map == A ≠ B`, and the assertion trips. Because the
RNG draws are identical up to `quiesce` regardless of fidelity, the violating sweep reaches the
exact same seeds the faithful one does, so the red is not resting on non-existence.

## Why a violating fidelity here, not another observer

Legs 1–3 use observers for their reds because the store's *behaviour* is correct and only the
caller's *interpretation* is at issue. Leg 4b is different: the property under test **is** a
store behaviour (the deferred resolver re-check), so the only faithful red is a store that
omits it. Adding the `DeferredResolverSkipped` variant is the minimal way to do that: +14 doc
lines + 1 enum discriminant + 2 lines in `settle_in_flight` (a `let` and an `||`), touching no
existing fidelity's path — versus the alternative of contorting the observer to *simulate* a
broken store, which would be a re-implementation of the store logic in the test (the exact
"stand-in that passes vacuously" the discipline forbids). Concretely the model delta is:
`git diff --stat` shows `support/mod.rs | 516 +` of which the iteration-4 part is ~19 lines.

## Refuting the test (forced self-check)

- **(a) Genuine red?** Yes, by mutation, not assertion. I reverted the fix in place — set
  `let skip_resolver = false;` in `settle_in_flight` (making `DeferredResolverSkipped` behave
  faithfully) — and re-ran: `a_deferred_1031_batch_that_skips_the_resolver_clobbers_the_winner`
  went **FAILED** ("test did not panic as expected", `commit_ambiguity.rs:1025`), because the
  stale batch is then rejected and never clobbers B. Restored (via re-apply, since the mutation
  was a manual edit) and re-ran green. The counting red
  (`counting_every_timed_out_commit_a_winner_fails_the_contended_sweep`) pins the *specific*
  load-bearing message "must equal the inode's version bump", not any panic. Bundle-level, the
  whole file is absent on the base (`git -C ../wyrd grep -in fdb origin/main -- crates/dst/` →
  no hits), so pre-fix it is a compile/absence red.
- **(b) Production path?** Yes with the standing caveat this whole bundle discloses: there is
  no shippable FDB code inside DST (linking `libfdb_c` is the very thing forbidden, ADR-0035).
  Leg 4 drives the **shipped** `SimFdbMetadataStore` through the real `MetadataStore` trait over
  the real four-phase `core::write` protocol (`write::intent` / `write_fragments` /
  `commit_overwrite`) — the same production code paths legs 1–3 drive — and asserts against the
  store's own `truth`/orphan-ledger, not a re-implementation. The *equivalence anchor* to the
  real `metadata-fdb` driver is the shared `run_all` contract leg
  (`conformance.rs:59`), unchanged. The model reproduces production
  `classify_commit_error`'s 1021/1031 split and the "re-check the read-conflict set even for a
  commit that lands after a timeout" rule (`crates/metadata-fdb/src/lib.rs:161-166`).
- **(c) Fixture includes the fault?** Yes. Leg 4a arms the actual `1031` nemesis over the
  actual four-writer fleet and asserts the deferral counters *fired* (`deferred_rejections >= 1`
  etc.), so the fault is present, not curated out. Leg 4b's red plants the actual violating
  store and asserts the sweep *reached* the deferred-past-a-winner case
  (`a_stale_deferred_..._rejected`, the faithful `reached >= 1`), so the clobber red is not
  vacuous.

## Verification — red→green through the project runner

- **`./engine/xtask.sh dst` (the gating `C4-ci` behavioural evidence) — exit 0**, full 50-seed
  sweep, on `$PDCA_WORKTREE`. `commit_ambiguity` 13/13 (incl. all six `should_panic`
  demonstrated reds); `conformance` 3/3 incl. `sim_fdb_backend_passes_shared_contract`;
  `no_fdb_linkage` 9/9; every other DST binary green.
- `cargo test -p wyrd-dst --test no_fdb_linkage` **bare** (the non-madsim `C4-verify` path) —
  9/9, so the structural discriminator survives the `support/mod.rs` edit.
- `cargo fmt -p wyrd-dst -- --check` clean; `cargo clippy -p wyrd-dst --tests` clean under both
  bare and `--cfg madsim`.
- `patch.diff` (`git diff HEAD`) applies cleanly on a pristine `HEAD` tree (verified in a temp
  `git archive` checkout).

## Disclosures for the human at sign-off (carry into Open Question 1 ratification)

The fidelity ratification should cover what was *actually* built. The three narrowings from
prior iterations still stand, plus one honest limit this iteration surfaces:

1. **Storage modelled as a plain `BTreeMap`, not versioned MVCC** (brief Open Question 1). The
   brief's position is this buys nothing for the trait's precondition-based contract; the call
   is the human's.
2. **Blind-batch narrowing** [resolved iteration 2]: the nemesis does not gate on
   `&& conditional`, matching production `classify_commit_error`.
3. **1031 vs 1021** [resolved iteration 2]: `SimCommitUnknownResult` carries `code` and models
   1031's "promises nothing" distinctly.
4. **New, honest limit of leg 4.** Because the model stores an inode as a **single whole
   record** (not an MVCC keyspace), the resolver re-check at the deferral is observable only
   through the *distinct-winner* scenario (leg 4b: a stale batch clobbering a **determinate**
   winner's chunk map). In the four-writer-same-prior race (leg 4a) the re-check's effect on
   the *terminal inode* is masked by whole-record overwrite — leg 4a's contribution is the
   invariant (winner count = version bump ≤ 1) and the executed-counter anti-vacuity, not a
   torn/hybrid inode. The GC-level consequence of skipping the re-check (a superseded winner's
   fragments leaking, unledgered) is **not** separately asserted; demonstrating it would need
   fragment-reachability tracking that the chosen `BTreeMap` fidelity does not carry. This is
   the same MVCC-vs-`BTreeMap` call as (1), and belongs in the ratification.
5. The nemesis is a single armed budget per scenario, armed after the fixture — "the ambiguity
   space searched exhaustively" means one armed point, all seeds, both codes, all fates;
   `FdbObservations` (now asserted for the deferral counters too) makes a vacuous sweep visible.

## Carried NEEDS-HUMAN / §10 items (not this bundle's to resolve)

- **§10 ACT candidate (carried):** `C4-verify`'s gates row reads "red without the fix, green
  with it", but `run-verify.sh` runs a **bare** `cargo test` under which `commit_ambiguity.rs`
  (`#![cfg(madsim)]`) compiles to nothing, so only the structural `no_fdb_linkage` string-scan
  goes red there. Only `C4-ci` is real evidence for criteria 1–3. Harness-wide scanning hazard,
  already tracked for Act — not a defect of this patch.
- The sign-off §6 items the prior iterations left open remain for the human: a full `C4-ci`
  rerun on a loopback-permitting host, T4 prior-art, T5 fidelity ratification (now including
  disclosure 4 above), and fitness-to-purpose.

## Scope discipline held

No `Undeterminable` `CommitOutcome` variant. `crates/metadata-conformance/` untouched. **No new
Cargo dependency.** Nothing under `crates/*/src/` changed. `xtask/src/main.rs` untouched.
`concurrency.rs`'s `exactly_one_writer_wins_over` not weakened — the ambiguity scenarios keep
their own property bodies in `commit_ambiguity.rs`. Legs 1–3, `conformance.rs`, and
`no_fdb_linkage.rs` carried unchanged from the signed-off iteration 3.

## External dependencies

None. `cargo xtask dst` is container-free and seed-deterministic; no Docker, no `libfdb_c`, no
live cluster required or used.
