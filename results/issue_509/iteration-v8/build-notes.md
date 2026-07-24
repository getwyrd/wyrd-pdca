# Build notes — issue 509 / delete-objects-bulk (iteration 8)

## What this iteration changes vs. the last one — the root-cause fix, not another patch

Iterations 4→7 were each rejected on the **same destructive class**: a body that MUST be
rejected could still authorise a deletion, because the hand-rolled XML tokenizer was
*permissive-and-patched* — it reduced or skipped what it did not understand, and each review
found one more construct it let through (multi-root → junk-after-tag → duplicate-attribute →
`<`/bare-`&` in an attribute value). The iteration-7 sign-off said explicitly: *"If the next Do
pass produces yet another same-class hole, escalate to iterate-plan … a real well-formedness
pass, or revisiting the … XML-crate decision."*

I took the **first option the note names — a real well-formedness pass** — inside the brief's
sanctioned approach (in-crate parser, no new dependency). The permissive tokenizer
(`tokenize_xml` + `validate_attributes` + `split_name` + a lenient `xml_unescape`, plus the
`Vec<XmlEvent>` two-phase pipeline) is **gone**. In its place is a single **`DeleteScanner`**
(`crates/gateway-s3/src/lib.rs`) — a strict, single-pass, character-by-character recursive-descent
scanner (with an *explicit* element stack, so no call-stack recursion) that **validates every XML
production it accepts and fails closed on the first deviation**. The invariant is now structural,
not a growing list of special cases: *anything that is not a well-formed XML document of the fixed
schema is `MalformedXml` and touches no key.*

Why a rewrite rather than a fifth point-patch (showing the cost, per the builder rules): a
point-patch to `validate_attributes` for the `x='<'` case would have been ~4 lines, but it would
have left the *class* open — the very failure mode that got iterations 4–7 rejected one instance
at a time. The rewrite is larger (~600 lines of parser + handler, ~250 of tests) but it closes the
class **by construction**: each grammar rule is one method that consumes exactly what the
production allows. As evidence it is closing the class and not just the reported instance, I
audited the whole grammar in one pass and found + closed a **sibling the review had not yet
reached**: XML requires an end tag's name to match the start tag's **exactly, prefix included**, so
`<s3:Object>…</Object>` is malformed — the old code (and my first draft of the new one) matched only
the local part and would have deleted the key. `DeleteScanner` now stores the raw `Name` per frame
and requires an exact match (`parse_end_tag`, `OpenElement.name`). Test:
`parse_delete_rejects_every_ill_formed_body`, the `s3:Object` / `s3:Delete` cases.

## The grammar the scanner enforces (each rule = one closed class)

- **Document**: `Misc* element Misc*`, exactly one root, root local name `Delete`. Content before
  or after the root that is not white space / comment / PI → `MalformedXml` (closes multi-root and
  leading/trailing-junk — the iter-4 class). `step_outside_root`, `root_seen`.
- **STag / EmptyElemTag**: `'<' Name (S Attribute)* S? ('>' | '/>')`. No white space between `<`
  and the name; an attribute must be `S`-separated (a zero-width skip before a non-`>` char is
  junk-butted-on → error). Closes `<Delete garbage>` (iter-5 class). `parse_start_tag`.
- **Attribute**: `Name Eq AttValue`, with **Unique Att Spec** enforced per tag (a repeated name →
  error; closes the iter-6 duplicate-attribute class). `AttValue` is validated char-by-char: a raw
  `<` is rejected, `&` must begin a valid `Reference`, terminated by the matching quote (closes the
  iter-7 `x='<'` / bare-`&` class). `parse_att_value`. Note this is also *more* correct than the old
  code: a `>` inside a quoted value is now accepted (well-formed XML) instead of naive-splitting the
  tag on the first `>`.
- **ETag**: `'</' Name S? '>'` — only white space after the name (closes `</Key garbage>`, iter-5),
  and the raw name must equal the open element's raw name (closes the prefix-mismatch sibling).
- **Reference**: the five predefined entities + numeric char refs (`&#38;`, `&#x26;`), each `;`-
  terminated and in range; an unknown/unterminated/bare `&` → error. `parse_reference`.
- **CharData**: no raw `<`; `&` → `Reference`; a literal `]]>` is rejected. `step_inside_element`.
- **Comment / PI / CDATA / DOCTYPE**: comments reject an interior `--`; PIs/`<?xml?>` skipped;
  CDATA read as literal text; a **DOCTYPE (or any other `<!`) is rejected** — DTDs are unsupported
  and rejecting them fails closed (no entity-definition / billion-laughs / XXE surface). Stated here
  because it is a deliberate strictness choice, not an omission.
