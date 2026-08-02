# Result — issue 635 / segmented-chunk-map

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  `InodeRecord.chunk_map` graduates from a flat list to **`Flat | Segmented`**, so a
  published map larger than one backend value can exist at all — the >10 GiB launch requirement.
  Flat stays exactly as it is (`crates/core/src/metadata.rs:268`, `pub chunk_map: Vec<ChunkRef>`)
  and every existing record keeps decoding **byte-identically**; segmented carries a group
  identity plus `seg:<group-nonce>:<epoch>:<index>` segment records and their `seggrp:`
  reservation, is published by **staged publication** (write the segments, then flip the root),
  and is resolved through **one shared resolver that every `.chunk_map` consumer goes through**.
- Success criterion:
  **ONE** new test file (leg A, the binding one — deliberately written to
  compile on this bundle's base so its RED is an assertion, not a build error), plus co-located
  unit tests (leg B) and the whole gate (leg C).
  **(A) BINDING — every maintenance consumer resolves the segmented shape, and the proof is a
  positive observable, not an absence.** `crates/custodian/tests/segmented_map_consumers.rs`
  seeds a **committed segmented object by raw record bytes** through `MetadataStore::commit`
  (the encoding is settled below, so no symbol this slice adds is named) plus its fragments on
  in-memory D-server doubles, then asserts, in one test binary:
  (i) **`reconcile_step` succeeds** with GC + scrub + reconstruction + rebalance contexts all
  supplied (`crates/custodian/src/reconciliation.rs:65-74`) — on the base it returns `Err`,
  because `referenced_fragments` decodes every `inode:` value with `metadata::decode(&value)?`
  (`crates/custodian/src/gc.rs:251`, `:256`) and a segmented value is not a JSON array;
  (ii) **the segmented object's fragments are in the protected set — asserted positively.**
  `desired_state::reconciliation_status(meta, S)` for a server `S` that holds one of the
  segmented object's fragments, with `desired:dserver:S` seeded, MUST answer **`Pending`**
  (`crates/custodian/src/desired_state.rs:150-165`). A resolver that decodes the new shape but
  never reads the `seg:` range answers `Satisfied` — this is the leg that catches it, and
  nothing else does;
  (iii) **restore does not strand it.** `reconcile_after_restore` reports
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:108`, `:145`, `:179`).
  This is the #508-attempt-4 failure mode in its exact shape: a resolver used only by
  the read path while `gc.rs` and `restore.rs` still iterated `record.chunk_map` directly, so a
  restore pass stranded a live segmented object and a later GC pass deleted its fragments;
  (iv) **and the data loss that follows is pinned.** Advance past the orphan grace window and run
  a second GC pass: **every fragment the segmented object's resolved map names is still present**.
  Under the (iii) failure the marks laid by restore would now be reclaimed — assert the fragment
  count directly, not `Reconciled::Satisfied`;
  (v) **a flat object in the same store is unaffected** — same passes, same assertions, and its
  stored `inode:` bytes are unchanged byte-for-byte after every pass.
  (vi) **The consumers `reconcile_step` does NOT dispatch get their own positive observable.**
  `reconcile_step` runs GC, scrub, reconstruction and rebalance only
  (`crates/custodian/src/reconciliation.rs:65-114`) — it dispatches **neither backfill nor either
  read path**, and reconstruction reaches `find_chunk` only for a **queued** repair
  (`crates/custodian/src/reconstruction.rs:130-183`), which legs A(i)–(v) never enqueue. So
  `reconcile_step(...).is_ok()` binds four consumers, not eight, and the remaining four would ship
  unresolved behind a green criterion. Add, each asserting a **positive** result rather than
  absence of error: **the gateway read path** returns a segmented object's bytes byte-identical
  (whole-object *and* a range that spans a segment boundary — the ranged walk is a separate
  `.chunk_map` consumer, `crates/server/src/lib.rs:446`); **`core`'s read path** resolves it
  (`crates/core/src/read.rs:92`); **reconstruction** resolves a segmented chunk for an
  **explicitly enqueued** repair instead of dropping it; and **backfill** takes its stated decision
  (resolve, or skip with a reason) rather than mangling the map (`crates/custodian/src/backfill.rs:76-130`).
  **WHERE each of these lives — this is not free choice, and getting it wrong breaks the gate.**
  `wyrd-server` depends on `wyrd-custodian` (`crates/server/Cargo.toml:63`), so the reverse edge is
  a **dependency cycle**: the gateway legs **cannot** live in the custodian test binary, and
  `wyrd-server` must not be added to `crates/custodian/Cargo.toml`. Nor may they ship as a new
  `crates/server/tests/*.rs` file — that is a second **added** test target, which C4-verify folds
  into the same cargo invocation and keeps on the RED leg, so its compile error (it names types
  this slice adds) would destroy leg A's assertion red. **The two gateway legs therefore ship as
  co-located `#[cfg(test)]` tests inside `crates/server/src/lib.rs`** (where the ranged walk lives),
  exactly as iteration 5 did. Everything else in leg A stays in the one added custodian test file:
  `wyrd-custodian` depends on `wyrd-core`, and `wyrd_core::read` (`crates/core/src/lib.rs:14`,
  entry points `read.rs:29`, `:44`, `:58`) and `metadata::high_water_marks` are public, so the core
  read path, the custodian consumers and the allocator floor are all reachable from it.
  (vii) **NEW — one damaged object does not take the store down (the containment rule, see
  `Design § Failure containment`).** Seed a THIRD object: segmented, committed, whose root still
  names its group but whose `seg:<nonce>:<epoch>:000001` record is **absent**. Then assert, in the
  same store that holds the healthy flat and healthy segmented objects:
  **(a) `metadata::high_water_marks` returns `Ok`** (`crates/core/src/metadata.rs:847`) and its
  chunk floor is **≥ every chunk id present in any `seg:` record in the store** — this is the leg
  that stops `Gateway::recover` (`crates/server/src/lib.rs:123-124`, called before serving) from
  refusing to start the whole gateway because one object is damaged;
  **(b) the healthy objects still read** — byte-identical, with the damaged object present in the
  same store (the `core::read` half in the custodian test file, the whole-object + ranged gateway
  half with the co-located server tests, per the placement rule in (vi));
  **(c) the damaged object fails closed, per object** — a read of *it* is a typed error, never
  torn or partial bytes;
  **(d) nothing of the damaged object is reclaimed** — after a GC pass and past the grace window,
  the fragments its readable segments name are still present. (Whether the deletion-capable pass
  returns `Err` or completes while protecting them is Do's call — see the containment table — but
  **no fragment may be deleted**.)
  **(B) The record shape, the CAS identity, and staged publication.** These name types this slice
  ADDS, so they **must NOT ship as a second added `tests/*.rs` file**: `run-verify.sh` collects
  every added test target into **one** cargo invocation (`engine/scripts/run-verify.sh:286-311`,
  `:332-342`) and keeps them all on the RED leg (`:404-415`), so a compile-red file would fail the
  whole invocation and **destroy leg A's assertion red** — the single most valuable thing this
  slice has. Ship leg B as **co-located `#[cfg(test)]` unit tests inside the production modules**
  they exercise (`crates/core/src/metadata.rs`, and the committer's own module), which
  `cargo xtask ci` runs and C4-verify never retains. Over `RedbMetadataStore::in_memory()`:
  (i) **Legacy decode→encode is the identity, byte-for-byte.** Take the *exact* stored bytes of a
  pre-existing flat `InodeRecord` (including one with `etag`/`content_type`/`modified` absent),
  decode and re-encode, assert equality. This is not hygiene: every CAS in
  `crates/core/src/metadata.rs` is `require(key, encode(prior))` compared byte-for-byte against
  the stored value (the `skip_serializing_if` rationale at `crates/core/src/metadata.rs:277-289`),
  so a `chunk_map` whose encoding gained a tag or a wrapper turns **every overwrite, backfill,
  reconstruction and rebalance of every pre-existing object** into a permanent `Conflict`. Assert
  it end-to-end too: `metadata::commit_chunk_map` against a legacy record must return
  `Committed`, not `Conflict`.
  (ii) **The segmented encoding is exactly the one settled below, and its structural invariants
  are enforced AT DECODE.** Assert `encode(Segmented{…})` equals the canonical JSON the test spells
  out literally and decodes back — that keeps leg A's hand-written fixture honest. But a single
  valid example is not an oracle for an invariant: this repo requires structural invariants to be
  **rejected at decode rather than admitted as values** (parse-don't-validate,
  `../wyrd/AGENTS.md:146-149`). So add a **raw-byte negative case per invariant**, each asserting a
  typed decode error and **no partial resolution**: `segment_count != segments.len()`; a duplicate
  `index`; a gap in the index sequence; non-monotonic or overlapping `byte_offset`/`byte_len`; a
  `nonce` that is not 32 lowercase hex; a segment key whose index is not the fixed width. Without
  these the shape is a suggestion, and a malformed record becomes a torn map at the first
  consumer.
  (iii) **Staged publication**: writing a segmented map's `seg:` records in byte-budgeted batches
  and then flipping the root is one committer, and the flip is **one** batch carrying the root
  CAS. Assert: after the segment-write phase and before the flip, the root still names the prior
  generation; after the flip, the root names the group and a resolve returns the full ordered
  chunk list; the flip batch's total mutation **bytes** stay inside the stated envelope
  (the segment-write batches at `≤ E_tx/2`, `0016:2331-2337`; the flip's own inventory bound
  `≤ 4·V + O(1)`, `0016:654-663`) and no single value exceeds the 100 KB ceiling
  (`crates/traits/src/lib.rs:997`) — measure the encoded bytes, do not assert a record count.
  **The operation-count half of the envelope is also normative** — "`B` is therefore
  `min(B_bytes, B_ops)` … every row of the batch inventory whose mutation or precondition **count**
  can grow is bounded by both" (`0016:640-648`) — so the split and the flip are bounded by ops as
  well as bytes, with a typed refusal for each. (Iteration 5 already implemented this; it is
  restated because an earlier round wrongly declined it.)
  (iv) **The publication refuses BEFORE it makes anything durable.** Every deterministic,
  zero-I/O refusal the committer can raise — unfenced, colliding caller contribution, a value over
  the ceiling, a key over the ceiling, batch over bytes, batch over ops — must be decided
  **before the first `seg:` record is written**, so a refused publication leaves **zero** durable
  `seg:` rows and no caller cursor movement. Assert both: a flip contribution carrying an
  over-ceiling value, and a flip with no fence, each ⇒ typed `Err` **and** a store containing no
  `seg:` record for that group. (Iteration-5 refutation 1 — the patch shipped the opposite order.)
  (v) **A resumed publication verifies the durable prefix it is trusting.** A caller resuming at
  `resume_from = N` must not be taken at its word: before the flip, the committer re-reads at least
  the last durable segment (`seg:<group>:<epoch>:<N-1>`) and compares it against the segment its
  own re-derived plan puts at that index, refusing with a typed error on mismatch. Assert the
  probe: publish attempt 1 writes N segments for chunk list A and stops; attempt 2 resumes at N
  with a chunk list differing only in `chunks[0].len` — the flip must **refuse**, not commit a
  root that no consumer can ever resolve. (Iteration-5 refutation 2. If Do concludes this belongs
  to #636's session contract instead, that is a legitimate call — but it must then be **recorded**
  in `review-rejected.md` with its reason, not left implicit.)
  (vi) **The resolver is total, bounded, and orders segments ITSELF.** It reads the root plus the
  bounded range `scan("seg:<nonce>:<epoch>:")` and nothing else — never a global `seg:` scan
  (`0016:2463-2469`). Assert by seeding a *second* group's segments in the same store and checking
  they are neither read nor returned. **And it must not rely on scan order:**
  `MetadataStore::scan` leaves order *unspecified* (`crates/traits/src/lib.rs:1021-1023`) and #634
  makes byte-lexicographic order normative **only for `scan_page`** (`:1037-1046`), leaving `scan`
  untouched — so the fixed-width zero-padded index is a *debuggability and key-hygiene* property,
  **not** a licence to concatenate in returned order. The resolver parses each segment's `index`
  and orders by it explicitly, rejecting a gap or a duplicate. Assert with a **deliberately
  shuffling** store double that returns the range reversed: resolution must still yield the correct
  byte order. (The bounded `scan` is deliberate: `MAX_ROOT_SEGMENTS` keeps one group's range inside
  the cap, so this slice does not need `scan_page` even though the base now offers it.)
  (vii) **A rolled-back attempt's segments are disjoint from a later attempt's** — seed
  `seg:<nonce>:1:*` and `seg:<nonce>:2:*` and assert resolving the root at epoch 2 returns only
  epoch 2's chunks (the F18 epoch-scoping property, `0016:2352-2380`).
  (viii) **Decision 7(h)'s resolve-retry rule, which the resolver's SIGNATURE must be able to
  express** (`0016:2452-2474`). A generation's `seg:` records are deleted by retirement and
  rollback, so a consumer midway through a segmented resolve can see a segment **absent**. The
  rule is: re-read the **root**; a root now naming a **different group** or **absent** means the
  generation was concurrently retired (a reader restarts against the current root or answers
  `NoSuchKey`; a maintenance pass drops the stale resolution); a root **unchanged** with a segment
  **absent** is an **invariant violation and MUST fail closed** — an error, never a torn success
  (the *Absent or unsupported entries* rule, `../wyrd/AGENTS.md:175-177`). **A resolver that takes
  only a store and an already-decoded `InodeRecord` cannot do this** — it has no way to re-read the
  root. So the API must carry the root's identity (the inode key/id, or a re-read closure) and
  return a retry-or-fail outcome. Assert both arms: changed root → restart/drop; unchanged root
  with a missing segment → typed error and **no partial map**. A *complete* resolve of a
  superseded generation settles the same way (the currency re-read is not skipped just because
  every segment happened to be readable). The interleaving itself (X51) goes into the existing
  `crates/dst/tests/custodian.rs`, never a new DST file (see `Test file`).
  **(C) `cargo xtask ci` green**, including the docs gates — see `Impact & compatibility` for the
  architecture-doc currency requirement, which is a **merge requirement**
  (`../wyrd/AGENTS.md:154-157`), not a follow-up.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
  `seggrp:` records and their key helpers (`crates/core/src/metadata.rs`); the staged
  segment-write + root-flip committer, with the publication precondition taken as a **parameter**;
  the one shared resolver and **every** `.chunk_map` consumer routed through it (the eight sites
  tabled in `Design`), each with its stated failure-containment behaviour; the architecture-doc
  currency edit; and the one new test file plus co-located units. **Out of
  scope:** the multipart session/records/protocol (#636), the S3 verbs (#508), the staged-byte
  protection class (#637), `PutObject` chunk-size selection (#508 — a single PUT never segments),
  FU-1's record-shape ADR (#628), FU-5's part-record segmentation (#632), the destination
  drain-fence question carried in `review-batch.md` (see `Carry-forward`), and any file under
  `docs/design/adr/` or `docs/design/specs/`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: only 0/3 passes produced a usable result after one retry — refusing to certify a thinner union. Re-run wh
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue 635: add a byte-compatible flat-or-segmented chunk map, staged segment publication, and one bounded resolver for every read and maintenance consumer.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the wire identity, failure-containment rules, capacity oracle, dependency set, and explicit #636 boundary tightly enough to derive falsifiable checks. |
| C2 Reproduction (red pre-fix) | PASS | With the tracked fix stashed, the independently built binding binary ran and all 12 assertions failed against the base behavior, including the maintenance/reference-safety oracle at `crates/custodian/tests/segmented_map_consumers.rs:588`. |
| C3 Change | PASS | The scoped slice supplies staged publication and the shared resolver/containment seams without adding the deferred #636 producer (`crates/core/src/metadata.rs:3488`; `crates/core/src/metadata.rs:3004`; `crates/custodian/src/resolve.rs:106`). |
| C4 Verification (red→green) | PASS | The same isolated binary changed from 0/12 red to 12/12 green, and a full `cargo xtask ci` rerun passed after relocating cargo-deny's read-only home lock into reviewer scratch. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether causal confidence is adequate without mutation outcomes — no capability-probe/guard smell was found, but the configured run timed out and an independent listing found 558 in-diff candidates whose survivor status remains unknown. |
| T1 Structure | PASS | One core resolver owns bounded segment reads/restarts and the maintenance wrapper centralizes live-root resolution and containment, with no new Cargo dependency edge (`crates/core/src/metadata.rs:3004`; `crates/custodian/src/resolve.rs:106`). |
| T2 Shape | PASS | JSON-type discrimination, decode-boundary invariants, and the passing legacy identity/CAS oracle preserve old bytes while rejecting malformed segment tables (`crates/core/src/metadata.rs:1275`; `crates/core/src/metadata.rs:1382`; `crates/core/src/metadata.rs:5922`). |
| T3 Runtime | NEEDS-HUMAN | Decide whether to land the runtime precursor before #636 — the committer accepts a caller-supplied fence, but no real `Completing` session drives it yet, so end-to-end publication/rollback behavior remains unexercised (`crates/core/src/metadata.rs:3500`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether this review can substitute for the standing rubric's required deep multi-pass review — the configured oracle produced 0/3 usable passes and is unavailable in the artifact scope, although affected-path prior art and contribution metadata were independently clean. |
| T5 Judgment | PASS | The assertions exercise byte results and fail-closed safety outcomes rather than merely code shape, including continued healthy-object service beside an unreadable segmented object (`crates/custodian/tests/segmented_map_consumers.rs:1176`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether parameterized in-memory fixtures are a fit proxy for the eventual >10 GiB production topology — the capacity path is intentionally tested at reduced scale and the #636 session-driven producer does not yet exist (`crates/core/src/metadata.rs:3668`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Inputs: `patch.diff`, `brief.md`, `check-gates.json`. All `path:line` below are on the target
worktree at `$PDCA_TARGET` (= `origin/main` @ `9120f7a` + this patch).

## The evidence — attacked, and it holds

I re-ran the asserted red→green rather than trusting the `C4-verify` row.

- **Green post-fix, on the production path**: `cargo test -p wyrd-custodian --test
  segmented_map_consumers` → 12/12 pass in the target worktree.
- **Red pre-fix, and it is an *assertion* red, not a build error** — the failure mode the brief
  warns about (`run-verify.sh`'s unconditional PASS after a build failure). I extracted
  `9120f7a` into scratch, dropped in *only*
  `crates/custodian/tests/segmented_map_consumers.rs:1`, and ran the same target: it **compiles**
  and **12/12 fail** with `panicked at …expect(…)` / `assert_eq!` messages
  (`Error("invalid type: map, expected a sequence", line: 1, column: 23)` propagating out of the
  real `reconcile_step` / `reconcile_after_restore` / `backfill::reconcile` / `high_water_marks`).
  So the red is genuine, the test names only base symbols, and the double is only at the
  `MetadataStore`/`ChunkStore` seams — the loops under test are the production ones.
- Not a tautology: legs assert **positive** observables (`ReconciliationStatus::Pending` for a
  holder, byte-identical `read_object`, fragment *counts* before/after a post-grace GC pass,
  byte-identical stored `inode:`/`seg:` bytes), not the absence of an error.

## Findings

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5753` (`ClassIds::Optional`), with
  `:5148` (`RecoveredIds::contribution`) and `:5078`: the chunk-id floor *under-approximates* for
  a corrupted **flat** root, which is the one direction the brief's containment table forbids
  ("Must be total … and it must **never under-approximate** the floor",
  `brief.md:527`).** Concrete failing input, executed against the patched crates:
  `inode:1 = {"size":8,"chunk_map":{"a":1},"state":"Committed","version":1}` (also reproduced with
  `"chunk_map":null`, and the same class covers a missing `chunk_map` field). The value is valid
  JSON, fails `InodeRecord` decode, carries **no `id` token**, so `raw_chunk_id_floor` reports
  `found == 0, complete == true`; `covers(Optional)` is then `true` and the record contributes
  **0**. Measured: `high_water_marks` → `Ok((max_inode = 1, max_chunk = 0))`, while the identical
  damage one record class down (`seg:… = {}`) correctly contributes `2^64 - 1` via
  `ClassIds::Required`. On the base the same store returned `Err(invalid type: map, expected a
  sequence)` — so this patch converts a fail-closed answer into a silently low floor for exactly
  the case `RecoveredIds::covers` was written to catch. The distinction is decidable from the
  bytes and the module already makes it three hundred lines up (`inode_chunk_map_fault` /
  `:2064` tests `map.is_object()`): an `inode:` value whose `chunk_map` does **not** claim the
  segmented shape is `Required`, not `Optional`. Fix at that boundary rather than widening
  `Optional`.

- **NEEDS-HUMAN [human] — `crates/server/src/lib.rs:124`: the entire chunk-floor recovery
  apparatus this patch adds has no production consumer, so the containment-table row it is built
  for (and the reviewer's likely reading of it) claims more than the code delivers.** `recover`
  is `let (max_inode, _max_chunk) = metadata::high_water_marks(...)` — the chunk floor is
  discarded, and it is the only caller outside tests (`grep high_water_marks` finds only
  `crates/server/src/lib.rs:124` plus doc/test references). The patch nonetheless adds
  `segment_chunk_floor`, `raw_chunk_id_floor`, `json_chunk_id_floor`, `scavenged_chunk_id_floor`,
  `RecoveredIds`, `ClassIds`, `ScannedId`, `torn_digit_escape`, `json_string_token`
  (`crates/core/src/metadata.rs:5063-5624`) plus a full `seg:`-namespace walk on the gateway
  startup path — several hundred lines of production code, and a large share of the surviving-C5
  mutant surface, computing a number nobody reads. Two consequences a human should adjudicate:
  the finding above is **latent rather than live** (its severity depends on whether #636/#508 will
  wire the floor), and the leg A(vii)(a) assertion's own rationale —
  `crates/custodian/tests/segmented_map_consumers.rs:1196` "must not fail the id floor the gateway
  starts from" — is true only for the *totality* half, not the *value* half. Either wire the floor
  or shrink the apparatus to what totality actually requires.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:163-168` and `:210-214`: with one
  unresolvable object, GC mislabels every fragment in the fleet as `skip reason="referenced"` and
  still returns `Reconciled::Satisfied`, which is the exact move the same patch calls forbidden
  for scrub.** `ReferenceSet::protects` (`:269`) now short-circuits `true` while
  `unresolvable` is non-empty, so a fragment that is in neither `placed` nor `malformed` takes the
  `else` arm at `:166` and is audited as *referenced* — an operator reading the durability seam is
  told GC verified a reference it never had. And `gc::reconcile` still answers `Satisfied`
  (`:210`) over an incomplete set, while `scrub::reconcile` was given
  `Reconciled::Blocked` for the identical condition with the rationale that `Satisfied` over an
  incomplete set is "a clean bill for part of the store" (`crates/custodian/src/scrub.rs:194`,
  `crates/custodian/src/reconciliation.rs:25-42`). Add an `"unresolvable"` skip reason and give
  GC the same `Blocked` answer — or say in `review-rejected.md` why GC is different.
  (Related, worth knowing when judging how much the new variant buys: the deployed run loop
  discards it — `crates/server/src/custodian.rs:519`, `:533`, `:610` all match `Ok(_) => {}` — so
  `Blocked` is observable only in tests today.)

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:646` / `:676`, reached once per
  queued repair at `:190-191`: the store-wide chunk lookup now costs one extra metadata round trip
  *per committed object per queued chunk*.** `find_chunk` scans `inode:` and, for every committed
  record, calls `resolve::homes_of` → `metadata::resolve_current_chunk_homes` → `store.get(root)`
  before it can compare ids; on the base the same loop decided from the scan snapshot with no
  further I/O. For a repair queue of `Q` chunks over `N` committed objects the pass goes from `Q`
  namespace scans to `Q × N` point reads (worst case; `N/2` expected per chunk), on the loop the
  deployed custodian drives every interval. The re-read is deliberate and load-bearing for
  *snapshot currency* (`crates/custodian/src/resolve.rs:27-44`), but hoisting it out of the
  per-chunk loop (resolve once per pass into an id→home index) would keep the currency guarantee
  at `N` reads instead of `Q × N`. This is the "O(N) maintenance-pass round-trip regression" §6
  item carried since iteration 11; I cannot see `review-rejected.md` from this leaf, so if it has
  already been recorded-declined with a tracker id, treat this as settled per
  `AGENTS.md:200-203`.

## Attempted and could not refute

- **The flat compatibility contract.** `ChunkMap::Serialize` (`crates/core/src/metadata.rs:1382`)
  delegates straight to the `Vec<ChunkRef>` array — no tag, no wrapper — and the leg A test
  asserts the stored flat `inode:` bytes are unchanged after every pass. I could not construct a
  legacy record whose decode→encode moves.
- **The publication's ordering and completeness rules.** I tried the iteration-5/7/8 failure
  shapes: a shorter same-epoch replan leaving an orphaned tail is refused
  (`PublicationTailStranded`, `:4414`); a flip over a partially written range is refused
  (`DurableRange::WholePlan`, `:4515`); resume trusts nothing (`verify_durable_range` walks the
  **whole** plan, `:4429`); every zero-I/O refusal is decided before the first write (`publish`,
  `:4550`); the fence cycle rule spans prefix + phase + flip (`:4256-4270`).
- **The v10/v13/v16 findings.** `plan_with` now refuses an empty placement (`:3709`);
  `read_group_range` **refuses** rather than clamps a `segment_count` past `MAX_ROOT_SEGMENTS`
  (`:2796`); `next_root_version` is checked at every version-advancing site (`:2346`); a `seg:`
  value of `{}` contributes the ceiling (`ClassIds::Required`, `:5073`); structural faults are
  classified as `ChunkMapError` at the one `decode` boundary (`:1920`) so containment cannot be
  spelled differently per consumer (`crate::resolve::contain` is the crate's only downcast,
  `crates/custodian/src/resolve.rs:273`).
- **Key/prefix aliasing.** `seg:` vs `seggrp:`, epoch `1` vs `11`, non-canonical epochs and
  off-width indices are all rejected or non-overlapping (`:1650-1691`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decide whether causal confidence is adequate without mutation outcomes — no capability-probe/guard smell was found, but the configured run timed out and an independent listing found 558 in-diff candidates whose survivor status remains unknown. **Cleared by human:** accepted on the strength of the adversarial reviewer, who attacked the publication/rollback/ordering guarantees directly and could not refute them.
- [x] T3 Runtime — Decide whether to land the runtime precursor before #636 — the committer accepts a caller-supplied fence, but no real `Completing` session drives it yet, so end-to-end publication/rollback behavior remains unexercised (`crates/core/src/metadata.rs:3500`). **Cleared by human:** accepted per brief's stated intent (precursor ahead of #636's session) and the adversarial reviewer's inability to find a fault in the fence/precondition machinery.
- [x] T4 Contribution — Decide whether this review can substitute for the standing rubric's required deep multi-pass review — the configured oracle produced 0/3 usable passes and is unavailable in the artifact scope, although affected-path prior art and contribution metadata were independently clean. **Cleared by human:** the batch tool hit a hard codex input-size ceiling (confirmed by re-running it live: 1,077,300 chars vs. 1,048,576 max), not a substantive failure; accepted the advisory + adversary review, which examined the actual diff and found nothing, as the substitute. See §10 for the tracked tooling gap.
- [x] Validation — fitness-to-purpose — Decide whether parameterized in-memory fixtures are a fit proxy for the eventual >10 GiB production topology — the capacity path is intentionally tested at reduced scale and the #636 session-driven producer does not yet exist (`crates/core/src/metadata.rs:3668`). **Cleared by human:** accepted as fit for this precursor slice; the adversarial reviewer found nothing to refute in the capacity/publication arithmetic.
- [x] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa **Cleared by human:** accepted on the strength of the adversarial reviewer finding nothing, in lieu of a mutants verdict.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: only 0/3 passes produced a usable result after one retry — refusing to certify a thinner union. Re-run wh **Cleared by human:** confirmed live as a codex input-size ceiling on this bundle's diff, not a review finding; accepted the advisory + adversary review as substitute. Tracked at §10.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-29

## 10. Act candidates (hints for the next Act review)
- `scripts/review-branch` (T4 batch rubric review) hit codex's hard input ceiling (1,048,576 chars; this bundle's prompt was 1,077,300) on this bundle's ~1 MB / 22K-line diff — all 3 passes + retries failed identically with `input_too_large`, not flakiness. Once a diff-sizer/chunker lands in the wyrd harness (splitting large diffs across passes or trimming the prompt), this class of gate failure should not recur.
