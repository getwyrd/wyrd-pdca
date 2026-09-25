# Build notes — #814 v2 (staged re-place under the session fence)

Target: `getwyrd/wyrd` @ `main` = `feb1e30` (holds #813). Worktree `$PDCA_WORKTREE`.
Starting point: `results/issue_814/iteration-v1/patch.diff`, which applied cleanly on `feb1e30`,
as the brief's carry-forward directs. Everything below is what changed on top of v1, and why.

Line numbers are on the patched tree (`feb1e30` + this `patch.diff`).

## What changed from v1

### 1. C(vii): one gate per move, writes sent together (v1's defect)

v1 stamped one pre-mark for every destination, then awaited the writes one after another and
re-checked `W_repoint` against that one stamp before each. A legal 12 s first write aged the
pre-mark past `W_repoint` (10 s), so every pass refused the second write (`pre-mark-stale`)
while rewriting the first fragment, and the chunk stayed degraded for good.

Fix — `crates/custodian/src/reconstruction/staged.rs:641-682`:

- the `W_repoint` gate is checked **once**, right after the pre-mark commits and before any write
  is issued (`:654-657`);
- all of the move's writes are then issued together through `futures_util::future::join_all`
  (`:658-666`), each carrying the deadline `pre-mark stamp + staged_write_window_millis` (`:658`),
  which the D server enforces;
- every write answers before the move goes on. A hard fault fails the pass, as before. A deadline
  refusal aborts the move. An `Unknown` ("may have landed") refusal outranks a clean `NotApplied`
  one in the audit reason (`:665-682`), the same "unknown outranks clean" rule the rubric states
  for `CommitUnknownResult` against `Conflict`.

`join_all` is the write path's own fan-out (`crates/core/src/write.rs:253` uses `try_join_all`):
runtime-agnostic, polled on the calling task, no task spawned, deterministic under madsim. It
needs `futures-util.workspace = true` in `crates/custodian/Cargo.toml:31-34` (the workspace
already pins it alloc-only, `Cargo.toml:88`) and one line in `Cargo.lock`.

This matches 0016 literally: "the worker MUST NOT authorize the destination write if its own
pre-mark is older than `W_repoint`", and `W_write` "bounds only the interval after authorization"
(`0016:1339-1349`). All writes are authorized in the same instant under a fresh pre-mark, and each
then has the whole of `W_write` to land. C(v) is unchanged: a hook that moves the clock past
`W_repoint` between the pre-mark and the write still gets no write at all.

Docs brought in line with the new rule: module doc `staged.rs:19-22` and `:75-79`, the `repair`
doc `staged.rs:554-558`, the context field doc `reconstruction.rs:117-121`, the writers'
obligation in `gc.rs:255-258`, and the runtime view `06-runtime-view.md:82`.

**Rejected alternative: one pre-mark per destination, each committed just before its write.**
It also passes C(vii) with 12 s writes, and it also removes the cause (each write gets its own
fresh authorization). I rejected it on the invariant, not on size:

- The first pre-mark ages by the sum of all the write latencies before the adoption reads it.
  With the deployed numbers (`G_orphan` = `LEASE_TTL_MILLIS` = 60 s, `crates/server/src/cli.rs:78`;
  `W_write` = 30 s, `gc.rs:202`), two legal 29 s writes put the first pre-mark at ~58 s by the
  adoption, and three (an RS(k,3) chunk that lost three) at ~87 s, past `G_orphan`. GC may then
  reclaim the first destination before the adoption, the adoption loses on its pre-mark
  precondition, and the next pass does the same. That is a legal timing that never completes,
  which the brief's "Invariant to restore" rules out. With one gate and concurrent writes, every
  pre-mark is at most `W_write` plus the adoption's latency old at the adoption, inside the 0016
  sizing `G_orphan > W_repoint + W_write + δ_clock`.
- Cost, for completeness: N+1 metadata commits per move instead of 2, plus a `Vec` of
  per-destination stamps and pre-mark bytes threaded into the adoption's preconditions — roughly
  +25/−10 lines in `staged.rs`, against the +27/−20 of the change I made.

**Also rejected: one gate up front, writes still sent one after another.** It passes C(vii) at
12 s, but a legal 20 s first write leaves the second write 10 s of its 30 s window, so a 15 s
second write (legal on its own) is refused every pass. The same stall class as v1, at a larger
number.

### 2. Leg E: the two missing cases

- `a_chunk_an_untrusted_owned_entry_holds_is_kept` (`staged_repair.rs:1686-1721`): a valid
  committed part in an `Open` session names the chunk, and a corrupt `sidx:` value under a key
  naming it puts it in `StagedSet::held` (`gc.rs:1390-1391`). Kept: `Blocked`, no write, no mark,
  part byte-identical, chunk named on the audit seam. **Mutation check:** deleting the `held` arm
  (`reconstruction.rs:792-795`) turns this test red, because the committed-part path then repairs
  the chunk. That arm was the surviving mutant v1's gates reported.
- `a_degraded_chunk_with_no_usable_destination_is_kept` (`:1723-1765`): both non-survivor servers
  carry a `desired:dserver:` record. Kept: no write, no mark, obligation queued, `desired-state`
  named on the audit seam. The pass answers `Satisfied`, as a committed chunk with no free domain
  does (`Assessment::Blocked` → `repair_blocked`, `reconstruction.rs:342`, not part of the hole at
  `:488-493`). Then one desired record is removed and the next pass repairs the chunk
  (byte-checked).

**Deviation from the brief's Falsifiability line, on purpose.** It says "Leg E is a guard and is
GREEN on the base". Three of the four leg-E cases are green on `main` (run-verify output below).
The no-usable-destination case is red on `main`: there every staged chunk is kept the same way,
so the pass answers `Blocked` and nothing ever chooses or passes over a destination. The pass
answer and the `desired-state` audit line are the "no-usable-destination outcome" carry-forward
item 2 asks to cover, and asserting less would let a mutation of that arm (`Blocked` → `Staged`)
survive. For the held case I first asserted the new `untrusted-staged-record` audit reason, which
was red on `main` for that string alone. I switched it to "the chunk is named on the audit
seam", as leg G asserts. That is green on `main`, and it still catches the held-guard deletion
(re-checked after the change).

The non-`Open` case moved to RS(2,2) with one fragment lost (servers A, B, C live, D dead, E free),
so the chunk is repairable and the refusal is the session state's. It still answers `Blocked`,
and its Completing→published follow-up now expects `[0, 1, 2, 4]` and the byte-checked
fragment 3 on server 4 (`staged_repair.rs:1767-1847`).

### 3. `deferred: #825` marker

`staged.rs:632-639`, directly after the pre-mark commits, the point from which every abort leaves
a pre-mark standing. It names both residuals the brief scopes out: a pre-mark whose write never
landed staying in the ledger, and an `Unknown` write landing after GC reclaimed its pre-mark on a
destination that already held bytes. `gc.rs:273-275` also carries `(deferred: #825)` where it
describes the unsettled pre-marks.

### 4. Leg G back on `Open`

`crates/custodian/tests/staged_protection.rs:2352-2420`: the committed-part case is back on
`State::Open` (v1 had moved it to `Aborting`). The only assertion change is `Blocked` →
`Satisfied` (`:2383`) and its message. The doc comment says why: single-copy, only fragment lost,
`Unrepairable` via `emit_data_loss`, same audit seam, not a hole. The "no write" message, which
said rebuilding is "#814's, not this slice's", now says why nothing is written. v1's
`an_open_uploads_single_copy_chunk_is_unrepairable` is gone from `staged_repair.rs`, whose module
doc now points at leg G instead. The other G–J cases are v1's (message-only edits where the old
text named #814 as future work).

### 5. C(viii), and the byte check for A, C(iii), C(vii)

- `gc_reclaiming_the_destination_before_the_adoption_makes_it_lose` (`staged_repair.rs:1486-1561`):
  the destination double's `on_stored` hook swaps `orphan:<P_new>` to `into_reclaiming()` (same
  stamp, same event) with an exact-value CAS, then deletes the fragment, in GC's order. Asserts
  the write arrived, no adoption, part byte-identical, the `reclaiming` bytes exactly as written,
  obligation queued. Green on v1's code, as the brief expected (v1's adoption already pins the
  pre-mark, `staged.rs:703-706`).
