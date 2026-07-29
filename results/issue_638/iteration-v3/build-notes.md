# Build notes — issue #638, fragment-write authorization deadline (`W_write`)

*Iteration 3.* Withheld from the reviewer; written for the human at sign-off.
All `path:line` citations are against `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt-l1`, base `b0cd199` = `origin/main`;
`origin/pdca-integration/main` does not exist in this run — no wave-0 fold happened — so
`run-verify.sh` resolved the brief's own base, `origin/main`, which is the same commit).

---

## 1. What this iteration changed, and why

The carry-forward named three implementation findings. The **first two are the same
defect**, and it is the one this rebuild is about:

> C5 / T3 — *"Close the interval between the last clock read and actual publication — the
> check precedes the potentially blocking rename, so clock skew/resolution margin cannot
> bound I/O and `W_write` is not yet an end-to-end effect bound."*
> T5 — *"Add a regression that advances time across the publication syscall, not merely
> before it."*

Iteration 2 judged the deadline **immediately before** `fs::rename`. The reviewer is right
that this is not the bound the invariant asks for: `rename` is itself a syscall that can
block arbitrarily (a stalled disk, a frozen NFS mount, a blocking-pool preemption), so a
write judged live at the check can still become visible long after `W_write`. `δ_clock` in
`G_orphan > W_write + δ_clock` (`0016:1478`) covers clock **resolution and skew** between
the two evaluation sites — it was never a budget for unbounded I/O.

**The fix: publish, then verify, and retract a publication that landed late.** No user-space
code can make "read the clock" and "rename" one atomic step, so the store instead makes the
*effect* undoable:

* `crates/chunkstore-fs/src/lib.rs:222-235` — **admission refusal**: a write already past
  its deadline on arrival is refused before any disk work (no chunk directory, no scratch).
  Explicitly documented as *not* the bound.
* `crates/chunkstore-fs/src/lib.rs:296-299` — the publishing `fs::rename`, unchanged.
* `crates/chunkstore-fs/src/lib.rs:301-341` — **the bound**: the clock is read *after* the
  rename returns. If the publication landed past the deadline, the write is **retracted**
  (`retract_publication`, `:539`), leaving the store as it would have been had the write
  never happened, and the caller gets the typed refusal.

That is what makes the brief's invariant true as stated — *"a durable write that has been
authorized must either take effect within a bounded, enforced window of its authorization or
never take effect at all"* — rather than true only up to the last check before the effect.

Three consequences fall out, each of which was also a review finding or a correctness
question I had to answer rather than dodge:

1. **A failed retraction is not a deadline refusal.** `WriteDeadlineExpired` asserts *the
   write did not take effect*; after a failed `remove_file` the bytes may still be on disk.
   So the removal error is propagated as a new, distinct backend fault,
   `FsChunkStoreError::RetractionFailed` (`:471-478`, Display at `:491-503`, `source` at
   `:512`), which classifies `Terminal` and maps to `INTERNAL` on the wire — never
   `FAILED_PRECONDITION`. Pinned by `crates/chunkstore-fs/src/lib.rs:668-712` (and the guard by `:718-727`).
   This also settles the iteration-2 review's *"ignoring a non-`NotFound` scratch-removal
   failure returns a definitive deadline refusal while leaving `.tmp` litter"* (seen by
   three passes): the deadline path no longer removes scratch at all — by the time the
   deadline is judged, the scratch has been renamed away — and the one removal it does
   perform is checked, not swallowed. `NotFound` remains success, because "the fragment is
   gone" *is* the state the retraction wanted (a concurrent `delete_fragment`/GC reclaim).
2. **The retraction is attributable.** A straggler must not delete a fragment an *earlier,
   live* write (or an idempotent retry) was already acknowledged for. `is_published`
   (`:527`) is read once, just before the rename, and the retraction is skipped when the
   fragment was already there — for a content-addressed id the bytes are identical, so our
   rename changed nothing observable and there is nothing of ours to undo. Without this
   guard the deadline mechanism would be a data-loss mechanism; the honest residual (a
   concurrent same-id publication landing between that `stat` and our `rename`) is
   documented in place at `:284-293` and costs a repairable duplicate, never a late write
   left in effect.
3. **The contract moved with the code.** `crates/traits/src/lib.rs:683-706` now states the
   post-publication judgment, the retraction, and the "a failed retraction must not be
   reported as `WriteDeadlineExpired`" rule as seam obligations, so a future backend (S3)
   implements the property, not this store's mechanics. `WriteDeadlineExpired`'s own doc
   (`:552-575`) says what receiving it *promises*: a definite "not applied".

