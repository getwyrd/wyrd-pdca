# Result — issue 771 / multipart-retire-obligation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the retirement **obligation value** does not exist. The base carries the
  whole `retire:` key half — `RetireMode` (`multipart.rs:945`), `parse_retire_mode` (`:1011`),
  `RetireToken` with the full `s:<upload-id>:<epoch>[:<part-number>:<attempt-id>]` /
  `g:<inode-id>:<version>` grammar (`:1022`), `retire_key` (`:1071`),
  `retire_session_range` (`:1081`), `parse_retire_key` (`:1092`) — and nothing that can read
  the value those keys name. This child lands the range-encoded part-number set (`0016:382-388`),
  the obligation payload, and `decode_retire_obligation(key, bytes)`: a **key-taking** decoder,
  because an obligation's identity lives partly in its token and a decode that cannot see the
  key cannot validate against it (which is exactly how #692's v2 shape failed review at
  `multipart.rs:1789/1800` of the archived patch). It also settles the two record-format
  decisions three Do rounds could not: the payload must be able to carry a **combined**
  `{session, parts}` obligation, and the token's epoch must have exactly one canonical value.
- Success criterion: every obligation `0016`'s writer rows install decodes from its own
  key; every shape no writer installs is rejected with a typed `RecordError` (ADR-0045); and
  every accepted witness re-encodes byte-identically. Ten legs, each asserted in the named
  test file:
  **(R1, shape completeness — the decision v2/v3 got wrong)** the payload MUST be able to
  express, as a **single** value under a **single** key, every obligation `0016` installs:
  under `retire:bytes:` + a suffix-free `s:` token — `{session, all}` (`0016:2187`),
  `{session, parts:<set>}` (`:665`, `:823`, `:2193`) and `{parts:<set>}` alone (the root
  flip's unnamed staged parts, `:662`, `:919-921`); under `retire:bytes:` + a per-part `s:`
  token — `{chunks:[…]}` (`:659`, `:672`, `:1620`); under `retire:bytes:` + a `g:` token —
  `{generation: {inode, version, chunks | segments}}` — **exactly one of the two, settled by
  the human 2026-09-11 after four rounds oscillated on it:** a generation mirrors the
  committed map, which is the two-arm `ChunkMap::Flat | Segmented` (`metadata.rs:1014`) with
  no inline chunks on a `SegmentedMap`, so a flat generation retires by its copied `chunks`
  and a segmented one by its `segments` group re-read at drain time (`:2417`). **Both present
  is a decode error, neither present is R2.** `0016` spells the row two ways (`:355`
  `chunks, segments?`; `:2417` `chunks?, segments`) — those are the two cases, not a union;
  the PR description records the erratum, this slice does not edit `0016`; under
  `retire:records:` + a suffix-free `s:` token — `{parts:<set>}` (`:662`), `{seg:{nonce,
  epoch}}` (`:663`, `:665`, `:823`), **and both together in one payload** (`:356`, "and/or").
  A payload type whose arms make `{session, parts}` or `{parts} + {seg}` inexpressible fails
  this leg — that is the reviewed defect, not a stylistic preference. Assert each of the seven
  shapes decodes under its own key;
  **(R2, empty)** an obligation naming nothing is rejected — residue nothing drains;
  **(R3, mode agreement)** `chunks` or `generation` under a `retire:records:` key, and `seg`
  under a `retire:bytes:` key, are rejected. The mode lives in the key precisely so this is a
  decode error and never a misread boolean (`0016:434-441`);
  **(R4, token-scope agreement — the #692 recorded defect, both directions)** a session-wide
  component (`session` / `parts` / `seg`) under a **per-part** token, and a per-part component
  (`chunks`) under a **session-wide** token, are both rejected. The optional
  `:<part-number>:<attempt-id>` suffix exists only for the per-part obligations
  (`0016:358-366`); #692's batch review recorded the broken arm accepting **every**
  session-scoped payload, so both directions are binding;
  **(R5, generation identity)** a `generation` payload whose `{inode, version}` differs from
  the `g:` token's is rejected; so are a `generation` payload under an `s:` token and a
  session-scoped payload under a `g:` token;
  **(R6, canonical token epoch — the round-3 blocking finding)** **the token's epoch is the
  epoch the installing fence was taken against** — the `require(mpu == …@E)` the batch
  preconditions on — which for a `{seg:<g>:<E>}` obligation is `E` itself, the epoch whose
  segment keys it names (`0016:2357-2362`, `:663-665`). Decode enforces `token.epoch ==
  seg.epoch` **exactly**; `E±1` is rejected. One canonical key per obligation is what makes
  `require_absent` mean anything (`0016:369-373`) — accepting a window lets one obligation be
  installed and drained twice. **State the limit of this check explicitly and do not
  over-claim it:** the payload's segment-group **nonce** is deliberately independent of the
  upload id (`0016:499-509`), so a foreign session's group under your token is **not**
  detectable at decode; this leg binds the **epoch component only**, and the group identity is
  the writer's and the drain's to establish;
  **(R7, nested chunk geometry)** every `ChunkRef` in `chunks` / `generation.chunks` is read
  through the module's own closed `ChunkRefWire` (`multipart.rs:1996`) and rejected unless
  `erasure::supported(k, m)` — the `checked_chunk_scheme` rule (`multipart.rs:1937`), the
  #285 precedent, ADR-0045's invariant table (`0045:71-72`). Placement **length** is never
  checked here (see R8's boundary note);
  **(R8, part-number set structure)** `parts` is range-encoded (`0016:382-388`) and its
  spelling is **canonical**: runs ordered, non-overlapping and non-adjacent (so `[[1,2],[3,4]]`
  is not a second spelling of `[[1,4]]`), each endpoint in `[1, MAX_PART_NUMBER]`
  (`multipart.rs:542`), `lo <= hi`. A non-canonical or out-of-range spelling is rejected —
  two spellings of one obligation defeat `require_absent` exactly as two keys do. **The
  boundary this child does NOT cross:** a `ChunkRef` whose `placement` length disagrees with
  its scheme's fragment count **decodes** — the standing contextual check, liberal on read
  (ADR-0045 `:45-49` and its `ChunkRef` row `:72`, `AGENTS.md:146-149`, `0016:416-432`);
  **(R9, serialization identity)** every accepted witness re-encodes byte-for-byte as it
  arrived, asserted file-wide by the `decode_both` helper pattern rather than test by test
  (`crates/core/tests/multipart_session_records.rs:169`; `AGENTS.md:170-172`). Every
  retirement obligation is installed and drained under exact-bytes preconditions, so a
  re-encode that is not the identity is a record nothing can precondition on;
  **(R10, docs currency)** the living architecture doc's multipart sentence
  (`docs/design/architecture/05-building-block-view.md:202`) gains the two `retire:`
  namespaces and what their values carry. `AGENTS.md:154-158` makes this a merge requirement,
  not a follow-up. Extend that sentence in its own voice and length; do not restate the
  proposal, and change nothing else in that file. **Resolve the contradiction you will find
  in the module header rather than inheriting it:** `multipart.rs:63-73` still argues that the
  living architecture doc "gains these namespaces with the slice that first *persists* one".
  That clause is **stale** — #715/#716 added the paragraph at `05-building-block-view.md:202`
  anyway, framed as "landed ahead of their writers", which is the reading `AGENTS.md:154-158`
  supports for a persisted record definition. Correct that header clause in the same hunk (it
  is in a file this child already edits) so child-2 and #656–#659 do not re-litigate it, and
  bring the header's key table and its "nothing here is written yet" section up to date with
  what this child landed — the same housekeeping #715 and #716 each did.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: extend `crates/core/src/multipart.rs` with the
  range-encoded part-number set, the retirement obligation payload, its `RecordError` variants,
  and `decode_retire_obligation(key, bytes)` — reusing the base's `RetireMode` / `RetireToken`
  / `parse_retire_key` (`:945`, `:1022`, `:1092`) and its closed `ChunkRefWire` /
  `EcSchemeWire` / `checked_chunk_scheme` (`:1996`, `:2029`, `:1937`) rather than adding a
  second spelling of any of them. Plus the one new test file and the one docs sentence.
  Budget: ≤ **1,000** added semantic lines across exactly **3** files (module ≈ 500, test ≈ 480,
  docs ≈ 20). A fourth file means the seam is wrong: STOP and hand back. / **out of scope:**
  `crates/core/src/metadata.rs` and every `PendingEntry` / `sidx:` concern (child-2's, and
  touching that file here would put this child into #721/#722's conflict set for nothing); any
  writer, store call, `async fn` or `WriteBatch` (the drain and its installers are #656–#659);
  the outcome enums, answer table and digests (#693); knob values (#655); reaper/windows
  (#625); every `docs/design/` file except the one `05-building-block-view.md` sentence — ADRs,
  proposals and specs are untouched (INTEGRATION §2 immutability), and `0016` is **not** to be
  edited even where this child's R6 decision settles something it leaves open (record the
  decision in the code's doc comment, citing the lines it resolves).

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 87.5% — 237 of 271 instrumentable changed lines executed (floor 80%); 271 of 1084 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 71 mutants tested in 2m: 40 caught, 31 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.47s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #771 correctly adds a key-bound multipart retirement-obligation value decoder and canonical part-set grammar; sign-off is owed only for the declared born-at-tier evidence posture and the format's fitness before its writers land.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the writer-shape, typed-rejection, byte-identity, and docs outcomes concrete and resolves the sole format ambiguity against the protocol's record table (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:346`). |
| C2 Reproduction (red pre-fix) | N/A | This is new born-at-tier functionality: the retained test cannot import the absent base API, so the pre-fix run stops at compilation before any behavioral discriminator executes (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:110`). |
| C3 Change | PASS | The three-file change stays on the authorized pure type/decoder/test/docs surface, and its public decoder returns the key's mode and token with the validated payload (`crates/core/src/multipart.rs:3245`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared compile-red posture — independent and frozen patched runs pass all 28 focused tests and frozen full CI is green, but no pre-fix test body ran, so behavioral red→green remains unproved (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:110`, `gate-logs/C4-ci.log:3432`). |
| C5 Causal adequacy | PASS | The causal rules live at construction and keyed decode rather than behind a capability/runtime guard, and independent mutation testing reproduced all 71 diff mutants as 40 caught plus 31 unviable (`crates/core/src/multipart.rs:2452`, `crates/core/src/multipart.rs:3109`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | The patch has exactly the scoped source, dedicated always-built test, and living-doc files; the test carries no cfg gate and centralizes accepted/refused witnesses through one decode helper (`crates/core/tests/multipart_retire_obligation.rs:56`, `crates/core/tests/multipart_retire_obligation.rs:178`). |
| T2 Shape | PASS | Closed wire types, non-empty canonical ranges, combinable payload components, and the settled exclusive generation map make accepted values representable while rejecting unsupported shapes as typed errors (`crates/core/src/multipart.rs:2446`, `crates/core/src/multipart.rs:2646`, `crates/core/src/multipart.rs:2904`). |
| T3 Runtime | PASS | All 28 focused tests execute with none ignored and frozen diff coverage is 87.5%; the local full-gate rerun reached the dependency audit before a read-only advisory-lock host fault, while the frozen worktree completed that audit and full CI (`gate-logs/C4-diff-cov.log:124`, `gate-logs/C4-ci.log:2853`, `gate-logs/C4-ci.log:3432`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check, and the mandatory publish rerun owes their substantive audit (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The sole batch-review blocker is rejected because it reopens the brief's settled exact-one-source decision, which matches the target's two-arm committed `ChunkMap` and is pinned by a rejection witness; the brief's affected-path prior-art record covers merged and closed/rejected attempts (`crates/core/src/metadata.rs:1014`, `crates/core/tests/multipart_retire_obligation.rs:444`, `gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve freezing this persisted grammar ahead of its writers and drain — pure decode evidence establishes structural safety but cannot establish that the later installer/drain semantics make the format fit for reclamation (`docs/design/architecture/05-building-block-view.md:202`). |

### Advisory — adversary

# Adversarial review — issue #771 (`multipart-retire-obligation`), round 5

Advisory only; I never gate. Everything below is grounded on the target source at
`$PDCA_TARGET` and on runs I performed there (`cargo test -p wyrd-core --test
multipart_retire_obligation` reproduces GREEN: 28/28). The RED leg is
criterion-absence (compile failure, `gate-logs/C4-verify.log`) — **pre-declared in the
brief's Falsifiability section and accepted at two prior sign-offs**, so I did not spend a
refutation attempt on it; instead I rebuilt the evidence independently (see the last
section), which is stronger than the missing red.

## Findings

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2427-2430`: the `PartNumberSet`
  doc's "cardinality bound, so no separate one is spelled" claim is false, and it
  contradicts the paragraph 15 lines above it.** `:2412-2414` states the worst case as
  "10,000 alternating part numbers — still fits inside one value" (true: 5,000 runs encode
  to **58,901 bytes**, under `metadata::MAX_VALUE_BYTES = 100_000`, `metadata.rs:324-327`).
  `:2427-2430` then re-states the worst case as `⌈MAX_PART_NUMBER / 2⌉` runs — "an
  alternating set over the whole key space, **the worst case `0016:382-388` sizes the
  encoding for**". Those two worst cases differ by 100×, and the second one does not fit.
  Measured against this exact build (probe run at `$PDCA_SCRATCH`, `decode_retire_obligation`
  on a canonical alternating set under `retire:bytes:s:<id>:7`):

  | runs | value bytes | decode | vs `MAX_VALUE_BYTES` |
  |---|---|---|---|
  | 5 000 (the brief's stated worst case) | 58 901 | `Ok` | fits |
  | 20 000 | 268 901 | `Ok` | **2.7×** over |
  | 500 000 (`⌈MAX_PART_NUMBER/2⌉`) | 7 888 901 | `Ok` | **79×** over |

  Concrete failing case: `{"parts":[[1,1],[3,3],…,[39999,39999]]}` (20 000 runs) under
  `retire:bytes:s:a1a1…:7` decodes `Ok` at 268 901 bytes — a value no backend can store, in
  a record format this slice **freezes** for #656–#659, #693 and #655. Worse on the writer
  side: `PartNumberSet::from_numbers` (`:2487`) is the writer-facing constructor this slice
  ships *specifically* "so that no writer (#656–#659) has to spell the rule a second time",
  and it minted the full 500 000-run set for me without complaint while its own doc tells
  that writer the canonicality rules already are the cardinality bound. I am **not** asking
  for a decode-time refusal — ADR-0045's liberal-on-read boundary and the
  `MAX_ROOT_SEGMENTS` precedent (`metadata.rs:302-322`) both say a capacity ceiling belongs
  where it becomes work, not at decode. What is wrong is the *claim*: the repo's own pattern
  for exactly this (`MAX_ROOT_SEGMENTS`, whose doc makes `max_segref_bytes × N ≤ V/2` an
  explicit obligation and whose worst case is **measured on `encode(...).len()`** in
  `crates/core/tests/segmented_map_record.rs`) has no counterpart here. Fix inside this
  diff: correct `:2427-2430` to name the real sizing argument (the live
  `MAX_PARTS_PER_SESSION` ceiling, which the same paragraph correctly says decode must never
  enforce), and either bound `from_numbers` or state the encoded-worst-case obligation the
  way `MAX_ROOT_SEGMENTS` does — ideally with the one-line `encode().len()` measurement the
  sibling test already models.

- **NEEDS-HUMAN [human] — the single gating failure is a re-litigation of a decision the
  human already settled, and the erratum that settles it has no in-tree home.**
  `gate-logs/T4-batch-review.log` blocks on `crates/core/src/multipart.rs:2776`: "Rejecting a
  generation containing both `chunks` and `segments` contradicts proposal 0016's normative
  `chunks?` plus `segments` shape". That is precisely the R1 question the brief records as
  **settled by the human on 2026-09-11** after four rounds oscillated on it ("Reviewer and
  adversary: cite R1 above; do not re-open it"), and `AGENTS.md`'s reviewer protocol makes a
  settled decision out of scope for a later round. Per the same protocol this should be
  *recorded-rejected*, not routed back to Do — an auto-iterate on it would rebuild attempt 4's
  hybrid, which the round-4 adversary already refuted from the data model. The judgment call
  the human actually owns: the erratum against `0016:355` currently rides **only the PR
  description**, while the merged proposal on `main` still spells `{inode, version, chunks,
  segments?}` and the diff now asserts the opposite in a *living* architecture doc
  (`docs/design/architecture/05-building-block-view.md:202`, "the two shapes a published map
  has, never both"). Three independent codex passes have now re-derived that contradiction
  from `0016` in two separate rounds, so the next reviewer and the next rebuild will too.
  Decide whether to open a tracked erratum issue against `0016` (the brief forbids editing
  it here) so the deferral becomes citable in-tree.

- *(observation, not a NEEDS-HUMAN)* `crates/core/src/multipart.rs:557-619` — the nine new
  `RecordError` `Display` arms are the **only** uncovered changed lines in the diff (all 34
  `MISS` entries in `gate-logs/C4-diff-cov.log` fall in this span), and the module's own
  stated convention is that a rejection is "asserted twice: the **variant** … and the exact
  `Display` text" (`crates/core/tests/multipart_keys.rs:356-360`). I rendered all twelve new
  messages myself and **every one is correct and non-panicking**, so this is not a live
  defect — only a gap in the falsification net for a format that is being frozen. Noting it
  so the reviewer does not mistake the 87.5% diff-cov for coverage of the error surface.

## What I attempted to refute and could not

- **The acceptance surface is over-broad.** I enumerated the full cross-product — `session ×
  {parts absent | explicit set | "all"} × chunks × {generation absent | flat | segmented} ×
  seg`, 288 (payload, key) pairs across `{bytes, records} × {s: suffix-free, s: per-part,
  g:}` — through the production `decode_retire_obligation`. **Exactly 10 pairs decode `Ok`,
  and they are exactly the 10 writer rows** in the `RetirePayload` doc table
  (`multipart.rs:2925-2939`). No extra shape is admitted; the mode × token-scope table
  (`:2760-2803`) closes every cross-component combination the protocol does not install. I
  could not find a payload the decoder accepts that no writer can produce.
- **A legal shape is falsely rejected.** Probed extremes that must round-trip:
  `u128::MAX` chunk id, `u64::MAX` chunk `len`, `EcScheme::None`, an empty `placement`
  (the deliberate contextual-check boundary), `u64::MAX` inode/version, a 2 000-chunk
  per-part list, `[[1, MAX_PART_NUMBER]]`. All accept and all re-encode byte-identically.
- **Serialization identity (R9) passes for the wrong reason.** `require_canonical`
  (`:1727-1737`) compares `metadata::encode(payload)` against the raw stored bytes, so the
  test's own `decode_witness` assertion is indeed a cross-check rather than evidence — the
  file's header says exactly that, honestly, and `a_foreign_spelling_of_an_accepted_payload_is_rejected`
  is the leg that actually pins the gate. I could not construct an accepted witness whose
  re-encode is not the identity.
- **A negation that fails to isolate.** I traced each of the ten mandated negations to the
  single test it would break (`checked_shape` → `an_obligation_owing_nothing_is_rejected`;
  the coalescing rule → `a_noncanonical_part_number_set_is_rejected`; the epoch equality →
  `a_segment_group_epoch_other_than_the_tokens_is_rejected`; the `all`-without-`session`
  rule → `the_all_parts_wildcard_outside_a_session_teardown_is_rejected`; etc.). Each check
  is load-bearing: with it removed the corresponding witness decodes `Ok` *and* re-encodes
  canonically, so the byte gate would not mask the loss. `C5-mutants` agrees (0 missed of
  71).
- **A key-side second spelling.** `decode_retire_obligation` never re-spells the key and
  compares it, which would be the key-side analogue of `require_canonical` — but
  `parse_retire_key` (`:1302-1330`) is already canonical (`canonical_decimal` rejects
  leading zeros and signs, `UploadId`/`AttemptId` fix the token width), so there is no second
  spelling to exploit. Pre-existing base code in any case.
- **The three round-4 sign-off `[impl]` items.** All three land: the mode is now part of the
  decode answer (`:3245-3258`, pinned by `the_mode_is_part_of_the_decoded_obligation`), the
  hybrid generation is rejected with a typed error (`:3070-3078`), and the architecture
  paragraph no longer asserts install/drain CAS behaviour that does not exist.
- **Budget/scope.** 3 files, 1 001 added semantic (non-blank, non-comment, non-attribute)
  lines against the brief's ≤ 1 000 — a 0.1% overshoot, down from ~1 060 in round 1. Not
  worth a round.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared compile-red posture — independent and frozen patched runs pass all 28 focused tests and frozen full CI is green, but no pre-fix test body ran, so behavioral red→green remains unproved (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:110`, `gate-logs/C4-ci.log:3432`).
- [x] Validation — fitness-to-purpose — Approve freezing this persisted grammar ahead of its writers and drain — pure decode evidence establishes structural safety but cannot establish that the later installer/drain semantics make the format fit for reclamation (`docs/design/architecture/05-building-block-view.md:202`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- [x] size backstop — this slice is behaving oversized: patch is 110 KB (threshold 100 KB); 4 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] T5 Judgment — Decide the safe public API boundary — `RetirePayload: Deserialize` exposes value-only decoding even though the record’s correctness depends on key relations, so future consumers can bypass the documented key-taking boundary (`crates/core/src/multipart.rs:2762`, `crates/core/src/multipart.rs:2994`).
- [x] **The decoder accepts a generation shape no writer can install, and the

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-11

## 10. Act candidates (hints for the next Act review)
- Bug + fix: `crates/core/src/multipart.rs:2427-2430` doc comment misstates `PartNumberSet`'s worst-case encoded size (100x off vs. the correct claim at `:2412-2414`, 58,901 bytes), and `from_numbers` has no guard against minting the oversized case; fix the comment and add a size bound or explicit obligation (mirror `MAX_ROOT_SEGMENTS`, `metadata.rs:302-322`) before writers land.
- Follow-up: narrow `RetirePayload`'s public value-only `Deserialize` boundary so the key-relation checks in `decode_retire_obligation` cannot be bypassed by later consumers (`crates/core/src/multipart.rs:2762`, `:2994`) — revisit when the writers (#656-#659) show whether a raw decode is needed at all.
