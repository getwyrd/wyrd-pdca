# Adversarial review — issue #717 (`multipart-staging-retire-pending`)

Advisory only; nothing here gates. Evidence was re-run at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt`, patch applied, working tree dirty over `c824243`).
Toolchain was available: `cargo test -p wyrd-core --test multipart_staging_retire` →
**24 passed, 0 failed**. The pre-declared UNVERIFIABLE C4-verify (RED reverts production and
the new test fails to *compile*) is not refutable and I did not score it; `C5-mutants`
(70 tested, 49 caught, 21 unviable, **0 missed**) independently answers "would each leg have
gone red", so I spent my attempts on holes mutation cannot see: rules that are *absent*, and
claims the comments make that the code does not keep. Concrete probes were run out of a
throwaway crate under `$PDCA_SCRATCH` linking `wyrd-core` by path (removed).

## Findings

- **NEEDS-HUMAN [human]** — `crates/core/src/multipart.rs:2765` (`pub enum RetirePayload`) and
  `crates/core/src/metadata.rs:1596`/`:1601` (`pub owner` / `pub staged`): every new invariant
  this child lands is enforced **only on the decode side**, and the public shapes will happily
  mint — and `metadata::encode` will happily serialize — records their own decoders then refuse
  forever. Verified, not hypothesised:
  `encode(&RetirePayload::Generation { inode: 42, version: 5, chunks: vec![c], segments: Some(g) })`
  → `{"Generation":{…"chunks":[…],"segments":{…}}}` → `decode_retire_obligation` =
  `RetireGenerationSourcesConflict`; `RetirePayload::Parts { parts: PartNumberSet::default() }`
  (note `Default` is derived at `:2658`) → `{"Parts":{"parts":[]}}` →
  `RetireObligationOwesNothing`; `RetirePayload::Chunks` with an `rs(0,1)` `ChunkRef` →
  `ChunkSchemeUnsupported`; `PendingEntry { lease_expiry_millis: 5, owner: Some(id), staged: None }`
  → `{"lease_expiry_millis":5,"owner":"…"}` → `TornOwnedEntry`. Why this is not cosmetic for
  *these two* classes specifically: a `retire:` obligation is installed with `require_absent`
  and the drain "must not guess" (`0016:358-372`), so an obligation stored through a public
  variant is reclamation evidence **nothing can ever read back** — the permanent-loss outcome
  the token grammar exists to prevent; and a torn `PendingEntry` landing under `pending:` makes
  `expired_pending_chunks` (`crates/custodian/src/gc.rs:489`, `metadata::decode(&value)?` inside
  the `scan("pending:")` loop) fail the **entire** GC sweep — precisely the "error that aborts a
  whole reconcile step" this patch's own doc argues against at `crates/core/src/multipart.rs:2449`.
  This is the module's own standing pattern being broken *inside this diff*: every sibling record
  carrying a cross-field rule keeps private fields and accessors (`AdmissionRecord`
  `multipart.rs:1604`, `SessionRecord` `:1902`, `SlotRecord` `:2053`, `PartRecord` `:2264`), as do
  both **structs** this patch adds (`StagedPlacement:2452`, `OwnedEntry`) — `RetirePayload` and
  `PendingEntry`'s two new fields are the only outliers. Routed to a human rather than to Do
  because Rust enum variant fields cannot be made private: closing it means a type redesign
  (private inner enum + validating constructors, or a newtype) that changes the API #656–#659 will
  write through, and `PendingEntry`'s public literal is the very shape the brief's 8-file
  mechanical ripple depends on. A decision is needed on whether decode-only enforcement is
  accepted here and recorded against the first writer, or closed now.

- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:1580-1583` and
  `crates/core/src/multipart.rs:2614-2615`: the justification given for `skip_serializing_if` on
  `staged` is refuted by the very line it cites. The comments claim decode→encode identity holds
  "on an owned `sidx:` entry across its own renewals … so the stored bytes are exactly what was
  read", and that "an owned lease is renewed in flight by re-encoding the entry it read
  (`renew_pending`, `metadata.rs:2079`)". `renew_pending`
  (`crates/core/src/metadata.rs:2057-2081`) does neither: it addresses `pending_key(chunk)` (`:2071`)
  and can never reach a `sidx:` key, and at `:2079` it writes `encode(entry)` — the **caller's**
  single `&PendingEntry`, applied to every chunk in the slice — while the decoded prior
  (`existing`, `:2074`) is read only for the expiry test. So no `owner`/`staged` value can survive a
  renewal by byte-identity today, and under this signature a future owned-renewal caller would
  necessarily write one chunk's `staged` placement onto every other chunk in the batch. The legacy
  half of the argument (`:1578-1580`) is correct and leg 2 stands; only the owned half is
  unwarranted. Fix is a reword (the wiring itself is #657's), and the brief's own warning —
  "`0016:475-485` mis-describes the current code on exactly this point … trust the code, not that
  paragraph" — is the reason not to re-import 0016's framing verbatim.

- **NEEDS-HUMAN [impl]** — `crates/core/src/multipart.rs:2508`: "the accepted sets are pinned equal
  by the test file's S1/S2 agreement helper" is false, and the same test file proves it. Probe:
  `{"lease_expiry_millis":9000,"owner":"1a…","staged":{…},"x":1}` is **accepted** by S1
  (`metadata::decode::<PendingEntry>` + `OwnedEntry::from_pending` — `PendingEntryWire` is
  deliberately open) and **refused** by S2 (`decode_owned_entry` → `NoncanonicalRecordValue`), which
  is exactly what `crates/core/tests/multipart_staging_retire.rs:735`
  (`an_unknown_field_is_refused`) asserts. `decode_owned_both`
  (`crates/core/tests/multipart_staging_retire.rs:155`) is invoked on only two accepted witnesses and
  would `assert_eq!` -fail if handed that one, so nothing "pins the sets equal"; the next paragraph
  of the same doc comment concedes the divergence. Reword to what is true (the *rules* are shared;
  the accepted sets differ by exactly the canonical-bytes gate).

- **NEEDS-HUMAN [impl]** — `crates/core/src/multipart.rs:2448-2449` and
  `crates/core/tests/multipart_staging_retire.rs:464`: "a length-mismatched placement decodes and
  is quarantined by GC's safety gate and attributed by the drain" is stated in the present tense
  about machinery that does not exist in this tree. GC's malformed set is built only over
  **committed** chunk maps (`crates/custodian/src/gc.rs:146`, `:152`, builder doc `:336-340`), the only `pending:`
  reader scans `pending:` alone (`crates/custodian/src/gc.rs:488`), nothing anywhere reads a
  `sidx:` key, and there is no drain. 0016 says this as *design* (`0016:416-429`, via a "staged
  reference build" that is decision 2's future pass) and the brief explicitly **withdrew** the
  quarantine claim as undemonstrable in this slice — so restating it as fact inside a module whose
  own header is scrupulous that "nothing here is written yet" is the one tense slip in the diff.
  Attribute it to the proposal or put it in the future tense.

## Attempted and could not refute

- **The key-taking cross-checks.** I tried to find a payload/key pair the decoders let through:
  `Records`/`Chunks`/`Parts`/`Session`/`Generation` against both modes, both token kinds, and
  present/absent part suffix — every illegal combination errors, and the mapping matches 0016's
  "Written by" columns (`0016:353-356`, token grammar `:357-378`). `retire:records:g:<inode>:<version>`
  is rejected, and reading 0016 that is right: a superseded generation's `seg:` records are deleted
  by its own `retire:bytes:` generation obligation, not by a record-mode generation token.
- **The nested-value seams.** `SegmentGroup` (the one nested type read *without* a module-local wire
  mirror) already validates its nonce and denies unknown fields at decode
  (`crates/core/src/metadata.rs:826-837`), so leg 1p has no `segments`-shaped hole beside it.
  `ChunkRefWire`/`EcSchemeWire`/`StagedPlacementWire` are closed and require `placement`, so
  round-trip identity through the retirement payloads holds.
- **Arithmetic and bounds.** `PartNumberSet::from_runs` validates both endpoints through
  `PartNumber::new` *before* `previous_hi + 1` (`multipart.rs:2665-2681`), so the coalescing test
  cannot overflow; `len()` is exact at the format maximum (`from_runs([(1, 999_999)]).len() == 999_999`).
- **Leg 1e's deliberate narrowing.** An owned-shaped value under a `pending:` key still decodes, as
  the brief requires, and explicit `"owner":null,"staged":null` decodes to the legacy shape and
  re-encodes without the nulls — no regression against the pre-patch open record.
- **Red→green load-bearingness.** I did not re-derive it by hand; `C5-mutants` reports 0 surviving
  mutants over this diff, which covers the "a leg green under its own negation" risk better than a
  spot check would.
