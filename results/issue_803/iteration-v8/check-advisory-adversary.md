# Adversarial review — issue #803 (staged protection class for GC and restore)

Red→green reproduced in a scratch clone of `$PDCA_TARGET` (cargo 1.96.0 present, so no
toolchain caveat): with `gc.rs` / `restore.rs` / `reconciliation.rs` reverted to the base
commit, `crates/custodian/tests/staged_protection.rs` gives **21 failed / 2 passed** — every
failure an assertion, none a compile error, and the 2 passes are exactly the two guard legs (D
and F). With the patch applied, 23/23 green. The evidence exercises the production
`gc::reconcile` / `restore::reconcile_after_restore` over in-memory doubles, not a parallel
re-implementation, and the DST double (`crates/dst/tests/custodian.rs:2660`) is a recording tap
over the real `SimTikvMetadataStore`, not a hand-rolled store.

I then hand-injected 14 mutants into the patched production code. Eleven were caught —
part-before-sidx read order, staged-read-after-committed in **both** passes, dropping a page of
a per-session range, dropping a page of the `mpu:` listing, skipping `parse_part_key`,
collapsing the owned-decode failure into a fleet-wide block, either of `StagedSet::protection`'s
`held` / `unresolvable` arms, zeroing the fragment index, identity-filling the D server, and
short-circuiting `place()` after one fragment. Three survived; those are the findings below.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:756`: the exact-length rule is **not covered
  for an empty staged placement**, the one length the committed-side classifier
  (`ChunkRef::placement_is_valid`, `crates/core/src/metadata.rs:204`) calls *valid*. Relaxing
  the check to `chunk.placement.is_empty() || chunk.placement.len() == usize::from(expected)` —
  i.e. drifting toward the shared committed gate, which `crates/core/src/metadata.rs:215` tells
  every maintenance loop to use — leaves **all 23 tests green**. The rule the patch's own doc at
  `gc.rs:660-663` makes load-bearing therefore has no red test. Concrete failing case: seed the
  E(ii) harness with `part(&[chunk_ref(held, RS_2_1, &[])])`; under that one-line change the
  post-restore pass marks `FragmentId { chunk, index: 2 }` on server 3 and the next GC pass
  deletes it (verified — the probe fails on the relaxed build, passes on the patch). Fix: add an
  empty-placement leg beside `a_part_with_a_wrong_length_placement_holds_its_chunk`
  (`crates/custodian/tests/staged_protection.rs:1732`) and its `sidx:` twin.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:719` +
  `crates/custodian/tests/staged_protection.rs:740`: the `ErasureCoded` fixture installs a stray
  copy only for the **committed part's** chunk (`let stray = (3, frag(part_chunk, 0));`), never
  for the **owned (`sidx:`)** chunk — although the fixture's own doc at `:693-696` names that
  guard ("a protection that held the chunk whole instead of placing it would keep the stray").
  So nothing distinguishes "the owned entry's placement was expanded" from "the chunk was
  quarantined whole" on the `sidx:` side. Concrete failing case: replace
  `scheme: staged.scheme()` with `EcScheme::None` at `gc.rs:719` — all 23 tests stay green,
  while every healthy multi-fragment owned entry silently becomes a `held` chunk: GC and restore
  emit `action=untrusted-staged-record` for a healthy record on every pass, `gc_untrusted_staged_records`
  ticks against sound data, and every stray copy carrying that chunk id is protected for as long
  as the record lives (a leak the patch elsewhere takes care to close). Adding the twin stray —
  `(3, frag(owned_chunk, 0))`, server 3 being the one `OWNED_PLACEMENT` does not name — catches
  it (verified: green on the patch, red on the mutant). Note this is the same defect family as
  iteration 7's item 2; the fix landed for `part:` only.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:820` vs `crates/server/src/cli.rs:1339-1341`
  and `docs/design/architecture/m4-first-deployment-blueprint.md:611-612`: the reader never
  decodes a session **value** (`for (key, _session) in &sessions`), a deliberate choice
  documented at `gc.rs:803-805` — but the operator-facing text promises the opposite. Both the
  CLI paragraph and the runbook say an unreadable staged record is "an upload session, a
  committed part or an in-flight staging entry **whose key or value** will not parse or decode";
  a torn `mpu:` value is in fact never named, never counted in `RestoreReport::unresolvable`,
  and never alarmed, so an operator who repairs by that description will not find it. The rule
  is also untested in either direction: no leg seeds a session record whose value the decoder
  rejects, so a plausible "harden the reader" edit — decode the session value and `continue` on
  failure — leaves **all 23 tests green** while silently stripping protection from every one of
  that session's staged fragments, which is precisely the data loss this slice exists to prevent
  (verified). Note also that 0016:406 states a non-decoding `mpu:` value makes the reader fail
  closed and alarm; the patch's choice is safer for protection but diverges from that sentence
  without saying so. Fix: narrow the two operator strings to the key, and add a leg pinning that
  a session with an undecodable value still has its ranges walked and its fragments protected.

Attempted and could **not** refute: the source-before-destination ordering at both handoffs
(`gc.rs:258` before `:273`; `restore.rs:340` before `:350`/`:361`) — mutants moving the staged
read after either committed reading are caught by the leg-C tests; the per-session and
session-listing paging loops (`gc.rs:837`, `:856`) — both single-page mutants are caught; the
`part:` key/value split validation (`gc.rs:740`); the `held` vs `unresolvable` split; the
`Reconciled::Blocked` widening at `gc.rs:411`; the read-cost boundary for scrub and drain status
(`reconciliation.rs:143` dispatches `scrub::reconcile` on its own arm and neither `scrub.rs` nor
`desired_state.rs` is touched, so leg F is not a mocked-away guard); `StagedReadFault::source()`
does preserve the chain `wyrd_traits::classify` walks (`crates/traits/src/lib.rs:801`); no other
key lives under `MPU_PREFIX` (`crates/core/src/multipart.rs:1122-1132`, and 0016:350 lists only
`mpu:<upload-id>`); the session record outlives its `part:` records as a tombstone
(0016:962-967), so the "records whose session is no longer listed are not read" caveat at
`gc.rs:797-799` is not a reachable hole; and no stale `committed object(s) …` string survives
outside the new test's negative assertions. The three `STAGED_PAGE` derivation numbers at
`gc.rs:144-152` check out against `MAX_PART_CHUNKS` (158) and `U_REF` (85,952), and the
`const _: () = assert!(…)` at `gc.rs:155` holds them.

Deliberately **not** raised, as already settled: the fleet-wide GC/restore stall on one
unreadable upload record (accepted at the 2026-09-16 re-plan and again at iteration 6); the
`mpuctl` budget-profile preflight (`// deferred: #806`, `gc.rs:810`); restore's mid-pass window
(`// deferred: #805`, `restore.rs:335`); held-record `needs_human` (`// deferred: #664`,
`restore.rs:818`); the tracker-vs-brief title mismatch; and the patch size against the 100 KB
backstop. No architectural or fitness-to-purpose finding of my own — all three items above are
build defects a Do round can close.
