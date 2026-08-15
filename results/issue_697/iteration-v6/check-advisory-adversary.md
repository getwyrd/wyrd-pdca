# Adversarial review — issue 697 (advisory, non-gating)

Inputs: `patch.diff`, `brief.md`, `check-gates.json`. All `path:line` below are on the
target at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `339da46`).
Red→green was re-run independently in a throwaway copy under `$PDCA_SCRATCH` (removed).

## Evidence re-run (what I could and could not break)

I reproduced the asserted proof rather than taking it: with `reconstruction.rs` reverted
to `origin/main` and the new test kept, **5 of 6 legs fail behaviourally** —
`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` for legs 1–4
and a decode error for leg 5 — and leg 6 passes, exactly as the brief declares. With the
patch, 6/6 green. No compile failure in the red leg, so the red is behavioural, not
UNVERIFIABLE. `C4-verify`'s phrase *"6 test(s) ran red"* reads as *ran*, not *failed*
(only 5 fail pre-fix); harness wording, not a patch defect.

## Findings

- **NEEDS-HUMAN [human] — the test file is ~1.6×/1.9× over the budget whose overrun the brief
  pre-declared as a STOP.** `crates/custodian/tests/segmented_map_reconstruction.rs:718` —
  the file is **718 raw** lines and **520** non-blank/non-comment lines against
  `brief.md` §Budget's `≤ 280 semantic / 460 raw`, whose own trigger says *"a test file past
  460 raw means the shape is wrong: **STOP and hand back rather than finish**"*. The
  production side is inside budget (121 added semantic ≤ 160), so this is squarely the
  compression rules §Budget spelled out ("ONE `BTreeMap`-backed metadata double … ONE
  parameterised seeding helper … ONE capture helper") not having bought what they were
  budgeted to buy. A verdict that scored C3/T1/T2 pass without adjudicating this accepted a
  bundle whose own plan says to hand back; the human decides whether to accept the overrun
  or send it back for compression.

- **NEEDS-HUMAN [impl] — the two new operator counters have no oracle at all; deleting both
  emissions leaves every test green.** `crates/custodian/src/reconstruction.rs:973`
  (`monotonic_counter.reconstruction_unresolvable_records`) and `:992`
  (`monotonic_counter.reconstruction_refused_records`). Demonstrated, not suspected: I
  removed both `tracing::warn!(monotonic_counter…)` lines and re-ran — all 6 legs of
  `segmented_map_reconstruction` **and the whole `wyrd-custodian` suite** stay green.
  `brief.md` §Scope pins these two counters as half of the added audit vocabulary and says
  *"each MUST be asserted by a leg above (an unasserted label is a finding waiting to
  happen)"*; legs 2/3/5 assert only the `action` strings via
  `rows()` (`tests/segmented_map_reconstruction.rs:530`, `:589`, `:690`). Mutation testing
  does not cover the gap either — `C5`'s log marks `replace emit_unresolvable with ()` and
  `replace emit_refused with ()` **unviable**, so they were never executed. Concrete fix:
  assert `logged.contains("reconstruction_refused_records")` in leg 2 and
  `…_unresolvable_records` in leg 3.

- **NEEDS-HUMAN [impl] — `reconcile`'s own rustdoc still documents only two outcomes after the
  patch taught it a third.** `crates/custodian/src/reconstruction.rs:138-141` still reads
  *"Returns [`Reconciled::Changed`] if any chunk's placement record was repointed,
  [`Reconciled::Satisfied`] otherwise"*, while the patched body returns
  `Reconciled::Blocked` at `:327` on two distinct conditions. Both merged peers this slice
  is told to mirror document it in exactly this doc block — `gc.rs:135-139`
  ("Returns [`Reconciled::Blocked`] if the reference set is **incomplete**…") and
  `scrub.rs:72-75`. This is **not** the #701 carve-out: that defers
  `reconciliation.rs:25-28` and *other files'* docs; `reconstruction.rs` is one of the two
  files this bundle owns, and the repo rubric's *Docs currency* MUST applies to the file
  under change. Four lines.

