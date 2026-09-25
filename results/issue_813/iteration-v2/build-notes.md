# Build notes — issue 813 (staged-scrub-and-keep), iteration 2

Iteration 2 is **v1 plus four targeted fixes**, not a rebuild. The human overrode the size
backstop and chose iterate-do precisely because the four review findings were targeted fixes
(`brief.md:150-157`), so I started from `iteration-v1/patch.diff` (it applies cleanly to the
worktree base `4ab2b28`) and changed only what the carry-forward names, plus the docs those
changes make false.

Base for every `path:line` below: the worktree at `4ab2b28` (`origin/main`; `97fc2f9` is an
ancestor, and none of the files this patch touches moved between them).

## The four carry-forward findings, and what each fix is

### 1. `scrub.rs` read order — staged parts BEFORE committed inodes

**Fix:** `crates/custodian/src/scrub.rs:102-123` now holds the `staged_committed_parts` read
(and its two attribution loops), and `:152-198` the `referenced_fragments` read. v1 had them
the other way round and its comment claimed the order "costs nothing here" — which is wrong
for exactly the reason GC's own order exists (`0016:782-800`): a publication writes the
committed inode and deletes the `part:` record it replaced **in a later batch**, so a pass
that scans `inode:` first and reads `part:` after the retirement drain sees the chunk in
**neither** class, fetches none of its fragments, and answers `Satisfied` over a lost one.
The comment is rewritten to say the order *is* the protection, and the module doc gains the
same paragraph (`scrub.rs:37-43`).

**Regression test** (the carry-forward asked for one): `staged_scrub.rs`'s leg **(C')** —
`a_publication_flipped_and_drained_between_scrubs_reads_leaves_the_chunk_checked` and its
`…_and_drained_after_…` companion. It mirrors GC's own (C)(ii) fixture
(`staged_protection.rs:1271-1392`): the flip lands right after the **first** of the pass's
two reads of `part_range` / `inode:` — whichever order the pass makes them in, so the hook is
order-agnostic and the *pass* is what is under test — and the drain after read 1 or read 2.
The moving chunk's fragment is missing, so scrub must enqueue it; a `WITNESS` committed object
whose own fragment is also missing is the positive control that the pass walked and enqueued
at all. `assert_published_in_two_batches` pins that the publication really landed as two
batches, not one collapsed batch that would make the window unreachable.

Red proof for this specific finding, not just for the slice: I swapped the two blocks back to
v1's order (production otherwise unchanged) and ran the file —
`a_publication_flipped_and_drained_between_scrubs_reads_leaves_the_chunk_checked` **FAILED**,
the other 10 passed. Restored, 11/11 green. The drain-after-read-2 companion is green under
both orders by construction (the part record is still present when the source is read either
way); it is in the file so the discriminating schedule is explicit rather than implied.

### 2. `gc.rs` `checked_fragments()` — an empty staged placement is damage, not a legacy spelling

**Fix:** one shared rule, `staged_placement` (`crates/custodian/src/gc.rs:1441-1472`), that
both staged readers resolve a staged placement through: `StagedSet::place` (`:1408-1429`,
GC/restore/drain-status) and `read_staged_part` (`:1618-1667`, scrub). It requires the
placement to be **exactly** one D server per fragment — the empty vector included. That is
deliberately stricter than the committed rule
(`ChunkRef::checked_fragments`, `crates/core/src/metadata.rs:441-452`), which admits an empty
vector as the pre-M3 identity fallback: that exemption exists for committed records written
before placements were, and every staged record is born with a full one (`0016:828`).
v1's `read_staged_part` called `checked_fragments()`, so a part record with an empty placement
was identity-filled: scrub fetched fragment `i` from D server `i` — positions no record ever
named — and enqueued a **phantom** repair when (of course) nothing was there.

`StagedSet::place`'s behaviour is byte-for-byte unchanged (same length test, same fault
string, inlined where it was) so `staged_protection.rs` legs A–E stay green unedited; I
checked (32/32 green, including `a_part_with_an_empty_placement_holds_its_chunk`,
`:1922`).

