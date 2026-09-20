# Build notes — #803 (662.1) staged protection class for GC and restore — iteration 9

Target: getwyrd/wyrd @ `main` = `78f9859`. Every `path:line` below is on the patched worktree
(`$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt`) unless marked "base". Patch: `patch.diff`
(8 files, +3362/−80); it applies cleanly to base — checked with `git read-tree 78f9859` into a
throwaway index and `git apply --cached --check`. Its one **added** file is
`crates/custodian/tests/staged_protection.rs`, which is what C4-verify needs to earn its red
(`engine/scripts/run-verify.sh:141-144`); I ran the script's own `_added_files` awk rule over the
patch and it reports exactly that one path.

## What this iteration changed

The iteration-8 sign-off scoped this round to three verified implementation gaps and said the rest
of §6 stays open for the human. I started from the iteration-8 patch (`iteration-v8/patch.diff`,
which applies cleanly to base) and touched **three files**: the new test file,
`crates/server/src/cli.rs` and `docs/design/architecture/m4-first-deployment-blueprint.md`.

Everything else is byte-identical to iteration 8. `crates/custodian/src/gc.rs` still hashes to
`9c2db3b` (`git hash-object`), the same post-image iteration 8 recorded; `restore.rs`,
`reconciliation.rs`, `crates/dst/tests/custodian.rs` and `06-runtime-view.md` are unchanged in the
diffstat against `iteration-v8/patch.diff` (`git apply --stat` on both patches differs on the three
files above and nothing else).

For each item I broke production the way the finding describes and ran the mutant through the
project's own gate, so the claim "the new leg catches it" is a recorded run, not an argument.

---

### Item 1 — the empty staged placement was outside the exact-length rule

**The gap was real.** `StagedSet::place` (`crates/custodian/src/gc.rs:754-773`) holds a chunk
whose placement is not exactly its scheme's fragment count. The committed-side classifier
(`ChunkRef::placement_is_valid`, `crates/core/src/metadata.rs:204-206`) is deliberately *more*
liberal: `placement.is_empty() || len == fragment_count()`, because an empty committed placement
means a pre-M3 record and resolves through the identity fallback. Nothing in the iteration-8 test
file distinguished the two rules, so relaxing the staged rule to the committed one — a one-token
edit, and the obvious "make it consistent" refactor a later reader would reach for — left the whole
file green.

That edit is a live data-loss path here: a staged record is born with a full placement
(`0016:828`), so an empty one is damage; identity-filling it protects fragment `i` on server `i`
and leaves every fragment that is actually somewhere else unprotected and reclaimable.

**The fix** — two legs on the existing E(ii) harness, one per staged class:

- `a_part_with_an_empty_placement_holds_its_chunk`
  (`crates/custodian/tests/staged_protection.rs:1867-1886`) — a `part:` record naming an
  `RS(2,1)` chunk with `placement: []`.
- `an_owned_entry_with_an_empty_placement_holds_its_chunk` (`:1888-1900`) — the `sidx:` twin. It is
  not redundant: the two classes reach `place` through different readers (`gc.rs:735-750` for a
  part record, `gc.rs:709-733` for an owned entry), so a mutant in one is invisible to the other's
  leg.

Both run the shared harness `an_untrusted_staged_record_holds_its_chunk` (`:1757-1826`), whose
`held_frags` puts the chunk's fragment 2 on server **3** (`:1779`) — a server neither a truncated
placement nor the identity fallback names. So "held whole" and "identity-filled" give different
answers, which is exactly what makes the leg bite.

**Mutant run.** I applied `if chunk.placement.is_empty() || chunk.placement.len() == …` at
`gc.rs:756`, regenerated the patch into a scratch bundle, and ran `./engine/scripts/run-verify.sh`:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test staged_protection (fix applied)
failures:
    a_part_with_an_empty_placement_holds_its_chunk
    an_owned_entry_with_an_empty_placement_holds_its_chunk
