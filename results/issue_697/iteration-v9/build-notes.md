# Build notes — issue #697 (iteration 9)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`, base `origin/main @ 339da46`.
Two files, as the brief's budget requires: `crates/custodian/src/reconstruction.rs` (modified)
and `crates/custodian/tests/segmented_map_reconstruction.rs` (new). All line numbers below are
on the **patched** worktree unless marked "base".

---

## 1. What changed against iteration 8 — one thing, and it removes two findings at once

Iteration 8's containment core, one-reading index and refusal accounting were not the problem
(no review round ever landed a finding on them), so they are carried forward unchanged. The
**write path** is what this round rebuilds:

| | iteration 8 | this patch |
|---|---|---|
| repairs inside ONE object | one version-conditional commit **per obligation**, each **chained** onto the generation the previous commit wrote | **ONE** version-conditional commit for the object, built from and conditioned on **the generation the scan returned** |
| full-record copies per object | Q (one per obligation, `..prior.clone()` at v8 `:884`) | 1 (`..object.prior.clone()`, `:848`) |
| full-map encodes per object | 2Q (CAS precondition + put, per obligation, v8 `:897-898`) | 2 (`:894-895`) |
| chained CAS writes | Q−1 per object (the `repaired: HashMap<usize, InodeRecord>` at v8 `:306`) | **none** — the map, and the chain, are gone |
| `inode:2` version after leg 4's pass | 5 | 2 |

Shape: `repair_chunk` (`:915`) keeps its name and its body — it rebuilds and re-places the
fragments and now **returns** the repoint (`Rebuilt`, `:810`) instead of committing it;
`repair_object` (`:834`) applies every repoint its object is owed to ONE copy of the scanned
record and makes the one binding commit (`:887-901`). `reconcile` groups the priority-ordered
plans by object first (`:302-308`) and reports per chunk exactly as the base does
(`:310-321`), so `emit_conflict` / `emit_aborted` still fire once per chunk in `reconcile`'s
own frame and the telemetry identity `repaired − conflict − aborted == committed` is
untouched (asserted, unmodified, by `crates/custodian/tests/reconstruction.rs:1943-1948`).

**Why this closes both open items:**

