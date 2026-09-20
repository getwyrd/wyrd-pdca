# Build notes — #804 (662.2): GC records reclaim intent before deletion + orphan-mark value shapes

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt` at `f41e9c5` (`origin/main`, #803
merged as PR #807). Line numbers below are in the patched tree unless marked "base".

## What changed, and why

### 1. One codec for the `orphan:` value — `crates/core/src/metadata.rs`

Beside `orphan_key` / `parse_orphan_key` (base `metadata.rs:62-85`), as the brief's SELF-TEST
requires (a GC-private decoder would let #659/#663/#800 spell marks their own way):

- `MAX_ORPHAN_EVENT_LEN = 256` (`metadata.rs:98`) and the event grammar `checked_orphan_event`
  (`:238`): 1..=256 bytes of visible ASCII other than `"` and `\`. Every identity 0016 enumerates
  (`0016:1174-1187`) fits (the longest retire token is 94 bytes). Reason: GC's intent commit
  carries each mark's value twice (precondition + `reclaiming` put) for up to 1,000 marks; with an
  unbounded event a single 100 KB value (the value ceiling) makes that commit ~200 MB, past the
  10 MB envelope, failing every pass. With the bound a mark is at most 328 bytes (pinned by
  `a_writer_cannot_encode_what_the_decoder_refuses`, `:4102`) and a full intent commit is under
  800 KB (documented on `CLEANUP_BATCH`, `gc.rs:121-139`).
- `OrphanMark` (`:142`) with private fields and constructors `legacy` (`:151`), `structured`
  (`:166`, refuses an event outside the grammar, so no writer can encode what decode refuses),
  `into_reclaiming` (`:180`, keeps stamp and event), getters, and `retire_token` (`:212`): the
  `retire:bytes:` token the event spells, read through the existing `parse_retire_key` (shared
  parser, rubric "grammar strictness"), `None` for a nonce or a non-canonical spelling.
- `OrphanMarkWire` (`:228`, `deny_unknown_fields`, `event`/`reclaiming` omitted when absent/false),
  `encode_orphan_mark` (`:257`, legacy = `u64::to_string()`), `decode_orphan_mark` (`:277`):
  digits-only → legacy, else JSON object; then the canonical gate — re-encode and require the input
  bytes back, else `RecordError::NoncanonicalRecordValue` — the same shape as multipart's
  `require_canonical` (`multipart.rs:1927-1937`), reusing `RecordError::{MalformedRecordValue,
  NoncanonicalRecordValue}` with namespace `"orphan:"`. So "decode accepts exactly what encode
  writes" holds by construction, and a legacy value round-trips byte for byte.
