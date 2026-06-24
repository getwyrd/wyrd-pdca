# Check review — issue 205 / dserver-grpc-admission-control

Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`,
`check-gates.json` (build-notes.md deliberately withheld). Every `path:line`
below was re-derived against the target source at
`$PDCA_TARGET=/home/eddie/wyrd/wyrd` (read-only; pre-patch `main` state — the
target does **not** yet contain `AdmissionControl`, so the patch is evaluated as
the proposed change against `main`).

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | One load-bearing, testable success criterion + restored invariant stated in `brief.md:21-47`: shed excess with a retryable status, cut a hung handler by timeout, limit configurable. Unambiguous oracle. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Target `serve()` is a bare `Server::builder()` with no limit/timeout (`crates/server/src/dserver.rs:160`), so pre-fix the excess request queues behind the held slot; the test's bounded `tokio::time::timeout(5s)` + `.expect(...)` (`patch.diff` test `overload_sheds_excess_…`) then elapses → red. Corroborated by gate `C4-verify=pass` (`check-gates.json`). |
| C3 — C3 Change | PASS | Adds `AdmissionControl` + six builder knobs and exposes config via CLI flags and `with_admission_control`; hunks match target context exactly (struct `dserver.rs:75-81`, builder `dserver.rs:160`, bind chain `cli.rs:277-279`, helper `cli.rs:300`) so it applies and remains correct on `main`; builds under `C4-ci=pass`. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json`: `C4-ci` (fmt/clippy/build/test/deny/conformance) = pass and per-fix `C4-verify` red→green = pass. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Fix bounds **per-connection** concurrency (`concurrency_limit_per_connection`), but §8.9's whole-posture "fail closed under pressure" (`docs/design/architecture/08-crosscutting-concepts.md:98-107`), the brief's self-test (`brief.md:44-47`), and §8.9's own "D servers … trust an admitted request" (`…08-crosscutting-concepts.md:96`) make the root cause contested — a human must confirm per-connection shedding restores the server-wide invariant rather than leaving aggregate overload (N connections × limit) unbounded. |
| T1 — T1 Structure | PASS | Tests live in the brief's designated file `crates/server/tests/dserver.rs` (`brief.md:66`), reuse the existing `fs_store()` helper (`crates/server/tests/dserver.rs:36`) and the file's `#[tokio::test]` idiom. |
| T2 — T2 Shape | PASS | The two tests mirror the criterion's two clauses: limit=1 + `load_shed` → excess asserts `ResourceExhausted`/`Unavailable`; timeout=200ms → hung handler asserts `Cancelled`/`DeadlineExceeded` (`patch.diff` tests `overload_sheds_…` / `hung_handler_is_cut_…`), matching `brief.md:21-34`. |
| T3 — T3 Runtime | PASS | `multi_thread` runtime + bounded waits; gate is documented race-free; `status_code()` matches the real `TransportError` variants (`crates/chunkstore-grpc/src/error.rs:18-35`) and the client API exists (`crates/chunkstore-grpc/src/client.rs:31,50`); ran green under `C4-verify` (`check-gates.json`). |
| T4 — T4 Contribution | PASS | Two net-new tests covering both binding behaviours (shed + timeout-cut); genuine new coverage absent at target (`crates/server/tests/dserver.rs` has no admission test), not a tautology. |
| T5 — T5 Judgment | NEEDS-HUMAN | Per gate oracle "reviewer + human sign-off": the pre-fix red is criterion-absence / net-new coverage, not a prior-assertion flip (`brief.md:69-77`), and the shed test rests on a one-client-one-connection assumption (`patch.diff` `overload_sheds_…`, `GrpcChunkStore::connect`) — whether this is a faithful, non-gameable proof needs sign-off. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human (`check-gates.json` oracle "human at sign-off"): does the per-connection posture plus the chosen defaults (64 requests / 30s / 256 streams, `patch.diff` `dserver.rs` new consts) actually fit the operational goal of device-tuned (HDD vs SSD), server-wide fail-closed admission stated in `brief.md:35-47`? |

## §6 — Items the human must clear (NEEDS-HUMAN)

1. **C5 — Causal adequacy / contested root-cause.** The change shins the
   admission limit at the **per-connection** layer
   (`.concurrency_limit_per_connection(...)`). The §8.9 invariant
   (`docs/design/architecture/08-crosscutting-concepts.md:98-107`) and the
   brief's own self-test (`brief.md:44-47`) are over the server's **whole**
   admission posture under overload. Aggregate in-flight work is
   `connections × max_concurrent_requests`, so many connections can still drive
   unbounded total concurrency even though each connection sheds. Compounding
   the contest, §8.9 as written is gateway-centric and states "D servers … stay
   tenant-oblivious, trusting an admitted request"
   (`…08-crosscutting-concepts.md:96`). Human to decide whether per-connection
   shedding is the correct locus and sufficient to restore the named invariant,
   or whether a global concurrency bound is also required.

2. **T5 — Test judgment.** Two points need sign-off: (a) the pre-fix "red" is
   *criterion-absence* (there is no shed status today because there is no limit),
   i.e. net-new coverage rather than the flip of a previously failing assertion
   (`brief.md:69-77`); and (b) `overload_sheds_excess_with_a_retryable_status`
   depends on both requests multiplexing over a **single** client connection so
   the per-connection limit governs them — if `GrpcChunkStore` ever opened a
   second connection the test would silently stop exercising the shed path.
   Human to accept these as a faithful proof of the criterion.

3. **V — Validation, fitness-to-purpose.** Human at sign-off: confirm the
   defaults (`max_concurrent_requests=64`, `request_timeout=30s`,
   `max_concurrent_streams=256`, `load_shed=true`) and the three CLI-exposed
   knobs (`--max-concurrent-requests`, `--request-timeout-secs`,
   `--max-concurrent-streams`) are the right operator-tunable surface for the
   HDD-vs-SSD queue-depth intent (`brief.md:35-47, 50-54`), and that the shed
   status returned to clients is the retryable "busy" signal the system contract
   expects.

## Notes (non-gating)

- The patch's in-comment claims — tonic's `load_shed` doc string ("especially
  useful in combination with setting a concurrency limit per connection") and
  the timeout→`CANCELLED` mapping ("`TimeoutExpired` → `Status::cancelled`") —
  could not be verified from the target source alone (tonic crate source is not
  under `$PDCA_TARGET`, and per scope I did not search other checkouts). Both are
  corroborated indirectly: `tonic` is pinned at **0.14.6** (`Cargo.lock`,
  matching the patch's stated version) and the runtime behaviour the claims
  predict was observed green by the `C4-verify` gate.
- CLI exposes 3 of the 6 knobs; `load_shed`, `tcp_nodelay`, and
  `http2_keepalive_interval` are not CLI-configurable but default fail-closed
  (`load_shed=true`). Acceptable against the brief (the binding knobs — limit,
  timeout, stream cap — are exposed), surfaced here so the omission is not
  silent.
