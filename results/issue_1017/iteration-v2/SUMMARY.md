# Result — issue 1017 / multipart-reaper-terminal-exits

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 625.1, two states still have no exit when no client verb arrives.
  - **`Completing`:** a gateway that died holding the fence leaves the session `Completing`
    forever. It is never rolled back at `W_completing`, and when over-age it is never moved to
    `Aborting`.
  - **`Completed`:** #659's drain retains the tombstone, because its deployed tombstone input
    says "retain" until #625 supplies the window. The record and its `mpuctl` slot are
    therefore held forever.

  Two more gaps fall to this slice:
  - The grace window `G_orphan` is not yet bound to the strict inequality 0016 assigns to #625.
  - `B_ops` rests on a per-operation latency "assumed, not measured" (`SLOWEST_OP_MILLIS`,
    `crates/core/src/multipart.rs:5082-5105`), whose replacement the source assigns to #625.
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper_exits.rs`
  drives the reaper through 625.1's one-shot as a subprocess (the same mechanics; the seeding
  helpers are **copied** into this file, not shared, per `run-verify.sh:141-144`). Every leg
  asserts on durable store state.
  **(D) `Completing` is exited, both ways.**
  - A `Completing` session whose `fenced_at_millis` is older than `W_completing`, and which is
    **not** over-age, rolls back to `Open@E+1` and installs `retire:records:{seg:<g>:<E>}` for
    that attempt's segments.
  - A `Completing` session that **is** over-age (`now - created_at > W_session`) goes
    **straight** to `Aborting@E+1`, in one batch that also installs that obligation. It is
    never rolled back first (`0016:1918-1930`).
  - Assert that the named epoch's `seg:` range drains empty and that a **later** epoch's
    segments are untouched.

  **(E-terminal) The clock guard on the terminal states.**
  - A foreign-clocked `Completing` session whose `fenced_at_millis` is far past `W_completing`
    is **not** rolled back: its state, epoch, `seg:` range and `retire:` records are all
    unchanged.
  - A foreign-clocked `Completed` session keeps its record, because its tombstone window
    cannot be judged. Assert that the record **survives**. Its only exit is #633's
    terminal-expiry verb.

  **(G) Tombstone expiry.** A local-clock `Completed` session whose `completed_at_millis` is
  older than `W_tombstone`, with no session `retire:` obligation left and an empty `sidx:`
  range, has its record deleted and `mpuctl.count` decremented by exactly one. A `Completed`
  session inside `W_tombstone` keeps its record, and an identical Complete retry still answers
  `AlreadyCompleted` (`multipart.rs:4789-4795`).

  **(H-terminal)** With a mismatched `mpuctl.profile`, a stale `Completing` session is not
  rolled back either (625.1's check guards every arm).

  **(F) Teardown over the composed reaper path is idempotent and waits for GC's decision.**
  The `orphan:` mark guard (arms (0), (a), (b), (c), and fail-closed on undecodable values) is
  **#659 child-1's** code and tested there. This slice does not implement, copy or re-test it
  arm by arm. It asserts only what the reaper's path adds on top:
  - Run the one-shot twice over a partially drained obligation of a session the reaper
    fenced. It converges to the same terminal state with **no double-decrement**.
  - **The reaper's teardown waits for the obligation.** In an `Aborting` session's obligation,
    one position carries a `reclaiming` mark. After the one-shot, the `mpu:` record is
    present, the count is unchanged, the mark is byte-identical, and the obligation is still
    there. The test then deletes that key, as GC's blind delete would (`gc.rs:804-824`), and
    runs the one-shot again. The position gets a fresh mark under `require_absent`, the
    obligation and the session are gone, and the count drops by exactly one.

  **`G_orphan` boundary** (`cargo xtask ci` evidence, appended to the existing
  `crates/custodian/tests/gc.rs`): pin the tick at which a mark written by a fence becomes
  reclaimable, and show that `G_orphan > W_write + δ_clock` holds **strictly**. GC's grace check
  is inclusive (`gc.rs:172-176`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the `Completing` rollback at `W_completing` and the over-age `Completing → Aborting` edge,
    both installing `retire:records:{seg:<g>:<E>}`, through the existing fenced transitions;
  - the clock guard applied to `W_completing` and `W_tombstone`;
  - the `W_tombstone` judgment, which replaces "retain" as #659's tombstone input via
    `ReaperPolicy`;
  - the knob values `W_completing`, `W_tombstone` and **`G_orphan`**: each inside 0016's range
    and cited at its definition, with `G_orphan > W_write + δ_clock` and, where a repoint can
    intervene, `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, `:2497-2509`);
  - the calibration experiment;
  - docs in `08-crosscutting-concepts.md`.

  **Not this slice's to choose:** `W_ref`, `B_bytes`, `B_ops`, the profile tuple and
  `MAX_SESSIONS` are #655's single value set, consumed unchanged.

  **Out of scope:**
  - the `Open` arms (625.1) and the deployed wiring (625.2);
  - the operator abort and terminal-expiry verbs and the foreign-clock alarm (#633);
  - the `orphan:` mark guard, including arm (0) (`reclaiming`), and its DST rows (#659
    child-1), and X56 (#660);
  - #806, #630, FU-2 and FU-4;
  - ADRs, specs, and any edit to 0016;
  - any change to the records or state machine of the slices below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 95 mutants tested in 6m: 1 missed, 79 caught, 15 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.86s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1017: give abandoned `Completing` and `Completed` multipart sessions exits, preserve retry and retirement safety, and substantiate orphan-grace and batch-budget bounds.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The durable-state criteria and accepted two-minute tombstone choice are explicit; the server-edit exception and remaining ownership boundary are distinguishable (`brief.md:31`, `brief.md:220`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new test executes eight tests: six fail, including both missing exits, and two pass (`reviewer-red.log:271`; `crates/server/tests/multipart_reaper_exits.rs:574`, `crates/server/tests/multipart_reaper_exits.rs:865`). |
| C3 Change | PASS | The supplied changes address the specified transitions through the existing fence/drain composition; the small core transition API preserves state-machine ownership and the permitted server fixture adjustment preserves its safety assertions (`crates/core/src/multipart.rs:2796`, `crates/custodian/src/reaper.rs:1107`, `crates/server/tests/custodian_gc.rs:1466`). |
| C4 Verification (red→green) | PASS | Restoring the patch returns all eight subprocess tests to green; 44 reaper unit tests, the GC boundary test, and nine DST tests also pass independently; remote-base diff coverage remains unmeasured, not a compilation defect (`reviewer-restored-green.log:17`, `reviewer-unit.log:100`, `reviewer-gc.log:8`, `reviewer-dst.log:252`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve the retry window after actual publication — a slow successful Complete can publish an already-expired tombstone, and the independent probe loses `AlreadyCompleted` with zero elapsed time after publication (`crates/custodian/src/retirement.rs:435`, `crates/core/src/multipart/complete.rs:1834`; `reviewer-probe.log:14`). |
| T1 Structure | PASS | The change keeps backend independence, uses the existing atomic fence/obligation seam, and pairs session clock identity with its source; the added backend core dependencies are test-only (`crates/custodian/src/reaper.rs:1379`, `crates/custodian/src/retirement.rs:456`, `crates/metadata-tikv/Cargo.toml:39`). |
| T2 Shape | PASS | The accepted single-slice shape remains coherent, the living architecture covers the new behavior, and independent formatting, spelling, docs lint/render and mutable-global checks pass (`docs/design/architecture/08-crosscutting-concepts.md:114`; `reviewer-scanners.log:1`, `reviewer-docs-render.log:3`, `reviewer-statics.log:3`). |
| T3 Runtime | NEEDS-HUMAN | Discharge the live-backend calibration dependency before relying on the published latency figures — FDB compiled but its first commit timed out and its health probe failed; TiKV has no configured endpoint here, and the supplied logs contain no raw successful calibration measurements (`reviewer-fdb.log:49`, `reviewer-fdb-health.log:1`, `reviewer-dependencies.log:2`; `crates/core/src/multipart.rs:5978`). |
| T4 Contribution | NEEDS-HUMAN | Establish affected-path prior art across merged history and closed/rejected work — the path-specific history check exposes only the synthetic base, with no remote/history evidence to settle duplication or earlier rejection; the contribution-artifact audit itself is deferred/N/A (`reviewer-prior-art.log:1`, `brief.md:214`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Assign the production `G_orphan` wiring to this slice or #1016 — the tested constant remains disconnected from the deployed value, and the brief explicitly requires a scope decision before editing that server code (`crates/custodian/src/gc.rs:303`, `crates/custodian/tests/gc.rs:1995`, `crates/server/src/custodian.rs:115`; `brief.md:228`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm operational fitness after the retry-window defect and calibration evidence are resolved — the accepted windows must support real lost-response retries and admission turnover, and single-host timing cannot establish deployment-wide latency (`crates/custodian/src/reaper.rs:264`, `docs/design/architecture/08-crosscutting-concepts.md:114`; `brief.md:169`). |

The patch needs one implementation correction: successful publication can lose its retry record immediately. The principal exit tests reproduce red→green, but they seed completion timestamps directly and do not establish the promised window after a slow Complete. Separate human decisions remain for production grace wiring, live calibration evidence, and prior art. These are advisory judgments; the frozen gate results are not overridden.

Source citations above are relative to `$PDCA_TARGET`; evidence files are relative to this review directory. The supplied target is readable and contains the applied patch over synthetic commit `3a6d16d`, identified as base `6e44de924cc8a451a71ce7f7ed63aca76c67f509`. Its base contains the earlier reaper and #659's reclaiming-mark guard (`crates/custodian/src/retirement/marks.rs:307`). The tracked fix was stashed and restored successfully; `git apply --reverse --check ../patch.diff` succeeds afterward. No implementation fix was made.

**The retry defect is reproduced through production Complete and retirement code.** `PublishRequest.completed_at_millis` is supplied before `complete_upload` fences and publishes (`crates/core/src/multipart/complete.rs:1474`, `crates/core/src/multipart/complete.rs:1522`). The root flip copies that unchanged value at line 1834. The new expiry check subtracts it from the current reaper clock (`crates/custodian/src/retirement.rs:435`), so work before publication consumes the advertised post-publication window.

The standalone reviewer probe reuses the existing terminal-session fixture and real redb backend, advances its single manual clock by 120,001 ms immediately before the Complete publication commit, and then drives the production reaper without advancing time again. Complete publishes successfully; an identical retry initially returns `AlreadyCompleted`; after draining, the same retry returns `Refused(NoSuchUpload)` and admission count is zero. Elapsed time since publication is still zero (`reviewer-probe.log:14`). This is an executable counterexample, not a claim inferred solely from the timestamp's name. The supplied G fixture directly plants a fresh `Completed` record (`crates/server/tests/multipart_reaper_exits.rs:876`) and misses this case. Acquire a suitable publication-time stamp on the same clock lifecycle and add a delayed-Complete regression; increasing the already-decided window does not remove the cause.

To reproduce the counterexample from this review directory:

```sh
TMPDIR="$PWD" CARGO_TARGET_DIR="$PWD/target/pdca-reviewer-1017-build" cargo +1.96.0 test --offline --manifest-path pdca-reviewer-1017-probe/Cargo.toml reviewer_new_publication -- --nocapture
```

The probe passes when it witnesses the defect; its assertions are at `pdca-reviewer-1017-probe/probe.rs:3709`. The original source files remain untouched by this probe.

**The main safety evidence is substantive, with specific limits.** Independent execution covered:

- `cargo test --offline --locked -p wyrd-server --test multipart_reaper_exits`: eight green, then two passed/six failed with the fix stashed, then eight green after restoration. F's two tests fail on the base before their drain assertions because they first require the new Completing fence (`crates/server/tests/multipart_reaper_exits.rs:991`, `crates/server/tests/multipart_reaper_exits.rs:1078`); their red result does not establish that #659's guard is broken.
- `cargo test --offline --locked -p wyrd-custodian --lib reaper::tests`: 44 passed, including clock identity and exact-window boundaries. The GC boundary test also passed. Its passing result establishes the injected grace value, not the deployed constant's linkage.
- `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=64 cargo test --offline --locked -p wyrd-dst --test custodian reaper_`: nine passed, one existing accepted-gap test ignored. The added race records the contested read and both commits; the crash wrapper refuses all subsequent I/O and compares the durable crash boundary (`crates/dst/tests/custodian.rs:8784`, `crates/dst/tests/custodian.rs:8849`, `crates/dst/tests/custodian.rs:9059`). The accepted #1015 gap is not reopened.
- Formatting, changed-file spelling, documentation lint and rendering/link audit, `cargo xtask statics`, and `git diff --check` passed independently. Full CI and TiKV feature compilation are supported by their frozen logs, not claimed as independently rerun in full.

The C5 capability-probe smell test does not fire: the new optional session-clock handling fails closed for a supported context without a reaper; it does not conceal an eager/load-time side effect (`crates/custodian/src/retirement.rs:658`).

**Frozen gate evidence is adjudicated without treating summaries as proof of unrelated claims.**

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-ci | PASS from captured execution | Formatting, clippy/build/tests, dependency checks, docs and DST ran; the final result is successful (`gate-logs/C4-ci.log:5102`). |
| C4-verify | PASS independently reproduced | Eight executed tests are green with the fix; six fail without it (`gate-logs/C4-verify.log:10`, `reviewer-red.log:279`, `reviewer-restored-green.log:17`). |
| C4-diff-cov | Unmeasured; base-state caveat | The wrapper rejects application to `origin/main` before measuring coverage; the supplied target does apply and execute the patch (`gate-logs/C4-diff-cov.log:10`). This is not a C4 patch-defect FAIL. |
| C5-mutants | Recorded red; sole survivor is equivalent for these fixtures | 79 caught, 15 unviable, one missed. Column 69 changes the second `+` to `-`, affecting only `built.deletes.len()`, which is zero for both prescribed batches (`gate-logs/C5-mutants.log:13`, `crates/metadata-conformance/src/lib.rs:1651`, `crates/metadata-conformance/src/lib.rs:1706`). No artificial delete case is owed to kill it. |
| T4-batch-review | Recorded red; findings independently triaged below | The log contains 12 entries, including duplicates; its raw count is not 12 independently established defects (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | The artifacts are drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is requested for this deferral. |
| host-tikv | PASS for compilation only | Both requested feature clippy invocations completed; this does not measure a live TiKV cluster (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:214`). |

The batch-review timestamp finding is confirmed by the executable counterexample. Its three grace-linkage entries describe the same unresolved scope issue: both values are currently 60 seconds and production already enforces the strict late-write inequality, but the new boundary test can remain green if production's value changes (`crates/server/src/custodian.rs:124`). The carry-forward explicitly reserves the server edit for a human decision, so this is T5 NEEDS-HUMAN, not another automatic implementation request.

The remaining batch-review classes do not establish additional defects in this slice:

- Conflicting reconstruction/reaper policies require an inconsistent caller configuration; the supplied deployed path passes the same `policy` to both (`crates/server/src/custodian.rs:628`, `crates/server/src/custodian.rs:638`). No current deployed retention violation is established; broader composition changes belong to #1016's scope decision.
- Redb calibration, guard-heavy batches, and large expected-value reads exceed the brief's chosen two networked-backend experiments (`brief.md:146`). Their representativeness limits the performance claim and belongs in runtime/fitness sign-off; it does not justify silently expanding the prescribed experiment. The existing TiKV retry behavior is explicitly addressed by requiring first-attempt evidence (`brief.md:163`), which is still owed here.
- The subprocess diagnostic assertion does not pair upload ID and reason on one line (`crates/server/tests/multipart_reaper_exits.rs:839`), but the required foreign-session state, segment, obligation and count invariants are asserted directly at lines 823–835; unit tests also pair the foreign tombstone and reason (`crates/custodian/src/reaper/tests.rs:2608`). The weaker auxiliary assertion does not invalidate the demonstrated clock safety.

**Live calibration and prior art remain undisclosed evidence, not verified successes.** Docker, OpenSSL and CMake were available, and the FDB test compiled with its real feature. Its first 500-operation batch returned `FdbError 1031` after 10,000 ms (`reviewer-fdb.log:49`); `fdbcli ... --exec 'status minimal' --timeout 5` independently timed out (`reviewer-fdb-health.log:1`). Despite the timing print's word “committed,” the subsequent assertion shows no successful commit was established. This is a plausible host/cluster availability fault, not a demonstrated patch defect or a successful latency measurement. TiKV's endpoint was unset. Neither frozen CI nor compile-only evidence supplies the raw historical 22/44 ms FDB and 252/373 ms TiKV measurements quoted in source; withheld builder notes were not consulted.

Before performance sign-off, run `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance` on healthy intended backends and preserve the first attempt's raw measurements and versions/host. If a successful batch exceeds the budget, leave constants unchanged and refer the value decision to #655 as the brief requires. The existing single-node qualifier should remain visible when deciding deployment fitness. For this custodian durability change, Tier-1 disk-fault and Tier-2 kill/reconstruct observation is warranted as complementary deployment evidence, not a substitute for the passing DST tests.

The prior-art investigation ran `git log --all -- crates/custodian/src crates/metadata-conformance/src/lib.rs` and checked available remote refs. It returned only the synthetic base and no remotes (`reviewer-prior-art.log:1`). No supplied gate records affected-path merged history or closed/rejected work, so that comparison cannot be certified from this artifact-only bundle. Supply that comparison before contribution sign-off; do not infer “no prior art” from the deliberately flattened target.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1017 (625.3: Completing exits, tombstone expiry, G_orphan, B_ops)

Verdict: the fix holds. I re-ran the red→green proof, broke the production code four ways in a
scratch copy, and could not show a production defect. What I did find: one server test that is
weaker than its doc comment says, and four claims or loose ends the human has to judge.

## What I re-ran (scratch copy of `$PDCA_TARGET`)

- Green: `cargo test -p wyrd-server --test multipart_reaper_exits` passed 8 of 8. Red, with
  `crates/core/src`, `crates/custodian/src` and `crates/metadata-conformance/src` reverted to the
  base and the test kept: 6 failed, 2 passed, at the same panic lines as `gate-logs/C4-verify.log`
  (`:212`, `:843`, `:991`, `:1078`). The two legs that pass on the base are the ones the brief
  calls vacuous there: `completing_session_fenced_moments_ago_is_kept_however_old_its_parts`
  (`crates/server/tests/multipart_reaper_exits.rs:734`) and H-terminal (`:926`). The
  `check-gates.json` C4-verify text "8 test(s) ran red" means 8 tests ran in the red leg, not
  that 8 failed. Only 6 failed.
- The tests go through the production path. Every leg runs the built `wyrd custodian
  --reap-abandoned-uploads` binary as a subprocess over a real redb store
  (`multipart_reaper_exits.rs:225-235`) and checks what the store holds afterwards.

## Findings

- NEEDS-HUMAN [impl] — **The E-terminal server leg does not check that the foreign `Completed`
  session is held, though its doc says it does.** The doc at
  `crates/server/tests/multipart_reaper_exits.rs:788` says "Every one-shot names both as held for
  their clock". But the check at `:837-841` looks for `mpu:{id}` and `foreign-clock` as two
  separate substrings anywhere in stderr. `emit_foreign` (`crates/custodian/src/reaper.rs:1574-1584`)
  prints `action = "reaper-foreign-clock"` together with the key for every foreign session,
  whether or not it is held, so both substrings always appear. The non-zero exit check (`:843-847`)
  is satisfied by the `Completing` hold alone. **Failing case:** I made the `Completed` arm of the
  foreign-clock branch (`reaper.rs:975`) `return Ok(())`, so it is never held.
  `multipart_reaper_exits` still passed 8 of 8. Only the custodian tests caught it
  (`crates/custodian/src/reaper/tests.rs:2648`, `crates/custodian/tests/gc.rs:1636`). So the
  behaviour is covered, but this leg proves less than it claims. Fix: match one line that carries
  `reaper-session-held`, the `foreign-clock` hold reason and `mpu:{id}` together, or check that
  the `reaper_sessions_held` tally is 2.
- NEEDS-HUMAN [human] — **`G_ORPHAN_MILLIS` is still not read by any production code.**
  `crates/custodian/src/gc.rs:303-308` says so itself. The deployed GC gets its grace from
  `crates/server/src/custodian.rs:115` (`GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`). The only
  compile-time check there (`:124`) is `LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS`; it
  does not tie the server value to `G_ORPHAN_MILLIS`. The boundary test passes `G_ORPHAN_MILLIS`
  in directly (`crates/custodian/tests/gc.rs:1995`), so if someone changes the server's value,
  the deployed reclaim tick moves and that test stays green. `08-crosscutting-concepts.md`
  describes `G_orphan` (60 s) as the value in force. The carry-forward said to wire it or raise a
  §6 item, because the fix needs an edit in `crates/server/src` (child-2's file). Confirm that the
  §6 item exists. The one-line fix is `GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`.
- NEEDS-HUMAN [human] — **No Check evidence backs the calibration figures; verdict provisional.**
  `crates/core/src/multipart.rs:5975-5985` and `08-crosscutting-concepts.md` quote 22/44 ms on
  FoundationDB 7.3.77 and 252/373 ms on TiKV 8.5.1 (host `saic`, 2026-11-22). The only gate
  evidence is `gate-logs/C4-ci.log:2623` and `:2784`, where both calibration tests report "ok" in
  0.00 s: they skipped because no cluster was present. I had no cluster either. The carry-forward
  forbids quoting figures without the raw first-attempt output. Check that `build-notes.md` (not
  shown to me) or SUMMARY §9 contains that output. If it does not, keep the "assumed, not
  measured" wording.
- NEEDS-HUMAN [human] — **The `W_tombstone` reasoning leaves out when the window starts.** The doc
  comment at `crates/custodian/src/reaper.rs:268-277` says two minutes covers an SDK retry made
  "seconds to tens of seconds" after the answer was lost. But the window is measured from
  `PublishRequest::completed_at_millis`, which the caller reads **before** `complete_upload`
  fences the session and writes its segments (`crates/core/src/multipart/complete.rs:290-293`,
  `:1470-1474`). The longest segmented Complete (512 segments, about 11 segment batches plus up to
  `R_PUBLISH` = 3 flips, each up to 5 s) runs about 70 s. That leaves about 50 s of the window
  when the client gets its answer. **Concrete case:** a Complete takes 90 s, its answer is lost,
  and the SDK retries 40 s later. The record may already be gone, so the client gets
  `NoSuchUpload` for an object that was published. 0016 accepts that answer once the window has
  passed, and the 2-minute value was chosen at sign-off, so this is the human's call. Either way,
  the doc comment should say so. The "about 23 a minute" figure (`reaper.rs:273`) also ignores
  the fleet loop's 30 s interval and the drain that has to finish before the delete.
- NEEDS-HUMAN [human] — **A server comment is now wrong, and it is outside this slice's scope
  exception.** `crates/server/src/custodian.rs:625-627` still says "until it is [measured], every
  `Completed` multipart session keeps its record, and its admission slot". With this patch,
  `policy.tombstone()` is `Window`, so that is no longer true. The sign-off exception covers only
  the `custodian_gc.rs` fixture and the 05/06 docs. Send it to #1016, or widen the exception.

## Attacks that failed

- **Fleet-loop tombstone wiring.** I set `sessions` to `None` at
  `crates/custodian/src/reconciliation.rs:189`. Two tests went red:
  `crates/custodian/tests/gc.rs:1643` and `:1750`. The deployed path is covered, not just the
  one-shot.
- **T4's BLOCKING "retaining reaper beside an expiring reconstruction context"**
  (`retirement.rs:617`). The deployed wiring cannot produce it: `crates/server/src/custodian.rs:590`,
  `:628` and `:640` take both inputs from the same `policy`. It is a misuse risk in the library
  API only, not a production defect.
- **Negation (9): delete while a `retire:` obligation is present.** I disabled the obligation wait
  in `terminal()` (`crates/custodian/src/retirement/sessions.rs:932-939`). Both F legs went red
  (`multipart_reaper_exits.rs:990`, `:1077`), so F's evidence holds.
- **Negation (15): roll back an over-age session before aborting it.** This would mark the
  positions under `s:<id>:2`. The leg asserts `s:<id>:1` and checks the tallies
  (`multipart_reaper_exits.rs:702-722`), so it would fail.
- **Could a healthy large Complete outlast `W_completing` and be rolled back on every retry?** No.
  `MAX_ROOT_SEGMENTS` = 512 (`crates/core/src/metadata.rs:572`) and a 5 MB batch holds about 50
  segment puts, so a Complete is about 11 batches plus 3 flips, well under 15 minutes.
- **Rollback racing a live completer.** The rollback requires the session's exact bytes, so a
  segment batch, flip or release that lands first makes it lose. A late completer's batches fail
  against the bumped epoch. The DST flip-first row confirms the flip landed between the reaper's
  read and its commit, with no fallback path (`crates/dst/tests/custodian.rs`, `FlipGate`).
  `CrashAt` fails all I/O after the crash point and compares the exact boundary snapshot.
- **The new API in core** (`SessionRecord::completing_rollback`). It has to live there:
  `fence_released` is `pub(crate)` (`crates/core/src/multipart.rs:2880`) and
  `RetireObligation::attempt_segments` is private (`:4678`), so the custodian could not build
  this transition itself.
- **The C5 surviving mutant** (`crates/metadata-conformance/src/lib.rs:1651`, `+` → `-`) changes
  nothing: `built.deletes.len()` is 0 for both batch shapes.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Expiry can consume the retry window before Complete publishes.** `crates/custodian/src/retirement.rs:436` now expires tombstones from `completed_at_millis`, but `complete_upload` receives that stamp before awaiting the fence, part reads and segment writes (`crates/core/src/multipart/complete.rs:1522`) and persists it unchanged (`crates/core/src/multipart/complete.rs:1834`). A Complete stamped at time t that publishes at t + 121 seconds is immediately eligible for expiry once its obligations drain; an immediate identical retry can then receive `NoSuchUpload`. Refresh the stamp at publication through the same clock seam, and add a delayed-Complete regression proving the two-minute window survives publication.

- NEEDS-HUMAN [human] — **The new grace constant duplicates the deployed configuration without controlling it.** `crates/custodian/src/gc.rs:308` adds `G_ORPHAN_MILLIS`, and the boundary test injects it directly at `crates/custodian/tests/gc.rs:1995`; production still derives its grace independently at `crates/server/src/custodian.rs:115`. Both are currently 60 seconds, and production independently enforces the strict safety bound, but its reclaim tick can change while this new boundary test remains green. Coordinate #1016 to consume the shared constant, or authorize the narrow server wiring exception requested by the brief, so the tested and deployed values cannot drift.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve the retry window after actual publication — a slow successful Complete can publish an already-expired tombstone, and the independent probe loses `AlreadyCompleted` with zero elapsed time after publication (`crates/custodian/src/retirement.rs:435`, `crates/core/src/multipart/complete.rs:1834`; `reviewer-probe.log:14`).
- [ ] T3 Runtime — Discharge the live-backend calibration dependency before relying on the published latency figures — FDB compiled but its first commit timed out and its health probe failed; TiKV has no configured endpoint here, and the supplied logs contain no raw successful calibration measurements (`reviewer-fdb.log:49`, `reviewer-fdb-health.log:1`, `reviewer-dependencies.log:2`; `crates/core/src/multipart.rs:5978`).
- [ ] T4 Contribution — Establish affected-path prior art across merged history and closed/rejected work — the path-specific history check exposes only the synthetic base, with no remote/history evidence to settle duplication or earlier rejection; the contribution-artifact audit itself is deferred/N/A (`reviewer-prior-art.log:1`, `brief.md:214`, `gate-logs/T4-contribution.log:10`).
- [ ] T5 Judgment — Assign the production `G_orphan` wiring to this slice or #1016 — the tested constant remains disconnected from the deployed value, and the brief explicitly requires a scope decision before editing that server code (`crates/custodian/src/gc.rs:303`, `crates/custodian/tests/gc.rs:1995`, `crates/server/src/custodian.rs:115`; `brief.md:228`).
- [ ] Validation — fitness-to-purpose — Confirm operational fitness after the retry-window defect and calibration evidence are resolved — the accepted windows must support real lost-response retries and admission turnover, and single-host timing cannot establish deployment-wide latency (`crates/custodian/src/reaper.rs:264`, `docs/design/architecture/08-crosscutting-concepts.md:114`; `brief.md:169`).
- [ ] **The E-terminal server leg does not check that the foreign `Completed` session is held, though its doc says it does.** The doc at `crates/server/tests/multipart_reaper_exits.rs:788` says "Every one-shot names both as held for their clock". But the check at `:837-841` looks for `mpu:{id}` and `foreign-clock` as two separate substrings anywhere in stderr. `emit_foreign` (`crates/custodian/src/reaper.rs:1574-1584`) prints `action = "reaper-foreign-clock"` together with the key for every foreign session, whether or not it is held, so both substrings always appear. The non-zero exit check (`:843-847`) is satisfied by the `Completing` hold alone. **Failing case:** I made the `Completed` arm of the foreign-clock branch (`reaper.rs:975`) `return Ok(())`, so it is never held. `multipart_reaper_exits` still passed 8 of 8. Only the custodian tests caught it (`crates/custodian/src/reaper/tests.rs:2648`, `crates/custodian/tests/gc.rs:1636`). So the behaviour is covered, but this leg proves less than it claims. Fix: match one line that carries `reaper-session-held`, the `foreign-clock` hold reason and `mpu:{id}` together, or check that the `reaper_sessions_held` tally is 2.
- [ ] **`G_ORPHAN_MILLIS` is still not read by any production code.** `crates/custodian/src/gc.rs:303-308` says so itself. The deployed GC gets its grace from `crates/server/src/custodian.rs:115` (`GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`). The only compile-time check there (`:124`) is `LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS`; it does not tie the server value to `G_ORPHAN_MILLIS`. The boundary test passes `G_ORPHAN_MILLIS` in directly (`crates/custodian/tests/gc.rs:1995`), so if someone changes the server's value, the deployed reclaim tick moves and that test stays green. `08-crosscutting-concepts.md` describes `G_orphan` (60 s) as the value in force. The carry-forward said to wire it or raise a §6 item, because the fix needs an edit in `crates/server/src` (child-2's file). Confirm that the §6 item exists. The one-line fix is `GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`.
- [ ] **No Check evidence backs the calibration figures; verdict provisional.** `crates/core/src/multipart.rs:5975-5985` and `08-crosscutting-concepts.md` quote 22/44 ms on FoundationDB 7.3.77 and 252/373 ms on TiKV 8.5.1 (host `saic`, 2026-11-22). The only gate evidence is `gate-logs/C4-ci.log:2623` and `:2784`, where both calibration tests report "ok" in 0.00 s: they skipped because no cluster was present. I had no cluster either. The carry-forward forbids quoting figures without the raw first-attempt output. Check that `build-notes.md` (not shown to me) or SUMMARY §9 contains that output. If it does not, keep the "assumed, not measured" wording.
- [ ] **The `W_tombstone` reasoning leaves out when the window starts.** The doc comment at `crates/custodian/src/reaper.rs:268-277` says two minutes covers an SDK retry made "seconds to tens of seconds" after the answer was lost. But the window is measured from `PublishRequest::completed_at_millis`, which the caller reads **before** `complete_upload` fences the session and writes its segments (`crates/core/src/multipart/complete.rs:290-293`, `:1470-1474`). The longest segmented Complete (512 segments, about 11 segment batches plus up to `R_PUBLISH` = 3 flips, each up to 5 s) runs about 70 s. That leaves about 50 s of the window when the client gets its answer. **Concrete case:** a Complete takes 90 s, its answer is lost, and the SDK retries 40 s later. The record may already be gone, so the client gets `NoSuchUpload` for an object that was published. 0016 accepts that answer once the window has passed, and the 2-minute value was chosen at sign-off, so this is the human's call. Either way, the doc comment should say so. The "about 23 a minute" figure (`reaper.rs:273`) also ignores the fleet loop's 30 s interval and the drain that has to finish before the delete.
- [ ] **A server comment is now wrong, and it is outside this slice's scope exception.** `crates/server/src/custodian.rs:625-627` still says "until it is [measured], every `Completed` multipart session keeps its record, and its admission slot". With this patch, `policy.tombstone()` is `Window`, so that is no longer true. The sign-off exception covers only the `custodian_gc.rs` fixture and the 05/06 docs. Send it to #1016, or widen the exception.
- [ ] **Expiry can consume the retry window before Complete publishes.** `crates/custodian/src/retirement.rs:436` now expires tombstones from `completed_at_millis`, but `complete_upload` receives that stamp before awaiting the fence, part reads and segment writes (`crates/core/src/multipart/complete.rs:1522`) and persists it unchanged (`crates/core/src/multipart/complete.rs:1834`). A Complete stamped at time t that publishes at t + 121 seconds is immediately eligible for expiry once its obligations drain; an immediate identical retry can then receive `NoSuchUpload`. Refresh the stamp at publication through the same clock seam, and add a delayed-Complete regression proving the two-minute window survives publication.
- [ ] **The new grace constant duplicates the deployed configuration without controlling it.** `crates/custodian/src/gc.rs:308` adds `G_ORPHAN_MILLIS`, and the boundary test injects it directly at `crates/custodian/tests/gc.rs:1995`; production still derives its grace independently at `crates/server/src/custodian.rs:115`. Both are currently 60 seconds, and production independently enforces the strict safety bound, but its reclaim tick can change while this new boundary test remains green. Coordinate #1016 to consume the shared constant, or authorize the narrow server wiring exception requested by the brief, so the tested and deployed values cannot drift.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- [ ] size backstop — this slice is behaving oversized: patch is 260 KB (threshold 100 KB); patch touches 21 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the slice whole and keep the design: the Completing exits, clock guard, crash/flip-first DST rows and calibration evidence hold. Fix: - REAL DEFECT — tombstone retry window: `W_tombstone` is measured from `PublishRequest.completed_at_millis`, stamped before `complete_upload` fences and writes segments (`crates/core/src/multipart/complete.rs:1474/:1522`, persisted unchanged at `:1834`), so a slow Complete can publish an already-expired tombstone and an immediate identical retry gets `NoSuchUpload` (reviewer probe, `reviewer-probe.log:14`). Take the stamp at publication through the same clock seam, and add a delayed-Complete regression proving the full 2-minute window survives publication. Do not change the 2-minute value. - Update the `W_tombstone` doc comment (`crates/custodian/src/reaper.rs:268-277`) to say when the window starts, and correct the "about 23 a minute" throughput figure for the fleet loop's interval and the drain before delete. - E-terminal server leg (`crates/server/tests/multipart_reaper_exits.rs:837-841`): assert the foreign `Completed` hold on one line carrying `reaper-session-held`, the `foreign-clock` reason and `mpu:{id}` together (or assert the `reaper_sessions_held` tally is 2), so making the `Completed` foreign arm a no-op turns the leg red. - WIDENED SCOPE EXCEPTION (granted at sign-off), in addition to last round's: this slice MAY edit `crates/server/src/custodian.rs` for exactly two things — (a) wire the deployed grace to the tested constant (`GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`, keeping the strict late-write compile-time check), so the G_orphan boundary test pins the deployed value; and (b) correct the stale comment at `:625-627` that says every `Completed` session keeps its record and slot. Nothing else in `crates/server/src`. Update the `gc.rs:303-308` doc to say production now reads it.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
