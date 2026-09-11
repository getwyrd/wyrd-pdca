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
  `{generation: {inode, version, chunks?, segments?}}` (`:355`, `:2417`); under
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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 90.6% — 232 of 256 instrumentable changed lines executed (floor 80%); 256 of 850 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 78 mutants tested in 2m: 49 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.90s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #771’s multipart retirement-obligation value, key-aware decoder, canonical range encoding, tests, and architecture documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the writer-derived accepted shapes, typed rejection rules, serialization identity, docs currency, scope, and born-at-tier evidence posture sufficiently to judge the change. |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no pre-fix value decoder or production consumer, so there is no existing behavioral symptom to reproduce (`crates/core/src/multipart.rs:70`). |
| C3 Change | FAIL | The change accepts `parts:"all"` wherever the mode-neutral `PARTS_COMPONENT` is allowed, including `retire:records:`, although only an explicit published-part set is safe there (`crates/core/src/multipart.rs:2664`, `crates/core/src/multipart.rs:2898`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared born-at-tier evidence posture — the independent and frozen green runs pass 27 tests, but the stashed pre-fix leg does not compile or execute a discriminator, so no behavioral red was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:180`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the scope relation and add isolating rejection coverage for record-mode or sessionless `parts:"all"` — the current causal suite omits the unsafe spelling identified by the writer table (`crates/core/src/multipart.rs:2736`, `crates/core/tests/multipart_retire_obligation.rs:334`). |
| T1 Structure | NEEDS-HUMAN | Decide whether to re-enter Plan for the size cap — the three-file patch has about 1,060 added nonblank/non-comment lines against the brief’s ≤1,000-semantic-line budget, which affects reviewability of this frozen format. |
| T2 Shape | FAIL | The persisted shape is over-permissive: the proposal distinguishes explicit published and unnamed part sets, while the decoder admits the all-parts wildcard in record mode and without session teardown (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:382`, `crates/core/src/multipart.rs:2669`). |
| T3 Runtime | N/A | The slice deliberately has no writer, store call, async work, or production consumer, so runtime behavior is outside this change (`crates/core/src/multipart.rs:5`, `crates/core/src/multipart.rs:70`). |
| T4 Contribution | FAIL | The deep-review gate is red with four blocking reports that ground to the wildcard-scope defect and the public value-only decode bypass; the publish-artifact audit is separately deferred by design (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Decide the safe public API boundary — `RetirePayload: Deserialize` exposes value-only decoding even though the record’s correctness depends on key relations, so future consumers can bypass the documented key-taking boundary (`crates/core/src/multipart.rs:2762`, `crates/core/src/multipart.rs:2994`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this data-loss-relevant persisted format is fit to freeze for later writers after the wildcard and decoder-boundary blockers are resolved (`docs/design/architecture/05-building-block-view.md:204`). |

### Advisory — adversary

# Adversarial review — issue #771 (multipart retirement obligation)

Re-ran the evidence against `$PDCA_TARGET` (patch applied as working-tree changes on
`debf521 pre-fix base`). The test exercises the **production** symbols
(`decode_retire_obligation`, `metadata::decode::<RetirePayload>`) — no parallel
re-implementation, no mock — and I could reproduce its GREEN leg. The `C4-verify`
UNVERIFIABLE is a compile-absence RED, pre-declared at `brief.md:85-101`, so I do not score it
as a refutation. All findings below were **executed** against the patched crate from a scratch
probe binary linked to `crates/core`, not reasoned about on paper.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2667` (`PARTS_COMPONENT { mode: None }`,
  with `PartScope::All` carrying no constraint of its own at `:2464-2485`): the `all` wildcard
  decodes under three key/shape combinations no writer row installs, and the T4 review caught
  only one of them.** Verified accepts:
  `decode_retire_obligation(retire_key(Records, <suffix-free s: token>), br#"{"session":true,"parts":"all"}"#)`
  → `Ok`, and likewise `{"parts":"all"}` and `{"parts":"all","seg":{…}}` under `retire:records:`,
  and `{"parts":"all"}` **alone** under `retire:bytes:`. `0016:2186-2187` is the only row that
  ever spells `all`, and it is `retire:bytes:` **with** `session`
  (`CAS Open@E -> Aborting@E+1 + put retire:bytes:{session, all}`); the patch's own writer table
  at `multipart.rs:2736-2744` has no other. A records-mode `{session, all}` instructs a drain to
  delete every `part:`/`psum:` record of the session **and** its own records with no orphan-marking
  step at all — the outcome-(a) loss the brief's invariant section names. The fix must bind all
  three combinations (`All` ⇒ mode `Bytes` **and** `session` present), not just the one T4 quoted,
  or the next round re-opens on the other two. No test would go red today:
  `crates/core/tests/multipart_retire_obligation.rs:337` covers records-mode `{parts:<set>}` only.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2658` (`SESSION_COMPONENT { mode: None }`):
  `{"session":true}` decodes under a `retire:records:` key (verified `Ok`, re-encode identical),
  and the justification given for the mode-neutrality is contradicted by the patch's own table.**
  The constant's doc claims `session` is "installed … by `retire:records:` by the publication that
  supersedes a session's staging records (`0016:356`)", but the publication's writer row
  (`0016:662`, root flip) installs `1 put retire:records:{parts}` "naming only the PUBLISHED parts"
  — no `session` — and the payload table this same patch writes at `multipart.rs:2742-2744` lists
  no `retire:records:` + `session` row either. Per `RetirePayload::session`'s own doc
  (`multipart.rs:2786-2788`, citing `0016:673`) that component names the `mpu:` record and the
  surviving `slot:` records, so a records-mode `{session}` obligation is precisely "tear the
  session's naming records down without ever marking the bytes they protect". Two documents inside
  one diff disagree, the code implements the looser one, and no test pins either behaviour.

- **NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:2762-2764`: the public `Deserialize` derive
  makes a `RetirePayload` reachable that carries *none* of the four key relations, and the type
  cannot tell the two provenances apart.** Verified through `metadata::decode::<RetirePayload>`:
  `{"chunks":[…]}` and `{"session":true,"generation":{…}}` both decode successfully even though
  `decode_retire_obligation` rejects them as `RetireModeMismatch` / `RetireTokenScopeMismatch`.
  The base precedent (`SessionRecord` `:1873`, `PartRecord` `:2235`) also derives it, but for those
  S1 ≡ S2 apart from the canonical-bytes gate; this is the first record class where the two seams
  differ in *rules*, so #656–#659's drain can legitimately hold a payload nothing validated against
  its key. T4 raised this as a convention break — but `brief.md:158-160` **mandates** the S1 seam
  (the `decode_both` helper asserts S1/S2 agreement), so "remove the derive" contradicts the brief.
  This is a scope/architecture call for the human: keep S1 and accept the hazard, seal the derive
  and rewrite the mandated helper, or introduce a distinct key-checked value type.

- **Attempted and could not refute:** R6's epoch exactness (`E±1` rejected, `E` accepted, and the
  `g:`-token limit correctly left unbound); R4 in both directions and R5 in all three; R7 over both
  chunk lists; R8's canonicality (adjacent, overlapping, out-of-order, reversed, `0`,
  `MAX_PART_NUMBER + 1`, and the `MAX_PART_NUMBER` boundary itself), plus `[[1,4294967296]]` which
  falls out as a typed `MalformedRecordValue`; R9 identity against trailing whitespace,
  `"session":false`, `"chunks":[]`, field reordering, a `\u` escape and duplicate JSON keys — every
  one refused; the owes-nothing family; and `retire:records:g:<inode>:<version>`, under which I
  could construct **no** decodable payload (correct: no writer installs one). `PartNumberSet`'s
  `+ 1` arithmetic is bounded by `PartNumber::new` on both paths, so neither `from_runs` nor
  `from_numbers` can overflow. `C5-mutants` reports 0 surviving mutants on the diff
  (`gate-logs/C5-mutants.log:13`), which corroborates the load-bearing claim for the checks I could
  not break by hand; the nine isolating negations themselves live in `build-notes.md`, which is
  withheld from this leaf, so that half of the claim is unverified here rather than refuted.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared born-at-tier evidence posture — the independent and frozen green runs pass 27 tests, but the stashed pre-fix leg does not compile or execute a discriminator, so no behavioral red was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:180`).
- [ ] C5 Causal adequacy — Rebuild the scope relation and add isolating rejection coverage for record-mode or sessionless `parts:"all"` — the current causal suite omits the unsafe spelling identified by the writer table (`crates/core/src/multipart.rs:2736`, `crates/core/tests/multipart_retire_obligation.rs:334`).
- [ ] T1 Structure — Decide whether to re-enter Plan for the size cap — the three-file patch has about 1,060 added nonblank/non-comment lines against the brief’s ≤1,000-semantic-line budget, which affects reviewability of this frozen format.
- [ ] T5 Judgment — Decide the safe public API boundary — `RetirePayload: Deserialize` exposes value-only decoding even though the record’s correctness depends on key relations, so future consumers can bypass the documented key-taking boundary (`crates/core/src/multipart.rs:2762`, `crates/core/src/multipart.rs:2994`).
- [ ] Validation — fitness-to-purpose — Decide whether this data-loss-relevant persisted format is fit to freeze for later writers after the wildcard and decoder-boundary blockers are resolved (`docs/design/architecture/05-building-block-view.md:204`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the declared born-at-tier evidence posture — the independent and frozen green runs pass 27 tests, but the stashed pre-fix leg does not compile or execute a discriminator, so no behavioral red was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:180`).; C5 Causal adequacy — Rebuild the scope relation and add isolating rejection coverage for record-mode or sessionless `parts:"all"` — the current causal suite omits the unsafe spelling identified by the writer table (`crates/core/src/multipart.rs:2736`, `crates/core/tests/multipart_retire_obligation.rs:334`).; T1 Structure — Decide whether to re-enter Plan for the size cap — the three-file patch has about 1,060 added nonblank/non-comment lines against the brief’s ≤1,000-semantic-line budget, which affects reviewability of this frozen format.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
