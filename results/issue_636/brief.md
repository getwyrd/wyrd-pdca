# Design proposal — issue 636 / multipart-commit-protocol

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on
> `origin/main` @ `22d71b4` (3,108 lines; merged by PR #627). **Decisions 1, 3, 4 and 5, the §1
> record table, the §2 state machine, the §3 batch inventory and the protocol half of decision 6
> ARE this slice's design.** Read them at:
> §1 records `0016:333-527` · §2 state machine `0016:528-602` · §3 batch inventory `0016:603-692`
> · D1 session-fenced publication `0016:693-764` · D3 lifecycle + verb×state table `0016:894-1037`
> · D4 bounded work `0016:1038-1504` · D5 reclamation evidence `0016:1505-1793` · the admission
> counter + terminal delete inside D6 `0016:1794-2277` · staged writes `0016:2692-2717` ·
> execution register `0016:2518-2643` · tests owed `0016:2876-2939`.
> **Do MUST read those sections before writing code.** Each carries a failure-mode table
> enumerating the ways to implement it wrong and the observable that catches each. This brief
> does not restate them; it scopes the slice, settles the two things 0016 leaves to the
> implementing slice (the ETag composition and the knob values), carries forward what three
> rejected attempts already proved, and states the C4 shape.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26.
> This is **seam (iii) of five** in #508's re-plan (634 → 635 → 636 → 637 → 508): the protocol
> itself, in `crates/core`, with **no gateway wiring** — so the commit protocol can be reviewed
> against 0016 without the S3 wire surface competing for attention in the same diff.

- **Slug:** multipart-commit-protocol
- **Kind:** enhancement (design proposal)
- **Goal:** the multipart record family and its state machine, in `crates/core`, over the
  `MetadataStore` seam: `mpuctl` (the fleet admission ledger), `mpu:<id>` sessions,
  `slot:<id>:<k>` (the per-session in-flight key space), `part:<id>:<n>` with its `psum:` summary
  sibling, `sidx:<id>:<part>:<chunk>` (the **disjoint owned-staging** entry carrying `owner`
  **and** `staged`, deliberately *not* under `pending:`), `retire:bytes:` / `retire:records:`,
  and the verbs that move between them — create, stage, fence, publish, abort, drain, terminal
  delete. A caller with a `MetadataStore` and a `ChunkStore` can create a session, stage parts
  out of order, and publish an object whose bytes are the parts concatenated in part-number
  order — with every batch inside the transaction envelope and every failure leaving either a
  protected record or reclamation evidence.
- **Success criterion:** **two NEW test files** plus seeded DST cases appended to an existing
  one (see `Test file`), all crate-level over an in-memory store, **no gateway**.
  **(A) The happy path is exact.** Create → stage parts **out of order**, with at least one
  non-final part that is **not** a whole multiple of the chunk size (e.g. 5 MiB + 7 B) →
  Complete publishes a map that reads back **byte-identical** to the parts concatenated in
  part-number order. Assert the published bytes, not a chunk count: multipart chunking follows
  part boundaries, so a correct implementation legitimately yields a different chunk count from a
  plain PUT of the same bytes.
  **(B) The ETag is the settled pure function.** `etag = lowercase_hex( SHA-256( d₁ ‖ d₂ ‖ … ‖ d_N ) ) + "-" + N`,
  where `dᵢ` is the **raw 32 binary digest bytes** (not their hex text, no separators, no part
  numbers mixed in) of the *i*-th part in **ascending part-number order over exactly the parts
  the caller named**, and `N` is that count. **Never MD5** — ADR-0047 closed the basis
  (`docs/design/adr/0047-object-metadata-model.md:73-89`, lowercase-hex SHA-256 as an opaque
  change-token) and deferred only the composition (`:112`, `0016:3064-3070`). The test computes
  the expected value itself from the part bodies, so the oracle is independent of the
  implementation's choice.
  **The subset case needs BYTES and EVIDENCE, not just an ETag** (0016's own oracle, `:1033`:
  "Stage parts 1–3, Complete naming 1 and 3: the object MUST be parts 1+3, and part 2's **bytes**
  MUST end up orphan-marked while parts 1 and 3's bytes must not"). An ETag-only assertion is
  vacuous here: an implementation that computes the digest from the named subset, **publishes every
  staged part**, and deletes the unnamed records without evidence passes leg A's separate
  all-parts byte check, this leg's ETag check, and leg H's residue check — while returning wrong
  object bytes and leaking fragments. So assert, for a Complete naming a strict subset: the
  read-back bytes are **exactly the named parts in part-number order**; every **unnamed** part's
  fragments carry `orphan:` evidence **before** the record naming them disappears (marks precede
  the deletion of the records that name the bytes — X104, `0016:2633`); and **no published**
  fragment is orphan-marked.
  **(C) The decision-3 answer table, cell by cell, for every state this slice can reach without
  a reaper** (`0016:969-978`). Each asserted as a **typed outcome**, never "an error":
  `UploadPart` / `Complete` / `ListParts` after Abort → `NoSuchUpload`; a second Abort →
  idempotent success; an **identical** Complete retry inside the tombstone window → success with
  the **recorded** ETag; a Complete reusing the upload id with a **different** part list →
  `NoSuchUpload` (the `complete_fingerprint` rule, `0016:898-908`, `:2610`); a wrong part list →
  the invalid-part outcome **without publishing**; a part whose chunk count exceeds
  `MAX_PART_CHUNKS` → the too-large refusal **with the session still usable and abortable**
  (decision 4.4). The S3 status/code mapping is **#508's**; this slice pins the typed answers the
  wire layer will map.
  **(D) The staging class is the disjoint owned one, observed WHILE the part is in flight.** Not
  merely absent afterwards: at a mid-stage checkpoint assert `scan("sidx:<id>:")` is **non-empty**
  while `scan("pending:")` is **empty**; after the part commits assert **no `sidx:` entry and no
  `slot:` record remains for that part**. A post-hoc scan alone proves nothing — an implementation
  that staged under `pending:` and deleted it during the commit passes it while re-entering the
  global `pending:` scans and the #557 cross-clock expiry semantics for the whole life of the
  upload, which is exactly what 0016's disjoint class exists to prevent (`0016:480-490`,
  restore's bound re-derivation at `0016:834-841`).
  **(E) Admission is exact and bounded — and a `Completed` tombstone STAYS COUNTED.** `mpuctl`
  bootstraps on the first create (`require_absent` + put, `{count, max_sessions, profile}`, `0016`
  F12/X53); two open sessions read `count == 2`; a create past `MAX_SESSIONS` is refused with the
  typed backpressure outcome. For create + **abort** assert **both halves separately**: the abort
  *returns* from the fence commit alone (`count` unchanged — teardown is **not** on the request
  path, 0016 F9), and `count` returns to its prior value **only after** the bounded drain and the
  terminal delete.
  **CORRECTED 2026-07-26 — do NOT release the slot on Complete.** An earlier revision of this
  brief (inherited from the issue body's carried-forward list) demanded that `Completed` sessions
  release their admission slot and that ≥ 3 × `MAX_SESSIONS` sequential create→complete cycles all
  succeed. **That is unsatisfiable by a conforming implementation**, and requiring it would push Do
  into one of two defects. The authority is explicit, three times over: `count` "is the number of
  `mpu:` records that exist in any state" (`0016:348`); "Tombstones are **counted by the admission
  counter** (they still hold an `mpu:` record) and their retention is bounded by `W_tombstone`"
  (`0016:966-968`); "the counter counts **all** session records in any state
  (Open/Completing/Aborting/Completed tombstones)" (`0016:2029-2031`). The decrement happens in the
  **terminal session-delete batch**, and for a `Completed` session that batch is driven by
  `W_tombstone` — which is **#625's**, explicitly out of scope here. So the ≈70-upload ceiling an
  earlier attempt hit was **correct behaviour with no reaper running**, not a defect: satisfying
  the old leg would have meant either a second, non-authoritative counter (a deviation a previous
  builder already flagged in `results/issue_508/iteration-v6/build-notes.md`) or deleting the
  tombstone early, which destroys the identical-Complete-retry idempotence leg C and #508 both
  depend on.
  **What to assert instead:** after a Complete, `count` is **unchanged** and the `mpu:` record
  survives in `Completed`; the tombstone still answers an identical retry (leg C); `count`
  decrements **exactly once**, in the terminal delete, and this slice's only terminal-delete path
  is the **abort/teardown** one. Assert the counter is exact across every transition this slice can
  drive, and that no path decrements twice.
  **(F) Concurrent creates on an empty store all succeed — the carried-forward `503` bug.**
  **16 concurrent** `create` calls against an empty store all succeed. A single retry budget used
  for *both* upload-id collision (a 2^-128 event) and the globally serialized admission CAS makes
  the k-th concurrent creator need O(k) attempts: measured 8 → 2 refused, 10 → 3, 16 → 3, with
  the ledger nowhere near its bound, and aws-cli's default `max_concurrent_requests` is 10. The
  two retries are **separate concerns with separate bounds**, and the CAS-contention retry gets a
  real bound with jittered backoff. Assert at 8, 10 and 16 — not one number.
  **"All succeed" alone is vacuous** — an implementation that answers success without landing a
  session passes it. Assert additionally: the N upload ids are **pairwise distinct**; **N `mpu:`
  records exist** afterwards; and `mpuctl.count == N` **exactly** (not ≥ N, not N±1) — so a lost
  increment, a double increment, or a fictitious success all fail.
  **(G) The drain converges past the real boundary, and is idempotent.** A Complete naming
  **≥ 4,001 parts** (straddling `B_ops`, not sitting below it) drains to empty in **byte-budgeted**
  batches — assert the maximum observed batch's **encoded mutation bytes**, not its record count
  (`0016:1496`). Earlier attempts failed this twice: one re-derived the same first `B_ops` keys
  forever with no cursor (never converged), and a later one truncated derivation while still
  marking the obligation **fully drained** (silent permanent part loss). So assert **both**: the
  walk terminates, and every named record is actually gone. Running the drain **twice** over a
  partially drained obligation converges to the same terminal state with **no double-decrement**
  of `mpuctl.count`.
  **(H) After the terminal delete, nothing is left.** The session's `sidx:` range is empty, its
  `slot:`, `part:`, `psum:` records are gone, the `mpuctl.count` decrement happened **exactly
  once**, and the terminal delete is preconditioned on the session record's **exact bytes**. A
  session that reserved a `seggrp:` nonce it never adopted deletes the marker in the same batch;
  one whose group **was** adopted leaves it (`0016:513-527`, the two-arm rule — #635 ships the
  bounded predicate this gates on).
  **(H2) The no-gap classification invariant holds after every scenario.** 0016 earns one shared
  test helper from this protocol: given a store and a fleet, assert **every** on-disk fragment is
  in **at least one** safe class — committed-referenced, staged-with-a-session, or
  evidenced-for-reclamation — and that it is in no *genuinely incompatible* combination
  (evidenced-for-reclamation with its grace elapsed **while** still committed-referenced is the
  pair GC would act on wrongly). **"Exactly one" would be WRONG and would fail on correct
  executions** — this protocol deliberately overlaps protection across both handoffs
  (`0016:2906-2921`). Invariant (2) is a **no-gaps** claim, not a partition. Ship the helper in
  this slice and **run it after every scenario in legs A–H**; without it, a fragment that falls out
  of all three classes at a handoff is invisible to every other leg.
  **(I) Seeded DST for this slice's own races** (ADR-0009 is the correctness authority for
  interleavings, `0016:2877-2905`), **appended to the existing `crates/dst/tests/concurrency.rs`
  — NOT a new DST file** (see `Test file` for the gate reason; this is not stylistic). Three, and
  only these three, belong here — the rest of 0016's list is #625's or #637's: **(i)** publication CAS loss — two flip attempts against a prior that
  moves; the published `version` is `prior.version + 1` computed from the **re-read** prior at
  each attempt, never frozen at fence time (`0016:350`, matching
  `crates/core/src/metadata.rs:551`,`:595`,`:656`); **(ii)** the slot-reserve race at the cap
  (X41/X55) — concurrent part starts at `MAX_INFLIGHT_PARTS` produce no
  `MAX_INFLIGHT_PARTS + 1`-th key and no starvation; **(iii)** two concurrent drainers over one
  owned `sidx:` range (X56) — exactly-once effects, no double-decrement.
  **(J) `cargo xtask ci` green.**
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — the folded
  `origin/pdca-integration/main` carrying #634 and #635 — with no container, cluster or deploy
  stack. But be honest about its **shape**: this slice introduces an entirely new module, so on
  the base the added test files **do not compile**, and `run-verify.sh` scores a build failure as
  a red without ever counting tests (the `TESTS_RAN == 0` guard at
  `engine/scripts/run-verify.sh:416-427` is inside the cargo-*succeeded* branch; a non-zero exit
  falls through to the unconditional `PASS` at `:433`). A compile-shaped red is the absence of a
  measurement, not evidence.
  **So the binding evidence for this slice is DEMONSTRATED RED, and it is mandatory — with the
  honest caveat that NO GATE CONSUMES IT.** The negation runs below are recorded in
  `build-notes.md`, which the driver withholds from the reviewer and no `[[gates.checks]]` row
  reads; they are therefore **sign-off evidence for the human**, not a mechanical check. That is a
  deliberate, declared limitation of this slice's shape (a net-new module has no flippable prior
  assertion), and it is why the human must actually read the recorded output at §9 rather than
  treat the C4-verify PASS as proof. With the
  patch applied, Do MUST — for each of legs **F**, **G** and **E** — temporarily
  negate the mechanism (collapse the two retry budgets back into one; drop the drain cursor; and,
  for E, **skip the abort path's terminal-delete decrement, or apply it twice**, and require the
  exact-count assertion to fail), re-run the named test, and record in `build-notes.md` the
  **observed failing output** of each. Those three are the exact defects three rejected attempts
  shipped; a test that does not visibly catch them is not a test of them. Do MUST also record,
  from the C4-verify RED leg, how many tests actually ran and failed, and state plainly that the
  red was a build error.
  **Base resolution is a gate-evaluability precondition, not a detail.** This is a **wave-2**
  bundle, so the driver stamps its stack base and `pdca gates` exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`,
  `src/pdca_harness/gates.py:352-360`), which `run-verify.sh` honours ahead of the brief's base
  (`engine/scripts/run-verify.sh:186-206`). Without it C4-verify resets to `origin/main`, where
  neither `scan_page` (#634) nor the segmented map (#635) exists — the patch would fail to apply
  (this slice edits `crates/core/src/metadata.rs`, which #635 also edits) and the gate would
  report a stale bundle rather than a verdict.
  **The DST leg (I) is evaluated by C4-ci, not C4-verify — deliberately.** It is appended to the
  existing `crates/dst/tests/concurrency.rs` (see `Test file` for the measured reason), so it is a
  *modified* path that never enters C4-verify's invocation, and its evidence comes from
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1614`, the **gating** row) which sets
  `--cfg madsim` and `MADSIM_TEST_NUM=50`. Do MUST record in `build-notes.md` that the new DST
  cases actually **ran** (a `#![cfg(madsim)]` file compiles to nothing without the flag and
  reports "0 tests"; `concurrency.rs` already carries the attribute — do not remove it) and how
  many seeds each swept.
