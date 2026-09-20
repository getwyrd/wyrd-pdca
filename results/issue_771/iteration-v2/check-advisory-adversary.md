# Adversarial review — issue #771 (multipart retirement obligation)

Method: re-ran the asserted GREEN leg against a writable copy of `$PDCA_TARGET`
(`cargo test -p wyrd-core --test multipart_retire_obligation` → 26/26 pass, reproduced), then
attacked `decode_retire_obligation` with ~15 hand-authored values it has no witness for, and
attacked the suite by deleting checks and assertions to see which ones are load-bearing.

## Findings

- NEEDS-HUMAN [human] — **The decoder accepts a generation shape no writer can install, and the
  doc that licenses it states a falsehood about the data model.**
  `crates/core/src/multipart.rs:2583` says a retired generation's map is "a flat chunk list, a
  segment group, or (for a generation whose root carried both) **each**", and `checked_shape`
  (`crates/core/src/multipart.rs:2917`) rejects a generation only when it names **neither**. No
  root can carry both: `InodeRecord` holds one `chunk_map` field (`crates/core/src/metadata.rs:1388`)
  of type `ChunkMap`, a two-arm `Flat | Segmented` enum whose own doc cites "proposal 0016
  decision 7(a)" (`crates/core/src/metadata.rs:1014`). Concrete case, run against this tree: key
  `retire_key(Bytes, g:42:4)`, value
  `{"generation":{"inode":42,"version":4,"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}],"segments":{"nonce":"c3…c3","epoch":9}}}`
  → `Ok(RetirePayload { generation: Some(RetireGeneration { chunks: [..], segments: Some(..) }) })`.
  That contradicts the brief's success criterion ("every shape no writer installs is rejected with
  a typed `RecordError`"), the module's own writer table (`crates/core/src/multipart.rs:2771-2781`,
  which has exactly one `generation` row), and the sentence this patch adds to the living
  architecture doc — `docs/design/architecture/05-building-block-view.md:204` asserts "the accepted
  shapes are exactly the ones some batch of the protocol installs" and describes "a superseded
  generation's chunk map **or** segment group". **Note the direction of the two gating T4
  blockers**: both ask for an *acceptance / round-trip* test of this hybrid, i.e. they would freeze
  it into a stored format three later slices build against. The authorities genuinely conflict —
  `0016:355` spells `{inode, version, chunks, segments?}` while `0016:2416` spells
  `{inode, version, chunks?, segments}`, and `ChunkMap` implements neither as a union — so whether
  to make the hybrid a `RecordError` (mutual exclusion, mirroring `ChunkMap`) or to justify it
  against `ChunkMap` is a format decision a rebuild should not guess.

- NEEDS-HUMAN [impl] — **R9's advertised "file-wide property" is unfalsifiable; it cannot fail for
  any witness in the file.** `crates/core/tests/multipart_retire_obligation.rs:167`'s
  `decode_witness` re-encodes every accepted witness and compares to the input, and the file header
  (`crates/core/tests/multipart_retire_obligation.rs:22-28`) claims that makes
  `encode(decode(bytes)) == bytes` "a property of the *whole* file's accepted set". But
  `decode_retire_obligation` already ends in `require_canonical(payload, value, "retire:")`
  (`crates/core/src/multipart.rs:3096`), which returns `Err` unless exactly that equality holds — so
  the assertion restates the production postcondition it is policing. Verified: with the whole
  `assert_eq!` block deleted from `decode_witness`, the suite is still **26 passed; 0 failed**. The
  suite's only real R9 leg is `a_foreign_spelling_of_an_accepted_payload_is_rejected`
  (`crates/core/tests/multipart_retire_obligation.rs:646`). The sibling helper this is modelled on
  (`crates/core/tests/multipart_session_records.rs:169`) earns its identity assertion through the
  **S1** leg (`metadata::decode`, which does not call `require_canonical`); this file legitimately
  drops S1, and with it the only decode path the assertion could ever have caught. Fix is the
  claim, not the code: say identity is enforced in production by `require_canonical` and pinned by
  the foreign-spelling leg, rather than presenting the helper as independent evidence.

