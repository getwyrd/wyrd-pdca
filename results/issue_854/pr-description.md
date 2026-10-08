## Summary
**User impact:** `wyrd-validate` exists to catch an S3 server that loses or
mishandles data. Its upload call could be fooled by exactly such a server: if the
server said "OK" before it had received the whole object, the validator counted
the object as stored, even when only 3 MiB of a 64 MiB upload had gone out. A
server that stopped reading partway could also leave the validator holding the
upload's data and an open connection after the call had already returned.
Nothing runs this client yet, so no validation report has been wrong so far; the
validation scenarios that will use it would have inherited both problems.

This PR makes the validator's upload count as stored only when the caller's data
had been fully handed over and finished before the server answered. Any earlier
"OK" is reported as an error carrying the server's request id. Every upload,
however it ends, now lets go of its data and closes its connection before the
call returns.

**Depends on #859** (the S3 client this changes), which in turn depends on #845
and #849. This branch is built on top of #859; merge it first. It also edits the
same files as the #853 change, so whichever lands second will need a rebase.

## What to look at
- **The rule for "stored".** The doc comment on `put_object` in
  `crates/validate/src/s3.rs` states it in one place: what counts as a receipt,
  what the client still cannot see, and what one upload costs.
- **How the call ends.** Each upload now runs on a small runtime of its own that
  is shut down before the result is returned. That is how the data and the
  connection are released even when the server has stopped reading.
- **The cost.** One thread and one fresh TCP connection per upload. Each leaves a
  socket in `TIME_WAIT`, which limits sustained uploads to roughly 470 a second
  per client/gateway address pair with Linux defaults. The doc says how an
  operator raises that limit. Reusing connections needs a custom HTTP connector
  and is left to a follow-up; it must land before high-rate validation scenarios.
- **The architecture overview** gets one updated paragraph to match.

To try it: `cargo test -p wyrd-validate --test s3_client_upload_peers`. It runs
the real client against a scripted server on loopback that answers early, stops
reading, or both. No containers or network. The file takes about 3 s.

## Root cause
The client returned whatever outcome the AWS SDK reported, and the SDK reports
success as soon as hyper reads a `200` response head, whether or not the request
body has been sent. A connection blocked writing to a server that has stopped
reading never polls the request body again, so waking the body is not enough to
release it; only dropping hyper's connection task frees the socket and the body,
and the SDK's connector exposes neither.

## Fix
- **Receipt rule** (`crates/validate/src/s3/body.rs:85-159`): the upload records
  whether the source had *ended* (given its whole declared length and then
  reported end-of-stream) at the moment the request sees the response head
  (`Upload::answered`, `:121-129`, called from the `ResponseArrival` interceptor
  in `s3.rs:427-443`). `Upload::acknowledged_early` (`:149-159`) turns any success
  that came sooner into `BodyError::AcknowledgedEarly`. The body sets `ended` only
  when the source reports its end at exactly the declared length (`:336-338`).
- **Nothing polled after the answer** (`s3/body.rs:162-254`, `Turn` and
  `RequestFirst`): the body never runs ahead of the request, so once the request
  has the response the source is not polled again and the recorded state stays
  final. A source that would overrun after the answer is therefore reported as
  `AcknowledgedEarly`; an overrun before the answer is still `SourceLength`.
- **Lifetime** (`s3.rs:372-417`, `on_own_runtime`): the request runs on a
  current-thread runtime on its own thread, which is shut down before the outcome
  is handed back. That drops hyper's connection task, its socket and the request
  body. Dropping the future does the same promptly. The uploads client keeps no
  idle connection pool (`s3.rs:92-94`, `:132-146`).
- **Error** (`s3/error.rs:95-108`, `Display` at `:197-208`): the new
  `BodyError::AcknowledgedEarly { declared, produced, request_id }`.
- **Docs**: the `put_object` doc (`s3.rs:154-208`) states the receipt rule; the
  limit (the client cannot see whether the server read what it was handed, nor an
  answer still sitting in the kernel's receive buffer, about which nothing can be
  done without access to the socket); that the read-back check planned for the
  validation scenarios will catch a false receipt only on a single-writer key read
  back before its next overwrite, and not on the contention pool; and the
  per-upload cost. `docs/design/architecture/05-building-block-view.md:255` says
  the same in brief. The `// deferred: #854` marker #859 left in `s3.rs` is
  removed.
- **Test** (`crates/validate/tests/s3_client_upload_peers.rs`, new): twelve
  loopback cases against the production client, the real SDK and hyper
  connector. Tests that need to watch the client's socket read `/proc/net/tcp`
  and fail with a clear message on a host that hides it, rather than passing
  without checking.

## Verification
- **Claim:** an answer that arrives before the source has ended is never a
  receipt, and carries the answer's request id.
  - **Checked:** `crates/validate/src/s3/body.rs:121-159` and
    `crates/validate/src/s3.rs:227-233` (the outcome check after the call returns).
  - **Test:** the seven early-answer cases in
    `crates/validate/tests/s3_client_upload_peers.rs:1205-1321` (answered
    mid-upload, source failing after the answer, answer body arriving late, a late
    `403 AccessDenied` that must stay the server's error, early answer under
    backpressure with 32 MiB pieces).
- **Claim:** the receipt boundary is "ended", not "gave every byte".
  - **Checked:** `s3/body.rs:296-297`, `:336-338` (where `ended` is read and set).
  - **Test:** `:1344` (source ended before the answer: a receipt), `:1380` (all
    10 bytes given, end held: not a receipt), `:1397` (10 of a declared 10 given,
    an 11th held: not a receipt). The held cases release the source only after
    the client has read the answer off its socket, so each has one legal outcome
    regardless of scheduling, and each asserts that this is what happened
    (`:1125`, `:1167`).
- **Claim:** whenever the call returns, the source is already dropped and the
  server sees the connection closed within 1 s of that return.
  - **Checked:** `s3.rs:372-417`.
  - **Test:** `assert_lifetime` (`:1059-1071`) runs on every case that returns,
    against one deadline taken from the call's actual return. `:1325` covers a
    server that stops reading and never answers (the typed operation timeout, plus
    the lifetime check). `:1427` covers a PUT dropped while blocked mid-write; its
    setup wait has its own 15 s bound, so a regression fails with a message rather
    than hanging.
- **Fails before, passes after:** with the production change reverted and the
  test kept, 11 of 12 fail by assertion (the client returned a receipt, a
  `SourceFailed` with no request id, or left the source or connection alive). The
  dropped-PUT case passes before the fix too, because hyper already releases a
  request dropped before its answer; it is kept as a guard. With the fix, 12 of 12
  pass, six runs out of six. Against an earlier version of this change that only
  required "every byte given", exactly the two held-end cases fail, which shows
  the stricter rule is what fixes them.
- **Whole tree:** `cargo xtask ci` passes (fmt, clippy, workspace tests, deny,
  docs, the madsim DST build).

Fixes #854
