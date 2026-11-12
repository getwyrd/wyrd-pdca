# Build notes — issue 1012 (#659 child-1): retirement drain step, cursor, deployed dispatch

Target: `getwyrd/wyrd` @ main, built on the bundle's stack base
(`pdca-integration/main-r9dd123c9ed70`, tip `db1a066`). Every `path:line` below is on the
patched worktree.

## What the patch does

1. **Cursor inside the obligation** (`0016:613-619`, no cursor record class).
   `crates/core/src/multipart.rs`:
   - `RetirePayload` gains `cursor: Option<RetireCursor>`, omitted when absent
     (`multipart.rs:3982`), wire `cursor: Option<RetireCursorWire>` (`:3888`).
   - `RetireWork` (`:4252`, drain order seg → parts → chunks → generation) and
     `RetireCursor { component, entry, mark }` (`:4297`), spelling documented in the type's doc
     table. Closed wire shape (`:4333`).
   - Decoded against the payload: `checked_cursor` (`:4063`) at decode (`:4405`), plus the one
     key-relative rule (a `retire:records:` cursor's mark must be 0) in `checked_cursor_mode`
     (`:4230`), applied by `checked_against_key` and by the writer `with_cursor` (`:4050`).
   - Typed errors `RecordError::RetireCursorNoWork` (`:554`) and `RetireCursorOutOfRange`
     (`:563`), with `Display` arms.
   - `PartNumberSet::first_from` (`:3476`) — the drain's paging step over a set.
   - Every installer constructor sets `cursor: None`, so no installed obligation's bytes change
     (pinned by `mod drain_cursor` at `:6496`).
2. **The drain** — new `crates/custodian/src/retirement.rs`:
   - `reconcile` (`retirement.rs:154`): walks `retire:` in `checked_page` pages of
     `OBLIGATION_PAGE` (`:122`), at most `STEPS_PER_PASS` = 16 commits a pass (`:118`).
   - One step = `Step::plan` → `run` (`:496`) → `finish` (`:540`): `require(retire:… == prior)`
     first, then the work, then a cursor put or (finishing step) the obligation delete.
   - Budget (`:396`) reserves the obligation's own require + put, then counts every precondition
     and mutation against `MAX_BATCH_OPS` / `MAX_BATCH_BYTES`; mark reads capped by
     `POSITIONS_PER_STEP` (`:131`).
   - Mark guard `mark` (`:1001`): arm 0 (`reclaiming`, `:1029`) holds byte-identical; (a) skip;
     (b) `require(== prior)`; (c) `require_absent`. An unreadable mark holds too.
   - Bytes `{parts}` (`staged_part`, `:767`): validate every chunk's staged placement
     (`gc::staged_placement`, exact length) before any mark; marks; then segments-before-parts
     (`segments_owed`, `:828`); then `part:` + `psum:` deletes. Records `{parts}`: blind deletes,
     never marks (`parts`, `:672`).
   - `{seg}` records (`seg`, `:627`), `{chunks}` and flat `{generation}` (`chunk_list`, `:879`),
     segmented `{generation}` re-reading each `seg:` record's current placement (`segments`,
     `:912`).
   - `session` component: held while the session's `sidx:` range is non-empty (`owned_residue`,
     `:606`) — the owned walk is child-2's.
   - Holds (`enum Hold`, `:277`) are typed; the ones a human must clear make the leg answer
     `Blocked`, the waiting ones `Partial`. Audit target `wyrd.custodian.retirement.audit`.
3. **Dispatch** — `crates/custodian/src/reconciliation.rs:166`: the retirement leg runs inside
   `reconcile_step`'s reconstruction input, after the reconstruction loop; both legs run whatever
   the other answered, first error wins. Production caller unchanged:
   `crates/server/src/custodian.rs:397` → the reconstruction pass of `run_reconstruction_until`,
   which runs every interval. Doc + log label updated (`server/src/custodian.rs:461`, `:606`,
   `:629`).
4. `crates/custodian/src/gc.rs:1472`, `:2068`: `staged_placement` and `checked_page` made
   `pub(crate)` (shared, not copied).
5. `crates/custodian/src/restore.rs:1157`: the `deferred: #659` marker replaced with the
   guarantee (segments before parts; the records obligation goes only once its range is empty).
6. Docs: `docs/design/architecture/06-runtime-view.md:84` (new paragraph in 6.7 item 2: bounded
   drain, mark-before-delete, grace from the drain, arm 0 and resume after GC's key delete, the
   cursor, the downgrade note); `05-building-block-view.md:246` (the cursor as a persisted
   component; the drain is no longer "not yet landed").
