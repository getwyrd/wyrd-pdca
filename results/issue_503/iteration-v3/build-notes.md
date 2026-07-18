# Build notes — issue 503 (object-metadata-model), iteration 3

## Scope of this iteration (do NOT redesign — carry-forward is precise)

The Iteration 2 carry-forward is explicit: *"the implementation and design stand — do not
redesign, fix exactly these two."* So this iteration takes the **standing v2 implementation
verbatim** as its base (applied from `iteration-v2/patch.diff`) and adds only the two
targeted deltas the reviewer flagged. Nothing in the v2 data-model / seam / wire / ADR work
was touched except the one GET-arm line that finding 2 names.

Everything the human already settled is left alone: T5/ADR-0047 decisions (SHA-256 opaque
ETag, flat `InodeRecord`), T4 prior-art, and carry-forward item 1 (repair preservation and
its four tests, kept intact).

## The two deltas

### Finding 1 — overwrite-freshness of `modified` was untested (mutant survives)

The v2 wire test `a_second_put_of_new_content_stamps_a_fresh_etag_and_content_type`
(`crates/server/tests/s3_object_metadata.rs`) only *shape*-validates `Last-Modified` (both
PUTs land in the same wall-clock second), and no unit test pinned `modified` freshness. So
regressing `modified: meta.modified` → `modified: prior.modified` at
`crates/core/src/metadata.rs:581` (`commit_chunk_map_superseding`) and `:641`
(`commit_chunk_map_superseding_leased`, the path the wire PUT actually drives via
`write::commit_overwrite`) kept every gate green.

**Fix (test-only):** two unit tests in `crates/core/tests/mutation_regressions.rs`
(import updated at `:15` to bring in `ObjectMeta`):
- `commit_chunk_map_superseding_stamps_a_fresh_modified` (`:398`) — plain path (`:581`).
- `commit_chunk_map_superseding_leased_stamps_a_fresh_modified` (`:449`) — leased path
  (`:641`); seeds a **live** pending lease (`put_pending`, expiry 1000 > now 500) so the
  leased CAS lands.

Each seeds a prior record with a **distinct** `modified` (`1_700_000_000_000`) and commits
an overwrite carrying a different one (`1_700_000_999_999`), then asserts the stored record
carries the **fresh** value — killing the `prior.modified` mutant on both sites. They also
assert the fresh `etag`/`content_type` land, complementing the preservation test at `:342`
(a repair keeps the old trio; an overwrite mints a new one — the two halves of ADR-0047's
"which commits set metadata" split).

No production change was needed here — the v2 code was already correct (`meta.modified`);
the gap was purely a missing binding test, which is exactly what the reviewer asked for.

### Finding 2 — a malformed stored `content_type` panics every GET (production bug)

`ObjectGateway::put_object_streaming` (`crates/gateway-core/src/lib.rs:119-131`) takes the
client's declared content type as an arbitrary `Option<String>` and the server commits it
**verbatim** (`crates/server/src/lib.rs`). The v2 GET arm rendered it straight into
`.header("content-type", <stored string>)`; an invalid HTTP header value (control bytes /
CRLF) makes `Response::builder()` record an `InvalidHeaderValue` error that only surfaces at
`.body(...).expect("streaming response is always valid")`
(`crates/gateway-s3/src/lib.rs:660`) — so **every** GET of such an object panics the handler.
Reachable without any HTTP client: axum rejects a malformed *request* header, but a malformed
*stored* value never passes through axum, and #504 (server-side copy) / #506 (HeadObject) call
this seam next with arbitrary strings.

**Fix (production, smallest change that restores the invariant "a GET never panics on stored
metadata"):**
- New helper `content_type_header(Option<&str>) -> HeaderValue`
  (`crates/gateway-s3/src/lib.rs:729`): returns the stored value **only if**
  `HeaderValue::from_str` accepts it, else falls back to `application/octet-stream` — the same
  fallback an absent type already uses.
- GET arm (`:642`) now calls it. Because the arm now passes an already-parsed `HeaderValue`
  (whose `TryInto<HeaderValue>` is infallible), the content-type header can no longer make the
  builder fail — the panic class is structurally removed, not merely probed.

**Why this over the alternative (enforce/validate at the seam).** The reviewer offered
either. Validating at the seam (`put_object_streaming` rejecting a bad content type) is a
larger blast radius: it would change the seam's contract for #504/#506 (they'd need to handle
a new reject path) and still would not protect a record already written with a bad value.
The wire-layer fallback is ~6 lines, touches one dispatch arm and one helper, protects *all*
readers including pre-existing records, and matches S3's own liberal-read posture (an
un-typeable object still serves, as `application/octet-stream`). Diff cost: the helper (10
lines incl. doc) + a 1-line call-site change, versus a seam signature/contract change rippling
into `gateway-core`, `server`, and every implementer + the two downstream slices.

