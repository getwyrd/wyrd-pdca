# issue 742 — the packaging decision, and how it was reached

**Issue:** getwyrd/wyrd#742 — *dist: ship wyrd-validate in the operator tarball*
**Question:** does the production OCI image carry the validation tool?
**Answer:** **YES — option A.** The image builds and carries both binaries; `cargo xtask
dist` extracts both out of that one image. Maintainer decision, 2026-08-17, in the Plan
session for the batch 736 / 738 / 740 / 741 / 742.
**Consequence:** #742 is briefable and IS briefed — see `brief.md` beside this file, whose
Design §1 implements this decision. This slice was set aside for the first part of that
session and un-blocked by the answer.

This file records the deliberation, so the reasoning survives the session and a future
reader does not have to re-derive it. It is not a driver artifact.

## 1. Why an answer was needed first — the maintainer's own words

From the plan review the maintainer left on the issue (2026-08-16):

> **And a decision nobody has posed: does the production OCI image now carry a validation
> tool?** Shipping in the tarball but not the image is defensible — operators install from
> the tarball — but it breaks the bit-identical guarantee dist relies on, so the artifacts
> need separate assembly paths. **That is a release-pipeline call, and this slice cannot
> proceed without it.**

Verified against the target checkout (`main` @ `65ca4fd`), the constraint is real and not
merely stylistic:

* `cargo xtask dist` does **not** assemble a tarball from build outputs. It builds the
  production image and extracts the binary out of it (`docker create` + `docker cp`), "so
  the tarball's `bin/wyrd` is bit-identical to the image's `/usr/local/bin/wyrd`"
  (`xtask/src/dist.rs:5-7`, and the extraction at `:496-510`).
* `deploy/docker/wyrd/Dockerfile` builds `--bin wyrd` (`:66`), describes its runtime stage
  as hosting "just the `wyrd` binary" (`:68`), and copies exactly one path (`:122`).
* `deploy/dist/install.sh` installs one path and uninstalls one path.
* `.github/workflows/release.yml` smoke-tests `/usr/local/bin/wyrd` (`:75`) and asserts its
  absence after uninstall (`:85`).

So a second binary is a change to the **pipeline** — Dockerfile, extraction, installer,
uninstaller and release workflow — and the choice of *whether the image carries it* decides
whether one assembly path still serves both artifacts. Writing a brief that picks one
silently would be Plan overriding a decision the maintainer explicitly reserved, three
hours before this session.

## 2. The options, and the one chosen

| | What changes | Cost | Consequence |
|---|---|---|---|
| **A — chosen** | Dockerfile builds `--bin wyrd --bin wyrd-validate` and copies both; `dist` extracts both | ~6 lines in the pipeline + installer / uninstaller / release-smoke updates | One assembly path; the bit-identical guarantee extends to both binaries; the image grows one binary; `ENTRYPOINT ["wyrd"]` unchanged |
| B | Image carries only `wyrd`; extract the validator from the build stage (`--target build` export, or a second build) | Real new machinery in `dist` | Two assembly paths; the tarball's second binary is no longer bit-identical to anything an operator can run |
| C | Build the validator with the host `cargo` | Small | Forfeits the bookworm glibc floor; a tarball whose two binaries have different libc provenance. Rejected on its face |
| D | Do not ship it in the tarball yet | Zero | Defer #742; operators get the tool later |

**A was chosen** because it changes the least about what the pipeline *is*. `dist` has
exactly one way to obtain a shipped binary — extract it from the image — plus the `--host`
escape hatch that deliberately forfeits the glibc floor. Keeping the image as the single
build vehicle preserves the bit-identical guarantee and extends it, at the cost of one more
binary in the production image.

**The accepted cost, recorded rather than buried:** the production image now carries a tool
that deletes objects. Three things bound that — it is not the `ENTRYPOINT`, it refuses to
run without an explicit `--endpoint` and credentials, and run-id-scoped keys are a stated
safety requirement (proposal 0017 §15: "it must never delete anything it did not create").
If the image should later be slimmed, the binary table the brief introduces is the single
place that changes.

## 3. The sequencing objection, and why it does not hold

