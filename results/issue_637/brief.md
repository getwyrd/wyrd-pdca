# Design proposal — issue 637 / staged-byte-protection

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on
> `origin/main` @ `22d71b4`. **Decision 2 (`0016:765-893`) IS this slice's design**, and it is
> written as a **per-consumer table** (`0016:820-871`) followed by a **failure-mode table**
> (**`0016:874-890`** — corrected 2026-07-26; an earlier revision cited `:2140-2160`, which is
> capacity arithmetic and admission/retire progress, not this table) that enumerates every way to
> implement it wrong and the observable that catches each. Read also: the GC / write summaries
> `0016:2688-2717`, decision 7(e)'s bounded segmented resolution `0016:2470-2478`, and the tests
> the slices owe `0016:2876-2939`. **Do MUST read decision 2 in full, table included, before
> writing code.** This brief does not restate it; it scopes the slice, settles the one design
> call 0016 leaves open (GC's per-pass page budget), and states the C4 shape — which for this
> slice is unusually strong, because almost every leg has a **real, assertion-shaped red**.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26.
> This is **seam (iv) of five** in #508's re-plan (634 → 635 → 636 → 637 → 508): the maintenance
> plane, landed on its own because it changes **destructive** passes and deserves a review that
> is not competing with a wire surface for attention.

- **Slug:** staged-byte-protection
- **Kind:** enhancement (design proposal)
- **Goal:** durable-but-unpublished bytes — a client's uploaded parts — become a **first-class
  protection class** across the whole maintenance plane. Today nothing references them, so every
  destructive pass is entitled to reclaim them: GC deletes them given any evidence, restore
  **marks them stranded**, and a drain reports a server holding nothing but staged bytes as
  `Satisfied` so an operator wipes it under a live upload. `ReferenceSet`
  (`crates/custodian/src/gc.rs:228-247`) gains a second, **disjoint** member — the staged set,
  built from committed `part:` records **and** in-flight owned `sidx:` entries through **bounded
  per-session ranges, never a global scan** — and each of the seven consumers in 0016's table
  makes its own stated decision from it.
- **Success criterion:** **one NEW test file** plus seeded DST cases appended to an existing
  one (see `Test file` — the split is what protects the evidence). The new file is written to
  compile on this bundle's base so that **its first seven legs red by assertion, not by build
  failure**.
  `crates/custodian/tests/staged_protection.rs`, over in-memory trait stores, with a session's
  records seeded through #636's key helpers (present on this bundle's base) and fragments placed
  on in-memory D-server doubles:
  **(A) GC — protect, with the evidence present.** With an `Open` session holding a committed
  `part:` record and an in-flight owned `sidx:` entry, seed an `orphan:` record for one fragment
  of each **and advance past the grace window**, then run `reconcile_step`: **both fragments
  survive**. This is 0016's own oracle (`0016:877`, "leave the reference set committed-only")
  and the evidence is what makes it discriminating — without the `orphan:` record GC's
  conservative branch retains an unevidenced fragment anyway (`crates/custodian/src/gc.rs:183-187`),
  so the leg would pass vacuously on the base. **On the base, with the mark present, GC deletes
  them — that is the red.**
  **(B) Drain / desired-state — count both classes as held.** For a server holding **only** an
  in-flight owned (`sidx:`) fragment, with `desired:dserver:<S>` seeded,
  `reconciliation_status(S)` MUST answer **`Pending`** (`crates/custodian/src/desired_state.rs:150-170`).
  On the base it answers `Satisfied` — the F6 wipe trace, and the one row whose violation is an
  operator wiping live bytes. Assert the same for a committed-`part:` fragment, separately: an
  implementation that counts only `part:` passes one and fails the other (the iteration-3
  finding-4 hole).
  **(C) Restore — protect, then fence, and prove the data loss it prevents.**
  `reconcile_after_restore` over a store with a staged session MUST report
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:104-145`) and must have
  **skipped** those fragments as staged rather than merely not reached them. `staged_skipped` is a
  field this slice ADDS beside `pending_skipped` (`:256-263`), so naming it directly would break
  this file's base compile and destroy the assertion red — assert it in a **base-compiling** form
  instead (e.g. over the report's `Debug` rendering, which carries no such field on the base and
  does after the fix). If Do can find no base-compiling form that genuinely binds, drop the
  counter assertion from this file and say so in `build-notes.md`: `stranded_marked == 0` plus the
  survival assertion below is the binding pair, and it must not be traded away for a counter.
  On the base restore marks every staged fragment `orphan:`
  (`crates/custodian/src/restore.rs:217-300`) — so **also** assert the consequence: after that
  restore pass, advance past the grace window, run a GC pass, and assert **every staged fragment
  is still present**. On the base they are deleted. That is the leg that shows this slice
  prevents data loss rather than tidies a counter.
  **And "then fence" must be an OBSERVABLE, not prose.** D-B requires restore to fence **every**
  resurrected `Open`/`Completing` session to `Aborting`, because a records-only image cannot prove
  the staged bytes still exist (`0016:836-841`). Assert on durable state, for **both** shapes:
  an `Open` session is `Aborting` afterwards; a **`Completing`** session that had already written
  `seg:<g>:<E>:*` takes the dedicated **restore-fence transition** whose single batch installs
  `retire:bytes:{session, parts}` **and** `retire:records:{seg:<g>:<E>}` (X57 — fencing it as if it
  were `Open` leaves those `seg:` records with **no deleter in the whole design**); the
  `sessions_fenced` counter moved; and a Complete retried against a restored session is **rejected**
  rather than publishing. Without these, "protect, then fence" is satisfied by protect alone.
  **(C2) This slice OWNS the restore-fence generation record — decided 2026-07-26.** 0016 requires
  that "the restore fence generation MUST complete before any gateway serves multipart verbs on the
  restored image" (`0016:823`), and two independent plan reviews found that requirement owned by no
  slice: this brief fenced sessions but published nothing a gateway could read, and #508 turned the
  absence into a Check §6 item — passing the hole downstream. **It lands here.** Ship a single
  **durable, authoritative generation/completion record** that a gateway can read without inferring
  anything: `reconcile_after_restore` advances it, and it distinguishes **incomplete/stale** from
  **complete**. Assert all three arms on durable state: absent (no restore has run) ⇒ readable as
  not-complete; a restore pass **in progress / not yet fenced** ⇒ not-complete; after the fence
  generation completes ⇒ complete, and the value identifies **which** restore it belongs to (a
  monotonically advancing generation, so a *later* restore invalidates an earlier completion rather
  than being masked by it). #508 consumes exactly this record — it must not derive a second source
  of truth, and this slice must not leave it implicit.
  **(D) Scrub — verify AND enqueue.** A **corrupt** staged fragment (a bit-flip into a real v1
  fragment, the idiom `crates/custodian/tests/scrub.rs` already uses) MUST result in a durable
  `repair:` obligation. "Walks it" is not a passing answer: on the base scrub never sees it at
  all (it iterates `referenced.placed`, `crates/custodian/src/scrub.rs:75-110`), so assert the
  **positive** — the queued obligation exists and names that chunk.
  **(E) Reconstruction — resolve and repair, under the session fence.** A lost staged fragment
  MUST be rebuilt **and the `part:` record's `ChunkRef.placement` updated**, under the
  destination-pre-mark rule: pre-mark `orphan:<P_new>` **before** writing the destination
  fragment, then CAS the part record under `require(mpu == Open@E)` **and**
  `require(part:<id>:<n> == prior)`; **on win** adopt `P_new` (delete the pre-mark) and orphan
  `P_old`; **on loss** it is a no-op that **leaves the `P_new` pre-mark standing** so GC reclaims
  the pre-written destination (`0016:854-861`). On the base the obligation resolves to no
  committed map and is silently dropped (`Assessment::Drain`,
  `crates/custodian/src/reconstruction.rs:188-191`).
  **A changed placement is NOT sufficient — it proves metadata moved, not that a byte was
  rebuilt.** An implementation that repoints the `part:` record without writing a fragment passes a
  placement-only oracle and leaves the object one copy short. Assert the whole protocol: the
  **new** D server actually holds a fragment that is **intact and scheme-correct** for that chunk;
  the `part:` record names **that** holder; the destination pre-mark `orphan:<P_new>` has been
  **removed** on the win; `P_old` is **newly orphan-evidenced**; and the repair obligation drains
  **only** on the win. Keep the loss-branch assertions separately: on a lost CAS the `P_new`
  pre-mark **stands** and the obligation stays queued.
  **(F) Rebalance — disjoint, and consistent with (B).** For a draining server holding **only**
  staged fragments the evacuation plan MUST be **empty** while `reconciliation_status` is
  **`Pending`**. The pair is the assertion: a design that merged staged fragments into `placed`
  instead of keeping the set disjoint makes those two answers contradict each other
  (`0016:880`). A committed **segmented** object's fragments, by contrast, ARE evacuated —
  assert that too, via #635's `seg:` records.
  **(G) The ledger walk is bounded per pass, and it converges.** With a `MemMeta` whose `scan`
  enforces a **lowered** cap (the `crates/metadata-redb/tests/scan.rs:9-11` idiom, implemented in
  the test's own double — `ScanCapExceeded` is base-visible in `wyrd_traits`), seed an `orphan:`
  population **past** that cap and assert: `reconcile_step` **succeeds** (on the base
  `orphan_leases` returns `Err` from its single `scan`, `crates/custodian/src/gc.rs:322`, which
  `?`-propagates and aborts the whole reconcile step before GC, scrub, reconstruction and
  rebalance run, `crates/custodian/src/reconciliation.rs:78-85`); **no single pass materialises
  the whole ledger** (assert the per-pass page budget is respected — see
  `Design § the page-budget decision`); and **repeated passes converge**.
  **The convergence fixture must be shaped so a cursorless loop FAILS it.** A population in which
  *every* entry is actionable is passed by an implementation that restarts at the first page every
  time: it consumes that page, and the next invocation simply exposes the following one. So seed a
  **retention-safe head** — a run of marks that are **within** their grace window, larger than one
  pass's page budget — followed by an **actionable tail** beyond it. A first-page-only walk never
  reaches the tail and never converges; a cursored walk processes the tail within a bounded number
  of passes. Assert that bound explicitly, and assert the continuation survives whatever
  restart/context boundary the design chooses.
  **(H) The three `orphan:` value variants all decode, and none is ever rejected**: the legacy
  bare decimal (`crates/custodian/src/gc.rs:110-122` writes exactly that today),
  `{ orphaned_at_millis, event }`, and `{ …, reclaiming: true }`. Seed one of each and assert
  every consumer handles all three. A value that does **not** decode at all fails **closed** —
  the pass leaves it untouched, classifies it, and surfaces it (ADR-0045's metadata-validation
  boundary, `docs/design/adr/0045-metadata-validation-boundaries.md:42-65`: rewriting corrupt
  metadata is the one thing a maintenance loop may never do).
  **(H2) The GC protocols decision 2 names get binding legs of their own — "handles it" is not an
  assertion.** Each of these is a distinct normative rule with its own observable, and none is
  implied by legs A–H: **(a) keyed pending-retirement protection (X97)** — while a
  `retire:bytes:{generation}` obligation is pending, its fragments are protected by an **O(1) keyed
  lookup**, not by expanding the obligation's whole prefix; assert with a store double that
  **fails the test** if the pass ever scans the obligation prefix wholesale, and assert protection
  still holds under a backed-up drain. **(b) reclaim restart (X86)** — crash between the
  `reclaiming` CAS and `delete_fragment`, then re-run: the pass resumes and completes exactly once,
  with no double delete and no stranded `reclaiming` key. **(c) the fragment-less mark sweep
  (X87/X96)** — a position no `list_fragments()` reported, aged past
  `W_repoint + W_write + δ_clock` and observed **absent after that deadline**, is cleaned up; a
  **stale listing** must not cause a live fragment's mark to be swept. **(d) the orphan-identity
  migration gate (X92) — and note this is the OPPOSITE of what an earlier revision of this brief
  said.** 0016 requires that a mark carrying a **different** unreference-event identity **IS**
  re-stamped with fresh identity and grace (`0016:1222-1224`); the migration concern is not
  "never re-stamp a legacy value" but that **identity-keyed retirement stays DISABLED until a
  durable `orphan:`-identity cleanup has completed** (`0016:1259-1273`). So the leg is two-armed:
  **before** the durable cleanup-complete marker is observed, identity-keyed retirement is
  disabled / fail-safe; **after** a bounded cleanup writes that marker, it is enabled and a
  different-or-legacy event receives the fresh identity 0016 requires. Assert both arms and the
  marker itself.
  **(I) Reclamation intent precedes destruction.** The sweep CASes `orphan:<pos>` to `reclaiming`
  and **commits before** `delete_fragment`, deleting the key afterwards as today
  (`0016:2686-2688`). Assert with a store double that fails the commit: the fragment must still
  exist. This is the ordering the adoption CAS's precondition depends on.
  **(I2) The source-before-destination handoff order is normative, and it needs its own cases.**
  Decision 2 makes the read order `sidx:` → `part:` → committed inodes **normative**
  (`0016:782-800`), because a build that reads a destination class before its source can observe a
  chunk in **neither** — the F6 trace, and one handoff further out, **X67**'s
  `part:`-before-inode variant. Add seeded cases for **both** handoffs to the existing DST target,
  each interleaving the atomic move against the reference build. And run 0016's **classification
  sweep** (the at-least-one-safe-class helper this protocol earns, `0016:2906-2921`) **after every
  scenario in legs A–I** — asserting no gaps, never a partition, since the protocol deliberately
  overlaps protection across both handoffs. If #636 already shipped that helper, consume it;
  if not, that is a Check §6 item, not a reason to skip the sweep.
  **(J) Seeded DST for the two races this slice owns**, appended to the **existing**
  `crates/dst/tests/custodian.rs` (the Tier-0 custodian property campaign) — **NOT a new DST
  file**; see `Test file`, where the reason is mechanical and load-bearing for this slice's
  evidence: **(i)** the
  drain-request-versus-intent fence (X59, `0016:2588`): select a placement naming `S` from a
  pre-drain topology snapshot, interleave the drain request and a `reconciliation_status(S)` read
  **before** the `sidx:` intent commits — the status MAY be `Satisfied` at that instant, but the
  intent MUST then fail `require_absent(desired:dserver:S)` and **re-plan**, and no fragment of
  that part ever lands on `S`; **(ii)** the staged-replace-versus-session-fence race (X29):
  interleave a reconstruction re-place **after** it wrote `P_new` and **before** its CAS, then
  fence the session — the re-place MUST return `Conflict`, leave `P_new` covered by its pre-mark,
  and leave the obligation queued.
  **(K) `cargo xtask ci` green.**
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — the folded
  `origin/pdca-integration/main` carrying #634, #635 and #636 — with no container, cluster or
  deploy stack. **This slice is the one in the stack whose red is real**: legs A, B, C, D, E, F
  and G all fail by **assertion** on that base, because every record class they need already
  exists there (#636 landed `sidx:`, `part:`, `psum:`, `mpu:` and their key helpers) while
  **nothing in the maintenance plane knows what they mean**. Use that. It is worth real care to
  keep it.
  **Corollary the test MUST obey:** `crates/custodian/tests/staged_protection.rs` may reference
  **only symbols present on this bundle's base** — `origin/main` + #634 + #635 + #636 — and
  **nothing this slice adds** — no new `ReferenceSet` field, no new `GcContext`/`ScrubContext`
  field, and no new `RestoreReport` field (see leg C for how `staged_skipped` is asserted without
  naming it). Prefer **raw record bytes and base-visible public API** for
  every observation: `reconcile_step`, `reconcile_after_restore`, `reconciliation_status`,
  `set_lifecycle`, `mark_orphaned`, `GcContext`, `ScrubContext`, `ReconstructionContext`,
  `RebalanceContext`, `Reconciled` (all re-exported at `crates/custodian/src/lib.rs:33-45`), plus
  `MetadataStore`/`WriteBatch`/`ScanCapExceeded` from `wyrd_traits`. **Do MUST NOT change
  `reconcile_step`'s signature** — a signature change would break this file's compile on the RED
  leg and convert every assertion-red into a build-error-red, destroying the evidence this slice
  is uniquely able to produce. If a consumer needs new context, add it in a way that leaves the
  7-argument `reconcile_step` call site source-compatible, or say why that is impossible in
  `build-notes.md` **before** giving up the assertion red.
  Do MUST record in `build-notes.md`, from the C4-verify RED leg, **how many tests actually ran
  and failed**, and confirm the failures were assertions. `run-verify.sh` cannot tell a build
  error from a real red on the failure path (the `TESTS_RAN == 0` guard at
  `engine/scripts/run-verify.sh:416-427` sits inside the cargo-*succeeded* branch; a non-zero exit
  falls through to the unconditional `PASS` at `:433`), so **a build-error red here is a defect
  in the test, not a pass.**
  **Base resolution is a gate-evaluability precondition.** This is a **wave-3** bundle; the
  driver stamps its stack base and `pdca gates` exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`,
  `src/pdca_harness/gates.py:352-360`), honoured by `run-verify.sh` ahead of the brief's base
  (`:186-206`). Without it the gate resets to `origin/main`, where none of the record classes
  exist and the patch does not even apply (this slice edits `crates/custodian/src/gc.rs`, which
  #635 also edits).
  **The DST leg (J) is evaluated by C4-ci, not C4-verify — and that separation is what protects
  the assertion red above.** See `Test file`: it is appended to the existing
  `crates/dst/tests/custodian.rs`, so it never joins C4-verify's invocation, and its evidence
  comes from `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1614`, the gating row), which
  sets `--cfg madsim` and `MADSIM_TEST_NUM=50`. Do not remove that file's `#![cfg(madsim)]`
  attribute — without it the file compiles to nothing and reports "0 tests".
