# Check review — issue 251 / reconstruction-read-around-fragment-read-fault

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json`. `build-notes.md` withheld by design. Citations
re-derived against the target tree at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt`,
read-only).

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | The brief carries a concrete, binding success criterion (`brief.md:18-28`): permanent `EIO`-class fault on a placed fragment ⇒ `Assessment::Repairable` (read-around), transient fault ⇒ no spurious re-placement. The patch targets exactly that surface (`reconstruction::assess`), nothing wider. Spec is well-formed and the change is on-spec. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Flippable test `reads_around_a_permanent_read_fault` (patch.diff:185-259) is genuinely red pre-fix: pre-fix `assess` is `store.get_fragment(frag).await?` (target `reconstruction.rs:257` before patch / brief.md:9), so the injected `EIO` `Err` propagates out of `reconcile_step` and `result.expect(...)` panics. `check-gates.json` C4-verify = pass corroborates the red→green flip. |
| C3 — C3 Change | PASS | Diff is minimal and coherent, touching only `reconstruction.rs` (src+test). Verified byte-for-byte against target: the classifying `match` at `reconstruction.rs:257-261` and helpers at `306-349`. `is_block_read_fault` walks `source()` (`reconstruction.rs:338-349`), mirroring `wyrd_traits::is_integrity_fault` (`traits/src/lib.rs:107-116`). No scope-forbidden file (scrub/read/traits/grpc/on-disk format) touched. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci = pass (fmt/clippy/build/test/deny/conformance) and C4-verify = pass (per-fix red→green), overall = pass. The `e.as_ref()` → `&(dyn Error + 'static)` coercion and the new tests compile and run green per the gate. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | The named defect (the bare `?` aborting the per-chunk drain) is removed for the **local** path, and the fs backend surfaces `EIO` at depth-0 (`chunkstore-fs/src/lib.rs:241`, `Err(e.into())`), so the classifier catches it. DECISION OWED: is that causally adequate for production? The dominant transport for remote D servers is gRPC, and `chunkstore-grpc/src/server.rs:83-88` maps a non-integrity read fault to `Status::internal` (not `DataLoss`); `raw_os_error()==Some(5)` does **not** survive the wire, so `is_block_read_fault` returns false and `assess` propagates a remote dead-sector `EIO` as transient. The brief's own failure scenario — "a disk that goes bad *after* its data lands can never be repaired" (brief.md:14) — is therefore unrestored for networked servers. The human must decide whether removing the abort in `assess` alone satisfies the invariant "along the whole path from the store to the consumer's decision point" (brief.md:38-43), or whether the `EIO`-carry across the gRPC seam is required for this slice vs. deferrable. |
| T1 — T1 Structure | PASS | Tests live in the brief-named file `crates/custodian/tests/reconstruction.rs` (patch.diff:81-353), use `#[tokio::test]`, and factor a shared driver `reads_around_a_permanent_read_fault(make_error)` parameterised over fault shape (patch.diff:185, 263-275). Conventional and well-placed. |
| T2 — T2 Shape | PASS | Assertions are specific and load-bearing, not smoke: `Reconciled::Changed`, drained obligation, `version == 2` (one CAS commit), exact `placement == vec![0,3,2]`, and rebuilt-fragment checksum (patch.diff:231-258). The transient guard asserts the inverse (err propagated, queue still holds `CHUNK`, `version == 1`, placement unchanged — patch.diff:329-352). Shape discriminates the over-broad `.ok().flatten()` regression the brief forbids (brief.md:43-46). |
| T3 — T3 Runtime | PASS | `check-gates.json` C4-ci = pass and C4-verify = pass: the suite (including the three new tests and the `FaultGetStore` mock) actually executed green, not merely compiled. |
| T4 — T4 Contribution | PASS | No inert scaffolding: `FaultGetStore` (patch.diff:96-122) is exercised by all three tests; `wrapped_permanent_eio_fault` (patch.diff:158-160) closes the depth-1 `source()`-walk gap that sank iteration 1, driving the classifier at non-zero depth. The transient test contributes the discriminating guard the brief requires (brief.md:81-86). |
| T5 — T5 Judgment | NEEDS-HUMAN | Brief routes T5 to reviewer + human sign-off, and Do exercised the ILLUSTRATIVE latitude on where to draw the permanent/transient line (brief.md:27-28). DECISION OWED: (a) `EIO` (errno 5) is the *sole* block-layer permanent shape recognised (`reconstruction.rs:310,342`); every other errno class (e.g. `ENODATA`, `EBADMSG`, a `dm-error` target that surfaces a non-5 errno) falls through to "transient → propagate/retry-forever". Is errno-5-only the correct closure of "device cannot return the bytes", or does it under-classify real dead-sector shapes? (b) Test fault shapes are a synthetic depth-0 raw `io::Error` and a depth-1 wrapper; the human must judge whether these are representative of the production fault shapes (especially the gRPC `Status` shape flagged in C5), since a green Check here does not by itself prove the classifier fires on a real remote dead sector. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. DECISION OWED: does a green Check mean *production* networked disks self-heal? The Check evidence is an in-process mock plus a verified fs-backend depth-0 path; it does not cover the gRPC transport, where the `EIO` errno is stripped at `server.rs:87` and the read-around never fires. The human owns whether this slice is fit for purpose as the standalone fix (local/fs path restored, gRPC `EIO`-carry deferred to a follow-up) or whether fitness requires the distinction to survive the wire seam before sign-off. Also confirm the rejected `.ok().flatten()` candidate (brief.md:104) is genuinely absent — it is (the patch classifies). |

