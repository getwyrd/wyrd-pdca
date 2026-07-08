# Build notes — issue 406 / consistency-workload-history-and-elle-serialization

> Withheld from the reviewer. Rationale + refutation for the human at sign-off.

## What was built (net-new, ADR-0041 §Decision, #329 slice 3)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree tip `a7c7408`,
which carries #405's `8af8e97`). Three files:

- `crates/server/src/consistency_workload.rs` (new module) — the substrate.
- `crates/server/src/lib.rs:18` — one line, `pub mod consistency_workload;`, beside the
  landed `pub mod consistency_observable;` (`:17`).
- `crates/server/tests/consistency_workload.rs` (new, the brief's named test file) — the
  load-bearing, flippable regression of record.

The module builds, on top of the landed #405 observable
(`crates/server/src/consistency_observable.rs`: `OpKind:40`, `OpRecord:55`, `History:84`,
`ObservableS3Client:127`), the six deliverables the brief's Scope names:

1. **Multi-process history** (`MultiProcessHistory::merge`) — merges per-client
   `History`s into one real-time-ordered log, each op a `ProcOp { process, record }` tagged
   with its client index (`:process`), sorted by the client-observed span.
2. **Concurrency witness** (`read_write_overlapping_pairs_across_processes` /
   `is_genuinely_concurrent`) — INV-2.
3. **Elle-EDN serializer** (`MultiProcessHistory::to_elle_edn`, `DirectoryHistory::to_elle_edn`)
   — one `:invoke` at each span start + one completion (`:ok`/`:fail`/`:info`) at each end,
   the flat log sorted by relative nanos; fields `:process`/`:type`/`:f`/`:value`/`:time`.
4. **Session RYW + monotonic-read checks** + a **per-key** monotonicity check
   (`session_read_your_writes`, `session_monotonic_reads`, `reads_monotone_per_key`).
5. **Directory-as-set history** (`DirectoryHistory`, create=`:add` / delete=`:remove` /
   probe=`:contains`) — no rename, no wire `LIST` (the wire floor is PUT/GET/DELETE only,
   `crates/gateway-s3/src/lib.rs:347`).
6. **Verdict-dispatch seam** (`consistency_verdict_dispatch` → `ConsistencyVerdictDispatch`),
   mirroring `xtask/src/metadata_faults.rs` (`MetadataTierDispatch:40`,
   `metadata_tier_dispatch:53`).

## The re-plan's whole point: INV-1/INV-2 enforced SURFACE-WIDE, not point-wise

v1–v3 built an in-gate linearizability verdict (rejected: false-accept). v4/v5 built the
substrate but fixed the two governing faults only at the exact spot a sign-off named, and the
same fault-class reappeared in an adjacent arm. So before writing any red I enumerated every
arm that (α) establishes an obligation / maps a wire outcome, and (β) counts an overlap, and
shipped **one crafted, socket-free, flippable red per arm**:

- **INV-1 (α):** the single predicate `is_indeterminate(status) = status == 0 || status >= 500`
  gates *every* arm — the register completion-type (`register_completion_type`), the directory
  completion-type + `membership` derivation (200→Present, 404→Absent, **else→Unknown**), the RYW
  **PUT** arm and **DELETE** arm (an indeterminate mutation clears the obligation to `Unknown`,
  never sets `AtLeast`/`Absent`), the RYW **read** side (keyed on the *status*, not on
  `version.is_some()`, so a crafted indeterminate GET carrying a stale `Some(_)` is not a
  violation), and both monotonic-read checks (`reads_are_monotone` skips on the *status*, the
  exact INV-1(iv) subtlety).
- **INV-2 (β):** the witness counts a pair only when `a.process != b.process` **and**
  `a.record.key == b.record.key` **and** read↔write **and** spans overlap. The v5 leak was the
  missing `key ==` clause; the cross-key negative pins it.

### RYW obligation model — why "clear to Unknown on an indeterminate mutation"

An indeterminate PUT/DELETE "may or may not have happened", so afterwards no definite lower
bound *or* absence can be asserted; the sound local choice is to clear the per-key obligation to
`Unknown`. This handles all four brief cases exactly (two ACCEPT-valid-indeterminate, two
REJECT-determinate) and, crucially, **never false-rejects** — the failure mode INV-1 exists to
kill. A contrived multi-indeterminate regression (`[PUT v=5 ok; PUT v=6 500; GET v=1]`) is a
*missed* local detection, not a false-reject; that is the safe direction, because the global
register verdict is Elle's, off-Check, over the SAME serialized history (ADR-0041 §Decision) —
the local check owes soundness-against-false-reject, not completeness.

## Decisions / alternatives ruled out

- **Did NOT re-implement an in-gate register/namespace linearizability verdict** (the rejected
  v1–v3 vehicle). The module produces/records/serializes + sound local invariants only; the
  verdict routes off-Check via `ConsistencyVerdictDispatch::OffCheckElle` (default). This keeps
  the ADR-0041/ADR-0016 division of labour.
- **Verdict-dispatch lives in the server module, not `xtask`.** The brief's named test file is
  `crates/server/tests/consistency_workload.rs`; a server integration test cannot cleanly import
  `xtask`. The brief says "mirroring `metadata_faults.rs`" (the *pattern* — a pure routing value
  with both alternatives representable, the off-Check leg deferred), which I reproduced. Putting
  it in xtask would have cost a new `xtask` dev-dependency edge from `crates/server/tests` (not
  expressible without restructuring), versus 0 extra edges here.
- **Did NOT modify #405's `versions_monotone_per_key`** (`consistency_observable.rs:105`). It is
  INV-1-unsound for the *checker surface* (it counts a PUT's recorded `Some(v)` regardless of
  status, so it would false-reject `[PUT v=2 500; GET v=1 200]`), but it is #405's landed API
  with its own passing test. Rather than mutate a landed function (risking #405's regression and
  overstepping the "consumer of #405" scope), I built the sound analog `reads_monotone_per_key`
  (reads-only, status-guarded) on the new surface. Cost of the alternative (editing #405's fn):
  touches a landed public API + its test `a_regressing_version_is_not_monotone`
  (`consistency_observable.rs:368`) for no gain the new surface doesn't already provide.
- **EDN shape:** exactly the five fields the criterion enumerates (`:process`/`:type`/`:f`/
  `:value`/`:time`); no `:key` field (a multi-key partition is the off-Check job's / a later
  slice's concern). `:time` is nanos relative to the earliest start (Jepsen's test-relative
  clock) so the golden bytes are small and stable. The golden `expected` is a hand-written
  `concat!` literal, NOT recomputed with the serializer's own `join`, so a delimiter/field/order
  drift is genuinely caught.
- **DECISION POINT (version-climb tautology):** left **out of scope** per the brief default —
  the workload asserts the writer's caller-supplied versions climb and that per-key *reads* are
  monotone (a backend-observed climb is a later, opt-in strengthening). Flagged for the human.

## Refutation of the test (forced, recorded)

Each governing arm was weakened in the production module and the corresponding assertion was
observed to flip RED, then reverted (all done via the project's cargo runner in
`$PDCA_WORKTREE`, bounded by `timeout`):

- **(a) Genuine red?** YES — demonstrated per arm, on real inputs, not on non-existence:
  - INV-2: dropped the `key ==` clause →
    `concurrency_witness_counts_only_same_key_read_write_overlaps` panicked
    ("a cross-key read↔write overlap is vacuous …"). Reverted.
  - INV-1 serializer: mapped indeterminate → `Ok` → the golden failed with the *only* diff being
    `:info`→`:ok` on the indeterminate write (line 6 of the EDN). Reverted.
  - INV-1 RYW PUT arm: removed the `is_indeterminate` guard (the exact v5 leak) →
    `session_read_your_writes_guards_indeterminate_on_every_arm` panicked
    ("an indeterminate PUT must not create a definite AtLeast obligation"). Reverted.
  - INV-1 monotonic-read guard: removed the status skip in `reads_are_monotone` → BOTH
    `session_monotonic_reads_*` and `reads_monotone_per_key_*` failed (the crafted indeterminate
    read carries a stale `Some(1)`, so an unguarded check wrongly compares it). Reverted.
- **(b) Production path?** YES — the tests drive the production module `to_elle_edn` /
  `session_*` / `reads_monotone_per_key` / witness / `consistency_verdict_dispatch` directly, and
  the wire leg drives the **real** #405 `ObservableS3Client` over the **real** in-process
  `wyrd_server::Gateway` S3 HTTP wire (`start_gateway` mirrors
  `crates/server/tests/consistency_observable.rs:38` and `s3_http_wire.rs:63`). No mock / copy /
  re-implementation.
- **(c) Fixture includes the fault?** YES — the reds are fed the *actual* faulty shape: an
  indeterminate 5xx op, a cross-key overlap, a read↔read overlap, a determinate resurrect and a
  determinate regression. Nothing is curated out. The wire fixture includes the concurrent writer
  AND reader on a shared key (the killed-node analog here is "genuine real-time overlap"): the
  test asserts `read_write_overlapping_pairs_across_processes()` is non-empty and re-checks each
  returned pair is genuinely cross-process + same-key.

## Verification run (project runner, bounded)

- `cargo fmt -p wyrd-server -- --check` → clean (commit-ready for the target's fmt hook).
- `cargo clippy -p wyrd-server --all-targets` → clean (workspace `warnings = "deny"`; fixed two
  `doc_lazy_continuation` cases by de-listing the invariant docs into prose).
- `cargo test -p wyrd-server --test consistency_workload` → **9 passed** (8 socket-free reds +
  the wire leg); `--lib consistency_workload` → 5 passed.
- Loopback `bind` **is** permitted in this worktree (the wire leg is green here), exactly as
  #405's landed loopback test requires; the socket-free reds carry the flippable RED regardless
  of the sandbox (brief Falsifiability / Verification posture).
- Did not run the full `cargo xtask ci` end-to-end (heavy; Check re-runs it). The change is purely
  additive — one `pub mod` line + two new files, **no** Cargo.toml / dependency change — so
  `cargo deny` / `cargo machete` / conformance / DST / statics are unaffected.

## External dependencies

None missing. The recognized checker (Elle → JVM/Clojure) is **off-Check by design**
(ADR-0041/ADR-0016) and is NOT a build/verify dependency of this bundle at Check — the module
serializes to Elle's EDN input schema in pure Rust and defers the verdict execution to the
privileged off-Check job (`ConsistencyVerdictDispatch::OffCheckElle`). No JVM/Clojure is pulled
into the merge gate. Nothing the brief's External-dependencies field omitted was needed.

## Deferred / for the human at sign-off (pre-declared, not a surprise)

- The **live Elle verdict** over the serialized history is off-Check (ADR-0041); its green is the
  maintainer/nightly job's, over the SAME history the Check-exercised serializer produces. The
  in-gate golden proves the serializer is stable + well-shaped (INV-1), **not** real-Elle-parser
  acceptance — do not over-read it.
- The **version-climb tautology** DECISION POINT is left out of scope (brief default); move it
  in-scope if a backend-observed GET-version climb is wanted.
