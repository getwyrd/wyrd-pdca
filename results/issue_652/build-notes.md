# Build notes — issue 652 / startup-recovery-total-and-bounded

## What changed, and why

`Gateway::recover()` (`crates/server/src/lib.rs:123`) is the one call `Gateway::new`'s
composition root runs **before the gateway serves anything**. Pre-patch it delegated to
`metadata::high_water_marks`, which was neither total nor bounded:

1. It `decode(&value)?`-ed every `inode:` record (pre-patch `metadata.rs:2081`), so one
   undecodable value turned the whole call `Err` and cost every healthy object its
   availability.
2. It read `inode:`, `pending:`, and `orphan:` with `MetadataStore::scan` (pre-patch
   `metadata.rs:2077,2094,2105`), which is complete-or-fail-loud at `SCAN_CAP`
   (`crates/traits/src/lib.rs:286`) — the bounded-page seam `scan_page` (#634, PR #645)
   exists precisely to escape that, and nothing in `metadata.rs` used it yet
   (`git grep -n "for_each_page\|scan_page" -- crates/core/` was empty pre-patch).
3. It also computed a chunk-id floor (`max_chunk`) that `Gateway::recover` has discarded
   since #487 (`fdd34f1`, 2026-07-08, `let (max_inode, _max_chunk) = …`) — dead code, and
   #647 (closed unmerged) grew a ~282-semantic-line byte-scavenging apparatus around it that
   itself under-approximated on a corrupted flat root (reads as `Optional`-classed-corrupt,
   contributes 0 — a silently low floor).

The fix, scoped exactly as the brief's Plan decision settles it (DELETE, not wire-up):

- `high_water_marks` now walks only `inode:`, through a new `for_each_page` helper
  (`metadata.rs:2033-2067`) built on `scan_page` — never `scan`. Its cursor loop mirrors the
  already-merged peer seam's paging contract (`crates/traits/src/lib.rs:1019-1023`,
  `:1037-1046`, `:1078-1092` — clauses 2/3, and "a page never fails with
  `ScanCapExceeded`").
- An `inode:` value that fails to decode no longer `?`-propagates: its id is still read from
  its **key** (`parse_inode_key`, unchanged) and the record is attributed via
  `tracing::warn!(target: "wyrd.metadata.audit", key = ..., ...)` — audited, not swallowed,
  and the walk continues (`metadata.rs:2117-2139`).
- The chunk-id floor, the `pending:`/`orphan:` walks that fed it, and the now-orphaned
  `parse_pending_chunk_key` helper are removed outright. `high_water_marks`'s signature
  changes from `Result<(InodeId, ChunkId)>` to `Result<InodeId>`; its one callsite
  (`Gateway::recover`, `lib.rs:124`) drops the discarded second element.
- The two `lib.rs` doc comments that described `high_water_marks` as scanning/recovering the
  `< 2^64` in-process chunk space (`mint_chunk_id` and `random_chunk_epoch` docs, pre-patch
  `lib.rs:237,251`) are corrected — brief Scope explicitly names "`Gateway::recover` and the
  doc comments describing what recovery does and does not recover" as in-scope.
- The standing unit test `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
  (pre-patch `metadata.rs:3417-3446`) is deleted, with a code comment in its place (not just
  the commit body) explaining why: its hazard needed a minter allocating chunk ids below
  `2^64`, and #487 removed the last such minter on 2026-07-08 — the scenario is unreachable
  on this tree, and its live half (a walk must not silently under-count) is superseded by
  the new target's totality-over-damage criterion.

## What I took from the salvage diff, and what I explicitly left behind

Per Citations expected, I extracted and adapted exactly the two things the brief names from
`results/issue_652/sources/salvage.diff`:

- The `for_each_page` helper (salvage.diff:4569-4591) — adapted essentially verbatim (same
  cursor-loop shape over `scan_page`), with its doc comment rewritten to cite this tree's
  current line numbers.
- The paged, containment-commented shape of `high_water_marks` itself (salvage.diff's
  version at :5247-5322) — but **only the inode half**. Salvage's version still computed
  `max_chunk` via a fourth paged walk (`seg:` via `segment_chunk_floor`) plus the `pending:`
  and `orphan:` walks, all preserved from before the Plan decision to delete the chunk floor
  was settled.

Left behind entirely, as instructed: `RecoveredIds`, `ClassIds`, `unreadable_record_floor`,
`raw_chunk_id_floor`, `json_chunk_id_floor`, `scavenged_chunk_id_floor`,
`torn_digit_escape`, `json_string_token`, `segment_chunk_floor` (salvage.diff:4593-5154 and
its dependents) — this is the ~282-semantic-line #647 apparatus the brief's Scope section
says to leave behind. None of it is imported or referenced anywhere in this patch.

## Alternatives considered and their cost

**Wire the chunk-id floor to a real caller instead of deleting it.** This is the *other*
permitted outcome the issue names, but the brief's Plan section settles it (maintainer
ACCEPTED, 2026-08-02): #487 (2026-07-08) already removed the only consumer, and neither
minter in the tree (`mint_chunk_id`, coordination-free ≥2^127; `cli::chunk_id_minter`,
`(inode << 64) | seq`, ≥2^64) mints into the `< 2^64` space the floor would guard. "Wiring"
would mean inventing a consumer with no correctness need — the opposite of "smallest change
that restores the invariant." Cost if attempted: keeping salvage's floor apparatus verbatim
(`RecoveredIds`/`ClassIds`/`raw_chunk_id_floor`/`json_chunk_id_floor`/
`scavenged_chunk_id_floor`/`torn_digit_escape`/`json_string_token`/`segment_chunk_floor`,
salvage.diff:4593-5154) is ~282 semantic lines of code whose only purpose is feeding a
number nothing reads — concretely, `git diff --stat` on that block alone in salvage.diff
spans ~560 diff lines (insertions) for zero behavioral effect on any minter in this tree.
Rejected on both correctness grounds (the settled Plan decision) and cost (an order of
magnitude larger diff for a dead computation).

**Keep `high_water_marks` returning `Result<(InodeId, ChunkId)>` with a hardcoded `0` for the
chunk half, to avoid a signature change.** Rejected: a stub `0` return is exactly the "silent
success on a call with no work to do" shape the recurring-defect-class rubric warns against,
and it is *why* the falsifiability section is explicit that driving `Gateway::recover()`
(signature-stable) rather than `high_water_marks` (whose signature this patch changes) is
load-bearing — a same-signature stub would still compile on the pre-fix tree, so a test
naming `high_water_marks` directly would silently pass RED as a compile success rather than
assert anything. Better to let the signature change ripple to its one callsite (a
one-line drop of a discarded tuple element, exactly the budget note's carved-out mechanical
migration) than keep a permanently-zero field around.

**Log the undecodable record via `tracing::error!` instead of `warn!`, or return an
`Err`-shaped-but-caught sentinel.** Rejected: an `error!` reads as "the process is in
trouble," which is the wrong signal for a record that is *contained*, not fatal — the
gateway starts and every healthy object serves. `warn!` on the `wyrd.metadata.audit` target
matches the module's existing audit-logging idiom (the salvage diff's own
`emit_record_unreadable` used `warn!` for the same reason, salvage.diff:5174).

## Refutation (forced self-check)

**(a) Genuine red?** Yes — verified directly, not asserted. I ran
`git stash push -- crates/core/src/metadata.rs crates/server/src/lib.rs` (leaving only the
new test file in place), then `cargo test -p wyrd-server --test gateway_recover_totality`:
both tests failed with real assertion panics —
`recover() must be Ok(()) despite one undecodable inode: record ... Error("expected ident",
line: 1, column: 2)` and `... ScanCapExceeded { cap: 1048576, prefix: [105, 110, ...] }` —
i.e. `high_water_marks` genuinely returned `Err` on the pre-fix tree, matching the brief's
Falsifiability section exactly. `git stash pop` restored the fix and both tests went green
again (`cargo test -p wyrd-server --test gateway_recover_totality` → `2 passed`). I also
independently re-verified by `git worktree add`-ing a **fresh** checkout of `origin/main`
(`d50f0ca`) in scratch, `git apply`-ing `patch.diff` there, and running the same test target
— green, in a tree that never saw my in-progress edits.

**(b) Production path?** Yes. Both tests drive `Gateway::recover()`,
`Gateway::put_object()`, and `Gateway::get_object()` — the real production methods, over a
real `RedbMetadataStore` (criterion 1) — no mock of `Gateway` itself. Criterion 2's
`ScanCapExceededStore` is a thin wrapper that forwards `get`/`scan_page`/`commit` to a real
`RedbMetadataStore` unchanged and only overrides `scan` to fail — the production
`high_water_marks`/`for_each_page` code path is exercised exactly as it runs in production;
only the one method this criterion needs to prove is *never called* is stubbed, and it is
stubbed to refuse (the failure direction), not to fake success.

**(c) Fixture includes the fault?** Yes. Criterion 1's fixture carries the actual fault
(a raw undecodable `inode:` value, written directly via `WriteBatch`, sitting in the same
store as a real committed object) rather than a store curated to exclude it. Criterion 2's
fixture is the store the fault would trigger against `scan` — proven by a sanity assertion
in the test itself (`MetadataStore::scan(&sanity, ...)` must return `Err`) before the main
scenario runs, so the double is not silently a no-op wrapper that happens to never
demonstrate the refusal.

## Rubric self-review (`AGENTS.md` § Review rubric & protocol, in `$PDCA_WORKTREE`)

- **One clock per correctness lifecycle**: no new clock read. The test file originally
  carried a leftover `#![allow(clippy::disallowed_methods)]` copied from
  `s3_http_wire.rs:34` out of habit; removed once I confirmed nothing in this file calls
  `SystemTime::now()` (`clippy.toml`'s only disallowed method) — an unjustified blanket
  allow is itself a rubric violation in spirit (the convention is a per-site allow *plus* a
  reason naming the clock source).
- **Narrow trait seams / dependency direction**: unchanged — `high_water_marks` still takes
  `&impl MetadataStore` generically; no new trait dependency.
- **Metadata validation boundaries**: unaffected — decode-time invariants (#648) are
  untouched; this patch changes what a caller does with a decode *failure*, not what decode
  itself accepts.
- **Docs currency**: `high_water_marks` is a private-to-the-workspace helper, not a port /
  API operation / RPC / CLI flag / persisted field, so the living-architecture-doc trigger
  does not fire; the brief's Scope section explicitly rules "any docs paragraph" out for this
  slice for the same reason, naming #648/#649-651/#653 as the PRs that own those edits.
- **Absent or unsupported entries** (recurring defect class): an undecodable `inode:` record
  is never silently skipped — it is attributed via `tracing::warn!` on the
  `wyrd.metadata.audit` target with its key, at the one call site that reads it
  (`metadata.rs:2127-2134`). It is not an `Err` and not a repair-ledger enqueue, because
  neither is available to a call that must remain total-over-damage by contract (that is the
  Invariant to restore this slice is scoped against) — but it is never silent.
- **Test fidelity**: the new target drives real production composition
  (`RedbMetadataStore` + `FsChunkStore` + `MemCoordination`), matching
  `s3_http_wire.rs:665-751`'s precedent exactly; no DST/simulation surface is touched by this
  slice.

## Test runner used

`cargo test -p wyrd-server --test gateway_recover_totality` — the exact command the brief's
own Falsifiability section names as what `run-verify.sh`'s C4-verify gate computes and runs
for this bundle (single-discriminator `ADDED_TEST crates/server/tests/gateway_recover_totality.rs`).
I did not run the full `cargo xtask ci` (the C4-ci gating check) myself — that is a
multi-minute whole-workspace gate (fmt + clippy --all-targets + build + test + cargo-deny +
conformance + statics + madsim DST), reserved for Check, and running it here would risk the
10-minute Bash-tool timeout on a cold-ish cache for no additional evidence beyond what the
scoped commands below already established. In its place I ran, scoped to the touched
crates, everything `ci` would run over them:

- `cargo fmt --all -- --check` — clean (after running `cargo fmt --all` once to apply
  rustfmt's own reformatting of the new test file, which I then re-verified as a no-op).
- `cargo clippy -p wyrd-core -p wyrd-server --all-targets` — clean, no warnings (workspace
  lints deny `clippy::all` and `rustdoc::broken_intra_doc_links`; I fixed two would-be broken
  intra-doc links — `[SCAN_CAP]` → `` `SCAN_CAP` `` — to match this file's own existing
  idiom for referencing a symbol from `wyrd-traits` without importing it, `metadata.rs:2337`
  pre-patch).
- `cargo test -p wyrd-core --lib` — 41 passed, 0 failed (no regression in the co-located
  `metadata.rs` unit tests, including the ones inside the `segmented_shape_invariants`
  module that share this file).
- `cargo test -p wyrd-server --test s3_http_wire recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`
  — passed unchanged (the brief's named no-regression case).
- `cargo test -p wyrd-server --test gateway_recover_totality` — the new target, both legs
  green.

## Mechanical acceptance check (criterion 3)

`git grep -n "_max_chunk" -- crates/` on the patched tree (both in my working worktree and
in the fresh scratch checkout with only `patch.diff` applied) returns nothing — verified
directly, not assumed. (My first draft's doc comments quoted the pre-patch source text
`let (max_inode, _max_chunk) = …` for readability, which itself matched the grep pattern; I
reworded both occurrences — `metadata.rs` and the new test's module doc — to describe the
discarded binding in prose instead of quoting its exact identifier, so the mechanical check
that stands in for "the dead half is gone" is unambiguous.)

## Budget

3 files touched (`crates/core/src/metadata.rs`, `crates/server/src/lib.rs`, the new test) —
well under the ≤15 cap. `git diff --stat` on the two production files: 118 insertions(+),
107 deletions(-) — net +11 lines, almost entirely doc-comment rewording plus the
`for_each_page` helper; nowhere near the ≤1,500 semantic-line budget, exactly as the brief
predicted ("this slice should land far under budget").

## External dependencies

None used beyond what the brief names. `async-trait` (for the `ScanCapExceededStore` test
double's `MetadataStore` impl) is already a normal (non-dev) dependency of `wyrd-server`
(`crates/server/Cargo.toml`), confirmed before use rather than assumed from the brief's
claim. No Docker, no protoc, no live backend, no new crate dependency, no docs-paragraph
gate in play.
