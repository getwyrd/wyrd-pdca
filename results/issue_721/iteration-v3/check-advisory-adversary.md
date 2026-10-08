# Adversarial review — issue #721 (segmented-repair-completes-through-repoint)

Advisory only; I never gate. Inputs: `patch.diff`, `brief.md`, `check-gates.json`, `gate-logs/`.
Every citation is grounded on the target source at `$PDCA_TARGET` (working tree = patch applied on
`cbd8b19 pre-fix base a801997`). The red→green claim was adjudicated from the frozen
`gate-logs/C4-verify.log` (#403), not re-run: it shows both discriminating legs failing against
reverted production with real assertion diffs (`Blocked` vs `Changed`) at
`crates/custodian/tests/segmented_map_repoint.rs:429` and `:493`, and 7/7 green with the fix. I
could not make that evidence tautological — the legs name no symbol the patch introduces, drive the
real `reconcile_step`, and read the store back through `metadata::decode`.

## Findings

- **NEEDS-HUMAN [human] — the segmented arm's `V/2` ceiling re-creates the exact refused-forever
  state this issue exists to remove, for a record band the read side explicitly admits.**
  `segment_value_ceiling_crossed` refuses any re-encoded `seg:` record above `MAX_ROOT_VALUE_BYTES`
  (`crates/core/src/metadata.rs:397`), while the resolver keeps and answers from stored `seg:` rows
  all the way to `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:2528`). Concrete failing case: a
  `seg:` record of 60 000 bytes — readable, resolvable, no anomaly — holding an under-replicated
  chunk. `repoint_chunk` answers `Repoint::Refused` on every pass, `reconcile` sets
  `ceiling_refused` and answers `Blocked` (`crates/custodian/src/reconstruction.rs:334`, `:362`),
  the obligation is never drained, and nothing in the tree shrinks the record. That is a state with
  no actor exiting it in bounded time — the C-1 shape named in the brief's *Invariant to restore*,
  relocated from "all segmented objects" to "segmented objects with a 50–100 KB segment". The
  brief's premise for choosing `V/2` — "a conforming publication never writes a `seg:` value above
  `V/2`" — is **not enforced anywhere in this tree**: `MAX_SEG_CHUNKS` has no definition
  (`crates/core/tests/multipart_budget_admission.rs:337` says so in as many words),
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1465` assigns both the knob and its
  enforcement to **#508**, and `SegmentRecord::new` (`crates/core/src/metadata.rs:1170`) caps
  nothing. Mitigating, and why this is a sign-off question rather than a build defect: this build
  ships no producer of segmented maps, so the band is unreachable in production today, and the brief
  says "flip it at sign-off if you disagree". What the human should know before deciding is that the
  choice is now **test-locked** — leg 5 (`crates/custodian/tests/segmented_map_repoint.rs:612`) goes
  red if the bound is widened to `MAX_VALUE_BYTES` — and that #508 is the load-bearing prerequisite
  the choice silently depends on.

- **NEEDS-HUMAN [impl] — the repair loop's containment names and counts once per *obligation*,
  dropping the once-per-*object* guarantee the deleted `Reading::refused` set carried.**
  `crates/custodian/src/reconstruction.rs:322` calls `reading.contain(...)` inside `for plan in
  &plans`, and `contain` emits unconditionally (`:406-409` → `emit_unresolvable`, `:1059-1068`).
  Concrete failing case: a segmented object with **two** queued chunks in the **same** `seg:` record,
  torn under the move — `repoint_chunk` returns `SegmentRecordUndecodable` twice, so one damaged
  object produces **two** `reconstruction_unresolvable_records` ticks and two NEEDS-HUMAN audit rows,
  while the identical fault met in `read_committed` (`:504`) produces exactly one. The code this
  patch deleted made the property explicit ("counted and named exactly ONCE PER OBJECT: two
  obligations inside one segmented object are one refusal, not two"). The new test cannot catch it:
  the torn leg (`crates/custodian/tests/segmented_map_repoint.rs:692`) enqueues a single chunk. Fix
  is a per-object dedupe on the containment path, mirroring what was removed.

- **NEEDS-HUMAN [impl] — an unattributable committed object is now a silent *skip* that feeds the
  drain path, and this patch widened that from flat objects to segmented ones.**
  `crates/custodian/src/reconstruction.rs:513-515` replaced the base's shape-gated
  `(Some(_), None) => continue` with an unconditional `let Some(inode_id) = parse_inode_key(&key)
  else { continue };`. That `continue` does **not** set `reading.incomplete`, so the object's chunks
  never enter `reading.sites`, `assess` answers `Assessment::Drain` (`:608`), and — because the
  reading still looks complete — the obligation is **deleted** at `:351`, for a chunk a committed map
  does reference. Concrete failing case: an `inode:`-prefixed row that decodes as a `Committed`
  `InodeRecord` with a **segmented** map under a non-canonical key spelling; on the base its chunks
  became `Site::Refused` and the obligation was *kept*, after this patch they are silently drained.
  That is the rubric's "absent or unsupported entries → never silent skip" class, on a line this diff
  rewrote. #698 is cited in the surrounding comment as owning the *key-spelling* hazard ("read at one
  key and written at another"); it does not own this *drain* consequence, and the fail-closed fix is
  one call — `reading.contain(&key, ...)` instead of the bare `continue`.

- **NEEDS-HUMAN [impl] — two public doc comments link to a private item, so the rendered link is dead
  and rustdoc's `private_intra_doc_links` fires.** `crates/core/src/metadata.rs:375` (doc of the
  `pub fn flat_value_ceiling_crossed`, `:380`) and `crates/core/src/metadata.rs:2768` (doc of the
  public variant `Repoint::Refused`) both link `[segment_value_ceiling_crossed]`, declared private at
  `:397`. Nothing catches it — there is no `cargo doc` step in `cargo xtask ci` nor in
  `.github/workflows/` — so it ships as a broken link in the published API docs. Either widen the
  helper's visibility or de-link it in the two public docs.

- **NEEDS-HUMAN [human] — the gating `T4-batch-review` red is composed entirely of Plan-settled
  deferrals, so iterating Do cannot clear it.** `gate-logs/T4-batch-review.log` reports two blocking
  findings: the missing orphan pre-mark at `crates/custodian/src/reconstruction.rs:908` — the 0016
  X47 pre-mark, which the brief's *Out of scope* assigns to **getwyrd/wyrd#723** and marks "DECIDED
  AT PLAN … do not re-open" — and absent seeded Tier-0 DST coverage at `:897`, which the brief
  assigns to **#722** while forbidding any edit to `crates/dst/tests/custodian.rs`. Both name real
  rubric classes on a surface this diff touches, and both are answered by a tracked deferral, which
  the repo's reviewer protocol treats as settled. A human must decide whether to accept the red
  against those two references or re-scope; a rebuild will reproduce it unchanged.

- **NEEDS-HUMAN [human] — the gating `C4-ci` red is an unrelated supply-chain advisory, not this
  diff.** `gate-logs/C4-ci.log:2847` fails `cargo deny check` on RUSTSEC-2026-0258 (`h2 0.4.15`,
  pulled via `hyper`/`tonic`/`aws-smithy-http-client`, `Cargo.lock:111`). Everything else in the run
  — fmt, clippy, the whole test suite including `placement_ceiling.rs` (`:1222`, 5/5 green) and
  `segmented_map_reconstruction.rs`, machete, conformance — is green on both attempts, and the patch
  touches no manifest or lockfile. The remedy is `cargo update -p h2`, outside the brief's 4-file
  budget. Human call whether to bump here or hold.

## Refutations attempted and failed

Recorded so the next reviewer does not respend them.

- *"The merge/conflict legs never reach the read→prepare window."* They do. `MemMeta::scan_page`
  fires the racing batch **after** materialising the `seg:` page
  (`crates/custodian/tests/segmented_map_repoint.rs:114`), which is strictly between the resolver's
  only `scan_page` (`crates/core/src/metadata.rs:2495`, inside `read_group_range`) and the move's
  only `get` (`:2885`) — the shape the brief demanded, not the parent attempt's inside-`commit()`
  injection. Each racing leg asserts `meta.raced()` as a fixture self-check, so a leg whose race
  never landed fails rather than passing vacuously.
- *"`chunk_at`'s offset addressing can mis-target a neighbour."* It cannot: `read_committed`
  accumulates the offset over the resolved list from 0
  (`crates/custodian/src/reconstruction.rs:535-538`) and `SegmentedMap::new` enforces a contiguous
  tiling from 0 at decode (`crates/core/src/metadata.rs:930`), so the caller's absolute offset and
  the root table's `SegmentRef.byte_offset` cannot disagree. Zero-length chunks, an object naming one
  `ChunkId` twice, and `u64` overflow all fall out correctly (`chunk_at`, `:2938-2950`: `checked_add`
  → `None` → `Conflict`).
- *"The two-precondition batch is a new backend contract."* Multi-key preconditions already exist in
  production (`crates/core/src/metadata.rs:1669`, `:1757-1761`, `:1969`) and every backend iterates
  `batch.preconditions` (`crates/metadata-redb/src/lib.rs:214`, `crates/metadata-fdb/src/lib.rs:1485`,
  `crates/metadata-tikv/src/lib.rs:1386`), with the fault-conformance suite exercising a two-`require`
  batch (`crates/metadata-fault-conformance/src/lib.rs:224-225`).
- *"The resolve's restart path lets a repair write into a non-`Committed` generation."* `read_committed`
  gates on the scanned record (`crates/custodian/src/reconstruction.rs:494`) **and**
  `resolve_current_chunk_map` re-checks `state != Committed → Ok(None)` on every restart
  (`crates/core/src/metadata.rs:2726`), so `Object::prior` is always a committed root.
- *"The flat arm regressed when its ceiling check moved into the primitive."* #710's
  `placement_ceiling.rs` — including the exactly-on-the-ceiling admissible leg and the
  aborted-not-refused precedence leg — is green in `gate-logs/C4-ci.log:1222`. The `MISS` on the flat
  refusal lines in `gate-logs/C4-diff-cov.log:397-400` is an artefact of the diff-cov run's narrower
  test selection (it runs `segmented_map_repoint.rs` plus `wyrd-core`, not the whole custodian suite),
  not unreachable code.
- *"A `Repoint::Conflict` can loop forever while the pass certifies `Satisfied`."* Every non-race
  conflict source is caught on the *next* pass by the resolver instead — the extent mismatch by
  `read_segments` (`crates/core/src/metadata.rs:2617`), an absent or undecodable row by `retired_or`
  — which contains the object and forces `Blocked`. The one non-transient fault the move itself meets
  is raised as a typed error rather than folded into `Conflict`, which is the right call.
- *"`decode→encode` identity of a segmented root is assumed but never tested."* Leg 1 exercises it
  end to end: the repair commits only because `require(inode_key, encode(prior))` matches the
  fixture's stored root bytes, and the leg then asserts the root is byte-identical afterwards
  (`crates/custodian/tests/segmented_map_repoint.rs:458`).

## Reading of the gate rows

`check-gates.json`'s `C4-verify` line — "7 test(s) ran red" — counts tests *executed* in the red leg,
not failures: the log shows **2 of 7** failing, exactly the two the brief nominated as
discriminating. Legs 3–7 pass on the base by construction and are bound by the C5 mutation oracle
(41 mutants, 25 caught, 16 unviable, 0 missed), not by C4-verify. No overclaim — the brief
pre-declared it — but the row must be read as the brief instructs, not at face value.
