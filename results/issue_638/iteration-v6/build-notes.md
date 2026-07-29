# Build notes — issue #638 / fragment-write-deadline (iteration 6)

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE = ../wyrd.pdca-wt-l1` at **`9120f7a`**
(base advanced since iteration 5's `b0cd199` — `9120f7a` is the merge of PR #645, the #634
`scan_page` seam this bundle declares a file conflict with; the seam widening was re-merged
onto it by hand, `crates/traits/src/lib.rs` `classify`'s doc table and
`crates/chunkstore-grpc/Cargo.toml`'s dev-dep comment being the two conflicting hunks).
All `path:line` citations are against the **patched** worktree unless marked "(base)".

Iteration 6 keeps iteration 5's settled mechanics — the proto field, the seam widening, the
wire path, the 45-file callsite migration — and changes **only** the enforcement semantics
and the rollback, which is where every one of the 8 batch findings landed. §1 is what is new
relative to `iteration-v5/patch.diff`.

---

## 1. The carry-forward, finding by finding

Iteration 5 left two failing gates: **C5-mutants** (3 missed of 46) and **T4-batch-review**
(8 blocking findings). Both are addressed below; C5 is now **0 missed of 47** and every batch
finding is either fixed here or answered in `review-rejected.md`.

### (i) Findings `:401`, `:401`, `:398` — the `PublishedLate` verdict (3 of 8)

Three findings, two distinct complaints, and they converge:

* *"A clock read after `rename` timestamps the later observation rather than the atomic
  publication, so descheduling after a timely rename can falsely classify an in-window write
  as `PublishedLate`."* — **correct, and it was a soundness bug.** The evidence a
  post-publication clock read carries is "the reading I could take was late", never "the
  syscall was late". `PublishedLate` asserted the second.
* *"merely returning `PublishedLate` cannot restore the already-reclaimed evidence"* /
  *"retaining the fragment … fails to enforce `W_write`"* — this demands **retraction**, which
  is rejected on evidence (§4).

**Fix: the outcome now says only what the store knows.** `WriteEffect::PublishedLate` is
replaced by `WriteEffect::Unknown` (`crates/traits/src/lib.rs:892`), produced by
`WriteDeadlineExpired::if_publication_unverified` (`:958`, was `if_published_late`), with
`bytes_landed()` → `may_have_landed()` (`:901`). The comparison, the wire code (`ABORTED`) and
the class (`Indeterminate`) are unchanged — what changed is the *claim*:

| before (v5) | now |
|---|---|
| "the bytes landed, at or after the deadline" | "the store could not verify the publication landed in window; the bytes may or may not be there, in window or not" |

This is exactly `CommitUnknownResult`'s register, and `ErrorClass`'s own doc
(`crates/traits/src/lib.rs:676-680`) is the precedent: the partition is not binary precisely so
a caller is never told a definite thing about durable state the producer does not know. The
strong half of v5's contract survives intact and is **stronger** than iterations 1–4:

> `Ok(())` from `ChunkStore::put_fragment` still means **the fragment was published strictly
> before its deadline**, checked at both ends of the publishing step.

Nothing is weakened: a false `Unknown` (timely rename, descheduled thread) costs a caller a
re-read; a false `PublishedLate` told every consumer an in-window write was a leak. The
negative half is now pinned by assertion — the rendering must **not** contain "landed late"
(`crates/traits/src/lib.rs:1900-1906`, `crates/chunkstore-fs/tests/conformance.rs:706-711`), so a future
"just call it late again" has to delete an assertion.

Renamed everywhere it is user-visible: the proto comment
(`crates/proto/proto/wyrd/v0/chunk.proto:29-61`), the D-server mapping
(`crates/chunkstore-grpc/src/server.rs:123`), the client reconstruction
(`crates/chunkstore-grpc/src/client.rs:172`), the architecture doc
(`docs/design/architecture/08-crosscutting-concepts.md:106`).

### (ii) Findings `:314`, `:315`, `:316` — `create_dir_all` cannot prove ownership (3 of 8)

All three say the same true thing: `create_dir_all` succeeds on a directory that already
exists, so v5's `created_chunk_dir` flag records a **belief**, not a fact, and an expiring
writer could act on it against a racer's directory.

**Fix: delete the flag and the belief.** The rollback now attempts the removal
*unconditionally* and only through `fs::remove_dir`
(`crates/chunkstore-fs/src/lib.rs:194-200`), whose emptiness test the kernel performs
**atomically with the removal**. Ownership stops mattering: a directory holding anything —
another write's scratch, another fragment — cannot be taken, by anyone, ever. `NotFound` and
`DirectoryNotEmpty` are the states the refusal wanted; everything else is a fault (§1(iv)).

Two second-order gains: it also collects an empty directory this write did *not* create (v5
could not), and it deletes state from the hot path (`created_chunk_dir` is gone from
`put_fragment`).

### (iii) Finding `:192` — `entries.next().is_some()` swallows `Some(Err(_))`

Deleted, not patched: the `read_dir` occupancy probe existed only to approximate what
`remove_dir` decides atomically. There is no iterator to mis-read any more.

### (iv) The interference the new removal introduces, and its bound

Removing an empty directory can strip it from under a concurrent live writer that has just
run `create_dir_all` and not yet written its scratch — which would kill a **live** write on
behalf of an **expired** one. So the data write's create-and-retry is now a bounded loop
(`crates/chunkstore-fs/src/lib.rs:303-324`, `CREATE_RETRIES = 2`) instead of the base's single
recovery: one retry per interfering rollback, capped so a `NotFound` from any other cause
surfaces as the I/O error it is rather than spinning.

Measured, not asserted: with the retry bound cut back to the base's single recovery
(`CREATE_RETRIES = 1`) a probe of **2 000 rounds × (1 live + 1 expired) writers on fresh chunk
ids** observed **0** knock-overs — the window is several syscalls wide inside a two-syscall
gap. So the second iteration is a **margin**, not a mask for a common failure, and it is the
one thing in this patch a mutation (`2 → 1`) cannot be made to fail deterministically at this
seam (there is no clock read, and no other hook, between `create_dir_all` and the retry).
`cargo mutants` did **not** generate that mutant (it does not mutate in-fn `const` items), so
C5 is clean; I am recording it here rather than letting it look like coverage I claimed.
Dropping the margin would trade an advisory mutant for a real, if rare, liveness bug — the
wrong direction.

### (v) Finding `network.rs:137` — the DST fake did not mirror the seam

*"checks expiry before acquiring the publication mutex and never rechecks after insertion."*
True, and it is the rubric's *Test fidelity* class. `DStore::put_fragment`
(`crates/dst/tests/network.rs:134-167`) now takes the publication lock **first**, judges under
it, inserts, and re-reads afterwards — the same three-phase shape as `FsChunkStore`, so the sim
model is neither stronger nor weaker than the adapter it mirrors.

### (vi) The three surviving mutants the carry-forward named

| carry-forward mutant | now killed by |
|---|---|
| rollback state check, `chunkstore-fs/src/lib.rs:196` | `conformance.rs:932` `a_refusal_whose_scratch_already_vanished_is_still_a_clean_refusal` |
| rollback state check, `chunkstore-fs/src/lib.rs:198` | `conformance.rs:989` (arm must not narrow) + `conformance.rs:1053` (arm must not widen) |
| ordinary gRPC reconstruction of `ABORTED`, `client.rs:172` | `chunkstore-grpc/tests/round_trip.rs:357` — a **non-madsim** test, which is why the DST leg alone left it surviving (`cargo mutants` never runs `--cfg madsim`) |

`scripts/mutants-in-diff` over this bundle's diff: **47 mutants tested in 37 s: 11 caught,
36 unviable, 0 missed** (was 46 tested / 3 missed).

---

## 2. Design decisions the brief asked me to state

**Deadline, not authorization instant, travels the wire** (`Design` open question 1).
Unchanged from v5 and re-affirmed: the receiver would otherwise need the *sender's* `W_write`
to derive the deadline — a policy value on the wire, silently mismatched in a mixed fleet.
Sending the deadline keeps the D server ignorant of sessions, leases and window sizing (#625
owns the value) and leaves the acceptor one comparison. Field:
`crates/proto/proto/wyrd/v0/chunk.proto:62`, `optional uint64 deadline_millis = 3` — a new tag,
never a renumber, explicit proto3 presence so absent stays absent.

**Which lifecycle owns the clock read.** The **store** — it is the acceptor, and one of the
two evaluation sites `δ_clock` bounds. `FsChunkStore` holds one injected `wyrd_testkit::Clock`
(`crates/chunkstore-fs/src/lib.rs:38-55`, default `SystemClock`) and **all three** reads come
from it: entry (`:274`), publication point (`:361`), publication completed (`:403`). No bare
`SystemTime::now()` is added anywhere, so `clippy.toml`'s wall-clock ban (#619) holds;
`SystemClock` is the seam's sanctioned arm and `coordination-mem` is the precedent for a
production crate depending on `wyrd-testkit` for it (`crates/coordination-mem/Cargo.toml:13`).

**Typed refusal (leg F).** `wyrd_traits::WriteDeadlineExpired` + `WriteEffect` +
`is_write_deadline_expired` / `write_deadline_outcome` (`crates/traits/src/lib.rs:837-1036`)
live in the seam crate for the same reason `IntegrityFault` / `BlockReadFault` /
`ScanCapExceeded` do — every backend raises the *same* type. One shared comparison
(`if_elapsed` / `if_publication_unverified`), inclusive `now >= deadline` to match GC's
inclusive grace test so `G_orphan > W_write + δ_clock` has no tick belonging to neither side.

**Where the gRPC service checks: it doesn't.** It forwards `request.deadline_millis` to the
store and maps the outcome (`crates/chunkstore-grpc/src/server.rs:72-127`). A handler-level
check would bound only when the D server *accepted* the request, and would give an embedded
caller holding `FsChunkStore` directly a weaker guarantee than a networked one — leg E forbids
exactly that.

**Migration of existing callsites.** Source-breaking by construction (the brief mandates it).
Every existing caller and double passes `None`, defined as exactly the pre-#638 behaviour
(`crates/traits/src/lib.rs:1117-1120`) — 45 files, mechanical and uniform. The only non-`None`
production forwarding is `PlacementChunkStore::put_fragment_at`'s default and
`FanoutChunkStore` (`crates/chunkstore-grpc/src/fanout.rs:88-92`, `:164-168`), both forwarding
the deadline **unchanged** — routing a write must never strip its deadline — with the fan-out's
in-crate double recording what it received, so the forwarding is asserted rather than assumed
(`fanout.rs:185-220`, `routing_forwards_the_authorization_deadline_unchanged`).

**Mixed-version fleet** (open question 2). Alpha assumption kept: additive degradation,
documented at `crates/chunkstore-grpc/src/client.rs:259-262`, in the proto comment and in the
architecture doc. A capability exchange is not in this slice. Still a maintainer call.

**`chunkstore-fs` and leg B** (open question 3). The local store cannot "queue", so leg B is
driven over gRPC and in DST. What it *can* exhibit — the deadline elapsing between its own data
write and its publication, and across its publication — is `conformance.rs:430` and `:675`.

---

## 3. Tests, and what each one binds

**The brief-named file — `crates/chunkstore-grpc/tests/write_deadline.rs` (NEW, 5 tests).**
Byte-identical to iteration 5's; it is the file that produces the base red (§4). Every
`PutFragment` is hand-encoded protobuf over a bare `tonic::client::Grpc<Channel>` so the file
**compiles against the base seam** and fails by *assertion*, not by build error. The channel is
dialed with **no timeout** and every await carries a 30 s watchdog — 600× the deadlines used —
so nothing client-side can produce the refusal. Legs A (`:389`), B (`:421`), C (`:466`),
D (`:491`), F (`:520`).

**New this iteration:**

* `crates/chunkstore-grpc/tests/round_trip.rs:357`
  `a_publication_the_server_could_not_verify_reconstructs_as_indeterminate_over_grpc` — the
  `ABORTED` → `Unknown` / `Indeterminate` reconstruction through the **production**
  `GrpcChunkStore`, in an ordinary (non-madsim) test. Also pins the third clock read via
  `SteppedClock::remaining() == 0`.
* `crates/chunkstore-fs/tests/conformance.rs:932`
  `a_refusal_whose_scratch_already_vanished_is_still_a_clean_refusal` — the rollback's job is a
  *state*, not a syscall; an already-absent scratch is that state, so it is not a fault.
* `conformance.rs:989` `a_refusal_leaves_a_chunk_directory_a_concurrent_write_is_using` — a
  sibling fragment is published in the verdict→rollback window; the refusal must come back
  clean, the sibling must survive, its directory must still stand.
* `conformance.rs:1053` `a_chunk_directory_the_rollback_could_not_remove_is_a_backend_fault` —
  the opposite direction: an unexpected `remove_dir` failure must surface, not be swallowed.
  The injection replaces the chunk directory with a **symlink to an empty directory**, which
  fails `rmdir` with `ENOTDIR` while leaving the store's `unlink` of its already-gone scratch a
  clean `NotFound` — so it exercises the *directory* arm and not the scratch arm. uid- and
  permission-independent, the same standard as this file's `ENOTDIR` control; `#[cfg(unix)]`.
