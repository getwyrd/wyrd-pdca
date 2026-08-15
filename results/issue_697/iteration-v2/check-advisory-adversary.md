# Adversarial review — issue #697 (advisory, never gating)

Re-ran the asserted red→green independently: rebuilt the **base** `wyrd-custodian`
(`339da46:crates/custodian/src/reconstruction.rs`) and the **patched** crate side by side in a
scratch package and drove `reconcile_step` through the real fenced control point on both. The
C4-verify log's shape is honest — 6 of 8 legs fail on the base, legs 7 and 8 pass, exactly as the
brief declared. What follows is where the evidence is thinner than it reads, and where the fix
over-corrects.

## Findings

- **NEEDS-HUMAN [impl] — leg 8 silently drops the Rule E assertion the brief made it responsible
  for, and its fixture cannot satisfy it.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:615` drives with `Capture::default()`
  inline and never inspects the seam; the brief's leg 8 requires "the unreadable object's name is
  **already** on the audit seam even though the pass returns `Err` (Rule E) … Leg 8 binds the
  placement". Concrete: the leg seeds only `seed_unresolvable()` (`:609` → `inode:00`), and that is
  the *same* object whose resolve raises the injected `get` fault, so `locate_queued_chunks`
  propagates at `crates/custodian/src/reconstruction.rs:838` having named nothing — the assertion is
  unsatisfiable as fixtured, which is presumably why it is absent. Swapping `:609` to
  `seed_damaged()` plants the undecodable `inode:0`, which sorts *before* `inode:00`, so a name is
  on the seam before the `Err` and the property becomes testable. As shipped, the load-bearing half
  of Rule E ("a later transient store fault cannot cost the operator the name of the record to
  repair" — brief, and `reconstruction.rs:727-728`) has **no** coverage on the `Err` path; only
  leg 3's `Ok` path is covered.

- **NEEDS-HUMAN [impl] — leg 5's red is confounded: on the base every substantive assertion in it
  already holds, so it does not demonstrate the loss it exists to prevent.**
  `crates/custodian/tests/segmented_map_reconstruction.rs:509`. Measured on base `339da46` with the
  leg's own fixture (outcome printed before the enum assertion): `outcome = Satisfied`,
  `queued = [161, 211]` (both `C_REPAIR` and `C_DUP` still queued), `inode:40 = [0,1,4]`,
  `inode:41 = [0,1,4]`, `inode:42[0] = [0,1,4]`, `inode:42[1] = [2,3,4]`. So `"neither"` (`:528`),
  the three `"no repoint"`s (`:531`), `"nor the duplicate"` (`:536`) and `"nothing rebuilt"`
  (`:537`) are all **already true on the base**. The cause is `stored()`
  (`crates/custodian/tests/segmented_map_reconstruction.rs:214`), which seeds every root in a
  space-injected spelling: the base's CAS is `require(key, metadata::encode(&plan.prior))`, so it
  conflicts on *every* object and repairs nothing anywhere. The brief's stated base behaviour —
  "Today the base repairs whichever reference `find_chunk` meets first (`:639`) and drains the
  obligation" — is therefore **not** reproduced; the leg goes red only on `Reconciled::Blocked` and
  on the absent audit rows. Seeding leg 5's three roots canonically (`metadata::encode`) would make
  the base actually repoint one reference and drain `C_REPAIR`, turning `:528`/`:531`/`:536` into
  real discriminators of the ambiguity rule.

- **NEEDS-HUMAN [impl] — the unparsable-key containment at `crates/custodian/src/reconstruction.rs:821`
  is unmandated over-containment: it permanently stalls the whole loop over a record it could
  repair perfectly well.** `parse_inode_key`'s *result* is discarded — it is the only call site
  (`:821`, `:906`) and the raw key is what is read, CAS'd on and named (Rule C, `:869`, `:688`) — so
  the check buys nothing, yet its `cannot_account_for` marks a fully decodable, `Committed`, flat,
  repairable record as unaccounted. Measured, patched build: a store holding `inode:not-an-id`
  (a valid committed flat record) beside `inode:9` (healthy work) and one obligation for a chunk no
  map references answers `Blocked`, leaves the odd-key object's under-replicated chunk at
  `[0,1,4]` **unrepaired**, and leaves the genuinely-unreferenced obligation queued — store-wide,
  every pass, forever, with no remediation but deleting the row. The brief pinned containment as
  "on **any** read fault by exactly gc.rs's downcast rule (`gc.rs:402-416`) … and no other"; a key
  that will not parse is not a read fault, and `gc.rs:365-410` — the walk this is a third copy of —
  has no parse check at all, so reconstruction alone now stalls on a row GC certifies over. (In
  fairness the base is *worse* here: it silently drains both obligations, `outcome = Changed`,
  `queued = []` — a real loss. The finding is that the fix over-corrects into a permanent stall
  rather than simply repairing the record under its raw key, which is exactly what Rule C enables.
  Either drop the check, or name without counting it toward `unaccounted`.)

- **NEEDS-HUMAN [human] — the discriminator is 19% over the brief's semantic budget; adjudicate
  whether the "shape is wrong" clause bites.** `crates/custodian/tests/segmented_map_reconstruction.rs`
  is **452** semantic lines (620 raw − 42 blank − 126 comment-only) against the brief's
  "≤ **380** semantic / 620 raw", and sits at *exactly* 620 raw — i.e. trimmed to clear the stated
  STOP trigger while the semantic budget went unchecked (C1 Spec is `"result": "none"` in
  `check-gates.json`, so nothing measured it). The brief's own reading of an over-budget test file
  is "the shape is wrong: STOP and hand back rather than finish"; only the raw cap is written as the
  trigger, so this is a judgment call, not a mechanical failure.

## Refutations attempted and **not** landed

- **Two repairable obligations inside ONE flat record.** Built it: one committed flat inode holding
  two RS(2,1) chunks, each missing fragment 2, both queued. Patched build repairs only the first —
  `placements = [0,1,2] [0,1,4]`, `still queued = [162]`, outcome `Changed` — because every plan
  carries the *same* `Arc<[u8]> prior_bytes` from the one reading and CASes on it
  (`crates/custodian/src/reconstruction.rs:690`), so the second commit loses and reports a spurious
  `reconstruction_conflict` after having already written its rebuilt fragments as orphan garbage.
  **This is not a refutation:** the base produces byte-identical results (`[0,1,2] [0,1,4]`,
  `queued = [162]`, `Changed`), because there too every `assess` runs before any `repair_chunk`.
  Pre-existing debt this diff neither introduces nor worsens — worth a tracked issue (one chunk per
  object per pass on a large multipart object), not a block on this bundle.
- **Rule A's value-equality guard** (`crates/custodian/src/reconstruction.rs:853`). Tried to find a
  resolve that restarts yet compares equal: `resolve_snapshot` short-circuits flat maps at
  `crates/core/src/metadata.rs:2585` (so `Superseded`/`Gone` are unreachable for a flat record), and
  `root_dropped` (`crates/core/src/metadata.rs:2337-2341`) only reports `Superseded` when the live
  root names a *different* group or a flat map — which forces the records unequal. Could not
  construct a mixed reading.
- **The two new abort paths in `repair_chunk`** (`:666` `as_flat` and `:678-681` index/id). Could
  not reach either; both are second guards behind facts `locate_queued_chunks` already established.
  Their being unreachable is a strength, not a hole — nothing is committed on either path.
- **C5's one surviving mutant is provably equivalent, so the `fail` row is noise.**
  `reconstruction.rs:261:31` (`+=` → `*=`) sits in the `Assessment::Withheld` arm; `Withheld` is
  returned only when `index.unaccounted != 0` (`:409-412`) and `noncertifying` is seeded from
  `index.unaccounted` (`:169`), so that increment can never change `noncertifying > 0`. Line 261 is
  effectively dead. (The `TIMEOUT` row at `:853` is the rule-A guard inverted, which stops all
  convergence — a hang, not a survivor.)
- **Rule B for the refusal path** (a `seg:`-resident refusal must not cost a healthy flat object its
  repair in the same pass): read the loop at `:304-320` — plans execute unconditionally of
  `noncertifying`, and leg 3 proves the equivalent for the unaccounted path. Could not break it.
