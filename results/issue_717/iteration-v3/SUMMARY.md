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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 72 mutants tested in 2m: 51 caught, 21 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #717's multipart staging and retirement record types, key-aware decoders, and backward-compatible `PendingEntry` ownership extension.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is precise and decidable at the metadata-validation layer established by `docs/design/adr/0045-metadata-validation-boundaries.md:42`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether criterion-absence compilation failure is an adequate red witness — the base plus `crates/core/tests/multipart_staging_retire.rs:55` failed on absent APIs and fields, but did not exhibit the forbidden behavior. |
| C3 Change | PASS | The change stays within the settled codec-record boundary and makes key/value identity and structural invariants decidable at `crates/core/src/multipart.rs:2663` and `crates/core/src/multipart.rs:3065`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether compile-only red followed by 27 focused passes and a complete CI pass is sufficient closure — all declared tools were exercised, but the red cannot demonstrate behavioral causality (`crates/core/tests/multipart_staging_retire.rs:270`). |
| C5 Causal adequacy | PASS | The missing key-aware decoding and record validation are addressed directly, without a capability probe or runtime guard, at `crates/core/src/multipart.rs:2663` and `crates/core/src/multipart.rs:3065`. |
| T1 Structure | PASS | The exact 12-file surface keeps substantive work in the codec, tests, and architecture documentation, with the legacy extension localized at `crates/core/src/metadata.rs:1554`. |
| T2 Shape | PASS | Dedicated typed views and key-taking entry points preserve the intended record boundaries rather than introducing a generic envelope (`crates/core/src/multipart.rs:2635`, `crates/core/src/multipart.rs:3065`). |
| T3 Runtime | PASS | Full workspace and 50-seed DST reruns passed, while legacy absence and byte identity are exercised at `crates/core/tests/multipart_staging_retire.rs:260`. |
| T4 Contribution | NEEDS-HUMAN | Resolve the two blockers reported by the unavailable `scripts/review-branch` batch report before contribution sign-off — the supplied artifacts expose only their count, so their paths and impact could not be independently grounded. |
| T5 Judgment | PASS | Deep source/test review and 72 in-diff mutation attempts found no independent defect; the cross-record failure cases are exercised from `crates/core/tests/multipart_staging_retire.rs:405`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether pure codec evidence is fit for this pre-writer slice — production writers and retirement draining are intentionally absent, so end-to-end reclamation is not demonstrated (`crates/core/src/multipart.rs:69`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether criterion-absence compilation failure is an adequate red witness — the base plus `crates/core/tests/multipart_staging_retire.rs:55` failed on absent APIs and fields, but did not exhibit the forbidden behavior.
- [ ] C4 Verification (red→green) — Decide whether compile-only red followed by 27 focused passes and a complete CI pass is sufficient closure — all declared tools were exercised, but the red cannot demonstrate behavioral causality (`crates/core/tests/multipart_staging_retire.rs:270`).
- [ ] T4 Contribution — Resolve the two blockers reported by the unavailable `scripts/review-branch` batch report before contribution sign-off — the supplied artifacts expose only their count, so their paths and impact could not be independently grounded.
- [ ] Validation — fitness-to-purpose — Decide whether pure codec evidence is fit for this pre-writer slice — production writers and retirement draining are intentionally absent, so end-to-end reclamation is not demonstrated (`crates/core/src/multipart.rs:69`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- [ ] The key/value invariant is still asymmetric. The brief says a decoder that cannot see the key cannot validate against it (`brief.md:6-8`), but it adds key-taking decoders only for `sidx:` and `retire:` and calls both-present `owner`/`staged` a valid owned shape (`brief.md:33-36`). The target design says both fields are `Some` only on `sidx:` and both are `None` on `pending:` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:442-457`), while the live pending readers decode bytes without the key (`crates/core/src/metadata.rs:1973-1980`, `:2011-2016`) and GC will reclaim any expired both-present value found under `pending:` (`crates/custodian/src/gc.rs:488-492`). The brief needs either a pending-key-aware rejection and its resulting source scope, or a narrower invariant plus an explicit deferral; as written it repeats the reviewed “decoder cannot see the key” defect on the existing namespace.
- [ ] The placement-length criterion promises an outcome this child cannot observe: it says the mismatched `sidx:` placement is “quarantined by GC” (`brief.md:41-45`), but its asserted leg proves only that decoding succeeds, custodian source is expressly out of scope (`brief.md:92-94`), and the first production `sidx:` writer is deferred to #657 (`brief.md:112-118`). The target GC currently scans only `pending:` (`crates/custodian/src/gc.rs:482-496`). Revise this to the verifiable decode-boundary claim alone, or bring the staged-reference/quarantine wiring and an observable test into scope.
- [ ] The “ONE `metadata.rs` hunk — nothing else in that file changes” scope (`brief.md:79-82`) omits an existing in-file `PendingEntry` constructor at `crates/core/src/metadata.rs:3369-3377`; adding two required fields makes that constructor fail to compile unless a second, distant hunk is changed. The scope/hunk and line budgets must explicitly allow this ninth constructor site (within the already-counted substantive file).
- [ ] The base-state claim contradicts the resolved prerequisite state. The brief says it builds on child 2's **merged** result (`brief.md:75`) and asks for citations on the merged base (`brief.md:119-127`), but `dependency-state.json:2-5` says #716 exists only in `PLANNED` state. The resolved target confirms that its record values and `encode_record`/`decode_record` are still “the next child's” (`crates/core/src/multipart.rs:5-12`) and contains no such definitions. Revise the execution precondition/target so this bundle waits for or materializes #716 instead of claiming that prerequisite is already merged.
- [ ] The tracker support for the root-cause and ignored-prior-attempt claims is unavailable: this advisory bundle has no `notes.json` or `sources/`, while the brief requires salvaging and correcting `results/issue_692/iteration-v2/patch.diff` (`brief.md:13-15`, `:138-142`), a path also absent from the resolved target. The planner needs to supply the cited tracker/review evidence (or quote its load-bearing lines in the brief) so the asserted v2 failures and constraints can be checked rather than taken on trust.
- [ ] size backstop — this slice is behaving oversized: patch is 121 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Not converging: this is the slice's own recommendation (size backstop — patch 121KB vs 100KB threshold, 2 rounds already spent) plus a gating T4 batch-review FAIL and multiple scope-validity §6 items (unresolved #716 base-state contradiction, missing tracker citations for salvaged v2 material, in-file constructor-site scope gap). The adversarial review also surfaces a record-format decision (RetirePayload Session/Parts cannot express the combined shape 0016 requires) that affects sibling children and belongs in Plan, not another Do pass. Re-plan and split rather than iterate-do: a slice this size will keep yielding implementation-shaped findings round after round without converging.
- By / date: Eduard Ralph / 2026-08-12

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
