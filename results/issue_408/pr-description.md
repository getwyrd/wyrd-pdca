# PR description

## Summary
**User impact:** Wyrd claims it doesn't lose data when the network fails — but until now
the only evidence was Wyrd's own tests, checked by Wyrd's own code. Anyone deciding
whether to trust the store with their data had nothing an outsider could independently
inspect: no run under a real fault, judged by a checker the wider community already
recognizes.

This PR adds that artifact: an opt-in command that drives real register and directory
workloads against a live 3-node FoundationDB cluster while a real network partition bites,
then has Elle (the Jepsen ecosystem's consistency checker, via elle-cli) judge the
recorded histories. The first witnessed run is committed with the PR: **Elle returned
`true` for both models** over a genuinely concurrent history recorded under a materialized
partition.

Implements #408 (slice 5 of #329, composing on the #407 nemesis seam; sequenced by
ADR-0041).

## What to look at
The run is built to refuse to overstate itself, and that refusal is the crux:

- it fails as **inconclusive** unless the history is provably concurrent AND the fault
  provably materialized (the nemesis leg's typed evidence, never a hard-coded flag) AND
  the post-heal final read resolved every member;
- the verdict is parsed from the checker's **output token** (`true`/`false`/`:unknown`),
  never keyed on the exit code alone — the real jar exits 0 for `:unknown`;
- the committed fixtures are **real elle-cli samples** (known-good → `true`, known-bad →
  `false`, plus a captured degraded shape), and every live run re-feeds them through the
  same jar before trusting its verdicts.

The committed report — `docs/design/reviews/m4-checked-consistency-run.md` — is the
deliverable; its carve-outs section states what the verdict does *not* license. The first
carve-out matters most: the client saw **zero** errors during the partition (a 1-of-3
partition leaves FDB a working quorum), so this run attests correctness under an
*absorbed* fault, not under client-visible disruption. A follow-up run under a
quorum-costing fault is deliberately out of scope here.

To try the host-independent logic without any special setup (both run inside the
unprivileged `cargo xtask ci`):

```
cargo test -p xtask --test consistency_run_orchestration
cargo test -p wyrd-server --test consistency_workload --test consistency_observable
```

The live run is opt-in and privileged: `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=<jar> cargo xtask
consistency-run` (needs docker, java, unzip, and the elle-cli 0.1.9 standalone jar).

## Root cause
No pipeline existed to produce the #329 credibility artifact, and the one premise the
landed substrate rested on had never been verified: the #406 Elle-EDN serializers emit a
vocabulary the real checker rejects — a scalar `:value` for `rw-register` ("Don't know how
to create ISeq from: java.lang.Long") and `:remove`/`:contains` for `set` ("No matching
clause"). Every shape this PR emits was instead verified by running the actual elle-cli
0.1.9 jar.

## Fix
- `crates/server/src/consistency_workload.rs`: the #406 serializers amended in place to
  the checker-verified vocabulary — `rw-register` transaction micro-ops
  (`[[:w key v]]`/`[[:r key v]]`) and a create-only integer `set` with ONE composed
  post-heal `:read`. Ops the models cannot represent (register DELETE, directory
  remove/probe) are excluded by pool construction, never per-op filtering; the #406
  consistency *checks* are untouched. The pool keys live here too: the delete pool is
  single-writer-per-key (`delete_pool_key`) because the client-assigned version tags the
  checks compare order by commit only under a single writer — shared-key version bands
  would fabricate violations on a correct system.
- `crates/server/src/consistency_observable.rs`: every invoked op is now recorded — a
  transport failure (exactly what a nemesis induces) records the op as indeterminate
  (`:info`) and returns the recorded `OpRecord`, so an op is never silently omitted and a
  caller never reads a neighbour's status off the history's tail. That includes a peer
  that accepts the connection and then closes without a complete HTTP response: response
  parsing is fallible (`parse_response`/`dechunk` return `Err`, propagated through
  `send`), never a panic that would abort the workload task and drop the invoked op, and
  a body must be completely framed — non-chunked against its declared `content-length`,
  chunked through every CRLF delimiter up to the terminal trailer — so a `42` register
  value reset after its first byte is `:info`, never a determinate read of version `4`.
- `xtask/src/consistency_run.rs` + `consistency_run_runner.rs` (new): the run-summary
  seam (`deny_unknown_fields` at every nesting level), the non-vacuity gate, the elle-cli
  invocation building, the token-keyed verdict parser, the fixtures self-check, and the
  report renderer — no `wyrd-server`/FDB/JVM dependency in xtask. Teardown is
  unconditional: bring-up runs inside the finalizer scope, so even a partially created
  compose stack is torn down (`fdb_faults::run_metadata_nemesis`'s pattern).
- `crates/server/tests/consistency_run_fdb.rs` (new, `fdb`-feature-gated): the live
  scenario — bring-up → three workload pools → partition window (#407 `drive_leg`) →
  heal → quiesce → member sweep with re-probes → composed final read → summary emission.
- `xtask/tests/fixtures/consistency-run/` (new): real-elle-cli-produced golden histories
  and captured checker outputs, pinning vocabulary and parser at review time and re-checked
  by every live run.
- `docs/design/reviews/m4-checked-consistency-run.md` (new): the first witnessed run's
  report — workload, nemesis + typed evidence, history sizes, models, checker version +
  jar sha256, verdicts, and the carve-outs.

## Verification
- **Claim:** the emitted EDN is the vocabulary the real checker accepts. **Checked:**
  `crates/server/src/consistency_workload.rs:396` (`MultiProcessHistory::to_elle_edn`,
  txn micro-ops) and `:910` (`DirectoryHistory::to_elle_edn`, integer `set`); pinned
  against the actual jar by the committed fixtures
  (`xtask/tests/fixtures/consistency-run/`) and re-confirmed off-line by the runner's
  self-check on every live run — both models, both polarities, plus the degraded shape.
- **Claim:** the verdict cannot be misread from an exit code. **Checked:**
  `parse_checker_output` at `xtask/src/consistency_run.rs:528`, fed the captured real
  checker outputs (incl. `:unknown` with exit 0) in
  `xtask/tests/consistency_run_orchestration.rs`.
- **Claim:** a vacuous run cannot report a verdict. **Checked:** `evaluate_summary` at
  `xtask/src/consistency_run.rs:341` — requires the concurrency witness, the materialized
  fault with typed evidence, and a determinate composed final read; each arm has a test
  that turns red when the gate arm is deleted.
- **Claim:** an indeterminate op can neither fabricate a pass nor a violation.
  **Checked:** `crates/server/src/consistency_observable.rs:62` (`INDETERMINATE_STATUS`)
  and `:74` (`OpFailed` — the op is recorded, never dropped); `delete_pool_key` at
  `crates/server/src/consistency_workload.rs:115` (single writer per key);
  `compose_final_read` at `:845` (an unresolved member degrades the read to `:info`,
  never a silent omission that Elle reads as a lost element). Guarded by
  `crates/server/tests/consistency_workload.rs:279`, `:380`, and `:432` — the last one
  *exhibits* the fabrication: it feeds the shared-key banded shape through the production
  checks and shows them wrongly report a violation on a linearizable history.
- **Claim:** the report's history sizes don't overstate what was checked. **Checked:**
  `DirectoryHistory::op_count` at `crates/server/src/consistency_workload.rs:1002`
  (sweep probes are the composed read's raw material, not history ops), pinned by
  `crates/server/tests/consistency_workload.rs:350`.
- **Test:** `xtask/tests/consistency_run_orchestration.rs` (40 tests) — fails pre-change
  (it imports `xtask::consistency_run`, absent on `main`, so reverting the production
  code while keeping the test fails to compile) and passes post-change, inside the
  unprivileged `cargo xtask ci`. The serializer/pool tests live in
  `crates/server/tests/consistency_workload.rs` and `consistency_observable.rs`.
- **Witnessed run:** `WYRD_TIER1=1 cargo xtask consistency-run` on the live
  3-node cluster, 2026-07-16 — exit 0; register (`rw-register`): `true` over 120 ops;
  directory (`set`): `true` over 121 ops; delete pool: 480 ops, all three Wyrd-side
  checks held; partition materialized with typed evidence; fixtures self-check passed.
  Report committed at `docs/design/reviews/m4-checked-consistency-run.md`. The scheduled
  privileged CI job for this run is #409.

Fixes #408
