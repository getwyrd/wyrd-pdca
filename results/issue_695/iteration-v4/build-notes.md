# build-notes — #695 backfill reads through the resolver, contained (iteration 4)

*Withheld from the reviewer. Written for the human at sign-off.*

Target branch: `getwyrd/wyrd @ main`, worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0` at
`339da46` (= `origin/main`). Every `path:line` below is that tree **with this patch applied**
unless it says `origin/main`.

---

## 1. What this iteration adds, and only this

Iteration 3's production hunks passed every gate except the batched rubric review, which left
**one** blocking finding (`review-batch.md:3`, restated in the sign-off carry-forward,
`brief.md:227`):

> Removing inode-key validation makes a decodable committed row such as `inode:not-an-id`
> eligible for backfill mutation instead of attributing it as an unaccountable namespace entry.

It is a real regression of iteration 3's own making. `origin/main`'s pass parsed the scanned key
to an `InodeId` (`backfill.rs:64-70`, `:84-86` on `origin/main`) **because it needed the id to
re-derive the CAS key** (`:142`) — the very thing Rule C forbids. Deleting the re-derivation
deleted the parse, and with it the `continue` that had kept the pass off rows it cannot
attribute. So a committed row under `inode:not-an-id` became eligible for a version-bumping
write.

This iteration keeps Rule C and restores the containment — **named and counted, not silent**:

| Piece | Where (this tree) | What |
|---|---|---|
| the predicate | `crates/custodian/src/backfill.rs:73-96` (fn at `:90`) | `names_an_object(key) -> bool` — a **bool**, never a parse that hands the id back |
| the guard | `crates/custodian/src/backfill.rs:155-169` (`if !names_an_object(&key)` at `:166`) | after the committed check, before the resolve: `emit_unaccountable(&mut incomplete, &key); continue;` |
| the attribution | `crates/custodian/src/backfill.rs:389-410` (fn at `:401`) | names the row (`gc::object_name`), counts it into `incomplete`, action `"unparsable-inode-key"`, counter `backfill_unaccountable_rows` |
| the answer | `crates/custodian/src/backfill.rs:98-102`, `:133-136`, `:305-313` | the fn doc, `incomplete`'s comment and the outcome comment now say the third class out loud; `incomplete > 0 ⇒ Blocked` was already there |

Everything else in `patch.diff` is iteration 3's already-reviewed production text, unchanged.

## 2. Why this shape

* **Its own emitter, not folded into `emit_unreadable`.** The file already gives each class of
  hole its own name+count entry point with its own counter and message —
  `emit_unreadable` (`:366-386`), `emit_changed_under_scan` (`:412-428`), `emit_declined`
  (`:431-448`). A fourth class gets a fourth. The repairs genuinely differ: an unreadable record
  is damaged *data* under a key that names an object; this is a row that names no object, which
  an operator resolves in the *namespace*. Folding it in would have cost 1 line less and would
  have made the existing message ("could not read a committed object's chunk map") false for the
  new action — I would have had to rewrite that message, i.e. touch an instrument iteration 3
  already had reviewed and mutation-tested, to save one line.
* **The action string is the merged one.** `core/src/metadata.rs:2045-2047` + `:2158-2173`
  (#652, on the base) already publishes `"unparsable-inode-key"` for *this exact shape in this
  exact namespace* from the gateway's startup-recovery walk. One grep now finds both surfaces.
  Its fixture even uses the finding's own spelling, `b"inode:not-an-id"`
  (`core/src/metadata.rs:3529`).
* **Placed where `origin/main` placed it** — after the `state != Committed` skip
  (`origin/main:81-86`), not before the decode. The pass's population is the *committed* one; a
  pending row was never in it, so containing pending rows would refuse certification over
  something the pass never claimed. A row that both fails to decode and sits under a bad key is
  attributed exactly once (the decode arm wins), the same "one repair obligation per row" rule
  `core/src/metadata.rs:2061-2067` states.
* **A predicate, not a parse.** `names_an_object` returns `bool` on purpose: the id is the thing
  that must not be in scope below, since holding it is how the re-derived CAS key (Rule C's
  defect) gets written in the first place.

## 3. The one judgment call: `inode:007` (round-trip, not `parse` alone)

`names_an_object` requires the key to be **exactly** what `metadata::inode_key(id)` spells
(`backfill.rs:90-96`), not merely to parse. `u64::from_str` also accepts `+3` and `007`, which
the prefix's sole writer (`core/src/metadata.rs:33-36`; grep confirms `format!("inode:{id}")` is
the only producer in the workspace) can never have written. Consequence: a committed row under
`inode:007` is left byte-identical and named, rather than filled in place.

Why strict:

* A row under a spelling the writer cannot produce is a row **no reader can reach** — every
  lookup derives `inode_key(id)`, so nothing will ever ask for `inode:007`. Filling it drains a
  number an operator watches on behalf of a row nobody can read, *and hides it*: the silent skip
  inverted.
* The brief's Rule C names both hazard shapes itself (`brief.md:120-123`: "`inode:007`" and
  "`inode:+3`"), and leg 5 explicitly permits either answer ("filled in place **or** left
  untouched", `brief.md:56-58`).
* AGENTS.md's *Grammar strictness* defect class lists "no `+`/`-` signs via `from_str`"; ADR-0045
  asks contextual checks to be liberal on read and **strict in maintenance paths**. This is a
  maintenance path.

The alternative I rejected — `…is_some_and(|id| id.parse::<InodeId>().is_ok())`, one line
shorter — fills `inode:007` in place under its own key. That is *safe* (Rule C means no
cross-row write), which is why it is a genuine alternative rather than a bug; it just leaves the
unreachable row silently drained. **If you disagree at sign-off it is a one-line flip** (drop
`.and_then(|id| id.parse::<InodeId>().ok())` / `.is_some_and(|id| metadata::inode_key(id) == key)`
back to `.is_some_and(|id| id.parse::<InodeId>().is_ok())`) plus removing the `PADDED` row from
leg 9 — 4 lines in total.

Two consequences I had to handle, both visible in the diff:

* Leg 3's fixture keys were `inode:00` / `inode:01` / `inode:02`, chosen for byte ordering; they
  are not canonical, so under the strict predicate the leg would have measured the key rule
  instead of the damage it exists for. They are now `inode:0` / `inode:1` / `inode:2` — same
  order, canonical (`tests/segmented_map_backfill.rs:293-295`).
* Strictness has to be **bound by a leg**, or it is behaviour no test holds: leg 9 seeds
  `inode:007` beside `inode:-1` (`tests/segmented_map_backfill.rs:483-517`). Under a
  parse-only predicate that row is filled and the leg fails, so the leg pins the choice.

## 4. Alternatives ruled out, with their cost

| Rejected | Concrete cost / reason |
|---|---|
| Fold the case into leg 3 instead of a new leg 9 | −13 semantic lines, but the finding's answer would live inside a leg named `an_unreadable_committed_object_…`; the next review round has to *find* the fix. Paid the 13 lines. |
| Fold into leg 5 (Rule C) | Leg 5 asserts `outcome == Changed` on its filled branch; an unaccountable row makes the pass `Blocked`, so leg 5 would have had to be restructured — it is the leg the brief pins verbatim, and reshaping it to host a new case is how a pinned leg quietly weakens. |
| Validate the key **before** `decode` (mirroring `high_water_marks:2156-2158`) | Same line count, but a *pending* row under a bad key would then block certification forever, over a row this pass never claimed (`origin/main:81-83` has always skipped pending rows). Over-containment; leg 7 exists precisely to keep containment from becoming a blanket refusal. |
| Restore `parse_inode_key` and keep using the id for the CAS key | That *is* the defect Rule C exists for (`brief.md:120-123`); `origin/main:142` reads `inode:007` and CASes `inode:7`. |
| Add `action` as an attribute on the existing `backfill_unreadable_records` counter event (as `core/src/metadata.rs:2074-2077` does) | 1 line, but it changes an instrument iteration 3 already shipped through review and mutation analysis, for a dimension the audit event already carries. Left alone. |

## 5. Budget

| Budget (`brief.md:156-162`) | Shipped |
|---|---|
| exactly 2 files | 2 — `src/backfill.rs`, `tests/segmented_map_backfill.rs` |
| `src/backfill.rs` ≤ 130 added semantic | **112** |
| test ≤ 520 raw (the STOP line) | **517** |
| test ≤ 320 semantic | **336** — 16 over, see below |

The semantic cap was written for **7** legs; the file ships **9**. Leg 8 was added in iteration 1
because the C5 gate reported 4 surviving mutants on the conflict telemetry
(`backfill.rs:209/219/271` as numbered then), and leg 9 is this round's mandated fix. A leg in
this file costs ~20 semantic lines, so the two mandated legs alone are ~40 against a cap that
allowed for none. Iteration 3 measured 323 with 8 legs; leg 9's net cost here is **+13**, because
I gave back 21 lines first: leg 2's three assertion blocks merged into one conjunction
(`tests/segmented_map_backfill.rs:270-281`), leg 3's two merged into one (`:308-321`), leg 3's
fixture assertion put on one line (`:304`), and leg 9 written as a single conjunction
(`:505-516`) rather than the two-block shape I first drafted. Raw — the brief's actual STOP line — is inside the cap with margin.

## 6. Evidence

Every command below was run through the project's own runners (`engine/scripts/run-verify.sh`,
`engine/xtask.sh`, `scripts/mutants-in-diff`), never a hand-rolled invocation.

* **C4-verify (the red→green discriminator): PASS.** `run-verify.sh: PASS — red without the fix,
  green with it (9 test(s) ran red)`. With production reverted, 8 of 9 legs fail; leg 7 is the
  brief's declared non-red over-containment guard (`brief.md:63-66`) and passes both ways by
  design.
* **Targeted refutation of *this iteration's* change** (§7a below): with only
  `names_an_object` neutralised to `true` — i.e. exactly iteration 3's behaviour — the whole
  suite passes **except** leg 9. So leg 9 binds this change, not merely the file's earlier ones.
* **C5 mutation analysis on the bundle diff: 0 missed.** `30 mutants tested: 18 caught, 12
  unviable` (iteration 1 had 4 missed). Of the new code: `replace == with != in names_an_object`
  — the round-trip comparison of §3 — is **caught**, as are both mutants of
  `emit_unaccountable`'s counter. The `-> bool with true/false` mutants of `names_an_object` are
  *unviable* because the workspace compiles with `-D warnings` and the mutant leaves
  `key`/`InodeId` unused — the same reason every pre-existing emitter's `()` mutant is unviable in
  this crate, not a gap this patch introduces. (Behaviourally those two are covered anyway: I ran
  the `true` one by hand, §7a, and it fails leg 9; `false` fails legs 1/3/5/6/8.)
* **Whole-tree gate: `./engine/xtask.sh ci` exit 0** (fmt, clippy `-D warnings`, build, test,
  deny, conformance) — re-run after the final edit.
* **`crates/custodian/tests/backfill.rs` is untouched and green** (5 passed), as the brief
  requires (`brief.md:148-150`); so are `backfill_telemetry.rs`, `gc.rs`, `scrub.rs`,
  `restore_reconcile.rs`, `segmented_map_consumers.rs`, `segmented_map_restore.rs`.
* **Commit-readiness:** `cargo fmt -p wyrd-custodian --check` clean, `cargo clippy -p
  wyrd-custodian --all-targets -- -D warnings` clean. No `Cargo.toml` change, no new dependency,
  no docs change (no doc in the tree catalogues backfill's instruments — grepped for the base's
  own `backfill_placement_remaining` / `backfill_chunks_filled`).

## 7. The three forced questions

**(a) Genuine red?** Yes, twice over.
1. Whole-patch: `run-verify.sh` reverts `src/backfill.rs`, keeps the test, and 8 of 9 legs fail —
   including leg 9, which on `origin/main` sees the pass fill `inode:8`, silently skip both
   unaccountable rows, answer `Changed` and publish `remaining=1` with no attribution at all.
2. This-iteration-only: I replaced the guard's predicate body with `true` (v3's behaviour) and
   re-ran — `8 passed; 1 failed`, the failure being
   `a_row_under_a_key_that_names_no_object_is_named_and_never_mutated`, which then observes the
   `inode:007` row *mutated* (version-bumped, placement filled). Production was restored from the
   scratch copy afterwards and the whole suite re-run green.

**(b) Production path?** Yes. The test drives `wyrd_custodian::backfill::reconcile` through the
public `BackfillContext` seam (`tests/segmented_map_backfill.rs:43`, `:208`); the only doubles are
the `MetadataStore` implementation (a `BTreeMap`) and a `tracing` subscriber that captures the
real audit events the production code emits. No copy, mock or re-implementation of the pass, and
the resolver under it is the real `wyrd_core::metadata::resolve_chunk_map`.

**(c) Fixture includes the fault?** Yes. Leg 9's store *contains* the two rows the fix is about
(`inode:-1`, `inode:007`) and they are seeded **first in key order** over a `BTreeMap`-backed
double, so the walk meets them before the healthy record whose fill proves it continued — the
fault is not curated out, it is met first. Leg 3 does the same for the two damaged shapes and
asserts in-fixture that the seeded root really does fail to resolve
(`tests/segmented_map_backfill.rs:304`), so no leg can
pass because the fault it was built around quietly stopped being one.

## 8. External dependencies

None missing. The five registered `[[doctor.checks]]` ids the brief names were all present; the
slice needs nothing beyond the pinned Rust toolchain (in-memory trait doubles, no Docker, no
backend, no DST leg). No `NEEDS-HUMAN external dependency` to declare.

## 9. Scratch

Everything throwaway lived under `$PDCA_SCRATCH/pdca-builder-695-*` (the refutation backup of
`backfill.rs`, the CI and mutants logs) and is removed; `mutants.out/` was deleted from the
worktree after each run. `git status` in the worktree shows only the two files the patch carries.
