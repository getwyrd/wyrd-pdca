# Build notes — issue 813 (staged-scrub-and-keep), iteration 3

Iteration 3 is **iteration 2 plus the two fixes the human named at sign-off**
(`iteration-v2/session-carry-forward`, `brief.md:164`), not a rebuild. I applied
`iteration-v2/patch.diff` to the worktree (it applies cleanly to `4ab2b28`) and changed only what
those two items need, plus the docs they make false. The size backstop was overridden again
("stay as one slice, do not split"), so I kept additions to what the two items need.

Base for every `path:line` below: the worktree at `4ab2b28` (`origin/main`; `97fc2f9`, the
brief's citation commit, is an ancestor). Line numbers are in the patched tree.

## Fix 1 — `ReconstructionContext::clock` and the pass's `now_millis` come from one source

**The defect (adversary, iteration 2):** the deployed loop filled `ReconstructionContext::clock`
with a fresh `wyrd_testkit::SystemClock` while each pass got `now_millis` from the caller's own
`clock` closure. The day-one tests drive that loop with `|| 500`, so the pass read 500 and the
seam read about 1.79e12. Nothing reads the seam yet, but #814 will stamp a pre-mark from the
pass's `now_millis` and check the write window through the seam: a logical clock and the wall
clock in one lifecycle, the #557/#565 class the rubric's first MUST forbids.

**The fix:** `crates/server/src/custodian.rs:134-169` adds `LoopClock<F>`, a private wrapper
that owns the caller's closure (behind a `std::sync::Mutex`, so it is `Sync` for the
`&(dyn Clock + Sync)` field) and implements `wyrd_testkit::Clock` by calling it.
`run_reconstruction_until` moves the closure into one `LoopClock` (`:542`); every pass reads
`clock.now_millis()` (scrub `:582-590`, reconstruction `:604-612`, GC `:689-697`), and the
context gets `clock: &clock` (`:570`). The closure is **moved**, so the loop cannot read time
any other way: a stray `clock()` no longer compiles.

The generic bound gains `+ Send` (`custodian.rs:535`, `cli.rs:1470`) because `Mutex<F>: Sync`
needs `F: Send`. Every caller's closure is `Send`: the deployed `wall_clock_millis` fn item
(`cli.rs:1046`) and the tests' `u64`-capturing counters and constants.

Poisoning follows the crate's existing convention, `unwrap_or_else(PoisonError::into_inner)`
(`crates/server/src/dserver.rs:291-292`).

