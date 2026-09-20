# Adversarial review — #803 staged protection class (iteration 3)

Verdict: I could not break the production fix. I found one test gap on a path where a mistake would delete data. Everything below was re-run on a scratch copy of `$PDCA_TARGET` (the patched tree, base commit `afa44c5`).

## Findings

- NEEDS-HUMAN [impl] — **No test checks that a store error during the staged build propagates.** `crates/custodian/src/gc.rs:787-788` promises that "a store fault propagates rather than being read as 'this session stages nothing'", and the code does use `?` at `gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`). But no test would notice if that stopped being true. I replaced those `?`s with `.unwrap_or_default()`, so a failed read turns into "no staged records". That is exactly the data-loss shape: one transient backend error on `sidx:<id>:` empties the staged set, and GC reclaims a live upload's fragments that are marked past grace. Results: all 11 tests in `staged_protection.rs` stayed green, all 21 test suites in `wyrd-custodian` stayed green, and all 18 DST tests in `crates/dst/tests/custodian.rs` stayed green. The committed side already guards the same rule (`crates/custodian/tests/segmented_map_consumers.rs:1122`, "a genuine store fault still propagates"). Suggested fix: let the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix. Then, for each of `mpu:`, `sidx:<id>:` and `part:<id>:`, assert that both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and still unmarked. cargo-mutants never generates this mutant because it does not mutate `?`, which is why C5 stayed green.

- The C5-mutants "pass" (`check-gates.json`, row `C5-mutants`: 25 mutants, 6 caught, **19 unviable**) is weak evidence. It rests on only 6 mutants that compiled, against about 400 changed production lines. To fill the gap I ran 10 more hand-written mutants on top of the two above:
  - M1: staged build after the `inode:` scan
  - M2: `part:` read before `sidx:`
  - M3: an `Open`-only session filter
  - M3b: `Open`-only for `sidx:`
  - M4: iteration 2's `unresolvable.extend(staged.unresolvable.clone())`
  - M7: silently skipping a bad `mpu:` key
  - M8: an undecodable owned value treated as a hole instead of a hold
  - M9: filling an empty staged placement with the identity placement (fragment i on server i)
  - M10: dropping the `staged-malformed` branch
  - The M5/M6 error-swallowing pair from the finding above

  The unit tests caught every one except M5/M6. DST property 13 also goes red under M1, M2 and M3. It passes under M3b, which is fine because the brief allows either choice. The iteration-1 carry-forward (test more than `Open` sessions) and the iteration-2 carry-forward (catch M4 via the scrub assertion) are both really closed.

- Informational, low priority: `crates/custodian/src/desired_state.rs:257-258` says `unresolvable-chunk-map` "is the shared action, so one query selects every unreadable-record signal across all the surfaces that read this set". This diff adds a second action, `unresolvable-staged-record` (`gc.rs:1238`, `restore.rs:966`), for a condition that blocks GC across the whole fleet. An alert built on that single documented query will miss a staged blocker. GC still answers `Blocked`, and the restore CLI text names both actions, so nothing is hidden outright. `desired_state.rs` is outside this slice's scope, so this is a note for #664, not a rebuild item.

## Re-running the evidence (holds)

- Green: `cargo test -p wyrd-custodian --test staged_protection` passes 11/11 on the patched tree.
- Red: with `gc.rs` and `restore.rs` reset to `afa44c5` and the new test kept, all 11 fail. Every failure is an assertion panic inside the test file (lines 764, 792, 852, 981, 1025, 1160), not a compile error. This matches `gate-logs/C4-verify.log`.
- The tests exercise the production path: they call the real `reconcile_step` and `reconcile_after_restore` over in-memory doubles, not a copy of the logic. The seeded records pass through the production decoders and re-encode byte-for-byte.
- Both new DST properties pass on the patch, in `gate-logs/C4-ci.log` (lines 3489 and 3499) and in my own run.

## Refutation attempts that failed

- **Read order.** The design requires `sidx:` → `part:` → `inode:` (`0016:782-800`). The code follows it: `staged_fragments` runs at `gc.rs:552`, before the `inode:` scan at `:557`, and within a session `:799` comes before `:802`. Leg C and DST property 13 catch either reversal (M1, M2).
- **Cleanup stall from protecting every session state.** The design orphan-marks and deletes each owned `sidx:` entry in the same batch (`0016:671`). `retire:bytes` marks first and then deletes the records. `mpu:` is deleted last, only once `sidx:` is empty and the retire obligations have drained (`0016:673`). So covering non-`Open` sessions cannot hold bytes forever.
- **Scan caps.**
  - `mpu:` is bounded by `MAX_SESSIONS` (clamped to at most `SCAN_CAP/2`, `crates/core/src/multipart.rs:4624`).
  - Each `sidx:<id>:` range is at most `SCAN_CAP/2` because of the slot key space.
  - `part:<id>:` holds at most 10,000 records (`multipart.rs:4470`), against `SCAN_CAP` = 1,048,576 (`crates/traits/src/lib.rs:286`).
- **Zero-fragment geometry.** A scheme like `rs(0, m)` would make `fragment_count()` 0 and silently place nothing in `add_chunk`. It cannot get there: both decoders reject unsupported schemes (`multipart.rs:2359-2370`, `:3574-3581`).
- **Scrub and drain-status answers.** Both still read only `placed`, `malformed` and `unresolvable` (`scrub.rs:114`, `:205`; `desired_state.rs:191-246`). M4 proves the new scrub assertion catches folding the staged holes into the committed set.
- **Concurrent upload during restore.** The staged set is built once, before the fleet listing, so a part upload that starts mid-restore could get marked. But restore is documented as writers-stopped (`restore.rs:227`, `:267-268`), and GC's own staged build would still keep those fragments. Not a refutation.
- **Not raised.** Drain status ignoring staged fragments (`0016:827`, X65) and whether a held staged record should set `needs_human()` are both #664's (`// deferred: #664` at `restore.rs:326`, and in the brief).
