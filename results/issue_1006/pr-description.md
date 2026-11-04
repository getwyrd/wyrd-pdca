## Summary
**User impact:** the black-box validation tool's capability check could not tell a working S3 server from a broken one. A run against a correct server could never pass. A server that returned the wrong data, lost an object it had stored, or claimed to delete objects it had kept was never reported as a failure. Nobody sees this yet, because the check isn't wired into a command, but the upcoming smoke run (#939) can't give a meaningful pass without this change.

This PR makes the check judge every operation a server must support, so a correct server passes and a wrong answer fails, naming the operation and the kind of failure.

Second part of #937, building on the capability matrix added in #1005.

## What to look at
The main rule to review: an incomplete answer can never hide a wrong one. If a download is cut off after some wrong bytes, the result is a failure, not "couldn't tell". The run is inconclusive only when nothing was wrong but something stayed unproved.

To try it, run:

```
cargo test -p wyrd-validate --test capability_matrix_required
```

To see the tests fail without the fix, restore `crates/validate/src/matrix.rs` from `main` and run the same command. The tests use fixed inputs and need no server, network or Docker.

Choices a reviewer may want to weigh in on:

- **Retries stay unproved.** A follow-up `HEAD` that got no answer or was cut off leaves the row unproved, even if a second `HEAD` of that key answers 404. The smoke runner will need to allow for this if it records every retry.
- **`DeleteObjects` of no keys still judges its follow-ups.** A 5xx or a body byte on one is a failure. The spec's two rules conflict here, and this reading reports more failures, never fewer.
- **`Content-Range` is compared by value.** `BYTES 2-5/16` and `bytes 02-05/016` both pass, following RFC 9110 §14.1 and §14.4. Signs, spaces, `*` and overflow are refused.
- **A 404 counts as a lost object only on required rows.** A generic exercise records no written key, so its 404 stays unsettled.

## Root cause
`required` in `crates/validate/src/matrix.rs` was a placeholder that marked every required row "not yet defined" (`matrix.rs:1001` on `main`). Only the shared 5xx check could reach a verdict, so no run could pass and no other wrong answer could fail.

## Fix
`required` now records findings for each row from a fixed table of what that row reads; the table is in its doc comment. No check returns early. Each one only adds a finding to the existing message-order evidence, and `settle` picks the first fatal finding, so a cut or an unusable request can't stop a later check from running.

- **Failure classes:** `oracle` means the server's account of the store is wrong, `integrity` means the delivered bytes are wrong, and `protocol` means any other answer that isn't the defined one.
- **New private helpers:** the row checks, plus `parse_content_range`.
- **Unchanged:** no public type, variant, field or function was added, and the verdict fold and `crates/validate/tests/capability_matrix.rs` are untouched.
- **Known gap:** the gateway line numbers in the `PRE_0_1_ROWS` `cites` strings (`matrix.rs:285-338`) have gone stale. They come from the earlier PR, so this one leaves them for a follow-up.

## Verification
Line numbers in `crates/validate/` are on this PR's branch (`main` at `fd64faf` plus this patch). Gateway line numbers are on `main`.

| Claim | Code | Test in `crates/validate/tests/capability_matrix_required.rs` |
|---|---|---|
| A fully correct `pre-0.1` run passes | `matrix.rs:1020` | `:383` |
| One wrong answer fails the run, naming the row and class | `matrix.rs:864` | `:397` |
| One unproved or unanswered exercise makes the run inconclusive | `matrix.rs:864` | `:444`, `:501` |
| A cut, an unusable request or a lost follow-up never hides a wrong answer | `matrix.rs:1020`, `:1391` | `:523`, `:567`, `:588` |
| The first failure in message order names the class | `matrix.rs:864` | `:630` |
| A 404 to a read of a written key is `oracle`, whatever was asked | `matrix.rs:1138` | `:749` |
| A wrong status alone is `protocol` on every row | `matrix.rs:1149` | `:1143` |
| Wrong bytes or a wrong declared length are `integrity` | `matrix.rs:1246` | `:982` |
| A wrong `HEAD` or listing is `oracle` | `matrix.rs:1227`, `:1342` | `:787`, `:802` |
| A bulk delete result must name each key once, with no errors | `matrix.rs:1361` | `:1343` |
| A clean 404 doesn't erase a lost follow-up | `matrix.rs:1391` | `:896` |
| A follow-up is judged against the key it asked about | `matrix.rs:1413` | `:851` |
| `Content-Range` must match the requested span exactly | `matrix.rs:1462` | `:1405` |

The gateway answers 200 even when individual deletes fail, which is why `<Error>` rows are checked (`crates/gateway-s3/src/lib.rs:2396-2397`, `:2548`).

**Results:**

- **Before the fix:** the new test file compiles, with 39 tests failing and 3 passing. Those 3 pin behaviour that was already correct.
- **After the fix:** all 42 new tests pass, and the 30 existing tests in `capability_matrix.rs` pass unedited.
- **`cargo xtask ci`:** passes.
- **Diff coverage:** 333 of 333 instrumentable changed lines were executed.
- **`cargo mutants` on the diff:** 75 caught, 3 unviable, none missed.

These ran on a base with the same `matrix.rs` as `main` (blob `70d2726`), not on `fd64faf` itself. The patch applies cleanly to `fd64faf`.

**Not covered:** the classifier hasn't been run against a live gateway. Its inputs are hand-built, and turning real responses into those inputs is part of #939.

Fixes #1006