7. Tests: `crates/custodian/tests/multipart_retirement_drain.rs` (new, legs B, C, C′, E-hold,
   F, G); `crates/core/src/multipart.rs` `mod drain_cursor` (codec units); DST property 21
   appended to `crates/dst/tests/custodian.rs:6435` (+ two `dst_campaign_test!` entries and the
   regression-seed replay line).

## Deviations from the brief, each with its reason

- **`Cargo.lock` changes by one line** (`"wyrd-metadata-redb"` added to `wyrd-custodian`'s list).
  The brief puts `Cargo.lock` out of scope but its Success criterion requires the custodian test
  to run over `RedbMetadataStore::in_memory()`, and `wyrd-custodian` had no redb dev-dependency.
  A workspace path crate, no new external crate, `cargo deny` unaffected. The alternative
  (an in-memory double, as the other custodian tests use) would not meet the criterion as
  written. Human to accept or reject at sign-off.
- **`bytes` moved from custodian dev-deps to deps.** The drain holds `Bytes` (the seam's value
  type). Already in the lockfile's list for `wyrd-custodian`, so no lockfile change from this.
- **Which leg carries the drain.** Brief allows either an existing context or a new parameter.
  I put it on the **reconstruction** input, not GC's. Rejected GC's, with the cost measured:
  `crates/custodian/tests/gc_reclaim_intent.rs:1143-1279` (leg D, 137 lines) plants
  `retire:bytes:` obligations and runs **GC-only** passes, asserting (1) the obligations stay
  installed across passes until the test deletes them one at a time, and (2) "no pass reads
  `retire:` by range" (`no_retire_scan`, `:1207-1218`). A drain in the GC leg reads `retire:` by
  range every pass and finishes both obligations in pass 1 (their marks already carry their
  own event, arm (a), and they name no record), so both assertions fail; leg D would have to be
  rewritten, weakening a merged GC test. Also: the deployed GC pass is gated on the whole
  operator fleet being visible (`server/src/custodian.rs` GC block); that gate exists for
  GC's chunk-wide `pending:` evidence, not for the drain, which only writes evidence and deletes
  evidenced records. On the reconstruction input the existing suites are untouched (core,
  custodian, server: all 100 test binaries green after the change). Child-2/child-3 add to the
  same leg (`reconciliation.rs:155-166` comment).
- **`session` component checked first**, holding the whole obligation while the session's
  `sidx:` range is non-empty. Checking it last would need a cursor position past the last
  entry, which decode (rightly) refuses. In production nothing has residue yet; child-2's owned
  walk empties the range and the drain then proceeds.
- **Segments-before-parts granularity.** While the same token's records obligation owes
  `{seg}`, the bytes drain marks the part it reached and stops there (cursor `(parts, n, total)`)
  rather than marking every part ahead. Safe and simple; "may still write marks" is honoured for
  that part. Test G shows marks land before the records obligation is gone.
- **"A single entry whose own deletes exceed one batch."** No entry in this protocol has more
  than two deletes; the entry whose own work exceeds a batch is a part whose **marks** do
  (158 RS(6,3) chunks = 1,422 positions = 2,844 ops). Test C uses that and asserts ≥ 6 commits
  with the cursor's mark strictly increasing inside the one entry.
- **A named part with no record is a hold**, not a skip (first draft skipped it with a warn
  audit; changed on self-review against the rubric's "absent entries" rule — the obligation
  would otherwise finish as drained with unknown bytes unmarked).
- `RS(0,0)` from the brief's E-hold example cannot decode as a `part:` record
  (`checked_chunk_scheme` refuses it), so E-hold uses the decodable failure: an RS(4,2) chunk
  placed on 5 servers.

## Rejected alternatives (with cost)

- A walk bookmark record for fairness across passes (like `gc:orphan-cursor`): the brief says
  "no new key or record class". Instead each pass walks from the head; held obligations commit
  nothing and do not spend the step budget, so they never starve later ones.
- New parameter on `reconcile_step`: 25 existing test files call it (`git ls-files 'crates/*/tests/*.rs' | xargs grep -l reconcile_step | wc -l` = 25), each call site would change. Not needed.

## Red → green (each a temporary edit, the named leg run, the file restored with `cmp` check)

All via `cargo test -p wyrd-custodian --test multipart_retirement_drain` (and the DST one via
`RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test custodian retirement_drain` in a scratch
target dir), each under `timeout`.

1. **Dispatch removed** (`retirement::reconcile` never taken in `reconcile_step`): all six legs
   fail; B with `the obligations never drained`. (A first try that deleted the call outright did
   not compile — dead code under `-D warnings` — so it proved nothing; redone as a never-taken
   branch.)
2. **Cursor not persisted** (each step stores the payload without its cursor): C fails,
   `the obligations never drained` (records steps re-delete parts 1..249 forever).
3. **Obligation deleted after the first batch**: C fails,
   `000…c1's obligation is gone with 3877 of its parts left` (assertion order fixed so this,
   not the fault-injection check, is what fires).
4. **Untrusted `part:` deleted without marking**: E-hold fails, `never deleted` (assertion
   order fixed so the record check fires before the outcome check).
5. **Arm 0 removed** (`reclaiming` treated as another event): C′ fails, `(0) is never written`
   with the reclaiming bytes required; DST property 21 fails on the GC-wins interleaving:
   `the drain retried its re-stamp against GC's reclaiming mark (drain at 6 ms):
   [GcPass, GcReadStamped, DrainReadStamped, Restamp(Conflict), Swap(Committed),
   DrainReadReclaiming, RestampOverReclaiming, …]`.
6. **Segments-before-parts removed**: G fails, `after drain commit 0 restore named the session`,
   fault `ChunkInNoPart`.

Green: 6/6 custodian legs; 4/4 `drain_cursor` units; DST property 21 and its coverage leg over
50 seeds (`MADSIM_TEST_NUM=50`), clippy `-p wyrd-dst` under madsim clean.

## Refuting my own test

- **(a) Genuine red?** Yes — six separate reverts above, each red on the leg's own assertion.
  Also, the test file cannot compile on the base (it names `RetireCursor`, `RetireWork`), the
  expected C4-verify UNVERIFIABLE.
- **(b) Production path?** Yes. Every leg drives the production `reconcile_step` with the
  contexts the deployed loop builds (`ReconstructionContext` for the drain, `GcContext` for GC),
  over the real `RedbMetadataStore::in_memory()`; the wrapper only records and (C) fails one
  commit. Leg B's obligations come from the production Complete (`create_session`,
  `upload_part`, `complete_upload`); leg G's from the production `reconcile_after_restore` fence,
  and restore is the production pass. No test-only entry into the drain exists (`reconcile` is
  `pub(crate)`).
