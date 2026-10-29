# Build notes — #933 (657.1b): in-flight renewal of the staging leases — iteration 2

Base: the cycle worktree at `93a6f37` ("pdca-integrate: issue_932", the folded 657.1a on
`pdca-integration/main-r42c5348f203d`); target `getwyrd/wyrd@main`. Every `path:line` below is on
the patched worktree unless it says "base". This iteration starts from iteration 1's patch
(`iteration-v1/patch.diff`, applied unchanged) and fixes the one defect the sign-off sent back.
The rest of iteration 1's design (renewal batch, per-round reads, settling, clamp) is kept as it
was; its notes are summarised at the end so this file stands alone.

## The carry-forward, and what this iteration changes

**The defect** (sign-off, T4 3/3, adversary repro): in `write_fragments` the renewal's failure
came back through `?` (iteration 1's `stage.rs:2077`), and `stage_part` turned a refusal into
`Ok(Refused)` (`:1457`). The pending puts, and any answers already collected, were dropped with no
account. So staging could answer a determinate error (`Terminal`, "no batch … in doubt") or
`NoSuchUpload` while a dropped put could still publish. That broke the module's own rule
(`stage.rs:46-56`: an abandoned write is `Indeterminate`, however the timing fell) and the docs.

**The fix** (`crates/core/src/multipart/stage.rs`):

- `Raced` gets a third outcome, `Raced::Halted(Halt)` (`:1169`): a renewal run beside the work
  failed or was refused. `Stager::renewing` now returns `Raced<O>` instead of
  `Result<Raced<O>, Halt>` (`:1948`); the only `?` it had was the renewal's, and that is now
  `Err(halt) => return Raced::Halted(halt)` (`:2033`). So no caller can let a renewal's stop skip
  its own accounting by accident.
- Body reads (`Stager::run`, `:2073`): `Raced::Halted(halt) => return Err(halt)`. A read writes
  nothing, so the renewal's own error or refusal is the honest answer. Unchanged behaviour.
- Fragment writes (`write_fragments`, `:2190-2235`): `Halted` goes through the same accounting as
  the deadline path. Pending puts are dropped and become abandoned writes (`WriteEffect::Unknown`,
  staging's own words, `abandoned_write`, `:973`). Answers already taken are sorted into refused
  (`NotApplied`) and reports, as before. Then:
  - if a write was abandoned or a report was kept, the stop is `Stop::WriteEffectUnknown` with
    the renewal's cause in a new field `renewal: Option<Box<Stop>>` (`:736`), via
    `Stager::halted_beside_writes` (`:1878`). A renewal error that was `InDoubt` keeps its outcome
    and `may_still_commit`, with the writes' account as its cause; a `Stopped` one becomes
    `Stopped { cause: WriteEffectUnknown { renewal: Some(<its cause>) } }`.
  - only when every write answered `Ok` or was refused as late does the renewal's own error or
    refusal stand (`:2231-2235`).
- **The refusal decision, stated explicitly** (the sign-off asked for it): a session found gone
  from `Open` while a write is unsettled is **not** answered `StageOutcome::Refused`. It becomes
  `WriteEffectUnknown { renewal: Some(Stop::SessionLeftOpen { refusal: NoSuchUpload }) }`, an
  error classed by the writes (`Indeterminate` for an abandoned or `Unknown` write). The new
  variant `Stop::SessionLeftOpen` (`:741`) only appears nested there; its doc says so. I followed
  the sign-off rationale, which names the refusal escape (`:1457`) as part of the defect. A
  refusal reads as settled, and a put may still publish. The cost is one retry for the client:
  the retry's reserve reads the session and answers `NoSuchUpload` with nothing in flight. With
  every write answered, the refusal still comes back as `Refused(NoSuchUpload)` (pinned by a
  control run, below).
- `abandoned_write` takes `renewal_stopped` (`:973-999`): a write abandoned because a renewal
  stopped staging gets a detail that says so, and says a store may still *begin* publishing it
  until the deadline. The deadline path's text is unchanged.
- `Display` for `WriteEffectUnknown` names the renewal when set (`:875`), keeping 657.1a's text
  when it is not; `SessionLeftOpen` has its own arm (`:893`) and no `source` (`:932`).
- `Halt::Failed` now boxes its `StageError` (`:1001-1012`; unboxed at the two public returns,
  `:591`, `:1525`). The new field made `Stop` 8 bytes larger, and clippy's `result_large_err`
  (`-D warnings`) then flagged `Footprint::moved_on -> Result<(), Halt>`. `Halt` is private, so
  boxing there changes no public type. The alternative, boxing the write account in
  `WriteEffectUnknown` (`Vec` → `Box<[_]>` for `abandoned` and `reports`), would change 657.1a's
  public field types and every pattern on them; I rejected it for that.
- Docs: the module doc's write-deadline table row (`:44`) and the paragraph after it (`:58-60`);
  the *Renewal* section's failing-renewal paragraph rewritten (`:122-135`); `StageOutcome::Refused`
  (`:368-370`) and `stage_part` (`:1472-1474`) say where a refusal is not one;
  `docs/design/architecture/05-building-block-view.md:246` gets one sentence after the lapse
  sentence ("A renewal that fails, or finds the session gone from `Open`, stops staging at once;
  …"). `:244` untouched.

**Ruled out: keep waiting for the writes after the renewal fails.** That would let more puts
answer `Ok` and so report fewer unknowns. But staging is already dead, it would hold the request
up to `W_write` longer for no gain, and it still needs the same combined error shape whenever a
put does not answer. So it adds a wait without removing the new error shape. Abandoning at once
is what 657.1a's and iteration 1's docs already promised ("abandoned, as at their deadline"); the
fix makes the error match that promise.

## New legs (appended to `crates/core/tests/multipart_upload_part_stage.rs`, R4 section)

- `r4_a_renewal_stopping_beside_a_pending_write_leaves_its_effect_unknown` (`:5819`): chunk 1's
  put held at arrival, deadline `T0 + TTL + 1,000` still ahead. Four runs, the adversary's three
  plus one with doubt: the renewal **fails** (`Act::Fail`), refuses a **lapsed** lease (clock at
  `T0 + TTL + 1`), is **refused** (a real Abort just before it commits), or is **held, then
  lapsed** (first round held unapplied while the clock passes the lease; the retry refuses). Each
  run asserts: `classify == Indeterminate`; `WriteEffectUnknown` with exactly the held put
  abandoned in staging's own words ("a renewal of the attempt's leases stopped staging"), no
  reports, the renewal's cause in `renewal` (`Store`, `LeaseLapsed{slot, T0+TTL+1, T0+TTL+1}`,
  `SessionLeftOpen{NoSuchUpload}`); staging ended before the put's deadline; the held round's
  `may_still_commit` kept (`InDoubt`, backend `scripted`); the put sent once; and **the put,
  released after staging returned, is stored**, so the doubt was real.
- `r4_a_renewal_stopping_after_the_writes_answered_keeps_their_account` (`:5940`): chunk 1's put
  held inside its publishing step, the renewal held at the wrapper's gate. The put answers while
  the renewal waits — `WriteEffect::Unknown` (the clock moved to its deadline first, so its
  publication straddles it) or `Ok` — and staging takes the answer (`Event::Delivered`). Then the
  renewal fails or is refused. With the `Unknown` answer already collected (the sign-off's fourth
  case): `WriteEffectUnknown` keeping that one report, nothing abandoned, the renewal's cause in
  it, `Indeterminate`. **Control runs** with every write `Ok`: the renewal's own answer stands, a
  `Terminal` `Stop::Store`, or `Ok(Refused(NoSuchUpload))`. These pin that the fix does not
  over-report.
- Helpers: `stopped_by_renewal` (`:1722`), `RenewalStop` (`:5769`), `assert_stopped_by` (`:5783`).
  My own R3 legs now assert `renewal` is `None` on the deadline path
  (`r3_the_write_deadline_cuts_off_a_renewal_still_committing`, `:5137`;
  `r3_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt`, `:5456`).
- File header (`:31-46`) mentions the new R4 coverage.

## 657.1a code in the test file that this slice changed (the brief asks for every one)

Legs — each asserted, in effect, that no renewal happens:

- `a_prime_a_write_pending_past_its_deadline_is_abandoned` (`:1787`, assertion `:1840`): waits
  `[IDLE, W]` → `[HALF, W]`. The first body read's wait now runs to the renewal instant (half a
  TTL), which comes before its idle bound. Comment added saying so.
- `a_prime_a_put_refused_as_late_is_reported_as_nothing_landed` (`:1914`, assertion `:1945`): the
  same `[IDLE, W]` → `[HALF, W]` change, comment added.
- `a_prime_an_ok_ready_as_the_deadline_timer_fires_is_counted_written` (`:2156`, assertion
  `:2189-2190`): the same change on `waits()[..2]`, comment added.
- `a_double_prime_the_idle_bound_is_per_read_not_per_body` (`:2337`): `req.lease_ttl_millis = 4 *
  TTL` added (`:2354`), so no renewal falls due inside the leg. Its exact `armed(n)` syncs and its
  waits `[BOUND, BOUND, 1, BOUND, 1]` assume no renewal; with the default TTL a renewal at
  `T0 + 9,998` adds two waits and the leg timed out at `armed(5)`. The idle bound beside renewals
  is R3's `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases`.

Shared fixture and helper (not legs):

- `request()` (`:238`): `max_part_chunks: 1_000` → `MAX_PART_CHUNKS` (158). The new staging check
  refuses 1,000 (`3 + 2 × 1,000 > 500`). No 657.1a leg stages more than five chunks; G sets its own
  bound of 3.
- `effect_unknown` (`:1707`): pattern gets `..` (`:1714`) for the new `renewal` field. Its return
  value is unchanged, so no 657.1a leg that calls it changed.

Every other 657.1a leg passes unchanged. Harness additions (not legs), from iteration 1:
`Kind::Renewal`, `Event::{Gated, ReadGated, Delivered}`, `Log::{landed, gated, reads_gated, acked,
delivered}`, `Act::Gated` with `HeldIfDropped`, `Then::Gate`, `Scripted::held_count`, the fake's
`Delivered` push, `impl StageClock for Runtime`, and the import lines.

## `multipart_knobs.rs` rows changed (Leg 2), unchanged from iteration 1

- `RENEWAL_FIXED_OPS = 3`, counted from the batch shape; the unused `part_ops` binding is gone.
- On-bound part-commit row `with(D, |k| k.batch_ops = part_ops) => Ok(())` (base `:200`) →
  `with(tiny, |k| k.batch_ops = tiny_part_ops) => Ok(())`, `tiny = derived(TINY)`, where the part
  commit (9 ops) binds over the renewal (5).
- Base `:242` `with(D, |k| k.batch_ops = part_ops - 1) => Err(PartCommitOverOps)` → the `tiny`
  set, `tiny_part_ops - 1`.
- Base `:243` `with(D, |k| k.batch_ops = MAX_PART_CHUNKS - 1) => Err(PartCommitOverOps)` →
  `with(tiny, |k| k.batch_ops = TINY.max_part_chunks - 1)`.
- New: `with(D, |k| k.batch_ops = renewal_ops) => Ok(())` (319 ops); `renewal_ops - 1 =>
  Err(RenewalOverOps)`; the adversary's set `derived(with(D, scheme = None, max_part_chunks = N))`
  at 248 `=> Ok(())` and 249 `=> Err(RenewalOverOps)`, with `3 + 2 × 249 == MAX_BATCH_OPS + 1`
  asserted.

**Not edited, for the human:** proposal 0016's knob table (`0016:1462-1479`) has no row for the
renewal clamp (`3 + 2 × MAX_PART_CHUNKS ≤ B_ops`). The brief forbids editing 0016 here. Decide
whether 0016 needs the row.

## Demonstrated reds

Each edit is applied to the final files, the named legs run with
`timeout 900 cargo test -p wyrd-core --test <file> -- <filters>`, then every file restored. The
working diff's sha256 was the same before and after the whole run (final tree:
`e867adb8…2426`, which is `patch.diff`). The set was run twice: once just before a one-line
doc-comment fix at `stage.rs:60`, and again on the final tree, with the same results. Script in
the appendix. M1–M18 are iteration 1's set, rerun on this iteration's files (anchors updated for
the new `renewing` signature, M8 only). M19 and M20 are new. Where an
edit also touches the test file, it only narrows a looping leg to one run, so each run's red is
shown on its own.

