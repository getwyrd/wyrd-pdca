# check-advisory-adversary.md — issue 257 (m4.6, iteration 13) — adversarial pass

Skeptic's pass over `patch.diff` + `brief.md` + `check-gates.json` only, grounded on
`$PDCA_TARGET` (= `/home/eddie/wyrd/wyrd.pdca-wt`, base 5d87cc4). Ratified items
(Option-B posture, exit-(b) seed labelling, the `last_heartbeat` oracle choice, the
toolchain-gated compile step) were **not** re-litigated; they were attacked only where the
iteration-13 code changes their standing.

## Refutations / findings

- NEEDS-HUMAN — **The entire recorded green sits on a stale base; the red→green was never
  demonstrated against the actual target.** Verified: `git apply --check patch.diff` on
  `origin/feat/m4-production-metadata-backend` @ 9cf4e8e **fails** (`error: patch failed:
  xtask/src/main.rs:82`) — the integration branch gained #448/#449/#450 (+194 lines in
  `xtask/src/main.rs`, +1213 in `Cargo.lock`) after the bundle's base 5d87cc4.
  `check-gates.json` records this honestly (`C4-verify: fail — "bundle is stale; rebase
  Do"`) yet carries `"overall": "pass"`. Every green in this bundle — the gating C4-ci
  pass, the pure-oracle tests, and (presumably) the live-leg evidence — was produced three
  merges behind the branch this patch must land on. The C4-ci `pass` row therefore
  certifies a tree that no longer exists; treating `overall: pass` as a merge verdict is
  the unwarranted claim. A rebased Do + re-run gates are required before any adequacy
  judgment is meaningful.

