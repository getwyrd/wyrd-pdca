# Brief — issue 742 / dist-ship-wyrd-validate-two-binary-tarball

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md` §2
> ("`wyrd-validate` **ships in the operator tarball**" and the pipeline finding beneath it).
> Read in place in the target checkout — never copied here.
>
> **The packaging decision this slice was blocked on was taken by the maintainer in the Plan
> session for this batch (2026-08-17): the production OCI image carries both binaries —
> option A.** See Design §1. That is the whole reason this brief exists; without it the slice
> could not proceed, and the deliberation is preserved in `PACKAGING-DECISION.md` beside this
> file.
>
> **Where that record lives, stated precisely because it matters.** It is a SESSION record —
> this brief and `PACKAGING-DECISION.md` are its artifacts. It is NOT in the tracker thread:
> issue #742's only comment (eduralph, 2026-08-16) is the one that POSES the question ("a
> decision nobody has posed … this slice cannot proceed without it"), and proposal 0017
> likewise "does not answer" it and assigns it to the release-pipeline owner
> (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:176-180`). The question was
> put to the maintainer in session and answered there, by the person the proposal assigns it
> to. Two consequences, neither of them Do's: (1) sign-off §9 must confirm option A and
> should mirror one sentence onto issue #742 so the tracker stops asking a settled question;
> (2) if it is not confirmed at sign-off, the slice is REJECTED and returns to Plan — it is
> not patched into option B, because B is a different pipeline.

- **Slug:** dist-ship-wyrd-validate-two-binary-tarball
- **Track:** blackbox
- **Kind:** enhancement (design proposal)
- **Goal:** `wyrd-validate` ships to operators. The production OCI image builds and carries
  both binaries, `cargo xtask dist` extracts both out of that image into the tarball, the
  installer places and removes both, and the container-free template tests pin the
  two-binary layout as a contract so it cannot silently regress to one.
