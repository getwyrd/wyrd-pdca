# Build notes — issue 695 / backfill-reads-through-resolver-contained (iteration 8)

> Withheld from the reviewer by the driver; written for the human at sign-off.

Target: `getwyrd/wyrd @ main`, base `origin/main` = `339da46` (verified: `git rev-parse
origin/main` in `$PDCA_WORKTREE` = `339da46d1d6e6473f655a095d9308fe224a4ac8d`, equal to the
worktree HEAD). All edits made in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`;
line citations below are that tree unless marked "(base)".

Two files, exactly as budgeted:

| File | Added semantic (non-blank, non-comment) | Budget |
|---|---|---|
| `crates/custodian/src/backfill.rs` | **61** | ≤ 95 |
| `crates/custodian/tests/segmented_map_backfill.rs` (new) | **236** semantic / **393** raw | ≤ 240 / ≤ 400 |

(Counted the same way the round-7 reviewer counted — `grep -v '^[[:space:]]*$' | grep -v
'^[[:space:]]*//'`; that method reproduces the reviewer's "240 semantic / 390 raw" for the
previous attempt's test file exactly, so the numbers above are on the same scale.)

---

## 1. What the change is, and why this shape

**The defect (base).** `backfill::reconcile` read the chunk map inline out of the record at two
sites — `crates/custodian/src/backfill.rs:98-101` (base) in the fill walk and `:180-183` (base) in
`emit_remaining` — each `…as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`. So one
segmented object returned `Err` for the *whole store*: no record anywhere got filled, and the drain
gauge was never published at all. A record that would not `decode` did the same at `:80` / `:174`
(base), before any resolver was involved.

**The fix** is the composition the two merged peers already perform, applied to the fourth loop:

* read every committed record through the shared resolver — `metadata::resolve_chunk_map(ctx.meta,
  &key, &record)`, `backfill.rs:156` — mirroring `gc.rs:402` and `restore.rs:644`;
* contain per object by *exactly* the peers' downcast rule (`gc.rs:402-416`): a typed
  `ChunkMapError` names the object and the walk continues (`backfill.rs:163-171`); anything else is
  a store fault and still propagates (`backfill.rs:175`); `Ok(None)` is skipped exactly as
  `gc.rs:404` / `restore.rs:646` skip it (`backfill.rs:162`);
* contain an undecodable record the same way, at the same point in the walk as `gc.rs:378-384`
  (`backfill.rs:135-142`);
* **decline** — never write — a fill whose chunks live in `seg:` records (`backfill.rs:219-222`);
* count the population in the *same* walk and publish it with the bound that qualifies it
  (`backfill.rs:202`, `:259`, `:274`, emitter at `:309-314`);
* refuse to certify a pass that answered over less than the committed store — `Reconciled::Blocked`,
  the vocabulary `reconciliation.rs:44` already carries, for the reason `gc.rs:234-246` states
  (`backfill.rs:276-283`).

**Attribution placement** is `gc.rs:155-166`'s, not just its call: the name is emitted *the moment
the walk meets the record*, ahead of every store read that follows, so a later `?` cannot cost the
operator the name of the record to repair. Names go through `gc::object_name` (`gc.rs:470-480`),
which escapes rather than replaces, so two damaged records never arrive under one name.

**Vocabulary** is exactly what the brief pinned, no more: `action = "unresolvable-chunk-map"` +
`monotonic_counter.backfill_unresolvable_records` (`backfill.rs:328-338`, the same `action` string
`gc.rs:563-573` and `restore.rs:825-835` publish, so one grep finds all three); `action =
"declined-segmented"` + `monotonic_counter.backfill_declined_records` (`:348-358`);
`gauge.backfill_placement_incomplete` beside `gauge.backfill_placement_remaining` on one event,
each with its own `gauge.` prefix (`:309-314`) so the OTel bridge does not turn the second into an
attribute that splits the operator's series. Every one of the four is asserted by a leg.

**Frozen, deliberately** (the brief's §Scope carve-outs, tracked as #698): `parse_inode_key`
(`backfill.rs:64-70`), its skip (`:146-148`), the CAS key `metadata::inode_key(inode_id)` with the
`metadata::encode(&record)` precondition (`:239-242`), and the `inode_id` audit fields of
`emit_backfilled` / `emit_conflict` are byte-identical to `origin/main` — confirmed by reading the
patch: no hunk touches them. The generation-restart comparison (#699) is **not** built; the
constraint is honoured by construction instead (see §3).

## 2. The invariant restored

C-1 (`docs/principles.md:109`, §6 row *Storage lifecycle / reclamation* `:137`) over the pass that
fills placements: it now reads every committed object the way every other consumer reads it; a
fault it meets is contained to the object that owns it and the answer is still made for the rest;
and it never certifies a drain it did not complete. The smallest change that restores that is the
one above — not smaller: dropping the refusal (`incomplete`/`Blocked`) would leave a pass that
answers `Satisfied` over a store it could not read, which is the operator-facing half of the
defect, and dropping the single-walk gauge would leave `emit_remaining`'s own `as_flat()?` abort in
place (the second of the two defect sites).

## 3. Alternatives considered and rejected — with costs

**(a) Guard the symptom: keep the inline read, skip segmented records.** `record.chunk_map.as_flat()`
→ `else { continue }` is a 2-line diff. Rejected: it does not restore the invariant. A skipped
object is one the pass silently did not answer for, and the pass would still say `Satisfied` — the
"silent skip" the repo rubric names explicitly ("Absent or unsupported entries: … never silent
success, silent skip"). It also leaves the *second* site (`emit_remaining`, base `:180-183`)
aborting, so a segmented store still publishes no gauge at all. Cost is not the axis here: the
brief names an invariant to restore, so the target is the smallest change that restores it
(`docs/principles.md` §1.2, §2).

**(b) The closed PR #647's shape — aggregate error at the end + a shared `crate::resolve` module.**
This is real prior art, and it is *rejected work*: PR #647 (CLOSED 2026-07-30, never merged) took
`backfill.rs` to a `SegmentedPlacementUnfillable` error raised after the walk, plus
`crate::resolve::{classify_root, contain, BACKFILL}` shared across six walks, and decided each
record on `live.record` (the re-resolved generation) rather than the scanned one. Cost, measured on
that PR's own diff: `gh pr diff 647` shows the `backfill.rs` hunk alone at **+119/−9** lines,
before the new `crates/custodian/src/resolve.rs` module it depends on. It is also
directionally wrong for this slice on two counts the brief pins: an `Err` is not the answer a
maintenance pass owes (`reconciliation.rs:36-44` — `Blocked` exists precisely so a per-object fault
does not end the step for every healthy object), and deciding on `live.record` is the
generation-restart path the brief closes by construction and carved out to #699. Reused: nothing
mechanical; it is cited here only as the closed-work prior art.

**(c) A `BTreeSet<Vec<u8>>` of unreadable keys instead of the `incomplete` counter.** Would give
de-duplication by key for free. Rejected: it buys nothing here (each key is visited exactly once by
one `scan`, so a set of keys and a count of increments are the same number), costs an import and a
per-object allocation on the hot walk, and the gauge needs a scalar anyway. The de-duplication
`restore.rs:711-717` needs exists because *two* reads of the namespace can meet the same record;
this pass has one.

**(d) Recompute the gauge with a second resolving scan (the base's structure, resolver-aware).**
Rejected on measurable cost: it re-reads the whole `inode:` namespace and, for every segmented
object, spends its `seg:` range read a second time — 2 namespace scans and 2·S range reads per pass
where 1 and S suffice. Leg 4 and leg 1 pin the cheaper shape (`reads(inode:) == 1`, `reads(seg:) ≤
S`); the base fails the first of those with `left: 2, right: 1`.

**(e) Bounding the resolve await with a caller-side timeout.** Rejected for the reason `gc.rs:394-401`
states in full for the identical call: the bound is the `MetadataStore` implementation's, not this
caller's (#508/#636); adding one would mean a production `tokio` dependency in a crate whose seam
boundary is `traits`/`core`/`tracing` (ADR-0010), and would bound one read of a pass built from
many. The path is fail-closed either way.

**(f) A seeded Tier-0 DST leg.** Not built, and none is owed — pre-declared by the brief's
*Verification posture* and true of this diff: every write this pass performs is on a **flat** record
resolved by borrow from the generation the scan returned (`crates/core/src/metadata.rs:2585` — a
flat snapshot reads nothing and can never be `Superseded`, `:2629`), committed under the base's own
unmodified version-conditional CAS; the segmented side performs a decline, which writes nothing. So
the patch adds no new concurrent or destructive path for the rubric's *test fidelity* clause to
attach to. If raised in review: record-reject citing `metadata.rs:2585`/`:2629` and carve-out #699.

## 4. Forced self-refutation (the three questions)

**(a) Genuine red?** **Yes.** Run through the project's own runner, not a hand-rolled command:
`PDCA_BUNDLE=…/results/issue_695 ./engine/scripts/run-verify.sh` (which applies `patch.diff` to a
clean `../wyrd-verify` worktree off `origin/main`, runs the green leg, then reverts *only*
`backfill.rs` and re-runs):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_backfill (fix applied)
test result: ok. 5 passed; 0 failed
run-verify.sh: RED — cargo test … (production reverted, test kept)
test result: FAILED. 0 passed; 5 failed
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

All five reds are **behavioural**, not compile errors — the file names no symbol this patch adds,
so it compiles against the base. The individual red messages:

* leg 1 `left: [] right: [0, 1, 2]` — one segmented object stopped the flat record being filled;
* leg 2 `unwrap() on an Err value: SegmentedMapUnsupported { operation: "backfill::reconcile" }`;
* leg 3 `contained: SegmentedMapUnsupported { … }` — the walk ended at the first damaged record;
* leg 4 `assertion left == right failed: one namespace reading  left: 2  right: 1`;
* leg 5 `wrong error: backfill::reconcile met a segmented chunk map, which this build cannot yet
  resolve` — the base fails, but with the *wrong* error, so the guard genuinely discriminates.

**(b) Production path?** **Yes.** Every leg calls `wyrd_custodian::backfill::reconcile` — the real
public entry, the same one `tests/backfill.rs` and `tests/backfill_telemetry.rs` drive — with a real
`BackfillContext`. The chunk maps are resolved by the real `wyrd_core::metadata::resolve_chunk_map`;
the audit strings are read off the real `tracing` events the production emitters publish; the
`Blocked` answer is the real `Reconciled` variant. The only doubles are the `MetadataStore` seam
implementations, which is what the trait seam exists for (and what `tests/gc.rs`,
`tests/segmented_map_consumers.rs`, `tests/segmented_map_restore.rs` all use). No copy,
re-implementation or mock of the unit under test.

**(c) Fixture includes the fault?** **Yes**, and it asserts its own faults are real:

* the damaged segmented object is a committed root naming two segments with only the first `seg:`
  record ever written, and `seed` re-reads the root, decodes it (proving the *root* is fine, so the
  fault is genuinely the resolve) and asserts `resolve_chunk_map(...).is_err()`
  (`tests/segmented_map_backfill.rs:190-199`);
* the undecodable record's bytes are asserted not to `decode` (`:154-158`);
* the store fault is injected into `get` and leg 5 asserts *that specific* fault text came back, so
  a chunk-map verdict cannot pass wearing its place (`:390-393`);
* nothing is curated out: the damaged records are `inode:1` / `inode:2` in a `BTreeMap`-backed
  store, so they sort **before** the healthy `inode:9` — "the healthy record was still filled" is a
  claim about a walk that had already met a blocker, not luck.

## 5. Addressing the carry-forward

**Iteration 6 (C5 surviving mutants; T5 "make each unreadable-object increment independently
observable").** Kept and strengthened. The three `incomplete += 1` sites (`backfill.rs:139`, `:169`,
`:221`) are each pinned alone: leg 3 runs each damaged class over **its own store** and asserts
`gauge.backfill_placement_incomplete == 1` there (`tests/…:320-328`), leg 2 pins the decline site at
1, and the combined store pins 2 — so no site can stop counting behind another's. Result:
`scripts/mutants-in-diff` → **17 mutants tested in 22s: 11 caught, 6 unviable, 0 missed.** T5 also
asked to compress the discriminator *below* the ceiling: 236/393 vs the previous 240/390, achieved
by removing a duplicated fixture (the ≤S `seg:`-read property now runs on leg 1's store, which
already holds S = 2 segmented objects) rather than by dropping coverage.

**Iteration 7 (T4 Contribution — the prior-art scan and the two harness scripts could not be
independently reproduced).** That row is about *evidence a reviewer can re-run*, so here is
everything needed to clear it at sign-off:

* The two commands are **PDCA-harness scripts, not target-repo scripts** — they live at
  `/home/eddie/wyrd/wyrd-pdca/scripts/review-branch` and `/home/eddie/wyrd/wyrd-pdca/scripts/pdca`
  (`ls /home/eddie/wyrd/wyrd-pdca/scripts/` shows both). They are absent from the *target* checkout
  by design: `pdca.toml:844` and `:866` invoke them with `cwd = <pdca root>`. Nothing in the target
  is expected to carry them.
* **Prior-art scan, re-run in the worktree just now** (commands verbatim):
  * `git log origin/main --oneline -- crates/custodian/src/backfill.rs` →
    `3e05891` (segmented chunk-map record shape — the commit that **created** the two defect sites),
    `68403eb` (ETag/Content-Type/Last-Modified, the ADR-0047 preservation this pass honours),
    `8b5365b` (log subscriber at role entry), `fddb448` (the original identity-placement backfill).
    Unchanged since; nothing else has touched the file.
  * `git log origin/main --oneline -- crates/custodian/tests/segmented_map_backfill.rs` → empty (new
    file, as the brief requires — so C4-verify earns its red from an **added** `*/tests/*.rs`).
  * `gh pr list --repo getwyrd/wyrd --state open` → **0 open PRs in the repo at all**, so none
    touches either path.
  * **Closed/rejected work by path:** `gh pr list --repo getwyrd/wyrd --state closed --search
    "backfill OR segmented"` → the one closed-without-merge PR is **#647** *"feat(core,custodian,
    server): segmented chunk maps beyond one value"* (CLOSED 2026-07-30), and `gh pr view 647 --json
    files` confirms it touched `crates/custodian/src/backfill.rs`, `tests/backfill.rs`,
    `tests/backfill_telemetry.rs`. Its approach and why this slice deliberately differs are in §3(b)
    above — that is the substance the T4 row was asking for.
  * Merged PRs by title: #402 (the original backfill) and #415 (the telemetry-leg split); both
    already in the merged-history list above.

## 6. Gate evidence (all run on this host, in `$PDCA_WORKTREE` unless noted)

| Check | Command | Result |
|---|---|---|
| C4-ci (whole tree) | `./engine/xtask.sh ci` | **exit 0 — "xtask ci: all checks passed"**, prose gates included (`typos` ran; `lint_docs: OK`; `render_site: wrote 98 page(s)`), so this is full CI parity, not a warn-and-skip green |
| C4-verify (red→green) | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **PASS — red without the fix (5 red), green with it (5 pass)** |
| C5 mutants | `PDCA_BUNDLE=… ./scripts/mutants-in-diff` | **17 tested: 11 caught, 6 unviable, 0 missed** |
| classification | `run-verify.sh --classify patch.diff` | `ADDED_TEST crates/custodian/tests/segmented_map_backfill.rs` + `CRATE crates/custodian` — matches the brief's Plan-time dry run |
| base | `run-verify.sh --print-base` | `origin/main` |
| formatter / commit hooks | `cargo fmt --check -p wyrd-custodian`, `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` | clean (also re-run inside `xtask ci`) |
| unmodified suites | `cargo test -p wyrd-custodian` | all green, including `tests/backfill.rs` (10) and `tests/backfill_telemetry.rs` (1) **unmodified** — the lost-CAS leg (`backfill.rs:278-325`) still answers `Satisfied`, and the drain-to-zero gauge still reads 0 |

No external dependency was missing: `typos`, the docs renderer, `cargo-mutants`, `cargo-deny` and
`cargo-machete` all ran (the last two inside `xtask ci`). **No NEEDS-HUMAN external dependency.**

### One flaky, unrelated test the human should know about before Check

`xtask ci` was run **three times** on this tree. Runs 1 and 3 passed end to end; run 2 failed on
one test in a crate this patch does not touch:

```
---- tests::a_bodyless_response_is_recorded_complete_not_aborted stdout ----
panicked at crates/gateway-s3/src/lib.rs:4259: … the row must say `complete`. … got:
test result: FAILED. 75 passed; 1 failed
```

It is **pre-existing and cannot be caused by this patch** — that is a dependency-graph fact, not a
guess: `cargo tree -p wyrd-gateway-s3 | grep -c wyrd-custodian` → **0** (and `grep wyrd-custodian
crates/gateway-s3/Cargo.toml` → nothing), so the only crate this diff changes is absent from that
test binary entirely, and each test binary is its own process. The failure itself is the empty-
capture shape of the #214 callsite-interest class: the test installs a *thread-scoped*
`tracing::dispatcher::set_default` inside a `#[tokio::test]` (`crates/gateway-s3/src/lib.rs:4067`)
and `capture.contents()` came back empty (`got: ` with nothing after it). Reproduction attempts on
this tree: the single test **6/6 green** in isolation, its whole lib suite **3/3 green**; only the
full `cargo test --workspace` run has ever shown it, once. Out of scope to fix here (different
crate, and the brief's file budget is two files) — flagged so that if `C4-ci` goes red at Check on
exactly this test, it is re-run rather than attributed to this bundle. Worth its own issue.

## 7. Notes for the human at sign-off

* Nothing outside the two budgeted files is touched — no `crates/dst/` hunk, no `Cargo.toml`, no
  docs edit (`docs/design/architecture/06-runtime-view.md` §6.2 `:29`/`:31` already states this
  containment rule fleet-wide), no sibling-child file (`rebalance.rs`, `reconstruction.rs`).
* `backfill::reconcile` is still a directly-callable public entry not yet wired into
  `reconcile_step` (module docs, `backfill.rs:40-49`), so the new `Blocked` answer reaches
  `least_certified` (`reconciliation.rs:51-61`) only once a later slice threads it through — no
  behaviour change for any other loop today. Grep confirms the only callers are the crate's own
  tests.
* Two review findings are pre-answered by the brief and should be **record-rejected**, not fixed, if
  they arrive: the malformed-inode-key/CAS-key question (frozen, #698) and a request for a seeded
  Tier-0 DST leg (#699). Both have tracker references and the frozen lines are verifiable by
  reading the patch.
