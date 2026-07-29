# Build notes — issue #638 / fragment-write-deadline (iteration 5)

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE = ../wyrd.pdca-wt-l1` at `b0cd199`.
All `path:line` citations are against the **patched** worktree unless marked "(base)".

Iteration 5 starts from iteration 4's patch (the mechanical seam widening, the proto field,
the wire path, the 45-file callsite migration) and changes the **enforcement semantics**.
Everything in §1 is new relative to `iteration-v4/patch.diff`.

---

## 1. What iteration 5 changes, and which finding each change discharges

The carry-forward left three implementation-level items. Two were fixable and are fixed; the
third is fixed *as far as physics allows* and the residue is recorded-rejected with its cost.

### (i) T5 [impl] — "surface cleanup failure as a backend fault" (and the empty-directory leak)

The v4 refusal path did `let _ = fs::remove_file(&temp);` and returned
`WriteDeadlineExpired` regardless. That verdict is an **unconditional** promise ("nothing of
this write is on the store"), so returning it over residue is a silent skip — the rubric's
*Absent or unsupported entries* class — and the litter is then invisible exactly to the party
that could clean it up. Four of the seven batch findings were this one defect.

Now (`crates/chunkstore-fs/src/lib.rs:359-374`):

* rollback goes through `restore_pre_write_state` (`:180-203`), which removes the scratch
  **and** the chunk directory *this call created* — `created_chunk_dir` (`:309`, `:315`).
  Without the directory arm, every late write to a fresh chunk id leaves one behind; nothing
  ever collects them (`list_fragments` parses `.frag`, `reap_stale_temps` unlinks `*.tmp`).
* its error is **returned, not discarded**: a failed rollback becomes
  `FsChunkStoreError::RefusalNotRolledBack` (`:533-543`, `:554-562`) — a backend fault, with
  the `io::Error` reachable as `source()`, and deliberately **not** in the
  `WriteDeadlineExpired` chain, so `is_write_deadline_expired` is `false` for it.
* two outcomes are recognised as non-failures and documented as such: the entry is already
  gone (`NotFound`), or the directory is **occupied** by a concurrent write of another
  fragment of the same chunk. The occupancy test is one extra `read_dir`, on the failure path
  only, rather than an errno or an unstable `ErrorKind` variant (`:189-201`).

Race note (in the code at `:160-179`, the function's doc): `create_dir_all` succeeds on an existing directory, so
`created_chunk_dir` can over-claim. The removal then either finds the directory occupied and
leaves it, or finds it empty — and a concurrent write that meets a missing directory
recreates it on its own `NotFound` arm, which is the recovery the base already runs for a
first fragment (`:311-317`).

### (ii) Batch finding `:316` — "`fs::rename` can block … publish arbitrarily after its deadline"

This is the finding that killed iterations 1–4, in one form or another, and it is a real
gap: the pre-publication verdict establishes that publication had not *begun* too late, which
is not the same as the write having *taken effect* in time. That is literally the "bounds
acceptance, not effect" defect `0016:1557-1564` rejects for caller-side timeouts, one layer
down.

**The fix is to verify the publication instead of assuming it.** The store re-reads the same
clock immediately after the rename and, if the deadline had passed, does **not** acknowledge
the write (`crates/chunkstore-fs/src/lib.rs:399-405`,
`WriteDeadlineExpired::if_published_late`, `crates/traits/src/lib.rs:691-706`). So:

> `Ok(())` from `ChunkStore::put_fragment` now means **the fragment was published strictly
> before its deadline**, checked at *both* ends of the publishing step.

That is a strictly stronger contract than iterations 1–4 shipped, and it is the property
`G_orphan > W_write + δ_clock` actually needs from the write path. It required a second
outcome on the class, because the two say opposite things about the bytes:

| `WriteEffect` | meaning | wire | `classify` |
|---|---|---|---|
| `NotApplied` | judged too late **before** publication; store restored to pre-write state | `FAILED_PRECONDITION` | `Terminal` |
| `PublishedLate` | publication itself overran; **the bytes are on the store** | `ABORTED` | `Indeterminate` |

`Indeterminate` is not decoration: `ErrorClass`'s own doc says the partition is not binary
precisely so a caller is never told "nothing happened" when durable state changed
(`crates/traits/src/lib.rs:432-436`). Two wire codes for the same reason — flattening them
would make the *wire* lie in the one case that matters. `DEADLINE_EXCEEDED` is deliberately
**not** the code for `PublishedLate`: intermediaries and tonic's own channel deadline
generate it (`client.rs:34-41`), so a caller could not tell "your bytes are on that D server,
late" from "the RPC never arrived". Nothing else on this RPC produces `ABORTED`.

**What I did not do, and the cost of doing it** — the bytes of a late publication stay where
they landed. Removing them means retracting after the rename, which iteration 3 shipped and
review rejected on two independent grounds (crash window; unlink-by-path can destroy a
concurrent same-id writer's *acknowledged* fragment). Making retraction crash-safe needs a
durable publication marker + open-time recovery: **≈100 lines** of new production code in the
hot write path (≈35 arm/disarm, ≈40 to teach `reap_stale_temps` to resolve markers — today it
only unlinks `*.tmp`, `:117-140` — ≈25 plus a `#[cfg(unix)]` arm for inode-precise
retraction), **2–3 extra syscalls per deadline-carrying write**, and a new hazard class:
startup recovery that *deletes published `.frag` files*. And it buys one turtle — the
marker's own confirm step is equally unbounded. The trade is stated in the code
(`crates/traits/src/lib.rs:626-645`, the `PublishedLate` variant doc) and recorded in `review-rejected.md`: a late fragment is
garbage its position's `orphan:` evidence already covers (`0016:1547-1550`); an erased live
fragment is unrecoverable. The difference from v4 is that this residue is now **reported**,
not silent.

