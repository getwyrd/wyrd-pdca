# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked the evidence, the fix and the verdict against the patched tree at `$PDCA_TARGET`
(base `9120f7a`). Four findings; the rest of the refutation attempts failed and are listed
at the end.

- **NEEDS-HUMAN [human]** — `crates/core/src/metadata.rs:5237` (`high_water_marks`) — **the
  chunk-id floor this round grew ~320 lines of new production code to compute is read by
  nothing, and the hazard it cites cannot occur on this tree.** The one caller,
  `crates/server/src/lib.rs:124`, binds it to `_max_chunk` and throws it away; no other
  non-test caller exists (`rg high_water_marks` → `server/src/lib.rs:124` plus doc prose).
  Worse, the id space it recovers is unmintable: `Gateway::mint_chunk_id`
  (`crates/server/src/lib.rs:238-240`) always yields ids ≥ 2^127 and the cluster minter
  `chunk_id_minter` (`crates/server/src/cli.rs:1716-1722`) always yields ≥ 2^64, so **no
  live path mints a `< 2^64` chunk id at all**. The code's stated justification — "a floor
  below an id whose fragments are on disk lets the allocator re-mint that id and clobber
  them (issue #364)" (`metadata.rs:4762-4769`) and "a floor below a live id costs an object
  its bytes" (`metadata.rs:5227-5228`) — is unwarranted, and it even cites
  `crates/server/src/lib.rs:123-124`, the exact line that discards the value. Concretely
  unwarranted claims that follow: leg A(vii)(a)'s `max_chunk >= in_segments` assertion
  (`crates/custodian/tests/segmented_map_consumers.rs:1207-1213`) tests a number no caller
  consumes, and the brief's containment-table row "it must **never under-approximate** the
  floor" is unfalsifiable in production. This is the carried §6 item "unreachable
  `max_chunk` cost", which iteration 8's sign-off asked to be resolved or declined *"not
  silent carry"* — it has since driven three review rounds (the escaped-leading-zero lexer,
  the truncated-prefix `2^64` boundary, the id-floor scan cost) and it produces **8 of the
  12 surviving mutants** in `check-gates.json` (`metadata.rs:4825/4829/4835/4847/4848/5033
  ×2/5056`, all inside `json_chunk_id_floor` / `json_chunk_id` / `json_string_token`). Only
  the **totality** half of the containment row is real (`recover` does `?` the call); the
  over-approximation half is buying nothing. A human must decide whether to keep growing
  this surface, shrink it to totality-only, or gate it behind a real consumer.

- **NEEDS-HUMAN [human]** — `crates/custodian/src/gc.rs:268-272` +
  `crates/core/src/metadata.rs:2315-2317` — **one damaged segmented object halts *all* GC
  reclamation fleet-wide, for ever, and the product offers no way to remove it.**
  `ReferenceSet::protects` returns `true` for *every* `(dserver, fragment)` in the fleet
  while `unresolvable` is non-empty (`gc.rs:269`), and it is the sole deletion gate
  (`gc.rs:162`) and the sole marking gate (`restore.rs:222`). Meanwhile the only in-product
  delete path, `Gateway::delete_object` → `metadata::unlink`
  (`crates/server/src/lib.rs:582`), refuses a segmented map outright
  (`metadata.rs:2313-2318`, `SegmentedRetirementUnsupported`), as do both superseding
  committers (`metadata.rs:2356-2360`, `:2426-2431`). Concrete case: take leg A(vii)'s own
  fixture — `DAMAGED_INODE` (root names a `seg:` record that was never written) — and then
  DELETE any *other*, healthy object. Its orphan grace records are laid, the grace window
  elapses, and GC skips every fragment on every pass for ever; the store leaks
  monotonically, and the damaged object cannot be deleted, overwritten or repaired by any
  API in the tree. The brief's containment table authorised "continuing while treating **the
  damaged object** as fully referenced"; the patch treats **the whole store** as fully
  referenced, and the composition with the (separately defensible) segmented-retirement
  refusal leaves the failure with no exit. The blast-radius trade this slice re-planned to
  avoid has moved rather than gone: from gateway availability to unbounded, unremediable
  storage leak. Needs a scope call — a bounded/attributed degradation, an escape hatch, or
  an explicit tracked deferral.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:163-168` — **the containment's own
  "attribute the blocker" rule is broken at the per-fragment audit seam.** When
  `unresolvable` is non-empty, `protects` short-circuits at `gc.rs:269` *before* `placed` or
  `malformed` are consulted, but the reason computed at `gc.rs:163-167` has only two arms,
  so every skipped fragment — including genuinely orphaned, past-grace fragments of
  *healthy* objects — is emitted as `emit_skip(dserver, frag, "referenced")`. Concrete
  failing case: seed the leg A(vii) store, DELETE a flat object, advance past `GRACE`, run
  `gc::reconcile`; the audit trail says its fragments are `referenced` when they are not —
  the operator is told the wrong cause on the one seam that explains why a fragment
  survived, and `emit_unresolvable` (which names the real blocker) fires once per damaged
  object, not per skip. Add an `"unresolvable-map"` reason arm and a test asserting it;
  today no test pins the skip reason for this path.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:307` and `:339` — **the documented
  boundary "anything that is *not* the object's own fault still propagates"
  (`gc.rs:283-291`) has no oracle.** Both match guards
  `err.downcast_ref::<ChunkMapError>().is_some()` survive mutation to `true`
  (`check-gates.json` C5 rows), i.e. the suite cannot tell containment from
  swallow-everything. Concrete failing case the tests would not catch: a `MetadataStore`
  double whose `get` of the inode root returns `Err` — that store fault reaches `:339`
  through `resolve::chunks_of` → `resolve_current_chunk_map` (`metadata.rs:3016`), and under
  the mutant it is reclassified as "one object unresolvable", so `referenced_fragments`
  returns `Ok` with an incomplete set and `reconciliation_status` answers
  `PendingUnresolvable` instead of surfacing the store failure — a silent
  skip of an indeterminate read, the rubric's *Absent or unsupported entries* class. Same
  for `:307` with a legacy **flat** record whose bytes fail serde (a non-`ChunkMapError`),
  which the doc says must abort the pass. Two doubles (failing `get`; corrupt flat record)
  and two `expect_err` assertions close both.

