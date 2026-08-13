<!-- pdca:split-proposal v1 -->
# Split proposal — issue 692

## Why this slice is oversized

The brief is one *module* but three *shippable outcomes*, and its own text says so: the
driver sized it at 17.9 KB against a 12 KB cutoff, v2 spent 2,140 added lines across 11
files and was refused at the 100 KB backstop, and the Budget line explicitly frames the
2,150-line / 12-file envelope as "the envelope the split must carve, not a per-child
figure." The header states outright: "THIS BRIEF IS THE SPLIT INPUT — it is not meant to
be built as one slice."

The seams follow the key-prefix families that proposal 0016 already keeps separate, and
each seam has its own defect, its own binding legs, and its own hand-authored torn values:

1. **The profile arithmetic** — `Budget` and `AdmissionRecord` (`mpuctl`), plus the
   `encode_record`/`decode_record` envelope every later record dispatches through. Its
   defect is arithmetic self-consistency: legs 1a (derived `max_sessions` vs stored
   tuple), 1f (both ends of every knob range), 1g (the `sidx:` scan bound counting
   committed staging), and leg 3's occupancy ruling. Pure `multipart.rs`, no key-taking
   decoders, no ripple. Shippable alone: after it, an out-of-range profile or a
   self-disagreeing admission record is a typed decode error.

2. **The session lifecycle records** — `SessionRecord` with
   `SessionState`/`PublishTarget`/`Completion` (`mpu:`), `SlotRecord` (`slot:`),
   `PartRecord`/`PartSummary` (`part:`/`psum:`). Its defect is *nested* identity and
   structural validation: legs 1c (publish target vs the session's own parent/object) and
   the two `multipart.rs` halves of 1i (`ChunkRef` validated structurally; a slot born
   already lapsed rejected). Pure `multipart.rs` again. Shippable alone: after it, every
   in-flight lifecycle value round-trips or errors.

3. **The staging/retirement records and the one live-path touch** —
   `OwnedEntry`/`StagedPlacement` (`sidx:`), `PartNumberSet`/`RetirePayload` (`retire:*`),
   the two **key-taking** decoders (`decode_owned_entry`, `decode_retire_obligation` —
   exactly how v2's API shape failed review), and the `PendingEntry` extension in
   `metadata.rs` with its mechanical 8-file ripple and the leg-4 docs paragraph. Its
   defect is key/payload identity: legs 1b, 1d, 1e, 1h, the `EcScheme` half of 1i, leg 2
   (byte-identical legacy re-encode), and leg 3's placement-length corollary. This is the
   only child that leaves `multipart.rs`, so it alone inherits the parent's declared
   conflicts with **#710** (shares `core/src/metadata.rs`) and **#711** (shares
   `metadata.rs` and `dst/tests/custodian.rs`) — per the parent's own ordering note, that
   conflict "belongs to the ONE child that touches `metadata.rs` — not to all three."

Three, not two: any two-way cut leaves one child at roughly v2's refused size (the record
types plus decoders alone were ~970 added-file lines in the v2 patch before the ripple),
and any six-way cut splits records from the decoders that validate them, which is no
longer independently shippable. Three, not fewer, also matches where the torn-value test
fixtures naturally partition — each child's new test file exercises only types the child
itself lands.

## Wave sketch

**Strictly linear: child-1 → child-2 → child-3. Three waves, one child each.** No pair
can share a wave regardless of preference: all three extend
`crates/core/src/multipart.rs`, and children 2 and 3 each add match arms to the
`encode_record`/`decode_record` dispatch that child-1 lands — the same function bodies,
guaranteed textual collision. The ordering is `Depends on:` rather than `Conflicts
with:` because the stacking is not mere avoidance: child-2's dispatch arms build on
child-1's merged envelope, and child-3's `StagedPlacement` decode reuses the validated
nested-type pattern child-2 establishes for `ChunkRef` (both are instances of leg 1i's
"nested types validate at decode"). Under `wave_mode = "merge"` with `auto_merge =
false`, the human merges at each wave boundary and each child's patch applies on a base
that already contains the scaffolding it imports — exactly the posture the parent's
Falsifiability section pre-declares.

Child-3 is deliberately **terminal**: it carries the highest-risk material (the key-taking
APIs the v2 review demanded, the only `metadata.rs` hunk, the ripple, the docs leg), so
the two pure-`multipart.rs` children are already merged and de-risked before the child
with external conflicts builds. At split acceptance: repoint **#693** (`Depends on: 692`)
and, transitively, #655 at **child-3**, per the parent's ordering note; the external
conflicts with **#710/#711** attach to **child-3 only**.

