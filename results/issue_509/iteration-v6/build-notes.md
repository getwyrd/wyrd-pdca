# Build notes — issue 509 / delete-objects-bulk (iteration 6)

> Target branch: `getwyrd/wyrd @ main`. Verified against the wave-≥1 folded base
> `99ef6e6 pdca-integrate: issue_507` (the `$PDCA_WORKTREE` HEAD, which contains 507's
> bucket-vs-object routing split). All `path:line` citations below are against that base.

## What this iteration changes (and why it is *only* the parser)

The design (in-crate XML parser, `POST /bucket?delete` routing, per-key idempotent fan-out
over `delete_object`, byte/key bounds) was **reviewer-confirmed sound** across iterations 4–5;
the human cleared §6 C4 and T4 at iteration 4. The **only** blocker carried into this run is the
iteration-5 rejection:

> the hand tokenizer applies first-whitespace-token reduction (`local_name`,
> `split_whitespace().next()`) to END tags, so `</Key garbage>` is accepted as `</Key>` — a
> body such as `<Delete><Object><Key>victim</Key garbage></Object></Delete>` **deletes `victim`
> instead of answering 400 MalformedXML**. … Fix: make end-tag lexing strict … prefer validating
> tag syntax over discarding it, so this stops being a per-iteration whack-a-mole.

So I did not touch the routing or the handler. I replaced the **lenient tag reduction** with a
**strict tag-syntax validator**, closing the whole destructive class the reviewer named — a body
that must be rejected can never authorise a deletion.

### The fix (crates/gateway-s3/src/lib.rs)

Removed the old lenient `local_name` (7 lines: `split_whitespace().next()` + `rsplit_once(':')`)
and replaced the tag branch of `tokenize_xml` with a grammar-validating tokenizer:

- `is_name_start` / `is_name_char` (`:1995`, `:2004`) — the XML `Name` character classes
  (ASCII subset the S3 schema uses).
- `split_name` (`:2011`) — splits a leading valid `Name` off a tag interior and **returns the
  remainder** so the caller can require that only permitted content follows. Returns `None`
  (→ `MalformedXml`) for bytes that are not a `Name`, instead of reducing arbitrary bytes to a
  plausible-looking name (the iteration-5 defect).
- `local_element_name` (`:2029`) — strips a `ns:` prefix and rejects an empty local part (`foo:`).
- `validate_attributes` (`:2045`) — validates a start-tag's attribute list
  (`S Name S? '=' S? AttValue`, quoted value) so junk butted onto a start tag
  (`<Delete garbage>`, `<Object x>`) is `MalformedXml`. Values are still discarded; only the
  **syntax** is validated.
- The rewritten tag branch in `tokenize_xml` (`:2124-2151`): an **end tag** is `</ Name S? >` —
  anything after the name is `MalformedXml("junk after end tag name")` (`:2135`); a **start /
  empty-element tag** is `< Name (S Attribute)* S? /? >` with `validate_attributes` on the tail.
  The interior is no longer `.trim()`-ed, so `< Delete>` (whitespace after `<`) is also rejected.

`parse_delete` (`:2211`) is unchanged from iteration 5 — the terminal-root guard (`root_seen`)
and the "no non-whitespace outside the root" check that resolved the **iteration-4** rejection
are retained.

### Unchanged from iteration 5 (context for the reviewer)

- Routing: `POST /bucket?delete` intercepted before the subresource denylist
  (`:1484`, via `has_query_key` `:466`). The `delete` subresource stays `501` on OBJECT paths.
- Handler `bulk_delete` (`:1803`): buffers the signed body under a byte cap
  (`buffer_capped` `:1907`, `MAX_DELETE_BODY_BYTES`), verifies the `Signed` digest (`:1832`),
  parses, bounds keys to `1..=1000` (`MAX_DELETE_KEYS` `:1760`), fans out to the existing
  single-object `delete_object` (impl `crates/server/src/lib.rs:426`) sequentially, and renders
  `<DeleteResult>` honouring `Quiet` (`render_delete_result` `:1938`).

## Design decisions recorded (brief §Open questions / §Scope)

- **Per-key `<Error>` vocabulary (brief: "Do decides"):** an internal per-key gateway fault maps
  through the shared `classify` helper to its S3 code + message (`:1882`), reported as a per-key
  `<Error>` rather than failing the whole batch — the same mapping the single-object path uses.
- **Content-MD5 (out of scope):** not enforced — **accept-and-ignore** at Alpha. The stock
  `aws-sdk-s3 delete_objects` at default integrity sends `x-amz-checksum-crc32` +
  `x-amz-sdk-checksum-algorithm` **headers** (not `Content-MD5`); they ride inside
  `SignedHeaders` and the subresource denylist is query-key-only, so SigV4 auth passes and the
  headers are ignored (brief §Open questions, resolved by adversarial review).
- **quick-xml rejected — cost shown:** the alternative adds a workspace dependency + its
  transitive tree + a `deny.toml` allowlist entry + the ADR-0003 three-test audit (a human-gated
  decision), versus the ~230-line self-contained, unit-tested in-crate parser. Revisit when 508
  (multipart) also needs XML — flagged for the maintainer at sign-off (brief §Alternatives).

