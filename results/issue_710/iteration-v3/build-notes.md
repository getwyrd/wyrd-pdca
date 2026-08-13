# Build notes — issue #710 (iteration 3)

Target branch: `getwyrd/wyrd @ main` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base
`9dbcd72`, which already carries the merged #696/#697 PRs #705/#706). Every `path:line`
citation below is **post-patch**, against that worktree.

---

## 1. What the carry-forward asked for, and what I did about each

| Carry-forward item (round 2) | Disposition |
|---|---|
| **4× review BUG (gating T4)** — the ceiling check ran **before** the source/target availability, fragment-integrity and fleet-membership checks, so a compound failure (an oversized record *and* an unreachable/off-fleet/missing fragment) was misreported as the **permanent** "record must shrink" refusal instead of the transient `Aborted` (`rebalance.rs:469`, `reconstruction.rs:896/897`) | **Fixed at the ordering, not with a filter.** Each loop now runs its transient checks **first** — resolving stores + intact bytes without writing them — and only then weighs the ceiling: `rebalance.rs:448-483` then `:504-519`; `reconstruction.rs:853-887` then `:915-929`. The refusal still happens **before any write** (that was iteration 2's fix and it is preserved), so both properties hold at once. |
| **C5 Causal adequacy [impl]** — "decide and test compound-failure precedence" | **Decided** (transient wins: it is recoverable, and the ceiling verdict is contingent on *which* target the selector picked — moot if that target is unreachable) and **tested twice**, once per loop: `placement_ceiling.rs:460` (drain: the fragment is gone from the draining server → aborted, and `rebalance_ceiling_refused` is **not** emitted) and `placement_ceiling.rs:486` (repair: a chunk whose root is AT the ceiling *and* whose only free domain is off-fleet counts as `aborted`, not `ceiling_refused`). |
| **C5 mutants (advisory, red)** — 2 surviving (`delete field size` / `state` in the `InodeRecord` literal iteration 2 moved) | **Gone: 0 missed.** `scripts/mutants-in-diff` now reports **20 mutants: 11 caught, 9 unviable, 0 missed.** The literals no longer move — see §3. |
| **T2 Shape [human]** — 549-line test file, 384 nonblank/noncomment lines vs the ≤250 budget | **Reduced ~40%**: the new test file is 554 lines / **362** nonblank-noncomment (iteration 2: 549 / 385) *while carrying one more leg*, and total added semantic lines fall from 510 → **459** (**262** excluding the mechanical harness). Full arithmetic and the honest gap in §6. |
| **T4 Contribution [human]** — no `scripts/pdca` / closed-work index on the target to reproduce the contribution checks | Unchanged and not mine to fix: that is tooling absent from the *target* repo, not a property of this patch. It stays a sign-off item. |
| **T5 (deferred to sign-off)** — are lost-CAS conflicts intentionally excluded from "a move that did not persist"? | Still answered in code, unchanged from iteration 2 (reviewers' round-1 findings asked for it, round 2 raised nothing against it): they are **not** excluded. `EvacOutcome::persisted()` (`rebalance.rs:412`) makes it structural. The human still owns the judgment; the two rewritten pins state the new meaning in place. |

---

## 2. The change

**One rule, two write paths:** *a placement write that would not survive is refused, and a
move that did not persist neither certifies nor counts.*

1. **`crates/core/src/metadata.rs:380`** — `flat_value_ceiling_crossed(&[u8]) -> Option<usize>`,
   sited immediately after the constants and the `const` assertion that ties them (`:327`,
   `:352`, `:354`). Bounds a **flat** record by the FULL ceiling (`MAX_VALUE_BYTES`), not the
   `MAX_ROOT_VALUE_BYTES` half — that half budgets a *segmented* root's segment table against
   the reserve its object metadata spends, and a flat record has no segment table. **No third
   constant invented** (brief scope). `>` not `>=`: a record landing exactly on the ceiling is
   storable, and it is the same boundary the resolver's read side refuses a stored row on
   (`metadata.rs:2493`). ADR-0045's boundary is respected exactly: a *contextual* capacity
   check, strict on the maintenance write path, nothing added at decode and nothing on read.
2. **The two binding commits weigh their own bytes before writing anything.**
   `rebalance.rs:513` and `reconstruction.rs:923` encode `next` and refuse on the ceiling; the
   bytes weighed are the bytes committed (`.put(inode_key, next_bytes)`, `rebalance.rs:532`,
   `reconstruction.rs:940`), so no re-encode can drift past the check. The CAS idiom,
   `version = prior.version + 1` and `..prior.clone()` (ADR-0047 metadata preservation) are
   untouched — the `InodeRecord` literals are byte-identical to the base and did not move.
3. **Ordering (this iteration's fix).** Each loop is now three phases:
   *resolve → refuse → write.*
   - `evacuate_chunk` (`rebalance.rs:448-483`) collects `(source, target_store, frag, bytes)`
     for every fragment it would copy, returning `Aborted` on an off-fleet server, a missing
     fragment or a checksum failure exactly as the base does — then refuses on the ceiling
     (`:513`) — then copies (`:521-527`).
   - `repair_chunk` (`reconstruction.rs:853-887`) collects `(target_store, frag, frag_bytes)`,
     returning `Aborted` on an off-fleet target — then refuses (`:923`) — then writes (`:931-935`).
   So a transient blocker always wins the classification, and a refusal still writes **nothing
   at all** (no stranded fragment for GC to hold with no grace evidence).
4. **New crate-private outcomes** `EvacOutcome::Refused { bytes, ceiling }` (`rebalance.rs:394`)
   and `RepairOutcome::Refused { … }` (`reconstruction.rs:813`), each named on the durability
   seam by `emit_ceiling_refused` (`rebalance.rs:664`, `reconstruction.rs:1156`) at **warn**
   with the record's own length, the ceiling and a NEEDS-HUMAN line — because unlike a conflict
   or an abort this shape never clears on its own.
5. **Accounting**: `reconstruction_ceiling_refused` joins the documented offset identity, which
   becomes `repaired − conflict − aborted − ceiling_refused` (`reconstruction.rs:269-277`,
   `:1022-1024`, `:1132-1134`).
6. **Certification**: reconstruction withholds on a ceiling refusal, folded into the one `hole`
   the segmented refusal already sets (`reconstruction.rs:343`); rebalance withholds on **any**
   non-persisted move (`rebalance.rs:156`, `:176`) — the `EvacOutcome::Aborted => {}` arm #696
   deferred here (`rebalance.rs:169-172`).

### Why the two loops still treat `Aborted` / `Conflict` differently

Deliberate, and stated in both files. Rebalance certifies a **drain** — an operator reads it as
"the box is safe to pull" — so every non-persisted move withholds it; the brief settles that arm
here. Reconstruction's certification is defined over the *reading*, and its transient outcomes
are each named and counted with the obligation left queued as the standing record; the ceiling
refusal joins that reading's `refused` class (a repair the pass **may not perform**, permanent
until the record shrinks), not the transient class. Extending reconstruction's certification to
`Aborted` / `Conflict` would need a 6th file (`tests/reconstruction.rs`), which the brief calls
"the shape is wrong" — and it is not this slice's rule.

---

## 3. Why the mutants went to zero (and the shape that did it)

Iteration 2 hoisted the whole `InodeRecord` literal above the copy loop, so `size:` and
`state:` became *added* lines and `cargo mutants --in-diff` generated two **equivalent**
mutants there (`delete field size` falls through to `..prior.clone()`, same value; `delete
field state` likewise, since only committed records are ever planned). Unkillable, and they
kept the advisory C5 row red.

This iteration moves the **write** instead of the record: the loops keep their read/verify work
where it always was and merely defer `put_fragment`, so the record literals never enter the
diff. The mutants disappear because the lines are no longer part of the change — not because
they were suppressed. Boundary coverage is unaffected: the `>` predicate lives in `wyrd-core`,
whose tests `cargo mutants` runs for a `wyrd-core` mutant, so the killing test is the in-crate
unit test at `metadata.rs:2768` (a `wyrd-custodian` test, however binding, can never catch a
`wyrd-core` mutant). The behavioural at-ceiling leg stays as well — it is what proves the
boundary *through the loop*.

Result: `20 mutants tested in 46s: 11 caught, 9 unviable` — **0 missed**.

---

## 4. The forced refutation of my own test

- **(a) Genuine red?** Yes — proven twice: by hand (`git stash push -- crates/core/src/metadata.rs
  crates/custodian/src/{rebalance,reconstruction}.rs`, test files kept) and by the project's own
  `engine/scripts/run-verify.sh`, which reverts production and keeps only the added test:
  `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red)`, 4 of 5 failing:
  - `a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted` → **FAILED**:
    `100019 bytes stored, past the ceiling` (`placement_ceiling.rs:329`) — the brief's binding
    assertion; the base commits the oversized record.
  - `an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain` → **FAILED**:
    `left: Changed, right: Blocked`.
  - `a_move_that_cannot_reach_its_fragment_is_aborted_not_refused` → **FAILED**:
    `left: Satisfied` — the base certifies a drain that moved nothing.
  - `a_ceiling_refused_repair_is_subtracted_from_the_reported_successes` → **FAILED**:
    `left: Changed, right: Blocked`.
  - `an_evacuation_landing_exactly_on_the_value_ceiling_still_commits` passes on base **by
    design**: it is the admissible-side regression guard, not a discriminator.
  - In the same reverted tree, the two rewritten pins in `tests/rebalance.rs` also go red
    (`:970` and `:1343`, `Satisfied` vs `Blocked`) — the sanctioned pin flip.
  - The file **compiles** against the base: it names no symbol this patch introduces (no
    `flat_value_ceiling_crossed`, no `Refused`, no `persisted`), so C4-verify gets a red, not
    an UNVERIFIABLE (exit 77).
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` — the
  real fenced control point — into `reconstruction::reconcile` / `rebalance::reconcile`. Nothing
  is re-implemented. The counters are read back through the real `DurabilityTelemetry`
  Prometheus surface a deployment scrapes.
- **(c) Fixture includes the fault?** Yes.
  - The oversized record is produced by the *loop's own* re-encode: the fixture pads the record
    the pass **would** write and then seeds the pre-move placement (`placement_ceiling.rs:372-379`),
    so the growth is the production repoint's, not a hand-built value.
  - The chunk under assertion is the one that must be refused; nothing is curated out. The
    leg-3 fixture keeps the repaired, the refused **and** the compound/aborted chunk in one pass
    and asserts exactly which obligations remain queued.
  - Fragments are real RS(2,1) shards through the production encoder (`wyrd_core::erasure::encode`
    + `wyrd_core::write::encode_ec_fragment`) stored in the **real** `chunkstore-fs` backend,
    which verifies identity + checksum on `put` and `get` — a mis-seeded fixture fails loudly
    instead of silently taking a different branch.
  - The compound legs inject a *real* second fault: a fragment genuinely absent from the
    draining server, and a target genuinely absent from the fleet view.

**Runner**: the project's own gate — `./engine/xtask.sh ci` (→ `cargo xtask ci`: fmt, clippy
`-D warnings`, build, workspace tests incl. DST, cargo-deny, conformance, statics) — **`xtask
ci: all checks passed`** on the final tree; `./engine/scripts/run-verify.sh` (C4-verify) PASS;
`./scripts/mutants-in-diff` 0 missed. Iteration used `timeout`-bounded scoped
`cargo test -p wyrd-custodian …` runs (the same command `xtask ci` issues, narrowed).

---

## 5. Cost of the alternatives I rejected (numbers, not adjectives)

- **Filter the misclassification instead of reordering** (keep the ceiling check first, then
  "downgrade" a `Refused` to `Aborted` when a later check would have failed): needs the
  transient checks to run *anyway* to know that, i.e. exactly the same reordering, plus a
  second classification site to keep in sync — strictly ≥ the 12 lines the reorder costs, and
  it guards the symptom rather than removing the cause.
- **Re-read fragments in the write phase instead of holding them** (avoid holding `Bytes` for
  one chunk's evacuated fragments across the refusal): saves 1 line of state and costs a second
  `get_fragment` per fragment plus a TOCTOU window between verify and copy. Rejected: the peer
  path already holds a whole chunk's shards in memory (`reconstruction.rs:843`, `all_shards`),
  and `Bytes` is refcounted; `plan.evac.len()` is bounded by `n` for one chunk.
- **Ceiling check inside a `commit_chunk_map`-style committer** (one site instead of two):
  the brief forbids it — "carve out **only** the ceiling helpers the write path needs — **not**
  the committer around them" — and the two loops do not share a committer today; introducing
  one is a ≥120-line refactor across `metadata.rs` plus every caller of the existing helpers.
- **Dropping leg 3 + the `counter()` helper** to meet the ≤250 budget exactly: ~55 semantic
  lines (`placement_ceiling.rs:485-554` plus the 8-line `counter`), at the cost of the new
  outcome's accounting rule going unpinned — the brief names leg 3 in the Success criterion.
- **A shared `MemMeta` test double** (would delete ~40 mechanical lines from the new file): it
  would be a **6th file** (`tests/common/mod.rs`), which the brief calls "the shape is wrong".

---

## 6. Size: the arithmetic, and the honest gap

Added semantic lines (non-blank, non-comment), measured off `patch.diff`:

| file | lines |
|---|---|
| `crates/core/src/metadata.rs` | 14 (3 predicate + 11 unit test) |
| `crates/custodian/src/rebalance.rs` | 43 |
| `crates/custodian/src/reconstruction.rs` | 35 |
| `crates/custodian/tests/rebalance.rs` | 5 (the two sanctioned pin flips) |
| **production + pins** | **97** |
| `crates/custodian/tests/placement_ceiling.rs` | 362 — of which **197** is mechanical harness and **165** is the five legs' own bodies |
| **total** | **459** (iteration 2: 510) |
| **total excluding the mechanical harness** | **262** vs the brief's ≤ 250 |

The 197 mechanical lines are: the in-memory `MetadataStore` double (≈40 — every `custodian`
integration test rolls its own; there is no shared one and adding one is a 6th file), the `use`
block and consts (≈31), the fleet/topology/record/fragment builders (≈76) and the two pass
runners plus the counter reader (≈50). None of them decide anything.

What I did to get from 510 → 459 while **adding** a leg: dropped the second pass runner into one
shared `pass()` (`placement_ceiling.rs:248`), replaced per-test topology chains with `topo()`
(`:168`), and — the largest single win — bound intermediates so `rustfmt` stops exploding
`assert!`/chain call sites across 5–9 lines each (its 60-char `fn_call_width`), which alone
removed ~80 physical lines without deleting an assertion.

**The gap is 12 lines and it is in the harness, not the assertions.** I did not buy it by
deleting a leg (§5 prices that at ~55 lines and the loss of the accounting pin).

---

## 7. Scope boundaries observed / notes for sign-off

- **5 files exactly**, all named by the brief. No `backfill.rs`, no `repoint_chunk`, no
  segmented addressing, no read-side change, no new dependency (the test's `chunkstore-fs`,
  `tempfile`, `testkit`, `coordination-mem` are existing `[dev-dependencies]` of
  `wyrd-custodian`).
- **ADR-0011 §2** states the offset identity with the three counters that existed when it was
  written; this patch adds a fourth. The brief puts "any new or edited ADR / spec / proposal"
  out of scope, the repo freezes accepted ADRs (`docs-immutability`, AGENTS.md §"Design
  documents"), and the ADR itself names `crates/custodian/src/reconstruction.rs` as "the source
  of truth for their exact emission" — so the fact is recorded there
  (`reconstruction.rs:1145-1155`), which is where AGENTS.md says implementation facts go.
  Note ADR-0011:40 already frames the identity as a convergence indicator with known unoffset
  error paths; this patch *shrinks* that imprecision by offsetting one of them. **If the human
  wants the ADR's prose refreshed, that is a separate, deliberate doc change.**
- **Rubric self-review** (AGENTS.md §"Review rubric & protocol"): one clock — no new clock read;
  trait seams / dependency direction — the predicate lives in the crate that owns the constant,
  custodian consumes it over the existing `wyrd_core::metadata` seam and gains no backend
  knowledge; ADR-0045 metadata-validation boundary — contextual check, strict in maintenance,
  nothing at decode, read path untouched; serialization identity — `..prior.clone()` and the
  `skip_serializing_if` CAS round-trip untouched, and the test asserts the refused record is
  **byte-identical**; *absent or unsupported entries* — the refusal is explicit, named, counted
  and leaves the obligation queued (never a silent skip); *transactions* — the refusal returns
  **before** any store write and before the `WriteBatch` exists, so there is nothing to roll
  back; no new `unsafe`, no DST-reachable global state, no port/RPC/CLI/persisted-field change
  that would trip the docs-currency rule.
- **Operator-visible change to weigh at sign-off** (unchanged from iteration 2): a rebalance
  pass whose only evacuation was aborted or lost its CAS now reports `Blocked` where it reported
  `Satisfied`. `reconciliation_status` (the per-server drain query) is unchanged and remains the
  authority on which server is still referenced. Two pins in `tests/rebalance.rs` were rewritten
  to state this (`:970`, `:1343`) — the brief's named 5th file.
- **No `NEEDS-HUMAN external dependency`.** All five brief-named ids are present on this host:
  `typos`, `docs-renderer` (`markdown_it`+`yaml`), `cargo-deny` 0.20.2 (≥ 0.20.0, so the C4-ci
  row is not the stale-tool failure), `cargo-machete`, `cargo-mutants`. Nothing else was needed:
  no Docker, no protoc, no live backend.
- **Scratch**: none created outside the worktree; `mutants.out*` is produced inside the worktree
  by the advisory gate and is git-ignored (`.gitignore:14`). Nothing to sweep under
  `$PDCA_SCRATCH`.
