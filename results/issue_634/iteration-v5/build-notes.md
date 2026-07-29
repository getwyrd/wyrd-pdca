# Build notes — issue 634 / scan-page-seam (iteration 5)

*Withheld from the reviewer; written for the human at sign-off.*

Every citation below is `path:line` in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`, = `origin/main` @ `22d71b4` + this patch).

## 0. What this iteration is

Iteration 4 passed every gating row (`C4-ci`, `C4-verify`, `T4` 0 blocking) and auto-iterated
on two things: the **advisory C5 mutants row** (22 missed of 84) and a §6 item that is not a
code defect (a maintainer deciding whether to trust the batched-review gate). So the production
seam is *not* what this iteration reopens — `scan_page` on redb/FDB/TiKV/the two sim stores is
byte-identical to the reviewed patch, apart from one function body **reordered** (§4, item 1).

What it does is close the one thing iteration 4's *advisory adversary* actually refuted — a
verified hole in the conformance suite this slice exists to write — plus the two smaller
findings beside it, plus the C5 row. Four items, ~+368 lines over six files:

| Item | Where | +/− vs iteration 4 |
|---|---|---|
| 1. A silently-skipping backend passed every clause (the invariant this slice restores) | `metadata-conformance/{src/lib.rs,tests/scan_page_demonstrated_red.rs}` | +97 / +140 |
| 2. A false structural claim in the trait + testkit docs | `traits/src/lib.rs`, `testkit/src/lib.rs` | +17, ~rewritten paragraph |
| 3. `ZeroPageLimit` crossed the seam unclassified and unpinned | `traits/src/lib.rs`, `metadata-redb/src/lib.rs` | +38 (redb), +14 (traits) |
| 4. C5 survivors that were *killable* | `traits/src/lib.rs`, `metadata-conformance/src/lib.rs`, `metadata-tikv/src/lib.rs` | +42 (tikv), rest inside the above |

---

## 1. Item 1 — the suite accepted a store that silently skips keys

**The defect.** A backend that resumes a page at the cursor's *arithmetic successor* — the
cursor with its last byte incremented, which is what you reach for when your range API offers
only an **inclusive** lower bound — starts each page strictly after the cursor (so it satisfies
"exclusive cursor" as the suite was asserting it) and **steps over every key that has the cursor
as a prefix**, on every lap, forever. `p:a` → resume at `p:b` → `p:a0` is never returned.

That is exactly the brief's *Invariant to restore* ("may never silently omit a key that was
present throughout the walk") failing while all seven new clauses pass. The clause fixtures did
contain the strict-prefix pair (`p:a`/`p:a0`, `crates/metadata-conformance/src/lib.rs:525`), but
they were only ever read at `limit` 2 and 16, where the page boundary falls elsewhere — so the
pair proved *ordering inside a page* and nothing about *continuation across one*.

**The fix is two assertions, not a new clause** (the clauses are 0016's, and adding a fifth
would be inventing contract):

* `crates/metadata-conformance/src/lib.rs:642-654` — clause (b)'s fixture gains **`p:200`**, a
  strict extension of the cursor `p:20` used by case (i), so the "cursor that exists" case now
  has an immediate successor that *extends* it. `:677-692` asserts the page resumes at the
  immediate next key.
* `crates/metadata-conformance/src/lib.rs:583-600` — clause (a) walks the same population a
  third way, **one key per page**, which puts a boundary between *every* adjacent pair
  including `p:a`/`p:a0`.

**Non-vacuity — the new violating double.** `BadSuccessorStore`
(`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:651-790`) is faithful in every
other respect and resolves its cursor with `bad_successor` (`:679-694`). Three tests:
`bad_successor_store_fails_the_exclusive_cursor_clause` (`:733`),
`bad_successor_store_fails_the_order_clause_one_key_per_page` (`:743`), and
`bad_successor_store_passes_every_clause_whose_boundary_never_lands_on_a_prefix` (`:754`) — the
last is the measurement that matters: **every other clause's seeded population is fixed-width**
(`{i:04}`, `{i:06}`, `walk:c{i}`, `p:{i}`), so no seeded key is a strict prefix of another and
this store passes all of them, plus the four pre-existing sequential clauses, plus the
cap-escape and zero-bound clauses. (Clause (d) does create one strict extension mid-walk — its
"inserted ahead of the cursor" key, which this store duly steps over — but that key is the one
`0016:2659-2661` explicitly leaves unconstrained, so the clause accepts either outcome and
passes.) Two assertions are the whole difference between shipping this backend and catching it.

**Measured red (§5a below): with only those two clause changes reverted, the suite is 23 passed
/ 2 failed — both `#[should_panic]` targets report "did not panic as expected".**

