# Result — issue 814 / staged-replace

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: after #813, reconstruction recognises a committed part's chunk but can only keep
  its obligation. Nothing rebuilds the fragment, so the part stays a fragment short until it is
  published, or forever if the client never completes. 0016 requires the rebuild (`0016:825`),
  and its failure table names "scrub staged fragments but leave reconstruction committed-only" as
  a wrong implementation (`:889`).
- Success criterion: the NEW file `crates/custodian/tests/staged_repair.rs` passes, one
  seeded case appended to the existing `crates/dst/tests/custodian.rs` passes, and
  `cargo xtask ci` is green. The new test names only symbols on its base, including
  `ReconstructionContext::{clock, staged_write_window_millis}` from #813, with time from a
  `wyrd_testkit::ManualClock` (`crates/testkit/src/lib.rs:49`). The D-server doubles — this
  file's and the DST's — **enforce** the deadline `put_fragment` carries, refusing at or after it
  through `WriteDeadlineExpired::if_elapsed` (`crates/traits/src/lib.rs:925`), as the real D
  server does since #638. v1–v2's doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`). Legs:
  **(A) The whole rebuild.** An `Open` session's committed part has one fragment lost and its
  obligation queued. One `reconcile_step` with a `ReconstructionContext` answers `Changed`, and:
  a new D server holds an intact fragment with the right scheme
  (`wyrd_core::repair::header_matches_identity`, `crates/core/src/repair.rs:58`); the `part:`
  record's `ChunkRef.placement` names that server; the destination's pre-mark `orphan:<P_new>` is
  gone; the vacated `P_old` carries an `orphan:` mark; the obligation has drained. A changed
  placement alone is not enough.
  **(B) Losing branches strand nothing (X29, `0016:888`).** A `put_fragment` hook moves the
  session from `Open@E` to `Aborting@E+1` after the destination write and before the adoption
  CAS. Then: no adoption, the `part:` record byte-identical, the pre-mark still there, the
  obligation queued. Repeat with the `part:` record rewritten instead of the session fenced.
  **(C) The pre-mark and deadline rules (`0016:1285-1358`), one case each:**
  (i) the pre-mark is durable before the destination write: the double checks `orphan:<P_new>`
  is present when `put_fragment` arrives;
  (ii) a destination position that already carries a mark from another event, or a legacy mark,
  is re-stamped fresh, never reused with its old stamp;
  (iii) a position with a `reclaiming` mark is never written, and ruling it out removes only that
  position, not its server: RS(2,2) with two lost fragments, two free domains and a stale
  `reclaiming` mark on one candidate position repairs both in ONE pass (v2's bug: excluding the
  whole server stalled this forever while every pass answered `Satisfied`);
  (iv) the write deadline is the time the context clock reads when the pre-mark commits, plus
  `staged_write_window_millis` — never the pass-start time. With the clock moved on between pass
  start and pre-mark, the deadline is still live (v1's stall). A write the double refuses as
  expired aborts the re-place: no adoption, pre-mark still there, obligation queued;
  (v) no destination write is authorized on a pre-mark older than `W_repoint`
  (`0016:1339-1349`). A hook moves the clock past it between pre-mark and write, and no write on
  the stale pre-mark is ever adopted. Enforcing this through the deadline the D server checks is
  accepted (v1 sign-off);
  (vi) a vacated `P_old` whose existing `orphan:` value decodes as none of the three shapes aborts
  the move before its CAS, keeps the obligation queued and names the fault; it is never
  overwritten (ADR-0045).
  **(D) The drain fence on the destination (`0016:885`).** A server with ANY
  `desired:dserver:<S>` record (`crates/custodian/src/desired_state.rs:36`), whatever its value —
  `maintenance` included — is never chosen, so selection and the CAS test the same fact (v1's
  repro: `maintenance` stalled four passes, all answering `Satisfied`). A drain recorded between
  selection and the adoption CAS makes the CAS lose (`require_absent(desired:dserver:<S_new>)`):
  not adopted, pre-mark still there.
  **(E) Kept, not rebuilt.** An `sidx:`-only chunk, and a chunk of a session that is not `Open`,
  keep their obligation and nothing is written (a repair blocked by a Complete is retried after
  publication, `0016:825`).
  **(F) Seeded DST for X29**, appended to the existing `crates/dst/tests/custodian.rs` (keep
  `#![cfg(madsim)]`, `:53`). Sweep the session fence across every point of the re-place: before
  the pre-mark, between pre-mark and write, between write and CAS, after the CAS. In every
  interleaving no fragment ends unreferenced and unevidenced, and the session never ends
  `Aborting` with a `part:` record naming a fragment that was not written. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`). Record the seed count
  in `build-notes.md`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: rebuild and re-place a committed part's chunk in an `Open` session: pre-mark before
  write; deadline from the context clock at pre-mark commit plus the window; the `W_repoint` gate;
  one adoption CAS pinned to the session state, the prior `part:` bytes, the pre-mark's bytes and
  `require_absent(desired:dserver:<S_new>)`. On any loss the obligation stays queued and the
  pre-mark stands. Time and window come only from #813's fields; add no other. Extend the
  last sentence of `docs/design/architecture/06-runtime-view.md:80` with the rebuild, and drop
  `#663` from the `deferred:` marker at `gc.rs:893`.
  A committed part's chunk in an `Open` session that is found at full redundancy resolves as the
  committed path resolves one: its obligation drains as a duplicate finding
  (`reconstruction.rs:216-218`, `Assessment::Drain`); "kept" is for a chunk that needs a repair
  this pass cannot make. Keep #813's legs D–F in `crates/custodian/tests/staged_protection.rs`
  passing: D's committed-part case asserts "kept, no write" for a committed part in an `Open`
  session, which this slice makes false — retarget it to a session that is not `Open` (leg E's
  rule here), or remove it, since `staged_repair.rs` leg A owns the rebuild assertion; D's
  `sidx:`-only case and control stay. E's chunk must be degraded (a fragment lost), so the pass
  reaches this slice's re-place and loses its CAS on the published `part:` record, rather than
  draining the chunk as one at full redundancy.
  / out of scope: the committed repair path (`reconstruction.rs:829-955`, unchanged); one
  degraded chunk per part per pass — every plan in a part pins the same `part:` bytes, as the
  committed path pins one inode (`reconstruction.rs:155-159`, `:294-303`); accepted here, say so
  in a comment; a late write whose effect is `WriteEffect::Unknown` landing after GC reclaims its
  pre-mark (a gap in the deadline model shared by every writer, `crates/traits/src/lib.rs:862-902`);
  `EcScheme::None` answering Unrepairable (by design, as `reconstruction.rs:641`); servers absent
  from the live fleet; `seg:` repair (#777); drain status, rebalance, restore (#808–#810); the
  upload-side drain fence (#657); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (20 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.1% — 444 of 467 instrumentable changed lines executed (floor 80%); 467 of 1157 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 12m: 8 missed, 26 caught, 48 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.90s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #814: rebuild committed multipart chunks under the Open-session fence; one confirmed repair-progress defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable rebuild, losing-race, deadline, drain-fence and seeded-race outcomes, with explicit exclusions; `brief.md:24`, `brief.md:100`, `brief.md:117`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new test reproduced the missing rebuild: 18 assertion failures, two passes, no compilation failure; `reviewer-red.log:165`, `crates/custodian/tests/staged_repair.rs:815`. |
| C3 Change | PASS | The change addresses the specified staged lifecycle and updates its living architecture description without adding context fields or changing the committed repair operation; `crates/custodian/src/reconstruction.rs:441`, `docs/design/architecture/06-runtime-view.md:82`. |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 20 new tests, 51 existing tests and both new DST tests across 50 seeds; full CI and coverage are supported by frozen logs, subject to the local rustfmt limitation below; `reviewer-green.log:90`, `reviewer-dst.log:248`, `gate-logs/C4-ci.log:3708`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Fix the shared pre-mark timing across multiple writes and add a latency regression — a successful 15-second first write prevents the second from ever being attempted on repeated passes, leaving the original redundancy deficit; `crates/custodian/src/reconstruction/staged.rs:636`, `reviewer-slow-writes.log:13`. |
| T1 Structure | PASS | The staged repair uses the existing metadata/chunk-store seams and a shared staged read, preserving source-before-destination ordering and avoiding a second inconsistent snapshot; `crates/custodian/src/gc.rs:1536`, `crates/custodian/src/reconstruction.rs:237`. |
| T2 Shape | PASS | Adoption preserves non-placement part fields and pins the exact session, part and mark bytes; no persisted field or public API shape is introduced; `crates/custodian/src/reconstruction/staged.rs:667`, `crates/custodian/src/reconstruction/staged.rs:732`, `crates/custodian/tests/staged_repair.rs:829`. |
| T3 Runtime | FAIL | F1 leaves a repairable RS(2,2) chunk degraded indefinitely while every pass reports Satisfied; four independent passes reproduced unchanged placement and a queued obligation; `crates/custodian/src/reconstruction/staged.rs:637`, `crates/custodian/src/reconstruction.rs:506`, `reviewer-slow-writes.log:13`. |
| T4 Contribution | FAIL | The batch review's slow-write finding is independently confirmed and unresolved; its two late-publication findings are outside #814's explicit scope, while the contribution-artifact audit is N/A until publish; `gate-logs/T4-batch-review.log:10`, `brief.md:120`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The review preserves the agreed split and exclusions, checks all eight affected paths against merged history and all 16 closed/unmerged PRs, and grounds the remaining defect in execution rather than mutation counts; `brief.md:145`, `reviewer-prior-art.json:2`, `reviewer-slow-writes.log:13`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether staged-upload recovery meets the intended operational recovery expectations after F1 is fixed — automated seam/DST evidence does not supply deployment fitness sign-off; consider Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up under the standing rubric; `AGENTS.md:78`, `brief.md:79`. |

F1 — **FAIL, high priority: successful slow writes can prevent staged reconstruction from making any progress.** All destinations receive one pre-mark timestamp before the write loop. The loop awaits each write before checking the next destination's pre-mark age. With two missing fragments moving to new servers, the first write can legitimately succeed after 15 seconds, inside the 30-second write window, yet exceed the second destination's 10-second authorization window. The function returns before the adoption CAS, so the next pass still reads the original placement and repeats the same first write. The timing constants are at `crates/custodian/src/gc.rs:202` and `crates/custodian/src/gc.rs:218`; the common stamp, sequential await and later adoption are at `crates/custodian/src/reconstruction/staged.rs:606`, `crates/custodian/src/reconstruction/staged.rs:641` and `crates/custodian/src/reconstruction/staged.rs:667`.

The independent scratch probe reuses the supplied deadline-enforcing doubles, with RS(2,2), survivors on servers 0/1, lost fragments on 2/3, and free destinations 4/5. A stored-write hook advances the shared ManualClock by 15 seconds. Across four passes it observed:

```text
pass=1 outcome=Satisfied now=25000 placement=[0, 1, 2, 3] writes=1 queued=true
pass=2 outcome=Satisfied now=40000 placement=[0, 1, 2, 3] writes=2 queued=true
pass=3 outcome=Satisfied now=55000 placement=[0, 1, 2, 3] writes=3 queued=true
pass=4 outcome=Satisfied now=70000 placement=[0, 1, 2, 3] writes=4 queued=true
```

Every written fragment remained named or marked in this probe; the demonstrated defect is failure to restore redundancy, accompanied by a misleading pass result. Preserve the stale-mark prohibition while arranging authorization/dispatch or fresh marking so that one successful write does not consume another destination's authorization window. Add a regression that combines multiple losses with successful write latency. The supplied two-loss test uses immediate writes, and its stale-mark test has only one missing fragment (`crates/custodian/tests/staged_repair.rs:1096`, `crates/custodian/tests/staged_repair.rs:1284`). Reproduce from this review directory:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-build" cargo test --offline \
  --manifest-path pdca-reviewer-814-probe/Cargo.toml \
  --test slow_writes reviewer_two_missing -- --nocapture
```

The probe is at `pdca-reviewer-814-probe/slow_writes.rs:1668`; its failed assertion is at line 1688. It calls the unchanged target production code. No fix was applied to the reviewed source.

The asserted red→green evidence reproduced independently. The disposable target was readable and matched the patch; its base was stashed, the new test retained, and the stash restored before verification. The restored tree passed 15 reconstruction tests, 36 staged-protection tests and 20 staged-repair tests (`reviewer-green.log:22`, `reviewer-green.log:64`, `reviewer-green.log:90`). With `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`, both new staged-replace DST tests passed, including the assertion that all four fence positions were reached (`reviewer-dst.log:245`, `crates/dst/tests/custodian.rs:4960`). Final `git apply --reverse --check ../patch.diff` and `git diff --check` passed; the target retains the supplied patch.

Every frozen gate log was available. The following records distinguish independent reruns from frozen evidence; C4's PASS describes the specified regression checks, not the additional failing F1 probe.

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-verify | PASS, independently reproduced | Base: 18 assertion failures; restored patch: 20/20 new tests pass; `reviewer-red.log:165`, `reviewer-green.log:90`. |
| C4-ci | PASS in frozen evidence; local host caveat | Frozen log ends with all checks passed and includes DST; the independent run passed typos, docs lint, 99-page render/link audit, gitlink and unsafe guards, then stopped because cargo-fmt is absent; `gate-logs/C4-ci.log:3708`, `reviewer-ci.log:19`. The statics scanner also passed separately (`reviewer-statics.log:3`). |
| C4-diff-cov | PASS in frozen evidence | 444/467 instrumentable changed lines executed, 95.1%; 690 other changed production lines were unscored, not proved covered; `gate-logs/C4-diff-cov.log:753`, `gate-logs/C4-diff-cov.log:789`. |
| C5-mutants | FAIL, advisory scanner result retained | 83 mutants: 8 missed, 26 caught, 48 unviable, 1 timeout; the log does not establish a production defect for each survivor; `gate-logs/C5-mutants.log:13`, `gate-logs/C5-mutants.log:22`. |
| T4-batch-review | FAIL; one confirmed finding remains | F1 is independently reproduced. The two remaining entries both concern late publication after GC has reclaimed evidence, explicitly excluded from this #814 slice by `brief.md:120`; their underlying deadline-contract context is #638 and `crates/traits/src/lib.rs:871`. They are declined for this slice, not claimed fixed; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | PR/commit contribution artifacts are drafted after Check; their substantive audit must rerun at publish; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS in frozen evidence | The log shows the TiKV crate and server feature selections compiled under clippy; this is compile evidence, not a live TiKV integration claim; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`. |

The brief's external dependencies, typos and docs-renderer, were exercised both in the frozen CI run and independently (`brief.md:128`, `reviewer-ci.log:2`, `reviewer-ci.log:10`). Missing local rustfmt does not invalidate the captured full-CI result or constitute a patch defect. Instance-scoped coverage/mutation/review wrappers were adjudicated from their supplied logs; no missing-wrapper finding is raised. No additional capability probe or eager-load workaround was found: session, mark, drain and deadline checks enforce the protocol's stated preconditions.

The mutation misses do not justify eight new defect claims. Four change the final field-equality conjunctions in `repointed_part`; successful normal encodings satisfy every equality, so those survivors do not demonstrate that this patch corrupts records. The other misses concern a held-chunk guard, first-reference filtering and the reachable-versus-lost arithmetic; the log identifies test-strength limits, and does not make the scanner green. F1 supplies the independently demonstrated causal/test gap for rebuild.

The prior-art check ran by all eight affected paths against GitHub's main-branch commit history and every closed, unmerged PR's file list. The only closed/unmerged overlaps were #647 (segmented chunk maps) and #336 (DST global-state policy), not a staged re-place implementation; neither new staged file had main-branch history. Results and issue #637/#663 split context are preserved in `reviewer-prior-art.json`. The prior rejected iterations' dispositions remain as recorded in `brief.md:145`; this review does not reopen the accepted split or its excluded late-write residual.

Source citations such as `crates/...`, `docs/...` and `AGENTS.md` are grounded in the supplied `$PDCA_TARGET` (`target/`); review and gate-log citations refer to this bundle. No target-state caveat was needed. This report is advisory and does not override deterministic gate decisions.

### Advisory — adversary

# Adversarial review — issue #814 (staged re-place)

Target: `$PDCA_TARGET` (patch applied). Inputs: patch.diff, brief.md, check-gates.json, gate-logs/.
Toolchain was available; I ran probe tests against the patched tree in a scratch copy (setup and output in the first finding).

## Findings

- NEEDS-HUMAN [impl] — **A move with 2+ missing fragments stalls for good when its first write is slow but legal.** `crates/custodian/src/reconstruction/staged.rs:636-640` checks the `W_repoint` gate again before **each** write, measured from the single pre-mark stamp. So the time spent on write #1 counts against write #2's permission. `W_REPOINT_MILLIS` (`crates/custodian/src/gc.rs:211-218`) only budgets for the pre-mark's own commit plus one more read or commit (10 s). It has no room for an earlier fragment write, while `W_WRITE_MILLIS` allows 30 s per write (`gc.rs:195-202`). **Concrete failing case, reproduced:** RS(2,2) with fragments 2 and 3 lost, destinations servers 4 and 5, and each first write taking 12 s (well inside the 30 s deadline the D server enforces). I ran 5 back-to-back passes with the production `reconcile_step`. The outcomes were `[Satisfied, Satisfied, Satisfied, Satisfied, Satisfied]`. Only server 4 ever got a write (5 arrivals, deadlines 40000…88000). Write #2 was never sent, the placement stayed `[0, 1, 2, 3]`, and the obligation stayed queued. The control test (the same 12 s write on a one-fragment move, `Fixture::standard`) completes in one pass (`Changed`). The chunks this hits are the ones with the least slack left (2+ fragments lost). Each pass also rewrites fragment #1 for nothing, which adds load during exactly the mass-repair episodes that make writes slow. The pass answers `Satisfied` each time, because `Aborted` is not a hole (`crates/custodian/src/reconstruction.rs:450`, `:488-507`). Only the backlog gauge and the `reconstruction_aborted` counter show the stall. That is the same "stalls while answering `Satisfied`" shape the brief cites as v1's and v2's defect. Each write already carries the fixed deadline `stamp + W_write` (`staged.rs:635`), and the patch's own doc says this bounds when the write can land (`gc.rs:255-257`). So a single gate check before the first write, or sending all writes at once after one check, keeps C(v) and the grace-window rule intact. No test covers this: C(v) (`crates/custodian/tests/staged_repair.rs:1284-1325`) uses a one-fragment chunk, and the DST case uses one RS(2,1) chunk with one write (`crates/dst/tests/custodian.rs:4469-4477`). This is the same defect as the T4 gate's first blocking item (`staged.rs:637`), now confirmed by running it. Add a two-fragment slow-first-write leg with the fix.

- NEEDS-HUMAN [human] — **The T4 gate's other two blocking items (`staged.rs:641`, `staged.rs:650`) are the late-`Unknown`-write gap the brief excludes by name.** The brief's out-of-scope list says: "a late write whose effect is `WriteEffect::Unknown` landing after GC reclaims its pre-mark". A human needs to record these as rejected-out-of-scope, or the gating T4 row stays red (`3 blocking, 0 recorded-rejected`). One detail for that decision: on a fresh destination, the structured pre-mark is never swept while nothing is listed under it (`gc.rs:900-903`), so a late landing is still covered. The gap only opens when the re-place **reuses** a destination that already holds bytes from an earlier aborted attempt. There GC's normal reclaim path (`gc.rs:680-702`) can delete the bytes and the refreshed pre-mark `G_orphan` (60 s) after the new stamp. This re-place's own retry behaviour creates that situation (`staged.rs:613-616` re-stamps whatever mark it finds). Whether the brief's "shared by every writer" wording covers it is a scope call, not a build defect.

- NEEDS-HUMAN [human] — **A pre-mark whose write never landed stays in the `orphan:` ledger forever, and no tracking issue is named.** The patch's own `gc.rs:269-275` states the settle rule (on `Unknown`, re-read; re-mark if landed; only then add the event to the swept set) and says "the staged re-place does not settle its pre-marks that way yet". There is no `deferred: #N` marker. The sweep keeps every structured, non-`reclaiming` mark over an empty position (`gc.rs:900-903`), and GC's reclaim path never visits a position with no fragment. **Concrete case:** pass 1 pre-marks `(S2, F2)` and the write is refused (`write-deadline-expired`, nothing stored). Before pass 2, a `desired:dserver:2` record appears, so pass 2 picks another server and adopts. The pre-mark at `(S2, F2)` is then never deleted by anyone. It does no harm to safety (unlink and the committed repair overwrite such marks with a blind put, `crates/core/src/metadata.rs:2116-2119`, `reconstruction.rs:1192-1195`), but it is unbounded ledger growth with no owner. The human should decide whether to file the tracking issue now (the rubric's deferral rule needs a `#N`).

- NEEDS-HUMAN [impl] — **Test gaps on guards this diff adds or moves (low priority; fold into the rebuild above).** (a) `reconstruction.rs:792`: the `held` guard survives the mutant "replace with `false`" (C5 log). The only way to reach it that `staged.rs:267` does not also catch is a chunk held because of a corrupt `sidx:` entry whose key names it (`gc.rs:1390-1391`) while a valid committed part also names it. No leg seeds that, so removing the guard would re-place a chunk the staged set marks untrusted, and no test would fail. (b) The staged `Blocked` path (no usable destination: `staged.rs:309`, `:397`) and the staged ceiling refusal (`staged.rs:573-576`) show as MISS in C4-diff-cov: no test reaches them. (c) The read-back checks in `repointed_part` (`staged.rs:733-737`) survive `&&`→`||`. I found no input that reaches them (see below), so this is coverage only.

## Attempted refutations that did not land

- **Red→green evidence.** The C4-verify log shows 18 of 20 `staged_repair.rs` tests failing on the base with assertion failures (not compile errors), and all 20 passing with the fix. The two green on the base are the E-leg "kept" tests, which the brief does not list as red. The gate's summary "20 test(s) ran red" overstates, but the evidence holds. The tests drive the production `reconcile_step`, and the D-server doubles enforce the deadline through `WriteDeadlineExpired::if_elapsed` / `if_publication_unverified` (`staged_repair.rs:314-345`, `crates/dst/tests/custodian.rs:4600-4632`), as the brief requires. I recompiled and ran the suite's harness myself (probes above), and the control passed.
- **Fence race (X29).** The DST coverage leg walks 25 landing points and asserts that all four landings (before the pre-mark, pre-mark→write, write→adoption, after the adoption) are reached, plus both outcomes (`crates/dst/tests/custodian.rs:4960-4991`). It checks named-or-marked after every pass. I found no interleaving that strands a fragment for the single-write move.
- **Byte-splice of the part record (`staged.rs:714-739`).** Records are serde_json (`crates/core/src/metadata.rs:1934-1941`), `chunks` is the first field, and `decode_part_record` has a canonical-bytes gate (`crates/core/src/multipart.rs:2578-2585`). So the first match is always the real chunk list, and a change in server-id width needs no length prefix. I could not make it pick the wrong span.
- **Rewriting an occupied destination on retry.** `FsChunkStore` publishes by `rename` onto the final path, so an overwrite succeeds (`crates/chunkstore-fs/src/lib.rs:303-339`). A retry does not fail the pass.
- **A stale pre-mark lets GC reclaim a committed-path repair's fresh write.** This would need GC and reconstruction to run at the same time. `reconcile_step` runs them one after another (`crates/custodian/src/reconciliation.rs:139-165`), so I could not reach it.
- **Position-level exclusion (`assign`/`augment`, `staged.rs:436-491`).** Checked the augmenting-path logic for a ruled-out position, several servers in one domain, and termination. Every round either considers a new server or reads a new position, and the selector excludes servers, not domains (`crates/core/src/placement.rs:147-162`). No stall found beyond the `W_repoint` one above.
- **A non-deadline write error fails the whole pass (`staged.rs:643`).** Same as the committed path (`reconstruction.rs:1183`), so not a regression.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:636`: Sequential writes share one pre-mark timestamp, so a valid slow write can prevent a multi-fragment repair from ever completing. For RS(2,2) with two lost fragments moving to new servers, a first write taking 15 seconds succeeds within `W_WRITE_MILLIS` (30 seconds), but the next iteration fails the 10-second `W_REPOINT_MILLIS` check before sending the second write. Adoption never runs. Subsequent passes still gather from the unchanged part placement, select the same destinations, and repeat the first write and abort indefinitely. Give each destination a fresh, fenced pre-mark immediately before its write, or dispatch the writes concurrently within the authorization window. Add a ManualClock regression with two replacement destinations and successful writes taking longer than `W_REPOINT_MILLIS`.

No additional actionable reuse, simplification, or efficiency findings. The two late-publication findings in the frozen T4 log fall under the brief's explicit exclusion for `WriteEffect::Unknown` landing after GC reclaims its pre-mark.

Validation: source inspection against `$PDCA_TARGET` and the frozen gate logs; no gates rerun. CI and red-to-green passed, but the existing multi-fragment test does not advance the clock during writes.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Fix the shared pre-mark timing across multiple writes and add a latency regression — a successful 15-second first write prevents the second from ever being attempted on repeated passes, leaving the original redundancy deficit; `crates/custodian/src/reconstruction/staged.rs:636`, `reviewer-slow-writes.log:13`.
- [ ] Validation — fitness-to-purpose — Decide whether staged-upload recovery meets the intended operational recovery expectations after F1 is fixed — automated seam/DST evidence does not supply deployment fitness sign-off; consider Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up under the standing rubric; `AGENTS.md:78`, `brief.md:79`.
- [ ] **A move with 2+ missing fragments stalls for good when its first write is slow but legal.** `crates/custodian/src/reconstruction/staged.rs:636-640` checks the `W_repoint` gate again before **each** write, measured from the single pre-mark stamp. So the time spent on write #1 counts against write #2's permission. `W_REPOINT_MILLIS` (`crates/custodian/src/gc.rs:211-218`) only budgets for the pre-mark's own commit plus one more read or commit (10 s). It has no room for an earlier fragment write, while `W_WRITE_MILLIS` allows 30 s per write (`gc.rs:195-202`). **Concrete failing case, reproduced:** RS(2,2) with fragments 2 and 3 lost, destinations servers 4 and 5, and each first write taking 12 s (well inside the 30 s deadline the D server enforces). I ran 5 back-to-back passes with the production `reconcile_step`. The outcomes were `[Satisfied, Satisfied, Satisfied, Satisfied, Satisfied]`. Only server 4 ever got a write (5 arrivals, deadlines 40000…88000). Write #2 was never sent, the placement stayed `[0, 1, 2, 3]`, and the obligation stayed queued. The control test (the same 12 s write on a one-fragment move, `Fixture::standard`) completes in one pass (`Changed`). The chunks this hits are the ones with the least slack left (2+ fragments lost). Each pass also rewrites fragment #1 for nothing, which adds load during exactly the mass-repair episodes that make writes slow. The pass answers `Satisfied` each time, because `Aborted` is not a hole (`crates/custodian/src/reconstruction.rs:450`, `:488-507`). Only the backlog gauge and the `reconstruction_aborted` counter show the stall. That is the same "stalls while answering `Satisfied`" shape the brief cites as v1's and v2's defect. Each write already carries the fixed deadline `stamp + W_write` (`staged.rs:635`), and the patch's own doc says this bounds when the write can land (`gc.rs:255-257`). So a single gate check before the first write, or sending all writes at once after one check, keeps C(v) and the grace-window rule intact. No test covers this: C(v) (`crates/custodian/tests/staged_repair.rs:1284-1325`) uses a one-fragment chunk, and the DST case uses one RS(2,1) chunk with one write (`crates/dst/tests/custodian.rs:4469-4477`). This is the same defect as the T4 gate's first blocking item (`staged.rs:637`), now confirmed by running it. Add a two-fragment slow-first-write leg with the fix.
- [ ] **The T4 gate's other two blocking items (`staged.rs:641`, `staged.rs:650`) are the late-`Unknown`-write gap the brief excludes by name.** The brief's out-of-scope list says: "a late write whose effect is `WriteEffect::Unknown` landing after GC reclaims its pre-mark". A human needs to record these as rejected-out-of-scope, or the gating T4 row stays red (`3 blocking, 0 recorded-rejected`). One detail for that decision: on a fresh destination, the structured pre-mark is never swept while nothing is listed under it (`gc.rs:900-903`), so a late landing is still covered. The gap only opens when the re-place **reuses** a destination that already holds bytes from an earlier aborted attempt. There GC's normal reclaim path (`gc.rs:680-702`) can delete the bytes and the refreshed pre-mark `G_orphan` (60 s) after the new stamp. This re-place's own retry behaviour creates that situation (`staged.rs:613-616` re-stamps whatever mark it finds). Whether the brief's "shared by every writer" wording covers it is a scope call, not a build defect.
- [ ] **A pre-mark whose write never landed stays in the `orphan:` ledger forever, and no tracking issue is named.** The patch's own `gc.rs:269-275` states the settle rule (on `Unknown`, re-read; re-mark if landed; only then add the event to the swept set) and says "the staged re-place does not settle its pre-marks that way yet". There is no `deferred: #N` marker. The sweep keeps every structured, non-`reclaiming` mark over an empty position (`gc.rs:900-903`), and GC's reclaim path never visits a position with no fragment. **Concrete case:** pass 1 pre-marks `(S2, F2)` and the write is refused (`write-deadline-expired`, nothing stored). Before pass 2, a `desired:dserver:2` record appears, so pass 2 picks another server and adopts. The pre-mark at `(S2, F2)` is then never deleted by anyone. It does no harm to safety (unlink and the committed repair overwrite such marks with a blind put, `crates/core/src/metadata.rs:2116-2119`, `reconstruction.rs:1192-1195`), but it is unbounded ledger growth with no owner. The human should decide whether to file the tracking issue now (the rubric's deferral rule needs a `#N`).
- [ ] **Test gaps on guards this diff adds or moves (low priority; fold into the rebuild above).** (a) `reconstruction.rs:792`: the `held` guard survives the mutant "replace with `false`" (C5 log). The only way to reach it that `staged.rs:267` does not also catch is a chunk held because of a corrupt `sidx:` entry whose key names it (`gc.rs:1390-1391`) while a valid committed part also names it. No leg seeds that, so removing the guard would re-place a chunk the staged set marks untrusted, and no test would fail. (b) The staged `Blocked` path (no usable destination: `staged.rs:309`, `:397`) and the staged ceiling refusal (`staged.rs:573-576`) show as MISS in C4-diff-cov: no test reaches them. (c) The read-back checks in `repointed_part` (`staged.rs:733-737`) survive `&&`→`||`. I found no input that reaches them (see below), so this is coverage only.
- [ ] `crates/custodian/src/reconstruction/staged.rs:636`: Sequential writes share one pre-mark timestamp, so a valid slow write can prevent a multi-fragment repair from ever completing. For RS(2,2) with two lost fragments moving to new servers, a first write taking 15 seconds succeeds within `W_WRITE_MILLIS` (30 seconds), but the next iteration fails the 10-second `W_REPOINT_MILLIS` check before sending the second write. Adoption never runs. Subsequent passes still gather from the unchanged part placement, select the same destinations, and repeat the first write and abort indefinitely. Give each destination a fresh, fenced pre-mark immediately before its write, or dispatch the writes concurrently within the authorization window. Add a ManualClock regression with two replacement destinations and successful writes taking longer than `W_REPOINT_MILLIS`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 197 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Size backstop: patch is 197 KB, nearly double the 100 KB threshold. This is an oversized slice — split it in re-plan (`pdca split`) rather than patching in place. The reproduced slow-write stall (shared pre-mark timestamp starving the second write's authorization window in multi-fragment moves) is real and should be addressed as part of the split, but the driving reason for iterate-plan over iterate-do is the size backstop itself, not an attempt to scope the bug fix.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
