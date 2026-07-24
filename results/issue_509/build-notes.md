# Build notes — issue 509 / delete-objects-bulk (re-plan: roxmltree)

## What this bundle does

Adds bulk **DeleteObjects** (`POST /bucket?delete`) to the S3 gateway, on the post-507 base
(`origin/main` @ `f33e6c1`). The request body is parsed with the vetted **`roxmltree`** XML-1.0
DOM parser — never a hand-rolled well-formedness pass — which is the *only* strategy change this
re-plan makes. Everything Check already blessed (routing interception ahead of the bucket
subresource denylist, capped buffering, signed-digest verify, idempotent per-key delete fan-out,
string-built `<DeleteResult>` honouring `Quiet`) is retained.

### Important: fresh build, nothing to delete

The brief's out-of-scope names deleting the bespoke `DeleteScanner` / `tokenize_xml` /
`validate_attributes`. **Those symbols do not exist on `origin/main`** — they lived only in the
unmerged iteration-v1…v8 branches. Confirmed:
`git grep -n 'DeleteScanner\|tokenize_xml\|delete_objects' f33e6c1 -- crates/gateway-s3/src/lib.rs`
→ *NONE on base*. So this patch **adds** the roxmltree-based handler from scratch; there is no
scanner to remove. `iteration-v8/patch.diff` was NOT reused (it was built on pre-507 dispatch and
does not apply here, per the brief).

## The change, with citations (line numbers are post-patch in the worktree = `origin/main`)

Manifests:
- `Cargo.toml` — `roxmltree = "0.21"` added to `[workspace.dependencies]` (resolves 0.21.1; the
  brief permits ≥0.20 "use the latest, not the `<0.19` xmlparser line").
- `crates/gateway-s3/Cargo.toml` — `roxmltree.workspace = true`.
- `Cargo.lock` — adds `roxmltree 0.21.1` whose ONLY transitive dep is `memchr`, already vendored
  (stayed at `2.8.2`, no bump). No new license surface (diff is +10 lines: the roxmltree package
  stanza + one dep line under `wyrd-gateway-s3`).

`crates/gateway-s3/src/lib.rs`:
- `is_delete_subresource` (**:402**) — bare-key `?delete` detector, mirroring
  `unsupported_subresource`'s split (base `:387-393`). NOT `query_param` (base `:453`), which
  splits on `=` and returns `None` for a valueless `?delete`.
- `MAX_DELETE_BODY_BYTES = 2 MiB` (**:420**) with a `const _: () = assert!(…)` pinning the range
  `(1.1 MB, 3 MiB)`: floor = a legal 1000-key request (~1.06 MB) must fit; ceiling = the retained
  3 MiB oversized-body test must be refused.
- Routing interception (**:1496**) inside the `bucket_scoped_path` branch (base `:1457`), placed
  **before** the subresource denylist (base `:1466`, which lists `"delete"` at base `:344`) so the
  bulk handler runs instead of a `501`. The OBJECT-path denylist (base `:1513`) is untouched, so
  `DELETE /b/k?delete` stays `501` (verified by reasoning: an object path has a non-empty key, so
  `bucket_scoped_path` returns `None` and the request never reaches this interception).
- `buffer_capped` + `BufferError` (**:1770-1804**) — buffers the body, refusing the moment the
  accumulated bytes would exceed the cap (bound by construction, never by trusting
  `Content-Length`).
- `delete_objects` handler (**:1827**) — buffer → verify `Signed` digest **exactly** as the PUT
  path (base `:1579`; mismatch code `XAmzContentSHA256Mismatch` + text from base `:2116`) → UTF-8
  gate → `parse_delete_request` → fan out over `state.gateway.delete_object` (the idempotent seam
  reused from the object DELETE arm, base `:1707`) mapping a per-key error to a per-key `<Error>`
  via the shared `classify` (base `:2065`) → `render_delete_result`.
- `parse_delete_request` (**:1913**) — `roxmltree::Document::parse` with **default options** (DTDs
  rejected → no XXE/billion-laughs). Matches by LOCAL name namespace-insensitively
  (`tag_name().name()`), accepting unqualified / S3-default-namespace / prefixed roots. Any parse
  `Err`, or a semantic-bound violation (root not `Delete`, empty key list, `>1000` `<Object>`,
  `<Object>` with 0 or `>1` `<Key>`, `>1` `<Quiet>`, empty `<Key>`), returns `Err(())` ⇒
  `MalformedXML`.
- `char_data` (**:1968**) — the **fail-closed** key/quiet extraction: `Err(())` if any child of
  `<Key>`/`<Quiet>` is not a text/CDATA node (a child element, comment, or PI). Builds the value by
  **concatenating** the text/CDATA child values in document order — NOT `Node::text()`, which is
  first-text-node-only and truncates a comment-split run. roxmltree has already entity-decoded the
  value once; it is used verbatim (no re-decode, no percent-decode, no trim).
- `render_delete_result` (**:1983**) — string-built `<DeleteResult>` via `xml_escape` (base
  `:1893`), mirroring 507's `render_list_v2` / `list_response` (base `:913`, `:1032`). `Quiet=true`
  omits `<Deleted>`; `<Error>` always emitted.

Test: `crates/server/tests/s3_delete_objects.rs` (NEW `*/tests/*.rs`, the C4-verify discriminator).

## Why roxmltree — the destructive-path argument

