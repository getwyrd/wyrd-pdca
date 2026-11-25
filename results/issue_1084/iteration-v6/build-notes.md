# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Attempt 6. All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`, base `6e44de9` =
`pdca-integration/main-r9dd123c9ed70`, which carries #1013 and #1015) with this patch applied,
unless marked *base* or *v5* (`iteration-v5/patch.diff`).

This attempt starts from v5's patch, which applies cleanly on the base, and changes only what the
iteration-5 sign-off asked for. The design is unchanged: destination pre-mark, write gate with a
D-server deadline, adoption that consumes the pre-mark, per-move nonce. The rationale for the
design is in `iteration-v4/build-notes.md` §2–§6 and `iteration-v5/build-notes.md` §2–§4, and is
not repeated here.

## 0. What this attempt changed

| Carry-forward item | What I did | Where |
|---|---|---|
| **No-free-domain verdict** | `Assessment::Blocked` now sets `refused`, so a capacity-blocked repair answers `Reconciled::Blocked`. It is still counted on the repair-blocked level and still gets no `repair-refused` audit line. Both `d4_no_free_domain` legs require `Blocked`. One existing test asserted `Satisfied` for this case and now asserts `Blocked` (§2). | `reconstruction.rs:381-384`; `move_destination_marks.rs:1405-1428`; `staged_repair.rs:2121-2135` |
| **Evacuation doubles enforce the write deadline** | The three `MemDServer` doubles hold the context's `ManualClock` and refuse a write at or after its deadline, the same two checks the reconstruction double makes. Every test and fixture shares one clock between its D servers and its context. | `tests/rebalance.rs:127-166`; `tests/segmented_map_evacuate.rs:181-220`, `:301`, `:329`; `tests/segmented_map_rebalance.rs:102-141`, `:219`, `:247` |
| **Pre-mark pins test** | New leg `d6_superseded_before_the_premark`, both shapes: a production Complete supersedes the object while the move is held just before its pre-mark commit. Asserts no write was sent to server 2 and no pre-mark key exists. Kills the `.take(0)` mutation (§4). | `move_destination_marks.rs:1735-1766` |
| **#1100 class widened, as docs** | The four places the sign-off named now say the exception is "any unreference whose mark is stamped before the in-place batch and commits after it", and list the gateway unlink and flat overwrite beside the competing move. Two more comments said the old, narrower thing and are corrected too. | `gc.rs:263-271`; `premark.rs:36-38`, `:48-50`, `:51-58`, `:61-65`, `:69-72`, `:480-482`; `reconstruction.rs:1331-1333`; `06-runtime-view.md:40` |

Two things beyond the four items, both small and both forced by them:

- `06-runtime-view.md:40` gained one sentence saying a repair with no free failure domain is not
  certified over (docs currency: the verdict changed, and the paragraph lists which repairs do
  not certify).
- `review-rejected.md` has new and re-pinned entries (§6).

Size of this attempt's change over v5, measured by rebuilding v5's tree in scratch and diffing
(changed lines, added plus removed):

| File | Lines | Of which code |
|---|---|---|
| `src/reconstruction.rs` | 57 | 5 (the arm: 1 removed, 4 added); the rest comments |
| `src/premark.rs` | 34 | 0 (comments only) |
| `src/gc.rs` | 12 | 0 (doc comment only) |
| `tests/move_destination_marks.rs` | 54 | test |
| `tests/rebalance.rs` | 138 | test (the double 32 lines, then 10 tests × ~10 lines) |
| `tests/segmented_map_evacuate.rs` | 64 | test |
| `tests/segmented_map_rebalance.rs` | 60 | test |
| `tests/staged_repair.rs` | 9 | test |
| `06-runtime-view.md` | 2 | doc |

Every other file is byte-identical to v5. The patch now touches 25 files (v5: 24; the new one is
`tests/staged_repair.rs`).

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base, so I built: `reconcile_step` calls `retirement::reconcile`
(`crates/custodian/src/reconciliation.rs:187`), and `wyrd-metadata-redb` is a custodian
dev-dependency (`crates/custodian/Cargo.toml:65`). No dependency or drain added. `Cargo.lock`,
`deny*.toml` and every `Cargo.toml` are untouched.

## 2. The no-free-domain verdict

### 2.1 The change

