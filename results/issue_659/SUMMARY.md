# Result — issue 659 / multipart-retirement-drain

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Every multipart slice so far installs `retire:` obligations (Abort's `{session, all}`,
  #657's per-part `{chunks}`, #658's `{parts}` and `{generation}`), and nothing drains them; a
  session's records and its admission slot are never released. This slice adds the drain and
  dispatches it in production: a byte- and operation-budgeted, cursor-keyed drain step for
  `retire:bytes:` (mark, then delete the naming records) and `retire:records:` (delete only, never
  mark); the walk of an `Aborting` or `Completed` session's owned `sidx:` entries; the terminal
  session delete with its exactly-once `mpuctl` decrement; the drain's dispatch from
  `reconcile_step` in the deployed custodian; the orphan-identity migration gate; and, last, the
  switch of `unlink` and the superseding commits from inline orphan marks to one
  `retire:bytes:{generation}` obligation.
- Success criterion: A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (core-level tests MAY also go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  drain **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), not through a test-only entry. It shows all of the
  following.
  **(A) Positive reclamation after `unlink`** (the discontinued run's leak). `MAX_BATCH_OPS` is
  **500** (`crates/core/src/multipart.rs:5093-5106`), and today's unlink batch spends 2
  preconditions + 2 deletes + one mark put per fragment (`crates/core/src/metadata.rs:2133-2165`).
  (A1) The tracker's historical case: a committed 30-chunk RS(6,3) object (270 fragments, 274
  operations, which fits one batch, so the inline path may still serve it) is deleted with
  `metadata::unlink`. After reconcile steps past the orphan grace, every one of its 270 fragments
  is gone. (A2) **Past the batch budget:** a committed flat 100-chunk RS(6,3) object (900
  fragments, 904 operations inline) is deleted with `metadata::unlink`. **Before any reconcile
  step**, the unlink's own batch has installed exactly one `retire:bytes:g:<inode>:<version>`
  obligation and written no `orphan:` mark. After reconcile steps past the orphan grace, **every
  one of its 900 fragments is gone from the chunk store** and no `retire:` obligation or
  `orphan:` mark of it remains. **The boundary, preconditions counted:** an 83-chunk RS(4,2)
  object (498 marks + 2 deletes + 2 preconditions = 502 operations; 500 if the preconditions were
  ignored) also takes the obligation path, with no inline mark. (A3) The same as A2 for an
  ordinary overwrite (`write::commit_overwrite`) of such an object, for the prior generation's fragments, and for an
  `unlink` of a **segmented** object published by #658's Complete (no inline path exists for
  one). Asserted as fragments absent, never as "no failure observed".
  **(B) Complete, then drain.** After a Complete (#658) and the drain: no `part:` or `psum:` record
  of a published part remains; **no** `orphan:` mark names a chunk of the published map
  (`0016:1503`); a subset Complete's unnamed part has every fragment marked **before** its `part:`
  record is deleted (a recording wrapper shows the order) and is reclaimed after grace.
  **(C) Convergence at scale.** A `{parts}` obligation over **at least 4,001 parts** (planted as
  canonical records is fine) drains to empty across many steps, each commit within the byte and
  operations budgets (preconditions counted); running the whole drain a second time is a no-op. A
  single entry whose own deletes exceed one batch's budget still makes progress every step and
  finishes. Assert that the obligation's stored cursor (see Design) advanced every step, that a
  drain restarted from a fresh process state (nothing in memory) resumes from the stored cursor,
  and that the obligation is deleted only after its last batch.
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue (from #657's staging), and a `Completed` session holding
  in-flight residue, each drain to: every owned fragment marked from its `sidx:` placement then the
  entry deleted; no `sidx:` record of the session left. **`Aborting`:** the terminal delete fires
  **once**, `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a
  wrapper still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no
  `seg:` record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with
  the contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795`); with the drain context's
  tombstone input saying the window has passed (see Design), the terminal delete fires once, as
  for `Aborting`. The terminal delete does not fire while the session's `sidx:` range is
  non-empty.
  **(E) Validated placements and untrusted records.** A `part:` or `sidx:` record whose placement
  fails validation (length not the scheme's fragment count, e.g. a decoded `RS(0,0)`) is never
  deleted unmarked. For an `Aborting` session, such a record and an owned `sidx:` value that will
  not decode are retired **by chunk id** per the 2026-09-18 tracker comment's rule: every fragment
  bearing that chunk id is marked, then the record is deleted, and after GC past grace those
  fragments are gone and the terminal delete has fired. Controls: an `Open` session with the same
  kind of record keeps the record and every fragment of its chunk; a committed object's fragments
  on the same servers are untouched.
  **(F) The migration gate** (`0016:1251-1278`). Before the cleanup marker exists, the
  identity-keyed retirement paths are off and GC treats every `orphan:` mark as protecting. The
  cleanup pass walks `orphan:` page-wise, and a mark found on a still-referenced fragment is
  **re-stamped, not dropped**, when the reference is a `part:` record (the trap the 2026-09-12
  comment records: dropping it there strands the fragments after a later `unlink`). When the pass
  finishes it writes the durable marker, and only then do (A)'s routing and GC's keyed lookup
  apply.
  **(G) Restore stays accurate between drain batches** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (its two obligations: `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, seeded as `restore_completing_fence.rs` seeds them)
  is drained **one step at a time**. After **every** step, the production
  `reconcile_after_restore` (`restore.rs:597`) runs and does not name the session (no
  `ChunkInNoPart`, `restore.rs:1225-1234`, and no `NoDeleter`), until the terminal delete.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the retirement drain step for both modes, the owned-`sidx:` walk, the terminal session
  delete, the drain's dispatch from `reconcile_step` in the deployed custodian, the orphan-identity
  cleanup pass with its durable completion marker and the gate on it, the retirement of untrusted
  staged records for `Aborting` sessions, the switch of `unlink` and the superseding commits to
  `retire:bytes:{generation}` above one batch's worth of fragments, and the docs. / out of scope:
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the `W_tombstone`
    clock (#625): the terminal delete here fires for `Aborting` sessions and, for `Completed`
    sessions, only when the drain context's tombstone input says the window has passed; the
    deployed caller passes "retain" until #625 supplies that input;
  - the operator abort (#633);
  - seeded DST: #660 owns exactly three cases, one of them "two concurrent drainers"; do not add a
    DST scenario here (the discontinued run's review flagged extra scenarios as structure creep);
  - the bulk `DeleteObjects` per-request batching (#509's, inside the per-transaction bound
    `0016:1107-1122` settles);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR (ADR-0045's refinement is #811); `Cargo.lock` and
    `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] **The headline regression can pass without exercising retirement.** `brief.md:40` says `MAX_BATCH_OPS = 250`, and `brief.md:127` claims today's 270-mark unlink exceeds it. The target actually defines **500** (`crates/core/src/multipart.rs:5093–5106`); today's unlink adds one put per fragment (`crates/core/src/metadata.rs:2133–2165`). Because `brief.md:250–252` permits small flat objects to retain inline marking, the 30-chunk fixture can remain inline and reclaim successfully even with drain dispatch removed, contradicting the promised red at `brief.md:79–80`. Keep the tracker's historical 30-chunk case, but add a fixture exceeding the actual full-batch budget and assert that unlink/overwrite installs a generation obligation before reconciliation. Correct the current-defect framing and count preconditions as well as mutations.
- [x] **Durable cursor storage is an undeclared persisted-format decision.** `brief.md:210–218` requires a persisted cursor, including progress inside an oversized entry, while `brief.md:271–273` promises no existing record-shape changes. On the supplied target, `RetirePayloadWire` has no cursor and rejects unknown fields (`crates/core/src/multipart.rs:3549–3562`); the stored payload likewise has none (`:3620–3632`). Specify whether this slice extends that codec or introduces separate progress records, and reconcile the compatibility claim accordingly. Include the progress record's atomic update, lifetime and restart behavior in scope; otherwise Do must invent an additional persistence contract to satisfy (C).
- [x] **Completed-session deletion has no specified production eligibility source.** `brief.md:112–114` delegates the tombstone-window judgment to #625, yet (D) requires Completed sessions to reach terminal deletion through deployed contexts (`brief.md:35–37,55–62`). The terminal-delete algorithm lists only empty-range/obligation gates (`brief.md:226–230`). The existing `GcContext` supplies orphan grace, not tombstone eligibility (`crates/custodian/src/gc.rs:332–342`), and the deployed caller supplies no such decision (`crates/server/src/custodian.rs:681–698`). The normative Completed exit also requires the tombstone window to elapse (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:574`); premature deletion changes an identical Complete retry from `AlreadyCompleted` to `NoSuchUpload` (`crates/core/src/multipart.rs:4789–4795`). Define the production handoff/default until #625 lands and test retention before eligibility as well as deletion afterward. If production completion depends on #625, reflect that dependency instead of promising it unconditionally.
- [x] **The restore deferral needs a behavioral guarantee, not just a replacement comment.** `brief.md:172–173` asks to update `restore.rs:1156`, but no acceptance leg covers its recorded failure: “a half-drained range can then read as chunks no part holds.” The target actually implements that check: `recheck_fenced` calls `check_attempt`, which compares surviving segments against remaining parts and reports `ChunkInNoPart` (`crates/custodian/src/restore.rs:1180–1182,1207–1212,1229–1234`). The proposed independent bytes/records drain permits deleting an Aborting session's parts while its records-mode segment cleanup is unfinished. A subsequent restore pass can therefore report legitimate intermediate state as damaged. Require an ordering guarantee or a progress-aware restore check, with a deterministic restore rerun between drain batches; merely retaining the obligation until its last batch does not preserve the parts this check needs.

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
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
