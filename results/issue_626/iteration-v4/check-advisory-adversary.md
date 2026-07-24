# Adversarial review — issue 626, iteration 4 (0016 multipart commit protocol)

Lens: refute the reworked design against the brief's Refutation standard (outcomes a–d absent
from the execution register) and the D-F honest-arithmetic bar. All citations grounded on
`$PDCA_TARGET` (wyrd.pdca-wt @ cd82a29 + this patch); `0016:N` = new-file line N in
`docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Note the deterministic T4 gate
is already red (6 blocking, `check-gates.json`); the findings below are independent of it —
I have not seen `review-batch.md`.

## Refutations that landed

- NEEDS-HUMAN [impl] — **`MAX_PART_CHUNKS` contradicts the doc's own knob rule — the capacity
  arithmetic is computed at a forbidden value, and the max-part-size ceiling is never stated
  (leg B(iv)).** The knob table (`0016:695`) binds `max_chunkref_bytes × MAX_PART_CHUNKS ≤ V/2`,
  i.e. `MAX_PART_CHUNKS ≤ 165–381` (same `b_ref` = 131–302 B as `0016:640-641`) — a `part:`
  record is one JSON value under the 100 KB ceiling. Yet the `W_ref`/`MAX_SESSIONS` arithmetic
  (`0016:943-945`, register `0016:1419`) is computed at `MAX_PART_CHUNKS = 5,120` ("a 5 GiB
  part") — 13–31× outside the knob's own valid range — and the narrative repeatedly treats a
  5 GiB part at 1 MiB chunks as admissible (`0016:633`, `0016:839`). Consequence if the knob
  rule is real: at the default 1 MiB chunk (`crates/server/src/lib.rs:51`) the maximum part is
  **165–381 MiB**; an S3-legal 5 GiB part cannot commit its part record (over-`V` value = a
  permanent `UploadPart` failure — the same class as iteration 1's hidden 165–390 MiB map
  ceiling, one record class down). The computable number `max_part_bytes = MAX_PART_CHUNKS ×
  chunk_size` appears nowhere; the S3-conformance consequence (parts above ~165–381 MiB refused
  at default chunks, vs S3's 5 GiB part maximum) is neither registered as an accepted cost nor
  flagged. Reconcile: state the part-size ceiling per chunk-size as a real number, recompute the
  `W_ref` scenarios at in-range values, or design part-record segmentation.

- NEEDS-HUMAN [impl] — **The `sidx:` record as specified cannot deliver the two load-bearing
  iteration-3 fixes (findings 2/4): its value carries no placement.** §1 fixes the value to
  `PendingEntry { owner, lease_expiry_millis }` (`0016:221`, `0016:242-264`; `PendingEntry` today
  is `lease_expiry_millis` only, `crates/core/src/metadata.rs:344-350`; `write::intent` writes
  exactly that, `crates/core/src/write.rs:198-214`). But (a) the reaper must "orphan-mark +
  delete owned `sidx:` entries" (`0016:365`, `0016:977-978`) and orphan records are
  **placement-keyed** — `orphan:<dserver>:<chunk>:<index>`
  (`crates/core/src/metadata.rs:68-70`); a record-only reaper (D-A, `0016:864`) holding only a
  chunk-id-in-key cannot compute those keys, and deleting the entry without evidence strands the
  fragments unreferenced-and-unevidenced — kept forever under `Defer` (outcome (a), the exact F3
  class the decision exists to close). (b) `genuinely_holds` must count in-flight owned
  fragments as held **on a specific draining server** (`0016:481`, X14 `0016:1232`), but it is
  computed from `(DServerId, FragmentId)` pairs (`crates/custodian/src/gc.rs:228-247`,
  `crates/custodian/src/desired_state.rs:157-164`) — unresolvable from a chunk id. The fix is
  additive (write the `WritePlan`'s placement into the `sidx:` value at intent time) but it is a
  record-shape change ADR-0046 requires stated, and the serialization-identity section
  (`0016:250-264`) currently covers only `owner`.

- NEEDS-HUMAN [impl] — **The cursor-keyed `retire:` walk rests on a store primitive that does
  not exist, and the seam change is nowhere acknowledged.** `MetadataStore::scan` is
  prefix-only, complete-or-fail-loud, no cursor/limit/range (`crates/traits/src/lib.rs:772-776`,
  `SCAN_CAP` at `:286`). The doc lets `retire:` grow past `SCAN_CAP` (X39 `0016:1258`,
  `0016:931`) and disposes of it by "cursor-keyed bounded key ranges" (`0016:223-224`,
  `0016:983-984`) — not expressible with `scan(prefix)`: any prefix covering the namespace fails
  `ScanCapExceeded` with no partial result, and fixed sharding only divides an *unbounded*
  (alarmed-not-bounded, `0016:931`) population by a constant. Enumerating obligations therefore
  needs a new ranged/limited scan on the `MetadataStore` seam — a change every backend, the DST
  sim store, and the conformance suite must implement (ADR-0010/0016 narrow-seam rule) — which
  "What the implementing slices change" (`0016:1266-1303`) does not name. X39's disposal is
  unimplementable as written.

- NEEDS-HUMAN [impl] — **F13's disposal has an unregistered window: restore-then-serve-before-
  fence.** X17 (`0016:1235`) disposes the sharpest carried trace by "the restore fences every
  resurrected session"; the fence lives in the restore pass / restore tool (`0016:477`,
  `0016:1515-1517`) — but nothing orders that fence against gateways resuming service on the
  restored image. Concrete execution: image goes live → the client's retried Complete arrives
  **before** `reconcile_after_restore` runs → the session is `Open@E` in the image, the fence
  CAS succeeds, the records-only proof passes over resurrected `part:` records → publication
  over GC-reclaimed bytes — outcome (c), the very F13 trace, absent from the register in this
  interleaving. Needs one normative line: the fence completes before the store serves multipart
  verbs (or gateways refuse multipart until the restore fence generation completes).

## Secondary findings (consistency / register quality)

- NEEDS-HUMAN [impl] — **Stale "≈ 52 × SCAN_CAP" arithmetic contradicts the doc's own bound.**
  `MAX_OWNED_FLEET = MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS ≤ W_ref ≤ SCAN_CAP` by
  the knob table's own ranges (`0016:699-700`), yet three passages still claim the fleet owned
  population "can reach ≈ 52 × SCAN_CAP" (`0016:477`, `0016:752`, X25 `0016:1243`, echoed
  `0016:1334`) — an iteration-3-era figure now unconstructible under the doc's own enforced
  admission (X25's test premise cannot be set up), and the `MAX_OWNED_FLEET` row's "NOT bounded
  by `SCAN_CAP`" is false given `W_ref ≤ SCAN_CAP`. The `sidx:`-disjointness argument survives
  without the inflated number; the numbers should agree with the knobs (D-F).

- NEEDS-HUMAN [impl] — **`seg:` maintenance writers are missing from the ADR-0046 contract, and
  the repoint-vs-drain race is unregistered.** §1's `seg:` row (`0016:222`) names only the
  Completing segment-write phase as writer, but decision 2 commits reconstruction to repairing
  committed objects (placement rewrite, `0016:479`, `0016:514`) and rebalance to evacuating
  committed objects (`0016:480` excludes only *staged*) — for a committed **segmented** object
  both must rewrite `seg:<id>:<E>:<i>` placement. No precondition serializes such a rewrite
  against the retirement drain deleting/enumerating the same records on supersede/delete
  (`0016:1142-1148`; the drain step preconditions only the obligation, `0016:363`): a drain that
  enumerates a segment's old placement, races a re-place repoint, then deletes the record leaves
  the moved fragment unreferenced and unevidenced (outcome (a) for that fragment). Add the
  writer rows and the exact-bytes CAS rule + a register row.

- NEEDS-HUMAN [impl] — **Reference-build work at the segmented ceilings is uncharged — the same
  class as iteration 3's rejected ~10^10-reads claim, reintroduced for committed objects.**
  `W_ref` charges sessions' `part:`/`sidx:` reads (`0016:933-951`) but decision 7(e)
  (`0016:1132-1140`) claims only scan-safety for segment resolution: committed segmented objects
  add up to `MAX_ROOT_SEGMENTS` (312–520) record reads per inode per reconcile pass with no
  budget, knob, or alarm, and one max segmented object alone contributes ~198K chunks → ~1.78 M
  `(server, fragment)` pairs to the in-memory `ReferenceSet` (`gc.rs:228-238`). The segmented
  population is bounded by nothing but the `inode:` scan cap, so per-pass custodian work/memory
  grows orders of magnitude past today's ceiling with no register row (D-F).

- NEEDS-HUMAN [impl] — **Grace-start contradiction, delete path.** Decision 4 claims "the
  reader-safe grace still starts when the object becomes unreferenced" (`0016:667-669`) while
  the accepted-costs row states "grace starts at drain, not at the supersede/delete commit"
  (`0016:1423`). The latter is what the mechanism does (`orphaned_at` is stamped by the drain,
  `metadata.rs:470-486`); the former sentence is false as written. Direction is safe; the text
  should say one thing.

## Attempted and could not refute

Attempted, against the register and by fresh construction, and **could not** refute: the
fence/epoch machine and the O(1) session-precondition publication proof (every part/intent/slot
batch carries `require(mpu == Open@E)`, checked at commit — the post-fence residue race of
iteration 3 finding 1 is genuinely closed at creation); the per-attempt epoch-scoped `seg:` keys
(re-ran the X40 rollback→re-Complete-while-obligation-pending trace — the disjoint epochs hold);
the exactly-once terminal decrement (exact-bytes precondition on `mpu:` serializes gateway vs
reaper); the Completed-path `sidx:` walk (iteration 3 finding 2 closed); the `409`-vs-resume
contradiction (resolved cleanly in favour of `409`, cost registered); the byte-budgeted batch
inventory (checked every row against `E_tx/2`); and the D-C/D-D tension, surfaced as the ⚑
NEEDS-HUMAN question exactly as iteration 3 directed. Leg-A mechanics verified: frontmatter,
template section set, index row, link targets resolve (`docs/design/{adr,architecture}`), and
`typos` is clean on the new file.
