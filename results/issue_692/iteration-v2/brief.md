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
- **Depends on:** 691
- **Conflicts with:** 710, 711
- **Ordering note:** builds on the key-grammar child: every record here is keyed by its
  grammar and built from its validated types (`UploadId`, `PartNumber`, `Digest`), and
  both extend the same `multipart.rs` — the dependency wave-serialises the shared file.
  Wave 1 of this chain. **#682 was SPLIT on 2026-08-08 into #710 (ceiling/certification)
  and #711 (segmented repoint)**, so the conflict follows the ids: **#710 shares
  `core/src/metadata.rs`** (its `MAX_VALUE_BYTES` enforcement vs this child's
  `PendingEntry` region) and **#711 shares both** `core/src/metadata.rs` (its
  `repoint_chunk` primitive) **and** `dst/tests/custodian.rs` (its substantive edits vs
  this child's mechanical initializer lines) — so this child must never share a wave with
  either: the `Conflicts with: 710, 711` on this brief is what schedules them apart
  (originally `682`, added at that split's acceptance and repointed at this one's; the
  proposal ordering fields can only name sibling labels). Keep this child's hunks in both
  shared files as small as briefed — a wider hunk is a needless rebase surface for
  #710/#711 whichever folds first.
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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must pin both safety branches—the `SCAN_CAP/2` clamp and generation-source mutual exclusion—because `cargo mutants --in-diff` leaves those two mutations alive (`crates/core/src/multipart.rs:1020`, `crates/core/src/multipart.rs:1844`).; T4 Contribution — Human must settle contribution readiness after inspecting the three recorded batch-review blockers and closed/rejected affected-path history—the `scripts/review-branch` runner, its log, and archived attempts were not supplied, although merged history shows only the prerequisite key grammar.; T5 Judgment — Rebuild must reject a generation obligation with neither chunks nor segments—the adversarial probe is currently accepted, allowing an obligation that owes nothing despite the explicit-error convention (`crates/core/src/multipart.rs:1841`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep ; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 96 mutants tested in 3m: 2 missed, 64 caught, 30 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized (106,723 bytes vs. 100KB threshold, 11 files) and it shows: T4 batched review gate fails with 4 blocking findings (budget/session-token validation gaps), plus substantive NEEDS-HUMAN findings on causal adequacy (decode still accepts degenerate ReedSolomon{k:0,m:1} and zero-lifetime lease_expiry==reserved_at) and judgment (test suite endorses the same lapsed-lease case it should reject). These are implementation-shaped findings consistent with the size backstop's own recommendation. Re-plan and split via `pdca split 692` rather than attempting another iterate-do on this cut.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
