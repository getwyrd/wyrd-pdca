# Result — issue 364 / s3-http-wire-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: an S3-compatible **HTTP listener** in the gateway role; **bucket-scoped**

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix — a net-new, spec-anchored feature landing behind the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 364 / s3-http-wire-surface (iteration 3)

**Task under review:** give the gateway its first client-facing network endpoint — an
S3-compatible HTTP listener serving **bucket-scoped object PUT / GET / DELETE** with
mandatory **SigV4** auth and **streaming** bodies, mapping onto the existing in-process
client paths so the blueprint day-one S3 round-trip (blueprint:698-699) runs over the wire.
This is iteration 3; iterations 1–2 were rejected for a buffering-only floor, hand-rolled
crypto on the auth boundary, non-idempotent DELETE, self-referential SigV4, and unescaped
XML errors — this patch claims to correct all of those.

> Advisory review. Deterministic gates block; my rows annotate. Bash/cargo/git are
> sandboxed off in this session, so I could **not** re-run the workspace test; C4 below is
> grounded on `check-gates.json` (an allowed input) and the source at the target worktree
> `/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied, readable — not stale).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief carries a concrete BINDING floor — signed PUT→GET→DELETE byte-identical + unsigned/bad-sig refused (brief:48-56) — with streaming, fail-closed auth and crate-boundary invariants enumerated (brief:118-134). Well-specified. |
| C2 Reproduction (red pre-fix) | PASS | Net-new surface: a compile/no-listener red is the declared, acceptable red for new coverage (brief:107-108). `run-verify.sh` re-derived red-without-fix / green-with-fix (check-gates C4-verify=pass), so the added test `crates/server/tests/s3_http_wire.rs:1` is load-bearing, not vacuous. |
| C3 Change | PASS | Adds `s3` module (`s3/mod.rs`, `sigv4.rs`, `crypto.rs`), a streaming write seam (`core/src/write.rs:400 stream_write_data`), streaming read (`core/src/read.rs:311 read_chunk_verified`), idempotent `metadata::unlink` (`core/src/metadata.rs:233`) + `Gateway::delete_object` (`server/src/lib.rs:727`), and a `wyrd s3` role (`cli.rs:526`). Reuses the client path; all cited APIs ground on target (`metadata.rs:142 fragments()`, `metadata-redb:36 in_memory`, `traits:317 put_fragment_at`). |
| C4 Verification (red→green) | **FAIL** | **Gating.** The Wyrd CI gate `cargo test --workspace --exclude wyrd-dst` exits 101 (check-gates.json C4-ci, gating=true, overall=fail). The per-fix test passes in isolation (C4-verify=pass) and the patch compiles, so this is a genuine workspace-suite failure, **not** a stale-target apply/compile artifact. The gate output names no failing test and cargo is sandboxed here, so I could not re-run to localise it. Blocks accept until the failing test is identified and green. |
| C5 Causal adequacy | PASS | Root cause is architectural absence (no wire endpoint); the fix adds the actual endpoint and reuses `Gateway` rather than papering over. Symptom-guard smell-test: the PUT's `PayloadHash::Streaming → 501` (`s3/mod.rs:1259`) is an explicit scope boundary (aws-chunked framing undecoded), **not** a capability probe / runtime guard over a load-time side effect — C5 guard does not fire. Iteration carry-forwards (real streaming, RustCrypto, idempotent DELETE, XML escaping) appear addressed at the code level. |
| T1 Structure | PASS | Wire surface lands in `crates/server/src/s3/` alongside the ADR-0010 composition root; crypto/sigv4 split into focused submodules; no stray edits. Crate-boundary *decision* itself is a T5/human item. |
| T2 Shape | PASS | `S3Gateway`/`handle` are generic over `Gateway<M,C,Co>` and name no concrete backend (`s3/mod.rs:1166,1208`); concretes are chosen only at `cli::cmd_s3` (`cli.rs:551-562`). ADR-0010 "concretes in one place" invariant held. |
| T3 Runtime | NEEDS-HUMAN | The patch's own tests pass under isolation, but the full-workspace `cargo test` fails (exit 101, C4-ci) with no named test. Decision owed: **obtain the actual `cargo test --workspace` failure log** and classify it — (a) a regression this patch introduced (e.g. a new test flaky under parallel load such as `concurrent_delete_is_idempotent` / the loopback listener tests), or (b) a pre-existing/flaky failure on the M4 base branch. The accept/iterate decision turns on which. |
| T4 Contribution | PASS | Net-new, load-bearing wire+auth surface exercised at Check (signed round-trip, unsigned refused, streaming behaviourally proven, concurrent DELETE, fragment reclaim, encoded-key identity) — `s3_http_wire.rs:2397-2663`. Not dead scaffolding. |
| T5 Judgment | NEEDS-HUMAN | Pre-declared human calls the builder committed unilaterally and must be ratified: (a) **crate boundary** — chosen `crates/server` over the named `gateway-s3` crate (§5:132; `s3/mod.rs:1060`); (b) **SigV4 scope** — header-only floor, aws-chunked → 501 and no *real* aws-sdk/boto3 interop test (the round-trip still signs with the gateway's own `sign`, `s3_http_wire.rs:2309`), so real-client compatibility is asserted only against the AWS published-example KAT, not a live SDK; (c) **crypto provenance** — now RustCrypto `sha2`/`hmac` (`crypto.rs:907`), claimed to be run through the ADR-0003 three-test audit (recorded in withheld build-notes) — confirm that audit + `deny.toml` allowlist actually landed; (d) **sequencing** (M4 branch vs own sequence) and **error-code floor**; (e) **TLS** modelled but unwired (`TlsIdentity` carried, plaintext loopback at Check) — pre-declared, accepted deferral to #367. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: is a header-only-SigV4, plaintext-loopback, aws-chunked-rejecting object floor **fit** as the first-deployment-gate (#367) S3 surface? A default boto3/aws-sdk PUT may use `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` and hit the 501 path (`s3/mod.rs:1259`); "S3-compatible to a first user" (blueprint:382) may require a real-SDK interop pass before #367 depends on it. Human sign-off at the deployment gate. |

## Notes for the human
- **Blocker is C4, and it is real.** The patch is internally coherent and compiles (per-fix
  red→green passed), but the gating `cargo test --workspace` failure (exit 101) is
  unexplained by the gate output. First action: re-run `cargo test --workspace
  --exclude wyrd-dst` in the target worktree, capture the failing test name, and decide
  regression-vs-base-flake (T3). Do **not** accept while C4-ci is red.
- **Prior art / duplicate work:** iterations v1/v2 on this exact surface were rejected and
  preserved (`iteration-v1/`, `iteration-v2/`); this is the sanctioned re-attempt, not an
  unsurfaced duplicate. #366 (custodian) covers the other half of 0015's process-role
  prerequisite per the 2026-07-04 maintainer decision (brief:35).
- **Security posture looks correct in code:** auth verified before body is read
  (`s3/mod.rs:1221` precedes any body use), constant-time signature compare
  (`crypto.rs:968`), XML error escaping (`s3/mod.rs:1347`), payload-hash check before
  commit (`lib.rs:668`). The residual replay-within-15-min and UNSIGNED-PAYLOAD-on-plaintext
  are pre-declared, TLS-deferral-linked residuals — not new findings.

### Advisory — adversary

# Adversarial review — issue #364 / s3-http-wire-surface (iteration 3)

Skeptic's pass. Inputs: `patch.diff`, `brief.md`, `check-gates.json`; every `path:line`
grounded on the target at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`). Advisory
only — I gate nothing.

