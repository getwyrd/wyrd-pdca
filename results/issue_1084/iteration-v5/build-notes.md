# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Attempt 5. All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`, base `6e44de9` =
`pdca-integration/main-r9dd123c9ed70`, which carries #1013 and #1015) with this patch applied,
unless marked *base* or *v4* (`iteration-v4/patch.diff`).

This attempt starts from v4's patch, which applies cleanly on the base, and changes only what the
iteration-4 sign-off asked for. The design is unchanged: destination pre-mark, write gate with a
D-server deadline, adoption that consumes the pre-mark, per-move nonce. v4's rationale for the
design itself is in `iteration-v4/build-notes.md` §2–§6 and is not repeated here.

## 0. What this attempt changed

| Carry-forward item | What I did | Where |
|---|---|---|
| **F1** a lost pre-mark or adoption with the root unmoved must not answer `Satisfied` | New helper `lost`: on a lost pre-mark or adoption it re-reads the root (`still_current`). Root unmoved ⇒ `RepairOutcome::Stopped("pre-mark-lost" \| "adoption-lost")`, which sets `refused`, so the pass answers `Blocked`. Root moved ⇒ `RepairOutcome::Conflict`, as on the base. | `reconstruction.rs:1313`, `:1343`, `:1361-1371` |
| F1: assert the verdict in repair D5 and D6 | D5 (four arms) asserts `Blocked` and the audit reason `adoption-lost`. D6 asserts the verdict too; see §2.2 for which one and why. D8 now also asserts A's verdict (`Blocked`), the same class. | `move_destination_marks.rs:1606`, `:1687`, `:1918` |
| F1: the pre-mark-conflict probe (GC swap between assessment and pre-mark) | New leg `d4_destination_raced`, both shapes. | `move_destination_marks.rs:1552` |
| **F2** change the vacated mark between the adoption's read and its commit, both movers, both shapes | New leg `d4_vacated_raced` in both rigs: absent→`reclaiming`, stamped→`reclaiming`, and a third case for the third arm (unreadable→legacy). | `move_destination_marks.rs:1501`; `rebalance.rs:2283` |
| **C5 survivor** `premark.rs:373` (`ruled_out -> true`) | New leg `d4_no_free_domain`, both shapes: no free domain, no marks. | `move_destination_marks.rs:1404` |
| **C5 survivor** `reconstruction.rs:379` (`+=` → `*=`, now `:381`) | The D4 refusal helper asserts the `repair_blocked` gauge reads 1. | `move_destination_marks.rs:1390-1395` |
| **DST rival** reads and pins the vacated mark | The rival's adoption reads the vacated key and writes under a precondition on it, the three arms `PreMarked::adopt` has. | `dst/tests/custodian.rs:6184-6202` |

Three more edits follow from F1 and could not be avoided:

- `docs/design/architecture/06-runtime-view.md:40` now lists the new refusal (docs currency).
- The DST crash property asserted `Satisfied` for a repair whose commit was dropped with nothing
  else changed. That is exactly the case F1 changes, so it now asserts `Blocked`
  (`dst/tests/custodian.rs:774-785`). I found this when the first full gate run went red on it.
- The three container-tier tests that model the same crash asserted `Satisfied` too. They do not
  run in the gate, but would fail in the nightly tiers, so they now assert `Blocked`
  (`chunkstore-grpc/tests/tier1_jepsen_consistency.rs:945-950`, `:1516-1521`;
  `tier2_kill_reconstruct.rs:648-653`).

Size of this attempt's change over v4, measured by rebuilding v4's tree in scratch and diffing:
`reconstruction.rs` 97 changed lines (about 30 of them code, the `lost` helper 11 of those; the rest comments), `rebalance.rs` 46 (all
test), `move_destination_marks.rs` 187, `dst/tests/custodian.rs` about 30, the two tier files 8
lines of substance, the doc 1 sentence. `premark.rs`, `gc.rs` and every other file are byte-identical
to v4.

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base, so I built: `reconcile_step` calls `retirement::reconcile`
(`crates/custodian/src/reconciliation.rs:187`), and `wyrd-metadata-redb` is a custodian
dev-dependency (`crates/custodian/Cargo.toml:65`). No dependency or drain added. `Cargo.lock` and
`deny*.toml` untouched.

## 2. F1 — the verdict

### 2.1 The rule