**Where the risk actually lives.** TiKV is the one backend that resolves the cursor by
arithmetic (`paging::next_page_start`, `crates/metadata-tikv/src/lib.rs:435`, which appends
`0x00` — the smallest strict extension — and is correct today), and TiKV is also the one whose
conformance run is off-Check. So I did not leave that backend's proof to the off-Check leg
alone: `crates/metadata-tikv/src/lib.rs:494-528` is a new **default-compiled** property test —
for five cursors × five suffixes, `next_page_start(cursor) <= cursor||suffix`, plus a
discriminating half showing the naive increment sorts *past* `cursor||0x00`. It runs in
`cargo xtask ci` and needs no cluster. FDB uses `KeySelector::first_greater_than`
(`crates/metadata-fdb/src/lib.rs:1442-1446`), a substrate primitive with nothing to unit-test,
and its behaviour under the strengthened clauses is proven on a **live cluster** (§4).

## 2. Item 2 — the trait's structural claim about the test-double helper was false

The reviewed patch said `test_double_scan_page` "lives in the **dev-only** testkit crate so no
production backend can reach it at all … a dev-dependency everywhere, never a dependency"
(`crates/traits/src/lib.rs`) and "a production `MetadataStore` body naming this function does not
compile" (`crates/testkit/src/lib.rs`). Checked, and it is not true as written:

* `wyrd-testkit` is a **regular** dependency of `wyrd-coordination-mem`
  (`crates/coordination-mem/Cargo.toml:16`, under `[dependencies]` at `:11`) and of
  `wyrd-metadata-fault-conformance` (`crates/metadata-fault-conformance/Cargo.toml:20`);
* `wyrd-coordination-mem` is a regular dependency of `wyrd-server`
  (`crates/server/Cargo.toml:58`) — so the helper is linked into the shipped binary.

The *narrow* claim is true and is the real backstop: the three metadata backends take
`wyrd-testkit` as a dev-dependency only (`crates/metadata-redb/Cargo.toml:24`,
`crates/metadata-fdb/Cargo.toml:51`, `crates/metadata-tikv/Cargo.toml:46`). So the docs now say
exactly that, split into "where it is a build error" and "where it is only a convention"
(`crates/testkit/src/lib.rs:772-791`), with the trait pointing at it rather than repeating the
overstatement (`crates/traits/src/lib.rs:1066-1072`). A future backend author will rely on this
sentence; an overstated guarantee is worse than an honest one.

I did **not** add `#[doc(hidden)]`: ~34 test doubles across 26 files name this function, and
hiding the one place its contract is written makes it *more* likely someone re-rolls a wrong
body.

## 3. Item 3 — a new seam error that no test classified

