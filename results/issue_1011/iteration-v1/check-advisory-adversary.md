# Adversarial review — #1011 (Complete refuses part sets past the value ceiling)

**Verdict: I could not refute the fix.** The check measures the same bytes the flip writes, it runs both before and after the fence, and the tests go red for the right reasons. What I found is one test gap and three problems with the gate evidence. None of them shows the code is wrong.

## Findings

- NEEDS-HUMAN [impl] — **No test pins the zero-commit refusal for the unnamed set.** `crates/core/src/multipart/complete.rs:1140` checks both obligations before the fence, but every zero-commit leg (I2 at `crates/core/tests/multipart_complete_value_ceiling.rs:501`, I3-past at `:578`) overflows the *published* `retire:records:` set. The only leg that overflows the *unnamed* `retire:bytes:` set is I4 (`:628`), and that one overflows after the fence. I ran a mutant where the check before the fence measures only the published set (the check after the fence still measures both). All 8 tests still passed. Concrete failing case for that mutant: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no part planted. Correct code refuses `SessionTooLarge { bytes: 100_005 }` with zero commits. The mutant fences, refuses after the fence, and releases: 2 commits and the session ends at `Open@2`. That breaks the brief's invariant that a Complete outside the range "refuses before it writes anything". Fix: add that leg and assert `take_sent().is_empty()` and that the session bytes are unchanged, as I3-past does.

- NEEDS-HUMAN [human] — **Diff coverage was never measured, and all green evidence is on a base that is not `origin/main`.** `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main — the bundle is stale". `check-gates.json` still says `overall: pass` because that row does not gate. C4-ci, C4-verify and C5 all ran on the pre-fix base `b74afe3`. The brief says this bundle conflicts with #1010 around the fence (`complete.rs:920-1036`, the code this patch edits). A human should decide which case this is: a stack whose parents (#1008–#1010) have not landed yet (expected), or a base that has gone stale. If it is stale, rebase and re-run the C4 gates before sign-off.

- NEEDS-HUMAN [human] — **The gating review looks too fast to have run.** `gate-logs/T4-batch-review.log:10` reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 19.03 s, for three model passes over an 80 KB diff. Zero findings of any kind, not even noise to drop, in 19 s suggests a cached or empty run. Someone should open `review-batch.md` and confirm the three passes actually reviewed this diff before treating T4 as green.

- Gate summary is wrong, but the evidence holds (no action on the fix): `gate-logs/C4-verify.log:51` says "8 test(s) ran red", while `:48` shows 3 failed and 5 passed on the reverted base. The 3 that failed are the ones the brief predicts (I2, I3-past, I4). The 5 that passed are controls (the wrapper probe, I1, I3-inside, I3-at, I4-control), which should pass on the base. Only the count in the summary line is wrong.

## What I tried that did not break it

- **Re-ran the proof.** I built a scratch copy of `$PDCA_TARGET` and ran `cargo test -p wyrd-core --test multipart_complete_value_ceiling`: 8/8 pass. The red leg in `C4-verify.log:20-48` fails on the production path: the base's committer refuses the flip (`Publication(ValueOverCeiling { phase: Flip, … })`, `crates/core/src/staged_publication.rs:747-757`) instead of answering `SessionTooLarge`. That is the defect the brief describes. The test is not checking a copy of production code.
- **The brief's three demonstrated reds, reproduced by mutation:**
  - check only before the fence → I4 fails;
  - check only after the fence → I2 and I3-past fail on "zero commits";
  - bare set measured (value length − 10) → I2, I3-past and I4 fail;
  - published set only → I4 fails.

  The SELF-TEST claim holds. C5 also caught the `>`→`>=` boundary mutant, through the at-ceiling leg (`:560`).
- **Measure vs. write drift.** `past_ceiling` (`complete.rs:1194`) and the flip (`complete.rs:1848`) both call `RetireObligation::value()` (`crates/core/src/multipart.rs:4382-4384`), and the flip installs the exact obligations that validation measured (`HeldFence.retired`, `complete.rs:1024`, `:1836-1837`). The check cannot drift from the stored bytes.
- **Epoch / key drift.** Both validations pass `placed.epoch()` (`complete.rs:961`, `:1011`). That is E+1, the same value as `HeldFence::epoch()` (`:429-431`, `:503-506`), which the removed `FlipPlan` code used. I1 checks the obligation keys at epoch 1.
- **A false refusal before the fence.** While a session is `Open` its frozen set only grows (a part number is replaced, never removed), and the named list is fixed. So a set already too large at the first read cannot fit later. The early refusal is sound.
- **Retry loop.** After a refusal behind the fence, a retry sees the larger set before the fence and is refused with zero commits, so it does not fence/release forever. The attempt is given back (I4 checks this).
- **Other values in the flip.** The root inode is checked by the committer (`staged_publication.rs:910-916`). The `Completed` record is bounded at Create (#894). The two part-set values (≤100 KB each) are far inside the 5 MB batch envelope (`multipart.rs:5490`, `:5507`). The patch adds no new way to cross a limit.
- **"Widest set inside 1–10,000 is 58,909 bytes" (`multipart.rs:3341-3346`).** The most runs possible is 5,000 non-adjacent singletons, which means all odds or all evens. The evens have wider digits and include 10,000. The claim holds, and the value is far under the ceiling either way.
- **Rubric surfaces.** No clock read, `static`, crate root or persisted encoding is added (`value()` is the same `metadata::encode`). The living architecture doc and the `SessionTooLarge` doc are updated in the same diff (docs currency).

<!-- pdca:leaf-complete -->
