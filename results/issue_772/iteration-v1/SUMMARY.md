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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 93.2% — 137 of 147 instrumentable changed lines executed (floor 80%); 147 of 528 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 35 mutants tested in 54s: 1 missed, 8 caught, 26 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_772/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.95s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #772: add typed, key-aware multipart owned staging records while preserving legacy `pending:` leases and quarantining namespace-invalid values.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The ten-leg brief fixes the decode boundary, fail-safe sweep behavior, serialization identity, cross-crate minting, docs, scope, and required toolchain precisely enough to judge (`brief.md:14`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept born-at-tier criterion absence as sufficient reproduction — the stashed base has no `multipart_owned_staging` target, while retaining the test only produces missing-new-API compile errors and never executes behavior (`gate-logs/C4-verify.log:10`). |
| C3 Change | PASS | The 13 scoped files implement both namespace decoders, typed structural rejection, fail-safe sweep handling, a checked external minting path, mechanical `Copy` fallout, and the living-doc update without adding an `sidx:` writer (`crates/core/src/multipart.rs:3343`, `crates/core/src/metadata.rs:1554`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept green verification without an executable behavioral RED — the reviewer reran all 13 focused tests and the custodian GC leg green, frozen CI/deny/docs and TiKV gates passed, but the RED discriminator never compiled (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:3452`). |
| C5 Causal adequacy | PASS | Key-aware decode entry points remove the namespace convention at every live reader and sweeps fail safe; the sole missed `||`→`&&` mutant is equivalent because pairing already proves the two options have identical presence (`crates/core/src/metadata.rs:1620`, `crates/core/src/metadata.rs:1650`). |
| T1 Structure | PASS | Namespace validation remains in `metadata`/`multipart`, production GC uses the existing durability signal, and no new trait seam or dependency inversion is introduced (`crates/core/src/metadata.rs:1629`, `crates/custodian/src/gc.rs:589`). |
| T2 Shape | PASS | Additive optional fields omit absence, closed owned wires reject unknown structure, typed errors cover torn/misfiled/unsupported geometry, and placement length remains contextual (`crates/core/src/metadata.rs:1585`, `crates/core/src/multipart.rs:3382`). |
| T3 Runtime | PASS | The reviewer reran the focused Redb reader/sweep suite and the real custodian reconciliation leg successfully; frozen full-workspace CI, 93.2% diff coverage, and TiKV feature compilation also passed (`crates/core/tests/multipart_owned_staging.rs:395`, `crates/custodian/tests/gc.rs:895`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check and the substantive contribution audit is mandatorily rerun at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm affected-file prior art across merged history and closed/rejected work before sign-off — this disposable target has one synthetic commit and no remotes, so the brief's prior-attempt claims cannot be independently reproduced and overlapping earlier work could change the judgment (`brief.md:284`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether typed rejection plus quarantine is operationally fit for the staged multipart rollout — tests establish the record mechanics, but no production `sidx:` writer or consumer yet exercises the end-to-end lifecycle (`docs/design/architecture/05-building-block-view.md:202`). |

### Advisory — adversary

# Adversarial review — #772 (multipart owned staging entry)

**Bottom line:** I could not refute the core fix. Each of the brief's negations, re-run on the
patched tree, fails exactly one test, and the tests drive the production readers over real
stores. I found one scope hole on the **write** side (a human call), one real test gap (the GC
skip's audit signal can be deleted with every test still green), and two small nits.

## Findings

- NEEDS-HUMAN [human] — **The `pending:` writers still accept an owned or torn entry.**
  `put_pending` (`crates/core/src/metadata.rs:2039-2047`) and `renew_pending`
  (`metadata.rs:2111`, `put(key, encode(entry))`) encode whatever `PendingEntry` they are given
  under `pending:`, with no shape check. Reproduced on the patched tree over an in-memory redb
  store: `put_pending(&store, 5, &OwnedEntry::new(..).to_pending())` → `Committed`;
  `renew_pending(&store, &[6], 10, &owned)` over a live ordinary lease → `Committed`, and
  `pending:6` now holds the owned shape; the next ordinary `renew_pending` of chunk 6 then fails
  with "an owned pending entry is stored under `pending:`". A torn literal (`owner: Some(..),
  staged: None`) compiles because the fields are `pub` (`metadata.rs:1595-1601`) and gets
  written the same way. Every `pending:` reader refuses the result, both sweeps skip it forever,
  and under the deployed `ExpiredPendingPolicy::Defer` nothing even logs it. That is the "bytes
  nothing can read back" hazard S9 exists to close, reached through the `pending:` writer
  instead of a `sidx:` one. It is also T4-batch-review's `metadata.rs:1596` blocker. The fix is
  small (refuse `owner.is_some() || staged.is_some()` in both writers; both already return the
  boxed error, so no signature changes). But it is not one of the ten legs, and it grows the
  `metadata.rs` rebase surface the brief wants kept small for #776. Fix it here, or decline with
  a follow-up issue.

- NEEDS-HUMAN [impl] — **Nothing tests the GC skip's audit signal.** That signal is the only
  thing behind the claim "so the skip is never silent" (`crates/custodian/src/gc.rs:490`), and
  it is the exact point T4 raised at `gc.rs:491`. I replaced the body of
  `emit_unreadable_pending` (`gc.rs:594`) with `let _ = (entry, fault);`: the new leg at
  `crates/custodian/tests/gc.rs:896` still passes, and so does the whole `wyrd-custodian` suite
  (19 test binaries, 111 passed, 0 failed). C5 missed this because cargo-mutants'
  `replace emit_unreadable_pending with ()` leaves both parameters unused, which does not compile
  under `warnings = "deny"` (`Cargo.toml:227`), so it was counted among the 26 "unviable".
  Fix: in the `gc.rs:896` leg, capture the audit output with the existing pattern (`Capture` /
  `capturing_dispatch`, `crates/custodian/tests/segmented_map_consumers.rs:295-339`, including its
  `set_global_default` guard at `:335` against the #214 callsite-cache race). Then assert exactly
  one `unreadable-pending-entry` line naming `pending:226` (the misfiled chunk `0xE2`,
  `tests/gc.rs:914`).

- NEEDS-HUMAN [impl] — Minor: `OwnedEntry::from_pending`'s ordinary-shape arm
  (`crates/core/src/multipart.rs:3560-3563`) never runs in any test (C4-diff-cov lists
  3560-3563 as MISS). Its doc (`:3548`) promises
  `PendingEntryNamespaceMismatch { namespace: "sidx:", shape: "ordinary" }`, and this is the public
  validator S9 relies on. One assertion in the S9 test (`from_pending` on an
  `owner: None, staged: None` literal) covers it.

- NEEDS-HUMAN [impl] — Minor docs wording in the S10 sentence
  (`docs/design/architecture/05-building-block-view.md:202`). "(`staged`, a scheme the erasure
  coder supports and one D server per fragment)" puts a property decode checks (geometry) next
  to one it deliberately does not check (placement length: S6, and a `[5,6]` placement under
  `rs(2,1)` decodes). Read as written, it suggests decode enforces length, which is the over-claim
  the brief withdrew at v3. Also, "is a typed error at every reader" makes the promise S4 says
  not to make: `renew_pending` / `live_lease_guards` return the crate's boxed error. Suggested
  wording: "one D server per planned fragment (its length checked only by maintenance)" and
  "is refused at every reader".

## Refutation attempts that did not land

- **Red→green evidence.** `cargo test -p wyrd-core --test multipart_owned_staging` on the
  patched tree: 13/13 pass. The RED leg is a compile failure, as the brief pre-declared (C4-verify
  exit 77). In its place I ran the negations myself. Each fails exactly one test:
  - S1: drop `supported` in `checked_staged_scheme` (`multipart.rs:3446`) → only s1 fails.
  - S2: `if false` at `multipart.rs:3615` → only s2.
  - S3: pairing forced to `Ok` (`:3373`) → only s3.
  - S4, one reader at a time back to the generic decode: `renew_pending` (`metadata.rs:2106`) →
    only `s4_renew_pending…`; `live_lease_guards` (`:2142`) → only `s4_a_leased_commit…`; the
    sweep (`write.rs:652`) → only s5.
  - S5: `?`-abort in `write.rs` → only s5.
  - S6: add a length check to `StagedPlacement::new` → only s6.
  - S7: drop `skip_serializing_if` on `owner`, then separately on `staged` → only s7 each time.
  - GC side: generic decode at `gc.rs:498` → the `tests/gc.rs:896` leg fails (the fragment is
    reclaimed); `?`-abort → the same leg fails at `:936`.
- **S8's negation as the brief words it cannot fail S8.** Dropping a `skip_serializing_if` does
  nothing to an owned value, because both fields are present. I ran it and only S7 fails. The
  test relies on the canonical-bytes gate instead: removing `require_canonical`
  (`multipart.rs:3622`) fails s8 alone, so S8 does test something real. At sign-off, check that
  build-notes.md records this re-targeted negation and not the one the brief describes.
- **C5's one MISSED mutant is equivalent** (`metadata.rs:1650:30`, `||` → `&&`).
  `PendingEntry::try_from` at `:1649` has already refused every torn value, so by `:1650`
  `owner.is_some() == staged.is_some()`, and the two operators agree on every input that can
  reach that line. This is not a test gap. To clear the C5 red, simplify the condition to
  `entry.owner.is_some()`, or pin it in `.cargo/mutants.toml`.
- **T4's three `write.rs` blockers** (`write.rs:650`, `:652`, "silent skip") hit a function with
  no production caller. `sweep_expired_leases` is defined at `write.rs:644` and called only from
  seven test files; the production expiry path is the GC's. The brief also forbids adding a
  tracing seam to `write.rs` for this leg. These blockers can be rejected with that reason
  recorded. T4's `gc.rs:491` finding is wrong on the facts (the skip does emit a warning and a
  counter), but see the `[impl]` finding above: no test holds that in place.
- **Fail-safe checks.**
  - All four production readers of `pending:` values now go through `decode_pending_entry`
    (`metadata.rs:2106`, `:2142`; `write.rs:652`; `gc.rs:498`). The only other production
    `pending:` scan, restore (`crates/custodian/src/restore.rs:731`), reads keys only, so a
    misfiled value still protects its fragments there.
  - A skipped chunk cannot be reclaimed some other way: GC deletes a fragment only when it has an
    orphan record or its chunk is in the expired set (`gc.rs:196-211`).
- **Odd inputs through both decoders.** Each was refused or decoded as intended:
  - `"owner":null` plus `staged` → `TornOwnedEntry` under both namespaces.
  - A `a`-escaped owner → `NoncanonicalRecordValue` under `sidx:`.
  - A duplicate `owner` field, or an uppercase owner → `MalformedRecordValue`.
  - An unknown top-level field → refused under `sidx:`.
  - `rs(255,255)` with an empty placement decodes under `sidx:`. That is correct: the coder
    supports that geometry, and length is the contextual check.
  - Two open-wire values decode under `pending:` but do not re-encode byte-identically:
    `"owner":null,"staged":null`, and an ordinary lease with an unknown field. The old
    `PendingEntry` derive was open in the same way, so this diff did not introduce it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept born-at-tier criterion absence as sufficient reproduction — the stashed base has no `multipart_owned_staging` target, while retaining the test only produces missing-new-API compile errors and never executes behavior (`gate-logs/C4-verify.log:10`).
- [ ] C4 Verification (red→green) — Accept green verification without an executable behavioral RED — the reviewer reran all 13 focused tests and the custodian GC leg green, frozen CI/deny/docs and TiKV gates passed, but the RED discriminator never compiled (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:3452`).
- [ ] T5 Judgment — Confirm affected-file prior art across merged history and closed/rejected work before sign-off — this disposable target has one synthetic commit and no remotes, so the brief's prior-attempt claims cannot be independently reproduced and overlapping earlier work could change the judgment (`brief.md:284`).
- [ ] Validation — fitness-to-purpose — Decide whether typed rejection plus quarantine is operationally fit for the staged multipart rollout — tests establish the record mechanics, but no production `sidx:` writer or consumer yet exercises the end-to-end lifecycle (`docs/design/architecture/05-building-block-view.md:202`).
- [ ] **The `pending:` writers still accept an owned or torn entry.**
- [ ] **Nothing tests the GC skip's audit signal.** That signal is the only
- [ ] Minor: `OwnedEntry::from_pending`'s ordinary-shape arm
- [ ] Minor docs wording in the S10 sentence
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_772/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept born-at-tier criterion absence as sufficient reproduction — the stashed base has no `multipart_owned_staging` target, while retaining the test only produces missing-new-API compile errors and never executes behavior (`gate-logs/C4-verify.log:10`).; C4 Verification (red→green) — Accept green verification without an executable behavioral RED — the reviewer reran all 13 focused tests and the custodian GC leg green, frozen CI/deny/docs and TiKV gates passed, but the RED discriminator never compiled (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:3452`).; **Nothing tests the GC skip's audit signal.** That signal is the only; Minor: `OwnedEntry::from_pending`'s ordinary-shape arm; Minor docs wording in the S10 sentence; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_772/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-11

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
