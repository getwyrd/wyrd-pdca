# Build notes — #772 `multipart-owned-staging-entry`, round 2

Base: the per-cycle worktree `$PDCA_WORKTREE` at `a1d48ed` (origin/main with child-1 #771 merged).
Every `path:line` below is on that base **with this patch applied**, unless marked "base".
`metadata.rs` lines are given for convenience only; the brief asks for `metadata.rs` to be cited
by symbol (#776 may land there first), so each one names its symbol.

Round 2 starts from round 1's patch (`iteration-v1/patch.diff`, which the adversary could not
refute on its core) and changes only what the carry-forward and the T4 review flagged. Same 13
files as round 1, exactly the brief's list: 4 substantive, 1 new test, 1 docs sentence, 7 ripple
files (one of them, `crates/custodian/tests/gc.rs`, carries the pre-declared S5 leg).

## What round 1 left open, and what this round did about each

| Round-1 finding | Source | What changed |
|---|---|---|
| `pending:` writers still accept an owned or torn entry (`metadata.rs:1596` BUG) | T4 (blocking) + adversary `[human]` | **Fixed.** Write-side gate, below. |
| `write.rs` sweep skips silently (`write.rs:650`, `:652`, `:650` CONVENTION ×3) | T4 (blocking) | **Fixed without a logging seam.** The sweep still skips and completes, then returns an error naming what it skipped. |
| GC skip "does not surface an error or repair obligation" (`gc.rs:491` CONVENTION) | T4 (blocking) | **Behaviour kept (brief-mandated), reasoning now in the code, and the signal is now tested.** See "If T4 re-raises the GC finding" below. |
| Nothing tests the GC skip's audit signal | adversary `[impl]` | **Fixed.** The GC leg captures the audit output and asserts it. |
| `OwnedEntry::from_pending`'s ordinary-shape arm never runs | adversary `[impl]` | **Fixed.** S9 asserts it. |
| S10 docs wording (geometry vs length; "typed error at every reader") | adversary `[impl]` | **Fixed** as suggested, plus the writer rule. |
| C5: one surviving mutant, `\|\|` → `&&` at old `metadata.rs:1650` (equivalent) | C5 (advisory) | **Removed.** The check after the pairing rule now reads one field. Local `cargo mutants --in-diff`: 38 mutants, 9 caught, 29 unviable, **0 missed**. |
| C2 / C4-verify UNVERIFIABLE (RED leg cannot compile) | pre-declared by the brief | Unchanged by design; the negation table and the base probe below stand in for it. |
| T5 prior art; Validation fitness-to-purpose | human | Nothing for Do. |

`deferred-findings.json` item 2 ("The `pending:` writers still accept an owned or torn entry") is
the first row: fixed here, so the human can clear it.

## What changed this round, and why

### 1. The `pending:` write-side gate — `crates/core/src/metadata.rs`

- New private method `PendingEntry::checked_ordinary_lease` (`metadata.rs:1645`): the `pending:`
  namespace's shape rule in one place. Pairing rule first (`checked_ownership_pairing`, so a torn
  value is refused as `TornOwnedEntry`), then `if self.owner.is_some()` →
  `PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" }`.
- Called from three places, so reader and writer cannot disagree:
  `decode_pending_entry` (`metadata.rs:1680`), `put_pending` (`metadata.rs:2071`) and
  `renew_pending` (`metadata.rs:2132`, after the existing empty-slice early return, so the documented
  "an empty slice is a no-op" still holds; no bytes are stored in that case, so there is nothing
  to protect).
- This mirrors the precedent in the same file: `InodeRecord::checked_for_publication`
  (`metadata.rs:1502`) — "`size` and `chunk_map` are independent public fields … so a caller *can*
  hand `create` a record … Encoding that record would put bytes in the store that this very type
  refuses to decode … So the check happens where the record becomes durable". Same situation,
  same answer.
- No signature change: both writers already return the crate's boxed error, and `RecordError`
  converts through `?`. No caller changes. The only production writers of `pending:` values are
  these two (`grep` over `crates/*/src`: `metadata.rs:2071` put, `:2141`–`:2146` renew put).
- Doc: `PendingEntry` gains a paragraph on the write side (`metadata.rs:1566-1570`); `put_pending`
  and `renew_pending` docs say what they refuse.