* `crates/chunkstore-fs/tests/concurrent_put.rs:232`
  `a_refused_writes_rollback_never_knocks_over_a_live_writer_creating_the_same_chunk` — 64
  writers (live/expired alternating) on **one chunk, distinct indices, a fresh chunk id per
  round**, so every round starts with the directory absent: live writes must all land, expired
  ones must all be the clean verdict, and the listing must hold exactly the live half.
* `crates/traits/src/lib.rs:1881` / `:1914` — the `Unknown` boundary + the negative
  ("must not read as a late landing"), and the two-effect classification rows.

**Kept from iteration 5, updated for the rename:** the publication-point placement proof
(`conformance.rs:430`), the unverified-publication leg (`:675`), the rollback-fault leg
(`:797`), the empty-directory leg (`:1131`), the same-id race (`concurrent_put.rs:131`), and
DST properties 6/7/8 (`crates/dst/tests/network.rs:648`, `:814`, `:959`).

---

## 4. What I did not do, and its cost

**Retracting the bytes of an unverified publication.** Findings `:398`/`:401` ask for it.
Rejected on evidence, recorded in `review-rejected.md`:

* `rename(2)` admits no predicate and cannot be cancelled, so a store can only judge *before*
  it or retract *after* it. Retraction was implemented in **iteration 3** and rejected by
  review on two independent grounds (`iteration-v3/check-review.md`): a crash between rename
  and unlink leaves exactly the bytes the refusal denies, and unlink-by-path can destroy a
  concurrent same-id writer's **already-acknowledged** fragment.
