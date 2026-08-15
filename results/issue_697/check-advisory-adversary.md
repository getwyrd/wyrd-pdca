# Adversarial review — issue #697 (advisory, non-gating)

Re-ran the asserted red→green independently in a scratch clone (source-only copy, private
`CARGO_TARGET_DIR`, removed afterwards), then attacked the fix with six hand-built probe
fixtures and two counterfactual production mutations. Toolchain was fully available
(cargo 1.96.0), so nothing here is provisional for want of tools.

**Evidence re-run (confirmed, with one caveat).** Base `reconstruction.rs` + the new test
compiles (no patch-introduced symbol is named) and goes **5/6 red behaviourally** —
`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` on legs 1–4 and
the injected-fault mismatch on leg 5; leg 6 passes on the base exactly as `brief.md:110-118`
declares. With the patch: 6/6 green, and the whole `wyrd-custodian` package green with
`crates/custodian/tests/reconstruction.rs` unmodified. 60 parallel + 20 serial repeat runs
of the discriminator binary: zero flakes.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:470`:** leg 1
  is the *only* leg whose fixture holds a healthy segmented object that is owed nothing, and it
  asserts nothing about the audit seam — only the outcome, the flat repoint and the queue
  (`:460-474`). Its own doc comment states the rule it is supposed to bind (`:446-447`, "not
  named, not counted"), and `brief.md:188-190` / `:234` pin it ("an unasserted label is a
  finding waiting to happen"). **Demonstrated concrete failing case:** I moved
  `emit_refused(&object_name(&key))` out of the `if reading.refused.insert(..)` guard at
  `crates/custodian/src/reconstruction.rs:537` (out of its `refused.insert` guard at `:536-539`) so the row is emitted for **every** segmented
  object the walk meets, owed or not, while leaving the `refused` set (and therefore the
  `Blocked` answer) exactly as-is. Result: **all six legs stay green, and so does the entire
  `wyrd-custodian` package** — while every store holding one healthy multipart object now emits
  a `refused-segmented` row naming it and ticks `reconstruction_refused_records` on every pass.
  That is precisely answer-rule A's mirror-image defect ("get this wrong and every store holding
  one multipart object is …", `brief.md:187-193`) leaking through the discriminator. Leg 2
  cannot catch it: its fixture holds exactly one segmented object which *is* owed, so
  `(rows, ticks) == (1, 1)` holds either way (`:507-514`). Contrast the sibling label — the
  `unresolvable-chunk-map` counts are pinned to exact values by legs 3 and 5 (`:561-568`,
  `:683-686`) and the same mutation there **is** caught. Fix is one assertion in leg 1:
  `rows(&logged, "refused-segmented") == 0 && rows(&logged, "unresolvable-chunk-map") == 0 &&
  !names(&logged, "inode:1")`.

## Attempted and could not refute

Everything below I tried to break with a built fixture or a mutation and failed:

- **Index aliasing in the shared snapshot** (`crates/custodian/src/reconstruction.rs:511-531`,
  `:866`): `FlatSite::index` enumerates `resolved.chunks`, and `repair_chunk` indexes
  `object.prior.chunk_map.as_flat()`. For a flat record these are provably the same slice —
  `resolve_snapshot` returns `Cow::Borrowed(chunks)` and never restarts
  (`crates/core/src/metadata.rs:2585`), and `FlatObject` is only ever constructed from that same
  iteration's `record` (sole producer, `:521`). No out-of-range or wrong-chunk repoint is
  reachable. Probed with an 8-chunk object where only odd indices are owed, and with a duplicate
  `ChunkId` at indices 0 and 2 — first index wins, matching the base's `position()`.
- **A hole met LAST in key order.** Every shipped leg seeds the damaged record first. I built the
  opposite (`inode:1` under-replicated, `inode:9` undecodable): the pass still answers `Blocked`,
  still lands the healthy repoint, and still drains nothing — the `incomplete` flag is read after
  the whole reading (`:323`, `:331`), so the property is order-independent.
- **Per-object refusal accounting across *two* segmented objects** (no shipped leg covers this):
  two roots under distinct group nonces, one owed chunk each → exactly 2 rows / 2 ticks, both
  obligations kept. Correct.
- **A refusal beside a complete reading:** a refused `seg:` chunk + a genuinely unreferenced
  obligation → the unreferenced one *is* drained, the refused one is kept, answer `Blocked`.
  Sound: a refused object was read successfully, so "no committed map references this chunk"
  is still a conclusion over a complete namespace.
- **Non-committed owner** (`:471`): an obligation whose owner record is `Pending` is drained.
  Byte-for-byte the base's behaviour (`find_chunk` skipped non-`Committed` and returned `None`),
  so not this patch's defect.
- **Unparseable `inode:` key on a flat record** (`:496-503`): `continue` → the obligation drains.
  Also base parity, and frozen to **#698** by `brief.md:266-277`.
- **`Blocked` swallowing `Changed`** (`:331-345`): matches `gc.rs:234-245` and `scrub.rs:210-214`
  exactly, and `least_certified` (`reconciliation.rs:56-61`) already defines the fold. No caller
  loops on `Changed`.
- **Citations the patch introduces** — `gc.rs:234-241`, `rebalance.rs:115-117`,
  `metadata.rs:2585`, `gc.rs:155-166`, `gc.rs:402-416` — all check out on the target, and
  `read_committed`'s containment is byte-for-byte `gc.rs`'s downcast rule.
- **Budget:** 2 files, 143 added semantic production lines (cap 160). The test file is 712 raw
  against the brief's 460 cap, but a human already waived that at 678 with "do not spend the
  round shrinking the file" (`brief.md:442`), so I did not score the further drift.
- **Not re-raised, per the target rubric's *Deferrals are settled* (`AGENTS.md:200-203`):** the
  `Ok(None)`-for-a-key-this-scan-saw-`Committed` silent drain (`:476`) carries an in-code
  `deferred: #702` marker at `:439` and an explicit human "do NOT fix in-slice" at
  `brief.md:454`. The unbounded `scan`/`resolve` awaits (`:458`, `:474`) and the seeded-Tier-0
  DST demand are likewise already recorded-rejected.

