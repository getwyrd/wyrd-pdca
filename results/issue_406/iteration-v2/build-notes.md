# Build notes — issue 406 (elle-register-listappend-models-and-workload-recorder), iteration 2

Target: `getwyrd/wyrd @ feat/m4-production-metadata-backend`, worktree HEAD `a7c7408`
(the branch tip — same tip v1 was built against). Net-new subsystem implementing accepted
**ADR-0041** (§Decision 1/2/3) — the mutable-metadata-register consistency-checker substrate
for #329 slice 3. Net-new functionality (principle 1.3): the minimalism maxim does not
govern; there is no invariant to restore.

## What changed vs iteration 1 (addressing the carry-forward)

The v1 sign-off **rejected** the register model because it FALSE-ACCEPTED a read of a value
only a definitely-FAILED write ever attempted. This rebuild fixes that root cause and
addresses every adversary improvement remark. Concretely:

1. **FIX (the rejection) — value provenance gated on committed writes only.**
   `crates/testkit/src/consistency.rs:328-336` (`check_register`, the `written` provenance
   loop; gate at `:330`) now requires `c.f == RegF::Write && c.ok` — the same committed-only
   filter Pass 1 (`:345`) and the namespace model (`:732` `if !c.ok { continue }`) already
   apply. A value that only a `Fail` write attempted (a lost CAS — "definitely did not take
   effect", `RegOp::write_fail` doc `:161-162`) is no longer admitted, so a read of it is now
   a `TornRead`. New flippable red pinning it:
   `register_rejects_a_read_of_a_failed_writes_value`
   (`crates/testkit/tests/consistency_models.rs:109`) — the exact case the adversary
   demonstrated (`[write_ok k=10@v1; write_fail k=99; read_ok k=99@v2]` → previously `Ok`,
   now `TornRead`).

2. **Exactly-one-writer-wins now catches same-value double-commits.**
   `consistency.rs:337-361` (Pass 1) counts *committed-write occurrences* per `(key, version)`
   (a `Vec<u64>` pushed at `:348`, flag when `len > 1`) instead of *distinct values* (the old
   `BTreeSet` that missed two identical-value winners). New flippable red:
   `register_rejects_two_winners_with_identical_values` (test `:162`). The distinct-value
   test `register_rejects_two_winners_at_one_commit_point` (`:143`) still passes.

3. **Rename is now recordable and covered (branch no longer dead).** `HistoryRecorder` gained
   `rename_invoke` / `rename_ok` (`consistency.rs:950,956`); the `NsOp` ctors are at `:599,607`.
   Three new crafted tests exercise the rename branch of `check_list_append` from both sides
   plus the recorder+serialization: `list_append_accepts_a_valid_rename` (`:231`),
   `list_append_rejects_a_rename_that_loses_its_destination` (the rename **add** branch,
   `:251`), `list_append_rejects_a_rename_source_resurrection` (the rename **remove** branch,
   `:270`), and `recorder_records_and_checks_and_serializes_a_rename` (drives the real recorder
   API + `namespace_to_edn`, `:289`). The in-process gateway has **no atomic rename** (see the
   scope note below), so the workload does not drive rename; it is exercised here by crafted
   histories, faithful to ADR-0041 §Decision 2 which puts rename in the model.

4. **Session-check "live teeth" claim scoped honestly.** The overstated comment is gone; the
   uncontended per-process key's role is now stated accurately (`consistency_models.rs` doc on
   `run_process`, `:485-489`): its single-writer own-writes give the RYW floor / monotonic
   last-version comparisons real produced data to run against, and a correct gateway passes —
   which is what the produced-history assertions confirm. The reject *logic* is bound by the
   crafted histories (a), (c). I did not manufacture a violation into a correct gateway's
   history.

5. **Recorded red is a model-WEAKENING red, not a compile-error red** — demonstrated below,
   in the worktree, per-clause.

Everything else keeps v1's structure (which passed the reviewer's C1/C3/T1–T4): pure models
in `crates/testkit/src/consistency.rs`, gateway-driving in the dev-only test.

## Files

- `crates/testkit/src/consistency.rs` (new, ~1080 lines) — pure Rust: `check_register`,
  `check_list_append`, `check_read_your_writes`, `check_monotonic_reads`, `HistoryRecorder`,
  `register_to_edn` / `namespace_to_edn`, the concurrency witnesses
  (`max_register_concurrency`, `max_observed_version`), and `verdict_dispatch` (the off-Check
  seam). No async, no server, no redb, no JVM.
