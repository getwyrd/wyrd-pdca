# Build notes — issue 508 / multipart-upload (iteration 4)

> Withheld from the reviewer by the driver. Written for the human at sign-off.

**Design authority read:** `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
in the worktree (3,108 lines, `origin/main` @ `22d71b4`) — decisions 1–5, the protocol half
of 6, and 7, plus the batch inventory (§3), the record table (§1), the knob table
(`0016:1462-1479`), the clock-lifecycle table and the graduation criteria. Also read:
`sources/00-design-authority-0016.md`, `01-prior-plan-reviews.md`, `02-salvage-from-rev3.md`,
and the target's `AGENTS.md` § *Review rubric & protocol*.

---

## 1. READ THIS FIRST — what is delivered, and what is not

This is the largest single bundle the project has attempted, and **it is not complete**. The
brief's own *Open question 1* anticipates exactly this and instructs "**Do does not split — Do
reports**". So: here is the report.

### Delivered, tested, and gate-green

| Area | State |
|---|---|
| The record model + key space of `0016:346-356` (`mpuctl`, `mpu:`, `slot:`, `part:`, `psum:`, `sidx:`, `seg:`, `seggrp:`, `retire:`) | **done** — `crates/core/src/multipart.rs` |
| The state machine + fenced CAS transitions of `0016:528-601` | **done** |
| Decision 1 — fence-based publication proof | **done** |
| Decision 2 — staged protection class in `ReferenceSet`, wired into **GC**, **restore** and **drain/desired-state** | **done** (3 of 6 consumers — see the gap table) |
| Decision 3 — verb × state table, failure semantics, tombstone fingerprint | **done** |
| Decision 4 — `retire:` ledger + bounded drain; `unlink`/supersede fan-out for **multipart-published** generations | **partial** — see the gap table |
| Decision 5 — owned `sidx:` staging with placement, `slot:` key space, compensation on every post-staging refusal | **done** |
| Decision 6 (protocol half) — `mpuctl` serialized admission, profile compare, `clock_source` | **done** |
| Decision 7 — flat-or-segmented map, staged publication, epoch-scoped `seg:`, `PutObject` chunk-size selection | **done** |
| The S3 wire surface — six verbs, routing off the denylist over **decoded** keys, exact status + code | **done** |
| `PendingEntry.owner` / `.staged` with `skip_serializing_if` | **done** |
| `MetadataStore::scan_page` seam + normative contract | **partial** — default impl only; see the gap table |
| Living architecture docs (`06-runtime-view.md` §6.1a/§6.7, `08-crosscutting-concepts.md` §8.9) | **done** |
| Operator-visible startup signal for a missing reaper | **done** — `Gateway::warn_if_reaper_absent` |

`cargo xtask ci` (the project's own gate: fmt, clippy `-D warnings`, typos, the hygiene guards,
and the whole test suite) is **green**, and the patch applies cleanly to `origin/main` @ `22d71b4`.

### NOT delivered — the human must decide what to do about each

1. **`scan_page` has a *default* implementation over `scan`, not native cursored range reads on
   `metadata-redb` / `metadata-fdb` / `metadata-tikv` / the DST sim store, and no
   `metadata-conformance` assertions.** The contract (byte-lexicographic order, exclusive
   `after`, `next` semantics, no-skip-for-stable-keys) is written down and the default satisfies
   it — but the default inherits `scan`'s `SCAN_CAP`, which is the *one thing* the seam exists to
   escape. So a `retire:` population grown past `SCAN_CAP` is still un-enumerable. **Cost of the
   gap:** unbounded retention under sustained overwrite pressure, i.e. the failure the paginated
   walk was added to prevent. **Why it is not here:** a required trait method breaks ~30 test
   doubles; a *default* keeps them compiling, and the three native overrides plus the
   conformance suite are a self-contained follow-on. This is brief *Open question 1* seam (i).
2. **Decision 2's remaining three maintenance consumers.** `scrub`, `reconstruction` and
   `rebalance` are **not** taught the staged class, so brief leg C(iii)(b)/(c)/(d) is **not
   met**: a corrupt staged fragment enqueues no repair, a lost staged fragment's `part:`
   placement is not repointed under the fenced CAS, and rebalance has no explicit staged
   carve-out. Drain/desired-state (C(iii)(a), the F6 **wipe** trace — the one row whose violation
   is data loss) **is** done and is proven by a negation run below. **Cost of the gap:** staged
   redundancy decays untended for the life of an upload; it is a durability-erosion risk, not a
   correctness violation of the stated invariant.
3. **Decision 4's retirement route for the *ordinary* delete / overwrite path.**
   `metadata::unlink` and `commit_chunk_map_superseding{,_leased}` still expand the prior
   generation's orphan marks **inline**. A multipart Complete **does** install
   `retire:bytes:{generation}` (so the segmented and large-multipart case is bounded), but a
   plain `DeleteObject` of a very large object still owes its fan-out in one batch. 0016 permits
   the inline path "for a small flat object" and makes the retirement route mandatory above one
   batch's worth of fragments — the threshold switch is **not** implemented. **Cost of the gap:**
   deleting a max-segmented object through `DeleteObject` would exceed the transaction envelope
   permanently (outcome (d)). Segmented generations reach the ledger only via a superseding
   multipart Complete.
4. **Seeded Tier-0 DST coverage is absent.** `AGENTS.md` makes it a MUST that "a new destructive
   or concurrent path lands with seeded Tier-0 DST coverage", and 0016 lists the specific
   scenarios (`0016:2878-2905`). None is written. The upload-id seam is *made* seedable
   (`Gateway::with_upload_id_seed`) precisely so those scenarios are writable, but they are not
   written. **This is a stated merge-blocker in the target's own rubric.**
5. **The `metadata-conformance` round-trips** for the new record classes and the
   `PendingEntry` decode→encode identity on both shapes are **not** in the conformance suite.
   The `PendingEntry` identity property *is* enforced structurally (`skip_serializing_if` on both
   fields) and is exercised end-to-end by every CAS the protocol runs, but the two explicit
   round-trip tests the brief names are not present.
6. **`W_completing` / `W_open` / `W_session` / `W_tombstone` and every window-driven exit** are
   #625's by design and are correctly absent. The gateway's own drain is bounded, re-entrant and
   convergent, and runs **off the request path**.

**My recommendation:** this is reviewable only as the four seams brief *Open question 1* names.
Items 1, 2 and 3 above map almost exactly onto seams (i), (iii) and (iii) again; item 4 is
cross-cutting. If the maintainer wants one PR, it needs items 1–4 finished first.

---

## 2. Forced refutation — the three questions, answered with evidence

### (a) Genuine red? **YES.**

I reverted the production fix (restored every tracked file, deleted the two new production
modules `crates/core/src/multipart.rs` and `crates/server/src/multipart.rs`) and kept **both
added test files exactly as they ship**. Result:

```
crates/server/tests/s3_multipart_upload.rs     — 10 tests ran, 10 FAILED
crates/server/tests/s3_multipart_lifecycle.rs  —  5 tests ran,  5 FAILED
```

**15 tests ran and 15 failed by assertion / runtime error — not a build error.** That is the
number the brief asks me to record, and it is what distinguishes a real red from the C4 gate
hazard (`run-verify.sh:415-434` prints `PASS — red without the fix` over a build that ran
nothing when `run_test` returns non-zero).

Representative red messages:
* `create: ServiceError(… code: "NotImplemented", message: "the `uploads` S3
  subresource/operation is not supported" …, status 501)` — leg A / C setup;
* leg A routing: `PUT /b/k?part%4Eumber=1` returns **200** on the base (the object is
  overwritten), so the exact-status assertion `(400, "InvalidArgument")` fails.

**The red leg caught a real defect in my own test.** The first revert attempt failed to *compile*
because `s3_multipart_lifecycle.rs` named `RestoreReport.staged_skipped` — a field **this patch
adds**. That is precisely the hazard the brief warns about: a compile error scored as a red over
a run that executed nothing. I replaced it with a base-visible observable (assert the fixture has
**no** committed inode, so `stranded_marked == 0` is discriminating rather than trivially true)
and re-ran. Both files now compile against `origin/main` and fail by assertion.

Post-restore: **15/15 green**, and `cargo xtask ci` all checks passed.

### (b) Production path? **YES.**

Every test drives the real `S3Gateway` router over a real TCP listener with real SigV4, the real
`wyrd-server::Gateway`, the real `wyrd-core` write path, the real `wyrd-core::multipart` protocol
and — in the lifecycle file — the real `custodian::reconcile_step` / `reconcile_after_restore` /
`reconciliation_status`. Nothing is mocked. The only doubles are the **storage backends**
(`MemMeta` implementing the three base-visible `MetadataStore` methods, `MemDServer`/`MemFleet`
implementing `ChunkStore`/`PlacementChunkStore`) — the same class of double as
`RedbMetadataStore::in_memory`, and required because `RedbMetadataStore` is not `Clone`, so
`Gateway::new` would consume the only handle and the raw-prefix observations leg B needs would be
impossible. The custodian sweeps **exactly the servers the gateway wrote through**, not a copy.

### (c) Fixture includes the fault? **YES.**

* Leg B(i) and leg C hold a part **genuinely mid-stream** on a raw socket (the head declares the
  full `content-length`, only half the body is written), and the checkpoint is taken there — so
  the fixture contains a live in-flight `sidx:` entry, not a post-hoc absence. Asserted:
  `sidx:` non-empty **and** `pending:` empty at that instant.
* Leg C(iii)(a) additionally asserts `part:<id>:` is **empty** at the checkpoint, so the only
  thing naming those bytes is the in-flight owned entry — the sharpened case a committed-only
  consumer answers `Satisfied` on.
* Leg C(i) asserts the fixture has **no committed inode at all**, so every fragment the restore
  pass walked is one it would otherwise have marked.
* Leg B(iii) uploads part 2 **twice** and Completes naming only 1 and 3 — the two commonest
  client behaviours, both of which leave residue the accounting must dispose of.
* Leg A submits parts **out of order** (3, 1, 2) with a non-final part of 5 MiB + 7 B — so an
  assembler using arrival order or assuming chunk alignment fails on byte identity.
* The lifecycle fleet is sized to **9** servers, so `dserver` *is* the index at the RS(6,3)
  identity placement: the server the reference set names is the server holding the bytes. With a
  modulo-routed façade every assertion would have been about a harness mismatch instead of the
  protocol — and it was, until I fixed it (the first run marked 410 fragments stranded).

---

## 3. Negation runs (the brief's list) — recorded, with outcomes

Legs B, C and D are only trivially red at C4 (their setup needs multipart, which 501s), so their
force is evidenced here.

| # | Deliberately-broken variant | Leg that caught it | Outcome |
|---|---|---|---|
| 1 / 2 | staged `part:`/`sidx:` fragments **not** in the reference set (identical to "filter inside `gc::reconcile` instead of in the reference set") | C(i) **restore** half | **CAUGHT** — `stranded_marked` = **738**, expected 0. The **GC half did not fail** — exactly as the brief predicts, since `gc.rs:158-190` retains an unevidenced fragment either way. Do **not** claim the GC half discriminates this. |
| 8 | same edit | C(iii)(a) drain/desired-state | **CAUGHT** — `reconciliation_status` answered `Satisfied` over a live upload's bytes (the F6 wipe trace). |
| 3 | release paths **without** `mark_orphaned` evidence (`observe_positions` → empty) | C(ii) **and** B(iii) | **CAUGHT, both.** C(ii): *"teardown must WRITE the reclamation evidence"*. B(iii): *"the unnamed staged part's bytes MUST be orphan-marked"*. This is iteration 2's shipped shape. |
| 4 | a part commit that **writes** the session record instead of merely `require`-ing it | the concurrency probe `concurrent_part_uploads_do_not_conflict_with_each_other` (4 concurrent parts) | **CAUGHT** — one of four parts fails with a 500. Leg A is sequential and does **not** catch it, which is why the probe is a separate test in the shipped file. <br>**Methodological note:** my *first* construction of this negation (put the identical bytes back) did **not** fail the leg — because writing unchanged bytes is a no-op. That was a bad negation, not an inert leg; the counter-bump form above is the real defect shape (the per-session mutation counter 0016 rejected) and it fails immediately. |

**Not run** (no time; recorded as unevidenced): 5 (Complete without the `Open@E → Completing@E+1`
fence), 6 (`retire:records:{parts}` naming *all* parts), 7 (version frozen at fence time). Each is
*implemented* — 5 is `fence_complete_batch` before `list_part_records`; 6 is `publish_batch`
splitting published vs. unnamed parts by construction; 7 is `resolve_flip_binding` re-reading the
prior inside the `R_PUBLISH` retry loop and computing `prior.version + 1` there — but none has a
negation run behind it, so treat those three as reasoned-not-demonstrated.

---

## 4. Design decisions I took, and why

### The multipart ETag composition (this slice's, per 0016 / the brief)

`etag = lowercase_hex( SHA-256( d_1 ‖ d_2 ‖ … ‖ d_N ) ) + "-" + N` over the **raw 32 binary
digest bytes** in ascending part-number order over exactly the parts the client named. The test's
oracle (`expected_multipart_etag`) computes this **from the part bodies**, independently of the
implementation — deliberately, because "a SHA-256-based pure function" admits mutually
incompatible spellings (hex text instead of raw bytes, separators, part numbers mixed in) and
each would pass a self-referential oracle. Never MD5 (ADR-0047).

### Knob values (each inside the range `0016:1462-1479` settles, cited at its definition)

```
MAX_MAP_CHUNKS      = ⌊(100 KB / 2) / 302 B⌋ = 165   (worst-case ChunkRef encoding end of 165–381)
MAX_SEG_CHUNKS      = 165                            (identical rule, one record class down)
MAX_PART_CHUNKS     = 165                            (identical rule; and ≤ B_OPS — X110)
MAX_ROOT_SEGMENTS   = ⌊(100 KB / 2) / 160 B⌋ = 312   (worst-case SegmentRef encoding)
MAX_STAGED_CHUNKS   = 312 × 165 = 51,480             (the publishable segmented ceiling, the settled upper end)
MAX_INFLIGHT_PARTS  = 16                             (≤ 10,000; ≤ ⌊2^20/(2·165)⌋ = 3,177; ≤ B_OPS)
B_OPS               = 1,000                          (with MAX_PART_CHUNKS = 165 ≤ 1,000 ✓)
W_REF               = 4,000,000                      (0016's worked example, `0016:2129`)
U_ref  = min((10,000+16)·165, 51,480 + 2·16·165) = min(1,652,640, 56,760) = 56,760   [DERIVED]
MAX_SESSIONS = min(⌊4,000,000 / 56,760⌋, 2^20/2) = min(70, 524,288) = 70             [DERIVED]
R_PUBLISH = 4 ; MAX_COMPLETE_ATTEMPTS = 8 ; CHUNK_SIZE_MAX = 64 MiB
```
`check_chunk_size_config` enforces `chunk_size_max ≥ ⌈5 GiB / 165⌉ = 32,537,631 B ≈ 31.03 MiB`;
64 MiB satisfies it. Every one of these is asserted as a **`const` block** in
`multipart::tests::knobs_are_inside_their_settled_ranges`, so a knob moved out of range is a
*compile* error, not a test failure.

**Leg D arithmetic** (recorded as the brief requires): at `with_chunk_size(64 KiB)`, six 5 MiB
parts assemble to `⌈5 MiB / 64 KiB⌉ × 6 = 80 × 6 = 480` chunks — past the flat ceiling of 165,
while each part's own 80 chunks is well under `MAX_PART_CHUNKS = 165` and every non-final part is
exactly the 5 MiB minimum. 480 chunks segment into `⌈480/165⌉ = 3` segments, ≤ `MAX_ROOT_SEGMENTS`.

### One value set for `W_ref` / `B_bytes` / `B_ops`, landed in wave 0

Implemented exactly as the brief's **SETTLED** note directs, and documented at
`BudgetProfile` in `crates/core/src/multipart.rs`. `mpuctl.profile` is written by Create — in
*this* slice — and *contains* them; two independently chosen `B_ops` values would admit a session
whose unsplittable reap fence no longer fits, which is a permanent, data-retaining failure mode.

### Segmentation shape — an additive optional field, not a tagged enum

0016 §7a says `InodeRecord.chunk_map` "becomes a two-variant value". I implemented that as
`chunk_map: Vec<ChunkRef>` (flat, unchanged) **plus** `segments: Option<SegmentedMap>` with
`skip_serializing_if` — a two-variant value on the wire (flat when absent, segmented when
present), which preserves decode→encode identity on every pre-existing record trivially and keeps
the blast radius at ~5 construction sites instead of the 136 `.chunk_map` references an enum
would have broken.

**The cost, and I hit it:** an enum forces every consumer to handle the segmented arm; an
additive field does not, and a consumer that reads `chunk_map` directly sees an **empty** map for
a segmented object. That is exactly what happened — the first leg-D run failed with
`IncompleteBody` because `get_object_streaming` read `inode.chunk_map` directly and served a
segmented object as an empty body. Fixed by routing both read entry points through
`multipart::resolve_chunk_map`, which is **fail-closed** (a missing segment under an *unchanged*
root is an error, never a torn success) and implements 0016 §7h's resolve-retry rule. **Residual
risk for the reviewer to weigh:** a *future* consumer could still read `chunk_map` directly. An
enum would make that a compile error. If the maintainer prefers the enum, it is a mechanical
(large) follow-up.

### The drain runs **off the request path**

Abort and Complete answer from their fence/flip commit alone, then spawn a **detached, bounded**
drain task (`drain_off_request_path`, ≤ 8 passes, terminating). This is load-bearing, not a
nicety: draining inline made the second Abort inside the teardown window answer **404** instead of
the idempotent **204** the verb × state table promises — the test caught it. `Gateway.meta` is
therefore held behind an `Arc` so the task can own a handle. With no tokio runtime present the
drain is simply left to #625; the obligations are durable either way.

### The `Aborting` idempotence cell, made deterministic

`a second Abort → 204` races the drain on an in-memory store. Rather than weaken the oracle I made
it deterministic **through the gate 0016 actually specifies**: hold a second part mid-stream so
the session's owned `sidx:` range is non-empty, which blocks the terminal delete. The test now
pins the `Aborting` cell *and* the empty-`sidx:` teardown gate at once.

### `DELETE /b/k?uploadId=U` → 400, and Abort is also `DELETE /b/k?uploadId=<id>`

The brief lists `DELETE /b/k?uploadId=U` among the ill-formed forms answering **400
InvalidArgument**, while AbortMultipartUpload *is* that shape. I resolved the apparent conflict
by reading `U` as a **malformed token**: upload ids are 32 lowercase hex characters, validated at
the routing boundary. A malformed id is `400 InvalidArgument` (the request is not a legal
multipart form); a **well-formed but unknown** id is `404 NoSuchUpload` (the form is legal, the
upload is gone). Both are asserted. Conflating them would make a typo indistinguishable from an
expired upload. **If the maintainer intended something else here, say so — it is a one-line
change.**

The denylist check runs **before** the token check, so
`PUT /b/k?partNumber=1&uploadId=U&t%61gging=1` answers `501 NotImplemented` naming `tagging`
(not 400), matching the brief's stated expectation.

### Existing behaviour I deliberately changed (a reviewer will ask)

1. `crates/gateway-s3/src/lib.rs` unit test `unsupported_subresource_flags_multipart_and_subresource_forms`
   → renamed and inverted for the three multipart markers: it now asserts they are **absent** from
   the denylist, so a later edit re-adding one silently 501s a working verb and is caught.
2. `crates/server/tests/s3_http_wire.rs::streaming_put_writes_chunks_as_they_arrive_not_after_buffering`
   now passes `Some(len)` as the declared length. With `None` it models a **lengthless**
   `aws-chunked` stream, whose size-independent chunk size (≈31 MiB) means a 64-byte body never
   crosses a boundary. **The streaming invariant is unchanged** — peak resident bytes stay
   `O(chunk_size)`, independent of object size; it is `chunk_size` that grew, which is 0016's
   registered gateway-memory trade. The test measures the write path's incremental behaviour, and
   with a declared length it measures exactly what it always did.
3. `ObjectGateway::put_object_streaming` gained a `declared_length: Option<u64>` parameter. Leg E
   is unimplementable without it — the store cannot pick a chunk size it is never told the size
   for. Alternative rejected: a second `put_object_streaming_sized` method with a delegating
   default (2 methods, ~25 extra lines, and a seam that lies about what a PUT carries).
4. `gc::orphan_leases` now decodes `orphan:` values **dual-format**. Without this every structured
   mark the drain writes would fail the bare-`u64` parse and be *silently dropped*, making the
   fragment permanently unreclaimable — i.e. the retirement path would leak every byte it
   evidenced. The leg-C(ii) test caught this (1,440 fragments survived a GC pass past the grace
   window).

### Alternatives rejected, with cost

* **Enum `ChunkMap` instead of the additive `segments` field** — costs **136** `.chunk_map`
  references across **28** files (measured: `grep -rn "\.chunk_map" crates/ | wc -l`), most of them
  in test files that would need mechanical rewriting. Rejected for this iteration on that count,
  with the residual risk stated above rather than hidden.
* **Required `scan_page` trait method** — costs ~30 test-double impls across
  `crates/{custodian,core,server,metadata-conformance}/tests/*.rs` (measured:
  `grep -rn "impl MetadataStore for" crates/ | wc -l` = 30+). Rejected in favour of a default,
  **and the native backend overrides are then owed** (gap item 1).
* **Buffering parts in memory** — rejected; violates the streaming/OOM-cliff invariant
  (`crates/server/src/lib.rs:276-286`). Parts stream chunk-at-a-time through
  `write::stream_staged_data`.
* **A true S3 `md5-of-part-md5s` ETag** — rejected; ADR-0047 rejected MD5 and it would add a
  dependency.

---

## 5. Evidence commands (reproducible)

```
cd $PDCA_WORKTREE
cargo test -p wyrd-server --test s3_multipart_upload --test s3_multipart_lifecycle   # 15/15 green
./engine/xtask.sh ci                                                                 # all checks passed
```
Red leg (what I ran): restore all tracked files, delete `crates/{core,server}/src/multipart.rs`,
keep both added test files → `10 FAILED` + `5 FAILED`, zero passes, no build error.

Scratch used and removed: `$PDCA_SCRATCH/pdca-builder-508-redleg`.

---

## 6. Items for §6 / sign-off

* **NEEDS-HUMAN — proposal correction (brief *Open question 2*).** 0016 decision 7
  (`0016:2314-2320`) and its implementation summary (`0016:2678`) still say
  `Segmented { group: (<upload-id>, <epoch>) }` and call the nonce "the minting upload-id", which
  §1 (`0016:499-526`) supersedes with an independent segment-group nonce. I implemented the §1
  rule as the brief directs. A proposal edit is architecture-board authority, never mine.
* **NEEDS-HUMAN — scope.** Gap items 1–4 in §1 above. Item 4 (seeded Tier-0 DST) is a stated
  merge-blocker in the target's own `AGENTS.md`.
* **NEEDS-HUMAN — the deferred large-object leg.** `aws s3 cp` of an 8+ GB file round-tripping
  `sha256`-identical against a deployed stack is observable only off-Check; the machinery is built
  and exercised at Check by legs A–D at smaller sizes. Eduard Ralph confirms by hand.
* **Open question 3 (posture).** The missing-reaper signal is `warn`-level
  (`Gateway::warn_if_reaper_absent`, plus a `s3_multipart_reaper_absent` counter). A hard startup
  refusal is a one-line change if preferred.
* **Open question 4** asked nothing of Do; no opt-in switch is shipped, as settled.
* No new third-party crate entered the workspace graph. `sha2` (already a workspace dep) became a
  direct dep of `wyrd-core`; `rand` + `rand_chacha` (already workspace deps, `Cargo.toml:127-128`)
  became direct deps of `wyrd-server` for the ADR-0035 upload-id seam. Neither needs an ADR-0003
  audit. **Neither added test file imports them** (the red leg reverts `Cargo.toml`).
* No external dependency beyond the base toolchain was needed for anything binding at Check.