`repair_chunk` had two places that answered `RepairOutcome::Conflict` for a lost commit: the
pre-mark (`reconstruction.rs:1306` in v4) and the adoption (`:1336` in v4). `Conflict` only logs;
it never sets `refused` or `changed`, so the pass could answer `Satisfied` with the repair undone.

Both now call `lost(meta, object, reason)` (`reconstruction.rs:1361`), which reads the root once
more with the existing `still_current` (`:1377`):

- **Root unmoved.** Nothing else repaired this chunk. GC swapped a destination mark to
  `reclaiming`, another move re-created the destination key, or a vacated mark changed after the
  adoption read it. The answer is `Stopped(reason)`, the variant v4 already uses for a stale
  pre-mark and a refused write. `Stopped` sets `refused` (`:486-489`), `refused` is part of `hole`
  (`:539`), and the pass answers `Blocked`. The audit line is `aborted` with the reason
  `pre-mark-lost` or `adoption-lost`; the staged peer already names its own lost pre-mark
  `pre-mark-lost` on an abort line (`reconstruction/staged.rs:528`), so one grep finds both.
- **Root moved.** A supersede or, in a flat map, a sibling chunk's repoint won. The plan is stale.
  The answer stays `Conflict`, the base's retry.

Rebalance needed no change: it already withholds certification for every move that did not persist
(`rebalance.rs:203`, `EvacOutcome::persisted` `:531`).

### 2.2 Two things the human should look at

**D6 asserts `Satisfied`.** In D6 the racer rewrites the root (version bumped, chunks unchanged),
so the root is no longer the planned generation and the sign-off's rule does not apply. The pass
answers what the base answers, `Satisfied`, and D6 now pins that, plus the audit line (`conflict`,
never `aborted`), the kept obligation, and a later pass that repairs. I assert it because the
sign-off asked for the verdict to be captured and asserted, and because it marks the boundary of
F1 (mutation F1c below turns D6 red). But read plainly, D6 is a pass that says `Satisfied` while a
repair is still owed. That is the base's standing answer for a CAS lost to a moved root
(`reconstruction.rs:536-538`), not something this change introduced, and the sign-off scoped F1 to
`still_current`. If you want that case non-certifying too, see alternative (A) in §2.3 for what it
costs.

**"asserting `Blocked`" in the no-free-domain leg.** I read this as the capacity class
(`Assessment::Blocked`), not the pass verdict. A repair blocked for want of a free domain answers
`Satisfied` on the base (the arm at `reconstruction.rs:377` sets no hole), and v4's notes §11
already listed that as a residual. Making it answer `Reconciled::Blocked` would be a behaviour
change outside the four fixes. So the leg asserts what tells the two classes apart: the
`repair_blocked` gauge reads 1, no `repair-refused` audit line exists, and the verdict is the
base's `Satisfied`. If you meant the pass verdict, that is a one-line production change
(`Assessment::Blocked => { repair_blocked += 1; refused = true }`) and a one-word test change, but
it changes what the deployed loop reports for a minimal cluster with a domain down.

### 2.3 What I ruled out

- **(A) Make every lost pre-mark or adoption non-certifying, no root read.** Smaller than what I
  shipped: two one-line returns, no helper (my helper is 11 lines of code). Rejected on behaviour,
  not size. I ran it (mutation F1c on the whole custodian suite,
  `altA-every-loss-stopped.log` in scratch): it turns two existing base tests red,
  `segmented_map_repoint.rs::a_superseded_root_generation_makes_the_repair_lose` and
  `segmented_map_reconstruction.rs::the_committed_namespace_is_read_once_per_pass`, plus D6. Both
  are cases where another writer moved the object and the base deliberately retries without
  withholding certification. The sign-off names `still_current` as the test.
- **(B) A new `RepairOutcome` variant with its own emitter.** About 25 lines (variant and its doc,
  a match arm, an emitter) against 0 new variants. `Stopped` already means "a committed repair the
  pass still owes, named by its reason", which is this case.
- **(C) Name the loss on the `conflict` audit line.** `emit_conflict` (`reconstruction.rs:1571`)
  has no `reason` field; adding one changes the line every existing conflict emits, the staged
  path's too. The `aborted` line already carries a reason.

## 3. F2 — the vacated mark's precondition

