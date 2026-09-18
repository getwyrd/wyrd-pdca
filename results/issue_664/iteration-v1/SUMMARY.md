# Result — issue 664 / staged-drain-and-restore-fence

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: four gaps, all on the operator-facing and post-restore side.
  1. **Drain status ignores staged bytes.** `reconciliation_status` answers `Satisfied` for a
     server holding only staged bytes, because `genuinely_holds` reads committed placements
     alone (`crates/custodian/src/desired_state.rs:188-196`). An operator is then told the
     server may be wiped under a live upload — the F6 trace. Its sharper form is an in-flight
     part with no `part:` record yet (`0016:827`).
  2. **Restore's report cannot tell staged skips apart.** Once #662 lands, restore skips staged
     fragments silently, through the shared gate (`restore.rs:383`). 0016 requires the report to
     say so: `staged_skipped` and `sessions_fenced` beside `pending_skipped` (`0016:823`;
     `RestoreReport`, `restore.rs:107-169`).
  3. **Restore fences no session.** A restored image can resurrect an `Open` or `Completing`
     session whose bytes are gone, and nothing stops it from completing over them (D-B,
     `0016:717-728`, F13). A `Completing` session that had already written segments needs its
     `seg:` records retired in the **same** batch as its fence, or they have no deleter anywhere
     in the design (X57, `0016:880`). Today the session record cannot even name them
     (`PublishTarget`, `multipart.rs:1842-1849`).
  4. **No durable record tells a gateway the fence has run.** 0016 requires the restore-fence
     generation to complete before any gateway serves multipart verbs on the restored image
     (`0016:723-728`, `:3017-3021`, X17b).