- **NEEDS-HUMAN [impl] — the "one resolve per segmented object, not per obligation" half of the
  O(N) claim has no oracle.** Leg 4 (`tests/segmented_map_reconstruction.rs:632`) measures
  `seg_pages` on a fixture whose single segmented object owns **zero** queued chunks, and
  leg 2 (`:497-543`) — the only leg with Q=2 obligations *inside* one segmented object —
  never calls `meta.counts()` at all. So an implementation that resolved a segmented
  object once per **owed chunk** rather than once per object would pass all six legs while
  restoring the Q×N shape on exactly the store the refusal path exists for. The current
  code is right (I measured `(inode_scans, seg_pages) == (1, 1)` under leg 2's fixture), so
  this is a missing assertion, not a bug: `assert_eq!(meta.counts(), (1, 1))` in leg 2 shuts
  it for one line — and `brief.md` leg 4 states the bound as *"the resolver's `seg:` reads
  are ≤ S"*, which leg 4 alone cannot distinguish from "≤ Q".

- **`C5-mutants`' `fail` row is an environment artifact, not diff coverage debt — do not
  rebuild for it.** The single non-green mutant is
  `crates/custodian/src/reconstruction.rs:440:25: replace != with ==` (the
  `record.state != InodeState::Committed` guard in `read_committed`), recorded as a
  *timeout*. I applied that exact mutation by hand and ran the crate: it is killed in under
  0.1 s by **14** tests in `crates/custodian/tests/reconstruction.rs` (e.g.
  `kills_a_d_server_and_reconstructs_to_full_redundancy_through_reconcile_step`, `Satisfied`
  vs `Changed`). Zero missed mutants; the timeout is host contention.

## Attempted and could not refute

- **An index/list mismatch in the new `Site::Flat` shortcut.** `assess` indexes
  `site.chunks[site.index]` (`reconstruction.rs:562`) where `index` comes from iterating
  `resolved.chunks` (`:476`) but `chunks` is cloned from `record.chunk_map`
  (`:466`). Chased it into the resolver: a `ChunkMap::Flat` snapshot returns
  `Resolution::Answer(Cow::Borrowed(chunks))` of that same record and can never restart
  (`crates/core/src/metadata.rs:2585`), and a `Segmented` snapshot yields `flat == None`
  and indexes nothing (`:471`). The two are provably the same slice; no panic, no
  cross-generation write.
- **`Blocked` masking `Changed` when a repair did land.** `reconcile:318` returns `Blocked`
  even with `changed == true`. Checked the only consumer: `crates/server/src/custodian.rs:530-545`
  discards the value (`Ok(_) => {}`), and `least_certified` (`reconciliation.rs:56-61`)
  already ranks `Blocked` over `Changed` by design. No behaviour rides on it.
- **First-committed-reference-in-key-order changing under the rewrite.** The old
  `find_chunk` skipped a record whose key would not parse and continued the scan; the new
  walk does the same (`reconstruction.rs:470`), and `:480`'s `sites.contains_key` guard
  preserves first-wins. Deliberately did not pursue duplicate-`ChunkId` behaviour — carved
  out to **#700** by the brief.
- **A drain surviving an incomplete reading, or a refusal wrongly suppressing one.** Built a
  mixed fixture the suite does not have (one segmented object with **two** owed chunks
  **plus** a landing flat repair **plus** an obligation nothing references): answer
  `Blocked`, one `refused-segmented` row, the flat placement moved to `[0, 2]`, the
  unreferenced obligation drained, both refused obligations still queued. And with an
  undecodable record added: `Blocked`, the unreferenced obligation **kept**. Both correct.
- **A same-object double obligation losing its CAS and stranding the rebuilt fragment
  un-orphan-marked.** Reproduced it (two owed chunks in one flat record → one repair lands,
  the second returns `Conflict` and its destination fragment carries no `orphan:` mark), then
  ran the identical probe against `origin/main` and got **byte-identical** behaviour.
  Pre-existing debt this diff neither created nor touched; not filed.
- **Over-containment of a merely-hot object.** `ChunkMapError::MapResolutionUnstable` (an
  object under sustained overwrite, not a corrupt one) is contained at
  `reconstruction.rs:449` as "this object is unreadable", naming it `unresolvable-chunk-map`
  and blocking the pass. Real, but it is exactly `gc.rs:402-416`'s rule, already shipped
  identically in three merged peers, and `brief.md` §Scope pins *"containment is by exactly
  gc.rs's downcast rule and no other"* — record-reject territory, not this bundle's defect.
