# Build notes — #772 `multipart-owned-staging-entry`, round 3

Base: the per-cycle worktree `$PDCA_WORKTREE` at `a1d48ed` (origin/main with child-1 #771 merged).
Every `path:line` below is on that base **with this patch applied**, unless marked "base".
`metadata.rs` lines are for convenience only; the brief asks for that file to be cited by symbol
(#776 may land there first), so each one names its symbol.

Round 3 starts from round 2's patch (`iteration-v2/patch.diff`) and changes only what the
round-2 carry-forward and the T4 review flagged. Production code changes this round are **doc
comments only**; the behaviour change is in the two tests that cover the sweeps. Same 13 files as
rounds 1 and 2, exactly the brief's list.

## What round 2 left open, and what this round did about each

| Round-2 finding | Source | What changed |
|---|---|---|
| GC half of S5 depends on scan order: `continue`→`break` in GC's skip arm passed 16 of 40 runs, because `MemMeta` is a `HashMap` | adversary `[impl]` | **Fixed.** The GC leg now controls scan order and sweeps every seed's population in a drawn order and in its reverse. The same mutant now fails 20 of 20 runs (control: 0 of 20). See "The GC leg" below. |
| Docs overstate the operator signal (GC's expired-lease input is off by default; `write::sweep_expired_leases` has no production caller) | adversary `[impl]` | **Fixed** in all four places that said it: the docs sentence, the `multipart.rs` module header, the `gc.rs` doc and the `write.rs` doc. Same length as before (164 words for the docs insertion, round 2: 164). |
| T4 blocking ×2: no seeded Tier-0 DST coverage for the changed sweep (`write.rs:654`) or the GC skip (`gc.rs:502`) | T4 (gating) | **Addressed inside the brief's file list:** seeded Tier-0 properties (`wyrd_testkit::Sim`) for both sweeps. Not in `crates/dst` — the brief keeps that file to the mechanical initializer. If T4 still wants `crates/dst`, that is a human call; rows to record are at the end. |
| C2 / C4-verify UNVERIFIABLE (RED leg cannot compile) | pre-declared by the brief | Unchanged by design. Fresh base probe this round (6 of 6 red on base, green on the fix), plus the negation table. |
| S8's briefed negation cannot bind (deferred item 6) | adversary `[human]` | Recorded again below with this round's output: the briefed negation fails only S7; the canonical-bytes negation fails only S8. |
| Out-of-scope note: `OwnedEntry` doc said the checked path is "public end to end", but the key is not part of it | adversary (note) | Doc made accurate (`multipart.rs:3506-3511`): the value's checked path is public, and the key relation is `decode_owned_entry`'s. No new function (a key helper would be `sidx:` writer surface, #656–#659). |
| T5 prior art; Validation fitness-to-purpose | human | Nothing for Do. |

## What changed this round, file by file

### `crates/custodian/tests/gc.rs` — the GC leg (the one ripple file allowed a substantive hunk)

Round 2's single fixture (`pending:225` ordinary, `pending:226` misfiled) is replaced by one
seeded property, `expired_lease_input_skips_what_it_cannot_read_under_every_scan_order`
(`:1142`):

- `ScanInOrder` (`:923`) wraps the file's `MemMeta` and answers `scan` in an order the test names;
  every other call goes straight through. Reason: `MetadataStore::scan` says "Order is
  unspecified" (`crates/traits/src/lib.rs:1353-1354`), so the sweep has to be right under every
  order, and a `HashMap` order is one the test cannot name. I kept `MemMeta` itself unchanged so
  the file's ten other legs see exactly what they saw before.
- `draw` (`:980`): per seed, one expired ordinary lease and one unreadable value (misfiled owned
  entry, `owner`-only torn literal, or garbage), then up to six more of any of five kinds, under
  distinct random chunk ids, each with a fragment on the D server. Expired leases include
  `lease == now` (the boundary). Every unreadable value carries an expired lease, so a misread
  would reclaim it.
- The test (`:1147-1161`): 32 seeds; each seed draws a Fisher–Yates order (the shape
  `crates/core/src/erasure.rs:258-280` uses) and runs `gc_pass_over` (`:1030`) twice — the drawn
  order and its reverse — each on fresh stores, through the real `reconcile_step`.
- Why order-then-reverse closes the adversary's finding **by construction**: a pass that stops at
  its first unreadable entry reaches only the entries before it. To reach every expired lease in
  both orders, each expired lease would have to sit before the first unreadable entry and also
  after the last one, which is impossible when both kinds are present. So every seed catches the
  mutant in at least one of its two passes, whatever order a store happens to use.
- Per pass it asserts: the pass returns `Ok` and `Changed`; every expired ordinary lease's
  fragment and `pending:` key are gone; every other entry's fragment is kept and its value is
  unchanged byte-for-byte; exactly one `unreadable-pending-entry` audit line per unreadable entry,
  each naming its key on target `wyrd.custodian.gc.audit`; one counter tick each. That keeps every
  assertion round 2 had (it asserted all of these for one entry).
- The module-doc note at `:23-26` now says "its audit lines" / "before its first pass runs".

### `crates/core/tests/multipart_owned_staging.rs` — S5 (core half)

- The named fixture gains an expired ordinary lease at `pending:16` (`:689`), which sorts after
  all three unreadable values (13, 14, 15) on redb's key-ordered scan. Round 2 only caught a
  stop-early sweep through the skipped-key list; now there is a reclaim after the skips too, and
  the test asserts `reclaimed == [11, 16]`.
- The same test then runs 64 seeded populations (`sweep_a_seeded_population`, `:580`; loop at
  `:731`) over the production redb adapter: random mixes of the same five kinds, random chunk ids
  (so unreadable values land before, between and after readable ones in key order), leases on
  the boundary included. For each seed it asserts every expired lease was reclaimed, every other
  value is byte-for-byte unchanged, and the error's `reclaimed` and `skipped` sets are exactly the
  expected keys.
- The torn kind is `owner`-only on purpose (doc at `:563-576`): the `staged`-only spelling is
  refused by the pairing rule alone, so including it would make S3's negation fail this test too
  and break the brief's one-negation-one-failure rule.
- `owner_only` / `staged_only` now take the lease (`:149`, `:157`); the S3 call sites pass
  `LEASE`, so S3 is unchanged.
- Module doc S5 bullet updated (`:41-43`).

### Doc-only production changes

- `crates/core/src/multipart.rs:97-107` (module header): an expiry sweep skips such a value and
  goes on; `write::sweep_expired_leases` reports it in its error; GC names it on its audit seam;
  neither runs by default (no production caller; GC reads `pending:` only with
  `--gc-expired-pending`, `crates/server/src/cli.rs:975-979`, `crates/custodian/src/gc.rs:172-175`).
- `crates/core/src/multipart.rs:3506-3511` (`OwnedEntry` doc): see the table.
- `crates/core/src/write.rs:641-645` (`sweep_expired_leases` doc): dropped "for a human"; says
  what reaches a human is whatever the caller does with the error.
- `crates/custodian/src/gc.rs:495-497` (`expired_pending_chunks` doc): only a `Reclaim` pass reads
  this; under `Defer`, the deployed default, nothing runs and the entry is left in place, unnamed.
- `docs/design/architecture/05-building-block-view.md:202`: "…is refused by every reader and both
  `pending:` writers, and an expiry sweep skips it without reclaiming its chunk; GC's
  expired-lease input, off unless an operator arms it, also names it on the audit seam." Also
  "skipped when absent" → "omitted when absent" so "skipped" means one thing in the sentence, and
  the placement clause reads "a D server per planned fragment, a count that decode leaves
  unchecked as for a committed chunk". Nothing else in the file changed.

## The T4 DST finding — what I did, and what the human may still need to decide

The rubric line is `AGENTS.md:188-190`: "a new destructive or concurrent path lands with seeded
Tier-0 DST coverage". The repo defines Tier 0 as "`testkit` and the commit-protocol property tests
(ADR-0009)" (`docs/design/architecture/10-quality-risks-glossary.md:103-105`), and `wyrd-core` and
`wyrd-custodian` both already take `wyrd-testkit` as a dev-dependency for exactly this ("Seeded
property tests reuse the deterministic simulator (ADR-0009)", `crates/core/Cargo.toml`). The
in-tree precedent outside `crates/dst` is `erasure.rs`'s `seeded_random_data_and_subsets_round_trip`.
Both new properties follow it: `wyrd_testkit::Sim` per seed, every failure message carries the
seed.

Why not madsim / `crates/dst`: the brief confines `crates/dst/tests/custodian.rs` to the mechanical
initializer (it is #722's rebase surface) and a fourteenth file is a STOP. Also, each sweep is one
sequential scan — there is no interleaving inside it for madsim's scheduler to vary — so what a
seed has to vary is the population and the scan order, which these properties do.

If T4 re-raises the finding anyway (for example "should be in `crates/dst`"), a rebuild cannot fix
it without breaking the brief. The human can record a rejection in `review-rejected.md`. The
format is `<file:line> | <CLASS> | <MATCH> | <reason>`, and the `file:line` must be the one the new
finding reports. A reason that fits:

```
<file:line from the finding> | TEST-GAP | seeded Tier-0 | Seeded Tier-0 coverage ships for both sweeps: wyrd_testkit::Sim properties in crates/core/tests/multipart_owned_staging.rs (s5_…, 64 seeds, redb) and crates/custodian/tests/gc.rs (expired_lease_input_skips_what_it_cannot_read_under_every_scan_order, 32 seeds × 2 scan orders); Tier 0 per 10-quality-risks-glossary.md:103-105. Brief #772 confines crates/dst/tests/custodian.rs to the mechanical initializer (#722 rebase surface).
```

I did not write `review-rejected.md` myself: the review script describes it as the human's
decisions file (`scripts/review-branch`, `load_rejected`).

## The eight isolating negations (the brief's list), plus extras

Method: `$PDCA_SCRATCH/pdca-builder-772-r3-neg/neg.py` applies one exact-string edit to one
production file, runs `cargo test -q -p wyrd-core --test multipart_owned_staging` (the invocation
`C4-verify` uses) or `cargo test -q -p wyrd-custodian --test gc` under `timeout 1500`, saves the
output, and writes the original bytes back. It checks each site matches exactly once, and compares
the hash of `git diff HEAD` before and after the whole run (unchanged). Run on the final code.

| # | Negation | Result | The one failing test and its message |
|---|---|---|---|
| S1 | `checked_staged_scheme` skips the `erasure::supported` check (`multipart.rs:3454`) | 13 passed; 1 failed | `s1_staged_geometry_the_erasure_coder_refuses_is_a_typed_error` — `rs(0,1) is geometry the coder refuses, never a value` — `left: Ok(OwnedEntry { … scheme: ReedSolomon { k: 0, m: 1 } … })`, `right: Err(StagedSchemeUnsupported { k: 0, m: 1 })` |
| S2 | `decode_owned_entry` skips `owner != key_owner` (`multipart.rs:3626`) | 13 passed; 1 failed | `s2_an_owner_other_than_the_keys_upload_id_is_refused` — `left: Ok(OwnedEntry { owner: UploadId("a1a1…") … })`, `right: Err(OwnedEntryOwnerMismatch { key_owner: UploadId("b2b2…"), entry_owner: UploadId("a1a1…") })` |
| S3 | `checked_ownership_pairing` always `Ok` (`multipart.rs:3381`) | 13 passed; 1 failed | `s3_a_torn_value_is_refused_under_both_namespaces` — `the pending: reading` — `left: Err(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })`, `right: Err(TornOwnedEntry { present: "owner", absent: "staged" })` |
| S4 | `renew_pending` back on the generic `decode` (`metadata.rs`, `renew_pending`, `:2141`) | 13 passed; 1 failed | `s4_renew_pending_refuses_a_misfiled_owned_entry` — `a misfiled owned entry was renewed as an ordinary lease: Ok(Committed)` |
| S5 | `sweep_expired_leases` aborts on the first unreadable value, before committing (`write.rs:657`) | 13 passed; 1 failed | `s5_the_lease_sweep_skips_what_it_cannot_read_and_completes_for_the_rest` — `one unreadable record must not stall the reclaim of an expired lease beside it` — `left: Some(<the expired lease's bytes>)`, `right: None` |
| S6 (reversed) | `StagedPlacement::new` rejects a placement whose length ≠ the scheme's fragment count | 13 passed; 1 failed | `s6_a_length_mismatched_staged_placement_decodes` — `a length-mismatched placement is liberal on read (ADR-0045 :45-49): StagedSchemeUnsupported { k: 0, m: 0 }` |
| S7 | `skip_serializing_if` removed from `PendingEntry.owner` only | 13 passed; 1 failed | `s7_an_ordinary_pending_entry_reencodes_byte_identically` — `left: "{\"lease_expiry_millis\":1500,\"owner\":null}"`, `right: "{\"lease_expiry_millis\":1500}"` |
| S8 | `decode_owned_entry` ignores its canonical-bytes gate (`multipart.rs:3633`) — **re-targeted, see below** | 13 passed; 1 failed | `s8_an_owned_entry_reencodes_byte_identically_across_a_renewal` — `a foreign spelling was accepted: {"owner":"a1a1…","lease_expiry_millis":1500,"staged":{…}}` — `left: Ok(OwnedEntry { … })`, `right: Err(NoncanonicalRecordValue { namespace: "sidx:" })` |

**S8: the brief's wording cannot bind, so the negation is re-targeted (unchanged from round 2).**
The brief's S8 negation is "remove one `skip_serializing_if` … on the owned witness". I ran that
too (S8-as-briefed: removed from `staged`): **only S7 fails**
(`left: "{\"lease_expiry_millis\":1500,\"staged\":null}"`); every owned-value test stays green.
It has to: serde checks `skip_serializing_if` only for a `None`, and an owned value has both
fields `Some`. What keeps an owned value's bytes stable across a renewal is the `sidx:` decode's
canonical-bytes gate, and removing that fails S8 alone.

**S9, negated by construction (unchanged).** S3's test builds the two torn literals an outside
caller can assemble (the fields are public, so they compile) and shows the public validator, both
decoders and both `pending:` writers refuse them. The GC property also stores such a literal
(`Drawn::Torn`, built from this other crate) and GC skips it.

Extras (the other readers, the writer gate's two rules, the sweep's report, and the GC half):

| # | Negation | Result |
|---|---|---|
| S4b | `live_lease_guards` back on the generic `decode` (`metadata.rs`, `:2177`) | 1 failed: `s4_a_leased_commit_refuses_over_a_misfiled_owned_entry` — `a commit was guarded by a misfiled owned entry read as a live lease: Ok(Committed)` |
| S4c | `sweep_expired_leases` reads through the generic `decode` | 1 failed: `s5_…` — the misfiled owned entry was reclaimed (`left: None`, `right: Some(<owned bytes>)`) |
| S5-break | `continue` → `break` in the core sweep's skip arm (`write.rs:657-660`) | 1 failed: `s5_…` — the named fixture's `pending:16` was left behind; `reclaimed: [11], skipped: [pending:13]` |
| S5-silent | the sweep returns `Ok(reclaimed)` over skipped entries (`write.rs:672`) | 1 failed: `s5_…` — `a sweep that skipped entries must not report success: [11, 16]` |
| W-put | `put_pending` drops its whole gate | 2 failed, one per rule the gate enforces: `s4_the_pending_writers_refuse_to_store_an_owned_entry` and `s3_…`. Each rule alone isolates: |
| W-put-namespace | `put_pending` applies only the pairing rule | 1 failed: `s4_the_pending_writers_refuse_to_store_an_owned_entry` — `an owned entry was stored under pending: Ok(Committed)` |
| W-put-pairing | `put_pending` applies only `owner.is_some()` | 1 failed: `s3_…` — `a torn entry was stored: Ok(Committed)` |
| GC-break | `continue` → `break` in GC's skip arm (`gc.rs:506-509`) — **the adversary's round-2 mutant** | 1 failed (gc binary, 10 passed): `expired_lease_input_skips_what_it_cannot_read_under_every_scan_order` — `seed 0`, `left: Satisfied`, `right: Changed` (in one of seed 0's two orders the pass stopped before any expired lease) |
| GC-abort | GC emits, then `?`-aborts on the unreadable value | 1 failed: same test — `seed 0: one unreadable pending entry must not fail the pass: reconciliation store access: pending entry carries `owner` but not `staged`…` |
| GC-generic | GC reads through the generic `decode` | 1 failed: same test — `seed 0: GC reclaimed chunk 8260's byte on a Misfiled value` |
| GC-silent | `emit_unreadable_pending` emits nothing | 1 failed: same test — `seed 0: one audit line per skipped entry. got: {…gc_fragments_reclaimed…}{…"action":"reclaim"…}` |

**Stability of the GC-break result** (`stability.py`: build once, run the test binary 20 times):
control (unmodified) failed **0 of 20**; the GC-break mutant failed **20 of 20**. Round 2's leg let
the same mutant pass 16 of 40.

**The seeded core populations catch a stop-early sweep on their own**, not only through the named
fixture's `pending:16`. Probe (`probe.py`, throwaway): `continue` → `break` in the core sweep, and
the S5 test edited to run only the 64 seeded populations. Result: 1 failed, at seed 0 —
`the expired lease on chunk 750 was left in place: Err(UnreadablePendingEntries { reclaimed: [],
skipped: [(pending:1214, TornOwnedEntry …)] })`. `pending:1214` sorts before `pending:750`, so the
sweep stopped before reaching it. Both files were restored byte-for-byte (hash of `git diff HEAD`
unchanged).

## Refuting my own test

**(a) Genuine red? Yes.**
1. *Whole fix reverted:* the named test cannot compile on base (it calls `decode_owned_entry`,
   `OwnedEntry`, `StagedPlacement`, `decode_pending_entry`, `WriteError::UnreadablePendingEntries`,
   and the new `PendingEntry` fields). `C4-verify` will report UNVERIFIABLE (exit 77), as the brief
   pre-declares for §6. Same for the GC leg.
2. *Behavioural red on base production code, re-run this round:* two throwaway probe test files
   (`base_probe.py`; sources kept in scratch, never in the patch) use only raw JSON and API that
   exists on both base and fix (`sweep_expired_leases`, `reconcile_step`, store calls). Run on the
   fixed tree, then with the four production files (`metadata.rs`, `multipart.rs`, `write.rs`,
   `gc.rs`) swapped to their `a1d48ed` bytes, then restored (byte-compared; `git diff HEAD` hash
   unchanged; probe files moved out of the worktree):
   - **fixed: core 3 passed, GC 3 passed.**
   - **base: core 0 passed / 3 failed, GC 0 passed / 3 failed:**
     - core: `owned-shaped value reclaimed as an ordinary lease: Ok([11, 13])`;
       `torn value reclaimed as an ordinary lease: Ok([11, 14])`;
       `expired lease stranded by one garbage value: Err(Error("expected ident", line: 1, column: 2))`.
     - GC: `owned-shaped value's byte reclaimed: Ok(Changed), [(225, false), (226, false)]`;
       `torn value's byte reclaimed: Ok(Changed), [(225, false), (227, false)]`;
       `one garbage value failed the pass: Err(Store(Error("expected ident", …))), [(225, true), (228, true)]`.
3. *Per rule:* the negation table — each of the brief's eight fails exactly one test, and every GC
   negation fails the GC property.

**(b) Production path? Yes.** Every assertion calls production code: `decode_owned_entry`,
`decode_pending_entry`, `StagedPlacement::new`, `OwnedEntry::{new, to_pending, from_pending}`,
`metadata::{encode, decode, put_pending, renew_pending, create_leased}` (→ `live_lease_guards`),
`write::sweep_expired_leases` over a real `RedbMetadataStore::in_memory()`, and GC through the real
`reconcile_step` → `gc::reconcile` → `expired_pending_chunks` → `emit_unreadable_pending`, captured
by a real `tracing` JSON subscriber. The only new double is `ScanInOrder`, which forwards every call
to the file's existing `MemMeta` and changes nothing but the order `scan` returns — an order the
`scan` contract allows.

**(c) Fixture includes the fault? Yes.** The fault is a misfiled, torn or garbage value under a real
`pending:` key, next to healthy entries, each unreadable value carrying an expired lease the reader
would act on if it misread it, and (for GC) a real fragment on the D server that a misread would
delete. Every seed has at least one unreadable value and at least one expired ordinary lease, and
the GC property puts them in both relative orders.

## Mutation run (C5's command, run locally)

`cargo mutants --in-diff <this patch> --no-shuffle` in the worktree (what `scripts/mutants-in-diff`
runs): **38 mutants tested in 63s: 9 caught, 29 unviable, 0 missed.** Same set as round 2 (this
round's production edits are comments). Unviable ones are body replacements that do not compile
under `warnings = "deny"`; for the ones in new code a manual negation above stands in
(`checked_ownership_pairing → Ok(())` = S3, `checked_staged_scheme → Ok(())` = S1,
`emit_unreadable_pending → ()` = GC-silent). cargo-mutants never generates `continue`→`break`,
which is why GC-break and S5-break are in the manual table.

## Gates run locally

- `cargo test -q -p wyrd-core --test multipart_owned_staging`: **14 passed** (0.31s).
- `cargo test -q -p wyrd-custodian --test gc`: **11 passed** (10 existing + the #772 property).
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets`:
  clean.
- Full gate `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`, the gating C4-ci row),
  run once on the final tree: **`xtask ci: all checks passed`, exit 0.** Steps in the log: `typos`,
  `lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK), gitlink guard, unsafe
  guard, `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo build`,
  `cargo test --workspace` (179 `test result: ok` lines, 0 failed), `cargo-machete`,
  `cargo deny check`, conformance vectors, statics gate, deploy guard, then the `--cfg madsim` DST
  clippy and tests (which compile the `crates/dst/tests/custodian.rs` ripple site).
- The target repo has no commit hooks configured (no `.pre-commit-config.yaml`, no
  `core.hooksPath`), so `cargo xtask ci` is the bar a publish commit must clear.
- `patch.diff` (13 files) applies cleanly to `a1d48ed` (`git apply --cached --check` against a
  scratch index read from `a1d48ed`) and reverse-applies to the worktree (`git apply -R --check`),
  so it is exactly the tree the gates ran on. The bundle's `multipart_owned_staging.rs` is
  byte-identical to the worktree's.
- Worktree state on hand-off: `HEAD` at `a1d48ed`, the patch in the working tree (new test file
  untracked). I used a temporary local commit of round 2's patch to see this round's changes on
  their own, then `git reset --mixed a1d48ed`; nothing was pushed.

## Size

+1,112 added semantic lines (non-blank, non-comment) against the brief's 1,250 budget; +1,784 raw.
Per file (semantic): `multipart.rs` 171, `metadata.rs` 57, `write.rs` 37, `gc.rs` 18, new test 592,
`custodian/tests/gc.rs` 216, the other six ripple files 2–6 each (all within the ≤ 8 changed lines
rule), docs 1. Up from round 2's 909: +100 in the new test (the seeded S5 populations and the
`pending:16` lease) and +103 in `custodian/tests/gc.rs`. That file is far over the brief's ≈35
estimate for the leg: roughly 20 lines are round 2's `Capture` harness, 35 are `ScanInOrder` (the
adversary's order finding), and the rest is the seeded population, the two-order passes and the
per-entry and audit assertions (the T4 finding). The brief's total budget still holds.

## Decisions carried over from rounds 1–2 (unchanged, still hold)

- **S9 mechanism: public fields + a public checked path** (`OwnedEntry::{new, to_pending,
  from_pending}`, `StagedPlacement::new`). Private fields cost 4 changed lines per literal site
  (12 in `mutation_regressions.rs` and in `server/tests/custodian_gc.rs`), over the brief's ≤ 8.
- **Write-side gate** (`PendingEntry::checked_ordinary_lease`, used by `decode_pending_entry`,
  `put_pending`, `renew_pending`), mirroring `InodeRecord::checked_for_publication`.
- **`sweep_expired_leases` returns `WriteError::UnreadablePendingEntries { reclaimed, skipped }`
  after committing the readable reclaims** — the only channel `write.rs` has (no `tracing` seam,
  and the brief forbids adding one). Changing the return type would touch seven caller files,
  five outside the brief's list.
- `PendingEntryWire` stays open (live corpus, mixed-version fleet); the owned shape is decoded
  closed and ends with the canonical-bytes gate.
- GC's skip does not turn the outcome into `Reconciled::Blocked` — the brief pins the
  `malformed-placement` / `emit_malformed` / `emit_unresolvable` precedent (skip + signal).

## Expected at Check

- `C4-verify`: GREEN leg passes; RED leg cannot compile → **UNVERIFIABLE (exit 77)**, pre-declared
  by the brief. The evidence standing in for it is the negation table and the base probe above.
- The custodian leg is outside `C4-verify`'s discriminator set; its evidence is the GC rows of the
  negation table, the 20/20 stability run, and `C4-ci`.
- `C5-mutants`: expected 0 missed (local run above).
- `T4`: the two round-2 blockers asked for seeded Tier-0 coverage, which now ships for both sweeps.
  If a pass still asks for `crates/dst`, see "The T4 DST finding" above.

## External dependencies

None missing. The build used only the base Rust toolchain plus the registered tools
(`cargo-mutants` present; `typos`, `cargo-deny`, `cargo-machete` and the docs renderer ran inside
`cargo xtask ci`). No `Cargo.toml` / `Cargo.lock` change: `wyrd-testkit` is already a
dev-dependency of both `wyrd-core` and `wyrd-custodian`.

## Scratch

Negation, probe and stability scripts and their logs are in
`$PDCA_SCRATCH/pdca-builder-772-r3-neg` (`neg.py <worktree> <logdir> all` re-runs the whole
table; `base_probe.py` and `stability.py` the other two checks). Left for the harness to reclaim
with `$PDCA_SCRATCH`; nothing was written outside the worktree, the bundle and `$PDCA_SCRATCH`
(cargo-mutants and the docs renderer put their temporary copies there, via `TMPDIR`). cargo-mutants
wrote `mutants.out/` at the worktree root (gitignored, `.gitignore:14`).
