# Adversarial review — issue #854, round 6 (dropped-PUT setup bound)

**Verdict: I tried to refute the fix and could not.** I re-ran the red→green proof myself, applied seven targeted mutants to the production code and the round-6 change, and stress-ran the suite under parallel load. Every attack either failed or was caught by the tests. The one bullet that needs a human is about wording, not about the fix.

## What I re-ran (toolchain present; scratch copy of `$PDCA_TARGET`)

- **Green leg:** 12/12 pass in 3.0 s (`cargo test -p wyrd-validate --test s3_client_upload_peers`).
- **Red leg** (base `s3.rs`, `s3/body.rs`, `s3/error.rs` from HEAD `35f4eb8`, new test kept): 11 fail by assertion and `a_put_dropped_while_blocked_mid_write_releases_the_upload` passes. That matches `gate-logs/C4-verify.log` exactly, and each failure is for the stated reason (receipt, `SourceLength`, `SourceFailed`, source alive at return, or connection not closed for 4(a)).
- **Flakiness:** 36 runs at 12-way parallel load and 96 runs at 48-way parallel load on 32 cores: **132/132 pass**, no flakes.

## Refutation attempts

- **Round-6 change, `crates/validate/tests/s3_client_upload_peers.rs:1434-1457` (bound at `:202-216`).** The attack was to bring back the exact regression the human named: a request wake that never reaches the request. I made `Turn::wake` a no-op (`crates/validate/src/s3/body.rs:212-214`). The dropped-PUT test now **fails at 15 s with the new diagnostic** ("the PUT never got as far as the drop … had given 0 of 536870912 bytes and had not been dropped") instead of hanging. The other 11 tests fail at their 4 s `CALL_BOUND` (`:1031-1044`). I found no remaining unbounded await in this file. `Peer::start` is bounded by `require_socket_observation` (`:870-881`, `:904-960`), `after_call` by `:609`, and the release loop by `:1464`. The compile-time check `DROPPED_SETUP_BOUND + CLOSE_BOUND < DROPPED_T_OP` (`:215-216`) holds (16 s < 60 s). Could not refute.
- **C5's 4 TIMEOUT mutants (`crates/validate/src/s3/body.rs:195`, `:196`, `:213`, `:217`) are not hangs in this diff's tests.** Under the `wake` mutant, the whole-package run stalls past 60 s in #852's `crates/validate/tests/s3_client_roundtrip.rs`: `empty_object_round_trips`, `put_get_delete_round_trips_byte_identical` and others, which use a 300 s `TRANSFER` bound (`s3_client_roundtrip.rs:135`). This diff's file catches the same mutants within 15 s. That older file is not in this diff, so I am not filing it here. It is only how the C5 row should be read.
- **Does the evidence exercise the production path, and would it go red on the defects that matter?** Each mutant below is one-line, applied to the production code, and run against the test as shipped:
  - The body keeps polling the source after the answer (deleted `body.rs:303-305`): **3 failed**, the three late-body tests ("polled past its hold" / `SourceFailed`).
  - No request-first wait (`Turn` check at `body.rs:308` disabled): **7 failed, in 5 out of 5 runs**.
  - Round 4's receipt rule, `produced == declared` instead of `ended`, at `body.rs:126`: **4(b) and 4(c) fail**, so the rule change does real work.
  - Outcome handed back before the runtime is shut down (swapped `s3.rs:403-406`): **8 failed, in 3 out of 3 runs** (lifetime contract).
  - Abandon signal ignored on drop (`s3.rs:396`): **scenario 5 fails** ("source outlived the dropped PUT by more than 1s"). So the green-only guard does guard the patch's own drop path.
- **Can concurrent PUTs on one `S3Client` share a connection across their private runtimes?** If they could, a PUT would end up on a connection whose task dies with another PUT's runtime. They can't. `pool_max_idle_per_host(0)` (`crates/validate/src/s3.rs:137-143`) turns the hyper-util pool off entirely (`hyper-util-0.1.20/src/client/legacy/pool.rs:115-116`, `max_idle_per_host > 0`). So no checkout waiter exists that a finished connection could be handed to. Could not refute.
- **Is anything awaited between hyper handing over the response and `ResponseArrival` recording it (`s3.rs:419-443`)?** If so, the source could end in that gap. I checked `aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:497-511`: the connector future resolves and `read_after_transmit` runs in the same poll. Stalled-stream protection is off, so `MaybeUploadThroughputCheckFuture` just passes through. Could not refute.
- **Edge inputs I considered and found handled:**
  - 0-length PUT: `empty_object_round_trips` passes in `gate-logs/C4-ci.log`.
  - Always-pending source: the operation deadline fires, then the runtime shuts down.
  - Future dropped before the thread starts: `abandoned` has already resolved at `s3.rs:396`.
  - Thread or runtime fails to start: the request is dropped before the error comes back (`s3.rs:388-392`, `:408-410`).
  - Early 4xx/5xx: comes back as an error, never a receipt.

## Unwarranted claims

- NEEDS-HUMAN [human] — **"12 tests must go red pre-fix" is not what happened, and was never expected to be.** Your round-6 sign-off note says "All 12 tests must still go red pre-fix and green post-fix as in round 5". The C4-verify row in `check-gates.json` says "12 test(s) ran red". Both the gate log and my own re-run show **11 red and 1 green on base**. The green one is `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`crates/validate/tests/s3_client_upload_peers.rs:1426`), which passes on #852 because hyper releases a dropped request itself. The brief already declares this test green-only (Falsifiability). The gate's "12 ran red" means 12 tests ran in the red leg, not 12 failed. Nothing needs rebuilding. Just confirm that a green-only scenario 5 is what you meant to accept. My drop-path mutant above shows it still catches a broken abandon path in this patch.

## Not checked

- I did not diff round 6 against round 5's patch to confirm that only the dropped-PUT test changed ("keep the production code, the docs, and the other eleven tests as they are"). Round 5's patch is not among my inputs. The red/green split and failure messages are the same as round 5's description.
- C4 diff coverage was not measured. The patch stacks on #852 and does not apply on `origin/main` (`gate-logs/C4-diff-cov.log`). The mutants above cover the lines that decide the outcome and the lifetime.