- **(c) Fixture includes the fault?** Yes. C′ plants the `reclaiming` mark on a position the
  obligation names; E-hold plants the malformed placement and the fleet holds that chunk's
  fragments; G plants 600 surviving segments over 600 parts so independent draining produces
  `ChunkInNoPart`; C injects a real failed commit mid-pass; the DST row runs GC and the drain
  concurrently on the same stamped mark and its coverage leg proves both winners and the
  "re-read finds reclaiming" interleaving are reached.

## Gate

`./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree, run on the final tree):
`xtask ci: all checks passed`, exit 0. That covers typos, `lint_docs`, `render_site --check`, the
repo guards, `cargo fmt --check`, workspace clippy/build/test (all green, the new custodian test
and the existing GC, restore, reconstruction and server suites included), cargo-machete,
cargo-deny (both configs), conformance vectors, the statics gate, and the madsim DST tier at 50
seeds — `retirement_drain_restamp_never_overwrites_reclaiming` and
`retirement_drain_race_reaches_both_winners` both ok, plus `clippy -p wyrd-dst` under madsim.
`patch.diff` applies cleanly to the stack base tip `db1a066` (`git apply --cached --check` on a
throwaway index). No NEEDS-HUMAN external dependency: `typos` and the docs renderer deps were both
present. C4-verify is expected to report UNVERIFIABLE (net-new test), as the brief pre-declares.

## Rubric self-review (AGENTS.md "Review rubric & protocol")

- One clock: the drain reads no clock; marks are stamped with the step's `now_millis`, the
  reading GC ages them on (module doc, `retirement.rs` "One clock").
- Narrow seams: `MetadataStore` only; no concrete backend in `custodian/src`.
- Validation boundaries: placement length stays liberal at decode; the drain (a maintenance path)
  is strict and holds. Cursor rules are structural → decode errors.
- No shared mutable globals; no new crate root.
- Docs currency: persisted field (cursor) and new behaviour documented in 05 and 06.
- Serialization identity: cursor omitted when absent; round-trip unit test; installers' bytes
  unchanged (unit test).
- Absent/unsupported entries: unreadable obligation, untrusted record, unreadable mark, absent
  named part → typed hold + audit + `Blocked`, never success.
- Transactions: a step's `Conflict` re-reads and re-plans; an `Err` (incl. `CommitUnknownResult`)
  ends the pass as an error, never reported as a conflict.
- Await discipline: every await is a store call bounded by the backend's own bound, as GC's.
- Test fidelity: seeded Tier-0 DST row for the new concurrent destructive path.

## Scratch left for the harness

`$PDCA_SCRATCH/pdca-builder-1012-redleg/` (about 2.3 GB: the madsim build cache used for the DST
red/green runs, file backups for the red edits, the gate log). My `rm -rf` of it was refused by
the sandbox; it is inside this bundle's scratch root, which the harness reclaims at
publish/freeze.
