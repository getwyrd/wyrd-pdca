# Adversarial review — #804 (GC reclaim intent + orphan-mark value shapes)

Verdict: **I could not break the core fix.** Recording the intent before the delete is sound in
every interleaving I tried. The red→green evidence holds, including the DST leg (the
simulated-time test), which no gate had checked on the base. What's left is three judgment calls
about contracts and cost. None of them is a failing case in today's tree.

## Evidence (re-checked)

- The unit legs fail by assertion on the base: `gate-logs/C4-verify.log` shows 8 of 9 red, each
  on a real assertion (`gc_reclaim_intent.rs:683,790,845,927,968,1024,1100,1233`). The 9th,
  `a_restore_counts_every_shape_already_marked_and_rewrites_none` (`gc_reclaim_intent.rs:732`),
  is the restore guard the brief designed to pass on the base. So the claim "9 test(s) ran red"
  in `check-gates.json` is off by one. This is a harness wording issue, not a patch defect.
- The DST leg (E) was never run against the base by any gate: C4-verify only runs the new
  `tests/*.rs` file, and C4-ci runs the patched tree. I re-ran it myself. I took a scratch copy,
  restored the base `metadata.rs`, `gc.rs` and `reconciliation.rs`, kept the patched
  `crates/dst/tests/custodian.rs`, and ran `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test
  custodian -- gc_reclaim_intent`. **Both tests fail on the base**, at `custodian.rs:3381`
  (base line numbering): `[Pass, PreMarkRead, Adoption(Committed), DeleteBegan(1, ..),
  Deleted(1, ..), ..]`. The adoption commits after the base GC read the pre-mark, and the base
  GC then deletes the fragment the new placement names. So E is genuinely red→green, and it runs
  the production `reconcile_step` over `SimTikvMetadataStore`.
- The unit legs call the production `reconcile_step` / `reconcile_after_restore`, not a copy.
  B(iii) commits its adoption from inside the double's `delete_fragment`
  (`gc_reclaim_intent.rs:405-420`), which is after GC's intent commit on the patched code and
  before any ledger write on the base. That is the right place to separate the two.

## Findings

- NEEDS-HUMAN [human] — **A stale `reclaiming` mark on a still-referenced fragment turns into a
  delete with no grace wait, once a writer obeys the new "never overwrite `reclaiming`" rule.**
  The codec doc makes the rule absolute: a writer that finds `reclaiming` "must not replace it …
  waits until the key is gone" (`crates/core/src/metadata.rs:126-137`). GC resumes any
  `reclaiming` mark with no grace test (`crates/custodian/src/gc.rs:486-493`), and the safety
  gate is the only thing in front of it (`gc.rs:475-482`).
  Concrete sequence:
  1. GC commits `reclaiming` for P and dies before `delete_fragment`.
  2. An in-tree mover re-places the chunk onto P without an adoption precondition
     (`reconstruction.rs:934`, `rebalance.rs:534`; that precondition only arrives with #663).
     P is now referenced and carries a `reclaiming` mark. GC skips it, correctly, because P is
     referenced.
  3. Later the object is retired by a writer that follows the rule, such as #659's drain. It
     either waits forever (GC never deletes a referenced key), or it drops the reference without
     re-marking. On the next pass GC sees P unreferenced and `reclaiming`, and deletes the bytes
     immediately. A reader still holding the prior version gets no grace.

  Today's dereferencing writers blind-put a fresh legacy mark, so this can't happen in the
  current tree. But the rule is written here for #659 and #663 to inherit. It also contradicts
  0016's cleanup pass, which "re-stamps or drops any mark found on a still-referenced fragment"
  (`0016:1255`). Someone needs to decide whether the rule needs a "referenced position"
  exception. A cheap step either way: GC could name a `reclaiming` mark it finds over a
  referenced fragment on the audit seam, instead of the generic `referenced` skip.
- NEEDS-HUMAN [human] — **One changed mark makes GC commit the whole batch one intent at a
  time.** When the batch commit returns `Conflict`, `record_intents` retries every intent alone
  (`gc.rs:600-613`). A batch of `W` = 1,000 intents with one re-stamped mark costs 1 + 1,000
  sequential commits instead of about 1 + 2·log₂(1000) ≈ 21 with bisection. Under steady
  contention (for example #659's drain re-stamping legacy marks "on contact",
  `0016:1203-1207`, while GC walks the same window), every conflicted batch behaves like the
  one-commit-per-intent mutant that leg C (`gc_reclaim_intent.rs:1076`) was written to kill. It
  is still correct: each lost intent costs only itself. This is cost only, so whether it matters
  before #659 lands is a judgment call.
- NEEDS-HUMAN [human] — **The docs overstate the draining-retirement guarantee.**
  `docs/design/architecture/06-runtime-view.md:78` says "A multipart retirement that is still
  draining never has its fragments reclaimed". The code only protects a fragment whose mark is
  structured and names that retirement (`gc.rs:569-575`, `metadata.rs:145-153`). A fragment of
  a draining retirement that still carries a legacy mark, or one naming an older event, is
  reclaimed on that mark's own grace. That is exactly the gap 0016 closes only with #659's
  migration gate (`0016:1249-1265`). The brief asked for this wording, and v1 was rejected for a
  similar overstatement ("never marked"). The sentence's last clause ("a fragment it marked is
  reclaimed only once …") is accurate. The opening "never" is not.

## What I tried that did not break it

- **Codec** (`metadata.rs:190-240`): leading zeros, `+`/`-`, whitespace, u64 overflow, duplicate
  or reordered fields, `null`, `"reclaiming":false`, JSON `\uXXXX` and `\/` escapes, trailing
  whitespace, and events with space, quote, backslash or non-ASCII. The re-encode check refuses
  every one of them, so `Intent::record`'s re-encoded precondition always equals the stored
  bytes. `retire_token()` goes through the strict `parse_retire_key`, and a non-canonical
  spelling returns `None`. Only `Bytes` mode ever marks (`multipart.rs:1339-1344`).
- **Fault paths**: an `Err` on the intent batch deletes nothing. If the batch actually landed,
  the next pass resumes it. A fallback commit that errors after earlier single commits landed
  and were destroyed still commits their key deletes through `finish_after_fault`
  (`gc.rs:1516-1520`). A cleanup batch whose own commit failed is not retried. That leaves
  `reclaiming` marks over deleted bytes, which is the documented `deferred: #800` case
  (`gc.rs:621-625`). The one untested line is the `emit_cleanup_lost` arm (diff-cov MISS
  `gc.rs:1518`). It is best-effort, so the risk is low.
- **Races**: two concurrent GC passes (the loser's CAS conflicts, and deletes are idempotent);
  an adoption before the read, between the read and the CAS, inside the delete, and after the
  pass (the DST covers all four); an obligation reappearing between the keyed `get` and the CAS
  (tokens are never reused, `multipart.rs:1409-1414`).
- **Protection order**: the referenced and staged gates run before every mark arm, `reclaiming`
  included. An incomplete reference set still withholds everything and answers `Blocked`. No
  other in-tree code reads `orphan:` values (checked all of `crates/*/src`), so GC's new JSON
  writes reach no bare-u64 parser.
- **Leg C** kills the per-intent, one-batch and per-D-server variants. It asserts exactly 2
  recording commits, each ≤ `W`, summing to the population.
