# Build notes — issue 634 / scan-page-seam (iteration 6)

*Withheld from the reviewer; written for the human at sign-off.*

Every citation is `path:line` in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt` = `origin/main` @ `22d71b4` + this patch).

## 0. What this iteration changes, and why

Iteration 5 passed every **gating** row (`C4-ci`, `T4` 0 blocking) and auto-iterated on two
things that were not code defects: the advisory **C5 mutants** row (18 missed of 77) and a §6
item about `scripts/review-branch`'s availability. So the seam itself is not what this
iteration reopens — the trait, the conformance clauses, redb, the two DST sim stores and the
~34 delegating doubles are **byte-identical** to the reviewed patch.

What it does close is the one thing iteration 5's *advisory adversary* actually landed, plus
the finding beside it. Both are in the two backends the brief declares DEFERRED, both were
demonstrated rather than argued, and neither had been addressed:

| # | Adversary finding (iteration 5) | This iteration |
|---|---|---|
| 1 | `crates/metadata-fdb/src/lib.rs:1460` — the `scan_page_once` chunk loop "is exercised by nothing, at any tier", and the failure it guards is a **silent truncation** of the walk. Measured by the adversary: with `>=` → `<`, the whole live `xtask fdb-conformance` leg stays green while a 600-key range answers **138 pairs with `next: None`**. | The rule the loop stops on is now the seam's own (`page_is_full`), so it cannot disagree with the cursor; **and** the at-scale legs the adversary asked for exist and run — `crates/metadata-fdb/tests/scan.rs:113`, `:129`, `crates/metadata-tikv/tests/scan.rs:136`. Re-measured on a live cluster: with the loop flipped the new test fails **134 vs 600** while everything else stays green (§3). |
| 2 | `crates/metadata-tikv/src/lib.rs:1165-1166` — one `scan_page` call can make tikv-client materialize `regions × limit` pairs (it carries a request's `limit` unchanged into every region's shard), i.e. up to `regions × 1,048,576` for the `usize::MAX` limit the seam explicitly invites. | `scan_page` now fills a page from `PAGE_SIZE`-bounded reads inside the one transaction — exactly the machinery the brief pointed Do at (`PageStep` / `next_page_start`), extended with `paging::chunk_size` + `paging::after_chunk` (`crates/metadata-tikv/src/lib.rs:505`, `:543`, driven at `:1315`, `:1336`). |
| 3 | "the Verification-posture claim … is over-broad": the suite's populations never reach either backend's within-page paging path. | Now they do at the live tier for FDB (§3) and by construction for TiKV (`crates/metadata-tikv/tests/scan.rs:160` asserts the fixture spans >1 chunk), and a **new violating double** proves the shared clauses catch the class in-process, in the default gate (`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:1407`). |
| 4 | recorded, not a refutation: `run_all_cap_scoped` is a second runner a driver can forget. | `run_all`'s own doc now says so and names it (`crates/metadata-conformance/src/lib.rs:1468`). |

Net delta over iteration 5: **+817 / −1 lines across 7 files**, of which 431 are the two
at-scale test binaries and 134 the new conformance double.

## 1. The load-bearing change: one fullness rule (`page_is_full`)

**The defect class.** `page_cursor` infers *why* a page stopped from its length — short means
"the prefix is exhausted at this instant", i.e. terminal (`0016:2657-2658`). That inference is
sound **only** if the fill loop stops on exactly that condition. Every backend was spelling the
condition again inside its own loop (`out.len() >= limit`, `items.len() >= limit`, a single
`txn.scan(range, page_size)`), so an off-by-one — or a substrate chunk boundary — produced a
short page that the seam then honestly labelled `next: None`. The caller stops walking a prefix
that is not exhausted: the silent skip the whole primitive exists to prevent, with no error, no
duplicate and no failing clause.

**The fix is to make the disagreement unrepresentable**, not to add a probe:
`wyrd_traits::page_is_full(got, limit)` (`crates/traits/src/lib.rs:545`) is called by
`page_cursor` (`:563`) *and* by every fill loop — redb `crates/metadata-redb/src/lib.rs:196`,
FDB `crates/metadata-fdb/src/lib.rs:1467`, TiKV through `paging::after_chunk`
(`crates/metadata-tikv/src/lib.rs:550`). A mutation of the shared predicate now changes both
sides together (the loop stops early *and* emits a cursor), so the walk stays complete; a loop
that stops on its own rule is what the clauses catch. Pinned as an equivalence over the
boundary neighbourhood in both crates: `crates/traits/src/lib.rs:1681` and
`crates/metadata-tikv/src/lib.rs:695`.

This is why the change is *smallest-that-restores-the-invariant* rather than smallest-diff: the
brief's invariant is "a store primitive … may never silently omit a key that was present
throughout the walk", stated over the category. Guarding it once per backend is five chances to
get it wrong; guarding it in the seam is one.

**Alternative rejected — leave the comparison inline and only add the at-scale tests.** Cost of
the rejected option is not size (it is ~14 lines *smaller*: no `page_is_full`, no two unit
tests, no three call-site edits); it is coverage. The at-scale legs run **off-Check** for both
distributed backends and not at all for redb's own loop, so an inline comparison stays a
`cargo mutants`-visible survivor that only a live cluster can kill — which is exactly the state
the adversary refuted. With the rule shared, the mutation `crates/traits/src/lib.rs:546 replace
>= with <` is **caught** by `cargo xtask ci` (measured, §4).

## 2. TiKV: the page is filled in `PAGE_SIZE` steps

`scan_page` was one `txn.scan(range, min(limit, cap))`. tikv-client shards that request per
region carrying the same `limit`, `Collect`-merges the replies and only then sorts and
truncates — so answering one 1 M-pair page can pull `regions × 1 M` pairs into client memory.
`scan` has always avoided this by looping `PAGE_SIZE = 1024` reads; `scan_page` now does the
same inside the same transaction (`crates/metadata-tikv/src/lib.rs:1307-1354`), which also
keeps the documented "one page is one read timestamp" property intact.

The two decisions are **default-compiled** and unit-tested, in the module whose stated purpose
is exactly that (`crates/metadata-tikv/src/lib.rs:396-413`):

* `chunk_size(limit, got)` (`:505`) — what the page still needs, bounded by `PAGE_SIZE`, and
  **never 0** (a request for zero rows returns nothing, and a loop that continued on it would
  never terminate). Tested `:656`.
* `after_chunk(got, chunk_len, asked, last_key, limit)` (`:543`) — `Full` (the seam's rule) /
  `Exhausted` (a short chunk, the rule `after_page` already uses) / `Continue(next_page_start)`.
  Tested `:674`, `:695`.

No cap arm, deliberately: the bound arrives already clamped by `page_limit`, so a page cannot
breach the cap — escaping that failure is the primitive's purpose.

**Honest limit:** the TiKV *body* is compile-verified here (4 clippy rows, §4) and its decision
logic is unit-tested, but the live cluster on this host still answers
`InvalidKeyMode { storage_api_version: V2 }` at the first commit, so the loop itself is not
exercised against a real TiKV. That is the standing, human-adjudicated posture from the
iteration-3 sign-off ("TiKV accepted as backseat as long as redb/FDB stay green" — not
re-litigated here, and I raise no new NEEDS-HUMAN for it). What is new is that the risk is
smaller than it was: the shape is `scan`'s own proven loop, the decisions are tested off-cluster,
and `crates/metadata-tikv/tests/scan.rs:136` is now waiting for the first maintainer run that
gets a compatible topology.

## 3. Red → green, through the project's runners

### (a) Genuine red? **Yes — twice, and one of them on a live cluster.**

**(i) The at-scale FDB leg, against the exact defect the adversary demonstrated.** With the fill
loop's fullness test inverted in production (`page_is_full` → `!page_is_full` at
`crates/metadata-fdb/src/lib.rs:1467`) and nothing else changed, `./engine/xtask.sh
fdb-conformance` against a throwaway `foundationdb:7.3.77`:

```
running 4 tests
thread 'a_scan_page_that_spans_several_reply_chunks_fills_and_resumes' panicked at
  crates/metadata-fdb/tests/scan.rs:353:9:
