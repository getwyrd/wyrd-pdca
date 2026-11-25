# Adversarial review — issue 1084 (committed-move destination pre-mark)

What I re-ran, on a scratch copy of the patched tree at `$PDCA_TARGET`: the new test file
(48 tests), rebalance's inline evacuation legs (36 tests), and 17 hand-made mutations of the fix,
plus one scratch-only repro test. `cargo` was available, so nothing below is provisional on
tooling.

## Refutations

- NEEDS-HUMAN [human] — **An in-place repair can still be stranded by a gateway DELETE or flat overwrite, which is not the trigger the #1100 deferral names.** The new docs say the only unreference that can mark an in-place position earlier than the pre-mark batch is "a competing move of the same fragment whose adoption was built before that batch" (`crates/custodian/src/gc.rs:263-269`, `crates/custodian/src/premark.rs:37`, `:50-56`, `:60`). They also say a lost repair leaves what it wrote "under its pre-mark ... or on a position the placement still names" (`docs/design/architecture/06-runtime-view.md:40`). That is false for `metadata::unlink`. Unlink writes blind legacy marks stamped with the caller's clock (`crates/core/src/metadata.rs:2186-2189`), and the gateway reads `now_millis()` before unlink's two reads and its commit (`crates/server/src/lib.rs:705`; the overwrite does the same at `:315`). The in-place batch does not touch the root, so an unlink that read before the batch still commits after it. **Concrete failing case** (scratch test modelled on D10b, flat shape, in-place repair on server 5, write held): the DELETE is stamped at batch − 12 s and commits after the batch. GC runs at batch + 29.5 s, finds the position empty and the legacy mark at least `D` = 41 s old, and sweeps it (`gc.rs:913-929`). The write is then released, and the D server accepts it because it is before its deadline at batch + 30 s. Result: the fragment is on server 5, there is no inode and no mark, and the pass answered `Satisfied`. This needs the gateway to stall for more than `D − W_write` = 11 s between reading its clock and committing, and the worst case is bytes kept for ever, never bytes lost: the same class as #1100. T4 raised this twice (`premark.rs:480`). It is a new trigger, not a restatement of #1100, so rejecting it as "restates #1100" would be wrong. Decide whether to widen #1100 to "any unreference whose mark is stamped before the in-place batch and commits after it" and correct the three doc passages above, or to handle it in code.
- NEEDS-HUMAN [impl] — **A mutation of the pre-mark's pins survives every relevant test.** I dropped the repoint's preconditions (the root generation and the `seg:` record) from the pre-mark batch: `.take(0)` on the fold at `crates/custodian/src/premark.rs:461-467`. All of these still pass: `move_destination_marks` (48), `--lib rebalance` (36), `segmented_map_repoint`, `reconstruction`, `segmented_map_reconstruction` and `staged_protection`. The doc claims these pins mean "a move whose record has already moved writes nothing at all" (`premark.rs:447-451`). The brief's Scope also requires the in-place stale-mark delete to run "under its exact bytes and the repoint's pins". I found no safety failure without the pins: the per-position mark pin, the adoption's own root pin, and the drain's exact-bytes re-stamp (`crates/custodian/src/retirement.rs:743-749`) cover every interleaving I tried. So this is a missing test, not a live bug. Fix: add one leg per shape where a supersede lands between the assessment and the pre-mark, and assert that no `put_fragment` is sent (`Fleet::sent_to` is empty) and no pre-mark key is written.

## Claims I tried to refute and could not

- **Red→green evidence holds, and the mutation reds the brief relies on are real.** C4-verify's red leg has 44 of 48 failing. The 4 that pass on the base are the two "no free domain" controls and the two D10a crash legs. Some legs are red on the base only because a setup `expect` fails (D7c and D9 at `crates/custodian/tests/move_destination_marks.rs:1031`; D7a, D7b and D7d at `:1780`, `:1806`, `:1885`). So I checked each property with a targeted mutation, and every one was caught on **both** movers and both shapes:
  - nonce made constant (D8);
  - adoption no longer requires the destination's pre-mark (D5, D8);
  - `W_repoint` gate removed (D7a, D7d);
  - `<` changed to `<=` at `premark.rs:505` (D7d);
  - a deadline refusal treated as landed (D7b, D7c, D10b);
  - writes sent with `None` instead of a deadline, separately in `reconstruction.rs:1329` and `rebalance.rs:694` (D7b, D7c, D9 / the unverified-copy leg);
  - `Stopped` no longer withholding certification (D4, D5, D7, D8, D9, D10b);
  - `lost()` always answering `Conflict` (the F1 legs);
  - the vacated mark written blind, in each of the `Kept`, `Absent` and `Stamped` arms (D4, F2);
  - a pre-mark written at an in-place position (D1, D10a);
  - a stale in-place mark kept (D1, D10a);
  - evacuation ignoring a lost adoption (D4, D5, D6, D8);
  - a stale evacuation pre-mark counted as a move (D7a, D7d).
- **The nonce replays under madsim.** `a_committed_moves_pre_mark_nonce_replays_under_its_seed` passed in the C4-ci DST leg, and it compares the full pre-mark bytes, nonces included, across two fresh-thread runtimes (`crates/dst/tests/custodian.rs:6869-6919`). A source madsim does not seed would make it fail. I did not re-run its madsim mutations. One brief claim is doubtful: madsim virtualises `SystemTime`, so a "wall clock" nonce would probably replay and *not* turn this test red. D8 still catches a stamp-derived identity.
- **The verdict's root check matches the repoint's pin.** `still_current` compares `encode(prior)` (`crates/custodian/src/reconstruction.rs:1377-1380`), the same bytes `repoint_chunk` pins (`crates/core/src/metadata.rs:3523`), so a root encoding mismatch cannot turn an F1 refusal into a certifying `Conflict`.
- **A batch with only preconditions still checks them.** An in-place repair over an absent mark commits a pre-mark batch with no writes. TiKV evaluates every precondition inside the transaction regardless (`crates/metadata-tikv/src/lib.rs:1386-1406`), and redb passes D10.
- **T4's "matching admits servers absent from the fleet" (`premark.rs:386`) cannot happen in the deployed wiring.** The deployed loop builds the fleet and the topology from the same reachable set (`crates/server/src/custodian.rs:304-305`). Only a library caller that passes a topology wider than its fleet reaches it, and that caller gets the base's `unplaced` abort. That is a sound reason to reject the finding.
- **T4's "vacated `reclaiming` mark left in place" (`premark.rs:556`)** argues against the brief's D4 requirement (left byte for byte, ADR-0045), not against the code. It is a scope question for the brief, not a defect in the fix.
- **Red gates that are not evidence against the fix.** C4-diff-cov's failure is a stale-base artifact: the patch targets `main` plus #1013, so it does not apply on `origin/main`. The C5 run's 3 timeouts in `choose`/`assign`/`augment` are hangs, which means the mutants were effectively caught. They were not missed.

<!-- pdca:leaf-complete -->
