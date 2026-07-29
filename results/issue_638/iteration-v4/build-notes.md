# Build notes — issue #638 / fragment-write-deadline (iteration 4)

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE = ../wyrd.pdca-wt-l1` at `b0cd199`.
All `path:line` citations below are against the **patched** worktree unless marked "(base)".

---

## 1. What changed relative to iteration 3, and why

Iterations 1–3 all shipped the same proto/traits/gRPC surface and were rejected on the same
axis: **where in `FsChunkStore::put_fragment` the deadline is judged.** The three rejections
are worth restating because they define the design space:

| Iter | Enforcement point | Reviewer finding |
|---|---|---|
| v1 | before `spawn_blocking` | "the clock read precedes `spawn_blocking`, disk write, and rename" — bounds acceptance, not effect |
| v2 | inside the closure, before `rename` | "close the interval between the last clock read and actual publication" |
| v3 | after `rename`, then retract (`remove_file`) | C5: "make the expiry verdict atomic and crash-safe with publication — the code publishes first and explicitly admits a reader-visible interval before retraction, so a crash can leave late bytes even though `WriteDeadlineExpired` promises 'not applied'"; T3: "the pre-rename existence snapshot races same-id puts and deletes … can retract another acknowledged writer's fragment" |

v2 and v3 pull in opposite directions, so this iteration had to resolve the tension rather
than slide along the axis again.

### The argument I settled on

`rename(2)` admits no predicate. There is no filesystem primitive that makes publication
conditional on a clock, so the verdict is either **before** the publishing step or **after**
it. Those are the only two positions, and they are not symmetric:

* **After** — the store must then either keep the late bytes (0016 outcome (a), the leak) or
  unlink them. Unlinking is not atomic with the rename, so (i) a reader can observe the
  fragment in between, (ii) a crash in between leaves durably exactly the bytes
  `WriteDeadlineExpired` says do not exist, and (iii) the unlink targets a *path*, not an
  inode, so a concurrent same-id writer's already-acknowledged fragment can be destroyed.
  That is v3, and all three of C5/T3's findings are consequences of this one choice.
* **Before** — the refusal has published nothing. `WriteDeadlineExpired`'s "not applied" is
  then true **unconditionally, including across a crash**: kill the process anywhere in the
  closure and the store holds either the pre-write state or a scratch file that
  `reap_stale_temps` clears at the next open (`crates/chunkstore-fs/src/lib.rs:117-140`).
  There is no retraction, so there is no cross-writer hazard either.

So "atomic and crash-safe with publication" — C5's literal demand — is *achieved* by the
pre-publication verdict and is *unachievable* by the post-publication one. That is the
change.

### What answers v2's finding at the same time

v2's objection was that a check placed early bounds acceptance rather than effect. That is
answered by moving **everything that can consume unbounded time above the verdict**, so the
only thing left below it is the publish primitive itself:

`crates/chunkstore-fs/src/lib.rs:275-320` — the verdict sits after the blocking-pool hop
(`offload`, `:245`), after `create_dir_all` (`:262-264`), and after the fragment's data
write (`:259-274`); the next statement is `fs::rename` (`:316`) on an already-written file
inside one directory. A write parked in the D server's accept queue, in the blocking pool,
or behind slow I/O is therefore **refused rather than queued** — 0016's own wording,
`0016:1560`.

The residue is one `rename(2)`: publication can complete marginally after the deadline.
I state that explicitly in the trait contract (`crates/traits/src/lib.rs:698-712`) rather
than eliding it. It is irreducible, and it is what 0016 already budgets for — `0016:1478`
defines `δ_clock` as bounding "the deployment wall clock's resolution and **any skew between
the two evaluation sites**". It is categorically not the unbounded queueing 0016 rejects,
because every queue is upstream of the verdict.

### Cost of the alternative I rejected (quantified)

I did consider making the *post*-publication verdict crash-safe rather than abandoning it.
The only way to do that on a filesystem is to make the visibility predicate itself depend on
the deadline, i.e. a durable publication marker:

1. write scratch; 2. verdict; 3. create `<index>.<seq>.<deadline>.publishing`; 4. rename;
5. re-verdict → unlink `<index>.frag` if late; 6. unlink the marker (confirm).
Recovery at `open` deletes any `<index>.frag` that still has a `.publishing` marker.

Concrete cost, counted against the files this patch already touches:
* `crates/chunkstore-fs/src/lib.rs`: ~35 lines to arm/disarm the marker, ~40 lines to teach
  `reap_stale_temps` to resolve markers (it currently only unlinks `*.tmp`, `:117-140`), and
  ~25 lines (plus a `#[cfg(unix)]` arm) for inode-precise retraction so step 5 cannot delete
  a rival's file — **≈100 lines** of new production code in the hot write path.
