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
- **Depends on:** 771
- **Conflicts with:** 721, 722
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
