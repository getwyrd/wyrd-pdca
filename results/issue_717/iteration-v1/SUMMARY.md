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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 70 mutants tested in 2m: 49 caught, 21 unviable

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

Task under review: add key-aware multipart staging/retirement record codecs and extend `PendingEntry` with ownership and placement while preserving legacy serialization.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the key/value identity, structural-decode, legacy-byte-identity, scope, and dependency decisions explicit and ties them to the target's decode-boundary rule at `docs/design/adr/0045-metadata-validation-boundaries.md:42`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the born-at-tier compile failure as adequate red evidence — base `c824243` with only `crates/core/tests/multipart_staging_retire.rs:48` retained failed on the absent APIs and fields, but this is criterion absence rather than an isolating behavioral red. |
| C3 Change | NEEDS-HUMAN | Decide whether to re-enter Plan for the size overrun — `patch.diff` adds 1,780 raw lines and at least 1,131 nonblank/noncomment lines against the brief's ≤960-semantic-line budget, materially increasing review and rebase surface. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the recorded full gate may stand despite incomplete independent reproduction — the focused test passed 24/24 and mutation testing reproduced 49 caught/21 unviable, but six existing server/custodian tests stalled beyond five minutes and two cargo-deny advisory checks could not lock the host's read-only database. |
| C5 Causal adequacy | PASS | The change removes the causal ambiguity at the boundary: both identity-bearing records decode with their keys at `crates/core/src/multipart.rs:2618` and `crates/core/src/multipart.rs:3004`, with no capability probe or in-path fallback guard. |
| T1 Structure | PASS | The patch uses exactly the briefed 12 files: four substantive surfaces anchored at `crates/core/src/metadata.rs:1588`, `crates/core/src/multipart.rs:2387`, `crates/core/tests/multipart_staging_retire.rs:1`, and `docs/design/architecture/05-building-block-view.md:204`, plus the eight mechanical initializer ripples. |
| T2 Shape | PASS | The public shape is the required pair of key-taking entry points rather than a bytes-only envelope, and structural versus contextual placement checks remain separated at `crates/core/src/multipart.rs:2442`. |
| T3 Runtime | PASS | The only live-path compatibility risk is covered: the legacy value re-encodes byte-identically at `crates/core/tests/multipart_staging_retire.rs:244`, while both-present owned values retain their fields at `crates/core/tests/multipart_staging_retire.rs:265`. |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected prior art by every affected path before sign-off — merged refs have no `-S` hit for the four new type names, but the supplied artifacts cannot mechanically establish the closed/rejected-work half of that check. |
| T5 Judgment | NEEDS-HUMAN [impl] | Remove or substantiate the GC-quarantine claim — the test only proves decode success at `crates/core/tests/multipart_staging_retire.rs:468`, while current GC scans only `pending:` at `crates/custodian/src/gc.rs:488` and cannot observe `sidx:`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm these pre-writer record shapes and the provisional namespace-deferral owner fit the downstream multipart lifecycle — the architecture states the writers/consumers have not landed yet at `docs/design/architecture/05-building-block-view.md:202`. |

### Advisory — adversary

# Adversarial review — issue #717 (`multipart-staging-retire-pending`)

