# Design proposal — issue 509 / delete-objects-bulk

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.
>
> **This is a re-plan (iterate-to-Plan after 8 cycles).** The feature's routing, handler,
> per-key idempotent semantics, byte/key bounds, and the whole wire test suite are
> reviewer-confirmed sound (C1/C2/T1 PASS across iterations 4–8). The bundle was rejected
> FIVE consecutive times on the SAME destructive class — a malformed XML body that must
> answer `400 MalformedXML` instead authorised a deletion — because the request body was
> parsed by a **hand-rolled XML tokenizer** patched hole-by-hole (multi-root → junk-after-tag
> → duplicate-attribute → `<`-in-attribute-value → malformed-PI). The v8 sign-off mandated:
> *do not re-plan another hole-by-hole tokenizer.* This re-plan changes **only the XML parse
> strategy**: replace the bespoke scanner with the vetted **`roxmltree`** parser
> (human-confirmed at Plan). Everything else that Check already blessed is retained.

- **Slug:** delete-objects-bulk
- **Kind:** enhancement (design proposal)
- **Goal:** bulk `DeleteObjects` — `POST /bucket?delete` with an XML body of keys —
  deletes each named key and returns the S3 `<DeleteResult>` (per-key
  `<Deleted>`/`<Error>`, honouring `Quiet`), instead of today's `501 NotImplemented`.
  Unblocks `aws s3 rm --recursive` and `aws s3 sync --delete`.
- **Success criterion:** against the in-process loopback S3 gateway with several objects
  stored: a stock `aws-sdk-s3` `delete_objects()` call (signed `POST /bucket?delete`,
  XML body listing N keys, some present and at least one absent) returns 200 with a
  well-formed `<DeleteResult>` naming each requested key exactly once — `<Deleted>` for
  removed AND absent keys (S3 delete is idempotent) — and subsequent GETs of the deleted
  keys answer 404 `NoSuchKey`; with `Quiet=true` the result omits `<Deleted>` entries;
  an entity-escaped key (`a&amp;b`) deletes the right object (`bucket/a&b`), and a key
  whose literal bytes contain a percent-escape (`a%2Fb`, `x%20y`) round-trips **UNCHANGED**
  — the `<Key>` body carries the literal key, so it is only XML-entity-decoded, never
  percent-decoded (a `<Key>a%2Fb</Key>` deletes `bucket/a%2Fb`, not `bucket/a/b`); a request
  naming more than 1000 keys is refused with 400 `MalformedXML`; an oversized body (past the
  buffered byte cap) is refused with 400 `MalformedXML`. **And the load-bearing safety
  invariant** (the one the five rejections were about): **any body that is not a
  well-formed XML `<Delete>` document — a second root, trailing non-whitespace content,
  junk after a tag name (`</Key garbage>`, `<Delete garbage>`), a duplicate attribute
  (`<Delete x='1' x='2'>`), a `<` or bare `&` in an attribute value (`<Delete x='<'>`),
  a malformed processing instruction (`<? ?>`), or any other well-formedness violation —
  answers 400 `MalformedXML` AND deletes nothing** (a rejected request authorises no
  deletion). Asserted by `crates/server/tests/s3_delete_objects.rs` (the retained suite),
  red on `origin/main` because `POST /bucket?delete` is `501 NotImplemented` there. The
  test drives the wire only (SDK / signed HTTP; imports no new production symbol), so the
  C4-verify red leg fails by assertion, not by compile error.
