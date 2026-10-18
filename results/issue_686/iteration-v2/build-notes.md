# Build notes — issue 686 / streamed-get-moves-flat-chunk-map (iteration 2)

Target: getwyrd/wyrd @ main, worktree base `7bf7eb9`. Line numbers are on the patched
tree unless marked "base".

## What this iteration changed, and why

The iteration-1 implementation passed review (C3 Change, C5 Causal adequacy, T1, T2, T3 all
PASS; the code review found nothing to fix). So I kept it as it was and addressed only the
carry-forward:

1. **Three doc comments that no longer matched the code** (the adversary's finding; the same
   defect class as #686 itself).
   - `ChunkMap::into_flat` (base `crates/core/src/metadata.rs:1267-1276`): **deleted**. Its
     doc named the streamed read as its consumer; this fix gives that read the owning
     resolver, so the method had no caller at all. I grepped the whole worktree, including
     `crates/dst` and every `tests/` dir: zero callers. Keeping a public method with no
     caller would also invite a store-holding consumer to bypass the resolver, which the
     `InodeRecord::chunk_map` doc forbids (it answers `None` on a segmented map instead of
     resolving it). Cost: -11 lines. The one doc link to it, in
     `resolve_chunk_map_owned`'s doc, now cites the commit instead
     (`metadata.rs:3112-3113`).
   - `InodeRecord::chunk_map` (`metadata.rs:1604-1611`): now lists `resolve_chunk_map_owned`
     next to `resolve_chunk_map` and `resolve_current_chunk_map` as the ways to resolve
     the map.
   - `resolve_chunk_map` (`metadata.rs:3018-3023`): no longer claims the gateway's
     streaming read. It names its borrowing consumers (`read_object`, the ranged read) and
     points an owning consumer at `resolve_chunk_map_owned`, with the reason.
2. **Two more spots of the same kind that I found while checking:**
   - `read::committed_inode` (`crates/core/src/read.rs:573-577`) said its record resolves
     through `resolve_chunk_map`. One of its callers (the streamed GET,
     `crates/server/src/lib.rs:419`) now uses the owning entry, so it names both.
   - `docs/design/architecture/06-runtime-view.md:29`: "one call turns a committed root
     into that ordered list" became "one resolver turns …". There are now two public
     entries, but they share the same segment read (`read_segments`), the same assembly
     (`assemble_segments`, `metadata.rs:3010`) and the same restart
     (`resolve_current_chunk_map`), so "single-sourced" still holds.
3. **Server comment tidy** (`crates/server/src/lib.rs:427-430`, `:437-440`): the first
   comment sentence was a fragment ("The OWNING entry, because …"); the stray blank `//`
   line between two comment paragraphs is gone. No code change there.
4. **One more test**, `a_record_deleted_mid_resolve_answers_no_such_object`
   (`crates/core/tests/streamed_read_chunk_map_ownership.rs:208`). The adversary and
   diff-coverage both noted that the owning resolver's `Gone` arm
   (`metadata.rs:3167`) was never run. The test holds a segmented snapshot whose root was
   deleted (its `seg:` rows still present, since the root always moves first) and asserts
   `Ok(None)`.

The rest of the change is the iteration-1 implementation, unchanged:

- **`metadata::resolve_chunk_map_owned`** (`crates/core/src/metadata.rs:3117`): same
  resolution rules as `resolve_chunk_map` (`:3040`), but takes the `InodeRecord` by value.
  - Flat: the record is destructured (`:3122-3130`) and `ChunkMap::Flat(chunks)` is moved
    straight into the answer (`:3134-3143`).
  - Segmented: the same `read_segments` call `resolve_snapshot` makes (`:3146`), and the
    list is built by `assemble_segments` (`:3010`), pulled out of base `resolve_snapshot`
    (base `:3010-3019`) so both entries share the table-order assembly.
  - Superseded: hands off to `resolve_current_chunk_map` (`:3155-3166`), unchanged. Its
    `ResolvedChunkMap<'static>` always has both `Cow`s `Owned` (`:3206-3207`), so the
    `into_owned()` calls there only unwrap. The restart path's own copy of a flat
    successor's list (`:3206`, base `:3091`) is left alone, as the brief scopes it out.
  - Gone: `Ok(None)` (`:3167`).
- **`OwnedResolvedChunkMap`** (`metadata.rs:3067`): `size`, `version`, `etag`,
  `content_type`, `modified`, `chunks: Vec<ChunkRef>`, plus a private `framed_by`
  (`:3085`) for the restart arm.
- **Streamed GET** (`crates/server/src/lib.rs:418-454`) calls
  `resolve_chunk_map_owned(.., inode)` (`:432`) with the owned inode from
  `read::committed_inode` (`:419`), moves the framing out of the answer (`:445-451`) and
  moves the list into `chunk_map` (`:454`), which the reader task only borrows (`:474`).
  The wrong comment at base `:447-450` is replaced by one that says what the code does
  (`:453`).

## How the record's framing is kept (for check (ii))

The record is **never kept whole and never cloned**. `resolve_chunk_map_owned` destructures
it once (`metadata.rs:3122-3130`: `let InodeRecord { size, version, etag, content_type,
modified, chunk_map, state: _ } = record;`), moves each framing field into
`OwnedResolvedChunkMap`, and moves the flat `Vec<ChunkRef>` into `chunks`. The destructure
has no `..`, so a new `InodeRecord` field breaks the build here instead of being dropped
silently. The server then moves `resolved.etag` / `resolved.content_type` out (base
`lib.rs:443-444` cloned them off a borrowed record) and copies the two `Copy` fields `size`
and `modified` (`lib.rs:445-451`). Between `read::committed_inode` and the reader task there
is no `.clone()`, `to_owned()`, `into_owned()` or `to_vec()` on an `InodeRecord`, a
`ChunkMap` or a `[ChunkRef]`. The only `Arc::clone` (`lib.rs:452`) clones the gateway
handle, not the record.

Why a separate type instead of `ResolvedChunkMap { record: Cow::Owned(record), .. }`: the
record's `chunk_map` is the list being moved, so the record would either have to be cloned
(the copy the brief forbids) or carry an emptied map that no longer says what the object
holds.

## Alternatives ruled out

- **Make `resolve_chunk_map` take `InodeRecord` by value.** It has eight other call sites
  (custodian gc, backfill, rebalance, reconstruction, restore, the ranged GET at
  `lib.rs:523`, `read::read_object` at `read.rs:528`, and custodian tests) that borrow on
  purpose. The brief puts them out of scope.
- **Call `inode.chunk_map.into_flat()` in the server before the resolver, as `bbdb7c5`
  did.** That puts a second "flat or segmented?" decision in the gateway, which PR #683
  removed by routing every consumer through the shared resolver.
- **Keep `into_flat` and only fix its comment.** The carry-forward allowed either. I deleted
  it because nothing calls it (see above); a sibling PR in this batch that wanted it would
  get a plain compile error, and none of the batch's stated scopes (#666, #671, #768: PUT
  path and a separate impl block) reads a chunk list.