The third finding was about the DST leg:

> CONVENTION — *"The simulated delayed write is canceled with its RPC future, unlike
> production `spawn_blocking` work that continues after client cancellation, so the Tier-0
> model cannot reproduce the required late-write-after-timeout behavior."*

Also right, and it was the more interesting one, because 0016's row (`0016:1784`) stages the
scenario with *"the caller has long since timed out"* — i.e. precisely a write no longer
attached to a caller. Iteration 2 parked the write **inside** the request future, so a
caller that actually gave up would have cancelled the very thing under test; it had to keep
the caller waiting, which weakens the scenario to "a slow server". Rebuilt at
`crates/dst/tests/network.rs:568-608`:

* the parked write runs on a **spawned, detached** task (`madsim::task::spawn`, `:586`;
  madsim's `JoinHandle::drop` detaches exactly as tokio's does —
  `madsim-0.2.34/src/sim/task/join.rs:73-79`), mirroring production, where the D server's
  store hands the work to `spawn_blocking` (`crates/chunkstore-fs/src/lib.rs:246`) and a
  cancelled tonic handler leaves that closure running on to its publish point;
* behind it is the **real production `FsChunkStore`**, not a fake that re-implements the
  check — madsim virtualises `SystemTime` (`madsim-0.2.34/src/sim/time/system_time.rs:39-70`
  overrides `clock_gettime`), so the store's own `SystemClock` reads the simulator's clock
  and the whole enforcement under test is production code;
* the caller **gives up after 500 ms** on a write parked 2 s past a 200 ms deadline
  (`:696-707`), and the assertion is made on what the D server holds afterwards.

The in-memory `DStore` fake consequently stopped modelling a queue (`:87-152`): its
publication is one map insert under its own lock, so a single judgment there *is* the
contract's requirement for it, and it explicitly no longer stands in for production
enforcement.

## 2. Decisions the brief asked me to state

