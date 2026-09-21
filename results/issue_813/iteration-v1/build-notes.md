# Build notes — issue 813 (staged-scrub-and-keep)

## What I built, and why

The brief's two defects are independent gaps in two different loops, so the fix is two
independent additions wired through one shared reader family already in `gc.rs`:

1. **Scrub** (`crates/custodian/src/scrub.rs`) now also checks every fragment a session's
   **committed** `part:` record places, with the EC scheme that record's own `ChunkRef`
   carries. It reads this through a **new**, narrow reader in `gc.rs` —
   `staged_committed_parts` / `StagedPartSet` — rather than GC's existing `staged_fragments` /
   `StagedSet`, because `StagedSet` deliberately folds a session's `sidx:` (owned, in-flight)
   and `part:` (committed) placements into one set: that is the right shape for GC's and
   restore's question ("is this fragment protected from reclaim/mark at all"), but scrub's
   question is narrower and different — "does the COMMITTED record still name it, and under
   what scheme" — and leg B requires scrub to leave an `sidx:`-only chunk alone entirely.
   Reusing `StagedSet` and then filtering would have meant either widening it with an
   origin tag every other consumer (GC, restore, drain-status) would carry and never read, or
   scrub silently reading `sidx:` bytes into a set it then discards — both worse than a second,
   honestly-narrower reader that shares GC's own session-listing/paging plumbing
   (`walk_staged_range`, `staged_page`, `MPU_PREFIX`) instead of duplicating it.

2. **Reconstruction** (`crates/custodian/src/reconstruction.rs`) now reads the **full** staged
   class (`crate::gc::staged_fragments`, both `sidx:` and `part:` — leg D explicitly covers an
   `sidx:`-only chunk too) **before** the committed namespace, and an obligation whose chunk a
   staged record still names is kept queued (`Assessment::Staged`) rather than drained, exactly
   like the existing `seg:` refusal (`Assessment::Refused`) it already had a slot for: not
   auto-repaired, off the repairable-backlog gauge, and it withholds `Satisfied`/`Changed`
   (answers `Blocked`) so an operator is never told redundancy is restored for a chunk nothing
   restored. The staged read happening BEFORE the committed one is what makes the publish race
   (leg E) safe: a chunk is always visible in at least one of the two readings, whichever order
   the mid-pass write lands in.

3. **The `ReconstructionContext` seam for #814** — `clock: &dyn wyrd_testkit::Clock` and
   `staged_write_window_millis: u64` — is added and threaded through every one of the 9
   existing construction sites (44 literals, 4 crates), per the brief's explicit instruction
   that this slice must not read either field; #814 (split from the same #663, the next wave)
   is the first reader. `wyrd-testkit` moves from dev- to normal dependency in `custodian` and
   `server` (the two crates whose *production* code now names `Clock`), mirroring
   `chunkstore-fs`'s existing precedent.

## What I ruled out

- **Merging the scrub reader into `staged_fragments`.** Rejected above (point 1): it would
  widen a struct three OTHER consumers (GC, restore, drain-status) already share for a fourth
  consumer's narrower need, or make scrub read bytes (`sidx:`) leg B says it must not read at
  all. Cost of the alternative I took instead: one new ~70-line reader in `gc.rs`
  (`StagedPartSet` + `staged_committed_parts` + `read_staged_part`) that reuses every existing
  paging/session-listing helper — cheaper than threading an origin tag through `StagedSet`'s
  three other call sites and their own tests.
