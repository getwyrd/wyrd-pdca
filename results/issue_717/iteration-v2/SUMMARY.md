# Result — issue 717 / multipart-staging-retire-pending

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the staging and retirement records — the two whose identity lives
  partly **in the key** — do not exist, and `PendingEntry` cannot yet carry ownership.
  This child lands `OwnedEntry`/`StagedPlacement` (`sidx:`, disjoint-staging rule
  `0016:475-491`), `PartNumberSet` and `RetirePayload` (`retire:*`, token grammar
  `0016:357-378`), the two **key-taking decode APIs** (`decode_owned_entry(key, bytes)`,
  `decode_retire_obligation(key, bytes)` — a decode that cannot see the key cannot
  validate against it, which is exactly how v2's shape failed its review — **and note this
  is precisely why a generic dispatching envelope was the wrong shape: a decode that must
  see the key cannot ride a `bytes`-only arm**; the two key-taking decoders here are their
  own entry points, over the base's `metadata::decode`, see **Citations expected**.
  **CORRECTED 2026-08-09 at #715's re-plan:** this field previously also required
  "their `encode_record`/`decode_record` arms" — **there is no such envelope and none is
  coming**, #715's third attempt built one and it was that round's T2 flat FAIL), and the
  ONE `metadata.rs` allowance: `PendingEntry`
  (`metadata.rs:1528`) gains `owner: Option<multipart::UploadId>` and
  `staged: Option<multipart::StagedPlacement>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`), drops `Copy` (forced — `UploadId` is a `String` newtype), and
  gains the torn-shape rejection in a manual `Deserialize`. Salvage the corresponding
  types and the `metadata.rs` hunk from `results/issue_692/iteration-v2/patch.diff`,
  fixing the recorded defects.
