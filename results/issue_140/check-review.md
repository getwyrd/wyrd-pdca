# Check review — issue 140 / m3.2-chunkstore-list-delete

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld). Citations re-derived
> against the target tree at `$PDCA_TARGET=/home/eddie/wyrd/wyrd` (read-only),
> which carries the applied patch.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Brief fixes the binding contract: exact signatures `list_fragments(&self) -> Result<Vec<FragmentId>>` / `delete_fragment(&self, id) -> Result<()>`, additive proto evolution, and a concrete success criterion (`brief.md:20-23`, `:28-36`). Verification posture (net-new, criterion-absence red) is stated (`brief.md:41-48`). |
| C2 — C2 Reproduction (red pre-fix) | PASS | Net-new posture: red = criterion-absence. `crates/chunkstore-grpc/tests/list_delete.rs` references `list_fragments`/`delete_fragment` (e.g. `list_delete.rs:332`, `:403`) which do not exist on the trait pre-patch, so the test cannot compile against the unfixed tree. `C4-verify` gate independently confirms "red without the fix" (`check-gates.json` rule `C4-verify`). |
| C3 — C3 Change | PASS | Affordances landed on every required surface: trait (`crates/traits/src/lib.rs:88,96`), proto service (`crates/proto/proto/wyrd/v0/chunk.proto:86-87`), client (`crates/chunkstore-grpc/src/client.rs:78-106`), D-server service (`crates/chunkstore-grpc/src/server.rs:81-111`), fs backend (`crates/chunkstore-fs/src/lib.rs:99-146`), and fanout (`crates/chunkstore-grpc/src/fanout.rs`). Matches brief Scope. |
| C4 — C4 Verification (red→green) | PASS | `cargo xtask ci` (fmt/clippy/build/test/deny/conformance) green and gating (`check-gates.json` rule `C4-ci`); per-fix `run-verify.sh` shows red pre-fix → green post-fix (rule `C4-verify`). `cargo-deny` green inside CI satisfies the brief's new-dependency guard (`brief.md:85-87`). |
| C5 — C5 Causal adequacy | PASS | Root cause is uncontested and mechanical: the affordance simply did not exist (`brief.md:57-64`). The change supplies exactly the missing methods, and the test demonstrates the seam is load-bearing — `get_fragment` returns `Some` before delete and `Ok(None)` after, siblings unaffected (`list_delete.rs:395-415`). No competing causal hypothesis. |
| T1 — T1 Structure | PASS | Primary test at the brief-specified path `crates/chunkstore-grpc/tests/list_delete.rs` (new), with supplementary fs-walk coverage added to `crates/chunkstore-fs/tests/conformance.rs:101-158`, as the brief directs (`brief.md:37-40`). Mock `ChunkStore` impls updated where the trait gained required methods (core/dst/server test files) — mandatory for compilation, not scope creep. |
| T2 — T2 Shape | PASS | Mirrors `round_trip.rs`: a shared `list_and_delete_round_trip` body exercised over both an in-process `FsChunkStore` and a local-tonic gRPC client (`list_delete.rs:429-441`), with set-equality assertions since order is unspecified (`list_delete.rs:330-334`). Exercises real HTTP/2 + prost (de)serialization of the additive messages. |
| T3 — T3 Runtime | PASS | The test compiles and runs green: `cargo xtask ci` (which builds and runs the test suite) passed and is gating (`check-gates.json` rule `C4-ci`). |
| T4 — T4 Contribution | PASS | Assertions are load-bearing and would catch regressions: exact-set listing, bytes-present-before / `Ok(None)`-after delete, sibling non-interference, idempotent absent-delete (`list_delete.rs:388-426`), plus fs-specific skipping of `.tmp`/foreign entries (`conformance.rs:138-158`). Not a tautological/smoke test. |
| T5 — T5 Judgment | PASS | Test-design judgment is sound: covers the empty store, non-zero EC index, idempotency edge, and the crash-mid-write phantom case; uses set equality rather than over-asserting order. No misjudged scope or hollow coverage observed. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human: whether these bytes-level affordances genuinely serve M3's scrub (enumerate-and-diff) and GC (reclaim) consumers — the slices that *call* them are deliberately out of scope (`brief.md:33-36`) — is a fitness-to-purpose call only the human at sign-off can clear. STOP discipline applies: draft only until sign-off (`brief.md:96-100`). |

## §6 — Items the human must clear

1. **(V) Validation fitness-to-purpose.** Confirm the added `list_fragments` /
   `delete_fragment` seam adequately serves the M3 maintenance consumers (scrub,
   GC) it is built for, given those consumers are intentionally deferred to later
   slices (#141+). Re-confirm the `DeleteFragment`-on-missing-id semantics choice
   (idempotent `Ok(())`, per the brief's Do-call latitude, `brief.md:93-94`) is
   acceptable, and that the additive proto evolution is the intended one-version-gap
   interop posture (ADR-0002 / §8.7).

## Reviewer notes (advisory, non-gating)

- **Additive proto evolution verified by re-derivation.** Existing `FragmentId`
  (fields 1–2) and all prior messages/rpcs are unchanged; the new
  `FragmentList*` / `FragmentDelete*` messages and the two appended rpcs reuse
  `FragmentId` without repurposing any field number (`chunk.proto:20-23`,
  `:46-60`, `:86-87`). Consistent with the ADR-0002 wire rule the brief binds.
- **fs walk is the inverse of `fragment_path`** and strict: `parse_chunk_dir_name`
  requires exactly 32 hex digits and `parse_fragment_file_name` requires a `.frag`
  suffix with a `u16` stem (`lib.rs:205-217`), so a `.tmp` from an interrupted put
  or any foreign entry is skipped — no phantom fragments. A never-written store
  returns an empty walk rather than erroring (`lib.rs:106-111`).
- **No new dependency surfaced** (cargo-deny green within the gating CI), so the
  brief's NEEDS-HUMAN dependency trigger (`brief.md:85-87`) is not raised.
- Scope held: no change to `put`/`get`/`health` behaviour, no on-disk-format or
  `format_version` change, and the consuming GC/scrub loops remain out of scope —
  matching `brief.md:33-36`.