- **A new `W_write`-shaped constant in `cli.rs`.** The brief's default instruction was to add
  one "beside `LEASE_TTL_MILLIS`", but it also says explicitly: "If a `W_write` constant
  already exists on the base (#800 names one), use it — never two definitions." One does:
  `wyrd_custodian::gc::W_WRITE_MILLIS` (`gc.rs:201`, landed by #800/#821 before this cycle
  started). I fed that constant into the deployed `ReconstructionContext` via one local alias in
  `server/src/custodian.rs` (`STAGED_WRITE_WINDOW_MILLIS`), not a new value in `cli.rs` — the
  override clause is more specific than the general instruction and a genuinely different
  `W_write` would silently drift from the one `LATE_WRITE_DEADLINE_MILLIS` is built from.
- **Rebuilding/re-placing a staged chunk in this slice.** Explicitly out of scope (#814's); I
  confirmed no D server receives a write for a staged-kept obligation in every leg G test
  (`store.list_fragments().await.unwrap().is_empty()`).
- **Touching `staged_protection.rs` legs A-E.** They stay green unedited (verified: `cargo test
  -p wyrd-custodian --test staged_protection` — 25 pre-existing legs A-E pass unchanged before
  and after). Only leg F (scrub's own guard) needed rewriting, because its whole premise — scrub
  reads no upload record at all — is exactly what this brief changes; the brief's scope item 4
  says so explicitly ("leg F ... now says scrub reads committed parts but no owned entry").

## Where the new/appended tests live, and why

- **Legs A-C** (scrub) are the brief's required **new** file, `crates/custodian/tests/staged_scrub.rs`.
- **Legs D-F** (brief's own letters, for reconstruction) are appended to the existing
  `crates/custodian/tests/staged_protection.rs` as legs **G, H, I** — new letters, because that
  file already has its own A-F sequence (about GC/restore/scrub) unrelated to the brief's D-F
  labels; reusing D/E/F there would have collided with existing names in the same file. Per
  brief §Verification posture, they cannot live in the new file: they construct a
  `ReconstructionContext`, whose two new fields (`clock`, `staged_write_window_millis`) do not
  exist on the red leg's base, so an added test file that doesn't compile on `origin/main` would
  make the WHOLE C4-verify run UNVERIFIABLE, not just this one test red.
- Leg F of `staged_protection.rs` (scrub's own guard) is rewritten, not appended-around: its
  premise is what the brief changes. It now asserts scrub reads `mpu:`/`part:` but never
  `sidx:`, and splits the 8 `UploadRecords` fixtures into three outcome buckets (no-effect /
  `Blocked` / `Err`) instead of one blanket "answers identically" claim.

## Docs updated (docs-currency, rubric MUST)

- `docs/design/architecture/06-runtime-view.md`: the closing sentence of the delete/GC section
  ("Scrub reads committed references only.") now states scrub's committed-part read and
  reconstruction's keep-not-drain behavior.
- `crates/custodian/src/gc.rs`: the GC `reconcile` doc's claim that scrub "reads no staged
  record" is corrected to name scrub's own narrower read; the `StagedSet` doc's "three passes
  read it" is corrected to four (adding reconstruction); the `deferred: #663` marker is narrowed
  to `deferred: #814` (the rebuild/re-place half only).
- `crates/custodian/src/scrub.rs`: module doc and the `reconcile` doc comment updated to
  describe the committed-part read and the `sidx:` guard.
- `crates/custodian/src/reconstruction.rs`: module doc gains a paragraph on the staged-keep
  behavior and the new `Clock` dependency.

## Self-refutation (forced questions)

**(a) Genuine red?** Yes, checked by hand twice, not just asserted:
- Legs A-C (`staged_scrub.rs`): reverted `scrub.rs` + `gc.rs` to `origin/main` (`git diff` saved,
  `git checkout --`, then `git apply` to restore) and re-ran
  `cargo test -p wyrd-custodian --test staged_scrub`: **5 failed, 2 passed** — every leg-A
  sub-case (corrupt/missing/wrong-scheme) and both leg-C sub-cases failed by assertion; leg B
  (the guard) and leg A's control passed on the reverted base too, exactly as the brief's
  Falsifiability section predicts ("B passes there by design ... a control"). Reapplied the
  diff; re-ran green (7/7).
- Legs G-I (`staged_protection.rs`, the brief's D-F): reverted only `reconstruction.rs` to base,
  and — since the reverted `ReconstructionContext` has no `clock`/`staged_write_window_millis`
  fields — temporarily deleted those two lines from the test file's `reconstruction_pass`
  helper (the brief's own prescribed method: "the appended legs with only the two field
  initialisers removed"). Ran `cargo test -p wyrd-custodian --test staged_protection`:
  **4 failed, 27 passed** — the two leg-G positives, leg H, and leg I all failed by assertion
  (each printing `left: Satisfied, right: Blocked`); the leg-G control and all pre-existing
  A-F legs passed unchanged. Restored both files; re-ran green (31/31).

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the one fenced
control point — with a real `ScrubContext` / `ReconstructionContext` over `MetadataStore` /
`ChunkStore` trait objects; the in-memory `Meta`/`Disk` doubles implement those two production
traits and nothing else (no scrub- or reconstruction-specific test seam). `staged_scrub.rs`'s
double is a narrowed copy of `staged_protection.rs`'s own (fewer fields — no read log, no
hooks — since legs A-C need neither), not a stand-in for the production loop itself.

**(c) Fixture includes the fault?** Yes: leg A's corrupt/missing/wrong-scheme fragments are
real bytes (or a real absence) on the real `Disk` double the pass fetches from
(`ChunkStore::get_fragment`); leg D/G's obligations are enqueued against a chunk whose only
committed reference is genuinely absent (nothing seeded under `inode:`) so the pass has to
consult the staged class to avoid draining it; leg E/H's publication race is a real
`WriteBatch` applied mid-pass via the store's own hook (`Meta::hook`, the same mechanism
`staged_protection.rs`'s existing legs C use), not a pre-baked "already published" fixture.

## Manual verification commands (for the record)

```
cargo test -p wyrd-custodian --test staged_scrub        # legs A-C, 7/7 green post-fix
cargo test -p wyrd-custodian --test staged_protection    # legs A-I, 31/31 green post-fix
cargo test -p wyrd-custodian                              # full crate suite, all green
cargo test -p wyrd-server --test custodian_day_one        # production deployed-loop tests, green
cargo check --workspace --all-targets                     # whole tree compiles
cargo fmt --all -- --check                                 # clean after `cargo fmt --all`
cargo clippy --workspace --exclude wyrd-dst --all-targets  # clean
cargo machete                                              # no unused deps
cargo deny check / --config deny-all-features.toml check advisories / check licenses bans sources
cargo xtask statics                                        # ADR-0035 gate, clean
python3 docs/publishing/tools/lint_docs.py                 # docs lint, OK
cargo xtask ci                                              # the project's real gate — see below
```

`cargo xtask ci` was run to completion (background, ~10 min): fmt, clippy (workspace,
all-targets, excl. `wyrd-dst`), build, the full workspace `cargo test` (187 `test result: ok`
blocks, 0 failures), cargo-machete, all three `cargo-deny` invocations (the only warnings are
two pre-existing unmatched ignore-list entries in `deny.toml`/`deny-all-features.toml`,
unrelated to this patch — `RUSTSEC-2026-0253`/`ISC` — present before my changes), conformance,
statics, the deploy-guard, and the full madsim DST tier (`--cfg madsim`, `DST_SEEDS=50`).
Final line: `xtask ci: all checks passed`. The driver's own C4-ci gate re-runs this
independently; this is my own pre-flight confirmation, not a substitute for it.

## NEEDS-HUMAN

None from me. Both `typos` and `docs-renderer` (the brief's `External dependencies`) are
installed and exercised clean in this environment (`typos-cli 1.48.0`; `python3 -c "import
markdown_it, yaml"` succeeds; `python3 docs/publishing/tools/lint_docs.py` → `OK`).
