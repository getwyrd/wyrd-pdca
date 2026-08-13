# Build notes — #695 backfill: reads through the resolver, contained

## What I built

`crates/custodian/src/backfill.rs` (`$PDCA_WORKTREE`, target `getwyrd/wyrd @ main`,
base `339da46`): replaced the two inline `record.chunk_map.as_flat().ok_or(
ChunkMapError::SegmentedMapUnsupported {..})?` sites (`reconcile` and the old
`emit_remaining`) with a single per-object walk that:

- resolves every committed record through `wyrd_core::metadata::resolve_chunk_map`
  (`backfill.rs:120`), the same resolver GC (`gc.rs:402`), scrub, and restore already
  share;
- contains a decode failure, an unparsable `inode:` key, or an unresolvable chunk map
  **per object** — named on the audit seam and counted, the walk continuing
  (`backfill.rs:100-133`, `Refusals::cannot_account_for`, `backfill.rs:253`);
- **declines** (never mutates) a record whose *scanned* snapshot is segmented
  (`backfill.rs:166-169`) — checked on `record.chunk_map.is_segmented()`, the shape the
  scan read, never on `resolved.record`'s shape (that is Rule A, below);
- fills a fillable **flat** record exactly as before, but CAS'd on the raw scanned
  `key`/`value` (`backfill.rs:191-194`) instead of `metadata::inode_key(parse(key))` /
  `metadata::encode(&record)` — Rule C;
- accumulates the empty-placement gauge and the declined/unaccounted counts in the
  **same** walk (`backfill.rs:96-97`, `:219`) — `emit_remaining` no longer re-scans
  (`backfill.rs:295-300`);
- answers `Reconciled::Blocked` whenever anything was declined or unaccounted
  (`backfill.rs:221-229`, `Refusals::total`, `backfill.rs:275-277`), never claiming more
  than it read (C-1).

New test: `crates/custodian/tests/segmented_map_backfill.rs` (also `patch.diff`), seven
legs over one `MemMeta` double, driven only through `wyrd_custodian::backfill::{reconcile,
BackfillContext}`, `wyrd_core::metadata::{...}`, `wyrd_custodian::reconciliation::Reconciled`.

## Why this shape, what I ruled out

