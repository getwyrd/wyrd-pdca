# Build notes — issue 697, iteration 11 (`reconstruction-reads-through-resolver-once-contained`)

Target branch: `getwyrd/wyrd @ main` (base `origin/main @ 339da46`). All edits made in
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`; every `path:line` below is that tree
(= the patched file), with base line numbers marked "(base)".

**Two files, as budgeted.** `crates/custodian/src/reconstruction.rs` (**143** added semantic
lines, cap 160 — unchanged from iteration 10) and the new
`crates/custodian/tests/segmented_map_reconstruction.rs` (712 raw / 492 semantic — see §6, the
overage the human pre-accepted at iteration 8's sign-off).

---

## 1. What this round is — and what it deliberately is NOT

The iteration-10 sign-off is explicit: *"Do NOT rebuild the mechanism — the resolver-once
restructure, the refusal path, and the write shape are accepted as-is. The residue is bounded
pointer-work."* So the mechanism is **byte-identical to iteration 10**. The whole code delta
against `iteration-v10/patch.diff` is **four comment hunks, zero semantic lines**:

| # | Where | What |
|---|---|---|
| 1 | `crates/custodian/src/reconstruction.rs:154-158` | the overstated complexity claim, narrowed (sign-off item 3) |
| 2 | `crates/custodian/src/reconstruction.rs:438-441` | `deferred: #702` marker on the `Ok(None)` rule (sign-off item 5) |
| 3 | `crates/custodian/tests/segmented_map_reconstruction.rs:573-575` | leg 4's claim narrowed to what it binds |
| 4 | `crates/custodian/tests/segmented_map_reconstruction.rs:587-590` | leg 4's convergence bullet, marked base-parity |

`diff` of the two patch files, with `index` lines stripped, shows exactly those four hunks plus
the `@@` offsets they shift (reproduce:
`diff <(sed 's/^index .*//' results/issue_697/iteration-v10/patch.diff) <(sed 's/^index .*//' results/issue_697/patch.diff)`).

Everything the mechanism does is described in `iteration-v10/build-notes.md` §1 and still holds:
one reading of the committed namespace per pass through `metadata::resolve_chunk_map`
(`reconstruction.rs:455`), containment by exactly `gc.rs`'s downcast rule (`:477-487`), a
per-object `Assessment::Refused` (`:581`, consumed `:255`), the one drain gate (`:323-329`), the
`Blocked` answer (`:331-340`), and the two pinned audit rows (`:1027`, `:1046`).

## 2. The six sign-off items, and where each landed

