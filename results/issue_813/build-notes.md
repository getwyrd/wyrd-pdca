# Build notes — issue 813 (staged-scrub-and-keep), round 5

Round 5 is **round 4's patch plus two tests**. The carry-forward block in `brief.md` asked for
exactly that: close the two test-gap findings from round 4's adversary review, change no
production code, do not re-split. I applied `iteration-v4/patch.diff` to the worktree at
`4ab2b28` (`git apply --check` first; it applied cleanly) and edited one file.

| file | change vs round 4 |
| --- | --- |
| `crates/custodian/tests/staged_scrub.rs` | +140 lines: two tests, a page log on the `Meta` double, module-doc entries |

`diff <(git apply --numstat iteration-v4/patch.diff) <(git diff --numstat)` shows that one line
and nothing else: all 18 other files are byte-for-byte round 4's. `patch.diff` is 224,956 bytes
against round 4's 218,237 (+6.7 KB, all of it test code). Every `path:line` below is in the
patched tree unless it says "base".

Everything round 4 built and recorded still stands; `iteration-v4/build-notes.md` holds the
notes for it (C'', C''-audit, A'-malformed, G-held, empty queue, J-discharge, review finding (c),
the prose fixes, and the by-hand red for G / G-held / H / I). I did not re-run the round-4
mutations or the `staged_protection.rs` by-hand red: no production file and no
`staged_protection.rs` line changed this round, so those results are about the same bytes.
`review-rejected.md` is unchanged too — its `scrub.rs:255` is still the fleet walk's line.

## The two tests

### (1) A-paged — sessions past the first page of the listing

`staged_scrub.rs:1004` `scrub_checks_committed_parts_of_sessions_past_the_first_page`.

What it pins: scrub's staged reader, `staged_committed_parts`, has its own session loop
(`gc.rs:1599-1619`). It lists `mpu:` sessions in pages of `STAGED_PAGE` = 512 (`gc.rs:300`) and
must carry on past a full page (`gc.rs:1615-1618`). GC's paged tests walk a different loop
(`staged_fragments`, `gc.rs:1501`), so nothing covered this one. `walk_staged_range`
(`gc.rs:1666`), which reads each session's `part:` range, *is* shared with GC and already covered.

Fixture: 1025 `Open` sessions (two full pages and one more), ids `format!("{n:032x}")` so the
`mpu:` keys sort in `n` order. Three sessions have a committed part placed on server 1, where no
fragment is: session 0 (page 1, a control that the pass checks anything at all), session 512 (the
first of page 2 — a walk that skipped one key at the page boundary would miss it), session 1024
(alone on page 3 — the short last page). Assertions, in this order:

1. all three chunks are in the repair queue (`:1026-1033`);
2. the pass read `1025.div_ceil(512)` = 3 pages under `mpu:` (`:1035-1041`);
3. the pass answered `Changed`.

Why assertion 2 exists: `STAGED_PAGE` is private to `gc.rs`, so the test restates 512
(`:990`). If production's page size later grows past 1025, every session fits one page, all
three chunks are still queued, and the test would go on passing while proving nothing about
paging. The page count turns that silent loss into a red test whose message says to resize the
fixture. For it I added a small log to the `Meta` double: `pages: Mutex<Vec<Vec<u8>>>`, the
prefix of each `scan_page` call (the field `:125-126`, `pages_read` `:224-232`, the push at
`:284`). I first had assertion 2 ahead of assertion 1; under the mutation below it fired first and blamed the
fixture, which is the wrong message for a real paging bug, so I swapped them.

I chose 1025 real sessions over a lowered page cap in the double. A lowered cap would exercise
the "store returned a short page with a cursor" path with a handful of records, but the finding
is about production's own 512-record page, and filling it needs no test-side knob. Cost: the
test seeds 1028 records and the pass makes 1025 + 3 paged reads of an in-memory `BTreeMap`; the
whole file's 18 tests finish in 0.02 s.

I did not add an "exactly 1024 sessions" case (a full last page followed by an empty one). The
empty-page handling is in `checked_page` (`gc.rs:2017-2048`), shared with GC and covered there;
the session loop's own match has no separate arm for it.

### (2) C-order — staged damage is named before a later committed-read fault

`staged_scrub.rs:936` `staged_damage_is_named_even_when_the_committed_read_then_faults`.

What it pins: `scrub.rs:144-163` emits both kinds of staged damage — a malformed `part:`
placement (`:152-156`) and an unreadable `part:` record (`:161-163`) — before
`referenced_fragments(ctx.meta).await?` at `:174`. The comments there promise that the committed
read's `?` cannot cost the operator the names. Reconstruction has this test
(`staged_protection.rs:2715`); scrub did not.

Fixture: one `Open` session with part 1 placed on `[]` (malformed) and part 2 holding
`{"chunks":"not a chunk list"}` (will not decode), then `meta.fail_reads_of(b"inode:")`.
Assertions: the pass returns `Err`, a `ReconcileError::Store` wrapping the injected fault;
one audit event carries part 1's key together with `malformed-staged-placement`; part 2's key
is named on the scrub audit seam. One record per emit loop, so each loop's position is pinned on
its own (mutations 2 and 2b below).

## Red → green

Quick runs used `timeout 1500 cargo test -p wyrd-custodian --test staged_scrub` (the form round
3 and 4 used for the by-hand red, wrapped in `timeout` so a hang cannot stall the beat). The
full gate ran through the project's wrapper, below.

### Green, full patch

```
cargo test -p wyrd-custodian --test staged_scrub
  18 passed; 0 failed
```

