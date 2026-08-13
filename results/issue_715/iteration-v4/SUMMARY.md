# Result — issue 715 / multipart-budget-admission

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the key grammar exists (#691, merged; base `origin/main @ 9dbcd72`) but no
  record **values** do. This child lands the **admission ledger**: `Budget` (the profile
  tuple `0016:348` names — `W_ref`, `MAX_PART_CHUNKS`, `MAX_PARTS_PER_SESSION`,
  `MAX_INFLIGHT_PARTS`, `MAX_STAGED_CHUNKS`) with its two pure derivations `U_ref`
  (`0016:1469`) and `MAX_SESSIONS` (`0016:1470`), and `AdmissionRecord` (the `mpuctl`
  singleton `{ count, max_sessions, profile }`, `0016:348`), each validating at decode.
  **RE-AUTHORED 2026-08-09 after `iterate-plan` on the third attempt** — the three archived
  rounds failed on TWO defects that were this brief's, not Do's, and both are removed here:
  **(A) the brief demanded bounds whose bounding constants do not exist on this base.**
  `max_chunkref_bytes × MAX_PART_CHUNKS ≤ V/2 ⇒ 165–381` (`0016:1466`),
  `MAX_PART_CHUNKS ≤ B_ops` (`0016:1466`, `:1475`) and
  `MAX_STAGED_CHUNKS ≤ MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` (`0016:1468`) rest on
  `max_chunkref_bytes`, `B_ops` and `MAX_SEG_CHUNKS` — **none of which has a code definition
  on `9dbcd72`**, and all three are other slices' (#508 / #625 / #655). So Do had to invent
  them, and did: `chunkref_bytes()`, `WIDEST_SCHEME_BYTES = 33`, `MAX_SEG_CHUNKS_FORMAT_MAX`,
  `MAX_PUBLISHABLE_CHUNKS` (`iteration-v3/patch.diff:164-252`). Every round the reviewers
  correctly refuted the inventions — the JSON-bracket off-by-one, the 156-chunk pin against
  the proposal's own 165–381, and the decisive one: `MAX_PUBLISHABLE_CHUNKS` drags
  **`MAX_ROOT_SEGMENTS` into decode**, which that constant's own doc on the base forbids
  ("a **capacity** guard … deliberately **not** at decode: rejecting a stored record on a
  derived capacity constant would turn a durable object unreadable if the constant ever
  moved (ADR-0045's liberal-on-read boundary)", `crates/core/src/metadata.rs:302-322`).
  That apparatus is also the whole 561-vs-550-line overrun. It is **out of scope here.**
  **(B) the brief demanded an `encode_record`/`decode_record` "envelope later children extend
  with their own arms"** — the v3 T2 flat FAIL. That mechanism has no counterpart on the base
  and no basis in `0016`'s wire shape (a value carries no type tag, so an arm has nothing to
  dispatch on). The base's actual pattern is three parts and all three already exist; see
  **Citations expected**. **No `encode_record` is added by this slice.**
  **This is an adjudication, not a silent deletion (plan-review finding, 2026-08-09) — the
  contrary evidence and why it does not carry:** the merged module doc DOES name
  `encode_record`/`decode_record` as this child's (`crates/core/src/multipart.rs:8-12`), and
  #715's own tracker body repeats it (`notes.json`). Both are the SAME forward-looking
  sentence, written at #691's Plan by this chain's own planner before any of it was built —
  a plan, not a normative source, and this brief supersedes it (Scope corrects the sentence
  in the same file). The normative sources say otherwise: `0016` §1's record table
  (`:333-356`) gives every value a **key-determined** shape with no type tag, and ADR-0045
  puts validation inside decode per type. And v3's T2 FAIL is not counter-evidence — read it
  (`iteration-v3/check-review.md`, T2 row): "The **required** extensible codec envelope is
  absent" is required *by the brief the reviewer was handed*, which is the artifact being
  replaced here. Nothing in the target requires it. If a maintainer disagrees, the place to
  say so is sign-off — but Do must not build one on its own initiative.
  **This is a PLANNER RE-SCOPE, and it is PRE-DECLARED as a §6 sign-off decision rather than
  asserted (revision pass, 2026-08-09).** Verified at this revision: `notes.json` carries
  `"comments": []` — there is **no** maintainer comment on #715 either authorizing or
  forbidding the re-scope, so the only tracker text is the planner-authored body itself. The
  human clears or reverses it at sign-off; the reversal remedy is stated below, not left to
  be re-derived. **Effect on the later children, recorded (the reviewer's specific ask):**
  #716 and #717 do **not** wait on an envelope and never did — both briefs already carry the
  same correction, each landing its own record types validating **inside their own
  `Deserialize`** over the base's `metadata::encode`/`decode`
  (`results/issue_716/brief.md:5-13`, `results/issue_717/brief.md:8-14`; #717 adds the
  sharper reason — its two decoders must *see the key*, which a `bytes`-only arm cannot pass).
  So no downstream slice is blocked or silently re-scoped by this decision; the chain is
  already consistent. The module-doc correction this slice makes (`multipart.rs:8-12`) is what
  stops the merged file from advertising a deliverable no child owns. **If sign-off restores
  the envelope**, the right home is a slice *after* #717 — when three record families exist to
  share one — not here, where it would have exactly one arm; nothing in this brief forecloses
  that.
- Success criterion: `AdmissionRecord` round-trips through the base's
  `metadata::encode` / `metadata::decode` (`metadata.rs:1536-1543`) preserving every field,
  and its decode enforces **exactly** the guard set below — no more, no less. One isolating
  negation per guard is demonstrated in `build-notes.md` (drop that single check, run the
  test, paste the failing output, revert).

  **THE TYPED DECODE SURFACE, named here rather than left implicit (revision pass,
  2026-08-09 — plan-review finding that "typed rejection" is not observable through
  `metadata::decode`, which is correct: it returns `anyhow::Result<T>`,
  `metadata.rs:1541-1543`, and the `DeError::custom` funnel stringifies the domain error,
  `metadata.rs:1212-1216`).** This slice therefore lands **two** surfaces over one
  validation, exactly as the base does for `seg:`:
  **(S1)** `impl<'de> Deserialize<'de> for AdmissionRecord` funnels through ONE fallible
  constructor, so a value that decodes cannot be malformed *whichever* surface read it — this
  is the Citations-expected (ii) pattern, and it is what makes
  `metadata::decode::<AdmissionRecord>` fail on a torn record (there the failure is untyped:
  the test may assert only `is_err()`, plus optionally a message substring);
  **(S2)** `pub fn decode_admission_record(value: &[u8]) -> Result<AdmissionRecord,
  RecordError>` in `multipart.rs`, the peer of `decode_segment_record`
  (`metadata.rs:2504-2517`) — **this is the surface the guard assertions are made through**,
  and every guard rejection MUST be pinnable to **one distinct `RecordError` variant**
  (a shared variant carrying a free-text detail is NOT sufficient: the twelve demonstrations
  below require the test to tell G4's rejection from G7's). `pub` because #656–#659 read
  `mpuctl` through it; that is its only reason to be public.
  **One trap, named because the base contains it:** `decode_segment_record` recovers its type
  by `err.downcast::<ChunkMapError>()` after `metadata::decode`, and through a
  `DeError::custom` funnel that downcast **cannot** succeed — the domain error is already a
  `serde_json::Error` by then, so the base falls back to `err.to_string()` and keeps only the
  *record-level* variant typed. Mirror the base's **structure**, not that dead branch, and do
  **not** recover a guard identity by matching on a message string. Reaching the wire struct
  and the fallible constructor without going through the stringifying funnel is Do's to
  arrange; the briefed observable is only that S2 returns the guard's own variant.
  Where this brief says "typed rejection" it means **S2's named variant**, and where it says
  "decodes" it means **both** surfaces return `Ok` with every field preserved.

  **THE SCOPE RULE, which replaces the old leg 1f and is the single thing to hold onto:
  a bound belongs to this slice IFF both of its sides are computable from the record's own
  stored fields plus a constant that EXISTS on `9dbcd72`.** Every constant that survives
  that rule (`MAX_PART_NUMBER`, `SCAN_CAP`) is a *format / seam* constant, never a live
  operator knob — so enforcing them at decode is exactly what `0016:390-402` demands, and
  nothing this slice enforces can make a durable record unreadable when a knob moves.

  **The guard set (each one independently enforced and independently falsified):**
  **(G1, a TOTALITY PRECONDITION rather than a peer guard — see Falsifiability)**
  `max_part_chunks ≥ 1` (`0016:1466`, `> 0`). At `max_part_chunks = 0` the `U_ref` of
  `0016:1469` is `0`, so G8's quotient divides by zero — G1 is what makes the derivation
  total, and it MUST be checked before `U_ref` is computed. **It is therefore the one rule
  that CANNOT be isolated** (no value violates G1 while satisfying G8, because G8 is
  undefined there); the brief's "violates only its own guard" rule is explicitly waived for
  G1 alone, and its demonstration is a different shape (plan-review finding, 2026-08-09);
  **(G2)** `max_inflight_parts ≥ 1` (`0016:1471`, range `[1, …]`) — at `0` no slot can ever
  be reserved, so no part can ever be committed and the session can never progress;
  **(G3)** `max_parts_per_session ≤ MAX_PART_NUMBER` (`crates/core/src/multipart.rs:271-277`
  — the **format** key-space bound already on the base; a larger value names a `part:` record
  no parser could read). Note `0016`'s knob table has **no row for `MAX_PARTS_PER_SESSION`**
  (verified at Plan row-by-row across `0016:1464-1479`), so the format bound is the only one
  there is — do not invent a knob range for it;
  **(G4)** `max_inflight_parts ≤ max_parts_per_session` (`0016:1471` clamp 1, iteration-13
  finding 2 — a session cannot have more parts in flight than it may ever hold);
  **(G5)** `max_inflight_parts × max_part_chunks ≤ SCAN_CAP / 2` (`0016:1471`, the bounding
  invariant "owned `sidx:` per session ≤ `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` ≤
  `SCAN_CAP/2`"; `SCAN_CAP` is a base seam constant, `crates/traits/src/lib.rs:286`,
  documented there as "a **correctness constraint, not a tuning knob**" — which is precisely
  why it passes the scope rule where `B_ops` does not).
  **G5 counts IN-FLIGHT `sidx:` entries only, and the archived brief's leg 1g ("the scan
  bound counts committed staging entries as well") is REJECTED as a category error —
  adjudicated here rather than silently dropped (plan-review finding, 2026-08-09).** The
  target keeps the two quantities apart: the **scan-cardinality** bound is stated over the
  `sidx:` range alone — "owned `sidx:` … `≤ MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS ≤
  SCAN_CAP/2` per session" (`0016:2098`, the same row as `:1471`) — while the **memory**
  charge that adds committed staging to in-flight is `U_ref` itself (`0016:1443-1448`,
  `:1469`). Folding the committed term into a scan bound bounds the wrong resource. The sum
  has **not** left this slice: `U_ref` charges it and **G7** enforces it;
  **(G6)** `max_staged_chunks ≥ max_part_chunks` (`0016:1468`, the lower end — at least one
  maximal part must remain stageable);
  **(G7)** `w_ref ≥ u_ref(profile)` (`0016:1473`, `W_ref` range `[U_ref, deployment RAM]`).
  This is the guard that keeps `max_sessions ≥ 1`: below it the derivation yields a ledger
  that can never admit a session;
  **(G8, the identity — the one this whole record exists for)** `max_sessions ==
  min( ⌊w_ref / u_ref⌋ , SCAN_CAP/2 )` computed from the record's **own** stored `profile`
  (`0016:1470`). Both terms bind: `0016:1470` is explicit that the `SCAN_CAP/2` term is
  **a clamp the implementation applies**, not an operator range check, because `W_ref` is
  sized from host RAM and `U_ref` from the caps, so a legal pairing (large `W_ref`, small
  parts) makes `⌊W_ref/U_ref⌋` exceed `SCAN_CAP` and break the reaper's `scan("mpu:")`.
  `U_ref = min( (max_parts_per_session + max_inflight_parts) × max_part_chunks ,
  max_staged_chunks + 2 × max_inflight_parts × max_part_chunks )`, verbatim `0016:1469`.

  **(P-arith — a BEHAVIOURAL oracle, deliberately not a code shape, and NOT a ninth guard.**
  Reworded twice: 2026-08-09 from a plan-review finding that "remove the checked operation" is
  unfalsifiable against a `u128`-widening implementation and shape-directing; and again at the
  **revision pass, 2026-08-09**, because the previous oracle — "a maximal value in each numeric
  field must produce a typed rejection" — **contradicted the guard set and P2**, as the plan
  review correctly showed with `{ max_part_chunks: 1, max_inflight_parts: 1,
  max_parts_per_session: 1, max_staged_chunks: <field max>, w_ref: 2, max_sessions: 1 }`:
  under exact arithmetic `U_ref = min(2, <field max> + 2) = 2`, so that record satisfies
  G1–G8 and MUST be accepted, while the old wording demanded it be rejected. "Exactly G1–G8,
  no more, no less" is the binding claim; P-arith adds no rule.**)**
  **The oracle, restated as a verdict equality:** for a hand-authored record carrying the
  **maximum value its field's wire type admits** — in each numeric field in turn, and in
  combination — decode's verdict MUST equal the verdict the same eight guards give when
  evaluated in **unbounded (exact) integers**, and it must reach that verdict with **no panic
  and no wrap**. Two required cases, one per side:
  **(P-arith-accept)** the plan review's witness above (maximal `max_staged_chunks`, all else
  minimal) is guard-legal under exact arithmetic — `U_ref`'s second term overflows the field
  type while the `min` makes it irrelevant — so it MUST decode `Ok`, with every field
  preserved. This is the case a naive `msc + 2·mip·mpc` panics on in debug and wraps on in
  release, and it is why the leg exists;
  **(P-arith-reject)** `max_part_chunks` at its field max with `max_inflight_parts =
  max_parts_per_session = MAX_PART_NUMBER` and `max_staged_chunks` at its field max — chosen
  so G1, G2, G3, G4 and G6 all still HOLD and the overflow lands squarely on the arithmetic
  guards. Under exact arithmetic **G5** is violated (the product is astronomically past
  `SCAN_CAP/2 = 524_288`) and **G7** is too (no representable `w_ref` reaches that `U_ref`),
  so decode MUST **reject**, naming one of those two — whichever the implementation's
  evaluation order reaches first, stated in `build-notes.md` together with the exact-arithmetic
  check that it is a genuine violation. What is forbidden is `Ok` via a wrapped product, a
  panic, or a variant naming a guard that exact arithmetic says HOLDS. (A maximal record cannot
  violate exactly one guard — that is arithmetic, not a defect in the leg: kind 1's isolation
  rule is for the guard negations, and P-arith is explicitly not one of them.)
  G5 is where a naive spelling overflows first,
  because the decoder reaches these values *before* it has validated them.
  How exactness is achieved — widening to `u128`, saturating ops (`saturating_add`/`_mul` give
  the right answer here because both surviving uses are inside a `min`/comparison), or the
  division form of G5 — is **Do's**; only the observable is briefed. ADR-0045 names checked
  arithmetic for `InodeRecord` version increments and `PendingEntry` lease timestamps
  specifically (`docs/design/adr/0045-metadata-validation-boundaries.md:73-74`) — cited as the
  precedent for the *posture*, not as a universal rule this brief is extending by fiat.

  **(leg D — docs currency: REVERSED at the revision pass, 2026-08-09. The architecture
  paragraph is OUT of scope and DEFERRED to the first *persisting* slice, #656–#659.)**
  The previous revision put a paragraph of
  `docs/design/architecture/05-building-block-view.md` in scope on the strength of
  `AGENTS.md:154-157` ("a change that adds or alters … a persisted field updates the living
  architecture doc in the same PR"). The plan review is right that this collides head-on with
  the target's own, **more specific and already-merged** statement — in the very file this
  slice edits: "This module … has no writer, no store call and no production consumer (the
  first writers are the store round trips, #656–#659). The living architecture doc describes
  the system **as it is** … and its metadata model … therefore gains these namespaces with the
  slice that first *persists* one — documenting records no code emits would make the living
  doc describe a system that does not exist" (`crates/core/src/multipart.rs:55-64`, verified
  verbatim on `9dbcd72` at this revision). This slice declares a record **shape**; it persists
  nothing (`Production reach` says so, and no writer exists until #656–#659), so the specific
  in-file rule governs and the general one does not fire yet. Keeping the paragraph would also
  have required *editing that merged policy comment out* — a second, unbriefed change — and
  left `Scope` self-contradictory (it both permitted a third file and forbade "any file
  outside" the module and its test).
  **The recurrence risk is handled by PRE-DECLARATION, not by shipping prose:** all three
  archived rounds raised docs currency as a §6 item, so it is pre-declared here as a known
  sign-off line (see `Production reach`), exactly as the UNVERIFIABLE red is. If sign-off
  decides the paragraph must ship in this PR after all, the remedy is a one-paragraph
  follow-up commit on this same branch **plus** the corresponding correction of
  `multipart.rs:55-64` — **not** a re-plan, and not a Do round.
  In-file currency IS in scope: `multipart.rs:55-64` keeps its policy but stops saying the
  module is "the key **grammar** only" (it now also carries the admission record values, still
  with no writer). No `docs/` file changes.

  **Three POSITIVE legs, binding the other way — a decoder that rejects these is as wrong as
  one that accepts a torn record.** All three were absent in every archived round, and their
  absence is why the boundary kept drifting.
  **What P2 and P3 do and do NOT claim — sharpened 2026-08-09 after a plan-review finding
  that they read as "absence of a constant makes a prohibited value valid", and made
  DECODE-ONLY at the revision pass, 2026-08-09, after the follow-up finding that "still
  constructs and decodes" crossed the very boundary the paragraph draws.** They do not.
  **Mechanically decode-only, and this is binding on the test:** P1/P2/P3 are asserted by
  hand-authoring the record's **JSON bytes** and feeding them to `decode_admission_record`
  (S2) and `metadata::decode` (S1). They are **NOT** asserted through any constructor, and
  **this slice ships no configuration-validation constructor at all** — no `Budget::new` that
  blesses an operator's knob choice. If Do gives `Budget` a constructor it is a plain data
  constructor over the same eight record guards, nothing more; the knob-range check
  (`> 0`, the `max_chunkref_bytes` value-ceiling ⇒ 165–381, the `B_ops` clamp; `0016:1458-
  1460` settles the ranges, `:1466` that row) lives at the **configuration boundary**, which
  is #508's to write and #655's to value, and at `UploadPart` per `0016:1466`.
  They are claims about **this decoder at this base**, not about the configuration's validity:
  the profile values they use are ones `0016` would reject as *operator configuration*, and
  nothing here says otherwise — they assert only that **THIS slice's decode does not reject
  them**, because the bound that would is not computable here and its enforcement site per
  `0016` is elsewhere (part commit / `UploadPart`). **Both legs are therefore EXPECTED TO BE
  SUPERSEDED**: when #508 lands `max_chunkref_bytes` and `MAX_SEG_CHUNKS`, the *format*
  maxima derived from them become decode bounds and these two legs must be rewritten by that
  slice. Do MUST say so in a comment on each. **Why this is not solved by declaring #508/#625
  as `Depends on`:** those slices are far downstream of this chain (#715→#716→#717→#693/#655
  all sit ahead of them), so the dependency would deadlock the whole chain, and `0016:1458-
  1463` is explicit that a knob's *range* is settled in the proposal while its *value* is the
  implementing slice's — a value this slice never needs. Considered and rejected at Plan; if
  sign-off disagrees, the remedy is to descope P2/P3 into #655, not to block here.
  **(P1)** an `AdmissionRecord` whose `count` **exceeds** its own `max_sessions` still
  **decodes**. Occupancy above a lowered cap is legitimate live state, not a decode error
  (`0016:390-402`; the same liberal-on-read boundary `metadata.rs:302-322` states for
  `MAX_ROOT_SEGMENTS`). Identity relations bind; occupancy relations do not;
  **(P2)** stored bytes whose `max_staged_chunks` is far above any publishable ceiling still
  **decode**. Full witness, re-checked at this revision so the record trips no *other* guard:
  `{ count: 0, max_sessions: 1, profile: { w_ref: 330, max_part_chunks: 165,
  max_parts_per_session: 1, max_inflight_parts: 1, max_staged_chunks: 10_000_000 } }` —
  `U_ref = min(2×165, 10_000_000 + 330) = 330`, so G1–G8 all hold. `max_staged_chunks`'s upper
  end is `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` (`0016:1468`) — a *deployment-capacity* product,
  enforced at part commit as a `400 EntityTooLarge` per that same row, and `MAX_ROOT_SEGMENTS`
  is explicitly not a decode constant (`metadata.rs:302-322`). **This leg is the direct fix of
  the v3 batch-review blocker at `review-batch.md` line 3 (`multipart.rs:1153`)**;
  **(P3)** stored bytes whose `max_part_chunks` lies outside the proposal's **165–381** window
  still **decode** — test **below** it and **far above** it. Full witnesses, both re-checked
  at this revision against all eight guards (`SCAN_CAP = 1 << 20`,
  `crates/traits/src/lib.rs:286`, so `SCAN_CAP/2 = 524_288`):
  **below** — `{ count: 0, max_sessions: 1, profile: { w_ref: 2, max_part_chunks: 1,
  max_parts_per_session: 1, max_inflight_parts: 1, max_staged_chunks: 1 } }`
  (`U_ref = min(2, 3) = 2`); **above** — `{ count: 0, max_sessions: 1, profile: {
  w_ref: 1_000_000, max_part_chunks: 500_000, max_parts_per_session: 1,
  max_inflight_parts: 1, max_staged_chunks: 500_000 } }` (`U_ref = min(1_000_000,
  1_500_000) = 1_000_000`; G5 holds since `500_000 ≤ 524_288`; G6 holds at equality).
  Note both witnesses are **decoded, not constructed** — and neither is thereby declared a
  legal operator configuration: that window is `max_chunkref_bytes`-derived and #508's, and
  `0016:1466` names `UploadPart` as its enforcement site. **This leg is what makes it
  impossible for a later round to be pushed back into inventing a chunk-ref width.**

  **NOT enforced here — enumerated so a reviewer can check the boundary instead of
  re-deriving it, each with its owner. Do MUST NOT invent a value or a derived constant for
  any of these:** `max_chunkref_bytes` and therefore the `≤ V/2` value-ceiling rule and the
  165–381 window (#508/#655, `0016:1466`); `B_ops` (#625, `0016:1475`; `0016:2907-2909`
  requires a per-backend timing case before it has a value at all); `MAX_SEG_CHUNKS` and
  therefore the `max_staged_chunks` upper end (#508, `0016:1465`/`:1468` — it has **no** code
  definition on the base, only prose); `⌊(E_tx/2) / bytes-per-slot-key⌋` (#625, `0016:1471`
  clamp 3); `count` vs `max_sessions` (occupancy, per P1).
  **Two bounds are omitted as REDUNDANT, not as descoped — a guard that cannot be falsified
  is noise, and shipping one would defeat the one-negation-per-guard requirement below:**
  `max_parts_per_session ≥ 1` is implied by G2 ∧ G4, and `max_inflight_parts ≤
  MAX_SLOT_INDEX + 1` (`multipart.rs:326-332`) is implied by G3 ∧ G4 (`MAX_PART_NUMBER` is
  `999_999` and `MAX_SLOT_INDEX + 1` is `1_000_000`, so G3 ∧ G4 already bind it tighter).

  **One durable-format decision, settled here rather than left to Do:** the wire shape is
  `#[serde(deny_unknown_fields)]`. **Rationale corrected TWICE on 2026-08-09, and the second
  correction is the interesting one — read it before assuming either failure mode.** The
  target has **two different CAS shapes**, verified in code, and which one `mpuctl` gets is
  not yet decided:
  (i) the `inode:` commits precondition on the **re-encoded** prior — `require(key,
  encode(prior))` (`metadata.rs:1766`, `:1891`; ADR-0047:44-51 states the rule). Under this
  shape a decoder that drops an unknown field makes `encode(prior)` differ from the stored
  bytes and turns **every** later CAS into a permanent `Conflict` — wedged, and silently;
  (ii) the `pending:` commits precondition on the **raw bytes they read** — `batch
  .require(key, current).put(key, encode(entry))`, where `current` is the `get` result
  (`metadata.rs:1984`, and `live_lease_guards` at `:2011-2020` pushes the same raw bytes).
  Under this shape the CAS **succeeds** and the `put` silently writes the record back
  **without** the dropped field — durable data loss, no error anywhere.
  `0016:348` says only that `mpuctl` is CASed whole (`require(mpuctl == prior)`) and does not
  say which `prior` it means; that choice belongs to #656–#659. **So both failure modes are
  live, and `deny_unknown_fields` forecloses both** — a loud typed decode error at the one
  place a human can read it, the ADR-0045 posture. **Caveat Do must respect:**
  `0016:475-485` claims `renew_pending`/`live_lease_guards` compare the *re-encoded* prior;
  that is **inaccurate about the current code** and its line references are stale. Trust the
  code, not that paragraph. **This is a forward-compatibility decision with a durable
  consequence, so it is flagged for sign-off rather than buried:** a future additive field to
  `mpuctl` requires a versioned format change, exactly as `0016:390-402` says a format
  maximum does. Wire field names are exactly
  `count`, `max_sessions`, `profile`, and within `profile`: `w_ref`, `max_part_chunks`,
  `max_parts_per_session`, `max_inflight_parts`, `max_staged_chunks` (`0016:348`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: extend `crates/core/src/multipart.rs` with `Budget` (the five-field profile),
  its `U_ref` / `MAX_SESSIONS` derivations, `AdmissionRecord`, their validating `Deserialize`
  (S1) and the typed `pub fn decode_admission_record` (S2, see the Success criterion) —
  plus the typed error variants those rejections need, **one distinct variant per guard**, as
  new variants of the module's existing `RecordError` (`multipart.rs:79-119`, enum at `:82`),
  whose doc sentence widens from "a multipart **key**" to cover a record value. **Three** doc
  corrections in the same file, all now stale: the module header's forward reference to
  `encode_record`/`decode_record` (`multipart.rs:8-12`) — no such function is landing — the
  record-values sentence it sits in, and the "key **grammar** only" phrasing of the
  living-architecture block (`multipart.rs:55-64`), whose *policy* (the doc update belongs to
  the first persisting slice) stands unchanged and is what keeps leg D out. One new test file.
  **TWO files total**, named exhaustively below. / **out of scope:** every other record type
  (#716, #717); the
  `encode_record`/`decode_record` envelope (removed — see Defect (B) and Citations expected);
  every constant listed under "NOT enforced here" and any invented stand-in for one; any
  configuration-validation constructor / knob-range check (see P2–P3); **any
  file outside `crates/core/src/multipart.rs` and the new test** — `metadata.rs` in
  particular is untouched (this slice only *reads* its constants), which is what keeps this
  bundle free of #710/#711's conflict set; the outcome enums, answer table, `Verb`,
  `MultipartEtag`, digests, `sha2` (#693 — no `Cargo.toml` / `Cargo.lock` change); knob
  **values** (#655); store round trips (#656–#659) — including the `mpuctl`
  absent-reads-as-`{ count: 0 }` bootstrap and the `require_absent` / CAS admission batch of
  `0016:348` and `:656`, which need a `MetadataStore` this pure slice must not touch; **every
  `docs/` file without exception** — the architecture paragraph is deferred to #656–#659 (leg
  D, reversed at the revision pass), and ADRs, proposals and specs are untouched (INTEGRATION
  §2: an Accepted ADR is immutable, and `0016` is the normative source this brief reads,
  never edits).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 59 mutants tested in 2m: 1 missed, 51 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #715's validated multipart admission-ledger values, derived budgets, typed decode failures, and boundary tests.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Maintainers must accept the planner's removal of the previously advertised generic record envelope—this fixes the codec scope inherited by later multipart slices (`crates/core/src/multipart.rs:15`). |
| C2 Reproduction (red pre-fix) | PASS | In an unpatched clone, the unconditional integration target's imports fail with 13 missing API/variant errors and zero tests run (`crates/core/tests/multipart_budget_admission.rs:34`). |
| C3 Change | PASS | The implementation enforces the eight stored-field invariants through the two fallible conversions without adding configuration bounds or a capability probe, preserving the decode/operation boundary (`crates/core/src/multipart.rs:1183`, `crates/core/src/multipart.rs:1314`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Humans must accept compile-only criterion absence as sufficient red evidence—zero tests run pre-fix, while the patched target ran 16/16 focused tests and every independently rerunnable CI component passed (`crates/core/tests/multipart_budget_admission.rs:91`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must add a witness where the ceiling arm determines `U_ref`: replacing its addition with multiplication survives all tests, so wrong admission-memory arithmetic is not detected (`crates/core/src/multipart.rs:1046`, `crates/core/tests/multipart_budget_admission.rs:80`). |
| T1 Structure | PASS | The patch is confined to the two briefed files and keeps the new surface pure, with no store call, writer, async path, manifest, or unrelated dependency change (`crates/core/src/multipart.rs:5`, `crates/core/tests/multipart_budget_admission.rs:31`). |
| T2 Shape | PASS | Private validated fields, closed wire structs, distinct rule errors, and the typed per-record decoder make malformed values unrepresentable without duplicating the generic codec (`crates/core/src/multipart.rs:1077`, `crates/core/src/multipart.rs:1299`, `crates/core/src/multipart.rs:1347`). |
| T3 Runtime | N/A | No production runtime path exists in this slice; the first writer/store consumers remain #656–#659 (`crates/core/src/multipart.rs:63`). |
| T4 Contribution | NEEDS-HUMAN | Humans must clear closed/rejected prior art and contribution evidence—the bundle omits `scripts/review-branch`, `scripts/pdca contribcheck`, and archived iterations; the independently queried affected-path PR history found only merged #703. |
| T5 Judgment | NEEDS-HUMAN | Maintainers must choose the closed `deny_unknown_fields` compatibility policy—it prevents silent whole-record CAS damage but requires versioning for any future additive field (`crates/core/src/multipart.rs:1065`, `crates/core/src/multipart.rs:1151`, `crates/core/src/multipart.rs:1299`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether a decode-only record type with no live writer or consumer is useful to land independently before the persistence slices (`crates/core/src/multipart.rs:63`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Maintainers must accept the planner's removal of the previously advertised generic record envelope—this fixes the codec scope inherited by later multipart slices (`crates/core/src/multipart.rs:15`).
- [ ] C4 Verification (red→green) — Humans must accept compile-only criterion absence as sufficient red evidence—zero tests run pre-fix, while the patched target ran 16/16 focused tests and every independently rerunnable CI component passed (`crates/core/tests/multipart_budget_admission.rs:91`).
- [ ] C5 Causal adequacy — Rebuild must add a witness where the ceiling arm determines `U_ref`: replacing its addition with multiplication survives all tests, so wrong admission-memory arithmetic is not detected (`crates/core/src/multipart.rs:1046`, `crates/core/tests/multipart_budget_admission.rs:80`).
- [ ] T4 Contribution — Humans must clear closed/rejected prior art and contribution evidence—the bundle omits `scripts/review-branch`, `scripts/pdca contribcheck`, and archived iterations; the independently queried affected-path PR history found only merged #703.
- [ ] T5 Judgment — Maintainers must choose the closed `deny_unknown_fields` compatibility policy—it prevents silent whole-record CAS damage but requires versioning for any future additive field (`crates/core/src/multipart.rs:1065`, `crates/core/src/multipart.rs:1151`, `crates/core/src/multipart.rs:1299`).
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether a decode-only record type with no live writer or consumer is useful to land independently before the persistence slices (`crates/core/src/multipart.rs:63`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] external dependency: exclusive ownership of the bundle + lane worktree — an orphaned
- [ ] The defect no longer matches the tracker-owned deliverable: `notes.json` says “there is no envelope to encode/decode any” and assigns this child “the shared `encode_record`/`decode_record` envelope that later record children extend,” while the merged target repeats that ownership at `crates/core/src/multipart.rs:8-12`. The brief removes it at `brief.md:25-42` solely by declaring the issue body and merged comment superseded; `notes.json` has no maintainer comment authorizing that re-scope. Restore the deliverable or record an explicit human re-scope and its effect on the later children.
- [ ] The success criterion is internally impossible: `brief.md:43-46` promises decode will enforce exactly G1–G8 “no more, no less,” but P-arith requires `u64::MAX` in *each numeric field* to reject (`brief.md:101-108`) while P1 requires an over-limit `count` to decode (`brief.md:144-147`). Even if “each” meant profile fields only, `{max_part_chunks: 1, max_inflight_parts: 1, max_parts_per_session: 1, max_staged_chunks: u64::MAX, w_ref: 2}` has mathematical `U_ref = min(2, u64::MAX + 2) = 2` and can satisfy every listed guard, yet P-arith still demands rejection despite the deliberate absence of a staged-chunks upper guard (`brief.md:148-169`). Narrow P-arith to inputs that actually violate a named guard, or add and justify the missing guard.
- [ ] P3 crosses the brief's own decode/configuration boundary. The brief says P2/P3 are claims only about “THIS decoder,” not configuration validity (`brief.md:129-143`), then requires `max_part_chunks = 1` and `500_000` to “still constructs and decodes” (`brief.md:154-160`). The target proposal says the valid ranges are already settled (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1458-1460`) and gives `MAX_PART_CHUNKS` the 165–381 value-ceiling range plus the `B_ops` clamp (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1466`). Make the positive leg decode-only (with a separate configuration-construction boundary), or move construction to the slice that owns those missing bounds.
- [ ] “Typed rejection” is not observable through the API named by the criterion. `metadata::decode` returns only `anyhow::Result<T>` (`crates/core/src/metadata.rs:1540-1543`), and the cited validation pattern converts the domain error to a serde error with `DeError::custom` (`crates/core/src/metadata.rs:1212-1216`); the target obtains a typed record-level failure only through a separate wrapper (`crates/core/src/metadata.rs:2504-2517`). The scope adds `RecordError` variants but names no analogous admission-record decoder (`brief.md:267-280`), so a deterministic test of a particular typed rejection from hand-authored bytes is unspecified. Name the typed decode surface and expected variant, or weaken the criterion to deterministic decode failure.
- [ ] The architecture paragraph is a second change that contradicts both tracker scope and the target's load-bearing comment. `notes.json` puts “all `docs/` files” out of scope, and the target says the living metadata model gains these namespaces only with the slice that first persists one because documenting records no code emits would describe a nonexistent system (`crates/core/src/multipart.rs:55-64`). The brief nevertheless adds the paragraph (`brief.md:115-124`) while admitting there is no live writer or production path (`brief.md:321-336`), and its scope simultaneously permits that third docs file and forbids “any file outside” the Rust file and test (`brief.md:274-287`). Either defer the architecture update to #656–#659 as the target comment says, or explicitly re-scope this issue and include correction of that source policy comment.
- [ ] C1 Spec — Resolve Plan scope before landing — this slice adds persisted `mpuctl` fields, but the binding rubric requires a same-PR living-architecture update while the brief excludes docs, so compliance changes scope (`AGENTS.md:154`).
- [ ] The defect states `MAX_SESSIONS = ⌊W_ref/U_ref⌋` (`brief.md:4-5`), but the target's normative formula is `min(⌊W_ref/U_ref⌋, SCAN_CAP/2)` and says the clamp is applied by the implementation (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1470`, reiterated at `:2118-2120`). Leg 1g later relies on that omitted term, so the brief gives two incompatible definitions of the identity relation that `AdmissionRecord` decode must enforce.
- [ ] The scope hides an unresolved policy/calibration prerequisite. `Budget::new` must enforce `MAX_PART_CHUNKS ≤ B_ops` (`brief.md:17-19`), while knob values are explicitly out of scope and `Depends on` is empty (`brief.md:58-60`, `:97`). The target proposal assigns `B_ops` to the backend-calibrated batch knob (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1475`) and requires a per-backend timing case before that value is established (`:2907-2909`). Likewise, the promised `MAX_STAGED_CHUNKS` upper bound needs `MAX_SEG_CHUNKS` (`brief.md:19-21`), but the target has no code definition for it (only a prose reference at `crates/core/src/metadata.rs:538`; the implemented related constant is `MAX_ROOT_SEGMENTS` at `:322`). The planner must either declare the prerequisite/value source or explicitly put those values and their calibration in scope; the stated two-file pure-record change cannot determine these acceptance thresholds as written.
- [ ] The falsifiability mapping cannot prove all of criterion 1f. The criterion requires independent enforcement of the value-size ceiling, the `B_ops` ceiling, the `max_staged_chunks` lower bound, and its upper bound (`brief.md:17-21`; the distinct rules are normative at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1466-1468`), but the demonstrated-red list collapses them into only `1f-lower` and `1f-upper` and directs Do to drop one check (`brief.md:35-37`). One upper case can violate multiple ceilings and stay red when any single guard remains, so the omitted guard is not falsified. Name a separately isolating case/negation for each independent bound.
- [ ] The claimed invariant citation is unresolved: `brief.md:40` cites `docs/principles.md:109` and `:137`, but that path does not exist at the resolved target. The available target authority is ADR-0045's structural-versus-contextual decode boundary (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`), which does not itself state the brief's C-1 wording. Replace the phantom citation with a resolvable source that actually supports the invariant.
- [ ] The tracker/prior-attempt claims cannot be checked from the review inputs: `notes.json` and `sources/` are absent, and the brief's required salvage/review artifacts (`brief.md:7-10`, `:76-89`) are also absent from the resolved target. Thus the assertions that the listed blockers came from the tracker/review and that no load-bearing thread constraint was omitted (`brief.md:90-95`) have no inspectable evidence. Put the relevant thread quote and prior-review evidence into the brief or supply the declared evidence bundle before relying on them.
- [ ] C1 Spec — Decide whether 1f-iii is bounded by the mutable `metadata::MAX_ROOT_SEGMENTS` named in the brief or by a new versioned format maximum—the target calls the former deployment capacity while decode must remain stable, and the choice changes durable-record readability (`crates/core/src/metadata.rs:302`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Humans must accept compile-only criterion absence as sufficient red evidence—zero tests run pre-fix, while the patched target ran 16/16 focused tests and every independently rerunnable CI component passed (`crates/core/tests/multipart_budget_admission.rs:91`).; C5 Causal adequacy — Rebuild must add a witness where the ceiling arm determines `U_ref`: replacing its addition with multiplication survives all tests, so wrong admission-memory arithmetic is not detected (`crates/core/src/multipart.rs:1046`, `crates/core/tests/multipart_budget_admission.rs:80`).; T4 Contribution — Humans must clear closed/rejected prior art and contribution evidence—the bundle omits `scripts/review-branch`, `scripts/pdca contribcheck`, and archived iterations; the independently queried affected-path PR history found only merged #703.. 16 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
