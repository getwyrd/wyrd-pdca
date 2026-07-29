# Build notes — issue 638 / fragment-write-deadline (iteration 2)

Withheld from the reviewer; written for the human at sign-off.

All `path:line` citations are against the patched tree in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt-l1`, base `b0cd199` = `origin/main`; the brief's
`$PDCA_VERIFY_BASE = origin/pdca-integration/main` does not exist on `origin` yet, so
`run-verify.sh` resolved `origin/main` — the patch applies and verifies there).

---

## 1. What the slice does (three properties, per 0016 `:1551-1576`)

1. **The deadline travels with the write.** `ChunkStore::put_fragment` grows a
   `deadline_millis: Option<u64>` parameter (`crates/traits/src/lib.rs:696-701`) and
   `FragmentPutRequest` an additive `optional uint64 deadline_millis = 3`
   (`crates/proto/proto/wyrd/v0/chunk.proto:29-43`).
2. **The acceptor enforces it at the point of application** — `chunkstore-fs` refuses
   immediately before the publishing `fs::rename`, inside the same blocking closure
   (`crates/chunkstore-fs/src/lib.rs:276-296`), plus a pre-I/O fast path at entry
   (`:222-236`).
3. **The refusal is typed** — `wyrd_traits::WriteDeadlineExpired` +
   `is_write_deadline_expired` in the seam crate (`crates/traits/src/lib.rs:551-643`),
   `FAILED_PRECONDITION` on the wire (`crates/chunkstore-grpc/src/server.rs:92-107`),
   reconstructed client-side (`crates/chunkstore-grpc/src/client.rs:147-167`).

### Open question 1 — deadline vs authorization instant on the wire (Do's choice)

**The derived deadline travels**, not the authorization instant. Reasons: (a) the receiver
would otherwise need the *sender's* `W_write` to derive anything, which means shipping the
policy value into the data plane or duplicating it in every D server's config — and #625, not
this slice, owns that value; (b) a D server that knows only "refuse after this instant" stays
ignorant of sessions, leases and `W_write` itself, which is the ADR-0010 independence the
alternative ("derive it from a lease lookup") would break; (c) it keeps the field a single
`uint64` with no companion, so the wire contract cannot drift out of step with a policy
constant. The cost is that the *authorizer* owns the arithmetic; that is where `W_write` will
live (#625/#636).

### Clock ownership (which lifecycle owns the read)

The **applying store** owns the read: the deadline lifecycle's second evaluation site is the
acceptor, and `δ_clock` in `G_orphan > W_write + δ_clock` exists precisely to absorb the skew
between it and the authorizer (0016 `:1478`). `clippy.toml` denies a bare `SystemTime::now()`,
and AGENTS.md § Review rubric says code needing *test-controlled* time takes the testkit
`Clock` seam (ADR-0024) — so `FsChunkStore` is now generic over it,
`FsChunkStore<C: Clock = SystemClock>` with `open()` / `open_with_clock()`
(`crates/chunkstore-fs/src/lib.rs:38-86`), mirroring `MemCoordination<C: Clock = SystemClock>`
(`crates/coordination-mem/src/lib.rs:38-89`), which takes `wyrd-testkit` as a production
dependency for exactly this reason. Both reads in one `put_fragment` come from the *same*
`Arc<C>` (`:245`), so no lifecycle mixes clocks. The D-server *service* deliberately reads no
clock: it forwards the field and maps the store's refusal (`server.rs:74-82`), so an embedded
caller holding `FsChunkStore` directly gets the identical guarantee (leg E).

---

## 2. How the previous iteration's findings were discharged

The five batch-review blockers (`review-batch.md`) plus the C5/T5 rebuild asks:

| # | Finding | Discharge |
|---|---|---|
| 1–3 | `chunkstore-fs:195/203` — deadline checked before verify / `spawn_blocking` / the rename, so a write live at entry can publish after `W_write` | **Fixed.** The authoritative check is now the last statement before `fs::rename` and inside the same closure (`crates/chunkstore-fs/src/lib.rs:276-296`); the entry check is documented as a pre-I/O fast path, *not* the bound (`:222-236`). Proven binding: reverting only the publish-point block turns two new tests red (§4). |
| 4 | `write_deadline.rs:245` — raw external RPC awaited without a watchdog | **Fixed.** Every await in the new file goes through `bounded()` (a 30 s fail-closed watchdog): `ready()`, `unary()`, the dial, and every read-back (`crates/chunkstore-grpc/tests/write_deadline.rs:58-66, 248-266, 355-361`). The same discipline in the two added `round_trip.rs` legs (`:212-217`) and the DST case (`crates/dst/tests/network.rs:620-641`). The bound is 30–60 s against 50 ms–60 s deadlines, so it can never be what produces a refusal. |
| 5 | `client.rs:241` — `put_fragment` does not bound the RPC await from `deadline_millis` | **Recorded-rejected**, with the reasons in `review-rejected.md`: #508's standing rejection of a duplicate caller-side bound; `connect_with_timeout` wired at composition (`crates/server/src/cli.rs:1441`); and — the substantive point — cancelling at the deadline both *destroys* the definite "not applied" verdict (tonic renders it `CANCELLED`, which the seam classifies **transient**, `client.rs:51-57`) and does **not** stop the write, because a dropped tonic handler does not cancel the store's `spawn_blocking` closure. The decision is documented in the code (`crates/chunkstore-grpc/src/client.rs:231-246`), not only in the bundle. |
| T5a | "the current layer delays before store entry" | **Fixed.** The publish-point evidence now uses a *scripted clock* inside the production call: `wyrd_testkit::SteppedClock` (`crates/testkit/src/lib.rs:80-135`) answers the admission read 500 ms before the deadline and the publish read 500 ms after it, so the elapsing happens **between** them — no wrapper, no sleeping. Legs: `crates/chunkstore-fs/tests/conformance.rs:314-364` (local store) and `crates/chunkstore-grpc/tests/round_trip.rs:272-321` (through the real tonic service). The accept-queue delay layer is kept for 0016's literal failure-mode row, not as the publish-point proof. |
| T5b | "the DST fake uses a stronger check order" | **Fixed.** `DStore::put_fragment` now mirrors production's order exactly — verify → entry refusal → the accept-queue window → **re-check at the application point** → insert (`crates/dst/tests/network.rs:124-161`), with the test-fidelity rule cited in its doc. Removing the model's second check turns the DST case red (§4). |
| T5c | "leg F only compares malformed input" | **Fixed.** Leg F now discriminates three outcomes, including a **genuine backend fault**: a file planted where the chunk directory must be makes the real store's scratch write fail `ENOTDIR` — privilege-independent, so it holds under a root runner (no `chmod`). Deadline → `FAILED_PRECONDITION`, malformed → `INVALID_ARGUMENT`, broken disk → `INTERNAL`, all three asserted mutually distinct (`crates/chunkstore-grpc/tests/write_deadline.rs:505-578`); the typed half is at `crates/chunkstore-fs/tests/conformance.rs:359-402` and `crates/traits/src/lib.rs:1300-1345`. |

Deferred human items are unchanged and re-enter §6: C1 (mixed-fleet capability negotiation —
the degradation is now stated in the proto comment, the client doc and the architecture doc,
but *whether Alpha accepts it* is the maintainer's call) and fitness-to-purpose (no #636
caller exists yet; the mechanism ships without a `G_orphan` end-to-end margin, which is #625's).

---

## 3. Migration of existing callsites (the source-breaking half)

The wire field is additive; the Rust signature is not. 47 files carry the mechanical churn:

* **Production callers pass `None`** — unchanged behaviour by construction:
  `crates/core/src/write.rs:248` and `:447` (the ordinary four-phase write path),
  `crates/custodian/src/reconstruction.rs:556`, `crates/custodian/src/rebalance.rs:269`.
  Choosing a *value* is #625's (the brief's out-of-scope list), so no production caller sets a
  deadline yet — which is exactly why leg D (absent ⇒ today's behaviour) is load-bearing.
* **Routing wrappers forward it unchanged**: `FanoutChunkStore::put_fragment` /
  `put_fragment_at` (`crates/chunkstore-grpc/src/fanout.rs:79-93`, `:154-170`) and
  `PlacementChunkStore::put_fragment_at`'s default (`crates/traits/src/lib.rs:769-782`). A
  wrapper that swallowed the deadline would silently disable `W_write` for every fan-out write
  and *no existing assertion would notice* (the write would just succeed) — so it is pinned by
  `fanout::tests::routing_forwards_the_authorization_deadline_unchanged`
  (`crates/chunkstore-grpc/src/fanout.rs:451-490`).
* **Delegating test doubles forward it too** (17 of them, e.g. `ParkedStore`, `GateStore`,
  `Fleet`, `BlockFaultingStore`): I deliberately did *not* let them pass `None` onwards, since
  a model that drops the deadline is a weaker store than production (the rubric's test-fidelity
  rule). Terminal in-memory doubles that store into a map take `_deadline_millis`.
* **The DST model enforces it** (above), because it stands in for a real D server.

Reuse note, for honesty: the *mechanical* hunks (39 files of `, None` and signature widening)
were lifted from iteration 1's patch, which had already been gate-green, and then audited —
that audit is what turned up the 17 delegators that were dropping the deadline. Every
non-mechanical file was rewritten from the base.

---

## 4. Forced self-refutation (the three questions, with evidence)

**(a) Genuine red?** Yes, three separate ways.

1. *The gate's own red leg* (`run-verify.sh`, base + the added test file only):
   **5 tests ran, 3 failed, all by assertion** — `expect_err` panicking with "a write whose
   deadline already elapsed must be refused by the D server: ()" (leg A),
   "…parked past its deadline…" (leg B) and "an expired deadline must be refused" (leg F).
   The other 2 (leg C live write, leg D absent deadline) pass on base *by design* — they are
   the controls that stop A/B/F passing by refusing everything. No build error: the file
   compiles against the base seam because every `PutFragment` is hand-encoded protobuf over
   `tonic::client::Grpc`, never the changed Rust arity (the brief's falsifiability note).
2. *Publish-point isolation* — with **only** the publish-point block removed (1487 chars, entry
   check left in place), `cargo test -p wyrd-chunkstore-fs --test conformance` →
   `a_write_that_expires_between_acceptance_and_publication_is_refused` **FAILED**
   (12 passed, 1 failed), and `-p wyrd-chunkstore-grpc --test round_trip` →
   `a_write_that_expires_before_the_server_applies_it_is_refused_over_grpc` **FAILED**
   (4 passed, 1 failed). Restored, both green. This is the finding-specific red the previous
   iteration lacked.
3. *DST isolation* — with the sim model's application-point check removed (460 chars),
   `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test network` →
   `a_write_parked_past_its_deadline_is_refused_over_the_simulated_network` **FAILED**
   (5 passed, 1 failed), reproducible from `MADSIM_TEST_SEED=1785109683818964823`. Restored,
   6 passed.

**(b) Production path?** Yes. Every leg drives production code, no stand-ins:

* the real generated `ChunkStoreServer`/`ChunkStoreService` over a real tonic loopback
  connection, hosting the real `FsChunkStore` writing to a real temp directory
  (`write_deadline.rs::serve_in`, `round_trip.rs::connected_with_clock`);
* the production `GrpcChunkStore::put_fragment`/`get_fragment` client for the typed-error legs;
* the real `FsChunkStore::put_fragment` directly for leg E;
* the real gRPC client *and* service over madsim's simulated network for the DST case.
  The only injected things are **time** (the `Clock` seam, which is production API) and, in the
  DST, the D-server store behind the service — which that campaign has always faked and which
  now mirrors production's check order. The hand-rolled part of the new file is the *client's
  request bytes*, never the server or the store.

**(c) Fixture includes the fault?** Yes, in each leg the failing element is present rather than
curated out: the deadline actually elapses (leg A: 60 s in the past; leg B: 50 ms deadline
against a 300 ms accept-queue park; publish-point legs: the clock advances *between* the two
production reads; DST: a 200 ms deadline against a 2 s park in simulated time), the write is
actually offered to the real store, and absence is verified by reading back through
`get_fragment` (plus a scratch-litter check on the fs legs, so "not stored" also means "no
`.tmp` left"). The genuine-fault control plants a real `ENOTDIR` obstruction rather than
mocking an error, and every "not stored" assertion is paired with a positive control that
*does* store.

---

## 5. Gate evidence (this iteration, final patch)

* `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (typos + docs lint + `render_site
  --check` + fmt + clippy `-D warnings` + build + workspace tests + `--cfg madsim` DST +
  conformance vectors + statics + deploy-guard). Run three times: after the production change, after the evidence rebuild, and on the exact
  patch that ships. Every one of the 13 new/changed tests appears green in the log, including
  the seeded DST case under `--cfg madsim`.
