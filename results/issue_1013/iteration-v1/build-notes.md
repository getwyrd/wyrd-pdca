# Build notes — #1013 (multipart session terminal delete, 636.6b)

Target: `getwyrd/wyrd` `main`, built on the stack base `b452551` (child-1 = #1061 + #1062
integrated). All `path:line` below are in the patched tree.

## What the change does

1. **Session leg** — new module `crates/custodian/src/retirement/sessions.rs`, run by
   `retirement::reconcile` (`crates/custodian/src/retirement.rs:447`) **before** the ledger walk
   (now `walk_ledger`, `retirement.rs:457`). For every session listed under `mpu:` in `Aborting`
   or `Completed`:
   - **Owned walk** (`sessions.rs:332`, `walk_step`): reads the first 16 entries of
     `sidx:<id>:`, marks every position through child-1's three-arm guard (`marks::read_arm`,
     made `pub(super)`) with event `<upload-id>:<epoch>` (`0016:672`; `crates/core/src/metadata.rs:238`
     already names this identity as "an owned-staging walk's"), stamp from `ctx.clock` read after
     the page is in hand (ADR-0009). An entry is deleted under `require(sidx == bytes read)` only
     in a later step, once every position read as this walk's own mark (child-1's "marks before
     deletes, in two commits" rule, `marks.rs` module doc). No cursor: nothing refills the range
     after `Open` (`stage.rs` intent precondition), so what is left is the progress.
   - **Terminal delete** (`sessions.rs:532`, `terminal`): only after a step observed the range
     empty **in this pass**, both session-scoped obligation ranges are empty (two one-row
     `checked_page` reads of `retire_session_range`), and for `Completed` the context's
     `Tombstone::Elapsed`. One batch: `require(mpu == bytes read)`, `require(mpuctl == bytes
     read)`, delete `mpu:`, delete surviving `slot:` records, delete `seggrp:<nonce>` iff a one-row
     read of `seg:<nonce>:` is empty, put `mpuctl` one lower (`AdmissionRecord::release_one`,
     new, `crates/core/src/multipart.rs:2091`). Slots past one page go first in their own batches
     under the same session precondition (`settle(.., None)`, `sessions.rs:627`).
   - A ledger that is absent/undecodable/at zero is a `Hold::AdmissionLedger` **only if the
     session record still holds the bytes read** (`ledger_unusable`, `sessions.rs:607`).
     Otherwise it is a lost race and the session is re-judged. This was a real bug the race test
     found on the first run: the losing drainer read a ledger the winner had just lowered to 0 and
     raised a false "needs a human" hold.
2. **Untrusted records retired by chunk id**, `Aborting` sessions only, whole-fleet passes only:
   - `ByChunk::listed` (`retirement.rs:379`): refuses (→ `Hold::PartialFleet`) unless
     `ctx.fleet.len() == ctx.operator_fleet_size` — the deployed GC gate, mirrored
     (`crates/server/src/custodian.rs:636-680` on base) — and also refuses if any server's
     `list_fragments` fails (a server not seen is a partial fleet). Returns every listed fragment
     whose chunk is wanted, keyed by position.
   - E1 (`part:` record): `marks.rs:475` `Walk::part`. `load_part` (`marks.rs:768`) now sets
     untrusted chunks apart instead of returning the hold; if the session record reads `Aborting`
     (one read charged to the step's mark-read share, so `READS_PER_MARKING_STEP` is unchanged),
     the entry's positions become its trusted chunks' placement + every listed fragment of the
     untrusted chunks, walked from position 0 every step (a listing is not a stable cursor
     space), and the step stops after it (one listing per step).
   - E2/E3 (`sidx:`): `read_entry` (`sessions.rs:255`). E2 (decodes, wrong length) → listed ∪
     every position the entry names; E3 (value will not decode, key parses to this session) →
     listed only, chunk from `parse_sidx_key` exactly as GC's hold takes it (`gc.rs:1392` on
     `db1a066`). `Completed` sessions keep the hold. One listing per walk step for all such
     entries of the page.
   - Undecodable `part:` value: unchanged `Hold::RecordUnreadable`, detail now ends "needs a human
     (operator repair)" (`marks.rs` `load_part`).
