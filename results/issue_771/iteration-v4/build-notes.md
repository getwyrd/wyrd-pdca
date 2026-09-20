# Build notes — #771 `multipart-retire-obligation` (iteration 4)

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`
at `a801997` (= `origin/main`; the brief's floor is `c824243`). Three files, as scoped:

| File | Added semantic lines (nonblank / non-`//`) |
|---|---|
| `crates/core/src/multipart.rs` | 478 |
| `crates/core/tests/multipart_retire_obligation.rs` (NEW) | 518 |
| `docs/design/architecture/05-building-block-view.md` | 1 |
| **total** | **997** (budget ≤ 1,000; round-3's re-derivation of the same rule gave 993) |

Counting rule = the reviewer's in round 3 ("added nonblank/non-comment lines"): every `+` line
of `patch.diff` that is neither blank nor a `//` comment.

---

## 1. What this iteration changed, and why

This is a **rebuild on the round-3 patch** (`iteration-v3/patch.diff`), not a fresh design: the
sign-off accepted its shape and named four defects. Each is addressed below with the production
line it lives on in the new tree.

### (1) C5 mutant survivor — R2 did not fire for a present-but-empty chunk list

Round 3's `RetireGeneration` decode carried the emptiness rule as a **match guard**
(`iteration-v3` `multipart.rs:2704`, `(Some(chunks), None) if !chunks.is_empty()`), and
`cargo mutants` replaced that guard with `true` while all 27 tests stayed green
(`iteration-v3/gate-logs/C5-mutants.log:13`). The reason the mutant survived is that no witness
exercised it: `{"generation":{…,"chunks":[]}}` was never authored, and the payload-level "owes
nothing" rule cannot see it (the payload *does* carry a `generation` component, so the outer
count is non-zero).

Fixed by moving the rule out of the guard into a **shared, typed check at the one place a
retirement chunk list is read** — `checked_chunks` (`crates/core/src/multipart.rs:2598`), used by
both the payload's own `chunks` (`:3108`) and the generation's (`:2698`). A present-but-empty list
is `RetireObligationOwesNothing { component }` naming *which* list it was, and the wire field is
now `Option<Vec<ChunkRefWire>>` (`:2625`, `:2827`) so "absent" and "present but empty" are
different values rather than the same `Vec::default()`. Witnesses added to
`an_obligation_owing_nothing_is_rejected`
(`crates/core/tests/multipart_retire_obligation.rs:428-445`): `{"chunks":[]}`,
`{"generation":{…,"chunks":[]}}`, plus `{"session":false}` (an explicitly-false flag is not a
component). Negations R2b/R2c below show each is now load-bearing.

`checked_chunks` also absorbed the R7 geometry loop that used to live in `checked_shape`, so the
two rules a chunk list has are applied where the list is read, once, for both lists — a second
copy of either is what the previous shape risked.

### (2) T2 Shape FAIL — the generation could not carry `chunks` **and** `segments`

Round 3 read `0016`'s two spellings of the generation row as a contradiction and resolved it with
a two-armed `RetiredMap` (`Flat` | `Segmented`), refusing the union as
`RetireGenerationTwoMaps`. The reviewer and the human both judged that wrong, and re-reading the
source they are right: `0016:355` writes `{generation: {inode, version, chunks, segments?:
<upload-id>:<epoch>}}` — `chunks` **required**, `segments` **optional** — which states the union
outright; `0016:2417` writes the same row with the optionality swapped. The brief's R1 spells it
`{generation: {inode, version, chunks?, segments?}}`.

So `RetiredMap` is **deleted** (with `RecordError::RetireGenerationTwoMaps` and its `Display`
arm) and `RetireGeneration` now stores the pair (`multipart.rs:2658-2665`, accessors `:2667-2690`):

```rust
pub struct RetireGeneration {
    inode: InodeId,
    version: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    chunks: Vec<ChunkRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segments: Option<SegmentGroup>,
}
```

with exactly one shape rule left — it names **something** (`:2699-2703`) — and both fields omitted
when absent, so serialization identity is unchanged. Accessors `chunks()` / `segments()` replace
`map()`/`as_flat()`. The accepted witness is
`bytes_generation_with_both_sources_decodes` (test `:348-372`), and negation **R1b** shows that
re-introducing the exclusivity fails exactly that leg.