- `a_slow_but_legal_write_never_stops_a_multi_fragment_move` (`:1404-1484`): RS(2,2), fragments 2
  and 3 lost, servers 4 and 5 free, every stored write advances the `ManualClock` 12 s
  (`SLOW_WRITE_MILLIS`, with compile-time checks `> W_REPOINT_MILLIS` and `2× < W_WRITE_MILLIS`).
  At most two passes; asserts the obligation drained, the placement names {4, 5}, both fragments
  byte-checked, both vacated positions marked, both pre-marks consumed. It completes in one pass.

**A defect in v1's byte check, found and fixed here.** v1's `holds_intact` compared the payload
with the seeded shard, but the fixture data was 64 bytes. `erasure::encode` pads each data shard
to a 64-byte multiple (`crates/core/src/erasure.rs:16`, `:79-82`), so data shard 1 was all zeros
and, under RS(2,1), the parity shard was **equal** to shard 0. A rebuild writing shard 0's payload
under index 2 passed leg A. I showed it by mutation (write `shards[(i + 1) % n]`): with the old
data only C(iii) and C(vii) went red, and leg A stayed green. Fix: `DATA` is now 218 bytes, so the
second data shard carries real bytes (`staged_repair.rs:106-113`), and `Fixture::new` asserts no
two encoded shards are equal (`:546-551`). With that, the same mutation turns 8 tests red, leg A
included.