3. **`{session}` obligation and residue** (`marks.rs:233` `owned_residue`): the obligation now
   **waits** (`owned-residue-first`, `marks.rs:140`) when the session is `Aborting`/`Completed`
   (the walk drains it), and keeps `Hold::OwnedResidue` only when the record is absent,
   undecodable or not past `Open`. Before, every pass with residue was a `Blocked` hold — an
   operator signal for a normal transient state.
4. **Drain context** (`crates/custodian/src/reconstruction.rs:149,155`): two fields on
   `ReconstructionContext`, the input child-1's dispatch leg already passes (no new
   `reconcile_step` parameter, as the brief requires): `operator_fleet_size: usize` (I chose to
   carry the fleet size, not the gate's answer, so the drain applies the same comparison GC's gate
   does against the very fleet it lists) and `tombstone: Tombstone` (`retirement.rs:352`,
   re-exported `lib.rs:45`). The deployed caller passes its `operator_fleet_size` and
   `Tombstone::Retain` (`crates/server/src/custodian.rs:577,580`).
5. **Docs**: `docs/design/architecture/06-runtime-view.md:90,92` (the retirement paragraph's
   residue/hold sentence corrected; a new paragraph: terminal delete, exactly-once release,
   tombstone kept until #625, retirement by chunk id on whole-fleet passes only, the late-write
   residual, undecodable `part:` needs a human, fragment-less walk marks stay);
   `05-building-block-view.md:246` (the session leg and the only `mpuctl` decrementer).

## Cost per pass (Production reach (b))

- Session listing: every `mpu:` record, pages of 16 values (≤ 1.6 MB a page), every pass. Bounded
  by admission (`max_sessions` ≤ `SCAN_CAP/2`, 46 at the shipped profile) — the same listing GC
  already reads every pass (`gc.rs` `staged_fragments`).
- Commits: ≤ 16 per pass for the session leg (`SESSION_STEPS_PER_PASS`), separate from the ledger
  walk's 17. Per walk step: 1 page read + ≤ 233 mark reads; per terminal judgement ≤ 6 reads.
- Fleet listings: **zero** unless an untrusted record of an `Aborting` session is met; then at
  most one full-fleet `list_fragments` per step, so worst case 16 (ledger) + 16 (session leg) per
  pass. It does **not** reuse GC's listing: GC runs as its own fenced pass after reconstruction
  (`crates/server/src/custodian.rs:667-672` on base explains the fault isolation), so the listing
  GC takes does not exist yet when the drain runs. Sharing it would mean folding GC into the
  reconstruction pass (which loses that isolation) or caching a whole-fleet listing in memory.
  Not taken; untrusted records come only from damage.
- Any configured server down: the by-chunk retirements pause (held, `partial-fleet`), as GC
  pauses. Nothing is lost while paused; the trusted walk and terminal deletes carry on.

## Alternatives ruled out (with the cost)

- **A persisted session cursor** (as the ledger walk has, `LEDGER_CURSOR_KEY`): about 60 lines
  and a new store key (`custodian:session-cursor`) plus a write whenever it moves. Not needed:
  the listing is admission-bounded and GC already reads it whole every pass. Starvation is not
  possible either: a held session costs reads but no steps, and a productive one finishes and
  leaves the listing.
- **One batch per owned entry (marks + delete together)**: the `0016:672` row allows it. I kept
  child-1's two-commit rule so both drains have one convention and the recording wrapper can show
  the order across batches.
- **Holding every untrusted `sidx:` entry for an operator** instead of the late-write residual:
  the brief offers this to the human; not taken.

## Tests

- **New**: `crates/custodian/tests/multipart_session_terminal.rs` (15 tests), over
  `RedbMetadataStore::in_memory()` behind a recording wrapper and a placement-aware D-server double
  with counted listings. Everything goes through `reconcile_step` with the deployed contexts
  (reconstruction pass: live fleet, `operator_fleet_size` = configured count, `Retain`; GC its own
  pass). Sessions come from production `create_session` / `abort_session` / `complete_upload`.
  Legs (D), (D marker), (D Completed + tombstone), (D gate), (D race ×2), (E1/E2/E3 + late
  landing), (E Open control), (E fleet controls ×4), (E undecodable part).
