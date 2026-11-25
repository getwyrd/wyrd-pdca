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
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 254 mutants tested in 84m: 53 missed, 179 caught, 20 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.55s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1013’s multipart residue retirement and exactly-once terminal session deletion: no confirmed implementation defect; verification exceptions and operational fitness require human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required exit preserves byte evidence, releases admission exactly once, and retains Complete retries; these obligations match the normative terminal-delete rule and the accepted scope (brief.md:25; docs/design/proposals/draft/0016-multipart-commit-protocol.md:674). |
| C2 Reproduction (red pre-fix) | N/A | This is explicitly new functionality with no executable baseline discriminator: independently stashing the tracked fix leaves four missing-API compilation errors, not an assertion red; acceptance of that declared exception is owed under C4 (brief.md:115; brief.md:123; reviewer-red.log:4). |
| C3 Change | PASS | An admission slot cannot be released twice by competing terminal deletes, and completed-upload retries retain their record; the session and ledger preconditions and deployed retention input support those outcomes (crates/custodian/src/retirement/sessions.rs:950; crates/custodian/src/retirement/sessions.rs:977; crates/server/src/custodian.rs:580). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared new-API verification exception and unavailable stacked diff coverage — 43 feature tests pass independently, but no behavioral pre-fix red or changed-line coverage was established (reviewer-green.log:191; reviewer-red.log:4; gate-logs/C4-verify.log:47; gate-logs/C4-diff-cov.log:10). |
| C5 Causal adequacy | PASS | The lifecycle now has the missing terminal exit, with evidence preceding deletion; the latest starvation and post-delete bookmark races have exercised regressions, and targeted ordering/progress/tombstone mutations are caught (crates/custodian/tests/multipart_session_terminal.rs:3373; crates/custodian/tests/multipart_session_terminal.rs:3491; reviewer-mutants.log:4). |
| T1 Structure | PASS | The change stays on MetadataStore/ChunkStore seams and stamps marks from the same loop clock; it adds no shared mutable production global or parallel dispatch path (crates/custodian/src/retirement.rs:527; crates/custodian/src/retirement/sessions.rs:631; crates/custodian/src/reconciliation.rs:168). |
| T2 Shape | PASS | The approved single-slice scope remains reviewable: about 42 KB of added non-comment source, with tests and context updates accounting for most of the diff; persisted walk records and behavior are documented (patch.diff:1145; docs/design/architecture/05-building-block-view.md:246; docs/design/architecture/06-runtime-view.md:92). |
| T3 Runtime | PASS | Budget boundaries and held-session progress are exercised, terminal races release one slot, and the deployed loop preserves the startup-partial-fleet hold and completed tombstones (crates/custodian/tests/multipart_session_terminal.rs:1398; crates/custodian/tests/multipart_session_terminal.rs:2143; crates/custodian/tests/multipart_session_terminal.rs:3681; reviewer-ci.log:2899). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit is required at publish, not waived or awaiting human clearance (gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | Path-based merged-history and closed-work searches ran for all 25 affected files; the recorded #636 rejection’s missing-production-dispatch failure is avoided, and the explicit #660/#825 deferrals remain settled (reviewer-prior-art.json:2; reviewer-prior-art.json:1424; crates/custodian/src/reconciliation.rs:168; brief.md:342). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the operating tradeoffs meet deployment needs — completed tombstones retain admission until #625, unnameable late fragments remain for operator repair, and retirement can require up to 78 fleet listings per pass at the shipped profile (crates/server/src/custodian.rs:580; crates/custodian/src/retirement.rs:138; crates/custodian/src/retirement.rs:158). |

The evidence supports the requested behavior, with these limits:

- **Independent execution:** `cargo test --offline -p wyrd-custodian --test multipart_session_terminal` passed all 43 tests. I stashed the tracked production changes, retained the new discriminator, observed the missing `LISTED_PER_CHUNK`, `Tombstone`, `operator_fleet_size` and `tombstone` APIs, then restored the stash. The restored test suite passed again during the CI run (reviewer-ci.log:1877). This confirms the frozen verifier’s limitation, not a behavioral baseline red. The applied target still passes `git diff --check` and `git apply --reverse --check ../patch.diff`.

- **Full gate and host caveat:** the independent `cargo xtask ci` run completed with 2,319 reported passing tests and one failing test. Its two failed checks both concern the disposable index’s missing `.github/advisory-db` gitlink: the index test and advisory-pin preparation (reviewer-ci.log:3835; reviewer-ci.log:4482). This is harness metadata, not a patch defect. The frozen gate actually ran the pinned advisory checks and finished green (gate-logs/C4-ci.log:4349; gate-logs/C4-ci.log:4995). Independently, formatting, workspace clippy/build, conformance, statics, DST, typos and docs lint/render ran; both TiKV/server feature clippy commands also passed (reviewer-tikv.log:104; reviewer-tikv.log:204). The brief’s `typos` and `docs-renderer` dependencies were exercised, not shimmed or skipped. Gitleaks found no leaks in the patch (reviewer-gitleaks.log:2).

- **Mutation evidence is partial:** the frozen full sweep reports 254 mutants: 179 caught, 53 missed, 20 unviable and two timeouts (gate-logs/C5-mutants.log:68). My focused nine-mutant run caught eight, including removal of the idle-step budget exemption and changes to deletion ordering, bookmark selection and tombstone retention (reviewer-mutants.log:4). The remaining survivor removes the per-entry `record` override when a fleet listing fails (crates/custodian/src/retirement/sessions.rs:791; reviewer-mutants.log:3). The feature double’s listing always succeeds, so this fault-path attribution is not established by those tests (crates/custodian/tests/multipart_session_terminal.rs:562). The current code supplies the correct key; this survivor does not demonstrate a current-code failure. I do not claim that all frozen survivors are equivalent or resolved.

- **Stacking and prior art:** the supplied target is usable and the patch matches it. The coverage gate’s inability to apply the patch to `origin/main` is the acknowledged prerequisite-base mismatch, not a compile or applicability defect in this target (brief.md:334; gate-logs/C4-diff-cov.log:10). Searches covered each affected path’s main-branch history and closed PR descriptions; the results, rejected PR #647’s file list, and #636’s rejection record are retained in reviewer-prior-art.json. The [#636 rejection](https://github.com/getwyrd/wyrd/issues/636#issuecomment-6094881252) concerned the oversized slice and missing production drain; this patch has exercised deployed dispatch. Search results are bounded prior-art evidence, not a claim to possess the discontinued local patch. The frozen batch-review log reports zero blocking findings (gate-logs/T4-batch-review.log:10).

The C5 capability-probe smell test does not fire: state, placement, fleet and tombstone checks enforce the cleanup contract rather than conceal a load-time side effect. The new destructive-path DST work remains explicitly deferred to #660 (crates/custodian/src/retirement/sessions.rs:249), and the never-landed-mark issue remains outside this slice under the accepted #825 disposition (brief.md:342). Tier-1/Tier-2 operational observation would complement these in-process tests; neither was run or claimed as verified here.

All source citations above refer to the supplied `$PDCA_TARGET`; brief, gate-log and reviewer-log citations refer to this review directory. No production source was edited, and the stash was restored.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1013, iteration 6

**Bottom line: I could not refute the fix.** I re-ran the evidence in a scratch copy of `$PDCA_TARGET` (`cargo test -p wyrd-custodian --test multipart_session_terminal`: 43/43 pass). Then I removed each guard this round added, and each guard the brief requires a demonstrated red for, one at a time. Every removal turned at least one test red. The findings below are one design question for the human, one doubt about a gate, and some notes. None of them is a concrete failing case in the code.

## Refutation attempts that failed (the evidence holds)

- **This round's three fixes each have a test that goes red without them** (scratch copy, one revert at a time):
  - `crates/custodian/src/retirement/sessions.rs:887` charge every step again (`if true`): `d_sixteen_sessions_held_past_one_step_spend_no_step` fails. That is the starvation regression the carry-forward asked for.
  - `sessions.rs:853` drop the session-record precondition on the bookmark put: `d_no_bookmark_is_put_once_another_drainer_has_ended_the_session` and `d_a_lost_bookmark_costs_reads_not_progress` fail. That is the recreated-bookmark regression.
  - `sessions.rs:834` let a re-read with no progress take the bookmark while the walk moves past held entries (`.filter(|_| true)`): `d_a_trusted_entry_behind_three_held_part_way_retires` (the steady-state-quiet assertion at `multipart_session_terminal.rs:2981-3018`) and `d_sixteen_sessions_held_part_way_spend_no_step` fail.
- **The brief's required reds reproduce independently.**
  - No tombstone check at `sessions.rs:927`: the `Completed` leg fails.
  - Whole-fleet gate forced open at `retirement.rs:417`: all four startup and unreachable fleet controls fail, plus five more tests.
  - E2's named positions dropped at `sessions.rs:434`: `e2_…_retired_by_chunk_id` (late landing) fails.
  - Session precondition removed from the terminal batch at `sessions.rs:950`: `d_two_drainers_racing_the_terminal_delete_release_the_slot_once` fails.
- **The tests use the production path, not a copy.** `pass()` (`multipart_session_terminal.rs:739-778`) builds a `ReconstructionContext` with the same fields the deployed loop sets (`crates/server/src/custodian.rs:564-581`) and calls the real `reconcile_step`.
- **A stuck session reporting `Changed` cannot make the loop spin.** The `PastOneStep` sessions move their bookmark every pass and can report `Changed` with nothing fixed. But the deployed loop discards the outcome (`crates/server/src/custodian.rs:602`, `:624` — `Ok(_) => {}`).
- **A stale session-record precondition cannot loop.** A walk step retries with the record bytes from the listing and never re-reads them (`sessions.rs:364-366`). That could only waste the pass's steps if the record changed. But `Aborting` and `Completed` records have no transition out except deletion: every fence requires `Open` or `Completing` (`crates/core/src/multipart.rs:2716-2756`).
- **No obligation can slip in between the terminal delete's emptiness check and its commit** (`sessions.rs:933-940`). Per-part obligations sit under the same `retire:<mode>:s:<id>:` range (`multipart.rs:1603-1622`, `:1661-1665`). The only writer that installs one after Abort or Complete, a losing writer's compensation, requires `mpu == Open@E` (`crates/core/src/multipart/commit.rs:14`).
- **A store page cut short to zero rows cannot pass as "range empty".** `checked_page` rejects an empty page that has `next` set (`crates/custodian/src/gc.rs:2108-2121`), so `Walked::Empty` (`sessions.rs:625-627`) cannot fire the terminal delete early.
- **The new `passing` filter cannot throw away the bookmarked entry's own progress.** The bookmarked entry is walked first (`sessions.rs:666-673`). Either it uses up the step's reads, so nothing after it is walked and nothing is held (so `passing` is false), or it finishes and is deleted (so the step is not idle). Only a re-read of a *different* entry is dropped, which is what was intended.

## Findings

- NEEDS-HUMAN [human] — **The tombstone input is one switch for the whole pass, but the window it stands for is per session.** `crates/custodian/src/retirement.rs:389-396` defines `Tombstone::{Retain, Elapsed}`, and `sessions.rs:927` reads it the same way for every `Completed` session. Each session's own `Completion.completed_at_millis` (`crates/core/src/multipart.rs:2313-2324`) is never consulted. No caller can pass `Elapsed` safely while sessions complete at different times. Passing it when only the oldest session's window has passed deletes younger tombstones early, so a retried Complete gets `NoSuchUpload`. Passing it only when every window has passed means that, with steady traffic, no `Completed` session is ever deleted, `mpuctl` fills, and Create returns `503`. So #625 will have to *replace* this type, not just "supply the tombstone input this child defines" as the brief says. That is safe today, because the deployed caller hard-codes `Retain` (`crates/server/src/custodian.rs:580`). The human should decide whether to accept the type as a stub that #625 must reshape (and say so on #625), or have it carry a per-session predicate now. Measuring the window across clocks stays #625's either way (ADR-0009).
- NEEDS-HUMAN [human] — **Check that the T4 review actually ran.** `check-gates.json` reports T4-batch-review as passing and gating: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **35 s** (`gate-logs/T4-batch-review.log`). That is three Codex multi-pass reviews plus triage of a 7,020-line diff. The four earlier rounds each found 2–7 blocking items. 35 s is short enough that a failed or empty run counted as zero findings would look exactly like this. I cannot see `review-batch.md`. Before trusting this pass, confirm it contains three completed reviews of *this* patch. I found no defect it should have caught, so this is a doubt about the evidence, not about the code.
- **Note (not a refutation): a session with two entries held deeper than one step can read is never write-quiet.** It writes its bookmark every pass, forever, and the test asserts this on purpose (`multipart_session_terminal.rs:3627-3630`). Add the scan-position writes, and that is at most 3 uncharged commits per such session per pass (`sessions.rs:859-889`; module doc `:108-115`). This is the carry-forward's option 1 ("do not charge a step"), applied in full. Its option 2 ("do not let a zero-progress re-read displace a held entry's count") is applied only when the walk moves past held entries (`sessions.rs:833-835`). It is bounded by `max_sessions` and only costs writes, so it needs no action unless the human wants these sessions fully quiet.
- **Note (C5, advisory): most of the 53 surviving mutants are equivalent or unreachable.**
  - Mutants on the ops and byte budgets (`sessions.rs:512-513`, `:573-578`, `:689-700`) survive because the 233-read budget always runs out first (static assert at `sessions.rs:189-190`).
  - `:1004`, `:244` and `:439` mutate checks for states that cannot happen: a record changing in `Aborting`, an empty bookmark key, a key under the range with another owner.
  - `:1032` and `:1040` are masked by the visit loop's own `steps_left == 0` check (`sessions.rs:358-361`).
  - **One mutant marks a real untested path, though the code is correct.** `crates/custodian/src/retirement/marks.rs:590` (`&&`→`||` at col 54) would take the listing even after a hold inside a part's trusted positions. An untrusted chunk with **zero** listed fragments would then add 1 to `covered` and save a cursor past the unmarked held position. No test combines "stopped inside trusted positions" with "untrusted chunk listed empty". Adding one is optional.
  - Likewise, nothing tests two owned entries naming one position in a single step (`sessions.rs:505-510`; `+=`→`-=` survives).

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in this diff.

Reviewed the owned walk and terminal delete (`crates/custodian/src/retirement/sessions.rs:594`, `crates/custodian/src/retirement/sessions.rs:917`), untrusted-part retirement (`crates/custodian/src/retirement/marks.rs:548`), deployed inputs (`crates/server/src/custodian.rs:573`), and regression tests against the target source. Validation used the frozen gate logs; no tests were rerun. The surviving mutations did not establish an additional actionable defect. The brief's recorded deferrals remain settled.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept or reject the declared new-API verification exception and unavailable stacked diff coverage — 43 feature tests pass independently, but no behavioral pre-fix red or changed-line coverage was established (reviewer-green.log:191; reviewer-red.log:4; gate-logs/C4-verify.log:47; gate-logs/C4-diff-cov.log:10).
- [x] Validation — fitness-to-purpose — Decide whether the operating tradeoffs meet deployment needs — completed tombstones retain admission until #625, unnameable late fragments remain for operator repair, and retirement can require up to 78 fleet listings per pass at the shipped profile (crates/server/src/custodian.rs:580; crates/custodian/src/retirement.rs:138; crates/custodian/src/retirement.rs:158).
- [x] **The tombstone input is one switch for the whole pass, but the window it stands for is per session.** `crates/custodian/src/retirement.rs:389-396` defines `Tombstone::{Retain, Elapsed}`, and `sessions.rs:927` reads it the same way for every `Completed` session. Each session's own `Completion.completed_at_millis` (`crates/core/src/multipart.rs:2313-2324`) is never consulted. No caller can pass `Elapsed` safely while sessions complete at different times. Passing it when only the oldest session's window has passed deletes younger tombstones early, so a retried Complete gets `NoSuchUpload`. Passing it only when every window has passed means that, with steady traffic, no `Completed` session is ever deleted, `mpuctl` fills, and Create returns `503`. So #625 will have to *replace* this type, not just "supply the tombstone input this child defines" as the brief says. That is safe today, because the deployed caller hard-codes `Retain` (`crates/server/src/custodian.rs:580`). The human should decide whether to accept the type as a stub that #625 must reshape (and say so on #625), or have it carry a per-session predicate now. Measuring the window across clocks stays #625's either way (ADR-0009).
- [x] **Check that the T4 review actually ran.** `check-gates.json` reports T4-batch-review as passing and gating: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **35 s** (`gate-logs/T4-batch-review.log`). That is three Codex multi-pass reviews plus triage of a 7,020-line diff. The four earlier rounds each found 2–7 blocking items. 35 s is short enough that a failed or empty run counted as zero findings would look exactly like this. I cannot see `review-batch.md`. Before trusting this pass, confirm it contains three completed reviews of *this* patch. I found no defect it should have caught, so this is a doubt about the evidence, not about the code.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [x] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [x] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [x] size backstop — this slice is behaving oversized: patch is 350 KB (threshold 100 KB); patch touches 25 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