- **Falsifiability:** RED is producible in-process on the shipped base. On `origin/main`
  (507 merged, PR #609 / `f33e6c1`) a bucket-only `POST /bucket?delete` enters 507's
  bucket route (`crates/gateway-s3/src/lib.rs:1457`, `bucket_scoped_path` :427) and hits
  the bucket-route subresource denylist — `"delete"` is listed (`lib.rs:344`), consulted
  at `lib.rs:1466` — so it answers **501 NotImplemented** before any DeleteObjects logic
  runs; every assertion in the retained suite then fails (the reviewer independently
  reproduced **0/11 red** on the base, iteration-v8 §5 C2). The C4-verify gate resolves
  its base from the brief's target — `origin/main` — with the right precedence
  (`run-verify.sh` `_resolve_base_ref`; INTEGRATION §2). The wave/staleness problem that
  failed v8 is resolved AT THE CAUSE — the base now carries 507's split — but **Do must
  build a FRESH `patch.diff` against `origin/main` and MUST NOT reuse
  `iteration-v8/patch.diff`: that patch was built on the PRE-507 dispatch (`bucket_only_path`
  / the old `split_bucket_key` layout) and does NOT apply on `origin/main`** (verified:
  `git apply --check` fails at `crates/gateway-s3/src/lib.rs:365`). Transplant the retained
  handler onto 507's `bucket_scoped_path` route (`lib.rs:1457`). The
  gate classifies red→green on an ADDED `*/tests/*.rs` file (`run-verify.sh:92-93,198`);
  the NEW `crates/server/tests/s3_delete_objects.rs` is the discriminator, carries no
  `#![cfg(...)]` root gate, and compiles on base (shipping symbols only) — so it earns a
  genuine red→green, not the green-only fallback.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** none
- **Conflicts with:** none
- **Ordering note:** single-bundle re-plan against current `origin/main`. The v8
  dependency on 507 is **satisfied** — 507 merged (PR #609, `f33e6c1`), so its
  bucket-scoped routing split is on the base this bundle builds against; no wave fold is
  needed. Informational for any future batch: issue **510** (OPEN) also edits
  `crates/gateway-s3/src/lib.rs` (Range / conditional GET/HEAD), so if 509 and 510 are ever
  co-scheduled they must land in different waves; if 510 merges first, Do rebases (the
  edits are in different handlers — no logical conflict, only a shared file).
- **Difficulty:** medium
- **Scope:** replace the request-body XML parse with **`roxmltree`** and keep everything
  Check already confirmed sound. Concretely: (1) add `roxmltree` as a workspace + `gateway-s3`
  dependency (human-gated — see Impact); (2) route bucket-scoped `POST /bucket?delete` to the
  bulk handler by intercepting it on 507's bucket route BEFORE the `"delete"` subresource
  denylist rejects it (`lib.rs:1457`–`1487`; leave `"delete"` in the OBJECT-path denylist at
  `lib.rs:1513` so `DELETE /b/k?delete` stays 501); (3) buffer the SigV4-signed body under an
  explicit byte cap (a new `buffer_capped`-style helper — refused AS READ, bound by construction
  not by trusting `Content-Length`) and verify the `Signed` payload digest exactly as the PUT
  path does (`lib.rs:1579`; digest-mismatch text `lib.rs:2116`). **Pin `MAX_DELETE_BODY_BYTES` =
  2 MiB**, and hold that bound with a `const` assertion or comment stating the range: the FLOOR is
  a legal max request — 1000 keys × 1024-byte keys + `<Object><Key></Key></Object>` envelope ≈
  1.06 MB, so the cap MUST exceed ~1.1 MB or it fail-closed-rejects a legal DeleteObjects (no test
  guards that lower bound — the 1000-key test uses tiny keys ≈ 33 KB); the CEILING is < 3 MiB so
  the retained oversized-body test (a 3 MiB body) is refused. 2 MiB sits in `[1.1 MB, 3 MiB)`;
  (4) **parse the buffered body
  with `roxmltree::Document::parse` — ANY parse error (and a non-UTF-8 body) ⇒ `MalformedXml`,
  no key touched — then WALK the validated tree; write no XML validation logic of our own.**
  Match elements by **LOCAL name, namespace-insensitively** (roxmltree `node.tag_name().name()`):
  accept a root local-named `Delete` whether it is unqualified (`<Delete>`), carries the S3
  default namespace (`<Delete xmlns="http://s3.amazonaws.com/doc/2006-03-01/">`), or is prefixed
  (`<s3:Delete>`) — this mirrors real S3's leniency and the stock SDK's body; likewise for
  `Object` / `Key` / `Quiet`; (5) enforce the semantic shapes on the parsed tree, all mapping to
  `MalformedXml`: an empty key list, `>1000` `<Object>` elements, an `<Object>` with zero or with
  more than one `<Key>` child, and more than one `<Quiet>`. UNKNOWN sibling elements are ignored
  ONLY as children of `<Delete>` / `<Object>` (S3 is lenient about extras there; `<VersionId>` is
  parsed-and-ignored per out-of-scope) — this leniency does NOT extend inside `<Key>` / `<Quiet>`
  (see (6)); (6) **extract each `<Key>`'s value FAIL-CLOSED. A valid `<Key>` contains ONLY
  character data: iterate its child nodes and REJECT the whole request as `MalformedXml` (delete
  nothing) if ANY child is not a text/CDATA node — i.e. a child element, a COMMENT, or a
  processing-instruction inside `<Key>` (or `<Quiet>`) is malformed, never ignored.** Build the
  key by CONCATENATING the text/CDATA child-node values in document order (roxmltree coalesces an
  adjacent text+CDATA run into ONE node, but concatenating is robust across versions) — do **not**
  rely on `Node::text()` alone: it is first-text-node-only and TRUNCATES a run split by a comment/PI
  (`<Key>a<!--x-->c</Key>` → `.text()` = `a`, not `ac` — which is exactly why such a `<Key>` is
  rejected above; note a CDATA-only key like `<Key>a<![CDATA[b]]>c</Key>` is INERT for this test,
  roxmltree merges it to `abc`). The value roxmltree yields is **already XML-entity-decoded exactly
  ONCE — do NOT re-decode it** (double-decoding `<Key>a&amp;amp;b</Key>`, whose literal key is
  `a&amp;b`, down to `a&b` deletes the WRONG object). It is the literal object key: **do NOT
  percent-decode and do NOT trim whitespace** — `percent_decode_utf8` (`lib.rs:1869`) applies ONLY
  to the URL path/query, so `<Key>a%2Fb</Key>` names the literal key `a%2Fb`, and S3 preserves key
  whitespace. An EMPTY key (`<Key></Key>`) is rejected (`MalformedXml`), never a delete of `bucket/`.
  Ship regression tests for: a literal-`%` key (`a%2Fb` → deletes `a%2Fb`), a nested-entity key
  (`<Key>a&amp;amp;b</Key>` → deletes literal `a&amp;b`, proving NO re-decode), and a comment-split
  key (`<Key>a<!--x-->c</Key>` → 400 `MalformedXml`, victim survives — the true truncation
  discriminator);
  (7) fan out over
  the existing single-object idempotent delete seam (`ObjectGateway::delete_object`, the DELETE
  arm at `lib.rs:1707` — `Ok(false)` for absent), mapping a per-key gateway error to a per-key
  `<Error>` rather than failing the batch; (8) render `<DeleteResult>` by string building with
  `xml_escape`, mirroring 507's `render_list_v2` / `list_response` (`lib.rs:913,1032`, escape
  `:1893`), honouring `Quiet`.
  / **out of scope:** the bespoke tokenizer (`DeleteScanner` / `tokenize_xml` / `validate_attributes`
  and the whole hand-rolled well-formedness pass) — it is DELETED, not extended; any change to
  the routing/handler/bounds/output beyond wiring `roxmltree` in; bucket-existence precondition
  on delete (`NoSuchBucket`, ADR-0046 — lands with #511); `Content-MD5` enforcement (the stock
  SDK sends `x-amz-checksum-*` headers inside `SignedHeaders`, not `Content-MD5`; accept-and-ignore
  at Alpha — state in build-notes); versioned deletes (`<VersionId>`); any change to single-object
  DELETE; and any change to XML *rendering* (string building is the maintainer-blessed output
  choice from 507 — `roxmltree` is adopted for INPUT parsing only).
- **External dependencies:** none for the automated gate — base toolchain; the criterion
  runs in-process on the loopback stack with the already-present `aws-sdk-s3` `crates/server`
  dev-dependency, and `roxmltree` is an ordinary cargo crate resolved at build (not a
  doctor-installable tool/service/topology, so no `[[doctor.checks]]` row). NOTE: adopting the
  `roxmltree` crate is a **human-gated dependency decision** (ADR-0003 three-test audit +
  `deny.toml` allowlist) — Check surfaces it as the project-defined NEEDS-HUMAN "new dependency"
  item (INTEGRATION §4), and the human has pre-approved it at Plan (see Impact). It is NOT a
  new license and pulls NO new transitive crate: current `roxmltree` (≥0.20; use the latest —
  do NOT pin an old `xmlparser`-based `<0.19`) is `MIT OR Apache-2.0`, and pulls no dependency
  that isn't already vendored — `roxmltree 0.20.0` has **zero** transitive deps (adversary probe),
  and any `memchr` an adjacent version uses is already in `Cargo.lock` at `memchr 2.8.2`; the
  license is already on the `deny.toml` allowlist, so `cargo deny check` stays green with no
  allowlist edit. Off-Check manual acceptance (`aws s3 rm --recursive`,
  `aws s3 sync --delete`) uses the AWS CLI (doctor row "aws cli (S3 gateway round-trip)").
- **Test file:** crates/server/tests/s3_delete_objects.rs   (the retained wire suite — a NEW
  `*/tests/*.rs` file, the C4-verify red→green discriminator; a working draft is preserved at
  `results/issue_509/s3_delete_objects.rs`. The draft's malformed-body cases encode FOUR of the
  five rejected classes as `assert_rejected_and_keeps_victim` (multi-root/trailing, junk-after-tag,
  duplicate-attribute, malformed-attribute-value). **Do MUST ADD**, all as regression tests:
  (a) the fifth malformed class — the malformed processing-instruction (`<? ?>`, the iteration-8
  rejection) — as `assert_rejected_and_keeps_victim`; (b) a literal-`%` key (`<Key>a%2Fb</Key>`
  deletes `a%2Fb`, proving no percent-decode); (c) a nested-entity key (`<Key>a&amp;amp;b</Key>`
  deletes the literal key `a&amp;b`, proving roxmltree's decode is NOT re-applied); (d) a
  **comment-split key** (`<Key>a<!--x-->c</Key>` → `assert_rejected_and_keeps_victim` 400
  `MalformedXml`) — this is the TRUE discriminator for the key-extraction fix (a CDATA-only key is
  inert: roxmltree merges text+CDATA into one node, so `.text()` and concat agree — do NOT rely on
  a CDATA case to prove concat-over-`.text()`). These are the permanent regression that the
  parses-but-deletes-wrong class stays closed both in the parser AND in key extraction.)
- **Citations expected:** Do must cite path:line on the target branch (`origin/main`) for
  every change. Peer callsites to mirror:
  - **Routing to plug into (507's bucket route):** `crates/gateway-s3/src/lib.rs:1457`
    (`bucket_scoped_path` split), the method match at `:1475`–`:1487` (add the `POST + ?delete`
    arm here — placed BEFORE the `:1466` denylist so `"delete"` at `:344` doesn't 501 it), the
    bucket-route subresource denylist at `:1466`, the const at `:344`. **Detect `?delete` with a
    bare-key match** (mirror `unsupported_subresource`'s split, `lib.rs:387-393`) — NOT
    `query_param` (`:453`), which does `split_once('=')` and returns `None` for a valueless
    `?delete`.
  - **Output pattern to mirror (string-built XML, no dependency):** `render_list_v2`
    `lib.rs:913`, `list_response` `lib.rs:1032`, `xml_escape` `lib.rs:1893`.
  - **Signed-body handling peer:** the PUT `PayloadHash::Signed` digest check `lib.rs:1579`
    (mismatch text `lib.rs:2116`); `error_response` `lib.rs:1911`; `percent_decode_utf8`
    `lib.rs:1869`; `query_param` `lib.rs:453`.
  - **Per-key delete seam to reuse:** the object DELETE arm `lib.rs:1707`
    (`state.gateway.delete_object(&object_key)` — the existing idempotent CAS unlink).
  - **Test harness peers:** the SDK client `crates/server/tests/s3_gateway_cluster.rs`
    (`sdk_client`) and the loopback + raw-signed-send helpers in
    `crates/server/tests/s3_object_metadata.rs`.
  - **The `roxmltree` API:** use `roxmltree::Document::parse(&str)` with **DEFAULT options** (no
    `allow_dtd` — the default rejects `<!DOCTYPE…>` outright, which is the XXE / billion-laughs
    fail-closed we want; do NOT enable DTDs). Walk `doc.root_element()` and match by LOCAL name
    namespace-insensitively — `root.tag_name().name() == "Delete"` (see Scope (4): accept
    unqualified / S3-default-namespace / prefixed roots); iterate child ELEMENTS whose local name is
    `Object`; for each `Object`'s `Key`, build the key per Scope (6): **reject (`MalformedXml`) any
    `Key` with a non-text/CDATA child** (element / comment / PI), else concatenate its text/CDATA
    child values (`key.children().filter(Node::is_text)…`, document order). roxmltree has
    **already entity-decoded** those values once — do NOT re-decode; do NOT percent-decode. Read an
    optional `Quiet` child. Treat EVERY `Err` (and a non-UTF-8 body, since `parse` takes `&str`) as
    `MalformedXml`. Do should confirm the exact `Err` / node-type / `is_text` API against the docs
    for the pinned version.
- **Disposition hint:** new-feature

## Motivation

Without bulk delete, `aws s3 rm --recursive` and `aws s3 sync --delete` degrade to
one-request-per-object at best, and today fail outright (`501 NotImplemented`). P1 of the
0.1-Alpha S3 completion epic (milestone #16). The feature was fully built and Check-confirmed
correct in every dimension except one — and that one dimension (safe rejection of malformed
XML) is exactly where a hand-rolled parser is most dangerous, because on this code path a
missed grammar production means a body S3 must reject instead deletes objects.

## Design

**What is retained (reviewer-confirmed sound, do not re-architect):** the interception of
`POST /bucket?delete` on 507's bucket route before the subresource denylist; buffering the
SigV4-signed body whole under an explicit byte cap (safe because the body is small — ≤1000
keys — and signed, unlike a streamed object PUT); verifying the `Signed` payload digest as the
PUT path does; fanning out over the existing single-object `delete_object` (idempotent CAS
unlink, `Ok(false)` for absent); mapping a per-key gateway error to a per-key `<Error>` via the
shared `classify` rather than failing the batch; and rendering `<DeleteResult>` by string
building with `xml_escape`, honouring `Quiet`. All of this passed C1/C2/T1 repeatedly.

**The one change — the parser.** The root cause of the five rejections was the *approach*:
hand-reimplementing XML well-formedness as a growing set of point checks, with completeness
"proven" by adversarial example, on a destructive path. Each round closed one production and
left the next unaudited. This re-plan removes the hand-rolled scanner entirely and delegates
well-formedness to **`roxmltree`**, a widely-used, fuzzed, pure-safe (`#![forbid(unsafe_code)]`)
XML-1.0 DOM parser. `roxmltree::Document::parse` validates the ENTIRE grammar by construction —
matched/properly-nested tags, exactly one root, unique attribute names, valid character/entity
references, no raw `<` in attribute values, comment/PI/CDATA grammar — and does NOT expand
external or DTD-defined entities (no XXE / billion-laughs surface). The handler therefore
becomes: buffer (capped) → `Document::parse` → on `Err`, `MalformedXml` (delete nothing) → on
`Ok`, WALK the validated tree for `Delete > Object > Key` and optional `Quiet`. **We write no
XML validation logic**, so there is no next production to miss. The invariant is structural, not
a list: *any body `roxmltree` does not accept as a well-formed document is `MalformedXml` and
touches no key.*

Semantic bounds that are NOT XML-validity (empty key list, >1000 `<Object>`) are still enforced
by us, on the parsed tree, as `MalformedXml` — a well-formed but over-limit body parses fine, so
Do counts `Object` nodes and refuses. The byte cap is enforced during buffering (before parse),
because `roxmltree` needs the whole `&str`. Each key is the `<Key>`'s character-data run as
`roxmltree` yields it — it has ALREADY entity-decoded the predefined + numeric references **once**,
so the key is used verbatim: NOT re-entity-decoded, NOT percent-decoded (unlike the URL-path object
key), NOT whitespace-trimmed. A `<Key>` that is not a clean character-data run (a child element,
comment, or PI inside it) is `MalformedXml` — see Scope (6), the fail-closed key-extraction contract
that closes the parses-but-deletes-wrong class in extraction, downstream of roxmltree's grammar gate.

## Alternatives considered

- **Another total hand-rolled well-formedness pass (v8's `DeleteScanner`, sign-off option b):**
  rejected. Five iterations of "audit the whole grammar this time" each still shipped one more
  missed production; the completeness-by-adversarial-example method does not converge here, and
  the v8 sign-off explicitly forbids re-planning it.
- **`xmlparser` directly (zero new crates):** `xmlparser 0.13.6` is already in `Cargo.lock`
  (via `aws-smithy-xml ← aws-sdk-s3`), so this adds no crate at all. But it is a *tokenizer* — it
  validates token-level well-formedness yet does NOT check document structure (single root, matched
  tags, unique attributes = two of the five rejected classes). It would leave those structural
  checks hand-rolled on the destructive path. `roxmltree` is a full well-formed-XML DOM parser that
  does that structural validation for us, removing ALL hand-rolled logic, for one pre-vetted crate
  (0.20.0 has zero transitive deps). The human chose `roxmltree`.
- **`aws-smithy-xml` (the AWS SDK's own decoder, already transitively present):** a
  decode-oriented pull parser; depending on `aws-smithy-*` in production couples the gateway to
  smithy runtime internals (semver churn) and is a weaker fit than a purpose-built DOM validator.
- **Shrink the trust surface only (sign-off option c):** cannot satisfy the success criterion,
  which REQUIRES answering `400 MalformedXML` to a malformed body — you cannot reject malformed
  XML without validating XML.

## Impact & compatibility

- **New dependency (`roxmltree`) — human-gated, pre-approved at Plan.** Adopting a crate is a
  project-defined NEEDS-HUMAN item (INTEGRATION §4: ADR-0003 three-test audit + `deny.toml`).
  The audit is light and the human approved it at Plan: `roxmltree` is permissive
  (`MIT OR Apache-2.0` — already on the `deny.toml` allowlist), pure-safe, well-maintained and
  widely used; it pulls no dependency not already vendored (`roxmltree 0.20.0` has zero transitive
  deps; a `memchr`-using adjacent version is already in `Cargo.lock` at `memchr 2.8.2`), so
  `cargo deny check licenses` stays green with no allowlist edit — no new license and no new
  transitive crate. Add it to `[workspace.dependencies]` in the root
  `Cargo.toml` and reference it
  `roxmltree.workspace = true` in `crates/gateway-s3/Cargo.toml`, matching the workspace
  convention. `deny.toml` needs no change; Do must run `cargo deny check` and report it clean.
- **Behavioural change, additive:** `POST /bucket?delete` changes from `501 NotImplemented` to a
  real `<DeleteResult>`. The `delete` subresource stays refused on OBJECT paths
  (`PUT/DELETE /b/k?delete` unchanged, `lib.rs:1513`). No gateway-core seam change, no on-disk
  change, no XML *rendering* change.
- **Blast radius (why `medium`):** `crates/gateway-s3/src/lib.rs` (routing + handler + the
  parse-and-walk, replacing the deleted scanner), two manifests (`Cargo.toml` × 2) + `Cargo.lock`,
  and the retained test file. Localized to the gateway; effects do not propagate past it.

## Open questions

- **Per-key `<Error>` vocabulary for internal faults** (`InternalError` vs mapping `GatewayError`
  variants) — reuse the shared `classify` the single-object arms use; Do decides, reviewer checks.
- **Unknown/extra elements in the `<Delete>` body:** S3 is lenient about unrecognised sibling
  elements (it ignores them) while strict about well-formedness. Do should ignore unknown
  elements/namespaces when walking the (already well-formed) tree, and reject only on
  well-formedness (`roxmltree` `Err`) or the semantic bounds — confirm against the stock SDK's
  actual body shape. (`<VersionId>` is parsed-and-ignored, per out-of-scope.)

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Re-plan carry-forward — the parser lesson (read before writing code)

Iterations 4–8 were each rejected on the SAME destructive class: a not-well-formed body that
must answer `400 MalformedXML` still authorised a deletion, because a hand-rolled tokenizer let
one more XML production through each round (multi-root → `</Key garbage>` → duplicate-attribute
→ `<`/bare-`&` in an attribute value → malformed-PI `<? ?>`). **Do NOT hand-roll XML
well-formedness again in any form.** Delegate it to `roxmltree` and walk the validated tree.
The retained wire-suite draft (`s3_delete_objects.rs`) encodes FOUR of the five classes as
`assert_rejected_and_keeps_victim` (multi-root → junk-after-tag → duplicate-attribute →
attribute-value); **Do must ADD the fifth, the malformed-PI `<? ?>` case** (see the Test-file
field), so all five — plus the literal-`%` key case — go green with the `roxmltree` handler and
stand as the permanent regression proving the class is closed by construction. Full prior
attempts are preserved in `iteration-v1/`…`iteration-v8/`.
