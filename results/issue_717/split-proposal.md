<!-- pdca:split-proposal v1 -->
# Split proposal — issue 717 (multipart-staging-retire-pending)

<!-- Authored at the re-plan of 2026-08-14, after the human answered `iterate-plan` on
     iteration-v3 ("Re-plan and split rather than iterate-do"). Every citation below was
     re-checked against `origin/main` @ c824243 — the base #715 (PR #724) and #716 (PR #725)
     merged into. Line numbers in `multipart.rs` / `metadata.rs` moved under those merges;
     these are the current ones. -->

## Why this slice is oversized

#717 is the last child of #692's split, and it is the only one that was never one outcome.
It bundles **two disjoint record namespaces** that share nothing but the module they live in:

* `retire:bytes:` / `retire:records:` — the retirement **obligation** value, whose identity
  lives in a token (`0016:358-380`) and whose payload must express every shape the protocol's
  writers install;
* `sidx:` — the owned **staging entry**, plus the `PendingEntry` extension it is spelled in,
  which is the one live-path touch in the whole #692 chain.

Three Do rounds bear that out. `patch.diff` reached **123 KB / 1,780 added lines across 12
files** against a ≤960-line budget (size backstop fired: 121 KB vs a 100 KB threshold, 2
rounds spent), and the gating `T4-batch-review` came back red in **all three** rounds. The
surviving findings were never implementation slips — they are three **record-format
decisions** that Do kept being asked to make inside a slice too large to hold them:

1. **The retirement payload cannot express `{session, parts}`.** v2/v3 modelled `Session {}`
   and `Parts { parts }` as mutually exclusive enum arms, while `0016` names `{session,
   parts}` as **one** obligation in four places (`:355`, `:665`, `:823`, `:2193`). One
   obligation gets one key under `require_absent(retire:<mode>:<token>)` (`0016:369-373`), so
   the two halves cannot be installed separately — the first writer (#656–#659) would be
   stuck spelling one half under epoch `E` and the other under `E+1`.
2. **The retirement token has no canonical epoch.** `checked_against_token` accepted a
   segment generation under both `E` and `E+1`, so one obligation has **two valid keys** —
   installable twice, drainable twice (batch-review round 3, blocking). `0016` never pins it;
   it is an open design question, and it belongs here.
3. **`PendingEntry` accepts an owned-shaped value under a `pending:` key.** Deferred in v3 as
   "a shape check, not a key check" — and re-raised by the batch reviewer in **every** round,
   because the deferral named #657 as an *expectation* rather than a filed issue (the target's
   `AGENTS.md:200-203` settles only a deferral tracked in a real `#N`).

None of those get better with another Do pass, and the two namespaces do not need each other:
the base already carries `RetireToken` (`multipart.rs:1022`), `retire_key` (`:1071`),
`parse_retire_key` (`:1092`), `sidx_key` (`:907`) and `parse_sidx_key` (`:925`) — the **key**
halves of both — so each child adds only its own **value** half and the cross-check between
them. Split by namespace, each child is ~1,000–1,200 lines and carries exactly one of the
open decisions (child-1 takes 1 and 2, child-2 takes 3).