- The old `decode_pending_entry` check `owner.is_some() || staged.is_some()` is gone; that was the
  C5 equivalent mutant (after the pairing rule, `||` and `&&` agree on every reachable input).

Why fix it here rather than decline with a follow-up (round 1 declined): it is small (one method
plus two one-line calls and doc), it closes exactly the "bytes nothing can read back" hazard S9
exists for, reached through the `pending:` writer instead of a `sidx:` one, and it was a gating
T4 BUG. The hunks stay inside the four existing `metadata.rs` hunk areas; no new hunk region.

### 2. The `write.rs` sweep reports what it skipped — `crates/core/src/write.rs`

- `sweep_expired_leases` (`write.rs:645`) still classifies each value through
  `decode_pending_entry`, skips an unreadable one (`write.rs:656-659`: collected, not reclaimed,
  not deleted), and commits the reclaim of every readable expired lease. Then, if it skipped
  anything, it returns `WriteError::UnreadablePendingEntries { reclaimed, skipped }`
  (`write.rs:671-673`; variant `write.rs:703`; message `write.rs:724`), naming each skipped key
  with its fault and carrying the reclaimed chunk ids so nothing the sweep did is lost.
- Why: ADR-0045 decision 3 (`0045:55-59`) says maintenance loops "MUST classify, skip, and emit
  NEEDS-HUMAN". Round 1 did classify and skip but emitted nothing; the rubric's "never silent
  skip" finding was right on the facts. The brief forbids adding a `tracing` seam to `write.rs`
  (S5: "do not add a logging seam to that module"), so the emission goes through the one channel
  the function already has: its result. No logging seam added; no caller changed.
- Existing callers are unaffected: seven test files call this function (no production caller —
  `crates/custodian/src/gc.rs:6` calls it "the test-invoked stand-in"), and none of them stores
  an unreadable `pending:` value (on base that would already have aborted the sweep with `Err`).

Alternatives rejected, with cost:
- *Change the return type to carry the skipped set.* Seven caller files
  (`crates/core/tests/{mutation_regressions,stream_lease_lapse,stream_lease_renewal}.rs`,
  `crates/server/tests/{write_path,dst_commit}.rs`, `crates/dst/tests/network.rs`) — at least five
  files outside the brief's list, which is its STOP condition.
- *Add `tracing` to `write.rs`.* Forbidden by the brief.
- *Abort on the first unreadable value.* Forbidden by S5 (a stalled sweep).
- *Keep the silent skip and ask the human to record a rejection.* All three T4 passes flagged it,
  so it would fail the gating T4 row again and burn another round.

### 3. GC: doc only, plus a test for the signal — `crates/custodian/src/gc.rs`, `crates/custodian/tests/gc.rs`

- Behaviour unchanged from round 1: `expired_pending_chunks` (`gc.rs:496`) skips an unreadable
  value (`gc.rs:505`) and names it through `emit_unreadable_pending` (`gc.rs:598`: a
  `monotonic_counter.gc_unreadable_pending_entries` tick plus a `wyrd.custodian.gc.audit` event
  naming the key). That is the brief's instruction for S5 ("mirror the skip-and-attribute
  precedents already there — the `malformed-placement` skip reason (`gc.rs:310`), `emit_malformed`
  (`gc.rs:558`), `emit_unresolvable` (`gc.rs:582`)").
- The doc on `expired_pending_chunks` (`gc.rs:484-495`) now says in so many words why it is
  neither an error (a `?` would fail the whole pass and every other reclaim in it — the stall
  ADR-0045 decision 3 rules out) nor a written repair record (the unreadable entry itself stays
  in place and every pass that reads this input names it again).
- The GC leg (`crates/custodian/tests/gc.rs:926`) now captures the audit output with the in-tree
  pattern (`Capture` at `tests/gc.rs:898`, copied from `segmented_map_consumers.rs:294-319`;
  `.with_subscriber(...)` as at `segmented_map_consumers.rs:665`), installs the #214 global-default
  guard first (`tests/gc.rs:930`), and asserts: exactly one `unreadable-pending-entry` line, on
  target `wyrd.custodian.gc.audit`, naming `pending:226` (the misfiled chunk `0xE2`), and exactly
  one counter tick. The adversary's round-1 probe (body of `emit_unreadable_pending` replaced by
  `let _ = (entry, fault);`) now fails this leg — negation GC-silent below.
