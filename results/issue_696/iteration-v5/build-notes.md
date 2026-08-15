# Build notes — issue 696, iteration 5 (rebalance reads through the resolver, contained)

**Withheld from the reviewer.** Rationale, alternatives, and the forced self-refutation.

Base: `origin/main @ 339da46`, built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
Two files, as the brief's `Budget` requires:

| File | Change | Budget | Measured |
|---|---|---|---|
| `crates/custodian/src/rebalance.rs` | modified | ≤ 130 semantic added | **94** |
| `crates/custodian/tests/segmented_map_rebalance.rs` | NEW | ≤ 330 semantic / 540 raw | **329 / 507** |

## 1. What this round changed, and why (the carry-forward)

Iteration 4 passed `C4-ci`, `C4-verify` (red→green) and `C5` (0 missed), and failed only
`T4-batch-review`. The human's sign-off named three concrete implementation items. Each is
addressed below; nothing else in the v4 production hunks was re-derived (the brief's
*Salvage first* instruction — those hunks are #681-v7's, already mutation- and
adversary-checked).

### (1) BUG — restore the `inode:` key validation (review-batch.md line 3)

*Finding:* removing `parse_inode_key` made any decodable `inode:`-prefixed row — including
`inode:foo` — eligible for evacuation **and a CAS write**, instead of being explicitly
refused.

The finding is right, and the base was not right either: the base *silently skipped* such a
row (`rebalance.rs:152-154` on `339da46`) and then reported the drain `Satisfied` over
whatever that row holds on a draining server — the same C-1 hole this slice exists to close,
one keyspace over. So neither "skip in silence" (base) nor "evacuate and CAS it" (v4) is the
answer.

**Fix** (`rebalance.rs:279-283`): `parse_inode_key` is restored **verbatim from the base**
(`:504-510`, byte-identical body, doc added) and read as a *predicate*. A committed row whose
key is not `inode:<id>` is **named on the audit seam and counted** — so the walk continues,
the row is a stated repair obligation, and the drain does **not** certify — and nothing about
it is planned, copied or written.

Two things make this the repo's own answer rather than an invention:

- `metadata::inode_key` is the **sole writer** of the prefix (`core/src/metadata.rs:2159-2161`),
  so such a row addresses no object: a CAS back under `inode:foo` would park a record where
  no reader looks (`read.rs:50` reads `inode_key(id)`). I verified there is no second writer:
  the other key families are `desired:dserver:`, `dirent:`, `pending:`, `bucket:`, `orphan:`,
  `seg:`, `seggrp:` (`core/src/metadata.rs:34-78`, `:1230-1277`,
  `custodian/src/desired_state.rs:33`).
