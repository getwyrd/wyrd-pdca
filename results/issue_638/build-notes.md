# Build notes — issue #638 / fragment-write-deadline (iteration 7)

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE = ../wyrd.pdca-wt-l1` at **`9120f7a`**
(the merge of PR #645 — the #634 `scan_page` seam this bundle declares a file conflict with).
All `path:line` citations are against the **patched** worktree unless marked "(base)".

Iteration 7 keeps the settled mechanics of iterations 5–6 — the proto field, the seam
widening, the wire path, the mechanical callsite migration, the typed refusal, the two-ended
enforcement — and changes **only** the three things the carry-forward names. §1 is what is new
relative to `iteration-v6/patch.diff`; everything after it is the standing record a sign-off
reader needs in one place.

---

## 1. The carry-forward, finding by finding

Iteration 6 left one failing gate: **T4 batch review**, 9 findings. Eight are *fixed* below;
the ninth (`src/lib.rs:367` — "the unconditional rename can be delayed … `Unknown` cannot
prevent an arbitrarily late fragment from landing") is the retraction demand, recorded-rejected
for the fourth time in `review-rejected.md`, now also at the `rename` line itself so the same
row matches wherever the finding lands.

### (i) **Primary** — the `CREATE_RETRIES = 2` margin (3 of 9 findings)

> *"Two retries are insufficient when three staggered writes expire after admission and
> successively remove the shared directory, allowing a valid concurrent write to fail with
> `NotFound`."* — and the sign-off's instruction: **close the race deterministically or land a
> test that genuinely forces it; do not re-treat the margin as settled by measurement.**

**The margin is gone, because the write that made it necessary is gone.** Iteration 6's refusal
rollback removed the *shared* chunk directory (`remove_dir`, atomic in emptiness) and then paid
for that with a bounded create-retry loop in the live write path. Both are deleted. The refusal
now removes **exactly one path: its own private scratch** — `<index>.<seq>.tmp`, a name no other
write can name (`crates/chunkstore-fs/src/lib.rs:97-109`, issue #203's per-call scratch):

* `restore_pre_write_state` takes one argument and performs one syscall
  (`crates/chunkstore-fs/src/lib.rs:182-216`);
* the data write is back to the base's single create-recovery
  (`crates/chunkstore-fs/src/lib.rs:324-339`; base shape at `9120f7a:crates/chunkstore-fs/src/lib.rs:209-224`).

The reviewer's arithmetic — *N retries lose to N+1 staggered refusals* — is exactly why a margin
was the wrong instrument: it makes the failure rarer, never impossible. **A refusal that writes
no shared path cannot interfere with any live write, at any concurrency, under any
interleaving.** That is a structural statement, not a measured one, and it is now pinned by a
*deterministic* test rather than by a thread race (§3, `a_refusal_never_removes_the_chunk_directory_even_one_it_created`).

**What leaving the directory costs, and why that is the right side of the trade.** A refused
write to a chunk the store has not seen leaves an **empty** `<32-hex>` directory. It is not a
fragment: `get_fragment` reads `None` through it, `list_fragments` cannot see it (it parses
`.frag`), so nothing is observable through the seam — and the base *already* leaves exactly this
residue on three pre-existing paths: a failed data write, a failed rename
(`9120f7a:crates/chunkstore-fs/src/lib.rs:215-228`), and `delete_fragment` reclaiming a chunk's
last fragment (`:306-318`), which GC does on every reclaimed chunk. So the residue class is
pre-existing and this slice is not its main producer.

It is nonetheless collected, at the one place where collection **cannot race a live write**:
`open`, where one D server owns the root (ADR-0034, Model A) and no write of this store is in
flight — the same argument that already licenses reaping stale scratch there. `reap_stale_temps`
becomes `reap_write_residue` and, after unlinking a chunk's scratch, attempts `fs::remove_dir`
on the directory (`crates/chunkstore-fs/src/lib.rs:111-162`, the sweep itself at `:156-160`). `remove_dir`, never
`remove_dir_all`: the kernel's atomic emptiness test decides, so a chunk still holding a
fragment keeps its directory — asserted at `crates/chunkstore-fs/tests/conformance.rs:1216-1227`.
Net effect versus the base: the store now collects **more** residue than before this slice, from
*all three* pre-existing producers as well as from refusals.

**Cost of the alternatives, in checkable numbers:**

| option | production lines (countable in the diff) | residual hazard |
|---|---|---|
| **taken** — refusal touches no shared path; collect empty dirs at `open` | **−7** (the `remove_dir` arm and its `Option<&Path>` parameter leave `restore_pre_write_state`), **−12** (the `const CREATE_RETRIES` + retry loop, replaced by the base's own 16-line create-recovery `match`, so `put_fragment` returns to its pre-#638 shape), **+1** syscall line (+4 of comment) at `crates/chunkstore-fs/src/lib.rs:156-160` | empty directories persist within one server lifetime; invisible through the seam, and collected at the next `open` |
| iteration 6 — remove the shared dir, retry the create | those 19 lines | **a live write fails `NotFound`** under N+1 staggered refusals (the finding) |
| serialize with a per-chunk lock | ≈35: `chunk_locks: Mutex<HashMap<ChunkId, Arc<Mutex<()>>>>` field (1) + init (1) + `lock_chunk` helper with doc (≈12) + acquire around `create_dir_all`+`fs::write` (≈5) + acquire in the rollback (≈4) + refcounted eviction so the map does not grow one entry per chunk id for ever (≈12) | a **new store-wide lock on the hot write path**, and it still serializes only *this process* — it rests on the very ADR-0034 single-owner assumption that already makes the open-time sweep safe, so it buys nothing the sweep does not, at the price of contention |

### (ii) **Coupled** — the regression that never reached the rollback (3 of 9 findings)

> *"The expired writers use deadlines already elapsed at admission, so they never create scratch
> files or execute rollback and the claimed shared-directory race is not exercised."*

Correct, and it applied to **both** deadline races in that file, not only the one reported. The
fix is a fixture that can express post-admission expiry under concurrency:
`PerWriterClock` (`crates/chunkstore-fs/tests/concurrent_put.rs:124-205`) keys the timeline by
the **writing thread**, so each writer scripts its own — `(10_000, 30_000)` against a 20_000
deadline is *admitted, then late once its bytes are on disk*, whatever the interleaving. A
store-wide `ManualClock` cannot express it (one fixed reading makes every "expired" writer
expired on arrival) and a store-wide `SteppedClock` cannot either (racing threads consume one
another's readings).

And the fixture asserts its **own** fidelity, per writer and per round: `reads == 2` — admitted
(reading 1), refused at the publication point with its scratch on disk (reading 2). One reading
would mean refused at the door, which is precisely the vacuous state the finding caught; that
assertion is what makes it fail instead of pass (`concurrent_put.rs:296-303`, `:421-428`).
Verified red by mutation M-C in §5.

The two races are now complementary rather than duplicated: `writers_expiring_after_admission_…`
(`:231`) races on **one `FragmentId`** (the data-loss hazard of a publish-then-retract design),
`a_rollback_next_to_a_live_writer_creating_the_same_chunk_…` (`:353`) races on **one chunk
directory with distinct fragment indices** (the container hazard), each round on a **fresh chunk
id** so the directory starts absent — the only state in which the create/remove window exists.

### (iii) **Secondary** — `WriteEffect::Unknown` was told to re-authorize (2 of 9 findings)

Fixed at the cause: the remedy is now read off the **effect**, not stamped on every outcome
(`crates/traits/src/lib.rs:993-1019`). `NotApplied` → "re-authorize and write again, do not
retry this authorization"; `Unknown` → "re-read to establish what landed before counting this
write either way", which is `CommitUnknownResult`'s own register (`:212-216`). Both directions
are asserted in one test so neither can be collapsed back into a single suffix
(`crates/traits/src/lib.rs:1928-1951`); mutation M-A in §5 shows it red.

### (iv) One thing the reviewers did not ask for, and why it is here

The **seam contract** now states the rule normatively, so it binds every backend and not just
`chunkstore-fs` (brief leg E): container state shared with concurrent writes "MUST NOT be
removed by the refusal … N retries lose to N+1 racing refusals … an implementation that collects
empty containers does it where no write of that store is in flight"
(`crates/traits/src/lib.rs:1116-1128`). Iteration 6's clause permitted the removal if it was
"atomic in the shared state's emptiness", which is exactly the permission that produced the
finding: atomicity prevents *data* loss, not *liveness* loss.

---

## 2. Design decisions the brief asked me to state

**Deadline, not authorization instant, travels the wire** (Design open question 1). The receiver
would otherwise need the *sender's* `W_write` to derive the deadline — a policy value on the
wire, silently mismatched in a mixed fleet. Sending the deadline keeps the D server ignorant of
sessions, leases and window sizing (#625 owns the value) and leaves the acceptor one comparison.
Field: `crates/proto/proto/wyrd/v0/chunk.proto:62`, `optional uint64 deadline_millis = 3` — a new
tag, never a renumber, explicit proto3 presence so absent stays absent.

**Which lifecycle owns the clock read.** The **store** — it is the acceptor, and one of the two
evaluation sites `δ_clock` bounds. `FsChunkStore` holds one injected `wyrd_testkit::Clock`
(`crates/chunkstore-fs/src/lib.rs:38-55`, default `SystemClock`) and **all three** reads come
from it: entry (`:288`), publication point (`:373`), publication completed (`:417`). No bare
`SystemTime::now()` is added anywhere, so `clippy.toml`'s wall-clock ban (#619) holds;
`SystemClock` is the seam's sanctioned arm and `coordination-mem` is the precedent for a
production crate depending on `wyrd-testkit` for it (`crates/coordination-mem/Cargo.toml:13`).

**Typed refusal (leg F).** `wyrd_traits::WriteDeadlineExpired` + `WriteEffect` +
`is_write_deadline_expired` / `write_deadline_outcome` (`crates/traits/src/lib.rs:837-1050`) live
in the seam crate for the same reason `IntegrityFault` / `BlockReadFault` / `ScanCapExceeded` do
— every backend raises the *same* type. One shared comparison (`if_elapsed` /
`if_publication_unverified`), inclusive `now >= deadline` to match GC's inclusive grace test so
`G_orphan > W_write + δ_clock` has no tick belonging to neither side.

**Where the gRPC service checks: it doesn't.** It forwards `request.deadline_millis` to the store
and maps the outcome (`crates/chunkstore-grpc/src/server.rs:72-127`). A handler-level check would
bound only when the D server *accepted* the request, and would give an embedded caller holding
`FsChunkStore` directly a weaker guarantee than a networked one — leg E forbids exactly that.

**Migration of existing callsites (leg D).** Source-breaking by construction (the brief mandates
it). Of the patch's 54 files, **38 are pure callsite migration** (64 added `None` arguments and
nothing else); every existing caller and double now passes `None`, defined as exactly
the pre-#638 behaviour (`crates/traits/src/lib.rs:1143-1146`) — mechanical and uniform, no
behaviour change. The only non-`None` production forwarding is `PlacementChunkStore::
put_fragment_at`'s default and `FanoutChunkStore` (`crates/chunkstore-grpc/src/fanout.rs:90-92`,
`:167-168`), both forwarding the deadline **unchanged** — routing a write must never strip its
deadline — with the fan-out's in-crate double recording what it received (`fanout.rs:183-188`),
so the forwarding is asserted rather than assumed
(`fanout.rs:457` `routing_forwards_the_authorization_deadline_unchanged`).

**Mixed-version fleet** (open question 2). Alpha assumption kept: additive degradation,
documented at `crates/chunkstore-grpc/src/client.rs:259-262`, in the proto comment and in the
architecture doc. A capability exchange is not in this slice. Still a maintainer call.

**`chunkstore-fs` and leg B** (open question 3). The local store cannot "queue", so leg B is
driven over gRPC and in DST. What it *can* exhibit — the deadline elapsing between its own data
write and its publication, and across its publication — is `conformance.rs:430` and `:698`.

---

## 3. Tests, and what each one binds

**The brief-named file — `crates/chunkstore-grpc/tests/write_deadline.rs` (NEW, 5 tests).**
Unchanged since iteration 5; it is the file that produces the base red (§5). Every `PutFragment`
is hand-encoded protobuf over a bare `tonic::client::Grpc<Channel>` so the file **compiles
against the base seam** and fails by *assertion*, not by build error. The channel is dialed with
**no timeout** and every await carries a 30 s watchdog — 600× the deadlines used — so nothing
client-side can produce the refusal. Legs A (`:389`), B (`:421`), C (`:466`), D (`:491`),
F (`:520`).

**New or reworked this iteration** (all in `chunkstore-fs`, where the change lives):

* `tests/conformance.rs:1076` `a_refusal_never_removes_the_chunk_directory_even_one_it_created` —
  **the deterministic replacement for the un-forceable race.** A refusal on a *fresh* chunk (the
  directory did not exist, so this write created it — the case a "remove what I created" rollback
  would claim) must leave the directory standing and empty, and the store must be
  indistinguishable through the seam from never having seen the write. Red the moment anyone puts
  the shared write back (§5, M-B).
* `tests/conformance.rs:490-527` — the same property inside the placement leg, plus the reopen:
  the empty directory is collected at `open`.
* `tests/conformance.rs:1159` `the_empty_directories_late_writes_leave_are_collected_at_the_next_open`
  — 8 late writes to fresh chunk ids leave 8 empty directories and **no fragment, no scratch**;
  the next `open` collects exactly those and **not** the occupied one, whose fragment still reads
  back. Pins both halves of the sweep (litter bounded / bytes never taken).
* `tests/concurrent_put.rs:231`, `:353` — the two races above, with the `reads == 2`
  fidelity assertion.
* `crates/traits/src/lib.rs:1928-1951` — the per-effect remedy, both directions.
* `tests/conformance.rs:955` (`…scratch_already_vanished…`) and `:1016`
  (`…chunk_directory_a_concurrent_write_is_using`) updated to the new postcondition.
* **Deleted:** `a_chunk_directory_the_rollback_could_not_remove_is_a_backend_fault`. It asserted
  the fault arm of a `remove_dir` that no longer exists on the refusal path. The scratch arm — the
  one that *does* exist — keeps both its directions: fault (`conformance.rs:820`) and benign
  already-gone (`:955`).

**Kept unchanged:** the publication-point placement proof (`conformance.rs:430`), the unverified
publication leg (`:698`), the wire reconstruction legs (`chunkstore-grpc/tests/round_trip.rs:227`,
`:287`, `:357`), and DST properties 6/7/8 (`crates/dst/tests/network.rs:648`, `:814`, `:959`).

---

## 4. What I did not do, and its cost

**Retracting the bytes of an unverified publication.** Rejected for the fourth time, on unchanged
evidence, recorded in `review-rejected.md` with the ≈100-line crash-safe alternative costed line
by line and its new startup-deletion hazard named. Summary: `rename(2)` admits no predicate and
cannot be cancelled, so a store can only judge *before* it or retract *after* it; retraction was
implemented in iteration 3 and rejected on two independent grounds (crash between rename and
unlink leaves exactly the bytes the refusal denies; unlink-by-path can destroy a concurrent
same-id writer's already-acknowledged fragment).

**A per-call client-side bound at `deadline_millis`.** Standing rejection carried forward
unchanged (`review-rejected.md`, last row): #508 already rejected a duplicate caller-side bound on
this path, the composition wires `connect_with_timeout` for every method, and cancelling *at* the
deadline would replace the server's definite verdict with tonic's `CANCELLED` — classified
`Transient`, i.e. strictly less information.

**Forcing the directory race by scheduling pressure.** I tried, with iteration 6's code restored:
64 writers × 400 rounds at a 7-expiring-to-1-live ratio, ≈25 600 writes, **0 knock-overs**
(consistent with iteration 6's own 0/2 000 measurement). The window is two syscalls wide inside
one live writer. That is *why* the sign-off's "or land a test that genuinely forces it" is not
the branch I took: the hazard is real but not reachable by scheduling at this seam, so the only
honest way to close it is to remove the shared write and assert its absence deterministically —
which is what M-B in §5 does.

---

## 5. Forced self-refutation

**(a) Genuine red?** **Yes — reproduced on this iteration's tree.** Reset to `9120f7a`, stashed
every production and test change, left only the new file in place, and ran it through the
project's compiler:

```
$ cargo test -p wyrd-chunkstore-grpc --test write_deadline      # base seam, no patch
running 5 tests
test expired_deadline_is_refused_by_the_server_and_never_stored ... FAILED
test a_deadline_refusal_is_distinguishable_from_both_a_client_fault_and_a_disk_fault ... FAILED
test absent_deadline_stores_exactly_as_before_issue_638 ... ok
test a_live_write_within_its_deadline_stores_and_reads_back_byte_identical ... ok
test a_write_parked_past_its_deadline_is_refused_when_finally_applied ... FAILED
test result: FAILED. 2 passed; 3 failed
```

**5 tests ran, 3 failed, 2 passed — and the crate COMPILED, so this is not a build-shaped red.**
The three failures are `expect_err` **assertions** at `write_deadline.rs:397`, `:438` and `:543`
(each reports `: ()` — the base server accepted *and stored* the expired write). The two passes
are the controls (a live write; a deadline-less write), which is exactly right: a file that went
fully red would not distinguish "enforces the deadline" from "refuses everything". Post-fix:
**5/5 green.**

Per-hunk mutation of **this iteration's** changes, each applied alone and reverted:

| # | mutation | result |
|---|---|---|
| M-A | collapse the remedy suffix back to the single "re-authorize" string | RED `a_publication_the_store_could_not_time_is_reported_as_unknown_not_as_late` (`traits/src/lib.rs:1934`) |
| M-B | restore iteration 6's shared-directory removal in the rollback | RED ×3: `a_refusal_never_removes_the_chunk_directory_even_one_it_created`, `the_deadline_verdict_falls_after_the_bytes_are_written_and_before_publication`, `the_empty_directories_late_writes_leave_are_collected_at_the_next_open` |
| M-C | script the racers as expired *at admission* (iteration 6's fixture verbatim) | RED ×2, both races, on the fidelity assertion: "one reading means it was turned away at admission and never rolled anything back" |
| M-D | drop the `remove_dir` from the open-time sweep | RED ×2: the collection leg and the placement leg's reopen assertion |

Plus the iteration-5/6 mutation set (rollback arms, post-publication verification, client
`ABORTED` reconstruction) which the kept tests still cover: `scripts/mutants-in-diff` over this
bundle's final diff — **45 mutants tested: 10 caught, 35 unviable, 0 missed** (iteration 6's
one surviving-by-construction mutant, `CREATE_RETRIES 2 → 1`, is gone with the constant).

**(b) Production path?** **Yes.** Every leg drives the real `ChunkStoreService`, the real
generated tonic server/client and the real `FsChunkStore`. Nothing is mocked. The only injected
thing is the store's `Clock`, through a **production** constructor
(`FsChunkStore::open_with_clock`, `crates/chunkstore-fs/src/lib.rs:70`), and in the placement legs
it *decides nothing*: `AtPublicationPoint` / `AtAnyScratch` report what the store's own directory
shows. `PerWriterClock` likewise only answers "what time is it for this writer" — the store's own
code decides admission, verdict and rollback. The `DStore` fake (`crates/dst/tests/network.rs:91`)
is a pre-existing sim model that mirrors the production seam's three-phase shape; all three DST
deadline properties run against the real `FsChunkStore`.

**(c) Fixture includes the fault?** **Yes, and this iteration is where that got fixed.** The old
concurrency fixture excluded the very thing it claimed to test — its expired writers were turned
away at admission and never rolled back. Now every expired writer is *admitted*, writes its
scratch, and rolls back next to a live writer, and each one **asserts** it took the second clock
reading that proves it (`concurrent_put.rs:296`, `:421`); M-C shows the assertion is what fails
when the fault is curated out. Elsewhere: leg A sends an already-elapsed deadline with a generous
client timeout; leg B parks the request behind a real delaying service so the deadline elapses
after acceptance; DST property 6 parks it in a **detached** task so the client's abandonment
cannot cancel it; property 7 lets it expire *inside* the store after its bytes are on disk;
property 8 lets publication itself go unverified. Every refusal leg is paired with a control in
the **same** run (a live write, or a deadline-less write with an identical park/clock), so
"refused everything" cannot pass. The rollback-fault leg asserts the residue is *present*, so it
cannot pass on a store that silently cleaned up.

---

## 6. Gate status

`./engine/xtask.sh ci` (the project's own runner, executing in `$PDCA_WORKTREE`) — run to
completion over the final tree: **`xtask ci: all checks passed`** (exit 0). The prose gates were
**present**, not warn-skipped (`$ typos` ran; `render_site: wrote 98 page(s) … link audit OK`), so
the brief's two external dependencies (`typos`, `docs-renderer`) were satisfied and **no
NEEDS-HUMAN external-dependency marker is warranted**. `./engine/xtask.sh dst` — green, 8/8 in
`tests/network.rs` including all three deadline properties. `scripts/mutants-in-diff` — 45 tested,
**0 missed**, re-run over the final tree. `cargo fmt --all` was run over every touched file and `cargo fmt --all -- --check`
is clean (the target's commit hook runs it); clippy is clean for the workspace and, under
`RUSTFLAGS=--cfg madsim`, for `wyrd-dst`.

Docs currency (`AGENTS.md:154-157`): the RPC changed, so
`docs/design/architecture/08-crosscutting-concepts.md:106` changed in the same patch — including
the shared-container clause, which now records the new rule rather than the old one.

Self-review against `AGENTS.md` § Review rubric: **one clock per lifecycle** (§2 — three reads,
one injected source); **await discipline** (every test await watchdog-bounded; the DST dials use
`connect_with_timeout`; the client-side per-call bound stays recorded-rejected);
`#![forbid(unsafe_code)]` untouched and unbroken in the new test code; no DST-reachable shared
mutable global (statics gate green; `PerWriterClock`'s map is per-test, not a static); *Absent or
unsupported entries* — the rollback's failure is an explicit error, never a swallowed cleanup;
*Test fidelity* — the sim model mirrors the adapter's seam and the destructive concurrent path
lands with seeded DST coverage; *Serialization identity* — the proto field is `optional`, so
absent stays absent on the wire (leg D asserts it over the wire).

## 7. Still open for the human at sign-off

* **Fitness-to-purpose (carried, unchanged):** a mixed-version fleet gets no guarantee — an old D
  server silently ignores `deadline_millis`. Alpha-acceptable is my assumption; a capability
  exchange is not in this slice.
* **The publication syscall's own duration is not preventable, only reported.** §4 and
  `review-rejected.md` give the alternative, its ≈100-line cost and the new startup-deletion
  hazard. The store says "I could not verify", which is what its evidence supports — and now tells
  the caller to re-read. This is the one place a maintainer might reasonably overrule me.
* **The empty chunk directory a refusal leaves** is collected at `open`, not at the refusal. Within
  one server lifetime the residue is one empty inode per fresh chunk id that a late write touched,
  invisible through the seam. I judged that strictly better than a hot-path race that fails live
  writes; a maintainer who wants prompt collection would need the per-chunk lock costed in §1(i)
  (≈35 lines and a write-path lock) — and it still would not cover a second process.
* **`scripts/review-branch` could not be run here**, so the batched-review gate's red could not be
  reproduced locally by me. Eight of the nine findings are **fixed** (they should leave the next
  run); the ninth is answered in `review-rejected.md` in the format the triage reads.
