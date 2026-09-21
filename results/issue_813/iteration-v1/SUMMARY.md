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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 78.6% — 88 of 112 instrumentable changed lines executed (below the 80% floor); 112 of 432 changed lines we
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 26 mutants tested in 4m: 14 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.97s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #813: make scrub check committed multipart fragments and keep staged repair obligations queued until reconstruction can safely handle them.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The detection/retention boundary and falsifiable A–F cases are explicit; rebuilding belongs to #814 (brief.md:20, brief.md:94). |
| C2 Reproduction (red pre-fix) | PASS | Independent base runs reproduce five scrub failures and four reconstruction failures, with the two scrub controls and orphan-drain control passing (reviewer-red-scrub.log:58; reviewer-red-reconstruction.log:41; reviewer-red-incomplete.log:21). |
| C3 Change | PASS | The patch stays within staged detection/retention, preserves the separate rebuilding deferral, and reuses the existing write-window value (target/crates/custodian/src/gc.rs:1320; target/crates/server/src/custodian.rs:132). |
| C4 Verification (red→green) | FAIL | All 38 supplied staged tests pass after restoration, but frozen diff coverage is 78.6%, below its 80% floor; this is a measured coverage failure, not a target/toolchain fault (reviewer-restored-green.log:38; reviewer-restored-green.log:51; gate-logs/C4-diff-cov.log:945). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Close the publication handoff gap — scrub can certify a missing fragment without checking either reference class, so the stated durability invariant still fails (target/crates/custodian/src/scrub.rs:104; target/crates/custodian/src/scrub.rs:144; reviewer-probes.log:16). |
| T1 Structure | PASS | The new reads retain the metadata trait boundary and reuse bounded paging and canonical decoders; GC/restore classification remains separate (target/crates/custodian/src/gc.rs:1553; target/crates/custodian/src/gc.rs:1562; target/crates/custodian/src/gc.rs:1578). |
| T2 Shape | FAIL | An empty staged placement is incorrectly accepted as legacy identity placement, fabricating server locations and a repair obligation instead of identifying untrusted metadata (target/crates/custodian/src/gc.rs:1584; reviewer-probes.log:13). |
| T3 Runtime | FAIL | An inode-store error suppresses attribution of staged corruption already observed, leaving the operator without the record that needs repair (target/crates/custodian/src/reconstruction.rs:211; target/crates/custodian/src/reconstruction.rs:217; reviewer-probes.log:9). |
| T4 Contribution | NEEDS-HUMAN | Confirm the affected-path prior-art disposition — the brief records merged/open checks and rejected #663/#637 iterations, but the supplied single-commit snapshot cannot independently establish the merged plus closed/rejected search (brief.md:144; reviewer-target-state.log:1). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add seeded Tier-0 coverage for the new staged reconstruction/publication path — its fixed Tokio hook test does not satisfy the standing concurrency rubric, and existing staged DST exercises GC (target/AGENTS.md:188; target/crates/custodian/tests/staged_protection.rs:2462; target/crates/dst/tests/custodian.rs:2813). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether detection and durable retention pending #814 meet this rollout's durability needs — the in-memory evidence proves the scoped mechanism, not operational fitness while staged rebuilding remains deferred (brief.md:121; target/crates/custodian/src/gc.rs:1320). |

Three implementation defects are independently reproduced, and the new concurrent path lacks the required seeded coverage. The ordinary A–F cases do go red→green. These judgments are advisory; the frozen deterministic gate results remain the harness's authority.

1. **FAIL — scrub misses a chunk during publication.** `target/crates/custodian/src/scrub.rs:104` reads inodes before `:144` reads parts. Publish an inode and retire its part immediately after the inode snapshot: neither reading includes the chunk. With its fragment missing, the production pass returned `Satisfied` and an empty repair queue (`reviewer-probes.log:15`). Read committed parts before inodes and add the handoff regression; the comment at `scrub.rs:139` incorrectly dismisses this race. The probe verifies that the publication hook committed. A second probe makes publication and retirement two separate commits in the same read gap; it also returns `Satisfied` without a repair (`reviewer-publication.log:9`), so the defect does not depend on combining those transactions.

