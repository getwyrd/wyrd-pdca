# core: UploadPart staging — in-flight renewal of the slot and owned leases (657.1b)

> Second of the two slices #926 (657.1) was split into on 2026-10-22. It builds on 657.1a
> (reserve, owned intent, drain fence, write and body deadlines), which lands in the wave before
> this one. Do reads ONLY this file, plus the one reference implementation it cites. The design is
> normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read
> these before writing code: the `slot:` row of the record table (`0016:349`); the **Part intent**
> row of the batch inventory (`0016:659`), which names renewal's coupling to the slot; the slot
> lease and renewal rules (`0016:1824-1847`); and the quiesce rule (`0016:1720-1733`). Citations
> were verified against `getwyrd/wyrd` `origin/main` @ `4bac538` on 2026-10-22, the plan-review
> revision of the same day included (R2's timing qualification and crossing-expiry leg, R3's
> armed-deadline rule, R7's coverage). 657.1a's own code
> is cited by what it does; lines marked **v3** are in the reference implementation.

- **Slug:** multipart-upload-part-stage-renewal
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.1a a part can be staged safely, but its leases are written only at the
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
- **Success criterion:** New legs **appended** to `crates/core/tests/multipart_upload_part_stage.rs`
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target. The
  timer is fired by hand while a body piece is withheld or a fragment put is held, the clock is
  moved by hand, and the wrapper holds, conflicts and answers unknown on the chosen renewal commit,
  so no leg depends on timing; R2's crossing-expiry leg runs its stand-in reaper read and fence by
  hand around the held renewal, and R7's coverage legs move the clock while the quiesce commit is
  held. Each red is a renewal that does not happen (R2), happens at the wrong
  moment (R2, R3), outlives a deadline (R3), revives a lease (R4), or guesses an outcome (R5). On
  657.1a's base, without this slice, every R2 leg fails (no renewal batch is ever sent) and R8's
  `knob_clamps_hold` leg fails (the adversary's set passes). (R9) is the gating `C4-ci` check. The
  per-fix red is **not** measured by C4-verify for this slice (see Verification posture); it is
  carried by the demonstrated reds.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the staging side, liveness half. While each renewal round
  finishes inside half a TTL and the clocks agree within `δ_clock`, a live request's slot lease and
  owned leases do not lapse while it is supplying data. When a round is slower and an observer reads
  a lease as lapsed, every renewal still rewrites the slot, so a reaper fence pinned on the slot
  bytes it judged cannot land over the renewed request (`0016:1843-1874`; the fence is #625's).
  The quiesce renewal hands the commit a lease that covers its transaction deadline, measured
  after the renewal returned. A lapsed lease is never revived. Renewal never holds staging past a fragment's write
  deadline or a body read's idle bound. An unknown renewal outcome is settled by a re-read or
  reported in doubt, never guessed. Every renewal batch fits `B_ops`, so no part can reach a size
  whose leases cannot be renewed. **Sources:** the maintainer's rule 2026-07-25 (via principles.md
  §5); 0016's slot-lease and renewal rules (`0016:1824-1847`), the slot row (`0016:349`), the
  intent row (`:659`), the quiesce rule (`:1720-1733`) and the write deadline (`:1553-1578`); the
  lapsed-lease rule of #490 (`crates/core/src/write.rs:496-517`); `B_ops` and why a batch past it
  is stuck (`crates/core/src/multipart.rs:5090-5106`). SELF-TEST: it binds the renewal batch, its
  scheduling against every pending wait, the quiesce method and the knob clamp, so guarding one
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 932
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.1a: renewal rewrites the slot and the owned entries 657.1a
  creates, races the deadlines 657.1a's waits enforce, and extends the module, the test file and
  the docs paragraph 657.1a adds. It builds on 657.1a's folded result. #927 (657.2) depends on
  this slice for `StagedAttempt::renew`. **No `Conflicts with` #653:** this slice edits neither
  `crates/core/src/lib.rs` nor `docs/design/architecture/08-crosscutting-concepts.md`. Intake:
  created by the split of #926 under the human's per-id cap override of 2026-10-22 (alpha 7/6
  before it, 9/6 with both children open).
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** in-flight renewal of an attempt's slot and owned leases in `wyrd-core`'s staging
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
- **Repro instruction:** n/a (new functionality). On 657.1a's base, stage a body whose pieces
  arrive more than half a lease TTL apart: no renewal batch is ever sent, and once a lease has
  lapsed, the next intent refuses with a lapse error.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_stage.rs`, **MODIFIED**: this slice
  appends its legs to 657.1a's file and reuses its harness, instead of copying the 44 KB harness
  into a second file. R8's clamp rows go in `crates/core/tests/multipart_knobs.rs`, also modified.
  A `--classify` dry run on a patch shaped like this slice returned no `ADDED_TEST`.
- **Verification posture:** (a) net-new, and **C4-verify cannot measure this slice's red.** With no
  added test file, C4-verify takes its green-only branch and reports `PASS (green-only)`
  (`engine/scripts/run-verify.sh:493-504`). That line is **not** a red→green: it says only that
  the patched tree's `wyrd-core` tests pass. Read it as such at sign-off.
  **Built and exercised at Check:** the renewal batch, its timer race, the quiesce method and the
  clamp, driven by the appended legs over a real store, the deadline-enforcing fake and the
  hand-moved clock and timer.
  **Demonstrated reds (required, recorded in `build-notes.md` with the exact edit and the name of
  the test that failed).** These carry the per-fix red. Do makes each temporary edit, shows the
  named leg fail, then restores the file. Each red must come from the edit its label names:
  - renewal removed entirely (657.1a's behaviour) → every R2 leg;
  - the slot rewrite dropped from the renewal batch → (R1);
  - renewal checked only between delivered pieces, with no timer → (R2) D-i;
  - the timer not raced against a pending fragment put → (R2) D-ii;
  - a renewal sent while the attempt's own intent is in flight → (R2)'s intent-start leg;
  - the renewal awaited before the pending work instead of raced against it → (R3)'s reviewers'
    and adversary's schedules;
  - a renewal still committing at the deadline reported as clean instead of in doubt → (R3);
  - renewal deferring the body-idle bound → (R3)'s idle leg;
  - renewal that moves a lapsed lease forward instead of refusing → (R4);
  - a renewal retry that reuses the first round's clock reading → (R4)'s between-rounds leg;
  - after an unknown renewal outcome, an absent read taken as "did not land" with
    `may_still_commit: true` → (R5);
  - a renewal loop counter that never reaches its bound → (R6);
  - `StagedAttempt::renew` given a fixed `now_millis` → (R7);
  - the post-commit coverage check removed from `renew` → (R7)'s coverage legs (i) and (ii);
  - the renewal awaited past a put's deadline treated as abandoned although the put had answered
    in time → (R3)'s put-then-renewal leg;
  - a renewal that sampled its leases live refused after its commit returned late → (R2)'s
    crossing-expiry leg;
  - the renewal clamp removed from `knob_clamps_hold` → R8's one-past row in
    `multipart_knobs.rs`; and removed from staging's request check → R8's staging leg (two
    edits).
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff` leaves
  alive in this slice's code, each with the test that now kills it or why it is equivalent.
- **Production reach:**
  - (a) The legs exercise renewal for real against a real store. Production stages no part in this
    slice.
  - (b) 657.2 (#927) calls `StagedAttempt::renew` as its quiesce step; the S3 verb lands with #669
    under #508, after #658/#659 and the reaper (#625), per 0016's same-release rule
    (`0016:236-268`). #508 loads the knobs the new clamp judges.
  - (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Reference implementation (the one prior-cycle file you may open):**
    `/home/eddie/wyrd/wyrd-pdca/results/issue_926/iteration-v3/patch.diff`, the third build of the
    parent slice, whose design the v3 sign-off kept. This slice is its renewal machinery:
    v3's `Footprint::renew`, `apply`, `reconcile` and `moved_on`, `RenewalLedger`, `renew_if_due`,
    the renewal arm of `Stager::renewing`, `StagedAttempt::renew`, `RenewOutcome`,
    `RENEWAL_ROUNDS`, `ABANDONED_COMMIT`, and the legs that test them (in its
    `crates/core/src/multipart/stage.rs` and `crates/core/tests/multipart_upload_part_stage.rs`).
    Port them onto 657.1a's module, apply this brief's fixes (the hook that waits for publication,
    the clamp), and do not re-submit v3's renewal unchanged.
  - **Peer for renewal:** `lease_write_chunk` (`crates/core/src/write.rs:485-535`): renew half a
    TTL ahead, and refuse rather than resurrect a lapsed lease (`:496-517`).
  - **Peer for settling an unknown outcome:** #895's Abort,
    `crates/core/src/multipart/abort.rs:282` (keeps a `may_still_commit: true` doubt, `:264-270`).
  - **Peer for the clamp:** `PartCommitOverOps` in `knob_clamps_hold`
    (`crates/core/src/multipart.rs:5385`, `:5453-5454`); `MAX_BATCH_OPS` and why a batch past it
    can never finish (`:5090-5106`); `KnobSet::batch_ops` (`:5308`) and `KnobSet::DEPLOYED`
    (`:5325-5342`).
  - **The trait's unknown-outcome flag:** `CommitUnknownResult::may_still_commit`
    (`crates/traits/src/lib.rs:240-247`).
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/src/write.rs`, `crates/core/tests/`,
  `docs/design/architecture/05-building-block-view.md`) on `origin/main` @ `4bac538`. Merged: the
  knob clamps (#801), the streaming write path's renewal and its lapsed-lease refusal (#490), Abort
  (#909). Rejected or iterated: #657's first attempt renewed only at chunk boundaries and revived a
  lapsed lease; #926's three builds (`iteration-v1` to `-v3`) carried renewal's retry-round,
  signature, deadline-race and test-fake findings, all named in this brief's legs. Open PRs on
  getwyrd/wyrd touching these paths: #931 (#744, `05-building-block-view.md:297` only),
  2026-10-22.
- **Disposition hint:** new-feature

## Design

The reference implementation (v3) is this design. Keep its shape.

**The renewal batch** (`0016:349`, `:659`, `:1824-1847`). One batch, under `require(mpu ==
Open@E)` and `require(slot == prior)`, rewrites the slot with its lease moved forward and every
owned `sidx:` entry with its lease moved forward, each under `require(sidx: == prior)`. Every
lease it writes is strictly later than the slot lease it replaces. A renewal that finds a lease of
its own already lapsed refuses and writes nothing (`0016:1843-1847`). A renewal that fails because
the session left `Open` stops staging and recreates nothing.

**When it runs.** Half a TTL after the last renewal, or after the reservation, measured on the
caller's clock. Before each intent, and never while the attempt's own intent batch is in flight
(both rewrite the slot under `require(slot == prior)`, so they would only fence each other out).
And while a body read or a fragment write is pending: 657.1a already races each of those against
the caller's wait for its deadline; this slice adds the next renewal instant to that race. When the
renewal instant wins, the renewal is sent and the pending read or write keeps being polled: it is
resumed, never dropped or re-sent. The code stays runtime-free (ADR-0009): it spawns nothing.

**Renewal against the deadlines.** A due renewal is raced against the pending work and the
deadline, never awaited first. Work that finishes while the renewal commits is taken (a store
answers `Ok` only for a write published before its deadline); the renewal is then run to its
end, since no other batch of the attempt may be sent while it is in flight. A renewal still
committing when an **armed** deadline passes is abandoned with the work, and its outcome is
reported in doubt, carrying `may_still_commit` (v3's `ABANDONED_COMMIT` backend names it). Renewal
never moves 657.1a's body-idle deadline.

**Which deadlines are armed** (added 2026-10-22, so the two rules above cannot meet on one
schedule). A fragment write deadline is armed while any write it bounds is pending, and disarmed
once every one of them has answered; the body-idle deadline is armed while a body read is pending.
A renewal in flight is raced against the armed deadlines only. If the last armed deadline is
disarmed while the renewal is in flight (its put answered in time), the renewal is awaited to its
end. That wait is still bounded: every store operation terminates, and a commit the store abandons
at its own deadline returns `CommitUnknownResult` (`crates/traits/src/lib.rs:1334-1345`), which R5
settles, within the renewal's round bound.

**Rounds.** Each renewal round, the first and every retry, takes a fresh clock reading and checks
every lease at it before it sends, while still settling a round already sent. A `Conflict` re-reads.
An unknown outcome is settled by re-reading the slot's exact bytes, as 657.1a settles an intent:
equal ⇒ landed; anything else proves "did not land" only with `may_still_commit: false`. Bound the
rounds; if they run out unsettled, return the in-doubt error.

**The quiesce renewal.** Add a public `renew` to the staged attempt, taking the caller's clock and
a coverage `cover_millis`, so 657.2 can run the last renewal before its part commit
(`0016:1720-1733`). Same batch, same rules, same bounds as the in-flight renewal, plus the coverage
check of R7: success only when a clock reading taken after the last commit returned is at least
`cover_millis` before every lease it wrote expires, and the success value reports that reading and
the expiry. 657.2 still owns the part commit and when it calls `renew`.

**The batch-size clamp.** A full renewal batch for `N` owned entries carries `3 + 2N` operations
in v3's shape (the session and slot requires, the slot put, and a require and a put per entry).
Make `knob_clamps_hold` refuse a set whose `max_part_chunks` would put that batch past `batch_ops`,
beside `PartCommitOverOps`, and make staging refuse a request whose chunk bound would, before it
writes anything. Do states the exact count its batch carries and the clamp matches it.

**Docs currency** (`AGENTS.md:154-157`). Extend the paragraph 657.1a added after
`docs/design/architecture/05-building-block-view.md:244`: for as long as the request runs,
including while a body read or a fragment write is pending (on a timer the caller supplies), one
batch moves the slot's lease and every owned lease forward together; that renewal never holds
staging past a pending write's deadline or the body-idle bound, and one still committing at the
deadline is reported as an unknown outcome; every round checks every lease at its own clock reading,
so a lapsed lease is refused, never revived; and no renewal batch can exceed `B_ops`. Do not touch
`:244` itself.

## Alternatives considered

- **Renew only at chunk boundaries, or between delivered pieces.** A slow client that has not filled
  its first chunk then holds a slot whose lease lapses, and a pending read or write can outlast the
  lease with nothing to wake the renewal (`0016:1824-1837`). Rejected; renewal is on the caller's
  timer.
- **Await a due renewal before resuming the pending work.** A slow renewal commit then holds staging
  past a write deadline and can discard a write answered in time (the v2 finding). Rejected;
  race them.
- **Split a large renewal across several batches.** The slot and the owned leases must move together
  (`0016:349`, `:659`); two batches would let one land without the other. Rejected; clamp the size.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-22): all three findings hold at `4bac538` and are revised in place.
(1) The no-lapse promise now states its timing assumptions: each renewal round finishes inside
half a TTL, and the clocks agree within `δ_clock`. A renewal that sampled live leases and lands
after an observer read them lapsed is taken; it is defeated as a reaper threat by the slot rewrite
the fence pins (`0016:1843-1874`). New R2 leg: a held renewal, a stand-in reaper read and fence;
the fence conflicts. (2) R3 now says which deadlines are armed. A write deadline stops
constraining the renewal once its writes have answered, and the remaining wait is bounded by the
store's own deadline and the round bound. New R3 leg runs the exact order: put `Ok` in time,
clock past its deadline with the renewal held, then the renewal lands; staging succeeds.
(3) `StagedAttempt::renew` takes `cover_millis` and succeeds only when, at a reading taken after
its last commit returned, every lease covers it. It reports that reading and expiry for #927.
New R7 legs: a delayed quiesce commit, every round delayed, and a cover not below the TTL. The
part commit stays in #927.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: On a failed or refused renewal in write_fragments, settle pending and already-reported-unknown fragment writes the way the deadline path does (Indeterminate, keeping the renewal's cause/doubt) instead of returning early via `?` at crates/core/src/multipart/stage.rs:2077 (also the refusal escape at :1457). The current code reports a determinate Terminal error while a dropped put can still publish, breaking stage.rs:46-56 and the docs at stage.rs:121-124 and 05-building-block-view.md:246. Add regression legs with a held put for the three adversary cases (renewal Act::Fail, LeaseLapsed, Abort-refusal), plus one where a WriteEffect::Unknown answer was already collected before the renewal fails. State the refusal-with-write-in-flight behaviour explicitly. Name every edited 657.1a leg in build-notes.md (max_part_chunks fixture change, the IDLE->HALF waits() edits, the 4xTTL lease in a_double_prime_the_idle_bound_is_per_read_not_per_body). The rest of the design (renewal batch, per-round reads, settling, clamp) held up under adversarial review; keep it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  On a failed or refused renewal in write_fragments, settle pending and already-reported-unknown fragment writes the way the deadline path does (Indeterminate, keeping the renewal's cause/doubt) instead of returning early via `?` at crates/core/src/multipart/stage.rs:2077 (also the refusal escape at :1457). The current code reports a determinate Terminal error while a dropped put can still publish, breaking stage.rs:46-56 and the docs at stage.rs:121-124 and 05-building-block-view.md:246.
  Add regression legs with a held put for the three adversary cases (renewal Act::Fail, LeaseLapsed, Abort-refusal), plus one where a WriteEffect::Unknown answer was already collected before the renewal fails. State the refusal-with-write-in-flight behaviour explicitly.
  Name every edited 657.1a leg in build-notes.md (max_part_chunks fixture change, the IDLE->HALF waits() edits, the 4xTTL lease in a_double_prime_the_idle_bound_is_per_read_not_per_body). The rest of the design (renewal batch, per-round reads, settling, clamp) held up under adversarial review; keep it.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 64 mutants tested in 8m: 1 missed, 51 caught, 12 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_933/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
