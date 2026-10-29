# Design proposal — issue 625 / multipart-reaper

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` **as it
> stands on `origin/main` @ `22d71b4`** (3,108 lines; content tip `c35d39d`, merged by PR #627,
> tracking issue #626). Read the file in the checkout, never a commit: `97e2392` is only the
> initial draft and ten review commits followed it. **Decision 6
> (`0016:1794-2277`) IS this slice's design** — "designed here, implemented in #625" — together
> with decision 5's per-session reference-based reclamation and 0016's *Definition of done* for
> #625 (`0016:2942-2945`). **Do MUST read decision 6 in full before writing code**, plus its
> failure-mode table (`0016:2252-2276`), which enumerates every way to implement this wrong and
> the observable that catches it. This brief does not restate it; it scopes it, fixes the
> operator surface, and states the C4 shape — which this slice needs designed deliberately.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-25; the passages revised on
> 2026-10-15 (legs A, F, H, `Falsifiability`, `Repro instruction`) were verified at `7bf7eb9`.
> **This bundle cannot build on bare `main`.** #636 no longer exists as a slice: it was re-sliced,
> and its pieces are the eleven ids in `Depends on`, four of which have not landed (all four
> planned, none built, as of 2026-10-20). See `Falsifiability` § *Base resolution* for what each supplies and when RED
> becomes runnable. Where this brief still says "#636" for a record or a seam, read "the
> successor slice named there".
>
> **RE-POINTED 2026-07-26.** #508 was re-planned as five slices (634 → 635 → 636 → 637 → 508)
> after its seventh attempt was rejected on reviewability. Everything this loop reads now lands in
> **#636** (the commit protocol in `crates/core`); #508 is reduced to the S3 wire surface. Below,
> "#508" has been replaced by the slice that actually lands each thing — except where it means the
> **release order** (this bundle still lands with or before #508), which is unchanged.

- **Slug:** multipart-reaper
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** reclaim the staged parts of a multipart upload the client **silently discontinued** —
  never aborted, never completed — and give every non-`Open` session state an actor that drives
  it to zero. Without it, an abandoned upload pins its staged fragments indefinitely (they are
  *deliberately* protected while their session lives, #637 / 0016 decision 2), the admission
  counter never falls, and `MAX_SESSIONS` of them is a permanent `503 SlowDown`. 0016 makes the
  ordering normative: **#625 MUST land with or before #508** (`0016:234-268`).
- **Success criterion:** one new test file, `crates/server/tests/multipart_reaper.rs`, driving
  the reaper through the **operator one-shot** (see `Design § the C4 shape`) against a redb
  store seeded with raw records. Legs, each asserted on durable store state:
  **(A) Idle abandonment is reclaimed — every byte evidenced, not just the residue.** An `Open`
  session whose last progress instant is older than `W_open`, holding a committed `part:`/`psum:`
  pair and crashed in-flight `sidx:` residue with a lapsed lease, is torn down. **The fixture
  must be reapable by the idle arm ONLY** (2026-10-20): `W_open < now - created_at_millis <
  W_session`, every progress stamp (`psum:` `committed_at_millis`, `slot:` `reserved_at_millis`)
  older than `W_open`, and every `sidx:` and `slot:` lease already lapsed. 0016 makes idle
  abandonment and the residency ceiling independent predicates (`0016:1910-1915`), and a fixture
  past `W_session` is reaped by the ceiling arm alone, so it cannot tell a reaper with no idle
  arm from a correct one. Concretely, against the wall clock the test reads at seeding:
  `created_at_millis = now − 14 days`, the progress stamps `now − 10 days`, the lease expiries
  `now − 9 days`; this needs the default windows bounded as `Scope` states (`W_open ≤ 7 days`,
  `W_session ≥ 21 days`), which leaves at least three days of margin on each side for the
  subprocess's run time. A one-shot that reaps this fixture with the idle arm disabled is a
  defect (negation (10) in `Verification posture`). After the pass(es)
  converge, `mpu:<id>`, `part:<id>:`, `psum:<id>:`, `sidx:<id>:` and `slot:<id>:` are **all
  empty**, `mpuctl.count` is back to its pre-session value, and an `orphan:` mark exists for
  **every fragment position of BOTH byte sets** — the `sidx:` residue's recorded `staged`
  placement **and every placement position of the committed `part:` record's chunks** (the
  `retire:bytes:{session}` obligation the reap fence installs, `0016:664`). Assert the committed
  part's positions explicitly: a design that marks only the residue, deletes the `part:`/`psum:`
  records and decrements the counter satisfies every *other* assertion in this leg while leaving
  the committed part's fragments unreferenced **and** unevidenced — which on this base means
  retained forever with nothing naming them (`crates/custodian/src/gc.rs:180-190`). Marks always
  precede the deletion of the records that name the bytes (`0016:1121-1125`).
  **The retirement obligations are gone too, or the marks never pay out.** After convergence both
  session-scoped `retire:` ranges are empty — `retire_session_range(RetireMode::Bytes, id)` and
  `retire_session_range(RetireMode::Records, id)` (`crates/core/src/multipart.rs:1490`). GC holds
  any expired mark whose event names a `retire:bytes:` obligation that still exists
  (`crates/custodian/src/gc.rs:690-692`, `:743-761`). So a design that marks every position and
  deletes the naming records but leaves the obligation passes every other assertion here while no
  fragment is ever reclaimed. Assert GC's own licence condition on durable state for every marked
  position: the mark decodes (`decode_orphan_mark`, `crates/core/src/metadata.rs:277`) as stamped,
  not `reclaiming`; its `retire_token()` (`:212`) names no `retire:bytes:` key that is present; and
  no `part:` or `sidx:` record names the position.
  **The terminal delete waits for the obligation.** A second fixture's obligation cannot finish in
  one pass: one position it names carries a `reclaiming` mark (leg F, arm 0). After the one-shot,
  its `mpu:` record is still present, `mpuctl.count` is unchanged, the `reclaiming` mark is
  byte-identical, and the obligation is still there. The test then deletes that key, as GC's blind
  delete would (`gc.rs:804-824`), and runs the one-shot again: the position gets a fresh mark under
  `require_absent`, the obligation and the session are gone, and the count drops by exactly one.
  **And GC really reclaims them** (`cargo xtask ci` evidence, not C4: it needs `reconcile_step`
  in-process, whose signature this slice may change): append a case to the existing
  `crates/custodian/tests/gc.rs`, which has in-memory metadata and chunk stores. Converge the
  reaper over leg A's fixture, put the fragments of both byte sets in the chunk store, run one GC
  pass at `now = mark + grace − 1` and assert nothing is deleted, then at `now = mark + grace` and
  assert every one is deleted. This is the tracker's own acceptance ("its fragments reclaimed
  after the reader-safe grace window", `results/issue_625/notes.json`).
  This is the binding red→green leg.
  **(B) A progressing upload is never reaped by the IDLE arm — for as long as the residency
  ceiling allows.** Three shapes, all of which MUST survive: (i) a session whose parts are old but
  which owns an **unexpired `sidx:` lease** (the single `UploadPart` streaming for longer than
  `W_open`); (ii) a session that has **reserved a `slot:` and not yet written a chunk**, whose
  slot lease is being renewed — the pre-first-chunk window (`0016:1822-1835`); (iii) a session
  whose newest `psum:` `committed_at_millis` is inside `W_open`. Assert the records are untouched
  **and** the admission count unchanged. **Each fixture MUST carry a `created_at_millis` strictly
  inside `W_session`**, or this leg and leg C demand opposite outcomes for the same session: 0016
  bounds even a demonstrably live session by the administrative ceiling (`0016:1886-1888`,
  `:1908-1913`, and the long-stream row at `:2256` — "it is still bounded by `W_session`, arm ii").
  "However long it runs" is true of the idle arm only.
  **(C) The administrative residency ceiling still bites.** A session with *fresh* progress but
  `created_at_millis` older than `W_session` is fenced to `Aborting` and torn down (arm (ii),
  `0016:1908-1913`) — the ceiling is not a race and carries no slot preconditions.
  **(D) `Completing` is exited, both ways.** A `Completing` session whose `fenced_at_millis` is
  older than `W_completing` but which is **not** over-age rolls back to `Open@E+1` and installs
  `retire:records:{seg:<g>:<E>}` for that attempt's segments; a `Completing` session that **is**
  over-age (`now - created_at > W_session`) goes **straight** to `Aborting@E+1` in one batch that
  also installs `retire:records:{seg:<g>:<E>}` — never a rollback first (`0016:1918-1930`).
  Assert the `seg:` range of the named epoch drains empty and a **later** epoch's segments are
  untouched.
  **(E) The clock guard suppresses judgments, not the pass.** A session whose `clock_source` the
  reaper does not own is **never** evaluated by any timestamp arm — assert this for **all four**
  windows, with a fixture each: an over-age foreign-clocked `Open` session survives intact
  (`W_open` and `W_session`), **and a foreign-clocked `Completing` session whose `fenced_at_millis`
  is far past `W_completing` is NOT rolled back** — its state, epoch, `seg:` range and `retire:`
  records all unchanged (`W_completing`; without this fixture an implementation that guards the
  open-session and tombstone judgments while still rolling back a stale foreign `Completing`
  passes every other assertion) — **while its clock-free work still runs**: a foreign-clocked **`Aborting`** session has
  its `sidx:` range walked, its `retire:` obligation drained and its terminal delete fire
  (`0016:1889-1907`, X71). A foreign-clocked **`Completed`** session keeps its records (its
  tombstone window is unjudgeable) — that one exit is #633's terminal-expiry verb, and this leg
  asserts the record **survives** here.
  **(F) Teardown is crash-safe, idempotent, and obeys the THREE-ARM mark rule — after the
  `reclaiming` check.** Running the pass
  twice over a partially drained obligation converges to the same terminal state with **no
  double-decrement** of `mpuctl.count`. For pre-existing `orphan:` marks, first: **(0) a mark
  that decodes as `reclaiming` is never replaced, whatever event it carries.** That shape is GC's
  recorded decision to reclaim (`crates/core/src/metadata.rs:109-140`: "No writer overwrites a
  `reclaiming` mark … Every mark writer that reads before it writes — the three-arm drain (#659)
  … inherits this rule"). GC deletes the key blind after destroying the fragment
  (`crates/custodian/src/gc.rs:804-824`), so a value written over it vanishes with it and leaves
  the new event with no evidence. The pass leaves it byte-identical, keeps the obligation (and
  its cursor at or before that position) so the work resumes, and on a later pass, once the key
  is gone, writes the position under `require_absent` (arm c). Leg A's second fixture exercises
  this. Its race with GC is an interleaving the record-level test cannot build: the reaper's
  exact-value replacement (arm b) and GC's swap to `reclaiming` both read the same stamped
  bytes. A seeded DST row (see `Verification posture`) asserts exactly one commit wins and that,
  when GC wins, the reaper's re-read finds `reclaiming` and preserves it rather than retrying
  arm (b) against it. Then assert 0016's three-arm
  rule (`0016:1127-1160`, iteration-7 finding 3 as corrected by iteration-9 finding 3 and
  iteration-13 finding 1) — **not** a blanket "never re-stamp", which 0016 explicitly rejects as
  "the concurrency half alone": (a) a mark carrying the **same** unreference-event identity as this
  walk is **skipped, byte-for-byte unchanged** (assert the stored value before and after — a
  re-stamp there restarts the grace window and can postpone reclamation indefinitely); (b) a mark carrying a **different unreference-event identity — including the legacy
  *parseable* bare-decimal value, which carries no identity** (`0016:1190-1224`) — is **replaced
  with a fresh stamp** under the exact-value guard (assert the value *does* change: leaving an
  already-expired mark from an earlier event in place would let GC reclaim with no grace at all
  once the last reference goes); (c) an absent position is written under `require_absent`. Seed
  one fixture per arm, arm (0) included. **A fourth case is NOT a re-stamp arm: a value that does not decode at all
  fails closed** — the pass leaves it untouched, classifies and surfaces it, and does not act on
  it (ADR-0045's metadata-validation boundary, `docs/design/adr/0045-metadata-validation-boundaries.md:42-65`;
  rewriting corrupt metadata is the one thing a maintenance loop may never do). Note the base
  helper is a precedent for neither arm: `mark_orphaned` is an unconditional plain put
  (`crates/custodian/src/gc.rs:110-122`).
  **(G2) The DEPLOYED loop reaps — not only the one-shot — on BOTH deployed shapes.** Every other
  leg drives the operator one-shot, and an implementation that wires the reaper **only** into
  that flag — leaving the ordinary interval path untouched — passes all of them while reaping
  nothing in production. The role has two deployed shapes at `12dd082`, and they run different
  code: **without `--endpoints`** it prints "nothing to reconstruct" and **exits**
  (`crates/server/src/cli.rs:1083-1100`); **with** them it runs `run_reconstruction_over_backend`
  (`:1241-1259`), whose interval driver is `CustodianService::run_reconstruction_until`
  (`crates/server/src/custodian.rs:521-545`). Required: **one** reaper interval driver that both
  shapes run, every interval — the no-endpoints role runs it instead of exiting (the reaper is
  record-only, so a fleet-less role has work to do), and the fleet role runs the same one. And
  the reaper pass runs however much of the fleet is reachable: it MUST NOT sit behind GC's
  whole-fleet gate (`if fleet.len() == operator_fleet_size`, `custodian.rs:680-713`), which
  defers GC on a degraded fleet; a metadata-only pass has no reason to inherit it. Do records in
  `build-notes.md` the call path from `cmd_custodian` to the reaper for each shape, the
  empty-fleet startup below included.
  **The third startup path: `--endpoints` given, none reachable (settled 2026-10-22).** The role
  panics on an empty fleet (`crates/server/src/cli.rs:1161-1174`) before it reaches the loop
  (`:1243`). That refusal stays: `connect_fleet` returns only the servers reachable at startup
  and the loop never re-dials one it skipped (`crates/server/src/custodian.rs:247-263`), so a
  role that stayed up on an empty fleet would run no reconstruction until someone restarted it
  by hand — the "clean vanish" the panic exists to prevent (its comment, `cli.rs:1162-1168`).
  The policy for the reaper on this path: **run exactly one reaper pass — the same pass, through
  `reconcile_step`, under the leadership the role already holds by then (`Custodian::elect`,
  `crates/server/src/cli.rs:1028`, runs before the endpoints check) — after the fleet comes back
  empty and before the existing panic, then fail as today.** Each supervisor restart therefore
  reaps once while the fleet is down, and the role still exits non-zero with the same
  diagnostic. The pass is bounded (decision 6's per-pass budgets), so it cannot delay the loud
  exit without limit. Do MUST NOT change the panic's message, its exit status, or the
  start-degraded behaviour for a partly reachable fleet. ⚑ **Human:** the alternative — keep the
  role up, reap every interval, and re-dial the fleet until a server answers — changes the
  reconstruction plane's startup and is outside this slice; say so at sign-off if you want it
  filed.
  **G2a, the no-endpoints shape (in `multipart_reaper.rs`, binding):** start the **ordinary
  custodian role** as a subprocess with no `--endpoints` and a short `--interval-secs`, against a
  store seeded exactly as leg A (the same idle-only timestamps), let it run one bounded interval,
  terminate it, and assert the **same** store transition — with **no one-shot flag** anywhere in
  its argv. On the base the role exits at once and the store is unchanged, so it reds on the
  store assertion.
  **G2c, the empty-fleet shape (in `multipart_reaper.rs`, binding; added 2026-10-22):** start the
  ordinary custodian role as a subprocess with `--endpoints` naming one address nothing listens
  on (e.g. `http://127.0.0.1:1`, refused at once), matching `--ids` / `--failure-domains`, a short
  connect timeout if the role takes one, no one-shot flag, against a store seeded exactly as leg
  A. Wait for it to exit, with a bounded wait. Assert: a non-zero exit status; stderr contains
  the existing fleet-outage diagnostic ("NO configured D server was reachable at startup",
  `cli.rs:1170`); and leg A's store transition happened. On the base the role panics before any
  pass and the store is unchanged, so it reds on the store assertion, not on the exit status
  (which is non-zero on both trees).
  **G2b, the fleet shape, degraded (`cargo xtask ci` evidence, not C4):** append a case to the
  existing `crates/server/tests/custodian_gc.rs`, which already drives the real run loop with an
  operator fleet larger than the reachable one (`drive_deployed_loop_operator`, `:447-480`;
  the startup-partial case, `:886`). Seed leg A's fixture into its store, drive the loop with
  `operator_fleet_size` greater than the servers it is given (GC deferred), and assert the same
  reaper transition. It lives in a modified file because it calls `run_reconstruction_until`,
  whose signature this slice may change, so it cannot share the added file (a compile failure
  there would cost every leg its red). Without G2a and G2b the `Production reach` field below is
  false.
  **(G) Tombstone expiry.** A `Completed` session (local clock) whose `completed_at_millis` is
  older than `W_tombstone`, with no session-scoped `retire:` obligation left and an empty
  `sidx:` range, has its record deleted and `mpuctl.count` decremented — the observable #636
  cannot produce on its own (its Complete leaves a live tombstone).
  **(H) A profile mismatch stops the reaper before it acts.** Seed `mpuctl` whose stored profile
  (`AdmissionRecord::profile`, `crates/core/src/multipart.rs:1858`) differs from the custodian's
  local `Budget` (`:1567`) and assert the pass **emits the operator signal and mutates nothing** —
  no fence, no mark, no deletion, no counter change — while the matching-profile fixture in leg A
  still reaps. The reaper's own reason: its reap fences and terminal deletes are unsplittable
  batches whose size is fixed by the profile the session was admitted under —
  `MAX_INFLIGHT_PARTS` slot preconditions in the fence, `MAX_PART_CHUNKS` chunk refs per part —
  so a reaper whose profile differs may meet a batch its own clamps never sized. Without this
  leg the comparison can simply be absent and every other leg still passes.
  **What profile equality does NOT prove (revised 2026-10-22): equal operation budgets.** The
  stored profile is five fields — `W_ref`, `MAX_PART_CHUNKS`, `MAX_PARTS_PER_SESSION`,
  `MAX_INFLIGHT_PARTS`, `MAX_STAGED_CHUNKS` (`crates/core/src/multipart.rs:1580-1586`). `B_ops`
  is `KnobSet::batch_ops` (`:5307-5308`), which `KnobSet::profile` leaves out (`:5346-5355`).
  Two processes with different `B_ops` therefore pass leg H's comparison, and leg H cannot catch
  that. This slice MUST NOT add `B_ops` to the stored profile (no format change, `Impact`).
  Operation capacity rests on three other things instead, and Do cites each in `build-notes.md`:
  (1) **one `B_ops`**: every reaper batch is sized from `MAX_BATCH_OPS` / `MAX_BATCH_BYTES`
  (`:5106`, `:5067`) and no other operation or byte budget is defined anywhere in this slice
  (the source forbids re-deriving it, `:5103-5105`); (2) **the clamps**: `knob_clamps_hold`
  refuses any set whose part commit or fence does not fit its own `B_ops` (`PartCommitOverOps`,
  `SlotRangeOverOps`, `:5454-5457`), and `crates/core/tests/multipart_knobs.rs:167` asserts the
  shipped set passes — it must stay green and unmodified. So any build that passes its own clamp
  test and admits under a given profile has a `B_ops` that fits that profile's unsplittable
  batches; equal profile plus a passing clamp test, not equal profile alone, is what makes the
  reaper's fences fit; (3) **calibration**: whether `B_ops` operations finish inside the
  transaction deadline on a real backend is a measurement, the off-Check calibration leg in
  `Verification posture`. No store-state leg can observe point (1) — a different local budget
  only changes how work is split — so it is evidenced by those citations and checked in review.
  **What this leg does not cover, and must not claim.** 0016's memory guard — the custodian
  compares the profile *before it builds the staged reference set* (`0016:2062-2070`) — protects
  every pass that builds that set, and the reaper is not where the set is built. On `7bf7eb9`
  GC is dispatched first (`crates/custodian/src/reconciliation.rs:138-145`) and reads the staged
  set at once (`crates/custodian/src/gc.rs:421-430`); restore (`restore.rs:626`), drain status
  (`desired_state.rs:256`), scrub (`scrub.rs:142`) and staged reconstruction
  (`reconstruction/staged.rs:217`) build it too. A check inside the reaper passes this leg and
  protects none of them. That preflight is **#806** ("custodian: compare the budget profile
  against mpuctl before building the staged reference set", open, 0.1 Alpha, no bundle yet), and
  its site is already marked in code (`gc.rs:1541-1546`, `deferred: #806`); its read-order
  evidence is #806's to produce. **One comparison, not two:** if #806 has landed when Do builds,
  the reaper calls its comparison; if not, Do writes the comparison as one `wyrd-custodian`
  function the reaper calls, with a doc comment naming #806 as the caller to add at
  `gc.rs:1541`.