- **Invariant to restore:** **every durable byte written by an assembled write is, at every
  instant, either protected by a record that names it or evidenced for reclamation — and no state
  of an upload session is absorbing.** Both directions bind and neither may be traded for the
  other: retaining unconditionally leaks forever; unprotecting without evidence leaks forever
  *silently*, because GC reclaims only on explicit evidence and otherwise conservatively retains
  (`crates/custodian/src/gc.rs:158-198`). **Source:** 0016's four invariants and its refutation
  standard (`0016:138-151`; outcomes (a)–(d) at `0016:2811-2813`), resting on the custodian's
  written safety rule that a referenced fragment is never reclaimed (`0005:294-295`). SELF-TEST:
  this cannot be satisfied by guarding one module — it is a property of the whole record family
  and of every batch that moves a fact between key ranges, which is why the §3 batch inventory
  (`0016:603-692`) enumerates each one and its bound.
- **Scope:** the multipart record family and its key helpers, decisions 1/3/4/5 and the
  protocol half of decision 6, the staged-write changes in `crates/core/src/write.rs`, the ETag
  composition, and the knob values inside 0016's ranges — all in `crates/core`, over the
  `MetadataStore`/`ChunkStore` seams. **Out of scope:** the S3 verbs, XML, HTTP status/error
  codes and routing (#508); the reaper loop and every window-driven exit (#625); operator abort /
  terminal expiry / foreign-clock alarm (#633); the custodian-side protection class (#637) — and
  in particular **no change to `reconcile_step`'s signature or `GcContext`'s fields**; any file
  under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016` itself.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 634, 635, 638
- **Conflicts with:**
- **Ordering note:** **wave 2 of the five-slice stack 634 → 635 → 636 → 637 → 508.** Both edges
  are genuine build-on dependencies, not mere file conflicts. **#638** (added 2026-07-26) ships the
  server-enforced fragment-write deadline this slice's staged writes are the first consumer of —
  without it `W_write` is a caller-side timeout only and decision 5's late-fragment bound is not
  real (`0016:1551-1576`). The `retire:` drain walks **#634**'s
  `scan_page` (`0016:2645-2652` — the namespace is deliberately unbounded, so a single `scan`
  cannot enumerate it), and a published multipart map is **#635**'s segmented map whenever the
  assembled chunk list exceeds `MAX_MAP_CHUNKS`, published through #635's staged-publication
  committer with this slice's session fence supplied as its precondition. This slice also edits
  `crates/core/src/metadata.rs`, which #635 edits — so the wave separation is required twice over.
  **A merge-order requirement that is NOT a batch dependency, and must not be encoded as one:**
  0016 makes it normative that **#625 (the reaper) and #633 (the operator exit) land with or
  before #508** (`0016:234-268`) — the protocol this slice ships has states whose only exit is the
  reaper. Neither is in this batch, so neither may appear in a `Depends on` line (the driver
  rejects a declared dependency that is neither in-batch nor COMPLETE,
  `src/pdca_harness/waves.py:57-83`). **Carry it as a human sign-off obligation on #508, not
  here.** Both sibling briefs were **re-pointed on 2026-07-26**:
  `results/issue_625/brief.md` now declares `Depends on: 636, 637` and
  `results/issue_633/brief.md` `Depends on: 625, 636`, so the graph no longer has the backwards
  #508 edge. Nothing further is owed here; the note is kept because the merge gate it protects
  (#625 and #633 with-or-before #508) is still a human obligation, not a machine one.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **External dependencies:** `typos`, `docs-renderer`
  <br>*(Both on the field's own line — the driver reads only that line. Needed because this
  slice's docs-currency edit to `docs/design/architecture/{06,08}` is gated by the prose gates,
  which `cargo xtask ci` **warn-skips** when the tools are absent, so a locally-green docs change
  opens the PR red on the host's always-on jobs, INTEGRATION §3. Both are installed on this host.
  Nothing else: every store this slice touches is in-process.)*
- **Test file:** `crates/core/tests/multipart_protocol.rs`, `crates/core/tests/multipart_admission_and_drain.rs`
  <br>*(Both paths on the field's own line — the driver parses only that line,
  `src/pdca_harness/brief.py:23-31`, `:101-113`.)* The first carries legs A–E, H and H2; the second
  legs F and G (the carried-forward failure modes), in their own binary so a `tracing`
  callsite-cache interaction in one cannot mask the other (the #214 discipline). **Both NEW files**
  under a `tests/` directory: `run-verify.sh`
  classifies on an **added** `*/tests/*.rs` (`engine/scripts/run-verify.sh:92-94`, `:300-311`), so
  a case appended to an existing suite would degrade the gate to green-only.
  **Leg I's DST cases are the deliberate exception: append them to the EXISTING
  `crates/dst/tests/concurrency.rs`** (the commit-protocol concurrency campaign) — do **not**
  create a new `crates/dst/tests/*.rs`. The reason is mechanical, and it was measured at Plan:
  `run-verify.sh` puts every **added** test file into **one** cargo invocation and, seeing a
  `#![cfg(madsim)]` crate root among them, applies `RUSTFLAGS=--cfg madsim` to the whole
  invocation (`engine/scripts/run-verify.sh:100-131`, `:344-385`). Under that flag
  `wyrd-core`'s dev-dependency graph **swaps `tonic` for `madsim-tonic`** (verified with
  `RUSTFLAGS="--cfg madsim" cargo tree -p wyrd-core -e normal,dev`: 0 madsim crates without the
  flag, `madsim-tonic` with it), so the crate-level tests would be built against a simulator
  graph they are not written for — and C4-verify would fail a correct bundle. Appended to an
  existing file the DST cases are a *modified* path, never enter `ADDED_TESTS`, and are run where
  they belong: `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1614`), the **gating** C4-ci
  row.
  `wyrd-core`'s dev-dependencies already carry `wyrd-metadata-redb`, `pollster`, `tokio`,
  `async-trait` and `tempfile`, and `wyrd-dst` carries `madsim`, `wyrd-core`,
  `wyrd-metadata-redb` and `wyrd-metadata-conformance` — prefer them over a new dependency, and
  if one is genuinely needed declare it in `build-notes.md` rather than working around it. A
  modified `Cargo.toml` is reverted on the RED leg, so a test that needs a new dev-dependency
  cannot earn a red.
- **Verification posture:** DEFAULT in form (red pre-fix, green post-fix at Check) but the red is
  **criterion-absence** — a net-new module, so "red" is a build failure, not a flipped assertion.
  That is declared here so C2/C4 land as a pre-declared sign-off item rather than a surprise
  NEEDS-HUMAN, and it is why the **demonstrated-red** obligation in `Falsifiability` is
  mandatory rather than best-effort: the three negation runs are what prove the new tests are
  load-bearing. Nothing is deferred off-Check — every store this slice touches (in-memory redb,
  the madsim sim) runs inside `cargo xtask ci`.
- **Production reach:** this slice builds the protocol **ahead of its wire surface, by design**.
  (a) What honours it at Check: the crate-level tests drive the **real** committers, the real
  `MetadataStore` seam and a real chunk store — nothing is stubbed except the *caller*, which is
  the test instead of the S3 handler. (b) Where the production wiring lands: **#508** exposes the
  six verbs over these functions; **#637** teaches the maintenance plane to protect the staged
  bytes this slice creates; **#625** drives the states whose only exit is the reaper. (c) The
  test caller is load-bearing, not scaffolding — legs D, G and H observe **durable store state**
  through raw key ranges, so an implementation that satisfies the caller while leaving residue
  fails. **Explicitly: when this slice merges, no client can reach any of it** (the S3 subresource
  denylist still refuses every multipart form, `crates/gateway-s3/src/lib.rs:343-345`,
  guard `:1696-1709`), and that is the intended state — it is what makes the protocol reviewable
  on its own.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/core/src/metadata.rs:540-660` — `commit_chunk_map` /
    `commit_chunk_map_superseding` / `..._superseding_leased`: the `require(key, encode(prior))`
    CAS shape, the `prior.version + 1` rule, and the `..prior.clone()` field-preservation idiom.
    Session-fenced publication is **this shape with the fence precondition added**, not a new one.
  * `crates/core/src/metadata.rs:736-800` — `renew_pending` (`:736`) / `live_lease_guards`
    (`:778`). **Read what they actually do rather than trusting a summary:** the CAS is an
    exact-value precondition against the bytes the caller read. The obligation on the new fields is
    unchanged and is what matters — `PendingEntry`'s new `owner` and `staged`
    fields **must** be `skip_serializing_if` (`0016:474-490`), so that `decode → encode` is the
    identity on a legacy entry wherever a path re-encodes a prior. Emitting `"owner":null` turns every
    renewal and every lease guard on a pre-upgrade `pending:` entry into a permanent `Conflict`.
  * `crates/core/src/write.rs:198-230` (`intent` — the pending-ledger write the `sidx:` intent
    replaces), `:231-262` (`write_fragments`, the fan-out), `:264-310` (`commit_create`) and
    `:474-545` (`lease_write_chunk` — the lease/renewal path whose refuse-don't-resurrect branch
    the fence adds to) — the staged write path is these, fenced.
  * `crates/core/src/placement.rs:141-152` — `Topology::excluding`, which staged placement
    (committed **and** in-flight owned) must select against (`0016:2692-2699`).
  * `crates/server/src/lib.rs:237-251` — the gateway's chunk-id minting. **Read it, do not assume
    it:** it is a *random-epoch + counter* scheme, **not** a 128-bit random token, so it is a
    precedent for *coordination-free gateway-side minting* (ADR-0019) and **not** for the 2^-128
    collision basis. Upload ids and the segment-group nonce are **full 128-bit random tokens**
    rendered lowercase-hex (`0016:492-497`), guarded by `require_absent` on their key; that is
    where the 2^-128 argument comes from, not from this callsite.
  * `crates/custodian/src/desired_state.rs:33-38` — the `desired:dserver:<S>` ledger key each
    `sidx:` intent must carry `require_absent` on (the drain fence, `0016:2699-2707`).
  * `crates/custodian/tests/gc.rs:26-120` — the in-memory store/D-server harness shape the new
    test files should follow.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. `git log -S"CreateMultipartUpload" --all` matches **only** the proposal document — no
  implementation has ever been merged. `crates/core/src/metadata.rs`'s last shape change is
  ADR-0047's object-metadata model (PR #594, MERGED); `crates/core/src/write.rs` last changed
  under the typed transient/terminal error classes (`3369cbd`). No open PRs. The rejected prior
  art is entirely inside this harness — seven `results/issue_508/iteration-v*/` attempts, of which
  three are load-bearing here: **v2** (the permanent session-less-part leak, which 0016 decisions
  2/3/5 now settle by design), **v4** (the `scan_page` default shim and the read-path-only
  resolver) and **v7** (the false `503 SlowDown` at ordinary client concurrency, the
  non-converging drain, and the ≈70-lifetime-upload admission ceiling — legs F, G and E of this
  brief). `results/issue_508/review-rejected.md` records what was *rejected with reason* in v7 and
  must not be re-earned: the missing-reaper startup posture (a plan decision), a second timeout
  inside `core`'s fan-out (the `ChunkStore` seam owns it), and a wall-clock bound inside the drain
  (`clippy.toml` denies a bare `SystemTime::now()` in `core` — bound the **work**, not the time).
- **Disposition hint:** likely-fix

## Motivation

Wyrd cannot accept an object larger than 5 GB, and `aws s3 cp` / boto3 switch to multipart above
~8 MB — so the default client fails on ordinary large uploads today. #508 is the verb set; this
slice is the protocol underneath it, split out because a commit protocol and a wire surface are
two different review problems and shipping them in one 44-file diff is what got the seventh
attempt rejected at sign-off.

The protocol is not "store parts, then concatenate". Its hard part is that a client's uploaded
parts are **durable but unpublished** bytes, which no existing record class describes: they must
be protected while the session lives, evidenced for reclamation when it dies, bounded so a
maintenance pass can enumerate them, and published atomically without ever exceeding a
transaction envelope. 0016 settles every one of those; this slice implements them.

## Design

Read 0016. What follows is only the scoping, the two settled values, and the boundaries.

### In scope

* **The records** (`0016:333-527`) and their key helpers, in `crates/core/src/metadata.rs` and a
  new `crates/core/src/multipart.rs`: `mpuctl`, `mpu:<id>`, `slot:<id>:<k>`, `part:<id>:<n>` +
  `psum:<id>:<n>` (written in the **same** batch), `sidx:<id>:<part>:<chunk>`,
  `retire:bytes:{generation}` / `retire:records:{…}`. `PendingEntry` gains `owner` and `staged`,
  both additive, both `skip_serializing_if`, `Some` only on a `sidx:` value.
* **Decision 1** — session-fenced publication: what replaces lease liveness as the
  publication-time proof, with the published version computed from the **re-read** prior at each
  flip attempt.
* **Decision 3** — lifecycle states, the verb × state answer table, and `complete_fingerprint`.
* **Decision 4** — bounded work for unbounded objects: **byte-budgeted** (never count-budgeted)
  drain and segment batches, the `MAX_*` caps and their derivations, and supersede/`unlink`
  routing through `retire:bytes:{generation}` instead of expanding orphans inline.
* **Decision 5** — reclamation evidence for failed in-flight work.
* **The protocol half of decision 6** — the serialized admission counter, and the terminal delete
  preconditioned on the session record's exact bytes, gated on the `sidx:` range observed empty,
  with an exactly-once `mpuctl.count` decrement. **The reaper loop itself is #625.**
* **Staged writes** in `crates/core/src/write.rs` (`0016:2692-2717`): placement selects against
  `Topology::excluding(draining)`; `intent` writes the owned `sidx:` entry with its `WritePlan`
  placement, preconditioned `require(mpu == Open@E)` **and** `require_absent(desired:dserver:<S>)`
  per selected server — the drain fence that makes the filter atomic with the drain request,
  **with a re-plan on failure, not a refusal**; the renewal loop renews owned `sidx:` leases and
  rewrites that request's `slot:` key **in the same batch**.
* **The ETag composition**, settled in leg B above.

### Out of scope — do not touch

* The S3 verbs, XML bodies, HTTP status/error codes and routing — **#508**.
* The reaper loop and every window-driven exit (`W_open`, `W_session`, `W_completing`,
  `W_tombstone`, the cursor-keyed out-of-band drain, the clock guard) — **#625**.
* Operator abort / terminal expiry / foreign-clock alarm — **#633**.
* The custodian-side protection class for staged bytes — **#637**. In particular: **do not change
  `reconcile_step`'s signature or `GcContext`'s fields** in this slice. #637 owns that change, and
  making it here would collide with a bundle that has to build on this one.
* Any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016` itself.

### The knob values (settled here, inside 0016's ranges)

0016 lists `MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS`, `MAX_PART_CHUNKS`, `MAX_ROOT_SEGMENTS`,
`MAX_STAGED_CHUNKS`, `MAX_INFLIGHT_PARTS`, `chunk_size`, `R_publish` and
`MAX_COMPLETE_ATTEMPTS` as **#508**'s to choose, "non-gating because no value inside a settled
range can break an invariant" (`0016:3072-3080`). But the *caps themselves* are enforced in this
slice's code, so this slice must pick values. **Pick them here, inside the ranges and the
bounding invariants tabled at `0016:1464-1474`, and give each a named constant with the
derivation in its doc comment.** #508 inherits them; it does not re-choose them.
**Two inputs are NOT this slice's and must be consumed, not invented:** `W_ref` (the reconcile RAM
budget) and the `B`/`B_ops` operation budget are **#625's** by 0016's own assignment
(`0016:3072-3080`), yet `MAX_SESSIONS = ⌊W_ref / U_ref⌋` and the `MAX_PART_CHUNKS ≤ B_ops` clamp
both depend on them. Since #625 builds *after* this slice, Do MUST (a) define them here as named
constants with their derivation, (b) record the chosen values explicitly in `build-notes.md` so
they are reviewable as a **value set**, and (c) state in the doc comment that #625 consumes these
and must not re-derive them — a split budget authority admits a permanent failure class (a legally
admitted session whose unsplittable reap fence no longer fits is never reaped). If Do believes a
value cannot be chosen without #625, that is a Check §6 item, not a placeholder. Two are not free:
`MAX_STAGED_CHUNKS = MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` (the publishable ceiling), and
`MAX_SESSIONS` is **derived** as `⌊W_ref / U_ref⌋` — never independently chosen
(`0016:2836-2860`, and the decision-6 derivation). A hard-coded `MAX_SESSIONS` is a defect, not a
value choice.

### What three rejected attempts already proved (do not re-earn)

1. **`CreateMultipartUpload` answered a false `503 SlowDown` on an empty store at ordinary client
   concurrency** — one 4-attempt budget served both the 2^-128 upload-id collision and the
   globally serialized admission CAS, so the k-th concurrent creator needed O(k) attempts.
   Separate the two retries, bound the CAS-contention one properly with jittered backoff, and
   **test concurrent Create** (earlier attempts only tested concurrent `UploadPart`). Leg F.
2. **The drain did not converge** — one attempt re-derived the same first `B_ops` keys forever
   with no cursor; a later one truncated derivation while marking the obligation fully drained,
   which is silent permanent part loss. Regression tests must **straddle** the real boundary
   (≥ 4,001 parts), not sit below it. Leg G.
3. **~~`Completed` sessions did not release their admission slot~~ — STRUCK 2026-07-26.** The
   issue body carries this as a carried-forward defect ("≈70 lifetime uploads and then every create
   refused"). It is **not** one: `mpuctl.count` counts every `mpu:` record *in any state*, tombstones
   included, and is decremented only at terminal deletion (`0016:348`, `:964-968`, `:2026-2031`), so
   that ceiling is the specified behaviour when **no reaper is running**. Do MUST NOT implement an
   early release; leg E asserts the opposite. What *is* carried forward from that attempt is the
   **exactness** of the counter — no lost, double or fictitious increment, and exactly one
   decrement per terminal delete.

## Alternatives considered

* **Implementing the verbs and the protocol together** (attempts 1–7). Rejected at sign-off on
  reviewability: 44 files / 14,117 lines across the wire, core and maintenance planes in one
  diff. The split is the remedy, and it only works if this slice stays wire-free.
* **Keeping owned staging entries under `pending:`** — rejected by 0016 decision 5 and re-derived
  twice: the fleet-wide owned population
  (`MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`) can exceed `SCAN_CAP`, so restore's
  global `scan("pending:")` would fail `ScanCapExceeded` and the restore command could never make
  progress (`0016:834-841`). The disjoint `sidx:` prefix is what keeps restore's bound exactly
  today's.
* **A count-budgeted drain/segment batch** — rejected (`0016:1496`): a fixed count of 1,000
  records × 100 KB is 100 MB, far past the transaction envelope, and it fails *permanently* on
  FoundationDB. Byte budgets or nothing.
* **A per-session `sinf:` in-flight counter** — 0016 records it as the rejected shape and the
  reason (`0016:3050-3060`): it serialized same-session part boundaries and carried a starvation
  hole. The `slot:` key space replaces it and there is nothing left to serialize.

## Impact & compatibility

* **Additive on disk**: every prefix this slice introduces (`mpuctl`, `mpu:`, `slot:`, `part:`,
  `psum:`, `sidx:`, `retire:`) is new; nothing reads them today.
* **One non-additive change**, and it must be handled as 0016's *Backward compatibility* section
  states: supersede and `unlink` stop expanding orphans inline and route through
  `retire:bytes:{generation}`, so the timing of orphan evidence changes (grace starts at drain,
  not at the supersede commit). Existing `orphan:` values must **all** still decode.
* **`PendingEntry` gains two optional fields** — additive, `skip_serializing_if`, `Some` only on
  a `sidx:` value. The round-trip identity test on **both** a legacy `pending:` value and an owned
  `sidx:` value is named in 0016's graduation criteria and is a merge requirement here.
* **Docs currency** (`../wyrd/AGENTS.md:154-157`): this slice adds persisted record classes, so
  `docs/design/architecture/06-runtime-view.md` and `08-crosscutting-concepts.md` gain them **in
  this PR**. #635 adds the segmented map to the same files; expect to extend, not replace, what
  the wave below wrote.
* **Client-visible: nothing.** No verb reaches this code until #508.

## Open questions

1. **The ETag spelling** is settled in leg B by this brief, not by 0016 (which fixes only "a pure
   function of the part records' recorded digests and their order"). It is the same spelling the
   seventh attempt used, so the salvaged S3 tests in `results/issue_508/` already encode it. If
   the maintainer wants a different composition, say so at sign-off **before** Do runs.
2. **How much of decision 4's `retire:` drain belongs here versus #625.** This slice ships the
   drain *function* and its convergence (leg G); #625 ships the loop that calls it out of band.
   The boundary is "a caller-driven bounded drain here, a scheduled one there". If Do finds a
   third thing, surface it rather than absorbing it.
3. **`R_publish` and `MAX_COMPLETE_ATTEMPTS`** interact with the reaper's `W_completing` rollback,
   which is #625's. Choose values that are safe with **no** reaper running (the state this slice
   merges into) and say so in the doc comment.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Confirm the mandatory F/G/E mechanism-negation runs failed as intended — my base replay ran 0 tests because the new tests did not compile, so it proves criterion absence but not load-bearing behavior.; C5 Causal adequacy — Require a segmented root-flip-loss regression — the current DST race stays flat, so it cannot catch the stale resume/fence path, consistent with 162 surviving in-diff mutants (`crates/dst/tests/concurrency.rs:451`).; T5 Judgment — Require event-keyed three-arm orphan restamping or equivalent reader-grace proof — unconditional “present means skip” can retain expired evidence across a later unreference event (`crates/core/src/multipart.rs:3479`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 51 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 550 mutants tested in 88m: 162 missed, 206 caught, 180 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 51 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Decide whether the mandatory F/G/E mechanism-negation evidence is persuasive at sign-off—the independent base replay failed at compile time with 0 tests run, so it proves criterion absence but not that those assertions are load-bearing.; T4 Contribution — Decide the disposition of the recorded 38 blocking batch-review findings—the permitted target and artifacts do not contain `scripts/review-branch` or its report, so that red gate cannot be independently reproduced or triaged; the separate contribution-artifact gate passed.; T5 Judgment — The no-gap oracle must distinguish the exact segmented epoch before it can validate reclamation—the helper constructs the epoch-specific group but scans every epoch under the nonce, so another attempt can falsely protect missing chunks (`crates/core/src/multipart.rs:4397`, `crates/core/src/multipart.rs:4399`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 38 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 38 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the compile-shaped pre-fix red only after reviewing the withheld F/G/E negation output — my clean-base replay ran 0 tests, so it proves criterion absence rather than that the concurrency and drain assertions at `crates/core/tests/multipart_admission_and_drain.rs:357` and `crates/core/tests/multipart_admission_and_drain.rs:610` are load-bearing.; C5 Causal adequacy — Require relational `mpuctl` validation plus a corruption regression — decode accepts `max_sessions != profile.max_sessions()` at `crates/core/src/multipart.rs:1477`, then admission trusts the inconsistent stored limit at `crates/core/src/multipart.rs:1753`, so an oversized torn value can violate the `W_ref` bound; the full mutation row also timed out.; T4 Contribution — Decide the disposition of the 21 recorded blocking batch-review findings — `scripts/review-branch` and its report are absent from the permitted target, so that red is provisional; the contribution gate passed and affected-path merged/closed prior-art searches found no earlier multipart implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
