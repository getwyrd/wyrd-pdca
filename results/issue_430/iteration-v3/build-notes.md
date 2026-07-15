# Build notes — issue 430 / fragment-identity-validation (iteration 3)

## What this iteration changes vs. iteration 2

Iteration 2's **production fix** was accepted on the merits and is carried forward
unchanged — it is the smallest change that restores the brief's Invariant (a fragment is
admitted into any read/repair/maintenance path only when its decoded header proves the FULL
identity requested: `chunk_id`, `ec_fragment_index`, and, for RS, an EC tuple consistent
with the committed `ChunkRef.scheme`). The v2 rationale for that shape is preserved in
`iteration-v2/build-notes.md`; I do not restate it here.

The auto-iterate to round 3 carried **one actionable, code-level** finding (the rest were
Check-side environmental gaps — the artifact reviewer cannot stash/revert, and this host
blocks loopback binds so `cargo xtask ci` stops at `list_delete_over_grpc`, both outside the
patch). The actionable finding, from the round-2 adversary
(`iteration-v2/check-advisory-adversary.md:8-22`):

> The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating it to accept any
> `k`/`m` (keeping only the `ec_scheme_type == ReedSolomon` check) survives the ENTIRE
> suite, including the new `fragment_identity.rs`. Cause: the "EC tuple disagrees" case uses
> a `None`-type header, so only the `ec_scheme_type` mismatch ever goes red; the `k`/`m`
> conjuncts are never exercised. The brief explicitly demanded the tuple case cover
> "`ec_k`/`ec_m`/scheme type" (brief.md:41-42).

This is a real hole in the Success criterion's proof: the brief says "likewise one whose
header EC tuple disagrees" and "cover BOTH cases: a wrong `ec_fragment_index` AND a header
EC tuple (`ec_k`/`ec_m`/scheme type) disagreeing" (brief.md:40-42). Case 2's `None`-type
header trips the `ec_scheme_type` check first, so the geometry compare (`ec_k`/`ec_m`) was
proven by no test — a future regression of it would be invisible.

## The fix for the gap — a same-SCHEME-TYPE, wrong-GEOMETRY case

The scheme-type mismatch and the geometry mismatch are two distinct rejection reasons in
one predicate (`crates/core/src/repair.rs:74-79` on the patched tree):

```rust
EcScheme::ReedSolomon { k, m } => {
    header.ec_scheme_type == EcSchemeType::ReedSolomon   // case 2 (None header) trips this
        && header.ec_k == k                              // NOW exercised by case 3
        && header.ec_m == m
}
```

To bind the `ec_k`/`ec_m` conjuncts I added a case whose header's scheme TYPE **matches**
(both `ReedSolomon`) and whose `ec_fragment_index` **matches** (0), so ONLY the geometry
compare can reject it: an **RS(3,1)** header against a committed **RS(2,1)** chunk.

Two additions, no production change beyond the accepted v2 patch:

1. **Integration case 3** — `ec_read_rejects_a_same_scheme_type_wrong_geometry_fragment`
   (`crates/core/tests/fragment_identity.rs:322`). Same deterministic-red shape as cases 1
   and 2 (serve exactly `k = 2` fragments, ONE wrong-identity; the RS fan-out stops at `k`
   so the decoder is forced to consume the wrong shard pre-fix). Slot 0 stamps
   `encode_ec_fragment(chunk_id, 0, 3, 1, &shards[1])` — correct chunk id, correct index 0,
   RS scheme type, but geometry RS(3,1) carrying index 1's bytes. Pre-fix it is admitted on
   `chunk_id` alone → decoder returns wrong bytes, no enqueue (red). Post-fix it is rejected
   on `ec_k` (3 ≠ 2) → 1 < k survivors → typed error + enqueue (green). Drives the PUBLIC
   surface (`read::read_object` + `repair::queued_repairs`), so a production revert fails it
   by ASSERTION, not compile error (brief.md:83-86).