**Test:** `staged_scrub.rs` leg **(C'')**
`an_empty_committed_part_placement_is_malformed_and_queues_no_phantom_repair`: nothing queued,
`Satisfied`, and the `malformed-placement` audit event naming the chunk. Red proof: I reverted
`read_staged_part` to `chunk.checked_fragments()` (one line) — that leg **FAILED** on
"scrub enqueued a repair for a chunk whose committed part placement names no D server at all",
the other 10 passed. Restored, 11/11 green.

### 3. `reconstruction.rs` — staged attribution emitted before the next fallible read

**Fix:** `crates/custodian/src/reconstruction.rs:207-224`. The `staged.unresolvable` emit loop
moved **inside** the `else` branch, between the staged read and
`read_committed(...).await?`. v1 had it after the whole `if/else`, so an `inode:`-store fault
one statement later ended the pass with `Err` and took the names of the unreadable staged
records with it. Same discipline `read_committed` itself already follows (it names each
unreadable object where it is met, `:568-583`, "a store fault a `?` later ends the pass with
an `Err`, and a name this pass already held must not go down with it").

**Test:** `staged_protection.rs`
`an_unreadable_staged_record_is_named_even_when_the_committed_read_then_faults` — an
undecodable `part:` record plus a fault armed on `inode:`; the pass returns
`ReconcileError::Store`, the obligation is still queued, and the record is still named on the
reconstruction audit seam. Red proof: I moved the emit back to v1's position — that leg
**FAILED** ("was found and then lost"), 31 others passed. Restored, 32/32.

### 4. Seeded Tier-0 DST coverage for the reconstruction/publication race

**Fix:** `crates/dst/tests/custodian.rs` property 13 now runs over **both** loops. The one
fixture is parameterised by a `Driver` enum (`:2604-2613`); `staged_handoffs_under(driver,
gaps)` (`:2841-3055`) seeds what that loop stands to lose and asserts it after **every** pass:

- `Driver::Gc` — unchanged from the base: the fragment on disk under an `orphan:` mark past
  grace, the stray beside it, "never reclaimed" + "the stray was".
- `Driver::Reconstruction` — the moving chunk's fragment is **gone** and its repair obligation
  queued; assert the obligation survives every pass and the whole run. The control is a queued
  obligation for `HANDOFF_STRAY`, a chunk no class ever names, which the pass **must** drain —
  so "the obligation survived" cannot pass vacuously over a loop that drained nothing.

The fragment is left off disk rather than placed intact on purpose: #814 will rebuild an
intact staged chunk and may then drain its obligation as a duplicate finding, so an intact
seed would make this leg go red on #814's landing (the same reasoning the brief gives for
leg E, `brief.md:53-55`).

Two new campaign legs (`reconstruction_staged_handoffs_never_drain_the_obligation`, the
seeded sweep; `…_reach_between_and_outside_the_reads`, the exhaustive spacing walk that proves
the windows are reached), both registered through `dst_campaign_test!`, and the seeded one
added to `REGRESSION_SEEDS`' replay. The `handoff_landings` analysis needed no change:
reconstruction reads the same three subjects (`sidx:<id>:`, `part:<id>:`, `scan("inode:")`) the
existing `Handoff::reads` already names.

Red proof, two ways:
- **destination-first ordering only** (staged read moved after `read_committed`, nothing else):
  at 50 seeds both new legs FAILED; `gc_*` legs green. The failure prints the store-seam event
  log showing `Read("inode:")` then `Landed(Flip)`, `Landed(Drain)`, then the `part:` read.
- **full base reconstruction** (production reverted to `4ab2b28`, only the two seam-field
  initialiser pairs stripped from the test as the brief prescribes): both new legs FAILED at
  50 seeds, `gc_*` legs green.

Cost of the alternative I did **not** take — a second, standalone reconstruction fixture
instead of parameterising the existing one: the fixture is 195 lines
(`dst/tests/custodian.rs:2841-3055`) of record seeding, the concurrent writer task and the
landing analysis; duplicating it is ~195 added lines against the 211-added/64-changed the
parameterisation actually costs, and it would leave two copies of the publication batches to
drift apart. Measured from `git diff --numstat`.

## What I removed again after adding it (and why)

