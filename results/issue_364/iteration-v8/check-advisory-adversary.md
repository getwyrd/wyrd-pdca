# Adversarial review — issue_364 (iteration 8), S3 HTTP wire surface

Scope: this diff only. Grounded on the target at `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
The iteration-7 carry-forward asked for (1) a durable id allocator / restart recovery,
(2) per-chunk streaming leases, (3) mid-stream GET-fault framing. I attacked all three.

## Findings

- **NEEDS-HUMAN — Durability finding 1 is only half-closed: `high_water_marks` ignores the
  orphan ledger, so chunk ids are re-minted after DELETE+restart.**
  `crates/core/src/metadata.rs:576` (`high_water_marks`) scans only `inode:` (`:580`) and
  `pending:` (`:591`); `Gateway::recover` (`crates/server/src/lib.rs:101`) reseeds
  `next_chunk`/`next_inode` from those marks. But a DELETE (`metadata::unlink`) removes the
  inode and leaves the object's chunk ids referenced **only** in `orphan:` grace records,
  with the fragments still on disk until the custodian GC runs. A committed object carries no
  `pending:` entry, so after a delete the deleted chunk ids appear in **no** prefix
  `high_water_marks` scans → the recovered mark drops to 0. Concrete failing case:
  `PUT bucket/a` → chunk id 1; `DELETE bucket/a` → writes `orphan:<ds>:1:*`; restart →
  `recover()` computes `max_chunk = 0` → `next_chunk = 1`; `PUT bucket/b` → **re-mints chunk
  id 1** (`mint_chunk_id`, `lib.rs:194`). The stale `orphan:<ds>:1:*` record then either
  (a) leaks permanently — once `b` commits and references chunk 1, GC's safety gate
  (`crates/custodian/src/gc.rs:124`, `protects`) skips it forever, so the cleanup that would
  delete the orphan key never runs; or, worse, (b) causes **data loss**: in the window
  between `b` writing its chunk-1 fragment (`write::intent_and_write_chunk`) and committing
  its inode, a concurrent GC pass sees chunk 1 unreferenced (not yet committed) with the
  grace window elapsed and reclaims the fragment via the stale orphan record
  (`gc.rs:134-152`) → `b` commits an object missing a fragment. This is precisely the
  finding-1 corruption class ("a restart replays ids … and collides with committed objects")
  the iteration claims to close. **The recover tests do not cover it**:
  `crates/server/tests/s3_http_wire.rs` `restart_recovers_id_allocators_no_collision` only
  exercises `PUT → restart → PUT-new-key`, and `restart_without_recover_collides…` only
  demonstrates the *inode* `require_absent` rejection (a safe failure), never a DELETE (or a
  crash mid-overwrite) followed by restart. Fix direction: `high_water_marks` must also scan
  `orphan:` (project chunk id, take the max) so re-mint never lands on an id whose orphan
  record / on-disk fragments are still live.

- **NEEDS-HUMAN — GET-during-DELETE truncation is mitigated only for readers faster than the
  grace window, not eliminated.** `get_object_streaming` (`crates/server/src/lib.rs:264`)
  resolves the committed chunk map up front, then reads lazily over a 4-deep channel
  (`:275`) that blocks on socket drain; DELETE defers reclaim to `orphaned_at + grace_window`
  (`metadata::unlink`, GC at `gc.rs:136`). A GET slower than `grace_window_millis` — a slow
  client plus bounded-channel backpressure on a multi-chunk object — still has its **tail**
  fragments reclaimed by GC before the reader reaches them → `read_chunk_verified` raises
  `MissingFragment`, the reader task breaks (`lib.rs:281-283`), and the body aborts *after*
  `200 OK` + `Content-Length` were already emitted (`crates/gateway-s3/src/lib.rs:262-269`).
  So the binding "byte-identical round-trip under concurrent access" holds only for readers
  that finish within a fixed time window; the grace window bounds reclaim delay but reader
  duration is unbounded. The diff's improvement is real (declared `Content-Length` turns a
  silent "complete" 200 into a detectable short read) but it converts truncation into a
  *detectable* truncation rather than preventing it. Human call whether that is acceptable
  first-deployment S3 semantics (a truncated GET is still a failed read of a live object).

- **Streaming lease renewal lapses if a single chunk's write exceeds the TTL.**
  `write.rs` `lease_write_chunk` (patch `crates/core/src/write.rs`, `lease_write_chunk`) only
  re-arms/renews at the *start* of the next chunk (`if !leased.is_empty() && now >= renew_at`).
  If one chunk's fragment fan-out stalls past `lease_ttl_millis` (a slow/So degraded D-server
  on a single chunk), the earlier in-flight leases expire mid-write and a concurrent sweep can
  reclaim them before the commit — the same durability-2 hole, just moved from "slow overall
  upload" to "one slow chunk". The code comments acknowledge the between-chunk stall case; with
  a bounded `chunk_size` this is unlikely, but it is not covered by
  `crates/core/tests/stream_lease_renewal.rs` (which advances the clock only *between* chunks).
  Advisory / not blocking.

## Attempted refutations that did NOT hold (fix survives)

- **Red→green for finding 1 is genuine.** `restart_without_recover_collides_showing_the_bug`
  drives the real production `Gateway::put_object` and reds via `metadata::create`'s
  `require_absent` inode guard — a true behavioural red, not a tautology. Could not refute the
  happy-path recover claim.
- **DELETE/overwrite → GC reclaim is exercised on production functions.**
  `crates/custodian/tests/gc_delete_backstop.rs` drives `metadata::unlink`,
  `commit_chunk_map_superseding`, and the real fenced `reconcile_step`, and asserts the
  live-fragment (referenced) is never reclaimed while the orphaned one is — no parallel
  re-implementation. Placement-aware keying (placed D-server, not index) is proven with a
  non-identity placement. Could not refute.
- **Auth-before-body / hash-after-stream.** `handle` verifies SigV4 before reading the body
  (`gateway-s3/src/lib.rs:184` precedes the PUT body stream `:219`), and the streamed body's
  running SHA-256 is checked before commit (`lib.rs:246-253`) — a tampered body is rejected
  pre-publish. Could not refute.
- **XML error escaping and percent-decode key identity.** `xml_escape`
  (`gateway-s3/src/lib.rs:328`) neutralises the five entities on interpolated messages;
  `percent_decode_utf8` (`:304`) recovers the true key and treats `+` literally. Boundary
  (trailing complete `%XX`) decodes correctly. Could not refute.
