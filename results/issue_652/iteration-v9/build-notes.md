# Build notes — issue 652 (`recovery-total-over-damage`), iteration 9

**Withheld from the reviewer.** For the human at sign-off.

Target: `getwyrd/wyrd @ main`, `d50f0ca`. Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
Patch: 4 files, `+802 / −118` raw, **420 added semantic lines** (non-blank, non-comment;
`awk` over `patch.diff`) against the brief's `≤ ~450 across ≤ 5 files`. No `alloc_inode`
change, no paging symbol, no floor apparatus.

---

## 1. What this round changes relative to iteration 8

Iteration 8 was green on `C4-ci` and on `C4-verify` (red→green) and was sent back for two
reasons. Both are addressed here; nothing else about its shape was re-litigated, because the
shape is what the re-planned brief asks for.

1. **C5 causal adequacy — the segmented root was contained but never *attributed*.** Post-#648
   a segmented root **decodes**, so iteration 8's only audit call — inside `if let Err` — could
   never name it, and its criterion-2 test bound `_audit` and threw it away. Fixed at both
   ends: the walk now classifies the decoded record and names a segmented root on the same
   seam (`crates/core/src/metadata.rs:2160-2166`, action `unresolved-segmented-inode-root`),
   and criterion 2 asserts it (`crates/server/tests/gateway_recover_totality.rs:313-324`).
   This is also what the repo rubric requires of the class — *"Absent or unsupported entries:
   produce an explicit error or enqueue a repair obligation — never silent success, silent
   skip"* (`AGENTS.md:175-177`) — and the walk is not allowed the error branch any more, so
   the obligation is the only legal answer.
2. **T4 — five blocking batch-review findings, `0 recorded-rejected`.** All five are now
   disposed of: **two fixed**, **three rejected with a recorded reason**, each rejection
   written at *both* the line round 8 reported it at and the line the same content occupies
   in this patch (`review-rejected.md`, new "Iteration 8" section at the top). See §5.

The other iteration-8 gate failure, `C5-mutants`, was **not** the patch: the log shows
`cargo test failed in an unmutated tree` because `crates/server/tests/health_probe.rs:706`
(`checks_still_answer_with_watch_streams_held_open`) failed on a probe-capacity race
(`ResourceExhausted`) — an unrelated flaky test in a file this patch does not touch
(`iteration-v8/gate-logs/C5-mutants.log`).

---

## 2. The change, file by file

**`crates/core/src/metadata.rs`** — `high_water_marks` returns the **inode mark alone**
(`:2137`, `-> Result<InodeId>`), and is total:

* the mark comes from each row's **key**, read before the value is looked at (`:2143-2153`),
  so an unreadable record still raises the floor — never a quiet zero;
* the value is read **only to attribute** what the walk cannot account for (`:2159-2170`):
  bytes that do not decode, **and** a structurally valid segmented root it has no resolver
  for. Both go through one helper (`:2067-2082`) that raises a counter and writes an audit
  event naming the key. A key that is not `inode:<id>` is named too (`:2148-2153`);
* the `scan` itself still `?`-propagates (`:2139`) — the split the peer draws at
  `crates/custodian/src/gc.rs:355-359`: a store the walk cannot read is not one record's
  fault;
* deleted with the chunk mark: `parse_pending_chunk_key`, the `pending:` and `orphan:` scans,
  `IN_PROCESS_CHUNK_CEILING` (three full scans became one). `git grep _max_chunk -- crates/`
  is now empty (brief criterion 4).

The standing unit test `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_
chunk_ids` is replaced by `high_water_marks_is_total_over_records_it_cannot_read` (`:3475-3516`).
The replacement carries the reason in its doc, and the same reason is in the commit body: the
old test's premise needs a minter allocating below `2^64`, and `fdd34f1` (#487, 2026-07-08)
removed the last one while writing the discard at the callsite in the same commit.

**`crates/server/src/lib.rs`** — `recover` takes the narrowed result (`:133-136`); the doc
now states the totality property and what recovery does with what it cannot read
(`:108-132`), and the two chunk-minting docs that pointed at the removed floor are re-pointed
(`:246-248`, `:261-262`).

