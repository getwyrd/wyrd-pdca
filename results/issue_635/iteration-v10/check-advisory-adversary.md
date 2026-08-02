# Adversarial review — issue #635 (segmented-chunk-map), advisory

Method: re-read the diff against the target source at `$PDCA_TARGET`, then attacked the
production API directly from a throwaway crate that path-depends on the patched
`wyrd-core` (built and run, then deleted). Findings below that say "verified" were
executed, not argued.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3224`: this slice's own committer
  mints exactly the record its own backfill declares unrecoverable, and one of them fails
  every backfill pass over the whole store, for ever.** `plan_with`'s write-boundary check
  refuses only a *malformed* placement (`chunk.checked_fragments()` error); an **empty**
  `placement` is *valid* there (ADR-0040 decision 3, the pre-M3 identity spelling), so
  `SegmentedPublication::publish` accepts it. Verified against the patched tree: 1 500
  chunks with `placement: vec![]` ⇒ `plan() -> Ok(3)`, `publish(&store) -> Committed`,
  and the resolve returns them with the placement still empty. That is precisely the
  record `crates/custodian/src/backfill.rs:163-167` collects into `unfillable` and
  `:216-223` converts into `Err(SegmentedPlacementUnfillable)` — raised for the **entire
  pass**, deterministically, on every future run (the diff's own test pins the fatality:
  `crates/custodian/tests/segmented_map_consumers.rs:1107-1109`), with the error text
  itself saying "no other pass drains them". The premise the fatal error rests on is
  stated at `crates/custodian/src/backfill.rs:313` — "structurally impossible today: a
  segmented map is produced only by a multipart Complete, which always records a
  full-length placement" — and the only producer this slice ships falsifies it. Concrete
  failing case: publish any segmented map through the shipped committer with a chunk whose
  `placement` is empty (e.g. a #636 caller that skips `plan.place(topology)`), then run
  `backfill::reconcile` — `Err` on that pass and on every pass after it, with no repair
  path anywhere in the slice. Cheapest fix in Do's hands: refuse an empty placement in
  `plan_with` too (make the "structurally impossible" premise true by construction), or
  make the backfill skip non-fatal for the pass.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2491` claims a bound the resolver
  does not enforce; the real budget is the untrusted root's own claim.** The doc says the
  segmented resolve reads "the bounded per-object range `seg:<nonce>:<epoch>:` (≤
  `MAX_ROOT_SEGMENTS`)", but `read_group_range` (`:2461`) passes
  `accounted.saturating_add(1)` — where `accounted` is the *root's declared*
  `segment_count` — as both the page limit and the materialisation budget, and nothing
  rejects a root naming more than 512 (`:284-288` makes the decode deliberately liberal).
  Verified: a root value of **97 309 bytes** — inside `MAX_VALUE_BYTES` (100 000), i.e. a
  value every backend in play accepts — decodes to `Ok(segment_count = 2000)`, 3.9× the
  documented ceiling; `n = 513` likewise decodes `Ok`. So a corrupt or non-conforming
  generation is resolved with a ~2 000-record budget (≈200 MB at the value ceiling) on the
  GET path and inside every maintenance pass. That is the same "materialise a corrupt
  generation's whole range" shape iteration 9 raised (`metadata.rs:2170` then) moved from
  `SCAN_CAP` onto the root's claim rather than closed. Fix: clamp `accounted` at
  `MAX_ROOT_SEGMENTS` in `read_group_range` (no publication can produce a longer table, so
  a longer one is already unresolvable) — or restate the claim at `:2491` to the bound the
  code actually holds.