**Deadline vs authorization instant on the wire (0016 open question 1).** The **deadline**
travels (`crates/proto/proto/wyrd/v0/chunk.proto:29-45`). Rationale: the receiver then needs
no knowledge of the sender's `W_write`, which is #625's knob and will differ per deployment
and per caller class (an ordinary write, a multipart staged write, a rebalance). Sending the
instant would make every D server a policy site that must be reconfigured in lockstep with
the gateway; sending the deadline keeps the D server "deliberately dumb" (architecture §5,
ADR-0010) — it compares two numbers. The cost is that the *authorizer's* `W_write` is not
auditable from the request alone; the refusal carries both readings (the deadline and the
acceptor's clock, `crates/traits/src/lib.rs:610-618`) so the audit trail still shows the two
evaluation sites `δ_clock` spans.

**Which lifecycle owns the clock read.** The **applying store**. `FsChunkStore` is generic
over the testkit `Clock` seam (`crates/chunkstore-fs/src/lib.rs:38-53`, ADR-0024, the seam
AGENTS.md prescribes over a bare `SystemTime::now()`, #619), defaulting to `SystemClock`;
both reads in one `put_fragment` come from that one source (`Arc<C>` carried into the
blocking closure, `:243-245`), so the admission check and the bound can never be judged
against two different epochs (the #557/#565 defect class). The seam crate itself owns no
clock: `WriteDeadlineExpired::if_elapsed(id, deadline, now)` takes the reading
(`crates/traits/src/lib.rs:610`).

**How existing callsites were migrated (leg D).** The trait change is source-breaking by
construction; every callsite passes `None`, which is exactly today's unbounded behaviour:
production writers at `crates/core/src/write.rs:248` and `:447` (the ordinary write path and
the intent-then-write path), `crates/custodian/src/reconstruction.rs:556` and
`crates/custodian/src/rebalance.rs:269` (repair and evacuation), the gRPC client/server/
fan-out forwarding the field unchanged, plus the test doubles across 40 test files. Choosing
a deadline for those callers is #625's and #636's business, not this slice's. The wire is
additive: leg D asserts a request with **no** field 3 stores exactly as before, over the
wire, in the added test file.

**Where enforcement deliberately is *not*.** Not in the gRPC handler
(`crates/chunkstore-grpc/src/server.rs:72-80` says why: a handler check bounds acceptance,
and the handler's future can be dropped while the store's blocking write runs on), and not
in the fan-out wrapper (`crates/chunkstore-grpc/src/fanout.rs:78-95`: still caller-side; a
second gateway or a retry reaches the service directly).

## 3. Alternatives considered — and what each would have cost

* **Keep the pre-rename check and call `δ_clock` the margin** (iteration 2). Rejected: it is
  the finding. A pre-publication check bounds when the store *decided* to publish. Diff
  cost: 0 lines — and that is the point; it is cheaper and wrong.
* **Pre-rename check *and* post-publication verification.** I kept only admission +
  post-publication. A third check is ~8 lines and would spare a rare late write one
  `rename`+`unlink` pair, but it is behaviourally invisible: `cargo mutants` would report it
  as a surviving mutant (deleting it changes nothing observable), and every check that
  cannot fail a test is a line a future reader must reason about. The two that remain are
  each pinned by a test that goes red without them
  (`conformance.rs:266` for admission, `:423` for the bound).
* **A per-fragment publish lock inside the store, so check→rename→verify→retract is atomic
  against concurrent same-id writes.** This would close the `is_published` stat/rename race
  completely. Concretely it is a `Mutex<HashMap<FragmentId, Arc<Mutex<()>>>>` plus
  acquire/release/evict logic — ~35-45 lines of new shared mutable state in a store whose
  #203 design deliberately removed same-id contention (each write gets a private scratch
  path so concurrent writers never serialise). It would also serialise every concurrent
  same-id write in the steady state to protect an exceptional path. Rejected as
  disproportionate: the race needs a *concurrent* write of the *same* fragment landing
  inside a sub-syscall window, and its worst outcome is one byte-identical fragment removed
  — a missing-fragment repair the custodian already performs (`scrub` → `reconstruction`) —
  whereas the failure this slice exists to exclude is a late write left durable and
  unevidenced. Recorded in code at `crates/chunkstore-fs/src/lib.rs:284-293` so a maintainer
  can overrule it with the facts in hand.
* **Publish with `hard_link` (EEXIST ⇒ "already published") instead of `rename`**, which
  would make "did *I* create this entry?" atomic. Rejected on correctness, not cost: a
  repair rewriting a bit-rotted fragment relies on the rename overwriting the corrupt file;
  `hard_link` would silently keep the corrupt bytes (`crates/custodian/src/scrub.rs` →
  `reconstruction` re-put path). One line of change, an entire repair path broken.
* **fsync the chunk directory after the retraction.** Not done: the store does not fsync the
  publication either (`rename` is its publish point, and nothing in the current durability
  model claims otherwise). Adding a sync to the undo but not the do would be an asymmetric
  half-measure; making both durable is a separate, larger durability change.
* **Deriving the deadline server-side from a lease lookup** — 0016 already rejects it
  (metadata read on the hot data path; couples the chunk store to the metadata plane it is
  independent of, ADR-0010).
* **A client-side cancel at `deadline_millis`.** Standing rejection, re-recorded in
  `review-rejected.md` and in the code at `crates/chunkstore-grpc/src/client.rs:230-246`: it
  duplicates the composition-level channel timeout (`crates/server/src/cli.rs:1441`), it
  would destroy the definite "not applied" verdict this field exists to deliver (tonic
  renders a channel-deadline cancel as `CANCELLED`, which the seam classifies *transient*),
  and it would not stop a late landing anyway.

## 4. Refutation — the three forced questions

**(a) Genuine red?** Yes, four separate refutations, each run:

| What was reverted/mutated | Result |
|---|---|
| The whole production change (C4-verify's RED leg, `engine/scripts/run-verify.sh`) | **5 tests ran, 3 failed by assertion**, 2 passed → `PASS — red without the fix, green with it` |
| Enforcement moved back to iteration 2's pre-rename placement | `a_write_published_past_its_deadline_is_retracted_not_left_in_place` **FAILED** ("must be refused — a check that precedes the rename cannot bound the rename itself") |
| `if !published_over_existing` → `if true` (always retract) | `a_late_duplicate_does_not_retract_an_already_published_fragment` **FAILED** (the first, live write's fragment was destroyed) |
| DST D server ignores the field (`put_fragment(.., None)`) — i.e. a pre-#638 server | `a_write_parked_past_its_deadline_is_refused_by_the_real_d_server` **FAILED** ("the parked write landed after its authorization deadline and MUST have been refused") |

The RED leg's three failures were **assertions, not build errors** — each is an
`expect_err(...)` that received `Ok(())` because the base server stored the write
(`write_deadline.rs:392`, `:433`, `:538`); the base compiled the file because every
`PutFragment` in it is hand-encoded protobuf over `tonic::client::Grpc`, never the changed
Rust API. The two legs that passed on the base are the controls that *must* pass there (a
live write, and a request with no deadline field). This is the brief's explicit
falsifiability requirement, discharged with numbers.

**(b) Production path?** Yes. The added file drives the real `ChunkStoreService` over a real
tonic loopback with a real `FsChunkStore` behind it; only the request *bytes* are
hand-rolled (so the file compiles on the base). The fs legs call the production
`FsChunkStore::put_fragment` directly. The DST leg runs the production `GrpcChunkStore`
client → real `ChunkStoreService` → real `FsChunkStore` on madsim's simulated network. The
only scaffolding is (i) the `Clock` seam, which is production API (ADR-0024) and defaults to
the wall clock, and (ii) the DST parking wrapper, which delays and delegates and enforces
nothing.

**(c) Fixture includes the fault?** Yes — the fault *is* the late publication, and nothing
curates it out. `AcrossPublication` (`crates/chunkstore-fs/tests/conformance.rs:340-411`) is
a clock anchored to the store's own on-disk state: it reports the write live until the
`.frag` exists, and past the deadline from the first read taken with it there. The test then
asserts, from the clock's recorded observations, that a reading really was taken with the
fragment on disk and the scratch renamed away (`:445-453`) — i.e. that the judgment happened
on the far side of the publication syscall, which is exactly what iteration 2's
scripted-reads clock could not show. The DST leg parks a real write 2 s past a 200 ms
deadline with its caller gone, and its **control** — same park, same abandoned caller, no
deadline — lands, so the refusal is attributable to the deadline and not to the caller's
disappearance. Leg F's genuine-fault control is a real `ENOTDIR` from the production store,
not a synthetic error.

## 5. Gate evidence (all run in `$PDCA_WORKTREE` via the project's own runners)

* `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** — including the prose gates,
  which really ran here (`typos`, `lint_docs: OK`, `render_site: link audit OK`), plus fmt,
  `clippy -D warnings`, build, the whole test suite, `cargo deny`, conformance vectors,
  the statics/gitlink/unsafe guards, and the 50-seed DST.
* `./engine/scripts/run-verify.sh` (C4-verify) → **PASS**, numbers in §4. Ran 4×; one early
  invocation failed on a stale `../wyrd-verify` worktree left by the previous iteration's
  Check (a partially-patched tree) and every run after the worktree reset passed — flagged
  here only so a one-off repeat at Check is recognised for what it is.
* `./scripts/mutants-in-diff` (C5, advisory) → **38 mutants: 7 caught, 31 unviable, 0
  missed** (iteration 2: 2 missed). The two former survivors were
  `SteppedClock::remaining -> 0 | 1`, i.e. the helper that pins "the store really took both
  clock reads" could have been vacuous; killed by
  `crates/testkit/src/lib.rs:854-878`.
* `cargo fmt --all` run over every touched file (commit-hook readiness).

No external dependency was missing: `typos` and the doc renderer are both installed here, so
no `NEEDS-HUMAN external dependency` marker is warranted.

## 6. Honest residuals for sign-off

1. **Retraction window.** Between the `rename` returning and the `unlink` completing, a
   concurrent reader can observe a fragment that is about to be retracted. It is bounded by
   one `unlink` in the same directory, and the *durable* state every later reader (GC,
   scrub, the read path) sees is the retracted one — which is the state 0016's argument is
   about. Documented at `crates/chunkstore-fs/src/lib.rs:315-322`.
2. **The `is_published` stat/rename race** — §3, third bullet. Bounded, documented, and a
   per-fragment lock is the fix if a maintainer wants it closed.
3. **Mixed-version fleets** (0016 open question 2, and the reviewer's C1). An old D server
   ignores the field, so a new client talking to one silently gets no guarantee. I kept the
   brief's assumption (acceptable for Alpha) and stated the degradation in three places
   rather than hiding it: the proto (`chunk.proto:37-45`), the client
   (`client.rs:242-246`), and the architecture doc. A capability exchange is the alternative
   and is not in this slice.
4. **`W_write`'s value is still nobody's** — this slice ships the mechanism and every
   production caller passes `None`, so nothing is enforced in deployment until #625 sets the
   window and #636 starts sending it. That is the brief's scope split, not an oversight;
   the "no caller sends a deadline yet" state is what leg D pins as unchanged behaviour.
5. **`chunkstore-fs` cannot meaningfully "queue"** (0016 open question 3). Its analogue of
   an accept queue is the blocking-pool hop plus the publish step, which is precisely where
   the bound now sits — so legs A, C and the publication-crossing leg are asserted on it,
   and the parked-queue scenario is asserted on the D server that really has one.

## 7. Scratch

Working files (red-leg copies, gate output) were kept under
`$PDCA_SCRATCH/pdca-builder-638-redleg/` and are removed at the end of this run; nothing was
written to `/tmp`.
