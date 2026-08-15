# Brief — multipart verb × state answer table + digests (654 split 3/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636), split per its 2026-08-05 sign-off.
> **The design is settled and normative:** proposal **0016** on `origin/main` @ `339da46`.
> Do MUST read: decision 3's lifecycle + verb × state answer table `0016:894-1037` · the
> ETag/fingerprint deferral `0016:3064-3070` + ADR-0047:73-89,112 · tests the slices owe
> `0016:2876-2939`. Pure code, no store I/O. Material is **salvaged** from
> `results/issue_654/iteration-v2/patch.diff`.

- **Slug:** multipart-state-machine-digests
- **Defect / goal:** the records exist (previous children) but nothing answers a verb and
  nothing computes the object's identity. This child lands the **typed outcome
  vocabulary** (`InvalidPart`, `Backpressure`, `Refusal`, `CreateOutcome`,
  `ReserveOutcome`, `UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`,
  `Publication`), **decision 3's verb × state answer table as pure, total functions**, and
  the two digests — `multipart_etag` and `complete_fingerprint` — with `sha2` added to
  `crates/core`. After this child, every later slice answers in this vocabulary and no
  slice invents its own.
- **Success criterion:** the NEW file `crates/core/tests/multipart_state_machine.rs`
  passes. Every leg pure:
  1. **Every reachable decision-3 cell is answered by a pure function with a typed
     outcome.** Table-test the matrix at `0016:969-978` — `{UploadPart,
     CompleteMultipartUpload, AbortMultipartUpload, ListParts, ListMultipartUploads}` ×
     `{Open, Completing, Aborting, Completed(tombstone), absent}` = **25 cells**, each
     asserted as its typed outcome, never "an error" and never an HTTP status (the
     status/XML mapping is #508's). The two conditional cells get both branches: a
     `Completed` tombstone answers success-with-recorded-ETag **only** on a
     `complete_fingerprint` match, and not-found otherwise (`0016:898-908`). A helper
     enumerates the full product and fails on any unanswered cell, so a later verb or
     state cannot be added silently.
  2. **`multipart_etag` is the settled composition, proved against an independent
     oracle:** `lowercase_hex(SHA-256(d₁ ‖ … ‖ d_N)) + "-" + N` over the **raw 32-byte
     digests** in part-number order, `N` the named count — never MD5 (ADR-0047:73-89
     closed the basis; `:112` and `0016:3064-3070` deferred only the composition to
     here). The test computes the expectation itself from the digest bytes. Discriminating
     cases: N=1; a strict subset differs from the full set; hex-text concatenation differs
     from raw bytes; the `-N` suffix is the named count; **a non-ascending or duplicate
     part-number list is a typed error — never silently sorted** (the carried-forward v2
     finding at `multipart.rs:1903`: sorting erased request order; 0016 makes ascending
     part numbers a Complete *validation*, `0016:707`, `0016:994`, so the pure functions
     receive an already-ascending list or refuse).
  3. **`complete_fingerprint` distinguishes an identical retry from a different
     assembly** (`0016:898-908`): identical ascending lists agree; one changed digest
     disagrees; same digests under different part numbers disagree; a strict subset
     disagrees; a non-ascending or duplicate list is the same typed error as leg 2 —
     canonical order **is** the request order, and the request must be ascending (pinned).
  4. **`MultipartEtag` decode is validating:** parsing rejects a count suffix of 0, a
     count above `MAX_PART_NUMBER`, and a malformed hex or suffix — the count-vs-keyspace
     relational check the v2 review found missing (`multipart.rs:1844`).
  5. **The outcome enums are exhaustive:** no `#[non_exhaustive]` on any public outcome
     enum — assert by matching each without a wildcard arm in the test. (Pinned at Plan:
     every consumer is in-workspace (`Cargo.toml:40` `publish = false`); a new outcome
     variant MUST break every gateway wire-mapping table at compile time rather than fall
     into a `_ =>` arm that maps it to a silently wrong status — reliability over compile
     convenience, the human's explicit call, doubly so with multiple protocol gateways
     planned.)
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier. C4-verify classifies
  `ADDED_TEST crates/core/tests/multipart_state_machine.rs` + `CRATE crates/core`
  (`--classify` dry-run confirmed); GREEN leg `cargo test -p wyrd-core --test
  multipart_state_machine`; RED leg fails to compile → **UNVERIFIABLE (exit 77), EXPECTED
  and PRE-DECLARED** §6 item. **Demonstrated red Do MUST capture (binding):** four named
  negations, output pasted into `build-notes.md`, then reverted — (a) answer one
  `Completing` cell as if `Open` (leg 1 must fail); (b) concatenate hex text instead of
  raw digest bytes (leg 2); (c) ignore part numbers in the fingerprint (leg 3); (d) sort a
  non-ascending list instead of refusing it (legs 2/3). A leg green under its negation
  must be rewritten. Builds on the previous child's folded result (wave merge), so the
  patch applies on a base already carrying the grammar and records.
- **Invariant to restore:** **C-1** (`docs/principles.md:109`, `:137`; `0016:2802-2813`),
  over this child's category: **every cell of the answer table has an answer, and the
  convenient answer is never a silently wrong one**. An unanswered verb × state cell is a
  state a client cannot leave; the cell 0016 spends most words on (`Completed` + a
  fingerprint mismatch) is one where the convenient answer tells a client *its* assembly
  succeeded while the store holds another — a silent wrong answer, worse than any error.
  Exhaustive enums extend the same rule to time: a future outcome must be answered
  deliberately at every consumer, not defaulted.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; base verified `339da46`)
