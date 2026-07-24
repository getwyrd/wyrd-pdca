# Design proposal — issue 509 / delete-objects-bulk

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.

- **Slug:** delete-objects-bulk
- **Kind:** enhancement (design proposal)
- **Goal:** bulk `DeleteObjects` — `POST /bucket?delete` with an XML body of keys —
  deletes each named key and returns the S3 `<DeleteResult>` (per-key
  `<Deleted>`/`<Error>`, honoring `Quiet`), instead of today's 400 `InvalidRequest`.
  Unblocks `aws s3 rm --recursive` and `aws s3 sync --delete`.
- **Success criterion:** against the in-process loopback S3 gateway with several objects
  stored: a stock `aws-sdk-s3` `delete_objects()` call (signed `POST /bucket?delete`,
  XML body listing N keys, some present and at least one absent) returns 200 with a
  well-formed `<DeleteResult>` naming each requested key exactly once — `<Deleted>` for
  removed AND absent keys (S3 delete is idempotent) — and subsequent GETs of the deleted
  keys answer 404 `NoSuchKey`; with `Quiet=true` the result omits `<Deleted>` entries; a malformed
  XML body answers 400 `MalformedXML`; a request naming more than 1000 keys is refused;
  an entity-escaped key (`&amp;` etc.) deletes the right object. Asserted by
  `crates/server/tests/s3_delete_objects.rs`, red on the wave base (bucket-only POST →
  400). The test drives the wire only (SDK/HTTP; no new production symbol imported), so
  the C4-verify red leg fails by assertion, not compile error.
- **Falsifiability:** RED is producible in-process: on the wave base a bucket-only path
  fails `split_bucket_key` (`crates/gateway-s3/src/lib.rs:371-377`) and answers 400
  (`lib.rs:788-795`) before the `delete` subresource (denylisted at `lib.rs:342`, guard
  at `lib.rs:813-820`) is even consulted — so the new test's bulk delete fails every
  assertion. This bundle is wave≥1 (depends on 507); `run-verify.sh` honours the driver's
  `$PDCA_VERIFY_BASE` with the right precedence (`_resolve_base_ref`,
  engine/scripts/run-verify.sh:186-192), so red/green run against the folded base that
  CONTAINS 507's routing split — the guard this slice extends.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 507
- **Conflicts with:** 510
- **Ordering note:** depends on 507 because both restructure the same dispatch: 507
  introduces the bucket-scoped vs object-scoped routing split in
  `crates/gateway-s3/src/lib.rs` that this slice's `POST /bucket?delete` route plugs
  into — building this first would re-invent (and then collide with) that split. 508 is
  sequenced AFTER this bundle (508 declares `Depends on: 507, 509, 510`), so this slice
  builds on 507's split exactly as designed; wave order [507] → [509] → [510] → [508].
  Conflict edge with 510: same dispatch file.
