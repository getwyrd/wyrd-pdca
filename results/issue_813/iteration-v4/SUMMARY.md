# Result — issue 813 / staged-scrub-and-keep

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: two custodian loops ignore a multipart upload's staged bytes. **Scrub** walks only
  the committed reference set (`crates/custodian/src/scrub.rs:88`, grouped `:130-135`, fetched
  `:137-203`), so a fragment named by a committed `part:` record is never fetched or checked. Rot
  or loss during a staging window that can last hours never becomes a repair obligation, though
  0016 says scrub must check it with the scheme the part record carries (`0016:824`).
  **Reconstruction** resolves an obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk has no committed site, so `assess`
  returns `Drain` (`:613`), the chunk joins `drain_only` (`:218`) and its obligation is deleted
  (`:333-339`). The only record that the chunk is short a fragment is thrown away, and the pass
  can answer `Satisfied`.
  **What is still wrong in v3 (the reason for round 4):** scrub answers `Satisfied` for a staged
  chunk it never checked because the chunk's `part:` placement is malformed (v3
  `scrub.rs:303`; the test at v3 `staged_scrub.rs:1091-1096` asserts that wrong answer). Three
  behaviours v3 has are pinned by no test, one audit line loses the record it is about, and one
  doc sentence is false. All are listed under "Round-4 delta". The round-4 plan review found one
  more: this brief's own invariant wording, which three v3 comments copy, forbade base's normal
  committed discharge. v3's code already discharges correctly; the words were wrong, and leg J
  pins the discharge only indirectly (J-discharge).
