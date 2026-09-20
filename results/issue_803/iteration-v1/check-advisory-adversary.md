# Adversarial review — #803 staged protection class

Verdict: I could not break the fix. One test gap is real: the rule that protects a session
"whatever its state" is not pinned by any test.

## Findings

- NEEDS-HUMAN [impl] — **Nothing tests a session that is not `Open`, so dropping every non-`Open` session goes unnoticed.** `crates/custodian/src/gc.rs:791-805` reads every session listed under `mpu:`, in any state, and that is correct. But every test fixture is `Open`: `crates/custodian/tests/staged_protection.rs:465` (`session_value`) and `crates/dst/tests/custodian.rs:2699` (`STAGED_SESSION`). Concrete case: I added a filter at the top of the `mpu:` loop (`gc.rs:791`) that skips any session whose value lacks `"kind":"Open"`. All 11 tests in `staged_protection.rs` passed, and so did both new DST properties (`gc_staged_build_under_concurrent_handoffs`, `gc_staged_build_reaches_every_landing`, 50 seeds). That filter drops exactly the parts that matter most. The root flip requires `mpu == Completing@E` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`), and a `Completed` session's `part:` records stay until its `retire:records:` drain runs (`0016:573`). So in the real protocol every publication handoff happens under a non-`Open` session. Leg C(ii) (`staged_protection.rs:762`) and the DST writer (`crates/dst/tests/custodian.rs:2785`) both publish from an `Open` session, which the protocol never does. That breaks the rubric's test-fidelity rule: a test model should match production behaviour. Fix: seed `Completing`, `Aborting` and `Completed` sessions in legs A and B (valid shapes are in `crates/core/tests/multipart_session_records.rs:81-141`). In c2 and property 13, move the session to `Completing` before the publication and to `Completed` in the publication batch.

## What I tried that did not break it

- **Red→green, re-run at `$PDCA_TARGET`.** 11 of 11 pass on the patched tree. `gate-logs/C4-verify.log` shows all 11 red on the base, each failing an assertion, none failing to compile. The tests drive the production `reconcile_step` and `reconcile_after_restore` (`staged_protection.rs` `gc_pass` / `restore_pass`), not a copy of their logic.
- **Mutations, run in a scratch copy.** Each was caught:
  - Reading `part:` before `sidx:` (`gc.rs:799-804`): c1 goes red, and so does the DST at `crates/dst/tests/custodian.rs:2819` (the fragment is reclaimed).
  - Building the staged set after the `inode:` scan (`gc.rs:552`): c2 goes red, and so does the DST at `:2819`.
  - Filling an empty staged placement by identity (`gc.rs:742`): e2 (owned placement of the wrong length) goes red.
  - Protecting the whole chunk instead of each (server, fragment) pair (`gc.rs:473`): A and B go red.
  
  The C5 gate's "pass" is thin: only 6 of its 25 mutants compiled. These manual mutations are the stronger evidence.
- **Scan-cap overflow.** Not reachable. `MAX_SESSIONS` is 46 (`crates/core/src/multipart.rs:4624`). Each session's `part:` range holds at most 10,000 records (`:4470`) and its `sidx:` range at most 16 × 158 (`:4479`). The scan cap is 1,048,576 (`crates/traits/src/lib.rs:286`).
- **`mpuctl` matching the `mpu:` prefix.** It doesn't: the fourth byte differs (`multipart.rs:1126-1132`), and core tests pin this.
- **A scheme with zero fragments, which would place nothing and hold nothing.** Rejected at decode: `checked_chunk_scheme` for `part:` (`multipart.rs:2556`) and `checked_staged_scheme` for `sidx:` (`:3574`).
- **Deadlock with teardown, where GC keeps bytes that teardown waits on.** None. Teardown orphan-marks the bytes and deletes the records in the same step (`0016:355`, `:671`), so the records never wait on GC.
- **A handoff between restore's two readings.** Only the first reading (`restore.rs:309`) reads staged records. But it reads source before destination, so a chunk that moves afterwards was already seen in the class it left.
- **Not raised:**
  - A held staged record does not set `needs_human()`, so the CLI still says "complete". This is marked `// deferred: #664` at `restore.rs:326`, and the rubric treats that as settled.
  - Restore's displaced-copy check (`restore.rs:385-436`) only looks at committed placements. So a staged fragment moved off its recorded server would be marked. Nothing moves staged fragments until #663, and the restore fence is #664's.
  - Scrub (`scrub.rs:88`) and drain status (`desired_state.rs:188`) now pay for the staged build and ignore the result. The brief accepted the shared builder.
