# Build notes — issue 406 (elle-register-listappend-models-and-workload-recorder), iteration 3

Target: `getwyrd/wyrd @ feat/m4-production-metadata-backend`, worktree HEAD `a7c7408` (the
branch tip v1/v2 were also built against). Net-new subsystem implementing accepted **ADR-0041**
(§Decision 1/2/3) — the mutable-metadata-register consistency-checker substrate for #329 slice 3.
Net-new functionality (principle 1.3): the minimalism maxim does not govern; there is no invariant
to restore, so the target is the smallest change that makes the *end result* (a checker whose
value is that it detects inconsistency) actually hold on the contended path.

This rebuild keeps v2's structure (which cleared the reviewer's C1/C3 and the crafted-history
red→green for torn read / failed-write value / regression / two-winners / namespace / rename /
sessions) and fixes **only** what iteration 2 rejected.

## The rejection (iteration 2) and the root cause

> The register model still FALSE-ACCEPTS on the CONTENDED path … contended writes record
> `version=None`, and every load-bearing check is gated on `Some(version)`, so on exactly the
> contended ops the model's detection is switched off … `version=None` is the COMMON case in the
> very workload fed to `check_register`, so on criterion (d)'s contended ops "the register model
> passes" is near-guaranteed regardless of correctness.

Four holes were named: (1) stale read after a `version=None` committed overwrite (Pass-3 excludes
it); (2) a superseded value reads clean (provenance per-key, not per-`(key,version)`); (3) a
vanished committed value / lost write (absent read skipped); (4) two-winners not counted when both
writes are `version=None`. The carry-forward asked for **both** the preferred cause-removal
("capture the REAL commit version even under contention … so Pass-1/Pass-3 observe every committed
write") **or** the conservative alternative ("REJECT unresolvable `version=None` overwrites rather
than silently skip them"), plus "a crafted flippable red for each of the four cases".

## The fix — both prongs (cause-removal AND backstop), not one

I did **both** the reviewer's preferred fix and the endorsed alternative, because they close the
hole from opposite ends and together make the near-vacuous pass impossible:

### Prong 1 — capture the real commit version (the PREFERRED fix; removes the cause in the workload)

The workload no longer records `version=None` for any committed write. `versioned_put`
(`crates/testkit/tests/consistency_models.rs:559`) captures each writer's **exact** commit version
by observing the just-committed inode; for the *shared* HOT key it serializes commit+observe under
a per-key `tokio::sync::Mutex` (`consistency_models.rs:714`, threaded through `run_process` at
`:625,635,648`), so no other writer interposes between our commit and our read-back and `observe`
returns OUR OWN value → OUR OWN version. An uncontended per-process key passes `None` (single
writer, no interposition). The gateway PUT returns `()`, not the commit version, and changing the
gateway API is out of scope (brief *Scope*: this slice is a **consumer** of the existing gateway
API — `crates/server/src/lib.rs:148` `put_object` → `commit_written:168`), so the version is
captured at the **consumer**, which is exactly "thread the committed inode version back through
`put_with_retry`". The inode `version` genuinely climbs 1→2→3 across the multi-writer overwrites of
the same inode (`crates/core/src/metadata.rs:471` `prior.version + 1`), so the history is genuinely
contended AND every committed write carries its real version. A new assertion locks this in:
`committed_writes.iter().all(|o| o.version.is_some())` (`consistency_models.rs:794`). **Effect:**
the register model's version-keyed detections (Pass 1 two-winners, Pass 2 torn/superseded, Pass 3
regression) now run on the produced contended ops instead of being switched off — criterion (d)'s
"the register model passes" is a *non-vacuous* pass.

### Prong 2 — reject unresolvable + detect lost writes (the endorsed backstop, in the model)

Even with Prong 1, the *model* must not be foolable on `version=None`. Two additions to
`check_register` (`crates/testkit/src/consistency.rs:351`):

- **Pass 0 — `UnresolvedWrite`** (`consistency.rs:354-374`, kind at `:250`): a committed write
  (`RegF::Write && ok`) with `version == None` cannot be placed in the commit point's version
  total order (the inode `version` under the CAS **is** the linearization index, ADR-0041
  §Decision 1), so the history is unverifiable there and is **rejected** rather than silently
  skipped. This single rule closes holes 1, 2 and 4 (all involve a `version=None` committed write):
  a version-less committed overwrite can never again be a free pass for a stale read, a superseded
  value, or an uncounted double-commit.
- **Pass 2b — `LostWrite`** (`consistency.rs:462-495`, kind at `:254`): the register has no delete,
  so once a write commits the key is present for every later read. A read that finds the key
  **absent** after a committed write to it completed (in real time) before the read began is a
  vanished/lost write. **Version-independent** — it fires even when the write carried a version, so
  hole 3 (lost write) is closed on the contended path *and* elsewhere.

