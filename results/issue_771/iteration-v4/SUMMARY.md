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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 87.5% — 223 of 255 instrumentable changed lines executed (floor 80%); 255 of 983 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 74 mutants tested in 2m: 43 caught, 31 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.06s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #771’s multipart retirement-obligation value, key-bound decoder, canonical part-range encoding, tests, and architecture documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable R1–R10 matrix for every accepted writer shape, every rejected structural mismatch, and exact-byte identity (`brief.md:14`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no base reader or writer; the declared evidence is criterion absence rather than an existing behavioral defect (`brief.md:144`). |
| C3 Change | PASS | The change stays within the authorized decoder, new test, and one living-architecture sentence, with the keyed decode boundary at `crates/core/src/multipart.rs:3144` and docs currency at `docs/design/architecture/05-building-block-view.md:202`. |
| C4 Verification (red→green) | FAIL | The required CI gate remains red on unchanged `Cargo.lock` dependency `h2 0.4.15` / RUSTSEC-2026-0258, and the retained test under stashed production fails to compile before running a discriminator, so a verified behavioral red→green cannot be claimed (`gate-logs/C4-ci.log:5250`; `gate-logs/C4-verify.log:102`). |
| C5 Causal adequacy | PASS | The key-bound checks cover mode, scope, generation identity, exact segment epoch, and wildcard context, while the 27-test causal suite and 74-mutant run report no survivor (`crates/core/src/multipart.rs:3021`; `crates/core/tests/multipart_retire_obligation.rs:413`; `gate-logs/C5-mutants.log:10`). |
| T1 Structure | PASS | The patch changes exactly the three authorized files and contains 997 added nonblank, non-comment semantic lines, within the brief’s 1,000-line cap; the new test is isolated at `crates/core/tests/multipart_retire_obligation.rs:1`. |
| T2 Shape | PASS | The closed wire supports combined `{session, parts}`, `{parts, seg}`, and hybrid generation obligations, while private fields and the absence of `Deserialize` prevent a value-only bypass (`crates/core/src/multipart.rs:2811`; `crates/core/src/multipart.rs:2869`). |
| T3 Runtime | PASS | The pure decoder has no store, async, clock, or external-service path, and its targeted 27-test suite plus the workspace test run pass with the intended liberal placement-length boundary (`crates/core/tests/multipart_retire_obligation.rs:727`; `gate-logs/C4-ci.log:988`). |
| T4 Contribution | N/A | `pr-description.md` is absent by design at Check, and the mandatory substantive contribution-artifact audit reruns at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The affected-path prior-art record covers merged history and the closed/rejected #654, #692, and #717 attempts, and the frozen multi-pass review reports zero blockers (`brief.md:204`; `gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether to freeze this exact-byte obligation format before any production writer or consumer exists — downstream persistence will make an inadequate shape costly to revise (`crates/core/src/multipart.rs:72`; `crates/core/src/multipart.rs:2397`). |

### Advisory — adversary

# Adversarial review — issue #771 (`multipart-retire-obligation`)

Method: rebuilt the target into a scratch clone (cargo 1.96 available) and (a) re-ran the
patch's suite, (b) ran a **16-negation battery** against the production checks to test the
brief's "nine isolating negations" substitute for the pre-declared UNVERIFIABLE red, and (c)
ran two probe suites of hand-authored values against `decode_retire_obligation` looking for an
accepted-but-unsafe or refused-but-legal shape. Scratch removed.

## Findings