- NEEDS-HUMAN [impl] — **`PartNumberSet::from_numbers` can mint a value whose stored spelling its
  own decode refuses**, against the "unrepresentable at the source" thesis its doc states
  (`crates/core/src/multipart.rs:2434-2438`: "it can only produce the canonical encoding its own
  decode accepts"). `from_numbers([])` returns `PartNumberSet(vec![])`
  (`crates/core/src/multipart.rs:2439-2452`) — the test pins this at
  `crates/core/tests/multipart_retire_obligation.rs:767` — and that set serializes to `[]`, which
  `checked_shape` then refuses: `decode_retire_obligation(retire:bytes:s:…:7, br#"{"parts":[]}"#)`
  → `Err(RetireObligationOwesNothing { component: "parts" })` (verified). The writer rows this
  constructor exists for compute a possibly-empty set (the root flip's "staged parts Complete did
  not name", `0016:662`, `:919-921`); a #656–#659 writer that mints one and installs it stores an
  obligation no drain can decode, and the session's terminal-delete emptiness gate (`0016:673`)
  never clears. Cheapest fix: make the constructor fallible (or return `Option<Self>` for the empty
  case) so the non-empty obligation lives in the type rather than in writer discipline.

## Attacks that failed (could not refute)

- The accepted set otherwise matches the writer table exactly. Hand-authored probes for
  `{parts:"all"}` under `retire:records:`, `{parts:"all"}` without `session`, `{session}` /
  `{parts}` / `{seg}` under a per-part token, `{chunks}` under a suffix-free token, `{generation}`
  under an `s:` token, `{session,all}` under a `g:` token, `{seg}` under `retire:bytes:`,
  `{chunks}`/`{generation}` under `retire:records:`, and `{}` under a records per-part key were each
  refused with the typed variant the `Component` table predicts
  (`crates/core/src/multipart.rs:2680-2734`, `:3003-3044`).
- The `Component` table is not covered by `C5-mutants` (consts, not functions), so I mutated it by
  hand: relaxing `mode` on `SESSION_`/`ALL_PARTS_`/`CHUNKS_`/`GENERATION_`/`SEG_COMPONENT`, and
  `scope` on `PARTS_`/`GENERATION_COMPONENT`, each changes a verdict the suite asserts by exact
  error identity. No entry is dead weight.
- Canonical-set arithmetic: `[[1,1],[1,1]]`, `[[1,5],[4,8]]`, `[[5,9],[1,3]]`, `[[4,2]]`, `[[0,3]]`,
  `[[1,1000000]]`, `[[1,4294967296]]`, `[[-1,4]]`, a 50-run set and `[[1,4],[6,9]]` (legal gap of 1)
  all land exactly where `PartNumberSet::from_runs` (`crates/core/src/multipart.rs:2414-2432`) says;
  the `previous_hi + 1` overflow argument holds because both endpoints pass `PartNumber::new` first.
  Duplicate JSON keys, `"chunks":[]`, `"segments":null`, a `\u`-escaped nonce and reordered fields
  are all refused.
- `C4-verify` is `unverifiable` (RED leg does not compile against the reverted base). That is
  pre-declared in the brief's Falsifiability section as a born-at-tier sign-off item, so I did not
  score it as a refutation. Likewise the 32 `C4-diff-cov` MISS lines are all `RecordError` `Display`
  arms (`crates/core/src/multipart.rs:535-596`); no sibling test in `crates/core/tests/` asserts
  error strings, so this is not a repo convention being broken.
- Size/scope: 966 added semantic lines across exactly 3 files (module 468, test 497, docs 1), inside
  the brief's ≤1,000 budget — the round-1 T1 concern is resolved and I am not re-raising it.
