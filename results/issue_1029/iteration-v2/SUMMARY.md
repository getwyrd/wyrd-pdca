# Result — issue 1029 / multipart-part-commit-settle-unknown

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.2b a commit answered `CommitUnknownResult` returns the in-doubt error at once, even
  when a re-read could show that its own commit landed, or that it did not and never can. A client then
  sees an in-doubt answer for a part that committed, and a slot stays held. This slice settles the outcome
  from re-reads where the evidence allows it, and otherwise keeps the doubt.
- Success criterion: Appended to `crates/core/tests/multipart_upload_part_commit.rs`.
  **(B) Settling.**
  - A commit that lands but answers unknown: `Committed`, slot released exactly once.
  - A re-upload that did not land (`may_still_commit: false`): the re-read finds the **prior** part. Never
    `Committed` for it; it ends with its own part committed.
  - **A held batch** (`may_still_commit: true`, landed later): no `Committed` and no compensation on that read
    alone. Every later batch requires its own slot at the bytes the held batch requires, so at most one
    lands. Three runs: (i) released before the next batch; (ii) released after the attempt has returned; (iii)
    every later batch `Conflict` unapplied until the rounds run out, then released. In every run exactly one of
    {its own part, its compensation} is durable at the end, the slot is released once, and `Committed` is
    answered only if its own part landed. (iii) returns the in-doubt error keeping the flag.
  - A same-number rival with this attempt's commit not applied, where the commit required the key **absent**
    (run once per `may_still_commit` value): a slot read made **after** the read that found the rival finds the
    slot still held. Never `Committed`; it compensates and answers `Refused(OperationAborted)`.
  - **A changed prior settles nothing** (decided at sign-off, iteration 4). A held re-upload over a non-empty
    prior that loses to a rival answers the in-doubt error with no compensation. A held re-upload while a repair
    moves the prior away and back, in real re-place batches or their exact shape: no compensation while the
    commit may land, never a false `Refused(OperationAborted)`, slot released once. A rival over an empty prior
    settles nothing, because those bytes can come back.
  - **The read-gap leg.** A non-empty commit is held. At **each** gap between the classifying reads the wrapper
    runs one of four histories. *Landed, then a rival:* no compensation, `Committed`, slot released once.
    *Landed, then a rollback:* no compensation, the in-doubt error, never `Refused(OperationAborted)`. *Not
    landed, a rival* (the commit required the key absent) and *not landed, a rollback:* the attempt reaches a
    slot read made after it saw the change, then compensates and answers `Refused(OperationAborted)`; the held
    batch, released later, answers `Conflict`.
  **(C) Regressions.**
  - **C1.** With a fixed clock, an empty part 1 commits. An empty re-upload in the same millisecond and epoch
    answers unknown, **not applied** (once per flag). Its bytes equal the prior's, and the answer must not be
    `Committed` on them. On return the slot is released or covered by the reported doubt, and the held-slot count
    equals the live attempts (`0016:1779`). Same for an empty rival in the same millisecond.
  - **C2, a torn read.** A commit held and never applied. At **each** read gap the wrapper runs a real Abort and
    deletes the session's `sidx:` and `slot:` records. Never `Committed`; `NoSuchUpload` or the in-doubt error.
  - **C3.** A commit **lands**, answers unknown with `may_still_commit: false`, and a rival replaces the part
    before the re-read: `Committed`. **C3′:** then a third attempt reserves the freed index, so the slot key holds
    another `attempt_id`: still `Committed`, and that attempt's slot is byte-for-byte unchanged.
  - Every commit round answered unknown with `may_still_commit: false`, not applied: it compensates and returns
    `Stopped(CommitRoundsSpent)` reporting the release.
  - **The kept doubt is the round that may still land:** round 1 held, round 2 unknown with `false`, then a read
    faults. The answer keeps `may_still_commit: true`.
  - A slot rewritten under the attempt's own id during a held commit: `InDoubt(SlotLost)`.
  - A slot gone under a moved session: the in-doubt error, never `Committed`, never a compensation.
  **(F) The lease.** A commit in doubt when the lease runs short: no further commit or compensation, the in-doubt
  error. **Delayed after dispatch:** a commit sent with the margin, held past the lease's expiry. (i) It lands before
  the re-read: `Committed`. (ii) It lands after return: the in-doubt error; the part ends durable. (iii) A test-local
  batch in the idle fence's shape (`0016:1859-1863`) lands first: the held batch answers `Conflict`, no `part:`
  exists, the answer is `NoSuchUpload` or in-doubt. (iv) The held batch lands, then that fence: it answers
  `Conflict`, the session stays `Open@E`. No compensation in any of them.
  **(H) A held commit that lands, then a rollback,** both right after the slot read. The next slot read finds the
  slot gone under a moved session, which proves nothing about whose batch removed it: the in-doubt error, no
  compensation, the slot released once, and the attempt's own part durable. This is the read-gap leg's *landed, then a
  rollback* at one named gap. **If the commit has not landed** when the rollback does, that is *not landed, a rollback*,
  and the attempt compensates and answers `Refused(OperationAborted)`.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a
  record at the same or a lower epoch, `Open` or not, and a missing `mpu:` with any record at the slot key: `InDoubt` keeping
  `may_still_commit`. Repairing the record mid-round must not produce `Refused(OperationAborted)` or a compensation,
  and a repaired record lets the held batch land.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: what a commit's `CommitUnknownResult` means: the own-commit proof, settling, the kept doubt, and retrying
  the same batch. / out of scope:
  - a **compensation's** unknown outcome: 657.2a's in-doubt stop stays until 657.2d;
  - the damage-only false `Committed` with the slot erased (657.2d);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs, including the wording at `0016:1727-1728`; `Cargo.lock`
    and `deny*.toml`; the gateway and any production caller; Complete, the drain, the reaper, DST.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 37 mutants tested in 3m: 1 missed, 26 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.00s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #1029’s settlement of unknown multipart part commits; human fitness-to-purpose sign-off remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The criteria distinguish this attempt’s committed outcome from another writer’s part and forbid compensation while a commit remains possible; the required read ordering implements that distinction (`brief.md:74`; `crates/core/src/multipart/commit.rs:1217`). |