- **Falsifiability:** RED is an assertion red, in-process, with no deploy stack, no fleet and no
  container — **but only on a base that carries every prerequisite, and no such base exists yet**
  (see *Base resolution* below; nothing in this field is evidence that the unlanded prerequisites
  will deliver what it assumes). On that base every multipart record class has a production
  writer and the retirement ledger is installed, yet **nothing drives a session the client
  abandoned**: legs A, C, D, E(second half), F, G, G2a and G2c all fail on their durable-state assertions
  because the records are still there. Leg B passes on the base **vacuously** (nothing reaps
  anything), so it carries no red — say so, and evidence its force by the negation runs in
  `Verification posture`.
  **Base resolution — a precondition, not met on 2026-10-15** (checked at `7bf7eb9` and with
  `gh issue view`). What each `Depends on` id must supply, and where it stands:
  - **on `main` already:** #693 (session state machine, answer table, digests; `605b33a`), #808
    (drain status reads the staged class; `cf6b420`), #810 (the restore-fence generation record;
    `39bf35a`), #813 and #814 (scrub and reconstruction of staged chunks; `feb1e30`, `ab35f6a`).
    #655's budget knobs (`4533549`) are on `main` too, so no field names them;
  - **merged since (checked 2026-10-20 at `12dd082`):** #894 (Create: the `mpuctl`, `mpu:` and
    `seggrp:` writers; PR #908, `f9b70e4`) and #895 (Abort's `Open → Aborting` fence with its
    `retire:bytes:` obligation, and the one fence batch builder this slice's reap fence must reuse
    rather than copy; PR #909, `6e26f1f`);
  - **planned, not built (2026-10-20):** #926 and #927 (#657's split: the `slot:`, `sidx:`,
    `part:` and `psum:` writers and the same-batch slot renewal leg B reads), #658 (Complete:
    `Completing`, `Completed` tombstones and `seg:` records, which legs D and G drive), #659
    (retirement routing, the drain and the terminal delete — its overlap with this slice is
    flagged in `Ordering note` and must be settled at #659's Plan).
  The driver holds this bundle unschedulable until all eleven are COMPLETE
  (`src/pdca_harness/waves.py:65-120`, `:286-320`). COMPLETE alone does not put a prerequisite's
  code in the base: an out-of-batch prerequisite imposes no ordering and is folded into nothing
  (`waves.py:197-200`). The base carries it only if it runs **in the same `pdca flow` batch** as
  this bundle (its accepted diff is folded onto the run's integration branch,
  `pdca-integration/main…`, `src/pdca_harness/integrate.py:73-85`, exported as
  `$PDCA_VERIFY_BASE`, `src/pdca_harness/gates.py:544-580`, honoured by
  `engine/scripts/run-verify.sh:247-265`) **or has merged to `main`** first. RED is runnable only
  once one of those holds for all four unlanded prerequisites. **Do MUST record in
  `build-notes.md` the base commit it built on and confirm that base contains each
  prerequisite's code before claiming any red.**
  **What the gate does on a wrong base.** A test or patch that does not build on the base runs
  zero tests, and `run-verify.sh` reports that as **UNVERIFIABLE (exit 77)**, a §6 item, never a
  PASS (`engine/scripts/run-verify.sh:40-56`, verdict `:207-229`, `:521-541`). Do MUST record, from
  the RED leg, **how many tests actually ran and failed**.
  **Corollary the test must obey:** the added test file may reference **only symbols that exist
  on this bundle's resolved base** (`main` plus the four unlanded prerequisites) — nothing this
  slice adds. That is what makes the
  RED leg a real assertion failure rather than a compile error, and it is why the reaper is
  driven through a stringly-typed operator command rather than a new Rust entry point (see
  `Design § the C4 shape`).
- **Invariant to restore:** **no state of an upload session is absorbing, and no durable byte is
  left with nothing that will ever reclaim it** — for every state in 0016's exit table
  (`0016:569-579`) there is an actor that drives it out, and a session's residency is bounded.
  The reaper is that actor wherever no client verb will ever arrive. **Source:** 0016 invariants
  (2) and (3) and its implementation-order argument (`0016:236-262`), resting on the custodian's
  written reclaim mandate and safety gate (`crates/custodian/src/gc.rs:14-25`). Provenance (`docs/principles.md`): this brief falls in the **§6 storage-lifecycle /
  reclamation** category, so per principle 4.2 it states catalogue invariant **C-1** — *a
  permanent or data-losing failure mode is never an acceptable cost* — with its citation
  (maintainer's standing rule, 2026-07-25; corroborated by 0016's refutation standard
  `:2802-2813` and `crates/custodian/src/gc.rs:22-25`). This
  is a **structural / lifecycle** change, so §1.2 applies: the target is the smallest change that
  **restores the invariant**, not the smallest diff.
  *Plan-exit gate (category-gated, both checks pass):* Scope names no probe/guard/helper
  mechanism — it names the loop, its arms and their bounds, all fixed by 0016; and the invariant
  cannot be satisfied by guarding a single module, because it quantifies over **every** session
  state and every staged byte in the store.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 813, 814, 808, 810, 693, 894, 895, 932, 933, 927, 658, 659
- **Conflicts with:**
- **Ordering note:** **RE-POINTED 2026-10-22 — `926` → `932, 933`.** #926 (657.1) was split at its third re-plan
  into #932 (657.1a: slot reserve, owned intent, drain fence, write and body deadlines) and #933
  (657.1b: in-flight renewal and `StagedAttempt::renew`, depends on #932) —
  `results/issue_926/split-proposal.md`. A split parent ends COMPLETE, so `926` would be met at
  once. 
  **RE-POINTED 2026-10-20 — `657` → `926, 927`.** #657 was split at its re-plan
  into #926 (UploadPart staging) and #927 (the part commit, depends on #926) —
  `results/issue_657/split-proposal.md`. A split parent ends COMPLETE, so `657` would be met at
  once; the reaper needs the records both children write. **RE-POINTED 2026-09-18 — `664` → `808, 810`.** #664 was split at its
  re-plan into #808 (drain status + rebalance read the staged class), #809 (the restore session
  fence + the nonce) and #810 (the durable fence-generation record + residue across generations)
  — `results/issue_664/split-proposal.md`. #810 carries #809 through its own `Depends on`; #808
  is independent of that chain, so it is named separately. The previous re-point stands below.
  **RE-POINTED 2026-09-12 — `637` → `663, 664`.** #637 was split into #661 → #662 → {#663, #664} (`results/issue_637/split-proposal.md`). A split parent reaches COMPLETE at its own sign-off, before any child lands (`src/pdca_harness/cli.py:1060-1073`), so the edge now names the last wave's two children, which carry #661 and #662 transitively. **RE-POINTED 2026-07-26 — #508 was re-planned as five slices, and this
  bundle's prerequisite moved from #508 to #636.** #508's seventh attempt was rejected at sign-off
  on reviewability and it is now split into 634 → 635 → 636 → 637 → 508; #508 is reduced to the S3
  wire surface, which this loop does not touch. Every record class this loop reads (`mpu:`,
  `slot:`, `part:`, `psum:`, `sidx:`, `retire:`, `mpuctl`) and the fenced state machine it CASes
  now land in **#636**; the `scan_page` seam its cursor-keyed drain walks lands in **#634** and is
  transitive through #636; the `seg:` records are **#635**'s, likewise transitive. The wave fold
  gives this bundle #636's accepted diff without waiting for a human merge.
  **`Depends on: 637` (upgraded from a conflict edge, 2026-07-26 plan review).** It is a genuine
  build-on, not merely a shared file: this bundle's leg H checks its budget profile **before the
  staged reference set is built**, and that staged reference set is #637's — so the reaper reads a
  structure #637 defines. *(Superseded 2026-10-15: the staged-set check is #806's, see leg H; the
  edge, now 808/810/813/814, stands for the `gc.rs` and `orphan:` reasons that follow.)* It also rewrites `crates/custodian/src/gc.rs` and the `orphan:` mark
  rules that #637 restructures. A bare conflict would have been oriented by **name order**, putting
  `issue_625` *ahead* of `issue_637` and handing #637 a base whose `reconcile_step` this bundle had
  already widened — which would break #637's base-compiling test file and destroy its seven
  assertion REDs. The dependency fixes the direction: **637 first, then 625.**
  The **release** direction is unchanged and is normative in 0016 (`0016:234-268`): the reaper
  must be running wherever the verbs are exposed, so **#625 and #633 are merged with or before
  #508**, bottom-up as one stack. Under the old plan that was in tension with "depends on #508";
  under the split it is not. Per the issue thread, **this issue must not close without #633
  landing alongside it** — #633 depends on this bundle.
  **RE-POINTED 2026-09-11 — `636` removed from `Depends on`.** #636 was DISCONTINUED at sign-off (2026-07-31) and re-sliced into #654–#660 (`docs/2026-07-31-alpha-reslicing-proposal.md`); #654 was then split into #691 (merged), #692 → #715/#716 (merged) + #717 → #771/#772, and #693. A `Depends on` id must reach COMPLETE before the driver schedules the dependent (`src/pdca_harness/waves.py:57-83`, `cli.py:_blocked_by`), and a DISCONTINUED bundle never does — so `636` was a permanent block, not a wait. Replaced by the slices whose behaviour the reaper drives: the state machine (#693), admission (#656), UploadPart staging (#657), Complete/tombstones (#658), and retirement routing + terminal delete (#659). #656–#659 have no bundle yet, so `pdca status` reads blocked-by them — that is the true state. ⚑ Flag for the Plan of #659: it names "terminal delete + exactly-once `mpuctl` decrement", which this brief also claims for the reaper; settle the owner there before either builds. **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
  **Re-pointed 2026-10-15:** #656 was split at its re-plan into #894 (admission + Create, 636.3a) and #895 (Abort fence + shared fence batch, 636.3b, which depends on #894); the field above names both in place of `656`.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`, `fdb cluster healthy`, `openssl dev (pkg-config + libssl)`, `cmake (tikv-client build dep)`
  — nothing beyond the base toolchain is needed for the BINDING criterion: every leg runs
  in-process over a redb store in a tempfile temp dir, and the reaper is **record-only**
  (`0016:1913-1915`), so no D-server fleet and no topology is required to go red→green. The
  six tokens (each a registered doctor row) are for the **one off-Check obligation this slice
  cannot honestly discharge on redb**: the first four are the *complete* prerequisite set for the
  FoundationDB conformance leg, and the last two (with `docker`) build the `tikv` feature for the
  TiKV leg (2026-10-22, when the calibration gained a TiKV test). FoundationDB is the
  production backend (ADR-0042), and its leg needs a container runtime **and** the system client
  library at build time, not Docker alone. `B_ops` must be calibrated so a batch's sequential
  round trips finish inside the 5,000 ms transaction deadline with the half-deadline margin the
  constants assume (`multipart.rs:5053`, `:5088-5106`) **on the slowest supported backend**,
  and 0016 makes that a per-backend *conformance* property, not a DST one (`0016:621-648`,
  `:2907-2910`). Redb cannot evidence it. See `Verification posture` for who runs that leg and
  when; a `B_ops` chosen only against redb can time out forever on FoundationDB or TiKV, which is
  the exact non-termination the knob exists to prevent.
