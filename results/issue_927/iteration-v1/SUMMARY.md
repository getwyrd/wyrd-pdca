# Result — issue 927 / multipart-upload-part-commit

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.1 a part's chunks can be staged, but nothing turns them into a part. This
  slice adds the second half of `UploadPart` as `wyrd-core` functions. It (1) commits the
  `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch, after a last
  renewal. It (2) classifies a `Conflict` or a `CommitUnknownResult` from a re-read, never
  blind. It (3) runs the live-session compensation, including against a session rolled back to
  a newer `Open@E'`. It (4) handles a re-upload of a committed part number, retiring the prior
  part's chunks in the same batch. It (5) refuses a part past the per-part or cumulative chunk
  ceiling and compensates what it staged. It (6) composes the whole `UploadPart` (reserve →
  stage → commit), answering in the landed vocabulary (`UploadPartOutcome`
  `crates/core/src/multipart.rs:4606`, `Refusal` `:4487`).
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`,
  over the same harness as 657.1's test: `RedbMetadataStore::in_memory()`, a test-local
  placement-aware chunk store, and a store wrapper that logs batches and can inject `Conflict`,
  `CommitUnknownResult` (either flag, applied or not), a held batch landed later, a write before
  a chosen commit, or **an action between two reads**. Each core test file carries its own copy,
  as `crates/core/tests/multipart_abort.rs` does. Every session is made by #894's Create, and
  every part is staged by 657.1. The test shows all of the following.
  **(A) Commit.** Parts staged **out of order** (3, then 1, then 2), at least one of them more
  than one chunk, each commit to `Committed { part_number, digest }`. After each commit, the
  part's `part:` and `psum:` records decode and agree (chunk count, length, digest), and no
  `sidx:` entry or `slot:` record of that attempt remains. The wrapper's log shows the attempt's
  last renewal **before** the commit's reads, and no renewal batch between those reads and the
  commit (`0016:1720-1733`).
  **(B) A lost commit is classified, never compensated blind** (`0016:1584-1653`).
  - The session fenced to `Aborting` between the last intent and the commit: the commit answers
    `Refused(NoSuchUpload)` and writes nothing (the residue is the reaper's).
  - **The session rolled back to `Open@E'` (`E' > E`)** between the last intent and the commit
    (a Complete fenced it and rolled back, decision 3). The attempt compensates under
    `require(mpu == Open@E')` (`0016:1607-1620`): one `retire:bytes:` obligation for its own
    chunks, its slot released, and its `sidx:` entries deleted, all in one batch. It answers
    exactly `Refused(OperationAborted)` (see Design § *The answer for a lost commit on a live
    session*), never `Committed` and never `NoSuchUpload`, because the session is `Open`.
  - A same-part rival committed first: the loser commits one `retire:bytes:` obligation for its
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3891`),
    releases its slot and deletes its `sidx:` entries in one batch, and answers exactly
    `Refused(OperationAborted)`.
  - A commit whose batch lands but answers `CommitUnknownResult`: the attempt re-reads, finds its
    own commit, answers `Committed`, and its slot is released exactly once.
  - A re-upload whose batch did not land (`may_still_commit: false`): the re-read finds the
    **prior** part. The attempt must not answer `Committed` for it; it carries on as for a commit
    that did not land, and ends with its own part committed.
  - **A held batch** (`may_still_commit: true`; the wrapper holds it and lands it later): the
    re-read finds the prior part, or none. The attempt neither answers `Committed` nor
    compensates on that read alone, and it sends **no compensation batch while the held batch
    may still land** (Design § *Settle before compensating*). Every batch it sends next requires
    its own slot at the bytes the held batch also requires, so at most one of them lands. Run it three times: (i) the held
    batch released before the attempt's next batch; (ii) released after the attempt has
    returned; (iii) every later batch answered `Conflict` unapplied until the rounds run out, then
    the held batch released. In every run: exactly one of {its own part, its compensation} is
    durable at the end; the slot is released exactly once; and the answer is `Committed` only if
    its own part is the one that landed. In (iii) the attempt returns an in-doubt error that keeps
    the `may_still_commit` doubt, as Abort's does (`crates/core/src/multipart/abort.rs:264-274`).
  - A same-number rival plus an unknown outcome, **with this attempt's commit not applied**
    (revised 2026-10-22): the wrapper answers the commit `CommitUnknownResult` without applying
    it (run once per `may_still_commit` value; the part number had no record before, so the
    commit required the key absent), a non-empty rival part then lands, and the
    re-read finds the rival's record **and this attempt's own slot still held at the bytes it
    last wrote**. The held slot is what proves the commit did not land (the commit deletes it),
    and because the commit required a value that cannot come back (the key absent, or a non-empty
    prior), it never can.
    Never `Committed`; the attempt compensates under its own slot and answers exactly
    `Refused(OperationAborted)`. This is the history that tells B apart from C3, whose commit
    **did** land and whose slot is gone: the own-commit proof is checked **first**, and the
    rival-record branch applies only when it fails (Design § *After a
    `CommitUnknownResult`*).
  - **A held commit, then an unknown empty compensation** (added 2026-10-22). An empty re-upload
    of part 1 over an empty part 1, clock fixed (C1's shape). The wrapper holds the commit
    (`may_still_commit: true`, not applied), then lands an empty same-number rival in the same
    millisecond, whose bytes equal the prior's, so the held commit's `require(part == prior)`
    can still hold. From then on, if the attempt sends a compensation batch (one that deletes its
    slot and puts no `part:`), the wrapper first lands the held commit and then answers the
    compensation `CommitUnknownResult { may_still_commit: true }` unapplied; any other batch it
    answers `Conflict` unapplied. A held commit is released, if still held, after the attempt
    returns. Required: the wrapper's log shows **no compensation batch** sent while the held
    commit was unsettled; the answer is never `Refused(OperationAborted)` — it is `Committed` if
    the attempt saw its slot gone with no compensation sent, else the in-doubt error keeping
    `may_still_commit`; at the end the slot is released exactly once and no `retire:` key
    exists. An implementation that compensates while the commit is held, then reads "slot gone at
    `Open@E`" as its compensation landing, answers `OperationAborted` for a commit that landed,
    and fails here.
  **(C) The three counterexamples found in #657's first attempt, each as a regression.**
  - **C1, an empty part looks the same as somebody else's.** With a fixed clock, an empty part 1
    is committed. Then an empty re-upload of part 1 in the same millisecond and session epoch
    commits with an answer of `CommitUnknownResult` and is **not applied** (run once per
    `may_still_commit` value). Its record's bytes equal the prior's, because an empty part has no
    chunk ids of its own. The answer must not be `Committed` on the strength of those bytes. When
    the call returns, the attempt's slot is no longer held, or, for an in-doubt return, is still
    covered by the doubt it reports; the session's held-slot count equals its live attempts
    (`0016:1779`). Same for a same-number rival whose empty part landed in the same millisecond.
  - **C2, a torn read.** The commit is held (`may_still_commit: true`) and never applied. At
    **each** gap between the reads the attempt makes to classify it, in turn, the wrapper runs a
    real #895 Abort and then deletes the session's `sidx:` entries and `slot:` records (standing
    in for #659's teardown). The answer is never `Committed`, since no part exists. It is
    `NoSuchUpload` or an in-doubt error. A reaper cannot touch an `Open` session (`0016:1628`), so
    a slot that vanished proves the attempt's own release only if the session was still the same
    `Open@E` when the slot was found gone.
  - **C3, a landed commit, then a rival before the re-read.** The attempt's commit **lands** but
    answers `CommitUnknownResult { may_still_commit: false }`. A rival re-upload of the same part
    number then replaces the part before the re-read. The answer is `Committed`: its slot vanished
    while the session stayed `Open@E`, and only its own commit could do that. It is never an
    error claiming that no batch is in doubt and that its slot and staged entries are left, when
    its commit released both. **C3′, and the freed index is reused** (2026-10-20): as C3, and
    then a third attempt reserves the index the commit freed and holds it, all before the
    re-read, with the session still `Open@E`. The re-read finds the rival's part and a slot at
    that index carrying **another** `attempt_id` (`SlotRecord::attempt_id`,
    `multipart.rs:2732-2734`). The answer is still `Committed`, and the third attempt's slot is
    byte-for-byte unchanged: the attempt sends no batch that requires or rewrites it. 0016 names
    that third attempt as a writer of the key (`0016:1625-1628`), so "the key is present" does
    not mean "my slot is still held".
  - **C4, empty parts and retirement** (2026-10-20). An obligation must name at least one chunk:
    `RetireObligation` refuses an empty `chunks` list and an empty payload
    (`multipart.rs:3272-3277`, `:3705-3711`; pinned by
    `crates/core/tests/multipart_retire_obligation.rs:467-473`), and this slice keeps that
    contract. So an obligation is installed only when the chunk list it would name is non-empty,
    and the slot release and `sidx:` deletes stay in the same batch either way. Shown by: an
    empty re-upload over an empty prior (C1's shape, every batch applied) commits with **no**
    `retire:` key written; an empty
    re-upload over a non-empty prior writes one obligation for the prior's chunks; and an empty
    part that loses to a same-number rival compensates with a batch that releases its slot and
    writes **no** obligation, answering `Refused(OperationAborted)`. That empty compensation's
    unknown outcome (landed; not landed with `may_still_commit: false`; held) has no obligation
    to re-read, so it is settled by the attempt's own slot and the session (see Design): each
    run ends with the slot released once and no `retire:` key, or an in-doubt error carrying
    the doubt.
  - An unknown outcome on the **compensation** batch (landed; not landed with
    `may_still_commit: false`; held) is settled by re-reading its own obligation, its own slot
    and the session (for an empty part, slot and session alone; C4). Each run ends with exactly
    one obligation and the slot released once, or an in-doubt error carrying the doubt.
  - Every loop of this slice (commit retry, compensation, the epoch chase after a rollback) has a
    stated maximum. With a wrapper that answers every batch `Conflict` unapplied, each ends in a
    typed error after **exactly** that many commits (the test counts them).
  **(D) Re-upload and ceilings.**
  - Re-uploading a committed part number replaces it under `require(part == prior)` and puts the
    prior part's chunks in a `retire:bytes:` obligation in the same batch (when the prior part
    has at least one chunk; C4).
  - A part whose staging hit the per-part bound (657.1) answers `Refused(EntityTooLarge { .. })`
    and compensates what it staged in one batch.
  - **The cumulative ceiling is a read-time check, and 0016 allows the race it leaves**
    (`0016:1441-1451`; `multipart.rs:5034-5043`). A commit whose read of the session's `psum:`
    range shows it would take the cumulative staged chunks past `MAX_STAGED_CHUNKS` answers
    `Refused(EntityTooLarge { .. })` and compensates. Concurrent commits may overshoot by at most
    `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`. **Witness, forced by the wrapper:** with `psum:`
    records planted one chunk below the ceiling, two attempts on two different part numbers, each
    holding a one-chunk part, both read that one chunk of headroom (the wrapper holds the first
    commit until the second has committed, then releases it). Both commit, and the total is
    `MAX_STAGED_CHUNKS + 1`, inside the bound. A third commit after them refuses. Do **not** add a
    shared counter or a session-wide fence to stop the overshoot; 0016 rejects that
    (`0016:1686-1742`). Use a test-sized ceiling only if the function takes it as a parameter;
    otherwise plant `psum:` records to approach it.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree, the tracker's acceptance
  line and `AGENTS.md:41-45`'s rule.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the second half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
  the part commit and its last renewal; the classification of a lost or unknown commit; the
  live-session compensation, including against a rolled-back `Open@E'`; the re-upload of a part
  number with the prior part's chunks retired; the per-part and cumulative ceiling refusals; and
  the composed `UploadPart`. Also: the writer-side constructor the part record lacks, with its
  `psum:` summary derived from it, the per-part `{chunks}` row of `RetireObligation` (which
  keeps refusing an empty list), the widened doc of `Refusal::OperationAborted` (no new variant),
  the docs sentence, and the new test file. / out of scope:
  - the slot reserve, the owned intent, the drain fence, placement, the fragment writes and
    renewal itself (657.1: #932 and #933; change them only where this slice's hand-off needs it,
    and say why);
  - Complete, the segment committer and publication (#658, #653); draining any `retire:`
    obligation, the owned-`sidx:` walk and the terminal delete (#659); the reaper (#625);
    seeded DST races (#660);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
    any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 82.5% — 438 of 531 instrumentable changed lines executed (floor 80%); 531 of 1109 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 63 mutants tested in 9m: 9 missed, 42 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #927’s UploadPart commit/composition: the staged-part protocol is implemented, but commit reads and retries can outlive the final renewal’s lease.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scoped core-only delivery has falsifiable ownership, atomicity, compensation and ceiling requirements; the quiesced lease must cover the actual commit deadline (brief.md:306; docs/design/proposals/draft/0016-multipart-commit-protocol.md:1726). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept or reject the declared compile-only pre-fix exception — independently reverting production leaves the new test unable to import the absent API, so no behavioral red executes (brief.md:233; review-red.log:2; crates/core/tests/multipart_upload_part_commit.rs:44). |
| C3 Change | PASS | The four changed files stay within the approved core, record-constructor, test and living-documentation surfaces; production integration remains deferred as specified (crates/core/src/multipart/commit.rs:107; crates/core/src/multipart.rs:3028; docs/design/architecture/05-building-block-view.md:246). |
| C4 Verification (red→green) | FAIL | The restored patch passes all 18 supplied tests, but an independent executable coverage invariant fails: a retry starts after lease expiry (review-restored-green.log:25; review-lease-probe.log:6; crates/core/src/multipart/commit.rs:709). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve lease coverage through preparation and every retry, with an advancing-clock regression — otherwise the final renewal cannot prevent an active upload becoming eligible for idle reaping (crates/core/src/multipart/commit.rs:627; crates/core/tests/multipart_upload_part_commit.rs:81; review-lease-probe.log:11). |
| T1 Structure | PASS | Core uses the existing metadata/chunk-store traits and the attempt’s clock, without adding concrete production dependencies or shared mutable globals; seeded DST coverage is explicitly deferred to #660 (crates/core/src/multipart/commit.rs:101; crates/core/src/multipart/commit.rs:113; crates/core/src/multipart/commit.rs:120). |
| T2 Shape | PASS | Atomic slot/owned-entry transitions and canonical writer records preserve the required record relationships; retry counts are finite and empty retirement lists are excluded (crates/core/src/multipart/commit.rs:528; crates/core/src/multipart/commit.rs:564; crates/core/src/multipart.rs:4103). |
| T3 Runtime | FAIL | Individually terminating store calls do not preserve a shared lease deadline: the probe sends its second commit 13,998 ms after expiry (crates/core/src/multipart/commit.rs:633; crates/core/src/multipart/commit.rs:709; review-lease-probe.log:7). |
| T4 Contribution | FAIL | The batch-review gate’s three messages describe one still-reproducible lease defect; the contribution-artifact audit is separately N/A until its mandatory publish rerun (gate-logs/T4-batch-review.log:10; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN | Confirm the merged and closed/rejected prior-art check by affected path — the brief records earlier triage, but this target contains one synthetic base commit and no PR history to corroborate rejected work (brief.md:293; review-prior-art.log:1). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the core-only delivery and OperationAborted response for live-session losses — gateway mapping and production reach remain #669, so the core tests do not establish the eventual client-facing behavior (brief.md:275; brief.md:354; crates/core/src/multipart.rs:4670). |

**One confirmed defect: preserve the quiesced lease until each commit’s deadline.** At `crates/core/src/multipart/commit.rs:627`, `Covered { .. }` discards both the check time and expiry returned by renewal. The code then scans summaries, reads the prior part, and can send four commits separated by classification reads, without checking the remaining coverage. The hand-off explicitly requires the commit to check those values against its deadline (`crates/core/src/multipart/stage.rs:376`), and proposal 0016 requires quiescing not to let the lease lapse under the commit (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1727`). The backend’s per-operation termination contract (`crates/traits/src/lib.rs:1333`) does not bound this entire sequence.

The independent probe uses the unchanged patched production code, a real in-memory redb store, and the supplied test harness copied into a separate scratch crate. It advances the same ManualClock by 4,000 ms for each completed metadata read and injects one unapplied Conflict. The final renewal expires at 1,010,002 ms; the first commit starts at 1,008,000 ms with only 2,002 ms of the required 5,000 ms remaining, and the second starts at 1,024,000 ms. The call still answers Committed. The invariant assertion fails, exit 101 (`review-lease-probe.log:6`). This directly demonstrates the deadline breach; exposure to an idle-reaper fence follows from the expired lease, rather than an exercised production reaper. The supplied suite deliberately never advances time (`crates/core/tests/multipart_upload_part_commit.rs:81`), so its green result cannot cover this case.

The builder should retain and enforce the renewal’s absolute coverage across preparation and retries, stop safely before a new batch lacks its transaction window, and preserve any outstanding CommitUnknownResult doubt. Add a regression that advances time during reads and retry rounds. Simply renewing between the existing commit reads and commit would break the quiesce contract. The three frozen batch-review messages are the same defect, counted once here. No optional-capability probe or load-time symptom guard is introduced by this diff.

Reproduce the independent failure from the review directory:

```sh
cargo test --offline --manifest-path pdca-reviewer-927-probe/Cargo.toml --target-dir target/target --test lease_probe reviewer_commit_preserves_lease_coverage -- --nocapture
```

**Verification distinguishes local reruns from frozen gate evidence.** Production-only changes were stashed while retaining the new test: cargo exited 101 with E0432 before running tests (`review-red.log:2`). After stash pop, the same command passed all 18 tests (`review-restored-green.log:25`). Local `cargo clippy --offline -p wyrd-core --tests -- -D warnings`, formatting, typos, docs lint and docs rendering also completed successfully; rendering produced 101 pages and passed its link audit (`review-docs-render.log:2`). These exercise both external dependencies named in the brief, without a substitute tool. The additional lease probe is the actual patched-tree verification failure, not an environment fault or the expected compile-only base result.

| Frozen row | Adjudication | Evidence and limit |
|------------|--------------|--------------------|
| C4-ci | PASS from captured execution | The log records real typos, docs render, workspace fmt/clippy/build/tests, dependency checks and DST checks, ending with all checks passed; the complete workspace/DST campaign was not independently rerun (gate-logs/C4-ci.log:11; gate-logs/C4-ci.log:4133; gate-logs/C4-ci.log:4512). |
| C4-verify | NEEDS-HUMAN | Independently reproduced: 18 green tests, then E0432 on the reverted API; no behavioral pre-fix red (gate-logs/C4-verify.log:10; gate-logs/C4-verify.log:24). |
| C4-diff-cov | PASS from captured execution | 438/531 instrumentable changed lines executed, 82.5%; 578 changed lines were unscored, so this is not coverage of every changed line (gate-logs/C4-diff-cov.log:64; gate-logs/C4-diff-cov.log:170). |
| C5-mutants | FAIL, advisory | 63 tested: 42 caught, 12 unviable and nine missed; the surviving mutations require semantic triage, not nine assumed production defects (gate-logs/C5-mutants.log:13; gate-logs/C5-mutants.log:22). |
| T4-batch-review | FAIL | Three messages, one grounded lease-coverage defect independently reproduced above (gate-logs/T4-batch-review.log:10). |
| T4-contribution | N/A | PR description is absent by design during Check; the substantive audit must run at publish (gate-logs/T4-contribution.log:10). |
| host-tikv | PASS from captured compilation | The captured command compiles both the TiKV crate tests and server selection features; this establishes compilation, not a live TiKV service test (gate-logs/host-tikv.log:2; gate-logs/host-tikv.log:211). |

All seven named logs are present and readable. Instance-root wrapper paths were adjudicated from their captured commands/output; their absence from this target is not a defect.

**The mutation survivors expose evidence limits, without proving additional current logic defects.** Independently reading the affected source gives these dispositions for all nine:

- The two arithmetic mutations at `crates/core/src/multipart/commit.rs:154` shorten the declared compensation budget. They are not equivalent: the loop-bound tests compare against the same exported constant (`crates/core/tests/multipart_upload_part_commit.rs:2092`), so changing the constant changes both behavior and expectation.
- Removing Display output at `crates/core/src/multipart/commit.rs:348` and the two replacements of `lossy` at `crates/core/src/multipart/commit.rs:449` alter diagnostics. They do not demonstrate a durability defect in the current implementation.
- Removing `UploadPartError::source` at `crates/core/src/multipart/commit.rs:357` is not equivalent: it can hide the inner unknown outcome from `classify`, whose fallback is Terminal (`crates/traits/src/lib.rs:786`). The composed-verb test covers success (`crates/core/tests/multipart_upload_part_commit.rs:1280`); current source preserves the chain, but the suite does not protect it.
- Making `Doubt::required` always true at `crates/core/src/multipart/commit.rs:419` removes the session identity proof used after an unknown compensation (`crates/core/src/multipart/commit.rs:912`). This is not equivalent when teardown removes the slot after the session changes; current source retains the proof.
- Removing `!` at `crates/core/src/multipart/commit.rs:862` leaves the initially empty obligation list empty, bypassing obligation-based settlement; slot/session evidence can mask that mutation in the supplied live-session fixtures. It is not generally equivalent when the session has moved after compensation landed.
- Making the comparison at `crates/core/src/multipart/commit.rs:879` unconditional accepts foreign obligation bytes instead of reporting damage. It changes the damage branch, rather than exposing a presently incorrect implementation.

**Prior-art and scope remain explicit sign-off decisions.** A target-only `git log --all -- <path>` check was run for each of the four affected paths (`review-prior-art.log:2`). Existing files yield only synthetic base `cfb6fa3`; the two added files have no history, and there are no remotes. The brief’s path-based triage names merged work and the earlier #657 attempt, but closed/rejected history is not furnished, so completeness is not mechanically established. The tracked #660 DST deferral is settled under the standing rubric and is not raised again. No production caller or gateway behavior is added by this slice.

All source citations above refer to the supplied `$PDCA_TARGET`; evidence/probe paths refer to the review directory. The target was readable, the restored patch passed `git apply --reverse --check ../patch.diff`, and its original four-file modification set was restored. No production source was edited by the reviewer; the separate probe and logs remain in the harness-owned review directory.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #927 (UploadPart commit, 657.2)

I re-ran the new test file on a scratch copy of `$PDCA_TARGET`. All 18 tests pass. C4-verify's
UNVERIFIABLE is the brief's pre-declared case (the test cannot compile without the new module).
It is not a refutation. Build-notes were withheld, so I re-made several of the brief's required
"red" edits myself and added five probe tests in scratch only (P1–P5; nothing written to the
target). Findings, worst first:

- NEEDS-HUMAN [impl] — **A commit can land after the attempt's own lease has lapsed. This confirms
  the T4 BLOCKING finding.** `crates/core/src/multipart/commit.rs:627-631` throws away
  `RenewOutcome::Covered { checked_at_millis, lease_expiry_millis }`. The staging API's contract
  says the commit must use them: "The commit checks both against its own deadline"
  (`crates/core/src/multipart/stage.rs:376-378`). The loop at `commit.rs:700-709` then sends up
  to `COMMIT_ROUNDS` = 4 commits, plus a psum scan, a part read and four reads per round, all on
  one 5 s cover. 0016 says the quiesce must make sure the lease "cannot lapse under the commit"
  (`0016:1726-1728`). **Concrete case (probe P3):** three benign `Conflict` rounds, each taking
  4 s (each one inside its own `E_tx`). The fourth commit lands with the clock at `1_012_000`,
  but the slot lease it requires ran out at `1_010_002`. The answer is still `Committed`. Once
  the session's last progress is older than `W_open`, this is the window the reaper's idle arm
  fences in (`0016:665`, the "reap a progressing upload" row at `0016:2258`). Fix: before each
  commit round, check the clock against `lease_expiry_millis - COMMIT_COVER_MILLIS`, and stop
  (or re-quiesce, then re-read) instead of sending.

- NEEDS-HUMAN [impl] — **The guard that stops a compensation while a commit may still land is
  not covered by any test.** If I replace `if doubt.any() { continue; }` at `commit.rs:795` with
  `if false && …`, all 18 shipped tests still pass. The brief assigns this red to the
  held-commit / unknown-empty-compensation leg. That leg cannot catch it: its rival's bytes equal
  the prior's, so with the guard gone the code falls through to the benign retry anyway
  (`commit.rs:802`). **Concrete case (probe P5):** the commit is held; right after the slot read
  (read 3) the held commit lands and a Complete rolls the session back to `Open@E'`. The shipped
  code correctly answers `InDoubt{SessionMoved}` and sends no compensation. With the guard
  deleted, it compensates under `E'` and answers `Stopped{SlotLost}`, which says "no batch of
  this part attempt is in doubt", for a part that is committed (the C3 defect class). Add P5,
  or an equivalent, to the suite.

- NEEDS-HUMAN [impl] — **Four C5 survivors are not equivalent. Each hides a wrong answer that a
  short test exposes:**
  (a) `Doubt::required -> true` (`commit.rs:418-420`). Probe P2: an empty part loses to a
  rival, its compensation is held, then an Abort and a teardown run. The shipped code answers
  `InDoubt`. The mutant answers `Refused(OperationAborted)` ("compensated, the session is still
  usable") for an aborted session where nothing of this attempt landed.
  (b) `delete !` at `commit.rs:862` (and the `found == *bytes` guard at `:879`). Probe P1: the
  compensation lands with an unknown outcome, then a client Abort runs before the re-read. Only
  the obligation re-read settles this. The mutant skips that read and answers `InDoubt` for a
  compensation that landed. No shipped test ever finds the obligation on re-read, and no test
  reaches `Stop::ObligationKeyTaken`.
  (c) `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS - 1` (`commit.rs:154`). Probe P4: one
  rollback beats the commit, then two more beat compensations. Three rollbacks is
  `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The mutant runs out of rounds and leaves
  its slot held under a live `Open` session, which is the trap 0016 warns about
  (`0016:1607-1615`). The bounded-rounds leg cannot catch this: it compares against the imported
  constant (`crates/core/tests/multipart_upload_part_commit.rs:2092`, `:2113`), so it checks
  itself, not the bound.
  (d) `UploadPartError::source -> None` (`commit.rs:355-361`). `classify` would then read an
  in-doubt composed `UploadPart` as `Terminal` (`crates/traits/src/lib.rs:786-803`). This is the
  rubric's *Transactions* class (`CommitUnknownResult` must outrank a definite failure), and the
  composed verb is what #669 will map to the wire. No test checks the class of an
  `upload_part` error.

- NEEDS-HUMAN [human] — **A failed quiesce renewal, or spent commit rounds with nothing in doubt,
  leave the slot and `sidx:` residue under a live session.** `commit.rs:627-631`
  (`CommitError::Renewal`) and `commit.rs:701-706` (`Stopped{CommitRoundsSpent}`, pinned by
  "nothing was released" at `multipart_upload_part_commit.rs:2082`) both return without
  compensating. Their docs say the residue "stays for the reaper" (`commit.rs:176-179`). But the
  reaper does not walk an `Open` session (`0016:1600-1606`), and 0016 is normative that "every
  refusal that happens AFTER chunks are staged must compensate", or repeats fill
  `MAX_INFLIGHT_PARTS` and the session answers `503` until `W_session` (`0016:1420-1431`). When
  the renewal is settled (`StageError::Stopped`, not `InDoubt`), the attempt knows its slot bytes
  and could compensate. The brief only says to "treat its typed coverage error as a refusal to
  commit". A human should decide whether a settled coverage stop counts as a post-staging local
  refusal under `0016:1420`.

- **Attempted to refute, and could not.** These required reds hold. Ignoring `attempt_id` in
  `judge_slot` (`commit.rs:473`) turns C3′ red. Judging the session from the read taken before
  the slot read (`:756`) turns C2 red. Sending the rolled-back `Open@E'` to the reaper (`:814`)
  turns the rollback leg red. Treating an unknown compensation as not landed (`:873`) turns the
  compensation leg and C4 red. Checking the rival-record branch before the own-commit proof
  (`:767`) turns C3 and the landed-unknown leg red. I also checked by hand that every exit to
  `compensate` comes only after a slot read that clears every commit still able to land
  (`commit.rs:748-750`, `:795-818`); apart from the unpinned guard above, I found no reachable
  compensation while a commit is unsettled. Obligation keys cannot collide between a commit and
  a compensation of the same attempt, because the slot compare-and-set lets only one of them
  land. The `psum:` overshoot witness matches `0016:1441-1451`. DST coverage is deferred to #660
  with an in-code marker (`commit.rs:113`), so I treat it as settled.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:627` and `crates/core/src/multipart/commit.rs:709`: **The quiesced lease can expire under an active commit.** Renewal guarantees only five seconds of remaining coverage when it returns, and its returned expiry is discarded. The subsequent summary scan, part read, classification reads and up to four commit attempts consume independent deadlines without checking remaining coverage. With a legal ten-second lease, a four-second scan, four-second read and four-second commit already cross expiry. For a long-running upload whose last progress timestamp is older than `W_open`, the idle reaper can then fence the unchanged expired slot and abort the progressing upload. Retain the expiry and enforce sufficient remaining lease coverage before each commit, bounding intervening work and preserving any outstanding commit doubt. Add a regression that advances the clock during reads/retries: `crates/core/tests/multipart_upload_part_commit.rs:81` freezes time throughout the current suite, so its passing tests cannot expose this defect.

No additional correctness or reuse/simplification findings. Review used the target source and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept or reject the declared compile-only pre-fix exception — independently reverting production leaves the new test unable to import the absent API, so no behavioral red executes (brief.md:233; review-red.log:2; crates/core/tests/multipart_upload_part_commit.rs:44).
- [ ] C5 Causal adequacy — Preserve lease coverage through preparation and every retry, with an advancing-clock regression — otherwise the final renewal cannot prevent an active upload becoming eligible for idle reaping (crates/core/src/multipart/commit.rs:627; crates/core/tests/multipart_upload_part_commit.rs:81; review-lease-probe.log:11).
- [ ] T5 Judgment — Confirm the merged and closed/rejected prior-art check by affected path — the brief records earlier triage, but this target contains one synthetic base commit and no PR history to corroborate rejected work (brief.md:293; review-prior-art.log:1).
- [ ] Validation — fitness-to-purpose — Approve the core-only delivery and OperationAborted response for live-session losses — gateway mapping and production reach remain #669, so the core tests do not establish the eventual client-facing behavior (brief.md:275; brief.md:354; crates/core/src/multipart.rs:4670).
- [ ] C4-verify — Independently reproduced: 18 green tests, then E0432 on the reverted API; no behavioral pre-fix red (gate-logs/C4-verify.log:10; gate-logs/C4-verify.log:24).
- [ ] **A commit can land after the attempt's own lease has lapsed. This confirms the T4 BLOCKING finding.** `crates/core/src/multipart/commit.rs:627-631` throws away `RenewOutcome::Covered { checked_at_millis, lease_expiry_millis }`. The staging API's contract says the commit must use them: "The commit checks both against its own deadline" (`crates/core/src/multipart/stage.rs:376-378`). The loop at `commit.rs:700-709` then sends up to `COMMIT_ROUNDS` = 4 commits, plus a psum scan, a part read and four reads per round, all on one 5 s cover. 0016 says the quiesce must make sure the lease "cannot lapse under the commit" (`0016:1726-1728`). **Concrete case (probe P3):** three benign `Conflict` rounds, each taking 4 s (each one inside its own `E_tx`). The fourth commit lands with the clock at `1_012_000`, but the slot lease it requires ran out at `1_010_002`. The answer is still `Committed`. Once the session's last progress is older than `W_open`, this is the window the reaper's idle arm fences in (`0016:665`, the "reap a progressing upload" row at `0016:2258`). Fix: before each commit round, check the clock against `lease_expiry_millis - COMMIT_COVER_MILLIS`, and stop (or re-quiesce, then re-read) instead of sending.
- [ ] **The guard that stops a compensation while a commit may still land is not covered by any test.** If I replace `if doubt.any() { continue; }` at `commit.rs:795` with `if false && …`, all 18 shipped tests still pass. The brief assigns this red to the held-commit / unknown-empty-compensation leg. That leg cannot catch it: its rival's bytes equal the prior's, so with the guard gone the code falls through to the benign retry anyway (`commit.rs:802`). **Concrete case (probe P5):** the commit is held; right after the slot read (read 3) the held commit lands and a Complete rolls the session back to `Open@E'`. The shipped code correctly answers `InDoubt{SessionMoved}` and sends no compensation. With the guard deleted, it compensates under `E'` and answers `Stopped{SlotLost}`, which says "no batch of this part attempt is in doubt", for a part that is committed (the C3 defect class). Add P5, or an equivalent, to the suite.
- [ ] **Four C5 survivors are not equivalent. Each hides a wrong answer that a short test exposes:** (a) `Doubt::required -> true` (`commit.rs:418-420`). Probe P2: an empty part loses to a rival, its compensation is held, then an Abort and a teardown run. The shipped code answers `InDoubt`. The mutant answers `Refused(OperationAborted)` ("compensated, the session is still usable") for an aborted session where nothing of this attempt landed. (b) `delete !` at `commit.rs:862` (and the `found == *bytes` guard at `:879`). Probe P1: the compensation lands with an unknown outcome, then a client Abort runs before the re-read. Only the obligation re-read settles this. The mutant skips that read and answers `InDoubt` for a compensation that landed. No shipped test ever finds the obligation on re-read, and no test reaches `Stop::ObligationKeyTaken`. (c) `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS - 1` (`commit.rs:154`). Probe P4: one rollback beats the commit, then two more beat compensations. Three rollbacks is `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The mutant runs out of rounds and leaves its slot held under a live `Open` session, which is the trap 0016 warns about (`0016:1607-1615`). The bounded-rounds leg cannot catch this: it compares against the imported constant (`crates/core/tests/multipart_upload_part_commit.rs:2092`, `:2113`), so it checks itself, not the bound. (d) `UploadPartError::source -> None` (`commit.rs:355-361`). `classify` would then read an in-doubt composed `UploadPart` as `Terminal` (`crates/traits/src/lib.rs:786-803`). This is the rubric's *Transactions* class (`CommitUnknownResult` must outrank a definite failure), and the composed verb is what #669 will map to the wire. No test checks the class of an `upload_part` error.
- [ ] **A failed quiesce renewal, or spent commit rounds with nothing in doubt, leave the slot and `sidx:` residue under a live session.** `commit.rs:627-631` (`CommitError::Renewal`) and `commit.rs:701-706` (`Stopped{CommitRoundsSpent}`, pinned by "nothing was released" at `multipart_upload_part_commit.rs:2082`) both return without compensating. Their docs say the residue "stays for the reaper" (`commit.rs:176-179`). But the reaper does not walk an `Open` session (`0016:1600-1606`), and 0016 is normative that "every refusal that happens AFTER chunks are staged must compensate", or repeats fill `MAX_INFLIGHT_PARTS` and the session answers `503` until `W_session` (`0016:1420-1431`). When the renewal is settled (`StageError::Stopped`, not `InDoubt`), the attempt knows its slot bytes and could compensate. The brief only says to "treat its typed coverage error as a refusal to commit". A human should decide whether a settled coverage stop counts as a post-staging local refusal under `0016:1420`.
- [ ] `crates/core/src/multipart/commit.rs:627` and `crates/core/src/multipart/commit.rs:709`: **The quiesced lease can expire under an active commit.** Renewal guarantees only five seconds of remaining coverage when it returns, and its returned expiry is discarded. The subsequent summary scan, part read, classification reads and up to four commit attempts consume independent deadlines without checking remaining coverage. With a legal ten-second lease, a four-second scan, four-second read and four-second commit already cross expiry. For a long-running upload whose last progress timestamp is older than `W_open`, the idle reaper can then fence the unchanged expired slot and abort the progressing upload. Retain the expiry and enforce sufficient remaining lease coverage before each commit, bounding intervening work and preserving any outstanding commit doubt. Add a regression that advances the clock during reads/retries: `crates/core/tests/multipart_upload_part_commit.rs:81` freezes time throughout the current suite, so its passing tests cannot expose this defect.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- [ ] **Compensation's slot-only success rule does not exclude an outstanding commit.** `brief.md:285-289` says a vanished own slot at the compensation epoch proves compensation landed; unlike the commit proof at `brief.md:309-311`, it has no condition excluding the other kind of release. Yet `brief.md:57-65` and `:317-319` allow subsequent slot-fenced batches while an earlier commit remains in doubt. If an empty commit is held, an empty compensation is attempted, and the held commit wins before compensation returns unknown, the slot is gone at the same `Open@E` although compensation never landed. The slot CAS proves at most one release, not which operation won. Empty compensation has no obligation to distinguish it (`$PDCA_TARGET/crates/core/src/multipart.rs:3276`); late application is expressly allowed by `$PDCA_TARGET/crates/traits/src/lib.rs:240-247`. Revise the brief to require a settled-not-committed outcome before switching to compensation, as `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1649-1653` specifies, or retain doubt whenever either operation could explain the release. Add a combined delayed-commit/unknown-empty-compensation regression, rather than testing the two unknown outcomes only separately.
- [ ] **Two success criteria prescribe different answers for the same stated observation.** `brief.md:67-69` says an unknown commit followed by a read of a same-number rival's record must never answer `Committed` and must compensate. C3 (`brief.md:86-90`) expressly requires `Committed` after an unknown commit followed by that same rival-record observation, because this attempt committed before the rival replaced it. The distinguishing history is missing from B. Specify that B's original commit was unapplied and its own slot remains held, and give the own-commit proof precedence over the rival-record branch. Otherwise the stated acceptance criteria can direct Do to recreate the very C3 defect they require it to eliminate.
- [ ] **The mandatory C3′ mutation red need not be reachable.** `brief.md:208-210` requires each red to arise from its named edit alone; `brief.md:223-224` assigns the mutation “retry requires re-read slot bytes instead of last-written bytes” to C3′. But C3′ (`brief.md:91-96`, `:307-311`) must recognize the released ownership and return `Committed` before sending another batch. A conforming implementation can therefore survive a mutation confined to its retry batch builder: that builder is never reached in this fixture. Change the required evidence to a separate reachable retry/compensation interleaving that exercises slot-byte selection, or an explicit batch-precondition invariant; do not require this fixture to kill an edit to code it need not execute.
- [ ] size backstop — this slice is behaving oversized: patch is 166 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rejected: oversized (166 KB vs 100 KB, gating T4 FAIL) with one confirmed defect and an open design question. Whether and how to split is left open to Plan (the six jobs in the Defect list are candidate seams). 1. Confirmed defect, any re-plan must carry it: the final renewal's RenewOutcome::Covered { checked_at_millis, lease_expiry_millis } is discarded (crates/core/src/multipart/commit.rs:627-631), so reads and up to COMMIT_ROUNDS commits run past the slot lease and still answer Committed (review-lease-probe.log:6; adversary probe P3). Require: keep the expiry and check remaining coverage before every commit round (stop safely, keeping any outstanding CommitUnknownResult doubt), per stage.rs:376-378 and 0016:1726-1728; plus a regression with a clock that advances during reads and retries (the suite's ManualClock never moves, multipart_upload_part_commit.rs:81). 2. Design question for Plan to decide in the brief: when the quiesce renewal fails with a settled error (StageError::Stopped, not InDoubt), or commit rounds are spent with nothing in doubt (commit.rs:627-631, :701-706), must the attempt compensate (release its slot and sidx: entries)? Today it leaves the residue "for the reaper" (commit.rs:176-179), but the reaper does not walk an Open session (0016:1600-1606) and 0016:1420-1431 says every post-staging refusal must compensate, else repeats fill MAX_INFLIGHT_PARTS and the session answers 503 until W_session. 3. Tests that missed real bugs; the brief must require tests that kill each: the doubt.any() guard before compensation (commit.rs:795; adversary P5); Doubt::required (commit.rs:418-420; P2); the obligation re-read after an unknown compensation (commit.rs:862/:879; P1, and reach Stop::ObligationKeyTaken); COMPENSATION_ROUNDS checked against a literal bound of MAX_COMPLETE_ATTEMPTS rollbacks, not the imported constant (commit.rs:154; P4); the error class of a composed upload_part in-doubt error via UploadPartError::source (commit.rs:355-361, traits/src/lib.rs:786-803). 4. Re-verify the 2026-10-22 brief fixes (own-commit proof before rival-record branch; reachable slot-byte-selection red; combined held-commit / unknown-empty-compensation leg) and re-run path-based prior art (merged + closed-unmerged) on a target that has PR history.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