```rust
// reconstruction.rs:381-384
Assessment::Blocked => {
    repair_blocked += 1;
    refused = true;
}
```

`refused` is part of `hole` (`reconstruction.rs:546`), so the pass answers `Blocked`. Nothing else
moves: the gauge is emitted from the same counter (`:454`), and `emit_repair_refused` is called
only from the `Assessment::Refused` arm (`:387-391`), so a capacity block still has no
`repair-refused` line. The `d4_no_free_domain` legs assert all three: gauge reads 1, no
`repair-refused`, verdict `Blocked`.

### 2.2 Three things the human should know

**The staged path shares the arm.** `staged::assess` also answers `Assessment::Blocked`
(`reconstruction/staged.rs:326`: no usable destination for a staged chunk). It
goes through the same match arm, so a staged repair with no usable destination now answers
`Blocked` too. That is the one existing test that asserted `Satisfied` for this case:
`staged_repair.rs::a_degraded_chunk_with_no_usable_destination_is_kept` (`:2121`), whose doc said
"as for a committed chunk with no free failure domain". It now requires `Blocked`. I kept the
staged case in, rather than carve it out, because the sign-off named the arm and the invariant
("no pass reports redundancy restored while a repair it owes is still undone") does not depend on
which record names the chunk. Carving it out would cost a second `Assessment` variant or a flag on
this one (about 8 lines) to keep a `Satisfied` the invariant says is wrong.