test result: FAILED. 24 passed; 2 failed
```

The two new legs, and only they, catch it.

---

### Item 2 — the leak-detection fixture had no stray for the owned (`sidx:`) chunk

**The gap was real.** `ErasureCoded` (`crates/custodian/tests/staged_protection.rs:683-751`)
seeded a stray copy for the committed part's chunk alone. A stray is the only assertion that can
tell "placed at its recorded servers" from "held whole": a held chunk protects every fragment
carrying its id, the stray included. With no owned-side stray, a bug that held every owned chunk
whole — the M2 shape iteration 8 caught on the part side — survived on the `sidx:` side untouched,
and with it a permanent leak of every stray copy of an in-flight chunk plus a false
`untrusted-staged-record` alarm on every pass for every healthy upload.

**The fix.** `strays` is now two, one per class (`:745-750`): `(3, frag(part_chunk, 0))` and
`(3, frag(owned_chunk, 0))`. Server 3 is named by neither `PART_PLACEMENT` `[2, 0, 1]` nor
`OWNED_PLACEMENT` `[1, 2, 0]` (`:680-681`), so neither copy is a fragment its record places. Leg A
asserts GC reclaims both (`:897-904`); leg B that the post-restore pass marks both (`:949-956`),
that `stranded_marked` is exactly **3** — the control and the two strays (`:957-960`) — and that the
following GC pass deletes both (`:978-984`).

**Mutant run.** I replaced `self.place(key, &planned)` at `gc.rs:723` with a `hold` of the owned
chunk and re-ran the gate:

```
thread 'gc_keeps_every_staged_fragment_in_every_session_state' panicked at …:898:9:
GC kept the stray copy FragmentId { chunk: 2626, index: 0 } on server 3 — no staged placement
names it …
thread 'restore_marks_no_staged_fragment_and_gc_then_keeps_them' panicked at …:950:9:
the post-restore pass left the stray copy FragmentId { chunk: 2882, index: 0 } on server 3
unmarked … RestoreReport { stranded_marked: 2, … }
test result: FAILED. 24 passed; 2 failed
```

`0xA42` = `0xA40 + 2` is leg A's **owned** chunk (`base + 2`), `0xB42` leg B's. The part-side
strays — which existed before this round — did not fire; the new owned-side ones are what caught it.

---

### Item 3 — the operator text promised a check the code does not perform

**The gap was real.** `staged_fragments` reads a session by **key** only: the value is bound to
`_session` and never decoded (`gc.rs:820-836`). That is deliberate and documented at `gc.rs:803-805`
— protection does not depend on the upload's state, and a damaged value still names its records'
key ranges through its key. But the two operator-facing strings said a staged multipart record
lands in the UNREADABLE list when "an upload session, a committed part or an in-flight staging
entry … whose **key or value** will not parse or decode". For a session that is false; for an
`sidx:` entry it is also false, because an undecodable value under a key that still names its chunk
takes the `held` path (`gc.rs:726-731`) and never reaches `unresolvable`. Only the `part:` class is
key-or-value (`gc.rs:740`). An operator repairing a session's *value* off that line would be
repairing bytes no pass reads.

**The fix — the text, narrowed per class.**

- `crates/server/src/cli.rs:1346-1356`: "…a staged multipart record: an upload session (`mpu:`) or
  an in-flight staging entry (`sidx:`) whose KEY will not parse, or a committed part (`part:`)
  whose key will not parse or whose value will not decode". The reason is recorded in the comment
  above it (`:1339-1345`).
- `docs/design/architecture/m4-first-deployment-blueprint.md:609-614`: the runbook's UNREADABLE
  entry says the same, with the explicit "the pass never decodes their values".

The two accurate strings in `restore.rs` (`:170-172` on `RestoreReport::unresolvable`, `:983-985`
on `emit_unresolvable_staged`) already spelled the three classes correctly and are unchanged.

**The fix — the leg that pins it.** `a_session_whose_value_will_not_decode_still_protects_its_records`
(`crates/custodian/tests/staged_protection.rs:1659-1753`) seeds a healthy session, then overwrites
its `mpu:` value with `b"not a session record"`, asserting first that the key still parses and the
value does not (`:1682-1685`) so the damage is in the value alone. It then asserts the behaviour
the narrowed text claims:

- both staged fragments unmarked by the post-restore pass and kept by GC (`:1693-1700`, `:1736-1742`);
- the control still marked and still reclaimed, so the pass did do its job (`:1701-1706`, `:1743-1746`);
- `report.unresolvable` **empty** and the record not on the restore audit seam (`:1707-1716`) — the
  session is not an unreadable record;
- GC answers `Changed`, not `Blocked` (`:1747-1751`);
- and both of the session's key ranges were actually read, off the double's read log
  (`:1718-1731`) — the positive half, so the leg cannot pass by the pass having skipped the
  session entirely.

**Mutant run.** I added a `decode_session_record` check to `staged_fragments` that pushes a failure
into `set.unresolvable` — i.e. made the code do what the old text promised — and re-ran:

```
failures:
    a_session_whose_value_will_not_decode_still_protects_its_records