**Tests:**
- `content_type_header_falls_back_when_the_stored_value_is_not_a_valid_header`
  (`crates/gateway-s3/src/lib.rs:1925`) — the pure fallback logic (valid → verbatim,
  `None` → default, CRLF/NUL → default).
- `a_malformed_stored_content_type_degrades_the_get_instead_of_panicking` (`:2007`) — the
  **binding** test: drives the REAL signed router dispatch → GET arm → response builder with a
  mock gateway (`StoredMetaGateway`, `:1952`) returning an `ObjectRead` whose `content_type`
  is `"text/plain\r\ninjected: header"`, and asserts `200` + `application/octet-stream` (and
  the ETag is still surfaced). This exercises the production `.expect(...)` line, not a copy.

## Refutation (forced, recorded)

- **(a) Genuine red?** YES, verified by reverting each fix and re-running:
  - Freshness: mutating `meta.modified` → `prior.modified` at metadata.rs `:581`/`:641` makes
    both new tests fail (`left: Some(1700000000000), right: Some(1700000999999)`). Restored.
  - Panic: reverting the GET arm to the raw-string `.header(...)` makes the panic test fail
    with the exact production panic — `panicked at crates/gateway-s3/src/lib.rs:660:22:
    streaming response is always valid: http::Error(InvalidHeaderValue)`. Restored.
- **(b) Production path?** YES. The freshness tests call the production
  `metadata::commit_chunk_map_superseding{,_leased}` directly over a real `MetadataStore`
  (`MemMeta`, the backend-agnostic trait store) — the same functions `write::commit_overwrite`
  and the wire PUT drive. The panic test drives the production `dispatch` GET arm through the
  real `S3Gateway` router (signed SigV4 request), hitting the actual `.expect(...)` line the
  fix guards — no re-implementation.
- **(c) Fixture includes the fault?** YES. The freshness fixtures seed a prior record whose
  `modified` genuinely **differs** from the overwrite's (a stale-carry regression is
  detectable, not curated out). The panic fixture returns a `content_type` that is genuinely
  an **invalid** HTTP header value (`"text/plain\r\ninjected: header"`) — the exact fault
  class; a benign value would prove nothing.

## Verification run (fast sanity pass; Check re-runs the real suite)

Targeted runs via cargo in `$PDCA_WORKTREE`, each under an explicit `timeout` (headless,
deterministic — no GUI/IO dependency loaded). The sanctioned whole-tree runner
(`./engine/xtask.sh ci` = `cargo xtask ci`) re-runs everything at Check (`C4-ci`/`C4-verify`);
it was not used for the per-test red→green here because it builds/tests the entire workspace
(and the carry-forward records a `cargo deny` host-lock env flake in it), which is heavier than
a fast sanity pass needs.

- `cargo test -p wyrd-gateway-s3 --lib -- a_malformed_stored_content_type content_type_header_falls_back` → 2 passed.
- `cargo test -p wyrd-core --test mutation_regressions` → 8 passed (v2 preservation tests + 2 new freshness tests).
- `cargo test -p wyrd-server --test s3_object_metadata` → 2 passed (the brief-named wire test, unchanged, still green through the hardened GET arm).
- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-core --all-targets -- -D warnings` → clean.

## Known sign-off routing (expected, not a defect)

ADR-0047 (`docs/design/adr/0047-object-metadata-model.md`, unchanged from v2) is a
project-defined human-only item (INTEGRATION §4): the reviewer routes it to §6 NEEDS-HUMAN;
the maintainer is the accepting authority. This is pre-declared in the brief, not a defect.
