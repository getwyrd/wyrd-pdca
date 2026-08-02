# Adversarial review — issue 508 / multipart-upload (iteration 5)

Advisory only; the deterministic gates already block (`overall: fail` — T4 gating, C5 advisory).
Attacked: the C4-verify red→green evidence, the four carry-forward defects the previous
iteration was rejected for, and the new drain/admission/sizing arithmetic. Three refutations
land; several attempts failed and are recorded as such.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2377` + `:2454`: the fix for
  carry-forward defect 2 replaced non-convergence with *silent loss*. A Complete naming
  ≥ 4,001 parts permanently leaks the `part:`/`psum:` records of every part past the
  4,000th.** `drain_records`' `Parts` arm truncates its derivation at
  `DRAIN_BATCHES_PER_PASS * B_OPS / 2` = 8 × 1000 / 2 = **4,000 units** (`break 'pages`,
  `:2377`) but passes no truncation flag to `commit_units`, whose `exhausted` is set `false`
  only when the *batch* budget is hit (`:2422`), never when the *derivation* was cut short.
  Each unit costs exactly 2 mutations (`part_key` + `psum_key`, no positions), so a batch
  admits 500 units and the truncated 4,000 units drain in exactly `DRAIN_BATCHES_PER_PASS`
  batches — the 8th batch therefore sees `queued.peek().is_none() && exhausted == true`, takes
  the `last` branch at `:2454`, and **deletes the obligation key** (`:2459`) plus returns
  `Ok(true)` / `obligations_drained += 1`. The remaining parts have no obligation left to
  drive them and nothing else deletes a *published* part's records — the code says so itself at
  `:2242-2243` ("the published set rides `retire:records:{parts}`"). I transcribed both loops
  into a standalone binary and ran it: `named=4001 → deleted=4000, obligation_deleted=true,
  leaked=1`; `named=10000 → deleted=4000, leaked=6000`; `named≤4000 → leaked=0`. (`rustc` even
  flags `exhausted` as a dead store in the transcription — the truncation path can never reach
  the guard that exists to protect it.) Concrete failing case: an upload of 4,001 × 5 MiB parts
  (~20 GiB, legal — `MAX_PARTS_PER_SESSION = 10_000` and the slice exists for objects > 5 GB);
  cheaper repro: seed 4,001 parts exactly as the existing test seeds 700 and run one `drain`.
  Consequence chain: while the tombstone lives the stale `part:` records hold their published
  fragments in the staged class; once `W_tombstone` (#625) removes it, `staged_fragments`
  (`crates/custodian/src/gc.rs:470`, keyed on `list_sessions`) no longer enumerates them and
  they are unreachable residue forever. This is the "no state of an upload session is
  absorbing" half of the brief's own invariant, in the arm the brief flagged as
  iteration-4 negation (6).

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2719`: the regression test added for
  this very arm would not have gone red.** `a_records_obligation_larger_than_one_batch_drains_to_completion`
  uses `let parts = 700u32;` (`:2723`, comment: "1,400 record keys, comfortably past `B_OPS`")
  — chosen to clear the *batch* bound (1,000 keys) but sitting far below the *derivation*
  bound (4,000 units) where the new defect appears. It asserts exactly the property that is now
  false above the threshold (`part:` range empty, obligation retired, `records_deleted == parts*2`).
  The oracle is right; only its part count is wrong. Raising it to 4,001 turns it red, which is
  the cheap proof the finding above is real and the cheap guard against its return.

- **NEEDS-HUMAN — `crates/core/src/multipart.rs:182` + `:1239` + `:2498`: on the wave-0 tree
  this bundle produces, the 71st `CreateMultipartUpload` is refused `503 SlowDown`
  permanently.** `max_sessions()` derives `⌊W_REF / u_ref()⌋` = ⌊4,000,000 / 56,760⌋ = **70**
  (`W_REF = 4_000_000` at `:138`; `u_ref()` = min(10,016 × 165, 51,480 + 2×16×165) = 56,760).
  `mpuctl.count` counts `mpu:` records **in any state** (0016:348, faithfully implemented), the
  only decrement is `terminal_delete_batch` (`:1878`), and `teardown_sessions` skips every
  session that is not `Aborting` (`:2498`) — so a `Completed` tombstone holds its admission slot
  until `W_tombstone`, which this slice correctly declares out of scope (#625). Net effect: 70
  *successful* uploads (not 70 concurrent ones) exhaust admission for the life of the store, and
  the wave-0 tree is exactly what #625 and #633 are built and gated on. The brief's ordering note
  covers *deployment* ("misconfigured by construction" without a reaper) but neither the brief nor
  the tests surface the number: leg B(iv) only asserts the create+**abort** round-trip returns
  `count` to 1 (`crates/server/tests/s3_multipart_upload.rs:1294-1307`), never that a *Complete*
  releases nothing. Two decisions for the human, neither Do's: (a) is 70 lifetime uploads
  acceptable for the intermediate stack trees, and (b) with #625 landed, is 70 sessions per
  `W_tombstone` window the intended production capacity for the verb — a tombstone holds **zero**
  staged chunk-refs, so charging it against a `W_ref` memory budget is the conservative reading of
  0016:348, not a forced one. Flagging rather than prescribing: changing it touches 0016's
  admission semantics.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2595` / `crates/server/src/lib.rs:528-530`:
  leg E's `MAX_MAP_CHUNKS` ceiling is bypassable by *omitting* `Content-Length`.**
  `chunk_size_for_put` enforces `SINGLE_PUT_MAX_BYTES` and refuses `EntityTooLarge` (`:2578-2580`),
  but `chunk_size_for_lengthless` (`:2595`) picks the size-independent ⌈5 GiB / 165⌉ ≈ 31.03 MiB and
  **nothing ever checks the bytes actually streamed** — `SINGLE_PUT_MAX_BYTES` appears nowhere in
  the streaming/commit path, and `write.rs` has no post-stream map-length guard. Concrete failing
  case: the same 6 GiB body is refused `400 EntityTooLarge` with a declared length and *accepted*
  as a lengthless `aws-chunked` PUT, publishing a **198-chunk** flat map — past `MAX_MAP_CHUNKS`
  (165) and past the `VALUE_CEILING_BYTES / 2` headroom `:48` calls load-bearing (198 × 302 ≈
  59.8 KB > 50 KB). Past ≈ 10.7 GiB the inode value crosses the hard 100 KB ceiling and the commit
  fails *after* the entire body was streamed and staged — the post-stream failure `:2603`
  says must never happen. Leg E(iii) only pins the declared-length refusal, so the hole is
  untested. Cheapest conforming fix is to count decoded bytes and fail before the commit; the
  brief's "never a mid-stream failure" rule cannot bind here (a lengthless stream's size is
  unknowable at header time), so silently publishing past the ceiling should not be the default.