## Self-tests from the brief (each run as a mutation, then reverted and `cmp`-checked)

| mutation | expected | observed |
|---|---|---|
| v1's `staged.rs` (gate re-checked per write) | passes A–F, fails C(vii) | only C(vii) red (22/23 green) |
| adoption without `require(orphan:<P_new> == pre-mark)` | passes B, fails C(viii) | only C(viii) red |
| wrong payload, right header | fails A | A red (8 red in all) after the `DATA` fix; A green before it |
| `held` guard deleted | fails the leg-E held case | only the held case red (before and after the audit-assertion change) |

## Refute-your-own-test (forced)

**(a) Genuine red?** Yes. `PDCA_BUNDLE=results/issue_814 ./engine/scripts/run-verify.sh` on the
final `patch.diff`: GREEN with the fix, 23/23 passed; RED with the production change reverted and
the test kept, 20 failed and 3 passed — `run-verify.sh: PASS — red without the fix, green with it`.
The 3 green on `main` are the leg-E guards (owned-entry-only, non-`Open`, held), green there by
design. The 20 red are A, all of B, C(i)–(viii), D, the in-place and duplicate-drain cases, the
two withheld cases and the no-usable-destination case, each failing by assertion (e.g.
`left: Blocked, right: Changed` for A). The test compiles on `main`: it names only symbols
already there, including `ReconstructionContext::{clock, staged_write_window_millis}`. Separately,
C(vii) is red against v1's `staged.rs` (at the "two passes must drain the obligation" assertion,
`staged_repair.rs:1451`).

**(b) Production path?** Yes. Every case calls the production `reconcile_step`
(`staged_repair.rs:602-630`) with a real `ReconstructionContext` whose clock is a
`wyrd_testkit::ManualClock`; the code under test is `reconstruction::staged::{assess, repair}`,
unmodified. Only the stores are doubles: an in-memory `MetadataStore` that applies preconditions
atomically, and a `ChunkStore` that enforces the write deadline through the production
`WriteDeadlineExpired::if_elapsed` / `if_publication_unverified` (`:325-356`), on the same
`ManualClock` the context reads. The session fence is a CAS the test applies itself, because
Abort and Complete do not exist yet (brief, Production reach).