After the red runs below I edited only doc comments in the test file (the module-doc
falsifiability note). The green on the file's final bytes is the gate run under "Gates", where
both new tests show as `ok`.

### Red, production reverted to base

`crates/custodian/src/{gc,reconstruction,scrub,reconciliation}.rs` and
`crates/custodian/Cargo.toml` replaced by `git show HEAD:<file>` (HEAD = `4ab2b28`), the new
`staged_scrub.rs` kept:

```
cargo test --no-fail-fast -p wyrd-custodian --test staged_scrub
  5 passed; 13 failed
```

The 13 are round 4's 11 plus the two new tests. Both new tests fail by assertion, not by a
compile error or a panic elsewhere:

- A-paged at the queue assertion (`:1027`; `:1026` when it ran, before a one-line doc reflow):
  "scrub never checked the committed part of session 0 of 1025 …" — base scrub reads no `part:`
  record.
- C-order at the malformed-record assertion (`:969`; `:968` when it ran): "the malformed part
  record part:c3c3…:000001 was found and then lost …" — base fails at the same `inode:` read (so
  `expect_err` holds) and names nothing.

The 5 that pass are the same guards as round 4: B, A's intact control, both A' legs,
A'-malformed. I then restored the five files from saved copies; `cmp` matched each one, and
`git diff` was byte-identical to the diff taken before the revert.

### One mutation per new test (full patch, each restored and `cmp`'d after)

| test | mutation | result |
| --- | --- | --- |
| A-paged | `gc.rs:1616` `(Some(_), Some((last, _))) => after = Some(last)` gets the guard `if last.is_empty()`, so the session loop returns after its first page | 1 red, A-paged only: "session 512 of 1025, which is on page 2 … was not queued" |
| C-order | `scrub.rs:152-163`, both emit loops moved below `referenced_fragments(…)?` (`:174`) | 1 red, C-order only, at the malformed-record assertion |
| C-order (2b) | only the `staged.unresolvable` loop (`scrub.rs:161-163`) moved below `:174` | 1 red, C-order only, at the unreadable-record assertion |

2b is there because mutation 2 stops at the test's first audit assertion, so on its own it says
nothing about the second. In every mutated run the other 17 tests stayed green, so each new test
is the only thing pinning its line.

## Gates

Through the project's runner, from `wyrd-pdca`:
`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt timeout 7200 ./engine/xtask.sh ci`.

One run: `xtask ci: all checks passed`, exit 0. It ran `typos`, the docs lint
(`lint_docs: OK`), the docs render and link audit (99 pages, `link audit OK`), the gitlink and
unsafe guards, `cargo fmt --all -- --check`, clippy, the workspace tests, cargo-machete, cargo
deny (three runs), madsim clippy and `cargo test -p wyrd-dst (--cfg madsim)`. 1472 tests passed,
0 failed — round 4's 1470 plus the two new ones, both of which appear in the log as `ok`, as do
the four seeded DST staged-handoff legs. Both external dependencies the brief names were present
(the gate ran `typos` and the docs tools, no warn-skip lines in the log), and I ran `typos` on
the test file alone before the gate.

`cargo fmt --all` was run after every edit. After the gate, `git diff` was byte-identical to the
`patch.diff` in the bundle (`cmp`).

## Self-refutation (the three forced questions)

**(a) Genuine red?** Yes, run, not argued. With production at base both new tests fail by
assertion (13 of 18 red in the file). With the full patch in place, each goes red under a
one-line mutation of the exact production line it is meant to pin, and only that test does.

**(b) Production path?** Yes. Both tests call `wyrd_custodian::reconcile_step` with a real
`ScrubContext`; the paging under test is `staged_committed_parts`' own loop over the real
`STAGED_PAGE`, and the audit assertions read the `tracing` events the production emitters send.
The doubles implement only the `MetadataStore` / `ChunkStore` traits. The double's `scan_page`
uses the seam crate's own `page_limit` / `page_start` / `page_cursor`, as it did in round 4; I
added one `push` to it and changed nothing about what it returns.

**(c) Fixture includes the fault?** Yes. A-paged puts a lost fragment on page 2 and on page 3,
and checks the listing really took 3 pages, so the fixture cannot quietly shrink to one page.
C-order arms a real read fault on `inode:` and asserts the pass returned that `Err`, so the
"later fault" demonstrably happened after the staged read and before anything else could name
the records.

## Items for the human

- The test restates `STAGED_PAGE` = 512 because the constant is private to `gc.rs`. I did not
  make it `pub` to share it: that would be a production change, which this round rules out. The
  page-count assertion is the guard against the two drifting apart.
- While editing the file's falsifiability note to name the two new legs, I also corrected its
  base hash from `97fc2f9` (an earlier round's base) to `4ab2b28`, the base I actually ran the
  red on.
- I read `iteration-v4/build-notes.md` and applied `iteration-v4/patch.diff`. The brief's
  `Citations expected` names the v3 copies; the carry-forward block names `iteration-v4/` as the
  preserved previous attempt and says everything in it stands, so v4 is the starting point. I
  did not open v4's review or adversary files; the carry-forward text was specific enough.

## Scratch

Backups, the saved diffs and the gate log (`ci.log`) are in
`$PDCA_SCRATCH/pdca-builder-813-r5` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_813/…`), about 1.3 MB
of text and no build cache. I left the directory in place: it sits inside the
harness's own per-issue scratch root, which the harness reclaims, and my instructions say not to
run `rm` there. Nothing of mine is under `/tmp`. Every
source edit was made in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`); the new test file is
intent-to-add there so `git diff` includes it. No branch was pushed and no PR was opened,
readied or merged.
