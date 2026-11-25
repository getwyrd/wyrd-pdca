# Adversarial review — issue 1017 (625.3: Completing exits, tombstone expiry, G_orphan, B_ops)

Verdict: the fix holds. I re-ran the red→green proof, broke the production code four ways in a
scratch copy, and could not show a production defect. What I did find: one server test that is
weaker than its doc comment says, and four claims or loose ends the human has to judge.

## What I re-ran (scratch copy of `$PDCA_TARGET`)

- Green: `cargo test -p wyrd-server --test multipart_reaper_exits` passed 8 of 8. Red, with
  `crates/core/src`, `crates/custodian/src` and `crates/metadata-conformance/src` reverted to the
  base and the test kept: 6 failed, 2 passed, at the same panic lines as `gate-logs/C4-verify.log`
  (`:212`, `:843`, `:991`, `:1078`). The two legs that pass on the base are the ones the brief
  calls vacuous there: `completing_session_fenced_moments_ago_is_kept_however_old_its_parts`
  (`crates/server/tests/multipart_reaper_exits.rs:734`) and H-terminal (`:926`). The
  `check-gates.json` C4-verify text "8 test(s) ran red" means 8 tests ran in the red leg, not
  that 8 failed. Only 6 failed.
- The tests go through the production path. Every leg runs the built `wyrd custodian
  --reap-abandoned-uploads` binary as a subprocess over a real redb store
  (`multipart_reaper_exits.rs:225-235`) and checks what the store holds afterwards.

## Findings

- NEEDS-HUMAN [impl] — **The E-terminal server leg does not check that the foreign `Completed`
  session is held, though its doc says it does.** The doc at
  `crates/server/tests/multipart_reaper_exits.rs:788` says "Every one-shot names both as held for
  their clock". But the check at `:837-841` looks for `mpu:{id}` and `foreign-clock` as two
  separate substrings anywhere in stderr. `emit_foreign` (`crates/custodian/src/reaper.rs:1574-1584`)
  prints `action = "reaper-foreign-clock"` together with the key for every foreign session,
  whether or not it is held, so both substrings always appear. The non-zero exit check (`:843-847`)
  is satisfied by the `Completing` hold alone. **Failing case:** I made the `Completed` arm of the
  foreign-clock branch (`reaper.rs:975`) `return Ok(())`, so it is never held.
  `multipart_reaper_exits` still passed 8 of 8. Only the custodian tests caught it
  (`crates/custodian/src/reaper/tests.rs:2648`, `crates/custodian/tests/gc.rs:1636`). So the
  behaviour is covered, but this leg proves less than it claims. Fix: match one line that carries
  `reaper-session-held`, the `foreign-clock` hold reason and `mpu:{id}` together, or check that
  the `reaper_sessions_held` tally is 2.
