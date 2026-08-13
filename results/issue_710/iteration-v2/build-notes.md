# Build notes — issue #710 (iteration 2)

Target branch: `getwyrd/wyrd @ main` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base
`9dbcd72`, which already carries the merged #696/#697 PRs #705/#706). All `path:line`
citations below are against that worktree.

## 1. What the carry-forward asked for, and what I did about each

| Carry-forward item | Disposition |
|---|---|
| **C5 mutants** — `>`→`>=`, `>`→`==`, `>`→`<` on the ceiling predicate survive; "add an exact-`MAX_VALUE_BYTES` acceptance test" | **Fixed, two ways.** (a) A behavioural boundary test through the production loop: `an_evacuation_landing_exactly_on_the_value_ceiling_still_commits` (`crates/custodian/tests/placement_ceiling.rs:453`) drives a repoint that lands on **exactly** `MAX_VALUE_BYTES` and asserts it COMMITS. (b) A unit test beside the predicate: `crates/core/src/metadata.rs:392` (`mod value_ceiling`). (b) is what actually kills the mutants — see §4. |
| **Review BUG** `rebalance.rs:161` — `move_incomplete` excludes `Conflict`, so an all-CAS-lost pass still certifies | **Fixed.** Certification is no longer decided per arm: `EvacOutcome::persisted()` (`crates/custodian/src/rebalance.rs:400`) answers it for the outcome itself, and the loop does `unmoved |= !outcome.persisted()` (`rebalance.rs:163`). Only `Committed` persists, so `Conflict` and `Aborted` withhold too. |
| **Review BUG** `rebalance.rs:153` — a CAS conflict is also "did not persist" | Same fix. Cost: one existing pin flips (`crates/custodian/tests/rebalance.rs:1347`, the racing-writer test), inside the brief's named 5th file. |
| **Review BUG** `rebalance.rs:465` / `reconstruction.rs:909` — the ceiling check ran **after** fragments were copied/rebuilt and returned with no orphan-ledger entries, stranding bytes GC cannot reclaim | **Fixed at the cause, not with a ledger entry.** The refusal is now decided **before any store is touched**: `rebalance.rs:437-473` and `reconstruction.rs:855-901` compute the prospective placement + `next` record from the selector's answer alone, encode it, and refuse there — the fragment copy / rebuild loop now runs *after* the check. Nothing is written at all, so there is nothing to orphan. Two test assertions pin it (`placement_ceiling.rs:333` "a refused repair stranded a fragment", `:442` "a refused move left a stranded fragment copy"). |
| **Review TEST-GAP** `placement_ceiling.rs:547` — the evacuation leg only rejected `Satisfied`, so `Changed` would pass | **Fixed.** `assert_eq!(outcome, Reconciled::Blocked, …)` (`placement_ceiling.rs:427`). `Blocked` is base-visible (`crates/custodian/src/reconciliation.rs:44`), so the hard constraint holds. |
| **T2 Shape FAIL** — 671 semantic lines vs the ≤250 budget (599 in the test) | **Materially reduced, still over — see §5.** 671 → 510 total; the new test file 599 → 385. |
| **T5 (deferred to sign-off)** — "decide whether lost-CAS conflicts are intentionally excluded from 'a move that did not persist'" | The question is now **answered in code**: they are *not* excluded. `persisted()` makes it structural, and the two `tests/rebalance.rs` pins that asserted the old meaning are rewritten with the reason in place. |

## 2. The change

**One rule, two write paths:** *a placement write that would not survive is refused, and a
move that did not persist does not certify.*

1. `crates/core/src/metadata.rs:381` — `flat_value_ceiling_crossed(&[u8]) -> Option<usize>`,
   sited immediately after the constants and the `const` assertion that ties them
   (`:327`, `:352`, `:354`). It bounds a **flat** record by the FULL ceiling
   (`MAX_VALUE_BYTES`), not the `MAX_ROOT_VALUE_BYTES` half — that half exists to budget a
   *segmented* root's segment table against the reserve its object metadata spends, and a
   flat record has no segment table. No third constant is invented (brief scope).
   `>` not `>=`: a record landing exactly on the ceiling is storable, and the resolver's
   read-side refusal uses the same boundary (`metadata.rs:2521`).
2. `crates/custodian/src/reconstruction.rs:895-901` and
   `crates/custodian/src/rebalance.rs:468-474` — each binding commit encodes its `next`
   record **first**, refuses on the ceiling, and only then writes fragments. The bytes
   checked are the bytes committed (`.put(inode_key, next_bytes)`,
   `reconstruction.rs:931`, `rebalance.rs:508`), so no re-encode can drift past the check.
   The CAS idiom, `version = prior.version + 1` and `..prior.clone()` (ADR-0047 metadata
   preservation) are untouched.
3. New crate-private outcomes `RepairOutcome::Refused { bytes, ceiling }`
   (`reconstruction.rs:815`) and `EvacOutcome::Refused { … }` (`rebalance.rs:382`), each
   named on the durability seam by `emit_ceiling_refused` (`reconstruction.rs:1152`,
   `rebalance.rs:640`) — `warn`, with the record's own length and the ceiling, and a
   NEEDS-HUMAN line, because unlike a conflict or an abort this shape never clears on its
   own.