Why a backstop *and* the cause-removal, when the guidance warns against guarding a symptom while a
cheaper cause-removal exists: here the cause-removal (Prong 1) and the backstop (Prong 2) are not
substitutes. Prong 1 alone would make criterion (d) pass but leaves the *model* still foolable by
any future/wire-driven recorder that drops a version — the checker's whole value is that it can't
be fooled, so the model must self-defend (cost: +21 lines, `consistency.rs:354-374`,`462-495`).
Prong 2 alone (reject all `version=None`) would make the produced workload **fail** criterion (d)
unless the workload also captures versions — i.e. it *forces* Prong 1. So both are required; neither
is a cheaper stand-in for the other. The reviewer offered them as "preferred … alternatively"; the
correct end result needs both.

## The four crafted flippable reds (one per named hole)

Added to `consistency_models.rs` (all in the `(a′) contended path` block):

| Red | Hole | Guard it pins | Line |
|-----|------|---------------|------|
| `register_rejects_a_stale_read_after_an_unversioned_overwrite` | 1 stale read | Pass 0 | `:199` |
| `register_rejects_a_superseded_value_read_via_an_unversioned_overwrite` | 2 superseded value | Pass 0 | `:222` |
| `register_rejects_a_lost_write_absent_read` | 3 lost write | Pass 2b | `:245` |
| `register_rejects_two_unversioned_winners` | 4 two winners (both None) | Pass 0 | `:264` |

## Forced refutation — the three questions (all "yes", with concrete evidence)

**(a) Genuine red — and each is a MODEL-WEAKENING red on real inputs, not a missing symbol.**
Done per-guard in the worktree, reverting only the guard (module still compiles), then restored:

- Weaken **Pass 0** (delete the `UnresolvedWrite` loop, keep the rest): the three Pass-0 reds
  **FAILED** — `register_rejects_a_stale_read_after_an_unversioned_overwrite`,
  `…_superseded_value_read_via_an_unversioned_overwrite`, `…_two_unversioned_winners` — while
  **19 tests stayed green** (incl. the failed-write-value red, two-winners-Some, lost-write, and
  the workload). Module compiled (proof it is behavioural, not a symbol error).
  → `test result: FAILED. 19 passed; 3 failed`.
- Weaken **Pass 2b** (delete the `LostWrite` block, keep the rest): only
  `register_rejects_a_lost_write_absent_read` **FAILED**; **21 stayed green** (the Pass-0 reds
  stayed green — proof the reds target *distinct* guards). → `test result: FAILED. 21 passed; 1 failed`.

Reverting the *whole* patch is a compile error (undefined `check_register`, …) and does **not**
count — which is exactly why the reds are captured by weakening a single guard, per the
carry-forward's request to confirm "a model-WEAKENING red, not a compile-error red".

**(b) Production path.** The workload
(`workload_against_the_in_process_gateway_yields_a_nonvacuous_checkable_history`) drives the
**production** `wyrd_server::Gateway` (`put_object` `crates/server/src/lib.rs:148`, `delete_object`
via `ObjectGateway` `:315`) over the real `wyrd_core::write` commit path and reads back through
`wyrd_core::read::{resolve,read_inode}` (`crates/core/src/read.rs:29,44`) — the same in-process
gateway `crates/server/tests/closed_write_path.rs:224-240` drives (real redb + fs + mem
coordination, ADR-0010). The models under test are the shipped deliverable (pure functions in
`crates/testkit/src/consistency.rs`), exercised directly. No mock / copy / re-implementation.

**(c) Fixture includes the fault.** Each crafted red *contains* the anomaly it asserts on: reds
1/2/4 each embed a `version=None` **committed** overwrite (`RegOp::write_ok(…, None)`) in an
otherwise-inconsistent history; red 3 embeds a `read_absent` after a committed `write_ok`. The
workload fixture *includes* its claimed non-vacuity: a `tokio::sync::Barrier` forces every process
to record its first write-invoke before any completes (`max_register_concurrency >= 2`, asserted),
retry-until-committed overwrites bump the hot inode past `commit_create`'s version 1
(`max_observed_version >= 2` + `version >= 2`, asserted), AND every committed write carries a real
version (`all(version.is_some())`, `consistency_models.rs:794`). Ran the workload leg **25×** —
25/25 green, deterministic (barrier + per-key serialization make the witnesses structural, not
probabilistic).

## Why the produced contended history is still linearizable (so Prong 1 doesn't false-red the workload)

Under the HOT-key mutex, versions are attributed exactly (1,2,3,…), each version produced by one
writer (no two-winners), and reads observe the current monotone version. The recorder mutex makes
the recorded `[t_invoke,t_complete]` interval *bracket* each op's real observe point, so Pass 3's
real-time reasoning (`a.t_complete < b.t_invoke ⇒ va ≤ vb`) holds against a monotone store. Session
RYW/monotonic now run on **real captured versions** for the contended key too (a strengthening),
and a correct gateway passes them.

