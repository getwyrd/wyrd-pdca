# Adversarial review — issue #717 (advisory, non-gating)

Evidence I re-ran myself, in a scratch copy of `$PDCA_TARGET` (cargo 1.96, scratch
`CARGO_TARGET_DIR`, removed afterwards):

- **Green reproduced.** `cargo test -p wyrd-core --test multipart_staging_retire` → 26/26 pass.
- **Red is criterion-absence, as pre-declared.** C4-verify `unverifiable` matches the brief's
  born-at-tier declaration; per issue #236 I do not score that as a refutation.
- **Leg 2's negation is isolating.** Removing `skip_serializing_if` from
  `PendingEntry::owner` (`crates/core/src/metadata.rs:1603-1604`) turned exactly one test red
  (`legacy_pending_entry_re_encodes_byte_identically`), 25 others still green — so that leg
  is load-bearing and not covered twice.
- The test drives the **production** decoders (`metadata::encode`/`decode`, `sidx_key`,
  `retire_key`, `decode_owned_entry`, `decode_retire_obligation`) over hand-authored bytes.
  No parallel re-implementation, no mock; I could not make any accepted leg pass for the
  wrong reason.

## Findings

- **NEEDS-HUMAN [human]** — `crates/core/src/metadata.rs:1618`: `PendingEntryWire` is
  `deny_unknown_fields`, i.e. this patch **closes the live `pending:` record's decode**. The
  brief asked only for the two optional fields, the `Copy` drop and "the torn-shape rejection
  in a manual `Deserialize`" — the closure is an unbriefed behaviour change on the one live
  path in the bundle, and its blast radius is not the record: `expired_pending_chunks`
  (`crates/custodian/src/gc.rs:489`) and `sweep_expired_leases`
  (`crates/core/src/write.rs:641`) both `?`-propagate a decode error out of a `scan("pending:")`
  loop, so **one** undecodable entry stops the whole expired-lease sweep on that node, not just
  that record. The justification at `metadata.rs:1584-1592` weighs only "silent vs loud" and
  never weighs that; the very scenario it invokes ("a field this build does not know") is a
  mixed-version fleet — exactly what this patch itself just created for `PendingEntry` — where
  the chosen behaviour is fleet-wide reclamation stoppage on the older build rather than one
  dropped field. It also buys less than the paragraph claims: I verified that
  `{"lease_expiry_millis":9000,"owner":null}` still decodes (`owner` is a *known* field, `null`
  → `None`) and still re-encodes to `{"lease_expiry_millis":9000}` — the silent durable shape
  rewrite the closure is sold as preventing. Note the repo does close some live shapes
  (`SegmentRecordWire`, `metadata.rs:1222`), so this is a judgement call about *this* record's
  failure mode, not a convention violation — hence a human decision, not an automatic revert.
- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:1582` asserts "…so what a renewal
  stores is what was read". `renew_pending` does not: it decodes `existing` only for the lapse
  check (`metadata.rs:2085`) and puts `encode(entry)` — the **caller's** entry
  (`crates/core/src/write.rs:494-502`) — at `metadata.rs:2090`. The identity property is
  therefore about the *encoder's image* on the legacy shape, not about a renewal echoing what
  it read; the brief corrected this mechanism once already ("it decides what the test must
  assert") and the corrected wording did not fully land. Concretely: `renew_pending(store,
  &[c1, c2], now, &entry)` writes **identical bytes to both chunks' keys**, and both it and
  `live_lease_guards` key on `pending_key(chunk)` (`:2079`, `:2117`), so the neighbouring claim
  that "the same half-TTL renewal loop and the same lease guards serve both"
  (`metadata.rs:1559-1563`) is backed by no code path today and would silently give chunk `c2`
  chunk `c1`'s `staged` placement if #657 reused it unchanged. The same wording ("puts the
  re-encoded entry") is repeated in the test at
  `crates/core/tests/multipart_staging_retire.rs:241-245`. The assertions stay valid; the
  recorded mechanism needs one sentence fixed in each place.
- `crates/core/src/multipart.rs:2624` — `decode_owned_entry` parses and then discards the
  key's part number and chunk id (`let (key_owner, _part_number, _chunk_id) = …`), while its
  sibling `decode_retire_obligation` (`:3016-3038`) returns the token it parsed. The caller
  `staged` exists for — a reaper computing `orphan:<dserver>:<chunk>:<index>`
  (`multipart.rs:2437-2440`) — therefore has to parse the same key a second time. Advisory
  only: the brief fixed the signature, not the return type; not escalated.
- Budget: exactly 12 files as briefed, but ~1174 **semantic** added lines (comments/blanks
  excluded) against the brief's `≤ 960` — the new test is 632 vs the briefed ≈440 and
  `multipart.rs` 485 vs ≈420. Visible in the diffstat; recorded, not escalated.

## Refutations attempted that failed

- **Torn-shape hole via explicit nulls under a `sidx:` reading** — `{"owner":null,"staged":{…}}`
  is still `TornOwnedEntry`, and every null spelling is caught by `require_canonical`
  (`multipart.rs:1706`) on both key-taking decoders.
- **Duplicate JSON keys** (`{"lease_expiry_millis":9000,"lease_expiry_millis":1}`) — rejected
  by serde's duplicate-field error, not last-wins.
- **A stranded obligation symmetric to this round's `Records` fix** —
  `{"Generation":{…,"chunks":[],"segments":{…}}}` *is* refused
  (`NoncanonicalRecordValue`), where `{"Records":{"parts":[],"segments":{…}}}` decodes
  (`multipart.rs:2894-2901`). I expected an inconsistency with the strand rationale at
  `crates/core/tests/multipart_staging_retire.rs:736-742`; it isn't one — `"parts":[]` is
  inside the encoder's image (`Option<PartNumberSet>` keeps `Some(empty)`) and `"chunks":[]`
  is not (`skip_serializing_if = "Vec::is_empty"`), so the canonical gate is self-consistent.
- **An uninhabitable key** — `retire:records:g:<inode>:<version>` accepts no payload at all
  (`Records` → scope mismatch, `Generation` → mode mismatch). Checked against `0016:333-378`:
  every `retire:records:` writer (publication batch, `Completing` rollback) is session-scoped
  and a superseded generation's records ride the bytes-mode generation obligation, so the
  empty combination is correct.
- **An unbounded payload** — `RetirePayload::Parts` is structurally capped by the
  coalesced/strictly-ascending rule (`multipart.rs:2670-2687`) at ⌈`MAX_PART_NUMBER`/2⌉ runs;
  no decode-time cardinality hole beyond what `PartRecord` already tolerates by design.
- **The token scope/suffix matrix** against the `<token>` grammar at `0016:357-378`
  (`Session`/`Parts`/`Records` session-wide, `Chunks` per-part, `Generation` under `g:` only,
  identity checked) — I could not construct a spelling 0016 permits and this decoder refuses,
  or vice versa.
- **The mechanical ripple** — all 8 files are `owner: None, staged: None` initializers only;
  `cargo test -p wyrd-custodian --test gc` passes in the scratch tree (10/10), and the grep for
  `PendingEntry` construction sites finds no 9th one outside the briefed set.
