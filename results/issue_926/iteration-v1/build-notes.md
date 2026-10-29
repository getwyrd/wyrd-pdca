# Build notes — issue 926 / multipart-upload-part-stage (657.1, first slice of #657)

## Base and target

Target `getwyrd/wyrd` `main`. Built in the per-cycle worktree on its base `61e5341`
(`pdca-integrate: issue_744`; stack base `pdca-integration/main-r42c5348f203d`), which is the
brief's verified `main` `12dd082` plus #929 and the integrated bundles 653 and 744. None of the
files this slice edits differ between `12dd082` and the base, except
`docs/design/architecture/05-building-block-view.md`, where the base changed only line 297
(`validate`, issue 744); this slice edits line 244. `patch.diff` is the worktree against
`61e5341`, the two new files included. All `path:line` below are on the patched tree.

## What the change is

| Change | Where |
|---|---|
| New module: the reserve, the per-chunk owned intent, staged placement minus draining servers with re-plan, fragment writes with their deadline, renewal raced against pending I/O, the chunk bound, the hand-off | `crates/core/src/multipart/stage.rs` (new; `reserve_slot` :705, `reserve` :725, `stage_part` :877, `Footprint::renew` :980, `Stager::renewing` :1210, `Stager::stage_chunk` :1239, `Stager::intent` :1282) |
| `pub mod stage;` | `crates/core/src/multipart.rs:155` |
| Slot record writer-side constructors, crate-private, refusing what decode refuses | `crates/core/src/multipart.rs:2762` (`SlotRecord::reserve`), `:2780` (`with_lease`); doc `:2741-2746` |
| Streaming SHA-256 (`DigestHasher`); `Digest::sha256` now goes through it, so one place names the algorithm | `crates/core/src/multipart.rs:1092-1110`, `:1053-1057` |
| Module header: `stage` listed as a child; `desired:dserver:` now pointed at core; who writes `slot:`/`sidx:` | `crates/core/src/multipart.rs:9-12`, `:71`, `:107-109` |
| One home in core for the drain-record key | `crates/core/src/metadata.rs:98` (`DESIRED_DSERVER_PREFIX`), `:102` (`desired_dserver_key`), `:108` (`parse_desired_dserver_key`) |
| Custodian uses core's key (same bytes); `desired_key` kept as a re-export so `reconstruction/staged.rs:104` and the custodian tests are unchanged | `crates/custodian/src/desired_state.rs:28`, `:39`, `:166` |
| The two `deferred: #657` notes settled (the writer now fences); the seeded interleaving stays `deferred: #665` | `crates/custodian/src/desired_state.rs:217-221`, `:236-239` |
| `encode_chunk` made `pub(crate)` so staging encodes exactly as the write path does | `crates/core/src/write.rs:137-140` |
| Docs currency: the multipart records paragraph | `docs/design/architecture/05-building-block-view.md:244` |
| The test file the brief names | `crates/core/tests/multipart_upload_part_stage.rs` (new) |

No production caller is added (brief Production reach (c)). `crates/core/src/lib.rs`,
`08-crosscutting-concepts.md`, proposal 0016, every ADR, `Cargo.lock` and `deny*.toml` are
untouched.

## Decisions, and where the code goes past or narrower than the brief's wording

1. **A held reserve never probes elsewhere** (`stage.rs:844`). 0016 says the reserver "probes
   onward only once it reads that index carrying *another* attempt's id (its own reserve
   definitively lost)" (`0016:1714-1715`). It is not definitively lost: the held batch requires
   `slot:<k>` absent, and the other attempt releases `k` when it commits or compensates, after
   which the held batch can still land. Probing onward would then leave this attempt two slots,
   which the brief's invariant forbids ("No attempt ever holds more than one slot"). So once a
   batch is held at `k`, the reserve retries only `k`, and answers `SlowDown` if another attempt
   holds `k` or after `RESERVE_AMBIGUOUS_ROUNDS` commits there. Cost: a reserve can refuse where
   0016's text would have probed on; the client retries. Leg
   `c_a_held_index_taken_by_another_refuses`.
