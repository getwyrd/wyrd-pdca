# Build notes — issue 636 / multipart-commit-protocol (iteration 2)

Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `origin/pdca-integration/main` (wave-2 stack base, worktree HEAD
`d15d555 pdca-integrate: issue_635`). Every `path:line` below is against that tree with this
patch applied. Iteration 1's artifacts are preserved in `iteration-v1/`.

---

## 1. What this iteration changed, and why

Iteration 1 shipped the protocol and passed `cargo xtask ci`, but Check returned **C3/T1/T2/T3/T4
FAIL**, two `[impl]` NEEDS-HUMAN cells (C5, T5) and **51 union'd rubric findings**. This iteration
is not a rewrite: the record family, the state machine, the knob derivations and the twelve
success-criterion legs are iteration 1's and still hold. What changed is every finding, plus the
three carry-forward items. The value set (§2 of `iteration-v1/build-notes.md`) is **unchanged** —
please re-read it there; nothing in this round re-chose a knob.

### The four gating verdicts

**C3 FAIL — "the slice owes bounded `retire:bytes:` routing for ordinary delete and overwrite".**
Iteration 1 declined this (its §5(i)) and the reviewer was right that the brief requires it (*Impact
& compatibility*: "supersede and `unlink` stop expanding orphans inline and route through
`retire:bytes:{generation}`") and that 0016 makes it normative twice
(`0016:1084-1104`, and the first two rows of decision 4's failure-mode table). It is now
implemented, and the shape is 0016's own three-arm routing rather than a blanket conversion —
which is what makes it a **bounded** change instead of the 11-callsite rewrite iteration 1 costed:

* `metadata::plan_generation_retirement` (one decision, three arms) + `metadata::retire_generation`
  (one implementation), consumed by `unlink`, `commit_chunk_map_superseding`,
  `commit_chunk_map_superseding_leased` **and** multipart's `publication_batch`, so the four cannot
  drift;
* **inline** while the fan-out fits half a batch on both axes — which is every object today's
  tests use, so all five oracles iteration 1 named as the cost (`mutation_regressions.rs`,
  `custodian/tests/gc.rs`, `restore_reconcile.rs`, `server/tests/custodian_gc.rs`,
  `dst/tests/custodian.rs`) pass **unchanged**. That is the measured cost of the split: 0 lines of
  oracle rewriting, against the ~5 files iteration 1 predicted;
* **obligation** above it and for *every* segmented generation, which removes the two hard
  refusals that made a multipart-published object undeletable
  (`SegmentedRetirementUnsupported` in `unlink` and both superseding committers);
* `SegmentedPublication::root()`'s refusal is **kept but made conditional and checked**: a
  segmented prior is admissible only when the caller's flip contribution carries the exact
  `retire:bytes:g:<inode>:<version>` key `plan_generation_retirement` chose (`flip_retires`). A
  promise would have been a comment; this is a witness.

**The residual arm is declared, not hidden.** A flat map so large that its chunk list will not fit
one obligation *value* falls back to inline. Under the settled knobs that is unreachable
(`MAX_MAP_CHUNKS × b_ref ≤ V/2` leaves exactly the 2× headroom the obligation needs), but it is
reachable for a map past its own ceiling — and it is how I found a real regression: the
`s3_http_wire` real-SDK test uses an **8-byte** chunk size, so its 9 000-byte object is 1 125
chunks, whose obligation payload exceeds `V`. My first cut returned `ValueOverCeiling` there and
the gateway answered **500 on DELETE**. The three-arm planner keeps today's behaviour for that
shape rather than making it undeletable; `docs/design/architecture/08` states it.

**Why this is safe to land before #637 (iteration 1's decisive objection).** Iteration 1 argued the
conversion would move orphan evidence later "while GC still has no notion of the obligation". Two
things answer it. (a) The direction is monotone-safe: GC reclaims only on **explicit** evidence and
otherwise retains (`crates/custodian/src/gc.rs:158-198`), so a *later* mark is never an earlier
reclamation — 0016 says this in as many words (`0016:1124-1126`, "never reclaim earlier than
today's inline orphaning, only later"). (b) The one hazard that *is* real — a fragment carrying an
**already-expired** mark from an older event, whose reference a new event removes — is T5's, and it
is now fixed rather than deferred (below). Before this change that hazard existed too and was worse:
the operation simply failed.

**T1 FAIL — the classification sweep's fleet-wide `sidx:` scan.** Every namespace the sweep reads
is now walked with `scan_page` in bounded pages (`scan_pages`, `SCAN_PAGE_ROWS = 1_000`). `scan`
answers `ScanCapExceeded` *whole*, so the audit used to fail at exactly the scale the disjoint
namespace exists to support.

**T2 FAIL — the session decoder.** `decode_session` now validates the **whole** state × field
table (four optional fields × four states, required *and* forbidden), with a new
`RecordError::FieldRequiredByState`. Ten illegal shapes are asserted to fail and four legal ones to
pass (`multipart::tests::every_state_field_combination_is_validated_not_just_two`).

**T3 FAIL / C5 NEEDS-HUMAN — the stale segmented resume, and a regression that catches it.** This
is the item the carry-forward is most pointed about ("the current DST race stays flat, so it cannot
catch the stale resume/fence path"). Fixed in two places — `publish_fenced` re-reads the session
between flip attempts and retries from the record its own segment batches left; the refusal path's
`release_fence` **outcome is acted on** (re-read + one retry) instead of discarded — and bound by a
new DST case, `a_segmented_publication_that_loses_the_root_flip_still_publishes`.

That case forces the interleaving instead of hoping for it, and the difference is the whole point:
my first version swept 12 seeds and **passed with the fix reverted** (recorded below as the failed
first attempt at the C5 negation), because no schedule put a spawned interloper into the narrow
window between the flip's resolve and its commit. `FlipLoserStore` makes the first batch that puts
`inode:5` lose deterministically — a version bump applied underneath it, which is exactly what a
concurrent `PutObject` leaves — so the retry path always runs.

## 2. The individual findings

51 union'd findings dedupe to 26 distinct ones. **24 are fixed**; 2 are recorded-rejected in
`review-rejected.md` (the `core`-side fan-out timeout, which the brief's own prior-art check names
as already-rejected, and the *placement-length* half of the owned-entry check, which `0016:416-432`
settles against a strict check — its structural half **is** fixed). Highlights worth the human's
eye, because each was a real defect rather than a style point:

| Finding | What it actually was | Fix |
|---|---|---|
| fingerprint framing | `"{n}:{d}\n"` over **unvalidated client digests**, so `[(1,"A"),(2,"B")]` and `[(1,"A\n2:B")]` collide — a tombstone answering *success* to a different assembly | length-prefixed, injective for any input |
| abort read-then-fence | a part commit only *reads* `mpu:`, so it and the fence both commit; the frozen list omits it and its records **outlive the session** — the session-less part leak a rejected attempt shipped | `RetirePayload::Session {}` carries no list; the drain enumerates the range the fence has made immutable |
| indeterminate create | a re-drive found its **own** landed session and called it an id collision, re-minting and double-charging admission | byte-equality adoption |
| slot replay | no way to adopt a landed slot, so a replay leaked one of 16 | `resume: Option<&str>` + slot **values** scanned for the attempt id |
| compensation outcome | `EntityTooLarge` returned "clean" while the slot, entries and bytes stayed — the accumulation that turns a usable session into a permanent 503 | `compensate_or_defer` (one epoch chase, then the teardown owns it) |
| dangling dirent | fabricated an empty version-1 inode and published version 2 over it | `ChunkMapError::DanglingDirent` |
| one clock per lifecycle | `clock_source` was recorded and never compared (the doc comment claimed X106 while nothing enforced it) | `clock_skew` at the three points a new lifecycle stamp enters |
| per-chunk clock | every chunk reused the reservation instant, so each slot rewrite was byte-identical and the lease never extended — a long, progressing upload was fenceable as idle | `clock: &mut impl FnMut() -> u64`, read per chunk |
| over-budget first unit | the "commit at least one unit" exception could emit a whole part's fan-out over the envelope | cursor gained `frag`: the indivisible unit is now **one fragment mark** |
| `seggrp:` after the obligation | a crash between the two commits leaked the marker with nothing left naming it | same batch — and adoption is judged **against that batch's own deletes**, which is what the failing test caught |
| losing Complete | answered `NoSuchUpload` even when the winner had published or released | bounded fence loop + `tombstone_answer` |
| flat root | `metadata::encode(&root)` bypassed the inode write boundary | `encode_inode` + the value ceiling |
| sweep credulity | every `part:` record counted as "safely staged" without checking its session exists — masking precisely the residue the teardown tests exist to detect | `session_less_records`, and `is_sound()` includes it |

Two fixes went beyond their finding because the finding was the symptom:

* **the drain's stall detector** is now the obligation's **bytes alone**. It used to also require the
  work counters to be idle — and with the three-arm guard a non-advancing walk *does* work (it
  re-writes the same marks), so the old detector would have seen a busy but stuck walk as healthy.
  This is why negation G2 hung on my first attempt and is a clean red now.
* **`retire:` decoding** cross-checks payload variant against key mode (`RetirePayload::is_valid_for`)
  and rejects a marking phase on a records-mode obligation.

## 3. T5 — the three-arm orphan guard, without the value-schema change

The carry-forward asks for "event-keyed three-arm orphan restamping **or equivalent reader-grace
proof**". 0016 wants the identity *in the mark's value* (`{orphaned_at_millis, event}`), but writing
that from `core` today would make `gc.rs`'s bare-`u64` parse drop those entries
(`gc.rs:315-330`) — the fragment becomes unreclaimable — and 0016 assigns the dual-format decoder to
the custodian slice. So the identity here is **structural**, and the proof is that the
same-identity arm is *unreachable*:

* a `retire:`-driven step only ever visits positions **at or after the obligation's own cursor**, and
  that cursor advances in the **same** CAS batch as the marks — so a position this obligation has
  already marked is never re-derived;
* the owned-`sidx:` walk writes a position's marks in the **same batch that deletes the entry naming
  it**, so a marked position has no entry left to re-derive it from;
* in both, the second concurrent drainer (X56) loses the batch **whole** and re-reads, so a position
  is marked **exactly once per unreference event** and no live grace clock is ever re-stamped — which
  is what the existing `two_concurrent_drainers_over_one_owned_range_are_exactly_once` DST case
  measures (committed puts per key, still exactly one).

What is left is the two arms that *are* reachable, and they are now both implemented
(`mark_fragment`): **absent ⇒ `require_absent` + stamp now**; **present ⇒ exact-value
`require(orphan == prior)` + a fresh stamp**, because a mark this event has not written predates the
reference this event is removing. That closes the reader-grace hole T5 names, and it is exactly the
guard `0016:667`/`:671` tabulate. Bound by
`a_stale_orphan_mark_is_restamped_by_the_event_that_unreferences_it` (negation below).

**Residual, declared:** on a `CommitUnknownResult` a re-derived step re-stamps once more. Bounded by
retries, and in the safe direction (later, never earlier).

## 4. Demonstrated red — the negation runs (**no gate consumes these**)

`build-notes.md` is withheld from the reviewer and no `[[gates.checks]]` row reads it, so these are
**sign-off evidence for the human**. Please read the recorded output rather than treating the
C4-verify PASS as proof. Logs are left in `$PDCA_SCRATCH` as `pdca-builder-636-neg-*.log`. Every
negation was reverted and the file verified byte-identical to its pre-negation copy (`diff -q`).

### The brief's three mandatory legs

**Leg F — collapse the two retry budgets into one** (`MAX_ADMISSION_CAS_ATTEMPTS: 64 → 4`):
```
test f_concurrent_creates_on_an_empty_store_all_succeed ... FAILED
8 concurrent creates against an EMPTY store: one was refused with
SlowDown { pressure: AdmissionContention { attempts: 4 } }. The admission ledger is nowhere near
its bound — this is the false `503 SlowDown` ... 4 of 8 succeeded.
```

**Leg G1 — truncate derivation while declaring the obligation drained** (`finished = true` on a
full batch):
```
test g_a_4001_part_complete_drains_to_empty_in_byte_budgeted_batches ... FAILED
8,002 record deletes under a 200-operation budget must take more than 40 batches, saw 1
```

**Leg G2 — drop the drain cursor** (`cursor: None`):
```
test g_a_partially_drained_teardown_converges_with_no_double_decrement ... FAILED
called `Result::unwrap()` on an `Err` value: DrainStalled { obligation: "retire:bytes:s:…:1" }
```
*First attempt at this negation HUNG* — see §2: the old stall detector required the work counters
to be idle, and a re-deriving walk now writes marks. That is how the detector got strengthened, and
the run above is with the strengthened one.

**Leg E1 — skip the terminal-delete decrement** / **E2 — apply it twice**:
```
E1: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 2   right: 1
E2: assertion `left == right` failed: the decrement happens in the terminal delete
      left: 0   right: 1
```

### This iteration's own fixes, negated the same way

**C3a — inline for every flat generation** (`generation_retires_inline` → always true):
```
test an_ordinary_overwrite_of_a_large_generation_retires_it_through_an_obligation ... FAILED
the fan-out is the drain's, not the request's
```

**C3b — restore `unlink`'s segmented refusal**:
```
test deleting_a_segmented_object_commits_and_drains_its_segments ... FAILED
called `Result::unwrap()` on an `Err` value: SegmentedRetirementUnsupported { operation: "unlink" }
```

**T5 — back to the unconditional "present means skip"**:
```
test a_stale_orphan_mark_is_restamped_by_the_event_that_unreferences_it ... FAILED
assertion `left == right` failed: a mark predating the event that removed the reference must be
re-stamped with that event's instant, not left at 1
  left: 1   right: 1000002
```

**Abort race — freeze the part list before the fence** (the shape iteration 1 shipped):
```
test a_part_landing_in_the_abort_fence_window_is_still_reclaimed ... FAILED
the racing part's bytes must be evidenced for reclamation
```

**C5 / T3 — retry from the fence-time snapshot** (the defect the carry-forward named):
```
test a_segmented_publication_that_loses_the_root_flip_still_publishes ... FAILED
the retry after a single lost flip must publish, got Refused(OperationAborted)
```
and with the release-outcome fix also reverted, the second half fires:
```
assertion `left != right` failed: a lost root flip must not strand a segmented publication in
Completing (outcome: Refused(OperationAborted))
  left: Some(Completing)   right: Some(Completing)
```
**The honest part of this record:** the *first* version of this DST case — a 12-seed sweep, no
forcing — **passed** against that same negation. A schedule-dependent race is not a regression test.
The case now forces the loss deterministically.

### The C4-verify RED leg, measured honestly

Unchanged from iteration 1, and re-stated because it is a declared limitation: the red is
**criterion-absence**. With production reverted and the two added test files kept, the failures are
build errors (`unresolved import wyrd_core::multipart`, `no field 'owner' on PendingEntry`), so
**0 tests ran and 0 tests failed**. `run-verify.sh` scores a build failure as a red without ever
counting tests (`engine/scripts/run-verify.sh:416-427`'s guard sits inside the cargo-*succeeded*
branch). **Stated plainly: the C4-verify red is a build failure, not a flipped assertion** — the
load-bearing evidence is the ten negation runs above.

### The DST cases ran, and how many seeds

`crates/dst/tests/concurrency.rs` keeps its `#![cfg(madsim)]`. Under `cargo xtask ci` → `run_dst`
(`--cfg madsim`, `MADSIM_TEST_NUM=50`) the file reports **12 tests where the base had 5**
(`/var/tmp/pdca/pdca-builder-636-ci9.log`), including
`a_segmented_publication_that_loses_the_root_flip_still_publishes`. Each `#[madsim::test]` is
multiplied across **50** seeds (`xtask/src/main.rs:1575-1614`); case (i) is a `#[test]` that sweeps
**48 seeds explicitly** and asserts both orders were reached, so its coverage cannot go vacuous.

## 5. Before declaring done — the three forced questions

**(a) Genuine red? YES.** Ten negation runs above, each reverted and re-verified green; two of them
(G2's hang, C5's first version) *changed the patch* because they proved the first attempt was not
binding. Plus the C4-verify-shaped red, measured and labelled a build error with 0 tests run.

**(b) Production path? YES.** The tests drive `wyrd_core::multipart`'s real verbs, the real
`MetadataStore` seam, the real `PlacementChunkStore`, `write::stage_intent`, `metadata`'s real
committers (including `unlink` and `commit_chunk_map_superseding`, which the C3 legs exercise
directly) and #635's `SegmentedPublication`. Read-back is `read::read_path`. Nothing is stubbed
except the *caller*, which is the test instead of #508's S3 handler. The DST legs run over both
backends the campaign pins the trait with (real redb, simulated TiKV).

**(c) Fixture includes the fault? YES**, and this round added three fixtures whose whole point is
that they include it:
* the abort-race leg **injects the racing commit** in the window immediately before the fence
  (`MemMeta::inject_before_next_commit`) and asserts on the state that commit leaves — it does not
  curate the part out;
* the T5 leg **plants the ancient mark on a live fragment** (0016's rollout-skew population) rather
  than testing a clean store;
* the C5 leg **forces the flip to lose** rather than sweeping seeds and hoping.
Plus iteration 1's five: leg B keeps part 2 staged, leg D observes `sidx:` mid-flight, leg H stages
a part that crashes mid-stream, leg F's store yields so the CAS contention is genuinely produced,
leg G stages 4,001 real parts through the production verb.

## 6. What I still deliberately did NOT do

**(i) The `orphan:` value is still the bare decimal.** The three-arm *guard* is implemented (§3);
the structured `{orphaned_at_millis, event}` value, its dual-format decode, the `reclaiming`
variant and the identity-keyed `protects()` lookup are `gc.rs`'s — #637's file, and 0016 assigns
them there ("What the implementing slices change"). This is a scope boundary, and §3 explains why
the arm I implemented is complete **without** the value change rather than a partial version of it.

**(ii) `reconcile_step`'s signature and `GcContext`'s fields are untouched** (#637's), as the brief
requires. The classification sweep takes the fleet inventory as an argument for exactly that reason.

**(iii) `Completed` sessions are not terminally deleted here** — `W_tombstone` is #625's, and leg E
asserts the opposite of the struck carry-forward item (count unchanged, tombstone still answers an
identical retry).

**(iv) The retirement obligation record family MOVED to `crates/core/src/metadata.rs`**
(`RetireMode`, `RetireToken`, `retire_key`, `PartNumberSet`, `SegmentGeneration`, `RetirePayload`,
`DrainPhase`, `RetireCursor`, `RetireObligation`), re-exported from `multipart.rs` so every import
path is unchanged. This is a consequence of C3, not a preference: the ordinary object committers in
`metadata.rs` now install obligations, and `metadata.rs` has **no** `use crate::` line at all — it is
the base layer. The alternative was `metadata.rs` depending on `multipart.rs` (a module cycle for a
record class the base layer writes), which is the drift `orphan_key`'s own doc comment warns
against. The drain, the state machine and the parsers stayed in `multipart.rs`.

## 7. Gates

* `cargo xtask ci` — **green** (`/var/tmp/pdca/pdca-builder-636-ci9.log`), including the prose gates
  (`typos`, `lint_docs`, `render_site --check`), so the docs-currency edits to
  `docs/design/architecture/{06,08}` are genuinely gated here rather than warn-skipped. Both
  external dependencies the brief declared (`typos`, `docs-renderer`) are installed on this host;
  nothing else was needed and no NEEDS-HUMAN dependency arose.
* `cargo fmt --all -- --check` — clean, so the target's commit hook will not reject the patch.
* `cargo clippy --workspace --all-targets` (and the `--cfg madsim` DST pass) — clean under
  `warnings = "deny"` + `clippy::all = "deny"`.
* `git apply --check patch.diff` on a pristine `d15d555` worktree — applies cleanly.
* Test counts: `wyrd-core --lib` 129 (was 111 on the base, 4 new multipart unit tests + the 3
  rewritten metadata oracles), `multipart_protocol` **13** (7 + 6 new), `multipart_admission_and_drain`
  5, `dst/concurrency` **12** (5 base + 7).

## 8. Self-review against the target's standing rubric (`AGENTS.md:122-210`)

* *One clock per lifecycle* — now enforced, not just recorded (`clock_skew`); `core` still reads no
  clock, every instant arrives as an argument, and the drain bounds **work**, never time.
* *Metadata validation boundaries* — decode-time structural checks widened (session table, retire
  mode/phase, owned-entry shape); placement length deliberately left liberal, with 0016's own
  citation (see `review-rejected.md`).
* *Serialization identity* — `RetireCursor::frag` is `skip_serializing_if`, with a byte-identity
  round-trip test on a legacy cursor; `PendingEntry`'s two fields keep theirs.
* *Absent or unsupported entries* — five silent-skip/silent-success paths became explicit errors
  (`RetireUnitAbsent`, `AdmissionLedgerMissing`, `DanglingDirent`, `OwnedEntryMalformed`, the
  malformed `desired:dserver:` key).
* *Transactions* — `CommitUnknownResult` outranking `Conflict` is the store's; every early return
  over staged work now routes through a compensation whose **outcome is inspected**.
* *Test fidelity* — the new destructive/concurrent path lands with a seeded Tier-0 DST case, and the
  DST legs run on both backends.
* *Docs currency* — `06-runtime-view.md` and `08-crosscutting-concepts.md` updated **in this PR**,
  including the two sentences this round made stale (the old "never rewritten" evidence rule, and
  the scope of the timing change).

## 9. Scratch

Everything under `$PDCA_SCRATCH` (`/var/tmp/pdca`), named `pdca-builder-636-*`: nine gate logs, ten
negation logs, three module backups and two extracted code fragments. The apply-check worktree was
removed. No `/tmp` use. **The negation logs are deliberately left in place** so the human can read
the recorded output at §9 of sign-off; delete them once recorded.

## 10. Verification trail (paste-ready)

```
./engine/xtask.sh ci                                          -> EXIT=0 (ci9.log)
cargo fmt --all -- --check                                    -> clean
cargo test -p wyrd-core --lib                                 -> 129 passed
cargo test -p wyrd-core --test multipart_protocol             -> 13 passed
cargo test -p wyrd-core --test multipart_admission_and_drain  -> 5 passed
RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test concurrency -> 12 passed
git apply --check patch.diff on a pristine d15d555            -> applies cleanly
```