- Only this test in the `gc` binary hits `emit_unreadable_pending`, so no sibling can latch its
  callsites; the guard is there anyway, as the pattern's doc asks. The file's module doc
  (`tests/gc.rs:19-26`) said telemetry read-back lives in `gc_telemetry.rs` because of #214; it
  now carries one sentence saying why this leg may read its one audit line here, so the two do
  not contradict each other.

### 4. Smaller changes

- `RecordError::PendingEntryNamespaceMismatch`'s message (`multipart.rs:693`) said "is stored
  under", which is wrong now that the writers raise it for a value they refuse to store. Now:
  "a `{namespace}` value is never an {shape} pending entry, and this one is". Its doc
  (`multipart.rs:467-479`) mentions the writer case. No test asserts the old text.
- `checked_ownership_pairing`'s doc (`multipart.rs:3361-3373`) lists the `pending:` rule among the
  seams that apply it.
- The sweep error's message (`write.rs:724`): "lease sweep skipped N pending-ledger value(s) it
  could not read as an ordinary lease, leaving each in place (first: `<key>`: `<fault>`); M
  expired lease(s) beside them were reclaimed". The key is rendered with `escape_ascii`, which is
  injective, for the reason `gc.rs`'s `object_name` escapes rather than renders lossily.
- `multipart.rs` module header: the "one decode entry point per namespace" paragraph
  (`multipart.rs:32-39`) and the live-path paragraph (`multipart.rs:97-104`) now say the
  `pending:` writers apply the same rule and the sweeps name what they skip.
- S10 (`docs/design/architecture/05-building-block-view.md:202`): the adversary's two wording
  points, plus the writers. The `staged` gloss now reads "an erasure scheme the coder must
  support, and one D server per planned fragment, a length decode leaves unchecked as it does for
  a committed chunk" (geometry checked, length not — S6), and "is refused by every reader and by
  both `pending:` writers, and the two `pending:` expiry sweeps skip it, reclaiming nothing on it,
  and name it for an operator" (no "typed error" promise for the boxed-error readers). Nothing
  else in the file changed.

### Tests — `crates/core/tests/multipart_owned_staging.rs` (14 tests; round 1 had 13)

- S3 (`:320`) additionally hands both torn literals to `put_pending` and `renew_pending`, and
  asserts both refuse and the store is unchanged.
- New S4 writer test (`:508`): an owned entry minted through the public path is refused by both
  writers; nothing is stored, the live lease is left byte-for-byte; the same writes with an
  ordinary entry still land.
- S5 (`:560`): asserts the store state first (the sweep completed: expired lease gone, live lease
  kept, the three unreadable values kept byte-for-byte), then that the result is
  `WriteError::UnreadablePendingEntries` with `reclaimed == [11]` and skipped keys
  `pending:13, pending:14, pending:15`, and that the message names `pending:13`. It asserts keys,
  not per-key faults, so the S3 negation cannot also fail it.
- S9 (`:708`): `from_pending` on an ordinary literal →
  `PendingEntryNamespaceMismatch { namespace: "sidx:", shape: "ordinary" }`.
- Small helpers `minted()` (`:100`) and `ordinary_entry()` (`:107`) replace repeated literals.

## Decisions carried over from round 1 (unchanged, still hold)

- **S9 mechanism: public fields + a public checked path.** Private fields were costed again: a
  literal `&PendingEntry {\n lease_expiry_millis: X,\n }` becomes `&PendingEntry::lease(X)` — 3
  deleted + 1 added = 4 changed lines per site, so 12 in each of `mutation_regressions.rs` and
  `server/tests/custodian_gc.rs` (3 sites each), over the brief's "≤ 8 changed lines per file".
  And private fields would not remove this round's writer gate: an owned entry is a *valid*
  `PendingEntry` (it is what `OwnedEntry::to_pending` returns), so the writers must refuse it
  either way.
- *A single `ownership: Option<Ownership>` field* (torn unrepresentable) was also considered: it
  departs from the two-field spelling `0016:442-457` and the brief names, needs a hand-written or
  `into`-based `Serialize`, and still needs the owned-shape writer gate. Not taken.
- `PendingEntryWire` stays open (no `deny_unknown_fields`): live corpus, mixed-version fleet; the
  owned shape is decoded closed by `decode_owned_entry` and ends with the canonical-bytes gate.
- GC outcome not turned into `Reconciled::Blocked` for an unreadable pending entry: that changes
  GC's certification contract, and a skipped pending chunk does not make the committed reference
  set incomplete. The brief pins the malformed-placement precedent (skip + signal, outcome
  unaffected).

