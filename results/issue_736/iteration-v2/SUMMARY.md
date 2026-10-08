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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 85.4% — 181 of 212 instrumentable changed lines executed (floor 80%); 212 of 679 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.78s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #736: advertise the baked build identity as `Server: wyrd/<version>` on every S3 response and keep it aligned with startup logs and distribution metadata.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | FAIL | The plan must include a living-architecture update for changing every S3 API response; its scope omits that merge requirement from `AGENTS.md:154-157`. |
| C2 Reproduction (red pre-fix) | PASS | My stash/reapply run reproduced two header-absence failures before the fix and two passes after it, matching `gate-logs/C4-verify.log:10-54`. |
| C3 Change | FAIL | The public response-contract change at `crates/gateway-s3/src/lib.rs:1633-1641` is incomplete until a living architecture document is updated as required by `AGENTS.md:154-157`. |
| C4 Verification (red→green) | PASS | Independent red→green and default-runtime runs passed; frozen CI completed every check at `gate-logs/C4-ci.log:3425`, while my rerun stopped only when cargo-deny tried to lock a read-only global advisory DB, a reviewer-host caveat. |
| C5 Causal adequacy | PASS | The single fallback handler stamps the category-wide invariant at `crates/gateway-s3/src/lib.rs:1528-1642` without a capability probe or symptom guard; mutation evidence was unavailable only because its unmutated copy could not run `git ls-files` (`gate-logs/C5-mutants.log:1937-1959`). |
| T1 Structure | PASS | The discriminator remains black-box and pre-fix-compilable (`crates/server/tests/s3_server_version_header.rs:35-45`), with child cleanup and bounded waits at `crates/server/tests/s3_server_version_header.rs:70-114`. |
| T2 Shape | FAIL | The supported explicit override wins at `crates/server/src/version.rs:114-118`, but the git-provenance leg unconditionally requires the HEAD SHA at `crates/server/tests/s3_server_version_header.rs:408-441`; `WYRD_VERSION=1.2.3` reproduces the false failure. |
| T3 Runtime | PASS | The built-binary test exercised signed 200, unsigned 403, and startup-log equality at `crates/server/tests/s3_server_version_header.rs:293-391`, and both default-runtime tests passed. |
| T4 Contribution | FAIL | The batched review remains red on the override oracle and docs currency (`gate-logs/T4-batch-review.log:10`); the later contribution-artifact audit is separately N/A until publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add an override-origin signal or equivalent so the SHA leg skips only intentional overrides—the current test rejects the supported distribution input at `crates/server/tests/s3_server_version_header.rs:408-441`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Sign-off must decide whether to accept container-free coupling until the first `v*` run or require a real `cargo xtask dist --oci-archive` plus the smoke at `.github/workflows/release.yml:59-109` now—tarball-to-wire equality remains unobserved. |

Prior-art check: PASS — I queried merged history for every affected path and compared the file lists of all 11 closed-unmerged PRs; none attempted an S3 server-version header (the sole overlapping rejected path was unrelated segmented-map work in `crates/server/src/lib.rs`).

