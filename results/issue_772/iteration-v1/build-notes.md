# Build notes — #772 `multipart-owned-staging-entry`

Base: the per-cycle worktree `$PDCA_WORKTREE` at `a1d48ed` (origin/main with child-1 #771
merged — "Add decoder for multipart retirement obligation records"). Every `path:line` below is
on that base **with this patch applied**, unless marked "base". The brief's `multipart.rs` line
numbers (`:907`, `:1937`, `:2029`, `:1828`, `:96`) are from `c824243`, before child-1 landed; on
this base they are `sidx_key` `multipart.rs:1202`, `parse_sidx_key` `:1220`,
`checked_chunk_scheme` `:2232`, `EcSchemeWire` `:2326`, `decode_session_record` `:2123`,
`RecordError` `:108`. `metadata.rs` is cited by symbol as the brief asks (#776 may land there
first); the current lines are given in brackets only as a convenience.

13 files, exactly the brief's list: 4 substantive, 1 new test, 1 docs sentence, 7 mechanical
ripple files (one of which, `crates/custodian/tests/gc.rs`, carries the pre-declared S5 leg).
Added lines, non-blank and non-comment: multipart.rs 172, metadata.rs 48, write.rs 9, gc.rs 18,
new test 414, custodian/tests/gc.rs 61 (2 mechanical + the S5 leg), ripple 2–6 per file, docs 1
line. About 740 in total against the 1,250 budget (1,252 raw lines including doc comments).

## What changed, and why

### `crates/core/src/multipart.rs` — the owned entry and its key-taking decoder

- **Four `RecordError` variants** (`:458-499`, Display `:685-706`): `TornOwnedEntry { present,
  absent }` (S3), `PendingEntryNamespaceMismatch { namespace, shape }` (S4, both directions — one
  rule, one variant, the way `RetireTokenSuffixMismatch` carries both directions of its rule),
  `OwnedEntryOwnerMismatch { key_owner, entry_owner }` (S2), `StagedSchemeUnsupported { k, m }`
  (S1).
- **Section 10** (`:3343` onward):
  - `checked_ownership_pairing` (`:3369`) — the one home of the pairing rule, `pub(crate)`,
    called by `PendingEntry`'s `TryFrom` (metadata.rs), by `decode_owned_entry`, and by
    `OwnedEntry::from_pending`.
  - `StagedPlacement` (`:3414`) with the closed `StagedPlacementWire` (`:3388`) that reads its
    scheme through the existing `EcSchemeWire` (`:2326`); `StagedPlacement::new` is the one
    checked constructor, and `checked_staged_scheme` (`:3444`) mirrors `checked_chunk_scheme`
    (`:2232`) on the same predicate, `erasure::supported` (`erasure.rs:120`). I did **not** fold
    the two into one shared helper: that is a refactor of #716's function, and the S1 negation
    would then also fail `multipart_session_records.rs` and `multipart_retire_obligation.rs`.
    Placement length is deliberately unchecked (S6).
  - `OwnedEntry` (`:3502`) — private fields; `new` (total), getters, `to_pending` (`:3537`),
    `from_pending` (`:3552`, the public validator).
  - `decode_owned_entry(key, value)` (`:3597`) — returns `(PartNumber, ChunkId, OwnedEntry)`,
    the key-taking shape of child-1's `decode_retire_obligation` (`:3328`). Order: parse key →
    closed wire (`OwnedEntryWire`, `:3475`) → pairing → namespace (ordinary shape refused,
    `:3609`) → geometry → owner against key (`:3615`) → canonical-bytes gate (`:3622`).
