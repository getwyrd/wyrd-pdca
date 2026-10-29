# Result — issue 657 / multipart-upload-part-staging

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Sessions can be created (#894) and aborted (#895), but nothing stages a part. This
  slice adds `UploadPart`'s staging as `wyrd-core` functions over `MetadataStore` and a
  placement-aware chunk store, answering in the landed vocabulary (`ReserveOutcome`
  `crates/core/src/multipart.rs:4590`, `UploadPartOutcome` `:4606`, `Refusal` `:4487`): claim
  one in-flight slot `slot:<id>:<k>`; stream the part's chunks, registering each as an owned
  `sidx:` staging entry **before** its fragments are written, in a batch fenced on the session
  being `Open@E` and on no selected D-server having a drain recorded; renew the owned leases and
  the slot together; and commit the `part:` + `psum:` pair, the slot release and the `sidx:`
  deletes in one batch. Staged placement selects against the topology **minus draining
  servers**, and a drain that lands mid-stage makes the part re-plan, never refuse.
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part.rs`, over
  `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store (the shape of
  `Fleet`, `crates/core/tests/domain_placement.rs:113`), and a test-local store wrapper that can
  log batches and inject an outcome or a write before a chosen commit. Every session in it is
  made by #894's Create. It shows all of the following.
  **(A) Staging and commit.** Parts staged **out of order** (3, then 1, then 2), at least one of
  them more than one chunk, each commit to `Committed { part_number, digest }`. While a part is
  mid-stage (after its first chunk's intent, before its commit), `scan("sidx:")` is non-empty and
  `scan("pending:")` is empty. After each commit, that part's `part:` and `psum:` records decode
  and agree (chunk count, length, digest), and no `sidx:` entry or `slot:` record of that part
  attempt remains. The wrapper's log shows every chunk's `sidx:` commit **before** the first
  fragment put of that chunk (`0016:1788`). **The write deadline is exact.** The test passes a
  controlled clock and a write window `W` (the functions take both as inputs, see Design), and
  the chunk-store double records `deadline_millis` (`crates/traits/src/lib.rs:1229`; #638) on
  every staged fragment put. Each one equals `t_auth + W`, where `t_auth` is the clock's reading
  taken before that chunk's intent batch was committed. With the clock advanced between the
  intent commit and the fragment dispatch (a delayed dispatch, the wrapper advancing it right
  after the intent commit returns), the deadline is still `t_auth + W`: never `t_dispatch + W`,
  never `None`, and never an unbounded value such as `u64::MAX`.
  **(B) The drain fence (kills the fence mutants of the discontinued patch).**
  - **B1:** the wrapper writes `desired:dserver:<S>` for a server `S` the plan selected, **just
    before** that chunk's intent commits. The intent must fail and re-plan; the part still
    commits, and no `sidx:`, `part:` or fragment of it names `S`. Removing the per-server
    `require_absent(desired:dserver:<S>)` from the intent batch makes B1 fail.
  - **B2:** with `desired:dserver:<S>` present **before** staging, and `S` the server the
    selector prefers (lowest utilization), the part commits, nothing names `S`, and the wrapper
    sees **no** intent conflict. Planning against the full topology instead of the topology minus
    draining servers makes B2 fail.
  - **B3:** with 4 of 12 single-server domains draining and a scheme that fits in the remaining 8
    (e.g. RS(4,2)), a part stages and commits. With too few non-draining domains for the scheme,
    the answer is a typed error, never `NoSuchUpload` for an `Open` session.
  **(C) Slots.** `MAX_INFLIGHT_PARTS` reserves (`multipart.rs:5032`) succeed on one session and
  the next answers `Refused(SlowDown { pressure: Backpressure::InflightParts { .. } })`
  (`multipart.rs:4470`, `:4528`); a reserve against a non-`Open` session
  answers `Refused(NoSuchUpload)` and writes nothing. A reserve whose batch lands but whose
  outcome is `CommitUnknownResult` adopts its own slot on re-read (same `attempt_id`): the
  session's held-slot count equals the number of live attempts (`0016:1779`). With
  `may_still_commit: true` and the slot read absent, the reserve retries the **same** index with
  the same `attempt_id`, never a different one (`0016:1707-1716`).
  **(D) Renewal.** Each renewal batch rewrites the attempt's own `slot:` record in the same batch
  as its owned `sidx:` leases (`0016:349`, `:659`); a renewal after the session leaves `Open`
  refuses and resurrects nothing.
  **(E) A lost commit is classified, never compensated blind** (`0016:1584-1647`).
  - The session fenced to `Aborting` between the last intent and the commit: the commit answers
    `Refused(NoSuchUpload)` and writes nothing (the residue is the drain's).
  - A same-part rival committed first: the loser commits one `retire:bytes:` obligation for its
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3891`),
    releases its slot and deletes its `sidx:` entries in one batch, and answers a refusal.
  - A commit whose batch lands but answers `CommitUnknownResult`: the writer re-reads and finds
    **its own** record (the exact `part:` bytes it built; its chunk ids are its own, so no other
    attempt writes those bytes), answers `Committed`, and releases the slot exactly once.
  - **A re-upload whose batch did not land** answers `CommitUnknownResult { may_still_commit:
    false }`: the re-read finds the **prior** part. The writer must not answer `Committed` for it;
    it carries on as for a commit that did not land, and ends with its own part committed.
  - **A held batch** (`may_still_commit: true`; the wrapper holds the batch and lands it later,
    as `crates/core/tests/multipart_abort.rs:961-980` does): the re-read finds the prior part (or
    none). The writer neither answers `Committed` nor compensates on that read alone. Every batch
    it sends next requires its own slot at the bytes the held batch also requires, so at most one
    of them lands. Run it three times: (i) the held batch released before the writer's next
    batch; (ii) released after the writer has returned; (iii) every later batch of the writer
    answered `Conflict` without being applied until its rounds run out, then the held batch
    released. In every run exactly one of {its own part, its compensation} is durable at the end,
    the slot is released exactly once, and the answer is `Committed` only if its own part is the
    one that landed. In (iii) the writer returns an in-doubt error that keeps the
    `may_still_commit` doubt, as Abort's does (`crates/core/src/multipart/abort.rs:264-274`).
  - **A same-number rival plus an unknown outcome:** the re-read finds the rival's record (not
    its own bytes). Never `Committed`; the writer compensates under its own slot.
  **(F) Re-upload and ceilings.** Re-uploading a committed part number replaces it under
  `require(part == prior)` and puts the prior part's chunks in a `retire:bytes:` obligation in
  the same batch. **The staged-chunk ceiling is a read-time check, and 0016 allows the race it
  leaves** (`0016:1443-1450`; `multipart.rs:5034-5042`). A commit whose read of the session's
  `psum:` range shows it would take the cumulative staged chunks past `MAX_STAGED_CHUNKS`
  (`multipart.rs:5043`) answers `Refused(EntityTooLarge { .. })` and compensates what it staged.
  Concurrent commits may overshoot by at most `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`. **Witness,
  forced by the wrapper:** with `psum:` records planted one chunk below the ceiling, two attempts
  on two different part numbers, each holding a one-chunk part, both read that one chunk of
  headroom (the wrapper holds the first
  commit until the second has committed, then releases it). Both commit, and the total is
  `MAX_STAGED_CHUNKS + 1`, inside the bound. A third commit after them refuses. Do **not** add a
  shared counter or a session-wide fence to stop the overshoot: 0016 rejects that
  (`0016:1686-1742`). Use a test-sized ceiling only if the function takes it as a parameter;
  otherwise plant `psum:` records to approach it.
  **(G) The whole gate.** `cargo xtask ci` is green on the patched tree. This is the tracker's
  own acceptance line, and `AGENTS.md:41-45` requires it for any change that can affect the Rust
  build. It matters here because this slice also edits `wyrd-custodian` and the architecture docs,
  which `cargo test -p wyrd-core` does not cover.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `UploadPart`'s staging as store-I/O functions in `wyrd-core`: slot reserve, owned
  intent, renewal, part commit with its classification, the live-session compensation, re-upload
  of a part number, and the per-part and cumulative chunk ceilings; staged placement against the
  topology minus draining servers, with re-plan on a drain-fence conflict; one home in core for
  the `desired:dserver:<S>` key so core and the custodian spell it once; the writer-side
  constructors the slot and part records lack; the docs sentence; the new test file. / out of
  scope:
  - Complete, the segment committer and publication (#658, #653);
  - draining any `retire:` obligation, the owned-`sidx:` walk, and the terminal delete (#659);
  - the reaper (#625); seeded DST races for the protocol (#660);
  - the D-server side of the `W_write` deadline (built by #638; this slice only passes the
    deadline on every staged write);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 82.9% — 748 of 902 instrumentable changed lines executed (floor 80%); 902 of 1841 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 120 mutants tested in 29m: 31 missed, 48 caught, 28 unviable, 13 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_657/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.29s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #657's multipart UploadPart staging: two reproduced correctness defects and one ineffective regression test require builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The staging-only scope defines ownership, drain fencing, outcome classification and explicit adversarial acceptance cases; production wiring is excluded (`brief.md:24`, `brief.md:279`, `brief.md:183`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality without a pre-existing staging API; reverting production makes the discriminator fail to compile, rather than execute a behavioral red (`brief.md:159`, `gate-logs/C4-verify.log:24`). |
| C3 Change | PASS | The seven changed files stay within the planned core/custodian/docs surface and preserve the drain-key bytes; the living architecture includes the new API (`crates/core/src/metadata.rs:101`, `docs/design/architecture/05-building-block-view.md:244`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared net-new verification exception — the restored suite passes 23/23, but the reverted test cannot execute, so there is no behavioral pre-fix red (`brief.md:167`, `reviewer-evidence/red.log:24`, `reviewer-evidence/restored-green.log:30`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair ambiguous-commit classification — identical empty-part bytes leak a successful attempt's slot, and mixed session/slot reads acknowledge a commit that never landed (`crates/core/src/multipart/upload_part.rs:810`, `crates/core/src/multipart/upload_part.rs:1139`; findings 1–2). |
| T1 Structure | PASS | Core retains the narrow metadata/chunk-store seams and the custodian depends on core's shared key spelling; no reverse dependency or runtime dependency is introduced (`crates/core/src/multipart/upload_part.rs:450`, `crates/custodian/src/desired_state.rs:28`). |
| T2 Shape | PASS | Writer constructors preserve the existing validated record shapes, summaries derive from the committed record, and retirement obligations retain their keyed identity (`crates/core/src/multipart.rs:2764`, `crates/core/src/multipart.rs:3010`, `crates/core/src/multipart.rs:4080`). |
| T3 Runtime | FAIL | The two reproduced false-success paths violate the promised slot-release and commit-result semantics despite bounded retry loops and one supplied clock (`crates/core/src/multipart/upload_part.rs:1113`, `crates/core/src/multipart/upload_part.rs:1148`, `reviewer-evidence/probes.log:18`, `reviewer-evidence/probes.log:27`). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design and must receive their substantive audit at publish; the deferred contribution row is not a failed or missing check (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Strengthen the ambiguous-reservation regression — its permanently lowest free index makes removal of pinning invisible, leaving the mandatory double-slot failure untested (`crates/core/tests/multipart_upload_part.rs:1278`, `crates/core/src/multipart/upload_part.rs:682`; finding 3). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected staging-only slice meets #657's intended acceptance — store tests establish this API's behavior, while production S3 wiring and seeded multipart DST remain explicitly assigned to #669/#660 (`brief.md:183`, `crates/core/src/multipart/upload_part.rs:21`). |

The advisory conclusion rests on two executable counterexamples and a demonstrated testing blind spot. Source citations below are relative to `$PDCA_TARGET`; `gate-logs/`, `brief.md` and `reviewer-evidence/` refer to this review bundle. The disposable target matches all seven post-patch blob hashes, and the stash/pop experiment restored it exactly. No production fix was made.

1. **P1 — Abort between reads can produce success for an unapplied commit.** `look()` reads the session before the slot and part (`crates/core/src/multipart/upload_part.rs:1071`). After an unknown result with `may_still_commit: true`, Abort can fence the session and teardown can delete the slot between those reads. `judge()` then combines the old Open session with the absent slot and infers that a pending commit landed (`crates/core/src/multipart/upload_part.rs:1134`). The independent test withholds the commit, runs the real Abort, and models teardown's removal of owned records during that read window. It observed `Ok(Committed)`, `part_present=false`, and `state=Aborting` (`reviewer-evidence/probes.log:27`). Establish a consistent lifecycle observation before making this inference, or retain uncertainty when no positive completion evidence exists. Add this interleaving as a regression; the drain implementation itself remains the separately tracked #659 work.

2. **P2 — Empty re-uploads can leak slots after an unknown result.** An empty body stages no chunks (`crates/core/src/multipart/upload_part.rs:951`). Its new `PartRecord` can therefore equal the previous empty part byte-for-byte when the timestamp and epoch match (`crates/core/src/multipart/upload_part.rs:1423`). `Look::shows` treats that equality as proof this attempt committed, without establishing its slot release (`crates/core/src/multipart/upload_part.rs:810`). With a fixed clock, one successful empty upload followed by an unapplied empty re-upload returned `Committed` and left one slot reserved, for both values of `may_still_commit` (`reviewer-evidence/probes.log:18`). Repeated affected attempts consume the in-flight allowance. Empty parts need completion evidence that cannot belong solely to the previous attempt, plus a regression asserting slot release.

3. **P2 — The same-index regression does not distinguish pinning from an ordinary free-slot scan.** The fixture starts empty and holds index zero without changing any other slot (`crates/core/tests/multipart_upload_part.rs:1280`). Returning `None` from `ReserveDoubt::pinned` still passes all 23 supplied tests (`reviewer-evidence/mutant-C-no-pinned-reserve-index.log`), independently reproducing the frozen survivor (`gate-logs/C5-mutants.log:27`). A discriminating schedule first occupies index zero, holds the tested reserve at index one, then releases zero before the retry. My additional case passes against the supplied implementation: index one remains pinned, the late batch conflicts, and one slot exists (`reviewer-evidence/lower-slot-baseline.log:6`). Against the no-pinning mutant it reserves zero, the held index-one batch also commits, and two slots exist (`reviewer-evidence/lower-slot-mutant.log:7`). Add that schedule to satisfy the required ambiguous-reserve discriminator (`brief.md:180`).

The executable evidence is retained in this sandbox. To reproduce the first two failures and the passing additional reservation case against the unchanged target:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-evidence/pdca-reviewer-657-build" \
TMPDIR="$PWD/reviewer-evidence/pdca-reviewer-657-scratch" \
cargo test --offline \
  --manifest-path reviewer-evidence/pdca-reviewer-657-probe/Cargo.toml \
  reviewer_ -- --nocapture
```

The harness uses the target's lockfile versions and real `RedbMetadataStore`; additional Rust cases live outside the target. `reviewer-evidence/run-required-mutants.py` records the temporary mutations of a separate core copy. Its baseline passes all 23 tests. The two drain-fence mutations, missing renewal slot write, compensation without rereading, presence-as-identity, and all three deadline mutations fail their named tests; removal of reserve pinning survives. No capability probe or load-time symptom guard was introduced. The tracked DST deferrals (#660/#665) are respected rather than re-raised.

All frozen gate rows were accounted for:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS (frozen evidence) | The captured run executes typos, docs lint/render, compilation, tests, dependency checks and conformance, ending in `all checks passed` (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3665`, `gate-logs/C4-ci.log:4290`). Independent rerun details follow below. |
| C4-verify | NEEDS-HUMAN | The logged exit 77 is the declared compile-only pre-fix limitation; independent stash/test/pop/test reproduces compilation failure followed by 23 passing tests (`gate-logs/C4-verify.log:45`, `reviewer-evidence/red.log`, `reviewer-evidence/restored-green.log`). |
| C4-diff-cov | PASS (frozen evidence) | The log reports 748/902 instrumentable changed lines executed, 82.9%, above the 80% floor; this is coverage evidence, not proof of the missing cases (`gate-logs/C4-diff-cov.log:736`). The instance-scoped coverage wrapper is not supplied here. |
| C5-mutants | FAIL (advisory) | The captured campaign reports 31 missed, 48 caught, 28 unviable and 13 timed out; the focused independent experiments substantiate the meaningful pinning survivor without treating every survivor as a defect (`gate-logs/C5-mutants.log:57`, `reviewer-evidence/required-mutants.json`). |
| T4-batch-review | FAIL | Its four messages reduce to two distinct concerns: three report the empty-part case, one reports the read-order race. Both were independently reproduced above (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Artifact drafting occurs after Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Both feature-enabled clippy commands completed in the frozen log and in the independent run; this establishes compilation, not a live TiKV integration test (`gate-logs/host-tikv.log:209`, `reviewer-evidence/host-tikv.log`). |

The named external dependencies were exercised: real `typos` and the docs renderer ran, and the renderer generated 99 pages with a successful link audit (`reviewer-evidence/ci.log:2`). The independent CI invocation encountered a host permission error on the default advisory-database lock, not a patch error (`reviewer-evidence/ci.log:3657`). The invocation completed with exactly those two host failures; all other checks, including the DST campaign, passed. Both affected dependency scans subsequently passed using copies of the same policies with only `advisories.db-path` relocated into this sandbox (`reviewer-evidence/deny-local.log:1`, `reviewer-evidence/deny-all-local.log:1`).

The prior-art check was independently repeated by all seven affected paths against the brief's exact base SHA. Complete path histories returned 25/15/15/6/39 commits for the five existing files and none for the two new staging files. All 398 closed PRs were enumerated, and file lists of all 19 unmerged ones were checked: rejected PR #647 overlaps metadata, write and desired-state files; none supplies this staging API. Issue #636 is confirmed closed `not_planned`. The results are preserved in `reviewer-evidence/prior-art-history.json` and `reviewer-evidence/prior-art-closed.json`; the brief's discontinued local patch was not supplied and no other checkout was searched. No unresolved upstream-ahead ambiguity was found. The target's standing review rubric was applied, including serialization identity, narrow seams, one clock, and settled deferrals.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #657 UploadPart staging (advisory)

Verdict: **refuted in part.** The drain fence, the write deadline and the main commit paths hold up.
But I could break three classification paths with concrete inputs. I appended three probe tests to a
scratch copy of `crates/core/tests/multipart_upload_part.rs`, using the file's own wrapper. All three
fail on the patched tree. In the same copy the 23 original tests pass. Two of the three breaks are the
T4 gate's open blocking findings, still present at the same lines.

## Concrete failing cases

- NEEDS-HUMAN [impl] — **An empty part is "recognised" by bytes a different attempt wrote** (`crates/core/src/multipart/upload_part.rs:810`, `:1423`). `PartRecord::new(chunks, digest, t, epoch)` holds no attempt-specific value when `chunks` is empty. So an empty re-upload in the same millisecond and epoch as an empty prior part builds bytes identical to the prior. Probe: prior empty part 1, then the attempt's commit answered `CommitUnknownResult { may_still_commit: false }` and **not applied**. Result: `Ok(Committed { .. })`, 1 commit sent, `landed=[false]`, and **the attempt's slot is still held** (`slots_still_held=1`). That breaks "the slot is released exactly once" and 0016:1779 (held slots == live attempts). The same happens with a same-number rival whose empty part landed in the same millisecond. Fix: the commit batch deletes the slot atomically, so count the commit as landed only if `part == built bytes` **and** the slot no longer holds `self.slot`. Read the part before the slot, so a commit that lands between the two reads still shows. This is T4's blocking finding at `:810`, and it is unfixed.
- NEEDS-HUMAN [impl] — **A missing slot is read as a landed commit even when the session was torn down between two reads** (`upload_part.rs:1071-1073` reads the session first, then the slot, then the part, each as a separate `get`; `:1134-1151` concludes a pending commit landed because the slot is gone). Probe: commit held (`may_still_commit: true`, never applied). Between `look`'s session read and its slot read, a real `abort_session` runs, then an `sidx:` drain, then a terminal delete (`mpu:` + `slot:`). Result: `Ok(Committed { .. })`, `part_present=false`, and the held batch **cannot** land afterwards (`held_batch_can_land=false`). The client is told the part committed when it never existed. The brief forbids this: "the answer is `Committed` only if its own part is the one that landed". Fix: read the session **last**, or re-read it after the slot and part reads. `Open@E` bytes never come back once the session leaves that state, so a session still `Open@E` after the slot read proves the slot vanished while `Open@E`. This is T4's blocking finding at `:1139`, and it is unfixed.
- NEEDS-HUMAN [impl] — **`settle()` throws away the very commit the release branch is meant to find** (`upload_part.rs:1118` vs `:1139-1145`; the comment at `:1135-1138` names this exact case). Probe: the attempt's commit **lands** but answers `CommitUnknownResult { may_still_commit: false }`, and a rival re-upload then replaces part 1 before the re-read. `settle()` drops the `may_still_commit: false` pending commit before the slot branch runs, so the branch finds nothing and returns `SlotLost`. Result: `Err(Stopped { cause: SlotLost, residue: Left })`, with `slots_held=0` and `owned_entries=0`. The error says "no batch of this attempt is in doubt" and "its slot and staged entries are left", and both are false: its commit landed and released everything. Fix: look for the slot-releasing pending batch before `settle()` prunes the pending list, or prune only on the `Unmoved` path.

## Tests that cannot fail for the property they name

- NEEDS-HUMAN [impl] — **The (C) "same index" test passes without the same-index rule** (`crates/core/tests/multipart_upload_part.rs:1278-1311`; production `upload_part.rs:599-616`). I turned the pin off in scratch (`doubt.pinned().filter(|_| false)`), and all 23 tests still pass. "Take the lowest free index" happens to choose the same index again, because nothing below it frees up. C5 agrees: `ReserveDoubt::observe`/`settle`/`pinned` mutants at `:660`, `:671`, `:675`, `:682` all survived. So the brief's demonstrated red "a different index probed after an ambiguous reserve outcome → (C) fails" cannot have come from removing the pin. A test that does discriminate: index 0 held by attempt X; ours reserves index 1 and is held (`may_still_commit: true`); X releases index 0 before the retry. Without the pin the retry takes index 0, and the held send at index 1 can still land, leaving two slots for one attempt (0016:1707-1716, :1779).
- NEEDS-HUMAN [impl] — **Most of the unknown-outcome logic outside the commit has no test** (rubric: *Test fidelity*, *count-based assertion*). These C5 mutants survived: `Look::shows` `==`→`!=` for intent, renewal and compensation (`upload_part.rs:808`, `:809`, `:813`); `Look::witness → None` (`:802`); `Doubt::observe`/`settle`/`clear`/`drop_commits`/`holds_commit` (`:745-777`); `reopen → Ok(())` (`:1027`). That last one means the rolled-back-Complete case (0016:1607-1620, "the trap": compensate under `Open@E'`) is never run. Also surviving: `rounds += 1` → `*=` at `:620`, `:1216`, `:1335`, `:1555`, which makes the reserve, intent, renewal and compensation loops unbounded without any test noticing. The module header's claim at `:51-53` ("an intent by its owned entry's exact bytes, a renewal by its slot bytes … a compensation by its obligation") has no test behind it. (Seeded DST is `deferred: #660`, which is settled. This finding is about the in-process tests the brief asked for.)
- NEEDS-HUMAN [impl] — Minor: every test sends the body as **one** `Bytes` item (`multipart_upload_part.rs:644-645`), so splitting a body into chunks when it arrives in uneven pieces is never tested. The C5 survivor at `upload_part.rs:940` (`-`→`+`) would merge a body whose pieces don't line up with `chunk_size` into one oversize chunk, and the suite stays green. Production S3 bodies arrive in arbitrary pieces.

## On the verdict and the evidence

- `check-gates.json` row `T4-batch-review` is **gating and red**: 4 blocking findings, which reduce to 2 distinct issues at `upload_part.rs:810` and `:1139`. The code at both lines is unchanged in this patch, and probes 1 and 2 above confirm both. A reviewer verdict that calls the fix adequate would have to explain these away. I found nothing in the code that does.
- `C4-verify` is UNVERIFIABLE (exit 77), which the brief declared in advance because the feature is new and its test can't compile without it. That is not a refutation. The demonstrated reds live in `build-notes.md`, which I was not given. The pin result above shows that at least one of them cannot be what its label says.
- `C4-ci` is genuine: `typos`, `lint_docs` and `render_site --check` actually ran (`gate-logs/C4-ci.log`), with no skip warnings.

## Attempted to refute, could not

- **Drain fence (B1/B2):** B1 asserts that the first intent carries `require_absent(desired:dserver:<S>)` and gets `Conflict` (test `:1028-1037`). B2 asserts that no intent conflicts (test `:1080-1083`); with `clone()` in place of `excluding`, the re-plan loops until `RoundsSpent` and B2 fails. C5 lists no surviving mutant in `intent_batch` (`upload_part.rs:1293-1305`) or `draining` (`:1014-1024`).
- **Deadline:** read once per plan before the intent (`upload_part.rs:1192-1193`) and carried from the intent that landed (`:1283`). The test checks `t_auth + W` exactly under a delayed dispatch (test `:974-989`).
- **Drain key bytes:** unchanged. `metadata.rs:99-113` keeps the custodian's spelling, and `desired_state.rs:42` delegates to it.
- **(F) overshoot witness and the per-part ceiling:** they behave as the brief asks. The `>`→`>=` mutant at `:1410` hangs the gated test, so it does not pass silently.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P1: Empty parts do not uniquely identify an attempt.** `crates/core/src/multipart/upload_part.rs:810` accepts matching `part:` bytes as proof of this commit. An empty body stages no chunks, so two empty uploads of the same part in the same millisecond and epoch produce identical records. If the second commit returns an unapplied `CommitUnknownResult`, the prior record satisfies this check: the call falsely returns `Committed` and leaves its slot reserved. Make success attribution attempt-specific even for empty parts, and test an empty re-upload with a fixed clock and unapplied unknown outcome, checking slot release.

- NEEDS-HUMAN [impl] — **P1: A torn read can mistake teardown for a successful commit.** `crates/core/src/multipart/upload_part.rs:1139` infers a pending commit landed when its slot disappeared, although `look()` reads the session before the slot in separate operations (`crates/core/src/multipart/upload_part.rs:1071`). Hold an unapplied commit with `may_still_commit: true`; return `Open@E` from the session read; then Abort and finish the protocol's teardown before the slot read. The absent slot makes this branch return `Committed` even though no part was committed. Revalidate the session after observing the slot change before making this inference, and inject this read interleaving in a regression test.

- NEEDS-HUMAN [impl] — **P2: Renewal stops while the body accumulates a chunk.** `crates/core/src/multipart/upload_part.rs:935` polls and buffers the body without renewing; renewal is reached only through `stage_chunk()` at a complete chunk boundary or final commit. A live client can continuously supply small buffers for longer than the lease TTL and `W_open` before filling its first chunk, leaving the reserved slot expired and making the session eligible for idle abandonment. Schedule renewal while receiving/awaiting body data, including before the first chunk, and quiesce it before commit. Cover a slow multi-buffer stream with a controlled clock; the current helper supplies the entire body in one stream item.

- NEEDS-HUMAN [impl] — **P2: Renewal revives leases already observed expired.** `crates/core/src/multipart/upload_part.rs:1318` unconditionally computes a later expiry and rewrites the slot and owned entries; it never rejects `t >= existing_expiry`. Advancing the clock beyond the current leases before the next chunk therefore resurrects them and permits the part to commit. This violates the brief's refuse-rather-than-resurrect requirement and proposal 0016's explicit renewal semantics (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843`). Check the existing slot/owned lease expiries before renewal and add a controlled-clock lapse test.

- NEEDS-HUMAN [impl] — **P2: The ambiguous-reserve test cannot detect missing index pinning.** `crates/core/tests/multipart_upload_part.rs:1280` starts with no occupied slots and holds the first reserve. Index zero remains the lowest free index, so ordinary free-slot selection passes every assertion even if ambiguous-outcome pinning is removed. The frozen mutation log confirms that replacing `ReserveDoubt::pinned` with `None` survives. Start with a lower index occupied, hold a reserve on a higher index, then release the lower index before the retry; assert the retry stays pinned and releasing the held batch cannot create a second slot.

No additional reuse, simplification, or efficiency findings. Review used the target source and frozen gate evidence; no builds were run or target files changed.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared net-new verification exception — the restored suite passes 23/23, but the reverted test cannot execute, so there is no behavioral pre-fix red (`brief.md:167`, `reviewer-evidence/red.log:24`, `reviewer-evidence/restored-green.log:30`).
- [ ] C5 Causal adequacy — Repair ambiguous-commit classification — identical empty-part bytes leak a successful attempt's slot, and mixed session/slot reads acknowledge a commit that never landed (`crates/core/src/multipart/upload_part.rs:810`, `crates/core/src/multipart/upload_part.rs:1139`; findings 1–2).
- [ ] T5 Judgment — Strengthen the ambiguous-reservation regression — its permanently lowest free index makes removal of pinning invisible, leaving the mandatory double-slot failure untested (`crates/core/tests/multipart_upload_part.rs:1278`, `crates/core/src/multipart/upload_part.rs:682`; finding 3).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected staging-only slice meets #657's intended acceptance — store tests establish this API's behavior, while production S3 wiring and seeded multipart DST remain explicitly assigned to #669/#660 (`brief.md:183`, `crates/core/src/multipart/upload_part.rs:21`).
- [ ] C4-verify — The logged exit 77 is the declared compile-only pre-fix limitation; independent stash/test/pop/test reproduces compilation failure followed by 23 passing tests (`gate-logs/C4-verify.log:45`, `reviewer-evidence/red.log`, `reviewer-evidence/restored-green.log`).
- [ ] **An empty part is "recognised" by bytes a different attempt wrote** (`crates/core/src/multipart/upload_part.rs:810`, `:1423`). `PartRecord::new(chunks, digest, t, epoch)` holds no attempt-specific value when `chunks` is empty. So an empty re-upload in the same millisecond and epoch as an empty prior part builds bytes identical to the prior. Probe: prior empty part 1, then the attempt's commit answered `CommitUnknownResult { may_still_commit: false }` and **not applied**. Result: `Ok(Committed { .. })`, 1 commit sent, `landed=[false]`, and **the attempt's slot is still held** (`slots_still_held=1`). That breaks "the slot is released exactly once" and 0016:1779 (held slots == live attempts). The same happens with a same-number rival whose empty part landed in the same millisecond. Fix: the commit batch deletes the slot atomically, so count the commit as landed only if `part == built bytes` **and** the slot no longer holds `self.slot`. Read the part before the slot, so a commit that lands between the two reads still shows. This is T4's blocking finding at `:810`, and it is unfixed.
- [ ] **A missing slot is read as a landed commit even when the session was torn down between two reads** (`upload_part.rs:1071-1073` reads the session first, then the slot, then the part, each as a separate `get`; `:1134-1151` concludes a pending commit landed because the slot is gone). Probe: commit held (`may_still_commit: true`, never applied). Between `look`'s session read and its slot read, a real `abort_session` runs, then an `sidx:` drain, then a terminal delete (`mpu:` + `slot:`). Result: `Ok(Committed { .. })`, `part_present=false`, and the held batch **cannot** land afterwards (`held_batch_can_land=false`). The client is told the part committed when it never existed. The brief forbids this: "the answer is `Committed` only if its own part is the one that landed". Fix: read the session **last**, or re-read it after the slot and part reads. `Open@E` bytes never come back once the session leaves that state, so a session still `Open@E` after the slot read proves the slot vanished while `Open@E`. This is T4's blocking finding at `:1139`, and it is unfixed.
- [ ] **`settle()` throws away the very commit the release branch is meant to find** (`upload_part.rs:1118` vs `:1139-1145`; the comment at `:1135-1138` names this exact case). Probe: the attempt's commit **lands** but answers `CommitUnknownResult { may_still_commit: false }`, and a rival re-upload then replaces part 1 before the re-read. `settle()` drops the `may_still_commit: false` pending commit before the slot branch runs, so the branch finds nothing and returns `SlotLost`. Result: `Err(Stopped { cause: SlotLost, residue: Left })`, with `slots_held=0` and `owned_entries=0`. The error says "no batch of this attempt is in doubt" and "its slot and staged entries are left", and both are false: its commit landed and released everything. Fix: look for the slot-releasing pending batch before `settle()` prunes the pending list, or prune only on the `Unmoved` path.
- [ ] **The (C) "same index" test passes without the same-index rule** (`crates/core/tests/multipart_upload_part.rs:1278-1311`; production `upload_part.rs:599-616`). I turned the pin off in scratch (`doubt.pinned().filter(|_| false)`), and all 23 tests still pass. "Take the lowest free index" happens to choose the same index again, because nothing below it frees up. C5 agrees: `ReserveDoubt::observe`/`settle`/`pinned` mutants at `:660`, `:671`, `:675`, `:682` all survived. So the brief's demonstrated red "a different index probed after an ambiguous reserve outcome → (C) fails" cannot have come from removing the pin. A test that does discriminate: index 0 held by attempt X; ours reserves index 1 and is held (`may_still_commit: true`); X releases index 0 before the retry. Without the pin the retry takes index 0, and the held send at index 1 can still land, leaving two slots for one attempt (0016:1707-1716, :1779).
- [ ] **Most of the unknown-outcome logic outside the commit has no test** (rubric: *Test fidelity*, *count-based assertion*). These C5 mutants survived: `Look::shows` `==`→`!=` for intent, renewal and compensation (`upload_part.rs:808`, `:809`, `:813`); `Look::witness → None` (`:802`); `Doubt::observe`/`settle`/`clear`/`drop_commits`/`holds_commit` (`:745-777`); `reopen → Ok(())` (`:1027`). That last one means the rolled-back-Complete case (0016:1607-1620, "the trap": compensate under `Open@E'`) is never run. Also surviving: `rounds += 1` → `*=` at `:620`, `:1216`, `:1335`, `:1555`, which makes the reserve, intent, renewal and compensation loops unbounded without any test noticing. The module header's claim at `:51-53` ("an intent by its owned entry's exact bytes, a renewal by its slot bytes … a compensation by its obligation") has no test behind it. (Seeded DST is `deferred: #660`, which is settled. This finding is about the in-process tests the brief asked for.)
- [ ] Minor: every test sends the body as **one** `Bytes` item (`multipart_upload_part.rs:644-645`), so splitting a body into chunks when it arrives in uneven pieces is never tested. The C5 survivor at `upload_part.rs:940` (`-`→`+`) would merge a body whose pieces don't line up with `chunk_size` into one oversize chunk, and the suite stays green. Production S3 bodies arrive in arbitrary pieces.
- [ ] **P1: Empty parts do not uniquely identify an attempt.** `crates/core/src/multipart/upload_part.rs:810` accepts matching `part:` bytes as proof of this commit. An empty body stages no chunks, so two empty uploads of the same part in the same millisecond and epoch produce identical records. If the second commit returns an unapplied `CommitUnknownResult`, the prior record satisfies this check: the call falsely returns `Committed` and leaves its slot reserved. Make success attribution attempt-specific even for empty parts, and test an empty re-upload with a fixed clock and unapplied unknown outcome, checking slot release.
- [ ] **P1: A torn read can mistake teardown for a successful commit.** `crates/core/src/multipart/upload_part.rs:1139` infers a pending commit landed when its slot disappeared, although `look()` reads the session before the slot in separate operations (`crates/core/src/multipart/upload_part.rs:1071`). Hold an unapplied commit with `may_still_commit: true`; return `Open@E` from the session read; then Abort and finish the protocol's teardown before the slot read. The absent slot makes this branch return `Committed` even though no part was committed. Revalidate the session after observing the slot change before making this inference, and inject this read interleaving in a regression test.
- [ ] **P2: Renewal stops while the body accumulates a chunk.** `crates/core/src/multipart/upload_part.rs:935` polls and buffers the body without renewing; renewal is reached only through `stage_chunk()` at a complete chunk boundary or final commit. A live client can continuously supply small buffers for longer than the lease TTL and `W_open` before filling its first chunk, leaving the reserved slot expired and making the session eligible for idle abandonment. Schedule renewal while receiving/awaiting body data, including before the first chunk, and quiesce it before commit. Cover a slow multi-buffer stream with a controlled clock; the current helper supplies the entire body in one stream item.
- [ ] **P2: Renewal revives leases already observed expired.** `crates/core/src/multipart/upload_part.rs:1318` unconditionally computes a later expiry and rewrites the slot and owned entries; it never rejects `t >= existing_expiry`. Advancing the clock beyond the current leases before the next chunk therefore resurrects them and permits the part to commit. This violates the brief's refuse-rather-than-resurrect requirement and proposal 0016's explicit renewal semantics (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843`). Check the existing slot/owned lease expiries before renewal and add a controlled-clock lapse test.
- [ ] **P2: The ambiguous-reserve test cannot detect missing index pinning.** `crates/core/tests/multipart_upload_part.rs:1280` starts with no occupied slots and holds the first reserve. Index zero remains the lowest free index, so ordinary free-slot selection passes every assertion even if ambiguous-outcome pinning is removed. The frozen mutation log confirms that replacing `ReserveDoubt::pinned` with `None` survives. Start with a lower index occupied, hold a reserve on a higher index, then release the lower index before the retry; assert the retry stays pinned and releasing the held batch cannot create a second slot.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_657/review-b
- [ ] **An unknown commit can report success for somebody else's part.** `brief.md:240-241` says “a part that is there is `Committed`.” On a re-upload, the previous part already exists; an unknown outcome that did not land would therefore report success for the replacement while retaining its slot and staging entries. A same-number rival creates the same ambiguity. The normative classification requires “its own commit present” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1630-1651`), and `PartRecord` has chunks but no attempt ID (`crates/core/src/multipart.rs:2930-2935`). Revise the design to establish identity from the attempted record/chunks, preserve unsettled `may_still_commit` outcomes, and extend E beyond its landed-success case to prior/rival records and delayed landing. A mere re-read does not settle absence when `may_still_commit` is true (`crates/traits/src/lib.rs:240-247`).
- [ ] **The ceiling criterion promises more than the planned commit enforces.** F (`brief.md:75-77`) requires refusal whenever a commit would take the cumulative total past `MAX_STAGED_CHUNKS`, but `brief.md:226-231` reads summaries before a batch that does not fence that total. Two different parts can both read one chunk of headroom and both commit. The target explicitly accepts this race and bounds the total by `MAX_STAGED_CHUNKS + MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1443-1450`; `crates/core/src/multipart.rs:5040-5042`). Revise F to distinguish the read-time refusal from the permitted concurrent overshoot, and specify a controlled two-commit witness. Otherwise Do must either violate the stated criterion or add serialization that the proposal deliberately excludes.
- [ ] **The deadline discriminator can pass without establishing the required bound.** A only checks `Some` rather than `None` (`brief.md:39-41`), and the required mutant changes only that option (`brief.md:142`). Passing `Some(u64::MAX)` satisfies both while violating the authorization-plus-`W_write` rule in `brief.md:212-218`. The target fixes `W_WRITE_MILLIS` at 30,000 and makes the writer responsible for carrying that bound (`crates/custodian/src/gc.rs:187-202`). Revise A to use a controlled authorization clock and assert the actual deadline against the supplied write window, including delayed fragment dispatch; require a wrong-but-`Some` deadline mutant to fail. This tests the in-scope writer without expanding into D-server implementation.
- [ ] **The tracker’s full acceptance command was dropped.** `notes.json:1` explicitly requires “`cargo xtask ci` green.” The brief names only `cargo test -p wyrd-core` (`brief.md:79`) and mutation demonstrations (`brief.md:130-142`), despite also changing the custodian and architecture documentation (`brief.md:149-164`, `:247-252`). Add the full command to the success/verification requirements; core tests alone cannot establish the tracker’s acceptance. The target independently requires it for Rust-affecting changes (`AGENTS.md:41-45`).
- [ ] size backstop — this slice is behaving oversized: patch is 196 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Oversized slice (patch 196 KB, threshold 100 KB; ~3.7k added lines). Split in Plan (`pdca split 657`), e.g. (a) slot reserve + owned sidx: intent with drain fence + renewal, (b) part commit + ambiguous-outcome classification + compensation + re-upload + ceilings. The rebuild of whichever slice owns them must fix the reproduced defects: - Empty-part identity (upload_part.rs:810, :1423): identical empty-part bytes are taken as this attempt's commit -> false Committed and a leaked slot. Count a commit as landed only with attempt-specific evidence (part bytes AND own slot gone; read part before slot). - Torn read (upload_part.rs:1071-1073, :1134-1151): session read before slot/part lets an Abort+teardown in between turn an absent slot into a false Committed. Re-read/read session last before inferring a landed commit; add the interleaving as a regression. - settle() (:1118 vs :1139-1145) prunes the may_still_commit:false pending commit before the slot-release branch -> landed commit reported as SlotLost. - Renewal: none while buffering below a chunk boundary (:935); renewal resurrects already-expired leases (:1318) — must refuse. Tests: the (C) same-index test must discriminate pinning (occupy index 0, hold reserve at 1, release 0 before retry; assert pinned and one slot). Cover unknown-outcome paths for intent/renewal/compensation, reopen (rolled-back Complete), bounded rounds, and a multi-piece body stream (C5 left 31 survivors there).
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
