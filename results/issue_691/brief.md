# Brief — multipart key grammar + validated identity types (654 split 1/3)

> Sub-issue of #654 (itself slice 1 of 7 of #636), split per its 2026-08-05 sign-off.
> **The design is settled and normative:** proposal **0016**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on `origin/main` @
> `339da46` (merged by PR #627). Do MUST read: §1 the records `0016:333-527` (for the key
> column only — the record *values* are the next child's), the `retire:` `<token>` grammar
> `0016:359-380`, the `sidx:` disjoint-staging rule `0016:475-491`. Pure code, no store
> I/O. Material is **salvaged** from `results/issue_654/iteration-v2/patch.diff`, not
> re-derived.

- **Slug:** multipart-key-grammar
- **Defect / goal:** nothing multipart exists in `crates/core`. This child lands the
  protocol's **key space and the validated types it is spelled in**: the module
  `crates/core/src/multipart.rs` with `RecordError`, the identity newtypes (`UploadId`,
  `AttemptId`, `PartNumber`, `SlotIndex`, `Digest` + `hex_lower`), every key prefix, the
  canonical fixed-width key constructors / range prefixes / parsers for `mpuctl`,
  `mpu:<id>`, `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`,
  `sidx:<id>:<part>:<chunk>`, and the `retire:bytes:<token>` / `retire:records:<token>`
  key grammar. After this child, every key the protocol will ever write has exactly one
  spelling, and a non-canonical spelling parses as no key at all.
- **Success criterion:** the NEW file `crates/core/tests/multipart_keys.rs` passes. Every
  leg is pure — no store, no async, no fixture beyond literals:
  1. **Round-trip + canonical rejection, per keyed class.** For each of `mpu:<id>`,
     `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`, `sidx:<id>:<part>:<chunk>`,
     `retire:bytes:<token>`, `retire:records:<token>`: `parse(key(x)) == x` over a table,
     AND each parser rejects with a typed error: a `+` sign, **every short width including
     the bare `7` and the padded-but-short `007`** (the carried-forward MUST-FIX the v2
     review found missing — `multipart_records.rs:162` in the archived attempt), an
     over-wide spelling, a non-decimal body, an empty upload id, an upload id containing
     `:`, a malformed/truncated key, a trailing component, and non-UTF-8 bytes. Two
     spellings of one record must never both parse.
  2. **Byte-lexicographic order equals numeric order** over a fixed-width series that
     crosses every digit-width boundary (1, 9, 10, 99, 100, …, max), for both the slot
     index and the part number — exactly the property `metadata.rs:270-273` states for
     `SEG_INDEX_WIDTH`.
  3. **No prefix is a prefix of another**, over the full set: the seven above plus the
     pre-existing `inode:`, `dirent:`, `pending:`, `bucket:`, `orphan:`
     (`metadata.rs:30-70`), `seg:`, `seggrp:` (`metadata.rs:293-300`) and
     `desired:dserver:` (`custodian/src/desired_state.rs:33`). The two named near-misses:
     `scan("mpu:")` must not return the `mpuctl` singleton, and `sidx:` must not be
     reachable from `scan("pending:")` (`0016:475-491`).
  4. **The `retire:` token grammar** distinguishes its two token forms (session-scoped vs
     generation-scoped, `0016:359-380`) at parse, round-trips both, and rejects a token of
     neither form.
  5. **The pinned format constants hold:** `PART_NUMBER_WIDTH == 6`,
     `MAX_PART_NUMBER == 999_999`, `SLOT_INDEX_WIDTH == 6`, `MAX_SLOT_INDEX == 999_999`,
     and each parser's width equals its constant.
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier; nothing multipart exists on
  `origin/main` (verified: `git -C ../wyrd grep "mpuctl\|MPU_PREFIX" origin/main -- crates/`
  → no matches). C4-verify classifies this patch `ADDED_TEST
  crates/core/tests/multipart_keys.rs` + `CRATE crates/core` (confirmed by `--classify`
  dry-run on the synthetic file set): the GREEN leg is `cargo test -p wyrd-core --test
  multipart_keys`; the RED leg reverts `multipart.rs`/`lib.rs`, the test then fails to
  compile, and the gate reports **UNVERIFIABLE (exit 77) — EXPECTED and PRE-DECLARED**, a
  §6 sign-off item, not a defect. **The demonstrated red Do MUST capture instead
  (binding):** two named negations, each run against the test with the failing output
  pasted into `build-notes.md`, then reverted — (a) make one fixed-width parser accept a
  short `007` spelling (leg 1 must fail); (b) spell one prefix without its trailing
  separator (`mpu` for `mpu:` — leg 3's near-miss must fail). A leg that stays green under
  its negation is not load-bearing and must be rewritten. No `#![cfg(...)]` in the test
  file (the gate's vacuous-green hazard, `run-verify.sh:445`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an
  acceptable cost** (`docs/principles.md:109`, §6 row *Storage lifecycle / reclamation*,
  `docs/principles.md:137`; `0016:2802-2813`), over this child's category: **the key space
  a lifecycle is addressed by**. A record that can be spelled two ways is a record that can
  be lost: if `slot:<id>:7` and `slot:<id>:000007` both parse, a `require_absent` guard
  admits past its cap and a bounded range scan misses a record that exists — residue
  nothing enumerates, and therefore nothing reclaims. A namespace that overlaps another is
  a namespace that gets swept by it. Canonicality and disjointness are enforced **at
  decode**, not by convention at call sites (ADR-0045).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: no live milestone
  integration branch; verified `git -C ../wyrd rev-parse origin/main` → `339da46`)
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** Wave 0 of this chain. The next two children extend the module this
  child creates — they depend on it and each lands in a later wave. Sibling #655 must be
  re-pointed to depend on the LAST child of this chain (it asserts knob values against this
  child's key-space bounds and edits the same file).
- **Surfaces:** data
- **Difficulty:** medium   (three files, zero existing call-sites — but every later slice
  and five sibling issues build on these key formats, and a wrong width or an overlapping
  prefix is a stored-format migration to undo; rated up for that forward reach)
- **Scope:** ONE new module `crates/core/src/multipart.rs` (flat sibling of `metadata.rs`;
  the workspace has no directory modules) + one `pub mod multipart;` line in
  `crates/core/src/lib.rs`: `RecordError` (typed, `Display`, `Error`), the validated
  newtypes `UploadId` / `AttemptId` (token grammar via `TOKEN_HEX_LEN` =
  `metadata::SEG_NONCE_HEX_LEN`, `is_token` / `require_token`), `PartNumber` /
  `SlotIndex` (validating constructors + `Deserialize` that routes through them),
  `Digest` ([u8; 32], lowercase-hex `Serialize`/`Deserialize`, `hex_lower`), the prefix
  constants (`MPUCTL_KEY`, `MPU_PREFIX`, `SLOT_PREFIX`, `PART_PREFIX`, `PSUM_PREFIX`,
  `SIDX_PREFIX`, `RETIRE_BYTES_PREFIX`, `RETIRE_RECORDS_PREFIX`), `fixed_width_u32` /
  `canonical_decimal` / `split_key`, the key constructors + `*_range` prefixes + `parse_*`
  parsers for all seven keyed classes, `RetireMode` / `RetireToken` /
  `parse_retire_mode` / `parse_retire_key` / `retire_key` / `retire_session_range`.
  **Plan decisions pinned here, not Do's to revisit:** `PART_NUMBER_WIDTH = 6`
  (protocol-neutral format headroom — S3 caps at 10,000 per its wire protocol, Azure block
  blobs at 50,000 committed / 100,000 staged; the gateway seam is protocol-neutral by
  ADR-0046, so the *format* must clear every known protocol ceiling with margin while each
  protocol's cap is enforced at admission as *capacity*, mirroring `SEG_INDEX_WIDTH` vs
  `MAX_ROOT_SEGMENTS`, `metadata.rs:270-321`); `SLOT_INDEX_WIDTH = 6` (0016's clamp
  arithmetic reaches ≈524,288, `0016:1471`). Doc-comment both with this reasoning.
  / out of scope: every record **value** type, `encode_record` / `decode_record` and all
  validating value decoders (child-2's); the outcome enums, answer table and digests
  (child-3's — `Digest` the *type* is here, `sha2` the *dependency* is child-3's, so no
  `Cargo.toml` change in this child); every store round trip (#656–#659); the knob values
  (#655); S3 verbs/XML/status (#508); `crates/core/src/metadata.rs` untouched; no file
  beyond the three named.
- **Budget:** ≤ 1,150 added semantic lines total (module ≈ 600, test ≈ 550) across exactly
  **3 files**: `crates/core/src/multipart.rs` (new), `crates/core/src/lib.rs` (one line),
  `crates/core/tests/multipart_keys.rs` (new). A fourth file means the shape is wrong —
  STOP and hand back.
- **Repro instruction:** n/a — new functionality; nothing multipart exists on the base
  (`git -C ../wyrd ls-tree origin/main crates/core/src/` →
  `erasure|lib|metadata|placement|read|repair|write`).
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_keys.rs` — a **NEW** file, not optional:
  C4-verify's discriminator is an added `*/tests/*.rs` (`run-verify.sh:97-98`); a
  co-located test would silently degrade the gate to green-only. Unit tests co-located in
  `multipart.rs` may ship in addition; the five legs live in the named file.
- **Verification posture:** declared — **born-at-tier (posture (a))**. "Red" is criterion
  absence; C4-verify's RED leg is a compile failure reported UNVERIFIABLE (exit 77 → §6),
  pre-declared above. Everything this child ships is built AND exercised at Check: every
  leg runs under `cargo test -p wyrd-core --test multipart_keys` and under gating C4-ci
  (`cargo xtask ci`). The two named negation demonstrations in `build-notes.md` replace
  the flippable red.
- **Production reach:** N/A — no production consumer by design; consumers are the sibling
  children and #656–#659, all separately filed.
- **Citations expected:** cite `path:line` on `origin/main` @ `339da46` for every change.
  Sources Do MUST open: 0016 sections in the header; ADR-0045 (parse-don't-validate);
  ADR-0046 (disjoint prefixes / protocol-neutral seam). Peer callsites Do MAY open and
  should mirror: `crates/core/src/metadata.rs:270-300` (`SEG_INDEX_WIDTH`,
  `MAX_SEGMENT_INDEX`, why the key space is enforced at decode);
  `crates/core/src/metadata.rs:1230-1300` (`seg_key`/`seg_range_prefix`/`parse_seg_key` —
  the house shape for constructor + range + parser + typed error). **Salvage — the primary
  lever:** `results/issue_654/iteration-v2/patch.diff`, sections `multipart.rs` lines
  ~86–845 of the added file (RecordError through the retire key grammar): take them,
  change `PART_NUMBER_WIDTH` from 5 to the pinned 6, add the missing short-width
  (`007`-class) rejection to every fixed-width parser's test table, and leave every record
  value type behind for child-2.
- **Prior-art check (triage cycles):** by affected file path over merged history and
  closed/rejected work, verified at this Plan: `crates/core/src/multipart.rs` and
  `crates/core/tests/multipart_keys.rs` do not exist on `origin/main`; no multipart symbol
  anywhere in `crates/` (grepped `mpuctl`, `MPU_PREFIX`, `mod multipart` → none); no open
  PR touches these paths. Closed/rejected: the #508 line (7 attempts, rejected on
  reviewability), #636 (3 rounds, discontinued for size with the explicit split
  instruction), #654's own two archived attempts (`iteration-v1/`, `iteration-v2/` — the
  second is this child's salvage; its recorded defects are distributed across the three
  children's binding legs).
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add a digit-bearing `Digest::from_hex` round trip and cover `AttemptId::as_str`—five independently reproduced survivors show the claimed validated-identity evidence can miss broken numeric-nibble decoding and a broken public accessor (`crates/core/tests/multipart_keys.rs:110`).; T4 Contribution — Human must determine whether the unavailable `scripts/review-branch --bundle` blocker is the same as C5—the supplied gate summary reports one blocker but omits its report/tool, so a distinct contribution finding cannot be ruled out; affected-path prior art itself was mechanically clear.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 134 mutants tested in 3m: 5 missed, 46 caught, 83 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Human must accept criterion absence as the born-at-tier red oracle — the base has no test target, while the applied patch passes all 19 focused tests, both required negations, mutation testing, and `cargo xtask ci` after relocating only cargo-deny's read-only host lock to scratch; deterministic `run-verify` therefore remains pre-declared UNVERIFIABLE despite independently green verification (`crates/core/tests/multipart_keys.rs:1`).; T4 Contribution — Human must accept the supplied contribution-gate summaries — `scripts/review-branch --bundle` and `scripts/pdca contribcheck` are unavailable in the reviewer artifacts/target, so their passes could not be rerun; this matters even though merged history and all closed-PR file lists independently showed no prior work on either new multipart path.. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix `parse_retire_mode` (crates/core/src/multipart.rs:712/729): it accepts truncated or non-UTF-8 keys such as `retire:bytes:` and `retire:bytes:\xff` on prefix match alone, contradicting its own doc-comment's fail-closed promise. Validate the token following the prefix rather than dispatching on the prefix; add the truncated and non-UTF-8 cases to the retire-grammar legs of crates/core/tests/multipart_keys.rs. Both T4 review passes raised this independently and it is the only blocking finding; `review-rejected.md` covers a different, older docs-currency finding and does NOT disposition this one. Nothing is broken end-to-end today — the full decode path (`parse_retire_key`) still rejects these downstream — but a caller that dispatches on `parse_retire_mode` alone (the store slices, #656-#659) would be misled, and this module is a durable format foundation, so the fix belongs here rather than in its consumers. The §6 size backstop's `iterate-plan` recommendation (2 rounds spent, threshold 2) was considered and is explicitly overridden: the finding is bug-shaped and localized to one function, not scope-shaped. Note for the next Check: this is the second iterate-do at the backstop threshold, so if the rebuild again returns fresh implementation-level parser defects elsewhere in the grammar, that is the signal the slicing — not the implementation — is the problem, and the next disposition should be `iterate-plan`. Procedural note: an identical `iterate-do` was decided for this bundle at 12:43 but the driver never recorded it (§9 empty, decision file left unconsumed); this decision is that one re-issued after review, not a new judgement. See issue_681 §10 for the driver defect. This decision was reaffirmed unchanged in the 2026-08-06 sign-off session after a §10 Act candidate was appended (skip/split driver behavior, unrelated to this disposition).
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_691/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
