# Build notes — #771, multipart retirement obligation (iteration 3)

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`,
base `a801997`). Three files, as the brief's scope fixes:

| File | Added semantic lines |
|---|---|
| `crates/core/src/multipart.rs` | **490** (brief's estimate: ≈ 500) |
| `crates/core/tests/multipart_retire_obligation.rs` (new) | **502** (≈ 480) |
| `docs/design/architecture/05-building-block-view.md` | **1** |
| **total (nonblank, non-comment `+` lines)** | **993** — under the brief's ≤ 1,000 |

The count is the reviewer's own method (round 2 reported 966 for the previous patch; the same
filter reproduces 966 on `iteration-v2/patch.diff` exactly, so 993 is comparable, not a
re-definition).

---

## 1. What this iteration changes, and why (the carry-forward)

This is a **rebuild of iteration v2**, not a new design: v2's decode boundary (key-taking decoder,
component table, mode/scope/identity/epoch relations, canonical-bytes gate) is kept, and three
recorded defects are fixed. Round-2 sign-off carried four items; each is answered below.

### (a) The hybrid `generation` — the round-2 T4 blockers and the adversary's [human] finding

**Round-2 state.** `RetireGeneration` held `chunks: Vec<ChunkRef>` **and** `segments:
Option<SegmentGroup>` as independent fields, rejected only when *both* were absent, and its doc
licensed "a generation whose root carried both". The two gating T4 findings asked for an
**acceptance / round-trip** witness of that hybrid; the adversary pass argued the opposite — that
no writer can install it — and left the direction to a rebuild rather than guessing
(`iteration-v2/check-advisory-adversary.md:10-32`).

**Decision taken here: the hybrid is a typed rejection, and the union is unrepresentable.**

* An object's root carries exactly **one** `ChunkMap` — `Flat(Vec<ChunkRef>) |
  Segmented(SegmentedMap)` (`crates/core/src/metadata.rs:1014-1021`), one field on the record
  (`crates/core/src/metadata.rs:1388`), and `SegmentedMap` carries no inline chunk list
  (`crates/core/src/metadata.rs:863-866`). A flat generation's bytes are named by its inline
  chunks; a segmented one's by its `seg:` range, which the drain re-reads for current placements
  (`0016:2417-2425`). **No batch installs both** — the two writer rows are the supersede at
  `0016:662` and the unlink at `0016:668`, each naming one root.
* `0016` spells the row twice and inconsistently — `{inode, version, chunks, segments?}` at `:355`
  against `{inode, version, chunks?, segments}` at `:2417`. Each spelling is **one arm's**; the
  union is what neither writes. Recorded in the code (`crates/core/src/multipart.rs:2619-2630`,
  `:394-412`) rather than in the proposal, which this child does not edit (brief scope).
* The brief's success criterion is *"every shape no writer installs is rejected with a typed
  `RecordError`"*. Accepting the hybrid would have violated it — and would have frozen it into a
  record format three later slices build against.
* **Direction of reversibility** (the argument that settles it without guessing): refusing a shape
  nothing stores can be relaxed later without stranding a byte; **accepting** it now could never be
  refused again without making durable records unreadable. Between two readings of a contradictory
  spec, the reversible one is the one to freeze.

Shape: `RetiredMap { Flat(Vec<ChunkRef>) | Segmented(SegmentGroup) }`
(`crates/core/src/multipart.rs:2632-2660`), flattened into `RetireGeneration` so the stored
spelling is unchanged — `{inode, version, chunks: […]}` or `{inode, version, segments: {…}}`
(`:2662-2675`) — with `TryFrom<RetireGenerationWire>` as the only way in (`:2698-2717`) and a new
`RecordError::RetireGenerationTwoMaps` (`:394-413`, Display at `:608-612`). Test:
`a_generation_naming_two_maps_is_rejected`
(`crates/core/tests/multipart_retire_obligation.rs:548-556`); both accepted arms keep their own
round-trip witnesses (`:319-332`, `:335-346`).

The T4 findings are therefore **fixed at their premise, not silenced**: the decoder no longer
"explicitly supports" the hybrid, so there is no accepted shape left untested. This is a
disagreement with the *remedy* those two findings proposed (freeze the hybrid by testing its
acceptance), not with the observation behind them; the observation — "an accepted shape with no
witness" — is what the rejection removes.

### (b) R9's over-claim — "a file-wide property that cannot fail"

The adversary showed `decode_witness`'s identity `assert_eq!` cannot fail for any accepted witness,
because production's `require_canonical` has already proved the same equality before returning
(`iteration-v2/check-advisory-adversary.md:34-49`, verified by deleting the assert: still 26/26).

Fixed as the adversary recommended — **the claim, not the code**
(`crates/core/tests/multipart_retire_obligation.rs:22-35`): the header now states that production
enforces R9 (`require_canonical`, `crates/core/src/multipart.rs:3175`), that the pinning leg is
`a_foreign_spelling_of_an_accepted_payload_is_rejected` (`:670`), and that the helper is a
cross-check — it *does* fire for a witness production stops rejecting (negation **N9** below shows
exactly that: with the gate removed, the helper's assert is what reports the non-identity), and it
would catch a refactor comparing a normalized form. No claim of independent evidence remains.

### (c) `PartNumberSet::from_numbers` could mint a value its own decode refuses

`from_numbers([])` returned `PartNumberSet(vec![])`, which serializes to `[]`, which
`checked_shape` then refused — against the "unrepresentable at the source" thesis its own doc
stated, and a live hazard for the #656–#659 writer that computes the root flip's *possibly empty*
unnamed-staged-part set (`0016:662`, `:919-921`) and would store an obligation no drain can decode
under a key the terminal-delete gate (`0016:673`) never sees cleared.

Fixed in the type, per the adversary's cheapest-fix note: `from_numbers` returns
`Option<Self>` — `None` for an iterator naming no part (`crates/core/src/multipart.rs:2481-2494`)
— and `from_runs` rejects the empty spelling at the boundary
(`:2446-2450`, `RetireObligationOwesNothing { component: "parts" }`, the same typed error the
decode leg already asserted). `is_empty()` is gone: emptiness is no longer a state the type has,
so `RetirePayload::checked_shape` drops its parts arm (`:2985-2999`). Test:
`part_number_set_minting_is_canonical_and_never_empty` (`:789-800`).

### (d) C5/T5 "hybrid witness" and T1 size

* C5/T5 asked for the hybrid *acceptance* witness — answered by (a) with a rejection witness and
  the format decision recorded; a reviewer who disagrees has one line of code and one test to flip,
  and the reversibility argument above is the reason to flip it only deliberately.
* T1 (round 1, ~1,060 semantic lines) stays satisfied: **993**, itemised at the top. The new type
  and its error cost lines, so four were paid back by deleting `PartNumberSet::is_empty`, the
  now-unreachable `checked_shape` arms, `RetiredMap::segmented()` (an accessor with no consumer and
  no test — the module's own stated discipline) and the identity fields of the new error variant
  (`RetireAllPartsWithoutSession` is the sibling precedent for a rule *between* components carrying
  no payload of its own).

---

## 2. The nine isolating negations (brief's binding requirement), plus four more

Method: remove exactly one production check, run `cargo test -p wyrd-core --test
multipart_retire_obligation`, record the failing test, `git checkout` the file. Every one of the
nine fails **exactly one** test. Full outputs below (trimmed to the assertion).

| # | Check removed (`crates/core/src/multipart.rs`) | Test that failed | Result |
|---|---|---|---|
| N1 | R2 — `checked_shape`'s "owes nothing" (`:2993`) | `an_obligation_owing_nothing_is_rejected` | 26 passed; **1 failed** |
| N2 | R3 — `Component::checked_mode` rejection (`:3081`) | `a_component_under_the_wrong_mode_is_rejected` | 26 passed; **1 failed** |
| N3 | R4a — session-wide component under a per-part token (`:3106`) | `a_session_wide_component_under_a_per_part_token_is_rejected` | 26 passed; **1 failed** |
| N4 | R4b — per-part component under a session-wide token (`:3109`) | `chunks_under_a_session_wide_token_is_rejected` | 26 passed; **1 failed** |
| N5 | R5 — generation identity (`:3058`) | `a_generation_disagreeing_with_its_token_is_rejected` | 26 passed; **1 failed** |
| N6 | R6 — segment-group epoch (`:3068`) | `a_segment_group_epoch_other_than_the_tokens_is_rejected` | 26 passed; **1 failed** |
| N7 | R7 — `checked_chunk_scheme` over both chunk lists (`:2991`) | `an_unsupported_chunk_scheme_is_rejected` | 26 passed; **1 failed** |
| N8 | R8 — the coalescing/canonical rule in `from_runs` (`:2456-2463`) | `a_noncanonical_part_number_set_is_rejected` | 26 passed; **1 failed** |
| N9 | R9 — `require_canonical` in `decode_retire_obligation` (`:3175`) | `a_foreign_spelling_of_an_accepted_payload_is_rejected` | 26 passed; **1 failed** |
| N10 | the `all`-wildcard's one writer row (`:3049-3051`) | `the_all_parts_wildcard_outside_a_session_teardown_is_rejected` | 26 passed; **1 failed** |
| N11 | the retired generation's one-map rule (`:2703`) | `a_generation_naming_two_maps_is_rejected` | 26 passed; **1 failed** |
| N12 | the empty-set rule in `from_runs` (`:2447`) | `an_obligation_owing_nothing_is_rejected` | 26 passed; **1 failed** |
| N12b | `from_numbers`' `None` for an empty iterator (`:2493`) | `part_number_set_minting_is_canonical_and_never_empty` | 26 passed; **1 failed** |
| N13 | **R1, negated the other way** — payload collapsed to one component | see below | 21 passed; **6 failed** |

### Outputs

```
### N1 (R2) — the obligation-owes-nothing check
test an_obligation_owing_nothing_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- an_obligation_owing_nothing_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {}
  left: Ok((Session { upload_id: UploadId("a1a1…"), epoch: 7, part: None },
            RetirePayload { session: false, parts: None, chunks: [], generation: None, seg: None }))
 right: Err(RetireObligationOwesNothing { component: "payload" })

