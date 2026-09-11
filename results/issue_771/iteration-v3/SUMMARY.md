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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 86.5% — 217 of 251 instrumentable changed lines executed (floor 80%); 251 of 1002 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 76 mutants tested in 2m: 1 missed, 46 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_771/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.24s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of the multipart retirement-obligation value/decoder, its canonical range encoding, validation rules, tests, and architecture documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The frozen brief makes the combined-value decision, ten acceptance/rejection legs, exact three-file scope, and external-tool posture explicit (`brief.md:14`, `brief.md:18`, `brief.md:128`, `brief.md:148`). |
| C2 Reproduction (red pre-fix) | PASS | The retained named test independently fails against the stashed pre-fix production tree because every new obligation API is absent, confirming the declared criterion-absence red (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:109`). |
| C3 Change | PASS | The planned change is one coherent persisted-format slice with writers, store calls, async work, and unrelated metadata excluded, and its three-file/1,000-semantic-line boundary is decision-complete (`brief.md:128`, `brief.md:134`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared born-at-tier evidence posture and classify the unchanged-base advisory separately — 27 post-fix tests pass but the pre-fix leg executes no behavioral discriminator, while `cargo deny` remains red on unchanged `h2 0.4.15` (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:110`, `Cargo.lock:1535`, `gate-logs/C4-ci.log:5244`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the causal suite to accept the required hybrid and independently reject empty generation chunks — it currently asserts the hybrid is invalid and the `!chunks.is_empty()` mutant survives (`crates/core/tests/multipart_retire_obligation.rs:548`, `crates/core/src/multipart.rs:2704`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | The patch stays within exactly the three allowed files and re-derived counting yields 993 added nonblank/non-comment semantic lines, within the 1,000-line budget (`brief.md:134`). |
| T2 Shape | FAIL | The frozen public representation must express a generation carrying both `chunks` and `segments`, but `RetiredMap` makes them mutually exclusive and decode rejects that required value (`crates/core/src/multipart.rs:2632`, `crates/core/src/multipart.rs:2703`). |
| T3 Runtime | N/A | No writer, store call, async work, or production consumer ships in this slice, so runtime integration is deliberately outside its reach (`crates/core/src/multipart.rs:72`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their audit reruns at publish; affected-path history/no-open-collision was rechecked and the brief inventories the archived rejected attempts, so no Check-time clearance is owed (`brief.md:204`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild around the brief's already-settled hybrid decision — the patch reopens proposal ambiguity and codifies exclusivity, which would freeze an incompatible format for downstream writers (`brief.md:221`, `crates/core/src/multipart.rs:2627`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this ahead-of-writers record format is fit to become the downstream slices' compatibility boundary after hybrid support and causal coverage are corrected — no production consumer yet supplies operational validation (`brief.md:168`). |

### Advisory — adversary

# Adversarial review — issue #771 (`multipart-retire-obligation`)

Inputs: `patch.diff`, `brief.md`, `check-gates.json`, `gate-logs/`. Grounded on
`$PDCA_TARGET`. I rebuilt the crate in a scratch copy and re-ran the named test plus a
19-input probe suite against the production decoder, so the findings below are reproduced,
not inferred.

## Findings

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2704`: half the R2 rule ("an obligation
  naming nothing is rejected") is not load-bearing, and a concrete input proves it.** The
  emptiness rule for a generation's flat map lives in a *match guard*
  (`(Some(chunks), None) if !chunks.is_empty()`). I replaced that guard with `true` in a scratch
  copy of `$PDCA_TARGET` and re-ran `cargo test -p wyrd-core --test multipart_retire_obligation`:
  **27 passed, 0 failed** — and the witness `{"generation":{"inode":42,"version":4,"chunks":[]}}`
  under `retire:bytes:g:42:4` then decodes to
  `Ok(RetirePayload { generation: Some(RetireGeneration { .., map: Flat([]) }) .. })`, i.e. an
  accepted obligation that owes nothing, re-encoding byte-identically so `require_canonical`
  (`multipart.rs:3175`) cannot catch it either. That is exactly the residue class the brief's R2
  forbids and this is exactly the gate's own red: `gate-logs/C5-mutants.log:13`
  (`MISSED … replace match guard !chunks.is_empty() with true`). The R2 test
  (`crates/core/tests/multipart_retire_obligation.rs:395-407`) covers `{}`, `{"chunks":[]}`,
  `{"parts":[]}` and a generation with **no** map key at all (`:402-406`) — never a generation
  whose flat map is *present but empty*. The brief's Falsifiability section makes this binding,
  not cosmetic: "A leg that stays green under its own negation is not load-bearing and must be
  rewritten." Since the born-at-tier posture makes the nine negations the *substitute* for a
  behavioural red (`gate-logs/C4-verify.log:110-116` — the RED leg never executed a
  discriminator), a rule that survives its own negation removes the only evidence this leg has.
  Minimal fix: add the witness to `an_obligation_owing_nothing_is_rejected`. While fixing, note
  a second, related inconsistency the same match has — `{"generation":{…,"chunks":[],"segments":
  {…}}}` is reported as `RetireGenerationTwoMaps` (probe output), so `Some([])` counts as a map
  for the two-maps arm at `:2703` but not for the owes-nothing arm at `:2704`; normalising an
  empty `chunks` to `RetireObligationOwesNothing` before the arm split makes both consistent and
  kills the mutant.

- **NEEDS-HUMAN [human] — the gating red `C4-ci` is not this patch's: `gate-logs/C4-ci.log:2856`
  is `RUSTSEC-2026-0258` (h2 0.4.15, "unbounded empty DATA frames") against `Cargo.lock:111`.**
  The diff touches exactly three files (`crates/core/src/multipart.rs`,
  `crates/core/tests/multipart_retire_obligation.rs`,
  `docs/design/architecture/05-building-block-view.md`) and no manifest or lockfile, and the log
  reports `advisories FAILED, bans ok, licenses ok, sources ok` — an advisory-DB refresh, not a
  regression this bundle introduced. Per issue #236 I do **not** score this as a refutation; it
  needs a human scope decision (bump `h2` to ≥ 0.4.16 in a separate bundle vs. a `deny.toml`
  exemption) because taking it inside this bundle would break the brief's "exactly 3 files"
  budget. Flagging it so sign-off does not read the gating red as evidence against the fix.

- **NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:204` states protocol
  behaviour the system does not have, in a doc whose stated rule is "as it is".** The added
  paragraph says an obligation "is installed under a compare-and-set that requires its key absent
  and drained under one that requires its exact bytes" — but nothing installs or drains one
  (`multipart.rs:77-78`, and the brief's own "Production reach: this child ships **no**
  production reach"), and the *immediately preceding* paragraph (`:202`) promises the opposite:
  "the protocol itself (fenced state transitions, staged publication, retirement) arrives with
  the store round trip (#656–#659) and is specified in the proposal, not here" — as does the
  module header the same patch rewrote (`multipart.rs:83-85`: the doc "defers the *protocol* …
  and the retirement drain — to the proposal"). The brief's R10
  asked to "Extend that sentence in its own voice and length; do not restate the proposal". Low
  severity and the host may decline it as taste — but the two adjacent paragraphs now disagree
  about what is landed, which is the thing a *living* doc is for. Trimming the install/drain-CAS
  and token-minting clauses (keeping the namespaces, the value's contents and the
  decoded-against-its-key rule, which are what this child actually landed) resolves it.

## Attempted refutations that failed (stated, so the silence is informative)

- **The records-mode `{session}` exclusion** (`multipart.rs:2764-2768`, `SESSION_COMPONENT` mode
  `Bytes`) looked like the highest-value refutation, since `0016:356`'s value column literally
  reads "`{session, parts}` and/or `{seg: …}`" for `retire:records:`. I checked it against every
  writer row in the batch table (`0016:659-673`) and the reaper (`:2186-2195`): the rows install
  `retire:records:{parts}` (root flip), `retire:records:{seg:<g>:<E>}` (fence release, abort/reap
  fence, `Completing`→`Aborting`, restore fence `:823`, reaper rollback `:2194`) — **no row
  installs a records-mode `{session}`**, and the session's own records are the terminal delete's
  (`:673`). Refusing it is also the reversible direction for a frozen format. Could not refute.
- **R6's exact epoch equality** (`multipart.rs:3067-3074`). I checked every row that installs a
  `{seg}` obligation: each preconditions `require(mpu == …@E)` and names `seg:<g>:E`, including
  the two that advance the session to `E+1` in the same batch (`0016:665`, `:2188-2194`). Token
  epoch `E` == group epoch `E` in all of them; no writer row needs the `E±1` window the archived
  v3 shape had. Could not refute.
- **R1 completeness.** All nine writer shapes decode under their own key, including the two
  combined ones (`{session, parts}`, `{parts}+{seg}`); the four session-scoped/per-part/generation
  cross-products I probed are all typed rejections. Could not refute.
- **Input probing of the decoder** (19 hand-authored values run against the production
  `decode_retire_obligation`): `parts` as `null` / object / 3-tuple / `[[1,4294967295]]` /
  `[[1,1],[1,1]]`, duplicate JSON keys, trailing whitespace, `"chunks":null` beside `segments`,
  a negative `len`, a `u128`-max chunk id, an empty `placement`, `{parts}+{seg}` under a
  `retire:bytes:` key, `{session, all}` under `retire:records:`, `{chunks}+{session}` under a
  per-part token, `{seg}` with `epoch ± 1`. Every one is either a typed `RecordError` or a
  correctly-accepted contextual case (empty/short `placement`, per ADR-0045 `:45-49` and the
  brief's explicit R8 boundary). Nothing decoded that should not have, apart from the guard case
  in finding 1. Could not refute.
- **Serialization identity (R9).** `require_canonical` (`multipart.rs:1725-1735`) is the real
  gate and it caught every foreign spelling I could invent (field order, inserted whitespace,
  `false` spelled instead of omitted, `"chunks":[]`, `"parts":null`, `\u`-escaped nonce). The test
  file no longer over-claims the helper as independent evidence
  (`crates/core/tests/multipart_retire_obligation.rs:22-37`), which was the previous round's
  finding. Could not refute.
- **No writer-side constructor / no `Deserialize` on `RetirePayload`** (`multipart.rs:2894`) —
  I checked this against the siblings rather than taking the doc comment's word: `SessionRecord`
  (`:1921`) and `PartRecord` (`:2283`) are equally constructor-less with private fields, so
  #656–#659 is no worse off here than it already is. Could not refute.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared born-at-tier evidence posture and classify the unchanged-base advisory separately — 27 post-fix tests pass but the pre-fix leg executes no behavioral discriminator, while `cargo deny` remains red on unchanged `h2 0.4.15` (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:110`, `Cargo.lock:1535`, `gate-logs/C4-ci.log:5244`).
- [ ] C5 Causal adequacy — Rebuild the causal suite to accept the required hybrid and independently reject empty generation chunks — it currently asserts the hybrid is invalid and the `!chunks.is_empty()` mutant survives (`crates/core/tests/multipart_retire_obligation.rs:548`, `crates/core/src/multipart.rs:2704`, `gate-logs/C5-mutants.log:13`).
- [ ] T5 Judgment — Rebuild around the brief's already-settled hybrid decision — the patch reopens proposal ambiguity and codifies exclusivity, which would freeze an incompatible format for downstream writers (`brief.md:221`, `crates/core/src/multipart.rs:2627`).
- [ ] Validation — fitness-to-purpose — Decide whether this ahead-of-writers record format is fit to become the downstream slices' compatibility boundary after hybrid support and causal coverage are corrected — no production consumer yet supplies operational validation (`brief.md:168`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] external dependency: cargo-deny advisory database (RUSTSEC-2026-0258, `h2 0.4.15`) — blocks the gating `C4-ci` (`cargo xtask ci`) at its `cargo deny check` step for the whole workspace, so a fully-green gate run cannot be produced for this bundle; every other CI step, including the 50-seed DST sweep, passes. Resolve by `cargo update -p h2` (to ≥ 0.4.16) on `main` or by a reviewed `deny.toml` ignore — both outside this child's scope.
- [ ] size backstop — this slice is behaving oversized: patch is 101 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Iteration delta (if iterating): Rebuild targeting the substantive implementation/spec-conformance defects found, not the unrelated RUSTSEC/cargo-deny item or the size backstop's iterate-plan suggestion: 1. C5 mutant survivor: the "empty obligation must be rejected" (R2) rule does not actually fire when a generation record's `chunks` list is present but empty (`crates/core/src/multipart.rs:2704`, match guard `!chunks.is_empty()` can be replaced with `true` and all 27 tests still pass). Normalize an empty `chunks` to `RetireObligationOwesNothing` before the arm split, add a witness test with a present-but-empty flat map to `an_obligation_owing_nothing_is_rejected`. 2. T2 Shape FAIL: the brief requires the payload to express a generation carrying BOTH `chunks` and `segments` together (a hybrid, `0016:355`/`:2417`), but `RetiredMap` (`crates/core/src/multipart.rs:2632`) makes them mutually exclusive and decode rejects the required hybrid shape. Implement the hybrid per the brief's R1 completeness leg. 3. The new architecture-doc paragraph (`docs/design/architecture/05-building-block-view.md:204`) asserts install/drain CAS behavior that does not exist in this tree yet and contradicts the immediately preceding paragraph. Trim it to only the namespaces, the value's contents, and the decoded-against-its-key rule per R10's own instruction ("extend the sentence, do not restate the proposal"). 4. T5 Judgment: consider narrowing `RetirePayload`'s public `Deserialize` boundary so value-only decoding cannot bypass the documented key-relation validation (`crates/core/src/multipart.rs:2762`, `:2994`). Explicitly out of scope for this iteration: the C4-ci `cargo deny` / RUSTSEC-2026-0258 `h2` advisory (unrelated supply-chain finding), and the size-backstop's iterate-plan recommendation — proceeding with iterate-do on the findings above instead.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- §6's last bullet is cut off mid-sentence ("The decoder accepts a generation shape no writer can install, and the...") — check the bundling/assembly step that writes §6 for a truncation bug.
