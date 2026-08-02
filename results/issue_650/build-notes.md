# Build notes — issue 650 / gc-scrub-through-resolver-fail-closed-containment

**Iteration 5.** Base `4e78aeb` (= `origin/pdca-integration/main`, carrying #648 and #649);
patch cut against it in `$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt`), which
the driver had reset to that base before this round. Every `path:line` below is on the
**patched** worktree unless it says "base".

## 1. The carry-forward, answered

Round 4's Check was **0 blocking review findings** and every gating gate green
(`iteration-v4/check-gates.md`: C4-ci pass, T4-batch-review pass, T4-contribution pass;
C4-verify PASS; C5-mutants pass). The auto-iterate fired on the single remaining
implementation-tagged cell:

| Carry-forward item | Answer in this round |
|---|---|
| **T4 Contribution — "affected-path GitHub history found #647 as the only functionally relevant unmerged prior art (#336 is unrelated), but `scripts/review-branch`, `scripts/pdca`, and their outputs are absent, so the reported three-agent review and contribution-check passes remain provisional."** | **Not a defect in the patch, and not fixable from Do** — those are PDCA-side tools in `wyrd-pdca/scripts/`, absent from the reviewer leaf's artifact-only sandbox. What I *can* do is make the disposition re-runnable in one line each; see §7, which lists the exact commands and re-runs the affected-path history myself (output below). Sign-off item. |
| 3 findings deferred to sign-off (`deferred-findings.json`) | Two are the C3 scope pair that iteration 4 already answered by **removing** `ReconciliationStatus::PendingUnresolvable` and the `tests/rebalance.rs` leg (still removed here; `review-rejected.md` §(v)). The third is C1 landing order — inherent to a stacked slice, and the driver's `wave_mode = "stack"` is what holds it; nothing in the patch can clear it. |

Affected-path history re-run this round (reproducible, no PDCA tooling needed):

```
gh pr list --repo getwyrd/wyrd --state all --limit 200 --json number,title,state,files \
  --jq '[.[] | select(.files != null)
        | select([.files[].path] | any(test("crates/custodian/src/(gc|scrub|reconciliation)\\.rs")))
        | {number,state,title}] | .[]'
→ 675 OPEN  (#649's PR — the resolver this slice consumes)
  672 OPEN  (#648's PR — the record shape)
  647 CLOSED "segmented chunk maps beyond one value"   ← the salvage source, the sole
                                                          functionally relevant unmerged prior art
  + 16 MERGED, none about chunk-map resolution in the reference build
    (564, 559, 555, 531, 489, 448, 397, 362, 361, 340, 247, 193, 190, 189, 188, 187)
```

## 2. What changed against iteration 4's patch

Iteration 4's design passed every rubric pass, so re-deriving it from scratch would have
traded a reviewed artifact for an unreviewed one. I kept it and did two things: re-established
it on the reset worktree and re-ran every gate on the **final** tree (§5), and closed the one
substantive hole I found in my own adversarial pass over it —

**A third consumer of the shared reference set was undocumented.**
`restore::reconcile_after_restore` reads the same `gc::referenced_fragments`
(`crates/custodian/src/restore.rs:200`) and gates on the same `protects` (`:239`). The brief's
second invariant is "every pass reading one set gives the same answer about it", so "restore
now silently succeeds over an incomplete set" is exactly the finding this patch should be able
to answer — and iteration 4 answered it nowhere. I checked whether it is *true* before writing
anything:

* the **mark** half withholds every fragment while the set is incomplete (the shared
  `ReferenceSet::protects`, `gc.rs:331`), so nothing an unreadable object might own is marked
  for GC; and
* the **report** half re-reads the same records through `committed_chunks`, called
  unconditionally at `restore.rs:326`, whose `metadata::decode(&value)?` (`:393`) still
  propagates — so an unreadable committed record still ends that pass with an `Err` and **no
  `RestoreReport` is ever returned over a set with a known hole in it**.

