# Result — issue 692 / multipart-record-family

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the key grammar exists (#691, merged) but no record **values** do.
  This slice lands the record family and its **validating decoders**: `Budget` (the profile
  tuple and its pure derivations), `AdmissionRecord` (`mpuctl`), `SessionRecord` with
  `SessionState`/`PublishTarget`/`Completion` (`mpu:`), `SlotRecord` (`slot:`),
  `PartRecord`/`PartSummary` (`part:`/`psum:`), `OwnedEntry`/`StagedPlacement` (`sidx:`),
  `PartNumberSet` and `RetirePayload` (`retire:*`), plus `encode_record`/`decode_record` —
  and the `PendingEntry` extension in `metadata.rs` (the two optional fields `owner` and
  `staged`) with its mechanical cross-crate ripple. After this slice, an internally
  inconsistent stored record is a decode error, not a value.
- Success criterion: every record type round-trips `encode`/`decode`, and a
  structurally invalid value is rejected at decode with a typed error (ADR-0045). Nine
  legs are **BINDING**, each proven with a hand-authored torn value. Legs 1a–1e are
  carried-forward MUST-FIXes from the v2 sign-off and its batch review; legs **1f–1i are
  NEW at this re-plan**, settling the four v2 blockers that `iterate-plan` was answered
  over:
  1. **Relational identity checks** — two stored spellings of one quantity may not
     disagree:
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
     (v2 review, `multipart.rs:1789/1800`; the archived test at `multipart_records.rs:1108`
     *affirmed* this case — it must now reject);
     (e) a `PendingEntry` with exactly one of `owner`/`staged` is **torn** and rejected at
     decode, under both a `pending:` and a `sidx:` reading (v2 review,
     `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
     valid shapes.
  1'. **Nested and range validation — the four v2 blockers, settled here:**
     (f) **`Budget::new` enforces BOTH ends of every knob range in `0016:1463-1480`**, not
     just the lower: `max_part_chunks` must satisfy the value-ceiling rule **and** the
     `B_ops` clamp (batch-review `multipart.rs:1041`), and `max_staged_chunks` must lie in
     `[max_part_chunks, MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS]` — the publishable ceiling
     (batch-review `multipart.rs:1027`). A profile above either end is rejected;
     (g) the **`sidx:` scan bound counts committed staging entries too**, not only
     in-flight chunks, so an accepted profile can never exceed `SCAN_CAP/2` and leave
     teardown's single scan silently incomplete (batch-review `multipart.rs:1074`);
     (h) **the retire session-token arm honours the token's optional `:<part>:<attempt>`
     suffix** (`0016:437-453`): a whole-session `Session`/`Parts` obligation under a
     per-part token, and a per-part `Chunks` obligation under a session-wide token, are
     **both** rejected — decoding either makes the drain act on the wrong scope
     (batch-review `multipart.rs:2024`);
     (i) **nested types validate at decode, not just the outer record** (reviewer C5 +
     T2 FAIL): `StagedPlacement`'s `EcScheme` is rejected unless
     `erasure::supported(k, m)` (`erasure.rs:120` — the #285 precedent: an untrusted
     `ReedSolomon { k: 0, m: 1 }` read back from stored metadata must be a typed error);
     `PartRecord` validates each `ChunkRef` structurally rather than accepting a raw one
     (`multipart.rs:1538`); and a `SlotRecord` with `lease_expiry_millis <=
     reserved_at_millis` — a slot born already lapsed — is rejected
     (`multipart.rs:1657`).
  2. **Decode→encode is the identity on a legacy value:** a legacy `pending:` value with
     neither new field re-encodes **byte-identically** — the `skip_serializing_if`
     identity every `require(key, encode(prior))` CAS in `metadata.rs` depends on
     (`metadata.rs:1368-1391`, ADR-0047:38-50; `0016:512-524`).
  3. **The identity/occupancy boundary holds** (a deliberate Plan ruling, unchanged from
     v2): a decoded `AdmissionRecord` whose `count` exceeds its `max_sessions` still
     **decodes** — occupancy above a lowered cap is a legitimate live state that admission
     (a later slice) refuses to grow, not a decode error; rejecting it would make a durable
     record unreadable the day the profile is lowered (`0016:499-511` states this for the
     whole record set: decode validates against **format** maxima, never live knobs; the
     `MAX_ROOT_SEGMENTS` boundary, `metadata.rs:312-321`). Assert it decodes. Identity
     relations (legs 1a–1e) are binding; occupancy relations are not. This settles the v2
     reviewer's "count above the derived admission cap" finding the other way.
     **Corollary, also binding:** a `sidx:` value whose `staged` placement length does not
     match its scheme's fragment count **decodes** — placement length is the standing
     *contextual*-check example (`AGENTS.md:146-149`, ADR-0045, `0016:513-527`), quarantined
     by GC rather than rejected. Leg 1i validates the scheme's **geometry**, never the
     placement's **length**; do not conflate them.
  4. **Docs currency — SETTLED YES at this re-plan** (this is the standing C1 NEEDS-HUMAN,
     closed here rather than re-asked every round): `PendingEntry` gains two **persisted**
     fields, and `AGENTS.md:154-158` makes updating the living architecture doc in the same
     PR *"a merge requirement, not a follow-up"*. The slice that touches `PendingEntry`
     MUST add a short paragraph to `docs/design/architecture/05-building-block-view.md`
     § "The metadata model", mirroring the ADR-0047 optional-inode-fields paragraph already
     there at `:186-192` (same `serde(default)` + `skip_serializing_if` + byte-identical
     re-encode reasoning). This is the ONLY `docs/` edit any child may make; ADRs,
     proposals and specs stay untouched (INTEGRATION §2 immutability).
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2; base verified `9dbcd72`)
- Scope: extend `crates/core/src/multipart.rs` with the record family and validating
  decoders listed under Goal, including `encode_record`/`decode_record` and all nine
  binding checks; **key-taking decode APIs** for the two records whose identity lives partly
  in the key (`decode_owned_entry(key, bytes)`, `decode_retire_obligation(key, bytes)`) — a
  decode that cannot see the key cannot validate against it, which is exactly how v2's shape
  failed its review. In `crates/core/src/metadata.rs`, **ONE allowance**: `PendingEntry`
  (`metadata.rs:1528`) gains `owner: Option<multipart::UploadId>` and
  `staged: Option<multipart::StagedPlacement>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`, doc comments per the v2 hunk), drops `Copy` (forced — `UploadId` is
  a `String` newtype), and gains the torn-shape rejection (leg 1e) in a manual
  `Deserialize`; nothing else in that file changes. **The mechanical ripple is explicitly
  ALLOWED** (this Plan's answer to the v2 C1 finding that a 5-file ceiling was unbuildable):
  the 8 files that construct or copy `PendingEntry` — `crates/core/src/write.rs`,
  `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs` — may each gain the mechanical `owner: None,
  staged: None` initializer lines / clone-instead-of-copy fixes, **≤ 8 changed lines per
  file, no logic change, no new function**. A ninth ripple file or a non-mechanical hunk in
  any of them means the seam is wrong: STOP and hand back. Plus the ONE docs paragraph of
  criterion leg 4.
  / out of scope: the outcome enums, answer table, `Verb`, `MultipartEtag`, digests and
  `sha2` (#693's — no `Cargo.toml`/`Cargo.lock` change here); the knob **values** (#655);
  every store round trip (#656–#659); reaper/windows (#625); custodian **source** code —
  `crates/custodian/src/` untouched (only its *tests'* initializers); every `docs/design/`
  file except the one `05-building-block-view.md` paragraph named in criterion leg 4 —
  ADRs, proposals and specs are untouched.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.

### Advisory — adversary

# Advisory — adversarial (refutation pass)

**No patch exists.** `close-disposition` is `split`, every C/T row in `check-gates.json` is
`none`, and C2/C4 read *"N/A — close disposition (no patch to verify)"*. There is therefore no
red→green proof to re-run and no fix to break. The artefact actually under judgment is
**`split-proposal.md`** — the three child briefs that will be materialised at acceptance and
then built. I attacked those: the seams, the leg carve, the budget arithmetic, and every
`path:line` they order Do to open, all grounded on the target base `9dbcd72`
(`crates/core/src/multipart.rs` verified at 854 lines, as the brief claims).

Toolchain was **not** a limiter: no compile or test run was required to reach any finding below.

---

- **NEEDS-HUMAN [human] — `split-proposal.md:83` (repeated `:92-93`): child-1's `MAX_SESSIONS`
  derivation drops the `SCAN_CAP/2` clamp, and leg 1a makes the wrong formula *binding*.**
  Child-1 states `MAX_SESSIONS = ⌊W_ref/U_ref⌋` and cites `0016:1469-1470`. That source line says
  the opposite: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1470` reads
  `MAX_SESSIONS = min( ⌊W_ref / U_ref⌋ , SCAN_CAP/2 )` — "**DERIVED**… and the `SCAN_CAP/2` term is
  a **clamp the implementation applies**, not a range check left to the operator". Leg 1a then
  rejects any `AdmissionRecord` whose stored `max_sessions` "disagrees with what its own stored
  `profile` tuple derives", so the omission fails in **both** directions. Failing case A (false
  reject): the deployment 0016:1470 names by hand — large `W_ref`, small parts — has
  `⌊W_ref/U_ref⌋ > SCAN_CAP/2`; a conforming writer stores the clamped
  `max_sessions = SCAN_CAP/2 = 524_288`, child-1's unclamped derivation computes something larger,
  and a **valid durable record is rejected at decode** — the exact "a durable record unreadable the
  day the profile is lowered" hazard leg 3 was written to prevent. Failing case B (false accept): a
  record storing the unclamped `⌊W_ref/U_ref⌋` passes leg 1a, and 0016:1470 states precisely what
  that breaks — it "can… break the reaper's `scan("mpu:")`; the clamp is what makes the two bounds
  compose". Child-1's leg 1g does not rescue this: its `SCAN_CAP/2` is the *per-session `sidx:`
  range* clamp of `0016:1471`/`:353`, a different bound on a different scan.

- **NEEDS-HUMAN [impl] — `split-proposal.md:190-192`: child-2's `ChunkRef` leg has no
  placement-length carve-out, so the split re-opens the conflation the parent explicitly forbade.**
  The parent brief warns at `brief.md:94-96` that placement length is the standing *contextual*
  check (`AGENTS.md:146-149`, verified verbatim on the target) and "leg 1i validates the scheme's
  **geometry**, never the placement's **length**; do not conflate them." The split keeps that
  warning **only** in child-3, attached to `StagedPlacement` (`split-proposal.md:304-308`), and
  gives child-2 the bare instruction "`PartRecord` validates each `ChunkRef` structurally rather
  than accepting a raw one". Failing case: `crates/core/src/metadata.rs:135-136` documents
  `ChunkRef.placement` as "**Empty on a pre-M3 record**; the read path then resolves by fragment
  index", `#[serde(default)]`. A child-2 decoder that reads "validate structurally" as
  "`placement.len() == k + m`" rejects every pre-M3 `ChunkRef` — every flat-map object written
  before M3 becomes undecodable inside a `PartRecord`, and the same over-strict helper is the one
  child-3 is told to reuse. The negation demonstration child-2 is asked for (`:201-203`) cannot
  catch this: it only proves the check *fires*, never that it fires on the right field.

- **NEEDS-HUMAN [human] — `split-proposal.md:58-60` vs `:129-132` and `:269-271`: the
  `encode_record`/`decode_record` signature is settled in child-1 but determined only by child-3, so
  the "each child adds match arms" claim is unwarranted.** Child-1 lands "the
  `encode_record`/`decode_record` envelope (arms for this child's records only)"; nothing in its
  brief says the envelope takes a key, and none of its records need one — `mpuctl` identity is
  wholly in the payload. Child-3 must then add arms for the two decoders whose whole point is
  key-taking: `decode_owned_entry(key, bytes)` / `decode_retire_obligation(key, bytes)`, "a decode
  that cannot see the key cannot validate against it, which is exactly how v2's shape failed its
  review". Failing case: child-1 merges `pub fn decode_record(bytes: &[u8]) -> Result<Record,
  RecordError>` — a faithful reading of its own brief — and child-3 has only two exits, both bad:
  change a merged public signature (contradicting `split-proposal.md:61-64`, "each child's patch
  applies on a base that already contains the scaffolding it imports"), or leave `sidx:`/`retire:`
  out of the envelope so the dispatch path silently *cannot* validate the two records with
  key-borne identity — reinstating the v2 defect one layer down. Child-1's brief must pin the
  envelope's signature, or the seam is wrong.

- **NEEDS-HUMAN [impl] — `split-proposal.md:73-75`: the budget carve does not sum inside the
  envelope it claims to sum inside.** The line reads "must sum within the parent's ≤ 2,150 lines /
  **12 files**: child-1 ≈ 550 lines / 2 files, child-2 ≈ 700 lines / 2 files, child-3 ≈ 900 lines /
  12 files." `2 + 2 + 12 = 16`. Deduplicated it is still 14 distinct files: `multipart.rs`
  (all three), **three** new test files, `metadata.rs`, the one docs paragraph, and the 8 ripple
  files. The parent's 12 (`brief.md:177-181`) was computed against **one** test file; the parent's
  own Test-file rule (`brief.md:185-189`) mandates "one **NEW** `crates/core/tests/<name>.rs` per
  child… each child takes a distinct name", which makes 14 unavoidable. Either the parent envelope
  must be raised to 14 as an explicit Plan ruling or the carve is out of budget on acceptance; the
  proposal instead asserts the arithmetic works.

- **NEEDS-HUMAN [human] — `split-proposal.md:415-418`: child-3's external conflicts with #710/#711
  survive only as prose, and the harness will not carry them.** The proposal concedes the ordering
  fields "may only name sibling labels", so `Conflicts with: 710, 711` "is added to THIS child's
  materialised `brief.md` at split acceptance". That is enforced nowhere:
  `src/pdca_harness/split.py:250-258` hard-errors on any ordering ref that is not a sibling label,
  and `:359` writes each child's brief as `rewrite_ordering(child.body, mapping)` — the child's own
  body only, with nothing inherited from the parent. Failing case: acceptance runs, nobody
  hand-edits child-3's brief, and `compute_waves` reads a child-3 with **no** `Conflicts with`; it
  is then free to share a wave with #710 or #711, both of which the parent declares as sharing
  `crates/core/src/metadata.rs` (#711 also `crates/dst/tests/custodian.rs`, which child-3 ripples).
  The same silence covers the #693/#655 repoint, also delegated to the same unenforced moment.

- **NEEDS-HUMAN [impl] — `split-proposal.md:268`, `:302-303`, `:305-306` (inherited from
  `brief.md:12-13`): three of the 0016 ranges in child-3's "sources Do MUST open" point at the
  wrong text.** Verified line-by-line on the target: (i) `0016:475-491` is labelled "the `sidx:`
  disjoint-staging rule" but is in fact the **`skip_serializing_if` identity argument** — the real
  disjoint-staging rule is `0016:290` and `0016:353`; (ii) `0016:437-453` is labelled "the
  retirement-token grammar leg 1h enforces" but is the **`PendingEntry` two-optional-fields block**
  — the token grammar with the optional `[:<part-number>:<attempt-id>]` suffix that leg 1h turns on
  is `0016:358-366`, the grammar itself at `:360`; (iii) `0016:512-527` is labelled "the
  `skip_serializing_if` identity argument and the placement-length contextual boundary" but is the
  **segment-group-nonce** section (`seggrp:` marker lifetime). `0016:499-511`, cited at
  `split-proposal.md:104-105` for leg 3's "decode validates against format maxima, never live
  knobs", is the same nonce section. Ranges (i) and (iii) are simply swapped. A builder who opens
  what it is told to open reads the wrong normative text for the two legs (1h, leg 2) the proposal
  calls highest-risk.

- **NEEDS-HUMAN [impl] — all three children's "Invariant to restore" (`split-proposal.md:117`,
  `:204`, `:326`): `docs/principles.md` does not exist at `9dbcd72`.** `git ls-files | grep -i
  principle` returns nothing and the path is absent from the checkout, yet each child anchors C-1 on
  `docs/principles.md:109` with a §6 row at `:137`. The repo's own convention for the same
  reference is sectional, not line-numbered — `crates/core/src/multipart.rs:49`,
  `crates/core/src/metadata.rs:2058`, `crates/custodian/src/rebalance.rs:51` all write
  "`docs/principles.md` §5 C-1". The line numbers are unverifiable at the base and cannot be
  checked by any reviewer of the children.

- **NEEDS-HUMAN [impl] — `split-proposal.md:343-344`: "nothing else in that file changes" is false
  for `metadata.rs`.** `crates/core/src/metadata.rs:3374` constructs `PendingEntry {
  lease_expiry_millis }` inside the file's own `#[cfg(test)] mod tests`. Adding two non-`Default`
  fields makes that struct literal fail to compile, so child-3 needs a second, mechanical hunk in
  `metadata.rs` beyond the ONE allowance its scope grants. Under a literal reading of the child's
  own STOP rule this is a scope breach the builder must hand back on; the child brief should name
  the in-file test construction explicitly. (I verified the reverse claim holds: the **8** ripple
  files named at `:346-350` are exactly the set that constructs `PendingEntry` outside
  `metadata.rs` — see the "could not refute" note below.)

- **NEEDS-HUMAN [human] — `split-proposal.md:73-75`, `:139-141`, `:225-226`, `:361-364`: the carve
  is back-solved to the ceiling, with zero slack for the material the re-plan added.** Every figure
  lands exactly on its bound: `550 + 700 + 900 = 2,150`, and each child's own itemisation sums
  exactly to its own cap (`300+250=550`; `400+300=700`; `400+45+400+40+15=900`). The 2,150 came
  from v2's measured 2,140 (`brief.md:177-181`), but this re-plan **adds** four new binding legs
  (1f–1i, `brief.md:36-38`), reshapes two decoders to take keys, and splits one test file into
  three — three preambles, three fixture sets, three `use` blocks. The proposal offers no evidence
  the additions fit in the 10 remaining lines; the risk it is guarding against (a child hitting the
  100 KB backstop, which is what refused v2) is precisely the one an exact-fit estimate hides.

---

## Attempted and could not refute

- **Leg coverage is complete.** Every binding leg is allocated exactly once: 1a/1f/1g/leg-3 →
  child-1; 1c/1i-`ChunkRef`/1i-slot → child-2; 1b/1d/1e/1h/1i-`EcScheme`/leg-2/leg-3-corollary/leg-4
  → child-3. Nothing from `brief.md:39-105` is dropped or duplicated.
- **The 8-file ripple list is exactly right.** `PendingEntry {` struct-literal construction outside
  `metadata.rs` occurs in precisely the eight files named (`core/src/write.rs`,
  `core/tests/mutation_regressions.rs`, `custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `dst/tests/custodian.rs`, `metadata-redb/tests/conformance.rs`, `server/tests/custodian_gc.rs`);
  no site exceeds the ≤8-line allowance. The four *other* files that mention `PendingEntry`
  (`core/tests/mutation_regressions_round2.rs:118`, `core/tests/stream_lease_renewal.rs:79`,
  `server/tests/gateway_lease_expiry.rs:153`, `custodian/src/gc.rs:489`) only `metadata::decode`
  into an owned value and read `lease_expiry_millis`, so neither the added fields nor the dropped
  `Copy` reaches them — the "`crates/custodian/src/` untouched" claim survives.
- **The `--classify` evidence transfers.** I expected the v2 dry-run (an 11-file set) not to
  generalise to the 2-file children, but `engine/scripts/run-verify.sh:252` emits `ADDED_TEST` per
  added test file and `CRATE` per touched crate with no count sensitivity, so the discriminator
  holds identically at 2 files.
- **The conflict assignment to child-3 alone is sound.** Children 1 and 2 touch only
  `multipart.rs` plus their own new test; #710/#711 read neither.
- **Three-way, not two- or six-way.** I could not construct a cheaper cut: the record types plus
  decoders really do exceed a single refused-size child, and every finer seam separates a record
  from the decoder that validates it.
- **"Draft" 0016 is not a defect.** I checked whether calling a `docs/design/proposals/draft/`
  proposal "settled and normative" was an overreach; the base already cites 0016 normatively from
  production source (`crates/core/src/metadata.rs:538`, `crates/core/src/multipart.rs:319`), so the
  convention is established.
- **Anchors that do verify exactly**, and which I therefore stopped attacking:
  `crates/core/src/erasure.rs:120` (`supported`), `crates/core/src/metadata.rs:129-140` (`ChunkRef`),
  `:1368-1391` (the `skip_serializing_if` precedent), `:1528` (`PendingEntry`), `AGENTS.md:146-149`
  and `:154-158`, `docs/design/architecture/05-building-block-view.md:186-192`, `0016:528` (the
  state machine).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Split proposal has 8 substantive adversary findings, more than a normal rework pass can absorb cleanly — the volume itself is a signal that the three-way cut needs a more careful breakdown, not just line-edits to the existing proposal: - leg 1a formula drops the SCAN_CAP/2 clamp from 0016:1470 (false reject + false accept) - child-2's ChunkRef check lacks the pre-M3 placement-length carve-out - encode_record/decode_record signature is used by child-3 but not pinned by child-1 - file-count budget (12 claimed) doesn't match actual (14) - #710/#711 conflict declarations rely on an unenforced manual edit step - three 0016 citations point at the wrong source text - docs/principles.md is cited but does not exist in the repo - metadata.rs "nothing else changes" claim is already false (test literal won't compile) - zero slack in the line/file estimate for the material this re-plan itself added Re-plan should re-derive the split from scratch with room to verify each seam against the base rather than patching the existing proposal's citations/arithmetic.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