Two things the merged base changed that shrink both children: #716 landed the closed wire
mirrors `ChunkRefWire` (`multipart.rs:1996`) and `EcSchemeWire` (`:2029`) plus
`checked_chunk_scheme` (`:1937`), so neither child hand-rolls nested chunk/scheme validation;
and `multipart_session_records.rs` (#716) established the `decode_both` two-surface test
pattern (`:169`) both children mirror.

## Wave sketch

Two children, two waves. **child-2 `Depends on: child-1`.**

The dependency is genuine on both counts the wave model cares about:

* **Build-on.** child-1 introduces the module's first **key-taking** decoder
  (`decode_retire_obligation(key, bytes)`); every decoder on the base is value-only
  (`decode_session_record`, `multipart.rs:1828`; `decode_part_record`, `:2156`). child-2's
  `decode_owned_entry(key, bytes)` mirrors that shape rather than inventing a second one, and
  both grow the shared `RecordError` enum (`multipart.rs:96`).
* **Shared file.** Both append to `crates/core/src/multipart.rs` and both extend the same
  sentence of `docs/design/architecture/05-building-block-view.md` (`:202`). Same-wave
  siblings would collide on the fold.

With `auto_merge = false` this means the run **stops after wave 0** and asks for child-1's PR
to be merged before child-2 builds — the same boundary #715 → #716 → #717 already went
through, and the reason child-2's `Depends on` is honest rather than a scheduling trick.

**Two things acceptance cannot carry, to do by hand after `--accept`:**

* Add `- **Conflicts with:** 721, 722` to **child-2's** materialised `brief.md`. #711 was
  itself split into #721 (the placement primitive in `crates/core/src/metadata.rs`) and #722
  (the drain caller + `crates/dst/tests/custodian.rs`) — child-2 shares both files, and it
  must never share a wave with either. Ordering fields in a proposal may only name sibling
  labels, so this cannot be declared here. child-1 has **no** external conflict (it touches
  neither file).
* Repoint **#693** (`Depends on: 717`, and #655 behind it) at **child-2**, the terminal child.

## Convergence estimate

| | files | added lines (est.) | vs. the v3 attempt |
|---|---|---|---|
| child-1 (retire) | 3 | ~1,000 | — |
| child-2 (sidx + pending) | 13 | ~1,250 | includes ~180 lines the v3 scope deferred |
| iteration-v3 (one slice) | 12 | 1,780 | 123 KB, size backstop fired |

Each child lands well under the 100 KB backstop that fired on v3, and child-2 absorbs the
deferred `pending:`-namespace work rather than carrying it forward as a fourth round of the
same finding.

<!-- pdca:child child-1 -->
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** multipart-owned-staging-entry
- **Defect / goal:** the owned **staging entry** — the `sidx:<upload-id>:<part-number>:<chunk-id>`
  value — does not exist, and the `pending:` ledger cannot tell one from an ordinary lease.
  The base carries the key half (`sidx_key`, `multipart.rs:907`; `sidx_range`, `:918`;
  `parse_sidx_key`, `:925`) and a `PendingEntry` (`metadata.rs:1556`) that holds only
  `lease_expiry_millis`. This child lands `StagedPlacement` and the owned entry with its
  **key-taking** `decode_owned_entry(key, bytes)`; extends `PendingEntry` with the two additive
  optional fields `0016:442-457` specifies (`owner`, `staged`, both
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, which forces `Copy` off since
  `UploadId` is a `String` newtype); and — the part three Do rounds deferred and the batch
  reviewer re-raised every time — makes the **namespace** a decode-time property rather than a
  convention, so an owned-shaped value can never be read as an ordinary lease by the four
  `pending:` readers.
- **Success criterion:** the owned entry round-trips under its own key, every torn or misfiled
  value below is rejected with a typed error (ADR-0045), and the legacy `pending:` path is
  provably unchanged. Ten legs, each asserted in the named test files:
  **(S1, staged geometry)** `StagedPlacement`'s `EcScheme` is rejected unless
  `erasure::supported(k, m)` (`erasure.rs:120`), read through the module's closed
  `EcSchemeWire` (`multipart.rs:2029`) and mirroring `checked_chunk_scheme` (`:1937`) — the
  #285 precedent, ADR-0045's invariant table (`0045:71`): untrusted stored geometry such as
  `ReedSolomon { k: 0, m: 1 }` is a typed error, never a panic;
  **(S2, key/value agreement)** `decode_owned_entry` takes the `sidx:` key and rejects a
  payload whose `owner` differs from the key's upload id (#692's v2 review defect). An owned
  entry attributed to the wrong session is staged data renewed or reclaimed under the wrong
  identity;
  **(S3, torn shape)** a value carrying exactly one of `owner` / `staged` is rejected under
  **both** a `pending:` and a `sidx:` reading. Both-absent (legacy) and both-present (owned)
  are the only valid shapes — `0016:454-457`;
  **(S4, namespace agreement — the finding three rounds deferred)** an **owned-shaped** value
  (both fields present) read on the `pending:` path is **rejected**, and a **legacy-shaped**
  value under a `sidx:` key is rejected by `decode_owned_entry`. Every one of the four
  `pending:` readers on the base decodes through the generic `metadata::decode` and so cannot
  see the disagreement: `renew_pending` (`metadata.rs:2007`), `live_lease_guards` (`:2043`),
  `write::sweep_expired_leases` (`write.rs:637`) and `gc::expired_pending_chunks`
  (`gc.rs:489`). They must instead read through **one decode entry point per namespace**, so
  that a misfiled owned entry can never be renewed as an ordinary lease — `renew_pending` puts
  the **caller's** re-encoded entry (`metadata.rs:2012`), so today it would silently erase an
  owned entry's `owner` / `staged` rather than refuse it. Leave the mechanism to Do; the
  binding property is that no `pending:` reader accepts an owned shape and no `sidx:` reader
  accepts a legacy one. **The observable this leg owns, stated exactly so it is neither
  under- nor over-scoped:** the `pending:`-path decode returns `Err` rather than `Ok` on an
  owned shape, and the sweeps behave as S5 says. It is **not** a promise that
  `renew_pending` / `live_lease_guards` surface a *typed* ADR-0045 validation error to their
  callers — they return the crate's boxed error (`crates/traits/src/lib.rs:68-74`), and
  re-typing that would change every caller of both. Assert the decode boundary and the sweep
  behaviour; do not assert the reader's error type, and do not touch its callers;
  **(S5, maintenance fails safe)** the two sweeps that scan `pending:` —
  `write::sweep_expired_leases` (`write.rs:629-649`) and `gc::expired_pending_chunks`
  (`gc.rs:483-497`) — **classify and skip** a value they cannot read as an ordinary pending
  entry: it is neither reclaimed nor deleted, and the sweep completes for every other entry.
  ADR-0045 decision 3 (`0045:55-59`): maintenance loops classify, skip and signal, and **GC
  must fail safe** — never reclaim on doubt. A `?`-abort here would turn one misfiled record
  into a stalled sweep, which is strictly worse than the quarantine and is **not** an
  acceptable reading of S4. **Attribution follows each module's existing seam, and no new one
  is introduced:** in `gc.rs` mirror the skip-and-attribute precedents already there — the
  `malformed-placement` skip reason (`gc.rs:309-310`), `emit_malformed` (`:539`),
  `emit_unresolvable` (`:563`); `write.rs` has **no** `tracing` seam today (zero call sites),
  so `sweep_expired_leases`' obligation is the skip itself — do not add a logging seam to that
  module to satisfy this leg;
  **(S6, placement length decodes)** a `sidx:` value whose `staged` placement length does not
  match its scheme's fragment count **decodes successfully**. That is the whole claim — a
  decode-boundary assertion, provable by this child's pure test. Placement length is the
  standing *contextual* check (ADR-0045 `:45-49` and its `ChunkRef` row `:72`,
  `AGENTS.md:146-149`, `0016:416-432`), and S1 validates the scheme's **geometry**, never the
  placement's **length**. **Do NOT extend this leg into a claim about GC quarantine**: the
  custodian's staged-reference build does not read `sidx:` yet (`gc.rs:483-497` scans only
  `pending:`) and the first `sidx:` writer is #656–#659, so nothing this child ships could
  demonstrate quarantine. That over-claim was withdrawn at the v3 re-plan and must not return;
  **(S7, serialization identity — legacy)** a legacy `pending:` value carrying neither new
  field re-encodes **byte-identically**, and the `skip_serializing_if` that makes it so is
  asserted, not assumed. **Why it matters, stated correctly** — the mechanism was wrong in
  earlier briefs and the difference decides what the test asserts: the `pending:` path does
  **not** use `require(key, encode(prior))`. `renew_pending` preconditions on the **raw bytes
  it read** and then puts the caller's re-encoded entry (`batch.require(key, current).put(key,
  encode(entry))`, `metadata.rs:2012`), and `live_lease_guards` pushes those same raw bytes
  (`:2047`). So a non-identity re-encode does **not** wedge those CASes on a permanent
  `Conflict` — it lets the CAS **win** and silently rewrite the record, dropping a field
  durably with no error anywhere. That is the worse failure and the one to assert against.
  `require(key, encode(prior))` IS the shape on the `inode:` path (`metadata.rs:1794`, `:1919`;
  ADR-0047:38-50) — do not transplant it. `0016:475-485` mis-describes the current code on
  exactly this point; trust the code, and say so in the doc comment;
  **(S8, serialization identity — owned)** an owned value carrying both fields re-encodes
  byte-identically too, across a lease renewal that changes only `lease_expiry_millis`
  (`0016:479-485`);
  **(S9, cross-crate mintability)** **the pairing rule must be reachable and enforceable from
  outside `wyrd-core`.** The first `sidx:` writer (#656–#659) lives in another crate, and
  in-crate call sites build the struct literal directly today (`write.rs:207`, `:433`,
  `:494`), so a rule that exists only as a `pub(crate)` check lets an external producer encode
  a torn value — bytes that **nothing can read back**, since both `metadata::decode` and
  `decode_owned_entry` reject them: a producer writing an obligation its own drain would
  refuse forever. The binding property is that an external crate has a **checked** way to
  build or validate an owned entry and is not obliged to hand-assemble a literal and hope.
  Mechanism is Do's — a public checked constructor pair, a public pairing validator, or making
  the two fields non-independently settable all satisfy it. **Whichever it picks decides the
  ripple's shape, and both shapes are inside the mechanical budget below:** `owner: None,
  staged: None` initializer lines if the fields stay public, or a switch to the constructor
  call if they do not. Assert the property by exercising it from a test outside
  `crates/core/src/` (the `crates/custodian/tests/gc.rs` leg is already such a site);
  **(S10, docs currency)** the living architecture doc's multipart sentence
  (`docs/design/architecture/05-building-block-view.md:202`) gains the `sidx:` namespace and
  `PendingEntry`'s two optional ownership fields, in the voice and length of the ADR-0047
  optional-inode-fields bullet at `:187-194`. **Read that sentence on the base before writing**
  — child-1 lands beneath this child and extends the same sentence with the `retire:`
  namespaces. Add only what this child introduces; do not restate the proposal, and change
  nothing else in that file. `AGENTS.md:154-158`: a merge requirement, not a follow-up. Bring
  the `multipart.rs` module header's key table and its "nothing here is written yet" section
  up to date with what this child landed, as #715/#716 each did — child-1 has already
  corrected that header's stale "the living doc gains these namespaces with the slice that
  first persists one" clause (`multipart.rs:63-73` on today's base), so do not restore it.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier, as its siblings. `C4-verify`
  classifies on **added test files** (`run-verify.sh:142`, `:269-273`), so
  `ADDED_TEST crates/core/tests/multipart_owned_staging.rs` is the discriminator and
  `cargo test -p wyrd-core --test multipart_owned_staging` is the GREEN leg; that path is not
  cfg-gated (`run-verify.sh:148-154`), so it compiles and runs under the gate's own
  invocation. The RED leg reverts production, the test fails to **compile**, and the gate
  reports **UNVERIFIABLE (exit 77)** — **EXPECTED and PRE-DECLARED** as a §6 sign-off item.
  **What Do MUST capture instead (binding): EIGHT isolating negations, and this list is the
  authority for that count** — S1 (drop the `supported(k, m)` check), S2 (drop the key/owner
  comparison), S3 (force the pairing check to `Ok`), S4 (let one `pending:` reader fall back to
  the generic decode), S5 (make the sweep `?`-abort instead of skipping — the "sweep completes
  for every other entry" leg must fail), S7 (remove ONE `skip_serializing_if` attribute), S8
  (same, on the owned witness), and S6 **negated the other way** (make the length-mismatched
  placement reject — the assert-it-decodes leg must fail). For each: apply the negation, run
  the test, paste the failing output into `build-notes.md`, revert. **Each negation must
  ISOLATE its rule** — exactly one test fails. A leg green under its own negation is not
  load-bearing and must be rewritten. S9 is negated by construction rather than by deletion:
  show that the torn literal the negation permits is refused by both decoders.
- **Invariant to restore:** ADR-0045 decision 1, **parse-don't-validate at decode for
  structural invariants** (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`,
  invariant table `:69-74`, and decision 3's liberal-read / strict-maintenance asymmetry at
  `:55-59`), applied over this child's category: **a lease-bearing staging record's fields may
  not disagree with each other OR WITH THE NAMESPACE THAT NAMES THEM, and the disagreement
  must surface as an error at every reader, never as a value.** This is deliberately stated
  across **both** namespaces, not just the new one: `sidx:` and `pending:` are two key spaces
  sharing one value shape by design (`0016:442-457`), so an invariant satisfied by validating
  only the new `sidx:` decoder is the narrow symptom-sentence — it leaves the four live
  `pending:` readers accepting an owned entry as an ordinary lease, which is how a structural
  invariant becomes a convention. What each disagreement costs: an owned entry read as an
  ordinary lease has its ownership erased on the next renewal (`metadata.rs:2012`) or its
  fragments reclaimed by an expiry sweep that believes it holds an abandoned write; a torn
  value is a record nothing can read back; untrusted `EcScheme` geometry that decodes is the
  #285 panic class made durable.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Ordering note:** Wave 1 — terminal. `Depends on: child-1` is a genuine build-on:
  `decode_owned_entry(key, bytes)` mirrors the module's first key-taking decoder, which
  child-1 introduces (every decoder on the base is value-only —`decode_session_record`,
  `multipart.rs:1828`), and both children grow the same `RecordError` enum (`:96`); it also
  wave-serialises the two files they share, `crates/core/src/multipart.rs` and the one sentence
  of `docs/design/architecture/05-building-block-view.md:202`. **This child alone carries the
  chain's external conflicts, and they cannot be declared here** (proposal ordering fields may
  only name sibling labels): after `--accept`, add `- **Conflicts with:** 721, 722` to this
  child's materialised `brief.md`. #711 was split into **#721** (the placement primitive in
  `crates/core/src/metadata.rs` + its repair caller) and **#722** (the drain caller +
  `crates/dst/tests/custodian.rs`); this child edits both files, so it must never share a wave
  with either. **#693** (and **#655** behind it) currently declare `Depends on: 717` and must be
  repointed at this child. **Cite `metadata.rs` by symbol, not by line number** — #721 may land
  in that file first.
