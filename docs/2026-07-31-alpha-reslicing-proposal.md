# 0.1 Alpha re-slicing proposal — the multipart stack in review-sized slices

**Date:** 2026-07-31. Companion to `docs/2026-07-31-oversized-slices-report.md` (the analysis this is derived from).
**Applies to:** getwyrd/wyrd issues #635, #636, #637, #508 (+ sequencing of #625/#633); #634 is merged (PR #645), #638 is accepted and only needs publishing.

**Filed 2026-07-31** as getwyrd/wyrd issues (parents stay open as seam trackers):
635.1–635.6 → **#648 #649 #650 #651 #652 #653** · 636.1–636.7 → **#654 #655 #656 #657 #658 #659 #660** · 637.1–637.5 → **#661 #662 #663 #664 #665** · 508.1–508.6 → **#666 #667 #668 #669 #670 #671**.

> **2026-08-02:** `wave_mode` changed `"stack"` → `"merge"` mid-batch. The base-discipline
> and sequencing rules below were written for `"stack"` and two of them no longer apply as
> written — see the **Addendum** at the end of this document before writing or running any
> remaining slice brief.

## Budget and rules (manual guardrails until pdca-harness 0.56)

Derived from this instance's own convergence data (report §3: <100 KB patches converge in 2–3 rounds; ≥250 KB take 6–18 or die):