2. **FAIL — scrub fabricates placement for a damaged part.** `target/crates/custodian/src/gc.rs:1584` calls `checked_fragments()`, whose empty-vector exception is for legacy committed objects (`target/crates/core/src/metadata.rs:413`). Multipart protection instead requires exactly the scheme's fragment count (`target/crates/custodian/src/gc.rs:1410`). A canonically decodable `EcScheme::None` part with `placement: []` caused scrub to invent server 0 and enqueue repair for chunk 60930 (`reviewer-probes.log:12`). Apply staged placement validation before expansion and attribute the malformed record; do not tighten the decoder's intentionally liberal contextual boundary.

3. **FAIL — later store failure erases earlier corruption attribution.** Reconstruction performs the fallible committed read at `target/crates/custodian/src/reconstruction.rs:211` before the staged audit loop at `:217`, despite the comment promising the opposite. An unreadable part followed by an injected `inode:` read error returned `Err` without naming the part (`reviewer-probes.log:8`). Emit the staged anomalies immediately after the staged read succeeds, before the next fallible read. Queue safety is preserved in this case; the confirmed defect is missing operator attribution.

4. **NEEDS-HUMAN [impl] — the concurrency regression is not seeded DST.** The new publication test is `#[tokio::test]` at `target/crates/custodian/tests/staged_protection.rs:2462`. The patch only fills the two context fields in `target/crates/dst/tests/custodian.rs`; its existing staged campaign at `:2813` exercises GC. Add the staged scrub/reconstruction invariants to a seeded campaign as required by `target/AGENTS.md:188`. Existing GC seed coverage and a fixed hook do not exercise this new path under the simulator.

The independent reruns used the supplied disposable target. `git stash` removed the tracked patch while keeping the added scrub test; the reconstruction red runs retained the patched test file with only the two unavailable context initializers removed, as the brief prescribes. Results were scrub **2 passed / 5 failed**, reconstruction **1 passed / 3 failed**, and unreadable-staged drain **0 passed / 1 failed**. After `git stash pop`, staged protection passed **31/31** and staged scrub **7/7** (`reviewer-rerun.log:1`; `reviewer-restored-green.log:38`). The three additional reviewer probes compiled and failed by assertion against the patched production code, not by build error (`reviewer-probes.log:3`, `:30`). Their retained source is `reviewer-probes.rs`, with the separate-commit publication variant in `reviewer-publication.rs`; both were moved out of the target after execution. No production fix was made. Reverse-application and whitespace checks pass (`reviewer-target-state.log:6`).

The full independent `cargo xtask ci` rerun passed spelling, documentation lint/render/link audit, repository guards, formatting, workspace clippy/build/tests, and dependency-use checking, then stopped because `cargo deny` could not acquire its advisory-database lock on the read-only home path (`reviewer-ci.log:3035`). This is a **host caveat**, not a patch failure. The frozen run shows the actual deny checks passing (`gate-logs/C4-ci.log:3057`, `:3068`, `:3071`) and the full CI gate completing (`:3661`); its remaining results are accepted only to that recorded extent. Independent conformance (5 valid / 6 invalid vectors) and the mutable-statics scanner also passed (`reviewer-scanners.log:2`, `:7`). The deployment guard has no standalone `xtask deploy-guard` command (`reviewer-scanners.log:10`); its successful integrated run is recorded at `gate-logs/C4-ci.log:3078`. Both named external dependencies, `typos` and the docs renderer, also executed successfully in the independent run (`reviewer-ci.log:2`, `:7`), so neither is an undischarged dependency.

Frozen gate evidence was read rather than inferred from the row labels:

- **C4-diff-cov: FAIL.** The log measures 88/112 instrumentable changed lines, with 320 unscored lines (`gate-logs/C4-diff-cov.log:945`). Its custodian measurement runs only the added `staged_scrub` binary (`:14`); reconstruction misses in that report do not mean the appended reconstruction tests never ran. The instance-scoped coverage wrapper is not supplied here, so this metric is adjudicated from the captured log.
- **C5-mutants: PASS within its stated scope.** The captured run reports 14 caught and 12 unviable mutants, with no survivor among 26 tested (`gate-logs/C5-mutants.log:10`). The wrapper is not supplied; this is frozen mutation evidence, not an independently repeated campaign or proof against the three reproduced cases.
- **T4-batch-review: FAIL.** The nine entries at `gate-logs/T4-batch-review.log:10` reduce to the four distinct concerns above. Three were reproduced; the seeded-coverage gap was confirmed against the tests and rubric. No recorded rejection/deferral disposes of these findings.
- **T4-contribution: N/A.** The PR description is intentionally not drafted yet; the substantive contribution audit reruns at publish (`gate-logs/T4-contribution.log:10`). No human clearance is needed for this deferred row.
- **host-tikv: PASS, independently repeated.** Both `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` and `cargo clippy -p wyrd-server --features tikv,etcd --tests` completed with exit 0 (`reviewer-scanners.log:114`, `:215`), agreeing with `gate-logs/host-tikv.log:209`. These compile/type-check the feature paths; they do not run a live TiKV service.

