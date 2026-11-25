# Build notes — #1061 core,custodian: retirement drain, records mode (636.6a-1)

Base: `c27cdb0` (`pdca-integration/main-r9dd123c9ed70`, the fold of `main` with #1008, #1009,
#1010, #1011, #1029, #1030). Every `path:line` below is on that base with this patch applied.
The brief's anchors were verified on `db1a066`; the ones that moved are re-located in the table at
the end.

## What the change does

Three parts, as the brief's SELF-TEST asks: the codec in core, the walk and step in custodian, the
dispatch in the deployed loop.

### 1. Core: the cursor inside `RetirePayload`, and the room for it

- `RetireComponent` (`crates/core/src/multipart.rs:3920`) and `RetireCursor`
  (`multipart.rs:3999`): one position in the payload's own ordered work — component, entry, item.
  Spelling: `"cursor":{"component":"parts","entry":17}`; `item` is omitted when zero, the whole
  cursor is omitted when absent (`multipart.rs:4187`, serialized last so it is one suffix of the
  value). The type's doc carries the per-component table of what an entry means and what bound
  decode checks.
- Decode: the wire struct gains `cursor` (`multipart.rs:4105`), `TryFrom` carries it
  (`multipart.rs:4502`), and `checked_against_key` ends with `checked_cursor(mode)`
  (`multipart.rs:4438`, the check itself at `multipart.rs:4250`). Refusals are four new typed
  `RecordError` variants (`multipart.rs:639` onward, Display beside the others): component not
  carried; entry outside the listed work (a part number that is not a member — past the last,
  between runs, or before the first; an index at or past a `{chunks}` or flat `{generation}`
  list); entry past the key grammar (`MAX_SEGMENT_INDEX` for `{seg}`/segmented generation,
  `[1, MAX_PART_NUMBER]` for `{session}`/`{parts: all}`); an item under `retire:records:`.
- `RetirePayload::advanced(mode, cursor)` (`multipart.rs:4238`): the only way to build a payload
  with a new cursor, and it runs the same `checked_cursor`, so the drain cannot write a position
  its own decode would refuse. No other constructor was added; the five `RetireObligation` rows
  set `cursor: None`.
- `RETIRE_CURSOR_MAX_BYTES` (`multipart.rs:4065`) = 69, spelled as the literal widest cursor
  (`,"cursor":{"component":"generation","entry":999999,"item":4294967295}`), with compile-time
  checks that every entry bound fits six digits. `RETIRE_VALUE_CEILING` (`multipart.rs:4079`) =
  `MAX_VALUE_BYTES - 69` = 99,931.
- The Complete's flip check (#1011) now measures against `RETIRE_VALUE_CEILING`
  (`crates/core/src/multipart/complete.rs:1200-1208`, the `.find` at `:1203`) and reports it as `limit`. That is "the
  ceiling installed obligations are held to". The other installers (`{session, all}`, `{seg}`,
  `{chunks}`, flat `{generation}`) cannot approach it: their values are fixed fields or a chunk
  list one value already held at V/2 (`metadata.rs:329-352`).
- `PartNumberSet::members_from` and `contains` (`multipart.rs:3517`, `:3529`): one
  `partition_point` binary search over the runs, then members read lazily. This is the brief's
  defect (3), the O(N²) rescan, removed. The `runs()` doc that said "no member iterator" is
  updated.

### 2. Custodian: the retirement leg (`crates/custodian/src/retirement.rs`, new)

Map: constants `:95-124`, `Hold` `:139`, the deferral markers `:184-188`, `reconcile` (the walk)
`:195`, `visit` (decode, step, conflict) `:279`, `plan` (component, ceiling check) `:352`,
`parts_step` `:431`, `seg_step` (incl. the whole-range finish read) `:476`.

- **Walk.** Resume point in `custodian:retire-cursor` (`LEDGER_CURSOR_KEY`), the last visited
  key or empty for the head, blind put only when it moves (the GC `ORPHAN_CURSOR_KEY` pattern,
  `gc.rs:279-291`, `:1949-1960`). A resume value that is not a `retire:` key, or is longer than
  FDB's 10 KB key limit, is named on the audit seam and the walk restarts at the head.
- **Bounds** (all `pub const`, so the test reads them): `VISITS_PER_PASS` 64, `LEDGER_PAGE` 16,
  `LEDGER_PAGES_PER_PASS` 5, `STEPS_PER_PASS` 16, `READS_PER_STEP` 3, `READS_PER_PASS` 54,
  `COMMITS_PER_PASS` 17. Every obligation read costs a visit, whatever happens to it. Pages ask
  for `min(visits_left, 16)`, so the visit counter, not the page size, is what stops the walk —
  which is what makes the brief's "held not counted" mutant observable.