- **Defect:** (framed as the gap) The distribution pipeline is single-binary end to end, in
  five places, verified on `main` at `65ca4fd`: the Dockerfile builds `--bin wyrd` (`:66`),
  calls its runtime stage "a minimal image hosting just the `wyrd` binary" (`:68`) and
  copies one path (`:122`); `dist::obtain_binary` extracts that one path
  (`IMAGE_BINARY_PATH = "/usr/local/bin/wyrd"`, `xtask/src/dist.rs:40`) and `assemble`
  copies it to one destination (`:559-564`); `deploy/dist/install.sh` installs
  `$HERE/bin/wyrd` and removes `$BINDIR/wyrd`; `deploy/dist/README.md` describes one
  binary; and `.github/workflows/release.yml` smoke-tests `/usr/local/bin/wyrd` (`:75`) and
  asserts its absence after uninstall (`:85`). So a validation tool an operator is meant to
  run against their own deployment has no way to reach them.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check, container-free, inside
  `cargo xtask ci`): the shipped-binary set becomes DATA — one pure function in
  `xtask::dist` mapping each binary's in-image path to its tarball destination — the Rust
  side of the pipeline READS that table, and every other stage's spelling of the binary set
  is PINNED to it by an assertion. A new test asserts, by reading the real repo files, that
  every one of the five places above names **both** binaries:
  1. the Dockerfile builds each binary and copies each into the runtime stage;
  2. `install.sh` installs each from `bin/<name>` and removes each on `--uninstall`;
  3. the release workflow's smoke step exercises each installed path and asserts each is
     gone after uninstall;
  4. `README.md` distinguishes the two roles of the two binaries;
  5. those four assertions are **one checker over a binary set, not written out per binary**,
     and that checker runs over **both** sets: the red file's local expected set AND the
     production table. Mind the split the gate forces (see Falsifiability): the red-earning
     file may not name a net-new symbol, so it holds the checker as a plain function of a
     binary set (returning the list of files that disagree) and calls it on its OWN local
     expected set. The existing `dist_templates.rs` then runs **that same checker** on
     `xtask::dist`'s production table — include the red file as a module
     (`#[path = "dist_two_binary_layout.rs"] mod layout;`), so the checker exists once; do NOT
     put it in a new helper file under `xtask/tests/`, because the C4 classifier treats every
     added `tests/*.rs` as a discriminator test (`engine/scripts/run-verify.sh:144`). Keep the
     red file module-includable (no crate-only inner attributes). `dist_templates.rs` also
     pins the local set EQUAL to the production table. The diagnostic this buys, stated
     exactly: add a third entry to the production table and change nothing else, and
     `dist_templates.rs` fails naming **every pipeline file that lacks the new binary**, while
     the equality assertion fails telling you to update the red file's local set. Two files,
     one checker, one declaration — the table stays the source and the text test still
     compiles against a reverted tree. This is the honest version of
     "single source": Docker, shell, YAML and Markdown cannot read a Rust function, so what
     the slice buys is *declared once, duplication checked by the gate* — the same shape the
     repo already uses for the FDB pin (`xtask/tests/fdb_image.rs` pins one `ARG FDB_VERSION`
     across three files). Adding a third binary stays a multi-file edit; what changes is that
     the gate names every file you missed instead of the release doing it.
  6. AND one observation beyond file text, so the criterion is not purely lexical: the
     binary-staging step is extracted as a **`pub`** callable that takes the table plus a
     source directory and populates a staging tree (today it is four hard-coded lines inside
     the private `assemble`, `xtask/src/dist.rs:559-564`, unreachable from any integration
     test). A test runs it over a tempdir holding two dummy "binaries" with **different
     contents** (e.g. `roles-binary` and `validator-binary`) and asserts the staging tree ends
     up with `bin/wyrd` and `bin/wyrd-validate`, both `0755`, and that **each destination is
     byte-for-byte equal to its own source** — so copying one source to both destinations, or
     swapping them, fails. The mapping that feeds staging is checked too: where `obtain_binary`
     extracts each in-image path on the packaging host is computed by a pure function of the
     table, and a test asserts those host paths are pairwise distinct (no two entries extract
     onto one file), as are the table's in-image paths and its tarball destinations. That is
     the real staging code, exercised, with no container. These assertions name new API, so
     they live in `dist_templates.rs`, not in the red-earning file.
  DEFERRED and named as such (see Verification posture): "a real tarball contains both
  binaries, and `install.sh` places both on a real host" — the release workflow's to prove.
  Not a choice: nothing in `cargo xtask ci` can build a tarball (that needs Docker and a
  network, `xtask/src/dist.rs:26-28`), and `install.sh` cannot be executed by a test at all —
  it refuses to run as non-root (`deploy/dist/install.sh:90`), writes `/etc/wyrd`, creates
  users and installs units. Verified by reading it, not assumed.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p xtask --test dist_two_binary_layout`, no Docker, no network. This is the
  one place in this batch where the red is *earned by construction rather than declared*,
  and it depends on a deliberate choice Do must honour: **the new test file must assert
  over the repo's FILE TEXT (`Dockerfile`, `install.sh`, `README.md`, `release.yml`) and
  must not name any symbol this patch introduces.** This instance's `C4-verify` reverts
  every modified production file and keeps the added test
  (`engine/scripts/run-verify.sh:499-517`) — verified by dry-running
  `run-verify.sh --classify` over this brief's expected file set, which returns
  `ADDED_TEST xtask/tests/dist_two_binary_layout.rs` + `CRATE xtask`. So on the red leg the
  Dockerfile and installer revert to their single-binary form, the kept test reads them and
  its assertions FAIL — a genuine red, with the test still compiling because it touches no
  new API. Concretely: that file declares its own `const EXPECTED_BINARIES: [(&str, &str); 2]`
  (in-image path, tarball destination) and a checker function over a binary set, and its
  test calls the checker on that constant. The assertion that this local set EQUALS
  `xtask::dist`'s production table, and the second run of the same checker over the
  production table (criterion 5), name new API, so they go in `dist_templates.rs` with the
  rest. Put the assertions about the new pure function in the EXISTING
  `xtask/tests/dist_templates.rs` instead, where no red is owed. On the red leg
  `dist_templates.rs` is a MODIFIED file and is reverted with the production change
  (`run-verify.sh:510-517`), so its module include of the red file costs the red nothing.
- **Invariant to restore:** *The set of binaries the distribution ships is DECLARED in one
  place, and no stage of the pipeline may disagree with that declaration without the gate
  saying so.* Stated over the category — the pipeline as a whole, not the tarball alone —
  because the defect being removed is not "one binary is missing" but "the binary set is
  spelled out five times independently and nothing compares the spellings", which is what
  makes the second binary a five-file change with five chances to miss one, silently. Note
  the deliberate wording: *declared once and checked*, NOT "every stage reads the
  declaration". A Dockerfile, a POSIX shell script and a GitHub workflow cannot read a Rust
  function, and a brief that promises they will is promising a code-generation mechanism this
  slice is not building (and should not: generated pipeline files would be a much larger
  architectural change). What is achievable, and what this invariant demands, is the FDB-pin
  shape: one declaration, and a gate that fails the moment any consumer drifts from it.
  Source: exactly those two precedents in this repo — the templates
  (`dist::staging_plan`, `xtask/src/dist.rs:196-260`, asserted against the repo by
  `xtask/tests/dist_templates.rs:339-380`) and the FDB pin ("`FDB_VERSION` is the SINGLE
  SOURCE OF TRUTH", `deploy/docker/wyrd/Dockerfile:17-26`, pinned across three files by
  `xtask/tests/fdb_image.rs`). SELF-TEST: this cannot be satisfied by adding a second
  hard-coded path beside the first and leaving the other four stages unchecked — that is the
  shape being removed.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 775, 852
- **Conflicts with:** 736, 738
- **Ordering note:** **RE-POINTED AGAIN 2026-10-02: the `741` slot is now #852.** #741 was
  split at its re-plan into #852 (the client core, which brings the aws SDK into
  `crates/validate`'s dependency set), #853 (non-conforming responses) and #854 (misbehaving
  upload peers). A split parent never reaches COMPLETE, so a surviving `Depends on: 741`
  would have made `_runnable` skip this bundle. Everywhere below that says "#741" about the
  dependency set, read **#852**; #853 and #854 add no dependency and share no file with this
  slice, so they are not prerequisites. One correction to the TLS note further down: #852
  uses the SDK's `default-client` feature, which brings `rustls-native-certs`,
  `rustls-pki-types` and, on Unix, `openssl-probe` into the normal graph (not the `rustls`
  crate itself). All three are pure Rust, so the "no native toolchain beyond the build stage"
  conclusion still holds; re-check it with `cargo tree -p wyrd-validate -e normal` on the base
  you get.
  **RE-POINTED 2026-08-18 (maintainer-approved): this was `Depends on:
  740, 741`; #740 was SPLIT and no longer builds anything.** #740 decomposed into #773 (the
  `h2`/RUSTSEC-2026-0258 lockfile bump) → #774 (creates `crates/validate` and its CLI) → #775
  (the no-`wyrd-*` dependency lint). A split parent never reaches COMPLETE, so a surviving
  `Depends on: 740` would have made `_runnable` (`flow.py:702`) skip this bundle outright.
  The `740` slot is now **#775**, the last link in that chain, which transitively carries
  #774's binary and #773's green base.
  Last in the batch. `Depends on 775` because the binary must
  exist to be built and staged; `Depends on 741` is a STRENGTHENING of the issue's own
  "Depends on #740" and is deliberate — #741 gives `crates/validate` its real dependency set
  (the aws SDK), and the point of this slice is that the *image build* compiles that crate,
  so proving the pipeline against the skeleton's dependency set would prove the wrong thing.
  `Conflicts with 736` (both edit `xtask/src/dist.rs` and `deploy/docker/wyrd/Dockerfile` —
  #736 adds the `WYRD_VERSION` build-arg and its `ARG`, this adds the second binary) and
  `Conflicts with 738` (both edit `xtask/tests/dist_templates.rs`). Both conflicts are
  already satisfied by the dependency path (#736 and #738 both land well before this), and
  are declared anyway so the constraint survives if the batch is ever re-run with a
  different membership. Re-verified with the driver's own scheduler after the #740 split and
  #736's re-point (2026-08-18): waves are
  `[773] → [736, 774] → [738, 775] → [741] → [742]`, and no conflicting pair shares one.
  **What the base already contains, so you extend rather than clobber it:** every earlier
  wave is
  merged into `main` before this builds, so `xtask/src/dist.rs` will ALREADY carry #736's
  `--build-arg WYRD_VERSION={version}` inside `obtain_binary` — the same function this slice
  reworks to extract two binaries — and `xtask/tests/dist_templates.rs` will already carry
  #738's `--chunk-size` entry in the s3 flag list. Read those files as they stand on your
  base, not as this brief's line numbers describe them on `65ca4fd`; a mechanical
  reapplication of the pre-batch shape would silently revert two accepted slices.
- **Surfaces:** data
- **Difficulty:** high
- **Sizing note:** expect the sizer to band this `oversized` (`difficulty=high`, brief
  length, two declared conflicts). Looked at, and not split. The blast radius is real and is
  exactly why the rating is `high` — five pipeline stages plus the release workflow — but
  every one of those edits is the SAME edit ("this stage names the binary set"), and they
  are only separable into children that each leave the pipeline internally inconsistent (an
  image carrying a binary nothing extracts; a tarball carrying a binary the installer
  ignores). A change whose parts are individually incoherent is one change.
- **Scope:** (a) the binary set becomes a pure data table in `xtask::dist` — in-image path →
  tarball destination — read by `obtain_binary` and `assemble`; (b) the Dockerfile builds
  and copies both binaries; (c) `dist` extracts both out of the image and stages both under
  `bin/`; (d) `install.sh` installs and uninstalls both; (e) `deploy/dist/README.md`
  distinguishes the roles binary from the validation tool; (f) `release.yml`'s smoke step
  covers both; (g) the container-free layout contract — the new red-earning text test (its
  assertions ITERATED from the table, per criterion 5) plus the pure-function and
  binary-staging assertions in `dist_templates.rs` (criterion 6); (h) the `--host` branch
  builds BOTH binaries — `cargo build --release --locked --bin wyrd --bin wyrd-validate
  --features …` — and returns both paths, so a `--host` tarball has the same contents as an
  image-built one. It does NOT refuse: removing the local-build path
  (`xtask/src/dist.rs:412-430`) is not this slice's to do, and a `--host` tarball silently
  missing the validator is the very bug class being removed. Assert it as a pure function:
  the cargo argv for the host branch is built from the table and named-both-bins is a
  container-free assertion like the rest.
  **/ out of scope:** making `wyrd-validate` a `wyrd` subcommand (it would share the roles'
  dependency closure and make #740's blackbox lint meaningless — the issue says so, and
  proposal 0017 §2 and §9 say so twice); a systemd unit for the validator (it is not a
  role — it is run by hand or by a launcher, and `install.sh`'s `ROLES` list must NOT grow
  an entry); a `/etc/wyrd/validate.env`; registry publication of the image (a named
  follow-up slice — the release still ships the OCI archive as a signed blob, not a `ghcr`
  push); multi-arch; the `tikv` flavor; changing what `wyrd-validate` DOES.
- **Repro instruction:** On `main` at `65ca4fd` in the target checkout, with #775 and #852
  applied: `grep -n "bin wyrd\|COPY --from=build" deploy/docker/wyrd/Dockerfile` → one build
  line, one copy, both naming only `wyrd`. `grep -n "bin/wyrd" deploy/dist/install.sh` →
  the install and uninstall each name one path. `grep -rn "IMAGE_BINARY_PATH"
  xtask/src/dist.rs` → a single `&'static str`. So a built tarball contains `bin/wyrd` and
  nothing else, and `cargo xtask ci` is green while the validator is unshippable.
