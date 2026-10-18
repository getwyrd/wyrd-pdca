# Build notes — issue 686 / streamed-get-moves-flat-chunk-map (iteration 3)

Target: getwyrd/wyrd @ main, worktree base `7bf7eb9` (still the tip of upstream `main`:
`git ls-remote origin refs/heads/main` → `7bf7eb906c63…` on 2026-10-15). Line numbers are
on the patched tree unless marked "base".

## Short version

The code is the same as iteration 2. The only thing that failed in round 2 is a gate step
that fails the same way on `main` without this patch, and the fix for it is a
supply-chain policy change that belongs in its own PR, decided by a human. This round I
proved that directly (ran the failing step on the clean base), re-ran every check, and wrote
down the exact separate change a human can land if they choose to.

## What the carry-forward asked, and what I did about each part

The round-2 sign-off rationale lists:

1. **C2 (red pre-fix): "Accept the declared mutation-based substitute".** A human decision,
   not build work. I re-ran the substitute evidence this round (below) so it is fresh, and
   added a second mutant.
2. **C4: "Determine the disposition of the existing TiKV advisory failures and incomplete
   gate evidence".** Also a human decision. What I can do as builder is make the evidence
   direct instead of inferred, which I did (next section).
3. **Failing gate: `cargo deny --all-features --config deny-all-features.toml check
   advisories`.** Fails on `main`. See next section.
4. The round-2 review found **no implementation defect**: C3, C5, T1, T2, T3, T5 all PASS;
   the T4 batch review had 0 blocking findings
   (`iteration-v2/check-review.md`, `iteration-v2/check-gates.md`).

So "don't resubmit the rejected approach unchanged" does not apply to the code: the approach
was not rejected. Changing working, reviewed code just to make the diff look different would
cost a review round and fix nothing. The gate cannot turn green from inside #686's scope, so
re-running the cycle again will hit the same red until a human acts on it.

## The failing gate is on `main`, not in this patch

Run on the **clean** base worktree (`git status --short` empty, HEAD `7bf7eb9`), before
applying any part of the patch, with cargo-deny 0.20.2 and the same arguments xtask uses
(`xtask/src/lib.rs:166-173`):

```
$ git status --short | wc -l
0
$ git rev-parse --short HEAD
7bf7eb9
$ timeout 600 cargo deny --all-features --config deny-all-features.toml check advisories
error[vulnerability]: `Debug` impls for client session values expose TLS secrets
    ├ ID: RUSTSEC-2026-0346
    ├ rustls v0.21.12
      ├── tokio-rustls v0.24.1
error[vulnerability]: Unauthenticated ServerHello can downgrade later connections to classical key exchange
    ├ ID: RUSTSEC-2026-0349
    ├ rustls v0.21.12
      ├── tokio-rustls v0.24.1
advisories FAILED
exit=1
```

(The full round-2 output shows the chain: rustls 0.21.12 ← tokio-rustls 0.24.1 / tonic 0.10.2
← tikv-client 0.4.0 ← wyrd-metadata-tikv, `iteration-v2/gate-logs/C4-ci.log:3478-3500`.)

The patch touches none of `Cargo.toml`, `Cargo.lock`, `deny.toml`, `deny-all-features.toml`
(`git diff --stat`: only `crates/core/src/metadata.rs`, `crates/core/src/read.rs`,
`crates/server/src/lib.rs`, `docs/design/architecture/06-runtime-view.md` and the new test).
Upstream has no issue or PR about either advisory yet (`gh search issues --repo
getwyrd/wyrd "RUSTSEC-2026-0346"` and `…0349` both return `[]`; `gh pr list --state open`
returns `[]`). The parent tracker #543 is closed.

The advisories' own fix is "Upgrade to >=0.23.46". The 0.21 line is what tikv-client 0.4.0
pins through tonic 0.10, so `cargo update` cannot reach a fixed version. That leaves an ignore
entry as the only way to green this step, which is a supply-chain decision (ADR-0003), and
the rubric's out-of-scope rule says a real finding outside the PR's scope gets an issue
reference, not an in-PR fix. So I did **not** put it in `patch.diff`.

### For the human: the separate change that would green the gate

If you decide to waive these two the same way as the other seven tikv-client advisories,
this is what a separate PR on `main` needs (three edits, roughly +12 lines). I have not
judged whether either advisory is reachable through tikv-client's TLS client; that call is
yours.