* **C5 / T5 (the mandated item).** "Q plans each call `repair_chunk`, which clones and
  serializes the N-entry prior, so the prohibited Q×N CPU/bytes path remains." There is now no
  per-obligation whole-record clone or encode anywhere: `RepairPlan` carries `object: usize`
  (`:122`) and the only `InodeRecord` clones in the file are per **object** — `:536` (the
  reading's one snapshot, allocated lazily on the first chunk owed inside the object) and
  `:848` (the record the commit puts). `grep -n "clone()" crates/custodian/src/reconstruction.rs`
  returns eight lines; the other six are a single `ChunkRef` per obligation (`:543`, O(1) in
  the object's size — the base cloned one too), a key clone per refused object (`:549`), and
  the base's own three per-chunk clones at `:691`, `:926`, `:940`, unchanged.
* **The three T4 batch-review blockers** ("seeded Tier-0 DST for the chained multi-commit CAS
  path", v8 `reconstruction.rs:298`/`:309`, `tests:568`). The path they name **no longer
  exists**: nothing is conditioned on a generation this pass wrote, because each object is
  written at most once. The brief's Verification posture is now literally true of the code —
  *"every write it performs is on a flat record resolved by borrow from the generation the
  scan returned … committed under the base's own unmodified version-conditional CAS"* — so
  no DST leg is owed and none was built (`brief.md` §Verification posture; #699 assigns the
  fleet-wide write-rule DST cost to the slice that adopts it, i.e. #682). I did **not** need
  the record-rejection the sign-off rationale authorised; removing the cause beat arguing the
  symptom. If a reviewer re-raises the class anyway, the rejection stands on the same
  reference plus `crates/custodian/src/reconstruction.rs:887-901` (one CAS per object,
  conditioned on scanned bytes; a racing writer loses the CAS, the obligations stay queued,
  and the rebuilt fragments are collectable garbage — never a corrupted record).

Also carried forward unchanged and worth re-checking at sign-off: first-committed-reference-in-
key-order wins (#700 — no claimant index, no landing guard); identity stays an `InodeId` parsed
from the scanned key with the CAS re-deriving `metadata::inode_key(..)` over a re-encoded prior
(#698 — the two fields moved from `RepairPlan` to `FlatObject` (`:401-412`), where they are
one per object instead of one per obligation, but the *meaning* is byte-for-byte the base's:
parse the key, re-derive the key, re-encode the prior); no generation-restart comparison (#699);
`Reconciled`'s rustdoc untouched (#701); no write to a segmented record (#682); no third file.

## 2. The T5 oracle — what the test now measures, and what it provably falsifies

Leg 4 (`tests/segmented_map_reconstruction.rs:624`) previously asserted one scan plus Q version
bumps; the reviewer showed that an injected per-obligation `black_box(record.clone())` left it
green. It now also measures the **full-map rewrite** at the store seam, which is where a copy
of a chunk map becomes observable: the `MemMeta` double charges every `inode:` blob a commit
carries — CAS precondition and put alike — to that object (`tests:142-157`, helper at `:89`),
and leg 4 asserts (`tests:652-662`):

```
meta.rewrites(2) == (1, scanned + meta.record_len(2).await)
```

i.e. the eight-entry map holding **four** of the six obligations is carried across the seam
**exactly twice** (once as the generation the write was built from, once as the generation it
wrote) in **one** commit.

**Refutation, measured — not predicted.** I re-implemented iteration 8's per-obligation
rewrite inside `repair_object` (chained commits, same final state: all four repairs land, all
obligations discharged, the store ends identical) and re-ran:

```
thread 'the_committed_namespace_is_read_and_rewritten_once_per_pass' panicked at …:652
  left: (4, 5448)
 right: (1, 1362)
```

4 commits and 5448 bytes of full-map encodes against 1 and 1362 — exactly Q×. That is the
property C5 named, failing on an implementation whose *outcome* is correct, which is what
"observe the cost, not the result" required. The mutation was reverted; `git diff` carries no
trace of it.

**What it still cannot bind, stated plainly.** A gratuitous copy that never reaches a seam.
I re-ran the reviewer's own probe — `let _ = std::hint::black_box(object.prior.clone());` at
the top of `repair_object`'s plan loop — and the leg stays **green**, because copying a
plain-data Rust value has no observable effect anywhere outside the process's heap. The two
ways to observe the heap were costed, not hand-waved:

1. **A counting `#[global_allocator]`** needs `unsafe impl GlobalAlloc`. The test file is a
   crate root, and `cargo xtask ci`'s guard scans every target `src_path` enforcing AGENTS.md's
   hard convention "every new crate root carries `#![forbid(unsafe_code)]`"
   (`xtask/src/repo_guard.rs:387-408`, wired at `xtask/src/main.rs:1443-1484`). `forbid` cannot
   be relaxed by `#[allow]`; the only escape is an `UNSAFE_FORBID_ALLOWLIST` entry
   (`xtask/src/repo_guard.rs:148-161`) — a **third file**, which §Budget forbids outright. No
   `Cargo.toml` change is permitted either, so `stats_alloc` / `dhat` are out.
2. **A `/proc/self/status` VmHWM delta** is std-only and safe, and it is a flake generator
   here: to clear the noise floor the regression must be ≈100 MB of *retained* copies; a
   retained `ChunkRef` costs ≈72–88 B, so ≈1.4 M entry-copies — with Q = 32 that is N ≈ 44 000
   chunks in one object, each repair then encoding ≈1.3 MB twice (≈2.8 M entry serialisations
   per run), re-run per mutant by `C5-mutants` (which already timed out at 15 mutants two
   rounds ago). VmHWM is also process-global while cargo runs the six legs on parallel threads
   in one process. Compare: the shipped leg seeds 8 + 2 + 1 + 1 objects and the whole file runs
   in 0.05 s.

So the *magnitude* is bound by construction and by review-visible type instead — no
per-obligation struct holds a record or a map, and any regression that re-adds one must either
use it (⇒ the seam assertion fires) or be dead code. **Residual for the human:** gating heap
magnitude itself needs an allocator-instrumented harness that this repo's `forbid(unsafe_code)`
convention rules out of a test crate root — a repo-policy decision (an allowlist entry, or a
`#[cfg(test)]` harness crate), not something this slice can smuggle in.

## 3. Budgets — both disclosed, neither hidden

**Production, `src/reconstruction.rs`: 207 added semantic lines** (non-blank, non-comment,
counted exactly as §Budget's calibration counts) against **≤ 160**. Of those, **60 are base
lines relocated verbatim** by splitting the commit out of `repair_chunk` into `repair_object`
(they appear as `-` lines in the same diff), leaving **147 genuinely new** — inside the budget.
Anyone can re-derive it: take the `+` lines of the production hunk, drop blanks and `//`
lines, and match each against the base file's own line multiset. Iteration 8 measured
153 / 42 / **111** by the identical script; the +36 genuinely-new lines are the per-object
commit the carry-forward mandated (`repair_object`'s body and doc, the grouping in `reconcile`,
the `Rebuilt` return). I could get the *gross* number down by ~30 by leaving the commit inline
and wrapping the rebuild in a loop, but the reindentation counts as churn too, and it buries
the one thing this round is about.

**Test, `tests/segmented_map_reconstruction.rs`: 743 raw / 510 semantic** against 460 / 280 —
the overage the human accepted last round at 678, plus **65** for the T5 oracle it also
mandated (the seam meter and its two helpers ≈ 30, leg 4's rewrite assertions and their
rationale ≈ 35). Per the carry-forward I did not spend the round shrinking it; I trimmed prose
where I was already editing.

## 4. Refuting my own test (forced check)

**(a) Genuine red?** **Yes — measured through the project's own runner.**
`engine/scripts/run-verify.sh` (the C4-verify gate) applied `patch.diff` to a clean
`origin/main` worktree, reverted `reconstruction.rs`, kept the test, and got
`test result: FAILED. 1 passed; 5 failed` — the red leg **compiled and ran six tests**, so the
red is behavioural, not a missing symbol (no assertion names anything this patch introduces):

* legs 1–4: `Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` — one
  segmented object ending the pass for the whole store, which is the defect;
* leg 5: `expected ident at line 1 column 2` — the base aborting on the undecodable record
  instead of naming it and letting the later store fault end the pass;
* leg 6 green, exactly as the brief pre-declares (a regression guard, not a base red).

Gate verdict: `run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).`
Plus the targeted refutation in §2: the leg also goes red on a *correct-outcome* implementation
that merely pays the Q×N rewrite.

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step`
(`lib.rs:41`), the real fenced control point, which dispatches `reconstruction::reconcile`
(`reconciliation.rs:131-137`) under a real `Custodian::elect` + `FencedZone` fence over
`MemCoordination`; chunk maps are resolved by the real `wyrd_core::metadata::resolve_chunk_map`.
The only doubles are the two trait seams a backend implements (`MetadataStore`, `ChunkStore`) —
no mock of the behaviour under test, no re-implementation.

**(c) Fixture includes the fault?** **Yes.** Damaged objects are seeded into the same `inode:`
namespace the pass walks, ahead of the healthy work in key order over a `BTreeMap`-backed store
(leg 3: `seg:`-hole root at `inode:1`, undecodable record at `inode:2`, the repairable object at
`inode:3`), and `seed` asserts the fault is *real* — `resolves.is_err() == matches!(what,
SegmentHole)` (`tests:365-370`) — so a fixture that quietly stopped being damaged fails there
instead of passing a leg silently. Leg 5 injects its store fault on the read the **resolver**
performs and asserts the returned error carries that exact injected text. Nothing is curated
out: leg 1 keeps the healthy segmented object beside the flat work, leg 3 keeps the healthy
repair beside both damaged records, leg 4 keeps the segmented object beside six obligations —
four of them inside the one object whose rewrite cost it measures.

## 5. Gates run here (the human's Check re-runs them)

* `engine/scripts/run-verify.sh` (C4-verify) → **PASS**, red→green as quoted above.
* `./engine/xtask.sh ci` → see `check-gates`; run locally before hand-off (fmt, clippy
  `-D warnings`, the `forbid(unsafe_code)` guard, statics, deny, machete, conformance, the full
  workspace test run — including `crates/custodian/tests/reconstruction.rs` **unmodified and
  green**, 15 tests, which is the brief's oracle for the five classifications this slice's own
  legs do not drive).
* `cargo fmt` run over both touched files, so the target's own commit hooks have nothing to
  reject.
* Not run here: `C5-mutants` and the T4 batch review (driver-side).

## 6. Alternatives considered and rejected

* **Keep iteration 8's chained per-obligation commits and only strengthen the test.** Rejected:
  the carry-forward mandates eliminating the path, and the test could then only have asserted
  the Q×N cost it was measuring — i.e. it would have gone red on the shipped code.
* **Batch every object's repoint into ONE store commit for the whole pass.** Fewer commits
  still, but one object's lost CAS race would then discard every other object's repair in the
  same pass; leg 4 asserts against it (`tests:680-685`, one rewrite per object, not one per
  pass).
* **Group plans by walking `reading.objects` and filtering `plans` per object** (4 lines
  instead of 8). Rejected twice over: it is O(objects × plans) — reintroducing a product this
  very slice exists to remove — and it would process objects in key order, silently discarding
  the base's repair-priority ordering (`plans.sort_by_key`, base `:228`, proposal 0005:305-317).
* **Emit `emit_conflict` / `emit_aborted` from inside `repair_object`** (≈8 lines lighter).
  Rejected: the base pins that emission to `reconcile`'s frame in-code (base `:510-511`), and
  moving it is exactly the kind of silent convention break a reviewer should catch.
* **Rename the rebuild half to `rebuild_chunk`.** Rejected: four files outside this bundle's
  two-file budget cite `reconstruction::repair_chunk` in prose for behaviour that stays in that
  function (`crates/dst/tests/custodian.rs:226`,
  `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:225`,
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:269`,
  `crates/server/tests/custodian_day_one.rs:916`), and I may not edit them to follow a rename.
* **A `size_of::<RepairPlan>()` assertion** as a stand-in for the heap property. Rejected as a
  false oracle: it catches an owned `InodeRecord` but not an owned `Vec<ChunkRef>` (24 B inline,
  N entries on the heap) — green on the very regression it claims to guard.
* **Timing assertions.** Rejected: flaky under CI load and under `cargo mutants`.
