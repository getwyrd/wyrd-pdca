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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 214 mutants tested in 70m: 59 missed, 133 caught, 20 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.46s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1013’s multipart residue retirement and terminal session deletion: one independently reproduced progress bug needs a builder iteration; normal-path tests pass.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable deletion, admission-count, tombstone, fleet and late-write outcomes, with the accepted iteration scope explicit (`brief.md:25`, `brief.md:100`, `brief.md:318`). |
| C2 Reproduction (red pre-fix) | N/A | This is declared new functionality; the pre-fix discriminator cannot compile against the old API and establishes no behavioral red (`brief.md:115`, `gate-logs/C4-verify.log:16`, `review-red.log:3`); acceptance of that limitation is owed under C4. |
| C3 Change | PASS | The changes stay within the accepted session-drain design and context propagation; the terminal batch couples session deletion to the admission CAS, and living docs describe the new bookmark (`crates/custodian/src/retirement/sessions.rs:799`, `docs/design/architecture/06-runtime-view.md:92`, `brief.md:318`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared absence of a behavioral pre-fix red and decide how to discharge unmeasured diff coverage on the integrated base — 102 relevant tests pass independently, but neither result supplies those missing measurements (`gate-logs/C4-verify.log:49`, `gate-logs/C4-diff-cov.log:10`, `review-restored-green.log:29`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve resumable progress for an eligible wide entry when an earlier entry is held — the first-left bookmark strands unrelated residue indefinitely; both independent probes fail (`crates/custodian/src/retirement/sessions.rs:619`, `crates/custodian/src/retirement/sessions.rs:673`, `review-probe.log:3`). |
| T1 Structure | PASS | The existing reconstruction context and metadata/chunk-store seams carry the session leg, with one clock owning its marks and no new shared mutable production global (`crates/custodian/src/reconciliation.rs:168`, `crates/custodian/src/retirement/sessions.rs:531`, `crates/custodian/src/reconstruction.rs:142`). |
| T2 Shape | PASS | Existing record encodings remain intact; the new bookmark and conservative tombstone behavior are documented, and independent fmt, spelling and docs checks pass (`crates/core/src/multipart.rs:2091`, `crates/custodian/src/retirement/sessions.rs:150`, `docs/design/architecture/06-runtime-view.md:92`, `review-docs-render.log:3`). |
| T3 Runtime | FAIL | A held head entry makes every pass spend its read budget on the same prefix of a later 256-position entry: six passes remain at 232/256 or 233/256, retaining that entry and its protected fragments (`crates/custodian/src/retirement/sessions.rs:601`, `crates/custodian/src/retirement/sessions.rs:678`, `review-probe.log:3`). |
| T4 Contribution | NEEDS-HUMAN | Confirm rejected-work history by affected path and supply the required commit-hook result — path history/searches were checked, but cannot establish the discontinued #636 patch’s disposition, and the disposable target supplies neither that hook nor its log (`brief.md:167`, `brief.md:322`, `review-prior-art.json:2`); contribution-artifact audit is separately deferred, not missing. |
| T5 Judgment | PASS | The accepted bookmark design, single-slice scope and #660 simulation deferral remain binding; the remaining progress bug is actionable within that design, without reopening settled scope (`brief.md:112`, `brief.md:318`, `AGENTS.md:215`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the operational tradeoff: unnamed late writes can remain forever, whole-fleet retirement pauses during outages, and completed sessions retain admission slots until #625 — these affect storage cost and admission capacity (`brief.md:226`, `crates/custodian/src/retirement.rs:155`, `crates/server/src/custodian.rs:578`). |

**Confirmed finding — P2: an earlier hold prevents a later wide entry from making progress.** `first_left` is filled by a held entry at `sessions.rs:619`, `:626`, or `:673`. A later entry can mark only part of its positions, but `get_or_insert` cannot retain its progress. The bookmark written at `:678` therefore names the earlier hold, or is absent when that hold covered zero positions. On the next pass, the later entry resumes at zero (`:601`) and spends its budget rereading the same marks. This is one defect, reflected in C5 and T3, not two independent findings.

The reviewer probes reuse the submitted Redb-backed harness and call only `reconcile_step` through its existing helpers. In the Aborting case, an earlier entry has an unreadable first mark; the following trusted RS(128,128) entry stays at 232/256 marked positions. In the Completed case, an earlier wrong-length entry is intentionally held even with `Tombstone::Elapsed`; the following trusted entry stays at 233/256. Both remain present with no bookmark after six passes (`review-probe.log:3`, `review-probe.log:14`). The earlier held record correctly remains; the defect is retention of the separate eligible entry. No data loss was observed.

The builder should retain progress for the entry actually advancing despite earlier holds, and add a mixed held-plus-wide regression asserting that the eligible entry disappears while the held entry stays. The submitted tests separately cover completed-session holds (`crates/custodian/tests/multipart_session_terminal.rs:1710`) and oversized entries (`:1813`), so neither exercises this interaction. To reproduce from this review directory after building the submitted test target, run `python3 review-probe-run.py`; both reviewer assertions fail. Production source was not edited. The C5 capability-probe smell-test does not trigger: the added guards enforce state, fleet and tombstone rules, with no optional-capability probe or load-time fallback.

**Verification supports the normal paths, with explicit limits.** I stashed the tracked fix in the disposable target, kept the new discriminator, ran it, restored the stash, and reran it. The pre-fix run exited 101 on missing new symbols/fields; it executed no assertions. After restoration, all 21 tests passed. The related `multipart_retirement_drain`, `multipart_retirement_marks`, and `staged_protection` suites passed 19, 26 and 36 tests respectively (`review-regression.log`). All 24 patched target files still match their patch blob hashes after restoration.

Independent `git diff --check`, `cargo fmt --all -- --check`, `typos`, docs lint and the 101-page docs render/link audit passed. Direct `gitleaks stdin --redact --no-banner --no-color < ../patch.diff` reported no leaks (`review-gitleaks.log:2`). This scanner run is not evidence that the missing local forbidden-file/commit hook ran. The brief’s named external dependencies, `typos` and `docs-renderer`, were actually exercised; neither requires an unmet-dependency exception. No `INTEGRATION.md` exists in the supplied target, so no additional §4 items could be recovered from it.

The frozen gates were adjudicated as follows; instance-root wrappers were not sought in unrelated checkouts:

- **C4-ci: PASS on captured evidence.** The log shows real fmt/clippy/build/tests, dependency checks, conformance and seeded DST, ending in success (`gate-logs/C4-ci.log:4972`). My independent runs cover the relevant tests and scanners, not the entire workspace gate.
- **C4-verify: no behavioral red established.** The independent stash/restore result agrees with the captured compile failure and subsequent 21-test green (`gate-logs/C4-verify.log:10`, `:49`). This is the brief’s declared new-API limitation, not a compiler defect in the applied patch.
- **C4-diff-cov: unmeasured.** The gate could not apply the bundle to its `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target is readable and compiles with the patch; the gate’s base mismatch is not a C4 patch-defect FAIL.
- **C5-mutants: incomplete discrimination.** The captured run reports 59 missed, 133 caught, 20 unviable and two timeouts out of 214 (`gate-logs/C5-mutants.log:74`). I did not repeat the 70-minute sweep or equate every surviving mutation with a defect; the concrete counterexample above independently establishes the actionable gap.
- **T4-batch-review: one reproduced defect; two DST requests rejected as out of scope.** Its bookmark finding reproduces. Its two seeded-coverage findings conflict with the brief’s explicit “seeded DST … add no DST scenario” exclusion and tracked #660 deferral (`gate-logs/T4-batch-review.log:10`, `brief.md:112`, `AGENTS.md:215`). They are not re-raised here.
- **T4-contribution: N/A.** The artifacts are drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This creates no human clearance item.
- **host-tikv: PASS for compilation on captured evidence.** Both requested feature compilations completed; this log does not claim live TiKV service testing (`gate-logs/host-tikv.log:2`, `:211`).

**Prior-art investigation is partial, not an assertion of novelty.** The target contains one synthetic base commit and no remote. GitHub merged-history queries by path found no `main` history for `crates/custodian/src/retirement.rs`, `retirement/marks.rs`, `retirement/sessions.rs`, or the new test file; `crates/core/src/multipart.rs` has existing multipart history. Closed-PR searches for the affected retirement path and new test filename returned no matches; a broader multipart/terminal search found related work, including [Abort #909](https://github.com/getwyrd/wyrd/pull/909) and [part compensation #1051](https://github.com/getwyrd/wyrd/pull/1051). PR text search does not exhaust closed/rejected diffs, and the brief’s discontinued #636 implementation was not supplied. Query results are retained in `review-prior-art.json`; that remaining disposition check is the T4 decision owed. No builder notes or other checkout were read.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1013 (owned `sidx:` walk, untrusted-record retirement, terminal session delete)

I re-ran the evidence in a scratch copy of the target (cargo works on this host). The 21 new tests
pass. All six reds the brief requires come back when I break the production code by hand, and seven
more breakages I tried are caught too. I could not break the safety core: mark before delete,
the whole-fleet gate, and the exactly-once `mpuctl` decrement all held. What I found: one scope gap,
one piece of production wiring no test guards, two test gaps, and a narrower reading of T4's blocking
item.

## Findings

- NEEDS-HUMAN [human] — **An aborted session with a damaged `{chunks}` obligation never ends.**
  `crates/core/src/multipart/commit.rs:1551` copies a re-uploaded part's *prior* chunks into a
  session-scoped `retire:bytes:s:<id>:<E>:<n>:<attempt>` obligation. `crates/custodian/src/retirement/marks.rs:994-996`
  holds a wrong-length chunk in that list as `untrusted-placement` for every session state, and
  `crates/custodian/src/retirement/sessions.rs:755-761` makes the terminal delete wait while any
  session-scoped obligation is left. Probe: an `Aborting` session holding such an obligation (RS(4,2)
  on 5 servers, fragments on 0–5). After 6 deployed whole-fleet passes, every pass was `Blocked`
  with `retire-obligation-held` / `untrusted-placement`, and the session record, the obligation and
  `mpuctl.count` (1) were all unchanged. This is the same damage E1 retires by chunk id when it sits
  in a `part:` record. Once a re-upload moves it into a `{chunks}` list, the session keeps its
  admission slot for good. The brief's invariant ("every session state the drain owns has an exit")
  is not met for this record kind, but its Scope names only `part:`/`sidx:`. The human decides:
  extend this slice (retire `{chunks}` lists by chunk id for `Aborting` sessions, also marking the
  positions each list names, because a compensated attempt's write may still land), or file a
  follow-up issue. It is audited, and no bytes are lost.

- NEEDS-HUMAN [impl] — **The deployed caller's two new inputs are not guarded by any test.**
  `crates/server/src/custodian.rs:577` (`operator_fleet_size`) and `:580` (`Tombstone::Retain`). I
  changed them in scratch to `operator_fleet_size: fleet.len()` and `Tombstone::Elapsed`, and every
  server test that drives the deployed loop still passed: `custodian_gc` (1), `custodian_day_one` (15)
  and `closed_write_path` (10). The new test file builds its own context
  (`crates/custodian/tests/multipart_session_terminal.rs:650-659`, `View::deployed`), so it cannot see
  the real caller. Both changes would be safety regressions. The first runs retirement by chunk id on
  a fleet that was short from startup, which strands a missing server's fragments for good. The
  second deletes `Completed` tombstones, so an identical Complete retry gets `NoSuchUpload`. So the
  brief's SELF-TEST claim that the test "binds … their dispatch in the deployed custodian" does not
  hold for these two inputs. Fix: add a server test through `run_reconstruction_until`
  (`custodian_gc.rs` already drives it) where `configured` is short from startup, with an `Aborting`
  session holding an untrusted entry (expect a `partial-fleet` hold) and a `Completed` session (expect
  its record to stay).

- NEEDS-HUMAN [impl] — **T4's blocking item at `sessions.rs:678` is real, but only applies while
  another entry is held.** Probe: an `Aborting` session with entry A (wrong-length, part 1) and entry
  B (RS(128,128), 256 positions, part 2), on a fleet missing server 5 from startup. Over 5 passes, B
  stayed at 233/256 positions marked, no bookmark was written, and every pass re-read the same 233
  marks. Once the fleet was whole, the session ended within one pass. Cause: held entries claim
  `first_left` with a count of 0 (`sessions.rs:619`, `:626`), and the bookmark filter drops a zero
  count (`:679`), so B's progress is never saved. A stored bookmark naming B would be deleted the
  same way. The stall only lasts while another entry of the same session is held, and that hold
  already blocks the terminal delete, so nothing is lost or stranded. The cost is 233 wasted reads
  per pass for each such session, and B's bytes are kept until the hold clears (an entry smaller
  than one step's budget would have been retired). Small fix: put the bookmark on the first entry
  with `covered > 0` instead of the first entry left. Resume matches by key (`:601-606`), so that is
  safe. Keep the probe as a regression test.

- NEEDS-HUMAN [impl] — **Recovering from a lost bookmark works but is untested.**
  `sessions.rs:704`: if I flip `!moved` to `moved`, all 21 shipped tests still pass (C5 also lists
  `704:39` and `910:5` `emit_unusable_bookmark` as missed). Probe: a wide entry whose first 233
  positions already carry the walk's mark, with no bookmark (I reached this state with the held-A
  pass above, then deleted A). The real code ends the session. The mutant stalls for good with no
  hold, which is the same stall class iteration 1 was sent back for. Separately, the C5 mutant that
  makes `owned_walk_key` (`sessions.rs:153-157`) return `vec![]` survives, because the test builds
  the key with that same function (`multipart_session_terminal.rs:1862`). So the stored key spelling
  `custodian:owned-walk:<id>`, which the docs name, is not pinned. Add the probe as a test and assert
  the literal key.

- NEEDS-HUMAN [human] — **T4's two DST items conflict with the brief.** T4 also blocks on missing
  seeded Tier-0 DST (deterministic simulation) coverage for the new destructive paths
  (`sessions.rs:714`, `multipart_session_terminal.rs:1067`). The brief says "seeded DST (#660 owns
  'two concurrent drainers'); add no DST scenario", and the in-code `// deferred: #660`
  (`sessions.rs:173`) covers only two drainers. This is not a builder defect. The human decides:
  either accept the brief's scope and widen #660 (or file a new issue) for single-drainer crash
  coverage, or require DST in this slice. My view: each walk step and the terminal delete is one
  atomic batch, and the next pass re-reads everything, so DST here would more likely confirm than
  find something.

- NEEDS-HUMAN [impl] — **The rebase is still not done.** `gate-logs/C4-diff-cov.log`: patch.diff does
  not apply on `origin/main`, so diff coverage is still not measured. Iteration 1's carry-forward
  asked for exactly this rebase. Rebase and re-run C4-diff-cov.

## Attempted to refute; could not

- **Red→green evidence.** C4-verify is UNVERIFIABLE, as the brief said in advance (the test uses
  API this patch adds). I rebuilt the six required reds by hand in scratch. Each turns the named leg
  red:
  1. Empty-`sidx:` gate removed: `d_the_terminal_delete_waits_for_an_empty_owned_range` fails.
  2. Tombstone check removed: the `Completed` leg fails (as do E1–E3, because the committed object's
     session is deleted too).
  3. Session-record precondition removed: the race leg fails.
  4. Listing marks skipped: E2/E3 fail.
  5. Fleet gate forced open: all four fleet controls fail.
  6. E2 named positions skipped: the late-landing leg fails ("(3, chunk 22007 idx 3) is unmarked").

  Also caught: part listing skipped (E1), obligations gate removed (10 tests), bookmark resume or
  bookmark count off by one (bookmark leg), `seggrp:` marker always deleted (adopted-group leg),
  ledger precondition dropped (ledger-moved leg).
- **Mark before delete via the bookmark.** I looked for a path where the bookmark counts a position
  that is not marked. The bookmark is written in the same batch as the marks it counts. GC leaves
  marks alone at positions the staged class protects (`crates/custodian/src/gc.rs:852-854`). Nothing
  else deletes `orphan:` keys at those positions. I found none.
- **Terminal-delete races.** The session-record and `mpuctl` preconditions sit in one batch
  (`sessions.rs:772`, `:799-801`). Nothing can add an `sidx:` entry after `Open` (intents are fenced
  on `Open`), and nothing can add a `seg:` record once the session is `Aborting` or `Completed`, so
  reading those ranges without a precondition is safe.
- **Cost claim** (`crates/custodian/src/retirement.rs:155-166`). At most (16 + 16 + 46) full-fleet
  listings per pass is correct for the shipped profile. The bound grows with `max_sessions` under
  other profiles, and the doc says so.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:673`: An earlier held entry monopolizes `first_left`, discarding a later entry's resumable progress. For example, put an unreadable mark at the first position of the first owned entry, followed by a valid RS(128,128) entry with 256 positions. The first pass marks only 232 positions of the later entry; the bookmark remains absent because the earlier entry's coverage is zero. Every subsequent pass spends its 233 reads on that same unreadable mark and those same 232 positions, so the healthy entry never finishes marking or retires while the unrelated hold persists. Preserve resumable progress for later actionable entries and add a repeated-`reconcile_step` regression combining a held first entry with a wide later entry; the existing wide-entry test at `crates/custodian/tests/multipart_session_terminal.rs:1813` has no preceding hold.

No separate reuse, simplification, or efficiency findings. Reviewed target source and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared absence of a behavioral pre-fix red and decide how to discharge unmeasured diff coverage on the integrated base — 102 relevant tests pass independently, but neither result supplies those missing measurements (`gate-logs/C4-verify.log:49`, `gate-logs/C4-diff-cov.log:10`, `review-restored-green.log:29`).
- [ ] C5 Causal adequacy — Preserve resumable progress for an eligible wide entry when an earlier entry is held — the first-left bookmark strands unrelated residue indefinitely; both independent probes fail (`crates/custodian/src/retirement/sessions.rs:619`, `crates/custodian/src/retirement/sessions.rs:673`, `review-probe.log:3`).
- [ ] T4 Contribution — Confirm rejected-work history by affected path and supply the required commit-hook result — path history/searches were checked, but cannot establish the discontinued #636 patch’s disposition, and the disposable target supplies neither that hook nor its log (`brief.md:167`, `brief.md:322`, `review-prior-art.json:2`); contribution-artifact audit is separately deferred, not missing.
- [ ] Validation — fitness-to-purpose — Accept the operational tradeoff: unnamed late writes can remain forever, whole-fleet retirement pauses during outages, and completed sessions retain admission slots until #625 — these affect storage cost and admission capacity (`brief.md:226`, `crates/custodian/src/retirement.rs:155`, `crates/server/src/custodian.rs:578`).
- [ ] **An aborted session with a damaged `{chunks}` obligation never ends.** `crates/core/src/multipart/commit.rs:1551` copies a re-uploaded part's *prior* chunks into a session-scoped `retire:bytes:s:<id>:<E>:<n>:<attempt>` obligation. `crates/custodian/src/retirement/marks.rs:994-996` holds a wrong-length chunk in that list as `untrusted-placement` for every session state, and `crates/custodian/src/retirement/sessions.rs:755-761` makes the terminal delete wait while any session-scoped obligation is left. Probe: an `Aborting` session holding such an obligation (RS(4,2) on 5 servers, fragments on 0–5). After 6 deployed whole-fleet passes, every pass was `Blocked` with `retire-obligation-held` / `untrusted-placement`, and the session record, the obligation and `mpuctl.count` (1) were all unchanged. This is the same damage E1 retires by chunk id when it sits in a `part:` record. Once a re-upload moves it into a `{chunks}` list, the session keeps its admission slot for good. The brief's invariant ("every session state the drain owns has an exit") is not met for this record kind, but its Scope names only `part:`/`sidx:`. The human decides: extend this slice (retire `{chunks}` lists by chunk id for `Aborting` sessions, also marking the positions each list names, because a compensated attempt's write may still land), or file a follow-up issue. It is audited, and no bytes are lost.
- [ ] **The deployed caller's two new inputs are not guarded by any test.** `crates/server/src/custodian.rs:577` (`operator_fleet_size`) and `:580` (`Tombstone::Retain`). I changed them in scratch to `operator_fleet_size: fleet.len()` and `Tombstone::Elapsed`, and every server test that drives the deployed loop still passed: `custodian_gc` (1), `custodian_day_one` (15) and `closed_write_path` (10). The new test file builds its own context (`crates/custodian/tests/multipart_session_terminal.rs:650-659`, `View::deployed`), so it cannot see the real caller. Both changes would be safety regressions. The first runs retirement by chunk id on a fleet that was short from startup, which strands a missing server's fragments for good. The second deletes `Completed` tombstones, so an identical Complete retry gets `NoSuchUpload`. So the brief's SELF-TEST claim that the test "binds … their dispatch in the deployed custodian" does not hold for these two inputs. Fix: add a server test through `run_reconstruction_until` (`custodian_gc.rs` already drives it) where `configured` is short from startup, with an `Aborting` session holding an untrusted entry (expect a `partial-fleet` hold) and a `Completed` session (expect its record to stay).
- [ ] **T4's blocking item at `sessions.rs:678` is real, but only applies while another entry is held.** Probe: an `Aborting` session with entry A (wrong-length, part 1) and entry B (RS(128,128), 256 positions, part 2), on a fleet missing server 5 from startup. Over 5 passes, B stayed at 233/256 positions marked, no bookmark was written, and every pass re-read the same 233 marks. Once the fleet was whole, the session ended within one pass. Cause: held entries claim `first_left` with a count of 0 (`sessions.rs:619`, `:626`), and the bookmark filter drops a zero count (`:679`), so B's progress is never saved. A stored bookmark naming B would be deleted the same way. The stall only lasts while another entry of the same session is held, and that hold already blocks the terminal delete, so nothing is lost or stranded. The cost is 233 wasted reads per pass for each such session, and B's bytes are kept until the hold clears (an entry smaller than one step's budget would have been retired). Small fix: put the bookmark on the first entry with `covered > 0` instead of the first entry left. Resume matches by key (`:601-606`), so that is safe. Keep the probe as a regression test.
- [ ] **Recovering from a lost bookmark works but is untested.** `sessions.rs:704`: if I flip `!moved` to `moved`, all 21 shipped tests still pass (C5 also lists `704:39` and `910:5` `emit_unusable_bookmark` as missed). Probe: a wide entry whose first 233 positions already carry the walk's mark, with no bookmark (I reached this state with the held-A pass above, then deleted A). The real code ends the session. The mutant stalls for good with no hold, which is the same stall class iteration 1 was sent back for. Separately, the C5 mutant that makes `owned_walk_key` (`sessions.rs:153-157`) return `vec![]` survives, because the test builds the key with that same function (`multipart_session_terminal.rs:1862`). So the stored key spelling `custodian:owned-walk:<id>`, which the docs name, is not pinned. Add the probe as a test and assert the literal key.
- [ ] **T4's two DST items conflict with the brief.** T4 also blocks on missing seeded Tier-0 DST (deterministic simulation) coverage for the new destructive paths (`sessions.rs:714`, `multipart_session_terminal.rs:1067`). The brief says "seeded DST (#660 owns 'two concurrent drainers'); add no DST scenario", and the in-code `// deferred: #660` (`sessions.rs:173`) covers only two drainers. This is not a builder defect. The human decides: either accept the brief's scope and widen #660 (or file a new issue) for single-drainer crash coverage, or require DST in this slice. My view: each walk step and the terminal delete is one atomic batch, and the next pass re-reads everything, so DST here would more likely confirm than find something.
- [ ] **The rebase is still not done.** `gate-logs/C4-diff-cov.log`: patch.diff does not apply on `origin/main`, so diff coverage is still not measured. Iteration 1's carry-forward asked for exactly this rebase. Rebase and re-run C4-diff-cov.
- [ ] `crates/custodian/src/retirement/sessions.rs:673`: An earlier held entry monopolizes `first_left`, discarding a later entry's resumable progress. For example, put an unreadable mark at the first position of the first owned entry, followed by a valid RS(128,128) entry with 256 positions. The first pass marks only 232 positions of the later entry; the bookmark remains absent because the earlier entry's coverage is zero. Every subsequent pass spends its 233 reads on that same unreadable mark and those same 232 positions, so the healthy entry never finishes marking or retires while the unrelated hold persists. Preserve resumable progress for later actionable entries and add a repeated-`reconcile_step` regression combining a held first entry with a wide later entry; the existing wide-entry test at `crates/custodian/tests/multipart_session_terminal.rs:1813` has no preceding hold.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- [ ] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [ ] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [ ] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [ ] size backstop — this slice is behaving oversized: patch is 244 KB (threshold 100 KB); patch touches 24 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep iteration 3's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); the safety core held under adversarial probing. Fix only: 1. sessions.rs:619/626/673-679 — a held entry claims first_left with count 0, so a later wide entry's progress is never bookmarked and it restarts at position 0 every pass (stalls at 232-233/256 while any earlier entry of the session is held). Bookmark the first entry that actually advanced (covered > 0); resume already matches by key (:601-606). Add a repeated-reconcile_step regression: held first entry + RS(128,128) second entry; assert the eligible entry retires while the held one stays (Aborting and Completed/Tombstone::Elapsed cases). 2. Guard the deployed caller's new inputs (crates/server/src/custodian.rs:577 operator_fleet_size, :580 Tombstone::Retain): add a server test through run_reconstruction_until where the fleet is short from startup — an Aborting session with an untrusted entry expects a partial-fleet hold, a Completed session keeps its mpu: record. Mutating either input must turn it red. 3. Test lost-bookmark recovery (sessions.rs:704 `!moved` mutant survives) and assert the literal stored key `custodian:owned-walk:<id>` rather than building it with owned_walk_key. Size: one slice — no split as long as the pure logic code stays under 150 KB (test file and one-line context ripples don't count). Diff coverage stays unmeasurable while the patch is stacked on #1012; that is not a builder defect. Add no DST scenario (#660 owns it).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