- **Refactor the owning resolver to remove the three repeated struct literals.** Possible
  with a small helper, but the code review called the iteration-1 shape fine, and churning
  reviewed code for style would cost a review round for no behaviour change.

## The failing C4-ci gate: not caused by this patch, not fixable inside it

The carry-forward's failing gate is `cargo deny --all-features --config
deny-all-features.toml check advisories`. It fails on RUSTSEC-2026-0346 and
RUSTSEC-2026-0349 against `rustls 0.21.12`, reached only through `tikv-client 0.4.0`
(behind the off-by-default `tikv` feature). I re-checked:

- The patch changes no `Cargo.toml`, `Cargo.lock`, `deny.toml` or `deny-all-features.toml`
  (`git diff --stat`: only `crates/core/src/metadata.rs`, `crates/core/src/read.rs`,
  `crates/server/src/lib.rs`, the doc, and the new test).
- `cargo deny --offline --all-features --config deny-all-features.toml check advisories` on
  the unchanged lockfile reports exactly those two advisories and `advisories FAILED`. So
  `main` at `7bf7eb9` fails the same way.
- Clearing it means adding two ignore entries to `deny-all-features.toml` under #543
  (the existing tikv-client entries are at `deny-all-features.toml:105-152`, each with a
  review trigger, and an xtask quarantine-boundary test tracks those IDs, commit
  `4323067`). That is a supply-chain policy decision, outside #686's scope; the rubric says
  an out-of-scope finding gets a decline with an issue reference, not an in-PR fix. It
  needs a human: either waive this red for #686, or land the ignores on `main` under #543
  first and re-run the gate.

I did **not** use the `NEEDS-HUMAN external dependency` marker for this. This instance has
`dependency_halt = true` (`pdca.toml:239`), so the marker would send the bundle down the
"unverifiable" fast path and skip the gates and reviewers. That would be wrong here: the
patch builds and its test runs; only an unrelated advisory step on `main` is red. The item
is already held for the human in `deferred-findings.json` (item 1).

Suggestion for Act, not for this bundle: a doctor row that runs the same `cargo deny
--all-features --config deny-all-features.toml check advisories` on the base checkout
(level WARN) would have shown this red before the cycle started, instead of after a full
build and review.

## Verification

All test runs used `cargo test` with a `timeout` wrapper. The project has no single-test
wrapper; its gate runner is `./engine/xtask.sh` (`cargo xtask`), which I used for the full
gate below.

**Green with the fix:**

```
$ timeout 1200 cargo test -p wyrd-core --test streamed_read_chunk_map_ownership
running 4 tests
test a_record_deleted_mid_resolve_answers_no_such_object ... ok
test a_segmented_record_answers_its_segments_in_table_order ... ok
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test a_flat_record_hands_over_its_own_chunk_list_by_move ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Demonstrated red** (the brief's Verification posture). I changed the flat arm's `chunks,`
to `chunks: chunks.to_vec(),` (`metadata.rs:3141`), which is what base's
`Cow::Borrowed(..).into_owned()` does, ran the test, then restored the file from a saved
copy:

```
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
test a_segmented_record_answers_its_segments_in_table_order ... ok
test a_record_deleted_mid_resolve_answers_no_such_object ... ok

thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' (379398) panicked at crates/core/tests/streamed_read_chunk_map_ownership.rs:139:5:
assertion `left == right` failed: the chunk list is the record's own allocation (moved), not a copy
  left: 0x766c40014b10
 right: 0x766c40014e50

test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**Full revert** (production files back to base, test kept): the test does not compile,
`error[E0432]: unresolved import wyrd_core::metadata::resolve_chunk_map_owned`. So
C4-verify will report UNVERIFIABLE (77), as the brief declares up front. The copy-mutant
red above is the substitute evidence; the iteration-1 reviewer accepted that same
substitute (C2, NEEDS-HUMAN for sign-off).

**Existing GET suites** (`cargo test -p wyrd-server --test segmented_object_read --test
s3_http_wire --test s3_range_conditional --test read_path`): 2 + 19 + 17 + 4 passed.

**Full gate** (`./engine/xtask.sh ci`, i.e. `cargo xtask ci`, on the patched worktree):
typos, lint_docs, render_site, the gitlink/unsafe/blackbox guards, `cargo fmt --check`,
workspace clippy, build, `cargo test --workspace --exclude wyrd-dst` (197 suites ok, none
failed), cargo-machete and `cargo deny check` all passed. It then stopped at the
all-features advisory check above. The steps after it never run in `ci`
(`xtask/src/main.rs:1683-1687`), so I ran them through the same wrapper:
`./engine/xtask.sh conformance` passed (5 valid + 6 invalid vectors), `./engine/xtask.sh
statics` passed, and `./engine/xtask.sh dst` passed (exit 0; every suite ok, custodian
33/33 in 1245 s). My first DST attempt was cut off by my own 10-minute background limit
while still running, not by a failure; the rerun above had a 90-minute limit. The deploy guard
(`run_orchestrator_guard`) scans for orchestrator imports; the patch adds none.

Also: `cargo fmt --all -- --check` clean (I ran `cargo fmt --all`, which reflowed one
assert in the test); `cargo clippy -p wyrd-core -p wyrd-server --all-targets -- -D
warnings` clean; `cargo doc -p wyrd-core` has no warnings in any line this patch touches
(the crate has pre-existing rustdoc warnings elsewhere, e.g. `erasure.rs:2`).

## Refute-your-own-test answers

- **(a) Genuine red?** Yes, against a copying implementation: with the flat arm changed to
  `.to_vec()` the flat test fails on the outer-pointer assertion
  (`streamed_read_chunk_map_ownership.rs:139`, output above). A full revert cannot compile
  the test, which the brief expects (C4-verify UNVERIFIABLE). The iteration-1 adversary
  also showed that cloning only each `placement` (keeping the outer buffer) goes red on the
  placement-pointer check (`:144`).
- **(b) Production path?** Yes. The tests call `wyrd_core::metadata::resolve_chunk_map_owned`,
  the function `get_object_streaming` calls (`crates/server/src/lib.rs:432`), over the
  real redb `MetadataStore` (`RedbMetadataStore::in_memory`). No mock, no copy of the
  logic.
- **(c) Fixture includes the fault?** Yes. The flat list has 3 chunks, each with a 3-entry
  `placement`, so a copy gets a new heap address (the test asserts both non-empty before
  resolving). The superseded case really drops the snapshot's `seg:` rows and replaces the
  root, so the restart branch runs. The deleted case really removes the root. Gap, as the
  brief says: the test cannot see a record clone that was then dropped; that is check (ii),
  a source review, and the destructure described above is what makes it hold.

## Rubric self-check

- Clock: no clock reads added.
- Seams / dependency direction: the new function lives in core beside the resolver; the
  server calls it through `metadata::`. No new trait.
- `#![forbid(unsafe_code)]` is at the top of the new test file (`:22`).
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. The one
  living-architecture sentence that the change made slightly off ("one call") is updated.
- Absent entries: `Gone` / `Superseded` / absent-segment handling is the existing
  resolver's, reached through the same `read_segments` / `retired_or` calls, and the `Gone`
  arm now has a test. No new silent skip.
- Await discipline: no new awaits beyond the resolver calls the old path already made.
- DST: not a new destructive or concurrent path; it is the existing read resolution with
  ownership changed, and the restart logic is reused unchanged.