* Making it crash-safe needs a durable publication marker plus open-time recovery: **≈100
  lines** of new production code in the hot write path (≈35 to arm/disarm the marker, ≈40 to
  teach `reap_stale_temps` to resolve markers — it only unlinks `*.tmp` today,
  `crates/chunkstore-fs/src/lib.rs:117-140` — and ≈25 plus a `#[cfg(unix)]` arm for
  inode-precise retraction), **2–3 extra syscalls per deadline-carrying write**, and a new
  hazard class: startup recovery that *deletes published `.frag` files*. It buys one turtle —
  the marker's own confirm step is equally unbounded.
* The trade is explicit: bytes that did land late are garbage their position's `orphan:`
  evidence covers (`0016:1547-1550`, position coverage); an erased live fragment is
  unrecoverable.

**A per-call client-side bound at `deadline_millis`.** Standing rejection carried forward
unchanged (`review-rejected.md`, second row): `results/issue_508/review-rejected.md:10` already
rejected a duplicate caller-side bound on this path, the composition wires
`connect_with_timeout` for every method (`crates/server/src/cli.rs:1441`), and cancelling *at*
the deadline would replace the server's definite verdict with tonic's `CANCELLED` — which the
seam classifies `Transient`, i.e. strictly less information.

---

## 5. Forced self-refutation