- **Test file:** `crates/server/tests/multipart_reaper.rs` — a **NEW**
  `crates/<c>/tests/<t>.rs` file, which this project's C4 gate requires: `run-verify.sh`
  discriminates on an **added** test file (`_is_test_file`, `engine/scripts/run-verify.sh:144`,
  collected at `:392`) and degrades to a green-only, proves-nothing branch otherwise
  (`:415-424`, `:497-500`). It lives in
  `crates/server` because the operator command it drives does (`crates/server/src/cli.rs`), and
  it must **not** be `#![cfg(...)]`-gated: the gate reads crate-level cfgs off the added sources
  and applies the resulting `RUSTFLAGS` to the whole invocation (`:155-182`, `:463`). **Every DST
  regression this slice owes (below) therefore goes into an EXISTING `crates/dst/tests/*.rs`
  file** — a modified file, which the gate does not add to its invocation, and which `cargo xtask
  ci` still runs. Do MUST NOT add a new file under `crates/dst/tests/`.
- **Verification posture:** flippable red→green at Check for legs A, C, D, E(clock-free half), F,
  G, G2a and G2c. Leg B is **vacuously green on the base** and carries no red, so its force must be
  evidenced by **negation runs Do performs and records in `build-notes.md`** — for each, the
  deliberately-broken variant and the leg that catches it: (1) derive progress from
  `created_at` + `psum:` only, ignoring `slot:` `reserved_at`/lease ⇒ B(ii) reaps a live
  pre-first-chunk request; **(2) and (3) are INTERLEAVING defects that the record-level test
  cannot construct — they belong to the DST rows below, and Do must not claim the integration
  test catches them:** (2) read `psum:` **before** `slot:` ⇒ the destination-first race of
  `0016:1808-1821` (a part commit landing between the reaper's two reads) reaps a session that
  just made durable progress; (3) omit the fence's slot preconditions ⇒ the
  renewal-commits-after-the-expired-read race (X62, `0016:1841-1884`) aborts a progressing
  upload; (4) walk `sidx:` only
  for `Aborting` and not for `Completed` ⇒ the crashed in-flight residue of a completed session
  is stranded forever; (5) fire the terminal delete before the `sidx:` range is observed empty ⇒
  residue nothing can discover; (6) skip a foreign-clocked session **wholesale** ⇒ leg E's
  clock-free half strands an `Aborting` session's records and its admission slot forever;
  (7) re-stamp an existing `orphan:` mark ⇒ leg F's grace clock restarts; (8) treat a
  `reclaiming` mark as a different-event mark (arm b) ⇒ leg F's arm (0) and leg A's second
  fixture fail; (9) delete the session and decrement the count while a `retire:` obligation is
  still present ⇒ leg A's obligation assertions fail; **(10) disable the idle arm (keep only the
  `W_session` ceiling) ⇒ leg A and G2a fail**, because their fixture is inside `W_session`;
  **(11) dispatch the reaper only inside GC's whole-fleet branch ⇒ G2b fails**; (12) leave the
  no-endpoints role's early exit in place ⇒ G2a fails; (13) skip the reaper pass on the
  empty-fleet startup path ⇒ G2c fails. A negation that does
  NOT fail its leg means the leg is inert — say so rather than reporting a pass.
  **Seeded Tier-0 DST is owed too** (`AGENTS.md`: a new destructive or concurrent path lands with
  seeded DST coverage; `0016:2878-2905`). Land at minimum the **Complete/reap race in both
  interleavings**, the **stale-snapshot mid-pass creation (X19)**, and the two interleaving
  negations (2) and (3) above — the **commit-between-the-reaper's-two-progress-reads race (X60)**
  and the **renewal-commits-after-the-expired-read fence race (X62)** — as seeded cases
  **appended to an existing `crates/dst/tests/custodian.rs`** — **plus, not optional**: the
  **pre-first-chunk liveness window** (decision 6's finding-2 row, both halves — live-and-renewing
  MUST survive, crashed MUST become reapable); **two concurrent drainers over one owned `sidx:`
  range (X56)**, the gateway-inline drain racing the reaper on the same per-mark precondition,
  which leg F's two *serial* one-shots do not exercise and which a "read absent, then both put"
  implementation passes; a **crash at each drain step**, asserting convergence to zero staged
  records, no mark re-stamped, and the counter at its true value (`0016:2876-2905`); and **the
  reaper's mark replacement racing GC's swap to `reclaiming`** on one stamped mark (leg F,
  arm 0), asserting exactly one commit wins, a GC win leaves the `reclaiming` value untouched by
  the reaper, and the obligation still drains to empty once GC's key delete lands. They are
  `cargo xtask ci` evidence, not C4-verify evidence — do not move the binding test there.
  **One pre-declared off-Check leg: the `B_ops` calibration (rewritten 2026-10-22).** No
  existing command measures it: `fdb-conformance` runs `--lib`, `conformance`, `contention`,
  `scan` and `timeout` only (`xtask/src/main.rs:571-576`), `tikv-conformance` runs `conformance`,
  `contention`, `scan` and `deadline` (`:296-301`), and the shared suite tests metadata semantics,
  not a budget-sized transaction's time. The figure behind `B_ops` is "assumed, not measured"
  (`SLOWEST_OP_MILLIS` = 5 ms, `crates/core/src/multipart.rs:5082-5088`), and the source assigns
  its replacement to this slice (`:5102-5105`). So this slice **adds the experiment**:
  - **Fixture:** one appended `#[test]` per networked backend, in the **existing**
    `crates/metadata-fdb/tests/conformance.rs` and `crates/metadata-tikv/tests/conformance.rs`,
    gated exactly as the trait-contract test beside it (feature off ⇒ skip notice; no cluster env
    ⇒ skip). The batch builder and the timing loop are one `pub` function in
    `crates/metadata-conformance/src/lib.rs`, kept **out of** `run_all` so redb's conformance run
    never times anything. The budget values come from `wyrd-core` (`MAX_BATCH_OPS`,
    `MAX_BATCH_BYTES`, and `SLOWEST_OP_MILLIS` made `pub`), added as a dev-dependency of both
    backend crates — neither has it today — never as copied literals; the `Cargo.lock` change
    that follows is mechanical. Not a new test file: an added `tests/*.rs` joins C4-verify's
    invocation (`engine/scripts/run-verify.sh:392`, `:403-412`), and this leg is not C4
    evidence.
  - **Worst-case mix — two batches, each committed and timed:** (i) **operation-bound:**
    `MAX_BATCH_OPS` operations, every one a precondition-guarded put (the drain's mark under
    `require_absent`, its commonest form) on a distinct key, values small; (ii) **byte-bound:**
    the same count and shape with values sized so the batch carries `MAX_BATCH_BYTES` of puts
    in total, with the precondition kinds split between `require_absent` and an exact-value
    guard on keys seeded beforehand (the fence's and the cursor advance's form). Both backends
    run every precondition and mutation one after another inside the transaction
    (`multipart.rs:5083-5085`), so these are the slowest batches the reaper sends.
  - **Assertion:** each batch commits `Committed`, and its measured wall time is at most
    `MAX_BATCH_OPS × SLOWEST_OP_MILLIS` (2,500 ms at `4bac538`) — the per-operation time the
    constants assume — not merely under the 5,000 ms deadline (`E_TX_MILLIS`, `:5053`), so the
    margin `B_ops` claims is what is checked. Do may make `SLOWEST_OP_MILLIS` (and `E_TX_MILLIS`)
    `pub` for the test; values unchanged. Run each batch several times (Do picks the count,
    at least five) and assert on the slowest. The wall-clock read carries the workspace lint's
    annotation (`clippy.toml`). Print every measurement.
  - **Wiring:** none to add — `conformance` is already in the FDB leg list (`xtask/src/main.rs:574`)
    and the TiKV list (`:297`). TiKV's runner retries a failing binary up to five times
    (`run_tikv_test`, `:304-344`), which would hide a slow run, so `build-notes.md` records the
    **first** attempt's figures, and any failed attempt is reported, never retried away.
  - **Who changes the shared constant:** if the slowest measured batch is within the bound, the
    constants stay as they are and this slice replaces only the "assumed, not measured" wording
    of `SLOWEST_OP_MILLIS`'s doc comment with the measured figures (backend, version, host,
    date). A ceiling above the measurement is safe; it is not lowered to match. If a batch is
    **over** the bound, `B_ops` is too high for that backend: lowering it moves the clamps
    (`:5454-5457`) and may move the stored profile, which is #655's settled value set and outside
    this slice (`Scope`, *the knob values*). Do then changes no constant, records the figures,
    and raises a §6 item naming the measurement; the human decides the re-valuing.
  - **Who runs it:** Do runs `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance`
    where the host allows and records the figures in `build-notes.md`; where it cannot, **Eduard
    Ralph runs the same two commands before merge and records the figures in SUMMARY §9**. Check
    compiles the TiKV test (the `host-tikv` gate, `clippy --tests`); it does not run either.
  What IS built and exercised at Check is the budget mechanism itself (the batch splitter and its
  bound, unit-tested here) — nothing deferred is unbuilt.
