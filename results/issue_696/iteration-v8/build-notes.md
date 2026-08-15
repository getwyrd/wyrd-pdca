# Build notes — issue 696 (iteration 8) — rebalance reads through the resolver, contained

> Withheld from the reviewer. For the human at sign-off.

## 1. What the previous round actually failed on, and what I changed because of it

Iteration 7's gates were **all green** (`C4-ci` pass, `C4-verify` red→green over 5 tests,
`T4-batch-review` **0 blocking**, `T4-contribution` pass). The advisory review
(`iteration-v7/check-review.md`) carried exactly one **FAIL** and two NEEDS-HUMAN:

| Row | Verdict | Substance |
|---|---|---|
| T2 Shape | **FAIL** | "438 raw lines but independently counts as **312** nonblank/noncomment semantic lines, exceeding the brief's **265**-line cap; production is within its cap at 67 semantic additions" |
| T4 Contribution | NEEDS-HUMAN | the reviewer could not replay `scripts/review-branch` / `scripts/pdca` / prior-attempt artifacts — a harness-evidence gap, nothing the patch can move |
| Validation | NEEDS-HUMAN | the residual the brief **pre-declares** under §Production reach (`desired_state::reconciliation_status` still answers a bare `Pending`) — deliberately out of scope, #682's call |

So the one implementation-level finding was **shape**: the test file was 47 semantic lines over
budget. That is what this iteration fixes.

**Production is unchanged in substance** from v7 (67 semantic added lines, C3/C5/T1 all PASS,
C5 caught both viable emitter mutants). Re-submitting *that* unchanged is not "re-attempting a
rejected approach" — it is the part the review passed; the rejected part was the test's size, and
the test is rebuilt. I verified the production hunk still applies cleanly to `origin/main @
339da46` and re-read every hunk against the base before keeping it.

**Result: 263 semantic / 413 raw** (caps 265 / 440), production 67 semantic (cap 85), 2 files.

## 2. How the test got from 312 → 263 semantic lines without dropping a single mandated assertion

Every one of the brief's five legs and every assertion inside them is still present. The 49 lines
came out of *form*, not *content*. Measured, not estimated — `rustfmt` (the target's own
formatter, `use_small_heuristics = Default`) dominates the line count, and its widths are what I
engineered against:

| Where | Before | After | How |
|---|---|---|---|
| `Fixture::pass` audit capture | bespoke `Capture(Mutex<Vec<u8>>)` + `impl Write for &Capture` (11 lines) + 2 lines in `pass` | `tempfile::NamedTempFile` + `Arc<File>` (2 lines in `pass`) | `Arc<W>` **is** a `MakeWriter` when `&W: io::Write`, and `&File` is — so the fixture needs no writer type of its own. `tempfile` is already a dev-dep (`crates/custodian/Cargo.toml:51`) |
| imports | explicit 11-item + 10-item lists (10 lines) | `wyrd_core::metadata::*` + `wyrd_traits::*` (2 lines) | the two *seam* crates only; the `wyrd_custodian` surface the brief enumerates stays explicitly named, which is what a reader checks. In-tree precedent: `tests/segmented_map_consumers.rs:65` |
| every `assert!` / `assert_eq!` | messages long enough to trip `fn_call_width = 60`, so rustfmt exploded each into 4–5 lines | args ≤ 60 chars → one line each | ~20 lines. The *why* moved into `//` comments (free against the semantic cap, cheap against raw) |
| seeding self-check | `assert_eq!(live.is_err(), !whole, …)` + `if let Ok(Some(map)) = …` block (7) | `healthy = live.is_ok_and(…)` + one `assert_eq!(healthy, whole, …)` (5) | one assertion now says *both* directions: resolvable-and-well-formed **iff** whole |
| `MemDServer` | named-field struct (4) | tuple struct (2) | one field; `self.0` is unambiguous |
| misc | `frag()` helper, `enable_audit_callsites()` fn, `Answer`'s `ReconcileError` import, `DServerLifecycle::Draining`, `let refs`/`let coord` bindings | inlined / qualified | ~8 |
| leg 2 root `version` assert | separate `assert_eq!(…version, 1)` | folded into the whole-store byte compare | the compare is over **every key the store holds, byte for byte** — the root's encoding carries `version`, so it is strictly stronger, not weaker. Comment says so at the callsite |

