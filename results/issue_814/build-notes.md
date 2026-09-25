# Build notes — #814 v5 (staged re-place under the session fence)

Target: `getwyrd/wyrd` @ `main` = `feb1e30` (holds #813). Worktree `$PDCA_WORKTREE`.
Starting point: `results/issue_814/iteration-v4/patch.diff`, applied cleanly on `feb1e30`.

The Iteration 4 sign-off accepted v4's production code ("the adversary could not break it, every
correctness gate passes, and the previous round's three requested tests are confirmed real") and
asked for ONE test: the adoption's two pins on the VACATED position's mark
(`crates/custodian/src/reconstruction/staged.rs:718` `require_absent`, `:721`
`require(key, current)`), both deletable on v4 with every test still green. It said not to grow the
patch beyond that test. So v5 is v4 plus one test function in
`crates/custodian/tests/staged_repair.rs` and a matching update to that file's leg list.
**No production file changed**: splitting v4's and v5's `patch.diff` by `diff --git` section, the
only section that differs is `crates/custodian/tests/staged_repair.rs` (10 files in each; checked
with a script over both patches). The worktree's `git diff` is what `patch.diff` holds.

Line numbers are on the patched tree (`feb1e30` + this `patch.diff`).

## The one test

`gc_reclaiming_the_vacated_position_before_the_adoption_makes_it_lose`
(`crates/custodian/tests/staged_repair.rs:1821-1911`; the leg list at `:28-31` now names it under
C(viii)).

Shape, as the sign-off and the adversary's probe prescribed: `Fixture::standard` (RS(2,1) on
`[0, 1, 3]`, fragment 2 on server 3 lost, the rebuild goes to server 2) with an
`after_read_of(fx.mark_key(3, 2))` hook. That key is the vacated position `orphan:3:<chunk>:2`.
The pass reads it exactly once, in `assess` (`staged.rs:296-307`), to pin it at the adoption — before
`choose_destinations` (`:309`), the pre-mark batch (`:616-633`), the write (`:662-669`) and the
adoption (`:699-726`). The hook lands right there and commits GC's own swap: `require(key, mark as
read)` then `put(key, into_reclaiming())`, the shape of `Intent::record`
(`crates/custodian/src/gc.rs:596-604`). Nothing pins the vacated mark before the adoption, so the
move goes all the way: the pre-mark commits, the write lands on server 2, and the adoption is tried
— and must lose on `:718` / `:721`.

Two arms in one loop, one per arm of the `match` at `staged.rs:716-724`:

- **a stamped mark** — a structured mark from another event (`OrphanMark::structured(1, "g:9:2")`)
  seeded at the vacated position before the pass; `assess` reads it as `VacatedMark::Stamped`; the
  hook swaps it to `reclaiming`. Pins `:721` (`require(key, current)`).
- **no mark** — the position is empty when `assess` reads it (`VacatedMark::Absent`); the hook has
  another event mark it (`require_absent` + `put`) and then GC swap that mark to `reclaiming`. Pins
  `:718` (`require_absent(key)`).

Asserts, per arm: the swap committed (so the race ran — this is the assertion that goes red on the
base, where nothing reads the vacated position); **the destination received exactly one write and
holds the fragment** (so the case cannot pass on a move that writes nothing, or one the pre-mark
batch stopped); the pass did not answer `Changed`; the part record is byte-identical; the
`reclaiming` mark at the vacated position is byte-identical to what GC wrote (never overwritten with
the adoption's `legacy(now)`, `staged.rs:698`); the pre-mark on the destination still stands, fresh
at `NOW`; the obligation is still queued; the lost adoption is reported as `action = "conflict"` on
the reconstruction audit seam for this chunk (`crates/custodian/src/reconstruction.rs:1409-1416`,
reached from `:449`) — so an abort for another reason would not satisfy it; and every fragment of
the chunk on every server is named or marked.

Why the pin matters (in the test's doc comment): once GC's swap commits, `record_intents` deletes
the fragment at that position without reading the mark again, and no writer replaces a
`reclaiming` mark (`staged.rs:188-191`). Without the pin the adoption commits, overwrites GC's
decision with `legacy(now)`, and the mark no longer says what GC is doing. Both mutations below show
exactly that: the pass answers `Changed`.

Upload pairs `d8`/`d9` and chunk ids `0x8161`/`0x8162` were free in the file.

## Mutation proof (each pin deleted, run through `run-verify.sh`)

Each mutation edits only the named lines of `staged.rs` in a copy of the final `patch.diff`, in a
scratch bundle under `$PDCA_SCRATCH` (`pdca-builder-814-M5-*`, `pdca-builder-814-M6-*`), run with
`PDCA_BUNDLE=<scratch> ./engine/scripts/run-verify.sh`. The GREEN leg (fix applied, mutated) is the
one that reports:

| id | mutation | tests red | the failing assertion |
|---|---|---|---|
| M5 | `:717-719`: `VacatedMark::Absent => adopt.put(key, vacated_mark.clone())` (pin dropped) | **only** `gc_reclaiming_the_vacated_position_…`, 29/30 green | `no mark: nothing was adopted` — `left != right` failed (`Changed`) |
| M6 | `:720-722`: `VacatedMark::Stamped(current) => { let _ = current.len(); adopt.put(key, vacated_mark.clone()) }` (pin dropped) | **only** `gc_reclaiming_the_vacated_position_…`, 29/30 green | `a stamped mark: nothing was adopted` — `left != right` failed (`Changed`) |

(M6 keeps `current` read with `let _ = current.len()` for the same reason v4's M1 did: the build
denies warnings and the variant's field would otherwise become dead code.)

Each mutation turns exactly one test red, and it is the new one, on the arm that pins that line.
That confirms the sign-off's finding (no v4 test covered these two pins) and that the gap is closed.
The two arms are a loop, so under M6 the loop stops at its first arm ("a stamped mark") and under M5
the first arm passes and the second ("no mark") fails — each mutation is caught by the arm meant
for it.

## Refute-your-own-test (forced)

**(a) Genuine red?** Yes. `run-verify.sh` on the final `patch.diff` (scratch bundle
`pdca-builder-814-verify`): GREEN with the fix, 30/30 passed; RED with the production change
reverted and the test kept, 27 failed and 3 passed —
`run-verify.sh: PASS — red without the fix, green with it (30 test(s) ran red)`. The 3 that pass
on `main` are the leg-E guards (owned-entry-only, non-`Open`, held), green there by design as in
v1–v4. The new test is red on `main` by assertion, at its first check:
`a stamped mark: GC never reclaimed the vacated position before the adoption`
(`staged_repair.rs:1877`) — on the base nothing reads `orphan:3:<chunk>:2`, so the hook never
fires. Beyond the revert, each pin was deleted on its own (M5, M6): each deletion turns the new
test red on its own arm and nothing else.

**(b) Production path?** Yes. The case calls the production `reconcile_step` with a real
`ReconstructionContext` over the in-memory doubles, as every case in the file does. The code under
test is `reconstruction::staged::{assess, choose_destinations, repair}`, unmodified since v3. The
hook does only what GC does through the store's commit: an exact-value swap of the mark it read to
its `reclaiming` form, as `Intent::record` builds it.

**(c) Fixture includes the fault?** Yes. The concurrent swap really lands between `assess`'s read
of the vacated mark and the adoption's commit, and the test asserts that it did before anything
else. The move really pre-marks, really writes the fragment to server 2 (asserted: one arrival,
the fragment held) and really tries the adoption against a vacated position GC has claimed. Under
M5/M6 the adoption really commits over GC's mark.

## DST (leg F)

Unchanged from v1–v4: `staged_replace_under_the_fence_strands_nothing` and
`staged_replace_reaches_every_point_of_the_fence` (`crates/dst/tests/custodian.rs`, still
`#![cfg(madsim)]`), run by `cargo xtask ci` → `run_dst` with `MADSIM_TEST_NUM` = `DST_SEEDS` =
**50 seeds** (`xtask/src/main.rs:1573`, `:1607`).