### N2 (R3) — the component mode check (Component::checked_mode)
test a_component_under_the_wrong_mode_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_component_under_the_wrong_mode_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for
  {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}]}
  left: Ok((Session { …, part: Some((PartNumber(3), AttemptId("b2b2…"))) },
            RetirePayload { chunks: [ChunkRef { id: 9, … }], … }))
 right: Err(RetireModeMismatch { key_mode: Records, component: "chunks" })

### N3 (R4, first direction) — the session-wide-under-a-per-part-token arm
test a_session_wide_component_under_a_per_part_token_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_session_wide_component_under_a_per_part_token_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {"session":true,"parts":"all"}
  left: Ok((Session { …, part: Some((PartNumber(3), AttemptId("b2b2…"))) },
            RetirePayload { session: true, parts: Some(All), … }))
 right: Err(RetireTokenSuffixMismatch { component: "session", token_names_part: true })

### N4 (R4, second direction) — the per-part-component-under-a-session-wide-token arm
test chunks_under_a_session_wide_token_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- chunks_under_a_session_wide_token_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for
  {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}]}
  left: Ok((Session { …, part: None }, RetirePayload { chunks: [ChunkRef { id: 9, … }], … }))
 right: Err(RetireTokenSuffixMismatch { component: "chunks", token_names_part: false })