4. Accounting: `reconstruction_ceiling_refused` joins the documented offset identity, which
   becomes `repaired − conflict − aborted − ceiling_refused` (`reconstruction.rs:269-279`,
   `:1026`, `:1136`).
5. Certification: reconstruction withholds on a ceiling refusal (`reconstruction.rs:344`,
   folded into the same `hole` the segmented `reading.refused` already sets); rebalance
   withholds on **any** non-persisted move (`rebalance.rs:163`, `:166`).

### Why the two loops treat `Aborted`/`Conflict` differently

Deliberate, and stated in both files. Rebalance certifies a **drain** — an operator reads it
as "the box is safe to pull" — so every non-persisted move withholds it; the brief settles
that arm here (#696 deferred it). Reconstruction's certification is already defined over the
*reading* (`reading.incomplete || !reading.refused.is_empty()`), and its transient outcomes
(`Conflict`, `Aborted`) are each named and counted with the obligation left queued as the
standing record. The ceiling refusal joins `reading.refused` (a repair the pass **may not
perform**, permanent until the record shrinks), not the transient class. Extending
reconstruction's certification to `Aborted`/`Conflict` would also have needed a 6th file
(`tests/reconstruction.rs`), which the brief calls "the shape is wrong".

## 3. The forced refutation of my own test

- **(a) Genuine red?** Yes — production hunks stashed (`git stash push -- crates/core/src/metadata.rs crates/custodian/src/{reconstruction,rebalance}.rs`), test file kept:
  - `a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted` → **FAILED**: `100019 bytes stored, past the backend value ceiling` (`placement_ceiling.rs:319`) — the base commits the oversized record, exactly the brief's binding assertion.
  - `an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain` → **FAILED**: `left: Changed, right: Blocked`.
  - `a_ceiling_refused_repair_is_subtracted_from_the_reported_successes` → **FAILED**: `left: Changed, right: Blocked`.
  - `crates/custodian/tests/rebalance.rs`: `spread_wins_when_no_free_distinct_domain_remains` → **FAILED** (`Satisfied` vs `Blocked`), `a_racing_writer_loses_the_version_conditional_commit_and_leaves_only_garbage` → **FAILED** (same).
  - The 4th test (`…landing_exactly_on_the_value_ceiling_still_commits`) passes on base **by design**: it is the admissible-side regression guard, not a discriminator.
  - The file **compiles** against the base — it names no symbol this patch introduces (checked: no `flat_value_ceiling_crossed`, no `Refused`, no `persisted`), so C4-verify gets a red, not an UNVERIFIABLE.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` — the real fenced control point — into `reconstruction::reconcile` / `rebalance::reconcile`. Nothing is re-implemented in the test. The metric read-back goes through the real `DurabilityTelemetry` Prometheus surface.
- **(c) Fixture includes the fault?** Yes.
  - The oversized record is produced by the *loop's own* re-encode: the fixture pads the record the pass **would** write and then seeds the pre-move placement, so the growth is the production repoint's (`placement_ceiling.rs:377-385`), not a hand-built value.
  - The chunk that must be refused is the one under assertion; nothing is curated out. The leg-3 fixture keeps the aborted chunk *and* the repaired chunk in the same pass, and asserts the queue contains exactly the refused + aborted obligations.
  - Fragments are real RS(2,1) shards through the production encoder (`wyrd_core::erasure::encode` + `wyrd_core::write::encode_ec_fragment`) stored in the **real** `chunkstore-fs` backend, which verifies identity + checksum on `put` and `get` — a mis-seeded fixture fails loudly instead of silently taking a different branch.

Runner: the project's own gate, `./engine/xtask.sh ci` (→ `cargo xtask ci`: fmt, clippy
`-D warnings`, build, `cargo test --workspace --exclude wyrd-dst`, deny, conformance,
statics) — **all checks passed** on the final tree. `./engine/xtask.sh dst` (madsim tier)
also passes, exit 0. Scoped `cargo test -p wyrd-custodian …` runs (the same command
`xtask ci` issues, narrowed) were used for the red/green iterations, each under `timeout`.

## 4. Mutation testing (the C5 carry-forward)

`scripts/mutants-in-diff` on this bundle: **27 mutants, 2 missed, 16 caught, 9 unviable**
(was 16/3 missed in iteration 1; all three boundary mutants are now caught).

Why the behavioural boundary test alone did **not** kill them, and what does: `cargo mutants`
runs, for a mutant in package *P*, only *P*'s own tests. `flat_value_ceiling_crossed` lives in
`wyrd-core`, whose tests never call it — so no test in `wyrd-custodian`, however binding, can
ever catch a mutant there. That is why the fix is the unit test *inside* `crates/core/src/metadata.rs`
(`mod value_ceiling`, `:386`): it pins both sides of the boundary in the package that owns it.
It costs no sixth file (metadata.rs is already file #1) and it disappears with the production
hunk when C4-verify reverts, so it cannot break the red leg. The behavioural
exactly-at-the-ceiling test stays as well — it is what proves the boundary through the
*loop*, which a unit test cannot.

The 2 remaining misses are **equivalent mutants**, both in the `InodeRecord` literal I moved
earlier in `evacuate_chunk`:

- `rebalance.rs:450 delete field size` — deleting it falls through to `..plan.prior.clone()`,
  which supplies `plan.prior.size`: the identical value.
- `rebalance.rs:452 delete field state` — same fall-through; `plan_evacuations` only plans
  **committed** records, so `prior.state` is already `Committed` on every reachable path.

Both exist verbatim on `origin/main` (they are only *in the diff* because the literal moved
above the copy loop), and neither is killable without inventing an unreachable state.

## 5. Size: what I got to, and the honest gap

Semantic added lines (non-blank, non-comment), by file:

| file | lines |
|---|---|
| `crates/core/src/metadata.rs` | 22 (3 predicate + 19 unit test) |
| `crates/custodian/src/rebalance.rs` | 49 |
| `crates/custodian/src/reconstruction.rs` | 50 |
| `crates/custodian/tests/rebalance.rs` | 4 |
| **production + pins** | **125** |
| `crates/custodian/tests/placement_ceiling.rs` | 385 |
| **total** | **510** (iteration 1: 671) |

Against the brief's ≤250. The gap is the new test file, and this is where I could not get
further without giving something up. What I did cut (599 → 385):

- Dropped the `Fleet` + `PlacementChunkStore` double and the `write_new_object_placed`
  seeding path entirely (~110 lines): records are hand-seeded and fragments are written to
  the **real** `chunkstore-fs` stores, so there is one double in the file, not three.
- Replaced the third fixture with parameterisation: `drain_pass(moved_len)` serves both the
  refusal leg and the at-ceiling acceptance leg.
- Factored `fleet()` and `repair_pass()` so the two reconstruction legs share the context +
  fence + telemetry plumbing.
- Compressed assertion messages to one line each (rustfmt was spending 4–5 lines per
  `assert!`).

Of the remaining 385, ~189 are harness with no decision content — the in-memory
`MetadataStore` double (42, and every `custodian` test file rolls its own; there is no shared
one and adding one would be a 6th file + a manifest edit), the `use` block and consts (32),
the five fixture builders (63) and the two pass runners (52). The four legs' own bodies are
~197. Production + legs = ~322.

What I explicitly did **not** do to get under the number, and why:
- **Drop leg 3.** It is named in the Success criterion, and it is the only place the new
  outcome's accounting is pinned.
- **Merge legs 1 and 3 into the mixed pass** (would save ~50). The mixed fixture needs
  utilization ordering to make three plans take three outcomes; putting the *discriminating*
  red inside it makes the one assertion that proves the defect depend on that ordering. The
  single-plan leg-1 fixture is deliberately dumb.
- **Drop the at-ceiling acceptance leg** (~18). That is the C5 carry-forward.

If the human wants the number met exactly, the cheapest honest cut is leg 3 plus the
`counter()` helper (~65 lines) — at the cost of the accounting rule going unpinned.

## 6. Scope boundaries observed / notes for sign-off

- **5 files exactly**, all named by the brief. No `backfill.rs`, no `repoint_chunk`, no
  segmented addressing, no read-side change, no new dependency (the test's `chunkstore-fs`,
  `tempfile`, `testkit`, `coordination-mem` are existing `[dev-dependencies]` of
  `wyrd-custodian`).
- **ADR-0011 §2** states the offset identity with the three counters that existed when it was
  written; this patch adds a fourth (`reconstruction_ceiling_refused`). The brief puts
  "any new or edited ADR / spec / proposal" out of scope and the repo gates ADR immutability,
  so I did **not** amend it; instead `emit_ceiling_refused`'s doc records that the ADR itself
  names `crates/custodian/src/reconstruction.rs` as "the source of truth for their exact
  emission" (`reconstruction.rs:1148-1151`). **If the human wants the ADR's prose refreshed,
  that is a separate, deliberate doc change.**
- The rewritten pin at `crates/custodian/tests/rebalance.rs:1347` (racing writer → `Blocked`)
  changes an *operator-visible* meaning: a pass whose only evacuation lost its CAS now
  reports `Blocked` rather than `Satisfied`. `reconciliation_status` (the per-server drain
  query) is unchanged and remains the authority on which server is still referenced. This was
  the T5 judgment deferred at iteration 1; it is now decided in the direction the reviewers'
  two BUG findings asked for.
- No `NEEDS-HUMAN external dependency` — everything needed was in the toolchain (`typos`,
  `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` were all present; `xtask ci`
  and `mutants-in-diff` both ran).
- Scratch: `${PDCA_SCRATCH}/pdca-builder-710-dst.log` was the only scratch file created; it is
  removed at the end of this run.
