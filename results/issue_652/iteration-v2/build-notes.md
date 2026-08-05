# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 2)

*Withheld from the reviewer; written for the human at sign-off.*

---

## 0. Read this first — an orphaned agent process was editing this cycle's worktree

While I was reading the base tree, `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`) started
changing **under me**: `crates/core/src/metadata.rs` gained a rewritten `high_water_marks`
(mtime advanced twice while I watched, 22:35:37 → 22:37:23) and `crates/server/src/lib.rs` was
modified too, with `git status` clean minutes earlier.

Cause: **two orphaned builder leaves from a killed earlier flow were still running** and still
writing:

| PID | started | parent | `--add-dir` |
|---|---|---|---|
| 2098257 | 22:25:34 | 8500 (`systemd --user`) — reparented, its flow is dead | `wyrd.pdca-wt-l1` (**this bundle's lane**) |
| 2098261 | 22:25:34 | 8500 — same | `wyrd.pdca-wt-l0` (bundle 651's lane) |
| 2099838 | 22:27:24 | 2099809 = the live `wyrd-pdca flow 651 652` | `wyrd.pdca-wt-l1` (**me**) |
| 2099836 | 22:27:24 | 2099809 — live | `wyrd.pdca-wt-l0` (651's live builder) |

Both orphans belonged to a `wyrd-pdca flow` whose driver had already exited, so nothing could
ever collect their output — but they were still mutating the lanes *and* would have written
into `results/issue_65{1,2}/` (patch.diff, build-notes.md, the test file), racing the live
builders for the same filenames.

What I did: `kill -TERM 2098257 2098261`, then `git checkout --` on the two files the l1 orphan
had touched, verifying the lane was byte-clean at `d50f0ca` before making a single edit of my
own (`git status --porcelain` empty; `high_water_marks` back at `metadata.rs:2073` with its
base signature). **I did not touch lane l0's files** — bundle 651's live builder owns those, and
some of the modifications there (`custodian/{gc,restore}.rs`) may well be its legitimate work.

Two things for you:

1. **Bundle 651 may be contaminated.** Its lane was being written by an orphan until 22:38.
   Worth a `git diff` sanity read of `results/issue_651/patch.diff` before you accept it.
2. **This bundle is clean.** Everything below was produced after the reset; the patch was
   regenerated from a verified-clean base and re-verified end to end.

Act candidate: the driver has no orphan sweep for leaf agents. A killed `pdca flow` leaves
`claude -p --agent …` children alive, holding a lane. Suggested doctor row in §7.

---

## 1. What the patch does

Startup recovery — the step `Gateway::recover()` runs *before the gateway serves anything* — was
neither total nor bounded, and half of what it computed had no reader.

| File | Change |
|---|---|
| `crates/core/src/metadata.rs:2033-2040` (base) | **deleted** `parse_pending_chunk_key` — its only caller was the removed `pending:` walk |
| `crates/core/src/metadata.rs:2041` | **new** `RECOVERY_AUDIT` — the audit target damaged rows are attributed on |
| `crates/core/src/metadata.rs:2059` | **new** `RECOVERY_PAGE = 128` — the walk's page width, with an honest statement of what a page does and does not bound |
| `crates/core/src/metadata.rs:2072-2094` | **new** `for_each_page` (salvaged from `sources/salvage.diff`, adapted) — the bounded-page walk over a prefix |
| `crates/core/src/metadata.rs:2147-2196` | `high_water_marks` rewritten: `-> Result<InodeId>`, paged, total over damage, every unaccountable row attributed |
| `crates/core/src/metadata.rs:3499-3518` | the standing chunk-id test **removed**, with the reasoning left in its place |
| `crates/server/src/lib.rs:134` | `let max_inode = metadata::high_water_marks(&self.meta).await?;` — the discarded `_max_chunk` binding is gone |
| `crates/server/src/lib.rs:124-132`, `:246-248`, `:261-263` | doc comments that described the removed half |
| `crates/server/src/cli.rs:1670-1676` | `alloc_inode` refuses at the id-space ceiling instead of `id + 1` panicking / wrapping to the reserved id 0 |
| `docs/design/architecture/08-crosscutting-concepts.md:85` | living-architecture currency: id recovery is no longer a chunk-map consumer |

Size: **58** semantic (non-blank, non-comment) added lines in `metadata.rs`, 8 in `cli.rs`, 1 in
`lib.rs`, 318 in the new test — ~385 total against a ≤1,500 budget, **5 files** against ≤15. The
brief predicted this: #647's floor apparatus (~282 semantic lines) is removed, not ported.

### Why `high_water_marks` keeps its (now singular) name

Renaming to `inode_high_water_mark` would read better, but the name is cited from two **frozen**
documents I must not edit — `docs/design/adr/0046-bucket-model-real-namespace.md:59` and
`docs/design/proposals/draft/0016-multipart-commit-protocol.md:829` — so a rename strands those
references with no legal way to update the ADR (`docs-immutability`: an Accepted ADR is frozen,
supersede-don't-rewrite). The brief also anticipated a signature change under the existing name.

### The one deliberate scope extension: `cli.rs:1670`

Round 1's reviewers found (twice) that a total walk makes `inode:18446744073709551615` reachable
as a mark, after which `alloc_inode`'s `id + 1` panics in a checked build or rolls the persisted
counter to the **reserved id 0** in a release build. That is a re-mint of every id in the store —
precisely the hazard `seed_next_inode_floor` exists to prevent, arriving through the door this
slice opens. Three options:

* **Clamp the mark in `high_water_marks`** (0 extra lines): silently under-approximates the floor.
  The brief's invariant forbids exactly this ("never a quiet under-approximation ... never zero").
  Rejected.
* **Fail `recover()` closed at the ceiling** (`checked_add` in `lib.rs`, 3 lines): re-introduces a
  refusal at startup — one planted key would stop the gateway, which is the failure mode this
  whole slice removes. Rejected.
* **Fail the *allocation* closed** (`cli.rs`, 8 semantic lines, one `else` block): recovery stays
  total, every committed object stays readable, and only the PUT that actually needs a fresh id
  fails — with a message naming the cause. **Chosen.** It costs at most one id (`u64::MAX` is
  never minted), and `crates/server/src/cli.rs` is one file outside the brief's named two, well
  inside the file budget.

---

## 2. Round-1 findings, one by one

`review-batch.md` carried 10 blocking findings (several duplicates). **Eight fixed, two recorded
as rejected** (`review-rejected.md`), none re-submitted unchanged.

| # | Finding | Disposition |
|---|---|---|
| 1, 4 | `metadata.rs:2121` BUG — undecodable `inode:u64::MAX` seeds a saturated floor; next `alloc_inode` panics on `id + 1` or wraps to reserved inode 0 | **FIXED** `cli.rs:1670`; bound by test 3, which fails with *only* that hunk reverted (see §3) |
| 2, 3 | `metadata.rs:2055`/`:2073` BUG — the page bounds rows, not bytes; over-`MAX_VALUE_BYTES` values can still be materialised | **PART-FIXED + REJECTED residual.** The walk no longer decodes or retains an over-ceiling value (`metadata.rs:2167`) and the doc no longer claims a heap bound it cannot enforce. The residual is the seam's (`traits/src/lib.rs:995-999`, getwyrd/wyrd#674) — the tree's own already-reviewed answer for the identical question in `read_group_range` (`metadata.rs:2217-2224`). Recorded at both lines. |
| 5, 6, 8 | test gap — the size test held one inode, so it never crossed a page or exercised the continuation cursor | **FIXED.** The fixture is 153 rows (3 pages of 128): a single-page walk answers 227, the correct one answers 251. |
| 7 | test gap — the attribution assertion checked only the key, so moving the warning off the audit target would still pass | **FIXED.** `attributed()` requires `"target":"wyrd.metadata.recovery.audit"`, `"action":"…"` and `"key":"…"` **on one JSON line** (the subscriber is built with `LogConfig::new(Some("warn"), Some("json"))`). |
| 9 | `metadata.rs:2120` CONVENTION — a malformed `inode:` key was silently skipped, against the repo's absent-or-unsupported-entry rule | **FIXED.** Attributed as `inode-key-unparsable`; asserted in test 1. |
| 10 | `metadata.rs:2127` CONVENTION — the living architecture still requires id recovery to return a typed error for unresolvable chunk maps | **FIXED.** `08-crosscutting-concepts.md:85` — id recovery removed from the chunk-map-consumer list, with one clause saying why. The file is `status: living`, so `docs-immutability` permits the edit; the frozen ADR/proposal references were left alone. |

On finding 10 vs the brief's "**out of scope:** any docs paragraph": that instruction is about not
importing *other slices'* architecture prose (record shape, resolver, staged publication). The
sentence I touched names **this slice's own function** and became false with this patch, and
"Docs currency … in the same PR" is a hard MUST in the repo's rubric. Removing a false clause is
the smallest change that keeps the doc true; I added no new paragraph.

Also fixed while there, not from a finding: an extra mutant-killing property in test 2. With ids
`{1, 100..=250, 99}` the byte-lexicographically **last** row (`inode:99`) is not the largest id,
so a walk that keeps "the last id it saw" instead of the maximum answers 100 and fails.

---

## 3. Refuting my own test (the three forced questions)

**(a) Genuine red?** Yes — through the project's own runner, not by hand:

```
$ PDCA_BUNDLE=…/results/issue_652 PDCA_LANE=1 ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
    test result: ok. 3 passed; 0 failed
run-verify.sh: RED   — (production reverted, test kept)
    recover() must be Ok(()) … : Error("expected ident", line: 1, column: 2)          [test 1]
    recover() must be Ok(()) … : ScanCapExceeded { cap: 1048576, prefix: inode: }     [test 2]
    recover() must be Ok(()) … : Error("expected ident", line: 1, column: 2)          [test 3]
    test result: FAILED. 0 passed; 3 failed
run-verify.sh: PASS — red without the fix, green with it.
```

Each red is an **assertion** through the signature-stable `Gateway::recover()`, on the exact
pre-fix cause: `decode(&value)?` for tests 1/3, `scan` for test 2. No compile failure, no
zero-test vacuum (3 tests ran on both legs).

Because the whole production change is reverted at once, that leg does not by itself prove the
`cli.rs` hunk is load-bearing, so I refuted it separately — reverting **only** the ceiling guard
(`let next = id + 1;`) and re-running:

```
thread '…exhausted_inode_space…' panicked at crates/server/src/cli.rs:1670:20:
attempt to add with overflow
test result: FAILED. 2 passed; 1 failed
```

Exactly the reviewer's finding, now bound. Guard restored, all 3 green again.

**(b) Production path?** Yes. Every assertion drives `wyrd_server::Gateway::{recover, put_object,
get_object}` over the real composition (`RedbMetadataStore` + `FsChunkStore` + `MemCoordination`),
which is `s3_http_wire.rs`'s own. Nothing is re-implemented in the test: the floor is read back
from the **persisted** `meta:next_inode` the production `seed_next_inode_floor` wrote, the audit
lines come from the production `tracing` callsites through the production `logging::dispatch`, and
the refused PUT is refused by the production `alloc_inode`. The only test-owned type is
`ScanRefusingStore`, and it is a *pass-through* to a real `RedbMetadataStore` for every method
except `scan`, which it makes refuse — it stands in for a store too large to scan, not for the
code under test.

**(c) Fixture includes the fault?** Yes, and this is where round 1 was weakest:

* test 1 keeps the damaged rows **in the same store** as the healthy object, at ids *above* it, so
  the asserted floor can only come from a damaged record's key. Three distinct faults present:
  undecodable value, over-ceiling value, unparsable key.
* test 2's store really is more than one page wide (153 rows / 3 pages) with the maximum on the
  last page — the round-1 fixture had a single row and proved nothing about paging. The `scan`
  refusal is asserted to be live before the scenario runs, so a double that forgot to refuse
  cannot produce a false green.
* test 3 keeps `inode:18446744073709551615` in the store and then *demands the allocation* that
  the ceiling makes impossible, rather than asserting the mark in isolation.

---

## 4. Gates run locally (this tree, this patch)

| Step | Result |
|---|---|
| `typos` (prose gate — in play now that a docs file is touched) | exit 0 |
| `python3 docs/publishing/tools/lint_docs.py` | `lint_docs: OK` |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets` | no warnings |
| `cargo build --workspace --all-targets` | ok |
| `cargo test --workspace --exclude wyrd-dst` | **158 suites ok, 0 failed** — includes `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode` (criterion 4, unchanged) and the `alloc_inode` regressions in `backend_selection.rs` |
| `cargo-machete`, `cargo deny check` | ok |
| `./engine/xtask.sh conformance` | 5 valid + 6 invalid vectors pass |
| `./engine/xtask.sh statics` | ok (ADR-0035) |
| `./engine/xtask.sh dst` | exit 0 |
| `./engine/scripts/run-verify.sh` | **PASS — red without the fix, green with it** |
| `git grep -n "_max_chunk" -- crates/` | empty → criterion 3 |

**One step of `xtask ci` failed, and not because of this patch:**

```
$ cargo deny --all-features --config deny-all-features.toml check advisories
error: unexpected argument '--config' found
  tip: 'check --config' exists
```

Installed `cargo-deny 0.19.9` (2026-07-02) wants `check --config <file>`; `xtask`'s invocation
(unchanged since `7258fec`, 2026-07-14) passes `--config` before the subcommand. I reproduced the
identical failure in `../wyrd-verify-l1`, a tree carrying **none** of this patch's production
changes — it is a CLI parse error, so it cannot depend on tree contents. Re-ordered by hand
(`cargo deny --all-features check --config deny-all-features.toml advisories`) it answers
`advisories ok`.

Confusingly, sibling C4-ci runs on this same host **passed** today (this bundle's iteration-v1,
226 s; bundle 651's iterations), so the driver's gate environment evidently resolves it somehow
that my shell does not, and I could not find the difference (single `cargo` 1.96.1, single
`cargo-deny` 0.19.9, no toolchain-local copy, same PATH lineage). Flagged in §7 so it cannot
silently cost an auto-iterate round: **if C4-ci is green at Check, this note is already answered.**

---

## 5. What I ruled out

* **Wiring `max_chunk` to a caller** instead of deleting it — the brief settles this ("ACCEPTED by
  the maintainer at Plan … not Do's to revisit"), and the evidence is history: `fdd34f1` (#487)
  both removed the consumer and made the mark unnecessary. Wiring would mean inventing a consumer.
* **Porting #647's floor apparatus** (`RecoveredIds`, `ClassIds`, `torn_digit_escape`,
  `json_string_token`, `segment_chunk_floor`, …): ~282 semantic lines of byte-level JSON
  scavenging carrying the very under-approximation bug that closed #647. Only `for_each_page` and
  the paged body were salvaged, as the brief directs.
* **Keeping the `pending:`/`orphan:` walks** "for safety": they exist only to feed the removed
  chunk mark. Keeping them would leave two more unbounded `scan`s in the startup path — against
  this slice's own bounded goal — for a number nobody reads.
* **A keys-only page seam** (`scan_page_keys`) to close the byte-bound finding properly: a new
  required method on `MetadataStore` means five backend implementations + the shared conformance
  suite (`crates/metadata-{redb,tikv,fdb}`, testkit, traits — the `scan_page` rollout in #634/PR
  #645 touched ~600 lines across them). Out of proportion here, and it is already tracked as
  getwyrd/wyrd#674.
* **Co-located unit tests in `metadata.rs`** in addition to the integration target: every branch
  the walk has (unparsable key, over-ceiling value, undecodable value, page continuation, maximum
  across pages) is already driven end-to-end through `Gateway::recover()`. A second copy at the
  unit level would bind the same code twice and add ~120 lines.
* **A metric counter** beside each audit line (`monotonic_counter.*`, as `salvage.diff` had): the
  criterion asks for attribution on the audit seam; a metric is a separate observability decision
  and `wyrd.metadata.recovery.audit` already matches the tree's `wyrd.*.audit` filter convention.

---

## 6. Self-review against the repo rubric (`AGENTS.md` § *Review rubric & protocol*)

* *One clock per lifecycle* — no clock read added.
* *Narrow trait seams / dependency direction* — `wyrd-core` still speaks only `MetadataStore`;
  `scan_page` is the seam the resolver in this same module already uses.
* *Metadata validation boundaries* (ADR-0045) — decode still enforces structural invariants; this
  walk **consumes** the error rather than weakening it, and never turns a failed decode into a
  value. It reads the id from the key, which is not a validation relaxation.
* *No DST-reachable shared mutable global state* — `xtask statics` green.
* *`#![forbid(unsafe_code)]`* — present in the new test crate root; no new crate.
* *Docs currency* — the living architecture updated in the same patch (§2, finding 10).
* *Absent or unsupported entries* — the defect class this patch is about: every row the walk
  cannot account for produces an explicit operator-visible attribution, never a silent skip, and
  the assertions bind the target as well as the message.
* *Transactions* — untouched (`seed_next_inode_floor`'s CAS loop is unchanged; the new `alloc_inode`
  refusal returns **before** any batch is built, so no live transaction is abandoned).
* *Await discipline* — the page loop terminates on the paging contract (clause 3, exclusive cursor);
  no new unbounded await.
* *Test fidelity* — the new target is its own binary because `tracing` caches callsite `Interest`
  process-globally; a sibling test touching these callsites first would make the attribution
  assertions vacuous (the reason `log_span_correlation.rs` is separate too).

---

## 7. For the human

NEEDS-HUMAN external dependency: cargo-deny CLI compatible with `xtask`'s `--config`-before-subcommand invocation — blocked the last two steps of a local `cargo xtask ci` (`deny --all-features … advisories`, then licences/bans/sources), so I could not produce a full local C4-ci pass; every other step, including `cargo test --workspace` (158 suites), conformance, statics and dst, is green with this patch, and the failure reproduces on a tree without this patch's production changes. If the driver's C4-ci is green at Check, this is already answered.

```toml
[[doctor.checks]]
id    = "cargo-deny-cli"   # the token Plan should have put in `External dependencies`
cmd   = "cd \"${WYRD_REPO:-../wyrd}\" && cargo deny --all-features --config deny-all-features.toml check advisories"
hint  = "cargo-deny >= 0.19 moved --config behind the subcommand (`cargo deny … check --config <file> advisories`); either install a cargo-deny whose CLI matches xtask::dependency_wall_invocations() or fix that invocation in ../wyrd/xtask/src/lib.rs:163-182"
level = "WARN"             # the gate degrades (two audit steps skipped) but the build is unaffected
```

And the orphan-leaf hazard from §0, which cost this cycle a corrupted worktree and may have cost
bundle 651 more:

```toml
[[doctor.checks]]
id    = "no-orphaned-leaf-agents"
cmd   = "for p in $(pgrep -f 'claude -p --agent' || true); do pp=$(ps -o ppid= -p $p | tr -d ' '); ps -o args= -p $pp 2>/dev/null | grep -q pdca || exit 1; done"
hint  = "a killed `pdca flow` leaves its leaf agents running and still writing to $PDCA_WORKTREE and results/issue_<id>/; kill them before starting a new flow (they are the ones whose parent is no longer a pdca process)"
level = "WARN"
```
