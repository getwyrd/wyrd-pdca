- **Slug:** multipart-session-lifecycle-records
- **Defect:** with the admission ledger and its profile arithmetic merged (child-1), the
  in-flight lifecycle records still do not exist. This child lands `SessionRecord` with
  `SessionState`/`PublishTarget`/`Completion` (`mpu:` — the *states* of `0016:528-602`;
  the answer table is #693's), `SlotRecord` (`slot:`), and `PartRecord`/`PartSummary`
  (`part:`/`psum:`), each validating **inside its own `Deserialize`** and encoded/decoded
  through the base's `metadata::encode` / `metadata::decode` (`metadata.rs:1536-1543`).
  **CORRECTED 2026-08-09, at #715's re-plan:** this field previously said each type gets
  "an arm in child-1's `encode_record`/`decode_record`". **There is no such envelope and
  none is coming** — #715's third attempt built one and it was the v3 T2 flat FAIL: a value
  carries no type tag (`0016:348` and the whole §1 record table), so a dispatching arm has
  nothing to dispatch on, and the base already provides the generic codec. The pattern to
  mirror is the target's own, cited under **Citations expected**. Salvage the corresponding
  types from `results/issue_692/iteration-v2/patch.diff`, fixing the recorded defects.
- **Success criterion:** every type this child lands round-trips `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md`:
  **(1c)** a `SessionRecord` whose `publish_target.parent`/`name` disagrees with the
  session's own `parent`/`object` (v2 review, `multipart.rs:1258`);
  **(1c-epoch, NEW 2026-08-09 — plan-review finding, and it is a second identity relation the
  brief had simply omitted)** a `SessionRecord` whose `publish_target`'s **fence epoch**
  disagrees with the session's own `epoch` is rejected. `publish_target` carries the
  `Completing` fence epoch `E` precisely so segments are written under a deterministic
  segment-group nonce for **that attempt** (`0016:350`, `:560-563`); a record whose two
  epochs disagree addresses another attempt's segment-group — the F18 class. The archived
  decoder already enforced it (`results/issue_692/iteration-v2/patch.diff:719`), so this is
  restoring a check the salvage would otherwise silently lose;
  **(1i, PartRecord half)** `PartRecord` validates each `ChunkRef` structurally at decode
  rather than accepting a raw one (v2 reviewer C5 + T2 FAIL, `multipart.rs:1538`) — nested
  types validate at decode, not just the outer record. **The structural checks are exactly
  these, enumerated so an over-strict decoder is as wrong as a permissive one**
  (`ADR-0045:69-74`, which tabulates them): the `EcScheme` is rejected unless
  `erasure::supported(k, m)` (`crates/core/src/erasure.rs:120` — the #285 precedent).
  **The "logical `len` must be non-zero" rule that stood here is WITHDRAWN (plan-review
  finding, 2026-08-09): it was an invention.** ADR-0045's row says "logical length
  **consistent with the scheme**", not non-zero, and the target's encoder handles a
  zero-length chunk without complaint (`crates/core/src/erasure.rs:79-83`, `shard_size`
  applies `.max(1)`). Do MUST NOT reinvent it — inventing a bound whose source does not
  demand it is exactly what cost #715 three rounds. The record-level length rule lives in
  leg 1k below, where the target genuinely does demand one.
  **PLUS a binding POSITIVE case:** a `ChunkRef`
  whose `placement` length does NOT match its scheme's fragment count **still decodes** —
  placement length is the standing *contextual* check, liberal on read
  (`ADR-0045:69-74`, `AGENTS.md:146-149`, `0016:416-429`). Geometry is validated; length is
  not. Without that positive case a decoder that rejects both passes this brief and breaks
  the target contract;
  **(1i, SlotRecord half)** a `SlotRecord` with `lease_expiry_millis <=
  reserved_at_millis` — a slot born already lapsed — is rejected (`multipart.rs:1657`);
  **(1j, NEW — state-forbidden fields, plan-advisory finding 2026-08-09)** an `mpu:` value
  that carries a field its own `state` forbids is a **decode error**, not a value: the
  normative example is a `Completing`-only `fenced_at_millis` on an `Open` session
  (`0016:403-415`). Without this leg a permissive state decoder passes every round trip and
  every other leg here while accepting exactly the record the target calls invalid. Prove it
  with a hand-authored `Open` session carrying `fenced_at_millis`, and the mirror: a
  `Completing` session MISSING a field that state requires.
  Decode validates against **format** maxima, never live knobs (`0016:390-402`).
  **(1k, NEW 2026-08-09 — plan-review finding)** a `PartRecord` whose own `len` does not equal
  the checked sum of its `chunks`' lengths is rejected at decode, and the sum is **overflow-
  checked** so an absurd chunk list is a typed error rather than a wrap. The record stores
  both fields (`0016:351`), and this is not a new invention: the target's analogous
  `SegmentRecord::from_wire` does exactly this — `checked_chunk_bytes(&chunks)` compared to
  `byte_len`, returning `ChunkMapError::SegmentLengthMismatch`
  (`crates/core/src/metadata.rs:1142-1156`). **Mirror that function.** The archived
  implementation had the check (`results/issue_692/iteration-v2/patch.diff:895`); without it
  as a criterion Do can drop it and still satisfy every other leg.
  **(1m, NEW 2026-08-09 — plan-review finding: the wire policy does NOT arrive from #715.)**
  Each landed record type is `#[serde(deny_unknown_fields)]`, and an unknown field in a
  stored value is a typed decode rejection. #715 settles this for its OWN admission type
  only — with the codec envelope withdrawn there is no shared wrapper through which a policy
  could propagate, so this child must decide it for the lifecycle types explicitly. The
  reasoning is #715's and it is verified in the target: `metadata.rs` has **two** CAS shapes
  — `require(key, encode(prior))` for `inode:` (`:1766`, `:1891`) and `require(key, current)`
  on the **raw** read bytes for `pending:` (`:1984`, `:2011-2020`) — so a permissive decoder
  either wedges every later CAS with a permanent `Conflict` or silently drops the field on
  the next `put`. A session transition CASes the session record whole (`0016:555-558`), and
  which shape it uses is #656–#659's to choose, so both hazards are live here too.
  **(leg D — docs currency, SETTLED YES, added 2026-08-09 at #715's re-plan.)** This child
  declares four more persisted record shapes, so `AGENTS.md:154-157` bites here exactly as it
  does on #715 and #717 — all three now carry the paragraph rather than leaving it to a
  fourth sign-off round. **EXTEND, do not duplicate:** #715 lands beneath this child and
  already adds a multipart paragraph to
  `docs/design/architecture/05-building-block-view.md` § "The metadata model"; this child
  extends that same paragraph with the session/slot/part records in one or two sentences.
  Read what #715 actually landed on the merged base before writing — do not assume this
  brief's wording of it. Verified by presence + the `C4-ci` prose/render gate, **not** by a
  negation; it is NOT one of the nine demonstrations below.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_session_records.rs` + `CRATE crates/core`; the
  GREEN leg is `cargo test -p wyrd-core --test multipart_session_records`; the RED leg
  reverts production and the test fails to **compile** → **UNVERIFIABLE (exit 77),
  EXPECTED and PRE-DECLARED** as a §6 item. **Demonstrated red Do MUST capture instead
  (binding):** **EIGHT** named negations, one per independently-enforced rejection —
  **1c, 1c-epoch, 1i-ChunkRef-scheme, 1i-slot, 1j-forbidden-field, 1j-missing-required,
  1k-len-mismatch, 1m-unknown-field** — drop that single check, run the test, paste the
  failing output into `build-notes.md`, revert. **Each negation must ISOLATE its rule:** the
  torn value must violate only that one, so the red proves that guard is load-bearing rather
  than riding on a neighbour's. **PLUS exactly ONE positive leg negated the other way** —
  make the wrong-length `ChunkRef` placement reject, and show the still-decodes assertion
  fail. **Nine demonstrations total.** A leg green under its own isolating negation is not
  load-bearing and must be rewritten.
  (Count history, since it has moved twice and a stale number is how the previous version
  contradicted itself: three at authoring → six at the 2026-08-09 plan advisory, which added
  leg 1j and split the `ChunkRef` checks → **eight** at the 2026-08-09 plan review, which
  withdrew the invented `1i-ChunkRef-len` and added `1c-epoch`, `1k` and `1m`. The positive
  count was wrongly stated as "two" throughout; there is and was only **one**.)
- **Invariant to restore:** sourced from the TARGET repo, the only tree Do can read:
  **ADR-0045 §"Parse-don't-validate at decode"**
  (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`), whose invariant table at
  `:69-74` names this child's nested rule outright — `EcScheme::ReedSolomon` →
  `erasure::supported(k, m)` — plus the format-maxima boundary at `0016:390-402`. (The
  harness-side catalogue rule is **C-1**, `docs/principles.md:109` / `:137` in the
  *wyrd-pdca* repo — audit trail only; **Do cannot open it**, being grounded on a wyrd
  checkout. Cite the ADR. Plan-advisory finding, 2026-08-09.) Over this child's category:
  **a stored record's fields may not
  disagree with each other, and a nested value is validated at decode exactly as the outer
  record is**. A session whose `publish_target` names a different object than
  the session's own parent/key publishes one client's upload under another's name. A
  `PartRecord` that accepts a raw, unvalidated `ChunkRef` lets untrusted stored geometry
  reach the read path — the #285 class. A slot born already lapsed
  (`lease_expiry <= reserved_at`) is reapable the instant it is written, so a live part
  attempt can have its staging reclaimed underneath it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality. **Execution PRECONDITION, not a claim about
  today** (clarified 2026-08-09; the plan advisory correctly observed that #715 currently
  reads `PLANNED` and that `Budget`/`AdmissionRecord` are absent from
  `origin/main`): this child MUST NOT build until **#715's PR is merged into the base**.
  That is what `Depends on: 715` enforces — under `auto_merge = false` the driver readies
  and merges nothing, stops at the wave boundary for the human to merge #715, and
  `_runnable` re-gates this bundle on `merged.is_merged` at the re-run. On the base it will
  then see, the imported symbols exist. Refresh every `path:line` citation and the line
  estimate against THAT base, not against `9dbcd72`.
- **Scope:** extend `crates/core/src/multipart.rs` with
  the lifecycle types named in **Defect** above and their validating decoders, plus the
  typed error variants those rejections need as new variants of the module's existing
  `RecordError` (which #715 has already widened from keys to record values beneath this
  child). **The landed public type list is exactly these
  SEVEN** — corrected 2026-08-09 from "five", which contradicted the Defect field and left
  the budget and the "every type round-trips" criterion without a fixed review surface:
  `SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`,
  `PartRecord`, `PartSummary`. Private wire/`*Wire` mirror structs and the error variants
  their manual `Deserialize` impls need are **implementation detail, not additional landed
  types** — they do not count against the seven and need no separate round-trip leg, but
  they DO count against the line budget. One new test file. **Plus the ONE docs paragraph
  extension of leg D** (added 2026-08-09). Budget ≈ 770 added lines /
  3 files. Cite `path:line`; sources Do MUST open: `0016:333-527`, `0016:528-602`,
  ADR-0045. / out of scope: `Budget`/`AdmissionRecord` (child-1, merged beneath this);
  `OwnedEntry`/`StagedPlacement`, retirement types, `PendingEntry`, and every file
  outside `multipart.rs` + the new test (child-3); the outcome enums and answer table
  (#693); knob values (#655); store round trips (#656–#659); **every `docs/` file except
  the one `05-building-block-view.md` paragraph** — ADRs, proposals and specs untouched
  (INTEGRATION §2 immutability).
- **Budget:** ≤ 770 added semantic lines (module extension ≈ 415, test ≈ 340, docs ≈ 15;
  raised 2026-08-09 for the three legs the plan review added — 1c-epoch, 1k, 1m)
  across exactly **3** files: `crates/core/src/multipart.rs`, the new test, and
  `docs/design/architecture/05-building-block-view.md`. A fourth changed file means the seam
  is wrong: STOP and hand back.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; `docs-renderer` and `typos` are load-bearing HERE because leg D edits a rendered architecture doc, the rest warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_session_records.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator). Co-located unit tests may
  ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as child-1: the
  UNVERIFIABLE RED is PRE-DECLARED so C2/C4 land as a known sign-off item rather than a
  surprise NEEDS-HUMAN. Everything built is exercised at Check under the named test +
  gating C4-ci, and the **nine** demonstrations enumerated under **Falsifiability** (eight
  negations plus the one positive leg negated the other way) replace the flippable red.
  (Corrected twice on 2026-08-09: from a stale "three", then to match the plan review's
  revised leg set. Falsifiability is the authority for this count.)
- **Production reach:** N/A by design — these records have no live writer until #656–#659
  wire the store round trips; nothing on an existing path changes.
- **Citations expected:** cite `path:line` on the merged base for every change. **Read the
  two citation namespaces apart — this is a trap:** a `crates/core/src/multipart.rs:NNNN`
  reference tagged *(batch-review …)* or *(v2 review …)* is relative to the **v2 PATCHED
  file** (~2,027 lines) preserved in `results/issue_692/iteration-v2/patch.diff`, NOT to
  the base — `multipart.rs` was **854 lines** on `9dbcd72` and grows only by #715 beneath
  this child, so those line numbers do not exist there. Locate them in the archived patch,
  by symbol; every other `path:line` in this brief is base-relative. Peer
  callsites Do MAY open: `crates/core/src/erasure.rs:120` (`supported(k, m)` — the
  validated-scheme predicate, and the #285 precedent that untrusted stored geometry is a
  typed error, not a panic; the `ChunkRef` half of leg 1i validates through it);
  `crates/core/src/metadata.rs:129-140` (the `ChunkRef` shape being validated).
  **The codec pattern, in place of the withdrawn envelope (added 2026-08-09) — three parts,
  all already on the base:** (i) encode/decode through `metadata::encode<T: Serialize>` /
  `decode<T: DeserializeOwned>` (`crates/core/src/metadata.rs:1536-1543`) — do NOT add a
  second generic encoder; (ii) put the structural validation **inside `Deserialize`**, so a
  value that decodes cannot be malformed: `InodeRecord`'s `#[serde(try_from =
  "InodeRecordWire")]` + `impl TryFrom<InodeRecordWire>` (`metadata.rs:1349`, `:1411`), or
  `SegmentRecord`'s hand-written `Deserialize` funnelling through a fallible constructor
  (`metadata.rs:1195`, `:1212-1216`) where a state-dependent shape needs it (leg 1j);
  (iii) where a call site needs the failure attributed, a per-record decode wrapper returning
  the typed error — `decode_segment_record` (`metadata.rs:2504-2517`). **Salvage:**
  ``$PDCA_HARNESS_ROOT/results/issue_692/iteration-v2/patch.diff` — the path is relative to the HARNESS repo (wyrd-pdca), NOT to `$PDCA_WORKTREE`; a claude builder's cwd is the harness root so it resolves as written, but a codex builder/escalation runs with cwd = the worktree and must resolve it absolutely` — take the lifecycle types and decoders, then
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
- **Depends on:** 715
- **Conflicts with:**
- **Ordering note:** **Wave 1.** `Depends on: 715` is a genuine build-on, not mere conflict
  avoidance — **restated 2026-08-09 now that #715 ships no envelope:** this child's typed
  rejections are new variants of the module's `RecordError`, which #715 widens from a
  key-only error to a record-value error (`multipart.rs:79-119` on `9dbcd72`), so the two
  edit the same enum as well as the same file. Both also extend
  `crates/core/src/multipart.rs`, so the dependency wave-serialises the shared file — either
  reason alone would order them, and neither is weakened by the envelope's removal.
  **`Conflicts with` is DELIBERATELY
  EMPTY, not unset:** this child touches only `multipart.rs` and its own new test file —
  nothing #710 or #711 reads — so it MAY share a wave with either (and does: the computed
  schedule puts #711 and #716 together in wave 1). **Downstream:** #717 depends on this
  child; #693 and #655 follow #717.
- **Disposition hint:** new-feature

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept compile-only absence as the born-at-tier red oracle — with production stashed but the criterion test retained, the test exits 101 on the missing APIs imported at `crates/core/tests/multipart_session_records.rs:26`, so no behavior-level pre-fix run exists.; C4 Verification (red→green) — Accept the declared compile-red→green evidence — restoring production makes all 20 focused tests and the complete `cargo xtask ci` gate pass, but the red leg remains criterion absence rather than an executable old behavior (`crates/core/tests/multipart_session_records.rs:26`).; C5 Causal adequacy — Rebuild must make the public-value evidence load-bearing — mutation rerun leaves `content_type`, `attempts`, and `is_empty` default-return mutants alive because tests exercise only `None`, `0`, and `false` at `crates/core/tests/multipart_session_records.rs:184`, `crates/core/tests/multipart_session_records.rs:188`, and `crates/core/tests/multipart_session_records.rs:295`.; T4 Contribution — Resolve contribution/review provenance — affected-file merged history plus open/closed PR inspection found no competing prior art, but the contribution artifacts and `scripts/review-branch` harness needed to rerun the asserted contrib PASS and three-finding review FAIL were not supplied.; T5 Judgment — Rebuild must add an omitted-`content_type` witness or reject omission — the round-trip helper always supplies explicit null at `crates/core/tests/multipart_session_records.rs:56`, so the suite passes while an accepted value changes bytes.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 51 mutants tested in 2m: 3 missed, 31 caught, 17 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept criterion absence as the born-at-tier red oracle — with production reverted but the new imports retained at `crates/core/tests/multipart_session_records.rs:35`, the test exits 101 at compile time, so no pre-fix behavior can execute.; C4 Verification (red→green) — Accept compile-red→green as sufficient verification — the reverted slice cannot compile at `crates/core/tests/multipart_session_records.rs:35`, while the patched focused suite passes 24/24 and a second full `cargo xtask ci` run passes, leaving no executable old behavior to compare.; T4 Contribution — Decide whether review and contribution provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted four-finding batch-review failure cannot be mechanically settled because the `review-branch`/contribution scripts and artifacts are absent.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