The same review:

> This sits at wave 1, depending only on #740 — whose binary "runs and prints its resolved
> config". So it would ship a stub to operators and pin a layout contract around a no-op,
> six waves before the tool works. Its own Purpose says the tool "has to be working before
> the gating run starts". Consider moving it after the tool does something.

The objection is sound about *wave 1* and is addressed by making the bundle depend on **both
#740 and #741** (a strengthening of the issue's own "Depends on #740"), which puts it at
wave 2 and means the image build compiles the crate with its real dependency set rather than
the skeleton's.

The remaining half — "it would ship a stub to operators" — is retired by a checkable fact
rather than a preference. **No `v*` tag has ever been cut**: the repo's only tag is
`archive/backup-premerge-signoff`, and `.github/workflows/release.yml` triggers on
`push: tags: ["v*"]`. And **0.1 Alpha cannot tag without the tool working** — milestone 17
records that its 7-day Hetzner endurance run "is an Alpha tag dependency", and #735's
definition of done says "0.1 Alpha does not tag without it". So the first tarball that can
ever reach an operator is one cut *after* the tool works; the release this objection
protects against cannot happen. Landing the pipeline change now instead puts the two-binary
layout under gate for every slice that follows, rather than retrofitting it beside a release
deadline.

## 4. The definition of done, as written, is unverifiable by the gate it names

> "The tarball contains both binaries; `install.sh` places both" — but
> `xtask/tests/dist_templates.rs` is "container-free by design … every template assertion is
> a file read + substring check" (`:5-9`), with the real build deferred to the release
> workflow. Nothing in `cargo xtask ci` can observe a tarball.

Confirmed, and the brief restates the criterion accordingly: the binary set becomes a pure
data table in `xtask::dist`, and the test asserts that every pipeline file — Dockerfile,
installer, README, release workflow — agrees with it. Tarball contents stay
`.github/workflows/release.yml`'s to prove.

One correction to an earlier draft of this note, found while writing the brief: **the binary
is not part of `dist::staging_plan()`**. That table stages *templates*; the binary is copied
separately in `assemble` (`xtask/src/dist.rs:559-564`) from whatever `obtain_binary`
returned. So "the staging plan carries the second entry" was wrong — a second table is
needed, which is exactly what the brief's Design §2 specifies.

## 5. What was checked

* `notes.json` (issue body + the maintainer's full review comment) — the source of truth.
* `git -C ../wyrd log --oneline -- xtask/src/dist.rs` → one commit, `f5d4575`
  ("dist(570): one pipeline ships the operator tarball and the OCI image").
* `gh pr list --state all --search "wyrd-validate"` → only PR #765, the merged proposal.
  No prior or rejected attempt at two-binary packaging exists.
* Proposal 0017 §2, which states the same pipeline finding and the same open packaging
  question, and does not answer it either.
* **Where the image actually goes**, since the decision turns on it. Three consumers:
  `cargo xtask dist --oci-archive` exports it as `wyrd-<version>-fdb.oci.tar.gz`, which the
  release workflow cosign-signs, provenance-attests and attaches to the GitHub Release (the
  filename matches its `*.tar.gz` globs — checked, not assumed); every wyrd container in
  `deploy/small-multi-node-fdb` runs `image: wyrd:fdb`; and it is the build vehicle the
  tarball's binary is extracted from. There is no registry push yet — a `ghcr` push is a
  named follow-up (`release.yml:10-12`).
* **That a single multi-`--bin` build across two packages works**, the obvious place option
  A could have broken: `cargo build --release --locked --bin wyrd --bin xtask --features
  "fdb,etcd"` exits 0 on this checkout. `xtask` is a featureless second package,
  structurally what `wyrd-validate` will be — so `Dockerfile:66` needs one line naming both
  bins, not a second `RUN` layer.
* **That the image build gains no native-toolchain requirement** from compiling the
  validator after #741: the aws SDK as pinned resolves no TLS stack
  (`cargo tree -i hyper-rustls -e normal,dev` finds nothing; the `ring` / `rustls` entries in
  `Cargo.lock` arrive only via `tikv-client` under the off-by-default `tikv` feature).