2. **Unit test** — extended
   `intact_shard_accepts_the_expected_fragment_and_rejects_wrong_identity`
   (`crates/core/src/repair.rs:205-220`) with a negative + positive pair over an RS(3,1)
   header: `intact_shard(rs31, at0, RS(2,1)) == None` (wrong geometry rejected) and
   `intact_shard(rs31, at0, RS(3,1)) == Some(payload)` (matching geometry admitted). The
   positive half proves it is the `k`/`m` compare — not the scheme-type alone — that gates
   admission, so the mutant that drops the compare cannot pass by making everything reject.
   Added `EcSchemeType` to the test module's `use` (`crates/core/src/repair.rs:153`).

## Mutation experiment (run this iteration, killing the round-2 survivor)

I re-ran the exact mutant the round-2 adversary reported: replace the RS arm body with
`{ let _ = (k, m); header.ec_scheme_type == EcSchemeType::ReedSolomon }` (drop the
`ec_k`/`ec_m` compare), keeping everything else.

- Before this iteration (v2 tests only): mutant **survives** — reported by the adversary,
  and consistent with case 2 using a `None`-type header.
- After this iteration: mutant **dies** — `fragment_identity` case 3 FAILS by assertion
  ("read returned WRONG bytes: a shard whose stripe geometry disagrees with the scheme was
  decoded", `fragment_identity.rs:373`) and the unit test FAILS ("an RS header with the
  WRONG stripe geometry (k/m) — same scheme type — is rejected"). Cases 1 and 2 still pass
  under the mutant (they don't target geometry), confirming case 3 is what binds it.

Reverting the mutant → all green again. Transcript captured below.

## Refutation (forced self-check — three questions)

- **(a) Genuine red?** YES. With the two production files reverted
  (`git stash push -- crates/core/src/read.rs crates/core/src/repair.rs`, keeping the test
  files), `cargo test -p wyrd-core --test fragment_identity` fails **all three** by
  assertion — the read returns index-1's shard bytes at both data positions instead of the
  true `[0,1,2,…]` (case 3 panics at `fragment_identity.rs:373`, cases 1/2 at `:204`/`:289`).
  Re-applying the fix → all three pass. (`fragment_identity.rs` uses only the public
  `read::read_object` / `repair::queued_repairs`, so it compiles against reverted core —
  the red is by ASSERTION, not compile error, exactly as brief.md:83-86 requires.)
- **(b) Production path?** YES. The tests call `read::read_object`, the real read entry, and
  the wrong fragment is served through the `ChunkStore` trait exactly as an adversarial
  backend would. Case 3 exercises the same production gate `header_matches_identity` at the
  RS fan-out decode site (`crates/core/src/read.rs:331`) — no copy/mock/re-implementation.
  The unit test calls the real `repair::intact_shard`.
- **(c) Fixture includes the fault?** YES. The wrong-geometry fragment is actually stored in
  the served set (slot 0), and only `k = 2` fragments are served with one of them wrong, so
  the decoder is forced to consider it, not curated around it.

## Verification run (project cargo toolchain, in `$PDCA_WORKTREE`)

Resolved via the same `engine/lib/ensure-cargo.sh` the project's `engine/xtask.sh` gate
runner uses (the runner delegates to `cargo xtask ci`, which cannot complete on this host —
it stops at an unrelated loopback-bind `PermissionDenied` in `list_delete_over_grpc`,
`docs/INTEGRATION.md` env; this is the same host limitation the round-2 carry-forward noted,
outside the patch). Focused legs, all bounded by the runner's timeout:

- RED leg (both production files reverted, tests kept): `fragment_identity` → **3 failed by
  assertion**.
- GREEN leg (fix applied): `fragment_identity` → **3 passed**; `repair::tests` unit → passed.
- Mutation experiment (k/m compare dropped): case 3 + unit → **failed** (mutant killed);
  reverted → green.
- `cargo fmt --all -- --check` → clean (commit-hook gate).
- `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets -- -D warnings` → clean.

The C4 `cargo xtask ci` gate re-runs the real whole-tree suite at Check; the host's
loopback-bind limitation for the full gate is a Check-side environment note, not a defect in
this patch (no external dependency beyond the toolchain was needed — brief: External
dependencies = none, so no NEEDS-HUMAN external-dependency marker is raised).
