# Adversarial review — issue #776 (`repoint_chunk`, seg-record placement move)

**Verdict: I tried to refute the fix and could not.** Every proof I could re-run came back
green, every hand mutation I tried went red, and the one behaviour gap I found is a
low-severity wording issue in the docs, not a wrong write. Nothing below needs a rebuild.

## Evidence re-run (scratch copy of `$PDCA_TARGET`, toolchain present)

- `crates/core/src/metadata.rs:4433` (`mod placement_move`): all 15 tests pass. `cargo mutants --in-diff`
  re-run gives the same result as the gate: 36 mutants, 27 caught, 9 unviable, **0 missed**
  (`missed.txt` is empty).
- cargo-mutants never touches the pins, the `retired_or` call, the placement assignment, or the
  segment rebuild. So I mutated those by hand (14 mutants). **All were caught:**
  - segment pin dropped at `:3308` (sibling test and seeded campaign go red)
  - root pin dropped in the segmented arm at `:3308` (superseded-root test and campaign go red)
  - root pin dropped in the flat arm at `:3259` (flat conflict test and campaign go red)
  - over-ceiling guard at `:3279` forced to `false` (over-ceiling test goes red)
  - anomaly always answered as `Conflict` at `:3316` (damaged-segment test and over-ceiling test go red)
  - `retired_or` pointed at the wrong root key (same two go red)
  - `chunk == prior` weakened to `chunk.id == prior.id` at `:3357` (5 tests go red)
  - version bump made wrapping at `:3247` (exhaustion test goes red)
  - placement never assigned, in either arm (3 and 5 tests go red)
  - segment rebuilt at offset 0 at `:3306` (4 tests go red)
  - `continue` changed to `break` at `:3302` (zero-length boundary test and campaign go red)

  That covers the brief's named negations: equality, root pin, ceiling, and unchecked version.
- The tests run through the real code paths: the real `repoint_chunk`, the production resolver for
  planning, and a real redb store. There is no parallel copy of the logic, and "nothing written" is
  checked by comparing the whole store before and after.
- Budget and scope checks all hold: 144 added non-test code lines (limit 170), `patch.diff` is
  49,324 bytes (limit 50 KB), one file, no `MAX_ROOT_VALUE_BYTES` comparison, and
  `commit_chunk_map` is untouched.

## Findings

- `crates/core/src/metadata.rs:3125-3126` (and `:3135-3136`): the doc says `Repoint::Refused` is
  "not transient — it fails every pass until the record shrinks". That is **not true for a stale
  plan**. I confirmed it with a probe:
  - Segmented: a generation whose segment row sits one byte under the ceiling's reach. Flip the
    root to a small flat generation, leave the old `seg:` row unreclaimed, and repoint to
    `[u64::MAX]`. The answer is `Ok(Refused { bytes: 100001, .. })`, not `Conflict`.
  - Flat: a stale near-ceiling generation whose object has since been overwritten small gives the
    same answer.

  `VersionExhausted` behaves the same way for a stale flat generation. No wrong write is possible,
  because nothing is committed. The cost is that #777 could raise an operator signal for an object
  that no longer has the problem, and it clears on the next pass. **Low severity. I am not flagging
  it for a rebuild:** the patch is within about 700 bytes of its 50 KB budget, and the natural fix
  belongs in the caller. #777 should re-resolve before treating `Refused` or `VersionExhausted` as
  permanent. The cheaper alternative is one caveat line in this doc.
- `check-gates.json` C4-ci row (`gate-logs/C4-ci.log`): the row is `unverifiable` (the gate hit
  its 7200 s timeout while `tests/custodian_gc.rs` hung), yet `overall` is `pass`. I closed that
  gap myself:
  - `cargo test -p wyrd-server --test custodian_gc`: 10/10 pass in 0.17 s.
  - `cargo test --workspace --exclude wyrd-dst`: all green.
  - `cargo xtask statics`, `cargo xtask conformance` and `cargo xtask dst` (madsim clippy and tests): all green.
  - I did not re-run `cargo deny` or `cargo machete`. The diff changes no manifest, so they cannot move.

  The hang is a host problem, not this patch.
- `gate-logs/T4-batch-review.log`: the "3x codex" review finished in 45 s with 0 findings. The
  previous round raised 8 blocking findings. The log is a single summary line, so it does not show
  the passes actually covered this diff. This is weak evidence, but it does not refute the fix,
  because my own attempts above found nothing blocking.

## Attempted and could not refute

- **Zero-length chunk at a segment boundary** (`:3338`, `:3351`): only the two segments touching
  the offset can hold it, because segments are never empty. Equality picks between them, and a
  chunk with bytes never reads the neighbouring segment.
- **Over-ceiling segment rows**: a row at exactly V (`MAX_VALUE_BYTES`, the 100,000-byte value
  limit) is admitted, and a row at V + 1 is typed corruption or a `Conflict`, never rewritten.
- **Retirement, deletion and reclaim races**: covered by `:5069` (the seeded campaign), which
  checks against a model that doesn't know about the pins. All four race tallies come out non-zero.
- **A-B-A on the segment byte pin** (the bytes change and then change back): harmless. The batch
  is built entirely from the pinned bytes, so if the bytes match again, the result is identical to
  a fresh prepare.
- **Root pin is `encode(generation)`, not the raw bytes**: this is the same idiom
  `commit_chunk_map` uses, which the brief says to mirror. It is not new debt.
- **A malformed committed `prior` placement is accepted**: ADR-0040 decision 4 makes that the
  maintenance loop's job (#777), and decision 5 (write a full-length vector) is enforced here by
  `MalformedReplacement`.