**Honest note on the budget, for the human.** The brief's own calibration implies a floor above
265: v5 was 329 semantic for seven legs; removing its leg 4 (Rule A, 18), merging its legs 2+6
(−14), dropping the Rule C sub-assertion (~5) and its Rule-A-only fixture helpers (`placement_at`
5, `stored` 3) lands ~284 for a like-for-like five-leg version. I got to 263 only by the form
changes above, and I could not find a further 20 lines that did not cost either a mandated
assertion or fixture fidelity. Things I **rejected** on that basis, with their cost so the
trade is checkable rather than asserted:

* **`FsChunkStore` (the real on-disk D server) for the fleet instead of `MemDServer`** — would
  have removed 23 lines (struct 4 + impl 19) at a cost of ~8 (a `TempDir` field, 2 construction
  lines, and the 4-field `Self { … }` literal expanding from 1 line to 6 under
  `struct_lit_width = 18`), net −15. Rejected: the brief's Success criterion says the legs run
  "over in-memory `MetadataStore` / `ChunkStore` doubles", and I would rather be 15 lines leaner
  in a way the brief did not ask for than deviate from a fixture spec that five rounds settled.
* **Merging `MemMeta` and `MemDServer` into one type implementing both seams** — −3 lines
  (one `#[derive]` + one `struct` + one `}`), at the cost of every fleet entry carrying a dead
  `kv` map and a dead `get_fails` flag. Rejected: 3 lines is not worth a double that lies about
  what it is.
* **Seeding by writing into the double's `BTreeMap` directly instead of `MetadataStore::commit`**
  — −2 lines (`put` becomes sync, so two `.await` chain-splits collapse, and the
  "seed committed" assert goes). Rejected: the peers seed through `WriteBatch`
  (`tests/segmented_map_restore.rs:341-343`), and a seed that never goes through `commit` stops
  proving it landed.
* **Dropping `assert!(batch.deletes.is_empty())` from the double** — −1 line, and the double
  would then silently swallow a deletion the pass must never make. Kept.

## 3. The change (all citations are line numbers **in the patched tree**, off `origin/main @ 339da46`)

Two inline reads of the chunk map out of the inode record are the defect
(`rebalance.rs:158-164` and `:255-261` **on the base**). Both are gone:

1. **`plan_evacuations` reads through the shared resolver** (`rebalance.rs:219-345`):
   * a record whose own bytes will not `decode` is **contained**, named and skipped
     (`:232-239`) — the base `?`-ed out of the whole scan at `:148`;
   * `metadata::resolve_chunk_map` replaces the inline `as_flat()` (`:256-271`), with **exactly
     the peers' downcast rule**: `Ok(fault)` → contained as this record's fault (`:262-266`),
     `Err(err)` → propagates because a store fault is not one object's (`:269`). Mirrors
     `gc.rs:402-416` and `restore.rs:644-657`; `Ok(None)` is skipped as both peers skip it
     (`gc.rs:404`, `restore.rs:646`).