### N5 (R5) — the generation identity check
test a_generation_disagreeing_with_its_token_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_generation_disagreeing_with_its_token_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for
  {"generation":{"inode":43,"version":4,"chunks":[{"id":9,…}]}}
  left: Ok((Generation { inode: 42, version: 4 },
            RetirePayload { generation: Some(RetireGeneration { inode: 43, version: 4,
                                                               map: Flat([ChunkRef { id: 9, … }]) }), … }))
 right: Err(RetireGenerationIdentityMismatch { key_inode: 42, key_version: 4,
                                               payload_inode: 43, payload_version: 4 })

### N6 (R6) — the segment-group epoch check
test a_segment_group_epoch_other_than_the_tokens_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_segment_group_epoch_other_than_the_tokens_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {"seg":{"nonce":"c3c3…","epoch":6}}
  left: Ok((Session { …, epoch: 7, part: None },
            RetirePayload { seg: Some(SegmentGroup { nonce: SegmentNonce("c3c3…"), epoch: 6 }), … }))
 right: Err(RetireSegmentEpochMismatch { key_epoch: 7, segment_epoch: 6 })

### N7 (R7) — the nested chunk-geometry check (both of a payload's chunk lists)
test an_unsupported_chunk_scheme_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- an_unsupported_chunk_scheme_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for
  {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":0,"m":1}},"len":100,"placement":[5]}]}
  left: Ok((…, RetirePayload { chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 0, m: 1 }, … }], … }))
 right: Err(ChunkSchemeUnsupported { chunk_id: 9, k: 0, m: 1 })