- Success criterion: the NEW file `crates/custodian/tests/staged_scrub.rs` passes, the legs
  added to the existing `crates/custodian/tests/staged_protection.rs` pass, the seeded DST legs
  pass, and `cargo xtask ci` is green (it is the only command that compiles `crates/dst`, which
  is `--cfg madsim`). All run over in-memory doubles. Records are seeded as raw JSON the base
  decoders accept (shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Leg names are v3's, so each maps to a test that already exists.
  **Legs v3 already has — they must stay green and keep their meaning:**
  **(A) Scrub checks committed-part fragments.** An `Open` session has one committed `part:`
  record. One fragment carries a flipped bit (`corrupt_fragment`,
  `crates/custodian/tests/scrub.rs:157`). One `reconcile_step` with a `ScrubContext` answers
  `Changed` and leaves the chunk in `wyrd_core::repair::queued_repairs`
  (`crates/core/src/repair.rs:151`). The same for a missing fragment, and for an intact fragment
  whose header names a different EC scheme from the part record's `ChunkRef`. Control: all
  intact, nothing queued, `Satisfied`.
  **(A′) A committed map wins over a leftover part record.** A chunk that a committed map names
  is checked where the committed map places it, never where the part record does.
  **(B) Scrub leaves in-flight chunks alone.** A chunk named only by an owned `sidx:` entry with
  a fragment missing queues nothing (`0016:776-781`). Green on base by design; a guard, not a
  red leg.
  **(C) Scrub fails closed on what it cannot read.** One `part:` record that will not decode:
  every other fragment is still checked, the record is named on the audit seam, the pass answers
  `Blocked` (scrub's rule for an unreadable committed map, `scrub.rs:99-116`, `:205-215`). A
  store fault reading a `part:` range fails the pass with `Err`. **(C‴)** the same for a session
  key the parser rejects.
  **(C′) Scrub reads source before destination.** `part:` before `inode:`. A publication that
  lands between the two reads leaves the chunk checked (both hook timings v3 tests).
  **(F, rewritten) Scrub reads committed parts and no owned entry.**
  **(G) Reconstruction keeps a staged chunk's obligation.** A committed part's fragment is lost
  and its chunk enqueued. One `reconcile_step` with a `ReconstructionContext`: the obligation is
  still queued; the pass answers `Blocked`, as for a refused `seg:` repair
  (`reconstruction.rs:249-256`, `:341-358`); no D server received a write; the `part:` record is
  byte-identical. The same for an `sidx:`-only chunk. Control: an obligation no committed map and
  no staged record names still drains, `Satisfied`.
  **(H) Source before destination, for reconstruction.** Staged classes are read before the
  committed namespace, `sidx:` → `part:` → `inode:` (normative, `0016:782-800`; GC's order,
  `gc.rs:419` then `:434`). A store hook (`Meta::hook`, `staged_protection.rs:202`) publishes the
  chunk right after the pass's first `inode:` read. The obligation must not drain. The chunk is
  seeded with one fragment lost, so the leg stays green after #814.
  **(I) An unreadable staged record holds back every drain**, and is named on the audit seam
  even when the committed read then faults.
  **(J) After publication, scrub and reconstruction settle on the committed placement.**
  (v3 `staged_protection.rs:2663`; round 4 adds assertions to it, see J-discharge below.)
  **DST property 13** (`crates/dst/tests/custodian.rs`, v3): seeded part-commit / publication /
  retirement handoffs never drain the obligation, for the GC driver and the reconstruction driver.
  **Round-4 delta — the legs that are new or change:**
  **(C″, CHANGED) A staged chunk whose `part:` placement is malformed is never certified.** The
  placement is empty or the wrong length; the chunk is named by no committed map. The pass queues
  no repair and invents no placement (as v3), names the record on the audit seam, and now answers
  **`Blocked`**, not `Satisfied`: `Satisfied` claims every referenced fragment was checked
  (`crates/custodian/src/reconciliation.rs:32`), and this one was not. Flip the assertion at v3
  `staged_scrub.rs:1091-1096`. Add the counter-case: the same damaged `part:` record, but a
  committed map with a valid placement also names the chunk. That chunk IS checked (leg A′), so
  the damaged leftover record does not make the pass `Blocked`. Whenever a committed map names
  the chunk, the committed map's rule alone decides the answer, whatever the part record holds.
  **(C″-audit, NEW) The audit line names the damaged record.** For a malformed `part:`
  placement the audit seam carries the `part:` key and says it is a staged record. v3 keeps only
  the chunk id (v3 `gc.rs:1650`) and prints the committed-map wording (v3 `scrub.rs:398-407`), so
  an operator goes looking for an inode that does not exist. GC's staged reader already keeps
  the key for this kind of damage (`StagedSet::hold`, `gc.rs:1423`). Assert on the key.
  **(A′-malformed, NEW)** a chunk that a committed map names with a *malformed* placement, and a
  part record also names: scrub does not check the part placement. Deleting the
  `referenced.malformed` half of v3 `scrub.rs:220-221` must turn this leg red.
  **(G-held, NEW) Reconstruction keeps an obligation for a chunk a staged record holds.** An
  `Open` session's `part:` record places an RS(2,1) chunk on only `[0, 1]`: readable, wrong
  length, so the chunk is in `StagedSet::held`, not `placed`. Queue its obligation, run one
  pass: still queued, `Blocked`. Add the `sidx:` twin. Deleting v3 `reconstruction.rs:242` must
  turn this leg red; v3's adversary review found that all 46 staged tests pass without that line.
  **(Empty queue, NEW assertion) An empty queue reads nothing at all.** With nothing queued, a
  reconstruction pass reads no `mpu:`, `sidx:` or `part:` key as well as no `inode:` key, and
  answers `Satisfied` even over a store whose staged read would fault. Extend
  `an_empty_queue_reads_nothing_and_answers_satisfied`
  (`crates/custodian/tests/segmented_map_reconstruction.rs:697-717`); if that file's double cannot
  see reads by prefix, put the leg beside G in `staged_protection.rs`, whose `Meta` can. Calling
  the staged reader in the empty-queue branch (v3 `reconstruction.rs:214-215`) must turn it red.
  Green on base by design; a guard.
  **(J-discharge, NEW assertions in leg J) A committed chunk's obligation is discharged as on
  base, whatever a leftover part record names.** v3's J proves the repair's discharge only
  indirectly (round 2's scrub sees no `repair:` key) and never runs reconstruction over a queued
  obligation for a whole committed chunk. In the same test, with the leftover `part:` record
  still in place and byte-identical at every step: (i) right after round 1's reconstruction
  answers `Changed`, `wyrd_core::repair::queued_repairs` is empty — the repair discharged the
  obligation in its repoint commit although a staged record still names the chunk; (ii) after
  the settle rounds, enqueue the chunk again (`repair::enqueue_repair`,
  `crates/core/src/repair.rs:138`, a duplicate as a health report makes) and run one
  reconstruction pass: it answers `Satisfied`, not `Blocked`, the queue is empty, and no
  fragment was written. Green on base by design (base discharges committed obligations and reads
  no staged record); a guard against the fix reaching too far. Its proof is a mutation: dropping
  from `drain_only` every chunk in `staged_chunks` (v3 `reconstruction.rs:238`) — the literal
  reading of the old wording, "removed only when no record names it" — must turn (ii) red.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the slice as v3 built it, plus the round-4 delta. **Start from
  `results/issue_813/iteration-v3/patch.diff` applied to base, and change only what the delta
  needs. Do not restructure, rename or re-flow what passed review; every round so far grew the
  patch (123 KB → 196 KB), and it must not grow by more than the delta.** If the base Do is
  given has moved and the patch no longer applies cleanly, carry v3's hunks over by hand onto
  that base and say so in `build-notes.md`; that is still not a rebuild. The slice: (1) scrub
  fetches and checks every fragment a committed `part:` record places, for a chunk no committed
  map names (a committed map's placement wins, leg A′), with the scheme that record carries,
  inside the loop it already runs (`scrub.rs:137-203`), reads `part:` before `inode:`, and
  reads no `sidx:` entry. (2) Reconstruction reads the staged classes before the committed
  namespace and never discards an obligation for a chunk that no committed map names while a
  staged record names or holds it: it keeps it, keeps it off the repairable-backlog gauge
  (`reconstruction.rs:175-199`) and names it on the audit seam, like the `seg:` refusal
  (`emit_refused`, `:1107`). A chunk a committed map names is assessed, repaired and discharged
  exactly as on base, whatever a part record says (leg J-discharge; v3's `assess` already
  consults the staged set only when the committed reading has no site, v3
  `reconstruction.rs:734`). Staged faults are named before any later fallible read. An empty queue still reads nothing
  (`:165-169`). (3) The seam #814 reads: `ReconstructionContext` (`reconstruction.rs:72-95`)
  has `clock: &'a (dyn wyrd_testkit::Clock + Sync)` (ADR-0024,
  `docs/design/adr/0024-clock-and-time-source-trust.md`; `crates/testkit/src/lib.rs:23`) and
  `staged_write_window_millis: u64`. Use these names; #814's brief names them. The deployed loop
  (`crates/server/src/custodian.rs:488`, context at `:511`) feeds the context's clock and the
  pass's `now_millis` from one source, and passes `wyrd_custodian::gc::W_WRITE_MILLIS`
  (`gc.rs:201`) as the window — never a second definition. `wyrd-testkit` is a normal dependency
  where production code names `Clock` (as `crates/chunkstore-fs/Cargo.toml:17-19`). Every
  construction site is updated (9 files). Nothing in this slice reads either field. (4) **The
  round-4 delta:** the behaviours of legs C″, C″-audit, A′-malformed, G-held, the empty-queue
  assertion and J-discharge. (5) Keep the prose true: the three v3 comments that state the old
  wording, "removed / discarded only when no record, committed or staged, names its chunk" (v3
  `reconstruction.rs:237`, `:293`; v3 `crates/dst/tests/custodian.rs:2597`), reworded to the
  discharge / discard split in `Invariant to restore`, with no change to the code around them;
  the module-doc sentence at `gc.rs:89-90` ("Scrub and the
  drain-status surface do not read the class at all" — false for drain status since #808, and
  for scrub and reconstruction after this slice); `gc.rs:396`; `staged_protection.rs:34` and the
  `deferred: #663` markers at `staged_protection.rs:2159` and `gc.rs:1316`, narrowed to the
  rebuild that #814 owns; the last sentence of `06-runtime-view.md:80` ("Scrub reads committed
  references only."). (6) **State the re-queue bound, do not fix it.** One case remains after
  v3: an object is deleted or overwritten before its `part:` records are retired, after
  reconstruction had moved one of its fragments. The leftover part record then names the empty
  old position; scrub re-queues the chunk each pass and reconstruction keeps it and answers
  `Blocked` each pass, until the `retire:records:` drain deletes the record. Say so, and name
  that drain as the bound, in scrub's module doc and in `06-runtime-view.md:80`. No production
  code on main writes a `part:` record yet (`part_key` has no caller outside
  `crates/core/src/multipart.rs`), so this is forward-looking. What GC and restore conclude must
  not change: `staged_protection.rs` legs A–E stay green unedited.
  / out of scope: rebuilding or re-placing anything (#814); closing the re-queue case in (6);
  the rule for a malformed COMMITTED placement, which keeps answering as it does today
  (`scrub.rs:95-96`, pinned by `short_placement_is_malformed_scrub_fails_safe`,
  `crates/custodian/tests/scrub.rs:788`) — the asymmetry with C″ is a known, human-accepted
  decision for this slice; servers absent from the live fleet (scrub has only ever visited
  `ctx.fleet`, `scrub.rs:138`, and the deployed loop drops unreachable peers and reads around
  them, `crates/server/src/custodian.rs:249-265` — the same for committed chunks today); a
  scrub driver in the DST sweep (decided at v1's sign-off: scripted hook tests for scrub, seeded
  coverage for reconstruction); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (16 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.5% — 189 of 200 instrumentable changed lines executed (floor 80%); 200 of 721 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 39 mutants tested in 6m: 22 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.91s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #813: scrub committed multipart parts for damage and preserve staged repair obligations without preventing normal committed-chunk discharge.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The discharge/discard distinction and malformed-part counter-case define falsifiable outcomes without reopening accepted scope decisions; brief.md:79, brief.md:144; crates/custodian/src/reconciliation.rs:28. |
| C2 Reproduction (red pre-fix) | PASS | Independent base-code runs reproduce 11 scrub assertion failures and seven reconstruction retention/handoff/audit failures; reviewer-red-green.log:113; reviewer-reconstruction-red.log:62, reviewer-reconstruction-red.log:94. |
| C3 Change | PASS | Staged-only damage remains actionable while committed maps retain authority over published chunks; crates/custodian/src/scrub.rs:236; crates/custodian/src/reconstruction.rs:728. |
| C4 Verification (red→green) | PASS | Restored code passes all 16 scrub and 36 protection tests plus seeded DST; full frozen CI passes, with the independent CI advisory-lock limitation documented below; reviewer-mutations.log:231; reviewer-mutations.log:253; reviewer-ci-remaining.log:532; gate-logs/C4-ci.log:3678. |
| C5 Causal adequacy | PASS | Source-before-destination reads close the handoff gap, and six independently killed mutations demonstrate the required boundaries; no capability probe conceals an eager/load-time cause; crates/custodian/src/scrub.rs:142; crates/custodian/src/reconstruction.rs:217; reviewer-mutations.log:2. |
| T1 Structure | PASS | Shared staged placement validation preserves GC/restore semantics, and the deployed context and pass use one clock source; crates/custodian/src/gc.rs:1461; crates/server/src/custodian.rs:148; crates/server/src/custodian.rs:570. |
| T2 Shape | PASS | Audit evidence identifies damaged part records, and the living architecture states the interim behavior and requeue bound; crates/custodian/tests/staged_scrub.rs:1143; docs/design/architecture/06-runtime-view.md:82; the 218,237-byte patch remains covered by brief.md:291's explicit size override. |
| T3 Runtime | PASS | Empty queues avoid namespace reads; paged staged reads fail closed, held chunks remain queued, and committed duplicates still discharge; crates/custodian/src/gc.rs:1601; crates/custodian/tests/staged_protection.rs:2543; crates/custodian/tests/staged_protection.rs:2906. |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit must rerun at publish; gate-logs/T4-contribution.log:10; the separate frozen batch-review and TiKV compile checks pass. |
| T5 Judgment | PASS | File-path prior-art searches found no competing staged scrub/retention implementation; closed overlaps concern segmented maps and DST, while the brief's recorded deferrals remain settled; reviewer-prior-art.json:3, reviewer-prior-art.json:990; brief.md:278. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept detection and obligation retention as sufficient until #814 supplies staged reconstruction — redundancy is not restored by this slice, and obsolete part records can keep requeueing until retirement; crates/custodian/src/reconstruction.rs:704; docs/design/architecture/06-runtime-view.md:82. |

No in-scope implementation defect was found. The independent red→green and mutation results support the patch; the remaining decision is fitness to purpose within the already accepted scope.

- **Independent behavioral evidence:** In the supplied disposable target, `git stash push` retained the added scrub test while restoring base production; 11 tests failed by assertion and five controls passed. After `git stash pop`, all 16 passed. Reconstruction was separately replayed against base production with only the two new context-field initializers omitted from the patched test file: five G/G-held/H tests and two I tests failed by assertion, then all 36 protection tests passed after restoration. Commands and full output are in reviewer-red-green.log:1 and reviewer-reconstruction-red.log:1. Source citations refer to the restored target; the adapted red test's line numbers are two lines earlier.
- **Round-four sensitivity:** Removing malformed-part blocking, erasing its audit key, removing malformed committed-map precedence, excluding held chunks, reading staged records on an empty queue, and retaining committed duplicates each caused the corresponding assertion failure. The final restored run passed all 52 tests (reviewer-mutations.log:2, :39, :72, :98, :135, :161, :187). The frozen broader mutation campaign reports 22 caught and 17 unviable mutants, with no survivors (gate-logs/C5-mutants.log:13).
- **Gate evidence and host limitation:** The independent `cargo xtask ci` reran typos, docs lint/render, hygiene checks, fmt, clippy, build, workspace tests and dependency-use checks successfully, then stopped because the sandbox denied an exclusive lock on the read-only advisory database (reviewer-ci.log:2979). This is a host caveat, not a patch defect. Independent conformance, statics and `cargo xtask dst` subsequently passed, including both handoff drivers and their schedule-coverage assertions (reviewer-ci-remaining.log:1, :4, :532, :546). `deploy-guard` is an internal CI step, not an exposed xtask subcommand; the direct invocation was rejected as unknown, and its frozen result passes (gate-logs/C4-ci.log:3092). The complete frozen CI confirms the remaining dependency/security scans and both declared external tools actually ran (gate-logs/C4-ci.log:11, :16, :3059, :3678). Frozen diff coverage is 189/200 instrumentable changed lines, with 521 unscored lines; it is not whole-patch coverage (gate-logs/C4-diff-cov.log:883, :907). The TiKV crate and server feature paths compiled (gate-logs/host-tikv.log:110, :209); batch review reports zero blocking findings (gate-logs/T4-batch-review.log:10). Instance-scoped coverage/mutation/review wrappers were adjudicated from those logs; the additional TiKV feature compile was also log-adjudicated, not independently rerun.
- **Prior art and scope:** Despite the synthetic target having one commit and no remote, read-only GitHub API queries checked merged history at the pinned base for all 19 affected paths and file lists for all 16 closed-unmerged PRs. No open PRs were returned. Twenty merged commits touching scrub/reconstruction introduced no staged-reader calls or prefixes; closed overlaps were #647 (segmented maps) and #336 (DST), neither implementing this change (reviewer-prior-art-reader-history.json:1; reviewer-prior-art-pr-647.json:1; reviewer-prior-art-pr-336.json:1). Internal rejected iterations are recorded by the brief, rather than independently available here (brief.md:283). The accepted fleet, malformed-committed-placement, staged-rebuild and size boundaries are not reopened. Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up are warranted for custodian durability changes; those privileged scenarios were not run here and remain non-gating (`cargo xtask disk-faults`, `cargo xtask kill-reconstruct`; AGENTS.md:78).

The target was restored after every replay and mutation. Reverse patch applicability and `git diff --check` pass, and the stash is empty. No implementation change is proposed by this advisory review.

### Advisory — adversary

# Adversarial review — issue 813 (staged-scrub-and-keep), round 4

**Verdict: I could not refute the production fix.** I found two test gaps the builder can close,
and one gate result a human should confirm. None of them is a production correctness bug, so the
brief's stop rule (a new production bug means `iterate-plan`) is not triggered.

What I re-ran myself, on a scratch copy of the patched tree (since deleted):

- Patched tree: `staged_scrub` 16/16 and `staged_protection` 36/36 green.
- By-hand red for G, G-held, H and I, done the way the brief describes: base production code, the
  patched `staged_protection.rs` with the two new `ReconstructionContext` field lines removed. 8 of
  36 failed by assertion: G ×2, G-held ×2, H, I ×2 and F. J, the empty-queue leg and the no-class
  control stayed green, which is what the brief says should happen.
- Every mutation the brief names turns its leg red: dropping `.chain(staged.held…)`
  (`crates/custodian/src/reconstruction.rs:244`) fails both G-held legs; reading the staged class
  in the empty-queue branch (`:214`) fails the empty-queue leg; dropping staged chunks from
  `drain_only` (`:308`) fails J at `crates/custodian/tests/staged_protection.rs:2906`; dropping the
  `referenced.malformed` half (`crates/custodian/src/scrub.rs:237`) fails A′-malformed.
- Two extra mutations were also caught: taking `staged_kept` out of `hole` fails 5 legs, and taking
  `!staged_incomplete` out of the drain gate fails leg I.

## Findings

- NEEDS-HUMAN [impl] — **No test covers scrub's staged reader past the first page of upload
  sessions.** `staged_committed_parts` has its own copy of the `mpu:` session paging loop
  (`crates/custodian/src/gc.rs:1600-1618`). Only the inner `walk_staged_range` is shared with GC.
  `gc.rs:1616` (`after = Some(last)`) is a MISS in `gate-logs/C4-diff-cov.log`. I changed that arm
  to return after page 1, and the **whole `wyrd-custodian` test suite still passed** (0 failures).
  Concrete case: with 513 or more sessions (`STAGED_PAGE = 512`, `gc.rs:300`), a rotten fragment in
  the 513th session's part would be skipped and scrub would answer `Satisfied` if this loop ever
  regressed. Today's code is correct; only the guard is missing. The comment at
  `crates/custodian/tests/staged_scrub.rs:111-112` ("the paged-range legs are GC's, over the same
  shared helpers") is only half true, because the session loop is not shared. Fix: add a scrub leg
  with more sessions than one page, or a lowered scan cap, like GC's (D) leg at
  `staged_protection.rs:1534`.
- NEEDS-HUMAN [impl] — **No test pins scrub's promise to name staged damage before the committed
  read.** `crates/custodian/src/scrub.rs:147-163` says the malformed and unreadable part records are
  named "before the committed read's own `?` can carry the names away". I moved both emit loops
  (`:152`, `:161`) to after `referenced_fragments` (`:174`). All five suites that touch this code
  still passed (`staged_scrub`, `staged_protection`, `segmented_map_reconstruction`,
  `reconstruction`, `scrub`). Reconstruction has a test for its version of this promise
  (`an_unreadable_staged_record_is_named_even_when_the_committed_read_then_faults`,
  `staged_protection.rs:2715`); scrub has none. Concrete case: an undecodable `part:` record plus a
  store fault on `inode:`. The pass returns `Err`, and with the emits moved the record is never
  named. Fix: add the scrub version of that test in `staged_scrub.rs`.
- NEEDS-HUMAN [human] — **The T4 batched-review pass may be stale or short-circuited.**
  `check-gates.json` reports `T4-batch-review` as "0 blocking, 0 recorded-rejected, 0
  noise-dropped" after 33.15 s, for three review passes over a 218 KB diff
  (`gate-logs/T4-batch-review.log`). That is quick for three fresh passes. The brief also says
  finding (a) (fleet-absent D server; the fleet walk is at `scrub.rs:255`) must be recorded in
  `review-rejected.md`, and the gate shows no recorded rejection. Either this round's review did not
  raise (a) again, or the rejection was not recorded. From the inputs I have, I can't tell which. A
  human should check that `results/issue_813/review-batch.md` was produced from this patch.
- **C4-verify overstates its own count** (this is about the gate's report, not the fix). The
  `path_line` says "16 test(s) ran red", but `gate-logs/C4-verify.log` shows 11 failed and 5 passed
  with production reverted. The 5 that pass on base are B, the intact control, both A′ legs and
  A′-malformed. The brief expects exactly those to pass on base, so the red→green proof holds. The
  summary line is just wrong.
- **The diff-coverage figure mostly measures scrub, not reconstruction.** C4-diff-cov ran only
  `--test staged_scrub` for `wyrd-custodian`, so `reconstruction.rs:226-227` and `:1238-1247` show
  as MISS even though leg I runs them in CI. The 94.5% says little about the reconstruction half.
  For that half, the evidence is the by-hand red above.

## Refutation attempts that failed

- **Publication race, both hook timings (scrub and reconstruction):** the source-first read order
  holds, and the DST property 13 reconstruction driver calls the production `reconcile_step`, with
  a control obligation that must drain. The test is not a tautology.
- **A committed map and a part record name the same chunk (valid, malformed or unreadable
  committed map):** the committed rule decides every time (`scrub.rs:236-243`,
  `reconstruction.rs` `assess`). An unreadable committed object makes the pass `Blocked` whatever
  the part record says.
- **J-discharge over-reach:** a whole committed chunk's duplicate obligation drains with the pass
  `Satisfied`, and the mutation named in the brief is caught.
- **Aborting sessions:** their part fragments are still checked and their obligations still kept
  until retirement. That is bounded, and it matches GC's "whatever state the upload is in".
- **Clock seam (ADR-0009):** `LoopClock` (`crates/server/src/custodian.rs:146-169`, struct at `:146`, `Clock` impl at `:165`) is the only
  time source for both a pass's `now_millis` and `ReconstructionContext::clock`. No lock is held
  across an `.await`, and the context field is not read yet.
- **A trade-off, not a bug:** a store fault on the staged ranges now fails the whole reconstruction
  pass (`reconstruction.rs:217`, `?`). Before this patch, committed repairs were not affected by
  such a fault. GC already behaves this way, the brief requires it, and the fault would usually hit
  the `inode:` read in the same pass too. I don't count it as a refutation.

### Advisory — code-review

No actionable findings on either advisory lens: introduced correctness bugs, or reuse, simplification and efficiency.

Reviewed staged reads and placement validation (`crates/custodian/src/gc.rs:1593`), scrub precedence and certification (`crates/custodian/src/scrub.rs:236`), reconstruction retention and discharge (`crates/custodian/src/reconstruction.rs:240`), clock wiring, and the changed regression/DST tests against the target source. The brief's settled scope exclusions were respected.

Validation relied on the frozen gate evidence; no builds were rerun. CI passed, all 16 staged-scrub tests passed with the patch (11 failed by assertion without it), and mutation testing reported 22 caught and 17 unviable mutants, with none surviving.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept detection and obligation retention as sufficient until #814 supplies staged reconstruction — redundancy is not restored by this slice, and obsolete part records can keep requeueing until retirement; crates/custodian/src/reconstruction.rs:704; docs/design/architecture/06-runtime-view.md:82.
- [ ] **No test covers scrub's staged reader past the first page of upload
- [ ] **No test pins scrub's promise to name staged damage before the committed
- [ ] **The T4 batched-review pass may be stale or short-circuited.**
- [ ] **The removal invariant forbids normal successful reconstruction.** `brief.md:125-127` says an obligation is removed “only when no record, committed or staged, names its chunk”; `brief.md:167-170` likewise unconditionally retains obligations named by staged records. On the pinned target, `crates/custodian/src/reconstruction.rs:705-707` drains a fully healthy committed chunk, and `:938-941` deletes the obligation atomically with a successful committed-map repoint. The existing test at `crates/custodian/tests/reconstruction.rs:386-387` requires that deletion. Thus the stated invariant conflicts with keeping CI green and with leg J’s committed-placement precedence (`brief.md:71`), particularly while a leftover part record still exists. Revise the brief to distinguish discarding an unresolved obligation from discharging a verified or repaired committed chunk: preserve normal committed repair/drain behavior, and retain staged obligations when no authoritative committed site resolves them. Make leg J explicitly assert queue discharge after committed repair or full-redundancy verification, including a leftover part record.
- [x] size backstop — this slice is behaving oversized: patch is 213 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. — Human overrides the size backstop again for this round: reaffirms the 2026-09-21 decision that a further split would still leave an oversized child; keep as one slice.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Round 5 scope: close the two test-gap findings from the round-4 adversary review only. (1) Add a scrub test for staged-record paging past the first page of upload sessions (STAGED_PAGE = 512; gc.rs:1600-1618's session loop is not shared with GC's paging, unlike walk_staged_range). (2) Add a scrub test pinning that staged damage is named on the audit seam before a later committed-read fault (scrub.rs:147-163's ordering promise; reconstruction already has this test, scrub does not). Both are test-only gaps — no production code changed. Everything else in this bundle stands: the size backstop (213 KB vs 100 KB) is overridden again by the human, reaffirming the 2026-09-21 decision; do not re-split. Fitness-to-purpose and the T4 batch-review staleness question were left open by the human as not needing a decision at this point — do not treat their absence as a rejection.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