- **Per slice: ≤ ~1,500 added semantic lines, ≤ 15 files, one concern.** Prefer one crate plus its tests; cross-crate only when the files are the concern's direct surface (e.g. a read path). Mechanical migration ripple is counted separately and allowed on top.
- **Caller-first:** no slice lands a producer whose consumer doesn't exist. Behavior *flips* (routing, denylist removal) land in the slice that completes the loop, never earlier.
- **Scaffolding separates:** conformance suites/test harnesses land before or beside the code they judge, as their own slice.
- **Test pruning is part of extraction:** when a slice is carved out of an existing oversized patch, it takes the *binding* test legs for its criteria, not every test body the big patch accumulated — report §4(b) shows criteria-compounding was half the problem.
- **Base discipline:** every brief names its base ref (previous slice's branch under `wave_mode = "stack"`); Do refuses to build if that base is absent from the sandbox rather than guessing — #635 lost its first five rounds exactly here (report §3).
- **Brief ≤ ~15 KB.** If the brief needs an N-row consumer table, plan ~N/3 slices, not 1.
- **Sign-off default for an over-budget patch is iterate-to-Plan**, not another Do round.

A salvage note before the slices: **the code is not lost.** 635's closed PR #647 and 636's discontinued `patch.diff` are correct-ish, CI-green material. Re-slicing here means *splitting and hardening existing diffs* (plus fixing the specific open findings, each named below in the slice that owns it) — mostly `git`-level extraction work per slice, not 19k lines of re-derivation. Line figures below are measured added *semantic* lines (non-blank, non-comment) in the corresponding regions of #647's diff.

---

## #635 — segmented chunk map (PR #647 closed; split into 6)

| Slice | Content | Source in #647's diff | Size notes |
|---|---|---|---|
| **635.1 record shape** | `ChunkMap::Flat \| Segmented` + decode-time invariants (raw-byte negative cases), `seg:`/`seggrp:` key helpers, `SegmentRecord`, byte-for-byte legacy compat + CAS round-trip tests; the record-shape paragraph of the living-arch docs (the resolver/publication paragraphs move to the slices that land that behavior) | `metadata.rs` shape+codec region + its inline tests | ~1,200 |
| **635.2 resolver + read paths** | the resolver implementation — which lives in **`metadata.rs`** (`MapResolution`, `read_group_range`, `resolve_chunk_map/_current/_live`, ~390 prod; `custodian/src/resolve.rs` is only a thin wrapper, 137), `core/src/read.rs`, gateway whole-object + ranged read (`server/src/lib.rs`), shuffling-store double, second-group-not-read proof, resolve-retry rule; the DST resolver-tear prop + its crash/restart harness | metadata.rs resolver region, resolve.rs, read.rs, server read portion, `dst/tests/custodian.rs` (first prop) | measures ~1,900 with #647's full test bodies — prune co-located resolver tests to the binding cases; if still over, split read-paths out. **Rework, not reuse:** `read.rs`'s co-located test seeds its object via the deferred committer — rewrite it to seed raw `seg:` records (the server tests already do) |
| **635.3 custodian consumers A: GC + scrub** | reference build via resolver (`ReferenceSet.unresolvable`), bounded `seg:` ranges; per-object fail-closed containment; `reconciliation.rs` `Reconciled::Blocked`; **fixes the open adversary finding**: with one unresolvable object, `protects()` short-circuits and GC audits *every* fleet fragment as `referenced` yet returns `Satisfied` (`gc.rs:163-214`) — the scrub twin was fixed in #647, the GC side was not | gc.rs + scrub portions + their test files + shared `segmented_map_consumers.rs` fixture | ~800 |
| **635.4 custodian consumers B: restore + reconstruction + backfill + rebalance** | same containment rule on the remaining passes (+`desired_state.rs`); deliberate segmented-record handling in backfill; the ~55-line `check_value_ceiling`/`check_record_ceilings` helpers `repoint_chunk` needs (carved out of the committer region); the central `reconcile_step` leg of `segmented_map_consumers.rs` (needs A and B, so it ships here) + the DST repoint-vs-supersede prop; **fixes the open perf finding**: `find_chunk`'s per-queued-chunk root re-read (Q×N point reads, `reconstruction.rs:646/:676`). **Depends on 635.3** (restore calls `gc::referenced_fragments` / gates on `protects`) — serial, not parallel | the four pass files + desired_state + their test files + DST prop | prod is small (~280); #647's full test files push this to ~2,000 — prune to the per-pass binding legs |
| **635.5 chunk-id floor, shrunk** | Replace the floor apparatus (`metadata.rs:5063-5624`: `RecoveredIds`/`ClassIds`/`torn_digit_escape`/`json_string_token`…) with the minimum totality requires: flat-map max + bounded `seg:` range max, total over damage, never under-approximating; **wire `max_chunk` to its caller or delete it** (today `server/src/lib.rs:124` discards it); **fixes the open adversary finding**: corrupted flat root classed `Optional` → floor 0. Its containment leg lives in `segmented_map_consumers.rs`, so it lands after 635.4 | rewrite, mostly deletion vs #647 | ~300 |
| **635.6 staged-publication committer** — **deferred to the 636 wave** | fence/rollback/resume machinery, precondition-as-parameter; the staged-publication doc paragraph; the DST publication-atomicity prop. Its only producer is #636's Complete; landing it now repeats the "apparatus with no caller" failure. | metadata.rs committer region (~730 prod) + tests | #647's full committer tests measure ~3,700–4,100 — far over budget. Take the flip-atomicity and resume-verification legs only; if still over, split 635.6a (committer + flip atomicity) / 635.6b (rollback-resume + DST prop) |

Order: 635.1 → 635.2 → 635.3 → 635.4 → 635.5, with 635.6 landing immediately before 636.5.

## #636 — multipart protocol (discontinued; split into 7, along the seams sign-off named)

| Slice | Content | Notes |
|---|---|---|
| **636.1 records + state machine, pure** | `mpuctl`/`mpu:`/`slot:`/`part:`+`psum:`/`sidx:` **and the `retire:bytes:{generation}` / `retire:records:` record types** (types + key grammar here; routing/drain *behavior* is 636.6's), validating decoders (fixed-width, canonical — the `+7`/`007` class), typed outcomes, decision-3 verb×state answer table as **pure functions**, `complete_fingerprint`, ETag composition (`multipart_etag`) with its oracle tests | no store I/O; the pure core of `multipart.rs`. 637.2/637.4 build against these types |
| **636.2 knobs + derivations** | the ~31 `MAX_*`/window/backoff constants with derivations in doc comments, `knob_clamps_hold`, `Budget` | pure; fold into 636.1 only if the pair stays ≤ budget |
| **636.3 admission + create/abort** | serialized admission CAS **with the CAS-contention retry separated from the id-collision retry** (jittered backoff, real bound — the carried-forward false-503 at concurrency 8–16), create/abort semantics, concurrent-create tests at 8/10/16. Abort tears the fence only; the admission decrement stays with the terminal delete (636.6) and a `Completed` tombstone **stays counted** (the corrected leg E) | first store-I/O slice |
| **636.4 UploadPart staging** | the per-part `slot:<id>:<k>` in-flight keyspace (claim by `require_absent`, release by keyed delete in the owning batch), reserve/stage/commit/renew; `write.rs` staged placement against `Topology::excluding(draining)` with the `require_absent(desired:dserver:)` fence **plus the test that writes a `desired:dserver:` key and stages a part** (zero-coverage finding — both fence mutants currently survive the whole suite), same-batch slot renewal, `NoSuchUpload`-vs-draining-fleet fix (a draining fleet must re-plan, not 404 an `Open` session) | |
| **636.5 Complete + publication** | fenced publish via 635.6 (landed immediately before), segmented map production, tombstone answers, Complete-retry semantics **without the 3-typo permanent wedge** (attempts budget must not wedge an `Open` session), subset-Complete orphan evidence | the first real producer of segmented maps |
| **636.6 retirement routing + drain + terminal delete** | `retire:` routing for supersede/`unlink`, byte-budgeted `drain_step`/`drain_obligation` (converges past 4,001 parts; single-oversized-entry progress), `reclaim_owned` (validated `StagedPlacement` — the RS(0,0) marks-nothing bug), terminal delete + exactly-once `mpuctl` decrement + slot-range teardown. **Hard rule: the `unlink`/supersede routing flip ships in the same slice as the drain's production dispatch** (from `reconcile_step`, coordinated with #637) — the discontinued run's fatal regression was precisely this flip landing with a drain that had "callers only in tests," silently ending space reclamation for ordinary deletes | caller-first rule, applied |
| **636.7 seeded DST races** | the three 0016-required cases (publication CAS loss, slot-reserve at `MAX_INFLIGHT_PARTS`, two concurrent drainers) | scaffolding slice over 636.3–.6 |

## #637 — staged-byte protection (not yet built; split the brief before Do runs)

Note: #637's brief declares a load-bearing conflict with #625 (#625 widens `reconcile_step`'s signature; **637 builds first, 625 builds on it**) — all 637 slices precede #625.

- **637.1 GC paged ledger walk** — `orphan_leases` + mark sweep on `scan_page` with a **bounded per-pass page budget** (the design call the issue reserves; no unbounded `loop { scan_page }` into a HashMap). Depends only on merged #634 — schedule early, but **after #638's patch is published/merged** (it touches the same custodian test files).
- **637.2 GC staged references + reclamation ordering** — `ReferenceSet` staged set (bounded per-session ranges), `protects(retire:bytes:)`, `reclaiming`-before-`delete_fragment` ordering, three `orphan:` value variants, fragment-less mark sweep. Needs 636.1's record types only.
- **637.3 scrub + reconstruction** — verify staged fragments, enqueue repair, repair updates part placement.
- **637.4 rebalance + desired_state + restore** — staged set disjointness, in-flight-owned counted as held, restore session fencing + the `retire:`-installing restore-fence generation record #508 consumes (needs 636.1's `retire:` types — available well before this wave).
- **637.5 seeded DST races** — drain-request-vs-intent fence, staged-replace-vs-session-fence.

## #508 — S3 wire surface (after the protocol exists; 6 slices)

1. **508.1** `MultipartGateway` companion trait + composition-root implementation, all verbs answering typed NotImplemented; routing untouched.
2. **508.2** routing/decoding hardening while the denylist still stands: canonical decoding of `uploads`/`uploadId`/`partNumber` in every percent-spelling (#491), duplicate-param refusal, tests proving non-evadability — the 501 default is *proven* unchanged before anything is exposed.
3. **508.3** Create + Abort + ListParts + ListMultipartUploads (denylist entries removed per-verb here), exact status+code cells.
4. **508.4** UploadPart (+ explicit `UploadPartCopy` 501, not a silent 0-byte part; signed-payload integrity check retained; partNumber range validation, no `+1` parsing).
5. **508.5** Complete: XML grammar tolerant of `Checksum*` children (aws-cli v2 default), fingerprint retry surface, recorded-ETag replay, reaper-absent operator signal (once, at startup — not per-request).
6. **508.6** PutObject chunk-size selection, `400 EntityTooLarge`, lengthless `aws-chunked` sizing with the config-load precondition.

**Merge gate unchanged:** #625 and #633 land with or before 508.3 (the first verb that creates sessions). Their briefs declare the order **636+637 → #625 → #633** (#625 depends on 636 and 637; #633 depends on 625 and 636), which the waves below respect. Apply the same slice budget at their Plan pass — #625 in particular should split (reaper pass ∥ window/cursor drain ∥ alarms) if its brief exceeds budget.

## Sequencing

Waves hold at most **two builder bundles** (the driver's `lanes = 2`); `publish #638` is the deterministic publish step, not a builder lane. No two same-wave slices touch the same files; each chained slice names the previous slice's branch as its base (`wave_mode = "stack"` — stacked PRs, merged bottom-up by the maintainer, so wave cadence is merge-gated).

| Wave | Lane A | Lane B | Notes |
|---|---|---|---|
| 1 | 635.1 | *(publish #638)* | |
| 2 | 635.2 | 637.1 | 637.1 after #638 merges (shared custodian test files); disjoint from 635.2 |
| 3 | 635.3 | 636.1 → 636.2 | a dependency chain occupies one lane and the driver waves the pair (636.2 is its own bundle; fold it into 636.1 only if the pair stays under budget) |
| 4 | 635.4 | 636.3 | 635.4 needs 635.3 (restore→ReferenceSet) |
| 5 | 635.5 | 636.4 | floor's containment leg needs 635.4's test file |
| 6 | 635.6 → 636.5 | 637.2 → 637.3 | the committer and its first caller as one stacked pair in one lane — #653 is never merged without #658 stacked behind it (caller-first holds at the merge boundary); 637.2 needs only 636.1's types, 637.3 chains on it |
| 7 | 636.6 | 637.4 | 636.6's dispatch coordinates with 637.2 (landed wave 6) |
| 8 | 636.7 | 637.5 | |
| 9 | #625 | *(independent item)* | after all of 636+637, per its brief |
| 10 | #633 | 508.1 | #633 builds on #625, and **#625 + #633 merge as one unit** (#633 is issue-titled "ships with #625"); neither is load-bearing until #668 exposes the verbs, which requires both |
| 11+ | 508.2 → 508.3 → 508.4 → 508.5 → 508.6 | *(independent items)* | serial — all edit `gateway-s3`; #668 only after the #625/#633 unit is in |

Independent 0.1-Alpha items (#511 bucket ops, #512 conformance harness, #560 lease renewal, #585 docker env, #596 gitlink) are already inside the budget individually and fill Lane B from wave 9 on (or any earlier idle lane); #512 runs after 508.5 to exercise multipart end-to-end.

~24 slices instead of 5 looks like more process, but the arithmetic from the report favors it decisively. The five big slices consumed **37 builder rounds** (634: 6, 635: 18, 636: 4, 638: 9, 637: 0) for one merged PR, one PR closed unmerged, one discontinuation, one accepted-but-unpublished patch and one unstarted brief — and because the escalation ladder pins every round after the first to the top builder tier, ~29 of those 37 rounds ran at maximum cost over up-to-1 MB diffs. At the historical small-slice median of 2 rounds, ~24 slices ≈ 48 *converging* rounds, most of them first-attempt-tier, on ~20× smaller review units — each of which the review, mutation and adversary machinery can actually cover, and each of which the maintainer can review in one sitting instead of confronting another 19,000-line PR.

---

## Addendum (2026-08-02) — running the remainder of this batch under `wave_mode = "merge"`

`wave_mode` switched `"stack"` → `"merge"` (getwyrd/wyrd-pdca#198) after the stale-stacked-PR
incident on #648–#650's PRs (wyrd #675/#676: the fold-rebuilt `pdca-integration/main` left
open stacked PRs DCO-red and CONFLICTING). Under `"merge"` the driver merges each
**non-final** wave's PRs into the real base before the next wave builds. Three of this
plan's rules change with it:

- **Base discipline, restated for `"merge"`:** every remaining brief names **`main`** as its
  "Repo + branch target" base — never the previous slice's branch, and never a `Stacks on:`
  declaration. The dependency is carried by wave ORDER (the predecessor is genuinely merged
  into `main` before the dependent builds), not by branch wiring. A brief that still names a
  predecessor's `fix/*` branch would (a) build against a branch that stops advancing after
  its PR merges, and (b) open — and in merge mode, MERGE — its PR into that branch instead
  of `main`, silently leaving the slice out of the release base. **Audit the already-written
  briefs for unstarted slices (651+) for this before the next run.**
- **Atomic merge pairs:** "merged bottom-up by the maintainer" no longer holds the pairs
  together — the driver merges every completed non-final wave at its boundary. The two
  affected constraints:
  - *Same-wave pairs* still hold: both members merge back-to-back at one wave boundary.
  - *Chained pairs* — **635.6→636.5 (#653 "never merged without #658") and #625→#633
    ("merge as one unit")** — cannot: the consumer builds on the merged producer, so the
    producer lands on `main` a full build-cycle earlier. Either **fold each chained pair
    into a single bundle** (accepting the budget overrun; the committer + first caller as
    one brief) or **pull the pair out of the driver** and run it as a hand-stacked pair of
    PRs the maintainer merges together. Decide per pair at its Plan pass; the driver has no
    per-bundle mode override.
- **Merge gating:** `gh pr merge` fails closed only on *required* checks, and wyrd `main`
  currently requires only `docs-check` / `require-issue` / `docs-immutability` — **not
  `rust`/`gate`**. Harness-side C4-ci runs the same `cargo xtask ci` before accept, but the
  host-only gates (CodeQL, dco) would not block an auto-merge. Before the first merge-mode
  batch, add `rust` and `gate` (and `dco`) to `main`'s required status checks so a non-final
  wave can never merge red.
