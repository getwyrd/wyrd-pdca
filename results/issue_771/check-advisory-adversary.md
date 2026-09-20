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