- Success criterion: every type this child lands round-trips, and each hand-authored
  torn value below is rejected with a typed error (ADR-0045) — one named negation per leg
  in `build-notes.md`:
  **(1b)** `decode_owned_entry` takes the `sidx:` key and rejects a payload whose `owner`
  differs from the key's upload id (v2 review, `multipart.rs:1555`);
  **(1d)** `decode_retire_obligation` takes the `retire:` key and rejects a payload whose
  mode or generation identity disagrees with the key's token — generation-scoped payload
  under a session token and vice versa are both errors (v2 review,
  `multipart.rs:1789/1800`; the archived test at `multipart_records.rs:1108` *affirmed*
  this case — it must now reject);
  **(1h)** the retire session-token arm honours the token's optional `:<part>:<attempt>`
  suffix (`0016:357-378`): a whole-session `Session`/`Parts` obligation under a per-part
  token, a per-part `Chunks` obligation under a session-wide token, **and — added 2026-08-09
  from a plan-review finding — a `Records` obligation under a per-part token** are **all
  three** rejected (batch-review `multipart.rs:2024`). The third case is not padding: #692's
  batch finding records that the broken arm accepted **every** session-scoped payload
  (`results/issue_692/review-batch.md`), and the suffix exists only for the per-part
  re-upload/compensation obligations (`0016:358-366`) while `retire:records:` is written by
  publication or a Completing rollback (`0016:356`, `:662-665`) — so omitting it leaves the
  recorded defect half-fixed and half-demonstrated;
  **(1n, NEW 2026-08-09 — plan-review finding: binding checks the prior attempt HAD and this
  brief had dropped)** a `RetirePayload::Generation` obligation naming **both** a chunk
  source and a segment source, and one naming **neither**, are both rejected. A generation
  payload is defined over its reclamation evidence (`0016:355`), a structural contradiction
  must fail at decode (`ADR-0045:42-49`), and #692's own review required both branches
  (`results/issue_692/iteration-v1/check-review.md`) with v2 implementing them — dropping
  them from the criterion is how a salvage silently loses a check;
  **(1p, NEW 2026-08-09 — plan-review finding)** a `RetirePayload` carrying a structurally
  invalid nested `ChunkRef` — in the `Chunks` arm **and** in the `Generation` arm — is
  rejected on the same `erasure::supported(k, m)` rule as leg 1i. Do NOT assume this arrives
  from #716: on the base both `EcScheme` and `ChunkRef` still derive an unchecked
  `Deserialize` (`crates/core/src/metadata.rs:99-111`, `:128-140`), and #716 promises
  validation *inside `PartRecord`*, not a globally validating `ChunkRef`. Whatever #716
  landed, this child must assert its own payloads reject;
  **(1i, EcScheme half)** `StagedPlacement`'s `EcScheme` is rejected unless
  `erasure::supported(k, m)` (`erasure.rs:120` — the #285 precedent: untrusted stored
  geometry like `ReedSolomon { k: 0, m: 1 }` is a typed error, not a panic);
  **(1e)** a `PendingEntry` with exactly one of `owner`/`staged` is torn and rejected at
  decode under both a `pending:` and a `sidx:` reading (v2 review,
  `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
  valid **shapes**. **Scope of 1e, narrowed deliberately (plan-advisory finding,
  2026-08-09):** this is a *shape* check on the value alone, NOT a key/namespace check. A
  both-present (owned-shaped) value stored under a `pending:` key still decodes, because the
  live `pending:` readers decode bytes **without the key**
  (`metadata.rs:1973-1980`, `:2011-2016`) and GC would reclaim an expired one it finds there
  (`crates/custodian/src/gc.rs:488-492`). Making that a rejection means teaching the
  existing `pending:` read path to be key-aware — a change to live readers and their
  callers, well outside a pure-record slice, and the wrong thing to smuggle in. **DEFERRED and
  named — but "owned by #657" is this Plan's EXPECTATION, not a verified contract (softened
  2026-08-09, plan-review finding).** What the target actually says is that the first writers
  are somewhere in #656–#659 (`crates/core/src/multipart.rs:55-64`); no #657 scope document
  was available at this Plan to confirm the assignment, and this bundle has no `notes.json`
  to check it against. So: the deferral is real and the reasoning above stands, but sign-off
  should treat the *owner* as provisional and confirm the obligation is actually recorded
  against whichever slice first writes a `sidx:` record — an unowned deferral is how a known
  gap becomes a forgotten one. Do MUST NOT widen 1e into the `pending:` readers; a torn *shape* is
  rejected everywhere, a misfiled *namespace* is not this child's to catch. **What "rejected with a typed error" means for THIS leg, narrowed
  2026-08-09 (plan-review finding — as written the criterion was not achievable in scope):**
  the rejection is raised by `PendingEntry`'s own `Deserialize`, and the *observable this
  child owns* is that `metadata::decode::<PendingEntry>` returns `Err` on a torn shape and
  `Ok` otherwise. It is **not** a promise that the live `pending:` readers surface an
  ADR-0045 `MetadataValidationError`: they call the generic `metadata::decode`
  (`metadata.rs:1979`, `:2015`), which boxes a serde failure (`:1540-1543`;
  `crates/traits/src/lib.rs:68-74`), and converting that to a typed validation error means
  changing live readers and their callers — squarely out of this slice's scope, and not to be
  smuggled in. Assert the decode boundary; do not assert the reader's error type.
  **(leg 2)** decode→encode is the identity on a legacy value: a legacy `pending:` value
  with neither new field re-encodes **byte-identically**. **The MECHANISM was wrong here and
  is corrected 2026-08-09 (plan-review finding) — the leg stands, the reason changes, and the
  difference matters because it decides what the test must assert.** The `pending:` path does
  **not** use `require(key, encode(prior))`: `renew_pending` preconditions on the **raw bytes
  it read** and then puts the re-encoded entry — `batch.require(key, current).put(key,
  encode(entry))` (`crates/core/src/metadata.rs:1984`), and `live_lease_guards` pushes the
  same raw bytes (`:2011-2020`). So a non-identity re-encode does **not** turn those CASes
  into permanent `Conflict`s as this brief previously claimed; it lets the CAS **succeed**
  and silently rewrite the record's shape — dropping a field on the `put`, durably, with no
  error anywhere. That is the worse failure and it is the one to assert against.
  `require(key, encode(prior))` IS the shape on the `inode:` path (`metadata.rs:1766`,
  `:1891`; ADR-0047:38-50), which is where that rule comes from — do not transplant it.
  **`0016:475-485` mis-describes the current code on exactly this point** (it says
  `renew_pending`/`live_lease_guards` compare the re-encoded prior, with stale line
  references): trust the code, not that paragraph;
  **(leg 3 corollary, binding — stated ONLY as what this child can observe)** a `sidx:`
  value whose `staged` placement length does not match its scheme's fragment count
  **decodes successfully**. That is the whole binding claim: it is a **decode-boundary**
  assertion, provable by this child's pure test. Placement length is the standing
  *contextual*-check example (`ADR-0045:69-74`, `AGENTS.md:146-149`, `0016:416-429`), so it
  is deliberately not a decode error, and leg 1i validates the scheme's **geometry**, never
  the placement's **length**. **The brief previously also asserted such a value is
  "quarantined by GC" — that claim is WITHDRAWN from the criterion** (plan-advisory
  finding, 2026-08-09): custodian source is out of scope here, GC today scans only
  `pending:` (`crates/custodian/src/gc.rs:482-496`), and the first production `sidx:`
  writer is #657 — so nothing this child ships could demonstrate quarantine. It is the
  downstream consumers' to prove, not a leg of this test;
  **(leg 4, docs currency — SETTLED YES)** `PendingEntry` gains two persisted fields, so
  this child adds the short paragraph to
  `docs/design/architecture/05-building-block-view.md` § "The metadata model" (`:183`),
  mirroring the ADR-0047 optional-inode-fields bullet at `:186-192` (`AGENTS.md:154-158`:
  a merge requirement, not a follow-up). **CORRECTED 2026-08-09 at #715's re-plan — this is
  now an EXTENSION, not a first paragraph:** #715 and #716 both land beneath this child and
  both now carry the same leg, so that section will ALREADY describe the multipart record
  set by the time this builds. Add only what this child introduces — `PendingEntry`'s two
  optional ownership fields and the disjoint `sidx:`/`retire:` namespaces — and do not
  restate what is there. **Read the section on the merged base before writing; do not trust
  this brief's account of what the siblings left.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: extend `crates/core/src/multipart.rs` with
  the staging/retirement types and both key-taking decoders (no dispatch arms — the envelope
  they were arms of was withdrawn at #715's re-plan, 2026-08-09); the
  `PendingEntry` hunk in `crates/core/src/metadata.rs` described in **Defect** above.
  **TWO hunks in that file, not one — corrected 2026-08-09 from a plan-advisory finding
  that would otherwise have stalled Do:** besides the struct definition at `:1528`,
  `metadata.rs` **constructs a `PendingEntry` in its own test module at `:3374`**
  (`&PendingEntry { lease_expiry_millis }`). Adding two fields breaks that struct literal,
  so it MUST gain the same mechanical `owner: None, staged: None` initializer as the 8
  external ripple sites. The previous wording said "nothing else in that file changes" AND
  told Do to STOP on finding a ninth construction site — i.e. the brief forbade the edit
  the compiler requires. That 9th site is **in-file and expected**; it does not trip the
  STOP rule. Nothing else in `metadata.rs` changes. Then the **explicitly ALLOWED
  mechanical ripple** in the 8 files
  that construct or copy `PendingEntry` (`crates/core/src/write.rs`,
  `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs`) — `owner: None, staged: None` initializer lines
  / clone-instead-of-copy fixes only, **≤ 8 changed lines per file, no logic change, no
  new function**; a ninth ripple file or a non-mechanical hunk means the seam is wrong:
  STOP and hand back. Plus the ONE docs paragraph of leg 4. Keep every hunk in the shared
  files as small as briefed — a wider hunk is a needless rebase surface for #710/#711
  whichever folds first. Budget ≈ 960 added lines / 12 files. / out of scope: custodian
  **source** code (`crates/custodian/src/` untouched — only its tests' initializers);
  every `docs/design/` file except the one `05-building-block-view.md` paragraph — ADRs,
  proposals and specs untouched (INTEGRATION §2 immutability); the outcome enums, answer
  table, digests, `sha2` (#693 — no `Cargo.toml`/`Cargo.lock` change); knob values
  (#655); store round trips (#656–#659); reaper/windows (#625).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 71 mutants tested in 2m: 50 caught, 21 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #717: add key-validated multipart staging and retirement records while extending `PendingEntry` with ownership metadata without changing legacy serialization.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit and independently grounded in structural-at-decode policy and the `sidx:`/`retire:` record contracts (`docs/design/adr/0045-metadata-validation-boundaries.md:42`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:346`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept criterion-absence as adequate red evidence — base `c824243` with only the new test failed on the imports and fields at `crates/core/tests/multipart_staging_retire.rs:53`, so this is a compile-red rather than an isolating behavioral pre-fix witness. |
| C3 Change | PASS | The patch implements the two key-taking decode boundaries and the shared-record validation in the named surfaces, with the key/value decisions located at `crates/core/src/multipart.rs:2623`, `crates/core/src/multipart.rs:3016`, and `crates/core/src/metadata.rs:1627`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the born-at-tier compile-red as sufficient red→green — patched tests passed 26/26 from `crates/core/tests/multipart_staging_retire.rs:207`, all independently runnable repository gates passed, and 71 in-diff mutants yielded 50 caught plus 21 compiler-unviable, but no behavioral pre-fix run exists. |
| C5 Causal adequacy | PASS | The change removes the missing validation boundary directly rather than adding a capability probe or runtime symptom guard, and the owner/token relations are enforced where both key and value are present (`crates/core/src/multipart.rs:2638`, `crates/core/src/multipart.rs:2935`). |
| T1 Structure | NEEDS-HUMAN | Decide whether to accept the expanded review surface or return to Plan — the exact 12-file boundary is respected, but 1,174 added nonblank/noncomment lines exceed the brief's 960-semantic-line cap and increase audit/rebase risk. |
| T2 Shape | PASS | The design keeps separate key-taking entry points and no generic bytes-only envelope, preserving the key-determined record shape required by `crates/core/src/multipart.rs:17` and `crates/core/src/multipart.rs:23`. |
| T3 Runtime | PASS | No new store/runtime path is introduced, while the one live `pending:` path retains omitted optional fields and byte-identical legacy re-encoding (`crates/core/src/metadata.rs:1573`, `crates/core/tests/multipart_staging_retire.rs:247`). |
| T4 Contribution | NEEDS-HUMAN | Re-run or resolve the unavailable batched-review blocker and confirm closed/rejected prior art by every affected path — its command/output and rejected-work refs are absent from the artifact sandbox, although merged history has no new-type symbol hit. |
| T5 Judgment | PASS | The exercised cases cover the specified key disagreement, torn shape, token suffix, generation-source, nested-scheme, and contextual-placement decisions (`crates/core/tests/multipart_staging_retire.rs:381`, `crates/core/tests/multipart_staging_retire.rs:620`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether these pure record contracts are fit for the later writers and drains before sign-off — this slice deliberately has no production `sidx:`/`retire:` consumer, so end-to-end operational fitness cannot yet be demonstrated (`crates/core/src/multipart.rs:69`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept criterion-absence as adequate red evidence — base `c824243` with only the new test failed on the imports and fields at `crates/core/tests/multipart_staging_retire.rs:53`, so this is a compile-red rather than an isolating behavioral pre-fix witness.
- [x] C4 Verification (red→green) — Accept the born-at-tier compile-red as sufficient red→green — patched tests passed 26/26 from `crates/core/tests/multipart_staging_retire.rs:207`, all independently runnable repository gates passed, and 71 in-diff mutants yielded 50 caught plus 21 compiler-unviable, but no behavioral pre-fix run exists.
- [ ] T1 Structure — Decide whether to accept the expanded review surface or return to Plan — the exact 12-file boundary is respected, but 1,174 added nonblank/noncomment lines exceed the brief's 960-semantic-line cap and increase audit/rebase risk.
- [x] T4 Contribution — Re-run or resolve the unavailable batched-review blocker and confirm closed/rejected prior art by every affected path — its command/output and rejected-work refs are absent from the artifact sandbox, although merged history has no new-type symbol hit.
- [x] Validation — fitness-to-purpose — Decide whether these pure record contracts are fit for the later writers and drains before sign-off — this slice deliberately has no production `sidx:`/`retire:` consumer, so end-to-end operational fitness cannot yet be demonstrated (`crates/core/src/multipart.rs:69`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- [x] The key/value invariant is still asymmetric. The brief says a decoder that cannot see the key cannot validate against it (`brief.md:6-8`), but it adds key-taking decoders only for `sidx:` and `retire:` and calls both-present `owner`/`staged` a valid owned shape (`brief.md:33-36`). The target design says both fields are `Some` only on `sidx:` and both are `None` on `pending:` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:442-457`), while the live pending readers decode bytes without the key (`crates/core/src/metadata.rs:1973-1980`, `:2011-2016`) and GC will reclaim any expired both-present value found under `pending:` (`crates/custodian/src/gc.rs:488-492`). The brief needs either a pending-key-aware rejection and its resulting source scope, or a narrower invariant plus an explicit deferral; as written it repeats the reviewed “decoder cannot see the key” defect on the existing namespace.
- [x] The placement-length criterion promises an outcome this child cannot observe: it says the mismatched `sidx:` placement is “quarantined by GC” (`brief.md:41-45`), but its asserted leg proves only that decoding succeeds, custodian source is expressly out of scope (`brief.md:92-94`), and the first production `sidx:` writer is deferred to #657 (`brief.md:112-118`). The target GC currently scans only `pending:` (`crates/custodian/src/gc.rs:482-496`). Revise this to the verifiable decode-boundary claim alone, or bring the staged-reference/quarantine wiring and an observable test into scope.
- [ ] The “ONE `metadata.rs` hunk — nothing else in that file changes” scope (`brief.md:79-82`) omits an existing in-file `PendingEntry` constructor at `crates/core/src/metadata.rs:3369-3377`; adding two required fields makes that constructor fail to compile unless a second, distant hunk is changed. The scope/hunk and line budgets must explicitly allow this ninth constructor site (within the already-counted substantive file).
- [x] The base-state claim contradicts the resolved prerequisite state. The brief says it builds on child 2's **merged** result (`brief.md:75`) and asks for citations on the merged base (`brief.md:119-127`), but `dependency-state.json:2-5` says #716 exists only in `PLANNED` state. The resolved target confirms that its record values and `encode_record`/`decode_record` are still “the next child's” (`crates/core/src/multipart.rs:5-12`) and contains no such definitions. Revise the execution precondition/target so this bundle waits for or materializes #716 instead of claiming that prerequisite is already merged.
- [x] The tracker support for the root-cause and ignored-prior-attempt claims is unavailable: this advisory bundle has no `notes.json` or `sources/`, while the brief requires salvaging and correcting `results/issue_692/iteration-v2/patch.diff` (`brief.md:13-15`, `:138-142`), a path also absent from the resolved target. The planner needs to supply the cited tracker/review evidence (or quote its load-bearing lines in the brief) so the asserted v2 failures and constraints can be checked rather than taken on trust. — RESOLVED at sign-off: `results/issue_692/iteration-v2/patch.diff` and its review artifacts (`review-batch.md`, `check-advisory-adversary.md`) are present on this device (sibling bundle, not inside `issue_717/`) and corroborate the brief's "generic dispatching envelope" defect narrative.
- [ ] size backstop — this slice is behaving oversized: patch is 103 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C3 Change — Decide whether to re-enter Plan for the size overrun — `patch.diff` adds 1,780 raw lines and at least 1,131 nonblank/noncomment lines against the brief's ≤960-semantic-line budget, materially increasing review and rebase surface.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rationale: the T4 batched-review gate correctly failed on a real, unaddressed correctness gap — `RetirePayload::Records { segments }` is accepted without cross-checking the segment group's epoch against the retirement token's session epoch, so a misfiled retirement obligation could reference (and a drain could delete) a different completion attempt's segment generation. This is a scoped fix inside `checked_against_token`/`checked_shape`, not a re-slice. Fold in two more small, same-file fixes while iterating: 1. Correct the `renew_pending` doc comment at `metadata.rs:1582` (and its echo in `multipart_staging_retire.rs`): it claims a renewal stores what it read; the code actually re-encodes the caller's entry. The test assertions are still correct — only the stated mechanism is wrong. 2. Change `decode_owned_entry`'s return type to include the parsed `part_number`/`chunk_id` (mirroring `decode_retire_obligation`'s `(token, payload)` shape), so a future caller isn't forced to re-parse the `sidx:` key a second time. Do NOT attempt the `PendingEntryWire deny_unknown_fields` blast-radius question (silent field-drop vs. whole-sweep failure on an unrecognized field) in this iteration — that's deferred to a separate follow-up tracker issue (milestone 0.1 Alpha, see SUMMARY.md §10), since a proper fix (warn-and-continue per record in the scan loop) requires touching live readers (`gc.rs`, `write.rs`) that this bundle's brief explicitly places out of scope. Scope overrun (T1 Structure / size backstop / C3 Change) is explicitly set aside for this round per human direction — not part of this iteration's ask. The base-state precondition (#716 merged) is confirmed resolved; #692 tracker-evidence and the two design-scope items (key/value asymmetry on `pending:`, placement-length/GC-quarantine wording) are accepted as correctly scoped/deferred by the brief and need no further action.
- By / date: Eduard Ralph / 2026-08-12

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- Follow-up issue needed (milestone 0.1 Alpha): decide `pending:`/`sidx:` unknown-field decode policy — warn-and-continue per record in the expiry-sweep scan, rather than silent field-drop (prior behavior) or whole-sweep failure on one bad record (this bundle's `PendingEntryWire deny_unknown_fields`); this touches live readers (`gc.rs`, `write.rs`) explicitly out of this bundle's scope. Note: this subject has been touched by a prior issue — cross-reference before filing.
- Harness/process bug (file upstream, eduralph/pdca-harness): the reviewer/artifact-check tooling scopes its evidence search to the current bundle's own directory only, so a brief's citation of a sibling bundle's artifact (here, `results/issue_692/iteration-v2/patch.diff`) was flagged as "unavailable" even though it exists on-device in the sibling `results/issue_692/` tree — the check doesn't know where cross-referenced result bundles live. Sign-off had to manually locate and verify it.
- (empty is the common case)
