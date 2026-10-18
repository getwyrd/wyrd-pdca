# Result — issue 767 / listing-last-modified-second-granularity

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: For one and the same object, a listing and a HEAD report different modification
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
- Success criterion: In a NEW wire-level test over the in-process gateway, take an object
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: how the S3 listing renders an object's `<LastModified>`, and the stated semantics
  at both timestamp formatters / out of scope: the stored precision (`modified` stays epoch
  milliseconds per ADR-0047, and #768's ordering rule relies on it); the conditional-request
  comparison, which already truncates to seconds (`crates/gateway-s3/src/lib.rs:2777`, `:2792`);
  ordering across overwrites (#768); the blackbox validation tool (`crates/validate`); any ADR
  edit (ADR-0047 already says `modified` is stored in milliseconds, and that stays true); any
  `docs/` edit beyond the one architecture-doc sentence in the success criterion.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `all-features-advisory-baseline` confirmed absent (detect cmd `timeout 600 bash -c '. ./engine/lib/ensure-cargo.sh && ensure_cargo && cd "${PDCA_WORKTREE:-${WYRD_REPO:-../wyrd}}" && cargo deny --locked --all-features --config deny-all-features.toml check advisories'` exited 1). Install hint: the target base fails the all-features advisory wall before any patch is applied; land an advisory disposition on main (a reviewed `ignore` in deny-all-features.toml, or a TiKV stack bump) so C4-ci can go green, then answer iterate-do. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] External dependency `all-features-advisory-baseline` confirmed absent (detect cmd `timeout 600 bash -c '. ./engine/lib/ensure-cargo.sh && ensure_cargo && cd "${PDCA_WORKTREE:-${WYRD_REPO:-../wyrd}}" && cargo deny --locked --all-features --config deny-all-features.toml check advisories'` exited 1). Install hint: the target base fails the all-features advisory wall before any patch is applied; land an advisory disposition on main (a reviewed `ignore` in deny-all-features.toml, or a TiKV stack bump) so C4-ci can go green, then answer iterate-do. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [x] external dependency: all-features-advisory-baseline — the C4-ci gate (`cargo xtask ci`) stops at `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (`Cargo.lock:3294-3295`, via `tikv-client 0.4.0` → `tonic 0.10.2` → `tokio-rustls 0.24.1`). This patch touches no `Cargo.toml`, `Cargo.lock` or `deny*.toml`, so every branch cut from `7bf7eb9` fails the same way. A green C4-ci needs a disposition on the base (an `ignore` entry with a reason, or a TiKV client stack that drops rustls 0.21), which is a human security decision outside this slice. I could not produce a green C4-ci without it.
- [x] The fix is selected before resolving the tracker’s explicit decision point. `brief.md:22-28` concludes S3 listings and headers “agree by construction” from a `.000Z` example and makes a live check optional; that conclusion also determines the expected output in `brief.md:29-37`. But `notes.json` → `body`, “To confirm before choosing a fix”, says “That should be verified against the real service rather than assumed, because it decides the fix” and explicitly allows a documentation-only resolution if S3 emits sub-second values. The target’s cited “shape S3 uses” comment (`crates/gateway-s3/src/lib.rs:615-617`) accompanies code that retains the millisecond remainder (`:627`, `:636`); it establishes a format, not an always-zero precision contract. The proposed test can prove Wyrd truncates, but cannot establish that truncation is the right compatibility behavior. Revise the brief to supply the requested service evidence or leave the behavioral choice explicitly for human adjudication, without presenting S3 agreement as established.
- [x] The scope excludes documentation that the target repository requires for this behavior change. `brief.md:65-70` rules out “any ADR or `docs/` edit”, while the proposed ListObjects response changes `.750Z` to `.000Z` (`brief.md:29-37`). Target `AGENTS.md:154-157` requires a change that “adds or alters … an API operation” to update the living architecture doc in the same PR. Formatter comments alone do not meet that instruction. Revise the scope to allow the minimal living architecture update for the chosen timestamp semantics; accepted ADRs can remain untouched.
- [x] `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The red is real and gating, but it is not evidence against this fix. It needs its own dependency bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.
- [x] `Cargo.lock:3294-3295` (`rustls 0.21.12`, reached via `tikv-client 0.4.0` → `tonic 0.10.2`): the only gating red, C4-ci, is `cargo deny --all-features … check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (`gate-logs/C4-ci.log:6473-6551`). The default-feature `cargo deny check` passes (`:6470-6471`), and fmt, clippy, build, the full test run (including this file, `:2698-2706`), typos and the docs checks are all green. The diff does not touch `Cargo.lock`, `Cargo.toml` or either `deny*.toml`, so this red is pre-existing on the base and not evidence against the fix. Fixing it means bumping the TiKV client stack or adding an advisory exception, which is outside this slice. A human has to decide whether to accept with the baseline red or wait for a separate dependency fix.
- [x] `gate-logs/C4-ci.log:3472-3550` (the second attempt repeats at `:6474-6551`): the failing C4-ci gate is the baseline advisory RUSTSEC-2026-0346/0349 in `rustls 0.21.12`, pulled in through `tikv-client` (`Cargo.lock:3294-3295`). It is not caused by this diff: the patch doesn't touch `Cargo.lock`, the default `cargo deny check` passes (`:3469-3470`), and typos, docs lint and render, fmt, clippy, build, the full test run and cargo-machete all passed before the stop. The gate never reached the steps after deny, so I ran `cargo xtask statics` and `cargo xtask conformance` on the patched tree in scratch, and both pass. I did not run DST, but `crates/dst/Cargo.toml` depends on neither `gateway-s3` nor `server`. This failure has already triggered two auto-iterate rebuilds (`brief.md:133-141`) that the builder cannot fix, because bumping the dependency or ignoring the advisory is a human-only call (ADR-0003 / `deny.toml`). Decide the advisory disposition; don't send this back to Do again.
- [x] `gate-logs/T4-batch-review.log:6`, `:10`: the gating rubric review reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 22.28 s for "3x codex". Last round the same gate reported 6 blocking. Three model passes over an ~800-line diff in 22 s is fast enough that a cached or empty result is possible. If so, the zero count passes while no review happened (the rubric's "count-based assertion that can pass while the property fails" class). I can't see `results/issue_767/review-batch.md`. Before counting this row as green, confirm it holds three real passes over this patch.
- [x] `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463`: this draft proposal still says a listing "keeps milliseconds" and that the two surfaces differ by up to 999 ms. After this patch that is false. The brief scopes out any other docs edit (`brief.md:91-93`), and the rubric says an out-of-scope finding gets a decline with an issue reference. Decide whether to amend the draft here (drafts aren't frozen) or file a follow-up against #769. The living architecture doc is correct (`docs/design/architecture/05-building-block-view.md:194-196`), and the iteration-1 placement problem is fixed.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #767 follow-up: docs/design/proposals/draft/0017-blackbox-validation-tool.md:460-462 still says listings keep milliseconds and differ from HEAD by up to 999 ms — stale after #767 (listings now whole seconds); amend the draft (#769 is closed).
