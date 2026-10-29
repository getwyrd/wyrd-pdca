# Adversarial review — #932 (657.1a, `UploadPart` staging core)

**Bottom line:** I could not break the safety properties. Each chunk's owned entry is written before any of its fragments. The session fence, the drain fence and the one-slot-per-attempt rule all hold, and a lapsed lease is never revived. The four findings below are test gaps and contract nits that the builder can fix. None is a safety break.

**How I attacked it.** I copied the patched tree into scratch and ran `crates/core/tests/multipart_upload_part_stage.rs`: 54/54 pass. C4-verify reports UNVERIFIABLE because the test uses API the patch adds (`gate-logs/C4-verify.log`; this was declared in advance). So I made the red legs myself with 23 hand mutations of `crates/core/src/multipart/stage.rs`. 20 of them go red. That includes every brief-listed red I tried:
- the per-server `require_absent` removed (B1 fails);
- planning against the full topology (B1, B2 and all of B3 fail);
- the intent's `require(mpu == Open@E)` removed (E′1, E′2 and two others fail);
- a reserve that probes onward after reading another attempt's id, or after its ambiguous rounds run out (the C legs fail);
- the pinned-reserve re-read on a session move removed (`stage.rs:961`), and the intent's re-read on a session move removed (`:1520`);
- slot-read adoption removed (`:1495`);
- the lapse check run only in round 1, or taken before the drain read, and `<=` changed to `<` (the D legs fail);
- an `Unknown` write counted as written, and an abandoned write reported as "nothing landed" (A′ fails);
- the deadline taken at dispatch (A′ fails);
- an empty piece re-arming the idle deadline (A″ fails through the watchdog);
- the chunk bound off by one (G fails);
- `ChunkIdTaken` ignored;
- `.max(..)` dropped from `next_expiry` (10 tests fail).

The three mutants that survive are all in finding 1.

- NEEDS-HUMAN [impl] — **No test covers a store read that fails while a commit is still in doubt, so C5's "103 caught, 0 missed" overstates the coverage.** Two hand mutants survive all 54 tests:
  - (a) At `crates/core/src/multipart/stage.rs:1479-1482` (the entry read after an intent commit), map a read fault to `StageError::stopped(..)` instead of `doubt.end(..)`. Staging then reports "no batch of this part attempt is in doubt" (`stage.rs:526-527`) while a held intent may still land. That is the rubric's *Transactions* class: a write that may still land, reported as if nothing is pending.
  - (b) At `stage.rs:679`, swap `held.or(unsettled)` to `unsettled.or(held)`. This silently turns `InDoubt.may_still_commit` from `true` into `false`.

  A concrete schedule that catches both:
  1. Intent round 1 answers `Act::Hold`.
  2. Round 2 answers `Act::Unknown { applied: false, may_still_commit: false }`.
  3. The following `get(sidx:…)` faults.

  The correct answer is `InDoubt { may_still_commit: true }`; both mutants answer otherwise. diff-cov already lists these fault arms as never run (`stage.rs:895, 931, 954, 959, 980, 1475`, in `gate-logs/C4-diff-cov.log`). cargo-mutants does not generate swaps like these inside closures, which is why C5 stayed silent.

  Fix: add a read-fault hook to the wrapper (`Then` at `crates/core/tests/multipart_upload_part_stage.rs:552-561` has none), then one reserve leg and one intent leg.

  A smaller gap with the same cause: `bounded`'s post-timer `now_or_never` (`stage.rs:762-764`, diff-cov MISS at `:763`) never runs, and deleting it keeps all 54 tests green. So the module-doc row "answered `Ok` … whatever the clock reads when the answer is seen" (`stage.rs:42`) has no test. Deleting that code would only make staging more cautious.
- NEEDS-HUMAN [impl] — **The error class of an unknown-effect write depends on timing, not on what landed.** In `a_prime_a_publication_straddling_the_deadline_is_never_counted`, both runs end with the same store state: the fragment is stored at a position the owned entry names. I printed `classify(&err)` for each: run 1 = `Indeterminate`, run 2 = `Terminal`.
  - Why: `Stop::WriteEffectUnknown` uses its first report as `source()` (`stage.rs:619-621`). A stop where every unknown write was abandoned has no report, so `classify` falls back to `Terminal` (`crates/traits/src/lib.rs:803`). The seam defines `Terminal` as "retrying cannot help" (`lib.rs:685-689`), and the trait already maps the same fact (`WriteEffect::Unknown`) to `Indeterminate` (`lib.rs:798-799`).
  - The same flaw hits mixed reports. Reports are kept in the order writes answered, so a write that fails with a `TransientFault` and answers first sets the class, even if a later write answered `WriteEffect::Unknown`.
  - The test checks the class only in run 1 (`multipart_upload_part_stage.rs:1765-1771`). `stage.rs:372-375` documents the choice, but 657.2 and #669 will route errors by class.

  Fix: use the most indeterminate report as the source, give an abandoned-only stop an `Indeterminate` source, and assert the class in run 2.
- NEEDS-HUMAN [impl] — **The lease docs claim slot values never repeat, and they do.** `stage.rs:67-69` says "every batch puts a slot value no earlier batch of the attempt put", and `stage.rs:1128-1130` says the same about `next_expiry`.
  - Why it's false: `next_expiry` depends only on the round's clock reading and the last *adopted* slot. So intent retries at one reading put byte-identical slot values. I checked with a probe in `f_the_intent_loop_is_bounded`: its 4 intent rounds put 1 distinct slot value. The reserve's ambiguous rounds also repeat bytes, on purpose (`stage.rs:875-876`).
  - The code already knows this (`stage.rs:1500-1501`) and settles by the entry's bytes, so there is no live bug today.
  - The risk: 657.1b adds renewal on top of these docs. A renewal settle step that trusts the claim and identifies its batch by slot bytes alone would adopt the wrong batch.

  Fix: reword to "later than the slot lease the batch replaces; retry rounds of one batch may repeat a value."
- NEEDS-HUMAN [impl] — **`reserve_slot` skips the request checks that `stage_part` applies, though (H) covers "the functions".** `reserve_slot` is at `stage.rs:839-856`; the checks in `stage_part` are at `stage.rs:1073-1084`. Two concrete cases:
  - `reserve_slot` with `lease_ttl_millis = 1` commits a slot whose lease is `stamp + 1`. That is exactly the request `stage_part` refuses as "would lapse at the next reading" (`stage.rs:1077`).
  - `max_inflight_parts = 0` answers `SlowDown` forever instead of a typed invalid-request error.

  `h_an_invalid_request_writes_nothing` (`multipart_upload_part_stage.rs:3600-3630`) only calls `stage_part`. The stakes are low (no production caller), and the fix is cheap: share one validator.

**Not raised, on purpose:** seeded DST coverage (tracked as deferred in #660 and #665), and the missing absolute landing bound for a write whose publishing step straddles the deadline. The brief already marks that bound for a human decision, and I checked that neither the code nor the docs claim it (`stage.rs:51-53`).

<!-- pdca:leaf-complete -->
