# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 7)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `d50f0ca` (= `origin/main`, the brief's target
base). Line citations are **post-patch** lines in that worktree unless marked "pre-fix" / "on
base" (which means `origin/main`).

---

## 1. Round 7 in one paragraph

Round 6's carry-forward named **one** blocking finding, reported at two lines
(`crates/core/src/metadata.rs:2212` and `:2169`): the paged `high_water_marks` walk gets no
namespace snapshot, so a concurrent legacy writer can commit an `inode:` key *behind* the
cursor, recovery under-seeds the allocator, and the allocator can then re-mint that still-live
inode id. **Fixed** — but not where the finding pointed, and §2 is the argument for why. The
walk cannot close it (the seam's contract refuses snapshot isolation to *every* backend, and the
peer keeps allocating after any walk ends), so the re-mint is made impossible at the point of
use instead: `cli::alloc_inode` now guards each id with `require_absent(inode:<id>)` **inside
the commit that reserves it**. Two new acceptance tests bind it, and the second one shows the
pre-fix behaviour is not a conflict but **silent data loss** — a read of the peer's object
returning a different object's bytes.

Everything the previous rounds' sign-offs accepted is kept unchanged: the paged `for_each_page`
walk, totality over damaged `inode:` rows and over a damaged `meta:next_inode`, the bounded seed
retry, the canonical-decimal grammars, the removal of the dead chunk-id mark with its two
`scan`s and its standing test, and the one-clause docs deletion.

## 2. The finding, and why the fix is not in the walk

**The finding is real.** `inode_key` is `format!("inode:{id}")` (`crates/core/src/metadata.rs:34`)
— unpadded decimal — so byte order is not id order: `inode:100` sorts *before* `inode:99`. A
peer allocating monotonically therefore writes **behind** a lexicographic cursor as a matter of
course (any 9→10, 99→100 digit-length crossing). And `scan_page` says so in the contract, in
capitals: "keys inserted before the cursor after it passed, or deleted mid-walk, may be missed
or duplicated, and **no snapshot isolation is required of any backend**"
(`crates/traits/src/lib.rs:1058-1066`).

**Three ways to close it, and what each costs.**

| Option | Cost | Verdict |
|---|---|---|
| **(a) Make the walk snapshot-consistent.** | Not a caller-side change at all: it is a new clause on `MetadataStore::scan_page`, binding all **five** backends (redb, TiKV, FDB, mem, testkit) plus the shared `wyrd-metadata-conformance` suite. The brief forbids any ADR / spec / conformance-vector change ("Out of scope"), and the seam's own doc explains the asymmetry is what "keeps this implementable on redb, FoundationDB and TiKV alike". | **Rejected** — out of scope, and it is the seam's decision, not this walk's. |
| **(b) Re-walk until stable.** | +~12 lines. A second pass catches pass 1's inserts and is open to its own; "until stable" does not terminate against a writer that does not stop, which is precisely the peer this fixes. Round 6 rejected apparatus that is "more often right and never provably right" — this is that. | **Rejected** — unbounded, and still not a proof. |
| **(c) Guard the id at the point of use.** | +14 production lines in `alloc_inode` (`crates/server/src/cli.rs:1849-1900`), **zero** extra round trips on the happy path (the guard rides in the commit the allocator already makes), one extra `get` only on a `Conflict` — a path the pre-#652 allocator already had. | **Taken.** |

The decisive argument for (c) is not cost, it is that **(a) and (b) do not actually close the
hazard.** The peer in this scenario is a legacy gateway allocating from its own in-memory
counter; it never reads `meta:next_inode`. It keeps allocating *after* the walk ends. A floor
derived from stored keys is therefore stale the instant it is computed — a perfect snapshot
would shrink the window from "during the walk" to "after the walk" and leave the same defect.
Only a check at hand-out time is stable under that, and (c) is stable under *all* of it: it
makes a low floor cost compare-and-sets instead of a live object.