- **Module header**: the opening paragraph (the owned entry is now landed, not "the next
  child's"), a new paragraph under "Two records break that shape" (`:25-38`) explaining the
  one-entry-point-per-namespace design, the `sidx:` key-table row, and a new closing paragraph
  in the "Nothing here is written yet" section (`:96-101`) saying which live path this child
  touches. Child-1's corrected living-doc clause is left as it was.

### `crates/core/src/metadata.rs` — `PendingEntry`, and the `pending:` decode entry point

- `PendingEntry` (symbol; [`:1588`]) gains `owner: Option<UploadId>` and
  `staged: Option<StagedPlacement>`, both `#[serde(default, skip_serializing_if =
  "Option::is_none")]` exactly as `0016:442-457` spells them (with `UploadId` instead of `String`,
  per the brief), drops `Copy`, and decodes through `PendingEntryWire` (symbol; [`:1608`]) whose
  `TryFrom` applies the pairing rule — the `InodeRecord`/`InodeRecordWire` precedent. The doc
  comment records S7's correction: `0016:475-485` says the pending CASes compare the
  **re-encoded** prior; the code (`renew_pending`: `require(key, current)` +
  `put(key, encode(entry))`) preconditions on raw bytes and puts the caller's entry, so a
  non-identity encoder would make the CAS **win** and silently rewrite. The proposal is not
  edited.
- `decode_pending_entry` (symbol; [`:1641`]) — **new**, the `pending:` namespace's one decode
  entry point. Returns `Result<PendingEntry, RecordError>`: malformed → `MalformedRecordValue {
  namespace: "pending:" }`; torn → `TornOwnedEntry`; any ownership field present →
  `PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" }`. The namespace check
  is `owner.is_some() || staged.is_some()`, not only `owner.is_some()`, so the `pending:` path
  refuses a staged-only value even if the pairing rule were ever weakened.
- `renew_pending` and `live_lease_guards` (symbols; [`:2106`], [`:2142`]) now read through it.
  Their callers are untouched: the `RecordError` goes out through `?` as the crate's boxed error.
- The in-file `segmented_shape_invariants` test constructor (symbol; [`:3521`]) gains the two
  `None` initializers — the ninth site the brief pre-declares.

### `crates/core/src/write.rs`

- Three literals (`:209`, `:437`, `:500`) gain `owner: None, staged: None`.
- `sweep_expired_leases` (`:644`, skip at `:652`) reads through `decode_pending_entry` and
  **skips** on `Err` (`let … else { continue }`). No tracing added, as the brief directs; the doc
  comment says why the skip carries no signal of its own (this is the test-invoked stand-in; it
  has no production caller — `grep` finds none under `crates/*/src` — and the production sweep
  is GC's, which does signal).

### `crates/custodian/src/gc.rs`

- `expired_pending_chunks` (`:492`, decode at `:498`) reads through `decode_pending_entry`;
  on `Err` it calls the new `emit_unreadable_pending` (`:594`) and `continue`s. The chunk never
  enters the expired set, so none of its fragments is reclaimed on that lease and its `pending:`
  entry is never deleted (only `swept_pending` entries are deleted, gc.rs `reconcile`).
  `emit_unreadable_pending` follows `emit_unresolvable` (`:578`) / `emit_malformed` (`:554`):
  a `monotonic_counter.gc_unreadable_pending_entries` plus an audit event on
  `wyrd.custodian.gc.audit`, the key escaped by the existing `object_name`.
- The `PendingEntry` import is dropped (type now inferred from `decode_pending_entry`).

### Docs — `docs/design/architecture/05-building-block-view.md:202`

One inserted passage (three sentences, about 125 words, close to the ADR-0047 bullet at
`:187-194`) placed after child-1's retirement-obligation sentence and before "Nothing writes or
consumes these records in production yet". It adds the `sidx:` namespace, the two optional
ownership fields and their omit-when-absent rule, and the namespace-agreement rule with the
sweeps' skip. Nothing else in the file changes.

### Ripple (mechanical, `owner: None, staged: None` only)

`crates/core/tests/mutation_regressions.rs:226,237,574`, `crates/custodian/tests/gc.rs:205`,
`crates/custodian/tests/restore_reconcile.rs:386`,
`crates/custodian/tests/segmented_map_consumers.rs:500`, `crates/dst/tests/custodian.rs:925`,
`crates/metadata-redb/tests/conformance.rs:119`, `crates/server/tests/custodian_gc.rs:645,728,924`.
Two added lines per site, at most 6 per file. No logic change, no new function. The three
decode-only external sites (`mutation_regressions_round2.rs:123`, `stream_lease_renewal.rs:79`,
`server/tests/gateway_lease_expiry.rs:158`) keep compiling unchanged: they read ordinary entries
through `metadata::decode::<PendingEntry>`, which still accepts that shape.

### Tests

- New `crates/core/tests/multipart_owned_staging.rs` (13 tests): S1–S4, S5 (core), S6–S9, plus
  a round-trip positive and a closed-wire check.
- `crates/custodian/tests/gc.rs:896` — `expired_lease_input_skips_a_misfiled_owned_entry_and_completes_for_the_rest`
  (S5, GC half). It mints its misfiled owned entry through the public checked path from the
  custodian crate, so it is also a second cross-crate S9 site.

## Decisions, and alternatives I rejected (with cost)

**S9 mechanism: public fields + a public checked path, not private fields.** The owned shape is
minted through `StagedPlacement::new` → `OwnedEntry::new` → `to_pending()`, and a hand-assembled
record is checked by `OwnedEntry::from_pending`. All of it is public, and all of it is exercised
from outside `crates/core/src/`: the named test is its own crate and sees only the public API,
and the custodian GC leg is a second, different crate.
- *Rejected: private `owner`/`staged` with constructors.* An external literal
  `&PendingEntry {\n lease_expiry_millis: X,\n }` becomes `&PendingEntry::lease(X)`, which is 3
  deleted + 1 added = 4 changed lines per site. `mutation_regressions.rs` and
  `server/tests/custodian_gc.rs` have 3 sites each, so 12 changed lines each, over the brief's
  "≤ 8 changed lines per file". The public-field ripple is 2 added lines per site (6 max per
  file). It would also be at odds with `0016:442-457`, which spells the fields `pub`.
- *Rejected: dropping `PendingEntry`'s `Deserialize` (the `RetirePayload` precedent).* It breaks
  the 3 decode-only external sites above, which would make 16 files and trip the brief's STOP
  condition on a fourteenth file.
- *Rejected: making `PendingEntry`'s own `Deserialize` refuse the owned shape (so the generic
  decode is the `pending:` decode).* Then `decode(encode(owned.to_pending()))` fails: the type
  cannot read what its own encoder writes. It would also leave S4's negation ("fall back to the
  generic decode") nothing to bind.

**`PendingEntryWire` stays open (no `deny_unknown_fields`).** The v3 salvage closed it. I kept the
`pending:` wire as open as its derive always was. Closing it is a format change on a live
namespace across a mixed-version fleet (a future build's extra field would make every older
build's sweeps skip, and its renewals and commits error, on every lease), and the brief asks
that the legacy path be provably unchanged. No CAS on `pending:` depends on decode→encode of the
stored value (`renew_pending` puts the caller's entry; `live_lease_guards` pins raw bytes), so
closure buys no identity there. The owned shape, which has no corpus yet, is decoded **closed**
by `decode_owned_entry` and ends with the canonical-bytes gate. This is documented on
`PendingEntry`.

**Not done, deliberately: a write-side guard in `put_pending` / `renew_pending`.**
`put_pending(store, c, &owned.to_pending())` would still write an owned shape under `pending:`,
which the readers now refuse (the sweeps skip it forever; renew and commit error on it). It is a
real hole but a writer concern. The brief scopes this child to readers ("no `pending:` reader
accepts an owned shape and no `sidx:` reader accepts a legacy one"), keeps `metadata.rs` hunks
minimal for #776's rebase, and puts every `sidx:` writer in #656–#659. No producer of an owned
entry exists in the tree. Sketch if wanted: a `fn checked_pending_shape(&PendingEntry) ->
Result<(), RecordError>` extracted from `decode_pending_entry`'s namespace check and called at
the top of `put_pending` and `renew_pending` — about 8 lines plus one test. Flag for the human:
worth a follow-up issue beside #656.

**Not done: a per-fragment `emit_skip` reason in GC's fleet walk.** The misfiled entry's
fragments fall through to the walk's existing "no evidence, keep" branch. Adding a
`"unreadable-pending-entry"` skip reason would mean threading the unreadable set out of
`expired_pending_chunks` into `reconcile`. The per-record audit event gives the operator the key
to repair, which is what `emit_unresolvable` does for its class too.

**Canonical-bytes gate on `sidx:`.** `decode_owned_entry` closes with `require_canonical`, as
every `decode_*` in the module does. Not on the `pending:` path (live corpus; no precedent there).

## The eight isolating negations (brief's list), plus two supplementary

Method: saved the fixed files to scratch, applied one negation per run with a script, ran
`cargo test -q -p wyrd-core --test multipart_owned_staging` (the invocation `C4-verify` uses,
bounded with `timeout 900`), restored, and checked the diff was byte-identical to the fixed diff
afterwards. Each negation fails **exactly one** test — "12 passed; 1 failed" on every run.

| # | Negation | The one failing test | Failure message |
|---|---|---|---|
| S1 | `checked_staged_scheme` drops the `erasure::supported` check | `s1_staged_geometry_the_erasure_coder_refuses_is_a_typed_error` | `rs(0,1) is geometry the coder refuses, never a value` — `left: Ok(OwnedEntry { … scheme: ReedSolomon { k: 0, m: 1 } … })`, `right: Err(StagedSchemeUnsupported { k: 0, m: 1 })` |
| S2 | `decode_owned_entry` drops the `owner != key_owner` comparison | `s2_an_owner_other_than_the_keys_upload_id_is_refused` | `left: Ok(OwnedEntry { owner: UploadId("a1a1…") … })`, `right: Err(OwnedEntryOwnerMismatch { key_owner: UploadId("b2b2…"), entry_owner: UploadId("a1a1…") })` |
| S3 | `checked_ownership_pairing` forced to `Ok` | `s3_a_torn_value_is_refused_under_both_namespaces` | `the pending: reading` — `left: Err(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })`, `right: Err(TornOwnedEntry { present: "owner", absent: "staged" })` |
| S4 | `renew_pending` falls back to `let existing: PendingEntry = decode(&current)?` | `s4_renew_pending_refuses_a_misfiled_owned_entry` | `a misfiled owned entry was renewed as an ordinary lease: Ok(Committed)` |
| S5 | `sweep_expired_leases` `?`-aborts: `let entry = metadata::decode_pending_entry(&value)?` | `s5_the_lease_sweep_skips_what_it_cannot_read_and_completes_for_the_rest` | `one unreadable record must not stall the sweep: PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" }` |
| S6 (reversed) | `StagedPlacement::new` rejects a placement whose length ≠ the scheme's fragment count | `s6_a_length_mismatched_staged_placement_decodes` | `a length-mismatched placement is liberal on read (ADR-0045 :45-49): StagedSchemeUnsupported { k: 0, m: 0 }` |
| S7 | `skip_serializing_if` removed from `owner` (one attribute) | `s7_an_ordinary_pending_entry_reencodes_byte_identically` | `decode->encode is not byte-identical for PendingEntry { lease_expiry_millis: 1500, owner: None, staged: None }` — `left: "{\"lease_expiry_millis\":1500,\"owner\":null}"`, `right: "{\"lease_expiry_millis\":1500}"` |
| S8 | `decode_owned_entry` drops `require_canonical` (see below) | `s8_an_owned_entry_reencodes_byte_identically_across_a_renewal` | `a foreign spelling was accepted: {"owner":"a1a1…","lease_expiry_millis":1500,"staged":{…}}` — `left: Ok(OwnedEntry { … })`, `right: Err(NoncanonicalRecordValue { namespace: "sidx:" })` |
| S4b (extra) | `live_lease_guards` falls back to the generic decode | `s4_a_leased_commit_refuses_over_a_misfiled_owned_entry` | `a commit was guarded by a misfiled owned entry read as a live lease: Ok(Committed)` |
| S5-GC (extra) | `expired_pending_chunks` emits and then `?`-aborts (`decode_pending_entry(&value).inspect_err(emit…)?`), run as `cargo test -p wyrd-custodian --test gc` | `expired_lease_input_skips_a_misfiled_owned_entry_and_completes_for_the_rest` ("10 passed; 1 failed") | `one unreadable pending entry must not fail the pass: Store(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })` |

A first try at S5-GC deleted the `emit_unreadable_pending` call along with the skip. That made
the function dead code, which the workspace denies, so the run failed to compile rather than
failing a test. The recorded negation keeps the emit and only swaps the skip for `?`.

**S8: why its negation is not "remove one `skip_serializing_if`".** The brief lists S8's
negation as "same, on the owned witness". I ran exactly that: removing the `skip_serializing_if`
on `staged` (the one S7 did not remove) fails **only** the S7 test (`left:
"{\"lease_expiry_millis\":1500,\"staged\":null}"`), and every owned test stays green. It has to:
serde consults `skip_serializing_if` only for a `None`, and an owned value has both fields
`Some`. A leg that stays green under its own negation is not binding (the brief's own rule), so
I rewrote S8 around the one mechanism that actually holds identity for an owned value: the
`sidx:` decode's canonical-bytes gate. Without it, a foreign spelling (fields reordered,
whitespace inserted) is accepted, and a renewal — which preconditions on the raw bytes and puts
a freshly encoded entry — would rewrite the spelling along with the lease, which is S8's hazard
("changes only `lease_expiry_millis`"). The S8 test asserts both halves: a canonical owned
entry's renewal changes only the lease digits, and the two foreign spellings are refused.

**S9, negated by construction.** S3's test builds the torn literals an outside caller could
assemble without the checked path (`PendingEntry { owner: Some(..), staged: None, .. }` and the
reverse — they compile, since the fields are public). It asserts `OwnedEntry::from_pending`
refuses both with `TornOwnedEntry`, and so do `decode_pending_entry` and `decode_owned_entry` on
the bytes `metadata::encode` writes for them: a producer that wrote one would have written a
record nothing can read back. The S9 test itself shows the checked path produces exactly the
hand-authored owned bytes and that both decoders agree on them. The torn-literal half lives in
S3's test so that S3's negation fails one test, not two.

## Refuting my own test

**(a) Genuine red? Yes, in two forms.**
1. *Whole fix reverted.* The named test cannot compile on base: it calls `decode_owned_entry`,
   `OwnedEntry`, `StagedPlacement` and `decode_pending_entry`, which this patch adds. `C4-verify`
   will report UNVERIFIABLE (exit 77), as the brief pre-declares for §6.
2. *The production readers on base, driven by the same assertions.* Because (1) yields no
   verdict, I wrote a scratch probe (`crates/core/tests/zz_probe_772.rs`, never shipped, deleted
   afterwards) that makes the S4/S5 reader assertions using only API the base has (raw JSON
   bytes, `renew_pending`, `create_leased`, `sweep_expired_leases`). I ran it with every tracked
   file restored to `HEAD` (`git checkout HEAD -- crates docs`, the new test set aside), then with
   the fix restored (diff verified byte-identical to the saved fix diff):
   - **base: `0 passed; 4 failed`.**
     `renewed a misfiled owned entry: Ok(Committed); stored now Some("{\"lease_expiry_millis\":4500}")`
     — the owned entry's `owner`/`staged` erased, exactly the `metadata.rs` `renew_pending`
     hazard the brief names. `committed over a misfiled owned entry: Ok(Committed)`.
     `sweep result: Ok([11, 13])` — the misfiled owned entry reclaimed as an ordinary lease.
     `sweep result: Err(Error("expected ident", line: 1, column: 2))` — one garbage value aborts
     the whole sweep.
   - **fixed: `4 passed; 0 failed`.**

   The GC half's red on base is the S5-GC negation above: with the skip turned into a `?` the
   pass fails. Separately, on base the generic, open decode reads the misfiled bytes as an
   ordinary lease (the unknown fields are ignored), which is the `Ok([11, 13])` reclaim the core
   probe shows.

**(b) Production path? Yes.** Every assertion calls production code: `decode_owned_entry`,
`decode_pending_entry`, `StagedPlacement::new`, `OwnedEntry::{new, to_pending, from_pending}`,
`metadata::{encode, decode}`, and the real readers `renew_pending`, `create_leased` (→
`live_lease_guards`), `write::sweep_expired_leases` over a real `RedbMetadataStore::in_memory()`,
and GC through the real `reconcile_step` over the existing test's `MemMeta`/`MemDServer` trait
doubles (the same doubles every GC leg in that file uses; the code under test is production
`gc::reconcile`). Nothing is re-implemented in the test.

**(c) Fixture includes the fault? Yes.** The fault is a misfiled or torn value in a real store
under a real `pending:` key, next to healthy entries. The S5 core fixture holds one expired
ordinary lease, one live ordinary lease, a misfiled owned entry, a torn value and a garbage value,
all with leases the sweep would act on. The GC fixture holds an expired ordinary lease and an
expired misfiled owned entry, each with a real fragment on the D server. The readers are asserted
both to refuse the fault and to still work on the healthy entry beside it (the positive controls
in both S4 reader tests and in both S5 tests).

## Gates run locally

- `cargo test -p wyrd-core --test multipart_owned_staging`: 13 passed.
- `cargo test -p wyrd-custodian --test gc`: 11 passed (10 existing + the S5 leg).
- `cargo fmt --all` then `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server -p wyrd-metadata-redb --all-targets`
  (workspace lints, `clippy::all = deny`, `warnings = deny`): clean.
- `typos` on every touched file: clean. `python3 docs/publishing/tools/lint_docs.py`: OK
  (renderer deps present).
- Full gate `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`), run on the final
  tree: **`xtask ci: all checks passed`, exit 0.** Steps seen in the log: `typos`,
  `lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK), gitlink guard, unsafe
  guard, `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo build`,
  `cargo test --workspace` (179 `test result: ok` lines, none failed — including
  `multipart_owned_staging` and custodian `gc`), `cargo-machete`, `cargo deny check` (and the
  all-features advisories / licenses / bans / sources rows), then `cargo clippy` and
  `cargo test -p wyrd-dst` under `--cfg madsim`, which compiles and runs the
  `crates/dst/tests/custodian.rs` ripple site.
- After the full gate I reworded one clause of the architecture-doc passage (a torn value
  disagrees with itself, not with its key, so the list now reads "fields disagree with each
  other or with its key"). Prose only; I reran `typos`, `lint_docs.py` and
  `render_site.py --check` (link audit OK) on it. No code changed after the gate run.
- `patch.diff` (13 files) applies cleanly to `HEAD` `a1d48ed` (`git apply --cached --check`
  against a scratch index) and reverse-applies to the worktree, so it is exactly the final tree.
- `C5-mutants` (cargo-mutants) is not part of `cargo xtask ci`; I did not run it. Check runs it
  on the bundle diff.

## Expected at Check (pre-declared by the brief)

- `C4-verify`: the added test file is the discriminator. The GREEN leg (`cargo test -p
  wyrd-core --test multipart_owned_staging`) passes. The RED leg reverts production and the test
  **fails to compile** (it names API this patch adds), so the gate reports **UNVERIFIABLE (exit
  77)**. That is expected and goes to §6 for sign-off; the evidence standing in for it is the
  negation table and the base probe above.
- The custodian S5 leg is outside `C4-verify`'s discriminator set; its evidence is the S5-GC
  negation above plus `C4-ci`.

## Salvage from `results/issue_717/iteration-v3/patch.diff`

Taken, then adapted: the `StagedPlacement`/`StagedPlacementWire`/`checked_staged_scheme` shape,
the `OwnedEntry` view and its `new`/`to_pending`/`from_pending`, `checked_ownership_pairing`,
`decode_owned_entry`'s order and canonical gate, the `PendingEntryWire` `TryFrom` and the
`owner: None, staged: None` ripple. Not taken: v3's retire code (child-1 has landed its own),
v3's `NotAnOwnedEntry` (replaced by the two-direction `PendingEntryNamespaceMismatch`), the
closed `PendingEntryWire` (see above), and v3's explicit statement that an owned shape under
`pending:` "still decodes" — the reading this child exists to close. Added beyond v3: the
`pending:` entry point and the four readers going through it (S4), both sweeps' skip (S5),
public cross-crate minting exercised from two external crates (S9), and the living-doc sentence.