## Gates run locally

- `run-verify.sh` as in (a), plus the two mutation runs.
- `./engine/xtask.sh ci` (the C4-ci gate's own command) on the final worktree:
  `xtask ci: all checks passed`, exit 0. That includes typos, `lint_docs`, `render_site --check`
  (the `typos` and `docs-renderer` external dependencies are present), fmt, clippy, build,
  workspace tests (`staged_repair`: 30 tests ok) and the DST under `--cfg madsim`
  (`staged_replace_under_the_fence_strands_nothing` and
  `staged_replace_reaches_every_point_of_the_fence` both ok). After the run the worktree's
  `git diff` was checked `cmp`-equal to this `patch.diff`, and the bundle's `staged_repair.rs`
  `cmp`-equal to the worktree's.
- `cargo fmt --all` applied and `cargo fmt --all -- --check` clean; `git diff --check` clean; no
  line over 100 characters in the test file (counted in characters — `awk length` reports seven
  pre-existing lines as 101 because each carries a multi-byte em dash; they are 99 characters).

## Carried over from v4, not changed

- The three v4 tests for the pre-mark batch's preconditions, and everything v3 and earlier settled:
  the two `Blocked` → `Satisfied` answers (no usable destination, below `k` behind an outage); the
  two T4 blocking findings the sign-off overrode (batch operation budget for very wide schemes;
  first-reference-only repair of duplicate part references); the size backstop.
- The four `repointed_part` mutants v3 described as equivalent in every reachable state are still
  there. Expect C5 to report them as missed, as for v3 and v4.
- Out of scope, as the brief says: the committed repair path, one degraded chunk per part per pass
  (commented), settling pre-marks (#825, `deferred: #825` at `staged.rs:635`), `EcScheme::None` →
  Unrepairable, `seg:` repair (#777), rebalance and restore (#809, #810), the upload-side drain
  fence (#657), `multipart.rs`, 0016 and every ADR.

## Self-review against the rubric

The v4→v5 delta is one test function and three doc-comment lines. No clock read is added (the
fixture's `ManualClock` is the one source, as before); no trait seam, crate root, port, API, RPC,
flag or persisted field changes, so no doc is made stale; the test drives the production pass and
its hook mirrors GC's real swap (test fidelity), and its assertions are on the stored bytes and the
D server's arrivals, not counts that could pass while the property fails (the write count is
asserted together with the fragment being held, the mark's exact bytes and the audit event).

Size: `patch.diff` is 237,412 bytes (v4: 232,630). The 4.8 KB of growth is the one test and the
leg-list update, all in the test file. The brief's Ordering note accepts a patch near 200 KB.