- **NEEDS-HUMAN [impl] — `decode_retire_obligation` validates the mode and then throws it
  away, so the drain it is built for cannot tell the two `{parts}` obligations apart**
  (`crates/core/src/multipart.rs:3144-3157`; the mode is in hand at `:3148` and dropped at
  `:3156`). Verified concretely in a scratch clone: `retire:bytes:s:<id>:7` and
  `retire:records:s:<id>:7` over the *same* value `{"parts":[[1,4]]}` both decode to
  **identical** `(RetireToken, RetirePayload)` pairs — `PARTS_COMPONENT.mode` is deliberately
  `None` (`:2758-2764`), so `{parts:<set>}` is the one legal shape under **both** modes and
  nothing in the returned pair distinguishes them. Those two obligations are opposites: bytes
  mode means *orphan-mark the unnamed staged parts' chunks, then delete their records*
  (`0016:662`, `:919-921`), records mode means *delete the published parts' records and never
  orphan-mark, because their bytes are live object content* (`0016:356`). A #656–#659 drain
  handed only the decode result must re-parse the key to know which; if it instead infers mode
  from the components present — the natural reading of a validated payload — it orphan-marks a
  published object's live fragments. That is exactly the "boolean misread once is silent data
  loss" hazard the mode-in-the-key rule exists to prevent (`0016:434-441`), and the module's own
  doc says "The drain dispatches on the prefix". The fix is inside this diff's own signature
  (return `(RetireMode, RetireToken, RetirePayload)`, or expose the mode on the payload) plus a
  test leg asserting the two keys' results differ; the frozen-format brief makes this the moment
  to get the seam right, since three later slices are built against it.
- **NEEDS-HUMAN [human] — the gating `C4-ci` red is real but not this diff's**: the only failure
  in the frozen run is `error[vulnerability]: h2 unbounded empty DATA frames` /
  RUSTSEC-2026-0258 (`gate-logs/C4-ci.log:2859`, `:5244`, tail `advisories FAILED, bans ok,
  licenses ok, sources ok`). I checked the whole log: fmt/clippy/build/test/conformance are
  green, no `test result: FAILED`, and the new suite's 27 tests pass in that same run
  (`gate-logs/C4-ci.log:1017`). It is a transitive `h2` (tonic/hyper) advisory, untouched by a
  patch that adds no dependency. Still a merge blocker needing a bump or a `deny.toml`
  exception — a supply-chain scope call this bundle cannot make, so it must land as an explicit
  sign-off decision rather than be inherited a fifth time.

## Refutations attempted and failed

- **The red→green substitute holds.** C4-verify is `unverifiable` because the reverted-production
  leg fails to *compile* (`gate-logs/C4-verify.log:16-30`) — pre-declared born-at-tier. I ran my
  own negation battery instead: `PartNumberSet::from_runs`'s empty / reversed / non-coalesced /
  endpoint-range checks (`multipart.rs:2427-2447`), `checked_chunks`' empty-list and
  `checked_chunk_scheme` legs (`:2596-2609`), `RetireGeneration`'s neither-source rule
  (`:2701-2705`), `checked_shape` (`:2985-2992`), `Component::checked_mode` (`:3061-3070`), each
  of the three `checked_scope` arms (`:3075-3096`), the all-without-session rule (`:3031`), the
  generation-identity and seg-epoch relations (`:3038-3055`) and `require_canonical` (`:3156`).
  **Every one is load-bearing and 15 of 16 fail exactly one test**; the sixteenth (removing
  `skip_serializing_if = "is_absent"`, `:2949-2950`) fails 13, which is the R9 property behaving
  as advertised. No dead check, no test passing for the wrong reason.
- **No accepted-but-unsafe shape found.** Probed the full mode × token-scope × component product
  plus: real 128-bit `ChunkId`s and `u64::MAX` `DServerId`s (round-trip exactly), `scheme:"None"`,
  duplicate JSON fields (`duplicate field` rejection), trailing whitespace (`Noncanonical`),
  `"session":false` / `"segments":null` (rejected), 200-run part sets, epoch `0` and `u64::MAX`,
  `version: u64::MAX`, `{session}+{chunks}` and `{parts}+{seg}` under each key. Every verdict was
  the documented one; every one of the eight new `RecordError` Display strings renders and names
  its own rule (they are the 32 uncovered lines behind the 87.5% diff-cov, but the sibling
  value-record suites assert no Display text either, so that is repo-consistent, not a gap).