`PreMarked::adopt` reads each vacated position's mark (`premark.rs:538`) and commits under a
precondition on it (`:548-557`, commit `:559`). v4's `d4_vacated` planted the mark before the move
started, so it never tested the precondition.

`d4_vacated_raced` arms the store wrapper's `Act::PlantFirst` on the adoption's commit, so another
writer's put lands after the adoption's read and before its commit. Three cases, one per arm:

| Mark when read | Mark at commit | Arm it tests |
|---|---|---|
| absent | `reclaiming` | `require_absent` (`premark.rs:550-552`) |
| stale legacy `1000` | `reclaiming` | `require(bytes)` + put (`:553-555`) |
| unreadable | legacy `1000` | `require(bytes)`, nothing written (`:556`) |

Each asserts: the mark still holds the other writer's exact bytes; the placement still names
server 5; the fragment landed on server 2 and sits under the move's own pre-mark; nothing on
server 2 is unreferenced and unmarked; the verdict is `Blocked` (repair: with the audit reason
`adoption-lost` and the obligation kept); and a later pass moves the chunk and leaves a
`reclaiming` mark as it finds it.

The sign-off named the first two cases. I added the third because it is one more row in the same
table and without it the third arm's pin has no test: mutation F2c below would survive.

## 4. The C5 survivors

- `ruled_out -> true` (`premark.rs:373`): `d4_no_free_domain` runs a repair over servers 3 and 4
  only, so no free domain exists and no mark is read. It asserts the gauge reads 1, no
  `repair-refused` line exists, and the verdict (§2.2). Under the mutant the pass logs
  `repair-refused` and answers `Blocked`; the leg goes red.
- `+=` → `*=` (`reconstruction.rs:381`): the D4 refusal helper now asserts the gauge reads 1 after
  a refused repair. Under the mutant it reads 0.

The gauge is read off the `tracing` event the pass emits
(`gauge.reconstruction_repair_blocked`, `reconstruction.rs:1440`), through the layer the rig
already attaches to each pass (`move_destination_marks.rs:708-717`, `:745`): 9 added lines. The
alternative, the Prometheus export `tests/reconstruction.rs:1640` uses, needs a
`DurabilityTelemetry` service wired into the rig, about 30 lines, for the same number.

The three C5 timeouts in v4's run (`premark.rs:269`, `:286`, `:324`) are infinite loops in
`choose` under the mutant, which the suite's timeout catches. Not touched.

## 5. Tests

### 5.1 Counts

- `crates/custodian/tests/move_destination_marks.rs`: 48 tests (v4: 42). New: `d4_no_free_domain`,
  `d4_vacated_raced`, `d4_destination_raced`, each on both shapes.
- Inline evacuation legs, `crates/custodian/src/rebalance.rs`: 36 (v4: 34). New:
  `d4_vacated_raced` on both shapes. The lib test binary runs 76 tests in all.
- `crates/dst/tests/custodian.rs`: no new test; the rival's model and one assertion changed.

### 5.2 Red on the base

Every modified tracked file back to `6e44de9`, the new test file kept (the base `lib.rs` has no
`mod premark`, so `premark.rs` is not compiled): **44 of 48 fail**, each at its own claim. Log:
`base-red.log` in scratch. The new and changed legs:

- `d4_vacated_raced`: `left: Some(b"10000000")` against the planted `reclaiming` bytes — the base
  wrote the vacated mark blind, over the other writer's.
- `d4_destination_raced`: `left: Some(b"1000")` — the base commits no pre-mark, so nothing stood
  between the assessment and the write.
- D5: "the move adopted a destination it may not publish" (placement names server 2).
- D6: "the lost move's fragment is kept for ever".
- D8: "B never reached its pre-mark commit".
- D4 refusals: "the repair wrote to a destination it may not write".

