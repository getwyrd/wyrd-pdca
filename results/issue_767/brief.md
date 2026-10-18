# Brief — issue 767 / listing-last-modified-second-granularity

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on 2026-10-14. The
> issue's line numbers were taken on an older base; the ones below are current.

- **Slug:** listing-last-modified-second-granularity
- **Track:** alpha
- **Defect:** For one and the same object, a listing and a HEAD report different modification
  times, up to 999 ms apart, for every object whose stored stamp is not on a whole second
  (about 999 in 1000). `render_contents` (`crates/gateway-s3/src/lib.rs:1020-1028`) renders
  `<LastModified>` through `iso8601` (`:624-638`), which keeps `epoch_millis % 1_000` as a
  `.mmm` fraction. GET/HEAD render the `Last-Modified` header through `http_date`
  (`:3186-3212`), which drops the milliseconds, as IMF-fixdate requires (RFC 7231 §7.1.1.1).
  Both read the same stored `modified`: the listing at `crates/server/src/lib.rs:739`, and
  GET/ranged GET/HEAD at `:445`, `:531` and `:626`. Example: a stored `1_700_000_000_750` lists
  as `2023-11-14T22:13:20.750Z` and HEADs as `Tue, 14 Nov 2023 22:13:20 GMT`. A sync tool that
  caches a listing's `LastModified` and then revalidates with HEAD sees a mismatch on every
  object, every pass, and cannot tell a real change from a formatting artefact. The header is
  correct; the listing is exposing precision the header cannot represent.
  **What real S3 emits: whole seconds, checked against the live service on 2026-10-14.** The
  issue asks for this to be verified, not assumed, because it decides between a rendering fix
  and a documentation-only answer. Anonymous listings of two public AWS Open Data buckets in two
  regions returned **3,000 of 3,000** `<LastModified>` values ending `.000Z`, under both
  ListObjectsV2 and v1 ListObjects:
  `noaa-goes16` (us-east-1), `?list-type=2&max-keys=1000&prefix=ABI-L1b-RadC/2024/` → 1,000/1,000;
  `noaa-goes16`, v1 `?max-keys=1000&prefix=GLM-L2-LCFA/2024/` → 1,000/1,000;
  `sentinel-cogs` (us-west-2), `?list-type=2&max-keys=1000&prefix=sentinel-s2-l2a-cogs/10/T/ES/2024/`
  → 1,000/1,000. A HEAD of a listed object agreed to the second: the listing said
  `2024-01-08T00:33:48.000Z` for `sentinel-s2-l2a-cogs/10/T/ES/2024/1/S2A_10TES_20240107_0_L2A/AOT.tif`,
  and HEAD said `Last-Modified: Mon, 08 Jan 2024 00:33:48 GMT`. If S3 kept milliseconds, about
  999 in 1,000 of those values would end in something other than `.000`. So S3 is
  second-granular and its listing and header agree, which picks the issue's first branch: render
  `.000Z`. Anyone can re-run the check with
  `curl -s "https://noaa-goes16.s3.amazonaws.com/?list-type=2&max-keys=1000&prefix=ABI-L1b-RadC/2024/"`.
  (The gateway's comment calling `.mmmZ` "the shape S3 uses", `crates/gateway-s3/src/lib.rs:615-617`,
  is about the format; it never said the fraction is non-zero.)
- **Success criterion:** In a NEW wire-level test over the in-process gateway, take an object
  whose stored `modified` is `1_700_000_000_750` (not on a second boundary):
  (1) ListObjectsV2 **and** v1 ListObjects render its `<LastModified>` as exactly
  `2023-11-14T22:13:20.000Z`;
  (2) HEAD **and** GET carry `Last-Modified: Tue, 14 Nov 2023 22:13:20 GMT`;
  (3) the listing value and the header value, each parsed, name the same instant.
  A second object stored exactly on a second (`1_700_000_000_000`) renders the same on both
  surfaces as it does today. The chosen rule (listings report whole seconds, matching the
  header and S3) is stated in the doc comments of both `iso8601` and `http_date`, and in one
  sentence of the living architecture doc's object-metadata paragraph
  (`docs/design/architecture/05-building-block-view.md:186-200`, where `modified` is described):
  the store keeps `modified` in milliseconds, and every S3 surface reports it at whole seconds.
  That doc line is a merge requirement, because this change alters an API operation's response
  (`AGENTS.md:154-157`, docs currency).
