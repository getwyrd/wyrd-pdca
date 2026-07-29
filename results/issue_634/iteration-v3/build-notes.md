# Build notes — issue 634 / scan-page-seam (iteration 3)

> Withheld from the reviewer; written for the human at sign-off.
> Every `path:line` is against `$PDCA_WORKTREE`
> (`/home/eddie/development/wyrd/wyrd.pdca-wt`, `origin/main` @ `22d71b4`) **with this
> patch applied**, unless it says "on the base".

## 0. What this iteration changed, and why (read this first)

Iteration 2 was rejected on **one defect class**, reported eight times by the batched
review (three distinct findings, `results/issue_634/review-batch.md`): a `scan_page`
cursor **at or past the prefix's exclusive upper bound** was treated as an ordinary
exclusive lower bound, and the conformance suite never drove that input.

The whole delta of iteration 3 is that one seam decision plus the clause that binds it:

| # | Change | Where |
|---|---|---|
| 1 | `page_lower_bound(prefix, after) -> Option<&[u8]>` (two-way) becomes `page_start(prefix, after) -> PageStart` (**three-way**: `Prefix` / `After` / `PastPrefix`) | `crates/traits/src/lib.rs:426-503` |
| 2 | Each of the five implementations answers the third arm with an **empty terminal page**, taken before any transaction/range exists | `crates/metadata-redb/src/lib.rs:177-180`, `crates/metadata-fdb/src/lib.rs:1908-1911`, `crates/metadata-tikv/src/lib.rs:1111-1114`, `crates/dst/tests/support/mod.rs:266-269`, `crates/testkit/src/lib.rs:803-809` |
| 3 | Clause (b) gains case **(v)** — cursors `p;`, `q:`, `q:decoy`, `\xff` over a `p:` walk, with a real key seeded past the prefix — asserted on **every** backend the suite runs | `crates/metadata-conformance/src/lib.rs:486-515`, `:602-637` |
| 4 | A ninth violating double, `InvertedRangeStore`, reproduces the *bounded* range read the distributed backends do and proves case (v) bites | `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:521-623` |
| 5 | Seam unit tests: the three arms, plus an equivalence proof against an independently computed `prefix_upper_bound` over a 6×12 matrix | `crates/traits/src/lib.rs:1521-1568` |
| 6 | redb regression on the real backend | `crates/metadata-redb/tests/scan_page.rs:280-306` |
| 7 | Two conformance fixtures tightened so their own invariants bind (mutation-driven, §6) | `crates/metadata-conformance/src/lib.rs:665-673`, `:955-960`, `:760-806` |

Everything else in `patch.diff` is iteration 2's reviewed content, unchanged.

## 1. Why the third arm is a *seam* decision, not a per-backend guard

The two degenerate cursors fail in **opposite** directions:

* below the prefix → the page must **widen** to the prefix (else a false "exhausted",
  the silent skip clause 3 forbids) — iteration 1's finding;
* at/past the prefix's exclusive end → the page must be **empty and terminal**, and the
  backend must not build a range at all — iteration 2's finding.

A single `Option` lower bound can express the first and *cannot* express the second, so
each backend was left to notice it, and two of five did not. `PageStart` makes the
`match` exhaustive: a backend that omits the arm **does not compile**. That is the
property I was buying — not fewer lines.

**What the substrates actually do with the inverted range `[cursor, upper_bound(prefix))`
— measured, not assumed:**

| Substrate | Behaviour | Evidence |
|---|---|---|
| tikv-client 0.4.0 | **panics**, client-side, before any RPC: `Buffer::scan_and_fetch` resolves the `BoundRange` against a `BTreeMap` with `entry_map.range(range)` (`~/.cargo/registry/src/*/tikv-client-0.4.0/src/transaction/buffer.rs:129`; `BTreeMap::range` panics when start > end) | **code read only** — the live TiKV leg is blocked, §7 |
| FoundationDB | **tolerates it**: a `first_greater_than` begin selector resolving past `end` reads back nothing, no `2005 inverted_range` | **live single-node cluster**, §4 |
| redb / BTreeMap models | tolerate it: the range is `Unbounded` on the right and stops at the first foreign key | `cargo xtask ci` |

