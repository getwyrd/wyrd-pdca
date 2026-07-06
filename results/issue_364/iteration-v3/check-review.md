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