- NEEDS-HUMAN — **The binding behavioural evidence (Success criterion (d), the four-leg
  mutation acceptance run) is invisible to this review and absent from the target.**
  `results/issue_257/evidence/` does not exist at `$PDCA_TARGET` and `patch.diff` carries
  no evidence artifact; the brief's iteration-12 amendment makes those captured logs THE
  behavioural red→green Option B defers to ("RUN and CAPTURED, not argued";
  brief.md:124-133, 246-255). A human must verify, from the bundle's evidence dir: (1) the
  real-cut leg GREEN with `fault_materialized = true`; (2) the no-op negative control RED
  with `fault_materialized = false`; (3) the scratch mutation deleting the `get_for_update`
  re-check (`crates/metadata-tikv/src/lib.rs:555-573`) flipping the contention leg RED via
  an **observed lost update** (`committed_contenders == 2` and/or the stale probe
  admitted — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:235-276`), not via a
  connection error or panic; (4) restored GREEN. Without leg (3)'s log the slice has, once
  again, zero executed behavioural red→green anywhere — the exact iteration-12 rejection.

- **Leader-identity is never verified at cut time — the outcome-neutral follower cut the
  brief forbids can pass GREEN as "leader isolation".**
  `resolve_leader_ip` (`tier1_metadata_consistency.rs:779`) resolves the leader of the
  **first region PD lists, cluster-wide** (`wyrd_testkit::parse_first_region_leader_store_id`,
  `crates/testkit/src/lib.rs:532`), once, **before** the cut (`:455-471`), and nothing ever
  re-checks that the cut store (a) still leads anything when the rules land, or (b) leads
  the region holding the test keys (`wyrd-tier1-consistency/<pid>/dir/version`). Concrete
  failing case: PD's balancer moves/splits leadership between resolve and apply (or the
  first listed region is a system region led by a different store than the test-key
  region) → a **follower** is cut → its heartbeat still goes stale →
  `fault_materialized = true` → all four signals green — the precise "minority-voter cut
  provably never changes the outcome" hollow flip brief.md:112-118 forbids, now passing
  silently under a `WYRD_TIER1_ISOLATE=leader` label. Parser edge worsens it: a leaderless
  first region emitting `"leader":{}` makes `parse_first_region_leader_store_id` read the
  next `"store_id"` in the byte stream — an arbitrary peer of a *different* region (the
  unit test at `crates/testkit/src/lib.rs:915-925` covers only a fully absent `leader`
  key). Fix shape: assert post-cut (from PD) that the isolated store WAS the test-key
  region's leader at cut time, or fail the leg as not-materialized.

- **No asserted commit is ever in flight during the leader election — the docstring's
  teeth claim is overstated.** The scenario blocks in `pd_still_sees_target_live_after`
  (`tier1_metadata_consistency.rs:195`, up to 45s; staleness threshold 20s, `:506`) until
  the heartbeat is provably stale, and only then issues the rename (`:210`) and releases
  the contenders (`:235`). TiKV re-elects in ~10s, so by construction every asserted
  commit runs against a settled 2/3-quorum cluster; the claim "forcing a leader election
  while the contenders' commits are in flight" (`:32-33`) is unwarranted. The mutation
  flip (leg 3) fires identically with **no partition at all** — the partition contributes
  only the materialization signal, not perturbation of the asserted commit path. Not a
  false-green by itself, but the leg proves less than its docstring and the brief's
  "contending … across the fault window" framing suggest.

- **The fault-effect oracle can classify a NO-OP cut as materialized on a single failed
  poll.** `pd_sees_target_live` maps *PD unreachable / HTTP timeout / parse failure* to
  `false` (`tier1_metadata_consistency.rs:647-656`, `None => false` at `:654`), and
  `pd_still_sees_target_live_after` (`:661-670`) returns "isolated" on the **first** poll
  that comes back `false`. Concrete failing case: during a no-op run (rules never matched),
  one 3s connect timeout or a momentary PD hiccup inside the 45s window yields
  `connected_during = false` → `partition_took_effect(true, false) = true` →
  `fault_materialized = true` for a fault that never existed — the oracle whose whole
  Invariant-B job is "red when the fault is a no-op" goes green on infrastructure noise.
  Requiring N consecutive successfully-parsed stale reads (distinguishing "PD said stale"
  from "no answer") closes it. This also taints the no-op negative-control evidence (leg 2)
  as a single-run artifact.

- **The iter-10 guard gap moved up one level rather than closing.**
  `ci_type_checks_feature_gated_metadata_scenario` (`xtask/src/main.rs:1128`) drives
  `run_ci_steps` directly; nothing asserts that `run_ci` (`xtask/src/main.rs:897-898`)
  still calls `run_ci_steps`. Deleting/inlining that one call re-creates the iter-9 hole
  (feature-gated bodies never compiled by the real gate) with the guard test still green —
  the comment "the sole cargo-step source run_ci iterates" (`:1121`) is asserted nowhere.
  Materially better than the iter-10 constant-restating tautology (the loop and the
  toolchain gate now live inside the tested function), but the specific sign-off wording
  "assert run_ci invokes the feature-gated check" is satisfied only one hop removed.

## Attempted to refute; could not

- **Contention teeth (in-code):** traced `TikvMetadataStore::commit`
  (`crates/metadata-tikv/src/lib.rs:540-601`): with the `get_for_update` precondition loop
  deleted, both pessimistic contenders eagerly lock, wait, and blind-write → **two**
  `Committed` → `no_lost_update(2, _) = false`; the stale probe (`require` on a
  two-bumps-stale version, own key only) commits → red independently. The mutation flip is
  real in principle — its *executed* proof is the NEEDS-HUMAN evidence item above.
- **Pure oracles:** ran `cargo test -p wyrd-testkit --lib` and `cargo test -p xtask` on the
  patched target — green; the quorum/heartbeat/heal/no-lost-update tables use hand-computed
  expectations, and negating any single `ConsistencySignals` field fails
  `consistency_passes` (`crates/testkit/src/lib.rs` tests). No tautology found.
- **Compile-flip / dead-code:** ran `cargo check -p wyrd-metadata-tikv --features tikv
  --tests` on the patched target — green in 8.4s (tikv-client 0.4.0 builds), so the
  feature-gated scenario bodies type-check and the iter-11 "confirm the toolchain-gated
  check actually compiles them" item is satisfiable; `partition_took_effect` /
  `heal_is_complete` / `heartbeat_is_fresh` are consumed by the live scenario, not dead.
- **Invariants:** `crates/metadata-tikv/src` and `crates/traits` untouched by the diff;
  `wyrd-testkit` lands under `[dev-dependencies]` (`crates/metadata-tikv/Cargo.toml:46`);
  the DST seed's docstring claims no correctness weight (exit (b), ratified) and I found no
  smuggled teeth claim in it.