So the *contract* cannot be delegated to the substrate — one of the three answers is a
panic in a metadata read, and the other two agree only by accident. That is exactly the
"four independent implementations must each honour it" property the brief's **Invariant
to restore** names, which is why the fix is in the seam plus the shared clause and not in
whichever backend happened to break.

**I corrected my own docs after measuring.** Iteration 3's first draft asserted "FDB
rejects it with `2005 inverted_range`" (repeating the review finding). The live run
disproved that; every occurrence now says what was measured
(`crates/metadata-fdb/src/lib.rs:1871-1876`, `crates/traits/src/lib.rs:453-463`,
`crates/metadata-conformance/src/lib.rs:501-511`).

## 2. The eight blocking review findings — each discharged or rebutted

`review-batch.md` lists 8 findings = 3 distinct claims.

1. **`crates/metadata-fdb/src/lib.rs:1898` — inverted range on an `after` at/above the
   prefix's upper bound (3 findings).** **Rebutted as a *failure-mode* claim, fixed as a
   *contract* defect.** Empirically FDB does **not** error: I reverted the fix on FDB
   alone (restoring the iteration-2 two-way rule) and ran the shared suite —
   `cargo xtask fdb-conformance`, a real single-node cluster, with the new case (v) in the
   compiled binary — and it **passed**. So the "returns an error instead of an empty
   terminal page" mechanism is false for FDB's key-selector form. The *finding under it*
   is still real and is fixed: FDB was answering a contract clause by substrate accident.
   Now it answers it by the same decision as every other backend. Evidence in §4.
2. **`crates/metadata-tikv/src/lib.rs:1107` — start-greater-than-end `BoundRange`
   panics in tikv-client's transaction-buffer lookup (2 findings).** **Accepted and
   fixed** (`crates/metadata-tikv/src/lib.rs:1096-1114`); the early return happens
   **before** `begin()`, so no transaction is open (the rubric's *Transactions* rule:
   never early-return over a live transaction). The panic mechanism is confirmed by a
   **code read** of the vendored client, not by a run — the live TiKV leg is blocked and
   declared in §7.
3. **`crates/metadata-conformance/src/lib.rs:545` — the "past the end" cursor `p:99` is
   still inside the `p:` range, so the suite never drives a cross-prefix high cursor
   (3 findings).** **Accepted and fixed.** Case (v) drives `p;` (exactly the exclusive
   upper bound), `q:`, `q:decoy` and `\xff`, with `q:decoy` **seeded** so an
   implementation answering "everything after the cursor" leaks it
   (`crates/metadata-conformance/src/lib.rs:530-533`, `:602-637`). The old `p:99` case is
   kept and re-labelled: it tests a different thing (a cursor past the last *key*, still
   inside the range).

Carry-forward items from iteration 1 that iteration 2 fixed and I have **not** regressed:
zero effective cap (`crates/traits/src/lib.rs:414-424` + the clause at
`crates/metadata-conformance/src/lib.rs:1030-1060` + the redb regression at
`crates/metadata-redb/tests/scan_page.rs:200-233`), the below-prefix cursor arm
(clause (b) case (iv)), and `test_double_scan_page` living in the **dev-only** testkit
crate rather than as a `pub` item of production `wyrd-traits`
(`crates/testkit/src/lib.rs:757-822`).

## 3. Forced refutation — the three questions

**(a) Genuine red? YES — and I ran it three ways, because one of them is weak on purpose.**