## On the two red advisory/gating rows

- **`C5-mutants` (fail, 2 missed) is not a real signal, and I can corroborate that
  independently.** Both survivors are provably **equivalent**: `size: object.prior.size`
  (`crates/custodian/src/reconstruction.rs:868`) and `state: InodeState::Committed` (`:870`) are
  re-supplied unchanged by the `..object.prior.clone()` functional update at `:875` — and
  `object.prior.state` is *always* `Committed` because `read_committed` filters at `:471` and is
  the sole producer of a `FlatObject` (`:521`). No test can kill either. Separately, the **1
  timeout** row (`:471` `!=`→`==`) is infrastructure noise, not an uncaught mutant: I applied it
  by hand and 14 tests in the existing suite plus 5 of the 6 new legs fail in 0.05 s. Three of
  the seven "unviable" entries (`emit_unresolvable`, `emit_refused`, `Reading::contain` → `()`)
  are unviable only because `-D warnings` rejects the resulting unused parameters, not because
  the behaviour is untested.
- **`C4-verify`'s `path_line` reads "(6 test(s) ran red)".** Six tests *ran* in the red leg; five
  went red. Leg 6 is green on the base by design (`brief.md:110-118`). Read as "six legs
  demonstrated red", that row overstates the evidence by one leg — worth not repeating in the
  SUMMARY.
- `T4-batch-review` is the only gating red (2 blocking). Its findings are not in my inputs, so I
  can neither confirm nor rebut them.

Net: one implementation-level oracle gap, demonstrated with a counterfactual the whole suite
misses. I could not refute the production logic, the containment rule, the one-reading property,
the write path, or the red→green itself.
