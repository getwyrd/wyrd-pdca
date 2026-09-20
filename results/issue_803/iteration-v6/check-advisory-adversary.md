# Adversarial review — #803 staged protection for GC and restore

**Verdict: I tried to refute the production fix and could not.** The staged read order, paging,
fail-closed handling and the restore gate all held up against targeted breakage. What I did find
is two test gaps (a wrong build passes) and one doc sentence that claims more than the code does.

## What I re-ran (scratch copy of `$PDCA_TARGET`, since removed)

- `cargo test -p wyrd-custodian --test staged_protection`: 22/22 green. The C4-verify log shows 20
  red on base, all by assertion; the 2 green are the D and F guards, as the brief expects.
- Leg I against base `restore_verdict` (base `cli.rs`, new test added): red by assertion —
  `the verdict does not say "4 record(s) UNREADABLE"`; base prints `4 committed object(s) UNREADABLE`.
- 14 hand mutations of `gc.rs` / `restore.rs`; **12 caught**: one page per staged range (the
  carry-forward #2 mutant → both paging legs red); one page of the `mpu:` listing; GC reading
  `inode:` before the staged class; `part:` before `sidx:`; restore reading the staged class after
  both committed reads, or between them (the three restore C(ii) tests red); `Blocked` ignoring
  staged unresolvable; an `sidx:` key naming no chunk skipped silently; `part:` key not parsed;
  the read fault not wrapped with its range; restore not naming staged records in the report;
  restore not auditing untrusted records.
- DST leg G (`--cfg madsim`, `MADSIM_TEST_NUM=50`): green on the patch, and **red** on both
  order-breaking builds (inode-first, part-before-sidx), so the property actually bites.

## Attacks that did not break it

- Non-session keys under `mpu:` — `mpuctl` is outside the prefix (`crates/core/src/multipart.rs:1126-1132`).
- A 512-record page overrunning a backend — `page_limit` clamps (`crates/traits/src/lib.rs:414`)
  and TiKV fetches in chunks (`crates/metadata-tikv/src/lib.rs:1275`).
- Another GC delete path skipping the gate — the expired-lease arm sits behind it (`crates/custodian/src/gc.rs:331-337`).
- A staged fault in GC silencing scrub — true inside one combined `reconcile_step`
  (`crates/custodian/src/reconciliation.rs:136-149`), but the production runtime runs each loop in
  its own call with GC last (`crates/server/src/custodian.rs:519`, `:533`, `:610`). Not a break.
- A second restore marking site — there is one, and it is gated (`crates/custodian/src/restore.rs:435-441`).
- Carry-forwards: `// deferred: #806` is present (`gc.rs:810`); items 2 and 3 are covered (mutants above went red).

## Findings

- NEEDS-HUMAN [impl] — **"hold the chunk whole" for a wrong-length staged placement is not pinned.**
  `crates/custodian/tests/staged_protection.rs:1501` places the held chunk's fragments at
  `(0,0) (1,1) (2,2)`, exactly where `ChunkRef::fragments()`'s identity fallback puts them
  (`crates/core/src/metadata.rs:164-169`, `:187-189`). So a build that identity-fills the short
  placement (the alternative `StagedSet`'s doc rejects: "held to the exact length") passes both
  wrong-length legs. Concrete survivor: remove the `held` arm at `crates/custodian/src/gc.rs:693`
  and, in `place` (`gc.rs:754-756`), insert `chunk.fragments()` into `placed` before `hold` →
  `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` and
  `a_part_with_a_wrong_length_placement_holds_its_chunk` stay green (only the undecodable-value leg
  goes red). Fix: put one held fragment on a server the fallback does not name, e.g.
  `(3, frag(held, 2))`. I checked: that turns all three legs red on the mutant and stays green on the patch.

- NEEDS-HUMAN [impl] — **GC's audit name for an unreadable staged record is never checked.**
  Emptying `emit_unresolvable_staged` (`crates/custodian/src/gc.rs:1344`, called at `:263`) leaves
  all 22 tests green: the E(i) harness's GC half (`staged_protection.rs:1378-1420`) asserts only
  `Blocked` and survival. That line is the only place the unattended GC loop names the record
  behind a fleet-wide stall — the stall the human accepted because it can be found and repaired —
  and the committed twin (`action=unresolvable-chunk-map`) is pinned elsewhere
  (`crates/custodian/tests/segmented_map_consumers.rs:369`). Fix: assert
  `named_on_audit_seam(GC_AUDIT, <damaged key>)` in the E(i) harness, as E(ii) already does.

- NEEDS-HUMAN [impl] — **doc sentence overclaims bounded reads.**
  `docs/design/architecture/06-runtime-view.md:78` says both passes read staging entries, committed
  parts "and only then the committed objects, a page at a time and never one listing of a whole
  namespace". The committed objects are one `meta.scan(b"inode:")` (`crates/custodian/src/gc.rs:549`),
  and restore does it twice. Limit the "page at a time" clause to the upload records.

- Gate claim not to lean on (no patch action): the C5 row in `check-gates.json` ("36 mutants: 9
  caught, 27 unviable") reads as mutation evidence, but the workspace sets `warnings = "deny"`
  (`Cargo.toml:227`), so most body-replacement mutants leave an unused parameter and fail to
  compile — 27 of 36 never ran. The hand mutations above cover that gap. Likewise C4-verify's
  "22 test(s) ran red" is 20 red plus 2 green guards (its own log shows this).
