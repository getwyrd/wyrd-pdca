# Build notes — issue 813 (staged-scrub-and-keep), round 4

Round 4 is **v3 plus the round-4 delta**, not a rebuild. I applied
`iteration-v3/patch.diff` to the worktree at `4ab2b28` (it applied cleanly, `git apply --check`
first) and changed only what the delta lists. Every `path:line` below is in the patched tree
unless it says "base" or "v3".

Round-4 change against v3, per file (lines added / removed, from `diff` against a v3-only copy of
each file):

| file | +/- vs v3 | what |
| --- | --- | --- |
| `crates/custodian/src/scrub.rs` | +67/-28 | C'', C''-audit, finding (c), docs, re-queue bound |
| `crates/custodian/src/gc.rs` | +10/-6 | `StagedPartSet::malformed` keeps the `part:` key; module doc `:89-91` |
| `crates/custodian/src/reconciliation.rs` | +3/-3 | `Blocked` doc now names scrub's new case |
| `crates/custodian/src/reconstruction.rs` | +6/-4 | two comments reworded (discharge / discard), no code |
| `crates/dst/tests/custodian.rs` | +6/-5 | one comment reworded (discharge / discard), no code |
| `docs/design/architecture/06-runtime-view.md` | +1/-1 (one long line) | C'', discharge/discard, re-queue bound |
| `crates/custodian/tests/staged_scrub.rs` | +155/-39 | C'' flipped + wrong-length, counter-case, C''-audit, A'-malformed |
| `crates/custodian/tests/staged_protection.rs` | +173/-3 | G-held x2, empty queue, J-discharge |

No other file changed from v3. `patch.diff` is 218,237 bytes against v3's 200,443 (+17.8 KB),
about four-fifths of it tests (328 of 421 added lines). `run-verify.sh --classify` on it returns
`ADDED_TEST crates/custodian/tests/staged_scrub.rs` and crates `chunkstore-grpc`, `custodian`,
`dst`, `server` — the same as v3.

## The delta, item by item

### C'' — a staged chunk with a malformed part placement is never certified (review finding b)

