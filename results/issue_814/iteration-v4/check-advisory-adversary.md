# Adversarial review — issue #814 (iteration 4)

I tried to refute this in three ways. First, I re-ran the patch's tests in a scratch copy of `$PDCA_TARGET`: 36 passed in `staged_protection` and 29 in `staged_repair`. Second, I deleted each commit precondition in `staged::repair` one at a time and checked whether some test went red. Third, I checked the gate evidence. The production code held up. I found one test gap that the three new tests don't close.

## Findings

- NEEDS-HUMAN [human] — **Both of the adoption's pins on the vacated position's mark can be deleted with every test still green.** This is the same kind of gap iteration 3's sign-off asked to close for the pre-mark batch.
  - The pins are `require_absent` at `crates/custodian/src/reconstruction/staged.rs:718` and `require(key, current)` at `crates/custodian/src/reconstruction/staged.rs:721`.
  - I removed each one in turn and ran `staged_repair` and `staged_protection`: 29/29 and 36/36 passed both times.
  - The DST sweep can't catch them either. It only fences the session (`crates/dst/tests/custodian.rs:4816-4833`) and never touches the vacated mark.
  - **Concrete failing case.** The pass reads the vacated mark `orphan:3:<chunk>:2` in `assess` (`staged.rs:296-307`). GC then swaps that mark to `reclaiming` before the adoption commits. The production code is correct here: the adoption loses and GC's mark survives. Without the pin, the adoption commits and overwrites GC's `reclaiming` mark with `legacy(now)`. That breaks "no writer replaces a reclaiming mark" (`staged.rs:188-191`). The same missing pin would also let the adoption overwrite a mark that became unreadable after `assess` (ADR-0045, leg C(vi)'s rule).
  - I wrote a throwaway probe to confirm this. It uses `Fixture::standard` and an `after_read_of(fx.mark_key(3, 2))` hook that commits GC's `into_reclaiming()` swap. It has two cases: a seeded structured mark, and no mark where another event marks the position first. It asserts the `reclaiming` mark is byte-identical afterwards. The probe passes on the patch. It fails when the pin at `:721` is removed (seeded case) and when the pin at `:718` is removed (no-mark case).
  - Tagged `[human]`, not `[impl]`: iteration 3's sign-off said "Do not grow the patch beyond the three tests." Adding this roughly 40-line test is your call. The production behaviour is already right, so this doesn't block acceptance. It only guards against a future regression.

## Refutation attempts that failed

- **The three iteration-3 tests are real.** Deleting each guarded precondition turns exactly the intended test red:
  - pin on the destination's mark (`staged.rs:625` and `:624`) → `gc_reclaiming_the_destination_before_the_premark_writes_nothing`, which covers both arms;
  - `require(part)` (`:620`) → `a_part_record_rewritten_before_the_premark_writes_nothing`;
  - `require_absent(desired)` (`:628`) → `a_drain_recorded_before_the_premark_writes_nothing`.
- **Every other pin in the pre-mark and adoption batches is tested.** Deleting any of these turns exactly one test red: `:619`, `:700`, `:701`, `:707`, `:709`. The same holds for the `W_repoint` gate (`:657`), for overwriting a `reclaiming` vacated mark (`:723`), and for treating an unverified write as success (`:676`). Deleting the obligation removal (`:703`) turns 6 tests red.
- **Red→green evidence.** `gate-logs/C4-verify.log`: on the base, 26 tests fail by assertion. The 3 that pass are leg E's guards, which pass on the base by design. The log's "29 test(s) ran red" summary line overstates this. That miscount is a known harness issue, not a flaw in the fix.
- **C5's 4 surviving mutants (`staged.rs:766-769`, `&&`→`||` in `repointed_part`) look equivalent, not a missing test.** The decoder accepts only canonical bytes, and there `chunks` is the first field (see the spelling at `crates/custodian/tests/staged_repair.rs:456-458`). So the first match at `staged.rs:756-759` is always the real chunk list. The splice therefore can't change `len`, `digest`, `committed_at_millis` or `session_epoch`, and the other comparisons can never be false.
- **Why T4 fails.** It has one blocking finding: the batch operation budget at `staged.rs:704`. Iteration 3's sign-off already overrode that exact finding, so it is not new. It is the only gating red in `check-gates.json`.
- **Retried moves rewriting a position.** A position left behind by an earlier aborted attempt gets rewritten on retry. `FsChunkStore` publishes by atomic rename, last writer wins (`crates/chunkstore-fs/src/lib.rs:303-311`), so the retry never fails there.
- **Window wiring.** The only production `ReconstructionContext` passes `W_WRITE_MILLIS` (`crates/server/src/custodian.rs:572`). Every `staged_write_window_millis: 0` is in a test that never reaches the staged path.
- **DST coverage.** The new DST cases ran under `--cfg madsim` in C4-ci and passed (`gate-logs/C4-ci.log:3654`, `:3663`).
