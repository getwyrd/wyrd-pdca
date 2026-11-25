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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 221 mutants tested in 73m: 51 missed, 148 caught, 20 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.87s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1013’s multipart residue retirement and terminal session deletion: one confirmed pagination defect still strands otherwise eligible residue.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The exit, exactly-once admission release, tombstone retention, fleet completeness and late-write residual have explicit acceptance criteria; proposal 0016’s terminal rule grounds them (`brief.md:25`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:582`). |
| C2 Reproduction (red pre-fix) | N/A | This is declared new functionality; the pre-fix discriminator cannot compile against the old API, so its compiler errors are not a behavioral reproduction (`brief.md:115`; `review-red-green.log:7`). |
| C3 Change | PASS | The changes stay within the accepted session-drain slice and its context/test/doc updates; the existing retirement dispatch is retained, with production tombstones protected (`crates/custodian/src/retirement.rs:523`; `crates/server/src/custodian.rs:577`; `brief.md:334`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared absence of an executable pre-fix behavioral red and the frozen dependency-audit evidence — 84 focused tests pass independently, but reverting removes required APIs and this disposable copy lacks the RustSec gitlink needed by full CI (`review-red-green.log:7`; `review-ci.log:3676`; `gate-logs/C4-ci.log:4979`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Restore progress beyond a full held page — both independent probes leave the trusted 17th entry present and unmarked after 20 passes because every walk restarts at the first 16 entries (`crates/custodian/src/retirement/sessions.rs:530`; `review-probes.log:8`; `review-probes.log:14`). |
| T1 Structure | PASS | Maintenance remains behind the existing metadata/chunk-store traits and reconstruction dispatch; no new concrete-backend dependency or correctness clock is introduced (`crates/custodian/src/retirement.rs:396`; `crates/custodian/src/retirement/sessions.rs:537`; `crates/custodian/src/reconciliation.rs:162`). |
| T2 Shape | PASS | The accepted single-slice size holds: approximately 38.6 KB of added non-comment production code; the persisted bookmark and runtime behavior are documented in the living architecture (`brief.md:334`; `docs/design/architecture/06-runtime-view.md:90`). |
| T3 Runtime | PASS | The fleet gate, tombstone input and batch bounds survive the focused runtime checks; the documented cost includes held sessions and does not claim GC-listing reuse (progress defect recorded separately in C5) (`crates/custodian/src/retirement.rs:155`; `crates/server/tests/custodian_gc.rs:1469`; `review-server.log:188`). |
| T4 Contribution | FAIL | The frozen batch review remains substantively red on one reproduced pagination defect; its three duplicate reports collapse to C5, and its DST objection is settled by #660; contribution-artifact checking is N/A until publish (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`; `brief.md:334`). |
| T5 Judgment | NEEDS-HUMAN | Confirm semantic supersession of the discarded #636 work — merged-history and closed/rejected searches ran by all 25 affected paths, but bounded history/search results and the old issue description cannot establish equivalence to the unavailable discarded patch (`review-prior-art.json:2`; `review-prior-art-rejected.json:2`; `brief.md:167`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the operational tradeoffs after the pagination defect is fixed — completed sessions retain admission slots until #625, unlisted late writes can remain indefinitely, and the shipped profile permits up to 78 fleet listings per pass (`crates/server/src/custodian.rs:578`; `crates/custodian/src/retirement.rs:135`; `crates/custodian/src/retirement.rs:155`). |

All repository `path:line` citations above and below were read in `$PDCA_TARGET`, the disposable patched copy. Evidence-log paths refer to this review directory. The target source is usable: the patch’s reverse-apply check succeeds, and the stash/reapply exercise preserved the original target status. The synthetic Git index’s missing submodule is a separate target-state caveat.

**One implementation finding: [P2] a full page of held entries hides later eligible residue.** At `crates/custodian/src/retirement/sessions.rs:530`, `scan_page` always receives `after = None`, and its continuation is discarded. The bookmark only reorders entries already in that page (`:565`). If all 16 entries are held, the visit stops without reaching any later entry (`:735`). Subsequent passes repeat the same page. The earlier fix for one held entry ahead of a wide entry therefore does not cover a whole held page; the existing tests exercise the former (`crates/custodian/tests/multipart_session_terminal.rs:2258`, `:2273`).