* **Seam reverted** (`page_start` restored to the iteration-2 two-way rule, everything
  else untouched): `cargo test -p wyrd-traits -p wyrd-metadata-conformance
  -p wyrd-metadata-redb -p wyrd-testkit` → **2 failed**, both in the production seam's own
  unit tests:
  `page_bound_tests::a_cursor_past_the_prefix_range_is_a_terminal_page_not_a_lower_bound`
  (`left: After([112, 59]) right: PastPrefix`) and
  `past_the_prefix_is_exactly_at_or_beyond_the_prefixs_upper_bound`. Restored → 20/20 green.
  **Everything else stayed green** — and that is the honest, load-bearing observation: redb,
  the testkit double and the BTreeMap sims all *tolerate* the wrong cursor, which is
  precisely why iteration 2 shipped the defect and why the binding red has to come from a
  double that models a **bounded** range read.
* **Clause vs. the violating double** (leg D, the brief's binding demonstration): with
  `InvertedRangeStore` in place, clause (b) fails **by assertion**, in the same
  `cargo xtask ci` run:

  ```
  thread 'inverted_range_store_fails_the_exclusive_cursor_clause' panicked at
  crates/metadata-conformance/src/lib.rs:621:17:
  a cursor at or past the prefix's upper bound ([112, 59]) must be answered with an empty
  terminal page, not an error: inverted range: begin [112, 59] is not below end [112, 59] —
  a bounded range read cannot express this cursor …
  ```

  Its sibling `inverted_range_store_passes_every_clause_that_never_passes_a_high_cursor`
  shows the same store passes the four pre-existing sequential clauses **and** order,
  termination, no-skip and the page bound — so case (v) is what catches it, not a
  differently-shaped restatement of an existing clause.
* **C4-verify** (`engine/scripts/run-verify.sh`): `PASS — red without the fix, green with
  it`. See §5 for the honest reading of that PASS.

**(b) Production path? YES.** The clauses call `MetadataStore::scan_page` through the
trait; the objects behind it are the real `RedbMetadataStore`
(`crates/metadata-redb/tests/conformance.rs`, `tests/scan_page.rs`), the real
`FdbMetadataStore` against a live cluster (§4), the two DST sim stores in-simulator
(`crates/dst/tests/conformance.rs`, run by `cargo xtask ci` → `run_dst`), and the real
`TikvMetadataStore` off-Check. `page_start` itself is production code in `wyrd-traits`
called by all five. The only stand-ins are leg D's violating doubles, whose entire job is
to be wrong.

**(c) Fixture includes the fault? YES.** Clause (b) seeds `o:decoy` *below* the prefix and
`q:decoy` *past* it, so both degenerate cursors meet a real foreign key rather than an
empty neighbourhood — without them a tolerant range read passes case (iv)/(v) by accident.
`InvertedRangeStore` uses a genuinely **bounded** `[lower, upper)` range (`bounded_range`,
`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:113-153`), because an
unbounded `BTreeMap` range **cannot** exhibit the fault at all — modelling it as the
tolerant shape would have been exactly the "fixture curated to exclude the failing element"
this question is about.

## 4. The FoundationDB legs — a real cluster, both ways

`cargo xtask fdb-conformance` brings up `deploy/fdb-single-node`, applies
`configure new single memory`, writes the cluster file and runs five `--features fdb`
legs. Run through the project runner (`./engine/xtask.sh fdb-conformance`), twice:

| Run | FDB `scan_page` body | Result |
|---|---|---|
| green | this patch | **passed** — `--lib` 40, `conformance` 2, `contention` 3, `scan` 2, `timeout` 4; "FoundationDB passed the shared MetadataStore conformance suite" |
| red-attempt | iteration-2 two-way rule restored | **also passed** — hence finding 1 is a false mechanism (§2) |

Both runs linked the *new* clause: I checked the test binary for the case (v) message
(`strings target/debug/deps/conformance-*` → "at or past the prefix's upper bound" ×2)
and its link time (15:50:22) against the reverted build.

**The four feature-gated rows the brief requires** (`xtask/src/lib.rs:81-137`), all run in
`$PDCA_WORKTREE`, all clean (workspace lints are `[workspace.lints]`, so clippy denies
warnings without a flag):

```
cargo clippy -p wyrd-metadata-tikv --features tikv       --tests   → Finished, 0 warnings
cargo clippy -p wyrd-server        --features tikv,etcd  --tests   → Finished, 0 warnings
cargo clippy -p wyrd-metadata-fdb  --features fdb        --tests   → Finished, 0 warnings
cargo clippy -p wyrd-server        --features fdb,etcd   --tests   → Finished, 0 warnings
```

## 5. The RED leg's shape — the brief's obligation (1)

`run-verify.sh`'s RED leg, on the base with the tests kept, is a **build error, not
assertion failures**, exactly as `brief.md:124-131` predicted:

* **GREEN leg** (fix applied): `cargo test -p wyrd-metadata-conformance --test
  scan_page_demonstrated_red -p wyrd-metadata-redb --test scan_page` → **28 tests ran, 28
  passed, 0 failed** (18 + 10).
* **RED leg** (production reverted, tests kept): **0 tests ran.** `cargo` exited on
  **55 compile errors** in `scan_page_demonstrated_red` alone — `E0432` (no
  `page_start` / `PageStart` / `page_limit` / `page_cursor` / `ScanPage` in
  `wyrd_traits`), `E0407` ×9 (`scan_page` is not a member of `MetadataStore`), `E0425`
  (no `contract_scan_page_*`, no `LOWERED_SCAN_CAP`), `E0433` (no `wyrd_testkit`) — plus
  the redb target.
* The gate still printed `PASS — red without the fix, green with it`, because the
  `TESTS_RAN == 0` guard at `engine/scripts/run-verify.sh:416-427` sits inside the
  cargo-succeeded branch. **A run that executed nothing is a non-result**, and I am
  saying so rather than letting the PASS stand unexplained.
* The **semantic** red is leg D: nine violating stores, each `#[should_panic(expected =
  …)]` on a clause-specific message, each with a sibling test proving the same store still
  passes the clauses it does not violate. Those fail *by assertion* inside
  `cargo xtask ci`. Treat leg D and the reverted-seam run of §3(a) as the evidence, and the
  C4-verify PASS as corroboration.

## 6. Mutation analysis (advisory C5)

`scripts/mutants-in-diff` over this bundle's `patch.diff`:

| | before this iteration's tightening | after |
|---|---|---|
| missed | 26 | **19** |
| caught | 20 | **27** |
| unviable | 26 | 26 |

The 7 newly-killed mutants were all real (if mild) weak bindings in the conformance
fixtures, and the fixes are assertions of the fixtures' own intent, not mutant-chasing:

* `contract_scan_page_walk_terminates_and_is_complete` now asserts its populations span
  **more than two pages** (`:670`), killing `LIMIT * 3 → LIMIT + 3`, `LIMIT * 2 + 1 →
  LIMIT * 2 - 1` and `LIMIT * 2 → LIMIT / 2` — each of which had left a population that
  still satisfied "is/​is not an exact multiple" while no longer reaching the boundary.
* `contract_scan_page_escapes_the_scan_cap` now requires `count > cap * 3` (`:956`), so
  the escape is demonstrated over several capped pages rather than one key past the cap.
* `contract_scan_page_no_skip_for_stable_keys` now asserts, at each mutation point, that
  the mutation lands on the side of the cursor the clause claims (`:764-806`) — killing
  `lap == 1 → lap != 1` and `lap == 2 → lap != 2`, which had silently moved the
  "behind the cursor" insert to *ahead* of it and left clause (d) asserting nothing about
  that mutation.

The **19 that remain** are two kinds, and neither is a test gap this slice can close:

* **18 in `crates/metadata-{fdb,tikv}/src/lib.rs`** — bodies behind `#[cfg(feature =
  …)]` that `cargo mutants`' default `cargo test` never compiles, so the mutant is
  trivially "not caught". The brief declares this posture
  (`brief.md:143-148`, `:219-239`); what closes it is `cargo xtask fdb-conformance`
  (run, green, §4) and `cargo xtask tikv-conformance` (blocked, §7).
* **1 in `page_start`: `replace > with >= `** — an **equivalent mutant**, provably. The
  arm is only reached when `cursor.starts_with(prefix)` is false; `cursor == prefix`
  implies `starts_with` is true, so the first arm always takes it. No input can reach the
  second arm with `cursor == prefix`, hence `>` and `>=` agree on every reachable input
  and no test can distinguish them. Every formulation of the comparison has this property
  (the boundary is absorbed by the `starts_with` arm), so it is not worth restructuring
  around.

## 7. NEEDS-HUMAN — the live TiKV leg is blocked by the host, not by the patch

```
NEEDS-HUMAN external dependency: TiKV cluster (api-version V1 / txn mode, host ports 2379+20160 free) — blocks `cargo xtask tikv-conformance`, so the new clause (b) case (v) and the whole shared suite are unverified against the real `TikvMetadataStore`; the tikv-client panic mechanism behind review finding 2 rests on a code read, not a run.
```

```toml
[[doctor.checks]]
id    = "tikv-conformance-cluster"   # the token Plan should have put in `External dependencies`
cmd   = "test -z \"$(ss -ltn 'sport = :2379 or sport = :20160' | tail -n +2)\" && docker compose -f \"${WYRD_REPO:-../wyrd}/deploy/tikv-single-node/docker-compose.yml\" config >/dev/null"
hint  = "`cargo xtask tikv-conformance` uses host networking and needs 127.0.0.1:2379 (PD) and :20160 (TiKV) FREE. A foreign cluster on those ports is what the run will silently talk to — if it is api-version V2, every commit fails `InvalidKeyMode { storage_api_version: V2 }` before scan_page is reached. Stop the other cluster (or run this on a host without one) and re-run."
level = "WARN"        # the slice builds and every other gate passes without it
```

Why I did not work around it: `docker ps` shows `upstream-repro-cluster-pd-1` /
`-tikv-1` (pingcap v8.5.5, up 23 h, custom `/pd.toml` + `/tikv.toml`) holding
127.0.0.1:2379 and :20160 with `network_mode: host`. They are **not mine** — they look
like a maintainer's upstream reproduction. `deploy/tikv-single-node/docker-compose.yml`
is host-networked on the same ports and its TiKV would register itself as a *store* with
that PD, then be torn down by the job's `compose down -v` — a real risk to someone else's
cluster for no evidence I can trust anyway (the V2 key mode is what made iteration 2's
attempt fail with `InvalidKeyMode`, before it reached `scan_page`). Running it against
that cluster would have produced the same non-result iteration 2 recorded. So: declared,
not routed around.

Everything else the brief asks for **did** run: `cargo xtask ci` (green), `xtask dst`
(green, the two sim stores in-simulator), `xtask fdb-conformance` (green, real cluster),
the four feature-gated clippy rows (clean), `run-verify.sh` (PASS, §5),
`scripts/mutants-in-diff` (advisory, §6).

## 8. Alternatives considered, with their costs

* **Per-backend guard, keeping the two-way helper** — add
  `if cursor_past_prefix(prefix, after) { return Ok((Vec::new(), None)); }` to each
  `scan_page`: 5 sites × 3 lines = **15 lines**, versus the enum's **5 sites × 1 arm** (5
  lines) plus 12 lines of helper + variant in the seam. Comparable size; rejected on
  *enforcement*, not size — nothing makes a backend include the guard, and "each backend
  notices it" is precisely the assumption that failed on 2 of 5 backends in iteration 2.
  With `PageStart` a missing arm is `error[E0004]: non-exhaustive patterns`.
