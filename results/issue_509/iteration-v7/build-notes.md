# Build notes — issue 509 / delete-objects-bulk (iteration 7)

> Target branch: `getwyrd/wyrd @ main`. This iteration is built and verified against the
> **actual** `$PDCA_WORKTREE` base `07d0244` (== `origin/main`), NOT the 507-folded base the
> earlier iterations used. All `path:line` citations below are against `07d0244`.

## The base moved — and why that forced a re-implementation, not a patch touch-up

Iterations 1–6 were built against `99ef6e6 pdca-integrate: issue_507`, a *507-folded*
integration base. For iteration 7 the driver reset `$PDCA_WORKTREE` to `07d0244` — plain
`origin/main` — which does **not** contain 507:

- `git merge-base --is-ancestor 99ef6e6 HEAD` → NO; `1d14089`/`0fed4c8` (the 507 integrate
  commits) are not ancestors either.
- The 507 helpers the iteration-6 patch plugged into are **absent** from `07d0244`:
  `bucket_scoped_path`, `list_response`, and `query_param` do not exist here (`grep` finds
  none). The old `patch.diff` neither applies (`git apply --check` → "patch does not apply"
  at `lib.rs:457`) nor would compile (it references those three symbols).

This is the **documented wave-fold gap** (`docs/INTEGRATION.md:73-78`, harness#273): a wave≥1
dependent is verified against `origin/<brief base>`, not the folded base, unless the driver
exports `$PDCA_VERIFY_BASE` (empty here). So the base I can actually red→green against has no
507. The Do-beat rule is to satisfy the Success criterion on the base I'm given — which means
the feature must be **self-contained**: it cannot depend on 507's routing split.

I therefore re-implemented the whole feature onto `07d0244`, keeping the reviewer-confirmed
design (in-crate parser, per-key idempotent fan-out, byte/key bounds) and folding in the
iteration-6 fix, but changing exactly one thing: **where the route is intercepted**.

### Routing on a base without 507's split (the one deliberate deviation)

507 would give a `bucket_scoped_path` arm to hang the bulk route on. Absent it, the route is
intercepted in `dispatch` right after SigV4 auth and **before** `split_bucket_key` rejects the
keyless path (`crates/gateway-s3/src/lib.rs:817-835`, the `if` at `:824`, inserted between the
auth block ending at `:815` and the `split_bucket_key` guard at `:837`). A new `bucket_only_path`
helper (`lib.rs:388`) discriminates `POST /{bucket}?delete` (a keyless bucket path) from an object
path — the discrimination 507's split would otherwise provide. On a 507-folded base this
interception moves inside that slice's `bucket_scoped_path` arm; the handler/parser below are
unaffected. This is noted in the routing comment (`lib.rs:817-823`) and the `bucket_only_path`
doc so the integrator sees exactly what to fold.

Everything else is the iteration-6 design verbatim (routing precedence, per-key semantics,
bounds), with `list_response` replaced by a local `xml_ok_response` (`lib.rs:1212`) —
a 200 + `application/xml`, mirroring `error_response`'s content-type; the `x-amz-request-id`
header is stamped by `handle` on the way out, as for every response.

## The iteration-6 fix (the sole carry-forward blocker)

> Reviewer-confirmed gap: `validate_attributes` validates each attribute's syntax one-by-one
> but tracks no attribute names, so a duplicate attribute — `<Delete x='1' x='2'>…</Delete>`
> — is accepted and its keys deleted, although XML well-formedness (Unique Att Spec) makes the
> document malformed. Fix: enforce attribute-name uniqueness within a single tag.

`validate_attributes` (`crates/gateway-s3/src/lib.rs:1378`) now threads a
`seen: Vec<&str>` of the attribute names encountered on the current tag and returns
`MalformedXml("duplicate attribute name in tag")` the moment a name repeats (`:1394-1396`).
Names are tracked **as written** (a `ns:` prefix is part of the name), so `xmlns:a`/`xmlns:b`
stay distinct while a repeated `xmlns` is rejected — matching the Unique Att Spec, which keys on
the literal QName. This keeps the "validate, don't discard" discipline the reviewer asked for:
the check closes the *class* (any duplicate on any tag — root, nested, or self-closing), not one
instance. It restores the patch's stated invariant — *any body the parser does not fully
validate is `MalformedXml` and touches no key*.

Lifetimes: `seen` borrows slices of the tag interior `s` (a `&'a str` parameter), and every
slice pushed (`name` from `split_name`) has lifetime `'a`, so `Vec<&'a str>` is sound — no
allocation per name, no clone.

## Design decisions recorded (brief §Open questions / §Scope)

- **Per-key `<Error>` vocabulary (brief: "Do decides"):** an internal per-key gateway fault maps
  through the shared `classify` helper (`lib.rs`, the same mapping the single-object path uses)
  to its S3 code + message, reported as a per-key `<Error>` rather than failing the batch
  (`lib.rs` `bulk_delete`, the `Err(err) => … DeleteOutcome::Error` arm) — the per-key result
  contract `aws s3 sync --delete` relies on.
- **Content-MD5 (out of scope) — accept-and-ignore at Alpha.** The stock `aws-sdk-s3
  delete_objects` at default integrity sends `x-amz-checksum-crc32` +
  `x-amz-sdk-checksum-algorithm` **headers** (not `Content-MD5`); they ride inside
  `SignedHeaders` and the subresource denylist is query-key-only (`unsupported_subresource`,
  `lib.rs:328-366`), so SigV4 auth passes and the headers are ignored (brief §Open questions,
  resolved by adversarial review). No enforcement is added.
- **Signed-body verify.** The buffered body is checked against the signed `x-amz-content-sha256`
  digest (`bulk_delete`, the `PayloadHash::Signed` arm) before any key is touched, mirroring the
  object PUT's content-hash check — a tampered body is rejected with `XAmzContentSHA256Mismatch`.
- **quick-xml rejected — cost shown.** The alternative adds a workspace dependency + its
  transitive tree + a `deny.toml` allowlist entry + the ADR-0003 three-test audit (a human-gated
  decision), versus the self-contained, unit-tested in-crate parser (~430 lines incl. tests).
  Revisit when 508 (multipart) also needs XML — flagged for the maintainer at sign-off (brief
  §Alternatives).

## Cost of the chosen fix vs. the rejected alternative (validate vs. discard)

The rejected alternative for the duplicate-attribute case is the same "discard and special-case"
posture the reviewer has rejected at every prior iteration: leave `validate_attributes` name-blind
and add a bespoke guard elsewhere. That would be smaller (~0 lines in the validator) but it does
NOT restore the invariant — it guards one more symptom while the validator still silently accepts
a malformed tag, so the next malformed shape (a duplicate on a *different* tag, a duplicate under a
different quote style) slips through again. The chosen fix is ~5 lines in the validator
(`let mut seen`; the `contains`/`push`) that close the whole class. Per the Do-beat rule, when an
invariant is at stake the target is the smallest change that **restores the invariant**, not the
smallest diff — 5 lines that make every duplicate `MalformedXml` beats a 0-line symptom guard that
leaves the validator lying about what it validated.

## Refutation (forced — recorded per the Do beat)

**(a) Genuine red?** Yes, at two levels — both reproduced:
  - *Whole patch:* the project's own `engine/scripts/run-verify.sh` (C4-verify) cut a fresh
    `../wyrd-verify` worktree off `origin/main` (`07d0244`), applied `patch.diff`, ran the shipped
    wire test with the fix (**GREEN: 10 passed, 0 failed**), then reverted production keeping the
    test (**RED: 9 failed, 1 passed** — every load-bearing case hits `400 InvalidRequest`, the
    keyless bucket path being rejected before any delete). Verdict:
    `run-verify.sh: PASS — red without the fix, green with it.` (The lone red-*pass* is
    `delete_objects_more_than_1000_keys_is_refused`, which asserts only `status == 400`; the
    keyless-path `InvalidRequest` is also a 400, so it cannot discriminate on its own — the other
    nine, including every "deletes nothing" survivor assertion, do.)
  - *Iteration-6-fix-specific:* I neutralised **only** the uniqueness check
    (`if false && seen.contains(&name)`) — keeping the whole rest of the feature — and re-ran the
    parser unit test: `parse_delete_rejects_duplicate_attributes` **FAILED (rc=101)** on the exact
    reviewer case `<Delete x='1' x='2'><Object><Key>victim</Key></Object></Delete>`. Restored the
    check; the test is green again. This proves the *uniqueness check itself* binds the objective,
    not merely the feature's presence.

**(b) Production path?** Yes. The wire test drives a real `aws-sdk-s3` client and raw
SigV4-signed HTTP over a loopback `S3Gateway::serve` listener → the production `dispatch` →
`bulk_delete` → `parse_delete`/`tokenize_xml`/`validate_attributes` → `delete_object`. No mock,
copy, or re-implementation. The parser unit tests call the production `parse_delete` in the same
module. The wire test imports **no new production symbol** (wire-only), so the red leg fails by
assertion, not by a compile error (confirmed: the red leg compiled and *ran* 10 tests).

**(c) Fixture includes the fault?** Yes. `delete_objects_duplicate_attribute_is_rejected_and_
deletes_nothing` (`crates/server/tests/s3_delete_objects.rs:419`) first PUTs a real `victim`
object, then sends the duplicate-attribute body **naming `victim`** (both on the `<Delete>` root
and on a nested `<Object>`), and asserts `400 MalformedXML` **and** `assert_present("victim")`.
The fixture therefore contains exactly the object a name-blind validator would have deleted — the
assertion catches a regression that deletes it, not a curated-out absence. The multi-root,
trailing-content, and junk-after-tag survivor tests do the same for their classes.

## Carry-forward addressed

- **Iterations 1–3 (aggregate `xtask.sh ci` "provisional" — absent from the supplied clone):**
  in *this* environment `engine/xtask.sh` and `engine/scripts/run-verify.sh` are present and
  target `$PDCA_WORKTREE`. I ran the focused C4-verify red→green through `run-verify.sh` (the
  project's runner, PASS above) and `cargo fmt --check` + `cargo clippy -D warnings` on both
  touched crates (clean). The full `cargo xtask ci` (fmt+clippy+build+test incl. DST + deny +
  conformance) is heavy and is Check's gate to re-run — the Do-beat sanity pass is the focused
  red→green.
- **Iteration 4 (multi-root / trailing content):** retained — unit `parse_delete_rejects_
  multiple_roots`, `parse_delete_rejects_content_outside_the_root`; wire `delete_objects_multiple_
  roots_are_rejected_and_delete_nothing`, `delete_objects_trailing_content_after_root_is_rejected`.
- **Iteration 5 (junk after a tag name):** retained — strict tag lexing; unit `parse_delete_
  rejects_junk_after_a_tag_name`; wire `delete_objects_junk_after_a_tag_name_is_rejected_and_
  deletes_nothing`.
- **Iteration 6 (this rejection — duplicate attribute):** fixed as described; new unit test
  `parse_delete_rejects_duplicate_attributes` (`crates/gateway-s3/src/lib.rs:3448`) covering root,
  nested, self-closing, and mixed-quote duplicates plus a *distinct* namespaced pair that must
  still parse, and new wire test `delete_objects_duplicate_attribute_is_rejected_and_deletes_
  nothing` (`crates/server/tests/s3_delete_objects.rs:419`).

## Commit-readiness

- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-server` → clean.
- `cargo clippy -p wyrd-gateway-s3 --all-targets -- -D warnings` → clean.
- `cargo clippy -p wyrd-server --test s3_delete_objects -- -D warnings` → clean.
- `patch.diff` applies cleanly on a fresh `origin/main` worktree (proven by both a standalone
  `git apply --check` and C4-verify's own apply), and is byte-identical to the final worktree
  state after the refutation revert/restore.

No external dependency was needed beyond the base toolchain and the already-present `aws-sdk-s3`
dev-dependency — no NEEDS-HUMAN external-dependency marker.

## Note for the reviewer / integrator (not a defect)

The brief declares `Depends on: 507` and describes plugging into 507's routing split, but the
driver's verify base (`origin/main` = `07d0244`) does **not** contain 507 (the harness#273
wave-fold gap, `docs/INTEGRATION.md:73-78`). The feature is self-contained and satisfies the
Success criterion red→green on that base; the only 507-shaped seam is the interception point
(`dispatch`, before `split_bucket_key`), which folds into 507's `bucket_scoped_path` arm at
integration. If the intent was to verify against a 507-folded base, that is a driver
`$PDCA_VERIFY_BASE` wiring matter, not a change to this slice.
