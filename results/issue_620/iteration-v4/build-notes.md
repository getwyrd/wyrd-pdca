# Build notes — issue 620 / s3-object-key-length-limit (iteration 4)

Target: `getwyrd/wyrd` @ `main` (`7bf7eb9`; `git ls-remote origin refs/heads/main` on
2026-10-15 still answers `7bf7eb9`). Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l0`. Line numbers
are on the patched tree unless marked "base".

## Summary

`patch.diff` is **byte-identical** to `iteration-v3/patch.diff` (`cmp` of a fresh `git diff`
against it: no difference). Round 3's full Check found no defect in it: T4 batch review
`0 blocking` (`iteration-v3/check-gates.md`), code review "No actionable findings", the
adversary "could not break it", C4-verify PASS, C5 `11 caught, 1 unviable`, C4-diff-cov 94.4%.
The one red gating row was C4-ci, and it is red on the unpatched base too.

What changed this round is how that red row is handed over, not the code.

Round 3 already declared the advisory failure as an external dependency, but the harness's
adjudicator could not read the declaration: `iteration-v3/dependency-adjudication.json` says
`"verdict": "unconfirmed"`, "no registered [[doctor.checks]] row and no parseable proposed row
names it". The dependency name in the marker line was a long description ("RustSec advisory DB
(RUSTSEC-2026-0346, …)"), and the proposed row's `id` was a different string
(`base-offbydefault-advisories`). The adjudicator matches the two exactly, case-insensitively
(`src/pdca_harness/dependency_halt.py:117-136`). With no match, full Check ran again and
auto-iterate sent the same patch back for a third time.

This round the marker's dependency name and the row `id` are the same string, and the detect
command is one I ran both ways (it fails on the base and passes with the waiver; see below). If
the harness confirms it, the bundle goes to the human at AWAITING_SIGNOFF instead of spending
auto-iterate rounds 4 and 5 (opus, then fable) on a patch no rebuild can turn green. That is
the human decision the round-3 reviewer asked for: "Decide disposition of the unchanged TiKV
dependency-policy failure under #543".

## The production change (unchanged since iteration 2)

- `MAX_KEY_BYTES = 1024` (`crates/gateway-s3/src/lib.rs:497`), with a doc comment naming the
  AWS source and where the limit is and is not enforced (`:489-496`).
- The `MAX_DELETE_BODY_BYTES` comment now points at `MAX_KEY_BYTES` and says the parser refuses
  a longer key (`lib.rs:505-506`), so its "1000 × 1024-byte key" sizing is true.
- PUT arm: the decoded bucket and key are bound once (`lib.rs:1776-1778`, base `:1765-1769`).
  After the CopyObject guard (`lib.rs:1809-1816`, base `:1800-1806`, the peer the brief names),
  two refusals run before the body stream is touched, both through the shared `error_response`:
  - decoded bucket contains `/` → `400 InvalidBucketName` (`lib.rs:1828-1835`, criterion 5);
  - `key_name.len() > MAX_KEY_BYTES` → `400 KeyTooLongError` (`lib.rs:1838-1840`, criteria 1-2).
    `str::len` is the UTF-8 byte length of the percent-decoded key alone.
- Bulk delete: `DeleteRequestError::KeyTooLong` (`lib.rs:1983`); `key_too_long()` built on
  `error_response` (`lib.rs:2023-2030`); mapped in `delete_objects` before any `delete_object`
  call (`lib.rs:2114`); the parser bound sits right after the empty-key check (`lib.rs:2267-2272`,
  base `:2209-2211`), measured on the key as `char_data` yields it (entity-decoded once, never
  percent-decoded).
- GET, HEAD and single-object DELETE are untouched (base `:1878-1891`), so criterion 4 holds.
- Docs currency: `docs/design/architecture/05-building-block-view.md:137` describes the limit,
  the PUT bucket-separator refusal, the whole-request bulk refusal, and that single-object
  GET/HEAD/DELETE still reach over-limit objects.

The test is `crates/server/tests/s3_key_length_limit.rs` (new file, 396 lines), five wire legs
at `:240`, `:272`, `:307`, `:355`, `:386`, matching criteria 1-5. Every socket exchange and
backend call runs under a 30 s bound (`:66`, `:88-93`), and the server task is aborted when
the harness drops (`:79-85`). In-crate tests in `lib.rs` (`:4545`, `:4685`, `:4738`) pin the
same bounds where the code lives.

## Iteration-3 carry-forward: what I did with each item

1. **"Decide disposition of the unchanged TiKV dependency-policy failure under #543."** This is
   a maintainer decision, not build work. I did not put the waiver in `patch.diff` (reasons
   below). I re-declared the dependency in a form the harness can check, with a detect command
   I ran in both states, so the decision reaches the human.
2. **"The supplementary DST sweep is incomplete."** That was the reviewer's own extra DST run,
   interrupted in the custodian seed campaign, which this patch does not touch. Round 3's full
   `cargo xtask ci` with the waiver applied did run DST clippy and `cargo test -p wyrd-dst`
   under `--cfg madsim` to completion, inside an exit-0 run
   (`builder-evidence/ci-full-with-waiver.log`; the patch is byte-identical, so it still applies).
3. **C4-ci red.** Unchanged and outside this patch: see the next section.
4. The items in `deferred-findings.json` are held for the human and marked "not build work". I
   left them alone. Two of them are still open design questions the human should see. Both
   come from brief criterion 3, which I implemented as written:
   - whole-request bulk refusal vs. a per-key `<Error>` row: one legacy over-limit key makes a
     list-then-bulk-delete tool fail its whole 1,000-key batch;
   - keys longer than the `http` crate's request-target limit (~64 KiB, or ~21,800 bytes when
     every byte needs percent-encoding) can only be written outside S3 (`wyrd put --key`). For
     those keys, single-object DELETE answers `414`, and the patch's bulk refusal closes the one
     S3 exit they had before.

## The cargo-deny failure

### What fails, and why no rebuild fixes it

`cargo xtask ci`'s second deny step (`cargo deny --all-features --config
deny-all-features.toml check advisories`) reports RUSTSEC-2026-0346 (CVSS 4.0, medium) and
RUSTSEC-2026-0349 (CVSS 3.7, low) on `rustls 0.21.12` (base `Cargo.lock:3294-3295`). The only
path to it is `tikv-client 0.4.0 → tonic 0.10 / tokio-rustls 0.24`, behind the off-by-default
`tikv` feature. Both advisories are patched only in `>= 0.23.46`, and `tikv-client 0.4.0` is
the latest release, so no lockfile bump exists. This patch touches no `Cargo.toml`,
`Cargo.lock`, or deny config, so the base fails this step in exactly the same way.

I re-ran the step on the base checkout this round: exit 1, the two IDs above, nothing else
(`cd ../wyrd && cargo deny --frozen --all-features --config deny-all-features.toml check
advisories`). No upstream PR or issue mentions RUSTSEC-2026-0346 yet
(`gh pr list -R getwyrd/wyrd --state all --search RUSTSEC-2026-0346` and the matching
`gh issue list` both return `[]`).

### Why the waiver is still not in `patch.diff`

- It is a security-policy change to `deny-all-features.toml` (#543's exposure boundary). Who
  makes it is the maintainer's call. Both reviews and the deferred findings say so ("A
  dependency bump or a recorded advisory exception has to be decided separately").
- The project's precedent puts such a waiver in its own PR with its own issue (#790, closing
  #773, for RUSTSEC-2026-0258), not inside a feature PR.
- All seven bundles in this batch (656, 666, 671, 767, 768, 620, 686) share the base and fail
  this step. One separate PR fixes all of them. Seven copies would collide in the same file.
- GitHub CI runs the same deny step, so this PR cannot merge before the base is fixed anyway.

Cost of folding it in, so it can be checked: +22/−3 lines in two files outside the brief's
Scope (`deny-all-features.toml | 19 +++++++++++++++++--`, `xtask/tests/fdb_harness.rs | 6
+++++-`). The ready diff is `base-advisory-waiver.diff` in this bundle (base `7bf7eb9`,
applies with `git apply`, and applies cleanly together with `patch.diff`; I applied both this
round). The reason to keep it out is who decides, not size.

### The declaration

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the gating C4-ci row (`cargo xtask ci`) stops at `cargo deny --all-features --config deny-all-features.toml check advisories` on the unpatched base `7bf7eb9`, on RUSTSEC-2026-0346/0349 in tikv-client's rustls 0.21.12, so no bundle on this base can show a green C4-ci; the ready fix is `base-advisory-waiver.diff` in this bundle (both advisories are medium/low, below the file's high/critical review trigger; no rustls 0.21.x fix exists); land it as its own PR as #790 did, bring the bundle's base up to that main, then iterate-do.

```toml
[[doctor.checks]]
group = "workspace"
id    = "cargo deny all-features advisories"
cmd   = 'cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories'
hint  = "the target's all-features advisory wall (the second cargo-deny step of cargo xtask ci) fails on the base, so every bundle's gating C4-ci is red until a separate deny-all-features.toml PR records or clears the new advisory (#543; precedent #790/#773). Offline and read-only (--frozen: cached advisory DB, Cargo.lock unchanged), so it reflects the DB the last C4-ci run fetched; it checks ../wyrd as checked out, or $WYRD_REPO"
level = "WARN"
```

The detect command, run from the pdca root as `doctor.probe` runs it (shell, `cwd` = project
root):

- on `../wyrd` at base `7bf7eb9`: **exit 1** in 0.6 s, failing on exactly RUSTSEC-2026-0346
  and RUSTSEC-2026-0349, `advisories FAILED`;
- with `WYRD_REPO` pointed at this worktree with `base-advisory-waiver.diff` applied:
  **exit 0**, `advisories ok`. I then reverted the waiver (`git apply -R`) and the worktree
  diff went back to byte-identical with `patch.diff`;
- `git -C ../wyrd status --short` before and after: unchanged (only the pre-existing untracked
  `target-madsim/`). `--frozen` means no network, no advisory-DB fetch, and no `Cargo.lock`
  write, which keeps the probe a read, as `pdca.toml:1020-1026` asks.

So the command tells "base advisory wall clean" from "not clean". The `id` is the same string
as the dependency name in the marker line above. That is the match round 3 lacked.

## Verification

- **Red→green, through the project's runner** (`engine/scripts/run-verify.sh` with
  `PDCA_BUNDLE` = this bundle, `PDCA_LANE=0`, `PDCA_BRIEF_BASE=origin/main`, under
  `timeout 3000`), on this round's `patch.diff`:
  - GREEN (fix applied): `5 passed; 0 failed`.
  - RED (production reverted, test kept): `1 passed; 4 failed`, each at
    `s3_key_length_limit.rs:231` (`assert_refused`) with `left: 200`.
  - Verdict: `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).`
    Only 4 legs went red; leg 4 is green on both sides by design. The "5 … ran red" wording
    overcounts. That is a harness-summary issue (the round-3 adversary saw it too), not a test
    problem. If you agree it is a bug, it belongs upstream in eduralph/pdca-harness.
- **In-crate tests**: `cargo test -p wyrd-gateway-s3 --lib` on the three key-limit tests:
  `3 passed; 0 failed`.
- **Formatter and lints** on the worktree with the patch applied: `cargo fmt --all -- --check`
  clean; `typos` clean on all three touched files; `cargo clippy -p wyrd-gateway-s3 -p
  wyrd-server --all-targets` clean (workspace lints are deny-level). The repo sets no git commit
  hooks (`core.hooksPath` unset, no `.pre-commit-config.yaml`), so these CI steps are what a
  commit has to pass.
- **Full gate**: not re-run this round, because the patch and the base are both unchanged since
  round 3. Round 3 ran a complete `cargo xtask ci` with patch + waiver applied: exit 0, every
  stage in order including the all-features advisory wall, conformance, statics, deploy-guard
  and the madsim DST suite, 1,769 tests passed, 0 failed
  (`builder-evidence/ci-full-with-waiver.log`). Without the waiver it stops at the advisory
  step, exactly as the base does.

## Forced self-refutation

- **(a) Genuine red? Yes.** `run-verify.sh` applied `patch.diff` to a clean `origin/main`
  checkout, reverted the production change and kept the test: `1 passed; 4 failed`, all on
  assertions (`left: 200`, expected `400`), not on a compile error. The failing legs are
  criterion 1 (PUT of a 1,025-byte key), 2 (PUT of 342 × `€`), 3 (DeleteObjects naming a
  1,025-byte key: it deleted `ok`) and 5 (`PUT /b%2Fa/<1,023 bytes>`). Leg 4 passes on both
  sides, as the brief's Falsifiability section says. With the fix: `5 passed; 0 failed`.
- **(b) Production path? Yes.** The wire test runs the real `S3Gateway::serve` on a loopback
  `TcpListener` in front of the real `wyrd_server::Gateway` (redb in-memory metadata,
  `FsChunkStore` on a tempdir, `MemCoordination`; `s3_key_length_limit.rs:97-132`), with real
  SigV4-signed HTTP/1.1. Nothing is mocked. The in-crate router tests replace only the storage
  seam with a recording double, and still go through the real `router()` → PUT arm /
  `delete_objects` → `parse_delete_request`; their point is to show the seam is never reached.
- **(c) Fixture includes the fault? Yes.** Leg 1 sends the 1,025-byte key, and an in-limit
  `control` object must appear in the listing, so an empty or broken listing cannot pass "not
  listed". Leg 2 sends the 1,026-byte `€` key and a fully percent-encoded 1,024-byte key. Leg 3
  sends both over-limit entries (ASCII and 342 × `€`) next to a stored `ok` and checks `ok`
  survives. Leg 4 seeds a real 1,025-byte object through `Gateway::put_object` and requires
  HEAD `200` before the DELETE, so the `204` cannot come from an absent key. Leg 5 sends the
  real `%2F` path.

## Ruled out

- **Guarding only the bulk-delete parser.** PUT could still create a key that bulk delete then
  refuses: the #612 finding and the brief's SELF-TEST.
- **Checking the limit on GET / HEAD / single DELETE.** That would strand legacy objects
  (criterion 4, principle C-1).
- **Measuring `"{bucket}/{key}"` or the wire path.** Both contradict criterion 2.
- **Folding the advisory waiver into this patch.** See "Why the waiver is still not in
  `patch.diff`".
- **Re-submitting round 3's declaration as it was.** Its name and row `id` did not match, so
  the harness could not check it. That is why this round changes the declaration and not the
  code.
- **A probe that only greps `deny-all-features.toml` for the two IDs.** It would report
  "still absent" after a different fix, such as a tikv-client bump that drops rustls 0.21. Running
  the same `cargo deny` step the gate runs answers the real question.
- **Plain `cargo deny` without `--frozen` as the probe.** It fetches the advisory DB, and it
  can rewrite `Cargo.lock` if the lockfile is stale. The doctor contract asks for a read
  (`pdca.toml:1020-1026`).

## Conflicts / ordering

Conflicts with #671 (size refusal in the same PUT arm). My refusals sit between the CopyObject
guard and the `content-type` read (`lib.rs:1817-1840`). This is wave 0.
