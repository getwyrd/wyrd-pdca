# Build notes — #814 (663.2): reconstruction rebuilds a staged chunk under the session fence

Base: the cycle worktree is detached at `feb1e30` (`origin/main` with #813 merged as PR #824). Every
"base line" below is a line of that commit; "new" lines are in the patched tree.

## What changed

**`crates/custodian/src/reconstruction/staged.rs` (new, 812 lines)** — the staged re-place, as the
brief's Scope lists it:

- `read` (new :199) takes, from the ONE staged walk, the committed part that first names each owed
  chunk, with the session's and the part record's exact bytes (first reference in key order, as
  `read_committed` does for committed maps).
- `assess` (new :238): session not `Open` → kept (`session-not-open`); session undecodable →
  `Withheld` (NEEDS-HUMAN); `EcScheme::None` → `Unrepairable` (by design, as the committed path);
  otherwise the shared `gather` + `settle` (full redundancy → `Drain`, below k → `Unreachable` /
  `Unrepairable`); then the vacated marks are read (undecodable → `Withheld`, never overwritten, C(vi));
  then `choose_destinations` (none → `Blocked`); then the repointed part bytes.
- `choose_destinations` (new :374), `assign`/`augment` (new :436/:464, Kuhn's matching of missing
  fragments to failure domains), `candidate` (new :497: in the fleet, in the topology, and NO
  `desired:dserver:<S>` record of any value — the fact the CAS tests, D), `position` (new :525:
  `reclaiming` or unreadable mark rules out that POSITION only, C(iii)).
- `repair` (new :559): ceiling refusal → pre-mark batch pinned to `mpu` bytes, `part` bytes, each
  destination's mark as read (absent → `require_absent`, else `require` + fresh re-stamp, C(ii)) and
  `require_absent(desired:dserver:<S_new>)`; stamp read from `ctx.clock` when the batch is built
  (new :606, C(iv)); the `W_repoint` gate before each write (new :637, C(v)); each write carries
  `deadline = stamp + staged_write_window_millis` (new :635); the adoption batch (new :666 on) pins
  the same facts plus each pre-mark's exact bytes, deletes the pre-marks and the obligation, and puts
  a legacy mark on each vacated position (skipped for an in-place rebuild).
- `repointed_part` (new :714): see "Rebuilding the part record's bytes" below.

**`crates/custodian/src/reconstruction.rs`**
- module docs (base :47-59), field docs of `clock` / `staged_write_window_millis` (base :109-130).
- `RepairPlan.object` → `RepairPlan.target: Target` (base :152-157; `enum Target` new :180).
- `reconcile`: staged reading via `staged::read` (base :205-229), `.set` accessors (base :240-252),
  `assess` call (base :300), `Assessment::Withheld` arm (base :347-352), dispatch of staged plans to
  `staged::repair` (base :399-407).
- `Assessment::Staged` doc + new `Assessment::Withheld` (base :704-712).
- `assess` None branch (base :717-744): held → kept; committed part → `staged::assess`; owned entry
  only → kept (`in-flight`); nothing → `Drain`.
- The fetch/verify loop moved out of `assess` into `gather` + `Gathered::settle` (base :776-877 →
  new :881-980), behavior-preserving, so the committed and staged paths share one copy of the
  permanent-vs-transient fault rule.
- `RepairOutcome::Aborted` doc (base :932-936), `emit_staged` gains a `reason` (base :1267-1283),
  `emit_conflict` / `emit_aborted` docs (base :1285-1303). `repair_chunk` (base :960-1086) is
  untouched.

**`crates/custodian/src/gc.rs`**
- `read_part` returns the decoded record (base :1393-1408); `staged_fragments` now delegates to
  `staged_fragments_observing`, which hands each decoded part (with its session's bytes) to an
  observer during the one walk (base :1501-1528; new :1510-1570). GC, restore and drain status call
  `staged_fragments` unchanged.
- The `deferred: #814` marker is discharged (base :1322-1328) — on this base #813 had already turned
  97fc2f9's `deferred: #663, #664` into `deferred: #814`, so dropping the #663 half now means dropping
  the marker.
- `LATE_WRITE_DEADLINE_MILLIS` doc (base :250-269) and the sweep doc (base :852-855): "no writer on
  `main` marks ahead of its fragment" is no longer true; they now name the re-place and say why the
  sweep stays sound.

**`crates/server/src/custodian.rs`** — three comments that said nothing reads the context clock
yet (base :124-126, :144-145, :540-541). Comments only.

**`docs/design/architecture/06-runtime-view.md`** — base :82, the sentence "it does not yet rebuild
or re-place a staged chunk itself" is replaced by the rebuild (docs-currency rule: a new writer of
`part:` and `orphan:` values).

**`crates/custodian/tests/staged_protection.rs`** — #813's leg-D committed-part case
(`reconstruction_keeps_an_obligation_a_committed_part_still_names`, base :2348-2401) now runs in an
`Aborting` upload, as the brief allows ("retarget it to a session that is not Open"); its `sidx:` case
and control are unchanged. Stale "#814's, not this slice's" messages and the header reworded.

**`crates/custodian/tests/staged_repair.rs`** (new, the brief's test) and **`crates/dst/tests/custodian.rs`**
(property 17 appended, new :4448-4991; two `dst_campaign_test!` entries and one line in
`committed_regression_seeds_stay_green`).

## Decisions, and what I ruled out (with costs)

1. **Where the part record's bytes come from.** The re-place must pin the session and part record
   bytes, which `StagedSet` does not keep. Chosen: an observer on the existing walk
   (`staged_fragments_observing`, ~45 lines in gc.rs), so the bytes come from the same reading the
   protection class and the source-before-destination order come from. Ruled out: (a) a second walk
   in reconstruction — one more session listing plus two range reads per listed session, every pass
   that owes anything, and a second reading that could see a different store than the one the keep
   decision was made on; (b) widening `StagedSet` with the bytes — every GC, restore and drain-status
   pass would carry every part record's bytes (up to 512 × 100 KB per page, per the `STAGED_PAGE`
   derivation at gc.rs:286-300) for one consumer. The observer keeps only parts that name an owed
   chunk.
2. **Rebuilding the part record's bytes.** `PartRecord` has no constructor and `multipart.rs` is out
   of scope; custodian has no `serde`. Chosen: splice the re-encoded chunk list into the stored
   bytes (the decoder only accepts canonical bytes, `multipart.rs:1927-1937`, so the stored value
   spells the list exactly as `metadata::encode` does), then accept the result only if
   `decode_part_record` reads it back with the intended chunks and every other field unchanged. Ruled
   out: a `serde` mirror struct in custodian (a new production dependency across the ADR-0010
   boundary, plus a second spelling of the wire shape); a core constructor (out of scope). The
   splice cannot fail for a canonical record; if it ever did, the repair is `Withheld` and named, not
   a pass-wide error.
3. **The vacated position's mark is the legacy shape.** 0016 wants marks to carry their event, but
   GC's fragment-less sweep leaves every structured, non-`reclaiming` mark in place
   (`event-may-await-write`, gc.rs:893 on base). A vacated position usually has no fragment (it is the
   lost one), so a structured mark there would stay in the ledger after EVERY successful repair. The
   committed path already writes a legacy mark on the positions it vacates (base
   reconstruction.rs:1073-1078), and the sweep retires legacy marks. The re-place writes nothing
   under a vacated position, so the "written after its fragment" condition the sweep relies on holds.
4. **`W_repoint` is an explicit gate AND the deadline is pinned to the pre-mark's stamp.** The brief
   accepts deadline-only enforcement, but 0016:1339-1349 states the gate as a MUST, and with
   production values (`W_write` 30 s > `W_repoint` 10 s) a deadline alone would let a write
   authorized at 20 s through. The gate costs 3 lines and uses the existing
   `crate::gc::W_REPOINT_MILLIS` (no new context field, per "add no other"). Leg C(v) sets the window
   above `W_repoint` so only the gate can refuse; a boundary control (`W_repoint - 1`) completes.
5. **What a pass answers when a staged race is lost.** Aborted/Conflict offset
   `reconstruction_aborted`/`reconstruction_conflict` and leave the obligation queued, exactly as the
   committed path does, so the pass is not `Changed` (in practice `Satisfied`, the committed path's
   answer for a lost CAS). Tests assert `!= Changed`, not a specific value. See "For the human" 2.
6. **Kept vs. rebuilt.** A chunk the staged class holds (untrusted placement) is kept before any
   rebuild is considered, so #813's G-held legs hold. A session value that does not decode is
   `Withheld` (NEEDS-HUMAN audit line, pass `Blocked`), since the re-place cannot tell whether it is
   `Open`.
7. **Servers outside the fleet** are passed over by the chooser (it keeps asking the selector for the
   next server), where the committed path would abort. The brief puts dead servers out of scope; this
   is the choice that avoids a new stall on the staged path and it costs nothing on the committed
   path.
8. **`gather` refactor.** Moving the fetch/verify loop out of `assess` touches ~70 base lines; the
   alternative was a second copy of the permanent-vs-transient fault classification (~60 lines) in
   staged.rs, which is the kind of duplicated rule the rubric asks to avoid.

## Red → green (the brief's test, `crates/custodian/tests/staged_repair.rs`)

- Compiles on the base: it names only base symbols (`ReconstructionContext::{clock,
  staged_write_window_millis}`, `wyrd_custodian::gc::{W_WRITE_MILLIS, W_REPOINT_MILLIS}`,
  `desired_state::desired_key`, `wyrd_testkit::ManualClock`, core/traits items).
- **Red:** production files (`reconstruction.rs`, `gc.rs`) checked out at `feb1e30`, test kept:
  **18 of 20 FAIL by assertion**; the 2 that pass are leg E's "kept, not rebuilt" guards, which hold
  on base by design.
- **Green:** 20 of 20 pass with the fix.
- `run-verify.sh --classify patch.diff` → `ADDED_TEST crates/custodian/tests/staged_repair.rs`.

Planted-defect checks (each planted in staged.rs, run, then restored byte-identical):
- no pre-mark (write then CAS) → leg A **passes**, leg B **fails** — the brief's SELF-TEST — plus 10
  other legs fail;
- stamp/deadline from the pass start (v1) → C(iv) fails;
- a ruled-out position excluding its whole server (v2) → C(iii) fails;
- drain check on recognised values only (v1's `maintenance`) → D fails;
- no `W_repoint` gate → C(v) fails;
- an undecodable vacated mark overwritten → C(vi) fails.

## DST (property 17, `crates/dst/tests/custodian.rs`)

- `staged_replace_under_the_fence_strands_nothing`: the seed picks where the fence lands; it runs at
  **50 seeds** (`MADSIM_TEST_NUM=50`, `xtask/src/main.rs` `DST_SEEDS`), and once more for each of
  the **8 committed regression seeds** in `committed_regression_seeds_stay_green`.
- `staged_replace_reaches_every_point_of_the_fence`: walks all **25** landings (0..=24 ms, each
  +0.5 ms so it never ties with the pass's whole-millisecond steps) and asserts all four points were
  hit: before the pre-mark (which loses), between pre-mark and write, between write and adoption
  (which loses), after the adoption — and that some runs adopted and some did not.
- Checked after every pass: no fragment of the chunk is unnamed AND unmarked; any position the part
  record names that the re-place moved holds an intact fragment. At the end: session `Aborting`;
  adopted ⇔ placement `[0,1,2]`, obligation drained, pre-mark consumed, vacated mark present; not
  adopted ⇔ part bytes identical, obligation queued, pre-mark present if it committed.
- The D servers enforce the deadline (`if_elapsed` before storing, `if_publication_unverified`
  after) on madsim's virtualised wall clock via `wyrd_testkit::SystemClock`, the same source as the
  context clock.
- Red on base: the coverage leg FAILS ("no landing in the span fenced the session before the
  re-place's pre-mark"); the seeded leg passes vacuously there (nothing is written), which is why the
  coverage leg exists. With the no-pre-mark defect planted, BOTH legs fail ("fragment 2 ... on server
  2 is named by no record and covered by no mark — stranded").

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With `reconstruction.rs` and `gc.rs` reverted to `feb1e30` (staged.rs
  then unreferenced), 18 of 20 legs fail by assertion; the DST coverage leg fails on base. Each
  rule's leg also fails with that rule's known-bad implementation planted (list above).
- **(b) Production path?** Yes. Every leg calls the production `reconcile_step` →
  `reconstruction::reconcile` → `staged::assess`/`staged::repair`; nothing is re-implemented in the
  test. The doubles are stores only (metadata map, D-server byte maps), and the D-server doubles
  apply the production deadline seam (`WriteDeadlineExpired::if_elapsed` /
  `if_publication_unverified`).
- **(c) Fixture includes the fault?** Yes. The fixtures lose a real fragment of a real RS-encoded
  chunk and queue its obligation; the fence, the part rewrite, the drain, the stale/reclaiming/
  unreadable marks, the clock jumps and the refused/unverifiable writes are all injected while the
  production pass runs (hooks on reads, commits and D-server writes), and the DST injects the fence
  as a concurrent task at every point of the re-place (the coverage leg proves each point is hit).

## Gate runs (project wrapper `./engine/xtask.sh`)

- `./engine/xtask.sh dst` — green (madsim clippy + all DST tests, 50 seeds).
- `./engine/xtask.sh ci` — first run found one clippy error in the new test (`assertions_on_constants`,
  fixed by a `const _: () = assert!(..)`); second run: **"xtask ci: all checks passed"** (typos, docs
  lint + render, gitlink/unsafe guards, fmt, clippy, build, tests, machete, deny, statics,
  deploy-guard, DST). After two more legs were added to the new test (in-place rebuild, single-copy
  chunk), a **final run on the finished tree also passed: "xtask ci: all checks passed"**. The
  `patch.diff` in this bundle is that tree, checked to apply to `feb1e30` with `git apply --check`.
- `cargo fmt --all -- --check` clean; `typos` clean. `typos` and the docs renderer are installed on
  this host, so no prose gate was skipped.
- Not run: the `host-tikv` row (`WYRD_TIKV_TOOLCHAIN=1` clippy of `wyrd-server --features
  tikv,etcd`). The only server change is three comments, which cannot change what that row compiles.

## For the human at sign-off

1. **Fragment-less pre-marks stay in the `orphan:` ledger.** A move that stops after its pre-mark
   and before its write lands (pre-mark `W_repoint` old, write refused as expired) leaves a
   structured mark with no fragment under it, and GC's sweep leaves such marks by design until the
   writer settles `WriteEffect::Unknown` writes (gc.rs `LATE_WRITE_DEADLINE_MILLIS` doc) — which the
   brief puts out of scope. It is bounded by the number of stopped moves (at most one mark per
   missing fragment per stopped move) and never leaves a fragment without evidence, but it is ledger
   growth with no retirement yet. It probably wants a tracking issue (settle Unknown writes, then add
   `replace:` events to the sweep); I did not invent an issue number.
2. **The pass's answer after a lost staged race** mirrors the committed path's lost CAS (not
   `Changed`, no hole — so `Satisfied` if nothing else happened). GC answers `Partial` for its own
   lost intents; if the reviewers want the staged race to answer `Partial` too, it is a one-line
   change in `reconcile` plus the committed path's matching question.
3. **ADR-0011's `reconstruction_aborted` row** describes only the committed cause ("the selector chose
   a server outside the fleet view"). The staged aborts offset the same counter so the success identity
   still holds; the code doc ADR-0011 names as the source of truth (`emit_aborted`) says so. Two new
   counters (`reconstruction_withheld_staged_repairs`, `reconstruction_unreadable_destination_marks`)
   are not in that ADR's table. ADR edits are out of scope.
4. **Duplicate chunk ids across part records**: the first part in key order is repointed; another
   part naming the same chunk keeps its old position (GC keeps protecting it). Same shape as the
   committed path's first-reference rule (#700 owns duplicates there).