That is also why this is the smallest change that **restores the invariant** rather than the
smallest diff (`docs/principles.md` §1.2, §2). The brief's invariant text is "a silently low
floor lets the allocator re-mint a still-live id and clobber a committed object's fragments".
(c) removes the *consequence clause* outright: the allocator cannot re-mint a still-live id, so
the floor is demoted from a safety property to a starting point — which is what it can honestly
be.

## 3. The one place the guard is deliberately **not** applied — and the test that forced it

`alloc_inode` guards the id only when the counter is a **stored** value
(`reserved`, `crates/server/src/cli.rs:1826-1831`, used at `:1855` and `:1865`). With an
**absent** counter it behaves exactly as on base.

I did not choose that from first principles — I chose it, then the existing suite chose it for
me, and the second reason is the stronger one:

* **Principled reason.** An absent counter reserves nothing: `1` is `alloc_inode`'s own default
  for a store whose allocator was never seeded (`ABSENT_COUNTER_NEXT_ID`). A record already
  holding it means startup recovery has not run at all. Correcting *that* by stepping climbs a
  legacy namespace of N inodes one compare-and-set at a time **on the write path**, where
  `Gateway::recover` covers the same ground in one paged walk before the gateway serves
  anything.
* **The suite's reason (this is the load-bearing one).** I first wrote the guard unconditionally
  and ran the suite. Two existing tests broke, in opposite directions:
  * with **step-past**, `s3_http_wire::recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`
    leg (1) fails — it asserts that *without* `recover()` a legacy-store PUT collides, which is
    the brief's **success criterion 4** ("still passes unchanged");
  * with **fail-closed**, `gateway_lease_expiry::losing_put_leaves_a_future_dated_lease`
    (`crates/server/tests/gateway_lease_expiry.rs:123-140`) fails — it forces a losing PUT by
    pre-occupying `inode:1` over a counter-less store and then reads the *pending lease* the
    losing PUT left behind. Failing earlier means no lease is ever written.

  Leaving the absent case alone is the only shape that keeps both, and it is also a strict
  non-change: absent-counter behaviour is byte-for-byte what base does. Both tests pass
  unmodified; **no existing test was edited or deleted in this round.**

## 4. What actually changed (delta on round 6)

| File | Change |
|---|---|
| `crates/server/src/cli.rs:1778-1798` | `alloc_inode` doc: the guard, and the exact gap it contains, with the seam citation. |
| `crates/server/src/cli.rs:1826-1831` | `reserved` — a stored counter vs. this function's default for an absent one. |
| `crates/server/src/cli.rs:1849-1900` | The guard: `require_absent(inode_key(id))` in the reserving commit; on `Conflict`, one `get` decides *which* precondition failed; an occupied id is attributed on `RECOVERY_AUDIT` and the counter stepped past it (bounded by the existing `ALLOC_INODE_BUDGET`, no backoff spent — it is progress, not contention). |
| `crates/server/src/cli.rs:1729-1741` | `next_inode_cas` — the counter's compare-and-set guard, single-sourced; `seed_next_inode_floor` (`:1998`) now shares it (net −4 lines of duplication). |
| `crates/server/src/cli.rs:1964-1974` | `seed_next_inode_floor` doc: what `floor` is and is not. |
| `crates/core/src/metadata.rs:2219-2247` | `high_water_marks` doc: the new "What a page does NOT give: a namespace snapshot" section — the finding, why the walk cannot close it, and where it *is* closed. The adjacent "stopping early" sentence (`:2255-2261`) is corrected, since it claimed the walk's floor was above everything stored. |
| `crates/server/src/lib.rs:146-154` | `recover` doc: "what the seeded counter is worth". |
| `crates/server/tests/gateway_recover_totality.rs` | Two new tests + the `WriterBehindCursorStore` double + three helpers (`capturing`, `all_rows`, `delete_rows`, `cluster_put`); `recover_capturing` is now a two-line wrapper over `capturing`. |

