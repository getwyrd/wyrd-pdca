# Adversarial review — issue #771 (`multipart-retire-obligation`)

Method: rebuilt the target into a scratch clone (cargo 1.96 available) and (a) re-ran the
patch's suite, (b) ran a **16-negation battery** against the production checks to test the
brief's "nine isolating negations" substitute for the pre-declared UNVERIFIABLE red, and (c)
ran two probe suites of hand-authored values against `decode_retire_obligation` looking for an
accepted-but-unsafe or refused-but-legal shape. Scratch removed.

## Findings

- **NEEDS-HUMAN [impl] — `decode_retire_obligation` validates the mode and then throws it
  away, so the drain it is built for cannot tell the two `{parts}` obligations apart**
  (`crates/core/src/multipart.rs:3144-3157`; the mode is in hand at `:3148` and dropped at
  `:3156`). Verified concretely in a scratch clone: `retire:bytes:s:<id>:7` and
  `retire:records:s:<id>:7` over the *same* value `{"parts":[[1,4]]}` both decode to
  **identical** `(RetireToken, RetirePayload)` pairs — `PARTS_COMPONENT.mode` is deliberately
  `None` (`:2758-2764`), so `{parts:<set>}` is the one legal shape under **both** modes and
  nothing in the returned pair distinguishes them. Those two obligations are opposites: bytes
  mode means *orphan-mark the unnamed staged parts' chunks, then delete their records*
  (`0016:662`, `:919-921`), records mode means *delete the published parts' records and never
  orphan-mark, because their bytes are live object content* (`0016:356`). A #656–#659 drain
  handed only the decode result must re-parse the key to know which; if it instead infers mode
  from the components present — the natural reading of a validated payload — it orphan-marks a
  published object's live fragments. That is exactly the "boolean misread once is silent data
  loss" hazard the mode-in-the-key rule exists to prevent (`0016:434-441`), and the module's own
  doc says "The drain dispatches on the prefix". The fix is inside this diff's own signature
  (return `(RetireMode, RetireToken, RetirePayload)`, or expose the mode on the payload) plus a
  test leg asserting the two keys' results differ; the frozen-format brief makes this the moment
  to get the seam right, since three later slices are built against it.
- **NEEDS-HUMAN [human] — the gating `C4-ci` red is real but not this diff's**: the only failure
  in the frozen run is `error[vulnerability]: h2 unbounded empty DATA frames` /
  RUSTSEC-2026-0258 (`gate-logs/C4-ci.log:2859`, `:5244`, tail `advisories FAILED, bans ok,
  licenses ok, sources ok`). I checked the whole log: fmt/clippy/build/test/conformance are
  green, no `test result: FAILED`, and the new suite's 27 tests pass in that same run
  (`gate-logs/C4-ci.log:1017`). It is a transitive `h2` (tonic/hyper) advisory, untouched by a
  patch that adds no dependency. Still a merge blocker needing a bump or a `deny.toml`
  exception — a supply-chain scope call this bundle cannot make, so it must land as an explicit
  sign-off decision rather than be inherited a fifth time.

## Refutations attempted and failed

- **The red→green substitute holds.** C4-verify is `unverifiable` because the reverted-production
  leg fails to *compile* (`gate-logs/C4-verify.log:16-30`) — pre-declared born-at-tier. I ran my
  own negation battery instead: `PartNumberSet::from_runs`'s empty / reversed / non-coalesced /
  endpoint-range checks (`multipart.rs:2427-2447`), `checked_chunks`' empty-list and
  `checked_chunk_scheme` legs (`:2596-2609`), `RetireGeneration`'s neither-source rule
  (`:2701-2705`), `checked_shape` (`:2985-2992`), `Component::checked_mode` (`:3061-3070`), each
  of the three `checked_scope` arms (`:3075-3096`), the all-without-session rule (`:3031`), the
  generation-identity and seg-epoch relations (`:3038-3055`) and `require_canonical` (`:3156`).
  **Every one is load-bearing and 15 of 16 fail exactly one test**; the sixteenth (removing
  `skip_serializing_if = "is_absent"`, `:2949-2950`) fails 13, which is the R9 property behaving
  as advertised. No dead check, no test passing for the wrong reason.
- **No accepted-but-unsafe shape found.** Probed the full mode × token-scope × component product
  plus: real 128-bit `ChunkId`s and `u64::MAX` `DServerId`s (round-trip exactly), `scheme:"None"`,
  duplicate JSON fields (`duplicate field` rejection), trailing whitespace (`Noncanonical`),
  `"session":false` / `"segments":null` (rejected), 200-run part sets, epoch `0` and `u64::MAX`,
  `version: u64::MAX`, `{session}+{chunks}` and `{parts}+{seg}` under each key. Every verdict was
  the documented one; every one of the eight new `RecordError` Display strings renders and names
  its own rule (they are the 32 uncovered lines behind the 87.5% diff-cov, but the sibling
  value-record suites assert no Display text either, so that is repo-consistent, not a gap).
- **The `session`-under-`retire:records:` refusal (`:2735-2757`) is not a defect**, though
  `0016:356`'s value column literally spells `{session, parts}` there: the row's own prose and
  every writer row give that namespace only the published parts' records and one rolled-back
  attempt's segments, brief R1 enumerates the records-mode shapes without `session`, and the
  code records the resolution citing `:356` as the brief instructed. Left as an observation, not
  a finding.
- **R9's "file-wide" identity assertion is a tautology while `require_canonical` stands** — but
  the test header now says so in its own words (`crates/core/tests/multipart_retire_obligation.rs:22-37`)
  and my N14 negation confirms the pinning leg (`a_foreign_spelling_of_an_accepted_payload_is_rejected`)
  is the one that goes red when the gate is dropped. Previously-raised, now honestly bounded.
- Also attempted without success: an unbounded run count or a `parts` cardinality escape (`0016:390-414`
  mandates format maxima only, and none applies), a non-canonical key spelling smuggling a second
  key for one obligation (`parse_retire_key` is base code and strict), and an over-strict rejection
  of any writer row in `0016:657-673`, `:2187-2194`, `:2417` — all eleven install shapes decode.