1. `deny-all-features.toml`, after the RUSTSEC-2026-0258 entry (base `:152`), two entries in
   the existing style, e.g.

   ```toml
   { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12 via tokio-rustls 0.24 / tonic 0.10 <- tikv-client 0.4.0; fixed only in rustls >= 0.23.46, unreachable from tikv-client 0.4.0. Absent from the shipped graph (#543)." },
   { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, same tikv-client-only chain as RUSTSEC-2026-0346 (#543)." },
   ```

2. `xtask/tests/fdb_harness.rs:1241-1254` — add both IDs to the list in
   `the_shipped_artifact_wall_carries_no_off_by_default_backend_exception`, so neither can
   leak into `deny.toml` (the same reason commit `4323067` added RUSTSEC-2026-0258).
3. Update the counts "seven" → "nine" in `deny-all-features.toml:52`, `:105` and the test
   comment at `xtask/tests/fdb_harness.rs:1240`.

Then re-run the C4-ci gate for this bundle.

I did not use the `NEEDS-HUMAN external dependency` marker. Nothing is missing: the tools
are installed and the patch builds and runs. With `dependency_halt = true`
(`pdca.toml:239`) the marker would also send the bundle down the "unverifiable" fast path
and skip review, which would be wrong for a patch that verifies fine.

Suggestion for Act (not this bundle): a WARN doctor row that runs this same advisory scan on
the base checkout would have shown the red before Do started, instead of after three full
build-and-review rounds.

### Incomplete gate evidence from round 2 (C4-diff-cov)

Round 2's diff-coverage gate measured the four new core tests (all ran), then timed out at
7200 s while running `cargo llvm-cov test -p wyrd-server` over the whole server suite
(`iteration-v2/gate-logs/C4-diff-cov.log:7`, `:33`). That is the gate's own runtime, not
something in the patch (the server change is 27 lines, `crates/server/src/lib.rs:427-454`).
I can't shorten it from the builder side.

## The change (unchanged from iteration 2)

- **`metadata::resolve_chunk_map_owned`** (`crates/core/src/metadata.rs:3117-3169`): same
  resolution rules as `resolve_chunk_map` (`:3040`), but takes the `InodeRecord` by value.
  - Flat: the record is destructured (`:3122-3130`) and `ChunkMap::Flat(chunks)` is moved
    straight into the answer (`:3134-3143`).
  - Segmented: the same `read_segments` call the borrowing path makes (`:3146`); the list
    is built by `assemble_segments` (`:3010-3016`), pulled out of base `resolve_snapshot`
    (base `:3010-3019`) so both entries share the table-order assembly.
  - Superseded: hands off to `resolve_current_chunk_map` (`:3155-3166`), unchanged. Its
    `ResolvedChunkMap<'static>` always has both `Cow`s `Owned` (`:3206-3207`), so the two
    `into_owned()` calls there only unwrap. The restart path's own copy of a flat
    successor's list (`:3206`, base `:3091`) is left alone, as the brief scopes it out.
  - Gone: `Ok(None)` (`:3167`).
- **`OwnedResolvedChunkMap`** (`metadata.rs:3066-3080`): `size`, `version`, `etag`,
  `content_type`, `modified`, `chunks: Vec<ChunkRef>`, plus a private `framed_by`
  (`:3085-3103`) for the restart arm.
- **Streamed GET** (`crates/server/src/lib.rs:418-454`): calls
  `resolve_chunk_map_owned(.., inode)` (`:432`) with the owned inode from
  `read::committed_inode` (`:419`), moves the framing out of the answer (`:445-451`) and
  moves the list into `chunk_map` (`:454`), which the reader task only borrows (`:474`). The
  wrong comment at base `:447-450` is replaced by one that says what the code does
  (`:453`), and `:427-430` says why this read uses the owning entry.
- **Doc comments kept true:** `ChunkMap::into_flat` (base `metadata.rs:1267-1276`) deleted —
  zero callers anywhere in the tree (`grep -rn into_flat crates` is empty), and its doc named
  a consumer that no longer uses it. `InodeRecord::chunk_map` (`metadata.rs:1604-1611`) lists
  the owning entry. `resolve_chunk_map` (`:3018-3023`) names its borrowing consumers and
  points owners at the new entry. `read::committed_inode` (`crates/core/src/read.rs:573-577`)
  names both entries. `docs/design/architecture/06-runtime-view.md:29`: "one call" → "one
  resolver".