### N8 (R8) — the canonical-spelling rule of the part-number set
test a_noncanonical_part_number_set_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_noncanonical_part_number_set_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {"parts":[[1,2],[3,4]]}
  left: Ok((…, RetirePayload { parts: Some(Set(PartNumberSet([(1, 2), (3, 4)]))), … }))
 right: Err(PartNumberRunsNotCoalesced { lo: 3, previous_hi: 2 })

### N9 (R9) — the canonical-bytes gate (require_canonical)
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_foreign_spelling_of_an_accepted_payload_is_rejected stdout ----
assertion `left == right` failed: decode->encode is not byte-identical for
  RetirePayload { session: true, parts: Some(Set(PartNumberSet([(1, 4)]))), … }
  left: "{\"session\":true,\"parts\":[[1,4]]}"
 right: "{\"parts\":[[1,4]],\"session\":true}"
```

(N9 is also the evidence for §1(b): with the production gate gone, it is `decode_witness`'s own
identity assertion that reports the foreign spelling — the helper is a backstop, not a tautology
*in general*, but it cannot fire while the gate stands.)

```
### N10 (the all-wildcard's one writer row)
test the_all_parts_wildcard_outside_a_session_teardown_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- the_all_parts_wildcard_outside_a_session_teardown_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {"parts":"all"}
  left: Ok((…, RetirePayload { session: false, parts: Some(All), … }))
 right: Err(RetireAllPartsWithoutSession)

### N11 (the retired generation's one-map rule) — hybrid re-admitted as a flat map
test a_generation_naming_two_maps_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- a_generation_naming_two_maps_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for
  {"generation":{"inode":42,"version":4,"chunks":[{…}],"segments":{"nonce":"c3c3…","epoch":7}}}
  left: Err(NoncanonicalRecordValue { namespace: "retire:" })
 right: Err(RetireGenerationTwoMaps)