### (iii) Batch finding `network.rs:662` — unbounded `connect` await

Both new DST properties (and the third one this iteration adds) now dial with
`GrpcChunkStore::connect_with_timeout(…, 60 s)` — `crates/dst/tests/network.rs:665`, `:846`,
`:989`. Generous by two orders of magnitude over every deadline in those tests, so it cannot
be what produces a refusal, and fail-closed as `AGENTS.md:181-183` requires. Pre-existing
bare `connect` calls elsewhere in the file are untouched (out of this slice's scope).

### (iv) Recurring since iteration 2 — "advance time across the publication syscall"

Iteration 2's T5 asked for a regression that advances time **across** publication rather than
before it; v3 and v4 kept calling the pre-rename instant "publication". `AtPublicationCompletion`
(`crates/chunkstore-fs/tests/conformance.rs:616-645`) steps the clock exactly when
`<index>.frag` appears — the store's own publication event — so the third read genuinely falls
on the far side of the syscall. It could only be satisfied once the store *takes* a read
there, which is (ii).

---

## 2. Design decisions the brief asked me to state

**Deadline, not authorization instant, travels the wire** (`Design` open question 1). The
receiver would otherwise need the *sender's* `W_write` to derive the deadline — a policy value
on the wire, with a silent mismatch in a mixed fleet. Sending the deadline keeps the D server
ignorant of sessions, leases and window sizing (#625 owns the value) and leaves the acceptor
one comparison to make. Field: `crates/proto/proto/wyrd/v0/chunk.proto:56`,
`optional uint64 deadline_millis = 3` — a new tag, never a renumber, explicit proto3 presence
so absent stays absent.

**Which lifecycle owns the clock read.** The **store** does — it is the acceptor, and it is
one of the two evaluation sites `δ_clock` bounds. `FsChunkStore` holds one injected
`wyrd_testkit::Clock` (`crates/chunkstore-fs/src/lib.rs:46-54`, default `SystemClock`), and
**all three** reads in `put_fragment` (entry `:276`, publication point `:361`, publication
completed `:401`) come from that one source — the rubric's "one clock per correctness
lifecycle". No bare `SystemTime::now()` is added anywhere, so `clippy.toml`'s wall-clock ban
(#619) holds; `SystemClock` is the seam's sanctioned arm.

**Typed refusal (leg F).** `wyrd_traits::WriteDeadlineExpired` + `WriteEffect` +
`is_write_deadline_expired` / `write_deadline_outcome` (`crates/traits/src/lib.rs:561-771`)
live in the seam crate for the same reason `IntegrityFault` / `BlockReadFault` /
`ScanCapExceeded` do — every backend raises the *same* type. One shared comparison
(`if_elapsed` / `if_published_late`), inclusive `now >= deadline` to match GC's inclusive
grace test so `G_orphan > W_write + δ_clock` has no tick belonging to neither side.

**Where the gRPC service checks: it doesn't.** It forwards `request.deadline_millis` to the
store and maps the outcome (`crates/chunkstore-grpc/src/server.rs:72-82`, `:92-121`). A
handler-level check would bound only when the D server *accepted* the request, and would give
an embedded caller holding `FsChunkStore` directly a weaker guarantee than a networked one —
exactly what leg E forbids.

**Migration of existing callsites.** Source-breaking by construction (the brief mandates it).
Every existing caller and double passes `None`, defined as exactly the pre-#638 behaviour
(`crates/traits/src/lib.rs:849-852`) — 45 files, mechanical and uniform. The only non-`None`
production forwarding is `PlacementChunkStore::put_fragment_at`'s default and
`FanoutChunkStore` (`crates/chunkstore-grpc/src/fanout.rs:88-92`, `:164-168`), both of which
forward the deadline **unchanged** — routing a write must never strip its deadline — and the
fan-out's in-crate double records what it received so the forwarding is asserted, not assumed
(`fanout.rs:185-220`, test `routing_forwards_the_authorization_deadline_unchanged`).

**Mixed-version fleet** (open question 2). Alpha assumption kept: additive degradation,
documented at `crates/chunkstore-grpc/src/client.rs:259-262`, in the proto comment and in the
architecture doc. A capability exchange is not in this slice. Still a maintainer call.

**`chunkstore-fs` and leg B** (open question 3). The local store cannot "queue", so leg B is
driven over gRPC and in DST. What it *can* exhibit — the deadline elapsing between its own
data write and its publication, and across its publication — is
`crates/chunkstore-fs/tests/conformance.rs:430` and `:672`.

---

## 3. Tests, and what each one binds

**The brief-named file — `crates/chunkstore-grpc/tests/write_deadline.rs` (NEW, 5 tests).**
Unchanged from iteration 4 (it produces a genuine base red; see §4). Every `PutFragment` is
hand-encoded protobuf over a bare `tonic::client::Grpc<Channel>` so the file **compiles
against the base seam** and fails by *assertion*, not by build error. The channel is dialed
with **no timeout** and every await carries a 30 s watchdog — 600× the deadlines used — so
nothing client-side can produce the refusal. Legs A (`:389`), B (`:421`), C (`:466`),
D (`:491`), F (`:520`).

**New this iteration:**

* `crates/chunkstore-fs/tests/conformance.rs:672`
  `a_publication_that_overran_its_deadline_is_reported_and_never_acknowledged` — the (ii)
  property, with a control write in the same run whose deadline outlives the `late` reading,
  so "refuses everything" cannot pass. Asserts the effect, the class, **and** that the bytes
  really are readable — the honest report is pinned, so a later "just delete them" has to
  change an assertion and argue for it.
* `crates/chunkstore-fs/tests/conformance.rs:788`
  `a_refusal_the_store_could_not_roll_back_is_reported_as_a_backend_fault` — the (i) property.
  The injection is uid- and platform-independent (no permissions): the clock, at the instant
  it reports the write expired, replaces the store's scratch **file** with a non-empty
  **directory** at the same path, so `unlink` cannot succeed for anyone. Same standard as this
  file's existing `ENOTDIR` control. Asserts: not classified as a deadline refusal, names
  `RefusalNotRolledBack`, keeps the `io::Error` reachable, classifies `Terminal`, the residue
  is genuinely there, and nothing was published.
* `crates/chunkstore-fs/tests/conformance.rs:883`
  `repeated_late_writes_to_fresh_chunk_ids_leave_no_directories_behind` — 8 late writes to 8
  fresh chunk ids; the store root must be **empty** afterwards.
* `crates/dst/tests/network.rs:945` property 8
  `a_write_the_d_server_published_late_is_reported_not_acknowledged` — (ii) end to end over
  madsim's simulated network, the generated tonic client/service and the real `FsChunkStore`:
  the verdict, its **effect** and its class all have to survive the wire, plus a read-back
  proving the report is true and a no-deadline control.
* `crates/traits/src/lib.rs:1503` / `:1528` — the `if_published_late` boundary and the
  two-effect classification rows (the file's own "each row is pinned by a unit test" rule).
* `crates/chunkstore-grpc/tests/round_trip.rs:304-318` — the production client path now also
  asserts the refusal arrives with `effect == NotApplied` and class `Terminal`, i.e. the wire
  does not flatten the two.

**Kept from iteration 4:** the publication-point placement proof (`conformance.rs:430`), the
64-writer × 16-round race (`concurrent_put.rs:132`), the accept-queue DST property
(`network.rs:634`), the in-store expiry DST property (`network.rs:800`), and leg F's
genuine-backend-fault control (a real `ENOTDIR`, `write_deadline.rs:520`).

---

## 4. Forced self-refutation

**(a) Genuine red?** Yes — I reverted the patch and re-ran, and I mutated each new production
hunk individually.

*Base red (the brief's `Falsifiability` requirement), reproduced by me this iteration:*
`git stash push` (leaving the untracked new test file), then
`cargo test -p wyrd-chunkstore-grpc --test write_deadline`:

```
running 5 tests
test expired_deadline_is_refused_by_the_server_and_never_stored ... FAILED
test a_deadline_refusal_is_distinguishable_from_both_a_client_fault_and_a_disk_fault ... FAILED
test absent_deadline_stores_exactly_as_before_issue_638 ... ok
test a_live_write_within_its_deadline_stores_and_reads_back_byte_identical ... ok
test a_write_parked_past_its_deadline_is_refused_when_finally_applied ... FAILED
test result: FAILED. 2 passed; 3 failed
```

**5 tests ran, 3 failed, 2 passed. The crate compiled — no build-shaped red.** The three
failures are `expect_err` **assertions** on `put_fragment_raw(...)` returning `Ok(())`, at
`write_deadline.rs:397`, `:438`, `:543` — the base server accepted and stored the expired
write. The two passes are the *controls* (live write; absent deadline), which is exactly
right. Post-fix: **5/5 green**.

*Mutation testing of the new hunks* (each mutation applied alone, then reverted; verified the
tree diffed clean against a pre-mutation copy afterwards):

| # | Mutation | Result |
|---|---|---|
| M1 | delete the post-publication verification (`if_published_late` block) | RED: `conformance::a_publication_that_overran…`, `conformance::an_expiring_write_never_disturbs…` (unconsumed clock read), `dst::network::a_write_the_d_server_published_late…` |
| M2 | `let _ = restore_pre_write_state(…)` — the iteration-4 defect verbatim | RED: `conformance::a_refusal_the_store_could_not_roll_back…` |
| M3 | pass `None` for the created chunk directory | RED: `conformance::repeated_late_writes_to_fresh_chunk_ids…`, `conformance::the_deadline_verdict_falls…` |
| M4 | both effects ride `FAILED_PRECONDITION` (flatten the wire) | RED: `dst::network::a_write_the_d_server_published_late…` |
| M5 | `PublishedLate` classifies `Terminal` | RED: `traits::the_two_deadline_effects_classify_differently`, `conformance::a_publication_that_overran…` |

**(b) Production path?** Yes. Every leg drives the real `ChunkStoreService`, the real
generated tonic server/client and the real `FsChunkStore`. Nothing is mocked. The only
injected thing is the store's `Clock`, through a **production** constructor
(`FsChunkStore::open_with_clock`, `crates/chunkstore-fs/src/lib.rs:70`), and in the new legs
it *decides nothing*: `AtPublicationCompletion` and `AtAnyScratch` report what the store's own
directory shows. `ExpiresAndBlocksRollback` additionally injects a filesystem state — that is
the fault injection, not a substitute for the code under test. The `DStore` fake in
`crates/dst/tests/network.rs:118` is a pre-existing sim model; all three deadline properties
there run against the real `FsChunkStore`.

**(c) Fixture includes the fault?** Yes. The expired write is *sent* and *accepted* by the
server, never filtered out before it: leg A sends an already-elapsed deadline with a generous
client timeout; leg B parks the request behind a real delaying service so the deadline elapses
after acceptance; DST property 6 parks it in a **detached** task so the client's abandonment
cannot cancel it (mirroring `spawn_blocking`'s uncancellable closure); property 7 lets it
expire *inside* the store after its bytes are on disk; property 8 lets publication itself
overrun. Every refusal leg is paired with a control in the **same run** (a live write, or a
deadline-less write with an identical park/clock), so "refused everything" cannot pass. The
rollback-fault leg asserts the residue is *present*, so it cannot pass on a store that
silently cleaned up.

---

## 5. Gate status

`./engine/xtask.sh ci` (the project's own runner, executing in `$PDCA_WORKTREE`) — run to
completion three times, most recently over the final tree: **`xtask ci: all checks passed`**. The prose gates were **present**, not
warn-skipped (`$ typos` ran; `render_site: wrote 98 page(s) … link audit OK`), so the brief's
two external dependencies were satisfied and **no NEEDS-HUMAN external-dependency marker is
warranted**. `./engine/xtask.sh dst` — green, 8/8 in `tests/network.rs` including all three
deadline properties. `cargo fmt --all` was run over every touched file (the target's commit
hook runs `fmt --check`), and clippy is clean for both the workspace and, under
`RUSTFLAGS=--cfg madsim`, `wyrd-dst`.

Docs currency (`AGENTS.md:154-157`): the RPC changed, so
`docs/design/architecture/08-crosscutting-concepts.md:106` changed in the same patch — rewritten
for the verified-publication contract, the rollback-fault rule and the two wire codes.

Self-review against `AGENTS.md` § Review rubric: one clock per lifecycle (§2); await discipline
(the DST dials are bounded now; the client-side per-call bound stays recorded-rejected);
`#![forbid(unsafe_code)]` untouched; no DST-reachable shared mutable global (statics gate green);
*Absent or unsupported entries* — the rollback failure is now an explicit error, not a silent
skip; *Test fidelity* — the new destructive path lands with a seeded DST property.

## 6. Still open for the human at sign-off

* **Fitness-to-purpose (carried, unchanged):** a mixed-version fleet gets no guarantee — an old
  D server silently ignores `deadline_millis`. Alpha-acceptable is my assumption; a capability
  exchange is not in this slice.
* **The late-publication residue is reported, not removed.** §1(ii) gives the alternative, its
  ≈100-line cost and the new startup-deletion hazard it introduces; `review-rejected.md` records
  it against the batch finding. This is the one place a maintainer might reasonably overrule me —
  and note that the *acknowledgement* half is now fixed either way: no caller is ever told a
  late write landed in window.
* `scripts/review-branch` is not available in this environment, so the batched-review gate's red
  could not be reproduced or re-run locally by me either. The two findings I could not simply
  delete are answered in `review-rejected.md` in the format the triage reads.
