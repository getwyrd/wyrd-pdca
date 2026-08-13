# Build notes — issue 710 · ceiling-refused-placement-writes-do-not-certify

**Iteration 4.** Target branch `getwyrd/wyrd @ main`; worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`
at `9dbcd72` (== `origin/main`, i.e. #696/#697 merged, as `Depends on (merged)` requires).
All line citations below are **post-patch** lines in that worktree unless marked *(base)*,
which means `git show HEAD:<path>`.

---

## 0. What changed since iteration 3 — the carry-forward, addressed

The v3 attempt was gate-green everywhere that measures behaviour (C4-ci pass, C4-verify pass,
C5-mutants 0 missed) and failed **only** the T4 batched review, on two findings that are the
same finding twice:

> `crates/custodian/src/rebalance.rs:160` / `:169` — *"Matching `outcome` by value consumes the
> non-`Copy` `EvacOutcome`, so the subsequent `outcome.persisted()` call fails to compile."*

**On the facts that finding is wrong** — `EvacOutcome::Refused { bytes, ceiling }` binds only
`usize` fields, so the arm copies rather than moves and `outcome` is still live after the
`match`; that is why v3's `cargo xtask ci` (which includes `cargo build --all-targets` and
`clippy -D warnings`) was green. I have **not** relied on that. Two reviewers reading the same
five lines as a compile error is itself a defect in the code: the certification rule was placed
*after* the arms that merely name the outcome, so a reader had to prove a borrow-checker
subtlety before they could believe the loop. The human's sign-off named the remedy; I took it,
in the stronger of the two forms offered:

- `crates/custodian/src/rebalance.rs:159-165` — `unmoved |= !outcome.persisted();` now runs
  **before** `match outcome` (`:166`), not after it. Nothing reads `outcome` past the `match`,
  so no borrow question can be raised about those lines again — and the ordering is the better
  code regardless: the drain's answer is read off the outcome *once*, ahead of and independent
  of every arm. An arm that forgets to withhold certification is the entire defect this slice
  closes; the rule should not live where a future arm can forget it.

I chose reordering over `match &outcome` because with `persisted()` computed first the borrow
is moot, whereas `match &outcome` would leave a `&`-binding whose only justification is a use
that no longer exists — i.e. it would answer the review comment without answering the reader.

Two further edits, made for the same "a reviewer should not have to reconstruct this" reason:

- `crates/custodian/src/rebalance.rs:172-180` — the base arm carries an in-code marker
  `// deferred: #682` *(base `rebalance.rs:142-152`)*. The target's own review protocol
  (`AGENTS.md`, "Reviewer protocol" → *Deferrals are settled*) treats such a marker as
  binding, so **silently deleting it** is exactly the shape that comes back as a finding. The
  replacement comment now says explicitly that the deferral is **DISCHARGED, not dropped**, and
  why this slice is where it is discharged (the refusal lands in the same `match`; leaving the
  arm silent would re-create the defect for the new outcome on the day it is born) — which is
  the brief's own sanction: *"the pre-existing silent `EvacOutcome::Aborted => {}` arm is
  settled here — #696 deliberately left it to this work"*.
- `crates/custodian/src/rebalance.rs:150-156` — the `unmoved` comment no longer says "this arm"
  (it is attached to a binding, not to an arm).

No other production or test change from v3. **The design below is unchanged from v3** and is
restated in full because build-notes are the human's record at sign-off, not a diff of the
previous round.

---

## 1. The change — one rule, two write paths

> A placement write that would not survive is refused, and a move that did not persist neither
> certifies nor counts.

### 1.1 The ceiling test lives in `core`, beside the constant that is normative for it

`crates/core/src/metadata.rs:356-382` adds **one** function:

```rust
pub fn flat_value_ceiling_crossed(encoded: &[u8]) -> Option<usize> {
    (encoded.len() > MAX_VALUE_BYTES).then_some(MAX_VALUE_BYTES)
}
```

- It takes **already-encoded bytes**, so the bytes weighed are literally the bytes committed —
  no second `encode` can drift past the check (both callers pass the same `next_bytes` buffer
  straight into the `put`: `rebalance.rs:522/541`, `reconstruction.rs:923/940`).
