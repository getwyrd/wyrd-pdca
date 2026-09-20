# Build notes — issue 661 / gc-orphan-ledger-paged-walk

## What changed and why

Base defect (`crates/custodian/src/gc.rs:522-538`'s `orphan_leases`, called at `:177`, and
`crates/custodian/src/restore.rs:308`): the `orphan:` ledger is read with one `MetadataStore::scan`,
which fails whole (no partial result) past `SCAN_CAP` (`wyrd_traits::SCAN_CAP`, `crates/traits/src/lib.rs:286`).
One maximum segmented-object retirement installs ~1.78M marks (0016:1392-1398), so a single large
delete permanently takes GC down, and the post-restore pass that would shrink the ledger can never
start either — self-sealing. Separately, a mark whose position holds no fragment is never visited,
because GC only ever consumes an `orphan:` key while iterating an actual `list_fragments()` result
(`gc.rs:183-219` on the base) — so fragment-less marks (teardown pre-marks whose write never landed)
accumulate toward the same cap forever.

### GC (`crates/custodian/src/gc.rs`)

- `orphan_leases`'s single `scan` is now `orphan_page` (new) + a loop for callers needing the whole
  ledger (restore) — both go through `MetadataStore::scan_page`, never `scan`.
- `gc::reconcile` reads **one bounded page** per pass — `ORPHAN_PAGE_BUDGET = 65_536` (1/16 of
  `SCAN_CAP`, the brief's own ceiling) — resuming from a cursor persisted as a plain metadata record
  (`gc:orphan-cursor`, deliberately outside the `orphan:` namespace so the walk's own bookkeeping is
  never itself paged as an entry). The cursor is written in the SAME commit as that page's deletes, so
  a page's cursor never advances past deletes that did not land.
- **Fragment-less-mark sweep** (X87/X91/X96, `0016:1359-1404`, `:1381-1391`, `:2625`): after the
  ordinary per-fragment fleet walk, every entry in the current orphan page whose `(dserver, frag)` this
  pass's OWN `list_fragments()` calls did not report — and whose dserver IS in this pass's fleet — is
  swept (ledger entry deleted, no `delete_fragment` call — there are no bytes) once
  `now_millis >= orphaned_at + LATE_WRITE_DEADLINE_MILLIS`. The "this pass's own listing" requirement
  (X96) falls out of the design for free: `present` is rebuilt from `list_fragments()` inside `reconcile`
  every call, never cached across passes.
- `LATE_WRITE_DEADLINE_MILLIS = W_REPOINT_MILLIS (5s) + W_WRITE_MILLIS (10s) + DELTA_CLOCK_MILLIS (1s)
  = 16_000`. No fixed global constants for `W_repoint` / `W_write` / `δ_clock` exist elsewhere in the
  tree (they are per-write negotiated deadlines, `crates/traits/src/lib.rs:1079`, `:808-844`) — this
  slice picks conservative, documented fixed values for its own derived deadline, each with its
  reasoning in the doc comment.
- All orphan-related deletes (reclaims, sweeps) and the cursor write commit in bounded batches of
  `GC_COMMIT_BATCH = 1_000` (mirrors `restore.rs`'s `MARK_BATCH`), never one page-sized transaction —
  Scope's "a pass that reclaims B marks must not hand the backend a B-key transaction."
- No change to `GcContext`'s fields, `reconcile_step`'s signature, or `reconcile_after_restore`'s
  signature (Scope's hard constraint).

### Restore (`crates/custodian/src/restore.rs`) — **zero lines touched**

`orphan_leases`'s signature and restore's call site (`restore.rs:308`) are unchanged. Its
implementation now loops `orphan_page` (bounded `scan_page` calls) to exhaustion instead of one
`scan` — sound because restore is a one-shot operator command (module docs: "run with writers
stopped"), not a continuous loop bound by a per-pass footprint the way `gc::reconcile`'s own walk is.
This satisfies leg F's requirement that restore's "already marked" judgement see EVERY existing mark,
on whichever page it sits, without ever calling `scan`.

### `crates/server/src/custodian.rs` — leg E

Added a `const _: () = assert!(wyrd_custodian::gc::LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS, ...)`
right after `GC_GRACE_WINDOW_MILLIS`'s own definition (`:114`). `LATE_WRITE_DEADLINE_MILLIS` is `pub`
(not `pub(crate)`) in `gc.rs` for exactly this cross-crate comparison. See "Where leg E's proof lives"
below for why this isn't in `gc_ledger_walk.rs`.

### `docs/design/architecture/06-runtime-view.md`

Section 6.7 ("Delete and space reclamation") step 2 now describes the paged walk, the persisted
cursor record, and the fragment-less-mark sweep — the new persisted record this slice adds
(AGENTS.md docs-currency rule).

## Where leg E's proof lives (Do's call, per the brief)

`GC_GRACE_WINDOW_MILLIS` is a private `const` inside `crates/server/src/custodian.rs`'s own module
(not `pub`, not `pub(crate)`), deliberately — it is deployment wiring, not `wyrd-custodian`'s library
API. `crates/custodian/tests/gc_ledger_walk.rs` compiles as a separate crate and cannot name it. So
leg E — `D` strictly inside the DEPLOYED grace window, proved against `GC_GRACE_WINDOW_MILLIS` ITSELF,
never a copy of its value — lives as a `const` assertion inside `wyrd-server`'s own
`crates/server/src/custodian.rs`, right beside the constant it checks. I chose a `const` assertion
over a `#[test]` because both operands are compile-time constants: `clippy::assertions_on_constants`
correctly flags a runtime `assert!` on two consts as pointless (I hit this — first attempt used a
`#[cfg(test)] mod tests { #[test] fn ... }`, which `cargo clippy -D warnings` rejected), and a `const`
assertion is strictly stronger anyway — checked on every `cargo build -p wyrd-server`, not only a
`cargo test` run that could be filtered past it.

## The per-pass budget and late-write deadline (Do's derivation)

- `ORPHAN_PAGE_BUDGET = 65_536`: the brief's own ceiling ("at most 65,536 (1/16 of SCAN_CAP)"). Chosen
  at the ceiling rather than smaller so a maximal ~1.78M-mark retirement drains in as few ordinary
  passes as the brief allows (~28).
- `LATE_WRITE_DEADLINE_MILLIS = 16_000`: see above. It only has to satisfy `D < GC_GRACE_WINDOW_MILLIS`
  strictly (leg E; deployed grace is `LEASE_TTL_MILLIS = 60_000`) and be large enough that a legitimate
  in-flight repoint/write is never swept out from under its worker — 16s against a 60s grace leaves
  ample headroom in both directions.

## Alternatives considered and rejected

- **Restructuring the fleet-fragment loop to also drive the mark sweep from inside it** (i.e., checking
  "does this orphan-page entry have a corresponding fleet fragment" as part of the SAME loop that walks
  `list_fragments()`): rejected because the fleet loop is naturally keyed by `(dserver, frag)` PRESENT
  on disk, while the sweep needs to iterate ledger entries ABSENT from disk — the two loops read
  different sets and conflating them would mean re-deriving "is this in `present`" per orphan-page
  entry anyway. Two small loops sharing one `present` set (built once) is the same cost, clearer to
  read, and keeps the safety-gate/grace-test/conservative-arm code the brief says to preserve
  "exactly as they judge" (`gc.rs:191`, `:196-205`, `:206-210`) completely untouched — a diff of ~15
  lines touched inside that block (adding `present.insert` and doc comments) vs. the sweep's own new
  ~25-line loop, rather than interleaving new branches into the existing one.
- **A separate `[[doctor.checks]]`-style external cursor store** (e.g. a dedicated small table) instead
  of a plain metadata key: rejected — `desired:dserver:<id>` (`desired_state.rs:33`) is the existing
  precedent for "custodian control-plane state is a plain metadata-ledger entry," and inventing a
  second mechanism for one cursor would be unmotivated complexity for no benefit `WriteBatch` doesn't
  already give a single `put`/`delete`.
- **Bounding `expired_pending_chunks`'s and `referenced_fragments`'s scans too**: out of scope per the
  brief ("GC's and restore's reads of the `orphan:` ledger" — not `inode:` or `pending:`). Left
  untouched; `cargo test -p wyrd-custodian` and the DST campaign (below) both still pass, so this
  slice introduces no regression on those paths either.
- **Making GC's fragment-less-mark sweep aware of the reference set (`referenced_fragments`)**:
  considered, then rejected as unnecessary — a fragment-less mark has no bytes to protect (nothing is
  ever passed to `delete_fragment` in that branch), so the safety gate that exists to keep a committed
  chunk map's bytes alive has nothing to say about deleting a bookkeeping record for a position with
  nothing on it. Gating the sweep on `referenced.unresolvable` being empty would ALSO leave
  fragment-less marks stuck forever whenever any other unrelated object in the store is unresolvable —
  worse, not safer.

## Falsifiability — how many tests ran red, and how

Ran via the project's own gate, `./engine/scripts/run-verify.sh` (C4-verify, `pdca.toml`), which
applies `patch.diff` to a clean worktree, runs GREEN (fix applied), then reverts the production change
(keeping the test) and runs RED:

```
GREEN: 9 passed; 0 failed
RED:   4 passed; 5 failed
  d1_a_fragment_less_mark_aged_exactly_d_is_swept                          FAILED (assertion)
  gc_survives_a_ledger_past_the_cap                                        FAILED (assertion: Err(ScanCapExceeded))
  one_pass_reads_a_bounded_pinned_amount                                   FAILED (assertion: Err(ScanCapExceeded))
  the_tail_does_not_starve_behind_a_retention_safe_head                    FAILED (assertion: Err(ScanCapExceeded))
  restore_survives_the_ledger_and_never_re_stamps_an_unread_mark           FAILED (assertion: Err(ScanCapExceeded))
run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
```

Every RED failure is an **assertion failing on a returned value** (`Err(...)` from `reconcile_step` /
`reconcile_after_restore`, or a swept-mark assertion) — none is a compile error, so none is
UNVERIFIABLE; the file compiles clean against the reverted base (it names only symbols already present
on `origin/main`: `reconcile_step`, `reconcile_after_restore`, `GcContext`, `ExpiredPendingPolicy`,
`Custodian`, `FencedZone`, `orphan_key`, `ORPHAN_PREFIX`, and the `wyrd_traits` seam types/helpers —
nothing this slice adds). The 4 legs that stayed green on the base (`d2`, `d3`, `d4`, `d5`) are exactly
the ones the brief predicts stay green pre-fix ("guard against over-deletion... may already be green on
the base; that is expected").

### The three refutation questions

- **(a) Genuine red?** Yes — shown above, reproduced independently by hand (manual `git stash` of the
  production changes, same 5 failures, same assertions) before running it through the project's gate.
- **(b) Production path?** Yes — every leg drives the real `wyrd_custodian::reconcile_step` /
  `reconcile_after_restore` (the same fenced control point / operator entry point production calls),
  over in-memory doubles for the metadata store and D-server fleet only (`crates/custodian/tests/gc.rs`
  already does this for the sibling suite; "Production reach" in the brief). No copy or
  re-implementation of `gc::reconcile`'s decision logic exists anywhere in the test.
- **(c) Fixture includes the fault?** Yes — legs A/B/C/F seed populations that genuinely exceed the
  double's own lowered `scan` cap (and, for B/C/F, the real per-pass budget `B`); leg D seeds a mark
  with NO corresponding on-disk fragment (the actual fault class — a stranded pre-mark) rather than a
  curated case that avoids it; leg D(v) actually lands a fragment mid-sequence to exercise the
  earlier-pass-listing hazard (X96), not a static fixture.

## What I verified, and how

- `cargo test -p wyrd-custodian` — all 11 pre-existing files + `gc_ledger_walk.rs` (9 new tests): green,
  no regression.
- `cargo test -p wyrd-server --test custodian_gc --test custodian_day_one` — the deployed-role
  integration tests (including `deployed_role_reclaims_orphaned_bytes_after_grace_elapses`,
  `deployed_role_reclaims_at_the_exact_grace_boundary`): green, no regression.
- `cargo test -p wyrd-server --lib` — the leg-E const assertion participates in every build; confirmed
  by building the crate (a `const` assertion has no separate "test" to point at — a false one would
  fail `cargo build -p wyrd-server` outright, which I also ran green).
- DST campaign: `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian` —
  all 14 Tier-0 properties green over 50 seeds each, including `committed_regression_seeds_stay_green`
  and the two properties that read the orphan ledger through `reconcile_step`
  (`gc_reclaims_only_true_orphans_q3`, `gc_over_a_segmented_map_never_reclaims_it_and_never_over_certifies`)
  and the restore two-reading property (`restore_two_readings_never_license_a_mark`,
  `restore_two_readings_cover_the_divergence_window`).
- `cargo fmt -p wyrd-custodian -p wyrd-server -- --check` and
  `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets -- -D warnings`: clean.
- `typos` over every file this patch touches: clean.
- `./engine/scripts/run-verify.sh` (project's own C4-verify gate): PASS, shown above.
- Did **not** run the full `cargo xtask ci` (leg G) end-to-end inside this session — it also builds
  and tests every other crate in the workspace (FDB/TiKV feature matrices, S3 gateway conformance,
  etc.) unrelated to this slice's diff, and `pdca.toml`'s own `default_timeout_secs = 7200` reflects
  how long a cold full run can take. Ran the targeted equivalent instead: full workspace build
  (`cargo build --workspace`), the two affected crates' fmt/clippy/tests, the DST campaign at full
  seed count, and the deployed-role integration tests — the parts `cargo xtask ci` runs that this
  diff could plausibly affect. Check's own gates re-run the real `C4-ci` gate.

## Did I consider a leader-change / concurrent-custodian angle for the cursor?

Yes (Scope explicitly leaves this "Do's call, stated in build-notes.md"). The cursor is a plain
metadata record read-then-written inside the SAME commit as that pass's deletes, through the ordinary
`MetadataStore::commit` — no separate precondition on the cursor key itself. Two custodians can never
run this concurrently by construction: `reconcile_step` is gated by `zone.authorize(custodian.term())`
at its very start (`reconciliation.rs`), so only the currently-fenced leader ever reaches `gc::reconcile`
at all — a superseded custodian's call is rejected before it reads or writes anything, cursor included.
A leader change BETWEEN passes is therefore exactly as safe as a leader change between any other GC
pass today: the new leader's first pass reads whatever cursor the last committed pass left, and resumes
from there (or re-laps from the start if that pass's cursor was `None`). I did not add a
precondition (`WriteBatch::require`) on the cursor key across the read-then-write, because the fence
already rules out the only actor that could race it.

## Rubric self-review note: DST coverage for the new sweep path

The target repo's review rubric asks that "a new destructive or concurrent path lands with
seeded Tier-0 DST coverage." The fragment-less-mark sweep is new and does delete a metadata
record, though never a fragment's bytes (nothing is ever passed to `delete_fragment` on that
branch). I did not add a new `crates/dst/tests/custodian.rs` property for it: the brief's
"Out of scope" list does not mention DST, its Success criterion's legs (A-G) are all
in-process `crates/custodian/tests/gc_ledger_walk.rs` legs plus the whole-tree `cargo xtask
ci` gate, and child-2 (#662) is where the richer `orphan:` value shapes and keyed retire
protection land — a natural place for a seeded property over the fuller sweep behavior. I
did run the FULL existing DST custodian campaign (50 seeds/property, including the two GC
properties and the two restore two-reading properties that already exercise `orphan:`
through the real `reconcile_step` / `reconcile_after_restore`) and it stayed green, so the
change is not silently broken under the simulator — but that is confirmation, not new
seeded coverage of the sweep's own X96/X87/X91 timing rules. If review wants seeded DST
coverage of the sweep specifically, I'd flag it as a fair finding to defer to #662 (or a
follow-up) rather than something this bundle already covers.

## Docs currency

`docs/design/architecture/06-runtime-view.md` §6.7 updated (a new persisted record — the walk cursor —
and the paged-walk/sweep behavior it drives). This is the "living architecture doc" AGENTS.md's
docs-currency rule (§154-157, cited in the brief) points at for the custodian loops.
