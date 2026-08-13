# Adversarial review — issue #697 (reconstruction reads through the resolver once per pass, contained)

Scope: `patch.diff`, `brief.md`, `check-gates.json`. All `path:line` are the patched target tree at
`$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `339da46`). Toolchain was available; every
claim below was executed, not read off. Throwaway clone `pdca-adversary-697-redleg` removed.

## The evidence — re-run in both directions

Reproduced independently: cloned the target at base `339da46`, dropped in **only** the new test file,
and ran it. **7 of 8 legs fail on the base**, each on a behavioural assertion or a real `Err` from the
base's `find_chunk`; only leg 7 (`an_empty_queue_reads_nothing_and_certifies`) passes, exactly as the
brief pre-declared. With the patched `reconstruction.rs` restored, **8/8 pass**. The legs drive
`wyrd_custodian::reconcile_step` — the production fenced entry — over `MetadataStore` / `ChunkStore`
trait doubles, not a parallel re-implementation, and none names a symbol the patch introduces. The
red→green stands.

## Findings

- **NEEDS-HUMAN [impl] — the new `would-overwrite` guard has no exit in the one window it is armed in;
  I reproduced a permanent stall.** `crates/custodian/src/reconstruction.rs:557-566` runs the probe
  only while `!index.complete()`, and `crates/custodian/src/reconstruction.rs:760` claims "Every no is
  a repair deferred … and it clears when the reading does". It does not: the guard's window is
  *exactly* the window in which `ReferenceSet::protection` returns `incomplete-reference-set` and GC
  withholds **every** fragment in the fleet (`crates/custodian/src/gc.rs:306-316`), so the occupant
  that blocks the landing can never be reclaimed while the block is in force. Concrete failing case,
  executed against the patched tree: (1) one under-replicated flat chunk, complete reading, repair
  dispatched — `put_fragment` lands the rebuilt fragment at the selected target `d2`
  (`reconstruction.rs:689`) and the repoint then **loses the CAS race**, which ADR-0011
  (`docs/design/adr/0011-…:33`) documents as a routine outcome leaving "collectable garbage";
  (2) one committed record elsewhere in the namespace stops decoding — an unbounded condition, since
  there is no operator tooling for it (brief, #694); (3) passes 2, 3, 4 and 5 each return
  `Reconciled::Blocked`, repoint nothing, and leave the obligation queued — because the guard sees
  `d2` occupied by **this loop's own previous rebuild, byte-identical to the fragment it would
  write**. The chunk stays under-replicated indefinitely and is off the
  `reconstruction_under_replicated` backlog gauge (`reconstruction.rs:239`), so the day-one
  "returns to zero" signal reads clean. This is the same "no exit path" defect the round-4 sign-off
  asked to be narrowed away (carry-forward item 1); narrowing it to `!index.complete()` did not remove
  it, it aligned it with GC's blanket withholding. Minimal fix inside the existing budget: have
  `nothing_stands_at` compare the occupant's **bytes** against the fragment the repair would write
  rather than testing presence — an orphan or a stranded self-rebuild of the same `FragmentId` is
  always byte-identical (deterministic `erasure::encode`), while a genuine second claimant under a
  different scheme/len is not.

- **NEEDS-HUMAN [human] — `Reconciled::Blocked`'s public rustdoc is still contradicted, which
  round-4 sign-off item 5 required be resolved or brought back to Plan.**
  `crates/custodian/src/reconciliation.rs:25-28` defines `Blocked` as "at least one committed
  object's chunk map **could not be read** … so the reference set the loop reasoned over is
  incomplete". `crates/custodian/src/reconstruction.rs:335` now also returns it when `refused > 0`
  over a reading with no hole in it — legs 2 and 5 (`segmented_map_reconstruction.rs:394`, `:519`)
  assert `Blocked` in stores where **every** chunk map resolved cleanly. The in-code comment at
  `reconstruction.rs:340-348` acknowledges the widening ("WIDER than GC's and scrub's") but neither
  updates the contract nor carries a `// deferred: #N` marker, and the brief's 2-file budget forbids
  the third file that would fix it. A human must decide: widen the doc (third file), narrow the
  behaviour, or record a tracked deferral. *(For the record: the sibling half of that item — the
  ADR-0011 netting formula — is **not** contradicted. I checked
  `docs/design/adr/0011-…:36-40`: no refusal path increments `reconstruction_repaired`, so
  `repaired − conflict − aborted` is unchanged. The builder's claim at `reconstruction.rs:281-284`
  is warranted.)*

- **NEEDS-HUMAN [impl] — the test file asserts in prose a property no leg tests, and finding 1 shows
  the property is false.** `crates/custodian/tests/segmented_map_reconstruction.rs:11-16` and the
  leg-1 doc at `:346-351` claim leg 1 "pins that a healthy store's repair still lands over a stray,
  so the guard can never become a stall with no exit". Leg 1's store has a **complete** reading
  (`:375` asserts nothing was withheld), so `reconstruction.rs:557` never even enters the probe —
  leg 1 cannot bound a guard that is not armed. No leg drives a **second** pass after a withhold, so
  "it clears" is untested everywhere. Either add a leg that runs a follow-up pass and asserts the
  withheld repair completes, or delete the claim.