Two executable probes independently reproduced this through `reconcile_step`, using the supplied real-redb fixture in a separate scratch test package:

- `Aborting`: 16 wrong-length entries are held on a five-of-six-server fleet; a trusted 17th entry on reachable servers remains present with zero orphan marks after 20 passes (`review-probes.log:8`).
- `Completed`, with `Tombstone::Elapsed`: the same 16 untrusted entries are held even on the whole fleet; the trusted 17th entry likewise remains present and unmarked after 20 passes (`review-probes.log:14`).

The impact is unnecessary permanent retention of eligible fragments while earlier entries remain held. The session correctly stays because held residue exists; this finding does not ask to delete it early. Carry bounded scan progress across held pages while preserving intra-entry mark progress, and add both page-boundary regressions. Reproduce the two failures from this directory with:

```sh
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1013-probes/Cargo.toml \
  --test terminal_review reviewer_ -- --nocapture
```

The probe package copies the supplied test fixture and appends the two tests at `pdca-reviewer-1013-probes/terminal_review.rs:2469` and `:2496`; it changes no target source.

**Verification supports the exercised safety properties, with explicit limits.** Independent focused runs passed 27 terminal-session tests, 19 records-drain tests, 26 marks-drain tests, 11 server-loop tests and the admission-release unit test (`review-green.log`, `review-retirement.log:26`, `review-retirement.log:58`, `review-server.log:197`, `review-core.log`). The server-loop test exercises both new production inputs through `run_reconstruction_until`, including startup-partial fleet recovery and completed-tombstone retention. Both new reviewer probes fail on assertions, not compilation.

Stashing the tracked production changes while retaining the new discriminator produced the same four missing-API compiler errors as the frozen gate; after restoring the patch, all 27 discriminator tests passed (`review-red-green.log:7`, `:111`; `gate-logs/C4-verify.log:15`). This is the declared C4 sign-off exception, not a demonstrated behavioral red. The frozen mutation output reports 148 caught, 51 missed, 20 unviable and two timeouts; it does not identify every caught mutant, so it cannot corroborate all six particular hand-mutation witnesses requested by the brief (`gate-logs/C5-mutants.log:66`; `brief.md:126`). Surviving mutations were inspected against the source; their count alone is not a list of defects. The 73-minute sweep was not repeated.

The independent formatting, spelling, documentation lint, 101-page documentation render/link audit, whitespace check and redacted patch secret scan all passed (`review-scanners.log:1`). Both named external dependencies, `typos` and the docs renderer, were actually exercised. The secret scan is a direct scan of `patch.diff`; the historical project commit hook is not present in this target copy, so this does not assert that the hook itself ran.

**Frozen-gate adjudication preserves environment caveats and settled deferrals.**

- C4-ci: its frozen log records the full successful gate (`gate-logs/C4-ci.log:4979`). The independent full-CI run finished with two failures, both caused by the absent `.github/advisory-db` Git-index entry: the index test fails and the pinned dependency audit cannot run (`review-ci.log:3676`, `:3694`). Both `git ls-files --stage` and `git ls-tree HEAD` return no entry; `patch.diff` does not touch it. This is a disposable-copy limitation, not a patch defect.
- C4-verify: UNVERIFIABLE, independently reproduced as described above; no assertion-level pre-fix red was established.
- C4-diff-cov: unmeasured against `origin/main`, consistent with the explicitly accepted stacked base (`gate-logs/C4-diff-cov.log:10`; `brief.md:334`). This is neither measured coverage nor a compilation defect in the supplied patched target.
- C5-mutants: advisory failure with the exact outcomes above; no blanket correctness claim follows from either the caught or missed count.
- T4-batch-review: its three pagination reports describe the same independently reproduced defect. Its missing-DST report is rejected for this review because the current brief explicitly assigns that work to #660 and says to add no DST scenario (`brief.md:334`; `crates/custodian/src/retirement/sessions.rs:178`).
- T4-contribution: **N/A**. The artifacts do not exist at Check by design; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: the frozen log records successful real feature compilations, including `tikv-client`, the metadata crate and server selection arms; this was not a code-read or shim assertion (`gate-logs/host-tikv.log:2`, `:205`, `:211`). The feature build was not independently repeated.

