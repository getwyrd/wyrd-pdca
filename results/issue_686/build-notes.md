# Build notes — issue 686 / streamed-get-moves-flat-chunk-map (iteration 4)

Target: getwyrd/wyrd @ main, worktree base `7bf7eb9` (still upstream `main`'s tip:
`git ls-remote origin refs/heads/main` → `7bf7eb906c63…` on 2026-10-15). Line numbers are
on the patched tree unless marked "base".

## Short version

The code is the same as iteration 3 except for one two-line comment the round-3 adversary
called loose (`crates/core/src/metadata.rs:3159-3160`). Round 3's review found no defect
(C3, C5, T1, T2, T3, T5 PASS; batch review 0 blocking; adversary "could not refute").

The only gating red is `cargo deny --all-features --config deny-all-features.toml check
advisories`. It fails on `main` without this patch, on two rustls advisories published
2026-10-14, and the only fix is a supply-chain waiver that a human has to decide on. This
round I **declare it as a NEEDS-HUMAN blocker** (below) so the cycle stops here for you,
instead of spending round 5 (the more expensive builder) to hit the same red again.

## What changed from iteration 3, and why

Three rounds rebuilt for the same gate. Round 3's builder chose not to use the
NEEDS-HUMAN marker, so that the patch would still get a full review. That review has now
happened three times, and every reviewer and the adversary said the red is out of this
diff's scope and asked that it not be routed back to Do (`deferred-findings.json` items 1
and 5). The auto-iterate loop still routed it back, because C4-ci is gating and
`verifies_base = false` on that row (`pdca.toml:1468`), so the driver can't tell a red
`main` from a red patch. With `max_auto_iters = 5` (`pdca.toml:96`) and the count at 3,
shipping the same code without the marker buys one more expensive round and the same red.

So this round:

1. **Declared the blocker** with the marker, plus a doctor row whose detect command I
   tested (exit 1 on the current base, offline, ~0.5 s). With `dependency_halt = true`
   (`pdca.toml:239`) the driver checks the claim by running that command; if it exits
   non-zero the bundle goes straight to sign-off and stays resumable.
2. **Tested the waiver change** you would land separately, so you have real evidence
   rather than a sketch: with the two ignore entries added, the scan passes. I removed
   them again; they are **not** in `patch.diff`.
3. **Fixed the loose comment** the round-3 adversary noted.

To be clear about what this marker is: nothing is missing from the host. Every tool is
installed and the patch builds, tests and lints clean. The blocker is a state of the
target's base (`main` fails one gate step) that only a maintainer decision can change. I
used the marker because it is the one way a builder can stop the loop and hand the
decision to you. If you think the full Check should still run on this round, refute it at
sign-off and answer iterate-do; the code is the same one round 3 reviewed.

NEEDS-HUMAN external dependency: deny-all-features-advisories — `main` (`7bf7eb9`) fails the gating C4-ci step `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (rustls 0.21.12 ← tokio-rustls 0.24.1 / tonic 0.10.2 ← tikv-client 0.4.0 ← wyrd-metadata-tikv). No patched rustls exists on the 0.21 line (both advisories: `patched = [">= 0.23.46"]`), tikv-client 0.4.0 can't reach 0.23, and this patch changes no dependency or deny file. A green C4-ci needs a maintainer to waive the two advisories under #543 (tested change below) or accept this patch with the red recorded as unrelated.

```toml
[[doctor.checks]]
group = "engine"
id    = "deny-all-features-advisories"
cmd   = "cd \"${WYRD_REPO:-../wyrd}\" && cargo deny --offline --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1"
hint  = "the target's base fails the all-features advisory wall (ADR-0003, #543), so the gating C4-ci row goes red on EVERY bundle for a reason in none of them. Land the advisory decision on main first (an ignore entry in deny-all-features.toml + the fdb_harness list, or a dependency bump), then re-gate"
level = "WARN"
```

Notes on the row: `--offline` keeps it a cached read (no network, no fetch), as the doctor
section asks (`pdca.toml:1020-1025`). It runs against `${WYRD_REPO:-../wyrd}` like the other
target rows (`pdca.toml:1361`); `../wyrd` is at `7bf7eb9` on `main` today. Tested from the
project root:

```
$ sh -c 'cd "${WYRD_REPO:-../wyrd}" && cargo deny --offline --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1'; echo "row exit=$?"
row exit=1
real 0m0.516s
(without the redirect:)    ├ ID: RUSTSEC-2026-0346
                           ├ ID: RUSTSEC-2026-0349
                           advisories FAILED