**Salvage, not re-derive.** The brief's `Citations expected` pointed at
`results/issue_681/iteration-v7/patch.diff:1-365` (backfill.rs's hunk) as having already
passed C1–C5, C4-verify and mutation analysis (82 mutants, 0 survivors) on the un-split
#681. I took that hunk verbatim as the starting point rather than re-deriving the
production change — the brief explicitly forbids re-deriving it, and re-deriving a shape
that already survived independent adversarial review would be pure risk for no benefit
(the "cost" of the alternative — writing a new decline/contain shape from scratch — is
the whole 100-semantic-line hunk's worth of surface re-exposed to review, for a shape
that already has a clean bill).

**Rule A analysis — I did not change the salvaged control flow, and here's why.** The
brief flags Rule A ("never act on a resolve that restarted") as the item four prior
review rounds kept re-opening, and requires a leg proving it. I traced the salvaged
code's actual data flow before writing that leg, because the natural first guess —
"check `resolved.record.chunk_map.is_segmented()` after resolving" — is exactly the
version that would be wrong: for a **flat** scanned record, `resolve_snapshot`
(`metadata.rs:2584-2585`) never touches the store and can never restart, so
`resolved.record` is always the same generation as `record`; for a **segmented** scanned
record, the salvaged code declines on `record.chunk_map.is_segmented()` — the SCANNED
shape — at `backfill.rs:166`, which runs *before* `resolved.chunks`/`resolved.record` is
ever used to build a write. So a restart (which only a segmented resolve can hit,
`metadata.rs:2338-2339` region / `resolve_current_chunk_map`, `metadata.rs:2652-2687`)
is always inside the declined branch, and the code that builds `next`/the CAS batch
(`backfill.rs:184-194`) is only reachable when the scanned record was flat, where a
restart is structurally impossible. Rule A is therefore an invariant of the *order of
checks* the salvaged hunk already has, not a missing check — but that was not obvious
without tracing it, which is exactly why the brief demands a test rather than an
argument. Leg 4 (`a_pass_never_writes_to_a_generation_it_did_not_scan`) manufactures the
scenario with a double whose `scan` answers a stale segmented root while `get` answers a
live flat one, confirms via `resolve_chunk_map` that the fixture genuinely restarts onto
a *different* generation (`resolved.record.version == 5` vs. the scanned `version == 3`),
then asserts the pass writes nothing and answers `Blocked`. I checked what a **plausible
regression** would look like (swapping the `is_segmented()` check to read `resolved`
instead of `record`) and confirmed by hand that it would make this exact leg fail: the
buggy variant would treat the (now-flat) `resolved` as fillable, build `next` from the
*stale* `record`, and CAS against the stale `value` — which the live store no longer
holds, so `commit` returns `Conflict`. Under the salvaged bookkeeping a `Conflict` is
`refused.superseded`, which — correctly, per Rule B — does **not** count toward
`Refusals::total()` (an ordinary lost race must not block certification). So the buggy
variant would silently answer `Satisfied` over a record it never legitimately assessed.
That is the failure leg 4 is built to catch, and it is why the leg's assertion is on the
**answer** (`Blocked`, not `Satisfied`) rather than only on "nothing was corrupted" — the
CAS was already going to prevent corruption either way; the omission four review rounds
kept re-opening is a wrong *answer*, not a data-loss bug.

**Rule C.** Already present in the salvaged hunk (raw `key`/`value` CAS,
`backfill.rs:191-194`) — no change needed beyond confirming it, which leg 5
(`inode:007` vs `inode:7`) does: on base, `inode_key(parse("007"))` == `b"inode:7"`,
so the base reads one record and CASes another, and that CAS silently loses (`Conflict`)
without ever being counted as a refusal — base answers `Satisfied` over undone work,
which is exactly what leg 5's red-leg failure shows (`assertion left != right failed:
left: Satisfied, right: Satisfied` — i.e. it answered `Satisfied`).

**What I did not do:** rewrite `rebalance.rs` / `reconstruction.rs` (the v7 patch's other
two hunks) — explicitly out of scope for this child, siblings of the #681 split, and
touching them would conflict with a bundle building in the same wave (brief `/ out of
scope`). I did not touch `crates/custodian/tests/backfill.rs` — it stays green
unmodified (verified below); a need to edit it would have signalled the answer had
changed further than intended (`brief.md`'s explicit warning), and it didn't need one.

## Compression against the budget

Budget: 2 files, `backfill.rs` ≤130 added semantic lines, test ≤320 semantic / ≤520 raw.
Measured on the final patch:

- `backfill.rs`: 100 added semantic lines (`git diff --cached -- crates/custodian/src/backfill.rs | grep '^\+' | grep -v '^+++' | grep -vE '^\s*$|^\s*//' | wc -l`) — matches the v7 salvage's own reported 100, since Rule A needed no code change and Rule C was already present.
- `segmented_map_backfill.rs`: 318 semantic / 431 raw lines. The first draft (391
  semantic / 555 raw) blew both caps; I cut it down by (a) removing a second `Precondition`-
  style redundant assertion per leg, folding related checks into one `assert!` with a
  combined boolean, (b) factoring three repeated multi-field struct literals
  (`InodeRecord`, `ChunkRef`, `SegmentRef`) into one-line helpers (`make_record`,
  `flat_ref`, `seg_ref`) reused across `commit_root`/`seed_seg`/leg 4 instead of being
  built inline three times each, and (c) `#[rustfmt::skip]` on those helpers and on
  `MemMeta`'s trait methods — the repo's default `struct_lit_width`/`fn_call_width`
  heuristics (no local `rustfmt.toml`) force any multi-field struct literal or multi-arg
  trait method onto 5+ lines regardless of column width, which is what actually drove
  the raw-line count; `#[rustfmt::skip]` on a tiny leaf builder mirrors the existing
  in-tree precedent at `results/issue_681/iteration-v7/patch.diff:1467` (`#[rustfmt::skip]
  fn chunk_ref(...)`). I did **not** cut a leg or a binding assertion to hit the budget —
  every one of the seven legs' brief-mandated observables is still asserted; what I cut
  was assertion *redundancy* (e.g., leg 2 originally asserted both "root bytes
  unchanged" and "root version unchanged" — the second implies the pass touched nothing,
  so the first was dropped as strictly redundant) and construction boilerplate.

## Alternative considered and rejected: sharing one walk across GC/scrub/backfill

The brief's `/ out of scope` explicitly rules this out ("Sharing ONE namespace walk
across all loops is a separate refactor"), and I did not pursue it. Cost if I had: it
would mean editing `gc.rs`/`scrub.rs`/`reconciliation.rs`, i.e. touching 4+ files against
a budget of exactly 2, and would conflict with the #650/#651 precedent this slice
explicitly reuses rather than restructures.

## Three-question self-check (Before you declare done)

**(a) Genuine red?** Yes — verified twice, by hand (`git stash push -- crates/custodian/src/backfill.rs`
then `cargo test -p wyrd-custodian --test segmented_map_backfill`) and through the
project's own runner (`./engine/scripts/run-verify.sh` with `$PDCA_BUNDLE` pointed at
this bundle, from `wyrd-pdca`). Both show 6/7 legs failing on the reverted production
file (leg 7 — the non-red over-containment guard — passes both ways, exactly as the
brief specifies) and the runner's own verdict: `run-verify.sh: PASS — red without the
fix, green with it (7 test(s) ran red)`.

**(b) Production path?** Yes — every leg drives `wyrd_custodian::backfill::reconcile`
(`backfill.rs:91`) through `BackfillContext` over an in-memory `MetadataStore` double; no
copy or re-implementation of the pass exists anywhere in the test.

**(c) Fixture includes the fault?** Yes, per leg: leg 1/2 seed a real segmented
`ChunkMap` (raw `seg:` records + a segmented root) alongside a flat record in the SAME
store; leg 3 seeds a root naming an unwritten `seg:` record AND a record whose bytes
won't `decode`, both ahead of the healthy record in `BTreeMap` key order (asserted, not
assumed, at `an_unreadable_committed_object_is_named_and_the_walk_continues` — `bad =
metadata::resolve_chunk_map(...).await; assert!(bad.is_err(), ...)`); leg 4's double
genuinely restarts the resolve onto a different generation (asserted via
`resolved.record.version == 5` against the scanned record's `version == 3`); leg 6 seeds
S=3 real segmented objects and counts real `scan`/`scan_page` calls on the double; leg 7
injects a real non-`ChunkMapError` fault on `get`. No leg's fixture excludes the
element it's meant to exercise.

## Verification run

- `cargo fmt -p wyrd-custodian -- --check` — clean.
- `cargo clippy -p wyrd-custodian --all-targets --tests` — clean (workspace lints:
  `warnings = "deny"`, `clippy::all = "deny"`).
- `cargo test -p wyrd-custodian` — full crate suite green, 16 targets, including the
  pre-existing `tests/backfill.rs` (5/5, unmodified) and the segmented-map peer suites
  (`segmented_map_consumers.rs` 8/8, `segmented_map_restore.rs` 5/5) that share the
  containment vocabulary this patch extends.
- `cargo check --workspace` — clean (confirms no downstream crate — `wyrd-server` is the
  only consumer of `wyrd_custodian::BackfillContext`/`Reconciled` — breaks on the
  `Reconciled::Blocked` answer `backfill::reconcile` can now return; `Blocked` already
  existed pre-patch from #650, so no exhaustiveness changes anywhere else).
- `./engine/scripts/run-verify.sh --classify results/issue_695/patch.diff` →
  `ADDED_TEST crates/custodian/tests/segmented_map_backfill.rs` / `CRATE
  crates/custodian` — matches the brief's `--classify` prediction exactly.
- `./engine/scripts/run-verify.sh` (full run, `$PDCA_BUNDLE` = this bundle) — PASS
  (green-with-fix, red-without-fix), 7 tests ran on both legs.

## Rubric self-review (target `AGENTS.md` "## Review rubric & protocol")

- *Metadata validation boundaries* (ADR-0045): unchanged — no new decode-time
  validation; contextual (placement) checks stay liberal-on-read.
- *Serialization identity*: the CAS now requires the raw scanned `value` bytes
  (`backfill.rs:193`) instead of a re-encoding of the decoded `record`
  (`metadata::encode(&record)`, the base's approach) — this patch is *stricter* on this
  defect class than the code it replaces, not merely non-regressive.
- *Absent or unsupported entries*: every contained fault is named on the audit seam and
  counted (`emit_unaccounted`/`emit_declined`, `backfill.rs:305`, `:319`) — never a
  silent skip.
- *Test fidelity* / seeded Tier-0 DST: the brief pre-declares and settles this as
  "recorded-rejected" at Plan (this slice adds a decline, which writes nothing, and its
  only write path is the pre-existing flat CAS) — per the rubric's "Deferrals are
  settled" reviewer protocol, I did not add a DST leg and am not re-litigating that
  Plan-time call.
- No port/API/RPC/CLI/persisted-field change, so no docs-currency update is triggered
  (the brief independently confirms `06-runtime-view.md` §6.2 already covers this
  fleet-wide and stays true).
- New crate roots carry `#![forbid(unsafe_code)]`: N/A (no new crate); the new test file
  itself carries `#![forbid(unsafe_code)]` (`segmented_map_backfill.rs:22`), matching
  every peer test file in this directory.

## Scratch discipline

No scratch checkout was needed beyond the existing `$PDCA_WORKTREE` (already provisioned)
and the project's own `../wyrd-verify` worktree (managed by `run-verify.sh` itself, not
mine to create or clean). Two throwaway files were written directly under `/tmp` during
line-count measurement (`/tmp/695.patch.check.diff`, `/tmp/added_backfill.txt`); both
were removed before finishing.