**Prior-art checking ran by affected path, with a semantic limit.** `review-prior-art.json` records the latest five merged-history commits and closed-PR text-search results for each of the 25 affected paths. The retirement modules and new terminal test return no main-branch history or closed-PR path matches, consistent with the stacked prerequisite. A separate closed-unmerged multipart search found PR #647, and its actual changed-file list was intersected with this patch’s surface; issue #636’s closed description also records the abandoned protocol and past convergence failures (`review-prior-art-rejected.json`). These searches do not recover the discarded #636 implementation or exhaust older unmerged diffs, so T5 retains the semantic prior-art decision. No scope expansion, capability-probe workaround, DCO finding, or re-raised #625/#660 deferral is requested.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #1013 (owned `sidx:` walk, retirement by chunk id, terminal session delete)

Bottom line: I could not break the safety core. The terminal delete fires once, marks always land before deletes, and the whole-fleet gate and tombstone input are all real and tested. I did find two concrete gaps by running probe tests. One is a liveness bug the builder can fix. The other is a cleanup gap that needs a scope decision. Both probes ran on a scratch copy of `$PDCA_TARGET` (`cargo test -p wyrd-custodian --test multipart_session_terminal`, cargo 1.96).

## Findings

- NEEDS-HUMAN [impl] — **16 held entries at the head of a session's `sidx:` range hide every later entry for good.** `crates/custodian/src/retirement/sessions.rs:530` always reads the *first* `OWNED_PER_STEP` (16) entries, with no `after`. When all 16 are held, the step commits nothing and returns `Walked::Stop` (`sessions.rs:735-742`), so the walk never reaches entry 17. Concrete case (probe, reproduced): an `Aborting` session has 16 owned entries under part 1, each with a `not-a-mark` value at its position 0 (an `unreadable-mark` hold). A trusted RS(2,1) entry under part 2 has its fragments landed. After 8 deployed passes the part-2 entry is still there and none of its positions is marked (`left=true marked=false`). With 15 held entries the same entry is marked and deleted (`left=false marked=true`). No audit event names the starved entries, so the operator only sees the 16 holds. The same thing happens to a `Completed` session that has 16 or more untrusted entries (held by design for `Completed`): its trusted residue behind them is never walked, even after #625 supplies `Tombstone::Elapsed`. This contradicts the module doc's claim at `sessions.rs:44-46` ("a held entry before it in the range never stalls it"). That claim only holds inside the first page. This is the same defect T4 reported three times as blocking (`sessions.rs:530/548`). My probe shows it is real, not noise. Fix: when a page ends with budget left and every entry held, read the next page from the last held key. Add a regression test with 16 held entries plus 1 drainable one through repeated `reconcile_step` calls.