## How the record's framing is kept (for check (ii))

The record is **never kept whole and never cloned**. `resolve_chunk_map_owned` destructures
it once (`metadata.rs:3122-3130`: `let InodeRecord { size, version, etag, content_type,
modified, chunk_map, state: _ } = record;`), moves each framing field into
`OwnedResolvedChunkMap`, and moves the flat `Vec<ChunkRef>` into `chunks` (`:3141`). The
destructure has no `..`, so a new `InodeRecord` field breaks the build here instead of being
dropped silently. The server then moves `resolved.etag` / `resolved.content_type` out (base
`lib.rs:443-444` cloned them off a borrowed record) and copies the `Copy` fields `size` and
`modified` (`lib.rs:445-451`). From `read::committed_inode` (`lib.rs:419`) to the reader task
(`:474`) there is no `.clone()`, `to_owned()`, `into_owned()` or `to_vec()` on an
`InodeRecord`, a `ChunkMap` or a `[ChunkRef]`. The only `Arc::clone` (`lib.rs:452`) clones
the gateway handle, not the record. (The `into_owned()` calls at `metadata.rs:3162-3163` are
on the restart arm only, and unwrap `Cow::Owned` values.)

## Alternatives ruled out (same as iteration 2, still holds)

- **Make `resolve_chunk_map` take `InodeRecord` by value.** Eight other call sites (custodian
  gc, backfill, rebalance, reconstruction, restore, the ranged GET, `read::read_object`,
  custodian tests) borrow on purpose; the brief puts them out of scope.
- **Call `inode.chunk_map.into_flat()` in the server before the resolver, as `bbdb7c5`
  did.** Puts a second "flat or segmented?" decision in the gateway, which PR #683 removed.
- **Add the advisory ignores to this patch.** Would green the gate, but bundles a security
  waiver into a performance fix without a human's call. See above.
- **Pin the server call site with a server-side test** (deferred finding 4 in
  `deferred-findings.json`). The driver holds that one for the human at handover, and the
  brief accepts source review for the wiring (`brief.md:34-47`). A pin would need a new
  public item in `wyrd-server` whose only purpose is the test; I left that for the human to
  decide.

## Verification (this round)

No single-test wrapper exists; the gate runner is `./engine/xtask.sh` (`cargo xtask`). I ran
the targeted tests with `cargo test` under `timeout`, and the full gate through the wrapper.

**Green with the fix:**

