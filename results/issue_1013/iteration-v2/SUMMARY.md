# Result — issue 1013 / multipart-session-terminal-delete

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 a session's `retire:` obligations drain, but nothing ends the
  session: its owned `sidx:` residue (crashed in-flight intents from #657's staging) stays and
  keeps its fragments protected; its `mpu:` and `slot:` records stay; and `mpuctl.count` only
  rises, until every Create answers `503`. And an untrusted staged record on a dead upload (a
  placement that fails validation, or an `sidx:` value that will not decode) is held for ever:
  child-1 holds it, GC holds every fragment of its chunk, and the session can never reach its
  exit. This child adds the owned `sidx:` walk, retirement of untrusted records by chunk id for
  `Aborting` sessions, and the terminal session delete with its exactly-once `mpuctl` decrement,
  dispatched from the drain leg child-1 added to `reconcile_step`.
- Success criterion: A NEW test file
  `crates/custodian/tests/multipart_session_terminal.rs`, over `RedbMetadataStore::in_memory()`
  and a real chunk store or placement-aware double, driving everything **through
  `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue, and a `Completed` session holding in-flight residue, each
  drain to: every owned fragment marked from its `sidx:` placement, then the entry deleted; no
  `sidx:` record of the session left. **`Aborting`:** the terminal delete fires **once**,
  `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a wrapper
  still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no `seg:`
  record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with the
  contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795` on `12dd082`); with the drain
  context's tombstone input saying the window has passed, the terminal delete fires once, as for
  `Aborting`. The terminal delete does not fire while the session's `sidx:` range is non-empty,
  nor while a session-scoped `retire:` obligation is left.
  **(E) Untrusted records.** For an `Aborting` session, each of these is retired **by chunk id**
  on a pass whose fleet is the whole operator-configured fleet: (E1) a `part:` record whose
  placement has the wrong length under a supported scheme (an RS(4,2) chunk placed on 5 servers;
  the maintainer's case); (E2) an owned `sidx:` entry with the same wrong-length placement (it
  decodes: `StagedPlacement` admits a length mismatch, `crates/core/src/multipart.rs:4441` on
  `db1a066`); (E3) an owned `sidx:` value that will not decode (e.g. an unsupported scheme, which
  `StagedPlacement` refuses, `multipart.rs:4481`), its chunk id taken from its key
  (`parse_sidx_key`, `multipart.rs:1436`) exactly as GC's hold takes it
  (`crates/custodian/src/gc.rs:1392`). For each: every fragment bearing that chunk id that the
  whole-fleet listing finds is marked, and for E2 every position the entry itself names as well;
  then the record is deleted (a recording wrapper shows the order); after GC past grace those
  fragments are gone and the terminal delete has fired.
  **Late landing** (a wrapper forces listing → record deletion → fragment publication): for E2,
  a fragment that lands at a position the entry names finds this retirement's mark and is gone
  after grace; for E3, a fragment that lands after the listing stays on disk across GC passes,
  never reclaimed and never an error: the stated residual (Design).
  **Controls:** an `Open` session with the same kind of record keeps the record and every
  fragment of its chunk; a committed object's fragments on the same servers are untouched; a
  server left out at startup, and a server unreachable this pass (each leaving the fleet short of
  `operator_fleet_size`), each leave the record and every fragment of its chunk in place with a
  typed hold, and once the server is back and the fleet is whole the record retires and that
  server's fragments are gone after grace too; a `part:` value that will not decode (an
  unsupported scheme such as `RS(0,0)`, which `decode_part_record` refuses) is held with a typed
  outcome and an audit event saying it needs a human, and the session's terminal delete does not
  fire.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the owned `sidx:` walk for `Aborting` and `Completed` sessions; retirement by chunk
  id of untrusted `part:` and `sidx:` records for `Aborting` sessions, on whole-fleet passes only
  (replacing child-1's hold for that case); the terminal session delete and its exactly-once
  `mpuctl` decrement; the tombstone input on the drain context, with the deployed caller passing
  "retain"; the docs. /
  out of scope:
  - the drain step itself, its cursor, and the segments-before-parts rule (child-1);
  - the migration gate and the routing switch (child-3);
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the
    `W_tombstone` clock (#625); the operator abort (#633);
  - recovering a `part:` value that will not decode: its key names no chunk
    (`part:<id>:<n>`, `multipart.rs:1386`), so this child holds it (E's control);
  - seeded DST (#660 owns "two concurrent drainers"; add no DST scenario);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

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

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `gitleaks` confirmed absent (detect cmd `gitleaks version` exited 127). Install hint: install gitleaks v8.30.1 (the rev the wyrd hook pins) so a builder can run the target's commit hook. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] External dependency `gitleaks` confirmed absent (detect cmd `gitleaks version` exited 127). Install hint: install gitleaks v8.30.1 (the rev the wyrd hook pins) so a builder can run the target's commit hook. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [ ] external dependency: gitleaks — the target's commit hook (`origin/main`
- [ ] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [ ] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [ ] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [ ] size backstop — this slice is behaving oversized: patch is 244 KB (threshold 100 KB); patch touches 24 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Resume only: iteration 2 was never reviewed because gitleaks was missing; it is now installed (v8.30.1, ~/.local/bin/gitleaks). Keep iteration 2's patch and design (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); do not redesign. Run the target's commit hook (forbidden-file check + gitleaks) over the patch and record the result in build-notes.md, then let Check run the full review. Size: still one slice, as decided in iteration 1; do not split.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- wyrd-pdca: add gitleaks `[[doctor.checks]]` row to pdca.toml (proposed in build-notes.md:287)
