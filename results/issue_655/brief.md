# Design proposal — issue 655 / multipart-knob-constants-and-derivations

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart commit
> protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on `origin/main`
> @ `339da46`. This brief authors **no new design**: it points at the sections that ARE this
> slice's specification, fixes which knobs are this seam's, and states the C4 shape.
> **Do MUST read these before writing code:**
> Decision 4 §"The arithmetic first" `0016:1038-1075` (the three ceilings and the real numbers) ·
> Decision 4's rules `0016:1076-1462` · **the knob table — valid range, bounding invariant and
> chooser, per knob — `0016:1463-1480`** (this table is the specification) ·
> the staged-ceiling / overshoot argument `0016:1430-1456` ·
> the accepted-costs register `0016:2836-2860` · the follow-ups `0016:3072-3080` (which knob 0016
> assigns to which slice) · the failure-mode table `0016:1484-1502`.
>
> **Slice 2 of 7 of #636. Depends on #693** — the last child of #654's split chain
> (#691 key grammar → #692 record family → #693 answer table + digests): the module and the
> format constants these values are clamped against land across that chain.
> Siblings: #656 (3), #657 (4), #658 (5), #659 (6), #660 (7).
> Tracker: https://github.com/getwyrd/wyrd/issues/655.

- **Slug:** multipart-knob-constants-and-derivations
- **Kind:** enhancement (design proposal)
- **Goal:** the multipart seam's **numbers**, each a named constant carrying its derivation in its
  own doc comment, plus one function — `knob_clamps_hold` — that asserts the inequalities *between*
  them. 0016 settles each knob's valid **range** and its **bounding invariant** and leaves the
  **value** to the implementing slice (`0016:1463-1480`, `:3072-3080`); the caps are enforced in
  this seam's code, so this seam picks them. After this slice the whole later protocol (#656–#660)
  and the reaper (#625) **consume** a single, self-consistent value set instead of each re-deriving
  one — which is the split-budget-authority failure `docs/principles.md:138-143` records as the
  reason the §6 invariant row exists at all.
- **Success criterion:** the NEW file `crates/core/tests/multipart_knobs.rs` passes. Five legs,
  all pure — no store, no runtime, no fixture beyond literals:
  1. **The shipped value set satisfies every clamp.** `knob_clamps_hold` over the deployment's own
     `Budget` returns success.
  2. **`knob_clamps_hold` is not vacuous — it REJECTS each violation, one leg per clamp.** A table
     of budgets, each violating exactly **one** inequality, each asserted to be rejected *and* to
     name the clamp it broke. At minimum: `max_chunkref_bytes × MAX_MAP_CHUNKS > V/2`; the same for
     `MAX_SEG_CHUNKS` and for `MAX_PART_CHUNKS`; `MAX_PART_CHUNKS > B_ops`; `MAX_STAGED_CHUNKS`
     below `MAX_PART_CHUNKS`; `MAX_STAGED_CHUNKS` above `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`;
     `MAX_INFLIGHT_PARTS > MAX_PARTS_PER_SESSION`; `MAX_INFLIGHT_PARTS > ⌊SCAN_CAP / (2 ×
     MAX_PART_CHUNKS)⌋`; `MAX_INFLIGHT_PARTS` whose whole-range fence/terminal-delete batch exceeds
     the mutation-byte budget; `MAX_INFLIGHT_PARTS > B_ops`; `MAX_SESSIONS × U_ref > W_ref`;
     `MAX_SESSIONS > SCAN_CAP/2`; `MAX_OWNED_FLEET > W_ref / 2`; a retry bound of 0. **This leg is
     the binding one** — a `knob_clamps_hold` that returns success unconditionally passes leg (1)
     and proves nothing.
  3. **The derivations are recomputed independently — the "no /2-vs-/4 drift" check, made
     mechanical.** The test recomputes, from first principles and *not* by calling the production
     helper, each derived quantity and asserts equality with the shipped constant:
     `VALUE_CHUNK_CAPACITY = ⌊(MAX_VALUE_BYTES / 2) / max_chunkref_bytes⌋`;
     `MAX_STAGED_CHUNKS = MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`;
     `U_ref = min( (MAX_PARTS_PER_SESSION + MAX_INFLIGHT_PARTS) × MAX_PART_CHUNKS ,
     MAX_STAGED_CHUNKS + 2 × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS )`;
     `MAX_SESSIONS = min( ⌊W_ref / U_ref⌋ , SCAN_CAP/2 )`;
     `MAX_OWNED_FLEET = MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`;
     `max_part_bytes = MAX_PART_CHUNKS × chunk_size`. Assert the **halving** each budget uses is
     the one 0016 states (`V/2`, `E_tx/2`, `W_ref/2`) — a `/4` or a whole-`V` sizing must fail this
     leg. Assert the *stated range* too: `MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS` and `MAX_PART_CHUNKS`
     each land in **165–381** at the `b_ref` extremes 0016 computes (`0016:1050-1075`), so a
     `max_chunkref_bytes` that drifted from the encoded reality is caught.
  4. **Every capacity knob fits the key space #691 gave it** (`PART_NUMBER_WIDTH = 6` /
     `SLOT_INDEX_WIDTH = 6`, pinned at the split Plan). `MAX_INFLIGHT_PARTS` is addressable by
     the slot-index width and `MAX_PARTS_PER_SESSION` by the part-number width, with
     byte-lexicographic order still equal to numeric order **at the cap** — the property that makes
     the `slot:` key space *be* the in-flight bound (`0016:349`) rather than an integer someone must
     CAS correctly. A knob that overflows its width is rejected by `knob_clamps_hold`.
  5. **The admission backoff is bounded, and its two retry budgets are separate.**
     `admission_backoff_millis` never exceeds the cap, is never below the base, grows with the
     attempt, and includes jitter within the stated envelope for every jitter input in a swept
     range. Assert **separately** that the upload-id-collision budget and the CAS-contention budget
     are **distinct constants**: a single shared budget is the carried-forward `503 SlowDown` defect
     (below), and this is the slice that fixes its *numbers*.
- **Falsifiability:** **RED is criterion-ABSENCE — born-at-tier, exactly as the #654 chain
  (#691–#693).** No multipart
  knob exists on `origin/main` (`git -C ../wyrd grep -rn "MAX_PART_CHUNKS\|MAX_INFLIGHT_PARTS\|W_REF\|knob_clamps_hold" -- crates/`
  → **no matches**), and after the chain lands there is still no *value* to contradict.
  - **C4-verify will report `UNVERIFIABLE` (exit 77) on its RED leg, and that is EXPECTED and
    PRE-DECLARED.** `run-verify.sh --classify` on a synthetic patch listing this slice's file set
    returned exactly `ADDED_TEST crates/core/tests/multipart_knobs.rs` + `CRATE crates/core`, so the
    GREEN leg is `cargo test -p wyrd-core --test multipart_knobs` (precise and fast) and the RED leg
    reverts `crates/core/src/multipart.rs` to its post-chain (#691–#693 merged) state, removing every constant the test
    names — a compile failure, reported as *"the discriminator never executed"* rather than a
    verdict (`run-verify.sh:487-497`). It routes to SUMMARY §6 as a NEEDS-HUMAN item; it is not a
    defect in the patch.
  - **The demonstrated red Do MUST capture instead (binding).** For legs **(2)**, **(3)** and **(5)**
    Do MUST temporarily negate the production code in one named way each, run the discriminator,
    paste the failing output into `build-notes.md`, and revert: (2) make `knob_clamps_hold` return
    success unconditionally; (3) size one budget against the whole `MAX_VALUE_BYTES` instead of half
    it; (5) collapse the two retry budgets into one constant. Each MUST make the discriminator fail.
    A leg that survives its negation is not load-bearing and must be rewritten before the bundle
    ships. This is the evidence #636's round-3 sign-off asked for by name after a clean-base replay
    *"ran 0 tests, so it proves criterion absence rather than that the assertions are
    load-bearing."*
  - **Base.** `PDCA_BUNDLE=results/issue_655 ./engine/scripts/run-verify.sh --print-base` →
    `origin/main` (run at Plan). Under `wave_mode = "merge"` (pdca.toml:90) each chain child's PR
    (#691, #692, #693) is merged into
    `origin/main` at its wave boundary before this wave builds, so the ref C4-verify resolves
    **is** the base the PR
    opens against and **does** contain the whole chain (INTEGRATION §2). No `Onto branch`, no
    `stack-base` marker in the bundle.
  - **No vacuous green.** No `crates/core/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped
    on the base), so the GREEN leg cannot report `0 tests … ok` (`run-verify.sh:445`).
  - **No environment is missing** — pure arithmetic over constants on a plain Linux workspace. No
    topology, cfg gate, Docker or live backend is involved.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the admission arithmetic that decides how much work
  the store may be asked to hold**. Sourced, not intuited: `docs/principles.md:137` (§6 row *Storage
  lifecycle / reclamation*), sourced to §5 C-1 (`docs/principles.md:109`), the maintainer's rule of
  2026-07-25, `0016:2802-2813`, `gc.rs:22-25`. Note that this row's **own provisional evidence is
  this exact failure**: `docs/principles.md:138-143` records the 2026-07-25 multipart batch, where
  *"a split budget authority would have shipped a legally-admitted session whose unsplittable reap
  fence could never commit (never reaped, never reclaimed)"*. Over that category:
  - **No number this seam admits against may exceed the envelope of the batch that must later
    undo it.** An unsplittable batch — a part commit, the reaper's whole-range fence, the terminal
    delete that decrements the admission count — is bounded by the transaction envelope. A cap
    admitted above it produces work whose *only* exit permanently fails: the session is never
    fenced, never deleted, and holds its admission slot forever. That is `MAX_PART_CHUNKS ≤ B_ops`
    and the byte-derived `MAX_INFLIGHT_PARTS` clamp, and they are the reason this slice exists.
  - **A derived value is derived, never chosen.** `MAX_SESSIONS = ⌊W_ref / U_ref⌋` and
    `MAX_STAGED_CHUNKS = MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` are the *only* admissible spellings; a
    hard-coded `MAX_SESSIONS` is a defect, not a value choice, because it silently decouples what is
    admitted from what the reconcile pass can hold.
  - **One authority for one budget.** Every consumer — this seam, #625's reaper, #508's gateway —
    reads the same constants. Two derivations of one budget is the split authority above; the
    doc comment must say so, so a later slice consumes rather than re-derives.
  - **Each session is charged its WORST case, for every interleaving.** `U_ref` charges the bounded
    commit overshoot (`MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`) explicitly, so a small-part-derived
    value cannot be overrun by a later large-part session — the iteration-4 C5/T2/T4 defect 0016
    records at `:1471`.
  - **A refusal must be a real bound, not a lost race.** A single retry budget serving both a
    2^-128 id collision and a globally serialized CAS answers a false `503` on an *empty* store at
    ordinary client concurrency; the numbers here are what make the backpressure honest.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- **Depends on:** 693
- **Conflicts with:**
- **Ordering note:** **Final wave of the multipart chain.** #654 was SPLIT at its 2026-08-05
  sign-off into the chain #691 (key grammar) → #692 (record family) → #693 (answer table +
  digests); this bundle now depends on **#693**, the chain's last child, which transitively
  carries the whole chain. The dependency is a genuine build-on twice over: (a) these
  constants are appended to `crates/core/src/multipart.rs`, the module #691 **creates** and
  #692/#693 extend, so on a shared base the patches would collide in that one file; and
  (b) leg (4) clamps each capacity knob against the **key-space widths** #691 defines
  (`PART_NUMBER_WIDTH = 6` / `MAX_PART_NUMBER = 999_999`, pinned at the split Plan —
  protocol-neutral headroom), and leg (3) recomputes derivations that use #692's profile
  tuple — none of which exists before the chain lands. Under `wave_mode = "merge"` each
  child's PR is merged to `origin/main` at its wave boundary, so this bundle builds and
  verifies on a tree that contains all three. **If the chain is not accepted, hold this
  bundle** — do not let it absorb the record family; that re-creates the monolith the split
  exists to avoid. **#682 — since SPLIT into #710/#711 (2026-08-08), both of which** touch
  `crates/core/src/metadata.rs`: this slice must **not** touch that file (see § Scope) — with
  that rule it shares no file with either, and #692 (which does share files with them)
  carries the `Conflicts with: 710, 711` that schedules those apart.
- **Surfaces:** data
- **Difficulty:** low   (one existing module gains a constants section and one function; **one**
  new test file. Zero call-sites — nothing consumes these values yet, and no existing behaviour
  changes. The reviewing work is arithmetic density, not blast radius, and the deterministic gates
  plus leg (2)'s violation table own the density.)
- **Scope:** the knob **values** and the clamps between them, appended to
  `crates/core/src/multipart.rs` (the module #691 creates and #692/#693 extend — the workspace has no directory modules;
  verified), with each constant carrying its **derivation in its own doc comment** and the whole set
  cross-checked by one `knob_clamps_hold`:
  - **Consumed, never redefined** — import from `crates/core/src/metadata.rs` and `wyrd_traits`:
    `MAX_VALUE_BYTES` (`metadata.rs:327`), `MAX_ROOT_VALUE_BYTES` (`:352`), `MAX_ROOT_SEGMENTS`
    (`:322`), `SCAN_CAP` (`crates/traits/src/lib.rs:286`). A second spelling of any of these is a
    defect.
  - **The value-ceiling family:** `max_chunkref_bytes` (with its per-scheme companion — the encoded
    worst case, **measured**, not asserted in prose), the shared `VALUE_CHUNK_CAPACITY =
    ⌊(V/2) / max_chunkref_bytes⌋`, and `MAX_MAP_CHUNKS` / `MAX_SEG_CHUNKS` / `MAX_PART_CHUNKS` over
    it — the *identical* rule for all three, because each is one JSON value (`0016:1058-1075`) —
    plus `max_part_bytes = MAX_PART_CHUNKS × chunk_size`, the number that becomes the `UploadPart`
    refusal.
  - **The session family:** `MAX_PARTS_PER_SESSION`, `MAX_INFLIGHT_PARTS` (under all **four** clamps
    at `0016:1471`), `MAX_STAGED_CHUNKS` (fixed at the publishable ceiling), and the fleet-wide
    `MAX_OWNED_FLEET`.
  - **The derived pair, spelled as derivations:** `U_ref` (both branches, `min` of them) and
    `MAX_SESSIONS = min(⌊W_ref/U_ref⌋, SCAN_CAP/2)`.
  - **The retry/backoff bounds:** `R_publish`, `MAX_COMPLETE_ATTEMPTS`, and the **two separate**
    budgets — upload-id collision vs. admission-CAS contention — with the jittered backoff base and
    cap, and `admission_backoff_millis`.
  - **`Budget`** — the value type the clamps are checked over, so `knob_clamps_hold` can be run
    against a hypothetical set (leg 2) and not only the shipped one; and `knob_clamps_hold` itself,
    returning **which** clamp failed, not a bare bool.
  - **The two inputs 0016 assigns elsewhere but this seam's clamps depend on:** `W_ref` (the
    reconcile RAM budget) and the `B` / `B_ops` operation-and-byte budget are **#625's** by
    `0016:3072-3080`, yet `MAX_SESSIONS` and the `MAX_PART_CHUNKS ≤ B_ops` clamp cannot be written
    without them, and #625 builds **after** this slice. So: define them **here** as named constants
    with their derivation, record the chosen values as an explicit **value set** in
    `build-notes.md`, and state in each doc comment that **#625 consumes these and must not
    re-derive them**. (Carried verbatim from #636's brief; it is the split-budget-authority rule
    above.) If a value genuinely cannot be chosen without #625, that is a Check §6 item — **not** a
    placeholder constant.

  **Out of scope:**
  - **`crates/core/src/metadata.rs` — DO NOT TOUCH.** #682 is in this same wave and owns that file;
    editing it here turns two independent bundles into a conflict. Everything this slice needs from
    it is available by import. This is a hard rule, not a preference. (The discontinued #636 patch
    put `MAX_BATCH_BYTES` / `MAX_BATCH_OPS` in `metadata.rs`; put them in `multipart.rs` instead.)
  - **The reaper's windows** — `W_open`, `W_session`, `W_completing`, `W_tombstone`, the cursor-keyed
    out-of-band drain and the clock guard are **#625's** (0016's own assignment, and #636's stated
    out-of-scope). This slice defines no time window except the memory budget `W_ref`. Likewise
    `W_write` / `G_orphan` (the write path and #625) and `W_repoint` (#653) are not this slice's.
  - **Enforcing any cap.** No admission check, no `EntityTooLarge` refusal, no batch splitting —
    those are #656/#657/#658/#659. This slice ships the numbers and the assertion that they are
    mutually consistent; it changes no behaviour because there is none yet.
  - **Any store round trip**, any `async fn`, any `WriteBatch`.
  - `crates/core/src/lib.rs` — untouched (the `pub mod multipart;` line is #691's).
  - The S3 verbs and their status codes (**#508**); the custodian protection class (**#637**).
  - Any file under `docs/design/adr/` or `docs/design/specs/`, any edit to `0016` itself, any
    conformance-vector change, any new dependency.
- **Budget:** ≤ **400** added semantic lines (non-blank, non-comment, non-mechanical — the doc
  comments carrying each derivation are the point of the slice and are **not** counted against it,
  but they must be derivations, not restatements), across ≤ **2** files:
  `crates/core/src/multipart.rs` and `crates/core/tests/multipart_knobs.rs` (**new**). A **third**
  file means the shape is wrong — most likely `metadata.rs` crept in: STOP and hand back.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — the five doctor.checks ids (pdca.toml :696, :703, :711, :733, :740), all OK on this host at Plan (scripts/pdca doctor). Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool. This slice is unusually doc-comment-heavy, so the spell gate is a real one here. Nothing else beyond the base Rust toolchain: pure arithmetic, no runtime, no Docker, no protoc, no live backend, no new dependency.
- **Test file:** `crates/core/tests/multipart_knobs.rs` — a **NEW** file, not optional. C4-verify's
  discriminator is an **added** `*/tests/*.rs`; a co-located `#[cfg(test)] mod tests` would make the
  gate fall back to `cargo test -p wyrd-core` over the crate's whole suite and take the green-only
  branch (`run-verify.sh:454-464`). Confirmed by the `--classify` dry-run above. `const` assertions
  co-located in `multipart.rs` (the shape `metadata.rs:354` already uses to tie
  `MAX_ROOT_VALUE_BYTES` to `MAX_VALUE_BYTES`) are **welcome in addition** — a compile-time tie is
  strictly stronger than a test — but the five legs must live in the named file, since a `const`
  assertion cannot express leg (2)'s rejection table.
- **Verification posture:** **declared, not default — born-at-tier (posture (a)).** "Red" is
  criterion **absence**: every constant the discriminator names is introduced by this patch, so
  C4-verify's RED leg is a compile failure reported as `UNVERIFIABLE` (exit 77 → SUMMARY §6).
  **What IS built and exercised at Check:** the whole slice — it is pure code, and all five legs run
  green under `cargo test -p wyrd-core --test multipart_knobs` and again under the gating `C4-ci`.
  Nothing is deferred and nothing is scaffolding. **What replaces the red:** the three named
  negation demonstrations under § Falsifiability, captured in `build-notes.md`.
- **Production reach:** N/A — no production consumer exists yet **by design**; the consumers are
  #656–#660 and #625, which are separate filed work items, not a deferred verification of this one.
  The one thing this slice must not do is let that fact soften the numbers: a value chosen "because
  nothing checks it yet" is the split-budget failure in slow motion. Record the value set in
  `build-notes.md` so sign-off reviews it **as a set**.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line number
  in this brief was verified against `origin/main` at `339da46` during the Plan verification pass;
  `crates/core/src/multipart.rs` line numbers will be the #691–#693 chain's, so cite by symbol there.
  **Sources Do MUST open (this slice's design is elsewhere, by INTEGRATION §6):**
  - `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — the sections in the header
    block, above all **the knob table `0016:1463-1480`**, which gives each knob its valid range, its
    bounding invariant and its chooser. Read the *bounding invariant* column, not just the range:
    the invariant is what the doc comment must state.
  - `docs/design/adr/0045-metadata-validation-boundaries.md` — why a capacity constant is enforced
    where work is admitted and **not** at decode.

  **Peer callsites Do MAY open — mirror them rather than invent a shape:**
  - `crates/core/src/metadata.rs:302-356` — `MAX_ROOT_SEGMENTS`, `MAX_VALUE_BYTES`,
    `MAX_ROOT_VALUE_BYTES` and the `const _: () = assert!(MAX_ROOT_VALUE_BYTES * 2 <=
    MAX_VALUE_BYTES);` at `:354`. This is **the** in-tree model for this whole slice: a derived
    capacity constant whose doc comment states its budget rule, its reserve, why it is not a decode
    invariant, and where the worst case is *measured* — plus a compile-time tie so the two halves
    cannot drift. Match this standard of doc comment; it is the deliverable.
  - `crates/core/tests/segmented_map_record.rs` — how `MAX_ROOT_SEGMENTS`' budget rule is
    **measured** on `encode(...).len()` rather than asserted in prose. `max_chunkref_bytes` needs
    the same treatment: measure the encoded worst case, do not assume 302.
  - `crates/traits/src/lib.rs:286` — `SCAN_CAP`, and `:990-1000`, the de-facto backend ceilings
    (10 KB key / 100 KB value / 10 MB transaction) the envelope constants derive from.
  - **Salvage:** `results/issue_636/patch.diff`, the discontinued monolithic patch, whose
    `crates/core/src/multipart.rs` already carries this slice's numbers —
    `MAX_CHUNKREF_BYTES`, `VALUE_CHUNK_CAPACITY`, `MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS`,
    `MAX_PART_CHUNKS`, `MAX_PART_VALUE_BYTES`, `MAX_PARTS_PER_SESSION`, `MAX_INFLIGHT_PARTS`,
    `MAX_STAGED_CHUNKS`, `MAX_STAGED_FRAGMENTS`, `MAX_SLOT_RECORD_BYTES`,
    `TERMINAL_DELETE_FIXED_OPS`, `W_REF`, `R_PUBLISH`, `MAX_COMPLETE_ATTEMPTS`,
    `MAX_UPLOAD_ID_ATTEMPTS`, `MAX_ADMISSION_CAS_ATTEMPTS`, `ADMISSION_BACKOFF_BASE_MILLIS`,
    `ADMISSION_BACKOFF_CAP_MILLIS`, `max_chunkref_bytes_for`, `max_part_chunks_for`,
    `admission_backoff_millis`, `Budget`, `knob_clamps_hold`. **Reuse the value set — it was
    derived against this same table — but (i) move `MAX_BATCH_BYTES` / `MAX_BATCH_OPS` out of
    `metadata.rs` into `multipart.rs` (§ Scope, out-of-scope rule 1), (ii) re-derive rather than
    re-copy each doc comment against `0016:1463-1480`, since the review that closed that patch
    never reached these lines, and (iii) satisfy leg (2)'s rejection table, which that patch's
    tests did not carry.**
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `crates/core/src/multipart.rs` and `crates/core/tests/multipart_knobs.rs`
  do **not** exist on `origin/main` (`git -C ../wyrd ls-tree origin/main crates/core/src/` →
  `erasure|lib|metadata|placement|read|repair|write`), and no multipart knob symbol exists anywhere
  in `crates/` (grepped `MAX_PART_CHUNKS`, `MAX_INFLIGHT_PARTS`, `W_REF`, `knob_clamps_hold` → no
  matches) — so there is no merged prior art to duplicate. The *related* merged art is
  `metadata.rs`'s own capacity constants from #648 (`3e05891`), which this slice **consumes** rather
  than restates. No open PR touches these paths (`gh pr list --state open` → empty).
  **Closed/rejected:** the #508 line (seven attempts, rejected on reviewability) and **#636** itself
  (three Do rounds, discontinued at the 2026-07-30 sign-off **for size, not direction**, with the
  instruction to split into these seven slices). That patch is *salvage*, not a rejected approach.
  Its recorded findings that bear on this slice: the false `503 SlowDown` from a single shared retry
  budget (leg 5), and the `mpuctl` relational-validation C5 whose derivation side lands here.
- **Disposition hint:** new-feature

## Motivation

0016 spends four hundred lines deriving these numbers because getting one wrong is not a
performance regression — it is a permanent failure. `MAX_PART_CHUNKS` above the operation budget
makes a *valid* part's commit **and its compensation** both time out forever, leaving the slot and
the staged residue with no path that clears them (`0016:1466`, X98). `MAX_INFLIGHT_PARTS` chosen
against the scan cap alone permits values whose whole-range fence blows the transaction envelope,
leaving an abandoned session permanently unfenceable *and* undeletable, holding its admission count
forever (`0016:1471`). A hard-coded `MAX_SESSIONS` decouples what is admitted from what the
reconcile pass can hold in RAM. Each of these is one integer.

They also cannot each be chosen where they are used. The reaper (#625) needs `W_ref` and `B_ops`;
this seam's clamps need the same two; the gateway (#508) refuses parts against `max_part_bytes`. If
three slices each derive their own, the composition is unchecked — and the failure it admits is the
one `docs/principles.md:138-143` records as the reason the §6 invariant row exists. One slice picks
the set, one function asserts the set is consistent, and everyone else consumes it.

Doing that in its own bundle — rather than folded into #654 — keeps the two review questions apart:
*"is this record shape right?"* and *"is this number derived correctly?"* are answered with
different evidence, and #636's four rejections were all reviewability failures.

## Design

The design is proposal 0016's, unchanged; see the header block for anchors and § Citations expected
for what Do must open. What is settled **here**:

1. **Where the numbers live:** appended to `crates/core/src/multipart.rs`, never in `metadata.rs`
   (which #682 owns this wave). The metadata-level ceilings are imported, never restated.
2. **The format/capacity boundary** the #654 chain established: this slice picks capacities and
   clamps each against #691's key-space widths (`PART_NUMBER_WIDTH = 6`, `SLOT_INDEX_WIDTH = 6`);
   it defines no key grammar.
3. **The two borrowed inputs** (`W_ref`, `B`/`B_ops`) are defined here, with #625 named in the doc
   comment as their consumer, because a split budget authority is the failure this whole invariant
   row exists to prevent.
4. **The doc comment is the deliverable**, held to `metadata.rs:302-356`'s standard: the budget
   rule, what the reserve is spent on, why it is not a decode invariant, and where the worst case is
   *measured*. A constant whose comment restates its value in words has not been derived.
5. **`knob_clamps_hold` returns which clamp failed**, so leg (2) can assert the specific inequality
   and so a future value change fails with a message an operator can act on.

## Alternatives considered

- **Fold this into #654** — the issue explicitly invites it (*"Fold into it only if the pair stays
  within the ~1,500 added-semantic-line budget"*). **Declined:** #654 is already budgeted at the
  full 1,500 for the record family alone, and this slice's ~31 derivations would roughly double it,
  past the reviewability ceiling four prior rejections established for exactly this work. The
  format/capacity boundary makes the seam between them clean, and waves make the ordering automatic.
  **This is a scoping decision the human confirmed at Plan** — if it should be folded, say so before
  Do starts.
- **Compile-time `const` assertions only, no test file.** Attractive (a compile-time tie is stronger
  than a test, and `metadata.rs:354` does exactly that) but insufficient alone: a `const` assertion
  cannot express leg (2)'s *rejection* table over hypothetical budgets, which is the only thing that
  proves `knob_clamps_hold` is not vacuous. Ship both — `const` ties for what is statically
  expressible, the test file for the rest.
- **Leave `W_ref` / `B_ops` to #625 and use placeholders here.** Rejected by 0016's own reasoning
  and by `docs/principles.md:138-143`: a placeholder is a second authority, and the clamps that
  depend on it would be unverified until a slice that builds later. If a value truly cannot be
  chosen, that is a §6 item, not a placeholder.
- **Deriving `MAX_SESSIONS` as a chosen constant.** Rejected outright by `0016:1469-1470`: it is
  `min(⌊W_ref/U_ref⌋, SCAN_CAP/2)`, and a hard-coded value is a defect.

## Impact & compatibility

- **Purely additive and inert.** Nothing enforces these values yet, no on-disk shape changes, no
  existing code path is touched. The compatibility surface is entirely forward: #656–#660 and #625
  consume this set.
- **One forward-compatibility rule to state in the code:** `MAX_INFLIGHT_PARTS` **defines** the
  `slot:` key space, so raising it is unconditionally safe while lowering it leaves live sessions
  holding indices above the new cap until those parts finish — a transient over-cap bounded by the
  *old* value (`0016:1471`). That is a rollout note, not a correctness case, and it belongs in the
  doc comment.
- **Docs currency:** none expected. These constants are internal and unobservable until a later
  slice enforces them; do not add architecture-doc paragraphs describing behaviour no code produces.

## Open questions

- **None blocking Do.** Two for sign-off: (a) whether `W_ref` should be a compile-time constant at
  all, or a deployment input with a compile-time default — 0016 sizes it from host RAM and assigns
  it to #625, so the doc comment must be explicit either way; (b) the concrete value of `B_ops`,
  which 0016 says is *"calibrated to keep a batch's sequential in-transaction round trips inside the
  5-second half of the envelope on the slowest supported backend"* (`0016:1475`) — a calibration
  this slice cannot run, so state the chosen number, its basis, and that #625 must re-check it
  against a measured backend before relying on it. Record both in `build-notes.md`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
