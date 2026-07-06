# Result — issue 364 / s3-http-wire-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: an S3-compatible **HTTP listener** in the gateway role; **bucket-scoped**

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix — a net-new, spec-anchored feature landing behind the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 364 / s3-http-wire-surface (iteration 8)

**Task under review:** give the in-process gateway a real client-facing **S3-compatible HTTP
wire surface** — bucket-scoped object **PUT / GET / DELETE** with mandatory **SigV4** auth and
**streaming** bodies, mapping onto the existing `Gateway` write/read paths so the blueprint's
day-one S3 round-trip can run over the wire. This iteration must close the two iter-7
durability findings (durable id-allocator recovery on restart; per-chunk lease renewal on slow
uploads) on top of the already-ratified crate extraction (`gateway-s3`), real-SDK interop, and
overwrite/DELETE fragment-orphaning.

## Grounding / re-run notes
- **Target resolution:** `$PDCA_TARGET` is not readable from this sandbox (env access denied),
  but the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt` holds the patch **pre-image**
  byte-for-byte (`crates/server/src/lib.rs:1-9` = the exact docstring this patch removes), so
  pre-existing-seam citations are grounded there and all new code is grounded on `patch.diff`.
- **Gate re-run:** `cargo`/`xtask` execution is blocked in this sandbox (bash exec denied), so
  C4 rests on the recorded `check-gates.json` (C4-ci **pass**, C4-verify **pass**) plus my
  line-level re-derivation — I could not independently re-run the workspace here. Flagged in the
  C4 basis, not asserted as an independent green.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding at-Check floor is unambiguous (signed loopback PUT→GET→DELETE byte-identical; unsigned/bad-sig refused) with deferred public-TLS/#367 scope explicit — brief:48-62. Spec is testable and bounded. |
| C2 Reproduction (red pre-fix) | PASS | Behavioral reds present, not just compile-error red: `restart_without_recover_collides_showing_the_bug` (s3_http_wire.rs:5651) demonstrates finding-1 corruption without the fix; `slow_streaming_put_renews_in_flight_leases_before_commit` (write.rs test:1703) exercises finding-2. run-verify recorded red→green. |
| C3 Change | PASS | Change maps 1:1 onto spec: HTTP listener + verb dispatch (gateway-s3/lib.rs:2674), SigV4 verify-before-body (lib.rs:2685), streaming PUT/GET (server/lib.rs:4747,4790), net-new DELETE (lib.rs:4839), durable `recover` (lib.rs:4660) wired at composition root (cli.rs:4568). |
| C4 Verification (red→green) | PASS | check-gates.json records C4-ci (fmt/clippy/build/test/deny/conformance) **pass** and C4-verify **pass**; the historical `gateway_lease_expiry` wall-clock flake is quarantined via a 20s skew allowance that still kills the `+→-`/`+→*` mutants (gateway_lease_expiry.rs:5034). NOTE: could not re-run cargo under this sandbox — relying on the recorded gate + line grounding, not an independent green. |
| C5 Causal adequacy | PASS | Root causes removed at source, not guarded: restart collision fixed by `high_water_marks`+`recover` (metadata.rs:1330 / lib.rs:4660); early-chunk GC reclaim fixed by per-chunk lease renewal (write.rs:1522); overwrite/DELETE leak fixed by orphan-ledger grace records in the same atomic commit (metadata.rs:1229,1156); GET-during-DELETE truncation fixed by deferring reclaim to the grace window + Content-Length framing (lib.rs:4790, gateway-s3/lib.rs:2769). **Symptom-guard smell-test: no capability probe / runtime guard around an optional capability — the fixes transform the cause, so C5 does not trip the guard rule.** |
| T1 Structure | PASS | Clean layering: neutral `wyrd-gateway-core` seam (`ObjectGateway`), `wyrd-gateway-s3` wire crate generic over `G: ObjectGateway` naming no concrete, `server` implements the seam and wires concretes only at `cli::cmd_s3` (ADR-0010). Ratifies the iter-6 crate-boundary decision. |
| T2 Shape | PASS | Seam types are minimal and neutral (`ObjectRead{size,stream}`, `ContentHash` enum, streaming `Stream<Item=Result<Bytes>>` source); shared orphan-key protocol hoisted to `core::metadata` as single source of truth so delete-writer and GC-reader can't key-drift (metadata.rs:1093, gc.rs:1788). |
| T3 Runtime | PASS | Bounded-channel streaming GET keeps peak resident at O(chunk) (lib.rs:4801); auth precedes body materialization so unsigned requests never force allocation (gateway-s3/lib.rs:2685 before :2718); constant-time payload-hash compare; XML error escaping (lib.rs:2829). Covered by behavioral tests (streaming-writes-as-they-arrive, unsigned-refused-before-body). |
| T4 Contribution | PASS | Substantial net-new behavioral coverage incl. a genuine independent oracle: real `aws-sdk-s3` dev-dep drives PUT/GET/DELETE byte-identical over loopback (s3_http_wire.rs:6003) — signer/framer is NOT the gateway's own sigv4 — closing the recurring iter-2..5 self-consistency gap; plus overwrite-reclaim, GET-during-DELETE, concurrent-delete-idempotent, restart-recovery, malformed/oversized chunk-header fail-closed. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: this patch edits **shared M4 core** (`write::commit_overwrite` signature change + `metadata.rs`/`read.rs` additions + `custodian/gc.rs` refactor) which the brief names as conflicting with concurrent M4 metadata slices; maintainer must ratify landing this durability seam on the `feat/m4-production-metadata-backend` integration branch vs. its own M4→M7 sequence (brief:63-68,143-145). Crate-boundary/SigV4-scope/TLS-deferral are already ratified (iter-6) — only the sequencing + shared-core blast-radius sign-off remains. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: the at-Check floor (signed loopback round-trip byte-identical; unsigned→403; real-SDK interop green) appears met, but the **LIVE public-TLS S3 round-trip on a deployed host** (blueprint DoD:696-699) is pre-declared **DEFERRED to the first-deployment gate #367** and is not observable here; human must confirm at #367 that the durability floor (restart recovery, lease renewal, overwrite/DELETE orphaning) holds under a **real running custodian topology** (Check runs plaintext loopback, single process, no live GC), and that plaintext-loopback-at-Check remains the accepted posture. |

## Notes for the human
- No blocking (FAIL) findings: every iter-7 carry-forward maps to a grounded fix + behavioral
  test, and the recorded gates are green. My accept-blocking limitation is only that I could
  not independently re-run `cargo xtask ci` under this sandbox.
- Prior-art within the issue history (iterations 1-7, preserved) is well-tracked and each
  rejected approach is visibly not re-attempted unchanged; the affected core/custodian files
  are the same ones prior iterations touched, so the history is the relevant prior art.

### Advisory — adversary

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

### Advisory — codex

- `crates/gateway-s3/src/streaming.rs:197` — The `aws-chunked` decoder bounds the declared data chunk size, but the chunk header line itself is unbounded: it keeps calling `fill()` and appending to `self.buf` until a CRLF appears. A signed malformed upload with a large body and no header terminator can still grow gateway heap proportional to the request body before returning 400, violating the streaming/OOM invariant this patch is meant to close. Add a small maximum chunk-header length and reject once the buffer exceeds it.
- `crates/gateway-s3/src/sigv4.rs:506` — `verify()` accepts the `STREAMING-*-TRAILER` sentinels, but the decoder still requires the zero-size chunk to be followed immediately by `\r\n` (`crates/gateway-s3/src/streaming.rs:221`) and never parses trailer headers/signatures. A real SDK request that uses checksum trailers will pass SigV4 seed verification and then fail as malformed during body decoding. Either reject trailer sentinels in `verify()` until supported, or implement trailer parsing/validation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Decision owed: this patch edits **shared M4 core** (`write::commit_overwrite` signature change + `metadata.rs`/`read.rs` additions + `custodian/gc.rs` refactor) which the brief names as conflicting with concurrent M4 metadata slices; maintainer must ratify landing this durability seam on the `feat/m4-production-metadata-backend` integration branch vs. its own M4→M7 sequence (brief:63-68,143-145). Crate-boundary/SigV4-scope/TLS-deferral are already ratified (iter-6) — only the sequencing + shared-core blast-radius sign-off remains.
- [ ] Validation — fitness-to-purpose — Decision owed: the at-Check floor (signed loopback round-trip byte-identical; unsigned→403; real-SDK interop green) appears met, but the **LIVE public-TLS S3 round-trip on a deployed host** (blueprint DoD:696-699) is pre-declared **DEFERRED to the first-deployment gate #367** and is not observable here; human must confirm at #367 that the durability floor (restart recovery, lease renewal, overwrite/DELETE orphaning) holds under a **real running custodian topology** (Check runs plaintext loopback, single process, no live GC), and that plaintext-loopback-at-Check remains the accepted posture.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Close the high_water_marks orphan-ledger blind spot before this lands: metadata::high_water_marks scans only `inode:` and `pending:` prefixes, so after PUT → DELETE → restart the deleted chunk ids appear in no scanned prefix and Gateway::recover() re-mints them — stale `orphan:` records then either leak permanently (GC's `protects` gate skips them forever) or reclaim a not-yet-committed fragment of the new object (data loss). Fix direction per adversary review: also scan `orphan:` (project the chunk id, take the max) so re-mint never lands on an id whose orphan record / on-disk fragments are still live. Add a behavioral red covering DELETE (or crash mid-overwrite) followed by restart — the existing recover tests only cover PUT → restart → PUT-new-key. Everything else is ratified and must not be re-litigated: shared-core placement of the durability seam on feat/m4-production-metadata-backend is accepted (§6.1 ticked); crate boundary, SigV4 scope, TLS deferral stand from iter-6; plaintext-loopback-at-Check posture is otherwise acceptable pending #367. Do not rework the wire surface — this is a targeted fix to recovery plus its test.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
