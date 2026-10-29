# Adversarial review — #932 (657.1a, UploadPart staging core)

The toolchain was present. I copied the patched tree into scratch, built
`multipart_upload_part_stage` (59/59 green, as in `gate-logs/C4-ci.log:1251-1314`), and
applied hand-made mutations, one at a time, restoring the file after each. The core claims
hold. I found two untested branches, one case where the answer depends on read timing, one
input the validator lets through, and one gate verdict I don't trust.

## Findings

- NEEDS-HUMAN [impl] — **A refusal-answer swap survives the whole suite** (the class the brief says sank v3).
  At `crates/core/src/multipart/stage.rs:1655-1662`, a held intent that landed before the
  session moved to a **non-`Open`** state is answered `Refused(NoSuchUpload)` and writes no
  fragment (the comment at `:1659-1660` says so). I changed `:1661` to
  `let _ = self.footprint.moved_on(moved, &mut doubt);`. All 59 tests still passed.
  Concrete schedule: `Act::Hold{advance:0}` on the first intent, then
  `after_read(slot:<id>:0, skip 0, [ReleaseHeld, Abort])`, one-chunk body. Unpatched code:
  `Refused(NoSuchUpload)`, 0 puts. Mutant: `Staged`, 6 fragment puts, session `Aborting`.
  The only test of this branch,
  `e_a_held_intent_landing_before_the_session_moves_is_adopted`
  (`crates/core/tests/multipart_upload_part_stage.rs:3745-3782`), moves the session to a
  later `Open` epoch, so the `Session::Refused` arm is never taken. Fix: add the Abort run of
  that leg, and assert `NoSuchUpload`, no puts, and the owned entry left as residue.

- NEEDS-HUMAN [impl] — **For one store history, the answer depends on which read sees the Abort first.**
  The slot-read adopt path at `stage.rs:1623-1639` adopts the landed intent and writes its
  fragments without looking at the session. Schedule: same hold, but hook the **entry** read
  (`after_read(sidx:<id>:<n>:1, skip 0, [ReleaseHeld, Abort])`). Result: `Staged`, 6 puts,
  session `Aborting`. The schedule above (Abort seen by the session read) gives `NoSuchUpload`,
  0 puts. This does not break C-1: the entry is durable before any put, and the reaper's
  full-placement orphan-mark covers late writes. But the rule at `:1659-1660` ("under a session
  no longer `Open` this is the refusal, and no fragment of the chunk is written") is only true on
  one of the two paths. Pick one rule: either re-judge the session after the `:1638` adopt, or
  reword `:1659-1660`, which overclaims. Then test both paths.

- NEEDS-HUMAN [impl] — **The reserve's `doubt.settle()` is untested.** I deleted `stage.rs:1075`
  and all 59 tests passed. Concrete case: the reserve answers
  `Unknown{applied:false, may_still_commit:false}` at index 0. The slot read finds it absent,
  and the reserve probes index 1. Then a store fault on the next `scan(slot:<id>:)` should be
  `Stopped(Store)`. The mutant reports `InDoubt{may_still_commit:false}` for a batch a read has
  already settled. The intent has this leg (`tests/...:3560`). The reserve does not.
  Low severity: over-reporting doubt is the safe direction.

- NEEDS-HUMAN [impl] — **`max_part_chunks: 0` passes `StageRequest::check`** (`stage.rs:216-244`).
  That breaks the check's own rule ("a field no staging can honour … before anything is read or
  written"). Tried: a 64-byte body with `max_part_chunks = 0` reserves a slot (1 reserve commit,
  1 `slot:` record) and returns `Ok(Staged)` with 0 chunks, `len 0`, `chunk_bound_exceeded`
  true (`:1384`). It should be `Stop::InvalidRequest` with nothing written, and the (H) leg
  should cover it.

