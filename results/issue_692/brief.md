# Brief — multipart record family + validating decoders (654 split 2/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636). **Re-planned 2026-08-09** after the
> 2026-08-08 sign-off answered `iterate-plan`: *"Re-plan and split via `pdca split 692`
> rather than attempting another iterate-do on this cut."* Two attempts are archived
> (`iteration-v1/`, `iteration-v2/`); v2 was 106 KB / 11 files / ~2,140 added lines, over
> the 100 KB size backstop, and its findings were substantive rather than cosmetic.
> **THIS BRIEF IS THE SPLIT INPUT — it is not meant to be built as one slice.**
>
> **The design is settled and normative:** proposal **0016** on `origin/main` @ `9dbcd72`.
> Do MUST read: §1 the records `0016:333-527` · §2 the state machine `0016:528-602` (the
> *states* — the answer table is #693's) · the `sidx:` disjoint-staging rule `0016:475-491`
> · the knob table `0016:1463-1480` (the *shape* of the profile tuple and every knob's
> **valid range** — the chosen *values* are #655's). Pure code, no store I/O. Material is
> **salvaged** from `results/issue_692/iteration-v2/patch.diff`.
>
> **Base has advanced since v2.** Child-1 (#691, the key grammar) is **merged** —
> `d986069`, PR #703, in `origin/main @ 9dbcd72`. `crates/core/src/multipart.rs` is 854
> lines on the base and already exports `RecordError`, the validated identity types
> (`UploadId`, `AttemptId`, `PartNumber`, `SlotIndex`, `Digest`) and every key
> constructor/parser. Nothing in this brief re-derives them.

- **Slug:** multipart-record-family
- **Defect / goal:** the key grammar exists (#691, merged) but no record **values** do.
  This slice lands the record family and its **validating decoders**: `Budget` (the profile
  tuple and its pure derivations), `AdmissionRecord` (`mpuctl`), `SessionRecord` with
  `SessionState`/`PublishTarget`/`Completion` (`mpu:`), `SlotRecord` (`slot:`),
  `PartRecord`/`PartSummary` (`part:`/`psum:`), `OwnedEntry`/`StagedPlacement` (`sidx:`),
  `PartNumberSet` and `RetirePayload` (`retire:*`), plus `encode_record`/`decode_record` —
  and the `PendingEntry` extension in `metadata.rs` (the two optional fields `owner` and
  `staged`) with its mechanical cross-crate ripple. After this slice, an internally
  inconsistent stored record is a decode error, not a value.
- **Success criterion:** every record type round-trips `encode`/`decode`, and a
  structurally invalid value is rejected at decode with a typed error (ADR-0045). Nine
  legs are **BINDING**, each proven with a hand-authored torn value. Legs 1a–1e are
  carried-forward MUST-FIXes from the v2 sign-off and its batch review; legs **1f–1i are
  NEW at this re-plan**, settling the four v2 blockers that `iterate-plan` was answered
  over:
  1. **Relational identity checks** — two stored spellings of one quantity may not
     disagree:
     (a) an `AdmissionRecord` whose `max_sessions` disagrees with what its own stored
     `profile` tuple derives is rejected (`0016:1469-1470`: `U_ref` and
     `MAX_SESSIONS = ⌊W_ref/U_ref⌋` are functions of the tuple, implemented HERE);
     (b) `decode_owned_entry` **takes the `sidx:` key** and rejects a payload whose
     `owner` differs from the key's upload id (v2 review, `multipart.rs:1555`);
     (c) a `SessionRecord` whose `publish_target.parent`/`name` disagrees with the
     session's own `parent`/`object` is rejected (v2 review, `multipart.rs:1258`);
     (d) `decode_retire_obligation` **takes the `retire:` key** and rejects a payload
     whose mode or generation identity disagrees with the key's token — a
     generation-scoped payload under a session token, and vice versa, are both errors
     (v2 review, `multipart.rs:1789/1800`; the archived test at `multipart_records.rs:1108`
     *affirmed* this case — it must now reject);
     (e) a `PendingEntry` with exactly one of `owner`/`staged` is **torn** and rejected at
     decode, under both a `pending:` and a `sidx:` reading (v2 review,
     `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
     valid shapes.
  1'. **Nested and range validation — the four v2 blockers, settled here:**
     (f) **`Budget::new` enforces BOTH ends of every knob range in `0016:1463-1480`**, not
     just the lower: `max_part_chunks` must satisfy the value-ceiling rule **and** the
     `B_ops` clamp (batch-review `multipart.rs:1041`), and `max_staged_chunks` must lie in
     `[max_part_chunks, MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS]` — the publishable ceiling
     (batch-review `multipart.rs:1027`). A profile above either end is rejected;
     (g) the **`sidx:` scan bound counts committed staging entries too**, not only
     in-flight chunks, so an accepted profile can never exceed `SCAN_CAP/2` and leave
     teardown's single scan silently incomplete (batch-review `multipart.rs:1074`);
     (h) **the retire session-token arm honours the token's optional `:<part>:<attempt>`
     suffix** (`0016:437-453`): a whole-session `Session`/`Parts` obligation under a
     per-part token, and a per-part `Chunks` obligation under a session-wide token, are
     **both** rejected — decoding either makes the drain act on the wrong scope
     (batch-review `multipart.rs:2024`);
     (i) **nested types validate at decode, not just the outer record** (reviewer C5 +
     T2 FAIL): `StagedPlacement`'s `EcScheme` is rejected unless
     `erasure::supported(k, m)` (`erasure.rs:120` — the #285 precedent: an untrusted
     `ReedSolomon { k: 0, m: 1 }` read back from stored metadata must be a typed error);
     `PartRecord` validates each `ChunkRef` structurally rather than accepting a raw one
     (`multipart.rs:1538`); and a `SlotRecord` with `lease_expiry_millis <=
     reserved_at_millis` — a slot born already lapsed — is rejected
     (`multipart.rs:1657`).
  2. **Decode→encode is the identity on a legacy value:** a legacy `pending:` value with
     neither new field re-encodes **byte-identically** — the `skip_serializing_if`
     identity every `require(key, encode(prior))` CAS in `metadata.rs` depends on
     (`metadata.rs:1368-1391`, ADR-0047:38-50; `0016:512-524`).
  3. **The identity/occupancy boundary holds** (a deliberate Plan ruling, unchanged from
     v2): a decoded `AdmissionRecord` whose `count` exceeds its `max_sessions` still
     **decodes** — occupancy above a lowered cap is a legitimate live state that admission
     (a later slice) refuses to grow, not a decode error; rejecting it would make a durable
     record unreadable the day the profile is lowered (`0016:499-511` states this for the
     whole record set: decode validates against **format** maxima, never live knobs; the
     `MAX_ROOT_SEGMENTS` boundary, `metadata.rs:312-321`). Assert it decodes. Identity
     relations (legs 1a–1e) are binding; occupancy relations are not. This settles the v2
     reviewer's "count above the derived admission cap" finding the other way.
     **Corollary, also binding:** a `sidx:` value whose `staged` placement length does not
     match its scheme's fragment count **decodes** — placement length is the standing
     *contextual*-check example (`AGENTS.md:146-149`, ADR-0045, `0016:513-527`), quarantined
     by GC rather than rejected. Leg 1i validates the scheme's **geometry**, never the
     placement's **length**; do not conflate them.
  4. **Docs currency — SETTLED YES at this re-plan** (this is the standing C1 NEEDS-HUMAN,
     closed here rather than re-asked every round): `PendingEntry` gains two **persisted**
     fields, and `AGENTS.md:154-158` makes updating the living architecture doc in the same
     PR *"a merge requirement, not a follow-up"*. The slice that touches `PendingEntry`
     MUST add a short paragraph to `docs/design/architecture/05-building-block-view.md`
     § "The metadata model", mirroring the ADR-0047 optional-inode-fields paragraph already
     there at `:186-192` (same `serde(default)` + `skip_serializing_if` + byte-identical
     re-encode reasoning). This is the ONLY `docs/` edit any child may make; ADRs,
     proposals and specs stay untouched (INTEGRATION §2 immutability).
- **Falsifiability:** RED is criterion-ABSENCE — **born-at-tier**, as #691 was. C4-verify
  classifies an `ADDED_TEST crates/core/tests/*.rs` plus the touched CRATEs (v2's
  `--classify` dry-run on the synthetic 11-file set confirmed this discriminator on the
  real gate); the GREEN leg is `cargo test -p wyrd-core --test <the child's test>`; the RED
  leg reverts production and the test fails to **compile** → **UNVERIFIABLE (exit 77),
  EXPECTED and PRE-DECLARED** as a §6 item. **Demonstrated red Do MUST capture instead
  (binding):** one named negation per binding leg the child carries — drop that single
  check, run the test, paste the failing output into `build-notes.md`, revert. A leg that
  stays green under its own negation is not load-bearing and must be rewritten. Each child
  builds on its predecessor's **merged** result, so its patch applies on a base that
  already contains the scaffolding it imports (INTEGRATION §2, `wave_mode = "merge"` with
  `auto_merge = false` — the human merges at each wave boundary and the re-run verifies it).
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, §6 row at `:137`;
  `0016:2802-2813`), over this slice's category: **a stored record's fields may not
  disagree with each other or with the key that names them, and the disagreement must
  surface as an error, never as a value** (ADR-0045). An admission record whose
  `max_sessions` does not match its own `profile` admits sessions past the memory bound the
  reconcile pass is sized for — a fleet-wide failure admitted by one unvalidated field. A
  profile accepted above its settled range produces a maximal part whose commit and whose
  compensation both time out permanently, stranding the slot forever (`0016:1466`, the
  `B_ops` clamp). An owned entry attributed to the wrong session is staged data renewed or
  reclaimed under the wrong identity. A retirement payload under the wrong token — or the
  wrong *scope* of token — reclaims one generation's data while clearing another's
  obligation. A torn `PendingEntry` turns a structural invariant into a convention.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; base verified `9dbcd72`)
- **Depends on:** 691
- **Conflicts with:** 710, 711
- **Ordering note:** #691 (key grammar) is COMPLETE **and merged** (PR #703 → `9dbcd72`),
  so this dependency is already satisfied; it is kept to record the lineage. **#682 was
  SPLIT on 2026-08-08 into #710 (ceiling/certification) and #711 (segmented repoint)**, so
  the conflict follows those ids: **#710 shares `core/src/metadata.rs`** (its
  `MAX_VALUE_BYTES` enforcement vs the `PendingEntry` region) and **#711 shares both**
  `core/src/metadata.rs` (its `repoint_chunk` primitive) **and** `dst/tests/custodian.rs`
  (its substantive edits vs the mechanical initializer lines here). **On the split, this
  conflict belongs to the ONE child that touches `metadata.rs` — not to all three**; the
  other two touch only `multipart.rs` and their own new test, which neither #710 nor #711
  reads. #693 (`Depends on: 692`) and #655 (blocked by #693) must be repointed at the
  TERMINAL child at split acceptance. Keep every hunk in the shared files as small as
  briefed — a wider hunk is a needless rebase surface for #710/#711 whichever folds first.
- **Surfaces:** data
- **Difficulty:** medium   (per child. The parent as one slice would be `high` — 12 files,
  6 crates — which is the reason it is being split. Each child is one module extension
  plus one new test file; only the `sidx:`/retire child carries the cross-crate ripple, and
  every ripple hunk is a mechanically checkable ≤8-line initializer touch)
- **Scope:** extend `crates/core/src/multipart.rs` with the record family and validating
  decoders listed under Goal, including `encode_record`/`decode_record` and all nine
  binding checks; **key-taking decode APIs** for the two records whose identity lives partly
  in the key (`decode_owned_entry(key, bytes)`, `decode_retire_obligation(key, bytes)`) — a
  decode that cannot see the key cannot validate against it, which is exactly how v2's shape
  failed its review. In `crates/core/src/metadata.rs`, **ONE allowance**: `PendingEntry`
  (`metadata.rs:1528`) gains `owner: Option<multipart::UploadId>` and
  `staged: Option<multipart::StagedPlacement>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`, doc comments per the v2 hunk), drops `Copy` (forced — `UploadId` is
  a `String` newtype), and gains the torn-shape rejection (leg 1e) in a manual
  `Deserialize`; nothing else in that file changes. **The mechanical ripple is explicitly
  ALLOWED** (this Plan's answer to the v2 C1 finding that a 5-file ceiling was unbuildable):
  the 8 files that construct or copy `PendingEntry` — `crates/core/src/write.rs`,
  `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs` — may each gain the mechanical `owner: None,
  staged: None` initializer lines / clone-instead-of-copy fixes, **≤ 8 changed lines per
  file, no logic change, no new function**. A ninth ripple file or a non-mechanical hunk in
  any of them means the seam is wrong: STOP and hand back. Plus the ONE docs paragraph of
  criterion leg 4.
  / out of scope: the outcome enums, answer table, `Verb`, `MultipartEtag`, digests and
  `sha2` (#693's — no `Cargo.toml`/`Cargo.lock` change here); the knob **values** (#655);
  every store round trip (#656–#659); reaper/windows (#625); custodian **source** code —
  `crates/custodian/src/` untouched (only its *tests'* initializers); every `docs/design/`
  file except the one `05-building-block-view.md` paragraph named in criterion leg 4 —
  ADRs, proposals and specs are untouched.
- **Budget:** ≤ 2,150 added semantic lines across ≤ **12** files IN TOTAL FOR THE PARENT —
  i.e. this is the envelope the split must carve, not a per-child figure. v2 spent 2,140
  across 11 and was refused as oversized, and legs 1f–1i only add validation, so **no
  child may reach the 100 KB size backstop**: each child budgets its own ceiling and the
  three must sum to no more than this.
- **Repro instruction:** n/a — new functionality on a base (`9dbcd72`) where only the key
  grammar exists.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). The `docs-renderer` leg is load-bearing for the child carrying criterion leg 4. Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** one **NEW** `crates/core/tests/<name>.rs` per child — a new file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator; v2's `--classify` dry-run
  confirmed). Co-located unit tests may ship in addition. The parent's own name was
  `crates/core/tests/multipart_records.rs`; each child takes a distinct name so two
  children never collide on one path.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as #691:
  UNVERIFIABLE RED pre-declared, everything built is exercised at Check under the named
  test + gating C4-ci, and the per-leg negation demonstrations in `build-notes.md` replace
  the flippable red.
- **Production reach:** N/A by design — consumers are #656–#659, separately filed. The one
  live-path touch is `PendingEntry`, and criterion leg 2 proves the live path (every
  existing `pending:` CAS) is byte-identical — the extension is inert until #657 writes the
  first `sidx:` record.
- **Citations expected:** cite `path:line` on the target base for every change. Sources Do
  MUST open: the 0016 sections named in the header; ADR-0045; ADR-0047:38-50 (the
  `Option`+`default`+`skip_serializing_if` identity rule). Peer callsites Do MAY open:
  `crates/core/src/erasure.rs:120` (`supported(k, m)` — the validated-scheme predicate leg
  1i must call, and the #285 precedent that untrusted stored geometry is a typed error, not
  a panic); `crates/core/src/metadata.rs:1368-1391` (the identity-preserving optional-field
  precedent the `PendingEntry` hunk must mirror); `metadata.rs:312-321` and
  `metadata.rs:327-352` (`MAX_VALUE_BYTES` / `MAX_ROOT_VALUE_BYTES` / `MAX_ROOT_SEGMENTS` —
  the format maxima leg 1f's ranges are computed against, and the occupancy-not-at-decode
  boundary of leg 3); `docs/design/architecture/05-building-block-view.md:186-192` (the
  paragraph leg 4 mirrors). **Salvage — the primary lever:**
  `results/issue_692/iteration-v2/patch.diff` — take the record types, wire structs and
  decoders (`Budget` through `RetirePayload`, added-file lines ~846–1818) and its
  `metadata.rs` hunk; then **fix the recorded defects** (legs 1a–1i — the reviews found
  each check missing, out of range, or the API unable to see the key) rather than
  re-shipping the reviewed shape. `results/issue_692/review-batch.md` holds the four
  blockers verbatim; `iteration-v2/check-review.md` the reviewer's C5/T2/T5 findings.
- **Prior-art check (triage cycles):** verified at this Plan against `9dbcd72`: no record
  type or decoder exists on `origin/main` — `git -C ../wyrd show origin/main:crates/core/src/multipart.rs`
  is 854 lines of keys and identity types only, ending at the retirement-key parsers; `git
  -C ../wyrd log origin/main -- crates/core/src/metadata.rs` shows no multipart-related
  commit; no open PR touches these paths. Closed/rejected: the #508 line, #636, #654's two
  archived attempts, and **this bundle's own two** (`iteration-v1/`, `iteration-v2/`) — the
  v2 batch review's four blockers and the reviewer's C5/T2/T5 findings are binding legs
  1f–1i above, not suggestions.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