## Attempted and could not refute

- **The "compile error scored as red" hazard the brief warned about (`run-verify.sh:415-434`) is
  genuinely defended.** Both added files import only base-visible symbols: every crate they use
  (`sha2`, `bytes`, `wyrd-chunkstore-fs`, `wyrd-coordination-mem`, `wyrd-gateway-s3`,
  `wyrd-custodian`, `aws-credential-types`) is already a base dependency of `wyrd-server`
  (`git show HEAD:crates/server/Cargo.toml`), the patch's only `Cargo.toml` additions are
  `rand`/`rand_chacha` under `[dependencies]` and neither test imports them, and every custodian
  context they construct matches the **base** shape (`GcContext` 4 fields, `ScrubContext` 2,
  `ReconstructionContext` 4, `reconcile_step` 7 args — all unchanged by the patch, as the brief
  required). On the base `create_multipart_upload` 501s, so the reds are panics/assertions, not
  build errors. I could not find a symbol that would fail the pre-fix compile.
- **The ETag oracle is not a tautology.** `expected_multipart_etag`
  (`crates/server/tests/s3_multipart_upload.rs:320-326`) recomputes
  `hex(SHA-256(SHA-256(body_1) ‖ … )) + "-" + N` from the part bodies alone; the production
  `multipart_etag` (`crates/core/src/multipart.rs:938`) is never imported. Independent, and
  byte-for-byte the composition the brief settled.
- **All four carry-forward defects are genuinely fixed, not papered over.** (1) GC and restore
  both resolve through the single `resolve_committed_map` (`crates/custodian/src/gc.rs:388`,
  `restore.rs:402`), and rebalance/reconstruction handle `segments` too; (2) the paged branch now
  derives from live state (the *new* defect above is a different bug, not the old one); (3)
  `scan_page` has native cursored implementations on redb (`metadata-redb/src/lib.rs:144`, whose
  `cursor ‖ 0x00` successor trick is a correct exclusive bound), fdb, tikv and the DST sim store,
  plus conformance additions; (4) scrub now chains `referenced.staged_committed`
  (`crates/custodian/src/scrub.rs`) and both C(iii)(b)/(c) tests assert the *positive* — the scrub
  test even runs a clean pass first so it cannot pass by enqueuing indiscriminately
  (`s3_multipart_lifecycle.rs:1043-1049`).
- Searched for a routing bypass past the percent-decoding fence: `has_multipart_marker` /
  `multipart_form` decode every key (`crates/gateway-s3/src/lib.rs:515-533`, `:610`),
  `foreign_subresource_on_multipart` re-applies the denylist to every non-marker key (`:545`),
  duplicate keys take the first occurrence, and out-of-range/non-numeric part numbers are refused
  (`:636`, `crates/server/src/multipart.rs:331`). The only forms I found that still reach the
  plain PUT arm (e.g. `?partNumber%3D1`, where the `=` itself is encoded) behave identically on
  the base and are the pre-existing #491 residual the brief puts out of scope.
- `drain_off_request_path` conforms to the await-discipline rule: bounded passes, finished
  handles pruned, all handles aborted in `Drop` (`crates/server/src/lib.rs:133-152`).

## On the gate rows

- `C4-verify: PASS — red without the fix, green with it` — I could not refute this row itself
  (see above), but note it proves nothing about the three defects here: all four findings sit on
  paths no added test reaches (>4,000 parts, the 71st Create, a lengthless >5 GiB body).
- `C5: 244 missed mutants` is worth **not** waving through as advisory noise this round: the
  headline defect lives in exactly that class — a truncation branch (`:2377`) whose only
  regression test is calibrated below its threshold — so at least one of the 244 marks a real
  bug rather than a coverage statistic.
