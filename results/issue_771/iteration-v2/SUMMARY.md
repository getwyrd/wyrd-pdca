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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 87.0% — 214 of 246 instrumentable changed lines executed (floor 80%); 246 of 923 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 72 mutants tested in 2m: 44 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.17s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #771: add the key-validated multipart retirement-obligation value format, canonical part ranges, its decoder tests, and living-architecture documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the accepted writer shapes, rejection boundaries, serialization identity, and documentation obligation precisely enough to judge the patch (`brief.md:14`). |
| C2 Reproduction (red pre-fix) | N/A | This is born-at-tier functionality: the stashed base lacks the new API, so the discriminator fails to compile before any test runs rather than producing a behavioral red (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:108`). |
| C3 Change | PASS | The scoped change supplies a key-taking decoder that validates the payload against its mode and token before canonical-byte acceptance, and records the persisted format in the living architecture (`crates/core/src/multipart.rs:3084`, `docs/design/architecture/05-building-block-view.md:204`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared born-at-tier evidence posture — the independent and frozen post-fix runs pass 26 tests and full CI, but the pre-fix leg never executes a discriminator, so no behavioral red→green was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:108`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add an accepted round-trip witness for a generation carrying both `chunks` and `segments` — the format explicitly permits the hybrid, while the suite tests only the two separate forms, so a regression rejecting the combined form can survive (`crates/core/src/multipart.rs:2581`, `crates/core/tests/multipart_retire_obligation.rs:309`, `crates/core/tests/multipart_retire_obligation.rs:325`). |
| T1 Structure | PASS | The patch touches exactly the three scoped files and contains 966 added nonblank, non-comment lines, within the brief's 1,000-semantic-line ceiling (`brief.md:128`). |
| T2 Shape | PASS | The component-set wire shape can express both required combined session obligations, while centralized mode, scope, identity, epoch, and wildcard checks reject cross-key shapes (`crates/core/src/multipart.rs:2736`, `crates/core/src/multipart.rs:2964`). |
| T3 Runtime | N/A | This slice intentionally has no writer, store call, async work, or production consumer; its executable behavior is the pure decode boundary exercised by the test suite (`crates/core/src/multipart.rs:72`). |
| T4 Contribution | FAIL | The batched review remains red on the missing hybrid-generation completeness/identity witness; the affected-path merged and closed/rejected prior-art check is recorded, while the contribution-artifact subcheck is correctly N/A until publish (`gate-logs/T4-batch-review.log:10`, `brief.md:204`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild with the hybrid generation acceptance/identity test and rerun the batched review — without that witness, the claimed complete accepted set for this frozen record format is not causally demonstrated (`crates/core/tests/multipart_retire_obligation.rs:306`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether freezing this decoder-only stored format ahead of its writers is fit for the downstream store slices — any mistaken accepted shape becomes a compatibility constraint once those writers persist it (`crates/core/src/multipart.rs:72`). |

### Advisory — adversary

# Adversarial review — issue #771 (multipart retirement obligation)

Method: re-ran the asserted GREEN leg against a writable copy of `$PDCA_TARGET`
(`cargo test -p wyrd-core --test multipart_retire_obligation` → 26/26 pass, reproduced), then
attacked `decode_retire_obligation` with ~15 hand-authored values it has no witness for, and
attacked the suite by deleting checks and assertions to see which ones are load-bearing.

## Findings

- NEEDS-HUMAN [human] — **The decoder accepts a generation shape no writer can install, and the
  doc that licenses it states a falsehood about the data model.**
  `crates/core/src/multipart.rs:2583` says a retired generation's map is "a flat chunk list, a
  segment group, or (for a generation whose root carried both) **each**", and `checked_shape`
  (`crates/core/src/multipart.rs:2917`) rejects a generation only when it names **neither**. No
  root can carry both: `InodeRecord` holds one `chunk_map` field (`crates/core/src/metadata.rs:1388`)
  of type `ChunkMap`, a two-arm `Flat | Segmented` enum whose own doc cites "proposal 0016
  decision 7(a)" (`crates/core/src/metadata.rs:1014`). Concrete case, run against this tree: key
  `retire_key(Bytes, g:42:4)`, value
  `{"generation":{"inode":42,"version":4,"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}],"segments":{"nonce":"c3…c3","epoch":9}}}`
  → `Ok(RetirePayload { generation: Some(RetireGeneration { chunks: [..], segments: Some(..) }) })`.
  That contradicts the brief's success criterion ("every shape no writer installs is rejected with
  a typed `RecordError`"), the module's own writer table (`crates/core/src/multipart.rs:2771-2781`,
  which has exactly one `generation` row), and the sentence this patch adds to the living
  architecture doc — `docs/design/architecture/05-building-block-view.md:204` asserts "the accepted
  shapes are exactly the ones some batch of the protocol installs" and describes "a superseded
  generation's chunk map **or** segment group". **Note the direction of the two gating T4
  blockers**: both ask for an *acceptance / round-trip* test of this hybrid, i.e. they would freeze
  it into a stored format three later slices build against. The authorities genuinely conflict —
  `0016:355` spells `{inode, version, chunks, segments?}` while `0016:2416` spells
  `{inode, version, chunks?, segments}`, and `ChunkMap` implements neither as a union — so whether
  to make the hybrid a `RecordError` (mutual exclusion, mirroring `ChunkMap`) or to justify it
  against `ChunkMap` is a format decision a rebuild should not guess.

- NEEDS-HUMAN [impl] — **R9's advertised "file-wide property" is unfalsifiable; it cannot fail for
  any witness in the file.** `crates/core/tests/multipart_retire_obligation.rs:167`'s
  `decode_witness` re-encodes every accepted witness and compares to the input, and the file header
  (`crates/core/tests/multipart_retire_obligation.rs:22-28`) claims that makes
  `encode(decode(bytes)) == bytes` "a property of the *whole* file's accepted set". But
  `decode_retire_obligation` already ends in `require_canonical(payload, value, "retire:")`
  (`crates/core/src/multipart.rs:3096`), which returns `Err` unless exactly that equality holds — so
  the assertion restates the production postcondition it is policing. Verified: with the whole
  `assert_eq!` block deleted from `decode_witness`, the suite is still **26 passed; 0 failed**. The
  suite's only real R9 leg is `a_foreign_spelling_of_an_accepted_payload_is_rejected`
  (`crates/core/tests/multipart_retire_obligation.rs:646`). The sibling helper this is modelled on
  (`crates/core/tests/multipart_session_records.rs:169`) earns its identity assertion through the
  **S1** leg (`metadata::decode`, which does not call `require_canonical`); this file legitimately
  drops S1, and with it the only decode path the assertion could ever have caught. Fix is the
  claim, not the code: say identity is enforced in production by `require_canonical` and pinned by
  the foreign-spelling leg, rather than presenting the helper as independent evidence.

- NEEDS-HUMAN [impl] — **`PartNumberSet::from_numbers` can mint a value whose stored spelling its
  own decode refuses**, against the "unrepresentable at the source" thesis its doc states
  (`crates/core/src/multipart.rs:2434-2438`: "it can only produce the canonical encoding its own
  decode accepts"). `from_numbers([])` returns `PartNumberSet(vec![])`
  (`crates/core/src/multipart.rs:2439-2452`) — the test pins this at
  `crates/core/tests/multipart_retire_obligation.rs:767` — and that set serializes to `[]`, which
  `checked_shape` then refuses: `decode_retire_obligation(retire:bytes:s:…:7, br#"{"parts":[]}"#)`
  → `Err(RetireObligationOwesNothing { component: "parts" })` (verified). The writer rows this
  constructor exists for compute a possibly-empty set (the root flip's "staged parts Complete did
  not name", `0016:662`, `:919-921`); a #656–#659 writer that mints one and installs it stores an
  obligation no drain can decode, and the session's terminal-delete emptiness gate (`0016:673`)
  never clears. Cheapest fix: make the constructor fallible (or return `Option<Self>` for the empty
  case) so the non-empty obligation lives in the type rather than in writer discipline.

## Attacks that failed (could not refute)

- The accepted set otherwise matches the writer table exactly. Hand-authored probes for
  `{parts:"all"}` under `retire:records:`, `{parts:"all"}` without `session`, `{session}` /
  `{parts}` / `{seg}` under a per-part token, `{chunks}` under a suffix-free token, `{generation}`
  under an `s:` token, `{session,all}` under a `g:` token, `{seg}` under `retire:bytes:`,
  `{chunks}`/`{generation}` under `retire:records:`, and `{}` under a records per-part key were each
  refused with the typed variant the `Component` table predicts
  (`crates/core/src/multipart.rs:2680-2734`, `:3003-3044`).
- The `Component` table is not covered by `C5-mutants` (consts, not functions), so I mutated it by
  hand: relaxing `mode` on `SESSION_`/`ALL_PARTS_`/`CHUNKS_`/`GENERATION_`/`SEG_COMPONENT`, and
  `scope` on `PARTS_`/`GENERATION_COMPONENT`, each changes a verdict the suite asserts by exact
  error identity. No entry is dead weight.
- Canonical-set arithmetic: `[[1,1],[1,1]]`, `[[1,5],[4,8]]`, `[[5,9],[1,3]]`, `[[4,2]]`, `[[0,3]]`,
  `[[1,1000000]]`, `[[1,4294967296]]`, `[[-1,4]]`, a 50-run set and `[[1,4],[6,9]]` (legal gap of 1)
  all land exactly where `PartNumberSet::from_runs` (`crates/core/src/multipart.rs:2414-2432`) says;
  the `previous_hi + 1` overflow argument holds because both endpoints pass `PartNumber::new` first.
  Duplicate JSON keys, `"chunks":[]`, `"segments":null`, a `\u`-escaped nonce and reordered fields
  are all refused.
- `C4-verify` is `unverifiable` (RED leg does not compile against the reverted base). That is
  pre-declared in the brief's Falsifiability section as a born-at-tier sign-off item, so I did not
  score it as a refutation. Likewise the 32 `C4-diff-cov` MISS lines are all `RecordError` `Display`
  arms (`crates/core/src/multipart.rs:535-596`); no sibling test in `crates/core/tests/` asserts
  error strings, so this is not a repo convention being broken.
- Size/scope: 966 added semantic lines across exactly 3 files (module 468, test 497, docs 1), inside
  the brief's ≤1,000 budget — the round-1 T1 concern is resolved and I am not re-raising it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared born-at-tier evidence posture — the independent and frozen post-fix runs pass 26 tests and full CI, but the pre-fix leg never executes a discriminator, so no behavioral red→green was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:108`).
- [ ] C5 Causal adequacy — Add an accepted round-trip witness for a generation carrying both `chunks` and `segments` — the format explicitly permits the hybrid, while the suite tests only the two separate forms, so a regression rejecting the combined form can survive (`crates/core/src/multipart.rs:2581`, `crates/core/tests/multipart_retire_obligation.rs:309`, `crates/core/tests/multipart_retire_obligation.rs:325`).
- [ ] T5 Judgment — Rebuild with the hybrid generation acceptance/identity test and rerun the batched review — without that witness, the claimed complete accepted set for this frozen record format is not causally demonstrated (`crates/core/tests/multipart_retire_obligation.rs:306`).
- [ ] Validation — fitness-to-purpose — Decide whether freezing this decoder-only stored format ahead of its writers is fit for the downstream store slices — any mistaken accepted shape becomes a compatibility constraint once those writers persist it (`crates/core/src/multipart.rs:72`).
- [ ] **The decoder accepts a generation shape no writer can install, and the
- [ ] **R9's advertised "file-wide property" is unfalsifiable; it cannot fail for
- [ ] **`PartNumberSet::from_numbers` can mint a value whose stored spelling its
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- [ ] T5 Judgment — Decide the safe public API boundary — `RetirePayload: Deserialize` exposes value-only decoding even though the record’s correctness depends on key relations, so future consumers can bypass the documented key-taking boundary (`crates/core/src/multipart.rs:2762`, `crates/core/src/multipart.rs:2994`).

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the declared born-at-tier evidence posture — the independent and frozen post-fix runs pass 26 tests and full CI, but the pre-fix leg never executes a discriminator, so no behavioral red→green was established (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:108`).; C5 Causal adequacy — Add an accepted round-trip witness for a generation carrying both `chunks` and `segments` — the format explicitly permits the hybrid, while the suite tests only the two separate forms, so a regression rejecting the combined form can survive (`crates/core/src/multipart.rs:2581`, `crates/core/tests/multipart_retire_obligation.rs:309`, `crates/core/tests/multipart_retire_obligation.rs:325`).; T5 Judgment — Rebuild with the hybrid generation acceptance/identity test and rerun the batched review — without that witness, the claimed complete accepted set for this frozen record format is not causally demonstrated (`crates/core/tests/multipart_retire_obligation.rs:306`).; **R9's advertised "file-wide property" is unfalsifiable; it cannot fail for; **`PartNumberSet::from_numbers` can mint a value whose stored spelling its; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