| Sign-off item | Where it landed |
|---|---|
| 1. Record-reject T4 batch finding 1 (`reconstruction.rs:300`, "repairs commit while `reading.incomplete`") **at the new line** | `review-rejected.md` — entries at `:300` (the line the finding named, unchanged: my only production edit above it is line-count-neutral) **and** `:303`, the repair loop the flagged comment describes. Reason is on the merits first: brief leg 3 REQUIRES the healthy repair to land beside an unreadable object (brief.md:76-84), the queue-entry delete rides the **same version-conditional batch** as the repoint (`:878-881`, base `:599-602`) so nothing is "silently discharged", and the duplicate-behind-an-unreadable-record residue is #700 DO-NOT-BUILD — the same finding as round 4's `:689` entry. |
| 2. Record-reject T4 batch finding 2 (`reconstruction.rs:423`, unbounded await) **at the new line** | `review-rejected.md` — entries at `:423` (as flagged) and `:474` (the call itself, which the `deferred: #702` marker moved down by 4). Reason is the round-3 `:482` rejection unchanged: `gc.rs:394-401` and `restore.rs:604-608` make the identical call unbounded and say why; the rule is restated in-code at `:449-454`; a caller-side timeout needs a production `tokio` dep = a forbidden `Cargo.toml` change. |
| 3. Narrow the overstated performance claim | `reconstruction.rs:154-158` now claims exactly one thing — *"the namespace is SCANNED once per pass instead of once per obligation"* — and states what it does **not** change: *"the per-repair map copy + encode below is the base's own, and a queue piled inside ONE object still drains over as many passes as it takes on `origin/main`."* The clone at `:865` and the encodes at `:879-880` are untouched (base `:586`, `:600-601`), per the sign-off. Same narrowing in the test's leg-4 doc (`tests/…:573-575`, `:587-590`) so the oracle's own prose no longer over-claims. |
| 4. Record-reject the C5/T5 demand for an oracle that fails on full-map copying | `review-rejected.md` — three entries (`tests/…:571`, `tests/…:611`, `src/…:865`, class TEST-GAP). The implementation half is done and readable (`RepairPlan::object` is an index, `:120`, `:520-526`); the oracle half is not honestly buildable through the `MetadataStore` / `ChunkStore` seams the discriminator is confined to — the adversary injected `black_box(encode(&record.clone()))` per obligation and all six legs stayed green, because a heap copy is invisible at a trait seam that can only count reads and writes. Manufacturing an allocation counter would be a test driving a probe instead of production. |
| 5. File the `Ok(None)` silent-drain race against #653/#682 | **Filed: <https://github.com/getwyrd/wyrd/issues/702>** — *"custodian: a repair obligation is drained, and the pass certifies, when the resolver answers `Ok(None)` for a key the scan saw Committed"* (label `bug`, milestone Foundations). It carries the adversary's executed reproduction, why it is not fixed in-slice (the fix contradicts the brief-pinned "`Ok(None)` is skipped" rule and is one answer for all four loops), and why it is unreachable on this build (`crates/core/src/metadata.rs:1460-1463` — no producer of a segmented root until #653). NOT fixed here. In-code deferral marker at `reconstruction.rs:439-441`, in the shape the repo already uses (`restore.rs:616`) and that AGENTS.md:200-203 treats as settled for review; also recorded in `review-rejected.md` at `:476`. |
| 6. Leave the two C5 "missed" mutants alone, note it | Left. Both are *equivalent* mutants: "delete field `size`" (`:868`) and "delete field `state`" (`:870`) from the `InodeRecord` expression in `repair_chunk`, both re-supplied identically by the `..object.prior.clone()` tail at `:875`, and `read_committed` admits only `Committed` records (`:471-473`). Deleting either produces a byte-identical record, so no test can kill them; only deleting the two redundant base lines removes them, which §3(c) explains I did not do. Re-measured this round: `22 mutants tested in 32s: 2 missed, 13 caught, 7 unviable` — the same two, shifted +4 by the deferral marker. |

## 3. Alternatives ruled out, with their cost

