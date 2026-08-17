- **Slug:** multipart-retire-obligation
- **Defect / goal:** the retirement **obligation value** does not exist. The base carries the
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
- **Success criterion:** every obligation `0016`'s writer rows install decodes from its own
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
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier, as both siblings beneath this
  chain. `C4-verify` classifies on **added test files** (`run-verify.sh:142`,
  `_is_test_file` matches `*/tests/*.rs`), so `ADDED_TEST crates/core/tests/multipart_retire_obligation.rs`
  is the discriminator and `cargo test -p wyrd-core --test multipart_retire_obligation` is the
  GREEN leg; that path is **not** cfg-gated (unlike `crates/dst/tests/*`, `run-verify.sh:148-154`),
  so it genuinely compiles and runs under the gate's own invocation. The RED leg reverts
  production, the test then fails to **compile**, and the gate reports **UNVERIFIABLE (exit
  77)** — **EXPECTED and PRE-DECLARED** here so it lands as a known sign-off item, not a
  surprise NEEDS-HUMAN. **What Do MUST capture instead (binding): NINE isolating negations,
  and this list is the authority for that count** — one per binding rejection leg (R2, R3,
  R4-session-under-part, R4-chunks-under-session, R5, R6-epoch, R7, R8-noncanonical) plus one
  for R9 (break the identity on one accepted witness). For each: remove that single check, run
  the test, paste the failing output into `build-notes.md`, revert. **Each negation must
  ISOLATE its rule** — exactly one test fails. A leg that stays green under its own negation
  is not load-bearing and must be rewritten. R1's completeness legs are negated the other way
  (they assert acceptance): collapse the payload's combined shape and show the
  `{session, parts}` and `{parts}+{seg}` legs fail to construct or decode.