- **Difficulty:** medium
- **Scope:** route bucket-scoped `POST /bucket?delete` to a bulk handler; parse the
  `<Delete><Object><Key>…</Key></Object>…<Quiet>` XML body with a minimal in-crate parser
  scoped to exactly this schema (no new dependency); percent-decoding/XML-unescaping of
  keys consistent with the object path (`lib.rs:801-805`); delete each key via the
  existing single-object path (`ObjectGateway::delete_object` —
  `crates/gateway-core/src/lib.rs:176`, impl `crates/server/src/lib.rs:426` — dirent+inode
  CAS unlink + orphan grace record, already idempotent: `Ok(false)` for absent); emit
  `<DeleteResult>` honoring `Quiet`; map a per-key gateway error to a per-key `<Error>`
  (code + message) rather than failing the whole request; bound the request body
  (S3 caps DeleteObjects at 1000 keys — refuse more with `MalformedXML`/400-class, and
  bound the buffered body bytes). / out of scope: bucket-existence precondition on delete
  (`NoSuchBucket`, ADR-0046 decision 4 — lands with #511); `Content-MD5` enforcement
  (verify what the stock SDK sends; accept-and-ignore at Alpha is acceptable — state it in
  build-notes); versioned deletes (`<VersionId>`); any change to single-object DELETE.
- **External dependencies:** none — base toolchain; in-process test on the loopback stack
  (aws-sdk-s3 is already a `crates/server` dev-dependency). Off-Check manual acceptance
  (`aws s3 rm --recursive`, `aws s3 sync --delete`) uses the AWS CLI, registered as doctor
  row "aws cli (S3 gateway round-trip)".
- **Test file:** crates/server/tests/s3_delete_objects.rs   (NEW `*/tests/*.rs` file —
  `run-verify.sh --classify` dry-run confirms an added `crates/server/tests/*.rs` is the
  red→green discriminator)
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites: the SDK client harness `crates/server/tests/s3_gateway_cluster.rs:98`
  (`sdk_client` — its `delete_objects()` builds the signed POST + XML body for real) and
  the loopback stack `crates/server/tests/s3_object_metadata.rs:43-71`; the per-key delete
  to reuse is the DELETE arm `crates/gateway-s3/src/lib.rs:1007-1011` →
  `crates/server/src/lib.rs:426`; the routing split to plug into is 507's (cited in its
  own bundle; on this wave's base it is already present).
- **Disposition hint:** new-feature

## Motivation

Without bulk delete, `aws s3 rm --recursive` and `aws s3 sync --delete` degrade to
one-request-per-object at best (and today fail outright). P1 of the 0.1-Alpha S3 epic.

## Design

The single-object delete path already provides the correct per-key semantics (CAS unlink,
orphan grace, idempotency); this slice is a wire-level fan-out plus XML in/out. The one
real design point is the XML **parser**: the workspace has no XML crate, and adding one is
a human-gated dependency/license decision (ADR-0003 three-test audit + deny.toml) — so
parse the tiny fixed `<Delete>` schema with a small, well-tested in-crate routine
(handle XML entity unescaping in keys, ignore unknown elements, reject malformed input
with S3's `MalformedXML`). Body handling: the POST body is SigV4-signed
(`PayloadHash::Signed` — verify against the signed digest as the PUT path does,
`lib.rs:858-902` peer) and small (≤1000 keys), so buffering it whole — unlike object
bodies — is acceptable; enforce an explicit byte cap so the bound is by construction, not
by trust. Keys execute sequentially at Alpha (correctness first; concurrency is an
optimization the acceptance does not require).

## Alternatives considered

- **quick-xml dependency**: heavier than the schema warrants and pulls the dependency
  audit; revisit when multipart (508) also needs XML parsing — if the maintainer prefers
  one shared XML approach, say so at sign-off.
- **Failing the whole request on the first bad key**: contradicts S3's per-key result
  contract that `sync --delete` relies on.

## Impact & compatibility

Additive routing: `POST /bucket?delete` changes from 400 to a real response; the `delete`
subresource stays refused (501) on OBJECT paths (`PUT/DELETE /b/k?delete` forms are
unchanged). No seam change in gateway-core, no on-disk change, no new dependency.

## Open questions

- Per-key `<Error>` code vocabulary for internal faults (`InternalError` vs mapping
  `GatewayError` variants) — Do decides, reviewer checks.
- (Resolved by adversarial review — not open:) the stock SDK ≥ default-integrity sends
  `x-amz-checksum-crc32` + `x-amz-sdk-checksum-algorithm` HEADERS on `delete_objects`,
  not `Content-MD5`; they ride inside `SignedHeaders` and the denylist is query-key-only
  (`crates/gateway-s3/src/lib.rs:361-365`), so auth passes — the gateway may
  accept-and-ignore them at Alpha (state so in build-notes).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus independent fmt/clippy is sufficient without rerunning the aggregate gate — all 5 wire tests changed from failing to passing at `crates/server/tests/s3_delete_objects.rs:163`, but the asserted `./engine/xtask.sh ci` script was absent from the supplied target/clone, so its full checks remain provisional.; T4 Contribution — Decide whether the available prior-art search is complete — affected-path `git log --all` found merged history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but available refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy is sufficient without the asserted aggregate wrapper — the wire suite changed from 0/5 to 5/5 at `crates/server/tests/s3_delete_objects.rs:163`, but `./engine/xtask.sh` and `./engine/scripts/run-verify.sh` are absent from the supplied target, so the reported aggregate gate cannot be independently rerun.; T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found existing gateway history and no history for the new test, but the available local refs cannot establish closed/rejected work, which matters for avoiding a duplicate contribution at `crates/gateway-s3/src/lib.rs:1`.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed focused 0/5→5/5 plus clean fmt/clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` and red→green wrapper are absent from the target, so their broader coverage could not be reproduced; the focused green begins at `crates/server/tests/s3_delete_objects.rs:208`.; T4 Contribution — Decide whether merged-history-only prior-art coverage is sufficient — affected-path `git log --all` found history for `crates/gateway-s3/src/lib.rs:1` and none for the new test, but local refs cannot establish closed/rejected work, which matters for avoiding duplicate contribution.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Reviewer-confirmed parser bug (verified in patch.diff): `parse_delete` in crates/gateway-s3/src/lib.rs accepts multi-root documents — after the first `<Delete>` root closes, the stack is empty, so a second `<Delete>` root passes the root check and its keys are deleted (e.g. `<Delete></Delete><Delete><Object><Key>victim</Key></Object></Delete>` executes instead of 400 MalformedXML); non-whitespace text outside the root is silently discarded instead of rejected. Fix: make root completion terminal — any token after the root element closes (a second root, or non-whitespace text) is MalformedXML. Extend the malformed-body wire test with multi-root and trailing-content cases so the red→green gate covers this. Do not change the overall approach (in-crate parser, routing, per-key semantics are fine); §6 C4 and T4 were cleared by the human — only the strict-document validation gap blocks acceptance.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the §5 reviewer findings (C3/C5/T2/T3/T5): the hand tokenizer applies first-whitespace-token reduction (`local_name`, `split_whitespace().next()`) to END tags, so `</Key garbage>` is accepted as `</Key>` — a malformed document such as `<Delete><Object><Key>victim</Key garbage></Object></Delete>` deletes `victim` instead of answering 400 MalformedXML. Same destructive class as the iteration-4 rejection: a body that must be rejected can still authorize a deletion. Fix: make end-tag lexing strict — an end tag is exactly `</` Name optional-whitespace `>`; any other content after the name is MalformedXML (per the XML ETag production). Review whether start-tag suffixes need the same strictness (start tags may carry attributes, but arbitrary junk like `<Delete garbage>` should not silently pass); prefer validating tag syntax over discarding it, so this stops being a per-iteration whack-a-mole. Extend the malformed-body wire tests with the `</Key garbage>` case asserting 400 + MalformedXML AND that the named key survives, plus matching parser unit tests, so red→green covers it. Do not change the overall approach — in-crate parser (no new dependency), routing, per-key idempotent semantics, byte/key bounds are all fine and reviewer-confirmed.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the advisory review's C3/C5/T2/T3/T5 findings (the reviewer leaf's original failure was infra — a stale codex models cache, since fixed; the review was re-run manually with the same sandbox/contract, verdict table preserved at /tmp/pdca-review-509-manual/check-review.md). Reviewer-confirmed gap, verified in patch.diff: `validate_attributes` (crates/gateway-s3/src/lib.rs, worktree ~:2045-2057) validates each attribute's syntax one-by-one but tracks no attribute names, so a duplicate attribute — e.g. `<Delete x='1' x='2'><Object><Key>victim</Key></Object></Delete>` — is accepted and its keys are deleted, although XML well-formedness (Unique Att Spec) makes the document malformed. Same class as the iteration-4/5 rejections: a body that must answer 400 MalformedXML can still authorize a deletion, so the patch's own stated invariant ("any body not fully validated is MalformedXml and touches no key") is not yet met. Fix: enforce attribute-name uniqueness within a single tag in `validate_attributes` (track seen names per tag; a repeated name is MalformedXml), keeping the validate-don't-discard discipline so this closes the class, not the instance. Extend coverage: a parser unit test rejecting a duplicate-attribute document, and a wire test asserting 400 + MalformedXML AND that the named key survives, so red→green covers it. Do not change the overall approach — the in-crate parser (no new dependency), the POST /bucket?delete interception before the subresource denylist, per-key idempotent semantics, and the byte/key bounds are reviewer-confirmed sound (C1/C2/T1 PASS; the reviewer independently reproduced 0/9 red on the base).
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the advisory review's C3/C5/T2/T3/T5 findings. The Check reviewer leaf originally failed on infra (a downgraded codex / stale models_cache.json, since fixed); the review was re-run manually with the same decorrelated sandbox/contract (build-notes withheld, grounded on $PDCA_TARGET @ 07d0244), verdict table preserved at /var/tmp/pdca/pdca-reviewer-509-manual/sandbox/check-review.md. Reviewer-confirmed AND independently verified in patch.diff: `validate_attributes` accepts an XML-forbidden literal `<` inside an attribute value — it locates the closing quote (`value.find(quote)`) but never rejects a `<` between the quotes — so `<Delete x='<'><Object><Key>victim</Key></Object></Delete>` parses successfully and deletes `victim` instead of answering 400 MalformedXML. This is the FOURTH instance of the same destructive class the iteration-4/5/6 rejections were about (multi-root -> junk-after-tag -> duplicate-attribute -> now `<`-in-attribute-value): a body that MUST be rejected can still authorize a deletion, so the patch's own stated invariant ("any body the parser does not fully validate is MalformedXml and touches no key") is still not met. Fix, per the reviewer: in `validate_attributes` reject a literal `<` between the attribute quotes as MalformedXml (per the XML `AttValue` production, which also forbids a raw `&` that does not begin a valid reference — check that too), keeping the validate-don't-discard discipline. Do NOT stop at this single instance: this is now a recurring whack-a-mole, so audit the WHOLE `AttValue`/tag grammar in one pass and close the remaining siblings, not just `x='<'`. Extend coverage with a parser unit test AND a wire test asserting 400 MalformedXML AND that the named key survives, so red->green covers it. The overall approach is otherwise reviewer-confirmed sound (C1/C2 PASS — reviewer reproduced 1/10 red on the base; T1/T4 PASS): the POST /bucket?delete interception before the subresource denylist, per-key idempotent semantics, and the byte/key bounds are fine — do not re-architect them here. Note: the reviewer could not re-run the aggregate C4 (`engine/xtask.sh ci` / `run-verify.sh` are not in $PDCA_TARGET, so C4 came back NEEDS-HUMAN), but the bundle's own C4-ci gate passed in the driver environment — the blocker is the parser gap, not the gate. (If the next Do pass produces yet another same-class hole, escalate to iterate-plan: four repeats suggest the hand-rolled permissive tokenizer patched hole-by-hole may be the root problem, and Plan should reconsider it — a real well-formedness pass, or revisiting the human-gated XML-crate decision the brief deferred.)
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on a fifth consecutive same-class, destructive defect in the hand-rolled XML scanner: `skip_pi` scans to the next `?>` without validating the PI target (`<? ?>` has no PITarget), so `<? ?><Delete>…<Key>victim</Key>…</Delete>` parses successfully and DELETES the key instead of answering 400 MalformedXML (advisory §5: C3/C5/T2/T3/T5 FAIL, patch.diff crates/gateway-s3/src/lib.rs:715-723). This is not a Do-quality failure this round — the previous four holes (multi-root, junk-after-tag, duplicate-attribute, `<`-in-attribute-value) were each closed correctly. The APPROACH is the root cause: hand-reimplementing XML well-formedness as a growing set of point checks, with completeness proven by adversarial example, on a code path where any missed production destroys data. Each fix closes one production and leaves the next unaudited (this round it was the PI target; the `skip_pi` "scan to terminator, don't validate" shape is the same leniency the earlier rounds were meant to eliminate). Do NOT re-plan another hole-by-hole tokenizer. Plan should reconsider the parser approach per the iteration-7 carry-forward (brief.md:153): either (a) adopt a vetted XML parser — the human-gated dependency decision the brief deferred (ADR-0003 three-test audit + deny.toml) — or (b) restructure so the scanner is a provably total well-formedness pass over a CLOSED production set, not a pile of point checks; and/or (c) shrink the trust surface so parser leniency cannot authorize a destructive delete. The routing split, per-key idempotent semantics, byte/key bounds, and the wire test harness are reviewer-confirmed sound (C1/C2/T1 PASS) — the re-plan is scoped to the XML validation strategy, not the whole feature. Note: C4-verify also failed as stale (patch.diff does not apply on origin/main because 509 sits on 507's routing split); the next attempt must rebase onto the folded base so red→green can actually be reproduced.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: patch.diff does not apply on origin/main — the bundle is stale; rebase Do.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
