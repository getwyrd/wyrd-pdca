# Brief — staged-drain-status

> Child 1 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `f41e9c5`
> (re-verified 2026-09-18). Base is plain `origin/main`. Background: the drain and rebalance
> rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`, `:826-827`) and the
> failure table (`:874-890`, `:881`, `:883`).

- **Slug:** staged-drain-status
- **Kind:** enhancement
- **Defect:** drain status tells an operator a server may be wiped while a live upload's bytes
  are on it. `reconciliation_status` answers `Satisfied` for a server holding only staged
  bytes, because its `genuinely_holds` test reads committed placements alone
  (`crates/custodian/src/desired_state.rs:181-196`) — the F6 trace. The sharper form is an
  in-flight part with no `part:` record yet (`0016:827`). The class that answers this already
  exists and is unread here: `StagedSet::protects` (`crates/custodian/src/gc.rs:705`). Second,
  0016 requires a rebalance pass over a draining server holding only staged fragments to plan
  no move and rewrite no `part:` record while drain status answers `Pending` for that same
  server (`0016:881`; `plan_evacuations`, `crates/custodian/src/rebalance.rs:257`). Nothing
  asserts that today. #803 left this to #664 by name: `deferred: #663, #664` at `gc.rs:669` and
  `crates/custodian/tests/staged_protection.rs:2160`.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_drain_status.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes
  in `crates/core/tests/multipart_session_records.rs:81-145`). Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class
  passes one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. Iteration 1's `*server != dserver` mutant survived
  every other leg; this case kills it. It is green on the base too — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For a
  draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The red
  comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass returns
  there, and why it does not tell an operator the drain is done.
  **(E) An unreadable or untrusted staged record never yields `Satisfied`.** A staged record the
  query cannot read blocks every drain; one it can read but not trust blocks them the way a
  committed map with an untrustworthy placement does (mirror `StagedSet::protection`,
  `gc.rs:690`).
  **(F) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on plain `origin/main` @ `f41e9c5`, no
  container, by **assertion**: the base's drain status counts committed placements only, so A,
  B and D's `Pending` half fail there. C is a guard. The new test may name only base-visible
  symbols (`wyrd_custodian::{reconciliation_status, set_lifecycle, reconcile_step, Reconciled,
  GcContext,
  RebalanceContext, ReconciliationStatus, DServerLifecycle}`,
  `wyrd_core::multipart::{mpu_key, part_key, sidx_key}`, `wyrd_traits`). A compile failure on
  the RED leg reports UNVERIFIABLE (`engine/scripts/run-verify.sh`). Record in `build-notes.md`
  how many tests ran red, all by assertion.
- **Invariant to restore:** a drain is `Satisfied` only when no byte that can still become
  referenced — committed, committed-part or in-flight — lives on that server. Source: 0016
  decision 2's drain and rebalance rows (`0016:826-827`, `:881`); the C-1 rule that a
  certification over an incomplete picture is a defect (`docs/principles.md` §5). SELF-TEST:
  counting only `part:` in the drain misses the in-flight `sidx:` case.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on `origin/main` @ `f41e9c5`, seed an owned `sidx:` entry whose fragment
  sits on server `S`, set `desired:dserver:<S>` with `set_lifecycle`, and call
  `reconciliation_status(S)`: it answers `Satisfied`.
- **Scope:** `reconciliation_status` reads the existing `StagedSet` and counts both staged
  classes as held; rebalance is confirmed disjoint from the staged set (expected: a test and a
  comment, no behaviour change — if `plan_evacuations` needs a real change, say why in
  `build-notes.md`); #664's half of the two `deferred:` markers is discharged, leaving #663's;
  the drain-status sentence in `docs/design/architecture/06-runtime-view.md:78` is corrected.
  / out of scope: anything in `restore.rs`, `crates/core/src/multipart.rs` or
  `crates/server/src/cli.rs` (child-2, child-3); rebuilding the staged class (#803, merged);
  `scrub.rs` and `reconstruction.rs` (#663); the mark codec (#804);
  `crates/dst/tests/custodian.rs`; evacuating committed segmented objects (#653/#722); any edit
  to 0016 or an ADR.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_drain_status.rs` — a **NEW** file. The
  C4-verify gate earns its red only from an added `*/tests/*.rs`
  (`engine/scripts/run-verify.sh`), so do not append to `staged_protection.rs`. The existing
  dev-dependencies suffice; no `Cargo.toml` change.
- **Difficulty:** medium
- **Conflicts with:** 809, 810, 813, 814, 804
- **Ordering note:** wave 1 of #664's split. Shares no logic with child-2 or child-3, but shares
  the paragraph at `06-runtime-view.md:78` with both and `gc.rs` with child-2, so it never
  shares a wave with them. After acceptance add `Conflicts with: 663, 804` (same doc). **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the passes under test are the production `reconciliation_status` and
  the rebalance loop. Every staged record is seeded by the test, because no client can create a
  session until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:672-781` — `StagedSet`, `protection` (`:690`), `protects`
    (`:705`), `staged_fragments` (`:809`). The class to read, not rebuild.
  * `crates/custodian/src/gc.rs:515` — `ReferenceSet::protects`, the committed twin.
  * `crates/custodian/src/desired_state.rs:181-247` — `reconciliation_status` and
    `genuinely_holds` (`:191`).
  * `crates/custodian/src/rebalance.rs:257` — `plan_evacuations`.
- **Prior-art check (triage cycles):** by path (`desired_state.rs`, `rebalance.rs`), re-run
  2026-09-18 on `f41e9c5`: #803 (PR #807, merged) explicitly excluded drain status and
  rebalance; no open PR touches them. #664 iteration 1
  (`results/issue_664/iteration-v1/patch.diff`) carried this work inside a 211 KB patch; its
  drain hunks drew no blocking finding, but the `*server != dserver` mutant survived — leg C.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether to rerun or waive clean aggregate CI—focused red→green, all custodian tests, and both implicated server binaries pass independently, but frozen CI first failed an unrelated health test and its confirmation timed out (`gate-logs/C4-ci.log:2272`, `:2299`), leaving criterion F unproven.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