## The eight isolating negations (the brief's list), plus extras

Method: a small script in `$PDCA_SCRATCH/pdca-builder-772-neg` (scratch, not shipped; the
sandbox refused my `rm -rf` of it at the end, so it is left for the harness to reclaim) applies
one exact-string edit to the production file, runs
`cargo test -q -p wyrd-core --test multipart_owned_staging` (the invocation `C4-verify` uses)
under `timeout 1200`, records the result, and writes the original bytes back. After every run the
worktree diff was compared byte-for-byte with the saved fixed diff (`cmp` → identical). Run on
the final code: the full set once, then the four `write.rs` rows (S5, S4c, S5-silent,
S5-display) again after the last one-string rewording of the sweep's message — same results.

| # | Negation | Result | The one failing test and its message |
|---|---|---|---|
| S1 | `checked_staged_scheme` drops the `erasure::supported` check (`multipart.rs:3451`) | 13 passed; 1 failed | `s1_staged_geometry_the_erasure_coder_refuses_is_a_typed_error` — `rs(0,1) is geometry the coder refuses, never a value` — `left: Ok(OwnedEntry { … scheme: ReedSolomon { k: 0, m: 1 } … })`, `right: Err(StagedSchemeUnsupported { k: 0, m: 1 })` |
| S2 | `decode_owned_entry` drops `owner != key_owner` (`multipart.rs:3620`) | 13 passed; 1 failed | `s2_an_owner_other_than_the_keys_upload_id_is_refused` — `left: Ok(OwnedEntry { owner: UploadId("a1a1…") … })`, `right: Err(OwnedEntryOwnerMismatch { key_owner: UploadId("b2b2…"), entry_owner: UploadId("a1a1…") })` |
| S3 | `checked_ownership_pairing` forced to `Ok` (`multipart.rs:3378`) | 13 passed; 1 failed | `s3_a_torn_value_is_refused_under_both_namespaces` — `the pending: reading` — `left: Err(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })`, `right: Err(TornOwnedEntry { present: "owner", absent: "staged" })` |
| S4 | `renew_pending` falls back to `let existing: PendingEntry = decode(&current)?` (`metadata.rs:2141`) | 13 passed; 1 failed | `s4_renew_pending_refuses_a_misfiled_owned_entry` — `a misfiled owned entry was renewed as an ordinary lease: Ok(Committed)` |
| S5 | `sweep_expired_leases` `?`-aborts on the first unreadable value (`write.rs:656`) | 13 passed; 1 failed | `s5_the_lease_sweep_skips_what_it_cannot_read_and_completes_for_the_rest` — `one unreadable record must not stall the reclaim of the expired lease beside it: Err(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })` — `left: Some(<the expired lease's bytes>)`, `right: None` |
| S6 (reversed) | `StagedPlacement::new` rejects a placement whose length ≠ the scheme's fragment count | 13 passed; 1 failed | `s6_a_length_mismatched_staged_placement_decodes` — `a length-mismatched placement is liberal on read (ADR-0045 :45-49): StagedSchemeUnsupported { k: 0, m: 0 }` |
| S7 | `skip_serializing_if` removed from `PendingEntry.owner` only | 13 passed; 1 failed | `s7_an_ordinary_pending_entry_reencodes_byte_identically` — `decode->encode is not byte-identical for PendingEntry { lease_expiry_millis: 1500, owner: None, staged: None }` — `left: "{\"lease_expiry_millis\":1500,\"owner\":null}"`, `right: "{\"lease_expiry_millis\":1500}"` |
| S8 | `decode_owned_entry` drops its canonical-bytes gate (`multipart.rs:3627`) — **re-targeted, see below** | 13 passed; 1 failed | `s8_an_owned_entry_reencodes_byte_identically_across_a_renewal` — `a foreign spelling was accepted: {"owner":"a1a1…","lease_expiry_millis":1500,"staged":{…}}` — `left: Ok(OwnedEntry { … })`, `right: Err(NoncanonicalRecordValue { namespace: "sidx:" })` |

