# Build notes — issue 408 (m4-checked-consistency-run-elle-report, replan v3)

## What this cycle's delta was (vs the reusable v2 patch)

The v2 pipeline was reviewed sound in shape but the real elle-cli 0.1.9 **rejected both
histories** — the #406 serializers emitted a vocabulary the checker does not accept. This
brief redraws the boundary to fix the **format contract at its source**. The delta I built:

1. **Register serializer → `rw-register` txn micro-op form** (`crates/server/src/consistency_workload.rs`):
   `MultiProcessHistory::to_elle_edn` now emits `{:process P, :type T, :f :txn, :value [[:w key v]] | [[:r key v]], :time N}` — the key lives **inside** the micro-op (no `:key`
   field). Replaced `register_f`/`register_invoke_value`/`register_completion_value` with
   `register_write_microop`/`register_read_microop`.
2. **Directory serializer → Elle `set` model**: replaced the old `:add`/`:remove`/`:contains`
   probe form with `DirCreate` (integer `:add`) + one composed post-heal `DirFinalRead`
   (`:read #{ints}`). Removed `DirOpKind`/`DirRecord`/`dir_*` serializer helpers; kept
   `membership`/`Membership` (used to compose the final read, INV-1-soundly).
3. **xtask module** (`xtask/src/consistency_run.rs`): `MODEL_DIRECTORY_SET = "set"` (was
   `set-full`); `CheckOutcome` is now **three-valued** (`Pass`/`Violation`/`Inconclusive`);
   `parse_checker_output` reads the trailing **token** of the `<file>\t<token>` line (not the
   whole line, not the exit code alone) so `:unknown`-at-exit-0 is inconclusive; `RunSummary`
   gained per-pool `OutcomeCounts`; `edn_history_has_expected_vocabulary` pins the txn shape.
4. **Real fixtures** (`xtask/tests/fixtures/consistency-run/`): every EDN + captured checker
   output was produced by running the real jar
   (`elle-cli-0.1.9-standalone.jar`, java 25) on this host on 2026-07-16. Verified:
   register-good→`true`, register-bad(read-reverts-to-nil)→`false`, directory-good→`true`,
   directory-bad(missing element)→`false`, and the `:contains` trap→`:unknown` (exit 0).
5. **Runner + live scenario** reshaped for the new pools (register overwrite-only Elle pool;
   directory create-only + post-heal composed read); the fixtures self-check now covers **both
   models, both polarities** (v2 covered only register). The scenario type-checks under
   `--features fdb` on this host (fdb toolchain present).

## Why the fix is at the source, not a translation layer (Invariant-to-restore reasoning)

The invariant to restore is **"the serialized history is checker-consumable"** (brief Success
criterion (a); Design "Alternatives", principles §1.2). A #408-owned export-time translator
that left the #406 serializers emitting the rejected scalar `:value` would keep dead-wrong
code alive with zero other consumers and split the format truth across two layers. That is a
symptom-guard, not a cause-removal. Concrete cost of the translator alternative: it would ADD
a whole new translation module (roughly the size of the two `to_elle_edn` bodies, ~120 lines)
**on top of** leaving the 2 wrong serializers + their 2 byte-exact golden tests asserting a
checker-rejected shape — i.e. more code AND a persistent lie in the test suite. Amending in
place is both smaller net and the only thing that actually restores the invariant. This is why
cost-vs-diff was not the deciding axis: the target is the smallest change that makes the
serialized history checker-consumable, which is the in-place amendment.

## Refute-your-own-test (forced answers)

**xtask orchestration test** (`xtask/tests/consistency_run_orchestration.rs`, 25 cases) and
**server serializer tests** (`crates/server/tests/consistency_workload.rs`, +byte-exact txn /
set golden + should_panic delete).

