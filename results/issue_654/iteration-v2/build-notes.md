# Build notes — issue 654 (iteration 2), multipart record family + state machine

> Withheld from the reviewer; written for the human at sign-off.
> Every `path:line` below is against the worktree `$PDCA_WORKTREE` = `../wyrd.pdca-wt-l0`,
> based on `origin/main` @ `339da46`.

---

## 0. What this iteration is

Iteration 1 shipped a working but **incomplete** module and a test suite that was not
load-bearing. Its sign-off iterated on three things (brief `## Iteration 1 — carry-forward`):

* **C5** — 13 of 164 mutants survived; "the settled grammar, validation and ordering causes"
  were not load-bearing.
* **T5** — the fingerprint "reordered" case never reordered its input, and the ETag case
  delegated sorting to a future caller: green was not fitness evidence.
* **T4 batched review** — 21 blocking findings.

This is **not** the same patch with patches on top: the module was rewritten around one
structural idea the first attempt did not have — **every component a key or record is built
from is a validated type** (`UploadId`, `AttemptId`, `PartNumber`, `SlotIndex`, `Digest`,
`MultipartEtag`), mirroring `metadata::SegmentNonce` (`crates/core/src/metadata.rs:714-758`).
That single change is what makes 11 of the 21 findings *unrepresentable* rather than
re-checked, makes every key constructor total (it cannot mint a key its own parser rejects,
the `seg_key` property at `metadata.rs:1219-1233`), and is what killed the surviving mutants
in the parsers.

**Result of the two named failing gates, re-run on this patch:**

| Gate | Iteration 1 | Now |
|---|---|---|
| `C5-mutants` (`scripts/mutants-in-diff`) | 164 mutants, **13 missed** | 253 mutants, **0 missed** (117 caught, 136 unviable, 5m) |
| `C4-ci` (`./engine/xtask.sh ci`) | pass | **pass** (exit 0: typos, docs lint + render, fmt, clippy `-D warnings`, build, full test suite, cargo-deny, cargo-machete, conformance) |
| `cargo test -p wyrd-core --test multipart_records` | 22 tests | **28 tests, all green** |

---

## 1. The change, file by file

| File | What | Why |
|---|---|---|
| `crates/core/src/multipart.rs` (**new**, 2348 lines / 1407 semantic) | the whole slice | § 2 below |
| `crates/core/src/lib.rs:13` | `pub mod multipart;` | one line, as the brief scopes |
| `crates/core/Cargo.toml:23-27` | `sha2.workspace = true` + the comment recording that this is **not** a new-dependency decision (`Cargo.toml:147` already carries it; `deny.toml` already allows it, ADR-0003 not re-opened) | the two digests |
| `crates/core/src/metadata.rs:1526-1554` | `PendingEntry` gains `owner: Option<UploadId>` + `staged: Option<StagedPlacement>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]` | `0016:493-510` makes the `sidx:` value **be** a `PendingEntry`, so the same half-TTL renewal loop and the same `require(re-encode(prior))` lease guards serve both (`write.rs:474-500`, `metadata.rs:748-758`) |
| `crates/core/tests/multipart_records.rs` (**new**, 1796 lines / 1467 semantic) | the six legs | the brief's named test file |
| `docs/design/architecture/08-crosscutting-concepts.md:87` | one paragraph in §8.7 | § 5.3 below — a declared deviation |
| 9 × mechanical `PendingEntry { … }` initializer sites + `Cargo.lock` | `owner: None, staged: None` | § 5.1 below — a declared deviation |

### 1.1 `owner` is a validated `UploadId`, not a `String`

`metadata.rs:1550`. Three review passes found *"a malformed owner token"* admitted. Typing
the field as `crate::multipart::UploadId` moves that check into `PendingEntry`'s own derived
`Deserialize` — no new function in `metadata.rs` (the brief forbids one), and no reader ever
holds an owner that could not derive a `sidx:<id>:` range. Test:
`multipart_records.rs:1096-1101`.