```

(My first try used `check --disable-fetch`, which cargo-deny 0.20.2 rejects with exit 2 —
a wrong-flag exit would have "confirmed" the blocker for the wrong reason. `--offline` is a
root option in 0.20.2; the row above uses it.)

For Act (not this bundle): this row, run by `pdca doctor` before a batch, would have shown
the red `main` before Do started instead of after four build-and-review rounds. A
base-red check on C4-ci (`verifies_base`) would stop the auto-iterate loop from rebuilding
for a red the patch can't touch; that one is a harness question for upstream.

## The failing step, re-run this round

On the patched worktree (same result as the clean-base run recorded in round 3):

```
$ timeout 600 cargo deny --all-features --config deny-all-features.toml check advisories
error[vulnerability]: `Debug` impls for client session values expose TLS secrets
    ├ ID: RUSTSEC-2026-0346
    ├ rustls v0.21.12
      ├── tokio-rustls v0.24.1
      │       └── tikv-client v0.4.0
error[vulnerability]: Unauthenticated ServerHello can downgrade later connections to classical key exchange
    ├ ID: RUSTSEC-2026-0349
    ├ rustls v0.21.12
advisories FAILED
exit=1
$ git diff --stat -- Cargo.toml Cargo.lock deny.toml deny-all-features.toml | wc -l
0
```

The advisory texts (local RustSec cache, `crates/rustls/RUSTSEC-2026-0346.md`, `…0349.md`):
0346 is CVSS AV:L/PR:H (secrets in `Debug` output of client session values, only hit by a
custom `ClientSessionStore` that logs them); 0349 is CVSS AC:H, C:L (a network attacker can
downgrade later TLS 1.3 connections to classical-only key exchange via the resumption
store). I have not judged whether tikv-client's TLS client reaches either; that is part of
your call.

### Tested: the separate change that greens the step (not in this patch)

With these two entries added after `deny-all-features.toml:152` (base), the same command
prints `advisories ok` and exits 0. I then restored the file with `git checkout --`
(`git status` shows only the patch's files):

```toml
{ id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12 via tokio-rustls 0.24 / tonic 0.10 <- tikv-client 0.4.0; patched only in rustls >= 0.23.46, unreachable from tikv-client 0.4.0. Absent from the shipped graph (#543)." },
{ id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, same tikv-client-only chain as RUSTSEC-2026-0346 (#543)." },
```

A full PR for that would also: add both IDs to the list in
`the_shipped_artifact_wall_carries_no_off_by_default_backend_exception`
(`xtask/tests/fdb_harness.rs:1241-1254`, base) so neither can leak into `deny.toml`, and
change "seven" to "nine" at `deny-all-features.toml:52`, `:105` and
`xtask/tests/fdb_harness.rs:1240` (base). (I only ran the scan with the probe entries; I did
not run the xtask test with the list change.)

Why it is not in `patch.diff`: it is a security waiver, and bundling it into a performance
fix would take the decision away from you and from the upstream maintainers. The rubric's
out-of-scope rule also asks for a decline with an issue reference rather than an in-PR fix.
No upstream issue exists for either ID yet (round 3 searched; #543, the parent tracker, is
closed), so whoever lands the waiver should open one.

## The change

Same design as rounds 2 and 3.

- **`metadata::resolve_chunk_map_owned`** (`crates/core/src/metadata.rs:3117-3169`): the
  same resolution rules as `resolve_chunk_map` (`:3040`), but it takes the `InodeRecord` by
  value.
  - Flat: the record is destructured (`:3122-3130`) and `ChunkMap::Flat(chunks)` is moved
    straight into the answer (`:3134-3143`). This restores what `bbdb7c5` did with
    `ChunkMap::into_flat` (base `metadata.rs:1267-1276`).
  - Segmented: the same `read_segments` call the borrowing path makes (`:3146`); the list
    is built by `assemble_segments` (`:3010-3016`), pulled out of `resolve_snapshot`
    (base `:3010-3019`) so both entries share the table-order assembly.
  - Superseded: hands off to `resolve_current_chunk_map` (`:3155-3166`), unchanged. That
    function always builds both `Cow`s as `Owned` (`:3206-3207`), so the two
    `into_owned()` calls only unwrap. The restart path's own copy of a flat successor's
    list (`:3206`, base `:3091`) is left alone, as the brief scopes it out.
  - Gone: `Ok(None)` (`:3167`).
- **`OwnedResolvedChunkMap`** (`metadata.rs:3055-3080`): `size`, `version`, `etag`,
  `content_type`, `modified`, `chunks: Vec<ChunkRef>`, plus a private `framed_by`
  (`:3082-3104`) for the restart arm.
- **Streamed GET** (`crates/server/src/lib.rs:418-454`): calls
  `resolve_chunk_map_owned(.., inode)` (`:432`) with the owned inode from
  `read::committed_inode` (`:419`), moves the framing out of the answer (`:445-451`), and
  moves the list into `chunk_map` (`:454`), which the reader task only borrows (`:474`). The
  wrong comment at base `:447-450` is replaced by one that says what the code does (`:453`);
  `:427-430` says why this read uses the owning entry.
- **Doc comments kept true:** `ChunkMap::into_flat` (base `metadata.rs:1267-1276`) is
  deleted — no caller anywhere (`grep -rn into_flat crates docs` on the patched tree: 0
  hits) and its doc named a consumer that no longer uses it. `InodeRecord::chunk_map`
  (`metadata.rs:1604-1612`) lists the owning entry. `resolve_chunk_map` (`:3018-3023`) names
  its borrowing consumers and points owners at the new entry. `read::committed_inode`
  (`crates/core/src/read.rs:573-577`) names both entries.
  `docs/design/architecture/06-runtime-view.md:29`: "one call" → "one resolver".
- **New this round:** the restart-arm comment (`metadata.rs:3159-3160`) now gives the real
  reason both `Cow`s are `Owned` — `resolve_current_chunk_map` builds them that way — not
  the `'static` lifetime, which on its own does not rule out `Cow::Borrowed` (round-3
  adversary, minor).

Size: 362 added / 43 removed lines in `patch.diff`, of which 224 are the new test file.

## How the record's framing is kept (for check (ii))

The record is **never kept whole and never cloned**. `resolve_chunk_map_owned` destructures
it once (`metadata.rs:3122-3130`: `let InodeRecord { size, version, etag, content_type,
modified, chunk_map, state: _ } = record;`), moves each framing field into
`OwnedResolvedChunkMap`, and moves the flat `Vec<ChunkRef>` into `chunks` (`:3141`). The
destructure has no `..`, so a new `InodeRecord` field breaks the build here instead of being
dropped silently. The server then moves `resolved.etag` / `resolved.content_type` out (base
`lib.rs:443-444` cloned them off a borrowed record) and copies the `Copy` fields `size` and
`modified` (`lib.rs:445`, `:451`). From `read::committed_inode` (`lib.rs:419`) to the reader
task (`:474`) there is no `.clone()`, `to_owned()`, `into_owned()` or `to_vec()` on an
`InodeRecord`, a `ChunkMap` or a `[ChunkRef]`. The only `Arc::clone` (`lib.rs:452`) clones
the gateway handle, not the record. (The `into_owned()` calls at `metadata.rs:3162-3163` are
on the restart arm only, and unwrap `Cow::Owned` values.)

## Alternatives ruled out

- **Make `resolve_chunk_map` take `InodeRecord` by value.** Eight other call sites (custodian
  gc, backfill, rebalance, reconstruction, restore, the ranged GET at `lib.rs:523`,
  `read::read_object`, custodian tests) borrow on purpose; the brief puts them out of scope.
- **Call `inode.chunk_map.into_flat()` in the server before the resolver, as `bbdb7c5`
  did.** Puts a second "flat or segmented?" decision back in the gateway, which PR #683
  removed.
- **Add the advisory waiver to this patch.** Would green the gate in about 4 lines
  (tested above), but bundles a security decision into a performance fix without your
  call.
- **Ship the same code without the marker.** Round 5 rebuilds on the more expensive
  builder (`pdca.toml:88-90`) and hits the same red; the loop then halts at the cap anyway.
- **Pin the server call site with a server-side test** (deferred finding 4). The driver
  holds that for you at handover, and the brief accepts source review for the wiring
  (`brief.md:34-47`). A pin needs a new public item in `wyrd-server` that exists only for
  the test; left for your decision.

## Verification (this round)

No single-test wrapper exists (`engine/` has `xtask.sh`, `run-verify.sh`, `run-diff-cov.sh`);
I ran the targeted tests with `cargo test` under `timeout`, in the worktree.

**Green with the fix:**

```
$ timeout 1500 cargo test -p wyrd-core --test streamed_read_chunk_map_ownership
running 4 tests
test a_record_deleted_mid_resolve_answers_no_such_object ... ok
test a_segmented_record_answers_its_segments_in_table_order ... ok
test a_flat_record_hands_over_its_own_chunk_list_by_move ... ok
test a_record_superseded_mid_resolve_restarts_onto_the_live_generation ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
exit=0
```

**Demonstrated red, mutant 1** — the flat arm's `chunks,` (`metadata.rs:3141`) changed to
`chunks: chunks.to_vec(),`, which is what base's `Cow::Borrowed(..).into_owned()` does:

```
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' (3156766) panicked at crates/core/tests/streamed_read_chunk_map_ownership.rs:139:5:
assertion `left == right` failed: the chunk list is the record's own allocation (moved), not a copy
  left: 0x770010014b10
 right: 0x770010014e50
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
exit=101
```

**Demonstrated red, mutant 2** — keep the outer list, copy only each placement vector
(`chunks.into_iter().map(|c| ChunkRef { placement: c.placement.clone(), ..c }).collect()`):

```
test a_flat_record_hands_over_its_own_chunk_list_by_move ... FAILED
thread 'a_flat_record_hands_over_its_own_chunk_list_by_move' (3157055) panicked at crates/core/tests/streamed_read_chunk_map_ownership.rs:144:5:
assertion `left == right` failed: the first placement vector is the record's own allocation (moved), not a copy
  left: 0x75ea480186b0
 right: 0x75ea48016410
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
exit=101
```

After each mutant I copied back the saved `metadata.rs` and checked it with `cmp`.

**Full revert** (production back to base with `git apply -R`, test kept):

```
error[E0432]: unresolved import `wyrd_core::metadata::resolve_chunk_map_owned`
error: could not compile `wyrd-core` (test "streamed_read_chunk_map_ownership") due to 1 previous error
exit=101
```

Then re-applied, and `git diff` matched the saved diff byte for byte. C4-verify will report
UNVERIFIABLE (77) again; the brief declares this up front (`brief.md:106-119`). The mutants
are the substitute evidence.

**Existing GET suites** (`timeout 2400 cargo test -p wyrd-server --test segmented_object_read
--test s3_http_wire --test s3_range_conditional --test read_path`): read_path 4/4,
s3_http_wire 19/19, s3_range_conditional 17/17, segmented_object_read 2/2, exit 0.

**Lint / format / spelling:** `cargo fmt --all -- --check` exit 0; `cargo clippy -p
wyrd-core -p wyrd-server --all-targets -- -D warnings` exit 0; `typos` 1.48.0 over all five
touched files exit 0 (the brief's one external dependency, present).

**Not re-run this round:** the full `./engine/xtask.sh ci`, `conformance`, `statics` and
`dst`. Round 3 ran all of them on code that differs from this round's only in the two
comment lines at `metadata.rs:3159-3160`: every `ci` step before the all-features advisory
scan passed (workspace tests 197 `test result: ok`, 0 `FAILED`), and `conformance`,
`statics` and `dst` each exited 0 (`iteration-v3/build-notes.md`, "Full gate"). If the
driver confirms the blocker, Check's gates do not run this round; to get fresh gate logs,
re-gate after the advisory decision.

`patch.diff` vs `iteration-v3/patch.diff`, ignoring `index` lines: the only difference is
the two comment lines.

## Refute-your-own-test answers

- **(a) Genuine red?** Yes, against copying implementations. Mutant 1 (copy the list, what
  base does) fails the outer-pointer check at `streamed_read_chunk_map_ownership.rs:139`;
  mutant 2 (copy only the placements) fails the placement-pointer check at `:144`. A full
  revert can't compile the test (E0432), which the brief expects. A record clone that is
  then dropped is invisible to any pointer test; that is check (ii), covered by the
  destructure described above.
- **(b) Production path?** Yes. The tests call `wyrd_core::metadata::resolve_chunk_map_owned`,
  the function `get_object_streaming` calls (`crates/server/src/lib.rs:432`), over a real
  redb `MetadataStore` (`RedbMetadataStore::in_memory`). No mock and no copy of the logic.
  The server call site itself is pinned by source review only (deferred finding 4).
- **(c) Fixture includes the fault?** Yes. The flat list has 3 chunks, each with a 3-entry
  `placement`, and the test asserts both are non-empty before resolving
  (`streamed_read_chunk_map_ownership.rs:123-130`), so a copy always lands at a new heap
  address. The superseded case really drops the snapshot's `seg:` rows and replaces the
  root, so the restart branch runs; the deleted case really removes the root.

## Rubric self-check

- Clock: no clock reads added.
- Seams / dependency direction: the new function sits in core beside the resolver; the
  server calls it through `metadata::`. No new trait, no new dependency.
- Metadata validation: segment decoding and checks are the existing `read_segments` path,
  unchanged.
- Global state: none added.
- `#![forbid(unsafe_code)]` is at the top of the new test file (`:22`).
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. The one
  living-architecture sentence the change made slightly off ("one call") is updated.
- Absent entries: `Gone` / `Superseded` / absent-segment handling is the existing
  resolver's, reached through the same `read_segments` / `resolve_current_chunk_map` calls,
  and the `Gone` arm has a test. No new silent skip.
- Await discipline: no new awaits beyond the resolver calls the old path already made; no
  new spawned task.
- DST: not a new destructive or concurrent path; it is the existing read resolution with
  ownership changed, and the restart logic is reused unchanged.
- Workflow edits / dependency manifests: none.
