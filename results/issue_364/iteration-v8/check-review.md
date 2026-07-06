# Check review — issue 364 / s3-http-wire-surface (iteration 8)

**Task under review:** give the in-process gateway a real client-facing **S3-compatible HTTP
wire surface** — bucket-scoped object **PUT / GET / DELETE** with mandatory **SigV4** auth and
**streaming** bodies, mapping onto the existing `Gateway` write/read paths so the blueprint's
day-one S3 round-trip can run over the wire. This iteration must close the two iter-7
durability findings (durable id-allocator recovery on restart; per-chunk lease renewal on slow
uploads) on top of the already-ratified crate extraction (`gateway-s3`), real-SDK interop, and
overwrite/DELETE fragment-orphaning.

## Grounding / re-run notes
- **Target resolution:** `$PDCA_TARGET` is not readable from this sandbox (env access denied),
  but the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt` holds the patch **pre-image**
  byte-for-byte (`crates/server/src/lib.rs:1-9` = the exact docstring this patch removes), so
  pre-existing-seam citations are grounded there and all new code is grounded on `patch.diff`.
- **Gate re-run:** `cargo`/`xtask` execution is blocked in this sandbox (bash exec denied), so
  C4 rests on the recorded `check-gates.json` (C4-ci **pass**, C4-verify **pass**) plus my
  line-level re-derivation — I could not independently re-run the workspace here. Flagged in the
  C4 basis, not asserted as an independent green.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding at-Check floor is unambiguous (signed loopback PUT→GET→DELETE byte-identical; unsigned/bad-sig refused) with deferred public-TLS/#367 scope explicit — brief:48-62. Spec is testable and bounded. |
| C2 Reproduction (red pre-fix) | PASS | Behavioral reds present, not just compile-error red: `restart_without_recover_collides_showing_the_bug` (s3_http_wire.rs:5651) demonstrates finding-1 corruption without the fix; `slow_streaming_put_renews_in_flight_leases_before_commit` (write.rs test:1703) exercises finding-2. run-verify recorded red→green. |
| C3 Change | PASS | Change maps 1:1 onto spec: HTTP listener + verb dispatch (gateway-s3/lib.rs:2674), SigV4 verify-before-body (lib.rs:2685), streaming PUT/GET (server/lib.rs:4747,4790), net-new DELETE (lib.rs:4839), durable `recover` (lib.rs:4660) wired at composition root (cli.rs:4568). |
| C4 Verification (red→green) | PASS | check-gates.json records C4-ci (fmt/clippy/build/test/deny/conformance) **pass** and C4-verify **pass**; the historical `gateway_lease_expiry` wall-clock flake is quarantined via a 20s skew allowance that still kills the `+→-`/`+→*` mutants (gateway_lease_expiry.rs:5034). NOTE: could not re-run cargo under this sandbox — relying on the recorded gate + line grounding, not an independent green. |
| C5 Causal adequacy | PASS | Root causes removed at source, not guarded: restart collision fixed by `high_water_marks`+`recover` (metadata.rs:1330 / lib.rs:4660); early-chunk GC reclaim fixed by per-chunk lease renewal (write.rs:1522); overwrite/DELETE leak fixed by orphan-ledger grace records in the same atomic commit (metadata.rs:1229,1156); GET-during-DELETE truncation fixed by deferring reclaim to the grace window + Content-Length framing (lib.rs:4790, gateway-s3/lib.rs:2769). **Symptom-guard smell-test: no capability probe / runtime guard around an optional capability — the fixes transform the cause, so C5 does not trip the guard rule.** |
| T1 Structure | PASS | Clean layering: neutral `wyrd-gateway-core` seam (`ObjectGateway`), `wyrd-gateway-s3` wire crate generic over `G: ObjectGateway` naming no concrete, `server` implements the seam and wires concretes only at `cli::cmd_s3` (ADR-0010). Ratifies the iter-6 crate-boundary decision. |
| T2 Shape | PASS | Seam types are minimal and neutral (`ObjectRead{size,stream}`, `ContentHash` enum, streaming `Stream<Item=Result<Bytes>>` source); shared orphan-key protocol hoisted to `core::metadata` as single source of truth so delete-writer and GC-reader can't key-drift (metadata.rs:1093, gc.rs:1788). |
| T3 Runtime | PASS | Bounded-channel streaming GET keeps peak resident at O(chunk) (lib.rs:4801); auth precedes body materialization so unsigned requests never force allocation (gateway-s3/lib.rs:2685 before :2718); constant-time payload-hash compare; XML error escaping (lib.rs:2829). Covered by behavioral tests (streaming-writes-as-they-arrive, unsigned-refused-before-body). |
| T4 Contribution | PASS | Substantial net-new behavioral coverage incl. a genuine independent oracle: real `aws-sdk-s3` dev-dep drives PUT/GET/DELETE byte-identical over loopback (s3_http_wire.rs:6003) — signer/framer is NOT the gateway's own sigv4 — closing the recurring iter-2..5 self-consistency gap; plus overwrite-reclaim, GET-during-DELETE, concurrent-delete-idempotent, restart-recovery, malformed/oversized chunk-header fail-closed. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: this patch edits **shared M4 core** (`write::commit_overwrite` signature change + `metadata.rs`/`read.rs` additions + `custodian/gc.rs` refactor) which the brief names as conflicting with concurrent M4 metadata slices; maintainer must ratify landing this durability seam on the `feat/m4-production-metadata-backend` integration branch vs. its own M4→M7 sequence (brief:63-68,143-145). Crate-boundary/SigV4-scope/TLS-deferral are already ratified (iter-6) — only the sequencing + shared-core blast-radius sign-off remains. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: the at-Check floor (signed loopback round-trip byte-identical; unsigned→403; real-SDK interop green) appears met, but the **LIVE public-TLS S3 round-trip on a deployed host** (blueprint DoD:696-699) is pre-declared **DEFERRED to the first-deployment gate #367** and is not observable here; human must confirm at #367 that the durability floor (restart recovery, lease renewal, overwrite/DELETE orphaning) holds under a **real running custodian topology** (Check runs plaintext loopback, single process, no live GC), and that plaintext-loopback-at-Check remains the accepted posture. |

## Notes for the human
- No blocking (FAIL) findings: every iter-7 carry-forward maps to a grounded fix + behavioral
  test, and the recorded gates are green. My accept-blocking limitation is only that I could
  not independently re-run `cargo xtask ci` under this sandbox.
- Prior-art within the issue history (iterations 1-7, preserved) is well-tracked and each
  rejected approach is visibly not re-attempted unchanged; the affected core/custodian files
  are the same ones prior iterations touched, so the history is the relevant prior art.
