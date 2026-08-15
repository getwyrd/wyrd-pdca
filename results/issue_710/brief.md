- **Slug:** ceiling-refused-placement-writes-do-not-certify
- **Defect / goal:** Two write-path defects on the **flat** repair/evacuation path, both
  permanent-state costs C-1 rules out, both red on today's `origin/main` (`339da46`).
  1. **A repair may grow a record past the backend value ceiling, and a record past it is
     permanently un-overwritable.** Every mutation in `crates/core/src/metadata.rs` is
     `require(key, encode(prior))` + `put(key, encode(next))` — a full-value CAS. A record whose
     encoded bytes exceed `MAX_VALUE_BYTES` (100 000, `metadata.rs:327`) is refused by the
     tightest backend on the `put`, and thereafter **every** repair of that object fails. The
     tree already states the invariant at `metadata.rs:334-352` — *"a root that cannot be
     re-written is an object whose placement can never be repaired"* — and then checks it
     nowhere on a write path. Verified: `MAX_VALUE_BYTES` has exactly one behavioural use, the
     resolver's **read**-side refusal at `:2465`; the others are the constant (`:327`), the const
     assertion (`:354`) and doc references. A placement move is a real growth vector —
     `placement: Vec<DServerId>` re-encodes each moved entry, and a small id (`1`) replaced by a
     large one (`18446744073709551615`) adds ~19 bytes per moved entry (only the rebuilt
     positions are re-encoded — the multiplier is however many fragments one repair moves). A
     crossing repair therefore either commits an oversized record (a store without native
     enforcement — thereafter un-overwritable on the tightest backend) or surfaces as a raw
     backend `Err` indistinguishable from a transient fault (a store with it). In neither case
     is there a *refusal* — classified, persisting nothing, distinguishable from a lost CAS:
     capacity spent as durability.
  2. **A move that did not persist still certifies, and can inflate reported successes.** At
     `crates/custodian/src/rebalance.rs:128` the arm is `EvacOutcome::Aborted => {}` — silent —
     and the loop answers `Reconciled::Satisfied` when nothing changed, reporting a drain
     converged while a fragment still sits on the draining server. That tells an operator the box
     is safe to remove. `reconstruction.rs` already carries the offsetting seam from #197
     (`emit_repaired` at `:257` fires before the outcome match, offset by `emit_conflict` /
     `emit_aborted`, documented at `:714-715` as `repaired − conflict − aborted`); the new
     **refused** outcome this child introduces must join that identity rather than inflate it.

  These two defects are one rule — **a move that did not persist neither certifies nor counts** —
  and the weld is structural, not rhetorical: introducing the ceiling refusal *forces* the
  certification/accounting decision, because a `Refused` arm added beside a still-silent
  `Aborted => {}` would re-create defect (2) for the new outcome on the day it is born. Both land
  in the same crate-private outcome `match`es and the same documented accounting seam; a third
  cycle to separate them would buy two colliding PRs over the same arms.