- The codec doc (`:126-141`) states the writer-side rule 0016 leaves implicit: no writer overwrites
  a `reclaiming` mark (#659/#663 inherit it), and why (GC's key delete is blind and would take the
  new value; the position's fragment is being deleted). It also says how the blind delete-time
  writers relate to it (they never meet one once #663's adoption precondition exists).
- Unit tests `mod orphan_mark_codec` (`:3977`…`:4126`): legacy = every writer's decimal, all three
  shapes round-trip byte for byte, a 23-value refusal table (garbage, signs, whitespace, overflow,
  wrong types, unknown/`null`/reordered fields, `"reclaiming":false`, a JSON `\u` escape, empty /
  spaced / quoted / over-long events, `007`), constructor refusals, max size, and `retire_token`.

`ORPHAN_PREFIX`'s doc (`:56-63`) now points at the codec.

### 2. GC records before it destroys — `crates/custodian/src/gc.rs`

- Module doc paragraph (`gc.rs:30-44`).
- `mark_orphaned` (`:240-252`) now writes `encode_orphan_mark(&OrphanMark::legacy(..))` — byte-
  identical output (pinned by the codec test and by `gc_ledger_walk.rs`'s
  `assert_seeding_is_mark_orphaned`).
- `reconcile` (`:277`): the fleet loop moved into `Sweep` (`:402`), `Intent` (`:426`).
  - `Sweep::judge` (`:460`): the protection gate first, unchanged and still outranking every
    mark (a `reclaiming` one included). Then by `ReadMark`:
    `Reclaiming` → `destroy` at once, no grace test (`:486-493`); `Stamped` → grace test, then
    `retirement_draining` (`:569`, one keyed `get` of `retire_key(Bytes, token)`, never a range
    read), else push an `Intent` and flush at `CLEANUP_BATCH`; `Unreadable` → kept; no mark +
    expired lease + covered → the expired-lease arm, **order unchanged** (delete first; #557/#490).
  - `Sweep::record_intents` (`:585`): one commit of every pending intent
    (`require(key, encode(mark read))` + `put(key, encode(mark.into_reclaiming()))`). `Committed` →
    `destroy` each (fragment, then queue the key delete). `Conflict` → each intent re-committed
    alone; a lost one emits skip `mark-changed`, sets `lost_intent`, keeps its fragment. `Err` →
    propagates before anything of that batch is deleted.
  - `destroy` (`:626`) carries the `deferred: #800` marker (`:621-625`) for the crash between the
    fragment delete and the key-delete commit.
  - Fault path (`:355-361`): `Cleanup::finish_after_fault` (`:1516`) commits the still-queued key
    deletes (every one is for a fragment already deleted), best effort; a failure there is named
    (`emit_cleanup_lost`, `:1657`) and dropped, and the original fault propagates.
  - Outcome (`:389`): `window.is_partial() || sweep.lost_intent` → `Partial`, so a pass whose only
    candidate lost is never `Satisfied`. No new `Reconciled` variant; `Partial`'s doc in
    `reconciliation.rs:46-51` names the new case.
- `ReadMark` (`:1195`) is `Stamped(OrphanMark) | Reclaiming | Unreadable`; `mark_of` returns a
  reference (`:1322`); `classify_ledger_entry` (`:1362`) decodes through the codec and passes the
  codec's reason to `emit_unreadable_mark` (`:1641`, new `fault` field).
- `emit_reclaim` reasons now `orphan` / `resumed` / `expired-lease`; `emit_skip` gains
  `draining-retirement` and `mark-changed` (`:1700`).
- Untouched: `marked_among` (restore's existence-only judgement), `restore.rs`, `scrub.rs`,
  `reconstruction.rs`, `rebalance.rs`, `desired_state.rs`, 0016, ADRs; no `reconcile_step`
  signature change, no new context field, no `Cargo.toml` change.

### 3. Docs (persisted value changed — `AGENTS.md:154-157`)

- `docs/design/architecture/08-crosscutting-concepts.md:87` — new §8.7 paragraph after `:85`: the
  three shapes, the dual-format / exact-bytes rule, `reclaiming` terminal, downgrade direction.
- `docs/design/architecture/06-runtime-view.md:78` — new §6.7 step 2 paragraph (inserted after the
  ledger-walk paragraph, before the staged paragraph #808-#810 edit): record-before-destroy for
  **marked** fragments only, lost swaps, resume, fault flush, the `reclaiming`-over-deleted-bytes
  leftover, expired-lease bytes unchanged, and a draining retirement's fragments never reclaimed
  while it drains ("its drain is what marks them").

## Tests

- **NEW** `crates/custodian/tests/gc_reclaim_intent.rs` — legs A (+ restore guard), B(i)-(v), C,
  D; 9 `#[tokio::test]`s. Doubles as `gc_ledger_walk.rs` builds them (ordered map, own
  `scan_page`, lowered cap), plus one shared event log so ordering (fragment deleted before key)
  is observable. Names no symbol this slice adds (it compiled and ran against the reverted base
  in C4-verify's RED leg). Structured / `reclaiming` values are raw JSON. `W = 1_000` literal.
- **E**: property 14 appended to the existing `crates/dst/tests/custodian.rs` (`:3044`…):
  `adoption_races_gc` (`:3261`) over `SimTikvMetadataStore` with hop-spanning D-server list and
  delete; seeded leg (`:3417`), coverage leg (`:3434`) asserting adopted / reclaimed /
  lost-to-the-adoption / answered-inside-the-delete are all reached (as property 11's coverage
  leg does), both registered with `dst_campaign_test!` and added to the regression-seed loop.

### Red → green evidence

- **C4-verify** (`engine/scripts/run-verify.sh`, run with `WYRD_VERIFY` in my scratch dir,
  `PDCA_LANE=b804`, base `f41e9c5`): GREEN 9/9 passed with the fix. RED (production reverted, test
  kept): **9 ran, 8 failed by assertion, 1 passed by design** (the restore guard
  `a_restore_counts_every_shape_already_marked_and_rewrites_none`, which the brief requires to be
  green on base). No compile failure. The 8 base failures, each an assertion:
  - A: a structured mark past grace was not reclaimed (after reordering, the first message is the
    brief's own "a structured mark past grace licenses nothing"; `007` is also reclaimed on base).
  - B(i): the fragment was deleted although the intent commit failed (base has no intent commit).
  - B(ii): reclaimed on a mark that had already been re-stamped.
  - B(iii): adoption answered `Committed` (want `Conflict`).
  - B(iv): the `reclaiming` mark is unreadable on base; nothing deleted.
  - B(v): the deleted fragments' keys were left behind by the fault.
  - C: 0 commits carried a precondition or put on an `orphan:` key (want 2).
  - D: pass 1's control (a structured mark whose obligation is gone) not reclaimed on base.
- **DST E on base** (scratch worktree reset to `f41e9c5` + only the new `custodian.rs`, run via
  `./engine/xtask.sh dst`): the three tests that run property 14 fail by assertion —
  `gc_reclaim_intent_never_publishes_over_deleted_bytes`, `gc_reclaim_intent_reaches_both_outcomes`
  and `committed_regression_seeds_stay_green` — all with "the committed placement names server 1,
  whose copy of the chunk was deleted" (outcome (c)); e.g. `[Pass, PreMarkRead, DeleteBegan(1,..),
  Adoption(Committed), Deleted(1,..), …]`. The other 17 DST tests pass on base. With the fix all
  20 pass (`./engine/xtask.sh dst`).
- **F**: `./engine/xtask.sh ci` — see the last section.

## Refute-my-own-test (forced)

- **(a) Genuine red?** Yes. C4-verify's RED leg reverted `metadata.rs`, `gc.rs`,
  `reconciliation.rs`, the DST file and the docs, kept the new test, and 8 of 9 tests failed by
  assertion (list above); the ninth is the base-green guard. The DST property was run on the
  unpatched base separately and failed by assertion in all three tests that exercise it.
- **(b) Production path?** Yes. Every custodian leg calls the production `reconcile_step` (a fresh
  `GcContext` per pass) or `reconcile_after_restore`; only the stores are doubles
  (`MetadataStore`/`ChunkStore` impls). The DST drives the same entry point over the DST tier's
  simulated-TiKV model. The codec tests call the production codec. Nothing re-implements GC.
- **(c) Fixture includes the fault?** Yes. B(i) the store really fails the intent commit; B(ii) a
  re-stamp really lands after the ledger page hands the mark out; B(iii) a real adoption commit is
  made from inside `delete_fragment`; B(v) a real delete fault (third of a recorded batch) and a real
  listing fault; D real `retire:bytes:` obligations that `decode_retire_obligation` accepts. The DST
  mover is a concurrent madsim task, and the coverage leg proves the adoption is answered inside
  GC's delete in at least one run (the landing the old order turns into outcome (c)).

## Alternatives ruled out (with costs)

1. **Hold the raw value `Bytes` for the precondition instead of re-encoding the decoded mark.**
   `wyrd-custodian` has `bytes` only as a dev-dependency (`crates/custodian/Cargo.toml`), so naming
   `Bytes` in `gc.rs` needs `+bytes.workspace = true` under `[dependencies]` (one manifest line, a
   production dependency change). A `Vec<u8>` copy avoids the manifest but adds up to 65,536 ×
   ≤310 B ≈ 20 MB per window beside the decoded mark GC keeps anyway. Re-encoding is exact because
   decode refuses anything its own encode would not produce (tested round-trip) — the rubric's
   "serialization identity" pattern. Chosen: re-encode, no manifest change.
2. **On `Conflict`, re-read every mark in the batch and re-commit the unchanged ones** instead of
   one commit per intent. Costs up to 1,000 `get`s plus a re-commit that can conflict again, so it
   needs its own loop and bound; the per-intent fallback is ≤1,000 small commits, one round, and
   only on a batch that actually lost. Chosen: per intent. (Leg C still pins exactly 2 commits on
   the no-conflict path, which kills v1's commit-each-alone mutant.)
3. **Make the key delete conditional on the `reclaiming` bytes.** Brief: "the key deleted after as
   today". A conditional delete lets one changed key fail its whole cleanup batch (up to 1,000),
   needing the same per-key fallback; under the writer rule nothing changes a `reclaiming` mark.
   Rejected.
4. **Retry a failed cleanup commit in the fault path.** A failed commit may have an unknown result;
   re-applying a blind delete after a first attempt that landed could delete a mark a writer wrote
   in between (the writer rule has it wait for the key to vanish, then write under
   `require_absent`). The fault path therefore commits only deletes never attempted. Rejected.
5. **Per-D-server intent flush.** Would make the commit count depend on the fleet shape; leg C
   spreads 1,001 marks over 4 servers so it fails. Chosen: one pass-wide batch.
6. **Route the core delete-path writers (`unlink`, both superseding commits,
   `metadata.rs:2118`, `:2226`, `:2299` in the patched tree) through the codec too.** Three
   one-line changes; their bytes are already the codec's legacy encoding (pinned by
   `the_legacy_shape_is_every_existing_writers_bare_decimal`, `:3996`). Left alone to keep the diff
   to what the slice needs; `restore.rs` / `reconstruction.rs` / `rebalance.rs` are out of scope
   and also write the identical bytes.
7. **A new `Reconciled` variant for a lost intent.** Brief forbids it; `Partial` already means "a
   caller driving to satisfaction must keep going".

## Expectations that changed

- A bare decimal in a non-canonical spelling (`007`) was read by base GC as a stamp (base
  `str::parse::<u64>`) and could be reclaimed; it is now refused as non-canonical, kept, and named.
  No in-tree writer produces such a value and no existing test relied on it.
- `emit_unreadable_mark` gained a `fault` field and new message text; `gc_ledger_walk.rs` D(iii)
  asserts action/target/mark only and still passes. `emit_skip`'s message text changed (no test
  reads it). New: skip reasons `draining-retirement`, `mark-changed`; reclaim reason `resumed`;
  audit action `cleanup-lost-after-fault` with counter `gc_cleanup_lost_after_fault`.
- A reclaiming pass now makes one extra commit per ≤1,000 marks before the deletes. The two
  timing-sensitive DST coverage properties (12 and 13) still reach their windows.
- The unit-test entry I first wrote for "a JSON `\u` escape" had its escape turned into a plain
  `g` when the file was written, so the refusal table contained a canonical value and the full
  gate's first run failed on it; it is now built from `char::from(92)`.

## Commit-readiness

`cargo fmt --all` applied (`cargo fmt --all -- --check` clean); clippy clean on the touched crates
and on `wyrd-dst` under `--cfg madsim` (via `xtask dst`). `typos` and the docs lint/render ran
inside `xtask ci` (present locally, not skipped). No external dependency missing.

## Gate F

`./engine/xtask.sh ci` on the final tree: **`xtask ci: all checks passed`** — typos, docs lint
(`lint_docs: OK`) and render (`render_site: link audit OK`), gitlink/unsafe guards, `cargo fmt
--check`, workspace clippy/build/test (the new file's 9 tests and the 5 codec unit tests among
them), cargo-machete, cargo-deny, conformance vectors, the ADR-0035 statics gate, the deploy guard,
and the DST tier (clippy + `cargo test -p wyrd-dst` under `--cfg madsim`, 50 seeds; properties
12/13/14 all green). The first full run failed on my own codec unit test (the `\u` entry noted
above); the second, after that fix, passed.

The final C4-verify run on this exact `patch.diff`: GREEN 9/9; RED 8 failed by assertion, 1 (the
restore guard) passed by design — "PASS — red without the fix, green with it".

No NEEDS-HUMAN item: no external dependency was missing, and nothing here is GUI- or
display-bound.

Scratch: the C4-verify worktree (`$PDCA_SCRATCH/pdca-builder-804-verify`, branch
`pdca-verify-lb804`) was created only for these runs and removed afterwards with `git worktree
remove` / `git branch -D`; run logs stay under `$PDCA_SCRATCH` for the harness to reclaim.
