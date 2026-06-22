# Build notes — issue 145 / m3.7-rebalance-drain-decommission (iteration 2)

Target: `getwyrd/wyrd @ main` (worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt`).
Planning artifact: `docs/design/proposals/accepted/0005-milestone-3-custodians.md` (slice 7,
`0005:537-540`), read as authoritative.

## Ordering gate (#144) confirmed on the worktree base

`git log --oneline` on the worktree base shows #144 merged before any of my work:

```
41c8165 Merge pull request #190 from getwyrd/fix/144-reconstruction-custodian
5fb905c custodian: reconstruct under-replicated chunks from the repair queue
```

So `select_distinct_domains_excluding` (`core/placement.rs:265`), `Topology::domain_of`
(`placement.rs:118`) and `gc::orphan_key` (`gc.rs:45`) — the commit-point-atomic re-place
machinery the rebalance move reuses verbatim (`0005:298-299`) — are present. The reviewer's
"confirm #144 is on main before sign-off" carry-forward note holds.

## What this iteration changes vs. iteration 1

The reviewer **accepted the design and approach** ("Design and approach are sound"). The only
gap was test coverage of **two reachable branches the v1 fixtures never constructed**. Per the
carry-forward I left **all production source byte-for-byte identical to v1** (placement.rs,
desired_state.rs, rebalance.rs, reconciliation.rs, lib.rs — same diff) and **added two tests**
to `crates/custodian/tests/rebalance.rs`. No rejected approach was re-attempted; the change is
purely additive test coverage on the accepted source.

I verified the source is unchanged by re-applying the v1 patch to a clean worktree base and only
editing the test file (plus the v1-identical source the patch carries).

### Test 1 — multi-fragment evacuation in ONE commit (`evac.len() > 1`)

`evacuates_two_drained_servers_of_one_chunk_in_a_single_commit`
(`crates/custodian/tests/rebalance.rs:526`). v1 used RS(2,1) one-fragment-per-server and
drained a single server, so `EvacPlan::evac` was always length 0 or 1 — the multi-fragment
path of `evacuate_chunk` (`rebalance.rs:223` loop + `select_distinct_domains_excluding(count=2)`,
`rebalance.rs:210`) was never exercised. The new test:

- five-domain topology A..E (`five_domains()`, helper added at `rebalance.rs:239`); RS(2,1)
  placed on servers 0,1,2 (A,B,C) with D,E spare;
- marks **both** server 0 (draining) and server 1 (decommissioning) — two fragments of the
  **same** chunk, so `evac == [0, 1]`;
- asserts the inode version bumps by **exactly one** (one atomic commit moves both fragments,
  not two commits), placement becomes `[3, 4, 2]`, the chunk still spans **3 distinct domains**
  (closes the C5 claim that `select_distinct_domains_excluding` preserves `n` for `count = 2`),
  both drains end `Satisfied`, both copies verify their checksums, **two** orphan records are
  written, and the object still reads back its original bytes.

### Test 2 — lost-CAS: a racing writer loses rather than corrupts (`EvacOutcome::Conflict`)

`a_racing_writer_loses_the_version_conditional_commit_and_leaves_only_garbage`
(`rebalance.rs:737`). This is **not** a fixture-size issue (the carry-forward is explicit) — it
needs a concurrency seam. I added a `RacingMeta` `MetadataStore` wrapper (`rebalance.rs:664`)
that injects **one** concurrent inode mutation the first time it sees an inode-conditional
commit *after* the test arms it (`arm()`), modelling a writer that supersedes the inode between
the loop's read (`plan_evacuations`) and its commit. The racer bumps the inode version with
placement **unchanged**, so the custodian's `.require(prior)` precondition (`rebalance.rs:262`)
misses and `MetadataStore::commit` returns `CommitOutcome::Conflict` → `EvacOutcome::Conflict`
→ `emit_conflict` (`rebalance.rs:319`). The test asserts the headline safety claim:

- the placement record reflects the **racing writer** (version 2, placement `[0,1,2]`), **not**
  the custodian's repoint (which would be `[0,3,2]`) — the custodian *lost*, not corrupted;
- the pre-commit copy on server 3 is present but is **collectable garbage** — and crucially the
  commit was **atomic**: **no** `orphan:` record was written (the orphan puts were in the same
  rejected batch), so there is no torn / hybrid move;
- the drain stays `Pending` (re-assessed next pass);
- the conflict is observable on the durability seam (`rebalance_conflict` in the Prometheus
  read-back), exercising `emit_conflict`.

## Demonstrated assertion-level red (each new test pins a real branch, not non-existence)

Both new tests pass on the accepted source. To prove each is **load-bearing** on the branch the
reviewer named (and not merely resting green), I temporarily flipped the source, ran the single
test, observed red, and reverted (source is back to the accepted v1 bytes — confirmed by the
full 5/5 green re-run and the clean gate):

- **Test 1 flip** — `take(1)` on the `evac` loop (`rebalance.rs:223`) so only the first of two
  drained fragments moves → `evacuates_two_…` FAILED at `rebalance.rs:587`:
  `left: [3, 1, 2], right: [3, 4, 2]` ("fragment 1 → server 4"). Proves the test pins the
  multi-fragment (both-in-one-commit) handling.
- **Test 2 flip** — dropped the `.require(inode_key, encode(prior))` precondition
  (`rebalance.rs:262`) so the move is no longer version-conditional → `a_racing_writer_…`
  FAILED at `rebalance.rs:800`: `left: Changed, right: Satisfied` (the custodian wins instead of
  losing the CAS, and would then have corrupted the racing writer's record). Proves the test
  pins the lost-CAS safety claim.

(The v1 load-bearing red for the central evacuation leg — temporary `draining.clear()` flipping
`drains_a_d_server_…` to `left: Satisfied, right: Changed` — still holds; unchanged this round.)

## Red→green + whole-gate evidence

- `cargo test -p wyrd-custodian --test rebalance` → **5/5 pass** (the three v1 tests + the two
  new ones); each flip above goes red and reverts to green.
- Commit-readiness: `cargo fmt --check -p wyrd-custodian -p wyrd-core` clean; `cargo clippy
  -p wyrd-custodian --all-targets` clean (`-D warnings`, the target's own pre-commit gates).
- Whole gate (the brief's binding confirmation): `./engine/xtask.sh ci` in `$PDCA_WORKTREE`
  → **`xtask ci: all checks passed`** (fmt --check, clippy -D warnings, build, full test suite
  incl. the DST network/concurrency sweep, cargo-deny, conformance).

## Scope / posture unchanged from v1 (still honoured)

- Production source is identical to the accepted v1 patch; the only net change is two added
  tests + two test helpers (`five_domains`, `RacingMeta`) and the module-doc legs 5–6.
- Out of scope, untouched: hot-spot rebalance (`0005:301-302`), the API-first management surface
  + CLI (ADR-0013, deferred `0005:355-356`), multi-zone placement, dashboards. No on-disk-format
  change; desired state remains a metadata-ledger entry (ADR-0010 boundary kept).
- Option-A in-process posture (`0005:519-523`): the operator API/CLI deferral is an accepted
  scope boundary (reviewer's V/fitness note), not a defect.
