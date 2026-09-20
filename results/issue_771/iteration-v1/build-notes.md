# Build notes — #771 `multipart-retire-obligation`

Target branch `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` at `a801997` (merge of #770;
`git log --oneline -3` → `a801997`, `278f709`, `d39e724`; the file's own history is `5eeca16`
(#715), `778f1cf` / `a3b2bbe` (#716) — the base the brief names).

Three files, as scoped:

| File | raw `+` | semantic `+` (non-comment, non-blank, non-attribute) |
|---|---|---|
| `crates/core/src/multipart.rs` | 850 | **436** (budget ≈ 500) |
| `crates/core/tests/multipart_retire_obligation.rs` (new) | 863 | **570** (budget ≈ 480) |
| `docs/design/architecture/05-building-block-view.md` | 2 | **1** (budget ≈ 20) |
| total | 1,715 | **1,007** vs the brief's ≤ 1,000 |

0.7 % over the envelope, all of it in the test file and none of it in the docs allowance I did not
spend. The overrun is nine acceptance legs (R1's eight shapes plus `{session}` alone) and three
boundary witnesses; I judged cutting one of them worse than being seven lines over, since each is
evidence the brief asks for by name. No fourth file.

---

## 1. What the child had to decide, and what I decided

The brief made two record-format decisions binding, both of them the reason the archived
attempts failed review. I took the salvage (`results/issue_717/iteration-v3/patch.diff`) for
`PartNumberSet::{from_runs, from_numbers}` and the shape of `decode_retire_obligation`, and
**changed both reviewed defects rather than re-shipping them**.

### R1 — the payload is a *set of components*, not a choice between them

The archived shape was `enum RetirePayload { Session {}, Parts { parts }, Chunks { … },
Generation { … }, Records { … } }` (salvage `patch.diff:935-986`). Its arms are mutually
exclusive, so `{session, parts}` (`0016:665`, `:823`, `:2193`) is **inexpressible** — a writer
following `0016` would have to install two records where the protocol installs one, i.e. two keys
where `require_absent` and the session's own emptiness gate expect one (`0016:369-380`). The
`Records` arm hid the same problem one level down: it could carry `{parts}`, `{seg}` or both, so
the type *did* model one combination and refused every other.

The landed shape is one struct of optional components — `session`, `parts`, `chunks`,
`generation`, `seg` — each carrying the mode and token scope of **its own writer row**
(`Component`, `multipart.rs` §9; the table is transcribed from `0016:355-356`, `:659-673`,
`:2187`, `:2193`, `:2417`). Every rule is then a lookup in that table, so no rule is spelled
twice and no component gets a weaker check than its siblings. All nine rows of the payload's own
writer table (`RetirePayload`'s doc — the brief's eight shapes plus `{session}` alone,
`0016:664`) decode as a single value under a single key (tests `bytes_session_all_decodes` …
`records_parts_and_seg_decodes`).

What I ruled out along the way, with the cost each carried:

* **Keeping the enum and adding a `SessionAndParts` arm.** It fixes one row, not the class:
  `0016:356` is explicitly "and/or", so records mode alone has three legal combinations, and the
  next writer row (`{session, parts}` in records mode, `0016:356`) would need a fifth arm. Arm
  count grows as 2^components; the component table is linear (5 constants, 20 lines).
* **Nesting the part set inside the session component** (`"session":"all"` /
  `"session":{"parts":[[1,4]]}`, with a separate top-level `parts`). It removes one redundancy
  but buys it with a worse one: the same part set would then have two homes, so
  `{"session":{"parts":S}}` and `{"parts":S}` under one bytes key would both be legal spellings
  of "these parts", and `{session, parts}` would no longer be two components in one value — which
  is precisely the property R1 exists to pin. Ruled out.

  What the shipped struct does instead is give the two components **different meanings**, so no
  implication rule is needed at all: `session` is the session's *own* records (the `mpu:` record
  and its surviving `slot:` records the terminal delete removes, `0016:673`), and `parts` is
  *which of its parts* the obligation covers — `all` (enumerate the session's own bounded range
  at drain time, because a list frozen before the fence lands would miss a part that won the
  read-then-fence window, `0016:2187`) or an explicit range-encoded set. So `{session}` alone
  (`0016:664` — records, no part) and `{session, all}` (`0016:2187` — records **and** every part)
  are *different* obligations, not two spellings of one, and both have exactly one spelling. Each
  has its own witness (`bytes_session_alone_decodes`, `bytes_session_all_decodes`).

* **`{"parts":{"set":[[1,4]]}}` (externally tagged `PartScope`).** Rejected because `0016:382`
  spells the value of `parts` as `[[1, 400]]` itself; a tag object would make the stored
  spelling differ from the proposal's for no gain. Cost of the alternative I took instead: a
  22-line hand-written `Deserialize` for `PartScopeWire`. I first used `#[serde(untagged)]`
  (6 lines) and reverted it: its rejection reads *"data did not match any variant of untagged
  enum PartScopeWire"*, naming an internal type and no rule — the visitor names the two legal
  spellings, and an operator can act on that.

### R6 — the token epoch is exact, not a window

The salvage admitted `token.epoch − group.epoch() ∈ {0, 1}` (salvage `patch.diff:1148`), with a
doc that also over-claimed F18 containment. Both are gone. Decode now enforces
`token.epoch == seg.epoch` exactly, and the reasoning is recorded in the code
(`RecordError::RetireSegmentEpochMismatch` and `RetirePayload::checked_against_key`), citing the
lines it settles: the fence that ends an attempt is the batch that installs the obligation naming
that attempt's segments and preconditions on `require(mpu == Completing@E)` (`0016:663-665`,
`:2357-2362`), so `E` is both the token's epoch and the epoch whose `seg:` keys the payload names.
A window would give one obligation several legal keys, and `require_absent` on one cannot see the
others — the obligation is then installed and drained twice (`0016:369-373`).

**The limit is stated, not over-claimed**, in the same doc and in a dedicated test
(`a_generations_segment_group_carries_an_unchecked_epoch`): the group **nonce** is deliberately
independent of the upload id because segment records outlive the `mpu:` tombstone
(`0016:499-509`), so a *foreign* group under your token is not detectable at decode. This check
binds the epoch component only; group identity is the writer's and the drain's (#656–#659). The
test asserts that a foreign nonce with the right epoch **is accepted**, so the claim cannot drift
upward later without a red test.

### Two smaller judgement calls

* **`session` is allowed under both modes.** `0016:356`'s records row literally reads
  "`{session, parts}` and/or `{seg: …}`", so refusing it would refuse a shape §1's own value
  column installs. R3's mode rule therefore binds exactly the three components the brief names:
  `chunks`/`generation` bytes-only, `seg` records-only.
* **A `generation` may name chunks *and* segments.** `0016:355` spells the row
  `{generation: {inode, version, chunks, segments?}}` and `:2417` spells it
  `{… chunks?, segments}`; the union is "both optional, either or both present". The salvage
  rejected the both-present case (`RetireGenerationSourcesConflict`, salvage `patch.diff:1050`);
  I dropped that variant rather than invent a rule that refuses a spelling §1 writes. Naming
  *neither* is still `RetireObligationOwesNothing` (R2).

---

## 2. What I reused rather than re-spelled (brief: "rather than adding a second spelling")

`RetireMode`, `RetireToken`, `parse_retire_key`, `retire_key` (`multipart.rs` §4);
`checked_chunk_scheme`, `ChunkRefWire`/`EcSchemeWire`, `require_canonical` (§8, §5);
`PartNumber::new` for every set endpoint; `metadata::SegmentGroup` for both segment-group fields
(its own validating `Deserialize`, `metadata.rs:826-837`); `erasure::supported` via
`checked_chunk_scheme` only. `decode_retire_obligation` takes `(key, value)` and returns
`(RetireToken, RetirePayload)` — the `decode_session_record`/`decode_part_record` wrapper shape
plus the one key parameter.

`PartNumberSet` deliberately has **no** `Deserialize` (the reason `metadata::SegmentNonce` has
none, `metadata.rs:757-760`): the only decode path is the payload's wire shape, which routes
through `from_runs`, so every R8 rejection arrives as its own typed `RecordError` instead of a
serde string.

---

## 3. Forced refutation — the three questions

**(a) Genuine red?** Yes, ten times over, each with the check actually reverted and the suite
re-run (§4). The whole test file is also red *by construction* on the base: it names
`decode_retire_obligation`, `RetirePayload`, `PartScope` and `PartNumberSet`, none of which exist
on `origin/main` (`git grep -n "RetirePayload\|PartNumberSet\|decode_retire_obligation"
origin/main -- crates/` → nothing), so with production reverted the test does not compile. That
is the pre-declared **UNVERIFIABLE (exit 77)** RED the brief registers for `C4-verify`; the nine
in-tree negations below are what stands in for a flippable red, per the brief's Falsifiability
section.

**(b) Production path?** Yes. Every witness goes through the shipped
`wyrd_core::multipart::decode_retire_obligation` (S2) *and* the store-wide
`wyrd_core::metadata::decode::<RetirePayload>` (S1) — the same codec every record in this
workspace is stored through. There is no test-local copy, mock or re-implementation of any rule;
the keys are minted by the production `retire_key`, and the identity assertion re-encodes with
the production `metadata::encode`.

**(c) Fixture includes the fault?** Yes, and this is the class the brief warns about: each
rejection witness is *otherwise valid* — the only thing wrong with it is the rule under test. The
R4 witnesses carry a real per-part token with a real attempt id (not a truncated one); the R6
witnesses carry a well-formed group whose nonce is the right length and whose epoch is off by one
in each direction; the R7 witnesses carry a fully-formed chunk whose only fault is `rs(0,1)`; the
R9 witnesses are byte-legal JSON that decodes to a payload the decoder otherwise accepts. Nothing
is curated out — which is why each negation fails exactly one test rather than none.

---

## 4. The nine isolating negations (plus R1's, negated the other way)

Each: remove that single check, run `cargo test -p wyrd-core --test multipart_retire_obligation`,
paste the failure, revert. **Exactly one test fails in each of the nine.** Full logs:
`$PDCA_SCRATCH/pdca-builder-771-negations/negations-full.md`.

### R2 — removed the obligation-owes-nothing check (`RetirePayload::checked_shape`)

```text
---- an_obligation_owing_nothing_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { session: false, parts: None, chunks: [], generation: None, seg: None }))
 right: Err(RetireObligationOwesNothing { component: "payload" })
test result: FAILED. 26 passed; 1 failed
```

### R3 — neutered the mode-in-the-key check (`Component::checked_mode`)

Removing the arm outright makes `Component::mode` dead code, which `-D warnings` refuses to
compile — itself evidence that this check is the field's only consumer — so the negation reads the
field and ignores the answer.

```text
---- a_component_under_the_wrong_mode_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2b2…"))) }, RetirePayload { … chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 2, m: 1 }, len: 100, placement: [5, 6, 7] }] … }))
 right: Err(RetireModeMismatch { key_mode: Records, component: "chunks" })
test result: FAILED. 26 passed; 1 failed
```

### R4, first direction — removed the `(SessionWide, Session { part: Some(_) })` arm

```text
---- a_session_wide_component_under_a_per_part_token_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2b2…"))) }, RetirePayload { session: true, parts: Some(All), … }))
 right: Err(RetireTokenSuffixMismatch { component: "session", token_names_part: true })
test result: FAILED. 26 passed; 1 failed
```

### R4, second direction — removed the `(PerPart, Session { part: None })` arm

```text
---- chunks_under_a_session_wide_token_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … part: None }, RetirePayload { … chunks: [ChunkRef { id: 9, … }] … }))
 right: Err(RetireTokenSuffixMismatch { component: "chunks", token_names_part: false })
test result: FAILED. 26 passed; 1 failed
```

### R5 — removed the generation identity comparison

```text
---- a_generation_disagreeing_with_its_token_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Generation { inode: 42, version: 4 }, RetirePayload { … generation: Some(RetireGeneration { inode: 43, version: 4, … }) … }))
 right: Err(RetireGenerationIdentityMismatch { key_inode: 42, key_version: 4, payload_inode: 43, payload_version: 4 })
test result: FAILED. 26 passed; 1 failed
```

### R6 — removed the `token.epoch == seg.epoch` comparison

```text
---- a_segment_group_epoch_other_than_the_tokens_is_rejected stdout ----
assertion `left == right` failed: epoch 6 under the epoch-7 token
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { … seg: Some(SegmentGroup { nonce: SegmentNonce("c3c3…"), epoch: 6 }) }))
 right: Err(RetireSegmentEpochMismatch { key_epoch: 7, segment_epoch: 6 })
test result: FAILED. 26 passed; 1 failed
```

(The same test also asserts `E + 1` and the accepted exact-`E` witness, so an "accept a window"
regression is red here too — that is the round-3 blocking finding, bound.)

### R7 — removed the nested chunk-geometry loop

```text
---- an_unsupported_chunk_scheme_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … }, RetirePayload { … chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 0, m: 1 }, len: 100, placement: [5] }] … }))
 right: Err(ChunkSchemeUnsupported { chunk_id: 9, k: 0, m: 1 })
test result: FAILED. 26 passed; 1 failed
```

### R8 — removed the coalesced-ascending comparison in `PartNumberSet::from_runs`

```text
---- a_noncanonical_part_number_set_is_rejected stdout ----
assertion `left == right` failed
  left: Ok((Session { … }, RetirePayload { … parts: Some(Set(PartNumberSet([(1, 2), (3, 4)]))) … }))
 right: Err(PartNumberRunsNotCoalesced { lo: 3, previous_hi: 2 })
test result: FAILED. 26 passed; 1 failed
```

### R9 — removed the canonical-bytes gate in `decode_retire_obligation`

The failure lands on `decode_both`'s **file-wide** identity assertion (test file line 161), not on
a bespoke one — the property is a property of the whole accepted set:

```text
---- a_foreign_spelling_of_an_accepted_payload_is_rejected stdout ----
assertion `left == right` failed: decode->encode is not byte-identical for RetirePayload { session: true, parts: Some(Set(PartNumberSet([(1, 4)]))), … }
  left: "{\"session\":true,\"parts\":[[1,4]]}"
 right: "{\"parts\":[[1,4]],\"session\":true}"
test result: FAILED. 26 passed; 1 failed
```

### R1 — negated the other way: collapse the payload to the archived shape

Added `if self.present_components().count() > 1 { return Err(…) }` — i.e. the mutually exclusive
arms of the #692/#717 shape. Six tests fail, three of them the combined-shape acceptance legs the
brief names:

```text
---- bytes_session_all_decodes stdout ----
the session teardown obligation: RetireObligationOwesNothing { component: "mutually exclusive arms" }
---- bytes_session_and_parts_decodes stdout ----
a session teardown naming its parts: RetireObligationOwesNothing { component: "mutually exclusive arms" }
---- records_parts_and_seg_decodes stdout ----
parts and segments in one obligation: RetireObligationOwesNothing { component: "mutually exclusive arms" }
test result: FAILED. 21 passed; 6 failed
```

---

## 5. Gates run here

* `cargo test -p wyrd-core --test multipart_retire_obligation` → **27 passed** (the `C4-verify`
  GREEN leg the brief names). `./engine/scripts/run-verify.sh --classify` on the shipped patch
  emits exactly the discriminator the brief predicts: `ADDED_TEST
  crates/core/tests/multipart_retire_obligation.rs` + `CRATE crates/core`.
* `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`: typos, docs-render, gitlink and
  unsafe-forbid guards, `fmt --check`, `clippy --workspace --all-targets`, `build`, `test`,
  machete, deny, conformance vectors, statics) → **all checks passed**, twice: once on the
  restored tree after the negations (the first run had overlapped them) and once after the final
  doc/API tidy. The only edits after that last green run are doc comments, re-verified with
  `cargo fmt --all -- --check` (clean), `cargo clippy -p wyrd-core --all-targets` (clean) and the
  test file (27 passed); the nine negations were then re-run against that final tree and still
  isolate one test each.
* `git apply --check --reverse patch.diff` against the patched worktree → clean, so the patch
  applies forward on the base.
* `cargo doc -p wyrd-core --no-deps` → no new rustdoc diagnostics from `multipart.rs` (four
  private-intra-doc-link errors my first draft introduced are fixed; the 15 pre-existing ones in
  `metadata.rs`/`erasure.rs` are untouched and `cargo doc` is not part of `xtask ci`).
* Commit-hook readiness: `cargo fmt --all` run over every touched file; the gate's own
  `fmt --check` and `clippy -D warnings` are green.

## 6. Scope discipline

`crates/core/src/metadata.rs` untouched (child-2's, and #721/#722's conflict set); no writer, no
store call, no `async fn`, no `WriteBatch`; no `Cargo.toml`/`Cargo.lock` change; no ADR, proposal
or spec edited — `0016` in particular is unmodified, and the R6 decision it leaves open is
recorded in the code's doc comments citing the lines it resolves, as instructed. The only
`docs/design/` edit is the one `05-building-block-view.md` paragraph (R10), extended in its own
voice beside the ADR-0047 bullet at `:187-194`; the stale module-header clause that deferred that
update to "the slice that first *persists* one" is withdrawn in the same hunk, and the header's
key table and "nothing here is written yet" section are brought up to date with #716 and this
child.

No external dependency beyond the base Rust toolchain was needed; nothing is deferred to an
off-Check environment.