`ZeroPageLimit` is a fault a **production** backend now raises (`crates/metadata-redb/src/lib.rs`
via `page_limit`), yet it was absent from the classifier's normative table and had no
classification test — while the repo's convention is explicit
(`crates/metadata-redb/src/lib.rs:250-256`: "each drives a real fault this backend actually
produces and asserts it classifies terminal, never transient (#591)"). The behaviour was already
right (it reaches `Terminal` through `classify`'s fail-safe default); what was missing is the pin
that stops a later refactor turning a permanent refusal into a retry loop. Added:

* the table row + the "last five rows are one arm" count (`crates/traits/src/lib.rs:727,730`);
* the seam-level pin (`crates/traits/src/lib.rs:1367-1379`, inside the existing
  `the_permanent_faults_classify_terminal`);
* the backend-level one, driven through the real store rather than a constructed error —
  `a_zero_page_bound_classifies_terminal`, `crates/metadata-redb/src/lib.rs:294-330`.

## 4. Item 4 — the C5 advisory row, and what is left in it

**Result: 22 missed → 18 missed** (`77 mutants tested in 33s: 18 missed, 28 caught, 31
unviable`), and the composition is now clean:

| Package | v4 missed | v5 missed | Why |
|---|---|---|---|
| `wyrd-traits` | 1 | **0** | §3a below — killed, not excluded |
| `wyrd-metadata-conformance` | 3 | **0** | 2 killed by a new unit test, 1 site removed |
| `wyrd-metadata-redb`, `wyrd-testkit` | 0 | 0 | — |
| `wyrd-metadata-fdb` | 11 | 11 | inside `#[cfg(feature = "fdb")] mod store` |
| `wyrd-metadata-tikv` | 7 | 7 | inside `#[cfg(feature = "tikv")] mod store` |

**The four that were killable, and how:**

1. `traits/src/lib.rs:499:32 replace > with >=` in `page_start`. It was an **equivalent** mutant
   in the old arm order (`cursor == prefix` was consumed by the `starts_with` arm above it, so
   no input distinguished `>` from `>=`) — which is exactly what
   `.cargo/mutants.toml`'s `exclude_re` list exists for. I did **not** take that route: the same
   file says "a killable survivor gets a regression test instead — it never belongs here", and
   the function can be *written* so the boundary is observable. `page_start` now decides in
   keyspace order — below the prefix, inside it, above it, which is also `PageStart`'s own
   declaration order (`crates/traits/src/lib.rs:494-518`). The comparison is now `cursor <
   prefix`, whose boundary case (`cursor == prefix`) is reachable and already asserted at
   `:1528-1537`. Verified: `cargo mutants -f crates/traits/src/lib.rs --package wyrd-traits -F
   'page_start|page_limit|page_cursor'` → **12 mutants: 5 caught, 7 unviable, 0 missed** (it was
   1 missed before). No config exclusion was added anywhere in this patch.
2. + 3. `metadata-conformance/src/lib.rs:428:36/:44` in `escaped`, the per-byte renderer every
   clause reports failures through. Its documented property — reversible, per byte, so `0x80`
   and `0xff` stay distinguishable in the one clause that seeds both — is now asserted rather
   than trusted (`crates/metadata-conformance/src/lib.rs:1517-1568`).
4. `metadata-conformance/src/lib.rs:941:44 replace > with >=`, picking clause (d)'s
   delete-ahead-of-the-cursor key. The advisory adversary's note was right that `>=` could let
   the fixture delete a key the walk had already returned, weakening the mutation without
   failing the clause. The `.rev().find(…)` search is gone; the fixture now takes the greatest
   control key and **asserts** it is ahead of the cursor (`:983-996`), so the premise is proven
   rather than searched for. Verified: `cargo mutants … -F 'escaped|rendered|no_skip'` → **14
   mutants: 8 caught, 6 unviable, 0 missed**.

**The 18 that remain are not a test gap — proven, not asserted.** Both `store` modules are
feature-gated (`crates/metadata-fdb/src/lib.rs:946-947`, `crates/metadata-tikv/src/lib.rs:594-595`)
and the C5 gate runs `cargo mutants --in-diff` with **default** features
(`wyrd-pdca/scripts/mutants-in-diff`, last line), so the mutated code is never compiled. I
measured it rather than claiming it — a deliberate type error inserted inside
`FdbMetadataStore::scan_page_once`:

```
$ cargo build -p wyrd-metadata-fdb                  # default features
DEFAULT_BUILD_EXIT=0
$ cargo check  -p wyrd-metadata-fdb --features fdb
error[E0308]: mismatched types
FEATURE_CHECK_EXIT=101
```

A mutation there cannot change anything the default-featured suite observes, so it is reported
MISSED by construction (cargo-mutants agrees: every one of the 18 is logged `in 0s build + 0s
test`). The repo's own mutation policy already draws this line: `.github/workflows/mutants.yml`
selects packages explicitly (`:96-103`, `:163-170`) and **neither metadata backend is in the
list**, precisely because they are feature-gated. The PDCA C5 row does not apply those
`--package` filters, which is why it sees them at all.

**What I deliberately did not do**, and the cost of each:

* **Add `exclude_globs` for the two `mod store` bodies to `.cargo/mutants.toml`.** ~6 lines and
  the row would read 0 missed. Rejected: that file is documented for *equivalent* mutants
  verified by hand, these are unevaluated ones, and editing a repo's mutation config so a gate
  reads green is exactly the move a reviewer should reject. The honest number plus this section
  is worth more than a green row.
* **Move the fdb/tikv page loops into default-compiled helpers** so the arithmetic becomes
  mutation-visible. Partly already true — the three page-bound decisions are shared seam
  functions with 0 surviving mutants, and TiKV's cursor arithmetic is `paging::next_page_start`,
  default-compiled and now property-tested (§1). What is left inside the gate is substrate I/O:
  FDB's `trx.get_range` loop and retry arm (`:1433-1477`, `:1891-1936`) and TiKV's
  `txn.scan`/deadline wrapper (`:1126-1180`). Extracting those would mean inventing a fake
  range-read seam to test against — a mock in place of the substrate, which is worth less than
  the live-cluster run in §4 and costs ~120 lines of indirection in two production backends.
* **Ask the C5 gate to run with `--features fdb,tikv`.** Not mine to change (it is
  `wyrd-pdca/scripts/mutants-in-diff`, not the target repo), and it would make every future
  C5 run require `libfdb_c` + the FDB headers on the host.

---

## 5. Red → green, through the project's own runners

### (a) Genuine red? **Yes — and this iteration adds a real, *semantic* red.**

Reverting **only** the two clause strengthenings of §1 (production code untouched, both new test
files kept) and re-running the project's runner:

```
$ cargo test -p wyrd-metadata-conformance --test scan_page_demonstrated_red
---- bad_successor_store_fails_the_exclusive_cursor_clause stdout ----
note: test did not panic as expected at …/scan_page_demonstrated_red.rs:733:4
---- bad_successor_store_fails_the_order_clause_one_key_per_page stdout ----
note: test did not panic as expected at …/scan_page_demonstrated_red.rs:743:4
test result: FAILED. 23 passed; 2 failed
```

That is the adversary's finding turned into a standing test: without the two added assertions a
store that silently drops `p:a0` and `p:200` **passes the entire suite**. With them: `25 passed;
0 failed`. The mutation runs in §4 are the second red→green measurement (1 missed → 0 in the
seam helpers).

The two targets are at `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:733`
and `:743` on the shipped tree, the same lines the transcript names.

**The gate's own C4-verify RED leg is still a *build* error, and the brief requires me to say
so plainly.** `./engine/scripts/run-verify.sh` → **PASS**:

* GREEN (fix applied): `-p wyrd-metadata-conformance --test scan_page_demonstrated_red -p
  wyrd-metadata-redb --test scan_page` → **25 passed** + **10 passed**, 0 failed.
* RED (production reverted, tests kept): **79 compile errors, zero tests executed** —
  `error[E0407]: method scan_page is not a member of trait MetadataStore`, `unresolved imports
  wyrd_traits::{page_cursor, page_limit, page_start, PageStart, ScanPage}`, `cannot find function
  contract_scan_page_*`. A run that executed **no tests** is a non-result; `run-verify.sh` scores
  it PASS because the `TESTS_RAN == 0` guard sits inside the cargo-succeeded branch
  (`engine/scripts/run-verify.sh:416-427`, `:433`). Treat the demonstrated-red file — and
  specifically the measurement above — as the binding evidence, and the C4-verify PASS as
  corroboration.

### (b) Production path? **Yes.**

The strengthened clauses are called on the real `RedbMetadataStore`
(`crates/metadata-redb/tests/scan_page.rs:330-346` and `tests/conformance.rs`), on both DST sim
stores **in-simulator** (`crates/dst/tests/conformance.rs`, green inside `xtask ci`'s `run_dst`
row), and on a **live FoundationDB cluster** — `cargo xtask fdb-conformance` exit 0 with
`trait_contract_against_fdb` passing, which drives both `run_all` and `run_all_cap_scoped`
(`crates/metadata-fdb/tests/conformance.rs:136`, `:155`) including the two new assertions. The
doubles are additional non-vacuity evidence, never the only evidence.

### (c) Fixture includes the fault? **Yes.**

The skipped key is *in* the fixture, not curated out: `p:200` is seeded in clause (b) and
`p:a0` in clause (a), and both are what `BadSuccessorStore` fails to return. The cap-escape
clause still asserts `scan` genuinely fails loud with `ScanCapExceeded { cap }` before walking
(so a cap-lowering hook that lowered nothing cannot leave it vacuous), and clause (d) still
commits real mid-walk mutations and proves through `get` that they landed.

## 6. Gates run in `$PDCA_WORKTREE`

| Run | Result |
|---|---|
| `./engine/xtask.sh ci` (typos, docs lint + render, guards, fmt, clippy `-D warnings`, build, `cargo test --workspace --exclude wyrd-dst`, machete, cargo-deny, conformance vectors, statics, orchestrator guard, **DST 50 seeds**) | **exit 0 — "xtask ci: all checks passed"** (run three times: mid-way, after the TiKV property test, and on the final tree) |
| ↳ in-simulator conformance (`run_dst` → `crates/dst/tests/conformance.rs`) | `sim_fdb_backend_passes_shared_contract`, `sim_tikv_backend_passes_shared_contract`, `redb_backend_passes_shared_contract` — 3 passed |
| `./engine/scripts/run-verify.sh` (C4-verify) | **PASS** (details in §5a) |
| `scripts/mutants-in-diff` (C5, advisory) | 77 mutants: **18 missed** (all feature-gated, §4), 28 caught, 31 unviable — was 22 missed |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features fdb,etcd --tests` | exit 0 |
| `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features tikv,etcd --tests` | exit 0 |
| `cargo test -p wyrd-metadata-tikv --features tikv --lib` (the new cursor property test with the feature ON) | 26 passed, 0 failed |
| `./engine/xtask.sh fdb-conformance` (throwaway Docker single-node FDB) | **exit 0** — "FoundationDB passed the shared MetadataStore conformance suite and the contention properties" |

**TiKV** stays exactly as the human adjudicated it at the iteration-3 sign-off (accepted as
backseat while redb/FDB stay green): the available cluster answers `InvalidKeyMode {
storage_api_version: V2 }` during the first commit, before `scan_page` is reached. I am
deliberately **not** re-raising it as a NEEDS-HUMAN marker — the sign-off closed it and asked
not to re-litigate. What is new is that TiKV's cursor arithmetic no longer depends on that leg
alone (§1, `crates/metadata-tikv/src/lib.rs:494-528`, in the default gate).

No new external dependency was needed: `docker`, `libfdb_c`, the FDB headers, `typos` and the
docs renderer were all present, as Plan recorded.

## 7. Self-review against the target's `## Review rubric & protocol` (root `AGENTS.md`)

* *One clock per lifecycle* — no clock read added or moved.
* *Narrow trait seams / dependency direction* — no new trait surface; `page_start` changed shape,
  not signature or semantics; `testkit → traits` only.
* *No DST-reachable shared mutable global state* — none added (the statics gate is green).
* *Docs currency* — no port/API/RPC/CLI flag/persisted field added by this delta; the two doc
  changes it does make are corrections (§2, §3).
* *Absent or unsupported entries: "never … a count-based assertion that can pass while the
  property fails"* — this is the class §1 closes, and the added assertions compare exact
  `(key, value)` sequences, never counts.
* *Test fidelity: conformance contracts run on every backend* — the two new assertions ride
  inside existing clauses, so `run_all` carries them to redb, both sim stores, FDB and TiKV with
  no per-driver list touched.
* *Deferrals are settled* — the TiKV live-cluster item is left closed, not re-raised.

## 8. Scratch

Everything transient lived under `${PDCA_SCRATCH}/pdca-builder-634-redleg` (gate logs, the
green copies used for the red-leg revert and the cfg experiment) and is removed. No temporary
git worktrees were created; `mutants.out/` in `$PDCA_WORKTREE` is cargo-mutants' own,
gitignored output from the C5 row.

Reproduce the §5a measurement from the bundle, in `$PDCA_WORKTREE` with this patch applied:

```sh
# drop ONLY the two clause strengthenings, keep everything else
#   - remove b"p:200" from the clause-(b) fixture (src/lib.rs:642-654) and put case (ii) back to &seeded[2..]
#   - delete the one-key-per-page walk (src/lib.rs:583-600)
cargo test -p wyrd-metadata-conformance --test scan_page_demonstrated_red   # → 23 passed; 2 failed
```