**S8: the brief's wording cannot bind, so the negation is re-targeted.** The brief lists S8's
negation as "same [remove one `skip_serializing_if`], on the owned witness". I ran exactly that
(S8-as-briefed: remove it from `staged`): **only S7 fails**
(`left: "{\"lease_expiry_millis\":1500,\"staged\":null}"`), every owned-value test stays green. It
has to: serde consults `skip_serializing_if` only for a `None`, and an owned value has both fields
`Some`. What actually holds an owned value's identity across a renewal is the `sidx:` decode's
canonical-bytes gate: without it, a foreign spelling (fields reordered, whitespace inserted) is
accepted, and a renewal — which preconditions on the raw bytes and puts a freshly encoded entry —
would silently re-spell it along with the lease. S8's test asserts both halves (renewal changes
only the lease digits; the foreign spellings are refused), and the re-targeted negation fails it
alone. The round-1 adversary checked this and asked that these notes record it.

**S9, negated by construction.** S3's test builds the two torn literals an outside caller could
assemble (`PendingEntry { owner: Some(..), staged: None, .. }` and the reverse — they compile, the
fields are public) and shows the public validator, both decoders, and now both `pending:` writers
refuse them. The S9 test shows the checked path produces exactly the hand-authored owned bytes and
that both decoders agree on them.

Extras — the readers the brief's S4 negation did not pick, this round's new rules, and the GC half
(`cargo test -q -p wyrd-custodian --test gc` for the GC rows):

