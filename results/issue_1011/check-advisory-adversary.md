# Adversarial review — #1011 (Complete refuses part sets past the value ceiling)

**Verdict: I could not refute the fix.** I re-ran the red→green myself, ran four mutants that each drop one guard, measured diff coverage by hand (the gate never ran it), and looked for a value the flip writes that the check misses. Nothing broke. Details:

## The evidence (re-run, not taken on trust)

- `crates/core/tests/multipart_complete_value_ceiling.rs:503,580,630,676` — **red→green reproduced.** On a scratch copy of `$PDCA_TARGET`: 9/9 pass with the fix. With `crates/core/src/multipart.rs` and `crates/core/src/multipart/complete.rs` reverted and the test kept, the same 4 legs fail as in `gate-logs/C4-verify.log`: I2, I3-past, I4 and I4-already-past. All four fail with the committer's `Publication(ValueOverCeiling { phase: Flip, … })` after a fence and release, not with `SessionTooLarge`. The test drives the production `complete_upload` over a real `RedbMetadataStore`. The wrapper only adds a ceiling refusal and a one-shot plant (`:288`), so this is not a copy of production logic.
- `crates/core/src/multipart/complete.rs:961,1011,1140,1194` — **the brief's three "demonstrated reds" hold, plus one more.** I built each mutant and ran the file:
  - check only before the fence → I4 fails;
  - check only after the fence → I2, I3-past and I4-already-past fail;
  - after-fence check measures only the published set → I4 fails;
  - check measures the bare set (value − 10 bytes) → I3-past fails, and so do I2 and both I4 legs.
  The round-1 gap (no zero-commit leg for the unnamed set) is closed by `:676`.
- The tests are not circular. Sizes are literal constants. Stored obligations are compared byte-for-byte with a spelling of the range encoding written in the test (`bare_set`), not with the codec's output. The refusals assert the exact `bytes`, the zero-commit legs assert an empty send log, and the session bytes are compared before and after.
- I4's race happens in production too. The part commit only *requires* `mpu:` and never puts it (`crates/core/src/multipart/commit.rs:1199`). So a part can land between Complete's first read and its fence without the fence conflicting, which is the window the plant hook at `:630` uses.

## The fix (inputs I tried)

- `crates/core/src/multipart/complete.rs:1837,1848` — the flip installs `fence.retired`, the same obligations validation measured. There is no second computation that could drift from the one checked. The pre-fence check uses `placed.epoch()` and `HeldFence::epoch()` is `placed.session.epoch()` (`:429-431`), so keys and values match. Only the key depends on the epoch anyway; the value is `{"parts":…}`.
- `crates/core/src/multipart/complete.rs:1195` vs `crates/core/src/staged_publication.rs:750` — both use `> MAX_VALUE_BYTES`, so no off-by-one between this check and the committer's. The at-ceiling leg (`:562`, exactly 100,000 bytes) pins it.
- The flip's other values can't cross the ceiling either. The superseded-prior `{generation}` obligation wraps a flat map capped at `MAX_ROOT_VALUE_BYTES` = 50,000 (`crates/core/src/metadata.rs:602`, `multipart.rs:5385-5397`) or a segment-group reference. The flip batch is far inside the 5 MB `MAX_BATCH_BYTES` envelope even with two obligations near 100 KB. Dropping the `// deferred: #1011` marker on `Stop::Publication` is therefore justified.
- The "widest set inside S3's range is 58,909 bytes" claim (`multipart.rs:3342-3346`, the complete.rs module doc) is correct. With 5,000 isolated runs, the i-th run can only sit at 2i−1 or 2i, so taking every even number is the maximum. Fewer, longer runs cost more slots per byte.
- After the fence, nothing can add a part: every part commit requires `Open@E`, which the fence replaced. So the frozen set the second check measures is the set the flip installs.
- Not raised: Complete has no DST suite (only `crates/core/tests/multipart_complete*.rs` call it). That predates this diff. The after-fence `SessionTooLarge` reuses the existing rejection→release path, so this is not a new destructive path under the rubric's *Test fidelity* rule.

## The gate verdicts

- `gate-logs/C4-diff-cov.log` — **the failing advisory gate hides no gap.** It failed for the third round because `patch.diff` does not apply on `origin/main`, which is expected for a slice stacked on #1008. I measured it myself with `cargo llvm-cov` over `multipart_complete_value_ceiling` + `multipart_complete`: all **55/55** instrumented added production lines run (3 in `multipart.rs`, 52 in `complete.rs`). This should not send the build back to Do again.
- `check-gates.json` C4-verify row — the summary "9 test(s) ran red" is wrong. The log shows 4 failed and 5 passed in the red run; the 5 are controls (I1, I3-inside, I3-at, I4-control, the wrapper probe) that are meant to pass on both sides. This is a harness reporting slip, not a patch defect. The red→green itself is sound.

## For sign-off (input to the open question already in the brief)

- NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:4941` — the reused `SessionTooLarge` describes "a session that does not fit", but for Complete the refusal depends on the **list**, not the session. Concrete case: parts {2} ∪ {1, 3, …, 15,871}. Naming only [2] is refused `SessionTooLarge { bytes: 100_005 }` (the I4-already-past leg, `:676`). Naming all of them publishes: {1,2,3} merges into one run, the published set is 99,999 bytes, and no part is left unnamed. So the S3 error #670 maps this to must not tell the client the upload can only be aborted. This bears on the brief's still-open choice between refusing at Complete with the reused variant and refusing out-of-range part numbers at `UploadPart`. It is not a builder fix (a new `Refusal` variant is out of scope).
<!-- pdca:leaf-complete -->