## Scope carried forward unchanged (not the reason for rejection — do not relitigate)

- The rename model branch remains built + recordable (`HistoryRecorder::rename_*`) + serialized +
  crafted-tested from both sides; the in-process `ObjectGateway` (`crates/gateway-core/src/lib.rs:108`)
  has put/get/delete but **no atomic rename**, so the workload drives create/delete/list and rename
  is exercised by crafted histories (faithful to ADR-0041 §Decision 2). Unchanged from v2.
- The **live Elle/JVM verdict** is the pre-declared **DEFERRED / off-Check** leg (ADR-0016/ADR-0041
  keep JVM/Clojure out of `cargo xtask ci`); per *External dependencies* it is **not** a build/verify
  dependency of this bundle, so no JVM/Clojure was pulled in and there is **no NEEDS-HUMAN external
  dependency**. `verdict_dispatch` encodes the routing as a pure unit-checked value (mirroring
  `xtask/src/metadata_faults.rs:39-60`). This split was accepted at iteration 2 — not relitigated.

## Verification posture (pre-declared, per brief)

DEFERRED / net-new — a pre-declared sign-off item, not a surprise NEEDS-HUMAN. The Check-exercised
core (two models + session checks + recorder + serialization + non-vacuous in-process history) is
fully built and green; the live recognized-checker verdict over the SAME serialized history is the
off-Check leg confirmed by the maintainer/nightly job. This slice is not mere dispatch plumbing —
the model/checker logic is functionally implemented, and the iteration-2 contended-path fault is now
a genuine, flippable regression.

## Commit-readiness (target's own hooks; run in `$PDCA_WORKTREE`, patch re-applied on a clean tree)

- `cargo test -p wyrd-testkit --test consistency_models` — **22/22 green** (18 prior + 4 new reds);
  the workload leg 25/25 across reruns.
- `cargo test -p wyrd-testkit --lib` — 23/23 green.
- `cargo fmt -p wyrd-testkit -- --check` — clean.
- `cargo clippy -p wyrd-testkit --all-targets` — clean (`-D warnings`).
- `cargo machete crates/testkit` — no unused dependencies.
- `cargo check -p wyrd-server --tests` — clean (the dev-only `wyrd-testkit ↔ wyrd-server` cycle
  resolves; Cargo permits dev-only cycles).
- `patch.diff` was regenerated against `HEAD` (`a7c7408`), then re-applied on a freshly-reset clean
  tree and re-run green — it is self-contained and commit-ready.

Runner note: the project's gate runner (`./engine/xtask.sh ci` → `cargo xtask ci`) exposes only the
whole-tree gate; per v2's Check the full `cargo xtask ci` fails in this sandbox on an **unrelated**
loopback-bind test (`crates/chunkstore-grpc/tests/list_delete.rs`), which would mask this bundle's
red→green signal. So the fast red→green sanity was run through `cargo test -p wyrd-testkit --test
consistency_models` under the tool's bounded timeout (self-contained: in-memory redb + fs temp +
bounded retry loops → no hang risk). The gating full `cargo xtask ci` is Check's step and re-runs
the real suite.

## Citations (path:line on feat/m4-production-metadata-backend, post-patch)

- ADR-0041 `docs/design/adr/0041-consistency-checker-substrate.md` — §Decision 1 (register: inode
  `version` under commit CAS is the linearization index), §Decision 2 (list-append incl. rename),
  §Decision 3 (sessions), JVM-off-Check constraint (§Decision closing + §Consequences).
- ADR-0015 `docs/design/adr/0015-consistency-contract.md:22-25` — the three guarantees.
- Commit point / version bump: `crates/core/src/write.rs:271` `commit_overwrite`,
  `crates/core/src/metadata.rs:471` (`prior.version + 1`), `:253` (`commit_create` version 1),
  `:243` `InodeRecord.version`.
- New model guards: `crates/testkit/src/consistency.rs:250` `UnresolvedWrite`, `:254` `LostWrite`,
  `:354-374` Pass 0, `:462-495` Pass 2b.
- Version-capturing workload: `crates/testkit/tests/consistency_models.rs:559` `versioned_put`,
  `:620-655` `run_process` (hot-key lock threaded), `:714` `hot_lock`, `:794` capture assertion,
  `:199/:222/:245/:264` the four crafted reds.
- In-process gateway driving (peer): `crates/server/tests/closed_write_path.rs:224-240`.
- Gateway API consumed: `crates/server/src/lib.rs:148` `put_object`, `:315` `delete_object`,
  `crates/gateway-core/src/lib.rs:108` `ObjectGateway`, `:75` `GatewayError::Conflict`.
- "Deferred ≠ unbuilt" seam mirrored: `xtask/src/metadata_faults.rs:39-60`.
