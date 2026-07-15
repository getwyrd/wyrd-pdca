# check-advisory-adversary.md — issue 430 / fragment-identity-validation

Adversarial pass. I re-ran the red→green proof myself and ran one mutation experiment
against the patched tree (scratch copy; target untouched).

## Findings

- NEEDS-HUMAN [impl] — The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating
  `crates/core/src/repair.rs:75-79` to accept any `k`/`m` (keeping only the
  `ec_scheme_type == ReedSolomon` check) survives the ENTIRE `wyrd-core` + `wyrd-custodian`
  + `wyrd-dst` suites, including the new `crates/core/tests/fragment_identity.rs` —
  verified by running the mutant (all green). Cause: the test's "EC tuple disagrees" case
  (`crates/core/tests/fragment_identity.rs:621`) uses a `None`-type header, so only the
  `ec_scheme_type` mismatch ever goes red; the unit test's tuple case
  (`crates/core/src/repair.rs:195`) is the same shape. Concrete failing case the suite
  cannot catch: an adversarial store serves a same-chunk, index-0 shard whose header
  says RS(3,1) against a committed RS(2,1) — today rejected only by unproven code; any
  future regression of the `k`/`m` compare is invisible. The brief explicitly demanded
  the tuple case cover "`ec_k`/`ec_m`/scheme type" (brief.md:41-42). Fix by iteration:
  add a same-type wrong-geometry case (header RS(3,1) vs committed RS(2,1)) to
  `fragment_identity.rs` and/or the `repair.rs` unit test. The `None`-arm
  `ec_k == 1 && ec_m == 0` conjuncts (`repair.rs:70`) are untested for the same reason.

## Refutation attempts that failed

- Attempted to refute the red→green evidence: could not. Reproduced independently —
  on a scratch copy with all production + peer-test files reverted to `dc503cd` (base)
  and only the new test kept, both tests in `crates/core/tests/fragment_identity.rs`
  fail **by assertion** (the read returns silently WRONG bytes: index-1's shard fed at
  both data positions), and pass on the patched tree. The red is on the production path
  (`read::read_object` over the `ChunkStore`/`MetadataStore` trait seams,
  `crates/core/src/read.rs:314-372`), not a parallel re-implementation, and the
  deterministic-red shape (serve exactly k fragments, one wrong-identity) removes the
  order-dependence the brief warned about. The enqueue assertions
  (`fragment_identity.rs:601-606`, `:684-689`) were red pre-fix too (queue empty), so
  they are not tautological.
- Attempted to find a remaining chunk_id-only admission site: could not. Every
  production decode-and-admit path now routes through `repair::header_matches_identity`
  — read single-fragment `crates/core/src/read.rs:237`, RS fan-out `read.rs:331`,
  reconstruction `crates/custodian/src/reconstruction.rs:391`, scrub
  `crates/custodian/src/scrub.rs:126`, rebalance `crates/custodian/src/rebalance.rs:266`.
  No other production `wyrd_chunk_format::decode` caller admits fragments.
- Attempted to break the fix with legacy/writer-conformance inputs: could not. The
  `None` arm's required stamp (`EcSchemeType::None`, k=1, m=0, index 0) is exactly what
  `FragmentHeader::new_v1` writes (`crates/chunk-format/src/header.rs:130-143`,
  `crates/core/src/write.rs:133`), and the RS arm matches `encode_chunk` /
  `encode_ec_fragment` (`write.rs:116-123`, `:142-147`). `git log -S` shows no core
  writer ever stamped `EcSchemeType::Replication`, so no previously-written on-disk
  fragment becomes unreadable under the tightened check.
- Attempted to make scrub silently skip a referenced fragment via the new
  `referenced.schemes` lookup (`crates/custodian/src/scrub.rs:99-101`): could not —
  `schemes.insert` is symmetric with `placed.insert` in the same `Ok` arm of
  `referenced_fragments` (`crates/custodian/src/gc.rs:232-247`), so every placed
  fragment has a scheme entry. (Two committed inodes sharing one chunk id with different
  schemes would make the last-scanned scheme win, but chunk ids are minted per write —
  no such aliasing path exists in this codebase.)
- Attempted to panic rebalance via `plan.prior.chunk_map[plan.chunk_index]`
  (`crates/custodian/src/rebalance.rs:266`): could not — `chunk_index` comes from
  enumerating the same `prior.chunk_map` when the plan is built (`rebalance.rs:194-202`),
  and the CAS commit (`rebalance.rs:288`) fences a concurrently-changed record.

## Note on the gate record

- The C4-verify oracle `./engine/scripts/run-verify.sh` (check-gates.json, rule
  `C4-verify`) does not exist in the target checkout, so its "PASS — red without the
  fix, green with it" claim is not auditable from the artifacts — the same gap iteration
  1 flagged. Not scored as a refutation: I reproduced the red→green substance
  independently (above), which resolves the carry-forward's C2/C4 question on the merits.