* `./engine/scripts/run-verify.sh` → **PASS — red without the fix, green with it** (numbers in
  §4a). Run logs were captured under `$PDCA_SCRATCH` during the build and swept afterwards; the counts above are quoted verbatim from them.
* `cargo fmt --all` clean and `cargo clippy --workspace --all-targets` warning-free — the
  target's own commit hooks run these, and no PDCA gate models them.

## 6. Alternatives considered and rejected (with cost, not adjectives)

* **Enforce in the gRPC service handler** instead of the store (`server.rs`) — 6 lines
  cheaper, and wrong twice: it bounds *acceptance*, not application (a handler future dropped
  by a client reset leaves the store's `spawn_blocking` closure running to the rename), and an
  embedded caller holding `FsChunkStore` — the NAS/dev profile, and `server`'s own composition
  — would get no bound at all, breaking leg E. Kept the service as a pure translator.
* **Enforce in `FanoutChunkStore`** — still caller-side; a second gateway or a retry reaches
  the service directly. Noted in its doc comment (`fanout.rs:79-83`).
* **Keep only the entry check and rely on it being "close enough"** — that is iteration 1's
  rejected shape; the measured gap is real (§4a2 turns two tests red with just that block
  removed).
* **A single check at the publish point, dropping the entry fast path** — 8 lines smaller and
  equally *correct*; rejected because an expired write would then do a full scratch write
  (`fs::write` of up to a fragment's bytes) plus a `create_dir_all` before being refused, i.e.
  it converts a cheap refusal into disk work on exactly the path that is already under
  pressure. The doc comment says which of the two is the bound so the asymmetry cannot be
  misread; both tests that matter assert the publish-point one.
* **`ManualClock` + a test-side `advance()`** for the publish-point leg — cannot express the
  window: advancing requires a moment when the test holds control, and the window between
  admission and rename is precisely one where it does not; every variant I sketched was a
  sleep-and-hope race. `SteppedClock` (55 lines in testkit, the crate that already owns
  `ManualClock`) makes it deterministic and reusable by #636.
* **A structured `WriteDeadlineExpired { deadline_millis, observed_millis }`** — reverted to
  `{ id, detail }` mid-build: the gRPC client reconstructs the class from a `Status`, and with
  numeric fields it would have to **fabricate** readings it never took (the first draft wrote
  zeros). `IntegrityFault { id, detail }` has the same shape for the same reason; the two
  readings survive verbatim in `detail`, rendered once by
  `WriteDeadlineExpired::if_elapsed`.
* **A new `ErrorClass` variant for the refusal** — unnecessary: `classify`'s fail-safe
  `Terminal` is already the right answer (re-sending the same expired authorization can never
  succeed), and `ScanCapExceeded`/`BlockReadFault` set the precedent of a typed error with no
  bespoke class. Pinned by `a_deadline_refusal_is_typed_distinguishable_and_terminal`.

## 7. Known limits (for the human, deliberately not hidden)

1. **Residual window = the `rename` syscall.** The check is the last statement before the
   publish; a write can still become visible a syscall's duration after its deadline. No
   user-space check can close that, and `δ_clock` is what absorbs it — stated in the code
   comment (`crates/chunkstore-fs/src/lib.rs:287-290`).
2. **Mixed-version fleets get no guarantee** (old server ignores the field). Stated in the
   proto, the client doc and the architecture doc; the capability-exchange question is the
   deferred C1 item.
3. **No production caller sets a deadline yet** — by scope (#625 owns `W_write`, #636 is the
   first consumer). The mechanism is exercised end-to-end by tests only, which is the
   fitness-to-purpose item the reviewer flagged for a human; it is a scope boundary the brief
   drew, not an omission.
4. **The scripted-clock legs assume the two clock reads** the trait contract documents; a
   future store that drops the admission fast path must re-script them, and
   `SteppedClock::remaining()` is asserted so that shows up as a named failure rather than a
   puzzle.

No NEEDS-HUMAN external dependency: `typos` and the docs renderer were present (the prose
gates ran, not skipped), and `crates/proto` builds with **protox**, so no `protoc` was needed.
