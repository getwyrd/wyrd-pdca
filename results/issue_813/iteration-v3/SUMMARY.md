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
- Success criterion: the NEW file `crates/custodian/tests/staged_scrub.rs` passes (legs A–C),
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (1) scrub fetches and checks every fragment a committed `part:` record places, with
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (13 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.3% — 183 of 194 instrumentable changed lines executed (floor 80%); 194 of 674 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 5m: 20 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.06s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #813: scrub committed staged-part fragments and retain staged repair obligations; one false-success defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The detect-and-retain slice has falsifiable A–F outcomes and explicit exclusions; the later clock/convergence requirements are authorized carry-forward scope (`brief.md:19`, `brief.md:121`, `brief.md:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent pre-fix execution reproduces lost detection and discarded obligations: scrub 9 failed/4 passed; reconstruction 5 failed/1 passed, all assertion failures (`review-red-scrub.log:90`, `review-red-reconstruction.log:57`). |
| C3 Change | PASS | The changes stay within staged detection, queue retention, the agreed clock seam, and their documentation/tests; staged rebuilding remains explicitly deferred (`crates/custodian/src/reconstruction.rs:109`, `docs/design/architecture/06-runtime-view.md:80`, `brief.md:121`). |
| C4 Verification (red→green) | PASS | Restored code passes all 46 focused tests; complete frozen CI is green, while independent CI stopped only at a read-only advisory-cache lock after workspace tests passed (`review-ci.log:1501`, `review-ci.log:1520`, `review-ci.log:2973`, `gate-logs/C4-ci.log:3672`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Refuse successful certification for an unverified staged chunk with malformed placement—the status guard omits that case and its regression incorrectly requires `Satisfied` (`crates/custodian/src/scrub.rs:303`, `crates/custodian/tests/staged_scrub.rs:1091`). |
| T1 Structure | PASS | Shared staged placement validation and paged readers preserve the maintenance boundary; the deployed pass and clock seam now share one source (`crates/custodian/src/gc.rs:1460`, `crates/custodian/src/gc.rs:1607`, `crates/server/src/custodian.rs:146`). |
| T2 Shape | PASS | Source-before-destination reads preserve publication visibility, and committed placement takes precedence over retired part placement; scoped regressions and seeded handoff coverage exercise both concerns (`crates/custodian/src/scrub.rs:130`, `crates/custodian/src/scrub.rs:219`, `crates/dst/tests/custodian.rs:3142`, `review-remaining-ci.log:542`). |
| T3 Runtime | FAIL | A decodable staged record with empty placement and no stored fragment produces neither verification nor a repair obligation, yet returns `Satisfied`, contradicting the outcome contract (`crates/custodian/tests/staged_scrub.rs:1073`, `crates/custodian/tests/staged_scrub.rs:1091`, `crates/custodian/src/reconciliation.rs:32`). |
| T4 Contribution | FAIL | One batch-review defect remains substantiated; its absent-fleet finding is outside #813's scope. The contribution-artifact subcheck is N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `brief.md:121`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the affected-path search across merged and closed/rejected work—the supplied target has only one synthetic commit and no remote, so the brief's prior-art claim cannot be independently substantiated (`review-prior-art.log:2`, `brief.md:144`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve detection and durable retention as the intended interim outcome, with staged rebuilding already assigned to #814, and decide the warranted Tier-1/Tier-2 follow-up before operational reliance (`brief.md:121`, `crates/custodian/src/gc.rs:1321`, `AGENTS.md:78`). |

The confirmed defect is false certification, not fabrication of a placement. A staged-only chunk with `placement: []` enters `staged.malformed` and is deliberately excluded from fragment checks (`crates/custodian/src/gc.rs:1650`). Scrub emits its warning, but only `unresolvable` affects the final blocked status (`crates/custodian/src/scrub.rs:139`, `crates/custodian/src/scrub.rs:303`). The existing regression proves the resulting `Satisfied` with no repair queued; it passes independently (`review-ci.log:1512`). A caller using `Satisfied` to stop reconciliation receives a complete-success answer despite an unchecked chunk. Keep the audit/no-fabricated-placement behavior, return `Blocked` for this unresolved staged case, and change the regression to require it. ADR-0040's classify/skip/audit rule does not require successful certification (`docs/design/adr/0040-mixed-era-placement-expansion.md:82`); the outcome contract explicitly says every referenced fragment was checked (`crates/custodian/src/reconciliation.rs:32`). This confirms the batch finding at its current location, `scrub.rs:303`, rather than the log's nearby `:300`.

The absent-fleet batch finding is declined for this slice. `brief.md:121` expressly excludes it, and the production loop retains the existing fleet-only traversal (`crates/custodian/src/scrub.rs:233`). It belongs to the separate reachability/detector concern already named in `crates/custodian/src/scrub.rs:286`, not an in-PR expansion. No separate tracking issue for that exclusion was supplied, so none is invented here. The agreed #814 and #806 deferrals are not reopened. The C5 capability-probe smell test does not fire: staged membership and placement precedence address metadata ownership directly; no optional-capability/load-time fallback was introduced.

Independent verification used only the disposable target and this scratch directory. Source citations above resolve under `$PDCA_TARGET` (`./target`); input and log citations resolve in this directory. The target is readable and matches the patch: `git apply --reverse --check ../patch.diff` succeeds. No stale-target qualification is necessary.

- Ran `cargo test --offline -p wyrd-custodian --test staged_scrub --test staged_protection`: 13 scrub and 33 staged-protection tests passed (`review-green.log:179`, `review-green.log:198`).
- Stashed tracked changes, retained the new scrub test, and reran it: 9 assertion failures and 4 controls passed (`review-red-scrub.log:90`). Against the same base, retained the patched staged-protection tests with only the two unavailable context-field initializers removed. Ran the `reconstruction_keeps`, `reconstruction_drains`, unreadable-record drain, and audit-before-inode-fault filters: 5 assertion failures and the unrelated-chunk drain control passed (`review-red-reconstruction.log:57`). Restored the original patch byte-for-byte with stash pop.
- Ran `cargo xtask ci` after restoration: spelling, docs lint/render, guards, formatting, clippy, build, workspace tests, and dependency-use scanning passed. Both focused suites passed again. The command then exited at the advisory database's read-only lock (`review-ci.log:2973`). This is a reviewer-host limitation, not a patch defect or an unmet brief dependency: both named dependencies, `typos` and the renderer, actually ran (`review-ci.log:2`, `review-ci.log:7`).
- Ran the remaining accessible checks separately: `cargo xtask conformance`, `cargo xtask statics`, and `cargo xtask dst` all exited zero. DST includes clippy and seeded tests, including both reconstruction handoff properties (`review-remaining-ci.log:529`, `review-remaining-ci.log:542`, `review-remaining-ci.log:593`).

The frozen evidence supplies the checks whose instance-scoped wrappers or host environment were not reproduced here; no missing-wrapper finding is inferred.

| Gate | Verdict | Basis |
|------|---------|-------|
| C4-ci | PASS | Full captured CI ends successfully, including all three dependency-wall checks, deployment guard and DST (`gate-logs/C4-ci.log:3066`, `gate-logs/C4-ci.log:3077`, `gate-logs/C4-ci.log:3080`, `gate-logs/C4-ci.log:3672`); local lock caveat described above. |
| C4-verify | PASS | Log shows 13 green tests, then 9 assertion failures/4 passes on reverted production; independently reproduced (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:101`). |
| C4-diff-cov | PASS | Captured run measures 183/194 instrumentable changed lines, 94.3%; 480 changed lines are unscored, so this is not coverage of the entire diff (`gate-logs/C4-diff-cov.log:904`). |
| C5-mutants | PASS | Captured run reports 20 caught and 16 unviable mutants, with no survivor; this does not validate the incorrect expected outcome identified above (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | FAIL | One false-certification finding confirmed; absent-fleet finding declined against explicit scope, with both dispositions recorded above (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Artifacts are intentionally drafted after Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Captured output shows actual compilation/checking of the TiKV backend and server feature selections, ending successfully; no live TiKV service test is claimed (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:207`, `gate-logs/host-tikv.log:209`). |

Prior-art evidence remains a human decision: `git log --all -- <all 18 affected paths>` yields only the harness base commit, and `git remote -v` yields nothing (`review-prior-art.log:2`). The brief names rejected #663/#637 attempts but supplies no inspectable merged/closed path-search results. Tier-1 disk faults and Tier-2 kill/reconstruction observation are warranted follow-ups under the standing rubric (`AGENTS.md:78`, `AGENTS.md:81`); they were not run, and do not replace the completed Tier-0 evidence.

### Advisory — adversary

# Adversarial review — issue #813 (663.1: staged scrub and keep)

Bottom line: I could not break the fix. The red→green proof holds up, and the main parts of the
fix are each pinned by a test that fails when that part is undone. What I did find: one line of
the "keep" rule that no test pins, two small gaps against what the brief asked for, one stale doc
line, and a red gating T4 row that a human has to rule on.

## What I re-ran (in a scratch copy of `$PDCA_TARGET`)

- **Green:** `staged_protection.rs` 33/33 and `staged_scrub.rs` 13/13 pass on the patched tree.
  The DST legs `reconstruction_staged_handoffs_*` also pass under `--cfg madsim`, 50 seeds.
- **Red for A–C:** confirmed from `gate-logs/C4-verify.log`. On the red run, 9 of the 13 new tests
  fail, each on an assertion and not a compile error. All of them drive the real `reconcile_step`
  (`crates/custodian/tests/staged_scrub.rs:507-515`).
- **Red for D–F, reproduced by hand the way the brief describes:** I used the base production code,
  kept the new legs, and removed only the two new field initialisers
  (`crates/custodian/tests/staged_protection.rs:535-536`). 6 tests fail on assertions: both leg-G
  tests, leg H, both leg-I tests, and the rewritten leg F. Leg J passes on base, which is expected
  because base scrub never reads `part:`.
- **Mutations that the tests catch (each one makes the named test fail):**
  - Removing `!staged_incomplete` from the drain gate (`reconstruction.rs:434`) → leg I fails.
  - Making `staged_kept` never count as a reason not to certify (`:449`) → leg G and leg H fail.
  - Swapping scrub's read order so it reads `inode:` before `part:` → the C′ test with flip and
    drain both between the reads fails.
  - Swapping reconstruction's read order → leg H fails, and so does the DST leg, on its intended
    "drained the moving chunk's obligation" assertion (`crates/dst/tests/custodian.rs:3054`).
  - Removing the supersession check (`scrub.rs:219-226`) → leg J and
    `a_part_placement_a_committed_map_supersedes_is_not_checked` fail.
  - Stopping unreadable staged records from blocking scrub (`scrub.rs:303`) → leg F fails.
- **Clock:** `LoopClock` (`crates/server/src/custodian.rs:146-169`) wraps the caller's one clock
  closure, and each pass's `now_millis` and the seam both read it. Tests that build a context set a
  `ManualClock` to the same instant they pass as `now`. `STAGED_WRITE_WINDOW_MILLIS` reuses
  `gc::W_WRITE_MILLIS` (`gc.rs:201`), and `LATE_WRITE_DEADLINE_MILLIS` is built from it
  (`gc.rs:269`), so the deadline can never be smaller than the window. I found nothing to refute
  here.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:242`
  (`.chain(staged.held.keys().copied())`) is not pinned by any test. If I delete that line, all 46
  staged tests and both reconstruction DST legs still pass. Concrete failing case: an `Open`
  session whose `part:` record places an RS(2,1) chunk on only `[0, 1]`. That record is readable
  but has the wrong length, so the chunk lands in `StagedSet::held`, not `placed`. Queue an
  obligation for that chunk and run one reconstruction pass. The patched code keeps the obligation
  and answers `Blocked`: I checked this with a throwaway test in scratch, and it passes. With the
  line deleted, the pass drains the obligation and answers `Satisfied`: the same test fails with
  "outcome Satisfied". That breaks the brief's invariant: "an obligation is removed only when no
  record, committed or staged, names its chunk". Fix: add this leg, plus its `sidx:` twin, to
  `staged_protection.rs`.
  Lower value, same kind of gap: the `|| referenced.malformed.contains_key(..)` half of scrub's
  supersession check (`scrub.rs:221`) can also be deleted with no test failing.

- NEEDS-HUMAN [impl] — the brief's Scope (2) says "an empty queue still reads nothing", but no test
  checks this for the new staged read. I changed the empty-queue branch
  (`crates/custodian/src/reconstruction.rs:214-215`) so it calls `staged_fragments` anyway, and all
  206 `wyrd-custodian` tests still pass. The existing empty-queue test
  (`crates/custodian/tests/segmented_map_reconstruction.rs:697-717`) only checks that `inode:` is
  not read. Why it matters: under that change, a pass with nothing to do would return `Err` on any
  `mpu:`/`sidx:`/`part:` store fault, where today it answers `Satisfied`. Fix: extend that test to
  also assert that no `mpu:`, `sidx:` or `part:` read happens.

- NEEDS-HUMAN [impl] — when a `part:` placement is malformed (wrong length), scrub reports it but
  loses the record's key. `read_staged_part` stores only the chunk id
  (`crates/custodian/src/gc.rs:1650`, `set.malformed.insert(chunk.id, m)`). `emit_malformed` then
  prints the committed-map message "scrub found a committed placement of the wrong length"
  (`crates/custodian/src/scrub.rs:398-407`) and bumps the same `scrub_malformed_placement` counter
  that committed maps use. An operator who sees chunk X goes looking for an inode, and no inode
  names X. The only way to find the damaged record is to scan every `part:` key. GC's own staged
  reader keeps the key for the same kind of damage (`StagedSet::hold`, `gc.rs:1425`, `:1436`). The
  loop at `scrub.rs:139-141` also runs before the supersession filter, so a damaged leftover part
  record keeps raising this signal after a committed map has taken over the chunk. Fix: keep the
  `part:` key in `StagedPartSet::malformed` and name it, with wording or an action that says it is
  a staged record.

- NEEDS-HUMAN [impl] — a stale doc line the brief's Scope (4) asked to fix is still there.
  `crates/custodian/src/gc.rs:89-90` (module doc) still says "Scrub and the drain-status surface
  do not read the class at all." Scrub now reads the `part:` half, and reconstruction now reads the
  whole class through `staged_fragments` (`reconstruction.rs:217`). The patch updated the
  `StagedSet` and `reconcile` docs but not this line.

- NEEDS-HUMAN [human] — the T4 batch review is a gating row and it is red (`check-gates.json`,
  `gate-logs/T4-batch-review.log`), so it has to be adjudicated before accept. I think both of its
  blocking findings should be turned down, with the reasons recorded:
  - (a) `scrub.rs:233`: a fragment on a D server that is not in `ctx.fleet` is skipped. The brief
    puts this out of scope in so many words ("servers absent from the live fleet … the same for
    committed chunks today"), and the deployed loop does the same thing for committed chunks
    (`crates/server/src/custodian.rs:555-558`).
  - (b) `scrub.rs:302-313`: a malformed part placement still lets the pass answer `Satisfied`.
    That is the same rule base scrub already applies to malformed committed placements (they are
    reported at `scrub.rs:161-168` and never block), and `staged_scrub.rs:1091-1096` pins
    `Satisfied` on purpose. Making both kinds block would change behaviour for committed maps,
    which is outside this slice.

- NEEDS-HUMAN [human] — the iteration-2 carry-forward asked to "prevent or explicitly bound" the
  scrub/reconstruction loop on stale `part:` placements. The patch prevents it only while a
  committed map names the chunk (`scrub.rs:219-226`). Remaining case: a published object is deleted
  or overwritten before its `part:` records are retired, and reconstruction had already moved one
  of its fragments. The leftover part record then names the empty old position again. Scrub
  re-queues the chunk every pass (`scrub.rs:274-278`), and reconstruction keeps it and answers
  `Blocked` every pass (`reconstruction.rs:623-626`), until the `retire:records:` drain deletes the
  record. Nothing on main runs that drain yet: no production code writes `part:` or `retire:`
  records. So this is forward-looking and low severity, and the only bound is future code. It is
  a scope call whether this slice needs to state that bound.

- Unwarranted claim, not a patch defect: C4-verify's summary line says "13 test(s) ran red", but its
  own log shows `4 passed; 9 failed` on the red run. The four that pass on base are leg B (green on
  base by design), the intact control, and the two supersession (A′) tests. The A′ tests only mean
  something against the patched code. I checked that they do catch the supersession check being
  removed.

- Note, not raised as a finding: scrub's own publication-race protection is covered only by the
  scripted-hook tests (`staged_scrub.rs:994-1055`). The seeded DST sweep's `Driver` enum
  (`crates/dst/tests/custodian.rs:2619-2623`) covers GC and reconstruction but not scrub. The
  iteration-1 sign-off asked for a scripted regression test for scrub and DST coverage only for
  reconstruction, so I am treating that as already decided.

## Refutation attempts that failed

I tried all of these and could not break the fix: the red→green evidence (both the automated A–C
proof and the hand-run D–F proof), both read orders (scripted tests and DST), fail-closed handling
of unreadable staged records in both loops, supersession after a fragment is moved, rejecting an
empty staged placement, the single-clock seam, and the `W_write` wiring.

### Advisory — code-review

No introduced correctness bugs found within the diff's stated scope.

- NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:200` — Low priority, efficiency: the new `fragments` map copies every included fragment before immediately regrouping it into `by_dserver`. Both input collections already deduplicate `(DServerId, FragmentId)`, and the staged filter at `crates/custodian/src/scrub.rs:220` excludes every chunk represented by the committed input. Populate `by_dserver` directly from those two loops, preserving that filter, to remove an unnecessary O(number of fragments) hash table and traversal on every scrub pass, including stores without multipart uploads.

Frozen gate evidence reviewed; no builds rerun. The two T4 reports do not establish additional in-scope defects: absent-fleet servers are expressly excluded by the brief; the malformed-part outcome at `crates/custodian/tests/staged_scrub.rs:1091` follows the existing audit-and-skip convention asserted at `crates/custodian/tests/scrub.rs:824` and specified by ADR-0040 decision 4.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Refuse successful certification for an unverified staged chunk with malformed placement—the status guard omits that case and its regression incorrectly requires `Satisfied` (`crates/custodian/src/scrub.rs:303`, `crates/custodian/tests/staged_scrub.rs:1091`).
- [ ] T5 Judgment — Confirm the affected-path search across merged and closed/rejected work—the supplied target has only one synthetic commit and no remote, so the brief's prior-art claim cannot be independently substantiated (`review-prior-art.log:2`, `brief.md:144`).
- [ ] Validation — fitness-to-purpose — Approve detection and durable retention as the intended interim outcome, with staged rebuilding already assigned to #814, and decide the warranted Tier-1/Tier-2 follow-up before operational reliance (`brief.md:121`, `crates/custodian/src/gc.rs:1321`, `AGENTS.md:78`).
- [ ] `crates/custodian/src/reconstruction.rs:242`
- [ ] the brief's Scope (2) says "an empty queue still reads nothing", but no test
- [ ] when a `part:` placement is malformed (wrong length), scrub reports it but
- [ ] a stale doc line the brief's Scope (4) asked to fix is still there.
- [ ] the T4 batch review is a gating row and it is red (`check-gates.json`,
- [ ] the iteration-2 carry-forward asked to "prevent or explicitly bound" the
- [ ] `crates/custodian/src/scrub.rs:200` — Low priority, efficiency: the new `fragments` map copies every included fragment before immediately regrouping it into `by_dserver`. Both input collections already deduplicate `(DServerId, FragmentId)`, and the staged filter at `crates/custodian/src/scrub.rs:220` excludes every chunk represented by the committed input. Populate `by_dserver` directly from those two loops, preserving that filter, to remove an unnecessary O(number of fragments) hash table and traversal on every scrub pass, including stores without multipart uploads.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 196 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Confirmed defect: a staged chunk with a malformed/empty placement record is skipped by scrub's fragment check but the pass still reports Satisfied, certifying success on a chunk that was never actually verified (crates/custodian/src/scrub.rs:303, crates/custodian/tests/staged_scrub.rs:1091). Both the main reviewer and the adversary reviewer independently confirmed this; the existing regression test asserts the wrong (Satisfied) outcome and needs to require Blocked instead. Separately, the size backstop fired: patch is 196 KB against a 100 KB threshold and this is already round 2. Given the confirmed bug plus a growing list of untested-but-real gaps found by the adversary review (an untested invariant at reconstruction.rs:242, an untested empty-queue read-nothing guarantee, a stale doc line), the slice looks too big to converge with another in-place patch. Re-split at Plan (`pdca split`) rather than iterate-do. The absent-fleet T4 finding is not a defect — both reviewers agree it is out of scope per the brief and matches existing non-staged behavior; no action needed on it.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