* **Fix TiKV only** (the one substrate empirically shown to break) — **4 lines**.
  Rejected: it leaves FDB and redb answering a normative clause by substrate accident,
  and the brief's Invariant is stated over the category ("every backend, every
  namespace"), with the self-test "this cannot be satisfied by guarding one module". The
  next backend would inherit nothing.
* **Let the clause accept `Err` for a past-the-prefix cursor** — 0 production lines.
  Rejected outright: that is weakening the shared suite to match the backends, which
  `brief.md:363-367` forbids ("weakening or forking the suite to make a backend pass
  violates the invariant the suite exists to enforce"), and it would bless a metadata
  read that panics on tikv-client.
* **Compute `prefix_upper_bound` in the seam and compare against it** — needs each
  physically-keyed backend to redo the computation in its own key space (fdb/tikv map
  logical → physical), so the seam would return something two backends must translate.
  The `c > prefix && !c.starts_with(prefix)` form is equivalent (proved in the doc at
  `crates/traits/src/lib.rs:483-492` and tested over a 6 × 12 prefix/cursor matrix at
  `:1538-1568`) and needs no translation.
* **Model the double's failure as a panic instead of an `Err`** (what tikv-client
  actually does) — rejected: a panicking store aborts before the clause can assert, so the
  red would prove the double panics rather than that the *clause* catches it. `Err` keeps
  the clause's own assertion the thing that fails; a panicking substrate is caught by the
  same assertion, only louder (noted at
  `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:113-125`).

