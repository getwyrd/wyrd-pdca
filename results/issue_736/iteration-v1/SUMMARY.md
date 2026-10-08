# Result — issue 736 / s3-server-version-header

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway advertises no version. Verified on `main` at `65ca4fd`:
  `grep -rn "header::SERVER\|\"server\"" crates/gateway-s3/src/` returns nothing — no
  `Server:` header, and no build identifier anywhere in the response path. The only header
  the gateway stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1557`). Nothing on the wire says what a client is
  talking to, so a client cannot tell last month's deployment from today's, a captured HTTP
  exchange does not identify the build that produced it, and `wyrd-validate`'s
  version-keyed capability matrix (proposal 0017 §3) has nothing to key on — its interim
  `--server-version` is an operator-supplied parameter that can silently be wrong.
- Success criterion: BINDING (demonstrable by C4-verify at Check, no container, no
  cluster): a new integration test spawns the **built `wyrd` binary** as an `s3` role over
  a loopback listener (`--s3-listen 127.0.0.1:0 --log-format json`, killed on every exit
  path) and asserts, against that one child process:
  1. **both planes carry the header** — a signed request that succeeds AND an unsigned
     request refused with 403 come back carrying a `Server` header whose value starts
     `wyrd/`. The error leg is the load-bearing half: an error response is the one most
     likely to end up in a bug report, and it is the leg a per-handler implementation
     would miss;
  2. **the value is the BAKED build identity, not a default** — the remainder after
     `wyrd/` is non-empty, is **not** the caller-agnostic `S3Config::new` default
     (`unknown`, see Design), and is not EQUAL to the bare workspace placeholder `0.0.0`
     (equality, not a prefix test: the legitimate derived value on an untagged checkout is
     `0.0.0+git.<sha>`, which starts with `0.0.0` and is correct). This is
     why the criterion drives the BINARY rather than composing `S3Config` in-process: only
     the composition root (`serve_s3`, `cli.rs:2377-2384`) sets the field from the build
     script's constant, so an in-process fixture would go green on `wyrd/unknown` with the
     whole derivation-and-plumbing half absent;
  3. **the startup log records the same string** — the child's `role started` JSON event
     (`cli.rs:2199-2206`, emitted to stderr; `--log-format json` per `cli.rs:496`) carries
     a `version` field whose value is byte-equal to the wire header's remainder. This is a
     tracker definition-of-done item in its own right and is asserted here, not assumed;
  4. **the value came from git, not from a constant** — when a repository is visible,
     `WYRD_VERSION` is unset in the test's environment, and `git describe --tags --always`
     does not resolve exactly onto a tag, the advertised remainder CONTAINS the short commit
     sha that `git rev-parse --short HEAD` prints. Deliberately a containment check on the
     sha rather than string equality with a re-normalized `describe`: it binds the value to
     the real derivation while staying immune to the `-dirty` suffix (a gate applies a patch,
     so the tree may or may not be dirty when the build script runs) and to tag shape. The
     test must not call the production normalizer — see Falsifiability. When the
     preconditions do not hold, this leg is skipped with a printed reason and legs 1-3 still
     bind.
  SUPPLEMENTARY (deferred, see Verification posture): the same string equals what
  `cargo xtask dist` writes to the tarball's `VERSION` for the same checkout.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) `S3Config` gains a server-identity field, defaulted in `S3Config::new` so
  every existing caller keeps compiling, and `handle` stamps it on the response beside the
  request id; (b) the composition root supplies the real value — `crates/server` grows a
  build script that bakes the build identity, and `serve_s3` sets the field from it; (c)
  the `role started` event gains the same string, so the running build is readable from the
  logs as well as the wire (a tracker DoD item, asserted by criterion leg 3); (d) the
  version derivation is SINGLE-SOURCED with `cargo xtask dist`'s (see Design), and the
  packaging path is plumbed so the binary an operator actually runs carries a real version
  rather than the fallback; (e) `.github/workflows/release.yml`'s existing installer smoke
  step gains the ONE end-to-end check that closes the tracker's "matches `VERSION`" clause
  — see Verification posture. It is ~6 lines of shell inside the block that already
  installs the tarball; it cannot run at Check and is not part of the binding criterion.
  **/ out of scope:** any capability-negotiation protocol or capability list on the wire
  (the issue says so explicitly — this is one header carrying one string); a `wyrd
  --version` subcommand or a version on any other role's wire surface (worth doing,
  separate issue); changing the workspace `version = "0.0.0"` placeholder
  (root `Cargo.toml:35`); the OCI image's `org.opencontainers.image.version` label, which
  `dist` already sets (`xtask/src/dist.rs:458`).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (2 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 88.5% — 184 of 208 instrumentable changed lines executed (floor 80%); 208 of 623 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.82s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #736: make every S3 success and error response advertise the baked build identity also recorded at startup and packaged in `VERSION`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the response categories, binary-level oracle, identity derivation, log equality, and explicitly deferred release equality, so the required behavior is decidable (`brief.md:21`). |
| C2 Reproduction (red pre-fix) | PASS | With production stashed and the added test retained, both cases independently failed on the absent header at `crates/server/tests/s3_server_version_header.rs:255`, matching the frozen red evidence (`gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The diff stays within the planned gateway/config, composition/logging, shared derivation, packaging, release-smoke, and test surfaces; it adds no capability protocol or unrelated role behavior (`brief.md:127`). |
| C4 Verification (red→green) | PASS | The same two-test command independently went red pre-fix and green post-fix; frozen CI also completed all checks and diff coverage was 88.5% (`gate-logs/C4-verify.log:54`, `gate-logs/C4-ci.log:3422`, `gate-logs/C4-diff-cov.log:732`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must reconcile the discriminator with the supported repository-less fallback: `0.0.0` is produced at `crates/server/src/version.rs:113` but rejected at `crates/server/tests/s3_server_version_header.rs:331`, so the unmutated mutation baseline failed and causal strength was not tested (`gate-logs/C5-mutants.log:1639`). |
| T1 Structure | PASS | Ownership follows the narrow dependency direction: the gateway owns header shape, the server composition root supplies identity, and tooling compiles the product's pure derivation (`crates/gateway-s3/src/lib.rs:98`, `crates/server/src/cli.rs:2381`, `xtask/src/dist.rs:119`). |
| T2 Shape | PASS | The value is validated once as an RFC token, the default remains well formed, and one post-dispatch stamp covers every router response without per-handler drift (`crates/gateway-s3/src/lib.rs:123`, `crates/gateway-s3/src/lib.rs:1635`). |
| T3 Runtime | FAIL | The restricted build-script watches omit ordinary worktree files and loose tag refs, so incremental builds can advertise a stale clean/tag-derived identity and defeat capture-to-build attribution (`crates/server/build.rs:48`). |
| T4 Contribution | FAIL | Affected-path history and all-state PR searches found no prior equivalent, and the artifact audit is N/A until its mandatory publish rerun, but the required batched review remains red on the two unresolved defect classes (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The fix acts on the direct composition and common response-stamp path rather than adding a capability probe or symptom guard, leaving no additional architecture or scope judgment beyond the concrete defects above (`crates/gateway-s3/src/lib.rs:261`, `crates/gateway-s3/src/lib.rs:1635`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Sign-off must decide whether checked wire/log behavior plus still-unobserved tag-time tarball equality is sufficient for operator attribution; a mismatched identity would mis-key support and capability decisions (`brief.md:196`). |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must reconcile the discriminator with the supported repository-less fallback: `0.0.0` is produced at `crates/server/src/version.rs:113` but rejected at `crates/server/tests/s3_server_version_header.rs:331`, so the unmutated mutation baseline failed and causal strength was not tested (`gate-logs/C5-mutants.log:1639`).
- [ ] Validation — fitness-to-purpose — Sign-off must decide whether checked wire/log behavior plus still-unobserved tag-time tarball equality is sufficient for operator attribution; a mismatched identity would mis-key support and capability decisions (`brief.md:196`).
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- [ ] The binding wire test can go green without proving the stated build identity. It only requires a non-empty value other than `0.0.0` (`brief.md:24-32`), while the design deliberately makes `S3Config::new` default to `wyrd/unknown` (`brief.md:215-217`). The cited fixture constructs `S3Config::new` and `S3Gateway::new` directly (`crates/server/tests/s3_http_wire.rs:78-89`), bypassing the production assignment in `serve_s3` (`crates/server/src/cli.rs:2377-2386`). Therefore `Server: wyrd/unknown` satisfies both test legs even if the build script and composition-root plumbing are absent; revise the criterion/fixture so the red→green gate exercises or independently proves the baked identity and rejects `unknown`.
- [ ] The brief defers a tracker definition-of-done claim without naming any mechanism that can verify it. The tracker requires that the wire version “matches what `cargo xtask dist` stamps into the tarball's `VERSION` file,” but the brief says this equality is “confirmed by `.github/workflows/release.yml`” (`brief.md:122-141`) and assumes that workflow needs no change (`brief.md:355-357`). The target workflow only builds the artifacts, installs the tarball, invokes `wyrd` with no arguments, and checks usage/uninstall behavior; it never starts S3, reads `Server`, or compares it with `VERSION` (`.github/workflows/release.yml:53-88`). Container-free build-argument coupling is not end-to-end equality, so the claimed outcome remains unfalsifiable unless the brief adds a concrete release check or narrows the promise.
- [ ] The tracker also makes “Startup log records the same string” part of its definition of done (`notes.json`, `Definition of done`), and the brief adds that log change to scope (`brief.md:90-97`), but its binding success criterion observes only two HTTP responses (`brief.md:24-32`) and its verification posture names no startup-log assertion (`brief.md:130-141`). The existing production event is the `role started` record at `crates/server/src/cli.rs:2199-2206`; require a deterministic assertion that this event carries the same baked value, or remove that independently observable deliverable from this slice.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must reconcile the discriminator with the supported repository-less fallback: `0.0.0` is produced at `crates/server/src/version.rs:113` but rejected at `crates/server/tests/s3_server_version_header.rs:331`, so the unmutated mutation baseline failed and causal strength was not tested (`gate-logs/C5-mutants.log:1639`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
