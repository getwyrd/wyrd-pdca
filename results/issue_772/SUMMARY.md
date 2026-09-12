# Result — issue 772 / multipart-owned-staging-entry

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the owned **staging entry** — the `sidx:<upload-id>:<part-number>:<chunk-id>`
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
- Success criterion: the owned entry round-trips under its own key, every torn or misfiled
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: exactly four substantive files, one new test,
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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.3% — 164 of 172 instrumentable changed lines executed (floor 80%); 172 of 617 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 38 mutants tested in 63s: 9 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_772/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add a validated multipart owned-staging entry under `sidx:`, make the shared `pending:` record namespace-safe, and keep both maintenance sweeps fail-safe.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The ten behavioral legs, the 13-file boundary, the live-path risk, and the born-at-tier limitation are explicit enough to judge independently (`brief.md:14`, `brief.md:171`, `brief.md:235`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept compile-time criterion absence as sufficient reproduction — the stashed base has no discriminator target, while the retained-test RED fails before any behavior executes (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:175`). |
| C3 Change | PASS | The scoped change supplies key-aware `sidx:` decoding, namespace-aware `pending:` decoding, and fail-safe handling in both sweeps without adding a writer or protocol transition (`crates/core/src/multipart.rs:3608`, `crates/core/src/metadata.rs:1671`, `crates/core/src/write.rs:646`, `crates/custodian/src/gc.rs:498`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept green-only verification without an executable behavioral RED — all 14 discriminator tests and the custodian scan-order leg pass, but production-reverted compilation never reaches a test (`gate-logs/C4-ci.log:986`, `gate-logs/C4-ci.log:1250`, `gate-logs/C4-verify.log:176`). |
| C5 Causal adequacy | PASS | The change removes namespace-blind reads through dedicated decode boundaries and classifies unreadable sweep entries instead of adding a capability probe or symptom guard (`crates/core/src/metadata.rs:1671`, `crates/core/src/write.rs:655`, `crates/custodian/src/gc.rs:504`). |
| T1 Structure | PASS | The patch stays within the 13 named files, places the discriminator in a new always-built integration test, and keeps mechanical ripple files to initializer/clone changes (`crates/core/tests/multipart_owned_staging.rs:1`, `crates/core/tests/mutation_regressions.rs:224`). |
| T2 Shape | PASS | Hand-authored wire witnesses drive the production decoders and redb store, while the custodian leg controls both forward and reverse scan order rather than relying on `HashMap` iteration (`crates/core/tests/multipart_owned_staging.rs:117`, `crates/core/tests/multipart_owned_staging.rs:209`, `crates/custodian/tests/gc.rs:919`). |
| T3 Runtime | PASS | The focused tests execute successfully and frozen diff coverage is 95.3% over instrumentable changed lines (`gate-logs/C4-ci.log:986`, `gate-logs/C4-ci.log:1250`, `gate-logs/C4-diff-cov.log:1037`). |
| T4 Contribution | FAIL | The contribution is not ready while the multi-pass review has one unresolved blocking convention defect: `decode_pending_entry` accepts spellings renewal can silently rewrite; the later contribution-artifact audit is N/A until publish (`crates/core/src/metadata.rs:1674`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Require `pending:` decoding/renewal to reject or preserve noncanonical and unknown-field spellings — the test helper asserts identity only for curated witnesses, while accepted bytes such as `"owner":null` can be CAS-replaced with fields dropped (`crates/core/tests/multipart_owned_staging.rs:193`, `crates/core/src/metadata.rs:1674`, `crates/core/src/metadata.rs:2146`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the durable metadata behavior is fit to ship only after the canonicalization defect and the green-only verification posture are resolved or explicitly accepted (`crates/core/src/metadata.rs:2123`, `crates/core/src/write.rs:646`). |

Host caveat: the independent `cargo xtask ci` rerun passed typos, docs render, guards, fmt, clippy, build, workspace tests, and cargo-machete, then stopped because `cargo deny` could not lock the host's read-only advisory database; the frozen gate completed the deny checks and ended green (`gate-logs/C4-ci.log:2869`, `gate-logs/C4-ci.log:3453`).

### Advisory — adversary

# Adversarial review — #772 (multipart owned staging entry)

**Bottom line:** I could not break the fix. The red→green evidence holds up when rebuilt independently. The one gating failure (T4) describes behavior the base code already had, not something this patch introduced. Whether that blocker stands is a human call.

## Findings

- NEEDS-HUMAN [human] — **The T4 gating blocker looks like old behavior, not a regression.** T4 flags `crates/core/src/metadata.rs:1674` (`decode_pending_entry` accepts `"owner":null` and unknown fields, so `renew_pending` "can CAS successfully and silently rewrite/drop fields"). I ran `renew_pending(&[chunk], now=1000, lease 4500)` over three stored `pending:` values, first on the base and then on the patched tree:
  - `{"lease_expiry_millis":1500,"x":1}`: **both** return `Committed` and store `{"lease_expiry_millis":4500}`.
  - `{"lease_expiry_millis":1500,"owner":null}`: **both** return the same.
  - An owned value (both fields present): the **base** returns `Committed` and erases the ownership fields. The **patched** code returns `Err` and leaves the bytes untouched.

  The `"owner":null` example loses nothing, because null means absent. The base already dropped unknown fields: its derive ignored them, and `renew_pending` stores the caller's entry by design (`metadata.rs:2146`). The rubric's identity rule applies only "wherever a compare-and-swap or content hash depends on it". Both `pending:` compare-and-swaps pin the raw bytes they read (`metadata.rs:2146`, `:2181`), not a re-encoding, so none of them depends on decode→encode identity. The patch explains why the wire stays open (`metadata.rs:1586-1589`: stored records read by a fleet running mixed versions). Closing it would change the `pending:` format, which the brief never asked for. I'd suggest declining with an issue reference under the rubric's out-of-scope rule rather than rebuilding, but the decision belongs to the human.

- NEEDS-HUMAN [human] — **The namespace rule lives in the entry point, not in the codec.** The store-wide `metadata::decode::<PendingEntry>` still reads an owned value as a plain `PendingEntry`: `metadata.rs:1593` and `:1626` apply only the pairing check. The tests pin this on purpose (`crates/core/tests/multipart_owned_staging.rs:314`, `:431`). All four current readers now use the new entry point, and a negation covers each one (see below). But on the base, all four readers used `let e: PendingEntry = metadata::decode(&v)?`. The next `pending:` reader written that way will accept an owned entry, and neither the compiler nor the runtime will object. That is the "invariant becomes a convention" outcome the brief's invariant section warns about. `decode_owned_entry` reads through its own `OwnedEntryWire` (`crates/core/src/multipart.rs:3613`) and never through `PendingEntry`'s `Deserialize`. So the ordinary-only rule could sit inside `PendingEntry`'s own `try_from` without affecting `sidx:`. The brief left the mechanism to the builder, and 0016 plans one renewal loop for both shapes, so this is a design choice, not a defect. Low priority.

- Minor, no action needed: the S10 docs addition (`docs/design/architecture/05-building-block-view.md:202`) is 164 words. The ADR-0047 bullet the brief named as the length model (`:187-194`) is 92 words. The content is accurate. I checked the "off unless an operator arms it" wording against `crates/custodian/src/gc.rs:172-175`, and confirmed that `write::sweep_expired_leases` has no production caller.

## What I tried and could not refute

- **Re-ran the evidence** in a scratch copy: `cargo test -p wyrd-core --test multipart_owned_staging` passed 14/14 and `-p wyrd-custodian --test gc` passed 11/11. Re-running the built binaries 30 and 60 times produced no flaky failures. C4-verify's RED leg is the pre-declared compile failure (`gate-logs/C4-verify.log`), so I rebuilt the brief's eight isolating negations myself instead of relying on the builder's account. Each one fails **exactly one** test:
  - S1, scheme check off: `s1_…`
  - S2, owner/key check off: `s2_…`
  - S3, pairing forced to `Ok`: `s3_…`
  - S4, `renew_pending` on the generic decode: `s4_renew_pending_…`
  - S4, `live_lease_guards` on the generic decode: `s4_a_leased_commit_…`
  - S5, sweep `continue`→`break`, or `Ok` on skip: `s5_…`
  - S6, reject a length mismatch: `s6_…`
  - S7, drop `owner`'s `skip_serializing_if`: `s7_…`
  - S8, drop `require_canonical`: `s8_…`
- **Extra negations beyond the brief, all caught:**
  - GC on the generic decode, GC `continue`→`break`, the GC audit call silenced, the GC counter removed, and GC skipping the next *readable* entry after an unreadable one (the order-dependent bug round 2 found). Each fails the GC leg, which runs every seed in both scan orders.
  - The write sweep on the generic decode fails `s5_…`.
  - Dropping the writer-side check from `put_pending` or `renew_pending` fails `s3_…` and `s4_the_pending_writers_…`.
  - Dropping the pairing check from `PendingEntryWire::try_from` fails `s3_…`.
- **Production path:** the tests call the real `decode_pending_entry`, `renew_pending`, `create_leased`, `put_pending`, `sweep_expired_leases` (over redb) and `reconcile_step` under `ExpiredPendingPolicy::Reclaim`. Nothing is mirrored or re-implemented.
- **Missed readers:** every production read of a `pending:` value now goes through the new entry point (`metadata.rs:2141`, `:2177`, `write.rs:655`, `gc.rs:504`). `crates/custodian/src/restore.rs:731` reads keys only.
- **Bypass inputs on `pending:`**, all refused by `decode_pending_entry` in a probe:
  - `\u`-escaped field names: namespace mismatch.
  - Duplicate `owner` or `staged` with a trailing `null`: duplicate-field error.
  - `"staged":null` beside an owner: torn value.
  - An owned value with an unknown field nested in `staged`: malformed-value error.
  - Duplicate `lease_expiry_millis`: duplicate-field error.

  No owned-shaped input reads as an ordinary lease. On `sidx:`, the closed wire and the canonical-bytes check also refuse `null` spellings, reordered fields and extra whitespace.
- **GC fails safe:** a skipped chunk never enters `expired_pending` (`gc.rs:173`, `:204`) and has no orphan record, so it lands in the "no evidence, keep it" branch (`gc.rs:207-211`). Its `pending:` entry is never added to `swept_pending`, so it is not deleted.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept compile-time criterion absence as sufficient reproduction — the stashed base has no discriminator target, while the retained-test RED fails before any behavior executes (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:175`). Cleared: this is new code with no prior behavior to reproduce a failure against.
- [x] C4 Verification (red→green) — Accept green-only verification without an executable behavioral RED — all 14 discriminator tests and the custodian scan-order leg pass, but production-reverted compilation never reaches a test (`gate-logs/C4-ci.log:986`, `gate-logs/C4-ci.log:1250`, `gate-logs/C4-verify.log:176`). Cleared: restatement of the C2 new-code reasoning.
- [x] T5 Judgment — Require `pending:` decoding/renewal to reject or preserve noncanonical and unknown-field spellings — the test helper asserts identity only for curated witnesses, while accepted bytes such as `"owner":null` can be CAS-replaced with fields dropped (`crates/core/tests/multipart_owned_staging.rs:193`, `crates/core/src/metadata.rs:1674`, `crates/core/src/metadata.rs:2146`). Cleared: pre-existing base behavior, not introduced by this patch — routed to Act (§10).
- [x] Validation — fitness-to-purpose — Decide whether the durable metadata behavior is fit to ship only after the canonicalization defect and the green-only verification posture are resolved or explicitly accepted (`crates/core/src/metadata.rs:2123`, `crates/core/src/write.rs:646`). Cleared: both gating concerns resolved above (new-code reasoning; pre-existing behavior routed to Act).
- [x] **The T4 gating blocker looks like old behavior, not a regression.** T4 flags `crates/core/src/metadata.rs:1674` (`decode_pending_entry` accepts `"owner":null` and unknown fields, so `renew_pending` "can CAS successfully and silently rewrite/drop fields"). I ran `renew_pending(&[chunk], now=1000, lease 4500)` over three stored `pending:` values, first on the base and then on the patched tree: Cleared: confirmed pre-existing, routed to Act (§10).
- [x] **The namespace rule lives in the entry point, not in the codec.** The store-wide `metadata::decode::<PendingEntry>` still reads an owned value as a plain `PendingEntry`: `metadata.rs:1593` and `:1626` apply only the pairing check. The tests pin this on purpose (`crates/core/tests/multipart_owned_staging.rs:314`, `:431`). All four current readers now use the new entry point, and a negation covers each one (see below). But on the base, all four readers used `let e: PendingEntry = metadata::decode(&v)?`. The next `pending:` reader written that way will accept an owned entry, and neither the compiler nor the runtime will object. That is the "invariant becomes a convention" outcome the brief's invariant section warns about. `decode_owned_entry` reads through its own `OwnedEntryWire` (`crates/core/src/multipart.rs:3613`) and never through `PendingEntry`'s `Deserialize`. So the ordinary-only rule could sit inside `PendingEntry`'s own `try_from` without affecting `sidx:`. The brief left the mechanism to the builder, and 0016 plans one renewal loop for both shapes, so this is a design choice, not a defect. Low priority. Cleared: design choice, accepted as-is — routed to Act (§10) as a future hardening idea.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above). Cleared: new-code reasoning, same as C2/C4 items above.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_772/review-b Cleared: underlying findings resolved above — canonicalization concern pre-existing (Act), DST-coverage gap scoped out deliberately (Act with owner).
- [x] size backstop — this slice is behaving oversized: patch is 107 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. Cleared: ignored for this iteration per human decision.
- [x] T5 Judgment — Confirm affected-file prior art across merged history and closed/rejected work before sign-off — this disposable target has one synthetic commit and no remotes, so the brief's prior-attempt claims cannot be independently reproduced and overlapping earlier work could change the judgment (`brief.md:284`). Cleared: no known conflicting prior work on these files.
- [x] **The `pending:` writers still accept an owned or torn entry.** Cleared: no supporting evidence in the bundle, and it contradicts the adversary's own verified findings (dropping the writer-side check from `put_pending`/`renew_pending` fails specific tests, confirming the check exists and is exercised) — treated as a stray, unsubstantiated bullet.
- [x] T5 Judgment — Confirm prior-art overlap against the current base for every affected path — the supplied check is anchored to older `origin/main` evidence and selected shared files, while this disposable target has one commit and no remotes, so merged or closed stacked work could change the conflict and scope judgment (`brief.md:284`). Cleared: same basis as the prior-art item above — no known conflicting prior work.
- [x] T4 is a gating check and it is still red, with two blockers (`gate-logs/T4-batch-review.log`): no seeded Tier-0 DST (deterministic simulation test) coverage for the changed sweep at `crates/core/src/write.rs:654` or the GC skip at `crates/custodian/src/gc.rs:502`. The finding is accurate. Before the patch, one unreadable `pending:` value aborted the whole pass. Now both sweeps keep going and delete other entries, and the rubric's *Test fidelity* rule asks for seeded DST coverage of that kind of change. The brief rules it out, though: `crates/dst/tests/custodian.rs` may only get the mechanical initializer, and a 14th file means STOP. The existing DST leg (`crates/dst/tests/custodian.rs:883`) seeds only ordinary leases. A rebuild can't clear this without breaking the brief. A human has to choose: record a rejection reason (the rubric's "Definition of done"), or widen scope, e.g. a follow-up issue for a seeded leg with misfiled, torn and garbage `pending:` values. Cleared: accepted with reason (deliberate wave-sequencing boundary with #722) — routed to Act (§10) with #722 named as the intended owner.
- [x] The brief's S8 negation can't work as written. "Remove ONE `skip_serializing_if` … on the owned witness" has no effect on an owned value, because both fields are `Some`. I removed the attribute from `owner` (`crates/core/src/metadata.rs:1601`): S7 (`crates/core/tests/multipart_owned_staging.rs:638`) failed and S8 (`:662`) **passed**. What S8 actually guards is the `sidx:` canonical-bytes check: dropping `require_canonical` at `crates/core/src/multipart.rs:3627` fails S8 and only S8. So the eight-negation evidence holds with that swap. But `build-notes.md` (withheld from me) should record the canonical-gate negation for S8. If it says the briefed negation failed S8, that claim is wrong. Cleared: checked `build-notes.md` — it already correctly records the re-targeted canonical-bytes negation, matching the adversary's finding; no inaccuracy.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-11

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- The `decode_pending_entry` canonicalization gap (accepts `"owner":null` / unknown-field spellings) predates this patch and was not introduced by it — file as a standalone follow-up rather than a blocker on this bundle.
- Ensure #722 or a follow-up issue adds seeded DST (deterministic simulation test) coverage for the sweep skip-instead-of-abort behavior (`crates/core/src/write.rs`, `crates/custodian/src/gc.rs`) with misfiled/torn/garbage `pending:` records — this slice was scoped out of touching `crates/dst/tests/custodian.rs` substantively, so the gap needs an owner.
- Consider hardening the namespace pairing rule into `PendingEntry`'s own `try_from` (rather than relying on every reader going through the entry point) so a future hand-written reader can't reintroduce the owned/lock mix-up.