## 9. Self-review against the target's rubric (`AGENTS.md` §Review rubric)

* *One clock per correctness lifecycle* — no clock read added.
* *Narrow trait seams* — this is the **one** trait change 0016 authorises; `scan` is
  untouched, and `page_start`/`page_limit`/`page_cursor` are free functions in the seam
  crate, not new trait surface.
* *Transactions — roll back before any early return over a live transaction* — the new
  early return is taken **before** the transaction exists on all three real backends
  (redb `:177` before `begin_read()` at `:181`, fdb `:1908` before `trx()` at `:1913`,
  tikv `:1111` before `begin()` at `:1121`); nothing to roll back, by construction rather than by luck.
* *Absent or unsupported entries — never silent success or silent skip* — the empty
  terminal page is not a skip: no key under the prefix *can* follow that cursor, and
  clause (b) asserts the answer on every backend. The genuinely un-answerable cases stay
  loud (`ZeroPageLimit`, `ScanCapExceeded`).
* *Test fidelity — DST/sim models mirror the production adapter's seam semantics;
  conformance contracts run on every backend* — the sim stores route through the same
  `page_start`; `run_all`/`run_all_cap_scoped` are the single per-backend list.
* *Docs currency* — the store's two-operation read surface is documented in
  `docs/design/architecture/05-building-block-view.md:204`; no ADR/spec/proposal file is
  touched (out of scope by `brief.md:396-398`).
* `#![forbid(unsafe_code)]` — no new crate roots; both new test files carry it.
* Formatter/commit hooks — `cargo fmt --all` run over every touched file; `cargo xtask
  ci` (which starts with `cargo fmt --all -- --check`, `typos`, the docs render check and
  the repo-hygiene guards) is green.

## 10. Scratch hygiene

Logs and backups under `$PDCA_SCRATCH` (`/var/tmp/pdca`), all named
`pdca-builder-634-*`: `fdbconf.log`, `fdbconf-RED.log`, `ci.log`, `ci2.log`,
`verify.log`, `mutants.log`, `mutants2.log`, `fdb-lib.rs.bak`, `traits.rs.bak`. Removed
before finishing. No container, worktree or branch was created; the FDB job's own
compose stack was torn down by the job itself (`compose down -v`), and the foreign
`upstream-repro-cluster` containers were left untouched.
