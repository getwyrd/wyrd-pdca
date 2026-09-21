# custodian: scrub checks committed staged fragments and reconstruction keeps their repair queued (663.1)

> Child 1 of 2 of #663's split (637.3). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `97fc2f9` (verified 2026-09-19). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-scrub-and-keep
- **Kind:** enhancement
- **Defect:** two custodian loops ignore a multipart upload's staged bytes. **Scrub** walks only
  the committed reference set (`crates/custodian/src/scrub.rs:88`, grouped `:130-135`, fetched
  `:137-203`), so a fragment named by a committed `part:` record is never fetched or checked. Rot
  or loss during a staging window that can last hours never becomes a repair obligation, though
  0016 says scrub must check it with the scheme the part record carries (`0016:824`).
  **Reconstruction** resolves an obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk has no committed site, so `assess`
  returns `Drain` (`:613`), the chunk joins `drain_only` (`:218`) and its obligation is deleted
  (`:333-339`). The only record that the chunk is short a fragment is thrown away, and the pass
  can answer `Satisfied`.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_scrub.rs` passes (legs A–C),
  legs D–F appended to the existing `crates/custodian/tests/staged_protection.rs` pass, and
  `cargo xtask ci` is green. All run over in-memory doubles. Records are seeded as raw JSON the
  base decoders accept (shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Legs:
  **(A) Scrub checks committed-part fragments.** An `Open` session has one committed `part:`
  record. Its chunk's fragment on one D server carries one flipped bit (`corrupt_fragment`,
  `crates/custodian/tests/scrub.rs:157-162`). One `reconcile_step` with a `ScrubContext` answers
  `Changed` and leaves that chunk in `wyrd_core::repair::queued_repairs`
  (`crates/core/src/repair.rs:151`). The same holds for a missing fragment, and for an intact
  fragment whose header names a different EC scheme from the part record's `ChunkRef` (this proves
  the part's scheme is the one checked). Control: with every fragment intact, nothing is queued
  and the pass answers `Satisfied`.
  **(B) Scrub leaves in-flight chunks alone.** A chunk named only by an owned `sidx:` entry (no
  `part:` record yet) with a fragment missing queues nothing: checking needs the committed scheme,
  which an in-flight chunk does not have yet (`0016:776-781`).
  **(C) Scrub fails closed on what it cannot read.** With one `part:` record whose value will not
  decode, the pass still checks every other fragment (A's corrupt chunk is still queued), names
  the record on the audit seam, and answers `Blocked` — scrub's rule for an unreadable committed
  map (`scrub.rs:99-116`, `:205-215`). A store fault while reading a session's `part:` range fails
  the pass with `Err`, as it fails GC (`docs/design/architecture/06-runtime-view.md:80`).
  **(D) Reconstruction keeps a staged chunk's obligation.** A committed part's fragment is lost
  and its chunk is enqueued (`enqueue_repair`). One `reconcile_step` with a
  `ReconstructionContext`: the obligation is still queued; the pass answers `Blocked`, as it does
  for a `seg:` repair it refuses (`reconstruction.rs:249-256`, `:341-358`); no D server received a
  write; the `part:` record is byte-identical. The same for an `sidx:`-only chunk. Control: an
  obligation for a chunk that no committed map and no staged record names still drains, and the
  pass answers `Satisfied`.
  **(E) Source before destination.** The pass reads the staged classes before the committed
  namespace, `sidx:` → `part:` → `inode:` (normative, `0016:782-800`; GC's order, `gc.rs:286`,
  `:301`). A store hook (`Meta::hook`, `staged_protection.rs:201`, as leg C uses it at
  `:1150-1468`) publishes the chunk — writes a committed inode naming it and deletes its `part:`
  record — right after the pass's first `inode:` read returns. The obligation must not drain. A
  pass that reads `inode:` first misses the chunk in both classes and drains it. Seed the chunk
  with one fragment lost: #814 drains an intact staged chunk as a duplicate finding, and this leg
  must stay green after it.
  **(F) An unreadable staged record holds back every drain.** With one `part:` record that will
  not decode, an obligation for a chunk that no class names is NOT drained, and the pass answers
  `Blocked`. This is the existing rule for an unreadable committed object
  (`reconstruction.rs:322-339`), applied to the staged read.
- **Falsifiability:** RED in-process on `origin/main` @ `97fc2f9`, no container. Scrub never reads
  `part:`, so A and C fail by assertion; B passes there by design (it guards against scrub
  over-reaching, it is not a red leg). Reconstruction deletes the obligation, so D, E and F fail
  by assertion when their red is run as Verification posture says.
- **Verification posture:** A–C are in the NEW file, so C4-verify proves their red→green. D–F
  are appended to `staged_protection.rs`, a modified file, which C4-verify reverts along with the
  production change, so there they are green-only (C4-ci runs them). They cannot go in the new
  file: they build a `ReconstructionContext`, whose two new fields (Scope item 3) do not exist on
  the red leg's base, and one added test that does not compile makes the whole C4-verify run
  UNVERIFIABLE (`engine/scripts/run-verify.sh:521-547`). So the new file must not build a
  `ReconstructionContext`. Do shows the D–F red by hand: base production code plus the appended
  legs with only the two field initialisers removed, each of D, E and F failing by assertion.
  Record the command and the pass/fail counts in `build-notes.md`.
- **Invariant to restore:** a fragment a staged record places is either present and intact or a
  durable repair obligation, and an obligation is removed only when no record, committed or
  staged, names its chunk. "I could not read a record" never counts as "no record names it".
  Sources: C-1, "a permanent or data-losing failure mode is never an acceptable cost"
  (the harness catalogue `wyrd-pdca/docs/principles.md` §5 and its §6 storage-lifecycle row, not
  a target file; its target sources are 0016's refutation standard, `0016:2802-2813`, and GC's
  "never reclaim a referenced fragment", `crates/custodian/src/gc.rs:47`; a dropped obligation
  leaves a chunk short for good); `0016:824-825`, the read order `:782-800`; scrub's own invariant
  (`scrub.rs:21-25`); proposal 0005's repair contract
  (`docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`); ADR-0045. SELF-TEST: fixing scrub alone queues obligations that reconstruction then deletes
  (D goes red); fixing reconstruction alone leaves rot undetected (A goes red).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 777
- **Ordering note:** first of the pair (#663's split, accepted 2026-09-19 under the human's
  intake-cap override, recorded in `results/issue_663/split-proposal.md`); #814 depends on this
  one. `Conflicts with: 777`: #777 edits `reconstruction.rs`. The briefs of #800, #808, #809 and
  #810 name this id in their own `Conflicts with` (#808 also rewrites leg F of
  `staged_protection.rs` and the same sentence of `06-runtime-view.md:80`); #508 and #625 depend
  on it.
- **Surfaces:** data
- **Difficulty:** high — it touches scrub, reconstruction's drain decision and the staged reader
  GC and restore share, and changes a struct that 9 files in 4 crates build.
- **Scope:** (1) scrub fetches and checks every fragment a committed `part:` record places, with
  the scheme that record carries, inside the loop it already runs (`scrub.rs:137-203`), and reads
  no `sidx:` entry. (2) Reconstruction reads the staged classes before the committed namespace and
  never drains an obligation for a chunk a staged record names: it keeps it, keeps it off the
  repairable-backlog gauge (`reconstruction.rs:175-199`) and names it on the audit seam, like the
  `seg:` refusal (`:1107`). An empty queue still reads nothing (`:161-169`). (3) The seam #814
  reads: `ReconstructionContext` (`reconstruction.rs:72-95`) gains
  `clock: &'a (dyn wyrd_testkit::Clock + Sync)` (ADR-0024, `docs/design/adr/0024-clock-and-time-source-trust.md`;
  `crates/testkit/src/lib.rs:23`) and `staged_write_window_millis: u64`. Use these names; #814's
  brief names them. The deployed loop passes the clock it already advances
  (`crates/server/src/custodian.rs:465-479`, context at `:502-509`) and a window value owned by
  `crates/server/src/cli.rs` as a `pub(crate) const` beside `LEASE_TTL_MILLIS` (`cli.rs:78`),
  passed down the way `GC_GRACE_WINDOW_MILLIS` feeds `GcContext::grace_window_millis`
  (`server/custodian.rs:114`, `gc.rs:200`). If a `W_write` constant already exists on the base
  (#800 names one), use it — never two definitions. Its doc comment states
  `G_orphan > W_repoint + W_write + δ_clock` (`0016:1348`) and that #800's late-write deadline
  must not be sized below it. `wyrd-testkit` moves from dev- to normal dependency where production
  code names `Clock` (as `crates/chunkstore-fs/Cargo.toml:17-19` has it). Update every existing
  construction site. Nothing in this slice reads either field. (4) Keep the prose true: leg F of
  `staged_protection.rs` (`:2035-2209`) now says scrub reads committed parts but no owned entry;
  the last sentence of `06-runtime-view.md:80` and the doc comments that say scrub and
  reconstruction read no staged record (`gc.rs:265`, `:873-876`; `staged_protection.rs:34`,
  `:2160`); narrow the `deferred: #663` marker at `gc.rs:893` to the rebuild, which #814
  removes. Reusing GC's staged reader (`staged_fragments`,
  `gc.rs:1033`; `walk_staged_range`, `:1069`) is expected; what GC and restore conclude must not
  change, so `staged_protection.rs` legs A–E stay green unedited.
  / out of scope: rebuilding or re-placing anything (#814); servers absent from the live fleet
  (scrub has only ever visited `ctx.fleet`, `scrub.rs:138`, and the deployed loop drops
  unreachable peers and reads around them, `server/custodian.rs:495-497` — the same for committed
  chunks today); drain status (#808); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, flip one bit in one of its fragments and run a scrub pass: nothing is queued. Enqueue the
  chunk by hand and run a reconstruction pass: the obligation is gone.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_scrub.rs` — **NEW** (C4-verify's red comes only
  from an added `*/tests/*.rs`, `run-verify.sh:141-144`; `--classify` on a synthetic patch for this
  slice returns `ADDED_TEST crates/custodian/tests/staged_scrub.rs`). Legs D–F go in the existing
  `crates/custodian/tests/staged_protection.rs`, which already has the staged fixtures and the
  store hook.
- **Production reach:** A–F run through the production `reconcile_step`. The two new context
  fields are a seam ahead of their reader: (a) the deployed loop fills them, nothing reads them;
  (b) #814, the next wave of the same run, reads them; (c) they sit here so #814's new test
  compiles on its base and C4-verify can prove #814's red.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY open:
  `scrub.rs:137-203` (the loop to extend); `gc.rs:286-301` and `:1033-1100` (read order and
  staged reader); `reconstruction.rs:249-256`, `:322-358`, `:1107` (the `seg:` refusal and the
  incomplete-reading rule to mirror); `staged_protection.rs:130-280` (the `Meta` double and hook).
  Prior art Do MAY read: v3's `scrub.rs` and `gc.rs` hunks in
  `results/issue_663/iteration-v3/patch.diff` (they passed review; v3 failed on size).
- **Prior-art check (triage cycles):** by path across merged history and open PRs: `scrub.rs` and
  `reconstruction.rs` have never read `part:` or `sidx:`; no open PR touches them (2026-09-19).
  Rejected prior art: #663 v1–v3 and #637 v1, all whole-slice builds.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Human overrode the size backstop's iterate-plan recommendation (123 KB vs 100 KB threshold) and chose iterate-do instead, judging the four findings to be targeted fixes a rebuild can land without re-splitting the slice. Fix on the next attempt: - crates/custodian/src/scrub.rs:144 — read committed `part:` records before `inode:` reads (currently backwards); a publication landing in the gap hides a lost fragment from both classes and the pass wrongly answers Satisfied. Add a publication-race regression test. - crates/custodian/src/gc.rs:1584 — `checked_fragments()` treats an empty/damaged staged placement as valid and invents identity server locations, letting a corrupted part record produce fabricated repair targets. Apply the exact-length validation `StagedSet::place` already uses (gc.rs:1412) and test an empty placement. - crates/custodian/src/reconstruction.rs:211 — `read_committed(...).await?` runs before the staged-corruption audit emit at line 217, so a later inode-store fault throws away attribution of an already-discovered unreadable staged record. Emit staged faults immediately after the staged read succeeds, before any further fallible read. - crates/custodian/tests/staged_protection.rs:2463 — the new reconstruction/publication race is only exercised by a fixed Tokio hook test, not seeded Tier-0/DST coverage, per the standing concurrency rubric (AGENTS.md:188). Extend the seeded staged-handoff campaign (dst/tests/custodian.rs:2813 currently exercises GC only) to drive reconstruction across part commit, publication, and retirement, asserting the obligation survives. Also carry forward two items needing a human decision, not a code fix, on the next round: confirm the prior-art/merged-PR search claim (brief.md:144) and decide fitness-to-purpose given #814 (rebuild) is still deferred (brief.md:121).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human overrode the size backstop's iterate-plan recommendation (123 KB vs 100 KB threshold) and chose iterate-do instead, judging the four findings to be targeted fixes a rebuild can land without re-splitting the slice. Fix on the next attempt:
  - crates/custodian/src/scrub.rs:144 — read committed `part:` records before `inode:` reads (currently backwards); a publication landing in the gap hides a lost fragment from both classes and the pass wrongly answers Satisfied. Add a publication-race regression test.
  - crates/custodian/src/gc.rs:1584 — `checked_fragments()` treats an empty/damaged staged placement as valid and invents identity server locations, letting a corrupted part record produce fabricated repair targets. Apply the exact-length validation `StagedSet::place` already uses (gc.rs:1412) and test an empty placement.
  - crates/custodian/src/reconstruction.rs:211 — `read_committed(...).await?` runs before the staged-corruption audit emit at line 217, so a later inode-store fault throws away attribution of an already-discovered unreadable staged record. Emit staged faults immediately after the staged read succeeds, before any further fallible read.
  - crates/custodian/tests/staged_protection.rs:2463 — the new reconstruction/publication race is only exercised by a fixed Tokio hook test, not seeded Tier-0/DST coverage, per the standing concurrency rubric (AGENTS.md:188). Extend the seeded staged-handoff campaign (dst/tests/custodian.rs:2813 currently exercises GC only) to drive reconstruction across part commit, publication, and retirement, asserting the obligation survives.
  Also carry forward two items needing a human decision, not a code fix, on the next round: confirm the prior-art/merged-PR search claim (brief.md:144) and decide fitness-to-purpose given #814 (rebuild) is still deferred (brief.md:121).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 78.6% — 88 of 112 instrumentable changed lines executed (below the 80% floor); 112 of 432 changed lines we
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the clock source mismatch in custodian.rs:501-505 so ReconstructionContext::clock and the pass's now_millis come from one source, and either prevent or explicitly bound the post-publish scrub/reconstruction flip-flop on stale part: placements. Size backstop overridden by the human — stay as one slice, do not split.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Confirmed defect: a staged chunk with a malformed/empty placement record is skipped by scrub's fragment check but the pass still reports Satisfied, certifying success on a chunk that was never actually verified (crates/custodian/src/scrub.rs:303, crates/custodian/tests/staged_scrub.rs:1091). Both the main reviewer and the adversary reviewer independently confirmed this; the existing regression test asserts the wrong (Satisfied) outcome and needs to require Blocked instead. Separately, the size backstop fired: patch is 196 KB against a 100 KB threshold and this is already round 2. Given the confirmed bug plus a growing list of untested-but-real gaps found by the adversary review (an untested invariant at reconstruction.rs:242, an untested empty-queue read-nothing guarantee, a stale doc line), the slice looks too big to converge with another in-place patch. Re-split at Plan (`pdca split`) rather than iterate-do. The absent-fleet T4 finding is not a defect — both reviewers agree it is out of scope per the brief and matches existing non-staged behavior; no action needed on it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Confirmed defect: a staged chunk with a malformed/empty placement record is skipped by scrub's
  fragment check but the pass still reports Satisfied, certifying success on a chunk that was never
  actually verified (crates/custodian/src/scrub.rs:303, crates/custodian/tests/staged_scrub.rs:1091).
  Both the main reviewer and the adversary reviewer independently confirmed this; the existing
  regression test asserts the wrong (Satisfied) outcome and needs to require Blocked instead.

  Separately, the size backstop fired: patch is 196 KB against a 100 KB threshold and this is
  already round 2. Given the confirmed bug plus a growing list of untested-but-real gaps found by
  the adversary review (an untested invariant at reconstruction.rs:242, an untested empty-queue
  read-nothing guarantee, a stale doc line), the slice looks too big to converge with another
  in-place patch. Re-split at Plan (`pdca split`) rather than iterate-do.

  The absent-fleet T4 finding is not a defect — both reviewers agree it is out of scope per the
  brief and matches existing non-staged behavior; no action needed on it.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
