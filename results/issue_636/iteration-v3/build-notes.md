# Build notes — issue 636 / multipart-commit-protocol (iteration 3)

Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `origin/pdca-integration/main` (wave-2 stack base, worktree HEAD
`d15d555 pdca-integrate: issue_635`). Every `path:line` below is against that tree **with this
patch applied**. Iterations 1 and 2 are preserved in `iteration-v1/` and `iteration-v2/`.

---

## 1. What this iteration is

Iteration 2 passed every gate **except one**: `T4 batched multi-pass rubric review` — 38 blocking
findings (`review-batch.md`), 0 recorded-rejected. `C4-ci` passed, `C4-verify` passed,
`T4-contribution` passed, `C5-mutants` timed out with no verdict. So this round is **not** a
redesign: the record family, the state machine, the knob derivations and the twelve
success-criterion legs are iterations 1–2's and still hold, and **the value set is unchanged** —
re-read §2 of `iteration-v1/build-notes.md`; nothing here re-chose a knob.

What changed is **all 38 findings, fixed** (they dedupe to 21 distinct defects), plus the T5
sign-off item the carry-forward names (the no-gap oracle's epoch exactness), plus **seventeen new
tests** — 11 in `multipart_protocol.rs`, 1 in `multipart_admission_and_drain.rs`, 5 unit tests in
`multipart.rs` — and three extended oracles, so that **every** fix is bound by an assertion that
goes red when the mechanism is removed (§4). Nothing is recorded-rejected this round: every finding was real.

## 2. The 21 distinct defects, and what each actually was

Ordered by the harm, not by the line number. "×N" is how many of the three review passes
independently reported it.

| # | Finding (line on iteration-v2) | What it actually was | Fix |
|---|---|---|---|
| 1 | `metadata.rs:4391` ×3 | `flip_retires` accepted **any put whose key matched**, so three different flips satisfied the segmented-prior rule while stranding the prior generation: a put of the wrong *value*, a put the same batch *deletes*, and a blind put with no `require_absent` (which overwrites a colliding obligation and loses *its* evidence) | the witness is now the whole mutation — exact key, exact planned value, the collision precondition, and no same-batch delete (`crates/core/src/metadata.rs:4657-4684`) |
| 2 | `multipart.rs:3392`, `:3511`, `:3515`, `:3479`, `:3424` | the **segment walk probed forward** and read the first absent index as end-of-range, so a missing *middle* `seg:` record finished the obligation with every later segment alive and unevidenced; the delete walk read a token/space mismatch as an empty key list and called that "done"; and `next_session_part` passed an **empty** cursor with `usize::MAX` — an O(parts²) sequence of full-range reads | every unit space is **enumerated** from the bounded range that holds it (`next_segment_unit`, `next_session_part`, `multipart.rs:3769-3819`), a gap is stepped over rather than mistaken for the end (`:4108-4126`), key derivation is total and propagates (`unit_record_keys`, `:3867-3892`), and each "next unit" is one page of one row keyed by the cursor the walk already has |
| 3 | `multipart.rs:3511`/`:3515` ×2 | an obligation naming **both** sparse parts and segments walked the two index spaces in **lockstep** on one counter and stopped at the first *joint* gap — the obligation deleted while later part records still named durable bytes | the payload's spaces are walked one after the other, each to its own exhaustion, with the cursor saying which (`RetirePayload::delete_spaces`, `metadata.rs:478-518`; `RetireCursor::arm`, `metadata.rs:549-556`; the walk at `multipart.rs:4225-4295`) |
| 4 | `multipart.rs:3827` ×2 | the cursor put, the obligation delete and the `seggrp:` marker mutations were appended **after the last `fits` check**, so a bounded step could commit an over-envelope batch — and re-derive the same over-envelope shape on every retry (stuck, not slow) | the closing cost is **withheld from the walk's budget up front** (`multipart.rs:4064-4076`, `CLOSING_SLACK_BYTES`/`CLOSING_OPS` at `:4429-4440`) |
| 5 | `multipart.rs:1650` | `CommitUnknownResult` propagated from the create **before any re-read**, and the API exposed neither the minted tokens nor a resume parameter — so the documented "re-drive the same upload id" was unperformable: an ordinary retry mints a new id and, if the first batch lands, leaves an `Open` session no client will ever name holding an admission slot, with the counter charged twice | re-read and adopt by byte equality; otherwise `IndeterminateCommit` carries the identity (`multipart.rs:1824-1841`), and `CreateParams::resume` presents it back (`:1525-1533`, `:1712-1725`) |
| 6 | `multipart.rs:1870` | the same, one namespace over: an unknown-result slot commit lost the locally minted attempt id, so each retry claimed a **second** index and leaked one of `MAX_INFLIGHT_PARTS` | `IndeterminateCommit::part_attempt` (`multipart.rs:1569-1639`, raised at `:2109-2129`), and `upload_part` re-drives once with that id (`:2621-2652`) |
| 7 | `multipart.rs:2100` ×3 | the **second** compensation outcome was discarded, so a fence-and-release race left the slot, the owned entries and the staged bytes on a session that is *still `Open`* — nothing walks an `Open` session's `sidx:` range — while returning a clean refusal | a bounded epoch chase whose every outcome is inspected, ending in `RecordError::CompensationUnlanded` rather than a tidy lie (`multipart.rs:2305-2348`) |
| 8 | `multipart.rs:2781`, `:2796` ×2 | an unknown result from the **fence** commit, and **any** `Err` from `publish_fenced` (a deterministic over-ceiling root, a dangling dirent), left the session `Completing` — where Complete answers `OperationAborted`, Abort answers `OperationAborted`, and the only exit is the reaper this slice merges without | the fence outcome is *established* by a re-read that adopts this caller's own fence bytes (`multipart.rs:3088-3101`), and the error path releases the fence best-effort **around** the original error (`:3134-3147`, `release_fenced_session` at `:2862-2882`) |
| 9 | `multipart.rs:3989` ×2 | `reclaim_owned` is **public** and verified nothing: called for an `Open` session it orphan-marks the fragments of a part in flight *right now* and deletes the only records protecting them | fence-then-walk is enforced in the function — the session is re-read every pass and its exact bytes pinned in the batch (`multipart.rs:4510-4531`) |
| 10 | `multipart.rs:4137` ×3 | `saturating_sub` on the admission decrement silently accepted a **zero count while the session record still exists**, deleted the session and reported a decrement that never happened — an undercount of live sessions, the one direction that lets `MAX_SESSIONS × U_ref ≤ W_ref` be exceeded | `checked_sub` → `RecordError::AdmissionLedgerTorn` (`multipart.rs:4662-4676`) |
| 11 | `multipart.rs:4399` ×3 + `:4437` (**T5**) | the classification sweep — the oracle every other leg leans on — scanned **every epoch under a segment nonce** (the constructed group was literally unused), collapsed obligations to **chunk ids**, and treated **any** value at an `orphan:` key as evidence though the production collector ignores one it cannot parse. Each is a way an unprotected byte is reported safe | the obligated generation's **own** `seg:<nonce>:<epoch>:` range, an obligated set keyed by `(chunk, index, dserver)`, and the collector's own parse spelled the same way (`multipart.rs:4915-4956`, `:4979-4996`; mirrors `crates/custodian/src/gc.rs:402-417`) |
| 12 | `metadata.rs:197` ×2 | `PartNumberSet` derived its `Deserialize`, so reversed, overlapping, out-of-range and non-canonical runs decoded — a reversed run underflows `len`, an unordered one makes the drain cursor's `first_at_or_after` skip members, and an out-of-range one names a `part:` key the grammar cannot spell | a validating decode (`metadata.rs:210-296`) |
| 13 | `metadata.rs:249` | `SegmentGeneration` did not validate its nonce at decode; the later constructor failure became an **empty key list** deep inside the drain | validated at decode through the same predicate `SegmentGroup::new` applies (`metadata.rs:298-353`) |
| 14 | `metadata.rs:355` | `RetireCursor` accepted payload-relative out-of-range `inner`/`frag`, which the drain treated as *exhausted* — silently skipping every position between the record's true length and the cursor | the resume is checked against the unit's own length → `ChunkMapError::RetireCursorOutOfRange` (`multipart.rs:4128-4155`) |
| 15 | `multipart.rs:3614` ×2 | `drain_step` cross-checked only the key **mode**, so a `Session` payload under a *generation* token was accepted — its walk enumerates a part range keyed by an upload id the token does not have, finds it empty, and deletes the obligation | `payload_matches_token` (`multipart.rs:3927-3963`), applied before any work (`:4035-4040`) |
| 16 | `multipart.rs:3281` | drain reports charged batches **before** the commit outcome, so conflicts counted as committed batches and inflated the envelope observables leg G asserts on | charged after `Committed`, in all three writers (`multipart.rs:3599-3612`, `:4320`, `:4586`, `:4694`) |
| 17 | `multipart.rs:370` | the knob validator never bounded `max_parts_per_session` by the **key grammar**, so a legal configuration could write part keys its own fixed-width parsers reject | bounded by `PART_NUMBER_FORMAT_MAX` (`multipart.rs:370-381`), which now lives beside `PartNumberSet` in `metadata.rs:194-207` so one constant binds both |
| 18 | `multipart.rs:3301` | `parse_retire_key` accepted any text as an upload id or attempt id — the id keys the very ranges the drain enumerates, so a wrong one makes them empty *for the wrong reason* | the 128-bit token grammar, enforced (`multipart.rs:3644-3663`) |
| 19 | `multipart.rs:2198` | `commit_part` stamped `committed_at_millis` from an unchecked clock: a public entry point where a caller could mix clock sources inside one session's lifecycle (`AGENTS.md:131-142`) | `clock_source` is a parameter and is checked (`multipart.rs:2415-2430`) |
| 20 | (part of #2) `unit_record_keys` returning `Vec::new()` for an invalid segment-group identity | silent "nothing to delete" ⇒ obligation deleted | `Result`, propagating `RetireTokenPayloadMismatch` |
| 21 | (new, from #12/#13) `RetirePayload` shapes no writer produces | a `Generation` naming **both** a flat list and a `seg:` generation evidences half its payload and deletes the obligation; an empty `Chunks`/`Parts`/`Records` obligation owes nothing | `RetirePayload::validate` at decode (`metadata.rs:427-476`) |

Two consequences worth the human's eye:

* **`RetireCursor` gains `arm`** — a `skip_serializing_if` field, so a cursor that names one space
  re-encodes byte-identically to one written before it existed (`AGENTS.md:170-174`), asserted by
  `a_legacy_drain_cursor_round_trips_byte_identically`.
* **`compensate_or_defer` can now return an error** where it used to return `Ok(())`. It is
  reachable only when the session is re-fenced faster than three compensations can land; in that
  state the *tidy* answer was false. `commit_part`'s ordinary refusals (`EntityTooLarge`, the
  invalid-part answers) are unchanged — leg C still asserts them as typed outcomes.

## 3. What is deliberately NOT changed

* **`Completed` sessions still hold their admission slot.** The brief's CORRECTED 2026-07-26
  paragraph, unchanged from iteration 1.
* **`reconcile_step`'s signature and `GcContext`'s fields** — #637's.
* **The `orphan:` value is still the bare decimal** (#637 owns the dual-format decode). This
  round makes the *oracle* read it exactly as `gc::orphan_leases` does, which is the opposite of
  changing the format: it stops the test from crediting evidence production would ignore.
* **The `retire:` family stays in `crates/core/src/metadata.rs`**, re-exported from
  `multipart.rs` (iteration 2's §6(iv) reasoning; `metadata.rs` is the base layer and has no
  `use crate::` line).

## 4. Demonstrated red — the negation runs (**no gate consumes these**)

`build-notes.md` is withheld from the reviewer and no `[[gates.checks]]` row reads it, so these
are **sign-off evidence for the human**: please read the recorded output rather than treating the
C4-verify PASS as proof. Every negation was applied to one file, run through the project's own
runner, then reverted and the file verified byte-identical to its pre-negation copy (`diff -q`,
enforced by the harness script — a run whose edit did not match aborts rather than reporting a
false red).

### The brief's three mandatory legs

**Leg F — collapse the two retry budgets into one** (`MAX_ADMISSION_CAS_ATTEMPTS: 64 → 4`):

```
test f_concurrent_creates_on_an_empty_store_all_succeed ... FAILED
8 concurrent creates against an EMPTY store: one was refused with
SlowDown { pressure: AdmissionContention { attempts: 4 } }. The admission ledger is nowhere near
its bound — this is the false `503 SlowDown` a single retry budget shared between the 2^-128
upload-id collision and the globally serialized admission CAS produced. 4 of 8 succeeded.
```

**Leg G1 — truncate derivation while declaring the obligation drained** (`finished = true` on a
budget-full batch):

```
test g_a_4001_part_complete_drains_to_empty_in_byte_budgeted_batches ... FAILED
8,002 record deletes under a 200-operation budget must take more than 40 batches, saw 1
```

**Leg G2 — a cursor that never advances** (the obligation re-written with a zeroed cursor):

```
test g_a_partially_drained_teardown_converges_with_no_double_decrement ... FAILED
called `Result::unwrap()` on an `Err` value:
DrainStalled { obligation: "retire:bytes:s:00000000000000000000000000000002:1" }
```

**Leg E1 — skip the terminal-delete decrement** / **E2 — apply it twice**:

```
E1: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 2   right: 1
E2: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 0   right: 1
```

### This iteration's own fixes, negated the same way

| Negation | Observed failure |
|---|---|
| `flip_retires` back to a key-only witness | `a_publication_over_a_segmented_generation_must_carry_its_retirement ... FAILED` — `a near-miss retirement must not publish: Committed` |
| `next_session_part` back to the empty cursor + `usize::MAX` | `the teardown read 22474590 rows for 4001 parts: the per-unit page bound is gone` (bound: 80,020; the run also took 39.6 s against 18.3 s) |
| sweep scans every epoch under the nonce | `a stale epoch's segments and an unnamed position protect nothing: [ObligatedForRetirement] / []` |
| obligated set keyed by chunk id | `a stale epoch's segments and an unnamed position protect nothing: [] / [ObligatedForRetirement]` |
| "present means evidenced" | `an orphan value the collector ignores is not evidence: [EvidencedForReclamation]` |
| `reclaim_owned` without the session guard | `an OPEN session's in-flight entries are untouched — left: [] right: [sidx:…]` |
| saturating admission decrement | `a torn ledger must not be papered over: DrainReport { … terminal_deleted: true, count_decremented: true }` |
| one delete space instead of two | `…and so is every segment of the rolled-back attempt` (the segments survive) |
| probe-forward segment walk | `a segment AFTER the gap must still be evidenced — the gap is not the end of the range` |
| create propagates the bare unknown result | `the identity to re-drive with must reach the caller` |
| reserve propagates the bare unknown result | `the identity to re-drive with must reach the caller` |
| compensation chase discards its outcome | `a refusal whose compensation never landed is not a clean refusal: Refused(EntityTooLarge { chunks: 2, limit: 1 })` |
| closing mutations appended after the last budget check | `the largest COMMITTED batch was 5306 B, past the 4000 B envelope` |
| report charged before the commit outcome | `a lost batch changed nothing, so it charges nothing — left: DrainReport { batches: 1, … max_batch_bytes: 807 }` |
| `commit_part` without the clock check | `a part commit stamped from another clock epoch must be refused` |
| `payload_matches_token` not applied in `drain_step` | `a token that cannot own the payload is a boundary error: Advanced` |
| `PartNumberSet` derived decode | `[[5,3]] must not decode` |
| `RetirePayload::validate` dropped | `{"payload":{"Generation":{…"chunks":[…],"segments":{…}}}} must not decode` |
| `parse_retire_key` without the token grammar | `retire:bytes:s:not-a-token:4 must not parse` |

**Two negations changed the patch rather than confirming it**, and that is the point of running
them:

* the first `closingBudget` negation **passed** — leg G's 4,001-part fixture is *operation*-bound,
  and the parity of `1 + 2k ≤ 200` happens to leave exactly one operation of headroom, so the
  closing put never crossed the envelope there. The fix was therefore unbound until
  `a_drain_step_budgets_the_mutations_that_close_it` was written with a **byte**-bound envelope
  and a payload that is a real fraction of it. Without that test, the reviewers' finding would
  have been "fixed" with no oracle.
* the first `tokenCrossCheck` negation **passed** — the unit test exercised
  `payload_matches_token` directly, so deleting its *call site* in `drain_step` changed nothing.
  `a_retire_key_that_cannot_own_its_payload_is_refused` now drives the guard through the real
  drain.

### The C4-verify RED leg, measured honestly

Unchanged from iterations 1–2, and re-stated because it is a **declared** limitation of this
slice's shape: the red is **criterion-absence**. With production reverted and the two added test
files kept, the failures are build errors (`unresolved import wyrd_core::multipart`, `no field
'owner' on PendingEntry`), so **0 tests ran and 0 tests failed**. `run-verify.sh` scores a build
failure as a red without ever counting tests (its `TESTS_RAN == 0` guard sits inside the
cargo-*succeeded* branch, `engine/scripts/run-verify.sh:416-427`). **Stated plainly: the
C4-verify red is a build error, not a flipped assertion** — the load-bearing evidence is the
twenty-three negation runs above.

### The DST cases ran, and how many seeds

`crates/dst/tests/concurrency.rs` keeps its `#![cfg(madsim)]` and is a **modified** path (never an
added one), so it is evaluated by C4-ci, not C4-verify. Under `cargo xtask ci` → `run_dst`
(`RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=50`, `xtask/src/main.rs:1573-1614`) the file reports
**12 tests where the base had 5**, including `multipart_publication_race_holds_on_sim_tikv`,
`concurrent_slot_reserves_never_exceed_the_cap_on_sim_tikv`,
`two_concurrent_drainers_over_one_owned_range_are_exactly_once` and
`a_segmented_publication_that_loses_the_root_flip_still_publishes`. Each `#[madsim::test]` is
multiplied across **50** seeds; leg I(i) is a `#[test]` that sweeps **48 seeds explicitly**
(`crates/dst/tests/concurrency.rs:584`) and asserts both orders were reached, so its coverage
cannot go vacuous.

## 5. Before declaring done — the three forced questions

**(a) Genuine red? YES.** Twenty-three negation runs above, each reverted and re-verified
byte-identical, each producing a *named* failure rather than a compile error (two negations that
only produced compile errors under `-D warnings` were re-expressed so they exercise behaviour).
Two of them changed the patch. Plus the C4-verify-shaped red, measured and labelled a build error
with 0 tests run.

**(b) Production path? YES.** The tests drive `wyrd_core::multipart`'s real verbs, the real
`MetadataStore` seam, the real `PlacementChunkStore`, `write::stage_intent`, `metadata`'s real
committers (`unlink`, `commit_chunk_map_superseding`, `SegmentedPublication`) and `read::read_path`
for the read-back. Nothing is stubbed except the *caller*, which is the test instead of #508's S3
handler. The one place a test writes a record directly is where the **writer is another slice**
(#625's rollback installs a `Records` obligation naming both spaces; a torn ledger; a torn
cursor) — the code under test is still the production drain, and in each case the assertion is on
durable store state afterwards.

**(c) Fixture includes the fault? YES**, and this round's fixtures are built around the fault
rather than beside it: the unknown-result legs **inject a commit that may or may not have
applied** and assert both arms; the compensation leg **keeps rolling the session back** so the
compensation genuinely never lands; the missing-middle-segment leg **removes a middle record**
and asserts on the segment *after* the gap; the oracle leg plants a **stale epoch under the same
nonce**, a fragment at a position no record names, and an `orphan:` value production cannot
parse; the closing-budget leg uses a payload that is a real fraction of the envelope.

## 6. Gates

* `cargo xtask ci` — **green** (`$PDCA_SCRATCH/pdca-builder-636-ci-iter3-final.log`, EXIT=0),
  including the prose gates (`typos`, `lint_docs`, `render_site --check`), so the docs-currency
  edits to `docs/design/architecture/{06,08}` are genuinely gated here rather than warn-skipped.
  Both external dependencies the brief declared (`typos`, `docs-renderer`) are installed on this
  host; nothing else was needed and **no NEEDS-HUMAN external dependency arose**.
* `cargo fmt --all -- --check` — clean, so the target's commit hook will not reject the patch.
* `cargo clippy --workspace --all-targets` and the `--cfg madsim` DST pass — clean under
  `warnings = "deny"` + `clippy::all = "deny"`.
* `git apply --check patch.diff` on a **pristine `d15d555`** worktree — applies cleanly.
* Test counts: `wyrd-core --lib` **134** (was 129 in iteration 2), `multipart_protocol` **24**
  (was 13), `multipart_admission_and_drain` **6** (was 5), `dst/concurrency` **12** (base 5).

**Two gate notes for the human.**

1. `C5-mutants` had no verdict in iteration 2 (killed at the 7200 s timeout). This round adds 11
   fast tests and no new heavy fixture, so the per-mutant cost is essentially unchanged — the
   campaign's cost is dominated by the three 4,001-part fixtures that iteration 1 introduced. If
   the row times out again it is a **budget** question, not a signal about this patch.
2. The deferred C5 note about **#638** (staged fragment writes have no server-enforced write
   deadline yet) is unchanged and is *by construction*: #638 is a declared `Depends on` of this
   brief and ships that deadline. Nothing in this slice can close it, and a second timeout inside
   `core`'s fan-out is the thing `results/issue_508/review-rejected.md` already rejected.

## 7. Scratch

Everything under `$PDCA_SCRATCH` (`/var/tmp/pdca`) and named `pdca-builder-636-*`: two CI logs,
the negation directory (`pdca-builder-636-neg3/` — 23 logs plus the per-negation backups), and one
throwaway `git worktree` for the apply-check. **All removed** before finishing; the worktree was
`git worktree remove`d. (Iterations 1–2's own `pdca-builder-636-neg-*.log` files are left where
they were — they are not this run's to delete.) No `/tmp` use. The negation *output* is transcribed above rather than left
on disk, so §9 of sign-off has it without depending on a scratch directory surviving.

## 8. Self-review against the target's standing rubric (`AGENTS.md:122-210`)

* *One clock per correctness lifecycle* — now checked at **four** entry points (reserve, part
  commit, Complete, Abort); `core` still reads no clock, every instant arrives as an argument, and
  the drain bounds **work**, never time.
* *Metadata validation boundaries* — structural checks widened at decode (`PartNumberSet`,
  `SegmentGeneration`, `RetirePayload`, the cursor's arm), with the one deliberately *contextual*
  check (placement length) left liberal on 0016's own instruction, as recorded in
  `review-rejected.md`.
* *Serialization identity* — `RetireCursor::arm` is `skip_serializing_if`, with a byte-identity
  round-trip test on a legacy cursor; `PendingEntry`'s two fields keep theirs.
* *Absent or unsupported entries* — five more silent paths became explicit errors this round
  (`AdmissionLedgerTorn`, `CompensationUnlanded`, `RetireTokenPayloadMismatch`,
  `RetireCursorOutOfRange`, and the empty-key-list arms).
* *Transactions* — every early return over staged work routes through a compensation whose outcome
  is inspected; `CommitUnknownResult` is re-read and *identified* rather than propagated bare, and
  the wrapper keeps `wyrd_traits::classify` answering `Indeterminate` (asserted).
* *Test fidelity* — the new concurrent/destructive paths land with seeded Tier-0 DST coverage
  (unchanged from iteration 2) and the crate-level oracle now mirrors the production collector's
  own parse.
* *Docs currency* — `06-runtime-view.md` gains the two-index-space rule this round; `08` keeps
  iteration 2's record-family paragraph.

## 9. Verification trail (paste-ready)

```
./engine/xtask.sh ci                                          -> EXIT=0 (ci-iter3-final.log)
cargo fmt --all -- --check                                    -> clean
cargo test -p wyrd-core --lib                                 -> 134 passed
cargo test -p wyrd-core --test multipart_protocol             -> 24 passed
cargo test -p wyrd-core --test multipart_admission_and_drain  -> 6 passed
RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test concurrency -> 12 passed (50 seeds each)
git apply --check patch.diff on a pristine d15d555            -> applies cleanly
```
