# Adversarial review: #744, round 6

**Verdict: I could not refute the fix.** All three round-5 asks landed, and each one goes red when I break it. There is one evidence gap in the T4 gate for the human to check. I also have two small notes that don't justify a rebuild.

## What I attacked and could not break

- **The pinned constants (A, C′).** I wrote a separate Python version using only the construction text in `crates/validate/src/payload.rs:17-27`. With it I re-derived all 6 whole-payload digests (`crates/validate/tests/payload_and_sizes.rs:158-201`) and both 64-byte tail constants at 10 GiB + 1 and 1 TiB (`:477-490`). All 8 match. So the docs are exact enough to reimplement the generator, and the constants are not just the Rust code's own output fed back to itself.
- **Red→green.** `C4-verify` is UNVERIFIABLE as the brief declared in advance: the new API doesn't compile on the base (`gate-logs/C4-verify.log`). On a scratch copy of the target, `cargo test -p wyrd-validate --test payload_and_sizes` passes 26/26 in 0.22 s. These are the reds I produced myself, each run against the real `resolve`/`size_classes` path:
  - I removed `check_root_budget` from `Tuning::resolve` (`crates/validate/src/sizes.rs:286`). 3 tests fail.
  - I removed it from `Tuning::check` (`sizes.rs:321`). The hand-built zero-budget case fails (`payload_and_sizes.rs:936`).
  - I removed the upper cap (`sizes.rs:254` → `1..=u64::MAX`). 3 tests fail, including the `--max-root-value-bytes 50001` CLI cases (`payload_and_sizes.rs:1178-1191`).
  - I allowed zero (`0..=CAP`). The `flags(None, Some(0), Some(GIB))` row fails (`:889`, asserted at `:919`).
  - I made the cap exclusive (`1..CAP`). 11 tests fail.
  - I dropped the generation from the derivation (`payload.rs:128`). (A), (B) and (C′) fail.
- **Cap drift.** `MAX_ROOT_VALUE_BYTES_CAP` is compared with `wyrd_core::metadata::MAX_ROOT_VALUE_BYTES` (`payload_and_sizes.rs:1313-1317`), and the compile-time assert at `sizes.rs:78-81` breaks the build if the default ever passes the cap. The "50,000, half the 100,000-byte value limit" wording matches `crates/core/src/metadata.rs:549`, `:574` and `:576`.
- **The docs claim (round-5 ask 2), `docs/design/architecture/05-building-block-view.md:297`.**
  - The largest derived `T` is 158 × 16 MiB = 2,650,800,128 B (2.47 GiB), and the largest 64 × `C` is 1 GiB. So with a derived threshold, every accepted tuning tops out at 10 GiB + 1.
  - A given `T` must be a whole number of chunks of at least 1 MiB. At `T` = 10 GiB exactly, `T + 1` lands on 10 GiB + 1, so only a `T` above 10 GiB goes past it, as the doc says.
  - "With the threshold given, any budget from 1 to the cap is accepted" matches `payload_and_sizes.rs:772-778`.
- **The server range mirror.** The server's `s3` role accepts any chunk size in `MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE` and does not require a power of two (`crates/server/src/cli.rs:2627-2650`). So the odd chunk size the overflow test uses (`payload_and_sizes.rs:847`) is one a real deployment can have.
- **CI really ran everything.** `gate-logs/C4-ci.log` shows typos, the docs lint and render, `blackbox-guard`, and the all-features advisory step all ran (lines 11-28 and 3671-3672). None was skipped.

## Findings

- NEEDS-HUMAN [human] — **The T4 batch-review evidence is again just one summary line.** `check-gates.json:104-113` marks the gating T4 row `pass`, but `gate-logs/T4-batch-review.log:10` holds only "0 blocking, 0 recorded-rejected, 0 noise-dropped", from a 20.6 s run. A fresh three-pass review of a 119 KiB diff that finds nothing at all, not even a dropped noise item, in 20 seconds looks like a reused result rather than a review of this round's diff. Round 4's sign-off asked for per-pass evidence in `review-batch.md` or the gate log, "not just a summary line" (`brief.md:239`). I can't see `review-batch.md`. Please open it and check that it names this round's diff and shows each pass. This is a harness problem, not builder work.

## Notes (no rebuild warranted)

- **A doc comment overstates the hand-built check.** `crates/validate/src/sizes.rs:315-317` says a hand-built `Tuning` "is held to the same rule as one `Tuning::resolve` produced". That's not quite true: `check()` never compares a threshold marked `Source::Derived` against the derivation rule.
  - Probe 1: `Tuning { chunk_size: 1 MiB, max_root_value_bytes: 100, segment_threshold_bytes: 1 MiB (Derived) }.size_classes()` returns `Ok`, while `Tuning::resolve` refuses the same budget with `NoChunkRefFits`.
  - Probe 2: a "derived" 7 MiB at budget 50,000 is also accepted.

  Nothing in the shipped path builds a `Tuning` by hand: `resolve_config` goes through `Tuning::resolve` (`crates/validate/src/lib.rs:100`). So no wrong `(derived)` echo can reach a user today. Rewording the comment to "the same deployment rule" would make it true. Round 5 asked for nothing else to change, so I'm leaving this untagged.
- **Patch size.** `patch.diff` is 121,798 bytes. The round-5 sign-off said it was "already 116 KB" and asked to keep it from growing (`brief.md:252`). The growth comes from the three asks' own tests and doc text. The architecture paragraph is one line, so any edit to it puts the whole line in the diff twice: 3,460 bytes removed and 6,324 added. I'm noting it only so the size isn't a surprise at sign-off.

<!-- pdca:leaf-complete -->