- **Falsifiability:** RED on `origin/main`, in-process, no special environment: (1) and (3) fail
  today (`.750Z` against `:20 GMT`). The test uses only pre-existing API: it seeds through
  `wyrd_core::write` before `Gateway::new`, then sends raw signed HTTP. So C4-verify's reverted
  base compiles it and fails on an assertion, not on a build error.
- **Invariant to restore:** Every S3 surface that reports an object's modification time reports
  the **same instant**, at the one-second granularity of the HTTP `Last-Modified` header (the
  granularity S3's own listings use), for every stored value. That includes the surfaces the
  multipart slices will add, such as ListParts' part `LastModified` (#508). **Sources:** RFC 7231
  §7.1.1.1, IMF-fixdate has one-second resolution (authoritative, for the header); the live S3
  listings and HEAD sampled above, 3,000 of 3,000 values on whole seconds (authoritative, for
  S3's listing behaviour), and the AWS ListObjectsV2 API reference example `…T17:50:30.000Z`
  (corroborating); ADR-0047 `docs/design/adr/0047-object-metadata-model.md:36`, `modified` is
  stored in epoch **milliseconds**. That stays true; this fix changes the rendering, not the
  record. SELF-TEST: this is a rule over every rendering surface, not a patch to one formatter.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 768
- **Ordering note:** Wave 0. No build-on dependency: #768 (ordering across overwrites) stores and
  compares milliseconds, while this slice changes only how the listing renders them.
  `Conflicts with 768`: both now edit the object-metadata paragraph of
  `docs/design/architecture/05-building-block-view.md` (this one a sentence on rendering near
  `:189-194`, #768 the overwrite rule at `:197-200`), so they go in different waves. #656 edits
  the multipart paragraph at `:202`; with #768 already between them in wave order, that is no
  extra edge.
  This slice edits `crates/gateway-s3/src/lib.rs` around `:610-640` and the `http_date` doc at
  `:3176-3186`, and maybe the in-file listing test near `:4590-4630`. #620 and #666 edit other
  regions of the same file (the PUT arm and bulk-delete parser; the generic bounds and test
  doubles), and their hunks do not overlap with these. **Intake-cap override (wyrd-pdca-P1):** the
  human allowed all seven ids of this batch (656, 666, 671, 767, 768, 620, 686) in over the alpha
  cap in the 2026-10-14 Plan session. Count at that moment: alpha 4/6 planned (508, 625, 633,
  654), room 2, need 7.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** how the S3 listing renders an object's `<LastModified>`, and the stated semantics
  at both timestamp formatters / out of scope: the stored precision (`modified` stays epoch
  milliseconds per ADR-0047, and #768's ordering rule relies on it); the conditional-request
  comparison, which already truncates to seconds (`crates/gateway-s3/src/lib.rs:2777`, `:2792`);
  ordering across overwrites (#768); the blackbox validation tool (`crates/validate`); any ADR
  edit (ADR-0047 already says `modified` is stored in milliseconds, and that stays true); any
  `docs/` edit beyond the one architecture-doc sentence in the success criterion.
- **Repro instruction:** On `origin/main`: `iso8601(1_700_000_000_750)` returns
  `"2023-11-14T22:13:20.750Z"` and `http_date(1_700_000_000_750)` returns
  `"Tue, 14 Nov 2023 22:13:20 GMT"` (both private in `crates/gateway-s3/src/lib.rs`). Over the
  wire: seed a bucket marker and one flat object with `plan.object_meta.modified =
  Some(1_700_000_000_750)` before `Gateway::new`. `GET /bucket?list-type=2` shows `.750Z`,
  `HEAD /bucket/key` shows `22:13:20 GMT`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/s3_last_modified_agreement.rs` — a **NEW** file (C4-verify
  treats only an added `crates/*/tests/*.rs` as the per-fix discriminator; an appended or inline
  test degrades to green-only).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer harness to mirror:** seed the store before it moves into `Gateway::new`, as
  `crates/server/tests/s3_list_objects.rs:82-114` does (`seed_bucket` + `start_gateway`). Write
  the object through `wyrd_core::write` as `crates/server/tests/segmented_object_read.rs:55-80`
  does (`write_flat`), setting the plan's `object_meta.modified` field. That is the field the
  gateway stamps at `crates/server/src/lib.rs:251-255`. The dirent name is `"{bucket}/{key}"`
  under `ROOT`, the encoding `crates/server/src/lib.rs:708-712` lists by. A raw signed listing
  request must sign its canonical query (`crates/server/tests/s3_list_objects.rs:34-37`).
- **Prior-art check (triage cycles):** searched by path `crates/gateway-s3/src/lib.rs`
  (`iso8601`, `http_date`, `render_contents`). Both formatters arrived with the ADR-0047 metadata
  model (PR #594, merged 2026-07-18) and the listing (#507). No later change touched the
  granularity. Proposal 0017 (`5682572`) documents the gap and cites #767. Open PRs on
  getwyrd/wyrd: none. Closed or rejected PRs on #767: none.
- **Disposition hint:** likely-fix

Plan-review response: both findings revised in place. (1) The tracker's "verify against the real
service" step is now done, not waived: 3,000 of 3,000 live S3 listing values (two public buckets,
two regions, v1 and v2) were on whole seconds, and a HEAD agreed to the second, so the
rendering fix is the issue's own first branch, chosen on evidence. (2) Scope now includes one
sentence in the architecture doc's object-metadata paragraph (`AGENTS.md:154-157`); that adds
`Conflicts with: 768`, since #768 edits the same paragraph.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the outstanding baseline advisory-gate disposition and obtain a complete CI result on a writable runner — independent red→green passed, but frozen CI rejects unchanged Rustls dependencies and local CI stops on a read-only advisory lock; `reviewer-green.log:13`, `gate-logs/C4-ci.log:6472`, `reviewer-ci.log:3264`, `Cargo.lock:3294`.; T4 Contribution — Confirm merged and closed/rejected prior art by all affected file paths — the brief records the gateway path and issue-specific PR search, while the supplied one-commit snapshot cannot establish coverage for the new test and architecture path; `brief.md:112`, `reviewer-target.log:2`.; T5 Judgment — Correct the response reader's false-success cases and bound its socket operations — the new wire oracle accepts incomplete/malformed responses and can hang, contrary to the standing rubric; `crates/server/tests/s3_last_modified_agreement.rs:229`, `crates/server/tests/s3_last_modified_agreement.rs:250`, `crates/server/tests/s3_last_modified_agreement.rs:209`, `reviewer-framing.log:1`, `reviewer-socket.log:1`.; `docs/design/architecture/05-building-block-view.md:190-194`: the new sentence went in between the list of the three fields and "Each is optional with a serde default *and* skipped when absent". "Each" now follows a sentence whose subjects are "the `Last-Modified` header and a listing's `<LastModified>`", so it reads as if the two wire surfaces are "optional with a serde default". Fix: move the new sentence after the serde/CAS sentence (after `:197`), or write "Each of the three fields is optional…". Also re-wrap the stub line `:194`.; `crates/server/tests/s3_last_modified_agreement.rs:209`: The new socket exchange has no deadline around connect, write, or `read_to_end`. If the gateway stalls or leaves the connection open, either integration test can hang indefinitely instead of reporting a failure. Bound the complete exchange with `tokio::time::timeout`, as required by the await-discipline rubric.; `crates/server/tests/s3_last_modified_agreement.rs:250`: The new response decoder treats invalid chunk sizes as termination, returns accumulated bytes when the terminal chunk is missing, and skips chunk terminators without validating them. For example, one complete XML chunk followed by EOF passes the timestamp assertions without the required `0\r\n\r\n`. The non-chunked branch at `crates/server/tests/s3_last_modified_agreement.rs:236` also ignores declared `Content-Length`. Reject incomplete or malformed framing before exposing the body, including length checks with HEAD's bodyless semantics. Otherwise these wire tests can pass responses a conforming client rejects, contrary to the protocol-input rubric.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the baseline Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passed, but frozen CI fails on unchanged dependencies and local CI stops on a read-only advisory lock before later checks; `reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3263`, `Cargo.lock:3294`.; C4-ci — Frozen attempts stop at all-features advisories RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in Rustls 0.21.12 through TiKV; local CI passes through cargo-machete, then cannot lock the advisory database. Neither run reaches the subsequent conformance/statics/deployment-guard/DST sequence; `gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`, `reviewer-ci.log:3257`, `reviewer-ci.log:3263`, `xtask/src/main.rs:1683`.; Optional reuse/simplification at `crates/server/tests/s3_last_modified_agreement.rs:477`: the new calendar conversion and two date parsers duplicate functionality available in `aws-smithy-types`, already a direct dev-dependency (`crates/server/Cargo.toml:134`). Use its `DateTime` parsing for the ISO-8601 and HTTP-date strings while retaining the exact wire-string assertions at `crates/server/tests/s3_last_modified_agreement.rs:842`. This preserves an oracle independent of the production formatter and removes roughly 100 lines of custom date-parsing logic without adding a dependency.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the existing TiKV/Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passes, but frozen CI fails on unchanged dependencies and local CI stops on a host lock restriction (`reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3262`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