## Cost of the chosen fix vs. the rejected alternative (validate vs. discard)

The rejected alternative is *keep the lenient reduction and special-case each new junk pattern*
— literally the "per-iteration whack-a-mole" the reviewer named: iter-4 patched multi-root,
iter-5 needed `</Key garbage>`, and a discard-based patch would next miss `<Object x>`,
`</Key\tx>`, `<Delete garbage>`, `<foo:>`, … Concretely the chosen fix trades the 7-line lenient
`local_name` for ~90 lines of grammar validators + a ~30-line rewritten tag branch (net ≈ +110
lines), but it restores the **invariant** — *any body the parser does not fully validate is
`MalformedXml` and touches no key* — instead of guarding one more symptom. Per the Do-beat rule,
when an invariant is at stake the target is the smallest change that **restores the invariant**,
not the smallest diff; +110 lines that close the class beats +6 lines that close one instance.

## Refutation (forced — recorded per the Do beat)

**(a) Genuine red?** Yes, at two levels:
  - *Whole patch:* the project's `run-verify.sh` (C4-verify) applies `patch.diff` to a fresh
    worktree at the 507 base, runs the shipped wire test with the fix (GREEN: **9 passed, 0
    failed**), then reverts production keeping the test (RED: **0 passed, 9 failed** — every case
    hits `501 NotImplemented`, the `delete` subresource being denylisted on the bare 507 base).
    Result: `PASS — red without the fix, green with it.`
  - *Strict-fix-specific:* I restored **only** the lenient end-tag reduction (a temporary stub)
    and re-ran the unit test — `parse_delete_rejects_junk_after_a_tag_name` **FAILED** (rc=101) on
    the exact `<Delete><Object><Key>victim</Key garbage></Object></Delete>` case, proving the
    strict validation — not merely the presence of the handler — is what binds the objective.
    Reverted the stub; the test is green again.

**(b) Production path?** Yes. The wire test drives a real `aws-sdk-s3` client and raw SigV4-signed
HTTP over a loopback `S3Gateway::serve` listener → the production `bulk_delete` →
`parse_delete`/`tokenize_xml` → `delete_object`. No mock, copy, or re-implementation. The unit
tests call the production `parse_delete` in the same module. The test imports **no new production
symbol** (wire-only), so the red leg fails by assertion, not by a compile error.

**(c) Fixture includes the fault?** Yes. `delete_objects_junk_after_a_tag_name_is_rejected_and_
deletes_nothing` (`crates/server/tests/s3_delete_objects.rs:389`) first PUTs a real `victim`
object, then sends the malformed body **naming `victim`**, and asserts both `400 MalformedXML`
**and** `assert_present("victim")`. The fixture therefore contains exactly the object a lenient
parser would have deleted — the assertion catches a regression that deletes it, not a curated-out
absence. Both the mangled-end-tag and mangled-start-tag forms are exercised.

## Carry-forward addressed

- **Iterations 1–3 (C4/T4 "provisional" — aggregate `xtask.sh ci` absent from the supplied
  clone):** the human cleared §6 C4 and T4 at iteration 4. In *this* environment the `xtask`
  crate is present and `engine/xtask.sh` targets `$PDCA_WORKTREE`, so the aggregate C4-ci is
  runnable by the Check beat. I ran its shape components — `cargo fmt --check` (clean) and
  `cargo clippy -D warnings` (clean) on both touched crates — plus the focused C4-verify
  red→green against the **correct 507-folded base** (`WYRD_VERIFY_BASE=99ef6e6`), not `origin/main`
  (which lacks 507). The full `cargo xtask ci` (fmt+clippy+build+test incl. DST sweep + deny +
  conformance) is heavy and is Check's gate to re-run.
- **Iteration 4 (multi-root / trailing content):** retained and covered — unit
  `parse_delete_rejects_multiple_roots`, `parse_delete_rejects_content_outside_the_root`; wire
  `delete_objects_multiple_roots_are_rejected_and_delete_nothing`,
  `delete_objects_trailing_content_after_root_is_rejected`.
- **Iteration 5 (this rejection):** fixed as described above; new unit test
  `parse_delete_rejects_junk_after_a_tag_name` (`crates/gateway-s3/src/lib.rs:4086`) and a
  companion `parse_delete_accepts_valid_namespaced_and_attributed_tags` (`:4116`, guards against
  over-strictness rejecting real SDK bodies), plus the wire test above.

## Commit-readiness

- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-server` → clean.
- `cargo clippy -p wyrd-gateway-s3 --all-targets -- -D warnings` → clean.
- `cargo clippy -p wyrd-server --test s3_delete_objects -- -D warnings` → clean.
- `patch.diff` applies cleanly on the base (proven by C4-verify's own `git apply`).

No external dependency was needed beyond the base toolchain and the already-present
`aws-sdk-s3` dev-dependency — no NEEDS-HUMAN external-dependency marker.
