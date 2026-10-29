# Result — issue 933 / multipart-upload-part-stage-renewal

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 657.1a a part can be staged safely, but its leases are written only at the
  reserve and at each intent. A request slower than one lease TTL, before its first chunk or
  between chunks, therefore stops with a lapse error. And until it stops, the reaper and the
  idle fence would read it as abandoned (`0016:1824-1837`). This slice adds in-flight renewal. It
  (1) moves the slot's lease and every owned lease forward together, in one batch, half a TTL
  ahead, for the whole request, including while a body read or a fragment write is still pending.
  It (2) races that renewal against 657.1a's write deadline and body-idle bound, so renewal never
  holds staging past either. It (3) refuses a lapsed lease rather than reviving it. It (4) settles
  an unknown renewal outcome by a re-read, or reports it in doubt. It (5) gives the staged attempt
  its quiesce renewal for the commit (`StagedAttempt::renew`, `0016:1720-1733`). It (6) keeps every
  renewal batch inside `B_ops` (`MAX_BATCH_OPS`, `crates/core/src/multipart.rs:5106`).
- Success criterion: New legs **appended** to `crates/core/tests/multipart_upload_part_stage.rs`
  (657.1a's file), over its harness: the logging store wrapper, the deadline-enforcing chunk-store
  fake, and the hand-moved clock and timer. Every leg is bounded by the harness's watchdog and ends
  any body channel it feeds, so a regression fails fast instead of hanging. R8's clamp rows go in
  the existing knob test, `crates/core/tests/multipart_knobs.rs`. Every one of 657.1a's legs still
  passes unchanged, except where a leg asserted the absence of renewal; such a leg is updated, and
  `build-notes.md` names it. The legs show all of the following.
  **(R1) The renewal batch.** Every renewal batch rewrites the attempt's own `slot:` record in the
  same batch as **every** owned `sidx:` lease of the attempt, under `require(mpu == Open@E)`,
  `require(slot == prior)` and `require(sidx: == prior)` for each entry (`0016:349`, `:659`). Each
  lease it writes is strictly later than the slot lease it replaces.
  **(R2) Renewal runs for the whole request, whatever is pending.**
  - **Before the first chunk:** a body delivered in small pieces, with the clock moved past half a
    lease TTL between them and before the first chunk boundary, gets its slot lease moved forward
    before the first intent.
  - **Body withheld (D-i):** after the slot is reserved, the test withholds the next piece, moves
    the clock past half a TTL and fires the timer. A renewal batch rewriting the slot commits
    **while the read is still pending**. Then the piece is delivered and staging finishes
    normally, with the piece neither dropped nor re-requested.
  - **Fragment write held (D-ii):** the fake holds one fragment put of chunk 2, after chunk 1's
    owned entry is durable. The test moves the clock and fires the timer. A renewal batch rewriting
    the slot and chunk 1's and chunk 2's owned leases commits **while the put is still pending**.
    Then the put is released (before its deadline), staging finishes normally, and that put is not
    re-sent.
  - A renewal falls due at exactly half a TTL after the last one; a renewal due as an intent starts
    is sent before that intent, and never while the attempt's own intent batch is in flight.
  - In all of these, with the clock moved by hand and no renewal commit held past half a TTL,
    neither the slot lease nor any owned lease is ever observed lapsed while the client is still
    supplying data (`0016:1824-1837`). That is a promise **under timing assumptions**, not an
    absolute one (revised 2026-10-22): it holds while every renewal round — its reads, its commit
    and its bounded retries — finishes inside half a TTL, and while the observer's clock agrees with
    the renewer's within `δ_clock`. A renewal round slower than that can leave a lease that an
    observer reads as lapsed; 0016 allows it and closes it elsewhere (next leg).
  - **A renewal that crosses expiry** (added 2026-10-22; `0016:1843-1874`). The renewal checks
    every lease live and sends; the wrapper holds its commit. The test moves the clock past the
    slot's **previous** lease expiry (no fragment deadline or idle bound passes), then, standing in
    for the reaper, reads the slot's bytes `S_old` and sees the lease lapsed. It releases the
    renewal, which lands — the store checks bytes, not expiry. It then commits a stand-in idle-fence
    batch, `require(mpu == Open@E)` + `require(slot:<id>:<k> == S_old)` + a put of the session at
    `Aborting`: that batch answers `Conflict`, because the renewal rewrote the slot (R1). The
    session is still `Open`, and staging goes on under the new leases and finishes normally. The
    renewal that sampled its leases live is taken, not refused after the fact. A renewal that
    **samples** a lapsed lease refuses (R4). A renewal batch without the slot rewrite lets the
    stand-in fence commit, so this leg fails.
  **(R3) Renewal never holds staging past a deadline.**
  - **The reviewers' schedule** (v2 sign-off): the fake holds a fragment put and the
    wrapper holds a renewal commit; the test moves the clock past the fragment deadline. Staging
    ends at the deadline, before the renewal is released, with an in-doubt error that keeps the
    renewal's `may_still_commit` doubt.
  - **The adversary's schedule:** a put answered in time while a slow renewal is still committing
    is taken, and staging succeeds.
  - **The put answers in time, then the renewal outlives the put's deadline** (added 2026-10-22).
    Exact order: the fake holds the chunk's last pending put and the wrapper holds a renewal
    commit; the fake publishes the put and answers `Ok` before its deadline (the hook waits until
    the fake has stored the fragment); the test then moves the clock **past that put's deadline**
    while the renewal is still held; then the wrapper releases the renewal as `Committed`. Staging
    succeeds — not in doubt — with the renewal's leases adopted and the put not re-sent. A second
    run releases the renewal as `CommitUnknownResult { may_still_commit: false }`, applied: it is
    settled by the re-read (R5) and staging succeeds. The rule (Design § *Which deadlines are
    armed*): a write deadline stops constraining the renewal once every write it bounds has
    answered; the renewal is then awaited to its end, bounded by the store's own operation
    deadline and the renewal's round bound.
  - **A write answered while a renewal commits is taken** — and the leg proves the write was
    published **before** its deadline: the hook that releases the held put waits until the fake
    has stored the fragment before it moves the clock (v3 sign-off; v3's hook at v3
    `crates/core/tests/multipart_upload_part_stage.rs:1589` moved the clock first, so the put
    published 101 ms late and passed only because v3's fake did not enforce deadlines; 657.1a's
    fake now refuses that).
  - A renewal cut off by the deadline between two of its own commits keeps exactly the doubt of
    the commit still in flight, no more and no less.
  - **The body-idle bound still holds under renewal:** a stalled body is stopped at 657.1a's idle
    bound even though renewals keep its leases alive meanwhile, and empty pieces still do not reset
    it.
  **(R4) A lapsed lease is refused, never revived** (`0016:1843-1847`; the peer rule at
  `crates/core/src/write.rs:496-517`).
  - With the clock moved past the slot's lease, or past an owned entry's, before the next renewal,
    the renewal refuses, writes nothing, and staging stops with a typed error.
  - **Every renewal round re-reads the clock and re-checks every lease before it sends**
    (iteration-1 finding), while still settling a batch it already sent: a conflict or held round
    whose retry crosses a lease's expiry refuses the retry and keeps the earlier round's doubt.
  - A renewal after the session left `Open` refuses and resurrects nothing.
  - A slot lost to another writer under an unchanged session is a typed error.
  **(R5) Unknown renewal outcomes are settled by a re-read, never guessed.** A renewal answered
  `CommitUnknownResult` is settled by re-reading its own slot's exact bytes: landed is adopted, and
  not landed with `may_still_commit: false` is retried. A held renewal that lands before the
  session is seen to move is adopted. A held renewal that never settles ends in an in-doubt error
  carrying its `may_still_commit` doubt once the rounds run out.
  **(R6) The renewal loop is bounded.** With a wrapper that answers every batch `Conflict` without
  applying it, one renewal ends in a typed error after **exactly** its stated maximum of commits.
  **(R7) The quiesce renewal (`StagedAttempt::renew`).** The method takes the caller's clock (not
  a fixed `now_millis`), reads it every round, never moves a lease backwards, refuses a lapsed
  lease, and answers a typed refusal (`NoSuchUpload`) once the session has left `Open`. 657.2 (#927)
  calls it as the last renewal before its part commit.
  **Coverage at hand-off** (added 2026-10-22). 0016 requires the final renewal to stamp a lease
  that covers at least the commit's transaction deadline (`0016:1726-1728`). So `renew` also
  takes a caller-supplied coverage `cover_millis` (#927 passes at least the commit's transaction
  deadline, the 5,000 ms envelope `E_TX_MILLIS`, `crates/core/src/multipart.rs:5053`). It answers
  success only if, at a clock reading taken **after** its last commit returned, every lease it
  wrote expires at least `cover_millis` later; the success value carries that reading and the
  lease expiry, so #927 can check them. A commit that returns too late for that runs another round,
  within the renewal's round bound; if the rounds run out, `renew` returns a typed error and no part
  commit should follow. A `cover_millis` not below the lease TTL is a typed input error, writing
  nothing. Legs: (i) the wrapper holds the quiesce commit while the test moves the clock by
  `TTL − cover_millis + 1`, then releases it; the next round is not held. `renew` does not answer
  success on the first round's lease; it renews again and answers success whose reported reading
  and expiry are at least `cover_millis` apart. (ii) Every round is held that long: a typed error
  after exactly the round bound. (iii) `cover_millis ≥ TTL`: refused, nothing written.
  **(R8) Every renewal batch fits `B_ops`** (v3 sign-off).
  - `knob_clamps_hold` (`crates/core/src/multipart.rs:5430`) refuses, **by name**, a `KnobSet`
    whose `max_part_chunks` would make a full renewal batch exceed its `batch_ops`, the way it
    refuses an oversize part commit (`:5385`, `:5453-5454`). The adversary's case passes every
    clamp on `main` today (re-checked in Plan, 2026-10-22, by a scratch run of
    `multipart_knobs.rs`): `derived(with(D, |k| { k.scheme = EcScheme::None; k.max_part_chunks =
    249; }))`, re-derived by that file's own `derived` (`crates/core/tests/multipart_knobs.rs:105-116`;
    without the re-derivation the set fails `SessionsOverFootprint` instead). Its renewal would
    carry `3 + 2 × 249 = 501` operations against `MAX_BATCH_OPS = 500` (`:5106`). With the clamp it
    is refused; the set one chunk smaller (248, 499 operations) passes. `KnobSet::DEPLOYED` still
    passes (158 chunks, 319 operations).
  - **The new clamp binds before the part-commit clamp.** `2N + 3 > N + 8` for every `N ≥ 6`, so at
    the shipped `N = 158` a `batch_ops` that fails the part commit (`N + 8 = 166`) already fails
    the renewal (319). So existing Leg 2 rows stop testing what they say: the on-bound row
    `with(D, |k| k.batch_ops = part_ops) => Ok(())` (`multipart_knobs.rs:200`) fails outright, and
    the two `PartCommitOverOps` rows (`:242-243`) answer the renewal clamp instead if it is checked
    first. Keep Leg 2's rule, "for every clamp, a set exactly on its bound passes and one step past
    is refused by name" (`multipart_knobs.rs:8-10`): move the part-commit rows to a set where the
    part commit binds (`N ≤ 4`, e.g. `TINY`, `:68-74`), and add the renewal clamp's own on-bound and
    one-past rows. `build-notes.md` names every row it changed.
  - Staging refuses, before writing anything, a request whose chunk bound would build a renewal
    batch past `MAX_BATCH_OPS`.
  **(R9) The whole gate.** `cargo xtask ci` is green on the patched tree (`AGENTS.md:41-45`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: in-flight renewal of an attempt's slot and owned leases in `wyrd-core`'s staging
  module: the renewal batch; when it runs (half a TTL ahead, for the whole request, including while
  a body read or fragment write is pending, on the caller's timer, never while the attempt's own
  intent is in flight); its race against 657.1a's write deadline and body-idle bound; its lapse
  refusal and per-round clock re-read; the settling of its unknown outcomes; its bounded loop; the
  quiesce renewal on the staged attempt; the renewal batch-size clamp in `knob_clamps_hold` and in
  staging's request check; the module docs and the docs paragraph 657.1a added; and the new legs.
  / out of scope:
  - anything 657.1a owns that renewal does not change: the reserve, the intent, placement, the drain
    fence, the deadlines themselves, the hand-off's fields, the drain key, 0016;
  - the part commit and everything else in 657.2 (#927), including **when** the commit calls the
    quiesce renewal;
  - the reaper (#625), the idle fence's own batch, and any knob loading (#508);
  - seeded DST races (#660, #665); any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's); any
    edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`. 0016's knob table
    (`0016:1462-1479`) does not list the renewal clamp; do not edit it here, and say so in
    `build-notes.md` so the human can decide whether 0016 needs the row.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 68 mutants tested in 8m: 1 missed, 51 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_933/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.67s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #933’s in-flight UploadPart slot/owned-lease renewal, deadline races, quiesce coverage, and batch limits: no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract defines observable timing, expiry, uncertainty, coverage, and capacity boundaries, including the slow-renewal exception; `brief.md:30`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843`. |
| C2 Reproduction (red pre-fix) | PASS | With the supplied patch stashed, three independent behavioral probes failed: a withheld read left its slot unrenewed, and both request and knob validation admitted a 501-operation renewal; `reviewer-evidence/red.log:7`. |
| C3 Change | PASS | The five changed files stay within the approved staging, knob, test, and living-document scope; the production-wiring boundary remains explicit; `crates/core/src/multipart/stage.rs:19`, `docs/design/architecture/05-building-block-view.md:246`. |
| C4 Verification (red→green) | PASS | All three probes passed after stash-pop, all 89 staging and 8 knob tests passed, and CI components passed after correcting the audit-cache location; coverage against prerequisite-free main remains unmeasured; `reviewer-evidence/green.log:10`, `reviewer-evidence/ci.log:1235`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | PASS | The stale-fence race is exercised against real CAS, unknown outcomes retain their doubt, and quiesce success requires post-commit coverage; no capability-probe workaround was added; `crates/core/tests/multipart_upload_part_stage.rs:4978`, `crates/core/src/multipart/stage.rs:1699`, `crates/core/src/multipart/stage.rs:1758`. |
| T1 Structure | PASS | Clock ownership and metadata dependencies remain narrow and explicit, with one renewal implementation shared by staging and quiesce; `crates/core/src/multipart/stage.rs:338`, `crates/core/src/multipart/stage.rs:575`, `crates/core/src/multipart/stage.rs:1638`. |
| T2 Shape | PASS | The unsplittable renewal fits the operation budget, while separate boundary tests preserve coverage of the tighter renewal clamp and the part-commit clamp; `crates/core/src/multipart.rs:5190`, `crates/core/tests/multipart_knobs.rs:200`, `crates/core/tests/multipart_upload_part_stage.rs:6422`. |
| T3 Runtime | PASS | Pending work retains its deadline and is not resent; renewal failures preserve pending-write uncertainty, and retries terminate at four commits; `crates/core/src/multipart/stage.rs:1948`, `crates/core/src/multipart/stage.rs:2218`, `crates/core/tests/multipart_upload_part_stage.rs:6158`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; the substantive artifact audit must rerun at publish, as the deferred gate records; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether proposal 0016 needs a separate renewal-budget constraint update — its knob table lacks this tighter bound, and the brief expressly reserves that document decision while excluding the edit here; `brief.md:214`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1468`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the staging-only handoff and its timing assumptions for the subsequent part commit — deterministic schedules demonstrate the scoped behavior, while production wiring and seeded integration races remain the explicitly tracked later slices; `brief.md:260`, `crates/core/src/multipart/stage.rs:137`, `crates/core/src/multipart/stage.rs:201`. |

Source citations above are relative to `$PDCA_TARGET`; brief and evidence citations are relative to this review directory. The supplied disposable target was readable and current for this patch. After the stash/red/pop/green exercise, its diff matched `patch.diff` byte-for-byte (`reviewer-evidence/commands.json:5`). No implementation changes remain from the review.

The independent checks support the implementation, with these evidence limits:

- **Behavioral verification:** the same three reviewer assertions failed on the pre-fix tree and passed on the patched tree. The probe source and commands are retained in `reviewer-evidence/probes.rs.txt` and `reviewer-evidence/commands.json`. The frozen verifier establishes green only (`gate-logs/C4-verify.log:151`); it is not the source of the independent red→green result. The full CI rerun also passed the 97 scoped tests, workspace checks, conformance, and simulation tests (`reviewer-evidence/ci.log:991`, `reviewer-evidence/ci.log:1235`, `reviewer-evidence/ci.log:3731`, `reviewer-evidence/ci.log:4277`).
- **Host and base caveats:** raw `cargo xtask ci` exited 1 solely because both advisory checks attempted an exclusive lock in the read-only shared cache (`reviewer-evidence/ci.log:3720`, `reviewer-evidence/ci.log:4357`). Both passed when rerun with copies of the same policies changing only the cache path (`reviewer-evidence/deny.log:7`, `reviewer-evidence/deny-all-features.log:1`). Spelling and the actual docs renderer ran successfully (`reviewer-evidence/ci.log:2`, `reviewer-evidence/ci.log:10`); neither external dependency is outstanding. The frozen diff-coverage failure concerns application to `origin/main`, whose path history lacks the prerequisite staging files; it is not a compile or application defect in this supplied target. No coverage percentage is claimed.
- **Prior art and standing rubric:** GitHub merged history was queried separately for all five affected paths; all 19 closed, unmerged PRs had their changed-file lists checked, with no overlap (`reviewer-evidence/prior-art.json:1`). The brief records the rejected/iterated #657/#926 work by path (`brief.md:287`). The tracked #660/#665 simulation deferrals remain settled, and the living architecture description is current. The frozen batch-review log reports zero blockers (`gate-logs/T4-batch-review.log:10`); the TiKV feature log shows both actual clippy builds completed (`gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:210`). Those instance-scoped gates were adjudicated from their captured logs.

The independent mutation audit matches the frozen result: **51 caught, 16 unviable, one equivalent survivor** (`reviewer-evidence/mutation-summary.json:2`; `gate-logs/C5-mutants.log:13`). Mutations initially rejected only by warnings were rerun with mutation-only lint suppression before counting the result. The survivor changes `>` to `>=` at `crates/core/src/multipart/stage.rs:326`: `renewal_batch_ops(N) = 3 + 2N` is always odd, whereas `MAX_BATCH_OPS` is currently 500, so equality is unreachable (`crates/core/src/multipart.rs:5171`, `crates/core/src/multipart.rs:5191`). This is not a missing boundary test.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #933 (657.1b, in-flight renewal)

Verdict: **could not refute the fix.** I re-ran the evidence, made four of the brief's required red edits myself, and drove the branches no leg reaches. The renewal code behaved correctly every time. What is left is two test and doc gaps the builder can close, and two gate rows a human should check before trusting them.

## Attempts that failed to refute

- **The evidence holds, and it runs the production path.** In a scratch copy of the patched tree, all 89 legs of `crates/core/tests/multipart_upload_part_stage.rs` pass. They call `stage_part` and `StagedAttempt::renew` over a real redb store, not over a copy of the logic. The C4-verify "pass" row is green-only (`gate-logs/C4-verify.log`, last lines), so I made four of the brief's required red edits myself:
  - Slot put dropped from `renewal_batch` (`crates/core/src/multipart/stage.rs:1616-1620`): 26 legs fail. `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` fails for exactly the reason it states: the stand-in fence answers `Committed` instead of `Conflict` (test file `:5024`).
  - Renewal disabled (`renew_at = u64::MAX` at `stage.rs:1838-1841` and `:1909`): 31 legs fail, including every R2 leg.
  - Iteration-1 fix reverted (`stage.rs:2228` returns the halt directly): both `r4_a_renewal_stopping_beside_a_pending_write…` and `r4_…after_the_writes_answered…` fail.
  - Quiesce cover judged at the round's own reading instead of after the commit (`stage.rs:1758`): `r7_the_quiesce_renewal_covers_its_hand_off_or_refuses` fails.
- **The logic.** I tried each of these and found no failing input:
  - abandoning a renewal between its commits or during one (`RenewalLedger::abandoned`, `stage.rs:1072-1090`);
  - matching unknown outcomes by slot bytes only (`stage.rs:1703-1707`);
  - disarming the write deadline once every put has answered (`stage.rs:1992-1995`);
  - poll order in the three-way race (`stage.rs:2004-2012`);
  - the refusal path beside pending writes (`stage.rs:1878-1905`);
  - lease monotonicity (`stage.rs:1567-1571`);
  - the clamp count `3 + 2N` (`crates/core/src/multipart.rs:5190-5192`, `:5539-5542`).
- **The single C5 survivor is an equivalent mutant, not a test gap.** The mutant swaps `>` for `>=` at `stage.rs:326`. `3 + 2N` is always odd and `MAX_BATCH_OPS` is 500, which is even, so the two comparisons agree for every `u32` N.

## Findings

- NEEDS-HUMAN [impl] — **Three changed branches in the renewal race are never run by any leg.** I measured diff coverage with `cargo llvm-cov` over the stage and knob test files, because the gate never measured it.
  - `stage.rs:2024` (`None => break None`): a body answer seen at or after its idle deadline while a renewal commit is still in flight. This is the only path where R3's "renewal never holds staging past the body-idle bound" meets a late answer. `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases` only covers the timer path.
  - `stage.rs:1733`: a renewal round lost to a session moved to a later `Open` epoch. It should adopt the new epoch and retry.
  - `stage.rs:2018`: the timer fires beside a renewal and the work answers in the same instant.

  I drove all three with scratch legs, and the code is correct:
  - **X2.** Setup: `body_idle_millis = 8_000`, renewal `Act::Gated`. The clock advances by `HALF`, then the timer fires, then the renewal reaches its gate. Then `clock.set(T0 + 8_000)` and the rest of the body is sent. Result: `InDoubt { may_still_commit: true, backend: ABANDONED_COMMIT, cause: BodyIdle { idle_millis: 8000 } }`, with no intent.
  - **X1.** Setup: `Act::First(move_session(.., open_at(session, 1)))` on the first renewal. Result: renewal answers `[Conflict, Committed]`. The second renewal and every intent require `Open@1`, and the attempt hands off `Open@1`.
  - **X3.** Setup: an `on_fire` hook sends the rest of the body as an early timer fires beside a gated renewal. Result: staged, digest intact.

  Add these as legs so a regression on any of these branches goes red.
- NEEDS-HUMAN [human] — **C4-diff-cov has not measured this bundle in two rounds** ("patch.diff does not apply on origin/main", `gate-logs/C4-diff-cov.log`). The bundle is either stale against `main` or stacked on an unmerged #932. Either way, it needs a rebase or refresh before publishing, and the rubric asks that stacked branches be refreshed before review.

  My own measurement: `stage.rs` has 394 instrumented added lines, 15 of them uncovered:
  - 808-810 and 815-820: `Display` for `RenewalRoundsSpent` and `CoverShort`;
  - 866, 872, 878: `Display` for `WriteEffectUnknown`, which neither file renders;
  - 1733, 2018, 2024: the three branches above.

  `multipart.rs` has 7 of 7 added lines covered. A human should decide the stack order with #932 before sign-off.
- NEEDS-HUMAN [human] — **The T4-batch-review "pass" may have rested on an empty review.** It is a gating row, and its only evidence is one summary line: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 38.65 s (`gate-logs/T4-batch-review.log`). That is three codex passes over a 212 KB diff, straight after a round that found 3 blocking issues. Open `results/issue_933/review-batch.md` and confirm three real passes over the whole diff before counting this row.
- **The timing promise in the docs is stated too weakly** (doc only, no safety effect). `stage.rs:140-144`, like brief R2, says no lease lapses "while each renewal round … finishes inside half a TTL". But an intent runs with no renewal (`stage.rs:1919`, by design), so the intent's time adds to the next renewal's.

  Concrete case (X5): chunk 2's bytes arrive at `T0+HALF−1`, so the renewal is not yet due. Chunk 2's intent takes 2,000 ms (`PassThenAdvance`). The renewal then starts at `T0+6,999` and takes 3,100 ms. At `T0+10,099`, chunk 1's owned lease (`T0+10,001`) is read lapsed. Every step stayed under half a TTL.

  It does no harm in practice: the slot lease, renewed by the intent, is live until `T0+14,999`, and the reaper reads any live lease as live (`0016:2517`). Suggested wording: "each renewal round, plus any intent that started before the renewal fell due, finishes inside half a TTL".
- **A lost owned entry is reported under the wrong name** (low; I found no realistic trigger). When an `sidx:` entry is removed while the slot and session are unchanged, every renewal round conflicts. That costs 4 commits and ends in `Stop::RenewalRoundsSpent` (class `Terminal`) instead of a "lost entry" error (X4). The cause is that `reconcile` compares only the slot (`stage.rs:1798-1806`), whereas the intent path names the same fact `Stop::EntryLost` (`stage.rs:2361`). Only the reaper deletes `sidx:` entries, and it fences the session first, so this is a naming nit, not a defect.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed renewal settlement, deadline races, and fragment-write accounting (`crates/core/src/multipart/stage.rs:1638`, `:1948`, `:2184`), the shared batch-size calculation (`crates/core/src/multipart.rs:5190`), and the accompanying regression tests.

Frozen evidence records CI passing, including all 89 staging tests. The lone mutation survivor at `crates/core/src/multipart/stage.rs:326` is equivalent: `3 + 2N` is always odd, so `> 500` and `>= 500` agree. Diff coverage was not measured; verification was green-only. No tests were rerun or target files changed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Decide whether proposal 0016 needs a separate renewal-budget constraint update — its knob table lacks this tighter bound, and the brief expressly reserves that document decision while excluding the edit here; `brief.md:214`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1468`.
- [x] Validation — fitness-to-purpose — Accept the staging-only handoff and its timing assumptions for the subsequent part commit — deterministic schedules demonstrate the scoped behavior, while production wiring and seeded integration races remain the explicitly tracked later slices; `brief.md:260`, `crates/core/src/multipart/stage.rs:137`, `crates/core/src/multipart/stage.rs:201`.
- [x] **Three changed branches in the renewal race are never run by any leg.** I measured diff coverage with `cargo llvm-cov` over the stage and knob test files, because the gate never measured it.
- [x] **C4-diff-cov has not measured this bundle in two rounds** ("patch.diff does not apply on origin/main", `gate-logs/C4-diff-cov.log`). The bundle is either stale against `main` or stacked on an unmerged #932. Either way, it needs a rebase or refresh before publishing, and the rubric asks that stacked branches be refreshed before review.
- [x] **The T4-batch-review "pass" may have rested on an empty review.** It is a gating row, and its only evidence is one summary line: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 38.65 s (`gate-logs/T4-batch-review.log`). That is three codex passes over a 212 KB diff, straight after a round that found 3 blocking issues. Open `results/issue_933/review-batch.md` and confirm three real passes over the whole diff before counting this row.
- [x] **The liveness promise exceeds the protocol's guarantee.** `brief.md:53-54` requires leases never be observed lapsed while data is supplied, repeated as the invariant at `brief.md:126-128`. However, target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843-1857` explicitly permits a renewal sampled while live to commit after the reaper observes expiry; its remedy is the fence's slot precondition (`:1859-1874`). The peer implementation likewise tests sampled time before awaiting commit (`crates/core/src/metadata.rs:2570-2576`). Timers and continued client progress cannot guarantee uninterrupted durable leases across delayed metadata operations. Qualify the no-lapse criterion with explicit timing assumptions, and specify the expected refusal/settlement behavior when renewal crosses expiry; do not promise that renewal alone prevents every expired observation.
- [x] **R3 assigns incompatible outcomes to the same deadline race.** `brief.md:60-67` requires success for a put answered in time while renewal is committing, and `brief.md:264-266` says to finish that renewal. But `brief.md:266-268` requires abandoning a renewal still committing when the deadline passes. Schedule: the put succeeds before its deadline, renewal remains pending across that deadline, then renewal succeeds. The first rule preserves success; the second returns in doubt. The target seam confirms that a successful put establishes timely publication (`crates/traits/src/lib.rs:1128-1136`). Specify whether the fragment deadline is disarmed once that put succeeds, what bounds the remaining renewal wait, and require a deterministic test with this exact ordering. The two-pending-operations timeout case does not settle it.
- [x] **R7 does not verify the quiesce renewal's required lease coverage.** `brief.md:89-92` tests fresh clock reads, monotonic expiry, lapse refusal and session state; `brief.md:277-279` simply reuses the in-flight rules. The cited target protocol additionally requires the final lease to cover at least the part commit's transaction deadline (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1726-1728`). A lease extended successfully but nearly expired by the time renewal returns satisfies the listed checks without establishing that guarantee. State the API contract or timing bound that gives #927 sufficient remaining lease time, and add a delayed-renewal acceptance test measuring coverage at hand-off. Keep the actual part commit in #927 as scoped.
- [x] size backstop — this slice is behaving oversized: patch is 208 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #933 follow-up: add legs for the three unexercised renewal-race branches (crates/core/src/multipart/stage.rs:1733, :2018, :2024 — adversary scratch legs X1–X3).
- #933 follow-up: fix the timing-promise wording at crates/core/src/multipart/stage.rs:140-144 — "each renewal round, plus any intent that started before the renewal fell due, finishes inside half a TTL".
- #933 follow-up: decide/update proposal 0016 knob table for the tighter renewal-budget bound (docs/design/proposals/draft/0016-multipart-commit-protocol.md:1468).
