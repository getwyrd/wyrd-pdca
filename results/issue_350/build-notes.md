# Build notes — issue #350 / placement-backfill-migration

Target branch: `getwyrd/wyrd @ main`, built at `389b4d23f1216c6d53632545a32ea9832acc9688`
(merge of PR #397, which lands #348's classifier — the brief's stated prerequisite).

## What I built

`crates/custodian/src/backfill.rs` (new module) — a custodian pass that:

1. Scans every **committed** inode record (`meta.scan(b"inode:")`,
   `backfill.rs:78`).
2. Classifies each chunk's committed `placement` by **reusing** #348's single-source
   classifier, `ChunkRef::checked_fragments()` (`crates/core/src/metadata.rs:174-185`)
   — `Ok(_)` + `placement.is_empty()` → queue for backfill; `Ok(_)` + non-empty →
   already explicit, leave untouched (idempotent); `Err(m)` → malformed, leave
   untouched and emit an operator signal (`backfill.rs:96-107`). I deliberately did
   **not** open-code a second `len != fragment_count()` check — the `is_empty()` read
   is just inspecting the already-classified field, never re-deciding validity.
3. For any record with ≥1 empty-placement chunk, materializes
   `(0..fragment_count()).map(u64::from)` into those chunks' `placement`
   (`backfill.rs:116-119`) and commits the whole updated `chunk_map` **in one
   version-conditional `MetadataStore::commit`**, CAS'd on the exact prior record
   (`backfill.rs:126-138`) — the same `require(prior)`/`put(next)` shape
   `rebalance.rs:evacuate_chunk` uses at `crates/custodian/src/rebalance.rs:284-294`
   (I inlined the batch build rather than calling
   `metadata::commit_chunk_map` — see "Alternatives" below).
4. Emits the **empty-placement-remaining** count as a `gauge.` sample on the
   `tracing`→OTel bridge every pass (`backfill.rs:161-176`), following the exact
   emission idiom `rebalance.rs:emit_domain_utilization`
   (`crates/custodian/src/rebalance.rs:318-325`) uses (`gauge.<name> = value`).
   Also emits `monotonic_counter.backfill_chunks_filled`,
   `monotonic_counter.backfill_malformed_placement`, and
   `monotonic_counter.backfill_conflict` plus matching audit-target `tracing::info!`/
   `warn!` events, mirroring `gc.rs`'s `emit_reclaim`/`emit_malformed`/`emit_skip`
   trio (`crates/custodian/src/gc.rs:295-338`).

`crates/custodian/src/lib.rs:23,32` — `pub mod backfill;` + `pub use
backfill::BackfillContext;`, matching every sibling loop's export shape.

`crates/custodian/tests/backfill.rs` (new, the brief's named test file) — 5 tests
covering the brief's five required legs:

- `backfills_identity_placement_for_an_empty_placement_committed_chunk` — (a):
  `placement.len() == fragment_count()`, `placement[i] == i`, version bumped by
  exactly one CAS commit.
- `a_racing_writer_wins_the_cas_and_backfill_retries_on_a_later_pass` — CAS-conflict:
  a `RacingMeta` wrapper (copied in shape from `rebalance.rs`'s `RacingMeta`,
  `crates/custodian/tests/rebalance.rs:1037-1107`) injects one concurrent inode
  mutation between backfill's scan and its commit; asserts the racing write's version
  lands untouched by backfill, then that a second pass (uncontested) converges.
- `malformed_placement_is_never_rewritten` — (b): a length-1 vector against
  `fragment_count() == 3` is left byte-for-byte, no commit lands.
- `already_explicit_full_length_placement_is_left_untouched` — idempotence leg.
- `emitted_remaining_count_reaches_zero_once_backfill_covers_the_store` — (c): three
  committed records with empty placement, gauge read back via
  `DurabilityTelemetry::gather_prometheus()` (same in-process seam every other
  custodian telemetry test uses, e.g. `crates/custodian/tests/gc_telemetry.rs:194-205`)
  asserted `== 0.0` post-pass, plus a direct store re-read confirming every chunk now
  carries an explicit placement.

## Why this shape

**Reused #348's classifier, not a fresh length check.** The brief is explicit that
`checked_fragments()`/`placement_is_valid()` (`crates/core/src/metadata.rs:159-185`)
is the single-source classification boundary the maintenance loops share (ADR-0040
decision 4) and that open-coding a second length check is "the defect class ADR-0040
exists to foreclose." `backfill.rs:88-95`'s `match chunk.checked_fragments() { Ok(_)
if chunk.placement.is_empty() => …, Ok(_) => …, Err(m) => … }` calls that shared
classifier for the malformed/valid decision; the `is_empty()` guard only
distinguishes the two **valid** sub-cases (empty vs. already-full-length) using the
raw field already in hand — it does not re-derive `len != fragment_count()`.

**Manual `WriteBatch` over calling `metadata::commit_chunk_map` directly.**
`commit_chunk_map` (`crates/core/src/metadata.rs:299-317`) takes `store: &impl
MetadataStore` (a generic bound), but `BackfillContext.meta` is `&dyn MetadataStore`
(a trait object) — matching `rebalance.rs`'s `RebalanceContext.meta` and `gc.rs`'s
`referenced_fragments(meta: &dyn MetadataStore)`. I grepped the whole tree
(`crates/**/*.rs`) for a precedent of passing a `&dyn MetadataStore` value into a
`&impl MetadataStore`-generic function and found none — every existing call site
either has a concrete store type in scope or (like `rebalance.rs:evacuate_chunk`,
`crates/custodian/src/rebalance.rs:284-294`) builds the `WriteBatch` by hand and
calls `ctx.meta.commit(batch)` directly. Rather than gamble on an untested
generic/dyn-object interaction on a headless run, I copied the proven-working manual
`require(prior)`/`put(next)` shape from `evacuate_chunk` verbatim (four lines longer
than a hypothetical `commit_chunk_map(ctx.meta, …)` call, if it even compiles) — zero
new risk, same CAS semantics, same citation the brief itself points at
(`rebalance.rs:evacuate_chunk` ~:269-289, actual lines 276-294 on this branch).

**Did NOT wire `backfill::reconcile` into `reconciliation.rs:reconcile_step`.** The
brief is explicit: "Hosting the pass as a `backfill::reconcile` step inside
`reconciliation.rs:reconcile_step` is ILLUSTRATIVE; the BINDING conditions are
(a)–(c)." I quantified the cost of doing it anyway before ruling it out: `\bgrep -c
'reconcile_step('` across the tree finds **72 call sites in 11 files** —
`crates/custodian/tests/{rebalance,scrub,gc,reconstruction,skeleton,gc_telemetry,
tier1_disk_faults}.rs`, `crates/custodian/src/reconciliation.rs` (the definition),
and, outside this crate entirely, `crates/dst/tests/custodian.rs` and
`crates/chunkstore-grpc/tests/{tier1_jepsen_consistency,tier2_kill_reconstruct}.rs`.
`reconcile_step`'s five loop parameters are **positional**
(`crates/custodian/src/reconciliation.rs:65-73`), so adding a sixth — regardless of
where in the parameter list — breaks every one of those 72 call sites' arity; each
needs a mechanical `, None` (or a real `Some(&ctx)`) inserted. That is a ~72-line
diff spread across test suites this brief does not own (DST and gRPC integration
tests unrelated to backfill), to satisfy a hosting detail the brief itself downgrades
to illustrative. I exposed `backfill::reconcile` as a `pub` function instead (unlike
its siblings' `pub(crate) reconcile`, only reachable via `reconcile_step`) so the
test drives the real, production classify/backfill/commit/emit logic directly — the
binding (a)–(c) behavior is exercised exactly as written, at a fraction of the diff.
A later slice can thread it through `reconcile_step` with a normal 72-site mechanical
follow-up once that's actually wanted; noted in-module at `backfill.rs:36-40` so the
next contributor sees the deferral and its reasoning, not just an absence.

**Per-record batching, not per-chunk.** A record with two empty-placement chunks
gets ONE commit for both (`backfill.rs:113-138`), mirroring how `rebalance.rs`'s
`evacuate_chunk` re-places multiple fragments of one chunk in a single commit
(`crates/custodian/tests/rebalance.rs:898-1027`, the `evac.len() > 1` leg) — fewer
CAS round-trips, and it means a malformed chunk sharing a record with an empty one
doesn't block the empty one's backfill (the malformed entry is simply left
unchanged inside the same rewritten `chunk_map`).

**Metric names are Do's call per the brief's "Open questions."** I named the gauge
`backfill_placement_remaining` and the counters
`backfill_chunks_filled`/`backfill_malformed_placement`/`backfill_conflict`,
following the existing `<loop>_<noun>` convention (`gc_fragments_reclaimed`,
`rebalance_fragments_evacuated`). The brief flags this as maintainer-confirms-at-
sign-off, not a build blocker.

## What I verified, and how

- `cargo test -p wyrd-custodian --test backfill` (direct, in `$PDCA_WORKTREE`): 5/5
  pass post-patch.
- `cargo fmt -p wyrd-custodian -- --check`: clean (I ran `cargo fmt -p
  wyrd-custodian` once to settle three formatting nits `rustfmt` wanted in the new
  test file, then re-checked clean).
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean (fixed one
  `clippy::double_ended_iterator_last` on `gauge_value`'s `.last()` → `.next_back()`).
- **The project's own C4-verify gate**, `./engine/scripts/run-verify.sh`
  (`PDCA_BUNDLE=results/issue_350`, run from the `wyrd-pdca` root, per
  `pdca.toml`'s `[[gates.checks]] id = "C4-verify"`) — the required red→green
  discriminator, not a hand-rolled invocation:
  - **GREEN** (fix applied): `cargo test -p wyrd-custodian --test backfill` → 5
    passed, 0 failed.
  - **RED** (production reverted — `backfill.rs`/`lib.rs` reverted to `origin/main`,
    the new `backfill.rs` test file kept): `error[E0432]: unresolved import
    'wyrd_custodian::backfill'` — the module genuinely does not exist pre-patch, so
    the test fails to build. This is the brief's own framing of the baseline ("no
    `crates/custodian/src/backfill.rs` exists… no drain signal exists" — the Prior-
    art check, `brief.md`) made mechanical: there is no partial/stub backfill
    behavior to exercise pre-patch, so the compile failure *is* the demonstrable red
    for this NET-NEW slice (the brief's Verification-posture: "the red is
    demonstrable, not absence-only" — it is captured by an actual run of the
    project's gate against the actual base commit, not asserted by inspection).
  - `run-verify.sh: PASS — red without the fix, green with it.`
- Confirmed the fix is scoped: `cargo build --workspace --tests` (both with and
  without my changes, via `git stash`) hits a **pre-existing, patch-independent**
  sandbox gap building `alloca v0.4.0` (a bench-only transitive dep of `wyrd-core`
  via `criterion`, pulled in only by `--workspace --all-targets`): `zig cc`'s clang
  frontend rejects `cc-rs`'s `--target=x86_64-unknown-linux-gnu` query
  ("UnknownOperatingSystem"). This exact failure, root cause, and conclusion are
  already documented in this bundle's own prior cycle
  (`results/issue_330/iteration-v1/build-notes.md:125-134`, `results/issue_330/
  build-notes.md:113-147`): "Not a real-CI concern: the project's actual C4-ci/
  C4-verify gates run with a real cc, where this shim workaround is unnecessary."
  I did not touch any bench/`Cargo.toml`/`Cargo.lock` to route around it — out of
  this brief's scope, same reasoning as the prior cycle. The sandbox's `cc`/`gcc`
  shim (`~/.local/bin/cc` → `zig cc`, from that same prior cycle) additionally had
  its backing `ziglang` venv (`/tmp/pyzig`) missing this run (a stale `/tmp` from a
  container reset) — I recreated it (`python3 -m venv /tmp/pyzig && /tmp/pyzig/bin/
  pip install ziglang`, resolved from pip's local wheel cache, no network) purely to
  get `cc`/linking working again for my own crate-scoped `cargo build`/`test`/
  `clippy`/`fmt` runs and for `run-verify.sh`'s worktree build; this touches no
  repository file and isn't part of the patch.

## Alternatives considered and ruled out

- **A `throttle`/batch-size limit on the scan** (so one pass doesn't rewrite the
  entire store at once): the brief doesn't ask for pacing, and every sibling loop
  (`gc::reconcile`, `rebalance::reconcile`) also scans+acts over the whole
  `inode:` prefix unthrottled per pass — matching that shape rather than inventing a
  new one.
- **Rewriting malformed vectors to identity too**: explicitly rejected by the brief's
  own "Alternatives considered" (ADR-0040 decision 3 — a non-empty wrong-length
  vector can only mean truncation/corruption; rewriting it fabricates placement and
  destroys the operator signal). Not implemented.
- **A separate `emit_remaining` scan fused into the main backfill loop** (tracking a
  running remaining-count instead of a second `meta.scan`): would save one scan per
  pass but couples "count what's left" to "what this pass touched," which silently
  goes wrong the moment two backfill instances (or a backfill + another writer) run
  concurrently — a fresh post-pass scan is the only count that's actually accurate
  regardless of what else touched the store meanwhile. The extra scan cost is the
  same shape gc.rs already pays (`referenced_fragments` is a full `inode:` scan on
  every GC pass too, `crates/custodian/src/gc.rs:112`).

## Validation update — re-run after the toolchain fix (2026-07-02)

The original Check recorded **C4-ci = fail** (`cargo clippy --workspace --exclude
wyrd-dst --all-targets` exit 101). As "What I verified" documents, that was a
**patch-independent sandbox toolchain artifact**, not a code defect: the `cc`/`gcc`
shim resolved to `zig cc`, whose clang frontend rejects `cc-rs`'s
`--target=x86_64-unknown-linux-gnu` triple while building the bench-only
`criterion → alloca` C dependency that `--all-targets` pulls in; and
`cargo-deny`/`cargo-machete` were not installed.

Re-validated with the toolchain repaired — `cc` is now real **gcc 15.2** (not the zig
shim), and `cargo-deny 0.19.9` + `cargo-machete 0.9.2` installed:

- Against the same base **`389b4d2`** (the #397 merge) with `patch.diff` applied, the
  full gate **`cargo xtask ci` passes all checks**: fmt · clippy (the previously
  failing leg, now clean) · build · `test --workspace` (incl. the new
  `tests/backfill.rs` 5/5) · `cargo-machete` (no unused deps) · `cargo deny check`
  (advisories/bans/licenses/sources ok) · conformance (5 valid + 6 invalid vectors) ·
  statics (ADR-0035) · the madsim DST sweep. Final line: `xtask ci: all checks passed`.
- The deterministic gate was then **regenerated via `pdca gates 350`** → `check-gates.json`
  now reads **overall = pass**, **C4-ci = pass** ("xtask ci: all checks passed"),
  **C4-verify = pass** (run-verify red→green, unchanged).

No source, `Cargo.toml`/`Cargo.lock`, or patch content changed in this re-validation —
only the recorded gate result was corrected to reflect a valid run. The remaining
NEEDS-HUMAN items (T5 reachability / metric shape, the unfenced `pub reconcile`, and
fitness-to-purpose) are genuine sign-off calls, unrelated to the toolchain, and still
stand.