I first also added two scrub legs purely to push the advisory diff-coverage number: a paged
`mpu:` listing leg and an unparsable-session-key leg, plus the `cap` / `ScanCapExceeded`
machinery the first needed. Measured coverage **with** them: 92.8% (128/138). I dropped the
paging leg and its machinery (~62 lines) because `staged_committed_parts` walks the *same*
`walk_staged_range` / `staged_page` helpers `staged_fragments` does, and
`staged_protection.rs:1101-1193` already pins their paging — so the leg re-proved a shared
helper while the human's standing concern about this slice is its size. Coverage without it:
**92.0% (127/138)**, still far over the 80% floor (the one remaining gap it covered is
`gc.rs:1612`, the session-listing cursor). I kept the unparsable-session-key leg (**C'''**):
it is a distinct fail-closed branch of the new reader (`gc.rs:1598-1604`) that nothing else
reaches, and it is the rubric's "absent or unsupported entries" class.

## Failing gates from iteration 1, now

- **C4 diff coverage** 78.6% → **92.0%** (127 of 138 instrumentable changed lines; floor 80%).
  Re-measured with the real gate: `PDCA_BUNDLE=… ./engine/scripts/run-diff-cov.sh` → PASS.
  The 11 remaining misses are lines the gate structurally cannot reach: it scores
  `wyrd-custodian` with `-p wyrd-custodian --test staged_scrub` only (the added test file), and
  legs D–F must live in `staged_protection.rs` per the brief's Verification posture. They are
  `reconstruction.rs:219-220` (the staged-fault emit), `:1229-1231` + `:1238`
  (`emit_unresolvable_staged` / `emit_staged`), `gc.rs:1424-1427` (`StagedSet::place`'s hold
  fault, GC's side) and `gc.rs:1612` (the session-listing cursor). All are covered by
  `staged_protection.rs` and, for the first group, by the DST campaign.
- **T4 batched rubric review** — the 9 findings were 4 distinct issues (three of them seen by
  three passes each); all four are fixed above.

## Size, stated plainly

The patch grew **123 KB → 176 KB** against v1. Per file, added/changed lines
(`git diff --numstat`): `staged_scrub.rs` +670 → +1076 (the publication-race fixture, the hook
machinery it needs, and three new legs); `dst/tests/custodian.rs` +14/-0 → +211/-64 (the
`Driver` parameterisation); `staged_protection.rs` +408 → +459 (finding 3's leg);
`gc.rs` +131/-12 → +199/-30; `scrub.rs` +116/-51 → +148/-52 (the reorder shows as a moved
block); `reconstruction.rs` +168/-13 → +172/-13. Every one of those four test additions is
something the carry-forward asked for by name. I trimmed the one thing it did **not** ask for
(see above, ~62 lines). The size backstop will flag this again and the judgment is the
human's: the alternative is re-splitting the slice, which is an iterate-plan, not something I
can do from here.

## Why the remaining two carry-forward items are untouched

The carry-forward's last paragraph names two items as "needing a human decision, not a code
fix": confirming the prior-art/merged-PR search claim (`brief.md:144`) and fitness-to-purpose
given #814 is still deferred (`brief.md:121`). I left both to the human at sign-off, as the
carry-forward says. I did not re-run the prior-art search.

## Commands and counts (for the record)

Run through the project's own runner where it is the whole gate, and with a targeted
`cargo test` (under an explicit `timeout`) for the iteration loop:

```
./engine/xtask.sh ci                                   → "xtask ci: all checks passed"
cargo test -p wyrd-custodian                           → every target green
cargo test -p wyrd-custodian --test staged_scrub        → 11 passed
cargo test -p wyrd-custodian --test staged_protection   → 32 passed
RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 \
  cargo test -p wyrd-dst --test custodian               → 26 passed
cargo fmt --all; cargo clippy --workspace --all-targets → clean
RUSTFLAGS=--cfg madsim cargo clippy -p wyrd-dst --all-targets → clean
./engine/scripts/run-verify.sh --classify patch.diff
  → ADDED_TEST crates/custodian/tests/staged_scrub.rs
    CRATE crates/{chunkstore-grpc,custodian,dst,server}
PDCA_BUNDLE=… ./engine/scripts/run-diff-cov.sh          → PASS 92.0%
```

### The C4-verify red, run by hand

Emulating what the gate does (production + every **modified** test file reverted to
`4ab2b28`, only the **added** test file kept):

```
cargo test -p wyrd-custodian --test staged_scrub
  → 2 passed; 9 failed
```

The 9 failures are legs A (×3 positives), C (×2), C' (×2), C'' and C'''. The 2 passes are
leg B (the over-reach guard — the brief predicts it passes on base,
`brief.md:61-62`) and leg A's intact-fragment control.

### The D–F red, run by hand (the brief's prescribed method)

Base production, `staged_protection.rs` from this patch with **only** the two
`ReconstructionContext` seam-field initialisers removed from `reconstruction_pass`:

```
cargo test -p wyrd-custodian --test staged_protection
  → 26 passed; 6 failed
```

Failing: `reconstruction_keeps_an_obligation_a_committed_part_still_names`,
`…_an_owned_entry_still_names` (leg D), `…_across_a_publication_between_its_reads` (leg E),
`an_unreadable_staged_record_holds_back_every_drain` (leg F),
`an_unreadable_staged_record_is_named_even_when_the_committed_read_then_faults` (finding 3),
and `scrub_checks_committed_parts_and_never_reads_owned_entries` (this slice's rewrite of that
file's own leg F). Passing: leg G's drain control and every pre-existing leg A–E, unedited.

### The DST red, run by hand

```
# production reverted to 4ab2b28, only the seam-field initialiser pairs stripped from the test
RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian -- staged_handoffs
  → 2 passed; 2 failed
```

Failing: both `reconstruction_staged_handoffs_*`. Passing: both `gc_staged_handoffs_*`.

## Self-refutation (the three forced questions)

**(a) Genuine red?** Yes — proved four separate ways above, each by actually reverting and
re-running, not asserted:

| what was reverted | what went red |
| --- | --- |
| the whole production change (C4-verify's own posture) | 9 of 11 `staged_scrub.rs` legs; 6 of 32 `staged_protection.rs` legs; both new DST legs |
| **only** the scrub read order (finding 1) | 1 leg: the publication-race regression |
| **only** `read_staged_part`'s placement rule (finding 2) | 1 leg: the empty-placement leg |
| **only** the reconstruction emit position (finding 3) | 1 leg: the committed-read-faults leg |

Each single-finding revert reds exactly the leg written for that finding and nothing else —
so no leg is passing for a neighbour's reason.

**(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step`, the one
fenced control point, with a real `ScrubContext` / `ReconstructionContext` / `GcContext`. The
doubles (`Meta`, `Disk`, `HandoffMeta` over `SimTikvMetadataStore`) implement only the
production `MetadataStore` / `ChunkStore` traits — there is no scrub- or
reconstruction-specific test seam anywhere in the patch. The queue is read through
`wyrd_core::repair::queued_repairs` (the DST `queued()` helper) and
`repair::repair_key` (the unit legs), never a key the test spells itself. Every seeded record
is round-tripped through the base decoders (`decode_session_record`, `decode_part_record`,
`decode_owned_entry`) and asserted byte-identical to the decoder's own spelling before a pass
reads it.

**(c) Fixture includes the fault?** Yes, and the fixtures are built so the fault cannot be
curated out:
- The publication-race legs seed the moving chunk with its fragment **missing** and assert a
  `WITNESS` object's own missing fragment is enqueued in the same pass — a pass that checked
  nothing would fail the witness assertion, and `assert_published_in_two_batches` fails if the
  flip and the drain did not both land inside the pass as two separate batches.
- The empty-placement leg seeds **no** fragment anywhere, so the identity fill it forbids is
  exactly what would produce a finding.
- The DST reconstruction leg seeds the fragment **lost** with its obligation queued, and pairs
  it with a control obligation the pass must drain; the exhaustive coverage leg asserts each
  of the three moves landed between the two reads it hands protection across **and** outside
  them, and that at least one run landed the flip and the drain inside one pass's window — the
  one schedule a destination-first reading loses the chunk on. Without that leg a span that
  drifted away from the reads would leave the campaign green with nothing behind it.
- Nothing is asserted by count: the queue legs assert set membership of a specific chunk id,
  the disk legs assert a specific `FragmentId` is present or absent.

## Scratch

Working copies for the revert/restore cycles lived in
`$PDCA_SCRATCH/pdca-builder-813-redleg` and are removed. Every source edit was made in
`$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`). No PR was pushed, opened, readied or
merged.
