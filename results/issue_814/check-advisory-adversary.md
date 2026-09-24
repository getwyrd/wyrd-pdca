# Adversarial review — issue_814 (iteration 5)

This round changed one thing: a new test, `gc_reclaiming_the_vacated_position_before_the_adoption_makes_it_lose`, which the iteration-4 sign-off asked for so the adoption's two pins on the vacated position's mark would be tested. I tried to refute it, and the production code around it, and could not. I found one item that needs a human decision: the gating T4 gate still reports a blocking finding that is new this round. It is overstated, but only a human can override it.

## Refutation attempts

- Attempted to refute the new test by **deleting each pin it is meant to catch**, and could not. I ran this in a scratch copy of `$PDCA_TARGET`: `cargo test -p wyrd-custodian --test staged_repair`.
  - Unmutated: 30 of 30 pass.
  - Deleting `.require_absent(key.clone())` at `crates/custodian/src/reconstruction/staged.rs:718`: the new test fails in its "no mark" arm (`crates/custodian/tests/staged_repair.rs:1888`, "nothing was adopted"). The other 29 tests still pass.
  - Deleting `.require(key.clone(), current.clone())` at `staged.rs:721`: the same test fails in its "a stamped mark" arm (`staged_repair.rs:1888`). The other 29 still pass.

  Both pins that the iteration-4 sign-off said "can be deleted today with all tests green" are now caught, each by its own arm.
- Attempted to make the new test **pass for the wrong reason**, and could not.
  - It runs the production pass (`fx.run()`).
  - Its hook fires after the test double's first `get` of the vacated key (`staged_repair.rs:206-218`, run once and then retired). That first read is `assess`'s read at `staged.rs:296`, before destinations are chosen, so the race it stages is the real one.
  - A pass that writes nothing, or one the pre-mark batch stopped, cannot satisfy it. It asserts:
    - exactly one arrival on the destination (`staged_repair.rs:1879-1883`);
    - the fragment is held there (`:1884-1887`);
    - a fresh pre-mark still stands (`:1899-1903`);
    - a `conflict` audit event names this chunk (`:1905-1908`).
  - Each arm uses its own chunk id (`0x8161`/`0x8162`, `:1846`; no other test uses them), so the audit log that all tests share cannot hand it another test's event.
- Attempted to strand a fragment through the **unpinned arm**, `VacatedMark::Reclaiming => adopt` (`staged.rs:723`), and could not.
  - GC never turns a `reclaiming` mark back into another shape. It resumes the delete (`crates/custodian/src/gc.rs:671-678`), and only the sweep of marks with no fragment under them retires the mark, once the fragment is gone (`gc.rs:1845-1847`).
  - While the part record still names the position, GC's reference check wins over the mark (`gc.rs:654-667`), so GC keeps the mark and finishes after the adoption.
  - So a vacated position read as `reclaiming` stays covered until its bytes are gone.
- **C5's 4 surviving mutants** (`staged.rs:766-769`, `&&` → `||` in `repointed_part`) cannot be told apart from the original code in any reachable state, so they are not a test gap.
  - The canonical part-record spelling puts `"chunks":[...]` first. The test fixture checks it matches the decoder's own spelling (`staged_repair.rs:457-470`).
  - The splice at `staged.rs:755-763` therefore always replaces the real chunk list and changes no other byte, so every conjunct at `:765-769` is always true.
  - The same 4 survived v3 and v4, and neither sign-off asked for them.
- **C4-verify's "30 test(s) ran red"** is the harness's known counting bug. The log shows 27 failed and 3 passed on the base (`gate-logs/C4-verify.log:248`). The 3 that pass on the base are the leg-E guards, which the brief says are green on the base by design. The new test fails on the base because nothing is rebuilt ("GC never reclaimed the vacated position"), which the brief allows for rule cases. The mutations above show its rule branch is reached on the fixed code.
- The DST cases (DST: deterministic simulation testing) ran under madsim inside the gating C4-ci run and passed: `staged_replace_under_the_fence_strands_nothing` and `staged_replace_reaches_every_point_of_the_fence` (`gate-logs/C4-ci.log:3376`, `:3655`, `:3664`).

## Needs a human

- NEEDS-HUMAN [human] — **T4 (gating) fails with 4 blocking findings. Three are settled; the fourth is new and overstated.**
  - The three at `crates/custodian/src/reconstruction/staged.rs:212` (the first-reference rule for duplicate part references) were overridden at the iteration-3 sign-off ("do not re-raise").
  - The new one is at `staged.rs:343`: keeping a full repointed part record (`next`) in every staged plan "multiplies assessment memory… tens of gigabytes". My measurement says the claim is overstated:
    - Every `RepairPlan`, committed or staged, already holds its survivors' decoded shard bytes (`crates/custodian/src/reconstruction.rs:167-168`). That is about one chunk per plan: 1 MiB by default (`crates/server/src/lib.rs:51`).
    - `next` is capped near `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:549`). A stored record is at most that size, and `staged.rs:582` refuses anything over the limit.
    - So `next` adds at most about 10% per plan on top of what the base already holds. Any "tens of GB" comes from the base's shape — assess every queued chunk, then repair (`reconstruction.rs:320-327`, `:440-446`) — not from this patch.
  - The shape does depart from the committed path's written rule, "an index, never a copy… not Q×N" (`reconstruction.rs:181-185`). Building `next` lazily inside `staged::repair` would fix that, but it grows the patch, which the brief forbids this round.
  - Decision needed: override T4 `:343` as bounded and non-blocking, or file it as a follow-up. It is a sign-off call, not a rebuild.

## Bottom line

I tried to refute:
- the new test: by deleting each pin, and by looking for ways it could pass wrongly;
- the unpinned `reclaiming` arm;
- the C5 survivors;
- the C4-verify count;
- the new T4 finding.

I could not refute the fix. The only open item is the T4 override above.
