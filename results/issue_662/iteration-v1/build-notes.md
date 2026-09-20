# Build notes — issue 662: staged reference set and reclaim intent

Base: the Do worktree at `78f9859` (origin/main with #661 merged as PR #802). The brief's
`path:line` citations are on `3969a3a`; `gc.rs` and `restore.rs` moved with #661, so every
location below was re-found by symbol. "base :N" means `78f9859`; "patched :N" means the tree
`patch.diff` produces. `0016` citations are unchanged (the proposal did not move).

Artifacts: `patch.diff` (8 files, applies cleanly to `78f9859` — checked with
`git apply --check --cached`), the test at `crates/custodian/tests/staged_protection.rs` (also
copied into this bundle at that path), and these notes.

## What changed, by the brief's four defect items

### 1. Staged bytes are now a protection class in the shared reference set

- `ReferenceSet` gains a disjoint member `staged: StagedSet` (patched `crates/custodian/src/gc.rs:434`,
  struct at `:646`), never merged into `placed` (`0016:780-782`, `:881`). `StagedSet` holds
  `placed` (dserver, fragment) pairs, `malformed` (chunk ids held whole) and `unresolvable`
  (records whose chunks cannot be known).
- `protection()` (base `gc.rs:424`, patched `:466`) returns `"staged"` for a staged fragment, after
  the committed rules and before the incomplete-set rule. `protects()` is the predicate both GC
  (patched `gc.rs:303-306`) and restore's mark gate (base `restore.rs:385`, unchanged line) use, so
  both honour the class. A new `is_incomplete()` (patched `gc.rs:507`) covers both holes.
- The build, `staged_fragments` (patched `gc.rs:712`), reads one `scan(MPU_PREFIX)`, then per
  session `scan(sidx_range(id))` **then** `scan(part_range(id))`, and `referenced_fragments` calls it
  **before** its `inode:` scan (patched `gc.rs:550`; base scan at `gc.rs:483`) — source before
  destination at both handoffs (`0016:782-800`). No scan of `part:` or `sidx:` as a whole.
- It covers every session under `mpu:` whatever its state, without decoding the session value
  (the brief's scope allows "every session's records whatever its state").
- Owned entries go through `decode_owned_entry`; their planned placement is expanded by building a
  `ChunkRef` from `(scheme, placement)` so the one expansion rule (`ChunkRef::fragments`) applies.
  Part records go through `decode_part_record`.
- Containment (ADR-0045 decision 3), in `stage_chunk` (patched `gc.rs:789`) and the builder:
  a staged placement whose length is not exactly the fragment count (empty included — staged
  records have no pre-M3 corpus, `0016:828`) holds the whole chunk; an owned entry whose value
  will not decode but whose key names the chunk holds the whole chunk; a part value that will not
  decode, an owned-entry key that names no chunk, or an `mpu:` key that names no upload goes into
  `StagedSet::unresolvable`, which makes the set incomplete for GC and restore only.
- Restore: the gate is unchanged code, so it honours `staged` through `protects()`. Staged
  `unresolvable` records are named in `RestoreReport::unresolvable` (existing field; doc updated at
  patched `restore.rs:160-167`) with their own audit line (patched `restore.rs:320-326`, emitter
  `:842`), so `needs_human()` holds and nothing is marked.

### 2. A pending byte retirement protects its fragments, by keyed lookup

`retirement_draining` (patched `gc.rs:1422`) does one `get` of `retire:bytes:<event>` for a
candidate whose mark names an event, and only once the fragment is about to be acted on (past
grace, or already `reclaiming`). The key is only formed when `event` parses as a canonical
retirement token through the shared `parse_retire_key` (`retirement_key`, patched `gc.rs:1434`), so a
key built from stored bytes is always bounded and in the canonical spelling installers use. The
`retire:` namespace is never read as a range (`0016:1226-1247`).

### 3. Reclamation is recorded before destruction

The fleet loop no longer calls `delete_fragment` for a marked fragment (base `gc.rs:314`, cleanup
at `:346`). A new `Reclaim` struct (patched `gc.rs:1213`) owns the pass's destructive half:

- `intend` queues a fragment; `commit_intents` (patched `gc.rs:1343`) commits up to
  `CLEANUP_BATCH` intents as one batch, each `require(orphan_key, mark.encode())` +
  `put(orphan_key, mark.reclaiming().encode())`, and only then deletes the fragments and queues
  their key deletes in `Cleanup` (unchanged) — `0016:1312-1320`.
- The precondition is `mark.encode()` because the codec decodes a value only when it is the
  encoder's own spelling, so re-encoding reproduces the bytes read; the window does not need to
  keep raw copies.
- On a batch `Conflict` each intent is retried alone; a lost one is skipped (`"mark-changed"`), its
  fragment kept. `Err` (including `CommitUnknownResult`) propagates with nothing deleted for
  unlanded intents; a landed-but-unacknowledged intent is finished by the next pass.
- A `reclaiming` mark is finished by `resume` (patched `gc.rs:1310`): delete fragment, then queue the
  key delete, with no grace test (`0016:1329-1333`).
- The expired-lease `pending:` bookkeeping from #661 moved into `Reclaim` unchanged
  (`keep`/`reclaimed`/`finish`, patched `gc.rs:1393`).
- A pass that lost an intent and reclaimed nothing answers `Partial`, not `Satisfied` (patched
  `gc.rs:381`; `Reconciled::Partial` doc, patched `reconciliation.rs:45-49`). Reason: a
  `Satisfied` there would certify a ledger the pass did not finish judging.

### 4. Three `orphan:` value shapes, dual-format, fail-closed on none

The codec lives in `crates/core/src/metadata.rs` beside `orphan_key`/`parse_orphan_key` (base
`metadata.rs:62-85`), where the delete path, GC, #659's drain and child-3's re-place can all reach
it: `MAX_ORPHAN_MARK_LEN` (patched `:105`), `OrphanMark` (`:134`), wire struct (`:227`),
`OrphanMarkError` (`:241`), `decode_orphan_mark` (`:301`), unit tests (`:3129`, `:3183`, `:3260`).

- Legacy: a canonical decimal, parsed by the module's existing `parse_canonical_u64` (patched
  `:1583`). Structured: `{"orphaned_at_millis":N,"event":"E"}`. Reclaiming:
  `{"orphaned_at_millis":N[,"event":"E"],"reclaiming":true}`.
- Decode accepts only the encoder's exact image (re-encode and compare), so a legacy value
  round-trips byte-for-byte and no reader rewrites a mark (`0016:1208-1211`).
- Length bound 512 bytes, measured on the reclaiming form, so GC can always write and read back
  the reclaiming form of any mark it could read, and an intent batch stays under ~1.2 MB.
- `mark_orphaned` is untouched (its output is the legacy shape; the doc now says so).
- GC: `ReadMark` is now `Readable(OrphanMark) | Unreadable` (base `gc.rs:640`, patched `:878`);
  `classify_ledger_entry` decodes through the codec (base `:800`, patched `:1042`) and names a
  none-of-three value with its fault (`emit_unreadable_mark`, patched `:1517`, same action/field
  the #661 test asserts).
- Restore: `marked_among` (base `gc.rs:872`, patched `:1117`) now returns each found mark's decode
  result; existence is still the whole judgement. Restore names a none-of-three value on its own
  seam (patched `restore.rs:447-455`, emitter `:859`) and leaves it byte-identical.

### Docs currency

`docs/design/architecture/08-crosscutting-concepts.md` §8.7: three paragraphs after the 0016
decision 7(a) paragraph (patched `:87`, `:89`, `:91`) — the value shapes, the staged class, and
reclaim-before-destroy. `06-runtime-view.md` §6.7 GC step: one paragraph (patched `:78`).

## Design choices and rejected alternatives

1. **Unreadable staged records: separate member, not folded into the committed `unresolvable`.**
   My first build folded them into `ReferenceSet::unresolvable`. That also made scrub answer
   `Blocked` and name a `part:` key as "unscrubbable", and the drain-status surface answer
   `PendingUnresolvable` — for consumers that do not read staged bytes at all yet (child-3/4).
   The separate member keeps the fail-closed effect on exactly the two passes that honour the
   class. Cost of the change: ~40 lines (a map, `is_incomplete`, one emitter each in GC and restore).
2. **Per-session `scan`, not per-session `scan_page`.** 0016 prescribes the bounded per-session
   ranges (`0016:802-805`): `part:<id>:` ≤ `MAX_PARTS_PER_SESSION` = 10,000 records, `sidx:<id>:`
   ≤ `SCAN_CAP/2` by the G5 clamp, `mpu:` bounded by the `MAX_SESSIONS` clamp (`0016:1470`). Paging
   would bound the raw values held per read further, at the cost of a generalised checked page
   walker (~40 lines, splitting #661's `ledger_page`, whose error texts #661's guard test pins).
   Not required by any leg; left as is. Memory note for sign-off: the staged set stores
   (dserver, fragment) pairs like `placed` does (0016:827 wants pairs for the drain union), so its
   size is ≈ fragments, i.e. up to 9× the chunk-ref count 0016's `W_ref` budget counts.
3. **Batched intents with per-intent fallback, not one commit per fragment.** A full window of
   65,536 reclaimable marks would be 65,536 intent commits; batched it is ⌈65,536 / 1,000⌉ = 66,
   plus singles only for a batch that conflicts. Giving up a whole conflicted batch instead was
   rejected: one re-stamped mark would stall up to 999 other reclaims every pass (leg F(ii) pins
   this; mutation 3 below).
4. **`event` is an opaque string**; only a canonical retirement token triggers the lookup. 0016
   also lists per-move nonces and other identities (`0016:1174-1187`) whose spelling is child-3's
   and #659's to choose, so the codec does not constrain it. Consequence: an event that is a
   non-canonical spelling of a token (e.g. `s:<id>:007`) triggers no lookup — no installer can
   write that key, so no obligation can sit under it.
5. **The retirement lookup also runs for `reclaiming` marks.** Conservative: 0016 only exempts them
   from the grace test. A retirement with the same token cannot be re-installed, so this can only
   matter after corruption or a restore, where skipping is the safe side.
6. **Legacy decoding is now strict.** The base parsed any `u64::from_str` spelling (`+5`, `05`); the
   codec accepts only canonical decimals, per the rubric's grammar-strictness rule. Every in-tree
   writer uses `u64::to_string()` (`gc.rs` `mark_orphaned`, `restore.rs`, `rebalance.rs:544`,
   `reconstruction.rs:944`, `metadata.rs` unlink/overwrite paths), so no written mark changes
   meaning; a hand-written non-canonical value now fails closed (kept + named) instead of being
   honoured.
7. **Not changed, out of scope — recorded so sign-off can weigh it:**
   - The expired-lease arm (`ExpiredPendingPolicy::Reclaim`, off in deployment) still deletes
     bytes before retiring the `pending:` entry. The invariant says "no pass destroys a byte before
     that destruction is durable in metadata"; the brief's defect item 3 and `0016:1312-1336` are
     about `orphan:` marks and adoption CASes, and the lease arm has its own policy (#557/#490).
   - Restore's displaced-copy check (`canonical`, base `restore.rs:365-368`) still uses committed
     placements only. A staged fragment whose bytes moved would be marked; child-4's session fence
     (D-B) aborts every restored session anyway.
   - `ReconciliationStatus::PendingUnresolvable`'s doc in `desired_state.rs` (out of scope) is
     unaffected by design choice 1.
   - Fragment-less `reclaiming` marks (a pass that deleted the fragment and died before the key
     cleanup) are never revisited — the same leftover the base has for legacy marks; #800's sweep.

## Test evidence

### The brief's test: `crates/custodian/tests/staged_protection.rs` (13 tests)

Legs A, B, C(i), C(ii), D, E, F(i)–F(iv) (as four tests), G, plus two containment tests for the
fail-closed paths (unreadable staged record; untrusted staged placement).

- **Green with the fix:** 13/13 (`cargo test -p wyrd-custodian --test staged_protection`, and inside
  `cargo xtask ci`).
- **Red on the base (production reverted, test kept):** **13 of 13 failed, all by assertion** — the
  file compiled on the base (it names only base-visible symbols), so no compile failure, no
  UNVERIFIABLE. Failing assertions on the base:
  A "staged fragment … was reclaimed on a mark past grace"; B "the post-restore pass marked staged
  fragment … stranded (stranded_marked: 7)"; C(i)/C(ii) "…landed between the builder's two reads
  and the chunk was reclaimed"; D "staged fragment … was reclaimed" (D's no-global-scan half is
  green on the base, as the brief expects; its survival half makes it red); E the second half
  "…the fragment survived — a structured mark must decode"; F(i) "the fragment was destroyed
  although its reclamation never became durable"; F(ii) "the fragment was reclaimed on a mark that
  changed after GC read it"; F(iii) the adoption CAS got `Committed`; F(iv) "a `reclaiming` mark …
  is finished"; G "the structured, past grace mark … did not license its reclaim"; both
  containment tests "…was reclaimed".
- **Mutation checks (one rule broken at a time in `gc.rs`, restored byte-for-byte after each):**
  1. drop "superseded ⇒ Partial" → F(ii) red; 2. ignore the retirement lookup → E red;
  3. no per-intent fallback on a batch conflict → F(ii) red; 4. read `part:` before `sidx:` →
  C(i) red; 5. build the staged half after the `inode:` scan → C(ii) red; 6. grace-test
  `reclaiming` marks → F(iv) and G red.

Symbols the test names beyond the brief's list, all present on the base (the red run proves
it): `wyrd_core::metadata::{EcScheme, InodeRecord, PendingEntry, decode, encode, inode_key,
ORPHAN_PREFIX}` (`StagedPlacement::new` needs `EcScheme`; `InodeRecord`/`decode` validate the
committed-inode fixture), `wyrd_core::multipart::{part_range, sidx_range}`, and the `wyrd_traits`
page helpers. Nothing the slice adds is named; structured marks are raw JSON.

### Seeded Tier-0 DST (rubric: a new destructive/concurrent path lands with DST coverage)

`crates/dst/tests/custodian.rs` property 13 (patched `:2562`): two seeded properties over the
simulated-TiKV store with a genuinely concurrent task at a seed-chosen half-millisecond — a part
commit then a publication during GC's staged build (`:2813`), and a mover's adoption CAS racing
GC's reclaim through a delete that spans a hop (`:2968`) — plus two coverage properties proving
the interesting landings are reached (`:2931`, `:3076`), and both seeded properties added to the
committed regression-seed replay. Green over 50 seeds in `cargo xtask dst`. **On the base the four
new tests and the regression replay fail** (5 failures), the adoption one with exactly the data
loss the change prevents: "the adoption published a placement naming a fragment GC deleted —
outcome (c)". The extra `mpu:` scan in every reference build did not disturb the existing
coverage properties (#661's and #651's).

### Refute-your-own-test

- **(a) Genuine red? Yes.** Reverted `crates/core/src` and `crates/custodian/src` to the base and
  re-ran: 13/13 red by assertion (above), and the DST properties red too. Six single-rule mutations
  each turn the matching leg red.
- **(b) Production path? Yes.** Every leg calls the production `reconcile_step` (GC loop) or
  `reconcile_after_restore`; nothing in `gc.rs`/`restore.rs` is copied or mocked. The doubles are
  the store seams only (`MetadataStore`, `ChunkStore`), and fixture records are validated through
  the production decoders before they are seeded.
- **(c) Fixture includes the fault? Yes.** Every protection leg seeds an `orphan:` mark past grace
  on the protected fragment (without it GC's conservative arm would keep it and the leg would pass
  on an absence); each includes an unprotected control that is reclaimed, so a pass that reclaims
  nothing cannot pass. The handoffs (C) are actually performed between the builder's reads, the
  intent failure (F(i)) is actually injected, the mark change (F(ii)) actually lands after the
  ledger read, and the adoption CAS (F(iii)) actually runs inside `delete_fragment`.

## Gates run

- `engine/xtask.sh ci` (→ `cargo xtask ci`: typos, docs lint+render, fmt, clippy, build, test,
  machete, deny, conformance, statics, orchestrator guard, madsim DST with 50 seeds): **all checks
  passed** on the tree before the final refactor and again after it; a last run on the exact final
  tree is recorded at the end of this file.
- `cargo fmt --all -- --check`: clean. `cargo clippy --all-targets` on core and custodian: clean.
- External dependencies the brief names: `typos` 1.48.0 and the docs renderer
  (`markdown-it-py` 3.0.0, `PyYAML` 6.0.2) are both installed, so the prose gates ran for real.
  No NEEDS-HUMAN external dependency.

## For sign-off

- `patch.diff` is ~189 KB across 8 files, over the `[driver.size_signal]` 100 KB threshold, so the
  harness will likely raise its size item. What makes it large: the brief's test (13 tests, ~57 KB),
  the DST property (~25 KB), and the codec with its unit tests. The production change itself is
  `gc.rs` +673/−137, `restore.rs` +63/−10, `metadata.rs` +~280 plus tests.
- The `CLEANUP_BATCH` constant now also bounds intent commits (doc updated at patched
  `gc.rs:97-121`); intents carry up to ~1.2 MB per commit, inside the 10 MB envelope but not
  calibrated for the 5 s half (the same caveat the constant already carried).

## Scratch left behind

`/tmp/pdca-builder-662-redleg` (a few MB: gate logs, the production-diff snapshot used for the
red runs, a `gc.rs` copy used to restore after each mutation). `$PDCA_SCRATCH` and `$TMPDIR` were
unset, so the fallback chain chose `/tmp`; the sandbox refused `rm -rf` on it, so it is left for
the harness sweep. Nothing in it is needed.

## Final gate on the exact final tree

`engine/xtask.sh ci` on the tree `patch.diff` produces (re-diffed and byte-compared after the run):
`xtask ci: all checks passed`, exit 0 — `typos`, `lint_docs: OK`, `render_site: link audit OK`,
fmt, clippy, build, workspace tests (the 13 staged-protection tests among them), machete, deny,
conformance, statics, orchestrator guard, and the madsim DST tier over 50 seeds with the four new
property-13 tests green.