- **Success criterion:** the NEW file `crates/custodian/tests/placement_ceiling.rs` passes,
  driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{inode_key, encode, decode, MAX_VALUE_BYTES, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Three legs:
  1. **A repoint that would cross the value ceiling is refused, not persisted.** Hand-seed a
     committed **flat** root whose encoded length is just under `MAX_VALUE_BYTES`, holding a chunk
     placed on small-id D servers, and arrange a repair whose new placement uses large `u64` ids
     so the re-encoded record crosses the ceiling. Assert: the record is **byte-identical**
     afterwards, the obligation **stays queued**, the pass does **not** answer `Satisfied`, and the
     refusal is named on the audit seam. Base behaviour: the oversized record is committed (the CAS
     has no ceiling check), so `get(inode_key)` returns bytes whose length **exceeds
     `MAX_VALUE_BYTES`** → **red**.
     **Assert the stored byte length, not a downstream un-repairability**: an in-memory
     `MetadataStore` double has no value ceiling and will happily hold the oversized value, so "the
     object is now un-repairable" is *not* observable through it. Do **not** copy the parent
     brief's optional two-phase demo ("commit once, then a second repair fails"): with a
     ceiling-enforcing double the *first* crossing write is already refused by the store, so that
     sequence demonstrates nothing — the two store models cannot share one narrative. The coherent
     supplementary leg, if wanted: over a ceiling-enforcing double, the base's crossing repair
     surfaces as a raw backend `Err` out of `reconcile_step` (unclassified, indistinguishable from
     a transient fault), while post-fix the pass returns cleanly with the obligation queued and
     the refusal named — assert that contrast. The binding assertion either way is the stored
     length over the unlimited double.
  2. **An evacuation that did not persist does not certify.** With `set_lifecycle(.., Draining)`
     on a server holding a fragment, arrange an evacuation that cannot proceed (refused by the
     ceiling, or aborted for want of a free distinct failure domain). Assert the fragment is still
     on the draining server and the pass MUST NOT answer `Satisfied` while `reconciliation_status`
     for that server is not converged. Base: the silent `Aborted => {}` arm lets the loop answer
     `Satisfied` → **red**. This leg **flips a deliberately pinned base assertion**:
     `crates/custodian/tests/rebalance.rs:940-975` seeds exactly this shape (no free distinct
     domain) and asserts `Satisfied` ("no move: collapsing the chunk's spread would violate
     durability"). Rewriting that pin is sanctioned — #696's brief defers the certification
     question to #682 — and is why `tests/rebalance.rs` is this child's **named** fifth file, not
     an option. No new public variant is needed: `Reconciled::Blocked` already exists on base
     (`crates/custodian/src/reconciliation.rs:44`, and it outranks `Changed`), so the
     discriminator asserts `!matches!(.., Satisfied)` with base symbols only.
  3. **A refused move is subtracted, never counted as a success.** Assert the documented
     `repaired − conflict − aborted` identity holds over a pass mixing one repaired, one refused
     and one aborted chunk — a refused repair must not inflate reported successes. The counters
     are observable through in-memory doubles: existing tests already read the
     `monotonic_counter`s back via the telemetry subscriber and compute exactly this identity
     (`crates/custodian/tests/reconstruction.rs:1939-1945`). (This leg is **not** independently
     discriminating: on the base the would-be-refused repair simply commits its oversized record
     and *correctly* counts as a success, so the identity holds there too — it is red only as a
     derivative of leg (1). It ships because it pins the accounting rule for the new outcome —
     the same discount this proposal applies to child-2's leg (3).)
  Legs (1) and (2) are the discriminating reds — both red against the current `origin/main`
  (`339da46`) without #696/#697, and both surviving their merge (#696 defers the `Aborted` arm to
  #682; neither PR adds a ceiling check). Leg (3) is binding but derivative.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly.)
- **Reproduction:** on the target checkout, `git -C ../wyrd grep -n MAX_VALUE_BYTES --
  crates/core/src/metadata.rs` returns the constant (`:327`), the const assertion (`:354`) and the
  resolver's read-side refusal (`:2465`) — **no write path checks it**; and
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` at `:125-130` shows
  `EvacOutcome::Aborted => {}` falling through to `Reconciled::Satisfied`. Seed a committed flat
  root just under 100 000 encoded bytes and run a repair that moves a placement onto large-id D
  servers: the oversized record commits.
- **Scope (one logical fix) / out of scope:** **one rule — a placement write that would not
  survive is refused, and a move that did not persist does not certify.**
  - `crates/core/src/metadata.rs` — the record-ceiling checks: a placement write whose re-encoded
    record would cross the backend value ceiling is **refused and not persisted**, and the refusal
    is distinguishable by the caller from a lost CAS (they mean different things to an obligation:
    one is "never retry this shape", the other is "retry next pass"). Carve out **only** the
    ceiling helpers the write path needs — **not** the committer around them. Enforce the ceiling
    that is actually normative for the record being written (`:324-354`, `MAX_VALUE_BYTES` /
    `MAX_ROOT_VALUE_BYTES` and the `const` assertion tying them); **do not invent a third
    constant**.
  - `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs` — the two
    binding commits check the ceiling before committing, and the new refused outcome joins the
    documented `repaired − conflict − aborted` accounting; the pre-existing silent
    `EvacOutcome::Aborted => {}` arm (`rebalance.rs:128`) is settled here — #696 deliberately left
    it to this work. The refusal path writes **nothing at all**; it does **not** retract bytes
    already published to a D server (settled, rejected 4× in #638,
    `results/issue_638/review-rejected.md:15-16`).
  - Keep the CAS idiom of `commit_chunk_map` (`metadata.rs:1741-1768`): `require(key,
    encode(prior))` + `put(key, encode(next))`, `version = prior.version + 1`, and `..prior.clone()`
    so ADR-0047 object metadata is **preserved** — a placement-maintenance commit must not move
    `Last-Modified` or drop the content type.
  - **Budget:** ≤ **250** added semantic lines (non-blank, non-comment, non-mechanical) —
    child-2's ≤450 makes the pair sum to the parent's ≤700 — and ≤ **5** files:
    `core/src/metadata.rs`, `custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs`,
    `custodian/tests/placement_ceiling.rs` (**new**), and `custodian/tests/rebalance.rs` (the
    pinned `Satisfied` assertion leg (2) rewrites — a **named** cost, not an option). A sixth
    file means the shape is wrong.
  - **Out of scope:** anything segmented — **no** `repoint_chunk`, no `seg:` record addressing, no
    change to `resolve_chunk_map` or any read side, no containment-rule change (#695/#696/#697).
    The committer, the destination pre-mark, the drain fence, rollback and resume (**#653**). The
    duplicate-`ChunkId` one-plan rule and the DST supersede property (child-2). `backfill.rs`
    (**#695**) — a backfill fill that would cross the ceiling is a real gap, but it is a *different*
    write path and belongs to whichever slice owns it next, not to a widened diff here.
    `restore.rs`, `gc.rs`, `scrub.rs`, `desired_state.rs`. Any new or edited ADR / spec / proposal
    (0016 is a **draft** and stays untouched); any conformance-vector change; any new dependency.
  - **Keep the discriminator assertion-red — HARD CONSTRAINT.** The new test MUST NOT name the
    ceiling helpers or any other symbol this patch introduces. The RED leg reverts production, so
    such a reference makes the target fail to **compile** and the gate reports UNVERIFIABLE
    (exit 77, `run-verify.sh:450`, `:500`) rather than a red. Drive everything through `reconcile_step`
    and observe the **store**. `MAX_VALUE_BYTES` is base-visible (`metadata.rs:327`) and may be
    named.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`,
  `cargo-mutants` — the five `[[doctor.checks]]` ids in `pdca.toml` (cite by **id**, not by line — the parent
  brief's line numbers were already stale at review, and the file churns). Named
  because the prose and dependency-wall legs warn-skip locally while CI enforces them
  (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with
  a message naming a flag rather than the stale tool. Nothing else beyond the base Rust toolchain:
  no Docker, no protoc, no live backend, no new dependency.
- **Test file:** `crates/custodian/tests/placement_ceiling.rs` — a **NEW** file, not optional.
  C4-verify earns its red only from an **added** `*/tests/*.rs` (`run-verify.sh:97`, `:98`);
  appending to an existing file makes the gate take the green-only branch (`run-verify.sh:454-464`)
  and prove no red. No `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` on the
  base, so neither zero-test guard trips (`run-verify.sh:445`, `:481`). Re-run
  `run-verify.sh --classify` at Plan to confirm the invocation.
- **Difficulty:** medium
- **Depends on:**
- **Depends on (merged):** 696, 697
- **Conflicts with:** 717
- **Ordering note:** **Wave 1.** Its RED depends on no other child and **not** on #696/#697 —
  the missing ceiling check is orthogonal to the segmented read side — but its **diff** does:
  the accepted, unmerged PRs for #696/#697 rewrite the two custodian passes across the very
  outcome `match`es this child edits (#696's diff annotates the `Aborted => {}` arm
  "deferred: #682"). Hence `Depends on (merged): 696, 697` above: `flow` holds this bundle until
  those PRs are merged into the base (`brief.py` `depends_on_merged`, enforced via
  `merged.is_merged`). The reds survive the merge — #696 leaves the `Aborted` arm silent and
  neither PR adds a ceiling check. Once merged, this builds in parallel with any bundle sharing
  no file (e.g. #655, `crates/core/src/multipart.rs`). **Conflict, not rebase fact:** the
  `PendingEntry` two-field extension formerly attributed to #654 now belongs to **#717** (#654
  was SPLIT and never lands; #692, its successor, was itself SPLIT on 2026-08-09 into #715 →
  #716 → #717, and only the terminal child **#717** carries the `metadata.rs` hunk). #717 is
  unbuilt and its brief declares `Conflicts with: 710, 711` over `core/src/metadata.rs` — a
  file this child edits. **Never share a wave with #717.** #715 and #716 touch only
  `crates/core/src/multipart.rs` and their own new test files, so they share nothing with this
  child and MAY run in the same wave. (Lineage: the conflict declaration was repointed from
  `682` to these ids at that split's acceptance, 2026-08-08, and from `692` to `717` at this
  one's, 2026-08-09.) **Cite by symbol, not by number**, in `metadata.rs`.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Add an exact-`MAX_VALUE_BYTES` acceptance test or accept the off-by-one regression risk — the `>`→`>=` mutant survives, so the legal boundary at `crates/core/src/metadata.rs:371` is not protected.; T4 Contribution — Confirm contribution and prior-art completeness — merged history was checked by each affected path, but `scripts/pdca` and a mechanically searchable closed/rejected-work index were unavailable, so linkage and duplicate-work coverage remain provisional.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 16 mutants tested in 42s: 3 missed, 4 caught, 9 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Decide and test compound-failure precedence — the ceiling return occurs before the existing unavailable source/target aborts, so an off-fleet large-ID candidate can be mislabeled as a permanent “record must shrink” refusal (`crates/custodian/src/rebalance.rs:469`; `crates/custodian/src/reconstruction.rs:896`).; T2 Shape — Approve the budget interpretation or require reduction — the five-file cap is met, but the 549-line new test contains 384 nonblank/noncomment Rust lines before disputed “mechanical harness” exclusions, so compliance with the ≤250 semantic-line cap is not mechanically settled (`crates/custodian/tests/placement_ceiling.rs:1`).; T4 Contribution — Confirm the four recorded batch-review findings and closed/rejected prior art are disposed — merged history was checked by each affected path, but this target has no `scripts/review-branch`, `scripts/pdca`, or closed-work index to reproduce the remaining contribution checks.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 27 mutants tested in 59s: 2 missed, 16 caught, 9 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batched multi-pass review failed with 2 blocking findings in crates/custodian/src/rebalance.rs:160 and :169: `outcome` (a non-Copy EvacOutcome) is consumed by `match outcome { ... }`, so the later `outcome.persisted()` call won't compile. Fix by matching on a reference (`match &outcome`) or computing `outcome.persisted()` before the match. Size backstop concern (2 rounds spent) was explicitly waived by the human — do not treat this as an oversized-slice signal. Other §6 items (T2 Shape, T4 Contribution disposition, Validation fitness-to-purpose, T5 Judgment on lost-CAS conflicts) were left unresolved pending the rebuild; re-evaluate them against the fixed patch on the next round.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