- **DoS bound**: nesting is capped at `MAX_XML_DEPTH = 100` (schema is 3 deep) so a pathological
  body cannot amplify a 2 MiB request into an unbounded element stack; and the scanner uses an
  explicit `Vec` stack, not recursion, so a deep body cannot overflow the call stack.
  Test: `parse_delete_rejects_pathologically_deep_nesting`.

## What is unchanged from iteration 7 (reviewer-confirmed sound, C1/C2/T1 PASS)

The routing and the handler were never the problem and are kept:
- `POST /{bucket}?delete` is intercepted in `dispatch` **before** `split_bucket_key`'s
  `400 InvalidRequest` and before the object-path subresource denylist, via `has_query_key`
  (percent-decoded key match, mirroring `unsupported_subresource`) + `bucket_only_path`.
- `bulk_delete` buffers the SigV4-signed body under an explicit byte cap (`buffer_capped` →
  `MAX_DELETE_BODY_BYTES`, refused as it is read — bound by construction, not by trusting
  `Content-Length`), verifies a `Signed` payload against its `x-amz-content-sha256` exactly as the
  PUT path does, fans out over the existing single-object `delete_object` (idempotent CAS unlink),
  maps a per-key gateway error to a per-key `<Error>` (never failing the batch), and renders
  `<DeleteResult>` honouring `Quiet`. Over-limit (>1000) and empty key lists are `MalformedXML`
  (refused, never truncated).

## Brief-mandated statements

- **`Content-MD5` / checksum headers (brief §Scope + Open-questions):** the stock modern SDK sends
  `x-amz-checksum-crc32` + `x-amz-sdk-checksum-algorithm` **headers** (not `Content-MD5`) on
  `delete_objects`; they ride inside `SignedHeaders`, and the query denylist is query-key-only, so
  SigV4 auth passes and the gateway **accepts-and-ignores** them at Alpha. The body is a normal
  `Signed` payload (small, buffered), so the digest check in `bulk_delete` is the integrity gate.
  Verified by the green real-SDK wire tests (the SDK builds the real signed request).
- **Per-key `<Error>` vocabulary (Open question — Do decides, reviewer checks):** a per-key gateway
  failure is mapped through the shared `classify(&err)` (the same mapping the single-object arms
  use) — e.g. `CommitUnknownResult`/unrecognised → `InternalError`, a conflict → the 409 code, etc.
  This reuses one seam-classification, rather than inventing a second `GatewayError`→code table.
- **Out of scope (unchanged):** bucket-existence precondition (`NoSuchBucket`, #511), versioned
  deletes (`<VersionId>` is parsed-and-ignored), any change to single-object DELETE.

## Refuting my own test (the three forced questions)

- **(a) Genuine red?** Yes — reverted the production change (stashed `crates/gateway-s3/src/lib.rs`,
  kept the test) and re-ran the wire suite: **0 passed, 11 failed**. On the base,
  `POST /bucket?delete` returns `400 InvalidRequest` ("expected a bucket-scoped object path
  /{bucket}/{key}"), so every assertion fails by assertion — the test compiles on the base (it
  imports no new production symbol), matching the brief's falsifiability. With the fix: 11/11 green,
  and 74/74 gateway-s3 unit tests green.
- **(b) Production path?** Yes — the wire test drives a **stock `aws-sdk-s3` client** and raw signed
  HTTP over a real loopback listener into the shipping `S3Gateway` → `dispatch` → `bulk_delete` →
  `parse_delete` → `DeleteScanner`. No mock, copy, or re-implementation. The RED leg's `400
  InvalidRequest` is the real base production answer.
- **(c) Fixture includes the fault?** Yes — every "…and deletes nothing" test **PUTs `victim`
  first** (the object that must survive), then sends the malformed body, then asserts `victim` is
  still GET-able. The fixture contains exactly the object a parser hole would have wrongly deleted,
  so a regression to any earlier hole would flip these red. `assert_rejected_and_keeps_victim`.

## How I verified (runner note)

Focused red→green was run with `cargo test … --test s3_delete_objects` and `--lib` under the Bash
tool's enforced timeout (not an unbounded ad-hoc invocation). The unit under test is import-light
and headless (a normal tokio test + a pure parser), so no GUI/display dependency is pulled at load.
`cargo fmt --check` and `cargo clippy --all-targets -D warnings` are clean on both crates, and the
patch `git apply --check`s cleanly against a tree reset to `origin/main` (the brief's target base),
so it is commit-ready for the target's own hooks. The aggregate `cargo xtask ci` (`C4-ci`) and the
bundle `C4-verify` gate are Check's to re-run; the C4/T4 concerns from iterations 1–3 were already
cleared by the human, and the only standing blocker was the parser class, which this pass closes.

No new external dependency was needed (no NEEDS-HUMAN): the whole change builds and is exercised
in-process on the loopback stack with the already-present `aws-sdk-s3` dev-dependency.