Deferred subcheck: `T4-contribution` is N/A because `pr-description.md` is intentionally drafted later and the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`).

### Advisory — adversary

# Adversarial review — issue 736 (advisory, never gating)

Attacked the red→green evidence, the stamp point, the derivation, the packaging plumbing
and the two gate reds. Four findings; the rest of the attack failed and is recorded at the
bottom.

- NEEDS-HUMAN [impl] — **A reachable tag containing `/` makes every response advertise
  `wyrd/unknown` and puts the log and the wire on two different strings — the exact drift
  this slice exists to prevent.** `crates/server/src/version.rs:87` interpolates the raw
  `git describe` text into the version (`format!("{fallback}+git.{d}{dirty_suffix}")`),
  but `crates/gateway-s3/src/lib.rs:142` (`is_tchar`) admits only RFC 9110 token bytes and
  `:122` (`server_header_value`) silently degrades anything else to `unknown`. Nothing pins
  the two contracts together: `version.rs`'s unit tests never feed a slash-tag and the
  gateway's token test only feeds hand-written `dist` shapes. Concrete input, reproduced
  here with git: a tag named `archive/backup-premerge-signoff` (**this repo's only existing
  tag**, per the brief) reachable from HEAD yields
  `git describe --tags --always` = `archive/backup-premerge-signoff-1-g2ece2f1`
  → version `0.0.0+git.archive/backup-premerge-signoff-1-g2ece2f1` → `/` is not a tchar →
  every response says `Server: wyrd/unknown` while the `role started` event's `version`
  field (`crates/server/src/cli.rs:2208`) and the tarball's `VERSION` say the real string.
  The same holds for the common `release/1.2.0` tag convention and for any operator-supplied
  `WYRD_VERSION` with a space in it (used *verbatim* at `version.rs:115-117`). Two consequences:
  in production the invariant fails silently-but-for-one-warn-line, and on any such checkout
  `cargo test -p wyrd-server --test s3_server_version_header` goes red for every developer
  (`tests/s3_server_version_header.rs:337` rejects `unknown`, `:350` and the leg-3 equality
  then also fail). Fixable in the diff: validate/sanitize at the derivation (or assert the
  coupling in `version.rs`'s tests) rather than only at the wire.

- NEEDS-HUMAN [human] — **C5 cannot go green for this bundle, and auto-iterating on it will
  loop.** The mutants baseline now fails on `xtask/tests/repo_hygiene_guards.rs:137`
  (`git ls-files -s -z must succeed`, `gate-logs/C5-mutants.log` tail) — cargo-mutants runs
  in a copy of the tree with no `.git`, and that test requires a real index. `xtask` is in
  the mutant package set only because this slice must edit `xtask/src/dist.rs:128` to
  single-source the derivation, so the builder cannot remove the interaction without
  abandoning the shared-module design. Round 1 failed C5 for a *different* reason (the
  `0.0.0` discriminator) that merely masked this one — `cargo test` stops at the first
  failing target. Net effect: mutation adequacy has now gone **unmeasured for two rounds**,
  so no claim about causal strength of the new tests is supported by evidence. This needs a
  gate-scope decision (exclude `xtask` from `mutants-in-diff`, or accept C5 as
  unmeasurable here), not another rebuild.

- NEEDS-HUMAN [human] — **The brief's Design decision #2 is reversed without the brief's
  sign-off.** The brief answers "Does a `-dirty` build advertise as dirty? **Yes** … An
  untagged, dirty binary in a deployment is precisely the thing worth catching";
  `crates/server/build.rs:45` deliberately asks `git describe --tags --always` with **no
  `--dirty`**, so any binary not built through `cargo xtask dist` advertises the clean
  commit's identity from a dirty tree. The reasoning given (a `--dirty` watch set would
  relink `wyrd-server` on every build) is sound and the shipped artefacts are unaffected
  (both `dist` paths inject `WYRD_VERSION` explicitly — `xtask/src/dist.rs:493`,
  `deploy/docker/wyrd/Dockerfile:76`), but "which builds may lie about dirtiness" is a
  fitness-to-purpose call the brief already made the other way. Ratify or restore.

- NEEDS-HUMAN [impl] — **The release smoke's own failure path cannot report itself.**
  `.github/workflows/release.yml:92` runs under `sh -eu`, so if the role never came up the
  assignment `advertised=$(curl …)` (escaped `\$` in the YAML) inherits curl's exit 7 and errexit aborts the step
  **before** the `if` at `:95` and its `cat /tmp/s3.log` at `:97` — and the container is
  `docker run --rm`, so the role's log is gone with it. Concrete case: the `wyrd s3` child
  dies during the ten one-second retries (a bad `--data-dir`, a port already bound); the
  release fails with a bare `curl: (7)` and no server log, which is exactly the diagnostic
  the added block promises. One line to fix (`advertised=$(curl … || true)` or capture the
  status). Worth fixing now because this leg is unobservable until the first `v*` tag —
  review is the only check it will get.

Non-refutations, recorded so they are not re-litigated:

- The two `[CONVENTION]` docs-currency findings in `gate-logs/T4-batch-review.log`
  (`crates/gateway-s3/src/lib.rs:1638-1639`) look like a false-positive class: the rubric's
  docs-currency rule names "a port, an API operation, an RPC, a CLI flag, or a persisted
  field", none of which a response header is, and the peer invariant this diff copies —
  #529's `x-amz-request-id`, stamped ten lines above the new `Server` insert (target
  source `:1631` vs `:1641`) — appears nowhere in
  `docs/design/**` (checked: zero hits). Routing that back to Do likely spends a round
  writing a doc section the repo does not keep.
- `crates/server/build.rs:64-65` claims the rerun set "is complete"; it is not, in one
  narrow case: `packed-refs` is skipped when it does not exist at build time
  (`:106`, and it does not exist in this checkout), so a later `git fetch --tags` that
  *creates* `packed-refs` bakes a stale identity until something else re-runs the script.
  Conversely watching the whole `refs/heads` directory (`:91`) re-runs the script — and
  relinks `wyrd-server` plus its integration-test binaries — on every unrelated branch
  update or fetch. Both are cost/precision trade-offs with no clean alternative; noted, not
  raised as a defect.
- **The evidence itself is genuine but conditional.** `gate-logs/C4-verify.log` shows a real
  red on the production path (both tests panic at `tests/s3_server_version_header.rs:260`
  on the absent header, against the reverted tree, with the test compiling unchanged) — not
  a tautology and not a mocked defect. The limit worth knowing at sign-off: in a tree with
  no visible repository the same tests pass on `0.0.0+git.unknown` with the entire git
  derivation absent — observed, in the mutants baseline copy (`gate-logs/C5-mutants.log`,
  `test the_advertised_version_carries_the_commit_it_was_built_from ... ok` beside
  `every_s3_response_advertises_the_baked_build_version ... ok`, where leg 4 self-skips and
  leg 2's discriminators are all satisfied). So "the value is the BAKED build identity" binds
  only where a repo is visible; the brief pre-declared that skip, so this is a limit, not a
  finding.

Attempted and could not refute: (a) that the single stamp point misses a response —
`handle` has no early return and the router is a bare `fallback` with no layers
(`crates/gateway-s3/src/lib.rs:276`, `:1641`), so success, 403, 404, 405, 501 and the
streaming-GET head all traverse it; (b) that the test passes for the wrong reason — it
drives `CARGO_BIN_EXE_wyrd` as a real child over a real socket, names no symbol the slice
introduces, kills the child from `Drop`, and bounds every wait; (c) that `S3Config` gained
a field incompatibly — no struct literal exists in the tree, only `S3Config::new`
(`:168`); (d) that the `image_build_args`/`version_file` extraction changed `dist`'s
behaviour — the argument order, the OCI second exporter, the `create_dir_all` and the
`VERSION` bytes are all preserved; (e) that the smoke's `wyrd s3` invocation cannot start —
`resolve_backend(None)` is redb and `resolve_coordination_backend(None)` is mem
(`crates/server/src/cli.rs:149`, `:359`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Rebuild must add an override-origin signal or equivalent so the SHA leg skips only intentional overrides—the current test rejects the supported distribution input at `crates/server/tests/s3_server_version_header.rs:408-441`.
- [ ] Validation — fitness-to-purpose — Sign-off must decide whether to accept container-free coupling until the first `v*` run or require a real `cargo xtask dist --oci-archive` plus the smoke at `.github/workflows/release.yml:59-109` now—tarball-to-wire equality remains unobserved.
- [ ] **A reachable tag containing `/` makes every response advertise
- [ ] **C5 cannot go green for this bundle, and auto-iterating on it will
- [ ] **The brief's Design decision #2 is reversed without the brief's
- [ ] **The release smoke's own failure path cannot report itself.**
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- [ ] The binding wire test can go green without proving the stated build identity. It only requires a non-empty value other than `0.0.0` (`brief.md:24-32`), while the design deliberately makes `S3Config::new` default to `wyrd/unknown` (`brief.md:215-217`). The cited fixture constructs `S3Config::new` and `S3Gateway::new` directly (`crates/server/tests/s3_http_wire.rs:78-89`), bypassing the production assignment in `serve_s3` (`crates/server/src/cli.rs:2377-2386`). Therefore `Server: wyrd/unknown` satisfies both test legs even if the build script and composition-root plumbing are absent; revise the criterion/fixture so the red→green gate exercises or independently proves the baked identity and rejects `unknown`.
- [ ] The brief defers a tracker definition-of-done claim without naming any mechanism that can verify it. The tracker requires that the wire version “matches what `cargo xtask dist` stamps into the tarball's `VERSION` file,” but the brief says this equality is “confirmed by `.github/workflows/release.yml`” (`brief.md:122-141`) and assumes that workflow needs no change (`brief.md:355-357`). The target workflow only builds the artifacts, installs the tarball, invokes `wyrd` with no arguments, and checks usage/uninstall behavior; it never starts S3, reads `Server`, or compares it with `VERSION` (`.github/workflows/release.yml:53-88`). Container-free build-argument coupling is not end-to-end equality, so the claimed outcome remains unfalsifiable unless the brief adds a concrete release check or narrows the promise.
- [ ] The tracker also makes “Startup log records the same string” part of its definition of done (`notes.json`, `Definition of done`), and the brief adds that log change to scope (`brief.md:90-97`), but its binding success criterion observes only two HTTP responses (`brief.md:24-32`) and its verification posture names no startup-log assertion (`brief.md:130-141`). The existing production event is the `role started` record at `crates/server/src/cli.rs:2199-2206`; require a deterministic assertion that this event carries the same baked value, or remove that independently observable deliverable from this slice.
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T5 Judgment — Rebuild must add an override-origin signal or equivalent so the SHA leg skips only intentional overrides—the current test rejects the supported distribution input at `crates/server/tests/s3_server_version_header.rs:408-441`.; **A reachable tag containing `/` makes every response advertise; **The release smoke's own failure path cannot report itself.**; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