- **Step.** One step per visit, one component per step, payload order (`parts` then `seg`).
  `{parts}`: members from the cursor via `members_from`, 249 per batch (`(500 − 2) / 2`), deletes
  `part:` + `psum:`. `{seg}`: page of the group's range from the cursor, 498 per batch; when the
  page from the cursor is the last one and the cursor is not at 0, one more one-row read of the
  whole range from its start — records below the cursor are a hold, not a finish. Batch budget:
  `B = min(ops, bytes)` with the fixed part charged at its largest (require: key + prior; put: key
  + a value at the ceiling). Every batch is `require(retire:… == prior)` + deletes + put back the
  advanced value, or delete the obligation in the batch with its last record.
- **Holds** (`Hold`, typed, with `as_str` for the audit field): `undecodable`,
  `value-past-ceiling` (the advanced value is measured before sending and never sent over
  `MAX_VALUE_BYTES`), `segment-cursor-past-records`, `malformed-segment-key`, `cursor-refused`
  (unreachable by construction; kept instead of a panic), `commit-failed`. Each: obligation
  untouched, `warn!` on `wyrd.custodian.retirement.audit` with `action = retire-obligation-held`,
  a counter, the pass answers `Blocked`, the walk continues.
- **Conflict.** Re-read the obligation, re-plan from what is there (another step from the
  budget); gone means another drainer finished it, stop.
- **Bytes mode.** Visited (costs a visit), left byte-identical, counted on
  `retirement_bytes_obligations_deferred`. It does not withhold `Satisfied` (reasoning below).
- `// deferred: #1062` (DST row) and `// deferred: #660` (concurrent drainers) markers sit above
  `reconcile`, per the brief's "No DST row in this child".

### 3. Dispatch

`reconcile_step` runs `retirement::reconcile(ctx)` right after `reconstruction::reconcile` when a
reconstruction context is given (`crates/custodian/src/reconciliation.rs:167`). No new parameter,
so none of the ~25 callers change, and GC-only passes still read no `retire:` range
(`gc_reclaim_intent.rs` leg D passes in the full suite). The deployed loop's reconstruction pass
(`crates/server/src/custodian.rs:604-615`, through `reconcile_pass` at `:397`) therefore runs it
every interval. Two visibility changes support it: `gc::checked_page` is `pub(crate)`
(`crates/custodian/src/gc.rs:2067`), `Reconciled::least_certified` is `pub(crate)`
(`reconciliation.rs:76`).

### Docs, deps

- `docs/design/architecture/06-runtime-view.md:88` (new paragraph in the reclaim step: the drain,
  its per-pass bounds, the resume record, the cursor, records mode never marks, the whole-range
  `{seg}` finish, holds, the reserved room, the downgrade note).
- `docs/design/architecture/05-building-block-view.md:246` (the sentence that said the drain had
  not arrived now says the custodian consumes `retire:records:` and names the cursor field).
- `crates/custodian/Cargo.toml` dev-dependency `wyrd-metadata-redb` and its one `Cargo.lock`
  line (approved in the brief).

## Tests

- `crates/custodian/tests/multipart_retirement_drain.rs` (new, 13 tests): B-rec, C ×3 (sparse
  `{parts}` of 4,001 runs; `{seg}` of 1,200 records; combined `{parts, seg}` crossing at a commit
  boundary), L1, L2, W, G-rec, K-`{seg}` hold, and four H legs (commit refused, precondition lost
  and re-planned, damaged resume point, stray key in a segment range). Every pass goes through
  `reconcile_step` with a fresh `MemCoordination` + `FencedZone` + `ReconstructionContext` and
  every other input `None` — the shape `reconcile_pass` passes. Store: `RedbMetadataStore::
  in_memory()` behind a recording wrapper (reads, ledger entries returned, every batch sent; an
  optional FDB-style refusal of puts over `MAX_VALUE_BYTES`; an optional "other writer" batch; an
  optional per-key commit failure). Audit events are captured by a subscriber attached to each
  pass's future (`WithSubscriber`), no statics.
- `crates/core/tests/multipart_retire_cursor.rs` (new, 6 tests): K (unchanged bytes for every
  row plus the two teardown installers, round trip incl. `advanced`, every typed refusal from
  decode and from `advanced`, non-canonical spellings, the 69-byte room) and `members_from`.