- **External dependencies:** none. The binding criterion is entirely container-free — text
  assertions plus a pure function, inside `cargo xtask ci` — which is the point of restating
  it that way. A real image build needs a container runtime and is the release workflow's
  job; Do MUST NOT attempt one, and MUST NOT weaken the criterion into something that
  requires one.
- **Test file:** `xtask/tests/dist_two_binary_layout.rs` — a NEW file. (Confirmed against
  this instance's gate by dry-running `engine/scripts/run-verify.sh --classify`: a new file
  under `xtask/tests/` is classified `ADDED_TEST`, so the patch earns the full red→green
  leg; assertions appended to the existing `dist_templates.rs` would not.)
- **Verification posture:** DECLARED. The binding criterion is a genuine flippable test —
  red pre-fix, green post-fix, at Check — which is unusual for a packaging slice and is why
  the criterion was restated around what the container-free tier can observe. One half is
  deferred and is named here so it lands as a pre-declared sign-off item rather than a
  surprise NEEDS-HUMAN:
  * BUILT AND EXERCISED AT CHECK: the pure binary-set table; the Rust consumers that read it
    (`obtain_binary`'s extraction list, the `--host` argv, and the staging step exercised for
    real over dummy binaries in a tempdir — criterion 6); and every pipeline file pinned to
    it (assertions ITERATED from the table over the real Dockerfile, installer, README and
    workflow); `dist --check` still validating templates and tokens; the whole
    `cargo xtask ci`.
  * DEFERRED, and to WHOM: that a real tarball CONTAINS both binaries and that `install.sh`
    places both on a real host. `xtask/tests/dist_templates.rs` is "container-free by
    design … with the real build deferred to the release workflow" (`:5-9`), and
    `.github/workflows/release.yml` already builds the tarball and installs it in a clean
    bookworm container (`:57-88`). Extending that step to cover the second binary is IN
    scope here; observing it run is the release workflow's, on the next `v*` tag. Say so in
    `build-notes.md`; do not claim it as demonstrated.
  * WHY IT IS DEFERRED RATHER THAN CHOSEN AWAY — checked, not assumed. Building a tarball
    inside `ci` needs Docker and a network (`xtask/src/dist.rs:26-28`, which is why `dist` is
    deliberately not a `ci` step). And `install.sh` cannot be executed by ANY test on the
    gate host: it exits non-zero unless `id -u` is 0 (`deploy/dist/install.sh:90`), installs
    into `$PREFIX/bin`, writes `/etc/wyrd/*`, stages systemd units and reads
    `/etc/wyrd/install-prefix` on uninstall. `--prefix` relocates the binaries but not
    `CONFDIR`. So "install.sh places both" is unobservable at Check by construction; what
    replaces it is the iterated text contract over both of its sites plus the release smoke
    step, and `shellcheck deploy/dist/install.sh` (`release.yml:41-42`) keeps the script
    itself honest.
  * **THE SIGN-OFF ITEM, stated so it is accepted rather than assumed:** at §9 the human is
    accepting that the tracker's literal definition of done ("the tarball contains both
    binaries; `install.sh` places both") is NOT observed in this cycle, and that what is
    observed instead is: the table, its Rust consumers including real staging, and every
    pipeline file pinned to it. If that trade is not acceptable, the remedy is not a weaker
    test here — it is to cut a `v*` tag (or run the release workflow via its
    `workflow_dispatch`, `release.yml:23`) and observe it there, which is a decision only the
    maintainer can take. Do must present it that way in `build-notes.md` and not paper over
    it.
  * The deferred half is a verification gap, not an unbuilt deliverable — the pipeline
    change is fully written in this slice and is exercised by the assertions above.
