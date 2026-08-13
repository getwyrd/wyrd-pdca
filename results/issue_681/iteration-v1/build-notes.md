# Build notes — issue 681 / passes-read-through-resolver-contained

*(Withheld from the reviewer; written for the human at sign-off.)*

All `path:line` citations are against the **target branch** `getwyrd/wyrd @ main` as checked out
in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`, base `339da46`, the exact SHA the brief
verified against). Post-patch line numbers are given for new code; pre-patch numbers for the
sites removed.

---

## 1. What was built

The three passes that walk `inode:` themselves now (a) resolve every committed object through
the **shared** resolver, (b) contain per object what they cannot read, and (c) **refuse** —
writing nothing — where the write belongs to #682. All seven fail-closed sites the brief
tabulated are gone; none was replaced by a skip.

| Brief's site (pre-patch) | What replaced it |
|---|---|
| `reconstruction.rs:332` (`assess`) | `assess` no longer reads the record at all — it classifies against the pass's ONE reading (`reconstruction.rs:424-444` post-patch: `Site::Flat` / `Site::Refused` / `Drain`-only-if-complete) |
| `reconstruction.rs:583` (`repair_chunk`) | the plan carries `prior_chunks: Arc<[ChunkRef]>` proven flat when the site was located, so the write path has **no segmented case left to answer for** (`reconstruction.rs:124`, `:693`) |
| `reconstruction.rs:636` (`find_chunk`) | `find_chunk` deleted; `locate_queued_chunks` (`reconstruction.rs:795-902`) is one scan + one resolve per object per pass |
| `backfill.rs:99` (`reconcile`) | decode-contain + `resolve_chunk_map` + downcast rule (`backfill.rs:96-130`), then a per-object `writable` decision (`:159-175`) |
| `backfill.rs:181` (`emit_remaining`) | the second, resolving walk is **gone**: `remaining` is accumulated in the one walk (`backfill.rs:90`, `:170`, `:216`, `:222`) and `emit_remaining` is now a pure emitter (`:255`) |
| `rebalance.rs:162` (`plan_evacuations`) | same containment shape (`rebalance.rs:192-226`) + per-chunk refusal (`:273-284`) |
| `rebalance.rs:259` (`evacuate_chunk`) | `EvacPlan.prior_chunks` (`rebalance.rs:94`, `:377`, `:389`) — same "no segmented case at the write path" trick |

Certification: each pass counts the work it could not do (`refused`) and answers
`Reconciled::Blocked` when it is non-zero — `reconstruction.rs:327-334`, `backfill.rs:224-229`,
`rebalance.rs:141-148`. `Blocked` outranks `Changed` via the existing `least_certified`
(`reconciliation.rs:55-61`), so `reconcile_step` reports the least-certified loop unchanged.

Peer callsites mirrored (the brief's "Do MAY open" list, and nothing beyond it):
`gc.rs:360-455` (`referenced_fragments` — decode-contain, resolve, the `Ok(ChunkMapError)`
downcast rule, `Ok(None)` skip), `gc.rs:155-165` (attribution emitted by the CONSUMER, per
object, before the rest of the pass), `gc.rs:470` (`object_name`'s escaping shape),
`restore.rs:621-692` (the same shape at per-object granularity), `reconciliation.rs:44,55`
(`Blocked` / `least_certified`), `reconstruction.rs:186-232` (the classification set and the
"never-repaired conditions stay OFF the repairable-backlog gauge" rule — every one of
`Repairable`/`Drain`/`Unreachable`/`Blocked`/`Unrepairable`/`Malformed` is preserved verbatim,
and the new `Refused` is likewise off that gauge),
`tests/segmented_map_restore.rs:387-431` + `tests/segmented_map_consumers.rs:80-120` (the
`seed_segmented`/`seed_damaged` shape and the `BTreeMap`-backed store).

## 2. The two design decisions worth the human's attention

### 2.1 The write path was made *total* rather than given a defensive branch

The obvious way to remove `repair_chunk:583` / `evacuate_chunk:259` is
`as_flat().ok_or(...)? → return Ok(Outcome::Refused)`. That leaves a branch that is
**unreachable by construction** (a plan is only built over a map already proven flat) — dead
code a reviewer must then reason about, in the one place where "dead" and "silently wrong" are
hard to tell apart.

Instead the plan carries the flat chunk list it was built from (`Arc<[ChunkRef]>`), so
`as_flat()` does not appear in either write path at all. Cost check, not an adjective:

* rejected shape: +4 lines per file, +1 enum variant per file, +1 emitter per file (≈ 20 lines),
  **plus** two permanently-unreachable arms;
* chosen shape: +1 struct field per plan (2 lines), `prior_chunks.to_vec()` in place of
  `prior.chunk_map.as_flat().ok_or(...)?.to_vec()` (net **−4** lines), zero unreachable arms.

It is also strictly *less* memory than the base: `rebalance.rs:206` (pre-patch) used to clone the
whole `InodeRecord` **per plan** (`prior: record.clone()` inside the per-chunk loop — O(chunks²)
on a large object); both files now share one `Arc<InodeRecord>` + one `Arc<[ChunkRef]>` per
object, materialized lazily and only for an object that actually yields a plan
(`rebalance.rs:240-243`, `:298-303`; `reconstruction.rs:880-881`).

The invariant it rests on: for a non-segmented generation, `resolve_chunk_map` returns
`chunks = Cow::Borrowed(record.chunk_map`'s flat slice`)` (`core/src/metadata.rs:2585`), and the
restart arm returns the live root together with *its* chunk list
(`core/src/metadata.rs:2666-2672`). So `resolved.chunks` **is** `resolved.record`'s own list
whenever the record is flat — which is exactly the condition under which a plan is built.

### 2.2 A drain is now conditional on the reading being complete

`assess`'s `None` arm (base `reconstruction.rs:322-325`) read "no committed map references this
chunk" as `Drain`, and the drain deletes the obligation (`:271-277`). Merely *skipping* an
unreadable object would have routed straight into it — the silent data-loss the brief's
consequence (2) names. `assess` now drains only when `index.unreadable == 0`
(`reconstruction.rs:430-435`); otherwise the obligation is `Refused(REFUSED_INCOMPLETE)`, kept,
and the pass does not certify (`reconstruction.rs:436-443`).

## 3. Things I deliberately did **not** do

* **No shared maintenance walk across passes.** Each pass still reads `inode:` itself. The
  brief's leg (5) scopes the count to the reconstruction pass alone and says sharing one walk is
  "a much larger refactor that is not in scope". **Human note:** the deferral marker at
  `restore.rs:616` says "the maintenance walk that both would share is that slice's" — that
  sentence now over-promises, since #681 closes without building it. I did not edit `restore.rs`
  (the brief forbids touching it and it is not among the eight budgeted files); re-pointing that
  marker at a follow-up is a one-line change someone should make in #682 or a housekeeping issue.
* **No `crates/custodian/src/resolve.rs`.** PR #647's custodian-local resolver is superseded by
  `metadata::resolve_chunk_map` (#649); the brief says so explicitly.
* **No write to any segmented record**, no `repoint_chunk`, no record ceilings (#682).
* **No edit to `docs/design/architecture/06-runtime-view.md`.** The docs-currency task was
  confirm-only. The sentence at `:29` — *"a consumer that has not yet adopted it refuses a
  segmented map outright"* — still reads true after this slice: the remaining refusers are
  `core/src/metadata.rs:1480` (`commit_chunk_map`), `core/src/read.rs:96`, and
  `core/src/metadata.rs:1872` (`high_water_marks`) (verified by grepping
  `SegmentedMapUnsupported` across the tree post-patch). The paragraph at `:31` — "a maintenance
  pass resolves **every** committed object this way … which every pass reading it must then say
  out loud" — becomes *more* true, not less. Nothing was claimed that the passes cannot evidence.
* **No additions to `tests/{reconstruction,backfill,rebalance}.rs`.** The brief allocates them
  for "positive matches on any variant or field this patch introduces" — this patch introduces
  **no public** variant or field (every new type is private to its module; the outcome is the
  base's own `Reconciled::Blocked`), so there is nothing that needs a positive match outside the
  discriminator, and the discriminator asserts `Blocked` positively already. Their existing 40
  tests all still pass unchanged, which is the regression evidence they were listed for.
* **No DST leg.** The brief's falsifiability section says none is required. `cargo xtask ci`
  runs `run_dst()` (`xtask/src/main.rs:1567`) and the existing seeded custodian DST properties
  (`crates/dst/tests/custodian.rs`) pass unmodified.

## 4. Budget — over the stated line cap, honestly reported

| File | added semantic lines (non-blank, non-comment) |
|---|---|
| `crates/custodian/src/reconstruction.rs` | 189 |
| `crates/custodian/src/rebalance.rs` | 88 |
| `crates/custodian/src/backfill.rs` | 77 |
| **production subtotal** | **354** |
| `crates/custodian/tests/segmented_map_passes.rs` (new) | 720 |
| **total** | **1074** (cap: 900) |

**Files: 4 of the 8 budgeted** (no ninth file; the docs headroom went unused).

The overrun is entirely the discriminator, and I did trim it (folded six
`assert_names_unreadable_record` calls into one `assert_names_unreadable_records` helper, −16
lines). What remains is five binding legs over a fixture that needs an in-memory
`MetadataStore` **with read counters and an injectable `get` fault**, a `ChunkStore`, a JSON
audit capture, raw `seg:`-record seeding, and real erasure-coded fragment bytes. For calibration,
the two files this one completes the family with: `tests/segmented_map_restore.rs` is 700 lines
for 5 legs and `tests/segmented_map_consumers.rs` is 1341 lines — this one is 1002 total lines,
squarely inside that band. I judged that trimming a further ~170 lines could only come out of
assertion *messages* (the multi-line explanatory strings), which is the part of a test a
reviewer actually reads. Flagging rather than silently absorbing it: if the human wants the cap
honoured literally, the cut I would make is leg (3)'s evacuation half (~25 lines) — and I would
argue against it, see §5(c).

## 5. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — proven mechanically, not by assertion. `PDCA_BUNDLE=results/issue_681
./engine/scripts/run-verify.sh` (the project's own per-fix runner; it applies `patch.diff` to a
clean `origin/main` worktree, then reverts the three production files and keeps the test) ends:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_passes (fix applied)
  test result: ok. 5 passed
run-verify.sh: RED — (production reverted, test kept)
  test result: FAILED. 0 passed; 5 failed
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

Every one of the five reds is an **assertion/expect panic**, not a compile error — the test names
no symbol this patch introduces (checked: it imports only base-visible items, and
`Reconciled::Blocked` predates this slice at `reconciliation.rs:44`). The base failure reasons,
one per leg, each *behavioural*:

1. `Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`
2. `Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })`
3. `Store(Error("expected ident", line: 1, column: 2))` — the undecodable record ending the walk
4. the store fault came back as a *chunk-map verdict* instead of the injected fault
5. `assertion left: 3, right: 1` — the base really does scan the namespace **once per
   obligation** (Q = 3). Note this leg's base red is *not* an error: `find_chunk` returns before
   it reaches the segmented objects (they sort last), so the base repairs correctly and fails
   **only** on the complexity count. That is the leg working as intended.

**(b) Production path?** Yes. The test drives `wyrd_custodian::reconcile_step` — the real fenced
control point, elected through the real `Custodian`/`FencedZone` over `MemCoordination` — and
`wyrd_custodian::backfill::reconcile`, the real public entry. Resolution goes through the real
`wyrd_core::metadata::resolve_chunk_map`; the survivor bytes are built with the real
`wyrd_core::erasure::encode` + `wyrd_core::write::encode_ec_fragment` and are verified by the
production `repair::intact_shard` (a fixture that faked them would not reconstruct). The only
doubles are the two *store seams* (`MetadataStore`/`ChunkStore`), which is what every sibling
custodian suite uses and what ADR-0010 makes the loops' contract. No copy, no mock of the
behaviour under test, no re-implementation.

**(c) Fixture includes the fault?** Yes, and the fixture asserts the faults are real rather than
assuming them:
* `seed_damaged` asserts `resolve_chunk_map(...).is_err()` before any leg runs
  (`tests/segmented_map_passes.rs:442-447`) — copied from the #651 precedent at
  `segmented_map_restore.rs:425-430`;
* `seed_undecodable` asserts `decode::<InodeRecord>(...).is_err()` (`:462-465`);
* both damaged records sit in the **same store** as the healthy ones and, because the double is
  `BTreeMap`-backed and they are `inode:0` / `inode:1`, they are met **first** — nothing is
  curated out. The healthy objects that must still be repaired / filled / evacuated are
  `inode:2` / `inode:3` / `inode:4`, i.e. strictly after the blockers;
* leg (4)'s store fault is genuinely injected into the double's `get` (`:113-119`) and the
  assertion pins the injected message, so a differently-shaped failure does not score as a pass;
* leg (2) additionally asserts the **whole store snapshot is byte-identical** after the pass
  (`:678-683`), so "wrote nothing" is not taken on trust.

One refutation I checked and want recorded: could the green legs pass with segmented objects
**silently skipped** (the quieter failure the brief warns about)? No — leg (2) enqueues a repair
for a chunk that lives *inside* a `seg:` record. A pass that skipped segmented objects would find
no site for it and, the reading being otherwise complete, would **drain** it; leg (2) asserts it
is still queued. So the segmented map is provably resolved, not stepped over. (Leg (5)'s `≤ N`
seg-read bound deliberately has no *lower* bound: an implementation that stopped resolving once
all Q chunks were located would be a legitimate variant, and leg (2) already binds the
"really resolved" property.)

## 6. Gates run locally

* `PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh` → **PASS** (red→green, above).
* `./engine/xtask.sh ci` (the gating whole-tree gate: typos → docs render → gitlink/unsafe
  guards → fmt --check → clippy -D warnings → build --all-targets → test --workspace →
  cargo-machete → cargo-deny → conformance → statics → orchestrator guard → **DST sweep**) →
  `xtask ci: all checks passed`. The prose gates really ran on this host (log lines `$ typos`
  and `render_site: link audit OK`), so this is CI parity, not a warn-skip.
* `cargo test -p wyrd-custodian` → 94 passed / 0 failed (the 5 new + 89 pre-existing, none
  modified).
* Formatter/commit-hook readiness: `cargo fmt --all -- --check` clean, `cargo clippy -p
  wyrd-custodian --all-targets` clean under the workspace `-D warnings` lint policy.

## 7. Behaviour changes a reviewer should look at twice

1. **A pass can now answer `Blocked` where it used to answer `Err` or `Satisfied`.** Consumers:
   only `reconcile_step`'s `least_certified` fold, which already handles `Blocked` (#650). No
   non-test consumer of `Reconciled` exists outside `crates/custodian` (grepped).
2. **Reconstruction skips the namespace read entirely when the queue is empty**
   (`reconstruction.rs:164-175`). Without this guard every idle pass would pay one resolve per
   object — the "bounded work" constraint — for an answer no obligation is waiting on. The
   trade-off is that an idle reconstruction pass does not name an unreadable record; GC and
   scrub walk the same namespace on their own cadence and do name it (`gc.rs:164-166`), so no
   operator signal is lost. The three level gauges are still emitted at 0 on an idle pass.
3. **Backfill's remaining-placement gauge changed how it is computed, not what it means.** It is
   now accumulated during the fill walk (empty placements that were filled contribute 0; a
   CAS-lost fill and a declined segmented record still contribute) instead of by a second scan.
   This is what removes the "second resolving walk" the brief forbids. The existing
   `backfill_telemetry.rs` drain-to-zero test and the CAS-race test both still pass unchanged,
   which is the evidence the meaning is preserved.
4. **Two new refusal reasons are charged that the brief did not enumerate**, both instances of
   the same invariant ("an obligation is discharged or kept; never discarded for want of a
   reading"): `REFUSED_UNADDRESSABLE` (a committed record under an `inode:` key that will not
   parse back to an inode id — on the base such a record's chunk fell through `find_chunk` to
   `None` → `Drain`, a latent obligation-loss of exactly the kind consequence (2) describes) and
   `REFUSED_AMBIGUOUS` (the brief's narrow duplicate-chunk-id rule: neither reference repaired,
   both objects named, `reconstruction.rs:904-918`). Neither builds a claim-counting apparatus,
   no new report schema, no verdict surface — as the brief's carried-forward constraint requires.
5. **New audit/metric names** (all `warn`-level, all on the existing per-pass audit targets):
   `reconstruction_unresolvable_records`, `reconstruction_repair_refused`,
   `backfill_unresolvable_records`, `backfill_declined_records`,
   `rebalance_unresolvable_records`, `rebalance_evacuation_refused`. They mirror #650's
   `gc_unresolvable_records` (`gc.rs:563`), which is likewise undocumented outside its emitter —
   so no metrics catalogue needed updating (grepped: no `.md` mentions `gc_unresolvable_records`).

## 8. Scratch discipline

Two log files under `$PDCA_SCRATCH` (`pdca-builder-681-ci.log`, `pdca-builder-681-ci-final.log`);
no throwaway checkouts or build dirs were created outside the driver's own worktrees. Nothing was
written to `/tmp`. Both logs are removed at the end of this beat.

## 9. STOP discipline

No branch pushed, no PR opened, nothing marked ready. The patch is staged in `$PDCA_WORKTREE`
and captured as `patch.diff`.
