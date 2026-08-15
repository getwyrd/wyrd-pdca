# Design proposal — issue 654 / multipart-record-family-and-state-machine

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart commit
> protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on `origin/main`
> @ `339da46` (3,108 lines; merged by PR #627). This brief authors **no new design**: it points at
> the sections that ARE this slice's specification, scopes the slice, settles the boundaries 0016
> leaves to the implementing slice, and states the C4 shape. **Do MUST read these sections before
> writing code** — each carries a failure-mode table enumerating the ways to implement it wrong and
> the observable that catches each:
> §1 the records `0016:333-527` · §2 the state machine `0016:528-602` ·
> Decision 3 lifecycle + the verb × state answer table `0016:894-1037` ·
> the `retire:` `<token>` grammar `0016:359-380` · the knob table `0016:1463-1480` (read for the
> *shape* of the profile tuple only — the **values** are #655's) ·
> the `sidx:` disjoint-staging rule `0016:475-491` · tests the slices owe `0016:2876-2939`.
>
> **Slice 1 of 7 of #636**, per its 2026-07-30 sign-off (*"split issue 636 into smaller,
> independently reviewable slices — record family/state machine vs. drain/reclamation wiring vs.
> admission knob handling"*). **Pure code, no store I/O.** Material is **salvaged** from the
> discontinued #636 patch, not re-derived. Siblings: **#655** (2 — the knob values, depends on
> this), #656 (3), #657 (4), #658 (5), #659 (6), #660 (7).
> Tracker: https://github.com/getwyrd/wyrd/issues/654.

- **Slug:** multipart-record-family-and-state-machine
- **Kind:** enhancement (design proposal)
- **Goal:** the multipart **vocabulary**, in `crates/core`, with **no store I/O and no gateway**:
  the record types and their key grammar, decoders that make an invalid record unrepresentable, the
  typed outcomes the later slices answer with, decision 3's verb × state answer table as **pure
  functions**, and the two digests — `complete_fingerprint` and `multipart_etag`. After this slice a
  reader can name every record the protocol writes, parse every key it uses, decide what any verb
  answers in any state, and compute an object's multipart ETag — all without a `MetadataStore`.
- **Success criterion:** the NEW file `crates/core/tests/multipart_records.rs` passes. Every leg is
  a **pure** test over values — no store, no async runtime, no fixture beyond literals:
  1. **Key grammar round-trips, and non-canonical spellings are REJECTED at decode.** For every
     keyed record class — `mpu:<id>`, `slot:<id>:<k>`, `part:<id>:<n>`, `psum:<id>:<n>`,
     `sidx:<id>:<part>:<chunk>`, `retire:bytes:<token>`, `retire:records:<token>` — assert
     `parse(key(x)) == x` over a table of inputs, **and** that each parser rejects, with a typed
     error: a `+7` sign, a short `7` where the width is fixed, an over-wide `0000007`, a non-decimal
     body, an empty upload id, an upload id containing the separator `:`, a truncated key, a key
     with a trailing component, and a key whose bytes are not UTF-8. Two spellings of one record
     must never both parse — that is what makes a `require_absent` guard and a bounded range scan
     mean what they say. Assert **byte-lexicographic order equals numeric order** over a fixed-width
     index/part-number series (the property the zero-padding exists for), exactly as
     `metadata.rs:270-273` states it for `SEG_INDEX_WIDTH`.
  2. **No prefix is a prefix of another.** Assert over the full set — the seven above plus the
     pre-existing `inode:`, `dirent:`, `pending:`, `bucket:`, `orphan:` (`metadata.rs:30-70`),
     `seg:`, `seggrp:` (`metadata.rs:293-300`) and `desired:dserver:`
     (`custodian/src/desired_state.rs:33`) — that no prefix is a prefix of any other, so no
     `scan` returns a neighbour's records ([ADR-0046][a46], `0016:344-347`). Two near-misses are
     named because they are where this actually breaks: **`scan("mpu:")` must not return the
     `mpuctl` singleton** (the separator is what keeps them disjoint — an implementation that
     spells the prefix `mpu` sweeps the admission ledger into every session scan), and **`sidx:`
     must not be reachable from a `scan("pending:")`**, which is the whole point of the disjoint
     owned-staging class (`0016:475-491`, and restore's bound re-derivation at `0016:486-490`).
  3. **Every record's decode is validating, and encode→decode is the identity on a legacy value.**
     Each record type round-trips through `encode`/`decode`; a structurally invalid value is
     rejected **at decode** with a typed error rather than becoming a value the code must re-check
     (ADR-0045, parse-don't-validate). **The relational `mpuctl` invariant is binding and is a
     carried-forward MUST-FIX** (see below): a decoded admission record whose `max_sessions`
     disagrees with what its own stored `profile` tuple derives is **rejected**, not trusted.
     Assert with a hand-authored torn value. Separately, `PendingEntry`'s two new optional fields
     round-trip **byte-identically** on a legacy `pending:` value that has neither — the
     `skip_serializing_if` identity every `require(key, encode(prior))` CAS in `metadata.rs`
     depends on (`ADR-0047`, `metadata.rs:1368-1391`).
  4. **Every reachable decision-3 cell is answered by a pure function with a typed outcome.**
     Table-test the verb × state matrix at `0016:969-978` — `{UploadPart, CompleteMultipartUpload,
     AbortMultipartUpload, ListParts, ListMultipartUploads}` × `{Open, Completing, Aborting,
     Completed(tombstone), absent}` = **25 cells**, each asserted as its **typed** outcome, never
     "an error" and never an HTTP status (the S3 status/XML mapping is **#508's**; this slice pins
     the typed answers the wire layer maps). The two cells with a condition get both branches: a
     `Completed` tombstone answers success-with-the-recorded-ETag **only** when the request's
     `complete_fingerprint` matches the recorded one, and the not-found outcome when it does not
     (`0016:898-908`). Assert the table is **total** — a helper that enumerates the product and
     fails if any cell is unanswered, so a later state or verb cannot be added silently.
  5. **`multipart_etag` is the settled pure function, proved against an independent oracle.**
     `etag = lowercase_hex( SHA-256( d₁ ‖ d₂ ‖ … ‖ d_N ) ) + "-" + N`, where `dᵢ` is the **raw 32
     binary digest bytes** (not their hex text, no separators, no part numbers mixed in) of the
     *i*-th part in **ascending part-number order over exactly the parts the caller named**, and
     `N` is that count. **Never MD5** — ADR-0047 closed the basis
     (`docs/design/adr/0047-object-metadata-model.md:73-89`: lowercase-hex SHA-256 as an opaque
     change-token) and deferred only the composition (`:112`, `0016:3064-3070`). The test computes
     the expected value **itself** from the digest bytes, so the oracle is independent of the
     implementation's choice. Assert the **discriminating** cases, not just one vector: N=1;
     ascending order is enforced even when the caller supplies part numbers out of order; a
     **strict subset** of the staged parts gives a different value from the full set; hex text
     concatenation gives a different value from raw-byte concatenation (so a hex-vs-raw slip
     fails); and the `-N` suffix is the **named** count.
  6. **`complete_fingerprint` distinguishes an identical retry from a different assembly.** It is a
     digest over the **ordered** `(part_number, digest)` pairs the winning Complete named
     (`0016:898-908`). Assert: identical lists agree; a list with one digest changed disagrees; a
     list with the same digests under different part numbers disagrees; a reordered list of the
     same pairs agrees **iff** the canonical order is part-number ascending (pin which); and a
     strict subset disagrees. This is the rule that stops a client being told *its* assembly
     succeeded while the store holds an earlier one — a silent wrong answer, worse than any error.
- **Falsifiability:** **RED is criterion-ABSENCE — this slice is born-at-tier and there is no
  behavioural red available on any environment.** Nothing multipart exists on `origin/main`
  (`git -C ../wyrd grep -rn "mpuctl\|multipart_etag\|MPU_PREFIX" -- crates/` → **no matches**;
  `crates/core/src/` holds only `erasure|lib|metadata|placement|read|repair|write`). So:
  - **C4-verify will report `UNVERIFIABLE` (exit 77) on its RED leg, and that is EXPECTED and
    PRE-DECLARED.** The gate classifies on an added `*/tests/*.rs` (`run-verify.sh:97`, `:98`);
    `run-verify.sh --classify` on a synthetic patch listing this slice's file set (including the
    `metadata.rs` `PendingEntry` hunk) returned exactly
    `ADDED_TEST crates/core/tests/multipart_records.rs` + `CRATE crates/core`, so the GREEN leg is
    `cargo test -p wyrd-core --test multipart_records` (precise and fast), and the RED leg reverts
    `crates/core/src/*` and `Cargo.toml`, removing every symbol the test names — the target then
    fails to **compile**, which `_red_verdict` correctly reports as *"the discriminator never
    executed"* rather than inventing a verdict (`run-verify.sh:487-497`). It routes to SUMMARY §6
    as a NEEDS-HUMAN item; it is **not** a defect in the patch and is declared here so sign-off
    meets it as a known item.
  - **The demonstrated red Do MUST capture instead (binding, and it answers a recorded prior
    finding).** #636's round-3 sign-off refused a compile-shaped red on exactly this ground:
    *"my clean-base replay ran 0 tests, so it proves criterion absence rather than that the
    assertions are load-bearing."* So for legs **(1), (3), (4), (5), (6)** Do MUST, for each,
    temporarily negate the production code in **one** named way, run the discriminator, and paste
    the failing output into `build-notes.md` — then revert the negation. The five negations are
    named so they cannot be chosen to be easy: (1) accept a `007` spelling in one fixed-width
    parser; (3) drop the `mpuctl` relational check; (4) answer one `Completing` cell as if it were
    `Open`; (5) concatenate hex text instead of raw digest bytes; (6) ignore part numbers in the
    fingerprint. Each MUST make the discriminator fail. A leg that stays green under its negation
    is not load-bearing and must be rewritten before the bundle ships.
  - **No environment is needed and none is missing** — the whole slice is pure functions over
    values on a plain Linux workspace. There is no topology, cfg gate, Docker or live backend
    involved, so this is not the "cannot produce the red" gap the Plan-blocking rule is about; it
    is the born-at-tier case the brief template's posture (a) names.
  - **No vacuous green.** No `crates/core/tests/*.rs` carries a crate-level `#![cfg(...)]` (grepped
    on the base), so the GREEN leg cannot report `0 tests … ok` (`run-verify.sh:445`).
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost**, stated over this slice's category: **the vocabulary a lifecycle is written in**. Sourced,
  not intuited: `docs/principles.md:137` (§6 row *Storage lifecycle / reclamation* — which names
  "or in which a state machine gains a state" explicitly), sourced to §5 C-1
  (`docs/principles.md:109`), the maintainer's rule of 2026-07-25, `0016:2802-2813`, `gc.rs:22-25`.
  A slice that ships no I/O still fixes which failures are **expressible** downstream:
  - **A record that can be spelled two ways is a record that can be lost.** If `slot:<id>:7` and
    `slot:<id>:007` both parse, a `require_absent` guard admits past its cap and a bounded range
    scan misses a record that exists — residue nothing enumerates, and therefore nothing reclaims.
    Canonicality is enforced at decode, not by convention at each call site.
  - **A namespace that overlaps another is a namespace that gets swept by it.** The disjointness of
    `sidx:` from `pending:` is the *only* thing keeping a global expiry sweep from reaping live
    owned staging entries.
  - **Every cell of the answer table has an answer.** An unanswered verb × state cell is a state a
    client cannot leave — and the specific cell 0016 spends most words on (`Completed` + a
    fingerprint mismatch) is one where the *convenient* answer is a silent wrong one.
  - **A stored record's fields may not disagree with each other.** An admission record whose
    `max_sessions` does not match its own `profile` lets a torn or rolled-back value admit sessions
    past the memory bound the reconcile pass is sized for — a fleet-wide failure admitted by one
    unvalidated field.
  - **Structural invariants surface as errors, never as values** (ADR-0045). A capacity constant,
    by contrast, is *not* a decode invariant: rejecting a stored record on a derived capacity number
    would make a durable record unreadable the day the number moves — the boundary
    `metadata.rs:312-321` already draws for `MAX_ROOT_SEGMENTS` and this slice must draw the same
    way (see § Design, boundary 2).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches, and M4's integration branch is merged and deleted. #634 and the whole #635
  slice sequence landed on `main` directly. Verified `git -C ../wyrd rev-parse origin/main` →
  `339da46`.)
- **Depends on:** *(none — #634's `scan_page` seam and #635's segmented-map shape are both merged
  to `main`, and this slice needs neither: it performs no store I/O)*
- **Conflicts with:** *(none in this batch)*
- **Ordering note:** **Wave 0.** No prerequisite to fold. **#655 depends on this** and is scheduled
  into wave 1: it appends the deployment's knob constants to the module this slice creates, so
  building the two on one base would collide in `crates/core/src/multipart.rs`. #681 is the other
  wave-0 bundle and shares no file with this one (`crates/custodian/src/*` vs
  `crates/core/{src/lib.rs,src/multipart.rs,Cargo.toml}`), so the two build in parallel safely.
  #682 (wave 1) also edits `crates/core/src/metadata.rs`, which this slice touches for **exactly**
  the two `PendingEntry` fields (§ Scope) — different region of the file, and #682 builds on this
  slice's merged result, so the wave order resolves it. Keep the hunk that small; a wider one is a
  needless rebase surface for #682.
- **Surfaces:** data
- **Difficulty:** medium   (large in **volume** — a whole record family — but narrow in
  **blast-radius**: two existing files are touched trivially (`crates/core/src/lib.rs` gains one
  `pub mod` line, `crates/core/Cargo.toml` one already-vetted workspace dependency) and **nothing
  existing changes behaviour** — there are zero call-sites, because nothing consumes these records
  yet. The one place effects propagate is forward: five later slices build on this API surface, so
  a wrong shape is expensive to move. Not `low` for that reason; not `high` because a reviewer
  holds one new module, not a cross-cutting change.)
- **Scope:** the **pure** half of the multipart seam, in **one new module**
  `crates/core/src/multipart.rs` (flat sibling of `metadata.rs` — the workspace has **no**
  directory modules anywhere; verified), reached by one `pub mod multipart;` line in
  `crates/core/src/lib.rs`, with `sha2.workspace = true` added to `crates/core/Cargo.toml`:
  - **record types** for `mpuctl`, `mpu:<id>`, `slot:<id>:<k>`, `part:<id>:<n>` with its `psum:`
    summary sibling, `sidx:<id>:<part>:<chunk>` (carrying `owner` **and** `staged`, under a prefix
    **disjoint** from `pending:`), and `retire:bytes:{generation}` / `retire:records:` — the field
    sets exactly as 0016 §1 tables them;
  - **the key grammar and its canonical parsers** — fixed-width, `+7` / `007` spellings rejected at
    decode, byte-lexicographic order equal to numeric order, plus the `retire:` `<token>` grammar
    (`0016:359-380`);
  - **validating decoders** — an invalid record is a decode error, not a value (ADR-0045);
  - **typed outcomes** — the vocabulary the later slices answer with, so no slice invents its own;
  - **decision 3's verb × state answer table as pure functions**, total over the product;
  - **`complete_fingerprint`** and **`multipart_etag`**, with their oracle tests.

  **Boundaries settled here so Do does not re-decide them (Plan decisions, not suggestions):**
  1. **Format constants are this slice's; capacity knobs are #655's.** This slice owns the key
     space — the prefixes, `SLOT_INDEX_WIDTH` / `PART_NUMBER_WIDTH` and the largest index each
     width can address — and enforces them at decode. It does **not** define `MAX_INFLIGHT_PARTS`,
     `MAX_PARTS_PER_SESSION`, `MAX_PART_CHUNKS`, `W_REF` or any other capacity value; #655 picks
     those and asserts each fits its key space. This is exactly the split `metadata.rs` already
     draws between `SEG_INDEX_WIDTH` / `MAX_SEGMENT_INDEX` (format, enforced at decode, `:270-287`)
     and `MAX_ROOT_SEGMENTS` (capacity, deliberately **not** at decode, `:305-318`) — mirror it.
  2. **Therefore no decoder enforces a capacity number.** A `part:` record with more chunks than
     some future `MAX_PART_CHUNKS` still **decodes**; the cap is enforced where the work is
     admitted, in a later slice. Rejecting it at decode would make a durable record unreadable the
     day the constant moves (`metadata.rs:312-321`, ADR-0045's liberal-on-read boundary).
  3. **The admission `profile`'s derivations are pure arithmetic over the stored tuple, and they
     live HERE.** `mpuctl` stores `{count, max_sessions, profile}` where `profile` is the budget
     tuple `(W_ref, MAX_PART_CHUNKS, MAX_PARTS_PER_SESSION, MAX_INFLIGHT_PARTS, MAX_STAGED_CHUNKS)`
     (`0016:348`). `U_ref` and `MAX_SESSIONS = ⌊W_ref / U_ref⌋` are functions **of that tuple**
     (`0016:1469-1470`), not of this deployment's chosen constants — so this slice can and must
     implement them, and with them the relational decode check in leg (3). #655 then supplies the
     deployment's own values and asserts the same relations hold for them. This removes the
     apparent circularity between the two slices; do not resolve it any other way (a placeholder
     constant here would be re-picked in #655 and the two would drift).
  4. **`sha2` is added to `crates/core`, and it is not a new dependency decision.** `sha2 = "0.11"`
     is already a **workspace** dependency (`Cargo.toml:147`) used by `gateway-s3` and `server`, and
     already inside the `deny.toml` allowlist — so ADR-0003's three-test audit is **not** re-opened
     and no new crate enters the tree. Add it with a doc comment saying exactly that. (`crates/core`
     has no SHA-256 today; `chunk-format` carries only crc32c.)

  **Out of scope:**
  - **Every store round trip.** No `MetadataStore` call, no `WriteBatch`, no `async fn` that reads
    or writes — that is #656 (admission + Create/Abort), #657 (UploadPart staging), #658 (Complete),
    #659 (retirement + terminal delete). If a function in this slice needs a store, it belongs to a
    later slice; **hand back rather than reaching for one.**
  - The knob **values** and `knob_clamps_hold` — **#655** (see boundary 1).
  - The S3 verbs, XML bodies, HTTP status/error codes and routing — **#508**. This slice pins typed
    outcomes; it names no status code.
  - The reaper loop and every window-driven exit (`W_open`, `W_session`, `W_completing`,
    `W_tombstone`, the cursor-keyed drain, the clock guard) — **#625**. Record `clock_source` as a
    field; do not implement the clock guard.
  - Operator abort / terminal expiry / foreign-clock alarm — **#633**.
  - The custodian-side protection class for staged bytes — **#637**. In particular **do not** change
    `reconcile_step`'s signature or `GcContext`'s fields, and do not touch `crates/custodian/`.
  - `crates/core/src/{write,read}.rs` — untouched. The supersede/`unlink` retirement routing is
    #659's; the staged write path is #657's.
  - **`crates/core/src/metadata.rs` — ONE allowance, and only this one:** `PendingEntry`
    (`metadata.rs:1528`) gains the two optional fields `owner` and `staged`, because 0016 makes the
    `sidx:` value *be* a `PendingEntry` rather than a parallel type (`0016:475-491`) and this slice
    owns that record. Nothing else in that file may change — no new function, no change to any
    existing CAS, no ceiling helper (**#682** edits this same file in wave 1 and builds on this
    slice's merged result, so a wider hunk here is a needless rebase surface). If the extension
    seems to require more than two `#[serde(default, skip_serializing_if = "Option::is_none")]`
    fields and their doc comments, STOP and hand back.
  - Any file under `docs/design/adr/` or `docs/design/specs/`, any edit to `0016` itself, and any
    conformance-vector change.
- **Budget:** ≤ **1,500** added semantic lines (non-blank, non-comment, non-mechanical) — the
  issue's own budget — across ≤ **5** files: `crates/core/src/multipart.rs` (**new**),
  `crates/core/src/lib.rs` (one line), `crates/core/Cargo.toml` (one dependency),
  `crates/core/src/metadata.rs` (the two `PendingEntry` fields, nothing else),
  `crates/core/tests/multipart_records.rs` (**new**). A **sixth** file means the shape is wrong —
  most likely a store round trip crept in: STOP and hand back. The salvaged module is the lever
  here: the discontinued patch's `crates/core/src/multipart.rs` is 6,317 added lines, of which this
  slice takes only the pure half. Take the types, the grammar, the parsers, the outcomes and the
  digests; leave every `async fn` behind.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — the five doctor.checks ids (pdca.toml :696, :703, :711, :733, :740), all OK on this host at Plan (scripts/pdca doctor). Named because the prose and dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3), and because a cargo-deny older than 0.20.0 hard-fails the gating C4-ci row with a message naming a flag rather than the stale tool. cargo-deny in particular re-runs the ADR-0003 wall over the sha2 addition to crates/core. Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no protoc, no live backend, and no crate new to the workspace.
- **Test file:** `crates/core/tests/multipart_records.rs` — a **NEW** file, not optional.
  C4-verify's discriminator is an **added** `*/tests/*.rs`; a co-located `#[cfg(test)] mod tests`
  would make the gate fall back to `cargo test -p wyrd-core` over the crate's whole suite and take
  the green-only branch (`run-verify.sh:454-464`). Confirmed by the `--classify` dry-run above.
  Unit tests co-located in `multipart.rs` may ship **in addition** (C4-ci runs them); the six legs
  of the success criterion must live in the named file.
- **Verification posture:** **declared, not default — born-at-tier (posture (a)).** "Red" here is
  criterion **absence**: every symbol the discriminator names is introduced by this patch, so no
  prior failing assertion exists to flip and C4-verify's RED leg is a compile failure reported as
  `UNVERIFIABLE` (exit 77 → SUMMARY §6). **What IS built and exercised at Check:** the entire slice
  — it is pure code, and every one of the six legs runs green under `cargo test -p wyrd-core --test
  multipart_records` and again under the gating `C4-ci` (`cargo xtask ci`). Nothing is deferred and
  nothing is scaffolding: there is no later slice that "turns this on". **What replaces the red:**
  the five named negation demonstrations under § Falsifiability, captured in `build-notes.md` — the
  evidence #636's sign-off asked for by name.
- **Production reach:** N/A — this slice has no production consumer *by design*, and that is the
  point of the split (seam (iii) of #508's re-plan: the protocol reviewed without the wire surface
  in the same diff). The consumers are #656–#659; they are separate, already-filed work items, not
  a deferred verification of this one.
- **Citations expected:** cite `path:line` on the target branch for every change. Every line number
  in this brief was verified against `origin/main` at `339da46` during the Plan verification pass.
  **Sources Do MUST open (this slice's design is elsewhere, by INTEGRATION §6):**
  - `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — the sections listed in the
    header block above. **Read them in full**; this brief does not restate them.
  - `docs/design/adr/0047-object-metadata-model.md:73-89` and `:112` — the ETag basis
    (lowercase-hex SHA-256, opaque, **never** MD5) and the deferral of the composition to here.
  - `docs/design/adr/0045-metadata-validation-boundaries.md` — parse-don't-validate, and where the
    liberal-on-read boundary sits.
  - `docs/design/adr/0046-bucket-model-real-namespace.md` — real records under a disjoint prefix;
    the scan-then-commit warning that makes admission a reservation rather than a scan.

  **Peer callsites Do MAY open — mirror them rather than invent a shape:**
  - `crates/core/src/metadata.rs:270-300` — `SEG_INDEX_WIDTH`, `MAX_SEGMENT_INDEX`, the prefix
    constants and the reasoning for enforcing the key space at decode: the exact precedent for
    boundary 1, in the same crate.
  - `crates/core/src/metadata.rs:1230-1300` — `seg_key` / `seg_range_prefix` / `parse_seg_key`: the
    house shape for a keyed record's constructor, its bounded range prefix and its canonical
    parser, including the typed error.
  - `crates/core/src/metadata.rs:312-321` — why `MAX_ROOT_SEGMENTS` is **not** a decode invariant
    (boundary 2).
  - `crates/core/src/metadata.rs:1368-1391` + `docs/design/adr/0047-object-metadata-model.md:38-50` —
    the `Option` + `#[serde(default)]` + `skip_serializing_if` rule that makes decode→encode the
    identity on a legacy record, which leg (3)'s `PendingEntry` assertion pins.
  - `crates/gateway-s3/src/crypto.rs:21-60` — the in-tree `sha2` usage (`Digest`, `Sha256`) and its
    hex helper, so `crates/core`'s use matches the workspace's.
  - **Salvage — this is the primary lever:** `results/issue_636/patch.diff`, the discontinued
    monolithic patch, whose `crates/core/src/multipart.rs` already implements this slice's pure
    half: `MPUCTL_KEY` / `MPU_PREFIX` / `SLOT_PREFIX` / `PART_PREFIX` / `PSUM_PREFIX` /
    `SIDX_PREFIX`, `SLOT_INDEX_WIDTH`, `is_token` / `require_token`, `mpu_key` / `slot_key` /
    `part_key` / `psum_key` / `sidx_key` and their `*_range` / `parse_*_key` companions,
    `fixed_width_u32`, `split_key`, `parse_retire_mode` / `parse_retire_key`, `RecordError`,
    `encode_record`, `decode_session`, `AdmissionRecord`, `SessionState`, `PublishTarget`,
    `Completion`, `SessionRecord`, `SlotRecord`, `PartRecord`, `PartSummary`, the outcome enums
    (`InvalidPart`, `Backpressure`, `Refusal`, `CreateOutcome`, `ReserveOutcome`,
    `UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`, `Publication`), `tombstone_answer`,
    `hex_lower`, `parse_digest`, `digest`, `multipart_etag`, `complete_fingerprint`.
    **Reuse it — but take only those, and fix the one defect its review recorded:** #636's sign-off
    found that `decode_session`/the admission decode accepted `max_sessions != profile.max_sessions()`
    and admission then trusted the inconsistent stored limit, so an oversized torn value could
    violate the `W_ref` bound. That is leg (3)'s binding assertion here. Leave every `async fn`
    (`create_session`, `reserve_slot`, `stage_chunk`, `commit_part`, `upload_part`, `complete`,
    `abort`, `drain_step`, `terminal_delete`, `classification_sweep`, …) and every knob constant
    behind — they are later slices'.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `crates/core/src/multipart.rs` and `crates/core/tests/multipart_records.rs`
  **do not exist on `origin/main`** (`git -C ../wyrd ls-tree origin/main crates/core/src/` →
  `erasure|lib|metadata|placement|read|repair|write`), and no multipart symbol exists anywhere in
  `crates/` (grepped `mpuctl`, `multipart_etag`, `MPU_PREFIX`, `mod multipart` → no matches). So
  there is no merged prior art to duplicate. `git -C ../wyrd log origin/main -- crates/core/src/lib.rs
  crates/core/Cargo.toml` → 5 commits, none multipart-related. No open PR touches these paths
  (`gh pr list --state open` → empty). **Closed/rejected:** the #508 line (seven attempts, rejected
  at sign-off on reviewability — one 44-file / 14,117-line cross-plane patch) and then **#636
  itself**, whose three Do rounds produced `results/issue_636/patch.diff` and were discontinued at
  the 2026-07-30 sign-off **for size, not direction** — with the explicit instruction to split into
  these seven slices. That patch is therefore *salvage*, not a rejected approach: reuse it. Its
  recorded blocking findings (21 batch-review items, the `mpuctl` relational-validation C5, and a
  compile-shaped pre-fix red that proved nothing) are carried into this brief's legs (3) and the
  negation requirement. #647 (the parallel #635 monolith) is unrelated to these paths.
- **Disposition hint:** new-feature

## Motivation

Five later slices — admission and Create/Abort (#656), UploadPart staging (#657), Complete (#658),
retirement and terminal delete (#659), the seeded race cases (#660) — all write, read and answer in
terms of the same records, the same keys and the same outcomes. #508's seven rejected attempts and
#636's three all shipped that vocabulary **inline with the behaviour that uses it**, which is what
made every one of them unreviewable: a 44-file / 14,117-line patch in which the question *"is this
key canonical?"* could not be asked apart from *"does this drain converge?"*. Fixing the vocabulary
first, with no store I/O in the diff, makes each later slice a change to **behaviour** over a
settled alphabet.

It is also where the cheap, permanent failures live. Two spellings of one key defeat a
`require_absent` guard; an overlapping prefix hands a global sweep records that belong to someone
else; an unanswered verb × state cell is a session a client cannot leave; an admission record whose
fields disagree admits past the memory bound the whole reconcile pass is sized for. None of these
is expensive to prevent **here**, and each is expensive to discover in a slice whose diff is about
something else.

## Design

The design is proposal 0016's, unchanged, and this brief does not restate it — see the header
block for the section anchors and § Citations expected for what Do must open. What is settled
**here**, because 0016 leaves it to the implementing slice, is:

1. **The module and its place.** One flat `crates/core/src/multipart.rs`, sibling to `metadata.rs`,
   because the workspace has no directory modules; one `pub mod multipart;` in `lib.rs`. The later
   slices extend this module rather than adding new ones, so the seam has one home.
2. **The format/capacity split** (§ Scope boundary 1), mirroring `SEG_INDEX_WIDTH` vs
   `MAX_ROOT_SEGMENTS` — the same distinction, in the same crate, already argued in-tree.
3. **Where the profile derivations live** (§ Scope boundary 3): on the stored tuple, here.
4. **The ETag composition** (success criterion leg 5) and the **fingerprint** (leg 6) — the two
   things ADR-0047 and 0016 explicitly deferred to the multipart slice, spelled out so the test
   oracle is independent of the implementation.
5. **`sha2` in `crates/core`** (§ Scope boundary 4), with the reason it is not a new-dependency
   decision recorded in the manifest itself.

## Alternatives considered

- **Fold #655 into this slice** (the issue itself invites it: *"Fold into it only if the pair stays
  within the ~1,500 added-semantic-line budget"*). **Declined.** This slice is already budgeted at
  the full 1,500 lines for the record family alone, and #655's ~31 constants each carry a
  derivation in prose — the pair would land at roughly twice the budget that four prior rejections
  established as the reviewability ceiling for this work. The two also have genuinely different
  review questions ("is this record shape right?" vs "is this number derived correctly?"). Kept
  separate, ordered as waves, with the format/capacity boundary above making the seam clean. **This
  is a scoping decision the human confirmed at Plan** — if it should be folded, say so before Do
  starts, not after.
- **A `wyrd-multipart` crate of its own.** Rejected: ADR-0016 keeps `core` coarse until compile
  times or boundaries demand a split, and this seam has no dependency the crate does not already
  have. A new crate would also make C4-verify take its `GREEN_ONLY` branch for a patch that creates
  the crate (`run-verify.sh:371`), losing even the green-leg precision.
- **Deriving the multipart ETag from MD5 for S3-classic parity.** Rejected by ADR-0047 (`:87-89`):
  it needs a new MD5 dependency through the ADR-0003 wall for a legacy equality S3 itself does not
  guarantee, and Wyrd already carries a vetted SHA-256 on this path.
- **Validating capacity at decode.** Rejected — see § Scope boundary 2 and `metadata.rs:312-321`:
  it turns a durable record unreadable the day a derived constant moves.

## Impact & compatibility

- **Purely additive, and inert.** Every prefix this slice names (`mpuctl`, `mpu:`, `slot:`, `part:`,
  `psum:`, `sidx:`, `retire:`) is new and nothing reads or writes them yet; no existing record
  shape, key, or code path changes. There is no migration and no on-disk change in this slice.
- **`PendingEntry` gains two optional fields** (`owner`, `staged`) — additive, `Option` +
  `#[serde(default)]` + `skip_serializing_if`, `Some` only on a `sidx:` value. The round-trip
  identity on **both** a legacy `pending:` value and an owned `sidx:` value is a merge requirement
  (0016's graduation criteria) and is leg (3) here. Getting this wrong turns every
  `require(key, encode(prior))` CAS over a legacy pending record into a permanent `Conflict`.
- **`crates/core` gains one already-vetted workspace dependency** (`sha2`); no crate new to the
  tree, no licence question, `deny.toml` unchanged.
- **Docs currency** (`../wyrd/AGENTS.md`): this slice defines persisted record **classes** but
  writes none. Treat the architecture docs as **confirm-only** here — the record classes become
  observable when #656+ write them, and claiming them in `06-runtime-view.md` /
  `08-crosscutting-concepts.md` now would describe a store shape no code produces. If a docs gate
  disagrees, that is a §6 item to raise, not a paragraph to invent.

## Open questions

- **None blocking Do.** Two to settle at sign-off rather than in code: (a) whether the typed
  outcome enums should be `#[non_exhaustive]` — cheap now, breaking later, and #508 is the consumer
  that would feel it; (b) whether `PART_NUMBER_WIDTH` should address S3's 10,000-part maximum
  exactly (5 digits) or leave headroom — a format decision that is expensive to change once a
  record is durable, so state the choice and its reasoning in `build-notes.md` and let sign-off
  ratify it.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

[a46]: ../wyrd/docs/design/adr/0046-bucket-model-real-namespace.md

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make the settled grammar, validation, and ordering causes load-bearing — direct adversarial checks fail and an independent scan leaves 13 of 164 mutants alive (`crates/core/src/multipart.rs:923`, `crates/core/src/multipart.rs:1042`).; T4 Contribution — Human must confirm closed/rejected prior art for all 14 affected paths — merged history and open PRs were mechanically clear, but the unavailable #508/#636 internal artifacts and contribution checker leave that half undischarged.; T5 Judgment — Rebuild must make tests exercise their claims — the fingerprint “reordered” case never reorders input and the ETag case delegates sorting to a future caller, so green is not fitness evidence (`crates/core/tests/multipart_records.rs:701`, `crates/core/tests/multipart_records.rs:613`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_654/review-. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 164 mutants tested in 4m: 13 missed, 61 caught, 90 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 21 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_654/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized (189 KB patch vs 100 KB threshold) and did not stay inside its own stated scope: patch touches 15 files including explicitly out-of-scope write.rs, custodian, and architecture-doc surfaces against a declared <=5-file pure-vocabulary slice. T4 batched review is gating-fail with 10 blocking findings, and the reviewer additionally failed C3 Change, T1 Structure, and T2 Shape — the core "invalid records are unrepresentable" property this slice exists to establish is not actually enforced (several internally-inconsistent record shapes still decode successfully), and the flagship 007-fixed-width rejection test is missing. Re-split at Plan rather than iterate-do — per docs/2026-07-31-oversized-slices-report.md, over-budget slices don't converge with more Do rounds. Carry forward into the split: - the brief's own <=5-file ceiling appears unbuildable as written: adding the two PendingEntry optional fields forces a caller change in crates/core/src/write.rs, so the next Plan should either widen the declared file allowance for that one mechanical touch or find another seam. - close the repeated-field identity relations at decode (mpuctl profile/max_sessions relation, sidx owner-vs-key, publication-target-vs-session, generation-vs-session-token, ETag count vs part keyspace) — five direct adversary cases showed parse-don't-validate is not yet enforced. - add the missing 007 fixed-width adversary case to the key-grammar test table. - two judgment calls for the next Plan to settle explicitly before Do rebuilds: five-digit persisted part-number headroom, and whether the public outcome enums should be non-exhaustive.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_654/review-
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
