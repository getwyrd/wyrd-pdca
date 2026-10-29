# Build notes — #932 (657.1a): UploadPart staging, safe half — iteration 2

Withheld from the reviewer. For the human at sign-off.

## Base and starting point

Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at `582da99`
(`pdca-integration/main-r42c5348f203d`, the stack base the driver chose: `getwyrd/wyrd` `main`
with #653, #744 and #853 folded). I started from the previous attempt's patch
(`iteration-v1/patch.diff`), which applied cleanly, because the sign-off said to keep its design
and slice boundary and fix five findings. Everything below is the delta from that attempt. Line
numbers are in the **patched** tree. `stage.rs` is `crates/core/src/multipart/stage.rs`; "the test"
is `crates/core/tests/multipart_upload_part_stage.rs`.

## The five carry-forward findings, and what I did

### 1. Store reads (and commits) that fail while a batch is in doubt — test gap

No production change was needed for the mapping itself: every read after a doubtful commit already
ended through `Doubt::end`. What was missing was any test that made such a read fail, so two hand
mutants survived (a fault answered as a plain stop; `held.or(unsettled)` swapped).

* Wrapper: `Then::Fault` (test `:684`) answers a chosen `get`/`scan` with a `TransientFault`
  (`read_fault`, `:293`) and logs `Event::ReadFault`, so each leg can assert the fault really
  fired. `Act::Fail` (`:795`) answers a commit with a definite (non-unknown) fault.
* `c_a_reserve_read_that_fails_while_a_batch_is_in_doubt_keeps_the_doubt` (`:3112`), four
  schedules: held then out-of-flight unknown then the slot read fails (both doubts live, the held
  one must win); the session read after the held round; the pinned index's second read after a
  session move; the retry's commit fails.
* `e_an_intent_read_that_fails_while_a_batch_is_in_doubt_keeps_the_doubt` (`:3838`), six
  schedules. The first is the carry-forward's exact one: intent `Hold`, then
  `Unknown { applied: false, may_still_commit: false }`, then `get(sidx:)` faults →
  `InDoubt { may_still_commit: true }`. Then: the held round's slot read; its session read; the
  next round's drain read; the entry's second read after a session move; the retry's commit.
* Every schedule asserts `classify == Indeterminate` (though the read fault itself is transient),
  `may_still_commit` true on both the error and the kept report, and then **releases the held
  batch**: it lands (or, after a session move, conflicts). So the doubt the error reports was
  real, not a formality.
* `bounded`'s post-timer `now_or_never`: `a_prime_an_ok_ready_as_the_deadline_timer_fires_is_counted_written`
  (`:2024`). This needed two harness pieces: `HoldAt::Answer` (fake `:1052`, the store has
  answered `Ok` but the reply is in transit) and `Runtime::on_fire` (`:1060`, a hook the test's
  timer runs inside its own firing). The timer fires, the hook releases the reply in that same
  poll, and `bounded` sees the timer branch with the answer ready behind it. No poll of staging
  can fall between the two, so the leg is deterministic.

### 2. The class of an unknown-effect stop depended on timing

* `Stop::WriteEffectUnknown.abandoned` is now `Vec<WriteDeadlineExpired>` (`stage.rs:525`), one
  per abandoned write, built by `abandoned_write` (`:720`): the fragment id,
  `WriteEffect::Unknown`, and a `detail` that starts "the writer, not the store", so nobody reads
  it as a store verdict. `write_fragments` (`:1454`) now tags each put with its fragment index, so
  it knows which writes it abandoned.
* `source()` (`:674`) returns `most_indeterminate(reports, abandoned)` (`:694`):
  `Indeterminate`, then `Integrity`, then `Terminal`, then `Transient`; ties go to the earliest,
  store reports first. `Display` (`:639`) names the store's most indeterminate report.
* Legs: the straddle leg (`:1859`) now asserts `Indeterminate` in **both** runs (run 2 was
  `Terminal`); the abandoned leg (`:1658`) asserts it too; the new
  `a_prime_a_write_stop_is_classed_by_its_most_indeterminate_answer` (`:1949`) has a transient
  fault answer first and then (run 1) a store `Unknown` → `Indeterminate`, or (run 2) a plain
  fault → `Terminal`.

### 3. Lease docs claimed slot values never repeat

Module doc "# Leases" (`stage.rs:70-85`) and `next_expiry` (`:1243-1246`) now say: every lease an
intent writes is later than the slot lease its batch replaces, so the value differs from every
slot value that landed before it; retry rounds of one intent may repeat a value, reserve rounds
repeat on purpose, and only the owned entry's bytes tell rounds apart. No code change: the code
already settled by the entry's bytes.

### 4. `reserve_slot` skipped the request checks

One validator, `StageRequest::check` (`stage.rs:216`), called first by `reserve_slot`
(`:964-966`) and `stage_part` (`:1199`). It adds `max_inflight_parts == 0` and
`max_inflight_parts > MAX_SLOT_INDEX + 1`. The (H) leg (`:4134`) runs every invalid request
through both entry points and checks the valid edges (TTL 2; a key space exactly the grammar's
width).

### 5. A late body answer could slip past the idle deadline

`Late` (`stage.rs:843`) and a fourth parameter to `bounded` (`:860`). Body reads pass
`Late::Refused` (`:1374`): an answer seen with the clock at or past the deadline, on either branch
of the race, is dropped and staging stops with `BodyIdle` (`:881`). Fragment writes pass
`Late::Taken` (`:1490`): an `Ok` counts whenever it is seen, which keeps the rule the sign-off
asked to keep. New leg `a_double_prime_a_body_answer_seen_past_its_idle_deadline_is_late`
(`:2144`), two runs: the body ends late, or sends a late piece and then ends. In both, the clock
reaches the bound first, the lease is still live (`const _: () = assert!(BOUND < TTL)`, `:103`),
and the 10 buffered bytes must never be staged.

### Docs

`docs/design/architecture/05-building-block-view.md:246` (the paragraph the previous attempt
added): two clauses — the stop is classed indeterminate for either kind of unverified write, and a
body piece or end seen only after the bound counts as none. Nothing about renewal, no landing-bound
claim.

## Judgment calls the human should see

* **What the abandoned-write account is.** I used the seam's own `WriteDeadlineExpired` with
  `WriteEffect::Unknown`. Rejected: `CommitUnknownResult` (a metadata-commit type; wrong domain),
  and a staging-local error type (`classify` cannot see it and falls to `Terminal`, the exact bug;
  teaching `classify` a new type is a `wyrd-traits` change, outside this slice). The cost of my
  choice: a consumer that walks the chain with `write_deadline_outcome` finds an `Unknown` verdict
  staging wrote, not the store. Its fact is still true (the write's effect is unknown), and its
  `detail` says whose account it is; the test asserts that text. The previous attempt declined to
  build one; the carry-forward asked for an `Indeterminate` source, so this is the human's call
  made concrete.
* **The rank among non-indeterminate classes.** The finding only asked for "most indeterminate
  first". I also fixed the rest of the order so the class never depends on answer order:
  `Transient` last (a stop reads "retry may help" only if every account says so, the fail-safe
  direction `classify`'s own doc argues for), `Integrity` before `Terminal` (the more specific
  terminal class). The `Transient`/`Terminal` order is bound by run 2 of the new leg. The
  `Integrity`/`Terminal` order is **not** bound by any test; both are terminal classes, and a put
  that answers an `IntegrityFault` is a case the fake has no reason to model.
* **`max_inflight_parts` wider than the grammar is refused.** Not in the carry-forward. Before,
  `reserve` skipped indices the `slot:` grammar cannot address (`SlotIndex::new(k).ok()`), so a
  wider cap silently behaved as a narrower one. One condition; both edges tested.
* **A late body answer is refused even if the bytes arrived in time.** Staging cannot tell "the
  client was late" from "the client was on time but staging was polled late"; it fails closed,
  as the finding asked. The cost is a rare spurious `BodyIdle` under heavy scheduler delay.

Still standing from the previous attempt (unchanged): the idle bound is measured from the instant
a read began; the yield after each empty piece; a zero write window is an invalid request; the
`zip(placement)` cannot truncate; the fake runs each put in its own task. See
`iteration-v1/build-notes.md` for the reasoning.

## ⚑ For the human (from the brief's Design, unchanged)

This slice does **not** establish an absolute landing bound. 0016 sizes the orphan grace on "no
accepted write can be applied more than `W_write` after it was authorized"
(`G_orphan > W_write + δ_clock`, `0016:1553-1578`). A publishing step no store bounds can straddle
the deadline by any amount, which is exactly `WriteEffect::Unknown`
(`crates/traits/src/lib.rs:871-892`, `:1128-1141`). What this slice guarantees: no publication
*begins* after the deadline, an unknown or abandoned write is never counted either way (and is now
classed `Indeterminate` either way), and every position it could land on is named by a durable
owned entry first. The module doc (`stage.rs:46-57`) says that and claims no bound. Decide whether
the gap gets an issue (the backends under #638, or the `G_orphan` owner #625) before the multipart
verbs ship.

## Test runner

Quick red/green loops used `cargo test -p wyrd-core --test multipart_upload_part_stage` under
`timeout` (the brief: "RED is reachable in-process on the plain `cargo test -p wyrd-core`
target"; `cargo xtask` has no single-test subcommand). Every leg also has its own 30 s watchdog.
The whole gate is `./engine/xtask.sh ci`, run once at the end (result below). The test needs no
display and no service: an in-memory redb store, an in-process fake, tokio.

## Verification posture

Net-new (posture (a)): `stage.rs` does not exist on the base, so the test cannot compile there and
C4-verify should report UNVERIFIABLE (exit 77), as the brief pre-declares. The reds below are the
red half.

## Demonstrated reds

Driven by a script (scratch, not shipped) that, per mutant, applies line-anchored edits to a clean
copy (asserting each edited text appears exactly once on its line), runs the named legs with
`--exact`, restores both files from memory and, at the end, checks their SHA-256 against the clean
copies (matched on every run). **49 mutants, 49 red, 0 compile errors.** One mutant first came out
green and was wrong, not the test: see N4.

Brief-required (`stage.rs` unless noted):

| # | Edit | Leg | Failure |
|---|---|---|---|
| M1 | per-server `require_absent(desired:dserver:<S>)` removed (`for server in named.filter(false)`) | `b1_…_makes_it_replan` | "the drain fence failed the intent", `left: Committed` |
| M2 | plan against the full topology | `b2_…_is_planned_around` | `IntentRoundsSpent { chunk: 1, rounds: 4 }` |
| M3 | same-index pin removed | `c_an_ambiguous_reserve_retries_the_same_index` | `Reserved { index: 0 }`, not 1 |
| M4 | after the ambiguous rounds, probe a fresh index | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | `Reserved { index: 2 }` instead of `SlowDown` |
| M5 | `stage_part`'s full-range refusal answered `NoSuchUpload` | `c_stage_part_with_every_slot_held_answers_slow_down` | got `NoSuchUpload` |
| M6 | session left `Open`, plain path → `SlowDown` | `c_a_session_leaving_open_under_the_reserve_is_no_such_upload` | got `SlowDown` |
| M7 | session left `Open`, pinned path → `SlowDown` | `c_the_pinned_reserve_answers_no_such_upload_once_the_session_left_open` | got `SlowDown` (lands run) |
| M8 | pinned index cleared on a newer epoch without settling | `c_a_held_reserve_landing_before_the_session_moves_keeps_its_one_slot` | two slots of one attempt |
| M9 | lapse check skipped on an intent retry | `d_an_intent_retry_refuses_a_lease_that_lapsed_between_rounds` | staged: the retry revived the slot |
| M10 | lapse check at a reading taken before the drain read | both `d_…_lapsed_during_its_drain_read` legs | both staged |
| M11 | absent read after an unknown intent taken as "did not land" | `e_ii_…` and `e_i_…` | `Stopped` not `InDoubt`; `ChunkIdTaken` |
| M12 | slot-read path answers "slot lost" without re-reading the entry | `e_a_held_intent_landing_between_its_entry_and_slot_reads_is_adopted` | `SlotLost` |
| M13 | `rounds += 1` → `*= 1` | `f_the_intent_loop_is_bounded` | runaway guard, 401 commits |
| M14 | `probes += 1` → `*= 1` | `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving` | runaway guard, 401 commits |
| M15 | `at_pin += 1` → `*= 1` | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | the 4th round reserved |
| M16 | fragment put with `None` | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `left: None` |
| M17 | fragment put with `Some(u64::MAX)` | same | `left: Some(18446744073709551615)` |
| M18 | deadline taken at dispatch | same | `left: Some(1003777)` (= `t_auth + 777 + W`) |
| M19 | the write wait not bounded (`every.await`) | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | "timed out waiting for staging to stop at the deadline" |
| M20 | the fake's publication-point check removed (test file) | `a_prime_a_held_put_released_after_expiry_is_refused_and_not_published` | "nothing of it was published", `left: Some(0)` |
| M21 | a `WriteEffect::Unknown` answer counted as written | `a_prime_a_publication_straddling_the_deadline_is_never_counted` | run 1 got `Staged` |
| M22 | an abandoned put reported as nothing written | same | run 2: "none was sent" |
| M23 | the intent's `require(mpu == Open@E)` removed, nothing else | `e_prime_1_…` and `e_prime_2_…` | the post-Abort intent `Committed` |
| M24 | an empty piece ends the read (re-arms the deadline) | `a_double_prime_empty_pieces_do_not_count_as_progress` | stopped at `T0 + 10000`, not `T0 + 5000` |
| M25 | the body read awaited without the idle bound | `a_double_prime_a_stalled_body_read_stops_at_its_idle_bound` | no wait ever armed (10 s sync point) |
| M26 | a piece spanning a boundary merged into one chunk | `a_stages_uneven_pieces_…` | 2 chunks, not 4 |
| M27 | last `ChunkRef.len` = `chunk_size` | same | read-back differs |

This iteration's findings:

| # | Edit | Leg | Failure |
|---|---|---|---|
| N1 | (1a) the intent's entry-read fault answered as a plain stop | `e_an_intent_read_…` | `Stopped(Store(TransientFault …))`, not `InDoubt` |
| N2 | (1b) `held.or(unsettled)` swapped | `c_a_reserve_read_…` and `e_an_intent_read_…` | "the held batch's doubt" (`may_still_commit` false) |
| N3a–d | reserve: commit fault, slot read, session read, pinned re-read → plain stop (4 edits) | `c_a_reserve_read_…` | each `Stopped`, not `InDoubt` |
| N3e–i | intent: commit fault, slot read, re-read after a move, drain read, reconcile's session read → plain stop (5 edits) | `e_an_intent_read_…` | each `Stopped`, not `InDoubt` |
| N4 | (1) `bounded` never polls the work after its timer fires (the post-timer `now_or_never` deleted) | `a_prime_an_ok_ready_as_the_deadline_timer_fires_is_counted_written` | `WriteEffectUnknown { abandoned: [index 0] }`, not `Staged` |
| N5a | (2) abandoned writes left out of the class | the abandoned leg and the straddle leg | `Terminal`, not `Indeterminate` (run 2) |
| N5b | (2) first report decides the class | `a_prime_a_write_stop_is_classed_by_its_most_indeterminate_answer` | `Transient`, not `Indeterminate` |
| N5c | (2) `Transient` ranked above `Terminal` | same | run 2: `Transient`, not `Terminal` |
| N6a | (4) `reserve_slot` skips the shared check | `h_an_invalid_request_writes_nothing` | `reserve_slot` reserved |
| N6b | (4) `max_inflight_parts == 0` not refused | same | `Refused(SlowDown { max_inflight_parts: 0 })` |
| N6c / N6d | width bound one too loose / one too tight | same | staged the too-wide request / refused the valid edge |
| N7 | (5) a body answer seen past the deadline taken | `a_double_prime_a_body_answer_seen_past_its_idle_deadline_is_late` | `Staged`: the buffered bytes were written |

**N4, first try.** I first wrote N4 as "filter the post-timer `now_or_never` result to `None`". It
stayed green, and the mutant was the problem: the call still ran, and for fragment writes the
answers are pushed into a vector outside the raced future, so the `Ok` was recorded anyway and
`write_fragments` saw every write answered. "Deleting" that code means not polling the work at all
once the timer fires; written that way (`if false { now_or_never } else { None }`, keeping the
import used) it goes red as shown. For body reads the deletion is equivalent after finding 5's
fix: a late answer is refused either way, and an early wake loops and polls the work first anyway.

## Refuting my own test

* **(a) Genuine red?** Yes. Each edit above was applied for real and its named leg failed with the
  message shown (49 of 49). With `stage.rs` reverted entirely the file cannot compile, which is
  the net-new posture.
* **(b) Production path?** Yes. Every leg calls the production `stage_part` / `reserve_slot` over
  a real `RedbMetadataStore::in_memory()`, real Create (#894) for every session and real Abort
  (#895) for the fences. The wrapper only reorders, fails or answers store calls, and the fake is
  a test double for the chunk-store seam, not for code under test. Every red edit is in the
  production file except M20, which tests the fake itself, as the brief asks.
* **(c) Fixture includes the fault?** Yes. The read faults are injected on the real read the code
  makes (each leg asserts `Event::ReadFault` fired) and the held batch is the real batch staging
  sent, released afterwards to show it lands. The late body answer comes after the real clock has
  passed the real bound. The in-transit `Ok` is a real store `Ok` (the fake stored the fragment
  and logged `Acked` before the hold). The previous attempt's fixtures (B1's drain on a server the
  plan really chose, the real Abort, the held put inside its publishing step) are unchanged.

## Flake check

Whole file: 20 consecutive runs, 5 runs with `--test-threads=1`, 3 copies run concurrently, then
15 more runs after the last test change. 0 failures.

## C5 (advisory): `cargo mutants --in-diff`

`cargo mutants --in-place --in-diff <this patch> -p wyrd-core --cap-lints true --timeout 120 --
--test multipart_upload_part_stage --lib` (cargo-mutants 27.1.0): **144 mutants, 111 caught,
33 unviable, 0 missed, 0 timeouts** (13 min). The tree was byte-identical afterwards (diffed).
The 33 unviable are all `Default::default()` (or a leaked `Default`) for types with no `Default`
impl: `StageOutcome`, `Refusal`, `ReserveOutcome`, `Reserve`, `Session`, `Found`, `Intent`,
`Halt`, `StagedAttempt`, `StageError`, `SlotRecord`, `SlotIndex`, `PartNumber`, `UploadId`,
`AttemptId`, `OwnedEntry`, `ChunkRef`-slice element, `Digest`, `WriteDeadlineExpired`, `Option<&dyn
Error>` targets, and `bounded`'s generic output. Each is unrepresentable, not a missed behaviour.
Every generated mutant in this iteration's new code was caught: all 13 in `StageRequest::check`,
`most_indeterminate -> None`, `source -> None`, `bounded -> None` and its `>=`/`<` swaps (`:869`,
`:881`), and every operator swap in `write_fragments`.

**Survivors: none.** cargo-mutants does not generate swaps inside closures or match-arm values
(`held.or`, the rank table, the `doubt.end` call sites), which is why the hand mutants N1–N5c
exist. The one hand-identified gap left is the `Integrity`/`Terminal` rank order (see Judgment
calls); I did not add a test for it.

The custodian side is unchanged since the previous attempt (a re-export and two doc notes); its
C5 result there was 1 caught, 1 unviable.

## Gates

`./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree), run once on the final tree:
**exit 0, "xtask ci: all checks passed"**. Every step ran, none skipped: `typos`,
`lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK), gitlink-guard,
unsafe-guard, blackbox-guard, `cargo fmt --all -- --check`, `cargo clippy --workspace --exclude
wyrd-dst --all-targets`, build, `cargo test --workspace --exclude wyrd-dst` (this file: 59
passed), `cargo-machete`, `cargo deny check` (advisories, bans, licenses, sources ok; one
pre-existing `license-not-encountered` warning for `ISC` in `deny.toml:111`, not this patch),
`cargo deny --all-features --config deny-all-features.toml check advisories` (ok), the
all-features licenses/bans/sources check (ok), conformance vectors, statics (ADR-0035),
deploy-guard (ADR-0010), and the DST tier (`clippy` and `test -p wyrd-dst` under
`--cfg madsim`).

Commit-readiness: `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core -p
wyrd-custodian --all-targets` clean with the workspace's `warnings = "deny"` / `clippy::all =
"deny"`. The target repo configures no git hooks (only samples); its formatter and linters are
the `cargo fmt` / `clippy` steps of `xtask ci`. The publish commit still needs its DCO
`Signed-off-by` (the publish step's job). `patch.diff` is `git diff` of the final tree against
`582da99`, new files included via intent-to-add.

## External dependencies

`typos` and the docs renderer (`docs-renderer`) both run inside `cargo xtask ci` (see Gates).