| C2 Reproduction (red pre-fix) | PASS | Two isolated behavioral regressions fail against the stashed implementation: a landed unknown commit and an unapplied re-upload both return premature doubt (`reviewer-evidence/behavioral-red.log:10`, `:30`; `crates/core/tests/multipart_upload_part_commit.rs:4160`, `:4190`). |
| C3 Change | PASS | The three-file change stays within commit settlement, its tests, and the living architecture; production wiring and compensation settlement retain their declared scope (`crates/core/src/multipart/commit.rs:253`, `:1039`; `docs/design/architecture/05-building-block-view.md:250`). |
| C4 Verification (red→green) | PASS | Both behavioral regressions turn green and all 69 multipart commit tests pass independently; the frozen full gate passes, while the local full rerun has the snapshot limitation detailed below (`reviewer-evidence/behavioral-green.log:3`, `:9`; `reviewer-evidence/green.log:76`; `gate-logs/C4-ci.log:4806`). |
| C5 Causal adequacy | PASS | Ordered session/slot evidence prevents premature settlement, and the new tests independently kill all five carry-forward fault mutations; no capability probe conceals a load-time cause (`crates/core/src/multipart/commit.rs:1198`, `:1206`, `:1213`; `reviewer-evidence/mutations-summary.log:2`). |
| T1 Structure | PASS | The implementation preserves the narrow metadata seam, per-attempt state, and the lease’s existing clock source; no global state or concrete backend dependency enters core (`crates/core/src/multipart/commit.rs:768`, `:1272`, `:1292`). |
| T2 Shape | PASS | Unresolved commits remain classifiable as indeterminate even when a definite store fault stops progress, and the changed public error behavior is documented without changing persisted records (`crates/core/src/multipart/commit.rs:543`; `docs/design/architecture/05-building-block-view.md:250`). |
| T3 Runtime | PASS | Commit dispatch remains bounded to four identical batches with lease margin checks; the extra read-only pass cannot repeat indefinitely, and held commits never trigger compensation (`crates/core/src/multipart/commit.rs:1136`, `:1316`, `:1332`, `:1344`; `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must rerun at publish. The other T4 evidence and affected-path prior-art checks are recorded below (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The surviving guard mutant is equivalent on reachable paths, the requested fault-test gap is closed, and tracked DST/compensation deferrals remain settled (`crates/core/src/multipart/commit.rs:808`, `:1237`, `:1332`, `:258`, `:1039`; `crates/core/tests/multipart_upload_part_commit.rs:4906`, `:4966`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this core-only slice as sufficient for the intended multipart lifecycle — real-store fault/interleaving tests support settlement safety, while end-to-end S3 use is deliberately deferred to #669 (`crates/core/src/multipart/commit.rs:253`; `crates/core/tests/multipart_upload_part_commit.rs:4582`). |

Source citations resolve against the supplied `$PDCA_TARGET`; evidence citations resolve in this review directory. The target contains the patch on its prerequisite base. Reverse-apply checking succeeds, and its three modified blobs match the patch’s destination hashes (`reviewer-evidence/source-hashes.json`). No implementation edits remain from this review.

The independent evidence supports the implementation verdict:

- **Behavioral red→green is demonstrated.** Stashing only the implementation and architecture changes leaves the complete new test file unable to compile against the old API because `InDoubt::cause` and new stop variants do not yet exist (`reviewer-evidence/red.log`). That compile failure is not the behavioral proof. A scratch harness combines the unchanged base fixture with the two unchanged appended test bodies at `crates/core/tests/multipart_upload_part_commit.rs:4160` and `:4190`. Both fail assertions against the pre-fix library and pass against the restored library (`reviewer-evidence/behavioral-red.log`, `reviewer-evidence/behavioral-green.log`). The restored full test target separately passes 69/69.
- **The carry-forward test gap is closed.** In a scratch copy of core, replacing `doubt.end(...)` with `stopped(...)` separately at `crates/core/src/multipart/commit.rs:1325`, `:1197`, `:1202`, `:1211`, and `:741` makes the new regression fail at the intended fault site. Each mutation compiles; the unchanged control passes. The failures expose an incorrect `Stopped` answer while the held round remains possible (`reviewer-evidence/mutations-summary.log`; reproducible driver: `reviewer-evidence/recheck_mutations.py`).
- **The surviving mutant does not expose a missed behavior.** The frozen mutation gate reports 26 caught, 10 unviable, and one missed (`gate-logs/C5-mutants.log:13`). I reproduced the missed `ReadAgain if send` → `if true` mutation with all 69 tests passing. Entering the read-only lap already sets every outstanding round’s `may_land` to false; no new round is submitted. A held slot therefore removes every doubt at `crates/core/src/multipart/commit.rs:808`, making `ReadAgain` at `:1237` impossible; other slot states or read faults return. Thus the changed guard cannot affect a reachable second read-only lap. This is a causal equivalence argument, not a claim that survival alone proves correctness.
- **Prior art was checked by every affected path.** GitHub default-branch history contains one commit for each Rust file and 49 for the architecture file; the Rust predecessor is `5e8aa249ee4a` (merged PR #1041). Enumerating all 445 closed PRs and inspecting the complete file lists of all 19 unmerged PRs found no overlap with any of the three affected paths (`reviewer-evidence/prior-art-summary.log:1`; captured responses in `prior-path-history.json`, `closed-pulls.json`, and `closed-unmerged-files.json` under `reviewer-evidence/`).

The gate evidence has two limitations, neither established as a patch defect:

- **The disposable snapshot omits the advisory-database gitlink.** Independent `cargo xtask ci` reaches an unrelated failing index assertion at `xtask/tests/advisory_db.rs:67`; `git ls-files -s -- .github/advisory-db` returns nothing. The same missing pin prevents its dependency audit (`reviewer-evidence/ci.log:3620`, `:3639`). The frozen gate actually fetched the pinned database and passed all dependency walls (`gate-logs/C4-ci.log:4161`, `:4173`, `:4176`, `:4179`), and ends with “all checks passed” at `:4806`. The local full rerun exits 1 with exactly those two failures; its other phases, including the complete DST run, finish successfully (`reviewer-evidence/ci.log:4266`). The local full rerun must not be described as wholly green.
- **Diff coverage was not measured.** Its log shows failure applying this dependent patch to `origin/main`, not an uncovered-line result (`gate-logs/C4-diff-cov.log:10`). The brief explicitly depends on #1028 (`brief.md:81`); the supplied prerequisite-based target builds the changed code and passes its tests. No coverage percentage is inferred from this stale-base gate.

The remaining frozen rows are accounted for: C4-verify is explicitly green-only (`gate-logs/C4-verify.log:161`), supplemented by the independent behavioral red→green above; T4-batch-review reports zero blocking findings but contains no review text or effort metadata (`gate-logs/T4-batch-review.log:10`), so this verdict rests on the independent source, interleaving, test-fidelity, and mutation review; host-tikv shows real crate and server feature compilation finishing successfully (`gate-logs/host-tikv.log:112`, `:211`); contribution checking remains N/A until its mandatory publish rerun. Both declared external tools were exercised: `typos` and documentation lint/render ran independently and in the frozen CI, with 101 pages rendered and a successful link audit (`reviewer-evidence/render.log:2`; `gate-logs/C4-ci.log:11`, `:19`). The #660 seeded-DST deferral is explicitly recorded in target source and is not reopened here.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1029 (657.2c), part commit settles a `CommitUnknownResult`

**Verdict: I could not refute the fix.** I re-ran the test file on the patched tree (69/69 pass, and 69/69 on each of 8 more runs, so no flakiness). Then I made 34 hand mutations of the new logic in `crates/core/src/multipart/commit.rs` in a scratch copy. Every one of them made at least one test fail. What is left is about the evidence and the gates, not the code.

## Attempted refutations that failed

- `crates/core/src/multipart/commit.rs:741`, `:1179`, `:1197`, `:1202`, `:1211`, `:1325`: I changed `doubt.end(upload_id, Stop::Store(fault))` to `stopped(...)` at each site, one at a time. All six are caught, by `c_a_store_fault_at_each_re_read_under_a_held_commit_keeps_the_doubt` (`crates/core/tests/multipart_upload_part_commit.rs:4966`) or `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt` (`:4906`). The iteration-1 sign-off gap is closed, and the part read at `:1179`, which the sign-off did not list, is pinned too.
- The brief's demonstrated reds, each re-created and each caught:
  - `.find(may_land).or_else(last)` replaced by `.last()` (`commit.rs:819`): caught by the kept-doubt leg.
  - The torn-part check moved ahead of the own-commit proof (`commit.rs:1218-1220`): caught by `r_damage_under_a_held_commit_keeps_the_doubt`.
  - Settling on the slot read made before the last session read (inserted after `commit.rs:1213`): caught by the read-gap leg and H.
  - A non-empty prior treated as settling (`commit.rs:1187` guard dropped): caught by all three changed-prior legs.
  - The rival-over-absent rule never settling (`:1187` made a no-op): caught by the B rival leg, the read-gap leg and C1.
  - A short lease compensating (`commit.rs:1351` sent to `unexplained_stop`): caught by both F legs.
  - Spent rounds compensating while a round is in doubt (`:1345`): caught by 6 tests.
  - Removing the first-read or last-read `session_seen` (`:1198`, `:1213`): caught.
  - `retain` replaced by `clear` or a no-op (`:809`): caught.
  - The reported flag taken from the store instead of the settled value (`:824`): caught.
  - `observe` forced to `true` or to `false`: caught.
  - The `session_gone` doubt arm removed, or its `cannot_land` removed (`:730`): caught.
  - The moved-and-gone arm removed (`:1232`): caught.
  - `doubt.any() ||` dropped from the `Again` arm (`:1254`): caught.
  - The proof's `doubt.any()` guard dropped, or the proof run on the first session read instead of the last: caught.
  - `ReadAgain` treated as an immediate end, or as a re-send (`:1332`): caught.
  - Every damage arm switched from `doubt.end` to `stopped` (part, slot, session, `PartRecordMissing`, rewritten slot): caught.
- The proof "slot gone while a later session read still finds `Open@E` means my own commit landed" depends on nothing else deleting `slot:` under an `Open` session. I grepped: the custodian (`gc.rs`, `rebalance.rs`, `reconstruction/staged.rs`) never writes `slot:` keys, and Abort fences before it deletes. The epoch only rises (`commit.rs:636-640` treats an equal or lower epoch as damage), so "`Same` at the later read" does mean `Open@E` at the slot read. I found no history that breaks it.
- C5's one surviving mutant, `commit.rs:1332:34` (guard `send` replaced with `true`), is **equivalent**, not a test gap. `ReadAgain` needs a held slot with a round still in doubt (`:1238`). The lap that sends nothing is entered only after the last read marked every round `may_land = false` (`:1213`). That lap observes no new round, so its slot read (`:1206` into `:809`) empties the doubt before `:1238` can match. The arm at `:1338` is therefore dead code, and both versions of the guard behave the same.
- The test wrapper's slot guard was loosened to accept "bytes this attempt released" (`multipart_upload_part_commit.rs:597`). That escape applies to every batch kind, compensations included, not only the "round sent again" its doc names. I tested whether this hides a compensation sent after the attempt's own commit landed (mutation: `Judged::Landed` also calls `refuse`). Eight tests catch it through their own compensation counts, so no hole is open today. Optional tightening: limit the escape to `Kind::Commit`.
- Rubric checks: the missing seeded DST coverage is an in-code deferral (`commit.rs:258`, `// deferred: #660`), so it is settled. The living doc is updated in the same change (`docs/design/architecture/05-building-block-view.md:250`). No production caller is added: the patch touches only `commit.rs`, its test file and that doc.

## Claims in `check-gates.json` that I think are not earned

- NEEDS-HUMAN [human] — `T4-batch-review` (a gating row) reports `pass` with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **14.85 s** (`gate-logs/T4-batch-review.log`). It is described as three review passes over a 156 KB patch. That time is too short for a fresh review of this size, and zero findings of every kind (not even dropped noise) fits a cache hit or empty reviewer output better than a real pass. The brief requires "a confirmed high-effort review of the final patch", and this row does not show one. Open `results/issue_1029/review-batch.md` and confirm it reviewed this revision, the one with the new fault-site tests, and not the iteration-v1 bundle.
- NEEDS-HUMAN [human] — `C4-diff-cov` says "patch.diff does not apply on origin/main", so coverage of the changed lines was never measured. One possibility is that #1028 (the slice this one builds on) is not merged yet, which is expected for a stacked slice. The other is that `main` has moved and the bundle is stale. I can't tell from the snapshot (the target has no `origin` refs). Confirm the base before merge. My 34 mutations cover the same new lines, but only for the paths I chose.
- `C4-verify` reports `PASS (green-only)` with the reason "test is co-located with the fix (no separate */tests/*.rs)". That reason is wrong for this patch: the tests live in the separate file `crates/core/tests/multipart_upload_part_commit.rs`. They are appended to an existing file and use new API (`InDoubt::cause`, `Stop::SessionMoved`), so they would fail to compile on the pre-fix code. That is a trivial red, not a meaningful one. The brief declared green-only in advance, so this is not a refutation. The mutation runs above stand in for the red-to-green evidence the gate did not produce.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [human] — **P1: the new not-landed proof can compensate an already committed part after delayed staging batches restore its slot** (`crates/core/src/multipart/commit.rs:1204`, `crates/core/src/multipart/commit.rs:1258`). With the existing fixed-clock rig, script `Reserve`, `Intent`, and the quiesce `Renewal` each as `[Act::hold(), Act::pass()]`, then stage one chunk and let its part commit apply but answer unknown. Before its re-read, release the three held batches in order. Successful retries leave those earlier invocations outstanding (`crates/core/src/multipart/stage.rs:1335`, `crates/core/src/multipart/stage.rs:2338`, `crates/core/src/multipart/stage.rs:1743`). The delayed reserve recreates the deleted slot; the delayed intent recreates its owned entry; the delayed renewal restores exactly the slot/entry bytes the commit required. Retry stamps can be identical (`crates/core/src/multipart/stage.rs:1567`). The re-read sees the attempt's own part, marks the absent-part precondition permanently lost at `crates/core/src/multipart/commit.rs:1187`, then clears the doubt because the restored slot is `Held`. It compensates and answers `OperationAborted`, leaving both the durable part and a retirement obligation for its own chunks. This diff introduces that compensation after an unknown result; the previous code stopped in doubt. Add this cross-phase regression and establish a guarantee that earlier staging batches cannot restore the evidence used to settle a commit. The brief excludes changes to staging, so providing that guarantee requires an explicit scope/protocol decision; this is not the deferred damage-only slot-erasure case.

No additional reuse, simplification, or efficiency findings. Validation: target-source review and an executed, source-derived atomic-CAS model confirmed the history above; the Rust regression was not run. Frozen C4-ci evidence reports all 69 multipart commit tests passing, but the added tests do not combine outstanding staging batches with commit settlement.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept this core-only slice as sufficient for the intended multipart lifecycle — real-store fault/interleaving tests support settlement safety, while end-to-end S3 use is deliberately deferred to #669 (`crates/core/src/multipart/commit.rs:253`; `crates/core/tests/multipart_upload_part_commit.rs:4582`).
- [ ] `T4-batch-review` (a gating row) reports `pass` with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **14.85 s** (`gate-logs/T4-batch-review.log`). It is described as three review passes over a 156 KB patch. That time is too short for a fresh review of this size, and zero findings of every kind (not even dropped noise) fits a cache hit or empty reviewer output better than a real pass. The brief requires "a confirmed high-effort review of the final patch", and this row does not show one. Open `results/issue_1029/review-batch.md` and confirm it reviewed this revision, the one with the new fault-site tests, and not the iteration-v1 bundle.
- [ ] `C4-diff-cov` says "patch.diff does not apply on origin/main", so coverage of the changed lines was never measured. One possibility is that #1028 (the slice this one builds on) is not merged yet, which is expected for a stacked slice. The other is that `main` has moved and the bundle is stale. I can't tell from the snapshot (the target has no `origin` refs). Confirm the base before merge. My 34 mutations cover the same new lines, but only for the paths I chose.
- [ ] **P1: the new not-landed proof can compensate an already committed part after delayed staging batches restore its slot** (`crates/core/src/multipart/commit.rs:1204`, `crates/core/src/multipart/commit.rs:1258`). With the existing fixed-clock rig, script `Reserve`, `Intent`, and the quiesce `Renewal` each as `[Act::hold(), Act::pass()]`, then stage one chunk and let its part commit apply but answer unknown. Before its re-read, release the three held batches in order. Successful retries leave those earlier invocations outstanding (`crates/core/src/multipart/stage.rs:1335`, `crates/core/src/multipart/stage.rs:2338`, `crates/core/src/multipart/stage.rs:1743`). The delayed reserve recreates the deleted slot; the delayed intent recreates its owned entry; the delayed renewal restores exactly the slot/entry bytes the commit required. Retry stamps can be identical (`crates/core/src/multipart/stage.rs:1567`). The re-read sees the attempt's own part, marks the absent-part precondition permanently lost at `crates/core/src/multipart/commit.rs:1187`, then clears the doubt because the restored slot is `Held`. It compensates and answers `OperationAborted`, leaving both the durable part and a retirement obligation for its own chunks. This diff introduces that compensation after an unknown result; the previous code stopped in doubt. Add this cross-phase regression and establish a guarantee that earlier staging batches cannot restore the evidence used to settle a commit. The brief excludes changes to staging, so providing that guarantee requires an explicit scope/protocol decision; this is not the deferred damage-only slot-erasure case.
- [ ] **The rollback criteria require opposite outcomes for the same history.** `brief.md:33-38` requires a held, unapplied commit interrupted by a rollback at *each* read gap to reach a later slot read, compensate, and return `Refused(OperationAborted)`. But H (`brief.md:61`) requires a held commit interrupted by a rollback immediately after the slot read to return in doubt without compensation. With the slot unchanged, reads succeeding, and lease/round budget remaining, that history satisfies both descriptions. The design itself requires the extra slot read (`brief.md:154-156`); the cited target peer likewise re-reads after detecting the session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise H to specify what prevents settlement—such as exhaustion before a confirming read, a failed confirming read, or the commit landing before the rollback—or align its expected result with the read-gap criterion. Otherwise the two required tests can demand incompatible behavior.
- [ ] size backstop — this slice is behaving oversized: patch is 153 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected for the code-review P1 (§6 item 4), confirmed against the base: the new settle can compensate a part that landed. A held Reserve stays outstanding after its retry is adopted (stage.rs:1345-1375; TiKV may apply it later, traits/src/lib.rs:244-247). Once the commit deletes the slot, its require_absent is true again, so it can recreate the slot, and a delayed Renewal with an identical retry stamp (stage.rs:1567) can restore the exact bytes the commit required. Settle then reads "slot still held at my bytes" as "did not land", compensates, and leaves a durable part with a retirement obligation for its own chunks. That is a C-1 data-loss path that this diff introduces; the old code stopped in doubt. Re-plan must decide the protocol: either staging guarantees that no earlier staging batch can restore the slot/entry evidence (a scope change, since the brief excludes stage.rs), or commit settlement stops treating unchanged slot bytes as proof that its own commit did not land. Add the cross-phase regression (Reserve, Intent and the quiesce Renewal each [hold, pass], commit applies but answers unknown, release the held batches before the re-read). Also: drop the stale "H vs read-gap" item (the brief already resolves it); diff coverage failed only because #1028 is unmerged; reconsider the split (153 KB) while in Plan.
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