No production line outside `alloc_inode` changed behaviour. `high_water_marks`, `for_each_page`,
`seed_next_inode_floor`'s body, `parse_inode_key`, `read_persisted_inode_counter` are round 6's,
unchanged.

## 5. The two new tests

**`an_inode_record_committed_behind_the_walk_cursor_is_never_re_minted`**
(`crates/server/tests/gateway_recover_totality.rs:962-1105`). This is the human's requested
regression — "force an insert behind the cursor mid-walk" — built against a real redb store:

* five committed objects take ids 1–5; a sixth is stored **through the production path** at id
  10, and the rows *that PUT wrote* are lifted back out (`all_rows` diff, then `delete_rows`) so
  what the peer later commits is byte-for-byte what a peer's PUT produces, not a hand-built
  approximation. Its fragments stay on the chunk store — what has not happened yet is the
  metadata commit;
* `meta:next_inode` is deleted: the legacy shape recovery exists for;
* `WriterBehindCursorStore` pages in **twos** (a conforming backend — a page may be shorter than
  `limit`, `crates/traits/src/lib.rs:1069-1073`) and commits the peer's rows once the walk has
  taken its first read of `inode:`. `inode:10` sorts before the cursor `inode:2`, so no later
  page can contain it;
* the test then **asserts the miss** — `persisted_next_inode == Some(6)` — before asserting
  anything else. If a future change made the walk see the row, that assertion fails rather than
  letting the test pass vacuously (this is the "fixture includes the fault" check, §7c);
* five new-key PUTs must all commit; the counter must end at 12 (6,7,8,9, then **11** — a
  counter at 11 would mean id 10 was handed out); the step must be attributed on the audit seam;
  and the peer's object must still read back byte-identically with its inode record unchanged.

**`a_re_minted_inode_id_would_overwrite_the_live_objects_fragments_on_the_cluster_path`**
(`:1107-1176`). The first test's pre-fix red is a `Conflict` — an availability loss. This one
shows what the same re-mint costs on the path where chunk ids are *derived* from the inode id
(`cli::chunk_id_minter`) and the fragments are written **before** the metadata commit that would
catch the collision (`cli::cluster_store_put`). Pre-fix the peer's object reads back as **the
other object's bytes** (§7a has the actual assertion output). That is the C-1 failure the
brief's invariant names, demonstrated rather than argued.

Neither test drives a mock of the code under test: `Gateway::recover`, `Gateway::put_object`,
`Gateway::get_object`, `cli::cluster_store_put` and `cli::alloc_inode` are all the production
functions, over `RedbMetadataStore` + `FsChunkStore`. The only doubles are a store that pages
small and writes concurrently (a legal backend doing a legal thing) — everything else is real.

## 6. Refuting my own test (forced, recorded)

**(a) Genuine red?** Yes, at two granularities.

*Guard-only revert* (the delta this round adds — I reverted just the `require_absent` guard and
its conflict handling in `crates/server/src/cli.rs`, keeping every other production change):

```
test an_inode_record_committed_behind_the_walk_cursor_is_never_re_minted ... FAILED
  every new-key PUT after recovery must commit: an id a live `inode:` record already
  holds must be stepped past, never handed out (issue #652): Conflict

test a_re_minted_inode_id_would_overwrite_the_live_objects_fragments_on_the_cluster_path ... FAILED
  assertion `left == right` failed: the peer's object must read back byte-identically
   left:  [97, 32, 100, 105, 102, 102, 101, 114, ...]   "a different object entirely, written much l"
   right: [116, 104, 101, 32, 112, 101, 101, 114, ...]  "the peer object's bytes, which must survive"
```

The left-hand side is the *other object's bytes served under the peer's name* — silent data
loss, not an error. That is the whole argument for C-1 in one assertion.