`crates/custodian/src/scrub.rs:236-252`: the v3 precedence test ("does a committed map name this
chunk?") became one closure, `committed`, used twice — to skip a part placement a committed map
supersedes (`:239-244`, unchanged behaviour) and to compute
`staged_unchecked = staged.malformed.keys().any(|chunk| !committed(chunk))` (`:252`). The answer
at `:324-328` adds `|| staged_unchecked` to the `Blocked` condition.

So a malformed `part:` placement blocks the pass only for a chunk no committed map names. When a
committed map names the chunk (valid or malformed), the committed map's rule alone decides, as
the brief says. The malformed COMMITTED placement still answers as on base (out of scope, pinned
by `short_placement_is_malformed_scrub_fails_safe`, `crates/custodian/tests/scrub.rs:788`).

Tests (`crates/custodian/tests/staged_scrub.rs`):

- `:1161` `an_empty_committed_part_placement_is_malformed_and_queues_no_phantom_repair` — v3's
  test, assertion flipped from `Satisfied` to `Blocked` (via the shared checker at `:1124`).
- `:1168` `a_wrong_length_committed_part_placement_is_never_certified` — new: `[0, 1]` for a
  one-fragment scheme. The brief says "empty or the wrong length"; v3 only had empty.
- `:1178` `a_malformed_part_placement_a_committed_map_supersedes_does_not_block` — the
  counter-case: same damaged record, plus a published inode placing the chunk validly on server
  0 with an intact fragment. `Satisfied`, nothing queued, and the damaged record is still named.

### C''-audit — the audit line names the `part:` record

`crates/custodian/src/gc.rs:1564`: `StagedPartSet::malformed` is now
`BTreeMap<ChunkId, Vec<(Vec<u8>, MalformedPlacement)>>`, the shape of `StagedSet::held`
(`gc.rs:1336`), and `read_staged_part` pushes the key with the fault (`gc.rs:1653-1654`), as
`StagedSet::hold` does (`gc.rs:1436-1441`). A `BTreeMap` so the emission order is
deterministic.

`crates/custodian/src/scrub.rs:436-447`: new `emit_malformed_staged(record, chunk, expected,
actual)` — action `malformed-staged-placement`, a `record` field carrying the `part:` key
through `crate::gc::object_name`, the chunk, and staged wording. It has its own counter,
`scrub_malformed_staged_placement`, the same split v3 already made for
`scrub_unresolvable_staged_records` vs `scrub_unresolvable_records`. `scrub.rs:152-156` emits
it the moment the staged reading returns (v3's timing, before the `inode:` read). `emit_malformed`
(`:409-426`) is committed-map-only again; I removed v3's paragraph there about part records,
which moved into the new emitter's doc, and put back base's "non-empty but of the wrong length"
wording, which is exact for committed maps.

Test: `one_audit_event_names` (`staged_scrub.rs:1067`) asserts that ONE event carries the
`part:` key, `malformed-staged-placement` and the chunk hex together, so the record and the fault
are proven to be one signal. v3's two separate `named_on_audit_seam` checks
(`b"malformed-placement"` and the chunk hex) are replaced by it.

No living doc lists scrub's audit actions or counters (I grepped `docs/` for every scrub action
and counter name; only the restore runbook lists restore's actions, `m4-first-deployment-
blueprint.md:617-618`), so the new action and counter need no docs row.

### A'-malformed

`staged_scrub.rs:1208` `a_part_placement_a_malformed_committed_map_supersedes_is_not_checked`:
the committed inode places the chunk on `[0, 1]` (malformed for a one-fragment scheme), a part
record places it validly on server 3, where nothing is. Nothing queued, `Satisfied`. Deleting
the `referenced.malformed` half of the precedence test (now `scrub.rs:237`, v3 `:220-221`) turns
it red — shown below.

### G-held

`crates/custodian/tests/staged_protection.rs:2474` harness, legs `:2514` (part record) and
`:2527` (`sidx:` twin): an `Open` session whose record places an RS(2,1) chunk on only `[0, 1]`,
so the chunk is in `StagedSet::held`, not `placed`. One pass: still queued, `Blocked`, no D
server written, record byte-identical. No production change: v3's
`.chain(staged.held.keys().copied())` (`reconstruction.rs:244`, v3 `:242`) already does it; the
legs pin it.

### Empty queue

`segmented_map_reconstruction.rs`'s double (`:57-135`) counts only `scan(b"inode:")` calls and
`seg:` pages; its `scan_page` does not log `mpu:` / `sidx:` / `part:` reads. So, as the brief
allows, the leg lives beside G in `staged_protection.rs`, whose `Meta` logs every read:
`staged_protection.rs:2543` `an_empty_queue_reads_no_staged_record_and_answers_satisfied`. It arms
faults on `mpu:`, `sidx:`, `part:` and `inode:`, runs one pass with nothing queued, and asserts
`Satisfied` (so no read faulted) and that the read log holds no read under those four prefixes.
Green on base by design.

### J-discharge

Leg J (`staged_protection.rs:2779`) gains, with the leftover part record asserted byte-identical
after round 1, after each settle round and at the end:

- (i) `:2846`: right after round 1's reconstruction answers `Changed`,
  `wyrd_core::repair::queued_repairs` is empty — the repair discharged the obligation in its
  repoint commit although a staged record still names the chunk.
- (ii) `:2885`: after the settle rounds, `enqueue_repair` the chunk again (a duplicate health
  report) and run one reconstruction pass: `Satisfied`, queue empty, and every D server's
  fragment map byte-identical to before the pass (no fragment written).

Green on base by design. No production change.

### Prose (Scope item 5 and 6)

- `reconstruction.rs:233-241` and `:292-297`, `crates/dst/tests/custodian.rs:2596-2602`: the
  three comments that said an obligation is "removed / discarded only when no record, committed
  or staged, names its chunk" now state the split: a committed chunk is discharged against its
  committed map alone; an obligation is discarded only when no record names or holds its chunk.
  The code around them is unchanged.
- `gc.rs:89-91` (base `:89-90`): "Scrub and the drain-status surface do not read the class at
  all" was false; it now says drain status and reconstruction read the class and scrub reads its
  `part:` half through its own reader.
- `gc.rs:396`, `staged_protection.rs:34`, `staged_protection.rs:2159` / `gc.rs:1316` `deferred:
  #663` markers: already fixed by v3 (checked; `grep "deferred: #663"` over `crates/` and `docs/`
  is empty).
- `reconciliation.rs:25-28`: the `Blocked` doc said only GC blocks on a staged record. It now
  names the staged record generally, and scrub's unusable part placement. Not in the brief's
  list, but C'' makes the old text false, so I counted it as part of that item.
- Re-queue bound (Scope 6): stated in scrub's module doc (`scrub.rs:38-44`) and in
  `06-runtime-view.md:82` (the brief's `:80`; v3's line-shift), naming the `retire:records:`
  drain as the bound. `06-runtime-view.md:82` also gets C'' and the discharge / discard split: its
  v3 sentence "The repair loop keeps rather than drains an obligation for a chunk either the
  committed or the staged reading still names" had the same over-wide wording the brief asked me
  to fix in the code comments.

### Review finding (c) — `fragments` map regrouped into `by_dserver`: fixed

`scrub.rs:215-220` and `:239-244` push straight into `by_dserver`; the intermediate
`HashMap<(DServerId, FragmentId), EcScheme>` and its regroup loop are gone (6 lines removed). The
map was also a dedup, so I checked that nothing can now be pushed twice: `referenced.placed` is a
`HashSet`, `staged.placed` a `HashMap`, and the precedence skip keeps any chunk out of the staged
half when a committed map names it — so one `(dserver, fragment)` can come from only one of the
two. The comment at `:234-235` says so. Both A' legs stay green (run below).

### Review finding (a): recorded rejected

`results/issue_813/review-rejected.md`, at `scrub.rs:255` (the fleet walk's line after round 4),
per the brief.

## Red → green

### By hand, the brief's Verification posture (base production, initialisers stripped)

Base production: `crates/custodian/src/{gc,reconstruction,scrub,reconciliation}.rs` and
`crates/custodian/Cargo.toml` replaced by `git show HEAD:<file>` (HEAD = `4ab2b28`); the new
`staged_scrub.rs` kept; `staged_protection.rs` from this patch with only its two new field
initialisers (`clock: …`, `staged_write_window_millis: 0`, `:537-538`) deleted. Command:

```
cargo test --no-fail-fast -p wyrd-custodian --test staged_scrub --test staged_protection
  staged_scrub:      5 passed; 11 failed
  staged_protection: 28 passed; 8 failed
```

`staged_scrub.rs` failures, all by assertion: A x3, C x2, C' x2, C'' empty and wrong-length
(`:1137`, the `Blocked` assertion), the C'' counter-case (`:1193`, the audit key), C'''.
Passing: B, A's control, both A' legs and A'-malformed — all guards, green on base by design.

`staged_protection.rs` failures, all by assertion: G x2 (`:2368`, `:2419`), G-held x2 (`:2485`),
H (`:2631`), I x2 (`:2686`, `:2747`), and leg F (`:2284`). Passing: legs A–E unedited, G's drain
control, the empty-queue leg and J with J-discharge (both green on base by design).

Then I restored every file from a saved copy; `git diff` was byte-identical to the diff before
the revert (`cmp` of the two).

### One mutation per round-4 leg (each on the full patch, each reverted after)

| leg | mutation | result |
| --- | --- | --- |
| C'' | `scrub.rs:327` `\|\| staged_unchecked` → `\|\| (staged_unchecked && false)` | 2 red: empty and wrong-length (`staged_scrub.rs:1137`) |
| C'' counter-case | `scrub.rs:252` `.any(\|chunk\| !committed(chunk))` → `.any(\|_\| true)` | 1 red: counter-case (`:1183`) |
| C''-audit | `scrub.rs:154` call v3's `emit_malformed(chunk, …)` instead (no key, committed wording) | 3 red: empty, wrong-length (`:1143`), counter-case (`:1193`) |
| A'-malformed | `scrub.rs:237` delete `\|\| referenced.malformed.contains_key(chunk)` (v3 `:220-221`) | 1 red: A'-malformed (`:1213`) |
| G-held | `reconstruction.rs:244` delete `.chain(staged.held.keys().copied())` (v3 `:242`) | 2 red: both G-held (`staged_protection.rs:2487`) |
| Empty queue | `reconstruction.rs:215` call `crate::gc::staged_fragments` in the empty branch (v3 `:214-215`) | 1 red: empty-queue leg (`:2560`, the pass faulted) |
| J-discharge | `reconstruction.rs:308` `Drain` pushes only chunks not in `staged_chunks` (v3 `:238`'s set) | 1 red: J at `:2906`, (ii)'s queue-empty assertion |

The crate builds with `-D warnings`, so the C'' and C''-audit mutations keep every binding used
(`&& false`, `let _ = (record, emit_malformed_staged)`); my first attempt without that did not
compile and proved nothing, so I re-ran both. After each mutation both production files were
restored from scratch copies and `cmp`'d against them.

### Green (full patch)

```
cargo test -p wyrd-custodian --test staged_protection --test staged_scrub
  staged_protection: 36 passed; staged_scrub: 16 passed
```

## Gates

Through the project's runner, `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt ./engine/xtask.sh ci`:

- First run failed at `typos`: my new upload pair `"ba"` read as a misspelling. I changed the
  three new pairs to `"b3"`, `"b4"`, `"b5"` (`staged_protection.rs:2516`, `:2529`, `:2547`),
  re-ran `typos` clean, and re-ran the whole gate.
- Second run: `xtask ci: all checks passed`, exit 0 — typos, docs lint, docs render + link
  audit, gitlink and unsafe guards, `cargo fmt --check`, clippy, build, workspace tests,
  cargo-machete, cargo deny (three runs), the statics and deploy guards, madsim clippy and
  `cargo test -p wyrd-dst (--cfg madsim)`. 1470 tests passed, 0 failed across the run, including
  the four seeded DST staged-handoff legs (`reconstruction_staged_handoffs_never_drain_the_
  obligation`, `gc_staged_handoffs_never_reclaim_the_chunk` and both `…_reach_between_and_
  outside_the_reads` coverage legs). DST production code is unchanged this round (only comments
  in `reconstruction.rs` and `dst/tests/custodian.rs`), so I did not run a separate DST red.

Both external dependencies the brief names are present: `typos-cli 1.48.0`, and
`python3 -c "import markdown_it, yaml"` succeeds (`docs-renderer`); the gate ran both the spell
check and the docs lint/render for real, not warn-skipped.

`cargo fmt --all` was run after every edit; the gate's `cargo fmt --all -- --check` is part of
`ci`.

## Self-refutation (the three forced questions)

**(a) Genuine red?** Yes, by reverting and re-running, not by argument. With production at base
(the brief's method), 11 of 16 `staged_scrub.rs` legs and 8 of 36 `staged_protection.rs` legs
fail by assertion, including every round-4 leg the brief says is red on base (C'' empty and
wrong-length) plus the C'' counter-case's audit assertion. The legs green on base are guards the
brief says are green by design (B, A's control, A', A'-malformed, empty queue, J/J-discharge),
and each round-4 guard goes red under the exact mutation the brief names for it (table above).

**(b) Production path?** Yes. Every leg runs `wyrd_custodian::reconcile_step` with a real
`ScrubContext` or `ReconstructionContext`; the doubles implement only the `MetadataStore` /
`ChunkStore` traits. The audit assertions read the real `tracing` events the production emitters
send. J-discharge's (i) observes the production repair's own repoint commit; (ii) runs the
production drain.

**(c) Fixture includes the fault?** Yes. C'' seeds the damaged part record with nothing on any
server, so an identity fill would find server 0 empty and enqueue (the phantom the leg forbids).
The counter-case seeds the same damage plus a valid committed placement with an intact fragment,
so "block on any malformed record" and "never block" each fail one of the two legs. A'-malformed
leaves server 3 — the part record's valid position — empty, so checking it would enqueue.
G-held seeds a readable wrong-length record, the exact thing `StagedSet::held` holds, and no
fragment anywhere, so a pass that ignored it would drain. The empty-queue leg arms faults on all
four prefixes, so any read there fails the pass, and it also checks the read log. J-discharge
keeps the leftover part record in place, byte-identical, while the duplicate obligation is
queued, which is exactly the over-reach condition.

## Items for the human

- `reconciliation.rs:25-28` (the `Reconciled::Blocked` doc) is outside the brief's file list; I
  edited it because C'' makes its old text false. Three lines.
- The new audit action `malformed-staged-placement` and counter `scrub_malformed_staged_placement`
  are new operator-facing names. Nothing in `docs/` lists scrub's names, so no doc was updated.
- Leg J reuses upload pair `"ae"` with leg I (`staged_protection.rs:2667` and `:2560` in v3);
  harmless (separate stores, per-thread audit log), not mine to rename per the brief's "do not
  rename" rule.
- The first ci run failed on `typos` (above). Fixed; the second run is the result.

## Scratch

Backups, the v3-only file copies, the mutation helper and the gate logs lived in
`$PDCA_SCRATCH/pdca-builder-813-mut` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_813/…`); I removed
that directory when done. The gate's docs render wrote its own
`wyrd-docs-build-2819556` under the same `$TMPDIR`; the gate chose that name, not me, so I left
it for the harness.
Every source edit was made in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`); the new test
file is intent-to-add there so `git diff` includes it. No branch was pushed and no PR was opened,
readied or merged.
