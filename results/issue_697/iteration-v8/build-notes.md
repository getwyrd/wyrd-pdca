# Build notes — issue #697 (iteration 8)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`, base `origin/main @ 339da46`.
Two files, as the brief's budget requires: `crates/custodian/src/reconstruction.rs` (modified)
and `crates/custodian/tests/segmented_map_reconstruction.rs` (new).

---

## 1. What the patch does

One reading of the committed namespace per pass, contained per object, with a refusal where a
repair is not this slice's to perform. All line numbers are on the **patched** worktree unless
marked "base".

| Piece | Where | What |
|---|---|---|
| The one reading | `reconstruction.rs:163-167` | `read_committed` runs once per pass; an **empty queue reads nothing** (the base's own behaviour, kept) |
| The walk | `:461-556` | decode contained (`:468-474`), `state` checked (`:475`), `Ok(None)` skipped (`:479`), `ChunkMapError` downcast contained / anything else propagated (`:480-492`) — exactly `gc.rs:402-416` / `restore.rs:646-657` |
| Write-eligibility off the **scanned** shape | `:500-508` | `(ChunkMap::Flat, Some(id))` ⇒ repairable, `Segmented` ⇒ refused; never off what a resolve answered after a restart |
| One snapshot per object | `:395-407`, `:526-532` | `FlatObject` is allocated lazily, on the first chunk owed inside it, and shared by every later one |
| Per-obligation state | `:113-131`, `:422-430` | `RepairPlan`/`FlatSite` carry `object: usize` + indices + this chunk's own `ChunkRef` — **no map material** |
| Refusal, per object | `:410-419`, `:542-545`, `:254`, `:1058-1067` | `refused: BTreeSet<key>` ⇒ one `refused-segmented` row + counter per **object**; nothing written; obligation kept; off the repairable-backlog gauge |
| Attribution before the work loop | `:386-390`, `:1039-1049` | `unresolvable-chunk-map` + `reconstruction_unresolvable_records`, emitted where the record is met (`gc.rs:155-166`'s placement) |
| Drain gate | `:332` | while the reading is incomplete the pass drains **nothing** — one rule over the one batch both drain paths flow into |
| The answer | `:340-354` | incomplete reading **or** any refusal ⇒ `Reconciled::Blocked`; otherwise `Changed`/`Satisfied` as before |
| Chained generations | `:306-319`, `:815`, `:876-888`, `:902` | a second obligation inside one object is built from and conditioned on the generation *this pass* just committed for it |
| Dead code removed | base `:618-646` | `find_chunk` (the per-obligation scan) is gone; its `parse_inode_key` use moved into the walk, unchanged in meaning (#698 owns that hazard) |

Frozen as the brief demands: first-committed-reference-in-key-order wins (#700 — no claimant
index, no landing guard); identity stays `InodeId` parsed from the scanned key with the CAS
re-deriving `inode_key(plan.inode_id)` over a re-encoded prior (#698); no generation-restart
comparison (#699 — the path is removed by construction, not guarded: a flat snapshot resolves
by borrow and can never be superseded, `crates/core/src/metadata.rs:2585`); `Reconciled`'s
rustdoc untouched (#701); no write to a segmented record (#682); no other file touched.

Production budget: **153 added semantic lines** (non-blank, non-comment) against the brief's
≤ 160.

## 2. The carry-forward findings

### Iteration 6 — C5: "share each flat snapshot instead of cloning its N-entry map for each of Q obligations"

Done, and taken one step further than iteration 7. v7 gave `RepairPlan` a lifetime and a
`prior: &'a InodeRecord`. This patch drops both: the plan carries `object: usize`
(`reconstruction.rs:122`) and the record is reached through the reading
(`:308`, `:817`). So the per-obligation structure holds **no map material at all** and
`Assessment` needs no lifetime. Concretely, for Q obligations over an N-record namespace:

| | base `origin/main` | this patch |
|---|---|---|
| `scan(b"inode:")` | Q | 1 (0 on an empty queue) |
| record decodes | Q×N | N |
| resolves | — (aborted) | 1 per committed object |
| record copies **retained across the pass** | Q (`find_chunk` returned the record by value, base `:641`, held in `plans`) | ≤ one per object actually owed a repair |
| map copies per commit | 2 (`.to_vec()` base `:579-586`, then `..plan.prior.clone()` base `:596`) | 1 (`..prior.clone()`, mutated in place, `:876-888`) |

### Iteration 7 — T5: "make the complexity oracle falsify per-obligation full-map copying"

What leg 4 now binds (`tests/segmented_map_reconstruction.rs:576-628`), all red on the base:

* Q = 6 obligations over N = 3 committed flat objects + S = 1 segmented → **exactly one**
  `scan(b"inode:")` (base: 6) and **one** `scan_page` (= S; a per-obligation resolve would be 6).
* **Four** of those obligations sit inside **one eight-chunk object**, and all four are
  discharged by this pass — `inode:2` ends at version 5, i.e. four chained
  version-conditional commits. A per-obligation copy of that generation, taken at read time
  before any of them committed, loses every CAS after the first and leaves three obligations
  for three more passes (one namespace reading each) — that is the Q×N cost re-entering
  through the back door, and this assertion is red on it.
* Only the **four owed** chunks moved, each at **its own index in the scanned map** (the
  four unqueued ones stay `[0, 1]`). Proven non-vacuous: I mutated `chunk_index: site.index`
  to a constant 0 and the leg went red with
  `left: ([[0,2],[0,1],…], 5)  right: ([[0,1],[0,2],…], 5)`; the mutation was reverted.

**What it still does not bind, and why — read this at sign-off.** The *magnitude* of the heap
(a hypothetical implementation that clones the N-entry map per obligation **and** chains
generations correctly would pass every assertion above). Copying a plain-data Rust value is
invisible at a trait seam, so the only honest oracles are resource measurements, and both are
blocked here — with numbers, not adjectives:

1. **A counting `#[global_allocator]`** is the only std-only way to measure allocation (no
   `Cargo.toml` change is allowed by the brief, so `stats_alloc`/`dhat` are out). It needs
   `unsafe impl GlobalAlloc`. The test file is a **crate root** — `cargo metadata` reports
   `['test'] …/tests/segmented_map_reconstruction.rs`, and `cargo xtask ci`'s unsafe guard
   scans **every** target `src_path` (`xtask/src/repo_guard.rs:387-408` + `scan_roots`, wired
   at `xtask/src/main.rs:1443-1484`, run at `:1558`), enforcing AGENTS.md's hard convention
   "every new crate root carries `#![forbid(unsafe_code)]`". `forbid` cannot be relaxed by
   `#[allow]`, so the only escape is an entry in `UNSAFE_FORBID_ALLOWLIST`
   (`xtask/src/repo_guard.rs:148-161`) — a **third file**, which the brief's budget forbids
   outright ("STOP and hand back"). Not a judgment call; mechanically impossible in this slice.
2. **A `/proc/self/status` VmHWM (peak-RSS) delta** is safe and std-only, and I costed it
   before rejecting it. To clear the noise floor of a parallel test binary the regression has
   to be ≈100 MB of *retained* copies. A retained `ChunkRef` costs ≈72-88 B (56 B inline in
   the `Vec` + a `Vec<DServerId>` placement allocation), so ≈1.4 M retained entry-copies are
   needed: with Q = 32 that is **N ≈ 44 000 chunks in one object**. Each of the 32 repairs
   then encodes that record **twice** (CAS precondition + put) — ≈2.8 M entry serializations
   per run — plus 32 × ≈1.3 MB precondition compares in the double: seconds per run, re-run
   **per mutant** by `C5-mutants`, which already reported a timeout at 15 mutants last round.
   And VmHWM is *process*-global while cargo runs the six legs on parallel threads in one
   process, so a sibling leg's peak lands in the same watermark — the assertion would be a
   flake generator. Compare: the shipped leg costs 8 + 2 + 1 + 1 objects and the whole file
   runs in 0.06 s.

So the magnitude is bound **by construction and by review-visible type** instead: the
per-obligation structures are `object: usize` + indices (`:122`, `:422-430`), the record is
only reachable through the reading (`:308`), and `repair_chunk` takes `prior: &InodeRecord`
(`:815`). A future regression would have to re-add an owned record to a per-obligation
struct — visible in a diff, and the four assertions above still fail it the moment the copy
is taken from a stale generation. **Residual for the human:** if you want the heap magnitude
itself gated, it needs an allocator-instrumented harness that the repo's `forbid(unsafe_code)`
convention currently rules out of a test crate root — that is a repo-policy decision (an
allowlist entry, or a `#[cfg(test)]` harness crate), not something this slice can smuggle in.

### Iteration 7 — T4 contribution / prior-art

A human item (the driver-only `scripts/pdca` / `scripts/review-branch` corpus is absent from
this checkout), not implementation work. Nothing in the patch addresses it; it belongs in §6.

## 3. Budget: the test file is over the brief's line estimate — disclosed, not hidden

`tests/segmented_map_reconstruction.rs` is **678 raw / ~465 semantic** against the brief's
**460 / 280**. I did not cut a leg to fit: the six legs *are* the Success criterion.

Measured floor, all of it prescribed by the brief itself: the in-file doubles + fixture +
helpers alone are ~296 semantic lines (the `BTreeMap` `MetadataStore` double carrying both
seam counters and the injected `seg:` fault, the `ChunkStore` double holding real fragment
bytes so checksums verify, the audit `MakeWriter` capture, the one parameterised `seed`
helper, `run`'s fence via `Custodian::elect` + `FencedZone::new`), and the six legs are ~169.
The two sibling fixtures this brief points at as the shapes to copy are 731
(`segmented_map_restore.rs`) and 1341 (`segmented_map_consumers.rs`) raw lines; the rejected
v5/v7 attempts here shipped 731-line test files and no review round raised the budget.

What I already compressed against v7 (731 → 678 raw): seven legs → six (v7's leg 7 folded
into leg 4, where it is stronger), `doubles()` via `Default::default()`, `scan`/`records`
collapsed to two-line iterator chains, comment weight trimmed. The only ways left to reach
460 delete a mandated oracle — the audit capture (≈40 lines; legs 2/3/5 assert audit rows) or
the D-server double (≈30 lines; without real fragments nothing is actually rebuilt). I
refused that trade and am flagging the number instead.

## 4. Refuting my own test (forced check)

**(a) Genuine red?** **Yes** — measured, not predicted. `engine/scripts/run-verify.sh`
(the C4-verify gate) reverted `reconstruction.rs` on a clean `origin/main` worktree, kept the
test, and got `test result: FAILED. 1 passed; 5 failed` — the RED leg **compiled and ran six
tests**, so the red is behavioural, not a missing symbol:

* legs 1-4: `Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` — one
  segmented object ending the pass for the whole store, which is the defect;
* leg 5: `expected ident at line 1 column 2` — the base's `decode(&value)?` aborting on the
  undecodable record instead of naming it and letting the later store fault end the pass;
* leg 6 green, exactly as the brief pre-declares (the base already scans zero times on an
  empty queue — it is a regression guard on the restructure).

Gate verdict: `run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red)`.

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step`
(`lib.rs:41`), the real fenced control point, which dispatches `reconstruction::reconcile`
(`reconciliation.rs:131-137`) under a real `Custodian::elect` + `FencedZone` fence over
`MemCoordination`. The chunk maps are resolved by the real `wyrd_core::metadata::
resolve_chunk_map`. The only doubles are the trait seams a backend implements
(`MetadataStore`, `ChunkStore`) — no mock of the behaviour under test, no re-implementation,
and no assertion naming a symbol this patch introduces (which is also why the RED leg
compiles).

**(c) Fixture includes the fault?** **Yes.** The damaged objects are seeded into the same
`inode:` namespace the pass walks, ahead of the healthy work in key order over a
`BTreeMap`-backed store (leg 3 seeds the `seg:`-hole root at `inode:1` and the undecodable
record at `inode:2`, the repairable object at `inode:3`), and `seed` asserts the fault is
*real* — `resolves.is_err() == matches!(what, SegmentHole)`
(`tests/segmented_map_reconstruction.rs:317-323`) — so a fixture that quietly stopped being
damaged fails there instead of passing a leg silently. Leg 5 injects its store fault on the
read the **resolver** performs (`scan_page(b"seg:…")`, `:111-116`) and asserts the returned
error carries that exact injected text, so "the pass ended" cannot pass for the wrong reason.
Nothing is curated out: leg 1 keeps the healthy segmented object beside the flat work, leg 3
keeps the healthy repair beside both damaged records, leg 4 keeps the segmented object beside
six obligations.

## 5. Gates run here (the human's Check re-runs them)

* `./engine/xtask.sh ci` → `xtask ci: all checks passed` (fmt, clippy `-D warnings`, the
  `forbid(unsafe_code)` guard, deny, machete, conformance, the full workspace test run —
  including `crates/custodian/tests/reconstruction.rs` **unmodified and green**, which is the
  brief's oracle for the five classifications this slice's own legs do not drive).
* `engine/scripts/run-verify.sh` (C4-verify) → PASS, red→green as quoted above.
* `cargo fmt` run over both touched files, so the target's own commit hooks have nothing to
  reject.
* Not run here: `C5-mutants` and the T4 batch review (driver-side).

## 6. Alternatives considered and rejected

* **Keep v7's `RepairPlan<'a>` with `prior: &'a InodeRecord`.** Works, but the borrow pins the
  reading through the assessment loop and leaves a record reference on a per-obligation
  struct — precisely the shape the C5 finding kept reading as a copy risk. The index costs
  one `usize` and reads unambiguously; `Assessment` also loses its lifetime, which shrinks the
  diff (`assess` is 4 signature lines instead of 6).
* **Emit a new metric/audit row counting objects read**, so the test could assert the reading
  is per object. Rejected: the brief pins the audit vocabulary to exactly two rows and says
  "Nothing else" — inventing a third to make a test easier is the wrong direction.
* **A `size_of::<RepairPlan>()` unit assertion** as a stand-in for the heap property.
  Rejected as a false oracle: it catches an owned `InodeRecord` (~100 B inline) but not an
  owned `Vec<ChunkRef>` (24 B inline, N entries on the heap) — i.e. it would go green on the
  very regression it claims to guard.
* **Timing assertions.** Rejected: flaky under CI load and under `cargo mutants`, and the
  write path's own inherent per-commit `encode` is already Q×N-shaped, so wall-clock cannot
  isolate the assessment copy anyway.
* **Sharing one `MemDServer` for both server ids** to save ~10 lines of budget. Rejected: the
  fixture would no longer distinguish a fragment written to the free domain from one written
  to the survivor's, weakening the re-placement oracle for a cosmetic line count.