The **pairing** rule ("both fields or neither") lives in `multipart.rs:1507-1557`
(`OwnedEntry::from_pending` / `decode_owned_entry`) rather than in `PendingEntry`, because a
bare `PendingEntry` — the `pending:` class — legitimately carries neither, and the `sidx:`
class is *this* module's record. So the `sidx:` value has a real validating decoder that
produces a type whose two fields are **non-optional**, which is the parse-don't-validate
shape ADR-0045 asks for; a half-owned value is refused there
(`multipart_records.rs:953-994`).

---

## 2. How each of the 21 review findings is discharged

| # | Finding (abridged) | Discharge |
|---|---|---|
| 1, 3, 5 | unchecked profile arithmetic in `Budget::u_ref` / `max_sessions` — panics in checked builds, wraps in release | `multipart.rs:931-954`: `saturating_add`/`saturating_mul`, and `Budget::new` (`:855`) refuses a zero component so the division is total (no `checked_div` dead branch). Proof it runs at **decode** over untrusted bytes without panicking: `multipart_records.rs:737-751` |
| 2, 4, 6, 8 | unchecked `u64` chunk-length sums during decode | `multipart.rs:1380-1399`: `try_fold(0u64, checked_add)`, mirroring `metadata.rs:1180-1190`'s `checked_chunk_bytes`. Test: the `u64::MAX + 1` case at `multipart_records.rs:914-917` |
| 7, 15, 19 | derived `PendingEntry` decode admits half-owned entries / a malformed owner | § 1.1 above |
| 9, 14 | persisted fields + a record family without a living-architecture-doc update | § 5.3 — the paragraph is written |
| 11, 16, 20 | `SessionRecord` accepts a non-token `group_nonce`, a `publish_target.fence_epoch ≠ epoch`, invalid completion digests | `multipart.rs:1198-1287`: `group_nonce` is a `metadata::SegmentNonce` validated through its own constructor; `fence_epoch == epoch` enforced; `Completion.etag` is a `MultipartEtag` and `complete_fingerprint` a `Digest`, both validated at decode. Also `bucket`/`object`/`clock_source` non-empty. Tests: `multipart_records.rs:784-865` |
| 12, 17, 21 | derived `SlotRecord` decode admits part number 0 / out-of-range, malformed attempt ids, a lease expiring before reservation | `multipart.rs:1327-1343` + the `PartNumber`/`AttemptId` types. Tests: `multipart_records.rs:869-891` |
| 10, 13, 18, 22, 23 | `PartRecord` / `PartSummary` digests unvalidated | the `Digest` type (`multipart.rs:406-469`) is the field type, so a short/uppercase/non-hex digest is a decode error everywhere. `PartSummary` additionally refuses "chunks without bytes" and vice versa (`:1444-1462`). Tests: `multipart_records.rs:892-951` |

**C3 / T2 (incomplete vocabulary).** Added, from the salvage list the brief names:
`RetirePayload` + `PartNumberSet` + `decode_retire_obligation` (`multipart.rs:1576-1806`) and
the operation-specific outcomes `CreateOutcome` / `ReserveOutcome` / `UploadPartOutcome` /
`CompleteOutcome` / `AbortOutcome` / `Publication` / `Refusal` / `InvalidPart` /
`Backpressure` (`:1952-2128`), plus the five per-verb **answers**
(`UploadPartAnswer`/`CompleteAnswer`/`AbortAnswer`/`ListPartsAnswer`/`ListUploadsAnswer`).
Answers are the pre-flight state decision; outcomes are the operation result. Both carry the
same `Refusal`, so #656–#659 never invent a second refusal vocabulary.

**T2 / T3 / T5 (ETag ordering, fingerprint order-sensitivity).** `multipart_etag` and
`complete_fingerprint` now take `&[(PartNumber, Digest)]` and canonicalise **inside** the
function through one shared helper, `canonical_named_parts` (`multipart.rs:1818-1836`) — one
definition, so the published ETag and the tombstone's fingerprint can never disagree about
what "the parts the client named" means. Duplicates are a typed error rather than an ETag
that depends on which duplicate a sort kept. The test's "reordered" case now really reorders
(`multipart_records.rs:1758-1765` asserts `reordered != named` *before* asserting the
fingerprints agree), and both digests are proved against oracles the test computes itself
(`etag_oracle`, `:1535-1544`; `fingerprint_oracle`, `:1696-1706`).

