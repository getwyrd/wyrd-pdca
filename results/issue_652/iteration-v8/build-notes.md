# Build notes — issue 652 / recovery-total-over-damage

Target: `getwyrd/wyrd @ main` (`d50f0ca`, the brief's resolved base). All edits made in
`$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`; every `path:line` below is on the
**patched** tree unless marked *pre-fix* (= `origin/main`).

Shipped: `patch.diff` (4 files, `+582/-118`, 45 KB), the new test target
`crates/server/tests/gateway_recover_totality.rs` (also copied to the bundle root), and this
file. `patch.diff` was byte-compared against `git diff` in the worktree as the last step, so
the three artifacts are in lockstep.

---

## 1. What the patch does, and why exactly this shape

The brief's **Invariant to restore** is C-1 over startup recovery: *recovery is total over
stored content*, *a floor an allocator trusts is never a quiet under-approximation*, and *a
number nobody reads is not a safety property*. The change is the smallest one that restores
all three — one function per file, no new seam, no new symbol beyond one private audit
emitter.

### `crates/core/src/metadata.rs` — `high_water_marks` (`:2117-2140`)

* Signature narrowed to `Result<InodeId>` (`:2117`). The chunk half — the `as_flat()` refusal,
  the `IN_PROCESS_CHUNK_CEILING` filter and the two extra complete scans of `pending:` and
  `orphan:` — is **deleted**, along with its now-unused helper `parse_pending_chunk_key`
  (pre-fix `:2033-2040`).
* The mark is taken from the **key** first and unconditionally (`:2121-2133`), so a record the
  walk cannot read still contributes its own floor. This is the ordering that makes the
  second invariant hold: damage can only ever *raise* the mark, never lower it.
* The value is decoded **only to attribute** (`:2135-2139`); a decode failure calls
  `emit_unreadable_inode_row` (`:2047-2060`) and the walk continues. That mirrors the peer the
  brief names, `crates/custodian/src/gc.rs:378-382`, for the reason its module doc gives at
  `gc.rs:22-31`.
* A `scan` failure still propagates with `?` (`:2118`) — a store the walk cannot read is not
  "one record is unreadable" (the split `gc.rs:355-359` already draws).
* **`scan` stays.** No `scan_page`, no `for_each_page`, no `RECOVERY_PAGE`, no cursor walk —
  verified by grep (`§4`).

### `crates/server/src/lib.rs` — `Gateway::recover` (`:133-136`)

Takes the narrowed result; the doc at `:108-132` states the new totality property, and the
two `mint_chunk_id` / `random_chunk_epoch` docs (`:239-252`, `:255-268`) no longer claim
`high_water_marks` recovers a `< 2^64` chunk space.

### `crates/server/src/cli.rs` — `seed_next_inode_floor` (`:1703-1750`)

An unreadable `meta:next_inode` becomes `None` rather than `?` (`:1717-1735`): `None` never
satisfies the floor test, so the existing CAS — already guarded on the exact bytes that were
read — **repairs** the counter to the recovered floor. Attribution goes to the same audit seam
(`wyrd.metadata.recovery.audit`). `repair_to = floor.max(1)` (`:1706`) keeps the written value
out of `ROOT`'s id; `floor` is `max_inode + 1 ≥ 1` from the only caller in the tree, so this
binds nothing that exists today. **Nothing else in `cli.rs` is touched** — in particular
`alloc_inode` (`:1661-1694`) is untouched, per the brief's out-of-scope list.

**Why repair rather than tolerate.** Returning `Ok(())` while leaving corrupt bytes in place
would satisfy "recover does not refuse" and still leave the gateway unable to serve a new key:
`alloc_inode` parses that same key at `:1667`. Criterion 3's second half ("the next PUT still
commits") is what forces the repair, and mutant 3 in §3 shows the test catches exactly that
lazy fix.

---

## 2. Alternatives considered and rejected — with the cost shown

**(a) Wire the chunk-id floor to a real caller instead of deleting it.** Settled DELETE by
the maintainer at Plan; the brief lists re-deriving/restoring/wiring it as a standing
do-not-re-earn (iv). The decisive fact is history and it is now in the code, not just here
(`metadata.rs:2102-2116`): `fdd34f1` (#487, 2026-07-08) wrote *both* halves in one commit —
before it `mint_chunk_id` was a plain counter from 0 and `recover` consumed the floor; after
it every minted id is ≥ 2^127 (`crates/server/src/lib.rs:239-252`) and the callsite discarded
the floor into an unused binding. The cluster minter mints ≥ 2^64
(`crates/server/src/cli.rs:1751-1758`). Nothing in the tree mints below 2^64, so wiring would
mean **inventing** a consumer.

**(b) Keep the walk failing closed on a record it cannot read (status quo).** This is the
defect: it converts one damaged record into a gateway that will not start, i.e. a permanent,
manual-repair-only failure mode for every healthy object — the exact shape C-1 forbids
(`docs/principles.md` §5; `crates/custodian/src/gc.rs:22-31`).

**(c) Skip a damaged record entirely (attribute, then `continue` before reading the key).**
The cheapest-looking containment, and *wrong*: it is total but silently under-approximates the
floor. Measured, not asserted: mutant 2 in §3 produces `meta:next_inode=2` over a store holding
`inode:50` — the allocator would then re-mint an id whose bytes are on disk. The invariant's
"never zero" clause is precisely this case.

**(d) Don't decode the value at all** (the mark needs only the key, so the decode is pure
overhead). Rejected because criterion 1 requires the unreadable record to be *attributed
rather than swallowed*, and startup is the one pass that sees every record. Cost of keeping
it: exactly the work `origin/main` already did (it decoded every value at pre-fix `:2081`), so
this is not a regression — the difference is `?` vs. one `tracing::warn!`. Cost of dropping
it: the store's damage becomes invisible until some later reader trips on it, which is the
"silent skip" the rubric's *absent-or-unsupported entries* class forbids.

**(e) Page the walk / bound it / guard `alloc_inode`.** Out of scope by explicit Plan decision
— that is **#687**, and standing do-not-re-earn (v). The salvaged v1 shape was taken for its
*containment* structure only; every paging symbol (`for_each_page`, `RECOVERY_PAGE`,
`ScanCapExceededStore`) and the whole floor apparatus (`RecoveredIds`, `ClassIds`,
`torn_digit_escape`, `json_string_token`, `scavenged_chunk_id_floor`, `segment_chunk_floor`)
was left behind — grep-verified in §4.

**(f) Rename `high_water_marks` (it now yields one mark).** Rejected: the brief's Scope says
"`high_water_marks` yields the inode mark alone", and the name is referenced from
`docs/design/proposals/draft/0016-multipart-commit-protocol.md:829`, which this slice may not
edit ("any docs paragraph" is out of scope). A rename would either strand that reference or
force an out-of-scope docs edit. The doc comment states the singular explicitly.

**On the removed standing test.** `high_water_marks_refuses_a_segmented_root_rather_than_re_
mint_its_chunk_ids` (pre-fix `metadata.rs:3417`) is replaced, not dropped, by
`high_water_marks_is_total_over_records_it_cannot_read` (`metadata.rs:3446-3489`), and the
expiry reasoning travels with it in that test's doc comment: its premise needs a minter
allocating below 2^64, and #487 removed the last one. Its live half — a walk must not
under-count a record it cannot read — is inverted into what totality requires and is bound
both there and by criterion 2 of the acceptance target.

---

## 3. Refuting my own test (forced, recorded)

Everything below was run through the project's own runner — the configured `C4-verify` gate
command `./engine/scripts/run-verify.sh` with `PDCA_BUNDLE` pointed at the bundle (or, for the
mutants, at a scratch bundle holding a deliberately broken patch). No hand-rolled test
invocation was used.

**(a) Genuine red? — YES.** The RED leg reverts all three production files, keeps the test,
and runs `cargo test -p wyrd-server --test gateway_recover_totality`:

```
run-verify.sh: GREEN — … (fix applied)      → 3 passed; 0 failed
run-verify.sh: RED   — … (production reverted, test kept)
  recover_is_total_over_an_undecodable_inode_record  FAILED: "expected ident at line 1 column 2"
  recover_is_total_over_a_segmented_root             FAILED: "high_water_marks met a segmented
                                                      chunk map, which this build cannot yet resolve"
  recover_is_total_over_a_corrupt_next_inode_counter FAILED: "invalid digit found in string"
run-verify.sh: PASS — red without the fix, green with it.
```

All three are **assertion** reds on `recover()` returning `Err` — the test compiles fine
against `origin/main` (it names only base-visible symbols and drives the signature-stable
`Gateway::recover()`), so no compile-failure is being mis-scored as a pass. Each red message
is a *different* pre-fix cause, matching the three defects the brief names.

**(b) Production path? — YES.** The test composes the real backends
(`RedbMetadataStore` + `FsChunkStore` + `MemCoordination`, the same composition
`s3_http_wire.rs:679-686` drives) and calls production `Gateway::recover()`,
`Gateway::put_object()`, `Gateway::get_object()`, and `wyrd_core::read::resolve` to read back
the id a PUT actually minted. No mock, no store double, no re-implementation. The only
artificial step is writing raw bytes into the store (`write_raw`), which is the same technique
the precedent uses to strip a counter (`s3_http_wire.rs:686-696`).

**(c) Fixture includes the fault? — YES.** Each fixture holds the damaged record **and** a
healthy committed object in the *same* store: `inode:50` = `b"not a metadata record"` (test 1),
a structurally valid segmented root at `inode:37` (test 2), `meta:next_inode` =
`b"not-a-number"` (test 3). Nothing is curated out — the healthy object is what makes the
assertion "containment", not "blanket tolerance", and it is read back byte-identically after
recovery in every leg.

**Beyond the three questions — three mutants, each caught** (this is what shows the test binds
the *invariant*, not the proxy "recover returned Ok"):

| Mutant (applied to the patched tree, run through `run-verify.sh`) | Result |
|---|---|
| 1. Silently skip a record whose value does not decode (`continue` before the key parse) | GREEN leg **FAILS** at `gateway_recover_totality.rs:206` — the record is no longer attributed |
| 2. Attribute it, then `continue` — total and loud, but the id never reaches the mark | GREEN leg **FAILS** at `:215`: `must seed the allocator strictly above the damaged record's key id (50); got meta:next_inode=2` |
| 3. `seed_next_inode_floor` returns `Ok(())` on an unreadable counter without repairing it | GREEN leg **FAILS** at `:338`: `must leave meta:next_inode READABLE and at least the recovered floor` |

Mutant 2 is the important one: it is the "quiet under-approximation" the invariant forbids, it
*passes* a naive "recover() is Ok" test, and this target catches it.

**Hang-proofing (criterion 3, "in bounded time").** `recover()` runs on its own thread with a
`recv_timeout(RECOVER_BUDGET = 60s)`; a never-committing retry loop **fails** the test with an
assertion instead of hanging the suite (`gateway_recover_totality.rs:63-70,151-181`).

---

## 4. The other criteria, mechanically

* **Criterion 4 — the dead half is gone.** `git grep -n "_max_chunk" -- crates/` → **no
  matches** (exit 1) on the patched tree. `git grep -nE
  "RecoveredIds|ClassIds|scavenged|torn_digit|json_string_token|segment_chunk_floor|for_each_page|RECOVERY_PAGE"`
  over `crates/core/src/metadata.rs` and `crates/server/` → no matches. The only `scan_page`
  hits in those paths are pre-existing (`metadata.rs:530,2164,2346,2382` — the #649 segment
  resolver — and five test doubles), none added here.
* **Criterion 5 — no regression.** `recover_seeds_the_allocator_over_a_legacy_store_without_
  meta_next_inode` (`crates/server/tests/s3_http_wire.rs:666`) passes **unchanged** in the full
  gate run, as do `restart_recovers_id_allocators_no_collision` and
  `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`.
* **Whole-tree gate.** `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`:
  typos → lint_docs → gitlink/unsafe guards → `fmt --check` → `clippy --workspace
  --all-targets` → `build --all-targets` → `test --workspace`) → **exit 0, "xtask ci: all
  checks passed"**, run twice (the second on the final tree bar one comment-only doc fix).
  Both runs included the new co-located unit test
  (`metadata::segmented_shape_invariants::high_water_marks_is_total_over_records_it_cannot_read
  ... ok`) and the new test target.