- **Surfaces:** data
- **Difficulty:** high — 13 files across 5 crates plus a rendered docs file, and the only child
  of this chain that leaves `crates/core/src/multipart.rs`. What a diff-reviewer must hold in
  view is the cross-crate reach: a struct on the **live `pending:` write path** gains two
  persisted fields and **drops `Copy`** (forced — `UploadId` is a `String` newtype), and it is
  the `Copy` removal, not the fields, that forces the mechanical ripple; and two live
  maintenance sweeps change how they treat a record they cannot read.
- **Scope (one logical fix) / out of scope:** exactly four substantive files, one new test,
  one docs sentence and seven mechanical ripple files — **13 in total, all named here.**
  Substantive: (1) `crates/core/src/multipart.rs` — `StagedPlacement`, the owned entry,
  `decode_owned_entry(key, bytes)` and their `RecordError` variants, reusing the base's
  `parse_sidx_key` (`:925`), `EcSchemeWire` (`:2029`) and `checked_chunk_scheme` (`:1937`);
  (2) `crates/core/src/metadata.rs` — `PendingEntry` gains the two optional fields, drops
  `Copy`, gains the torn-shape rejection and the namespace-scoped decode entry point S4
  requires, which `renew_pending` (`:2007`) and `live_lease_guards` (`:2043`) then read
  through; the in-file constructor at **`:3420`** gains the same mechanical initializer as the
  external ripple sites — that ninth site is **in-file and expected**, not a STOP condition;
  (3) `crates/core/src/write.rs` — three `PendingEntry` literals (`:207`, `:433`, `:494`) and
  `sweep_expired_leases`' decode + skip (`:629-649`); (4) `crates/custodian/src/gc.rs` —
  `expired_pending_chunks`' decode + skip and its audit signal (`:483-497`, mirroring `:539`
  / `:563`). New test: `crates/core/tests/multipart_owned_staging.rs`. Docs: the one sentence
  at `05-building-block-view.md:202`. **Mechanical ripple** — per S9's chosen mechanism,
  either `owner: None, staged: None` initializer lines or a switch to the checked constructor,
  plus clone-instead-of-copy fixes; **nothing else**, ≤ 8 changed lines per file, no logic
  change, no new function — in the seven files that construct a `PendingEntry`:
  `crates/core/tests/mutation_regressions.rs`, `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs`. `crates/custodian/tests/gc.rs` additionally gains
  S5's custodian-side leg (the misfiled entry is skipped, not reclaimed, and the sweep
  completes) — that is the one ripple file allowed a substantive hunk, and it is named here so
  it is not mistaken for scope creep. Budget: ≤ **1,250** added semantic lines
  (`multipart.rs` ≈ 400, `metadata.rs` ≈ 130, `write.rs` ≈ 30, `gc.rs` ≈ 35, new test ≈ 560,
  ripple ≈ 40 mechanical, `custodian/tests/gc.rs` leg ≈ 35, docs ≈ 20). A **fourteenth** file,
  or a non-mechanical hunk in a ripple file other than `crates/custodian/tests/gc.rs`, means
  the seam is wrong: STOP and hand back. Keep every hunk in `metadata.rs` and
  `dst/tests/custodian.rs` as small as briefed — a wider hunk is needless rebase surface for
  #721/#722. / **out of scope:** every `retire:` concern (child-1's); any writer of a `sidx:`
  record, any store call, `async fn` or `WriteBatch` beyond the two existing sweeps' skip
  handling (#656–#659); `crates/custodian/src/` beyond `gc.rs`'s two-sweep change — the
  staged-reference build, restore's `pending_chunks` scan and the drain are untouched; the
  outcome enums, answer table and digests (#693); knob values (#655); reaper/windows (#625);
  every `docs/design/` file except the one `05-building-block-view.md` sentence — ADRs,
  proposals and specs untouched (INTEGRATION §2 immutability), including `0016:475-485` where
  it mis-describes `renew_pending` (record the correction in the code's doc comment, do not
  edit the proposal).
- **Reproduction:** n/a — new functionality on a merged base, built on child-1's accepted
  result. The one live-path touch is `PendingEntry` and the two sweeps that read it: S7 proves
  the legacy path re-encodes byte-identically, and S5 proves the sweeps still complete over
  every readable entry, so the extension is inert for every record written today. Verify the
  gap on the base with `git -C ../wyrd grep -n "OwnedEntry\|StagedPlacement" origin/main --
  crates/` (no hit) and `git -C ../wyrd show origin/main:crates/core/src/metadata.rs | sed -n
  '1554,1560p'` (`PendingEntry` carries only `lease_expiry_millis`).
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`,
  `cargo-mutants` — all registered `[[doctor.checks]]` ids; `docs-renderer` is load-bearing
  here (S10 edits a rendered architecture doc), the rest warn-skip locally while CI enforces
  them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions and two
  existing sweeps, no runtime, no Docker, no new crate, no `Cargo.toml` / `Cargo.lock` change.
- **Test file:** `crates/core/tests/multipart_owned_staging.rs` — a **NEW** file, not
  optional: `C4-verify` classifies on added `*/tests/*.rs` (`run-verify.sh:142`, `:269-273`),
  so an appended test would degrade the gate to its green-only branch and prove nothing. It
  carries S1–S4 and S6–S9, mirroring `crates/core/tests/multipart_session_records.rs` (#716):
  pure, hand-authored JSON bytes, no store and no async, every witness **decoded rather than
  constructed**, both surfaces asserted to agree, and identity asserted file-wide through a
  `decode_both`-shaped helper (`:169`). S5's core half (`write::sweep_expired_leases`) is
  reachable from this file over an in-process store — mirror the harness
  `crates/core/tests/stream_lease_lapse.rs` and `crates/core/tests/stream_lease_renewal.rs`
  already use to drive that same sweep, rather than inventing a second one; **S5's custodian half ships as an added leg in the existing
  `crates/custodian/tests/gc.rs`** — it needs the custodian crate, so it cannot live in the
  discriminator file. That is deliberate and pre-declared: the added leg is outside
  `C4-verify`'s discriminator set, and its evidence is the S5 negation in `build-notes.md`
  plus the gating `C4-ci` run.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as its siblings. The
  UNVERIFIABLE RED is pre-declared above so C2/C4 land as known sign-off items. Everything
  built IS exercised at Check: the named test and the `crates/custodian/tests/gc.rs` leg under
  the gating `C4-ci` (`cargo xtask ci`), plus `C5-mutants` on the bundle diff, plus the eight
  demonstrated negations in `build-notes.md` standing in for the flippable red. Nothing is
  deferred to an off-Check environment — every leg is observable from a pure test or an
  in-process sweep over a test store.
- **Production reach:** this is the **one** live-path touch in the whole #692 chain, so it is
  declared rather than left to be discovered. (a) What honours the seam now: for the two new
  fields, the **legacy shape itself** — both absent — which is every record on disk today, and
  S7 proves that path re-encodes byte-identically; for the owned shape, hand-authored values
  in the named test. The `pending:` guard (S4/S5), by contrast, is **not** a test-double seam:
  it changes what four live readers do, and both sweeps traverse it in production from the
  moment it lands. (b) Where the production wiring lands: the first writer of a `sidx:` record
  is the store round trips, #656–#659 (`multipart.rs:63-73`); nothing writes `owner` / `staged`
  before then. (c) The doubles are load-bearing, not scaffolding: each of the eight negations
  makes exactly one leg fail.
- **Citations expected:** cite `path:line` on the base this actually builds on (child-1's
  accepted result folded onto `origin/main`), and in `metadata.rs` cite by **symbol** — #721
  may land there first. Sources Do MUST open: `0016:353` (the `sidx:` row — what the value
  carries, who writes it, who deletes it, and why no global scan sees it), `0016:442-457` (the
  two additive optional fields and their exact serde spelling), `0016:459-473` (why the
  placement is on the record), `0016:475-491` (the `skip_serializing_if` identity argument —
  read it **with S7's correction in hand**: it is right that the property is load-bearing and
  wrong about the mechanism), `0016:416-432` (the placement-length contextual boundary S6 must
  not cross), ADR-0045 (`:42-49`, `:55-59`, `:69-74`), ADR-0047:38-50.
  Peer callsites Do MAY open and SHOULD mirror rather than re-derive:
  `crates/core/src/multipart.rs:1937` (`checked_chunk_scheme` — S1's predicate, already
  written); `:2029` (`EcSchemeWire`, the closed nested wire shape to read the scheme through,
  with the doc explaining why closure is required for identity); `:1828` (`decode_session_record`
  — the attributed per-record decode wrapper shape); child-1's `decode_retire_obligation` (the
  key-taking variant of that shape, one wave below); `crates/core/src/metadata.rs:1377`/`:1426`
  (`InodeRecord`'s `#[serde(try_from = "InodeRecordWire")]` and that wire struct) and
  `:1121`/`:1240` (`SegmentRecord` and its hand-written `Deserialize`) — the two
  validation-inside-`Deserialize` precedents; `metadata.rs:1990-2015`
  and `:2032-2050` (`renew_pending` / `live_lease_guards` — the two `pending:` readers S4
  rewires, and the code that proves S7's mechanism); `crates/core/src/write.rs:629-649`
  (`sweep_expired_leases`); `crates/custodian/src/gc.rs:483-497` (`expired_pending_chunks`) with
  `:309-310`, `:539` and `:563` (the skip-reason and audit-signal precedents S5 mirrors);
  `crates/core/src/erasure.rs:120` (`supported(k, m)`);
  `docs/design/architecture/05-building-block-view.md:202` (the sentence S10 extends) and
  `:187-194` (the ADR-0047 bullet whose voice and length to match).
  **Salvage:** `$PDCA_HARNESS_ROOT/results/issue_717/iteration-v3/patch.diff` (the path is
  relative to the HARNESS repo, not `$PDCA_WORKTREE`) holds a working `StagedPlacement`,
  `OwnedEntry`, `decode_owned_entry` and `metadata.rs` hunk whose torn-shape rejection and
  byte-identity legs the adversary pass independently re-derived and could not refute. Take
  them, then add what v3 deferred: S4's namespace agreement across the four `pending:` readers,
  S5's fail-safe sweeps, and S9's cross-crate mintability. Do **not** re-ship v3's
  `sidx:`-only reading of the invariant — that is the finding this child exists to close.
- **Prior-art check (triage cycles):** verified at Plan against `origin/main` @ c824243:
  `git -C ../wyrd grep -n "OwnedEntry\|StagedPlacement" origin/main -- crates/` returns
  nothing; `PendingEntry` (`metadata.rs:1556`) carries only `lease_expiry_millis` and is
  `Copy`; the `sidx:` key half exists (`multipart.rs:907-944`) and the value half does not.
  `git -C ../wyrd log --oneline origin/main -- crates/core/src/metadata.rs` shows the last
  multipart-adjacent commit is #710's ceiling work (`d2609b2`), not a `PendingEntry` change.
  Open PRs over the shared files: **#721 and #722** (both `crates/core/src/metadata.rs`, #722
  also `crates/dst/tests/custodian.rs`) — hence the conflict declaration in the ordering note.
  Closed / rejected work over this material: #654's two archived attempts, #692's two, and
  #717's own three — the batch review's `PendingEntry`-under-`pending:` blocker is leg S4 here
  and the adversary's cross-crate-mintability finding is leg S9; neither is a suggestion.
- **Disposition hint:** new-feature
<!-- pdca:end child-2 -->
