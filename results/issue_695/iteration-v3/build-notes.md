# build-notes — issue 695, iteration 3 (backfill reads through the resolver, contained)

*Withheld from the reviewer. For the human at sign-off.*

Target branch: `getwyrd/wyrd @ main`, verified `main == origin/main == 339da46`.
Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`. Every `path:line` below is
against that worktree **with the patch applied** unless it says "on the base".

---

## 1. What changed, and why this shape

Two files, exactly as the brief budgets:

| File | Change | Semantic added | Cap |
|---|---|---|---|
| `crates/custodian/src/backfill.rs` | the pass | **95** | 130 |
| `crates/custodian/tests/segmented_map_backfill.rs` | NEW, 8 legs | **323** semantic / **485** raw | 320 / 520 |

The production hunks are the v7 salvage the brief pointed at (`results/issue_681/iteration-v7/patch.diff`)
with rules A and C applied, minus the accounting struct the last two review rounds kept
attacking (§2). The walk is now `gc::referenced_fragments`' walk (`crates/custodian/src/gc.rs:360-455`),
applied a third time after #650 (gc/scrub) and #651 (restore):

```
scan  -> decode (fail = THIS object's fault, contained)      backfill.rs:119-125
      -> resolve_chunk_map                                   backfill.rs:132
           Ok(None)   -> Rule A: no live generation          backfill.rs:142-145
           Err(ChunkMapError) -> contained, named            backfill.rs:150-153
           Err(other)         -> propagate (a store fault)   backfill.rs:157
      -> Rule A: did the resolve answer MY generation?       backfill.rs:164-167
      -> classify (unchanged, #348's classifier)             backfill.rs:173-184
      -> remaining += (owed until a fill lands)              backfill.rs:193
      -> segmented? decline, write nothing                   backfill.rs:203-206
      -> CAS under the STORE'S key on the STORE'S bytes      backfill.rs:237-239
```

**The answer.** One counter, `incomplete` (`backfill.rs:110`), incremented at each containment
or decline site through the emitter that also names the object, so a site can neither name
without counting nor count without naming. `incomplete > 0` ⇒ `Reconciled::Blocked`
(`backfill.rs:264-273`) — the vocabulary `gc.rs:234-246` already uses, folded by
`reconciliation.rs:55-61`'s `least_certified`; no parallel outcome invented.

**The gauge.** `emit_remaining` no longer re-scans (on the base it was a *second* resolving
reading of the whole namespace, `backfill.rs:171-190` on the base). It is counted in the walk
that fills, and published beside a second instrument, `gauge.backfill_placement_incomplete`,
which is what bounds it. That second number is not decoration: a gauge reading 0 over a store
with three unreadable objects is exactly the lie #363's fallback-removal gate would fire on.

## 2. What I deliberately did NOT re-attempt (the carry-forward)

Iteration 2 shipped a `Refusals` struct with four counters and a `refuses_to_certify()`
predicate that **excluded** the lost-CAS case, with a doc paragraph arguing for the exclusion.
Two of the three review passes independently flagged that exclusion as a BUG
(`review-batch.md`: `backfill.rs:322`, `:323`). They were reading the code correctly — but the
behaviour they wanted (`Blocked` on a lost CAS) is pinned by the *unmodified* existing suite,
`crates/custodian/tests/backfill.rs:290-295`, which asserts `Satisfied` after a lost CAS, and
the brief forbids editing that file.

So I removed the argument rather than restating it. There is no `Refusals` struct, no
`superseded` counter and no `refuses_to_certify()` predicate in this patch. The Conflict arm
(`backfill.rs:248-255`) is the base's, changed only in the name it emits, and carries a comment
citing the two pinned tests. The lost record's empty placements stay on `remaining` — which is
also what the base achieved via its re-scan — so no lost race is ever published as a drain.

Cost of the alternative, concretely: making a lost CAS non-certifying is a **1-line** change
(`incomplete += 1` in the Conflict arm) but it flips `crates/custodian/tests/backfill.rs:290-295`
from `Satisfied` to `Blocked`, i.e. it edits a file the brief's scope section names as
must-stay-green-unmodified, and it changes an answer #350 shipped and #363 will read. Out of
scope for this slice; if the repo wants it, it is a one-line follow-up with its own issue.

The third finding (`backfill.rs:149`, "the equality check cannot detect a stale FLAT scan") is
true and was never what the check is for. Fixed as a *claim*: the doc comment now says exactly
what the comparison catches (a resolver RESTART, which only a segmented snapshot can take —
`crates/core/src/metadata.rs:2624-2628` borrows a flat one and reads nothing) and what settles
the flat side instead (the `require(stored bytes)` CAS, which loses rather than clobbering),
`backfill.rs:90-100`. And `emit_remaining`'s doc now states that `remaining` is a **sample over
the generations this pass read**, never a proof of drain (`backfill.rs:295-301`) — which is
inherent to any scan-and-act pass and true on the base too.

Rejected alternative, with its cost: *detect* a stale flat scan by re-reading each root before
classifying. That is one extra `MetadataStore::get` per committed object per pass — for a store
of N objects, **N extra round trips**, doubling the pass's metadata reads — and it still would
not be atomic (the successor can land between the re-read and the CAS). It buys nothing the CAS
does not already give on the write path, and it breaks the brief's "one resolving reading of the
namespace per pass" constraint that leg 6 binds. Not taken.

The fourth finding (the seeded Tier-0 DST TEST-GAP) is the standing one the brief settles at
Plan. It is now marked the repo's own way — `// deferred: #682` at `backfill.rs:201`, which
`AGENTS.md` § Reviewer protocol declares settled for review purposes — **and** recorded in
`review-rejected.md` at the five lines it can land on, since the previous round's rejection was
anchored at lines the finding did not land on and therefore did not suppress it.

## 3. Rules A and C — what I chose, and why

**Rule A** ("how it tells is Do's to choose"): a value comparison of the resolved generation
against the scanned one, `*resolved.record != record` (`backfill.rs:164`). `ResolvedChunkMap`
carries `record: Cow<InodeRecord>` for exactly this (`crates/core/src/metadata.rs:2256-2272`),
`InodeRecord` derives `PartialEq` (`metadata.rs:1348`), and the only way the two differ is the
restart at `metadata.rs:2629`. I considered and rejected `matches!(resolved.record, Cow::Owned(_))`
(borrow-ness is an optimisation, not a contract) and comparing `version` alone (weaker, and no
cheaper). `Ok(None)` is Rule A's other arm and is *also* non-certifying here, unlike its two peer
walks (`gc.rs:404`, `restore.rs:646` skip it) — the difference is spelled out at
`backfill.rs:134-141`: their claims are about objects that still exist, this pass's claim is that
it saw the whole population it is draining. Iteration 1's carry-forward asked for exactly this
("`Ok(None)` … bypass refusal"); leg 4's third successor binds it.

**Rule C**: `parse_inode_key` is **deleted**. The pass never parses the key: it reads, CASes and
names under the store's own bytes (`backfill.rs:237-239`, `:247`), which is what `gc.rs:280-294`
already does and why `gc::object_name` exists. That also removes the base's silent skip of a
committed record under a key the grammar refuses — such a record is now simply repaired under its
own key. The CAS precondition moved from `metadata::encode(&record)` to the scanned `value`: the
row's own stored bytes, so a record whose stored spelling does not round-trip cannot lose this CAS
forever.

## 4. Red → green (the project's runner, both legs)

`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` — the C4-verify runner, which applies `patch.diff`
to a clean `../wyrd-verify-l0` worktree off `origin/main`, runs the added test green, then reverts
**only** the production file and runs it again:

```
run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
   red leg: test result: FAILED. 1 passed; 7 failed
   green leg: 8 passed
```

Seven of eight legs are base-red; leg 7 (`a_fault_that_is_not_one_objects_map_still_ends_the_pass`)
is the declared non-red over-containment guard — it passes before and after, and without it
"contain everything" would satisfy every other leg. Note the base-red is an **assertion/panic**
red (`SegmentedMapUnsupported` out of `reconcile`, and value assertions), never a compile error:
no leg names a symbol this patch introduces — the new instruments and audit actions are matched as
strings.

## 5. The three forced questions

**(a) Genuine red?** Yes, and mechanically, not by eye: `run-verify.sh` reverts
`crates/custodian/src/backfill.rs` to `origin/main` while keeping the added test, and the target
goes `FAILED. 1 passed; 7 failed`. Both legs actually ran tests (8 in each), so neither is the
"0 tests ran" vacuum the gate reports as UNVERIFIABLE.

**(b) Production path?** Yes. The test calls `wyrd_custodian::backfill::reconcile` — the same
public entry `crates/custodian/tests/backfill.rs` and `backfill_telemetry.rs` drive — and every
resolution goes through the real `wyrd_core::metadata::resolve_chunk_map`, real `encode`/`decode`,
real `seg_key`, and the real validating constructors `SegmentedMap::new` / `SegmentRecord::new`
(so a fixture typo cannot silently change which rule a leg exercises). The only double is the
`MetadataStore` seam itself, which is how every custodian suite in this repo is written; there is
no copy or re-implementation of the pass.

**(c) Fixture includes the fault?** Yes, and each leg asserts its own fault is real before relying
on it:
* leg 3 seeds a root naming a `seg:` record that was **never written** and asserts in the fixture
  that `resolve_chunk_map` genuinely errors on it (`segmented_map_backfill.rs:306-308`), plus a
  record whose bytes will not decode — both **first in key order** over a `BTreeMap`-backed store,
  so "the healthy record beyond the blockers was still filled" is a property, not luck;
* leg 4 asserts in the fixture that the resolve really **restarts** onto a generation other than
  the scanned one (`:362-367`) before asking what the pass did about it;
* leg 7 injects a real `get` fault that the resolve really reaches — every segmented resolve ends
  with the root re-read at `crates/core/src/metadata.rs:2563-2572`;
* nothing is curated out: legs 1, 3 and 6 all put the healthy object in the **same store** as the
  damaged one, which is the whole point of containment.

## 6. Other gates run locally

* `cargo fmt --all --check` — clean (the commit hook's formatter).
* `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` — clean.
* `cargo check --workspace --all-targets` — clean (no other crate calls `backfill::reconcile`).
* `cargo test -p wyrd-custodian -p wyrd-chunkstore-grpc` — all green, including the two existing
  suites the brief says must stay green **unmodified**: `tests/backfill.rs` (10) and
  `tests/backfill_telemetry.rs` (1). Neither file is touched by this patch.
* `typos crates/custodian/src/backfill.rs crates/custodian/tests/segmented_map_backfill.rs` — exit 0.
* `scripts/mutants-in-diff` — **23 mutants: 14 caught, 9 unviable, 0 missed.** Iteration 1's C5 row
  had 4 survivors at the telemetry sites; leg 8 and the audit assertions added to legs 2, 3 and 4
  are what closed them.

## 7. Budget note (honest overage)

The test file is **323** semantic lines against the brief's 320 — +3, or +1%. Raw is 485 against
the 520 cap, and the file count is 2, so neither STOP condition ("a third file, or a test file past
520 raw") is near. The overage is the **8th leg**: iteration 1's carry-forward required
"assertions for conflict telemetry", and the brief's own seven legs contain none, so
`a_lost_cas_is_named_under_the_stores_own_key_and_keeps_its_population` (`:457-485`, 19 semantic
lines) was added. Without it the file is 304 semantic — comfortably inside — and four mutants
survive. I judged binding the telemetry worth 3 lines over budget; flag it at sign-off if not.

## 8. Anything a human should look at

Nothing is unverifiable and no external dependency was missing — the pass runs entirely over the
`traits`/`core` seams with in-memory doubles, exactly as the brief's *External dependencies*
predicted. The one judgement call worth a human's eye is §2: this patch **does not** make a lost
CAS non-certifying, because the existing pinned suite says otherwise and the brief forbids editing
it. Two of three review passes last round wanted the opposite. If you agree with them, it is a
one-line follow-up plus an edit to `crates/custodian/tests/backfill.rs:290-295`, and it should
carry its own issue rather than ride this slice.
