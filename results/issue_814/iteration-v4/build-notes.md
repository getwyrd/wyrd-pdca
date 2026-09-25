# Build notes — #814 v4 (staged re-place under the session fence)

Target: `getwyrd/wyrd` @ `main` = `feb1e30` (holds #813). Worktree `$PDCA_WORKTREE`.
Starting point: `results/issue_814/iteration-v3/patch.diff`, applied cleanly on `feb1e30`.

The Iteration 3 sign-off accepted v3's production code ("the adversary could not break it, and
all correctness gates pass") and asked for three tests, one per pre-mark precondition that could be
deleted with every test still green. It said not to grow the patch beyond them. So v4 is v3 plus
three test functions in `crates/custodian/tests/staged_repair.rs` and a matching update to that
file's leg list. **No production file changed**: every line where `patch.diff` differs from v3's
is inside `staged_repair.rs` (checked by diffing the two patches; the only differences outside that
file's body are its blob hash and hunk header).

Line numbers are on the patched tree (`feb1e30` + this `patch.diff`).

## The three tests

All three use the same hook point the sign-off named: `after_read_of(mark_key(2, 2))` on
`Fixture::standard`. That read is `position()` reading the destination mark
(`crates/custodian/src/reconstruction/staged.rs:534-555`, called from `choose_destinations` at
`:422`). It comes after `candidate()` has read `desired:dserver:2` (`:519`) and after the part
record was read, and before the pre-mark batch commits (`:616-633`). So the hook lands exactly in
the gap the pre-mark's preconditions exist for. The existing
`a_session_fenced_before_the_premark_writes_nothing` (`staged_repair.rs:1033`) already used this
hook for the session pin (`staged.rs:619`).

### 1. GC's swap to `reclaiming` before the pre-mark (`staged.rs:624-625`)

`gc_reclaiming_the_destination_before_the_premark_writes_nothing` (`staged_repair.rs:1745-1818`).

The hook commits GC's own swap: `require(key, mark as read)` then `put(key, into_reclaiming())`,
the same shape as `Intent::record` (`crates/custodian/src/gc.rs:596-604`). Asserts: the swap
committed (so the race ran), the pass did not answer `Changed`, **no write arrived at any D
server**, the `reclaiming` mark is byte-identical to what GC wrote, the part record is
byte-identical, the obligation is still queued, and a `staged-aborted` / `pre-mark-lost` audit
event names this chunk.

Two cases in one loop, one per arm of the `match` at `staged.rs:623-626`:

- **a stamped mark** — the case the sign-off specified: a structured mark from another event
  (`OrphanMark::structured(1, "g:9:2")`) seeded at the destination before the pass; the hook
  swaps it to `reclaiming`. Pins line 625 (`require(key, current)`).
- **no mark** — the position is empty when `position()` reads it; the hook has another event mark
  it (`require_absent` + `put`) and then GC swap that mark to `reclaiming`. Pins line 624
  (`require_absent(key)`).

Why I added the second case, though the sign-off asked only for the first: it cited
`staged.rs:624-625`, which is both arms, and its stated goal was "tests for the pre-mark batch's
preconditions, each of which can be deleted today with every test still green". I checked: with
line 624's `require_absent` removed, every v3 test stayed green (mutation M2 below, where only this
new case goes red). So leaving it out would have left one of the gaps the sign-off wanted closed.
It adds one loop iteration and about ten lines, not a fourth test.

Why this matters (stated in the test's doc comment and checked against the code): once GC's swap
commits, `record_intents` deletes the fragment without reading the mark again
(`gc.rs:780-784` → `destroy`, `gc.rs:821`). Without the pin, the pre-mark overwrites the
`reclaiming` mark, the write lands, and the adoption commits, because it pins the pre-mark's own
bytes and those are still there. The part record then names a fragment GC is about to delete.
Both mutations show exactly that: the pass answers `Changed`.

### 2. The part record rewritten before the pre-mark (`staged.rs:620`)

`a_part_record_rewritten_before_the_premark_writes_nothing` (`staged_repair.rs:1061-1113`).

The hook commits a CAS rewrite of the part record (`COMMITTED_AT + 1`, the same rewrite leg B's
adoption-time case uses at `:987`). Asserts: the rewrite committed, not `Changed`, **no write
arrived**, no pre-mark at the destination, the rewritten record stands, the obligation is queued,
and `pre-mark-lost` names this chunk.

### 3. A drain recorded before the pre-mark (`staged.rs:628`)

`a_drain_recorded_before_the_premark_writes_nothing` (`staged_repair.rs:1916-1953`).

The hook records `desired:dserver:2 = draining`, as the existing adoption-time drain test does
(`:1892`). Asserts: the drain is present, not `Changed`, **no write arrived**, no pre-mark, the part
record is byte-identical, the obligation is queued, and `pre-mark-lost` names this chunk.

### Why each test also asserts that its race ran

On `main` the pass never chooses a destination, so it never writes, and every "no write arrived /
mark unchanged / obligation queued" assertion would pass there with nothing tested. Each test
therefore first asserts that its hook fired (the swap committed, the rewrite committed, the drain is
present). `position()` is never called on `main`, so that assertion is what goes red there, and in
the green leg it proves the race was actually run. Each test also requires the `pre-mark-lost`
audit event for its own chunk (one event naming both, via `audit_event_naming`), so a pass that
aborted for another reason would not satisfy it.

## Mutation proof (each precondition deleted, run through `run-verify.sh`)

Each mutation edits only the named line of `staged.rs` in a copy of the final `patch.diff`, in a
scratch bundle under `$PDCA_SCRATCH`, run with `PDCA_BUNDLE=<scratch> ./engine/scripts/run-verify.sh`.
The GREEN leg (fix applied, mutated) is the one that reports:

| id | mutation | tests red | the failing assertion |
|---|---|---|---|
| M1 | line 625: `Stamped(current) => { let _ = current.len(); batch }` (pin dropped) | **only** `gc_reclaiming_…_before_the_premark…`, 28/29 green | `a stamped mark: nothing was adopted` — `left: Changed` |
| M2 | line 624: `Absent => batch` (pin dropped) | **only** `gc_reclaiming_…_before_the_premark…`, 28/29 green | `no mark: nothing was adopted` — `left: Changed` |
| M3 | line 620: the part pin replaced by a second copy of the session pin | **only** `a_part_record_rewritten_before_the_premark…`, 28/29 green | `no write may be sent … : [(2, FragmentId { chunk: 33109, index: 2 }, Some(40000))]` |
| M4 | line 628: `.require_absent(desired_key(..))` removed | **only** `a_drain_recorded_before_the_premark…`, 28/29 green | `no write may be sent … : [(2, FragmentId { chunk: 33120, index: 2 }, Some(40000))]` |

(M1's first spelling, `Stamped(_) => batch`, did not compile: the build denies warnings and the
variant's field became dead code. The `let _ = current.len()` form keeps the field read and drops
only the precondition.)

Each mutation turns exactly one test red, and it is the new one. That confirms the sign-off's
finding (no v3 test covered these four preconditions) and that the gaps are now closed.

## Refute-your-own-test (forced)

**(a) Genuine red?** Yes. `run-verify.sh` on the final `patch.diff`: GREEN with the fix, 29/29
passed; RED with the production change reverted and the test kept, 26 failed and 3 passed —
`run-verify.sh: PASS — red without the fix, green with it (29 test(s) ran red)`. The 3 that pass on
`main` are the leg-E guards (owned-entry-only, non-`Open`, held), green there by design as in
v1–v3. All three new tests are red on `main` by assertion: the part rewrite and the drain at "was
never rewritten/recorded between the choice and the pre-mark", GC's swap at "GC never reclaimed the
destination position". Beyond the revert, each precondition was deleted on its own (M1–M4): each
deletion turns its own new test red and nothing else.

**(b) Production path?** Yes. Every case calls the production `reconcile_step` with a real
`ReconstructionContext` over the in-memory doubles. The code under test is
`reconstruction::staged::{assess, choose_destinations, position, repair}`, unmodified. The hooks do
only what a concurrent writer does through the store's commit: GC's swap is built as
`Intent::record` builds it, the part rewrite is a CAS on the stored bytes, and the drain is the
record `desired_state` reads.

**(c) Fixture includes the fault?** Yes. The concurrent write really lands between `position()`'s
read and the pre-mark commit, and each test asserts that it did before anything else. In the GC
case the destination really carries a `reclaiming` mark when the pre-mark is tried. Under M1/M2
the move really writes to that position and adopts it.

## DST (leg F)

Unchanged from v1–v3: `staged_replace_under_the_fence_strands_nothing` and
`staged_replace_reaches_every_point_of_the_fence` (`crates/dst/tests/custodian.rs:5148`, `:5154`,
still `#![cfg(madsim)]`), run by `cargo xtask ci` → `run_dst` with `MADSIM_TEST_NUM` = `DST_SEEDS` =
**50 seeds** (`xtask/src/main.rs:1573`, `:1607`). Both passed in this build's `xtask ci` run.

## Gates run locally

- `run-verify.sh` as in (a), plus the four mutation runs.
- `./engine/xtask.sh ci` (the C4-ci gate's own command) on the final worktree:
  `xtask ci: all checks passed`, exit 0. That includes typos, `lint_docs`, `render_site --check`
  (the `typos` and `docs-renderer` external dependencies are present), fmt, clippy, build,
  workspace tests (`staged_repair`: 29 tests ok), and the DST under `--cfg madsim`. The worktree's
  `git diff` was checked `cmp`-equal to this `patch.diff` after the run.
- `cargo fmt --all` applied and `cargo fmt --all -- --check` clean;
  `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` clean; `git diff --check` clean; no
  line over 100 characters in the test file.

## Carried over from v3, not changed

- The two `Blocked` → `Satisfied` answers (no usable destination, below `k` behind an outage) are
  as the sign-off confirmed.
- The two T4 blocking findings the sign-off overrode (batch operation budget for very wide
  schemes; first-reference-only repair of duplicate part references) are settled and untouched.
- The four `repointed_part` mutants v3 described as equivalent in every reachable state are still
  there. Expect C5 to report them as missed, as for v3.
- Out of scope, as the brief says: the committed repair path, one degraded chunk per part per pass
  (commented), settling pre-marks (#825, `deferred: #825` at `staged.rs:635`), `EcScheme::None` →
  Unrepairable, `seg:` repair (#777), rebalance and restore (#809, #810), the upload-side drain
  fence (#657), `multipart.rs`, 0016 and every ADR.

Size: `patch.diff` is 232,630 bytes (v3: 225,130). The 7.5 KB of growth is the three tests and
the leg-list update, all in the test file. The brief's Ordering note accepts a patch near 200 KB.
