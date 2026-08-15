- **Slug:** multipart-staging-retire-pending
- **Defect:** the staging and retirement records — the two whose identity lives
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
- **Success criterion:** every type this child lands round-trips, and each hand-authored
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
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_staging_retire.rs` + CRATEs
  core/custodian/dst/metadata-redb/server; the GREEN leg is `cargo test -p wyrd-core
  --test multipart_staging_retire`; the RED leg reverts production and the test fails to
  **compile** → **UNVERIFIABLE (exit 77), EXPECTED and PRE-DECLARED** as a §6 item.
  **Demonstrated red Do MUST capture instead (binding) — ELEVEN demonstrations, and the
  count is spelled out because the previous version said "six" here and "eight" under
  Verification posture (plan-review finding, 2026-08-09; THIS list is the authority):**
  *nine isolating negations*, one per binding rejection leg — **1b, 1d,
  1h-session-under-part, 1h-part-under-session, 1h-records-under-part, 1i-EcScheme, 1e,
  1n-generation-both-sources** (the neither-source case may ride the same negation if one
  guard covers both; say which in `build-notes.md`), **1p-nested-chunkref** — drop that
  single check, run the test, paste the failing output into `build-notes.md`, revert.
  *Plus one for leg 2*: remove ONE `skip_serializing_if` attribute and show the byte-identity
  leg fail. *Plus the leg-3 corollary negated the other way*: make the length-mismatched
  placement reject and show the assert-it-decodes leg fail. **Each negation must ISOLATE its
  rule.** A leg green under its own negation is not load-bearing and must be rewritten.
- **Invariant to restore:** sourced from the TARGET repo, the only tree Do can read:
  **ADR-0045 §"Parse-don't-validate at decode"**
  (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`; its invariant table at
  `:69-74` names the `EcScheme::ReedSolomon` → `erasure::supported(k, m)` rule leg 1i
  enforces), plus `0016:390-402` (format maxima, never live knobs) and `0016:442-457` (the
  two additive `PendingEntry` fields and the load-bearing `skip_serializing_if`). (The
  harness-side catalogue rule is **C-1**, `docs/principles.md:109` / `:137` in the
  *wyrd-pdca* repo — audit trail only; **Do cannot open it**, being grounded on a wyrd
  checkout. Cite the ADR. Plan-advisory finding, 2026-08-09.) Over this child's category,
  **scoped to the `sidx:` and `retire:` namespaces this child introduces** (see the
  deferral in Scope): **a stored record's fields may not
  disagree with each other OR WITH THE KEY THAT NAMES THEM, and the disagreement must
  surface as an error, never as a value**. An owned entry attributed to the
  wrong session is staged data renewed or reclaimed under the wrong identity. A retirement
  payload under the wrong token — or the wrong *scope* of token — reclaims one generation's
  data while clearing another's obligation. Untrusted `EcScheme` geometry that decodes is
  the #285 panic class made durable. A torn `PendingEntry` turns a structural invariant
  into a convention. And a `PendingEntry` that does not re-encode byte-identically turns
  every existing `pending:` lease renewal into a permanent `Conflict`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality. **Execution PRECONDITION, not a claim about
  today** (clarified 2026-08-09 from a plan-advisory finding: #716 currently reads
  `PLANNED`, and `multipart.rs` on `origin/main` still says the record values are "the next
  child's"): this child MUST NOT build until **#716's PR — and #715's beneath it — are
  merged into the base**. `Depends on: 716` enforces it: with `auto_merge = false` the
  driver merges nothing, stops at each wave boundary for the human, and re-gates on
  `merged.is_merged`. Refresh every `path:line` citation and the line estimate against the
  base this actually builds on, not against `9dbcd72`. The
  one live-path touch is `PendingEntry`, and leg 2 proves the live path (every existing
  `pending:` CAS) is byte-identical — the extension is inert until #657 writes the first
  `sidx:` record.
- **Scope:** extend `crates/core/src/multipart.rs` with
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
- **Budget:** ≤ 960 added semantic lines (module extension ≈ 420, `metadata.rs` ≈ 45,
  test ≈ 440;  raised 2026-08-09 for the plan review's added legs 1h-records, 1n, 1p, ripple ≈ 40 mechanical, docs ≈ 15) across exactly **12** files — 4
  substantive (`multipart.rs`, `metadata.rs`, the new test, the one docs paragraph) and 8
  mechanical, all named above.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; `docs-renderer` is load-bearing HERE (leg 4 edits a rendered architecture doc), the rest warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_staging_retire.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator). The key-taking legs, the
  byte-identity leg and the docs-adjacent legs live here; co-located unit tests may ship in
  addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as its siblings: the
  UNVERIFIABLE RED is PRE-DECLARED so C2/C4 land as a known sign-off item rather than a
  surprise NEEDS-HUMAN. Everything built is exercised at Check under the named test +
  gating C4-ci, and the ELEVEN demonstrations enumerated under **Falsifiability** (which
  is the authority for that count) in `build-notes.md` replace the
  flippable red.
- **Production reach:** the ONE live-path touch in this whole 3-child chain. `PendingEntry`
  is on the existing `pending:` write/renew path, so (a) what honours the seam now is the
  legacy shape itself — both new fields absent — and leg 2 proves that path re-encodes
  **byte-identically**, i.e. the extension is genuinely inert; (b) the production wiring
  that writes a `sidx:` record with `owner`/`staged` set lands in **#657**, which needs the
  store round trips (#656–#659) first; (c) the torn-shape rejection (leg 1e) is exercised
  load-bearingly by hand-authored values in the named test, not by dead scaffolding.
- **Citations expected:** cite `path:line` on the merged base for every change. **Read the
  two citation namespaces apart — this is a trap:** a `crates/core/src/multipart.rs:NNNN`
  reference tagged *(batch-review …)* or *(v2 review …)* is relative to the **v2 PATCHED
  file** (~2,027 lines) preserved in `results/issue_692/iteration-v2/patch.diff`, NOT to
  the base — `multipart.rs` was **854 lines** on `9dbcd72` and grows only by #715/#716
  beneath this child, so those line numbers do not exist there. Locate them in the archived
  patch, by symbol. The `metadata.rs` citations ARE base-relative and were verified at this
  Plan (`PendingEntry` at `:1528`, the `skip_serializing_if` precedent at `:1368-1391`) —
  but cite by SYMBOL anyway, since #710/#711 may land in `metadata.rs` first. Sources Do
  MUST open: `0016:475-491` (the `sidx:` disjoint-staging rule), `0016:357-378` (the
  retirement-token grammar leg 1h enforces — the `<token>` grammar paragraph, where the
  optional `:<part-number>:<attempt-id>` suffix is defined; `0016:437-453` is the
  adjacent *mode-in-the-key* argument, related but NOT the grammar). **Two source citations
  CORRECTED 2026-08-09 (plan-review finding — the previous range was simply the wrong
  section):** the `skip_serializing_if` identity argument is `0016:475-485`, **not**
  `0016:512-527` (that range is the segment-group **marker lifetime** and has nothing to do
  with serialization); the placement-length contextual boundary is `0016:416-429`. Read
  `0016:475-485` with leg 2's correction in hand — it is right that the property is
  load-bearing and wrong about the mechanism. Also ADR-0045, ADR-0047:38-50.
  Peer callsites Do MAY open: `crates/core/src/metadata.rs:1368-1391` (the
  identity-preserving optional-field precedent the `PendingEntry` hunk must mirror);
  **the codec pattern, in place of the withdrawn envelope (added 2026-08-09):**
  `metadata::encode` / `decode` (`crates/core/src/metadata.rs:1536-1543`) for the bytes,
  validation **inside `Deserialize`** (`InodeRecord`'s `#[serde(try_from =
  "InodeRecordWire")]` at `metadata.rs:1349`/`:1411`, or `SegmentRecord`'s hand-written impl
  at `:1195`/`:1212-1216`), and a per-record decode wrapper that attributes the failure to a
  typed error — `decode_segment_record` (`metadata.rs:2504-2517`), **which is exactly the
  shape `decode_owned_entry(key, bytes)` / `decode_retire_obligation(key, bytes)` take, one
  extra parameter aside**;
  `crates/core/src/erasure.rs:120` (`supported(k, m)`, leg 1i's predicate and the #285
  precedent); `docs/design/architecture/05-building-block-view.md:186-192` (the ADR-0047
  paragraph leg 4 mirrors — match its voice and length, do not restate the proposal).
  **Salvage:** ``$PDCA_HARNESS_ROOT/results/issue_692/iteration-v2/patch.diff` — the path is relative to the HARNESS repo (wyrd-pdca), NOT to `$PDCA_WORKTREE`; a claude builder's cwd is the harness root so it resolves as written, but a codex builder/escalation runs with cwd = the worktree and must resolve it absolutely` — take the staging/retirement
  types and its `metadata.rs` hunk, then FIX the recorded defects (the reviews found the
  decoders unable to SEE the key at `multipart.rs:1555/1789/1800`, the token suffix ignored
  at `:2024`, and `StagedPlacement` deriving unchecked `EcScheme` at `:1657`) rather than
  re-shipping the reviewed shape.
- **Prior-art check (triage cycles):** verified at Plan against `9dbcd72`: no
  `OwnedEntry`, `StagedPlacement`, `PartNumberSet` or `RetirePayload` exists on
  `origin/main`; `PendingEntry` (`metadata.rs:1528`) carries only `lease_expiry_millis`;
  `git -C ../wyrd log origin/main -- crates/core/src/metadata.rs` shows no multipart-related
  commit. Open PRs: **none touching these paths today, but #710 and #711 are in flight over
  `core/src/metadata.rs`** — hence the conflict declaration below. Closed/rejected: #654's
  two archived attempts and #692's own two — the batch review's token-scope blocker
  (`multipart.rs:2024`) and the reviewer's C5/T2 findings are this child's binding legs
  1h/1i, not suggestions.
- **Difficulty:** high   (**12 files across 6 crates** — core, custodian, dst, metadata-redb,
  server, plus a rendered docs file — and the only child of this chain that leaves
  `multipart.rs`. What a diff-reviewer must hold in view is the whole cross-crate reach: a
  struct on the **live `pending:` write path** gains two persisted fields and **drops
  `Copy`**, and it is that `Copy` removal — not the fields — that forces the 8-file ripple,
  because every construct/copy site must become a clone. Rated **up** from the `medium` the
  split proposal carried: the parent brief argued medium on the grounds that every ripple
  hunk is mechanically checkable, but blast-radius is the criterion here, not edge-case
  density or hunk difficulty, and 6 crates plus a live-path struct change is wide by that
  measure. Rating up is also the instructed default when unsure, and it is load-bearing —
  `high` routes the opus/xhigh builder (`pdca.toml:280`) and enables the adversary
  refutation pass at Check (`pdca.toml:543`), both of which this child should get and its
  two pure-`multipart.rs` siblings need not.)
- **Depends on:** 716
- **Conflicts with:** 710, 721, 722
- **Ordering note:** **Wave 2 — terminal.** `Depends on: 716` is a genuine build-on
  (this child's `StagedPlacement` decode reuses the validated-nested-type pattern #716
  establishes for `ChunkRef`, and its typed rejections are further variants of the shared
  `RecordError` #715 widened and #716 extended — restated 2026-08-09, since the envelope
  clause that used to appear here was withdrawn at #715's re-plan), and it
  also wave-serialises the shared `multipart.rs`. **This child alone carries the chain's
  external conflicts** — #715 and #716 touch only `multipart.rs` and their own new test,
  neither of which #710 or #711 reads. #682 was SPLIT on 2026-08-08 into **#710** (shares
  `core/src/metadata.rs` — its `MAX_VALUE_BYTES` enforcement vs this child's `PendingEntry`
  region) and **#711** (shares BOTH `core/src/metadata.rs`, its `repoint_chunk` primitive,
  AND `dst/tests/custodian.rs`, its substantive edits vs this child's mechanical
  initializer lines). The proposal's ordering fields may only name sibling labels, so
  `Conflicts with: 710, 711` was added to this materialised brief at split acceptance
  (2026-08-09) — it must never share a wave with either. **Repointed 2026-08-10:** #711 was
  itself SPLIT (iterate-plan, oversize) into **#721** (the placement primitive in
  `core/src/metadata.rs` + the repair caller) and **#722** (the drain caller +
  `dst/tests/custodian.rs`), so the field now reads `710, 721, 722` — #721 carries the
  `core/src/metadata.rs` overlap and #722 the `dst/tests/custodian.rs` one. #693 and #655, which were blocked
  on the pre-split #692, were repointed at THIS issue at the same moment. **Cite by symbol,
  not by line number**, in `metadata.rs`: the base will have advanced under #710/#711.
