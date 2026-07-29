# Design proposal — issue 634 / scan-page-seam

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` as it
> stands on `origin/main` @ `22d71b4` (3,108 lines; merged by PR #627). The paragraph that IS
> this slice's specification is the first bullet of *What the implementing slices change*,
> **`0016:2645-2672`** — it fixes the signature, all four semantic clauses, and the reason each
> one is load-bearing. **Do MUST read that bullet in full before writing code.** This brief
> does not restate it; it scopes it, settles the two decisions 0016 leaves to the implementer
> (required-vs-default trait method, and how the **36 in-test `MetadataStore` impls across 26
> test files** absorb the change), and states the C4 shape.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26.
> This is **seam (i) of five** in #508's re-plan (634 → 635 → 636 → 637 → 508). It depends on
> nothing and lands on `main`.

- **Slug:** scan-page-seam
- **Kind:** enhancement (design proposal)
- **Goal:** `MetadataStore` gains a **bounded, cursor-keyed range scan** whose semantics are
  normative and identical on every backend, so a namespace that grows past `SCAN_CAP` can be
  enumerated at all. Today `scan(prefix)` is prefix-only and complete-or-`ScanCapExceeded`
  (`crates/traits/src/lib.rs:770-776`; `SCAN_CAP = 1 << 20` at `:286`; backends clamp and
  **refuse to raise** it, `crates/metadata-redb/src/lib.rs:73-78`), so two multipart-era
  populations are simply un-walkable: the deliberately unbounded `retire:` namespace (0016
  decision 6) and GC's `orphan:` ledger, where one maximum segmented-object retirement installs
  **~1.78 M marks against a cap of 1,048,576** and the scan fails *whole*
  (`crates/custodian/src/gc.rs:322`), `?`-propagating out of the GC leg
  (`crates/custodian/src/reconciliation.rs:78-85`) and aborting the entire reconcile step
  before GC, scrub, reconstruction and rebalance run.
  The signature is `async fn scan_page(&self, prefix: &[u8], after: Option<&[u8]>, limit: usize)
  -> Result<(Vec<(Vec<u8>, Bytes)>, Option<Vec<u8>>)>`, and **the signature alone is not the
  contract** — the four clauses at `0016:2653-2666` are.
- **Success criterion:** two **NEW** test files (see `Test file`), both compiled and run by the
  default `cargo xtask ci` and by C4-verify.
  **(A) The four normative clauses land in `metadata-conformance` and are asserted on every
  backend that the gate actually runs.** New `contract_scan_page_*` clause functions in
  `crates/metadata-conformance/src/lib.rs`, added to `run_all` (`:428-441`) so redb's
  `trait_contract` (`crates/metadata-redb/tests/conformance.rs:28-34`) and the in-simulator
  `crates/dst/tests/conformance.rs` (redb + `SimTikvMetadataStore` + `SimFdbMetadataStore`)
  pick them up with **no per-driver list to edit**. The clauses:
  (a) **Order** — results ordered by **raw byte-lexicographic key**. Assert with keys that
  separate byte order from `str`/`String` order: keys containing bytes `0x7F`, `0x80`, `0xFF`
  and a multi-byte UTF-8 sequence, seeded in a shuffled insertion order, plus keys where one is
  a strict prefix of another (`p:a` before `p:a0`). A backend that sorts by decoded string,
  or by insertion order, must fail.
  (b) **Exclusive cursor** — a page starts strictly *after* `after`, asserted for an `after`
  that **exists** in the range and for one that does **not** (a synthesized key lying between
  two stored keys). An inclusive-cursor implementation duplicates the boundary key forever;
  assert the returned page's first key is `> after`.
  (c) **Termination** — `next` is `Some(last_key_returned)` while more may remain and `None`
  **only** when the prefix is exhausted at that instant. Both boundary shapes: a population that
  is an **exact multiple** of `limit` (the classic off-by-one — a walk that returns `None` on
  the last full page skips nothing but a walk that returns `Some` must terminate on the next,
  empty page), and one that is not. Assert the whole-population walk terminates **and** returns
  every key exactly once.
  (d) **No-skip for stable keys under concurrent mutation.** The guarantee is **exactly and only**
  what `0016:2658-2660` states: *a key present **throughout** the walk and not lexicographically
  before the cursor is returned exactly once*. Everything else is explicitly unconstrained — "keys
  inserted before the cursor after it passed, or deleted mid-walk, may be missed or duplicated",
  and "no snapshot isolation is required of any backend". So assert: a **control set seeded before
  the walk and never touched** appears exactly once each; a key **inserted behind the cursor**
  mid-walk may be missed; a key **deleted ahead of the cursor** may be missed or returned. **A key
  INSERTED AHEAD of the cursor mid-walk is NOT covered by the contract** — it was not present
  throughout the walk — so the clause MUST accept either outcome. Requiring it would reject a
  conforming backend that reads a snapshot per page, which is a shape 0016 deliberately permits.
  If the maintainer wants the stronger rule, it is an amendment to 0016, not a conformance clause
  written here.
  **(B) The primitive escapes the bound it exists to escape — on a real backend.**
  `crates/metadata-redb/tests/scan_page.rs` drives a `RedbMetadataStore::in_memory()
  .with_scan_cap(LOWERED)` (the established idiom — `crates/metadata-redb/tests/scan.rs:9-11`,
  `:74-90` — the production path is driven either way, only the ceiling moves) seeded with
  `LOWERED × k + r` keys under one prefix, and asserts **both halves in the same test**:
  `scan(prefix)` returns `Err` downcasting to `ScanCapExceeded` with `cap == LOWERED`, while a
  `scan_page` walk of the same store returns **every** key exactly once in byte order. This is
  the leg that fails against any `scan()`-backed shim.
  **(C) The `limit` is a page bound, and the API makes non-progress impossible.** Settled here
  rather than left to Do, because a page that returns nothing while reporting more-to-come is an
  infinite walk: (i) **`items.len() <= min(limit, effective_cap)`** always — the page is bounded by
  BOTH the caller's limit and the store's own cap, since 0016 requires no page to exceed `SCAN_CAP`
  (`0016:2647-2650`); (ii) a `limit` **above** the store's configured cap is **clamped to the cap**,
  never an `Err` — the cap refuses to be raised (`crates/metadata-redb/src/lib.rs:73-78`) and a
  caller asking for more must not be failed for it — **including redb, whose `with_scan_cap` knob
  makes its effective cap explicit** (`crates/metadata-redb/src/lib.rs:73-90`), so the clamp is
  observable there and must be asserted there; (iii) **`limit == 0` is rejected** with a **named
  error type declared in `crates/traits`** — beside `ScanCapExceeded`, so every backend raises the
  *same* type and a caller classifies it identically whichever store it holds
  (`crates/traits/src/lib.rs:288-300` is the model) — rather than answered with an empty page — an empty page carrying `next: Some(_)` is a
  successful non-terminal response with no progress, which is exactly the shape that makes a drain
  loop forever, and answering `next: None` instead would falsely report the prefix exhausted.
  Assert all three, plus **cursor progress**: for any non-terminal page, the returned `next` is
  strictly greater than the `after` it was called with.
  **(D) The clauses are provably load-bearing (non-vacuity).**
  `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs` implements
  dev-scope violating `MetadataStore` doubles — one that orders by decoded string, one whose
  cursor is inclusive, one that answers `next: None` on a full page, one that drops a stable key
  under mutation — and asserts with `#[should_panic]` that the matching clause **catches** each,
  while the pre-existing sequential clauses (`contract_commit_and_get`, `contract_scan_by_prefix`,
  `contract_require_absent_gates`, `contract_require_value_gates`) still **pass** against the
  same double. Exactly the pattern `crates/metadata-conformance/tests/demonstrated_red.rs:1-24`
  established for #419, and the only thing that turns a compile-shaped RED into evidence.
  **(E) Every backend is native and cursored; none is a `scan()` shim — and be precise about what
  is *proven* versus *required*.** `metadata-redb`, `metadata-fdb`, `metadata-tikv` and both DST
  sim stores (`crates/dst/tests/support/mod.rs:291`, `:739`) implement `scan_page` over their own
  cursor / range primitive. A default body over `scan()` is **not acceptable** anywhere — it
  inherits `SCAN_CAP` and therefore fails to escape the bound the method exists to escape; that
  scope-cut is what got #508's 4th attempt rejected.
  **Where each backend's cap-escape is actually demonstrated** (the required-method rule is a
  *convention*, not a structural proof — it stops silent inheritance, it does not by itself stop a
  deliberate `scan()`-then-slice body):
  | Backend | Cap-escape proof | When |
  |---|---|---|
  | redb | leg B's lowered-cap walk | at Check, every run |
  | the two DST sim stores | the same lowered-cap clause driven in-simulator | at Check, via `run_dst` |
  | `metadata-fdb`, `metadata-tikv` | the shared clause under `xtask {fdb,tikv}-conformance` against a real cluster, where `with_scan_cap` lowers the cap the same way | **off-Check**, maintainer-run (see `Verification posture`) |
  Do MUST therefore write the lowered-cap escape as a **shared conformance clause** parameterised
  over a cap-lowering hook, not as a redb-local test, so the same assertion runs on every backend
  that has a driver. Where a backend cannot expose the hook, say so in `build-notes.md` rather
  than quietly leaving that backend unproven.
  **(F) `cargo xtask ci` green**, and the two feature-gated backends type-check — see
  `Verification posture`.