- **Production reach:** the live path is exercised at Check — **and leg G2 is what makes that
  claim true rather than asserted**: G2a drives the ordinary no-endpoints custodian role, with no
  one-shot flag, G2b drives the fleet role's interval driver on a degraded fleet, and G2c starts
  the role on a configured fleet none of which answers; all three demand the same store transition. The one-shot and the deployed loop dispatch the **same**
  reaper through the **same** fenced control point
  (`crates/custodian/src/reconciliation.rs:65-115`) — the one-shot is an operator entry, not a
  test-only parallel entry (the anti-#141 rule that module states). Do MUST NOT add a test-only
  reaper entry point, and MUST NOT wire the reaper into the one-shot alone.
- **Scope:** decision 6 implemented as a custodian loop, dispatched from `reconcile_step` like
  every other pass — concretely: the liveness observable (`max(created_at, max psum
  committed_at, max slot reserved_at)`, plus "live while any owned `sidx:` **or** `slot:` lease
  is unexpired"), read **source-first** (`slot:` then `psum:`, normative); both abandonment arms
  (idle `W_open` with the fence's slot preconditions pinned, and the administrative `W_session`
  ceiling without them); the `Completing` rollback at `W_completing` and the over-age
  `Completing → Aborting` edge, both installing `retire:records:{seg:<g>:<E>}`; the
  `clock_source` guard that suppresses **judgments only** and still performs the clock-free
  teardown; fence-then-walk per-session `sidx:` reclamation for **both** `Aborting` and
  `Completed`, orphan-marking each entry's full recorded `staged` placement; the cursor-keyed,
  byte- **and** operation-budgeted `retire:` drain over `scan_page`; the tombstone expiry at
  `W_tombstone`; the terminal session delete preconditioned on the session record's exact bytes,
  gated on an observed-empty `sidx:` range, discarding surviving `slot:` records and the
  never-adopted `seggrp:` marker, with the exactly-once `mpuctl.count` decrement; the
  reaper's own **profile validation** — it compares its local `Budget` against `mpuctl.profile`
  and **fails closed with an operator signal before it fences, marks, deletes or decrements
  anything** (leg H) — **reading the budget profile #655 landed (`Budget`,
  `crates/core/src/multipart.rs:1567`; the shipped profile, `:4913-4923`), unchanged**; the
  `reclaiming` check ahead of the three mark arms (leg F, arm 0); the knob values this slice owns: the **reaper's own windows** `W_open`,
  `W_completing`, `W_session`, `W_tombstone`, the pass cadence, and **`G_orphan`** — which 0016 assigns here explicitly
  ("#625 (`G_orphan`, today's grace window)") together with the **strict** inequality
  `G_orphan > W_write + δ_clock`, and, where a repoint can intervene,
  `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, restated in the clock-lifecycle
  table `:2497-2509`). The inequality is strict because GC's grace check is inclusive
  (`crates/custodian/src/gc.rs:172-176`): at `G_orphan == W_write` a fragment authorized before a
  fence can land in the very tick its evidence becomes reclaimable. Ship a regression that pins
  that boundary tick. **`W_ref`, `B_bytes`, `B_ops`, the profile tuple and `MAX_SESSIONS` are NOT
  this slice's to choose — they are ONE value set, landed on `main` by **#655** (`4533549`; the
  #636 role this paragraph was written for), and consumed here unchanged.** The ledger's `mpuctl.profile` is written by Create in #636;
  `MAX_PART_CHUNKS` / `MAX_INFLIGHT_PARTS` are clamped by `B_ops`; a reaper that derived its own
  **profile** values would fail its own profile check against every session #636 admitted. A
  reaper that derived its own **`B_ops`** would NOT fail that check — `B_ops` is not in the stored
  profile (leg H, *What profile equality does NOT prove*) — and a lower one could make this loop's
  **unsplittable** reap fence and terminal delete time out forever on sessions #636 legally
  admitted. That is why the reaper reads `MAX_BATCH_OPS` and nothing else, and why the clamp test
  stays the evidence that the profile's batches fit it. 0016's *Open questions* nominally assigns `W_ref`/`B` here, but a slice cannot consume a
  later wave's choice, and split budget authority admits a permanent failure class
  (a legally-admitted session whose unsplittable reap fence no longer fits is never reaped and
  never reclaimed) — which Wyrd does not accept as a cost. **SETTLED by Eduard Ralph,
  2026-07-25**; the knob-value authority now sits in **#636**'s brief (§ *The knob values*), which
  records the resolution and the reasoning. Consume
  the seam; if its values are wrong, that is a Check §6 item against the stack, never a local
  override. Each value inside the
  range 0016 settles and citing that range at its definition, with generous (days-scale)
  `W_open`/`W_session` defaults, **bounded `W_open ≤ 7 days` and `W_session ≥ 21 days`** (0016
  sets no range for either; the bound is what lets leg A's fixture sit between them without the
  test importing a symbol this slice adds); wiring into the deployed `wyrd custodian` role so
  the pass runs every interval, **through one interval driver on both deployed shapes and
  outside GC's whole-fleet gate** (leg G2); **an operator one-shot** on that same role that runs exactly one
  reaper pass against the metadata store and exits, **without requiring `--endpoints`** (the
  reaper reads no fleet) — this is both the runbook tool and the test seam; and the **living
  architecture docs** (`docs/design/architecture/06-runtime-view.md`,
  `08-crosscutting-concepts.md`) updated for the reaper loop and the new CLI flag — a **merge
  requirement** in this repo (`AGENTS.md` "Docs currency"), not a follow-up.
  / out of scope: the **record classes, the state machine and the retirement-ledger installation**
  (**#636**, this bundle's prerequisite), the **`scan_page` seam** (#634), **segmentation** (#635),
  the **staged-byte protection class** (#637) and the **S3 verbs** (#508) — this slice must consume
  them rather than re-shape them; the **staged-reference-set profile preflight** in GC, restore,
  drain status, scrub and staged reconstruction (**#806**, see leg H; ⚑ for the human: 0016 makes
  that guard part of the protocol, so #806 belongs in the same release gate as #625 and #633,
  before #668 exposes the verbs); the **operator session-abort verb,
  the terminal-expiry verb and the foreign-clock alarm surface** — **#633**, wave 2, which ships
  with this one (the split is deliberate and the pair is what satisfies 0016: this slice lands the
  **skip behaviour**, #633 lands the **alarm that must never let that skip be silent**,
  `0016:1989-1991`, and the two verbs that are the skipped session's only exit — neither half is
  releasable alone, which is why they are one stack); **FU-3** retirement-drain and admission telemetry/alerting (#630, filed
  separately, "with the reaper" but its own bundle); **FU-2** urgent operator-forced staged
  evacuation; **FU-4** surfacing the abandonment reason in the S3 error text; any file under
  `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016 itself; and **any change to
  the records and state machine of the slices below (#693, #894, #895, #657–#659), or #508's
  verbs and wire behaviour** — if this slice finds it
  needs one, that is a Check §6 item to raise, not a silent edit to the layer below.
- **Repro instruction:** on this bundle's **resolved** base (`main` plus the four unlanded
  prerequisites, as `Falsifiability` § *Base resolution* states; not reproducible before that
  base exists), create a temp dir, open `RedbMetadataStore::open(dir.join("meta.redb"))`,
  seed an abandoned session's records as **canonical raw JSON bytes** (or through the public
  key/record helpers on `main` — `mpu_key`, `retire_session_range`, `decode_*` in
  `crates/core/src/multipart.rs` — and the prerequisites' writers where they are `pub`) with leg A's
  idle-only timestamps (`created_at_millis = now − 14 days`, progress stamps `now − 10 days`,
  leases lapsed at `now − 9 days`: past `W_open`, inside `W_session`; a timestamp of `1` is for
  leg C's ceiling fixture only, never leg A's), plus
  `mpuctl` at `{count: 1, …}`; **drop the store** (redb holds an exclusive file lock); run the
  operator one-shot as a **subprocess** (`Command::new(env!("CARGO_BIN_EXE_wyrd"))`, flag last in
  argv); reopen the store and scan the prefixes. Nothing happens on the base: the records are
  exactly as seeded. **Mechanics the test must respect** — the test function is a plain `#[test]`
  doing its seeding and its assertions inside their own short-lived runtimes (never
  `#[tokio::test]`); the one-shot **MUST return after exactly one pass** with a **bounded
  subprocess wait** in the test — a fall-through into the role's interval loop would otherwise
  hang the whole test binary, and on the base the unknown flag must fail at parse rather than
  start a loop; and every wall-clock read the test makes
  needs the annotation the workspace lint demands (bare `SystemTime::now()` is denied by
  `clippy.toml`, wyrd#619 / `63d66b9` — state which clock owns the read).
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and should mirror:
  **the fenced control point** — `reconcile_step` (`crates/custodian/src/reconciliation.rs:65-115`)
  and its "dispatched only from the fenced control point, never a parallel entry" rule
  (`crates/custodian/src/gc.rs:124-128`). The reaper is a new loop **alongside** GC, scrub,
  reconstruction and rebalance, dispatched the same way.
  **bounded, resumable, idempotent batches** — `MARK_BATCH = 1_000` and its FoundationDB
  transaction-limit rationale (`crates/custodian/src/restore.rs:92-100`), and the partial-progress
  argument at `:278-286`. 0016 tightens it: bound by **bytes** (`E_tx/2`) **and** by operation
  count (`0016:621-652`) — the `MARK_BATCH` precedent is the count half only.
  **the same-event duplicate skip** (arm (a) only — NOT a blanket no-restamp rule) — the restore
  pass's `already`-marked skip (`crates/custodian/src/restore.rs:278-286`). `mark_orphaned`
  (`crates/custodian/src/gc.rs:110-122`) is an unconditional put and is a precedent for neither
  arm; the three-arm guard is 0016's (`:1127-1224`).
  **the clock hazard this loop must not re-enter** — `ExpiredPendingPolicy`'s cross-clock
  reasoning (`crates/custodian/src/gc.rs:77-104`), the #557 defect class, and the CLI's fixed
  logical clock (`crates/server/src/cli.rs`, `NOW_MILLIS`). This is precisely why the reaper is
  guarded by `clock_source` and why its staged reclamation is clock-free.
  **the renewal loop whose liveness the reaper reads** — `crates/core/src/write.rs:460-520`
  ("refuse rather than resurrect", half-TTL renewal). 0016 requires the renewal (and every
  `sidx:` intent) to rewrite its `slot:` record in the SAME batch; that half lands in #636 — this
  slice **depends** on it and must assert it rather than duplicate it.
  **the operator role** — `cmd_custodian` (`crates/server/src/cli.rs:913-1010`): flag parsing, the
  stray-positional refusal, the leadership election and the scoped logging dispatch; the
  valueless-flag allowlist a new valueless flag must join is `VALUELESS_FLAGS`
  (`crates/server/src/cli.rs:2244`) with its parser at `:2253-2275`. The new one-shot follows `--reconcile-after-restore`'s shape
  (`crates/server/src/cli.rs:1166-1230`) but **without** its whole-fleet requirement.
  **telemetry seam** — `DurabilityTelemetry` (declared `crates/telemetry/src/lib.rs:79-84`) and
  its in-process read-back `gather_prometheus` (`:170-180`) and the audit-event pattern
  (`crates/custodian/src/restore.rs:433-470`), for the pass's outcome counts.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  closed/rejected work on 2026-07-25 at `22d71b4`. `crates/custodian/src/reconciliation.rs` and
  `gc.rs` were last touched by #554 (`1ea566c`), #430 (`0c97685`) and #551 (`5e1e7af`); no
  reaper, no multipart-aware pass, and no `crates/custodian/src/reaper.rs` exists.
  `crates/server/src/cli.rs` has no multipart or reaper flag. No open or closed PR implements an
  abandoned-upload reaper; this issue was split out of #508 during planning and its design was
  produced by #626 (merged as PR #627, no code). No bundle in `results/` has ever built it.
  Result: **no prior art; net-new.**
- **Disposition hint:** likely-fix

Plan-review response (2026-10-15): all four findings hold at `7bf7eb9` and are revised in place.
(1) `reclaiming`: leg F gains arm (0) ahead of the three arms (never replace; keep the work; mark
under `require_absent` once GC's key is gone), with a fixture, negation (8) and a seeded DST row
for the race with GC's swap. (2) Leg A now asserts both session `retire:` ranges empty and GC's
licence condition per mark, adds a fixture proving the terminal delete waits for the obligation,
and owes a post-grace GC assertion appended to `crates/custodian/tests/gc.rs`. (3) Leg H is
narrowed to the reaper; the staged-set preflight that protects GC's allocation is **#806**'s
(open, marked at `gc.rs:1541-1546`), named out of scope, with ⚑ asking the human to put #806 in
the release gate with #625/#633. (4) `Falsifiability` replaces the #636 base with the ten
successor ids, what each supplies, which have landed (5), which are planned (#894, #895) and which
have no bundle (#657, #658, #659), and the condition (same batch, or merged first) under which RED
becomes runnable. Stale `run-verify.sh` claims (no zero-test guard) corrected: a build failure is
now UNVERIFIABLE (77).

Plan-review response (2026-10-20): both findings hold at `12dd082` and are revised in place.
(1) Leg A's fixture (and G2a's copy, and the repro) is now inside `W_session` and past
`W_open`: created 14 days ago, progress 10 days ago, leases lapsed. The defaults are bounded
(`W_open ≤ 7 days`, `W_session ≥ 21 days`, flagged for sign-off) so the test needs no new
symbol. Negation (10), the idle arm disabled, must fail A and G2a. (2) G2 now covers both
deployed shapes. The no-endpoints role exits today (`cli.rs:1083-1100`) and the fleet role runs
`run_reconstruction_until` (`custodian.rs:521-545`), so one shared reaper interval driver is
required on both, outside GC's whole-fleet gate (`:680-713`). G2a is the no-endpoints subprocess
(binding); G2b is a degraded-fleet case appended to `crates/server/tests/custodian_gc.rs`
(ci evidence). Call-path evidence goes in `build-notes.md`, with negations (11) and (12).

Plan-review response (2026-10-22): all three findings hold at `4bac538` and are revised in
place. (1) Leg H now says what it proves and what it does not: the stored profile has five
fields and no `B_ops` (`multipart.rs:1580-1586`, `:5346-5355`), so operation capacity rests on
one `MAX_BATCH_OPS`, the clamp test (`multipart_knobs.rs:167`, clamps `:5454-5457`) and the
calibration leg; no format change. (2) The empty-fleet startup keeps its panic (a skipped peer
is never re-dialed, `custodian.rs:247-263`), and runs exactly one reaper pass before it; new
binding leg G2c starts the role with an unreachable `--endpoints` and asserts the non-zero exit,
the existing diagnostic and leg A's store transition; negation (13). ⚑ The keep-alive-and-redial
alternative is left to the human. (3) The calibration leg is now a defined experiment: a new
timing test appended to each networked backend's existing `conformance.rs` (no new test file,
so C4-verify's invocation is unchanged), two worst-case batches at `MAX_BATCH_OPS` (small values;
`MAX_BATCH_BYTES`), each timed against `MAX_BATCH_OPS × SLOWEST_OP_MILLIS`, run by the existing
xtask legs, first-attempt figures recorded. Within the bound only the doc comment changes;
over it, no constant changes and a §6 item goes to the human.

## Motivation

The multipart slices make staged parts **live, referenced state** while their session lives —
#636 writes the records, and #637's staged protection set is what stops GC from eating an
in-flight upload. The consequence is that an abandoned session pins storage *deliberately* rather
than by accident, and nothing in those slices ever un-pins it: a client that presses Ctrl-C, or a gateway that dies holding a fenced `Completing`
session, leaves records, bytes and an admission slot held forever. #508's own acceptance text
asks for "aborted/**abandoned** uploads leave no permanently-orphaned fragments"; this is the
abandoned half. 0016 goes further and makes it a **hard ordering requirement**: the protocol has
states whose only exit is this loop, so without it those states are absorbing and `MAX_SESSIONS`
of them is a permanent `503 SlowDown` that no in-system actor can clear.

## Design

**The design is 0016 decision 6** (`0016:1794-2277`), with decision 5's reclamation and the
`Definition of done` at `0016:2942-2945`. The algorithm is written out as pseudocode at
`0016:2163-2228`; implement that pass, in that order, with those preconditions.

### The issue's own design constraints, and where 0016 answers them

The constraints recorded on the issue during #508's planning are all **subsumed** — read them as
rationale, not as an alternative design:

| Issue constraint | 0016's answer |
|---|---|
| 1. Detect by absence of PROGRESS, not by age (#557 class) | the idle arm's progress observable; **and** an *administrative* `W_session` ceiling (arm ii) that is deliberately age-based — legitimate because it is a residency bound, not a liveness judgment, and because both arms are gated by `clock_source` |
| 2. No progress marker on the session; derive it | `max(created_at, psum committed_at, slot reserved_at)` over bounded per-session ranges — the session record is never written by a part commit |
| 3. Progress alone is not liveness (the in-flight-part hazard) | the owned `sidx:` lease **and** the `slot:` lease, both renewed in flight by the half-TTL loop; plus the `slot:` record that exists *before* the first chunk |
| 4. Reuse #636's state machine | the reaper CASes the same fenced transitions and drives the same teardown path |
| 5. Teardown bounded and crash-safe | byte- **and** operation-budgeted batches, cursor inside the obligation, session record deleted last |
| 6. Watch-record lifecycle | **there is no watch record**: the reaper is stateless per pass and re-derives its work from durable records; the only cursors live inside the `retire:` obligation and the session record |
| 7. Window default: conservative and configurable | `W_open` / `W_session` days-scale deployment defaults, tighten-only per bucket for `W_session` |

### The C4 shape — designed deliberately, because the default one cannot work here

The issue flagged this and it is real. This project's per-fix gate applies `patch.diff` to a
clean base, keeps the **added** test file, reverts every production change, and runs it. A test
that references anything this slice adds fails to *compile* on the reverted base, runs zero
tests, and the gate reports **UNVERIFIABLE (exit 77)** — no red at all, only a §6 item
(`engine/scripts/run-verify.sh:40-56`, `:207-229`, `:521-541`; when this brief was first written
the gate scored that case as a false PASS, which is now fixed).

That bites hardest here because the honest production shape **changes a signature**: a new loop
means `reconcile_step` gains a `reaper: Option<&ReaperContext<'_>>` parameter (or `GcContext`
gains a field), and every existing caller — including
`crates/custodian/tests/gc.rs:206-214`'s 7-argument calls — moves with it. A test that called
`reconcile_step` directly could compile against **one** of the two trees, never both.

**Resolution: drive the loop through the operator command, as a SUBPROCESS.** This slice adds a
one-shot flag to the `wyrd custodian` role that runs exactly one reaper pass over the metadata
store and exits. The test runs the **real binary** — `Command::new(env!("CARGO_BIN_EXE_wyrd"))`,
which cargo provides to integration tests of the crate that declares `[[bin]] name = "wyrd"` — with
the flag as argv. That is stringly typed, so the test compiles identically pre- and post-patch and
the difference it measures is entirely in the store; and a subprocess additionally gives the test a
real **exit status** and **stderr**, which an in-process call cannot (`ExitCode` is not comparable,
and the role's own logging dispatch swallows the in-process alternative). It also sidesteps two
mechanical traps: the role builds its own tokio runtime and `block_on`s it
(`crates/server/src/cli.rs:981-982`), which panics inside `#[tokio::test]`; and redb holds an
exclusive file lock, which a separate process releases on exit.
On the base the flag is unknown — `ParsedArgs` requires a value for any flag not in
`VALUELESS_FLAGS` (`crates/server/src/cli.rs:2244`, parser at `:2253-2275`) — so the command
errors and reaps nothing; the test's **store-state** assertions are what fail, and they fail as
assertions, not as a build error. Do MUST assert on store state (not merely on the exit status)
for exactly this reason, and MUST record the RED leg's executed-test count.

Three constraints follow, and all three are normative for this slice:

1. The one-shot runs through `reconcile_step` — the same fenced control point the deployed loop
   uses. It is an operator entry, **not** a parallel test-only entry (`gc.rs:124-128`).
2. It must not require `--endpoints`: the reaper is record-only, so demanding a reachable fleet
   would make the operator tool useless in exactly the incident it exists for (and would make the
   test need a fleet it does not need).
3. The deployed loop must dispatch the reaper too, every interval — the one-shot is *additional*
   surface, never the only wiring.

### What the next slice needs from this one

**#633 (wave 2) builds on this bundle**: its operator-abort and terminal-expiry verbs act on the
states this loop defines, and its alarm is emitted from this loop's foreign-clock skip path.
Its test will call the reaper entry this slice lands (`reconcile_step`'s new parameter and the
`ReaperContext` shape) **as base-visible symbols**. So: give `ReaperContext` a stable, public
shape, and do not leave the reaper's dispatch reachable only through the CLI.

## Alternatives considered

Recorded in 0016 and **not reopened**: reaping by owned-lease expiry (rejected — an `Open`
session whose leases lapsed is a stalled but live client); a per-session watch/heartbeat record
(rejected — a record class with no deleter, and it would serialize the part path); a global
`scan("pending:")` backstop for owned residue (rejected — `ScanCapExceeded` halts the whole
maintenance plane); bounding session life with `W_open` alone (rejected — a live, silent-but-
progressing session is bounded by nothing, so a drain behind it stalls indefinitely);
asynchronous collection **without** an admission bound (rejected — a collector establishes no
cardinality bound; both are required). One alternative is this brief's own: driving the binding
test through a new Rust entry point instead of the operator command — rejected above on the C4
gate's mechanics, and it would also have meant a test-only parallel entry the repo forbids.

## Impact & compatibility

- **No new record class and no format change.** The reaper reads and deletes what #636 writes;
  its only durable writes are the fenced state transitions, the `orphan:` marks, the `retire:`
  cursor advances and the deletions — all shapes #636 already defines.
- **`reconcile_step`'s signature widens** (or an existing context grows), touching every caller
  including the custodian test suite. That is a deliberate, reviewable ripple; keep it mechanical.
- **A new operator flag** on `wyrd custodian` — hence a docs-currency obligation in the same PR.
- **Operationally**: an abandoned upload now disappears after `W_open`, and *every* session after
  `W_session`. Both are client-visible as `404 NoSuchUpload` on the next verb (0016 decision 3);
  FU-4 will surface the reason in the error text.

## Open questions

1. **Knob defaults.** `W_open`, `W_session`, `W_completing`, `W_tombstone` and the cadence are
   this slice's to choose inside 0016's ranges. Propose days-scale `W_open`/`W_session` (S3's own
   posture is that nothing is auto-aborted without a lifecycle rule) and state the chosen values
   in `build-notes.md` with the range cited. **Bounded at the 2026-10-20 revision:** `W_open ≤ 7
   days`, `W_session ≥ 21 days`, so leg A's fixture (created 14 days ago, idle for 10) is reapable
   by the idle arm only. The maintainer may retune at sign-off; a retune outside those bounds must
   move leg A's timestamps with it. ⚑ This bound is a Plan choice, not the human's — confirm or
   redirect at sign-off.
2. **The one-shot's name.** Pick a clear operator-facing flag and use the *same* spelling in the
   test, the runbook and the architecture doc.
3. **FU-3 telemetry (#630)** is a separate bundle. This slice should still emit the pass's outcome
   counts on the existing durability seam; it must not build the alerting surface.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before sign-off
accepts, and this issue MUST NOT be closed without #633 landing alongside it.
