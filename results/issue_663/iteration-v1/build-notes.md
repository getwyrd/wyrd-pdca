# Build notes — #663 staged scrub and repair

Base: `origin/main` @ `97fc2f9` (#803 and #804 merged). The patch applies to it cleanly
(`git apply --cached --check`). All `path:line` below are on the patched tree.

## What the patch does, in one paragraph

Scrub now verifies the fragments a multipart upload's **committed parts** name, against the
scheme each part record carries, and queues the ordinary repair obligation on rot or loss.
Reconstruction reads the staged records **before** the committed namespace and resolves an
obligation for a staged chunk instead of dropping it: a committed part's chunk is rebuilt and
re-placed under 0016's pre-mark rule (pre-mark → write with deadline → one adoption CAS pinned
to the session being `Open@E`, the part record's exact bytes, the pre-mark's exact bytes and the
destination's drain key); a still-streaming or fenced chunk keeps its obligation; a record the
pass cannot read withholds drains and certification. Every losing branch leaves the written
fragment under its pre-mark and the obligation queued. The committed repair path
(`repair_chunk`) is untouched in behaviour.

## Changes (patched tree)

- `crates/custodian/src/gc.rs`
  - `walk_staged` (`:1106`), `StagedRanges` (`:1057`), `StagedRecord` (`:1067`): the one bounded
    session walk every staged reader now shares. Each `Part` record carries its session's value
    as the listing read it, so a reader acting under the session fence can pin those bytes.
  - `staged_fragments` (`:1035-1052`) rewritten over it — same reads, same order, same outcome;
    #803's 26 staged-protection tests and #804's GC tests are unchanged and green.
  - Docs: module doc (`:67-69`), `StagedSet` doc, and its deferred marker now names only #664.
- `crates/custodian/src/scrub.rs`
  - Reads committed parts first (`:100`), names what it cannot read or trust (`:104-113`),
    verifies part fragments not already in the committed set against their own scheme
    (`:175-181`), refuses to certify over an unreadable part (`:253`).
  - `CommittedParts` (`:272`) / `committed_parts` (`:331`): parts only (`StagedRanges::Parts`) —
    no owned `sidx:` entry is read, per brief leg A and `0016:824`. A committed part's placement
    is held to its exact length (no identity fallback: every staged record is born full,
    `0016:828`); a wrong-length one is named, never expanded.
  - Emitters `emit_unscrubbable_staged` (`:350`), `emit_malformed_staged` (`:365`).
- `crates/custodian/src/reconstruction.rs`
  - `mod staged;` (`:74`); `RepairPlan.object` → `target: Target` (`:150`).
  - Staged reading first, committed after (`:203-208`); draining servers read only when a staged
    chunk is owed (`:211`).
  - `Assessment::Deferred` (`:678`) and `Assessment::Withheld` (`:684`), dispatched at
    `:311`/`:315`; repair dispatch by target (`:369-379`); drains withheld over an incomplete
    staged reading (`:404`); the pass does not certify over an unreadable staged record or a
    withheld repair (`:415-419`).
  - `assess`'s miss now goes to `staged::assess` (`:706`) instead of `Drain`.
  - `gather` (`:775`) and `Gathered::settle` (`:847`) are the committed `assess` loop and
    classification moved into functions unchanged (the only edit is the scheme parameter), so
    the committed and staged assessments classify faults and survivors by one rule.
- `crates/custodian/src/reconstruction/staged.rs` (new): `W_WRITE_MILLIS` (`:99`),
  `StagedReading` (`:105`), `read` (`:243`), `assess` (`:268`), `repair` (`:367`; pre-mark
  `:404`, deadline `:423`, deadline refusal `:438`, adoption `:451`, drain fence `:461`,
  vacated marks `:463-476`), `choose_destinations` (`:492`), `repointed_part` (`:559`).
- `crates/custodian/src/reconciliation.rs:27` — `Blocked` doc now names scrub and
  reconstruction beside GC.
- Tests: new `crates/custodian/tests/staged_repair.rs` (31 tests); leg F of
  `crates/custodian/tests/staged_protection.rs` rewritten (its own deferred marker handed it to
  #663/#664); property 15 appended to `crates/dst/tests/custodian.rs` (campaign leg, coverage leg,
  and the regression-seed runner).

## Decisions and what I ruled out

### 1. The `W_repoint` rule — the one judgment call worth reading

The pass has exactly one clock reading, `now_millis` (`reconcile_step`'s argument; the deployed
loop passes the wall clock). The brief forbids changing `reconcile_step`'s signature or adding a
context field, so the re-place has no way to read a clock *again* after the pre-mark commits.
A check "is my pre-mark older than `W_repoint`?" made on that one reading is always false for a
pre-mark this pass wrote — dead code the diff-coverage gate would flag.

What I did instead: the write's **authorization deadline is fixed at the pre-mark's own stamp**
(`deadline = stamp + W_WRITE_MILLIS`, `staged.rs:423`), and a pre-mark is **never reused**: any
mark already at the destination — another event's, a legacy one, or our own from an earlier
attempt — is re-stamped under a precondition on its exact bytes (`staged.rs:404-414`). So:

- age of the pre-mark at authorization = 0 < `W_repoint`, by construction;
- a worker that stalls after the pre-mark commits (the pause `W_repoint` exists for) delivers an
  authorization whose deadline is already fixed; the D server refuses it once past
  (`WriteDeadlineExpired::if_elapsed`, #638). Any landing is before `stamp + W_WRITE` on the
  acceptor's clock, strictly inside the pre-mark's grace given `G_orphan > W_WRITE + δ_clock`.
  This bounds when the write **takes effect**, which a caller-side check cannot (0016's own
  lesson at `:1302-1310`).

`W_WRITE_MILLIS = 20_000` is provisional (0016's knob table leaves `W_write` to the write path /
#625). Sized against the deployed `G_orphan` of 60 s (`crates/server/src/custodian.rs:114`),
leaving 40 s for `δ_clock` with nothing spent on `W_repoint`.

Ruled out, with cost:
- **`SystemTime::now()` at authorization**: mixes clocks in every in-process test (logical
  `now_millis` ≈ 1e6 vs wall ≈ 1.7e12 → every pre-mark instantly "stale", every repair restarts
  forever) — the #557/#565 class the rubric's first MUST names. Rejected outright.
- **`std::time::Instant` elapsed since the pre-mark**: legal under clippy.toml and madsim-virtual,
  but the in-process test cannot advance it, so leg D(v) could only be proven in DST; and it
  bounds only issuance, which the anchored deadline already bounds more strongly. Cost ≈ 15 lines
  + a new clock read to justify, for no additional safety.
- **Cross-pass pre-mark reuse with a deterministic per-move nonce** (so the check is live when a
  young own pre-mark is reused): ≈ 40 lines (nonce derivation, a third destination-mark arm, a
  reuse branch) to make a vacuous check non-vacuous; saves one metadata commit per retry.
  Nothing it adds to safety. Rejected.
- **Adding a clock / `W_repoint` to `ReconstructionContext`**: forbidden by the brief.

Consequence for the tests: D(v) is proven twice — in-pass (a hook advances the D servers' clock a
day between the pre-mark commit and the write: the stale authorization is refused, nothing lands,
the next pass restarts from a fresh pre-mark and only that write lands), and cross-pass (the first
attempt's write fails; the next pass re-stamps before writing; the landed write observed the fresh
stamp). A reviewer may still ask for an explicit `W_REPOINT` constant; the argument above is the
answer, and it is also in the module doc (`staged.rs:44-61`).

### 2. Rewriting a `part:` record without a constructor

`PartRecord` has no writer-side constructor (`crates/core/src/multipart.rs:2492`) and the brief
puts `multipart.rs` off limits. `repointed_part` (`staged.rs:559`) therefore derives the new value
from the stored bytes: the decoder accepts only the canonical encoding, so the chunk list appears
in the stored bytes exactly as `metadata::encode(&chunks)`; it is replaced with the repointed list
and the result must decode through `decode_part_record` (canonical gate included) before anything
is written. No field name is spelled by hand in production. Ruled out: a constructor in
`multipart.rs` (brief), hand-spelled JSON in the custodian (drifts silently from the codec).

### 3. Resolution rules (reconstruction)

- Read order staged (`sidx:` then `part:`) → committed (`inode:`): a chunk's reference only moves
  forward, so reading each source before its destination sees it on one side of any move that
  lands mid-pass; a chunk published under the pass is never mistaken for a deleted one.
- Committed site wins over a staged one (a published chunk is repaired where readers read it).
- `Open` session → repair; other state → `Deferred("session-fenced")`, obligation kept (repaired
  after publication, `0016:825`); `sidx:`-only → `Deferred("in-flight")`; undecodable session
  record → `Withheld` (NEEDS-HUMAN audit, pass `Blocked`).
- Vacated-source marks are read at **assess** time (`staged.rs:305-321`): an undecodable one
  withholds the move before anything is written or dispatched (D(vi)) — so no dispatched-repair
  counter needs a new offset. Their values are pinned in the adoption CAS.
- Destination choice (`staged.rs:492`) excludes draining servers, servers outside the fleet view,
  and positions whose mark is `reclaiming` or undecodable (never overwritten, ADR-0045), re-running
  the selector until the pick is usable or no free domain is left (terminates: one more exclusion
  per round).
- Drains are withheld while either reading is incomplete (`reconstruction.rs:404`).

### 4. Scrub reads parts of every listed session

Whatever the state — consistent with the staged protection class (#803). An obligation it queues
for a non-`Open` session's part is deferred by reconstruction until the part is published (then
the committed path repairs it) or retired (then it drains). Reading session values in scrub would
have added a decode and a failure mode for no gain.

### 5. Leg F of `staged_protection.rs`

It asserted scrub reads no upload record, with `// deferred: #663, #664` handing the change to
these slices. Rewritten so the drain-status half is still a full guard (no `mpu:`/`sidx:`/`part:`
read, same answer under every damage/fault — #664 owns it), and the scrub half pins the new
contract: lists sessions, reads parts, never reads `sidx:`; `Blocked` on an unreadable part /
part key / session key; `Err` on a fault under the listing or a part range; unaffected by owned
entry damage or faults.

## Evidence

- **C4-verify** (`engine/scripts/run-verify.sh`, the project's gate, patch applied to a clean
  `origin/main` worktree), final patch: GREEN `31 passed`; RED `2 passed; 29 failed`;
  `PASS — red without the fix, green with it (31 test(s) ran red)`. The two that pass on the base
  are guards: `a_staged_chunk_already_whole_drains_its_obligation` (the base drains that one too,
  correctly) and `a_scrub_verifies_a_chunk_named_by_both_classes_once` (on the base the committed
  map alone names the chunk). The first run, before I added tests, was 22/22 red.
- **C4-ci** (`./engine/xtask.sh ci` = `cargo xtask ci`), run twice, the second time on the final
  tree (identical to `patch.diff`): `xtask ci: all checks passed` both times (typos, docs lint,
  guards, fmt, clippy `-D warnings`, workspace build + test, machete, deny, conformance, statics
  gate, deploy guard, madsim clippy + DST 50 seeds). The repo configures no git commit hooks
  (`core.hooksPath` unset, no hook config), so `cargo fmt --all -- --check` and `typos` on the
  changed files are the commit-time checks; both clean.
- **Diff coverage** (`engine/scripts/run-diff-cov.sh`): first run 83.2% (397/477). Every
  rule branch from the brief was covered; I then added tests for the secondary paths it listed as
  missed (vacated-source mark stamped / `reclaiming`, no usable destination, off-fleet and
  unreadable destination, staged Drain / below-k / Blocked / Malformed / `None` scheme, unnamed
  session key, damaged owned entry, scrub's unreadable / malformed parts, the dedupe). Final:
  **96.9% (462/477)**. The 15 lines still missed: `gc.rs:1044-1046` (GC's unnamed-session arm —
  covered by `staged_protection.rs`, which this gate does not measure), `reconstruction.rs:740`
  and `:860` (the committed path's settle return and the `Unreachable` arm — covered by
  `tests/reconstruction.rs`, not measured here), `scrub.rs:338` (the `Owned` arm a parts-only walk
  never meets), `staged.rs:392-395` and `:574-578` (the two defensive branches in the open items).
- **DST** (leg F): `staged_replace_never_strands_under_a_fence` (campaign; the seed draws the fence
  delay in 0..=20 ms) and `staged_replace_reaches_every_fence_window` (coverage: walks all 21
  delays and asserts the fence landed before the pre-mark, between pre-mark and write, between
  write and adoption, and after the adoption). **Seed count: 50** (`MADSIM_TEST_NUM=50`, set by
  `xtask::run_dst`) for each of the two, so the coverage leg makes 50 × 21 = 1,050 runs; plus the
  campaign leg in `committed_regression_seeds_stay_green` over its 8 fixed seeds.
  Timeline the windows rest on: every simulated metadata read is 1 ms, a commit 2 ms, and the D
  server double's write 2 ms each way — so each window has a delay landing strictly inside it.
  The fencer retries its CAS: the simulated-TiKV model locks precondition keys too, so a fence can
  lose a lock race to an in-flight pre-mark/adoption pinned to the same session, as a real
  Complete/Abort would.

### Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With the production files reverted (`git checkout` of `gc.rs`,
  `scrub.rs`, `reconstruction.rs`, `reconciliation.rs`; `staged.rs` then uncompiled) every
  behavioural test in `staged_repair.rs` failed on an assertion, not a compile error (they name
  only base symbols); `run-verify.sh` reported the same independently (29 of 31 red; the two
  guards named above pass on the base, as they should). The DST cases, with production reverted,
  both failed:
  `the obligation outlived the repair, or went without one … left: false right: true` — the base
  drops the obligation without repairing.
- **(b) Production path?** Yes. Every leg calls the production `reconcile_step` with the real
  `ScrubContext` / `ReconstructionContext` / `GcContext`; nothing is re-implemented. Only the
  stores are doubles, and the D-server double enforces the deadline through the seam's own
  `WriteDeadlineExpired::if_elapsed`. The fences/drains/clock moves are injected through store
  hooks, the way the brief's production-reach note says (Abort/Complete do not exist yet).
- **(c) Fixture includes the fault?** Yes. The lost fragment is really missing or really
  bit-flipped on its server; the fence really rewrites the `mpu:` record between write and CAS;
  the drain is a real `desired:dserver:` record; the stale pre-mark is a real mark in the store;
  the refusal comes from the double's real deadline check; the DST fence is a concurrent task
  over the simulated-TiKV model at seed-chosen instants, and the coverage leg proves all four
  windows are actually hit.

## Size

The patch is ~177 KB over 8 files, so the harness's size signal (`patch_kb = 100`) will fire.
Of ~145 KB of added lines, ~85 KB (59%) are tests (`staged_repair.rs` 63 KB, the DST property
18 KB, leg F 4 KB) — the brief asks for one case per rule plus a seeded DST sweep. Production is
~59 KB, of which 59% (34 KB) is comments; `reconstruction/staged.rs` is 33 KB of it.

## Open items for sign-off (not blockers I could resolve inside the brief)

- **Docs currency.** `docs/design/architecture/06-runtime-view.md:80` says "Scrub and the
  drain-status query read committed references only" — now false for scrub. And
  `05-building-block-view.md:202` says nothing writes the multipart records in production —
  reconstruction now rewrites a `part:` record's placement. The rubric makes doc currency a merge
  requirement; the brief lists `docs/` as out of scope (the same sentence also covers drain status,
  which is #664's). Needs a human call: fix in this PR, or in #664.
- `W_WRITE_MILLIS` is a provisional 20 s (see §1).
- The caller-side await on `put_fragment` is bounded by the `ChunkStore` implementation, the same
  rule every custodian await follows (#508/#636); the effect is bounded by the D-server deadline.
- The committed repair path still does not exclude draining servers (unchanged by design; brief).
- Reconstruction now reads every listed session's `sidx:`/`part:` ranges on each pass with a
  non-empty queue — bounded pages, the same cost class GC already pays every pass.
- `RepairOutcome::Refused` for a part record over the value ceiling (`staged.rs:391-396`) and the
  `repointed_part` "does not spell its own chunk list" error are defensive and not reachable from a
  test without forging a >100 KB canonical part record; left uncovered.
- `cargo doc --document-private-items` already fails on the base (private links in public docs,
  redundant link targets); it is not a gate. I removed the two new private links my docs added.