Budget carve (must sum within the parent's ≤ 2,150 lines / 12 files): child-1 ≈ 550 lines
/ 2 files, child-2 ≈ 700 lines / 2 files, child-3 ≈ 900 lines / 12 files (of which the
8 ripple files are ≤ 8 mechanical lines each).

<!-- pdca:child child-1 -->
- **Slug:** multipart-budget-admission
- **Defect / goal:** the key grammar exists (#691, merged at `9dbcd72`) but no record
  **values** do, and there is no envelope to encode/decode any. This child lands `Budget`
  (the profile tuple of `0016:1463-1480` and its pure derivations `U_ref` and
  `MAX_SESSIONS = ⌊W_ref/U_ref⌋`, `0016:1469-1470`), `AdmissionRecord` (`mpuctl`,
  `0016:333-527`), and the shared `encode_record`/`decode_record` envelope that later
  record children extend with their own arms. Salvage from
  `results/issue_692/iteration-v2/patch.diff` (record types and decoders, added-file
  lines ~846–1818, the `Budget`/`AdmissionRecord` portion), fixing the recorded defects
  rather than re-shipping the reviewed shape.
- **Success criterion:** `Budget` and `AdmissionRecord` round-trip `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md` (drop the
  single check, paste the failing output, revert):
  **(1a)** an `AdmissionRecord` whose stored `max_sessions` disagrees with what its own
  stored `profile` tuple derives (the derivation functions are implemented HERE);
  **(1f)** `Budget::new` enforces BOTH ends of every knob range in `0016:1463-1480`:
  `max_part_chunks` must satisfy the value-ceiling rule AND the `B_ops` clamp
  (batch-review `multipart.rs:1041`), and `max_staged_chunks` must lie in
  `[max_part_chunks, MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS]` (batch-review
  `multipart.rs:1027`) — a profile above either end is rejected;
  **(1g)** the `sidx:` scan bound counts committed staging entries too, not only
  in-flight chunks, so an accepted profile can never exceed `SCAN_CAP/2` (batch-review
  `multipart.rs:1074`);
  **(leg 3, binding the other way)** a decoded `AdmissionRecord` whose `count` exceeds
  its `max_sessions` still **decodes** — occupancy above a lowered cap is a legitimate
  live state, not a decode error (`0016:499-511`; format maxima per
  `metadata.rs:312-321`); assert it decodes. Identity relations are binding; occupancy
  relations are not.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier, as #691. C4-verify
  classifies `ADDED_TEST crates/core/tests/multipart_budget_admission.rs` + `CRATE
  crates/core`; the GREEN leg is `cargo test -p wyrd-core --test
  multipart_budget_admission`; the RED leg reverts production and the test fails to
  **compile** → **UNVERIFIABLE (exit 77), EXPECTED and PRE-DECLARED** as a §6 item.
  **Demonstrated red Do MUST capture instead (binding):** four named negations, one per
  binding leg (1a, 1f-lower, 1f-upper, 1g) — drop that single check, run the test, paste
  the failing output into `build-notes.md`, revert. A leg green under its own negation is
  not load-bearing and must be rewritten. Leg 3 is negated the other way: make the
  occupancy case reject and show the assert-it-decodes leg fail.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, §6 row at `:137`;
  `0016:2802-2813`), over this child's category: **a stored record's fields may not
  disagree with each other, and the disagreement must surface as an error, never as a
  value** (ADR-0045). An admission record whose `max_sessions` does not match its own
  `profile` admits sessions past the memory bound the reconcile pass is sized for — a
  fleet-wide failure admitted by one unvalidated field. A profile accepted above its
  settled range produces a maximal part whose commit and whose compensation both time out
  permanently, stranding the slot forever (`0016:1466`, the `B_ops` clamp).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality on a base (`9dbcd72`) where only the key
  grammar exists (`crates/core/src/multipart.rs` is 854 lines of keys and identity types).
- **Scope (one logical fix) / out of scope:** extend `crates/core/src/multipart.rs` with
  `Budget`, `AdmissionRecord`, their validating decoders, and the
  `encode_record`/`decode_record` envelope (arms for this child's records only). One new
  test file. Budget ≈ 550 added lines / 2 files. Cite `path:line` on `9dbcd72` for every
  change; sources Do MUST open: `0016:333-527`, `0016:1463-1480`, ADR-0045;
  `metadata.rs:312-321` and `:327-352` (the format maxima the ranges are computed
  against). / out of scope: every other record type (child-2, child-3); `metadata.rs`
  and any file outside `multipart.rs` + the new test; the knob **values** (#655); the
  outcome enums, answer table, `Verb`, `MultipartEtag`, digests, `sha2` (#693 — no
  `Cargo.toml`/`Cargo.lock` change); store round trips (#656–#659); all `docs/` files.
- **Budget:** ≤ 550 added semantic lines (module extension ≈ 300, test ≈ 250) across
  exactly **2** files: `crates/core/src/multipart.rs` and the new test. Well under the
  100 KB size backstop v2 breached.
- **External dependencies:** `typos`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_budget_admission.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator). Co-located unit tests may
  ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as #691: the
  UNVERIFIABLE RED is PRE-DECLARED here so C2/C4 land as a known sign-off item rather than
  a surprise NEEDS-HUMAN (it surfaced as one in BOTH archived attempts). Everything built
  is exercised at Check under the named test + gating C4-ci, and the four negation
  demonstrations in `build-notes.md` replace the flippable red.
- **Production reach:** N/A by design — `Budget`/`AdmissionRecord` have no live writer
  until #656–#659 wire the store round trips; nothing on an existing path changes.
- **Citations expected:** cite `path:line` on `9dbcd72` for every change. Peer callsites
  Do MAY open: `crates/core/src/metadata.rs:312-321` and `:327-352` (`MAX_VALUE_BYTES` /
  `MAX_ROOT_VALUE_BYTES` / `MAX_ROOT_SEGMENTS` — the **format** maxima leg 1f's ranges are
  computed against, and the occupancy-not-at-decode boundary of leg 3). **Salvage:**
  `results/issue_692/iteration-v2/patch.diff` — take the `Budget`/`AdmissionRecord` types,
  wire structs and decoders, then FIX the recorded defects (legs 1f/1g were found missing
  or one-ended) rather than re-shipping the reviewed shape;
  `results/issue_692/review-batch.md` holds those blockers verbatim.
- **Prior-art check (triage cycles):** verified at Plan against `9dbcd72`: no `Budget`,
  `AdmissionRecord` or record codec exists on `origin/main` (`multipart.rs` is 854 lines
  of keys and identity types, ending at the retirement-key parsers); no open PR touches
  these paths. Closed/rejected: #654's two archived attempts and #692's own two
  (`iteration-v1/`, `iteration-v2/`) — the batch review's three budget blockers
  (`multipart.rs:1027/1041/1074`) are this child's binding legs 1f/1g, not suggestions.
- **Difficulty:** medium
- **Ordering note:** **Wave 0.** #691 (the key grammar this child's types are built from)
  is COMPLETE **and merged** — `d986069`, PR #703, in `origin/main @ 9dbcd72` — so it is
  not carried as a `Depends on`: the base already contains it. No `Conflicts with`: this
  child touches only `multipart.rs` and its own new test, neither of which #710 or #711
  reads.
- **Disposition hint:** new-feature
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** multipart-session-lifecycle-records
- **Defect / goal:** with the envelope and profile arithmetic merged (child-1), the
  in-flight lifecycle records still do not exist. This child lands `SessionRecord` with
  `SessionState`/`PublishTarget`/`Completion` (`mpu:` — the *states* of `0016:528-602`;
  the answer table is #693's), `SlotRecord` (`slot:`), and `PartRecord`/`PartSummary`
  (`part:`/`psum:`), each with a validating decoder and an arm in child-1's
  `encode_record`/`decode_record`. Salvage the corresponding types from
  `results/issue_692/iteration-v2/patch.diff`, fixing the recorded defects.
- **Success criterion:** every type this child lands round-trips `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md`:
  **(1c)** a `SessionRecord` whose `publish_target.parent`/`name` disagrees with the
  session's own `parent`/`object` (v2 review, `multipart.rs:1258`);
  **(1i, PartRecord half)** `PartRecord` validates each `ChunkRef` structurally at decode
  rather than accepting a raw one (v2 reviewer C5 + T2 FAIL, `multipart.rs:1538`) —
  nested types validate at decode, not just the outer record;
  **(1i, SlotRecord half)** a `SlotRecord` with `lease_expiry_millis <=
  reserved_at_millis` — a slot born already lapsed — is rejected (`multipart.rs:1657`).
  Decode validates against **format** maxima, never live knobs (`0016:499-511`).
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_session_records.rs` + `CRATE crates/core`; the
  GREEN leg is `cargo test -p wyrd-core --test multipart_session_records`; the RED leg
  reverts production and the test fails to **compile** → **UNVERIFIABLE (exit 77),
  EXPECTED and PRE-DECLARED** as a §6 item. **Demonstrated red Do MUST capture instead
  (binding):** three named negations, one per binding leg (1c, 1i-ChunkRef, 1i-slot) —
  drop that single check, run the test, paste the failing output into `build-notes.md`,
  revert. A leg green under its own negation is not load-bearing and must be rewritten.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, §6 row at `:137`;
  `0016:2802-2813`), over this child's category: **a stored record's fields may not
  disagree with each other, and a nested value is validated at decode exactly as the outer
  record is** (ADR-0045). A session whose `publish_target` names a different object than
  the session's own parent/key publishes one client's upload under another's name. A
  `PartRecord` that accepts a raw, unvalidated `ChunkRef` lets untrusted stored geometry
  reach the read path — the #285 class. A slot born already lapsed
  (`lease_expiry <= reserved_at`) is reapable the instant it is written, so a live part
  attempt can have its staging reclaimed underneath it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality; builds on child-1's **merged** result
  (`wave_mode = "merge"`, human-merged at the wave boundary).
- **Scope (one logical fix) / out of scope:** extend `crates/core/src/multipart.rs` with
  the five lifecycle types listed under Goal, their validating decoders, and their
  `encode_record`/`decode_record` arms. One new test file. Budget ≈ 700 added lines /
  2 files. Cite `path:line`; sources Do MUST open: `0016:333-527`, `0016:528-602`,
  ADR-0045. / out of scope: `Budget`/`AdmissionRecord` (child-1, merged beneath this);
  `OwnedEntry`/`StagedPlacement`, retirement types, `PendingEntry`, and every file
  outside `multipart.rs` + the new test (child-3); the outcome enums and answer table
  (#693); knob values (#655); store round trips (#656–#659); all `docs/` files.
- **Budget:** ≤ 700 added semantic lines (module extension ≈ 400, test ≈ 300) across
  exactly **2** files: `crates/core/src/multipart.rs` and the new test.
- **External dependencies:** `typos`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_session_records.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator). Co-located unit tests may
  ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as child-1: the
  UNVERIFIABLE RED is PRE-DECLARED so C2/C4 land as a known sign-off item rather than a
  surprise NEEDS-HUMAN. Everything built is exercised at Check under the named test +
  gating C4-ci, and the three negation demonstrations in `build-notes.md` replace the
  flippable red.
- **Production reach:** N/A by design — these records have no live writer until #656–#659
  wire the store round trips; nothing on an existing path changes.
- **Citations expected:** cite `path:line` on the merged base for every change. Peer
  callsites Do MAY open: `crates/core/src/erasure.rs:120` (`supported(k, m)` — the
  validated-scheme predicate, and the #285 precedent that untrusted stored geometry is a
  typed error, not a panic; the `ChunkRef` half of leg 1i validates through it);
  `crates/core/src/metadata.rs:129-140` (the `ChunkRef` shape being validated). **Salvage:**
  `results/issue_692/iteration-v2/patch.diff` — take the lifecycle types and decoders, then
  FIX the recorded defects (the v2 reviewer found `PartRecordWire` accepting a raw
  `ChunkRef` at `multipart.rs:1538` and the lapsed-slot case *affirmed* rather than
  rejected at `:1657`) rather than re-shipping the reviewed shape;
  `results/issue_692/iteration-v2/check-review.md` holds those findings verbatim.
- **Prior-art check (triage cycles):** verified at Plan against `9dbcd72`: no
  `SessionRecord`, `SlotRecord`, `PartRecord` or `PartSummary` exists on `origin/main`; no
  open PR touches these paths. Closed/rejected: #654's two archived attempts and #692's own
  two — the v2 reviewer's T2 FAIL and T5 findings on these exact lines are this child's
  binding leg 1i, not suggestions.
- **Difficulty:** medium
- **Depends on:** child-1
- **Ordering note:** **Wave 1.** `Depends on: child-1` is a genuine build-on: this child's
  records dispatch through the `encode_record`/`decode_record` envelope child-1 lands, and
  both extend the same `multipart.rs` — so the dependency also wave-serialises the shared
  file. No `Conflicts with`: this child touches only `multipart.rs` and its own new test,
  neither of which #710 or #711 reads.
- **Disposition hint:** new-feature
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
- **Slug:** multipart-staging-retire-pending
- **Defect / goal:** the staging and retirement records — the two whose identity lives
  partly **in the key** — do not exist, and `PendingEntry` cannot yet carry ownership.
  This child lands `OwnedEntry`/`StagedPlacement` (`sidx:`, disjoint-staging rule
  `0016:475-491`), `PartNumberSet` and `RetirePayload` (`retire:*`, token grammar
  `0016:437-453`), the two **key-taking decode APIs** (`decode_owned_entry(key, bytes)`,
  `decode_retire_obligation(key, bytes)` — a decode that cannot see the key cannot
  validate against it, which is exactly how v2's shape failed its review), their
  `encode_record`/`decode_record` arms, and the ONE `metadata.rs` allowance: `PendingEntry`
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
  suffix (`0016:437-453`): a whole-session `Session`/`Parts` obligation under a per-part
  token, and a per-part `Chunks` obligation under a session-wide token, are **both**
  rejected (batch-review `multipart.rs:2024`);
  **(1i, EcScheme half)** `StagedPlacement`'s `EcScheme` is rejected unless
  `erasure::supported(k, m)` (`erasure.rs:120` — the #285 precedent: untrusted stored
  geometry like `ReedSolomon { k: 0, m: 1 }` is a typed error, not a panic);
  **(1e)** a `PendingEntry` with exactly one of `owner`/`staged` is torn and rejected at
  decode under both a `pending:` and a `sidx:` reading (v2 review,
  `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
  valid shapes;
  **(leg 2)** decode→encode is the identity on a legacy value: a legacy `pending:` value
  with neither new field re-encodes **byte-identically** — the identity every
  `require(key, encode(prior))` CAS in `metadata.rs` depends on (`metadata.rs:1368-1391`,
  ADR-0047:38-50; `0016:512-524`);
  **(leg 3 corollary, binding)** a `sidx:` value whose `staged` placement length does not
  match its scheme's fragment count **decodes** — placement length is the standing
  contextual-check example (`AGENTS.md:146-149`, ADR-0045, `0016:513-527`), quarantined
  by GC, not rejected; leg 1i validates the scheme's **geometry**, never the placement's
  **length**;
  **(leg 4, docs currency — SETTLED YES)** `PendingEntry` gains two persisted fields, so
  this child adds the short paragraph to
  `docs/design/architecture/05-building-block-view.md` § "The metadata model", mirroring
  the ADR-0047 optional-inode-fields paragraph at `:186-192` (`AGENTS.md:154-158`:
  a merge requirement, not a follow-up).
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_staging_retire.rs` + CRATEs
  core/custodian/dst/metadata-redb/server; the GREEN leg is `cargo test -p wyrd-core
  --test multipart_staging_retire`; the RED leg reverts production and the test fails to
  **compile** → **UNVERIFIABLE (exit 77), EXPECTED and PRE-DECLARED** as a §6 item.
  **Demonstrated red Do MUST capture instead (binding):** six named negations, one per
  binding rejection leg (1b, 1d, 1h-session-under-part, 1h-part-under-session, 1i-EcScheme,
  1e) — drop that single check, run the test, paste the failing output into
  `build-notes.md`, revert. Plus one for leg 2: remove ONE `skip_serializing_if` attribute
  and show the byte-identity leg fail. The leg-3 corollary is negated the other way: make
  the length-mismatched placement reject and show the assert-it-decodes leg fail. A leg
  green under its own negation is not load-bearing and must be rewritten.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, §6 row at `:137`;
  `0016:2802-2813`), over this child's category: **a stored record's fields may not
  disagree with each other OR WITH THE KEY THAT NAMES THEM, and the disagreement must
  surface as an error, never as a value** (ADR-0045). An owned entry attributed to the
  wrong session is staged data renewed or reclaimed under the wrong identity. A retirement
  payload under the wrong token — or the wrong *scope* of token — reclaims one generation's
  data while clearing another's obligation. Untrusted `EcScheme` geometry that decodes is
  the #285 panic class made durable. A torn `PendingEntry` turns a structural invariant
  into a convention. And a `PendingEntry` that does not re-encode byte-identically turns
  every existing `pending:` lease renewal into a permanent `Conflict`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality; builds on child-2's **merged** result. The
  one live-path touch is `PendingEntry`, and leg 2 proves the live path (every existing
  `pending:` CAS) is byte-identical — the extension is inert until #657 writes the first
  `sidx:` record.
- **Scope (one logical fix) / out of scope:** extend `crates/core/src/multipart.rs` with
  the staging/retirement types, both key-taking decoders, and their dispatch arms; the
  ONE `PendingEntry` hunk in `crates/core/src/metadata.rs` described under Goal — nothing
  else in that file changes; the **explicitly ALLOWED mechanical ripple** in the 8 files
  that construct or copy `PendingEntry` (`crates/core/src/write.rs`,
  `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs`) — `owner: None, staged: None` initializer lines
  / clone-instead-of-copy fixes only, **≤ 8 changed lines per file, no logic change, no
  new function**; a ninth ripple file or a non-mechanical hunk means the seam is wrong:
  STOP and hand back. Plus the ONE docs paragraph of leg 4. Keep every hunk in the shared
  files as small as briefed — a wider hunk is a needless rebase surface for #710/#711
  whichever folds first. Budget ≈ 900 added lines / 12 files. / out of scope: custodian
  **source** code (`crates/custodian/src/` untouched — only its tests' initializers);
  every `docs/design/` file except the one `05-building-block-view.md` paragraph — ADRs,
  proposals and specs untouched (INTEGRATION §2 immutability); the outcome enums, answer
  table, digests, `sha2` (#693 — no `Cargo.toml`/`Cargo.lock` change); knob values
  (#655); store round trips (#656–#659); reaper/windows (#625).
- **Budget:** ≤ 900 added semantic lines (module extension ≈ 400, `metadata.rs` ≈ 45,
  test ≈ 400, ripple ≈ 40 mechanical, docs ≈ 15) across exactly **12** files — 4
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
  gating C4-ci, and the eight negation demonstrations in `build-notes.md` replace the
  flippable red.
- **Production reach:** the ONE live-path touch in this whole 3-child chain. `PendingEntry`
  is on the existing `pending:` write/renew path, so (a) what honours the seam now is the
  legacy shape itself — both new fields absent — and leg 2 proves that path re-encodes
  **byte-identically**, i.e. the extension is genuinely inert; (b) the production wiring
  that writes a `sidx:` record with `owner`/`staged` set lands in **#657**, which needs the
  store round trips (#656–#659) first; (c) the torn-shape rejection (leg 1e) is exercised
  load-bearingly by hand-authored values in the named test, not by dead scaffolding.
- **Citations expected:** cite `path:line` on the merged base for every change. Sources Do
  MUST open: `0016:475-491` (the `sidx:` disjoint-staging rule), `0016:437-453` (the
  retirement-token grammar leg 1h enforces), `0016:512-527` (the `skip_serializing_if`
  identity argument and the placement-length contextual boundary), ADR-0045, ADR-0047:38-50.
  Peer callsites Do MAY open: `crates/core/src/metadata.rs:1368-1391` (the
  identity-preserving optional-field precedent the `PendingEntry` hunk must mirror);
  `crates/core/src/erasure.rs:120` (`supported(k, m)`, leg 1i's predicate and the #285
  precedent); `docs/design/architecture/05-building-block-view.md:186-192` (the ADR-0047
  paragraph leg 4 mirrors — match its voice and length, do not restate the proposal).
  **Salvage:** `results/issue_692/iteration-v2/patch.diff` — take the staging/retirement
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
- **Difficulty:** medium
- **Depends on:** child-2
- **Ordering note:** **Wave 2 — terminal.** `Depends on: child-2` is a genuine build-on
  (this child's `StagedPlacement` decode reuses the validated-nested-type pattern child-2
  establishes for `ChunkRef`, and its records dispatch through child-1's envelope), and it
  also wave-serialises the shared `multipart.rs`. **This child alone carries the chain's
  external conflicts.** #682 was SPLIT on 2026-08-08 into **#710** (shares
  `core/src/metadata.rs` — its `MAX_VALUE_BYTES` enforcement vs this child's `PendingEntry`
  region) and **#711** (shares BOTH `core/src/metadata.rs`, its `repoint_chunk` primitive,
  AND `dst/tests/custodian.rs`, its substantive edits vs this child's mechanical
  initializer lines). Because the proposal's ordering fields may only name sibling labels,
  `Conflicts with: 710, 711` is added to THIS child's materialised `brief.md` at split
  acceptance — it must never share a wave with either. #693 (`Depends on: 692`) and #655
  (blocked by #693) are repointed at THIS child at the same moment. **Cite by symbol, not
  by line number**, in `metadata.rs`: the base will have advanced under #710/#711.
- **Disposition hint:** new-feature
<!-- pdca:end child-3 -->