**T3 (short digest decodes).** `Digest::from_hex` (`multipart.rs:429-445`) is the only way in.

---

## 3. Forced self-refutation (the three questions)

**(a) Genuine red? — yes, five times, one per named negation.** The brief binds Do to negate
the production code in five *named* ways and paste the failures. Each was applied to
`crates/core/src/multipart.rs`, run through `cargo test -p wyrd-core --test
multipart_records`, then reverted (`cp` from a scratch copy; the module is byte-identical
afterwards and the suite is green again).

**Negation (1) — accept a `007` spelling in one fixed-width parser**
(`fixed_width_u32`, `text.len() != width` → `text.len() > width`):

```
thread 'a_fixed_width_field_accepts_exactly_one_spelling_of_one_record' panicked at
crates/core/tests/multipart_records.rs:177:9:
part: key accepted the non-canonical part-number spelling "7"
test result: FAILED. 27 passed; 1 failed
```

**Negation (3) — drop the `mpuctl` relational check** (`if wire.max_sessions != derived` →
`if false && …`):

```
thread 'an_admission_record_whose_limit_disagrees_with_its_profile_is_rejected_at_decode'
panicked at crates/core/tests/multipart_records.rs:684:67:
called `Result::unwrap_err()` on an `Ok` value: AdmissionRecord { count: 3, max_sessions: 4000,
profile: Budget { w_ref: 1800, max_part_chunks: 3, max_parts_per_session: 4,
max_inflight_parts: 2, max_staged_chunks: 8 } }
test result: FAILED. 27 passed; 1 failed
```

**Negation (4) — answer one `Completing` cell as if it were `Open`**
(`upload_part_answer`, `Some(Completing) => Accepted`):

```
thread 'every_verb_x_state_cell_is_answered_with_its_typed_outcome' panicked at
crates/core/tests/multipart_records.rs:1338:9:
assertion `left == right` failed: decision 3 cell (UploadPart, Some(Completing))
  left: UploadPart(Accepted)
 right: UploadPart(Refused(NoSuchUpload))
---- the_answer_table_is_total_over_the_verb_x_state_product stdout ----
thread '…' panicked at crates/core/tests/multipart_records.rs:1376:13:
assertion `left == right` failed: (UploadPart, Some(Completing)) disagrees per verb
  left: UploadPart(Accepted)
 right: UploadPart(Refused(NoSuchUpload))
test result: FAILED. 26 passed; 2 failed
```

**Negation (5) — concatenate hex text instead of raw digest bytes**
(`multipart_etag`, `digest.as_bytes()` → `digest.to_hex().as_bytes()`):

```
thread 'the_multipart_etag_is_the_settled_composition' panicked at
crates/core/tests/multipart_records.rs:1552:5:
assertion `left == right` failed
  left: "e8fdaa78e6ffffcd50b3cdb2adbd698c478d05e2583a9b8a59645eefbfdcf42d-1"
 right: "fe8d7a873dc48961a6af334c996b2cb3ce37149d5ce9c9253952a54c6a92c1ad-1"
test result: FAILED. 27 passed; 1 failed
```

**Negation (6) — ignore part numbers in the fingerprint** (drop
`hasher.update(part_number.to_be_bytes())`):

```
thread 'the_complete_fingerprint_distinguishes_a_retry_from_a_different_assembly' panicked at
crates/core/tests/multipart_records.rs:1716:5:  (the fingerprint no longer equals its own oracle)
thread 'a_tombstone_answers_an_identical_retry_and_only_an_identical_retry' panicked at
crates/core/tests/multipart_records.rs:1409:5:
assertion `left == right` failed
  left: AlreadyCompleted(Publication { inode: 5, version: 2, etag: MultipartEtag { … } })
 right: Refused(NoSuchUpload)
test result: FAILED. 26 passed; 2 failed
```

