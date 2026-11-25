# Adversarial review: #1007, Complete fence

**Result:** partly refuted. The fence, the release and the budget held up against every attack below. A deleted and re-created bucket gets past the bucket check, and several behaviours have no test that fails when they break. All reruns used a scratch copy of the target, and nothing was changed at `$PDCA_TARGET`.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:732` never compares the bucket marker read before the fence with the one read behind it. Failing case, run in scratch: the wrapper re-puts `bucket:photos` with different bytes just before the fence commits, and `fence_complete` returns `Held` carrying the new marker. Create treats that same change as `NoSuchBucket`, because an immutable marker that changed belongs to a bucket deleted and created again (`crates/core/src/multipart/create.rs:226-228`, `:408-410`). Fix: keep the first marker, reject a mismatch behind the fence, release, and add this test leg.
- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:732` has the same gap over the session's whole life, and the fix above does not close it. Failing case, run in scratch: Create under marker M, re-put the marker as M2, then send a valid Complete, and the answer is `Held` with M2. The session record stores no marker, and at Alpha the target is root plus `<bucket>/<key>` (`crates/core/src/multipart/create.rs:53-55`), so the target check at `complete.rs:677` cannot tell the two buckets apart. The next slices would then publish an upload from the deleted bucket into the new one. `0016:1035` only states the plain delete case. The human must decide whether a session records its marker (a new persisted field) or bucket deletion (#511) handles open sessions. The `// deferred: #511` marker at `create.rs:701` covers only the deletion fence key, so it does not settle this.
- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:36-40` and `:735` read only `psum:`, but the brief's Scope asks for "the frozen `psum:` / `part:` ranges" (brief line 107). Failing case, run in scratch: delete `part:` for named part 2 and leave its `psum:`, and the request returns `Held`. The part commit writes both records in one batch (`0016:660`), so this needs an already-torn store, and the module doc gives a sound reason (chunk lists can reach tens of megabytes). Still, the rubric's "absent entries" class applies and the choice departs from the brief's text. Please confirm it, and make sure the next slice errors on a missing `part:` record rather than skipping it.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:123`: setting `RELEASE_ROUNDS = 1` passes all 35 tests. D4 compares against the constant by name, and no test has a release lose a `Conflict` and then land. The shipped code does handle this: my scratch test (conflict once, then land) passed with 3 commits and the session `Open`. Nothing stops a later edit from removing the retry, and that would leave a fence standing on a single TiKV lock conflict. Add that leg, and pin the bound to a literal.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:677`: dropping the `session.parent() != request.parent` half passes all 35 tests. The only "another target" leg changes `object` (`crates/core/tests/multipart_complete.rs:893-899`). Add a leg with a different `parent`.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:584`: restoring the `attempts` from the first read, rather than the read the winning fence was built on, passes all 35 tests. The shipped code is right, because `opened` is rebound on each round. The untested case is: the fence loses to a session moved to a later `Open` with a different `attempts`, the retry fences, and the request is then rejected. A regression here would hand back an attempt a rival had been charged. The nonzero-budget tests (`multipart_complete.rs:1525-1590`) never retry.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:722-725` and `:736`: the doc promises the bucket is checked first and the first bad part is the one reported, but neither has a test. Walking the list in reverse, or reading the marker after the parts, both pass all 35 tests. The failing inputs are a list with two bad parts, and a deleted bucket combined with a bad part. This matters little, because 0016 does not fix either order. Test it or drop the promise.
- NEEDS-HUMAN [impl] — The docs do not match the code in three places:
  - `crates/core/src/multipart/complete.rs:59-60`, `:523` and `docs/design/architecture/05-building-block-view.md:248` say a store fault behind the fence is an error "whatever happens next". `moved()` at `complete.rs:915-920` returns an answer on that path, and `a_fault_behind_the_fence_whose_release_finds_the_session_moved` expects it. The answer itself follows the sign-off's "Conflict -> the existing moved() path", so only the wording is wrong. `complete.rs:70` already describes it correctly.
  - `complete.rs:73` says each error keeps the fault as its cause, but the in-doubt arm at `complete.rs:882-885` drops it, so an operator never sees the scan fault that started the release.
  - `crates/core/src/multipart/abort.rs:53-54` says a client sending invalid Completes can move the session under an Abort, but `complete.rs:30-32` answers those with no commit. Only a list that passes the first check and is rejected behind the fence can move it.
- NEEDS-HUMAN [human] — Two green rows in `check-gates.json` prove less than they appear to:
  - `C5-mutants` reports 46 caught and none missed, yet it missed the gaps above, because it did not try these edits (changed constants, a removed `||` operand, reordered reads). Read it as "no missed mutants among those tried", which does not meet carry-forward item 8's aim on its own.
  - `T4-batch-review` reports 0 blocking, 0 recorded-rejected and 0 noise-dropped in 35.76 s, for three codex passes over a 154 KB diff (`gate-logs/T4-batch-review.log`). Not even one noise-dropped item is surprisingly clean. `review-batch.md` is outside my inputs, so I can't say the passes were empty. Open it at sign-off before relying on this row.

## Evidence

- **Red→green:** `C4-verify` is UNVERIFIABLE (exit 77) because the module is new and the test can't compile without it, as the brief declared up front. That is not a refutation. The baseline here was 35 passed on the production code.
- **The test path:** the tests call the real `fence_complete` over a real redb store, through a wrapper that forwards to it. I found no parallel copy of the production logic.

The brief's required reds, reproduced with temporary edits (all tests failed as they should):

| Temporary edit | Failing test |
|---|---|
| attempts not restored | `a_rejected_request_restores_the_attempts_it_read` and others |
| session left `Completing` after a rejection | same group |
| another publisher's `Completing` fence treated as fenceable | `e_another_publishers_completing_fence_is_refused_and_left_untouched` |
| definite release failure answered with its refusal | `d4_a_fence_it_owns_and_cannot_release_stops_with_the_fence_standing` |
| definite release failure reported as in doubt | same D4 test |
| unknown fence treated as success | `g0_an_unknown_fence_outcome_returns_the_in_doubt_error_and_nothing_more` |

## Attempted, could not refute

- **Lost `require`:** removing it from the fence or the release fails 12 and 11 tests.
- **Other broken versions, all caught:** a wrong stamp, wrong held bytes, a constant charge, a wrong marker, a hard-coded `may_still_commit`, and the paging checks.
- **C-1 on every return path:** I found no answer that leaves this Complete's own fence standing. A moved session is never overwritten, because each release requires the fence's exact bytes and epochs only rise.
- **`scan_page` contract:** using the last key as the cursor and treating a repeated or lower key as a fault both fit clauses 2-4 of `crates/traits/src/lib.rs`. Part keys are fixed width, so byte order matches number order.
- **`downcast_ref::<CommitUnknownResult>` without walking the source chain:** this is the documented contract (`crates/traits/src/lib.rs:216`), and Abort and Create do the same.
- **Epoch and attempts ceilings:** these stop before any write, because the release record is built with the fence.
- **Not raised:** Tier-0 DST coverage is deferred to #660 by the marker at `complete.rs:480`, so that is settled. The value ceiling (child-5) and recovery after an unknown outcome (child-4) are out of scope.

<!-- pdca:leaf-complete -->
