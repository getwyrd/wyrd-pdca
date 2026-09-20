# Adversarial review — issue 663 (staged scrub and repair)

Advisory only. I rebuilt the patched tree in a scratch copy, re-ran `staged_repair` (38/38 green),
and ran two probe tests of my own. The red leg in `gate-logs/C4-verify.log` fails legs A, B, C and
D(iv) by assertion on the base, and the tests drive the production `reconcile_step`, not a copy.
One liveness defect is confirmed by a probe. The rest are judgment calls or notes.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:388-394` (`choose_destinations`):
  if one *(server, fragment index)* position is unusable, the whole server is excluded. It is not
  offered to the chunk's other missing fragments. **Confirmed by probe:** RS(2,2) on servers 0..3
  (domains A..D), servers 2 and 3 unreachable, free reachable domains E (S4) and F (S5), and one
  stale `reclaiming` mark at `orphan:<S4, frag 2>`. That is the state GC leaves when it dies between
  deleting a fragment and deleting its key; `gc.rs:622-626` defers cleaning it up to #800, so it
  stays. Result: 3 passes in a row, 0 writes, obligation still queued, every pass reports
  `Satisfied`. The swap (S4 → fragment 3, S5 → fragment 2) was valid the whole time. The chunk
  sits at survivors == k, the most urgent repair there is. An unreadable mark at that position
  (`:435-437`) blocks it the same way until a human steps in. Fix: exclude the position, not the
  server. For example, keep a per-index exclusion set, or try a rejected server against the other
  missing indices before dropping it. Add this case as a test. This confirms T4's three duplicate
  findings at `:392`.

- NEEDS-HUMAN [human] — `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`:
  carry-forward item 4 ("a CLI-configurable constant in `cli.rs`, following `LEASE_TTL_MILLIS`")
  is met in name only. At runtime the library's constant is used. The `cli.rs` value is a copy held
  equal by a compile-time assert, so changing it breaks the build instead of changing behaviour.
  `cli.rs` owns `LEASE_TTL_MILLIS` and the server passes it down (`custodian.rs:114`). Here the
  ownership is reversed. The builder had no choice: the brief forbids a new context field or a
  `reconcile_step` signature change. You decide: accept the copy, or relax the brief so the
  composition root can pass the window in. The inequality doc against #800's `D` is present
  (`cli.rs:108-121`).

- NEEDS-HUMAN [human] — `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): the fix for
  carry-forward item 2 adds an `Instant` read to the pre-mark and deadline lifecycle: the caller's
  `now_millis` plus the real time elapsed. **I found no production failure.** In the deployed loop
  both parts are real time (`server/src/custodian.rs:529,543`). I tried wall-clock steps both
  forward and back mid-pass: the pre-mark's grace and the write deadline move together, so the
  inequality holds. Under madsim, `Instant` is simulated time. But every non-madsim test passes a
  manual clock, so stamps become "manual + real elapsed". They are not test-controlled: the tests
  need a 60 s `PASS_SLACK` tolerance (`tests/staged_repair.rs:100`), and no test can script a write
  landing exactly at `stamp + W_write`. The rubric's first MUST says test-controlled time goes
  through the testkit `Clock` seam (ADR-0024), which the brief's no-new-field rule blocks. You
  decide whether this compromise stands.

- NEEDS-HUMAN [human] — the gating T4 failure (`gate-logs/T4-batch-review.log`), for the two findings
  besides the one above. I recommend rejecting both with these recorded reasons:
  (1) `staged.rs:536` (a late `Unknown` landing is stranded). This needs two things at once: a stale
  fragment already at P_new, so GC's list-driven walk visits the position and reclaims the
  pre-mark; and a publication hung more than `G_orphan − W_write` (40 s) past its deadline. The
  late bytes then land unmarked, and GC never collects unmarked bytes (`gc.rs:552`). The same gap
  exists for every writer that carries a deadline (`traits/src/lib.rs:886-891`, "position
  coverage"). It belongs to 0016's deadline model, not to this diff.
  (2) `staged.rs:314` (`EcScheme::None` → `Unrepairable` before any fetch). This is the same code
  as the committed path at `reconstruction.rs:750`, so it keeps existing behaviour and is out of
  this slice's scope.

- Claim in `check-gates.json` (C4-verify): "38 test(s) ran red" is overstated. The log shows
  `2 passed; 36 failed` on the base. The two passes are
  `a_scrub_verifies_a_chunk_named_by_both_classes_once` and
  `a_staged_chunk_already_whole_drains_its_obligation`. Both are regression guards that *should*
  pass on the base, so the red→green proof still holds. This is the harness's count, not the
  builder's.

## Attempted and could not refute

- **X29 losing branches** (`staged.rs:492-503`, `:547-575`): the pre-mark and the adoption each pin
  the session bytes, the part bytes, and the destination's drain key. The adoption also pins the
  pre-mark bytes. Every fence position I traced loses a CAS with the written bytes still covered.
  Two custodians re-placing the same chunk (split-brain) also works out: the second one loses on
  either the pre-mark pin or the part pin.
- **In-place rebuild** (`:559-564`): a lost adoption leaves a pre-mark on a position the part still
  names. That is harmless: GC skips protected fragments before it looks at marks
  (`gc.rs:476-483`), and unlink overwrites marks with a plain put (`core/src/metadata.rs:2116-2119`).
  The fs store's rename overwrites a corrupt fragment in place.
- **`SourceMark::Reclaiming` adds no precondition** (`:573`): this is safe. GC cannot clear a
  `reclaiming` mark over a fragment the part still protects, and a fragment-less one strands
  nothing.
- **Two owed chunks in one part (probe):** pass 1 repairs one. The other loses at the pre-mark with
  nothing written. Pass 2 repairs it, and nothing is left unmarked. That is one chunk per record
  per pass, the same pace as the committed path (`reconstruction.rs:207-211`).
- **`repointed_part` byte splice** (`:593-619`): the canonical spelling puts the chunk list first,
  and the result is decoded again before use. I could not make it rewrite any other field.
- **Deadline enforcement:** both test doubles now refuse expired writes (NotApplied) and report
  unverified ones (Unknown). CI ran `staged_replace_never_strands` and
  `staged_replace_reaches_every_window` under `--cfg madsim`, both green (`gate-logs/C4-ci.log:3577,3585`).