test result: FAILED. 25 passed; 1 failed
```

Only the new leg moves. Text and code now agree, and a test holds them together.

---

## Red → green (the project's own runner)

Both runs are `./engine/scripts/run-verify.sh` with `PDCA_BUNDLE` pointed at this bundle — the
configured C4-verify gate, which applies `patch.diff` to a clean checkout of the bundle's base,
runs the shipped test green, then reverts the **production** hunks (keeping the test) and runs it
again.

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test staged_protection (fix applied)
test result: ok. 26 passed; 0 failed
run-verify.sh: RED — … (production reverted, test kept)
test result: FAILED. 2 passed; 24 failed
run-verify.sh: PASS — red without the fix, green with it (26 test(s) ran red).
```

**24 of the 26 ran red, every one by assertion** (no compile error, no panic from a missing
symbol — the file names no symbol this slice adds). The three tests added this round are among
them:

| leg | test | red on base |
|---|---|---|
| E(ii) | `a_part_with_an_empty_placement_holds_its_chunk` | **new this round** |
| E(ii) | `an_owned_entry_with_an_empty_placement_holds_its_chunk` | **new this round** |
| E | `a_session_whose_value_will_not_decode_still_protects_its_records` | **new this round** |
| A | `gc_keeps_every_staged_fragment_in_every_session_state` | yes |
| A | `gc_keeps_every_staged_fragment_of_an_upload_whose_records_span_pages` | yes |
| B | `restore_marks_no_staged_fragment_and_gc_then_keeps_them` | yes |
| B | `restore_marks_no_staged_fragment_of_an_upload_whose_records_span_pages` | yes |
| C | `a_part_commit_between_its_two_reads_leaves_the_chunk_protected` | yes |
| C | `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` | yes |
| C | `a_publication_flipped_between_the_reads_and_drained_after_leaves_the_chunk_protected` | yes |
| C | `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_1` | yes |
| C | `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_2` | yes |
| C | `restore_leaves_unmarked_a_chunk_flipped_after_read_1_and_drained_after_read_2` | yes |
| E(i) | `an_undecodable_part_record_withholds_both_passes` | yes |
| E(i) | `a_part_key_the_parser_rejects_withholds_both_passes` | yes |
| E(i) | `an_owned_key_naming_no_chunk_withholds_both_passes` | yes |
| E(i) | `a_session_key_naming_no_upload_withholds_both_passes` | yes |
| E(ii) | `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` | yes |
| E(ii) | `a_part_with_a_wrong_length_placement_holds_its_chunk` | yes |
| E(ii) | `a_part_with_an_over_long_placement_holds_its_chunk` | yes |
| E(ii) | `an_undecodable_owned_value_holds_its_chunk` | yes |
| E(iii) | `a_fault_reading_the_session_listing_fails_both_passes` | yes |
| E(iii) | `a_fault_reading_a_session_owned_range_fails_both_passes` | yes |
| E(iii) | `a_fault_reading_a_session_part_range_fails_both_passes` | yes |
| **D** | `gc_and_restore_never_scan_a_whole_staged_namespace` | **green on base — a guard** |
| **F** | `scrub_and_drain_status_do_not_read_upload_records` | **green on base — a guard** |