2. **Reserve probe exhaustion is `SlowDown`, not an error** (`stage.rs:753-770`). A conflict no
   read explains (TiKV's lock case) marks that index lost for this reserve and probes the next,
   so the probes are each against a different key and number at most `max_inflight_parts`. A
   separate counter bounds the case the lost-set cannot: a session that moves to a later `Open`
   epoch under every commit (leg `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving`).
3. **A lease never moves backwards** (`stage.rs:951-955`): every batch stamps
   `max(now + TTL, current slot lease + 1)`. This is what makes "settle a renewal by re-reading
   its own slot's bytes" exact (every batch puts a slot value no earlier batch put), and it keeps
   a clock that steps back from shortening a live lease. Cost: a lease 1 ms past `now + TTL` when
   the clock has not moved since the last write. Leg `d_the_quiesce_renewal_never_moves_a_lease_backwards`.
4. **The intent also `require_absent`s its own `sidx:` key** (`stage.rs:1337`), beyond the
   brief's batch listing. A fresh chunk id should never meet an entry, but if it does the intent
   must not overwrite a foreign record (`Stop::ChunkIdTaken`, leg
   `e_a_chunk_id_already_taken_is_a_typed_error`); and it keeps a held intent and its retry
   mutually exclusive even when their slot puts happen to be equal.
5. **Placement is checked before the reserve** (`stage.rs:902-914`): too few non-draining
   domains for the scheme is `Stop::Placement` with nothing written, so a misconfigured fleet
   leaves no slot residue. The per-chunk plan still refuses with the same typed error when drains
   land after the reserve (leg `b3_drains_after_the_reserve_leave_a_typed_placement_error`).
6. **The wait for fragment writes is bounded at their deadline** (`stage.rs:1210-1237`). 0016
   requires "every fragment write to a D server is a bounded, fail-closed await"
   (`0016:1555-1557`) and the rubric's *Await discipline* says the same; relying only on the D
   server's refusal bounds when a write can *land*, not how long staging waits. Writes still
   pending at `t_auth + W` are abandoned (no store may apply them after it) and staging stops
   with `Stop::WriteDeadlineElapsed`. Leg `a_prime_a_write_pending_past_its_deadline_is_abandoned`.
   The body read is not bounded here: renewal keeps a slow body alive by design, so the gateway's
   request timeout (#669) ends a stalled body; the doc at `stage.rs:866-873` says so.
7. **The runtime seam** (`stage.rs:160-171`): `StageRuntime { now_millis, next_chunk_id,
   wait(millis) }`, `wait` in the shape of `CreateRuntime::wait` (`create.rs:81-82`). Every
   clock read in the module goes through `now_millis`; the module doc names it as the one clock
   of the staging lease lifecycle (rubric: one clock per lifecycle).
8. **A session moved to a later `Open` epoch** (a Complete fence and its release) is adopted by
   the reserve, the intent and the renewal, which then fence on the new bytes; any doubtful batch
   required the old bytes and can no longer land. A session no longer `Open` stops staging as
   `Refused(NoSuchUpload)` with the residue left for the reaper, as the brief's Design says.
9. **The chunk bound stops reading** (`stage.rs:1157-1160`): with `max_part_chunks` chunks
   staged, any further byte sets `chunk_bound_exceeded` and staging returns the attempt intact;
   its `len` and `digest` then cover the staged chunks only (documented on the getters). An empty
   body stages no chunk.
10. **Renewal is checked before each intent, before each body read and fragment write, and on
    every timer wake** (`stage.rs:1189-1237`, `:1251`). A lapsed lease is always due (every lease
    is at least half a TTL past `renew_at`), so the intent can never be the batch that revives a
    lapsed slot lease.

## Alternatives ruled out

- **Re-reading every owned entry after an unexplained renewal conflict**, to name a lost entry:
  about 15 lines and one more leg. Without it the renewal ends `RenewalRoundsSpent` after its
  bounded rounds, still a typed error; the attribution can come with the reaper work.
- **Renewing through `metadata::renew_pending`**: it refuses an owned entry by the `pending:`
  namespace rule (`crates/core/src/metadata.rs:2588`), and it does not rewrite the slot in the same batch, which
  0016 requires (`0016:349`, `:659`).
- **A spawned renewal task**: core spawns nothing (ADR-0009); racing the caller's wait against
  the pending future gives the same effect, and the pending read or write is resumed rather than
  dropped (legs D-i, D-ii).

## Refuting my own test

- **(a) Genuine red? Yes, with one honest limit.** The staging functions do not exist on the base,
  so with the whole fix reverted the test file does not compile: C4-verify will report
  UNVERIFIABLE (exit 77), as the brief's Verification posture pre-declares. The binding evidence
  is therefore the demonstrated reds below: each named production edit, applied alone to the
  final `stage.rs`, turns its named leg red, and the file is restored after each (checked with
  `cmp` against a saved copy).
- **(b) Production path? Yes.** Every leg calls the production `stage_part` / `reserve_slot` /
  `StagedAttempt::renew` in `crates/core/src/multipart/stage.rs` over a real
  `RedbMetadataStore::in_memory()`; every session is made by the production `create_session`,
  every fence by the production `abort_session`, and chunk bytes are read back by the production
  `read::read_chunk_verified`. The doubles are only at the seams the brief names: a store
  wrapper that logs batches and scripts outcomes (it delegates every applied batch to redb), a
  placement-aware chunk store in the shape of `domain_placement.rs`'s `Fleet`, and a runtime
  with a manual clock and a hand-fired timer.
- **(c) Fixture includes the fault? Yes.** The drain record is really written to the store
  before the intent commits (B1) or before staging (B2, B3); the `CommitUnknownResult` outcomes
  are really returned, with the batch really applied or really held and later really committed;
  X's release of index 0 is a real `require(slot == X's bytes) + delete` commit between the held
  batch and the retry; the withheld body piece and the held fragment put are really pending
  (the renewal is asserted to commit before the test supplies them); the lapsed leases are
  lapsed by moving the same clock the code reads. The wrapper also checks, on every slot or owned
  value staging writes, that its decoder accepts it back byte for byte.

## Demonstrated reds

Each edit was applied alone to the final `crates/core/src/multipart/stage.rs`, the whole test file
was run with `cargo test -p wyrd-core --test multipart_upload_part_stage`, and the file was
restored (`cmp` against a saved copy after every mutant, and after the run). 20 of 20 compiled
and turned their named legs red. The driver script and its JSON record lived in the cycle's
scratch directory (removed at the end; this table is the record). The test edits made after this
run only add legs and assertions; "Gates run" lists them.

| Edit (brief's label) | Named leg that failed | Other legs that also failed |
|---|---|---|
| Per-server `require_absent(desired:dserver:<S>)` removed from the intent batch (the loop iterates nothing) | `b1_a_drain_recorded_just_before_the_intent_commits_makes_it_replan` | — |
| Planning against the full topology instead of the topology minus draining servers (`topology.clone()` for `topology.excluding(draining)`) | `b2_a_drain_recorded_before_staging_is_planned_around` | b1, three b3 legs |
| The slot rewrite dropped from the renewal batch | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_…`, `d_ii_…` | `d_the_quiesce_renewal_…`, `e_an_unknown_renewal_…` |
| Renewal only at chunk boundaries: the body read not raced and not renewed before (`next.await`) | `d_renewal_runs_while_the_body_arrives_before_the_first_chunk`, `d_i_renewal_commits_while_a_body_read_is_pending` | the three D legs that wait on a withheld body |
| Renewal checked only between delivered pieces, with no timer (`renewing` returns `work.await` after the due-check) | `d_i_renewal_commits_while_a_body_read_is_pending` | `d_ii_…`, both lapse legs, session-left, slot-lost, the deadline leg |
| The timer not raced against a pending fragment put (`try_join_all(puts).await`) | `d_ii_renewal_commits_while_a_fragment_write_is_pending` | `d_a_lapsed_owned_lease_…`, `a_prime_a_write_pending_past_its_deadline_…` |
| The hand-off's last `ChunkRef` given `len == chunk_size` | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | — |
| After an ambiguous reserve refuses, a probe of a fresh index for the same attempt (unpin, mark lost, `continue`) | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` | — |
| Renewal that moves a lapsed lease forward (`check_live` result ignored) | `d_a_lapsed_slot_lease_is_refused_never_revived`, `d_a_lapsed_owned_lease_is_refused_never_revived` | — |
| The same-index pin removed after an ambiguous reserve (never pinned, so the lowest free index is taken) | `c_an_ambiguous_reserve_retries_the_same_index` | `c_a_held_index_taken_by_another_refuses`, `c_a_held_reserve_landing_…` |
| After an unknown intent outcome, an absent read taken as "did not land" with `may_still_commit: true` (the intent's `doubt.settle()` made `doubt.release()`) | `e_ii_a_held_intent_that_never_settles_ends_in_doubt` | — |
| `rounds += 1` made `rounds *= 1` in the intent loop | `f_the_intent_loop_is_bounded` | `e_ii_…` |
| `rounds += 1` made `rounds *= 1` in the renewal loop | `f_the_renewal_loop_is_bounded` | `f_a_held_renewal_…` |
| `probes += 1` made `probes *= 1` in the reserve | `f_the_reserve_probe_is_bounded_while_the_session_keeps_moving` | — |
| `at_pin += 1` made `at_pin *= 1` in the reserve | `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` (exact commit count) | — |
| Fragment put with deadline `None` | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…` |
| Fragment put with deadline `Some(u64::MAX)` | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…` |
| Deadline taken at dispatch (`runtime.now_millis().max(t_auth) + W`) | `a_prime_every_fragment_deadline_is_t_auth_plus_w` | `e_i_…` |
| A body piece spanning a chunk boundary merged into one oversize chunk (`split_at(rest.len())`) | `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs` | 12 others |
| *(extra, not in the brief)* The wait for fragment writes not bounded by their deadline | `a_prime_a_write_pending_past_its_deadline_is_abandoned` | — |

Two edits needed a second attempt, both my own mistakes, not test gaps: the first B1 edit deleted
the loop and left `batch` declared `mut` but never mutated, which `warnings = "deny"` refuses to
compile; and the first run of "renewal only at chunk boundaries" was a real **miss**. The slow-client
leg then only asserted "a renewal before the first intent", which a renewal at the chunk boundary
itself also satisfies. I rewrote the leg to the hazard 0016 names (`0016:1824-1837`): the clock
passes a whole TTL before the first chunk boundary, each piece read is logged, and the leg
asserts the first renewal lands before the third piece is read. It now goes red on that edit (the
staging fails with the slot lapsed at the boundary).

## Gates run on the final tree

- **`cargo xtask ci`, through the project's runner** (`engine/xtask.sh ci`, with
  `$PDCA_WORKTREE` set): `xtask ci: all checks passed`, exit 0. It ran `typos`, the docs lint and
  the docs render with its link audit (both brief-listed external dependencies were present, so
  neither was skipped), the gitlink, unsafe and blackbox guards, `cargo fmt --check`, workspace
  clippy with warnings denied, build, `cargo test --workspace --exclude wyrd-dst` (2,001 passed,
  0 failed across all test binaries, the new file's 43 legs included), cargo-machete, all three
  cargo-deny runs, the conformance vectors, the ADR-0035 statics gate, the deploy guard, and
  clippy plus tests for `wyrd-dst` under `--cfg madsim`. This is the brief's (H). It ran on the
  final tree, the one `patch.diff` records (checked by regenerating the diff and comparing).
- The first `ci` run stopped at clippy: two fixture sanity checks in the test were plain
  `assert!` on constants (`clippy::assertions_on_constants`). They are now
  `const { assert!(..) }` blocks. A second run was green with 40 legs; the C5 pass below then
  added three legs and two message assertions, and the final run above covers them.
- The demonstrated reds above ran against the 40-leg file. The later test changes only add legs
  and assertions, and the final `stage.rs` differs from the red-tested copy in one line, a doc
  comment (`stage.rs:11`, a garbled code span in the module's batch table), so every red recorded
  there still holds.
- `cargo doc -p wyrd-core -p wyrd-custodian --no-deps` (not part of `ci`): 21 rustdoc warnings,
  every one in code this patch does not touch (`erasure.rs:2`, `read.rs`, `write.rs:2`, `:123`,
  `:555`, `multipart.rs:5043`, and 15 in `metadata.rs` that only moved down by this patch's 28
  inserted lines). None points into a line this patch adds.
- `cargo fmt --all -- --check` clean; `rustfmt` was run on the two new files. `typos` clean on
  every touched file. These are the target's commit-time checks (fmt, clippy, typos); the patch
  passes them.
- `patch.diff` checked to apply cleanly to the base (`git apply --cached --check` against a
  throwaway index of `61e5341`).
- Per the brief's Verification posture, C4-verify will see a test that cannot compile on the
  reverted base and report UNVERIFIABLE (exit 77). The demonstrated reds above are the
  red→green evidence.

## C5: `cargo mutants --in-diff` survivors

Run as the project's C5 row runs it (`--in-diff patch.diff --no-shuffle --cap-lints=true`), with
two local choices: `--in-place` (to reuse the worktree's warm build), and for the core files the
test command scoped to the new file (`-- --test multipart_upload_part_stage`). The project's own
C5 row runs each package's whole suite, which contains this file, so the survivors below are an
upper bound on what it will report. 121 mutants in the diff:

- **Core** (`stage.rs` 109, `multipart.rs` 5, `metadata.rs` 5): 75 caught, 7 timeout, 31
  unviable (mostly `Default::default()` for types with no `Default`), **6 missed** on the first
  pass. Each of the 6 was a real test gap; I added an assertion or a leg for each, and a re-run of
  exactly those 6 caught all 6:

  | Survivor | What now kills it |
  |---|---|
  | `stage.rs:477` `Display for StageError::fmt` → `Ok(())` | `e_ii_…` and `d_a_lapsed_slot_lease_…` assert the message names the upload, the cause, and whether a batch is in doubt |
  | `stage.rs:500` `Display for Stop::fmt` → `Ok(())` | the same two assertions (the cause text) |
  | `stage.rs:596` `Doubt::settle` → `()` | new leg `e_an_out_of_flight_unknown_a_read_settled_is_not_reported_in_doubt` (must end `Stopped`, not `InDoubt`) |
  | `stage.rs:601` `Doubt::release` → `()` | new leg `e_a_held_intent_whose_session_moved_on_is_no_longer_in_doubt` (the held batch needed the old session bytes; it is shown to conflict when released) |
  | `stage.rs:812` `&&` → `\|\|` when recognising its own slot | `c_a_held_index_taken_by_another_refuses` now squats with another attempt of the **same** part number, so only the attempt id tells the slots apart |
  | `stage.rs:1195` `<` → `<=` in `renew_if_due` | new leg `d_renewal_falls_due_at_exactly_half_a_ttl` |

  The 7 timeouts are detections, not survivors: each makes a leg wait forever (a lapsed lease
  revived, renewal never run, the chunk boundary never reached), and cargo-mutants kills the run at
  its 20 s limit. `stage.rs:961`, `:967`, `:986`, `:1166`, `:1194`, `:1195` (`==`, `>`).
- **Custodian** (`desired_state.rs`, tested against the whole `wyrd-custodian` suite): 1 caught
  (`draining_servers` → empty map), 1 unviable. No survivor.

So no mutant in this slice's code is left alive by the final test file.

## For the human at sign-off

- **C4-verify will report UNVERIFIABLE**, as the brief pre-declares: the test calls functions the
  base does not have. The evidence it cannot produce is in "Demonstrated reds".
- **One deliberate departure from 0016's text** (decision 1 above): a held reserve whose index
  another attempt then holds refuses `SlowDown` instead of probing onward, because probing onward
  could give one attempt two slots once the other releases. Worth a look against 0016:1714-1716;
  if the proposal's text is meant literally, the brief's invariant and it disagree.
- **Two additions beyond the brief's batch listing** (decisions 4 and 6): the intent's
  `require_absent` on its own `sidx:` key, and the caller-side bound at the write deadline.
- **Seeded DST coverage** of these races is not in this slice (brief scope: #660, #665); the
  module doc carries the `deferred: #660, #665` marker for the rubric's *Test fidelity* line.
- No external dependency was missing: `typos` and the docs renderer (the brief's two) both ran
  inside `cargo xtask ci`. No `NEEDS-HUMAN` item from the build.

## Housekeeping

Nothing was committed, pushed or opened. The worktree holds the change uncommitted, matching
`patch.diff`. The test is also copied into the bundle at `multipart_upload_part_stage.rs` and
`crates/core/tests/multipart_upload_part_stage.rs`. My scratch directories under
`$PDCA_SCRATCH` (`pdca-builder-926-reds`, `-ci`, `-mutants`, `-render`, `-apply`) were removed.
