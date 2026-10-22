# Adversarial review — issue #768 (last-modified-never-decreases)

The red→green holds on the production path. The patch implements the brief's rule exactly. The main refutation is about whether the fix does its job, not about the code. On this base every S3 surface shows `Last-Modified` in whole seconds, so the `+1 ms` bump almost never reaches the wire. In the issue's own skew scenario, the value clients see goes from "one second older" (before) to "identical" (after). It never shows as newer.

- NEEDS-HUMAN [human] — **The fix does not reach the wire, and for some sync rules it makes the issue's scenario worse.** `crates/core/src/metadata.rs:2272` commits `max(t, p + 1 ms)`. Both renderers truncate to seconds: `crates/gateway-s3/src/lib.rs:637` (listing, always `.000Z`) and `:3264` (header). The base already documents this at `docs/design/architecture/05-building-block-view.md:195-198`: #767 has landed. So the brief's statement "a listing on `main` renders milliseconds … `.751Z` after `.750Z`" is out of date here, and the bump is visible on **no** S3 surface unless `p` ends in `.999`.

  **Reproduced** in a scratch copy, on the real `S3Gateway` over signed HTTP:
  1. Seed `stamps/doc` with `p` = 2 s ahead of the wall clock, at `.500`.
  2. Read HEAD and ListObjectsV2.
  3. Overwrite through `Gateway::put_object` with different bytes of the same size.
  4. Read both surfaces again.

  | | HEAD and ListObjectsV2 | GET with `If-Modified-Since: <old header>` |
  |---|---|---|
  | With the fix | `08:31:29 GMT` / `…08:31:29.000Z` before **and** after | `304 Not Modified` |
  | Fix reverted | `08:31:40` before → `08:31:39` after | `304 Not Modified` |

  What this means:
  - Comparators that need "newer": the `If-Modified-Since` check at `gateway-s3/src/lib.rs:2862`, HTTP caches, and a "source newer than destination" rule. They miss the new content both before and after the fix, so the fix changes nothing for them.
  - Comparators that react to "different": aws-cli's documented download rule ("ignore same-sized items unless the local version is newer than the S3 version"; with `--exact-timestamps`, unless the timestamps differ). Before the fix they saw the local copy at `:40` as newer than S3's `:39` and downloaded again. After the fix they see `:40 == :40` and skip silently. That is the exact failure the Goal names.

  The tests cannot catch this. They read raw milliseconds through `head_object` (`crates/server/tests/last_modified_monotonic.rs:142-147`), and the brief says so on purpose. So they pass while the property users rely on fails. The Plan-review claim "the reported gateway defect is fully fixed" does not hold on this base.

  Fixing it means changing the maintainer-decided rule, for example bumping to the next whole second (`max(t, (p / 1000 + 1) * 1000)`). That rule has its own cost: a burst of same-second overwrites would build a lead of seconds, not milliseconds. Or the human accepts "never older on the wire, but often equal" and records that limit. Either way it is a design call.

- NEEDS-HUMAN [human] — **One bad clock reading now sticks to a file permanently. Before the fix the next good write healed it.** The rule has no cap (`crates/core/src/metadata.rs:2272`). The patch's own leg (1) shows a prior an hour ahead is carried forward exactly (`last_modified_monotonic.rs:136-151`).

  Concrete case: a gateway boots with its clock stepped to 2036 (a VM restored before NTP syncs, or a bad RTC) and serves one PUT of key `k`. From then on, every PUT of `k` from any accurate gateway records 2036 + n ms. A GET with `If-Unmodified-Since: <now>` returns `412` for that key until 2036 (`crates/gateway-s3/src/lib.rs:2847`). Before the fix, the next accurate overwrite healed the stamp. Now only delete + recreate clears it.

  The brief says the lead is "bounded by the largest clock skew in the fleet". That is not a bound: the brief itself says nothing limits skew on this path (ADR-0024 sets no limit here, and option 2 was rejected). Whether to cap the carry-forward (for example, ignore a prior that is more than X ahead of the publisher's clock, and log it) is a design decision.

- NEEDS-HUMAN [human] — **The red C4-ci gate is not caused by this patch.** It fails only at `cargo deny … check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on `rustls 0.21.12` (`Cargo.lock:302`, pulled in through `tikv-client 0.4.0`; see `gate-logs/C4-ci.log:3530-3608`). The patch does not touch `Cargo.lock`. fmt, clippy, build and the full test run passed, including all five new tests (`C4-ci.log:2574-2581`, `:5633-5640`). This needs a waiver, or a dependency fix on `main`. It is not a builder iteration.

- **C4-diff-cov "executed 0 tests" is a harness misdiagnosis, not a property of the test.** The coverage worktree (`/home/eddie/wyrd/wyrd-cov`) failed to compile the test: `E0061`, it expects a 4-argument `put_object_streaming` (`gate-logs/C4-diff-cov.log`). This target has the 5-argument signature (`crates/gateway-core/src/lib.rs:330-337`), so that worktree is on an older base. The script then reported "compiled out". Coverage of the diff is not in doubt: the core leg calls both committers directly, and C5 caught 3 of 3 viable mutants.

- **Attempts that did not refute the fix:**
  - **Red→green, re-run locally.** On a scratch copy of `$PDCA_TARGET` with `metadata.rs` reverted: 3 fail (A1, A2, A5) and 2 pass (A3, A4 are guards). With the fix: 5 of 5 pass. Same panics and lines as `gate-logs/C4-verify.log`.
  - **Production path.** Both legs go through `Gateway` → `commit_written` (`crates/server/src/lib.rs:309-316`) → `write::commit_overwrite` → the leased committer (`metadata.rs:2330`). This is not a copy of the logic.
  - **Concurrency.** The stamp comes from the same `prior` the batch CAS-requires (compare-and-swap on the record's exact bytes), so a racing overwrite cannot order against a stale generation.
  - **Paths that could skip the helper.** Every S3 PUT enters `put_object_streaming` (`gateway-s3/src/lib.rs:1881-1908`). The CLI put is create-only (`cli.rs:2267`). Repairs keep the stamp through `..prior.clone()` (`metadata.rs:2164-2172`, `custodian/src/backfill.rs:239-246`).
  - **Edge cases.** `None` and saturation are pinned. The assertions are exact (`==`), not `>=`.
  - **Delete-then-recreate.** It starts again from the publisher's clock (`crates/core/src/write.rs:284`). That is outside the brief's overwrite scope, so I did not raise it.
<!-- pdca:leaf-complete -->
