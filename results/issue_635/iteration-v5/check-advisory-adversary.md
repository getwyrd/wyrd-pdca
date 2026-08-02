# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Method: re-ran the asserted red→green myself in throwaway scratch copies (now deleted), then
attacked the fix with three probe tests written against the **production** API in a scratch
copy of the patched tree. Two probes reproduced concrete failing cases; a third pinned a
blast-radius escalation. Everything below is grounded on the target source at `$PDCA_TARGET`.

## Evidence re-run (what I could confirm)

- **C4-verify's claim reproduces.** Base tree (`git archive HEAD`) + only the added test file:
  `cargo test -p wyrd-custodian --test segmented_map_consumers` → **8 failed / 0 passed**, and
  every failure is an *assertion / unwrap*, not a build error (`invalid type: map, expected a
  sequence at line 1 column 23`), i.e. the brief's "RED is an assertion" requirement holds and
  the `run-verify.sh` PASS-over-a-build-failure hazard did **not** bite. Patched tree → **8
  passed**. Full `cargo test --workspace` on a clean copy is green apart from
  `xtask/tests/repo_hygiene_guards.rs:129`, which shells out to `git ls-files` and fails only
  because my scratch copy has no `.git` — an artefact of my sandbox, not of the patch.
- **C5 was not re-run** (`cargo mutants --in-diff` is available but an 8-minute campaign was not
  worth the budget); I hand-checked five high-value mutants instead (drop the final
  `root_still_names` check, invert `retired_or_fail`, drop the contiguity check, relax
  `check_fenced`'s `expected.is_some()`, drop backfill's `to_fill.is_empty()` guard) and each is
  killed by a named test.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2939` (`SegmentedPublication::publish`)
  commits the whole segment phase before it ever *builds* the flip batch**, so a flip the
  committer itself refuses deterministically and with zero I/O is discovered only after the
  `seg:` records are durable *and* the caller's cursor has moved. Probed twice against
  `RedbMetadataStore::in_memory()`: (a) a flip contribution carrying a 100 001-byte value →
  `publish()` = `Err(ValueOverCeiling)` with **1 durable `seg:` row**; (b) a flip with no fence →
  `Err(Unfenced)` with **1 durable `seg:` row and the session record left at
  `Completing@7|written=1`**. This contradicts the patch's own stated rationale at
  `crates/core/src/metadata.rs:3022-3025` ("a publication that emitted it would fail at commit …
  *after its segments were already durable*") — the per-record ceiling check exists precisely to
  avoid this, and `publish()` reintroduces it for six error variants (`Unfenced`,
  `ContributionCollides`, `ValueOverCeiling`, `KeyOverCeiling`, `BatchOverBudget`,
  `BatchOverOps`). Fix is one line: evaluate `self.flip_batch()?` **before** `write_segments`.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2698` (`staged_batches`) trusts
  `resume_from` against a durable prefix it never reads, and a violation publishes a
  permanently unreadable object.** Probe: attempt 1 writes 20 segments for chunk list A and
  "crashes"; attempt 2 uses the **same** `nonce:epoch`, `resume_from = 20`, and a chunk list
  differing only in `chunks[0].len` (8 → 16). `segment_batches()` returns **0 batches** (the
  cursor is at the plan end), `root()` is built from list B, the flip commits **`Committed`** —
  and every subsequent resolve fails with `SegmentBoundsMismatch { index: 0, root: (0, 328),
  segment: (0, 16) }`. The doc at `crates/core/src/metadata.rs:2527-2531` asserts the prefix "is
  re-derived identically" but nothing checks it, and the failure is silent at publication and
  terminal at read. A one-`get` defence is available: compare
  `store.get(seg_key(group, resume_from - 1))` with `encode(planned[resume_from - 1].1)` and
  refuse with a typed error before the flip. (If the team prefers to declare this #636's
  contract, that is a legitimate recorded rejection — but it should be recorded, not implicit.)
- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:3272` (`high_water_marks`) turns one
  damaged segmented object into a store-wide outage.** The patch moves a **store-state-dependent**
  resolution into whole-store scans that previously could only fail on an undecodable *value*.
  Probe: a store holding one healthy flat object and one segmented object whose
  `seg:<nonce>:<epoch>:000001` is missing while its root still names the group →
  `high_water_marks()` = `Err(segment 1 of live generation … is absent — fail closed)`.
  `Gateway::recover()` is exactly that call (`crates/server/src/lib.rs:123-125`) and the
  composition root runs it "after `new` and before serving", so **the gateway refuses to start**
  and every flat object in the store becomes unavailable. The same `?` shape sits in
  `crates/custodian/src/gc.rs:262`, `restore.rs:381`, `rebalance.rs:165`,
  `reconstruction.rs:614`, `backfill.rs:109`, so GC, scrub, reconstruction, rebalance and restore
  all stop store-wide too. Per-object fail-closed is right and is what 0016 asks for; making it
  abort a whole-store scan is a *different* decision, and this repo already has the containment
  precedent (`desired_state::reconciliation_status`'s `PendingMalformed` — refuse to certify, but
  attribute and keep going). Needs a human call on blast-radius per consumer (GC must fail
  closed; startup recovery arguably must not).
- **NEEDS-HUMAN [human] — stack-base evidence is unverifiable here, and the added double still
  lacks `scan_page` (`crates/custodian/tests/segmented_map_consumers.rs:82-114`).** The brief made
  `$PDCA_VERIFY_BASE = origin/pdca-integration/main` (#634 in wave 0) a gate-evaluability
  precondition and told Do that `MemMeta` "must implement `scan_page` … one delegating line".
  `MetadataStore` in this worktree (`crates/traits/src/lib.rs:767-777`) has **no** `scan_page`,
  `origin/pdca-integration/main` does not exist in this checkout, and HEAD is plain `origin/main`
  (`b0cd199`). So the red→green I reproduced is genuine *on this base*, and adding `scan_page`
  now would not compile here — but the iteration-1/-4 carry-forward finding (E0046 on the
  normative stack) is neither reproduced nor closed. **Verdict provisional: toolchain/base
  unavailable in this sandbox (issue #236), not scored as a refutation.**

## Attacked and could not refute

- Leg A's oracles are not vacuous: leg 2's `Pending` has a live mirror (`server 9` → `Satisfied`,
  `:679-686`) and the two objects sit on disjoint halves of the fleet, so `Pending` can only come
  from reading the `seg:` range; a resolver returning `Ok(None)` for segmented maps passes leg 1
  but fails legs 2 and 3. Leg 4 is asserted on fragment presence and again standing alone
  (`:584-640`), not on a `Reconciled` value.
- Every production reader of `.chunk_map` either routes through the resolver or fails closed:
  `unlink` (`metadata.rs:1624`), `commit_chunk_map` (`:1667`), both superseding committers
  (`:1738`, `:1805`) and `repoint_chunk` (`:2376`, `:2409`) all `ok_or`/refuse a segmented shape;
  I found no remaining silent walk in `crates/*/src/**`, and no scan prefix collides with `seg:` /
  `seggrp:`.
- Ordering: `read_segments` (`metadata.rs:2036-2057`) keys a `BTreeMap` on the **parsed** index, so
  a shuffled `scan` cannot reorder the map; the in-test `MemMeta` iterates a `HashMap`, which
  shuffles for free.
- Byte-identity: legacy flat decode→encode is exercised on raw stored bytes end-to-end
  (`metadata.rs:3428`, `:3446`), and the segmented rebalance test proves it for the new shape by
  landing a `require(inode == encode(prior_root))` CAS against hand-written fixture bytes.
- Envelope/ceiling/fence checks are already probed adversarially on the *caller's* contribution
  rather than the committer's own fixture (`metadata.rs:4250-4345`, `:4355`, `:4703`), closing the
  iteration-3/-4 findings I tried to re-raise.
- Non-finding worth one line (no NEEDS-HUMAN): `crates/custodian/src/resolve.rs:34-42` claims
  `LiveMap.record` "is the live one", but the **flat** arm returns immediately with no currency
  re-read, so a stale flat snapshot whose live root has moved yields `restarted: None` and the
  superseded chunk list. I could not build harm from it — backfill/rebalance/reconstruction all
  lose their CAS, and GC only reclaims fragments carrying orphan/expired-lease evidence
  (`gc.rs:171-190`), which a freshly published generation has none of — so this is a doc
  overclaim, not a defect.

## On the verdict

`check-gates.json` is `overall: fail` on **T4** alone (5 blocking findings, artifact path
truncated in the row). Nothing I found contradicts C4-ci or C4-verify; C1/C2/C3/T1–T3/T5 are
`none`, so the only positive claims on the sheet that I could test, I tested.
