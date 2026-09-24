# custodian: reconstruction rebuilds a staged chunk under the session fence (663.2)

> Child 2 of 2 of #663's split (637.3). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `feb1e30` (verified 2026-09-21), which already holds
> #813 (PR #824). 0016 = `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
> (unchanged since the first brief). This is the second brief for this id: read "Carry-forward
> from v1" at the end before building.

- **Slug:** staged-replace
- **Kind:** enhancement
- **Defect:** reconstruction recognises a committed part's chunk but can only keep its obligation
  (`crates/custodian/src/reconstruction.rs:736-739`, `Assessment::Staged`). Nothing rebuilds the
  fragment, so the part stays a fragment short until it is published, or forever if the client
  never completes. 0016 requires the rebuild (`0016:825`), and its failure table names "scrub
  staged fragments but leave reconstruction committed-only" as a wrong implementation (`:889`).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_repair.rs` passes, one
  seeded case appended to the existing `crates/dst/tests/custodian.rs` passes, and
  `cargo xtask ci` is green. The new test names only symbols on `main`, including
  `ReconstructionContext::{clock, staged_write_window_millis}` (`reconstruction.rs:123`, `:131`),
  with time from a `wyrd_testkit::ManualClock` (`crates/testkit/src/lib.rs:49`). The D-server
  doubles — this file's and the DST's — **enforce** the deadline `put_fragment` carries, refusing
  at or after it through `WriteDeadlineExpired::if_elapsed` (`crates/traits/src/lib.rs:925`), as
  the real D server does since #638. Earlier doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`). Legs:
  **(A) The whole rebuild.** An `Open` session's committed part has one fragment lost and its
  obligation queued. One `reconcile_step` with a `ReconstructionContext` answers `Changed`, and:
  a new D server holds the lost fragment, and its payload (`wyrd_core::repair::intact_shard`,
  `crates/core/src/repair.rs:124`) is byte-for-byte the shard the fixture encoded for that index
  with `wyrd_core::erasure::encode` (`crates/core/src/erasure.rs:90`); the `part:` record's
  `ChunkRef.placement` names that server; the destination's pre-mark `orphan:<P_new>` is gone;
  the vacated `P_old` carries an `orphan:` mark; the obligation has drained. A changed placement
  alone is not enough, and neither is a header check: `header_matches_identity` (`repair.rs:58`)
  and `fragment_intact` (`:106`) both accept a well-formed fragment with the wrong payload. Every
  fragment rebuilt in C(iii) and C(vii) is held to the same byte check.
  **(B) Losing branches strand nothing (X29, `0016:888`).** A `put_fragment` hook moves the
  session from `Open@E` to `Aborting@E+1` after the destination write and before the adoption
  CAS. Then: no adoption, the `part:` record byte-identical, the pre-mark still there, the
  obligation queued. Repeat with the `part:` record rewritten instead of the session fenced.
  **(C) The pre-mark and deadline rules (`0016:1285-1358`), one case each:**
  (i) the pre-mark is durable before the destination write: the double checks `orphan:<P_new>`
  is present when `put_fragment` arrives;
  (ii) a destination position that already carries a mark from another event, or a legacy mark,
  is re-stamped fresh, never reused with its old stamp;
  (iii) a position with a `reclaiming` mark is never written, and ruling it out removes only that
  position, not its server: RS(2,2) with two lost fragments, two free domains and a stale
  `reclaiming` mark on one candidate position repairs both in ONE pass;
  (iv) the write deadline is the time the context clock reads when the pre-mark commits, plus
  `staged_write_window_millis` — never the pass-start time. With the clock moved on between pass
  start and pre-mark, the deadline is still live. A write the double refuses as expired aborts
  the re-place: no adoption, pre-mark still there, obligation queued;
  (v) no destination write is authorized on a pre-mark older than `W_repoint`
  (`0016:1339-1349`, `crates/custodian/src/gc.rs:218`). A hook moves the clock past it between
  pre-mark and write, and no write on the stale pre-mark is ever adopted. Enforcing this through
  the deadline the D server checks is accepted (sign-off, #663 v1);
  (vi) a vacated `P_old` whose existing `orphan:` value decodes as none of the three shapes aborts
  the move before its CAS, keeps the obligation queued and names the fault; it is never
  overwritten (ADR-0045);
  (vii) **a slow but legal write never stops a multi-fragment move (v1's defect).** RS(2,2),
  fragments 2 and 3 lost, two free destinations in two free domains, and every stored write moves
  the `ManualClock` on by 12 s — longer than `W_repoint` (10 s), and twice that is still inside
  `W_write` (30 s, `gc.rs:202`), so no design is refused by the double for timing alone. Within at
  most TWO `reconcile_step` passes both fragments are rebuilt and adopted, the placement names
  both new servers and the obligation has drained. Leg (v) stays green beside it;
  (viii) **GC reclaiming the destination between write and adoption makes the adoption lose.**
  A `put_fragment` hook, after the destination write is stored and before the adoption CAS, does
  what GC's reclaim does: it swaps `orphan:<P_new>` to its `reclaiming` form with the same stamp
  and event (`OrphanMark::into_reclaiming`, `crates/core/src/metadata.rs:180`; GC's own swap,
  `Intent::record`, `gc.rs:589-595`) and then deletes the fragment from the destination, in that
  order (GC commits the swap before it deletes, `gc.rs:765-776`). The production pass then
  resumes: no adoption, the `part:` record byte-identical, the `reclaiming` mark left as GC wrote
  it, the obligation queued. The case asserts the destination received the write, so it cannot
  pass on a pass that writes nothing. This is the only case that fails an adoption CAS missing its
  pre-mark precondition; the existing DST adoption case (`crates/dst/tests/custodian.rs:3441-3455`)
  builds its own batch and never runs this path.
  **(D) The drain fence on the destination (`0016:885`).** A server with ANY
  `desired:dserver:<S>` record (`crates/custodian/src/desired_state.rs:35-40`), whatever its value
  — `maintenance` included — is never chosen, so selection and the CAS test the same fact. A drain
  recorded between selection and the adoption CAS makes the CAS lose
  (`require_absent(desired:dserver:<S_new>)`): not adopted, pre-mark still there.
  **(E) Kept, not rebuilt — nothing is written and the obligation stays queued:** an `sidx:`-only
  chunk; a chunk of a session that is not `Open` (a repair blocked by a Complete is retried after
  publication, `0016:825`), seeded as an RS(2,2) chunk with one fragment lost so that it is
  repairable and the refusal is the session state's, not the scheme's, with the pass answering
  `Blocked` as #813's kept path does (`reconstruction.rs:352`, `:448-462`); a chunk the staged reading holds as untrusted (`StagedSet::held`,
  `gc.rs:1337`) because a corrupt `sidx:` entry names it, even though a valid committed part in an
  `Open` session names it too; and a degraded chunk with no usable destination (every other
  server carries a `desired:` record).
  **(F) Seeded DST for X29**, appended to the existing `crates/dst/tests/custodian.rs` (keep
  `#![cfg(madsim)]`, `:53`). Sweep the session fence across every point of the re-place: before
  the pre-mark, between pre-mark and write, between write and CAS, after the CAS. In every
  interleaving no fragment ends unreferenced and unevidenced, and the session never ends
  `Aborting` with a `part:` record naming a fragment that was not written. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`). Record the seed count
  in `build-notes.md`.
- **Falsifiability:** RED in-process on `origin/main`, the gate's base for this bundle (no
  `Onto branch`, no prereq wave; `engine/scripts/run-verify.sh:263-269` resolves it). There
  reconstruction keeps the obligation and writes nothing, so A, B, C(iv), C(vii) and D fail by
  assertion. C(i)–(iii), (v), (vi) and (viii) fail there on the missing rebuild rather than on the rule;
  each rule's own branch must be reached in the green leg (C4-diff-cov reports it). C(vii) has a
  second red: v1's code fails it (four passes, no progress). Leg E is a guard and is GREEN on the
  base by design; it is not part of the red. The new test compiles on the base because both
  context fields exist there; the fix must add no other symbol the test names. The DST case is in
  a modified file, so C4-ci is its gate, not C4-verify.
- **Invariant to restore:** a staged chunk degraded while its session is `Open` is rebuilt, and
  no outcome strands a fragment: every fragment the re-place writes is, at every instant, either
  named by a record or covered by an `orphan:` mark GC can act on; the obligation is removed only
  in the commit that makes the repair durable; and a chunk that can be repaired is repaired in a
  bounded number of passes — no legal timing of a successful write leaves it degraded while
  passes keep running. Sources: C-1 (the harness catalogue `wyrd-pdca/docs/principles.md` §5 and
  its §6 storage-lifecycle row, not a target file: every durable byte is at every instant named
  by a record or evidenced for reclamation; its target sources are 0016's refutation standard,
  `0016:2802-2813`, and GC's "never reclaim a referenced fragment", `gc.rs:647`); `0016:825`,
  `:885`, `:888-889`; "no fragment is written after its evidence may have been reclaimed"
  (`:1356-1358`); the writers' obligation GC states (`gc.rs:250-258`); ADR-0045.
  SELF-TEST: a rebuild that writes then CASes without a pre-mark passes A and fails B; one that
  lets one write's latency use up the next write's authorization passes A–F and fails C(vii);
  one that writes a well-formed fragment with the wrong payload passes a header check and fails
  A; one that pre-marks but drops the pre-mark from the adoption CAS passes B and fails C(viii).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 777
- **Ordering note:** #813 is merged (PR #824, `feb1e30`), so the first brief's `Depends on: 813`
  is dropped and the base is plain `main`. `Conflicts with: 777`: #777 edits `reconstruction.rs`
  and the DST file. #809 and #810 name this id in their own `Conflicts with`; #508 and #625
  depend on it. SIZE ACCEPTED: v1's patch was 197 KB against the 100 KB backstop and went back to
  Plan for it. Measured by file, no cut puts a first child under the line (56 KB of edits to
  existing files and a 32 KB test harness land before the first rebuild test goes green), so on
  2026-09-21 the human chose one bundle over a split, knowing the patch will again be near
  200 KB. The size backstop firing at sign-off is expected, not new information. Intake cap: this
  repairs an ITERATE_PLAN id the flow is driving, which `wyrd-pdca-P1` exempts (count 21/6).
- **Surfaces:** data
- **Difficulty:** high — a multi-step metadata protocol with four fences across
  `reconstruction.rs`, `gc.rs`, a new module and the server composition, plus a DST property.
- **Do model:** opus-max
- **Scope:** rebuild and re-place a committed part's chunk in an `Open` session: pre-mark before
  write; deadline from the context clock at pre-mark commit plus the window; the `W_repoint`
  rule; one adoption CAS pinned to the session state, the prior `part:` bytes, the pre-mark's
  bytes and `require_absent(desired:dserver:<S_new>)`. On any loss the obligation stays queued
  and the pre-mark stands. Time and window come only from the two context fields; add no other.
  A committed part's chunk in an `Open` session that is found at full redundancy resolves as the
  committed path resolves one: its obligation drains as a duplicate finding
  (`reconstruction.rs:836-839`), and one no survivor set can rebuild (`EcScheme::None`, or fewer
  than `k` intact) is `Unrepairable`, as a committed one is (`:772`, `:850`). "Kept" (leg E) is
  for a chunk whose repair this pass may not or cannot make yet.
  Keep #813's legs G–J in `crates/custodian/tests/staged_protection.rs` passing, and do not move
  leg G off an `Open` session. Its committed-part case (`:2353-2401`) seeds an `Open` session
  whose only fragment, `EcScheme::None` (`:2360`), is lost; nothing can rebuild that, so its
  "obligation queued, no write, `part:` byte-identical, chunk named on the reconstruction audit
  seam" assertions stay true and stay as they are. The one intended change is its
  `Reconciled::Blocked` (`:2372`), which becomes `Reconciled::Satisfied`: the chunk is now
  assessed as a committed one, `Unrepairable` raises the data-loss signal (`:329`,
  `emit_data_loss` `:1197`, same audit target, same chunk name) and is not a hole in the pass
  (`:448-466`). Update its doc comment to say so. Refusal for a session that is not `Open` is
  leg E's, on a repairable RS fixture. The other G–J cases stay.
  Docs and markers this slice makes stale, all to be brought up to date: the sentence "it does
  not yet rebuild or re-place a staged chunk itself" in
  `docs/design/architecture/06-runtime-view.md:82`; the `deferred: #814` marker at
  `gc.rs:1322-1328`; the "#814" seam notes in `reconstruction.rs:44-59`, `:110-131`, `:706-707`,
  `:1272` and in `crates/server/src/custodian.rs`.
  / out of scope: the committed repair path (`repair_chunk`, `reconstruction.rs:960-1086`,
  unchanged in behaviour); one degraded chunk per part per pass — every plan in a part pins the
  same `part:` bytes, as a second obligation inside one committed object loses its CAS and waits
  for the next pass (`reconstruction.rs:396-399`); accepted here, say so in a comment;
  **settling a pre-mark** — a late write whose effect is `WriteEffect::Unknown`
  (`crates/traits/src/lib.rs:862-902`) landing after GC reclaims its pre-mark, which can only
  happen on a destination that already held bytes from an earlier aborted attempt, and a
  pre-mark whose write never landed staying in the `orphan:` ledger (GC's sweep keeps it,
  `gc.rs:260-269`). Both are one follow-up, #825: carry an in-code
  `deferred: #825` marker where the re-place aborts after a write, so review treats them
  as settled (target `AGENTS.md:200-205`); `EcScheme::None` answering Unrepairable (by design, as
  `reconstruction.rs:772`); servers absent from the live fleet; `seg:` repair (#777); rebalance
  and restore (#809, #810); the upload-side drain fence (#657); `crates/core/src/multipart.rs`;
  edits to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, delete one of its fragments, `enqueue_repair` its chunk and run a reconstruction pass:
  the obligation is still queued and no fragment was written.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_repair.rs` — **NEW** (`run-verify.sh --classify`
  on v1's patch returns `ADDED_TEST crates/custodian/tests/staged_repair.rs`; the DST file is
  modified, not added). Never add a new DST file: an added `#![cfg(madsim)]` file joins
  C4-verify's single cargo run and switches it to `--cfg madsim` (`run-verify.sh:155-176`).
- **Production reach:** the passes under test are the production `reconcile_step`. The test
  applies the session fence itself because Abort and Complete (#656, #658) do not exist yet. That
  is intended: the race is the re-place against any fence, and a fence is a CAS on the `mpu:`
  record whoever writes it.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY open:
  `reconstruction.rs:721-878` (`assess`) and `:960-1086` (`repair_chunk`: rebuild, destination
  choice, CAS shape `:1068-1071`; it passes no deadline at `:1065`, this path must);
  `crates/traits/src/lib.rs:837-970` (`WriteDeadlineExpired`, `if_elapsed`,
  `if_publication_unverified`); `gc.rs:196-270` (`W_write`, `W_repoint`, the writers'
  obligation); `crates/custodian/tests/reconstruction.rs` (repair-harness idioms). Prior art Do
  SHOULD start from: `results/issue_814/iteration-v1/patch.diff` — see the carry-forward below.
- **Prior-art check (triage cycles):** by path across merged history and open PRs (2026-09-21): no
  staged re-place exists on `main`; `git log origin/main` for `reconstruction.rs`, `gc.rs`, the
  DST file and `06-runtime-view.md` ends at #813's commit `f683dbe`, and no `reconstruction/`
  directory exists. v1's review checked all eight paths against every closed, unmerged PR and
  found only #647 and #336, neither a staged re-place. Rejected prior art: #814 v1 (size, plus
  the C(vii) defect), #663 v1–v3 (size, not protocol) and #637 v1 (the undecodable-source commit,
  the reused destination stamp and the never-exercised deadline: C(vi), C(ii), the enforcing
  doubles).
- **Disposition hint:** likely-fix

## Carry-forward from v1 (`results/issue_814/iteration-v1/`)

v1 was sent back to Plan for size, which the human has since accepted (Ordering note). Its patch
still applies cleanly to `origin/main` @ `feb1e30` and is the starting point, not a rejected
approach: it passed `cargo xtask ci`, C4-verify (18 tests red on the base by assertion, 20 green)
and diff coverage at 95.1%, and an adversarial review could not strand a fragment on the
single-write move. What this build must change:

1. **Fix C(vii).** v1 stamps one pre-mark for every destination, then awaits the writes one after
   another and re-checks the `W_repoint` rule against that one stamp before each
   (`crates/custodian/src/reconstruction/staged.rs:606-650` in v1's patch). A legal 12 s first
   write therefore refuses the second write on every pass, forever, while each pass rewrites the
   first fragment. Keep C(v) true while fixing it. v1's two-loss test used immediate writes and
   its stale-mark test had one missing fragment, so neither saw this.
2. **Add the two leg-E cases** the mutation and coverage gates showed untested: the held chunk
   (v1's `held` guard in `assess` survived deletion) and the no-usable-destination outcome.
3. **Put the `deferred: #825` marker in** (Scope, out of scope). v1 said in a comment that
   it does not settle its pre-marks and named no issue, which review raised three times.
4. **Undo v1's leg-G retarget** (Scope). v1 moved the committed-part case to `Aborting` and
   changed its answer in the same edit, which dropped the only `Open` single-copy guard on the
   `staged_protection.rs` side. Put it back on `Open`, change only `Blocked` → `Satisfied`, and
   drop v1's `an_open_uploads_single_copy_chunk_is_unrepairable` from `staged_repair.rs`, which
   then duplicates it. Leg E's non-`Open` case uses an RS fixture instead.
5. **Add leg C(viii)** and apply the byte check of leg A to C(iii) and C(vii). v1's adoption
   already pins the pre-mark (`staged.rs`, the adoption batch in v1's patch), and v1's
   `holds_intact` helper already compares the payload with the seeded shard, so both are
   expected to be test-only work. If C(viii) goes red on v1's code, fix the code, not the case.
6. Do not grow the patch beyond what 1–5 need. Record in `build-notes.md` what changed from v1.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

Plan-review response: all three findings verified on `feb1e30` and taken into the brief, none
rebutted. (1) Leg A now requires the rebuilt payload to equal the seeded shard byte for byte,
and C(iii) and C(vii) inherit that check. (2) New leg C(viii) has GC reclaim the destination
between write and adoption; the adoption must lose. (3) Leg G stays on an `Open` single-copy
fixture; its only change, `Blocked` → `Satisfied`, is now stated in Scope with its reason, and
the non-`Open` refusal moves to leg E on a repairable RS fixture. That `Blocked` → `Satisfied`
change is a visible behaviour change for sign-off to weigh.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Missing regression tests on the core durability property: no test proves destination writes are sent concurrently (join_all) rather than sequentially, and a sequential regression would let a legal-but-slow two-write repair blow its deadline and stay degraded forever — the brief's central invariant. Also close the two surviving-mutant gaps: outage classification (reconstruction.rs:913, Unreachable arm untested for staged chunks) and selective reading (staged.rs:212, stale/duplicate part-record filtering untested). Confirmed not defects, no rebuild needed for these: the EcScheme::None / single-copy "Blocked"->"Satisfied" behavior change (staged.rs:280, also T4 batch review's blocking finding) was requested by the brief. Size backstop overridden — already litigated in the brief's Ordering note on 2026-09-21. Efficiency finding (N(N+1)/2 rebuild cost, staged.rs:589) filed to Act log as a separate enhancement, not a blocker for this bundle. Three other §6 findings (wrong payload accepted, missing adoption-fence test, leg-G rewrite premise) read as stale against the current brief text and were left untouched, as were the harness count bug and the mixed-version D-server caveat.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Missing regression tests on the core durability property: no test proves destination
  writes are sent concurrently (join_all) rather than sequentially, and a sequential
  regression would let a legal-but-slow two-write repair blow its deadline and stay
  degraded forever — the brief's central invariant. Also close the two surviving-mutant
  gaps: outage classification (reconstruction.rs:913, Unreachable arm untested for staged
  chunks) and selective reading (staged.rs:212, stale/duplicate part-record filtering
  untested).

  Confirmed not defects, no rebuild needed for these: the EcScheme::None / single-copy
  "Blocked"->"Satisfied" behavior change (staged.rs:280, also T4 batch review's blocking
  finding) was requested by the brief. Size backstop overridden — already litigated in
  the brief's Ordering note on 2026-09-21. Efficiency finding (N(N+1)/2 rebuild cost,
  staged.rs:589) filed to Act log as a separate enhancement, not a blocker for this
  bundle. Three other §6 findings (wrong payload accepted, missing adoption-fence test,
  leg-G rewrite premise) read as stale against the current brief text and were left
  untouched, as were the harness count bug and the mixed-version D-server caveat.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 83 mutants tested in 12m: 7 missed, 27 caught, 48 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_814 — rebuild to close the test gaps; the production code is not rejected (the adversary could not break it, and all correctness gates pass). Add the missing tests for the pre-mark batch's preconditions, each of which can be deleted today with every test still green: 1. The pin on the destination's existing mark (staged.rs:624-625) — the important one. GC swaps the destination's mark to `reclaiming` after `position()` reads it (staged.rs:534) and before the pre-mark commits. Use an `after_read_of(mark_key(2, 2))` hook on `Fixture::standard` with a structured mark seeded there, committing GC's `into_reclaiming()` swap. Assert: no write arrived, the `reclaiming` mark is byte-identical, the obligation is still queued. The existing C(iii) case seeds the mark before the pass starts, so it never runs this race. 2. The pre-mark's `require(part == prior)` (staged.rs:620): same hook point, rewrite the part record; assert no write arrives. 3. The pre-mark's `require_absent(desired:dserver:<S_new>)` (staged.rs:628): same hook point, seed `desired:dserver:2`; assert no write arrives. The existing drain test records the drain after the write, so it only covers the adoption's pin. If any of these goes red on the current code, fix the code, not the case. Confirmed at sign-off as intended, do not change: the two further Blocked -> Satisfied answers (no usable destination, staged_repair.rs:1856; below k behind an outage, staged_repair.rs:2006). They follow the brief's "resolves as the committed path resolves one"; the conditions report through the repair_blocked / unreachable gauges and the audit seam. Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on the batch operation budget for very wide schemes (staged.rs:704, pre-mark staged.rs:621). It needs 100+ fragments moved in one chunk, which no realistic profile has, and the committed repoint has the same shape. Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on first-reference-only repair of duplicate part references (staged.rs:212). Sign-off agrees with the adversary: chunk ids are minted fresh per write (crates/server/src/lib.rs:255-258) and nothing copies chunks between parts, so two part records naming one chunk means something went wrong earlier; it is not a state this repair has to converge. The committed reading applies the same first-reference rule (reconstruction.rs:677). Both T4 blocking findings are therefore settled; do not re-raise them. Do not grow the patch beyond the three tests.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  issue_814 — rebuild to close the test gaps; the production code is not rejected (the adversary could not break it, and all correctness gates pass).

  Add the missing tests for the pre-mark batch's preconditions, each of which can be deleted today with every test still green:
  1. The pin on the destination's existing mark (staged.rs:624-625) — the important one. GC swaps the destination's mark to `reclaiming` after `position()` reads it (staged.rs:534) and before the pre-mark commits. Use an `after_read_of(mark_key(2, 2))` hook on `Fixture::standard` with a structured mark seeded there, committing GC's `into_reclaiming()` swap. Assert: no write arrived, the `reclaiming` mark is byte-identical, the obligation is still queued. The existing C(iii) case seeds the mark before the pass starts, so it never runs this race.
  2. The pre-mark's `require(part == prior)` (staged.rs:620): same hook point, rewrite the part record; assert no write arrives.
  3. The pre-mark's `require_absent(desired:dserver:<S_new>)` (staged.rs:628): same hook point, seed `desired:dserver:2`; assert no write arrives. The existing drain test records the drain after the write, so it only covers the adoption's pin.
  If any of these goes red on the current code, fix the code, not the case.

  Confirmed at sign-off as intended, do not change: the two further Blocked -> Satisfied answers (no usable destination, staged_repair.rs:1856; below k behind an outage, staged_repair.rs:2006). They follow the brief's "resolves as the committed path resolves one"; the conditions report through the repair_blocked / unreachable gauges and the audit seam.

  Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on the batch operation budget for very wide schemes (staged.rs:704, pre-mark staged.rs:621). It needs 100+ fragments moved in one chunk, which no realistic profile has, and the committed repoint has the same shape.

  Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on first-reference-only repair of duplicate part references (staged.rs:212). Sign-off agrees with the adversary: chunk ids are minted fresh per write (crates/server/src/lib.rs:255-258) and nothing copies chunks between parts, so two part records naming one chunk means something went wrong earlier; it is not a state this repair has to converge. The committed reading applies the same first-reference rule (reconstruction.rs:677).

  Both T4 blocking findings are therefore settled; do not re-raise them. Do not grow the patch beyond the three tests.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 83 mutants tested in 12m: 4 missed, 30 caught, 48 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_814 — rebuild only to close one test gap; the production code is not rejected (the adversary could not break it, every correctness gate passes, and the previous round's three requested tests are confirmed real). Add ONE test in `crates/custodian/tests/staged_repair.rs` for the adoption's two pins on the VACATED position's mark (`crates/custodian/src/reconstruction/staged.rs:718` `require_absent`, `:721` `require(key, current)`). Both can be deleted today with all 29 + 36 tests still green. Shape, per the adversary's probe: `Fixture::standard` with an `after_read_of(fx.mark_key(3, 2))` hook that commits GC's `into_reclaiming()` swap on the vacated mark after `assess` reads it (`staged.rs:296-307`) and before the adoption commits. Two arms: (a) a seeded structured mark on P_old, swapped to `reclaiming` — kills the `:721` pin; (b) no mark on P_old, another event marks it and GC swaps that to `reclaiming` — kills the `:718` pin. Assert: nothing adopted, `part:` byte-identical, the `reclaiming` mark byte-identical to what GC wrote (never overwritten with `legacy(now)`), obligation still queued. If either arm goes red on the current code, fix the code, not the case. Do not grow the patch beyond this one test. Confirmed at this sign-off, do not re-raise: the T4 batch-budget blocking finding (`staged.rs:704`) and the size backstop stay overridden as before; the three plan-advisory §6 findings (wrong payload, adoption-fence test, leg-G premise) are stale — each is already answered by the current brief and patch (`staged_repair.rs:716-717`, `:1678`, `staged_protection.rs` leg G hunk) and should not be acted on.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  issue_814 — rebuild only to close one test gap; the production code is not rejected (the adversary could not break it, every correctness gate passes, and the previous round's three requested tests are confirmed real).

  Add ONE test in `crates/custodian/tests/staged_repair.rs` for the adoption's two pins on the VACATED position's mark (`crates/custodian/src/reconstruction/staged.rs:718` `require_absent`, `:721` `require(key, current)`). Both can be deleted today with all 29 + 36 tests still green. Shape, per the adversary's probe: `Fixture::standard` with an `after_read_of(fx.mark_key(3, 2))` hook that commits GC's `into_reclaiming()` swap on the vacated mark after `assess` reads it (`staged.rs:296-307`) and before the adoption commits. Two arms: (a) a seeded structured mark on P_old, swapped to `reclaiming` — kills the `:721` pin; (b) no mark on P_old, another event marks it and GC swaps that to `reclaiming` — kills the `:718` pin. Assert: nothing adopted, `part:` byte-identical, the `reclaiming` mark byte-identical to what GC wrote (never overwritten with `legacy(now)`), obligation still queued. If either arm goes red on the current code, fix the code, not the case.

  Do not grow the patch beyond this one test. Confirmed at this sign-off, do not re-raise: the T4 batch-budget blocking finding (`staged.rs:704`) and the size backstop stay overridden as before; the three plan-advisory §6 findings (wrong payload, adoption-fence test, leg-G premise) are stale — each is already answered by the current brief and patch (`staged_repair.rs:716-717`, `:1678`, `staged_protection.rs` leg G hunk) and should not be acted on.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 83 mutants tested in 12m: 4 missed, 30 caught, 48 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