## Refutation attempts

- **NEEDS-HUMAN — the recorded gating C4 failure does not reproduce.** `check-gates.json`
  marks `C4-ci` **fail / gating=true** with `path_line` "xtask: `cargo test --workspace
  --exclude wyrd-dst` failed with exit status: 101". On the target that command passes
  cleanly: two full runs → exit 0, 82 test binaries green (including `s3_http_wire.rs`'s
  8 tests, `crates/server/tests/s3_http_wire.rs`); the s3 binary looped 20× with no
  failure; `cargo fmt --check`, `cargo clippy --workspace --exclude wyrd-dst --all-targets`
  and `cargo deny check` (advisories/bans/licenses/sources) all green. So the deterministic
  blocking gate is recorded **red** while the current tree is **green** — either the gate is
  stale (target advanced past gate-time) or a *pre-existing* timing-sensitive test
  (`gateway_lease_expiry.rs` / `gateway_cluster.rs`, not this diff) flaked to exit 101.
  Per the gate rules a deterministic gate recorded red **blocks**; it must be re-run green
  before sign-off. Do **not** treat C4 as satisfied on the strength of the per-fix
  `run-verify.sh` red→green alone (which only exercises the new test, not the workspace).

- **NEEDS-HUMAN — DELETE crash-leak: the "GC backstop" claim is unwarranted (same false-GC
  class the iteration-2 carry-forward asked to fix).** `crates/server/src/lib.rs:222-225`
  and `crates/core/src/metadata.rs:319-324` assert the custodian GC "would eventually
  reclaim any [fragments] the caller misses" after a crash between the metadata commit and
  the eager fragment reclaim. But `delete_object` (`crates/server/src/lib.rs:226-273`)
  reclaims only via `reclaim_fragments` and **never writes an orphan-ledger grace record**
  (no `mark_orphaned`; the `server` crate does not even depend on `custodian`). GC only
  reclaims a fragment that (a) has an orphan-ledger record past grace, or (b) belongs to an
  expired **pending** lease — `crates/custodian/src/gc.rs:146-161`; an unreferenced fragment
  with neither is *"conservatively kept"* (`gc.rs:157-160`). A committed object's fragments
  carry **no** pending entry (released after the PUT commit, `write.rs:274-278`) and DELETE
  writes **no** orphan record, so a crash after the `unlink` commit strands them
  **permanently** — GC never reclaims them. Concrete failing case: PUT an object → DELETE it
  → kill the process between `metadata::unlink` committing and `reclaim_fragments`
  finishing → the object's fragments leak forever. The "no leak" property holds only on the
  happy path, which is the *only* path `delete_reclaims_committed_fragments`
  (`s3_http_wire.rs:366-393`) tests; the crash-safety backstop the comment promises does not
  exist. The comment was reworded from the rejected "GC reclaims" wording but is still
  false.

- **NEEDS-HUMAN — real-SDK interop still absent despite two carry-forwards demanding it.**
  Iteration-1 asked for "ideally a real-SDK interop test"; iteration-2 said "Add a real-SDK
  interop path." The rebuild anchors canonicalization on the AWS published known-answer
  vectors (`crates/server/src/s3/sigv4.rs:557-621`) — a genuine independent oracle for the
  *signing chain* — but adds no boto3/aws-sdk request through the actual axum handler. The
  wire round-trip still signs with the gateway's own `sign()` (`s3_http_wire.rs:87`), which
  shares `canonical_request`/`canonical_query` with `verify`, so the **handler-level**
  path→canonical-URI pass-through (`mod.rs:180-192` feeds `parts.uri.path()` verbatim as the
  canonical URI, no re-encode/normalize) is never exercised against a real client. A real
  SDK that normalizes/encodes its canonical URI differently from the raw request target
  would 403, and no test would catch it. Reviewer should not accept "canonicalization is
  proven" beyond the KAT: the *end-to-end handler path* against an external signer is
  unproven.

- **NEEDS-HUMAN — a default-config recent SDK PUT is rejected 501, so the "S3-compatible
  round-trip" does not hold against an out-of-the-box client.** `verify` classifies any
  `x-amz-content-sha256` beginning `STREAMING-` as `PayloadHash::Streaming`
  (`sigv4.rs:370-376`) and the PUT handler returns `501 NotImplemented`
  (`mod.rs:218-227`). Recent boto3 / aws-sdk versions (2024+ flexible-checksums default)
  send `STREAMING-UNSIGNED-PAYLOAD-TRAILER` / `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` for
  `put_object` **by default** — every such upload gets 501. The binding
  PUT→GET→DELETE round-trip therefore succeeds only with a client specifically configured
  for a single-chunk signed / `UNSIGNED-PAYLOAD` body (exactly what the gateway's own signer
  produces). This is *declared* as real-SDK break 2 (reject-not-misstore), but it means the
  blueprint's day-one "S3-compatible" round-trip does **not** run against a default SDK
  client — a human should explicitly acknowledge this is the accepted M4 floor rather than a
  demonstrated S3 round-trip.

## Attempted but could not refute

- **Streaming is behaviourally demonstrated** (not a buffering impl passing identically):
  `streaming_put_writes_chunks_as_they_arrive_not_after_buffering` (`s3_http_wire.rs:500-561`)
  drives the production `put_object_streaming` with a lazy source + recording store and
  asserts the first fragment lands after ≤2 of 16 pieces are pulled. Legit; addresses
  carry-forward item 1 for PUT (GET side is only structurally bounded via the `channel(4)`
  in `lib.rs:300`, not asserted, but that's minor).
- **Crypto provenance (T5-a)**: `crypto.rs` now wraps RustCrypto `sha2`/`hmac`
  (`crates/server/src/s3/crypto.rs:17-18`); `cargo deny` is green; correctness pinned to
  FIPS-180-4 / RFC 4231 / AWS vectors. The hand-rolled implementation is gone.
- **Concurrent-DELETE idempotency**: the CAS-conflict → re-resolve → success branch
  (`lib.rs:243-252`) is sound; `concurrent_delete_is_idempotent` (`s3_http_wire.rs:324-360`)
  drives the production path over 64 racing rounds and I could not construct an interleaving
  that yields two removals or a 409.
- **percent-decode key identity** (`mod.rs:282-300`) and **XML error escaping**
  (`mod.rs:306-319`) are correct for the cases I tried, including the trailing-`%XX`
  boundary and the SignedHeaders-name injection vector.

Net: the fix's *demonstrated* floor is solid, but (1) a deterministic gate is recorded red
and must be re-run, (2) the DELETE crash-safety claim overreaches its implementation, and
(3) real-SDK compatibility remains asserted rather than proven — all human calls at sign-off.

### Advisory — codex

- `crates/server/src/lib.rs:269` — `delete_object` reclaims committed fragments with `ChunkStore::delete_fragment(frag)`, which routes by the fragment index on placement-aware stores. That leaks fragments whose committed placement record points at a non-index D-server after movement/rebalancing; the rest of the read/write path uses placement-aware addressing (`crates/chunkstore-grpc/src/fanout.rs:143`, `:147`) specifically so moved fragments are found by recorded `dserver`. Reclaim should delete from the placed D-server for each `ChunkRef`, or the trait needs a placement-aware delete counterpart.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — The patch's own tests pass under isolation, but the full-workspace `cargo test` fails (exit 101, C4-ci) with no named test. Decision owed: **obtain the actual `cargo test --workspace` failure log** and classify it — (a) a regression this patch introduced (e.g. a new test flaky under parallel load such as `concurrent_delete_is_idempotent` / the loopback listener tests), or (b) a pre-existing/flaky failure on the M4 base branch. The accept/iterate decision turns on which.
- [ ] T5 Judgment — Pre-declared human calls the builder committed unilaterally and must be ratified: (a) **crate boundary** — chosen `crates/server` over the named `gateway-s3` crate (§5:132; `s3/mod.rs:1060`); (b) **SigV4 scope** — header-only floor, aws-chunked → 501 and no *real* aws-sdk/boto3 interop test (the round-trip still signs with the gateway's own `sign`, `s3_http_wire.rs:2309`), so real-client compatibility is asserted only against the AWS published-example KAT, not a live SDK; (c) **crypto provenance** — now RustCrypto `sha2`/`hmac` (`crypto.rs:907`), claimed to be run through the ADR-0003 three-test audit (recorded in withheld build-notes) — confirm that audit + `deny.toml` allowlist actually landed; (d) **sequencing** (M4 branch vs own sequence) and **error-code floor**; (e) **TLS** modelled but unwired (`TlsIdentity` carried, plaintext loopback at Check) — pre-declared, accepted deferral to #367.
- [ ] Validation — fitness-to-purpose — Decision owed: is a header-only-SigV4, plaintext-loopback, aws-chunked-rejecting object floor **fit** as the first-deployment-gate (#367) S3 surface? A default boto3/aws-sdk PUT may use `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` and hit the 501 path (`s3/mod.rs:1259`); "S3-compatible to a first user" (blueprint:382) may require a real-SDK interop pass before #367 depends on it. Human sign-off at the deployment gate.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: gating C4-ci is recorded red (cargo test --workspace exit 101), and beyond the gate there are two real DELETE correctness defects. Direction for the rebuild: - DELETE crash-leak: delete_object reclaims only via reclaim_fragments and writes NO orphan-ledger grace record (server crate doesn't depend on custodian), so a crash between the unlink commit and reclaim_fragments strands the object's fragments permanently — custodian GC never reclaims them (no orphan record, no pending lease). The "GC backstop" comment is false; make the backstop real (write an orphan/grace record so GC can reclaim) or stop claiming it. - Placement-aware delete: delete_object deletes by fragment index (ChunkStore::delete_fragment), but the read/write path is placement-aware; after fragment movement/rebalance the reclaim targets the wrong D-server and leaks. Delete from the placed D-server per ChunkRef (placement-aware delete counterpart). - C4 gate: re-run cargo test --workspace --exclude wyrd-dst on the current target and localize. The adversary re-ran clean (exit 0, 82 binaries green), so the red is likely stale or a pre-existing timing flake (gateway_lease_expiry.rs / gateway_cluster.rs), NOT this diff — but the recorded gate must be re-run green before any accept; do not lean on the per-fix run-verify green (it only exercises the new test). - Carry-forward judgments to fold in: real-SDK interop is still asserted, not proven (round-trip signs with the gateway's own sign(); a default modern SDK PUT hits the 501 STREAMING path), and the crate-boundary choice (crates/server vs the named gateway-s3 crate) remains a pre-declared human call — surface both again next Check. §6 items: none ticked — cannot accept while the gating gate is red; the open items are the reject basis.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
