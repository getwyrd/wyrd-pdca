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
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-25. **This bundle builds on
> #636's accepted patch, not on bare `main`** — see `Depends on` / `Ordering note`.
>
> **RE-POINTED 2026-07-26.** #508 was re-planned as five slices (634 → 635 → 636 → 637 → 508)
> after its seventh attempt was rejected on reviewability. Everything this loop reads now lands in
> **#636** (the commit protocol in `crates/core`); #508 is reduced to the S3 wire surface. Below,
> "#508" has been replaced by the slice that actually lands each thing — except where it means the
> **release order** (this bundle still lands with or before #508), which is unchanged.

- **Slug:** multipart-reaper
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
  pair and crashed in-flight `sidx:` residue with a lapsed lease, is torn down: after the pass(es)
  converge, `mpu:<id>`, `part:<id>:`, `psum:<id>:`, `sidx:<id>:` and `slot:<id>:` are **all
  empty**, `mpuctl.count` is back to its pre-session value, and an `orphan:` mark exists for
  **every fragment position of BOTH byte sets** — the `sidx:` residue's recorded `staged`
  placement **and every placement position of the committed `part:` record's chunks** (the
  `retire:bytes:{session}` obligation the reap fence installs, `0016:664`). Assert the committed
  part's positions explicitly: a design that marks only the residue, deletes the `part:`/`psum:`
  records and decrements the counter satisfies every *other* assertion in this leg while leaving
  the committed part's fragments unreferenced **and** unevidenced — which on this base means
  retained forever with nothing naming them (`crates/custodian/src/gc.rs:180-190`). Marks always
  precede the deletion of the records that name the bytes (`0016:1121-1125`). This is the binding
  red→green leg.
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
  **(F) Teardown is crash-safe, idempotent, and obeys the THREE-ARM mark rule.** Running the pass
  twice over a partially drained obligation converges to the same terminal state with **no
  double-decrement** of `mpuctl.count`. For pre-existing `orphan:` marks, assert 0016's three-arm
  rule (`0016:1127-1160`, iteration-7 finding 3 as corrected by iteration-9 finding 3 and
  iteration-13 finding 1) — **not** a blanket "never re-stamp", which 0016 explicitly rejects as
  "the concurrency half alone": (a) a mark carrying the **same** unreference-event identity as this
  walk is **skipped, byte-for-byte unchanged** (assert the stored value before and after — a
  re-stamp there restarts the grace window and can postpone reclamation indefinitely); (b) a mark carrying a **different unreference-event identity — including the legacy
  *parseable* bare-decimal value, which carries no identity** (`0016:1190-1224`) — is **replaced
  with a fresh stamp** under the exact-value guard (assert the value *does* change: leaving an
  already-expired mark from an earlier event in place would let GC reclaim with no grace at all
  once the last reference goes); (c) an absent position is written under `require_absent`. Seed
  one fixture per arm. **A fourth case is NOT a re-stamp arm: a value that does not decode at all
  fails closed** — the pass leaves it untouched, classifies and surfaces it, and does not act on
  it (ADR-0045's metadata-validation boundary, `docs/design/adr/0045-metadata-validation-boundaries.md:42-65`;
  rewriting corrupt metadata is the one thing a maintenance loop may never do). Note the base
  helper is a precedent for neither arm: `mark_orphaned` is an unconditional plain put
  (`crates/custodian/src/gc.rs:110-122`).
  **(G2) The DEPLOYED loop reaps — not only the one-shot.** Every other leg drives the operator
  one-shot, and an implementation that wires the reaper **only** into that flag — leaving the
  ordinary interval path untouched — passes all of them while reaping nothing in production
  (the role's no-endpoints branch returns before the loop, `crates/server/src/cli.rs:1048-1065`;
  the deployed loop is a separate branch at `:1240-1258`). So: start the **ordinary custodian
  role** as a subprocess with a short `--interval-secs`, against a store seeded exactly as leg A,
  let it run one bounded interval, terminate it, and assert the **same** store transition — with
  **no one-shot flag** anywhere in its argv. Without this leg the `Production reach` field below
  is false.
  **(G) Tombstone expiry.** A `Completed` session (local clock) whose `completed_at_millis` is
  older than `W_tombstone`, with no session-scoped `retire:` obligation left and an empty
  `sidx:` range, has its record deleted and `mpuctl.count` decremented — the observable #636
  cannot produce on its own (its Complete leaves a live tombstone).
  **(H) A profile mismatch fails CLOSED, before the reference set is built.** Seed `mpuctl` whose
  stored `profile` differs from the custodian's local configuration and assert the pass **emits the
  operator signal and mutates nothing** — no fence, no mark, no deletion, no counter change — while
  the matching-profile fixture in leg A still reaps (`0016:2061-2070`, iteration-13 finding 7: the
  process that actually holds `W_ref` worth of chunk-refs is the reconcile pass, so checking only
  admitters protects the wrong process). Without this leg the comparison can simply be absent and
  every other leg still passes; its failure mode is the maintenance host OOMing under a rolling
  configuration change.
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — the folded
  `origin/pdca-integration/main` carrying #634, #635 and #636 — with no deploy stack, no fleet
  and no container. On that base every multipart record class, the `Open`→`Aborting` fence and the
  retirement ledger exist (**#636** landed them; `scan_page` is #634's and the segmented map
  #635's, both transitive) but **nothing drives a session the client abandoned**: legs A,
  C, D, E(second half), F and G all fail on their durable-state assertions because the records
  are still there. Leg B passes on the base **vacuously** (nothing reaps anything), so it carries
  no red — say so, and evidence its force by the negation runs in `Verification posture`.
  **Base resolution is a gate-evaluability precondition, not a detail:** this is a **wave-1**
  bundle, so the driver stamps its stack base and exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`, `gates.py:352-360`), which
  `run-verify.sh` honours ahead of the brief's base (`engine/scripts/run-verify.sh:186-192`).
  If that export is missing, C4-verify resets to `origin/main`, #636's records do not exist, the
  added test **fails to compile**, and the RED leg's failure branch — which has **no zero-test
  guard on the non-zero path** (the `TESTS_RAN == 0` check at `:416-427` is inside the
  cargo-*succeeded* branch; a compile failure skips that block entirely and falls through to the
  unconditional PASS at `:433`, see `:415-434`) — so it prints "PASS — red without the fix" over a
  build that ran nothing. Do MUST record, from the RED leg, **how many tests actually
  ran and failed**; a run reporting zero tests is a non-result, not a pass, and Do must say so.
  **Corollary the test must obey:** the added test file may reference **only symbols that exist
  on this bundle's base** (i.e. `main` + #634 + #635 + #636) — nothing this slice adds. That is what makes the
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
- **Depends on:** 636, 637
- **Conflicts with:**
- **Ordering note:** **RE-POINTED 2026-07-26 — #508 was re-planned as five slices, and this
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
  structure #637 defines. It also rewrites `crates/custodian/src/gc.rs` and the `orphan:` mark
  rules that #637 restructures. A bare conflict would have been oriented by **name order**, putting
  `issue_625` *ahead* of `issue_637` and handing #637 a base whose `reconcile_step` this bundle had
  already widened — which would break #637's base-compiling test file and destroy its seven
  assertion REDs. The dependency fixes the direction: **637 first, then 625.**
  The **release** direction is unchanged and is normative in 0016 (`0016:234-268`): the reaper
  must be running wherever the verbs are exposed, so **#625 and #633 are merged with or before
  #508**, bottom-up as one stack. Under the old plan that was in tension with "depends on #508";
  under the split it is not. Per the issue thread, **this issue must not close without #633
  landing alongside it** — #633 depends on this bundle.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`, `fdb cluster healthy`
  — nothing beyond the base toolchain is needed for the BINDING criterion: every leg runs
  in-process over a redb store in a tempfile temp dir, and the reaper is **record-only**
  (`0016:1913-1915`), so no D-server fleet and no topology is required to go red→green. The
  four tokens (each a registered doctor row) are for the **one off-Check obligation this slice
  cannot honestly discharge on redb**, and they are the *complete* prerequisite set for the
  FoundationDB conformance leg — the production backend (ADR-0042) — which needs a container
  runtime **and** the system client library at build time, not Docker alone: `B_ops` must be calibrated so a batch's sequential round
  trips fit the 5-second half of the transaction envelope **on the slowest supported backend**,
  and 0016 makes that a per-backend *conformance* property, not a DST one (`0016:621-648`,
  `:2907-2910`). Redb cannot evidence it. See `Verification posture` for who runs that leg and
  when; a `B_ops` chosen only against redb can time out forever on FoundationDB or TiKV, which is
  the exact non-termination the knob exists to prevent.
- **Test file:** `crates/server/tests/multipart_reaper.rs` — a **NEW**
  `crates/<c>/tests/<t>.rs` file, which this project's C4 gate requires: `run-verify.sh`
  discriminates on an **added** test file (`_is_test_file`, `engine/scripts/run-verify.sh:93`)
  and degrades to a green-only, proves-nothing branch otherwise (`:392-402`). It lives in
  `crates/server` because the operator command it drives does (`crates/server/src/cli.rs`), and
  it must **not** be `#![cfg(...)]`-gated: the gate reads crate-level cfgs off the added sources
  and applies the resulting `RUSTFLAGS` to the whole invocation (`:347-366`). **Every DST
  regression this slice owes (below) therefore goes into an EXISTING `crates/dst/tests/*.rs`
  file** — a modified file, which the gate does not add to its invocation, and which `cargo xtask
  ci` still runs. Do MUST NOT add a new file under `crates/dst/tests/`.
- **Verification posture:** flippable red→green at Check for legs A, C, D, E(clock-free half), F
  and G. Leg B is **vacuously green on the base** and carries no red, so its force must be
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
  (7) re-stamp an existing `orphan:` mark ⇒ leg F's grace clock restarts. A negation that does
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
  implementation passes; and a **crash at each drain step**, asserting convergence to zero staged
  records, no mark re-stamped, and the counter at its true value (`0016:2876-2905`). They are
  `cargo xtask ci` evidence, not C4-verify evidence — do not move the binding test there.
  **One pre-declared off-Check leg:** the `B_ops` per-backend timing evidence (see `External
  dependencies`) — a batch at the byte budget must commit inside the 5-second half of the envelope
  on the slowest supported backend, which redb cannot evidence. Do runs it through the existing
  `cargo xtask fdb-conformance` / `tikv-conformance` legs where the host allows and records the
  measured per-batch commit time in `build-notes.md`; where the host cannot, **Eduard Ralph
  confirms it before merge and records it in SUMMARY §9**. What IS built and exercised at Check is
  the budget mechanism itself (the batch splitter and its bound, unit-tested here) — nothing
  deferred is unbuilt.
- **Production reach:** the live path is exercised at Check — **and leg G2 is what makes that
  claim true rather than asserted**: it drives the ordinary custodian role, with no one-shot flag,
  and demands the same store transition. The one-shot and the deployed loop dispatch the **same**
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
  custodian-side **profile validation** — the reaper compares its local budget profile against
  `mpuctl.profile` and **fails closed with an operator signal before building the staged
  reference set** (`0016:2061-2070`) — **reading the budget-profile configuration seam #636 lands,
  unchanged**; the knob values this slice owns: the **reaper's own windows** `W_open`,
  `W_completing`, `W_session`, `W_tombstone`, the pass cadence, and **`G_orphan`** — which 0016 assigns here explicitly
  ("#625 (`G_orphan`, today's grace window)") together with the **strict** inequality
  `G_orphan > W_write + δ_clock`, and, where a repoint can intervene,
  `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, restated in the clock-lifecycle
  table `:2497-2509`). The inequality is strict because GC's grace check is inclusive
  (`crates/custodian/src/gc.rs:172-176`): at `G_orphan == W_write` a fragment authorized before a
  fence can land in the very tick its evidence becomes reclaimable. Ship a regression that pins
  that boundary tick. **`W_ref`, `B_bytes`, `B_ops`, the profile tuple and `MAX_SESSIONS` are NOT
  this slice's to choose — they are ONE value set landed by **#636** in an earlier wave and
  consumed here unchanged.** The ledger's `mpuctl.profile` is written by Create in #636;
  `MAX_PART_CHUNKS` / `MAX_INFLIGHT_PARTS` are clamped by `B_ops`; a reaper that derived its own
  values would fail its own profile check against every session #636 admitted, and a *lower* local
  `B_ops` would make this loop's **unsplittable** reap fence and terminal delete time out forever
  on sessions #636 legally admitted. 0016's *Open questions* nominally assigns `W_ref`/`B` here, but a slice cannot consume a
  later wave's choice, and split budget authority admits a permanent failure class
  (a legally-admitted session whose unsplittable reap fence no longer fits is never reaped and
  never reclaimed) — which Wyrd does not accept as a cost. **SETTLED by Eduard Ralph,
  2026-07-25**; the knob-value authority now sits in **#636**'s brief (§ *The knob values*), which
  records the resolution and the reasoning. Consume
  the seam; if its values are wrong, that is a Check §6 item against the stack, never a local
  override. Each value inside the
  range 0016 settles and citing that range at its definition, with generous (days-scale)
  `W_open`/`W_session` defaults; wiring into the deployed `wyrd custodian` role so
  the pass runs every interval; **an operator one-shot** on that same role that runs exactly one
  reaper pass against the metadata store and exits, **without requiring `--endpoints`** (the
  reaper reads no fleet) — this is both the runbook tool and the test seam; and the **living
  architecture docs** (`docs/design/architecture/06-runtime-view.md`,
  `08-crosscutting-concepts.md`) updated for the reaper loop and the new CLI flag — a **merge
  requirement** in this repo (`AGENTS.md` "Docs currency"), not a follow-up.
  / out of scope: the **record classes, the state machine and the retirement-ledger installation**
  (**#636**, this bundle's prerequisite), the **`scan_page` seam** (#634), **segmentation** (#635),
  the **staged-byte protection class** (#637) and the **S3 verbs** (#508) — this slice must consume
  them rather than re-shape them; the **operator session-abort verb,
  the terminal-expiry verb and the foreign-clock alarm surface** — **#633**, wave 2, which ships
  with this one (the split is deliberate and the pair is what satisfies 0016: this slice lands the
  **skip behaviour**, #633 lands the **alarm that must never let that skip be silent**,
  `0016:1989-1991`, and the two verbs that are the skipped session's only exit — neither half is
  releasable alone, which is why they are one stack); **FU-3** retirement-drain and admission telemetry/alerting (#630, filed
  separately, "with the reaper" but its own bundle); **FU-2** urgent operator-forced staged
  evacuation; **FU-4** surfacing the abandonment reason in the S3 error text; any file under
  `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016 itself; and **any change to
  #636's records and state machine, or #508's verbs and wire behaviour** — if this slice finds it
  needs one, that is a Check §6 item to raise, not a silent edit to the layer below.
- **Repro instruction:** on this bundle's base (`origin/pdca-integration/main`, i.e. `main` +
  #634 + #635 + #636), create a temp dir, open `RedbMetadataStore::open(dir.join("meta.redb"))`,
  seed an abandoned session's records as **raw JSON bytes** (or through #636's public key/record
  helpers if they are `pub`) with `created_at_millis` / `committed_at_millis` / `reserved_at_millis` set
  to a small value (e.g. `1`) so that **every** window has elapsed against the wall clock, plus
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
clean base, keeps the **added** test file, reverts every production change, and calls a non-zero
`cargo test` exit "RED" — with **no zero-test guard on that branch**
(`engine/scripts/run-verify.sh:415-434` — the unconditional PASS is at `:433`). So a test that references anything this slice adds
fails to *compile* on the reverted base and the gate reports a false PASS over a build that ran
nothing.

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
   in `build-notes.md` with the range cited. The maintainer may retune at sign-off.
2. **The one-shot's name.** Pick a clear operator-facing flag and use the *same* spelling in the
   test, the runbook and the architecture doc.
3. **FU-3 telemetry (#630)** is a separate bundle. This slice should still emit the pass's outcome
   counts on the existing durability seam; it must not build the alerting surface.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before sign-off
accepts, and this issue MUST NOT be closed without #633 landing alongside it.