**(a) Genuine red?** **Yes — reproduced this iteration, on this base.** Reset the worktree to
`9120f7a`, dropped every production change, left only the new test file in place, and ran the
project's compiler on it:

```
$ cargo test -p wyrd-chunkstore-grpc --test write_deadline      # base seam, no patch
running 5 tests
test a_deadline_refusal_is_distinguishable_from_both_a_client_fault_and_a_disk_fault ... FAILED
test expired_deadline_is_refused_by_the_server_and_never_stored ... FAILED
test a_live_write_within_its_deadline_stores_and_reads_back_byte_identical ... ok
test absent_deadline_stores_exactly_as_before_issue_638 ... ok
test a_write_parked_past_its_deadline_is_refused_when_finally_applied ... FAILED
test result: FAILED. 2 passed; 3 failed
```

**5 tests ran, 3 failed, 2 passed — and the crate COMPILED, so this is not a build-shaped
red.** The three failures are `expect_err` **assertions** at `write_deadline.rs:397`, `:438`
and `:543`: the base server accepted and stored the expired write. The two passes are the
*controls* (a live write; a deadline-less write), which is exactly right — a file that went
fully red would not distinguish "enforces the deadline" from "refuses everything". Post-fix:
**5/5 green.**

Per-hunk mutation, each applied alone and reverted (`cargo test -p wyrd-chunkstore-fs -p
wyrd-chunkstore-grpc -p wyrd-traits`):