- **(a) Genuine red?** YES, demonstrated by mutation (not just revert):
  - Neutering `evaluate_summary` to `Ok(())` flipped `a_summary_missing_the_inv2_witness_is_inconclusive`
    and `a_summary_whose_typed_evidence_did_not_materialize_is_inconclusive` to **FAILED**;
    restoring returned 25/25 green.
  - Mutating `register_write_microop` away from `[[:w key v]]` broke the byte-exact txn golden
    test (the mutated build did not produce a passing run); restoring returned 13/13 green.
  - The canonical C4-verify red (revert production hunks, keep the added test) leaves
    `xtask/tests/consistency_run_orchestration.rs` unable to resolve `xtask::consistency_run::*`
    (the whole module is net-new) → red, exactly as the brief's Falsifiability predicts.
- **(b) Production path?** YES. The xtask test drives the real `xtask::consistency_run`
  functions the runner (`consistency_run_runner.rs`) calls; the server test drives the real
  `MultiProcessHistory::to_elle_edn` / `DirectoryHistory::to_elle_edn` the live scenario writes.
  No copy/mock/re-implementation. The seam is the run-summary JSON (the test imports no
  `wyrd-server` type — the test-graph constraint).
- **(c) Fixture includes the fault?** YES. The verdict fixtures are the **real checker's**
  outputs (`false` from a genuine read-reverts-to-nil violation; `:unknown` from the real
  rejected-vocabulary trap), not curated stand-ins. `self_check_matches(false, …)` demands a
  genuine `Violation`, so an `:unknown` can never masquerade as the caught known-bad (pinned by
  `the_self_check_confirms_… _parses_violation`).

## Environment verification done at Do (not asserted)

`java -version` → openjdk 25; jar present at the brief's path; `docker` 29.6.1; `libfdb_c.so`
loadable; `/usr/include/foundationdb/fdb_c.h` present. Every EDN vocabulary claim (§3) was
re-confirmed by feeding the exact shapes to the jar (see fixtures). The elle-cli/java/docker/fdb
doctor rows already exist in `pdca.toml` (`elle-cli`:718, `java`:708, `docker`:618, fdb rows
663–685) — no doctor change needed.

## NEEDS-HUMAN — the witnessed report (Design §6, pre-declared deferred live green)

The Check-core is fully green red→green. The **witnessed run report** (real `true` verdicts
from both models under a materialized partition, committed under `docs/design/reviews/`) is the
off-Check leg ADR-0041's own MUST keeps out of `cargo xtask ci`, and the brief §6 makes the
ready-mark the human's step regardless. I did **not** attempt the privileged live run from this
beat: `WYRD_TIER1=1 cargo xtask consistency-run` stands up the 3-node `deploy/fdb-multi-replica`
compose stack, builds the iptables agent (needs NET_ADMIN on the 172.30.58.0/24 net), cuts a
real partition, and shells four JVM elle-cli invocations — a multi-minute privileged operation
whose hang would stall the beat, and which §6 explicitly permits the maintainer to witness at
sign-off. Acceptance = Check-core green (done) + the witnessed report present at sign-off.

Reproduction for the maintainer (environment already preflights green on this host):

```
export WYRD_ELLE_CLI_JAR=/home/eddie/Downloads/elle-cli-bin-0.1.9/target/elle-cli-0.1.9-standalone.jar
WYRD_TIER1=1 cargo xtask consistency-run      # partition leg by default
# -> writes target/consistency-run/{register,directory}-history.edn, run-summary.json, report.md
# Commit target/consistency-run/report.md under docs/design/reviews/ once both models return true.
```

This surfaces a NEEDS-HUMAN item at sign-off for the maintainer to validate the live green —
exactly where an irreducibly-privileged check belongs.

## What I ruled out (brief alternatives, re-confirmed)

- `set-full` (v2 pin): the jar has no such acceptance for our shape; `set` + composed final
  read is what states what we observe. Left the old vocabulary behind entirely.
- Encoding register deletes as nil-writes: verified `false` on a correct history at Plan, so the
  register serializer **panics** on a delete (exclusion by construction, not per-op filtering) —
  pinned by `register_serializer_panics_on_a_delete_never_fabricating_a_representation`.