*Full production revert* (the project's own gate, `engine/scripts/run-verify.sh`, which resets
`../wyrd-verify-l1` to `origin/main`, keeps only the added test file, and runs
`cargo test -p wyrd-server --test gateway_recover_totality`):

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
run-verify.sh: RED   — (production reverted, test kept)  7 failed; 0 passed
run-verify.sh: PASS — red without the fix, green with it.
```

All seven reds are **assertion** reds (no compile error), and the two new ones red on their own
subject line, not incidentally. I re-shaped the double for this: it now writes the peer's rows
after the first read of `inode:` by **either** seam (`scan` on base, `scan_page` after the fix),
because on the un-patched tree `recover` never calls `scan_page` at all and the test would
otherwise have reported a fixture failure instead of the property failure.

**(b) Production path?** Yes. `Gateway::recover()` (`crates/server/src/lib.rs:155-158`),
`Gateway::put_object`/`get_object`, `cli::cluster_store_put` (`crates/server/src/cli.rs:2106`)
and `cli::alloc_inode` — the exact functions the patch changes — over `RedbMetadataStore` and
`FsChunkStore`. Nothing is re-implemented in the test; the peer's records are the real path's
own output, lifted out and replayed.

**(c) Fixture includes the fault?** Yes, and it is asserted rather than assumed. The walk really
does miss the peer's row: `assert_eq!(persisted_next_inode(&db), Some(6), "the fixture must
reproduce the miss: a floor of 11 would mean the peer's row was seen after all and this test
binds nothing")` (`:1022`). The live record is separately asserted present before the
allocations run (`:1034`). On the cluster-path test the object at risk is a **real committed
object** whose fragments are on the shared chunk store, not a synthesised row.

## 7. Gates run in this worktree

* `cargo fmt --all --check` — clean (the target's own commit hook).
* `cargo clippy -p wyrd-server -p wyrd-core --all-targets` — clean (`-D warnings`); one finding
  fixed on the way (`clippy::type_complexity` on the double's `Mutex<Option<Vec<(Vec<u8>,
  Bytes)>>>` → the `Rows` alias).
* `cargo test -p wyrd-server` — all binaries green, including the two the guard interacts with
  (`s3_http_wire`, `gateway_lease_expiry`, `gateway_multi_writer`, `backend_selection`).
* `engine/scripts/run-verify.sh` (C4-verify) — **PASS**, output above.
* `./engine/xtask.sh ci` — see §9.

## 8. Budget

1,114 semantic added lines (non-blank, non-comment), 5 files — against the brief's ≤ ~1,500 and
≤ 15. 1,145 of the 2,133 raw added lines are the acceptance test file. No mechanical migration:
`high_water_marks`'s signature change has exactly one callsite (`crates/server/src/lib.rs:156`).

## 9. Open items the human should still weigh at sign-off

* **Startup *time* is still O(inode namespace)** — memory-bounded and refusal-free, not
  time-bounded. Carried since round 4 and still open; `high_water_marks`'s doc states the
  available bound and why taking it is a behaviour trade rather than an optimisation
  (`crates/core/src/metadata.rs:2249-2270`). Unchanged this round: nothing in the finding
  touches it. It needs a deployment-scale judgement, not a code change.
* **The absent-counter case (§3)** is a deliberate non-change with two justifications, one of
  which is "an existing test asserts the current behaviour". If the human thinks
  `gateway_lease_expiry`'s mechanism for forcing a losing PUT should change, the guard could be
  made unconditional — but that is a decision about an unrelated test's fixture, so I did not
  take it inside this bundle.
* **Residual, pre-existing and unchanged:** the guard sees `inode:` records. An id whose object
  was deleted while its fragments are still deadlined under an `orphan:` grace record has no
  `inode:` key, so it is not covered — exactly as on base, where the floor is also
  `max(inode: key) + 1`. Round 6's `seed_next_inode_floor` doc already states this hazard
  (`crates/server/src/cli.rs:1935-1944`). Closing it means walking the orphan ledger at
  allocation time, which is the apparatus this slice's Plan decision removes.
