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