* **2–3 extra syscalls per deadline-carrying write** (create marker, unlink marker, plus a
  `stat` for the inode check).
* A **new hazard class**: open-time recovery that *deletes published `.frag` files*. On a
  filesystem where the marker create fails (or on a partial `reap`) that is data deletion at
  startup — strictly worse than the residue it removes.

And it buys only the last `rename(2)` — the same residue the pre-publication verdict has,
one turtle down (step 6's confirm is itself unbounded). I judged ~100 lines of new
delete-at-startup machinery a bad trade for a residue `δ_clock` already covers, and the
brief's leg E asks for the local store's parity "cheaply". If the maintainer disagrees at
sign-off, the marker scheme above is the shape to ask for.

---

## 2. Design decisions the brief asked me to state

**Deadline, not authorization instant, travels the wire** (`Design` open question 1). The
receiver would otherwise need the *sender's* `W_write` to derive the deadline, which puts a
policy value on the wire and makes a mixed-fleet mismatch silent. Sending the deadline keeps
the D server ignorant of sessions, leases and window sizing — #625 owns the value — and
leaves the acceptor with exactly one comparison to make. Field:
`crates/proto/proto/wyrd/v0/chunk.proto:46`, `optional uint64 deadline_millis = 3`
(a new tag, never a renumber; explicit proto3 presence so absent stays absent).

**Which lifecycle owns the clock read.** The **store** does — it is the acceptor, and it is
one of the two evaluation sites `δ_clock` bounds. `FsChunkStore` holds one injected
`wyrd_testkit::Clock` (`crates/chunkstore-fs/src/lib.rs:50-54`, default `SystemClock`,
`:57-63`), and *both* reads in `put_fragment` (`:230`, `:309`) come from that one source —
the rubric's "one clock per correctness lifecycle". No bare `SystemTime::now()` is added
anywhere, so `clippy.toml`'s wall-clock ban (#619) is respected; `SystemClock` is the seam's
own sanctioned arm (`crates/testkit/src/lib.rs:33-43`).

**Typed refusal (leg F).** `wyrd_traits::WriteDeadlineExpired` + `is_write_deadline_expired`
(`crates/traits/src/lib.rs:582`, `:643`) live in the seam crate for the same reason
`IntegrityFault`/`BlockReadFault`/`ScanCapExceeded` do — every backend raises the same type
(`crates/traits/src/lib.rs:288-300` is the model the brief cited). One shared comparison,
`WriteDeadlineExpired::if_elapsed` (`:613`), inclusive `now >= deadline` to match GC's
inclusive grace test so `G_orphan > W_write + δ_clock` has no tick belonging to neither
side. On the wire it rides `FAILED_PRECONDITION` (`crates/chunkstore-grpc/src/server.rs:105`)
and reconstructs client-side as the typed class (`.../client.rs:160-168`).

**Where the gRPC service checks: it doesn't.** It forwards `request.deadline_millis` to the
store (`crates/chunkstore-grpc/src/server.rs:82`). A handler-level check would bound only
when the D server *accepted* the request, and would give an embedded caller holding
`FsChunkStore` directly a weaker guarantee than a networked one — the exact thing leg E
forbids. Rationale is in the code at `server.rs:72-80`.

**Migration of existing callsites.** The trait change is source-breaking by construction (the
brief mandates it). Every existing caller and double passes `None`, which the contract
defines as exactly the pre-#638 behaviour (`crates/traits/src/lib.rs:714-717`) — 45 files,
mechanical and uniform. The only non-`None` production forwarding is
`PlacementChunkStore::put_fragment_at`'s default (`crates/traits/src/lib.rs:790-800`) and
`FanoutChunkStore` (`crates/chunkstore-grpc/src/fanout.rs:88-92`, `:164-168`), both of which
forward the deadline **unchanged** — routing a write must never strip its deadline. The
fan-out's in-crate double records what deadline it received
(`crates/chunkstore-grpc/src/fanout.rs:185-220`) so that forwarding is asserted, not assumed.

**Mixed-version fleet** (open question 2). I kept the Alpha assumption: additive degradation,
documented honestly at `crates/chunkstore-grpc/src/client.rs:244-247`, in the proto comment,
and in the architecture doc. A capability exchange is not in this slice. The reviewer flagged
this as a fitness-to-purpose judgment call for the maintainer; it remains one.

**`chunkstore-fs` and leg B** (open question 3). The local store cannot "queue", so leg B's
parked-write scenario is driven over gRPC and in DST. What the local store *can* exhibit —
and what iteration 3's reviewer asked for — is the deadline elapsing between its own data
write and its publication; that is `crates/chunkstore-fs/tests/conformance.rs:429` and DST
property 7.

---

## 3. Tests, and what each one binds

**The brief-named file — `crates/chunkstore-grpc/tests/write_deadline.rs` (NEW, 5 tests).**
Unchanged in structure from iteration 3 (it produced a genuine base red and passed C2/C4);
only its module doc was corrected for the new enforcement point. Every `PutFragment` is
hand-encoded protobuf over a bare `tonic::client::Grpc<Channel>` so the file **compiles
against the base seam** and fails by *assertion*, not by build error. Client channel is
dialed with **no timeout** and every await carries a 30 s watchdog — 600× the deadlines used
— so nothing client-side can produce a refusal.

**Deep enforcement-point proofs** (these are what iteration 4 adds):

* `crates/chunkstore-fs/tests/conformance.rs:429`
  `the_deadline_verdict_falls_after_the_bytes_are_written_and_before_publication`.
  A `Clock` anchored to the store's own on-disk progress (`AtPublicationPoint`, `:320`):
  `live` while the chunk directory is empty, `late` once the scratch **or** the published
  `.frag` exists, recording `(answer, scratch_present, fragment_present)` per read. Two
  assertions, one per direction: some verdict was taken with the bytes on disk (late enough
  to be a bound), and **no** verdict was ever taken with the fragment published (early enough
  to be honest — i.e. crash-safe). Plus: nothing on disk afterwards, no scratch, and a
  **reopen** of the store (the crash-restart recovery path) still finds nothing.
* `crates/chunkstore-fs/tests/concurrent_put.rs:132`
  `expired_writers_racing_live_ones_refuse_themselves_and_never_remove_the_fragment` —
  64 writers × 16 rounds on one id, alternating live and long-expired, released by a
  `Barrier`. The live writers' fragment must survive every interleaving. This is T3's
  hazard made a test.
* `crates/dst/tests/network.rs:820`
  `a_write_that_expires_inside_the_d_servers_store_is_refused_before_publication` — the
  seeded network case iteration 3's T5 asked for: the parking is **inside** `FsChunkStore`
  (between its data write and its publication), not in front of it, driven over madsim's
  simulated network through the real tonic service and client, with a no-deadline control
  write that lands.
* `crates/dst/tests/network.rs:634` (kept from v3) — 0016's failure-mode row verbatim
  (`0016:1784`): the write parked in the D server's **accept queue** past `W_write` with the
  caller long gone, against the real `FsChunkStore`, plus a same-park no-deadline control.

**Leg F's genuine-backend-fault control** (kept from v3, iteration 2's finding): three
outcomes over the wire from the *same* production store — deadline refusal
(`FAILED_PRECONDITION`), malformed fragment (`INVALID_ARGUMENT`), and a **real `ENOTDIR`**
raised by planting a file where the chunk directory belongs (`INTERNAL`) — asserted mutually
distinct (`crates/chunkstore-grpc/tests/write_deadline.rs:518`). The seam-level version is
`crates/chunkstore-fs/tests/conformance.rs:558`.

