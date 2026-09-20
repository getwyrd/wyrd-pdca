# Adversarial review — issue #771 (multipart retirement obligation)

Re-ran the evidence against `$PDCA_TARGET` (patch applied as working-tree changes on
`debf521 pre-fix base`). The test exercises the **production** symbols
(`decode_retire_obligation`, `metadata::decode::<RetirePayload>`) — no parallel
re-implementation, no mock — and I could reproduce its GREEN leg. The `C4-verify`
UNVERIFIABLE is a compile-absence RED, pre-declared at `brief.md:85-101`, so I do not score it
as a refutation. All findings below were **executed** against the patched crate from a scratch
probe binary linked to `crates/core`, not reasoned about on paper.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2667` (`PARTS_COMPONENT { mode: None }`,
  with `PartScope::All` carrying no constraint of its own at `:2464-2485`): the `all` wildcard
  decodes under three key/shape combinations no writer row installs, and the T4 review caught
  only one of them.** Verified accepts:
  `decode_retire_obligation(retire_key(Records, <suffix-free s: token>), br#"{"session":true,"parts":"all"}"#)`
  → `Ok`, and likewise `{"parts":"all"}` and `{"parts":"all","seg":{…}}` under `retire:records:`,
  and `{"parts":"all"}` **alone** under `retire:bytes:`. `0016:2186-2187` is the only row that
  ever spells `all`, and it is `retire:bytes:` **with** `session`
  (`CAS Open@E -> Aborting@E+1 + put retire:bytes:{session, all}`); the patch's own writer table
  at `multipart.rs:2736-2744` has no other. A records-mode `{session, all}` instructs a drain to
  delete every `part:`/`psum:` record of the session **and** its own records with no orphan-marking
  step at all — the outcome-(a) loss the brief's invariant section names. The fix must bind all
  three combinations (`All` ⇒ mode `Bytes` **and** `session` present), not just the one T4 quoted,
  or the next round re-opens on the other two. No test would go red today:
  `crates/core/tests/multipart_retire_obligation.rs:337` covers records-mode `{parts:<set>}` only.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2658` (`SESSION_COMPONENT { mode: None }`):
  `{"session":true}` decodes under a `retire:records:` key (verified `Ok`, re-encode identical),
  and the justification given for the mode-neutrality is contradicted by the patch's own table.**
  The constant's doc claims `session` is "installed … by `retire:records:` by the publication that
  supersedes a session's staging records (`0016:356`)", but the publication's writer row
  (`0016:662`, root flip) installs `1 put retire:records:{parts}` "naming only the PUBLISHED parts"
  — no `session` — and the payload table this same patch writes at `multipart.rs:2742-2744` lists
  no `retire:records:` + `session` row either. Per `RetirePayload::session`'s own doc
  (`multipart.rs:2786-2788`, citing `0016:673`) that component names the `mpu:` record and the
  surviving `slot:` records, so a records-mode `{session}` obligation is precisely "tear the
  session's naming records down without ever marking the bytes they protect". Two documents inside
  one diff disagree, the code implements the looser one, and no test pins either behaviour.

- **NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:2762-2764`: the public `Deserialize` derive
  makes a `RetirePayload` reachable that carries *none* of the four key relations, and the type
  cannot tell the two provenances apart.** Verified through `metadata::decode::<RetirePayload>`:
  `{"chunks":[…]}` and `{"session":true,"generation":{…}}` both decode successfully even though
  `decode_retire_obligation` rejects them as `RetireModeMismatch` / `RetireTokenScopeMismatch`.
  The base precedent (`SessionRecord` `:1873`, `PartRecord` `:2235`) also derives it, but for those
  S1 ≡ S2 apart from the canonical-bytes gate; this is the first record class where the two seams
  differ in *rules*, so #656–#659's drain can legitimately hold a payload nothing validated against
  its key. T4 raised this as a convention break — but `brief.md:158-160` **mandates** the S1 seam
  (the `decode_both` helper asserts S1/S2 agreement), so "remove the derive" contradicts the brief.
  This is a scope/architecture call for the human: keep S1 and accept the hazard, seal the derive
  and rewrite the mandated helper, or introduce a distinct key-checked value type.

- **Attempted and could not refute:** R6's epoch exactness (`E±1` rejected, `E` accepted, and the
  `g:`-token limit correctly left unbound); R4 in both directions and R5 in all three; R7 over both
  chunk lists; R8's canonicality (adjacent, overlapping, out-of-order, reversed, `0`,
  `MAX_PART_NUMBER + 1`, and the `MAX_PART_NUMBER` boundary itself), plus `[[1,4294967296]]` which
  falls out as a typed `MalformedRecordValue`; R9 identity against trailing whitespace,
  `"session":false`, `"chunks":[]`, field reordering, a `\u` escape and duplicate JSON keys — every
  one refused; the owes-nothing family; and `retire:records:g:<inode>:<version>`, under which I
  could construct **no** decodable payload (correct: no writer installs one). `PartNumberSet`'s
  `+ 1` arithmetic is bounded by `PartNumber::new` on both paths, so neither `from_runs` nor
  `from_numbers` can overflow. `C5-mutants` reports 0 surviving mutants on the diff
  (`gate-logs/C5-mutants.log:13`), which corroborates the load-bearing claim for the checks I could
  not break by hand; the nine isolating negations themselves live in `build-notes.md`, which is
  withheld from this leaf, so that half of the claim is unverified here rather than refuted.
