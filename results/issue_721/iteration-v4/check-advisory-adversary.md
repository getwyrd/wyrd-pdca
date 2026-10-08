# Adversarial review — issue #721 (advisory, never gating)

Re-ran the asserted red→green myself on a scratch clone of `$PDCA_TARGET`: with
`crates/core/src/metadata.rs` and `crates/custodian/src/reconstruction.rs` reverted to the base
and the new test kept, `cargo test -p wyrd-custodian --test segmented_map_repoint` fails 3 of 7
(legs 1, 2 and the torn-record leg, each with the base's `refused-segmented` audit row in the
panic message); with the two production files restored it is 7/7 green. The discriminator drives
the real control point (`reconcile_step`) and observes the store, not a parallel re-implementation.
The red is genuine and the gate's claim holds. Two findings survive that.

- **NEEDS-HUMAN [impl] — a zero-length chunk at the *tail* of a `seg:` record is unrepairable
  forever, silently, and the pass certifies `Satisfied` over it.**
  `crates/core/src/metadata.rs:2869-2874` picks the covering segment **by offset alone**
  (`covers`, `:2922`) *before* the `ChunkRef` equality pin is applied, so a chunk whose object
  offset equals a segment boundary but which lives at the end of the **previous** record is looked
  up in the wrong record and answered `Repoint::Conflict`. The flat arm has no such split — its
  `chunk_at` (`:2932`) scans every chunk sharing that offset and finds the one that equals `prior`
  — so the two arms of one primitive disagree on the same chunk list. Reproduced twice on the
  patched tree: (a) at the primitive, `repoint_chunk` over segments `[[c(len 8), z(len 0)] @0,
  [c(len 8)] @8]` moving `z` at offset 8 answers `Conflict`, while the identical chunk list as a
  flat map answers `Prepared`; (b) end-to-end through `reconcile_step` with `z` queued, three
  consecutive passes each answered `Reconciled::Satisfied` with the obligation still queued, the
  `seg:` record byte-identical and nothing named on the durability seam. That is exactly the C-1
  shape the brief says the fix must remove ("no actor that exits the state") re-entered through a
  different door, and it is *worse* than the base, which at least named the object and answered
  `Blocked`. Note the report path compounds it: `reconstruction.rs:930` routes this prepare-time
  refusal to `emit_conflict` (`:1099-1106`), whose row states "lost the version-conditional
  commit" when no commit was attempted and no fragment was written — so a permanent condition is
  logged as transient churn. (I am **not** asking for the "collectable garbage" wording, which
  getwyrd/wyrd#723 owns.) Zero-length chunks are format-admissible today —
  `SegmentRecord::checked` (`:1179-1201`) rejects only an empty list and a zero *total* — and
  `erasure::encode(1,1,&[])`/`reconstruct` both succeed, so `assess` does plan such a chunk; I
  found no in-tree producer that mints one yet (the segmented committer is #653's), which is why
  this is a latent input rather than a live outage. Cheap fix: pick the covering segment *and* the
  segment ending at `byte_offset`, then let the existing equality pin choose.

- **NEEDS-HUMAN [human] — leg 3's "the pass does not certify" is not what the shipped test
  asserts, and the behaviour is the opposite.** `crates/custodian/tests/segmented_map_repoint.rs:485-489`
  asserts only `assert_ne!(outcome, Reconciled::Changed)` under the message "the pass must not
  certify a repair it did not make". `Changed` is not the certification — `Satisfied` is
  ("Reality already matched the desired state; nothing was done",
  `crates/custodian/src/reconciliation.rs:21-22`). I ran leg 3's own fixture and printed the
  outcome: the pass answers **`Satisfied`** with the obligation still queued and the chunk still
  under-replicated, because `hole` at `crates/custodian/src/reconstruction.rs:363` counts only
  containment and the ceiling refusal, never a conflict. So the brief's leg-3 requirement ("no
  orphan mark was published; **the pass does not certify**") is unmet, and the assertion is worded
  to look as though it is met. This needs a human because the two exits differ in blast radius:
  tightening the test to assert `Satisfied` amends the stated success criterion, while making a
  repoint conflict a hole changes the **flat** arm's standing base behaviour too (a lost CAS has
  always answered `Satisfied`) and is outside this slice's scope. Finding 1 above is what makes
  the choice load-bearing rather than cosmetic: a *permanent* conflict is certified as `Satisfied`
  on every pass forever.

## Attempted and could not refute

- The read→prepare window in legs 2/3 is genuinely reached, not aspirational: `MemMeta::scan_page`
  fires the racing batch after materialising the `seg:` page (`segmented_map_repoint.rs:106-119`)
  and every racing leg self-checks `meta.raced()`, so a leg cannot pass because the race never
  landed. The parent attempt's `RaceAtRepoint` shape is not reproduced.
- Two obligations inside **one** `seg:` record: I expected the second to lose its CAS on bytes the
  first superseded; both land in one pass (`placements(1) == [[0,2],[0,2]]`, queue empty), because
  the primitive re-reads the record and pins only the planned `ChunkRef`. The merge design holds
  under its own worst case.
- Serialization identity of the segmented CAS: the `seg:` precondition uses the **raw stored
  bytes** (`metadata.rs:2879`, `:2912-2917`), not a re-encode, so a racing writer's row is pinned
  byte-exactly; `SegmentRecord::new(record.chunks().to_vec(), record.byte_offset())` drops no field
  (`byte_len` is re-derived from the same chunks) and the root, which *is* re-encoded, carries no
  `ChunkRef` in the segmented arm.
- Boundary of the V/2 ceiling (`segment_value_ceiling_crossed`, `metadata.rs:386-398`): admits
  exactly `MAX_ROOT_VALUE_BYTES`, refuses `+1`, refuses before any fragment write; a shrinking
  repoint of an already-oversized record is still allowed. No off-by-one.
- Ordering: `repoint_chunk` now runs **before** the fragment writes (`reconstruction.rs:924-936`),
  so a ceiling refusal and a prepare-time conflict strand nothing new; only the commit-time CAS
  loss does, which is the pre-existing #723 leak the brief scopes out.
- Containment dedupe: a torn `seg:` record met under two obligations produces one audit row and
  one counter tick (`Reading::contain`, `reconstruction.rs:412-424`), and the drain batch is gated
  on `incomplete()` *after* the repair loop, so a containment discovered at repoint time still
  suppresses the drain. The iteration-3 carry-forward items look genuinely addressed.
- Duplicate chunk id at two offsets in one segmented object: first reference wins, the second keeps
  the dead placement and the obligation is drained — identical to the base's flat behaviour and
  explicitly #700's (`reconstruction.rs:444-450`), so not filed.
- `C4-ci` red is `cargo deny` / RUSTSEC-2026-0258 (`h2` 0.4.15, reached through `hyper`/`tonic`);
  the log shows every test target green. That is a dependency advisory, not a defect in this patch,
  and the previous sign-off already scoped it out — **not** a refutation.
- The T4 batch review's `[CONVENTION]` item about the new unbounded `MetadataStore::get` await
  (`metadata.rs:2879`) reads as noise against this repo's own written rule: `read_committed`'s
  doc (`reconstruction.rs:494-500`) records that the bound on such awaits is the store
  implementation's, not the caller's (#508/#636), and every peer walk follows it.