---

## 4. Forced self-refutation

**(a) Genuine red?** Yes, three ways, all run through the project's runner
(`./engine/xtask.sh`) or `cargo test` inside `$PDCA_WORKTREE`:

* **Base red (the brief's `Falsifiability` requirement).** Reverted the whole patch, kept
  only the new `crates/chunkstore-grpc/tests/write_deadline.rs`, ran
  `cargo test -p wyrd-chunkstore-grpc --test write_deadline`:
  **5 tests ran, 3 failed, 2 passed.** The crate **compiled** — no build-shaped red. The
  three failures are `expect_err` **assertions** on `put_fragment_raw(...)` returning `Ok`,
  at `write_deadline.rs:397`, `:438`, `:543` — i.e. the base server accepted and stored the
  expired write. The two passes are the *controls* (`a_live_write_within_its_deadline…`,
  `absent_deadline_stores_exactly_as_before_issue_638`), which is exactly right: they assert
  unchanged behaviour. Post-fix: **5/5 green**.
* **Mutation A — delete the publication-point verdict, keep only the entry check.** Red:
  `the_deadline_verdict_falls_after_the_bytes_are_written_and_before_publication`,
  `an_expiring_write_never_disturbs_an_already_published_fragment`,
  `a_write_that_expires_before_the_server_applies_it_is_refused_over_grpc`
  (`round_trip.rs`), and DST property 7. (Restored; all green again.)
* **Mutation B — reinstate iteration 3's shape: rename, then judge, then `remove_file`.**
  Red on the crash-safety clause specifically:
  `no clock read may be taken with the fragment already published … [ClockRead { answer:
  9500, scratch_present: false, fragment_present: false }, ClockRead { answer: 10500,
  scratch_present: false, fragment_present: true }]`. So the new test does not merely pass
  under the new design — it *rejects the design that was rejected*.

**(b) Production path?** Yes. Every leg drives the real `ChunkStoreService`, the real
generated tonic server/client, and the real `FsChunkStore`. Nothing is mocked: the only
injected thing is the store's `Clock`, which is a production constructor
(`FsChunkStore::open_with_clock`, `crates/chunkstore-fs/src/lib.rs:70`) and which *decides
nothing* — `AtPublicationPoint` reports what the store's own directory shows. The `DStore`
fake in `crates/dst/tests/network.rs:118` is a pre-existing sim model, not a stand-in for the
enforcement: both deadline properties there run against the real `FsChunkStore`, and the
fake's doc says so (`:119-128`).

**(c) Fixture includes the fault?** Yes. The expired write is *sent* and *accepted* by the
server, not filtered out before it: leg A sends an already-elapsed deadline with a generous
client timeout; leg B parks the request behind a real delaying `tower`-shaped service so the
deadline elapses after acceptance; DST property 6 parks it in a **detached task** so the
client's abandonment cannot cancel it (mirroring `spawn_blocking`'s uncancellable closure);
DST property 7 lets it expire *inside* the store after its bytes are on disk. Every
refusal-leg is paired with a control in the **same run** (a live write, or a deadline-less
write with an identical park) so "refused everything" cannot pass.

