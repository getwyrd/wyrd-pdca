# Adversarial review — issue #771 (`multipart-retire-obligation`)

Inputs: `patch.diff`, `brief.md`, `check-gates.json`, `gate-logs/`. Grounded on
`$PDCA_TARGET`. I rebuilt the crate in a scratch copy and re-ran the named test plus a
19-input probe suite against the production decoder, so the findings below are reproduced,
not inferred.

## Findings

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2704`: half the R2 rule ("an obligation
  naming nothing is rejected") is not load-bearing, and a concrete input proves it.** The
  emptiness rule for a generation's flat map lives in a *match guard*
  (`(Some(chunks), None) if !chunks.is_empty()`). I replaced that guard with `true` in a scratch
  copy of `$PDCA_TARGET` and re-ran `cargo test -p wyrd-core --test multipart_retire_obligation`:
  **27 passed, 0 failed** — and the witness `{"generation":{"inode":42,"version":4,"chunks":[]}}`
  under `retire:bytes:g:42:4` then decodes to
  `Ok(RetirePayload { generation: Some(RetireGeneration { .., map: Flat([]) }) .. })`, i.e. an
  accepted obligation that owes nothing, re-encoding byte-identically so `require_canonical`
  (`multipart.rs:3175`) cannot catch it either. That is exactly the residue class the brief's R2
  forbids and this is exactly the gate's own red: `gate-logs/C5-mutants.log:13`
  (`MISSED … replace match guard !chunks.is_empty() with true`). The R2 test
  (`crates/core/tests/multipart_retire_obligation.rs:395-407`) covers `{}`, `{"chunks":[]}`,
  `{"parts":[]}` and a generation with **no** map key at all (`:402-406`) — never a generation
  whose flat map is *present but empty*. The brief's Falsifiability section makes this binding,
  not cosmetic: "A leg that stays green under its own negation is not load-bearing and must be
  rewritten." Since the born-at-tier posture makes the nine negations the *substitute* for a
  behavioural red (`gate-logs/C4-verify.log:110-116` — the RED leg never executed a
  discriminator), a rule that survives its own negation removes the only evidence this leg has.
  Minimal fix: add the witness to `an_obligation_owing_nothing_is_rejected`. While fixing, note
  a second, related inconsistency the same match has — `{"generation":{…,"chunks":[],"segments":
  {…}}}` is reported as `RetireGenerationTwoMaps` (probe output), so `Some([])` counts as a map
  for the two-maps arm at `:2703` but not for the owes-nothing arm at `:2704`; normalising an
  empty `chunks` to `RetireObligationOwesNothing` before the arm split makes both consistent and
  kills the mutant.

- **NEEDS-HUMAN [human] — the gating red `C4-ci` is not this patch's: `gate-logs/C4-ci.log:2856`
  is `RUSTSEC-2026-0258` (h2 0.4.15, "unbounded empty DATA frames") against `Cargo.lock:111`.**
  The diff touches exactly three files (`crates/core/src/multipart.rs`,
  `crates/core/tests/multipart_retire_obligation.rs`,
  `docs/design/architecture/05-building-block-view.md`) and no manifest or lockfile, and the log
  reports `advisories FAILED, bans ok, licenses ok, sources ok` — an advisory-DB refresh, not a
  regression this bundle introduced. Per issue #236 I do **not** score this as a refutation; it
  needs a human scope decision (bump `h2` to ≥ 0.4.16 in a separate bundle vs. a `deny.toml`
  exemption) because taking it inside this bundle would break the brief's "exactly 3 files"
  budget. Flagging it so sign-off does not read the gating red as evidence against the fix.

- **NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:204` states protocol
  behaviour the system does not have, in a doc whose stated rule is "as it is".** The added
  paragraph says an obligation "is installed under a compare-and-set that requires its key absent
  and drained under one that requires its exact bytes" — but nothing installs or drains one
  (`multipart.rs:77-78`, and the brief's own "Production reach: this child ships **no**
  production reach"), and the *immediately preceding* paragraph (`:202`) promises the opposite:
  "the protocol itself (fenced state transitions, staged publication, retirement) arrives with
  the store round trip (#656–#659) and is specified in the proposal, not here" — as does the
  module header the same patch rewrote (`multipart.rs:83-85`: the doc "defers the *protocol* …
  and the retirement drain — to the proposal"). The brief's R10
  asked to "Extend that sentence in its own voice and length; do not restate the proposal". Low
  severity and the host may decline it as taste — but the two adjacent paragraphs now disagree
  about what is landed, which is the thing a *living* doc is for. Trimming the install/drain-CAS
  and token-minting clauses (keeping the namespaces, the value's contents and the
  decoded-against-its-key rule, which are what this child actually landed) resolves it.

## Attempted refutations that failed (stated, so the silence is informative)

- **The records-mode `{session}` exclusion** (`multipart.rs:2764-2768`, `SESSION_COMPONENT` mode
  `Bytes`) looked like the highest-value refutation, since `0016:356`'s value column literally
  reads "`{session, parts}` and/or `{seg: …}`" for `retire:records:`. I checked it against every
  writer row in the batch table (`0016:659-673`) and the reaper (`:2186-2195`): the rows install
  `retire:records:{parts}` (root flip), `retire:records:{seg:<g>:<E>}` (fence release, abort/reap
  fence, `Completing`→`Aborting`, restore fence `:823`, reaper rollback `:2194`) — **no row
  installs a records-mode `{session}`**, and the session's own records are the terminal delete's
  (`:673`). Refusing it is also the reversible direction for a frozen format. Could not refute.
- **R6's exact epoch equality** (`multipart.rs:3067-3074`). I checked every row that installs a
  `{seg}` obligation: each preconditions `require(mpu == …@E)` and names `seg:<g>:E`, including
  the two that advance the session to `E+1` in the same batch (`0016:665`, `:2188-2194`). Token
  epoch `E` == group epoch `E` in all of them; no writer row needs the `E±1` window the archived
  v3 shape had. Could not refute.
- **R1 completeness.** All nine writer shapes decode under their own key, including the two
  combined ones (`{session, parts}`, `{parts}+{seg}`); the four session-scoped/per-part/generation
  cross-products I probed are all typed rejections. Could not refute.
- **Input probing of the decoder** (19 hand-authored values run against the production
  `decode_retire_obligation`): `parts` as `null` / object / 3-tuple / `[[1,4294967295]]` /
  `[[1,1],[1,1]]`, duplicate JSON keys, trailing whitespace, `"chunks":null` beside `segments`,
  a negative `len`, a `u128`-max chunk id, an empty `placement`, `{parts}+{seg}` under a
  `retire:bytes:` key, `{session, all}` under `retire:records:`, `{chunks}+{session}` under a
  per-part token, `{seg}` with `epoch ± 1`. Every one is either a typed `RecordError` or a
  correctly-accepted contextual case (empty/short `placement`, per ADR-0045 `:45-49` and the
  brief's explicit R8 boundary). Nothing decoded that should not have, apart from the guard case
  in finding 1. Could not refute.
- **Serialization identity (R9).** `require_canonical` (`multipart.rs:1725-1735`) is the real
  gate and it caught every foreign spelling I could invent (field order, inserted whitespace,
  `false` spelled instead of omitted, `"chunks":[]`, `"parts":null`, `\u`-escaped nonce). The test
  file no longer over-claims the helper as independent evidence
  (`crates/core/tests/multipart_retire_obligation.rs:22-37`), which was the previous round's
  finding. Could not refute.
- **No writer-side constructor / no `Deserialize` on `RetirePayload`** (`multipart.rs:2894`) —
  I checked this against the siblings rather than taking the doc comment's word: `SessionRecord`
  (`:1921`) and `PartRecord` (`:2283`) are equally constructor-less with private fields, so
  #656–#659 is no worse off here than it already is. Could not refute.