## §6 — items the human must clear

1. **(C5 / V) gRPC seam strips the `EIO` signal.** For remote D servers — the dominant
   production transport — a block-layer read fault that is *not* a corruption/integrity
   fault is mapped to `Status::internal` at `crates/chunkstore-grpc/src/server.rs:83-88`,
   not `DataLoss`. The raw `errno 5` does not cross the wire, so `is_block_read_fault`
   (`reconstruction.rs:338-349`) returns false and `assess` propagates the fault as
   transient. Result: the brief's headline scenario — "a disk that goes bad *after* its
   data lands can never be repaired" (brief.md:14) — remains unrestored for networked
   servers. Decide: is the in-process/fs-backend fix sufficient for this slice with the
   gRPC `EIO`-carry deferred, or is the seam carry in scope here? (The fs backend itself
   *is* covered: `chunkstore-fs/src/lib.rs:241` boxes the raw `io::Error` at depth-0 and
   the classifier catches it; the iteration-1 depth-0-only gap is closed by the depth-1
   wrapped fixture.)

2. **(T5) Classifier line: errno-5-only.** `EIO` is the lone recognised block-layer
   permanent shape (`reconstruction.rs:310,342`); all other non-integrity error classes
   default to transient/propagate. Confirm this is the intended closure of
   "device cannot return the bytes" and that no real dead-sector path surfaces a non-5
   errno that would then be retried forever rather than read around.

3. **(V) Fitness-to-purpose / root-cause sign-off.** Confirm that, given items 1–2, the
   change as scoped (abort removed in `assess`, permanent/transient distinction preserved
   per the `scrub.rs:102` precedent) is the accepted fix for this wave, and that the
   deferred privileged `dm-error` scenario (#195) plus any gRPC follow-up are the right
   place for the remaining coverage.

## Notes (advisory, non-gating)

- Precedents cited by the patch all verified on target: `scrub.rs:102` classify-and-continue
  (`Err(e) if is_integrity_fault(e.as_ref())` → repair; else `return Err(e)`),
  `read.rs:189` read-around (`if let Ok(Some(fragment)) = fetched`), and the
  `IntegrityFault` seam contract at `traits/src/lib.rs:64-116`.
- The transient fixture uses `ErrorKind::ConnectionReset` (`raw_os_error()==None`), so it
  correctly fails both `is_integrity_fault` and `is_block_read_fault` and is propagated —
  the discriminating-guard logic is sound.