Advisory only; nothing here gates. Evidence was re-run at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt`, patch applied, working tree dirty over `c824243`).
Toolchain was available: `cargo test -p wyrd-core --test multipart_staging_retire` →
**24 passed, 0 failed**. The pre-declared UNVERIFIABLE C4-verify (RED reverts production and
the new test fails to *compile*) is not refutable and I did not score it; `C5-mutants`
(70 tested, 49 caught, 21 unviable, **0 missed**) independently answers "would each leg have
gone red", so I spent my attempts on holes mutation cannot see: rules that are *absent*, and
claims the comments make that the code does not keep. Concrete probes were run out of a
throwaway crate under `$PDCA_SCRATCH` linking `wyrd-core` by path (removed).

## Findings

- **NEEDS-HUMAN [human]** — `crates/core/src/multipart.rs:2765` (`pub enum RetirePayload`) and
  `crates/core/src/metadata.rs:1596`/`:1601` (`pub owner` / `pub staged`): every new invariant
  this child lands is enforced **only on the decode side**, and the public shapes will happily
  mint — and `metadata::encode` will happily serialize — records their own decoders then refuse
  forever. Verified, not hypothesised:
  `encode(&RetirePayload::Generation { inode: 42, version: 5, chunks: vec![c], segments: Some(g) })`
  → `{"Generation":{…"chunks":[…],"segments":{…}}}` → `decode_retire_obligation` =
  `RetireGenerationSourcesConflict`; `RetirePayload::Parts { parts: PartNumberSet::default() }`
  (note `Default` is derived at `:2658`) → `{"Parts":{"parts":[]}}` →
  `RetireObligationOwesNothing`; `RetirePayload::Chunks` with an `rs(0,1)` `ChunkRef` →
  `ChunkSchemeUnsupported`; `PendingEntry { lease_expiry_millis: 5, owner: Some(id), staged: None }`
  → `{"lease_expiry_millis":5,"owner":"…"}` → `TornOwnedEntry`. Why this is not cosmetic for
  *these two* classes specifically: a `retire:` obligation is installed with `require_absent`
  and the drain "must not guess" (`0016:358-372`), so an obligation stored through a public
  variant is reclamation evidence **nothing can ever read back** — the permanent-loss outcome
  the token grammar exists to prevent; and a torn `PendingEntry` landing under `pending:` makes
  `expired_pending_chunks` (`crates/custodian/src/gc.rs:489`, `metadata::decode(&value)?` inside
  the `scan("pending:")` loop) fail the **entire** GC sweep — precisely the "error that aborts a
  whole reconcile step" this patch's own doc argues against at `crates/core/src/multipart.rs:2449`.
  This is the module's own standing pattern being broken *inside this diff*: every sibling record
  carrying a cross-field rule keeps private fields and accessors (`AdmissionRecord`
  `multipart.rs:1604`, `SessionRecord` `:1902`, `SlotRecord` `:2053`, `PartRecord` `:2264`), as do
  both **structs** this patch adds (`StagedPlacement:2452`, `OwnedEntry`) — `RetirePayload` and
  `PendingEntry`'s two new fields are the only outliers. Routed to a human rather than to Do
  because Rust enum variant fields cannot be made private: closing it means a type redesign
  (private inner enum + validating constructors, or a newtype) that changes the API #656–#659 will
  write through, and `PendingEntry`'s public literal is the very shape the brief's 8-file
  mechanical ripple depends on. A decision is needed on whether decode-only enforcement is
  accepted here and recorded against the first writer, or closed now.

- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:1580-1583` and
  `crates/core/src/multipart.rs:2614-2615`: the justification given for `skip_serializing_if` on
  `staged` is refuted by the very line it cites. The comments claim decode→encode identity holds
  "on an owned `sidx:` entry across its own renewals … so the stored bytes are exactly what was
  read", and that "an owned lease is renewed in flight by re-encoding the entry it read
  (`renew_pending`, `metadata.rs:2079`)". `renew_pending`
  (`crates/core/src/metadata.rs:2057-2081`) does neither: it addresses `pending_key(chunk)` (`:2071`)
  and can never reach a `sidx:` key, and at `:2079` it writes `encode(entry)` — the **caller's**
  single `&PendingEntry`, applied to every chunk in the slice — while the decoded prior
  (`existing`, `:2074`) is read only for the expiry test. So no `owner`/`staged` value can survive a
  renewal by byte-identity today, and under this signature a future owned-renewal caller would
  necessarily write one chunk's `staged` placement onto every other chunk in the batch. The legacy
  half of the argument (`:1578-1580`) is correct and leg 2 stands; only the owned half is
  unwarranted. Fix is a reword (the wiring itself is #657's), and the brief's own warning —
  "`0016:475-485` mis-describes the current code on exactly this point … trust the code, not that
  paragraph" — is the reason not to re-import 0016's framing verbatim.

- **NEEDS-HUMAN [impl]** — `crates/core/src/multipart.rs:2508`: "the accepted sets are pinned equal
  by the test file's S1/S2 agreement helper" is false, and the same test file proves it. Probe:
  `{"lease_expiry_millis":9000,"owner":"1a…","staged":{…},"x":1}` is **accepted** by S1
  (`metadata::decode::<PendingEntry>` + `OwnedEntry::from_pending` — `PendingEntryWire` is
  deliberately open) and **refused** by S2 (`decode_owned_entry` → `NoncanonicalRecordValue`), which
  is exactly what `crates/core/tests/multipart_staging_retire.rs:735`
  (`an_unknown_field_is_refused`) asserts. `decode_owned_both`
  (`crates/core/tests/multipart_staging_retire.rs:155`) is invoked on only two accepted witnesses and
  would `assert_eq!` -fail if handed that one, so nothing "pins the sets equal"; the next paragraph
  of the same doc comment concedes the divergence. Reword to what is true (the *rules* are shared;
  the accepted sets differ by exactly the canonical-bytes gate).

- **NEEDS-HUMAN [impl]** — `crates/core/src/multipart.rs:2448-2449` and
  `crates/core/tests/multipart_staging_retire.rs:464`: "a length-mismatched placement decodes and
  is quarantined by GC's safety gate and attributed by the drain" is stated in the present tense
  about machinery that does not exist in this tree. GC's malformed set is built only over
  **committed** chunk maps (`crates/custodian/src/gc.rs:146`, `:152`, builder doc `:336-340`), the only `pending:`
  reader scans `pending:` alone (`crates/custodian/src/gc.rs:488`), nothing anywhere reads a
  `sidx:` key, and there is no drain. 0016 says this as *design* (`0016:416-429`, via a "staged
  reference build" that is decision 2's future pass) and the brief explicitly **withdrew** the
  quarantine claim as undemonstrable in this slice — so restating it as fact inside a module whose
  own header is scrupulous that "nothing here is written yet" is the one tense slip in the diff.
  Attribute it to the proposal or put it in the future tense.

## Attempted and could not refute

- **The key-taking cross-checks.** I tried to find a payload/key pair the decoders let through:
  `Records`/`Chunks`/`Parts`/`Session`/`Generation` against both modes, both token kinds, and
  present/absent part suffix — every illegal combination errors, and the mapping matches 0016's
  "Written by" columns (`0016:353-356`, token grammar `:357-378`). `retire:records:g:<inode>:<version>`
  is rejected, and reading 0016 that is right: a superseded generation's `seg:` records are deleted
  by its own `retire:bytes:` generation obligation, not by a record-mode generation token.
- **The nested-value seams.** `SegmentGroup` (the one nested type read *without* a module-local wire
  mirror) already validates its nonce and denies unknown fields at decode
  (`crates/core/src/metadata.rs:826-837`), so leg 1p has no `segments`-shaped hole beside it.
  `ChunkRefWire`/`EcSchemeWire`/`StagedPlacementWire` are closed and require `placement`, so
  round-trip identity through the retirement payloads holds.
- **Arithmetic and bounds.** `PartNumberSet::from_runs` validates both endpoints through
  `PartNumber::new` *before* `previous_hi + 1` (`multipart.rs:2665-2681`), so the coalescing test
  cannot overflow; `len()` is exact at the format maximum (`from_runs([(1, 999_999)]).len() == 999_999`).
- **Leg 1e's deliberate narrowing.** An owned-shaped value under a `pending:` key still decodes, as
  the brief requires, and explicit `"owner":null,"staged":null` decodes to the legacy shape and
  re-encodes without the nulls — no regression against the pre-patch open record.
- **Red→green load-bearingness.** I did not re-derive it by hand; `C5-mutants` reports 0 surviving
  mutants over this diff, which covers the "a leg green under its own negation" risk better than a
  spot check would.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the born-at-tier compile failure as adequate red evidence — base `c824243` with only `crates/core/tests/multipart_staging_retire.rs:48` retained failed on the absent APIs and fields, but this is criterion absence rather than an isolating behavioral red.
- [ ] C3 Change — Decide whether to re-enter Plan for the size overrun — `patch.diff` adds 1,780 raw lines and at least 1,131 nonblank/noncomment lines against the brief's ≤960-semantic-line budget, materially increasing review and rebase surface.
- [ ] C4 Verification (red→green) — Decide whether the recorded full gate may stand despite incomplete independent reproduction — the focused test passed 24/24 and mutation testing reproduced 49 caught/21 unviable, but six existing server/custodian tests stalled beyond five minutes and two cargo-deny advisory checks could not lock the host's read-only database.
- [ ] T4 Contribution — Confirm closed/rejected prior art by every affected path before sign-off — merged refs have no `-S` hit for the four new type names, but the supplied artifacts cannot mechanically establish the closed/rejected-work half of that check.
- [ ] T5 Judgment — Remove or substantiate the GC-quarantine claim — the test only proves decode success at `crates/core/tests/multipart_staging_retire.rs:468`, while current GC scans only `pending:` at `crates/custodian/src/gc.rs:488` and cannot observe `sidx:`.
- [ ] Validation — fitness-to-purpose — Confirm these pre-writer record shapes and the provisional namespace-deferral owner fit the downstream multipart lifecycle — the architecture states the writers/consumers have not landed yet at `docs/design/architecture/05-building-block-view.md:202`.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- [ ] The key/value invariant is still asymmetric. The brief says a decoder that cannot see the key cannot validate against it (`brief.md:6-8`), but it adds key-taking decoders only for `sidx:` and `retire:` and calls both-present `owner`/`staged` a valid owned shape (`brief.md:33-36`). The target design says both fields are `Some` only on `sidx:` and both are `None` on `pending:` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:442-457`), while the live pending readers decode bytes without the key (`crates/core/src/metadata.rs:1973-1980`, `:2011-2016`) and GC will reclaim any expired both-present value found under `pending:` (`crates/custodian/src/gc.rs:488-492`). The brief needs either a pending-key-aware rejection and its resulting source scope, or a narrower invariant plus an explicit deferral; as written it repeats the reviewed “decoder cannot see the key” defect on the existing namespace.
- [ ] The placement-length criterion promises an outcome this child cannot observe: it says the mismatched `sidx:` placement is “quarantined by GC” (`brief.md:41-45`), but its asserted leg proves only that decoding succeeds, custodian source is expressly out of scope (`brief.md:92-94`), and the first production `sidx:` writer is deferred to #657 (`brief.md:112-118`). The target GC currently scans only `pending:` (`crates/custodian/src/gc.rs:482-496`). Revise this to the verifiable decode-boundary claim alone, or bring the staged-reference/quarantine wiring and an observable test into scope.
- [ ] The “ONE `metadata.rs` hunk — nothing else in that file changes” scope (`brief.md:79-82`) omits an existing in-file `PendingEntry` constructor at `crates/core/src/metadata.rs:3369-3377`; adding two required fields makes that constructor fail to compile unless a second, distant hunk is changed. The scope/hunk and line budgets must explicitly allow this ninth constructor site (within the already-counted substantive file).
- [ ] The base-state claim contradicts the resolved prerequisite state. The brief says it builds on child 2's **merged** result (`brief.md:75`) and asks for citations on the merged base (`brief.md:119-127`), but `dependency-state.json:2-5` says #716 exists only in `PLANNED` state. The resolved target confirms that its record values and `encode_record`/`decode_record` are still “the next child's” (`crates/core/src/multipart.rs:5-12`) and contains no such definitions. Revise the execution precondition/target so this bundle waits for or materializes #716 instead of claiming that prerequisite is already merged.
- [ ] The tracker support for the root-cause and ignored-prior-attempt claims is unavailable: this advisory bundle has no `notes.json` or `sources/`, while the brief requires salvaging and correcting `results/issue_692/iteration-v2/patch.diff` (`brief.md:13-15`, `:138-142`), a path also absent from the resolved target. The planner needs to supply the cited tracker/review evidence (or quote its load-bearing lines in the brief) so the asserted v2 failures and constraints can be checked rather than taken on trust.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the born-at-tier compile failure as adequate red evidence — base `c824243` with only `crates/core/tests/multipart_staging_retire.rs:48` retained failed on the absent APIs and fields, but this is criterion absence rather than an isolating behavioral red.; C4 Verification (red→green) — Decide whether the recorded full gate may stand despite incomplete independent reproduction — the focused test passed 24/24 and mutation testing reproduced 49 caught/21 unviable, but six existing server/custodian tests stalled beyond five minutes and two cargo-deny advisory checks could not lock the host's read-only database.; T4 Contribution — Confirm closed/rejected prior art by every affected path before sign-off — merged refs have no `-S` hit for the four new type names, but the supplied artifacts cannot mechanically establish the closed/rejected-work half of that check.; T5 Judgment — Remove or substantiate the GC-quarantine claim — the test only proves decode success at `crates/core/tests/multipart_staging_retire.rs:468`, while current GC scans only `pending:` at `crates/custodian/src/gc.rs:488` and cannot observe `sidx:`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-12

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
