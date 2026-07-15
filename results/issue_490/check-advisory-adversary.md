# check-advisory-adversary.md — issue 490 / renew-pending-lease-resurrection (iteration 3 patch)

Skeptic's pass. I independently re-ran the red→green evidence (not just re-read the gate) and
attacked the fix's boundaries, atomicity claims, and caller contracts. The target worktree was
restored byte-identically after the red leg (live diff sha256 verified before/after).

## Evidence — attempted to refute, could not

- **Red leg re-run (independent):** reverted all 11 modified files to base `dc503cd`, kept only the
  added `crates/core/tests/stream_lease_lapse.rs`, ran it: it **compiles clean against base** (no
  mechanical compile-error "red") and fails with 4 genuine assertion failures for the right reasons —
  `Committed` where refusal is required (`crates/core/tests/stream_lease_lapse.rs:262`, `:305`,
  `:356`) and an `Ok` plan + publish where an abort is required (`stream_lease_lapse.rs:185`); the
  healthy control passes on base. **Green leg re-run:** all 5 added tests plus all 4 in the modified
  `stream_lease_renewal.rs` pass on the patched tree. The C4-verify claim holds.
- **Tautology probe:** Repro (A) drives `commit_overwrite` on the `Ok` arm before asserting
  (`crates/core/tests/stream_lease_lapse.rs:180-181`), so "nothing published" is genuinely
  falsifiable — iteration 1's tautological binding is fixed as the brief demanded. The
  fault-injection guard (`stream_lease_lapse.rs:172-176`) pins that the sweep really reclaimed the
  in-flight chunk id.
- **Production-path probe:** the tests drive `write::stream_write_data`, `write::commit_overwrite`,
  `write::sweep_expired_leases` and `read::read_path` over `RedbMetadataStore::in_memory()` +
  `FsChunkStore` — the production seams; no parallel re-implementation, no mocks.

## Fix — attempted to refute, could not

- **Atomicity / require-after-put ordering:** both leased committers append the per-chunk lease
  `require`s *after* the puts in the builder chain (`crates/core/src/metadata.rs:339-341`,
  `:583-585`). Not a hazard: `WriteBatch` keeps `preconditions` in a separate vec evaluated as a
  set (`crates/traits/src/lib.rs:648-655`) and redb checks every precondition inside the serialized
  write transaction before applying writes (`crates/metadata-redb/src/lib.rs:139-141`). The
  interleave the brief forbade (sweep between read-back and commit) turns the require false — the
  guard value is the exact read-back bytes (`crates/core/src/metadata.rs:694-697`).
- **`<=` boundary:** the new guards (`crates/core/src/metadata.rs:658`, `:694`), the sweep
  (`crates/core/src/write.rs:610`) and GC's expired-lease input (`crates/custodian/src/gc.rs:254`)
  all agree on `expiry <= now`. The exact-deadline tests (`stream_lease_lapse.rs:341-356`, in the
  added, red-proven file; `stream_lease_renewal.rs:851`, `:953`) kill a `<` mutant — iteration 1's
  rejected boundary bug is closed at both seams.
- **Renewal-cadence gap probe:** tried to construct a lapse the renewal check misses: leases are
  renewed en masse to `now+TTL` with `renew_at = now+TTL/2` (`crates/core/src/write.rs:478`,
  `:491`), so any lapse implies `now >= min_expiry > renew_at`, meaning the conditional renewal
  always fires and refuses before the next chunk is written. No gap found.
- **Existing-suite conformance:** suites driving phase 3 directly all run `intent` first
  (pre-existing, or added by this patch, e.g. `crates/server/tests/erasure_path.rs:153-156`);
  `metadata::commit_chunk_map` stays deliberately unconditional for reconstruction/backfill per the
  brief's out-of-scope carve-out.
- **Create-seam coverage:** the create-seam refusal tests live in the *modified*
  `stream_lease_renewal.rs` (`:896-945`, `:953-1000`), so they are post-fix regression guards, not
  part of the red→green proof — but that placement is what the brief's compile-against-base
  constraint itself mandates (the added file may not call the changed-signature `commit_create`).
  Both guards pass green and pin the seam a `create_leased`-only mutant would reopen. Could not
  turn this into a refutation.

## Findings

- NEEDS-HUMAN — **Buffered PUT availability edge (fitness call, not a defect):** the buffered
  `put_object` path (`crates/server/src/lib.rs:158-166`) has **no lease renewal** — it stamps
  `now+TTL` at intent and commits at a fresh `now_millis()` (`crates/server/src/lib.rs:184`,
  `:193`). With `DEFAULT_LEASE_TTL_MILLIS = 30_000` (`crates/server/src/lib.rs:49`), any buffered
  PUT whose data phase takes longer than 30s now deterministically fails at commit with `Conflict`,
  and a retry re-runs the same >TTL data phase, so it can never succeed. This is the invariant
  working as specified (those bytes genuinely are GC-reclaimable; before the patch this published
  the durability hole instead), but it converts silent corruption into hard unavailability for
  large/slow buffered PUTs. A human should confirm this failure mode is acceptable or file a
  follow-up (renew on the buffered path, or route buffered PUTs through the streaming path).
- NEEDS-HUMAN — **Caller-supplied `now` leaves the expiry arm vacuous on the `write_new_object`
  compositions (brief-prescribed, latent):** `write_new_object` / `write_new_object_placed` pass
  their start-of-call `now_millis` as the commit instant (`crates/core/src/write.rs:343`, `:378`),
  so on those paths `expiry = now+TTL > now` is always true — only the absent/changed (`require`)
  arm of the create guard can fire. Today's only production callers are the frozen-clock CLI
  (`crates/server/src/cli.rs:67`, `NOW_MILLIS = 0`, documented "the CLI runs no custodian sweep"),
  so there is **no live defect**; but a future caller reusing these helpers with a real clock
  inherits exactly the Repro-(C) shape (present-but-expired publishes) because wall time elapsed
  inside `write_fragments` is invisible to the guard. Obligation (e) of the brief itself prescribed
  passing this in-scope `now`, so this is a scope/architecture note for the human, not a builder
  iteration item.

## Reviewer-verdict probe

- `check-gates.json` C4 rows: both re-verified independently here (see above); no rationalized
  claim found. The one thing I could **not** re-verify is the full `cargo xtask ci` environment
  (the iteration-2 carry-forward noted a prior sandbox `PermissionDenied` on loopback binds at
  `list_delete_over_grpc`); the gate row records "all checks passed" and is deterministic, so I
  treat it as credible — environment doubt, not substantive doubt (issue #236).

**Bottom line:** attempted to refute the red→green evidence, the batch-atomicity claim, the `<=`
boundary at both seams, the renewal-cadence coverage, and the existing-suite updates; **could not**.
The two NEEDS-HUMAN findings above are consequence/latency notes for sign-off, not holes in the fix.
