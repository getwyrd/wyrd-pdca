# Build notes — issue 634 / scan-page-seam (iteration 4)

*Withheld from the reviewer; written for the human at sign-off.*

## 0. Scope of this iteration

The iteration-3 sign-off rejected **solely** on the two live T4 findings in
`review-batch.md`, both in the new `scan_page` **conformance test code** this slice adds —
not in the production backends — and explicitly cleared every other §6 item ("do not
re-litigate those; TiKV is accepted as backseat as long as redb/FDB stay green, and the C4
exit-101 gate was independently reproduced clean (flake)").

So this iteration is **iteration-3's patch plus a delta confined to the shared clauses and
their demonstrated-red doubles**. No production backend body changed by one byte:
`crates/{traits,metadata-redb,metadata-fdb,metadata-tikv}/src/lib.rs`,
`crates/testkit/src/lib.rs` and `crates/dst/tests/support/mod.rs` are **identical** to the
patch that was reviewed. The delta, measured against `iteration-v3/patch.diff`:

| File | +/− vs iteration 3 |
|---|---|
| `crates/metadata-conformance/src/lib.rs` | +360 / −151 |
| `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs` | +196 / −7 |
| `crates/metadata-redb/tests/scan_page.rs` | +30 / −19 |
| `crates/metadata-conformance/Cargo.toml` | +4 / −1 |
| everything else (23 files) | 0 |

---

## 1. Finding 1 — the clauses never asserted the returned **values**

> `crates/metadata-conformance/src/lib.rs:465` **TEST-GAP**: every new `scan_page` clause
> discards or ignores returned values, so a backend returning correct keys and cursors with
> stale, swapped, or corrupted `Bytes` would pass shared conformance.

Correct, and it mattered more than "an assertion is missing": the whole clause set was
written around the *cursor*, and a cursor is a key, so every fixture had drifted into
`map(|(k, _)| k)`. The two consumers this seam exists for — the `retire:` drain and GC's
`orphan:` ledger (#636/#637) — **decode** what a page hands back, so a page of right keys
carrying wrong bytes is the same unbounded retention a skip causes, arriving through the
value.

**Fix (`crates/metadata-conformance/src/lib.rs`).**

* `walk` now returns `Vec<(Vec<u8>, Bytes)>` rather than keys (`:397-420`), so *every*
  walk-based assertion in the suite compares values.
* Two shared assertions carry the property once, with a readable message
  (`escaped`/`rendered` at `:425-445` render bytes as `p:\x80`, because a lossy decode maps
  both `0x80` and `0xff` to U+FFFD — indistinguishable in the very clause that seeds both):
  * `assert_pairs_eq` (`:454-464`) — exact `(key, value)` sequence;
  * `assert_page_is_next_of` (`:480-508`) — one page against the pairs that remain at its
    cursor (see §2; it is also the short-page fix).
* Every clause that returns content now seeds **distinct values** and asserts them:
  order (`:524`), exclusive cursor (`:614`, values `v-p:10` … so a neighbour's bytes are
  visible), termination (`:757`), no-skip (`:838`, per stable key: "returned exactly once"
  **and** carrying the value it was committed with), page bound (`:1079`), cap escape
  (`:1157`). The zero-page-bound clause (`:1245`) returns no page at all — a refusal — so
  there is nothing to assert there.
* `crates/metadata-redb/tests/scan_page.rs:60-93` — the backend-local `seed`/`walk` helpers
  carry values too, so leg B's cap-escape walk asserts pairs, not keys.

**Non-vacuity (this is the part that makes the fix real).** A new dev-scope double,
`KeysOnlyStore` (`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:468-576`):
order, cursors, page bounds and termination all exactly correct, values filled in as
`Bytes::new()`. It is not a strawman — a keys-only range read is a real substrate call
(`tikv_client::Transaction::scan_keys`) and the natural one to reach for when what you are
building is a cursor. Two `#[should_panic]` tests assert clause (a) and clause (d) each
**catch** it; a sibling test asserts it still passes the four pre-existing sequential
clauses and the zero-page-bound clause, which is what shows the value assertion (not
something else) is doing the catching.

## 2. Finding 2 — a fixture that failed a **conforming** store

> `crates/metadata-conformance/src/lib.rs:772` **TEST-GAP**: the fixture assumes the first
> page fills its two-item limit, so a conforming store that returns `[walk:c0]` with
> `next = Some(walk:c0)` — a short non-final page explicitly permitted by this contract —
> fails this assertion.

Also correct, and the worse of the two: a suite that rejects a legal store tells a backend
author the contract says something it does not. The contract bounds `items.len()` from
**above** and constrains `next`; it never obliges a store to fill the page (`0016:2657-2658`;
FDB's `more()` and a region boundary both produce short non-final pages in practice).

Auditing for the *class* rather than the cited line found the same assumption at **five**
more sites, all now fixed:

| Site (iteration-3 numbering) | The assumption |
|---|---|
| clause (a) `:464` | one page of limit 16 must return the whole 6-key population |
| clause (a) `:469` | "a page shorter than the limit must report `next: None`" — flatly stronger than the contract |
| clause (b) `:637`, `:650`, `:661`, `:680` | four fixed-page equality assertions |
| page bound `:1075` | `page.len() == 2` |
| page bound `:1088` | `page.len() == 5` for `limit = usize::MAX` |
| clause (d) `:771-782` | mutation keys placed where a **full** first page would have left the cursor |

**Measured, not asserted.** I drove each of the seven clauses **separately** against the new
`ShortPagedStore` with iteration-3's `src/lib.rs` restored, and recorded which reject a
conforming store (the split was temporary; the shipped file keeps one test):

```
test probe_order      ... FAILED   panicked at src/lib.rs:464   (whole population out of one page)
test probe_cursor     ... FAILED   panicked at src/lib.rs:539   (fixed-page equality on the resumed page)
test probe_no_skip    ... FAILED   panicked at src/lib.rs:771   (the cited finding)
test probe_limit      ... FAILED   panicked at src/lib.rs:904   (`page.len() == 2`)
test probe_terminates ... ok       test probe_cap_escape ... ok   test probe_zero_bound ... ok
test result: FAILED. 3 passed; 4 failed
```

So the cited line was **one of four**, not one of one — which is why the fix is the class,
not the line. The three that already tolerated it are the ones that *walk* rather than
assert on a single page.

**Fix.** `assert_page_is_next_of` (`:480-508`) replaces every fixed-page equality: the page
must be the next `page.len()` pairs of the expected remainder — so nothing is skipped or
re-ordered — and it may stop short **only** by carrying a cursor; `next: None` still means
exhausted and therefore must have returned everything. Clause (d) (`:838-1046`) now
**derives** each mutation key from the cursor the walk actually reached:

* behind the cursor = the last key returned with its final byte dropped (a strict prefix, so
  strictly less, whatever came back), plus a `get` assertion that it is genuinely an insert
  and not a collision with a seeded key;
* deleted ahead = the greatest control key the walk has not yet reached;
* inserted ahead = the last key returned with a suffix appended (always sorts after it).

Each derivation is asserted to be on the side of the cursor the clause claims, so the
fixture proves its own premise instead of eyeballing it.

**Non-vacuity.** A second new double, `ShortPagedStore`
(`tests/scan_page_demonstrated_red.rs:299-393`) — conforming, but returns **one pair per
page** whatever the caller's limit, terminal only when the prefix really is exhausted:
exactly the `[walk:c0]` + `next = Some(walk:c0)` shape the finding describes. The test
`a_short_paging_store_passes_every_new_clause_too` asserts every clause passes against it.

## 3. Red → green, run through the project's own runner

**(a) Genuine red? Yes — and by *assertion*, not by compile error.** With
`crates/metadata-conformance/src/lib.rs` restored to its **iteration-3** content (the two
new test files kept):

```
cargo test -p wyrd-metadata-conformance --test scan_page_demonstrated_red
test result: FAILED. 19 passed; 3 failed
  a_short_paging_store_passes_every_new_clause_too
      panicked at src/lib.rs:464: assertion `left == right` failed:
      one page of the whole prefix must come back in raw byte order
        left: [[112, 58, 97]]   right: [[112,58,97], [112,58,97,48], … 6 keys]   ← finding 2
  keys_only_store_fails_the_order_clause_on_its_values   note: test did not panic as expected ← finding 1
  keys_only_store_fails_the_no_skip_clause_on_its_values note: test did not panic as expected ← finding 1
```

With the fix: `22 passed; 0 failed`. That is the semantic red the brief's *Falsifiability*
section demands (obligation 2): **22 tests ran** and 3 failed — one on an assertion *inside*
a clause (the suite rejecting a conforming store), two because the clause under test did
**not** panic where it must (the missing value assertion) — in the same `cargo test` the gate
runs. A compile error can never be that.

**The gate's own C4-verify RED leg is a *build error*, and I am recording it as the brief
requires (obligation 1).** `./engine/scripts/run-verify.sh` → **PASS**:

* GREEN (fix applied): `-p wyrd-metadata-conformance --test scan_page_demonstrated_red
  -p wyrd-metadata-redb --test scan_page` → **22 passed** + **10 passed**, 0 failed.
* RED (production reverted, tests kept): **69 compile errors**, zero tests executed —
  `error[E0407]: method scan_page is not a member of trait MetadataStore` ×11,
  `unresolved imports wyrd_traits::{page_cursor, page_limit, page_start, PageStart, ScanPage}`,
  `cannot find function contract_scan_page_*`. A run that executed **no tests** is a
  non-result: the gate scores it PASS because `run-verify.sh`'s `TESTS_RAN == 0` guard sits
  inside the cargo-succeeded branch. Treat §D (the doubles) as the binding demonstration and
  the C4-verify PASS as corroboration — exactly as the brief instructs.

**(b) Production path? Yes.** The clauses are called on the real `RedbMetadataStore`
(`crates/metadata-redb/tests/scan_page.rs:330-346`, `tests/conformance.rs`), on the two DST
sim stores in-simulator (`crates/dst/tests/conformance.rs`, green in the `run_dst` row
below), and on a **live FoundationDB** cluster (below). The doubles are *additional*
non-vacuity evidence, never the only evidence.

**(c) Fixture includes the fault? Yes.** The cap-escape clause seeds a population past the
store's own lowered cap and asserts `scan` genuinely fails loud with `ScanCapExceeded { cap }`
*first* — so a cap-lowering hook that lowered nothing cannot leave the walk vacuous. The
mutation clause commits real mutations mid-walk and proves through `get` that they landed
(insert present, delete gone) and that the walk had ≥3 laps. The new `KeysOnlyStore` /
`ShortPagedStore` doubles are driven through the *shared* clause functions, not copies.

## 4. Gates run in `$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt`)

| Run | Result |
|---|---|
| `./engine/xtask.sh ci` (typos, docs, guards, fmt, clippy `-D warnings`, build, `cargo test --workspace --exclude wyrd-dst`, machete, **cargo-deny**, conformance vectors, statics, orchestrator guard, **DST 50 seeds**) | **exit 0 — "xtask ci: all checks passed"** (run twice: once mid-way, once on the final tree) |
| ↳ in-simulator conformance (`crates/dst/tests/conformance.rs`, the `run_dst` row) | `sim_fdb_backend_passes_shared_contract`, `sim_tikv_backend_passes_shared_contract`, `redb_backend_passes_shared_contract` — 3 passed |
| `./engine/scripts/run-verify.sh` (C4-verify) | **PASS** (details in §3) |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features fdb,etcd --tests` | exit 0 |
| `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` | exit 0 |
| `cargo clippy -p wyrd-server --features tikv,etcd --tests` | exit 0 |
| `./engine/xtask.sh fdb-conformance` (throwaway Docker single-node FDB) | **exit 0** — "FoundationDB passed the shared MetadataStore conformance suite and the contention properties"; `trait_contract_against_fdb` drives both `run_all` **and** `run_all_cap_scoped` (`crates/metadata-fdb/tests/conformance.rs:155-167`), i.e. the value-asserting clauses and the cap escape, against a **real cluster** |

The iteration-3 C4-ci failure (`cargo test --workspace --exclude wyrd-dst` exit 101), which
sign-off had already reproduced clean, did **not** recur: the full workspace row is green
here.

**TiKV** remains as the human adjudicated it at the iteration-3 sign-off (accepted as
backseat while redb/FDB stay green): the available cluster answers
`InvalidKeyMode { storage_api_version: V2 }` during the first commit, before `scan_page` is
ever reached, so live-TiKV parity is still unproven. Its *code* is compiled by the two
clippy rows above, and its `scan_page` is the same shared-seam implementation the other
backends use. I am deliberately **not** re-raising this as a NEEDS-HUMAN marker, because the
sign-off explicitly closed it and asked not to re-litigate it.

## 5. Alternatives considered, with costs

1. **Assert values by cross-checking each returned pair against `store.get(key)` inside
   `checked_page`.** Tempting (one place, every page). Rejected because it is *wrong under
   clause (d)*: a key deleted mid-walk may legitimately still be returned by a page, and
   `get` would then answer `None` — the check would fail a conforming store, i.e. the same
   defect class as finding 2. Cost as well as correctness: the cap-escape fixture is 25 keys
   over up to 26 pages, so it would add ~325 extra round-trips per clause on a live cluster
   (`xtask fdb-conformance` / `tikv-conformance`) for *weaker* evidence than an exact
   expected-pair comparison, which pins the value a key was committed with rather than
   whatever `get` says at that instant.
2. **Read clause (d)'s pages through the shared `checked_page`** (uniform, and it adds the
   ordering/prefix checks). I implemented it and measured the result: the offset-paging
   double then fails on `checked_page`'s "a page must start strictly after the cursor"
   instead of on `"returned exactly once"` — i.e. clause (d)'s *own* assertion loses its
   demonstrated red, and the evidence that the no-skip rule binds becomes evidence that the
   exclusive-cursor rule binds (already demonstrated by two other doubles). Reverted, with a
   comment at `src/lib.rs:875-883` recording exactly that trade so the next reader does not
   "tidy" it back. This is why the clause keeps its light per-page checks (`items.len() <=
   LIMIT`, `next` is the last key, no cursor on an empty page).
3. **Fix only the two cited lines** (`:465`, `:772`). Rejected: the same two classes were
   live at five more sites (table in §2) and one of them (`:469`, "a short page must report
   `next: None`") was *more* wrong than the cited one. The extra cost of fixing the class
   rather than the line is +209 net lines in one file; leaving it would have bought one more
   review round.
4. **Tighten the contract instead of the fixture** — require a full page while more remains,
   so the fixtures become legal again. Rejected twice over: it rewrites clause 3 of
   `0016:2657-2658`, and proposal/ADR edits are explicitly out of scope and an automatic
   NEEDS-HUMAN (brief §"Impact & compatibility"); and it would forbid a conforming FDB page
   whose `more()` returned early, which is exactly the portability 0016 bought by *not*
   requiring snapshot isolation.
5. **A `SwappedValuesStore` (values rotated within a page) instead of `KeysOnlyStore`.**
   Either demonstrates the class — the assertion compares exact expected pairs, so stale,
   swapped and empty all fail it. Chose keys-only because it is a *named substrate call*
   (`scan_keys`), it is caught on a one-item page too (a rotation is the identity there), and
   one double keeps the file's cost down (it is already 1,224 lines).

## 6. What is deliberately not here

* No consumer switches to `scan_page` (that is #636/#637) — the brief's *Production reach*
  states this is a seam built ahead of its consumers by design.
* `scan`, `SCAN_CAP`, and every file under `docs/design/adr|specs` and proposal 0016 itself
  are untouched.
* `crates/metadata-conformance/Cargo.toml` moves `bytes` from `[dev-dependencies]` to
  `[dependencies]` (the clauses now name `Bytes` in `src/`); `cargo machete` is green with
  it (it runs inside `xtask ci`).

## 7. Scratch, and how to reproduce the two measurements

Everything transient lived under `${PDCA_SCRATCH}/pdca-builder-634-redleg` (gate logs, the
red-leg copy of `lib.rs`, a temporary `git worktree` used only to measure the delta against
iteration 3) and **has been removed**, along with the temporary worktree — `git worktree
list` shows only `wyrd`, `wyrd-verify` (the C4-verify gate's own), and `wyrd.pdca-wt`.

Both measurements in §2/§3 are reproducible from the bundle in three commands, run in
`$PDCA_WORKTREE` with this patch applied:

```sh
# restore ONLY the iteration-3 clause bodies, keeping the new test files
git checkout HEAD -- crates/metadata-conformance/src/lib.rs
git apply --include='crates/metadata-conformance/src/lib.rs' results/issue_634/iteration-v3/patch.diff
cargo test -p wyrd-metadata-conformance --test scan_page_demonstrated_red   # → 19 passed; 3 failed
```

(The per-clause probe of §2 additionally split
`a_short_paging_store_passes_every_new_clause_too` into one `#[test]` per clause so each
failure is independent — a temporary edit, not shipped, because one aggregate test is what
the file's own pattern uses.)
