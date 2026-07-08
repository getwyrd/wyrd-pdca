# Adversarial review — issue #406 (consistency workload + Elle serializer)

**Advisory only — never gates.** I re-ran the green half in the target checkout: all 8
tests in `crates/server/tests/consistency_workload.rs` pass (6 socket-free crafted-history
reds→green + the 2 wire-driven workload tests; loopback bind *was* permitted in this
sandbox, so leg (a) ran). I attempted to refute the serializer golden, the session checks,
the concurrency witness, and the verdict seam. Findings below; several are `NEEDS-HUMAN`.

## Refutation attempts

- **The concurrency witness is over-broad — read↔read overlaps count as "genuine
  concurrency", so the non-vacuity claim is weaker than success-criterion (a) intends.**
  `MultiProcessHistory::overlapping_pairs_across_processes` (`crates/server/src/consistency_workload.rs:115`,
  predicate at `:119`) counts *any* distinct-process overlapping pair with **no op-kind
  filter**; `is_genuinely_concurrent` (`:129`) then returns true on ≥1 such pair. In the
  workload the two reader clients (processes 1,2) loop GET for the whole run and overlap
  *each other*, so the assertion at `crates/server/tests/consistency_workload.rs:352` is
  satisfied even when **no read span ever overlaps a write span** — the only overlap that is
  non-vacuous for register linearizability. Concrete failing-to-prove case: shrink the write
  phase so all 24 PUTs complete before the readers' mutual-overlap window; `overlapping_pairs
  >= 1` still holds and the test still passes, yet the history exercises **zero read/write
  concurrency**. The witness therefore does not prove the property Elle would need to find
  anything; it proves only that two clients ran at the same time.

- **NEEDS-HUMAN — the serializer bakes *indeterminate* outcomes into *definite* ones, which
  can make the deferred off-Check Elle verdict unsound.** `register_completion_type`
  (`crates/server/src/consistency_workload.rs:274`) maps every non-2xx write to `:fail`, and
  `DirectoryHistory::from_register_history` (`:487`, mapping at `:497`) maps every GET-probe
  status ≠ 200 to `present: Some(false)` (definitely absent). A 5xx / timeout write is
  *indeterminate* (it may have committed) and a 5xx probe is *unknown* membership; the
  Jepsen/Elle convention records those as `:info`, **not** `:fail` / definitely-absent.
  The register golden hard-codes this (a status-500 write at
  `crates/server/tests/consistency_workload.rs:95` asserted as `:type :fail` at `:109`).
  When the off-Check Elle job consumes this EDN, a committed-but-500 write recorded `:fail`
  can hide or fabricate an anomaly — the exact "false-accept an inconsistent history" class
  the v1–v3 re-plan warns against, merely relocated downstream. Human should confirm the
  verdict's soundness under this mapping (or switch 5xx/unknown to `:info`).

- **NEEDS-HUMAN — the verdict-dispatch seam (e) has no consumer in the tree; the exemplar it
  cites clears a bar this one does not.** `verdict_dispatch` / `VerdictDispatch`
  (`crates/server/src/consistency_workload.rs:627`, `:648`) is referenced **only** by its
  own module and test (grep across the repo finds no `xtask`/runner/off-Check-job caller).
  The cited exemplar `metadata_tier_dispatch` is wired into `xtask/src/faults.rs` and a live
  scenario test, and its own docstring sets the bar as "flips red behaviourally — **not by a
  constant the runner never reads**" (`xtask/src/metadata_faults.rs:37`). As landed,
  `verdict_dispatch` *is* a constant no runner reads; the red→green
  (`crates/server/tests/consistency_workload.rs:266`) proves only that a pure `bool→enum`
  mapping wasn't swapped — near-tautological, with zero production reach today. This is
  arguably inside the brief's DEFERRED scope, but the reviewer's "mirrors `metadata_faults`"
  acceptance mirrors the *shape*, not the *wiring* that made the exemplar non-vacuous.

- **NEEDS-HUMAN — per-process invoke→complete nesting relies on wall-clock monotonicity that
  `SystemTime` does not provide.** `to_elle_edn` orders events by `(time, process, seq, phase)`
  (`crates/server/src/consistency_workload.rs:311`, sort at `:367`) where `time` is
  `SystemTime`-derived (`relative_nanos`, `:258`). A process's next-op invoke is placed after
  its prior-op completion only because `start_{i+1} >= end_i` in wall-clock terms — but
  `SystemTime` is non-monotonic (NTP step/skew). A single backward clock adjustment between
  two sequential ops makes `invoke_{i+1}.time < complete_i.time`, sorting a process's next
  invoke *before* its own prior completion → an EDN history where one process holds two
  concurrent ops (invalid / rejected by a register checker). Low-probability and partly
  inherited from #405's `OpRecord` using `SystemTime`, but the serializer is new here and its
  ordering hinges on it; a monotonic sequence or `Instant`-based ordering would close it.

- **NEEDS-HUMAN — `session_read_your_writes` clears the obligation on the session's own
  DELETE, admitting a resurrected stale read that the crafted reds never exercise.**
  `crates/server/src/consistency_workload.rs:387` (delete arm `:405`, `last_write.remove` at
  `:406`) drops the key's write-watermark on DELETE, so `[PUT k v5, DELETE k, GET k v3]`
  returns `true` (accepted). Whether reading v3 after your own delete is a RYW violation is
  semantically arguable (a delete arguably imposes no lower bound), but the crafted reds
  (`crates/server/tests/consistency_workload.rs:1052`-area) only cover PUT→stale-GET, never
  delete-then-read — so this false-accept branch is untested. Given the whole bundle exists
  because earlier iterations *false-accepted* inconsistent histories, human should pin the
  intended delete semantics rather than leave it unasserted.

- **The recorded red→green evidence is a whole-module-absent (compile-failure) red, not the
  per-mutation "module-weakening" red the brief promises.** `check-gates.json` C4-verify
  records "red without the fix, green with it," but for a net-new module the only pre-fix red
  is the test binary failing to compile because `consistency_workload` is absent — the
  "red rests on non-existence" pattern the brief's §Verification-posture explicitly disclaims.
  The per-assertion module-weakening reds (mutate `register_f`, swap `verdict_dispatch` arms,
  weaken `>= w`) are *asserted* flippable but were not independently gate-verified. I
  confirmed all 8 tests GREEN post-fix in the target checkout; I did **not** reproduce any
  per-mutation red (read-only — cannot edit the target). So the strongest evidence on record
  is *non-existence-red + green*, which is weaker than the brief's "demonstrated red on real
  inputs, not resting on non-existence."

## What I could not refute

- The register and directory **golden bytes** are internally self-consistent: I hand-derived
  the `(time, process, seq, phase)` sort and op-kind/value mappings and they reproduce the
  golden strings exactly; a wrong field name, op-kind, ordering, or `:time` origin does flip
  them red. Attempted to find an ordering case where a same-process completion sorts after a
  later invoke under *monotonic* time — could not (the seq tie-break prevents it; only the
  non-monotonic-clock case above breaks it).
- `session_monotonic_reads` and the reused `versions_monotone_per_key` reject the crafted
  regressions and accept the monotone runs as claimed; I could not construct a regressing
  single-session read sequence they wrongly accept.
- The wire leg's `version_climbs_for_key` and `per_process_reads_monotone` assertions
  exercise real gateway commits (not mocks) and passed against the real in-process wire here.
