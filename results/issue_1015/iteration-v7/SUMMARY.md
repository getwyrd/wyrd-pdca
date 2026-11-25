# Result — issue 1015 / multipart-reaper-open-sessions

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: An `Open` multipart session whose client went silent (never aborted, never
  completed) is never fenced. Its staged parts stay protected on purpose (#637, 0016 decision
  2), its admission slot is never released, and `MAX_SESSIONS` such sessions make a permanent
  `503 SlowDown`. 0016 makes the reaper a hard ordering requirement (`0016:234-268`). Nothing in
  the custodian evaluates an `Open` session today, and no operator command runs such a pass.
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper.rs` drives the
  reaper through the **operator one-shot** (see `Design § the C4 shape`) as a subprocess,
  against a redb store seeded with raw records. Every leg asserts on durable store state.
  **(A) Idle abandonment is reclaimed, with every byte evidenced.** An `Open` session whose
  latest progress instant is older than `W_open`, holding one committed `part:`/`psum:` pair
  and crashed in-flight `sidx:` residue with a lapsed lease, is fenced to `Aborting` and
  drained. **Only the idle arm may be able to reap the fixture:** `W_open < now -
  created_at_millis < W_session`; every progress stamp (`psum:` `committed_at_millis`, `slot:`
  `reserved_at_millis`) is older than `W_open`; every `sidx:` and `slot:` lease has already
  lapsed. Concretely, against the wall clock the test reads at seeding: `created_at_millis =
  now − 14 days`, progress stamps at `now − 10 days`, lease expiries at `now − 9 days`. The
  defaults are bounded (`Scope`) to leave at least three days of margin on each side. 0016 makes
  idle abandonment and the residency ceiling independent predicates (`0016:1910-1915`), and a
  fixture past `W_session` cannot tell a reaper that has no idle arm from a correct one. After
  repeated one-shots converge (bounded, at most N runs, where N is stated in the test):
  `mpu:<id>`, `part:<id>:`, `psum:<id>:`, `sidx:<id>:` and `slot:<id>:` are **all empty**;
  `mpuctl.count` is back to its pre-session value; and an `orphan:` mark exists for **every
  fragment position of both byte sets**, meaning the `sidx:` residue's recorded `staged`
  placement **and** every placement position of the committed `part:` record's chunks (the
  `retire:bytes:{session}` obligation the reap fence installs, `0016:664`). Assert the committed
  part's positions explicitly. Both session-scoped `retire:` ranges are empty
  (`retire_session_range(RetireMode::Bytes, id)` and `(RetireMode::Records, id)`,
  `crates/core/src/multipart.rs:1490`). GC's licence condition holds for every marked
  position: the mark decodes (`decode_orphan_mark`, `crates/core/src/metadata.rs:277`) as
  stamped, not `reclaiming`; its `retire_token()` (`:212`) names no `retire:bytes:` key that is
  present; and no `part:` or `sidx:` record names the position.
  **GC really reclaims them** (the tracker's own acceptance; `cargo xtask ci` evidence after the
  change, not C4's red→green): append a case to the existing
  `crates/custodian/tests/gc.rs` (in-memory metadata and chunk stores, `reconcile_step`
  in-process). Converge the reaper over leg A's fixture and put the fragments of both byte sets
  in the chunk store. Run one GC pass at `now = mark + grace − 1` and assert nothing is deleted,
  then run one at `now = mark + grace` and assert every fragment is deleted. **This case has no
  same-test red.** It calls `reconcile_step` with the reaper context, which does not compile
  against the base's signature (`crates/custodian/src/reconciliation.rs:126-134`). **The binding
  red→green is this file's subprocess test, over the durable marks of leg A.** The physical half's
  red is negation (15) under `Verification posture`. Never report it as a C4 red→green.
  **(B) The IDLE arm never reaps a progressing upload, for as long as the ceiling allows.**
  Three shapes, each of which MUST survive: (i) old parts but an **unexpired owned `sidx:`
  lease** (one `UploadPart` streaming for longer than `W_open`); (ii) a **reserved `slot:` with
  no chunk written yet** and a renewed slot lease (the pre-first-chunk window,
  `0016:1822-1835`); (iii) the newest `psum:` `committed_at_millis` inside `W_open`. Assert the
  records are untouched **and** the admission count is unchanged. Every fixture's
  `created_at_millis` MUST be **strictly inside `W_session`** (0016 bounds even a live session
  by the ceiling, `0016:1886-1888`, `:1908-1913`, `:2256`).
  **(C) The residency ceiling still bites.** A session with *fresh* progress but
  `created_at_millis` older than `W_session` is fenced to `Aborting` and drained (arm (ii),
  `0016:1908-1913`). The ceiling carries no slot preconditions.
  **(E-open) The clock guard suppresses judgments, not the pass.** A session whose
  `clock_source` the reaper does not own is never judged by `W_open` or by `W_session`: an
  over-age foreign-clocked `Open` session survives intact (one fixture per window). **The
  clock-free work still runs:** a foreign-clocked **`Aborting`** session has its `sidx:` range
  walked, its `retire:` obligation drained, and its terminal delete fired, all by #659's drain,
  which the reaper's guard MUST NOT suppress (`0016:1889-1907`, X71). The `Completing` and
  `Completed` halves of this leg are child-3's.
  **(H) A profile mismatch stops the reaper before it acts.** Seed `mpuctl` with a stored
  profile (`AdmissionRecord::profile`, `crates/core/src/multipart.rs:1858`) that differs from
  the custodian's local `Budget` (`:1567`). Assert that the pass **emits the operator signal and
  mutates nothing**: no fence, no mark, no deletion, no counter change. Meanwhile leg A's
  matching-profile fixture still reaps. The reason: the reap fence is an unsplittable batch
  sized by the admitting profile (`MAX_INFLIGHT_PARTS` slot preconditions,
  `MAX_PART_CHUNKS` chunk refs per part). **What profile equality does NOT prove: equal
  operation budgets.** The stored profile has five fields (`multipart.rs:1580-1586`).
  `B_ops` is `KnobSet::batch_ops` (`:5307-5308`), and `KnobSet::profile` leaves it out
  (`:5346-5355`). This slice MUST NOT add it (no format change). Do cites in `build-notes.md`
  the three things operation capacity rests on instead: (1) every reaper batch is sized from
  `MAX_BATCH_OPS` / `MAX_BATCH_BYTES` (`:5106`, `:5067`), and no other budget is defined here
  (`:5103-5105`); (2) `knob_clamps_hold` (`:5454-5457`) and
  `crates/core/tests/multipart_knobs.rs:167` stay green and unmodified; (3) the calibration,
  which is child-3's. **This leg does not cover 0016's staged-set memory guard**
  (`0016:2062-2070`). That is **#806**'s, at `gc.rs:1541-1546`. One comparison, not two: if
  #806 has landed, the reaper calls its comparison. If it has not, write the comparison as one
  `wyrd-custodian` function the reaper calls, with a doc comment naming #806 as the caller to
  add at `gc.rs:1541`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: decision 6's `Open`-session half as a custodian loop dispatched from
  `reconcile_step`:
  - the liveness observable `max(created_at, max psum committed_at, max slot reserved_at)`,
    plus "live while any owned `sidx:` **or** `slot:` lease is unexpired", read
    **source-first** (`slot:` then `psum:`, normative);
  - the idle arm at `W_open`, fencing `Open@E → Aborting@E+1` with the fence's slot
    preconditions pinned, **reusing #895's one fence batch builder** and not copying it;
  - the administrative `W_session` ceiling, without slot preconditions;
  - the `clock_source` guard, which suppresses judgments only and never #659's clock-free
    drain;
  - the profile check, which fails closed with an operator signal before any mutation (H);
  - the pass's byte and operation budget (`MAX_BATCH_OPS` / `MAX_BATCH_BYTES`, never
    re-derived). **This slice adds no batch splitter.** Each fence it sends is one unsplittable
    batch (`0016:647-649`) from #895's builder (`crates/core/src/multipart/abort.rs:77-92`) plus
    the idle arm's slot pins. The splittable work is the drain's (`0016:668`, `:672`), and its
    batching and tests stay with #1012 and #1013. A unit test shows the largest fence this slice
    can build (`MAX_INFLIGHT_PARTS` slot pins, at the profile (H) checks) fits both budgets;
  - the pass's outcome counts on the existing `DurabilityTelemetry` seam (no alerting surface:
    that is #630).

  `reconcile_step` gains the reaper (a `reaper: Option<&ReaperContext<'_>>` parameter, or a
  field on an existing context). Keep the ripple through every caller mechanical, including
  `crates/custodian/tests/gc.rs`'s calls. **Give `ReaperContext` and a public `ReaperPolicy` a
  stable shape** (#633 calls them, and so do child-2 and child-3).

  - The server builds the policy from `ReaperPolicy::default()`, with **no** window flags on
    the CLI. Child-3 then adds its windows inside `crates/custodian` without touching a server
    file.
  - **Route #659's tombstone input through `ReaperPolicy`.** It still says "retain" in this
    slice, and child-3 flips it there.

  Knob values owned here: `W_open` and `W_session`, as days-scale defaults **bounded `W_open ≤
  7 days` and `W_session ≥ 21 days`**, each cited at its definition; and the pass cadence.

  **The operator one-shot:** a valueless flag on `wyrd custodian` that joins `VALUELESS_FLAGS`.
  - It runs **exactly one** `reconcile_step` pass covering the reaper and #659's record-only
    drain (not GC, scrub or reconstruction, which need a fleet), then exits.
  - It does **not** require `--endpoints`.
  - It follows `--reconcile-after-restore`'s shape (`crates/server/src/cli.rs:1166-1230`)
    without the whole-fleet requirement.
  - Use one spelling in the test, the runbook and the docs (Open question 2).

  **Docs** (a merge requirement, `AGENTS.md` "Docs currency"): add the reaper loop and the new
  flag to `docs/design/architecture/06-runtime-view.md` and `08-crosscutting-concepts.md`.
  Child-2 later edits only `06`, and child-3 only `08`, so write each paragraph so that a later
  sentence can be appended.

  **Out of scope:**
  - `Completing` rollback and the over-age edge, tombstone expiry, the foreign-clock
    `Completing`/`Completed` cases, `G_orphan` and the calibration (all child-3);
  - the no-endpoints role, the degraded-fleet driver and the empty-fleet pass (child-2);
  - the drain, the mark guard, the owned walk and the terminal delete (#659);
  - the staged-set preflight (#806);
  - the operator verbs and the foreign-clock alarm (#633);
  - telemetry alerting (#630);
  - FU-2 and FU-4;
  - any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016;
  - any change to the records or state machine of #693, #894, #895, #932, #933, #927, #658 or
    #659 (raise it as a §6 item).

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
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 221 mutants tested in 13m: 197 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.60s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #1015 abandoned-Open-upload reaper reproduces red→green, but two operator one-shot safeguards remain incomplete.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Open-session exit, independent idle/ceiling predicates, profile refusal and sibling boundaries are explicit and testable; the latest accepted scope remains binding (brief.md:26, brief.md:353; docs/design/proposals/draft/0016-multipart-commit-protocol.md:1796). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashed production while retaining the subprocess test: 12 tests ran and all 12 failed, including durable-state nonconvergence; no compilation-only red (reviewer-verification.log:413; crates/server/tests/multipart_reaper.rs:185). |
| C3 Change | PASS | The missing Open-session driver reaches the existing fence and retirement machinery through the production control point, within the accepted slice (crates/custodian/src/reaper.rs:1162; crates/custodian/src/reconciliation.rs:184; crates/server/src/custodian.rs:680). |
| C4 Verification (red→green) | PASS | Restoring the patch independently gives 12/12 green; frozen full CI is green; local full CI has only the harness-copy advisory-gitlink failures described below (reviewer-verification.log:497; gate-logs/C4-ci.log:5067; reviewer-ci.log:4435). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Remove the create-at-open cause from the existing-store command — its existence probe still permits initialization and a false all-clear on a zero-byte database file (F1; crates/server/src/cli.rs:2127; reviewer-empty-store.log:2). |
| T1 Structure | PASS | Shared fence construction, narrow metadata seams and one clock preserve the intended dependency and lifecycle boundaries (crates/core/src/multipart/abort.rs:373; crates/server/src/custodian.rs:629; crates/custodian/src/retirement.rs:565). |
| T2 Shape | PASS | The policy supports the named sibling extensions, callsite changes are mechanical, and living architecture documents cover the new operation and persisted cursor (crates/custodian/src/reaper.rs:291; docs/design/architecture/06-runtime-view.md:94; docs/design/architecture/08-crosscutting-concepts.md:113). |
| T3 Runtime | PASS | Work and fence sizes are bounded, clock ownership gates judgments, and required production-interleaving DST cases pass; the accepted slot-cycle exception remains explicit (crates/custodian/src/reaper.rs:709, crates/custodian/src/reaper.rs:1185; reviewer-ci.log:4319; crates/dst/tests/custodian.rs:8735). |
| T4 Contribution | FAIL | The frozen review still contains two grounded, unresolved operator issues, F1/F2; affected-path prior art was checked, and the contribution-artifact audit is N/A until publish (gate-logs/T4-batch-review.log:10; gate-logs/T4-contribution.log:10; reviewer-prior-art.json:2). |
| T5 Judgment | NEEDS-HUMAN [impl] | Identify the resolved network store in the outcome — backend-only labels cannot distinguish a successful pass against the wrong cluster (F2; crates/server/src/cli.rs:1722). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the seven-day idle/thirty-day residency policy and release ordering, and assign the accepted slot-cycle follow-up its tracker before production upload creation — its regression remains deliberately ignored (crates/custodian/src/reaper.rs:200; crates/custodian/src/reaper.rs:206; crates/dst/tests/custodian.rs:8735; brief.md:353). |

All `crates/`, `docs/` and `AGENTS.md` citations refer to the supplied `$PDCA_TARGET`; evidence files refer to this review directory. The target is the disposable, self-contained base snapshot with the patch applied. Stash/pop completed, no stash remains, and `git apply --reverse --check ../patch.diff` succeeds after verification. No product fix was made.

Two implementation findings remain:

1. **F1 — P2: an existing file is not necessarily an existing database.** `open_existing_local_meta_redb` checks `try_exists()` and then calls `RedbMetadataStore::open`, which uses `Database::create` (`crates/server/src/cli.rs:2116`; `crates/metadata-redb/src/lib.rs:66`). I created an empty `meta.redb` inside the review directory and ran the built one-shot. It grew from **0 to 1,056,768 bytes**, exited **0**, and reported **“satisfied — nothing to reap or drain”** (`reviewer-empty-store.log:2`, `reviewer-empty-store.log:13`). The path is printed, satisfying the brief's minimum identification alternative, but the implementation and living documentation additionally promise that this command creates no store (`docs/design/architecture/06-runtime-view.md:94`). Use a non-creating database open and extend the existing missing-store regression to a zero-byte file; another existence check leaves the create-capable operation in place. The separate check/open removal race follows from the same operation but was not needed for this reproduction.

2. **F2 — P2: network-store outcomes identify a backend, not the selected store.** Both TiKV and FoundationDB branches return constant names (`crates/server/src/cli.rs:1722`, `crates/server/src/cli.rs:1724`), although their selected targets come from PD endpoints and a configured/default cluster-file path (`crates/server/src/cli.rs:302`, `crates/server/src/cli.rs:313`). Consequently two differently configured clusters have identical store identifiers in the one-shot's final result. Carry the resolved target identity from opening the backend into that result, with tests distinguishing two configurations. This is grounded in the compiled selection/helper code; no live wrong-cluster run is claimed.

Verification supports the primary reaper behavior, with these precise limits:

- **Independent red→green:** `cargo test --offline --locked -p wyrd-server --test multipart_reaper`, with production stashed and the new test retained, then restored: **0 passed / 12 failed → 12 passed / 0 failed**. Leg B's red exit is an unknown-flag assertion, not proof of unsafe reaping on the base; its liveness evidence comes from the green assertions and the DST interleavings. The durable-state legs A/C/E-open fail on the unchanged base store, as intended (`reviewer-verification.log:269`, `reviewer-verification.log:413`, `reviewer-verification.log:497`).
- **Independent full CI:** typos, docs lint/render/link audit, formatting, clippy, builds, affected tests, conformance, statics and deployment guard ran. The GC grace-boundary case, deployed-loop case, largest-fence test and required reaper DST rows passed (`reviewer-ci.log:1580`, `reviewer-ci.log:1617`, `reviewer-ci.log:2825`, `reviewer-ci.log:4319`). The custodian DST suite finished **42 passed, 1 intentionally ignored** (`reviewer-ci.log:4355`). Full CI nevertheless exited **1**: the disposable snapshot omits the `.github/advisory-db` gitlink, causing its index test and dependency-audit prerequisite to fail (`reviewer-ci.log:3779`, `reviewer-ci.log:4435`; `xtask/tests/advisory_db.rs:65`). This is a harness-state caveat, not a patch defect. The frozen run contains the actual pinned database audit and complete green CI (`gate-logs/C4-ci.log:4413`, `gate-logs/C4-ci.log:5067`). Thus the brief's `typos` and `docs-renderer` dependencies were actually exercised.
- **TiKV feature gate:** independently repeated both clippy commands for the metadata crate and server `tikv,etcd` selection arms; both exit successfully (`reviewer-tikv.log:103`, `reviewer-tikv.log:202`). This establishes compilation, not live cluster operation.
- **C5-mutants: PASS from frozen evidence, not independently reproduced.** The instance-scoped wrapper's captured output records an unmutated baseline and **221 tested: 197 caught, 24 unviable**, with no reported survivors (`gate-logs/C5-mutants.log:10`). It does not enumerate the brief's individual negations, so those are not separately claimed as reproduced.
- **C4-diff-cov: unmeasured, accepted stack caveat.** The log proves only that applying onto `origin/main` failed (`gate-logs/C4-diff-cov.log:10`). The brief expressly keeps the unmerged prerequisite stack and accepts unavailable coverage (`brief.md:324`, `brief.md:353`); this is neither a compile defect nor a new rebase request.
- **T4-contribution: N/A.** The artifacts are absent by design and the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`). There is no missing-log escalation. No `INTEGRATION.md` was supplied or found in the target, so no additional project-specific human-only items are asserted.

The frozen batch-review's eight entries reduce to four distinct concerns (`gate-logs/T4-batch-review.log:10`). Its three empty-store entries are F1; its three network-identification entries are F2. The first-Create/drain race is already explicitly deferred to #659's layer (`brief.md:308`) and is not reopened. The remaining error-precedence entry is not upheld as the rubric's unknown-result-to-clean-conflict defect: reconstruction handles `Conflict` as an outcome (`crates/custodian/src/reconstruction.rs:456`); the two-error branch returns an error and separately logs the later error (`crates/custodian/src/reconciliation.rs:232`), and its deployed caller logs and starts a fresh pass (`crates/server/src/custodian.rs:688`). No clean-conflict return or unsafe retry of an ambiguous batch is established here. The accepted slot-cycle gap, larger-profile clean-lap deferral to #508, and previously rejected damaged-ledger/separate-context concerns remain settled.

Prior-art checking covered **all 46 affected paths** against merged `main` history and intersected the changed-file lists of **all 19 closed, unmerged PRs** with those paths (`reviewer-prior-art.json:2`). The overlapping non-dependency work is [PR #647](https://github.com/getwyrd/wyrd/pull/647), concerning segmented maps, and [PR #336](https://github.com/getwyrd/wyrd/pull/336), concerning shared mutable state; neither is an abandoned-upload reaper. The new reaper path has no merged history or closed PR match. Only `Cargo.lock` reached the 100-commit history cap; its patch change adds the DST crate's existing `futures-util` dependency, rather than reviving a rejected upgrade. No duplicate prior solution was found.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1, multipart reaper, `Open` half), iteration 7

Line references are to the patched tree at `$PDCA_TARGET`. I re-ran the binding test and two probes in a scratch copy (the toolchain was available).

## Findings

- NEEDS-HUMAN [impl] — **A zero-byte `meta.redb` is turned into a new store and reported "satisfied", exit 0.** `crates/server/src/cli.rs:2118` only checks that the path exists. `:2127` then calls `RedbMetadataStore::open`, which is `Database::create` (`crates/metadata-redb/src/lib.rs:67`). I reproduced it with the built binary: `: > DIR/meta.redb; wyrd custodian --data-dir DIR --reap-abandoned-uploads` grew the file from 0 to 1,056,768 bytes and printed `multipart reaper pass satisfied — nothing to reap or drain [store: redb at DIR/meta.redb]`, exit 0. That breaks the usage text's own promise at `cli.rs:496` ("on a redb --data-dir that holds no metadata store … it creates none"). It also breaks iteration 6's item 4: never report all-clear on a store the one-shot just made. It also wipes the evidence that the file was empty, for example after a failed backup copy. The fix is small and I checked it: `redb::Database::open` refuses the same file (`InvalidData: "Database file is empty and creating a new database was not requested"`). A non-creating adapter method (say `RedbMetadataStore::open_existing`) used at `cli.rs:2127` closes this case, and also the window between the existence check and the open, which T4 also flagged. The regression test is `the_reaper_one_shot_opens_only_a_store_that_exists` (`cli.rs` tests) with a zero-length file. Today that test covers only a missing file.

- NEEDS-HUMAN [impl] — **On TiKV and FDB the outcome line does not say which cluster it ran on.** `cli.rs:1722` prints `tikv` and `:1724` prints `fdb`. The address is one env read away: `WYRD_TIKV_PD_ENDPOINTS` (`cli.rs:302`), and `WYRD_FDB_CLUSTER_FILE` or the default cluster file. Iteration 6 added the `[store: …]` tail so an operator who aimed at the wrong store can tell from that line alone. That works for redb but not for the networked backends. Low severity, since no production path creates sessions yet. It is still the same "satisfied, but on which store?" gap, and T4 raised it three times this round.

- NEEDS-HUMAN [human] — **The accepted slot-cycle gap still has no tracker number.** The `#[ignore]` reason at `crates/dst/tests/custodian.rs:8739` and the module doc at `crates/custodian/src/reaper.rs:137-139` still say "tracker issue to be filed". The rubric settles a deferral only when it carries `#N`. The gap needs a 0016 change before #668/#508 create sessions, so a human should file the issue and the builder should cite it in both places. This is already on the sign-off list. I'm noting that it is still open, not raising it again.

## Attempts that did not refute the fix

- **Red→green evidence.** I re-ran `cargo test -p wyrd-server --test multipart_reaper` on the patched tree: 12 of 12 pass. The frozen `gate-logs/C4-verify.log` shows all 12 red on the base. Every red has one cause: the flag is unknown, so parsing fails (`needs a value`). The brief requires exactly that design (`brief.md:103-116`), so all 12 being red only shows that the binary lacks the one-shot. What each leg can catch rests on the store-state assertions, the builder's negations (withheld from me) and C5 (197/221 caught, 0 missed, 24 unviable). The tests drive the real binary through `reap_abandoned_uploads_once` → `reconcile_pass` → `reconcile_step` (`crates/server/src/custodian.rs:453-492`), which is the production path, not a copy.
- **Deployed-loop wiring.** I ran the negation the sign-off asked for in my scratch copy: `custodian.rs:680` `Some(&reaper)` → `None`. `deployed_loop_reaps_an_idle_open_upload_and_tears_it_down` then fails (`crates/server/tests/custodian_gc.rs:1635`, the session left `Open@0`). The wiring is covered.
- **Idle fence missing a fresh staging write.** The slot pins (`reaper.rs:1095-1110`) cover only `slot:`, not `sidx:`. But the intent batch rewrites the slot (`crates/core/src/multipart/stage.rs:2333-2336`), and so does the renewal (`:1622-1624`), as `0016:1868-1877` requires. A new chunk or a renewal after the reads therefore makes the fence conflict. No hole beyond the accepted slot-cycle gap.
- **Lease checks with no clock-skew margin** (`reaper.rs:896`, `:1079`). These match `0016:1841-1886`: the pins close the read-then-renew race, and a stamp ahead of the clock is held (`:854`, `:892`). Not a defect.
- **Fence too large to send.** `largest_fence_fits_both_batch_budgets` builds the largest session Create admits, with the widest epoch and attempt counter and all 16 widest slot pins, and checks the cost against a count made independently. The `Aborting` value stays under `MAX_VALUE_BYTES`.
- **Paging that never ends.** `checked_page` (`crates/custodian/src/gc.rs:2092-2123`) refuses a page that is too long or that does not advance, so `walk` and the three range readers end.
- **T4's "reconstruction error hides `CommitUnknownResult`"** (`crates/custodian/src/reconciliation.rs:225-242`). Both errors are `ReconcileError::Store`, and the second is logged. No caller classifies a step's error: the deployed loop logs it and retries (`custodian.rs:684-695`), and the one-shot has no reconstruction input. Nothing is reported as a clean `Conflict`, so the rubric's ordering rule is not broken in practice.
- **T4's "first Create/Abort lands between the last ledger read and the drain"** (`reconciliation.rs:199`). This is the documented step-5 gap (`reaper.rs:103-109`). It was deferred to #659's follow-ups at iteration 3's sign-off, so I treated it as settled and did not raise it again.

I did not write to `$PDCA_TARGET` except for one empty probe file, which I deleted straight away. All builds and probes ran in `$PDCA_SCRATCH/pdca-adversary-1015-probe`.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:2127`: `open_existing_local_meta_redb` checks existence but then calls `RedbMetadataStore::open`, which uses `Database::create` (`crates/metadata-redb/src/lib.rs:67`). If the file disappears between those calls, the one-shot creates a fresh store and reports “satisfied — nothing to reap or drain”; an existing zero-length file also reaches the create-capable opener. Use an open-existing database operation and cover the empty-file case, so this maintenance command cannot initialize the store it certifies.

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:1722`: The new outcome-line store identifier is only `tikv` or `fdb` for network backends. Pointing `WYRD_TIKV_PD_ENDPOINTS` or `WYRD_FDB_CLUSTER_FILE` at another cluster therefore produces an indistinguishable successful “nothing to reap” report, defeating the operator safeguard this identifier supplies for redb. Carry the resolved connection identity into the outcome line and test that different configured stores produce different identifiers.

No additional reuse, simplification, or efficiency findings. Reviewed the target source and frozen gate evidence; tests were not rerun. The brief's accepted deferrals are not reopened.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Remove the create-at-open cause from the existing-store command — its existence probe still permits initialization and a false all-clear on a zero-byte database file (F1; crates/server/src/cli.rs:2127; reviewer-empty-store.log:2).
- [ ] T5 Judgment — Identify the resolved network store in the outcome — backend-only labels cannot distinguish a successful pass against the wrong cluster (F2; crates/server/src/cli.rs:1722).
- [ ] Validation — fitness-to-purpose — Confirm the seven-day idle/thirty-day residency policy and release ordering, and assign the accepted slot-cycle follow-up its tracker before production upload creation — its regression remains deliberately ignored (crates/custodian/src/reaper.rs:200; crates/custodian/src/reaper.rs:206; crates/dst/tests/custodian.rs:8735; brief.md:353).
- [ ] **A zero-byte `meta.redb` is turned into a new store and reported "satisfied", exit 0.** `crates/server/src/cli.rs:2118` only checks that the path exists. `:2127` then calls `RedbMetadataStore::open`, which is `Database::create` (`crates/metadata-redb/src/lib.rs:67`). I reproduced it with the built binary: `: > DIR/meta.redb; wyrd custodian --data-dir DIR --reap-abandoned-uploads` grew the file from 0 to 1,056,768 bytes and printed `multipart reaper pass satisfied — nothing to reap or drain [store: redb at DIR/meta.redb]`, exit 0. That breaks the usage text's own promise at `cli.rs:496` ("on a redb --data-dir that holds no metadata store … it creates none"). It also breaks iteration 6's item 4: never report all-clear on a store the one-shot just made. It also wipes the evidence that the file was empty, for example after a failed backup copy. The fix is small and I checked it: `redb::Database::open` refuses the same file (`InvalidData: "Database file is empty and creating a new database was not requested"`). A non-creating adapter method (say `RedbMetadataStore::open_existing`) used at `cli.rs:2127` closes this case, and also the window between the existence check and the open, which T4 also flagged. The regression test is `the_reaper_one_shot_opens_only_a_store_that_exists` (`cli.rs` tests) with a zero-length file. Today that test covers only a missing file.
- [ ] **On TiKV and FDB the outcome line does not say which cluster it ran on.** `cli.rs:1722` prints `tikv` and `:1724` prints `fdb`. The address is one env read away: `WYRD_TIKV_PD_ENDPOINTS` (`cli.rs:302`), and `WYRD_FDB_CLUSTER_FILE` or the default cluster file. Iteration 6 added the `[store: …]` tail so an operator who aimed at the wrong store can tell from that line alone. That works for redb but not for the networked backends. Low severity, since no production path creates sessions yet. It is still the same "satisfied, but on which store?" gap, and T4 raised it three times this round.
- [ ] **The accepted slot-cycle gap still has no tracker number.** The `#[ignore]` reason at `crates/dst/tests/custodian.rs:8739` and the module doc at `crates/custodian/src/reaper.rs:137-139` still say "tracker issue to be filed". The rubric settles a deferral only when it carries `#N`. The gap needs a 0016 change before #668/#508 create sessions, so a human should file the issue and the builder should cite it in both places. This is already on the sign-off list. I'm noting that it is still open, not raising it again.
- [ ] `crates/server/src/cli.rs:2127`: `open_existing_local_meta_redb` checks existence but then calls `RedbMetadataStore::open`, which uses `Database::create` (`crates/metadata-redb/src/lib.rs:67`). If the file disappears between those calls, the one-shot creates a fresh store and reports “satisfied — nothing to reap or drain”; an existing zero-length file also reaches the create-capable opener. Use an open-existing database operation and cover the empty-file case, so this maintenance command cannot initialize the store it certifies.
- [ ] `crates/server/src/cli.rs:1722`: The new outcome-line store identifier is only `tikv` or `fdb` for network backends. Pointing `WYRD_TIKV_PD_ENDPOINTS` or `WYRD_FDB_CLUSTER_FILE` at another cluster therefore produces an indistinguishable successful “nothing to reap” report, defeating the operator safeguard this identifier supplies for redb. Carry the resolved connection identity into the outcome line and test that different configured stores produce different identifiers.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 471 KB (threshold 100 KB); patch touches 46 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (attempt 8, narrow). The logic is ~100 KB source / ~1,200 code lines; the size backstop trips on test volume, not two outcomes, so no re-plan. Red->green, full CI and mutants pass. Fix only these two, each with a test that fails today: 1. F1 — zero-byte meta.redb is initialized and reported "satisfied" (crates/server/src/cli.rs:2118-2127 -> RedbMetadataStore::open = Database::create, crates/metadata-redb/src/lib.rs:67). Add a non-creating open (e.g. RedbMetadataStore::open_existing over redb::Database::open) and use it in the one-shot, which also closes the check-then-open race. Extend the_reaper_one_shot_opens_only_a_store_that_exists with a zero-length file: non-zero exit, file left at 0 bytes. 2. F2 — TiKV/FDB outcome line names only the backend (cli.rs:1722, :1724). Carry the resolved target identity (WYRD_TIKV_PD_ENDPOINTS; WYRD_FDB_CLUSTER_FILE or the default cluster file) into the [store: ...] tail, with a test showing two configurations give different identifiers. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item at sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 stays deferred to #508; recorded rejections in review-rejected.md stand. Do not widen scope beyond F1/F2.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