| # | mutation | result |
|---|---|---|
| M1 | drop the scratch `NotFound` arm in `restore_pre_write_state` | RED `a_refusal_whose_scratch_already_vanished…` |
| M2 | swallow **every** `remove_dir` error | RED `a_chunk_directory_the_rollback_could_not_remove…` |
| M3 | treat **every** `remove_dir` error as a fault | RED `a_refusal_leaves_a_chunk_directory_a_concurrent_write_is_using`, `an_expiring_write_never_disturbs…` |
| M4 | never remove the chunk directory | RED `repeated_late_writes_to_fresh_chunk_ids…`, `the_deadline_verdict_falls…`, `a_refusal_whose_scratch…` |
| M5 | `let _ = restore_pre_write_state(…)` (the iteration-4 defect verbatim) | RED both rollback-fault legs |
| M6 | delete the post-publication verification | RED `a_publication_the_store_could_not_verify…`, `an_expiring_write_never_disturbs…` |
| M7 | `CREATE_RETRIES = 1` (the base's single recovery) | **survived** — §1(iv): the window is not deterministically forceable at this seam; measured 0/2 000 rounds |
| M8 | client stops reconstructing `ABORTED` | RED `a_publication_the_server_could_not_verify_reconstructs_as_indeterminate_over_grpc` |

**(b) Production path?** **Yes.** Every leg drives the real `ChunkStoreService`, the real
generated tonic server/client and the real `FsChunkStore`. Nothing is mocked. The only injected
thing is the store's `Clock`, through a **production** constructor (`FsChunkStore::open_with_clock`,
`crates/chunkstore-fs/src/lib.rs:70`), and in the placement legs it *decides nothing*:
`AtPublicationPoint` / `AtPublicationCompletion` / `AtAnyScratch` report what the store's own
directory shows. `AtVerdictInject` additionally injects a **filesystem state** — that is fault
injection, not a substitute for the code under test. The `DStore` fake
(`crates/dst/tests/network.rs:91`) is a pre-existing sim model and now mirrors the production
seam's shape; all three DST deadline properties run against the real `FsChunkStore`.

