# Build notes — #814 v3 (staged re-place under the session fence)

Target: `getwyrd/wyrd` @ `main` = `feb1e30` (holds #813). Worktree `$PDCA_WORKTREE`.
Starting point: `results/issue_814/iteration-v2/patch.diff`, applied cleanly on `feb1e30`. v2
passed `cargo xtask ci`, C4-verify and diff coverage; sign-off sent it back for missing tests, not
for a protocol defect. So v3 is v2 plus the tests the sign-off asked for, one comment, and nothing
else in production behaviour.

Line numbers are on the patched tree (`feb1e30` + this `patch.diff`).

## What changed from v2 (the Iteration 2 carry-forward)

### 1. A test that the move's writes are sent together, not one after another

`a_moves_writes_are_sent_together` (`crates/custodian/tests/staged_repair.rs:1541-1614`).

Why C(vii) did not already cover it: in v2's D-server double a write finished inside its first
poll, so `join_all` and a sequential loop gave the same timeline. With 12 s writes, a sequential
move still lands both writes inside `W_write` (12 + 12 < 30), so C(vii) stayed green under a
sequential regression. v2's own notes said so ("one gate up front, writes still sent one after
another ... passes C(vii) at 12 s").

What I added to the double (`staged_repair.rs:285-316`, `:357-364`): an opt-in per-server latency
(`Disk::taking`). A write judges its deadline on arrival, as before, then yields once, then moves
the shared `ManualClock` to `arrival + latency` (never backwards), then publishes and runs the
second deadline check (`if_publication_unverified`), as before. The single yield is what lets
writes sent together all arrive before any of them publishes, the way writes to separate D servers
overlap. Latency 0 (the default) takes the old path with no yield, so every existing case runs
exactly as in v2.

The case: RS(2,2), fragments 2 and 3 lost, destinations 4 and 5, each write taking 20 s
(`PARALLEL_WRITE_MILLIS`, compile-time checked `< W_WRITE_MILLIS` and `2× >= W_WRITE_MILLIS`).
Asserts, in this order: ONE pass answers `Changed`; the obligation drained; both writes arrived at
the pre-mark's instant (`[NOW, NOW]`); every write carries `NOW + W_write`; the part record names
{4, 5}; both fragments byte-checked against the writer's shards; both vacated positions marked;
both pre-marks consumed.

**Mutation proof** (replace `join_all(sent).await` at `staged.rs:669` with a `for write in sent {
write.await }` loop, run through `run-verify.sh` on a scratch bundle): only this case goes red,
25/26 green, with

```
one pass must rebuild and adopt both fragments: writes arrived at [10000, 30000, 50000, 70000]
  left: [Satisfied, Satisfied]
 right: [Changed]
```

That is the exact failure the sign-off described: every pass answers `Satisfied`, the second write
publishes at +40 s against a +30 s deadline, and the chunk stays degraded. C(vii) (12 s) stays
green under the same mutation, which confirms the new case is the one that binds concurrency.

### 2. The staged outage arm (`reconstruction.rs:913`, `+` → `-` / `*` survived)

`a_chunk_below_k_only_behind_an_outage_is_not_data_loss` (`staged_repair.rs:2006-2054`). RS(2,1)
on `[0, 1, 3]`: fragment 0 survives, server 1 is out of the fleet, fragment 2 is lost from server
3's disk. Two runs:

- server 1 reported in `ReconstructionContext::unreachable`: 1 survivor + 1 behind the outage = k,
  so `Unreachable`. Asserts `Satisfied`, obligation kept, nothing written or marked, part
  byte-identical, and NO `data-loss` audit event naming the chunk.
- server 1 not reported: `Unrepairable`. Same kept/no-write asserts, and a `data-loss` audit event
  naming the chunk IS emitted.

The numbers are chosen so both mutants fail: `1 - 1 = 0 < 2` and `1 * 1 = 1 < 2`. Mutation proof:
each of the two mutants turns only this case red (25/26 green), at the "must not raise the
data-loss signal" assertion. The fixture needed one new field, `Fixture::unreachable`
(`staged_repair.rs:542`), passed through in `Fixture::pass`. The audit check needed a helper that
matches the action and the chunk in ONE event (`audit_event_naming`, `:833`), so an unrelated
data-loss line cannot satisfy it.

### 3. The staged reading's first-reference rule (`staged.rs:212`, `||` → `&&` survived)

`a_chunk_two_parts_name_is_repaired_against_the_first` (`staged_repair.rs:2060-2083`). The
standard fixture plus a second committed part (part 2, same session) with the same bytes, so two
part records name the chunk. One pass: `Changed`, part 1 (first in key order, checked by an
`assert!` on the keys) is repointed to `[0, 1, 2]`, the fragment is byte-checked, and part 2's
record is byte-identical.

Why this is the only observable half of that mutant: with `&&`, a chunk no obligation is owed on
is no longer skipped, but `staged::assess` is only ever asked about owed chunks, so the only
visible effect is that a later part overwrites an earlier one's site (last reference instead of
first). The rule is the committed reading's own (`reconstruction.rs:674-677`, same `||`
construction). Mutation proof: only this case goes red (25/26), `left: [0, 1, 3]`.