The prior-art investigation is limited by the supplied history, not by a suspected patch defect: affected-path `git log --all` returns only synthetic base `627cfbe`, and `git remote -v` is empty (`reviewer-target-state.log:1`). The brief documents a path-based merged/open search and rejected #663/#637 builds, but no independent closed-work search output is present. No other checkout or builder rationale was read. The target is readable and the supplied patch matches it; no stale-target caveat is needed.

The capability-probe smell test does not fire: the new staged guards classify metadata, not optional capabilities or load-time side effects. Rebuilding (#814), unavailable fleet members, and existing tracked deferrals remain outside this review's fixes. After the defects are addressed, the repository's Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns warrant follow-up observation for this durability change; they are not claimed as exercised here.

### Advisory — adversary

# Adversarial review — issue #813 (663.1, staged scrub + keep)

Method: re-ran the patch's own suite on a writable copy of `$PDCA_TARGET`
(`cargo test --offline -p wyrd-custodian`, all green), then wrote four probe tests against the
production entry points to try to break the fix. Three probes went red and one mutant survived
the whole crate suite. The scratch tree was deleted afterwards.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:144`: scrub reads the committed
  namespace before the `part:` records, so a publication landing between the two reads hides a
  lost fragment from both classes and the pass still answers `Satisfied`.** Probe (production
  `reconcile_step` + the `Meta::hook` leg H already uses): an `Completing` session, one `part:`
  record naming chunk `0x9001` on D server 2, that fragment lost, and a hook that commits the
  publication batch (inode + dirent + session→`Completed` + `delete part:`) right after the
  pass's first `inode:` read returns. Result: `outcome=Satisfied repairs=0
  hook=[Some(Committed)]` — the corrupt/absent fragment was never fetched and the pass certified
  the store. Moving line 144 above `let referenced = referenced_fragments(...)` at
  `scrub.rs:104` turns the probe green and leaves the entire `wyrd-custodian` suite green (I ran
  both). The inline justification at `scrub.rs:139-143` — "reading this class before or after
  the committed one above costs nothing here … scrub only ever adds a check, never removes a
  protection, so it carries no race to guard" — is the unwarranted claim: 0016 states the
  opposite in the lines the brief cites, "**The rule is general, not local to this build:**
  wherever one batch atomically moves a fact from one key range to another, every reader of both
  ranges reads the source first", and names publication as the third instance
  (`0016:782-800`). The sibling pass in this same patch honours it and ships leg H
  (`staged_protection.rs:2463`) to prove it; scrub gets the same handoff and no guard. (Three of
  the three T4 review passes flagged this independently — this bullet adds the executable repro
  and the one-line fix.)

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1584`: `read_staged_part` runs a part
  record's placement through `ChunkRef::checked_fragments()`, which calls an EMPTY vector valid
  and identity-fills it, so a damaged `part:` record makes scrub check fabricated locations and
  enqueue a phantom repair.** Probe: a decodable `part:` record whose chunk carries
  `"placement":[]` (the wire field is required but an empty array decodes — `multipart.rs:2414`
  says length is deliberately unchecked), with the chunk's real, intact fragment on D server 3.
  Result: `outcome=Changed repair_queued=true` — scrub asked identity server 0, got nothing, and
  called an intact chunk lost. GC's own reader over the *same record* does the opposite:
  `StagedSet::place` (`gc.rs:1410-1429`) requires `placement.len() == fragment_count()` and
  otherwise *holds* the chunk as an untrusted staged record. Identity placement is a pre-M3
  legacy affordance for `inode:` records; 0016's own Backfill row says part records "are born
  with an explicit full-length placement written by the current write path", so an empty one in
  a `part:` record can only be corruption. With the reconstruction half of this patch the
  phantom obligation is then never drained (a staged record names the chunk), so the pass
  answers `Blocked` forever. This also falsifies the surviving comment at `scrub.rs:214-222`
  ("an `Ok(None)` here can only mean genuine loss"), which still reasons only about the
  committed set. The branch is untested either way — `C4-diff-cov` MISSes `gc.rs:1599-1601` (the
  malformed arm) and `scrub.rs:149-150` (its emit).

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/staged_scrub.rs:511-514`: the "wrong EC scheme"
  leg does not prove what it says it proves.** Its doc claims "This proves the PART record's
  scheme is what scrub checks", but every part record in the file declares `EcScheme::None`
  (`chunk_ref(..., EcScheme::None, ...)` at every seed site), so the assertion cannot separate
  "the record's scheme" from a constant. I replaced `chunk.scheme` with a hardcoded
  `EcScheme::None` at `gc.rs:1595` and **all 7 tests in the new file passed, and so did the
  entire `wyrd-custodian` suite** (25 test binaries, 0 failures). The brief's leg A and 0016's
  scrub row both turn on "using the scheme recorded in the part record"; nothing in the bundle
  covers it. One discriminating case closes it: a part record declaring `ReedSolomon{k,m}` with
  matching intact fragments — green only if the record's own scheme is used.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:211`: the unreadable-staged-record
  attribution is emitted after `read_committed(...).await?`, so an `inode:` store fault throws
  away the names of the records a human has to repair.** The comment directly above the emit
  loop (`reconstruction.rs:214-216`) says "Attributed the moment the staged reading returns,
  before any later store read, exactly as GC attributes it (`gc.rs:420-425`)" — GC really does
  emit between its two reads (`gc.rs:423-425`), this code does not. Probe: one undecodable
  `part:` record, one queued obligation, `fail_reads_of(b"inode:")`. Result:
  `err=true named=false` — the pass returned `Err` and the damaged `part:` key never reached
  `wyrd.custodian.reconstruction.audit`. Fix is moving lines 217-219 into the `else` branch,
  between the two reads.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:144` + `crates/custodian/src/reconstruction.rs:823`:
  the two halves together make scrub flag a fully intact, published chunk as lost for the whole
  retirement window.** Probe: a `Completed` session whose `part:` record has not been retired yet
  and still names the pre-repair placement (server 0), plus the repointed committed map naming
  server 1 where the intact fragment actually sits. Result: `outcome=Changed
  repairs=[repair:37377]` — scrub checked the stale staged location independently of the
  committed map and enqueued. Reconstruction then finds `missing.is_empty()` and drains
  (`reconstruction.rs:823`), so every pass repeats the enqueue→drain cycle and emits a false
  `fragment missing` durability signal until the `retire:records:{parts}` drain lands. This
  patch makes the window likelier rather than rarer: the keep-obligation half holds a staged
  chunk's repair queued until publication, which is exactly when the repair runs and repoints
  it. Deciding what scrub should do when the two classes disagree about one chunk's placement
  (prefer the committed map for a chunk that has one, or skip staged entries for
  already-committed chunks) is a small change, but it is a decision the brief does not make.

- **NEEDS-HUMAN [human] — `crates/server/src/custodian.rs:505` and
  `crates/custodian/src/reconstruction.rs:111-116`: the new `clock` seam is filled from a
  different time source than the pass it belongs to, and both doc claims about it are false.**
  `run_reconstruction_until` builds its own `let clock_seam = wyrd_testkit::SystemClock;` and
  says it is "the same clock this loop's own `clock` closure already advances" — but that
  closure is a caller-supplied `FnMut() -> u64`, and `custodian_day_one.rs:1093-1097` passes a
  logical counter starting at 500. So `ctx.clock` (wall) and the pass's `now_millis` (logical)
  are two sources inside one lifecycle, which is the `#557`/`#565` class the rubric's first hard
  convention (ADR-0009, "one clock per correctness lifecycle") forbids. The same split appears
  at every fill site: `staged_protection.rs:518` pairs `SystemClock` with `NOW`, and
  `dst/tests/custodian.rs:652,706,828,1032,1107,1214,1325` pair it with literal `now_millis`
  values (200, …). The field doc also claims "every construction site in this crate's tests
  [fills it] from a `wyrd_testkit::Clock` double" — they all pass `SystemClock`, the production
  wall-clock arm, not `ManualClock`. Nothing reads the field in this slice, so no behaviour is
  wrong today; the cost lands on #814, whose whole purpose for the seam is to compare it against
  a pre-mark stamped from the pass's own time. Fixing it properly means `run_reconstruction_until`
  taking the `Clock` seam and deriving `now_millis` from it (a signature change reaching
  `cli.rs:1476-1519`) — an architectural call, not a rebuild-and-go, which is why this is tagged
  `[human]`.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:1262`: `emit_staged` fires
  once per chunk per pass and these obligations persist for the life of an upload.** The sibling
  `emit_refused` right above it is deliberately "once per **object**, not once per chunk — two
  obligations inside one segmented object are one refusal" (`reconstruction.rs:1237-1238`). A
  staged obligation is kept, by design, until publication (hours, per the brief), so every pass
  re-emits one warn plus one `reconstruction_kept_staged` increment for every staged chunk in
  the backlog — a D-server loss during a wave of open uploads reproduces the log volume the
  `emit_refused` rule exists to avoid.

## Attacked and could not refute

- **The C4-verify red→green is real.** I re-ran the green side (7/7 pass on the patched tree),
  and the frozen `gate-logs/C4-verify.log` red side shows five distinct assertion failures with
  real values (`Satisfied` vs `Changed`/`Blocked`), not compile or harness noise. The legs drive
  the production `reconcile_step`/`ScrubContext`, seed every record through the real
  `decode_session_record`/`decode_part_record`/`decode_owned_entry` and assert on
  `metadata::encode` round-trip identity, and the intact-fragment control rules out "queues
  everything". Leg B being green on base is stated in the brief, not concealed.
- **Aborted uploads do not produce phantom findings.** I expected scrub to flag part-named
  fragments while an abort's `retire:bytes:` drain reclaimed them; GC's reclaim gate consults
  `staged.protection` first (`gc.rs:653-657`), so those bytes cannot be reclaimed while the
  record that names them exists. No window.
- **`staged_committed_parts`' paging (`gc.rs:1552-1571`) is a faithful copy of
  `staged_fragments`' (`gc.rs:1474-1497`)** — same `staged_page`/`walk_staged_range` bound, same
  cursor advance, same containment of an unparsable session key. No new unbounded scan.
- **The committed half of scrub's new merge (`scrub.rs:170-181`) changes nothing measurable**:
  `ReferenceSet::placed` is a `HashSet` (`gc.rs:1097`), so moving from a per-server `Vec` push to
  a `HashMap` insert cannot drop or duplicate a `scrubbed` coverage emission.
- **Legs G–I discriminate what they claim.** I could not re-run their red (the brief's posture
  puts them in a reverted file), but leg H's hook shape genuinely separates the two read orders —
  I built the mirror-image probe for scrub with the same fixture and it is red, which is only
  possible because the hook fires between the two reads as advertised.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/scrub.rs:144`: Scrub reads committed inodes before staged parts. If publication writes the inode and retirement deletes its part after the inode snapshot but before this read, both sets omit the chunk. A corrupt or missing fragment receives no repair obligation and the pass can return `Satisfied`. Read parts before inodes, preserving audit attribution before subsequent fallible reads, and add a publication-race regression.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1584`: `checked_fragments()` accepts an empty placement and invents identity locations (`crates/core/src/metadata.rs:427`). Multipart parts have no legacy-placement exemption; their decoder intentionally accepts contextual length errors for maintenance to classify. An empty staged placement therefore makes scrub check invented servers, enqueue phantom repairs, or silently skip verification when those servers are outside the fleet. Apply the exact-length rule already used by `StagedSet::place` (`crates/custodian/src/gc.rs:1412`), preferably sharing that classification, and test an empty placement.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:211`: `read_committed(...).await?` runs before the staged-corruption audit loop at line 217. With an unreadable part already discovered and a subsequent inode-store fault, reconstruction returns without naming the damaged staged record. Emit staged faults immediately after `staged_fragments` returns, as GC already does, and test that attribution survives the later read error.

- NEEDS-HUMAN [impl] — `crates/custodian/tests/staged_protection.rs:2463`: The new reconstruction/publication race is exercised only by a fixed Tokio hook. The diff's DST changes only initialize context fields; the existing staged-handoff simulation runs GC (`crates/dst/tests/custodian.rs:2813`), so it cannot detect reconstruction losing an obligation. The standing rubric requires seeded Tier-0 coverage for this concurrent path. Extend the handoff campaign to drive reconstruction across part commit, publication, and separate part retirement, asserting that a missing fragment's obligation survives.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Close the publication handoff gap — scrub can certify a missing fragment without checking either reference class, so the stated durability invariant still fails (target/crates/custodian/src/scrub.rs:104; target/crates/custodian/src/scrub.rs:144; reviewer-probes.log:16).
- [ ] T4 Contribution — Confirm the affected-path prior-art disposition — the brief records merged/open checks and rejected #663/#637 iterations, but the supplied single-commit snapshot cannot independently establish the merged plus closed/rejected search (brief.md:144; reviewer-target-state.log:1).
- [ ] T5 Judgment — Add seeded Tier-0 coverage for the new staged reconstruction/publication path — its fixed Tokio hook test does not satisfy the standing concurrency rubric, and existing staged DST exercises GC (target/AGENTS.md:188; target/crates/custodian/tests/staged_protection.rs:2462; target/crates/dst/tests/custodian.rs:2813).
- [ ] Validation — fitness-to-purpose — Decide whether detection and durable retention pending #814 meet this rollout's durability needs — the in-memory evidence proves the scoped mechanism, not operational fitness while staged rebuilding remains deferred (brief.md:121; target/crates/custodian/src/gc.rs:1320).
- [ ] `crates/custodian/src/scrub.rs:144`: Scrub reads committed inodes before staged parts. If publication writes the inode and retirement deletes its part after the inode snapshot but before this read, both sets omit the chunk. A corrupt or missing fragment receives no repair obligation and the pass can return `Satisfied`. Read parts before inodes, preserving audit attribution before subsequent fallible reads, and add a publication-race regression.
- [ ] `crates/custodian/src/gc.rs:1584`: `checked_fragments()` accepts an empty placement and invents identity locations (`crates/core/src/metadata.rs:427`). Multipart parts have no legacy-placement exemption; their decoder intentionally accepts contextual length errors for maintenance to classify. An empty staged placement therefore makes scrub check invented servers, enqueue phantom repairs, or silently skip verification when those servers are outside the fleet. Apply the exact-length rule already used by `StagedSet::place` (`crates/custodian/src/gc.rs:1412`), preferably sharing that classification, and test an empty placement.
- [ ] `crates/custodian/src/reconstruction.rs:211`: `read_committed(...).await?` runs before the staged-corruption audit loop at line 217. With an unreadable part already discovered and a subsequent inode-store fault, reconstruction returns without naming the damaged staged record. Emit staged faults immediately after `staged_fragments` returns, as GC already does, and test that attribution survives the later read error.
- [ ] `crates/custodian/tests/staged_protection.rs:2463`: The new reconstruction/publication race is exercised only by a fixed Tokio hook. The diff's DST changes only initialize context fields; the existing staged-handoff simulation runs GC (`crates/dst/tests/custodian.rs:2813`), so it cannot detect reconstruction losing an obligation. The standing rubric requires seeded Tier-0 coverage for this concurrent path. Extend the handoff campaign to drive reconstruction across part commit, publication, and separate part retirement, asserting that a missing fragment's obligation survives.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 123 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human overrode the size backstop's iterate-plan recommendation (123 KB vs 100 KB threshold) and chose iterate-do instead, judging the four findings to be targeted fixes a rebuild can land without re-splitting the slice. Fix on the next attempt: - crates/custodian/src/scrub.rs:144 — read committed `part:` records before `inode:` reads (currently backwards); a publication landing in the gap hides a lost fragment from both classes and the pass wrongly answers Satisfied. Add a publication-race regression test. - crates/custodian/src/gc.rs:1584 — `checked_fragments()` treats an empty/damaged staged placement as valid and invents identity server locations, letting a corrupted part record produce fabricated repair targets. Apply the exact-length validation `StagedSet::place` already uses (gc.rs:1412) and test an empty placement. - crates/custodian/src/reconstruction.rs:211 — `read_committed(...).await?` runs before the staged-corruption audit emit at line 217, so a later inode-store fault throws away attribution of an already-discovered unreadable staged record. Emit staged faults immediately after the staged read succeeds, before any further fallible read. - crates/custodian/tests/staged_protection.rs:2463 — the new reconstruction/publication race is only exercised by a fixed Tokio hook test, not seeded Tier-0/DST coverage, per the standing concurrency rubric (AGENTS.md:188). Extend the seeded staged-handoff campaign (dst/tests/custodian.rs:2813 currently exercises GC only) to drive reconstruction across part commit, publication, and retirement, asserting the obligation survives. Also carry forward two items needing a human decision, not a code fix, on the next round: confirm the prior-art/merged-PR search claim (brief.md:144) and decide fitness-to-purpose given #814 (rebuild) is still deferred (brief.md:121).
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