### N11b (compound, supplementary) — one-map rule AND require_canonical removed together
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
test a_generation_naming_two_maps_is_rejected ... FAILED
test result: FAILED. 25 passed; 2 failed
  → with both gone the hybrid is ACCEPTED (Ok(...)), which is the honest statement of what the
    one-map rule buys: alone it fixes the ATTRIBUTION (a typed shape error instead of "not this
    codec's spelling"); together with the type it makes the union unrepresentable for a writer as
    well as a reader. Recorded rather than overclaimed.

### N12 (R2, type-level half) — the non-emptiness rule of from_runs
test an_obligation_owing_nothing_is_rejected ... FAILED
test result: FAILED. 26 passed; 1 failed
---- an_obligation_owing_nothing_is_rejected stdout ----
assertion `left == right` failed: unexpected verdict for {"parts":[]}
  left: Ok((…, RetirePayload { parts: Some(Set(PartNumberSet([]))), … }))
 right: Err(RetireObligationOwesNothing { component: "parts" })

### N12b — from_numbers made total again
test part_number_set_minting_is_canonical_and_never_empty ... FAILED
test result: FAILED. 26 passed; 1 failed
---- part_number_set_minting_is_canonical_and_never_empty stdout ----
assertion `left == right` failed
  left: Some(PartNumberSet([]))
 right: None

### N13 (R1, negated the other way) — payload collapsed to ONE component
    (the archived v2/v3 shape: mutually exclusive arms)
test bytes_session_and_all_parts_decodes ... FAILED
test bytes_session_and_parts_decodes ... FAILED
test records_parts_and_seg_decodes ... FAILED
test a_generation_disagreeing_with_its_token_is_rejected ... FAILED
test a_session_wide_component_under_a_per_part_token_is_rejected ... FAILED
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
test result: FAILED. 21 passed; 6 failed
  → the three combined-shape acceptance legs are the first three: `{session, all}`,
    `{session, parts}` and `{parts} + {seg}` cannot decode under a single-arm payload. R1 is
    load-bearing.
```

---

## 3. The three forced questions

**(a) Genuine red?** Yes, thirteen times over — §2. Each of the nine mandated rules, plus four
more, was removed from **production** and the named test went red; every one of the nine isolates
(exactly one test fails). The whole-patch red (revert everything) is *not* a behavioural red: this
is born-at-tier functionality, so the reverted base does not compile the test at all and `C4-verify`
reports **UNVERIFIABLE (exit 77)**. That is pre-declared in the brief's Falsifiability section as a
known sign-off item, and the negations are what the brief substitutes for it.

**(b) Production path?** Yes. Every witness goes through the production entry point
`wyrd_core::multipart::decode_retire_obligation(key, value)`
(`crates/core/src/multipart.rs:3163`) via one helper (`decode_witness`,
`crates/core/tests/multipart_retire_obligation.rs:177`), with keys minted by the production
`retire_key` (`crates/core/src/multipart.rs:1092` on the base) and values re-encoded through the
store-wide `wyrd_core::metadata::encode`. No mock, no re-implementation, no test-only decoder: the
payload type has **no** `Deserialize` at all, so there is no second decode path to accidentally
test (`crates/core/src/multipart.rs:2878-2896`). Every witness is **decoded, never constructed** —
`RetirePayload` has no writer-side constructor.

**(c) Fixture includes the fault?** Yes. Each rejection leg's witness *contains* the fault it is
about, spelled in hand-authored JSON: a payload with no component, a component under the other
mode's key prefix, a session-wide payload under a per-part token (and the converse), a generation
whose `(inode, version)` differs from its `g:` token's, `seg.epoch = E±1`, `rs(0,1)` geometry
inside both chunk lists, `[[1,2],[3,4]]` and `[[0,3]]` and `[[1, MAX+1]]`, and five foreign
spellings of accepted values. Nothing is curated out: the accepted set and the refused set are
authored from the **same** writer table (`0016:355-356`, `:659-673`, `:2187`, `:2193`, `:2417`),
and the two boundary witnesses that must *not* be rejected (a placement-length mismatch,
`crates/core/tests/multipart_retire_obligation.rs:706`; a foreign segment-group nonce, `:590`) are
asserted to decode, so the rules cannot pass by over-rejecting.

---

## 4. Alternatives considered, with costs

1. **Keep the hybrid accepted and add its round-trip witness** (what the two T4 findings asked
   for). Cost: ~14 test lines, ~0 module lines — genuinely cheaper *as a diff*. Rejected because it
   is not a cost question: it would freeze a shape `ChunkMap` cannot produce
   (`crates/core/src/metadata.rs:1014-1021`) into a record format #656–#659, #693 and #655 build
   against, and contradict the brief's success criterion. The reversibility argument (§1a) is the
   decider: refusing is undoable, accepting is not.
2. **Keep the two optional fields and add an exclusivity check in `checked_shape`.** Cost:
   ~6 module lines vs. the ~30 the enum costs (`RetiredMap` + accessor + `TryFrom` arms, measured
   on the diff). Rejected: it leaves the illegal state representable *in memory*, so a #656–#659
   writer could still build `RetireGeneration { chunks, segments }` and only find out at decode —
   the same defect class as `from_numbers` minting an empty set (§1c). ADR-0045 decision 1's point
   is that the type carries the rule; a 24-line difference does not buy back a representable
   illegal state.
3. **`RetirePayload` as a closed enum of the nine writer rows.** Rejected in v2 and still: it makes
   `{session, parts}` and `{parts} + {seg}` inexpressible — the exact reviewed defect of #692's v2
   and #717's v3, demonstrated by negation **N13** (6 tests red, three of them the combined-shape
   acceptance legs).
4. **A wider `seg` epoch window (`E`, `E±1`)** — the archived #717 v3 shape. Rejected on
   `0016:369-373`: one obligation with several legal keys is one `require_absent` cannot refuse
   twice. Unchanged from v2; `a_segment_group_epoch_other_than_the_tokens_is_rejected` pins it and
   the doc states the **limit** of the check (the nonce is not bound — `0016:499-509`).
5. **Adding a `retire:` arm to `crate::metadata::decode` so the file could keep the sibling's S1
   leg.** Rejected: `crates/core/src/metadata.rs` is explicitly out of scope (and in #721/#722's
   conflict set), and a value-only decode surface is precisely what this record class must not
   have.

---

## 5. Commit-readiness and gates run here

* `cargo fmt -p wyrd-core` — clean (run before measuring and before emitting the diff), so the
  target's own commit hook has nothing to reformat.
* `cargo test -p wyrd-core --test multipart_retire_obligation` — **27 passed**.
* `cargo test -p wyrd-core` — every suite green.
* `./engine/xtask.sh ci` — the project's own gate, run in `$PDCA_WORKTREE`. Green through
  `typos`, the docs renderer, the gitlink and unsafe-forbid guards, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets` (warnings-as-errors from `[workspace.lints]`),
  `cargo build --all-targets`, the whole `cargo test --workspace` run and `cargo-machete` — then
  **stopped at `cargo deny check`** (§6).
* The steps `cargo deny` short-circuited were run individually afterwards and all pass:
  `./engine/xtask.sh conformance`, `./engine/xtask.sh statics`, `./engine/xtask.sh dst` (the
  50-seed madsim sweep) — `exit=0`, no failures.
* `cargo doc -p wyrd-core --no-deps` — no rustdoc diagnostic anywhere in `multipart.rs`
  (`broken_intra_doc_links` is `deny` in `[workspace.lints.rustdoc]`). The two errors it does
  report are pre-existing links in `crates/core/src/write.rs:120` and `:540`, untouched here.

## 6. The one gate that is red, and it is not this patch

`cargo deny check` fails on **`RUSTSEC-2026-0258` — `h2 0.4.15`** ("unbounded empty DATA frames",
low severity, patched in `0.4.16`), reached transitively through `hyper`/`tonic`/`aws-smithy`
(`Cargo.lock:111`). `cargo deny` reads only `Cargo.toml`/`Cargo.lock` and the advisory database;
**this patch touches neither** — its three files are `crates/core/src/multipart.rs`,
`crates/core/tests/multipart_retire_obligation.rs` and
`docs/design/architecture/05-building-block-view.md` — so the failure is a repo-wide, patch-
independent condition that will make `C4-ci` red for **any** bundle until the dependency moves.
The fix (`cargo update -p h2`) is a `Cargo.lock` change the brief explicitly puts out of scope
("no `Cargo.toml` / `Cargo.lock` change"), and choosing between bumping and recording a reviewed
`deny.toml` ignore is a human call, not a builder's workaround.

NEEDS-HUMAN external dependency: cargo-deny advisory database (RUSTSEC-2026-0258, `h2 0.4.15`) — blocks the gating `C4-ci` (`cargo xtask ci`) at its `cargo deny check` step for the whole workspace, so a fully-green gate run cannot be produced for this bundle; every other CI step, including the 50-seed DST sweep, passes. Resolve by `cargo update -p h2` (to ≥ 0.4.16) on `main` or by a reviewed `deny.toml` ignore — both outside this child's scope.

```toml
[[doctor.checks]]
id    = "cargo-deny-advisories"   # the token Plan should have put in `External dependencies`
cmd   = "cd \"${WYRD_REPO:-../wyrd}\" && cargo deny check advisories"
hint  = "A fresh RUSTSEC advisory against a workspace dependency makes `cargo xtask ci` (C4-ci) red for every cycle, regardless of the patch. Bump the dependency (`cargo update -p <crate>`) or record a reviewed `deny.toml` ignore before starting a cycle."
level = "WARN"        # the slice still builds and tests; only the gate's deny step is red
```

## 7. Honest limits

* **No production reach**, and none claimed: this child has no writer, no store call and no live
  consumer — the record grammar lands ahead of the store round trips (#656–#659), exactly as
  `AdmissionRecord` (#715) and `SessionRecord` (#716) did (`crates/core/src/multipart.rs:72-85`).
  The binding evidence is the named test over hand-authored bytes plus the thirteen negations.
* **`C4-verify` will report UNVERIFIABLE (exit 77)** — pre-declared in the brief.
* **What R6 does *not* prove**, stated in the code and the test rather than glossed: the payload's
  segment-group **nonce** is independent of the upload id (`0016:499-509`), so a foreign session's
  group carrying the right epoch is not detectable at decode. Only the epoch component is bound
  (`crates/core/tests/multipart_retire_obligation.rs:590-608`).
* **The one format decision a human may want to overturn** is §1(a). It is one match arm
  (`crates/core/src/multipart.rs:2703`) and one test; the reversibility argument is why it is
  frozen in the refusing direction rather than the accepting one.

No external dependency was missing: this child is pure functions over hand-authored bytes and
needed nothing beyond the base Rust toolchain (`typos` / `docs-renderer` / `cargo-deny` /
`cargo-machete` / `cargo-mutants` are the brief's registered ids and are exercised by the gate, not
by this build).
