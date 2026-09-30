# Adversarial review — issue #776 (`repoint_chunk` placement-move primitive)

Probes were run on a scratch copy of `$PDCA_TARGET` (patch applied) with
`cargo test -p wyrd-core --lib placement_move`. Line numbers are in the target's
`crates/core/src/metadata.rs`.

## Findings

- NEEDS-HUMAN [impl] — **A live `seg:` row over the full ceiling is rewritten, not reported as corruption.** `crates/core/src/metadata.rs:3237-3243` reads and decodes the segment without the `value.len() > MAX_VALUE_BYTES` check that `read_group_range` applies (`metadata.rs:2888-2895`). Concrete case, run as a probe: seed a live one-segment root whose segment row is 100_100 bytes. The lead chunk is RS(10,4) with placement `[u64::MAX; 14]`. `resolve_chunk_map` on that same generation returns `Err` ("segment 0's record is 100100 bytes, over the 100000-byte value ceiling"). `repoint_chunk(.., vec![0; 14])` returns `Prepared`, and the commit rewrites the row down to 99_834 bytes. The read side calls this row structural corruption, while the write side quietly "repairs" it. This goes against brief item 6 ("never hiding a persistently damaged record") and the rubric's *Protocol input* class (oversize input is never silently accepted). The `# Anomalies` doc at `:3184` also leaves this shape out. It is reachable only when the row changed after the caller's resolve, or when a caller plans without resolving, so severity is moderate. The fix is small: map an over-ceiling `bytes` to `ChunkMapError::SegmentValueOverCeiling` and send it through `retired_or` at `:3273`, as the resolver does. Add the test described here. (This is the same issue as the T4 BUG row, now confirmed with a probe.)

- NEEDS-HUMAN [impl] — **The replacement placement is never checked against the chunk's fragment count, in either arm.** `crates/core/src/metadata.rs:3201` takes `placement: Vec<DServerId>` and writes it straight in at `:3218` (flat) and `:3262` (segmented). Concrete cases, both committed in probes:
  - Flat arm: an RS(2,1) chunk with placement `[1,2,3]`, moved to `vec![9]`. The result is `Committed`, and the stored chunk now fails `placement_is_valid()` and `checked_fragments()`. Every maintenance loop will now flag it as Malformed (`reconstruction.rs:832-834`).
  - Segmented arm: the same chunk moved to `vec![]`. This commits an *empty* placement. That counts as "valid" (pre-M3 identity fallback), so reads now resolve its fragments to D servers 0, 1 and 2, where the fragments are not. The move silently points the chunk at the wrong servers.

  The rubric's hard convention reads "contextual checks (e.g. placement length) are … strict in maintenance paths", and this function *is* the placement maintenance write. Refuse anything other than `placement.len() == prior.fragment_count()` with a typed error before building the batch. (This is the T4 CONVENTION row, confirmed. The peer `commit_chunk_map` doesn't check either, but that is no reason to ship a new write path without the check.)

- NEEDS-HUMAN [impl] — **The overflow guard in `chunk_at` is untested, and the test that claims to cover it passes for a different reason.** At `crates/core/src/metadata.rs:4887-4889` ("Lengths that cannot be summed are no address at all"), `chunk_at(&huge, 0, &c())` returns `None` through the `break` at `:3311`, not through `checked_add(..)?` at `:3317`. Hand mutation: replacing `:3317` with `at = at.saturating_add(chunk.len);` leaves all 13 original `placement_move` tests green. Under that mutant, `chunk_at(&huge, u64::MAX, &c())` returns `Some(2)`, the wrong chunk, which my probe shows. Low impact, because decoded records check their chunk sums. Still, it is a mutant that survives and that `cargo mutants` does not generate. Fix: assert at `byte_offset = u64::MAX`, or correct the comment.

- NEEDS-HUMAN [human] — **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.

## Attempted refutations that did not land

- **The brief's named negations are real, not just asserted.** Hand mutations on the scratch copy:
  - Dropping `&& chunk == prior` (`:3314`, with `prior` kept referenced) turns 4 original tests red: the same-chunk conflict, the zero-length boundary, the flat-arm conflict, and `chunk_at`.
  - Dropping the root pin in both arms (`:3204`) turns 2 red.
  - Dropping only the segmented arm's root pin turns `a_superseded_root_fails_the_batch_or_the_move` red.
  - Dropping the segment-bytes pin turns the sibling/after-read test red.
  - Skipping the `retired_or` arbiter (`:3273`) turns the damaged-segment test red.

  So the tests exercise the real code path and are not tautological.
- **The C5 claim of "0 missed" holds, but it is weak on its own.** Of the 30 mutants, 9 were unviable, and `cargo mutants` does not delete conditions. My hand mutations fill that gap. Only the overflow guard above survived.
- **C4-verify "pass" is green-only, as the brief declared.** The gate log's last lines show `PASS (green-only)`, so there is no hidden red→green claim to attack.
- **Zero-length chunk on a segment boundary.** At most two segments can touch one offset, because `SegmentedMap::new` refuses an empty segment (`:1137`). Equality picks the record, and a miss in the first candidate falls through to the second. I found no wrong-record write.
- **The ceiling boundary.** Both arms go through `flat_value_ceiling_crossed` (`:605`), and the diff has no `MAX_ROOT_VALUE_BYTES` comparison. The segmented test pins both V+1 (refused) and exactly V (admitted), as the human's 2026-08-19 decision requires.
- **Concurrency.** Two moves in different segments of one root both land correctly, since each pins the unchanged root plus its own segment. Two moves in the same segment: the second loses the CAS (compare-and-swap) cleanly. A supersede between prepare and commit loses on the root pin. In each case I could not find a lost or torn write.