- **Depends on:** 717
- **Conflicts with:**
- **Ordering note:** **Repointed 2026-08-09: was `Depends on: 692`.** #692 was SPLIT into
  **#715** (`Budget`/`AdmissionRecord` + the record codec envelope) → **#716** (the `mpu:`/
  `slot:`/`part:`/`psum:` lifecycle records) → **#717** (`sidx:` staging, `retire:*`, the
  `PendingEntry` extension), so the record family this child builds on is complete only
  after **#717**, the terminal child — hence the repoint. Leaving `692` here would have
  named a bundle marked `split`, which can never go COMPLETE, and under `auto_merge =
  false` `_runnable` gates every `Depends on` on `merged.is_merged`, so this child would
  have been held forever. It builds on the record family: the answer table is a function of
  #716's `SessionState`/`Completion`, the digests take #691's `PartNumber`/`Digest`, and
  every child extends the same `multipart.rs` — the chain wave-serialises the file. Final
  wave of this chain; #655 depends on THIS child (it appends to the same module and asserts
  against its siblings' surfaces).
- **Surfaces:** data
- **Difficulty:** medium   (four files, one crate, zero existing call-sites — low
  blast-radius today, but the outcome enums and the two digests are the durable contract
  #508 and four later slices map to wire and store; rated up for that forward reach)
- **Scope:** extend `crates/core/src/multipart.rs` with the outcome enums, `Verb`, the
  `*Answer` types, the per-verb answer functions + the total `answer` dispatcher,
  `canonical_named_parts` (validates ascending/duplicate-free, **refuses** otherwise),
  `MultipartEtag` (validating parse/serde per leg 4), `multipart_etag`,
  `complete_fingerprint`; add `sha2.workspace = true` to `crates/core/Cargo.toml` with a
  doc comment recording it is not a new dependency decision (`sha2 = "0.11"` is already a
  workspace dependency at `Cargo.toml:147`, used by `gateway-s3`/`server`, inside the
  `deny.toml` allowlist — ADR-0003's audit is not re-opened; `Cargo.lock` updates
  mechanically). **Plan decision pinned:** all public outcome enums exhaustive — no
  `#[non_exhaustive]` (rationale in leg 5).
  / out of scope: any store round trip (#656–#659); the S3 status/XML mapping (#508 — this
  child names no HTTP status); the knob values (#655); reaper/windows (#625);
  `metadata.rs`, `lib.rs`, `write.rs`, `custodian/` untouched; `docs/design/` untouched.
- **Budget:** ≤ 950 added semantic lines total (module extension ≈ 350, test ≈ 550) across
  exactly **4 files**: `crates/core/src/multipart.rs`, `crates/core/Cargo.toml`,
  `Cargo.lock` (mechanical), `crates/core/tests/multipart_state_machine.rs` (new).
- **Repro instruction:** n/a — new functionality; only the grammar and records (previous
  children) exist on the base.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; `cargo-deny` in particular re-runs the ADR-0003 wall over the `sha2` addition to `crates/core` (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no crate new to the workspace.
- **Test file:** `crates/core/tests/multipart_state_machine.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator; `--classify` dry-run
  confirmed). The five legs live here; co-located unit tests may ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as its siblings:
  UNVERIFIABLE RED pre-declared, everything built is exercised at Check under the named
  test + gating C4-ci, and the four negation demonstrations in `build-notes.md` replace
  the flippable red.
- **Production reach:** N/A by design — the consumers are #508 (wire mapping) and
  #656–#659 (store slices), all separately filed; nothing existing changes behaviour.
- **Citations expected:** cite `path:line` on the target base for every change. Sources Do
  MUST open: `0016:894-1037` (read the failure-mode tables in full — each enumerates the
  ways to implement a cell wrong); ADR-0047:73-89 + `:112`. Peer callsite Do MAY open:
  `crates/gateway-s3/src/crypto.rs:21-60` (the in-tree `sha2` usage — `Digest`, `Sha256`,
  the hex helper — so `crates/core`'s use matches the workspace's). **Salvage — the
  primary lever:** `results/issue_654/iteration-v2/patch.diff` — take the outcome enums,
  answer functions, `MultipartEtag` and the two digest functions (added-file lines
  ~1819–2349); then fix the two recorded defects: `canonical_named_parts` must refuse
  rather than sort (leg 2/3), and `MultipartEtag` decode must validate its count (leg 4).
- **Prior-art check (triage cycles):** verified at this Plan against `339da46`: no
  outcome/answer/digest symbol on `origin/main` (grepped `multipart_etag`,
  `complete_fingerprint`, `CompleteOutcome` → none); no open PR touches these paths.
  Closed/rejected: #508 line, #636, #654's two archived attempts — the v2 review finding
  at `multipart.rs:1903` (sort-erases-order) is this child's binding refusal leg.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