- NEEDS-HUMAN [human] — **`G_ORPHAN_MILLIS` is still not read by any production code.**
  `crates/custodian/src/gc.rs:303-308` says so itself. The deployed GC gets its grace from
  `crates/server/src/custodian.rs:115` (`GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`). The only
  compile-time check there (`:124`) is `LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS`; it
  does not tie the server value to `G_ORPHAN_MILLIS`. The boundary test passes `G_ORPHAN_MILLIS`
  in directly (`crates/custodian/tests/gc.rs:1995`), so if someone changes the server's value,
  the deployed reclaim tick moves and that test stays green. `08-crosscutting-concepts.md`
  describes `G_orphan` (60 s) as the value in force. The carry-forward said to wire it or raise a
  §6 item, because the fix needs an edit in `crates/server/src` (child-2's file). Confirm that the
  §6 item exists. The one-line fix is `GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`.
- NEEDS-HUMAN [human] — **No Check evidence backs the calibration figures; verdict provisional.**
  `crates/core/src/multipart.rs:5975-5985` and `08-crosscutting-concepts.md` quote 22/44 ms on
  FoundationDB 7.3.77 and 252/373 ms on TiKV 8.5.1 (host `saic`, 2026-11-22). The only gate
  evidence is `gate-logs/C4-ci.log:2623` and `:2784`, where both calibration tests report "ok" in
  0.00 s: they skipped because no cluster was present. I had no cluster either. The carry-forward
  forbids quoting figures without the raw first-attempt output. Check that `build-notes.md` (not
  shown to me) or SUMMARY §9 contains that output. If it does not, keep the "assumed, not
  measured" wording.
- NEEDS-HUMAN [human] — **The `W_tombstone` reasoning leaves out when the window starts.** The doc
  comment at `crates/custodian/src/reaper.rs:268-277` says two minutes covers an SDK retry made
  "seconds to tens of seconds" after the answer was lost. But the window is measured from
  `PublishRequest::completed_at_millis`, which the caller reads **before** `complete_upload`
  fences the session and writes its segments (`crates/core/src/multipart/complete.rs:290-293`,
  `:1470-1474`). The longest segmented Complete (512 segments, about 11 segment batches plus up to
  `R_PUBLISH` = 3 flips, each up to 5 s) runs about 70 s. That leaves about 50 s of the window
  when the client gets its answer. **Concrete case:** a Complete takes 90 s, its answer is lost,
  and the SDK retries 40 s later. The record may already be gone, so the client gets
  `NoSuchUpload` for an object that was published. 0016 accepts that answer once the window has
  passed, and the 2-minute value was chosen at sign-off, so this is the human's call. Either way,
  the doc comment should say so. The "about 23 a minute" figure (`reaper.rs:273`) also ignores
  the fleet loop's 30 s interval and the drain that has to finish before the delete.
- NEEDS-HUMAN [human] — **A server comment is now wrong, and it is outside this slice's scope
  exception.** `crates/server/src/custodian.rs:625-627` still says "until it is [measured], every
  `Completed` multipart session keeps its record, and its admission slot". With this patch,
  `policy.tombstone()` is `Window`, so that is no longer true. The sign-off exception covers only
  the `custodian_gc.rs` fixture and the 05/06 docs. Send it to #1016, or widen the exception.

## Attacks that failed

- **Fleet-loop tombstone wiring.** I set `sessions` to `None` at
  `crates/custodian/src/reconciliation.rs:189`. Two tests went red:
  `crates/custodian/tests/gc.rs:1643` and `:1750`. The deployed path is covered, not just the
  one-shot.
- **T4's BLOCKING "retaining reaper beside an expiring reconstruction context"**
  (`retirement.rs:617`). The deployed wiring cannot produce it: `crates/server/src/custodian.rs:590`,
  `:628` and `:640` take both inputs from the same `policy`. It is a misuse risk in the library
  API only, not a production defect.
- **Negation (9): delete while a `retire:` obligation is present.** I disabled the obligation wait
  in `terminal()` (`crates/custodian/src/retirement/sessions.rs:932-939`). Both F legs went red
  (`multipart_reaper_exits.rs:990`, `:1077`), so F's evidence holds.
- **Negation (15): roll back an over-age session before aborting it.** This would mark the
  positions under `s:<id>:2`. The leg asserts `s:<id>:1` and checks the tallies
  (`multipart_reaper_exits.rs:702-722`), so it would fail.
- **Could a healthy large Complete outlast `W_completing` and be rolled back on every retry?** No.
  `MAX_ROOT_SEGMENTS` = 512 (`crates/core/src/metadata.rs:572`) and a 5 MB batch holds about 50
  segment puts, so a Complete is about 11 batches plus 3 flips, well under 15 minutes.
- **Rollback racing a live completer.** The rollback requires the session's exact bytes, so a
  segment batch, flip or release that lands first makes it lose. A late completer's batches fail
  against the bumped epoch. The DST flip-first row confirms the flip landed between the reaper's
  read and its commit, with no fallback path (`crates/dst/tests/custodian.rs`, `FlipGate`).
  `CrashAt` fails all I/O after the crash point and compares the exact boundary snapshot.
- **The new API in core** (`SessionRecord::completing_rollback`). It has to live there:
  `fence_released` is `pub(crate)` (`crates/core/src/multipart.rs:2880`) and
  `RetireObligation::attempt_segments` is private (`:4678`), so the custodian could not build
  this transition itself.
- **The C5 surviving mutant** (`crates/metadata-conformance/src/lib.rs:1651`, `+` → `-`) changes
  nothing: `built.deletes.len()` is 0 for both batch shapes.

<!-- pdca:leaf-complete -->
