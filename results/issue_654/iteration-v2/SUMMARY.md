# Result — issue 654 / multipart-record-family-and-state-machine

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the multipart **vocabulary**, in `crates/core`, with **no store I/O and no gateway**:
  the record types and their key grammar, decoders that make an invalid record unrepresentable, the
  typed outcomes the later slices answer with, decision 3's verb × state answer table as **pure
  functions**, and the two digests — `complete_fingerprint` and `multipart_etag`. After this slice a
  reader can name every record the protocol writes, parse every key it uses, decide what any verb
  answers in any state, and compute an object's multipart ETag — all without a `MetadataStore`.
- Success criterion: the NEW file `crates/core/tests/multipart_records.rs` passes. Every leg is
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
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; Wyrd has no
  maintenance branches, and M4's integration branch is merged and deleted. #634 and the whole #635
  slice sequence landed on `main` directly. Verified `git -C ../wyrd rev-parse origin/main` →
  `339da46`.)
- Scope (one logical fix) / out of scope: the **pure** half of the multipart seam, in **one new module**
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 253 mutants tested in 6m: 117 caught, 136 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_654/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #654’s pure multipart record vocabulary, canonical key grammar, validating codecs, typed state-machine outcomes, and digest functions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Human must ratify five-digit persisted part-number headroom and whether the public outcome enums are non-exhaustive — either choice fixes a costly-to-change storage/API boundary (`crates/core/src/multipart.rs:306`, `crates/core/src/multipart.rs:2063`). |
| C2 Reproduction (red pre-fix) | N/A | This is a declared born-at-tier feature: replaying the new discriminator on base `339da46` fails to compile on the absent module/dependency/fields before any assertion executes, so no behavioral pre-fix red exists. |
| C3 Change | FAIL | Plan must re-enter because the patch touches 15 paths, including explicitly out-of-scope write, custodian, and architecture surfaces, rather than the approved at-most-five-file pure slice (`crates/core/src/write.rs:198`, `docs/design/architecture/08-crosscutting-concepts.md:87`). |
| C4 Verification (red→green) | FAIL | Rebuild must add the binding `007` fixed-width adversary — that named parser negation stayed green at 28/28, so compile-shaped red plus the otherwise-green full gate and 253-mutant scan do not prove the grammar discriminator load-bearing (`crates/core/tests/multipart_records.rs:158`, `crates/core/src/multipart.rs:524`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must close repeated-field identity relations at decode — five direct adversaries showed the claimed parse-don’t-validate cause still admits inconsistent records (`crates/core/src/multipart.rs:1023`, `crates/core/src/multipart.rs:1258`, `crates/core/src/multipart.rs:1783`). |
| T1 Structure | FAIL | The settled reviewability boundary is broken: the 2,348-line module and 1,796-line test, inside a 15-file patch, exceed the brief’s 1,500-semantic-line and five-file ceiling (`crates/core/src/multipart.rs:2348`, `crates/core/tests/multipart_records.rs:1796`). |
| T2 Shape | FAIL | Invalid stored shapes remain representable: decoders accept count above the derived admission cap, a publication target differing from its session, an `sidx` owner differing from its key, a generation payload under a session token, and an ETag count beyond the part keyspace (`crates/core/src/multipart.rs:1023`, `crates/core/src/multipart.rs:1258`, `crates/core/src/multipart.rs:1555`, `crates/core/src/multipart.rs:1783`, `crates/core/src/multipart.rs:1844`). |
| T3 Runtime | PASS | Existing-path compatibility is independently green: the legacy pending value remains byte-identical, 28 focused tests pass, and the full fmt/clippy/build/test/docs/dependency gate passes after rerunning cargo-deny with a writable advisory-db home (`crates/core/tests/multipart_records.rs:996`). |
| T4 Contribution | NEEDS-HUMAN | Human must clear closed/rejected prior art for all 15 affected paths — merged `HEAD` has no multipart symbols or commits for the two new files, but the closed-work corpus and `scripts/review-branch` runner were not present, so the asserted 10-finding batch-review red is provisional. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make the tests exercise their claims — the fixed-width table omits the required `007` case, and the retirement test affirmatively accepts a generation payload under a session-scoped key (`crates/core/tests/multipart_records.rs:162`, `crates/core/tests/multipart_records.rs:1108`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether an oversized vocabulary with unresolved fail-closed identity checks is fit to freeze as the persisted/API foundation for five downstream slices — accepting it makes those omissions expensive storage-format and consumer contracts. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Human must ratify five-digit persisted part-number headroom and whether the public outcome enums are non-exhaustive — either choice fixes a costly-to-change storage/API boundary (`crates/core/src/multipart.rs:306`, `crates/core/src/multipart.rs:2063`).
- [ ] C5 Causal adequacy — Rebuild must close repeated-field identity relations at decode — five direct adversaries showed the claimed parse-don’t-validate cause still admits inconsistent records (`crates/core/src/multipart.rs:1023`, `crates/core/src/multipart.rs:1258`, `crates/core/src/multipart.rs:1783`).
- [ ] T4 Contribution — Human must clear closed/rejected prior art for all 15 affected paths — merged `HEAD` has no multipart symbols or commits for the two new files, but the closed-work corpus and `scripts/review-branch` runner were not present, so the asserted 10-finding batch-review red is provisional.
- [ ] T5 Judgment — Rebuild must make the tests exercise their claims — the fixed-width table omits the required `007` case, and the retirement test affirmatively accepts a generation payload under a session-scoped key (`crates/core/tests/multipart_records.rs:162`, `crates/core/tests/multipart_records.rs:1108`).
- [ ] Validation — fitness-to-purpose — Human must decide whether an oversized vocabulary with unresolved fail-closed identity checks is fit to freeze as the persisted/API foundation for five downstream slices — accepting it makes those omissions expensive storage-format and consumer contracts.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_654/review-
- [ ] size backstop — this slice is behaving oversized: patch is 189 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C1 Spec — Plan must decide whether mechanical `PendingEntry` initializer updates are allowed beyond the five-file ceiling — adding the fields forces callers such as `crates/core/src/write.rs:206` to change, so the stated scope cannot build as written.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized (189 KB patch vs 100 KB threshold) and did not stay inside its own stated scope: patch touches 15 files including explicitly out-of-scope write.rs, custodian, and architecture-doc surfaces against a declared <=5-file pure-vocabulary slice. T4 batched review is gating-fail with 10 blocking findings, and the reviewer additionally failed C3 Change, T1 Structure, and T2 Shape — the core "invalid records are unrepresentable" property this slice exists to establish is not actually enforced (several internally-inconsistent record shapes still decode successfully), and the flagship 007-fixed-width rejection test is missing. Re-split at Plan rather than iterate-do — per docs/2026-07-31-oversized-slices-report.md, over-budget slices don't converge with more Do rounds. Carry forward into the split: - the brief's own <=5-file ceiling appears unbuildable as written: adding the two PendingEntry optional fields forces a caller change in crates/core/src/write.rs, so the next Plan should either widen the declared file allowance for that one mechanical touch or find another seam. - close the repeated-field identity relations at decode (mpuctl profile/max_sessions relation, sidx owner-vs-key, publication-target-vs-session, generation-vs-session-token, ETag count vs part keyspace) — five direct adversary cases showed parse-don't-validate is not yet enforced. - add the missing 007 fixed-width adversary case to the key-grammar test table. - two judgment calls for the next Plan to settle explicitly before Do rebuilds: five-digit persisted part-number headroom, and whether the public outcome enums should be non-exhaustive.
- By / date: Eduard Ralph / 2026-08-05

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