## Refutation attempts that failed

- **The red→green evidence.** I could not break it. Every symbol
  `crates/custodian/tests/segmented_map_consumers.rs` names is base-visible (checked its
  import list at `:56-74` against `git show HEAD:crates/custodian/src/lib.rs:23-44` and
  `crates/core`'s `read`/`write`/`metadata` surface); it seeds roots and segments as **raw
  bytes** in the settled encoding (`:266-311`) and names none of the types this slice adds
  (`rg 'ChunkMap|Segment\w+::|resolve_chunk|PendingUnresolvable'` over the file → no hits),
  so the RED should be assertion-shaped rather than the build error the brief warns falls
  through to an unconditional PASS. The assertions are specific enough not to pass for the
  wrong reason on the base: `:1112` requires the backfill error to *contain* `"segmented
  chunk map"`, which the base's `invalid type: map, expected a sequence` serde error cannot
  satisfy even though `expect_err` alone would. Caveat: I did **not** re-run
  `run-verify.sh` myself — the only warm build cache (87 GB) lives inside the read-only
  target and a scratch rebuild would be from zero — so this leg rests on inspection plus
  the C4-verify row, not on an independent execution.
- **Resolver ordering.** `read_segments` orders by the *parsed* index through a `BTreeMap`
  (`metadata.rs:2836-2853`), and a genuinely reversing store double that asserts it actually
  shuffled exists (`metadata.rs:8981-9075`) — the "must not rely on `scan` order" rule holds.
- **Prior rounds' blocking findings.** Iteration 10's two are closed:
  `read_group_range` now refuses `accounted > MAX_ROOT_SEGMENTS` rather than trusting the
  root's `segment_count` (`metadata.rs:2768-2773`), and `plan_with` refuses an empty
  placement before anything is durable (`metadata.rs:3577-3582`). Iteration 11's headline
  fence-ABA finding is closed too: the cycle rule now spans the durable prefix, the segment
  phase **and** the flip (`metadata.rs:3739-3747`, `:3990-3998`). I tried to construct an
  `A → B → A` that escapes it across a resumed prefix and could not — the adjacent-repeat
  skip at `:4547` is sound because one batch's put is the next batch's pin, and I verified
  the surviving mutant at `:3859` (`+`→`*` on `written`) is behaviourally *equivalent* under
  that dedup, so it is not a real gap.
- **Legacy byte identity.** `ChunkMap::Serialize` delegates straight to `Vec<ChunkRef>`
  (`metadata.rs:1382-1389`), field order and the three `skip_serializing_if` attrs are
  unchanged (`:1725-1763`), and `try_from = "InodeRecordWire"` touches only `Deserialize` —
  I could not find a flat record whose decode→encode moves a byte.
- **The ranged gateway leg** genuinely crosses a segment boundary rather than mirroring it:
  8×8-byte chunks split 4/4, range 28–39 straddling byte 32
  (`crates/server/src/lib.rs:1222-1250`), asserted against the payload slice, on the real
  `get_object_range`.
