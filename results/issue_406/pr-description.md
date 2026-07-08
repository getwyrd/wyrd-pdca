# server: check metadata consistency under concurrent clients

## Summary
**User impact:** Wyrd's metadata store promises that a client always sees its own
writes and never watches a value go backwards (read-your-writes and monotonic
reads). Until now those promises were only exercised by a single client running
one operation at a time, so nothing checked that they still hold when clients hit
the same key at the same moment — one overwriting it while another reads it. A
change that let concurrent clients observe a stale, torn, or resurrected value
could land without any test catching it.

This adds a concurrent workload that drives several real S3 clients at once,
records what each of them observed, checks those consistency promises against the
combined history, and serializes that history so an external checker can pass the
final linearizability verdict. It is the third step of #329's consistency-checker
work (ADR-0041); there is no dedicated tracker URL beyond the linked issue.

## What to look at
The workload lives in one self-contained module plus its test; nothing else in the
server changes. The two ideas worth a close read are the concurrency check — only a
same-key read-versus-write overlap between two *different* clients counts as
genuine concurrency, because that is the only overlap that actually constrains a
single value — and the soundness rule that an operation whose outcome is unknown (a
timeout or 5xx) is never scored as a definite success or failure.

To try it: `cargo test -p wyrd-server --test consistency_workload`. Most of the
suite is deterministic and needs no network; one test binds a loopback port and
drives two concurrent clients through the real S3 HTTP wire to prove the recorded
history is genuinely concurrent.

## Root cause
This is net-new capability rather than a bug fix. Wyrd had a single-client,
sequential consistency observer but no concurrent workload, no cross-client history
merge, and no serialization a recognized register/set checker could consume — so
the store's behaviour under concurrent access was never checked end to end, and the
consistency guarantees in ADR-0015 rested on inspection rather than an executable
check.

## Fix
Adds a `consistency_workload` module to `wyrd-server` that:

- runs two or more real `ObservableS3Client`s concurrently against the in-process
  gateway and merges their per-client histories into one real-time-ordered log,
  each operation tagged with the client that observed it;
- counts an overlap as genuine concurrency only when it constrains a single
  value — same key, read-versus-write, across distinct clients — so a cross-key or
  read-versus-read overlap is not mistaken for concurrency;
- checks read-your-writes and monotonic reads, treating any indeterminate (timeout
  / 5xx) outcome as "may or may not have happened" so it never manufactures a false
  violation, while still rejecting a read that regresses or resurrects a deleted
  key;
- serializes the history (register and directory-as-set forms) to the checker's
  operation-history format, mapping every indeterminate outcome to `:info` rather
  than a fabricated definite result;
- routes the linearizability verdict to a separate privileged job by default, so no
  JVM/Clojure checker is pulled into `cargo xtask ci` (ADR-0041, ADR-0016).

The gateway, metadata store, and S3 wire are unchanged; the module only consumes
the landed observable client and the existing PUT/GET/DELETE wire.

## Verification
- **Claim:** Two or more concurrent clients over the real wire produce a
  non-vacuous, genuinely concurrent same-key read↔write history.
  - **Checked:** `crates/server/src/consistency_workload.rs:143` —
    `read_write_overlapping_pairs_across_processes` counts only same-key,
    read-versus-write overlaps across distinct clients.
  - **Test:** `crates/server/tests/consistency_workload.rs` —
    `concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history` binds a
    loopback port, drives a concurrent writer and reader on a shared key, and
    asserts at least one such overlap; green under the full `cargo xtask ci`
    (which permits loopback bind).

- **Claim:** An indeterminate (timeout / 5xx) outcome is never scored as a definite
  success, failure, obligation, or membership.
  - **Checked:** `crates/server/src/consistency_workload.rs:57` (`is_indeterminate`),
    `:500` (`register_completion_type` maps it to `:info`), `:187`
    (`session_read_your_writes` guards its PUT and DELETE arms), `:641`
    (`membership` returns Unknown, never a fabricated Absent).
  - **Test:** `register_serializer_emits_byte_exact_elle_edn_with_info_for_indeterminate`,
    `session_read_your_writes_guards_indeterminate_on_every_arm`, and
    `directory_serializer_maps_indeterminate_probe_to_info_not_fabricated_absence` —
    each assertion is red when its guard is removed from the module and green with
    it in place.

- **Claim:** A read that regresses or resurrects a deleted key is rejected, while a
  valid history (including one whose only ordering-relevant op is indeterminate) is
  accepted.
  - **Checked:** `crates/server/src/consistency_workload.rs:187`
    (`session_read_your_writes`), `:249` (`session_monotonic_reads`), `:265`
    (`reads_monotone_per_key`).
  - **Test:** `session_monotonic_reads_guard_indeterminate_and_reject_determinate_regression`
    and `reads_monotone_per_key_is_global_and_guards_indeterminate` reject the
    crafted violations and accept the valid histories.

- **Claim:** The recorded history serializes stably to the checker's
  operation-history format, and the linearizability verdict runs off-gate.
  - **Checked:** `crates/server/src/consistency_workload.rs:282` (`to_elle_edn`) and
    `:742` (`consistency_verdict_dispatch` routes to the privileged off-gate job by
    default).
  - **Test:** the two serializer tests above assert byte-exact golden output;
    `verdict_dispatch_routes_to_off_check_elle_by_default` pins the default route.
    The golden proves the serializer is stable and well-shaped; the recognized
    checker (JVM/Clojure) parses it and returns the verdict in the separate
    privileged job, not in this gate.

- **Claim:** The change is a consumer only — no change to the gateway, metadata
  store, or wire.
  - **Checked:** `crates/server/src/lib.rs:18` adds `pub mod consistency_workload;`
    beside the landed `consistency_observable`; the workload drives
    `ObservableS3Client` (`crates/server/src/consistency_observable.rs:127`) over the
    existing PUT/GET/DELETE wire floor (`crates/gateway-s3/src/lib.rs:347`), with no
    edit to those files.

Fixes #406
