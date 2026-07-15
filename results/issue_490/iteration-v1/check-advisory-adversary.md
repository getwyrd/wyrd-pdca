# check-advisory-adversary.md — issue 490 / renew-pending-lease-resurrection

Adversarial pass. I independently re-ran the red→green proof in a scratch copy of the
target tree (green with the patch; red on base production files with the test kept —
fails at the binding "upload must abort" assertion, on the real `stream_write_data` +
`RedbMetadataStore` + `FsChunkStore` production path, no mocks). The evidence is genuine
and the healthy-path suite (`crates/core/tests/stream_lease_renewal.rs`) stays green.
The fix's mechanism is sound where it applies: read-back + `require(current)`+`put` in one
batch (`crates/core/src/metadata.rs:550-565`) closes the check/put interleave, and
`lease_write_chunk` now hard-errors on `Conflict` (`crates/core/src/write.rs:446-451`).
I could not refute the renewal seam itself. I did refute the *invariant claim*:

- NEEDS-HUMAN — **The invariant is not restored; the fix closes the renewal seam but the
  commit seam still publishes over reaped leases.** Renewal only fires inside
  `lease_write_chunk` when a *next* chunk arrives (`crates/core/src/write.rs:431`); a
  stall past the TTL **after the last chunk but before end-of-stream** (a client that
  wrote all bytes and then hangs before closing) never triggers it, and
  `commit_create`/`commit_chunk_map` are unconditional on the pending ledger
  (`crates/core/src/write.rs:247-261`, `crates/core/src/metadata.rs:421-439` — no
  `require` on any `pending:` key). **Probe confirmed on the patched tree**: a 3-chunk
  stream whose generator sweeps at 2·TTL on the EOF pull has all three leases reaped
  mid-call (`sweep_expired_leases` returns [1,2,3]), yet `stream_write_data` returns
  `Ok`, `commit_create` returns `Committed`, and `read_path` serves the object — a
  committed inode referencing chunks whose leases the sweep revoked, exactly the
  never-wrong-bytes violation the brief says can "no longer publish" (success criterion)
  and must "fail closed" (invariant). The same window exists between `stream_write_data`
  returning and the caller driving phase 3 (e.g. during a payload-hash check). The brief's
  Scope narrowed the mechanism to renewal + surfacing, so the patch conforms to its brief —
  but the brief's success criterion ("no committed inode references reclaimed fragments")
  is broader than what the fix delivers. Closing it fully needs a design decision
  (lease-conditional commit, or a pre-commit lease re-verification protocol) — human
  scope call: accept as a partial fix with a follow-up issue, or extend this cycle.

- NEEDS-HUMAN [impl] — **Boundary disagreement between reaper and renewer at
  `now == expiry`.** `sweep_expired_leases` reaps at `lease_expiry_millis <= now_millis`
  (`crates/core/src/write.rs:584`); the new renewal refuses only at strict `<`
  (`crates/core/src/metadata.rs:559`), and its doc claims `now == expiry` is a "healthy
  renewal-at-the-deadline path" (`metadata.rs:534`) — but per the sweep's own contract
  that lease is already dead (a sweep at that same instant reaps it). No unsound
  interleave results (the atomic `require` serializes renewal against the sweep, so
  whichever commits first wins cleanly), so this is a conformance/doc nit, not a
  durability hole — but the two consumers of the lease contract should agree on the
  boundary (note: the brief itself specified `<` in obligation (b), so aligning to `<=`
  needs a one-word brief amendment).

- NEEDS-HUMAN [impl] — **Test binding #3 is tautological as written.**
  `crates/core/tests/stream_lease_lapse.rs:281-287` asserts `read_path(...) == None`
  under the banner "nothing was committed" — but the test never drives phase 3
  (`commit_create`) on any path, so no implementation could ever make that read return
  `Some`; the assertion can't go red on its own (pre-fix the test dies earlier at binding
  #1). The red→green is legitimately carried by bindings #1 and #2 (verified), so this is
  a test-strength nit only: committing the plan on the `Ok` arm (as the pre-fix behaviour
  allows) would make #3 a genuine "committed inode references reclaimed chunk" red.

Attempted and could NOT refute: (a) the red→green proof — reproduced independently, red
fails for the right reason on the production path; (b) the atomicity of the conditional
renewal — `WriteBatch` preconditions are evaluated with the writes in one commit
(`crates/traits/src/lib.rs:648-670`), so a sweep between read-back and commit yields
`Conflict`, not resurrection; (c) hidden callers — `renew_pending` has exactly one caller
(`write.rs:437`), so the signature change breaks nothing else; (d) a mid-stream lapse the
renewal misses — for any lapse observable on the writer's own clock, `now ≥ expiry >
renew_at` forces the renewal to fire before the next chunk is written, so the only
escapes are the end-of-stream/commit window (finding 1) and cross-process clock skew
(subsumed by finding 1's commit-seam gap).
