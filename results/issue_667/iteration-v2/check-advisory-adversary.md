# Adversarial review: issue #667 (multipart routing classifier)

**Result:** the data-loss fix held against every attack. Two defects remain: the required docs update is missing, and an encoded `delete` key breaks the brief's invariant. Neither one loses data.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:137` still says "the S3 wire still refuses every multipart form with `501`". That is now false: `PUT /b/k?partNumber=1` answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:2035`, table row in `crates/server/tests/s3_multipart_routing.rs`, leg 2). The diff touches no docs, and this is the same gap behind the red T4 gate (`gate-logs/T4-batch-review.log:10`). The unwarranted claim is in `brief.md:94`, "docs (no API operation is added)". The AGENTS.md Docs currency rule also covers a change that *alters* an API operation, and calls it a merge requirement. Fix: correct that sentence in the same PR, stating that well-formed forms answer `501` and ill-formed shapes answer `400`.

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:1972` still detects bulk delete on the raw query (`is_delete_subresource`, `:696`), so one spelling of an implemented form does not reach its route. Probe over the wire: `POST /b?delete` runs the bulk handler, but `POST /b?d%65lete`, which SigV4 signs as the same key, answers `501` naming `delete`. This contradicts two things:
  - the brief's invariant, "a form it does implement is reachable by exactly one route, whatever spelling the client uses" (`brief.md:75-76`), and its scope line naming bulk delete as a reader of the classification (`brief.md:82-83`);
  - the new comment at `lib.rs:1952-1954`, "no spelling of a key reaches a route its plain spelling cannot".

  It fails closed and the base behaves the same, so this is not data loss. The `lib.rs:694` comment records the residual, but no test pins it. Fix: match on `DecodedQuery` here and in `op_label` (`:320`), which must keep mirroring this route, and add a wire row. Otherwise, narrow the comment and record the exception.

## Notes (no action routed)

- `check-gates.json:48` says "9 test(s) ran red", which overstates the result. Only 5 of the 9 failed (`gate-logs/C4-verify.log:107`), and I got the same 5 failed, 4 passed. The red itself is real. On the base, `PUT ?part%4Eumber=1` overwrites `k` and `DELETE ?upload%49d=…` deletes it. Leg 5 is red on the base only because of message wording, where `brief.md:69` predicted green on both trees. The test file header records this.
- `crates/server/tests/multipart_gateway_seam.rs:77-78` is now stale. It cites the removed `unsupported_subresource` and says the 501 names the "FIRST denylisted query key", but these keys have left the denylist. The test still passes because the new message keeps the wording "the `{key}` S3 subresource". This is comment-only and outside the brief's file scope, so #668 can correct it when it changes these routes.
- These different keys still reach the plain verb, and each row overwrote or deleted `k`:
  - `DELETE /b/k?uploadid=U` → `204`
  - `PUT /b/k?partnumber=1&uploadid=U` → `200`
  - `DELETE /b/k?uploadId%20=U`, `?uploadId%00=U` and `?uploadId%FF=U` → `204`
  - `DELETE /b/k?x=1;uploadId=U` → `204`

  I did not file these as refutations. SigV4 signs each one as a key other than `uploadId` or `partNumber`, and the denylist comment (`lib.rs:403-409`) says it is not a ban on all query strings. A human who reads "every spelling" as including case changes would need to widen the brief.

## What I tried and could not break

- **The evidence:** with toolchain 1.96.0, I rebuilt in scratch and got green with the fix (9 of 9). With `lib.rs` reverted to the base, I got 5 failed by assertion and 4 passed. Forty repeat runs had no failures. The test drives the production `serve_s3` over TCP with hand-signed requests, so it does not use a parallel copy of the routing. The harness checks framing strictly, and every row re-checks HEAD, ETag and GET on each seeded object.
- **Percent-encoding:** lowercase hex, a fully encoded key, `upload%49%64`, and double encoding (`%254E` stays a different key) all classify as intended. `sigv4.rs:195` and `lib.rs:3783` use the same decoding rules, so routing and signing agree on what a key is, including the lossy UTF-8 case.
- **`partNumber` values:** `+1`, `%2B1`, `01`, `1%00`, Arabic digit `%D9%A3`, a 20-digit number and `4294967296` all answer `400`. `%31` is accepted as `1`, which is correct after one decode.
- **Paths:** `//b/k`, `/b//k`, `/b/k/` and `/b/` never reached a plain verb or a listing, and `/?uploads` answers `400 InvalidRequest`.
- **Other shapes:**
  - An SDK's `x-id` beside a shape, `uploads=`, and a trailing or doubled `&` still route to the operation.
  - An empty `uploadId` routes to the operation and answers `501`. The brief leaves checking the id to the verb.
  - Repeats across spellings are refused.
  - `HEAD`, `OPTIONS` and `PATCH` with a multipart key answer `400`.
  - A method named `A&B` comes back XML-escaped in the message.
- **Denylist and bulk delete:** `DELETE ?version%49d=v` and `PUT ?%61cl` now answer `501`. `POST /b?delete&part%4Eumber=1` is refused and leaves the objects alone.
- **Gates:** the mutation gate shows 48 caught and none missed (`gate-logs/C5-mutants.log:13`). The largest spans the diff-coverage gate left unscored, the ones I read at `lib.rs:554-580` and `:589-602`, are doc comments and declarations.

<!-- pdca:leaf-complete -->
