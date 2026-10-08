# Result — issue 736 / s3-server-version-header

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway advertises no version. Verified on `origin/main` at `a801997`:
  the only header `handle` stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1556`), and no build identifier exists anywhere in
  the binary — `crates/proto/build.rs` is the tree's ONLY build script, so nothing compiled
  into `wyrd` can report which checkout produced it. A client cannot tell last month's
  deployment from today's, a captured HTTP exchange does not identify the build that
  produced it, and `wyrd-validate`'s version-keyed capability matrix (proposal 0017 §3) has
  nothing to key on — its interim `--server-version` is an operator-supplied parameter that
  can silently be wrong.
- Success criterion: SPLIT — the binding criteria live in the children (#778, #779).
  The parent's own criterion, restated for the record so the children can be checked
  against it: driving the built `wyrd` binary as an `s3` role, a signed request that
  succeeds, an unsigned request refused 403, a **signed request answered in the 5xx
  server-error class** (`GET /<bucket>?acl` → `501 NotImplemented` — the subresource
  denylist, `crates/gateway-s3/src/lib.rs:342-360` and `:1655-1661`), and a streaming GET
  all return a `Server` header whose value is `wyrd/<v>`, where `<v>` is byte-equal to the
  `version` field of the child process's `role started` JSON log event and is a real
  git-derived build identity, not a default or a placeholder.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: SPLIT — see below. As a whole: (a) a build script bakes the build identity
  into `wyrd`, single-sourcing its derivation with `cargo xtask dist`; (b) the packaging
  path passes that identity INTO the image build, because `.dockerignore:6` excludes
  `.git/` and `dist` builds the shipped binary inside the image; (c) the `role started`
  event records it; (d) `S3Config` carries it and `handle` stamps `Server: wyrd/<v>` on
  every response; (e) the release smoke step compares the wire value with the tarball's
  `VERSION`. / **out of scope:** any capability-negotiation protocol or capability list on
  the wire (the issue says so explicitly — one header, one string); a `wyrd --version`
  subcommand; the version on any other role's wire surface; changing the workspace
  `version = "0.0.0"` placeholder; the OCI image's `org.opencontainers.image.version`
  label, which `dist` already sets (`xtask/src/dist.rs:458`).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: split
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

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] The binding criterion does not falsify the brief's own response categories. It exercises a success, a pre-auth 403, and a streaming GET (`brief.md:26-32`), but the invariant explicitly includes a **server error** (`brief.md:40-42`). Add a deterministic 5xx-producing case; otherwise a check can go green without proving the stated “every response” outcome.
- [x] Equality with the tarball's `VERSION` has no PR-time red→green gate in the stated criterion. The only named command is still the placeholder `cargo test -p wyrd-server --test <name>` and is promised to use no Docker (`brief.md:33-39`), while the target creates `VERSION` only during `cargo xtask dist` (`xtask/src/dist.rs:566-571`, `xtask/src/dist.rs:614-623`) and the proposed wire-vs-`VERSION` check is assigned to the release smoke (`brief.md:136-145`), whose workflow runs only on tag push or manual dispatch (`.github/workflows/release.yml:20-24`). Name a pre-merge command that builds/reads the artifact pair, or narrow the binding criterion; as written, the ordinary test can pass while dist injection drifts.
- [x] The identity contract forks on a dirty checkout. The tracker requires the same source as dist, explicitly `git describe --tags --always --dirty`, and byte equality with dist's `VERSION` (`notes.json:1`); the target still derives and normalizes the dirty suffix (`xtask/src/dist.rs:119-146`, `xtask/src/dist.rs:355-363`). The brief simultaneously promises same-checkout equality (`brief.md:22-25`) and says an ordinary dirty build deliberately advertises only its base commit while dist injects its own version (`brief.md:98-107`). Specify whether equality is only for dist-built binaries, or make local and dist derivation identical, then cover that choice with a dirty-checkout case.
- [x] The split introduces an unresolved prerequisite: `736b` “depends on 736a” (`brief.md:136-145`), but `dependency-state.json:1-7` contains only the existing, planned prerequisite `773`; there is no resolvable bundle/issue state for `736a`. Create and cite the actual child bundle IDs (and their repo/base), or the second child can be scheduled against a phantom base.
- [x] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.
- [x] The binding wire test can go green without proving the stated build identity. It only requires a non-empty value other than `0.0.0` (`brief.md:24-32`), while the design deliberately makes `S3Config::new` default to `wyrd/unknown` (`brief.md:215-217`). The cited fixture constructs `S3Config::new` and `S3Gateway::new` directly (`crates/server/tests/s3_http_wire.rs:78-89`), bypassing the production assignment in `serve_s3` (`crates/server/src/cli.rs:2377-2386`). Therefore `Server: wyrd/unknown` satisfies both test legs even if the build script and composition-root plumbing are absent; revise the criterion/fixture so the red→green gate exercises or independently proves the baked identity and rejects `unknown`.
- [x] The brief defers a tracker definition-of-done claim without naming any mechanism that can verify it. The tracker requires that the wire version “matches what `cargo xtask dist` stamps into the tarball's `VERSION` file,” but the brief says this equality is “confirmed by `.github/workflows/release.yml`” (`brief.md:122-141`) and assumes that workflow needs no change (`brief.md:355-357`). The target workflow only builds the artifacts, installs the tarball, invokes `wyrd` with no arguments, and checks usage/uninstall behavior; it never starts S3, reads `Server`, or compares it with `VERSION` (`.github/workflows/release.yml:53-88`). Container-free build-argument coupling is not end-to-end equality, so the claimed outcome remains unfalsifiable unless the brief adds a concrete release check or narrows the promise.
- [x] The tracker also makes “Startup log records the same string” part of its definition of done (`notes.json`, `Definition of done`), and the brief adds that log change to scope (`brief.md:90-97`), but its binding success criterion observes only two HTTP responses (`brief.md:24-32`) and its verification posture names no startup-log assertion (`brief.md:130-141`). The existing production event is the `role started` record at `crates/server/src/cli.rs:2199-2206`; require a deterministic assertion that this event carries the same baked value, or remove that independently observable deliverable from this slice.
- [x] **C5 cannot go green for this bundle, and auto-iterating on it will
- [x] **The brief's Design decision #2 is reversed without the brief's

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
