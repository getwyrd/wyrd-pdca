# Adversarial review — issue 634 / scan-page-seam (advisory, non-gating)

Attacked: the red→green evidence, the four normative clauses, the five `scan_page`
bodies, and the two red gates. Re-ran everything reproducible at `$PDCA_TARGET`
(`cargo test --workspace --exclude wyrd-dst` ×2, the `--cfg madsim` DST leg, the two
feature-gated clippy rows) and fuzzed the redb backend against an independent oracle.
Three findings; the rest of the attack failed and is recorded as such.

- **NEEDS-HUMAN [impl]** — `crates/traits/src/lib.rs:518` (`page_cursor`) with
  `crates/traits/src/lib.rs:1009` (contract clause 3): **the shipped contract makes
  "short page" mean "prefix exhausted", but nothing in it obliges a backend to fill a
  page** — so a backend whose substrate legitimately under-fills one answers
  `next: None` with keys still behind it, which is exactly the silent skip this slice
  exists to prevent. Clause 3 as written ("`next` is `Some(last key returned)` while
  more may remain, `None` **only** when the prefix is exhausted") is satisfied by a
  store that returns 3 of 6 pairs with `next = Some(last)`; `page_cursor` instead
  infers exhaustion from `items.len() < limit`. The suite silently enforces the
  missing rule — and its own comment denies enforcing it. Concrete case, run against
  the shipped clauses: a `LazyPagedStore` honouring clauses 1–4 and
  `items.len() <= min(limit, cap)` but capping each page at 3 pairs goes **red at three
  places** — `crates/metadata-conformance/src/lib.rs:464` ("one page of the whole
  prefix must come back in raw byte order", left 3 keys / right 6),
  `:563` ("a cursor below the range skips nothing", left 3 / right 4) and `:917`
  ("a limit above the store's cap must be clamped… never refused", left 3 / right 5) —
  while `crates/metadata-conformance/src/lib.rs:305-310` states the opposite in prose
  ("the contract lets a store answer a short non-final page"). Two of those three are
  also mis-messaged: `:917` blames the cap clamp for a maximality failure, and `:563`
  is the count-based assertion shape the repo rubric calls out. Fix is cheap and
  belongs in Do: state maximality (or "a short page means exhausted") as a numbered
  clause on `MetadataStore::scan_page`, correct the `LAP_BUDGET` comment, and reword
  `:917`. It is load-bearing rather than cosmetic — the two backends whose behavioural
  evidence is deferred (fdb, tikv) are the ones whose substrates can hand back a
  partial range read, and `crates/metadata-tikv/src/lib.rs:1141` derives `next` from
  `page_cursor` on exactly that assumption.

- **NEEDS-HUMAN [impl]** — `crates/traits/src/lib.rs:1054` claims of
  `wyrd_testkit::test_double_scan_page` that testkit "is a dev-dependency everywhere,
  never a dependency", and `crates/testkit/src/lib.rs:772-780` upgrades that to "a
  production `MetadataStore` body naming this function does not compile… what would
  otherwise be a convention is a build error". **Both are false as stated**:
  `crates/coordination-mem/Cargo.toml:16` lists `wyrd-testkit` under
  `[dependencies]` (with the comment "production code is written against testkit's
  abstractions"), as does `crates/metadata-fault-conformance/Cargo.toml:20`, and
  `cargo tree -p wyrd-server -e normal -i wyrd-testkit` resolves
  `wyrd-testkit → wyrd-coordination-mem → wyrd-server`. The new `pub async fn
  test_double_scan_page` — documented as a cap-inheriting `scan()` shim, i.e. exactly
  the body #508's 4th attempt was rejected for — is therefore compiled into the
  production server binary, and the "build error, not a convention" guarantee holds
  only for the three metadata crates by accident of their *current* dev-dep edges.
  This is the same visibility risk iteration 1 was asked to reduce, moved rather than
  removed; a `#[cfg(feature = "test-doubles")]` gate (or an accurate doc) would settle
  it. (Same doc block, minor: `crates/traits/src/lib.rs:1038-1039` states "a page
  **never** fails with `ScanCapExceeded`" as a trait-wide contract, while ~34
  in-workspace impls raise exactly that via the helper by design.)

- **NEEDS-HUMAN** — `check-gates.json` reports the sole gating build row red
  ("xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101"),
  and **I cannot reproduce it**: at `$PDCA_TARGET` with the patch applied that exact
  command exited 0 twice (153 test binaries green, incl. `metadata-redb/tests/scan_page.rs`
  10/10 and `metadata-conformance/tests/scan_page_demonstrated_red.rs` 18/18), and the
  DST leg the sim stores depend on — `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 cargo
  test -p wyrd-dst`, `xtask/src/main.rs:1602-1608` — also exited 0. So the red is
  either a flake or an environment artefact of the gate run, but an unexplained red on
  the one gating row cannot be waived by my green: a human should read the gate log and
  name the failing binary (if it is a flaky test elsewhere in the workspace, that is a
  repo item, not this patch's). Verdict on that row provisional (issue #236).

- (Not a refutation, recorded because the human sees the red) **C5's 19 missed mutants
  are fully explained**: 18 sit in `crates/metadata-fdb/src/lib.rs` and
  `crates/metadata-tikv/src/lib.rs`, the two bodies the brief declares off-Check, and
  the 19th — `crates/traits/src/lib.rs:499:32 replace > with >=` in `page_start` — is an
  **equivalent mutant**: `cursor == prefix` is absorbed by the `starts_with` arm at
  `:497`, so `>` and `>=` are observationally identical. Every other seam mutant
  (`page_limit`, both `page_start` guards, `page_cursor`'s `>=`, redb's `!` and `>=`,
  the testkit helper's `>`) is caught.

## Attempted and could not refute

- **The redb body, fuzzed against an independent oracle** (400 seeds × 40 random probes
  + 24 full walks each; prefixes `""`, `p`, `p:`, `o:`, `q`, `p\xff`; keys containing
  `0x7f/0x80/0xff/é` and prefix-of-prefix pairs; cursors below / at / inside / past the
  range and non-existent; limits 1–7 and `usize::MAX`; caps 1, 2, 3, 5, 8, 2^20): **zero
  divergences** from "keys under prefix strictly greater than `after`, truncated to
  `min(limit, cap)`", every walk complete, in order and terminating, every zero-bound
  refused with `ZeroPageLimit`. The iteration-1 unbounded-page defect at cap 0 is
  genuinely fixed (`crates/metadata-redb/src/lib.rs:170`, `crates/traits/src/lib.rs:414`).
- **The `page_start` upper-bound equivalence** (`crates/traits/src/lib.rs:494-503`) — I
  could not find a `(prefix, cursor)` pair where `cursor > prefix && !starts_with` differs
  from `cursor >= upper_bound(prefix)`, including all-`0xff` and empty prefixes.
- **Leg D's non-vacuity** — the seven violating doubles are real: each `#[should_panic]`
  string matches the clause's own message, and each double passes the pre-existing
  sequential clauses, so the reds are not compile-shaped.
- **The deferred backends compile**: `cargo clippy -p wyrd-metadata-fdb --features fdb
  --tests` and `-p wyrd-metadata-tikv --features tikv --tests` are both clean here.
- **The `tikv-client` panic citation** repeated in three doc blocks is accurate —
  `tikv-client-0.4.0/src/transaction/buffer.rs:129` is `self.entry_map.range(...)` on a
  `BTreeMap`, which panics when start > end; the `PastPrefix` arm is a real fix, not
  decoration.
- **Docs currency**: `06-runtime-view.md` / `08-crosscutting-concepts.md` contain no
  paragraph claiming the store offers only a whole-namespace scan, so
  `05-building-block-view.md:204` is the correct and sufficient update.