- **Falsifiability:** the RED is producible **in-process on `origin/main` @ `22d71b4`**, with no
  container, cluster or deploy stack: redb in-memory and the madsim sim stores are the whole
  harness. But be honest about its *shape* — **on the base the added test files do not compile**,
  because `scan_page` does not exist. `run-verify.sh` scores that as a red: when the RED leg's
  `cargo test` exits non-zero the `TESTS_RAN == 0` guard at `engine/scripts/run-verify.sh:416-427`
  is inside the *cargo-succeeded* branch and is skipped entirely, so execution falls through to
  the unconditional `PASS — red without the fix` at `:433`. A build error is therefore scored as
  a red over a run that executed nothing.
  **Two obligations follow, and they are not optional.** (1) Do MUST record in `build-notes.md`,
  from the RED leg, exactly what happened — build error vs assertion failures, and how many tests
  ran and failed. A run reporting zero tests is a non-result, and Do must say so rather than let
  the gate's PASS stand unexplained. (2) The **semantic** red is what leg (D) buys: the violating
  doubles make each clause fail *by assertion* against a deliberately wrong implementation, in
  the same `cargo xtask ci` run, which is evidence a compile error can never be. Treat (D) as
  the binding demonstration and the C4-verify PASS as corroboration.
  **Base resolution:** this is a **wave-0** bundle, so neither `$PDCA_BASE` nor
  `$PDCA_VERIFY_BASE` is exported and `run-verify.sh` resolves the brief's own base
  (`_resolve_base_ref`, `engine/scripts/run-verify.sh:180-192`) → `origin/main`. That is correct
  here and needs nothing from the operator.
  **The one place RED is NOT producible at Check** is the feature-gated pair: `metadata-fdb`'s
  and `metadata-tikv`'s bodies are compiled by **nothing** in the default gate — the workspace
  build/clippy/test rows exclude their features, and the feature-gated rows only run when
  `WYRD_FDB_TOOLCHAIN` / `WYRD_TIKV_TOOLCHAIN` are set (`xtask/src/main.rs:1536-1545`,
  `xtask/src/lib.rs:81-130`). That is a named, closed gap, not a discovery for Do — see
  `Verification posture`.