**(c) Fixture includes the fault?** **Yes.** The expired write is *sent* and *accepted* by the
server, never filtered out before it: leg A sends an already-elapsed deadline with a generous
client timeout; leg B parks the request behind a real delaying service so the deadline elapses
after acceptance; DST property 6 parks it in a **detached** task so the client's abandonment
cannot cancel it (mirroring `spawn_blocking`'s uncancellable closure); property 7 lets it expire
*inside* the store after its bytes are on disk; property 8 lets publication itself go
unverified. Every refusal leg is paired with a control in the **same run** (a live write, or a
deadline-less write with an identical park/clock), so "refused everything" cannot pass. The
rollback-fault legs assert the residue is *present*, so they cannot pass on a store that
silently cleaned up.

---

## 6. Gate status

`./engine/xtask.sh ci` (the project's own runner, executing in `$PDCA_WORKTREE`) — run to
completion over the final tree: **`xtask ci: all checks passed`**. The prose gates were
**present**, not warn-skipped (`$ typos` ran; `render_site: wrote 98 page(s) … link audit OK`),
so the brief's two external dependencies were satisfied and **no NEEDS-HUMAN
external-dependency marker is warranted**. `./engine/xtask.sh dst` — green, 8/8 in
`tests/network.rs` including all three deadline properties. `scripts/mutants-in-diff` — 47
tested, **0 missed**. `cargo fmt --all` was run over every touched file (the target's commit
hook runs `fmt --check`); clippy is clean for the workspace and, under `RUSTFLAGS=--cfg
madsim`, for `wyrd-dst`.

Docs currency (`AGENTS.md:154-157`): the RPC changed, so
`docs/design/architecture/08-crosscutting-concepts.md:106` changed in the same patch.

Self-review against `AGENTS.md` § Review rubric: **one clock per lifecycle** (§2 — three reads,
one injected source); **await discipline** (every test await watchdog-bounded; the DST dials use
`connect_with_timeout`; the client-side per-call bound stays recorded-rejected);
`#![forbid(unsafe_code)]` untouched (the one new platform call, `std::os::unix::fs::symlink`, is
safe); no DST-reachable shared mutable global (statics gate green); *Absent or unsupported
entries* — every rollback failure is an explicit error, on the scratch **and** on the shared
directory; *Test fidelity* — the sim model now mirrors the adapter's three-phase seam, and the
new destructive path lands with seeded DST coverage; *Serialization identity* — the proto field
is `optional`, so absent stays absent on the wire (leg D asserts it over the wire).

## 7. Still open for the human at sign-off

* **Fitness-to-purpose (carried, unchanged):** a mixed-version fleet gets no guarantee — an old
  D server silently ignores `deadline_millis`. Alpha-acceptable is my assumption; a capability
  exchange is not in this slice.
* **The publication syscall's own duration is not preventable, only reported.** §4 gives the
  alternative, its ≈100-line cost and the new startup-deletion hazard it introduces. What
  changed this iteration is that the report no longer over-claims: the store says "I could not
  verify", which is what its evidence supports, instead of "it landed late", which it never
  established. This is the one place a maintainer might reasonably overrule me.
* **The `CREATE_RETRIES = 2` margin** (§1(iv)) is one branch that no deterministic test at this
  seam can force. Measured cost of removing it: a live write can, in principle, be failed by a
  racing refusal's rollback; measured frequency at `= 1`: 0 in 2 000 rounds.
* `scripts/review-branch` is not available in this environment, so the batched-review gate's red
  could not be reproduced or re-run locally by me. Six of the eight findings are **fixed** (they
  should leave the next run); the two that remain are answered in `review-rejected.md` in the
  format the triage reads.