- NEEDS-HUMAN [human] — **The owned walk's marks over positions that never got a fragment are never deleted.** The walk marks every position an entry names, including positions whose write never landed. GC's sweep keeps any structured, non-`reclaiming` mark with no retire token as `event-may-await-write` (`crates/custodian/src/gc.rs:924-928`). The walk's `<upload-id>:<epoch>` event is not a token. The module doc states this outright (`sessions.rs:73-75`). Concrete case (probe, reproduced): an `Aborting` session holds one RS(2,1) entry with only fragment 0 landed. After the session ends and GC runs 6 passes, each far past both the grace window and `D`, `orphan:1:30000:1` and `orphan:2:30000:2` are still in the store. Nothing names them and nothing will ever delete them. Proposal 0016 says "every mark this protocol writes has a deleter whether or not its position is ever written" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1408`). Every aborted upload with a crashed in-flight write now adds permanent `orphan:` keys, and GC pages through that namespace on every pass. The staged re-place's pre-marks have the same gap, but that one is tracked (`deferred: #825`, `gc.rs:274-275`). This new writer has no tracking issue. The human should decide: accept it and track it (extend #825 or open a new issue), or require the walk to settle its marks the way `gc.rs:270-273` describes. Not a builder iteration item.

## The evidence: re-run, not taken on trust

- C4-verify is UNVERIFIABLE only because the new test names new API (`Tombstone`, `LISTED_PER_CHUNK`, two new context fields; `gate-logs/C4-verify.log`). To fill that gap I broke the production code one way at a time in the scratch copy. Each break turned the new tests red, and the unbroken code is green (27/27):
  - Terminal delete allowed while the walk holds an entry (`sessions.rs:292`): 3 tests fail, including `d_the_terminal_delete_waits_for_an_empty_owned_range`.
  - Tombstone check disabled (`sessions.rs:780`): `d_a_completed_session_keeps_its_tombstone_until_the_window_passes` fails, plus 4 more.
  - Session-record precondition dropped from the terminal batch (`sessions.rs:803`): `d_two_drainers_racing_the_terminal_delete_release_the_slot_once` fails. This test really isolates the session precondition. The rival lowers the ledger *before* the first drainer reads it, so the ledger precondition still matches and only the session precondition stops a second decrement.
  - `mpuctl` precondition dropped (`sessions.rs:831`): `d_a_terminal_delete_whose_ledger_moved_is_judged_again_in_the_same_pass` fails.
  - Listed fragments not marked (`sessions.rs:672`): 8 tests fail, including e2/e3.
  - Whole-fleet gate forced open (`retirement.rs:413`): 6 tests fail, including all four short-fleet controls.
  - E2's named positions not marked (`sessions.rs:359`): `e2_…` (the late-landing leg) fails, plus 2 more.
  - Deployed caller passes `Tombstone::Elapsed` (`crates/server/src/custodian.rs:580`), or `fleet.len()` in place of `operator_fleet_size` (`:577`): `custodian_gc::deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones` fails for each. The unbroken run is green.
- Every red the brief requires reproduces. The tests run through `reconcile_step` over a real `RedbMetadataStore` (in memory), not a re-implementation.

## Things I tried that did not break it

- **Bookmark trusts marks it doesn't re-read** (`sessions.rs:622-627`). This is safe. GC's staged class protects every owned entry's positions whatever the session state (`gc.rs:1376-1386`, built from every `mpu:` session), so those marks can't be reclaimed or swept while the entry stands.
- **Exactly-once decrement under an ABA ledger** (release, then a Create, gives the same bytes back). The session-record precondition still refuses the second batch (the M3 red above).
- **`seggrp:` marker deleted while a `seg:` record appears.** No writer creates `seg:` records for a nonce once its session has left `Open`/`Completing`, and the batch requires the session bytes.
- **Step-counter underflow.** `visit` checks `steps_left` before each walk step (`sessions.rs:284`), and `settle` runs only after a walk step that consumed nothing.
- **Unbounded await on the new fleet listing** (rubric *await discipline*). Deployed D-server clients use `GrpcChunkStore::connect_with_timeout` (`crates/server/src/cli.rs:1748`), so a listing can't hang the pass.
- **Whole-fleet gate weaker than GC's.** It is the same `fleet.len() == operator_fleet_size` test as the deployed GC gate (`crates/server/src/custodian.rs:688`).

## Notes on the verdict (not refutations)

- T4's fourth blocking item (no seeded Tier-0 DST coverage) is settled by the brief's scope ("add no DST scenario") and the `// deferred: #660` marker at `sessions.rs:178`. Under the rubric's deferral rule I am not re-raising it. T4's other three blocking items are the starvation finding above, so T4's red should not be waved off as reviewer noise.
- Most of C5's 51 surviving mutants are byte-budget arithmetic (`sessions.rs:437-438`, `:498-503`, `:587-596`). The read budget always runs out before the byte budget in any reachable case (`MAX_BATCH_BYTES = E_TX/2`), so these are equivalent in practice. One branch is never exercised: two entries in one step naming the same position (`sessions.rs:430-435`). It is reachable only under damage (two `sidx:` keys for one chunk). Low priority, no NEEDS-HUMAN.
- C4-diff-cov is red because the patch is stacked on #1062 and does not apply to `origin/main`. The iteration-3 sign-off already ruled that this is not a builder defect.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:530`: The owned walk always requests the first 16 entries and discards the continuation. If those entries are held (for example, a Completed session's untrusted entries), every later, independently drainable entry is permanently invisible. The within-entry bookmark only reorders that same page. Preserve progress across owned-entry pages, with wraparound for held entries, and add a repeated-`reconcile_step` regression containing 16 held entries followed by a trusted entry.

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:697`: Bookmark selection requires `covered > 0`, but `covered` counts only named positions, excluding the fleet-listed positions walked at line 680. An undecodable entry has no named positions. In an Aborting session on a whole fleet, put two entries whose first marks are unreadable before an undecodable entry with 232 listed fragments (`LISTED_PER_CHUNK`). The earlier holds consume two of the 233 reads, so only 231 listed fragments are examined each pass. The undecodable entry never gets a bookmark, its last fragment never gets marked, and it never retires. Preserve scheduling priority for a budget-limited entry even when its named-position count is zero, without treating the listing offset as durable evidence; add this three-entry regression.

No separate reuse, simplification, or efficiency findings. Reviewed target source and frozen gate evidence; no builds were rerun. The explicitly deferred DST coverage is not raised again.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared absence of an executable pre-fix behavioral red and the frozen dependency-audit evidence — 84 focused tests pass independently, but reverting removes required APIs and this disposable copy lacks the RustSec gitlink needed by full CI (`review-red-green.log:7`; `review-ci.log:3676`; `gate-logs/C4-ci.log:4979`).
- [ ] C5 Causal adequacy — Restore progress beyond a full held page — both independent probes leave the trusted 17th entry present and unmarked after 20 passes because every walk restarts at the first 16 entries (`crates/custodian/src/retirement/sessions.rs:530`; `review-probes.log:8`; `review-probes.log:14`).
- [ ] T5 Judgment — Confirm semantic supersession of the discarded #636 work — merged-history and closed/rejected searches ran by all 25 affected paths, but bounded history/search results and the old issue description cannot establish equivalence to the unavailable discarded patch (`review-prior-art.json:2`; `review-prior-art-rejected.json:2`; `brief.md:167`).
- [ ] Validation — fitness-to-purpose — Approve the operational tradeoffs after the pagination defect is fixed — completed sessions retain admission slots until #625, unlisted late writes can remain indefinitely, and the shipped profile permits up to 78 fleet listings per pass (`crates/server/src/custodian.rs:578`; `crates/custodian/src/retirement.rs:135`; `crates/custodian/src/retirement.rs:155`).
- [ ] **16 held entries at the head of a session's `sidx:` range hide every later entry for good.** `crates/custodian/src/retirement/sessions.rs:530` always reads the *first* `OWNED_PER_STEP` (16) entries, with no `after`. When all 16 are held, the step commits nothing and returns `Walked::Stop` (`sessions.rs:735-742`), so the walk never reaches entry 17. Concrete case (probe, reproduced): an `Aborting` session has 16 owned entries under part 1, each with a `not-a-mark` value at its position 0 (an `unreadable-mark` hold). A trusted RS(2,1) entry under part 2 has its fragments landed. After 8 deployed passes the part-2 entry is still there and none of its positions is marked (`left=true marked=false`). With 15 held entries the same entry is marked and deleted (`left=false marked=true`). No audit event names the starved entries, so the operator only sees the 16 holds. The same thing happens to a `Completed` session that has 16 or more untrusted entries (held by design for `Completed`): its trusted residue behind them is never walked, even after #625 supplies `Tombstone::Elapsed`. This contradicts the module doc's claim at `sessions.rs:44-46` ("a held entry before it in the range never stalls it"). That claim only holds inside the first page. This is the same defect T4 reported three times as blocking (`sessions.rs:530/548`). My probe shows it is real, not noise. Fix: when a page ends with budget left and every entry held, read the next page from the last held key. Add a regression test with 16 held entries plus 1 drainable one through repeated `reconcile_step` calls.
- [ ] **The owned walk's marks over positions that never got a fragment are never deleted.** The walk marks every position an entry names, including positions whose write never landed. GC's sweep keeps any structured, non-`reclaiming` mark with no retire token as `event-may-await-write` (`crates/custodian/src/gc.rs:924-928`). The walk's `<upload-id>:<epoch>` event is not a token. The module doc states this outright (`sessions.rs:73-75`). Concrete case (probe, reproduced): an `Aborting` session holds one RS(2,1) entry with only fragment 0 landed. After the session ends and GC runs 6 passes, each far past both the grace window and `D`, `orphan:1:30000:1` and `orphan:2:30000:2` are still in the store. Nothing names them and nothing will ever delete them. Proposal 0016 says "every mark this protocol writes has a deleter whether or not its position is ever written" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1408`). Every aborted upload with a crashed in-flight write now adds permanent `orphan:` keys, and GC pages through that namespace on every pass. The staged re-place's pre-marks have the same gap, but that one is tracked (`deferred: #825`, `gc.rs:274-275`). This new writer has no tracking issue. The human should decide: accept it and track it (extend #825 or open a new issue), or require the walk to settle its marks the way `gc.rs:270-273` describes. Not a builder iteration item.
- [ ] `crates/custodian/src/retirement/sessions.rs:530`: The owned walk always requests the first 16 entries and discards the continuation. If those entries are held (for example, a Completed session's untrusted entries), every later, independently drainable entry is permanently invisible. The within-entry bookmark only reorders that same page. Preserve progress across owned-entry pages, with wraparound for held entries, and add a repeated-`reconcile_step` regression containing 16 held entries followed by a trusted entry.
- [ ] `crates/custodian/src/retirement/sessions.rs:697`: Bookmark selection requires `covered > 0`, but `covered` counts only named positions, excluding the fleet-listed positions walked at line 680. An undecodable entry has no named positions. In an Aborting session on a whole fleet, put two entries whose first marks are unreadable before an undecodable entry with 232 listed fragments (`LISTED_PER_CHUNK`). The earlier holds consume two of the 233 reads, so only 231 listed fragments are examined each pass. The undecodable entry never gets a bookmark, its last fragment never gets marked, and it never retires. Preserve scheduling priority for a budget-limited entry even when its named-position count is zero, without treating the listing offset as durable evidence; add this three-entry regression.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- [ ] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [ ] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [ ] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [ ] size backstop — this slice is behaving oversized: patch is 281 KB (threshold 100 KB); patch touches 25 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep iteration 4's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under 10 hand-mutations. Fix only: 1. sessions.rs:530 — the owned walk always calls scan_page with after = None and discards the continuation, so 16 held entries at the head of a session's sidx: range hide every later entry for good (reproduced by reviewer and adversary: trusted 17th entry left unmarked after 20 passes, Aborting on a short fleet and Completed with Tombstone::Elapsed). Carry bounded scan progress past a page where every entry is held (resume from the last held key, wrap to the start when the range ends), keeping intra-entry mark progress and mark-before-delete. Fix the module doc claim at sessions.rs:44-46 to match. Add repeated-reconcile_step regressions: 16 held entries + 1 trusted entry, for Aborting and for Completed/Tombstone::Elapsed; assert the trusted entry is marked and deleted while the held ones stay. 2. sessions.rs:697 — bookmark selection requires covered > 0, but covered counts only named positions, not the fleet-listed positions walked at :680, so an undecodable entry never gets a bookmark. Two held entries ahead of an undecodable entry with LISTED_PER_CHUNK (232) listed fragments leave it one fragment short every pass. Count listed-fragment progress for scheduling (without treating the listing offset as durable evidence) and add that three-entry regression. Out of scope for the builder: the owned walk's never-deleted marks over positions that never landed (adversary finding; cf. #825) — a tracking decision for the human, not a builder item. Size: still one slice under the iteration-3 rule (logic code ~38.6 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