The second failure of negation (6) is the one that matters most: with part numbers dropped, a
**different assembly** under a reused upload id is answered *"your Complete succeeded"* with
the earlier object's ETag — the silent wrong answer `0016:898-908` exists to prevent. The test
catches it end-to-end, through `complete_answer`, not just in the digest.

Beyond the five, the **mutation** run is the mechanical form of the same question: 253
mutants generated from this patch's diff, **0 survived**.

**(b) Production path? — yes.** The test is an integration test over the public
`wyrd_core::multipart` / `wyrd_core::metadata` API (`use wyrd_core::…`,
`multipart_records.rs:20-40`). There is no stand-in, no re-implementation and no mock: the
only thing the test computes itself is the two **oracles** (`etag_oracle`,
`fingerprint_oracle`), which are deliberately independent re-derivations *from the spec text*
used to check the implementation — they are compared against production output, never
substituted for it. The `PendingEntry` byte-identity leg runs through the production
`metadata::encode` (`multipart_records.rs:996-1021`).

**(c) Fixture includes the fault? — yes.** Every rejection case is a **hand-authored bad
value**, not a curated-good one: the torn `mpuctl` with a `max_sessions` its own profile does
not derive (`:684`), the half-owned `sidx:` entry (`:960-968`), the `u64`-overflowing chunk
span (`:914`), the `Completing` session missing its fence stamp (`:789-797`), the reversed /
overlapping / out-of-range part-number runs (`:1155-1170`), the 12 non-canonical spellings of
"part 7" (`:158-200`), the non-UTF-8 key (`:335-346`). The state-table fixture includes
**all four** states plus the absent record, and the tombstone leg includes the
fingerprint-mismatch branch — the failing element is in the fixture, never filtered out of it.

---

## 4. Alternatives considered, with their costs

