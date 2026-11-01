# Adversarial review: issue #1007, Complete fence

I could refute one behaviour and two test claims. Fence ownership, release and the budget held up. All probes and mutations ran on a scratch copy of the target (since removed). The suite passes there: 43 of 43 tests.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:609`: **a pre-fence rejection answers from a session state that may be out of date.** The session is read at `complete.rs:570`, the list is checked at `:607`, and a rejection returns at `:609` without looking at the session again. So "the state answers first" (`complete.rs:687`) only holds at the moment of the first read.
  - Probe: a store wrapper lets the first `mpu:` read return `Open`, then writes a `Completed` tombstone (the record a published session leaves behind) and deletes the `psum:` and `part:` records, as the retirement drain would.
  - Result: an identical retry answers `Refused(InvalidPart { part_number: 1, reason: Absent })`. The same request sent again answers `AlreadyCompleted`, which is what the `0016:976` table requires.
  - Variants: a tombstone plus a deleted bucket answers `NoSuchBucket`. `Aborting` plus deleted `psum:` records answers `InvalidPart` where the table says `NoSuchUpload`.
  - Impact: the client is told a published upload failed. The window is small and needs publication and the drain (#1008, #659), which have not landed, so nothing can hit this in production today. It would be cheap to fix now: on a pre-fence rejection, re-read `mpu:`, and if the bytes differ from `read`, go round the loop and answer from the new state. That costs one read and no commit. The sign-off note says to keep the logic "unless a new test proves otherwise", and this probe is such a test.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:2437`: **this test's claim about the cause can't fail.** Its doc comment says "the cause stays the fault that stopped validation", and `complete.rs:75` makes the same promise for every error in the module doc's table. But `assert_read_fault` (`multipart_complete.rs:661`) only compares the text to `FAULT`, and the failing scan and the failing re-read both produce that same text.
  - Mutation: at `complete.rs:896`, report the re-read fault as the cause and drop the validation fault. All 43 tests still pass.
  - This is the sign-off's iteration-2 item 6 (keep the scan fault visible), so the promise is untested on this row.
  - Fix: give the armed scan fault and the armed get fault different text, and assert the scan fault's text here.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:1101`: **no test shows the held bucket marker comes from the read behind the fence.** `complete.rs:251` documents it as "read behind the fence", and the brief wants it for the publication's precondition. The only value assertion compares it to a `MARKER` constant that never changes.
  - Mutation: return the marker from the pre-fence read at `complete.rs:661`. All 43 tests still pass.
  - The same mutation for the frozen part set is caught by `the_held_fence_carries_the_set_read_behind_the_fence` (`multipart_complete.rs:1970`). The marker has no matching leg.
  - Fix: rewrite `bucket:` with new bytes just before the fence commits, and assert the held fence carries the new bytes.

- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:39`: **the patch narrows the brief, and a missing `part:` record now passes.** The brief's scope (`brief.md:107`) says "the frozen `psum:` / `part:` ranges", but the patch reads only `psum:`.
  - Probe: delete `part:` for a named part and keep its `psum:` record. The request returns `Held`. A `part:` record whose digest disagrees with its `psum:` record is also `Held`.
  - The part commit writes both records in one batch (`0016:660`), so reaching this needs store damage, and the module doc gives a sound reason (chunk lists can reach tens of megabytes). But it does sit in the rubric's "absent entries, never silent success" class, and the valid-list path has no test that shows the choice.
  - Decision needed: confirm `psum:`-only validation, and confirm that #1008 must treat a missing `part:` behind a held fence as an error. A key-only presence check of `part:` is the cheaper alternative if you want this slice to catch it.

- NEEDS-HUMAN [human] — `check-gates.json:104-113`: **the logs don't show that the T4 review did real work.** The gating row is `pass`, but `gate-logs/T4-batch-review.log` shows three passes over a 176 KB patch finishing in 19.25 seconds, with "0 blocking, 0 recorded-rejected, 0 noise-dropped" and no per-pass counts. Zero noise dropped from three passes is unusual. This may be fine (a cached or parallel run, say), but I did not have `review-batch.md`, the file that would settle it. Please check it before counting this row as a deep review.

## Tried and could not refute

- **Red-to-green evidence.** C4-verify is `unverifiable` because the module doesn't exist on the base, which the brief declared in advance. That can't be reproduced as a plain revert, so I forced the reds through the production code instead. Each of these made the suite fail:
  - a rejected request that doesn't release (15 tests);
  - attempts not restored;
  - a definite release failure answered as a refusal (D4);
  - an unknown fence treated as success (G0);
  - a release or fence commit with its `require` removed.

  I did not separately run the brief's other two reds: a rejected request charged against `attempts` (D1), and a request that releases another publisher's fence (E). D1's zero-commit assertions and E's byte-identity assertions should catch them. The tests run over real redb through the production `fence_complete`, with no parallel copy.
- **Mutation depth beyond the C5 gate.** 42 hand-written mutations ran, all caught except the two above and two I discount: one is equivalent to the original code, and one only truncates the walk past 200 parts, deeper than the two-page tests go. The bounds `RELEASE_ROUNDS` and `FENCE_ROUNDS` are pinned by numbers written in the test: a release bound off by one in either direction fails, and so does one extra fence round.
- **C-1 (no answer leaves this Complete's own fence up).** I walked every return path in `release` and `moved` (`complete.rs:875-953`). Each answer finds the fence down, and only an error can leave it standing. `NotPlaced` is never reported after a `Committed` fence.
- **Serialization identity.** A session with a `content_type` and an object name containing a quote and a non-ASCII character went through fence and release. The stored bytes differ from the originals only in `epoch` and still decode as canonical.
- **Epoch and attempts edges, and fence lost to a later `Open`.** From reading the code and its existing legs (I wrote no new probe here): the bound checks and the re-read after a lost fence hold, and the release restores the attempts of the session actually fenced.
- **Paging.** Clause 4 of the `scan_page` contract permits duplicates, while `complete.rs:795` treats one as a store fault. Clauses 1 and 2 make a repeat at or below the cursor impossible on a conforming backend, so this is not a false fault in practice.
- **Top-level `downcast_ref::<CommitUnknownResult>` (`complete.rs:630`, `:909`).** The trait documents this as how to recognise that class on any backend (`crates/traits/src/lib.rs:216`), so a wrapped outcome is outside the contract.
- **Not raised, as settled or out of scope:** DST coverage (deferred to #660 in the code), the deleted-and-recreated bucket question (open with the human per the sign-off), and the `bucket` field not being cross-checked against the session's object (the session stores no bucket, and the S3 verb composes both from one path).

<!-- pdca:leaf-complete -->
