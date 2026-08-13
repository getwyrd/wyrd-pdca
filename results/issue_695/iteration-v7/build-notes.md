# build-notes — issue 695 (iteration 7)

*Withheld from the reviewer; written for the human at sign-off.*

Target branch `getwyrd/wyrd @ main`, base `339da46` (= `origin/main`, verified in the worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l0`). **Two files, as budgeted**:

| file | added semantic (non-blank, non-comment) | raw | budget |
|---|---|---|---|
| `crates/custodian/src/backfill.rs` | **61** | 206 added | ≤ 95 semantic |
| `crates/custodian/tests/segmented_map_backfill.rs` (NEW) | **240** | **390** | ≤ 240 / ≤ 400 |

No `crates/dst/` hunk, no `Cargo.toml` change, no docs edit, no third file.
`tests/backfill.rs` and `tests/backfill_telemetry.rs` are **unmodified and green**.

---

## 1. What the change is, and why this shape

The defect: backfill read the chunk map inline out of the record at two sites, each
`record.chunk_map.as_flat().ok_or(SegmentedMapUnsupported)?` — `origin/main`
`backfill.rs:98-101` (in `reconcile`) and `:180-183` (in `emit_remaining`) — so ONE segmented
object returned `Err` for the whole store and the drain gauge was never published at all;
and a record that would not `decode` ended the walk at `:80`/`:174` before any resolver was
involved.

The patch (all citations on the worktree, post-patch line numbers):

1. **Read every committed record through the shared resolver** — `metadata::resolve_chunk_map`,
   `backfill.rs:145-166`. Arms copied from the two merged peers rather than invented:
   `Ok(None) => continue` (`gc.rs:404`, `restore.rs:646`), `Err → downcast::<ChunkMapError>` →
   contain (`gc.rs:402-416`), anything else `return Err` (`gc.rs:412-414`).
2. **Contain a decode failure per object** — `:125-132`, conservatively before the `state` check,
   exactly as `gc.rs:366-384` sets out and for the reason stated there.
3. **Name the object the moment the walk meets it**, ahead of every store read that follows —
   `emit_unresolvable`, `:321-330`, mirroring `gc.rs:155-166`'s *placement*, not merely its call;
   named through `gc::object_name` (`gc.rs:470-480`), the injective escaping.
4. **Decline, never write, a segmented generation's fill** — `:208-212`. The decline is a
   `let … else` on **the scanned record's own inline chunk list**; the fill below is built from
   that same list (`:217`) and CAS'd on that same record's bytes (`:238-241`, unchanged from the
   base). §2 explains why this is the fix for round 6's blocker.
5. **Count the population in the pass's own walk** — `remaining` at ONE site (`:191`), off it only
   by a committed fill (`:248`) — so the second `scan(b"inode:")` disappears entirely, and publish
   it beside `incomplete` on ONE event, each as its own `gauge.`-prefixed instrument (`:302-307`).
6. **Refuse to certify** while `incomplete > 0` (`:271-277`) — the `gc.rs:234-246` shape and
   vocabulary (`Reconciled::Blocked`, `reconciliation.rs:44`), not a parallel outcome.

The audit/metric vocabulary is exactly the three items the brief pinned, each asserted by a leg:
`action = "unresolvable-chunk-map"` + `monotonic_counter.backfill_unresolvable_records`
(the same action string as `gc.rs:563-573` / `restore.rs:826-835`), `action = "declined-segmented"`
+ `monotonic_counter.backfill_declined_records`, and `gauge.backfill_placement_incomplete` beside
`gauge.backfill_placement_remaining`.

**Frozen lines, verified byte-identical against `git show origin/main:…`:** `parse_inode_key`
(`:70-76`), its skip (`:136-138`), the CAS key + `encode(&record)` precondition (`:238-241`), the
`inode_id` audit fields of `emit_backfilled`/`emit_conflict` (`:365`, `:393`). The #698 carve-out is
untouched. (One consequence I checked and deliberately left alone: a record under a non-canonical
key — unreachable today, `metadata::inode_key` is the sole writer of the prefix,
`crates/core/src/metadata.rs:33-36` — is skipped before the count, where the base's second scan
would have counted it. Fixing that means moving the frozen skip, which is #698's, and is what
sank rounds 3 and 5.)

---

## 2. The Iteration-6 carry-forward, item by item

### C5 — "each unreadable class independently withholds certification remains unproven"
Two mutants survived in v6: `incomplete += 1` → `*=` at each of the two unreadable sites, because
the combined leg needed only one of them to answer `Blocked`.

Fixed in the **discriminator**, not by weakening the production shape:
* leg 3 now pins the exact published count — `assert_eq!(gauge(&logged, INCOMPLETE), Some(2))`
  (`segmented_map_backfill.rs:346`) — so a class that stops counting is a `1 != 2`, not a silent
  absorption;
* and each class is additionally run **alone over its own store** (`:313-326`): `Blocked`,
  `INCOMPLETE == 1`, and the healthy record beside it still filled.

Evidence — `scripts/mutants-in-diff` on the final patch: **17 mutants tested: 11 caught, 6
unviable, 0 missed** (v6: 2 missed). `cargo mutants --list` confirms `backfill.rs:129:28` and
`:158:32` (`+= → *=`) are among those tested. Manually re-applying the exact v6 survivor at `:158`
gives:

```
assertion `left == right` failed: inode:1 … left: Changed  right: Blocked
   at crates/custodian/tests/segmented_map_backfill.rs:323
```

### T5 — "compress the discriminator below the hard ceiling"
v6: 473 raw / 351 semantic. Now: **390 raw / 240 semantic**, with an assertion *added*. Where the
83 raw / 111 semantic went, in case a reviewer asks whether coverage was traded away:
* rationale moved from `assert!` messages into `//` comments above the assertion — the same
  reasoning, no semantic lines (rustfmt splits any macro whose arguments exceed 60 chars into 5
  lines, which is what made v6's messages so expensive);
* six `const` needles for the seam strings (`DECLINED`, `UNREADS`, `INCOMPLETE`, …) so every
  audit/gauge assertion is one line;
* `Shape` carries one placement per object instead of a per-chunk vector, collapsing `seed`'s
  flat arm to one line;
* the `attributed()` set-of-names helper became two `assert_hits` on `inode=inode:N;` (same
  property: each damaged record named exactly once, under its own key);
* the base-parity oracle is computed inline in leg 4 from the store's own post-pass content (3
  lines) instead of a 9-line helper;
* byte-identity is now over the store's **whole** row set (`:279`, `:287`) rather than two named
  keys — shorter *and* stronger;
* the `≤ S seg: range reads` bound moved onto leg 1's store, which already holds a segmented
  object and already ran a pass.
Nothing the brief asks for was dropped: all five legs are present, and every pinned vocabulary
item is asserted.

### T4 — the two batch-review blockers
* **`backfill.rs:204` — restart onto a newer flat root misreports `declined-segmented`.**
  **Fixed by construction.** v6 branched on `record.chunk_map.is_segmented()` while building the
  fill from `resolved.chunks`; that is what let a restarted resolution reach the write path at all.
  Now the branch and the write are the *same* expression on the scanned generation
  (`let Some(scanned_chunks) = record.chunk_map.as_flat() else { decline }`, `:208`; the fill is
  `scanned_chunks.to_vec()`, `:217`), the audit line claims only what is true on every path — the
  **scanned** generation keeps its chunks in `seg:` records (`:353`) — and the residual (after a
  restart the counted `placements` are the live root's) carries a `deferred: #699` marker
  (`:341-345`), which `AGENTS.md` § *Reviewer protocol* settles for review purposes. Blocking is
  correct in that window either way: a fill the pass READ was not performed by it.
* **`backfill.rs:252` — the lost-CAS gauge can overcount.** True, and **pinned by the brief** as
  answer rule 2 ("including one whose CAS was lost… only a committed fill takes it off"). It is
  recorded-rejected in `results/issue_695/review-rejected.md` (three locs, MATCH `overcount`) with
  the reasoning: the pass never read the winner's bytes; the one direction a drain signal may not
  err in is publishing a convergence nobody performed (C-1); reading the winner back would need
  the second read leg 4 forbids. **Please confirm that rejection at sign-off** — if you disagree,
  delete those three lines from `review-rejected.md` and the finding will block again.
  The DST-leg class is recorded-rejected there too, per the brief's §Verification posture.

---

## 3. Alternatives considered, with their cost

* **Keep v6's `is_segmented()` decline and only reword the audit** — 1 line changed instead of 3.
  Rejected: it leaves `next_chunk_map = resolved.chunks.to_vec()` (v6 patch line 180), i.e. the
  write still *reads* from a list a restart could have replaced. The brief's constraint ("the bytes
  any write is built from … are decided from the generation the scan returned") would then hold only
  by an argument about unreachability, not by construction — which is exactly what a reviewer
  re-derived as a BUG in round 6.
* **Add a generation comparison (v5's "Rule A") to tell a restarted resolution apart** — measured
  on the v5 patch it replaces: ~14 production semantic lines plus a **325-line** seeded Tier-0 DST
  property in `crates/dst/tests/custodian.rs` (a third file this bundle may not touch), and it was
  the direct cause of the round-1 and round-4 blockers. Out of scope, carved to **#699**.
* **Split leg 3 into three `#[tokio::test]`s** to prove independence — +14 raw / +9 semantic over
  the in-test loop, which would have put the file at 404 raw, over the STOP ceiling. The loop
  proves the same property (each class alone ⇒ `Blocked` + its own count of 1).
* **Re-read the winning record after a lost CAS** to make the gauge exact — costs a second `get`
  per conflicted record and contradicts leg 4's "one reading per pass"; and it still cannot be
  exact (the winner can change again between the read and the emit). Rejected on the brief's rule.
* **Move `parse_inode_key` after the resolve** so a non-canonical key's placements stay on the
  gauge — 3 lines moved, but they are the lines the brief freezes; rounds 3 and 5 died there.
  Left to **#698**.

---

## 4. Forced refutation of my own test

* **(a) Genuine red?** Yes, through the project's own runner. `./engine/scripts/run-verify.sh`
  (PDCA's C4-verify, which applies `patch.diff` to a clean `../wyrd-verify` worktree off
  `origin/main`, then reverts only the production hunk):
  `GREEN — 5 passed` with the fix; `RED — 0 passed; 5 failed` with `backfill.rs` reverted and the
  test kept; verdict `PASS — red without the fix, green with it (5 test(s) ran red)`. The reds are
  **assertion/expect** reds on base-visible symbols (e.g. `one namespace reading  left: 2 right: 1`;
  `SegmentedMapUnsupported { operation: "backfill::reconcile" }`), not a compile error — the test
  names no symbol this patch introduces, so the red is behavioural, not UNVERIFIABLE.
* **(b) Production path?** Yes. Every leg calls `wyrd_custodian::backfill::reconcile` — the real
  public entry the pass ships — over doubles that implement the real `wyrd_traits::MetadataStore`
  seam, and the resolution under test is the real `wyrd_core::metadata::resolve_chunk_map`. No
  mock, copy or re-implementation of the logic under test; the only test-owned code is the store
  double and the `tracing` capture layer.
* **(c) Fixture includes the fault?** Yes, and it asserts its own faults. `seed` proves the
  segmented root genuinely fails to resolve (`metadata::resolve_chunk_map(…).is_err()`,
  `segmented_map_backfill.rs:195-197`) and that the undecodable bytes genuinely fail
  `metadata::decode` (`:160-162`) — so no leg can pass because a seeded fault quietly stopped being
  one. The store is `BTreeMap`-backed and the damaged ids (`inode:1`, `inode:2`) sort **before** the
  healthy `inode:9`, so "the healthy record was still filled" is met after both blockers as a
  fixture *property*. Leg 5 injects a real non-`ChunkMapError` fault into `get` and asserts THAT
  error came back (`:389`), so containment cannot silently swallow a store outage.

## 5. Gates run locally

| check | command | result |
|---|---|---|
| whole-tree CI (fmt/clippy/build/test/deny/conformance/typos/docs) | `./engine/xtask.sh ci` | **all checks passed** |
| per-fix red→green | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **PASS** (5 red pre-fix, 5 green post-fix) |
| mutation on the bundle diff | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **0 missed** (11 caught, 6 unviable) |
| custodian suite | `cargo test -p wyrd-custodian` | 94 tests, all green (incl. `backfill.rs` 5, `backfill_telemetry.rs` 1, unmodified) |
| formatter / commit hooks | `cargo fmt --all -- --check`, `typos` on both files | clean |

No external dependency beyond the base Rust toolchain was needed; nothing to declare as
NEEDS-HUMAN on that axis. Scratch: none left behind (one 15 KB backup of the production file was
made and deleted inside the same command while checking the mutant by hand).

## 6. For the human at sign-off

1. **Confirm the recorded rejection** of the lost-CAS "gauge can overcount" finding in
   `review-rejected.md` (§2). It is the brief's own answer rule; if you read it differently, remove
   the three machine-readable lines and let the reviewer re-raise it.
2. The `deferred: #699` marker at `backfill.rs:341-345` is the only place this patch leans on a
   tracker deferral. It covers exactly one thing: which generation's `placements` a decline counted
   when the resolver restarted mid-resolve. Nothing is written for that object either way.
3. Every pinned vocabulary item ships and is asserted; no new metric or action string beyond the
   three the brief named.