- **Production reach:** Declared, and it is the honest limit of this slice. At Check nothing
  builds an image, so the claim "the extracted binary is bit-identical to the image's" is
  carried by the pipeline's STRUCTURE (both binaries come out of the same `docker cp` from
  the same build) rather than observed. What honours the criterion at Check is the text and
  data contract; what honours it in production is the release workflow. There is no test
  double standing in for a real build, and none should be invented — a fake tarball proves
  nothing that the text assertions do not already prove.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  Peer callsites Do MAY open and should mirror:
  * **The single-source pattern to copy** — `xtask/src/dist.rs:196-260` (`staging_plan()`
    returning `Vec<StagedFile>`, a pure data table) and its repo-coupled assertion
    `xtask/tests/dist_templates.rs:339-380` (`the_staging_plan_matches_the_repo`: every
    source is a real file, the destination list is pinned). The binary table is the same
    idea for binaries; write it the same way and assert it the same way.
  * **The extraction and staging to extend** — `xtask/src/dist.rs:414-511`
    (`obtain_binary`: the `--host` cargo branch, the buildx invocation, `docker create` +
    `docker cp` of `IMAGE_BINARY_PATH`, and the `docker rm -f` cleanup that must run
    whatever happens) and `:520-572` (`assemble`: the staging loop, then the explicit
    `bin/wyrd` copy at `:559-564` and the `VERSION` file at `:566-571`). Note the binary is
    NOT part of `staging_plan()` — it is copied separately, which is precisely why a second
    one needs a table rather than a second hard-coded line.
  * **The Dockerfile build and copy** — `deploy/docker/wyrd/Dockerfile:66`
    (`RUN cargo build --release --locked --bin wyrd ${FEATURES:+--features "$FEATURES"}`)
    and `:122` (`COPY --from=build /src/target/release/wyrd /usr/local/bin/wyrd`), with the
    runtime-stage description at `:68` that stops being true and must be updated.
  * **The installer's two sites** — `deploy/dist/install.sh`: `install -m 0755
    "$HERE/bin/wyrd" "$BINDIR/wyrd"` in the install path, and `rm -f "$BINDIR/wyrd"` in the
    uninstall path. Both must cover the set. The `ROLES` list is NOT the place — see Design
    §4.
  * **The installer's contract test** — `xtask/tests/dist_templates.rs:205-239`
    (`install_sh_keeps_its_contract`, a list of required substrings) and `:245-259`
    (`install_sh_never_enables_or_starts_units`, which must keep passing — the validator
    gets no unit and no `systemctl`).
  * **The release smoke step** — `.github/workflows/release.yml:59-88`, in particular the
    usage assertion at `:75` and the post-uninstall absence assertion at `:85`.
  * **The README sentence that stays true** — `deploy/dist/README.md:1-8` ("One `wyrd`
    binary serves every role as a subcommand"). It describes Wyrd's ROLES and the validation
    tool is deliberately not one, so the sentence is kept and the second binary is
    introduced beside it, not by rewriting it. The issue is explicit about this.
- **Prior-art check (triage cycles):** searched by affected path on `main` at `65ca4fd`.
  `git log --oneline -- xtask/src/dist.rs` → one commit, `f5d4575` ("dist(570): one pipeline
  ships the operator tarball and the OCI image") — the pipeline has never been modified
  since it was written, and never for a second binary. `gh pr list --state all --search
  "wyrd-validate"` → only PR #765, the merged proposal 0017 document. No open or closed PR
  has attempted two-binary packaging; there is no superseded attempt to avoid repeating.
- **Disposition hint:** new-feature

## Motivation

`wyrd-validate` is release content, not merely a test of the release. Its two audiences ask
two questions with the same tool — *we* ask "is Wyrd correct", an operator asks "is my
hardware and configuration sound" — and the second audience can only ask it if the tool is
in the box they installed. Proposal 0017 §2 states it plainly: the tool "**ships in the
operator tarball**", which makes it the tarball's second binary.

The deeper reason to do it as a *table* rather than a second hard-coded path is the one the
issue's own review exposed: this pipeline names its binary in five independent places, so
"add a binary" is a five-file change with five chances to miss one, and the failure mode is
silent — a tarball that installs a binary the uninstaller leaves behind, or a README that
describes an artifact that is not there.

## Design

### 1. The settled decision: the image carries both

**Maintainer decision, 2026-08-17 — option A.** `deploy/docker/wyrd/Dockerfile` builds and
carries both binaries; `cargo xtask dist` extracts both out of that one image.

This is the option that changes the *least* about what the pipeline IS. `cargo xtask dist`
has exactly one way to obtain a shipped binary — extract it from the image, "so the
tarball's `bin/wyrd` is bit-identical to the image's `/usr/local/bin/wyrd`"
(`xtask/src/dist.rs:5-7`) — plus the `--host` escape hatch that deliberately forfeits the
bookworm glibc floor. Keeping the image as the single build vehicle preserves that
guarantee and extends it to the second binary, at the cost of one more binary in the
production image. `ENTRYPOINT ["wyrd"]` is unchanged, so nothing about how the image is
*run* changes; the validator is present, not invoked.

The alternatives and why they lost are in *Alternatives considered*; the full deliberation,
including the facts checked to reach it, is in `PACKAGING-DECISION.md` beside this brief.

One fact verified rather than assumed, because it is the obvious place this design could
have broken: a single multi-`--bin` build across two workspace packages, with `--features`,
is fine. Checked on this checkout — `cargo build --release --locked --bin wyrd --bin xtask
--features "fdb,etcd"` exits 0 (`xtask` is a featureless second package, structurally what
`wyrd-validate` will be). So `:66` becomes one line naming both bins and needs no second
`RUN` layer.

### 2. The binary set becomes data

Today `IMAGE_BINARY_PATH` (`dist.rs:40`) is a lone `&'static str` and `assemble` hard-codes
`bin/wyrd` (`:559`). Replace both with one pure function in the `staging_plan()` style —
each entry pairing the path inside the image with the destination under the tarball's
`bin/`, plus whatever the installer and the smoke step need to name it.

`obtain_binary` then extracts each entry (one `docker create`, N `docker cp`s — the
throwaway container is created once and removed once, and the existing "remove the
container regardless of the cp outcome" discipline at `dist.rs:503-509` must cover all of
them), `assemble` copies each into place through a `pub` staging callable a test can drive
(criterion 6 — today those four lines are private and unreachable), and the tests assert
that the pipeline files agree with the table.