| # | Edit (brief's label) | Legs run → result |
|---|---|---|
| M1 | renewal removed entirely: `renew_at = reserved_at + u64::MAX` (657.1a's behaviour) | all 7 `r2_*` FAILED |
| M2 | slot rewrite dropped from `renewal_batch` | `r1_a_renewal_rewrites_the_slot_with_every_owned_lease`, `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` FAILED |
| M3 | renewal checked only between delivered pieces, no timer | `r2_d_i_a_renewal_commits_while_a_body_read_is_pending` FAILED |
| M4 | timer not raced against a pending put | `r2_d_ii_a_renewal_commits_while_a_fragment_write_is_pending` FAILED |
| M5 | `renew_if_due` on every intent round | `r2_no_renewal_is_sent_while_an_intent_is_in_flight` FAILED (`r2_a_renewal_due_as_an_intent_starts_is_sent_before_it` still passes: that renewal is still before the intent) |
| M6 | renewal awaited first | `r3_the_write_deadline_cuts_off_a_renewal_still_committing`, `r3_a_write_answered_while_a_renewal_commits_is_taken` FAILED |
| M7 | `RenewalLedger::abandoned` ignores `awaiting` | `r3_the_write_deadline_cuts_off…`, `r3_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt` FAILED |
| M8 | a landed renewal re-arms the body-idle deadline | `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases` FAILED |
| M9 | `check_live` removed from the renewal round | `r4_a_renewal_refuses_a_lapsed_slot_lease`, `…_owned_lease`, `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds`, `r7_the_quiesce_renewal_refuses_a_lapsed_lease` FAILED |
| M10 | retry reuses the first round's reading | `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds` FAILED |
| M11 | `doubt.release()` instead of `settle()` after an absent slot read | `r5_a_held_renewal_that_never_settles_ends_in_doubt` FAILED |
| M12 | `rounds += 0` | `r6_the_renewal_loop_is_bounded` FAILED |
| M13 | `StagedAttempt::renew` passes a one-reading clock | `r7_the_quiesce_renewal_reads_the_callers_clock_every_round`, `r7_the_quiesce_renewal_refuses_a_lapsed_lease` FAILED |
| M14 | cover check made always true | `r7_the_quiesce_renewal_covers_its_hand_off_or_refuses` FAILED |
| M15 | write deadline kept armed after the writes answered | `r3_a_renewal_outliving_an_answered_writes_deadline_is_awaited` FAILED |
| M16 | in-flight renewal re-checks the old leases after its commit returns | `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` FAILED |
| M17 | `RenewalOverOps` removed from `knob_clamps_hold` | `leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it` FAILED |
| M18 | staging's renewal check disabled (`false && …`) | `r8_a_chunk_bound_past_a_renewal_batch_is_refused` FAILED |
| M19 | **the iteration-1 defect**: `Raced::Halted(halt) => return Err(halt)` in `write_fragments` (the renewal's stop escapes before the writes are accounted for) | `r4_a_renewal_stopping_beside_a_pending_write_leaves_its_effect_unknown` FAILED in each of its four runs alone (Fails, Lapsed, Refused, HeldThenLapsed); `r4_a_renewal_stopping_after_the_writes_answered_keeps_their_account` FAILED in both `Unknown`-answer runs alone. Its two all-`Ok` runs pass under M19, as they must: they are the control, where the old answer was already right |
| M20 | a refusal let through as `Halt::Refused` although a write is unsettled | the pending-write leg's Refused run and the answered leg's `Unknown`+Refused run FAILED |

On M19 the failures are the right ones: `Fails`/`Lapsed`/`HeldThenLapsed` fail at
`classify == Indeterminate` (they were `Terminal`, or `InDoubt` with the lapse as the bare
cause), `Refused` fails at `expect_err` (it was `Ok(Refused(NoSuchUpload))`).

**657.1a's base.** The new legs do not compile there (`RenewOutcome`, `StageClock`,
`RENEWAL_ROUNDS`, `ABANDONED_COMMIT`, `KnobClamp::RenewalOverOps`, `Stop::SessionLeftOpen` don't
exist). M1 stands in for "no renewal at all", M17 for "the adversary's knob set passes", and M19
for "iteration 1's escape".

## Refuting my own tests

- **(a) Genuine red?** Yes. M19 is the fix reverted, and both new legs went red, each failing run
  checked alone. M1–M18 and M20 each turned their named legs red on the final files.
- **(b) Production path?** Yes. The legs call `wyrd_core::multipart::stage::stage_part`,
  `reserve_slot` and `StagedAttempt::renew`; the knob rows call
  `wyrd_core::multipart::knob_clamps_hold`. The store is `RedbMetadataStore::in_memory()` behind a
  wrapper that delegates every read and commit. No renewal or accounting logic is copied into the
  tests; the `3 + 2N` count is recomputed from the batch shape, not imported.
- **(c) Fixture includes the fault?** Yes. The held put is a real pending put inside the fake,
  which enforces the deadline at publication; after staging returns, the test releases it and
  sees it stored (`Event::Stored`), so the abandoned write really could still publish. The
  `Unknown` answer is the fake's real straddled publication (`Event::Unverified`). The renewal
  failure is a real commit error from the wrapper, the lapse is the real clock past the real
  stored lease, the refusal is the real `abort_session`, and the held round is a real unapplied
  commit kept in the wrapper's queue.

## C5: `cargo mutants --in-diff` (advisory)

Command (the gate's flags, plus `-j 4` and output in scratch), on this iteration's diff:
`cargo mutants --in-diff patch-draft.diff --no-shuffle --cap-lints=true -j 4`.
Result: **68 mutants, 51 caught, 16 unviable, 1 missed** (iteration 1: 64 / 51 / 12 / 1). The
mutants run used the tree one doc-comment line before the final one (`stage.rs:60` reworded, no
code change), so its line numbers after line 60 read one higher than the final tree's.

- **Missed: `stage.rs:326` `>` → `>=` in `StageRequest::check`** (the run's `:327`). The same
  survivor as iteration 1, and **equivalent**: `renewal_batch_ops(N) = 3 + 2N` is always odd and
  `MAX_BATCH_OPS` is 500, even, so no chunk bound makes the two sides equal and `>` and `>=` agree
  on every input. The iteration-1 adversary and code review reached the same verdict. Killing it
  would mean rewriting the check to compare chunks against a derived bound
  (`(MAX_BATCH_OPS − 3) / 2 = 248`), which adds a helper and its own arithmetic mutants for no
  change in behaviour; I left it. The knob clamp's `<=` (`multipart.rs`) is not equivalent,
  because `batch_ops` can be odd, and the on-bound row kills it.
- **New code in this iteration, all caught**: `write_fragments`' `!abandoned.is_empty() ||
  !reports.is_empty()` (`||` → `&&`, and each `!` deleted), `refused > 0` (three mutants),
  `write_fragments` → `Ok(())`, and `Display for Stop` → `Ok(Default::default())` (the new legs
  check the renewal text).
- **Unviable (16):** each replaces a return value with `Default::default()` (or `Raced::new()` and
  similar) for a type that has none, so they do not compile: `OwnedChunk::with_lease`,
  `StagedAttempt::renew`, `StageError::source`, `From<StageError> for Halt` (new: the boxing
  touched it), `abandoned_write` (new: its signature changed), `RenewalLedger::abandoned`,
  `stage_part`, `Footprint::renewal`, `Footprint::renew`, `Stager::expired`,
  `Stager::halted_beside_writes` (new), `Stager::renewing` (×4), `Stager::run`.

## Gates run

- `cargo test -p wyrd-core --test multipart_upload_part_stage --test multipart_knobs`: 89 + 8
  pass (iteration 1: 87 + 8; the two new legs). The staging file ran 10 times in a row with no
  failure.
- `cargo fmt --all` (no change after the last edit) and `cargo clippy -p wyrd-core --all-targets
  -- -D warnings`: clean. Clippy is what caught the `Halt` size (above).
- `./engine/xtask.sh ci` (`cargo xtask ci`, `PDCA_WORKTREE` set) on the final tree: "xtask ci: all
  checks passed", exit 0. That run includes `typos` (clean), `lint_docs: OK` and `render_site: …
  link audit OK` (the brief's `typos` and `docs-renderer` dependencies were present and ran), fmt,
  clippy, build, workspace tests, deny (one standing `license-not-encountered` warning, not an
  error), the statics gate and DST.
- `git apply --check -R patch.diff` on the worktree: clean, so the patch is exactly the tree that
  was gated and red-tested.

No external dependency was missing, so there is no NEEDS-HUMAN marker.

C4-verify: as the brief says, there is no added test file, so C4-verify takes its green-only branch
(`PASS (green-only)` is not a red→green). The per-fix red is the table above.

C4-diff-cov failed in iteration 1 with "patch.diff does not apply on origin/main". That is the
stack: this patch is against 657.1a's folded tree (`93a6f37`), not `origin/main`. It is a harness
limit for stacked bundles, not something the patch can fix; diff coverage stays unmeasured until
#932 merges and the branch is refreshed onto it.

## Iteration 1's design, kept (summary)

- Module docs: renewal row in the batch table; *Leases* covers renewals; *Renewal* section (when it
  runs, armed/disarmed deadlines, abandoned renewals, failing renewals, sampled leases and the
  timing assumptions, the `3 + 2N` size and its clamp); bounds table; one-clock section.
- `RENEWAL_ROUNDS = 4`, `ABANDONED_COMMIT`; `StageRequest::check` refuses a `max_part_chunks` whose
  renewal would pass `MAX_BATCH_OPS` (`stage.rs:325-329`); `StageClock` split out of
  `StageRuntime`; `RenewOutcome::{Covered, Refused}`; `StagedAttempt::renew(store, clock,
  cover_millis)`; `Stop::{RenewalRoundsSpent, CoverShort}`; `RenewalLedger` (doubt and an
  "awaiting a commit" flag kept outside the renewal future); `Stager::renewing` replacing 657.1a's
  `bounded`; `Footprint::{renewal, renewal_batch, renew, apply}`; `Stager::{renew_at, expired,
  renewed, renew_if_due}`.
- `crates/core/src/multipart.rs`: `RENEWAL_FIXED_OPS = 3` and `renewal_batch_ops`, the one count
  both clamps use; `KnobClamp::RenewalOverOps`, checked in `knob_clamps_hold` right after
  `PartCommitOverOps`; docs of `MAX_PART_CHUNKS` and `SlotRecord::with_lease`.
- Decisions kept: one timer wait for `min(renew_at, deadline)`; work polled before the renewal in
  the race; the quiesce renewal leases to the attempt's own TTL; cover rule `lease_expiry −
  checked_at ≥ max(cover_millis, 1)`; an in-flight renewal that sampled live leases is taken
  without a post-commit check; renewal clamp after the part-commit clamp.
- **Reversed in this iteration:** iteration 1's decision 6 ("a renewal that fails while work is
  pending stops staging with the renewal's own error"). For a body read it still does. For
  fragment writes it now reports the writes' account, as described at the top.

## Scratch

Working files for this iteration are in `$PDCA_SCRATCH/pdca-builder-933-v2/`. Iteration 1 left
`$PDCA_SCRATCH/pdca-builder-933-reds/`. I did not delete either: the harness owns cleanup of its
scratch root.

## Appendix: the demonstrated-red script

Run as `python3 -I reds.py <worktree> <results-file> [M1 …]` from outside the worktree.

```python
#!/usr/bin/env python3
"""Demonstrated reds for #933: apply one named edit, run the named legs, restore.

Usage: reds.py <worktree> <results-file> [mutant-id ...]
Every edit must match exactly once; the original files are always restored.
An edit may touch a test file too, only to narrow a looping leg to one run.
"""
import pathlib
import subprocess
import sys

WT = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
ONLY = set(sys.argv[3:])
STAGE = "crates/core/src/multipart/stage.rs"
MP = "crates/core/src/multipart.rs"
STAGE_TF = "crates/core/tests/multipart_upload_part_stage.rs"
STAGE_T = "multipart_upload_part_stage"
KNOBS_T = "multipart_knobs"

FIXED_CLOCK = """

/// MUTANT: a clock that only ever gives one reading.
struct FixedReading(u64);

impl StageClock for FixedReading {
    fn now_millis(&self) -> u64 {
        self.0
    }
}
"""

# The iteration-1 behaviour: a renewal that stops staging beside a chunk's writes answers with
# its own error or refusal, the writes dropped unaccounted.
ESCAPE = (STAGE, "            Raced::Halted(halt) => (None, Some(halt)),",
          "            Raced::Halted(halt) => return Err(halt),")
# A refusal answered as a refusal although a write is unsettled.
REFUSAL_THROUGH = (STAGE, "            Halt::Refused(refusal) => self\n                .footprint\n                .stop(writes(Box::new(Stop::SessionLeftOpen { refusal }))),",
                   "            Halt::Refused(refusal) => return Halt::Refused(refusal),")

PENDING_RUNS = "    for how in [\n        RenewalStop::Fails,\n        RenewalStop::Lapsed,\n        RenewalStop::Refused,\n        RenewalStop::HeldThenLapsed,\n    ] {"
ANSWERED_RUNS = "    for (answers_unknown, how) in [\n        (true, RenewalStop::Fails),\n        (true, RenewalStop::Refused),\n        (false, RenewalStop::Fails),\n        (false, RenewalStop::Refused),\n    ] {"


def only_pending(how):
    return (STAGE_TF, PENDING_RUNS, f"    for how in [RenewalStop::{how}] {{")


def only_answered(unknown, how):
    return (STAGE_TF, ANSWERED_RUNS,
            f"    for (answers_unknown, how) in [({unknown}, RenewalStop::{how})] {{")


PENDING_LEG = "r4_a_renewal_stopping_beside_a_pending_write_leaves_its_effect_unknown"
ANSWERED_LEG = "r4_a_renewal_stopping_after_the_writes_answered_keeps_their_account"

MUTANTS = [
    ("M1", "renewal removed entirely (657.1a's behaviour): the renewal instant never comes",
     [(STAGE, "            .reserved_at_millis()\n            .saturating_add(request.lease_ttl_millis / 2);",
       "            .reserved_at_millis()\n            .saturating_add(u64::MAX);")],
     [(STAGE_T, ["r2_"])]),
    ("M2", "the slot rewrite dropped from the renewal batch",
     [(STAGE, "            .require(self.slot_key.clone(), self.slot.clone())\n            .put(self.slot_key.clone(), renewal.slot.clone());\n        for (held, next)",
       "            .require(self.slot_key.clone(), self.slot.clone());\n        for (held, next)")],
     [(STAGE_T, ["r1_", "r2_a_renewal_that_crosses_expiry"])]),
    ("M3", "renewal checked only between delivered pieces, with no timer",
     [(STAGE, "            started = true;\n            if now < self.renew_at {\n                let wake = self.renew_at.min(deadline) - now;",
       "            let entered = !started;\n            started = true;\n            if !entered || now < self.renew_at {\n                let wake = deadline - now;")],
     [(STAGE_T, ["r2_d_i_"])]),
    ("M4", "the timer not raced against a pending fragment put",
     [(STAGE, "            started = true;\n            if now < self.renew_at {\n                let wake = self.renew_at.min(deadline) - now;",
       "            started = true;\n            let renews = late == Late::Refused;\n            if !renews || now < self.renew_at {\n                let wake = if renews { self.renew_at.min(deadline) } else { deadline } - now;")],
     [(STAGE_T, ["r2_d_ii_"])]),
    ("M5", "a renewal sent while the attempt's own intent is in flight (renewal checked every intent round)",
     [(STAGE, "            rounds += 1;\n            // Plan against the topology minus every server whose drain is recorded, read afresh",
       "            rounds += 1;\n            self.renew_if_due(store, runtime).await?;\n            // Plan against the topology minus every server whose drain is recorded, read afresh")],
     [(STAGE_T, ["r2_no_renewal_is_sent_while_an_intent_is_in_flight", "r2_a_renewal_due_as_an_intent_starts_is_sent_before_it"])]),
    ("M6", "the renewal awaited before the pending work instead of raced against it",
     [(STAGE, "                loop {\n                    if answered.is_some() {",
       "                loop {\n                    if true {\n                        break Some((renewal.as_mut().await, None));\n                    }\n                    if answered.is_some() {")],
     [(STAGE_T, ["r3_the_write_deadline_cuts_off_a_renewal_still_committing", "r3_a_write_answered_while_a_renewal_commits_is_taken"])]),
    ("M7", "a renewal still committing at the deadline reported as clean instead of in doubt",
     [(STAGE, "        if !self.awaiting {\n            return self.doubt.end(upload_id, cause);",
       "        if true {\n            return self.doubt.end(upload_id, cause);")],
     [(STAGE_T, ["r3_the_write_deadline_cuts_off_a_renewal_still_committing", "r3_a_renewal_cut_off_between_its_commits"])]),
    ("M8", "renewal deferring the body-idle bound (a landed renewal re-arms the idle deadline)",
     [(STAGE, "        deadline: u64,\n        late: Late,\n    ) -> Raced<F::Output> {",
       "        mut deadline: u64,\n        late: Late,\n    ) -> Raced<F::Output> {"),
      (STAGE, "        let judge = |output| match late {", "        let judge = move |output| match late {"),
      (STAGE, "                Ok(renewed) => self.renewed(renewed.at),",
       "                Ok(renewed) => {\n                    self.renewed(renewed.at);\n                    if late == Late::Refused {\n                        deadline = runtime.now_millis().saturating_add(self.request.body_idle_millis);\n                    }\n                }")],
     [(STAGE_T, ["r3_the_body_idle_bound_holds_while_renewals_keep_the_leases"])]),
    ("M9", "renewal that moves a lapsed lease forward instead of refusing (no lapse check)",
     [(STAGE, "            let now_millis = clock.now_millis();\n            self.check_live(now_millis)\n                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 1;\n            let renewal = self",
       "            let now_millis = clock.now_millis();\n            rounds += 1;\n            let renewal = self")],
     [(STAGE_T, ["r4_a_renewal_refuses", "r4_a_renewal_retry", "r7_the_quiesce_renewal_refuses_a_lapsed_lease"])]),
    ("M10", "a renewal retry that reuses the first round's clock reading",
     [(STAGE, "        let mut rounds = 0u32;\n        loop {\n            if rounds == RENEWAL_ROUNDS {",
       "        let mut rounds = 0u32;\n        let first_reading = clock.now_millis();\n        loop {\n            if rounds == RENEWAL_ROUNDS {"),
      (STAGE, "            let now_millis = clock.now_millis();\n            self.check_live(now_millis)",
       "            let now_millis = first_reading;\n            self.check_live(now_millis)")],
     [(STAGE_T, ["r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds"])]),
    ("M11", "after an unknown renewal outcome, an absent read taken as 'did not land' with may_still_commit: true",
     [(STAGE, "                        doubt.settle();\n                        let Some(moved) = self.reconcile(store, found, doubt).await? else {",
       "                        doubt.release();\n                        let Some(moved) = self.reconcile(store, found, doubt).await? else {")],
     [(STAGE_T, ["r5_"])]),
    ("M12", "a renewal loop counter that never reaches its bound",
     [(STAGE, "                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 1;\n            let renewal = self",
       "                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 0;\n            let renewal = self")],
     [(STAGE_T, ["r6_the_renewal_loop_is_bounded"])]),
    ("M13", "StagedAttempt::renew given a fixed now_millis",
     [(STAGE, "        let mut ledger = RenewalLedger::default();\n        let renewed = self\n            .footprint\n            .renew(\n                store,\n                clock,",
       "        let mut ledger = RenewalLedger::default();\n        let fixed = FixedReading(clock.now_millis());\n        let renewed = self\n            .footprint\n            .renew(\n                store,\n                &fixed,"),
      (STAGE, None, FIXED_CLOCK)],
     [(STAGE_T, ["r7_the_quiesce_renewal_reads_the_callers_clock_every_round", "r7_the_quiesce_renewal_refuses_a_lapsed_lease"])]),
    ("M14", "the post-commit coverage check removed from renew",
     [(STAGE, "            if lease_expiry_millis.saturating_sub(checked_at_millis) >= cover_millis.max(1) {",
       "            if lease_expiry_millis.saturating_sub(checked_at_millis) >= cover_millis.min(0) {")],
     [(STAGE_T, ["r7_the_quiesce_renewal_covers_its_hand_off_or_refuses"])]),
    ("M15", "the renewal awaited past a put's deadline treated as abandoned although the put had answered in time",
     [(STAGE, "                    if answered.is_some() {\n                        // Every write the deadline bounds has answered, or the read has: the\n                        // deadline is disarmed, and the renewal runs to its end.\n                        break Some((renewal.as_mut().await, answered));\n                    }\n",
       ""),
      (STAGE, "                    let woke = poll_fn(|cx| {\n                        if let Poll::Ready(output) = work.as_mut().poll(cx) {\n                            return Poll::Ready(Woke::Work(output));\n                        }",
       "                    let done = answered.is_some();\n                    let woke = poll_fn(|cx| {\n                        if !done {\n                            if let Poll::Ready(output) = work.as_mut().poll(cx) {\n                                return Poll::Ready(Woke::Work(output));\n                            }\n                        }"),
      (STAGE, "                        Woke::Renewed(renewed) => break Some((renewed, None)),",
       "                        Woke::Renewed(renewed) => break Some((renewed, answered.take())),"),
      (STAGE, "                        Woke::Timer => match work.as_mut().now_or_never() {",
       "                        Woke::Timer if done => continue,\n                        Woke::Timer => match work.as_mut().now_or_never() {")],
     [(STAGE_T, ["r3_a_renewal_outliving_an_answered_writes_deadline_is_awaited"])]),
    ("M16", "a renewal that sampled its leases live refused after its commit returned late",
     [(STAGE, "            let (at, lease_expiry_millis) = (landed.at, landed.record.lease_expiry_millis());\n            self.apply(landed);",
       "            let (at, lease_expiry_millis) = (landed.at, landed.record.lease_expiry_millis());\n            if cover_millis.is_none() {\n                self.check_live(clock.now_millis())\n                    .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            }\n            self.apply(landed);")],
     [(STAGE_T, ["r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence"])]),
    ("M17", "the renewal clamp removed from knob_clamps_hold",
     [(MP, "    ensure(\n        renewal_batch_ops(set.max_part_chunks) <= ops,\n        RenewalOverOps,\n    )?;\n", "")],
     [(KNOBS_T, ["leg2_"])]),
    ("M18", "the renewal clamp removed from staging's request check",
     [(STAGE, "                renewal_batch_ops(self.max_part_chunks) > u64::from(MAX_BATCH_OPS),",
       "                false && renewal_batch_ops(self.max_part_chunks) > u64::from(MAX_BATCH_OPS),")],
     [(STAGE_T, ["r8_"])]),
]

# M19: the iteration-1 defect, one run of each new leg at a time.
for how in ["Fails", "Lapsed", "Refused", "HeldThenLapsed"]:
    MUTANTS.append((f"M19-pending-{how}",
                    f"a renewal stopping beside a pending write answers its own stop (iteration 1); run {how}",
                    [ESCAPE, only_pending(how)], [(STAGE_T, [PENDING_LEG])]))
for unknown, how in [("true", "Fails"), ("true", "Refused"), ("false", "Fails"), ("false", "Refused")]:
    MUTANTS.append((f"M19-answered-{unknown}-{how}",
                    f"a renewal stopping after the writes answered answers its own stop (iteration 1); run unknown={unknown} {how}",
                    [ESCAPE, only_answered(unknown, how)], [(STAGE_T, [ANSWERED_LEG])]))
# M20: the refusal let through as a refusal while a write is unsettled.
MUTANTS.append(("M20-pending", "a refusal answered as a refusal beside a pending write",
                [REFUSAL_THROUGH, only_pending("Refused")], [(STAGE_T, [PENDING_LEG])]))
MUTANTS.append(("M20-answered", "a refusal answered as a refusal after a write answered Unknown",
                [REFUSAL_THROUGH, only_answered("true", "Refused")], [(STAGE_T, [ANSWERED_LEG])]))


def run(target, filters):
    cmd = ["timeout", "900", "cargo", "test", "-p", "wyrd-core", "--test", target, "--"] + filters
    proc = subprocess.run(cmd, cwd=WT, capture_output=True, text=True)
    text = proc.stdout + proc.stderr
    keep = [l for l in text.splitlines()
            if l.startswith("test ") or "test result" in l or l.startswith("error")
            or "panicked at" in l]
    return proc.returncode, keep


def main():
    with OUT.open("a") as out:
        for mid, label, edits, runs in MUTANTS:
            if ONLY and mid not in ONLY:
                continue
            originals = {}
            try:
                for path, old, new in edits:
                    f = WT / path
                    originals.setdefault(path, f.read_text())
                    text = f.read_text()
                    if old is None:
                        text = text + new
                    else:
                        n = text.count(old)
                        assert n == 1, f"{mid}: edit matched {n} times in {path}: {old[:60]!r}"
                        text = text.replace(old, new)
                    f.write_text(text)
                out.write(f"=== {mid}: {label}\n")
                for target, filters in runs:
                    rc, keep = run(target, filters)
                    out.write(f"--- cargo test -p wyrd-core --test {target} -- {' '.join(filters)}  (exit {rc})\n")
                    out.write("\n".join(keep) + "\n")
                out.flush()
            finally:
                for path, text in originals.items():
                    (WT / path).write_text(text)


if __name__ == "__main__":
    main()
```