- It weighs `MAX_VALUE_BYTES` *(base `metadata.rs:327`)*, **not** `MAX_ROOT_VALUE_BYTES`. The
  half-ceiling budgets a **segmented** root's segment table against the reserve its object
  metadata is spent from *(base `metadata.rs:329-352`)*; a flat record has no segment table —
  its whole value *is* the record. Weighing a flat record against 50 000 would refuse records
  the backend stores happily and make objects unrepairable by the very check meant to prevent
  that. The brief forbids inventing a third constant; I invented none.
- `Option<usize>` rather than `bool` so the caller's audit line can report the ceiling beside
  the record's own length without re-deriving which ceiling applied — and so a segmented caller
  (#682) can return a different one from a sibling helper without changing the call shape.
- The boundary is `>`, matching the resolver's read-side refusal *(base `metadata.rs:2465`)*:
  a record landing **exactly** on the ceiling is a value every backend stores, so refusing it
  would itself create the "cannot be re-written" object this slice exists to prevent. Both
  sides are pinned in `core`'s own tests at `crates/core/src/metadata.rs:2768` — added in
  iteration 2 because the `>`→`>=` mutant survived C5 in iteration 1.

### 1.2 Both flat write paths refuse *before writing anything at all*

- `crates/custodian/src/rebalance.rs:508-527` and `crates/custodian/src/reconstruction.rs:915-929`
  build the record the binding CAS would leave behind, weigh it, and `return
  {Evac,Repair}Outcome::Refused { bytes, ceiling }` **before** any fragment copy/write and
  before the commit. The refusal path therefore writes **nothing** — no record, and no
  unreferenced fragment that GC would have to hold forever with no grace evidence for it. It
  does **not** retract bytes already published (settled, out of scope).
- **Precedence over compound failures** (iteration 2's finding): the transient checks stay
  *first*. `rebalance.rs:452-487` resolves source/target stores and verifies fragment integrity
  before the ceiling test; `reconstruction.rs:853-886` resolves every target store before it.
  So a move that could not have proceeded anyway is named by its **recoverable** cause (an
  abort, retried next pass), never by the permanent "this record must shrink" refusal that
  pages a human. Cost of that ordering: the fragment bytes are buffered in a `copies` /
  `writes` `Vec` instead of being written as they are computed — bounded by the fragments of
  **one** chunk this pass moves (≤ `m`), and the shards were already fully in memory
  (`all_shards`), so the buffer adds no new order of memory.
- The CAS idiom is untouched: `require(key, encode(prior))` + `put(key, next_bytes)`,
  `version = prior.version + 1`, `..prior.clone()` — ADR-0047 object metadata preserved
  (`rebalance.rs:497-506`, `reconstruction.rs:904-913`).

Two rubric classes this deliberately sits inside (`AGENTS.md` → *Review rubric & protocol*):
**metadata validation boundaries** (ADR-0045) — a *contextual* check, liberal on read (the read
side is untouched; the resolver keeps its own refusal at base `metadata.rs:2465`) and strict in
the maintenance path, which is exactly the prescribed placement; and **transactions** — the
early return happens before any `WriteBatch` exists, so there is no live transaction to roll
back and nothing published to retract.

### 1.3 A move that did not persist does not certify

- **Rebalance** (`rebalance.rs:150-182`): `unmoved` is set from `EvacOutcome::persisted()`
  (`:421`, true only for `Committed`), and the pass answers `Reconciled::Blocked` when
  `scan.withheld || unmoved` (`:187`). `Blocked` already exists on base and outranks `Changed`
  (`crates/custodian/src/reconciliation.rs:44`), so no new public variant was needed.
- **Reconstruction** (`reconstruction.rs:305-320`, `:341-343`): the new `Refused` joins
  `reading.incomplete` and the segmented `reading.refused` as a **hole** in what the pass may
  claim. `Conflict`/`Aborted` keep their base certification behaviour there deliberately —
  the brief settles the silent arm in **rebalance** only, and reconstruction's aborts are
  already named and offset; widening that is #682's call, not this diff's.

### 1.4 The refusal joins the accounting identity rather than inflating it

`emit_repaired` fires up front for every dispatched repair *(base `reconstruction.rs:257`)*, so
an outcome that does not commit must be **subtracted** or it reads as a success. The new
`emit_ceiling_refused` (`reconstruction.rs:1156`, `rebalance.rs:673`) does exactly what
`emit_conflict`/`emit_aborted` do, and every doc comment that states the identity is updated to
`repaired − conflict − aborted − ceiling_refused` (`reconstruction.rs:269-277`, `:1020-1024`,
`:1129-1134`). It is **warn**, not info, because unlike the other two it is not transient: the
backlog it leaves never drains on its own, which is precisely the operator's signal.

---

## 2. Alternatives ruled out — with their costs, not adjectives

1. **Enforce the ceiling inside `MetadataStore::commit` / a shared committer.** Rejected: the
   brief scopes this to "carve out **only** the ceiling helpers the write path needs — **not**
   the committer around them", and a commit-level check cannot distinguish "never retry this
   shape" from "retry next pass" — it would surface as an `Err` at the same seam a transient
   fault does, which is the *existing* defect. Concrete cost: every `MetadataStore` impl in the
   workspace would need the rule or inherit it from a wrapper — `crates/metadata-redb`,
   `metadata-tikv`, `metadata-fdb`, plus every in-test double (`grep -rl "impl MetadataStore
   for" crates/` → **10 files under `crates/custodian/tests/` alone**), against 2 call sites
   in the chosen shape.
2. **Refuse at `encode()`** (make encoding fallible). Rejected: `encode` is called on read-back
   paths and in tests everywhere (`git grep -c "metadata::encode" crates/` is in the hundreds);
   turning it into `Result` is a workspace-wide signature change for a rule that binds two
   maintenance writers. Also wrong in kind: a record can be legally *published* at any size the
   backend accepts; what is illegal is a *repair* that grows one past the ceiling.
3. **A third constant** (`MAX_PLACEMENT_RECORD_BYTES` or similar). Rejected outright by the
   brief, and it would decouple the check from the `const _: () = assert!(...)` that ties the
   two existing halves *(base `metadata.rs:354`)*.
4. **Shrink the record instead of refusing** (e.g. drop object metadata to fit). Rejected:
   ADR-0047 metadata preservation is a stated invariant of a placement-maintenance commit, and
   silently discarding a client's `content_type` to make room is a data-loss-shaped fix for a
   durability bug.
5. **Emit the refusal on the existing `reconstruction_aborted` counter** (so the documented
   identity needs no new term, and ADR-0011's table stays literally true — a ~4-line diff
   instead of ~30). Rejected: it conflates a *permanent* record defect with a *transient*
   retry, which is the one distinction the operator needs — the brief requires the refusal be
   "distinguishable by the caller from a lost CAS" and the audit line says NEEDS-HUMAN. The
   identity is *extended* instead, which is what the brief's "must **join** that identity rather
   than inflate it" asks for. See §6 for the ADR-0011 currency question this raises.
6. **Guarding the symptom instead of the cause.** Worth stating plainly: this patch does not add
   a probe that notices oversized records after the fact — it removes the cause (the unweighed
   write) at both sites that can create one. The remaining flat-path writer that can still cross
   the ceiling is `backfill.rs`, which the brief explicitly assigns elsewhere (#695).

---

## 3. The test — `crates/custodian/tests/placement_ceiling.rs` (NEW, 556 lines)

Five `#[tokio::test]`s, all driving the **real fenced control point** `reconcile_step` over the
production `reconstruction` / `rebalance` passes, with **real** `FsChunkStore` D servers (which
verify fragment identity + checksum on `put` and `get`, so a bogus fixture fails loudly instead
of proving nothing) and an in-memory `MetadataStore` double — the shape the brief mandates.

| Test (`:line`) | Leg | What it binds |
|---|---|---|
| `a_repair_that_would_cross_the_value_ceiling_is_refused_and_not_persisted` (`:309`) | 1 | stored record **byte-identical**, ≤ `MAX_VALUE_BYTES`; obligation still queued; no stranded rebuilt fragment; pass ≠ `Satisfied`; refusal named on the seam |
| `an_evacuation_that_would_cross_the_value_ceiling_does_not_certify_the_drain` (`:402`) | 2 | same, on the drain path, **plus** `reconciliation_status == Pending` while the pass may not answer `Satisfied` |
| `an_evacuation_landing_exactly_on_the_value_ceiling_still_commits` (`:434`) | 2 | the admissible side of the boundary — a record landing exactly on the ceiling still commits (`>`, never `>=`) |
| `a_move_that_cannot_reach_its_fragment_is_aborted_not_refused` (`:462`) | 2 | compound-failure precedence: the transient cause wins, and the abort still does not certify |
| `a_ceiling_refused_repair_is_subtracted_from_the_reported_successes` (`:488`) | 3 | `repaired − conflict − aborted − ceiling_refused` == obligations actually drained by a commit, over one pass with one repaired + one refused + one aborted chunk |

`crates/custodian/tests/rebalance.rs` — the two deliberately pinned base assertions this rule
flips, the brief's **named** fifth file: `:963-975` (no free distinct domain → was `Satisfied`,
now `Blocked`) and `:1336-1350` (lost CAS → was `Satisfied`, now `Blocked`). Both rewrites carry
the reason in-comment.

**Hard constraint honoured:** the test names no symbol this patch introduces
(`flat_value_ceiling_crossed`, `Refused`, `persisted` appear nowhere in it); the new counters
are observed as *strings* on the Prometheus surface. Proof: the RED leg **compiled and ran**
(see §4a) instead of reporting UNVERIFIABLE.

---

## 4. Forced refutation of my own test

**(a) Genuine red?** **Yes** — measured, not asserted. `PDCA_BUNDLE=results/issue_710
./engine/scripts/run-verify.sh` (the configured C4-verify gate) applies the patch to a clean
worktree off `origin/main`, then reverts the production change and keeps the test:

```
GREEN: test result: ok. 5 passed; 0 failed
RED  : test result: FAILED. 1 passed; 4 failed
  a_repair_..._refused_and_not_persisted   → "100019 bytes stored, past the ceiling"
  an_evacuation_...does_not_certify_the_drain → left: Changed,   right: Blocked
  a_ceiling_refused_repair_is_subtracted...   → left: Changed,   right: Blocked
  a_move_that_cannot_reach_its_fragment...    → left: Satisfied, right: Satisfied (assert_ne!)
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

The first line is the brief's stated base behaviour reproduced exactly: **100 019 bytes
committed** where the ceiling is 100 000 — the un-overwritable record. The fifth test
(`..._exactly_on_the_value_ceiling_still_commits`) passes on base by design: it pins the
admissible side, so it *must* be green both ways; it is red only against a `>=` mutant, which
C5 confirms is caught.

**(b) Production path?** **Yes.** Every leg enters through `wyrd_custodian::reconcile_step`
(the fenced control point, elected `Custodian` + installed `FencedZone`), which dispatches the
real `reconstruction::reconcile` / `rebalance::reconcile` — the functions this patch edits.
Nothing is re-implemented in the test: the D servers are the real `chunkstore-fs` backend, the
fragments are produced by the production encoder (`erasure::encode` +
`write::encode_ec_fragment`), the records by production `metadata::encode`, the obligations by
`repair::enqueue_repair`, and every assertion reads the **store** or the **telemetry exporter**
back afterwards. The only double is the `MetadataStore` (in-memory), which the brief mandates —
and mandates *because* it has no ceiling of its own, which is what makes the oversized commit
observable at all as a stored byte length.

**(c) Fixture includes the fault?** **Yes** — the fault is *in* every fixture, not curated out:
the seeded root is padded to land **exactly on** `MAX_VALUE_BYTES` (or on `MAX_VALUE_BYTES + 1`
after the move, computed by encoding the post-move record and padding *that*, so the growth is
whatever the id widths really cost rather than a hard-coded delta); the only free distinct
failure domain is held by the twenty-digit `u64::MAX` server, so the repoint really does cross;
the draining server really does still hold the fragment at assert time; the compound leg really
does withhold the fragment from its D server; and leg 3's pass really does contain all three
outcomes at once (asserted: `(refusals, aborts) == (1, 1)` plus an independent commit count read
from the repair queue, not from the metric being checked).

---

## 5. Gates run here (the driver re-runs them at Check)

| Gate | Command | Result |
|---|---|---|
| C4-verify (red→green) | `PDCA_BUNDLE=results/issue_710 ./engine/scripts/run-verify.sh` | **PASS** — 5 green with fix, 4 red without (file still compiles) |
| C4-ci | `./engine/xtask.sh ci` | **PASS** — "xtask ci: all checks passed" (fmt, clippy -D warnings, build --all-targets, workspace tests, cargo-deny, cargo-machete, conformance vectors, statics, DST) |
| C5-mutants (advisory) | `PDCA_BUNDLE=… ./scripts/mutants-in-diff` | **PASS** — 20 mutants: 11 caught, 9 unviable, **0 missed** |
| formatter / commit-readiness | `cargo fmt --all`, `typos <touched files>` | clean |

**External dependencies** (the brief's five `[[doctor.checks]]` ids) all present locally:
`typos`, `docs-renderer` (`markdown_it`+`yaml`), `cargo-deny` **0.20.2** (> the 0.20.0 floor the
brief warns about), `cargo-machete`, `cargo-mutants`. Nothing else was needed — no Docker, no
protoc, no live backend, no new dependency. **No NEEDS-HUMAN external dependency.**

---

## 6. Open questions for the human at sign-off (carried, with my position)

1. **T2 Shape — semantic-line budget (≤250).** Measured on the final patch: **865 added lines,
   459 non-blank/non-comment**, of which **97 are production**
   (`metadata.rs` 14 · `rebalance.rs` 48 · `reconstruction.rs` 35 · `tests/rebalance.rs` 5 —
   the last being the two flipped assertions the brief names) and **362 are the new test file**,
   ~197 of which are the pre-`// ---- legs ----` harness (imports, the `MetadataStore` double,
   the fleet/record builders, the pass runners). The brief's budget excludes "mechanical" lines;
   under any reading the *decision content* is the 97 production lines plus ~165 leg lines. I did
   not trim the test: the two tests that push it over (`..._exactly_on_the_value_ceiling...`,
   `..._aborted_not_refused`) exist because **rounds 1 and 2 demanded them** (a surviving
   `>`→`>=` mutant, and compound-failure precedence), and every custodian integration test rolls
   its own `MetadataStore` double — there is no shared one to borrow (verified: `wyrd-testkit`
   exports `test_double_scan_page` but no store double; the only `impl MetadataStore` in testkit
   is inside its own `mod tests`). The size backstop was explicitly waived by the human at
   round 3; this row is the *classification* question, not the backstop.
2. **ADR-0011 currency.** `docs/design/adr/0011-…:34-42` tabulates the durability counters and
   states the identity `repaired − conflict − aborted`; this slice adds a fourth term. I did
   **not** edit it: the brief puts "any new or edited ADR / spec / proposal" out of scope and
   caps the diff at five named files ("a sixth file means the shape is wrong"), and ADR-0011
   itself delegates — *"the source of truth for their exact emission is
   `crates/custodian/src/reconstruction.rs`"* — which this patch updates. The repo rubric's
   *Docs currency* rule enumerates ports, API operations, RPCs, CLI flags and persisted fields;
   a metric term is none of those. **If the human wants the ADR row added, it is a one-line
   table entry + one identity line and belongs in the publish commit or a follow-up.**
3. **Validation — fitness to purpose.** A refused object stays queued and its drain stays
   `Blocked` until something shrinks the record; the audit line is `warn` and says NEEDS-HUMAN.
   That is the intended trade: the alternative is an object whose placement can *never* be
   repaired (silently, on a store with no native enforcement) or an `Err` indistinguishable from
   a transient fault. Worth the human confirming that a permanently-`Blocked` drain plus a warn
   line is the operationally right shape.
4. **T5 — lost-CAS conflicts are deliberately *included* in "did not persist"** (deferred at
   round 2, now visible in the patch): `rebalance.rs:421` makes `persisted()` true only for
   `Committed`, so a lost CAS also withholds drain certification, and
   `crates/custodian/tests/rebalance.rs:1336-1350` was rewritten accordingly. Rationale: the
   fragment is still on the draining server whichever way the move failed, and that is the only
   fact the drain's answer is about. Reconstruction is untouched here (its conflicts already
   emit and stay queued; its `Blocked` is about the *reading*, not about one repair).
5. **T4 Contribution.** `scripts/review-branch` / `scripts/pdca` / a closed-work index live in
   the **wrapper** repo, not in the target checkout the reviewer is allowed to read — that is
   why this row keeps coming back NEEDS-HUMAN. Nothing in the patch can clear it.

---

## 7. Scratch

Logs under `$PDCA_SCRATCH` (`pdca-builder-710-{verify,ci,mutants,final}.log`); no other scratch
dirs created. The `../wyrd-verify-l0` worktree is the gate's own, managed by `run-verify.sh`.
