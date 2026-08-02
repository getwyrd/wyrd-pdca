# Brief — issue 649 / shared-segmented-map-resolver-and-read-paths

> Slice **2 of 6** of the #635 re-slicing. **Re-planned after iteration 7** was rejected at
> sign-off: the boundedness question below is settled HERE, and the slice is cut down. History
> is on #635; the seven prior attempts are in `iteration-v1/`…`iteration-v7/`.

- **Slug:** shared-segmented-map-resolver-and-read-paths
- **Defect:** After #648 an `InodeRecord.chunk_map` can be `ChunkMap::Segmented`, but **nothing
  can resolve one**. Every reader still takes the inline list off the record and fails closed —
  `crates/core/src/read.rs:96-97`, `crates/server/src/lib.rs:364-365` (whole-object / streaming)
  and `:459-460` (ranged), each `as_flat().ok_or(SegmentedMapUnsupported)` on this slice's base.
  There is no shared resolution call at all (`git -C ../wyrd grep -n "resolve_chunk_map\|MapResolution"
  pdca-integration/main -- crates/` is empty), so the way each consumer learns which chunks an
  object owns is about to be re-derived once per consumer — which 0016 decision 7(e) forbids, and
  which is exactly how #508's 4th attempt let GC delete a live object's fragments.