Say plainly what "agree" means, because the invariant above turns on it: the Dockerfile, the
installer, the workflow and the README each keep their own literal spelling of the binary
names — nothing generates them — and the gate compares those spellings to the table on every
`cargo xtask ci`. That is checked duplication, which is what the repo already does for
`FDB_VERSION`, and it is the whole of what this slice claims.

Keep the `--host` branch honest too, and the choice is made here rather than left to Do
(scope (h)): it **builds and returns both binaries** — one `cargo build --release --locked
--bin wyrd --bin wyrd-validate --features …`, both paths returned from the table — and it
does **not** refuse. Refusing would delete a documented local-build path
(`xtask/src/dist.rs:412-430`, the developer escape hatch that "forfeits the bookworm glibc
floor" deliberately) for no benefit to this slice's outcome; a `--host` tarball silently
missing the validator is the same class of bug as the one being removed, and building both
fixes it directly. The argv is built from the same table and asserted container-free, so the
branch cannot drift back to one bin.

### 3. What the container-free tier can actually pin — the restated definition of done

The issue's original DoD ("the tarball contains both binaries; `install.sh` places both")
is unverifiable by the gate it names, and the review said so.
`xtask/tests/dist_templates.rs` is "container-free by design … every template assertion is a
file read + substring check" (`:5-9`); nothing in `cargo xtask ci` can observe a tarball.

So the criterion is restated over what IS observable: the pure table, and every pipeline
file that must agree with it. That is not a weaker claim than the original in the way that
matters — a tarball can only contain what the Dockerfile built, `dist` extracted and
`assemble` staged, and all three now read one table that the test pins. What genuinely
cannot be checked here — that `tar` and `install -m 0755` did their jobs on a real host —
is precisely what `.github/workflows/release.yml:59-88` already exists to check, and this
slice extends that step to the second binary.

**Split the assertions across two files, deliberately.** The new
`xtask/tests/dist_two_binary_layout.rs` asserts over FILE TEXT only, iterating a LOCAL
expected-binaries constant, so it compiles against a tree with the production change
reverted and therefore earns a real red→green (see Falsifiability). Everything that calls
new `xtask::dist` API goes in the existing `dist_templates.rs`, which owes no red: the
table's own shape, the staging callable over dummy binaries with distinct contents, the
distinct extraction paths, the `--host` argv, and — the join between the two files — the
red file's checker run again over the production table, plus the assertion that the text
test's local constant equals the production table. Do not merge them into one file; doing
so costs the per-fix red. Do not omit the production-table checker run or the equality
assertion either; without them the "declared once" half is a comment.

### 4. The installer, and the thing it must not do

`install.sh` installs from `$HERE/bin/<name>` and removes `$BINDIR/<name>`, for each entry.
Both sites, or `--uninstall` leaves a binary behind — and the release smoke step asserts
absence after uninstall (`release.yml:85`), so a half-done job fails the release rather than
CI, which is the expensive place to find it.

**`ROLES` must not grow an entry.** That list drives systemd units, `/etc/wyrd/<role>.env`
files and `systemctl disable` — and `wyrd-validate` is deliberately not a role: it is not
long-running, not supervised, and has no config file. It gets a binary and nothing else.
`install_sh_never_enables_or_starts_units` (`dist_templates.rs:245-259`) must keep passing
unchanged.

Two smaller things that follow: the `s3.env` credential chown/chmod block is role-specific
and untouched; and the `libfdb_c` preflight warning is about the roles binary, so it stays
as it is — the validator does not link `libfdb_c`, which is worth a sentence in the README
because it means the validator runs on a host with no FoundationDB client at all.

### 5. README

Keep the opening sentence. "One `wyrd` binary serves every role as a subcommand" describes
Wyrd's *roles*, and the validation tool is deliberately not one of them — the issue makes
that point explicitly, and it is the same reasoning that keeps the tool out of the `wyrd`
subcommand surface. Add the second binary beside it: what it is for, that it is optional,
that it is blackbox by construction, and that it needs no `libfdb_c`.

## Alternatives considered

**B — image carries only `wyrd`; extract the validator from the build stage.** Keeps the
production image minimal, and costs the property the pipeline is built around: `dist` would
need a second way to obtain a binary (a `--target build` export, or a second build), the two
artifacts would need separate assembly paths, and the tarball's second binary would no
longer be bit-identical to anything an operator can run. Rejected by the maintainer in
favour of A.

**C — build the validator with the host `cargo`.** Forfeits the bookworm glibc floor and the
"packaging host needs none of the fdb/etcd build toolchain" property, and ships a tarball
whose two binaries have different libc provenance. Rejected on its face.

**D — do not ship it in the tarball yet.** Defensible on sequencing grounds, and it was the
reason this slice was initially set aside: the maintainer's own objection on #742 is that the
current order would "ship a stub to operators" (`notes.json`, eduralph, 2026-08-16). That
objection is **real, not hypothetical**, and this brief does not claim otherwise. Landing
this slice is chosen over D, and the choice rests on a human checkpoint, not on a gate:

* **Nothing mechanical stops a stub from shipping.** The release workflow runs on any `v*`
  push (`.github/workflows/release.yml:20-23`), builds with `cargo xtask dist --oci-archive`
  (`:53-54`), and publishes with the tag as its only condition (`:106-115`). After this
  slice lands and before #743 adds `smoke`, `wyrd-validate` echoes its configuration and
  exits (#852 leaves it that way), so a `v*` tag cut in that window ships exactly the stub
  the objection describes.
* **What prevents it is the maintainer, at the tag.** Proposal 0017 says plainly that no
  machine gate exists and names a **release-runbook step** instead: the tag procedure
  requires the committed 7-day endurance verdict, substrate `hetzner`, scenario `endurance`
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`). That verdict
  cannot exist until the tool works. The step is a human one, and today it is written down
  only in that draft proposal: there is no release runbook file in the target yet
  (`git ls-files | grep -i releas` finds only `release.yml` and ADR-0030). Milestone 17 and
  #735's "0.1 Alpha does not tag without it" record the same intent; they do not enforce it.
* **Facts that bound the risk today, not guarantees.** No `v*` tag has ever been cut (the
  repo's only tag is `archive/backup-premerge-signoff`), and `workflow_dispatch` builds the
  artifacts but does not publish them (the publish step needs a tag).
* **SIGN-OFF ITEM.** At §9 the maintainer accepts, explicitly, that packaging lands before
  the tool works, and that from this merge until #743 (and the endurance verdict) the only
  thing standing between a `v*` tag and a stub on operators' hosts is the maintainer not
  cutting one. If that is not acceptable, the remedy is D itself: reject this slice back to
  Plan and re-point it after #743. It is not a reason to weaken the pipeline change.

The benefit of landing now, given that acceptance: the two-binary layout is under gate for
every slice that follows, rather than being retrofitted next to a release deadline.

**Make it a `wyrd validate` subcommand.** Out of scope and rejected twice already: it would
share the roles' dependency closure and make #740's lint meaningless.

**Give the validator a systemd unit.** It is not a long-running supervised role. A unit
would also drag in a config file, a `/etc/wyrd/validate.env`, and an entry in the uninstall
sweep, for a tool that is run by hand or by a launcher.

## Impact & compatibility

The production image grows by one binary. That is the accepted cost of option A, and the
posture question it raises deserves recording rather than burying: the image now carries a
tool that **deletes objects**. Three things bound it, and Do should state them in
`build-notes.md` rather than leave a reviewer to reconstruct them — it is not the
`ENTRYPOINT`, it refuses to run without an explicit `--endpoint` and credentials, and
run-id-scoped keys are a stated safety requirement (proposal 0017 §15: "it must never delete
anything it did not create"). If the maintainer later wants the image slimmed, the table
from §2 is where that change STARTS — and the gate then names the Dockerfile, installer,
README and workflow lines that must follow, which is the whole benefit being bought here.

The tarball grows by one binary and its `VERSION` file is unchanged in shape. The installer
gains one install and one removal. No existing behaviour changes for an operator who ignores
the new binary: no new unit, no new config file, nothing enabled, nothing started.

`cargo xtask dist --check` keeps working unchanged — it validates templates and tokens and
builds nothing, so it never touches the binary table.

One consequence worth flagging at review: the image build now compiles `crates/validate`,
which after #741 pulls the aws SDK. Checked so it is not a surprise — that tree needs no
native toolchain beyond what the build stage already installs (the SDK as pinned resolves no
TLS stack; `cargo tree -i hyper-rustls -e normal,dev` finds nothing, and the `ring`/`rustls`
entries in `Cargo.lock` arrive only via `tikv-client` under the off-by-default `tikv`
feature). If Do finds otherwise, that is a finding to report, not to work around.

## Plan-review response (#301 revision pass)

Four findings; three revised, one kept with its provenance corrected and made a sign-off item.

* **"The decisive premise is not in the tracker."** Correct as to the record, and the header
  now says exactly where the decision lives (session, 2026-08-17, plus
  `PACKAGING-DECISION.md`), that issue #742's only comment POSES the question, and that
  proposal 0017 assigns it to the release-pipeline owner — who is the person who answered it.
  Sign-off must confirm option A and should mirror it onto the issue; an unconfirmed decision
  rejects the slice back to Plan rather than being patched into option B.
* **"One Rust table cannot be read by Docker, shell, YAML and Markdown."** Correct, and the
  invariant as written was literally impossible. It is narrowed to what the FDB-pin precedent
  actually achieves — *declared once, duplication checked by the gate* — with the reason
  spelled out (a generation mechanism would be a separate, larger architectural change and is
  not being smuggled in here). Criterion 5 now says the assertions are ITERATED from the
  table, so a third entry makes every stage demand it and the gate names the files that
  disagree; adding a binary stays a multi-file edit, and that is stated rather than denied.
* **"The binding criterion does not demonstrate the tracker's DoD."** Correct — and checked
  further rather than argued away: `install.sh` cannot be executed by any test on the gate
  host (`install.sh:90` refuses non-root, and it writes `/etc/wyrd`, users and units), and a
  tarball needs Docker (`dist.rs:26-28`). Two changes follow. Criterion 6 adds a NON-lexical
  present-run observable — the real binary-staging step, extracted and exercised over dummy
  binaries in a tempdir, asserting `bin/wyrd` and `bin/wyrd-validate` at `0755`. And the
  residual is now an explicit §9 acceptance ("the literal DoD is not observed this cycle;
  here is what is"), with the only real alternative named: run the release workflow
  (`workflow_dispatch`, `release.yml:23`) or cut a tag — a maintainer's call, not Do's.
* **"`--host` is left ambiguous."** Correct; chosen. Scope (h): `--host` BUILDS both
  (`--bin wyrd --bin wyrd-validate`) and returns both paths, it does not refuse, its argv
  comes from the same table, and that argv is asserted container-free. Removing the
  local-build path was never this slice's to do.

## Plan-review response (#301 revision pass, round 2, 2026-10-02)

Three findings; all three revised. None changes the slice's scope or the option-A decision.

* **"Criterion 5 promises coverage the test split does not give."** Correct: with the file
  checks iterating only the red file's fixed constant, a third production entry tripped the
  equality check alone. Criterion 5 now requires ONE checker, run twice — on the local set in
  the red file, and on the production table in `dist_templates.rs` through a module include
  of the red file — and states the diagnostic exactly (every file lacking the new binary is
  named, and the equality check says to update the local set). The red leg is unaffected:
  `dist_templates.rs` is a modified file and is reverted on it (`run-verify.sh:510-517`).
* **"The staging test passes with the wrong binary copied to both places."** Correct.
  Criterion 6 now uses distinct dummy contents and asserts each staged file byte-for-byte
  against its own source, and requires the host extraction paths to come from a pure
  function of the table, asserted pairwise distinct. Still container-free.
* **"The sequencing answer overstates release enforcement."** Correct, and re-checked:
  `release.yml` publishes on any `v*` tag with no other condition, proposal 0017 names a
  human runbook step and says no machine gate exists (`0017:1013-1019`), and no runbook file
  exists in the target yet. Alternative D is rewritten to say so, to name the maintainer at
  the tag as the checkpoint and the committed endurance verdict as what that step requires,
  and to make "packaging lands before the tool works" an explicit §9 acceptance with D as
  the fallback if it is refused.

## Open questions

1. **Does the validator belong in `SHA256SUMS`/provenance separately?** Today the release
   signs and attests the tarball and the OCI archive as whole blobs, so both binaries are
   covered transitively. Naming them individually is a possible refinement, not this slice's.
2. **Should `install.sh --uninstall` warn if the validator is running?** It cannot be, in
   any supervised sense — no unit — so probably not. Mentioned so it is a decision rather
   than an oversight.
3. **Registry publication.** The release ships the image as a signed OCI *archive*; a `ghcr`
   push is a named follow-up ("signing the registry digest instead",
   `release.yml:10-12`). Unaffected by this slice, but the two-binary image is what that
   slice will publish.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring real image build, extraction, tarball assembly, and privileged install/uninstall to the release workflow, or require that run before sign-off — those dependencies were not exercised here; evidence is file contracts plus real staging of dummy bytes (brief.md:237; .github/workflows/release.yml:54; pdca-reviewer-742-evidence/coverage-summary.txt:3).; **The release-workflow checker ignores comments and order.** `xtask/tests/dist_two_binary_layout.rs:194-196` treats any line in the smoke step that contains `/usr/local/bin/<name>` as a token as proof the binary gets run, and that includes a comment line. `:202` only checks that `test ! -e /usr/local/bin/<name>` appears somewhere in the step, even though its failure message says "after --uninstall". I ran two concrete false greens in a scratch copy, and all 24 tests passed in both: (a) replace the whole validator invocation at `.github/workflows/release.yml:74-77` with `# TODO smoke /usr/local/bin/wyrd-validate later`; (b) move `test ! -e /usr/local/bin/wyrd-validate` from `:93` to just after `cd /tmp/wyrd-*` (`:69`), before `./install.sh`. The release run would also pass, so the absence check would quietly stop proving anything. Fix: skip lines starting with `#`, and require each absence line to come after the `./install.sh --uninstall` line. The install.sh split at `:127` has the same weakness at lower risk: anything above the `# ── install` banner counts as "the uninstall path".; **The brief's "BUILT AND EXERCISED AT CHECK" list includes "`obtain_binary`'s extraction list", but no test exercises it.** No test reaches `extract_binaries` (`xtask/src/dist.rs:593-612`, the `docker cp` loop the release actually uses) or the `assemble` call at `:656` (C4-diff-cov MISS 585-609, 656). Concrete surviving mutation: change `:656` to `stage_binaries(&shipped_binaries()[..1], …)` and all 24 tests still pass. Only the path helper (`:306`) and the `--host` argv (`:312`) are pinned. Every break of this kind I could construct fails loudly at release time, not silently. `stage_binaries` errors on a missing source. A swapped `image_path` is caught by the smoke step, because `wyrd` cannot load before libfdb_c is installed. But that only happens on a `v*` tag, which is the expensive place to find it. Cheap fix in the `host_build_args` style: a pure function that returns the `docker cp` argv for each binary (`<cid>:<image_path>` → `binary_source_path(extracted, b)`), checked without a container.; `xtask/tests/dist_two_binary_layout.rs:194`: The release smoke checker counts comments and command arguments as binary execution. Commenting out the entire `wyrd-validate` invocation and usage-check block in `release.yml` still satisfies this predicate: `# if /usr/local/bin/wyrd-validate ...` contains the required token. I reproduced this in memory; deleting the block instead correctly reports the missing invocation. Consequently, the layout tests can remain green after the validator’s runtime smoke coverage disappears. Ignore comments, require the binary in command position, and add a negative test for a commented-out smoke block.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 74 instrumentable changed lines executed (below the 80% floor); 74 of 137 changed lines were i
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring the real release build and installation, or require that evidence before sign-off — Docker/buildx extraction and privileged bookworm install/uninstall were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and a host-built CLI usage check (`brief.md:237`, `.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/validator-usage.log:1`).; `deploy/dist/install.sh:202`: the patch dropped a space from the `units` label (` units ` → ` units `, diff line 162). In the operator-facing install summary, the units path now starts one column left of the `binary` / `tool` / `config` / `data` rows at `:200-204`. Before this diff all five rows lined up. One-character fix: restore the fourth space.. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 113 instrumentable changed lines executed (below the 80% floor); 113 of 201 changed lines were
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring the real release build/install or require that evidence before sign-off—Docker/buildx, image extraction, archive creation, and privileged bookworm install/uninstall were not exercised; compiled contracts and real staging of dummy bytes establish a narrower result (.github/workflows/release.yml:54; xtask/tests/dist_templates.rs:1042; pdca-reviewer-742-evidence/mutants-lint-capped.log:5).; **The release-smoke checker pins the invocation but not the assertion on its result, so the smoke can stop proving anything while every test stays green.** `xtask/tests/dist_two_binary_layout.rs:351-355` counts a binary as "run with its exit status checked" when it is first in command position and its line has no `||`/`&&`. It never looks at the `grep -q 'usage: …'` lines (`.github/workflows/release.yml:77`, `:85`), and those greps are what actually fail the smoke. Without them, `if /usr/local/bin/wyrd-validate …; then exit 1; fi` passes when the binary cannot run at all (a loader error or wrong arch exits 127, so the `if` branch is skipped). I ran four mutations in a scratch copy, and all 29 tests in `dist_templates` + `dist_two_binary_layout` passed for each one: (1) delete `release.yml:77`; (2) delete `:77` and `:85` and turn both `exit 1` bodies into `:`; (3) replace `:74-77` with `/usr/local/bin/wyrd-validate 2>&1 | cat >/dev/null`; (4) replace `:74-77` with `/usr/local/bin/wyrd-validate &`. So the failure message at `:359` ("with its exit status checked") claims more than the check does, and the negative cases at `xtask/tests/dist_templates.rs:658` only try `|| true` and `&& echo ok`. Cheapest fix that closes the case that matters: for each binary, also require a `grep -q 'usage: <name>'` (or an equivalent output assertion) after the invocation and before `./install.sh --uninstall`, and add mutation (1) as a negative case. Treating a pipeline (`|`) or background (`&`) as "not run" is a one-line addition. Low severity: today's `release.yml` is correct. The gap only matters if a later edit weakens the smoke, but catching that kind of drift is this checker's whole job.. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 113 instrumentable changed lines executed (below the 80% floor); 113 of 201 changed lines were
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring real image build/extraction, archive creation, and privileged install/uninstall to release, or require that run before sign-off — Docker/buildx and the installation environment were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and host CLI checks (`.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/coverage-summary.txt:16`, `pdca-reviewer-742-evidence/extra-checks.log:11`).. 10 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 113 instrumentable changed lines executed (below the 80% floor); 113 of 201 changed lines were
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the patch breaks the release smoke step it extends, and the new layout test passes on the broken file. Fix in place, same plan: 1. .github/workflows/release.yml (~:76): the comment `would read as "refused correctly"` has bare double quotes inside the outer `sh -eu -c "..."` string. They end the script early, so every check after them (validator smoke, wyrd usage smoke, reinstall check, all uninstall assertions) never runs and the step still goes green. Use single quotes or drop the quotes. Do the same in the fdb-image.yml comment for consistency. 2. xtask/tests/dist_two_binary_layout.rs release_smoke_disagreements: model the outer `sh -c "` boundary. An unescaped `"` before the closing line must count as the end of the script, so the checker reports every check after it as never run. Add a planted regression in dist_templates.rs: a comment line with "quoted" words placed before the validator invocation must make the checker name wyrd-validate. 3. .github/workflows/fdb-image.yml: add `crates/validate/**` to the pull_request path filters, since the new smoke greps the validator's usage line. Pin it in the workflow contract test. 4. xtask/src/dist.rs (~:625): prepare and clean the extraction directory BEFORE `docker create`, or put every fallible step after create under guaranteed best-effort `docker rm`, so a failed cleanup can't leak containers. Maintainer questions still open (not decided this round, do not assume an answer): reconfirm option A / packaging before #743 with no early v* tag; README :17 present-tense claim about what wyrd-validate checks; whether a manual release.yml workflow_dispatch is required as real-artifact proof.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rejected: the patch breaks the release smoke step it extends, and the new layout test passes on the broken file. Fix in place, same plan:
  1. .github/workflows/release.yml (~:76): the comment `would read as "refused correctly"` has bare double quotes inside the outer `sh -eu -c "..."` string. They end the script early, so every check after them (validator smoke, wyrd usage smoke, reinstall check, all uninstall assertions) never runs and the step still goes green. Use single quotes or drop the quotes. Do the same in the fdb-image.yml comment for consistency.
  2. xtask/tests/dist_two_binary_layout.rs release_smoke_disagreements: model the outer `sh -c "` boundary. An unescaped `"` before the closing line must count as the end of the script, so the checker reports every check after it as never run. Add a planted regression in dist_templates.rs: a comment line with "quoted" words placed before the validator invocation must make the checker name wyrd-validate.
  3. .github/workflows/fdb-image.yml: add `crates/validate/**` to the pull_request path filters, since the new smoke greps the validator's usage line. Pin it in the workflow contract test.
  4. xtask/src/dist.rs (~:625): prepare and clean the extraction directory BEFORE `docker create`, or put every fallible step after create under guaranteed best-effort `docker rm`, so a failed cleanup can't leak containers.
  Maintainer questions still open (not decided this round, do not assume an answer): reconfirm option A / packaging before #743 with no early v* tag; README :17 present-tense claim about what wyrd-validate checks; whether a manual release.yml workflow_dispatch is required as real-artifact proof.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 77 instrumentable changed lines executed (below the 80% floor); 77 of 165 changed lines were i
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — C5 Causal adequacy — Repair the usage-assertion checker — it accepts suppressed or inverted greps, allowing a loader failure to pass the smoke while the layout checker reports no disagreement (`xtask/tests/dist_two_binary_layout.rs:595`; `pdca-reviewer-742-work/checker-probes.log:2`, `pdca-reviewer-742-work/checker-probes.log:16`).; T3 Runtime — Accept deferring real artifact proof or require a release-workflow run before sign-off — Docker/buildx, image extraction and privileged installation were not exercised; evidence rests on compiled contracts, staging dummy bytes and a host CLI invocation (`.github/workflows/release.yml:54`, `xtask/tests/dist_templates.rs:625`; `pdca-reviewer-742-work/mutants-capped.log:3`).. 12 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 81 instrumentable changed lines executed (below the 80% floor); 81 of 177 changed lines were i
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Production change is accepted as sound; only the test design changes. - Replace the hand-written shell-quoting/statement-shape model in xtask/tests/dist_two_binary_layout.rs (and its 70+ planted-drift cases in dist_templates.rs) with the cheaper exact-text pin already used for fdb-image.yml (xtask/tests/fdb_image.rs:358): pin the release.yml smoke step as exact expected text, with the per-binary block generated from the shipped-binary table. This closes the `set -n` / `set -o noexec` false green and every similar hole by construction, and cuts the test weight (~2,300 test lines vs ~180 production). - Do NOT forbid `for` loops, `if` or groups in the release smoke step — banning the natural way to iterate the binary set is wrong. A loop over the binaries is fine. - Apply the same exact-text approach to the install.sh install/uninstall lines so `|| true` or an `if [ -f … ]` guard around the validator install cannot pass silently. - Keep: the binary table in xtask::dist, the shared checker over both the local and production sets, byte-for-byte staging test, distinct-path assertions, the red-earning text test. - Settled at sign-off, do not revisit: shipping before #743 is OK (nothing released yet); README present-tense wording stays; validator in the production image (option A) is fine; real image/tarball/install proof comes from fdb-image.yml on the PR and a manual release.yml run before first release.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production change is accepted as sound; only the test design changes.
  - Replace the hand-written shell-quoting/statement-shape model in xtask/tests/dist_two_binary_layout.rs (and its 70+ planted-drift cases in dist_templates.rs) with the cheaper exact-text pin already used for fdb-image.yml (xtask/tests/fdb_image.rs:358): pin the release.yml smoke step as exact expected text, with the per-binary block generated from the shipped-binary table. This closes the `set -n` / `set -o noexec` false green and every similar hole by construction, and cuts the test weight (~2,300 test lines vs ~180 production).
  - Do NOT forbid `for` loops, `if` or groups in the release smoke step — banning the natural way to iterate the binary set is wrong. A loop over the binaries is fine.
  - Apply the same exact-text approach to the install.sh install/uninstall lines so `|| true` or an `if [ -f … ]` guard around the validator install cannot pass silently.
  - Keep: the binary table in xtask::dist, the shared checker over both the local and production sets, byte-for-byte staging test, distinct-path assertions, the red-earning text test.
  - Settled at sign-off, do not revisit: shipping before #743 is OK (nothing released yet); README present-tense wording stays; validator in the production image (option A) is fine; real image/tarball/install proof comes from fdb-image.yml on the PR and a manual release.yml run before first release.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 0.0% — 0 of 81 instrumentable changed lines executed (below the 80% floor); 81 of 177 changed lines were i
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: shellcheck is now installed on the host; run it on deploy/dist/install.sh and change nothing else unless it or Check finds something. The v7 rework (exact-text pins) stands; production change and v7 sign-off decisions are settled.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
