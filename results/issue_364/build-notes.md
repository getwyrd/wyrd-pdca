# Build notes — issue 364 / s3-http-wire-surface (iteration 9)

## Scope of this iteration (targeted, per iter-8 carry-forward)

The iter-8 sign-off **ratified everything else** and rejected on a single blind spot:

> Close the `high_water_marks` orphan-ledger blind spot before this lands: … after
> PUT → DELETE → restart the deleted chunk ids appear in no scanned prefix and
> `Gateway::recover()` re-mints them — stale `orphan:` records then either leak
> permanently … or reclaim a not-yet-committed fragment of the new object (data
> loss). Fix direction … also scan `orphan:` (project the chunk id, take the max)…
> Add a behavioral red covering DELETE (or crash mid-overwrite) followed by restart.
> … **Do not rework the wire surface — this is a targeted fix to recovery plus its
> test.**

So this iteration carries forward the ratified iter-8 patch **unchanged** and adds
exactly two things:

1. `crates/core/src/metadata.rs` — `high_water_marks` now also scans the `orphan:`
   prefix, projects each record's chunk id, and folds it into `max_chunk`.
2. `crates/server/tests/s3_http_wire.rs` — a new behavioral regression,
   `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`.

Nothing in the wire surface (gateway-s3, gateway-core, streaming, sigv4, TLS
posture, crate boundary) was touched.

## The defect, precisely

`high_water_marks` (`crates/core/src/metadata.rs:576` on the target branch, i.e. the
iter-8 tree) seeds `Gateway::recover`'s in-process id allocators by scanning only
`inode:` and `pending:`. Trace `PUT bucket/a → DELETE bucket/a → restart`:

- PUT A commits inode 1, chunk 1 (pending ledger for chunk 1 cleared at commit).
- DELETE A → `metadata::unlink` removes the `inode:1` key **and** writes an
  `orphan:<ds>:1:<index>` grace record for each of chunk 1's fragments in the same
  atomic batch (`crates/core/src/metadata.rs:60`, `394-411`). A's fragments stay on
  disk until the custodian GC's reader-safe window elapses
  (`crates/custodian/src/gc.rs:134-141`).
- Restart → `recover` calls `high_water_marks`, which now sees **no** `inode:` key
  (removed) and **no** `pending:` key (cleared at commit) referencing chunk 1. It
  returns `max_chunk = 0`, so `next_chunk` resets to 1.
- The next new-key PUT (B) re-mints chunk id **1** — the exact id whose fragments are
  still live on disk under A's orphan record.

Two failure modes then follow from the re-mint, both real:

- **Data loss:** once A's grace window elapses, GC deletes the fragments the
  `orphan:1:*` records name (`gc.rs:152`, `store.delete_fragment(frag)`). Those are
  now **B's** fragments — B is silently truncated/destroyed.
