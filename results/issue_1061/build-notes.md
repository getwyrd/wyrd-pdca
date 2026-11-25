# Build notes — #1061 core,custodian: retirement drain, records mode (636.6a-1) — iteration 2

Base: `c27cdb0` (`pdca-integration/main-r9dd123c9ed70`, the fold of `main` with #1008, #1009,
#1010, #1011, #1029, #1030). Every `path:line` below is on that base with this patch applied.

## What this iteration is

A fix round, as the sign-off asked: the previous attempt's patch (`iteration-v1/patch.diff`) is
the starting point, unchanged where Check found it sound (the core cursor codec, the installer
ceiling, the dispatch, the step logic for `{parts}`), and changed in the three places the
carry-forward names, with two sentences added to the runtime-view doc to match. The previous notes' description of the slice still holds for everything not
listed here; the parts that changed are below.

## The three carry-forward items, and what I did

### 1. `seg_step` honoured no continuation (the clamped page)

`crates/custodian/src/retirement.rs:515` (`seg_step`). The page from the cursor asks for one batch
plus one row (`take + 1`). Before, a page with no extra row was taken for the end of the group,
whatever `next` said; a store that cut the page short (`scan_page` allows it,
`crates/traits/src/lib.rs:1402`; redb's `with_scan_cap`, `crates/metadata-redb/src/lib.rs:86`)
made the step delete the obligation with records left.

Now (`retirement.rs:535-564`): `next` is kept (`more`). What follows the batch is the page's own
extra row when it has one (`:544-545`); otherwise, when `more` is `Some`, one more one-row read past
the page's last key finds the record the cursor moves to, or — an empty page is terminal — that
none follows (`:546-556`). Only then is the group a candidate to finish, and the whole-range read
from the start still runs when the cursor is past 0 (`:565-583`). The obligation is never deleted
while a read says more may follow. Keys are matched by exact spelling (`segment_index`, `:589`,
replacing the old nonce/epoch guard, which the prefix already fixed).

Why the extra read and not "cursor = last index + 1":
- the cursor's `{seg}` entry is bounded at `MAX_SEGMENT_INDEX` by decode
  (`crates/core/src/metadata.rs:536`, `multipart.rs:4250`); a short page ending on segment 999,999
  would need entry 1,000,000, which decode refuses, so that group could never finish;
- when the short page is exactly the end of the group, "last + 1" leaves the obligation alive for
  a further step that deletes zero records and then the obligation, which breaks the brief's (C)
  "deleted only in the batch that deletes its last record".
Cost of the lookahead: one read, only on a step whose page the store cut short.
`READS_PER_STEP` goes 3 → 4 (`retirement.rs:132`), `READS_PER_PASS` 54 → 70 (`:135`). Rejected as
well: reading pages in a loop inside one step until `take + 1` rows — under a cap of 1 that is
about 500 reads in one step, not a fixed per-step bound.

### 2. A pass certified work it had not seen (ledger wrap and retry exhaustion)

The rule I chose, one rule for both cases: **`Satisfied` only when the pass read the whole ledger,
from its head to its end, and found no `retire:records:` obligation in it.**
- `Pass::owed` (`retirement.rs:200`) is set whenever a visit decodes a records obligation
  (`:329`), before anything else happens to it. A pass whose steps were all spent on lost
  preconditions (`:330-335`) therefore never certifies — that is the T4 blocking finding at the
  old `:300`.
- `from_head` (`:239`) and `reached_end` give `saw_whole_ledger` (`:293`). A pass that resumed
  part-way and reached the end did not see the obligations before its start this pass, so it
  answers `Partial` and the walk returns to the head (`:294-302`).
- Order is unchanged otherwise: `Blocked` > `Changed` > `Partial` > `Satisfied`.
- Module doc section *What a pass certifies* (`:65`), `reconcile`'s doc (`:208-214`), and two
  sentences in `docs/design/architecture/06-runtime-view.md:88` (the short-page rule for `{seg}`
  and what a pass certifies).

The other option the sign-off offered, "Partial whenever a step this pass left an advanced
cursor", does not fix the adversary's probe: the pass that wrongly certifies there made no step at
all. A lap-carried "clean since the head" flag in the resume record was the third option I
considered and rejected: it changes the resume record's encoding and its damaged-value handling
(roughly 30–40 lines plus tests), and it would certify on evidence from earlier passes — an
obligation installed behind the walk's position mid-lap would be certified away.

Cost of the chosen rule, stated in the module doc: a ledger longer than one pass's visits (64)
never certifies in a single pass. While #1062 leaves `retire:bytes:` obligations in the ledger,
a ledger of more than 64 of them answers `Partial` every pass. The deployed loop ignores the
outcome (`crates/server/src/custodian.rs:604-617`), and no in-repo caller drives this leg until
`Satisfied` over such a ledger (the full suite is green).

`Reconciled::least_certified` went back to private (`reconciliation.rs:76`): the leg no longer
uses it, so the visibility change from iteration 1 is gone from the patch.

### 3. Tests that compared the code with itself

- Every hold reason is asserted as a literal string (`"undecodable"`, `"value-past-ceiling"`,
  `"segment-cursor-past-records"`, `"commit-failed"`, `"malformed-segment-key"`); `Hold` is no
  longer imported by the test.
- Every per-pass bound is a literal in the test (`VISIT_BOUND` 64, `LEDGER_PAGE_BOUND` 5,
  `STEP_BOUND` 16, `READ_BOUND` 70, `COMMIT_BOUND` 17, test file `:106-112`), and one test pins
  the built constants to them (`the_built_bounds_are_the_promised_ones`, `:1085`). That kills the
  constant-arithmetic survivors at the old `:109` and `:121`.
- The page cap and the walk's stop are now pinned by behaviour, not just by count:
  - `l1_a_productive_backlog_drains_in_key_order_within_fixed_per_pass_bounds` (`:1172`): eight
    `retire:bytes:` obligations ahead of 192 one-step records obligations, so the second ledger page
    a pass reads holds more records obligations than the pass has steps left. After pass `k`
    exactly the first `16k` are gone (no obligation passed by — the old `:233` survivor), each pass
    reads exactly the ledger pages its steps need, and the end of the backlog gives `Partial` then
    `Satisfied`.
  - `l1_a_seg_backlog_reads_within_fixed_per_pass_bounds` (`:1222`): 192 `{seg}` obligations, half
    with a cursor; each pass's `seg:` reads equal exactly what its steps need (1 or 2 each).
  - `l1_short_ledger_pages_make_the_page_bound_end_a_pass` (`:1270`): a store capped at 3 rows, so
    the page bound (5) ends each pass at 15 obligations (the old `:222` survivor).

## New legs for the defects

- `c_a_seg_obligation_drains_to_empty_on_a_store_that_cuts_pages_short` (`:988`) — the (C)-style
  leg the sign-off asked for, on `RedbMetadataStore::in_memory().with_scan_cap(50)`: 200 records,
  four steps of 50, the obligation present with its cursor at 50/100/150 after each of the first
  three, deleted with the last record by the fourth, whose full page is followed by nothing.
  Exact `seg:` reads per step `[2, 2, 2, 3]`.
- `s_a_pass_that_resumed_part_way_does_not_certify_the_ledger` (`:1372`) — the adversary's probe
  B shape: 63 bytes obligations, then a records obligation needing three steps. Outcomes are
  `[Changed, Partial, Changed, Partial, Changed, Partial, Satisfied]`, and no pass answers
  `Satisfied` while the obligation is stored.
- `s_a_pass_whose_every_step_lost_its_precondition_does_not_certify` (`:1412`) — a second writer
  moves the obligation before every commit conditioned on it (`Contender`, test `:238`); 16 lost
  steps, `Partial`, nothing landed, within the bounds; with the writer gone the next pass drains
  from the writer's cursor.
- (W) gains an obligation installed without the reservation whose advanced value is exactly
  `MAX_VALUE_BYTES` (99,954 bytes bare + the 46-byte cursor): it drains, and its 100,000-byte put is
  sent and lands (the old `:388` `>`→`>=` survivor).
- The `{seg}` (C) leg now asserts exact `seg:` reads `[1, 1, 2]`; `drain()` checks every pass
  against the literal bounds.

The test file is now 19 tests (was 13). Fixture changes: `Store::capped` (`:277`), `Store::under`
reads page by page so a capped store answers (`:321`), the log counts ledger pages and `seg:` reads.

## Demonstrated reds (each a temporary edit; the file restored and compared byte-for-byte after)

Run with `cargo test -p wyrd-custodian --test multipart_retirement_drain -- <filter>` under
`timeout 1200`, through a helper that swaps in the mutated file, runs, restores and `cmp`s. All
files restored (`cmp` clean after every run).

**The previous attempt's drain module, verbatim** (`iteration-v1`'s `retirement.rs`, with
`least_certified` re-exposed so it compiles): 5 of 19 fail —
`c_…_cuts_pages_short` (1 step instead of 4: the obligation deleted on the first short page),
`s_…_lost_its_precondition…` (`Satisfied`, expected `Partial`), `s_…_resumed_part_way…`
("certified with work left"), `l1_a_productive_backlog…` (`Satisfied` after resuming at the end),
and the bounds pin (`READS_PER_STEP` 3 vs 4).

The brief's list:

| # | Temporary edit | Leg | Result |
|---|---|---|---|
| 1 | dispatch inside `if black_box(false)` (`reconciliation.rs:166-170`) | B-rec, C | all 4 C legs, B-rec ("the records obligation drains") and G-rec FAILED |
| 2 | `plan` ignores the stored cursor | C | all 4 C legs FAILED — sparse `{parts}` at "the stored value changes every step"; both `{seg}` legs on their exact read counts (no whole-range read is ever made); combined |
| 3 | obligation deleted after its first batch | C, G-rec | all 4 C legs FAILED; G-rec at the `reconcile_after_restore` assertion |
| 4 | `{parts}` finishing does not move to `seg` | C combined | FAILED ("the obligation outlives its parts"); others pass |
| 5 | `{seg}` finish without the whole-range read | K `{seg}` | FAILED (`Changed`, expected `Blocked`) |
| 6 | `steps_left -= 1` → `+= 1` | L1 | `{parts}` and `{seg}` L1 legs FAILED on the step bound |
| 7 | an undecodable visit gives its visit back | L2 | FAILED on the visit bound |
| 8 | walk always starts at the head | L2 | FAILED: the productive obligation never drains |
| 9 | ceiling check removed | W | FAILED: an over-ceiling put is sent |

New for this iteration:

| # | Temporary edit | Leg | Result |
|---|---|---|---|
| 10 | `seg_step` ignores `next` (the iteration-1 shape) | C short pages | FAILED: 1 step, obligation gone with 150 records left |
| 11 | a resumed pass that reaches the end counts as having seen the whole ledger | S resumed, L1 | both FAILED |
| 12 | `pass.owed = true` removed | S contended | FAILED (`Satisfied`) |
| 13 | the walk's "steps spent" stop removed (visits obligations it has no step for) | L1 `{parts}` | FAILED: pass 2 drained the wrong 16 |
| 14 | `pages_left -= 1` → `+= 1` | L1 short pages | FAILED on the page bound |
| 15 | whole-range read on every finish (`entry > 0` → `>= 0`) | L1 `{seg}` | FAILED on the exact read count |
| 16 | ceiling check `>` → `>=` | W | FAILED: the exactly-at-ceiling obligation is held instead of drained |
| 17 | `Hold::Undecodable` spelled `"xyzzy"` | L2 | FAILED on the literal hold name |
| 18 | the walk's loop condition `steps_left > 0` → `>= 0` (the old `:221` survivor) | L1 `{parts}` | FAILED: a ledger page read with no step left for it (2 pages, 1 expected) |

## Refuting my own test

- **(a) Genuine red?** Yes. The previous attempt's module fails 5 of the 19 tests, one per
  carry-forward defect plus the bound pin, and each of the 18 temporary edits above fails its leg.
  On the bare base the custodian test file cannot compile (no `retirement` module, no redb
  dev-dependency), so C4-verify will report UNVERIFIABLE (exit 77); the brief records the human's
  advance acceptance of that (2026-11-11).
- **(b) Production path?** Yes. Every leg drives the drain only through
  `wyrd_custodian::reconcile_step` with a fresh reconstruction context and every other input
  `None`, the shape `reconcile_pass` uses (`crates/server/src/custodian.rs:397`). B-rec's
  obligations come from the production `create_session` + `complete_upload`; G-rec's from the
  production restore fence, re-checked by the production `reconcile_after_restore` after every
  step. The store is the real `RedbMetadataStore`; the short-page legs use its own
  `with_scan_cap`, not a stand-in pager.
- **(c) Fixture includes the fault?** Yes. The short-page legs run on a store that really cuts
  pages short and sets `next`; the contended leg has a real second writer moving the obligation
  before each commit, so every precondition really fails; the resumed leg really leaves a
  half-drained obligation before the resume point; the W wrapper really refuses over-ceiling puts.

## Survivors from the previous C5 run, and where they stand

- Killed by the bound pin: `:109` (`LEDGER_PAGES_PER_PASS` arithmetic), `:121` (`READS_PER_PASS`).
- Killed by behaviour: `:165` (literal hold names, red 17), `:222` (red 14), `:233` (red 13),
  `:388` (red 16), `:513` (the code is restructured; its `entry > 0` is red 15), `:501` (the guard
  is gone; see below).
- `:221` `steps_left > 0` → `>= 0`: one extra ledger page read after the steps run out. The L1
  `{parts}` leg now asserts the exact ledger pages per pass; run as red 18 below, it fails
  (2 pages read, 1 expected).
- **Expected to survive, equivalent in practice:** the batch-byte arithmetic (old `:414`, `:423`,
  `:444`). `MAX_BATCH_BYTES` is 5 MB and the operation budget (500) binds first for any key the
  grammar spells (part, psum and seg keys are under 100 bytes; the byte budget would bind only for
  keys near 9.6 KB). The byte side exists because the batch rule is `B = min(ops, bytes)`
  (`0016:622-648`). `segment_index`'s exact-spelling compare (`retirement.rs:592`) is also
  equivalent while it is called only on keys under the group's prefix: the prefix fixes nonce and
  epoch and `parse_seg_key` fixes the index's spelling. I kept it so the helper stays correct if
  reused. `emit_step` (metrics only) is not asserted.

## Choices carried over from iteration 1 (unchanged, still the reasons)

- `retire:bytes:` visits count against the visit bound and do not withhold `Satisfied`: this leg
  certifies the records-mode ledger only; they are counted on
  `retirement_bytes_obligations_deferred`.
- Commit errors are held (`commit-failed`), not propagated: propagating would pin the walk on one
  obligation the store refuses for good.
- One step per obligation per visit; one component per step (component boundaries are commit
  boundaries, which the combined leg needs).
- The walk's resume record is outside `retire:` (`custodian:retire-cursor`), GC's pattern; the
  obligation's own cursor is inside the obligation (`0016:612-619`).
- No DST row (brief: child-2's); `// deferred: #1062` and `// deferred: #660` above `reconcile`
  (`retirement.rs:203-207`).

## Gates run

- `./engine/xtask.sh ci` (the project's gate, `cargo xtask ci`, on this worktree):
  **`xtask ci: all checks passed`, exit 0** — typos (ran, clean), `lint_docs` OK, `render_site
  --check` (101 pages, link audit OK), gitlink/unsafe/blackbox guards, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`, `cargo test --workspace` (the drain file 19/19, the
  core cursor and Complete-ceiling files, every other custodian test), cargo-deny (incl. the
  all-features advisories run — passed, so no NEEDS-HUMAN for it), conformance, statics
  (ADR-0035), deploy-guard and the madsim DST sweep.
- One edit landed while that run was in progress: the exact ledger-pages assertion in the L1
  `{parts}` leg (five test lines, no production change). After the run I re-ran, on the final tree:
  `cargo fmt --all -- --check` (clean), `cargo clippy -p wyrd-custodian --all-targets -- -D
  warnings` with both drain files touched so they were re-checked (clean), `cargo clippy -p
  wyrd-core --all-targets` (clean), `typos` on the touched drain files and the runtime-view doc
  (clean), `git diff --check` (clean), and the drain file (19/19).
- `patch.diff` applies to `c27cdb0` (`git apply --cached --check` against a fresh index of that
  commit). Size 214 KB (iteration 1: 190 KB); the growth is the new test legs and fixtures, plus
  `retirement.rs` growing from 568 to 634 lines (110 lines added, 44 removed against iteration 1).
- The test file is also copied to the bundle as `multipart_retirement_drain.rs`.

NEEDS-HUMAN: none from Do. C4-verify's UNVERIFIABLE (exit 77) is the pre-accepted sign-off item
from the brief, not a new one.