- Startup recovery already answers the identical row this way — named under action
  `"unparsable-inode-key"`, walk continuing (`core/src/metadata.rs:2045-2047`, `:2158-2173`,
  merged as #652 / `b083ec4`). I reuse **that action string**, and GC's
  `"unresolvable-chunk-map"` (`gc.rs:563-571`) for the chunk-map faults, through one
  `emit_unaccounted(action, …)` (`rebalance.rs:562`) — so one fault is never filed under
  another's name.

Rule C is preserved *because* the parse is validation-only: the CAS still addresses
`plan.inode_key`, the store's own bytes (`:366`, `:475`), so a live record under `inode:007`
still drains and `inode:7` stays a different record. That is why the predicate is
deliberately **liberal about spelling** (`007`, `+3` parse) and strict about the *write* — a
canonical-spelling check would refuse `inode:007`, which the brief's leg 3 requires to be
evacuated, and would also make Rule C unbindable (with only canonical keys, re-deriving the
key and using the store's key are the same operation).

*Alternative considered and rejected:* name the row but **don't** count it (keep the base's
certification behaviour). Cost of the rejection is one line (`self.0 += 1` inside
`Refusals::unaccounted`, `:189`), so this is not a size argument: counting is what makes the
answer honest. A row the pass will never evacuate may hold fragments on the draining server;
`Satisfied` would tell an operator the decommission is safe. That is exactly the C-1 clause
the brief names ("it never claims more than it read").

### (2) TEST-GAP — Tier-0 DST for the continue-past-failure path (review-batch.md line 4)

Recorded-rejected in `review-rejected.md` at the finding's own anchor
(`crates/custodian/src/rebalance.rs:140`), at the rigor of the existing entries and on its
**new framing** (the work loop, not the resolver call already recorded at `:259`). Four
checkable facts, each a diff someone can run against `339da46`: the patch adds a `continue`
at the named site and no write; the write path is shape-identical to the base's
(`:459-484`); partial progress is base behaviour and its loss chain cannot close, because
the evacuation orphan-**marks** rather than deletes (`:479-484`) and `ReferenceSet::protection`
withholds **every** fragment while any object is unresolvable (`gc.rs:306-316`, consulted
before each delete at `gc.rs:191-194`); and the concurrent-writer question is answered by
the untouched version-conditional CAS plus Rule A's write-nothing containment (`:314-318`).

It is also **budget-forbidden here, not merely deferred**: a seeded Tier-0 case lands under
`crates/dst/tests/*` — a **third** file, which the brief's `Budget` says to STOP over rather
than add — and the brief's `Verification posture` pre-declares that no Tier-0 case ships in
this child. #682 builds it.

### (3) Leg 5's fixture is now a genuinely healthy segmented object

*Finding:* leg 5 seeded a segmented object one of whose chunks carried a **malformed**
placement, so `fragments == 0` could be reached for ADR-0040's reason rather than for the
reason under test — the leg could pass with the over-containment guard removed.

**Fix** (`tests/segmented_map_rebalance.rs:440-442`): `[(0xE1, vec![TARGET]), (0xE2, vec![TARGET])]`
— both `seg:` records written, both placements well-formed for `EcScheme::None`, nothing on
the draining server, nothing damaged anywhere in the store. Ablation below proves the leg is
now bound by the guard it names. Leg 6 keeps the malformed chunk (that is *its* property:
one refusal per object, counting only the fragments the pass can actually see).

Also removed: the `certifies` helper. Iteration 3's carry-forward required it to assert the
`Ok` variant explicitly; with that done it was a 6-line wrapper around `.expect()`, the idiom
the other six legs already use — so leg 5 now uses `.expect()` directly
(`:445`, `:455`). That freed the semantic-line budget the leg-3 sub-assertion needed.

### (4) The leg-3 sub-assertion that binds (1)

Leg 3 gains a third damaged object, seeded **last** in key order so it cannot disturb the
"damaged record met FIRST" property (`inode:001` < `inode:002` < `inode:007` < `inode:7` <
`inode:foo`): a committed, perfectly decodable flat record under `inode:foo` whose fragment
**is** on the draining server (`:371-373`). The leg asserts its bytes are unchanged, that
nothing of it was copied to the target (`:385-387`), and that it is named under
`"unparsable-inode-key"` (`:392`). Note the assertion names **no symbol this patch introduces** — only store keys
and audit strings — so the red leg still compiles against the reverted production file.

## 2. Alternatives ruled out

- **Keep v4's "no key validation at all"** — rejected: it is the reported BUG, and a CAS
  under a key `inode_key` never spells is a write nobody can read back.
- **Refuse the row *before* `decode`** — rejected: a row that is both unparsable-keyed and
  undecodable would then be filed under the key fault when the record is *also* unreadable;
  ordering it after the `Committed` check keeps an uncommitted row skipped exactly as the
  base skips it, and keeps one repair obligation per row.
- **A stricter canonical-key check (reject `007`, `+3`)** — rejected: it would refuse
  `inode:007`, which the brief requires evacuated (leg 3), and would make Rule C unbindable.
  The rubric's *Grammar strictness* class is about RFC wire formats; here the strictness that
  matters is "write where you read", which `EvacPlan::inode_key` enforces directly.
- **Adding the Tier-0 DST leg anyway** — rejected: third file, contradicts the brief's
  pre-declared posture; see (2).
- **Widening containment to `Err(_)` on the resolve** — rejected (unchanged from v4): a store
  fault is not one object's; leg 7 binds it and `gc.rs:405-415`'s downcast rule is the
  precedent the brief pins.

## 3. Forced self-refutation (the three questions)

**(a) Genuine red — does the test fail with the fix reverted?** Yes, through the project's own
runner: `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh results/issue_696/patch.diff` →
`GREEN — cargo test -p wyrd-custodian --test segmented_map_rebalance (fix applied)`, then
`RED — (production reverted, test kept)` with **6 of 7 legs failing** on the base
(`Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`), and the
gate's own verdict `PASS — red without the fix, green with it (7 test(s) ran red)`. The one
passing leg is leg 7, which the brief pre-declares non-red. Leg 5 is base-red as in v4 — the
open C1 Spec question the human flagged for sign-off, not implementation work this round.

Two targeted **ablations** on top of the fix (each reverted afterwards; the file was restored
from a backup and re-verified):

| Ablation | Result |
|---|---|
| `parse_inode_key` → `Some(0)` (validation neutered, function still used) | leg 3 **FAILS** — the `inode:foo` record is evacuated: `version` 2, `placement [1]`, and the fragment appears on `d1`. Exactly the reported BUG, now caught. |
| `if fragments == 0 { return; }` → `if false` (the v7 adversary's mutation) | legs 1 **and** 5 **FAIL** — the pass answers `Blocked` where it must answer `Changed`/`Satisfied`. At v7 this mutation survived the whole suite; it does not survive this fixture. |

**(b) Production path — does the test drive the code the fix changes?** Yes. Every leg drives
`wyrd_custodian::reconcile_step` — the real fenced control point — over a real
`Custodian::elect` + `FencedZone` on `wyrd_coordination_mem::MemCoordination`, with a real
`RebalanceContext`. Nothing internal to `rebalance.rs` is called by the test, no logic is
re-implemented; the doubles are only the two trait seams (`MetadataStore`, `ChunkStore`), and
the flat objects carry **real checksummed bytes** written through `wyrd_core::write::plan_write`
/ `write_fragments`, because `repair::fragment_intact` (`rebalance.rs:447`) rejects anything
else — a stubbed fragment would abort the evacuation and the "is evacuated" assertions would
fail.

**(c) Fixture includes the fault?** Yes — each leg's store *contains* the failing element
rather than curating it out: leg 1/5 a real segmented root with its `seg:` records; leg 2 a
`seg:`-resident fragment actually sitting on the draining server; leg 3 an unresolvable root
(asserted unresolvable in the fixture via `resolve_chunk_map`, `:356-357`), an undecodable
record, an `inode:007`/`inode:7` pair, **and** the unaddressable `inode:foo` row; leg 4 a
`scan`/`get` disagreement that makes the resolver genuinely restart; leg 6 a 3-chunk object
with 2 draining fragments; leg 7 an injected non-`ChunkMapError` store fault. No leg asserts
over a store from which the fault was removed.

## 4. Gates run locally (in `$PDCA_WORKTREE`, through the project's runners)

- `./engine/xtask.sh ci` → **exit 0**, `xtask ci: all checks passed` (fmt `--check`, clippy
  `-D warnings`, build, whole-workspace test incl. DST, `cargo deny`, conformance, prose
  gates). This is also the target's commit-hook surface, so the patch is commit-ready.
- `./engine/scripts/run-verify.sh` (C4-verify) → **PASS** red→green, as quoted in §3(a).
- `scripts/mutants-in-diff` (C5) → **21 mutants tested: 11 caught, 10 unviable, 0 missed.**
- `cargo test -p wyrd-custodian` → all suites green, including `tests/rebalance.rs`
  **unmodified** (the brief's constraint), which still pins `Satisfied` over a malformed flat
  placement at `:1455-1496`.

## 5. Scratch

`$PDCA_SCRATCH` only: `pdca-builder-696-rebalance.rs.bak` (ablation backup) and
`pdca-builder-696-ci.log`; both removed at the end of the run. Nothing was written to `/tmp`.

## 6. Nothing declared NEEDS-HUMAN

No external dependency beyond the base Rust toolchain was needed: the pass runs over the
`traits`/`core` seams with in-memory doubles, headless, no Docker, no new dev-dependency, no
`Cargo.toml` change. The five registered `[[doctor.checks]]` tools were present. Two items
remain the human's at sign-off (both flagged in the previous sign-off, unchanged by this
round): whether leg 5 being base-red is acceptable (C1 Spec), and fitness-to-purpose.