- Success criterion: the NEW file `crates/custodian/tests/staged_drain_restore.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes in
  `crates/core/tests/multipart_session_records.rs:81-145`). A `Completing` fixture carries the
  new nonce field. Base decoding rejects it (`#[serde(deny_unknown_fields)]`), which is harmless
  there: base restore never reads `mpu:`. Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class passes
  one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. v1's `*server != dserver` mutant survived every
  leg; this case kills it. It is green on the base as well — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For
  a draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The
  red comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass
  returns there, and why it does not tell an operator the drain is done.
  **(E) Restore reports staged skips.** `reconcile_after_restore` over a store with two staged
  fragments reports them as staged-skipped, separately from `pending_skipped`. The test cannot
  name a field this slice adds, or it would not compile on the base, so assert it through the
  report's `Debug` rendering. The rendering must contain `staged_skipped: 2` (0016's name for
  the counter, `0016:823`); the base rendering has no such counter.
  **(F) Restore fences a resurrected `Open` session (D-B).** An `Open@E` session in the store
  ends as `Aborting@E+1`. In the same batch — assert atomicity with a double that fails that one
  commit, after which **none** of the writes are present — its byte-retirement obligation is
  installed. Round-trip every obligation the fence writes through `decode_retire_obligation`
  (`crates/core/src/multipart.rs:3333`) against the key it sits under, and assert it decodes.
  The fenced-session counter moves: the `Debug` rendering contains `sessions_fenced: 1` (0016's
  name, `0016:823`). A Complete retried against
  that session cannot fence it, since the Complete fence requires `Open@E` (`0016:660`). The
  client-visible `4xx` is #658's to answer.
  **(G) Restore fences a resurrected `Completing` session with its segments' deleter (X57).** A
  `Completing@E` session with `segments_written > 0`, its nonce on the record, and
  `seg:<nonce>:<E>:*` records present ends as `Aborting@E+1`. One batch installs
  `retire:bytes` naming the session and its parts, **and** `retire:records` naming exactly
  `seg:<nonce>:<E>` (`0016:665`, the "one shape for all three doors" row). Both decode through
  `decode_retire_obligation`, and the records obligation's `segments()` names that group. v1
  installed only the first and reported the records as residue. That draining empties the range
  is #659's drain to prove (it stays on #665).
  **(H) What cannot be fenced cleanly is never passed off as done.** Two cases.
  (i) A `Completing` record with **no** nonce — the pre-decision shape — fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds — a part record missing from the restored image — is still fenced, and still named as
  needing a human. v1 silently built the teardown from whatever `part:` keys were present
  (`results/issue_637/iteration-v1/review-batch.md`).
  In both cases `RestoreReport::needs_human()` is true (`restore.rs:197`), and the fence
  generation (leg I) is **not** marked complete.
  **(I) The restore-fence generation record.** Three arms, each on durable state: (i) before any
  post-restore pass, the record is absent; (ii) **during** a pass, read through a double hook at
  the first fence commit, it names the pass's generation and reads not-complete; (iii) after the
  pass, it reads complete for that generation, and only if leg H found nothing. A second pass
  **advances** the generation and reads not-complete until it finishes, so a later restore
  invalidates an earlier completion instead of being masked by it. "Complete" becomes
  observable only after every write the pass makes, the mark batches included. How a gateway
  acts on the record is #508's: document the record's key and shape for it, and keep one source
  of truth.
  **(J) The session record carries the nonce, and nothing else changes for it.** A `Completing`
  session record with the nonce round-trips byte-identically through its codec, and one without
  it is refused. Put this in the codec's own test module in `crates/core/src/multipart.rs`; it is
  green-only by nature, which is fine for a codec leg.
  **(K) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: four things. (1) Drain status counting both staged classes as held. (2) Rebalance
  confirmed disjoint from the staged set: a code change only if it currently moves or rewrites
  staged records. (3) Restore's staged accounting and its session fence, both shapes, with the
  obligations 0016's rows name in one batch each. Any writer-side construction added for those
  obligations must produce only values `decode_retire_obligation` accepts against their key,
  since the module withholds a writer on purpose (`multipart.rs:3036-3060`). (4) The durable
  restore-fence generation record, the nonce on the `Completing` session record (option (i)),
  and the post-restore command's report of fenced and unfenceable sessions
  (`crates/server/src/cli.rs`, whose tests build `RestoreReport` literals at `:2885-2990`).
  `RestoreReport` gains its fields plainly, with no `#[non_exhaustive]` (decided at Plan: it
  derives `Default` and is only built inside the workspace). Record the nonce decision in the new
  field's doc comment, citing the `0016:354` / `:2333` disagreement it settles. Docs currency
  (`AGENTS.md:154-157`: new persisted fields and a new persisted record): describe the fence,
  the generation record and the nonce in `docs/design/architecture/06-runtime-view.md`, and the
  post-restore exit reasons in `docs/design/architecture/m4-first-deployment-blueprint.md`.
  Must NOT change the signatures of `reconcile_after_restore`, `reconciliation_status` or
  `reconcile_step`, and must NOT add a field to any context struct. / out of scope: scrub and
  reconstruction (child-3 — do not touch `scrub.rs` or `reconstruction.rs`); the staged set
  itself and the mark codec (child-2); the retire drain that empties the obligations (#659); the
  gateway's reading of the generation record (#508); evacuating committed segmented objects
  (#653/#722); client Abort and Complete (#656, #658); any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.9% — 497 of 535 instrumentable changed lines executed (floor 80%); 535 of 1453 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 74 mutants tested in 3m: 9 missed, 25 caught, 40 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_664/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.01s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review outcome: the staged-drain and post-restore fencing change is not contribution-ready because a residue-bearing fence is forgotten on rerun, although its intended first-pass behavior reproduces red→green.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted contract is coherent and falsifiable: drains must count both staged classes, and restore fencing must complete before gateways serve multipart verbs (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:827`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:3017`). |
| C2 Reproduction (red pre-fix) | PASS | An independent production-only stash run executed 12 focused tests and failed 11 by assertion while the uploads-live-elsewhere guard stayed green; the first required drain leg begins at `crates/custodian/tests/staged_drain_restore.rs:517`. |
| C3 Change | PASS | The change stays on the planned surfaces: the drain unions committed and staged placements at `crates/custodian/src/desired_state.rs:241`, restore commits each fence with its obligations at `crates/custodian/src/restore.rs:940`, and the durable nonce/generation shapes live at `crates/core/src/multipart.rs:2012` and `crates/core/src/multipart.rs:3687`. |
| C4 Verification (red→green) | PASS | Independent reruns produced 11/12 red before the production fix and 12/12 green after it; frozen full CI and 92.9% diff coverage are green (`gate-logs/C4-verify.log:112`, `gate-logs/C4-ci.log:3593`, `gate-logs/C4-diff-cov.log:1201`), while the local CI rerun stopped only on a read-only host advisory-database lock. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must prove malformed part/segment handling and retry persistence: nine mutants survive in the residue planner (`crates/custodian/src/restore.rs:1035`), and the residue test checks only the first pass (`crates/custodian/tests/staged_drain_restore.rs:1027`). |
| T1 Structure | PASS | Dependency direction remains narrow: core owns validated persisted shapes, custodian owns the restore writer, server owns operator reporting, and the drain reuses the shared staged-set seam (`crates/custodian/src/desired_state.rs:232`). |
| T2 Shape | PASS | The new nonce and restore-generation records are structurally validated and canonical, and the living architecture documentation identifies their durable roles (`crates/core/src/multipart.rs:2031`, `crates/core/src/multipart.rs:3797`, `docs/design/architecture/05-building-block-view.md:202`). |
| T3 Runtime | FAIL | An unresolved teardown can be falsely certified on rerun: the first pass records residue only in memory after changing the session to `Aborting` (`crates/custodian/src/restore.rs:940`), the next pass skips that state (`crates/custodian/src/restore.rs:916`), and the now-clean report permits `mpufence` completion (`crates/custodian/src/restore.rs:750`) without expanding the already-installed explicit part obligation. |
| T4 Contribution | FAIL | Contribution is not ready because the batched review independently converged on the same retry-certification defect (`gate-logs/T4-batch-review.log:10`); the separate PR-description audit is correctly N/A/deferred until publish (`gate-logs/T4-contribution.log:10`), while affected-path merged/open/closed history exposed no competing implementation. |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must retain or re-derive a fenced session's residue across generations and add a repair-and-second-pass regression, because the documented operator remedy is to repair and rerun (`docs/design/architecture/m4-first-deployment-blueprint.md:640`) but the current state skip makes that rerun certify unchanged residue (`crates/custodian/src/restore.rs:916`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether runbook-only sequencing is an acceptable production fence until #508 adds the gateway reader — this slice writes `mpufence`, but its own contract says reading and enforcement remain external (`crates/core/src/multipart.rs:3704`). |

### Advisory — adversary

# Adversarial review — issue 664 (staged-drain-and-restore-fence)

Evidence re-run in a scratch clone of `$PDCA_TARGET` (`cargo 1.96.0`, in-memory doubles only):
the patch's 12 tests pass post-fix, and the frozen `gate-logs/C4-verify.log` shows 11 of them
failing **by assertion** on the reverted base against the real `reconciliation_status` /
`reconcile_after_restore` / `reconcile_step` entry points — no mock stands in for the production
path, no tautology, no compile failure. The red→green itself survives attack. What follows is
where the *fix* does not.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:916`: a second pass over an
  **unrepaired** store certifies the restore fence complete.** `fence_one_session` returns early
  for `SessionState::Aborting {}`, and a session fenced *with residue* in pass 1 is `Aborting` in
  pass 2 — so `sessions_fenced_with_residue` comes back empty, `needs_human()` is false, and
  `complete_fence_generation` (`restore.rs:750`) stamps the record. Re-ran the H(ii) fixture twice
  with nothing changed between the passes: pass 1 → `sessions_fenced_with_residue:
  ["mpu:2b2b…"], mpufence = {"generation":1,"complete":false}`; pass 2 → report empty,
  `mpufence = {"generation":2,"complete":true}`, while the `seg:` record naming chunk `0x0A06` is
  still in the store and the only obligation still reads `parts: Set([(1,1)])`. This directly
  contradicts the fix's own doc at `restore.rs:743-744` ("the record stays at this generation, not
  complete, so the image reads as unfenced until the named records are repaired") and the brief's
  invariant that an image is declared fenced only when every record a resurrected session wrote has
  a named deleter. Under #508's gate, a gateway would read `complete` and serve multipart verbs.
  (The T4 batch review reached the same line from three independent passes; this is the executed
  proof of it.)

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1079` + the instruction the operator is
  given at `crates/server/src/cli.rs:1317` / `docs/design/architecture/m4-first-deployment-blueprint.md:640`:
  doing exactly what the runbook says makes the outcome *worse*, silently.** The `Completing`
  teardown freezes an explicit `PartScope::Set` from the part records present at the first fence,
  and the fence is never revisited. Probe: fence a `Completing` session whose `seg:` record names
  chunks `{held, orphaned}` while only part 1 (`held`) survives → residue reported. Then do what
  the NEEDS-HUMAN line says — seed the missing `part:` record for `orphaned` and re-run. Result:
  `retire:bytes:s:<id>:<E>` still reads `parts: Set([(1,1)])` (part 2 is *not* added — the session
  is `Aborting`, so `restore.rs:916` skips it), yet `mpufence` flips to
  `{"generation":2,"complete":true}`. The restored part's bytes now have no deleter *and* the store
  certifies itself fenced. The `Open` teardown is immune only because it uses `PartScope::All`.

- **NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:2380-2394` bakes the false generalization
  in.** The DST property re-runs the pass only after a *racing well-behaved fencer* already
  installed a complete obligation, then asserts "a re-run over the settled store" certifies —
  concluding in its header comment (`:2200`) that "the withholding is a withholding rather than a
  pass that can never certify". It never re-runs over a store where the blocker was **not** settled,
  which is the case above. A reviewer reading this property would reasonably believe re-run
  behaviour is covered; it is not. The cheapest fix to the *evidence* is a second arm that re-runs
  over an unrepaired residue store.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1067` and `:1085`: the residue predicate
  and the cursor/range disjunction are asserted in prose and by nothing else.** `C5-mutants` lost 9
  mutants, all in `completing_plan`: `||`→`&&` at `:1067` and `:1085`, `>`→`==` at `:1085`, and
  `+=`→`-=`/`*=` on the two unreadable counters at `:1035`, `:1039`, `:1062` (a `-=` on a `usize` at
  0 would panic in debug, so those lines are executed by **no** test — `C4-diff-cov.log` lists the
  same lines as MISS). Concrete cases nothing covers: (a) a `Completing` session with
  `segments_written: 0` and `seg:` records present — the `&&` mutant drops the `retire:records`
  obligation and the segment records lose their deleter, which is the X57 defect this slice exists
  to close (I confirmed production gets it right, so only the test is missing); (b) a `Completing`
  session whose `part:` key parses but whose value will not decode, with every `seg:` chunk covered
  — the `unreadable_parts` arm of the residue sentence, promised at `restore.rs:229-230` and in the
  CLI's FENCE RESIDUE paragraph, is never exercised.

- **NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:750`: the multipart fence certification is
  gated on findings that have nothing to do with multipart.** `complete_fence_generation` withholds
  on the whole of `needs_human()`, which includes `dangling` and `misplaced`. Probe: one cleanly
  fenced `Open` session plus one committed object whose fragment the restore did not bring back →
  the session is fenced correctly in pass 1, and every pass thereafter reports `dangling: [1794]`
  and leaves `mpufence` at `complete:false` forever. Since the runbook now says "do NOT re-enable
  the S3 gateways until a run reports the fence complete"
  (`m4-first-deployment-blueprint.md:632`, `:722-730`), any restore that actually lost data — the
  case this pass exists for — keeps multipart disabled until the operator deletes each lost object.
  The brief asked for the narrower rule (leg I(iii): complete "only if leg H found nothing", i.e.
  the *fence* findings). Whether the stricter coupling is the intended product behaviour is a
  scope/fitness call, not something Do should silently pick.

## Claim in the bundle I think is overstated

- `check-gates.json` C4-verify row reads "red without the fix, green with it (12 test(s) ran red)".
  `gate-logs/C4-verify.log:109` reads `1 passed; 11 failed`. Leg C is a guard and is green on the
  base by design (brief, leg C), so the count of legs that *earned* a red is 11, not 12. The gate
  still passes on its own terms; only the number quoted in the row is wrong.

## Attacked and could not refute

- **The red is real and on the production path.** Every base failure in `C4-verify.log:33-94` is an
  assertion against the shipped functions (`Satisfied` vs `Pending`, a `Debug` rendering with no
  `staged_skipped`/`sessions_fenced`, a probe that never fired). Nothing is mocked away.
- **Paging the `mpu:` listing** (`restore.rs:858-867`, `STAGED_PAGE = 512`). Seeded 600 `Open`
  sessions: `sessions_fenced = 600`, 0 still `Open`. The cursor hand-off is correct even though no
  test in the patch reaches the continuation arm (`restore.rs:864`, a diff-cov MISS).
- **The obligation writer** (`multipart.rs:329`): `encode_retire_obligation` re-decodes against the
  key it mints, so an obligation the #659 drain would refuse cannot become durable. Tried the
  `Completing` path with zero surviving parts and with a lying cursor; every installed value
  decoded back through `decode_retire_obligation`.
- **`MPUFENCE_KEY = b"mpufence"`** does not collide with `scan(b"mpu:")` (4th byte `f` ≠ `:`), so no
  session listing — including the fence's own — can pick it up.
- **Atomicity legs F/G**: the injected-fault double genuinely refuses the whole batch, and neither
  the session record nor either obligation survives it; the assertions are not vacuous.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The rebuild must prove malformed part/segment handling and retry persistence: nine mutants survive in the residue planner (`crates/custodian/src/restore.rs:1035`), and the residue test checks only the first pass (`crates/custodian/tests/staged_drain_restore.rs:1027`).
- [ ] T5 Judgment — The rebuild must retain or re-derive a fenced session's residue across generations and add a repair-and-second-pass regression, because the documented operator remedy is to repair and rerun (`docs/design/architecture/m4-first-deployment-blueprint.md:640`) but the current state skip makes that rerun certify unchanged residue (`crates/custodian/src/restore.rs:916`).
- [ ] Validation — fitness-to-purpose — Decide whether runbook-only sequencing is an acceptable production fence until #508 adds the gateway reader — this slice writes `mpufence`, but its own contract says reading and enforcement remain external (`crates/core/src/multipart.rs:3704`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_664/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 206 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Slice is oversized (206 KB patch vs 100 KB threshold) and the advisory review (both rubric and adversary passes) converged on a real correctness bug: crates/custodian/src/restore.rs:916 skips already-`Aborting` sessions on a second fence pass, so a repair-and-rerun (the documented operator remedy) can certify the restore-fence generation complete while a `Completing` session's orphaned seg: residue still has no deleter — a gateway trusting that marker would resume multipart verbs over an unfenced image. Given the size flag, treat this as a slicing problem, not an implementation bug to patch in place: split at re-plan (drain/rebalance staged-accounting vs. restore fencing + generation record vs. residue-across-generations handling) rather than iterate-do, so the rebuild doesn't keep producing implementation-shaped findings on an oversized diff. Carry into the split: residue must survive re-fencing across generations, and a repair-then-second-pass regression test is required before the fence-complete marker can be trusted.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
