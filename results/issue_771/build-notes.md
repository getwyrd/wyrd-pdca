# Build notes — #771 `multipart-retire-obligation` (round 5)

Withheld from the reviewer; written for the human at sign-off.

All `path:line` citations are against **the patched tree** (`$PDCA_WORKTREE`,
`/home/eddie/wyrd/wyrd.pdca-wt`, base `origin/main` @ `1943ded`); in `multipart.rs` the symbol
name is the durable anchor, the line number is convenience.

---

## 1. What this round is, and what it is not

Round 5 is **not** a rebuild from zero. Rounds 1–4 converged on a record grammar the reviewers
accepted everywhere except one question, which oscillated for four rounds: **may a retired
generation name both an inline chunk list and a segment group?** The brief now settles it (R1,
and the "Decision 2026-09-11 (human, before round 5)" note): *exactly one*, both present is a
decode error. This round therefore starts from `iteration-v4/patch.diff` (which applies clean to
`1943ded`) and changes:

| # | Change | Where |
|---|---|---|
| 1 | A generation carries **one** [`RetiredMap`] — `Flat(Vec<ChunkRef>) \| Segmented(SegmentGroup)` — mirroring `metadata::ChunkMap` (`metadata.rs:1014`) | `multipart.rs:2668` (`RetiredMap`), `:2708` (`RetireGeneration`), `:2767` (`TryFrom<RetireGenerationWire>`) |
| 2 | Both sources present ⇒ new typed `RecordError::RetireGenerationBothSources { inode, version }` | `multipart.rs:410` (variant), `:610` (`Display`), `:2770` (raise site) |
| 3 | `RetireGeneration` serializes through a closed wire struct, the shape `SegmentedMap` uses (`metadata.rs:983-992`), so the absent source is omitted rather than spelled `null` | `multipart.rs:2731` |
| 4 | `decode_retire_obligation` returns **`(RetireMode, RetireToken, RetirePayload)`** — the round-4 carry-forward's finding 1 | `multipart.rs:3238` |
| 5 | `checked_chunks` takes a **present** list (`Vec<ChunkRefWire>`), the absence question moving to its two callers | `multipart.rs:2627`, `:2770`, `:3193` |
| 6 | Module doc + the `RetirePayload` writer table + the living-architecture sentence corrected to "one of the two, never both" | `multipart.rs:2676-2701`, `:2924-2925`, `05-building-block-view.md:202` |
| 7 | Test: the hybrid **acceptance** witness is gone; `a_generation_naming_both_sources_is_rejected` and `the_mode_is_part_of_the_decoded_obligation` are new | `multipart_retire_obligation.rs:444`, `:393` |

Everything else in the patch is round 4's, reviewed and unchanged.

## 2. Why the two-arm type, not "keep the pair and add a check"

The brief's decision note says "tightens the shape check". I went one step further and made the
pair **unrepresentable**, because the cheaper reading would have left the defect the decision was
about still expressible inside the program:

* the obligation mirrors the map it retires, and that map is the two-arm `ChunkMap`
  (`metadata.rs:1002-1021`); a `SegmentedMap` has no inline chunk list at all
  (`metadata.rs:863`). A pair-shaped `RetireGeneration` would keep asking every later consumer
  (#656–#659, #693, #655) "and what if both?" — the question this cycle burned four rounds on;
* ADR-0045 decision 1 is *parse-don't-validate*: "a value that decodes is structurally
  trustworthy thereafter" (`0045:42-49`). A struct with two `Option`-ish sources and an invariant
  held only by its constructor is exactly the "validate" half the ADR replaces.

**Cost of the stricter choice, measured, not adjectival:** the generation region of the module
(`struct RetireGenerationWire` through `impl TryFrom<RetireGenerationWire>`) goes from **48 to 79**
non-blank non-comment lines — **+31**. What buys them: the `enum` itself (4), the manual
`Serialize` through a closed wire struct (26, see §4 for why not `flatten`), the `map()` accessor
(3), the four-arm `match` that replaces the old two-line guard (+2); against that it drops the two
`skip_serializing_if` attributes, one of the two fields, and the `chunks()`/`segments()` accessor
pair (−4 net). Its typed `RecordError` variant and `Display` arm are a further +9. In exchange the
"both" state cannot be constructed by a later writer even in memory, and no consumer of this
format has to ask the question again.

**Honest limit of the typed check (also stated in the test's own doc, `:444`).** Once the type is
a choice, `{chunks, segments}` could not survive the canonical-bytes gate anyway: the encoder
emits exactly one field, so the re-encode of a both-fields value is never the identity, and
`require_canonical` (`multipart.rs:1727`) would refuse it as `NoncanonicalRecordValue`. The typed
variant is therefore about **attribution**, not about narrowing the accepted set — the same
division of labour the patch already makes for a present-but-empty chunk list (`checked_chunks`,
`:2627`, whose doc argues it explicitly). The R1 negation below shows precisely this: with the
check removed the value is still refused, but as "non-canonical bytes", which sends an operator
looking for whitespace instead of at a generation naming two maps. What *does* narrow the
accepted set relative to round 4 is the **type**: round 4's pair round-tripped byte-identically
and was accepted (`iteration-v4` test `bytes_generation_with_both_sources_decodes`).

## 3. The round-4 carry-forward, item by item

1. **Mode discarded by `decode_retire_obligation`** — fixed: the decoder now answers
   `(RetireMode, RetireToken, RetirePayload)` (`multipart.rs:3238`), with the doc stating why the
   mode is part of the answer and not a check made and dropped (`:3211-3217`). The test leg the
   finding asked for is `the_mode_is_part_of_the_decoded_obligation`
   (`multipart_retire_obligation.rs:393`): the same `{"parts":[[1,4]]}` bytes under
   `retire:bytes:s:…` and `retire:records:s:…` decode to **equal payloads and equal tokens** and
   **different modes** — the drain can tell "orphan-mark then delete" from "delete records, never
   orphan" from the decode result alone. No production caller had to change (there is none yet).
2. **The hybrid generation** — fixed, §1 and §2 above.
3. **C4 CI red (`cargo deny` / RUSTSEC-2026-0258 `h2`)** — **gone on this base.**
   `cargo xtask ci` is green end to end on `1943ded` (§6), `cargo deny check` included. Nothing in
   this patch touches dependencies; the advisory was resolved upstream of us. Carried here only so
   the human does not go looking for it.
4. **T5 Judgment: `RetirePayload`'s `Deserialize` boundary** — already closed in round 4 and kept:
   `RetirePayload` derives `Serialize` only (`multipart.rs:2966-2967`), so
   `metadata::decode::<RetirePayload>` does not compile and `decode_retire_obligation` is the only
   way to obtain one. `PartNumberSet` and `PartScope` carry no `Deserialize` either; the wire types
   that do are private. I did not re-open it.

Also carried from earlier rounds and still satisfied: the round-3 C5 survivor (an empty
`generation.chunks` must not pass R2 — `checked_chunks` refuses it where it is read, witnessed in
`an_obligation_owing_nothing_is_rejected`), and the round-1 finding about a record-mode or
sessionless `parts:"all"` (`a_component_under_the_wrong_mode_is_rejected` +
`the_all_parts_wildcard_outside_a_session_teardown_is_rejected`).

## 4. Alternatives ruled out

* **Untagged serde enum for the generation** (`#[serde(untagged)]`, ~4 lines instead of the 14-line
  hand-written match): rejected. Its rejection message is "data did not match any variant of
  untagged enum …", which names an internal type and no rule, and it cannot distinguish
  *both* from *neither* — the two rejections the brief separates (R1 vs R2). The same argument the
  module already makes for `PartScopeWire`'s hand-written visitor (`multipart.rs:2559`).
* **`#[serde(flatten)]` for the generation's map instead of a manual `Serialize`** (saves ~18
  lines): rejected. `flatten` routes through serde's map-collecting path, which reorders/buffers
  keys and is the classic source of a re-encode that is not the identity — the one property this
  record class cannot lose (`0016:369-373`, `:667`).
* **Keeping `bytes_generation_with_both_sources_decodes` as a "documented acceptance"**: rejected —
  it is the exact witness the round-4 adversary refuted from the data model, and the human's
  decision is that no writer can install it.
* **Editing `0016:355`'s row to remove the `chunks, segments?` spelling**: out of scope (INTEGRATION
  §2 immutability; the brief pins it). The erratum belongs in the PR description; the code's doc
  comment records the decision and cites the two lines it resolves (`multipart.rs:2682-2693`).
