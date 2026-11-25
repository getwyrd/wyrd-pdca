## Summary
**User impact:** if the metadata store loses track of whether a write went
through (a timeout or dropped connection at the wrong moment), completing a
multipart upload always fails with an "outcome unknown" error. That happens
even when the object was in fact published, so the client is told it failed
when it didn't. It also happens when the upload could simply have carried on,
so the client has to start the Complete again for nothing.

This PR makes Complete check the upload's recorded state after such a write
and carry on, or report success, whenever that state proves what happened. When
the state can't prove it, Complete still returns the "outcome unknown" error
and never guesses.

**Stacked PR:** this builds on #1050 and #1059 (the earlier Complete slices),
which are not merged yet. Review and merge it after them. Until then the diff
against `main` includes their changes.

## What to look at
The new logic is in the multipart Complete code: what it does after a write
whose result is unknown. There are three outcomes to check:

- **Carry on:** the write didn't land, or landed in a way only this request
  could cause. Complete resumes or retries, and publishes once.
- **Report success:** the final publish step landed. Complete answers
  "published" once, with no duplicate side effects.
- **Stay in doubt:** a write might still land later, a re-read fails, or the
  recorded state could have come from a competing request. Complete returns the
  "outcome unknown" error and writes nothing more.

To try it, run `cargo test -p wyrd-core --test multipart_complete`. The test
store wrapper can now apply a write and report "unknown", hold a write back and
apply it later, or fail a read. Each case above has its own test.

## Root cause
Every `CommitUnknownResult` from a commit made by Complete went straight to
`CompleteError::InDoubt`, with no re-read of the session. The multipart design
(0016:942-964) calls for re-reading the session and recovering from it.

## Fix
After an unknown outcome, Complete re-reads `mpu:<id>` and acts on what it
finds. It tracks two kinds of doubt, the same way Abort (#909) does:

- **Held** (`may_still_commit: true`): the first such outcome is kept. It is
  cleared only by a read that shows the session has moved off the bytes the
  held batch requires. An unchanged re-read does not count, because the batch
  may still land (`crates/traits/src/lib.rs:240-247`).
- **Unsettled** (`may_still_commit: false`): cleared by the next read that
  succeeds.

While either kind is open, any stop ends in `InDoubt`, never in an answer.

**Initial fence.**
- The session is still `Open@E` and the outcome was not held: the fence did not
  land, so it is sent again, within the existing round limit.
- The session is `Completing@E+1` with this fence's exact bytes: the request
  stays in doubt. A `Completing` record names no request, so a rival Complete
  that read the same `Open@E` with the same timestamp produces identical bytes.
  Those bytes do not prove which request owns the fence.
- The session is `Open` again with an attempt charged since (the reaper rolled
  it back): Complete answers `OperationAborted` so the client retries with a
  fresh timestamp. The original fence timestamp is never reused.

**Segments.** Complete resumes from the recorded `segments_written` cursor at
the same epoch.

**Flip.**
- The session is `Completed` with this request's bytes: Complete answers
  `Published` with the recorded publication.
- The session is `Open`: Complete answers `OperationAborted`.

**Records that do not decode.** A session record that does not decode settles
no doubt. It can't show what landed before it was damaged.

The module docs and `docs/design/architecture/05-building-block-view.md` now
state these rules. Error types and record formats are unchanged.

**Known limitation, left for a follow-up.** In one fence-phase case the error
over-reports `may_still_commit: true`. A held first fence, followed by a re-read
that finds the fence's own bytes, returns `InDoubt` before the held doubt is
re-checked. The field then says the fence may still land when it no longer can.
This is the cautious direction: it never claims a result. But the field and
module docs promise it is exact, and the gateway's same-request recovery (#670)
will read this flag. #670 should not rely on it unchecked until this is fixed in
the code or the docs are narrowed.

## Verification
Line numbers are on this branch: `main` plus #1050, #1059 and this PR.

- **Claim:** after an unknown outcome, Complete re-reads the session and answers
  only from a state it can attribute to itself.
  **Checked:** `crates/core/src/multipart/complete.rs:186-200` (the rules),
  `:888-897` (fence re-read: decode first, then settle),
  `:960-965` (matching fence bytes end in doubt),
  `:1500-1508` and `:1545-1559` (flip re-read and `Published` answer),
  `:1830-1915` (`Doubt`: `holds`, `settle`, `in_doubt`, `end`).
- **Claim:** a commit that may still land is never treated as "did not land" on
  an unchanged re-read.
  **Checked:** the store contract at `crates/traits/src/lib.rs:240-247`, and the
  `may_still_commit` field doc at `complete.rs:525-531`.
- **Claim:** the test store behaves like a real store when it holds a batch. A
  held batch is applied later through the real redb store's `commit`, so it
  re-checks its preconditions.
  **Checked:** `crates/core/tests/multipart_complete.rs:572-575` and `:616-627`.
- **Tests:** `crates/core/tests/multipart_complete.rs`, 85 tests, all passing:
  - G1, flip landed, answered unknown, published once: `:5446`
  - G2, held flip ends in doubt, then lands once when released: `:5584`
  - G3, failed re-read is in doubt: `:5647`, `:6419`
  - G3, undecodable record is in doubt: `:6489`, `:6544`
  - G4, unknown segment write resumes from the cursor: `:5745`
  - G5, matching fence bytes never prove ownership (two-rival runs): `:5849`
  - G6, unapplied fence is retried: `:5994`
  - G6, session rolled back to Open is never re-fenced: `:6248`
  - extra legs for each held-doubt rule: `:6062`-`:7065`
- **Fails before the fix:** with the new tests run against the old
  `complete.rs`, 17 of the new tests fail. The other 6 expect the in-doubt
  error, which the old code always returned. They guard against recovering too
  eagerly, and hand mutations show they bite: treating matching fence bytes as
  ownership turns G5 red, and each torn-record fix reverted alone turns its test
  red.
- **Mutation testing:** `cargo mutants --in-diff` found 27 mutants: 21 caught,
  6 unviable (`Default` stubs for types with no `Default`), none missed.
- **Full gate:** `cargo xtask ci` passes (fmt, clippy, build, tests including
  DST, deny, conformance, statics). `typos` is clean.

Fixes #1010
