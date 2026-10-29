# Adversarial review — #933 (657.1b, UploadPart staging renewal)

Scope: `patch.diff` against the patched tree at `$PDCA_TARGET`. I re-ran parts of the proof in a
scratch copy (`cargo test -p wyrd-core --test multipart_upload_part_stage`, toolchain present), added one
probe leg, and made two temporary mutations. Overall: the renewal batch, the round logic, the
quiesce `renew` and the clamp held up, but I broke one path with a concrete test.

## Refuted

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:2075-2077` (with `:1925-1928`): a renewal that **fails** while a chunk's fragment writes are still pending returns through `?`. That drops `pending`, `answers` and `unanswered` with no accounting. Staging then answers a determinate error (or a refusal) while a put is still in flight. This breaks 657.1a's own rule at `stage.rs:46-56` ("an abandoned write ... `ErrorClass::Indeterminate`, however the timing fell"). It also makes two docs claims false: the module doc at `stage.rs:121-124` ("writes pending beside it are abandoned, **as at their deadline**"; at the deadline they become `WriteEffectUnknown`, here they don't), and `docs/design/architecture/05-building-block-view.md:246` ("a write still pending then ... the error that stops staging is classed indeterminate in either case"). **Concrete failing case**, reproduced with a probe leg over the patch's own harness. Hold `FragmentId{chunk:1,index:0}` at `HoldAt::Arrival`, use `write_window_millis = WINDOW`, advance the clock by `HALF`, and fire the timer. Then:
  - (a) Renewal scripted `Act::Fail` → `Err(Stopped{cause: Store(..)})`, `classify` = **Terminal**, and the error text says "no batch of this part attempt is in doubt".
  - (b) `write_window_millis = TTL+1_000`, clock `+TTL+1` → `Stopped{LeaseLapsed{slot..}}`, **Terminal**.
  - (c) Renewal scripted `Act::First(Side::Abort)` → `Ok(Refused(NoSuchUpload))`.

  In all three runs, releasing the held put **after staging returned** stores the fragment (`Event::Stored`). Write answers already collected, such as a `WriteEffect::Unknown` report, are lost the same way when the renewal fails after the writes answered. No appended leg covers a failing renewal beside a pending put: the R4 legs at `multipart_upload_part_stage.rs:5530-5748` all fail the renewal during a **body read**, where dropping the work has no side effect. Fix direction: on a renewal `Halt` in `write_fragments`, settle the writes the way the deadline path does. Fold pending and unknown writes into an Indeterminate stop, keeping the renewal's cause or doubt. Add a leg for (a)–(c). The T4 batch review raised the same defect 3/3 as blocking. This run confirms it with a reproduction rather than reading. Case (c) is the weakest: a refusal with a write in flight may be acceptable since the session is gone. The builder should decide that explicitly and state it, not leave it implicit.

## Attempted, could not refute

- `stage.rs:1549-1560` (R1): with the slot put removed from `renewal_batch`, 8/8 R1/R2 legs go red. `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` fails for the **right** reason: the stand-in fence answers `Committed`, not `Conflict`, at `multipart_upload_part_stage.rs:5012`. So the crossing-expiry leg is not a tautology.
- `stage.rs:1602` (R4/R7 per-round reading): with one reading reused across rounds, `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds` and three R7 legs go red. The "fresh reading every round" claim is real.
- `stage.rs:1626-1681` (R5 settling): I looked for an ABA or wrong-adoption case. A renewal's slot value is strictly later than every slot value that landed before it (`next_expiry`, `:1500-1504`), rounds with equal slot values put identical bytes everywhere, and a landed round fences every other round's `require(slot == prior)`. I found no input where a non-landed round is adopted or a landed one is retried.
- `stage.rs:1007-1029`, `:1884-1927` (R3 abandoned-renewal doubt): every await point at which the renewal can be dropped leaves the ledger correct. `awaiting` is set only around `commit`, and a `Committed` answer reaches `apply` with no await in between. The four-run leg at `multipart_upload_part_stage.rs:5357` pins each case.
- R8 clamp: `stage.rs:315` / `multipart.rs:5539-5542` match the batch's real `3 + 2N` count, and `renewals()` checks that count on every batch sent. The single C5 survivor (`stage.rs:315:57`, `>` → `>=`) is an **equivalent** mutant. `3 + 2N` is always odd and `MAX_BATCH_OPS = DEADLINE_OPS/2 = 500` (`multipart.rs:5158`, `:5171`) is even, so the two can never be equal. Not a test gap. It belongs in `build-notes.md`, or in `.cargo/mutants.toml`'s `exclude_re` with that reason.

## Claims to read with care at sign-off (not refutations)

- `check-gates.json` C4-verify "pass": as the brief says, this is green-only, not a red→green. The two reds I reproduced above are real, but I did not check the other ~16 demonstrated reds `build-notes.md` should record (withheld from me).
- C4-diff-cov "fail": not measured, because the bundle does not apply on `origin/main` (stacked on 657.1a, #932). The claim that every changed line is executed is unverified. C5 (51/52 viable caught) partly makes up for it. The branch needs refreshing onto #932's merged form before merge, per the rubric's stacked-branch rule.
- Edits to 657.1a's legs beyond "asserted the absence of renewal": the shared fixture's `max_part_chunks` changes `1_000` → `MAX_PART_CHUNKS` (`multipart_upload_part_stage.rs:236`), forced by the new R8 refusal. Three `waits()` assertions change `IDLE` → `HALF`, and `a_double_prime_the_idle_bound_is_per_read_not_per_body` now uses a 4×TTL lease so no renewal runs in it (`:2342`). All are justified, but the brief requires `build-notes.md` to name each one. I could not check that.

<!-- pdca:leaf-complete -->