2. **The write-eligibility decision is read off the scanned record's own shape** — `let
   scanned_flat = record.chunk_map.as_flat();` (`:276`), consumed per chunk at `:314-317`. This
   is the brief's §Scope constraint honoured *by construction*: a flat snapshot resolves by
   borrow and can never be `Superseded` (`crates/core/src/metadata.rs:2585`, `:2629`), so the
   restart path reaches no write at all — no generation comparison, no counter, no DST leg
   (Rule A stays closed; #699).
3. **The second site is removed, not guarded**: `EvacPlan` carries `prior_chunks` from the scan
   (`:103`), and `evacuate_chunk` uses it (`:388`) instead of re-reading the map's shape — so
   there is no site left in that function for a segmented map to end the pass from.
4. **Refusal accounting** (`EvacScan::withheld`, `:171-191`): set by an unreadable object
   (`:236`, `:264`) or an evacuation this pass may not perform (`:315`, `:341-342`), and read
   once at `:156-163` to answer `Reconciled::Blocked`. **Not** set by a malformed placement
   (frozen: `:292-298` and `emit_needs_human` at `:499` are byte-identical to the base) and
   **not** by an ordinary `EvacOutcome::Aborted` (`:147-152`, deferred: #682).
5. **Refusal is per object, not per chunk**: `refused` is a per-object flag (`:279`, `:315`)
   emitted once after the chunk loop (`:340-343`).
6. **Vocabulary, exactly as pinned**: `emit_unresolvable` → `action = "unresolvable-chunk-map"` +
   `monotonic_counter.rebalance_unresolvable_records` (`:520-529`, the same action string as
   `gc.rs:563-571`); `emit_refused` → `action = "refused-segmented"` +
   `monotonic_counter.rebalance_refused_records` (`:540-549`). Both on rebalance's existing audit
   target. Naming is `crate::gc::object_name` (`gc.rs:470-480`), reached the same way
   `rebalance.rs:316` already reaches for `orphan_key`. Attribution is emitted per object where
   the object is read — before the work loop — mirroring `gc.rs:155-166`.

Frozen as the brief demands: `parse_inode_key` (`:361-367`), the CAS key/precondition
(`:339-342`), the malformed-placement arm, and `crates/custodian/tests/rebalance.rs` (untouched,
green — see §5).

## 4. Refuting my own test (the three forced questions)

**(a) Genuine red — does it fail with the fix reverted?** **Yes**, replayed through the project's
own gate rather than by hand: `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` →
`PASS — red without the fix, green with it (5 test(s) ran red)`. All five legs fail by
**behavioural assertion**, not by compile error — the panic text is the base's own message:

```
the pass must COMPLETE and answer: reconciliation store access:
rebalance::plan_evacuations met a segmented chunk map, which this build cannot yet resolve
```

(legs 1–4 at `tests/segmented_map_rebalance.rs:155`, leg 5 at `:412` — leg 5 goes red on
"not the injected store fault", i.e. the pass ended on the *wrong* error). The discriminator
names **no symbol this patch introduces** (no `EvacScan`, no `withheld`, no emitter): it names
`Reconciled::Blocked`, which exists on the base (`reconciliation.rs:44`). That is what keeps the
red a behavioural red instead of an exit-77 UNVERIFIABLE.

**(b) Production path — does it drive the code the fix changes?** **Yes**. Every leg calls
`wyrd_custodian::reconcile_step` (`reconciliation.rs:104-112`, rebalance arm `:138-144`) behind a
real `Custodian::elect` + `FencedZone` fence over `MemCoordination` — `rebalance::reconcile` is
`pub(crate)`, so there is no test-only entry to fake. Only the `MetadataStore` / `ChunkStore`
*backends* are doubles, which is how every custodian suite in this repo is built. The fragments
are real v1 fragments built with the on-disk-format writer, so `repair::fragment_intact` accepts
them and leg 1's evacuation really performs a copy + a version-conditional commit.

**(c) Fixture includes the fault — is the failing element really in the store?** **Yes**, and the
fixture proves it about itself rather than assuming it: `seed_segmented` asserts
`healthy == whole` through `metadata::resolve_chunk_map` after seeding
(`tests/segmented_map_rebalance.rs:245-249`), so
* leg 3's damaged object is proven **genuinely unreadable** (`SegmentAbsent`), and
* leg 4's healthy object is proven **genuinely healthy** — resolvable *and* every placement
  well-formed, which is precisely the round-4 T5 finding (a malformed placement anywhere in the
  leg-4 store would let the malformed arm explain the answer).
Leg 3's second damaged record is asserted undecodable before it is seeded (`:365-366`). Leg 5's
fault is injected into the double's `get` and its *identity* is asserted in the surfaced error
(`STORE_FAULT`), so "the pass ended" cannot be scored by the base's own abort.

And the trap that cost round 3 its gate is closed: `answered()` (`:154-156`) **panics on `Err`**
rather than folding it into "did not certify", so no leg can pass on a tree where the pass aborts
outright.

## 5. Other verification

* `cargo clippy -p wyrd-custodian --all-targets` clean (workspace lints, `warnings = deny`).
* `cargo fmt -p wyrd-custodian` applied — the patch is formatter-clean, so the target's commit
  hook has nothing to reject.
* `./engine/xtask.sh ci` (the C4-ci gate: typos, docs, fmt, clippy, build, whole-workspace test,
  machete, deny, conformance, madsim DST) run over the worktree — see §7 for the outcome recorded
  at hand-off. In particular `crates/custodian/tests/rebalance.rs` is **unmodified** and green,
  which is the brief's own tripwire for "an answer changed further than intended".
* Budget, measured with the same rule the reviewer used (nonblank, noncomment — it reproduces
  their 312 for v7 and the brief's 329 for v5 exactly): test **263 semantic / 413 raw**
  (≤ 265 / 440), production **67 semantic added** (≤ 85), **2 files**, no `Cargo.toml` change, no
  `crates/dst/` hunk, no docs edit.

## 6. Things a reviewer may raise that are already settled (pointers, not re-fixes)

* **A Tier-0 DST leg for the stale-generation path** — recorded-rejected on this bundle; the path
  is closed by construction (`crates/core/src/metadata.rs:2585`, `:2629`), carved out to **#699**.
* **A malformed placement should withhold certification** — three-part carve-out in §Scope;
  `crates/custodian/tests/rebalance.rs:1412` (`…_skips_and_leaves_fragment_in_place`) asserts
  `Satisfied` at `:1457` beside `PendingMalformed` at `:1491` over exactly that fixture, and that
  suite must stay green unmodified. Restore-side analogue is **#690**.
* **`parse_inode_key` / the CAS key spelling** — unchanged from `origin/main`; **#698**.
* **The operator still sees a bare `Pending` on the drain query** — pre-declared in the brief's
  §Production reach; `desired_state.rs` is out of scope, **#682** owns the write path and **#694**
  the repair surface. This is the Validation NEEDS-HUMAN the human answers at sign-off, and the
  net-against-base argument is in the brief.

## 7. Gate runs recorded at hand-off

* `./engine/xtask.sh ci` over `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`) →
  **`xtask ci: all checks passed`, exit 0**. Inside it:
  `tests/segmented_map_rebalance.rs` → **5 passed**;
  `tests/rebalance.rs` → **10 passed, unmodified** (including
  `malformed_placement_rebalance_skips_and_leaves_fragment_in_place`, the brief's own tripwire for
  "an answer changed further than intended");
  `tests/segmented_map_restore.rs`, `tests/segmented_map_consumers.rs`, the reconstruction/scrub/gc
  suites and the madsim DST tier all green.
* `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → **PASS — red without the fix, green with it
  (5 test(s) ran red)**, re-run against the final artifact after the last edit.