**Why a live wrapper, not the adversary's suggestion (one `ManualClock`, `set(clock())` once per
pass).** Both give one source. The suggestion freezes the seam for the whole pass: every seam
read returns the pass's starting instant. The seam's only planned reader is #814's write-window
check, and a frozen clock can never show the window expiring mid-pass, however long the write
took. That is the "guard that only exists on entry" defect `SteppedClock`'s own doc describes
(`crates/testkit/src/lib.rs:80-93`, #638). The wrapper reads the same closure on every call, so in
production the seam moves with the wall clock and in tests it moves with the test's own clock.

Cost, measured: the suggestion is about 4 changed lines in the loop (bind a `ManualClock`, then
`let now = clock(); seam.set(now);` before the reconstruction pass). The wrapper costs
`crates/server/src/custodian.rs` +115/-6 against iteration 2's +17/-0: 37 lines of struct, impls
and doc, 26 lines of unit test, 24 lines from rustfmt breaking the three `reconcile_pass(...,
clock.now_millis())` calls over 100 columns, and doc/comment lines. I judged the frozen-clock
hazard for #814 worth that; the axis is the reader's correctness, not the line count.

**Test sites.** The adversary also named the test and DST sites that pair a `SystemClock` with a
fixed `now`. All 44 `ReconstructionContext` constructions outside production now back `clock`
with a `wyrd_testkit::ManualClock` reading the same instant the pass is handed:

- 37 contexts used for one pass: `&wyrd_testkit::ManualClock::new(N)` where `N` is that pass's
  `now_millis` (`500`, `600`, `200`, `1_000`, `0`, `NOW`, `now`, `HANDOFF_NOW`, whichever the site
  passes). These lines were already in iteration 2's patch as `&wyrd_testkit::SystemClock`, so
  this changes their content, not the patch size.
- 7 contexts reused across two instants (`crates/dst/tests/custodian.rs:701/759`, `:1029/1080`,
  `:1106/1143`, `:1215/1262`; `crates/server/tests/custodian_day_one.rs:619/654`, `:743/782`,
  `:998/1038`): one `let pass_clock = wyrd_testkit::ManualClock::new(500);` and
  `pass_clock.set(600);` right before the 600 pass.

I classified every site by script (the context's binding name, then every `Some(&<name>)` use and
the `now` it passes) rather than by eye; no site's clock disagrees with its pass.

`ReconstructionContext::clock`'s doc now states the rule for anyone building one
(`crates/custodian/src/reconstruction.rs:116-123`).

**Test:** `crates/server/src/custodian.rs:732-761`,
`the_pass_clock_and_the_seam_read_one_source` — a pass read, a read through `&(dyn Clock + Sync)`
and another pass read take 501, 502, 503 in turn from one counter closure. Red: I changed the
trait impl to read `wyrd_testkit::SystemClock` (iteration 2's wiring, in effect) — FAILED,
`left: 1789975588006, right: 502`. Restored, green. A per-pass snapshot fails the same assertion
(it would return 501).

**What this test cannot show, stated plainly.** Nothing reads `ctx.clock` in this slice, so no
behavioural test can observe the loop's wiring end to end. The unit test pins the wrapper's
contract; that the loop routes both reads through it is structural (the closure is moved into
the wrapper). #814's own tests will be the first behavioural reader.

## Fix 2 — the post-publish scrub/reconstruction flip-flop is prevented

**The defect (adversary, iteration 2):** after publication, the `part:` record stays until its
retirement drain (no retirement drain exists in `crates/custodian/src` yet, so that window has no
bound in the tree). If reconstruction then moves a lost fragment of the published chunk, it
repoints the committed inode but not the part record. Scrub checked both placements, so every
pass it found the part record's old position empty and enqueued the chunk; every reconstruction
pass found the committed chunk whole and drained it. Scrub never again answered `Satisfied`.

**Prevent, not bound.** A bound would need either the retirement drain (not in the tree) or a
counter or clock in scrub (new state for a loop that has none). Preventing it needed one rule.

**The rule** (`crates/custodian/src/scrub.rs:206-226`): a part record's placement is checked only
for a chunk **no committed chunk map names**. If the committed reading names the chunk, validly
placed (`referenced.schemes`) or malformed (`referenced.malformed`), scrub checks it where the
committed map places it, or reports the malformed placement, and skips the part record's
placement. This is the precedence reconstruction already applies: `assess` consults the staged
reading only when the committed reading has no site for the chunk
(`crates/custodian/src/reconstruction.rs:734`, the `None if staged.contains(&chunk)` arm). The two
loops now agree on which record is authoritative, so they cannot undo each other.

Why a malformed committed placement also supersedes: the committed record is the one reads use
once it exists, and reconstruction treats a malformed committed chunk as committed (NEEDS-HUMAN,
left queued). Scrub still names the chunk on its audit seam as malformed, so this is not a silent
skip.

Why the read order still protects the publication race: the staged read comes first and the
committed read second (`scrub.rs:130`, `:159`). A chunk a publication moves mid-pass is named by
at least one of the two reads, and it is checked at the committed placement if the second read
saw it, at the part placement otherwise. At publication the inode copies the part record's
placement, so either one covers the same fragments. The C' publication-race legs stay green.

**Scope note for the human.** The brief says scrub checks "every fragment a committed `part:`
record places" (`brief.md:95`). This narrows it to "every fragment a committed `part:` record
places for a chunk no committed map names". The sign-off asked to "prevent or explicitly bound"
the flip-flop, which I read as authorising this. The brief's invariant still holds where it
matters: once a committed map names the chunk, that map is where the bytes are read from, and it
is checked. Module doc `scrub.rs:31-36`, the missing-fragment comment `:261-273`, reconstruction's
module doc (`reconstruction.rs:50-59`) and `docs/design/architecture/06-runtime-view.md:82` say so.

**Tests:**

- `staged_scrub.rs` leg **A'** (`:702-781`), two tests over one harness: a published upload whose
  part record places the chunk on server 3 while the committed inode places it on server 0.
  - `a_part_placement_a_committed_map_supersedes_is_not_checked`: server 3 empty, server 0 intact.
    Nothing queued, `Satisfied`.
  - `a_superseded_part_is_checked_where_the_committed_map_places_it` (control): server 0 empty,
    server 3 still holding an intact copy. The chunk is queued, `Changed`. This proves the rule
    moves the check to the committed placement rather than skipping the chunk, and that bytes left
    at the old position do not hide a loss.
- `staged_protection.rs` leg **J** (`:2647-2752`,
  `scrub_and_reconstruction_settle_after_a_published_chunk_is_moved`): the adversary's scenario,
  production end to end. A published RS(2,1) chunk, inode and part record both on `[3, 1, 2]`,
  real encoded fragments (`wyrd_core::erasure::encode` + `wyrd_core::write::encode_ec_fragment`)
  on servers 1 and 2, fragment 0 lost from server 3, which stays up. Round 1: scrub enqueues;
  **reconstruction itself** rebuilds fragment 0 and moves it to server 0 (the selector takes free
  domain A before D, `crates/core/src/placement.rs:289-294`); the leg asserts the inode now says
  `[0, 1, 2]`, the fragment is on 0 and not on 3, and the part record is byte-identical. Rounds 2
  and 3: scrub `Satisfied` with no `repair:` key, reconstruction `Satisfied`. To give
  reconstruction a free domain I added `four_domains()` (`:506-517`) and switched the shared
  `reconstruction_pass` helper to it. Legs G-I never reach placement, and all stay green.

Red, reverting **only** the rule (the loop back to iteration 2's
`fragments.entry(key).or_insert(scheme)`), `cargo test --no-fail-fast -p wyrd-custodian --test
staged_scrub --test staged_protection`:

```
staged_protection: 32 passed; 1 failed — scrub_and_reconstruction_settle_after_a_published_chunk_is_moved
  panicked at staged_protection.rs:2733: round 2: scrub checked the published chunk at the empty
  position ... left: Answers { scrub: Changed, repairs: [repair:5953] }
staged_scrub:      12 passed; 1 failed — a_part_placement_a_committed_map_supersedes_is_not_checked
  panicked at staged_scrub.rs:748
```

Restored: 33/33 and 13/13. Only the two legs written for this fix went red.

Both A' legs and leg J are **green on base** `4ab2b28`: base scrub reads no part record, so it
never checks a stale one. They guard against a scrub that reads part records without the
precedence rule, which is iteration 2's code. The file's own falsifiability paragraph says so
(`staged_scrub.rs:50-57`).

## C4-verify's red, run by hand (the brief's Verification posture)

Base production (`crates/custodian/src/{gc,reconstruction,scrub}.rs` and
`crates/custodian/Cargo.toml` from `4ab2b28`), the new `staged_scrub.rs` kept, and
`staged_protection.rs` from this patch with **only** the two new field initialisers removed from
`reconstruction_pass` (the brief's prescribed method, `brief.md:64-72`):

```
cargo test --no-fail-fast -p wyrd-custodian --test staged_scrub --test staged_protection
  staged_scrub:      4 passed; 9 failed
  staged_protection: 27 passed; 6 failed
```

`staged_scrub.rs` failures (all by assertion): A ×3 (`:625`, `:645`, `:668`), C ×2 (`:850`,
`:884`), C' ×2 (`:1009`), C'' (`:1097`), C''' (`:1145`). Passing: B (the over-reach guard,
`brief.md:61-62`), A's intact control, and both A' legs (green on base by construction, above).

`staged_protection.rs` failures (all by assertion): G ×2 (`:2366`, `:2417`, brief's leg D),
H (`:2521`, brief's leg E), I ×2 (`:2576`, `:2637`, brief's leg F and iteration 2's
fault-attribution leg), and the rewritten leg F (`:2282`). Passing: every pre-existing leg A-E
unedited, G's drain control, and leg J.

Then I restored every file from a saved copy and confirmed `git diff` was byte-identical to the
diff before the revert.

## Gates and commands

All through the project's own runner (`engine/xtask.sh`, which runs `cargo xtask` in
`$PDCA_WORKTREE`), except targeted `cargo test` runs under `timeout` for the red/green loops:

```
PDCA_WORKTREE=… ./engine/xtask.sh ci    → "xtask ci: all checks passed", exit 0
  (fmt, clippy, build, workspace tests, madsim clippy + tests, deny, conformance, statics)
PDCA_WORKTREE=… ./engine/xtask.sh dst   → 78 passed, 0 failed (incl. both
  reconstruction_staged_handoffs_* and both gc_staged_handoffs_* legs)
cargo test -p wyrd-custodian --test staged_scrub       → 13 passed
cargo test -p wyrd-custodian --test staged_protection  → 33 passed
cargo test -p wyrd-server --lib custodian::tests       → 1 passed
cargo fmt --all -- --check                             → clean
./engine/scripts/run-verify.sh --classify patch.diff
  → ADDED_TEST crates/custodian/tests/staged_scrub.rs
    CRATE crates/{chunkstore-grpc,custodian,dst,server}
PDCA_BUNDLE=… ./engine/scripts/run-diff-cov.sh         → PASS 94.3% (183 of 194
  instrumentable changed lines; floor 80%; 439 tests ran). Iteration 2: 92.0% (127/138).
```

The 11 coverage misses are the same lines iteration 2 reported (shifted by the new doc lines):
`gc.rs:1425-1428` and `:1613`, `reconstruction.rs:226-227`, `:1236-1238` and `:1245`. The gate
scores `wyrd-custodian` under `--test staged_scrub` only. Ten of the eleven are reached by tests
it does not run for that crate: `gc.rs:1425-1428` by `staged_protection.rs` leg E(ii), and the
`reconstruction.rs` lines by legs G and I and the DST campaign. `gc.rs:1613`, the session-listing
page cursor in scrub's own part reader, is reached by no test: no fixture lists more than one page
of sessions for scrub. It is the same loop shape `staged_fragments` pages with, which
`staged_protection.rs`'s paging legs do cover. Iteration 2 dropped a scrub paging leg to save
size (`iteration-v2/build-notes.md:127-139`) and I left it out too.

After the CI run I changed one doc comment on the unit test (`custodian.rs:736-740`); `cargo fmt
--all -- --check` stayed clean and the test was re-run green.

I did not re-run the DST **red** (production reverted, madsim sweep): reconstruction's
production code is unchanged since iteration 2, which proved that red (`iteration-v2/
build-notes.md:112-118`). The DST file changed only in which clock its contexts carry.

## Size

`patch.diff` is 200 KB (iteration 2: 176 KB). Per file, added/removed lines against iteration 2
(`git apply --numstat`):

| file | v2 | v3 |
| --- | --- | --- |
| `crates/server/src/custodian.rs` | +17/-0 | +115/-6 |
| `crates/server/src/cli.rs` | — | +1/-1 |
| `crates/server/tests/custodian_day_one.rs` | +14/-0 | +20/-0 |
| `crates/dst/tests/custodian.rs` | +211/-64 | +219/-64 |
| `crates/custodian/src/scrub.rs` | +148/-52 | +180/-57 |
| `crates/custodian/src/reconstruction.rs` | +172/-13 | +179/-13 |
| `crates/custodian/tests/staged_scrub.rs` | +1076 | +1163 |
| `crates/custodian/tests/staged_protection.rs` | +459/-33 | +583/-33 |

Every other file is unchanged in size. The growth is the two fixes and their tests. Leg J is the
largest single addition (about 110 lines); it is the only test that shows the loops settling with
reconstruction doing the move itself, which is exactly what the human asked to prevent.

## Items still for the human

- From iteration 1's carry-forward, still open in iteration 2's §6: confirm the prior-art claim
  (`brief.md:144`). I did not re-run that search.
- The fix-2 scope note above: scrub no longer checks a part record's placement once a committed
  map names the chunk.
- `+ Send` on `run_reconstruction_until` / `run_reconstruction_over_backend`: every caller in the
  workspace compiles, but it is a tighter public bound.

## Self-refutation (the three forced questions)

**(a) Genuine red?** Yes, each by reverting and re-running, not by argument:

| what was reverted | what went red |
| --- | --- |
| whole production change (C4-verify posture; initialisers stripped per brief) | 9 of 13 `staged_scrub.rs` legs, 6 of 33 `staged_protection.rs` legs |
| only the scrub precedence rule (fix 2) | exactly 2 legs: A' primary and J |
| only the seam's source (fix 1: trait impl reads `SystemClock`) | the one `LoopClock` unit test |

Legs that stay green on base (B, A's control, both A' legs, J) are guards against over-reach, and
the table above shows A' and J do go red on the code they guard against.

**(b) Production path?** Yes. Every custodian leg runs `wyrd_custodian::reconcile_step` with a
real `ScrubContext` or `ReconstructionContext`. The doubles (`Meta`, `Disk`) implement only the
production `MetadataStore` / `ChunkStore` traits. Leg J's re-place is reconstruction's own
production repair (`erasure::reconstruct`, `select_distinct_domains_excluding`, the
version-conditional repoint), not a seeded result; the test only asserts what it produced. The
`LoopClock` test drives the production type the loop uses, not a copy. Every seeded record is
round-tripped through the base decoders.

**(c) Fixture includes the fault?** Yes. A' seeds the stale part record **and** the empty old
position, and its control seeds the loss at the committed position with intact bytes left at the
stale one, so "skip the chunk" and "check the stale position" each fail one of the two legs. Leg J
keeps server 3 **in both fleets** (up, reachable, missing the fragment): with server 3 dropped from
scrub's fleet, the old position would never be fetched and the leg would pass for the wrong
reason. It also asserts the move really happened (`[0, 1, 2]`, fragment on 0 and not on 3, part
record byte-identical) before the settling rounds, so the rounds cannot pass over a fixture where
the placements never diverged. The clock test's counter closure moves on every read, so a second
source or a cached reading cannot match its sequence by accident.

## Scratch

Working copies for the revert/restore cycles and the gate logs lived in
`$PDCA_SCRATCH/pdca-builder-813-redleg` and are removed; the diff-coverage gate's own output is in
this bundle's `coverage/`. I also created one stray 764-byte file, `/tmp/.x`, by a
mistaken redirect, and removed it at once. Every source edit was made in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt`). The new test file is marked intent-to-add in that worktree's
index so `git diff` includes it. No branch was pushed and no PR was opened, readied or merged.
