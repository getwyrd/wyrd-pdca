# Build notes — issue 1010 (636.5d): Complete settles unknown commit outcomes

Base: the stacked integration tip `db1a066` (`stack-base`), which carries child-3 (#1009).
Every `path:line` below is on the patched worktree unless it says `base`.

## What the patch does

Until now `complete.rs` returned `CompleteError::InDoubt` at once on every
`CommitUnknownResult` (base `complete.rs:919-931` for the fence, `:1450-1459` for a
publication round). The patch makes both phases re-read the session and answer only from what
they can attribute to themselves (`0016:942-964`), with Abort's two kinds of doubt
(`abort.rs:259-273`, `:394-451`).

* **`Doubt`** (`crates/core/src/multipart/complete.rs:1795-1876`). *Held* = the first outcome that
  said `may_still_commit`, kept against the session bytes the batch in doubt requires; *unsettled*
  = the latest out-of-flight outcome until the next successful read. `end` / `in_doubt` give the
  in-doubt error about held, else unsettled. One difference from Abort: for a publication round
  the required bytes are not known when the outcome comes back (the committer does not say which
  of a round's batches it was), so `observe(.., None)` and the read right after pins them
  (`settle`, `complete.rs:1841-1859`). Sound because every batch before the doubtful one answered
  `Committed`, so the doubtful one required the session where they left it, and the read after it
  finds the session there (not landed) or past it (landed); the cursor and epoch never go back.
  A second held outcome re-pins the bytes but keeps the first outcome for the report (the brief's
  "first `may_still_commit: true` outcome is kept").
* **Fence phase** (`complete.rs:866-952`).
  - Unknown fence → `observe(Fence, outcome, Some(read))` (`:939`) → re-read.
  - Found the fence's own bytes while a doubt is open → `InDoubt`, nothing more written (`:947-951`).
    This is G5: a `Completing` record names no request (`multipart.rs:2276-2285`), so a rival with
    the same stamp builds the same bytes.
  - Found `Open@E` unchanged → `settle` (`:877`) and loop: the fence is sent again within
    `FENCE_ROUNDS` (G6).
  - While a held fence may still land, the pre-fence validation is skipped (`:912`), so no refusal
    can be answered from that `Open` session while our fence might still land behind it. The
    fence then settles the doubt, and validation behind it decides.
  - Every stop goes through `stop` → `doubt.end` (`:867`).
* **Publication** (`complete.rs:1423-1549`).
  - Unknown round → `observe(Flip | Publication, outcome, None)` (`:1471`), and the flip's
    `Completed` bytes are remembered (`Flip.completed`, `:1601`, `:1721`).
  - The read after it: a flip of ours found landed → `Published` with that flip's publication
    (`:1509-1515`, G1). Our fence at a cursor → `settle(own.bytes)` (`:1525`) and the next round
    resumes from that cursor via the existing `resume_from` (G4). Off the fence → the existing
    `moved()`. A failed read → in doubt (G3).
  - Unknown rounds count against `R_PUBLISH`.
  - While a held doubt stands, nothing is released: every path that used to `give_up`/`release`
    goes through `end_publication` (`:1565-1577`), and the definite-failure path checks first
    (`:1481`). So G2 ends in doubt when the rounds run out, with no release sent.
* **Docs**: new module-doc section *Unknown outcomes* (`complete.rs:185-233`), and updates to the
  `InDoubt` / `DoubtfulCommit::Fence` field docs, `fence_complete` and `complete_upload` docs and
  the `deferred: #660` note. `docs/design/architecture/05-building-block-view.md:252`, `:254`
  (docs currency: an API operation's behaviour changed). `crates/core/src/multipart.rs:127-129`,
  `:3903` drop #1010 from the "still to land" lists (it adds no writer and no row).
* **Test** (`crates/core/tests/multipart_complete.rs`): the wrapper gains `OnCommit::Hold` +
  `Logged::release_held` (`:378`, `:505`; the shape of `multipart_abort.rs:961-980`), gates
  `Reach`/`Wait`/`Then` + `Arm::OpenOnGet` (`:417`, `:438`) to order two racing Completes. The G0
  fence leg keeps only the applied cases (`:1510`); the G0-flip leg and both G0-seg legs are
  removed (replaced by G1, G3, G4 as the brief says). New legs: G1 `:5353`, G1-open `:5398`,
  G2 `:5453`, G3 `:5516`, G4 `:5614`, G5 `:5718`, G6 `:5865`. The old "G0-seg resumed" check
  (the error names the flip only in a round with no segment batch) now lives in G3.

## Red → green evidence

Green: `cargo test -p wyrd-core --test multipart_complete` → 69 passed; `multipart_abort` → 24
passed. Full gate `./engine/xtask.sh ci` (fmt, clippy, build, test, deny, conformance, statics,
guards) → `xtask ci: all checks passed`, rc 0, nothing warn-skipped.

**Red on unpatched production** (base `complete.rs`, the new test kept): G1, G1-open, G2, G4,
G6 FAIL; 64 others pass. G3 and G5 PASS on base. That is expected: the base returns the in-doubt
error at once on every unknown outcome, which is the safe-but-coarse answer those two legs
accept. Their red comes from the brief's "demonstrated reds", which I ran as six breakages of
the final fix (script and logs in the scratch dir, see below), each with the whole file:

| Breakage (brief's demonstrated red) | Legs that go red |
|---|---|
| M1: the flip's unknown outcome returned without a re-read | G1, G1-open, G2, G4 |
| M2: an unchanged re-read after `may_still_commit: true` taken as "did not land" (every outcome treated as out of flight) | G2 |
| M3: a failed re-read answered as a refusal (`OperationAborted`) | G3 (and the existing `a_read_that_fails_after_a_lost_flip_releases_the_fence_and_stops`) |
| M4: a resume restarting from segment 0 (`resume_from: 0`) | G4 (and D3, G3, M) |
| M5: matching fence bytes taken as ownership (`break` with the fence instead of `InDoubt`) | G5, G0-fence (applied) |
| M6: an unapplied unknown fence returned without a retry | G6 |

So C4-verify should report the base red on G1/G1-open/G2/G4/G6 (it runs the whole test file);
the brief already expects `PASS (green-only)` posture for the legs whose red is a breakage.

## Refuting my own test

* **(a) Genuine red?** Yes. Base `complete.rs` restored, test kept: 5 legs fail (G1, G1-open, G2,
  G4, G6). G3 and G5 do not fail on base, and I say so above; each goes red under its named
  breakage of the fix (M3, M5). First version of G5 hung 120 s on base (B's flip waited on a
  gate only A's re-read opened, and base never re-reads). I changed it so the gate also opens
  when A returns (`multipart_complete.rs:5731-5735`); base then passes G5 in ~2 s, and M5 still
  fails it fast.
* **(b) Production path?** Yes. Every leg calls `complete_upload` / `fence_complete` on a real
  `RedbMetadataStore` through the logging wrapper; parts are staged by the real `upload_part`
  (or planted as canonical records for the fence-only G0 leg), and published objects are read
  back with `read_object`. The wrapper only changes commit outcomes (apply / hold / withhold the
  answer / order), never the code under test.
* **(c) Fixture includes the fault?** Yes. Each fault is injected at the real commit point:
  the flip applied and answered unknown (G1); held with `may_still_commit: true` and landed later
  by `release_held` (G2, G6); the session read failing after the unknown (G3); the segment batch
  applied/not applied (G4); a real rival Complete on the same store with a byte-identical fence,
  both orders (G5). G2 and G6 check that the held batch really lands (G2) or really conflicts (G6)
  when released.

## Decisions and alternatives (for the human)

1. **`Open` after an unknown outcome answers `OperationAborted`; this call does not re-fence.**
   The brief's Design says "`Open` ⇒ answer as a fresh Complete". I read that as "the session
   is back to `Open`, nothing of ours can land, so answer as a fresh request would be answered
   *by the client's retry*". I did not re-fence inside the same call. Reason: the request's
   `fenced_at_millis` (`CompleteRequest`) is the caller's single clock reading from before the
   first fence, and core reads no clock (ADR-0009). An `Open` session found here means the
   reaper rolled the fence back because `now - fenced_at_millis > W_completing`
   (`0016:1005-1009`). A second fence stamped with that same reading would already be past the
   window, which is F16. A fresh stamp is the caller's to read, so the fresh Complete is the
   client's retry. Cost of the alternative: a bounded loop in `complete_upload` around
   `fence_complete` + `publish_held` (about 25 lines) plus a new `PublishOutcome::Reopened`
   return. It is cheap to write but wrong on the stamp. `G1-open` pins the behaviour
   (`multipart_complete.rs:5398`). **Please confirm this reading at sign-off.**
2. **No release while a held doubt stands (G2).** The alternative is to release anyway: the
   release CAS is safe, and it would turn some in-doubt endings into `OperationAborted`. It
   would need `release()`/`moved()` to recognise our own landed flip, so the answer is
   `Published` and not `AlreadyCompleted`. That means passing `unknown_flips` into `release` and
   `moved` and checking before `follow`, about 20 lines over 3 functions. I did not do it
   because the brief says "When the bounded rounds run out ... return the in-doubt error (G2)"
   and "Never release a fence that cannot be proven still held". Cost accepted: such a fence
   waits for the held batch or the reaper (#625), like G5's.
3. **Held bytes pinned by the next read, not computed at commit time.** The other way is for the
   committer (`staged_publication::publish`) to report which batch answered unknown. That
   changes #653's API (`staged_publication.rs:445-465` returns the store's error as-is, by
   contract `:76-81`). Out of scope, and pinning by read is sound (argument above).
4. **Pre-fence validation skipped while a fence may still land** (`complete.rs:912`). The other
   way is to keep it and turn any refusal into `InDoubt`. That ends in doubt where a retried
   fence could settle it; the brief says "A retried fence that answers a definite success
   settles it".
5. **The in-doubt error carries no stop cause.** Abort's `InDoubt` has `cause: Stop`;
   Complete's has only `fault_behind_fence`, whose text says "that release followed a fault",
   wrong here since no release was sent. Adding a `cause` field changes the error type, which
   the brief gives to child-1. So an in-doubt ending after, say, a failed marker read does not
   name that read. The outcome and the commit are named. Worth a follow-up if operators need it.
6. **G5 is flat (parts 1+2 vs 1+3)**, so "no `seg:` record" is trivially true there; the leg
   still asserts it, and what really catches the M5 breakage is "no flip, nothing after the
   fence". A segmented G5 would need the deep fixture (~0.5 s planting) and could not read the
   object back; I kept the brief's literal parts.

## External dependencies

`typos` (the brief's only one) is installed (`typos-cli 1.48.0`) and passes on every changed
file. No undeclared dependency was needed.

## Commit-readiness

`cargo fmt --all` applied; `cargo clippy -p wyrd-core --all-targets` clean (workspace lints deny
warnings); `typos` clean; full `./engine/xtask.sh ci` green.

## Scratch

Evidence (base and breakage logs, the breakage script, the full gate log) is under
`$PDCA_SCRATCH/pdca-builder-1010-redleg/`, plus an empty `pdca-builder-1010-applycheck.*` dir
from an apply check. Both are inside the harness's scratch root; I left removal to the harness.