* **Dropping `PartNumberSet::from_numbers`** (−10 semantic lines, −14 test lines, would have put the
  patch closer to the 1,000-line budget): rejected. It is the writer-side half of the
  canonical-spelling rule the brief's Salvage note tells this child to take, and without it the
  first writer (#656–#659) hand-builds a run vector — precisely how a second spelling of one
  obligation gets stored.

## 5. The ten isolating negations (forced evidence)

Method: `negate.py` applies **one** minimal edit to `crates/core/src/multipart.rs`, runs
`cargo test -p wyrd-core --test multipart_retire_obligation`, then restores the file byte-for-byte.
Each negation must fail **exactly one** test. All ten do — every run reads
`27 passed; 1 failed` out of 28.

```
=== negation: R1-generation-both ===   [remove the both-sources rejection arm]
test a_generation_naming_both_sources_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"generation":{"inode":42,"version":4,"chunks":[{...}],"segments":{"nonce":"c3c3…","epoch":2}}}
  left: Err(NoncanonicalRecordValue { namespace: "retire:" })
 right: Err(RetireGenerationBothSources { inode: 42, version: 4 })
test result: FAILED. 27 passed; 1 failed

=== negation: R2-owes-nothing ===   [checked_shape -> Ok(())]
test an_obligation_owing_nothing_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {}
  left: Ok((Bytes, Session { upload_id: UploadId("a1a1…"), epoch: 7, part: None }, RetirePayload { session: false, parts: None, chunks: [], generation: None, seg: None }))
 right: Err(RetireObligationOwesNothing { component: "payload" })
test result: FAILED. 27 passed; 1 failed

=== negation: R3-mode ===   [Component::checked_mode never errors]
test a_component_under_the_wrong_mode_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,…}]}
  left: Ok((Records, Session { …, part: Some((PartNumber(3), AttemptId("b2b2…"))) }, RetirePayload { …, chunks: [ChunkRef { id: 9, … }], … }))
 right: Err(RetireModeMismatch { key_mode: Records, component: "chunks" })
test result: FAILED. 27 passed; 1 failed

=== negation: R4-session-under-part ===   [drop the (SessionWide, part: Some(_)) arm]
test a_session_wide_component_under_a_per_part_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"session":true,"parts":"all"}
  left: Ok((Bytes, Session { …, part: Some((PartNumber(3), AttemptId("b2b2…"))) }, RetirePayload { session: true, parts: Some(All), … }))
 right: Err(RetireTokenSuffixMismatch { component: "session", token_names_part: true })
test result: FAILED. 27 passed; 1 failed

=== negation: R4-chunks-under-session ===   [drop the (PerPart, part: None) arm]
test chunks_under_a_session_wide_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,…}]}
  left: Ok((Bytes, Session { …, part: None }, RetirePayload { …, chunks: [ChunkRef { id: 9, … }], … }))
 right: Err(RetireTokenSuffixMismatch { component: "chunks", token_names_part: false })
test result: FAILED. 27 passed; 1 failed

=== negation: R5-generation-identity ===   [drop the (inode, version) comparison]
test a_generation_disagreeing_with_its_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"generation":{"inode":43,"version":4,"chunks":[…]}}
  left: Ok((Bytes, Generation { inode: 42, version: 4 }, RetirePayload { …, generation: Some(RetireGeneration { inode: 43, version: 4, map: Flat([…]) }), … }))
 right: Err(RetireGenerationIdentityMismatch { key_inode: 42, key_version: 4, payload_inode: 43, payload_version: 4 })
test result: FAILED. 27 passed; 1 failed

=== negation: R6-segment-epoch ===   [drop the group.epoch() == token.epoch comparison]
test a_segment_group_epoch_other_than_the_tokens_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"seg":{"nonce":"c3c3…","epoch":6}}
  left: Ok((Records, Session { …, epoch: 7, part: None }, RetirePayload { …, seg: Some(SegmentGroup { nonce: SegmentNonce("c3c3…"), epoch: 6 }) }))
 right: Err(RetireSegmentEpochMismatch { key_epoch: 7, segment_epoch: 6 })
test result: FAILED. 27 passed; 1 failed

=== negation: R7-chunk-scheme ===   [drop the checked_chunk_scheme loop in checked_chunks]
test an_unsupported_chunk_scheme_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":0,"m":1}},…}]}
  left: Ok((Bytes, Session { …, part: Some(…) }, RetirePayload { …, chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 0, m: 1 }, … }], … }))
 right: Err(ChunkSchemeUnsupported { chunk_id: 9, k: 0, m: 1 })
test result: FAILED. 27 passed; 1 failed

=== negation: R8-noncanonical-set ===   [make the coalesced-runs rule unreachable]
test a_noncanonical_part_number_set_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"parts":[[1,2],[3,4]]}
  left: Ok((Bytes, Session { …, part: None }, RetirePayload { …, parts: Some(Set(PartNumberSet([(1, 2), (3, 4)]))), … }))
 right: Err(PartNumberRunsNotCoalesced { lo: 3, previous_hi: 2 })
test result: FAILED. 27 passed; 1 failed

=== negation: R9-canonical-bytes ===   [return the payload without require_canonical]
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
assertion `left == right` failed: decode->encode is not byte-identical for RetirePayload { session: true, parts: Some(Set(PartNumberSet([(1, 4)]))), … }
  left: "{\"session\":true,\"parts\":[[1,4]]}"
 right: "{\"parts\":[[1,4]],\"session\":true}"
test result: FAILED. 27 passed; 1 failed
```

Two of the ten needed their edit shaped so the crate still **compiles** under `-D warnings`
(`R3-mode` keeps reading `self.mode` but never errors; `R8` keeps `previous_hi` read by making the
comparison unreachable rather than deleting it). That is a property of the negation, not of the
rule: in both the check no longer rejects anything.

R1's completeness legs are negated the other way, by construction rather than by edit: the ten
acceptance tests (`multipart_retire_obligation.rs:257-427`) each decode one writer row's payload
under its own key, so a payload type whose components were mutually exclusive could not compile
them — `{session, parts}` (`:274`), `{parts} + {seg}` (`:412`), `{session, all}` (`:257`).

## 6. Gates run here (the project's own runner, not a hand-rolled command)

* `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`) — **exit 0, all checks passed**:
  typos, `lint_docs`, `render_site --check` (link audit OK — this is the R10 docs edit's gate),
  gitlink/unsafe guards, `cargo fmt --check`, clippy `--all-targets`, build, the whole test suite,
  `cargo-machete`, `cargo deny check` **and** the all-features advisory pass, conformance vectors,
  the ADR-0035 statics gate, the deploy guard, and the madsim DST tier.
* `scripts/mutants-in-diff` (the C5 row) — **71 mutants: 40 caught, 31 unviable, 0 missed**
  (round 3 left 1 survivor; round 4, 1). The new arms are each pinned by a leg.
* `cargo test -p wyrd-core --test multipart_retire_obligation` — 28 passed.
* Reverted-production leg (the C4-verify RED the brief pre-declares as **UNVERIFIABLE / exit 77**):
  with `crates/core/src/multipart.rs` restored to `HEAD`, the test does not compile —
  `error[E0432]: unresolved imports wyrd_core::multipart::{decode_retire_obligation, PartNumberSet,
  PartScope, RetirePayload, RetiredMap}` plus `E0599` for each of the nine new `RecordError`
  variants. Born-at-tier, exactly as the brief's Falsifiability section predicts; the ten negations
  above are the behavioural red it stands in for.

## 7. The three refutation questions

* **(a) Genuine red?** Yes, in the only two senses available to a born-at-tier slice. Reverting the
  *whole* production hunk makes the test fail to compile (§6, pasted). Reverting **one check at a
  time** makes exactly one leg fail, ten times out of ten (§5, pasted). No leg stayed green under
  its own negation.
* **(b) Production path?** Yes. Every witness goes through `wyrd_core::multipart::
  decode_retire_obligation` — the function this patch adds — and re-encodes through the store-wide
  `wyrd_core::metadata::encode`. There is no test-local decoder, no mock and no re-implementation:
  the helper at `multipart_retire_obligation.rs:178` is a two-line wrapper around the production
  call plus the identity assertion. The only non-decode entry point exercised is
  `PartNumberSet::from_numbers`, also production.
* **(c) Fixture includes the fault?** Yes. Each rejection leg feeds the **actual** malformed
  witness rather than curating it out: a generation naming both maps (`:444`), `{}` and
  `{"chunks":[]}` and a chunk-less generation (`:467`), every component under the opposite mode
  (`:495`), a session-wide payload under a per-part token and `chunks` under a session-wide one
  (`:544`, `:564`), `(inode+1, version)` and `(inode, version+1)` under a `g:` token (`:580`),
  `E−1` and `E+1` segment epochs (`:623`), `rs(0,1)` in both chunk positions (`:677`), four
  non-canonical part-set spellings and both out-of-range endpoints (`:697`), four foreign
  spellings of accepted values (`:729`). The accepted set keeps the two boundary witnesses that
  pin what is deliberately **not** checked — a placement-length mismatch still decodes (`:773`)
  and a foreign segment-group nonce is undetectable at decode (`:649`) — so the suite cannot pass
  by over-rejecting either.

## 8. Size accounting (the standing T1 signal)

Added non-blank, non-comment lines: **1,056** across exactly 3 files — `multipart.rs` 519,
`multipart_retire_obligation.rs` 536, `05-building-block-view.md` 1 (one long paragraph line).
The brief's budget is ≤1,000; the file count — the brief's hard "STOP and hand back" condition —
is met at 3.

Round 4's patch measures **997** by the same counter, so this round is **+59** and that is what
crosses the line. Every one of the 59 is the decision the brief made this round, itemised:
**+31** the exclusivity type (§2, measured), **+9** its typed error and `Display` arm, **+13** the
test delta (the both-sources rejection leg +10 and the mode leg +15, against the hybrid acceptance
witness −12), **+6** the `flat()` test helper. Nothing else grew.

The one lever I could have pulled to get back under: drop `PartNumberSet::from_numbers` and its
test (≈24 lines). I did not (§4) — it is the writer-side half of the canonical-spelling rule the
brief's Salvage note tells this child to take, and trading it for a number would leave the first
writer hand-building run vectors. Nor did I drop a negation leg. The human weighed and accepted
this overshoot at round 4's sign-off ("overriding the bundle's own size-backstop recommendation of
iterate-plan"); it is 5.6% now rather than ~0%, for a format decision that took four rounds.

## 9. For the human at sign-off

* **No NEEDS-HUMAN external dependency.** Everything the brief listed was present and ran:
  `typos`, `docs-renderer` (both prose gates green), `cargo-deny`, `cargo-machete`,
  `cargo-mutants`. No Docker, no service, no new crate, no `Cargo.toml`/`Cargo.lock` change.
* **The C4-verify UNVERIFIABLE (exit 77) is expected**, pre-declared by the brief: the RED leg
  cannot compile because the criterion is born at this tier.
* **The `0016:355` erratum** (its generation row spells `chunks, segments?`, which reads as a
  union) is recorded in the code's doc comment at `multipart.rs:2682-2693` and belongs in the PR
  description. This slice does not edit the proposal (INTEGRATION §2).
* Scratch used: `$PDCA_SCRATCH/pdca-builder-771-work` (negation script + gate logs), removed at the
  end of the run.
