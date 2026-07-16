# check-advisory-adversary.md — issue #408 (m4-checked-consistency-run-elle-report, v4)

Adversarial pass. I re-ran the green legs at `$PDCA_TARGET` (37/37 orchestration tests,
13/13 + 11/11 server-side consistency tests), re-ran the REAL elle-cli 0.1.9 jar over every
committed fixture, and executed the production #406 checks against a delete-pool-shaped
history in a scratch harness. Two findings are execution-verified refutations, both in the
live scenario file — the one file **no Check gate exercises** (it compiles only under
`--features fdb`; the C4-verify red→green covers only the xtask pure core).

- NEEDS-HUMAN [impl] — **The new delete pool fabricates violations on a correct system; the
  witnessed run will near-certainly FAIL with a false "real violation".**
  `crates/server/tests/consistency_run_fdb.rs:497-509` gives each process a disjoint
  *version band* (p0: `1..`, p1: `1_000_000..`) on the **shared** `DELETE_POOL_KEY`, but all
  three #406 checks judging the pool assume per-key version tags are monotone in *commit*
  order: `reads_are_monotone` compares raw version numbers across processes
  (`crates/server/src/consistency_workload.rs:512-530`), and the RYW arm flags any read
  below the session's own write with no cross-process-overwrite waiver
  (`consistency_workload.rs:260` — only 404s are waived, `:271-282`). Concrete failing case,
  **verified by running the production checks**: the linearizable history
  `p0 PUT v=1; p1 PUT v=1_000_001; p1 GET→1_000_001; p0 PUT v=2 (newer commit, smaller tag);
  p0 GET→2; p1 GET→2` returns `false` from ALL THREE of `reads_monotone_per_key`,
  `session_monotonic_reads`, `session_read_your_writes`. With both processes interleaving 60
  PUT/GET/DELETE/GET rounds on one key inside the fault window, a cross-band read pair is
  essentially guaranteed, and the runner escalates it to "checked consistency run FAILED —
  ... a real violation observed on the live cluster"
  (`xtask/src/consistency_run_runner.rs:537-545`). This inverts INV-1 (a fabricated
  violation instead of fabricated certainty) and blocks the brief's acceptance artifact (the
  witnessed `true`-verdict report). Fix by construction: per-process disjoint *keys* inside
  the delete pool (single writer per key keeps tag order = commit order), not shared-key
  disjoint bands. No Check-time test covers the two-writer banded shape, which is why C4 is
  green anyway.

- NEEDS-HUMAN [impl] — **The composed final read silently omits Unknown-probed members —
  fabricating a "lost element" `false` from Elle on a correct run.**
  `crates/server/tests/consistency_run_fdb.rs:599-608`: a post-heal probe whose status is
  neither 200 nor 404 (5xx/timeout ⇒ `Membership::Unknown`) simply drops that member from
  the composed `:read` set. In the `set` model an acknowledged `:add` missing from the final
  read is a lost element — **verified against the real jar**: the committed
  `directory-history-known-bad.edn` is exactly this shape and returns `false`. Aggravating:
  Design §2 requires the sweep "after heal + quiesce", but `consistency_run_fdb.rs:209` runs
  `compose_final_read` immediately after `drive_leg` returns — no quiesce — so a transient
  probe error while FDB is still recovering from the partition is plausible. An Unknown
  probe must re-probe, abort, or degrade the composed read to `:info`; silent omission from
  a definite `:ok` read is the same INV-1 fabrication the scenario's own comment (`:603`)
  claims to avoid, just in the absence direction.

- NEEDS-HUMAN [impl] — **`deny_unknown_fields` does not cover the nested seam objects,
  contradicting the "seam fails loudly" claim.** `xtask/src/consistency_run.rs:136-152`
  (`NemesisEvidence`) and `:159-169` (`OutcomeCounts`) lack `#[serde(deny_unknown_fields)]`
  (serde does not propagate it from `RunSummary`), so a field the scenario adds inside
  `nemesis` or any outcomes object is still silently dropped — the exact
  `member_id_map`-style loss the doc at `consistency_run.rs:220-226` declares closed. The
  orchestration test pins only a top-level unknown field
  (`xtask/tests/consistency_run_orchestration.rs:337-352`).

- `RUN_STAGES`/`run_plan` is a mirror, not the production path: the impure runner never
  consults it (`run_consistency_check`'s control flow at
  `xtask/src/consistency_run_runner.rs:499-564` is hand-sequenced), so
  `run_plan_carries_bring_up_through_report_in_order` would stay green if the runner dropped
  a real stage. Mild tautology (shared with the `metadata_faults` peer pattern) — the
  reviewer's "run-orchestration plan exercised red→green" claim is true only of the
  constant, not the orchestration.

- Minor overstatement: `directory_ops = creates.len() + universe.len()`
  (`crates/server/tests/consistency_run_fdb.rs:217`) counts the post-heal sweep probes as
  history ops, but they enter the EDN only as ONE composed read — the report's "history
  size" field overstates the checked directory history (~2x), against the Success
  criterion's "refuses to overstate itself".

Attempted and could NOT refute: (a) the golden fixtures' authenticity — I re-ran
`java -jar elle-cli-0.1.9-standalone.jar` over all five committed EDN fixtures and got
byte-identical verdict lines to the committed capture files (`true`/exit 0, `false`/exit 1,
`:unknown`/exit 0), so the "REAL elle-cli-accepted samples" claim holds; (b) the
three-valued verdict parser (token-keyed, `:unknown`+exit-0 → inconclusive, `true`+non-zero
→ inconclusive) — consistent with observed real outputs; (c) the C4-verify red — the
`xtask::consistency_run` module is absent on `origin/main`, so the kept test genuinely
fails pre-fix; (d) the v3 sign-off items are all structurally addressed (OpFailed returns
the op's OWN record, errored ops recorded as `:info`, checker version + member-id map cross
the seam into the report). The gates' green is real for what it measures — but note for the
verdict: `check-gates.json`'s all-pass says nothing about `consistency_run_fdb.rs`, where
both execution-verified defects live; per the v3 sign-off rationale the live leg must not
be attempted until they are fixed.
