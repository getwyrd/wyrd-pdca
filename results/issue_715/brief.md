- **Slug:** multipart-budget-admission
- **Defect:** the key grammar exists (#691, merged; base `origin/main @ 9dbcd72`) but no
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
- **Success criterion:** `AdmissionRecord` round-trips through the base's
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
- **Falsifiability:** RED is criterion-ABSENCE — **born-at-tier**, as #691 and as both
  archived rounds here. C4-verify classifies `ADDED_TEST
  crates/core/tests/multipart_budget_admission.rs` + `CRATE crates/core` — **not assumed:
  dry-run** with `./engine/scripts/run-verify.sh --classify` on a synthetic patch touching
  exactly the files this slice may touch, which printed those two lines verbatim (the
  added-`*/tests/*.rs` discriminator, `run-verify.sh:248-257`). **Re-run at the revision pass
  (2026-08-09) on the now-TWO-file shape** — `crates/core/src/multipart.rs` modified +
  `crates/core/tests/multipart_budget_admission.rs` added — after leg D took the docs file
  back out of scope: same two lines, unchanged (the docs file never perturbed either, which
  is why removing it cannot). The GREEN
  leg is `cargo test -p wyrd-core --test multipart_budget_admission` (crate name verified,
  `crates/core/Cargo.toml:2`; the test is a plain integration target under no `cfg`/feature
  gate, so it genuinely compiles and runs under that invocation, and `multipart` is
  `pub mod` at `crates/core/src/lib.rs:13`, so every symbol under test is reachable from
  `crates/core/tests/`); the RED leg reverts
  production, the test fails to **compile**, 0 tests run → **UNVERIFIABLE (exit 77)**
  (`run-verify.sh:181-183`, `:486-500`). **EXPECTED and PRE-DECLARED** as a §6 item so it
  lands as a known sign-off line rather than a surprise NEEDS-HUMAN for the fourth time.
  **Demonstrated red Do MUST capture instead (binding) — TWELVE named demonstrations, in
  THREE kinds. The kinds differ deliberately; a uniform "drop the check" rule is what made
  the previous brief's list unhonourable (plan-review finding, 2026-08-09):**
  **(kind 1 — seven ISOLATING negations, G2 G3 G4 G5 G6 G7 G8):** drop that single check, run
  the test, paste the failing output into `build-notes.md`, revert. **Each torn value MUST
  violate ONLY its own guard**, so the red proves that guard is load-bearing rather than
  riding on a neighbour's; a value tripping two guards stays red on the survivor and
  falsifies nothing. Worked isolating witnesses were checked at Plan (e.g. G4:
  `mpps=10, mip=20, mpc=1, msc=100, w_ref=1000, max_sessions=33` satisfies G1,G2,G3,G5,G6,G7
  and G8, and violates only G4).
  **(kind 2 — G1, the totality precondition, which is NOT isolable and must not be faked as
  if it were):** at `max_part_chunks = 0` the derivation is undefined, so no value violates
  G1 while satisfying G8. Demonstrate it as what it is: drop G1, feed the record, and show
  the result is a **division-by-zero panic or an accepted nonsense profile** rather than the
  typed rejection — i.e. show that removing G1 breaks *totality*, not that it flips one
  assertion. Say so in `build-notes.md` in those words.
  **(kind 3 — four INVERTED legs, P1 P2 P3 and P-arith):** these assert something is
  ACCEPTED, so they are negated the other way — add the bound that would reject the witness
  and show the accepting assertion fail. For **P-arith** the negation is specific after the
  revision-pass rewording: replace the exact arithmetic with the naive same-width spelling
  (`msc + 2·mip·mpc`, `mip × mpc`) and show that **P-arith-accept** now panics (debug) or
  wraps to a wrong verdict, **and** that **P-arith-reject** stops naming G5. Both halves go in
  the table; a leg that stays green under the naive spelling is not load-bearing.
  `build-notes.md` must carry a leg → kind → negation → pasted-output table. A leg green
  under its own negation is not load-bearing and must be rewritten.
  **Two candidate bounds were REMOVED from the guard set precisely because they fail kind 1**
  — `max_parts_per_session ≥ 1` and `max_inflight_parts ≤ MAX_SLOT_INDEX + 1` are implied by
  other guards (see the Success criterion), so no isolating witness exists and shipping them
  would have produced exactly the collapsed, unhonourable negation list of the archived
  brief.
- **Invariant to restore:** sourced from the TARGET repo, which is the only tree Do can read
  (the builder is grounded on `$PDCA_WORKTREE`, a wyrd checkout — cite the ADR, never a
  harness-side catalogue path): **ADR-0045 §Decision 1, "Parse-don't-validate at decode, for
  structural invariants"** (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`),
  read together with its boundary clause — *contextual* checks that need external state stay
  at the operation boundary, never at decode — and the format-maxima statement at
  `0016:390-402`. Over this child's category: **a stored record's fields may not disagree
  with each other, and that disagreement must surface as a typed error, never as a value;
  equally, a stored record must never be refused on a number this deployment merely chose.**
  Both halves are load-bearing and the second is the one three rounds broke. An admission
  record whose `max_sessions` does not match what its own `profile` derives admits sessions
  past the memory bound the reconcile pass is sized for — a fleet-wide OOM admitted by one
  unvalidated field, landing on the maintenance plane rather than on the gateway that caused
  it (`0016:2593`, X64; `0016:2605`, X76 is why the whole profile is stored rather than the
  quotient). Conversely a decoder that enforced a live capacity number would make the ledger
  unreadable the day an operator lowered it — and the ledger is the record every teardown
  path must read to decrement `count`, so an unreadable `mpuctl` wedges multipart fleet-wide
  with no path that clears it (`0016:390-402`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** medium
- **Reproduction:** n/a — new functionality on a base (`origin/main @ 9dbcd72`) where only
  the key grammar exists: `crates/core/src/multipart.rs` is **854 lines** of keys and
  identity types, ending at the retirement-key parsers (verified at Plan).
- **Scope:** extend `crates/core/src/multipart.rs` with `Budget` (the five-field profile),
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
- **Size budget:** ≤ 450 added semantic lines (module extension ≈ 200, test ≈ 250)
  across exactly **2** files, named exhaustively: `crates/core/src/multipart.rs` and
  `crates/core/tests/multipart_budget_admission.rs` (new). A **third**
  changed file means the seam is wrong: STOP and hand back. Down from the 550 the archived
  brief allowed and the 561 v3 actually spent: the reduction is the measure of the descope,
  and a patch approaching 550 means the invented-constant apparatus has crept back in.
  **A SPLIT was considered at this re-plan and REJECTED — recorded here because the size
  backstop will fire again and the next reader deserves the reasoning, not a re-derivation.**
  The v3 sign-off recommended `iterate-plan` over `iterate-do` (correct, and taken) and asked
  whether this slice splits along the `B_ops` / `MAX_SEG_CHUNKS` boundary (#625/#508). It does
  not: those are not *parts of this slice* to separate out, they are bounds this slice was
  wrongly asked to enforce with constants that do not exist on its base. Removing them leaves
  one coherent ~200-line module extension with a single test file — a child, not a parent.
  Splitting it would cost two full cycles to land what is now one small slice, against the
  standing "prefer fewer, larger children" rule. If the size backstop fires again on ROUNDS
  (it fires at 2, and three are already spent on the archived brief), read that as evidence
  about the ARCHIVED brief, not this one: the round counter does not reset, so its next
  firing carries no information about the re-authored slice.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; `typos` still applies (it lints Rust doc comments, which this slice edits), and `docs-renderer` is retained as a registered gate tool only — **no longer load-bearing** now that leg D is reversed and no `docs/` file is edited; the rest warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_budget_admission.rs` — a **NEW** file, not
  optional (C4-verify classifies on an *added* `*/tests/*.rs`; an appended or co-located test
  silently degrades to the green-only branch and would prove nothing). Co-located unit tests
  may ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**. What is built IS
  exercised at Check: the named integration test plus the gating `C4-ci`
  (fmt/clippy `-D warnings`/build/test/deny/conformance) run over the patched tree, and
  advisory `C5-mutants` runs over this diff — v3's equivalent scored 88 mutants, 64 caught,
  24 unviable, 0 surviving, so the mechanical sensitivity bar is known to be reachable for
  this shape. The twelve demonstrations in `build-notes.md` replace the flippable red, and
  the UNVERIFIABLE exit 77 is pre-declared above rather than discovered. **Two §6 items are
  pre-declared, not one** (revision pass): that exit 77, and the docs-currency decision under
  `Production reach` (leg D deferred). Both are known sign-off lines with the reasoning
  already written; neither is a patch defect.
- **Production reach:** N/A by design — `AdmissionRecord` has no live writer until #656–#659
  wire the store round trips; nothing on an existing path changes and no existing call site
  is touched.
  **Docs currency (`AGENTS.md:154-157` — citation corrected at the revision pass; the bullet
  runs 154–157, not 158) is SETTLED as DEFERRED, and PRE-DECLARED as a §6 sign-off line.**
  Reversed 2026-08-09 at the revision pass; the previous revision had it shipping here. The
  general rule fires on a change that "adds or alters … a **persisted field** … in the same
  PR. This is a merge requirement, not a follow-up". **The more specific, already-merged rule
  in the file this slice edits governs instead:** the living architecture doc describes the
  system **as it is** (`docs/design/README.md:28`), so its metadata model "gains these
  namespaces with the slice that first *persists* one — documenting records no code emits
  would make the living doc describe a system that does not exist"
  (`crates/core/src/multipart.rs:55-64`). This slice persists **nothing** — no writer, no
  store call, no production consumer, exactly as `Production reach` says — so it declares a
  record *shape*, not a persisted field in service. The first persisting slices are
  #656–#659, and that is where the paragraph belongs.
  **What the human decides at sign-off (pre-declared so it is a known line, not a fourth
  surprise):** all three archived rounds raised docs currency as §6. The two live readings are
  above; if sign-off takes the general rule over the specific one, the remedy is a
  one-paragraph follow-up commit on this branch — the `mpuctl` record shape into
  `docs/design/architecture/05-building-block-view.md` § "The metadata model" (`:183`), in the
  voice of the ADR-0047 bullet at `:186-192` — **plus** the matching correction of
  `multipart.rs:55-64`. Neither outcome costs a re-plan or a Do round.
- **Citations expected:** cite `path:line` on `9dbcd72` for every change. **The base pattern
  Do must mirror is three parts and every one of them already exists — this is a composition
  slice, and these are the peer callsites Do MAY open:**
  (i) **encode/decode**: the base's `metadata::encode<T: Serialize>` / `decode<T:
  DeserializeOwned>` (`crates/core/src/metadata.rs:1536-1543`) — use them; adding a second
  generic encoder in `multipart.rs` is the duplication that failed v3's T2;
  (ii) **validation inside `Deserialize`**, so a value that decodes cannot be malformed:
  `InodeRecord`'s `#[serde(try_from = "InodeRecordWire")]` + `impl TryFrom<InodeRecordWire>`
  (`metadata.rs:1349`, `:1411`) — the closer model — or `SegmentRecord`'s hand-written
  `Deserialize` funnelling through a fallible constructor (`metadata.rs:1195`, `:1212-1216`);
  (iii) **a per-record decode that attributes the failure to a typed error**:
  `decode_segment_record` (`metadata.rs:2504-2517`).
  **Read the citation namespaces apart — there are THREE, and conflating them is a trap
  (corrected 2026-08-09 after a plan-review finding that the blanket claim below was false):**
  (1) **base-relative** — every `crates/`, `docs/` and `AGENTS.md` citation in this brief
  resolves on `9dbcd72` and was checked there at Plan; these are the only ones Do can open;
  (2) **patched-file-relative** — a `crates/core/src/multipart.rs:NNNN` tagged
  *(batch-review …)* or *(v2/v3 review …)* is relative to a **patched** file (v2's ~2,027
  lines, v3's ~1,400), NOT to the base, where `multipart.rs` is 854 lines. Locate those by
  symbol in the archived patch;
  (3) **HARNESS-relative** — `engine/scripts/run-verify.sh:…` under **Falsifiability**, and
  the `results/issue_715/iteration-v3/patch.diff` salvage path below, live in the *wyrd-pdca*
  harness repo and **do not exist on `9dbcd72` at all**. They are recorded for the human's
  audit of the gate reasoning; Do neither needs nor can open the first.
  **Salvage — narrowed deliberately:** `$PDCA_HARNESS_ROOT/results/issue_715/iteration-v3/patch.diff`
  (path relative to the HARNESS repo, not `$PDCA_WORKTREE`; a codex builder runs with cwd =
  the worktree and must resolve it absolutely). Take **only** the `Budget` / `AdmissionRecord`
  struct shapes and the `u_ref` / `max_sessions` arithmetic. Do **NOT** carry over
  `encode_record`, `chunkref_bytes`, `WIDEST_SCHEME_BYTES`, `CHUNKREF_FRAGMENTS_REFERENCE`,
  `MAX_CHUNKREF_BYTES`, `MAX_SEG_CHUNKS_FORMAT_MAX`, `MAX_PART_CHUNKS_FORMAT_MAX`,
  `MAX_PUBLISHABLE_CHUNKS` or `VALUE_CEILING_HALF` — that block is exactly what this re-plan
  removes.
- **Prior-art check (triage cycles):** verified at Plan against `origin/main @ 9dbcd72` by
  affected path. `git -C ../wyrd show origin/main:crates/core/src/multipart.rs` is 854 lines
  and defines no `Budget`, `AdmissionRecord` or record codec; `metadata.rs` defines no
  multipart record type. No open PR touches `crates/core/src/multipart.rs` (#710 and #711 are
  in flight over `crates/core/src/metadata.rs`, which this slice does not modify).
  Closed/rejected work, all read at this Plan: #654's two archived attempts, #692's two
  (`results/issue_692/iteration-v{1,2}/`), and **this bundle's own three**
  (`iteration-v{1,2,3}/`) — the v3 batch review's three blockers (`review-batch.md`) are
  answered here rather than deferred: the two `max_chunks_per_value` bracket-overhead
  findings are **removed with the constant they were about**, and the
  `MAX_PUBLISHABLE_CHUNKS`-at-decode finding is inverted into binding positive leg **P2**.
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0 — the chain's root; both ordering fields are DELIBERATELY
  EMPTY, not unset.** #691 (the key grammar these types build on) is COMPLETE **and merged**
  — PR #703, in `origin/main @ 9dbcd72` — so it is deliberately NOT carried as a
  `Depends on`: the base already contains it, and under `auto_merge = false` `_runnable`
  gates every declared `Depends on` on `merged.is_merged`, so naming an already-merged
  prerequisite adds a liveness check that can only ever block this bundle, never help it.
  `Conflicts with` is empty because this child touches only `crates/core/src/multipart.rs`
  and its own new test file — #710 and #711 are in flight over
  `crates/core/src/metadata.rs` and `crates/custodian/`, neither of which this slice modifies,
  so it MAY share a wave with either. **Re-checked at the revision pass, and the check got
  *simpler*, not harder:** leg D's reversal takes
  `docs/design/architecture/05-building-block-view.md` back out of this bundle's file set, so
  the one newly-shared file the previous revision had to reason about is gone. Within this
  chain #716 and #717 are ordered behind this bundle by `Depends on` and are wave-serialised
  against it regardless — no `Conflicts with` entry is needed or correct.
  **Downstream:** #716 depends on this child, #717 on #716; #693 and #655 follow #717.
  Anything that later edits `multipart.rs` must declare against this chain.
- **Disposition hint:** new-feature
- **Plan-review response:** one revision pass over `plan-advisory-plan-reviewer.md`
  (2026-08-09), every finding re-checked against `origin/main @ 9dbcd72` before acting.
  **(1) envelope re-scope** — brief STANDS on the substance (no type tag on the wire, the
  base already provides the codec, `notes.json` carries `"comments": []` so the issue body is
  the planner's own forward-looking text, not a maintainer instruction) but is REVISED to do
  what the reviewer actually asked: the re-scope is now labelled as such and pre-declared as a
  §6 sign-off decision, with the effect on the later children recorded and verified
  (`results/issue_716/brief.md:5-13`, `results/issue_717/brief.md:8-14` already carry the same
  correction — nothing downstream is blocked) and the reversal remedy named (a slice after
  #717, never here). **(2) success criterion internally impossible** — ACCEPTED, real
  contradiction: P-arith rewritten from "maximal value ⇒ typed rejection" to a verdict-equality
  oracle against exact integer arithmetic, with the reviewer's own witness promoted to the
  required **P-arith-accept** case and a maximal G5 violation as **P-arith-reject**; P-arith is
  now explicitly not a ninth guard, so "exactly G1–G8" holds. **(3) P3 crosses the
  decode/configuration boundary** — ACCEPTED: P1/P2/P3 are now mechanically decode-only
  (hand-authored JSON bytes through S1/S2, never a constructor), this slice ships no
  configuration-validation constructor at all, and full guard-checked witnesses replace the
  partial ones. **(4) "typed rejection" not observable** — ACCEPTED, and it was a real hole:
  `metadata::decode` returns `anyhow::Result<T>` (`metadata.rs:1541-1543`) and `DeError::custom`
  stringifies (`:1212-1216`), so the criterion now names two surfaces — validating `Deserialize`
  (S1) and `pub fn decode_admission_record → Result<_, RecordError>` (S2, peer of
  `decode_segment_record`, `metadata.rs:2504-2517`) — with one distinct `RecordError` variant per
  guard, and states which surface each assertion is made through. **(5) architecture paragraph**
  — ACCEPTED and REVERSED: leg D is out, deferred to the first persisting slice (#656–#659) per
  the merged in-file policy at `multipart.rs:55-64` (read verbatim on the base), the recurrence
  handled by pre-declaring the decision as a §6 line instead; Scope's self-contradiction is gone
  (2 files, not 3), and the `--classify` dry-run was re-run on the two-file shape — `ADDED_TEST
  crates/core/tests/multipart_budget_admission.rs` + `CRATE crates/core`, unchanged. Knock-ons
  updated: Scope, Size budget (≤ 450 / 2 files), External dependencies, Falsifiability,
  Verification posture (two pre-declared §6 items), Production reach, Ordering note, and the
  `AGENTS.md:154-158` → `:154-157` citation fix.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Humans must accept compile-only criterion absence as sufficient red evidence—zero tests run pre-fix, while the patched target ran 16/16 focused tests and every independently rerunnable CI component passed (`crates/core/tests/multipart_budget_admission.rs:91`).; C5 Causal adequacy — Rebuild must add a witness where the ceiling arm determines `U_ref`: replacing its addition with multiplication survives all tests, so wrong admission-memory arithmetic is not detected (`crates/core/src/multipart.rs:1046`, `crates/core/tests/multipart_budget_admission.rs:80`).; T4 Contribution — Humans must clear closed/rejected prior art and contribution evidence—the bundle omits `scripts/review-branch`, `scripts/pdca contribcheck`, and archived iterations; the independently queried affected-path PR history found only merged #703.. 16 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 59 mutants tested in 2m: 1 missed, 51 caught, 7 unviable
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Humans must decide whether compile-only criterion absence is sufficient red evidence — removing production leaves 12 unresolved-symbol/variant errors and runs zero tests, while restoring it independently passes 17/17 focused tests and every CI component after isolating cargo-deny's read-only-cache host fault (`crates/core/tests/multipart_budget_admission.rs:37`).; T4 Contribution — Humans must inspect or rerun the two blockers reported by the unavailable `scripts/review-branch --bundle` log before treating deep review as complete — the independent affected-path query found only merged #703 among 296 closed/merged PRs and no competing open PR.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b. 18 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