- **Disposition hint:** new-feature

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the born-at-tier compile failure as adequate red evidence — base `c824243` with only `crates/core/tests/multipart_staging_retire.rs:48` retained failed on the absent APIs and fields, but this is criterion absence rather than an isolating behavioral red.; C4 Verification (red→green) — Decide whether the recorded full gate may stand despite incomplete independent reproduction — the focused test passed 24/24 and mutation testing reproduced 49 caught/21 unviable, but six existing server/custodian tests stalled beyond five minutes and two cargo-deny advisory checks could not lock the host's read-only database.; T4 Contribution — Confirm closed/rejected prior art by every affected path before sign-off — merged refs have no `-S` hit for the four new type names, but the supplied artifacts cannot mechanically establish the closed/rejected-work half of that check.; T5 Judgment — Remove or substantiate the GC-quarantine claim — the test only proves decode success at `crates/core/tests/multipart_staging_retire.rs:468`, while current GC scans only `pending:` at `crates/custodian/src/gc.rs:488` and cannot observe `sidx:`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rationale: the T4 batched-review gate correctly failed on a real, unaddressed correctness gap — `RetirePayload::Records { segments }` is accepted without cross-checking the segment group's epoch against the retirement token's session epoch, so a misfiled retirement obligation could reference (and a drain could delete) a different completion attempt's segment generation. This is a scoped fix inside `checked_against_token`/`checked_shape`, not a re-slice. Fold in two more small, same-file fixes while iterating: 1. Correct the `renew_pending` doc comment at `metadata.rs:1582` (and its echo in `multipart_staging_retire.rs`): it claims a renewal stores what it read; the code actually re-encodes the caller's entry. The test assertions are still correct — only the stated mechanism is wrong. 2. Change `decode_owned_entry`'s return type to include the parsed `part_number`/`chunk_id` (mirroring `decode_retire_obligation`'s `(token, payload)` shape), so a future caller isn't forced to re-parse the `sidx:` key a second time. Do NOT attempt the `PendingEntryWire deny_unknown_fields` blast-radius question (silent field-drop vs. whole-sweep failure on an unrecognized field) in this iteration — that's deferred to a separate follow-up tracker issue (milestone 0.1 Alpha, see SUMMARY.md §10), since a proper fix (warn-and-continue per record in the scan loop) requires touching live readers (`gc.rs`, `write.rs`) that this bundle's brief explicitly places out of scope. Scope overrun (T1 Structure / size backstop / C3 Change) is explicitly set aside for this round per human direction — not part of this iteration's ask. The base-state precondition (#716 merged) is confirmed resolved; #692 tracker-evidence and the two design-scope items (key/value asymmetry on `pending:`, placement-length/GC-quarantine wording) are accepted as correctly scoped/deferred by the brief and need no further action.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Not converging: this is the slice's own recommendation (size backstop — patch 121KB vs 100KB threshold, 2 rounds already spent) plus a gating T4 batch-review FAIL and multiple scope-validity §6 items (unresolved #716 base-state contradiction, missing tracker citations for salvaged v2 material, in-file constructor-site scope gap). The adversarial review also surfaces a record-format decision (RetirePayload Session/Parts cannot express the combined shape 0016 requires) that affects sibling children and belongs in Plan, not another Do pass. Re-plan and split rather than iterate-do: a slice this size will keep yielding implementation-shaped findings round after round without converging.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_717/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
