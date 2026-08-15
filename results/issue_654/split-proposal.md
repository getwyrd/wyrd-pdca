<!-- pdca:split-proposal v1 -->
# Split proposal — issue 654

## Why this slice is oversized

Two Do rounds proved it: the v2 attempt produced a **1,408-semantic-line production module
plus a 1,468-semantic-line test file** (189 KB patch, 15 files), and its blocking findings
were quality findings that never got review bandwidth because the diff drowned them —
five relational identity checks missing at decode, the flagship `007` fixed-width adversary
absent, tests not exercising their claims. The module itself falls into three
dependency-ordered blocks with a clean seam between each: (1) the key grammar and the
validated identity types it is built from; (2) the record family and its validating
decoders — which also carries the one cross-crate ripple, `PendingEntry` losing `Copy`;
(3) the verb × state answer table and the two digests. Each block is independently
shippable — its own invariants, its own test file, its own PR — and each lands well inside
the reviewability ceiling the #508/#636 rejections established. The v2 patch
(`results/issue_654/iteration-v2/patch.diff`) is **salvage** for all three: it is already
shaped this way internally, and each child names exactly which of its sections to take and
which recorded defect to fix in them.

Four decisions the human settled at this Plan, pinned into the children so no Do round or
review relitigates them: **(1)** `PART_NUMBER_WIDTH = 6` — Wyrd targets other storage
protocols beyond S3 (the gateway seam is protocol-neutral by ADR-0046/0047), Azure's
documented staging ceiling (100,000 uncommitted blocks) exceeds a width-5 key space by
exactly one, and widening later is a stored-format migration; **(2)** the public outcome
enums are **exhaustive** (no `#[non_exhaustive]`) — a new outcome variant must break every
gateway's wire-mapping table at compile time rather than fall into a `_ =>` arm that maps
it to a silently wrong status; **(3)** the `PendingEntry` extension stays in this work (not
deferred to #657), carried by child-2 with the mechanical ripple explicitly allowed —
the v2 sign-off found the parent's ≤5-file ceiling unbuildable as written; **(4)** three
children, not two: two would re-fuse the ripple, the five relational checks and the
25-cell table into one lump, which is the shape that already failed twice.

## Wave sketch

Strict chain — three waves, one child each: **child-2 depends on child-1** (the record
types are built from child-1's validated identity types and keyed by its grammar) and
**child-3 depends on child-2** (the answer table is a function of child-2's
`SessionState`/`Completion`; the digests take child-1's `PartNumber`/`Digest`). The
dependency is also what serialises the shared file: all three children extend the same
`crates/core/src/multipart.rs`, so no two may build blind on one base — the chain gives
each wave the previous child's folded result (`wave_mode = "merge"`: each non-final wave's
PR is merged to `main` before the next builds). Between siblings the `Depends on:` chain
already forces distinct waves.

**The rest of the batch (654's chain is not alone in this run — 655, 681, 682):**
- **#681** shares no file with any child (its surface is `crates/custodian/src/*`;
  child-1's is `crates/core` only), so it may share wave 0 with child-1. It is currently
  UNPLANNED with two archived iterations — it needs its own re-Plan before the batch runs;
  that is a separate Plan session, not this proposal's.
- **#682 CONFLICTS with child-2** — its named file allocation includes
  `core/src/metadata.rs` and `dst/tests/custodian.rs`, both also in child-2's set (the
  `PendingEntry` hunk + its mechanical ripple). Both bundles land in wave 1 by their
  dependency edges alone (682 ← 681, child-2 ← child-1), i.e. they WOULD build blind on
  two shared files. The proposal's ordering fields can only name sibling labels
  (validator rule), so the fix is a post-accept step: **add `Conflicts with: 682` to the
  materialised child-2 brief** — `conflict_map` is symmetric, one side suffices, and
  `compute_waves` then orients the pair into different waves. 682's own custodian test
  files (`reconstruction/rebalance/segmented_map_repoint`) do not overlap child-2's three
  (`gc/restore_reconcile/segmented_map_consumers`).
- **#655** (knob values, currently `Depends on: 654`) must be re-pointed at child-3 after
  acceptance — it appends constants to the same module and asserts them against child-1's
  key-space bounds and child-2's `Budget` derivations, so it belongs in the wave after the
  whole chain. No file overlap with 682 (its set is `multipart.rs` + its own new test).

<!-- pdca:child child-1 -->
# Brief — multipart key grammar + validated identity types (654 split 1/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636), split per its 2026-08-05 sign-off.
> **The design is settled and normative:** proposal **0016**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on `origin/main` @
> `339da46` (merged by PR #627). Do MUST read: §1 the records `0016:333-527` (for the key
> column only — the record *values* are the next child's), the `retire:` `<token>` grammar
> `0016:359-380`, the `sidx:` disjoint-staging rule `0016:475-491`. Pure code, no store
> I/O. Material is **salvaged** from `results/issue_654/iteration-v2/patch.diff`, not
> re-derived.

- **Slug:** multipart-key-grammar
- **Defect / goal:** nothing multipart exists in `crates/core`. This child lands the
  protocol's **key space and the validated types it is spelled in**: the module
  `crates/core/src/multipart.rs` with `RecordError`, the identity newtypes (`UploadId`,
  `AttemptId`, `PartNumber`, `SlotIndex`, `Digest` + `hex_lower`), every key prefix, the
  canonical fixed-width key constructors / range prefixes / parsers for `mpuctl`,
  `mpu:<id>`, `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`,
  `sidx:<id>:<part>:<chunk>`, and the `retire:bytes:<token>` / `retire:records:<token>`
  key grammar. After this child, every key the protocol will ever write has exactly one
  spelling, and a non-canonical spelling parses as no key at all.
- **Success criterion:** the NEW file `crates/core/tests/multipart_keys.rs` passes. Every
  leg is pure — no store, no async, no fixture beyond literals:
  1. **Round-trip + canonical rejection, per keyed class.** For each of `mpu:<id>`,
     `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`, `sidx:<id>:<part>:<chunk>`,
     `retire:bytes:<token>`, `retire:records:<token>`: `parse(key(x)) == x` over a table,
     AND each parser rejects with a typed error: a `+` sign, **every short width including
     the bare `7` and the padded-but-short `007`** (the carried-forward MUST-FIX the v2
     review found missing — `multipart_records.rs:162` in the archived attempt), an
     over-wide spelling, a non-decimal body, an empty upload id, an upload id containing
     `:`, a malformed/truncated key, a trailing component, and non-UTF-8 bytes. Two
     spellings of one record must never both parse.
  2. **Byte-lexicographic order equals numeric order** over a fixed-width series that
     crosses every digit-width boundary (1, 9, 10, 99, 100, …, max), for both the slot
     index and the part number — exactly the property `metadata.rs:270-273` states for
     `SEG_INDEX_WIDTH`.
  3. **No prefix is a prefix of another**, over the full set: the seven above plus the
     pre-existing `inode:`, `dirent:`, `pending:`, `bucket:`, `orphan:`
     (`metadata.rs:30-70`), `seg:`, `seggrp:` (`metadata.rs:293-300`) and
     `desired:dserver:` (`custodian/src/desired_state.rs:33`). The two named near-misses:
     `scan("mpu:")` must not return the `mpuctl` singleton, and `sidx:` must not be
     reachable from `scan("pending:")` (`0016:475-491`).
  4. **The `retire:` token grammar** distinguishes its two token forms (session-scoped vs
     generation-scoped, `0016:359-380`) at parse, round-trips both, and rejects a token of
     neither form.
  5. **The pinned format constants hold:** `PART_NUMBER_WIDTH == 6`,
     `MAX_PART_NUMBER == 999_999`, `SLOT_INDEX_WIDTH == 6`, `MAX_SLOT_INDEX == 999_999`,
     and each parser's width equals its constant.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier; nothing multipart exists on
  `origin/main` (verified: `git -C ../wyrd grep "mpuctl\|MPU_PREFIX" origin/main -- crates/`
  → no matches). C4-verify classifies this patch `ADDED_TEST
  crates/core/tests/multipart_keys.rs` + `CRATE crates/core` (confirmed by `--classify`
  dry-run on the synthetic file set): the GREEN leg is `cargo test -p wyrd-core --test
  multipart_keys`; the RED leg reverts `multipart.rs`/`lib.rs`, the test then fails to
  compile, and the gate reports **UNVERIFIABLE (exit 77) — EXPECTED and PRE-DECLARED**, a
  §6 sign-off item, not a defect. **The demonstrated red Do MUST capture instead
  (binding):** two named negations, each run against the test with the failing output
  pasted into `build-notes.md`, then reverted — (a) make one fixed-width parser accept a
  short `007` spelling (leg 1 must fail); (b) spell one prefix without its trailing
  separator (`mpu` for `mpu:` — leg 3's near-miss must fail). A leg that stays green under
  its negation is not load-bearing and must be rewritten. No `#![cfg(...)]` in the test
  file (the gate's vacuous-green hazard, `run-verify.sh:445`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an
  acceptable cost** (`docs/principles.md:109`, §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`; `0016:2802-2813`), over this child's category: **the key space
  a lifecycle is addressed by**. A record that can be spelled two ways is a record that can
  be lost: if `slot:<id>:7` and `slot:<id>:000007` both parse, a `require_absent` guard
  admits past its cap and a bounded range scan misses a record that exists — residue
  nothing enumerates, and therefore nothing reclaims. A namespace that overlaps another is
  a namespace that gets swept by it. Canonicality and disjointness are enforced **at
  decode**, not by convention at call sites (ADR-0045).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: no live milestone
  integration branch; verified `git -C ../wyrd rev-parse origin/main` → `339da46`)
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** Wave 0 of this chain. The next two children extend the module this
  child creates — they depend on it and each lands in a later wave. Sibling #655 must be
  re-pointed to depend on the LAST child of this chain (it asserts knob values against this
  child's key-space bounds and edits the same file).
- **Surfaces:** data
- **Difficulty:** medium   (three files, zero existing call-sites — but every later slice
  and five sibling issues build on these key formats, and a wrong width or an overlapping
  prefix is a stored-format migration to undo; rated up for that forward reach)
- **Scope:** ONE new module `crates/core/src/multipart.rs` (flat sibling of `metadata.rs`;
  the workspace has no directory modules) + one `pub mod multipart;` line in
  `crates/core/src/lib.rs`: `RecordError` (typed, `Display`, `Error`), the validated
  newtypes `UploadId` / `AttemptId` (token grammar via `TOKEN_HEX_LEN` =
  `metadata::SEG_NONCE_HEX_LEN`, `is_token` / `require_token`), `PartNumber` /
  `SlotIndex` (validating constructors + `Deserialize` that routes through them),
  `Digest` ([u8; 32], lowercase-hex `Serialize`/`Deserialize`, `hex_lower`), the prefix
  constants (`MPUCTL_KEY`, `MPU_PREFIX`, `SLOT_PREFIX`, `PART_PREFIX`, `PSUM_PREFIX`,
  `SIDX_PREFIX`, `RETIRE_BYTES_PREFIX`, `RETIRE_RECORDS_PREFIX`), `fixed_width_u32` /
  `canonical_decimal` / `split_key`, the key constructors + `*_range` prefixes + `parse_*`
  parsers for all seven keyed classes, `RetireMode` / `RetireToken` /
  `parse_retire_mode` / `parse_retire_key` / `retire_key` / `retire_session_range`.
  **Plan decisions pinned here, not Do's to revisit:** `PART_NUMBER_WIDTH = 6`
  (protocol-neutral format headroom — S3 caps at 10,000 per its wire protocol, Azure block
  blobs at 50,000 committed / 100,000 staged; the gateway seam is protocol-neutral by
  ADR-0046, so the *format* must clear every known protocol ceiling with margin while each
  protocol's cap is enforced at admission as *capacity*, mirroring `SEG_INDEX_WIDTH` vs
  `MAX_ROOT_SEGMENTS`, `metadata.rs:270-321`); `SLOT_INDEX_WIDTH = 6` (0016's clamp
  arithmetic reaches ≈524,288, `0016:1471`). Doc-comment both with this reasoning.
  / out of scope: every record **value** type, `encode_record` / `decode_record` and all
  validating value decoders (child-2's); the outcome enums, answer table and digests
  (child-3's — `Digest` the *type* is here, `sha2` the *dependency* is child-3's, so no
  `Cargo.toml` change in this child); every store round trip (#656–#659); the knob values
  (#655); S3 verbs/XML/status (#508); `crates/core/src/metadata.rs` untouched; no file
  beyond the three named.
- **Budget:** ≤ 1,150 added semantic lines total (module ≈ 600, test ≈ 550) across exactly
  **3 files**: `crates/core/src/multipart.rs` (new), `crates/core/src/lib.rs` (one line),
  `crates/core/tests/multipart_keys.rs` (new). A fourth file means the shape is wrong —
  STOP and hand back.
- **Repro instruction:** n/a — new functionality; nothing multipart exists on the base
  (`git -C ../wyrd ls-tree origin/main crates/core/src/` →
  `erasure|lib|metadata|placement|read|repair|write`).
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_keys.rs` — a **NEW** file, not optional:
  C4-verify's discriminator is an added `*/tests/*.rs` (`run-verify.sh:97-98`); a
  co-located test would silently degrade the gate to green-only. Unit tests co-located in
  `multipart.rs` may ship in addition; the five legs live in the named file.
- **Verification posture:** declared — **born-at-tier (posture (a))**. "Red" is criterion
  absence; C4-verify's RED leg is a compile failure reported UNVERIFIABLE (exit 77 → §6),
  pre-declared above. Everything this child ships is built AND exercised at Check: every
  leg runs under `cargo test -p wyrd-core --test multipart_keys` and under gating C4-ci
  (`cargo xtask ci`). The two named negation demonstrations in `build-notes.md` replace
  the flippable red.
- **Production reach:** N/A — no production consumer by design; consumers are the sibling
  children and #656–#659, all separately filed.
- **Citations expected:** cite `path:line` on `origin/main` @ `339da46` for every change.
  Sources Do MUST open: 0016 sections in the header; ADR-0045 (parse-don't-validate);
  ADR-0046 (disjoint prefixes / protocol-neutral seam). Peer callsites Do MAY open and
  should mirror: `crates/core/src/metadata.rs:270-300` (`SEG_INDEX_WIDTH`,
  `MAX_SEGMENT_INDEX`, why the key space is enforced at decode);
  `crates/core/src/metadata.rs:1230-1300` (`seg_key`/`seg_range_prefix`/`parse_seg_key` —
  the house shape for constructor + range + parser + typed error). **Salvage — the primary
  lever:** `results/issue_654/iteration-v2/patch.diff`, sections `multipart.rs` lines
  ~86–845 of the added file (RecordError through the retire key grammar): take them,
  change `PART_NUMBER_WIDTH` from 5 to the pinned 6, add the missing short-width
  (`007`-class) rejection to every fixed-width parser's test table, and leave every record
  value type behind for child-2.
- **Prior-art check (triage cycles):** by affected file path over merged history and
  closed/rejected work, verified at this Plan: `crates/core/src/multipart.rs` and
  `crates/core/tests/multipart_keys.rs` do not exist on `origin/main`; no multipart symbol
  anywhere in `crates/` (grepped `mpuctl`, `MPU_PREFIX`, `mod multipart` → none); no open
  PR touches these paths. Closed/rejected: the #508 line (7 attempts, rejected on
  reviewability), #636 (3 rounds, discontinued for size with the explicit split
  instruction), #654's own two archived attempts (`iteration-v1/`, `iteration-v2/` — the
  second is this child's salvage; its recorded defects are distributed across the three
  children's binding legs).
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# Brief — multipart record family + validating decoders (654 split 2/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636), split per its 2026-08-05 sign-off.
> **The design is settled and normative:** proposal **0016** on `origin/main` @ `339da46`.
> Do MUST read: §1 the records `0016:333-527` · §2 the state machine `0016:528-602` (the
> *states* — the answer table is child-3's) · the `sidx:` disjoint-staging rule
> `0016:475-491` · the knob table `0016:1463-1480` (the *shape* of the profile tuple only —
> values are #655's). Pure code, no store I/O. Material is **salvaged** from
> `results/issue_654/iteration-v2/patch.diff`.

- **Slug:** multipart-record-family
- **Defect / goal:** the key grammar exists (previous child) but no record **values** do.
  This child lands the record family and its **validating decoders**: `Budget` (the
  profile tuple and its pure derivations), `AdmissionRecord` (`mpuctl`), `SessionRecord`
  with `SessionState`/`PublishTarget`/`Completion` (`mpu:`), `SlotRecord` (`slot:`),
  `PartRecord`/`PartSummary` (`part:`/`psum:`), `OwnedEntry`/`StagedPlacement` (`sidx:`),
  `PartNumberSet` and `RetirePayload` (`retire:*`), plus `encode_record`/`decode_record` —
  and the `PendingEntry` extension in `metadata.rs` (the two optional fields `owner` and
  `staged`), with its mechanical cross-crate ripple **explicitly allowed**. After this
  child, an internally inconsistent stored record is a decode error, not a value.
- **Success criterion:** the NEW file `crates/core/tests/multipart_records.rs` passes.
  Every leg pure — no store, no async:
  1. **Every record decode is validating** (ADR-0045): each record type round-trips
     `encode`/`decode`, and a structurally invalid value is rejected at decode with a
     typed error. **Five relational identity checks are BINDING, each proven with a
     hand-authored torn value** (all five are carried-forward MUST-FIXes from the v2
     sign-off and its batch review):
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
     (v2 review, `multipart.rs:1789/1800`; the archived test at
     `multipart_records.rs:1108` *affirmed* this case — it must now reject);
     (e) a `PendingEntry` with exactly one of `owner`/`staged` is **torn** and rejected at
     decode, under both a `pending:` and a `sidx:` reading (v2 review,
     `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
     valid shapes.
  2. **Decode→encode is the identity on a legacy value:** a legacy `pending:` value with
     neither new field re-encodes **byte-identically** — the `skip_serializing_if`
     identity every `require(key, encode(prior))` CAS in `metadata.rs` depends on
     (`metadata.rs:1368-1391`, ADR-0047:38-50).
  3. **The identity/occupancy boundary holds:** a decoded `AdmissionRecord` whose `count`
     exceeds its `max_sessions` still **decodes** — occupancy above a lowered cap is a
     legitimate live state that admission (a later slice) refuses to grow, not a decode
     error; rejecting it would make a durable record unreadable the day the profile is
     lowered (the `MAX_ROOT_SEGMENTS` boundary, `metadata.rs:312-321`). Assert it decodes.
     Identity relations (two stored spellings of one quantity — legs 1a–1e) are binding;
     occupancy relations are not. This settles the v2 reviewer's "count above the derived
     admission cap" finding the other way, deliberately, at Plan.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_records.rs` + CRATEs
  core/custodian/dst/metadata-redb/server (confirmed by `--classify` dry-run on the
  synthetic 11-file set); GREEN leg `cargo test -p wyrd-core --test multipart_records`;
  RED leg reverts production, the test fails to compile → **UNVERIFIABLE (exit 77),
  EXPECTED and PRE-DECLARED** as a §6 item. **Demonstrated red Do MUST capture instead
  (binding):** five named negations, one per relational check (1a–1e) — drop that check,
  run the test, paste the failing output into `build-notes.md`, revert. Plus one for leg 2:
  remove one `skip_serializing_if` attribute and show the byte-identity leg fail. A leg
  green under its negation is not load-bearing and must be rewritten. This child builds on
  the previous child's **folded** result (wave fold / merged PR), so its patch applies on a
  base that already contains the key grammar — the verify base is the brief's `@ main`
  after the wave merge, per INTEGRATION §2 `wave_mode = "merge"`.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, §6 row at `:137`;
  `0016:2802-2813`), over this child's category: **a stored record's fields may not
  disagree with each other, and the disagreement must surface as an error, never as a
  value** (ADR-0045). An admission record whose `max_sessions` does not match its own
  `profile` admits sessions past the memory bound the reconcile pass is sized for — a
  fleet-wide failure admitted by one unvalidated field. An owned entry attributed to the
  wrong session is staged data renewed or reclaimed under the wrong identity. A retirement
  payload under the wrong token reclaims one generation's data while clearing another's
  obligation. A torn `PendingEntry` turns a structural invariant into a convention.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; base verified `339da46`)
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** builds on the key-grammar child: every record here is keyed by its
  grammar and built from its validated types (`UploadId`, `PartNumber`, `Digest`), and
  both extend the same `multipart.rs` — the dependency wave-serialises the shared file.
  Wave 1 of this chain. **#682 (same batch, also wave 1 by its own dependency on #681)
  shares TWO files with this child** — `core/src/metadata.rs` (its `MAX_VALUE_BYTES`
  enforcement vs this child's `PendingEntry` region) and `dst/tests/custodian.rs` (its
  substantive edits vs this child's mechanical initializer lines) — so the two must never
  share a wave: the `Conflicts with: 682` on this brief is what schedules them apart
  (added at split acceptance; the proposal's ordering fields can only name sibling
  labels). Keep this child's hunks in both shared files as small as briefed — a wider
  hunk is a needless rebase surface for #682 whichever of the two folds first.
- **Surfaces:** data
- **Difficulty:** medium   (11 files, 6 crates — but 8 of the files are mechanical
  ≤8-line initializer touches with zero logic; the reviewable substance is one module
  extension + one struct extension + one test file. The cross-crate reach is what a
  reviewer must hold in view; rated medium, not high, because every ripple hunk is
  mechanically checkable)
- **Scope:** extend `crates/core/src/multipart.rs` with the record family and validating
  decoders listed under Goal, including `encode_record`/`decode_record` and the five
  relational checks; **key-taking decode APIs** for the two records whose identity lives
  partly in the key (`decode_owned_entry(key, bytes)`, `decode_retire_obligation(key,
  bytes)`) — a decode that cannot see the key cannot validate against it, which is exactly
  how v2's shape failed its review. In `crates/core/src/metadata.rs`, **ONE allowance**:
  `PendingEntry` (`metadata.rs:1528`) gains `owner: Option<multipart::UploadId>` and
  `staged: Option<multipart::StagedPlacement>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`, doc comments per the v2 hunk), drops `Copy` (forced — `UploadId`
  is a `String` newtype), and gains the torn-shape rejection (leg 1e) in a manual
  `Deserialize`; nothing else in that file changes. **The mechanical ripple is explicitly
  ALLOWED (this Plan's answer to the v2 C1 finding that the 5-file ceiling was
  unbuildable):** the 8 files that construct or copy `PendingEntry` —
  `crates/core/src/write.rs`, `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs` — may each gain the mechanical `owner: None,
  staged: None` initializer lines / clone-instead-of-copy fixes, **≤ 8 changed lines per
  file, no logic change, no new function**. A ninth ripple file or a non-mechanical hunk
  in any of them means the seam is wrong: STOP and hand back.
  / out of scope: the outcome enums, answer table, `MultipartEtag`, digests and `sha2`
  (child-3's — no `Cargo.toml`/`Cargo.lock` change here); the knob **values** (#655);
  every store round trip (#656–#659); reaper/windows (#625); custodian **source** code —
  `crates/custodian/src/` untouched (only its *tests'* initializers); `docs/design/`
  untouched.
- **Budget:** ≤ 1,250 added semantic lines total (module extension ≈ 560, `metadata.rs`
  ≈ 40, test ≈ 500, ripple ≈ 40 mechanical) across exactly **11 files** (3 substantive:
  `multipart.rs`, `metadata.rs`, the new test; 8 mechanical, named above).
- **Repro instruction:** n/a — new functionality on a base where only the key grammar
  (previous child) exists.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_records.rs` — a **NEW** file, not optional
  (C4-verify's added-`*/tests/*.rs` discriminator; `--classify` dry-run confirmed). The
  five relational legs and the byte-identity leg live here; co-located unit tests may ship
  in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as child-1:
  UNVERIFIABLE RED pre-declared, everything built is exercised at Check under the named
  test + gating C4-ci, and the six negation demonstrations in `build-notes.md` replace the
  flippable red.
- **Production reach:** N/A by design — consumers are #656–#659, separately filed. The one
  live-path touch is `PendingEntry`, and leg 2 proves the live path (every existing
  `pending:` CAS) is byte-identical — the extension is inert until #657 writes the first
  `sidx:` record.
- **Citations expected:** cite `path:line` on the target base for every change. Sources Do
  MUST open: the 0016 sections in the header; ADR-0045; ADR-0047:38-50 (the
  `Option`+`default`+`skip_serializing_if` identity rule). Peer callsites Do MAY open:
  `crates/core/src/metadata.rs:1368-1391` (the identity-preserving optional-field
  precedent this child's `PendingEntry` hunk must mirror); `metadata.rs:312-321` (the
  occupancy-not-at-decode boundary, leg 3). **Salvage — the primary lever:**
  `results/issue_654/iteration-v2/patch.diff` — take the record types, wire structs and
  decoders (`Budget` through `RetirePayload`, added-file lines ~846–1818) and its
  `metadata.rs` hunk; then **fix the five recorded defects** (legs 1a–1e — the review
  found the checks missing or the API unable to see the key) rather than re-shipping the
  reviewed shape.
- **Prior-art check (triage cycles):** verified at this Plan against `339da46`: no record
  type or decoder exists on `origin/main` (grep as child-1); `git -C ../wyrd log
  origin/main -- crates/core/src/metadata.rs` shows no multipart-related commit; no open
  PR touches these paths. Closed/rejected: #508 line, #636, and #654's two archived
  attempts — the v2 batch review's six findings on these paths
  (`results/issue_654/review-batch.md`) are this child's binding legs 1a–1e, not
  suggestions.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# Brief — multipart verb × state answer table + digests (654 split 3/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636), split per its 2026-08-05 sign-off.
> **The design is settled and normative:** proposal **0016** on `origin/main` @ `339da46`.
> Do MUST read: decision 3's lifecycle + verb × state answer table `0016:894-1037` · the
> ETag/fingerprint deferral `0016:3064-3070` + ADR-0047:73-89,112 · tests the slices owe
> `0016:2876-2939`. Pure code, no store I/O. Material is **salvaged** from
> `results/issue_654/iteration-v2/patch.diff`.

- **Slug:** multipart-state-machine-digests
- **Defect / goal:** the records exist (previous children) but nothing answers a verb and
  nothing computes the object's identity. This child lands the **typed outcome
  vocabulary** (`InvalidPart`, `Backpressure`, `Refusal`, `CreateOutcome`,
  `ReserveOutcome`, `UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`,
  `Publication`), **decision 3's verb × state answer table as pure, total functions**, and
  the two digests — `multipart_etag` and `complete_fingerprint` — with `sha2` added to
  `crates/core`. After this child, every later slice answers in this vocabulary and no
  slice invents its own.
- **Success criterion:** the NEW file `crates/core/tests/multipart_state_machine.rs`
  passes. Every leg pure:
  1. **Every reachable decision-3 cell is answered by a pure function with a typed
     outcome.** Table-test the matrix at `0016:969-978` — `{UploadPart,
     CompleteMultipartUpload, AbortMultipartUpload, ListParts, ListMultipartUploads}` ×
     `{Open, Completing, Aborting, Completed(tombstone), absent}` = **25 cells**, each
     asserted as its typed outcome, never "an error" and never an HTTP status (the
     status/XML mapping is #508's). The two conditional cells get both branches: a
     `Completed` tombstone answers success-with-recorded-ETag **only** on a
     `complete_fingerprint` match, and not-found otherwise (`0016:898-908`). A helper
     enumerates the full product and fails on any unanswered cell, so a later verb or
     state cannot be added silently.
  2. **`multipart_etag` is the settled composition, proved against an independent
     oracle:** `lowercase_hex(SHA-256(d₁ ‖ … ‖ d_N)) + "-" + N` over the **raw 32-byte
     digests** in part-number order, `N` the named count — never MD5 (ADR-0047:73-89
     closed the basis; `:112` and `0016:3064-3070` deferred only the composition to
     here). The test computes the expectation itself from the digest bytes. Discriminating
     cases: N=1; a strict subset differs from the full set; hex-text concatenation differs
     from raw bytes; the `-N` suffix is the named count; **a non-ascending or duplicate
     part-number list is a typed error — never silently sorted** (the carried-forward v2
     finding at `multipart.rs:1903`: sorting erased request order; 0016 makes ascending
     part numbers a Complete *validation*, `0016:707`, `0016:994`, so the pure functions
     receive an already-ascending list or refuse).
  3. **`complete_fingerprint` distinguishes an identical retry from a different
     assembly** (`0016:898-908`): identical ascending lists agree; one changed digest
     disagrees; same digests under different part numbers disagree; a strict subset
     disagrees; a non-ascending or duplicate list is the same typed error as leg 2 —
     canonical order **is** the request order, and the request must be ascending (pinned).
  4. **`MultipartEtag` decode is validating:** parsing rejects a count suffix of 0, a
     count above `MAX_PART_NUMBER`, and a malformed hex or suffix — the count-vs-keyspace
     relational check the v2 review found missing (`multipart.rs:1844`).
  5. **The outcome enums are exhaustive:** no `#[non_exhaustive]` on any public outcome
     enum — assert by matching each without a wildcard arm in the test. (Pinned at Plan:
     every consumer is in-workspace (`Cargo.toml:40` `publish = false`); a new outcome
     variant MUST break every gateway wire-mapping table at compile time rather than fall
     into a `_ =>` arm that maps it to a silently wrong status — reliability over compile
     convenience, the human's explicit call, doubly so with multiple protocol gateways
     planned.)
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_state_machine.rs` + `CRATE crates/core`
  (`--classify` dry-run confirmed); GREEN leg `cargo test -p wyrd-core --test
  multipart_state_machine`; RED leg fails to compile → **UNVERIFIABLE (exit 77), EXPECTED
  and PRE-DECLARED** §6 item. **Demonstrated red Do MUST capture (binding):** four named
  negations, output pasted into `build-notes.md`, then reverted — (a) answer one
  `Completing` cell as if `Open` (leg 1 must fail); (b) concatenate hex text instead of
  raw digest bytes (leg 2); (c) ignore part numbers in the fingerprint (leg 3); (d) sort a
  non-ascending list instead of refusing it (legs 2/3). A leg green under its negation
  must be rewritten. Builds on the previous child's folded result (wave merge), so the
  patch applies on a base already carrying the grammar and records.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, `:137`; `0016:2802-2813`),
  over this child's category: **every cell of the answer table has an answer, and the
  convenient answer is never a silently wrong one**. An unanswered verb × state cell is a
  state a client cannot leave; the cell 0016 spends most words on (`Completed` + a
  fingerprint mismatch) is one where the convenient answer tells a client *its* assembly
  succeeded while the store holds another — a silent wrong answer, worse than any error.
  Exhaustive enums extend the same rule to time: a future outcome must be answered
  deliberately at every consumer, not defaulted.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; base verified `339da46`)
- **Depends on:** child-2
- **Conflicts with:**
- **Ordering note:** builds on the record-family child: the answer table is a function of
  its `SessionState`/`Completion`, the digests take child-1's `PartNumber`/`Digest`, and
  all three extend the same `multipart.rs` — the chain wave-serialises the file. Final
  wave of this chain; #655 must be re-pointed to depend on THIS child (it appends to the
  same module and asserts against both siblings' surfaces).
- **Surfaces:** data
- **Difficulty:** medium   (four files, one crate, zero existing call-sites — low
  blast-radius today, but the outcome enums and the two digests are the durable contract
  #508 and four later slices map to wire and store; rated up for that forward reach)
- **Scope:** extend `crates/core/src/multipart.rs` with the outcome enums, `Verb`, the
  `*Answer` types, the per-verb answer functions + the total `answer` dispatcher,
  `canonical_named_parts` (validates ascending/duplicate-free, **refuses** otherwise),
  `MultipartEtag` (validating parse/serde per leg 4), `multipart_etag`,
  `complete_fingerprint`; add `sha2.workspace = true` to `crates/core/Cargo.toml` with a
  doc comment recording it is not a new dependency decision (`sha2 = "0.11"` is already a
  workspace dependency at `Cargo.toml:147`, used by `gateway-s3`/`server`, inside the
  `deny.toml` allowlist — ADR-0003's audit is not re-opened; `Cargo.lock` updates
  mechanically). **Plan decision pinned:** all public outcome enums exhaustive — no
  `#[non_exhaustive]` (rationale in leg 5).
  / out of scope: any store round trip (#656–#659); the S3 status/XML mapping (#508 — this
  child names no HTTP status); the knob values (#655); reaper/windows (#625);
  `metadata.rs`, `lib.rs`, `write.rs`, `custodian/` untouched; `docs/design/` untouched.
- **Budget:** ≤ 950 added semantic lines total (module extension ≈ 350, test ≈ 550) across
  exactly **4 files**: `crates/core/src/multipart.rs`, `crates/core/Cargo.toml`,
  `Cargo.lock` (mechanical), `crates/core/tests/multipart_state_machine.rs` (new).
- **Repro instruction:** n/a — new functionality; only the grammar and records (previous
  children) exist on the base.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; `cargo-deny` in particular re-runs the ADR-0003 wall over the `sha2` addition to `crates/core` (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no crate new to the workspace.
- **Test file:** `crates/core/tests/multipart_state_machine.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator; `--classify` dry-run
  confirmed). The five legs live here; co-located unit tests may ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as its siblings:
  UNVERIFIABLE RED pre-declared, everything built is exercised at Check under the named
  test + gating C4-ci, and the four negation demonstrations in `build-notes.md` replace
  the flippable red.
- **Production reach:** N/A by design — the consumers are #508 (wire mapping) and
  #656–#659 (store slices), all separately filed; nothing existing changes behaviour.
- **Citations expected:** cite `path:line` on the target base for every change. Sources Do
  MUST open: `0016:894-1037` (read the failure-mode tables in full — each enumerates the
  ways to implement a cell wrong); ADR-0047:73-89 + `:112`. Peer callsite Do MAY open:
  `crates/gateway-s3/src/crypto.rs:21-60` (the in-tree `sha2` usage — `Digest`, `Sha256`,
  the hex helper — so `crates/core`'s use matches the workspace's). **Salvage — the
  primary lever:** `results/issue_654/iteration-v2/patch.diff` — take the outcome enums,
  answer functions, `MultipartEtag` and the two digest functions (added-file lines
  ~1819–2349); then fix the two recorded defects: `canonical_named_parts` must refuse
  rather than sort (leg 2/3), and `MultipartEtag` decode must validate its count (leg 4).
- **Prior-art check (triage cycles):** verified at this Plan against `339da46`: no
  outcome/answer/digest symbol on `origin/main` (grepped `multipart_etag`,
  `complete_fingerprint`, `CompleteOutcome` → none); no open PR touches these paths.
  Closed/rejected: #508 line, #636, #654's two archived attempts — the v2 review finding
  at `multipart.rs:1903` (sort-erases-order) is this child's binding refusal leg.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-3 -->