The four that pass on the base: the two D10a crash legs (as in v4; they guard v4's mutation M11),
and the two `d4_no_free_domain` legs. The base has no `Refused` class, so a capacity block is
already answered as a capacity block there; those two legs guard mutation C5a, not the base
defect.

### 5.3 Mutations of the fixed tree (this attempt's)

Each applied alone to the worktree, the repair file and the lib tests run with `--no-fail-fast`,
the file restored after each and the tree's diff hash checked equal at the end. Script, logs and
results: `$PDCA_SCRATCH/pdca-builder-1084-i5/` (`mutate.py`, one `.log` per mutation). They ran
before I changed the DST and tier-test assertions (§0); those files are not part of the custodian
test binaries the mutations ran, so the results hold for the final tree.

| Mutation | Red | Which legs |
|---|---|---|
| F1a a lost pre-mark answers `Conflict` | 2 | `d4_destination_raced` ×2 |
| F1b a lost adoption answers `Conflict` | 8 | D5 ×4, repair `d4_vacated_raced` ×2, D8 ×2 |
| F1c every loss answers `Stopped` (no root read) | 2 | D6 ×2 |
| F2a vacated absent: `require_absent` removed | 4 | `d4_vacated_raced`, repair ×2 and evacuation ×2 |
| F2b vacated stamped: `require` removed | 4 | the same four |
| F2c vacated kept: pin removed | 4 | the same four |
| C5a `ruled_out` always `true` | 2 | `d4_no_free_domain` ×2 |
| C5b refused chunk not counted (`*=`) | 4 | D4 `reclaiming` ×2, D4 unreadable ×2 |

F2a–c together are the mutant the iteration-4 reviewer and adversary reported as surviving.

v4's mutation table (M1–M12, `iteration-v4/build-notes.md` §4.3) covers code this attempt did not
touch. I did not re-run it; `premark.rs` is byte-identical to v4.

### 5.4 Refuting my own test

- **(a) Genuine red?** Yes. On the base 44 of 48 fail at their own claims (§5.2). The four that
  pass on the base go red under a mutation of the fix (C5a here, M11 in v4). Each new assertion
  was checked by reverting the line it binds and re-running: §5.3, eight mutations, eight reds. The
  inline evacuation legs cannot compile on the base; F2a–c turn the new ones red.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` or `rebalance::reconcile`, the real `premark` module, real GC
  and the real retirement drain, over `RedbMetadataStore::in_memory()`. The store wrapper only
  holds a commit or lands another writer's put ahead of it; the D-server double applies the
  trait's own deadline checks (`WriteDeadlineExpired::if_elapsed`,
  `if_publication_unverified`). No production logic is copied into a test.
- **(c) Fixture includes the fault?** Yes. The racing writer's put is really committed to the
  store between the adoption's read and its commit, or between the assessment and the pre-mark,
  and each leg asserts it landed: the mark must hold the racer's exact bytes afterwards, which is
  only true if the hook fired. D5 asserts the key holds `reclaiming` at the moment the adoption is
  released. The no-free-domain leg really has no free domain (fleet and topology are servers 3
  and 4).

One honest limit: the DST rival's new pin is model fidelity, not a new property. The property it
serves schedules no writer on the vacated key (the iteration-4 reviewer noted this; seeded GC
interleavings are #682's), so no seed can show the pin failing. It was checked by the gate's DST
leg staying green.

## 6. The iteration-4 review batch (10 rows)

| Row | Disposition |
|---|---|
| `reconstruction.rs:1306`, `:1308`, `:1336` (BUG ×3) | Fixed: F1 (§2). |
| `move_destination_marks.rs:1400` ×2, `rebalance.rs:707` (TEST-GAP ×3) | Fixed: F2 (§3). |
| `dst/tests/custodian.rs:6181` (TEST-GAP) | Fixed: the rival pins the vacated mark. |
| `premark.rs:163` (BUG: a `reclaiming` mark on a named position blocks in-place repair) | Recorded in `review-rejected.md`: out of scope, GC's side is #1083. |
| `reconstruction.rs:924` (now `:927`), `premark.rs:373` (BUG: the choice may offer a server outside the fleet) | Recorded in `review-rejected.md`: the base's abort contract, not reachable in the deployed loop. |

**The three recorded rejections are my proposal, for the human to confirm.** The sign-off said
"fix only" the four items, and the iteration-4 reviewer had already judged these three not to be
defects of this change (`iteration-v4/check-review.md`, "The other batch reports"). Without an
entry the T4 gate would block on them again. I wrote the entries from the reviewer's reasoning and
marked them in the file as the builder's. If you disagree with any, delete its line and the gate
will raise it again. The fleet-versus-topology one has no tracker issue; you may want to file one.

## 7. Gates run

All on the final tree. `patch.diff` is `git diff 6e44de9` of it (24 files, the same set as v4);
the tree's diff hash is the same before the last gate run and at the moment `patch.diff` was
written.

- **The project's gate, `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci`: exit 0, "xtask ci: all
  checks passed".** It ran `typos` (clean), the docs render with its link audit (101 pages), the
  gitlink, unsafe and blackbox guards, `cargo fmt --all -- --check`, workspace clippy, build and
  test, cargo-machete, the three `cargo deny` runs, conformance, the statics gate (ADR-0035), the
  deploy guard, and clippy plus tests for `wyrd-dst` under `--cfg madsim` with the seed sweep.
  Totals: 235 test binaries, **2690 passed, 0 failed, 16 ignored**. In the DST leg the nonce's
  replay test, the drain-race property (the rival) and the crash property all pass. Log:
  `ci-i5-run2.log` in scratch.
- **The first gate run was red**, and I kept its log (`ci-i5.log`): the DST crash property and
  its committed-seed replay failed with `left: Blocked, right: Satisfied`
  (`dst/tests/custodian.rs:779` then). That is F1 working on a test that had encoded the old
  answer; §0 says what I changed. Nothing else failed in that run (2654 passed, 2 failed).
- Red on the base and the eight mutations: §5.2, §5.3. These used
  `timeout 560 cargo test --offline -p wyrd-custodian …`, the command the verify gate itself runs,
  with a hard timeout, because `cargo xtask` has no way to run one test file.
- `engine/scripts/run-verify.sh --classify patch.diff`: `ADDED_TEST
  crates/custodian/tests/move_destination_marks.rs`; crates `chunkstore-grpc`, `custodian`, `dst`,
  `server`, as the brief's dry run. I did not run its full mode: it builds a `../wyrd-verify`
  worktree outside the roots I may write. §5.2 is the same red leg, done in the cycle worktree.
- `patch.diff` applies forward on the base's files and reproduces the worktree byte for byte
  (checked in scratch with `patch -p1`); `git apply --check -R` passes on the worktree;
  `git diff --check` is clean. `Cargo.lock`, `deny*.toml` and every `Cargo.toml` are untouched.
- Commit hooks: the repo's formatter is `cargo fmt` and its spell checker `typos`; both ran inside
  the gate on the final tree.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013 and
  #1015. It could not measure last round for that reason, and nothing in the patch can change it.

## 8. Manual validation (nightly tiers, not run here)

Not declared as dependencies (pdca-harness #969). They compile in this tree (the gate builds every
target). This attempt changed an assertion in two of them, so they are worth one run:

- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`
  (docker). Phase 1 now expects `Reconciled::Blocked` (`:648-653`).
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen` (docker).
  Phase 1 of both legs now expects `Reconciled::Blocked` (`:945-950`, `:1516-1521`).
- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (root +
  device-mapper). No assertion changed.

What to check: phase 1 of each scenario answers `Blocked`, the later phases still repair
(`Reconciled::Changed`) and then answer `Satisfied`; kill-reconstruct still finds its orphan on
the spare; no `repair-refused` line appears in a healthy run.

## 9. Residuals

- **#1100** (in-place late adoption), **#825** (settling an aborted move's pre-marks), **#682**
  (seeded DST for these interleavings): deferred, as in v4.
- **A CAS lost to a moved root still answers `Satisfied`** while the repair is owed (D6, §2.2).
  The base's behaviour.
- **A capacity-blocked repair and an `unplaced` abort still answer `Satisfied`**, as on the base
  (§2.2). Candidate follow-up, with the one above.
- **A `reclaiming` mark on a position the placement already names** keeps an in-place repair
  waiting until GC clears it (review row `premark.rs:163`). Named on the audit seam every pass;
  GC's side is #1083.
- **Fleet versus topology** (review rows `reconstruction.rs:927`, `premark.rs:373`): library-only.
- The rest of v4's list (`iteration-v4/build-notes.md` §11) stands unchanged.

## 10. Scratch

`$PDCA_SCRATCH/pdca-builder-1084-i5/`: the mutation script and logs, `base-red.log`,
`altA-every-loss-stopped.log`, the two gate logs (`ci-i5.log` red, `ci-i5-run2.log` green), copies
of v4's tree and of the patched base files used to measure this attempt's delta and to check the
patch applies, and snapshots of the worktree diff. 2.8 MB in all, and no build directory of my
own: every run used the worktree's `target/`. I left it in place because the notes cite its logs;
the harness reclaims it.
