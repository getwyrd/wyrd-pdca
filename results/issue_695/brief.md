# Brief — issue 695 / backfill-reads-through-resolver-contained

> Child 1 of 3 from the #681 split; siblings **#696** (rebalance) and **#697** (reconstruction)
> touch disjoint files. **#682 depends on this one.**
>
> **Re-planned 2026-08-07 after five rejected rounds.** Deliberately SMALLER than the brief it
> replaces (`iteration-v5/brief.md`): two rules that one carried — a generation-restart comparison
> ("Rule A") and a key-identity predicate ("Rule C") — are **out of this slice**, and the lines they
> touched are **frozen at the base**. §Scope says why; the closing table is the evidence.

- **Slug:** backfill-reads-through-resolver-contained
- **Defect:** `crates/custodian/src/backfill.rs` reads the chunk map inline out of the inode record
  at **two** sites, each `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported
  { .. })?` — `:98-101` in `reconcile` (`:76`), ending the fill scan for the whole store, and
  `:180-183` in `emit_remaining` (`:171`), ending the drain gauge. Re-verified on `origin/main @
  339da46`. So a **single** segmented object stops backfill for **every** object in the store, and
  stops the gauge being published at all. Containment is not per object either: a record that will
  not `decode` ends the walk at `:80` and `:174`, before any resolver is involved. Backfill is the
  last of the four custodian loops still reading this way — GC (#650) and restore (#651) already
  read through the shared resolver and contain per object.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_backfill.rs` passes,
  driven only through symbols visible on the base — `wyrd_custodian::backfill::{reconcile,
  BackfillContext}`, `wyrd_custodian::reconciliation::Reconciled`, and
  `wyrd_core::metadata::{seg_key, encode, decode, inode_key, resolve_chunk_map, SegmentGroup,
  SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord}` — over in-memory `MetadataStore`
  / `ChunkStore` doubles. **The discriminator MUST NOT name any symbol this patch introduces** (no
  new variant, field, helper or `pub fn`): the red leg reverts `backfill.rs` and keeps the test, so
  such a reference makes the target fail to compile and the red degrades to UNVERIFIABLE (exit 77)
  instead of a behavioural red. `Reconciled::Blocked` exists on the base (`reconciliation.rs:44`)
  and may be named.

  **Five legs over ONE shared fixture** — one store double, one seeding helper, one audit-capture
  helper:

  1. **A healthy segmented object no longer ends the pass, and blocks nothing.** One segmented
     object whose placements are already full — so it needs no fill — (raw `seg:` records + a
     segmented root, **never** a committer) beside a **fillable flat** record (empty `placement`):
     `reconcile` returns `Ok` (today `Err`), the flat record **is filled** with the full-length
     identity vector, and the answer is `Reconciled::Changed` — **not** `Blocked`. *(binding —
     base-red; also binds answer rule 1)*
  2. **A segmented record whose fill this pass may not perform is declined, not mutated, and the
     pass does not certify.** A segmented object carrying an **empty** placement: its `seg:` record
     bytes and its root's `version` are **byte-identical** afterwards; the decline is on the audit
     seam under an action a reader can tell apart from "unreadable" (§Scope pins the vocabulary) and
     is counted; those empty placements are still on the remaining-gauge; `reconcile` answers
     `Reconciled::Blocked`. *(binding — base-red)*
  3. **An unreadable committed object is named, the walk continues, and nothing certifies.** Seed
     — **first in key order**, over a `BTreeMap`-backed store so it is a fixture property and not
     luck — (a) a committed root naming a `SegmentRef` whose `seg:` record was never written, and
     (b) a committed record whose own bytes will not `decode`; assert in the fixture that
     `resolve_chunk_map` really errors on (a). Beside them, a fillable flat record. Assert the
     conjunction: `Ok`, `Blocked` (never `Satisfied`), **the healthy record is still filled**, and
     both damaged objects **named** on the audit seam by their `inode:` key (`gc::object_name`'s
     escaping shape, `gc.rs:470-480`). *(binding — base-red)*
  4. **One reading of the namespace per pass.** Over a store of ordinary **flat** records a counting
     double records exactly **one** `scan(b"inode:")` across `reconcile` *and* the gauge it
     publishes — today two (`:79`, `:173`) — and that gauge's value is unchanged from the base's for
     the same store. Over a store of S segmented objects it makes **≤ S** `seg:` range reads.
     *(binding — base-red on the scan count)*
  5. **A fault that is not one object's map still ends the pass.** A metadata double whose `get`
     fails with a **non-`ChunkMapError`** error makes `reconcile` return `Err`. *(binding; the
     over-containment guard — without it, containing everything would pass legs 1–4. Its base
     behaviour is incidental (the base fails closed on almost everything, so it may go red there
     too); do not spend effort making it non-red.)*

  **A lost CAS is not a blocker.** `crates/custodian/tests/backfill.rs:278-325` already pins
  `Reconciled::Satisfied` after a racing writer wins the version-conditional commit. "Declined work
  ⇒ `Blocked`" must NOT be generalised to "any unfilled record ⇒ `Blocked`", or that existing test
  goes red. Conflicts stay exactly what they are on the base.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, on a plain Linux
  workspace over in-memory trait doubles — no topology, no cfg gate, no Docker, no new
  dev-dependency, **no DST leg**. Verified at Plan, not assumed: `main == origin/main == 339da46`;
  `--print-base` on this bundle → `origin/main`; the `--classify` dry-run on a synthetic patch
  listing exactly `crates/custodian/src/backfill.rs` + the new test returns `ADDED_TEST
  crates/custodian/tests/segmented_map_backfill.rs` and `CRATE crates/custodian`, so the green leg
  is `cargo test -p wyrd-custodian --test segmented_map_backfill` and the red leg reverts
  `backfill.rs` while keeping the test (`engine/scripts/run-verify.sh:96-98`, `:248-256`). No
  `crates/custodian/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped on the base), so
  neither zero-test guard can trip. Legs 1–4 go red on the base; leg 5 is declared above.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an acceptable
  cost** (`docs/principles.md:109`, via the §6 row *Storage lifecycle / reclamation*, `:137`), over
  **the maintenance pass that fills placements**: it reads every committed object the way every
  other consumer reads it; a fault it meets is contained to the object that owns it and the answer
  still gets made for the rest; and it never certifies a drain it did not complete — an operator
  reading `Satisfied` is being told the store converged, and will act on it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0, parallel with #696 and #697.** Touches
  `crates/custodian/src/backfill.rs` plus one new test file; neither sibling touches either. Every
  code prerequisite is already merged on the base (#649's `resolve_chunk_map`, #650's
  `Reconciled::Blocked` and the GC containment precedent, #651's restore precedent). **#682 depends
  on this child** and lands after it.
- **Surfaces:** data
- **Difficulty:** medium   (one production file, two call-sites; the answer change propagates to
  `reconcile_step`'s `least_certified` fold, `reconciliation.rs:51-61`, and the folded gauge is read
  back by `crates/custodian/tests/backfill_telemetry.rs`.)
- **Scope:** backfill **reads every committed object through the resolver every other consumer
  already shares, contains per object what it cannot read, declines — rather than aborts or silently
  mutates — the work it does not own, reports the placement gauge from that same single reading, and
  refuses to certify a pass that answered over less than the committed store.** A record whose
  chunks live in `seg:` records is left **byte-identical** and its fill declined (the segmented write
  path is #682's); a fillable **flat** record in the same store is still filled in the same pass.

  **Two answer rules, pinned so they are not re-derived (each is a finding waiting to happen):**
  * **A decline is per unfilled placement, not per segmented object.** A segmented object the pass
    read successfully and that needs no fill is ordinary and healthy: it blocks nothing and the pass
    may still answer `Satisfied`. Only a **fillable** placement this pass may not write, or an object
    it could not read, withholds certification. Get this wrong and every store holding one multipart
    object is `Blocked` forever, which is worse than the defect being fixed.
  * **An empty placement this pass READ stays on the remaining-gauge until a fill is known to have
    landed** — including one it declined, and including one whose CAS was lost. Only a committed
    fill takes it off. The gauge is the operator's drain signal; a declined fill is still owed.

  **The constraint that keeps the write honest — it bounds the shape, it names no mechanism.**
  Whether this pass may write for an object, and the bytes any write is built from and conditioned
  on, are decided from **the generation the scan returned** — never from what a resolve answered
  after restarting onto a newer root. *Why that needs no machinery of its own:* a **flat** snapshot
  resolves to a borrow of the record and reads nothing — `ChunkMap::Flat(chunks) => return
  Ok(Resolution::Answer(Cow::Borrowed(chunks)))`, `crates/core/src/metadata.rs:2585` — so it can
  never be `Superseded` and never restarts (`:2629`). Only a **segmented** snapshot can, and a
  segmented snapshot is one this slice declines. Honour the constraint and the restart path reaches
  no write at all, **by construction**: no generation comparison, no new counter, no new concurrent
  path to sweep. (The previous brief added the comparison, then had to buy a 325-line seeded DST
  property to justify it. Both go with the path they guarded.)

  **The added audit/metric vocabulary, pinned at Plan — do not invent a parallel set, do not
  relitigate the names.** Exactly this, and each item MUST be asserted by a leg above (an unasserted
  label is a finding waiting to happen):
  * `action = "unresolvable-chunk-map"` + `monotonic_counter.backfill_unresolvable_records` for a
    record that will not decode or a generation the resolver refused — the **same action string** gc
    and restore already publish (`gc.rs:563-573`, `restore.rs:825-835`), so one grep finds all three;
  * `action = "declined-segmented"` + `monotonic_counter.backfill_declined_records` for a fill this
    pass may not perform;
  * `gauge.backfill_placement_incomplete` beside the existing `gauge.backfill_placement_remaining`,
    on the same event, each as its own `gauge.`-prefixed instrument (an unprefixed integer beside a
    gauge reaches the `tracing`→OTel bridge as an attribute on every metric in the event and would
    split the series an operator watches).

  Nothing else. Naming is by the store's own key through `gc::object_name` (`gc.rs:470-480`), which
  escapes rather than replaces, so two damaged records never arrive under one name.

  **/ out of scope — and for the first two, the base lines are FROZEN:**
  * **Key identity and attribution (the previous brief's "Rule C") — DO NOT TOUCH. Tracked as
    #698.** `parse_inode_key` (`backfill.rs:64-70`), its skip (`:84-86`), the CAS key
    `metadata::inode_key(inode_id)` with the `metadata::encode(&record)` precondition (`:142-145`),
    and the `inode_id` audit fields of `emit_backfilled` / `emit_conflict` (`:195`, `:223`) all stay
    **byte-identical to `origin/main`**. Yes, a row under a non-canonical spelling (`inode:007`)
    would be read at one key and CAS'd at another — real, **pre-existing**, unreachable today
    (`metadata::inode_key` is the sole writer of the `inode:` prefix,
    `crates/core/src/metadata.rs:33-36`), and **not this issue's defect**. Removing that parse is
    what produced the sole blocking finding in rounds 3 and 5. If a reviewer raises it: *"unchanged
    from `origin/main`; carved out to #698 and out of scope by the brief"* — record-reject with that
    reference, do not fix.
  * **A generation-restart comparison, a `changed-under-scan` class, and any seeded Tier-0 DST leg
    (the previous brief's "Rule A") — DO NOT BUILD. Tracked as #699.** The constraint above removes
    the path instead of guarding it. `Ok(None)` from the resolver is **skipped**, exactly as both
    merged peers skip it (`gc.rs:404`, `restore.rs:646`) — not counted, not named. **`crates/dst/`
    is not a file this bundle may touch.**
  * **Any write to a segmented record** — `repoint_chunk`, the record ceilings and the write path
    for a `seg:`-resident chunk are **#682**. A decline writes **nothing at all**.
  * `rebalance.rs` and `reconstruction.rs` — the sibling children **#696** / **#697**. Do not touch
    them; a diff that does collides with a bundle building in the same wave.
  * `gc.rs`, `scrub.rs`, `restore.rs`, `desired_state.rs` — untouched (`object_name` is *used*, not
    changed). Sharing ONE namespace walk across the loops is a separate refactor.
  * The chunk-id floor (#652); the committer/fence/rollback/resume (#653); the operator repair
    surface (#694); restore's malformed-placement report (#690).
  * The existing suites `crates/custodian/tests/backfill.rs` and
    `crates/custodian/tests/backfill_telemetry.rs` stay green **unmodified** — both were green under
    the much larger v5 patch, so needing to edit either signals an answer changed further than
    intended; it is not a licence to edit them.
  * **No docs edit** (checked at Plan: `docs/design/architecture/06-runtime-view.md` §6.2, `:29` and
    `:31`, already states this containment rule fleet-wide — *"the damaged record is attributed and
    the walk continues"*, and a pass that cannot read every object *"does not certify"* — so the
    living architecture already describes the post-fix behaviour); no new or edited ADR / spec /
    proposal; no conformance-vector change; **no `Cargo.toml` change** — every dev-dependency the
    discriminator needs (`wyrd-testkit`, `tokio`, `async-trait`, `bytes`, `tracing-subscriber`) is
    already declared on `crates/custodian` (verified at Plan); adding one would trip the ADR-0003
    audit.
- **Budget:** **exactly 2 files.** `src/backfill.rs` ≤ **95** added semantic lines (non-blank,
  non-comment); `tests/segmented_map_backfill.rs` ≤ **240 semantic / 400 raw**. Calibration: the
  rejected v5 patch spent 111 production semantic lines *including* Rules A and C, so the core alone
  sits comfortably inside 95. A **third file**, a `crates/dst/` hunk, or a test file past 400 raw
  means the shape is wrong: **STOP and hand back rather than finish.** Compression rules: ONE
  `BTreeMap`-backed metadata double carrying both the counters leg 4 reads and the injected `get`
  fault leg 5 needs; ONE parameterised seeding helper; ONE audit-capture helper.
- **Repro instruction:** on the target checkout, `git -C ../wyrd show
  origin/main:crates/custodian/src/backfill.rs` at `:98-101` and `:180-183`. Seeding **any**
  `seg:`-backed committed root — or any committed record that will not decode — makes
  `backfill::reconcile` return `Err` for the whole store. The seeding shape to copy is
  `seed_segmented` at `crates/custodian/tests/segmented_map_restore.rs:387-410`, and `seed_damaged`
  at `:417-431`, which asserts its own fixture is genuinely unreadable.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`, `cargo-machete` — the five registered `[[doctor.checks]]` ids at pdca.toml lines 696, 703, 711, 733 and 740, each re-run and OK on this host at Plan. Nothing else beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory doubles. No Docker, no protoc, no live backend, no new dependency, no DST leg.
- **Test file:** `crates/custodian/tests/segmented_map_backfill.rs` — a **NEW** file, not optional
  and not appended elsewhere. C4-verify earns its red only from an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:96-98`); appending to `segmented_map_consumers.rs` or
  `segmented_map_restore.rs` makes it a *modified* file, the gate takes the green-only branch and
  proves no red at all. Confirmed by the `--classify` dry-run at Plan. The name completes the family
  (`…_consumers.rs` #650, `…_restore.rs` #651).
- **Verification posture:** default — assertion-red on the base, green with this patch, both at
  Check. Pre-declared so it arrives at sign-off settled rather than as a surprise: **no seeded
  Tier-0 DST case ships in this child, and none is owed.** The repo rubric asks a *new concurrent or
  destructive path* for seeded Tier-0 coverage; this slice adds neither. Every write it performs is
  on a flat record resolved by borrow from the generation the scan returned
  (`crates/core/src/metadata.rs:2585` — a flat snapshot reads nothing and can never be superseded),
  committed under the base's own unmodified version-conditional CAS; the segmented side performs a
  decline, which writes nothing. A review finding asking for a DST leg here is **recorded-rejected**
  in `review-rejected.md` with that reason, citing `metadata.rs:2585` and `:2629` and the carve-out
  **#699** — it is not fixed by adding one, and adding one puts the bundle over budget and out of
  scope.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice: mirror the two merged peers rather than invent.** Peer callsites Do
  MAY open:
  * `crates/custodian/src/restore.rs:620-690` — **the closest peer and primary model** (#651, merged
    `8decc93`): the same walk over the same namespace — decode contained, state checked, `Ok(None)`
    skipped, the resolve arms and the `ChunkMapError` downcast rule.
  * `crates/custodian/src/gc.rs:360-416` — the same walk a second time (#650), the downcast rule
    stated in full at `:402-416`. Contain by exactly this rule and no other.
  * `crates/custodian/src/gc.rs:155-166` — attribution emitted **per object, before the work loop**,
    so a later store fault cannot cost the operator the name of the record to repair; `:470-480` for
    `object_name`'s injective escaping. Mirror the placement, not just the call.
  * `crates/custodian/src/gc.rs:234-246` — the refusal to certify over an incomplete reading, and why
    it is not `Satisfied`. Reuse this shape.
  * `crates/custodian/src/reconciliation.rs:44` + `:51-61` — `Reconciled::Blocked` and
    `least_certified`. Reuse this vocabulary; do not invent a parallel outcome.
  * `crates/core/src/metadata.rs:2579-2587` (flat resolves by borrow, never restarts) and
    `:2619-2631` (`resolve_chunk_map`'s three arms) — the §Scope constraint lives here.
  * `crates/custodian/tests/segmented_map_consumers.rs:77-116` — the `BTreeMap`-backed `MemMeta`
    whose ordering makes "the damaged record is met FIRST" a fixture property rather than luck; its
    `scan_page` delegates to `wyrd_testkit::test_double_scan_page` (`:109-116`).

  **Salvage, carefully.** `results/issue_695/iteration-v5/patch.diff` is the rejected fifth attempt;
  its containment core passed C4-ci, C4-verify red→green and 0 surviving mutants, and its module-doc
  and emitter wording are reusable. It also carries Rules A and C and a `crates/dst/` hunk, **all
  out of scope here**. Reference, not a starting diff to subtract from — the peers above are the
  positive model.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work, re-run at Plan. `git -C ../wyrd log origin/main --
  crates/custodian/src/backfill.rs` → nearest are `3e05891` (#648 — the segmented record shape,
  which **created** these two sites) and `fddb448` (identity placement backfill); unchanged since.
  No open PR touches it. Prior attempts: seven on the un-split #681
  (`results/issue_681/iteration-v1..v7/`) and five here (`results/issue_695/iteration-v1..v5/`).
- **Disposition hint:** likely-fix

## What five rounds measured (why this brief is smaller, not different)

| Round | Sole blocking finding | Rule that generated it |
|---|---|---|
| v1 | restart-refusal bypass; conflict/refusal accounting; 4 surviving mutants | Rule A |
| v2 | (review output could not be reproduced — not a code finding) | — |
| v3 | malformed-inode-key gap, caused by *removing* `parse_inode_key` | Rule C |
| v4 | no seeded Tier-0 DST for the generation-race path | Rule A |
| v5 | decode-before-key-validation misclassification at `backfill.rs:148` | Rule C |

Not one finding landed on the containment core. v5's gates were otherwise all green — `C4-ci` pass,
`C4-verify` red→green over 8 tests, `C5` 0 survivors, reviewer PASS on C1–C5/T1/T2/T3/T5 — with
`T4-batch-review` the only failure, on the Rule C finding. Siblings #696 and #697 show the identical
signature. So: one real defect carrying two unrelated hardening rules that each kept re-opening. The
rules are **removed, not redistributed** — Rule A's path is closed by construction, Rule C's lines
are frozen at the base as a known, unreachable, pre-existing hazard. Neither is dropped: both were
filed at Plan as **#699** and **#698** (milestone *Foundations*), each carrying its evidence and the
question it has to settle — so a reviewer who raises either has a tracker reference to be pointed at
rather than a rebuild to trigger.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Whether each unreadable class independently withholds certification remains unproven — changing either per-object `incomplete += 1` to `*=` survived mutation because the combined test needs only the other blocker; crates/custodian/src/backfill.rs:127, crates/custodian/src/backfill.rs:156, crates/custodian/tests/segmented_map_backfill.rs:369.; T4 Contribution — Confirm that the two reported batch-review blockers are resolved and the closed/rejected-work prior-art scan is complete — `scripts/review-branch` and the contribution checker/corpus were absent, while merged history by affected path only re-derived 3e05891 and fddb448.; T5 Judgment — A rebuild is owed before sign-off: compress the discriminator below the hard ceiling and make each unreadable-object increment independently observable, because current evidence permits an incomplete-object undercount; crates/custodian/src/backfill.rs:127, crates/custodian/tests/segmented_map_backfill.rs:473.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 17 mutants tested in 27s: 2 missed, 9 caught, 6 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Confirm the closed/rejected-work prior-art and contribution artifacts — merged history was re-run by both affected paths, but `scripts/review-branch` and `scripts/pdca contribcheck` are absent from the supplied target and artifacts, so their claimed green rows cannot be independently reproduced..
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrides the size-backstop's iterate-plan recommendation: this bundle already went through one iterate-plan/split round, so the round counter should be treated as reset. The single live T4 blocker (review-batch.md, backfill.rs:268 — CAS conflict / stale-or-zero gauge / Satisfied) is the round-6 blocker (2) resurfacing at a new line: do NOT fix it in code — RECORD-REJECT it in review-rejected.md. Grounds: (1) the brief mandates Satisfied after a lost CAS ("A lost CAS is not a blocker"; tests/backfill.rs:278-325 pins it); (2) answer rule 2 pins the gauge to what the pass READ — a lost-CAS placement stays owed, and re-reading the winner would violate leg 4's one-reading-per-pass criterion; (3) the winner's never-read placements are the generation-accounting residue carved to #699 (in-code deferred marker at :341-345; #699 calls it "accounting only, self-correcting on the next pass"). A code fix would either double-read the namespace or flip the pinned Satisfied — breaking the brief either way. The T4-contribution and fitness-to-purpose NEEDS-HUMAN items remain open for the next sign-off pass.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_695/review-b
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