**What the deployed loop now reports.** A minimal cluster at exactly `n` servers with one domain
down answers `Blocked` on every reconstruction pass until capacity returns. Nothing in
`crates/server` branches on that for the loop (`Reconciled::Blocked` is read only by the one-shot
CLI, `crates/server/src/cli.rs:1129-1138`, which is the reaper's). The server test for this case,
`custodian_day_one.rs::a_repair_with_no_free_domain_is_blocked_off_the_backlog_gauge` (`:1003`),
asserts the gauges and not the verdict; it passes unchanged.

**Other tests I checked for the old answer.** The custodian suite (three failures after the
production change, all expected: the two `d4_no_free_domain` legs and the staged test above; log
`item1-custodian.log`), the workspace and the madsim DST leg through the gate (§8), and by reading
the container-tier tests the gate does not run (`tier1_jepsen_consistency.rs`,
`tier2_kill_reconstruct.rs`, `tier1_disk_faults.rs`): none has a capacity-blocked pass.

### 2.3 What I ruled out

- **A flag of its own instead of reusing `refused`.** Sketch:

  ```rust
  let mut capacity_blocked = false;                       // +1
  Assessment::Blocked => { repair_blocked += 1; capacity_blocked = true; }
  let hole = … || refused || capacity_blocked;            // +1 term
  ```

  Three more lines than what I shipped (declaration, its comment, the `hole` term), for a name that
  reads better. Not taken: the sign-off spelled the change as `refused = true`, and the comment on
  `refused` (`reconstruction.rs:349-357`) now says it covers a repair the pass could not make as
  well as one it refused. If you prefer the separate name it is a mechanical follow-up.
- **Making an `unplaced` abort non-certifying too** (`RepairOutcome::Aborted`,
  `reconstruction.rs:492`: the selector picked a server outside the fleet view). One line
  (`refused = true`), and it would close one more case where a pass answers `Satisfied` with a
  repair it owes undone. Not taken: the sign-off listed four fixes and this is not one of them; the
  base's test for that abort (`tests/reconstruction.rs:1951`) asserts `Changed` for a pass that
  mixes a committed repair with an aborted one, and would have to change; and the deployed loop
  cannot reach it (fleet and topology are built together,
  `crates/server/src/custodian.rs:293-306`). Listed under residuals (§10).

## 3. The evacuation doubles

Each of the three doubles dropped `put_fragment`'s deadline (`_deadline_millis`). Each now is the
double `tests/reconstruction.rs:119-159` already is: it holds a `ManualClock`, refuses at or after
the deadline before storing (`WriteDeadlineExpired::if_elapsed`), and refuses to certify a landing
its clock reads at or past the deadline once the bytes are stored
(`WriteDeadlineExpired::if_publication_unverified`). That is what `FsChunkStore` does
(`crates/chunkstore-fs/src/lib.rs:286-288`).

One clock per lifecycle (ADR-0009): the clock the double judges on is the one the context reads.

- `tests/rebalance.rs`: each of its 10 tests makes one `pass_clock`, builds its servers with
  `MemDServer::on(&pass_clock)` and passes `clock: &pass_clock`. Before, each context got a fresh
  `ManualClock::new(500)` nothing else could see.
- `tests/segmented_map_evacuate.rs`, `tests/segmented_map_rebalance.rs`: the `Fixture` owns the
  clock (`:301`, `:219`), its servers are built on it, and `pass()` hands it to the context
  (`:329`, `:247`).

**Does it bind?** Mutation `deadline-at-stamp`: `PreMarked::authorize` returns the pre-mark's stamp
as the deadline (window 0), so every write arrives at its deadline and a D server must refuse it.

| Doubles | Result on the three files |
|---|---|
| v5's (deadline dropped) | 28 passed, 0 failed (`mut-deadline-at-stamp.v5-doubles.log`) |
| this attempt's | 12 passed, **16 failed**: 7 + 6 + 3 (`mut-deadline-at-stamp.new-doubles.log`) |

The 12 that still pass under the mutation are tests whose claim does not need an evacuation write
to land: aborts, conflicts, ceiling refusals, contained objects, store faults, a malformed
placement, and a pass that owes nothing.

**Not changed: `tests/staged_drain_status.rs`.** Its `Disk` double also drops the deadline
(`:177`), and it builds a `RebalanceContext` (`:641`). Its one rebalance test asserts that the pass
writes no fragment anywhere, so no deadline ever reaches the double. The sign-off named three
files; I left the fourth alone and say so here in case a reviewer raises it.

**Ruled out: one shared double instead of four copies.** The custodian test directory would need a
`tests/common/mod.rs` (about 45 lines) and `mod common;` in four files, and would drop about 45
lines from each: roughly 130 fewer lines than what I shipped. Not taken, for three reasons that
are not about size: no file in `crates/custodian/tests/` shares a module today (19 files each
define their own `MemMeta` or `MemDServer`), so it would be a new convention; it adds a file under
`tests/`, and the brief's C4-verify classification was dry-run on a file set with exactly one
added test file; and the sign-off asked for the doubles to do "as the updated reconstruction
double does", which is a per-file copy.

## 4. The pre-mark pins test

`premark::premark` folds the repoint batch's own preconditions into the pre-mark batch
(`premark.rs:465-471`): the root generation and, for a segmented map, the `seg:` record. v5 had no
test that failed when that fold was emptied; C5's `.take(0)` mutant survived.

`d6_superseded_before_the_premark` (`move_destination_marks.rs:1735`):

1. Server 5 loses the fragment; the repair is run with the store wrapper holding the first commit
   that pre-marks server 2's position (`Act::Before`, matcher `premarks_free`). At that point the
   assessment has chosen server 2 and `repoint_chunk` has prepared the repoint.
2. While it is held, `rig.supersede()` runs a production multipart Complete over the same name.
   It asserts for itself that the superseded generation's `retire:bytes:` obligation exists.
3. The commit is released.

It asserts: the hold was reached; server 2 was sent no write (`Fleet::sent_to(FREE)` empty); the
key `orphan:2:<chunk>:2` does not exist; and the audit seam names the loss a `conflict` and not an
`aborted` (the root moved, so this is a stale plan, not a refusal).

Under `.take(0)` the pre-mark commits without its pins, the write is sent, and the adoption then
loses; both legs fail at "a move whose object was superseded before its pre-mark sent a write"
(`mut-pins-dropped.log`). The other 48 tests in the file pass under that mutant, which is why it
survived before.

I put the leg in the repair file only, on both shapes, as the sign-off asked ("one leg per
shape"). The fold is in the shared module, so one mover is enough to kill the mutant. An
evacuation twin in `src/rebalance.rs` would be about 40 more lines; I did not add it.

## 5. The #1100 wording

The sign-off's finding: the in-place exception is wider than "a competing move's late adoption".
Unlink and the flat overwrite write a legacy mark inline, stamped with a time the caller read
before the commit (`crates/core/src/metadata.rs:2186-2189`, `:2301-2304`, `:2505-2508`). If that
stamp is older than the in-place batch and the commit lands after it, GC can sweep the mark while
the position is still empty and the repair's write deadline is still open. The write then lands
with no reference and no mark. Bytes kept, never lost.

What I changed, all comments or docs:

- `gc.rs:263-271`: the sweep's premise now says an unreference that reads its stamp *after* the
  batch marks no earlier than it, and names the exception in the sign-off's words.
- `premark.rs:36-38`: the same correction in the paragraph that explains why a named position is
  never marked. v5 said "an unreference after the pre-mark batch is marked by its own drain",
  which is false for an inline mark stamped earlier.
- `premark.rs:48-50`: the same claim, made a second time in that paragraph.
- `premark.rs:51-58`: the `deferred: #1100` paragraph, rewritten around the wider class.
- `premark.rs:61-65`, `:69-72`: the two later sentences that referred to "the in-place exception".
- `premark.rs:480-482`, `reconstruction.rs:1331-1333`: the two in-code `deferred: #1100` markers.
- `06-runtime-view.md:40`: one sentence naming the open case and #1100.

The second hunk of v5's `gc.rs` edit (`gc.rs:871-881`, about the retirement drain's mark) is
unchanged: it is about a drain, which stamps its marks after its obligation is installed, so it is
not part of the wider class.

**For the human — widen #1100 on the tracker.** Its title and body describe only the competing
move ("…behind a late-committing move"). Suggested addition to the body:

> **Wider than a competing move (2026-11-24, #1084 iteration 5).** The same strand follows from
> *any* unreference whose mark is stamped before the in-place repair's pre-mark batch and commits
> after it. Unlink and the flat overwrite stamp their legacy marks with a time the caller read
> before the commit (`crates/core/src/metadata.rs`, `unlink` and the two overwrite paths), so a
> slow DELETE or PUT is enough: no second mover is needed. GC's fragment-less sweep can retire
> that early mark while the position is still empty and the repair's write deadline
> (`batch stamp + W_write`) is still open; the write then lands unreferenced and unmarked. Bytes
> kept, never lost. The code and docs name this class at `crates/custodian/src/premark.rs`
> (module doc, `deferred: #1100`), `crates/custodian/src/gc.rs` (`LATE_WRITE_DEADLINE_MILLIS`) and
> `docs/design/architecture/06-runtime-view.md` §6.3 step 2.

Suggested title: "custodian: an in-place repair's write can land unreferenced and unmarked behind
any unreference stamped before it and committed after it".

## 6. The iteration-5 review batch (9 rows) and `review-rejected.md`

| Row | Disposition |
|---|---|
| `tests/segmented_map_evacuate.rs:181`, `tests/rebalance.rs:863` (TEST-GAP ×2: doubles drop the deadline) | Fixed (§3). |
| `premark.rs:480` ×2 (BUG: unlink / DELETE / overwrite strands an in-place fragment) | Docs widened (§5), recorded as deferred to #1100, as the sign-off decided. |
| `premark.rs:386` ×2 (BUG: a topology server outside the fleet) | Recorded-rejected since iteration 5; the entry did not match the new wording, so I re-pinned its MATCH to `fleet`. |
| `premark.rs:546` (BUG: no destination drain fence at the adoption) | Recorded as out of scope. **Needs a tracker issue, see below.** |
| `premark.rs:556` (BUG: a vacated `reclaiming` mark is left as it is) | Recorded-rejected: it is the brief's own rule (Scope, *The vacated position's mark*; D4). |
| `reconstruction.rs:1330` (TEST-GAP: no seeded Tier-0 partial fan-out) | Recorded as deferred to #682, which the brief puts out of scope and the code marks (`reconstruction.rs:1260`). |

**The entries are my proposal, for the human to confirm.** The sign-off said "fix only these", and
the iteration-5 reviewer ruled on each of the other rows (`iteration-v5/check-review.md`, "The
batch review's other concerns are settled or outside this slice's declared change"). Without an
entry the T4 gate blocks on them again, as it did in iteration 5, where none of v5's four entries
matched ("0 recorded-rejected"). I wrote each entry from the reviewer's reasoning and marked the
block in the file as the builder's. Delete a line and the gate raises its finding again.

I dry-ran the file against the iteration-5 batch with the gate's own matcher
(`scripts/review-branch`, `load_rejected` and `is_rejected`; script `check_rejections.py` in
scratch): the seven rows above that are not fixed in code all resolve as rejected; the two
TEST-GAP rows do not, as they should not, because they are fixed rather than recorded.

One entry's MATCH is broad on purpose: `premark.rs:484 | BUG | in-place repair`. The two
iteration-5 wordings of the unlink trigger share no narrower phrase. A decision binds only to the
one finding nearest its line, so it cannot clear a second in-place finding elsewhere in the file,
but a *different* in-place bug reported near line 484 would be cleared by it. Tighten the phrase
if you want that risk gone.

**The drain fence has no open issue behind it.** Proposal 0016's repoint row requires
`require_absent(desired:dserver:<S_new>)` on the adoption (`0016:670`). The committed movers have
never carried it, on the base or here; the staged re-place does. The iteration-5 reviewer
reproduced the gap (a server marked draining just before the adoption still becomes the published
home) and called it a tracked exclusion, citing #722's deferral to #653. I checked the tracker:
#722 does say "drain fence, rollback and resume (#653)", but **#653 is closed** and the fence is
still missing. So the deferral points at nothing open. I did not add it here (not one of the four
fixes; about 15 lines in `PreMarked::adopt` plus a refusal path and legs for two movers and two
shapes). I did not file an issue either; that is yours. Draft:

> **custodian: committed repair and evacuation adopt a destination without the drain fence
> (0016:670).** The adoption commit of a committed move (`crates/custodian/src/premark.rs`,
> `PreMarked::adopt`) pins the root generation, the `seg:` record, the destination's pre-mark and
> the vacated mark, but not `desired:dserver:<S_new>`. A server marked draining after the move
> chose it, and certified empty, can become the published home of the moved fragment. 0016's
> segment-repoint row requires `require_absent(desired:dserver:<S_new>)`; the staged re-place has
> it (`crates/custodian/src/reconstruction/staged.rs`). #722 deferred this to #653, which closed
> without it. Reproduced by the #1084 iteration-5 reviewer on both map shapes. Success: a
> desired-state record written between the pre-mark and the adoption makes the adoption lose, the
> fragment stays under its pre-mark, and the pass does not certify.

Once it exists, the `review-rejected.md` entry should name it, and the adoption can carry a
`// deferred: #N` marker.

## 7. Tests

### 7.1 Counts

- `crates/custodian/tests/move_destination_marks.rs`: 50 tests (v5: 48). New:
  `d6_superseded_before_the_premark` on both shapes. Changed: `d4_no_free_domain` on both shapes.
- Inline evacuation legs, `crates/custodian/src/rebalance.rs`: 36, unchanged from v5.
- `tests/rebalance.rs` 10, `tests/segmented_map_evacuate.rs` 13, `tests/segmented_map_rebalance.rs`
  5: same tests, now against deadline-enforcing doubles.
- `tests/staged_repair.rs`: 30 tests, one assertion changed.

### 7.2 Red on the base

Every tracked file back to `6e44de9` (`git apply -R` of the tracked diff), the new test file kept;
the base `lib.rs` has no `mod premark`, so `premark.rs` is not compiled. **48 of 50 fail**, each at
its own claim (`base-red.log`). The legs this attempt added or changed:

- `d4_no_free_domain` ×2: `left: Satisfied, right: Blocked` — "a pass certified over a repair it
  owes and could not place". In v5 these two passed on the base; now they are red there.
- `d6_superseded_before_the_premark` ×2: "the move never reached its pre-mark commit" — the base
  commits no pre-mark, so nothing stands between the assessment and the write.

The two that pass on the base are the D10a crash legs, as in v4 and v5 (they guard v4's mutation
M11: a pre-mark written at an in-place position).

The tree was restored with `git apply` of the same diff and its hash checked equal.

### 7.3 Mutations of the fixed tree (this attempt's)

Each applied alone, the named test files run with `--no-fail-fast`, the file restored from a saved
copy and checked byte-equal. Script and logs: `$PDCA_SCRATCH/pdca-builder-1084-i6/` (`mutate.py`,
`mut-*.log`).

| Mutation | Red | Which tests |
|---|---|---|
| `pins-dropped`: `.take(0)` on the pin fold (`premark.rs:465-471`) | 2 | `d6_…_superseded_before_its_pre_mark_writes_nothing` ×2 |
| `blocked-certifies`: `refused = true` removed from the `Blocked` arm (`reconstruction.rs:383`) | 3 | `d4_…_no_free_domain_is_blocked_not_refused` ×2, `a_degraded_chunk_with_no_usable_destination_is_kept` |
| `deadline-at-stamp`: the write deadline fixed at the pre-mark's stamp (`premark.rs:509-510`) | 16 | 7 in `tests/rebalance.rs`, 6 in `segmented_map_evacuate.rs`, 3 in `segmented_map_rebalance.rs`; 0 with v5's doubles (§3) |

v4's and v5's mutation tables (M1–M12; F1a–c, F2a–c, C5a–b) cover code this attempt did not touch.
I did not re-run them, with one exception, because this attempt changes what kills it: v5's C5a
(`ruled_out` always `true`) was killed by `d4_no_free_domain` through the verdict *and* the
`repair-refused` line. Now both classes answer `Blocked`, so only the `repair-refused` assertion
(`move_destination_marks.rs:1416-1420`) can kill it. Re-run: 2 red, both at that assertion
(`mut-ruled-out-always.log`).

### 7.4 Refuting my own test

- **(a) Genuine red?** Yes. On the base 48 of 50 fail at their own claims (§7.2); the two that
  pass go red under v4's M11 (v4's notes; not re-run here). Each assertion this attempt added was checked by reverting the line
  it binds and re-running: three mutations, three reds (§7.3). For the doubles, the same mutation
  is green against v5's doubles and red against these, which is the red→green for a test-fidelity
  fix.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` or `rebalance::reconcile`, the real `premark` module and the
  real retirement drain. The pins leg's supersede is `complete_upload`, the production multipart
  Complete, not a hand-written root rewrite. The doubles apply the trait crate's own deadline
  checks (`WriteDeadlineExpired::if_elapsed`, `if_publication_unverified`); no production logic is
  copied into a test.
- **(c) Fixture includes the fault?** Yes. The pins leg asserts the hold was reached (so the
  supersede ran after the assessment and before the pre-mark commit), and `supersede()` asserts
  the superseded generation's obligation exists. The no-free-domain legs really have no free
  domain: fleet and topology are servers 3 and 4, and the legs assert the gauge reads 1.

One honest limit, unchanged from v5: the evacuation doubles' new checks are exercised by the
mutation above, not by a test that moves the clock in those three files. The deadline legs proper
(D7a–d, both movers) live in `move_destination_marks.rs` and the inline module of
`src/rebalance.rs`, whose doubles already enforced it.

## 8. Gates run

All on the final tree. `patch.diff` is `git diff HEAD` of the worktree with the two new files
marked intent-to-add (25 files). The tree's hash was recorded when the last gate run started and
checked equal when `patch.diff` was written.

- **The project's gate, `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci`: exit 0, "xtask ci: all
  checks passed"** (`ci-i6-run2.log`). It ran `typos` (clean), the docs lint and the site render
  with its link audit (101 pages), the gitlink, unsafe and blackbox guards,
  `cargo fmt --all -- --check`, workspace clippy, build and test, cargo-machete, the three
  `cargo deny` runs, the statics gate (ADR-0035), the deploy guard, and clippy plus tests for
  `wyrd-dst` under `--cfg madsim` with the seed sweep. Totals: 235 test binaries, **2692 passed,
  0 failed, 16 ignored** (v5: 2690; the two more are the new pins legs). In the DST leg the
  nonce's replay test and the crash properties pass.
- **The first gate run was red, on a test this patch does not touch**, and I kept its log
  (`ci-i6.log`): `crates/server/tests/health_probe.rs::check_reports_serving_while_the_store_is_healthy`
  panicked with "connect to the configured health endpoint within budget: transport error" — a
  real TCP connect to localhost that did not come up inside its 5-second budget. The patch changes
  no file under `crates/server/src` and not that test file. Run alone right after, the file passed
  13 of 13; in the second full run it passed. I read it as a timing flake on a busy host, not a
  result of this change, but you can see it happened. Because `cargo test --workspace` stops at
  the first failing binary, that first run executed only 1929 tests; its DST leg passed.
- Between the two gate runs I changed two comments and nothing else: one clause in
  `06-runtime-view.md:40` ("that guarded step (the in-place batch)") and the wrapping of one
  sentence in `premark.rs:36-38`. The second run is on the tree that includes both.
- Red on the base and the three mutations: §7.2, §7.3. These used
  `timeout <n> cargo test --offline -p wyrd-custodian …`, the command the verify gate itself runs
  (`engine/scripts/run-verify.sh:441`), with a hard timeout, because `cargo xtask` has no way to
  run one test file.
- `engine/scripts/run-verify.sh --classify patch.diff`: `ADDED_TEST
  crates/custodian/tests/move_destination_marks.rs`; crates `chunkstore-grpc`, `custodian`, `dst`,
  `server`. The same as v5 and as the brief's dry run plus `chunkstore-grpc` (v5's tier-test
  edits). I did not run its full mode: it builds a `../wyrd-verify` worktree outside the roots I
  may write. §7.2 is the same red leg, done in the cycle worktree.
- `patch.diff` applies forward on a copy of the base's files and reproduces the worktree byte for
  byte, all 25 files (checked in scratch, `applycheck/`); `git diff --check` is clean.
  `Cargo.lock`, `deny*.toml` and every `Cargo.toml` are untouched.
- Commit hooks: the target repo configures no pre-commit hook of its own (no
  `.pre-commit-config.yaml`, no `core.hooksPath`); its formatter is `cargo fmt` and its spell
  checker `typos`, and both ran inside the gate on the final tree.
- External dependencies: the brief names `typos` and `docs-renderer`. Both were present and both
  ran in the gate. I met no dependency the brief did not name.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013 and
  #1015, so it could not measure in the last two rounds. Nothing in the patch can change that.

## 9. Manual validation (nightly tiers, not run here)

Not declared as dependencies (pdca-harness #969). They compile in this tree (the gate builds every
target). No assertion in them changed in this attempt; v5 changed phase 1 of two of them to expect
`Blocked`, which still stands:

- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`
  (docker). Phase 1 expects `Reconciled::Blocked` (`:648-653`).
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen` (docker).
  Phase 1 of both legs expects `Reconciled::Blocked` (`:945-950`, `:1516-1521`).
- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (root +
  device-mapper).

What to check: phase 1 of each scenario answers `Blocked`, the later phases still repair
(`Reconciled::Changed`) and then answer `Satisfied`; kill-reconstruct still finds its orphan on
the spare; no `repair-refused` line appears in a healthy run. New in this attempt: if a scenario
ever runs a reconstruction pass with a domain down and no spare, that pass now answers `Blocked`,
not `Satisfied`. I found no such pass by reading them.

## 10. Residuals

- **#1100**, now the wider class (§5). **#825** (settling an aborted move's pre-marks), **#682**
  (seeded DST for these interleavings): deferred, as before.
- **The destination drain fence** on the committed movers (`0016:670`): missing on the base and
  here, and with no open issue (§6).
- **An `unplaced` abort still answers `Satisfied`** while the repair is owed (§2.3). Library-only:
  the deployed loop cannot hand in a topology wider than its fleet.
- **A CAS lost to a moved root still answers `Satisfied`** (D6; the new pins leg is the same
  stale-plan class, and I did not assert its verdict). The base's standing answer for a stale plan; the iteration-4 sign-off scoped the non-certifying rule
  to a root that has not moved.
- **A `reclaiming` mark on a position the placement already names** keeps an in-place repair
  waiting until GC clears it. Named on the audit seam every pass; GC's side is #1083.
- `tests/staged_drain_status.rs`'s double still drops the deadline (§3); no write reaches it.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013 and
  #1015. It could not measure in the last two rounds for that reason, and nothing in the patch can
  change it.
- The rest of v4's list (`iteration-v4/build-notes.md` §11) stands unchanged.

## 11. Scratch

`$PDCA_SCRATCH/pdca-builder-1084-i6/`: the three rewrite scripts used on the test files
(`share_clock.py`, `segmented_doubles.py`, `segmented_fixtures.py`), `mutate.py` and its logs,
`check_rejections.py`, `base-red.log`, `item1-custodian.log`, the gate logs, a copy of v5's tree
(`v5tree/`, used to measure this attempt's delta) and saved copies of files a check replaced for
its run. No build directory of my own: every run used the worktree's `target/`. I left it in place
because these notes cite its logs; the harness reclaims it.
