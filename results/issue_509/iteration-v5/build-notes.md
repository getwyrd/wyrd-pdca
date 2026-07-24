# Build notes — issue 509 / delete-objects-bulk (iteration 5)

Withheld from the reviewer. Rationale, evidence, and the alternatives considered.

## What this iteration changes (and why it is scoped tightly)

Iterations 1–3 landed the bulk-`DeleteObjects` feature and had **C4 (red→green) and T4
(prior-art) cleared by the human** at Iteration 4. The Iteration-4 carry-forward left
**exactly one** blocker — a reviewer-confirmed parser bug — and told Do explicitly: *"Do
not change the overall approach (in-crate parser, routing, per-key semantics are fine)."*
So this iteration keeps the whole iteration-4 patch and makes the **smallest change that
restores the invariant**: a `<Delete>` DeleteObjects body is a *well-formed XML document*,
which has **exactly one root** and no character data outside it.

The reviewer-confirmed defect (verbatim from the carry-forward):

> `parse_delete` accepts multi-root documents — after the first `<Delete>` root closes,
> the stack is empty, so a second `<Delete>` root passes the root check and its keys are
> deleted (`<Delete></Delete><Delete><Object><Key>victim</Key></Object></Delete>` executes
> instead of 400 MalformedXML); non-whitespace text outside the root is silently discarded
> instead of rejected. Fix: make root completion terminal.

This is a **correctness/security** gap, not a cosmetic one: a request the gateway *rejects*
as malformed must delete nothing, yet the buggy parser silently executed a second root's
keys. That is the invariant to restore.

### The fix (invariant-restoring, ~20 lines in one function)

`crates/gateway-s3/src/lib.rs:2121` `parse_delete` — two guards make root completion terminal:

1. **Second root → `MalformedXml`.** In `XmlEvent::Open` with an empty stack, if the root
   was already seen (`root_seen`), a further top-level element is a second root and is
   rejected before the `== "Delete"` check that previously waved it through
   (`crates/gateway-s3/src/lib.rs:2138`).
2. **Character data outside the root → `MalformedXml`.** `XmlEvent::Text` with an empty stack
   used to be silently dropped (`if let Some(..) = stack.last_mut()`); it now matches on
   `stack.last_mut()` and, when there is no open element, rejects any **non-whitespace** text
   — leading or trailing (`crates/gateway-s3/src/lib.rs:2158`). Insignificant surrounding
   whitespace (an SDK may pretty-print) is still tolerated.

Doc comment on `parse_delete` updated to state the exactly-one-root rule
(`crates/gateway-s3/src/lib.rs:2116`). No other production line changed from iteration 4;
routing (`crates/gateway-s3/src/lib.rs:1484`), the handler
(`crates/gateway-s3/src/lib.rs:1803`), the byte-cap buffer
(`crates/gateway-s3/src/lib.rs:1907`), the tokenizer
(`crates/gateway-s3/src/lib.rs:2008`), entity unescape
(`crates/gateway-s3/src/lib.rs:2074`) and the renderer
(`crates/gateway-s3/src/lib.rs:1938`) are unchanged.

### Test extension (the carry-forward's explicit ask)

Wire test `crates/server/tests/s3_delete_objects.rs` gains the two regression cases the
carry-forward named, each asserting **400 MalformedXML AND that nothing was deleted** (the
load-bearing half — a rejected body must not smuggle keys through):

- `delete_objects_multiple_roots_are_rejected_and_delete_nothing`
  (`crates/server/tests/s3_delete_objects.rs:338`) — PUTs `victim`, POSTs the exact
  `<Delete></Delete><Delete><Object><Key>victim</Key></Object></Delete>` attack, asserts
  400 + `MalformedXML`, then asserts `victim` **still GETs 200** (new `assert_present`
  helper, `crates/server/tests/s3_delete_objects.rs:134`).
- `delete_objects_trailing_content_after_root_is_rejected`
  (`crates/server/tests/s3_delete_objects.rs:364`) — POSTs a valid `<Delete>…</Delete>`
  with `trailing garbage` appended, asserts 400 + `MalformedXML`, and that the named
  present key survived.

Two matching in-crate unit tests were added for the same logic at the parser level:
`parse_delete_rejects_multiple_roots` (`crates/gateway-s3/src/lib.rs:3961`) and
`parse_delete_rejects_content_outside_the_root` (`crates/gateway-s3/src/lib.rs:3977`) — the
latter also pins that surrounding whitespace stays valid.

## Refutation — the three forced questions (evidence, not "yes")

**(a) Genuine red?** Two levels, both recorded:

- *Whole-feature* red→green via the project's own C4 runner
  (`./engine/scripts/run-verify.sh`, `PDCA_VERIFY_BASE` pinned to the 507-folded base
  `99ef6e6` per the wave-fold rule): **"PASS — red without the fix, green with it."** On the
  base the RED leg shows all 8 wire tests fail with `501 NotImplemented` (the `delete`
  subresource denylist — exactly the brief's falsifiability prediction for a wave≥1 base).
- *This iteration's specific fix*: I reverted **only** the `parse_delete` guard (leaving the
  entire handler intact) and re-ran. The 6 pre-existing wire tests **passed**; the 2 new
  wire tests **failed with `200 OK` instead of `400`** (the buggy parser accepted the second
  root / discarded trailing content), and the 2 new unit tests failed likewise. That isolates
  the red to the parser bug, not merely "the feature is absent". Restored the fix → all 8
  wire + all 5 parser unit tests green.

**(b) Production path?** Yes. The wire tests drive a real loopback listener with a stock
`aws-sdk-s3` client / signed raw HTTP through the actual `S3Gateway::serve` →
`bulk_delete` → `parse_delete` path; no production symbol is imported or mocked. The unit
tests call the real `parse_delete` in `crates/gateway-s3/src/lib.rs`. The temporary revert
above toggled the **production** function, not a copy.

**(c) Fixture includes the fault?** Yes. The multi-root fixture contains the *actual*
`<Delete></Delete><Delete>…<Key>victim</Key>…</Delete>` attack and a really-stored `victim`
object; the assertion that `victim` survives is what fails when the fault is present. The
trailing-content fixture contains real appended non-whitespace bytes after the closed root.

## Accept-and-ignore stances the brief asked Do to state

- **`Content-MD5`**: the stock SDK (`aws-sdk-s3`, default integrity) does **not** send
  `Content-MD5` on `DeleteObjects`; it sends `x-amz-checksum-crc32` +
  `x-amz-sdk-checksum-algorithm` **headers** (confirmed by the green wire suite, which uses
  the real SDK). Those ride inside `SignedHeaders`, and the subresource denylist is
  query-key-only, so SigV4 auth passes. The gateway **accepts and ignores** both at Alpha —
  no MD5/CRC enforcement — as the brief permits. Body integrity is still enforced: a
  `Signed` payload is verified against its `x-amz-content-sha256` digest before any key is
  touched (`crates/gateway-s3/src/lib.rs:1815` onward).
- **Out of scope (unchanged from plan):** `NoSuchBucket` precondition (#511), `<VersionId>`
  versioned deletes (ignored per the fixed schema), and any change to single-object DELETE.

## Alternatives considered for the fix

- **quick-xml / a real XML crate** would make single-root/epilog validation free, but adding
  a dependency is a human-gated ADR-0003 audit + `deny.toml` decision, and the plan
  explicitly deferred it ("revisit when multipart (508) also needs XML parsing"). The
  invariant here is restorable in ~20 lines inside the existing in-crate scanner, so pulling
  a dependency to fix a two-branch gap is disproportionate and off-plan.
- **Guarding the symptom at the handler** (e.g. counting `<Delete>` roots via a string scan
  before `parse_delete`) was rejected: it duplicates parsing, is fooled by `<Delete>` inside
  a comment/CDATA/attribute, and leaves `parse_delete` itself still wrong for its unit
  callers. Fixing the cause in the tokenizer-consumer is both smaller and correct — the
  parser is the single place that decides document validity.

## Commit-readiness

- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p wyrd-gateway-s3 --all-targets -- -D warnings` → clean;
  `cargo clippy -p wyrd-server --test s3_delete_objects -- -D warnings` → clean.
- Patch generated with `git diff` against the folded target base
  (`99ef6e6`, `getwyrd/wyrd @ main` + 507); applies cleanly (verified by the C4 verify run,
  which `git apply`s it on a fresh worktree).
- No new dependency, no on-disk change, no gateway-core seam change.

## Note on the recurring C4/T4 sign-off caveat (iterations 1–4)

Earlier rounds' provisional sign-off cited that `engine/xtask.sh` / `run-verify.sh` were
"absent from the supplied target/clone" — that is a property of the **reviewer's** clone,
not the builder worktree. For this iteration I ran the project's real
`./engine/scripts/run-verify.sh` against the 507-folded base and it reports PASS; the
whole-tree `cargo xtask ci` gate is the human's Check gate. T4 was cleared by the human at
Iteration 4 and the approach is unchanged, so no new prior-art question arises.