| # | Negation | Result |
|---|---|---|
| S4b | `live_lease_guards` falls back to the generic decode (`metadata.rs:2177`) | 1 failed: `s4_a_leased_commit_refuses_over_a_misfiled_owned_entry` — `a commit was guarded by a misfiled owned entry read as a live lease: Ok(Committed)` |
| S4c | `sweep_expired_leases` falls back to the generic decode | 1 failed: `s5_…` — the misfiled owned entry was reclaimed (`left: None`, `right: Some(<owned bytes>)`) |
| W-gate | both writers drop `checked_ordinary_lease` entirely | **2 failed**: `s4_the_pending_writers_refuse_to_store_an_owned_entry` (`an owned entry was stored under pending: Ok(Committed)`) and `s3_…` (`a torn entry was stored: Ok(Committed)`). Expected: the gate enforces two rules, and the tests are organised by rule. Each half alone isolates: |
| W-namespace | writers apply only the pairing rule | 1 failed: `s4_the_pending_writers_refuse_to_store_an_owned_entry` |
| W-pairing | writers apply only `owner.is_some()` | 1 failed: `s3_a_torn_value_is_refused_under_both_namespaces` (the staged-only literal was stored) |
| S5-silent | the sweep returns `Ok(reclaimed)` over skipped entries | 1 failed: `s5_…` — `a sweep that skipped entries must not report success: [11]` |
| S5-display | `UnreadablePendingEntries` renders as an empty message | 1 failed: `s5_…` — `the report must name a skipped entry: ` |
| GC-abort | `expired_pending_chunks` emits, then `?`-aborts (`gc.rs:505`) | 1 failed (gc binary, 10 passed): `expired_lease_input_skips_…` — `one unreadable pending entry must not fail the pass: Store(PendingEntryNamespaceMismatch { namespace: "pending:", shape: "owned" })` |
| GC-generic | `expired_pending_chunks` falls back to the generic decode | 1 failed: `expired_lease_input_skips_…` — `GC reclaimed bytes on a lease it could not read as an ordinary one` |
| GC-silent | `emit_unreadable_pending` emits nothing (the adversary's round-1 probe) | 1 failed: `expired_lease_input_skips_…` — `one audit line per skipped entry. got: {…gc_fragments_reclaimed…}{…"action":"reclaim"…}` — `left: 0`, `right: 1` |

The brief's S3 negation fails exactly one test even with the new writer assertions, because the
owned-entry writer test only uses the owned shape (refused by `owner.is_some()` whether or not
the pairing rule runs) and the torn-at-writer assertions live in S3's own test.

## Mutation run (C5's command, run locally)

`cargo mutants --in-diff <this patch> --no-shuffle` in the worktree (C5 runs exactly this):
**38 mutants tested in 62s: 9 caught, 29 unviable, 0 missed.** The unviable ones are body
replacements that do not compile under `warnings = "deny"` (a parameter goes unused, or the type
has no `Default`). For the ones in new code, a manual negation above stands in:
`checked_ownership_pairing → Ok(())` = S3; `checked_staged_scheme → Ok(())` = S1;
`emit_unreadable_pending → ()` = GC-silent; `WriteError::fmt → Ok` = S5-display;
`sweep_expired_leases → Ok(vec![])` fails S5 and the existing sweep tests;
`expired_pending_chunks → Ok(HashSet::new())` fails the GC leg's reclaim assertion.

## Refuting my own test

**(a) Genuine red? Yes.**
1. *Whole fix reverted:* the named test cannot compile on base (it calls `decode_owned_entry`,
   `OwnedEntry`, `StagedPlacement`, `decode_pending_entry`, `WriteError::UnreadablePendingEntries`).
   `C4-verify` will report UNVERIFIABLE (exit 77), as the brief pre-declares for §6.
2. *Behavioural red on the base production code:* a throwaway probe
   (`crates/core/tests/zz_probe_772.rs`, never shipped; moved out of the worktree to scratch
   after the run) makes the S4/S5 reader assertions with only the API the base has (raw JSON,
   `renew_pending`, `create_leased`, `sweep_expired_leases`). Run with every tracked file restored
   to `HEAD` (`git checkout HEAD -- crates docs`, the named test moved aside), then with the fix
   re-applied (`git apply` of the saved diff; afterwards `cmp` against the saved diff → identical):
   - **base: `0 passed; 4 failed`.**
     `renewed a misfiled owned entry: Ok(Committed); stored now Some("{\"lease_expiry_millis\":4500}")`
     — the owned entry's `owner`/`staged` erased by the renewal, the hazard the brief names at
     `renew_pending`. `committed over a misfiled owned entry: Ok(Committed)`.
     `sweep result: Ok([11, 13]); misfiled entry now None` — the misfiled owned entry reclaimed as
     an ordinary lease. `sweep result: Err(Error("expected ident", line: 1, column: 2)); expired
     lease now Some(…)` — one garbage value aborts the whole sweep and strands the expired lease.
   - **fixed: `4 passed; 0 failed`.**
3. *Per rule:* the negation table — each of the brief's eight fails exactly one test.
   The writer gate has no base red (base `PendingEntry` has no ownership fields, so no owned entry
   can be handed to a writer there); its red is the W-gate / W-namespace / W-pairing rows.

**(b) Production path? Yes.** Every assertion calls production code: `decode_owned_entry`,
`decode_pending_entry`, `StagedPlacement::new`, `OwnedEntry::{new, to_pending, from_pending}`,
`metadata::{encode, decode, put_pending, renew_pending, create_leased}` (→ `live_lease_guards`),
`write::sweep_expired_leases` over a real `RedbMetadataStore::in_memory()`, and GC through the real
`reconcile_step` → `gc::reconcile` over the file's existing `MemMeta`/`MemDServer` trait doubles
(the ones every GC leg there uses) with the real `tracing` JSON subscriber capturing the real
`emit_unreadable_pending`. Nothing is re-implemented in a test.

**(c) Fixture includes the fault? Yes.** The fault is a misfiled, torn or garbage value under a
real `pending:` key in a real store, next to healthy entries, each carrying a lease the reader
would act on if it misread it: the S5 core fixture holds an expired ordinary lease, a live one, a
misfiled owned entry, a torn value and a garbage value; the GC fixture an expired ordinary lease
and an expired misfiled owned entry, each with a real fragment on the D server. For the writers,
the fault is the owned or torn entry itself, handed to the real writer over a store holding a live
lease. Every reader and writer test also has a positive control on the healthy entry.

## Gates run locally

- `cargo test -p wyrd-core --test multipart_owned_staging`: **14 passed**.
- `cargo test -p wyrd-custodian --test gc`: **11 passed** (10 existing + the S5 leg).
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server
  -p wyrd-metadata-redb --all-targets`: clean (workspace lints: `warnings = deny`, `clippy::all =
  deny`).
- `cargo mutants --in-diff`: 0 missed (above). It ran before the last two edits — the sweep
  message's wording (a string literal inside the same `fmt` arm) and the custodian test's
  module-doc note (a comment) — neither of which adds a mutation site.
- Full gate `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`), run twice — once
  before and once after a one-string rewording of the sweep's error message:
  **`xtask ci: all checks passed`, exit 0, both times.** Steps in the log: `typos`,
  `lint_docs.py` (OK), `render_site.py --check` (99 pages, link audit OK), gitlink guard, unsafe
  guard, `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo build`,
  `cargo test --workspace` (179 `test result: ok` lines, none failed — `multipart_owned_staging`
  and custodian `gc` among them), `cargo-machete`, `cargo deny check` (three rows), statics gate,
  deploy guard, then `cargo clippy` and `cargo test -p wyrd-dst` under `--cfg madsim`, which
  compiles and runs the `crates/dst/tests/custodian.rs` ripple site.
- After the second gate run, one comment-only change: the three-line module-doc note in
  `crates/custodian/tests/gc.rs:23-26`. Re-checked after it: `cargo fmt --all -- --check`,
  `typos` on the file, `cargo clippy -p wyrd-custodian --all-targets` (clean), and
  `cargo test -p wyrd-custodian --test gc` (11 passed).
- The target repo has no commit hooks configured (no `.pre-commit-config.yaml`, no
  `core.hooksPath`, nothing installed under `.git/hooks`), so `cargo xtask ci` is the whole bar a
  publish commit must clear.
- `patch.diff` (13 files) applies cleanly to `HEAD` `a1d48ed` (`git apply --cached --check`
  against a scratch index read from `HEAD`) and reverse-applies to the worktree
  (`git apply -R --check`), so it is exactly the final tree.
- Size: +909 added semantic lines (non-blank, non-comment) against the brief's 1,250 budget;
  +1,512 raw. Per file (semantic): `multipart.rs` 171, `metadata.rs` 57, `write.rs` 37, `gc.rs`
  18, new test 492, `custodian/tests/gc.rs` 113, the other six ripple files 2–6 each, docs 1.
  `custodian/tests/gc.rs` is over the brief's ≈35 estimate for that leg: about 45 of its lines are
  the log-capture harness and audit assertions the round-1 adversary asked for (the in-tree
  `Capture` pattern; a shared helper would need a fourteenth file).

## Expected at Check

- `C4-verify`: GREEN leg passes; RED leg cannot compile → **UNVERIFIABLE (exit 77)**, pre-declared
  by the brief. The evidence standing in for it is the negation table and the base probe above.
- The custodian S5 leg is outside `C4-verify`'s discriminator set; its evidence is the GC rows of
  the negation table plus `C4-ci`.
- `C5-mutants`: expected 0 missed (local run above).

## If T4 re-raises the GC finding (for the human)

`gc.rs`'s skip was flagged by one of three passes in round 1 ("logging and then skipping … does
not surface an error or enqueue a durable repair obligation"). Behaviour is unchanged because the
brief mandates it; if a pass raises it again, the reason to record in `review-rejected.md` is:
ADR-0045 decision 3 (`0045:55-59`) prescribes exactly classify, skip and emit NEEDS-HUMAN for GC
sweeps, and fail-safe rather than reclaim on doubt; the skip is not silent — it emits a named
audit event and a counter tick on every pass that reads the input, now pinned by a test
(`crates/custodian/tests/gc.rs:926`, GC-silent negation); an error would abort the whole pass and
every other reclaim in it; the unreadable entry stays in place as the durable record; and the
brief's S5 pins this module to the `malformed-placement` / `emit_malformed` / `emit_unresolvable`
precedent, which has the same shape. I did not write `review-rejected.md` myself: the review
script describes it as the human's decisions file, and its match needs the exact `file:line` a
future pass reports.

A new risk this round introduces for T4: `sweep_expired_leases` now returns `Err` after committing
the readable reclaims. That is deliberate and documented at `write.rs:636-644` (the error carries
`reclaimed`, so a caller loses nothing), but a reviewer could read it as "partial success reported
as failure". The alternatives are costed in section 2.

## External dependencies

None missing. The build used only the base Rust toolchain plus the registered tools
(`cargo-mutants` 27.1.0 present; `typos`, `cargo-deny`, `cargo-machete` and the docs renderer are
exercised inside `cargo xtask ci`). No `Cargo.toml` / `Cargo.lock` change; `tracing-subscriber`
with its JSON formatter was already a `wyrd-custodian` dev-dependency
(`crates/custodian/Cargo.toml:30`, used the same way by `segmented_map_consumers.rs`).

## Salvage

Round 1's patch is the base of this round (itself built from `results/issue_717/iteration-v3/`'s
`StagedPlacement` / `OwnedEntry` / `decode_owned_entry` / `PendingEntryWire`). Not re-shipped
unchanged: round 1's silent `write.rs` skip, its reader-only `pending:` rule (writers now gated),
its untested GC signal, and its `||` check.
