# Result — issue 840 / completing-session-nonce

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: no session record can name its segment group's nonce. The session record carries
  `parent`, `object`, `content_type`, `created_at_millis`, `clock_source`, `epoch`, `attempts`
  and `state` (`crates/core/src/multipart.rs:2078-2088`); `Completing`'s `publish_target`
  carries `parent`, `name` and the fence `epoch` (`:1954-1961`); `Open` and `Aborting` carry no
  fields (`:2019`, `:2033`). The nonce is deliberately independent of the upload id (`:3307-3311`),
  so nothing on the record can derive it. 0016 needs it on the record for the session's whole
  life:
  * **at Create**, which mints it and reserves it with `require_absent(seggrp:<nonce>)` plus a
    `seggrp:` marker in the same batch (`0016:508-518`, `:656`);
  * **while `Completing`**, so a writer that ends the attempt can install
    `retire:records:{seg:<nonce>:<E>}` in the same batch as its fence (`0016:665`,
    `:2193-2196`). That covers the restore fence (child-4 of this split), the reaper and operator
    abort (#656, #659) and Complete's own rollback (#658). Without it those `seg:` records have
    no deleter anywhere (X57, `0016:880`);
  * **at the terminal delete**, which removes the `seggrp:` marker when the nonce's `seg:` range
    is empty (`0016:673`), including for an upload aborted before Complete, whose record is
    `Open` or `Aborting`;
  * **across a rollback**: `Completing@E → Open@E+1` (`0016:538-552`) drops `publish_target`,
    and the next attempt must write under the same nonce at a new epoch.
  `metadata.rs:1016-1018` already describes the nonce as "minted with the publishing session".
  **Design settled at Plan (the human):** option (i), 2026-09-12: the nonce is **stored on the
  session record**, not derived. Rejected: deriving it from `(upload id, E)` as `0016:2333` says,
  because the code keeps the nonce independent of the upload id on purpose (`:3307`).
  **Placement A, 2026-09-29:** the nonce is a field of the session record itself, present in
  **every** state, and a `Completing` session's segment group is `(the session's nonce,
  publish_target.epoch)`. Rejected: placement B, the nonce inside `Completing`'s
  `publish_target` only (this brief's earlier shape). It left `Open` and `Aborting` records
  unable to name the marker they reserved, and lost the nonce on every rollback.
- Success criterion: the NEW file `crates/core/tests/multipart_segment_nonce.rs` passes. The
  wire spelling is fixed here so that this test and the fixtures of #841, #842, #843 and #810
  agree: every session record carries `"segment_nonce"`, a string of exactly 32 lowercase hex
  characters (`SegmentNonce`, `crates/core/src/metadata.rs:963-1002`), immediately after
  `"clock_source"` and before `"epoch"` (creation-time fields first, then the ones transitions
  change). `publish_target` is unchanged. For example:
  `{"parent":1,"object":"n","created_at_millis":1000,"clock_source":"wall","segment_nonce":"0123456789abcdef0123456789abcdef","epoch":3,"attempts":1,"state":{"kind":"Open"}}`
  and, for `Completing`, the same record with
  `"state":{"kind":"Completing","fenced_at_millis":1,"segments_written":2,"publish_target":{"parent":1,"name":"n","epoch":3}}`.
  Legs, each over all four states (`Open`, `Completing`, `Aborting`, `Completed`):
  (a) a record carrying the nonce decodes through `decode_session_record` (`multipart.rs:2250`),
  and re-encoding the decoded value with `wyrd_core::metadata::encode` (`metadata.rs:1934`)
  gives back the input bytes exactly;
  (b) the same record **without** `segment_nonce` is refused;
  (c) a nonce that is not 32 lowercase hex characters (uppercase, 31 characters, one containing
  `:`) is refused;
  (d) the record names its nonce once: a `segment_nonce` inside `publish_target` is refused, and
  so is the field in any position other than the one above (the canonical-bytes check,
  `multipart.rs:2121-2125`).
  In `crates/core/tests/multipart_session_records.rs` (green-only): the decoded record exposes
  its nonce in every state, so the terminal delete can mint `seggrp_key` (`metadata.rs:1520`)
  from it; and a `Completing` record exposes its attempt's `SegmentGroup` `(nonce,
  publish_target.epoch)`, so a writer can mint the `seg:` range (`seg_range_prefix`,
  `metadata.rs:1505`) from it. Neither re-parses the nonce. Any other state has no attempt
  group.
  **(L) `cargo xtask ci` green.** After this change every existing session fixture in the
  workspace carries the nonce; they are listed under Scope.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the nonce as a field of the session record in every state: `SessionRecordWire`
  and `SessionRecord` (`multipart.rs:2078-2088`, `:2133`), the record's canonical-bytes decode
  (`:2121-2125`), and accessors for the nonce and for a `Completing` session's attempt group.
  The field decodes through `SegmentNonce`'s validating constructor: the type deliberately has no
  `Deserialize` (`metadata.rs:979-982`). Update the record's docs to match: the field count
  (`multipart.rs:2102`, "Seven of the eight fields"), and `PublishTarget`'s note that its epoch
  "makes the attempt's segment-group nonce deterministic" (`:1945`), which now reads as the
  attempt's epoch within the session's group. Every existing full session record in the
  workspace gains the field, each through its one builder:
  `crates/core/tests/multipart_session_records.rs` (`session_with`, `:88`, and the inline records
  at `:646`, `:711`), `crates/core/tests/multipart_state_machine.rs` (`completed_session_bytes`,
  `:402`), `crates/custodian/tests/staged_protection.rs` (`session`, `:577`),
  `crates/custodian/tests/staged_scrub.rs` (`session`, `:427`),
  `crates/custodian/tests/staged_repair.rs` (`session`, `:415`),
  `crates/custodian/tests/staged_drain_status.rs` (`session_open`, `:275`), and
  `crates/dst/tests/custodian.rs` (`handoff_session` `:2760` and `replace_session` `:4531`
  only). State-only fixtures (`{"kind":…}` values, `publish_target` values) do not change. Also
  the persisted-field sentence in `docs/design/architecture/05-building-block-view.md:202`
  (`AGENTS.md:154-157`, "Docs currency"), noting that 0016's `mpu:` row does not yet list the
  field. `metadata.rs` only for a constructor that builds a `SegmentGroup` from an
  already-validated `SegmentNonce`, if the accessor needs one. Size budget: under 45 KB of diff.
  / out of scope: any writer of the nonce (Create, #508; #658) and any reader of it (child-4);
  the `seggrp:` marker's writes and deletes; `restore.rs` and every custodian source file; the
  retire-obligation codec; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (17 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 30.0% — 6 of 20 instrumentable changed lines executed (below the 80% floor); 20 of 92 changed lines were i
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 6 mutants tested in 19s: 1 caught, 5 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_840/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #840: retain a validated segment-group nonce on every multipart session record; no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The human-settled placement, canonical bytes and downstream boundaries make this slice falsifiable without reopening the rejected Completing-only design (`brief.md:34`, `brief.md:39`, `brief.md:118`, `brief.md:152`). |
| C2 Reproduction (red pre-fix) | PASS | Independent stash/retest reproduced 17 assertion failures for the missing wire field, with successful compilation (`reviewer-evidence/red.log:207`, `reviewer-evidence/red.log:310`; `crates/core/tests/multipart_segment_nonce.rs:110`). |
| C3 Change | PASS | The patch supplies the required identity throughout the record's lifetime and stays within the authorized codec, accessor, fixture and documentation scope; all 11 postimages match the supplied patch, whose 44,873 bytes meet the budget (`crates/core/src/multipart.rs:2103`, `crates/core/src/multipart.rs:2179`, `reviewer-evidence/integrity.log:2`). |
| C4 Verification (red→green) | PASS | Independent restoration yields 17/17 codec, 37/37 session-record and 22/22 state-machine tests green; every frozen coverage MISS is reached with the prescribed accessor suite included; full CI is supported by its frozen log, with the local host limitation detailed below (`reviewer-evidence/green.log:27`, `reviewer-evidence/green.log:70`, `reviewer-evidence/green.log:98`, `reviewer-evidence/coverage-summary.log:4`, `gate-logs/C4-ci.log:3765`). |
| C5 Causal adequacy | PASS | A session-owned validated identity resolves the inability of Open/Aborting records to name their group, while the Completing accessor uses the checked fence epoch; no capability probe or symptom guard substitutes for the cause (`crates/core/src/multipart.rs:2179`, `crates/core/src/multipart.rs:2225`, `crates/core/src/multipart.rs:2288`). |
| T1 Structure | PASS | The existing validating type remains the sole nonce grammar boundary, and group construction accepts that validated type without reparsing or adding dependencies (`crates/core/src/multipart.rs:2082`, `crates/core/src/metadata.rs:996`, `crates/core/src/metadata.rs:1044`). |
| T2 Shape | PASS | Strict required-field decoding and canonical-byte identity protect later CAS operations; malformed, duplicate and misplaced nonces are exercised, and the persisted-field change is reflected in the living architecture document (`crates/core/tests/multipart_segment_nonce.rs:129`, `crates/core/tests/multipart_segment_nonce.rs:144`, `crates/core/tests/multipart_segment_nonce.rs:168`, `docs/design/architecture/05-building-block-view.md:202`). |
| T3 Runtime | PASS | Nonce validation and the 32-byte clone add bounded work without new clocks, global state, external awaits or destructive paths; all 28 affected custodian simulation tests pass independently (`crates/core/src/metadata.rs:998`, `crates/core/src/multipart.rs:2228`, `reviewer-evidence/dst.log:276`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit remains mandatory at publish, exactly as the deferred gate records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The tests exercise the production decoder and concrete key identities; affected-path history and closed work were checked, and the recorded rejection of placement B is respected without expanding scope (`crates/core/tests/multipart_session_records.rs:860`, `crates/core/tests/multipart_session_records.rs:876`, `reviewer-evidence/prior-art.log:3`, `reviewer-evidence/prior-art.log:71`, `brief.md:139`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this codec/API prerequisite as sufficient for #840's intended place in the multipart rollout — Create/nonce reservation and lifecycle cleanup still depend on the explicitly separate writer and downstream slices (`brief.md:85`, `brief.md:118`, `docs/design/architecture/05-building-block-view.md:202`). |

The evidence supports the scoped change, with one coverage-selection discrepancy and one reviewer-host limitation. Source citations above resolve against `$PDCA_TARGET` (the supplied `target/` copy); brief and log citations resolve in this review directory. The target is readable and matches every patch postimage. Stashing left the new regression file in place; popping the stash restored the tracked patch exactly. No source fix was made (`reviewer-evidence/integrity.log:1`).

The frozen coverage failure is real for its selected tests, but does not establish an implementation or test defect. `gate-logs/C4-diff-cov.log:14` selects only `multipart_segment_nonce`; the brief explicitly places new-API assertions in the existing `multipart_session_records` file so the red leg compiles (`brief.md:58`, `brief.md:126`). The frozen result remains **FAIL: 6/20 instrumentable changed lines (30%)**, with 72 unscored lines (`gate-logs/C4-diff-cov.log:1075`). I reran `cargo llvm-cov test --locked --offline -p wyrd-core --test multipart_segment_nonce --test multipart_session_records --json`; all 54 tests passed, and every one of the 14 reported missing lines executed, including both accessor branches and `SegmentGroup::from_nonce` (`reviewer-evidence/coverage.log:1`, `reviewer-evidence/coverage-summary.log:4`). This explains the advisory metric without rewriting its frozen verdict.

The local full-CI attempt stopped on a host permission boundary after its earlier checks passed. Formatting, workspace clippy/build/tests, cargo-machete, spelling, documentation lint and rendering ran successfully; `cargo deny` then could not acquire an exclusive lock on the read-only advisory cache (`reviewer-evidence/ci.log:4`, `reviewer-evidence/ci.log:3026`, `reviewer-evidence/ci.log:3032`). This is a host caveat, not a patch verification failure. The frozen CI output explicitly shows all three deny invocations succeeding, the remaining structural checks and DST running, and the final all-checks-passed result (`gate-logs/C4-ci.log:3153`, `gate-logs/C4-ci.log:3164`, `gate-logs/C4-ci.log:3167`, `gate-logs/C4-ci.log:3765`). I additionally reran the affected custodian DST suite under `RUSTFLAGS="--cfg madsim"`; all 28 tests passed. Both declared external dependencies, `typos` and the docs renderer, were actually exercised, so neither remains undischarged.

The other frozen rows have readable evidence and require no missing-oracle escalation:

- **C4-verify: PASS**, corroborated independently above; its own log records 17 assertion failures before the fix and 17 passes afterward (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:144`).
- **C5-mutants: PASS as recorded**, with limited breadth: one caught mutant and five unviable mutants, not six demonstrated behavioral checks (`gate-logs/C5-mutants.log:10`).
- **T4-batch-review: PASS as recorded**; the captured summary reports zero blocking findings. It supplies no individual review transcripts, so the substantive judgments above rest on this review (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution: N/A**, deferred to the publish audit (`gate-logs/T4-contribution.log:10`).
- **host-tikv: PASS from captured compile evidence** for both feature selections, not a claim of service integration testing (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`).

The prior-art investigation used GitHub's read-only API for every affected path's merged history and relevant closed PR file lists among the latest 100 closed PRs, including unmerged #647. `multipart.rs` still last changed at `4533549`; no duplicate session-nonce implementation appeared in the inspected history. The brief records the rejected #809/#637 iterations and the subsequent human placement decision (`reviewer-evidence/prior-art.log:13`, `reviewer-evidence/prior-art.log:71`, `reviewer-evidence/prior-art.log:102`, `brief.md:139`, `brief.md:152`). Those settled decisions are not raised again. The supplied target contains no `INTEGRATION.md` with additional human-only checklist items.

### Advisory — adversary

# Adversarial review — #840 (809.2): every session record carries its segment-group nonce

**Verdict: could not refute.** I attacked the evidence, the fix, and the gate claims. None of
them broke. No NEEDS-HUMAN items.

## The evidence (red→green re-run myself, in a scratch copy of `$PDCA_TARGET`)

- Attempted to refute the red leg: reverted `crates/core/src/{multipart,metadata}.rs` to the
  pre-fix base and kept `crates/core/tests/multipart_segment_nonce.rs`. **17 of 17 failed**, by
  assertion, with `unknown field segment_nonce`. With the fix, **17 of 17 passed**. The
  `C4-verify` claim holds. Every red failure comes from leg (a)'s positive arm
  (`crates/core/tests/multipart_segment_nonce.rs:113`), so (b)–(d) never ran their negative
  arms on the base. The brief says so up front (Falsifiability), so this is not a hidden gap.
- Attempted to show the test copies production instead of calling it: it doesn't. Every leg
  calls the real `decode_session_record` (`crates/core/src/multipart.rs:2315`) and
  `metadata::encode`, and also checks that the store-wide `metadata::decode::<SessionRecord>`
  agrees.
- Attempted to show leg (c) is a tautology, using a hand mutant: `de_segment_nonce`
  (`crates/core/src/multipart.rs:2085`) lowercases the input before validating it, so an
  uppercase nonce gets through as a second spelling. **Caught** in all four states
  (`multipart_segment_nonce.rs:144`, `leg_c`): S2 answers `NoncanonicalRecordValue` where the
  test expects `MalformedRecordValue`, and S1 accepts where the test expects an error.
- Attempted to find an old negative test that now passes for the wrong reason, i.e. refused
  for the missing nonce instead of its intended fault. There are none. Every hand-built session
  record in the workspace (grep on `"object"` and `clock_source` across `crates/`) now carries
  the nonce. The negative legs that reuse those builders pin their own needle or variant: 1j at
  `multipart_session_records.rs:499`, the `null` content type at `:647`, and field order at
  `:713`. Every custodian and DST builder asserts that its record decodes
  (`crates/custodian/tests/staged_drain_status.rs:283`, `crates/dst/tests/custodian.rs:2767`),
  so a missed fixture would fail loudly, not quietly.

## The fix

- Attempted to break serialization identity (rubric: *Serialization identity*). The field is
  required, not optional, so there is no absent-or-default spelling. `SegmentNonce` serializes
  `transparent`, and its struct position (`multipart.rs:2179`) matches its position on the wire
  (`:2104`). A `\u`-escaped nonce decodes, then fails the canonical re-encode
  (`multipart.rs:1933`), so it is refused and never stored in a second spelling. A present
  `content_type` combined with the nonce round-trips (`multipart_session_records.rs:242`).
- Attempted to make `SegmentGroup::from_nonce` (`crates/core/src/metadata.rs:1044`) create an
  invalid group. It can't: `SegmentNonce`'s field is private (`metadata.rs:989`), and its only
  constructor validates (`:996-1006`). `SegmentGroup::new` already accepted any epoch, so no
  new state becomes possible.
- Attempted to make `attempt_segment_group` (`multipart.rs:2225`) name the wrong epoch. A
  mutant that uses `self.epoch` instead of `publish_target.epoch` would survive the tests, but
  it behaves identically: `try_from` refuses any record where the two differ
  (`multipart.rs:2288`). Not a defect.
- Attempted a production-reader regression. The one production decoder,
  `crates/custodian/src/reconstruction/staged.rs:274`, sends a decode fault to `Withheld`.
  `SessionRecord` has no constructor and nothing writes `mpu:` values yet, so there are no old
  stored records to break.
- Checked the design against 0016's lifecycle (`0016:508-520`, the fence rows in the batch
  table, and the state diagram at `:536-553`). `(nonce, publish_target.epoch)` is exactly the
  `seg:<g>:<E>` that the fence-release and `Completing → Aborting` rows retire. Returning `None`
  for `Completed` is correct: the terminal delete only needs the whole-group prefix
  (`seggrp:`/`seg:<nonce>:`), and the record carries that in every state.

## Gate claims the reviewer might have over-read (advisory; no action needed)

- `C4-diff-cov` "fail, 30%": this reflects how the tool ran, not a real gap. It ran only the new
  test file. The lines it reports as missed (`multipart.rs:2215-2235`, `metadata.rs:1044-1046`)
  are exercised by the tests at `multipart_session_records.rs:860`, `:876` and `:890`. I ran
  that file myself: 37 of 37 green.
- `C5-mutants` "pass": this is weak evidence. It made 6 mutants, 5 of which did not compile and
  1 was caught, so it says almost nothing about legs (c) and (d). The hand mutant above covers
  (c). For (d), the "beside" arm (`multipart_segment_nonce.rs:201-208`) requires
  `unknown field` and so blocks a placement-B mutant. The "instead" arm (`:209-215`) only checks
  that the error names `segment_nonce`, so it would pass whether the error is "unknown" or
  "missing". That is a loose assertion, but the "beside" arm already covers it. Not worth a
  rebuild.

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in this diff.

The decoder reuses the existing nonce validator (`crates/core/src/multipart.rs:2082`), and the attempt-group accessor uses the validated nonce with the fence epoch without reparsing (`crates/core/src/multipart.rs:2225`, `crates/core/src/metadata.rs:1044`).

Evidence note: the non-gating 30% diff-coverage result measures only `multipart_segment_nonce` for core. The accessor assertions live in `crates/core/tests/multipart_session_records.rs:860`, `crates/core/tests/multipart_session_records.rs:876`, and `crates/core/tests/multipart_session_records.rs:890`; the frozen `gate-logs/C4-ci.log` records all three passing. The frozen verification log also records 17 regression tests failing before the fix and passing afterward. Review used the supplied evidence and read-only target source; no gates were rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept this codec/API prerequisite as sufficient for #840's intended place in the multipart rollout — Create/nonce reservation and lifecycle cleanup still depend on the explicitly separate writer and downstream slices (`brief.md:85`, `brief.md:118`, `docs/design/architecture/05-building-block-view.md:202`).
- [x] **The Completing-only contract leaves the nonce's reservation lifecycle unresolved.** `brief.md:37-38` requires rejecting `segment_nonce` in every other state, and `brief.md:60-61` assigns its writer to Complete (#658). However, the target's `docs/design/proposals/draft/0016-multipart-commit-protocol.md:508-518` requires an independent nonce reserved at Create and available at terminal deletion, including an upload aborted before Complete and a flat-map publication; the batch inventory repeats this at `:656` and `:673`. `crates/core/src/metadata.rs:1016-1018` likewise describes the nonce as minted with the session. The current session's common fields have no such identity (`crates/core/src/multipart.rs:2078-2088`), and `Open`/`Aborting` carry no fields (`:2019`, `:2033`). Adding it only to `publish_target` therefore supplies the attempt-fence identity but does not explain how the reserved identity survives outside that state. Before fixing this exclusion into acceptance tests, revise the brief to distinguish attempt identity from any separately persisted reservation identity and name the follow-up owning that storage, or document the decision that changes reservation/cleanup timing. This is a schema/lifecycle decision beyond merely deferring writer implementation; it need not expand this slice into implementing those writers.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
