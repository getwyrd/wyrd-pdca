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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 64 mutants tested in 8m: 1 missed, 51 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_933/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.79s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #933: add in-flight and quiesce renewal of UploadPart staging leases; one reproduced loss of fragment-write uncertainty needs builder follow-up.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract distinguishes live-request renewal, armed deadlines, expiry refusal, and commit coverage, with explicit timing assumptions and release boundaries (`brief.md:36`, `brief.md:55`, `brief.md:118`, `brief.md:199`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, the withheld-body probe timed out waiting for renewal and the 249-chunk-bound probe incorrectly staged; both compiled and failed behaviorally (`reviewer-red.log:7`, `reviewer-red.log:17`). |
| C3 Change | PASS | The five changed files stay within the staging, clamp, regression-test, and living-doc scope; the target matches the supplied patch and was restored byte-for-byte after the red run (`crates/core/src/multipart/stage.rs:550`, `crates/core/src/multipart.rs:5539`, `docs/design/architecture/05-building-block-view.md:246`). |
| C4 Verification (red→green) | PASS | Both independent baseline failures pass with the patch restored, and all 87 staging plus 8 knob tests pass; this verifies those cases, while the additional failing counterexample is recorded under C5/T3 (`reviewer-red-green-restored.log:12`, `reviewer-green.log:187`, `reviewer-green.log:280`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve pending and collected fragment-write uncertainty when renewal fails, and cover that failure path — a definite renewal error currently overrides writes that can still publish (`crates/core/src/multipart/stage.rs:2077`, `reviewer-probes-green.log:10`). |
| T1 Structure | PASS | The caller-owned clock and narrow metadata/chunk-store seams preserve dependency direction; renewal spawns no helper task or shared global state (`crates/core/src/multipart/stage.rs:179`, `crates/core/src/multipart/stage.rs:329`, `crates/core/src/multipart/stage.rs:1845`). |
| T2 Shape | PASS | Renewal uses one bounded batch and shared request/knob operation arithmetic; the public hand-off and architecture description cover the new behavior (`crates/core/src/multipart/stage.rs:1549`, `crates/core/src/multipart.rs:5190`, `docs/design/architecture/05-building-block-view.md:246`). |
| T3 Runtime | FAIL | A fragment published after staging returned Terminal/LeaseLapsed, proving the early renewal-error return bypasses the required indeterminate-write accounting (`crates/core/src/multipart/stage.rs:2077`, `crates/core/src/multipart/stage.rs:2089`, `reviewer-probes-green.log:10`). |
| T4 Contribution | FAIL | The batch-review gate's three entries reduce to the same independently reproduced, unresolved defect; its disposition remains owed. Contribution-artifact auditing is separately N/A until publish (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the recorded affected-path audit covers merged and closed/rejected work — the supplied target has only a synthetic base commit and no remotes, and the path API returned no history to corroborate that audit (`brief.md:287`, `reviewer-prior-art.log:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this staging-only slice meets the intended hand-off under the stated timing assumptions — #927 still owns the part commit and #669 owns production integration, so passing staging tests alone cannot establish end-to-end UploadPart fitness (`brief.md:55`, `brief.md:260`, `crates/core/src/multipart/stage.rs:16`). |

One implementation finding is confirmed: **retain fragment outcomes on every renewal-error exit**, including a renewal refusal. At `crates/core/src/multipart/stage.rs:2077`, `await?` returns before processing `answers` and `unanswered`. The returned renewal error therefore discards both pending puts and any already collected unknown-effect reports. A refusal can likewise escape through `crates/core/src/multipart/stage.rs:1457`. Position coverage by durable owned entries does not establish whether those writes happened; the existing error contract requires indeterminate classification for unverified writes (`crates/core/src/multipart/stage.rs:53`, `crates/core/src/multipart/stage.rs:587`).

The independent counterexample uses the supplied Redb/scripted-store/deadline-enforcing Fleet harness, without changing production source. Hold chunk 1, fragment 0 at arrival; use a 10,000 ms lease and a 20,000 ms write window; advance from 1,000,000 to 1,010,002 ms and fire the timer. Renewal refuses the slot that expired at 1,010,001 ms. Staging returns `Stopped { LeaseLapsed }`, classified `Terminal`. Release the held put after that return: it publishes and answers `Ok` before the 1,020,000 ms write deadline. The assertion requiring `Indeterminate` fails (`reviewer-probes-green.log:10`). The fix should aggregate fragment outcomes even when renewal returns an error/refusal, preserve any metadata-commit doubt, and add regression coverage for both pending writes and previously collected unknown replies. This is the single defect represented by all three batch-review entries, not three separate findings.

The retained reproducer is runnable from this report's directory:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-933-build" TMPDIR="$PWD" \
  cargo test --manifest-path pdca-reviewer-933-probes/Cargo.toml --offline \
  reviewer_pending_write_must_survive_renewal_failure_accounting -- --nocapture
```

Verification evidence supports the following limits:

- **Independent execution:** 87 staging tests and 8 knob tests passed. Two additional probes failed against the stashed baseline and passed after restoration: renewal during a withheld body read and rejection of a 249-chunk request bound. The baseline probe fixture only adapts the clock trait implementation to the pre-fix API; production code was changed only by stash/pop. The new failure-accounting probe remains red on the patch. `git diff --check` and reverse-application checking passed, and the restored diff's SHA-256 is `dcf0eb7ad8e0fc9160128239d144c54904b41a25ff0ee23eb112f0784f79261b`.
- **C4-ci and C4-verify:** the frozen CI log shows fmt, clippy, builds, tests, dependency checks, conformance, and DST completing successfully (`gate-logs/C4-ci.log:30`, `gate-logs/C4-ci.log:3836`, `gate-logs/C4-ci.log:4469`). Local fmt/clippy attempts could not run because those rustup components are absent (`reviewer-fmt.log:1`, `reviewer-clippy.log:1`); this is a reviewer-host limitation, not a patch defect. C4-verify explicitly supplies green-only evidence (`gate-logs/C4-verify.log:151`), supplemented by the independent red→green above.
- **Declared external dependencies:** `typos`, docs lint, and the renderer were exercised successfully here; the renderer produced 99 pages and passed its link audit (`reviewer-docs-lint.log:1`, `reviewer-docs-render.log:2`). The frozen log also shows both declared dependencies executing (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:16`). No unmet declared dependency is inferred from the reviewer's missing fmt/clippy components.
- **C4-diff-cov:** coverage was not measured because the wrapper could not apply the patch to its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied target accepts the exact patch and compiles; with the stated #932 prerequisite (`brief.md:188`), the wrapper's base mismatch is an ordering/target-state caveat, not evidence of a compilation defect.
- **C5-mutants:** the log reports 51 caught, 12 unviable, and one missed mutant (`gate-logs/C5-mutants.log:13`). The survivor changes `>` to `>=` in the request clamp (`crates/core/src/multipart/stage.rs:315`). It is equivalent at the current fixed limit: `3 + 2N` is odd for every `u32` N, cannot equal 500, and is computed without overflow in `u64` (`crates/core/src/multipart.rs:5190`, `crates/core/src/multipart.rs:5171`). It does not warrant another finding.
- **T4-contribution: N/A.** The PR-description/commit artifacts are absent by design; their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). **host-tikv: PASS on frozen evidence:** the captured command includes both requested feature checks and completes successfully (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

The standing rubric was applied without reopening tracked deferrals. The new checks enforce lease authority and batch limits; they are not capability probes masking a load-time side effect. Seeded DST races remain explicitly deferred to #660/#665 (`crates/core/src/multipart/stage.rs:187`), and no production caller was found. The prior-art investigation found one synthetic local base commit, no configured remote, and no commit records from the affected-path API query; `brief.md:287` records a path-based audit of merged and rejected/iterated work, but independent historical corroboration remains a human decision. Repository citations above are grounded in `$PDCA_TARGET`; evidence-log and probe citations refer to this review sandbox. No builder rationale was consulted and no fix was applied.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #933 (657.1b, UploadPart staging renewal)

Scope: `patch.diff` against the patched tree at `$PDCA_TARGET`. I re-ran parts of the proof in a
scratch copy (`cargo test -p wyrd-core --test multipart_upload_part_stage`, toolchain present), added one
probe leg, and made two temporary mutations. Overall: the renewal batch, the round logic, the
quiesce `renew` and the clamp held up, but I broke one path with a concrete test.

## Refuted

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:2075-2077` (with `:1925-1928`): a renewal that **fails** while a chunk's fragment writes are still pending returns through `?`. That drops `pending`, `answers` and `unanswered` with no accounting. Staging then answers a determinate error (or a refusal) while a put is still in flight. This breaks 657.1a's own rule at `stage.rs:46-56` ("an abandoned write ... `ErrorClass::Indeterminate`, however the timing fell"). It also makes two docs claims false: the module doc at `stage.rs:121-124` ("writes pending beside it are abandoned, **as at their deadline**"; at the deadline they become `WriteEffectUnknown`, here they don't), and `docs/design/architecture/05-building-block-view.md:246` ("a write still pending then ... the error that stops staging is classed indeterminate in either case"). **Concrete failing case**, reproduced with a probe leg over the patch's own harness. Hold `FragmentId{chunk:1,index:0}` at `HoldAt::Arrival`, use `write_window_millis = WINDOW`, advance the clock by `HALF`, and fire the timer. Then:
  - (a) Renewal scripted `Act::Fail` → `Err(Stopped{cause: Store(..)})`, `classify` = **Terminal**, and the error text says "no batch of this part attempt is in doubt".
  - (b) `write_window_millis = TTL+1_000`, clock `+TTL+1` → `Stopped{LeaseLapsed{slot..}}`, **Terminal**.
  - (c) Renewal scripted `Act::First(Side::Abort)` → `Ok(Refused(NoSuchUpload))`.

  In all three runs, releasing the held put **after staging returned** stores the fragment (`Event::Stored`). Write answers already collected, such as a `WriteEffect::Unknown` report, are lost the same way when the renewal fails after the writes answered. No appended leg covers a failing renewal beside a pending put: the R4 legs at `multipart_upload_part_stage.rs:5530-5748` all fail the renewal during a **body read**, where dropping the work has no side effect. Fix direction: on a renewal `Halt` in `write_fragments`, settle the writes the way the deadline path does. Fold pending and unknown writes into an Indeterminate stop, keeping the renewal's cause or doubt. Add a leg for (a)–(c). The T4 batch review raised the same defect 3/3 as blocking. This run confirms it with a reproduction rather than reading. Case (c) is the weakest: a refusal with a write in flight may be acceptable since the session is gone. The builder should decide that explicitly and state it, not leave it implicit.

## Attempted, could not refute

- `stage.rs:1549-1560` (R1): with the slot put removed from `renewal_batch`, 8/8 R1/R2 legs go red. `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` fails for the **right** reason: the stand-in fence answers `Committed`, not `Conflict`, at `multipart_upload_part_stage.rs:5012`. So the crossing-expiry leg is not a tautology.
- `stage.rs:1602` (R4/R7 per-round reading): with one reading reused across rounds, `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds` and three R7 legs go red. The "fresh reading every round" claim is real.
- `stage.rs:1626-1681` (R5 settling): I looked for an ABA or wrong-adoption case. A renewal's slot value is strictly later than every slot value that landed before it (`next_expiry`, `:1500-1504`), rounds with equal slot values put identical bytes everywhere, and a landed round fences every other round's `require(slot == prior)`. I found no input where a non-landed round is adopted or a landed one is retried.
- `stage.rs:1007-1029`, `:1884-1927` (R3 abandoned-renewal doubt): every await point at which the renewal can be dropped leaves the ledger correct. `awaiting` is set only around `commit`, and a `Committed` answer reaches `apply` with no await in between. The four-run leg at `multipart_upload_part_stage.rs:5357` pins each case.
- R8 clamp: `stage.rs:315` / `multipart.rs:5539-5542` match the batch's real `3 + 2N` count, and `renewals()` checks that count on every batch sent. The single C5 survivor (`stage.rs:315:57`, `>` → `>=`) is an **equivalent** mutant. `3 + 2N` is always odd and `MAX_BATCH_OPS = DEADLINE_OPS/2 = 500` (`multipart.rs:5158`, `:5171`) is even, so the two can never be equal. Not a test gap. It belongs in `build-notes.md`, or in `.cargo/mutants.toml`'s `exclude_re` with that reason.

## Claims to read with care at sign-off (not refutations)

- `check-gates.json` C4-verify "pass": as the brief says, this is green-only, not a red→green. The two reds I reproduced above are real, but I did not check the other ~16 demonstrated reds `build-notes.md` should record (withheld from me).
- C4-diff-cov "fail": not measured, because the bundle does not apply on `origin/main` (stacked on 657.1a, #932). The claim that every changed line is executed is unverified. C5 (51/52 viable caught) partly makes up for it. The branch needs refreshing onto #932's merged form before merge, per the rubric's stacked-branch rule.
- Edits to 657.1a's legs beyond "asserted the absence of renewal": the shared fixture's `max_part_chunks` changes `1_000` → `MAX_PART_CHUNKS` (`multipart_upload_part_stage.rs:236`), forced by the new R8 refusal. Three `waits()` assertions change `IDLE` → `HALF`, and `a_double_prime_the_idle_bound_is_per_read_not_per_body` now uses a 4×TTL lease so no renewal runs in it (`:2342`). All are justified, but the brief requires `build-notes.md` to name each one. I could not check that.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:2077`: The new `.await?` returns on renewal failure before accounting for fragment writes. Hold one dispatched put, fire the renewal timer, and have all four renewal commits return `Conflict`: staging returns `RenewalRoundsSpent` (classified `Terminal`) while the dropped put may still publish. A renewal refusal can similarly return `NoSuchUpload`, and previously collected unknown-effect write reports are also discarded. Handle the renewal result after collecting fragment outcomes; preserve unknown write effects as `Indeterminate` together with any metadata-commit doubt. Add regression coverage for renewal failure with a pending put and with an already-reported unknown write effect. This confirms the single underlying defect repeated in the frozen T4 log.

No additional actionable reuse, simplification, or efficiency findings. The C5 survivor at `crates/core/src/multipart/stage.rs:315` is equivalent: `3 + 2N` is always odd, while `MAX_BATCH_OPS` is 500, so changing `>` to `>=` cannot change this check's result.

Reviewed the target source and frozen gate evidence; no tests were rerun and no target files were changed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve pending and collected fragment-write uncertainty when renewal fails, and cover that failure path — a definite renewal error currently overrides writes that can still publish (`crates/core/src/multipart/stage.rs:2077`, `reviewer-probes-green.log:10`).
- [x] T5 Judgment — Confirm the recorded affected-path audit covers merged and closed/rejected work — the supplied target has only a synthetic base commit and no remotes, and the path API returned no history to corroborate that audit (`brief.md:287`, `reviewer-prior-art.log:1`).
- [x] Validation — fitness-to-purpose — Decide whether this staging-only slice meets the intended hand-off under the stated timing assumptions — #927 still owns the part commit and #669 owns production integration, so passing staging tests alone cannot establish end-to-end UploadPart fitness (`brief.md:55`, `brief.md:260`, `crates/core/src/multipart/stage.rs:16`).
- [ ] `crates/core/src/multipart/stage.rs:2075-2077` (with `:1925-1928`): a renewal that **fails** while a chunk's fragment writes are still pending returns through `?`. That drops `pending`, `answers` and `unanswered` with no accounting. Staging then answers a determinate error (or a refusal) while a put is still in flight. This breaks 657.1a's own rule at `stage.rs:46-56` ("an abandoned write ... `ErrorClass::Indeterminate`, however the timing fell"). It also makes two docs claims false: the module doc at `stage.rs:121-124` ("writes pending beside it are abandoned, **as at their deadline**"; at the deadline they become `WriteEffectUnknown`, here they don't), and `docs/design/architecture/05-building-block-view.md:246` ("a write still pending then ... the error that stops staging is classed indeterminate in either case"). **Concrete failing case**, reproduced with a probe leg over the patch's own harness. Hold `FragmentId{chunk:1,index:0}` at `HoldAt::Arrival`, use `write_window_millis = WINDOW`, advance the clock by `HALF`, and fire the timer. Then:
- [ ] `crates/core/src/multipart/stage.rs:2077`: The new `.await?` returns on renewal failure before accounting for fragment writes. Hold one dispatched put, fire the renewal timer, and have all four renewal commits return `Conflict`: staging returns `RenewalRoundsSpent` (classified `Terminal`) while the dropped put may still publish. A renewal refusal can similarly return `NoSuchUpload`, and previously collected unknown-effect write reports are also discarded. Handle the renewal result after collecting fragment outcomes; preserve unknown write effects as `Indeterminate` together with any metadata-commit doubt. Add regression coverage for renewal failure with a pending put and with an already-reported unknown write effect. This confirms the single underlying defect repeated in the frozen T4 log.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_933/review-b
- [x] **The liveness promise exceeds the protocol's guarantee.** `brief.md:53-54` requires leases never be observed lapsed while data is supplied, repeated as the invariant at `brief.md:126-128`. However, target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843-1857` explicitly permits a renewal sampled while live to commit after the reaper observes expiry; its remedy is the fence's slot precondition (`:1859-1874`). The peer implementation likewise tests sampled time before awaiting commit (`crates/core/src/metadata.rs:2570-2576`). Timers and continued client progress cannot guarantee uninterrupted durable leases across delayed metadata operations. Qualify the no-lapse criterion with explicit timing assumptions, and specify the expected refusal/settlement behavior when renewal crosses expiry; do not promise that renewal alone prevents every expired observation.
- [x] **R3 assigns incompatible outcomes to the same deadline race.** `brief.md:60-67` requires success for a put answered in time while renewal is committing, and `brief.md:264-266` says to finish that renewal. But `brief.md:266-268` requires abandoning a renewal still committing when the deadline passes. Schedule: the put succeeds before its deadline, renewal remains pending across that deadline, then renewal succeeds. The first rule preserves success; the second returns in doubt. The target seam confirms that a successful put establishes timely publication (`crates/traits/src/lib.rs:1128-1136`). Specify whether the fragment deadline is disarmed once that put succeeds, what bounds the remaining renewal wait, and require a deterministic test with this exact ordering. The two-pending-operations timeout case does not settle it.
- [x] **R7 does not verify the quiesce renewal's required lease coverage.** `brief.md:89-92` tests fresh clock reads, monotonic expiry, lapse refusal and session state; `brief.md:277-279` simply reuses the in-flight rules. The cited target protocol additionally requires the final lease to cover at least the part commit's transaction deadline (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1726-1728`). A lease extended successfully but nearly expired by the time renewal returns satisfies the listed checks without establishing that guarantee. State the API contract or timing bound that gives #927 sufficient remaining lease time, and add a delayed-renewal acceptance test measuring coverage at hand-off. Keep the actual part commit in #927 as scoped.
- [ ] size backstop — this slice is behaving oversized: patch is 179 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): On a failed or refused renewal in write_fragments, settle pending and already-reported-unknown fragment writes the way the deadline path does (Indeterminate, keeping the renewal's cause/doubt) instead of returning early via `?` at crates/core/src/multipart/stage.rs:2077 (also the refusal escape at :1457). The current code reports a determinate Terminal error while a dropped put can still publish, breaking stage.rs:46-56 and the docs at stage.rs:121-124 and 05-building-block-view.md:246. Add regression legs with a held put for the three adversary cases (renewal Act::Fail, LeaseLapsed, Abort-refusal), plus one where a WriteEffect::Unknown answer was already collected before the renewal fails. State the refusal-with-write-in-flight behaviour explicitly. Name every edited 657.1a leg in build-notes.md (max_part_chunks fixture change, the IDLE->HALF waits() edits, the 4xTTL lease in a_double_prime_the_idle_bound_is_per_read_not_per_body). The rest of the design (renewal batch, per-round reads, settling, clamp) held up under adversarial review; keep it.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