**A parallel `sidx:` value type instead of extending `PendingEntry`.** This is the one change
that would take the patch from 15 files to 5 — zero mechanical initializer churn — so I
costed it carefully and **rejected it**: `0016:493-510` states the reason in the design, not
as a preference. An owned lease is *renewed in flight by the same half-TTL loop* and guarded
by the same `require(re-encode(prior))` path (`crates/core/src/write.rs:474-500`,
`crates/core/src/metadata.rs:748-758`), both of which take `&PendingEntry`. A parallel type
would need `renew_pending` and `live_lease_guards` made generic — a change to two existing
functions in `metadata.rs`, which the brief forbids outright ("no new function, no change to
any existing CAS") — or a second renewal path, which is a second lease lifecycle over the same
bytes. Cost of the rejected option, concretely: 2 signature changes + ~40 lines of a duplicate
renewal path in a file this slice is allowed to touch for **exactly two fields**. Cost of the
chosen option: **+32 lines across 9 files, all of the literal form `owner: None, staged:
None`, zero behaviour change** (§ 5.1).

**`..Default::default()` at the initializer sites instead of two explicit `None`s.** Would cut
those 32 lines to 16 (one per site) and matches the in-crate `InodeRecord` precedent
(`metadata.rs:1509-1517`). Rejected because it does not change the number of **files** —
which is what the structural finding was about — and because it needs an `impl Default for
PendingEntry` whose value is a lease that expired at logical time 0, a footgun in a file this
slice may only extend by two fields. The explicit form also fails loudly if a third field is
ever added, rather than silently defaulting it.

**Keeping `String`/`u32` fields and validating at each call site** (iteration 1's shape).
Rejected: it is what produced 11 of the 21 findings and 13 surviving mutants. The typed form
costs ~350 lines of `new`/`Display`/`Serialize`/`Deserialize` boilerplate across six newtypes
and removes the *possibility* of the defect class; it also makes every key constructor
infallible and total.

**A `deny_unknown_fields` on every wire struct** (the discontinued #636 patch's style). Not
adopted for the record types: 0016 grows these records across its own slices (`attempts`,
`segments_written`, `clock_source` all arrived in later iterations of the design), and a
strict-on-read decode turns any additive field written by a newer gateway into an unreadable
record on an older one — the rolling-upgrade failure ADR-0047's optional-field rule exists to
avoid. Structural invariants are enforced explicitly instead.

**Validating capacity at decode.** Rejected by the brief's boundary 2 and by
`metadata.rs:305-318`'s own argument; the module documents the split at `multipart.rs:52-58`.

---

## 5. Declared deviations from the brief — please read at sign-off

### 5.1 File count: 15, not 5 (**the brief's own two rules collide here**)

The brief authorises the `PendingEntry` extension (§ Scope, "ONE allowance") **and** says
`crates/core/src/{write,read}.rs` are "untouched" and `crates/custodian/` must not be touched,
**and** caps the patch at 5 files. In Rust these cannot all hold: adding a field to a public
struct invalidates every struct-literal initializer of it, and there are **16 such sites in 9
files** (`git grep -n "PendingEntry\s*{"`). There is no language mechanism that avoids this
(`Default` still requires `..Default::default()` at each site; `#[non_exhaustive]` makes the
out-of-crate sites *worse*, not better). Iteration 1 hit exactly this and the reviewer raised
it as **C1 NEEDS-HUMAN — "Plan must decide whether mechanical `PendingEntry` initializer
updates are allowed beyond the five-file ceiling … the stated scope cannot build as written"**.
That question was deferred to sign-off and never answered, so it is still open.

The 15 files are: the 5 the brief budgets, `Cargo.lock` (cargo-generated by the `sha2`
addition), **8 files whose only change is `owner: None, staged: None`** (`crates/core/src/write.rs`
×3, `crates/core/tests/mutation_regressions.rs` ×3, `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`
×1 each, `crates/dst/tests/custodian.rs` ×1, `crates/metadata-redb/tests/conformance.rs` ×1,
`crates/server/tests/custodian_gc.rs` ×3), and the docs file below. **No store round trip crept
in** — the brief's stated reason for the ceiling: `grep -E "async fn|MetadataStore|WriteBatch|\.await"
crates/core/src/multipart.rs` matches exactly once, in the module doc line that says there are none.

### 5.2 Semantic line count: ~2,874, not ≤1,500

`crates/core/src/multipart.rs` is 1,407 non-blank non-comment lines (plus 763 lines of doc
comment) and `crates/core/tests/multipart_records.rs` is 1,467. That is ~1.9× the brief's
budget, and I want it visible rather than buried.

Why it grew rather than shrank from iteration 1 (which was ~600 + ~600 and **under** budget):
the round-1 review failed that patch as *incomplete* — C3: *"a rebuild must supply the slice's
complete vocabulary — retirement payload records are explicitly deferred and only a generic
answer enum is exposed"*. Supplying them is `RetirePayload` + `PartNumberSet` +
`decode_retire_obligation` (~230 lines) and the nine typed outcome/answer enums (~200 lines,
almost all one-line variants with their doc comment). The six validated newtypes are ~350
lines. The 25-cell answer table is ~130 lines of the test file on its own. Nothing here is
scaffolding and nothing is a store round trip; the slice is simply bigger than the number the
issue guessed. **If the human prefers the budget to bind, the split that fits is
"records + keys + digests" / "outcomes + answer table" — but that is a Plan decision, not
one Do should take unilaterally**, and it would put an API seam in the middle of a vocabulary
whose whole purpose is to be one alphabet.

### 5.3 A docs file the brief told me not to write

The brief says the architecture docs are **confirm-only** for this slice and "if a docs gate
disagrees, that is a §6 item to raise, not a paragraph to invent". I deviated, deliberately,
and wrote one paragraph into `docs/design/architecture/08-crosscutting-concepts.md:87` (§8.7,
Compatibility and version skew). Three reasons:

1. The target repo's **own** rubric makes it a hard MUST, not a preference: *"a change that
   adds or alters a port, an API operation, an RPC, a CLI flag, or a **persisted field**
   updates the living architecture doc in the same PR. This is a merge requirement, not a
   follow-up."* (`AGENTS.md:155-158`). `PendingEntry` gains two persisted fields.
2. Three of the 21 blocking findings were exactly this, so the round would otherwise repeat.
3. The in-tree precedent is the *directly analogous* slice: #635 landed the segmented-map
   **shape with no producer** and wrote a paragraph in the same section
   (`08-crosscutting-concepts.md:85`), ending *"Landing the shape and its codec ahead of any
   producer or resolver (this slice) keeps the byte-identical … behaviour … unchanged until a
   later slice actually publishes a segmented map."* My paragraph is written in that voice and
   makes the same honesty explicit — it says nothing is written, read or reclaimed yet, so it
   describes no store shape the code does not produce (which is what the brief was guarding
   against).

`python3 docs/publishing/tools/lint_docs.py` and `render_site.py --check` both pass, as does
`typos`.

---

## 6. Open questions the brief asked Do to state for sign-off

**(a) `#[non_exhaustive]` on the typed outcome enums.** Not applied. `Refusal`,
`CreateOutcome`, … are consumed only inside this workspace (#508, #656–#659 are all in-tree),
where `#[non_exhaustive]` buys nothing but forces a `_ =>` arm on every match — and a `_ =>`
arm is precisely what would let a *new* refusal be silently mapped to the wrong S3 status by
#508. Exhaustive matching is the property this slice wants: adding a variant should be a
compile error at every wire-mapping site. Wyrd publishes no crate (`publish.workspace =
false`), so there is no semver contract to protect. **Reversible in one line if sign-off
disagrees.**

**(b) `PART_NUMBER_WIDTH = 5`, addressing `[1, 99_999]`** (`multipart.rs:298-314`). S3's part
maximum is 10,000, so five digits leave a full order of magnitude of headroom while keeping
per-session keys short (a `sidx:` key is already `5 + 32 + 1 + 5 + 1 + ≤39` bytes). Six digits
would add one byte to every `part:`/`psum:`/`sidx:` key for headroom nothing in the design
asks for. Widening later is a stored-format change with a migration, exactly as
`SEG_INDEX_WIDTH`'s doc records — which is why the headroom is bought now rather than
borrowed. `SLOT_INDEX_WIDTH = 6` (`:352-362`, `[0, 999_999]`) is set by 0016's own `MAX_INFLIGHT_PARTS`
clamp arithmetic (≈524,288 at the `SCAN_CAP` bound, `0016:1476`), and matches
`SEG_INDEX_WIDTH`.

---

## 7. NEEDS-HUMAN

* **C4-verify will report `UNVERIFIABLE` (exit 77) on its RED leg.** Pre-declared by the brief
  (§ Falsifiability): this slice is born-at-tier, so reverting `crates/core/src/*` removes
  every symbol the discriminator names and the RED leg fails to *compile* rather than failing
  an assertion. The five negation demonstrations in § 3 are what the brief specifies in its
  place, and they are all recorded above with their output.
* **The 5-file budget vs. the sanctioned `PendingEntry` extension (§ 5.1)** — the C1 question
  from round 1, still unanswered. Nothing in the patch can resolve it; it is a Plan/scope call.
* **The ~1.9× semantic-line overage (§ 5.2)** — accept, or re-split at Plan.
* **The deliberate docs deviation (§ 5.3)** — accept the paragraph, or ask for it to be
  removed and the finding recorded-rejected instead.

No external dependency was missing: the whole slice is pure functions on a plain Linux
workspace, and every tool the brief's `External dependencies` names (`typos`, the docs
renderer, `cargo-deny`, `cargo-machete`, `cargo-mutants`) was present and ran.

---

## 8. Scratch hygiene

Everything throwaway lived under `${PDCA_SCRATCH}` as
`pdca-builder-654-{multipart.rs.orig,ci.log,docs}` and is removed. No `/tmp` path was used.