- **Success criterion:** Two **added** test files pass, and every assertion is driven through
  **base-visible** entry points (`wyrd_core::read::{read_object, read_path}` and
  `wyrd_gateway_core::ObjectGateway`) over objects seeded as **raw `seg:` records** (never via a
  committer — none exists until #653):
  1. **Byte-identical reads.** A segmented object reads back — whole-object, and over a range
     that **spans a segment boundary** — byte-identical to the flat equivalent of the same
     payload, through both the core read path and the gateway.
  2. **The work a read demands is bounded by the reader, not by the record.** Asserted on a
     **self-contained fake `MetadataStore`** (see *Test file*) whose recorded request log — every
     `get` key, and every `scan_page` prefix / cursor / limit — **is** the oracle:
     a. resolving one object requests the root key plus keys under **only** `seg:<nonce>:<epoch>:`
        — a second group seeded under a different nonce, **and the same nonce at a different
        epoch**, are never requested;
     b. a root naming more segments than `MAX_ROOT_SEGMENTS` is refused with a typed error and
        the log shows **no range request at all** (refused unread);
     c. every page request carries a limit no larger than **a fixed bound this reader sets** —
        never the root's claimed segment count — so a record cannot size one page;
     d. an object that cannot be resolved fails closed **for that object only**: a second,
        well-formed object seeded in the same store still reads.
  3. **Resolution is total and never tears.** Both arms of the resolve-retry rule, asserted on
     the interleaving: root **moved on** (or gone) + a segment absent ⇒ the resolution is dropped
     as a concurrently-retired generation (the read restarts, or answers no-such-key) — never a
     torn half-map, never "this object owns no bytes"; root **unchanged** + a segment that is
     absent, undecodable, over `MAX_VALUE_BYTES`, unnamed, or whose extents disagree with the
     root's table ⇒ **fail closed** with a typed error. Chunks are ordered by the **parsed
     index**, proved against a deliberately shuffling fake. A read driven from a stale
     (superseded) snapshot resolves against the **live** root.

  Also in the issue's acceptance and shipped here, but **not** a Check discriminator: the DST
  resolver-tear property (see *Verification posture*).
- **Falsifiability:** RED is an **assertion** red on base-visible symbols. `C4-verify` resets
  `../wyrd-verify` to this bundle's base: as a wave>0 bundle it is handed
  `PDCA_VERIFY_BASE=origin/pdca-integration/main` (`src/pdca_harness/gates.py:402-407`), honoured
  first by `_resolve_base_ref` (`engine/scripts/run-verify.sh:186-192`), so the patch applies onto
  a tree already carrying #648. That ref exists on `origin` (verified at Plan). `--classify`
  dry-run on this file set returns exactly the two test files below and no cfg-gated addition.
  The RED leg keeps both test files, reverts `crates/core/src/metadata.rs`,
  `crates/core/src/read.rs` and `crates/server/src/lib.rs`. On that tree a segmented root still
  **decodes** (#648's contribution, so every fixture still builds) but no read can resolve one —
  criteria (1)–(3) fail as assertions, and both files still compile because they import only
  symbols visible on the base (`ChunkMap`, `SegmentedMap`, `SegmentGroup`, `SegmentRef`,
  `SegmentRecord`, `seg_key`, `seg_group_prefix`, `parse_seg_key`, `MAX_ROOT_SEGMENTS`,
  `MAX_VALUE_BYTES`, `encode`/`decode`/`inode_key`, `read::{read_object, read_path}`,
  `ObjectGateway` — all verified present on `pdca-integration/main` at Plan). **No dev-dependency
  is added and none is needed** — `async-trait`, `pollster`, `tokio`, `tempfile`, `wyrd-testkit`
  and `wyrd-metadata-redb` are already dev-dependencies of `wyrd-core`
  (`crates/core/Cargo.toml:24-46`) and `async-trait` is a normal dependency of `wyrd-server`, so
  the fake store below compiles as-is. Plain Linux workspace, no topology, no cfg gate on either
  file, so neither the vacuous `0 tests … ok` branch (`run-verify.sh:383-389`,`:420-427`) nor a
  compile-red-scored-as-pass can occur.
  **Keep the DST property out of the discriminator set:** ship it by *modifying*
  `crates/dst/tests/custodian.rs` (`#![cfg(madsim)]`). A **new** `crates/dst/tests/*.rs` would
  join the added-test set and force `RUSTFLAGS=--cfg madsim` + 50 seeds onto the whole
  `C4-verify` invocation (`run-verify.sh:110-134`,`:347-366`). Verified at Plan by a `--classify`
  dry-run on a synthetic patch of exactly this file set: it returns the two added test files and
  nothing else, and because `TEST_SRC_CRATES` is populated only on the no-added-test fallback
  path (`run-verify.sh:300-322`), `C4-verify` compiles **only** those two targets — the modified
  `crates/dst` file is never built by it, so it cannot drag the madsim cfg in either.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **how any process learns which durable bytes an object owns**:
  - **Resolution is total and single-sourced.** Exactly one way to turn a committed inode into
    its ordered chunk list, and every consumer that *can* resolve goes through it. Two answers to
    "which bytes does this object own" is the condition under which one process protects a
    fragment another reclaims.
  - **An answer is never a quiet under-approximation.** A resolution that cannot complete is a
    typed error for **that object**, never an empty or partial list.
  - **Fail-closed is scoped to the object that failed** — one unreadable object must not end a
    read of any other.
  - **The WORK a record can demand of a reader is bounded by the reader, not the record.** A
    root's own table may not set the budget spent on its behalf: a table past the ceiling is
    refused unread, and a page's size is this reader's constant. **The BYTES a read materialises
    are bounded at the `MetadataStore` seam, not here** — see *Settled at Plan* below.

### Settled at Plan — the byte-materialisation bound is the seam's, tracked as #674

The iteration-7 rejection asked whether the byte ceiling on a group's range read is the
resolver's responsibility or the store seam's. **It is the seam's.** Do must not re-litigate this,
and must not grow a mechanism inside the resolver to chase it:

1. **The caller physically cannot check earlier.** `scan_page` returns
   `Vec<(Vec<u8>, Bytes)>` and `get` returns `Option<Bytes>` — values arrive **already
   materialised** (`crates/traits/src/lib.rs:1105`, `:1017`). There is no byte budget and no
   streaming value at the seam, so "check the size as each row streams in" is not a resolver
   change at all; it is a trait change plus five backends plus the shared conformance suite.
2. **The trait already assigns it.** *"The trait sets no key/value/batch size limits of its own;
   a backend's native limits are inherited and surface as `Err`"* (`crates/traits/src/lib.rs:995-999`).
   FoundationDB (100 000 B) and TiKV enforce that on write; only the embedded redb store can hold
   an over-ceiling value at all, and the only writer of `seg:` rows (#653's committer) sizes them
   to fit — so such a row means corruption, not traffic.
3. **The exposure is generic, so a guard here fixes nothing.** No reader anywhere in the tree
   checks a value's length (`MAX_VALUE_BYTES` is an encode-side budget only — verified at Plan),
   and none can: `high_water_marks`' `inode:` walk, GC's `orphan:` ledger and every `get` have the
   identical property. A check in this one function guards one call site while the same hole stays
   open in every other — the single-module symptom guard `docs/principles.md` 1.2 rules out.

**What this slice therefore ships instead** — the bounds that *are* the reader's, criterion 2:
the segment-count ceiling refused **before any row is read**, a group- and epoch-scoped prefix, a
**fixed page bound this reader chooses** (not the root's claim — this is the change from
iteration 7, and it caps the transient page cost at a constant), and a per-record refusal of an
over-ceiling value **documented for what it is**: this resolver will not decode or retain it —
*not* a claim that no memory was spent. Any doc comment or error text asserting a
`MAX_ROOT_SEGMENTS × MAX_VALUE_BYTES` **memory** bound must be corrected or deleted; that
sentence is what earned the finding.

**Do MUST record this as settled in `review-rejected.md` in the FIRST build**, next to the
existing timeout/deadline rows (keep those — they stand), in the gate's
`<file:line> | <CLASS> | <MATCH> | <reason>` format, at every line the range read and the value
check occupy, with `MATCH` phrases covering the class as reviewers spell it — `materializ`,
`scan_page`, `heap`, `value ceiling`, `over-materiali` — and the reason ending
**"Deferred — tracked in getwyrd/wyrd#674"** plus the three citations above. Per INTEGRATION §8 a
finding deferred to a tracked issue is settled; an *unrecorded* one blocks the gate, which is what
burned iterations 6 and 7.

- **Repo + branch target:** getwyrd/wyrd @ main
  (Verified at Plan, and written with **no backticks after the `@`** on purpose: origin/main is
  9120f7a, and this bundle's build/verify base is the wave fold origin/pdca-integration/main =
  6e7c255, carrying #648. The bash base-parser in `engine/scripts/run-verify.sh:168-178` takes the
  first backticked span **anywhere** in the field, unlike its Python twin
  `publish._clean_ref:400-414`, which anchors the span at the start after #235 — so a backtick
  later on this line would resolve `origin/origin/main`. Harmless here because a wave>0 bundle is
  handed `PDCA_VERIFY_BASE` explicitly, but do not reintroduce one.)
- **Depends on:** 648
- **Conflicts with:** *(none — #651 and #652 also edit `crates/core/src/metadata.rs`, but both sit
  in later waves via the dependency chain)*
- **Ordering note:** Second of the serial chain — it consumes #648's `ChunkMap` / `SegmentRecord`
  types and `seg:` helpers and edits the same file; #650 then calls this resolver.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** **the one shared resolver, and the three read call sites that consume it in this
  slice.** Four files of production change, nothing that reclaims, repairs or publishes:
  - `crates/core/src/metadata.rs` — the resolution result type, the group-range read with its
    pre-read segment-count ceiling and its fixed page bound, the resolve-retry arbiter, the
    resolve-against-the-live-root entry, and the `ChunkMapError` variants they raise. **At most
    two public resolve entries** (one from a caller-held snapshot record, one that reads the live
    root); a third needs a caller **in this slice** or it does not ship. Iteration 7 shipped three
    entries and two result types — that breadth is a named cause of the review surface.
  - `crates/core/src/read.rs` — the placement-aware entries (`read_object`, `read_path`) resolve
    through the resolver before assembling bytes. **`read_object_from` keeps its current
    signature** (`:60`) and keeps failing closed on a segmented map: it takes no `MetadataStore`,
    so it *cannot* resolve, and it has **no production caller** — only tests and one bench
    (verified at Plan). Iteration 7 changed its shape and spent ~200 lines and 10 files migrating
    those callers for nothing. If a signature change ripples past two files, that is the wrong
    shape — stop and hand back.
  - `crates/server/src/lib.rs` — the two gateway sites (`:364-365` streaming, `:459-460` ranged)
    resolve through the resolver instead of `as_flat()`. This is a ~6-line swap at each site; the
    surrounding stream/range logic is not this slice's to rewrite.
  - `crates/dst/tests/custodian.rs` — the DST resolver-tear property, added to the existing file.
  **Out of scope:** `crates/custodian/src/resolve.rs` — the custodian-facing wrapper has **no
  caller in this slice** (its consumers are #650/#651, which name the core resolver, not it); it
  ships with its first caller, not here. The byte-materialisation bound (**#674**, settled above).
  GC / scrub / `ReferenceSet` (#650); restore, reconstruction, backfill, rebalance, `desired_state`,
  `repoint_chunk` (#651); the chunk-id floor and startup recovery (#652); the committer, fence,
  rollback and resume (#653); the base lockfile advisory (**#673**); any new/edited ADR / spec /
  proposal; any conformance-vector change.
- **Budget:** **≤ ~1,000 added semantic lines** (non-blank, non-comment) across **≤ 10 files** —
  down from iteration 7's measured 1,961 / 19, which is what the slice was rejected on. Expected
  shape: `metadata.rs` ~350 · `read.rs` ~30 · `server/src/lib.rs` ~30 · core test ~350 · server
  test ~130 · DST ~100 · docs ~10. **No mechanical migration is expected** (see `read_object_from`
  above); if one appears, that is the signal to stop, not a line-count allowance. Prune co-located
  unit tests wherever the integration file already binds the same case — the integration file is
  the discriminator, the co-located one is not. If mid-build the tree exceeds this, STOP and hand
  back a proposed split rather than finishing; an over-budget patch is iterate-to-Plan by default.
- **Repro instruction:** On this slice's base, `git -C ../wyrd show
  pdca-integration/main:crates/core/src/read.rs` (`:96-97`) and `.../crates/server/src/lib.rs`
  (`:364-365`,`:459-460`) — all three fail closed with `SegmentedMapUnsupported`. Seed a
  segmented root plus its raw `seg:` records and read the object: it cannot be read at all.
- **External dependencies:** `typos`, `docs-renderer`
  Both are registered doctor rows (pdca.toml, ids typos and docs-renderer), named because this
  slice edits a living-architecture paragraph and the prose gates inside cargo xtask ci
  warn-and-skip when those tools are absent locally (INTEGRATION §3). Nothing else beyond the base
  Rust toolchain: no Docker, no protoc, no live backend, and no new dev-dependency (verified —
  async-trait, pollster, tokio, tempfile, wyrd-testkit and wyrd-metadata-redb are already
  dev-dependencies of wyrd-core, and async-trait is a normal dependency of wyrd-server); the DST
  property runs under the workspace's own madsim harness. Known base condition, not a dependency
  of this slice and with no installable tool to detect: the base lockfile fails the advisory step
  of cargo xtask ci — RUSTSEC-2026-0221, now tracked as getwyrd/wyrd#673 — see *Verification
  posture*.
- **Test file:** `crates/core/tests/segmented_map_resolution.rs` **and**
  `crates/server/tests/segmented_object_read.rs` — two **NEW** files, both required. This
  project's C4 discriminator is an **added** `*/tests/*.rs` (`run-verify.sh:287-302`,`:392-402`);
  an appended or co-located test degrades to the green-only branch and proves no red. Both MUST
  import only base-visible symbols (listed under *Falsifiability*) — nothing this patch adds — so
  criteria (2)–(3) are observed **through the read paths plus the fake store**, never by calling
  the resolver directly. **Which store backs which criterion** (decided here so Do neither
  re-implements a store nor falls back to a delegating wrapper):
  - **criterion 1** runs against the **real redb** store (already a dev-dependency): seed the
    segmented object's root + raw `seg:` rows directly, write the flat equivalent through the
    ordinary write path, compare bytes. No instrumentation is needed to compare bytes.
  - **criteria 2 and 3** run against a **self-contained fake `MetadataStore`** — it owns its own
    ordered map of rows and answers `get` / `scan` / `scan_page` from it, recording each request;
    tests seed rows into it directly, so it needs no write-path CAS fidelity beyond what the read
    paths call. It MUST NOT wrap or delegate to a real backend: iteration 7's `PageCap` double
    delegated first and truncated after, so it could not prove anything about what was requested,
    and the review said exactly that (`segmented_map_resolution.rs:553`, TEST-GAP). A
    self-contained fake makes its request log the oracle and retires that finding class.

  **Which file carries what**, so the server file stays small: the **core** file carries criteria
  1 (core read path), 2 and 3; the **server** file carries criterion 1 only — whole-object /
  streaming and a range spanning a segment boundary, through `ObjectGateway`, plus the
  fail-closed-per-object case at the gateway. The DST property goes into the **existing**
  `crates/dst/tests/custodian.rs`.
- **Verification posture:** default for criteria (1)–(3) — assertion-red on the base, green with
  this patch, both at Check. Two declared conditions:
  1. **The DST resolver-tear property is not a Check discriminator.** It ships in this patch and
     is exercised in this cycle by `cargo xtask dst` / `ci`, not by `C4-verify`, for the cfg/seed
     reason under *Falsifiability*. Not deferred work — built and run this cycle.
  2. **The gating `C4-ci` is red on the BASE**, at `cargo deny check`, from RUSTSEC-2026-0221
     (`event-listener 5.4.1`) in the base lockfile — independent of this patch, and now tracked as
     **getwyrd/wyrd#673** (0.1 Alpha). `deny.toml` is a declared zero-tolerance wall
     (`deny.toml:19-24`), so it must not be suppressed here, and the lockfile must not be touched
     by this slice — an unscoped `5.4.1→5.4.2` bump was rejected at iteration 6 and did not clear
     the advisory anyway. Because the deny step runs *before* conformance / statics / DST
     (`xtask/src/main.rs:1563-1567`, verified at Plan), a red deny also stops `ci` short of those
     tiers: Do MUST run
     `cargo xtask conformance`, `statics` and `dst` individually and record their results in
     `build-notes.md`, so the evidence exists at sign-off.
- **Citations expected:** cite `path:line` on this slice's base for every change.
  **Salvage — do not re-derive settled code.** Two permitted inputs inside this bundle:
  `iteration-v7/patch.diff` (a complete, `C4-verify`-passing red→green implementation of this
  slice — the fastest starting point) and `sources/salvage.diff` (#647's original). Take
  iteration 7's resolver, read-path and gateway wiring, and its DST property, then **make exactly
  these changes** — they are the whole point of this re-plan:
  1. **drop** `crates/custodian/src/resolve.rs` and its `crates/custodian/src/lib.rs` line (202
     semantic lines, no in-slice caller);
  2. **revert** the `read_object_from` signature change and delete the resulting 10-file
     benches/tests migration;
  3. **replace** the delegating `PageCap` store double with a self-contained fake, and assert on
     its request log (criterion 2), including the new fixed-page-bound clause 2(c);
  4. **correct** every doc comment and error text that claims a memory/materialisation bound, per
     *Settled at Plan*, and write the `review-rejected.md` rows;
  5. **prune** the co-located resolver unit tests and the integration cases to the binding set
     above, and collapse three resolve entries to at most two.
  Peers Do MAY open on the base: `crates/traits/src/lib.rs:1105` (the four `scan_page` clauses —
  order, exclusive cursor, termination, no-skip — which is why a group range is **paged**),
  `:387-401` (`page_limit` clamping, the peer for clause 2(c)), `:995-999` (the operational
  envelope quoted above) and `crates/core/src/metadata.rs:276-352` (#648's ceiling constants and
  their stated division of labour — `MAX_ROOT_SEGMENTS` is *"enforced where a segment table
  becomes work … the ranged read that would spend it (#649/#653)"*). Normative design:
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — decision **7(e)** at
  `:2393-2400` (the bounded, epoch-scoped group range; *"never a global `seg:` scan"*) and **7(h)**
  at `:2452-2471` (the resolve-retry rule, both arms). Both line ranges verified at Plan.
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — **the resolver
  paragraph, and only that one**: one metadata value has a ceiling so a large map is segmented;
  resolving is the root plus one bounded, group- and epoch-scoped range read ordered by parsed
  index; every consumer that can resolve does so through the same call; the fail-closed and
  retired-generation arms; fail-closed scoped to the object. State the bound honestly — the reader
  bounds the **work** (segments, pages); the **bytes** are the store's (#674). Do **not** claim
  universal consumer routing here — the custodian passes still fail closed until #650/#651, and
  claiming it now is what earned iteration 7's C1 finding. Containment sentences belong to
  #650/#651, staged publication to #653.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs (re-run at Plan). `crates/core/src/metadata.rs` + `read.rs` + `server/src/lib.rs`:
  the only **closed-unmerged** PR touching any of them is **#647** (this slice's salvage source,
  closed on **reviewability, not correctness**); the only **open** one is **#672** (= #648, this
  bundle's base). Six merged PRs touch `metadata.rs`/`read.rs` recently (#609, #594, #565, #564,
  #558, #534) — ETag/metadata, lease publication, fragment identity, repair enqueue, fragment
  naming — none touching chunk-map resolution. No merged or rejected prior art for a shared
  chunk-map resolver.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout / deadline over a `MetadataStore`
  await* — rejected 3× across #508/#636 and argued in full in this bundle's `review-rejected.md`:
  the store implementation owns the network bound (`crates/traits/src/lib.rs:1000-1012`), and
  `wyrd-core` is executor-free by design; (ii) *the byte-materialisation bound* — settled above,
  **#674**; (iii) *retraction of already-published bytes* — rejected 4× in #638 on unchanged
  evidence; (iv) *"`Completed` releases its admission slot"* — withdrawn as unsatisfiable;
  (v) every settled decision named in the slice issue's body. Do MUST record each rejection in
  `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Carry-forward from the seven prior attempts

Iterations 1–7 are preserved in `iteration-v1/`…`iteration-v7/`. Iteration 7 passed `C4-verify`
(red→green) and `C5-mutants`, and failed the two gating rows: `C4-ci` on the base-tree advisory
(now #673) and `T4-batch-review` on four findings, all of the byte-materialisation class settled
above. The sign-off rejected it to **Plan**, not to Do, because "7 prior build iterations patching
symptoms within the same resolver shape have not converged — the diff has been growing rather than
shrinking". This brief answers that with three structural changes, not another patching round:
the boundedness question is **decided and assigned** (#674), the caller-less wrapper and the
signature-change migration are **removed from scope**, and the budget is halved with a hard STOP.
Do not re-attempt the rejected shape unchanged.

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: C5 Causal adequacy is unresolved: mutation testing left two mutants alive at crates/core/src/metadata.rs:2271 (`>`->`>=`) and crates/core/src/metadata.rs:2290 (`||`->`&&`) — the segment-count ceiling's exact edge and a group-mismatch condition are not proven by any test. Add targeted test coverage that kills both mutants (an exact-ceiling case for MAX_ROOT_SEGMENTS, and a one-component group-mismatch case), or — if on inspection either check turns out not to be load-bearing — remove the unsupported defense instead. Re-run the C5 mutants-in-diff gate after. All other §6 items were cleared by the human at sign-off: - C1 Spec, T4 Contribution, T5 Judgment, Validation fitness-to-purpose, and the C4 deny-gate (pre-existing base issue #673) were explicitly accepted/waived. - Two §6 bullets (the event-listener/Cargo.lock item and the ~1,925-line item) were stale findings from an earlier internal build round (deferred-findings.json, through_round: 5) referencing code no longer in the final patch.diff; confirmed moot and cleared. - One §6 bullet ("external dependency.**") was a markdown-parsing artifact from a bolded sentence in build-notes.md that explicitly denied any such finding; confirmed moot and cleared. - Both pipeline bugs (stale deferred-findings not re-validated against the final diff; markdown-bold parsing producing a spurious NEEDS-HUMAN bullet) are recorded as Act candidates in SUMMARY.md §10.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 57 mutants tested in 2m: 2 missed, 15 caught, 40 unviable
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Unresolved blocking finding at crates/core/src/metadata.rs:2162 (T4 batch-review, not recorded-rejected): the resolve-retry arbiter collapses an absent root into the same `false` result as a superseded generation, so a deletion observed on the final allowed retry attempt returns `MapResolutionUnstable` instead of `None`. This is a real correctness gap in the resolve-retry logic that success criterion 3 of the brief (resolution is total and never tears; root-gone must be distinguished from root-superseded) directly requires. Fix the arbiter to distinguish "root absent" from "root superseded" so a genuinely deleted object resolves to not-found rather than an unstable/retry error, then re-run the batch review to confirm the finding clears. The other §6 items (T2 Shape line-count overage, Validation fitness-to-purpose pending #653) were not adjudicated this round — revisit at the next sign-off once this finding is fixed.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
