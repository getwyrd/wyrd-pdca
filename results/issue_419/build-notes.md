# Build notes — issue 419 / read-consistency-conformance-properties

## What I built

Three new backend-agnostic `contract_*` fns in
`crates/metadata-conformance/src/lib.rs` (appended after the existing four, which end
at `:111` on `feat/m4-production-metadata-backend`):

- `contract_read_after_commit` (`lib.rs:135-159` post-patch) — a `get` observes the
  most recently committed value across a **sequence** of overwrites to the same key,
  not just the single commit-then-read `contract_commit_and_get` already pins
  (`lib.rs:24-37`). Grounded in ADR-0015 clause 3 ("Per-session read-your-writes and
  monotonic reads", `../wyrd/docs/design/adr/0015-consistency-contract.md:24`) and
  proposal 0015's "Read consistency to document" open question
  (`.../0015-milestone-4-production-metadata-backend-revised.md:780-785`).
- `contract_rename_race_yields_conflict` (`lib.rs:161-224` post-patch) — a mutation
  landing between a read-then-commit's `get` and its own `commit` yields `Conflict`,
  never a torn/duplicated binding. Models `rename`'s exact shape
  (`crates/core/src/metadata.rs:276`, `get(&old_key)` at `:284`, `.require(old_key,
  current)` at `:288`) and proposal 0015's locking-read rule ("§the mandatory rule —
  lock read-only precondition keys", `.../0015-...-revised.md:311-323`).
- `contract_scan_is_consistent_cut` (`lib.rs:226-260` post-patch) — a single `scan()`
  observes one consistent cut across a rename: the moved binding appears in exactly
  one of the two positions. Deliberately modest (see "Consistent-cut" below).

Wired into the redb driver at `crates/metadata-redb/tests/conformance.rs:34-37`
(`trait_contract`, now calling all seven `contract_*` fns against a fresh
`RedbMetadataStore` each). `crates/metadata-conformance/Cargo.toml` gained
`[dev-dependencies]` (`async-trait`, `bytes`, `pollster`, all pre-existing
workspace-pinned versions) for the demonstrated-red harness below. No file under
`metadata-redb/src`, `metadata-tikv/src`, `core`, or `traits` was touched — scope held
to test/suite code only, per the brief's "no production code change" invariant.

## Demonstrated-red (the FORCING FUNCTION, #146)

New file `crates/metadata-conformance/tests/demonstrated_red.rs` — dev/test scope
only (Cargo `tests/` integration-test convention: never compiled into the library,
never linked into any shipped binary), answering the brief's "Violating-store test
double: … confirm placement" NEEDS-HUMAN note by placement alone (it cannot be
reached from any production build).

Three purpose-built violating `MetadataStore` stubs, one per property, each proven to:

1. **fail** (panic) the property it targets — a `#[should_panic(expected = "...")]`
   test asserting the specific `assert_eq!` message from the new `contract_*` fn, and