D and F are guards by the brief's own design (`brief.md:53-56`, `:72-84`): D pins that no pass
scans a bare `part:`/`sidx:` prefix, and F pins that scrub and drain status keep today's answers —
both hold on base and would go red against the rejected shared-builder design. The gate's summary
line counts all 26 as "ran red"; the honest number is **24 red by assertion, 2 green guards**.

### Leg I, run once against base (`brief.md:111-113`)

C4-verify cannot red leg I: it lives in a *modified* file (`cli.rs`'s own test module), and the
gate earns its red only from an added `*/tests/*.rs`. So I ran it by hand, once, through the
project's gate: I grafted the new `#[test]` onto **base** `cli.rs` (production hunks reverted,
`RestoreReport` derives `Default` on base at `restore.rs:106`, so it compiles) and ran
`./engine/xtask.sh ci`. The failing assertion:

```
---- cli::tests::restore_verdict_names_unreadable_staged_records_as_staged stdout ----
panicked at crates/server/src/cli.rs:3055:17:
the verdict does not say "4 record(s) UNREADABLE": wyrd custodian: post-restore reconciliation
INCOMPLETE — … 4 committed object(s) UNREADABLE (their chunk maps could not be read …)
wyrd custodian: NEEDS-HUMAN — 4 committed object(s) could not be READ: inode:7,
mpu:0123456789abcdef…, part:0123456789abcdef…:000001, sidx:0123456789abcdef…:000001:9. …
test result: FAILED. 41 passed; 1 failed
xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
```

Base calls all four names "committed object(s)", including the `mpu:`, `part:` and `sidx:` keys —
exactly the operator defect leg I exists to catch. `cli.rs` was restored afterwards and the
worktree diff re-checked byte-identical to `patch.diff`.

## Gates run

- `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (fmt, clippy, the workspace suite, the
  `--cfg madsim` DST suite that carries leg G, the statics gate, the dependency wall, and the prose
  gates). Both of the brief's external dependencies are installed on this host and therefore
  actually ran rather than warn-skipping: `typos-cli 1.48.0`, and `python3 -c "import markdown_it,
  yaml"` succeeds. No NEEDS-HUMAN external dependency.
- `./engine/scripts/run-verify.sh` → **PASS** (above).
- Commit-readiness: the repo carries no `.pre-commit-config.yaml`, no `.githooks/` and no
  `core.hooksPath`; `cargo fmt --check` and clippy are legs of `cargo xtask ci`, which is green.

## Alternatives considered and rejected

**Item 1 — relax the staged rule to `placement_is_valid` instead of testing the difference.** This
is the change the mutant makes, and it is wrong, not merely untested: it would silently
identity-fill a damaged staged placement and hand GC a set of servers the bytes are not on. The
committed rule's empty arm exists for records written before the `placement` field did
(`metadata.rs:191-198`); no staged record predates its own placement (`0016:828`). Rejected on
correctness, not cost.

**Item 2 — assert on `StagedSet::placed` directly instead of seeding a stray.** It would need the
test to reach a `pub(crate)` type from an integration test, which the crate does not export — and
it would stop driving the production passes end to end, which is the whole point of this file. The
stray costs 4 lines in the fixture (`:745-750`) and 8 in each of two legs; it is checked through
`reconcile_step` / `reconcile_after_restore` like everything else here.

**Item 3 — make the code match the text instead (decode the session value).** Rejected: that is a
behaviour change outside this slice's scope, and a bad one. A session's value says which *state*
the upload is in, and protection deliberately does not depend on the state (`gc.rs:644-645`,
`0016:770-775`) — covering every listed session only keeps more. Decoding it would turn a damaged
session value into a fleet-wide GC stall for no protective gain. The scope line is explicit that
the class is found "through each session's own bounded ranges", keyed by upload id
(`brief.md:160-166`). So the text moved, and a leg now holds it there.

