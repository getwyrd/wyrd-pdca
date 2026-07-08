# Adversarial review — issue #406 (elle-register-listappend-models-and-workload-recorder)

Advisory / non-gating. Grounded on the target source at
`/home/eddie/wyrd/wyrd.pdca-wt-l0`. Toolchain available (cargo 1.96.0); I re-ran the
suite and built a standalone harness that links the **real** `wyrd-testkit` crate.

## Refutations found

- **NEEDS-HUMAN — the register model FALSE-ACCEPTS a read of a definitely-failed
  write's value at a version the commit point never produced.**
  `crates/testkit/src/consistency.rs:310-317` builds the value-provenance domain
  `written` from **every** write op, *without* the `c.ok` filter that Pass 1
  (`consistency.rs:324`) and the Pass-2 seed (`consistency.rs:345`) both apply. So a
  value that only a **`Fail`** write attempted — which the module's own doc
  (`Kind::Fail`, "definitely did not take effect") says never committed — is treated as
  legitimately readable. Concrete failing case, **empirically ACCEPTED by the real
  `check_register`**: `[write_ok "k"=10 @v1; write_fail "k"=99; read_ok "k"=99 @v2]`.
  Value 99 was never committed and version 2 was never produced, yet `check_register`
  returns `Ok`. The degenerate `[write_fail "k"=99; read_ok "k"=99 @v5]` is also
  accepted. This directly contradicts the brief's own TornRead definition ("a read
  returned a (value, version) the commit point never produced"). It is unreachable by
  *this* workload only because `put_with_retry` never records a register `write_fail`
  — but `HistoryRecorder::write_fail` is public API and the wire-driven (#405) /
  fault-injected (#407) histories this "reusable machinery" is built for **will**
  contain rejected writes, where the model will silently under-report torn reads.
  One-line fix: gate `written` on `c.ok`, as the sibling namespace model already does
  (`consistency.rs:767`, `if !c.ok { continue; }`).

- **NEEDS-HUMAN — the recorded C4-verify "red" may be the missing-symbol / compile-error
  red the brief explicitly disavows, not a model-weakening red on real inputs.**
  `check-gates.json` C4-verify asserts "red without the fix, green with it" (non-gating).
  The crafted-rejection tests are `expect_err` assertions; if "without the fix" reverts
  the whole patch, `tests/consistency_models.rs` fails to **compile** (undefined
  `check_register`, `RegOp`, …) — exactly the "red … by a missing symbol" the test
  docstring and brief §Falsifiability say does **not** count. Genuine flippability
  requires weakening a model to accept-all and watching the `expect_err`s go red.
  `run-verify.sh` lives in the driver, not the target, so I cannot confirm which red was
  captured. Human: confirm the recorded red was a model-weakening red, not a compile
  failure.

- **NEEDS-HUMAN — the brief/Scope claims the workload drives "create/delete/rename", but
  rename is entirely unexercised and cannot even be recorded.** `NsOp::rename_invoke/ok`
  (`consistency.rs:575,583`), the `NsF::Rename` remove+add logic in `check_list_append`,
  and the `[:rename …]` EDN branch exist, but `HistoryRecorder` exposes **no** rename
  method and **no** test constructs a rename op (0 occurrences in
  `crates/testkit/tests/consistency_models.rs`). The workload does create/delete/list
  only. The rename branch of the namespace model is dead relative to its coverage — it
  could be wrong with no failing test.

- **The workload does not actually exercise the session checks' reject paths — the
  "gives the session checks live teeth" comment overstates.** In
  `crates/testkit/tests/consistency_models.rs` `run_process`, contended HOT writes
  record `version = None` (`put_with_retry` returns `None` whenever the writer's own
  value was overwritten before `observe`), so `check_read_your_writes` never sets a floor
  for HOT (`consistency.rs:447-451` skips `None`). The only keys with observable
  own-writes are the uncontended `reg/p{p}` — single-writer, monotonically climbing
  versions — where RYW and monotonic-read violations are impossible by construction. So
  the produced history can never trip either session check; their reject logic is
  validated **only** by the hand-crafted histories. Consistent with the brief's declared
  posture, but the inline comment claims more than the code delivers.

- **Exactly-one-writer-wins is only partially modeled.** `consistency.rs:322-339` flags
  `TwoWinners` only when two committed writes report the same `(key, version)` with
  **distinct** values. Two winners with identical values — or a real double-commit that
  `put_with_retry` masks by recording `version = None` — are not surfaced. Not a
  refutation of the crafted test (which uses distinct values), but the guarantee-2
  "exactly-one-writer-wins" clause is weaker than advertised.

## Attempted refutations that did NOT hold up

- **Concurrency-test flakiness:** ran `workload_against_the_in_process_gateway_…` **40×**,
  0 failures. The barrier genuinely forces overlapping recorded invoke intervals, and
  `version >= 2` is robust (`commit_overwrite` bumps `prior.version + 1` per winning CAS,
  `crates/core/src/metadata.rs:471`; ≥2 successful HOT commits are guaranteed). Not flaky.
- **False-reject of the produced history:** traced `check_register` Pass 3
  (real-time-proxy version-regression) and `check_list_append` (lost-create /
  resurrected-delete) against the recorder's index protocol — ok-events are recorded
  *after* the store op commits, so "definitely before the list/observation" implies truly
  committed-before; no spurious rejection path found. The crafted version-regression,
  torn-read, two-winners, lost-create and resurrected-delete rejections are all genuine
  (each goes red if the model returns `Ok`), and the workload drives the real production
  read path (`wyrd_core::read::resolve`/`read_inode`) and real gateway commit point — not
  a parallel re-implementation.
