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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (18 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.0% — 190 of 200 instrumentable changed lines executed (floor 80%); 200 of 721 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 39 mutants tested in 6m: 22 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.95s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in issue #813’s staged-part scrub and repair-obligation retention, including the paging and audit-order regressions; fitness and prior-art acceptance remain human judgments.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required protection, committed-reference precedence, and final two test gaps are explicit and falsifiable; staged rebuilding remains the accepted #814 follow-up (`brief.md:144`, `brief.md:320`). |
| C2 Reproduction (red pre-fix) | PASS | Independent execution on the pre-fix tree produced assertion failures in 13/18 scrub tests and 8/36 staged-protection tests, including lost staged obligations (`reviewer-evidence/red.log:89`, `reviewer-evidence/red.log:114`, `reviewer-evidence/red.log:237`). |
| C3 Change | PASS | The patch covers the specified gaps without extending staged repair scope; current-tree tests pin multi-page coverage and attribution before a failing committed read (`crates/custodian/tests/staged_scrub.rs:936`, `crates/custodian/tests/staged_scrub.rs:1004`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all 54 focused tests pass; workspace and DST reruns pass, with the complete CI verdict supported by frozen evidence where the local advisory-cache lock prevented completion (`reviewer-evidence/green.log:46`, `reviewer-evidence/green.log:70`, `gate-logs/C4-ci.log:3680`). |
| C5 Causal adequacy | PASS | The omitted-reference cause is addressed, while committed obligations still discharge; no capability probe or load-time symptom guard was introduced (`crates/custodian/src/reconstruction.rs:728`, `crates/custodian/src/scrub.rs:236`, `crates/custodian/tests/staged_protection.rs:2888`). |
| T1 Structure | PASS | The change preserves metadata/trait boundaries, reuses staged placement validation, and gives the deployed pass and future deadline reader one clock source (`crates/custodian/src/gc.rs:1461`, `crates/server/src/custodian.rs:542`, `crates/server/src/custodian.rs:570`). |
| T2 Shape | PASS | The 224,956-byte patch remains within the explicitly overridden slice; the living architecture documents both current behavior and the accepted retirement-bounded re-queue case (`brief.md:320`, `docs/design/architecture/06-runtime-view.md:82`). |
| T3 Runtime | PASS | Independent runs exercise pages beyond the first, held chunks, empty queues, and concurrent publication handoffs; seeded checks prove the dangerous read windows are reached (`crates/custodian/tests/staged_scrub.rs:1035`, `crates/dst/tests/custodian.rs:3158`, `reviewer-evidence/remaining-checks.log:534`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit is owed at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Accept the documented path-based prior-art search or supply independently inspectable history — this target has one synthetic commit and no remote, so merged and closed/rejected alternatives cannot be independently settled here (`brief.md:278`, `reviewer-evidence/grounding.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this slice is fit to advance with detection and durable retention before #814 restores staged redundancy — obligations can remain Blocked, and leftover placements can re-queue until record retirement (`brief.md:289`, `docs/design/architecture/06-runtime-view.md:82`). |

The review found the two requested test gaps closed and no new production correctness defect. The independent red→green reproduction, committed-discharge counter-case, and seeded handoff coverage support that conclusion. The accepted size override, absent-fleet exclusion, malformed committed-placement behavior, and tracked #814 deferral are not reopened.

Source citations above refer to the patched `$PDCA_TARGET`; evidence and brief citations refer to this review directory. The target is current for this patch: reverse application checks successfully, and the tracked diff was restored byte-for-byte after reproduction (`reviewer-evidence/grounding.log:14`). No builder notes or other checkouts were consulted.

Independent reproduction used `git stash` to restore production, retained the new scrub test file, and retained the staged-protection fixtures with only the two context field initializers absent on base removed, as the brief prescribes. `cargo test --offline -p wyrd-custodian --test staged_scrub --test staged_protection --no-fail-fast` then produced 21 assertion failures and 33 passing controls. After restoring the original patch, the identical command passed all 54 tests. This reproduces both detection and obligation retention; the failures were not compilation failures (`reviewer-evidence/red.log:48`, `reviewer-evidence/red.log:141`, `reviewer-evidence/green.log:46`).

The gate evidence supports these dispositions:

- **C4-ci: PASS from combined evidence, with a local host caveat.** The independent `cargo xtask ci` completed typos, docs lint/render and link audit, guards, fmt, workspace Clippy/build/tests, and cargo-machete. It then failed because cargo-deny could not lock `/home/eddie/.cargo/advisory-dbs/db.lock`, a read-only sandbox path; an offline retry encountered the same restriction (`reviewer-evidence/ci.log:2985`, `reviewer-evidence/deny-offline.log:1`). This is not a patch defect and is not a claim of a fully green local CI rerun. The frozen log explicitly shows all three dependency-wall checks passing and the final complete CI success (`gate-logs/C4-ci.log:3074`, `gate-logs/C4-ci.log:3085`, `gate-logs/C4-ci.log:3088`, `gate-logs/C4-ci.log:3680`). Separate independent conformance, statics, and full DST Clippy/test runs passed (`reviewer-evidence/remaining-checks.log:6`, `reviewer-evidence/remaining-checks.log:13`, `reviewer-evidence/remaining-checks.log:600`). The frozen deployment scanner also reports success (`gate-logs/C4-ci.log:3095`).
- **C4-verify: PASS, independently reproduced.** The frozen log’s “18 test(s) ran red” means 18 tests executed on base: its actual result is 13 failed and five passed, matching the independent scrub result (`gate-logs/C4-verify.log:133`, `reviewer-evidence/red.log:237`).
- **C4-diff-cov: PASS from frozen evidence.** It reports 190/200 instrumentable changed lines covered, 95.0%; 521 other changed production lines were unscored. This is not 95% coverage of every changed line (`gate-logs/C4-diff-cov.log:885`, `gate-logs/C4-diff-cov.log:908`).
- **C5-mutants: PASS from frozen evidence.** The log reports 22 caught and 17 unviable mutants, with no survivors; unviable mutants are not counted as behavioral proof (`gate-logs/C5-mutants.log:13`).
- **T4-batch-review: PASS as reported by its frozen log.** It records zero blocking, rejected, or noise-dropped findings; the underlying review artifact is not among these inputs (`gate-logs/T4-batch-review.log:10`). This review independently examined the current patch.
- **host-tikv: PASS, independently rerun.** Both metadata-TiKV and server `tikv,etcd` Clippy invocations compiled their tests successfully (`reviewer-evidence/remaining-checks.log:705`, `reviewer-evidence/remaining-checks.log:808`).
- **T4-contribution: N/A until publish.** Its required later audit remains owed; the deferred result is not a failure or a request for a human waiver (`gate-logs/T4-contribution.log:10`).

The instance-scoped coverage, mutation, and review wrappers are absent from this disposable target as expected; their complete supplied logs were used rather than treating wrapper absence as a defect (`reviewer-evidence/grounding.log:20`). Both declared external tools were actually exercised, including rendering 99 documentation pages with a successful link audit (`reviewer-evidence/ci.log:6`, `reviewer-evidence/ci.log:13`).

Prior-art acceptance is the remaining evidence judgment: the brief records affected-path searches of merged history, closed-unmerged #647, and rejected #663/#637 iterations. Running the path-filtered history query here returns only the synthetic base commit, and `git remote -v` returns nothing (`reviewer-evidence/grounding.log:7`). This limitation does not establish a duplicate patch or rejected design.

Tier-1 disk-fault and Tier-2 kill-and-reconstruct follow-up is warranted for the custodian durability rollout, as the standing rubric requests; those real-environment scenarios were not exercised here. They complement the passing Tier-0 checks and do not establish staged reconstruction before #814 (`AGENTS.md:78`, `crates/custodian/src/reconstruction.rs:58`).

### Advisory — adversary

# Adversarial review — issue 813, round 5 (staged-scrub-and-keep)

**Verdict: I tried to refute this and could not.** Round 5 adds two tests and changes no
production code. Both tests catch the defect they claim to pin. Every round-4 mutation the brief
names is still caught. None of the production inputs I built against scrub or reconstruction broke
the fix. Toolchain was present: I rebuilt the patched tree in a scratch copy, ran
`staged_scrub` (18/18) and `staged_protection` (36/36) green, then ran each mutation below
against it and put the file back each time.

## The evidence (red→green)

- **Weak spot in the gate evidence, then closed by hand.** Both round-5 tests go red on base
  only because base scrub reads no `part:` record at all (`gate-logs/C4-verify.log`). Any test
  that needs scrub to read a part record fails there, so C4-verify alone does not show that
  these two tests pin *paging* or *emit-before-the-committed-read*. I ran the mutations that
  would:
  - **A-paged.** Make scrub's own session loop check only the first page's sessions while
    still listing every page (`crates/custodian/src/gc.rs:1602`, add
    `if after.is_some() { break; }`). Result: `scrub_checks_committed_parts_of_sessions_past_the_first_page`
    goes red at `crates/custodian/tests/staged_scrub.rs:1027`. The lost chunks sit at session 0,
    512 and 1024 (the first session of each page, and the only one on page 3), so skipping the
    first or last session of a later page would also be caught. The page-count assertion
    (`staged_scrub.rs:1035-1041`) fails loudly if `STAGED_PAGE` (`gc.rs:300`) changes in either
    direction.
  - **C-order.** Move both staged emit loops (`crates/custodian/src/scrub.rs:152-163`) to after
    `referenced_fragments` (`scrub.rs:174`). Result: red at `staged_scrub.rs:969`. Moving only
    the unreadable-record loop turns it red at `staged_scrub.rs:978`, so each record is pinned
    on its own. I found no route by which the test passes for the wrong reason: if the fault hit
    the staged read instead, nothing would be emitted and the test would fail.
- **The round-4 mutations the brief names, re-run on this tree. All were caught:**
  - dropping `.chain(staged.held…)` (`crates/custodian/src/reconstruction.rs:244`) turns both
    G-held legs red;
  - dropping the `referenced.malformed` half of the supersede check (`scrub.rs:237`) turns
    A′-malformed red;
  - calling `staged_fragments` in the empty-queue branch (`reconstruction.rs:215`) turns the
    empty-queue leg red;
  - keeping every `Drain` whose chunk a staged record names (`reconstruction.rs:308`) turns
    J-discharge red at `crates/custodian/tests/staged_protection.rs:2900`.
- **Extra mutations, also caught:**
  - removing `!staged_incomplete` from the drain gate (`reconstruction.rs:436`) turns
    `an_unreadable_staged_record_holds_back_every_drain` red;
  - neutering `staged_kept` in the certification check (`reconstruction.rs:451`) turns 5 G/H
    legs red;
  - removing `!staged.unresolvable.is_empty()` from scrub's answer (`scrub.rs:326`) turns C and
    C‴ red.
- **A misstated gate claim, verdict unaffected.** The C4-verify row in `check-gates.json` says
  "18 test(s) ran red". The log shows 13 failed and 5 passed on the red leg. The 5 are B, the
  intact control, the two A′ legs and A′-malformed, all green on base by design as the brief
  says. The PASS stands, but the count in the summary text is wrong. This is the harness's
  wording, not something the patch can fix.
- **Diff-coverage misses are a gap in what the gate measures.** The gate reports misses at
  `reconstruction.rs:226-227` and `:1238-1247`. It measures the custodian crate only through
  `--test staged_scrub` (`gate-logs/C4-diff-cov.log:14`). Leg I
  (`staged_protection.rs:2715`) runs those lines, and the drain-gate mutation above shows the
  path is pinned.

## The fix: inputs I tried that did not break it

- **Zero-fragment scheme.** A part record with `ReedSolomon{k:0,m:0}` and `placement: []` would
  pass `staged_placement`'s exact-length check (0 == 0). It would put no fragments in the set and
  mark nothing malformed, so scrub would certify a chunk it never checked. Blocked earlier: the
  decoder rejects it (`checked_chunk_scheme`, `crates/core/src/multipart.rs:2359-2369`), so the
  record becomes unresolvable and the pass answers `Blocked`.
- **A multipart upload published as a segmented object.** `referenced_fragments` resolves
  `seg:` chunks into `schemes` (`gc.rs:1234-1273`), so scrub's "committed map wins" check
  (`scrub.rs:237`) covers them. Reconstruction returns `Refused` for them before it looks at the
  staged set.
- **A part re-commit, an abort, or a retirement landing mid-pass.** Each one yields at worst a
  spurious enqueue, which the next reconstruction pass drains once no record names the chunk.
  Base already behaves this way when a committed object is deleted mid-scrub. Nothing new.
- **The same chunk under two part records with different schemes.** `placed` keeps one scheme
  per `(dserver, fragment)`. That is the same one-scheme-per-chunk shape as
  `ReferenceSet::schemes` on base, and a mismatch fails safe by enqueueing a repair.
- **Settled, not re-raised:** fragments on servers outside `ctx.fleet` (review finding (a),
  rejected by the human); the malformed-committed-placement asymmetry; the re-queue bound after
  delete-before-retire; one unreadable staged record stopping every drain. The brief states all
  four as accepted decisions.

## Where the reviewer might have rationalized

- The T4 batch review took 34 s and returned 0 findings over a patch of about 225 KB
  (`gate-logs/T4-batch-review.log`). That is thin for three multi-pass reviews. The human left
  the T4 staleness question open on purpose (brief, carry-forward), so I note it here and do not
  raise it again as a NEEDS-HUMAN item. My own mutations back up the test-strength claims that a
  review would have had to take on trust.

No NEEDS-HUMAN items from this pass.

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found within the diff and the brief’s accepted scope.

Reviewed staged-read ordering and placement handling (`crates/custodian/src/gc.rs:1593`, `crates/custodian/src/scrub.rs:236`), reconstruction retention and committed discharge (`crates/custodian/src/reconstruction.rs:436`, `crates/custodian/src/reconstruction.rs:728`), and the new audit-order and session-paging tests (`crates/custodian/tests/staged_scrub.rs:936`, `crates/custodian/tests/staged_scrub.rs:1004`).

Validation used the frozen gate evidence: CI including DST passed; all 18 staged-scrub tests passed, with 13 failing against reverted production; mutation testing reported 22 caught and 17 unviable. No builds or tests were rerun; the target was read only.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Accept the documented path-based prior-art search or supply independently inspectable history — this target has one synthetic commit and no remote, so merged and closed/rejected alternatives cannot be independently settled here (`brief.md:278`, `reviewer-evidence/grounding.log:3`).
- [x] Validation — fitness-to-purpose — Confirm this slice is fit to advance with detection and durable retention before #814 restores staged redundancy — obligations can remain Blocked, and leftover placements can re-queue until record retirement (`brief.md:289`, `docs/design/architecture/06-runtime-view.md:82`).
- [x] **The removal invariant forbids normal successful reconstruction.** `brief.md:125-127` says an obligation is removed “only when no record, committed or staged, names its chunk”; `brief.md:167-170` likewise unconditionally retains obligations named by staged records. On the pinned target, `crates/custodian/src/reconstruction.rs:705-707` drains a fully healthy committed chunk, and `:938-941` deletes the obligation atomically with a successful committed-map repoint. The existing test at `crates/custodian/tests/reconstruction.rs:386-387` requires that deletion. Thus the stated invariant conflicts with keeping CI green and with leg J’s committed-placement precedence (`brief.md:71`), particularly while a leftover part record still exists. Revise the brief to distinguish discarding an unresolved obligation from discharging a verified or repaired committed chunk: preserve normal committed repair/drain behavior, and retain staged obligations when no authoritative committed site resolves them. Make leg J explicitly assert queue discharge after committed repair or full-redundancy verification, including a leftover part record.
- [x] size backstop — this slice is behaving oversized: patch is 220 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. **Human overrode this at sign-off: accepted as-is, no split.**

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
