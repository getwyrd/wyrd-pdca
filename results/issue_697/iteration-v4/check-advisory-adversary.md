# Adversarial review — issue #697 (reconstruction reads through the resolver once, contained)

**Evidence re-run, independently, in both directions** (scratch clone of `339da46`, `cargo 1.96.0`):
base + new test → **8 of 9 legs red** (legs 1–6, 8, 9; leg 7 green, as the brief declares);
patched → **9/9 green**; whole `-p wyrd-custodian` suite green, `tests/reconstruction.rs` untouched
and green. The legs drive `reconcile_step`, name no patch-introduced symbol, and go red on
behaviour (`find_chunk met a segmented chunk map`, `namespace reads 3 ≠ 1`, `Satisfied ≠ Blocked`),
not on compilation. The C4-verify claim holds. Budget: production 229/230 semantic lines (pass);
test file **400 semantic vs. the brief's 380 cap** (614/620 raw), self-declared in its header.

The findings below are all against **leg 9 / `may_land`** — the one leg the builder added beyond
the brief's eight. The rest of the patch I could not break; see the last section.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:671`: `may_land` is applied on
  *every* claimed slot, but its own justification is conditional — so it permanently stalls a
  repair the base completes.** The guard's stated reason (`:662-667`) is "*While its reading of the
  namespace has a hole in it*, an object it could NOT read may reference this very `FragmentId`".
  Under a **complete** reading no claimant can be hidden — `CommittedIndex::note` (`:799-811`) has
  already turned any second claim into `Site::Refused`, so nothing is repaired anyway. Yet the call
  site never consults `index.complete()`. Demonstrated failing case (run, both directions): one
  committed flat object under `inode:7`, chunk `0xA1` RS(2,1) placed `[0,1,4]` with fragment 2
  absent, no segmented object, no unreadable record, and a stray non-identical fragment already
  standing at `FragmentId{0xA1, 2}` on server 2 (the domain the selector deterministically picks).
  **Base:** pass 0 → `Changed`, placement `[0,1,2]`, obligation drained. **Patched:** pass 0, 1, 2 →
  `Blocked`, placement still `[0,1,4]`, obligation still queued, a fresh `NEEDS-HUMAN`
  `would-overwrite` row each pass. There is **no exit**: the selector is deterministic, and GC never
  reclaims an un-orphan-marked stray (`crates/custodian/src/gc.rs:196-211` — no orphan lease, no
  expired pending lease ⇒ `reason = None`, "conservatively keep it"), so the chunk stays
  under-replicated forever. That is the permanent failure mode C-1 forbids, introduced by the fix
  rather than removed by it. One-line scope fix: only ask `may_land` when the pass's reading is
  incomplete.
- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:579-613`: leg 9
  never constructs the hazard it names, and pins the over-broad guard.** Its doc says the foreign
  bytes belong to "an object legs 2-4 show this pass may be unable to READ, which leaves
  `ambiguous-chunk-id` blind to it" — but its fixture is `seed_fixture()` + `inode:8`, every record
  of which decodes, parses and resolves. Measured on the patched tree: **0 `unaccounted` rows, 0
  `incomplete-reading` rows, 0 `refused` rows, 1 `would-overwrite` row** — the reading is
  *complete*, so in that store a hidden second claimant is impossible and the planted bytes could
  not exist. The leg therefore asserts nothing about the hidden-duplicate case and instead *locks
  in* the unconditional guard: scope `may_land` correctly (previous bullet) and leg 9 fails on
  `assert_eq!(held, Some(foreign))` at `:601`. Related gap in the same helper: no leg drives a
  **transient** `get_fragment` fault at the re-placement target, so the "fail-closed" half of
  `reconstruction.rs:756-760` is unasserted (`--in-diff` mutants replace whole functions, not match
  arms, so C5's 0 survivors does not cover it).
- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:215` + `:293`: a `would-overwrite`
  refusal floors `gauge.reconstruction_under_replicated` at ≥1 forever.** `under_replicated += 1`
  happens in the assessment loop; the refusal happens later, in the repair loop (`:326`), so a
  never-completable repair stays **on** the repairable-backlog gauge. Measured over three
  consecutive passes on the store above: `{"gauge.reconstruction_under_replicated":1}` every pass,
  with the repair never landing. This is exactly the defect class this file's own comments call an
  iteration-5/7 MUST-FIX (`:178-202`, `:236-242`, `:518-530` — `Assessment::Blocked` and
  `Assessment::Refused` are both deliberately diverted **off** the gauge for this reason) and which
  the brief pins as preserved ("including the rule that a never-repaired condition stays off the
  repairable-backlog gauge", brief §Scope). The day-one "rise then return to zero" signal is
  unobservable on any store carrying one such slot.
- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:1154-1164`: the `would-overwrite`
  NEEDS-HUMAN row names no object, though the pass is holding its key.** Rule E's own rationale
  (brief rule 7) is that "a fragment carries only `FragmentId { chunk, index }`
  (`crates/traits/src/lib.rs:45-48`) and there is no operator tooling (#694), so that name is the
  operator's entire situational awareness". `emit_would_overwrite` emits `dserver`, `chunk`, `index`
  and stops — captured row: `{"action":"would-overwrite","dserver":2,"chunk":"…a1","index":2}`, no
  `inode` field — while the caller has `plan.inode_key` in scope two lines away (`:672`, used at
  `:717`). Every other new emitter names its object (`emit_unaccounted`, `emit_ambiguous`,
  `emit_refused`). One added field.
- **NEEDS-HUMAN [human] — two written contracts are now contradicted, and the brief forbids editing
  either file.** (a) `docs/design/adr/0011-…md:32-42` documents exactly three reconstruction
  counters and the netting rule *"successful repairs ≈ `reconstruction_repaired` −
  `reconstruction_conflict` − `reconstruction_aborted`"*, naming `reconstruction.rs` as its source
  of truth; the patch adds a **fourth** terminal offset and rewrites the formula in a code comment
  (`crates/custodian/src/reconstruction.rs:282-285`) only — an operator applying the ADR now
  over-counts successful repairs by every `would-overwrite`. (b) `Reconciled::Blocked`'s public
  rustdoc (`crates/custodian/src/reconciliation.rs:22-44`) defines the outcome as *"at least one
  committed object's chunk map could not be read … so the reference set the loop reasoned over is
  incomplete"*; reconstruction now answers `Blocked` with a **complete** reading whenever
  `refused > 0` (`:339`). The brief pins "no new or edited ADR" and "exactly 2 files", so neither
  can be corrected inside this bundle — a human must decide whether to widen the budget, update the
  ADR, or drop the extra counter along with the guard above.

## Attempted and could not refute

Tried, with a working fixture, and failed to break: **Rule A** (`:924` compares `resolved.record`
by value — a flat resolve borrows the caller's record so equality is exact, and a superseded
segmented resolve genuinely restarts onto a differently-grouped root; leg 4 goes red on the base for
the right reason). **Rule B's backstop** — verified independently at `gc.rs:296-316` and `:191-195`:
`protection` really does withhold *every* fragment while `unresolvable` is non-empty, so a repair
made under an incomplete reading cannot have its displaced fragment reclaimed. **Rule C** — the base
demonstrably reads `inode:007` and CASes `inode:7` (I reproduced the lost CAS); the patch keys on the
raw scanned bytes throughout. **Serialization identity** — the CAS precondition is now the *stored*
bytes (`:719`), and the fixture's deliberately non-canonical spelling makes that binding. **The Q×N
property** — leg 6 measures 3 scans on the base against 1 with the patch, and `seg:` reads ≤ S.
**Containment reach** — I searched for a legitimate `inode:`-prefixed key that is not
`metadata::inode_key(id)` (which would make `unparsable-inode-key` block every pass forever); there
is none in the tree. **Multi-obligation objects** — two queued chunks in one flat record still
converge (one per pass, one bogus `conflict` row), byte-identical to base behaviour, not a
regression. **`assess`'s six existing classifications and their gauge accounting** are unchanged
apart from the flooring above.