Verified, not assumed: a throwaway probe appended to `crates/custodian/tests/restore_reconcile.rs`
on the patched tree seeded an undecodable committed `inode:1` and asserted the pass errors —
`reconcile_after_restore -> Err(key must be a string at line 1 column 2)`, test passed. The
probe was then reverted (`git checkout`), because a restore leg belongs to #651's own added
file and the brief forbids pulling those forward. So the patch adds **17 comment lines and 0
semantic lines** at `restore.rs:183-199`, stating that fact and carrying an in-code
`// deferred: #651` marker (`:196`) for the *attributed* restore answer — the shape AGENTS.md's
reviewer protocol treats as settled. `review-rejected.md` §(vii) records it at all three lines.

Nothing else moved. Diff vs `iteration-v4/patch.diff`: `crates/custodian/src/restore.rs` only.

## 3. The change — 12 files, 1,284 semantic added lines (129 production, 1,155 tests/docs/lock)

Budget: ≤ ~1,500 semantic lines, ≤ 15 files. The `Reconciled` third variant needed **no**
mechanical `match`-arm migration: every consumer outside `reconciliation.rs` compares with `==`
or forwards the value (`crates/server/src/custodian.rs:341` forwards it untouched; its callers
match `Ok(_)`), so the brief's "22 files across 4 crates" allowance went unused — verified by
`cargo clippy --workspace --all-targets` inside `xtask ci`.