The five prior rejections were all the same class: a hand-rolled tokenizer let one more XML
production through each round (multi-root → `</Key garbage>` → duplicate-attribute → `<`/`&` in an
attribute value → malformed-PI `<? ?>`), and a body S3 must reject instead authorised a deletion.
Completeness-by-adversarial-example does not converge on a destructive path. Delegating the WHOLE
grammar to a DOM validator makes the invariant **structural** ("any body roxmltree does not accept
is `MalformedXML` and touches no key"), not a list of point checks — there is no next production to
miss. I confirmed roxmltree's grammar coverage empirically against a scratch build of 0.21.1: all
five classes + `<!DOCTYPE>` + plain text parse `false`; unqualified/namespaced/prefixed `<Delete>`
parse `true`.

Cost of the rejected alternative (another hand-rolled pass, sign-off option b): it is not a
line-count question — the v8 sign-off *forbids* re-planning it, and it cannot restore the invariant
structurally (each round is one more point check, `~1` missed production per iteration, 5/5 so
far). `xmlparser` (zero new crates) is a *tokenizer* only: it validates token-level well-formedness
but NOT document structure (single root, matched tags, unique attributes = 2 of the 5 classes),
which would leave those checks hand-rolled on the destructive path — exactly the thing we are
removing. The human chose roxmltree at Plan.

## Content-MD5 (brief asks to state here)

The stock `aws-sdk-s3` `delete_objects()` (behavior-version-latest) sends its integrity as
`x-amz-content-sha256` (the real body digest — verified here, the `Signed` path) plus flexible
`x-amz-checksum-*` headers inside `SignedHeaders`, NOT a legacy `Content-MD5`. This handler does
not read or require `Content-MD5`; it is accept-and-ignore at Alpha (out of scope). The GREEN leg
proves the real SDK request is accepted, so whatever headers it sends are handled.

## New-dependency NEEDS-HUMAN (project-defined, pre-approved at Plan)

Adopting `roxmltree` is the project-defined NEEDS-HUMAN "new dependency" item (INTEGRATION §4:
ADR-0003 three-test audit + `deny.toml` allowlist). Per the brief the human pre-approved it at
Plan. Evidence gathered here for the sign-off:
- License `MIT OR Apache-2.0` — already on the `deny.toml` allowlist; `cargo deny check` →
  `advisories ok, bans ok, licenses ok, sources ok` with **no** allowlist edit.
- `#![forbid(unsafe_code)]`, widely used + fuzzed XML-1.0 DOM parser.
- Zero NEW transitive crates: its only dep is `memchr`, already at `Cargo.lock` `2.8.2`.

## Verification (project runner)

- `cargo build -p wyrd-gateway-s3` — clean.
- `cargo clippy -p wyrd-gateway-s3 --all-targets` and `cargo clippy -p wyrd-server --test
  s3_delete_objects` — clean (workspace `warnings = "deny"`).
- `cargo fmt` — applied; committable for the target's hooks.
- `cargo deny check` — clean (see above).
- `./engine/scripts/run-verify.sh` (the project's per-fix red→green gate, `PDCA_BUNDLE` set) →
  **"PASS — red without the fix, green with it."** Green leg: 15/15 pass. Red leg (production
  reverted, test kept): 15/15 fail — the base answers `POST /bucket?delete` with
  `501 NotImplemented` (the `delete` bucket subresource), so every assertion fails by assertion,
  not compile error.

## Forced test refutation (recorded)

- **(a) Genuine red?** YES. `run-verify.sh` reverts the production change (lib.rs + both
  Cargo.toml + Cargo.lock) while keeping the new test, and all 15 tests fail — the base is
  `501 NotImplemented` on `POST /bucket?delete`. The test binds the objective, not an adjacent
  proxy.
- **(b) Production path?** YES. The test starts the real `S3Gateway` on a loopback TCP listener
  over the in-process redb+fs stack and drives it with a stock `aws-sdk-s3` client and raw
  SigV4-signed HTTP. It exercises the production `dispatch → delete_objects → parse_delete_request
  → char_data → ObjectGateway::delete_object → render_delete_result` path — no mock, copy, or
  re-implementation. It imports no new production symbol (wire-only), so the red leg fails by
  assertion.
- **(c) Fixture includes the fault?** YES. Each malformed-body test PUTs the `victim` object the
  request names and asserts it SURVIVES the rejection (the fault — the malformed body — is actually
  sent, and the object it would have deleted is present in the fixture). The
  literal-`%` and nested-entity tests store DECOYS (`a/b`, `x y`, `a&b`) that a percent-decode /
  re-decode bug would delete, and assert those decoys survive while the correct literal key is
  gone. The comment-split test stores BOTH the `.text()`-truncation target (`a`) and the
  concat target (`ac`) and asserts both survive.

## Added regression tests (beyond the retained suite)

Per the brief's Test-file field, added as permanent regression:
- `delete_objects_malformed_processing_instruction_is_rejected_and_deletes_nothing` — the 5th
  class (`<? ?>`, iteration-8), leading and trailing forms.
- `delete_objects_literal_percent_key_is_not_percent_decoded` — `<Key>a%2Fb</Key>` /
  `<Key>x%20y</Key>` delete the literal keys; the percent-decoded decoys survive.
- `delete_objects_nested_entity_key_is_decoded_exactly_once` — `<Key>a&amp;amp;b</Key>` deletes the
  literal `a&amp;b`; the `a&b` decoy survives (proving no re-decode).
- `delete_objects_comment_split_key_is_rejected_and_deletes_nothing` — the TRUE key-extraction
  discriminator: `<Key>a<!--x-->c</Key>` ⇒ 400 `MalformedXML`, `a` and `ac` both survive. (A
  CDATA-only key is inert — roxmltree merges text+CDATA to one node — so it is deliberately not
  used to prove concat-over-`.text()`.)