- **Permanent leak:** if B's placement happens to cover the same `(dserver, chunk,
  index)` slots, GC's `ReferenceSet::protects` (`gc.rs:200`, keyed on chunk id) now
  reports them referenced, so the stale orphan bytes are never reclaimed.

## The fix

`high_water_marks` adds a third scan over `ORPHAN_PREFIX`, using the existing
`parse_orphan_key` inverse to project each record's `frag.chunk` into `max_chunk`
(same `< 2^64` in-process projection as the inode/pending scans). `recover` then
resumes `next_chunk` **above** every chunk id whose orphan record / on-disk fragments
are still live, so a re-mint can never collide with a deleted-but-not-yet-reclaimed
object. This is the smallest change that restores the invariant "the id allocator
resumes above everything on disk" — the same invariant the inode/pending scans
already serve; the orphan ledger was simply an un-scanned third home for a live id.

Chunk-only on purpose: an orphaned object's `inode:` key is already gone, so its
inode id is genuinely free to reuse; only the chunk id is still pinned by on-disk
fragments. `max_inode` is deliberately left untouched.

### Alternative considered and rejected

**Eager reclaim on DELETE** (delete the fragments immediately, so no lingering
orphan record exists to collide with) was rejected on two counts, not cost:
(a) it re-opens the GET-during-DELETE truncation the wire layer already closed by
deferring reclaim to the grace window (`crates/server/src/lib.rs:303-312`), an
explicitly ratified property; and (b) it does not restore the stated invariant —
recovery must resume above *any* live id, and pending/expired-lease ids can outlive a
process too, so the allocator seam is the correct place to fix it. The carry-forward
named the invariant ("re-mint never lands on an id whose orphan record / on-disk
fragments are still live"), so the target is the smallest change that restores it —
the orphan scan — not a redesign of the delete path.

## The test — behavioral, red→green, drives production

`restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`
(`crates/server/tests/s3_http_wire.rs`) drives the production paths end-to-end:

1. `put_object("bucket/a")` then `delete_object("bucket/a")` (the `ObjectGateway`
   trait method), then drop the handles — a real PUT + DELETE leaving A's fragments
   under the orphan ledger.
2. Reopen the same persisted redb + fs state, `recover()`, `put_object("bucket/b")`.
3. Read the `orphan:` records back (`ORPHAN_PREFIX` scan + `parse_orphan_key`) and
   perform **exactly the reclaim the ledger authorises** — `delete_fragment` for each
   fragment GC would delete after grace (`gc.rs:152`) — through the same `ChunkStore`.
4. Reopen + `recover` + `get_object("bucket/b")` and assert byte-identity.

This is not an adjacent proxy: it reproduces the actual **data-loss** end result. It
uses production `high_water_marks` (via `recover`), production PUT/DELETE, and the
ledger's own named fragments for the reclaim step; nothing is re-implemented.

- **RED** (orphan scan removed): `GET B` fails with
  `InsufficientFragments { chunk_id: 1, have: 0, need: 6 }` — B re-minted chunk 1 and
  the orphan reclaim deleted B's own fragments. Demonstrated by reverting only the
  new scan loop and re-running.
- **GREEN** (fix in place): B is minted chunk id 2 (disjoint from the orphaned chunk
  1), the reclaim leaves B untouched, and `GET B` returns the bytes byte-identical.

Why the test lives in `crates/server/tests/s3_http_wire.rs` rather than a core unit
test: the failure is a composition of `Gateway::recover` + `put_object`/`delete_object`
+ the orphan ledger + a GC-style reclaim, which only compose at the server crate.
`high_water_marks` is still driven as production code through `recover`. The runner is
headless (`cargo xtask` / `cargo test`, no display); the test pulls in no GUI/IO-heavy
module at load — redb-in-file + fs chunk store only.

## Verification

- `cargo test -p wyrd-server --test s3_http_wire
  restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss` — **RED** with
  the scan removed (data-loss panic above), **GREEN** with it in place.
- Full `s3_http_wire` binary: 16/16 green (all ratified iter-8 tests + this one).
- `cargo test -p wyrd-core --lib metadata`: green.
- `cargo fmt --check -p wyrd-core -p wyrd-server`: clean (fmt run over touched files
  so the target's commit hook has nothing to reformat).
- `cargo clippy -p wyrd-core -p wyrd-server --tests`: clean.

The full `cargo xtask ci` gate (`./engine/xtask.sh ci`) is Check's authoritative gate
and re-runs the whole suite; this Do beat verified the targeted red→green plus fmt/
clippy on the touched crates.

## Standing NEEDS-HUMAN (unchanged, pre-declared — not re-litigated here)

Per the iter-6/iter-8 ratifications these remain human calls at sign-off and were
**not** reworked this iteration: crate boundary (extracted `gateway-s3` accepted),
SigV4 scope (header-only, minimal error floor), plaintext-loopback-at-Check with
public TLS deferred to #367, and the `gateway_lease_expiry.rs` wall-clock flake
quarantine carried from iter-8. The live "gateway serving S3 publicly over TLS" green
is the off-Check #367 deliverable, as pre-declared in the brief's STOP discipline.