- **Invariant to restore:** ADR-0045 decision 1, **parse-don't-validate at decode for
  structural invariants** (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`; its
  invariant table at `:69-74`), applied over this child's category: **a stored obligation's
  payload may not disagree with the token that names it, and the disagreement must surface as
  a typed error, never as a value.** A retirement obligation is the only evidence that bytes
  or records are reclaimable; a payload accepted under the wrong token — or the wrong *scope*
  of token, or a second spelling of the right one — reclaims one attempt's data while clearing
  another's obligation, and the loss is invisible because no record names the bytes any more
  (`0016:369-373`, outcome (a)). Untrusted `EcScheme` geometry that decodes is the #285 panic
  class made durable (ADR-0045 `:71`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** Wave 0 — no prerequisite. It touches only `crates/core/src/multipart.rs`,
  its own new test and one sentence of `05-building-block-view.md`, so it collides with no
  other bundle in flight (#721 and #722 share `crates/core/src/metadata.rs` and
  `crates/dst/tests/custodian.rs` with **child-2**, not with this one). child-2 depends on this
  child; see its ordering note.
- **Surfaces:** data
- **Difficulty:** high — rated on **effect propagation**, the second half of the blast-radius
  criterion, not on edge-case density (the gates own that) and not on file count, which is
  only 3. This child **freezes a stored record format**: three later slices (#656–#659's store
  round trips, then #693 and #655) are built against these shapes, and a wrong shape is
  undetectable until a writer exists — a diff-reviewer must hold `0016`'s entire writer table
  in view to judge R1 and R6, which is exactly what two of the three prior rounds failed to do.
  Rating up is also the instructed default when unsure, and it is load-bearing: `high` routes
  the opus builder (`pdca.toml:483-486`) and enables the adversary refutation pass
  (`pdca.toml:754-765`) — the pass that found the `{session, parts}` defect in the first place.
- **Scope (one logical fix) / out of scope:** extend `crates/core/src/multipart.rs` with the
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
- **Reproduction:** n/a — new functionality on a merged base. Nothing in production reads or
  writes a `retire:` value today; the key half exists (`multipart.rs:945-1100`) and the value
  half does not, which is the gap. Verify that on the base with
  `git -C ../wyrd grep -n "RetirePayload\|PartNumberSet" origin/main -- crates/` (no hit).
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`,
  `cargo-mutants` — all registered `[[doctor.checks]]` ids; `docs-renderer` is load-bearing
  here (R10 edits a rendered architecture doc), the rest warn-skip locally while CI enforces
  them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no
  runtime, no Docker, no new crate, no `Cargo.toml` / `Cargo.lock` change.
- **Test file:** `crates/core/tests/multipart_retire_obligation.rs` — a **NEW** file, not
  optional: `C4-verify` classifies on added `*/tests/*.rs` (`run-verify.sh:142`,
  `:269-273`), so a test appended to an existing suite would silently degrade the gate to its
  green-only branch and prove nothing. Mirror `crates/core/tests/multipart_session_records.rs`
  (#716): pure, hand-authored JSON bytes, no store and no async, every witness **decoded rather
  than constructed**, and both surfaces asserted to agree — the module's attributed
  `decode_retire_obligation` and the store-wide `metadata::decode` — through a `decode_both`-shaped
  helper (`:169`) that makes serialization identity (R9) a property of the whole accepted set.
  Co-located unit tests may ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as its siblings. The
  UNVERIFIABLE RED is pre-declared above so C2/C4 land as known sign-off items. Everything this
  child builds IS exercised at Check: the named test under the gating `C4-ci`
  (`cargo xtask ci`), plus `C5-mutants` on the bundle diff, plus the nine demonstrated
  negations in `build-notes.md` standing in for the flippable red. Nothing here is deferred to
  an off-Check environment — the whole child is pure functions over hand-authored bytes.
- **Production reach:** this child ships **no** production reach at all and none is claimed:
  no writer, no store call, no live path traverses it. That is not a seam-ahead-of-consumer
  declaration but the slice's whole shape — the record grammar and its decoder land ahead of
  the store round trips (#656–#659) that first install an obligation, exactly as `AdmissionRecord`
  (#715) and `SessionRecord` (#716) did beneath it (`multipart.rs:63-73`). The binding criterion
  is honoured by hand-authored values in the named test, load-bearingly (each of the nine
  negations makes exactly one leg fail), never by dead scaffolding.
- **Citations expected:** cite `path:line` on the merged base (`origin/main`, c824243 or later)
  for every change; in `multipart.rs` prefer **symbol** over line where a citation must survive
  a rebase. Sources Do MUST open: `0016:346-356` (the §1 value column — the three bytes-mode
  shapes and the two records-mode ones R1 enumerates), `0016:358-388` (the token grammar R4/R6
  turn on, and the range-encoded `parts` set of R8), `0016:390-414` (structural validity at
  decode, against FORMAT maxima and never live knobs), `0016:416-432` (the placement-length
  contextual boundary R8 must NOT cross), `0016:434-441` (mode-in-the-key, R3), `0016:659-673`
  (the writer rows R1 is derived from), `0016:2350-2380` (the per-attempt epoch scoping R6
  rests on), `0016:499-509` (why the segment-group nonce is independent of the upload id — the
  limit R6 must state), ADR-0045 (`:42-49`, `:55-59`, `:69-74`).
  Peer callsites Do MAY open and SHOULD mirror rather than re-derive:
  `crates/core/src/multipart.rs:1937` (`checked_chunk_scheme` — R7's predicate, already
  written); `:1996` and `:2029` (`ChunkRefWire` / `EcSchemeWire`, the closed nested wire shapes
  R7 reads through, with the doc that explains why a closed shape is required for
  serialization identity); `:1828` and `:2156` (`decode_session_record` / `decode_part_record`
  — the per-record attributed-decode wrapper shape `decode_retire_obligation` takes, one extra
  key parameter aside); `:1022-1075` (`RetireToken` / `retire_key` — the token half this child
  cross-checks against, including its canonical-spelling doc); `crates/core/src/erasure.rs:120`
  (`supported(k, m)`); `docs/design/architecture/05-building-block-view.md:202` (the sentence
  R10 extends) and `:187-194` (the ADR-0047 optional-fields bullet whose voice and length to
  match).
  **Salvage:** `$PDCA_HARNESS_ROOT/results/issue_717/iteration-v3/patch.diff` (the path is
  relative to the HARNESS repo, not `$PDCA_WORKTREE`) holds a working `PartNumberSet`
  (`from_runs` / `from_numbers`, whose overflow arithmetic the adversary pass verified) and a
  `decode_retire_obligation`. Take them, then **fix the two recorded defects rather than
  re-shipping the reviewed shape**: the mutually exclusive `Session {}` / `Parts { parts }`
  arms (R1) and the `token.epoch − group.epoch() ∈ {0,1}` window (R6). Its
  `checked_against_token` doc also over-claims F18 containment; R6's limit note is the
  correction.
- **Prior-art check (triage cycles):** verified at Plan against `origin/main` @ c824243:
  `git -C ../wyrd grep -n "RetirePayload\|PartNumberSet\|decode_retire_obligation" origin/main
  -- crates/` returns nothing; the `retire:` key half exists and the value half does not
  (`multipart.rs:945-1100`). `git -C ../wyrd log --oneline origin/main -- crates/core/src/multipart.rs`
  shows only `5eeca16` (#715) and `778f1cf` / `a3b2bbe` (#716). Open PRs touching
  `crates/core/src/multipart.rs`: **none**. Closed / rejected work over this material: #654's
  two archived attempts, #692's two, and #717's own three — the batch review's token-scope
  blocker and the round-3 epoch blocker are legs R4 and R6 here, not suggestions.
- **Disposition hint:** new-feature