- **The `session`-under-`retire:records:` refusal (`:2735-2757`) is not a defect**, though
  `0016:356`'s value column literally spells `{session, parts}` there: the row's own prose and
  every writer row give that namespace only the published parts' records and one rolled-back
  attempt's segments, brief R1 enumerates the records-mode shapes without `session`, and the
  code records the resolution citing `:356` as the brief instructed. Left as an observation, not
  a finding.
- **R9's "file-wide" identity assertion is a tautology while `require_canonical` stands** — but
  the test header now says so in its own words (`crates/core/tests/multipart_retire_obligation.rs:22-37`)
  and my N14 negation confirms the pinning leg (`a_foreign_spelling_of_an_accepted_payload_is_rejected`)
  is the one that goes red when the gate is dropped. Previously-raised, now honestly bounded.
- Also attempted without success: an unbounded run count or a `parts` cardinality escape (`0016:390-414`
  mandates format maxima only, and none applies), a non-canonical key spelling smuggling a second
  key for one obligation (`parse_retire_key` is base code and strict), and an over-strict rejection
  of any writer row in `0016:657-673`, `:2187-2194`, `:2417` — all eleven install shapes decode.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether to freeze this exact-byte obligation format before any production writer or consumer exists — downstream persistence will make an inadequate shape costly to revise (`crates/core/src/multipart.rs:72`; `crates/core/src/multipart.rs:2397`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] size backstop — this slice is behaving oversized: patch is 101 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] T5 Judgment — Decide the safe public API boundary — `RetirePayload: Deserialize` exposes value-only decoding even though the record’s correctness depends on key relations, so future consumers can bypass the documented key-taking boundary (`crates/core/src/multipart.rs:2762`, `crates/core/src/multipart.rs:2994`).
- [ ] **The decoder accepts a generation shape no writer can install, and the

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
- Iteration delta (if iterating): Rationale (human decision, overriding the bundle's own size-backstop recommendation of iterate-plan): the findings are judged tractable within this slice as-is. Fix on the next round: 1. [impl] `decode_retire_obligation` (multipart.rs:3144-3157) validates the record's mode (bytes vs. records) but discards it before returning, so a drain given only the decode result cannot tell apart two opposite obligations that share the same {parts} shape (bytes mode = orphan-mark then delete; records mode = delete records, never orphan-mark live data). Fix inside this diff's own signature — return (RetireMode, RetireToken, RetirePayload), or expose the mode on the payload — plus a test leg asserting the two keys' decode results differ. 2. [impl] carried from round 2: the decoder accepts a hybrid generation shape (both flat chunk-list AND segment-group) that no writer in the codebase can install, licensed by a module doc comment (multipart.rs:2583) that misstates the data model — InodeRecord's ChunkMap is a two-arm Flat | Segmented enum, never both. Concrete repro: key retire_key(Bytes, g:42:4) with value {"generation":{"inode":42,"version":4,"chunks":[...], "segments":{...}}} currently decodes Ok. Tighten checked_shape to reject "both" and correct the doc comment's claim. 3. The C4 CI red (cargo deny / h2 RUSTSEC-2026-0258) is a pre-existing transitive dependency advisory unrelated to this diff — not this slice's to fix; carry forward as a known, separately-tracked blocker rather than re-diagnosing it each round. 4. T5 Judgment (RetirePayload: Deserialize bypasses the key-bound decode boundary) and the Validation/fitness-to-purpose freeze-timing question remain open for the next round's sign-off to weigh, not resolved here. Note: §6's last bullet was truncated by a known SUMMARY assembly bug (carried-forward items losing their tail); the full text was recovered from the archived round-2 check-advisory-adversary.md and is reflected in finding #2 above.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- §6's carried-forward NEEDS-HUMAN items get truncated mid-sentence across rounds (this
  round's last bullet, "The decoder accepts a generation shape no writer can install, and
  the..." cut off) — check the bundling/assembly step that writes §6 for a truncation bug.