**(c) Fixture includes the fault?** Yes. Each fixture really loses the fragment (it is never
written to the disk double). The fence, the rewrite, the drain and GC's reclaim are really
committed mid-move through hooks that assert they fired (`fenced` / `replaced` / `reclaimed` =
`Committed`). The slow write really advances the shared clock, and the double really enforces the
deadline. C(viii) asserts the destination received the write, so it cannot pass on a move that
writes nothing. The fixture now checks its own shards are pairwise distinct, so the byte check
cannot go blind again.

## DST (leg F)

v1's property 17, unchanged: `staged_replace_under_the_fence_strands_nothing` and
`staged_replace_reaches_every_point_of_the_fence` in `crates/dst/tests/custodian.rs` (still
`#![cfg(madsim)]`), plus its entry in the regression-seed block. Both are `#[madsim::test]` via
`dst_campaign_test!` (`crates/dst/src/lib.rs:69-78`), and `cargo xtask ci` → `run_dst` runs them
with `MADSIM_TEST_NUM` = `DST_SEEDS` = **50 seeds** (`xtask/src/main.rs:1573`, `:1607`). The
campaign test draws one fence offset per seed from 25 (0–24 ms, +0.5 ms). The coverage test walks
all 25 offsets on every seed (50 × 25 = 1 250 runs) and asserts each of the four landings — before
the pre-mark, pre-mark→write, write→adoption, after the adoption — is reached, and that some runs
adopt and some do not. The DST D-server double enforces the deadline the same way
(`DeadlineDServer`). Both passed in the `ci` runs below.

Judgment call, recorded: the DST move is single-fragment, so it does not exercise the new
two-write fan-out. The rubric asks for seeded DST coverage of a new concurrent path; the fan-out
is covered deterministically in-process by C(vii) and C(iii) (two writes each) instead. Extending
the DST fixture to RS(2,2) would need a fifth server and domain in its shared `servers()` /
`four_domains()` helpers, which the brief's "do not grow the patch beyond what 1–5 need" argues
against. If review wants it, that is the place to add it.

## Gates run locally

- `./engine/xtask.sh ci` (the C4-ci gate's own command) on the tree before the last one-line
  test-assertion change: `xtask ci: all checks passed`, exit 0. That covers typos, the docs lint
  and render (the `typos` and `docs-renderer` external dependencies are present), the gitlink and
  unsafe guards, fmt, clippy, build, the workspace tests, cargo-machete, cargo-deny, the statics
  and deploy guards, and the DST clippy and tests under `--cfg madsim` (both staged-replace
  properties `ok`).
- After that change: `cargo fmt --all -- --check` clean, `cargo clippy -p wyrd-custodian
  --all-targets` with no warnings, `staged_repair` 23/23, and the run-verify pass above. A second
  full `./engine/xtask.sh ci` on the final tree: `xtask ci: all checks passed`, exit 0, both
  staged-replace DST properties `ok`.
- The formatter the target's commit runs (`cargo fmt`) has been applied to every touched file.

## Out of scope, left as the brief says

The committed repair path (`repair_chunk`) is unchanged in behaviour; one degraded chunk per part
per pass is accepted with a comment (`reconstruction.rs:429-435`); settling pre-marks is #825
(marker above); `EcScheme::None` → Unrepairable; `seg:` repair (#777); rebalance and restore
(#809, #810); the upload-side drain fence (#657); no edits to `multipart.rs`, 0016 or any ADR.

Size: `patch.diff` is ~216 KB, near v1's 202 KB (the brief's Ordering note accepts this). The
additions over v1 are the `join_all` change, the four new test cases, the `DATA` / distinct-shard
fix, and the docs lines listed above.