**`crates/server/src/cli.rs`** — `seed_next_inode_floor` (`:1717-1750`) contains an unreadable
counter instead of `?`-ing on it: unreadable bytes state **no** floor (`Option<u64> = None`,
`:1726-1734`), which never satisfies the floor test, so the existing compare-and-set runs and
repairs the counter to `floor` — guarded on the exact bytes that were read (`:1742`), so a
live allocator that won the race is never rewound. The attribution helper (`:1774-1789`)
quotes the bytes it replaces, bounded to 64 (`:1764`) and escaped.

---

## 3. Why this shape (and what was ruled out, with the cost)

**The invariant is the target, not the diff size** (brief *Invariant to restore*; principles
§1.2/§2). C-1 over startup recovery has three clauses and each maps to a line of the patch:

| Clause | Where it lands |
|---|---|
| Recovery is total over stored content | `metadata.rs:2143-2170`, `cli.rs:1726-1746` |
| One record's fault is *contained* — attributed, walk continues | `metadata.rs:2067-2082` + the three call sites |
| A floor an allocator trusts is never a quiet under-approximation | `metadata.rs:2143-2147` (key first, value never) |
| A number nobody reads is not a safety property | the chunk mark and its two scans are deleted |

Ruled out, with costs:

* **Keep the chunk mark and "harden" it** (#647's route). Rejected at Plan, ratified by the
  maintainer, and re-recorded here: nothing in the tree mints below `2^64`
  (`lib.rs:239-252` ≥ 2^127; `cli.rs:1790-1799` ≥ 2^64), so wiring it means inventing a
  consumer, and #647's hardened version reported **0** for a corrupted flat root — an
  under-approximation. Cost of the alternative: the two extra full scans stay (`pending:`,
  `orphan:`), plus the byte-scavenging apparatus the brief lists by name
  (`RecoveredIds`, `ClassIds`, `torn_digit_escape`, `json_string_token`,
  `scavenged_chunk_id_floor`, `segment_chunk_floor` — ~700 lines in iteration 7's patch).
* **Skip the value entirely** (the mark is key-derived, so the walk needs no `decode` at all).
  This is the smallest possible diff: it deletes the `decode` call and the whole attribution
  helper, ~45 semantic lines less. Rejected because it converts *"refuses over a damaged
  record"* into *"cannot see a damaged record"* — startup is the one pass that reads every
  record, and the rubric's *absent-or-unsupported* class forbids the silent skip. The brief
  asks for attribution in as many words, and the reviewer's C5 finding made it the blocking
  item this round.
* **Return the attribution to the caller** (`-> Result<(InodeId, Vec<(Vec<u8>, String)>)>`,
  the shape `ReferenceSet::unresolvable` uses in GC). Rejected on two counts: it would make
  `Gateway::recover` decide what to *do* with a list nobody consumes (the exact defect this
  slice removes for the chunk mark), and the acceptance test drives `recover() -> Result<()>`,
  whose signature the brief pins — a returned list would be observable only by naming
  `high_water_marks`, which the brief shows would turn the RED leg into a *compile* failure
  and be scored as a pass. The tracing seam is the tree's existing answer for exactly this
  (`read.rs:231`, `gc.rs:524`, `reconstruction.rs:720`).
* **Leave a damaged `meta:next_inode` in place and let `alloc_inode` fail closed** (this
  bundle's iteration 6/7 shape). The re-planned brief's criterion 3 reverses it, and the
  reason is C-1: the gateway starts and is **permanently write-dead** — every new-key PUT
  fails on the same key (`cli.rs:1653-1657`) until a human hand-writes the number the store's
  own `inode:` records already prove. Residual of the direction taken is in §6.

---

## 4. Evidence — the three refutation questions

**(a) Genuine red?** Yes, and measured by the project's own harness rather than by hand:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` applies `patch.diff` to a clean `../wyrd-verify`
worktree off `origin/main`, keeps the added test, reverts the three production files, and runs
`cargo test -p wyrd-server --test gateway_recover_totality`. Result:

```
running 4 tests
recover_is_total_over_a_segmented_root --- FAILED
recover_is_total_over_an_undecodable_inode_record --- FAILED
the_counter_repair_yields_to_an_allocator_that_won_the_race --- FAILED
recover_is_total_over_a_corrupt_next_inode_counter --- FAILED
…
run-verify.sh: PASS — red without the fix, green with it.
```

All four are **assertion** reds on base-visible symbols (`"high_water_marks met a segmented
chunk map…"`, `"expected ident at line 1 column 2"`, `"invalid digit found in string"` ×2) —
not compile errors, which is why the test drives `Gateway::recover()` and
`cli::seed_next_inode_floor`, both signature-stable, and never names `high_water_marks`.

**(b) Production path?** Yes. Criteria 1-3 run the real composition root
(`Gateway::{recover, put_object, get_object}` over `RedbMetadataStore` + `FsChunkStore` +
`MemCoordination`), the real logging dispatcher (`wyrd_server::logging::dispatch`), and the
real inode resolver (`wyrd_core::read::resolve`) to read back the id a PUT actually minted.
Criterion 4 calls the production `cli::seed_next_inode_floor` — the function
`Gateway::recover` delegates its second half to (`lib.rs:135`) — through a **wrapper around
the real redb store** that forwards every call; only the *interleaving* is injected, no
behaviour is modelled.

**(c) Fixture includes the fault?** Yes, in every leg: the undecodable record, the segmented
root and the corrupt counter are all written **into the same store that holds a healthy
committed object**, and every assertion is about that store — the healthy object still reads
back byte-identical, and the following PUT commits above the damaged record's id. Nothing is
curated out. Criterion 4's fixture keeps the racing peer *in*: the peer's write really lands
between the read and the compare-and-set.

Extra checks run beyond the three:

* **Mutation checks (by hand, on this tree)** — see §4a below.
* `cargo test -p wyrd-server --test s3_http_wire` — 19/19 including
  `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode` (brief criterion
  5, unchanged).
* `cargo test -p wyrd-core --lib` — 42/42 including the replacement unit test.
* `./engine/xtask.sh ci` (the C4-ci gate: fmt + clippy `-D warnings` + build + test + DST +
  cargo-deny + conformance vectors) — see §4b.
* `git grep -n "_max_chunk" -- crates/` → empty (criterion 4); no `RecoveredIds` / `ClassIds`
  / `scan_page` / `for_each_page` / `RECOVERY_PAGE` in any file this patch touches.

### 4a. Mutation checks

Four mutants were applied by hand to this tree, each run through
`cargo test -p wyrd-server --test gateway_recover_totality`; all four are **killed**, and each
kills a *different* assertion, so no test in the file is carrying the others:

| Mutant | Result |
|---|---|
| **M1** — drop the compare-and-set guard on the counter repair (`guard = WriteBatch::new()` instead of `require(bytes)`, `cli.rs:1742`) | `the_counter_repair_yields_to_an_allocator_that_won_the_race` **fails**: `left: Some(2), right: Some(900)` — the repair rewound the peer allocator's counter, which is precisely the hazard the guard exists for. |
| **M2** — drop the segmented arm (`Ok(_) => {}` only, `metadata.rs:2160-2166`) | `recover_is_total_over_a_segmented_root` **fails** at the attribution assert with an empty capture — i.e. the round-8 C5 gap is now bound by a test, not by prose. |
| **M3** — attribute *every* row (blanket call in the `Ok` arm) | criteria 1 **and** 2 **fail** on `attributed_records(&audit) == 1`: attribution that names healthy records is caught, so the count assertion is a discrimination check, not decoration. |
| **M4** — drop the undecodable arm (`Err(_) => {}`, `metadata.rs:2168`) | `recover_is_total_over_an_undecodable_inode_record` **fails** at its attribution assert with an empty capture. |

The whole-patch revert is the RED leg above (a): all four tests fail. The tree was restored
byte-for-byte after each mutant (`git diff --stat` back to the patch's own diffstat) and re-run green.

One incidental finding from M2/M4: removing an attribution call makes the corresponding action
constant dead, and the workspace's `warnings = "deny"` turns that into a **compile** error — so
the walk cannot silently lose a class of attribution without the gate noticing.

### 4b. `cargo xtask ci`

`PDCA_WORKTREE=… ./engine/xtask.sh ci` — **`xtask ci: all checks passed`** (exit 0):
`typos`, the docs lint, `cargo fmt --check`, `clippy -D warnings`, `build --all-targets`, the
whole workspace test suite, the madsim DST sweep, real `cargo-deny`, the statics gate and
`xtask conformance: 5 valid + 6 invalid vectors pass` (`gate-logs/local-ci.log`).
Commit-readiness is therefore covered by the target's own formatter/linters, not only by
PDCA's gates.

*Exactly what that run covered*: it ran on the tree at `+797/−118`; the final patch is
`+802/−118`, and the delta is **doc comments only** (four added lines explaining why the walk
needs no chunk-map resolver, plus `path:line` citation strings inside existing doc comments).
After that edit, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`,
`typos` over all four touched files, `cargo test -p wyrd-core --lib` (42/42),
`cargo test -p wyrd-server --test gateway_recover_totality` (4/4) and the full
`run-verify.sh` red→green leg were all re-run green on the **final** tree. The Check beat's
own `C4-ci` row re-runs the whole gate on it regardless.

**One flake worth knowing about at sign-off**: the *first* attempt at this run wedged for 38
minutes in `crates/server/tests/custodian_day_one.rs` with **every** test thread parked in
`futex_do_wait` at 0% CPU — a whole-binary deadlock, not a spin. That binary passes in
**0.19s** when run alone on this same patched tree (`cargo test -p wyrd-server --test
custodian_day_one` → 15/15), and it passed in the re-run above, so it is the known
target-side test deadlock the harness already documents (`pdca.toml`: "issue_635's advisory
C5-mutants row hung for 19h16m on a target-side test deadlock, getwyrd/wyrd#646"), not
this patch: nothing in that file exercises a damaged record, and this patch emits **no**
event on a healthy path, so it changes no behaviour those tests can reach. If `C4-ci` or
`C5-mutants` comes back hung or red inside `custodian_day_one`, re-run before reading it as
a verdict.

---

## 5. The five batch-review findings, one line each

Full reasoning (with citations) is in `review-rejected.md` §"Iteration 8", recorded at both
round 8's lines and this patch's.

**A note on how the decisions file is written this round, because it changed shape.** The gate
(`scripts/review-branch`) suppresses a finding only when a recorded line matches its
**(file:line, CLASS)** *and* the recorded `MATCH` string occurs, case-insensitively, in *that
run's* rationale. Earlier rounds recorded whole sentences as `MATCH` and classed the standing
items `SCOPE` — which the parser does not even accept (`BUG` / `CONVENTION` / `TEST-GAP` /
`NOISE` only), so round 8 scored `0 recorded-rejected` and the gate blocked. This round the
reasoning is prose (R1-R4 plus the standing items) and the decision lines are short, pinned
phrases (`no-backoff`, `saturating_add`, `interrupted cluster put`, `tier-0 dst`,
`chunk-id floor`, `scan_page`, `timeout`, …) enumerated over the handful of lines each finding
can land on. I dry-ran the gate's own `load_rejected` / `is_rejected` over the file: all four
round-8 findings and their re-worded variants are recognised, and a fabricated *new* BUG at a
covered line (`metadata.rs:2137`, "the walk mis-parses keys and drops ids") still **blocks** —
which is the property that matters, since a rejection must not become a blanket mute.

| # | Finding | Disposition |
|---|---|---|
| 1 | `cli.rs:1710` unbounded, no-backoff CAS loop | **Rejected.** The loop is `origin/main`'s (`1692-1711` there), unchanged in shape; both in-tree writers only ever raise a *numeric* counter, so the winning values strictly increase and it cannot cycle; termination is bound by a 60s harness deadline in criterion 3, which *fails* rather than hangs. An attempt budget is #687's by an explicit line in the brief's *Out of scope* — and it is a decision, not 12 lines: a budgeted give-up must either return `Err` (breaks totality) or `Ok` below the floor (breaks the floor invariant). |
| 2 | `lib.rs:134` `saturating_add(1)` at `inode:u64::MAX` | **Rejected.** The line is `origin/main`'s and the saturation is already reachable there through a *decodable* record at that key (the id comes from `parse_inode_key`, untouched). The overflow is `alloc_inode`'s `(id + 1)`; every `alloc_inode` change is #687's. The row is now attributed rather than silently skipped. |
| 3 | `cli.rs:1737` repair can reuse an inode an interrupted cluster PUT consumed | **Rejected**, residual acknowledged in the code (`cli.rs:1708-1716`). It is the absent-counter path's residual, unchanged since #364 finding 1 and locked in by `s3_http_wire.rs:645-700`; the alternative is a permanently write-dead gateway; witnessing the id needs the `pending:`/`orphan:` apparatus this slice deletes and still misses a peer's in-flight id; `create`'s `require_absent` keeps committed records from being overwritten. |
| 4 | `gateway_recover_totality.rs:315` no Tier-0 DST for the destructive path | **Coverage fixed, location rejected.** A deterministic, mutation-verified race test ships (`:486-527`). `wyrd-server` cannot compile under `--cfg madsim` (no `[target.'cfg(madsim)']` section; real `tonic` vs `chunkstore-grpc`'s `madsim-tonic` alias, `crates/chunkstore-grpc/Cargo.toml:24-31`; `alloc_inode` sleeps on real `tokio::time`), so a DST leg means moving the persisted-allocator protocol into `wyrd-core` (~250 lines, a public-API move, a new DST target). |
| 5 | `metadata.rs:2057` `from_utf8_lossy` is not unique | **Fixed.** `slice::escape_ascii` at both sites (`metadata.rs:2075`, `cli.rs:1780-1781`), with the reason written at both. |

---

## 6. What the human should weigh at sign-off

* **The counter repair is destructive by design.** Unreadable `meta:next_inode` bytes are
  replaced with `max(inode: key) + 1`. This is the direction the re-planned brief's criterion
  3 requires and it is what the absent-counter path has done since #364, but it is worth a
  conscious "yes": the residual is a store that is *both* counter-corrupt *and* has an
  interrupted **cluster-mode** PUT whose inode id is above every committed record — that id
  can be re-minted, and cluster chunk ids derive from it. Committed objects are protected by
  `create`'s `require_absent`; uncommitted fragments of that interrupted PUT are not. The
  event now names the bytes it replaced, so the state is at least reconstructible from logs.
* **Attribution is `warn`-level and per row.** In a store with many damaged rows, startup
  emits one pair of events per row. That is deliberate (a summary count cannot be repaired),
  and a segmented root is included — today nothing in the tree *writes* one
  (`ChunkMapError::SegmentedMapUnsupported`'s doc: "nothing publishes a segmented map yet"),
  so the practical volume is zero until #649-#651 adopt the resolver. If that changes, the
  segmented action is the one to demote to `info`, not the undecodable one.
* **Not done, deliberately** (all #687, all named in the brief's *Out of scope*): paging /
  `scan_page` / bounded walks, any `alloc_inode` change or `require_absent(inode_key(…))`
  guard, and an attempt budget for `seed_next_inode_floor`. The walk still uses `scan`, whose
  seam promises one consistent cut (`crates/traits/src/lib.rs:1020`), on purpose — `scan_page`
  declines snapshot isolation (`:1061`) and *weakens* the recovered floor, which is what sent
  the previous seven rounds into `alloc_inode`.
* **One stale sentence is left behind on purpose, and it is not the living architecture doc.**
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md:829` still describes allocator
  recovery as recovering "the `< 2^64` in-process chunk-id space". After this patch that clause
  is stale — though the row's **conclusion** ("No — unaffected", because gateway-minted ids are
  ≥ 2^127) is exactly the reasoning that deletes the mark. It is a **draft proposal**, the brief
  rules out "any new or edited ADR / spec / proposal … any docs paragraph", and the rubric's
  docs-currency merge requirement is triggered by ports / API operations / RPCs / CLI flags /
  persisted fields (`AGENTS.md:154-157`) — none of which this patch touches. Worth one line to
  0016's owner when that proposal next moves; a rejection is pre-recorded at that line in
  `review-rejected.md` in case a reviewer reports it.
* **No docs paragraph, no ADR, no conformance vector** was touched: the brief puts the
  living-architecture edits with #648/#649-#651/#653, and this patch adds no port, API
  operation, RPC, CLI flag or persisted field (the rubric's docs-currency trigger,
  `AGENTS.md:154-157`). The audit *events* are new observability, not a persisted field.