- **Changed, because this child changes the behaviour they pinned**:
  - `multipart_retirement_marks.rs:1790` (R): child-1's placeholder asserted the `{session}`
    obligation is **held** as `owned-residue` "until #1013". Now: while the walk cannot finish
    (an unreadable mark over the residue) the obligation **waits** (`owned-residue-first`) and is
    never held; after the mark is repaired the walk empties the residue, the obligation drains,
    and the terminal delete follows.
  - `staged_protection.rs:2574`: the empty-queue guard asserted an idle reconstruction pass
    reads no `mpu:` key. The session leg now reads the session listing every pass by design, so
    `mpu:` is dropped from the faulted and checked prefixes. The guard still holds for `sidx:`,
    `part:` and `inode:` (an `Open` session's ranges are not read).
  - Every other `ReconstructionContext` literal (50 sites in 13 files, including
    `crates/dst/tests/custodian.rs`, the chunkstore-grpc tier tests and the server day-one tests)
    gains `operator_fleet_size: <that literal's own fleet>.len()` (4 or 0 for inline arrays) and
    `tombstone: Tombstone::Retain`. Mechanical; nothing about them changes.

### Demonstrated reds (each a mutation of production code, run against the new file, restored)

| Mutation | Red test(s) and the failing assertion |
|---|---|
| M1 the `sidx:`-empty gate removed (`visit`: `Walked::Stop` falls through to `terminal`) | `d_the_terminal_delete_waits_for_an_empty_owned_range` — "round 1": session deleted with residue left |
| M2 `Completed` deleted without the tombstone input (Retain check disabled) | `d_a_completed_session_keeps_its_tombstone_until_the_window_passes` — "the tombstone stays"; e1/e2/e3 also red (their Completed control session got deleted, count −2) |
| M3a decrement not under the session precondition (`require(mpu == prior)` dropped) | `d_two_drainers_racing_…` — "released once" left 0 right 1 (two decrements) |
| M3b decrement in its own batch before the delete | same test, same assertion (two decrements) |
| M4 untrusted record deleted without marking by chunk id (listed positions dropped in both legs) | e1, e2, e3 — "(0, FragmentId{..}) is unmarked"; all four fleet controls — "… survived" |
| M5 retirement by chunk id on a short fleet (gate disabled) | all four `e_*_wait_for_a_server_*` — round 0 `Changed` vs `Blocked` |
| M6 E2 marks only what the listing found | `e2_…` — "(3, FragmentId{chunk:22007,index:3}) is unmarked" |
| M6b = M6 with the test's named-position assertion skipped, to reach the late-landing check | `e2_…` — "FragmentId{chunk:22007,index:3} on 3 survived": the late fragment is kept for good |
| M0 the whole fix neutralized (session leg not dispatched, by-chunk refused, residue held as on base) | 13 of 15 red; the 2 green are the safety controls (Open kept whole, undecodable `part:` held), which base already satisfies |

On the real base the new file does not compile (no `Tombstone`, no context fields), which is why
C4-verify reports this net-new file UNVERIFIABLE (the brief's pre-declared posture). M0 is the
runtime stand-in for "base behaviour".

### Refuting my own test

- **(a) Genuine red?** Yes. With the fix neutralized (M0), 13/15 fail at runtime. Each of the six
  brief-named mutations turns its leg red (table above). The script applied each mutation to the
  production files, ran `cargo test -p wyrd-custodian --test multipart_session_terminal` and
  restored them; `git status` was checked after.
- **(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` with a
  `ReconstructionContext` shaped as `crates/server/src/custodian.rs` builds it, and GC through its
  own `reconcile_step` pass. Sessions, aborts and completes are the production `wyrd_core`
  functions. The metadata store is real redb. Only the D servers are a double (in-process maps
  with the `ChunkStore` contract), which the brief allows ("a real chunk store or placement-aware
  double").
- **(c) Fixture includes the fault?** Yes. The fleet controls run with server 5 actually absent
  from the pass's fleet (once never connected, once in `unreachable`) while it holds a fragment of
  the chunk, and the test checks that fragment is reclaimed only after a whole-fleet pass. The race
  leg runs a real second `reconcile_step` pass in the gap. The late-landing legs publish the
  fragment after the record's deletion, after a counted listing. The residue legs include a
  position whose write never landed.

## Residuals (for the human)

- **Fragment-less walk marks stay.** A walk mark over a position whose write never landed has
  event `<id>:<epoch>`. GC's sweep leaves structured non-token marks in place
  (`event-may-await-write`, `gc.rs` sweep doc), because an abandoned write may still land with no
  time bound (`stage.rs` *The write deadline*). So such a mark stays in the `orphan:` ledger for
  good. That is a mark kept too long, never a fragment left without one. It is bounded per session
  by the residue size (≤ `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` entries × n positions).
  Stated in the runtime view; no follow-up issue filed.
- **Late-write residual** (brief's Design): stated in the runtime view and the `retirement.rs`
  module doc; E3's test shows it (kept, no error).
- **Completed sessions never end in production** until #625 supplies `W_tombstone`
  (`Tombstone::Retain` in the deployed caller). They keep their `mpuctl` slot until then.

## Gates run

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test -p wyrd-custodian` (all test binaries): green after the two test updates above.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree): **exit 0, "all checks passed"**.
  The prose gates ran rather than warn-skipping: `typos`, `render_site.py --check` (101 pages,
  link audit OK). The madsim DST crate built and tested (`cargo clippy/test -p wyrd-dst
  --cfg madsim`), so the edited `crates/dst/tests/custodian.rs` literals compile there, and the
  ADR-0035 statics gate passed. A comment-only edit landed after the gate started (the
  `// deferred: #660` marker, `sessions.rs:115-117`); `typos` over the touched files, `cargo fmt
  --check` and `cargo test -p wyrd-custodian` (365 passed, 0 failed) were re-run on the final tree.
- `patch.diff`: 24 files (22 changed, 2 new), checked with `git apply -R --check` against the
  gated worktree.

No external dependency was missing: `typos` and the docs renderer are installed and ran.

## Rubric self-review (AGENTS.md *Review rubric & protocol*)

- **One clock per lifecycle**: the walk's marks are stamped from `ctx.clock`, read once the page is
  in hand, the source the deployed loop's `now_millis` is read from (`LoopClock`). The terminal
  delete and the by-chunk retirement read no clock. The tombstone window is an input, not a clock
  read (#625 owns it).
- **Narrow seams / dependency direction**: the custodian still uses only `MetadataStore` /
  `ChunkStore`. One new `pub` core method (`AdmissionRecord::release_one`).
- **Metadata validation boundaries**: everything is read through the namespace decoders
  (`decode_session_record`, `decode_owned_entry`, `decode_admission_record`). The placement-length
  judgement stays contextual, in the maintenance path (`staged_placement`).
- **No shared mutable global state**: none added (statics gate green).
- **Docs currency**: runtime view and building-block view updated in the same patch.
- **Absent or unsupported entries**: every case the leg cannot act on is a typed, audited hold
  (`partial-fleet`, `admission-ledger`, `record-unreadable`, `untrusted-placement`,
  `unreadable-mark`, `reclaiming-under-retained-record`, `undecodable`, `commit-failed`), never a
  silent skip. A `Completed` session kept under `Retain` is a deliberate retention, not a skip.
- **Transactions**: each write is one `commit`. An `Err` (including `CommitUnknownResult`) is held
  as `commit-failed` and re-read next pass, never reported as a conflict. The exact-bytes session
  precondition makes a re-run of a landed terminal delete a no-op.
- **Await discipline**: only `MetadataStore` / `ChunkStore` awaits, bounded as every custodian read
  is (the GC doc states the same). No spawned tasks.
- **Test fidelity / DST**: a new destructive and concurrent path. Seeded DST is out of scope by the
  brief (#660); settled in code with `// deferred: #660` (`sessions.rs:115`), and the race that
  matters is forced deterministically by a wrapper in the new test.
