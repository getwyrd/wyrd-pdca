# Adversarial review — issue 469 / fdb-deploy-profiles

Skeptic's pass. I tried to break the binding red→green, the compose fixtures, and the
reviewer's verdict. Grounded on the target at `/home/eddie/wyrd/wyrd.pdca-wt` (patch
applied, uncommitted). C4-verify/run-verify and the xtask harness are **not present** in
this checkout, so I could not re-execute the asserted red→green — that inability is noted,
not scored as a refutation (issue #236).

## What I attempted to refute and could not

- **The tautology fix (the iteration-1 actionable).** `xtask/tests/fdb_deploy_profiles.rs:758`
  counts `"--metadata-backend"` flag+value **pairs**, not loose `contains("fdb")`. Verified
  on the target file: 6 total, 6 `"--metadata-backend", "fdb"`, 0 `"tikv"`
  (`deploy/small-multi-node-fdb/docker-compose.yml`, custodian0-2 + gateway0-2). Flipping any
  one role to another backend makes `fdb != total` → red. This genuinely closes the
  reviewer's "passes even if the three gateways are flipped to tikv" complaint. Could not
  refute.
- **Genuine RED for the binding (unconditional) tests.** Pre-fix, `deploy/small-multi-node-fdb/`
  and `deploy/fdb-multi-replica/` do not exist, so `read()`/`exists()` panic/fail
  (`:744`, `:758`, `:768`), and README lacks any `fdb-*` profile (brief repro:
  `grep fdb README.md` empty). Red is real by non-existence, independent of Docker.
- **Flag/subcommand acceptance.** The compose `command:` lines pass `--metadata-backend fdb`,
  `--coordination-backend etcd`, `--endpoints` to `custodian`/`s3`. Confirmed the binary
  accepts these: `crates/server/src/cli.rs:120` (`fdb` arm), `:281` (`s3` usage lists both
  flags), `:1303` (`cmd_s3`). `WYRD_FDB_CLUSTER_FILE` default `/etc/foundationdb/fdb.cluster`
  (`:120` doc) matches the compose env. Not speculative.
- **Compose parse validity.** No dangling `depends_on`, all named volumes declared, FDB env
  convention (`FDB_PROCESS_CLASS: unset`, `FDB_CLUSTER_FILE_CONTENTS`) mirrors the known-good
  `deploy/fdb-single-node/docker-compose.yml:30-33`; the reused sidecar context
  `../tikv-multi-replica/iptables-agent` exists. I could not construct a `docker compose
  config` parse failure.

## Findings (advisory)

- **`xtask/tests/fdb_deploy_profiles.rs:793` — over-broad assertion; the "profile matrix"
  guard is not guarding a matrix.** `readme_profile_matrix_names_all_six_profiles` only checks
  that six substrings appear *anywhere* in `deploy/README.md`. All six also appear as section
  headers (`deploy/README.md:36,59,75,106,187` + the `small-multi-node-fdb` prose). Concrete
  false-green: delete the entire `## Profile matrix` table (`deploy/README.md:8-25`) and the
  test still passes — every substring survives in the headers. The test name and its failure
  message ("all six profiles ... must appear") overclaim: it pins name presence, not the
  matrix, not the tier×backend mapping. It does its job for *this* patch (matrix is present),
  but provides no drift protection for the artifact it purports to pin.

- **`xtask/tests/fdb_deploy_profiles.rs:815` and `:825` — shallow prose pins.**
  `readme_states_which_single_zone_stack_is_canonical` asserts only `contains("currently
  canonical")`; `readme_records_the_tikv_fdb_single_zone_pairing` pins the literal sentence
  `"is the TiKV peer of"`. Neither verifies *which* stack is named canonical, nor that the
  pairing statement is internally consistent — they pass on the presence of a string the
  author placed. A future edit that inverts the canonical stack (says FDB is currently
  canonical) or that names the wrong peer keeps both green. The brief accepts prose-pinning,
  so this is not a defect, but the guard is weaker than "the README records the pairing"
  reads.

- **`xtask/tests/fdb_deploy_profiles.rs:901,936` — the Docker-backed leg cannot supply a
  per-fix RED on a Docker-less host.** Both `docker compose config` tests return early = green
  via `skip_or_fail_without_docker()` (`:884`) when Docker is absent locally. So the entire
  "structurally valid compose" claim rests on Docker being present at verify time; where it is
  not, these tests are green pre- *and* post-fix. The brief acknowledges this explicitly
  (assertion 1 carries the binding red), so it is not a refutation — but it means the
  "genuinely valid compose / declares three fdbserver processes" evidence is *conditional*,
  and if the verify host lacked Docker the reviewer's structural-validity confidence would be
  unearned. Worth a human confirming Docker actually ran this leg (not skipped).

- **NEEDS-HUMAN — the `small-multi-node-fdb` stack's runtime correctness is entirely
  unverified at Check, and the iteration-1 reviewer ask was only partly met.** Every test here
  is parse-only (`docker compose config`, `xtask/tests/fdb_deploy_profiles.rs:863`) or
  filesystem existence. `docker compose config` does **not** validate that
  `deploy/docker/wyrd/Dockerfile` can build `--features fdb,etcd` into `wyrd:fdb`, nor that
  the built image runs. The iteration-1 carry-forward (brief.md:296) explicitly asked to
  "confirm the build context resolves and the image builds" — **no test in the patch builds
  the image or the build context**. The plan (brief §Verification posture) declares the
  21-container bring-up deferred to a maintainer by hand, so this is a scope decision, not a
  patch bug — but the consequence is that a broken `wyrd:fdb` build or a rejected flag would
  pass C4-ci, C4-verify, and the reviewer, surfacing only in the deferred manual leg. A human
  should adjudicate whether "image builds at least once" evidence is required before sign-off,
  since the reviewer's own actionable requested exactly that and the patch does not deliver it.

- **NEEDS-HUMAN — I could not re-run the asserted red→green.** `engine/scripts/run-verify.sh`
  and the xtask harness are absent from this checkout, so C4-verify's "red without the fix,
  green with it" (`check-gates.json:46`) and C4-ci's "all checks passed" (`:37`) are taken on
  the gate's word. The static evidence above is consistent with them, but the red→green itself
  is unreproduced here — verdict provisional on that point.
