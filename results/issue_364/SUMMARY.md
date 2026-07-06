# Result — issue 364 / s3-http-wire-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: an S3-compatible **HTTP listener** in the gateway role; **bucket-scoped**

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix — a net-new, spec-anchored feature landing behind the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 364 / s3-http-wire-surface (iteration 9)

**Task under review:** give the gateway its first client-facing network endpoint — an S3-compatible HTTP wire surface (bucket-scoped object PUT/GET/DELETE, mandatory SigV4, streaming bodies) over the existing in-process client paths, so the blueprint's day-one S3 round-trip (blueprint:698–699) runs over the wire; this iteration's directed delta (iter-8 carry-forward) closes the `high_water_marks` orphan-ledger blind spot (PUT → DELETE → restart re-mints a still-orphaned chunk id → reclaim destroys the new object) plus its behavioral test.

**Reviewer grounding caveat:** `$PDCA_TARGET` could not be resolved in this sandbox (all environment-variable expansion is permission-blocked), so per the fallback rule every citation below grounds on `patch.diff` alone and I could **not** independently re-run the gates or drive the listener. Both C4 gates are recorded green in `check-gates.json` (gating `C4-ci`: "xtask ci: all checks passed"; `C4-verify`: "red without the fix, green with it"). This is a target-state caveat, not a patch defect, and is **not** presented as a blocking C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Every binding Check criterion (brief.md:49–56) has a load-bearing implementation + test in the artifact: signed byte-identical PUT→GET→DELETE over a real loopback listener (`crates/server/tests/s3_http_wire.rs`, patch.diff:5275), unsigned/bad-sig refused fail-closed before the body is read (patch.diff:5325,5355; `handle()` verifies SigV4 first, gateway-s3/src/lib.rs @ patch.diff:2707–2721), streaming bodies (patch.diff:4607–4841), bucket-scoped keys on the flat namespace (patch.diff:2735–2739); public-TLS green is the pre-declared DEFERRED item for #367 (brief.md:57–62). |
| C2 Reproduction (red pre-fix) | PASS | `run-verify.sh` recorded red-without/green-with (check-gates.json rule `C4-verify`); the iter-8 defect's red is *behavioral by construction*: without the `orphan:` scan, recovery re-mints the deleted object's chunk id (post-DELETE the id appears in no `inode:`/`pending:` key) and the ledger-authorised reclaim then deletes the new object's fragments → GET B fails (test doc + assertions, patch.diff:5703–5798); `restart_without_recover_collides_showing_the_bug` (patch.diff:5675) additionally proves `recover()` is load-bearing, not a no-op. Could not re-run locally (target unreadable — see caveat). |
| C3 Change | PASS | The change matches the iter-8 direction exactly: `high_water_marks` now scans `ORPHAN_PREFIX`, projects the chunk id, takes the max (patch.diff:1365–1375) with the re-mint hazard documented (patch.diff:1326–1336); `Gateway::recover()` is monotone `fetch_max` (patch.diff:4684–4693) and called at the composition root before serving (`cmd_s3`, patch.diff:4592); orphan-key protocol single-sourced in `core::metadata` with a `pub(crate)` re-export so GC and the delete path cannot key-format-drift (patch.diff:1807–1816). |
| C4 Verification (red→green) | PASS | Gating `C4-ci` (fmt/clippy/build/test/deny/conformance) recorded PASS and non-gating `C4-verify` recorded PASS (check-gates.json:33–49); the historical `gateway_lease_expiry.rs` wall-clock flake — cause of two prior spurious reds — is quarantined in-place with a 20s skew allowance that still kills both `+→-` and `+→*` mutants (patch.diff:5045–5071), so the green should now be durable. Reviewer re-run was not possible (target-state caveat above), so the human accept leans on the recorded gate. |
| C5 Causal adequacy | PASS | Each fix removes the cause rather than guarding a symptom: id-collision corruption fixed by recovering durable high-water state (incl. the orphan ledger) instead of probing for conflicts; DELETE/overwrite leaks fixed by writing orphan grace records *in the same atomic commit* that unbinds/supersedes (patch.diff:1182–1201, 1246–1259); GET-during-DELETE truncation fixed by deferring reclaim to GC's reader-safe grace window (patch.diff:4853–4862); slow-upload reclaim fixed by per-chunk lease stamping + renewal (patch.diff:1532–1583). Symptom-guard smell-test: no capability probes or try-and-fall-back guards introduced. Prior contested root-cause items were ratified iter-6/7 and are honoured, not re-litigated. |
| T1 Structure | PASS | The iter-6 ratified crate boundary is implemented as decided: S3 wire layer extracted to `crates/gateway-s3` (architecture §5:132's named crate) generic over the shared `ObjectGateway` seam in neutral `crates/gateway-core` (patch.diff:2550–2562, 2158–2327); concretes named only at the composition root per ADR-0010 (`cmd_s3`, patch.diff:4574–4585); the durability seam (orphan protocol, `high_water_marks`, `stream_write_data`) lives in shared core, where both gateway and custodian consume it. |
| T2 Shape | PASS | Fail-closed shapes throughout: closed sentinel set — unknown `x-amz-content-sha256`/streaming framings refused, never half-accepted (patch.diff:3436–3477); declared chunk size bounded *before* buffering → 400, never a truncated 200 (patch.diff:4213–4222); XML error messages escaped against SignedHeaders markup injection (patch.diff:2853, test :2950); GET declares exact `Content-Length` so a mid-stream fault is detectable truncation (patch.diff:2787–2794). Minor non-blocking observation: `percent_decode_utf8(bucket)` lets a bucket named `a%2Fb` alias key `b/…` under bucket `a` on the flat namespace (patch.diff:2735–2739) — cosmetic at M4's single-credential floor, worth a future ListObjects-era cleanup. |
| T3 Runtime | PASS | Bounded memory end-to-end: streaming PUT holds one chunk + fragments (patch.diff:1585–1682), streaming GET reads over a 4-slot bounded channel (patch.diff:4823–4836), auth precedes any body materialisation (patch.diff:2707–2721, test `unsigned_put_is_refused_before_its_body_is_read` patch.diff:5355); behavioral (not structural) streaming proof via pull-count observation (patch.diff:5801–5808); DELETE retry bounded at 8 so an overwrite storm cannot spin (patch.diff:4866). Workspace test/clippy/deny green on record (C4-ci). |
| T4 Contribution | PASS | Load-bearing, not scaffolding: unblocks #367 (the first-deployment gate cannot run without a network S3 endpoint, brief.md:71–72) and retires the `lib.rs:1–9` "later milestone" marker (patch.diff:4618–4626); real-SDK interop is now *demonstrated* (a stock `aws-sdk-s3` client — its own signer/framer, not the gateway's — round-trips byte-identical and a forged credential gets `InvalidAccessKeyId`, patch.diff:6094–6221), closing the recurring iter-2→5 self-referential-oracle finding. Prior art: iterations v1–v8 preserved per brief; note the artifact is cumulative from base, so the iter-9-only delta vs v8 ("targeted fix, no wire rework") cannot be mechanically isolated from the files given here — the wire-surface content is consistent with the ratified iter-6/7 shape. |
| T5 Judgment | NEEDS-HUMAN | All previously-ratified calls are implemented as decided (gateway-s3 crate ✓, header-only SigV4 ✓, M4 target ✓, aws-sdk-s3 dev-dep ✓, minimal error floor ✓) — but two **new documented residuals on the auth boundary** need explicit ratification at sign-off: (1) trailer variants (`STREAMING-*-TRAILER`) consume-and-ignore the trailer checksum rather than re-validating it (patch.diff:3465–3469, 4247–4252), and (2) `STREAMING-UNSIGNED-PAYLOAD-TRAILER` is accepted framing-only, extending the already-accepted UNSIGNED-PAYLOAD envelope (patch.diff:3475). Decision owed: confirm both sit inside the ratified UNSIGNED-PAYLOAD/TLS-deferral risk envelope for the plaintext-loopback floor, or direct trailer re-validation before #367 exposes the port publicly. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human-by-design: is this floor fit for the first-deployment gate? At Check the listener is plaintext loopback (accepted iter-2/6 posture); the live public-TLS, deployed-host green is observed at #367 with the coordination prerequisite (0015:443–463). Reviewer could not drive the listener (target unreadable) — concrete runnable steps for the human: (1) `cargo test -p wyrd-server --test s3_http_wire` (covers signed round-trip, auth refusal, restart/orphan-ledger recovery, real-SDK interop); (2) live smoke: `cargo run --bin wyrd -- s3 --access-key AKIAIOSFODNN7EXAMPLE --secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-s3` then `aws --endpoint-url http://127.0.0.1:8080 s3api put-object --bucket b --key k --body <file>` / `get-object` / `delete-object` with those creds exported, verifying byte-identity and that an unset/forged credential is refused; (3) kill and restart the process against the same `--data-dir` and re-verify a fresh PUT + old GET (the durability seam this iteration closes). Decision owed: accept the loopback floor and route the remaining live-TLS observation to #367. |

## Notes for §6
- Both NEEDS-HUMAN rows above are decision items, not defects found: the T5 row asks for ratification of two new, documented auth-boundary residuals; the Validation row is the standing fitness call plus the runnable manual-verification steps the reviewer could not execute here.
- No finding in this review grounds a FAIL. The iter-8 directive (orphan-ledger scan + behavioral DELETE→restart red) is implemented and tested; all earlier iteration ratifications are honoured without re-litigation.

### Advisory — adversary

# Adversarial review — issue_364 (iter-9), orphan-ledger high-water-mark fix

Scope of this diff's headline fix: `high_water_marks` now also scans `orphan:`
(`crates/core/src/metadata.rs:615-621`) so `Gateway::recover`
(`crates/server/src/lib.rs:101-109`) bumps `next_chunk` past a deleted object's
still-live orphan chunk id, plus a new red→green test
`restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`
(`crates/server/tests/s3_http_wire.rs:628-707`).

## Findings

- **NEEDS-HUMAN — The RED half of the red→green is synthetic: the test's "reclaim" bypasses
  the production `ReferenceSet::protects` gate, so it demonstrates a data-loss that the real
  custodian GC would NOT produce.** The test drives production `put_object`/`delete_object`/
  `recover`, but then models GC reclaim with a **test-local loop** that unconditionally calls
  `store.delete_fragment(frag)` for every `orphan:` record
  (`crates/server/tests/s3_http_wire.rs:679-685`), asserting it is "exactly the reclaim the
  orphan ledger authorises … gc.rs:152" (`:668-670`). It is **not** exactly that: the real
  GC runs the reference-safety gate *first* — `if referenced.protects(dserver, frag) { …skip }`
  (`crates/custodian/src/gc.rs:124`, built from committed inodes at `gc.rs:100,207-215`) —
  and only reaches `delete_fragment` at `gc.rs:152` for **unreferenced** fragments. In the
  buggy (no-orphan-scan) world the test targets, object B re-mints chunk 1 and **commits** an
  inode referencing it *before* the reclaim runs (`put_object` returns fully committed at
  `:663-665`), so `protects(dserver, {chunk:1,index:0})` is TRUE → the real GC **skips** the
  fragment and `GET B` **succeeds**. The genuine production consequence of the missing
  orphan-scan in that ordering is a **silent, permanent fragment leak** (A's orphan record is
  never reclaimable), not the "GET B fails / data loss" the test asserts (`:698-706`). The
  fix is still defensible, but the evidence proves a failure mode the production path
  prevents — a human should decide whether this counts as a valid red→green for the claimed
  defect.

- **NEEDS-HUMAN — The "permanent data loss" framing in the doc/test overstates the reachable
  defect; the only real data-loss window is one the test does not exercise.** The doc comment
  hedges "either leaks the old bytes permanently … or reclaims a fragment the re-minting
  object has just written **but not yet committed** (data loss)"
  (`crates/core/src/metadata.rs:578-582`). Data loss under the real GC requires GC to fire in
  the narrow window between B *writing* its fragment and B *committing* its inode (while B is
  still `Pending`, hence unprotected, and A's grace has already elapsed). The test instead
  fully commits B and then reclaims (`:663-685`), which is precisely the ordering in which the
  real `protects` gate makes data loss impossible. So the test cannot distinguish "leak" (the
  real outcome of its scenario) from "data loss" (the outcome it asserts) — it only reaches
  data loss because its reclaim loop drops the safety gate. The uncovered concurrent
  write-before-commit window has **no regression test**.

- The fix is correct for the hazard it names, on every angle I could attack: chunk-id
  projection to the `<2^64` in-process space is consistent with the `inode:`/`pending:` scans
  (`metadata.rs:599,606,617`); `parse_orphan_key` cleanly rejects non-orphan keys sharing the
  prefix (`metadata.rs:66-73`); `recover` is monotone `fetch_max` (`lib.rs:103-108`);
  PUT-overwrite's superseded chunks are covered because `commit_overwrite` writes the same
  orphan records the scan reads (`lib.rs:161-165`); and inode re-mint after DELETE is
  genuinely safe because `unlink` removes the inode key so `create`'s `require_absent`
  succeeds. **Attempted to refute chunk-projection truncation, orphan-key mis-parse,
  overwrite-orphan coverage, and inode re-mint safety; could not.**

## Bottom line

The code change is sound and closes a real leak/re-mint hazard. My objection is to the
**evidence**: the new test's reclaim is a re-implementation that omits the production GC's
reference-safety gate (`gc.rs:124`), so its RED demonstrates a data-loss outcome the real
custodian would prevent, and it leaves the one genuinely data-losing window (reclaim between
B's fragment write and B's commit) untested. Advisory only — human adjudicates at sign-off.

### Advisory — codex

- `crates/gateway-s3/src/sigv4.rs:508` accepts `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` and `STREAMING-UNSIGNED-PAYLOAD-TRAILER` as supported payload modes, but the decoder returns success as soon as it sees the zero-length chunk at `crates/gateway-s3/src/streaming.rs:244` and never consumes or validates the advertised trailer section. That reintroduces the iter-6 "half-accept" problem for trailer-framed SDK requests: a malformed or bogus trailer can be silently ignored while the object is committed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — All previously-ratified calls are implemented as decided (gateway-s3 crate ✓, header-only SigV4 ✓, M4 target ✓, aws-sdk-s3 dev-dep ✓, minimal error floor ✓) — but two **new documented residuals on the auth boundary** need explicit ratification at sign-off: (1) trailer variants (`STREAMING-*-TRAILER`) consume-and-ignore the trailer checksum rather than re-validating it (patch.diff:3465–3469, 4247–4252), and (2) `STREAMING-UNSIGNED-PAYLOAD-TRAILER` is accepted framing-only, extending the already-accepted UNSIGNED-PAYLOAD envelope (patch.diff:3475). Decision owed: confirm both sit inside the ratified UNSIGNED-PAYLOAD/TLS-deferral risk envelope for the plaintext-loopback floor, or direct trailer re-validation before #367 exposes the port publicly.
- [x] Validation — fitness-to-purpose — Human-by-design: is this floor fit for the first-deployment gate? At Check the listener is plaintext loopback (accepted iter-2/6 posture); the live public-TLS, deployed-host green is observed at #367 with the coordination prerequisite (0015:443–463). Reviewer could not drive the listener (target unreadable) — concrete runnable steps for the human: (1) `cargo test -p wyrd-server --test s3_http_wire` (covers signed round-trip, auth refusal, restart/orphan-ledger recovery, real-SDK interop); (2) live smoke: `cargo run --bin wyrd -- s3 --access-key AKIAIOSFODNN7EXAMPLE --secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-s3` then `aws --endpoint-url http://127.0.0.1:8080 s3api put-object --bucket b --key k --body <file>` / `get-object` / `delete-object` with those creds exported, verifying byte-identity and that an unset/forged credential is refused; (3) kill and restart the process against the same `--data-dir` and re-verify a fresh PUT + old GET (the durability seam this iteration closes). Decision owed: accept the loopback floor and route the remaining live-TLS observation to #367.

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
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Need an in-tree S3 client / smoke-test harness: live sign-off had to fall back to the in-suite real-SDK test because `aws` CLI was absent and no `wyrd s3` client subcommand exists — a signed PUT/GET/DELETE over the wire couldn't be driven by hand.