- **Invariant to restore:** **a store primitive may fail loud on a bound, but it may never
  silently omit a key that was present throughout the walk** — enumeration of a namespace must
  be possible whatever the population size, and a continuation must not be able to skip. Stated
  over the category (every backend, every namespace), not over `retire:`/`orphan:`. **Source:**
  the completeness-or-fail-loud clause of the store contract (`crates/traits/src/lib.rs:270-286`,
  #262 / ADR-0011: "a silently truncated `inode:` scan shrinks GC's never-reclaim safety set,
  which is data loss, so this is a correctness constraint, not a tuning knob") extended to the
  paginated case by `0016:2653-2666` — a skipped `retire:` obligation retains its bytes and its
  records **forever**, which is the precise failure the paginated walk exists to prevent.
  SELF-TEST: this cannot be satisfied by guarding one module — it is a trait-level property that
  four independent implementations must each honour, which is why the conformance suite, not a
  callsite, is where it is asserted.
- **Scope:** the `scan_page` trait method and its normative doc contract
  (`crates/traits/src/lib.rs`); **native cursored implementations** on `metadata-redb`,
  `metadata-fdb`, `metadata-tikv` and both DST sim stores
  (`crates/dst/tests/support/mod.rs`); the `contract_scan_page_*` clauses in
  `metadata-conformance` wired into `run_all`; the ~26 in-test `MetadataStore` doubles the
  required method obliges (one delegating line each); and the two new test files. **Out of
  scope:** every consumer — no caller switches to `scan_page` in this slice (GC's `orphan:` walk
  is #637, the `retire:` drain is #636), GC's per-pass page budget (#637's design call), any
  change to `scan` itself or to `SCAN_CAP`, and any file under `docs/design/adr/` or
  `docs/design/specs/`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 635
- **Ordering note:** **wave 0 of the five-slice stack 634 → 635 → 636 → 637 → 508.** No
  dependency edge: 634 needs nothing from 635, and 635's bounded `seg:` range read is
  deliberately a plain `scan` (`0016:2463-2464`), so it does not consume this seam. The
  **conflict is a file conflict**: a required trait method touches all **36** in-test
  `MetadataStore` impls across **26** files (`crates/custodian/tests/{gc,gc_telemetry,
  gc_delete_backstop,rebalance,reconstruction,restore_reconcile,scrub,backfill,
  backfill_telemetry,tier1_disk_faults}.rs`, `crates/core/tests/*.rs`, `crates/server/tests/*.rs`,
  `crates/chunkstore-grpc/tests/*.rs`, `crates/dst/tests/*`), and 635's
  `Flat | Segmented` change touches many of the *same* files' `InodeRecord` construction sites.
  Built blind on one base they collide at the fold. The harness orients an undeclared-direction
  conflict by name order (`src/pdca_harness/waves.py:167-175`, name-lower builds first), which
  puts `issue_634` in wave 0 and `issue_635` in wave 1 — the order this stack wants anyway.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `docker`, `openssl dev (pkg-config + libssl)`, `libfdb_c loadable`, `fdb headers (bindgen)`, `fdb cluster file`, `fdb cluster healthy`, `typos`, `docs-renderer`, `WYRD_FDB_TOOLCHAIN` (no-check: an env flag, not an installable — set it to any value to switch the fdb clippy row on), `WYRD_TIKV_TOOLCHAIN` (no-check: same, for the tikv/etcd clippy rows)
  <br>*(Every token above is on the field's own line deliberately — the driver reads only that
  line, `src/pdca_harness/brief.py:182-190`. `docker` + the four `fdb …` rows and `openssl dev`
  are for the off-Check `xtask fdb-conformance` / `tikv-conformance` legs and for the
  feature-gated clippy rows of `Verification posture`; `typos` + `docs-renderer` because the
  prose gates warn-skip when absent and a locally-green docs change then opens the PR red
  (INTEGRATION §3). All were probed present on this host at Plan.)*
- **Test file:** `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs`, `crates/metadata-redb/tests/scan_page.rs`
  <br>*(Both paths on the field's own line — the driver parses only that line,
  `src/pdca_harness/brief.py:23-31`, `:101-113`, so a path on a continuation line is invisible to
  it.)* The first carries legs A + D (the clauses and their non-vacuity proof), the second legs
  B + C (the cap escape on a real backend). **Both NEW files**, both under a `tests/`
  directory: `run-verify.sh` classifies on an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:92-94`, `:300-311`), so a clause appended to the existing
  `crates/metadata-conformance/src/lib.rs` alone would degrade the gate to green-only and prove
  nothing per-fix. The new conformance clauses themselves live in `src/lib.rs` (that is where
  `run_all` can reach them); the added redb test file **calls those clause functions directly**
  — `wyrd-metadata-conformance` is already a dev-dependency of `wyrd-metadata-redb`
  (`crates/metadata-redb/Cargo.toml`) — so C4-verify, which runs only the added targets, still
  exercises the contract rather than the cap-escape alone.
- **Verification posture:** mixed, per backend — declared here so C2/C4 land as a pre-declared
  sign-off item.
  * **redb — DEFAULT posture**, red→green at Check under **both** gates: `cargo xtask ci`'s
    workspace `test` row and C4-verify (the two added test files are its whole invocation).
  * **The two DST sim stores — DEFAULT posture, but the C4-ci row only.** `crates/dst/tests/support/mod.rs`
    is a *modified* file, not an added one, so it never enters C4-verify's invocation — which is
    correct and deliberate: an added `crates/dst/tests/*.rs` would drag `RUSTFLAGS=--cfg madsim`
    over the whole invocation (`engine/scripts/run-verify.sh:100-131`, `:344-385`) and rebuild the
    other crates against the simulator's dependency graph. Their evidence is `cargo xtask ci` →
    `run_dst` (`xtask/src/main.rs:1575-1614`, the **gating** row), which runs the shared
    conformance clauses in-simulator via `crates/dst/tests/conformance.rs`. Nothing is deferred.
  * **`metadata-fdb` + `metadata-tikv` — DEFERRED compile evidence, and the deliverable is BUILT,
    not scaffolded.** Their `scan_page` bodies are real, native, cursored implementations shipped
    in this slice; what is deferred is only *where* they are compiled and exercised. The default
    gate does not compile them. So: **Do MUST run, in `$PDCA_WORKTREE`, the two rows the gate
    would run with the toolchain flags set** — all **four** of them, not just the two metadata
    crates (`xtask/src/lib.rs:81-137` expands each flag into a backend row **and** a
    `wyrd-server` row): `cargo clippy -p wyrd-metadata-fdb --features fdb --tests`,
    `cargo clippy -p wyrd-server --features fdb,etcd --tests`,
    `cargo clippy -p wyrd-metadata-tikv --features tikv --tests`, and
    `cargo clippy -p wyrd-server --features tikv,etcd --tests` — the server rows are where the
    `MetadataBackend` selection arms live, so a metadata-only check leaves the CLI wiring
    uncompiled and free to rot (this host has
    `libfdb_c`, the FDB headers and openssl dev; both were probed present at Plan) — and paste
    the outcome into `build-notes.md`. A row that cannot be made to compile is a Check §6 item,
    not something to work around by narrowing the trait.
    The **behavioural** green for those two backends is the shared conformance suite run against
    real servers: `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance`, which bring
    up the throwaway Docker clusters and re-run the identical `run_all` clauses
    (`crates/metadata-fdb/tests/conformance.rs:24-40`, `crates/metadata-tikv/tests/conformance.rs:11-34`
    — both skip cleanly with no cluster, which is why the default gate stays green and why this
    is *deferred*, not absent). **Named confirmer: Eduard Ralph at sign-off**, on this host.
- **Production reach:** this slice builds a **seam ahead of its production consumers, by
  design** — "no consumer switches to it in this slice". (a) What honours the seam at Check: the
  conformance clauses on redb + two sim stores, the violating doubles of leg D, and the redb
  cap-escape walk of leg B — all load-bearing, none dead scaffolding (leg B fails against any
  `scan()`-backed body, leg D fails against any of the four wrong semantics). (b) Where the
  production wiring lands: GC's `orphan:` ledger walk and the mark sweep adopt it in **#637**
  (`0016:2688-2691`), and the `retire:` drain in **#636**. (c) Nothing in the live path calls
  `scan_page` when this slice merges, and that is the intended state — bounding GC's per-pass
  page budget is a design call belonging to #637, not a scope cut here.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/metadata-tikv/src/lib.rs:443-476` — `PageStep` / `after_page`, the existing
    cursored paging loop and its `next_page_start` successor-key helper. The TiKV `scan_page` is
    a re-exposure of this machinery, not a new one; note the boundary rule already agreed there
    (`total > cap`, and a **short** page means exhausted).
  * `crates/metadata-fdb/src/lib.rs:1385-1420` (`scan_once`) and `:1762-1790` (`scan`) — FDB's
    own `more()`-driven paging, likewise already cursored.
  * `crates/metadata-redb/src/lib.rs:110-140` (`scan`) and `:73-90` (`with_scan_cap` /
    `scan_cap`) — the embedded range read and the cap knob the test lowers.
  * `crates/metadata-conformance/src/lib.rs:41-58` (a clause's shape) and `:420-441` (`run_all`,
    and the comment explaining why a new clause must be added *there* and nowhere else).
  * `crates/metadata-conformance/tests/demonstrated_red.rs:1-60` — the violating-store /
    `#[should_panic]` non-vacuity pattern leg D must follow, including its dev-scope-only
    justification.
  * `crates/metadata-redb/tests/scan.rs:9-11`, `:20-34`, `:74-90` — the lowered-cap idiom and
    the "proving the ceiling by writing 2^20 keys would be absurd" argument that licenses it.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. `git log -S"scan_page" --all` matches **only** the proposal document (`97e2392` and
  its review rounds) — no implementation has ever existed. `crates/traits/src/lib.rs`'s recent
  history is `3530bd8` (made the scan cap + its error one seam definition, #516) and `8806843`
  (consolidated the store contract) — both *narrow* the seam, neither adds pagination; this
  slice is the first widening and is the **one** trait change 0016 authorises (ADR-0010 /
  ADR-0016 narrow-seam rule, `0016:2645-2647`). No open PRs on the repo. The only rejected prior
  art is inside this harness: #508's 4th attempt shipped `scan_page` as a **default shim over
  `scan()`** and was rejected for exactly that; #508's 7th attempt shipped it inside a 44-file /
  14,117-line cross-plane patch and was rejected at sign-off on reviewability
  (`results/issue_508/iteration-v4/`, `iteration-v7/`, `results/issue_508/review-rejected.md`).
  This slice exists to be the reviewable version of that seam.
- **Disposition hint:** likely-fix

## Motivation

Two multipart-era populations cross `SCAN_CAP` and neither is walkable today.

1. **The retirement drain's `retire:` namespace is deliberately unbounded** (0016 decision 6):
   it absorbs supersede and `unlink` work that used to expand orphans inline, so its size is a
   function of churn, not of a cap. A namespace with no bound cannot be enumerated by a
   primitive with one.
2. **GC's `orphan:` ledger already exceeds the cap on a single object.** 0016 computes it: one
   maximum segmented-object retirement installs **~1.78 M** marks against `SCAN_CAP = 1,048,576`
   (`0016:2649-2651`). `orphan_leases` reads the ledger with one `scan`
   (`crates/custodian/src/gc.rs:322`); past the cap it returns `Err`, which `?`-propagates
   through `gc::reconcile` into `reconcile_step` (`crates/custodian/src/reconciliation.rs:78-85`)
   and aborts the **whole** maintenance pass — GC, scrub, reconstruction and rebalance all stop.
   A durability plane that stops reconciling because one object was large is not an acceptable
   failure mode.

Ordering is the part that is easy to get wrong and expensive to get wrong. Today's `scan` leaves
ordering **unspecified** (`crates/traits/src/lib.rs:770-775`, "Order is unspecified"), and the
shared conformance clause even sorts before asserting (`metadata-conformance/src/lib.rs:41-58`).
A `scan_page` that inherited that freedom could return a continuation that silently **skips** a
key — and a skipped `retire:` obligation retains its bytes and its records forever. Clause (d) is
what the drain actually needs and no more: the drain is idempotent and re-entrant per obligation
(`require(retire:… == prior)`), so a *duplicate* is a no-op while a *skip* is unbounded
retention. That asymmetry is why no snapshot isolation is required of any backend — which is
precisely what keeps this implementable on redb, FoundationDB and TiKV alike.

## Design

### The signature, and why it is not the contract

```rust
async fn scan_page(&self, prefix: &[u8], after: Option<&[u8]>, limit: usize)
    -> Result<(Vec<(Vec<u8>, Bytes)>, Option<Vec<u8>>)>;
```

The four clauses at `0016:2653-2666` are the contract; the doc comment on the trait method is
where they become normative for implementers, and the `metadata-conformance` clauses are where
they become enforceable. Write the doc comment in the register the surrounding trait already
uses (`crates/traits/src/lib.rs:770-786` is the model): state the property, state what breaks if
it is violated, cite the issue.

### The required-method decision (settled here)

**`scan_page` is a REQUIRED trait method — no default body.** A default over `scan()` would be
inherited silently by any backend that forgot to override it, and *nothing* could detect that: the
conformance suite cannot lower a backend's cap through the trait seam (`with_scan_cap` is a
per-backend inherent method, not part of `MetadataStore`), so the cap-escape leg is unreachable
generically. An undetectable wrong default in the one primitive whose whole purpose is escaping
the cap is not a trade worth making, and shipping it is what got #508's 4th attempt rejected.

The cost is the in-test doubles: measured at Plan with a line-anchored declaration search, the
workspace holds **39** `impl MetadataStore for` blocks — **3** production
(`crates/metadata-redb/src/lib.rs:103`, `crates/metadata-tikv/src/lib.rs:947`,
`crates/metadata-fdb/src/lib.rs:1725`) and **36** across **26** files under
`crates/{core,custodian,server,chunkstore-grpc,dst,metadata-conformance}/tests/` — and every one
must grow the method. (An earlier count of 40/37/27 came from an unanchored grep that also matched
a source-shaped string literal.) **Keep that cost mechanical and keep the reviewable surface small:** provide one
shared, clearly-labelled test-grade helper in `wyrd-traits` (a free function over the store's own
`scan`, documented as "for test doubles only — it inherits `SCAN_CAP` and is not a backend
implementation") so each double's impl is a single delegating line. The reviewer then reads three production
backends plus the two sim stores, the conformance clauses, and ~34 identical one-liners — not 39
bespoke implementations. Do MAY choose a different spelling; the *outcome* this brief requires is that no
production backend can inherit a `scan()`-based body, and that the test-double churn is uniform.

### What each backend implements

* **redb** — an embedded range read from the successor of `after` (or the prefix start),
  bounded by `limit`; no cap involvement beyond leaving `scan`'s behaviour untouched.
* **TiKV** — build on the existing `PageStep` / `after_page` / `next_page_start` machinery
  (`crates/metadata-tikv/src/lib.rs:443-476`); it already computes exactly this cursor.
* **FoundationDB** — build on `scan_once` / the `more()` paging loop
  (`crates/metadata-fdb/src/lib.rs:1385-1420`).
* **The DST sim stores** — `SimTikvMetadataStore` (`crates/dst/tests/support/mod.rs:291`) and
  `SimFdbMetadataStore` (`:739`). These are models, not backends; a sorted-map slice is the
  faithful implementation and it must keep the model's await-inside-commit shape intact.

`scan` itself is **unchanged**: same signature, same cap, same fail-loud semantics. This slice
adds a primitive; it does not relax an existing one.

### Why the conformance suite is the enforcement point

`run_all` (`crates/metadata-conformance/src/lib.rs:420-441`) exists precisely so a new clause is
picked up by every backend driver with no per-driver list to drift — the seam that once let the
read-consistency clauses run on redb but skip TiKV. Add the clauses there and the redb, DST
(×3 stores), FDB and TiKV drivers all inherit them. Do **not** fork a per-backend copy; weakening
or forking the suite to make a backend pass violates the invariant the suite exists to enforce.

## Alternatives considered

* **A default body over `scan()`** — rejected above: undetectable inheritance of the very cap the
  method exists to escape. It is also the recorded reason #508's 4th attempt was rejected.
* **Packing `retire:` into fixed shards under today's `scan(prefix)`** — 0016 rejects it
  (`0016:2670-2672`): sharding only divides an *unbounded* population by a constant, so a shard
  can still cross `SCAN_CAP`. The paginated seam is the honest primitive.
* **Raising `SCAN_CAP`** — it is a correctness constraint, not a tuning knob
  (`crates/traits/src/lib.rs:270-282`), and backends deliberately refuse to raise it
  (`crates/metadata-redb/src/lib.rs:73-78`). Raising it moves the cliff, it does not remove it.
* **Requiring snapshot isolation for the walk** — would make the clause unimplementable on
  backends that do not offer it. Clause (d)'s deliberate asymmetry (duplicates permitted, skips
  forbidden) is what buys portability, and it is exactly what the idempotent drain needs.

## Impact & compatibility

* **Additive at the wire and on disk.** No record shape changes, no key changes, no stored data
  is read or written differently. `scan` is untouched.
* **Source-breaking for `MetadataStore` implementors** — by construction (the required-method
  decision). Every implementor in the workspace is either a production backend (4) or a
  dev/test double (~26); there are no external implementors (the crate is unpublished,
  `publish.workspace = true` notwithstanding — the workspace is the whole population).
* **Docs currency:** this slice adds a store primitive, not a persisted record class or an API
  operation, so the architecture documents (`docs/design/architecture/06-runtime-view.md`,
  `08-crosscutting-concepts.md`) are **not** required to change here — they change in #635 and
  #636/#637, which add record classes and loops. If Do finds a paragraph in those files that
  states the store offers only a whole-namespace scan, correct that paragraph and cite it.
* **ADR/spec/proposal files are OUT OF SCOPE.** Do not edit `docs/design/adr/`,
  `docs/design/specs/`, or `0016` itself — ADR immutability (ADR-0001) and architecture-board
  authority (INTEGRATION §4) make any such edit an automatic NEEDS-HUMAN.

## Open questions

1. ~~**`limit == 0`.**~~ **Settled in leg C, not an open question** (removed 2026-07-26: leaving it
   here restated a contract leg C had already fixed, and the two texts disagreed). The single
   contract, stated once and applying to **every** backend: `limit == 0` is **rejected** with a
   named seam error; a `limit` above the store's effective cap is **clamped** to it, never an
   `Err`; and `items.len() <= min(limit, effective_cap)` always. There is no per-backend
   variation to choose.
2. **Whether `next` should be the last key returned or an opaque token.** 0016 settles it as
   `Some(last_key_returned)` (`0016:2657-2659`) — implement that, not an opaque token. Recorded
   here only so Do does not "improve" it.
3. **X70 is this slice's; X90 is NOT — it is #637's.** 0016 lists two distinct obligations:
   **X70**, the `scan_page` ordering and continuation contract, which "lands in
   `metadata-conformance` on every backend" — that is legs A–D here; and **X90**, the `orphan:`
   *pagination scale* case, which needs "a scale case that drives one maximum segmented-object
   retirement past `SCAN_CAP`" (`0016:2903-2910`). X90 is about the **ledger**, and the ledger
   walk is adopted in **#637** (whose leg G already seeds an `orphan:` population past a lowered
   cap) — so X90 is assigned there and this brief claims only X70 plus the generic cap-escape.
   A literal 1.78 M-key case is not runnable inside `cargo xtask ci` in either slice; if the
   maintainer wants real scale it belongs in the off-Check conformance legs — say so at sign-off.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the confirmed implementation defects, not just the review verdicts: - `crates/metadata-redb/src/lib.rs:178` (with `page_limit` at `crates/traits/src/lib.rs:388`): when a store's effective scan cap is 0, `scan_page` returns an UNBOUNDED page (empirically verified: 25 keys seeded, cap 0, limit 5 -> 25 items returned, next: None), directly violating the brief's own "items.len() <= min(limit, effective cap) always" contract. Fix the guard (`>=` not `==`, or floor the effective cap so 0 is rejected like `limit == 0` is) and add a conformance/unit case for a zero effective cap. - The T4 batched multi-pass review failed gating with 6 blocking findings; discharge or rebut each explicitly in the next attempt's build-notes.md. - Also correct, if practical in the same pass: the untested "cursor below prefix" fallback guard (`crates/metadata-redb/src/lib.rs:166` and siblings in fdb/tikv/dst) — add a clause with an `after` lexicographically below the prefix so clause (b) actually exercises it; and reduce the dead-code / visibility risk of `test_double_scan_page` being an unconditional `pub` item of the production `wyrd-traits` crate. - The TiKV real-cluster and `cargo deny` runs remain environmentally blocked here (off-Check per the brief's verification posture) and are not the reason for this rejection — re-attempt only needs to address the items above; the maintainer-run TiKV conformance and fitness-to-purpose question can still be settled at the next sign-off.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 89 mutants tested in 33s: 44 missed, 12 caught, 33 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — T3 Runtime — A maintainer must supply a TiKV topology compatible with the client’s API mode and run `cargo xtask tikv-conformance` until both runners at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69` pass—the available cluster failed every attempt with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before exercising `scan_page`, so live TiKV parity remains unverified.; T4 Contribution — A maintainer must inspect or rerun the reported eight batched-review findings before relying on that gate—`scripts/review-branch --bundle` and its finding artifact were absent from the allowed inputs and target, so its red summary is provisional rather than a confirmed patch defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 69 mutants tested in 28s: 25 missed, 17 caught, 27 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected solely on the two live T4 batched-review findings in review-batch.md (both unaddressed, both in the new scan_page conformance test code added by this patch, not the production backends): 1. crates/metadata-conformance/src/lib.rs:465 — the new scan_page clauses assert on returned keys/cursors but never assert on the returned Bytes values, so a backend returning correct keys with stale/swapped/corrupted values would still pass. Add a value assertion to the shared clause(s). 2. crates/metadata-conformance/src/lib.rs:772 — the fixture hard-assumes the first page always fills its limit (2 items), but the brief's own clause (c) explicitly permits a short non-final page (next: Some(...)) with fewer items. Fix the fixture so it does not fail a conforming store that legitimately returns a short page. All other §6 NEEDS-HUMAN items (T3 Runtime/TiKV, T4 Contribution, Validation fitness-to-purpose, C4 gate flake, external TiKV dependency) were cleared at this sign-off — do not re-litigate those; TiKV is accepted as backseat as long as redb/FDB stay green, and the C4 exit-101 gate was independently reproduced clean (flake). Only the two findings above are the reason for iteration.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 72 mutants tested in 29s: 19 missed, 27 caught, 26 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — T4 Contribution — A maintainer must decide whether to rely on the reported zero-blocking batched review — `scripts/review-branch` and its finding report are absent from the allowed target/inputs, so that gate cannot be independently reproduced; affected-path history and closed-PR search otherwise found no prior implementation (only proposal PR #627), and current open PRs are dependency/workflow updates.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 84 mutants tested in 34s: 22 missed, 29 caught, 33 unviable
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — T4 Contribution — A maintainer must decide whether the reported zero-blocking batched review is sufficient without its unavailable `scripts/review-branch` executable/report — this matters because definition of done requires one deep multi-pass review; the independent affected-path merged/closed-PR check found no prior implementation (`AGENTS.md:206`).
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 77 mutants tested in 33s: 18 missed, 28 caught, 31 unviable
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