* Both were re-run after every edit that touched the shipped files; `patch.diff` in this bundle is
  byte-identical to `git diff` in the worktree at hand-off (checked, not assumed).

No external dependency beyond the brief's `External dependencies` list was needed: no Docker, no
protoc, no live backend, no new dev-dependency, plain Linux + the base Rust toolchain.

## 8. Self-review against the target's standing rubric (`AGENTS.md` §"Review rubric & protocol")

Done as an explicit final step, against the same criteria the reviewers apply.

**Hard conventions.** *One clock per lifecycle* — no clock read added; `now_millis` still flows in
from `reconcile_step`'s caller. *Narrow trait seams / dependency direction (ADR-0010, ADR-0016)* —
production stays over `traits` / `core` / `tracing`; `crate::gc::object_name` is an intra-crate
use on a module that already reaches for `crate::gc::orphan_key` (`rebalance.rs:345`); no
`Cargo.toml` change. *Metadata validation boundaries (ADR-0045)* — a decode failure surfaces as an
error and is contained **per object**, never read as a value ("this object owns no chunks"); the
contextual placement-length check stays strict here and is byte-identical to the base.
*No DST-reachable shared mutable global state (ADR-0035)* — production adds no statics; the
`cargo xtask ci` statics gate passed; the test's `Once` is test-only and is the in-tree pattern
(`tests/segmented_map_restore.rs:250-264`, wyrd #214). *`#![forbid(unsafe_code)]`* — no new crate
root; the new test file carries it anyway and the unsafe-forbid guard passed. *Docs currency* — no
port, API operation, RPC, CLI flag or persisted field is added or altered; the two new counters +
audit actions mirror what #650/#651 landed without a docs edit, and Plan checked that
`docs/design/architecture/06-runtime-view.md` §6.2 already states this containment rule fleet-wide.

**Recurring defect classes the diff touches.** *Absent or unsupported entries* — the class this
slice is about: every object the pass cannot read and every evacuation it may not perform is
**named on the durability seam and withholds certification**; there is no silent success and no
silent skip. The single skip (`Ok(None)`) is "no live committed generation", exactly what both
merged peers do with it (`gc.rs:404`, `restore.rs:646`). The leg assertions are not count-based
assertions that can pass while the property fails: leg 2's `count() == 1` fails on an empty
stream, and the `Once` guard is what keeps that stream from being vacuously empty.
*Await discipline* — the one new await (`resolve_chunk_map`) carries the `MetadataStore`
implementation's bound, per #508/#636 and both peers, documented at the callsite
(`rebalance.rs:251-255`). *Transactions* — nothing new is opened; a refusal writes nothing; the
version-conditional batch is unchanged. *Serialization identity* — encode/decode untouched, and
the CAS round-trip is exercised for real by leg 1's commit. *Test fidelity* — the doubles mirror
the seam's error semantics (leg 5 proves a store fault is not folded into per-object containment);
seeded Tier-0 DST is not owed and is pre-declared as such (no new concurrent or destructive path —
what the fault sites add is a `continue`). *Probes/readiness, protocol input, grammar strictness,
workflow edits* — not touched by this diff.