---

## 5. Gate status

`./engine/xtask.sh ci` (the project's own runner, executing in `$PDCA_WORKTREE`) — run to
completion; `typos` and the docs renderer were **present**, not warn-skipped
(`render_site: wrote 98 page(s) … link audit OK`), so the brief's two external dependencies
were satisfied and no NEEDS-HUMAN external-dependency marker is warranted.
`./engine/xtask.sh dst` — 7/7 in `tests/network.rs`, including both deadline properties.
`cargo fmt --all` was run over every touched file (the target's commit hook runs
`fmt --check`; the first CI pass caught three formatting diffs, now fixed), and
`cargo clippy --workspace --exclude wyrd-dst --all-targets` is clean (a `type_complexity`
error in the new concurrency test was fixed with a named alias,
`crates/chunkstore-fs/tests/concurrent_put.rs:115`).

Docs currency (`AGENTS.md:154-157`): the RPC changed, so
`docs/design/architecture/08-crosscutting-concepts.md:106` changed in the same patch, and its
wording was rewritten for the new enforcement point.

## 6. Still open for the human at sign-off

* **Fitness-to-purpose (carried from iteration 3, unchanged):** a mixed-version fleet gets no
  guarantee — an old D server silently ignores `deadline_millis`. Acceptable for Alpha is my
  assumption; a capability exchange is not in this slice.
* **The `rename(2)` residue** is a deliberate, documented engineering position, not an
  oversight. §1 above gives the alternative (a durable publication marker + open-time
  recovery), its ~100-line cost, and why I judged it a worse trade. This is the one place
  where a maintainer might reasonably overrule me.
* `scripts/review-branch` is not available in this environment, so the batched-review gate's
  red could not be reproduced or re-run locally by me either.
