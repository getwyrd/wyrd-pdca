# Build notes — issue 575 / request-red-capacity-signals (iteration 2)

## What this iteration changed

The carry-forward scoped this rebuild to **one defect** (adversary impl finding), and I held
to exactly that. Everything else from iteration 1 is carried forward unchanged — the patch is
`iteration-v1/patch.diff` plus the fix below. I did not re-open the two findings the sign-off
explicitly excluded (the compile-failure red leg → §10 Act candidate; the per-connection
silent sheds → getwyrd/wyrd#584).

**The defect.** The RED class was captured **once, at head time**, from the response extension
(`crates/gateway-s3/src/lib.rs:664-668` on the target, `None => ErrorClass::Terminal`). A
streaming GET's head is built before a single byte is read, so it carries no seam error and can
only take that `Terminal` fail-safe default. But a streaming GET is marked errored *later*, by
the body wrapper's `Err` arm (`:510-514` pre-fix). Net effect: a D-server dying mid-read raises
a `TransientFault` inside the body stream and the request is counted
`s3_request_errors{op="get",class="terminal"}` — the transient-vs-terminal distinction the
counter exists to carry, **inverted**, on exactly the transfers a real fleet fails on (long
streaming reads, where there is time to die after the head).

**The fix** (3 lines of logic, the rest comments):

- `crates/gateway-s3/src/lib.rs:542-543` — the body wrapper's `Err` arm now runs the seam's own
  classifier over the error that actually ended the transfer:
  `let class = wyrd_traits::classify(&e); self.record_classified("failed", class);`
- `crates/gateway-s3/src/lib.rs:476-505` — `record` split into `record` (head-time class, every
  pre-existing caller) + `record_classified` (explicit class). No caller's behaviour changes.
- `crates/gateway-s3/src/lib.rs:664-673`, `:516-525` — comments recording *why* head time is
  provisional for a streaming 200, and why the `truncated` arm correctly keeps the head-time
  class.

**Consume, never re-derive** (the brief's 577 Depends-on contract) is satisfied: I call
`wyrd_traits::classify` (`crates/traits/src/lib.rs:535`), the same entry point
`gateway_error_response` uses at `:1046`. No local classification logic was added. The
fully-qualified path is required — `gateway-s3` has its own unrelated `classify`
(status/code mapping, `:1073`), and an unqualified call would silently pick the wrong one.

**Why classification works through the wrapper.** `classify` walks `source()`. axum's
`StreamBody::poll_frame` wraps every stream error as `Error::new(err)`, and `axum::Error`'s
`source()` returns the inner `BoxError` (verified in the vendored
`axum-core-0.5.6/src/body.rs` + `error.rs`; workspace pins axum 0.8 → axum-core 0.5.6). So the
chain is `axum::Error → TransientFault` and the walk reaches it. I checked this in the vendored
source rather than assuming, because the whole fix rests on it.

**Why not fix it at head time instead?** There is nothing to fix there — no error exists yet.
The alternative would be plumbing a shared cell from the handler into the body stream so the
head could "learn" the class later, which is the same information flow with an extra
indirection: ~15 lines (a `Arc<Mutex<Option<ErrorClass>>>`, its write in the `Err` arm, its
read at the completion point) versus the 3 above, and it would still classify in the `Err`
arm — because that is the only place the error is. The `Err` arm is where the fact is.

**Scope discipline.** The `truncated` arm (`:516-525`) deliberately keeps the head-time class:
a short EOF yields *no error object*, so there is nothing to classify, and `Terminal` is what
`classify` itself returns for anything it cannot name. That is the same answer, not a gap.

## The test — and the fixture bug I found and killed

`crates/server/tests/request_capacity_planes.rs`, test 3 of 6:
`the_request_plane_classes_a_mid_stream_fault_by_the_seam_not_the_head`.

**My first fixture was wrong, and it would have passed review.** I began with the obvious
`stream::iter([Ok(chunk), Err(fault)])`. It went red — but I probed what actually reached the
client instead of banking the red, and got **0 bytes**: `read result=Ok(0) len=0 raw=""`. Hyper
buffers the head and the body's first frames together and flushes when the body pends or the
buffer fills; a body that errors *immediately* is torn down with the head still unflushed. So
that fixture was a **head-time failure wearing a mid-stream costume** — the client never
received a `200` at all. It would have exercised the right code path by luck while asserting
a shape that does not occur in the field, and the "red" would have been the fixture panicking
on its own head assertion, not the metric.

The shipped fixture (`MidStreamFaultGateway`) makes the shape **deterministic**: the body
stream yields its first chunk, then parks on a `oneshot` (`Pending` ⇒ hyper flushes head +
chunk), and the test **reads the `200` and the real object bytes off the wire** before firing
the fault. The client is genuinely holding a promise when the fleet dies under it — the field
case. `read_head_and_body_prefix` blocks until the head is truly on the wire, so the fixture
cannot silently regress to the degenerate shape.

The drain tolerates the tear-down (`let _ = conn.read_to_end(..)`): the head promised a
`content-length` that will never be met, so hyper's only truthful move is to drop the
connection unterminated, which a client may see as a reset. Insisting on a clean EOF would
flake on the very fault being injected. What is asserted instead is client-observable and
exact: head `200`, real prefix bytes delivered, total short of the declared length.

## Refuting my own test

- **(a) Genuine red?** Yes — and reverted **surgically**, only the fix (the `Err` arm back to
  `self.record("failed")`), leaving the rest of the feature intact. The test fails on the
  **metric assertion**, not on a compile error and not on a fixture panic:
  `panicked at request_capacity_planes.rs:619 — a GET torn down by a mid-stream TransientFault
  is counted under class="transient"`. All fixture assertions (head 200, prefix delivered)
  pass first, so the red is about the class and nothing else. The exported series pre-fix:
  `s3_request_errors_total{class="terminal",op="get"} 1` /
  `{class="transient",op="get"} 0` — the inversion, exactly as the adversary described.
  Post-fix both assertions hold. Note the counter is **pre-registered** (every op × class
  series is exported at 0 before anything fails), so this red is a true assertion red on a
  live series — it is not the weaker compile-failure red the adversary flagged for the file
  as a whole.
- **(b) Production path?** Yes. It drives the real `S3Gateway` router over a real loopback
  listener, composed as `cli::serve_s3` composes it (`S3Gateway::new(..)
  .with_metrics_dispatch(telemetry.metrics_dispatch())` + `serve`), and reads back through
  `DurabilityTelemetry::gather_prometheus`. The classification under test is production
  `wyrd_traits::classify` called from production `AccessLogged::poll_next`. Nothing is mocked
  or re-implemented; the only test-supplied part is the `ObjectGateway` backend, which is the
  fault injector — the thing a test is supposed to supply.
- **(c) Fixture includes the fault?** Yes, and it is asserted rather than assumed. The fault is
  a real `wyrd_traits::TransientFault` wrapping a real cause (so `classify` has the same chain
  to walk as in production — the test injects the fault, it never asserts the classification),
  raised **inside** the body stream after the client has provably read the `200` and the real
  object bytes. The test asserts the head was 200 (if it were 500 the fixture would have
  degenerated into the existing head-time `FaultyGateway` and proved nothing) and that the
  body stopped short of the declared length (a complete body would mean no fault landed).

## Verification run

- `cargo test -p wyrd-server --test request_capacity_planes` — **6/6 green**, run **10×** with
  no flake (the fault is gated on a channel, not a timing guess).
- Red leg, fix-only revert — 5 pass, 1 fails on the class assertion (above).
- `cargo test -p wyrd-gateway-s3` — 37/37 green; the in-crate `AccessLogged` unit tests
  (which construct the struct directly and call `record`) are untouched by the `record` /
  `record_classified` split.
- `cargo clippy --workspace --exclude wyrd-dst --all-targets` — clean (exit 0, `-D warnings`).
- `cargo fmt --all` applied; `cargo fmt --all -- --check` clean — the target's commit hook
  runs this.
- `./engine/xtask.sh ci` — the project's own runner, full gate. See below.
- `git apply --check` against a pristine `d25c352` (`pdca-integration/main` tip, the resolved
  base) — applies cleanly.

## Residual, unchanged from iteration 1 (not re-opened; recorded so they are not lost)

1. **The in-flight gauge's mutex** spans the emission — correct, but a hot-path cost question.
2. **No `/metrics` scrape endpoint** — a live scrape stays off-Check/manual (the brief scopes
   it that way; tracked getwyrd/wyrd#585). `--otlp-endpoint` push works on both new roles.
3. **Per-connection sheds stay silent** — disclosed in code at `dserver.rs`, tracked
   getwyrd/wyrd#584. Behaviour-affecting to fix; outside this brief's emission-only constraint.
4. **Role-entry glue is executed by no test** (the unmarked adversary note) — the tests compose
   the same ~4 lines `cli.rs` composes rather than driving `cmd_s3` / `cmd_d_server`.

No external dependency was needed; nothing was worked around.
</content>
</invoke>