- `crates/testkit/src/lib.rs:22` — `pub mod consistency;` (one decl, doc `:18-21`).
- `crates/testkit/Cargo.toml:20-40` — **dev-only** deps (server/core/redb/fs/mem/gateway-core
  + async-trait/bytes/tempfile/tokio); the library ships **zero** new normal deps
  (`cargo machete crates/testkit` clean).
- `crates/testkit/tests/consistency_models.rs` (new, the brief's named test file) — 18 tests:
  the flippable red→green crafted assertions (a)/(b)/(c) and (d) the workload driving the
  in-process `Gateway`.
- `Cargo.lock` — the dev-deps recorded.

## Forced refutation — the three questions (answers recorded, all "yes")

**(a) Genuine red — and each is a MODEL-WEAKENING red on real inputs, not a missing symbol.**
Done per-clause in the worktree; each weakening was reverted after:

- Un-gate value provenance (drop `&& c.ok` in the `written` loop): →
  `register_rejects_a_read_of_a_failed_writes_value` **FAILED** ("a read of a failed write's
  value must be rejected: ()" — `expect_err` got `Ok`). Behavioral. *This is the exact fault
  the v1 sign-off rejected, now covered.*
- Collapse two-winners to distinct-value semantics (`if !vals.contains(&val) { push }`): →
  `register_rejects_two_winners_with_identical_values` **FAILED**, while
  `register_rejects_two_winners_at_one_commit_point` stayed green.
- Drop only the rename **add** (keep remove): → `list_append_rejects_a_rename_that_loses_its_destination`
  **FAILED**, source-resurrection stayed green.
- Drop only the rename **remove** (keep add): → `list_append_rejects_a_rename_source_resurrection`
  **FAILED**, destination-lost stayed green.
- Accept-all `check_register` (`if !done.is_empty() { return Ok(()) }`): → all 5 register
  `*_rejects_*` FAILED; the 13 accept/plumbing/workload tests stayed green.
- Accept-all `check_list_append` + both session checks: → the 4 list-append `*_rejects_*` and
  the 2 `session_rejects_*` FAILED; the 12 others stayed green.

  All weakenings compile (I referenced the guarded vars to avoid a `-D warnings` dead-code
  *compile* error masquerading as the red), so every red above is the model returning the
  wrong verdict on a real history — the brief's §Falsifiability bar. Reverting the *whole*
  patch is a compile error (undefined `check_register`, …) and does NOT count; that is why
  the reds are captured by weakening, not by removal.

**(b) Production path.** The workload
(`workload_against_the_in_process_gateway_yields_a_nonvacuous_checkable_history`) drives the
**production** `wyrd_server::Gateway` (`put_object` `crates/server/src/lib.rs:148`; the
`ObjectGateway::delete_object` impl `:171`) over the real `wyrd_core::write` commit path, and
reads back through `wyrd_core::read::{resolve,read_inode}` (`crates/core/src/read.rs:29,44`) —
the same in-process gateway `crates/server/tests/closed_write_path.rs:224-240` drives. The
models under test are the shipped deliverable (pure functions), exercised directly. No mock,
copy, or re-implementation.

**(c) Fixture includes the fault.** Each crafted history *contains* the anomaly it asserts on
(the failed-write value read, the torn value, the regressing read, the duplicate-version
writes with identical AND distinct values, the omitted create, the resurrected name, the
lost/resurrected rename, the backwards `meta:version` read). The workload fixture *includes*
its claimed non-vacuity: a `tokio::sync::Barrier` forces every process to record its first
write-invoke before any completes (`max_register_concurrency >= 2`, asserted), and
retry-until-committed overwrites of the hot key bump the commit point past `commit_create`'s
version 1 (`commit_overwrite` = `prior.version + 1`, `crates/core/src/metadata.rs:432,471`;
`max_observed_version >= 2`, asserted). Ran the workload leg repeatedly (v1 recorded 40×); the
barrier + retry make both witnesses structural, not probabilistic.

## Scope note for the human (sign-off) — the in-process gateway has no atomic rename

The brief's *Scope* says the workload drives "create/delete/**rename** against the in-process
gateway". The in-process gateway's object surface (`wyrd_gateway_core::ObjectGateway`,
`crates/gateway-core/src/lib.rs:108-134`) exposes **put/get/delete only — there is no atomic
rename op** (a rename would be a single dirent mutation, `commit`-level, not part of the
object gateway API this slice consumes; "any change to the gateway or `MetadataStore`" is
out of scope). So the workload drives create/delete/list, and the **rename model branch**
(mandated by ADR-0041 §Decision 2: "create appends … delete removes … rename moves it") is
built, recordable (`HistoryRecorder::rename_*`), serialized (`[:rename …]` EDN), and
**crafted-tested from both sides** here — ready for the wire-driven driver (#405) or a future
gateway rename. I did **not** fake a rename by recording a non-atomic delete+create as one
`rename` op (that would misrepresent atomicity the gateway didn't provide). This is a faithful
reading of the authoritative plan (ADR-0041), with the one honest gap — no in-process atomic
rename to drive — surfaced here rather than papered over. Confirm this at sign-off.

## Verification posture / deferred leg (pre-declared, per brief)

The **Check-exercised core** — the two models, the session checks, the recorder, the
serialization, and the non-vacuous in-process history production — is fully built and green.
The **live Elle/JVM verdict** over the serialized EDN is the brief's pre-declared
**DEFERRED / off-Check** leg (ADR-0016/ADR-0041 keep JVM/Clojure out of `cargo xtask ci`) and,
per *External dependencies*, is explicitly **not a build/verify dependency of this bundle** —
so no JVM/Clojure was pulled in and there is **no NEEDS-HUMAN external dependency**.
`verdict_dispatch` encodes the routing as a pure, unit-checked value (mirroring
`xtask/src/metadata_faults.rs:39-60` `metadata_tier_dispatch`); the off-Check job runs the
same history the Check-exercised recorder produces.

## Commit-readiness (target's own hooks; run in the worktree)

- `cargo fmt --all -- --check` — clean (both new files formatted).
- `cargo clippy -p wyrd-testkit --all-targets` — clean (workspace lints = `-D warnings`).
- `cargo machete crates/testkit` — no unused dependencies.
- `cargo test -p wyrd-testkit --test consistency_models` — **18/18 green**;
  `cargo test -p wyrd-testkit --lib` — 23/23 green.
- `cargo check -p wyrd-server --tests` — clean (confirms the **dev-only** `wyrd-testkit ↔
  wyrd-server` dependency cycle resolves; Cargo permits dev-only cycles).

Runner note: the project's gate runner (`./engine/xtask.sh ci` → `cargo xtask ci`) exposes
only the whole-tree gate — there is no narrow-test wrapper — and per v1's Check the full
`cargo xtask ci` fails in this sandbox on an **unrelated** loopback-bind test
(`crates/chunkstore-grpc/tests/list_delete.rs`), which would obscure this bundle's red→green
signal. So the fast red→green sanity pass was run through `cargo test -p wyrd-testkit --test
consistency_models` under an explicit tool timeout (the test is self-contained — in-memory
redb + fs temp dir + no network, bounded retry loops — so no hang risk). The gating full
`cargo xtask ci` is Check's step and re-runs the real suite.

## Citations (path:line on feat/m4-production-metadata-backend)

- ADR-0041 `docs/design/adr/0041-consistency-checker-substrate.md` — §Decision 1 (register),
  §Decision 2 (list-append incl. rename), §Decision 3 (sessions), and the JVM-off-Check
  constraint (§Decision closing para + §Consequences).
- ADR-0015 `docs/design/adr/0015-consistency-contract.md:22-25` — the three guarantees.
- Commit point / version bump: `crates/core/src/write.rs:271` `commit_overwrite`,
  `crates/core/src/metadata.rs:243` `InodeRecord.version`, `:253` (`commit_create` version 1),
  `:432`/`:471` (`prior.version + 1`), `:27` `VERSION_KEY`.
- In-process gateway driving (peer): `crates/server/tests/closed_write_path.rs:224-240`
  (`Gateway::new` + `put_object` + `read::resolve`/`read_inode` over a shared store).
- Gateway API consumed: `crates/server/src/lib.rs:81` `Gateway::new`, `:148` `put_object`,
  `:171` `delete_object`, `:40` `ROOT`; `crates/gateway-core/src/lib.rs:75-98`
  `GatewayError::Conflict`, `:108-134` `ObjectGateway` (put/get/delete — no rename).
- MetadataStore seam: `crates/traits/src/lib.rs:338-350` (`get`/`scan`/`commit`).
- "Deferred ≠ unbuilt" seam mirrored: `xtask/src/metadata_faults.rs:39-60`.
- Testkit oracles peer style: `crates/testkit/src/lib.rs:441` `consistency_passes`,
  `:394` `partition_materialized`.