```
$ timeout 1500 cargo test -p wyrd-core --test streamed_read_chunk_map_ownership
running 4 tests
test a_flat_record_hands_over_its_own_chunk_list_by_move ... ok
test a_segmented_record_answers_its_segments_in_table_order ... ok
test a_record_deleted_mid_resolve_answers_no_such_object ... ok
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

**Demonstrated red, mutant 1** — the flat arm's `chunks,` (`metadata.rs:3141`) changed to
`chunks: chunks.to_vec(),`, which is what base's `Cow::Borrowed(..).into_owned()` does:

```
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' (1228544) panicked at crates/core/tests/streamed_read_chunk_map_ownership.rs:139:5:
assertion `left == right` failed: the chunk list is the record's own allocation (moved), not a copy
  left: 0x7f2df4014b10
 right: 0x7f2df4014e50
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
exit=101
```

**Demonstrated red, mutant 2** — keep the outer list, copy only each placement vector
(`chunks.into_iter().map(|c| ChunkRef { placement: c.placement.clone(), ..c }).collect()`):

```
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' (1229002) panicked at crates/core/tests/streamed_read_chunk_map_ownership.rs:144:5:
assertion `left == right` failed: the first placement vector is the record's own allocation (moved), not a copy
  left: 0x77d3a40186b0
 right: 0x77d3a4016410
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
exit=101
```

After each mutant I restored `metadata.rs` from a saved copy and confirmed the working-tree
diff has the same content as iteration 2's patch (`diff <(sort now) <(sort v2)` empty; only
the order of files in the diff differs, because `git diff` sorts the new test path before
`crates/server`).

**Full revert** (production back to base, test kept): does not compile (`error[E0432]:
unresolved import wyrd_core::metadata::resolve_chunk_map_owned`), as round 2's C4-verify
showed (`iteration-v2/gate-logs/C4-verify.log`). C4-verify will report UNVERIFIABLE (77)
again; the brief declares this up front (`brief.md:106-119`). The mutants above are the
substitute evidence.

**Existing GET suites** (`timeout 2400 cargo test -p wyrd-server --test segmented_object_read
--test s3_http_wire --test s3_range_conditional --test read_path`): read_path 4/4,
s3_http_wire 19/19, s3_range_conditional 17/17, segmented_object_read 2/2.

**Lint / format / spelling:** `cargo fmt --all -- --check` exit 0; `cargo clippy -p
wyrd-core -p wyrd-server --all-targets -- -D warnings` exit 0; `typos` 1.48.0 over all five
touched files exit 0 (the brief's one external dependency, present).

**Full gate** (`./engine/xtask.sh ci` on the patched worktree, exit 1): every step before
the advisory scan passed — `typos`, `lint_docs: OK`, `render_site` (99 pages, link audit
OK), gitlink / unsafe / blackbox guards, `cargo fmt --all -- --check`, workspace clippy,
build, `cargo test --workspace --exclude wyrd-dst` (197 `test result: ok`, 0 `FAILED`),
`cargo-machete` (no unused deps), `cargo deny check` (`advisories ok, bans ok, licenses ok,
sources ok`). It then stopped at `cargo deny --all-features --config deny-all-features.toml
check advisories` with exactly RUSTSEC-2026-0346 and RUSTSEC-2026-0349 — the same output as
the clean-base run above. `ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so
I ran the later steps through the same wrapper: `./engine/xtask.sh conformance` → `5 valid +
6 invalid vectors pass` (exit 0); `./engine/xtask.sh statics` → `no DST-reachable shared
mutable global state (ADR-0035)` (exit 0); `./engine/xtask.sh dst` → exit 0, every suite `ok`, 0 `FAILED` (commit_ambiguity 14,
concurrency 5, conformance 3, coordination 12, custodian 33 in 1217 s, network 8,
no_fdb_linkage 9, tikv_await_commit_interleaving 1).
The deploy guard (`run_orchestrator_guard`, `xtask/src/main.rs:1364`) has no standalone
subcommand; it scans `crates/` for orchestrator/k8s imports. The only new `use` lines in the
patch are in the new test file (`wyrd_core`, `wyrd_metadata_redb`, `wyrd_traits`), so it
cannot trip.

## Refute-your-own-test answers

- **(a) Genuine red?** Yes, against copying implementations. Mutant 1 (copy the list, what
  base does) fails the outer-pointer check at `streamed_read_chunk_map_ownership.rs:139`;
  mutant 2 (copy only the placements) fails the placement-pointer check at `:144`. A full
  revert cannot compile the test, which the brief expects. A record clone that is then
  dropped is invisible to any pointer test; that is check (ii), covered by the destructure
  described above.
- **(b) Production path?** Yes. The tests call `wyrd_core::metadata::resolve_chunk_map_owned`,
  the function `get_object_streaming` calls (`crates/server/src/lib.rs:432`), over a real
  redb `MetadataStore` (`RedbMetadataStore::in_memory`). No mock and no copy of the logic.
  The server call site itself is pinned by source review only (see deferred finding 4).
- **(c) Fixture includes the fault?** Yes. The flat list has 3 chunks, each with a 3-entry
  `placement`, and the test asserts both are non-empty before resolving
  (`streamed_read_chunk_map_ownership.rs:123-130`), so a copy always lands at a new heap
  address. The superseded case really drops the snapshot's `seg:` rows and replaces the
  root, so the restart branch runs; the deleted case really removes the root.

## Rubric self-check

- Clock: no clock reads added.
- Seams / dependency direction: the new function sits in core beside the resolver; the server
  calls it through `metadata::`. No new trait, no new dependency.
- `#![forbid(unsafe_code)]` is at the top of the new test file (`:22`).
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. The one
  living-architecture sentence the change made slightly off ("one call") is updated.
- Absent entries: `Gone` / `Superseded` / absent-segment handling is the existing resolver's,
  reached through the same `read_segments` / `resolve_current_chunk_map` calls, and the `Gone`
  arm has a test. No new silent skip.
- Await discipline: no new awaits beyond the resolver calls the old path already made.
- DST: not a new destructive or concurrent path; it is the existing read resolution with
  ownership changed, and the restart logic is reused unchanged.
- Workflow edits / dependency manifests: none.