* **`crates/custodian/src/gc.rs`** (+64 semantic) — the reference build resolves every committed
  record through the one shared resolver (`metadata::resolve_chunk_map`, `:402`), so a
  **segmented** object's chunks are in the protected set at all; `ReferenceSet::unresolvable`
  (`:294`, keyed by the record's raw bytes) carries what it could not read;
  `protection`/`protects` (`:306`, `:331`) say *why* a fragment is withheld and withhold
  everything while the set is incomplete; `reconcile` answers `Reconciled::Blocked` there
  (`:234-241`) and attributes each blocker **before** the fleet walk (`:164-166`,
  `emit_unresolvable` at `:563`). One damaged object does not end the walk: a record that will
  not decode (`:378-384`) or a generation the resolver types as a chunk-map anomaly
  (`:405-411`) is contained; a store fault under the read still propagates (`:414-416`).
  `object_name` (`:470`) is the injective operator name.
* **`crates/custodian/src/scrub.rs`** (+16) — the same set, its own attribution (`:114-116`,
  `emit_unscrubbable` at `:229`), and the **same answer for the same condition** (`:210`).
* **`crates/custodian/src/reconciliation.rs`** (+30) — `Reconciled::Blocked` (`:44`) and
  `least_certified` (`:55-61`), folded through every loop in `reconcile_step`
  (`:118`, `:125`, `:132`, `:139`), so a step never claims more than its weakest loop.
* **`crates/custodian/src/desired_state.rs`** (+3) — the non-regression guard (`:188-190`) and
  the deferral marker (`:183-187`); the `Pending` doc gains the second case it now covers
  (`:85-90`). No public API change. Why it cannot be zero lines: §4(b).
* **`crates/custodian/src/restore.rs`** (+0 semantic) — §2.
* **`crates/core/src/metadata.rs`** (+16) — kept from iteration 2's C5 fix:
  `ChunkMapError::RootRecordUndecodable` (`:574`) and `decode_root_record` (`:2404`), used at
  **both** root re-reads (`:2243`, `:2571`), so a root rewritten unreadable *under* a resolve is
  still one object's fault and not an unattributable store outage.
* **Tests** — the added discriminator `crates/custodian/tests/segmented_map_consumers.rs`
  (8 legs, 941 semantic lines), the positive `Reconciled::Blocked` matches it cannot carry
  (`crates/custodian/tests/gc.rs:860-864`, `crates/custodian/tests/scrub.rs:1069-1073`), and the
  seeded Tier-0 DST property `crates/dst/tests/custodian.rs:1532-1715` (three arms drawn from
  the run seed: readable, retired-mid-resolve, incomplete). The `tests/gc.rs` leg is #648's own
  test **rewritten in place** (`:788-877`), not deleted: #648 asserted the placeholder contract
  its doc comment named ("no resolver reads `seg:` records until #649", so the pass aborts on
  the shape); with the resolver wired in, the same fixture now asserts the superseding contract
  — the pass completes, certifies nothing, and reclaims nothing fleet-wide.
  `ChunkMapError::SegmentedMapUnsupported` stays live: `restore::committed_chunks`
  (`crates/custodian/src/restore.rs:404`) still raises it, which is #651's to route.
* **`docs/design/architecture/06-runtime-view.md:29-31`** — the containment sentences the brief
  names, and only those; the report-only clause states what this slice actually lands (never
  "satisfied" over an incomplete set) rather than the attributed answer #651 will add.
* **`Cargo.lock`** (+2) — `event-listener` 5.4.1 → 5.4.2. Not cosmetic: with the lockfile
  reverted, `cargo deny check advisories` prints `advisories FAILED` for RUSTSEC-2026-0221
  (reproduced in iteration 4, which is the round it went red on), and `cargo deny` is inside the
  gating `cargo xtask ci`.

## 4. What I ruled out, with the cost

**(a) Re-deriving the patch from the salvage rather than keeping iteration 4's.** Cost: the
same ~2,100-line diff written a second time, losing four rounds of review-driven fixes
(iteration 2's typed root decode, iteration 3's audit-attribution assertions, iteration 4's
order-B seeding fix and the `PendingUnresolvable` removal) — each of which was a *finding*,
not a preference. The carry-forward names no defect in the design; "do not re-submit the
rejected approach unchanged" binds an approach that was rejected, and this one was not. What I
did instead is re-attack it myself (§2) and re-run every gate on the final tree.

**(b) Touching `desired_state.rs` not at all — rejected, because it is not free.** On the base
tree, an unreadable committed record makes that query **fail**: `referenced_fragments`
propagates the decode error (base `crates/custodian/src/gc.rs:256`) or refuses the segmented
shape (base `:266-267`), and `reconciliation_status` awaits it with `?` (base
`crates/custodian/src/desired_state.rs:157`). Routing the build through the resolver *without*
the guard would silently turn that `Err` into `Ok(Satisfied)` — "this server is safe to
decommission" computed from a reference set the system knows is partial, a permanent,
data-losing outcome from a report (C-1, `docs/principles.md` §5). So "0 lines" would have been
a **C-1 regression introduced by this patch**, not minimalism. Verified in iteration 4 with the
guard disabled (`if false && !referenced.unresolvable.is_empty()`): two fixture legs go red
(`segmented_map_consumers.rs:721`, `:1094`).

**(c) Keeping iteration 3's `ReconciliationStatus::PendingUnresolvable` — rejected on the
brief's own scope line.** Measured on iteration 3's patch: `desired_state.rs` +33 raw / +11
semantic (a new **public** enum variant with a `Vec<String>` payload, plus its return site) and
`crates/custodian/tests/rebalance.rs` +46 raw, in two files the brief lists under *Out of
scope*, on a public status API #651 must then re-cut. Two review passes called it blocking
scope. The replacement costs 3 semantic lines and no API change.

**(d) Giving `restore` its own "cannot certify" answer here — rejected, and it would also be
wrong.** Cost sketch: a `RestoreReport` field (public struct, `restore.rs:104-140`) + its
population + `is_clean()` + the emit + a leg in `tests/restore_reconcile.rs` ≈ 40 raw lines in
a file the brief names *Out of scope* — and it would pre-empt the shape #651 must choose for
the whole restore/evacuation walk. It is also unnecessary for safety *today*: the pass cannot
return a report over an incomplete set at all (§2, probe-verified). Comment + deferral marker:
17 raw, 0 semantic.

**(e) A caller-side timeout around the resolver await — standing rejection (i), and the cost is
a dependency, not lines.** `wyrd-custodian` has **no production `tokio` dependency**
(`crates/custodian/Cargo.toml`: `tokio` is under `[dev-dependencies]`), and the crate's declared
boundary is `traits` / `core` / `tracing` (ADR-0010, `crates/custodian/src/gc.rs:27-28`). A
`tokio::time::timeout` at `gc.rs:402` therefore buys a bound on **one** of the pass's many
awaits by adding a production runtime dependency to a seam-only crate — while
`meta.scan(b"inode:")` at `:365`, and every await in the other three custodian loops, stay
unbounded because the store implementation owns the bound (#508/#636, 3×). The rubric's clause
is "bounded (timeout, **fail-closed**)"; this await is fail-closed by construction.

**(f) Narrowing `protects` to the unreadable object's own chunks — settled rejection (iv), not
re-litigated.** Impossible in principle: an unreadable map's chunk ids are exactly what it
withholds, so no fragment in the fleet can be shown not to be one of them.

## 5. Refutation — the three questions, answered with evidence

**(a) Genuine red?** **Yes**, through the project's own runner, not a hand-rolled command.
`./engine/scripts/run-verify.sh` (the `C4-verify` gate, run with
`PDCA_VERIFY_BASE=origin/pdca-integration/main` exactly as the driver sets it for a wave>0
bundle) applies this bundle's `patch.diff` to a clean `../wyrd-verify` worktree off that base,
then reverts the production files, keeps the added test, and re-runs the target:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
test result: ok. 8 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-custodian --test segmented_map_consumers (production reverted, test kept)
test result: FAILED. 0 passed; 8 failed
run-verify.sh: PASS — red without the fix, green with it.
```

All 8 legs fail by **assertion/`expect`**, not compile error — the red-leg panics read
`... : Store(SegmentedMapUnsupported { operation: "gc::referenced_fragments" })` — because the
file names only base-visible symbols: `Reconciled::Blocked` is asserted as
`!= Satisfied && != Changed`, and the positive match ships in `tests/{gc,scrub}.rs`, which
`C4-ci` runs. `--classify` confirms the single discriminator
`ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`.

Beyond the whole-file red, the individual mutations that refute the two most-argued legs were
run in iteration 4 and are unchanged by this round's comment-only delta: reordering
`least_certified` (`reconciliation.rs:57-58`) so `Changed` outranks `Blocked` fails order A at
`segmented_map_consumers.rs:1239` (`got Changed`) and, with that assertion disabled, order B at
`:1310` (`got Changed`); disabling the `desired_state` guard reds `:721` and `:1094`. The
mechanical counterpart re-run this round: `scripts/mutants-in-diff` →
`30 mutants tested in 31s: 6 caught, 24 unviable` — **no survivors** on this diff.

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point (`crates/custodian/src/reconciliation.rs:104`) — over a real
`FencedZone`/`Custodian` from `wyrd_coordination_mem`, and the reference set is built by the
production `gc::referenced_fragments` calling the production
`wyrd_core::metadata::resolve_chunk_map`. Nothing is re-implemented: the only test code is the
two in-memory `MetadataStore`/`ChunkStore` doubles (the ADR-0010 seams these loops are
*specified* over) and the raw-record seeding, which writes exactly the bytes `metadata::encode`
/ `metadata::seg_key` put in a store, built through the real validating constructors
(`segmented_map_consumers.rs:419-460`) so a fixture typo cannot silently change which rule is
exercised. The drain answer comes from the production `desired_state::reconciliation_status`;
the restore claim in §2 was probed against the production `reconcile_after_restore`. The DST
leg drives the same entry point under madsim with the arm drawn from the run seed.

**(c) Fixture includes the fault?** **Yes**, and each fault is asserted to *be* a fault before
it is relied on. `seed_damaged` (`segmented_map_consumers.rs:466-479`) asserts the seeded root
genuinely fails to resolve; the structural leg asserts its root genuinely fails to decode
(`:832-835`); the race leg asserts its scripted rewrites were actually **spent** (`:996-1001`),
so a race that never happened cannot pass. The store the pass walks always contains the damaged
record — `inode:1` sorts **before** the healthy `inode:2` and the double is a `BTreeMap`, so the
build meets the blocker **first**, and the continuation legs would fail rather than skip if the
walk abandoned there. Nothing is curated out: the containment legs' fleet holds the damaged
object's own fragment *and* an unrelated, plainly collectable expired-lease/orphan fragment, and
both must survive; criterion (1)'s fleet carries lapsed orphan grace records on **every**
segmented fragment, so "still on disk" is load-bearing rather than incidental (without them a
build that never resolved the object would pass too). In the DST property every fragment of the
generation live *while the pass runs* carries a lapsed grace record, on all three arms.

## 6. Gates run here (the same commands the Check gates use), on the FINAL tree

* `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** — `typos`, `lint_docs`,
  `render_site --check` (98 pages, link audit OK), gitlink/unsafe guards, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets` (`-D warnings`), build, workspace tests incl. the DST
  sweep, all three `cargo deny` invocations, conformance vectors, and the ADR-0035 statics gate.
  The prose gates **ran** rather than warn-skipping: `typos` and the docs renderer are both
  installed here.
* `./engine/scripts/run-verify.sh` → **PASS** (red→green, §5(a)).
* `scripts/mutants-in-diff` → `30 mutants tested in 31s: 6 caught, 24 unviable` — no survivors.
* `cargo fmt --all` was run over every touched file before the patch was cut, so the target's
  own commit hooks have nothing to reformat.

`patch.diff` in this bundle is byte-identical to `git diff` on the tree all four ran against.

## 7. For the human at sign-off

1. **The T4 Contribution cell (the carry-forward).** The reviewer cannot run the two PDCA-side
   tools from its sandbox. From `wyrd-pdca/` you can, and each is one line:
   `scripts/review-branch --bundle` (the 3× codex rubric review; round 4: 0 blocking) and
   `scripts/pdca contribcheck` (contribution artifacts). The affected-path prior-art query is
   re-run and quoted in §1 — no PDCA tooling needed for that one, just `gh`.
2. **Landing order (C1, deferred).** This slice consumes #649's resolver at `gc.rs:402`; PRs
   #672/#675 (#648/#649) are still open. It must stay stacked until they land — which is what
   `wave_mode = "stack"` does; nothing in the patch can assert it.
3. **The policy call (Validation, deferred).** One unreadable committed record pauses
   **fleet-wide reclamation** (`gc.rs:311` via `protects`) while healthy objects keep being
   verified (`scrub.rs:118`). That is the brief's invariant as written ("a partial reference set
   authorizes nothing and certifies nothing"), and it is a *leak-until-repaired* trade against
   the reclaim-live-bytes failure. It is a maintainer's fitness judgment, not a mechanical one.
4. **Scope judgment.** I read the brief's *Out of scope* line as binding for `desired_state` and
   `restore`: both get only the **non-certifying / non-regression** treatment here, with in-code
   `// deferred: #651` markers (`desired_state.rs:183`, `restore.rs:196`), and their *attributed*
   answers land in #651. If you would rather have the drain attribution now, iteration 3's
   `PendingUnresolvable` is preserved verbatim in `iteration-v3/patch.diff`.

## 8. External dependencies

None missing. The brief named `typos` and `docs-renderer`; both are present here
(`typos` ran as the first `xtask ci` step; `render_site.py` wrote 98 pages), so no prose gate
warn-skipped. No Docker, no protoc, no live backend, no new dev-dependency.
**No NEEDS-HUMAN external dependency for this bundle.**