**Item 3 — just delete the "key or value" clause rather than name the class split.** Cheaper by
about 3 printed words, but it would leave the operator with "a staged multipart record" and no way
to know whether to look at a key or a value — and the answer genuinely differs per class. The
paragraph is the CLI-only operator's whole instruction set mid-restore (`cli.rs:1321-1325`), so the
per-class spelling earns its width.

## Refuting my own test (the three forced questions)

**(a) Genuine red?** Yes, and in two independent senses.
*Against base:* `run-verify.sh` reverts the production hunks and re-runs the shipped test — 24 of
26 fail by assertion (table above), and leg I fails on base with the message quoted above.
*Against the three specific mutants this round exists to catch:* each was applied to production,
run through the same gate, and caught by exactly the new legs and nothing else — the relaxed-length
rule by the two empty-placement legs, the owned-chunk-held-whole bug by the new owned-side stray,
the session-value decode by the new session leg. All three runs are quoted above. Production was
restored after each; `git hash-object crates/custodian/src/gc.rs` is back to `9c2db3b` and the
worktree diff is byte-identical to `patch.diff`.

**(b) Production path?** Yes. Every leg drives the shipped entry points over in-memory doubles of
the `MetadataStore` / `ChunkStore` seams: `wyrd_custodian::reconcile_step` (GC and scrub),
`reconcile_after_restore`, `reconciliation_status`, `mark_orphaned`, `set_lifecycle`. There is no
copy of the protection logic in the test file — `StagedSet` is `pub(crate)` and the test never
names it; it observes through fragments on disk, `orphan:` keys in the store, `Reconciled`,
`RestoreReport` and the real `tracing` audit seam. Leg I calls the real `restore_verdict` from
inside `crates/server/src/cli.rs`'s own test module (it is private to the server lib, so no
integration test can reach it).

**(c) Fixture includes the fault?** Yes — each new leg seeds the failing element rather than
curating it out. The empty-placement legs seed a record whose placement is genuinely `[]` and place
the chunk's fragment 2 on server **3**, which the identity fallback does not name, so the fixture
cannot be satisfied by an identity fill. The owned-side stray is a real extra fragment on a server
`OWNED_PLACEMENT` does not contain, on disk, marked past grace — not excluded from the fleet. The
session leg seeds a genuinely undecodable value (asserted undecodable before the pass runs) under a
key asserted to parse, and checks the double's read log to prove the session's ranges were read
rather than skipped. In every case the mutant runs above show the fixture changes the verdict.

## Still open at sign-off (not this round's scope)

The iteration-8 carry-forward listed five §6 items it deliberately left open — the
fitness-to-purpose tradeoff (one unreadable upload record stalls GC and restore fleet-wide), the
tracker-record vs brief authorization mismatch, leg C(ii)'s publication handoff shape, the
operator-verdict red check, and the key-validation hole — and said the human will address them
after this iteration. I did not fold them in. Two of them do have current evidence in this bundle
that the human may want when weighing them: leg I's base red is recorded above (the
operator-verdict red check), and `a_part_key_the_parser_rejects_withholds_both_passes` covers the
key-validation hole (added in iteration 5, unchanged). The `gc.rs:810` mpuctl budget-profile
finding stays deferred to getwyrd/wyrd#806, recorded in `review-rejected.md` with the
`// deferred: #806` marker at `crates/custodian/src/gc.rs:810`.

## Scratch

Everything throwaway went under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_803`) as `pdca-builder-803-*` and was removed at the end:
`pdca-builder-803-mutant.diff`, `pdca-builder-803-mutantbundle/`, `pdca-builder-803-check.diff`,
`pdca-builder-803-final.diff`, `pdca-builder-803-legi/`, `pdca-builder-803-applycheck/`. No PR was
pushed, opened or marked ready.