- **Invariant to restore:** **every durable byte is, at every instant, classifiable as
  committed-referenced, staged-with-a-named-exit, or garbage-with-a-sound-reclamation-path — and
  every maintenance pass acts on that classification rather than on the absence of one.**
  The corollary that binds here: *no gaps*, not a partition — protection deliberately **overlaps**
  across each handoff, so a helper demanding disjointness would pressure an implementer to remove
  the very property that makes the handoffs safe (`0016:2906-2921`). **Source:** 0016 invariant
  (2) (`0016:138-151`) as decision 2 instantiates it per consumer (`0016:820-891`), resting on
  the custodian's written safety rule that a referenced fragment is never reclaimed
  (`0005:294-295`, enforced at `crates/custodian/src/gc.rs:159-170`) and on ADR-0045's rule that
  a maintenance loop never rewrites metadata it cannot parse. SELF-TEST: this cannot be satisfied
  by guarding one module — a filter inside `gc::reconcile` passes every GC test while restore
  strands a live upload's parts and the next GC pass deletes them (leg C is exactly that trace),
  which is why the class lives in the **shared reference set** and each consumer reads it.
- **Scope:** decision 2 implemented per consumer across
  `crates/custodian/{gc,scrub,reconstruction,rebalance,desired_state,restore}.rs` and the shared
  `ReferenceSet` — the disjoint staged set, the per-consumer answers of 0016's table, the
  reclamation-intent ordering, the three `orphan:` value variants, the `scan_page` ledger walk
  with its bounded per-pass budget, and the seeded DST races. **Out of scope:** the reaper loop
  (#625); the S3 verbs (#508); the multipart records themselves (#636); **`reconcile_step`'s
  signature** (changing it destroys this slice's assertion red and collides with #625); any file
  under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 634, 636
- **Conflicts with:** 625
- **Ordering note:** **wave 3 of the five-slice stack 634 → 635 → 636 → 637 → 508.** Both edges
  are genuine build-on dependencies: GC's ledger walk consumes **#634**'s `scan_page`
  (`0016:2688-2691`), and there is nothing staged to protect until **#636**'s `sidx:` / `part:`
  records exist. #635 is a transitive prerequisite through #636 (leg F's segmented-evacuation
  assertion needs its `seg:` records), so it is not listed separately — the wave levelling already
  places it earlier.
  **The `Conflicts with: 625` edge is load-bearing for this slice's evidence, not hygiene.**
  `pdca flow` drives **every** unfinished brief in the batch, not only the five ids a human names
  (`src/pdca_harness/flow.py:700-720`), so #625 can be co-scheduled without anyone intending it.
  #625 **widens `reconcile_step`'s signature** (its own `Impact & compatibility` says so), and the
  wave scheduler orients an undeclared conflict by name order — which would put `issue_625` ahead
  of `issue_637` and hand this slice a base whose `reconcile_step` has a different arity than its
  test file was written against. Declaring the conflict, plus #625's own `Depends on: 637`, fixes
  the direction: **637 builds first, 625 builds on it.** That is also the honest dependency —
  #625's leg H checks its budget profile *before building the staged reference set*, and the
  staged reference set is this slice's.
  **Consequently, state the base as a floor, not an equality:** this bundle's base is the fold of
  every earlier wave, **at minimum `main` + #634 + #635 + #636**. The test file must therefore call
  `reconcile_step` with **whatever arity its base provides** rather than hard-coding today's seven
  arguments — and this slice still must not change it.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **External dependencies:** `typos`, `docs-renderer`
  <br>*(Both on the field's own line — the driver reads only that line. Needed because this
  slice's docs-currency edit to `docs/design/architecture/{06,08}` is gated by the prose gates,
  which `cargo xtask ci` **warn-skips** when the tools are absent, so a locally-green docs change
  opens the PR red on the host's always-on jobs, INTEGRATION §3. Both are installed on this host.
  Nothing else: the custodian passes and the DST cases are all in-process.)*
- **Test file:** `crates/custodian/tests/staged_protection.rs` — **ONE new file**, and it is legs
  A–I. It must be under a `tests/` directory and it must be **added**: `run-verify.sh` classifies
  on an **added** `*/tests/*.rs` (`engine/scripts/run-verify.sh:92-94`, `:300-311`), so a case
  appended to `crates/custodian/tests/gc.rs` would degrade the gate to green-only and throw away
  this slice's best asset.
  **Leg J's DST cases go into the EXISTING `crates/dst/tests/custodian.rs`** — do **not** create a
  new `crates/dst/tests/*.rs`. This is not stylistic; it protects the assertion-red. `run-verify.sh`
  puts every **added** test file into **one** cargo invocation and, seeing a `#![cfg(madsim)]`
  crate root among them, applies `RUSTFLAGS=--cfg madsim` to the whole invocation
  (`:100-131`, `:344-385`) — and, worse, an added DST file that references anything this slice
  adds **fails to compile on the RED leg**, taking the *entire* invocation down with it and
  converting every one of legs A–G's assertion failures into a single build error. That would
  destroy exactly the evidence this bundle exists to produce. Appended to an existing file the DST
  cases are a *modified* path, never enter `ADDED_TESTS`, and are run where they belong:
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1614`), the **gating** C4-ci row. Do MUST
  record in `build-notes.md` that they actually ran and how many seeds each swept.
  `wyrd-custodian`'s dev-dependencies already carry `async-trait`, `bytes`, `tokio`,
  `wyrd-coordination-mem`, `wyrd-chunk-format` (for the bit-flip of leg D) and
  `wyrd-chunkstore-fs`; **no `Cargo.toml` change is needed for the tests, and none may be made** —
  a modified `Cargo.toml` is reverted on the RED leg, turning assertion-reds into build errors.
- **Verification posture:** DEFAULT — a flippable regression test, red pre-fix and green post-fix
  at Check, for legs A–G. That is unusual for this stack and it is deliberate; see
  `Falsifiability`. **There is NO third-file option**: an added test file naming anything this
  slice introduces fails to compile on the RED leg and, because `run-verify.sh` puts every added
  target into ONE cargo invocation and keeps them all on RED
  (`engine/scripts/run-verify.sh:286-311`, `:391-414`), would convert all seven assertion failures
  into a single build error. Legs H, H2, I and I2 MUST stay base-compiling (raw record bytes and
  base-visible API), or their post-fix-only oracle goes into the **modified existing**
  `crates/dst/tests/custodian.rs`. Nothing is deferred off-Check.
- **Production reach:** the live path traverses every seam this slice builds, at Check. The
  custodian passes under test are the **production** `reconcile_step` / `reconcile_after_restore`
  / `reconciliation_status` functions, not doubles of them; only the metadata store and the
  D-server fleet are in-memory, exactly as `crates/custodian/tests/gc.rs` already does it. The
  one thing that is *not* production-reachable when this slice merges is a **client-created**
  session — no S3 verb exists until #508 — so every staged record under test is seeded by the
  test. That is the intended state and it is why legs A–G are phrased over durable store state
  rather than over a request.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/custodian/src/gc.rs:228-247` — `ReferenceSet` and `protects()`, the exact structure
    decision 2 extends with the disjoint staged member.
  * `crates/custodian/src/gc.rs:251-297` — `referenced_fragments`, the shared build every
    consumer derives from (scrub calls it at `scrub.rs:43`,`:75`; `reconciliation_status` at
    `desired_state.rs:157`).
  * `crates/custodian/src/gc.rs:110-122` (`mark_orphaned` — an unconditional plain put today, and
    therefore a precedent for **neither** re-stamp arm) and `:158-198` (the reclaim decision: the
    safety gate `:159-170`, the grace window `:172-179`, the **conservative-retain** branch
    `:183-187` that makes an unevidenced fragment survive, and the `cleanup` batch / delete
    ordering leg I changes).
  * `crates/custodian/src/restore.rs:196-300` — the two-pass mark loop, its `displaced_kept`
    data-loss trap, `pending_skipped` at `:256-263`, and the bounded `MARK_BATCH` commit. The
    staged skip goes in beside the pending skip, with its own counter.
  * `crates/custodian/src/desired_state.rs:145-175` — `reconciliation_status` and
    `genuinely_holds`, which must test the **union** of `referenced.placed` and the staged set.
  * `crates/custodian/src/scrub.rs:60-115` and `crates/custodian/src/reconstruction.rs:180-200`,
    `:300-340`, `:560-620` — the verify/enqueue and assess/re-place paths, and the
    `Assessment::Drain` branch that silently drops a staged chunk's obligation today.
  * `crates/custodian/src/rebalance.rs:140-200`, `:260-290` — the evacuation plan and its repoint
    CAS; the committed-segment repoint is the same shape against a `seg:` record.
  * `crates/custodian/tests/gc.rs:26-120` and `crates/custodian/tests/scrub.rs` — the in-memory
    `MemMeta`/`MemDServer` harness and the bit-flip idiom the new test file should follow.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. No merged change has ever added a staged/second reference class: `crates/custodian/`'s
  reference set has been committed-only since M3.4 (`crates/custodian/tests/gc.rs:1-24`), and
  `git log -S"staged"` on the custodian surfaces nothing of this shape. No open PRs. The rejected
  prior art is in this harness: #508's **4th** attempt shipped a resolver used only by the read
  path while `gc.rs` and `restore.rs` iterated maps directly (restore stranded, GC then deleted a
  live object's fragments); its **7th** attempt replaced GC's `SCAN_CAP`-bounded `scan` with an
  **unbounded** `loop { scan_page(...) }` materialising every entry into one `HashMap` — trading a
  loud failure for a multi-hundred-MB heap allocation inside the custodian — and was rejected at
  sign-off on reviewability (`results/issue_508/iteration-v4/`, `iteration-v7/`,
  `results/issue_508/review-rejected.md`). Leg G exists to make that swap impossible to repeat.
- **Disposition hint:** likely-fix

## Motivation

Without this slice the maintenance plane and the multipart protocol **contradict each other**. A
GC pass concludes a staged fragment is unreferenced and deletes bytes a client is actively
uploading; a restore pass marks a live upload's parts stranded and the next GC pass makes the
loss real; an operator drains a server, is told `Satisfied`, wipes the disk, and a later Complete
publishes a map naming wiped fragments. Every one of those is 0016 outcome (c) — a published
object that references bytes that no longer exist — and each is reachable today with no
concurrency and no fault injection.

The reason a loose criterion is dangerous here is that **"does nothing" passes it**. GC reclaims
only on explicit evidence and otherwise conservatively retains (`crates/custodian/src/gc.rs:183-187`),
so "staged fragments survive a GC pass" is *true on the base* for a fragment with no `orphan:`
record. That is why every leg above either supplies the evidence that makes the pass act (A), or
asserts a **positive** consumer behaviour (B, D, E, F), or asserts the data loss the base actually
produces (C). This is the per-consumer failure table 0016 wrote for exactly this reason.

## Design

Read 0016 decision 2 and implement its table row by row. What follows is the scoping and the one
call the proposal leaves to this slice.

### In scope

`crates/custodian/src/{gc,scrub,reconstruction,rebalance,desired_state,restore}.rs`, the shared
`ReferenceSet`, **the durable restore-fence generation record of leg C2**, and the seeded DST
cases of leg J. Per-consumer, per 0016's table:

| Consumer | Decision |
|---|---|
| **GC** | `ReferenceSet` gains the **disjoint** staged set (committed `part:` **and** in-flight owned `sidx:`, built by bounded per-session ranges); `protects()` returns true for either; it also gains the pending `retire:bytes:` class; the expiry arm is unchanged (it scans only `pending:`, which no longer holds owned entries) |
| **GC, ordering** | reclamation **intent precedes destruction**: CAS `orphan:<pos>` to `reclaiming`, **commit**, then `delete_fragment`, then delete the key. All three `orphan:` value variants decode; a value that does not decode fails closed |
| **GC, ledger walk** | `orphan_leases` and the mark sweep walk with **`scan_page`**, never one `scan`, under a **bounded per-pass budget** (below). Plus the fragment-less mark sweep: positions no `list_fragments()` reported, aged past `W_repoint + W_write + δ_clock` and observed absent *after* that deadline |
| **Scrub** | verifies staged fragments **and enqueues repair** for a corrupt one — acting on the committed-`part:` subset only, because verification needs the committed EC scheme an in-flight chunk does not yet carry (`0016:775-783`) |
| **Reconstruction** | resolves and **repairs** the obligation and updates the part placement, under the destination-pre-mark + fenced-CAS rule; it does not silently re-place and it does not drain the obligation |
| **Rebalance** | leaves **staged** fragments unmoved; its answer is disjoint from the staged set. A committed **segmented** object's fragments *are* evacuated, by the same pre-mark + `require(seg == prior)` + `require(inode == prior)` rule |
| **`desired_state`** | counts in-flight owned fragments as **held** |
| **Restore** | its `pending_chunks` scan is bounded again (owned entries are disjoint, so `scan("pending:")` sees only ordinary pending); it skips staged fragments with a `staged_skipped` counter beside `pending_skipped`; and it **fences resurrected sessions** |

### The page-budget decision (this slice owns it, and it is a design call)

#508's 7th attempt replaced the `SCAN_CAP`-bounded `scan` with an unbounded
`loop { scan_page(...) }` that materialised every entry into one `HashMap` — trading a loud
failure for a multi-hundred-MB heap allocation inside the custodian. Both are wrong. The settled
shape, stated as the property Do must satisfy rather than as a mechanism:

1. **One pass reads a bounded number of ledger pages**, so its peak resident footprint is bounded
   by a named constant with its derivation in the doc comment — not by the ledger's size.
2. **A pass that did not exhaust the ledger must draw only retention-safe conclusions from it.**
   Every inference a partial read supports must err toward *keeping* bytes: an unread mark makes a
   fragment look unmarked, which GC's conservative branch retains. Any conclusion that would
   *destroy* on the strength of not having seen something — most sharply the fragment-less mark
   sweep — must be gated on having actually observed the relevant range, not on its absence from a
   truncated read.
3. **The tail must not starve.** Successive passes make progress across the whole ledger, so a
   population larger than one pass's budget still drains. Leg G asserts convergence, not just
   survival.
4. **Falling behind is operator-visible** — the drain-health signal 0016 requires
   (`0016:2929-2938`, "oldest obligation age" is one of the three that matter operationally).

### Out of scope — do not touch

* The reaper loop and every window-driven exit — **#625**.
* The S3 verbs — **#508**. The multipart records themselves — **#636**.
* Any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.
* **`reconcile_step`'s signature** — see `Falsifiability`; changing it destroys this slice's
  assertion red and collides with #625.

## Alternatives considered

* **Merging staged fragments into `ReferenceSet::placed`** — simpler, and rejected by 0016
  (`0016:767-782`, `:880`): keeping the set disjoint is what lets each consumer make its own
  decision instead of inheriting GC's. Merged, rebalance's evacuation plan and
  `reconciliation_status` give contradictory answers for a staged-only server; leg F is the test.
* **A global `part:` / `sidx:` scan to build the staged set** — rejected (`0016:890`): create
  sessions past `SCAN_CAP / MAX_PARTS_PER_SESSION` (≈104) and the build dies. The set is built
  from the bounded `mpu:` scan (≤ `MAX_SESSIONS`) and, per session, bounded `part:<id>:` and
  `sidx:<id>:` ranges.
* **Reading `part:` before `sidx:`** — rejected, and the reason is subtle enough to be worth
  restating: a part commit atomically deletes a chunk's `sidx:` entries and writes the `part:`
  record protecting the same bytes, so a build that read `part:` first could observe a chunk in
  **neither** set. **Source before destination is normative** — `sidx:` → `part:` → committed
  inodes (`0016:783-800`) — and the rule is general, not local to this build.
* **Filtering placement against the draining set without fencing it** — rejected (iteration-8
  finding 1, `0016:2699-2707`): selection reads a topology snapshot, and a drain recorded inside
  that window escapes the filter entirely. The `require_absent(desired:dserver:<S>)` precondition
  on every intent is what makes it a single-winner race instead of two independent writes ordered
  by luck. Leg J(i) is that race.

## Impact & compatibility

* **Behaviour change in destructive passes** — that is the point. The direction is strictly
  toward retention: passes protect more and destroy less, and no pass gains a new reclamation
  path except the `reclaiming` intent record, which precedes a destruction that already happened.
* **`orphan:` values gain variants.** All three shapes must decode, forever; the legacy bare
  decimal is what `mark_orphaned` writes today (`crates/custodian/src/gc.rs:110-122`) and stores
  in the field are full of them.
* **Per-pass cost grows** by the staged population, which the admission counter bounds
  (`W_ref`/`U_ref`, `0016:2836-2860`). That is a stated, computed cost, not an unbounded one.
* **Docs currency** (`../wyrd/AGENTS.md:154-157`): this slice alters what the maintenance plane's
  reference set *means* — a crosscutting concept — so
  `docs/design/architecture/08-crosscutting-concepts.md` (and `06-runtime-view.md` where the
  custodian loops are described) gain the staged protection class **in this PR**. #635 and #636
  edit the same files one and two waves earlier; extend what they wrote.
* **No ADR.** Graduating "the staging protection class and per-consumer visibility rule" to an
  ADR is 0016's third graduation recommendation (`0016:2960-2966`) and is architecture-board
  authority, not this slice's.

## Open questions

1. **Whether `staged_skipped` belongs on `RestoreReport`.** 0016 says it does
   (`0016:841`, "the `pending_skipped` counter gains `staged_skipped` and `sessions_fenced`
   siblings"). Adding a public field to `RestoreReport` is source-compatible for readers but
   changes the struct — confirm at sign-off that it does not need to be `#[non_exhaustive]`.
2. **Restore's session fencing (D-B)** — "a restore rewinds records to an image whose bytes may be
   gone, so no resurrected session may Complete". The `Open` → `Aborting` fence is straightforward;
   the **`Completing`** case needs the dedicated restore-fence transition that installs
   `retire:bytes:{session, parts}` **and** `retire:records:{seg:<g>:<E>}` in **one** batch
   (`0016:836-841`, X57). That transition is defined in #636's state machine — if #636 did not
   ship it, that is a §6 item for this bundle, **not** something to re-derive here.
3. **The `W_repoint + W_write + δ_clock` deadline** the fragment-less mark sweep needs is a window
   whose value is #625's to choose (`0016:3074-3079`). Pick a safe local constant with its
   derivation, and note the coupling so #625 can reconcile them.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
