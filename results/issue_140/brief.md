# Design proposal — issue 140 / m3.2-chunkstore-list-delete

> Implementation slice of the **already-accepted** proposal 0005 (Milestone 3 —
> custodians), PR-sequence step 2. The normative design lives in
> `docs/design/proposals/accepted/0005-milestone-3-custodians.md` §"`ChunkStore`:
> enumerate + delete" — an Accepted proposal, immutable (INTEGRATION §2). This brief
> points at 0005 and scopes the one slice for Do; it does not re-decide the design.

- **Slug:** m3.2-chunkstore-list-delete
- **Kind:** enhancement (design proposal — implements accepted proposal 0005, step 2)
- **Goal:** Add the two `ChunkStore` affordances M1/M2 deliberately left out — a store
  can be **walked** (`list_fragments`, scrub needs it) and a fragment can be **deleted**
  (`delete_fragment`, GC needs it) — on the trait, the gRPC `ChunkStore` service, and
  both backends, keeping the D server deliberately dumb.
- **Success criterion:** With the methods present on `ChunkStore`, a store can be
  enumerated and a fragment's bytes deleted **over real tonic and in-process**:
  `list_fragments` returns exactly the `FragmentId`s a store holds, and after
  `delete_fragment(id)` a subsequent `get_fragment(id)` returns `Ok(None)` while other
  fragments are unaffected — demonstrable at C4-verify against the in-process / local-tonic
  harness. (The two method names + signatures `list_fragments(&self) -> Result<Vec<FragmentId>>`
  and `delete_fragment(&self, id) -> Result<()>`, and the **additive** proto service
  evolution — fields never repurposed — are BINDING per 0005 / ADR-0002. The fs backend
  realising `list` as a directory walk is ILLUSTRATIVE.)
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2)
- **Depends on:** 139
- **Ordering note:** no data dependency (0005 marks #140 parallel with #139), but the PR-sequence order 139 → 140 → 141 is kept, so this waits until #139 is COMPLETE. No file overlap with #139's `core`/`fanout` edits (touches `traits` / `proto` / both backends), so no `Conflicts with`.
- **Surfaces:** data   (trait + RPC + backends; no GUI)
- **Scope:** add `list_fragments(&self) -> Result<Vec<FragmentId>>` and
  `delete_fragment(&self, id: FragmentId) -> Result<()>` to the `ChunkStore` trait
  (`crates/traits/src/lib.rs`), the additive `ListFragments` / `DeleteFragment` rpcs on
  the gRPC `ChunkStore` service (`proto` + `chunkstore-grpc` client + D-server service),
  and both backends (`chunkstore-fs` directory walk; `chunkstore-grpc`). / out of scope:
  the GC loop that *calls* `delete_fragment` and the scrub loop that *calls*
  `list_fragments` (custodian slices, #141 onward); promoting `sweep_expired_leases` from
  a test helper to a running loop; any change to `put_fragment` / `get_fragment` / `health`
  behaviour; any `format_version` / on-disk-format change.
- **Test file:** `crates/chunkstore-grpc/tests/list_delete.rs` (new) — round-trips
  `list_fragments` + `delete_fragment` over in-process and local-tonic, mirroring the
  existing `round_trip.rs` shape. (Backend `chunkstore-fs` walk coverage in
  `crates/chunkstore-fs/tests/conformance.rs` is supplementary.)
- **Verification posture:** NET-NEW coverage (template posture (a)) — the methods do not
  exist on the trait, so "red" is **criterion-absence** (a pre-fix test referencing them
  does not compile), not a flipped assertion. Do should land the test so it is **green
  post-implementation over in-process tonic** (Check-observable, no Docker) and, where
  feasible, demonstrate the seam is load-bearing (e.g. assert `get_fragment` still returns
  the bytes *before* delete, `Ok(None)` *after*). The **real-network** (docker-compose)
  variant in `crates/chunkstore-grpc/tests/tier2_integration.rs` is observable only off-Check
  (needs a Docker host) — confirmed by `cargo xtask ci` / Tier-2 CI, supplementary evidence.
- **Citations expected:** Do must cite path:line on `origin/main` for every change
  (`crates/traits/src/lib.rs:72-84` the trait; proto `ChunkStore` service; both backends).
- **Prior-art check (triage cycles):** searched merged history and all PRs by file path
  (`crates/traits/src/lib.rs`, `crates/chunkstore-grpc/`, `crates/chunkstore-fs/`) — the
  trait was last evolved to fragment-addressed (`f428ec7`); **no** `list_fragments` /
  `delete_fragment` work merged, open, or closed. Net-new.
- **Disposition hint:** new-feature

## Motivation
The `ChunkStore` trait is `put_fragment` / `get_fragment` / `health` only
(`crates/traits/src/lib.rs:72-84`) — a store cannot be *walked* and a fragment cannot be
*deleted*. The two M3 maintenance loops need exactly those: **scrub** must enumerate what
a D server actually holds to diff it against the chunk map, and **GC** must reclaim bytes —
today the test-invoked ledger sweep (`core::sweep_expired_leases`) deletes *ledger entries*
but **no fragment bytes**, because the affordance does not exist. This slice adds the
affordances; the loops that consume them are later slices.

## Design
Per 0005 §"`ChunkStore`: enumerate + delete" (authoritative):
- `list_fragments` enumerates what a store holds (scrub diff against the chunk map);
  networked store gains a new `ChunkStore` gRPC rpc, `chunkstore-fs` does a directory walk.
- `delete_fragment` lets GC reclaim bytes; the networked store gains the matching rpc and
  the D server, staying **deliberately dumb** (§8.5), simply removes the bytes it is told to.
- Both land **additively** in `proto` (the `ChunkStore` service), `chunkstore-grpc`
  (client + D-server service), and `chunkstore-fs` — a one-version-gap-compatible service
  evolution (§8.7 / ADR-0002 wire rule), **not** a `format_version` or trait-contract break
  in the sense of repurposing existing fields.

## Alternatives considered
Settled in 0005 and not reopened: a separate "maintenance" trait vs. extending `ChunkStore`
(0005 extends the existing trait — the D server stays one dumb service); making the D server
interpret fragments for GC (rejected — it stays dumb, §8.5). 0005 is Accepted; any change
to it requires a superseding proposal (INTEGRATION §2).

## Impact & compatibility
Existing round-trip `put` / `get` / `health` must be **unaffected**. Proto evolution is
additive and must stay one-version-gap interop (§8.7); `cargo-deny` must be green (no new
dependency expected, but confirm if the proto/tonic surface pulls one — a new dep is a
NEEDS-HUMAN per INTEGRATION §4 / ADR-0003). On-disk format unchanged.

## Open questions
- `list_fragments` over a large store: streaming vs. a single `Vec` — 0005 specifies the
  `Vec` signature for M3; keep it unless a sign-off item argues otherwise (out of scope to
  redesign here).
- Confirm `DeleteFragment` semantics on a missing id (idempotent `Ok(())` vs. error) — a Do
  call; pick idempotent unless a gate disagrees, and note it in build-notes.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: PR #186 conflicts with M3.1 (#185, now merged) on chunkstore-fs/src/lib.rs + chunkstore-grpc/src/fanout.rs — it was built off pre-M3.1 main. Re-Do off current main (which now carries the placement record) for a clean rebuild.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
