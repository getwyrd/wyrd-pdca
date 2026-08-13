# Adversarial review — issue #717 (advisory, non-gating)

Attacked the evidence first (re-ran the suite at `$PDCA_TARGET`: 27/27 green), then ran **five
independent negations** of my own in a throwaway copy, then attacked the fix against proposal
0016 and the target's own rubric. Three findings below; the evidence itself survived.

## Findings

- **NEEDS-HUMAN [human] — `RetirePayload`'s `Session`/`Parts` split cannot express the one
  bytes-mode payload 0016's own batch table requires, and the token grammar leaves no second
  key to put it in.** `crates/core/src/multipart.rs:2821` (`Session {}`) and `:2824`
  (`Parts { parts }`) are **mutually exclusive** enum arms, while the proposal names
  `{session, parts}` as a *single* obligation in four places — the reaper's
  `Completing@E → Aborting@E+1` row (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`,
  pseudo-code `:2193` `put retire:bytes:{session, parts}`, `{session, all}` at `:2187`), the
  restore fence (`:823`), and X57 (`:2587`) — and the §1 value column lists it as one of the
  three bytes-mode shapes (`:355`, and again for records mode at `:356`). Concrete failing case
  for the first writer (#656–#659): that fence must install **one** `retire:bytes:` obligation
  carrying both halves, but its only available key is
  `retire:bytes:s:<upload-id>:<epoch>` — the per-part suffix is now *rejected* for both
  shapes by leg 1h (`multipart.rs:3009-3011`) — so installing `Session {}` and `Parts {…}`
  separately collides on one key under `require_absent(retire:<mode>:<token>)` (`0016:369-373`),
  and the writer is left spelling one obligation under epoch `E` and the other under `E+1`,
  which is a token whose fence did not install it. The diff argues only why `Session {}`
  carries no *frozen* list (`multipart.rs:2817-2820`) — a good argument that does not address
  the combined payload at all. This is a stored-format shape being frozen ahead of its writers:
  either the arm set needs a combined shape, or the diff needs to record why drain-time
  enumeration of the fenced `part:<id>:` range subsumes 0016's `parts` component. A human call,
  because it decides a record format the two sibling children build on.

- **NEEDS-HUMAN [impl] — the leg-1r "attempt-scope" cross-check is claimed to close the F18
  class, but it constrains only the epoch integer; a foreign session's segment generation
  decodes under your token.** `crates/core/src/multipart.rs:3020-3031` compares
  `token.epoch − group.epoch() ∈ {0,1}` and nothing else, while the variant doc at
  `crates/core/src/multipart.rs:386-390` and the test doc at
  `crates/core/tests/multipart_staging_retire.rs:651-655` both claim it prevents "deleting a
  *different* attempt's `seg:` records … the F18 class the epoch-scoped key space exists to make
  impossible". Concrete case, run against the patched tree:
  `decode_retire_obligation(retire_key(Records, s:1a…1a:3), {"Records":{"segments":{"nonce":"bb…bb","epoch":3}}})`
  returns **`Ok`** — session B's group nonce filed under session A's token. Epochs are small
  per-session integers, so an inter-session collision is the *normal* case, not an exotic one,
  and that is precisely the drain-deletes-a-published-map outcome the doc claims to foreclose.
  The code cannot do better (the group nonce is deliberately independent of the upload id,
  `0016:354`, `:508-509`, iteration-14 finding 2) — so it is the **claim** that must be narrowed
  to "the epoch component only, the group identity is the writer's/drain's", in the variant doc,
  the `checked_against_token` bullet (`multipart.rs:2965-2971`) and the test's leg-1r header.
  Builder-fixable in one iteration; left as-is, sign-off is being asked to accept a guarantee
  the record does not carry.

- **NEEDS-HUMAN [impl] — the torn-`PendingEntry` rule has no writer-side guard outside
  `wyrd-core`, contradicting this diff's own argument for making `checked_shape` public.**
  `crates/core/src/metadata.rs:1599-1613` keeps `owner`/`staged` **public** fields (it must —
  `write.rs:207` etc. build the literal) while the rule lives in
  `crates/core/src/multipart.rs:2441`, which is `pub(crate)`. Concrete case, run against the
  patched tree: `metadata::encode(&PendingEntry { lease_expiry_millis: 9000, owner: Some(id),
  staged: None })` yields `{"lease_expiry_millis":9000,"owner":"1a…"}` — bytes that
  **nothing can read back**: `metadata::decode::<PendingEntry>` errs and `decode_owned_entry`
  errs with `TornOwnedEntry`. That is exactly the "a producer could store an obligation its own
  drain would then refuse to decode forever" hazard the diff cites at
  `crates/core/src/multipart.rs:2910-2913` as the reason `RetirePayload::checked_shape` is
  `pub` — and #657's `sidx:` writer lives in another crate. Fix is small (export the pairing
  check, or a `PendingEntry` constructor / `OwnedEntry::to_pending`-only path recorded as such).

## Attacked and could not refute

- **The red→green evidence is real, not a pre-declared excuse.** C4-verify is `unverifiable`
  by design (born-at-tier); I substituted my own negations in a scratch copy of the tree and
  each one made **exactly one** test fail, i.e. every rule I probed isolates:
  (1) `checked_ownership_pairing` forced to `Ok` → only `torn_pending_entry_is_rejected_under_both_readings`
  fails; (2) dropping `skip_serializing_if` on `owner` (`metadata.rs:1606`) → only
  `legacy_pending_entry_re_encodes_byte_identically` fails (leg 2 is load-bearing);
  (3) widening the epoch rule to `Some(0|1|2)` → only the leg-1r test fails; (4) deleting the
  `(Records, Some(_))` suffix arm (`multipart.rs:3009`) → only
  `records_obligation_is_rejected_under_a_per_part_token` fails, and it fails with `Ok(...)`,
  i.e. the #692 recorded defect is genuinely re-demonstrated; (5) adding a placement-length
  rejection to `StagedPlacement::new` → only the leg-3-corollary test fails. The tests exercise
  the production entry points (`decode_owned_entry`/`decode_retire_obligation`/`metadata::decode`),
  not a parallel re-implementation, and the C5 mutants run (51 caught / 21 unviable / **0
  missed**) agrees.
- **Over-strictness of the new decode rules** — I walked every writer 0016 defines
  (`:662-665`, `:823`, `:2187-2196`) against `checked_against_token`: every documented
  installer produces a token epoch of `E` or `E+1` relative to the segment generation, so the
  rule rejects no legitimate obligation I could construct.
- **`Generation`'s XOR of chunk/segment sources** — checked against `ChunkMap`
  (`crates/core/src/metadata.rs:1014-1020`, `Flat` XOR `Segmented`); no hybrid generation is
  representable, so leg 1n rejects nothing real.
- `PartNumberSet::from_runs` arithmetic (`multipart.rs:2713-2729`): `previous_hi + 1` cannot
  overflow because both endpoints pass `PartNumber::new` (≤ `MAX_PART_NUMBER`) first; same for
  `from_numbers`' `last.1 + 1`.
- Explicit `null`s for the two new optional fields (`{"lease_expiry_millis":9000,"owner":null,
  "staged":null}`) decode and re-encode to the omitted spelling — not a defect, since
  `renew_pending` puts the *caller's* freshly encoded entry either way
  (`crates/core/src/metadata.rs:2093`) and the `sidx:` seam's canonical gate refuses it.
- Not re-raised, per `AGENTS.md:200-203`: the `PendingEntryWire` `deny_unknown_fields`
  blast-radius question (deferred to a tracked follow-up at iteration 2) and the size/scope
  overrun (explicitly set aside by human direction for this round).