* **Commit-ready.** `cargo fmt --all` applied and `cargo fmt --all -- --check` clean on the
  final tree; `typos` clean on the final tree (re-run after the last doc edit — a comment-only
  change can only move `typos` / `fmt`, both re-run); `lint_docs.py` OK; `clippy --all-targets`
  clean. No docs paragraph, no ADR/spec/proposal, no conformance vector, no new dependency.
* **Budget.** 303 added semantic (non-blank, non-comment) lines across 4 files — 64 of them
  production (`metadata.rs` 43, `cli.rs` 20, `lib.rs` 1), 239 test. Ceiling was ~450 across ≤ 5
  files. Patch is 45 KB, nowhere near the 100 KB stop line, and does not reach into
  `alloc_inode`.

---

## 5. Notes for sign-off

* **No NEEDS-HUMAN external dependency.** Nothing beyond the base Rust toolchain was needed;
  `cargo xtask ci` ran to completion locally (so `cargo-deny`, the brief's one registered
  external dependency, was present and did not block).
* **Known stale reference this slice may not fix.**
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md:829` describes
  `high_water_marks` as recovering "the `< 2^64` in-process chunk-id space". Its *conclusion*
  ("Allocator recovery — No, unaffected") stays correct, but the justification is now stale.
  Editing it is out of scope for this slice ("any docs paragraph"), and no gate covers it. It
  is a one-line follow-up for whoever lands #687 or the next 0016 slice.
* **Commit body — the history the brief requires Do to carry.** Suggested body for publish
  (the same reasoning is already in the code at `metadata.rs:2102-2116` and
  `metadata.rs:3446-3462`, so it survives the commit message too):

  > Startup recovery (`Gateway::recover` → `metadata::high_water_marks` →
  > `cli::seed_next_inode_floor`) refused to finish over content it could not read: one
  > undecodable `inode:` value, one segmented root, or a corrupt `meta:next_inode` each made
  > the gateway fail to start, costing **every healthy object** its availability (C-1,
  > `docs/principles.md` §5). Recovery is now total: the id mark comes from each row's key —
  > so a record it cannot read still contributes its true floor, never a quiet zero — the
  > fault is attributed on `wyrd.metadata.recovery.audit`, and the walk continues, exactly as
  > the custodian's GC walk already contains this same namespace (`gc.rs:22-31,378-382`).
  >
  > The chunk-id mark is **deleted**, not repaired. `fdd34f1` (#487, 2026-07-08) did both
  > halves at once: before it, `mint_chunk_id` was a plain counter from 0 and `recover`
  > consumed the floor; after it, ids are `(chunk_epoch << 64) | seq` with the epoch's top bit
  > set — every minted id ≥ 2^127 from a random per-process epoch, `next_chunk_seq` never
  > seeded — and the same commit rewrote the callsite to discard the floor. The cluster minter
  > never needed it either: `chunk_id_minter` yields `(inode_id << 64) | seq` with
  > `inode_id ≥ 1`, so every cluster id is ≥ 2^64. Nothing in the tree mints below 2^64, so
  > wiring it would mean inventing a consumer, and computing it forced two further complete
  > scans (`pending:`, `orphan:`) whose only product was a discarded number.
  >
  > `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids` goes with
  > it: its premise — a minter allocating below 2^64 — expired with #487. Its live half is
  > kept, inverted into what totality requires, as
  > `high_water_marks_is_total_over_records_it_cannot_read` plus the new acceptance target.
  >
  > Paging, bounded walks and `alloc_inode` safety are **#687**, deliberately not pre-empted
  > here: `MetadataStore::scan`'s one-consistent-cut stays, because `scan_page` declines
  > snapshot isolation and would weaken the recovered floor.
