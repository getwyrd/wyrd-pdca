# Adversarial review — issue 468 / metadata-fdb-dst-story

Reproduced the evidence first. `cargo test -p wyrd-dst --test no_fdb_linkage` → 5 passed.
`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=5 cargo test -p wyrd-dst --test commit_ambiguity`
→ 3 passed, and `assuming_an_ambiguous_commit_did_not_land_fails_the_sweep` genuinely panics
with the pinned message at `crates/dst/tests/commit_ambiguity.rs:171`. The behavioural
red→green is real. What follows attacks it anyway.

## Refutations

- **NEEDS-HUMAN — `crates/dst/tests/no_fdb_linkage.rs:58,77`: the purity guard is blind to the
  renamed-dependency form, which is this manifest's own house style.** `scan_line` takes the key
  as the text before the first `=`/`.`/space. Adding
  `fdb = { package = "foundationdb", version = "0.10", features = ["fdb-7_3"] }` to
  `crates/dst/[dev-dependencies]` links `libfdb_c` into every DST test binary and
  `the_dst_manifest_declares_no_fdb_dependency` (`:165`) stays **green**. I compiled `scan_line`
  verbatim and ran it: that line, `wyrd-fdb = { package = "wyrd-metadata-fdb" }`, and
  `[dependencies."foundationdb"]` all return `None`. This is not an exotic evasion —
  `crates/dst/Cargo.toml:56,66,68` already declares `tonic`, `etcd-client` and `tokio` in exactly
  that `package = ` rename form. The doc claim at `no_fdb_linkage.rs:10` ("there is exactly one
  way `libfdb_c` could enter this graph") and brief goal (c) ("mechanically guaranteeing that no
  `libfdb_c` symbol is ever reachable") are therefore unwarranted as stated.

- **`crates/dst/tests/no_fdb_linkage.rs:148`: the planted red is over-fitted to the scanner.**
  The fixture plants exactly the two shapes `scan_line` was written to recognise
  (`wyrd-metadata-fdb.workspace = true`, `foundationdb = { version = "0.9", … }`). It plants
  neither the rename form above nor a transitive edge, so it proves the scanner catches what the
  scanner catches. (Also: the fixture pins `version = "0.9"`/`fdb-7_1`; the workspace actually
  pins `foundationdb 0.10`/`fdb-7_3` at `Cargo.toml:108` — the fixture was not derived from the
  real dependency.) Separately the needle set is *over*-broad in the other direction:
  `crates/metadata-fdb/Cargo.toml:11-22` makes `foundationdb` an optional dep behind a
  default-off `fdb` feature, so a bare `wyrd-metadata-fdb` dev-dep would trip the guard without
  linking `libfdb_c` at all. The guard scans one manifest's text; the linkage condition is a
  feature-unified graph property.

- **NEEDS-HUMAN — `crates/dst/tests/support/mod.rs:494` (`&& conditional`) contradicts the
  production contract it cites.** `classify_commit_error`
  (`crates/metadata-fdb/src/lib.rs:213-215`, doc at `:194`) returns `UnknownResult` for 1021/1031
  **"for *every* batch, conditional or not."** The model gates the nemesis on `conditional`, so a
  blind batch can never be ambiguous in the simulator. The justification at `support/mod.rs:435-438`
  cites `metadata-fdb/src/lib.rs:53-56`, but that passage says 1021 is never *re-applied* by the
  blind retry gate — it does not say a blind batch is never ambiguous. Concrete consequence: make
  the model faithful (drop `&& conditional`) and `commit_ambiguity.rs:116`
  (`write::intent(…).await.unwrap()` → blind `put_pending`) panics on the first writer. The sweep
  is green *because* the model is narrower than the contract, and the four-phase write protocol's
  behaviour under an ambiguous pending-ledger put/delete is exercised by nothing.

- **NEEDS-HUMAN — `crates/metadata-fdb/src/lib.rs:165` vs `crates/dst/tests/support/mod.rs:398`:
  the model renders only the *strong* half of the ambiguity class.** Production maps 1021 **and**
  1031 to the same `CommitUnknownResult`, carrying `code` because "the two codes are not equally
  bad": *"Where 1021 promises the transaction is out of flight, 1031 promises nothing."*
  `SimCommitUnknownResult` is a unit struct with no code, and the whole settling re-read at
  `commit_ambiguity.rs:131` assumes the ambiguous txn is out of flight — precisely the guarantee
  1031 withholds. So "the 1021 ambiguity space, searched exhaustively" (brief §Falsifiability) is
  really "the out-of-flight half of it." The brief's Open Question 1 asks the human to ratify
  fidelity w.r.t. MVCC-vs-`BTreeMap`; it does **not** surface this narrowing or the blind-batch one
  above. Both belong in that ratification.

- **`crates/dst/tests/commit_ambiguity.rs:150` + `:184`: the torn-inode assertion is a tautology on
  exactly the path this file exists to test.** When the ambiguous commit *landed*, the nemesis
  struck the first accepted CAS, so every other writer conflicts and the sole winner is the `Err`
  writer — selected at `:150` by `settled.chunk_map == *chunk_map`. `:184` then asserts
  `settled.chunk_map == expected` where `expected` is that same `chunk_map`: `x == x`, unfalsifiable.
  It carries content only on the *not*-landed half (an `Ok(Committed)` winner), which
  `concurrency.rs:126` already covers for redb/sim-TiKV. And the model cannot produce a torn inode
  anyway — `apply()` runs inside the `Mutex` guard with no await (`support/mod.rs:470-508`). Brief
  criterion 2(iii) ("no torn/hybrid inode is ever observable") therefore has no demonstrated red,
  unlike 2(i)/2(ii). (`assert!(winners <= 1)` at `:163` is likewise unreachable-by-construction.)

- **NEEDS-HUMAN — `check-gates.json` row `C4-verify` ("run-verify.sh: PASS — red without the fix,
  green with it") is true but carries no information about the fix.** The discriminator is
  `no_fdb_linkage.rs:191`, a `String::contains("pub struct SimFdbMetadataStore")` over
  `support/mod.rs` — and `:185-186` states that this binary deliberately does *not* link the module.
  `run-verify.sh` runs `cargo test -p wyrd-dst` without `--cfg madsim`, so `support/mod.rs` is never
  compiled in either phase. The row would read identically if `SimFdbMetadataStore` were an empty
  `pub struct SimFdbMetadataStore;` with no `MetadataStore` impl, no nemesis, and no RNG. The brief
  is candid that this is "a *structural* red"; the gates row is not, and a reviewer scanning the
  table can reasonably misread it as per-fix evidence for criteria 1–3. The only real evidence for
  those is `C4-ci`.

## Attempted and could not refute

- **The both-halves sweep is not fragile.** I feared `AMBIGUITY_SWEEP_SEEDS = 64`
  (`commit_ambiguity.rs:55`) might straddle a lopsided parity. Replayed
  `ChaCha8Rng::seed_from_u64(seed).next_u64() % 2` for seeds `0..64`: **28 landed / 36 not**, first
  landed at seed 0. Both halves are amply covered and `assuming_…` panics on the first iteration.
- **The demonstrated red is not a tautology.** `#[should_panic(expected = "must equal the inode's
  version bump")]` pins the *load-bearing* assertion (`:171`), not any panic; the `AssumeNotCommitted`
  observer differs from the correct one on exactly the `Err` arm, and it passes on the not-landed
  half — so the red is caused by the store's version bump, as claimed.
- **The nemesis cannot be stolen by the fixture or by `intent`.** `metadata::put_pending` /
  `sweep_pending` build precondition-free batches (`crates/core/src/metadata.rs:499,508`) and
  `arm_commit_ambiguity` runs after the fixture (`commit_ambiguity.rs:106`), so exactly the
  four-writer `commit_overwrite` CAS is struck — `obs.ambiguous_commits == 1` holds by
  construction, not by luck.
- **`assert!(conditional)` in the Conflict arm (`support/mod.rs:481`) is unreachable, but not
  wrong**: `preconditions_hold` is vacuously true for an empty list, which is the FDB behaviour it
  claims to model.
- **No `Undeterminable` variant was added, `crates/metadata-conformance/` is untouched, no new Cargo
  dependency was introduced, and nothing under `crates/*/src/` changed** — the scope discipline the
  brief demanded holds.