- **NEEDS-HUMAN [human] — the §1(vii) containment leg is proven only through GC; the two
  consumers that abort are never run against the damaged object.**
  `crates/custodian/tests/segmented_map_consumers.rs:1261-1270` calls
  `reconcile_step(&zone, &custodian, Some(&gc), None, None, None, …)` — scrub,
  reconstruction and rebalance are `None`, and the drain is set only afterwards at
  `:1287`. But `crates/custodian/src/reconstruction.rs:615` and
  `crates/custodian/src/rebalance.rs:168` call `resolve::homes_of(…)?` with **no**
  `ChunkMapError` containment (contrast `gc.rs:307`, `:323`). Concrete configuration: the
  same damaged object plus *either* one queued repair obligation *or* one requested drain
  ⇒ `reconcile_step` returns `Err` on every cadence (`reconciliation.rs:96`, `:105`), so
  GC's "attribute the blocker and keep going" containment (`gc.rs:292-330`) never gets to
  matter, reconstruction stops repairing under-replicated chunks fleet-wide, and the drain
  the status surface politely reports as `PendingUnresolvable` can never progress — while
  the slice ships no repair path for the object it names. The brief's containment table
  does permit a byte-moving pass to abort, so this is a sign-off judgement, not a spec
  breach: accept that per-object containment covers reads, the id floor and the status
  surface only, or have Do contain the same downcast at those two call sites.

- **NEEDS-HUMAN [human] — the bundle carries no multi-pass rubric review at all this
  round, and the two findings above are in the classes that review kept producing.**
  `check-gates.json` T4 row: "only 0/3 passes produced a usable result after one retry —
  refusing to certify a thinner union". Rounds 5–9 each produced 5–6 blocking findings
  from that same tool (write-boundary strictness, corrupt-generation materialisation,
  containment spelling), and this round's silence is a tool failure, not a clean bill.
  `C4 ci` and `C4-verify` are unaffected. Treat T4 as "not run", not as "nothing found",
  when weighing the bundle at sign-off.

## Attempted and could not refute

- **The red→green evidence.** `crates/custodian/tests/segmented_map_consumers.rs` names
  only base symbols (raw-byte fixtures at `:266-310`, a local `seg_key`, no
  `ChunkMap`/`SegmentRef`/resolver reference, `ReconciliationStatus` compared through
  `Debug` rendering at `:1305-1312`), and its passes are `unwrap`ped (`:645`, `:652`,
  `:682`, `:747`, `:763`), so on the pre-fix tree the red is genuine assertion/panic failures from
  `metadata::decode` rejecting a segmented value — not a build error. It drives the real
  `reconcile_step` / `reconcile_after_restore` / `reconciliation_status` /
  `high_water_marks` / `read_object`, not re-implementations. Leg 2 (`:810-813`) is genuinely
  discriminating: a resolver that decoded the root but skipped the `seg:` range answers
  `Satisfied`.
- **Serialization identity (leg B(i)).** Verified: legacy flat bytes decode→encode
  byte-identically through the new `ChunkMap` enum, so no `require(key, encode(prior))`
  CAS is broken (`metadata.rs:1232-1272`).
- **The id floor's totality and over-approximation.** Verified over a store holding a
  published segmented map plus a `seg:` value truncated mid-`id` token: `high_water_marks`
  returns `Ok` and widens the torn token upward (`98765…` ⇒ `9876599999999999999`), never
  under-reporting; the escaped/padded-digit lexer at `metadata.rs:4380-4530` and
  `widest_id_with_prefix`'s capped walk both check out by hand at the `2^64` boundary
  (prefix `18` ⇒ `18446744073709551615`, prefix `19` ⇒ `1999999999999999999`).
- **Fail-closed on an in-range stray.** Verified: a row the live root does not name yields
  a typed `SegmentUnknown` through `retired_or`, not a torn map.
- **Publication ordering and the resume probe.** `publish` assembles both phases before
  any write (`metadata.rs:3785-3792`), `flip` verifies `DurableRange::WholePlan`
  (`:3745-3747`), and `verify_durable_range` walks every planned index rather than the
  cursor's last record (`:3660-3700`) — the iteration-5/7/8 defects look genuinely closed.
- **Consumer coverage.** Every non-test `.chunk_map` reader in the workspace now goes
  through the shared resolver or is deliberately refused (`unlink` / `commit_chunk_map*`
  raise `SegmentedRetirementUnsupported`, `metadata.rs:2017-2021`, `:2060`).
