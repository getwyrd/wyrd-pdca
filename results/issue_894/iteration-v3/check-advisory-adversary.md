# Adversarial review — issue 894 (multipart-admission-create)

**Verdict:** I could not break `create_session` on any path its tests cover, or find a wrong answer it gives on `main`. I did find two decision-table rules that the shipped suite does not pin. For each I wrote a probe test in a scratch copy: it passes on the patch and goes red under a mutation that the 35 core legs and the C5 mutants run (58 mutants, 0 missed) all let through. Separately, the gating T4 finding is real in principle, but it can't be reached on `main` and leaks nothing. Deciding it means changing the brief's decision table, so it goes to the human.

## Findings

- NEEDS-HUMAN [human] — **T4-batch-review's blocking finding (`crates/core/src/multipart/create.rs:347-373`).** The scenario: a commit returns `CommitUnknownResult { may_still_commit: false }` after it applied, and another writer then rewrites `mpu:<id>` away from R (for example `Open@0` → `Aborting@1`) before the re-read. The re-read finds a record that is not R, with nothing held. Line 352 clears the doubt, lines 363-373 treat it as a collision and re-mint, and the caller gets `Created` for a second session. Nobody is told about the first one. The code is doing what brief row 3 says ("not held: collision"), so this is a question about the table, not an implementation slip. On `main` it can't happen:
  - The only writer that rewrites an `Open` session is the post-restore fence (`crates/custodian/src/restore.rs:1119`). That fence runs "with writers stopped, before any gateway is re-enabled" (`docs/design/architecture/05-building-block-view.md:208`; 0016 X17).
  - Abort (#895), the reaper (#625) and operator abort (#633) don't exist yet.
  - Even if the race happened, nothing leaks. The fenced session's `retire:` obligation was committed with its fence, so the drain (#659) reclaims it and decrements `mpuctl`. The only thing that breaks is the sentence "no session it installs goes unreported" (`create.rs:15-19`, `05-building-block-view.md:206`).

  My suggestion: reject it for this slice with that reason recorded. Give #625 and #633 a note that a create seeing its own record fenced will re-mint, or that slice can refine row 3 (for example, a record that carries this identity's nonce counts as this create's own).
- NEEDS-HUMAN [impl] — **No leg pins `may_still_commit` when the create is both held and unsettled and its reads then fail (`create.rs:521-523`, reached via `:342` → `finish` `:570-578`).** The settling rule says `may_still_commit` is `true` exactly when the create is held. If `standing()` were changed to prefer `unsettled` over `held`, every one of the 35 legs still passes. Yet that change makes the create report `InDoubt { may_still_commit: false }` ("may have been applied") while a batch can still land. The gap is in the tests:
  - F6 (`crates/core/tests/multipart_create.rs:2288`) does reach held + unsettled, but its next read succeeds and clears `unsettled` at `create.rs:352`.
  - F8 (a) (`:2399`) is held only.

  Failing case: plan `[Hold, Unknown { applied: false, may_still_commit: false }]` on the shipped profile, with `GetFault { after_commits: 2, sessions_only: false, skip: 0, fail: usize::MAX, transient: false }`. Expect `in_doubt(outcome, &original, true)`, then `assert_held_batch_lands`. I ran this as a scratch probe: it passes on the patch and fails under the swap at `multipart_create.rs:359` (`unknown.may_still_commit` was `false`). Add it as an F8 (e) leg.
- NEEDS-HUMAN [impl] — **No leg pins the read order "bucket, then ledger, and only then contention" (brief, Design § Order; `create.rs:415-426` must stay after `:376-413`).** Moving the `lost` block above the bucket check passes all 35 legs. With that change, a lost commit that met a full ledger on the last budgeted attempt answers `AdmissionContention` instead of `SessionCap`, and every refusal met after a loss costs one extra wait. Failing case: `profile_of(1)`, plan `[AdmitFirst(1)]`, starting with no ledger. The right answer is `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })` with `script.waits` empty. Scratch probe: green on the patch, red under the move. The cheapest fix is that probe as a (B) leg, plus `assert!(script.waits.is_empty())` in `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (`multipart_create.rs:1174-1199`). This is low impact, since both answers are a 503 SlowDown, but the brief states the order as a rule.
- (Note, no action needed.) The `InDoubt::outcome` doc says it is "the first that said `may_still_commit`" (`create.rs:130-132`). Changing `create.rs:514` to keep the latest one instead also survives, because every scripted outcome carries identical fields. Only the audit `detail` would differ, and the answer the caller acts on does not change.

## What I tried and could not break

- **Re-ran the evidence.** In a scratch copy, the core suite is 35/35 green. Brief table rows 5 and 6 go red as claimed (row 5 fails C4; row 6 fails C2, C3, C4 and F3).
- **Extra mutations, all caught:**
  - dropping the `unsettled` clear at `:352` (caught by f9 1021-settled);
  - keeping `lost` after a re-mint at `:372` (c2, c3);
  - ignoring a changed marker at `:378` (B marker-moved);
  - `>` for `>=` at `:419` (B contention, f6, f8c);
  - settling only while held at `:570` (f9);
  - dropping the settle read in the held, identity-taken branch at `:358` (f5, between reads);
  - answering `Created` on any successful read there (c6 ii).

  Making `:347` adopt R only while held or unsettled (instead of after any unknown outcome) also survives. That mutation is equivalent outside the stated same-identity, same-request bound, so it is not a gap.
- **Size headroom.** The test builds each state's widest form itself (`multipart_create.rs:1329-1374`) and decodes each through the production decoder. It does not copy production code. Production (`crates/core/src/multipart.rs:2393-2436`) sets every field to the widest the decoder allows: ETag `N ≤ MAX_PART_NUMBER` (`:4296-4302`) and `segments_written ≤ MAX_SEGMENT_INDEX + 1` (`:2613`). `created_at_millis`, `clock_source`, `object` and `content_type` never change after Create. I found no field it under-counts.
- **The new creation route.** The session decoder checks no string content other than the nonce (`multipart.rs:2603-2644`). So any request strings encode and decode back to the same bytes, and `SessionRecord::open` cannot install a session that later fails to decode.
- **DST.** The witness assertions (an unapplied 1021, 1031 then 1021, a held in-doubt batch that lands) live in plain `#[test]` sweeps (`crates/dst/tests/multipart_admission.rs:483-570`), and `C4-ci.log:4109-4118` shows them running under `--cfg madsim`. `defer_in_flight_landings` (`crates/dst/tests/support/mod.rs` diff) picks one legal FDB schedule ("could get committed at a later point in time") and stays off by default.
- **C4-verify `unverifiable`.** This was declared up front: the test calls an API the patch adds, so it can't compile on the old code. That is not a refutation.
- **Rubric checks.**
  - No `static` in the wrappers.
  - No clock read in core.
  - Docs-currency paragraph updated.
  - The bucket deletion-fence key is deferred to #511 with a `// deferred:` marker (`create.rs:641`). That counts as settled.
  - Awaits on the store have no timeout of their own. Sign-off already settled that by adding the cancel-safety paragraph (`create.rs:306-313`).

<!-- pdca:leaf-complete -->