**(a) Fixing the `Ok(None)` race in-slice** (mark the reading incomplete when the resolver
answers `Ok(None)` for a key this pass's own scan returned `Committed`). Rejected: the sign-off
says file it, not fix it, and the cost is not the local hunk — it is the **fleet-wide contract**.
The local change is ~4 lines at `reconstruction.rs:476`; making it *correct* means the same
decision at `gc.rs:404` and `restore.rs:646` (two more files, both out of scope by the brief's
§Scope list) plus the seam question #702 asks — the resolver knows both facts, the caller has
only one, so today no caller can even distinguish "gone" from "retired under my read" without
re-reading the key. Fixing one loop only makes reconstruction disagree with its two merged peers
about what one `Option` means, which is the class that produced this bug. Cost of NOT fixing:
zero today — the shape has no producer in this build (`crates/core/src/metadata.rs:1460-1463`),
which is exactly why #702 is filed against the two slices (#653, #682) that create the window.

**(b) Removing the per-repair map copy + encode** to answer C5/T5 by construction
(`:865` `prior_chunk_map.to_vec()`, `:879-880` the two `metadata::encode`s). Rejected: they are
**the base's own lines** (base `:586`, `:600-601`), and they are what the CAS precondition and
the put are *made of* — `require(inode_key, encode(&object.prior))` cannot be built without
encoding the prior record, and `next_chunk_map[plan.chunk_index].placement = new_placement`
cannot be written without owning a copy of the list. Removing them means changing the write
shape, which §Scope freezes and the iteration-9 sign-off already rejected once. Concrete cost of
the only variant that would work — hold the map by `Cow` and mutate in place — measured on the
v9 attempt that tried a related restructure: **+64 production semantic lines** (207 vs 143) and
a commit shape with no counterpart on the base.

**(c) Deleting the two redundant fields the C5 mutants land on** (`size:` at `:868`, `state:` at
`:870`, both already supplied by `..object.prior.clone()`). Cost: **−2 lines**, and it clears
the advisory red. Rejected anyway, and re-affirmed by sign-off item 6: they are base lines
(base `:589`, `:591`) outside this slice's subject, and keeping the record construction
byte-identical to the base is the stronger claim for a change whose whole point is "the write is
unchanged".

**(d) Rebuilding anything the sign-off accepted** (the grouping mechanism of v9, a snapshot
refresh between commits, a wider containment rule). Rejected on instruction — *"Do NOT rebuild
the mechanism"* — and on the merits already recorded in `iteration-v10/build-notes.md` §3.

## 4. Judgment calls a reviewer will want stated

* **The narrowed claim is narrower than the brief's own wording, on purpose.** brief §Invariant's
  fifth bullet reads as a *convergence* property ("its work is bounded by the obligations it
  holds, not by their product with the namespace"). What this slice delivers, and what leg 4
  binds, is the **per-pass** property: one `scan(b"inode:")` per pass whatever Q is, and ≤ S
  `scan_page`s. The residue — Q obligations piled inside ONE object still take up to Q passes,
  each rebuilding fragments that then lose the CAS — is **base parity**: the adversary ran the
  same 8-obligation fixture against `origin/main:crates/custodian/src/reconstruction.rs` and
  measured the identical 8 passes. So the comments now say what is true and what is unchanged,
  rather than implying the slice fixed the aggregate cost too. Narrowing the *claim* (not the
  *code*) is what the sign-off asked for; the aggregate cost belongs with the grouping route the
  iteration-9 sign-off closed.
* **The deferral marker is not a fix.** `reconstruction.rs:439-441` records #702 and states why
  the answer is not this loop's to give alone. It changes no behaviour; the `Ok(None) => continue`
  at `:476` is exactly what both merged peers do (`gc.rs:404`, `restore.rs:646`) and what
  brief §Scope pins.
* **Line stability was engineered, not lucky.** The two recorded rejections had to be filed *at
  the new line*, so the narrowing hunk at `:154-158` was written to occupy exactly the four lines
  it replaced, keeping the flagged `:300` and `:423` where the review found them; the deferral
  marker sits below both, so it shifts only the resolve call (`:470` → `:474`), which is recorded
  as its own entry. The test edits are likewise line-count-neutral (712 raw, as in v10).
* **Duplicate committed `ChunkId`, key identity, generation-restart, `Blocked`'s rustdoc** —
  #700 / #698 / #699 / #701, out of scope by the brief, still untouched.
* **No DST leg, and none owed** (brief §Verification posture). Nothing about the write path
  changed this round; there is still no new concurrent or destructive path.
* **No docs / ADR / conformance / `Cargo.toml` change**; still exactly 2 files.

## 5. Gates run here (the project's own runners, not hand-rolled)

| Check | Result |
|---|---|
| `./engine/scripts/run-verify.sh` (C4-verify, red→green) | **PASS** — *"red without the fix, green with it (6 test(s) ran red)"*: production reverted + test kept → **5 of 6 legs fail behaviourally** (`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` on legs 1–4; leg 5 `expected ident at line 1 column 2` — the base's `decode(&value)?` ending the pass before any name is out), leg 6 green exactly as the brief pre-declares. Re-run after the final edit. |
| `./engine/xtask.sh ci` (C4-ci, whole tree) | **PASS** — *"xtask ci: all checks passed"* (fmt, clippy `-D warnings`, build, whole test suite incl. DST, deny, conformance, prose gates) |
| `cargo test -p wyrd-custodian` (final tree, after the last comment edit) | all green; `crates/custodian/tests/reconstruction.rs` **15 passed, unmodified**; `segmented_map_reconstruction` 6 passed |
| `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check`, `typos` | clean (the target's own commit hooks) |
| `scripts/mutants-in-diff` (C5, advisory) | `22 mutants tested in 32s: 2 missed, 13 caught, 7 unviable` — the two equivalent mutants of §2 item 6 |

## 6. Known deviations to weigh at sign-off

* **Test file shape**: 712 raw / 492 semantic vs the brief's 460 / 280 cap — **unchanged from
  iteration 10**, which the human accepted at iteration 8's sign-off (*"human accepts as fine —
  do not spend the round shrinking the file"*). This round did not add a line to it.
* **C5**: 2 missed mutants, both equivalent (§2 item 6), advisory. Left deliberately per
  sign-off item 6.
* **#702 is filed, not fixed** (sign-off item 5). It is real, demonstrated, and unreachable on
  this build; it must be answered before #653 or #682 land.
* **The recorded rejections are line-pinned guesses about where the next review will point.**
  `scripts/review-branch` matches a rejection on `(loc, class)` + a substring of the rationale,
  so each of the two batch findings is recorded at **two** locs (the flagged line and the code it
  is about) to survive a one-line drift in the reviewer's citation. If a future round's finding
  lands somewhere else again, the reason text is the thing to re-read — not a new decision.

## 7. Forced self-refutation (recorded, per the Do protocol)

**(a) Genuine red?** **Yes — measured this round, not inherited.** `run-verify.sh` applies
`patch.diff` to a clean `origin/main @ 339da46` worktree, then reverts `reconstruction.rs`,
keeps the test file, and re-runs: **5 of 6 legs fail behaviourally** — assertion / `expect`
panics on the base's `Err`, at `tests/…:461`, `:493`, `:545`, `:607`, `:680`, not compile errors
(the test names no symbol this patch introduces, so the target still builds). Leg 6 stays green,
which the brief declares in advance as a regression guard rather than a base red. Verdict line:
`run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).`

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point, elected through `Custodian::elect` over `MemCoordination` and authorized by
a real `FencedZone` (`tests/…:399-432`) — which dispatches the production
`reconstruction::reconcile`. No internal helper is called and nothing is re-implemented in the
test: the doubles implement the `MetadataStore` / `ChunkStore` **trait seams** (the store below
the pass), and the resolver under test is the production `wyrd_core::metadata::resolve_chunk_map`.
The only test-side logic is seeding and reading the store back.

**(c) Fixture includes the fault?** **Yes, and it proves its own damage.** `seed`
(`tests/…:295-331`) resolves every object it plants and asserts `resolves.is_err()` **iff** the
seeded shape is the damaged one — so no leg can pass because its fault silently stopped being
one. Leg 2 refuses over a real segmented object with real `seg:` records and asserts every
non-`repair:` row is byte-identical afterwards. Leg 3 seeds a segment the root names that was
genuinely never written plus a record whose bytes genuinely will not decode, **first in key
order** over the `BTreeMap`-backed store, so "met first" is a fixture property rather than luck.
Leg 5 injects its fault on the read the **resolver** performs (`scan_page(b"seg:…")`,
`tests/…:111-116`), never on `scan(b"inode:")`, and asserts the injected fault's own text came
back. Nothing is curated out: the damaged objects sit in the same store as the healthy repair
each leg asserts still lands.

## 8. Scratch

`$PDCA_SCRATCH/pdca-builder-697-v11/` (the #702 issue body, one `xtask ci` log) — removed at the
end of the run. The worktree keeps the patch applied, as the repo-scoped gates require.