I considered and did not add a test that pins what happens to part 2 afterwards. It stays
degraded, the obligation drains with part 1's adoption, and a later scrub re-queues it only to see
part 1 at full redundancy. That is the committed path's first-reference behaviour for a chunk two
objects name, not something this slice introduced, and pinning it in a test would pin a
limitation. Flagging it here for the human; it is not in the brief's scope.

### 4. One comment, for the T4 finding the sign-off rejected

`staged.rs:278-283`: the `EcScheme::None` arm's comment now says outright that a staged and a
committed single-copy chunk raise the same signal for the same obligation, and that recovering a
single copy is a replica-copy concern on both paths. No behaviour change. The sign-off confirmed
the finding is not a defect (the brief's Scope asks for exactly this). **The T4 gate will keep
blocking until the human records that decision** in `$PDCA_BUNDLE/review-rejected.md`. I did not
write that file: `scripts/review-branch` describes it as the human's own record. A line the human
can paste, if a re-review raises it again at the new line:

```
crates/custodian/src/reconstruction/staged.rs:283 | BUG | single-copy | Brief Scope requires a staged EcScheme::None chunk to be Unrepairable as a committed one is (reconstruction.rs:840); sign-off 2026-09-21 confirmed not a defect.
```

(`is_rejected` matches the location exactly, so adjust the line if the reviewer cites another.)

### Not changed, on purpose

- **The four `repointed_part` mutants (`staged.rs:762-766`, `&&` → `||`).** The sign-off named
  two mutant gaps, not these. They are equivalent in every reachable state: `StagedPart::record`
  is always the decode of `StagedPart::prior` (both come from the same walk read), and the
  chunk list is the first field of the canonical JSON (`{"chunks":[...],...}`), so the splice
  always lands on the real list and `len`/`digest`/`committed_at_millis`/`session_epoch` read back
  equal by construction. Killing them would need a white-box test with a `StagedPart` whose record
  disagrees with its own bytes, a state production cannot build. I left the guards in (they are
  cheap protection against a future encoding change) and did not write that test. Expect C5 to
  report 4 missed.
- **DST (leg F) stays single-fragment.** Same judgment as v2: the fan-out is covered in-process by
  C(vii), C(iii) and now the concurrency case above. Growing the DST fixture to RS(2,2) needs a
  fifth server and domain in its shared helpers. The sign-off did not ask for it.
- Production behaviour is identical to v2: the only production-file edit is the comment in item 4.

## Refute-your-own-test (forced)

**(a) Genuine red?** Yes. `PDCA_BUNDLE=results/issue_814 ./engine/scripts/run-verify.sh` on the
final `patch.diff`: GREEN with the fix, 26/26 passed; RED with the production change reverted and
the test kept, 23 failed and 3 passed — `run-verify.sh: PASS — red without the fix, green with it
(26 test(s) ran red)`. The 3 green on `main` are the leg-E guards (owned-entry-only, non-`Open`,
held), green there by design. All three new cases are red on `main` by assertion (`left: Blocked,
right: Satisfied` for the outage case, `left: Blocked, right: Changed` for the other two). Beyond
the revert, each new case was checked against the specific regression it exists for, by mutation
of the fix itself (items 1-3): each mutation turned exactly its own case red and nothing else.

**(b) Production path?** Yes. Every case calls the production `reconcile_step` with a real
`ReconstructionContext` whose clock is a `wyrd_testkit::ManualClock`; the code under test is
`reconstruction::staged::{read, assess, repair}` and `Gathered::settle`, unmodified. Only the
stores are doubles. The D-server double enforces the deadline through the production
`WriteDeadlineExpired::if_elapsed` / `if_publication_unverified` on the same clock. The new latency
model only moves that clock; it does not decide anything the production code decides.

**(c) Fixture includes the fault?** Yes. The concurrency case really takes 20 s per write on the
shared clock, and the double really refuses a publication past the deadline (that is what fails the
sequential mutant). The outage case really removes server 1 from the fleet and really lists it as
unreachable; its control run uses the same fixture without the listing. The two-parts case really
seeds a second part record naming the chunk. Every lost fragment is never written to the disk
double.

## Self-tests from the brief (carried from v2, still hold)

| mutation | expected | observed (v2) |
|---|---|---|
| v1's `staged.rs` (gate re-checked per write) | fails C(vii) only | only C(vii) red |
| adoption without `require(orphan:<P_new> == pre-mark)` | fails C(viii) | only C(viii) red |
| wrong payload, right header | fails A | A red (8 red in all) |
| `held` guard deleted | fails the leg-E held case | only the held case red |

New in v3 (this build, via `run-verify.sh` on scratch bundles):

| mutation | expected | observed |
|---|---|---|
| writes awaited one after another | fails the concurrency case | only it red, `[Satisfied, Satisfied]` |
| `settle`: `+` → `-` | fails the outage case | only it red |
| `settle`: `+` → `*` | fails the outage case | only it red |
| `read`: `||` → `&&` | fails the two-parts case | only it red |

## DST (leg F)

Unchanged from v1/v2: `staged_replace_under_the_fence_strands_nothing` and
`staged_replace_reaches_every_point_of_the_fence` in `crates/dst/tests/custodian.rs` (still
`#![cfg(madsim)]`), run by `cargo xtask ci` → `run_dst` with `MADSIM_TEST_NUM` = `DST_SEEDS` =
**50 seeds** (`xtask/src/main.rs:1573`, `:1607`). The campaign test draws one fence offset per seed
from 25 (0–24 ms, +0.5 ms); the coverage test walks all 25 offsets on every seed (50 × 25 = 1 250
runs) and asserts each of the four landings is reached and that some runs adopt and some do not.

## Gates run locally

- `run-verify.sh` as in (a).
- `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-custodian --all-targets` no warnings.
- `./engine/xtask.sh ci` (the C4-ci gate's own command) on the final tree (checked `cmp`-equal to
  this `patch.diff` afterwards): `xtask ci: all checks passed`, exit 0. That covers typos, the docs
  lint and render (the `typos` and `docs-renderer` external dependencies are present), fmt, clippy,
  build, the workspace tests (`staged_repair`: 26 tests ok), machete, deny, the statics and deploy
  guards, and the DST under `--cfg madsim` (`staged_replace_under_the_fence_strands_nothing ... ok`,
  `staged_replace_reaches_every_point_of_the_fence ... ok`).
- `cargo fmt` (the formatter the target's commit runs) applied to every touched file;
  `git diff --check` clean.

## Out of scope, left as the brief says

The committed repair path (`repair_chunk`) is unchanged in behaviour; one degraded chunk per part
per pass is accepted with a comment (`reconstruction.rs:429-434`); settling pre-marks is #825
(`staged.rs:635`, `gc.rs:275`); `EcScheme::None` → Unrepairable; `seg:` repair (#777); rebalance
and restore (#809, #810); the upload-side drain fence (#657); no edits to `multipart.rs`, 0016 or
any ADR.

Size: `patch.diff` is ~225 KB (v2 216 KB; the brief's Ordering note accepts this). The growth is
test-only: the three cases, the latency and `unreachable` hooks in the doubles, and the audit
helper.