2. **pass unmodified** all four pre-existing sequential `contract_*` fns — a sibling
   test with no `#[should_panic]`, so it fails loudly if the violating store happens
   to also break the old suite (which would mean the new property isn't adding
   anything the old one didn't already catch).

Both directions are load-bearing: (1) proves the property is non-vacuous (a green
that could never go red is not a pin); (2) proves it is non-redundant (existing
`contract_*` fns can't already substitute for it). All 6 tests currently pass (5
straight `ok`, 1 `should_panic ... ok` each — see `cargo xtask ci` run below).

### Store 1 — `StaleCacheStore` (targets `contract_read_after_commit`)

Models ADR-0015's *rejected* Option B (nearest-replica, bounded staleness): `get` is
correct through a key's first **and second** successful write, then pins the
second write's value forever after — i.e. every write from the third onward is
silently invisible to reads.

I first tried a cruder "caches on first read, never invalidates" stub. It turned out
to **also** fail `contract_require_value_gates` (`lib.rs:83-111`) — I hadn't
noticed that test itself commits key `k` twice and re-reads it twice (once after a
no-op conflicting commit, once after a real second write), which is already a
"read-after-a-second-write" check. Caught this by actually running the demonstrated-
red harness (`cargo xtask ci` → `stale_cache_store_passes_existing_sequential_contracts`
panicked at `lib.rs:110`, left `Some([118])` / right `Some([118, 50])`) rather than
reasoning it through on paper — this is exactly why the brief demands a *run*, not an
assertion in prose. Fixed by pinning only from the **third** write onward: no existing
`contract_*` fn writes any single key more than twice, so `contract_require_value_gates`
still passes (its second read lands exactly at the pin point, which coincidentally
still holds the correct value), while `contract_read_after_commit`'s four-iteration
loop (`lib.rs:143`) trips it at iteration 3.

### Store 2 — `IgnoresRequireOnDeleteStore` (targets `contract_rename_race_yields_conflict`)

A plausible real bug: "a `require` precondition on a key this same batch also
`delete`s is redundant, skip it." Exactly wrong for `rename`'s
`require(old_key, current)` + `delete(old_key)` shape — it lets a stale racer's commit
through, producing a **duplicated** binding (the winner's key AND the loser's key both
end up holding the moved value). Neither `contract_require_value_gates` nor
`contract_require_absent_gates` ever combines a `require` precondition with a `delete`
of that same key in one batch, so the bug is invisible to both; the new property's
`.require(old_key, ...).delete(old_key)` (mirroring `rename`) catches it.

### Store 3 — `LeakyScanIndexStore` (targets `contract_scan_is_consistent_cut`)

The brief's own suggested example (`brief.md:22`, "a `scan` that returns a torn
view"): a listing index updated on `put` but never purged on `delete`. After a
rename (delete old key, put new key, same prefix), one `scan()` call returns **both**
positions. `contract_scan_by_prefix` (`lib.rs:41-56`) never deletes anything before
scanning, so it can't see the leak; `contract_scan_is_consistent_cut`'s
scan-before/mutate/scan-after shape does.

## Consistent-cut expressibility (the brief's flagged judgment call)

The brief's own Difficulty note and NEEDS-HUMAN item both flag that redb's `scan` is
a single atomic local read, so `contract_scan_is_consistent_cut` risks passing only
trivially there. I did **not** attempt to fabricate genuine concurrent execution
(e.g. real OS threads racing a `scan()` against a `commit()`) to force it — that
would violate the brief's own "Deterministic on redb" invariant (reusing
`pollster::block_on`, no thread-timing nondeterminism) and ADR-0009's DST-over-real-
concurrency spirit. Instead the property is deliberately modest: a single scan
before a rename-shaped mutation, a single scan after, asserting the count and
identity invariant ("exactly one of the two positions") holds across it. This
necessarily passes on redb without exercising true interleaving — but it is **not**
vacuous: `LeakyScanIndexStore` shows a real (if redb-inapplicable) bug class it would
catch. I flag explicitly, as the brief's NEEDS-HUMAN item anticipates, that whether
this redb-trivial version is worth landing here (as the inherited, TiKV-facing
pin) versus deferring the *genuinely* discriminating version to #254's TiKV
paged-scan-at-scale test is the reviewer's call, not mine to resolve unilaterally.

## Alternatives considered and ruled out

- **True concurrent interleaving via OS threads / `tokio::join!`.** Rejected: redb's
  `MetadataStore` methods are synchronous under `pollster::block_on` (no await
  points to interleave cooperatively), so genuine concurrency needs real threads —
  which reintroduces exactly the timing nondeterminism the brief's "Deterministic on
  redb" invariant and ADR-0009 rule out. Cost if attempted: every one of the three
  properties would need a `std::thread::scope` + channel-based rendezvous to pin the
  interleave point, roughly 30-40 extra lines per property, and the redb run would
  become flaky under CI load — a materially heavier, less deterministic test for a
  gain (true interleaving on an atomic backend) that cannot manifest on redb anyway
  (its `commit`/`scan` each hold the single writer lock for their whole duration).
  Chose deterministically-ordered decomposed ops instead (get, then a separate
  interleaving commit, then the racer's own commit) — the brief explicitly leaves
  this choice open ("the exact interleaving mechanism … is Do's call").
- **`wyrd_testkit::Sim` seed-driven interleaving**, mirroring
  `version_cas_rejects_a_stale_writer` (`crates/metadata-redb/tests/conformance.rs:125-179`).
  Read that test closely: `Sim` there is used only as a seeded RNG for generating ids
  (`sim.gen()`), not as an actual interleaving scheduler — the "race" is itself
  deterministically-ordered (writer A commits, then writer B commits, sequentially).
  So reusing `Sim` would add a dependency (`wyrd_testkit` in
  `metadata-conformance`'s `[dependencies]`, which the brief's "Scope" note says are
  illustrative-only, not binding) without buying any actual extra interleaving power
  over what I already have with plain deterministic ordering. Not worth the added
  dependency edge (`metadata-conformance` currently depends on nothing but
  `wyrd-traits` — deliberately, per its own doc header ADR-0016 dependency
  discipline).
- **A single richer property function per concern instead of three.** Rejected per
  the brief's own enumeration ("#419's own body … enumerates the three properties
  verbatim") — the three are individually named in the brief's Scope/Success
  criterion, so collapsing them would not match the planning artifact.

## Verification

Ran the project's own gate, `./engine/xtask.sh ci` (delegates to `cargo xtask ci` in
`$PDCA_WORKTREE`), per `docs/INTEGRATION.md` §3/§9 — not a hand-rolled `cargo test`
invocation. Two failures surfaced and were fixed in place before the final green run:

1. First run: `stale_cache_store_passes_existing_sequential_contracts` panicked —
   the demonstrated-red harness caught my own first-draft violating store being
   *too* crude (see "Store 1" above). Fixed the store, not the test.
2. Second run: `cargo fmt --all -- --check` failed on `demonstrated_red.rs`'s
   formatting (rustfmt wanted `self.pinned_after_second_write.lock().unwrap().insert(k, v);`
   on one line, not four). Ran `cargo fmt --all` (the project's configured
   formatter) and re-committed the reformatted file — this is exactly the
   commit-hook-shaped failure the harness warns "no gate models," so I ran it
   before declaring done rather than after.
3. Third run: full `cargo xtask ci` (fmt, clippy `--all-targets` workspace-wide,
   build, `cargo test --workspace --exclude wyrd-dst` — including
   `wyrd-metadata-conformance`'s new `demonstrated_red.rs` and
   `wyrd-metadata-redb`'s `trait_contract`, both green — `cargo-machete`,
   `cargo-deny`, the chunk-format conformance vectors, `statics`, and the madsim DST
   sweep) exited 0. Log: `/tmp/xtask-ci-post3.log` on this machine (not shipped in
   the bundle; a human re-running `pdca gates` will reproduce it).

Red-before / green-after (NET-NEW posture, per the brief's "Verification posture" —
criterion-ABSENCE is the pre-patch "red", there being no prior failing assertion to
flip): on `feat/m4-production-metadata-backend` before this patch,
`crates/metadata-conformance/src/lib.rs` has no `contract_read_after_commit` /
`contract_rename_race_yields_conflict` / `contract_scan_is_consistent_cut` symbol at
all and `crates/metadata-redb/tests/conformance.rs::trait_contract` doesn't call
them — confirmed by reading the pre-patch file (quoted in full in this bundle's
transcript). Post-patch, `trait_contract` calls all seven and `cargo xtask ci` is
green.

## Scope discipline

Touched exactly the two files the brief's "Test file" / "Scope" fields name
(`crates/metadata-conformance/src/lib.rs`, `crates/metadata-redb/tests/conformance.rs`)
plus the two additions the "and add whatever violating-store test double the
demonstrated-red requires" clause explicitly licenses
(`crates/metadata-conformance/tests/demonstrated_red.rs`,
`crates/metadata-conformance/Cargo.toml` dev-dependencies for it). Nothing under
`metadata-tikv/*` touched (that's #254's wave-2 wiring, per the brief's "Ordering
note" — out of scope here). No ADR/proposal/doc edited (the module-level
read-consistency doc is explicitly #254's, not #419's, per "out of scope").

## NEEDS-HUMAN carried forward from the brief (not resolved by Do; recorded per the
brief's own "Known NEEDS-HUMAN" section, unchanged)

- Consistent-cut expressibility: is the redb-trivial version of
  `contract_scan_is_consistent_cut` worth landing here, or does the real
  discriminator belong with #254's TiKV at-scale test? (See "Consistent-cut
  expressibility" above — I built the modest version and demonstrated it's not a
  tautology, but whether it's *sufficient* for #419's purposes is the reviewer's call.)
- Whether #261's decision should be folded into proposal 0015 / a new ADR before
  being frozen into the shared suite — not a code prerequisite, per the brief.
- Read-after-commit's marginal value — I addressed the mechanical redundancy
  concern (demonstrated it catches something the existing suite doesn't, via the
  three-writes threshold), but the reviewer's broader judgment call ("is this
  enumeration worth keeping as its own property vs. folding into #254's doc") stands
  as the brief states it.