assertion `left == right` failed: one page bounded at 600 must be FILLED to 600 across FDB's
  reply chunks — a fill loop that stops at the first chunk returns a short page, which the seam
  then labels `next: None`, and the caller stops walking a prefix that is not exhausted
  left: 134
 right: 600
test result: FAILED. 3 passed; 1 failed
```

…while in the **same run** `trait_contract_against_fdb` (the whole shared suite, `run_all` +
`run_all_cap_scoped`) passed, as did the pre-existing `scan` legs. That is the adversary's
claim reproduced and then closed: before this iteration nothing at any tier saw it; now one
binary does, in the maintainer leg. Reverted and re-run: 4 passed, leg exit 0.

**(ii) The new violating double, in the default gate.** `StoppedFillStore`
(`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:1407`) is faithful in every
other respect — correct order, exclusive cursor, correct values, and it derives `next` from the
page it actually built — and stops its fill one pair short of the bound. Three `#[should_panic]`
tests assert the clauses catch it (`:1460` termination, `:1470` the page bound, `:1482` the cap
escape) and a fourth asserts it still passes all four pre-existing sequential clauses plus the
zero-bound refusal (`:1494`). Its counterpart `ShortPagedStore` (a *conforming* store that also
returns short pages, but carries the cursor) still passes every clause — the pair is what shows
the suite discriminates rather than just rejects short pages. Suite went 25 → **29 tests**.

