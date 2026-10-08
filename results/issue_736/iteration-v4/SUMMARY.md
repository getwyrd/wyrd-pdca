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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 86.6% — 323 of 373 instrumentable changed lines executed (floor 80%); 373 of 1237 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.58s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make every S3 success, error, and streaming response advertise the same compile-time build identity recorded at startup and packaged in `VERSION`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding decision is explicit about response planes, identity provenance, the caller-agnostic default, and the release-only equality that remains deferred (`brief.md:24`, `brief.md:182`). |
| C2 Reproduction (red pre-fix) | PASS | My stash/restore rerun kept the binary-level discriminator and reproduced a pre-fix failure on the signed 200 response's absent `Server` header, then green after restoration (`crates/server/tests/s3_server_version_header.rs:278`). |
| C3 Change | PASS | The patch stays on the declared wire/composition/packaging/documentation surfaces: the composition root supplies the identity and the common response stamp owns the header (`crates/server/src/cli.rs:2386`, `crates/gateway-s3/src/lib.rs:1652`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Land/refresh the frozen base onto prerequisite #773 and rerun the full gate — the focused red→green and targeted suites pass, but `cargo xtask ci` stops on base `h2 0.4.15` / RUSTSEC-2026-0258 rather than a patch failure (`Cargo.lock:111`, `gate-logs/C4-ci.log:5245`, `gate-logs/C4-verify.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must make dirty-state invalidation complete and add a regression — the explicit watch set replaces Cargo's package-wide default yet omits ordinary source files, so an edited/relinked binary can retain a stale clean identity; mutation strength is unmeasured because its unmutated sandbox failed the Git-index guard (`crates/server/build.rs:110`, `crates/server/build.rs:142`, `gate-logs/C5-mutants.log:1954`). |
| T1 Structure | PASS | The dependency direction remains narrow: the shipped pure normalizer is compiled by the build script and `xtask`, while only the server composition root passes identity into the gateway seam (`crates/server/src/version.rs:14`, `xtask/src/dist.rs:120`, `crates/server/src/cli.rs:2386`). |
| T2 Shape | PASS | The response value is validated as a non-empty HTTP token, malformed configuration degrades visibly, and packaging passes the same version string into the image build (`crates/gateway-s3/src/lib.rs:111`, `crates/gateway-s3/src/lib.rs:142`, `xtask/src/dist.rs:163`). |
| T3 Runtime | FAIL | An ordinary unstaged source edit can change and relink the executable without re-running version derivation because only `build.rs`, `src/version.rs`, Git metadata, and the index are watched; the running binary may therefore falsely advertise the prior clean build (`crates/server/build.rs:129`, `crates/server/build.rs:154`). |
| T4 Contribution | FAIL | The frozen multi-pass review is red on the grounded rerun-watch defect; the absent PR-artifact subcheck is N/A because it reruns at publish, while the TiKV compile subcheck passed (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`, `gate-logs/host-tikv.log:207`). |
| T5 Judgment | NEEDS-HUMAN | Confirm no overlapping merged or closed/rejected work across every affected file — this target has one synthetic base commit and no remote, while the recorded prior-art check names path history for only the gateway and dist files, so the required all-path search cannot be mechanically settled here (`brief.md:270`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the version-keyed client use case is fit for release with tarball/wire equality still unobserved; on the next `v*` tag, run the release smoke and verify its installed binary's `Server` remainder equals the untarred `VERSION` value (`.github/workflows/release.yml:79`). |

### Advisory — adversary

# Adversarial review — issue 736 (s3-server-version-header)

Attacked the evidence, the fix and the verdict. The red→green itself survived (see
"could not refute" at the end); four attacks landed, one of them on a claim the
patch's own module doc makes.

- **NEEDS-HUMAN [impl] — `crates/server/build.rs:163` watches the git *index*, which
  reintroduces the exact rebuild cost the doc two paragraphs above it (`:129-137`)
  rejects.** Both halves measured in this sandbox: (a) a plain `git status` after any
  working-tree edit rewrites `.git/index` (mtime bumped, verified on a throwaway repo);
  (b) cargo 1.96.1 recompiles a crate whose build script re-ran **even when the script's
  output is byte-identical** (verified with a two-file probe crate: `touch watched` →
  `Compiling probe2`). Concrete case: edit any file anywhere in the workspace, run
  `git status` (or `git diff`, or let an editor's git integration poll), then
  `cargo test -p wyrd-server` → `wyrd-server` recompiles and its **41** integration-test
  binaries (`crates/server/tests/*.rs`) relink, with nothing in that crate changed. The
  doc at `:132-134` declines to watch source files precisely because that "recompiles and
  relinks `wyrd-server` — the `wyrd` bin plus the ~40 integration-test binaries — every
  time"; the `index` entry buys most of that cost back for the ordinary edit→status→test
  loop while still not closing the `.dirty` lag. Note this pulls *against* the batch
  review's four blocking `build.rs` findings (which ask for a *wider* watch set): the two
  cannot both be satisfied by adding paths, so the builder should pick a side explicitly
  — either accept a lagging `.dirty` and drop `index`, or accept the churn and say so —
  rather than widening the set again.

- **NEEDS-HUMAN [impl] — `crates/server/src/version.rs:104-117`: `is_identity` does not
  enforce the docker-tag rule its own doc invokes to justify its narrowness, so a
  well-formed-looking identity fails the packaging build minutes later.** Compiled
  `version.rs` standalone and ran it; measured outputs:
  `resolve_version(Some("-1.2.3"), …)` → `Ok(version="-1.2.3", origin=Explicit)`,
  `Some(".1.2.3")` → `".1.2.3"`, `Some("+build")` → `"+build"`, and without any override
  `resolve_version(None, Some("v-1.0"), Some("abc1234"), …)` → `"-1.0"` (origin `Git`).
  A docker tag is `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}` — the **first** character may not be
  `-`, `.` or `+`. So `WYRD_VERSION=-1.2.3 cargo xtask dist` passes rung 1's validation,
  reaches `xtask/src/dist.rs:image_build_args` → `-t wyrd:-1.2.3-<flavor>`, and
  `docker buildx build` dies with `invalid reference format` after the whole build — the
  late, confusing failure the validator at `:104-115` claims to prevent ("a `tchar` such
  as `|` … would be a valid header and an invalid tag"). One added leading-byte rule in
  `is_identity` (rejecting `-`/`.`/`+` first, and a 128-byte ceiling) closes it and would
  turn the failure into the build-time error message `resolve_version` already writes.

- **NEEDS-HUMAN [impl] — `docs/design/architecture/08-crosscutting-concepts.md:85` states
  the header is stamped on "**every** response"; responses hyper generates before the
  service is entered carry none.** `handle`'s stamp (`crates/gateway-s3/src/lib.rs:1660`)
  is only reached once axum dispatches. A malformed request line, an invalid
  `Content-Length`, or a header section over hyper's buffer limit is answered by hyper
  itself with `400`/`431` and no `Server:` — concretely,
  `printf 'GET /a HTTP/1.1\r\nHost: x\r\ncontent-length: abc\r\n\r\n' | nc 127.0.0.1 <port>`
  against a `wyrd s3` role returns a bare 400. This is the same reasoning the pre-fix RED
  leg relies on (hyper adds no `Server` of its own), so it cuts both ways. The code is
  right; the living-doc sentence an operator reads should be scoped to responses the
  gateway service produces, or the gap named.

- **NEEDS-HUMAN [human] — the header is deliberately stamped on the pre-auth 403
  (`crates/gateway-s3/src/lib.rs:1660`, exercised by
  `crates/server/tests/s3_server_version_header.rs`'s unsigned leg), so any anonymous
  client learns the deployment's exact commit and whether it was built from an edited tree
  (`…+git.<sha>.dirty`), and `docs/design/architecture/14-threat-model.md:89`
  ("Information disclosure") was not revisited.** Concrete: `curl -sSD- http://<gw>/`
  with no credentials → `403` + `Server: wyrd/0.0.0+git.abc12de.dirty`. Real S3 answers
  the coarse `Server: AmazonS3` for this reason. The brief decided granularity (decision
  1) and `.dirty` (decision 2), but it decided them for the *client* use case; nothing in
  the slice weighs unauthenticated reach, and there is no configuration knob to coarsen or
  suppress the value for an internet-facing gateway. A human should confirm this is the
  intended posture (and, if so, that the threat model records it) rather than a decision
  taken sideways.

- **NEEDS-HUMAN [human] — the C5 mutation gate never ran, so ~560 net-new lines of pure
  logic have no mutation evidence, and the verdict on them is provisional (#236).**
  `gate-logs/C5-mutants.log:1954` shows the failure is the unmutated *baseline*:
  `xtask/tests/repo_hygiene_guards.rs:137` — `git ls-files -s -z must succeed` — because
  cargo-mutants' scratch copy carries no `.git`. Environmental and untouched by this
  patch, so **not** a refutation, but it means the one gate that would have probed
  `version.rs`'s branch logic (the `-dirty` carry on the fail-closed path, the empty-
  override rung, `describe_is_dirty`) produced nothing. Recorded here so the "gates green
  except a known base failure" framing is not read as "the new logic was mutation-tested".
  Same class: C4-ci's red is exclusively RUSTSEC-2026-0258 (`h2`) at
  `gate-logs/C4-ci.log:2859-2866`, with `bans ok, licenses ok, sources ok` — base-carried
  and matching the brief's `Depends on: 773`, not a defect of this patch.

## Attempted and could not refute

- **The red→green is real and on the production path.** Re-adjudicated from
  `gate-logs/C4-verify.log`: the reverted tree fails at
  `crates/server/tests/s3_server_version_header.rs:279` with the full response head
  printed and no `Server:` line — the test drives the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) through the real router and the real composition root,
  not an in-process fixture, and it names no symbol the patch introduces, so it compiles
  on both legs. It cannot pass on `wyrd/unknown` (leg 2), on a constant (leg 4's sha
  containment against an independent `git` probe), or with the log field deleted (leg 3
  fails on a missing field rather than skipping).
- **No second production path emits an unconfigured identity.** `S3Config::new` has
  exactly one non-test caller, `crates/server/src/cli.rs:2386`, and `serve_s3` sets
  `server_version` there; `router()` is the only `AppState` constructor
  (`crates/gateway-s3/src/lib.rs:287`) and `serve` adds no layers, so there is no route
  that bypasses the stamp inside the service.
- **The release smoke's kill→reinstall race does not exist.** Hypothesised `ETXTBSY` when
  `./install.sh` (`deploy/dist/install.sh:136`, `install -m 0755 … /usr/local/bin/wyrd`)
  re-runs immediately after `kill $s3_pid` in `.github/workflows/release.yml:104` → `:120`;
  measured on this host — `install`/`cp` over a *still-running* executable both return 0.
- **`is_tchar` (`crates/gateway-s3/src/lib.rs:161`) is exactly RFC 9110 §5.6.2** — all 15
  punctuation `tchar`s, no extras — and `server_header_value`'s `expect` is unreachable
  because every byte is checked first.
- **`resolve_version`'s postcondition holds** on every probed input (explicit, tagged,
  untagged, dirty, unrepresentable-tag, empty, no-input): the returned version always
  satisfies `is_identity`, and the two distinct tags `v1/2` / `v1.2` stay distinct.
- **The packaging coupling is sound**: `ARG WYRD_VERSION=""` is global and re-declared in
  the `build` stage before `ENV WYRD_VERSION=${WYRD_VERSION}`
  (`deploy/docker/wyrd/Dockerfile:39,47,76`), the empty default falls through to the next
  rung rather than baking `wyrd/`, and `release.yml:36-38` checks out with
  `fetch-depth: 0` + `fetch-tags: true`, so `git describe` sees the tag it releases.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Land/refresh the frozen base onto prerequisite #773 and rerun the full gate — the focused red→green and targeted suites pass, but `cargo xtask ci` stops on base `h2 0.4.15` / RUSTSEC-2026-0258 rather than a patch failure (`Cargo.lock:111`, `gate-logs/C4-ci.log:5245`, `gate-logs/C4-verify.log:10`).
- [ ] C5 Causal adequacy — Rebuild must make dirty-state invalidation complete and add a regression — the explicit watch set replaces Cargo's package-wide default yet omits ordinary source files, so an edited/relinked binary can retain a stale clean identity; mutation strength is unmeasured because its unmutated sandbox failed the Git-index guard (`crates/server/build.rs:110`, `crates/server/build.rs:142`, `gate-logs/C5-mutants.log:1954`).
- [ ] T5 Judgment — Confirm no overlapping merged or closed/rejected work across every affected file — this target has one synthetic base commit and no remote, while the recorded prior-art check names path history for only the gateway and dist files, so the required all-path search cannot be mechanically settled here (`brief.md:270`).
- [ ] Validation — fitness-to-purpose — Decide whether the version-keyed client use case is fit for release with tarball/wire equality still unobserved; on the next `v*` tag, run the release smoke and verify its installed binary's `Server` remainder equals the untarred `VERSION` value (`.github/workflows/release.yml:79`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- [ ] The binding wire test can go green without proving the stated build identity. It only requires a non-empty value other than `0.0.0` (`brief.md:24-32`), while the design deliberately makes `S3Config::new` default to `wyrd/unknown` (`brief.md:215-217`). The cited fixture constructs `S3Config::new` and `S3Gateway::new` directly (`crates/server/tests/s3_http_wire.rs:78-89`), bypassing the production assignment in `serve_s3` (`crates/server/src/cli.rs:2377-2386`). Therefore `Server: wyrd/unknown` satisfies both test legs even if the build script and composition-root plumbing are absent; revise the criterion/fixture so the red→green gate exercises or independently proves the baked identity and rejects `unknown`.
- [ ] The brief defers a tracker definition-of-done claim without naming any mechanism that can verify it. The tracker requires that the wire version “matches what `cargo xtask dist` stamps into the tarball's `VERSION` file,” but the brief says this equality is “confirmed by `.github/workflows/release.yml`” (`brief.md:122-141`) and assumes that workflow needs no change (`brief.md:355-357`). The target workflow only builds the artifacts, installs the tarball, invokes `wyrd` with no arguments, and checks usage/uninstall behavior; it never starts S3, reads `Server`, or compares it with `VERSION` (`.github/workflows/release.yml:53-88`). Container-free build-argument coupling is not end-to-end equality, so the claimed outcome remains unfalsifiable unless the brief adds a concrete release check or narrows the promise.
- [ ] The tracker also makes “Startup log records the same string” part of its definition of done (`notes.json`, `Definition of done`), and the brief adds that log change to scope (`brief.md:90-97`), but its binding success criterion observes only two HTTP responses (`brief.md:24-32`) and its verification posture names no startup-log assertion (`brief.md:130-141`). The existing production event is the `role started` record at `crates/server/src/cli.rs:2199-2206`; require a deterministic assertion that this event carries the same baked value, or remove that independently observable deliverable from this slice.
- [ ] size backstop — this slice is behaving oversized: patch is 116 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.
- [ ] **C5 cannot go green for this bundle, and auto-iterating on it will
- [ ] **The brief's Design decision #2 is reversed without the brief's

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rationale: T4's rubric review failed (4 blocking), C5 mutation testing never ran (environmental, but leaves ~560 new lines unevidenced), and the size backstop has tripped (patch 116KB > 100KB threshold, 3 rounds already spent > 2 threshold) — matches the bundle's own iterate-plan recommendation. Send back to re-plan rather than iterate-do because several open items are brief/criterion defects, not just implementation gaps: - The brief defers the tracker's "wire version matches cargo xtask dist's tarball VERSION" requirement to the release workflow, but that workflow never actually performs the comparison — the equality claim is currently unfalsifiable as scoped. - The tracker's "startup log records the same string" definition-of-done item is not covered by the binding success criterion at all, only by non-binding scope prose — needs to be made binding or dropped from this slice's claimed deliverables. - An unresolved tension between the batch review (wants a wider build.rs watch set) and the adversary review (current git-index watch already reintroduces the rebuild-cost problem the script's own doc says it avoids) — these can't both be satisfied by adding more paths; the re-plan needs to pick a side (accept dirty-state staleness, or accept the rebuild churn) and say so explicitly in the brief. - Version-string validator doesn't enforce Docker's tag-naming rule (leading -/./+ rejected), causing a confusing late failure in the Docker build step rather than an early, clear one. - Unauthenticated 403 responses disclose the exact build commit and dirty-tree status; the brief decided this granularity for the client use case but never weighed it against the threat model or gave an operator a way to suppress/coarsen it for internet-facing gateways. - One §6 item (binding test can go green on the unconfigured `wyrd/unknown` default via s3_http_wire.rs) appears to be carried forward from an earlier round and may already be addressed — the current adversary review confirms the red->green runs against the built binary and cannot pass on `unknown` or a hardcoded constant. Re-plan should confirm/resolve this discrepancy rather than carry it forward again. - Two §6 bullets were truncated by the known SUMMARY assembly bug (same as #771); full text was not retrieved before this decision — re-plan should check the archived iteration rounds' check-advisory-adversary.md for the complete findings before re-authoring. Human directive: iterate-plan for 736 — return to Plan for `pdca split` / brief rework.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