- NEEDS-HUMAN [human] — **An over-limit body comes back under the success variant.** (G) is met
  as the brief words it. But `StageOutcome::Staged` is documented as "the body is staged: every
  chunk is registered" (`stage.rs:267`). With `chunk_bound_exceeded` set, it holds a **prefix**
  whose `len()`/`digest()` describe only that prefix (`:398-410`). So a #927 commit that forgets
  to check the flag publishes a truncated part with a valid-looking digest. The rubric's
  protocol-input class says oversize input is "an error — never silently accepted". Should the
  overrun be its own outcome variant carrying the attempt? That would still satisfy (G) and
  would stop a caller from skipping the check. This is an API-shape call for the #927 owner. At
  minimum, the doc at `:267` is wrong for this case.

- NEEDS-HUMAN [human] — **The gating T4 review verdict looks vacuous.** `check-gates.json`'s
  `T4-batch-review` row is `gating: true, result: pass`, with "0 blocking, 0 recorded-rejected,
  0 noise-dropped" in **21.18 s** (`gate-logs/T4-batch-review.log`). That is supposed to be
  three codex passes over a 6,251-line, 284 KB diff. The brief's carry-forward records that last
  round's codex reviewer "exited 1 early". Zero findings *and* zero noise in 21 s looks like a
  review that never read the diff, not a clean one. Check `review-batch.md` before counting this
  row toward the definition of done.

## Claims I tried to refute and could not

- **C4-verify `unverifiable`**: this is the brief's pre-declared exit 77
  (`gate-logs/C4-verify.log`, a compile failure on the reverted base). I re-ran the brief's
  required reds that history says survived before. Each went red on the leg its label names:
  - per-server `require_absent(desired:dserver:<S>)` removed → B1
  - planning on the full topology → B2 (and B1/B3)
  - `stage_part` full-range refusal swapped to `NoSuchUpload` → `c_stage_part_with_every_slot_held_answers_slow_down`
  - session-left-`Open` answered `SlowDown`, plain path `:1113` and pinned path `:1100` → the two (C) session legs
  - same-index pin removed → five (C) legs
  - intent's `require(mpu == Open@E)` removed → E′1, E′2 and three more
  - an empty piece re-arming the idle deadline → `a_double_prime_empty_pieces_do_not_count_as_progress`
  - the fake's publication-point check removed → the late-put and refused-as-late legs
  - a late body answer accepted → `a_double_prime_a_body_answer_seen_past_its_idle_deadline_is_late`
  - the pre-dispatch check `>=` made `>` → `a_prime_a_deadline_passed_before_dispatch_sends_nothing`

  The red→green evidence is real and runs the production functions. The fake enforces the
  deadline at its publication point (`tests/...:948-955`), and the fake's puts run in their own
  task, so an abandoned put really does finish later.
- **Silently dropped fragments via `.zip(placement)`** (`stage.rs:1471-1481`): not possible.
  `select_distinct_domains` returns exactly `n` or errors (`crates/core/src/placement.rs:217-244`).
- **Fragment deadline later than `t_auth + W`**: an adopted round uses its own `t_auth`. Two
  rounds can only put identical entry bytes at the same clock reading, so the first-match
  `position` (`:1612`, `:1634`, `:1655`) never picks a later deadline.
- **Reserve giving one attempt two slots**: after a session move the pin is dropped
  (`:1107-1110`), but only after a second read of the pinned index (`:1090-1105`). Every
  in-flight batch required the old session bytes, so a probe elsewhere is safe.
- **`parse_desired_dserver_key` accepts `+5` / `05`** (`crates/core/src/metadata.rs:107-113`).
  It was moved verbatim from the custodian, and the staging planner only errs toward *excluding*
  a server. Old code, not raised.
- **`write_window_millis` ≥ `lease_ttl_millis` is not rejected**: a fragment can then publish
  after its owned lease lapses. Teardown orphan-marks the full staged placement (0016 X49), so
  the fragment is still evidenced. Not a C-1 break.
- **No seeded DST coverage**: deferred to #660/#665 in the module doc (`stage.rs:129-131`).
  That deferral is settled under the rubric. Not raised.

All scratch work was done in `$PDCA_SCRATCH/pdca-adversary-932-mut`. The target was not changed:
`stage.rs` diffs clean against `$PDCA_TARGET`.

<!-- pdca:leaf-complete -->