**The gate's own C4-verify RED leg is still a *build* error, and the brief requires me to say so
plainly.** `./engine/scripts/run-verify.sh` → **PASS**:

* GREEN (fix applied): `-p wyrd-metadata-conformance --test scan_page_demonstrated_red -p
  wyrd-metadata-redb --test scan_page` → **29 passed** + **10 passed**, 0 failed.
* RED (production reverted, tests kept): **86 compile errors, zero tests executed** —
  `error[E0407]: method scan_page is not a member of trait MetadataStore`, `unresolved imports
  wyrd_traits::{page_cursor, page_limit, page_start, …}`, `cannot find function
  contract_scan_page_*`. A run that executed **no tests** is a non-result; `run-verify.sh` scores
  it PASS because the `TESTS_RAN == 0` guard sits inside the cargo-succeeded branch
  (`engine/scripts/run-verify.sh:416-427`, `:433`). Treat (i) and (ii) above as the binding
  evidence and the C4-verify PASS as corroboration.

### (b) Production path? **Yes.**

The new legs drive the real `FdbMetadataStore` against a live cluster and the real
`TikvMetadataStore` against a real one when a maintainer has it; the shared clauses drive the
real `RedbMetadataStore` (`crates/metadata-redb/tests/scan_page.rs`, `tests/conformance.rs`),
both DST sim stores in-simulator (`crates/dst/tests/conformance.rs`, green inside `xtask ci`'s
`run_dst` row), and FDB via `trait_contract_against_fdb`. `page_is_full` is production code in
`wyrd-traits`, called from all five bodies. The doubles are additional non-vacuity evidence,
never the only evidence.

### (c) Fixture includes the fault? **Yes.**

The FDB fixture is 600 × 512 B keys — *grounded* against the live server, which is asserted to
report `more()` on the range (`crates/metadata-fdb/tests/scan.rs:528`, called at `:184`, `:349`), so the page really is
assembled by the loop rather than satisfied in one read; the red measurement (134 of 600)
proves the boundary is inside the fixture and not curated out. The TiKV fixture asserts it
spans more than one `PAGE_SIZE` chunk and ends raggedly (`crates/metadata-tikv/tests/scan.rs:160`).
The cap-escape leg still asserts `scan` genuinely fails loud first, so a cap-lowering hook that
lowered nothing cannot leave it vacuous.

## 4. Gates run in `$PDCA_WORKTREE`

| Run | Result |
|---|---|
| `./engine/xtask.sh ci` (typos, docs lint + render, guards, fmt, clippy `-D warnings`, build, `cargo test --workspace --exclude wyrd-dst`, machete, cargo-deny, conformance vectors, statics, orchestrator guard, **DST 50 seeds**) | **exit 0 — "xtask ci: all checks passed"** (run three times; final tree twice) |
| ↳ in-simulator conformance (`run_dst` → `crates/dst/tests/conformance.rs`) | `sim_fdb_backend_passes_shared_contract`, `sim_tikv_backend_passes_shared_contract`, `redb_backend_passes_shared_contract` — 3 passed |
| `./engine/scripts/run-verify.sh` (C4-verify) | **PASS** (29 + 10 green; red = build error, 0 tests — §3a) |
| `scripts/mutants-in-diff` (C5, advisory) | **17 missed, 28 caught, 38 unviable** (was 18/28/31) |
| `./engine/xtask.sh fdb-conformance` (throwaway Docker single-node FDB) | **exit 0** — including the two new at-scale `scan_page` legs |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features fdb,etcd --tests` | exit 0 |
| `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features tikv,etcd --tests` | exit 0 |
| `cargo test -p wyrd-metadata-tikv --features tikv --lib` | 29 passed |

**C5, and what the remaining 17 are.** Every mutant that was a *decision* is now killed rather
than excluded — no entry was added to `.cargo/mutants.toml`:

| Mutant | v5 | now |
|---|---|---|
| `metadata-fdb/src/lib.rs … replace >= with < in scan_page_once` | MISSED | **CAUGHT** as `traits/src/lib.rs:546 … in page_is_full` |
| `metadata-tikv … the page fill decisions` | did not exist (single-shot read) | **CAUGHT** as `metadata-tikv/src/lib.rs:554 … in paging::after_chunk` |

The 17 survivors are **10 in `crates/metadata-fdb/src/lib.rs` and 7 in
`crates/metadata-tikv/src/lib.rs`, every one of them inside `#[cfg(feature = …)] mod store`**
(fdb `:947`, tikv `:734`) — whole-body replacements of substrate I/O (`scan_page_once`,
`scan_page`, `scan`, `connect`, `with_scan_cap`), FDB's `is_retryable` retry arm, and its
streaming `iteration += 1` counter. The C5 row runs `cargo mutants --in-diff` with **default**
features (`wyrd-pdca/scripts/mutants-in-diff`, last line), so that code is never compiled:
cargo-mutants logs each of the 17 as `in 0s build + 0s test`. Measured rather than asserted in
iteration 5 (a deliberate type error inside `FdbMetadataStore::scan_page_once` builds fine with
default features and fails `--features fdb`), and the repo's own `.github/workflows/mutants.yml`
draws the same line by listing packages and omitting both metadata backends.

