# Result — issue 839 / restore-staged-report

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the post-restore pass keeps staged multipart fragments without saying so. A
  fragment kept only because the staged class protects it is skipped **uncounted**
  (`crates/custodian/src/restore.rs:431-441`; the comment at `:433-434` reads "Staged counters
  are #664's"), while 0016 requires the report to carry `staged_skipped` beside `pending_skipped`
  (`0016:823`; `RestoreReport`, `restore.rs:114-183`). A staged record the pass **read but could
  not trust** holds its chunk and reaches only the audit seam (`emit_untrusted_staged`,
  `restore.rs:1000-1013`). The report does not name it, and `is_clean()` (`restore.rs:197`)
  certifies the run clean over it. `restore.rs:819-821` marks the question `deferred: #664`. The
  human decided it at #664's plan revision (2026-09-18, recorded in the harness repo at
  `results/issue_664/brief.md:152-165`): such a record needs **no** human, but the report names
  it and the run is not clean. The decision, its condition and its limits are quoted under leg
  H-iii.
- Success criterion: the NEW file `crates/custodian/tests/restore_staged_report.rs` passes
  over in-memory doubles, calling the production `reconcile_after_restore`. Legs:
  **(E) Staged skips are counted, and each kept fragment is counted once.** The counting rule:
  `staged_skipped` counts a fragment whose **first** matching protection, in the pass's existing
  order, is the staged class. That order is: the incomplete-set gate and both committed readings
  (`restore.rs:435-437`), then the staged class (`:438`), then the displaced check (`:454-462`), then the pending lease
  (`:489-491`). So a fragment the committed set protects is never counted as staged, and a
  fragment that the staged class **and** a later protection would both keep is counted as staged,
  not by the later counter. Every kept fragment lands in at most one counter. The store holds:
  two fragments that only the staged class protects (a committed `part:` record of an `Open`
  session places them, no committed inode references them, and no `pending:` lease holds their
  chunk); one fragment a staged record places whose chunk a `pending:` lease also holds; one
  fragment a staged record places that the displaced check would also keep (the committed map
  names its chunk's fragment on another D server, which does not hold it: the shape of
  `crates/custodian/tests/restore_reconcile.rs:685-715`, plus a `part:` record placing that
  fragment where it actually is); one ordinary unreferenced stray; and one fragment a committed
  inode references at this server **and** a staged record places. After the pass, the report's
  `Debug` rendering contains `staged_skipped: 4`, `pending_skipped: 0` and `displaced_kept: 0`,
  and `stranded_marked` is 1 (the stray alone).
  **(H-iii) An untrusted staged record is named, and needs no human.** An untrusted record is one
  the pass read but cannot trust about where its chunk's fragments are: a staged placement whose
  length is not its scheme's fragment count, or an owned `sidx:` value that will not decode under
  a key that still names its chunk (`crates/custodian/src/gc.rs:1318-1324`). Fixture: two `Open`
  sessions. Session 1 holds one `part:` record with a wrong-length placement; session 2 holds only
  trusted records. After the pass:
  (a) the report's `Debug` rendering contains `staged_untrusted` and session 1's record key as
  `object_name` renders it (`gc.rs:1788`; an ASCII key is unchanged), and none of session 2's
  keys appear in that list. This is the discriminating arm;
  (b) `needs_human()` is **false**, and `is_clean()` is **false**. The not-a-clean-bill predicate
  already counts findings that need no human (marks, under-replication, `restore.rs:186-199`);
  (c) no fragment of that chunk carries an `orphan:` mark, and the `untrusted-staged-record`
  audit line still fires (`restore.rs:1004-1013`).
  **(CLI)** `restore_verdict` (`crates/server/src/cli.rs:1256`) counts staged skips in its
  summary line. It names untrusted staged records on an **informational** line, not a
  `NEEDS-HUMAN` one, using `named_records` (`cli.rs:1389`) as the unreadable-records paragraph
  does (`cli.rs:1346-1358`). The line says this pass marked none of those chunks' fragments. It
  must not claim the bytes all survived the restore, and it must **not** promise automatic
  cleanup: whether the retire drain removes such a record is #659's call and not yet decided.
  Cover this in `cli.rs`'s own report tests (green-only). The exit status stays
  `report.needs_human()`, and the agreement test
  (`restore_needs_human_agrees_with_every_paragraph_it_prints`, from `cli.rs:2898`) must still
  hold: an informational line never reads as a NEEDS-HUMAN paragraph.
  **Why no human: the human's decision, quoted, with its condition.** From #664's plan revision
  (2026-09-18, `results/issue_664/brief.md:152-165` in the harness repo): "no, but the report
  names it … Why no human: once #809 fences the session its bytes are garbage whatever the record
  says, so what is left is cleanup, which is automatic work (#659), not a judgement. Why not
  silent: a damaged record points at a bug or corruption, and after #808 it blocks every drain in
  the cluster … so the operator should hear about it at restore time rather than when a drain
  stalls." The rule behind it (keep-on-doubt protects user data, not system residue) is #811.
  - *Its condition is the fence.* #809's fence now lands in child-3 (#841) and child-4 (#842).
    Until they do, the record's session is still `Open`. That window cannot be reached in
    production: no client can create a session until #508, which lands after the whole fence
    stack (#810 depends on #842).
  - *What this pass can claim, and no more:* it marks none of the chunk's fragments, so the pass
    itself destroys nothing the record might name. It does **not** claim the staged bytes all
    survived the restore: the pass lists the fragments that exist (`restore.rs:398-402`) and
    judges missing bytes for committed chunks only (Pass 3, `restore.rs:534-585`).
  - *It is an exception to `needs_human()`'s own rule,* which reads "the findings no loop
    resolves on its own" (`restore.rs:201-211`). No loop removes such a record yet (#659 has not
    decided), so the exception rests on the decision above, not on that rule. Say so in
    `needs_human()`'s doc comment, pointing at #811 and #659, so a later reader does not take the
    omission for an oversight.
  Why not silent, on the base: the record blocks every drain in the cluster
  (`docs/design/architecture/06-runtime-view.md:82`, "blocks every drain the same way").
  **(L) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `staged_skipped` and `staged_untrusted` on `RestoreReport`, with `is_clean()`
  counting `staged_untrusted` and `needs_human()` unchanged in behaviour (its doc comment gains
  the exception, leg H-iii); the pass counting and naming them;
  replacing the `deferred: #664` marker (`restore.rs:819-821`) and the "Staged counters are
  #664's" comment (`:433-434`) with a pointer to this behaviour; `restore_verdict`'s summary and
  the informational line, with `cli.rs`'s report tests; the verdict list in the m4 blueprint's
  step 7 (`docs/design/architecture/m4-first-deployment-blueprint.md:599-625`, per
  `AGENTS.md:154-157` "Docs currency"). Size budget: four files, well under 50 KB of diff. A
  fifth file means the shape is wrong. / out of scope: reading or decoding any `mpu:` session
  value, and any session fence or `sessions_fenced` counter (child-3, child-4); `multipart.rs`;
  `gc.rs` (the staged reader already exposes each held record's key and fault, `gc.rs:1344`);
  every existing file under `crates/custodian/tests/` (their report assertions are
  field-by-field and should not break; if one does, say so in `build-notes.md` rather than
  editing around it); `06-runtime-view.md` (child-3 writes the restore paragraph);
  `crates/dst/`; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (2 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 98 of 98 instrumentable changed lines executed (floor 80%); 98 of 184 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 19 mutants tested in 4m: 12 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_839/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.52s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #839’s restore-report change: count staged skips and name untrusted staged records without falsely certifying a clean run.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | First-matching protection and the approved informational-only exception are explicit, falsifiable requirements; the exception’s fence ordering and cleanup deferral remain documented (`brief.md:26`, `crates/custodian/src/restore.rs:239`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the three tracked changes while retaining the new regression file produced two assertion failures after successful compilation: both required report fields were absent (`reviewer-red.log:144`, `reviewer-red.log:152`, `reviewer-red.log:158`). |
| C3 Change | PASS | Operators receive the missing accounting and exact held-record names without changing the agreed human-action exit policy; the runbook describes the same limited claim (`crates/custodian/src/restore.rs:220`, `crates/server/src/cli.rs:1376`, `docs/design/architecture/m4-first-deployment-blueprint.md:626`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently changed both regressions to passing; workspace lint/build/tests and CLI agreement also passed locally; full CI is supported by frozen evidence, with the local advisory-cache limitation detailed below (`reviewer-green.log:9`, `reviewer-ci.log:2037`, `gate-logs/C4-ci.log:3744`). |
| C5 Causal adequacy | PASS | The reporting omission is corrected where protection and attribution are decided; regressions discriminate overlapping protections, duplicate record attribution, false clean verdicts, and actual orphan marks; no capability-probe symptom guard was added (`crates/custodian/src/restore.rs:471`, `crates/custodian/src/restore.rs:856`, `crates/custodian/tests/restore_staged_report.rs:454`, `crates/custodian/tests/restore_staged_report.rs:557`). |
| T1 Structure | PASS | Existing trait seams and the shared staged classification remain authoritative, so reporting cannot invent a competing protection interpretation; existing key formatting and bounded CLI naming are reused (`crates/custodian/src/restore.rs:856`, `crates/server/src/cli.rs:1384`). |
| T2 Shape | PASS | Four files and 46,725 diff bytes fit the stated size budget; session fencing, codecs, existing custodian tests, and dependency manifests remain outside this slice (`reviewer-integrity.log:1`, `brief.md:124`). |
| T3 Runtime | PASS | Reporting adds per-fragment counting and ordered attribution without extra external reads or a new collection path; fragment-level assertions preserve the protection contract, and the existing DST suite passed independently (`crates/custodian/src/restore.rs:480`, `crates/custodian/src/restore.rs:864`, `crates/custodian/tests/restore_staged_report.rs:581`, `reviewer-tail-checks.log:595`). |
| T4 Contribution | N/A | Contribution text is absent by design at Check; its substantive audit is owed to the mandatory publish-time rerun, not human clearance now (`gate-logs/T4-contribution.log:10`); living runbook currency is satisfied (`docs/design/architecture/m4-first-deployment-blueprint.md:626`). |
| T5 Judgment | PASS | The reporting slice respects the settled #664 decision and #659 deferral; independent history queries by all four affected paths and inspection of closed-unmerged #647 found no conflicting rejected approach (`crates/custodian/src/restore.rs:239`, `reviewer-prior-art.log:2`, `reviewer-prior-art.log:33`, `brief.md:162`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whether the message and runbook give restore operators sufficient visibility into an unclean run with a successful exit status — automated checks establish accounting and text/status consistency, while operational usefulness remains a human acceptance decision (`crates/server/src/cli.rs:1376`, `docs/design/architecture/m4-first-deployment-blueprint.md:626`). |

Source citations above are relative to `$PDCA_TARGET`; evidence and brief citations are relative to this review directory. All four target file hashes match the patch’s resulting blobs, and reverse-apply validation succeeds (`reviewer-integrity.log:3`). The disposable target was restored after the red leg; no implementation edits were made.

Independent execution supports the reporting contract. `cargo test --offline --locked -p wyrd-custodian --test restore_staged_report` ran twice: two assertion failures with the production changes stashed, then two passes after `git stash pop`. The green tests establish four staged-first skips with zero pending/displaced double counts and only the stray marked; the isolated untrusted-record fixture names one record once across two held chunks, excludes trusted keys, preserves its audit event, marks none of those fragments, and returns `needs_human() == false` with `is_clean() == false` (`crates/custodian/tests/restore_staged_report.rs:454`, `crates/custodian/tests/restore_staged_report.rs:566`).

The local full-CI attempt has a host caveat, not a patch failure. `cargo xtask ci` passed real `typos`, docs lint, the 99-page render/link audit, hygiene guards, formatting, workspace clippy/build/tests, and cargo-machete. It then stopped because cargo-deny could not acquire `/home/eddie/.cargo/advisory-dbs/db.lock` on a read-only path; explicit offline mode hit the same restriction (`reviewer-ci.log:3041`, `reviewer-deny.log:1`). The frozen CI log actually records all three cargo-deny checks passing and full CI completion (`gate-logs/C4-ci.log:3136`, `gate-logs/C4-ci.log:3147`, `gate-logs/C4-ci.log:3150`, `gate-logs/C4-ci.log:3744`). Conformance, the DST-reachable statics scanner, and DST clippy/tests were independently run afterward and passed (`reviewer-tail-checks.log:1`). The brief’s external dependencies, `typos` and the docs renderer, were exercised locally (`reviewer-ci.log:2`, `reviewer-ci.log:10`); neither was substituted or skipped.

Other frozen gates support only their recorded scope:

- Diff coverage reports 98/98 instrumentable changed lines, with 86 lines unscored; its server measurement uses the existing suite and credits colocated test additions (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:597`). This is not evidence of execution of every changed line.
- Mutation evidence records 12 caught and seven unviable mutants, with no survivor reported (`gate-logs/C5-mutants.log:13`). The harness-scoped coverage/mutation wrappers were adjudicated from these logs rather than reconstructed from other checkouts.
- The batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The TiKV log records successful real feature clippy checks for the metadata crate and server (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`); it establishes compilation, not live-backend operation. Contribution auditing remains N/A until publish as above.

Prior-art review was performed by affected path against merged history, 100 closed PRs, and all open PRs returned by GitHub. Twelve closed PRs touched those paths; only #647 was unmerged, and its inspected diff concerns segmented-map resolution rather than this staged-reporting contract. Its rejection requested a smaller change. No open PR was returned. The brief separately records rejection of #809’s oversized omnibus; this patch contains only the four-file reporting slice (`reviewer-prior-art.log:17`, `reviewer-prior-art.log:27`, `reviewer-prior-art.log:32`, `brief.md:162`). Existing tracked fence/cleanup deferrals are settled, not renewed findings. A separate disk-fault or kill/reconstruct campaign is unnecessary for this reporting-only scope.

### Advisory — adversary

# Adversarial review — issue #839 (809.1, restore-staged-report)

**Verdict: could not refute.** I re-ran the proof myself, probed the edge cases the new test
file skips, and hand-mutated the ordering. The fix held every time. No NEEDS-HUMAN items.

## Attempts that failed to refute the fix

- **Red→green re-run, independently.** In a scratch clone of the base (`ffc75ff`), with only the
  new test file added, both tests fail **by assertion**, not by compile error:
  `crates/custodian/tests/restore_staged_report.rs:471` (`staged_skipped`: `None` vs `Some("4")`)
  and `:561` (no `staged_untrusted` in the report). With the full patch both pass, and the 5
  `restore_` tests in `crates/server/src/cli.rs` pass. This matches `gate-logs/C4-verify.log`.
  The tests call the production `reconcile_after_restore`. Only the store and the disks are
  doubles, and nothing in the path is mocked away.
- **Is the "first protection" order really pinned?** cargo-mutants never generates a reordering,
  so I tested two by hand at `crates/custodian/src/restore.rs:480-483`. Moving the staged check
  after the displaced check makes E fail with `staged_skipped` 3 vs 4. Moving it ahead of the
  committed readings makes E fail with 7 vs 4. The count-based assertion is discriminating: it
  cannot pass while the ordering property fails.
- **Untrusted `sidx:` entry** (a value that will not decode, under a key that names its chunk),
  which the new file does not cover. It is named in `staged_untrusted`, its 2 fragments count as
  `staged_skipped`, nothing is marked, `needs_human()` is false and `is_clean()` is false. This is
  correct: `crates/custodian/src/gc.rs:1391-1392` holds it, and `restore.rs:856-871` names it.
- **Untrusted record beside an unreadable one.** The untrusted record is still named, the
  unreadable one goes to `unresolvable`, `staged_skipped` is 0 (the incomplete gate at
  `restore.rs:471` runs first), and nothing is marked. No double listing: a key lands in `held`
  or in `unresolvable`, never both (`gc.rs:1405-1417`, `:1376-1397`).
- **Untrusted record over a chunk a committed object also places.** The committed copies are
  skipped uncounted, and a stale extra copy on another server is held and counted as staged, not
  marked. The record is named.
- **Second run.** The same `staged_skipped` and `staged_untrusted` come back, and the stray moves
  to `already_marked`. The new counters are stable across re-runs.
- **CLI claims.** "blocks every drain in the cluster" (`cli.rs:1379-1380`, and the blueprint)
  matches `crates/custodian/src/desired_state.rs:322-340`: a held staged chunk returns
  `PendingMalformed` for every server. The `needs_human()` doc says no production client can
  create a session yet, and that holds: the only `mpu_key` in non-test code is its definition at
  `crates/core/src/multipart.rs:1212`. The note line contains no "NEEDS-HUMAN", "automatic" or
  "cleanup". The agreement test (`cli.rs:2930-3033`) still pairs each paragraph with the exit
  status.
- **Reviewer rationalization.** I found no unwarranted claim in `check-gates.json`. One caveat
  on reading the evidence: E's `pending_skipped: 0` and `displaced_kept: 0` assertions were
  already true on the base (the staged check sat in the same gate there), so they were **not**
  part of the red. They guard the ordering, and the hand mutations above show they do that job.
  Only `staged_skipped` and `staged_untrusted` produced the red, which is what the brief predicted.

## Minor observations (non-blocking; no rebuild needed)

- `crates/custodian/src/restore.rs:132-135`: the `staged_skipped` doc lists the order as
  "committed readings (uncounted), the staged class, …" and leaves out the incomplete-set gate
  that runs before all of them. So on any INCOMPLETE run (one unreadable record anywhere)
  `staged_skipped` is 0 even though every staged fragment was kept. The CLI's INCOMPLETE
  paragraph then says "the counts above cover the REST of the store only"
  (`crates/server/src/cli.rs:1361-1362`), which is not quite true of this count. The older
  `pending_skipped` and `displaced_kept` counts already had the same wording problem; this
  patch adds one more count under it. A one-clause doc fix if anyone cares. It is not a defect.
- `restore.rs:1050` vs `:1075`: the existing counter `restore_untrusted_staged_records`
  increments once per (record, chunk) pair, but the new summary field `staged_untrusted` counts
  records. For the H-iii fixture (one record, two chunks) the counter says 2 and the summary
  says 1. The unit difference is documented on the field ("once per record"), so this is not a
  defect. It is worth knowing before anyone builds a dashboard that compares the two.

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Checked protection precedence (`crates/custodian/src/restore.rs:471`), record deduplication (`crates/custodian/src/restore.rs:856`), informational CLI reporting (`crates/server/src/cli.rs:1370`), and the production-path regression fixtures (`crates/custodian/tests/restore_staged_report.rs:389`, `crates/custodian/tests/restore_staged_report.rs:498`). All citations were verified against `$PDCA_TARGET`.

Validation uses the frozen gate evidence: CI passed; both regression tests failed by assertion before the fix and passed afterward; instrumentable diff coverage was 98/98; mutation testing reported 12 caught and 7 unviable, with no survivors. No builds or tests were rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept whether the message and runbook give restore operators sufficient visibility into an unclean run with a successful exit status — automated checks establish accounting and text/status consistency, while operational usefulness remains a human acceptance decision (`crates/server/src/cli.rs:1376`, `docs/design/architecture/m4-first-deployment-blueprint.md:626`).
- [x] The justification for the no-human verdict is not established by the supplied evidence. `brief.md:19-20` attributes it to a human decision on #664, but this sandbox contains neither `notes.json` nor `sources/`, so that decision and its conditions cannot be checked. The fallback rationale, “nothing is lost, since every fragment of the chunk is kept” (`brief.md:53`), exceeds what the target proves: the pass enumerates existing fragments (`crates/custodian/src/restore.rs:398-402`) and assesses missing bytes only for committed chunks (`restore.rs:538-569`). Holding staged bytes prevents further reclamation; it does not establish that all staged bytes survived the restore. Moreover, `needs_human()` is documented around findings no loop resolves (`restore.rs:204-209`), while the brief expressly leaves automatic cleanup undecided (`brief.md:48-49`). Include the exact decision and its conditions in the brief, explain the intended exception to that predicate's rationale, and replace the no-loss guarantee with the narrower preservation claim.
- [x] The counter criterion leaves protection overlap unresolved. `brief.md:23-30` defines `staged_skipped` as fragments kept because of the staged class “alone,” but tests only staged-only fragments and overlap with a committed placement. In the target, staged protection short-circuits at `crates/custodian/src/restore.rs:435-440`, before displaced committed copies are kept at `restore.rs:454-462` and pending leases are counted at `restore.rs:489-491`. Simply splitting out and counting the staged branch passes E, yet also counts a staged fragment that a pending lease or displaced-copy protection would independently keep. Specify whether this is an exclusive-cause count or a first-matching-protection count, and add explicit expected counts for those overlaps to E. Otherwise materially different counter meanings satisfy the advertised success criterion.

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Doc nit: `staged_skipped` doc (restore.rs:132-135) omits that an INCOMPLETE run reports 0, and cli.rs:1361-1362 "counts cover the REST of the store" is inexact for it (pre-existing for pending_skipped/displaced_kept too).
- Unit mismatch: metric `restore_untrusted_staged_records` counts (record, chunk) pairs, report field `staged_untrusted` counts records — note before dashboards compare them.