- `crates/core/tests/multipart_complete_value_ceiling.rs` (#1011's, modified): its I3/I4 legs sat
  between 99,931 and 100,000 bytes, which the lowered ceiling now refuses. Re-pointed: inside
  99,921 (odd to 15,859), past 99,935 (odd to 15,861; bare set 99,925 still inside), at exactly
  99,931 (6,245 six-digit numbers), and a new leg: #1011's old "exactly 100,000 publishes" set is
  now refused `SessionTooLarge { bytes: 100_000, limit: 99_931 }` with zero commits — the brief's
  (W) Complete refusal "where #1011 refuses one over the ceiling". I4 uses odd to 15,859 + part
  15,861.

## Demonstrated reds (each edit temporary; file restored and re-checked after)

Run with `cargo test -p <crate> --test <file> -- <filter>` under a `timeout`, through a helper
that applies the edit, runs, and restores the file in a `finally`.

| Temporary edit | Leg run | Result |
|---|---|---|
| dispatch wrapped in `if black_box(false)` (`reconciliation.rs`) | B-rec, C | B-rec FAILED ("the records obligation drains": never does); all three C legs FAILED; G-rec FAILED too |
| `plan` ignores the stored cursor (each step from the start) | C | sparse `{parts}` FAILED at "the stored value changes every step" (step 2 re-derives step 1's batch and puts the same value back), combined FAILED. `{seg}` leg passes — see below |
| obligation deleted after its first batch (`value` forced `None`) | C, G-rec | all C legs FAILED; G-rec FAILED at the restore assertion: `reconcile_after_restore` names the session in `segments_unaccounted` after step 1 |
| `{parts}` finishing does not move on to `seg` (obligation deleted with the first component) | C combined | combined FAILED ("the obligation outlives its parts"); the others pass |
| `{seg}` finish from the cursor alone (whole-range read skipped) | K-`{seg}` | FAILED (obligation deleted with 10 records below the cursor) |
| `steps_left -= 1` → `+= 1` | L1 | FAILED on the commit bound |
| held (undecodable) obligations give their visit back | L2 | FAILED at the visit bound: "80 obligations visited" (5 pages × 16, the page cap, against 64) |
| walk always starts at the head | L2 | FAILED: the productive obligation never drains in 4 passes |
| ceiling check removed | W | FAILED: an over-ceiling put is sent (and the hold reads `commit-failed`, not `value-past-ceiling`) |
| extra: Complete check back on `MAX_VALUE_BYTES` | #1011 test, new I3 leg | FAILED (100,000-byte set publishes) |
| extra: `checked_cursor` not called from decode | core K refusals | FAILED |

**The `{seg}` sub-leg under "cursor not stored".** It stays green, and that is correct rather than
a gap: deleted `seg:` records vanish, so a read from the start of the group lands exactly where the
cursor says. The `{seg}` cursor is not what makes that component converge; what it protects is the
finish decision, which the K-`{seg}` leg pins (red 5), and the stored value advancing every step,
which the `{seg}` leg asserts. The `{parts}` component is the one where a cursorless drain
re-derives the same keys forever (the #636 defect), and that leg goes red.

**G-rec reorder.** My first version checked "obligation present iff records remain" before calling
`reconcile_after_restore`, so red 3 failed on my own assertion. I moved the restore call first so
the red shows the production symptom (the session named for segments with no deleter).

## Refuting my own test

- **(a) Genuine red?** Yes. Every leg was run red with its fix reverted (table above). Beyond the
  table: on the bare base the whole custodian test file cannot compile (no `retirement` module,
  no redb dev-dependency), which is why C4-verify will report UNVERIFIABLE (exit 77), accepted by
  the human on 2026-11-11 per the brief's verification posture.
- **(b) Production path?** Yes. The drain is reached only through `wyrd_custodian::reconcile_step`
  with a reconstruction context, as `reconcile_pass` calls it. B-rec's obligations are installed
  by the production `create_session` + `complete_upload`, its object read back through the
  production `read_object`; G-rec's obligation is installed by the production
  `reconcile_after_restore` fence and re-checked by it after every step; the codec legs call the
  production `decode_retire_obligation` and `RetirePayload::advanced`. Nothing re-implements the
  drain.
- **(c) Fixture includes the fault?** Yes. W's wrapper actually refuses over-ceiling puts (the
  same constant FDB enforces) and the over-ceiling obligation is present and sorts first; L2's
  held prefix is 192 real undecodable values ahead of the productive one; K-`{seg}` plants real
  records below the planted cursor; H plants a real refused commit, a real concurrent write, a
  damaged resume record and a stray key. No fixture curates the failing element out.

## Choices and what I ruled out

- **Bytes-mode visits answer `Satisfied`, not `Partial`/`Blocked`.** `Blocked` claims a fault
  where there is none; `Partial` claims "the next pass resumes where this one stopped", which is
  not true of an obligation no build drains yet, and a caller driving until satisfied would spin
  on both until #1062. The leg certifies the records-mode ledger only (doc on `reconcile`), and
  the visits are counted on a metric so a backlog is not silent. A reviewer may still call this a
  silent skip; the answer is the brief's scope ("leaves each byte-identical") plus the counter.
- **Commit errors are contained (hold `commit-failed`), not propagated with `?` as GC does.**
  Propagating would end the pass before the resume point moves, so a batch the store refuses
  for good would pin the walk on that obligation every pass — exactly the starvation the
  invariant forbids. Reads still propagate (a ledger read failing is not one obligation's fault).
  An unknown outcome is safe to contain: the step is a single CAS on the exact prior bytes, so the
  next visit's re-read shows whether it landed and plans from there.
- **One step per obligation per visit**, rather than looping on one obligation until the step
  budget runs out. Cost: a single large obligation drains one batch per pass — 4,001 sparse parts
  is 17 passes, a 10,000-part sparse set 41 passes, at the deployed interval. Benefit: one
  obligation cannot take a whole pass's budget from the rest, and G-rec can observe each step.
  Looping would be a ~6-line change in `visit` if throughput matters later.
- **Pages of 16 under a visit counter of 64**, instead of one page of 64. Memory per read is
  1.6 MB instead of 6.4 MB at the value ceiling, and the visit counter becomes the thing that
  bounds the walk (with one page of 64 the counter could be deleted without any observable
  change, and the brief's L2 red would not be expressible).
- **One component per step.** Makes every component boundary a commit boundary, which is what
  the combined leg needs to resume "exactly at that boundary"; costs at most one extra step per
  obligation.
- **Resume record outside `retire:`** (`custodian:retire-cursor`), not a field in any obligation:
  0016's "no cursor record class" (`0016:612-619`) binds the per-obligation drain cursor, which is
  inside the obligation; the walk position is the brief's GC-style bookmark, stated as the choice.
- **Not shrinking the part set as it drains** (rewriting `{parts}` without the drained members):
  0016 and the brief put progress in a cursor; shrinking would also rewrite up to 100 KB per step
  exactly as the cursor put does, so it saves nothing.
- **Did not dispatch on GC's input** (brief: breaks `gc_reclaim_intent.rs` leg D).
- **No new `reconcile_step` parameter** (brief: ~25 callers).

## Brief anchors re-located on `c27cdb0`

| Brief | Now |
|---|---|
| `complete.rs:598` `deferred: #1011` | #1011 landed: `PartRetirement::past_ceiling`, `complete.rs:1200` (now against `RETIRE_VALUE_CEILING`) |
| `multipart.rs:3904` `RetirePayload` | `multipart.rs:4171` (struct), wire `:4093` |
| `multipart.rs:4175` `decode_retire_obligation` | `multipart.rs:4540` |
| `reconciliation.rs:126-168` | `reconciliation.rs:128-180` |
| `gc.rs:2066` `checked_page` | `gc.rs:2067` |
| `restore.rs:597`, `:1152-1190`, `:1156` | unchanged |

## Gates run

- `cargo xtask ci` through `engine/xtask.sh` on the worktree: **`xtask ci: all checks passed`,
  exit 0** — typos (typos-cli 1.48, ran), `lint_docs` and `render_site --check` (ran, link audit
  OK), the repo guards, fmt, clippy, build, `cargo test --workspace --exclude wyrd-dst`,
  cargo-machete, cargo-deny (incl. the all-features advisories run: `advisories ok`, so no
  NEEDS-HUMAN for it), conformance, statics (ADR-0035), deploy-guard, and the madsim DST sweep.
- That run was on the tree before the last edits: four doc-comment rewraps (`complete.rs`
  module doc and `past_ceiling` doc, a memory note on `seg_step`) and the four H legs plus their
  header bullet in the custodian test. After them I re-ran `cargo fmt --all -- --check`,
  `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` (clean), `typos` on every touched
  file (clean), the whole `cargo test -p wyrd-custodian` (33 binaries ok, the drain file 13/13),
  and the four core multipart test files the patch touches or depends on (ok). Rustdoc on the two
  crates shows no warning in any line this patch adds (the base already has unrelated ones).
- The four H legs each went red with their handling removed (commit error propagated instead of
  held; Conflict re-read but never re-planned; resume value not checked for the `retire:`
  prefix; stray segment key skipped instead of held). A first attempt at the re-plan red did not
  compile (an unused variable under `warnings = "deny"`) and proved nothing; I redid it with an
  edit that compiles.
- `patch.diff` applies to `c27cdb0` (`git apply --cached --check`) and its reverse matches the
  worktree.

NEEDS-HUMAN: none from Do. C4-verify's UNVERIFIABLE (exit 77) is the pre-accepted sign-off item
from the brief, not a new one.