- **NEEDS-HUMAN [impl] — `emit_ambiguous` fires once per *claim*, not once per ambiguous id, and its
  own rustdoc says otherwise.** `crates/custodian/src/reconstruction.rs:818-821` emits from inside
  `CommittedIndex::note`, whose comment reads "Reported per ambiguous ID, not per claim on it", and
  `reconstruction.rs:1119-1131` (fn at `:1121`)'s rustdoc says it names "BOTH committed objects". Executed against
  the patched tree: one committed record naming a single queued `ChunkId` **three** times produces
  **two** `ambiguous-chunk-id` rows and two `reconstruction_ambiguous_chunk_id` increments for one
  id, and both rows read `"inode":"inode:42","other":"inode:42"` — the same object named as its own
  counterparty. This is rule D's failure mode (one fact reported per reference instead of per
  object). Leg 5 (`segmented_map_reconstruction.rs:528`) uses `said()`, which returns only the first
  matching row, so it cannot catch it. Fix: emit only on the Vacant→Occupied transition.

- **NEEDS-HUMAN [impl] — two new `Aborted` returns strand rebuilt fragments under a counter whose
  documented meaning excludes them.** `crates/custodian/src/reconstruction.rs:710-714` and `:722-725`
  return `RepairOutcome::Aborted`, which fires `emit_aborted` (`:1208-1221`). Both that rustdoc and
  ADR-0011's table (`docs/design/adr/0011-…:34`) define `reconstruction_aborted` as *"the
  failure-domain selector chose a server outside the fleet view, so nothing was committed"* — and the
  pre-existing abort at `:673-677` returns **before** any write, which is what makes "nothing was
  committed" true. The two new ones return **after** `put_fragment` has already landed every rebuilt
  fragment at `:689`, so an "aborted" repair now sometimes leaves stranded bytes, which the ADR
  attributes only to `conflict`. Adjust the emitter's wording / reason field (in-file, within budget)
  so an operator is not told nothing was written.

- **NEEDS-HUMAN [human] — the recorded rejection of Tier-0 DST coverage rests on a premise this
  patch's own content falsifies.** The brief's Verification posture (`brief.md:262-265`) records the
  standing finding as rejected because "this slice introduces no new destructive or concurrent path:
  … what it adds on the segmented side is a refusal, which writes nothing at all." The patch now adds
  a *decision that gates a destructive write*: `nothing_stands_at` is evaluated in the assessment
  frame (`reconstruction.rs:557-566`) and `put_fragment` lands in the repair frame
  (`reconstruction.rs:689`) — after every other assessment and after `plans.sort_by_key` at `:266` —
  so the guard carries a genuine TOCTOU window between probe and write. I am not re-litigating the
  class (the rubric settles recorded rejections); I am flagging that the *stated reason* no longer
  describes the diff, so the rejection should be re-recorded with an accurate reason or reconsidered.

## Correction to the record — the C5 red is not a signal about this patch's new logic

`check-gates.json:64` reports "42 mutants tested: 2 missed, 23 caught, 17 unviable". I reproduced it
exactly (42 / 2 / 23 / 17). **Both survivors are the same statement:**
`crates/custodian/src/reconstruction.rs:232:61` — `Assessment::Unreachable => unreachable_degraded
+= 1`, replaced with `-=` and with `*=`. That line is unchanged by this patch (it is diff *context*),
and it survives because no test in `crates/custodian/tests/` ever supplies a non-empty `unreachable`
set — every `ReconstructionContext` in `crates/custodian/tests/reconstruction.rs` passes
`unreachable: &[]`. Nothing in the new refusal / withhold / index logic survived. So the brief's
pre-declaration at `brief.md:265-266` — "a survivor here is a real signal about the compressed legs,
not noise" — is **not** borne out, and a reviewer treating C5's red as evidence against the new legs
would be wrong. It is pre-existing coverage debt this patch did not touch.

## Attempted and could not refute

- **The red→green itself** — reproduced in both directions on `339da46` (7/8 red, 8/8 green), through
  the production `reconcile_step`. Not a tautology, not a mirrored copy, not mocked away.
- **Budget conformance** — exactly 2 files; production **219** added semantic lines (cap 230); test
  **380** semantic (cap 380) and **607** raw (cap 620). No third file, no `Cargo.toml` change, no
  docs edit, `crates/custodian/tests/reconstruction.rs` untouched.
- **Probe/write slot agreement** — I checked `select_distinct_domains_excluding`
  (`crates/core/src/placement.rs:265-305`) is pure over an immutable `Topology`, so the slot `assess`
  probes really is the slot `repair_chunk` writes; the `targets[slot]` / `missing` pairing at
  `reconstruction.rs:558-559` matches `repair_chunk`'s at `:671-672`.
- **Rule A** — tried to find a false negative: for a flat record `resolve_chunk_map` answers
  `Cow::Borrowed(record)` (`crates/core/src/metadata.rs:2625-2628`), so the check is only live on the
  restart path, which is precisely where it must be; a restart onto a **value-equal** generation is
  safe by the same argument the comment gives.
- **Rule C / the CAS** — the switch from `metadata::encode(&plan.prior)` to the stored
  `plan.prior_bytes` (`reconstruction.rs:734`) is a genuine strengthening against the rubric's
  *Serialization identity* class, and the fixture's `stored()` helper
  (`segmented_map_reconstruction.rs:168-173`) seeds a non-canonical spelling that would break a
  re-encoding CAS. I could not construct a case where the raw key and the CAS key disagree.
- **Hidden claimants over a complete reading** — I tried to find one the index would miss:
  `wanted.contains(&c.id)` is applied to every `ChunkRef` of every committed object
  (`reconstruction.rs:937-938`), so any second committed reference to a *queued* id is caught.
- **Unparsable `inode:` keys as a mass trigger** — checked that no component writes a sub-namespace
  under `inode:`; `metadata::inode_key` (`crates/core/src/metadata.rs:34-36`) is the sole producer, so
  `reconstruction.rs:890-894` cannot mark a healthy deployment's reading incomplete.
- **C4's recorded flake** — ran `cargo test -p wyrd-custodian` four times on the patched tree; zero
  failures, so I could not attribute the gate's first-run failure to this crate. No finding filed.
