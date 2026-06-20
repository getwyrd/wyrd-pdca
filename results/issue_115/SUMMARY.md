# Result — issue 115 / parallel-any-k-read

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: for an `rs(6,3)` chunk, a GET reconstructs **byte-identically**
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: in `core::read_chunk` (`crates/core/src/read.rs`, the

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

# Check review — issue 115 / parallel-any-k-read (iteration 2)

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json`. `build-notes.md` withheld by design; the planning
artifact and `getwyrd/wyrd@main` source are not on this host, so every basis below
is re-derived from the diff context, the brief, and the gate record — not copied
from the builder's narrative.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Success criterion fully enumerated and testable in `brief.md:21-24` / `:40-48`: byte-identical reconstruction from any `k`, ≤`m` read around, clean typed error below `k`, outstanding fetches cancelled, `EcScheme::None` stays a single fetch, `cargo xtask ci` exits 0. Oracle present (`check-gates.json:8`). |
| C2 — C2 Reproduction (red pre-fix) | PASS | Discriminator is re-derivably red against `main`: the diff *removes* the serial `for index in 0..(k+m) { get_fragment(...).await }` loop (`patch.diff:62-67` removed lines), which can only have one fetch outstanding → `in_flight` peaks at 1. The new oracle `assert_eq!(probe.peak(), N)` (`patch.diff:400-404`, N=9) therefore must fail on `main`. Actual red-run evidence lives in withheld `build-notes.md`; redness itself is provable from the diff. No gate configured (`check-gates.json:15-21`). |
| C3 — C3 Change | PASS | `read.rs` `EcScheme::ReedSolomon` arm replaces the in-order serial fetch with a `FuturesUnordered` fan-out over all `n=k+m` indices, pushing verified shards and `break`ing at `k` (dropping/cancelling the rest); `EcScheme::None` arm untouched (`patch.diff:76-103`). Directly implements any-`k`-arrive-first. |
| C4 — C4 Verification (red→green) | PASS | The single gating gate — `cargo xtask ci` (fmt/clippy/build/test/deny/conformance) — is `pass`, "xtask ci: all checks passed" (`check-gates.json:33-39`). Green side verified by the authoritative gate; the new tests compile and pass. |
| C5 — C5 Causal adequacy | PASS | The fix targets the exact named defect — serial in-index fragment fetch that waits on the slowest `m` (`brief.md:14-19`) — by making the fetch concurrent and reconstructing from the first `k` to verify. Root cause uncontested; iteration-1 carry-forward already accepted the implementation patch as not in question (`brief.md:59`). |
| T1 — T1 Structure | PASS | The brief's named oracle is delivered: `crates/server/tests/dst_read_fanout.rs`, seed-driven via `wyrd_testkit::Sim`, declared in `dst_erasure.rs` style (`patch.diff:117-164`). The networked `read_fanout.rs` is kept as the complementary over-the-wire check the carry-forward explicitly permits (`brief.md:59`). |
| T2 — T2 Shape | PASS | Asserts every spec property: byte-identical any-`k` reconstruction + `peak==n` + `reached==k` (`patch.diff:396-409`); below-`k` surfaces typed `InsufficientFragments{have:5,need:6}` with `peak==n` (`patch.diff:434-446`); `None` stays a single fetch `peak==1`/`reached==1` (`patch.diff:463-473`); run over a 64-seed sweep plus a pinned regression seed (`patch.diff:477-506`). |
| T3 — T3 Runtime | PASS | Tests run green under the C4 gate. Re-derived sound: `pollster::block_on` drives the self-waking `Yield` future cooperatively; on `FuturesUnordered`'s first poll all `n` children enter `get_fragment` and increment `in_flight` before any resolves → `peak==n`; distinct seed-ranked yield counts make the 6 fastest reach the inner store and `break` drop the slow 3 while still `Pending` → `reached==k`. Counts are a pure function of the seed (no wall clock). |
| T4 — T4 Contribution | PASS | `peak==n` is a genuine discriminator (serial peaks at 1, fix at 9); `reached==k` adds the cancellation invariant; the 64-seed sweep adds the arrival-/index-order independence that iteration-1's single hung arrangement lacked (`brief.md:59`). Each assertion earns its place. |
| T5 — T5 Judgment | NEEDS-HUMAN | Iteration-1 was rejected for not exercising the simulation harness — ADR-0009 determinism asserted "only by comment" (`brief.md:59`). v2 uses `wyrd_testkit::Sim` for *seeding only* and drives execution with `pollster::block_on` + a hand-rolled `Yield` (`patch.diff:236-253, 381-411`) rather than a testkit sim executor. Whether driving the read this way satisfies "exercises the simulation harness / determinism-under-simulation" cannot be settled from the provided artifacts (testkit internals withheld); needs human judgment against the rejection rationale. Oracle: reviewer + human sign-off (`check-gates.json:87-93`). |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Whether the reworked tests now deliver the M2.5 tail-latency intent *and* clear the specific iteration-1 rejection (named DST oracle exercising the sim path with order-independence + reproducibility) is a sign-off decision. Oracle: human at sign-off (`check-gates.json:96-102`). |

## §6 — items the human must clear

1. **T5 — DST test vs. the simulation harness.** Confirm whether
   `dst_read_fanout.rs` driving the read with `pollster::block_on` + a custom
   self-waking `Yield` future (using `wyrd_testkit::Sim` only for seeded RNG)
   counts as "exercising the simulation harness" / honoring ADR-0009
   determinism-under-simulation, as the iteration-1 carry-forward demanded
   (`brief.md:59`). If the testkit exposes its own deterministic executor that
   `dst_erasure.rs`-style tests are expected to run the read *under*, using
   `pollster` instead is a deviation; if the testkit's DST contract is just
   "seeded RNG + single-threaded cooperative drive," it is satisfied. This needs
   the testkit source the reviewer was not given.

2. **V — Validation fitness-to-purpose.** Sign-off that the change as a whole
   meets the M2.5 goal and that the reworked tests resolve the iteration-1
   rejection. The fix (C-side) is re-derived PASS and was already accepted in
   iteration 1; the open question is entirely the test rework adjudicated in
   item 1.

## Notes

- C2's red is asserted by re-derivation from the serial loop visible in the diff
  context, not from an observed CI failure (that evidence is in the withheld
  `build-notes.md`). If the human wants a recorded red run, request it.
- No scope ambiguity found: changes are confined to the `read.rs`
  `ReedSolomon` arm plus two new test files and an `async-trait` dev-dependency
  (`patch.diff:106-116`), all within the brief's scope (`brief.md:26-39`) and the
  carry-forward's allowance for a complementary networked test.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 — T5 Judgment — **CLEARED.** The reviewer flagged this only because the testkit
  source was withheld. With the source in hand: `crates/testkit/src/lib.rs` exposes **no
  async executor** (madsim is the "intended" runtime but not yet wired in — see the module
  doc), and the brief's own named style reference `crates/server/tests/dst_erasure.rs`
  drives the read with `pollster::block_on` over a seeded `wyrd_testkit::Sim`. So
  `dst_read_fanout.rs` using `pollster::block_on` + a self-waking `Yield` (with `Sim` for
  seeded RNG) **is** the established DST contract for this codebase, not a deviation. The
  test also closes the exact iteration-1 gap: order-independence across a 64-seed arrival-
  permutation sweep + a pinned regression seed, the `peak==n` concurrency discriminator
  (red on serial `main`, green on the fix), and the `reached==k` cancellation invariant.
- [x] V — Validation — fitness-to-purpose — **ACCEPTED.** The `FuturesUnordered` fan-out
  delivers the M2.5 tail-latency intent (reconstruct from the first `k` to verify, never
  wait on the slow `m`); the test rework directly answers iteration-1's named rejection.
  Re-verified green on the target working tree: `dst_read_fanout` 4/4, networked
  `read_fanout` 2/2, clippy clean on core+server; C4 `cargo xtask ci` recorded pass.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden: **confirmed** — disposition hint was likely-fix.
- Outcome: **ACCEPTED**. Both §6 NEEDS-HUMAN items cleared (T5 settled against the testkit
  source the reviewer lacked; V accepted as fitness-to-purpose). Re-verified green on the
  target working tree.
- Iteration delta (if iterating): none — iteration 2 is final. v2 reworked the tests only
  (the fix was accepted in v1); the named DST oracle `dst_read_fanout.rs` now closes the
  iteration-1 rejection.
- By / date: Eduard Ralph / 2026-06-20 (recorded manually — `pdca` driver bypassed for
  this cycle).

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