Direction-of-error note, since round 3 argued the opposite: refusing a shape a *writer* needs is
**not** the conservative direction. A writer that must retire a generation naming both sources
would have to install two obligations under two keys — and `require_absent` plus the session's
own emptiness gate (`0016:369-380`) are written for one obligation per key. That is the same
"combined shape inexpressible" defect the brief's R1 was written to stop, one level in.

### (3) Architecture doc — a new paragraph asserting behaviour this tree does not have

Round 3 added a **separate paragraph** at `05-building-block-view.md:204` describing install/drain
CAS mechanics (`require_absent` on install, exact-bytes on drain) that no code in this tree
performs, immediately after a paragraph saying "Nothing writes or consumes these records in
production yet".

That paragraph is removed. R10 asks for the **existing sentence** (`:202`, the one #715/#716 grew)
to gain the two `retire:` namespaces and what their values carry, so the retirement pair is now
folded into that same sentence's list, plus one sentence for the rule this child's decoder adds —
the value is decoded **against its own key**. One line changed, none added
(`git diff --stat`: `2 +-`). No behaviour is claimed that does not exist: the surrounding
paragraph's "Nothing writes or consumes these records in production yet — the protocol itself …
arrives with the store round trip (#656–#659)" now covers the retirement namespaces too, which is
exactly true.

### (4) T5 — `RetirePayload`'s decode boundary

The item is carried from the **round-2** deferred set (`deferred-findings.json`,
`"through_round": 2`, citing v2's `multipart.rs:2762`/`:2994`), where `RetirePayload` derived
`Deserialize`. Round 3 already removed it, and this iteration keeps that: **no** public type in
section 9 derives `Deserialize` (`PartNumberSet`, `PartScope`, `RetireGeneration`,
`RetirePayload` are `Serialize`-only), so `metadata::decode::<RetirePayload>(…)` does not compile
and `decode_retire_obligation(key, value)` is the only surface that exists. The module header
(`multipart.rs:24-29`) and the type doc (`:2867-2878`) both state it. Verified mechanically: the
test file's own header records that the S1 leg of the sibling `decode_both` helper cannot exist
for this class, and `grep -n "Deserialize" multipart.rs` in section 9 matches only the private
`*Wire` types.

---

## 2. The record-format decisions this child freezes (unchanged from round 3, restated)

* **Mode lives in the key** (`0016:434-441`): each component carries the mode of its own writer
  row in the `Component` table (`multipart.rs:2753-2807`), so `session`/`parts:"all"`/`chunks`/
  `generation` are `retire:bytes:` only and `seg` is `retire:records:` only.
* **Token scope** (`0016:358-366`): `generation` only under `g:`, everything else only under
  `s:`, and the optional `:<part>:<attempt>` suffix belongs to `chunks` alone — both directions
  refused (the #692 recorded defect).
* **R6, the canonical token epoch**: `token.epoch == seg.epoch` exactly, `E±1` refused, with the
  limit stated and not over-claimed — the group **nonce** is deliberately independent of the
  upload id (`0016:499-509`), so a foreign group under your token is not detectable at decode.
  Both the production doc (`:3005-3018`, `checked_against_key` at `:3019`) and the test
  (`a_segment_groups_nonce_and_a_generations_epoch_are_unchecked`) say so.
* **R8**: the part-number set is canonical — ordered, non-overlapping, non-adjacent, endpoints in
  `[1, MAX_PART_NUMBER]`, `lo <= hi`, never empty — and a `ChunkRef` whose `placement` length
  disagrees with its scheme still **decodes** (the contextual boundary this child must not cross).
* **`parts: "all"`** has one writer row (`0016:2187`), so it is legal only inside the
  `{session, all}` teardown and only under `retire:bytes:`.

---

## 3. Red → green

**Pre-fix (RED) — criterion-absence, as pre-declared in the brief.** With `crates/core/src/multipart.rs`
reverted to `origin/main` and the new test file kept, the test target does not compile:

```
error[E0432]: unresolved imports `wyrd_core::multipart::decode_retire_obligation`,
  `wyrd_core::multipart::PartNumberSet`, `wyrd_core::multipart::PartScope`,
  `wyrd_core::multipart::RetirePayload`
  --> crates/core/tests/multipart_retire_obligation.rs:61:5
error[E0599]: no variant named `RetireObligationOwesNothing` found for enum `RecordError`
  --> crates/core/tests/multipart_retire_obligation.rs:430:49
error[E0599]: no variant named `RetireModeMismatch` found for enum `RecordError` …
```

This is the pre-declared **UNVERIFIABLE (exit 77)** leg for `C4-verify`, not a surprise.

**Post-fix (GREEN).** `cargo test -p wyrd-core --test multipart_retire_obligation` →
`27 passed; 0 failed`. Full project gate `./engine/xtask.sh ci` (= `cargo xtask ci` in the
worktree): typos ✅, `lint_docs` ✅, `render_site --check` (99 pages, link audit OK) ✅,
gitlink-guard ✅, unsafe-guard ✅, `cargo fmt --all --check` ✅, `cargo clippy --workspace
--all-targets` ✅, build ✅, **168 test binaries green** (including
`tests/multipart_retire_obligation.rs`), conformance ✅ — and **`cargo deny check` red on
`RUSTSEC-2026-0258` (`h2 0.4.15` unbounded empty DATA frames)**, see §6.

## 4. The isolating negations — twelve, each failing exactly one test

Method: neutralise **one** production check in `crates/core/src/multipart.rs`, run
`cargo test -p wyrd-core --test multipart_retire_obligation`, record the failures, restore the
pristine file (scripted; the script re-writes the file from a pristine copy in
`$PDCA_SCRATCH` after every run, and the module was byte-compared to that copy afterwards).
The brief requires **nine**; the two R2 component rules and the `all`-wildcard rule bring it to
twelve, and R1 is negated the other way (acceptance) in §5.

| # | Rule negated (production site) | Failing test(s) |
|---|---|---|
| R2 | `checked_shape`'s "owes something" (`:2971`) | `an_obligation_owing_nothing_is_rejected` (1) |
| R2b | `checked_chunks`' present-but-empty list (`:2605`) — **the round-3 mutant survivor** | `an_obligation_owing_nothing_is_rejected` (1) |
| R2c | generation names neither source (`:2699`) | `an_obligation_owing_nothing_is_rejected` (1) |
| R3 | `component.checked_mode(mode)?` (`:3025`) | `a_component_under_the_wrong_mode_is_rejected` (1) |
| R4a | scope arm `SessionWide` + suffixed token (`:3090`) | `a_session_wide_component_under_a_per_part_token_is_rejected` (1) |
| R4b | scope arm `PerPart` + suffix-free token (`:3093`) | `chunks_under_a_session_wide_token_is_rejected` (1) |
| R5 | generation identity vs `g:` token (`:3037`) | `a_generation_disagreeing_with_its_token_is_rejected` (1) |
| R6 | `seg.epoch == token.epoch` (`:3047`) | `a_segment_group_epoch_other_than_the_tokens_is_rejected` (1) |
| R7 | `checked_chunk_scheme` over both lists (`:2610`) | `an_unsupported_chunk_scheme_is_rejected` (1) |
| R8 | run coalescing in `PartNumberSet::from_runs` (`:2440`) | `a_noncanonical_part_number_set_is_rejected` (1) |
| R9 | `require_canonical` in `decode_retire_obligation` (`:3154`) | `a_foreign_spelling_of_an_accepted_payload_is_rejected` (1) |
| R10 | `all` without the `session` teardown (`:3028`) | `the_all_parts_wildcard_outside_a_session_teardown_is_rejected` (1) |

Verbatim failure output, the first assertion of each run (the runs themselves were driven from a
scratch harness under `$PDCA_SCRATCH/pdca-builder-771-neg/`, removed at the end of the beat — this
transcript is the record):

```
=== R2-payload-owes-nothing ===  exit=101 failing tests (1): an_obligation_owing_nothing_is_rejected
assertion `left == right` failed: unexpected verdict for {}
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { session: false, parts: None, chunks: [], generation: None, seg: None }))
 right: Err(RetireObligationOwesNothing { component: "payload" })

=== R2b-present-but-empty-chunk-list ===  exit=101 failing tests (1): an_obligation_owing_nothing_is_rejected
assertion `left == right` failed: unexpected verdict for {"chunks":[]}
  left: Err(RetireObligationOwesNothing { component: "payload" })
 right: Err(RetireObligationOwesNothing { component: "chunks" })

=== R2c-generation-names-neither-source ===  exit=101 failing tests (1): an_obligation_owing_nothing_is_rejected
assertion `left == right` failed: unexpected verdict for {"generation":{"inode":42,"version":4}}
  left: Ok((Generation { inode: 42, version: 4 }, RetirePayload { … generation: Some(RetireGeneration { inode: 42, version: 4, chunks: [], segments: None }), seg: None }))
 right: Err(RetireObligationOwesNothing { component: "generation" })

=== R3-mode-in-the-key ===  exit=101 failing tests (1): a_component_under_the_wrong_mode_is_rejected
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}]}
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2…"))) }, RetirePayload { … chunks: [ChunkRef { id: 9, … }] }))
 right: Err(RetireModeMismatch { key_mode: Records, component: "chunks" })

=== R4a-session-wide-under-per-part ===  exit=101 failing tests (1): a_session_wide_component_under_a_per_part_token_is_rejected
assertion `left == right` failed: unexpected verdict for {"session":true,"parts":"all"}
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2…"))) }, RetirePayload { session: true, parts: Some(All), … }))
 right: Err(RetireTokenSuffixMismatch { component: "session", token_names_part: true })

=== R4b-per-part-under-session-wide ===  exit=101 failing tests (1): chunks_under_a_session_wide_token_is_rejected
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,…}]}
  left: Ok((Session { … part: None }, RetirePayload { … chunks: [ChunkRef { id: 9, … }] }))
 right: Err(RetireTokenSuffixMismatch { component: "chunks", token_names_part: false })

=== R5-generation-identity ===  exit=101 failing tests (1): a_generation_disagreeing_with_its_token_is_rejected
assertion `left == right` failed: unexpected verdict for {"generation":{"inode":43,"version":4,"chunks":[…]}}
  left: Ok((Generation { inode: 42, version: 4 }, RetirePayload { … generation: Some(RetireGeneration { inode: 43, version: 4, … }) }))
 right: Err(RetireGenerationIdentityMismatch { key_inode: 42, key_version: 4, payload_inode: 43, payload_version: 4 })

=== R6-canonical-token-epoch ===  exit=101 failing tests (1): a_segment_group_epoch_other_than_the_tokens_is_rejected
assertion `left == right` failed: unexpected verdict for {"seg":{"nonce":"c3…c3","epoch":6}}
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { … seg: Some(SegmentGroup { nonce: SegmentNonce("c3…"), epoch: 6 }) }))
 right: Err(RetireSegmentEpochMismatch { key_epoch: 7, segment_epoch: 6 })

=== R7-nested-chunk-geometry ===  exit=101 failing tests (1): an_unsupported_chunk_scheme_is_rejected
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":0,"m":1}},…}]}
  left: Ok((Session { … }, RetirePayload { … chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 0, m: 1 }, … }] }))
 right: Err(ChunkSchemeUnsupported { chunk_id: 9, k: 0, m: 1 })

=== R8-noncanonical-part-set ===  exit=101 failing tests (1): a_noncanonical_part_number_set_is_rejected
assertion `left == right` failed: unexpected verdict for {"parts":[[1,2],[3,4]]}
  left: Ok((Session { … }, RetirePayload { … parts: Some(Set(PartNumberSet([(1, 2), (3, 4)]))) … }))
 right: Err(PartNumberRunsNotCoalesced { lo: 3, previous_hi: 2 })

=== R9-canonical-bytes-gate ===  exit=101 failing tests (1): a_foreign_spelling_of_an_accepted_payload_is_rejected
assertion `left == right` failed: decode->encode is not byte-identical for RetirePayload { session: true, parts: Some(Set(PartNumberSet([(1, 4)]))), … }
  left: "{\"session\":true,\"parts\":[[1,4]]}"
 right: "{\"parts\":[[1,4]],\"session\":true}"

=== R10-all-parts-outside-a-teardown ===  exit=101 failing tests (1): the_all_parts_wildcard_outside_a_session_teardown_is_rejected
assertion `left == right` failed: unexpected verdict for {"parts":"all"}
  left: Ok((Session { … part: None }, RetirePayload { session: false, parts: Some(All), … }))
 right: Err(RetireAllPartsWithoutSession)
```

Note R9's negation: dropping `require_canonical` does **not** leave the file green — the
`decode_witness` helper's identity assertion catches the foreign spelling that production stopped
refusing, which is exactly the cross-check the test header claims for it (round 2's "R9's
advertised file-wide property is unfalsifiable" finding is answered by that leg, not by prose).

## 4b. Mutation evidence — the round-3 survivor is gone, and nothing replaced it

`PDCA_BUNDLE=… scripts/mutants-in-diff` (the C5 gate's own command, `cargo mutants --in-diff
patch.diff --no-shuffle`) over this bundle's diff, in `$PDCA_WORKTREE`:

```
Found 74 mutants to test
ok       Unmutated baseline in 10s build + 1s test
74 mutants tested in 2m: 43 caught, 31 unviable
```

**0 missed** (`mutants.out/missed.txt` is empty), against round 3's `1 missed, 46 caught, 29
unviable`. The mutants over the new code are caught individually, including the ones the redesign
introduced — e.g. `multipart.rs:2699:30: replace && with || in <impl TryFrom<RetireGenerationWire>
for RetireGeneration>::try_from`, `:2681 RetireGeneration::chunks -> Vec::leak(Vec::new())`,
`:2686 RetireGeneration::segments -> None`, `:2675 version -> 0 / 1`.

## 5. R1 negated the other way (acceptance)

* **R1a — collapse the payload's combined shape** (`checked_shape`'s `…next().is_none()` →
  `…count() != 1`, i.e. the arms become mutually exclusive, the reviewed v2/v3 defect):
  6 tests fail, including all three combined-shape legs —
  `bytes_session_and_all_parts_decodes`, `bytes_session_and_parts_decodes`,
  `records_parts_and_seg_decodes` (plus three rejection legs whose witnesses are themselves
  combined values).
* **R1b — collapse the generation hybrid** (re-introduce "both sources ⇒ error"):
  **exactly one** test fails, `bytes_generation_with_both_sources_decodes` —
  `Err(RetireObligationOwesNothing { component: "generation" })` where the writer row of
  `0016:355` needs an accepted value.

## 6. Refutation of my own test (forced answers)

**(a) Genuine red?** Yes, twice over. With the whole production change reverted the test does not
compile (§3, criterion-absence — the brief's pre-declared UNVERIFIABLE red). With production
present, each of the twelve single-rule negations in §4 turns the suite red on exactly one test,
and the two R1 collapses in §5 turn it red on the acceptance legs. No rule in the diff is
unpinned.

**(b) Production path?** Yes. Every witness goes through `decode_retire_obligation(key, value)` —
the production entry point in `crates/core/src/multipart.rs` — with keys minted by the production
`retire_key(mode, &token)` and bytes re-encoded through the production store-wide
`metadata::encode`. There is no stand-in, no re-implementation, and no writer-side constructor:
the types have none, so a witness cannot be *constructed*, only decoded. The one thing the test
cannot do is drive an S1 (`metadata::decode::<RetirePayload>`) surface, because that surface
deliberately does not exist (§1(4)); that is stated in the test header rather than faked.

**(c) Fixture includes the fault?** Yes. The rejection legs author the **faulty** bytes by hand
and assert the typed error: the empty and present-but-empty payloads (R2), the wrong-mode key
(R3), the wrong-scope token in both directions (R4), the mismatched `(inode, version)` (R5), the
`E±1` epoch (R6), the `rs(0,1)` geometry (R7), the non-canonical/out-of-range run sets (R8), the
reordered/whitespaced/`false`/`null` spellings (R9). Nothing is curated out: the two boundary legs
deliberately keep faults this child must *not* reject in the fixture (a placement-length mismatch
and a foreign segment-group nonce) and assert they decode.

## 7. Alternatives considered and rejected (with costs)

* **Keep round 3's exclusive `RetiredMap` and record the union as "not installed".** Rejected on
  source: `0016:355` states the union (`chunks` required + `segments?` optional). Cost of keeping
  it is not diff size but a **frozen format defect** three slices build against (#656–#659, #693,
  #655); the reversibility argument round 3 made runs the wrong way here — a writer that needs
  both sources under one `g:` token has no legal single value, so it must install a second
  obligation under a second key, and `require_absent`/the emptiness gate assume one.
* **Fix the C5 survivor by adding a witness only, keeping the match guard.** That is what the
  human's item (1) suggested as a minimum ("normalize an empty `chunks` … add a witness"). I did
  the normalisation one level up instead, in `checked_chunks`, because the guard would otherwise
  have had to be repeated for the payload's own `chunks` field once the hybrid removed the
  two-armed enum: two spellings of one rule, in a file whose whole thesis is that a rule has one
  home. Concrete cost of the shared helper: **+22 semantic lines** (the `checked_chunks` fn),
  against **−31** removed with `RetiredMap`, `as_flat`, `RetireGenerationTwoMaps` and its
  `Display` arm; net −9, and the R7 loop moved rather than copied.
* **Attribute a present-but-empty list to `NoncanonicalRecordValue`** (it would be caught anyway
  by the re-encode gate, since `[]` is dropped on the way out). Rejected: the operator signal
  would name "non-canonical bytes" for a record whose real defect is that it owes nothing, and the
  R9 leg would then be doing R2's work — the isolation the brief demands (§4) would collapse into
  one test. Cost of the typed variant reuse: **0 new error variants** (it reuses
  `RetireObligationOwesNothing` with a `component` string).
* **`#[serde(untagged)]` for `PartScope`'s wire** (kept from round 3, restating why): the
  rejection message would read "data did not match any variant of untagged enum PartScopeWire",
  naming an internal type and no rule. The hand-written visitor is **+30 semantic lines** and
  names the two legal spellings.
* **A separate architecture-doc paragraph** (round 3's shape): rejected per R10's own instruction
  and the human's item (3) — one line changed in the existing sentence, no new paragraph, no claim
  about install/drain CAS behaviour this tree does not implement.

## 8. Known gate state — pre-existing `cargo deny` advisory

`cargo xtask ci` ends red on **`RUSTSEC-2026-0258`** (`h2 0.4.15`, "unbounded empty DATA frames"),
reached through `hyper`/`tonic`/`axum`. This is:

* **not caused by this patch** — `git diff --stat` is exactly `crates/core/src/multipart.rs`,
  `crates/core/tests/multipart_retire_obligation.rs`, `docs/design/architecture/05-building-block-view.md`;
  no `Cargo.toml`, no `Cargo.lock`, no dependency edge changed;
* **the same finding round 3 hit** (`iteration-v3/gate-logs/C4-ci.log:5244`, `Cargo.lock:1535`),
  and the sign-off for round 3 declared it **explicitly out of scope for this iteration**
  ("the C4-ci `cargo deny` / RUSTSEC-2026-0258 `h2` advisory (unrelated supply-chain finding)").

Fixing it means bumping `h2`/`hyper` across the workspace lockfile — a supply-chain change that
belongs in its own bundle, and one this child is forbidden to make (its scope is 3 files, and a
`Cargo.lock` edit would be a fourth). Everything else in `cargo xtask ci` is green, including the
prose gates the docs change needs (`typos`, `lint_docs`, `render_site --check`).

## 9. Scratch discipline

Everything throwaway lived under `$PDCA_SCRATCH/pdca-builder-771-neg/` (pristine copies for the
negation harness, `negate.py`, `negations.log`, the `xtask ci` and `cargo mutants` logs) and was
removed at the end of the beat. Nothing was written outside `$PDCA_WORKTREE` and the bundle
directory. The worktree is left carrying exactly the three files of `patch.diff` — verified twice:
`git diff --stat` shows only them, and `git stash -u` + `git apply --check patch.diff` against the
pristine `origin/main` tree reports the patch applies clean. `mutants.out/` is left where the C5
gate itself creates it (gitignored, `.gitignore:14`).
