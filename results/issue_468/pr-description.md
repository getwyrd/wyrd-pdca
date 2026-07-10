# Give the FoundationDB backend a deterministic-simulation story

## Summary
**User impact:** FoundationDB is the storage backend Wyrd runs in production, yet
it was the only metadata backend never put through its worst failure — a commit
the database reports it *may or may not* have applied. If that case were
mishandled, a write could be silently lost, or two concurrent writers could both
believe they won, and no automated test would have caught it before a release.

This adds a deterministic simulation of that FoundationDB failure, so the
production backend is now held to the same automated correctness bar — under the
same concurrent-writer stress — as the backends it replaces.

Reported in #468.

## What to look at
The change is **test-only**: nothing under the shipped crates changes, and no new
dependency is added. There are three pieces:

- a simulator model of the FoundationDB backend that can, on a seed-chosen
  commit, return "I can't tell you whether this landed" — and then decide, from
  the seed, whether it actually did;
- a suite that races four concurrent writers against that failure and checks that
  the outcome is always consistent once each writer re-reads to settle it;
- a guard that inspects the simulator's resolved dependency graph and fails if the
  real FoundationDB client library could ever be linked into it (linking it would
  spawn a network thread and destroy the simulator's deterministic replay).

To exercise it: `cargo xtask dst` (also run inside `cargo xtask ci`) sweeps the
seeds; `cargo test -p wyrd-dst --test no_fdb_linkage` runs the linkage guard on
its own.

## Root cause
The simulator pins the metadata-store contract with two in-simulator
implementations (redb and a simulated-TiKV model), but the FoundationDB client
binds a native library that boots its own network thread and does
non-deterministic I/O, so it can never run inside the seed-replayed simulator.
The one genuinely new failure FoundationDB introduces — an ambiguous commit
(`crates/metadata-fdb/src/lib.rs:67-73`, `:159-166` on the target branch) — was
therefore verified by nothing: not the shared contract suite (no clause touches
it), not a real cluster (a healthy `fdbserver` cannot be made to emit it on
demand), and not the simulator (the model was absent).

## Fix
A seed-driven `SimFdbMetadataStore` is added beside the existing simulated-TiKV
model as a second parametrization of the shared skeleton — not a second
framework. It reproduces the production driver's exact classification of the two
undeterminable outcomes: `1021 commit_unknown_result` (out of flight, settleable
by one re-read) and the strictly weaker `1031 transaction_timed_out` ("promises
nothing" — may still land after the error), surfaced as a distinguishable typed
error rather than a normal conflict, matching `CommitOutcome`'s two variants
(`crates/traits/src/lib.rs:354-357`) with no new variant added. A new property
body races the version check-and-set under an injected ambiguity nemesis and
settles every ambiguous result by re-reading the store's own state before
counting winners. A companion guard resolves the `wyrd-dst` feature-unified
dependency graph (via `cargo tree`, forcing `--cfg madsim` so it also covers the
target-gated section) and asserts neither `foundationdb` nor `foundationdb-sys`
can appear.

## Verification
- **Claim:** the FoundationDB model passes the *identical* shared metadata-store
  contract every other in-simulator backend passes.
  - **Checked:** `crates/dst/tests/conformance.rs:58`
    (`sim_fdb_backend_passes_shared_contract`) drives the shared `run_all` suite,
    with the ambiguity nemesis off (the shared clauses assume determinate
    commits).
- **Claim:** across the seed sweep no invariant is violated — an ambiguous commit
  is never counted a winner or loser without a settling re-read; once settled,
  exactly one writer wins and the version bumps at most once; no torn write is
  observable.
  - **Checked:** `crates/dst/tests/commit_ambiguity.rs:335`
    (`commit_ambiguity_invariants_hold_under_the_dst_seed_sweep`, the `1021`
    version-CAS race) and `crates/dst/tests/commit_ambiguity.rs:878`
    (`contention_under_1031_keeps_exactly_one_winner_through_the_deferral`, the
    `1031`-under-contention race), which additionally assert the deferral counters
    fired so a hollow sweep fails loudly.
- **Claim:** each invariant is load-bearing, not a tautology.
  - **Test:** `crates/dst/tests/commit_ambiguity.rs` ships demonstrated reds —
    e.g. an observer that assumes an ambiguous commit did not land (`:407`), one
    that counts every timed-out commit a winner (`:917`), and a store fidelity
    that skips the deferral resolver re-check and clobbers the winner (`:1029`) —
    each pinned to its specific panic message. They fail (panic as expected) with
    the fix and stop panicking if the modelled behaviour is reverted.
- **Claim:** the real FoundationDB library can never be linked into the
  simulator.
  - **Checked:** `crates/dst/tests/no_fdb_linkage.rs:282`
    (`the_dst_dependency_graph_links_no_libfdb_c`), with planted reds proving the
    scanner catches both the rename form and a transitive edge
    (`:305`) and the real `foundationdb` dependency with its feature on (`:251`).
- **Whole gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, the full DST
  seed sweep, `cargo deny`, conformance) exits 0; the new test files are absent on
  the base branch, so pre-fix the behavioural suite is a compile/absence red and
  post-fix it is green.

Fixes #468
