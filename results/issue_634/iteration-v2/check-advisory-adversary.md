# Adversarial review — issue 634 / scan-page-seam (advisory, non-gating)

Re-ran the asserted evidence in a scratch clone of `$PDCA_TARGET` (removed afterwards):
`wyrd-traits --lib` (18/18), `metadata-redb --test scan_page` (9/9),
`metadata-conformance --test scan_page_demonstrated_red` (16/16, incl. 7
`#[should_panic(expected = …)]`), `wyrd-testkit --lib` (34/34), and the gating
in-simulator row `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test conformance`
(3/3). Also compile-checked both backends the default gate never builds:
`cargo clippy -p wyrd-metadata-{tikv,fdb} --features {tikv,fdb} --tests` — both clean.

## Findings

- **NEEDS-HUMAN [impl] — `crates/metadata-tikv/src/lib.rs:1107`** — the new `scan_page`
  builds `(cursor..upper)` from a **caller-supplied** cursor, and **panics** whenever that
  cursor sorts at or above the prefix's upper bound. Concrete case:
  `store.scan_page(b"orphan:", Some(b"retire:0001"), 64)` (minimal: `scan_page(b"p:", Some(b"q"), 8)`).
  `page_lower_bound` returns `Some(after)` because `after >= prefix`
  (`crates/traits/src/lib.rs:444`), so `cursor = next_page_start(physical(after))`
  (`:435`, appends `0x00`) while `upper = prefix_upper_bound(physical(prefix))` (`:80`,
  increments the last byte) — giving `cursor > upper`, an **inverted** `BoundRange`.
  `txn.scan` → `Transaction::scan_inner` → `Buffer::scan_and_fetch` →
  `self.entry_map.range(range)` (tikv-client 0.4.0 `src/transaction/buffer.rs:129`, a
  `BTreeMap<Key, BufferEntry>`) → `panicked at …btree/search.rs: range start is greater
  than range end in BTreeMap`. I reproduced that panic standalone against tikv-client
  0.4.0 with the exact key shapes above. redb answers the identical call `(vec![], None)`
  — verified in-process; it iterates and breaks on the first non-prefix key
  (`crates/metadata-redb/src/lib.rs:182,186`), as do both sim models
  (`crates/dst/tests/support/mod.rs:270`) and `test_double_scan_page`. So this is not a
  hypothetical "don't do that": one backend aborts the task where the other three return
  the terminal empty page the contract wants. The patch has already conceded that a
  foreign cursor is in-contract input — `page_lower_bound` exists precisely for "the
  drain's persisted cursor after a namespace rename, … a shared cursor column across
  namespaces" — but it floors the cursor at the prefix without ceiling it at the prefix's
  upper bound, so it covers exactly half of the input space it was introduced for. Fix
  shape: when the resolved cursor is `>= prefix_upper_bound(prefix)`, short-circuit to
  `(vec![], None)` (or clamp the range), decided once in the seam alongside the floor.

- **NEEDS-HUMAN [impl] — `crates/traits/src/lib.rs:1438`** — the seam's own unit test
  *blesses* the input above as safe: `assert_eq!(page_lower_bound(b"p:", Some(b"q")), Some(&b"q"[..]))`
  under the comment "the page is then exhausted, which is a terminal answer rather than a
  wrong one". That is a claim about **every backend's** behaviour asserted by a test that
  only exercises a two-line pure function — and it is false for `metadata-tikv` (above).
  The conformance suite cannot catch the divergence either: clause (b) drives four
  *below*-the-prefix cursors (`crates/metadata-conformance/src/lib.rs:557`) and one
  past-the-last-key cursor that is still **under** the prefix (`:545`, `Some(b"p:99")`),
  but no clause ever passes an `after` at or above `prefix_upper_bound(prefix)`. So
  neither `cargo xtask ci` nor the maintainer-run `xtask tikv-conformance` /
  `fdb-conformance` legs can observe it — the brief's "normative and identical on every
  backend" holds only on the inputs the suite happens to drive. The mirror case belongs
  in `contract_scan_page_cursor_is_exclusive` beside case (iv), and it would have gone
  red on TiKV before this shipped.

## Attempted and could **not** refute

- **The C4-verify PASS is compile-shaped, but the substantive red is real.**
  `check-gates.json:46` claims "red without the fix, green with it"; on `origin/main`
  the added files cannot compile at all, and `brief.md:124-131` pre-declares that
  `run-verify.sh` scores that as red over a run that executed zero tests. I therefore
  went at leg D directly and could not break it: all seven violating doubles carry
  `#[should_panic(expected = "…")]` with clause-specific messages (not bare
  `should_panic`), each has a sibling test proving the *same* double still passes the
  four pre-existing sequential clauses, `FaithfulPagedStore` proves the clauses are not
  red-by-construction, and `ScanBackedStore` proves the rejected `scan()`-shim shape
  actually fails leg B. That is genuine discriminating power, not a tautology.
- **The cap-0 regression from iteration 1 is genuinely fixed, not papered over.**
  I drove `RedbMetadataStore::with_scan_cap(0)` with 25 seeded keys at limits
  `1 / 5 / usize::MAX`: all three refuse with a typed `ZeroPageLimit` rather than the
  previous unbounded 25-item page. The `>= limit` guard at
  `crates/metadata-redb/src/lib.rs:193` and the refusal at `crates/traits/src/lib.rs:416`
  are both killed by mutants (`mutants.out/caught.txt`).
- **Other edge inputs I expected to break redb, and did not:** cursor above the prefix
  range; empty prefix `b""`; a stored key exactly equal to the prefix (walk still returns
  it once and terminates); an all-`0xff` prefix where `prefix_upper_bound` is `None`;
  a below-prefix cursor over an *empty* prefix range; and the cap-clamp and cursor-floor
  interacting (`with_scan_cap(3)` + `after = Some(b"")` + `limit = usize::MAX`).
- **The 25 surviving C5 mutants are not hiding a defect.** I read `mutants.out/missed.txt`:
  they are (a) fdb/tikv bodies the default gate never runs — expected, and the brief's
  declared off-Check posture — and (b) arithmetic in the conformance *fixtures*
  (`:592`, `:593`, `:854`). I checked each of (b): mutating `LIMIT * 3` → `LIMIT + 3` or
  `cap * 3 + 1` → `cap * 3 - 1` still satisfies the fixtures' own asserted invariants
  (exact-multiple-ness, `count > cap`), so the clause still tests what it claims.
- **The ~34 delegating doubles are uniform and none is a decorator.** All delegate to
  `wyrd_testkit::test_double_scan_page`; no `impl MetadataStore` in the workspace wraps
  another store, so no fault-injection seam is bypassed by the delegation. `wyrd-testkit`
  is a `[dev-dependencies]` entry in all three backend crates, so the "a production
  backend naming this helper does not compile" claim holds; the one non-dev consumer
  (`crates/metadata-fault-conformance`) is itself dev-only downstream and declares no
  `MetadataStore` impl.
- **`run_all_cap_scoped` is a second runner rather than a `run_all` extension** (a
  deviation from `brief.md:36-43`'s "no per-driver list to edit"), but every metadata
  driver was in fact wired: redb, both DST sim stores, fdb and tikv. The brief itself
  asks for "a shared conformance clause parameterised over a cap-lowering hook", which
  this is. Not a refutation.

*Context, not a finding: `check-gates.json` is `overall: fail` with the gating T4 row at
**8** blocking findings, up from 6 in iteration 1.*