What I did **not** do, and its cost:

* **Exclude the two `mod store` bodies in `.cargo/mutants.toml`** — ~6 lines, and the row reads
  0 missed. Rejected: that file is for *equivalent* mutants verified by hand, and editing a
  repo's mutation config so a gate reads green is the move a reviewer should reject.
* **Extract FDB's `iteration += 1` counter into a default-compiled helper** to kill 2 more —
  it is FDB's streaming-mode iteration number and the sibling `scan_once` loop (untouched,
  pre-existing) spells it the same way; a helper existing only to satisfy a mutation counter
  would make the two loops read differently for no behavioural reason. The behavioural risk is
  covered instead by the at-scale leg, which fails if the loop stops early for *any* reason.
* **Ask the C5 gate to run `--features fdb,tikv`** — not mine to change (it is
  `wyrd-pdca/scripts/mutants-in-diff`, not the target repo), and it would make every future C5
  run require `libfdb_c` + FDB headers on the host.

## 5. Self-review against the target's `## Review rubric & protocol` (root `AGENTS.md`)

* *One clock per lifecycle* — no clock read added or moved; the FDB test file's existing
  wall-clock exemption (fresh-namespace uniqueness across runs) is unchanged.
* *Narrow trait seams / dependency direction* — no new trait surface: `page_is_full` is a free
  function in the seam crate, and `metadata-tikv` already depends on `wyrd-traits` (it
  re-exports `SCAN_CAP` from it).
* *No DST-reachable shared mutable global state* — none added (the statics gate is green).
* *Docs currency* — this delta adds no port, API operation, RPC, CLI flag or persisted field.
* *Transactions: roll back before any early return over a live transaction* — the TiKV fill
  loop's error arm still goes through `rollback_then`, and the success path rolls back
  explicitly before returning.
* *Await discipline: every await on external work is bounded* — the whole multi-chunk loop runs
  inside the same `under_deadline` envelope `scan` uses.
* *Absent or unsupported entries: never a count-based assertion that can pass while the property
  fails* — the at-scale legs assert counts **and** the exact per-key value, byte order, and
  no-duplicate properties; the count alone is never the assertion.
* *Test fidelity: conformance contracts run on every backend* — no clause was added or forked,
  so `run_all`/`run_all_cap_scoped` carry everything to redb, both sim stores, FDB and TiKV with
  no per-driver list touched.
* *Deferrals are settled* — the TiKV live-cluster item stays closed; §2 states the new code's
  verification status as fact, and raises nothing.

## 6. Scratch

Gate logs lived in `${PDCA_SCRATCH}/pdca-builder-634-*.log` and are removed. `mutants.out/` in
`$PDCA_WORKTREE` is cargo-mutants' own gitignored output from the C5 row. No temporary git
worktrees or containers were left behind (the FDB compose project is torn down by the xtask leg
itself — verified in the log tail).

Reproduce the §3a(i) measurement from the bundle, in `$PDCA_WORKTREE` with this patch applied:

```sh
# invert the fill loop's fullness test in production, and nothing else
sed -i 's/if page_is_full(out.len(), limit)/if !page_is_full(out.len(), limit)/' \
    crates/metadata-fdb/src/lib.rs
./engine/xtask.sh fdb-conformance     # -> a_scan_page_that_spans_several_reply_chunks… FAILED, 134 vs 600
git checkout crates/metadata-fdb/src/lib.rs
```
